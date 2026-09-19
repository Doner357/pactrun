//! Associate only Pactrun-owned execution identities, never session paths.
use super::{
    ActionCancellation, materialize::MaterializedAction, runtime::RecoveryRiskPersistence,
};
use crate::domain::{RunId, TerminalContractV1};
pub(super) fn diagnostic_scope(
    store: &dyn RecoveryRiskPersistence,
    run: RunId,
    materialized: &MaterializedAction,
    cancellation: &ActionCancellation,
) -> Option<super::diagnostics::DiagnosticScope> {
    let root = store.diagnostic_root()?;
    let session = materialized.session();
    let mut stage = format!("{:?}", materialized.operation());
    if let Some(revision) = session.get("revision") {
        stage.push_str(&format!(" target_revision={revision}"));
    }
    if let Some(operation) = session.get("operation") {
        for key in ["source_revision", "target_revision"] {
            if let Some(revision) = operation.get(key) {
                stage.push_str(&format!(" {key}={revision}"));
            }
        }
    }
    Some(cancellation.diagnostics.scope(
        root,
        run,
        stage,
        materialized.terminal() == TerminalContractV1::Interactive,
    ))
}
