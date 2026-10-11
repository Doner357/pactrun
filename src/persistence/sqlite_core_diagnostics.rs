//! Bounded safe Core facts; their storage does not decide an execution outcome.
use super::{PactrunPersistence, PersistenceError};
use crate::domain::{CoreDiagnosticInspection, CoreDiagnosticWindow, CoreEvidence, RunId};
use rusqlite::{OptionalExtension, params};

fn sql(error: rusqlite::Error) -> PersistenceError {
    PersistenceError::sqlite("Core diagnostics", error)
}
impl PactrunPersistence {
    pub(crate) fn save_core_diagnostics(
        &self,
        run: RunId,
        window: &CoreDiagnosticWindow,
        closing: bool,
        failed: bool,
    ) -> Result<(), PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql)?;
        self.check_write_admission(&tx)?;
        let committed = tx
            .query_row(
                "SELECT observed FROM run_core_diagnostic_collections WHERE run_id=?1",
                [run.as_bytes().as_slice()],
                |r| r.get::<_, i64>(0),
            )
            .map_err(sql)? as u64;
        tx.execute("UPDATE run_core_diagnostic_collections SET started=1,observed=?2,closed=?3,failed=?4 WHERE run_id=?1",
            params![run.as_bytes().as_slice(), i64::try_from(window.observed).unwrap_or(i64::MAX), closing && !window.incomplete && !failed, failed]).map_err(sql)?;
        let ids: Vec<_> = window.events().map(|e| e.sequence.to_string()).collect();
        if ids.is_empty() {
            tx.execute(
                "DELETE FROM run_core_diagnostic_events WHERE run_id=?1",
                [run.as_bytes().as_slice()],
            )
            .map_err(sql)?;
        } else {
            tx.execute(&format!("DELETE FROM run_core_diagnostic_events WHERE run_id=?1 AND sequence NOT IN ({})", ids.join(",")), [run.as_bytes().as_slice()]).map_err(sql)?;
        }
        for e in window.events().filter(|e| e.sequence > committed) {
            let failure = serde_json::to_string(&e.failure).expect("typed safe helper failure");
            tx.execute(
                "INSERT OR IGNORE INTO run_core_diagnostic_events VALUES (?1,?2,?3,?4,?5)",
                params![
                    run.as_bytes().as_slice(),
                    i64::try_from(e.sequence).unwrap_or(i64::MAX),
                    e.received_at_unix_ms
                        .map(|v| i64::try_from(v).unwrap_or(i64::MAX)),
                    e.stage,
                    failure
                ],
            )
            .map_err(sql)?;
        }
        tx.commit().map_err(sql)
    }
}
pub(super) fn load(
    database: &rusqlite::Connection,
    run: RunId,
) -> Result<Option<CoreDiagnosticInspection>, PersistenceError> {
    let mut value = database
        .query_row(
            "SELECT observed,closed,failed,started FROM run_core_diagnostic_collections WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |r| {
                Ok(CoreDiagnosticInspection {
                    started: r.get(3)?,
                    observed: r.get::<_, i64>(0)? as u64,
                    collection_closed: r.get(1)?,
                    persistence_failed: r.get(2)?,
                    events: Vec::new(),
                })
            },
        )
        .optional()
        .map_err(sql)?;
    if let Some(value) = &mut value {
        let mut statement = database.prepare("SELECT sequence,received_at_unix_ms,stage,failure FROM run_core_diagnostic_events WHERE run_id=?1 ORDER BY sequence").map_err(sql)?;
        let rows = statement
            .query_map([run.as_bytes().as_slice()], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(sql)?;
        for row in rows {
            let (sequence, time, stage, failure) = row.map_err(sql)?;
            let failure = serde_json::from_str(&failure).map_err(|_| {
                PersistenceError::SchemaMismatch("invalid Core diagnostic fact".into())
            })?;
            value.events.push(CoreEvidence {
                sequence: sequence as u64,
                received_at_unix_ms: time.map(|t| t as u64),
                stage,
                failure,
            });
        }
    }
    Ok(value)
}
