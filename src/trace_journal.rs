//! A separate, bounded, non-authoritative diagnostic journal. Never opens a Store.
use crate::domain::{RunId, TRACE_DOCUMENT_LIMIT, TraceDocument, TraceId};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

const APPLICATION_ID: i64 = 0x5052_4447;
const VERSION: i64 = 1;
const TABLE: &str = "CREATE TABLE records (id BLOB PRIMARY KEY CHECK(length(id)=16), run_id BLOB CHECK(run_id IS NULL OR length(run_id)=16), updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=0), document BLOB NOT NULL CHECK(length(document)<=524288)) STRICT, WITHOUT ROWID";
const INDEX: &str = "CREATE INDEX records_run ON records(run_id, updated_at_ms)";

mod retention;
pub(crate) use retention::{RetentionGuard, TraceRetentionCandidate};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Error {
    Missing,
    UnsafePath,
    ForeignJournal,
    InvalidJournal,
    InvalidRecord,
    Busy,
    AccessDenied,
    Full,
    Io,
    RetentionIncomplete,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Missing => "diagnostic journal is unavailable",
            Self::UnsafePath => "diagnostic journal path is unsafe",
            Self::ForeignJournal => "diagnostic destination is not a Pactrun journal",
            Self::InvalidJournal => "diagnostic journal schema is invalid or unsupported",
            Self::InvalidRecord => "diagnostic record is invalid",
            Self::Busy => "diagnostic journal is busy",
            Self::AccessDenied => "diagnostic journal access denied",
            Self::Full => "diagnostic storage is full",
            Self::Io => "diagnostic journal I/O failed",
            Self::RetentionIncomplete => "diagnostic retention has not completed",
        })
    }
}
impl std::error::Error for Error {}
fn io_error(e: std::io::Error) -> Error {
    match e.kind() {
        std::io::ErrorKind::NotFound => Error::Missing,
        std::io::ErrorKind::PermissionDenied => Error::AccessDenied,
        _ if matches!(e.raw_os_error(), Some(28 | 112)) => Error::Full,
        _ => Error::Io,
    }
}
fn sql_error(e: rusqlite::Error) -> Error {
    match e.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            Error::Busy
        }
        Some(rusqlite::ErrorCode::DiskFull) => Error::Full,
        Some(rusqlite::ErrorCode::PermissionDenied | rusqlite::ErrorCode::ReadOnly) => {
            Error::AccessDenied
        }
        _ => Error::Io,
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct Location {
    path: PathBuf,
    create_parent: bool,
}
impl std::fmt::Debug for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DiagnosticLocation { path withheld }")
    }
}
impl Location {
    pub(crate) fn store(root: &Path) -> Self {
        Self {
            path: root.join("database/diagnostics.sqlite3"),
            create_parent: false,
        }
    }
    pub(crate) fn startup(path: PathBuf) -> Self {
        Self {
            path,
            create_parent: true,
        }
    }
    pub(crate) fn overlaps_store(&self, root: &Path) -> bool {
        fn key(path: &Path) -> Option<PathBuf> {
            let absolute = std::path::absolute(path).ok()?;
            if absolute.components().take(257).count() > 256 {
                return None;
            }
            let mut normalized = PathBuf::new();
            for part in absolute.components() {
                match part {
                    std::path::Component::ParentDir => {
                        normalized.pop();
                    }
                    std::path::Component::CurDir => (),
                    _ => normalized.push(part.as_os_str()),
                }
            }
            let mut anchor = normalized.clone();
            let mut missing = Vec::new();
            let existing = loop {
                match fs::canonicalize(&anchor) {
                    Ok(existing) => break existing,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        missing.push(anchor.file_name()?.to_os_string());
                        if !anchor.pop() {
                            return None;
                        }
                    }
                    Err(_) => return None,
                }
            };
            let mut absolute = existing;
            for part in missing.into_iter().rev() {
                absolute.push(part);
            }
            #[cfg(windows)]
            let absolute = PathBuf::from(absolute.to_string_lossy().to_uppercase());
            Some(absolute)
        }
        match (self.path.parent().and_then(key), key(root)) {
            (Some(journal), Some(store)) => {
                journal.starts_with(&store) || store.starts_with(&journal)
            }
            _ => true,
        }
    }
}

pub(crate) struct Journal {
    connection: Connection,
    _guard: File,
}
impl Journal {
    pub(crate) fn open(location: &Location) -> Result<Self, Error> {
        let parent = location.path.parent().ok_or(Error::UnsafePath)?;
        directory(parent, location.create_parent)?;
        let (mut guard, created) = match file(&location.path, true) {
            Ok(file) => (file, true),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                (file(&location.path, false).map_err(io_error)?, false)
            }
            Err(e) => return Err(io_error(e)),
        };
        qualify_file(&guard)?;
        if !created {
            header(&mut guard)?;
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        let mut connection =
            Connection::open_with_flags(&location.path, flags).map_err(sql_error)?;
        connection
            .busy_timeout(Duration::from_millis(20))
            .map_err(sql_error)?;
        if created {
            connection
                .pragma_update(None, "journal_mode", "DELETE")
                .map_err(sql_error)?;
            connection
                .pragma_update(None, "synchronous", "NORMAL")
                .map_err(sql_error)?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?;
            tx.execute_batch(&format!("{TABLE}; {INDEX};"))
                .map_err(sql_error)?;
            tx.pragma_update(None, "application_id", APPLICATION_ID)
                .map_err(sql_error)?;
            tx.pragma_update(None, "user_version", VERSION)
                .map_err(sql_error)?;
            tx.commit().map_err(sql_error)?;
        }
        validate_schema(&connection)?;
        connection
            .pragma_update(None, "synchronous", "NORMAL")
            .map_err(sql_error)?;
        Ok(Self {
            connection,
            _guard: guard,
        })
    }

    pub(crate) fn save(&mut self, document: &TraceDocument, now_ms: u64) -> Result<(), Error> {
        self.save_policy(document, now_ms, true)
    }
    pub(crate) fn save_pending_retention(
        &mut self,
        document: &TraceDocument,
        now_ms: u64,
    ) -> Result<(), Error> {
        self.save_policy(document, now_ms, false)
    }
    fn save_policy(
        &mut self,
        document: &TraceDocument,
        now_ms: u64,
        trim: bool,
    ) -> Result<(), Error> {
        if !document.valid() {
            return Err(Error::InvalidRecord);
        }
        let bytes = serde_json::to_vec(document).map_err(|_| Error::InvalidRecord)?;
        if bytes.len() > TRACE_DOCUMENT_LIMIT {
            return Err(Error::InvalidRecord);
        }
        let now = i64::try_from(now_ms).map_err(|_| Error::InvalidRecord)?;
        let cutoff = now_ms.saturating_sub(u64::from(document.policy.retention_days) * 86_400_000);
        let run = document.run_id.as_ref().map(|id| id.as_bytes().as_slice());
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        if trim {
            tx.execute(
                "DELETE FROM records WHERE updated_at_ms < ?1",
                [cutoff as i64],
            )
            .map_err(sql_error)?;
        }
        tx.execute("INSERT INTO records(id,run_id,updated_at_ms,document) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET run_id=excluded.run_id,updated_at_ms=excluded.updated_at_ms,document=excluded.document",
            params![document.id.as_bytes().as_slice(), run, now, bytes]).map_err(sql_error)?;
        if !trim {
            return tx.commit().map_err(sql_error);
        }
        let mut retained: i64 = tx
            .query_row(
                "SELECT COALESCE(SUM(length(document)),0) FROM records",
                [],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        let budget = document.policy.max_bytes as i64;
        while retained > budget {
            let victims = {
                let mut query = tx.prepare("SELECT CASE WHEN length(id)=16 THEN id ELSE X'' END,length(document) FROM records WHERE id<>?1 ORDER BY updated_at_ms,CASE WHEN length(id)=16 THEN id END LIMIT 64").map_err(sql_error)?;
                let rows = query
                    .query_map([document.id.as_bytes().as_slice()], |r| {
                        Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?))
                    })
                    .map_err(sql_error)?;
                let mut ids = Vec::new();
                for row in rows {
                    let (id, size) = row.map_err(sql_error)?;
                    if id.len() != 16 || size < 0 {
                        return Err(Error::InvalidRecord);
                    }
                    ids.push(id);
                    retained -= size;
                    if retained <= budget {
                        break;
                    }
                }
                ids
            };
            if victims.is_empty() {
                return Err(Error::InvalidRecord);
            }
            for id in victims {
                tx.execute("DELETE FROM records WHERE id=?1", [id])
                    .map_err(sql_error)?;
            }
        }
        tx.commit().map_err(sql_error)
    }
}

pub(crate) fn read(location: &Location, id: TraceId) -> Result<Option<TraceDocument>, Error> {
    let Some((connection, _guard)) = reader(location)? else {
        return Ok(None);
    };
    let row = connection
        .query_row(
            "SELECT CASE typeof(document) WHEN 'blob' THEN length(document) ELSE -1 END, CASE typeof(document) WHEN 'blob' THEN CASE WHEN length(document)<=524288 THEN document ELSE X'' END ELSE X'' END FROM records WHERE id=?1",
            [id.as_bytes().as_slice()],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    row.map(|(size, bytes)| {
        let document = decode(size, &bytes)?;
        if document.id != id {
            return Err(Error::InvalidRecord);
        }
        Ok(document)
    })
    .transpose()
}
pub(crate) fn read_run(location: &Location, run: RunId) -> Result<Option<TraceDocument>, Error> {
    let Some((connection, _guard)) = reader(location)? else {
        return Ok(None);
    };
    let row = connection.query_row("SELECT CASE WHEN length(id)=16 THEN id ELSE X'' END, CASE typeof(document) WHEN 'blob' THEN length(document) ELSE -1 END, CASE typeof(document) WHEN 'blob' THEN CASE WHEN length(document)<=524288 THEN document ELSE X'' END ELSE X'' END FROM records WHERE run_id=?1 ORDER BY updated_at_ms DESC LIMIT 1", [run.as_bytes().as_slice()], |r| Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,i64>(1)?,r.get::<_,Vec<u8>>(2)?))).optional().map_err(sql_error)?;
    row.map(|(id, size, bytes)| {
        let document = decode(size, &bytes)?;
        if document.id.as_bytes().as_slice() != id || document.run_id != Some(run) {
            return Err(Error::InvalidRecord);
        }
        Ok(document)
    })
    .transpose()
}

#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) struct Summary {
    pub(crate) id: TraceId,
    pub(crate) run_id: Option<String>,
    pub(crate) operation: crate::domain::TraceOperation,
    pub(crate) updated_at_unix_ms: String,
    pub(crate) closed: bool,
    pub(crate) exit_code: Option<i32>,
    pub(crate) observed: String,
    pub(crate) omitted: String,
}
#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) struct Listing {
    pub(crate) items: Vec<Summary>,
    pub(crate) has_more: bool,
}
pub(crate) fn list(location: &Location, limit: usize) -> Result<Listing, Error> {
    if !(1..=100).contains(&limit) {
        return Err(Error::InvalidRecord);
    }
    let Some((connection, _guard)) = reader(location)? else {
        return Ok(Listing {
            items: vec![],
            has_more: false,
        });
    };
    let mut query = connection.prepare("SELECT CASE WHEN length(id)=16 THEN id ELSE X'' END,updated_at_ms,CASE typeof(document) WHEN 'blob' THEN length(document) ELSE -1 END,CASE typeof(document) WHEN 'blob' THEN CASE WHEN length(document)<=524288 THEN document ELSE X'' END ELSE X'' END FROM records ORDER BY updated_at_ms DESC LIMIT ?1").map_err(sql_error)?;
    let rows = query
        .query_map([limit as i64 + 1], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Vec<u8>>(3)?,
            ))
        })
        .map_err(sql_error)?;
    let mut items = Vec::new();
    for row in rows {
        let (id, at, size, bytes) = row.map_err(sql_error)?;
        let doc = decode(size, &bytes)?;
        if id != doc.id.as_bytes().as_slice() || at < 0 {
            return Err(Error::InvalidRecord);
        }
        items.push(Summary {
            id: doc.id,
            run_id: doc.run_id.map(|id| id.to_string()),
            operation: doc.operation,
            updated_at_unix_ms: at.to_string(),
            closed: doc.closed,
            exit_code: doc.exit_code,
            observed: doc.observed.to_string(),
            omitted: doc.omitted.to_string(),
        });
    }
    let has_more = items.len() > limit;
    items.truncate(limit);
    Ok(Listing { items, has_more })
}

fn decode(size: i64, bytes: &[u8]) -> Result<TraceDocument, Error> {
    if size < 0 || size as u64 > TRACE_DOCUMENT_LIMIT as u64 || size as usize != bytes.len() {
        return Err(Error::InvalidRecord);
    }
    crate::strict_json::parse_json(bytes, TRACE_DOCUMENT_LIMIT)
        .map_err(|_| Error::InvalidRecord)?;
    let record: TraceDocument = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRecord)?;
    if !record.valid() {
        return Err(Error::InvalidRecord);
    }
    Ok(record)
}
fn reader(location: &Location) -> Result<Option<(Connection, File)>, Error> {
    match directory(location.path.parent().ok_or(Error::UnsafePath)?, false) {
        Ok(()) => (),
        Err(Error::Missing) => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut guard = match file(&location.path, false) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(e)),
    };
    qualify_file(&guard)?;
    header(&mut guard)?;
    let connection = Connection::open_with_flags(
        &location.path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(sql_error)?;
    connection
        .busy_timeout(Duration::from_millis(20))
        .map_err(sql_error)?;
    validate_schema(&connection)?;
    Ok(Some((connection, guard)))
}
fn header(file: &mut File) -> Result<(), Error> {
    let mut header = [0; 100];
    file.read_exact(&mut header)
        .map_err(|_| Error::ForeignJournal)?;
    if &header[..16] != b"SQLite format 3\0"
        || header[18] != 1
        || header[19] != 1
        || i64::from(u32::from_be_bytes(header[68..72].try_into().unwrap())) != APPLICATION_ID
        || i64::from(u32::from_be_bytes(header[60..64].try_into().unwrap())) != VERSION
    {
        return Err(Error::ForeignJournal);
    }
    Ok(())
}
fn validate_schema(connection: &Connection) -> Result<(), Error> {
    let app: i64 = connection
        .pragma_query_value(None, "application_id", |r| r.get(0))
        .map_err(sql_error)?;
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(sql_error)?;
    if app != APPLICATION_ID || version != VERSION {
        return Err(Error::ForeignJournal);
    }
    let mut query = connection
        .prepare("SELECT name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY name")
        .map_err(sql_error)?;
    let rows: Vec<(String, String)> = query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(sql_error)?
        .collect::<Result<_, _>>()
        .map_err(sql_error)?;
    if rows
        != vec![
            ("records".into(), TABLE.into()),
            ("records_run".into(), INDEX.into()),
        ]
    {
        return Err(Error::InvalidJournal);
    }
    Ok(())
}
fn directory(path: &Path, create: bool) -> Result<(), Error> {
    let path = std::path::absolute(path).map_err(io_error)?;
    let ancestors: Vec<_> = path.ancestors().take(257).collect();
    if ancestors.len() > 256 {
        return Err(Error::UnsafePath);
    }
    for path in ancestors.into_iter().rev() {
        match fs::symlink_metadata(path) {
            Ok(meta) => {
                if !meta.is_dir() || reparse(&meta) {
                    return Err(Error::UnsafePath);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
                let mut builder = fs::DirBuilder::new();
                builder.recursive(false);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                match builder.create(path) {
                    Ok(()) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        let meta = fs::symlink_metadata(path).map_err(io_error)?;
                        if !meta.is_dir() || reparse(&meta) {
                            return Err(Error::UnsafePath);
                        }
                    }
                    Err(e) => return Err(io_error(e)),
                }
            }
            Err(e) => return Err(io_error(e)),
        }
    }
    Ok(())
}
fn reparse(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}
fn qualify_file(file: &File) -> Result<(), Error> {
    let meta = file.metadata().map_err(io_error)?;
    if !meta.is_file() || reparse(&meta) {
        return Err(Error::UnsafePath);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 {
            return Err(Error::UnsafePath);
        }
    }
    #[cfg(windows)]
    {
        if pactrun_windows_ntfs::file_link_count(file).map_err(io_error)? != 1 {
            return Err(Error::UnsafePath);
        }
    }
    Ok(())
}
fn file(path: &Path, create: bool) -> std::io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.read(true).write(create).create_new(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .custom_flags(0x0020_0000 | 0x0200_0000)
            .share_mode(3);
    }
    options.open(path)
}

#[cfg(test)]
mod tests;
