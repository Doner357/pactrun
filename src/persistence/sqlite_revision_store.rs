//! Crate-private SQLite persistence for exact Revision content.
//!
//! This module owns database reference publication only. Runtime blob bytes are
//! published durably by M1-B before a new relational reference is committed.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use rusqlite::{Connection, ErrorCode, OptionalExtension, TransactionBehavior, params};

use super::runtime_content_store::{
    RuntimeContentStore, RuntimeContentStoreError, StoredRuntimeBlob,
    validate_existing_regular_entry, validate_supported_storage_root,
};
use super::sqlite_revision_metadata::apply_revision_metadata_in_transaction;
use crate::{
    domain::{
        PackageId, RevisionIdentity, RevisionMetadataMutationBatch, RunId, Sha256Digest,
        ValidatedRevisionContentV1,
    },
    revision_core_v1::{
        calculate_revision_content_digest_v1, decode_canonical_revision_content_v1,
        encode_canonical_revision_core_v1, encode_canonical_runtime_content_v1,
    },
};

const DATABASE_DIRECTORY: &str = "database";
const RUNTIME_CONTENT_DIRECTORY: &str = "runtime-content";
const DATABASE_NAME: &str = "pactrun.sqlite3";
pub(super) const APPLICATION_ID: i64 = 0x5041_4354;
const SCHEMA_V1_VERSION: i64 = 1;
const SCHEMA_V2_VERSION: i64 = 2;
pub(super) const SCHEMA_V3_VERSION: i64 = 3;
pub(super) const SCHEMA_VERSION: i64 = 4;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) const SCHEMA_V1_SQL: &str = r#"
CREATE TABLE packages (
    package_id BLOB NOT NULL
        CHECK(length(package_id) = 16),

    PRIMARY KEY (package_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE revisions (
    package_id BLOB NOT NULL
        CHECK(length(package_id) = 16),

    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),

    core_jcs BLOB NOT NULL,
    runtime_content_jcs BLOB NOT NULL,

    PRIMARY KEY (
        package_id,
        revision_content_digest
    ),

    FOREIGN KEY (package_id)
        REFERENCES packages(package_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE revision_runtime_content_refs (
    package_id BLOB NOT NULL
        CHECK(length(package_id) = 16),

    revision_content_digest BLOB NOT NULL
        CHECK(length(revision_content_digest) = 32),

    content_id TEXT NOT NULL COLLATE BINARY,

    blob_digest BLOB NOT NULL
        CHECK(length(blob_digest) = 32),

    PRIMARY KEY (
        package_id,
        revision_content_digest,
        content_id
    ),

    FOREIGN KEY (
        package_id,
        revision_content_digest
    )
    REFERENCES revisions(
        package_id,
        revision_content_digest
    )
) STRICT, WITHOUT ROWID;
"#;
pub(super) const SCHEMA_V2_ADDITIONS_SQL: &str =
    include_str!("persistence_schema_v2_additions.sql");
pub(super) const SCHEMA_V3_ADDITIONS_SQL: &str =
    include_str!("persistence_schema_v3_additions.sql");
pub(super) const SCHEMA_V4_ADDITIONS_SQL: &str =
    include_str!("persistence_schema_v4_additions.sql");

/// The ordered schema ladder: every version applies the SQL of all lower
/// versions first. Index `n` holds the additions that produce version `n + 1`.
const SCHEMA_LADDER: [(&str, &str); 4] = [
    (SCHEMA_V1_SQL, "PersistenceSchemaV1"),
    (SCHEMA_V2_ADDITIONS_SQL, "PersistenceSchemaV2 additions"),
    (SCHEMA_V3_ADDITIONS_SQL, "PersistenceSchemaV3 additions"),
    (SCHEMA_V4_ADDITIONS_SQL, "PersistenceSchemaV4 additions"),
];

#[derive(Debug)]
pub(crate) struct PactrunPersistence {
    pub(super) database: Mutex<Connection>,
    pub(super) database_path: PathBuf,
    pub(super) runtime_content: RuntimeContentStore,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StoredRevisionContentV1 {
    pub(crate) identity: RevisionIdentity,
    pub(crate) content: ValidatedRevisionContentV1,
}

#[derive(Debug)]
pub(crate) enum PersistenceError {
    RuntimeContent(RuntimeContentStoreError),
    Sqlite {
        operation: &'static str,
        source: rusqlite::Error,
    },
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
    DatabaseOwnership(String),
    SchemaMismatch(String),
    PublicationWitness(String),
    CorruptRevision(String),
    InvalidMetadata(String),
    MetadataConflict(String),
    CorruptMetadata(String),
    CorruptInstance(String),
    MissingInstance(String),
    StaleInstanceState,
    CompilationObservationChanged,
    InvalidManagedInput(String),
    UnauthorizedSecretExport,
    CorruptManagedInput(String),
    MissingRevision(RevisionIdentity),
    MissingRun(RunId),
    CorruptRun(String),
    InvalidRunTransition(String),
    RunNotRunning,
    RecoveryGuardActive,
    MissingRecoveryGuard,
    InvalidRunArtifact(String),
    DatabaseLockPoisoned,
}

impl PersistenceError {
    pub(super) fn sqlite(operation: &'static str, source: rusqlite::Error) -> Self {
        Self::Sqlite { operation, source }
    }
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeContent(source) => write!(formatter, "runtime content: {source}"),
            Self::Sqlite { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::Io { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::DatabaseOwnership(message) => {
                write!(formatter, "database ownership: {message}")
            }
            Self::SchemaMismatch(message) => write!(formatter, "schema mismatch: {message}"),
            Self::PublicationWitness(message) => {
                write!(formatter, "publication witness: {message}")
            }
            Self::CorruptRevision(message) => write!(formatter, "corrupt Revision: {message}"),
            Self::InvalidMetadata(message) => write!(formatter, "invalid metadata: {message}"),
            Self::MetadataConflict(message) => write!(formatter, "metadata conflict: {message}"),
            Self::CorruptMetadata(message) => write!(formatter, "corrupt metadata: {message}"),
            Self::CorruptInstance(message) => write!(formatter, "corrupt Instance: {message}"),
            Self::MissingInstance(name) => write!(formatter, "Instance {name:?} is not persisted"),
            Self::StaleInstanceState => formatter.write_str("stale InstanceStateVersion"),
            Self::CompilationObservationChanged => {
                formatter.write_str("Instance compilation observation changed")
            }
            Self::InvalidManagedInput(message) => {
                write!(formatter, "invalid Managed Input: {message}")
            }
            Self::UnauthorizedSecretExport => {
                formatter.write_str("Secret export is not authorized")
            }
            Self::CorruptManagedInput(message) => {
                write!(formatter, "corrupt Managed Input: {message}")
            }
            Self::MissingRevision(identity) => write!(
                formatter,
                "Revision {} / {} is not persisted",
                identity.package_id, identity.content_digest
            ),
            Self::MissingRun(run) => write!(formatter, "Run {run} is not persisted"),
            Self::CorruptRun(message) => write!(formatter, "corrupt Run: {message}"),
            Self::InvalidRunTransition(message) => {
                write!(formatter, "invalid Run transition: {message}")
            }
            Self::RunNotRunning => formatter.write_str("Run is not Running"),
            Self::RecoveryGuardActive => {
                formatter.write_str("Instance is in ManualRecoveryRequired")
            }
            Self::MissingRecoveryGuard => {
                formatter.write_str("Instance is not in ManualRecoveryRequired")
            }
            Self::InvalidRunArtifact(message) => {
                write!(formatter, "invalid Run Artifact: {message}")
            }
            Self::DatabaseLockPoisoned => formatter.write_str("database mutex poisoned"),
        }
    }
}

impl std::error::Error for PersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::RuntimeContent(source) => Some(source),
            Self::Sqlite { source, .. } => Some(source),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<RuntimeContentStoreError> for PersistenceError {
    fn from(source: RuntimeContentStoreError) -> Self {
        Self::RuntimeContent(source)
    }
}

impl PactrunPersistence {
    pub(crate) fn open(root: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        let root = validate_supported_storage_root(root.as_ref())?;
        let database_root = validate_supported_storage_root(&root.join(DATABASE_DIRECTORY))?;
        let runtime_root = root.join(RUNTIME_CONTENT_DIRECTORY);
        let runtime_content = RuntimeContentStore::open(&runtime_root)?;

        validate_existing_regular_entry(&database_root, DATABASE_NAME)?;
        let database_path = database_root.join(DATABASE_NAME);
        let database = open_database(&database_path)?;
        validate_existing_regular_entry(&database_root, DATABASE_NAME)?;

        Ok(Self {
            database: Mutex::new(database),
            database_path,
            runtime_content,
        })
    }

    pub(crate) fn put_runtime_content<R: Read>(
        &self,
        expected: &Sha256Digest,
        source: &mut R,
    ) -> Result<StoredRuntimeBlob, PersistenceError> {
        self.runtime_content
            .put_verified(expected, source)
            .map_err(PersistenceError::from)
    }

    pub(crate) fn persist_revision(
        &self,
        package_id: PackageId,
        content: &ValidatedRevisionContentV1,
        publications: &[StoredRuntimeBlob],
    ) -> Result<RevisionIdentity, PersistenceError> {
        self.persist_revision_internal(package_id, content, publications, None)
    }

    pub(crate) fn persist_revision_with_metadata(
        &self,
        package_id: PackageId,
        content: &ValidatedRevisionContentV1,
        publications: &[StoredRuntimeBlob],
        metadata: &RevisionMetadataMutationBatch,
    ) -> Result<RevisionIdentity, PersistenceError> {
        self.persist_revision_internal(package_id, content, publications, Some(metadata))
    }

    fn persist_revision_internal(
        &self,
        package_id: PackageId,
        content: &ValidatedRevisionContentV1,
        publications: &[StoredRuntimeBlob],
        metadata: Option<&RevisionMetadataMutationBatch>,
    ) -> Result<RevisionIdentity, PersistenceError> {
        let core_jcs = encode_canonical_revision_core_v1(&content.core)
            .map_err(|error| PersistenceError::CorruptRevision(error.to_string()))?;
        let runtime_content_jcs = encode_canonical_runtime_content_v1(&content.runtime_content)
            .map_err(|error| PersistenceError::CorruptRevision(error.to_string()))?;
        let content_digest = calculate_revision_content_digest_v1(content)
            .map_err(|error| PersistenceError::CorruptRevision(error.to_string()))?;
        let identity = RevisionIdentity::new(package_id, content_digest);
        let expected_references = derive_references(content);

        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin Revision transaction", error))?;

        if let Some(raw) = load_raw_revision(&transaction, &identity)? {
            let actual_references = load_reference_rows(&transaction, &identity)?;
            ensure_package_exists(&transaction, &identity.package_id)?;
            if raw.core_jcs != core_jcs
                || raw.runtime_content_jcs != runtime_content_jcs
                || actual_references != expected_references
            {
                return Err(PersistenceError::CorruptRevision(
                    "existing identity has different canonical bytes or references".to_owned(),
                ));
            }
            validate_raw_revision(&identity, raw, actual_references)?;
            if let Some(metadata) = metadata {
                apply_revision_metadata_in_transaction(&transaction, &identity, metadata)?;
            }
            transaction
                .commit()
                .map_err(|error| PersistenceError::sqlite("commit idempotent retry", error))?;
            return Ok(identity);
        }

        validate_publications(
            &self.runtime_content,
            publications,
            &distinct_blob_digests(content),
        )?;

        transaction
            .execute(
                "INSERT OR IGNORE INTO packages(package_id) VALUES (?1)",
                params![identity.package_id.as_bytes().as_slice()],
            )
            .map_err(|error| PersistenceError::sqlite("persist Package identity", error))?;
        transaction
            .execute(
                "INSERT INTO revisions(\
                    package_id, revision_content_digest, core_jcs, runtime_content_jcs\
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    identity.package_id.as_bytes().as_slice(),
                    identity.content_digest.as_bytes().as_slice(),
                    core_jcs,
                    runtime_content_jcs,
                ],
            )
            .map_err(|error| PersistenceError::sqlite("persist Revision content", error))?;
        for (content_id, blob_digest) in &expected_references {
            transaction
                .execute(
                    "INSERT INTO revision_runtime_content_refs(\
                        package_id, revision_content_digest, content_id, blob_digest\
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        identity.package_id.as_bytes().as_slice(),
                        identity.content_digest.as_bytes().as_slice(),
                        content_id,
                        blob_digest.as_slice(),
                    ],
                )
                .map_err(|error| {
                    PersistenceError::sqlite("persist Revision runtime-content reference", error)
                })?;
        }
        if let Some(metadata) = metadata {
            apply_revision_metadata_in_transaction(&transaction, &identity, metadata)?;
        }
        fault(FaultPoint::BeforeRevisionCommit);
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit Revision transaction", error))?;
        fault(FaultPoint::AfterRevisionCommit);
        Ok(identity)
    }

    pub(crate) fn load_revision(
        &self,
        identity: &RevisionIdentity,
    ) -> Result<Option<StoredRevisionContentV1>, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let Some(raw) = load_raw_revision(&database, identity)? else {
            return Ok(None);
        };
        ensure_package_exists(&database, &identity.package_id)?;
        let references = load_reference_rows(&database, identity)?;
        validate_raw_revision(identity, raw, references).map(Some)
    }

    pub(crate) fn verify_revision_runtime_content(
        &self,
        identity: &RevisionIdentity,
    ) -> Result<(), PersistenceError> {
        let stored = self
            .load_revision(identity)?
            .ok_or_else(|| PersistenceError::MissingRevision(identity.clone()))?;
        self.runtime_content
            .verify_runtime_content_available(&stored.content.runtime_content)
            .map_err(PersistenceError::from)
    }
}

impl PactrunPersistence {
    pub(super) fn open_read_connection(&self) -> Result<Connection, PersistenceError> {
        let database = Connection::open(&self.database_path)
            .map_err(|error| PersistenceError::sqlite("open SQLite read connection", error))?;
        configure_connection(&database)?;
        let journal_mode: String = database
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(|error| PersistenceError::sqlite("verify WAL journal mode", error))?;
        if journal_mode != "wal" {
            return Err(PersistenceError::DatabaseOwnership(format!(
                "read connection journal_mode is {journal_mode:?}, expected WAL"
            )));
        }
        Ok(database)
    }
}

fn open_database(path: &Path) -> Result<Connection, PersistenceError> {
    let mut database = Connection::open(path)
        .map_err(|error| PersistenceError::sqlite("open SQLite database", error))?;
    configure_connection(&database)?;

    classify_database(&database)?;
    establish_wal_mode(&database)?;
    configure_connection(&database)?;
    fault(FaultPoint::AfterWalBeforeBootstrap);

    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| PersistenceError::sqlite("begin schema bootstrap", error))?;
    let state = classify_database(&transaction)?;
    let migrated = match state {
        DatabaseState::Pristine => {
            apply_schema_ladder(&transaction, 0)?;
            transaction
                .pragma_update(None, "application_id", APPLICATION_ID)
                .map_err(|error| PersistenceError::sqlite("set application_id", error))?;
            transaction
                .pragma_update(None, "user_version", SCHEMA_VERSION)
                .map_err(|error| PersistenceError::sqlite("set user_version", error))?;
            validate_schema(&transaction, SCHEMA_VERSION)?;
            fault(FaultPoint::BeforeBootstrapCommit);
            false
        }
        DatabaseState::V1 | DatabaseState::V2 | DatabaseState::V3 => {
            let current = state.version().expect("exact versions carry a version");
            apply_schema_ladder(&transaction, current)?;
            validate_schema(&transaction, SCHEMA_VERSION)?;
            transaction
                .pragma_update(None, "user_version", SCHEMA_VERSION)
                .map_err(|error| PersistenceError::sqlite("set user_version", error))?;
            fault(FaultPoint::BeforeSchemaMigrationCommit);
            true
        }
        DatabaseState::V4 => {
            validate_schema(&transaction, SCHEMA_VERSION)?;
            false
        }
    };
    transaction
        .commit()
        .map_err(|error| PersistenceError::sqlite("commit schema bootstrap", error))?;
    if migrated {
        fault(FaultPoint::AfterSchemaMigrationCommit);
    }
    configure_connection(&database)?;
    Ok(database)
}

/// Executes every ladder step above `current` so the schema becomes exact
/// `SCHEMA_VERSION`; `current = 0` builds the complete schema from nothing.
fn apply_schema_ladder(database: &Connection, current: i64) -> Result<(), PersistenceError> {
    let start = usize::try_from(current).expect("schema versions are non-negative");
    for (sql, label) in &SCHEMA_LADDER[start..] {
        database
            .execute_batch(sql)
            .map_err(|error| PersistenceError::Sqlite {
                operation: label,
                source: error,
            })?;
    }
    Ok(())
}

fn establish_wal_mode(database: &Connection) -> Result<(), PersistenceError> {
    let started = Instant::now();
    loop {
        match database.query_row("PRAGMA journal_mode=WAL", [], |row| row.get::<_, String>(0)) {
            Ok(journal_mode) if journal_mode == "wal" => return Ok(()),
            Ok(journal_mode) => {
                return Err(PersistenceError::DatabaseOwnership(format!(
                    "journal_mode=WAL returned {journal_mode:?}"
                )));
            }
            Err(error)
                if is_transient_journal_mode_race(&error) && started.elapsed() < BUSY_TIMEOUT =>
            {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                return Err(PersistenceError::sqlite(
                    "establish WAL journal mode",
                    error,
                ));
            }
        }
    }
}

fn is_transient_journal_mode_race(error: &rusqlite::Error) -> bool {
    let rusqlite::Error::SqliteFailure(sqlite, _) = error else {
        return false;
    };
    // Concurrent first-time WAL transitions can contend on SQLite's journal
    // files. Keep the retry narrow and bounded; a persistent result is still
    // reported as a bootstrap failure rather than treated as successful WAL.
    matches!(
        sqlite.code,
        ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked
    ) || sqlite.extended_code == rusqlite::ffi::SQLITE_IOERR_FSTAT
}

fn configure_connection(database: &Connection) -> Result<(), PersistenceError> {
    database
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|error| PersistenceError::sqlite("set SQLite busy timeout", error))?;
    database
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|error| PersistenceError::sqlite("enable foreign keys", error))?;
    database
        .pragma_update(None, "synchronous", "FULL")
        .map_err(|error| PersistenceError::sqlite("set synchronous FULL", error))?;

    let foreign_keys: i64 = database
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .map_err(|error| PersistenceError::sqlite("verify foreign keys", error))?;
    let synchronous: i64 = database
        .pragma_query_value(None, "synchronous", |row| row.get(0))
        .map_err(|error| PersistenceError::sqlite("verify synchronous mode", error))?;
    if foreign_keys != 1 || synchronous != 2 {
        return Err(PersistenceError::DatabaseOwnership(format!(
            "connection settings are foreign_keys={foreign_keys}, synchronous={synchronous}"
        )));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DatabaseState {
    Pristine,
    V1,
    V2,
    V3,
    V4,
}

impl DatabaseState {
    fn version(self) -> Option<i64> {
        match self {
            Self::Pristine => None,
            Self::V1 => Some(SCHEMA_V1_VERSION),
            Self::V2 => Some(SCHEMA_V2_VERSION),
            Self::V3 => Some(SCHEMA_V3_VERSION),
            Self::V4 => Some(SCHEMA_VERSION),
        }
    }
}

fn classify_database(database: &Connection) -> Result<DatabaseState, PersistenceError> {
    let application_id: i64 = database
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(|error| PersistenceError::sqlite("read application_id", error))?;
    let user_version: i64 = database
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| PersistenceError::sqlite("read user_version", error))?;
    let has_user_objects = !schema_objects(database)?.is_empty();

    match (application_id, user_version, has_user_objects) {
        (0, 0, false) => Ok(DatabaseState::Pristine),
        (APPLICATION_ID, SCHEMA_V1_VERSION, true) => {
            validate_schema(database, SCHEMA_V1_VERSION)?;
            Ok(DatabaseState::V1)
        }
        (APPLICATION_ID, SCHEMA_V2_VERSION, true) => {
            validate_schema(database, SCHEMA_V2_VERSION)?;
            Ok(DatabaseState::V2)
        }
        (APPLICATION_ID, SCHEMA_V3_VERSION, true) => {
            validate_schema(database, SCHEMA_V3_VERSION)?;
            Ok(DatabaseState::V3)
        }
        (APPLICATION_ID, SCHEMA_VERSION, true) => {
            validate_schema(database, SCHEMA_VERSION)?;
            Ok(DatabaseState::V4)
        }
        (APPLICATION_ID, version, _) if version > SCHEMA_VERSION => {
            Err(PersistenceError::DatabaseOwnership(format!(
                "unsupported newer persistence schema version {version}"
            )))
        }
        (application, _, _) if application != 0 && application != APPLICATION_ID => {
            Err(PersistenceError::DatabaseOwnership(format!(
                "foreign application_id 0x{application:08x}"
            )))
        }
        (0, 0, true) => Err(PersistenceError::DatabaseOwnership(
            "unmarked database contains user schema objects".to_owned(),
        )),
        _ => Err(PersistenceError::DatabaseOwnership(format!(
            "incompatible ownership markers application_id=0x{application_id:08x}, user_version={user_version}"
        ))),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SchemaObject {
    object_type: String,
    name: String,
    table_name: String,
    sql: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TableListRow {
    name: String,
    object_type: String,
    columns: i64,
    without_rowid: i64,
    strict: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ColumnRow {
    position: i64,
    name: String,
    declared_type: String,
    not_null: i64,
    default_value: Option<String>,
    primary_key_ordinal: i64,
    hidden: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ForeignKeyRow {
    id: i64,
    sequence: i64,
    parent_table: String,
    child_column: String,
    parent_column: String,
    on_update: String,
    on_delete: String,
    match_clause: String,
}

fn validate_schema(database: &Connection, version: i64) -> Result<(), PersistenceError> {
    if !(SCHEMA_V1_VERSION..=SCHEMA_VERSION).contains(&version) {
        return Err(PersistenceError::SchemaMismatch(format!(
            "unsupported expected schema version {version}"
        )));
    }
    let expected = Connection::open_in_memory()
        .map_err(|error| PersistenceError::sqlite("open expected schema database", error))?;
    let steps = usize::try_from(version).expect("schema versions are non-negative");
    for (sql, _) in &SCHEMA_LADDER[..steps] {
        expected
            .execute_batch(sql)
            .map_err(|error| PersistenceError::sqlite("construct expected schema", error))?;
    }

    if schema_objects(database)? != schema_objects(&expected)? {
        return Err(PersistenceError::SchemaMismatch(format!(
            "sqlite_schema manifest differs from PersistenceSchemaV{version}"
        )));
    }
    if table_list(database)? != table_list(&expected)? {
        return Err(PersistenceError::SchemaMismatch(
            "STRICT or WITHOUT ROWID table structure differs".to_owned(),
        ));
    }
    let expected_tables = table_list(&expected)?
        .into_iter()
        .filter(|row| row.object_type == "table")
        .map(|row| row.name)
        .collect::<Vec<_>>();
    for table in expected_tables {
        if table_columns(database, &table)? != table_columns(&expected, &table)? {
            return Err(PersistenceError::SchemaMismatch(format!(
                "column structure differs for {table}"
            )));
        }
        if foreign_keys(database, &table)? != foreign_keys(&expected, &table)? {
            return Err(PersistenceError::SchemaMismatch(format!(
                "foreign keys differ for {table}"
            )));
        }
    }
    Ok(())
}

fn schema_objects(database: &Connection) -> Result<Vec<SchemaObject>, PersistenceError> {
    let mut statement = database
        .prepare(
            "SELECT type, name, tbl_name, sql \
             FROM main.sqlite_schema \
             WHERE name NOT LIKE 'sqlite_%' \
             ORDER BY type, name",
        )
        .map_err(|error| PersistenceError::sqlite("prepare schema manifest", error))?;
    statement
        .query_map([], |row| {
            Ok(SchemaObject {
                object_type: row.get(0)?,
                name: row.get(1)?,
                table_name: row.get(2)?,
                sql: row.get(3)?,
            })
        })
        .map_err(|error| PersistenceError::sqlite("query schema manifest", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read schema manifest", error))
}

fn table_list(database: &Connection) -> Result<Vec<TableListRow>, PersistenceError> {
    let mut statement = database
        .prepare(
            "SELECT name, type, ncol, wr, strict \
             FROM pragma_table_list \
             WHERE schema = 'main' AND name NOT LIKE 'sqlite_%' \
             ORDER BY name",
        )
        .map_err(|error| PersistenceError::sqlite("prepare table list", error))?;
    statement
        .query_map([], |row| {
            Ok(TableListRow {
                name: row.get(0)?,
                object_type: row.get(1)?,
                columns: row.get(2)?,
                without_rowid: row.get(3)?,
                strict: row.get(4)?,
            })
        })
        .map_err(|error| PersistenceError::sqlite("query table list", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read table list", error))
}

fn table_columns(database: &Connection, table: &str) -> Result<Vec<ColumnRow>, PersistenceError> {
    let mut statement = database
        .prepare(
            "SELECT cid, name, type, \"notnull\", dflt_value, pk, hidden \
             FROM pragma_table_xinfo(?1) ORDER BY cid",
        )
        .map_err(|error| PersistenceError::sqlite("prepare table columns", error))?;
    statement
        .query_map([table], |row| {
            Ok(ColumnRow {
                position: row.get(0)?,
                name: row.get(1)?,
                declared_type: row.get(2)?,
                not_null: row.get(3)?,
                default_value: row.get(4)?,
                primary_key_ordinal: row.get(5)?,
                hidden: row.get(6)?,
            })
        })
        .map_err(|error| PersistenceError::sqlite("query table columns", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read table columns", error))
}

fn foreign_keys(
    database: &Connection,
    table: &str,
) -> Result<Vec<ForeignKeyRow>, PersistenceError> {
    let mut statement = database
        .prepare(
            "SELECT id, seq, \"table\", \"from\", \"to\", on_update, on_delete, \"match\" \
             FROM pragma_foreign_key_list(?1) ORDER BY id, seq",
        )
        .map_err(|error| PersistenceError::sqlite("prepare foreign keys", error))?;
    statement
        .query_map([table], |row| {
            Ok(ForeignKeyRow {
                id: row.get(0)?,
                sequence: row.get(1)?,
                parent_table: row.get(2)?,
                child_column: row.get(3)?,
                parent_column: row.get(4)?,
                on_update: row.get(5)?,
                on_delete: row.get(6)?,
                match_clause: row.get(7)?,
            })
        })
        .map_err(|error| PersistenceError::sqlite("query foreign keys", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read foreign keys", error))
}

#[derive(Debug)]
struct RawRevision {
    core_jcs: Vec<u8>,
    runtime_content_jcs: Vec<u8>,
}

fn load_raw_revision(
    database: &Connection,
    identity: &RevisionIdentity,
) -> Result<Option<RawRevision>, PersistenceError> {
    database
        .query_row(
            "SELECT core_jcs, runtime_content_jcs \
             FROM revisions \
             WHERE package_id = ?1 AND revision_content_digest = ?2",
            params![
                identity.package_id.as_bytes().as_slice(),
                identity.content_digest.as_bytes().as_slice(),
            ],
            |row| {
                Ok(RawRevision {
                    core_jcs: row.get(0)?,
                    runtime_content_jcs: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load Revision row", error))
}

fn ensure_package_exists(
    database: &Connection,
    package_id: &PackageId,
) -> Result<(), PersistenceError> {
    let exists: bool = database
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM packages WHERE package_id = ?1)",
            params![package_id.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| PersistenceError::sqlite("load Package identity", error))?;
    if exists {
        Ok(())
    } else {
        Err(PersistenceError::CorruptRevision(
            "Revision refers to a missing Package identity".to_owned(),
        ))
    }
}

fn load_reference_rows(
    database: &Connection,
    identity: &RevisionIdentity,
) -> Result<BTreeMap<String, [u8; 32]>, PersistenceError> {
    let mut statement = database
        .prepare(
            "SELECT content_id, blob_digest \
             FROM revision_runtime_content_refs \
             WHERE package_id = ?1 AND revision_content_digest = ?2 \
             ORDER BY content_id COLLATE BINARY",
        )
        .map_err(|error| PersistenceError::sqlite("prepare runtime references", error))?;
    let rows = statement
        .query_map(
            params![
                identity.package_id.as_bytes().as_slice(),
                identity.content_digest.as_bytes().as_slice(),
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .map_err(|error| PersistenceError::sqlite("query runtime references", error))?;
    let mut references = BTreeMap::new();
    for row in rows {
        let (content_id, digest) =
            row.map_err(|error| PersistenceError::sqlite("read runtime reference", error))?;
        let digest: [u8; 32] = digest.try_into().map_err(|_| {
            PersistenceError::CorruptRevision(
                "runtime reference contains a non-SHA-256 blob digest".to_owned(),
            )
        })?;
        if references.insert(content_id, digest).is_some() {
            return Err(PersistenceError::CorruptRevision(
                "runtime reference index contains a duplicate ContentId".to_owned(),
            ));
        }
    }
    Ok(references)
}

fn validate_raw_revision(
    identity: &RevisionIdentity,
    raw: RawRevision,
    actual_references: BTreeMap<String, [u8; 32]>,
) -> Result<StoredRevisionContentV1, PersistenceError> {
    let content = decode_canonical_revision_content_v1(&raw.core_jcs, &raw.runtime_content_jcs)
        .map_err(|error| PersistenceError::CorruptRevision(error.to_string()))?;
    let calculated = calculate_revision_content_digest_v1(&content)
        .map_err(|error| PersistenceError::CorruptRevision(error.to_string()))?;
    if calculated != identity.content_digest {
        return Err(PersistenceError::CorruptRevision(
            "stored canonical components do not match the Revision identity".to_owned(),
        ));
    }
    if derive_references(&content) != actual_references {
        return Err(PersistenceError::CorruptRevision(
            "runtime reference index differs from the canonical closure".to_owned(),
        ));
    }
    Ok(StoredRevisionContentV1 {
        identity: identity.clone(),
        content,
    })
}

fn derive_references(content: &ValidatedRevisionContentV1) -> BTreeMap<String, [u8; 32]> {
    content
        .runtime_content
        .files()
        .iter()
        .map(|file| (file.id.as_str().to_owned(), file.blob_digest.to_bytes()))
        .collect()
}

fn distinct_blob_digests(content: &ValidatedRevisionContentV1) -> BTreeSet<Sha256Digest> {
    content
        .runtime_content
        .files()
        .iter()
        .map(|file| file.blob_digest.clone())
        .collect()
}

fn validate_publications(
    store: &RuntimeContentStore,
    publications: &[StoredRuntimeBlob],
    required: &BTreeSet<Sha256Digest>,
) -> Result<(), PersistenceError> {
    let mut supplied = BTreeSet::new();
    for publication in publications {
        if !store.owns_publication(publication) {
            return Err(PersistenceError::PublicationWitness(
                "token belongs to another RuntimeContentStore instance".to_owned(),
            ));
        }
        if !supplied.insert(publication.digest().clone()) {
            return Err(PersistenceError::PublicationWitness(format!(
                "duplicate token for {}",
                publication.digest().as_str()
            )));
        }
    }
    if &supplied != required {
        let missing = required
            .difference(&supplied)
            .map(Sha256Digest::as_str)
            .collect::<Vec<_>>();
        let extra = supplied
            .difference(required)
            .map(Sha256Digest::as_str)
            .collect::<Vec<_>>();
        return Err(PersistenceError::PublicationWitness(format!(
            "digest set mismatch; missing={missing:?}, extra={extra:?}"
        )));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FaultPoint {
    AfterWalBeforeBootstrap,
    BeforeBootstrapCommit,
    BeforeSchemaMigrationCommit,
    AfterSchemaMigrationCommit,
    BeforeMetadataCommit,
    AfterMetadataCommit,
    BeforeRevisionCommit,
    AfterRevisionCommit,
    BeforeRunAcceptCommit,
    AfterRunAcceptCommit,
    BeforeRunAdmitCommit,
    AfterRunAdmitCommit,
    BeforeRecoveryRiskCommit,
    AfterRecoveryRiskCommit,
    BeforeRunFinishCommit,
    AfterRunFinishCommit,
}

impl FaultPoint {
    #[cfg(test)]
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::AfterWalBeforeBootstrap => "after_wal_before_bootstrap",
            Self::BeforeBootstrapCommit => "before_bootstrap_commit",
            Self::BeforeSchemaMigrationCommit => "before_schema_migration_commit",
            Self::AfterSchemaMigrationCommit => "after_schema_migration_commit",
            Self::BeforeMetadataCommit => "before_metadata_commit",
            Self::AfterMetadataCommit => "after_metadata_commit",
            Self::BeforeRevisionCommit => "before_revision_commit",
            Self::AfterRevisionCommit => "after_revision_commit",
            Self::BeforeRunAcceptCommit => "before_run_accept_commit",
            Self::AfterRunAcceptCommit => "after_run_accept_commit",
            Self::BeforeRunAdmitCommit => "before_run_admit_commit",
            Self::AfterRunAdmitCommit => "after_run_admit_commit",
            Self::BeforeRecoveryRiskCommit => "before_recovery_risk_commit",
            Self::AfterRecoveryRiskCommit => "after_recovery_risk_commit",
            Self::BeforeRunFinishCommit => "before_run_finish_commit",
            Self::AfterRunFinishCommit => "after_run_finish_commit",
        }
    }
}

#[cfg(not(test))]
pub(crate) fn fault(_point: FaultPoint) {}

#[cfg(test)]
pub(crate) fn fault(point: FaultPoint) {
    if ["PACTRUN_M1C_FAULT", "PACTRUN_M1D_FAULT", "PACTRUN_M3_FAULT"]
        .iter()
        .any(|variable| std::env::var_os(variable).is_some_and(|value| value == point.name()))
    {
        std::process::exit(87);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Cursor,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::Arc,
        thread,
    };

    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;
    use crate::domain::{
        ContentId, RevisionCoreProjectionInputV1, RuntimeContentProjectionInputV1,
        RuntimeFileKindV1, RuntimeFileV1, RuntimePath, project_revision_core_v1,
        project_runtime_content_closure_v1, validate_revision_content_v1,
    };

    const WORKER_TEST: &str = "persistence::sqlite_revision_store::tests::m1c_subprocess_worker";

    fn digest(bytes: &[u8]) -> Sha256Digest {
        Sha256Digest::from_bytes(Sha256::digest(bytes).into())
    }

    fn package(tag: u8) -> PackageId {
        PackageId::from_bytes([tag; 16])
    }

    fn test_root() -> (TempDir, PathBuf) {
        let parent = std::env::var_os("PACTRUN_M1C_TEST_PARENT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m1c-tests"));
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("sqlite-persistence-")
            .tempdir_in(parent)
            .unwrap();
        let root = temporary.path().join("pactrun");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join(DATABASE_DIRECTORY)).unwrap();
        fs::create_dir(root.join(RUNTIME_CONTENT_DIRECTORY)).unwrap();
        (temporary, root)
    }

    fn database_path(root: &Path) -> PathBuf {
        root.join(DATABASE_DIRECTORY).join(DATABASE_NAME)
    }

    fn final_blob_path(root: &Path, digest: &Sha256Digest) -> PathBuf {
        root.join(RUNTIME_CONTENT_DIRECTORY).join(
            digest
                .as_str()
                .strip_prefix("sha256:")
                .expect("typed digest prefix"),
        )
    }

    fn content_with_files(files: Vec<RuntimeFileV1>) -> ValidatedRevisionContentV1 {
        let core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
            inputs: Vec::new(),
            actions: Vec::new(),
            snapshot: None,
            migrations: Vec::new(),
            cleanup: None,
        })
        .unwrap();
        let runtime_content =
            project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files }).unwrap();
        validate_revision_content_v1(core, runtime_content).unwrap()
    }

    fn shared_content(blob_digest: &Sha256Digest) -> ValidatedRevisionContentV1 {
        content_with_files(vec![
            RuntimeFileV1 {
                id: ContentId::parse("launcher").unwrap(),
                path: RuntimePath::parse("bin/launcher").unwrap(),
                kind: RuntimeFileKindV1::RegularFile,
                blob_digest: blob_digest.clone(),
                executable: true,
            },
            RuntimeFileV1 {
                id: ContentId::parse("script").unwrap(),
                path: RuntimePath::parse("hooks/script").unwrap(),
                kind: RuntimeFileKindV1::RegularFile,
                blob_digest: blob_digest.clone(),
                executable: false,
            },
        ])
    }

    fn one_file_content(
        id: &str,
        path: &str,
        blob_digest: &Sha256Digest,
    ) -> ValidatedRevisionContentV1 {
        content_with_files(vec![RuntimeFileV1 {
            id: ContentId::parse(id).unwrap(),
            path: RuntimePath::parse(path).unwrap(),
            kind: RuntimeFileKindV1::RegularFile,
            blob_digest: blob_digest.clone(),
            executable: false,
        }])
    }

    fn raw_database(root: &Path) -> Connection {
        let database = Connection::open(database_path(root)).unwrap();
        database.pragma_update(None, "foreign_keys", "ON").unwrap();
        database
    }

    fn initialize_direct(root: &Path, application_id: i64, user_version: i64, sql: &str) {
        let database = Connection::open(database_path(root)).unwrap();
        database.execute_batch(sql).unwrap();
        database
            .pragma_update(None, "application_id", application_id)
            .unwrap();
        database
            .pragma_update(None, "user_version", user_version)
            .unwrap();
    }

    fn run_worker(root: &Path, operation: &str, fault: Option<FaultPoint>, marker: &Path) -> bool {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg(WORKER_TEST)
            .arg("--nocapture")
            .env("PACTRUN_M1C_WORKER", operation)
            .env("PACTRUN_M1C_ROOT", root)
            .env("PACTRUN_M1C_SUCCESS_MARKER", marker)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(fault) = fault {
            command.env("PACTRUN_M1C_FAULT", fault.name());
        }
        let output = command.output().unwrap();
        if !output.status.success() && fault.is_none() {
            eprintln!(
                "M1-C worker failed with {}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        output.status.success()
    }

    fn persist_sample(
        persistence: &PactrunPersistence,
        package_id: PackageId,
        bytes: &[u8],
    ) -> (
        ValidatedRevisionContentV1,
        StoredRuntimeBlob,
        RevisionIdentity,
    ) {
        let blob_digest = digest(bytes);
        let publication = persistence
            .put_runtime_content(&blob_digest, &mut Cursor::new(bytes))
            .unwrap();
        let content = shared_content(&blob_digest);
        let identity = persistence
            .persist_revision(package_id, &content, std::slice::from_ref(&publication))
            .unwrap();
        (content, publication, identity)
    }

    // Test-ID: PR-TEST-0052
    // Verifies: PR-REQ-0231, PR-REQ-0234
    #[test]
    fn schema_bootstrap_is_exact_owned_and_concurrency_safe() {
        let (_temporary, root) = test_root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let database = persistence.database.lock().unwrap();
        assert_eq!(
            database
                .pragma_query_value(None, "application_id", |row| row.get::<_, i64>(0))
                .unwrap(),
            APPLICATION_ID
        );
        assert_eq!(
            database
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        assert_eq!(
            database
                .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "wal"
        );
        assert_eq!(
            database
                .pragma_query_value(None, "synchronous", |row| row.get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            database
                .pragma_query_value(None, "foreign_keys", |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        validate_schema(&database, SCHEMA_VERSION).unwrap();
        assert!(
            database
                .execute(
                    "INSERT INTO packages(package_id) VALUES (?1)",
                    [vec![0_u8; 15]],
                )
                .is_err()
        );
        database
            .execute("INSERT INTO packages(package_id) VALUES (?1)", [[7_u8; 16]])
            .unwrap();
        assert!(
            database
                .execute(
                    "INSERT INTO revisions(\
                    package_id, revision_content_digest, core_jcs, runtime_content_jcs\
                 ) VALUES (?1, ?2, ?3, ?4)",
                    params![[7_u8; 16], vec![0_u8; 31], b"{}", b"{}"],
                )
                .is_err()
        );
        drop(database);

        let bytes = b"schema length fixture";
        let (_, _, identity) = persist_sample(&persistence, package(8), bytes);
        let database = persistence.database.lock().unwrap();
        assert!(
            database
                .execute(
                    "INSERT INTO revision_runtime_content_refs(\
                    package_id, revision_content_digest, content_id, blob_digest\
                 ) VALUES (?1, ?2, 'invalid_length', ?3)",
                    params![
                        identity.package_id.as_bytes().as_slice(),
                        identity.content_digest.as_bytes().as_slice(),
                        vec![0_u8; 31],
                    ],
                )
                .is_err()
        );
        drop(database);

        let (_foreign_temporary, foreign_root) = test_root();
        initialize_direct(
            &foreign_root,
            0x1234,
            1,
            "CREATE TABLE foreign_data(value TEXT);",
        );
        assert!(PactrunPersistence::open(&foreign_root).is_err());

        let (_unmarked_temporary, unmarked_root) = test_root();
        initialize_direct(&unmarked_root, 0, 0, "CREATE TABLE unmarked(value TEXT);");
        assert!(PactrunPersistence::open(&unmarked_root).is_err());

        let (_newer_temporary, newer_root) = test_root();
        initialize_direct(
            &newer_root,
            APPLICATION_ID,
            SCHEMA_VERSION + 1,
            &format!("{SCHEMA_V1_SQL}\n{SCHEMA_V2_ADDITIONS_SQL}"),
        );
        assert!(PactrunPersistence::open(&newer_root).is_err());

        let (_drift_temporary, drift_root) = test_root();
        initialize_direct(
            &drift_root,
            APPLICATION_ID,
            SCHEMA_VERSION,
            &format!(
                "{SCHEMA_V1_SQL}\n{SCHEMA_V2_ADDITIONS_SQL}\nCREATE TABLE drift(value TEXT) STRICT;"
            ),
        );
        assert!(PactrunPersistence::open(&drift_root).is_err());

        for fault in [
            FaultPoint::AfterWalBeforeBootstrap,
            FaultPoint::BeforeBootstrapCommit,
        ] {
            let (temporary, crash_root) = test_root();
            let marker = temporary.path().join("bootstrap.success");
            assert!(!run_worker(&crash_root, "open", Some(fault), &marker));
            assert!(!marker.exists());
            PactrunPersistence::open(&crash_root).unwrap();
        }

        let (concurrent_temporary, concurrent_root) = test_root();
        let workers = (0..2)
            .map(|index| {
                let root = concurrent_root.clone();
                let marker = concurrent_temporary
                    .path()
                    .join(format!("bootstrap-{index}.success"));
                thread::spawn(move || (run_worker(&root, "open", None, &marker), marker))
            })
            .collect::<Vec<_>>();
        for worker in workers {
            let (success, marker) = worker.join().unwrap();
            assert!(success);
            assert!(marker.exists());
        }
        PactrunPersistence::open(&concurrent_root).unwrap();
    }

    // Test-ID: PR-TEST-0073
    // Verifies: PR-REQ-0078, PR-REQ-0269, PR-REQ-0270
    #[test]
    fn exact_v2_migrates_transactionally_to_current_and_preserves_wal() {
        let (_temporary, root) = test_root();
        initialize_direct(
            &root,
            APPLICATION_ID,
            SCHEMA_V2_VERSION,
            &format!("{SCHEMA_V1_SQL}\n{SCHEMA_V2_ADDITIONS_SQL}"),
        );
        let persistence = PactrunPersistence::open(&root).unwrap();
        let database = persistence.database.lock().unwrap();
        assert_eq!(
            database
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        assert_eq!(
            database
                .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "wal"
        );
        validate_schema(&database, SCHEMA_VERSION).unwrap();
        drop(database);

        let (before_temporary, before_root) = test_root();
        initialize_direct(
            &before_root,
            APPLICATION_ID,
            SCHEMA_V2_VERSION,
            &format!("{SCHEMA_V1_SQL}\n{SCHEMA_V2_ADDITIONS_SQL}"),
        );
        let before_marker = before_temporary.path().join("migration-before.success");
        assert!(!run_worker(
            &before_root,
            "open",
            Some(FaultPoint::BeforeSchemaMigrationCommit),
            &before_marker,
        ));
        let database = raw_database(&before_root);
        assert_eq!(
            database
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_V2_VERSION
        );
        assert_eq!(
            database
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name='instances'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        drop(database);
        PactrunPersistence::open(&before_root).unwrap();

        let (after_temporary, after_root) = test_root();
        initialize_direct(
            &after_root,
            APPLICATION_ID,
            SCHEMA_V2_VERSION,
            &format!("{SCHEMA_V1_SQL}\n{SCHEMA_V2_ADDITIONS_SQL}"),
        );
        let after_marker = after_temporary.path().join("migration-after.success");
        assert!(!run_worker(
            &after_root,
            "open",
            Some(FaultPoint::AfterSchemaMigrationCommit),
            &after_marker,
        ));
        let database = raw_database(&after_root);
        assert_eq!(
            database
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        validate_schema(&database, SCHEMA_VERSION).unwrap();
        drop(database);
        PactrunPersistence::open(&after_root).unwrap();
    }

    // Test-ID: PR-TEST-0053
    // Verifies: PR-REQ-0232
    #[test]
    fn exact_revision_components_and_derived_references_round_trip() {
        let (_temporary, root) = test_root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let bytes = b"shared canonical blob";
        let (content, publication, identity) = persist_sample(&persistence, package(1), bytes);

        let expected_core = encode_canonical_revision_core_v1(&content.core).unwrap();
        let expected_runtime =
            encode_canonical_runtime_content_v1(&content.runtime_content).unwrap();
        let database = persistence.database.lock().unwrap();
        let stored: (Vec<u8>, Vec<u8>) = database
            .query_row(
                "SELECT core_jcs, runtime_content_jcs FROM revisions \
                 WHERE package_id=?1 AND revision_content_digest=?2",
                params![
                    identity.package_id.as_bytes().as_slice(),
                    identity.content_digest.as_bytes().as_slice(),
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (expected_core.clone(), expected_runtime.clone()));
        assert_eq!(
            load_reference_rows(&database, &identity).unwrap(),
            derive_references(&content)
        );
        drop(database);

        let loaded = persistence.load_revision(&identity).unwrap().unwrap();
        assert_eq!(loaded.identity, identity);
        assert_eq!(loaded.content, content);
        assert_eq!(
            calculate_revision_content_digest_v1(&loaded.content).unwrap(),
            identity.content_digest
        );

        let other_package = persistence
            .persist_revision(package(2), &content, std::slice::from_ref(&publication))
            .unwrap();
        assert_eq!(other_package.content_digest, identity.content_digest);
        assert_ne!(other_package.package_id, identity.package_id);

        let other_bytes = b"different canonical blob";
        let other_digest = digest(other_bytes);
        let other_publication = persistence
            .put_runtime_content(&other_digest, &mut Cursor::new(other_bytes))
            .unwrap();
        let other_content = one_file_content("other", "lib/other", &other_digest);
        let other_revision = persistence
            .persist_revision(
                identity.package_id,
                &other_content,
                std::slice::from_ref(&other_publication),
            )
            .unwrap();
        assert_ne!(other_revision.content_digest, identity.content_digest);
        assert_eq!(
            expected_core,
            encode_canonical_revision_core_v1(&content.core).unwrap()
        );
        assert_eq!(
            expected_runtime,
            encode_canonical_runtime_content_v1(&content.runtime_content).unwrap()
        );
    }

    // Test-ID: PR-TEST-0054
    // Verifies: PR-REQ-0232, PR-REQ-0234
    #[test]
    fn persisted_revision_records_are_immutable_and_exactly_idempotent() {
        let (_temporary, root) = test_root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let (content, _, identity) = persist_sample(&persistence, package(3), b"immutable");
        assert_eq!(
            persistence
                .persist_revision(identity.package_id, &content, &[])
                .unwrap(),
            identity
        );

        for mutation in ["core", "runtime", "reference"] {
            let (_temporary, root) = test_root();
            let persistence = PactrunPersistence::open(&root).unwrap();
            let (content, _, identity) =
                persist_sample(&persistence, package(4), mutation.as_bytes());
            let database = persistence.database.lock().unwrap();
            match mutation {
                "core" => {
                    database
                        .execute(
                            "UPDATE revisions SET core_jcs=?3 \
                             WHERE package_id=?1 AND revision_content_digest=?2",
                            params![
                                identity.package_id.as_bytes().as_slice(),
                                identity.content_digest.as_bytes().as_slice(),
                                b"{}",
                            ],
                        )
                        .unwrap();
                }
                "runtime" => {
                    database
                        .execute(
                            "UPDATE revisions SET runtime_content_jcs=?3 \
                             WHERE package_id=?1 AND revision_content_digest=?2",
                            params![
                                identity.package_id.as_bytes().as_slice(),
                                identity.content_digest.as_bytes().as_slice(),
                                b"{\"files\":[]}",
                            ],
                        )
                        .unwrap();
                }
                "reference" => {
                    database
                        .execute(
                            "UPDATE revision_runtime_content_refs SET blob_digest=?3 \
                             WHERE package_id=?1 AND revision_content_digest=?2",
                            params![
                                identity.package_id.as_bytes().as_slice(),
                                identity.content_digest.as_bytes().as_slice(),
                                [0x55_u8; 32],
                            ],
                        )
                        .unwrap();
                }
                _ => unreachable!(),
            }
            drop(database);
            assert!(
                persistence
                    .persist_revision(identity.package_id, &content, &[])
                    .is_err()
            );
        }
    }

    // Test-ID: PR-TEST-0055
    // Verifies: PR-REQ-0232, PR-REQ-0233
    #[test]
    fn publication_witnesses_authorize_only_distinct_physical_digests() {
        let (_temporary, root) = test_root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let shared_bytes = b"one witness two content ids";
        let shared_digest = digest(shared_bytes);
        let publication = persistence
            .put_runtime_content(&shared_digest, &mut Cursor::new(shared_bytes))
            .unwrap();
        let content = shared_content(&shared_digest);

        let exact = persistence
            .persist_revision(package(10), &content, std::slice::from_ref(&publication))
            .unwrap();
        assert_eq!(
            load_reference_rows(&persistence.database.lock().unwrap(), &exact)
                .unwrap()
                .len(),
            2
        );
        assert!(
            persistence
                .persist_revision(package(11), &content, &[])
                .is_err()
        );
        assert!(
            persistence
                .persist_revision(
                    package(12),
                    &content,
                    &[publication.clone(), publication.clone()]
                )
                .is_err()
        );

        let extra_bytes = b"extra witness";
        let extra_digest = digest(extra_bytes);
        let extra = persistence
            .put_runtime_content(&extra_digest, &mut Cursor::new(extra_bytes))
            .unwrap();
        assert!(
            persistence
                .persist_revision(package(13), &content, &[publication.clone(), extra])
                .is_err()
        );

        let (_other_temporary, other_root) = test_root();
        let other = PactrunPersistence::open(&other_root).unwrap();
        let wrong_store = other
            .put_runtime_content(&shared_digest, &mut Cursor::new(shared_bytes))
            .unwrap();
        assert!(
            persistence
                .persist_revision(package(14), &content, &[wrong_store])
                .is_err()
        );

        let empty = content_with_files(Vec::new());
        persistence
            .persist_revision(package(15), &empty, &[])
            .unwrap();
        assert!(
            schema_objects(&persistence.database.lock().unwrap())
                .unwrap()
                .iter()
                .all(|object| !object
                    .sql
                    .as_deref()
                    .unwrap_or_default()
                    .contains("witness"))
        );
    }

    // Test-ID: PR-TEST-0056
    // Verifies: PR-REQ-0231, PR-REQ-0233, PR-REQ-0234
    #[test]
    fn concurrent_and_crashed_transactions_recover_without_old_witnesses() {
        let (_temporary, root) = test_root();
        let persistence = Arc::new(PactrunPersistence::open(&root).unwrap());
        let bytes = b"threaded Revision";
        let blob_digest = digest(bytes);
        let publication = persistence
            .put_runtime_content(&blob_digest, &mut Cursor::new(bytes))
            .unwrap();
        let content = shared_content(&blob_digest);
        let writers = (0..8)
            .map(|_| {
                let persistence = Arc::clone(&persistence);
                let content = content.clone();
                let publication = publication.clone();
                thread::spawn(move || {
                    persistence.persist_revision(package(20), &content, &[publication])
                })
            })
            .collect::<Vec<_>>();
        let identities = writers
            .into_iter()
            .map(|writer| writer.join().unwrap().unwrap())
            .collect::<Vec<_>>();
        assert!(identities.windows(2).all(|pair| pair[0] == pair[1]));

        let (process_temporary, process_root) = test_root();
        let processes = (0..3)
            .map(|index| {
                let root = process_root.clone();
                let marker = process_temporary
                    .path()
                    .join(format!("writer-{index}.success"));
                thread::spawn(move || (run_worker(&root, "persist", None, &marker), marker))
            })
            .collect::<Vec<_>>();
        for process in processes {
            let (success, marker) = process.join().unwrap();
            assert!(success);
            assert!(marker.exists());
        }

        let expected_content = shared_content(&digest(b"m1c worker content"));
        let expected_identity = RevisionIdentity::new(
            package(42),
            calculate_revision_content_digest_v1(&expected_content).unwrap(),
        );
        let reopened = PactrunPersistence::open(&process_root).unwrap();
        assert!(
            reopened
                .load_revision(&expected_identity)
                .unwrap()
                .is_some()
        );

        for fault in [
            FaultPoint::BeforeRevisionCommit,
            FaultPoint::AfterRevisionCommit,
        ] {
            let (temporary, crash_root) = test_root();
            let marker = temporary.path().join("revision.success");
            assert!(!run_worker(&crash_root, "persist", Some(fault), &marker));
            assert!(!marker.exists());
            let reopened = PactrunPersistence::open(&crash_root).unwrap();
            let content = shared_content(&digest(b"m1c worker content"));
            let identity = RevisionIdentity::new(
                package(42),
                calculate_revision_content_digest_v1(&content).unwrap(),
            );
            match fault {
                FaultPoint::BeforeRevisionCommit => {
                    assert!(reopened.load_revision(&identity).unwrap().is_none());
                    assert!(final_blob_path(&crash_root, &digest(b"m1c worker content")).exists());
                }
                FaultPoint::AfterRevisionCommit => {
                    assert!(reopened.load_revision(&identity).unwrap().is_some());
                    assert_eq!(
                        reopened
                            .persist_revision(identity.package_id, &content, &[])
                            .unwrap(),
                        identity
                    );
                }
                _ => unreachable!(),
            }
        }
    }

    // Test-ID: PR-TEST-0057
    // Verifies: PR-REQ-0232, PR-REQ-0234
    #[test]
    fn logical_revision_integrity_is_separate_from_physical_blob_availability() {
        for physical_state in ["missing", "corrupt"] {
            let (_temporary, root) = test_root();
            let persistence = PactrunPersistence::open(&root).unwrap();
            let bytes = physical_state.as_bytes();
            let (content, _, identity) = persist_sample(&persistence, package(30), bytes);
            let blob_path = final_blob_path(&root, &digest(bytes));
            if physical_state == "missing" {
                fs::remove_file(blob_path).unwrap();
            } else {
                fs::write(blob_path, b"externally corrupted").unwrap();
            }
            let loaded = persistence.load_revision(&identity).unwrap().unwrap();
            assert_eq!(loaded.content, content);
            assert!(
                persistence
                    .verify_revision_runtime_content(&identity)
                    .is_err()
            );
        }

        let (_canonical_temporary, canonical_root) = test_root();
        let canonical = PactrunPersistence::open(&canonical_root).unwrap();
        let (content, _, identity) = persist_sample(&canonical, package(31), b"canonical");
        let before_core = encode_canonical_revision_core_v1(&content.core).unwrap();
        let before_runtime = encode_canonical_runtime_content_v1(&content.runtime_content).unwrap();
        let before_digest = calculate_revision_content_digest_v1(&content).unwrap();
        let loaded = canonical.load_revision(&identity).unwrap().unwrap();
        assert_eq!(
            encode_canonical_revision_core_v1(&loaded.content.core).unwrap(),
            before_core
        );
        assert_eq!(
            encode_canonical_runtime_content_v1(&loaded.content.runtime_content).unwrap(),
            before_runtime
        );
        assert_eq!(
            calculate_revision_content_digest_v1(&loaded.content).unwrap(),
            before_digest
        );

        for corruption in ["canonical", "references"] {
            let (_temporary, root) = test_root();
            let persistence = PactrunPersistence::open(&root).unwrap();
            let (_, _, identity) = persist_sample(&persistence, package(32), corruption.as_bytes());
            let database = persistence.database.lock().unwrap();
            if corruption == "canonical" {
                database
                    .execute(
                        "UPDATE revisions SET core_jcs=?3 \
                         WHERE package_id=?1 AND revision_content_digest=?2",
                        params![
                            identity.package_id.as_bytes().as_slice(),
                            identity.content_digest.as_bytes().as_slice(),
                            b"{}",
                        ],
                    )
                    .unwrap();
            } else {
                database
                    .execute(
                        "UPDATE revision_runtime_content_refs SET blob_digest=?3 \
                         WHERE package_id=?1 AND revision_content_digest=?2",
                        params![
                            identity.package_id.as_bytes().as_slice(),
                            identity.content_digest.as_bytes().as_slice(),
                            [0x77_u8; 32],
                        ],
                    )
                    .unwrap();
            }
            drop(database);
            assert!(persistence.load_revision(&identity).is_err());
        }

        let vectors: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/vectors/revision_core_format_v1/vectors.json"
        ))
        .unwrap();
        let catalog: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/vectors/error_taxonomy_v1/catalog.json"
        ))
        .unwrap();
        assert_eq!(vectors["status"], "frozen");
        assert_eq!(catalog["status"], "frozen");
    }

    // Test-ID: PR-TEST-0082
    // Verifies: PR-REQ-0078, PR-REQ-0275, PR-REQ-0276
    #[test]
    fn exact_v3_migrates_to_v4_preserving_instances_with_crash_and_concurrency() {
        fn v3_sql() -> String {
            format!("{SCHEMA_V1_SQL}\n{SCHEMA_V2_ADDITIONS_SQL}\n{SCHEMA_V3_ADDITIONS_SQL}")
        }
        fn seed_v3(root: &Path) {
            initialize_direct(root, APPLICATION_ID, SCHEMA_V3_VERSION, &v3_sql());
            let database = raw_database(root);
            database
                .execute_batch(
                    "INSERT INTO packages(package_id) VALUES (x'01010101010101010101010101010101');\
                     INSERT INTO revisions(package_id, revision_content_digest, core_jcs, runtime_content_jcs) \
                       VALUES (x'01010101010101010101010101010101', \
                               x'0202020202020202020202020202020202020202020202020202020202020202', \
                               x'7b7d', x'7b7d');\
                     INSERT INTO instances(instance_id, instance_name, active_package_id, \
                                           active_revision_content_digest, instance_state_version) \
                       VALUES (x'03030303030303030303030303030303', CAST('node' AS BLOB), \
                               x'01010101010101010101010101010101', \
                               x'0202020202020202020202020202020202020202020202020202020202020202', \
                               x'04040404040404040404040404040404');\
                     INSERT INTO managed_input_payloads(instance_id, payload_id, protection_rank, byte_length) \
                       VALUES (x'03030303030303030303030303030303', x'05050505050505050505050505050505', 1, 3);\
                     INSERT INTO managed_input_payload_chunks(instance_id, payload_id, chunk_index, chunk_bytes) \
                       VALUES (x'03030303030303030303030303030303', x'05050505050505050505050505050505', 0, x'616263');\
                     INSERT INTO managed_input_bindings(instance_id, input_identity, payload_id) \
                       VALUES (x'03030303030303030303030303030303', CAST('config' AS BLOB), \
                               x'05050505050505050505050505050505');",
                )
                .unwrap();
        }
        fn preserved_rows(database: &Connection) -> (i64, i64, i64, i64, Vec<u8>) {
            let counts = |table: &str| -> i64 {
                database
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })
                    .unwrap()
            };
            (
                counts("instances"),
                counts("managed_input_payloads"),
                counts("managed_input_payload_chunks"),
                counts("managed_input_bindings"),
                database
                    .query_row("SELECT instance_state_version FROM instances", [], |row| {
                        row.get(0)
                    })
                    .unwrap(),
            )
        }
        fn user_version(database: &Connection) -> i64 {
            database
                .pragma_query_value(None, "user_version", |row| row.get(0))
                .unwrap()
        }
        fn has_table(database: &Connection, table: &str) -> bool {
            database
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                == 1
        }

        // Exact V3 with real Instance rows migrates to exact V4 and preserves
        // every row without inferring any Run state.
        let (_temporary, root) = test_root();
        seed_v3(&root);
        let before = preserved_rows(&raw_database(&root));
        let persistence = PactrunPersistence::open(&root).unwrap();
        let database = persistence.database.lock().unwrap();
        assert_eq!(user_version(&database), SCHEMA_VERSION);
        validate_schema(&database, SCHEMA_VERSION).unwrap();
        assert_eq!(preserved_rows(&database), before);
        for table in [
            "runs",
            "run_executions",
            "run_revision_pins",
            "run_payload_pins",
            "run_outcomes",
            "run_artifacts",
            "instance_recovery_guards",
        ] {
            assert!(has_table(&database, table));
            assert_eq!(
                database
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        assert_eq!(
            database
                .query_row(
                    "SELECT count(*) FROM pragma_table_list \
                     WHERE schema = 'main' AND name NOT LIKE 'sqlite_%'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            35
        );
        drop(database);
        drop(persistence);

        // Crash before the commit leaves exact V3; crash after exposes exact V4.
        let (before_temporary, before_root) = test_root();
        seed_v3(&before_root);
        let before_marker = before_temporary.path().join("v4-before.success");
        assert!(!run_worker(
            &before_root,
            "open",
            Some(FaultPoint::BeforeSchemaMigrationCommit),
            &before_marker,
        ));
        assert!(!before_marker.exists());
        let database = raw_database(&before_root);
        assert_eq!(user_version(&database), SCHEMA_V3_VERSION);
        assert!(!has_table(&database, "runs"));
        validate_schema(&database, SCHEMA_V3_VERSION).unwrap();
        assert_eq!(preserved_rows(&database), before);
        drop(database);
        PactrunPersistence::open(&before_root).unwrap();

        let (after_temporary, after_root) = test_root();
        seed_v3(&after_root);
        let after_marker = after_temporary.path().join("v4-after.success");
        assert!(!run_worker(
            &after_root,
            "open",
            Some(FaultPoint::AfterSchemaMigrationCommit),
            &after_marker,
        ));
        let database = raw_database(&after_root);
        assert_eq!(user_version(&database), SCHEMA_VERSION);
        validate_schema(&database, SCHEMA_VERSION).unwrap();
        assert_eq!(preserved_rows(&database), before);
        drop(database);
        PactrunPersistence::open(&after_root).unwrap();

        // Pristine, exact V1, and exact V2 converge on exact V4 as well.
        let (_pristine_temporary, pristine_root) = test_root();
        let pristine = PactrunPersistence::open(&pristine_root).unwrap();
        assert_eq!(
            user_version(&pristine.database.lock().unwrap()),
            SCHEMA_VERSION
        );
        for (version, sql) in [
            (SCHEMA_V1_VERSION, SCHEMA_V1_SQL.to_owned()),
            (
                SCHEMA_V2_VERSION,
                format!("{SCHEMA_V1_SQL}\n{SCHEMA_V2_ADDITIONS_SQL}"),
            ),
        ] {
            let (_temporary, root) = test_root();
            initialize_direct(&root, APPLICATION_ID, version, &sql);
            let persistence = PactrunPersistence::open(&root).unwrap();
            let database = persistence.database.lock().unwrap();
            assert_eq!(user_version(&database), SCHEMA_VERSION);
            validate_schema(&database, SCHEMA_VERSION).unwrap();
        }

        // Concurrent migrators converge through SQLite locking alone.
        let (concurrent_temporary, concurrent_root) = test_root();
        seed_v3(&concurrent_root);
        let workers = (0..2)
            .map(|index| {
                let root = concurrent_root.clone();
                let marker = concurrent_temporary
                    .path()
                    .join(format!("migrate-{index}.success"));
                thread::spawn(move || (run_worker(&root, "open", None, &marker), marker))
            })
            .collect::<Vec<_>>();
        for worker in workers {
            let (success, marker) = worker.join().unwrap();
            assert!(success);
            assert!(marker.exists());
        }
        let database = raw_database(&concurrent_root);
        assert_eq!(user_version(&database), SCHEMA_VERSION);
        assert_eq!(preserved_rows(&database), before);
        drop(database);

        // A V4 marker over a drifted manifest and a V3 marker over a partial
        // V4 table set are both inadmissible.
        let (_drift_temporary, drift_root) = test_root();
        initialize_direct(
            &drift_root,
            APPLICATION_ID,
            SCHEMA_VERSION,
            &format!(
                "{}\n{SCHEMA_V4_ADDITIONS_SQL}\nCREATE TABLE drift(value TEXT) STRICT;",
                v3_sql()
            ),
        );
        assert!(matches!(
            PactrunPersistence::open(&drift_root),
            Err(PersistenceError::SchemaMismatch(_))
        ));
        let (_partial_temporary, partial_root) = test_root();
        initialize_direct(
            &partial_root,
            APPLICATION_ID,
            SCHEMA_V3_VERSION,
            &format!(
                "{}\n{}",
                v3_sql(),
                SCHEMA_V4_ADDITIONS_SQL
                    .split("CREATE TABLE run_action_invocations")
                    .next()
                    .unwrap()
            ),
        );
        assert!(matches!(
            PactrunPersistence::open(&partial_root),
            Err(PersistenceError::SchemaMismatch(_))
        ));
    }

    #[test]
    fn m1c_subprocess_worker() {
        let Some(operation) = std::env::var_os("PACTRUN_M1C_WORKER") else {
            return;
        };
        let root = PathBuf::from(std::env::var_os("PACTRUN_M1C_ROOT").unwrap());
        let marker = PathBuf::from(std::env::var_os("PACTRUN_M1C_SUCCESS_MARKER").unwrap());
        let persistence = PactrunPersistence::open(&root).unwrap();
        match operation.to_string_lossy().as_ref() {
            "open" => {}
            "persist" => {
                let bytes = b"m1c worker content";
                let blob_digest = digest(bytes);
                let publication = persistence
                    .put_runtime_content(&blob_digest, &mut Cursor::new(bytes))
                    .unwrap();
                let content = shared_content(&blob_digest);
                persistence
                    .persist_revision(package(42), &content, &[publication])
                    .unwrap();
            }
            other => panic!("unknown M1-C worker operation {other}"),
        }
        fs::write(marker, b"m1c-operation-returned-success").unwrap();
    }
}
