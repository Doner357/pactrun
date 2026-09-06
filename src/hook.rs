//! Action Hook materialization, process supervision, and Frozen V1 runtime.

#![allow(dead_code)]

mod materialize;
mod platform;
mod protocol;
mod runtime;
#[cfg(test)]
mod tests;
#[cfg(all(test, windows))]
mod windows_tests;

use std::{
    collections::BTreeMap,
    fmt,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[cfg(test)]
use std::sync::atomic::AtomicUsize;

use crate::{
    domain::{
        ActionPlanStep, HookCompletionRecord, ManagedOutputIdentity, PactrunErrorRefV1,
        RecoveryRiskState, RunFailedStep, RunFailureRecord, RunId, RunOutcome, RunPrimaryFailure,
    },
    executor::AdmittedExecution,
    managed_data::StagingSession,
    persistence::PactrunPersistence,
};

use self::runtime::LiveExecution;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct HookRuntimePolicy {
    pub(crate) startup_timeout: Option<Duration>,
    pub(crate) action_timeout: Option<Duration>,
    pub(crate) termination_grace: Option<Duration>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ActionCancellation {
    requested: Arc<AtomicBool>,
}

impl ActionCancellation {
    pub(crate) fn request(&self) {
        self.requested.store(true, Ordering::Release);
    }

    pub(super) fn is_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }
}

pub(crate) struct LiveOutputSlot {
    pub(crate) output: ManagedOutputIdentity,
    pub(crate) path: PathBuf,
}

pub(crate) struct RuntimeTerminalFacts {
    pub(crate) run: RunId,
    pub(crate) outcome: RunOutcome,
    pub(crate) primary_failure: Option<RunPrimaryFailure>,
    pub(crate) hook_completion: Option<HookCompletionRecord>,
    pub(crate) completion_accepted: bool,
    pub(crate) process_terminated: bool,
    pub(crate) outputs: Vec<LiveOutputSlot>,
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
            .field("process_terminated", &self.process_terminated)
            .field("outputs", &self.outputs)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) enum OwnerContinuation {
    ReadyForSlice5(RuntimeTerminalFacts),
    RetryProcessControl(ProcessControlRetry),
    RetryDurableOperation(DurableOperationRetry),
}

#[derive(Debug)]
pub(crate) struct ProcessControlRetry {
    pub(super) live: LiveExecution,
    pub(super) operation: ProcessControlOperation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProcessControlOperation {
    AwaitExit,
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
                OwnerContinuation::ReadyForSlice5(_) => "ready",
                OwnerContinuation::RetryProcessControl(_) => "process",
                OwnerContinuation::RetryDurableOperation(_) => "durable",
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

pub(super) fn ready_failure(
    run: RunId,
    failure: FailureKind,
    outputs: Vec<LiveOutputSlot>,
) -> OwnerContinuation {
    let (_, primary_failure) = outcome_and_failure(OutcomeWinner::Failed(failure));
    OwnerContinuation::ReadyForSlice5(RuntimeTerminalFacts {
        run,
        outcome: RunOutcome::Failed,
        primary_failure,
        hook_completion: None,
        completion_accepted: false,
        process_terminated: true,
        outputs,
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
    SessionMaterialization,
    Launch,
    ProtocolTransport,
    HookReportedProtocol,
    HookFailure,
    Protocol(protocol::ProtocolFailure),
}

impl FailureKind {
    fn record(&self) -> (&'static str, &'static str, ActionPlanStep, &'static str) {
        match self {
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
