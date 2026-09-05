//! Execution-plan runtime ownership.
//!
//! Slice 3 owns Run acceptance and Admission: it turns an ephemeral
//! `ActionExecutionPlan` into a durable Run and either admits it (durable
//! pins) or records the refusal in that same Run. Process launch, Workspace
//! materialization, and HookProtocolV1 runtime belong to later slices.

// Later M3 slices add the production callers.
#![allow(dead_code)]

use std::fmt;

use crate::{
    domain::{
        ActionExecutionPlan, ActionRunIdentity, AdmissionFacts, AdmissionRefusal,
        ExecutionOwnerSession, InterpreterLauncherObservation, RunId,
    },
    persistence::{PactrunPersistence, PersistenceError},
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
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AdmittedExecution {
    run: RunId,
    plan: ActionExecutionPlan,
}

impl AdmittedExecution {
    pub(crate) fn run(&self) -> RunId {
        self.run
    }

    pub(crate) fn plan(&self) -> &ActionExecutionPlan {
        &self.plan
    }
}

#[derive(Debug)]
pub(crate) enum ExecutorError {
    /// Admission refused the Plan; the refusal is already durable as the Run's
    /// terminal `Failed` outcome.
    Refused {
        run: RunId,
        refusal: AdmissionRefusal,
    },
    /// An infrastructure failure. A Run created before the failure remains
    /// Running and Accepted for later reconciliation; it occupies no
    /// exclusivity.
    Persistence(PersistenceError),
}

impl fmt::Display for ExecutorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused { run, refusal } => {
                write!(
                    formatter,
                    "Run {run} was not admitted: {}",
                    refusal.message()
                )
            }
            Self::Persistence(source) => write!(formatter, "persistence: {source}"),
        }
    }
}

impl std::error::Error for ExecutorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Refused { .. } => None,
            Self::Persistence(source) => Some(source),
        }
    }
}

impl From<PersistenceError> for ExecutorError {
    fn from(source: PersistenceError) -> Self {
        Self::Persistence(source)
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
    let run = persistence.create_accepted_run(
        plan.instance(),
        plan.expected_state_version(),
        &ActionRunIdentity {
            revision: plan.active_revision().clone(),
            action: plan.action().clone(),
        },
        owner,
    )?;
    let facts = AdmissionFacts {
        expected_state_version: plan.expected_state_version(),
        active_bindings: plan.active_bindings(),
        runtime_content: plan.runtime_content(),
        launch: plan.launch(),
    };
    let launcher_check =
        |observation: &InterpreterLauncherObservation| reselect_launcher(launcher, observation);
    match persistence.admit_run(run, &facts, &launcher_check, options.recovery_override)? {
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
fn reselect_launcher<L: HostLauncherLookup>(
    launcher: &L,
    observation: &InterpreterLauncherObservation,
) -> Result<(), String> {
    match launcher.resolve(&observation.search_directories, &observation.command) {
        Ok(path) if path == observation.resolved_absolute_path => Ok(()),
        Ok(_) => Err("a different candidate is selected".to_owned()),
        Err(error) => Err(error.to_string()),
    }
}
