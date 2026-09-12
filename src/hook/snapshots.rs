//! Snapshot ownership reuses the existing continuation registry. S5/S6 attach
//! their actual process/result states; this substrate never fabricates success.
use super::*;
use crate::{
    domain::{
        ActionRunBoundary, ExecutionOwnerSession, RunFinish, RunState, SnapshotExecutionPlan,
    },
    executor::{AdmissionOptions, ExecutorError},
    persistence::AcceptanceError,
};

#[derive(Debug)]
pub(crate) struct SnapshotOwnerState {
    run: RunId,
    owner: ExecutionOwnerSession,
    plan: SnapshotExecutionPlan,
    options: AdmissionOptions,
    cancellation: ActionCancellation,
    uncertain: bool,
    phase: SnapshotPhase,
}
#[derive(Debug)]
enum SnapshotPhase {
    Admission,
    Ready,
    Claimed,
    Stop(RunFinish),
}

pub(super) fn instance(continuation: &OwnerContinuation) -> Option<crate::domain::InstanceId> {
    match continuation {
        OwnerContinuation::Snapshot(state) => Some(state.plan.instance()),
        _ => None,
    }
}

pub(crate) fn accept_snapshot(
    p: &PactrunPersistence,
    staging: &StagingSession,
    registry: &OwnerContinuationRegistry,
    plan: SnapshotExecutionPlan,
    options: AdmissionOptions,
    cancellation: ActionCancellation,
) -> Result<RunId, ExecutorError> {
    let run = RunId::generate().map_err(|error| ExecutorError::Persistence {
        run: None,
        source: PersistenceError::CorruptRun(error.to_string()),
    })?;
    let mut guard = registry.begin(run);
    let owner = staging.owner();
    let result = p.create_accepted_managed_run(
        run,
        plan.instance(),
        plan.expected_state_version(),
        plan.operation(),
        &owner,
        &cancellation,
    );
    let mut state = SnapshotOwnerState {
        run,
        owner,
        plan,
        options,
        cancellation,
        uncertain: false,
        phase: SnapshotPhase::Admission,
    };
    match result {
        Ok(_) => {
            guard.finish(OwnerContinuation::Snapshot(Box::new(state)));
            Ok(run)
        }
        Err(AcceptanceError::Uncertain { source, .. }) => {
            state.uncertain = true;
            guard.finish(OwnerContinuation::Snapshot(Box::new(state)));
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
                .unwrap_or_else(|poison| poison.into_inner())
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
    let OwnerContinuation::Snapshot(state) =
        guard.continuation.as_mut().expect("held continuation")
    else {
        unreachable!()
    };
    if state.run != guard.run {
        return Err(PersistenceError::CorruptRun(
            "Snapshot continuation RunId changed".to_owned(),
        ));
    }
    if state.owner != staging.owner() {
        return Err(PersistenceError::InvalidRunTransition(
            "Snapshot continuation belongs to another owner".to_owned(),
        ));
    }
    let Some(view) = p.load_managed_run(state.run)? else {
        if state.uncertain {
            guard.complete();
            return Ok(true);
        }
        return Err(PersistenceError::CorruptRun(
            "accepted Snapshot Run disappeared".to_owned(),
        ));
    };
    if view.operation != *state.plan.operation() || view.instance != state.plan.instance() {
        return Err(PersistenceError::CorruptRun(
            "Snapshot continuation does not match its Run".to_owned(),
        ));
    }
    let RunState::Running(execution) = view.state else {
        guard.complete();
        return Ok(true);
    };
    if execution.owner != state.owner {
        return Err(PersistenceError::CorruptRun(
            "Snapshot Run owner changed".to_owned(),
        ));
    }
    state.uncertain = false;
    if matches!(state.phase, SnapshotPhase::Admission)
        && execution.boundary == ActionRunBoundary::Admitted
    {
        state.phase = SnapshotPhase::Ready;
    }
    if state.cancellation.is_requested()
        && matches!(state.phase, SnapshotPhase::Admission | SnapshotPhase::Ready)
    {
        state.phase = SnapshotPhase::Stop(cancelled());
    }
    if let SnapshotPhase::Stop(finish) = &state.phase {
        p.finish_run_owned(&state.owner, state.run, finish, &[], &mut [])?;
        guard.complete();
        return Ok(true);
    }
    if matches!(state.phase, SnapshotPhase::Admission) {
        match crate::executor::admit_snapshot_existing(
            p,
            &crate::workflow::PlatformHostLauncherLookup,
            state.run,
            &state.owner,
            &state.plan,
            state.options,
        ) {
            Ok(()) => state.phase = SnapshotPhase::Ready,
            Err(ExecutorError::Refused { .. }) => {
                guard.complete();
                return Ok(true);
            }
            Err(ExecutorError::Persistence { source, .. }) => return Err(source),
            Err(ExecutorError::CancelledBeforeAcceptance) => unreachable!(),
        }
    }
    Ok(false) // dropping the guard restores the state, never launches a Hook.
}
fn cancelled() -> RunFinish {
    RunFinish {
        outcome: RunOutcome::Cancelled,
        primary_failure: None,
        secondary_failures: Vec::new(),
        hook_completion: None,
    }
}

pub(crate) fn stop_snapshot_before_launch(
    registry: &OwnerContinuationRegistry,
    run: RunId,
    finish: RunFinish,
) -> Result<bool, PersistenceError> {
    if finish.outcome == RunOutcome::Succeeded {
        return Err(PersistenceError::InvalidRunTransition(
            "Snapshot success requires atomic result publication".to_owned(),
        ));
    }
    let Some(mut guard) = registry.take(run) else {
        return Ok(false);
    };
    let OwnerContinuation::Snapshot(state) =
        guard.continuation.as_mut().expect("held continuation")
    else {
        return Ok(false);
    };
    if state.run != run {
        return Err(PersistenceError::CorruptRun(
            "Snapshot continuation RunId changed".to_owned(),
        ));
    }
    if matches!(state.phase, SnapshotPhase::Claimed) {
        return Err(PersistenceError::InvalidRunTransition(
            "claimed Snapshot execution must terminate through its runtime owner".to_owned(),
        ));
    }
    state.phase = SnapshotPhase::Stop(finish);
    Ok(true)
}

pub(crate) struct SnapshotExecutionClaim<'a> {
    guard: ContinuationGuard<'a>,
}
pub(crate) struct SnapshotLaunchAttempt<'a, T> {
    value: T,
    guard: ContinuationGuard<'a>,
}
impl<T> SnapshotLaunchAttempt<'_, T> {
    pub(crate) fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }
    pub(crate) fn replace(self, make: impl FnOnce(T) -> OwnerContinuation) {
        self.guard.replace(make(self.value));
    }
}
impl<T> fmt::Debug for SnapshotLaunchAttempt<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SnapshotLaunchAttempt")
            .field("run", &self.guard.run)
            .finish_non_exhaustive()
    }
}
impl<'a> SnapshotExecutionClaim<'a> {
    fn state(&self) -> &SnapshotOwnerState {
        match self.guard.continuation() {
            OwnerContinuation::Snapshot(state) => state,
            _ => unreachable!(),
        }
    }
    pub(crate) fn run(&self) -> RunId {
        self.state().run
    }
    pub(crate) fn plan(&self) -> &SnapshotExecutionPlan {
        &self.state().plan
    }
    pub(super) fn cancellation(&self) -> ActionCancellation {
        self.state().cancellation.clone()
    }
    /// This typestate has not invoked the launch callback. S5/S6 may fail
    /// materialization here; a launch attempt cannot regain this method.
    pub(crate) fn stop_before_launch(mut self, finish: RunFinish) -> Result<(), PersistenceError> {
        if finish.outcome == RunOutcome::Succeeded {
            return Err(PersistenceError::InvalidRunTransition(
                "Snapshot success requires atomic result publication".to_owned(),
            ));
        }
        if let Some(OwnerContinuation::Snapshot(state)) = self.guard.continuation.as_mut() {
            state.phase = SnapshotPhase::Stop(finish);
        }
        Ok(())
    }
    /// Only a successful callback may hand its supervisor and this still-held
    /// claim to S5/S6. Errors stay Claimed (no replay), not Ready or Succeeded.
    pub(crate) fn launch_once<T, E>(
        mut self,
        launch: impl FnOnce(&SnapshotExecutionPlan) -> Result<T, E>,
    ) -> Result<Option<SnapshotLaunchAttempt<'a, T>>, Box<SnapshotLaunchAttempt<'a, E>>> {
        let cancellation = self.state().cancellation.clone();
        match cancellation.arbitrate_launch(|| launch(self.plan())) {
            Ok(Some(value)) => Ok(Some(SnapshotLaunchAttempt {
                value,
                guard: self.guard,
            })),
            Err(value) => Err(Box::new(SnapshotLaunchAttempt {
                value,
                guard: self.guard,
            })),
            Ok(None) => {
                if let Some(OwnerContinuation::Snapshot(state)) = self.guard.continuation.as_mut() {
                    state.phase = SnapshotPhase::Stop(cancelled());
                }
                Ok(None)
            }
        }
    }
    pub(crate) fn replace(self, continuation: OwnerContinuation) {
        self.guard.replace(continuation);
    }
}

pub(crate) fn claim_snapshot<'a>(
    p: &PactrunPersistence,
    staging: &StagingSession,
    registry: &'a OwnerContinuationRegistry,
    run: RunId,
) -> Result<Option<SnapshotExecutionClaim<'a>>, PersistenceError> {
    let Some(mut guard) = registry.take(run) else {
        return Ok(None);
    };
    let OwnerContinuation::Snapshot(state) =
        guard.continuation.as_mut().expect("held continuation")
    else {
        return Ok(None);
    };
    if state.run != run {
        return Err(PersistenceError::CorruptRun(
            "Snapshot continuation RunId changed".to_owned(),
        ));
    }
    if state.owner != staging.owner() {
        return Err(PersistenceError::InvalidRunTransition(
            "Snapshot claim belongs to another owner".to_owned(),
        ));
    }
    if !matches!(state.phase, SnapshotPhase::Ready) {
        return Ok(None);
    }
    let view = p
        .load_managed_run(run)?
        .ok_or_else(|| PersistenceError::CorruptRun("Snapshot Run disappeared".to_owned()))?;
    match view.state {
        RunState::Finished(_) => {
            guard.complete();
            return Ok(None);
        }
        RunState::Running(execution)
            if execution.owner == state.owner
                && execution.boundary == ActionRunBoundary::Admitted
                && view.operation == *state.plan.operation() => {}
        _ => {
            return Err(PersistenceError::CorruptRun(
                "Snapshot is not qualified for execution".to_owned(),
            ));
        }
    }
    state.phase = SnapshotPhase::Claimed;
    Ok(Some(SnapshotExecutionClaim { guard }))
}
