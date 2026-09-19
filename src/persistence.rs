//! Persistence boundary and storage-adapter ownership.

// M1-B establishes the production storage contract before M1-C adds its first
// application caller.
#![allow(dead_code)]

mod chunked_blob;
mod immutable_data;
#[cfg(test)]
mod object_lifecycle_tests;
mod runtime_content_store;
#[cfg(test)]
mod schema_v5_contract_tests;
mod sqlite_catalog;
mod sqlite_deletions;
mod sqlite_diagnostics;
pub(crate) use sqlite_diagnostics::DiagnosticInspection;
mod sqlite_instances;
mod sqlite_migration_runs;
mod sqlite_migrations;
mod sqlite_object_lifecycle;
mod sqlite_revision_metadata;
mod sqlite_revision_store;
mod sqlite_runs;
mod sqlite_service_admission;
mod sqlite_service_migrations;
mod sqlite_service_preparations;
mod sqlite_service_storage;
mod sqlite_service_targets;
mod sqlite_service_views;
mod sqlite_snapshots;
#[cfg(test)]
pub(crate) use sqlite_snapshots::corrupt_snapshot_for_test;
mod sqlite_v5;
#[cfg(test)]
mod sqlite_v6;
#[cfg(test)]
mod sqlite_v7;
#[cfg(test)]
mod sqlite_v8;
mod sqlite_v9;
pub(crate) use sqlite_revision_store::SCHEMA_VERSION;
pub(crate) use sqlite_snapshots::{
    SnapshotImportReceipt, SnapshotInspection, SnapshotVerification,
};
#[cfg(test)]
pub(crate) use sqlite_v9::current_to_v9_fixture;

/// Orders an acceptance transaction against a foreground cancellation request.
/// Implementations run the supplied commit while holding the acceptance
/// decision, or return `None` so the caller can roll the transaction back.
pub(crate) trait AcceptanceArbiter {
    fn retain_hook_text(&self) -> bool {
        true
    }
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
pub(crate) use sqlite_migration_runs::MigrationEdgePublication;
#[cfg(test)]
pub(crate) use sqlite_migration_runs::fail_next_edge_ack_for_test as fail_next_migration_edge_ack_for_test;
#[allow(unused_imports)]
pub(crate) use sqlite_revision_store::{
    FaultPoint, PactrunPersistence, PersistenceError, StoredRevisionContent, fault,
};
#[cfg(test)]
pub(crate) use sqlite_runs::fail_next_capture_publication_for_test;
#[allow(unused_imports)]
pub(crate) use sqlite_runs::{RunArtifactWrite, RunFinishReceipt, SnapshotBlobWrite};
