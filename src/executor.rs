//! Execution-plan runtime ownership.
//!
//! Run acceptance and Admission turn an ephemeral `ActionExecutionPlan` into a
//! durable Run and either admit it with durable pins or record the refusal in
//! that same Run. An admitted value is a one-shot capability consumed by the
//! Hook runtime.

// Later M3 slices add the production callers.
#![allow(dead_code)]

use std::fmt;

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_ADMISSION_FOR_TEST: Cell<bool> = const { Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn fail_next_admission_for_test() {
    FAIL_NEXT_ADMISSION_FOR_TEST.with(|failed| failed.set(true));
}

use crate::{
    domain::{
        ActionExecutionPlan, ActionRunIdentity, AdmissionFacts, AdmissionRefusal,
        ExecutionOwnerSession, InterpreterLauncherObservation, RunId,
    },
    persistence::{
        AcceptanceArbiter, AcceptanceError, PactrunPersistence, PersistenceError,
        UnconditionalAcceptance,
    },
    workflow::HostLauncherLookup,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct AdmissionOptions {
    /// The explicit one-execution recovery override of PR-REQ-0068. It bypasses
    /// only the trust guard, never stale-plan checks or mutation conflicts.
    pub(crate) recovery_override: bool,
}

/// A Run that is durably Admitted with its pins established. The Plan is the
/// exact compiled Plan; Admission never re-resolves or recompiles it.
#[derive(Debug, PartialEq)]
pub(crate) struct AdmittedExecution {
    run: RunId,
    plan: ActionExecutionPlan,
}

impl AdmittedExecution {
    pub(crate) fn from_durable(run: RunId, plan: ActionExecutionPlan) -> Self {
        Self { run, plan }
    }

    pub(crate) fn run(&self) -> RunId {
        self.run
    }

    pub(crate) fn plan(&self) -> &ActionExecutionPlan {
        &self.plan
    }
}

#[derive(Debug)]
pub(crate) enum ExecutorError {
    /// Cancellation won before the acceptance transaction committed. The
    /// candidate was never durable and must not be retried or replaced.
    CancelledBeforeAcceptance,
    /// Admission refused the Plan; the refusal is already durable as the Run's
    /// terminal `Failed` outcome.
    Refused {
        run: RunId,
        refusal: AdmissionRefusal,
    },
    /// An infrastructure failure. `run` is present when acceptance committed
    /// before the failure, so the caller can inspect or retry that exact Run.
    Persistence {
        run: Option<RunId>,
        source: PersistenceError,
    },
}

impl fmt::Display for ExecutorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CancelledBeforeAcceptance => {
                formatter.write_str("invoke cancelled before Run acceptance; no Run was created")
            }
            Self::Refused { run, refusal } => {
                let error = refusal.error_ref();
                write!(
                    formatter,
                    "{}.{}: Run {run} was not admitted: {}",
                    error.owner(),
                    error.code(),
                    refusal.message()
                )
            }
            Self::Persistence { source, .. } => write!(formatter, "persistence: {source}"),
        }
    }
}

impl std::error::Error for ExecutorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CancelledBeforeAcceptance | Self::Refused { .. } => None,
            Self::Persistence { source, .. } => Some(source),
        }
    }
}

impl From<PersistenceError> for ExecutorError {
    fn from(source: PersistenceError) -> Self {
        Self::Persistence { run: None, source }
    }
}

/// Accepts a compiled Plan as an execution attempt and admits it.
///
/// The durable Run is created first (PR-REQ-0049). Every authoritative check and
/// the resulting transition then happen inside one persistence transaction in
/// the precedence of PR-REQ-0279; nothing here decides anything outside it. The
/// caller holds the per-Instance mutation guard for exactly this call.
pub(crate) fn accept_and_admit<L: HostLauncherLookup>(
    persistence: &PactrunPersistence,
    launcher: &L,
    owner: &ExecutionOwnerSession,
    plan: &ActionExecutionPlan,
    options: AdmissionOptions,
) -> Result<AdmittedExecution, ExecutorError> {
    accept_and_admit_with_arbiter(
        persistence,
        launcher,
        owner,
        plan,
        options,
        &UnconditionalAcceptance,
    )
}

pub(crate) fn accept_and_admit_with_arbiter<L: HostLauncherLookup, A: AcceptanceArbiter>(
    persistence: &PactrunPersistence,
    launcher: &L,
    owner: &ExecutionOwnerSession,
    plan: &ActionExecutionPlan,
    options: AdmissionOptions,
    arbiter: &A,
) -> Result<AdmittedExecution, ExecutorError> {
    let run = RunId::generate().map_err(|error| ExecutorError::Persistence {
        run: None,
        source: PersistenceError::CorruptRun(format!("generate RunId: {error}")),
    })?;
    let run = persistence
        .create_accepted_run(
            run,
            plan.instance(),
            plan.expected_state_version(),
            &ActionRunIdentity {
                revision: plan.active_revision().clone(),
                action: plan.action().clone(),
            },
            owner,
            arbiter,
        )
        .map_err(|error| match error {
            AcceptanceError::Cancelled { .. } => ExecutorError::CancelledBeforeAcceptance,
            AcceptanceError::NotCommitted { source, .. } => {
                ExecutorError::Persistence { run: None, source }
            }
            AcceptanceError::Uncertain { run, source } => ExecutorError::Persistence {
                run: Some(run),
                source,
            },
        })?;
    admit_existing(persistence, launcher, run, plan, options)
}

/// Retries Admission for an already durable Accepted Run. This never creates
/// another Run and is used after an infrastructure failure at the admission
/// boundary.
pub(crate) fn admit_existing<L: HostLauncherLookup>(
    persistence: &PactrunPersistence,
    launcher: &L,
    run: RunId,
    plan: &ActionExecutionPlan,
    options: AdmissionOptions,
) -> Result<AdmittedExecution, ExecutorError> {
    #[cfg(test)]
    if FAIL_NEXT_ADMISSION_FOR_TEST.with(|failed| failed.replace(false)) {
        return Err(ExecutorError::Persistence {
            run: Some(run),
            source: PersistenceError::DatabaseLockPoisoned,
        });
    }
    let facts = AdmissionFacts {
        service: Some((plan.service_hook(), plan.service_bindings())),
        expected_state_version: plan.expected_state_version(),
        active_bindings: plan.active_bindings(),
        runtime_content: plan.runtime_content(),
        launch: plan.launch(),
    };
    let launcher_check =
        |observation: &InterpreterLauncherObservation| reselect_launcher(launcher, observation);
    match persistence
        .admit_run(run, &facts, &launcher_check, options.recovery_override)
        .map_err(|source| ExecutorError::Persistence {
            run: Some(run),
            source,
        })? {
        Ok(()) => Ok(AdmittedExecution {
            run,
            plan: plan.clone(),
        }),
        Err(refusal) => Err(ExecutorError::Refused { run, refusal }),
    }
}

/// The PR-REQ-0194 Admission clause: repeat the ordered selection with the
/// exact search configuration bound into the Plan and require the same
/// candidate path. No object identity, digest, or canonical target is compared.
pub(crate) fn reselect_launcher<L: HostLauncherLookup>(
    launcher: &L,
    observation: &InterpreterLauncherObservation,
) -> Result<(), String> {
    match launcher.resolve(&observation.search_directories, &observation.command) {
        Ok(path) if path == observation.resolved_absolute_path => Ok(()),
        Ok(_) => Err("a different candidate is selected".to_owned()),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn admit_snapshot_existing<L: HostLauncherLookup>(
    persistence: &PactrunPersistence,
    launcher: &L,
    run: RunId,
    owner: &ExecutionOwnerSession,
    plan: &crate::domain::SnapshotExecutionPlan,
    options: AdmissionOptions,
) -> Result<(), ExecutorError> {
    #[cfg(test)]
    if FAIL_NEXT_ADMISSION_FOR_TEST.with(|failed| failed.replace(false)) {
        return Err(ExecutorError::Persistence {
            run: Some(run),
            source: PersistenceError::DatabaseLockPoisoned,
        });
    }
    let launcher_check =
        |observed: &InterpreterLauncherObservation| reselect_launcher(launcher, observed);
    match persistence
        .admit_snapshot_plan(run, owner, plan, &launcher_check, options.recovery_override)
        .map_err(|source| ExecutorError::Persistence {
            run: Some(run),
            source,
        })? {
        Ok(()) => Ok(()),
        Err(refusal) => Err(ExecutorError::Refused { run, refusal }),
    }
}
