//! Managed Run, execution pin, recovery, and retained Action Artifact repository.
//!
//! Every mutation is one `BEGIN IMMEDIATE` transaction. Run creation, pin
//! establishment, risk transitions, and a terminal publication without an
//! Instance consequence never publish a new `InstanceStateVersion`; a
//! `ManualRecoveryRequired` guard and `ResolveManualRecovery` always do.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

#[cfg(test)]
thread_local! {static FAIL_CAPTURE_PUBLICATION:std::cell::Cell<bool>=const {std::cell::Cell::new(false)};}
#[cfg(test)]
thread_local! {static FAIL_RESTORE_PUBLICATION:std::cell::Cell<bool>=const {std::cell::Cell::new(false)};}
#[cfg(test)]
pub(crate) fn fail_next_capture_publication_for_test() {
    FAIL_CAPTURE_PUBLICATION.with(|v| v.set(true));
}

use super::{
    AcceptanceArbiter, AcceptanceCommitResult, AcceptanceError, PactrunPersistence,
    PersistenceError, VerifiedRuntimeBlob,
    chunked_blob::{ChunkedBlobTable, insert_chunks, stream_chunks},
    runtime_content_store::{RuntimeContentStore, RuntimeContentStoreError},
    sqlite_instances::{
        binding_payload, ensure_ordered, fresh_state_version, instance_header, instance_id,
        load_revision_core, payload_id, reclaim_payload_if_unreferenced, revision_identity,
        state_version, stream_payload, update_state_version,
    },
    sqlite_revision_store::{FaultPoint, fault},
};
use crate::domain::{
    ActionIdentity, ActionRunBoundary, ActionRunIdentity, AdmissionFacts, AdmissionRefusal,
    CompiledHookLaunch, ExecutionOwnerSession, HookCodeV1, HookCompletionRecord,
    HookCompletionStatus, InputIdentity, InstanceId, InstanceStateVersion,
    InterpreterLauncherObservation, ManagedInputPayloadId, ManagedOutputIdentity,
    ManagedRunIdentity, ManualRecoveryTrigger, OperationAccessV1, PactrunErrorRef,
    RUN_ARTIFACT_MAX_BYTES_V1, RecoveryGuardView, RecoveryRiskState, RevisionIdentity,
    RunArtifactSummary, RunExecutionView, RunFailedStep, RunFailureRecord, RunFinish, RunId,
    RunInspectionData, RunOutcome, RunOutcomeView, RunPrimaryFailure, RunState, RunSummary,
    RunView, RuntimeFileV1, Sha256Digest, risk_transition, terminal_consequence,
    validate_running_action_state,
};

/// Re-runs the ordered interpreter launcher selection bound into a Plan. The
/// closure returns `Err(reason)` when the selection no longer yields the exact
/// bound path; the reason is diagnostic text only.
pub(crate) type LauncherCheck<'a> =
    dyn Fn(&InterpreterLauncherObservation) -> Result<(), String> + 'a;

pub(crate) struct RunArtifactWrite<'a> {
    pub(crate) output: ManagedOutputIdentity,
    pub(crate) byte_len: u64,
    pub(crate) reader: &'a mut dyn Read,
}

pub(crate) struct RunArtifactSubmission<'a, 'b> {
    pub(crate) submitted: &'a [ManagedOutputIdentity],
    pub(crate) artifacts: &'a mut [RunArtifactWrite<'b>],
}

pub(crate) struct SnapshotBlobWrite<'a> {
    pub(crate) digest: Sha256Digest,
    pub(crate) byte_len: u64,
    pub(crate) reader: &'a mut dyn Read,
}

struct CapturePublication<'a> {
    manifest: &'a crate::snapshot_integrity::VerifiedSnapshotManifest,
    blobs: &'a mut [SnapshotBlobWrite<'a>],
    files: Vec<super::StoredRuntimeBlob>,
    content: &'a RuntimeContentStore,
}

struct FinishAuthority<'a, 'o> {
    owner: Option<&'a ExecutionOwnerSession>,
    outputs: Option<&'o [ManagedOutputIdentity]>,
    capture: Option<CapturePublication<'a>>,
    restore: bool,
    migration: bool,
    deletion: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RunFinishReceipt {
    pub(crate) published_state_version: Option<InstanceStateVersion>,
}

const ARTIFACT_CHUNKS: ChunkedBlobTable = ChunkedBlobTable {
    representation_maximum: RUN_ARTIFACT_MAX_BYTES_V1,
    insert_chunk_sql: "INSERT INTO run_artifact_chunks(\
        run_id, output_identity, chunk_index, chunk_bytes\
     ) VALUES (?1, ?2, ?3, ?4)",
    select_chunks_sql: "SELECT chunk_index, chunk_bytes FROM run_artifact_chunks \
         WHERE run_id=?1 AND output_identity=?2 ORDER BY chunk_index",
    read_operation: "read Run Artifact staging",
    write_operation: "write Run Artifact chunk",
    invalid: PersistenceError::InvalidRunArtifact,
    corrupt: PersistenceError::CorruptRun,
};

struct RunHeader {
    instance: InstanceId,
    accepted_state_version: InstanceStateVersion,
    accepted_at_unix_ms: u64,
}

struct OutcomeRow {
    outcome: RunOutcome,
    boundary: ActionRunBoundary,
    terminal_risk: RecoveryRiskState,
    finished_at_unix_ms: u64,
}

mod admission;
mod artifacts;
mod pins;
mod publication;
mod queries;
mod records;
mod recovery;
mod scoped_artifacts;
mod terminal;

pub(super) use admission::managed_mutation_conflict;
pub(super) use artifacts::copy_artifact_from;
use artifacts::insert_artifact;
use publication::validate_capture_publication;
use records::{
    execution_row, guard_row, has_revision_pin, insert_managed_invocation, load_run_from,
    managed_invocation_row, outcome_row, pinned_payloads, run_header, run_id, unix_ms_now,
    utf8_message,
};
pub(super) use records::{
    inspect_run_from, load_instance_recovery_guard_from, load_managed_run_from,
};
pub(super) use scoped_artifacts::insert_scoped_artifacts;
pub(super) use terminal::{
    finish_deletion_in_transaction, finish_deletion_refusal, finish_migration_in_transaction,
};
use terminal::{finish_run_authorized, finish_run_in_transaction, finish_run_in_transaction_owned};

#[cfg(test)]
#[path = "sqlite_runs_tests.rs"]
mod tests;
