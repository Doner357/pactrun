//! Inventory and bounded deletion of evidence only. No Run/state cleanup.
use super::{PactrunPersistence, PersistenceError};
use crate::domain::RunId;
use rusqlite::{TransactionBehavior, params};

const PAGE_SIZE: usize = 64;
// Charge a closed JSON array of the stored scalar fields, including the Run key.
// This is private retention accounting, not a new export or persistence format.
const ROWS: &str = "SELECT 0 AS channel,run_id,sequence,received_at_unix_ms,\
 length(CAST(json_array(hex(run_id),sequence,received_at_unix_ms,stage,kind,severity,\
 code,message,truncated,truncated_prefix_bytes,completion_status) AS BLOB)) AS bytes \
 FROM run_diagnostic_events INDEXED BY run_diagnostic_retention UNION ALL \
 SELECT 1,run_id,sequence,received_at_unix_ms,\
 length(CAST(json_array(hex(run_id),sequence,received_at_unix_ms,stage,failure) AS BLOB)) \
 FROM run_core_diagnostic_events INDEXED BY run_core_diagnostic_retention";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Channel {
    Hook,
    Helper,
}

#[derive(Clone, Debug)]
pub(crate) struct DiagnosticRetentionCandidate {
    channel: Channel,
    run: RunId,
    sequence: i64,
    pub(crate) received_at_ms: Option<u64>,
    pub(crate) bytes: u64,
    pub(crate) preferred: bool,
}
pub(crate) struct DiagnosticRetentionInventory {
    pub(crate) bytes: u64,
    pub(crate) candidates: Vec<DiagnosticRetentionCandidate>,
}

impl DiagnosticRetentionCandidate {
    pub(crate) fn key(&self) -> ([u8; 16], u64, u8) {
        (
            *self.run.as_bytes(),
            self.sequence as u64,
            match self.channel {
                Channel::Hook => 0,
                Channel::Helper => 1,
            },
        )
    }
}

fn sql(error: rusqlite::Error) -> PersistenceError {
    PersistenceError::sqlite("diagnostic retention", error)
}
fn invalid() -> PersistenceError {
    PersistenceError::CorruptRun("invalid diagnostic retention record".into())
}

impl PactrunPersistence {
    /// Reads through a separate snapshot, without a management write transaction.
    /// Unknown timestamps are capacity candidates, never invented expiry dates.
    pub(crate) fn diagnostic_retention_inventory(
        &self,
        expired_before_ms: Option<u64>,
        preferred: Option<RunId>,
    ) -> Result<DiagnosticRetentionInventory, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        let bytes: i64 = tx
            .query_row(
                &format!("SELECT COALESCE(SUM(bytes),0) FROM ({ROWS})"),
                [],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let cutoff = expired_before_ms
            .map(i64::try_from)
            .transpose()
            .map_err(|_| invalid())?;
        let mut query = tx.prepare(&format!(
            "SELECT channel,CASE WHEN length(run_id)=16 THEN run_id ELSE X'' END,sequence,received_at_unix_ms,bytes,\
             CASE WHEN run_id=?1 THEN 1 ELSE 0 END AS preferred FROM ({ROWS}) \
             WHERE (?2 IS NULL OR received_at_unix_ms < ?2) \
             ORDER BY preferred,received_at_unix_ms,channel,run_id,sequence LIMIT {PAGE_SIZE}"
        )).map_err(sql)?;
        let rows = query
            .query_map(
                params![
                    preferred.as_ref().map(|run| run.as_bytes().as_slice()),
                    cutoff
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, bool>(5)?,
                    ))
                },
            )
            .map_err(sql)?;
        let mut candidates = Vec::new();
        for row in rows {
            let (channel, run, sequence, at, bytes, preferred) = row.map_err(sql)?;
            if sequence <= 0 || bytes <= 0 {
                return Err(invalid());
            }
            candidates.push(DiagnosticRetentionCandidate {
                channel: match channel {
                    0 => Channel::Hook,
                    1 => Channel::Helper,
                    _ => return Err(invalid()),
                },
                run: RunId::from_bytes(run.try_into().map_err(|_| invalid())?),
                sequence,
                received_at_ms: at.map(u64::try_from).transpose().map_err(|_| invalid())?,
                bytes: bytes as u64,
                preferred,
            });
        }
        drop(query);
        tx.commit().map_err(sql)?;
        Ok(DiagnosticRetentionInventory {
            bytes: u64::try_from(bytes).map_err(|_| invalid())?,
            candidates,
        })
    }

    /// A retention coordinator serializes evidence writers before selecting keys.
    /// Primary Run deletion may remove a selected key first; it is never recreated.
    pub(crate) fn delete_diagnostic_retention_candidates(
        &self,
        candidates: &[DiagnosticRetentionCandidate],
    ) -> Result<(), PersistenceError> {
        if candidates.len() > PAGE_SIZE {
            return Err(invalid());
        }
        if candidates.is_empty() {
            return Ok(());
        }
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql)?;
        self.check_write_admission(&tx)?;
        for candidate in candidates {
            let statement = match candidate.channel {
                Channel::Hook => {
                    "DELETE FROM run_diagnostic_events WHERE run_id=?1 AND sequence=?2"
                }
                Channel::Helper => {
                    "DELETE FROM run_core_diagnostic_events WHERE run_id=?1 AND sequence=?2"
                }
            };
            tx.execute(
                statement,
                params![candidate.run.as_bytes().as_slice(), candidate.sequence],
            )
            .map_err(sql)?;
        }
        tx.commit().map_err(sql)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0803
    // Verifies: PR-REQ-0398
    #[test]
    fn diagnostic_inventory_does_not_reserialize_retained_text_for_each_pass() {
        let database = rusqlite::Connection::open_in_memory().unwrap();
        database
            .execute_batch(crate::persistence::sqlite_revision_store::BASELINE_SQL)
            .unwrap();
        let query = format!("EXPLAIN SELECT COALESCE(SUM(bytes),0) FROM ({ROWS})");
        let mut statement = database.prepare(&query).unwrap();
        let functions: Vec<_> = statement
            .query_map([], |row| row.get::<_, Option<String>>(5))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            functions
                .iter()
                .flatten()
                .all(|value| !value.starts_with("json_array(")),
            "retention accounting reserializes the entire retained text history instead of reading cached charges"
        );
    }
}
