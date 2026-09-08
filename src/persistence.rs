//! Persistence boundary and storage-adapter ownership.

// M1-B establishes the production storage contract before M1-C adds its first
// application caller.
#![allow(dead_code)]

mod chunked_blob;
mod runtime_content_store;
mod sqlite_instances;
mod sqlite_revision_metadata;
mod sqlite_revision_store;
mod sqlite_runs;

/// Orders an acceptance transaction against a foreground cancellation request.
/// Implementations run the supplied commit while holding the acceptance
/// decision, or return `None` so the caller can roll the transaction back.
pub(crate) trait AcceptanceArbiter {
    fn before_durable_acceptance(
        &self,
        commit: impl FnOnce() -> Result<(), PersistenceError>,
    ) -> AcceptanceCommitResult;
}

pub(crate) struct UnconditionalAcceptance;

impl AcceptanceArbiter for UnconditionalAcceptance {
    fn before_durable_acceptance(
        &self,
        commit: impl FnOnce() -> Result<(), PersistenceError>,
    ) -> AcceptanceCommitResult {
        AcceptanceCommitResult::Committed(commit())
    }
}

pub(crate) enum AcceptanceCommitResult {
    Cancelled,
    Committed(Result<(), PersistenceError>),
    Uncertain(PersistenceError),
}

#[derive(Debug)]
pub(crate) enum AcceptanceError {
    Cancelled {
        run: crate::domain::RunId,
    },
    NotCommitted {
        run: crate::domain::RunId,
        source: PersistenceError,
    },
    Uncertain {
        run: crate::domain::RunId,
        source: PersistenceError,
    },
}

#[allow(unused_imports)]
pub(crate) use runtime_content_store::{
    RuntimeContentStore, RuntimeContentStoreError, StoredRuntimeBlob, VerifiedRuntimeBlob,
    validate_supported_storage_root,
};
#[allow(unused_imports)]
pub(crate) use sqlite_instances::ManagedInputWrite;
#[allow(unused_imports)]
pub(crate) use sqlite_revision_store::{
    FaultPoint, PactrunPersistence, PersistenceError, StoredRevisionContentV1, fault,
};
#[allow(unused_imports)]
pub(crate) use sqlite_runs::{RunArtifactWrite, RunFinishReceipt};
