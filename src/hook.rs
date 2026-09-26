//! Managed Hook materialization, process supervision, and Frozen V1 runtime.

#![allow(dead_code)]

mod capture;
mod deletions;
pub(crate) mod delivery;
mod diagnostic_scope;
pub(crate) mod diagnostics;
mod executable;
mod materialize;
mod migrations;
mod platform;
mod protocol;
mod restore;
mod runtime;
pub(crate) mod service_storage;
pub(crate) mod shell_loader;
mod snapshots;
pub(crate) mod startup;
mod versioned_protocol;
pub(crate) use deletions::{accept_deletion, execute_ready as execute_ready_deletion};
pub(crate) use migrations::TargetCommitPermit;
#[cfg(test)]
pub(crate) use migrations::accept_declarative_migration;
#[cfg(test)]
pub(crate) use migrations::uncertain_next_migration_acceptance_for_test;
pub(crate) use migrations::{MigrationExecutionSettings, accept_migration_inputs};
pub(crate) use snapshots::{
    SnapshotExecutionClaim, accept_snapshot, claim_snapshot, stop_snapshot_before_launch,
};
#[cfg(test)]
mod tests;
#[cfg(all(test, windows))]
mod windows_tests;

use std::{
    collections::BTreeMap,
    fmt,
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[cfg(any(unix, windows))]
use std::ffi::OsString;

#[cfg(test)]
use std::sync::{Barrier, atomic::AtomicUsize};

use crate::{
    domain::{
        ActionPlanStep, HookCompletionRecord, ManagedOutputIdentity, PactrunErrorRefV1,
        RecoveryRiskState, RunFailedStep, RunFailureRecord, RunFinish, RunId, RunOutcome,
        RunPrimaryFailure,
    },
    executor::AdmittedExecution,
    managed_data::{ExecutionDirectory, StagedFile, StagingSession},
    persistence::{
        AcceptanceArbiter, AcceptanceCommitResult, PactrunPersistence, PersistenceError,
        RunArtifactWrite,
    },
};

use self::runtime::LiveExecution;

pub(crate) const INTERACTIVE_ADAPTER_ARGUMENT: &str = platform::INTERACTIVE_ADAPTER_ARGUMENT;

#[cfg(any(unix, windows))]
pub(crate) fn run_interactive_adapter(arguments: &[OsString]) -> i32 {
    platform::run_interactive_adapter(arguments)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct HookRuntimePolicy {
    pub(crate) startup_timeout: Option<Duration>,
    pub(crate) action_timeout: Option<Duration>,
    pub(crate) termination_grace: Option<Duration>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ActionCancellation {
    pub(crate) diagnostics: Arc<diagnostics::Diagnostics>,
    pub(crate) delivery: Arc<delivery::DeliverySlot>,
    requested: Arc<AtomicBool>,
    acceptance_gate: Arc<Mutex<()>>,
    #[cfg(test)]
    acceptance_test_hooks: Arc<Mutex<Option<ArbitrationTestHooks>>>,
    #[cfg(test)]
    launch_test_hooks: Arc<Mutex<Option<ArbitrationTestHooks>>>,
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct ArbitrationTestHooks {
    before_gate: Arc<Barrier>,
    after_gate: Arc<Barrier>,
    gate_entered: Arc<AtomicBool>,
}

impl ActionCancellation {
    pub(crate) fn request(&self) {
        let _gate = self
            .acceptance_gate
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        self.requested.store(true, Ordering::Release);
    }

    pub(crate) fn is_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }

    pub(crate) fn lock_acceptance_gate(&self) -> MutexGuard<'_, ()> {
        self.acceptance_gate
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Serializes the cancellation decision with the launch commit. The gate
    /// is held only until the spawn operation returns the real supervisor;
    /// Hook execution never runs under this lock.
    pub(crate) fn arbitrate_launch<T, E>(
        &self,
        launch: impl FnOnce() -> Result<T, E>,
    ) -> Result<Option<T>, E> {
        #[cfg(test)]
        let test_hooks = self
            .launch_test_hooks
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone();
        #[cfg(test)]
        if let Some(hooks) = &test_hooks {
            hooks.before_gate.wait();
        }

        let _gate = self.lock_acceptance_gate();
        #[cfg(test)]
        if let Some(hooks) = &test_hooks {
            hooks.gate_entered.store(true, Ordering::Release);
            hooks.after_gate.wait();
        }
        if self.is_requested() {
            Ok(None)
        } else {
            launch().map(Some)
        }
    }

    #[cfg(test)]
    pub(crate) fn install_launch_test_hooks(
        &self,
        before_gate: Arc<Barrier>,
        after_gate: Arc<Barrier>,
    ) -> Arc<AtomicBool> {
        let gate_entered = Arc::new(AtomicBool::new(false));
        *self
            .launch_test_hooks
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(ArbitrationTestHooks {
            before_gate,
            after_gate,
            gate_entered: Arc::clone(&gate_entered),
        });
        gate_entered
    }

    #[cfg(test)]
    pub(crate) fn install_acceptance_test_hooks(
        &self,
        before_gate: Arc<Barrier>,
        after_gate: Arc<Barrier>,
    ) -> Arc<AtomicBool> {
        let gate_entered = Arc::new(AtomicBool::new(false));
        *self
            .acceptance_test_hooks
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(ArbitrationTestHooks {
            before_gate,
            after_gate,
            gate_entered: Arc::clone(&gate_entered),
        });
        gate_entered
    }
}

impl HookRuntimePolicy {
    pub(crate) fn from_millis(
        startup_timeout_ms: Option<u64>,
        action_timeout_ms: Option<u64>,
        termination_grace_ms: Option<u64>,
    ) -> Result<Self, &'static str> {
        let startup_timeout = checked_timeout(startup_timeout_ms)?;
        let action_timeout = checked_timeout(action_timeout_ms)?;
        let termination_grace = checked_timeout(termination_grace_ms)?;
        Ok(Self {
            startup_timeout,
            action_timeout,
            termination_grace,
        })
    }
}

// Keep the accepted wire value inside the signed millisecond range used by
// the runtime deadline seam. This gives the CLI a stable, testable boundary
// instead of relying on platform-specific `Instant` capacity for enormous
// durations.
const MAX_TIMEOUT_MS: u64 = i64::MAX as u64;

fn checked_timeout(value: Option<u64>) -> Result<Option<Duration>, &'static str> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value > MAX_TIMEOUT_MS {
        return Err("timeout value cannot be represented as a runtime deadline");
    }
    let duration = Duration::from_millis(value);
    checked_deadline(Instant::now(), duration)
        .map(|_| Some(duration))
        .ok_or("timeout value cannot be represented as a runtime deadline")
}

fn checked_deadline(start: Instant, duration: Duration) -> Option<Instant> {
    start.checked_add(duration)
}

impl AcceptanceArbiter for ActionCancellation {
    fn accepted(&self, run: RunId) {
        self.delivery.accepted(run);
    }
    fn retain_hook_text(&self) -> bool {
        self.diagnostics.retain()
    }
    fn before_durable_acceptance(
        &self,
        commit: impl FnOnce() -> Result<(), PersistenceError>,
    ) -> AcceptanceCommitResult {
        #[cfg(test)]
        let test_hooks = self
            .acceptance_test_hooks
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone();
        #[cfg(test)]
        if let Some(hooks) = &test_hooks {
            hooks.before_gate.wait();
        }
        let _gate = self.lock_acceptance_gate();
        #[cfg(test)]
        if let Some(hooks) = &test_hooks {
            hooks.gate_entered.store(true, Ordering::Release);
            hooks.after_gate.wait();
        }
        if self.is_requested() {
            AcceptanceCommitResult::Cancelled
        } else {
            AcceptanceCommitResult::Committed(commit())
        }
    }
}

pub(crate) struct LiveOutputSlot {
    pub(crate) output: ManagedOutputIdentity,
    pub(crate) handle: String,
    pub(crate) path: PathBuf,
}

pub(crate) struct MigrationOutputSlot {
    input: crate::domain::InputIdentity,
    handle: String,
    path: PathBuf,
}

pub(crate) struct RuntimeTerminalFacts {
    pub(crate) run: RunId,
    pub(crate) outcome: RunOutcome,
    pub(crate) primary_failure: Option<RunPrimaryFailure>,
    pub(crate) hook_completion: Option<HookCompletionRecord>,
    pub(crate) completion_accepted: bool,
    pub(crate) process_started: bool,
    pub(crate) process_terminated: bool,
    pub(crate) submitted_outputs: Vec<ManagedOutputIdentity>,
    pub(crate) outputs: Vec<LiveOutputSlot>,
    pub(crate) migration_outputs: Vec<MigrationOutputSlot>,
    pub(crate) execution: Option<ExecutionDirectory>,
}

impl fmt::Debug for LiveOutputSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LiveOutputSlot")
            .field("output", &self.output)
            .field("path", &"<live-owner-only>")
            .finish()
    }
}

impl fmt::Debug for RuntimeTerminalFacts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeTerminalFacts")
            .field("run", &self.run)
            .field("outcome", &self.outcome)
            .field("primary_failure", &self.primary_failure)
            .field("hook_completion", &self.hook_completion.is_some())
            .field("completion_accepted", &self.completion_accepted)
            .field("process_started", &self.process_started)
            .field("process_terminated", &self.process_terminated)
            .field("submitted_outputs", &self.submitted_outputs)
            .field("outputs", &self.outputs)
            .field(
                "execution",
                &self.execution.as_ref().map(|_| "<owner-ephemeral>"),
            )
            .finish()
    }
}

/// Owner-held terminal facts and independently staged output bytes. This state
/// remains in the continuation registry until a durable terminal transaction
/// is confirmed.
pub(crate) struct FinalizationState {
    pub(super) facts: RuntimeTerminalFacts,
    pub(super) prepared: Option<Vec<PreparedOutput>>,
    pub(super) cleanup_attempted: bool,
    pub(super) publication_failed: bool,
    pub(super) cleanup_failed: bool,
}

pub(super) struct PreparedOutput {
    output: ManagedOutputIdentity,
    bytes: StagedFile,
}

impl fmt::Debug for PreparedOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedOutput")
            .field("output", &self.output)
            .field("bytes", &"<owner-private-staging>")
            .finish()
    }
}

impl fmt::Debug for FinalizationState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FinalizationState")
            .field("facts", &self.facts)
            .field("prepared", &self.prepared.as_ref().map(Vec::len))
            .field("cleanup_attempted", &self.cleanup_attempted)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) enum OwnerContinuation {
    Deletion(Box<deletions::DeletionOwnerState>),
    CleanupFinalization(Box<deletions::CleanupFinalization>),
    TargetReady(runtime::TargetRuntimeFacts),
    Migration(Box<migrations::MigrationOwnerState>),
    CaptureFinalization(Box<capture::CaptureFinalization>),
    RestoreFinalization(Box<restore::RestoreFinalization>),
    Snapshot(Box<snapshots::SnapshotOwnerState>),
    ReadyToFinalize(FinalizationState),
    RetryProcessControl(ProcessControlRetry),
    RetryDurableOperation(DurableOperationRetry),
    RetryFinalization(FinalizationState),
}

#[derive(Debug)]
pub(crate) struct ProcessControlRetry {
    pub(super) live: LiveExecution,
    pub(super) operation: ProcessControlOperation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProcessControlOperation {
    AwaitExit,
    AwaitTree,
    TerminateTree,
}

#[derive(Debug)]
pub(crate) struct DurableOperationRetry {
    pub(super) live: LiveExecution,
    pub(super) operation: PendingDurableOperation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PendingDurableOperation {
    SetRisk {
        request_id: u64,
        requested: RecoveryRiskState,
    },
}

#[derive(Default)]
pub(crate) struct OwnerContinuationRegistry {
    entries: Mutex<BTreeMap<RunId, RegistryEntry>>,
}

enum RegistryEntry {
    Active,
    Stable(Box<OwnerContinuation>),
}

impl OwnerContinuationRegistry {
    fn begin(&self, run: RunId) -> ActiveContinuationGuard<'_> {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        assert!(entries.insert(run, RegistryEntry::Active).is_none());
        ActiveContinuationGuard {
            registry: self,
            run,
            finished: false,
        }
    }

    pub(crate) fn take(&self, run: RunId) -> Option<ContinuationGuard<'_>> {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let continuation = match entries.remove(&run)? {
            RegistryEntry::Active => {
                entries.insert(run, RegistryEntry::Active);
                return None;
            }
            RegistryEntry::Stable(continuation) => *continuation,
        };
        entries.insert(run, RegistryEntry::Active);
        drop(entries);
        Some(ContinuationGuard {
            registry: self,
            run,
            continuation: Some(continuation),
        })
    }

    #[cfg(test)]
    pub(super) fn stable_kind(&self, run: RunId) -> Option<&'static str> {
        match self
            .entries
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(&run)
        {
            Some(RegistryEntry::Stable(continuation)) => Some(match **continuation {
                OwnerContinuation::ReadyToFinalize(_) => "ready",
                OwnerContinuation::Deletion(_) => "deletion",
                OwnerContinuation::CleanupFinalization(_) => "cleanup_finalization",
                OwnerContinuation::TargetReady(_) => "target_ready",
                OwnerContinuation::RetryProcessControl(_) => "process",
                OwnerContinuation::RetryDurableOperation(_) => "durable",
                OwnerContinuation::RetryFinalization(_) => "finalization",
                OwnerContinuation::Snapshot(_) => "snapshot",
                OwnerContinuation::Migration(_) => "migration",
                OwnerContinuation::CaptureFinalization(_) => "capture_finalization",
                OwnerContinuation::RestoreFinalization(_) => "restore_finalization",
            }),
            _ => None,
        }
    }
}

struct ActiveContinuationGuard<'a> {
    registry: &'a OwnerContinuationRegistry,
    run: RunId,
    finished: bool,
}

impl ActiveContinuationGuard<'_> {
    fn finish(mut self, continuation: OwnerContinuation) {
        self.registry
            .entries
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(self.run, RegistryEntry::Stable(Box::new(continuation)));
        self.finished = true;
    }
}

impl Drop for ActiveContinuationGuard<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.registry
                .entries
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .entry(self.run)
                .or_insert(RegistryEntry::Active);
        }
    }
}

pub(crate) struct ContinuationGuard<'a> {
    registry: &'a OwnerContinuationRegistry,
    run: RunId,
    continuation: Option<OwnerContinuation>,
}

impl ContinuationGuard<'_> {
    pub(crate) fn managed_mutation_instance(&self) -> Option<crate::domain::InstanceId> {
        match self.continuation() {
            OwnerContinuation::Deletion(state) => Some(state.instance()),
            OwnerContinuation::Migration(state) => Some(state.instance()),
            other => snapshots::instance(other),
        }
    }
    pub(crate) fn continuation(&self) -> &OwnerContinuation {
        self.continuation
            .as_ref()
            .expect("continuation guard has not completed")
    }

    pub(crate) fn replace(mut self, continuation: OwnerContinuation) {
        self.registry
            .entries
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(self.run, RegistryEntry::Stable(Box::new(continuation)));
        self.continuation = None;
    }

    pub(crate) fn complete(mut self) -> OwnerContinuation {
        self.registry
            .entries
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(&self.run);
        self.continuation
            .take()
            .expect("continuation guard has not completed")
    }
}

impl Drop for ContinuationGuard<'_> {
    fn drop(&mut self) {
        if let Some(continuation) = self.continuation.take() {
            self.registry
                .entries
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .insert(self.run, RegistryEntry::Stable(Box::new(continuation)));
        }
    }
}

pub(crate) fn execute_admitted_action(
    persistence: &PactrunPersistence,
    staging: &StagingSession,
    registry: &OwnerContinuationRegistry,
    admitted: AdmittedExecution,
    policy: HookRuntimePolicy,
    cancellation: ActionCancellation,
) -> RunId {
    execute_admitted_action_with_risk_persistence(
        persistence,
        persistence,
        staging,
        registry,
        admitted,
        policy,
        cancellation,
    )
}

pub(crate) fn execute_snapshot(
    persistence: &PactrunPersistence,
    staging: &StagingSession,
    claim: SnapshotExecutionClaim<'_>,
    policy: HookRuntimePolicy,
) -> RunId {
    runtime::execute_snapshot(persistence, staging, claim, policy)
}

#[cfg(test)]
pub(crate) fn execute_capture_with_risk_failures(
    persistence: &PactrunPersistence,
    staging: &StagingSession,
    claim: SnapshotExecutionClaim<'_>,
    policy: HookRuntimePolicy,
    failures: &AtomicUsize,
) -> RunId {
    struct FailingRisk<'a> {
        p: &'a PactrunPersistence,
        failures: &'a AtomicUsize,
    }
    impl runtime::RecoveryRiskPersistence for FailingRisk<'_> {
        fn set_recovery_risk(&self, run: RunId, risk: RecoveryRiskState) -> Result<(), ()> {
            if self
                .failures
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1))
                .is_ok()
            {
                return Err(());
            }
            runtime::RecoveryRiskPersistence::set_recovery_risk(self.p, run, risk)
        }
    }
    runtime::execute_snapshot_with_risk(
        persistence,
        &FailingRisk {
            p: persistence,
            failures,
        },
        staging,
        claim,
        policy,
    )
}

fn execute_admitted_action_with_risk_persistence(
    persistence: &PactrunPersistence,
    risk_persistence: &dyn runtime::RecoveryRiskPersistence,
    staging: &StagingSession,
    registry: &OwnerContinuationRegistry,
    admitted: AdmittedExecution,
    policy: HookRuntimePolicy,
    cancellation: ActionCancellation,
) -> RunId {
    let run = admitted.run();
    let guard = registry.begin(run);
    let continuation = runtime::execute_registered(
        persistence,
        risk_persistence,
        staging,
        admitted,
        policy,
        cancellation,
    );
    guard.finish(continuation);
    run
}

#[cfg(test)]
pub(crate) fn execute_admitted_action_with_risk_failures(
    persistence: &PactrunPersistence,
    staging: &StagingSession,
    registry: &OwnerContinuationRegistry,
    admitted: AdmittedExecution,
    policy: HookRuntimePolicy,
    cancellation: ActionCancellation,
    remaining_failures: &AtomicUsize,
) -> RunId {
    struct FaultingRiskPersistence<'a> {
        persistence: &'a PactrunPersistence,
        remaining_failures: &'a AtomicUsize,
    }

    impl runtime::RecoveryRiskPersistence for FaultingRiskPersistence<'_> {
        fn set_recovery_risk(&self, run: RunId, requested: RecoveryRiskState) -> Result<(), ()> {
            if self
                .remaining_failures
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                    remaining.checked_sub(1)
                })
                .is_ok()
            {
                return Err(());
            }
            runtime::RecoveryRiskPersistence::set_recovery_risk(self.persistence, run, requested)
        }
    }

    let faulting = FaultingRiskPersistence {
        persistence,
        remaining_failures,
    };
    execute_admitted_action_with_risk_persistence(
        persistence,
        &faulting,
        staging,
        registry,
        admitted,
        policy,
        cancellation,
    )
}

pub(crate) fn resume_owner_continuation(
    persistence: &PactrunPersistence,
    _staging: &StagingSession,
    mut guard: ContinuationGuard<'_>,
) {
    let continuation = guard
        .continuation
        .take()
        .expect("continuation guard has not completed");
    let next = runtime::resume_registered(persistence, continuation);
    guard
        .registry
        .entries
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(guard.run, RegistryEntry::Stable(Box::new(next)));
}

/// Advances an owner-held continuation. `true` means it has been consumed:
/// an accepted Run is durably Finished, or uncertain Snapshot acceptance was
/// proven not committed. It never means Succeeded without reading the outcome.
/// On persistence failure the continuation is restored before returning.
pub(crate) fn advance_owner_continuation(
    persistence: &PactrunPersistence,
    staging: &StagingSession,
    mut guard: ContinuationGuard<'_>,
) -> Result<bool, PersistenceError> {
    if matches!(guard.continuation(), OwnerContinuation::Migration(_)) {
        return migrations::advance(persistence, staging, guard);
    }
    if matches!(
        guard.continuation(),
        OwnerContinuation::RestoreFinalization(_)
    ) {
        return restore::advance(persistence, staging, guard);
    }
    if matches!(
        guard.continuation(),
        OwnerContinuation::CaptureFinalization(_)
    ) {
        return capture::advance(persistence, staging, guard);
    }
    if matches!(guard.continuation(), OwnerContinuation::Snapshot(_)) {
        return snapshots::advance(persistence, staging, guard);
    }
    if matches!(guard.continuation(), OwnerContinuation::Deletion(_)) {
        return deletions::advance(persistence, staging, guard);
    }
    if matches!(
        guard.continuation(),
        OwnerContinuation::CleanupFinalization(_)
    ) {
        return deletions::finalize(persistence, staging, guard);
    }
    let continuation = guard
        .continuation
        .take()
        .expect("continuation guard has not completed");
    let continuation = match continuation {
        OwnerContinuation::ReadyToFinalize(state) => OwnerContinuation::ReadyToFinalize(state),
        OwnerContinuation::RetryFinalization(state) => OwnerContinuation::RetryFinalization(state),
        other => runtime::resume_registered(persistence, other),
    };
    let (published, next) = match continuation {
        OwnerContinuation::ReadyToFinalize(state) | OwnerContinuation::RetryFinalization(state) => {
            match finalize_owner_state(persistence, staging, state) {
                Ok(FinalizationAdvance::Published) => (true, None),
                Ok(FinalizationAdvance::Retained(state)) => {
                    (false, Some(OwnerContinuation::RetryFinalization(*state)))
                }
                Err(error) => {
                    let (state, error) = *error;
                    guard
                        .registry
                        .entries
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner())
                        .insert(
                            guard.run,
                            RegistryEntry::Stable(Box::new(OwnerContinuation::RetryFinalization(
                                state,
                            ))),
                        );
                    return Err(error);
                }
            }
        }
        other => (false, Some(other)),
    };
    if let Some(next) = next {
        guard
            .registry
            .entries
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(guard.run, RegistryEntry::Stable(Box::new(next)));
    } else {
        guard
            .registry
            .entries
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(&guard.run);
    }
    guard.continuation = None;
    Ok(published)
}

enum FinalizationAdvance {
    Published,
    Retained(Box<FinalizationState>),
}

fn finalize_owner_state(
    persistence: &PactrunPersistence,
    staging: &StagingSession,
    mut state: FinalizationState,
) -> Result<FinalizationAdvance, Box<(FinalizationState, PersistenceError)>> {
    if state.facts.process_started && !state.facts.process_terminated {
        return Err(Box::new((
            state,
            PersistenceError::InvalidRunTransition(
                "terminal facts require observed process termination".to_owned(),
            ),
        )));
    }
    if state.prepared.is_none() {
        state.prepared = Some(Vec::new());
        if state.facts.completion_accepted && !state.facts.submitted_outputs.is_empty() {
            let mut slots = state
                .facts
                .outputs
                .iter()
                .map(|slot| (slot.output.clone(), slot.path.clone()))
                .collect::<BTreeMap<_, _>>();
            let mut prepared = Vec::new();
            for output in &state.facts.submitted_outputs {
                let Some(path) = slots.remove(output) else {
                    state.publication_failed = true;
                    prepared.clear();
                    break;
                };
                match staging.stage_action_output(&path) {
                    Ok(bytes) => prepared.push(PreparedOutput {
                        output: output.clone(),
                        bytes,
                    }),
                    Err(_) => {
                        state.publication_failed = true;
                        prepared.clear();
                        break;
                    }
                }
            }
            if !state.publication_failed {
                prepared.sort_by(|left, right| left.output.cmp(&right.output));
                state.prepared = Some(prepared);
            }
        }
    }

    if !state.cleanup_attempted {
        state.cleanup_attempted = true;
        if let Some(execution) = state.facts.execution.as_ref()
            && execution.cleanup().is_err()
        {
            state.cleanup_failed = true;
        }
    }

    let prepared = state
        .prepared
        .as_ref()
        .expect("preparation state initialized");
    let mut readers = Vec::new();
    for artifact in prepared.iter() {
        match artifact.bytes.try_clone_reader() {
            Ok(reader) => readers.push(reader),
            Err(_) => {
                state.publication_failed = true;
                state.prepared = Some(Vec::new());
                break;
            }
        }
    }
    let finish = finish_for_state(&state);
    let prepared = state
        .prepared
        .as_ref()
        .expect("preparation state initialized");
    let mut artifacts = prepared
        .iter()
        .zip(readers.iter_mut())
        .map(|(artifact, reader)| RunArtifactWrite {
            output: artifact.output.clone(),
            byte_len: artifact.bytes.byte_len(),
            reader,
        })
        .collect::<Vec<_>>();
    let owner = staging.owner();
    match persistence.finish_run_owned(
        &owner,
        state.facts.run,
        &finish,
        &state.facts.submitted_outputs,
        &mut artifacts,
    ) {
        Ok(_) => Ok(FinalizationAdvance::Published),
        Err(error) => Err(Box::new((state, error))),
    }
}

fn finish_for_state(state: &FinalizationState) -> RunFinish {
    let mut finish = RunFinish {
        outcome: state.facts.outcome,
        primary_failure: state.facts.primary_failure.clone(),
        secondary_failures: Vec::new(),
        hook_completion: state
            .facts
            .hook_completion
            .as_ref()
            .map(structural_hook_completion),
    };
    if state.publication_failed {
        let failure = safe_execution_failure(
            "managed_output_publication_failed",
            "Managed Action output publication failed",
        );
        if finish.primary_failure.is_none() && finish.outcome == RunOutcome::Succeeded {
            finish.outcome = RunOutcome::Failed;
            finish.primary_failure = Some(RunPrimaryFailure {
                failure,
                step: RunFailedStep::Plan(ActionPlanStep::PublishDeclaredOutputs),
            });
        } else {
            finish.secondary_failures.push(failure);
        }
    }
    if state.cleanup_failed {
        finish.secondary_failures.push(safe_execution_failure(
            "workspace_cleanup_failed",
            "Action execution workspace cleanup failed",
        ));
    }
    finish
}

fn structural_hook_completion(completion: &HookCompletionRecord) -> HookCompletionRecord {
    HookCompletionRecord {
        status: completion.status,
        code: None,
        message: None,
    }
}

fn safe_execution_failure(code: &'static str, message: &'static str) -> RunFailureRecord {
    RunFailureRecord {
        error: PactrunErrorRefV1::new("execution", code)
            .expect("Slice 5 execution error identities are valid"),
        message: message.to_owned(),
    }
}

pub(super) fn ready_failure(
    run: RunId,
    failure: FailureKind,
    outputs: Vec<LiveOutputSlot>,
    execution: Option<ExecutionDirectory>,
) -> OwnerContinuation {
    let (_, primary_failure) = outcome_and_failure(OutcomeWinner::Failed(failure));
    OwnerContinuation::ReadyToFinalize(FinalizationState {
        facts: RuntimeTerminalFacts {
            run,
            outcome: RunOutcome::Failed,
            primary_failure,
            hook_completion: None,
            completion_accepted: false,
            process_started: false,
            process_terminated: true,
            submitted_outputs: Vec::new(),
            outputs,
            migration_outputs: Vec::new(),
            execution,
        },
        prepared: None,
        cleanup_attempted: false,
        publication_failed: false,
        cleanup_failed: false,
    })
}

pub(super) fn ready_cancelled(
    run: RunId,
    outputs: Vec<LiveOutputSlot>,
    execution: Option<ExecutionDirectory>,
) -> OwnerContinuation {
    OwnerContinuation::ReadyToFinalize(FinalizationState {
        facts: RuntimeTerminalFacts {
            run,
            outcome: RunOutcome::Cancelled,
            primary_failure: None,
            hook_completion: None,
            completion_accepted: false,
            process_started: false,
            process_terminated: false,
            submitted_outputs: Vec::new(),
            outputs,
            migration_outputs: Vec::new(),
            execution,
        },
        prepared: None,
        cleanup_attempted: false,
        publication_failed: false,
        cleanup_failed: false,
    })
}

pub(super) fn outcome_and_failure(
    winner: OutcomeWinner,
) -> (RunOutcome, Option<RunPrimaryFailure>) {
    match winner {
        OutcomeWinner::Succeeded => (RunOutcome::Succeeded, None),
        OutcomeWinner::Cancelled => (RunOutcome::Cancelled, None),
        OutcomeWinner::TimedOut => (RunOutcome::TimedOut, None),
        OutcomeWinner::Failed(FailureKind::HookFailure) => (RunOutcome::Failed, None),
        OutcomeWinner::Failed(failure) => {
            let (owner, code, step, message) = failure.record();
            (
                RunOutcome::Failed,
                Some(RunPrimaryFailure {
                    failure: RunFailureRecord {
                        error: PactrunErrorRefV1::new(owner, code)
                            .expect("execution diagnostics are valid stable names"),
                        message: message.to_owned(),
                    },
                    step: RunFailedStep::Plan(step),
                }),
            )
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum FailureKind {
    CapturePublication(&'static str),
    SessionMaterialization,
    Launch,
    ProtocolTransport,
    IpcInitialization(startup::Reason),
    LoaderInitialization(startup::Reason),
    HookReportedProtocol,
    HookFailure,
    Protocol(protocol::ProtocolFailure),
    ProtocolV2(protocol::v2::Failure),
    ServiceAccess(&'static str),
    ServicePrerequisite,
}

impl FailureKind {
    fn materialization(error: &materialize::MaterializationError) -> Self {
        use service_storage::NativeServiceError;
        match error {
            materialize::MaterializationError::Service(NativeServiceError::Access(error)) => {
                Self::ServiceAccess(error.code())
            }
            materialize::MaterializationError::Service(NativeServiceError::Prerequisite) => {
                Self::ServicePrerequisite
            }
            materialize::MaterializationError::Service(NativeServiceError::Storage(
                PersistenceError::CorruptServiceStorage(_),
            )) => Self::ServiceAccess("corrupt_storage_state"),
            materialize::MaterializationError::Service(NativeServiceError::Storage(
                PersistenceError::ServiceStorageUnavailable(_),
            )) => Self::ServiceAccess("allocation_unavailable"),
            _ => Self::SessionMaterialization,
        }
    }
    fn record(&self) -> (&'static str, &'static str, ActionPlanStep, &'static str) {
        match self {
            Self::ProtocolV2(error) => (
                "hook_protocol_v2",
                error.code,
                ActionPlanStep::AcceptCompletion,
                "Hook V2 protocol failed",
            ),
            Self::ServiceAccess(code) => (
                "service_storage",
                code,
                ActionPlanStep::EstablishSession,
                "ServiceStorage authority qualification failed",
            ),
            Self::ServicePrerequisite => (
                "admission",
                "plan_invalidated",
                ActionPlanStep::EstablishSession,
                "service presence prerequisite is not satisfied",
            ),
            Self::CapturePublication(message) => (
                "execution",
                "managed_output_publication_failed",
                ActionPlanStep::PublishDeclaredOutputs,
                message,
            ),
            Self::SessionMaterialization => (
                "execution",
                "session_materialization_failed",
                ActionPlanStep::EstablishSession,
                "Action execution materialization failed",
            ),
            Self::Launch => (
                "execution",
                "launch_failed",
                ActionPlanStep::LaunchHook,
                "Hook process launch failed",
            ),
            Self::ProtocolTransport => (
                "execution",
                "protocol_transport_failed",
                ActionPlanStep::AcceptCompletion,
                "Hook Protocol transport failed",
            ),
            Self::IpcInitialization(reason) => (
                "execution",
                "ipc_initialization_failed",
                ActionPlanStep::EstablishSession,
                reason.message(),
            ),
            Self::LoaderInitialization(reason) => (
                "execution",
                "shell_loader_initialization_failed",
                ActionPlanStep::EstablishSession,
                reason.message(),
            ),
            Self::HookReportedProtocol => (
                "execution",
                "hook_reported_protocol_error",
                ActionPlanStep::AcceptCompletion,
                "Hook reported a protocol failure",
            ),
            Self::HookFailure => unreachable!("Hook failure is represented by Hook completion"),
            Self::Protocol(protocol) => (
                "hook_protocol_v1",
                protocol.code,
                ActionPlanStep::AcceptCompletion,
                protocol.message,
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum OutcomeWinner {
    Succeeded,
    Failed(FailureKind),
    Cancelled,
    TimedOut,
}
