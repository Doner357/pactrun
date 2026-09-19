//! Non-authoritative diagnostic evidence, committed separately from Run results.
use super::{PactrunPersistence, PersistenceError};
pub(crate) use crate::domain::DiagnosticInspection;
use crate::domain::{
    DiagnosticKind, DiagnosticSeverity, DiagnosticWindow, HookEvidence, HookText, RunId,
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

impl PactrunPersistence {
    pub(crate) fn diagnostic_root(&self) -> std::path::PathBuf {
        self.database_path
            .parent()
            .expect("database directory")
            .parent()
            .expect("store root")
            .to_owned()
    }
    pub(crate) fn bound_diagnostic_wait(&self) -> Result<(), PersistenceError> {
        self.database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?
            .busy_timeout(std::time::Duration::from_millis(50))
            .map_err(|e| PersistenceError::sqlite("bound diagnostic wait", e))
    }
    pub(crate) fn save_diagnostics(
        &self,
        run: RunId,
        window: &DiagnosticWindow,
        retain: bool,
        closed: bool,
        failed: bool,
    ) -> Result<(), PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin diagnostics", e))?;
        self.check_write_admission(&tx)?;
        let committed: i64 = tx
            .query_row(
                "SELECT observed FROM run_diagnostic_collections WHERE run_id=?1",
                [run.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| PersistenceError::sqlite("read diagnostic cursor", e))?;
        // Never insert the collection here: a deleted Run cannot be resurrected.
        let changed = tx.execute("UPDATE run_diagnostic_collections SET retain_text=?2, started=1, closed=?3, observed=?4, failed=?5 WHERE run_id=?1", params![run.as_bytes().as_slice(), retain, closed, i64::try_from(window.observed).unwrap_or(i64::MAX), failed]).map_err(|e| PersistenceError::sqlite("update diagnostic collection", e))?;
        if changed != 1 {
            return Err(PersistenceError::SchemaMismatch(
                "diagnostic collection no longer exists".into(),
            ));
        }
        let kept = if retain {
            window
                .events()
                .iter()
                .map(|e| e.sequence.to_string())
                .collect::<Vec<_>>()
                .join(",")
        } else {
            String::new()
        };
        // The IN list contains only internally allocated integer sequences.
        let deletion = if kept.is_empty() {
            "DELETE FROM run_diagnostic_events WHERE run_id=?1".to_owned()
        } else {
            format!(
                "DELETE FROM run_diagnostic_events WHERE run_id=?1 AND sequence NOT IN ({kept})"
            )
        };
        tx.execute(&deletion, [run.as_bytes().as_slice()])
            .map_err(|e| PersistenceError::sqlite("trim diagnostic window", e))?;
        if retain {
            let mut insert = tx
                .prepare(
                    "INSERT INTO run_diagnostic_events VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                )
                .map_err(|e| PersistenceError::sqlite("prepare diagnostic events", e))?;
            for event in window
                .events()
                .into_iter()
                .filter(|e| e.sequence > committed as u64)
            {
                let kind = match event.text.kind {
                    DiagnosticKind::Diagnostic => 0,
                    DiagnosticKind::Completion => 1,
                    DiagnosticKind::ProtocolError => 2,
                };
                let severity = event.text.severity.map(|s| match s {
                    DiagnosticSeverity::Info => 0,
                    DiagnosticSeverity::Warning => 1,
                    DiagnosticSeverity::Error => 2,
                });
                insert
                    .execute(params![
                        run.as_bytes().as_slice(),
                        i64::try_from(event.sequence).unwrap_or(i64::MAX),
                        event
                            .received_at_unix_ms
                            .map(|time| i64::try_from(time).unwrap_or(i64::MAX)),
                        event.stage,
                        kind,
                        severity,
                        event.text.code,
                        event.text.message,
                        event.text.truncated,
                        event.text.truncated_prefix_bytes as i64,
                        event.text.completion_status.map(|status| status.rank())
                    ])
                    .map_err(|e| PersistenceError::sqlite("save diagnostic event", e))?;
            }
        }
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit diagnostic window", e))
    }
    pub(crate) fn inspect_diagnostics(
        &self,
        run: RunId,
    ) -> Result<Option<DiagnosticInspection>, PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction()
            .map_err(|e| PersistenceError::sqlite("begin diagnostic inspection", e))?;
        let value = load_diagnostics_from(&tx, run)?;
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("finish diagnostic inspection", e))?;
        Ok(value)
    }
}

#[cfg(test)]
#[path = "sqlite_diagnostics_tests.rs"]
mod tests;

pub(super) fn load_diagnostics_from(
    database: &rusqlite::Connection,
    run: RunId,
) -> Result<Option<DiagnosticInspection>, PersistenceError> {
    let mut value = database.query_row("SELECT retain_text,started,closed,observed,failed FROM run_diagnostic_collections WHERE run_id=?1", [run.as_bytes().as_slice()], |row| Ok(DiagnosticInspection { retain_text:row.get(0)?,started:row.get(1)?,closed:row.get(2)?,observed:row.get::<_,i64>(3)? as u64,failed:row.get(4)?,events:Vec::new() })).optional().map_err(|e| PersistenceError::sqlite("read diagnostic collection",e))?;
    if let Some(value) = &mut value {
        let mut stmt = database.prepare("SELECT sequence,received_at_unix_ms,stage,kind,severity,code,message,truncated,truncated_prefix_bytes,completion_status FROM run_diagnostic_events WHERE run_id=?1 ORDER BY sequence").map_err(|e| PersistenceError::sqlite("prepare diagnostic inspection",e))?;
        let rows = stmt
            .query_map([run.as_bytes().as_slice()], |row| {
                let kind: i64 = row.get(3)?;
                let severity: Option<i64> = row.get(4)?;
                Ok(HookEvidence {
                    sequence: row.get::<_, i64>(0)? as u64,
                    received_at_unix_ms: row.get::<_, Option<i64>>(1)?.map(|time| time as u64),
                    stage: row.get(2)?,
                    text: HookText {
                        kind: match kind {
                            0 => DiagnosticKind::Diagnostic,
                            1 => DiagnosticKind::Completion,
                            _ => DiagnosticKind::ProtocolError,
                        },
                        severity: severity.map(|s| match s {
                            0 => DiagnosticSeverity::Info,
                            1 => DiagnosticSeverity::Warning,
                            _ => DiagnosticSeverity::Error,
                        }),
                        code: row.get(5)?,
                        message: row.get(6)?,
                        truncated: row.get(7)?,
                        truncated_prefix_bytes: row.get::<_, i64>(8)? as usize,
                        completion_status: row.get::<_, Option<i64>>(9)?.map(|rank| {
                            if rank == 0 {
                                crate::domain::HookCompletionStatus::Success
                            } else {
                                crate::domain::HookCompletionStatus::Failure
                            }
                        }),
                    },
                })
            })
            .map_err(|e| PersistenceError::sqlite("query diagnostic events", e))?;
        for row in rows {
            value
                .events
                .push(row.map_err(|e| PersistenceError::sqlite("decode diagnostic event", e))?);
        }
    }
    Ok(value)
}

impl PersistenceError {
    pub(crate) fn diagnostic_contention(&self) -> bool {
        matches!(self, Self::Sqlite { source: rusqlite::Error::SqliteFailure(error,_), .. }
            if matches!(error.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked))
    }
}
