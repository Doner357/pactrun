//! Persistence boundary and storage-adapter ownership.

#![allow(dead_code)]

#[cfg(test)]
mod baseline_schema_tests;
mod chunked_blob;
mod immutable_data;
mod local_catalog;
#[cfg(test)]
mod object_lifecycle_tests;
mod pack_publication;
mod runtime_content_store;
#[cfg(test)]
mod schema_constraint_tests;
mod schema_upgrade;
mod store_bootstrap;
pub(crate) use store_bootstrap::prepare_new_store;
mod sqlite_catalog;
mod sqlite_core_diagnostics;
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
mod writer_admission;
pub(crate) use sqlite_snapshots::{
    SnapshotImportReceipt, SnapshotInspection, SnapshotVerification,
};

/// Orders an acceptance transaction against a foreground cancellation request.
/// Implementations run the supplied commit while holding the acceptance
/// decision, or return `None` so the caller can roll the transaction back.
pub(crate) trait AcceptanceArbiter {
    fn accepted(&self, _run: crate::domain::RunId) {}
    fn retain_hook_text(&self) -> bool {
        true
    }
    fn before_durable_acceptance(
        &self,
        commit: impl FnOnce() -> Result<(), PersistenceError>,
    ) -> AcceptanceCommitResult;
}

pub(crate) use sqlite_catalog::SnapshotCatalog;

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
