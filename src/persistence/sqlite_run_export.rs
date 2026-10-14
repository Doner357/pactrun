//! One Store read snapshot; the consumer owns only report encoding and output.
use super::{
    PactrunPersistence, PersistenceError, sqlite_input_history_export, sqlite_runs,
    sqlite_snapshots,
};
use crate::domain::*;
use rusqlite::TransactionBehavior;
use std::io::Write;

pub(crate) trait RunExportSink {
    fn inspection(&mut self, view: &ManagedRunInspectionData) -> Result<(), PersistenceError>;
    fn input(
        &mut self,
        context: &HistoricalInputContext,
        input: &HistoricalInput,
        copy: impl FnOnce(&mut dyn Write) -> Result<HistoricalInputExport, PersistenceError>,
    ) -> Result<(), PersistenceError>;
    fn artifact(
        &mut self,
        artifact: &RunArtifactSummary,
        copy: impl FnOnce(&mut dyn Write) -> Result<(), PersistenceError>,
    ) -> Result<(), PersistenceError>;
}

impl PactrunPersistence {
    pub(crate) fn write_run_export(
        &self,
        run: RunId,
        options: RunExportOptions,
        sink: &mut impl RunExportSink,
    ) -> Result<(), PersistenceError> {
        options
            .validate()
            .map_err(|message| PersistenceError::InvalidRunTransition(message.into()))?;
        let mut db = self.open_read_connection()?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("begin Run report read snapshot", e))?;
        let mut view =
            sqlite_runs::inspect_run_from(&tx, sqlite_runs::load_managed_run_from(&tx, run)?)?;
        if !options.hook_text
            && let Some(diagnostics) = &mut view.diagnostics
        {
            diagnostics.events.clear();
        }
        sink.inspection(&view)?;
        for context in &view.input_history {
            if !context
                .inputs
                .iter()
                .any(|input| options.includes(input.protection))
            {
                continue;
            }
            // Restore has one immutable manifest. Decode/index it once, not once per Input.
            let snapshot = if context.scope == InputHistoryScope::Snapshot
                && context.inputs.iter().any(|input| {
                    options.includes(input.protection)
                        && input.origin == InputHistoryOrigin::Snapshot
                }) {
                sqlite_snapshots::historical_source(&tx, context)?
            } else {
                None
            };
            for input in context
                .inputs
                .iter()
                .filter(|input| options.includes(input.protection))
            {
                sink.input(context, input, |mut destination| {
                    if input.origin == InputHistoryOrigin::Snapshot {
                        match &snapshot {
                            Some(snapshot) => snapshot.copy_input(
                                &tx,
                                &self.runtime_content,
                                input,
                                options.secrets && options.authorize_secret,
                                &mut destination,
                            ),
                            None => Ok(HistoricalInputExport::Unavailable),
                        }
                    } else {
                        sqlite_input_history_export::copy_from(
                            &tx,
                            &self.runtime_content,
                            view.run.instance,
                            context,
                            input,
                            options.secrets && options.authorize_secret,
                            &mut destination,
                        )
                    }
                })?;
            }
        }
        if options.artifacts {
            for artifact in &view.published_artifacts {
                sink.artifact(artifact, |mut destination| {
                    sqlite_runs::copy_artifact_from(
                        &tx,
                        run,
                        &artifact.output,
                        artifact.edge.as_ref(),
                        &mut destination,
                    )
                })?;
            }
        }
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("finish Run report read snapshot", e))?;
        Ok(())
    }
}
