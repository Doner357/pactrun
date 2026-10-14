//! One Store evidence-writer lock and bounded trace-side retention operations.
use super::*;
use std::{io::Write, time::Instant};

const LOCK_HEADER: &[u8] = b"pactrun.diagnostic-retention\0\x01";

pub(crate) struct RetentionGuard {
    _file: File,
}
impl RetentionGuard {
    pub(crate) fn acquire(root: &Path) -> Result<Self, Error> {
        let directory_path = root.join("database");
        directory(&directory_path, false)?;
        let path = directory_path.join("diagnostic-retention.lock");
        let (mut guard, created) = match file(&path, true) {
            Ok(guard) => (guard, true),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                (file(&path, false).map_err(io_error)?, false)
            }
            Err(e) => return Err(io_error(e)),
        };
        qualify_file(&guard)?;
        let deadline = Instant::now() + Duration::from_millis(20);
        loop {
            match guard.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(std::fs::TryLockError::WouldBlock) => return Err(Error::Busy),
                Err(std::fs::TryLockError::Error(e)) => return Err(io_error(e)),
            }
        }
        if created {
            guard.write_all(LOCK_HEADER).map_err(io_error)?;
            guard.sync_all().map_err(io_error)?;
        } else {
            if guard.metadata().map_err(io_error)?.len() != LOCK_HEADER.len() as u64 {
                return Err(Error::ForeignJournal);
            }
            let mut bytes = [0; LOCK_HEADER.len()];
            guard.read_exact(&mut bytes).map_err(io_error)?;
            if bytes != LOCK_HEADER {
                return Err(Error::ForeignJournal);
            }
        }
        Ok(Self { _file: guard })
    }
}

#[derive(Clone)]
pub(crate) struct TraceRetentionCandidate {
    id: TraceId,
    pub(crate) updated_at_ms: u64,
    pub(crate) bytes: u64,
    pub(crate) preferred: bool,
}
impl TraceRetentionCandidate {
    pub(crate) fn key(&self) -> [u8; 16] {
        *self.id.as_bytes()
    }
}
impl Journal {
    pub(crate) fn mark_retention(
        &mut self,
        id: TraceId,
        status: crate::domain::TraceRetention,
    ) -> Result<(), Error> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let row = tx.query_row("SELECT length(document),CASE WHEN length(document)<=524288 THEN document ELSE X'' END FROM records WHERE id=?1", [id.as_bytes().as_slice()], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,Vec<u8>>(1)?))).optional().map_err(sql_error)?;
        let Some((size, bytes)) = row else {
            return Ok(());
        };
        let mut document = decode(size, &bytes)?;
        if document.id != id {
            return Err(Error::InvalidRecord);
        }
        document.retention = status;
        let bytes = serde_json::to_vec(&document).map_err(|_| Error::InvalidRecord)?;
        // A pending marker reserves the longest status spelling; this update
        // cannot invalidate a successfully enforced combined byte budget.
        if bytes.len() > size as usize {
            return Err(Error::InvalidRecord);
        }
        tx.execute(
            "UPDATE records SET document=?2 WHERE id=?1",
            params![id.as_bytes().as_slice(), bytes],
        )
        .map_err(sql_error)?;
        tx.commit().map_err(sql_error)
    }
    pub(crate) fn retention_inventory(
        &self,
        expired_before_ms: Option<u64>,
        preferred: Option<TraceId>,
    ) -> Result<(u64, Vec<TraceRetentionCandidate>), Error> {
        let cutoff = expired_before_ms
            .map(i64::try_from)
            .transpose()
            .map_err(|_| Error::InvalidRecord)?;
        let total: i64 = self
            .connection
            .query_row(
                "SELECT COALESCE(SUM(length(document)),0) FROM records",
                [],
                |r| r.get(0),
            )
            .map_err(sql_error)?;
        let mut query = self.connection.prepare("SELECT CASE WHEN length(id)=16 THEN id ELSE X'' END,updated_at_ms,length(document),CASE WHEN id=?1 THEN 1 ELSE 0 END AS preferred FROM records WHERE (?2 IS NULL OR updated_at_ms < ?2) ORDER BY preferred,updated_at_ms,id LIMIT 64").map_err(sql_error)?;
        let rows = query
            .query_map(
                params![
                    preferred.as_ref().map(|id| id.as_bytes().as_slice()),
                    cutoff
                ],
                |r| {
                    Ok((
                        r.get::<_, Vec<u8>>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, bool>(3)?,
                    ))
                },
            )
            .map_err(sql_error)?;
        let mut candidates = Vec::new();
        for row in rows {
            let (id, at, bytes, preferred) = row.map_err(sql_error)?;
            if bytes <= 0 || bytes as u64 > TRACE_DOCUMENT_LIMIT as u64 {
                return Err(Error::InvalidRecord);
            }
            candidates.push(TraceRetentionCandidate {
                id: TraceId::from_bytes(id.try_into().map_err(|_| Error::InvalidRecord)?),
                updated_at_ms: u64::try_from(at).map_err(|_| Error::InvalidRecord)?,
                bytes: bytes as u64,
                preferred,
            });
        }
        Ok((
            u64::try_from(total).map_err(|_| Error::InvalidRecord)?,
            candidates,
        ))
    }
    pub(crate) fn delete_retention_candidates(
        &mut self,
        candidates: &[TraceRetentionCandidate],
    ) -> Result<(), Error> {
        if candidates.len() > 64 {
            return Err(Error::InvalidRecord);
        }
        if candidates.is_empty() {
            return Ok(());
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        for candidate in candidates {
            tx.execute(
                "DELETE FROM records WHERE id=?1",
                [candidate.id.as_bytes().as_slice()],
            )
            .map_err(sql_error)?;
        }
        tx.commit().map_err(sql_error)
    }
}
