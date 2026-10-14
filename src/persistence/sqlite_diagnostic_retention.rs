//! Inventory and bounded deletion of evidence only. No Run/state cleanup.
use super::{PactrunPersistence, PersistenceError};
use crate::domain::RunId;
use rusqlite::{Connection, TransactionBehavior, params};

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

#[derive(Clone, Debug, Eq, PartialEq)]
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

fn candidate_sql(expired: bool) -> String {
    // A nullable-parameter OR prevents a timestamp range search through both
    // UNION branches. Expiry always has a cutoff; capacity always binds NULL.
    let predicate = if expired {
        "received_at_unix_ms < ?2"
    } else {
        "?2 IS NULL"
    };
    format!(
        "SELECT channel,CASE WHEN length(run_id)=16 THEN run_id ELSE X'' END,sequence,received_at_unix_ms,bytes,\
         CASE WHEN run_id=?1 THEN 1 ELSE 0 END AS preferred FROM ({ROWS}) \
         WHERE {predicate} \
         ORDER BY preferred,received_at_unix_ms,channel,run_id,sequence LIMIT {PAGE_SIZE}"
    )
}

fn retention_candidates(
    database: &Connection,
    expired_before_ms: Option<u64>,
    preferred: Option<RunId>,
) -> Result<Vec<DiagnosticRetentionCandidate>, PersistenceError> {
    let cutoff = expired_before_ms
        .map(i64::try_from)
        .transpose()
        .map_err(|_| invalid())?;
    let mut query = database
        .prepare(&candidate_sql(cutoff.is_some()))
        .map_err(sql)?;
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
    Ok(candidates)
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
        let candidates = retention_candidates(&tx, expired_before_ms, preferred)?;
        tx.commit().map_err(sql)?;
        Ok(DiagnosticRetentionInventory {
            bytes: u64::try_from(bytes).map_err(|_| invalid())?,
            candidates,
        })
    }

    /// Expiry selects the same bounded keys without scanning unrelated capacity.
    /// Combined capacity is still checked after expiry under the evidence lock.
    pub(crate) fn expired_diagnostic_candidates(
        &self,
        cutoff: u64,
    ) -> Result<Vec<DiagnosticRetentionCandidate>, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db.transaction().map_err(sql)?;
        let candidates = retention_candidates(&tx, Some(cutoff), None)?;
        tx.commit().map_err(sql)?;
        Ok(candidates)
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

    fn query_fixture(count: u32) -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(crate::persistence::sqlite_revision_store::BASELINE_SQL)
            .unwrap();
        db.pragma_update(None, "foreign_keys", true).unwrap();
        // Query-adapter fixture only. Real admitted Store transactions and
        // lifecycle preservation remain covered by PR-TEST-0797/0798/0800.
        let tx = db.transaction().unwrap();
        tx.execute("INSERT INTO instance_history_identities VALUES(zeroblob(16),CAST('query-fixture' AS BLOB))", []).unwrap();
        let failure = serde_json::to_string(&crate::domain::HelperFailure {
            command: crate::domain::HelperCommand::Diagnostic,
            stage: crate::domain::HelperStage::ReadJson,
            reason: crate::domain::HelperReason::NotFound,
        })
        .unwrap();
        for n in 1..=count {
            let mut id = [0; 16];
            id[..4].copy_from_slice(&n.to_be_bytes());
            let at = (n % 7 != 0).then_some(i64::from(100 + n / 3));
            tx.execute(
                "INSERT INTO runs VALUES(?1,zeroblob(16),zeroblob(16),0)",
                [id.as_slice()],
            )
            .unwrap();
            if n % 3 == 0 {
                tx.execute(
                    "INSERT INTO run_core_diagnostic_collections VALUES(?1,1,1,1,0)",
                    [id.as_slice()],
                )
                .unwrap();
                tx.execute(
                    "INSERT INTO run_core_diagnostic_events VALUES(?1,1,?2,'action',?3)",
                    params![id.as_slice(), at, &failure],
                )
                .unwrap();
            } else {
                tx.execute(
                    "INSERT INTO run_diagnostic_collections VALUES(?1,1,1,1,1,0)",
                    [id.as_slice()],
                )
                .unwrap();
                tx.execute("INSERT INTO run_diagnostic_events VALUES(?1,1,?2,'action',0,0,NULL,'retained',0,0,NULL)", params![id.as_slice(), at]).unwrap();
            }
        }
        tx.commit().unwrap();
        db
    }

    fn legacy_candidate_sql() -> String {
        format!(
            "SELECT channel,CASE WHEN length(run_id)=16 THEN run_id ELSE X'' END,sequence,received_at_unix_ms,bytes,CASE WHEN run_id=?1 THEN 1 ELSE 0 END AS preferred FROM ({ROWS}) WHERE (?2 IS NULL OR received_at_unix_ms < ?2) ORDER BY preferred,received_at_unix_ms,channel,run_id,sequence LIMIT {PAGE_SIZE}"
        )
    }

    fn query_rows(
        db: &Connection,
        query: &str,
        cutoff: Option<i64>,
        preferred: Option<&[u8]>,
    ) -> (Vec<Vec<rusqlite::types::Value>>, i32) {
        let mut statement = db.prepare(query).unwrap();
        let rows = statement
            .query_map(params![preferred, cutoff], |row| {
                (0..6)
                    .map(|column| row.get(column))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        (
            rows,
            statement.get_status(rusqlite::StatementStatus::VmStep),
        )
    }

    // Test-ID: PR-TEST-0807
    // Verifies: PR-REQ-0398
    #[test]
    fn expiry_query_preserves_legacy_order_bounds_unknown_times_and_uses_range_search() {
        let db = query_fixture(4096);
        let mut preferred = [0; 16];
        preferred[..4].copy_from_slice(&1u32.to_be_bytes());
        let legacy = legacy_candidate_sql();
        for cutoff in [None, Some(0), Some(101), Some(120), Some(200), Some(10000)] {
            for preferred in [None, Some(preferred.as_slice())] {
                let old = query_rows(&db, &legacy, cutoff, preferred);
                let new = query_rows(&db, &candidate_sql(cutoff.is_some()), cutoff, preferred);
                assert_eq!(new.0, old.0);
                assert!(new.0.len() <= PAGE_SIZE);
                if cutoff == Some(0) {
                    assert!(new.1 < old.1, "expiry lost its indexed range search");
                }
            }
        }
        assert!(retention_candidates(&db, Some(u64::MAX), None).is_err());
        // Exhaust more than one batch; unknown timestamps must remain untouched.
        loop {
            let old = query_rows(&db, &legacy, Some(10000), None).0;
            let new = query_rows(&db, &candidate_sql(true), Some(10000), None).0;
            assert_eq!(old, new);
            if new.is_empty() {
                break;
            }
            for row in new {
                let table = if row[0] == rusqlite::types::Value::Integer(0) {
                    "run_diagnostic_events"
                } else {
                    "run_core_diagnostic_events"
                };
                db.execute(
                    &format!("DELETE FROM {table} WHERE run_id=?1 AND sequence=?2"),
                    params![&row[1], &row[2]],
                )
                .unwrap();
            }
        }
        let remaining = query_rows(&db, &candidate_sql(false), None, None).0;
        assert_eq!(remaining.len(), PAGE_SIZE);
        assert!(
            remaining
                .iter()
                .all(|row| row[3] == rusqlite::types::Value::Null)
        );
    }

    // Supporting measurements for PR-TEST-0807, not a machine-dependent gate.
    #[test]
    #[ignore = "bounded query benchmark; invoke explicitly and retain exact-source evidence"]
    fn diagnostic_expiry_query_timing_probe() {
        for count in [128, 4096, 16384] {
            let db = query_fixture(count);
            let legacy = legacy_candidate_sql();
            let indexed = candidate_sql(true);
            let old = query_rows(&db, &legacy, Some(50), None);
            let new = query_rows(&db, &indexed, Some(50), None);
            assert_eq!(old.0, new.0);
            for sample in 0..3 {
                let started = std::time::Instant::now();
                for _ in 0..100 {
                    assert!(query_rows(&db, &legacy, Some(50), None).0.is_empty());
                }
                let legacy_us = started.elapsed().as_micros();
                let started = std::time::Instant::now();
                for _ in 0..100 {
                    assert!(query_rows(&db, &indexed, Some(50), None).0.is_empty());
                }
                eprintln!(
                    "RETENTION_QUERY_SAMPLE {}",
                    serde_json::json!({
                        "rows": count, "iterations": 100, "sample": sample,
                        "legacy_us": legacy_us.to_string(), "indexed_us": started.elapsed().as_micros().to_string(),
                        "legacy_vm_steps": old.1, "indexed_vm_steps": new.1, "sqlite": rusqlite::version(),
                    })
                );
            }
        }
    }

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
