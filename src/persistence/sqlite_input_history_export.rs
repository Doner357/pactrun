//! Resolve original references in one read snapshot; never inspect replacement bindings.
use super::{PactrunPersistence, PersistenceError, RuntimeContentStore};
use crate::domain::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::io::Write;

impl PactrunPersistence {
    /// Writes only to caller-owned staging. An error can leave partial bytes;
    /// callers must discard them rather than publish an incomplete report.
    pub(crate) fn export_historical_input(
        &self,
        run: RunId,
        selector: &HistoricalInputSelector,
        authorize_secret: bool,
        destination: &mut impl Write,
    ) -> Result<HistoricalInputExport, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("begin historical Input export", e))?;
        let view = super::sqlite_runs::load_managed_run_from(&tx, run)?;
        let contexts = super::sqlite_input_history::load(&tx, run)?;
        let Some(context) = contexts.iter().find(|context| {
            context.edge_index == selector.edge_index && context.scope == selector.scope
        }) else {
            return Ok(HistoricalInputExport::NotRecorded);
        };
        let Some(input) = context
            .inputs
            .iter()
            .find(|input| input.input == selector.input)
        else {
            return Ok(HistoricalInputExport::NotRecorded);
        };
        let result = copy_from(
            &tx,
            &self.runtime_content,
            view.instance,
            context,
            input,
            authorize_secret,
            destination,
        )?;
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("finish historical Input export", e))?;
        Ok(result)
    }
}

pub(super) fn copy_from(
    db: &Connection,
    content: &RuntimeContentStore,
    instance: InstanceId,
    context: &HistoricalInputContext,
    input: &HistoricalInput,
    authorize_secret: bool,
    destination: &mut impl Write,
) -> Result<HistoricalInputExport, PersistenceError> {
    use HistoricalInputExport as E;
    if input.protection == ManagedInputProtection::Secret && !authorize_secret {
        return Err(PersistenceError::UnauthorizedSecretExport);
    }
    match input.origin {
        InputHistoryOrigin::Absent => Ok(E::Absent),
        InputHistoryOrigin::Transient => Ok(E::Transient),
        InputHistoryOrigin::PendingOutput => Ok(E::PendingOutput),
        InputHistoryOrigin::Managed(payload) => {
            let stored: Option<i64> = db.query_row(
                "SELECT protection_rank FROM managed_input_payloads WHERE instance_id=?1 AND payload_id=?2",
                params![instance.as_bytes().as_slice(), payload.as_bytes().as_slice()],
                |r| r.get(0),
            ).optional().map_err(|e| PersistenceError::sqlite("resolve historical Input payload", e))?;
            let Some(stored) = stored else {
                return Ok(E::Unavailable);
            };
            let stored = ManagedInputProtection::from_rank(stored).map_err(|_| {
                PersistenceError::CorruptManagedInput(
                    "invalid historical payload protection".into(),
                )
            })?;
            let protection = input.protection.sticky(stored);
            if protection == ManagedInputProtection::Secret && !authorize_secret {
                return Err(PersistenceError::UnauthorizedSecretExport);
            }
            super::sqlite_instances::stream_payload(db, content, instance, payload, destination)?;
            Ok(E::Copied { protection })
        }
        InputHistoryOrigin::Snapshot => super::sqlite_snapshots::copy_historical_input(
            db,
            content,
            context,
            input,
            authorize_secret,
            destination,
        ),
    }
}
