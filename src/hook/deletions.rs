//! Deletion owner continuations. No lost or uncertain launch is replayed.
use super::*;
use crate::domain::{
    ActionRunBoundary, DeletionPlan, DeletionWork, ExecutionOwnerSession, ManagedRunIdentity,
    RunState,
};
use crate::executor::{AdmissionOptions, ExecutorError};
use crate::persistence::AcceptanceError;

#[derive(Debug)]
pub(crate) struct DeletionOwnerState {
    run: RunId,
    owner: ExecutionOwnerSession,
    plan: DeletionPlan,
    options: AdmissionOptions,
    cancellation: ActionCancellation,
    policy: HookRuntimePolicy,
    phase: Phase,
    uncertain_acceptance: bool,
    prepared: Option<materialize::MaterializedAction>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Admission,
    Ready,
    Claimed,
}
impl DeletionOwnerState {
    pub(super) fn instance(&self) -> crate::domain::InstanceId {
        self.plan.instance
    }
}

pub(crate) fn accept_deletion(
    p: &PactrunPersistence,
    staging: &StagingSession,
    registry: &OwnerContinuationRegistry,
    plan: DeletionPlan,
    options: AdmissionOptions,
    cancellation: ActionCancellation,
    policy: HookRuntimePolicy,
) -> Result<RunId, ExecutorError> {
    let run = RunId::generate().map_err(|e| ExecutorError::Persistence {
        run: None,
        source: PersistenceError::CorruptRun(e.to_string()),
    })?;
    let mut guard = registry.begin(run);
    let owner = staging.owner();
    let result = p.create_accepted_managed_run(
        run,
        plan.instance,
        plan.expected,
        &ManagedRunIdentity::Deletion {
            revision: plan.revision.clone(),
            mode: plan.mode,
        },
        &owner,
        &cancellation,
    );
    let mut state = DeletionOwnerState {
        run,
        owner,
        plan,
        options,
        cancellation,
        policy,
        phase: Phase::Admission,
        uncertain_acceptance: false,
        prepared: None,
    };
    match result {
        Ok(_) => {
            guard.finish(OwnerContinuation::Deletion(Box::new(state)));
            Ok(run)
        }
        Err(AcceptanceError::Uncertain { source, .. }) => {
            state.uncertain_acceptance = true;
            guard.finish(OwnerContinuation::Deletion(Box::new(state)));
            Err(ExecutorError::Persistence {
                run: Some(run),
                source,
            })
        }
        Err(error) => {
            guard
                .registry
                .entries
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&run);
            guard.finished = true;
            match error {
                AcceptanceError::Cancelled { .. } => Err(ExecutorError::CancelledBeforeAcceptance),
                AcceptanceError::NotCommitted { source, .. } => {
                    Err(ExecutorError::Persistence { run: None, source })
                }
                AcceptanceError::Uncertain { .. } => unreachable!(),
            }
        }
    }
}

pub(super) fn advance(
    p: &PactrunPersistence,
    staging: &StagingSession,
    mut guard: ContinuationGuard<'_>,
) -> Result<bool, PersistenceError> {
    let OwnerContinuation::Deletion(state) =
        guard.continuation.as_mut().expect("held continuation")
    else {
        unreachable!()
    };
    if state.owner != staging.owner() || state.run != guard.run {
        return Err(PersistenceError::InvalidRunTransition(
            "deletion continuation owner changed".to_owned(),
        ));
    }
    let Some(view) = p.load_managed_run(state.run)? else {
        if state.uncertain_acceptance {
            guard.complete();
            return Ok(true);
        }
        return Err(PersistenceError::CorruptRun(
            "accepted deletion disappeared".to_owned(),
        ));
    };
    let RunState::Running(execution) = &view.state else {
        guard.complete();
        return Ok(true);
    };
    if execution.owner != state.owner {
        return Err(PersistenceError::CorruptRun(
            "deletion owner changed".to_owned(),
        ));
    }
    state.uncertain_acceptance = false;
    if execution.boundary == ActionRunBoundary::Admitted && state.phase == Phase::Admission {
        state.phase = Phase::Ready;
    }
    if state.phase == Phase::Claimed {
        return Err(PersistenceError::InvalidRunTransition(
            "claimed Cleanup cannot be relaunched".to_owned(),
        ));
    }
    if state.cancellation.is_requested() {
        if let Some(prepared) = &state.prepared {
            prepared.cleanup_unlaunched();
        }
        if execution.boundary == ActionRunBoundary::Admitted
            && matches!(state.plan.work, DeletionWork::Cleanup(_))
        {
            p.publish_cleanup_result(state.run, &state.owner, &cancelled(), false, false)?;
        } else {
            p.finish_run_owned(&state.owner, state.run, &cancelled(), &[], &mut [])?;
        }
        guard.complete();
        return Ok(true);
    }
    if state.phase == Phase::Admission {
        use crate::workflow::HostLauncherLookup;
        let check = |observed: &crate::domain::InterpreterLauncherObservation| {
            match crate::workflow::PlatformHostLauncherLookup
                .resolve(&observed.search_directories, &observed.command)
            {
                Ok(path) if path == observed.resolved_absolute_path => Ok(()),
                _ => Err("Cleanup launcher selection changed".to_owned()),
            }
        };
        match p.admit_deletion(
            state.run,
            &state.owner,
            &state.plan,
            state.options.recovery_override,
            &check,
        )? {
            Ok(()) => state.phase = Phase::Ready,
            Err(_) => {
                guard.complete();
                return Ok(true);
            }
        }
    }
    match &state.plan.work {
        DeletionWork::Cleanup(_) => Ok(false), // launch is outside the application mutation guard
        DeletionWork::Abandon => {
            if !p.finish_instance_retirement(state.run, &state.owner, &succeeded())? {
                return Ok(false);
            }
            guard.complete();
            Ok(true)
        }
        DeletionWork::NoCleanup | DeletionWork::FinalizationOnly { .. } => {
            p.prepare_deletion_finalization(state.run, &state.owner)?;
            if finalize_storage(p, state.run, &state.owner, &succeeded())? {
                guard.complete();
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }
}

pub(crate) fn execute_ready(
    p: &PactrunPersistence,
    staging: &StagingSession,
    registry: &OwnerContinuationRegistry,
    run: RunId,
) -> Result<bool, PersistenceError> {
    let Some(mut guard) = registry.take(run) else {
        return Ok(false);
    };
    let Some(OwnerContinuation::Deletion(state)) = guard.continuation.as_mut() else {
        return Ok(false);
    };
    if state.phase != Phase::Ready {
        return Ok(false);
    }
    let DeletionWork::Cleanup(cleanup) = &state.plan.work else {
        return Ok(false);
    };
    if state.prepared.is_none() {
        match materialize::MaterializedAction::create_cleanup(
            p,
            staging,
            run,
            &state.plan.revision,
            cleanup,
        ) {
            Ok(value) => state.prepared = Some(value),
            Err(error) => {
                let failure = FailureKind::materialization(&error);
                let continuation = before_launch(run, failure, false, None);
                guard.replace(continuation);
                return Ok(true);
            }
        }
    }
    // This live continuation has not attempted spawn. A prior SQL commit may
    // have succeeded despite an error, so inspect the exact attempt, never
    // obtain a fresh launch permit. A restarted process cannot recreate this
    // continuation: the original owner lease is required below.
    match p.deletion_obligation(state.plan.instance)? {
        Some(obligation)
            if obligation.attempt == run
                && obligation.phase == crate::domain::DeletionPhase::LaunchAuthorized => {}
        Some(_) => {
            return Err(PersistenceError::InvalidRunTransition(
                "Cleanup launch evidence differs from its live continuation".to_owned(),
            ));
        }
        None => p.authorize_cleanup_launch(run, &state.owner)?,
    }
    if state.owner != staging.owner() {
        return Err(PersistenceError::InvalidRunTransition(
            "Cleanup owner changed".to_owned(),
        ));
    }
    state.phase = Phase::Claimed;
    let prepared = state.prepared.take().expect("prepared Cleanup");
    let continuation =
        runtime::execute_materialized(p, run, prepared, state.policy, state.cancellation.clone());
    guard.replace(continuation);
    Ok(true)
}

fn cancelled() -> RunFinish {
    RunFinish {
        outcome: RunOutcome::Cancelled,
        primary_failure: None,
        secondary_failures: vec![],
        hook_completion: None,
    }
}
fn succeeded() -> RunFinish {
    RunFinish {
        outcome: RunOutcome::Succeeded,
        ..cancelled()
    }
}

fn finalize_storage(
    p: &PactrunPersistence,
    run: RunId,
    owner: &ExecutionOwnerSession,
    finish: &RunFinish,
) -> Result<bool, PersistenceError> {
    match p.finalize_next_allocation(run, owner) {
        Ok(false) => Ok(false),
        Ok(true) => p.finish_instance_retirement(run, owner, finish),
        Err(PersistenceError::ServiceStorageUnavailable(_)) => {
            let failure = RunFinish {
                outcome: RunOutcome::Failed,
                primary_failure: Some(RunPrimaryFailure {
                    failure: RunFailureRecord { error: PactrunErrorRefV1::new("service_storage", "allocation_unavailable").expect("registered error"), message: "storage finalization could not complete safely; the obligation is retained".to_owned() },
                    step: RunFailedStep::DeletionPlan(crate::domain::DeletionPlanStep::FinalizeStorage),
                }),
                secondary_failures: finish.secondary_failures.clone(), hook_completion: finish.hook_completion.clone(),
            };
            p.finish_run_owned(owner, run, &failure, &[], &mut [])?;
            Ok(true)
        }
        Err(error) => Err(error),
    }
}

#[derive(Debug)]
pub(crate) struct CleanupFinalization {
    facts: RuntimeTerminalFacts,
    may_have_run: bool,
    housekeeping_attempted: bool,
    housekeeping_failed: bool,
    cancellation: ActionCancellation,
}
pub(super) fn terminal(facts: RuntimeTerminalFacts, may_have_run: bool) -> OwnerContinuation {
    terminal_with_cancellation(facts, may_have_run, ActionCancellation::default())
}
pub(super) fn terminal_with_cancellation(
    mut facts: RuntimeTerminalFacts,
    may_have_run: bool,
    cancellation: ActionCancellation,
) -> OwnerContinuation {
    if let Some(primary) = &mut facts.primary_failure {
        primary.step = RunFailedStep::for_operation(
            primary.step.rank(),
            crate::domain::ManagedExecutionKind::Deletion,
        )
        .expect("valid step");
    }
    OwnerContinuation::CleanupFinalization(Box::new(CleanupFinalization {
        facts,
        may_have_run,
        housekeeping_attempted: false,
        housekeeping_failed: false,
        cancellation,
    }))
}
pub(super) fn before_launch(
    run: RunId,
    failure: FailureKind,
    may_have_run: bool,
    execution: Option<ExecutionDirectory>,
) -> OwnerContinuation {
    let (outcome, primary_failure) = outcome_and_failure(OutcomeWinner::Failed(failure));
    terminal(
        RuntimeTerminalFacts {
            run,
            outcome,
            primary_failure,
            hook_completion: None,
            completion_accepted: false,
            process_started: false,
            process_terminated: false,
            submitted_outputs: vec![],
            outputs: vec![],
            migration_outputs: vec![],
            execution,
        },
        may_have_run,
    )
}
pub(super) fn adapt_early(
    continuation: OwnerContinuation,
    cleanup: bool,
    may_have_run: bool,
) -> OwnerContinuation {
    if !cleanup {
        return continuation;
    }
    match continuation {
        OwnerContinuation::ReadyToFinalize(state) => terminal(state.facts, may_have_run),
        other => other,
    }
}
pub(super) fn finalize(
    p: &PactrunPersistence,
    staging: &StagingSession,
    mut guard: ContinuationGuard<'_>,
) -> Result<bool, PersistenceError> {
    let Some(OwnerContinuation::CleanupFinalization(state)) = guard.continuation.as_mut() else {
        unreachable!()
    };
    if matches!(
        p.load_managed_run(state.facts.run)?.map(|view| view.state),
        Some(RunState::Finished(_))
    ) {
        guard.complete();
        return Ok(true);
    }
    if state.facts.process_started && !state.facts.process_terminated {
        return Err(PersistenceError::InvalidRunTransition(
            "Cleanup process tree is still live".to_owned(),
        ));
    }
    if !state.housekeeping_attempted {
        state.housekeeping_attempted = true;
        state.housekeeping_failed = state
            .facts
            .execution
            .as_ref()
            .is_some_and(|execution| execution.cleanup().is_err());
    }
    let mut finish = RunFinish {
        outcome: state.facts.outcome,
        primary_failure: state.facts.primary_failure.clone(),
        secondary_failures: vec![],
        hook_completion: state
            .facts
            .hook_completion
            .as_ref()
            .map(structural_hook_completion),
    };
    if state.housekeeping_failed {
        finish.secondary_failures.push(safe_execution_failure(
            "workspace_cleanup_failed",
            "Cleanup workspace housekeeping failed",
        ));
    }
    if p.publish_cleanup_result(
        state.facts.run,
        &staging.owner(),
        &finish,
        state.may_have_run,
        state.facts.completion_accepted,
    )? {
        if state.cancellation.is_requested() {
            finish.outcome = RunOutcome::Cancelled;
            p.finish_run_owned(&staging.owner(), state.facts.run, &finish, &[], &mut [])?;
        } else if !finalize_storage(p, state.facts.run, &staging.owner(), &finish)? {
            return Ok(false);
        }
    }
    guard.complete();
    Ok(true)
}
