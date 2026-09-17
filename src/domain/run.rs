//! Run records, managed execution ownership, and recovery state.
//!
//! These types preserve historical Action meanings and extend them for V5.
//! They carry no Execution Plan and are never a replay contract.

use std::fmt;

use super::{
    ActionIdentity, ActionPlanStep, ActiveInstanceBindingReference, CompiledHookLaunch, HookCodeV1,
    InstanceId, InstanceStateVersion, ManagedOutputIdentity, PactrunErrorRefV1, RevisionIdentity,
    RunId, RuntimeFileV1,
};

const ADMISSION_ERROR_OWNER: &str = "admission";

const SESSION_NAME_PREFIX: &str = "session-";
const SESSION_NAME_HEX_LENGTH: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunRecordError {
    message: String,
}

impl RunRecordError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for RunRecordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RunRecordError {}

macro_rules! closed_rank {
    ($name:ident, $label:literal, { $($variant:ident = $rank:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(crate) enum $name {
            $($variant),+
        }

        impl $name {
            pub(crate) fn rank(self) -> i64 {
                match self {
                    $(Self::$variant => $rank),+
                }
            }

            pub(crate) fn from_rank(rank: i64) -> Result<Self, RunRecordError> {
                match rank {
                    $($rank => Ok(Self::$variant),)+
                    _ => Err(RunRecordError::new(format!(
                        concat!("invalid ", $label, " rank {}"),
                        rank
                    ))),
                }
            }
        }
    };
}

closed_rank!(RunOutcome, "Run outcome", {
    Succeeded = 0,
    Failed = 1,
    Cancelled = 2,
    TimedOut = 3,
    Interrupted = 4,
});

closed_rank!(ActionRunBoundary, "Run boundary", {
    Accepted = 0,
    Admitted = 1,
});

closed_rank!(ManagedExecutionKind, "managed execution kind", {
    Action = 0,
    SnapshotCapture = 1,
    SnapshotRestore = 2,
    Migration = 3,
    Deletion = 4,
});

closed_rank!(RecoveryRiskState, "recovery risk", {
    Clear = 0,
    Open = 1,
});

closed_rank!(ManualRecoveryTrigger, "manual recovery trigger", {
    OpenRiskFailure = 0,
    OpenRiskOwnerLoss = 1,
    SuccessWithOpenRisk = 2,
});

closed_rank!(HookCompletionStatus, "Hook completion status", {
    Success = 0,
    Failure = 1,
});

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RunPhase {
    Running,
    Finished,
}

/// The logical step at which a Pactrun-recorded primary failure occurred.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RunFailedStep {
    Admission,
    Plan(ActionPlanStep),
    SnapshotPlan(SnapshotPlanStep),
    MigrationPlan(super::MigrationPlanStep),
    DeletionPlan(DeletionPlanStep),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeletionPlanStep {
    EstablishSession,
    LaunchCleanup,
    AcceptCompletion,
    AuthorizeFinalization,
    FinalizeStorage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotPlanStep {
    EstablishSession,
    LaunchHook,
    AcceptCompletion,
    PublishManagedResult,
    Finalize,
}

impl RunFailedStep {
    pub(crate) fn rank(self) -> i64 {
        match self {
            Self::DeletionPlan(step) => match step {
                DeletionPlanStep::EstablishSession => 1,
                DeletionPlanStep::LaunchCleanup => 2,
                DeletionPlanStep::AcceptCompletion => 3,
                DeletionPlanStep::AuthorizeFinalization => 4,
                DeletionPlanStep::FinalizeStorage => 5,
            },
            Self::MigrationPlan(step) => step.rank() + 1,
            Self::Admission => 0,
            Self::Plan(ActionPlanStep::EstablishSession) => 1,
            Self::Plan(ActionPlanStep::LaunchHook) => 2,
            Self::Plan(ActionPlanStep::AcceptCompletion) => 3,
            Self::Plan(ActionPlanStep::PublishDeclaredOutputs) => 4,
            Self::Plan(ActionPlanStep::Finalize) => 5,
            Self::SnapshotPlan(SnapshotPlanStep::EstablishSession) => 1,
            Self::SnapshotPlan(SnapshotPlanStep::LaunchHook) => 2,
            Self::SnapshotPlan(SnapshotPlanStep::AcceptCompletion) => 3,
            Self::SnapshotPlan(SnapshotPlanStep::PublishManagedResult) => 4,
            Self::SnapshotPlan(SnapshotPlanStep::Finalize) => 5,
        }
    }

    pub(crate) fn from_rank(rank: i64) -> Result<Self, RunRecordError> {
        match rank {
            0 => Ok(Self::Admission),
            1 => Ok(Self::Plan(ActionPlanStep::EstablishSession)),
            2 => Ok(Self::Plan(ActionPlanStep::LaunchHook)),
            3 => Ok(Self::Plan(ActionPlanStep::AcceptCompletion)),
            4 => Ok(Self::Plan(ActionPlanStep::PublishDeclaredOutputs)),
            5 => Ok(Self::Plan(ActionPlanStep::Finalize)),
            _ => Err(RunRecordError::new(format!(
                "invalid failed step rank {rank}"
            ))),
        }
    }

    pub(crate) fn for_operation(
        rank: i64,
        kind: ManagedExecutionKind,
    ) -> Result<Self, RunRecordError> {
        let action = Self::from_rank(rank)?;
        if kind == ManagedExecutionKind::Action || rank == 0 {
            return Ok(action);
        }
        if kind == ManagedExecutionKind::Migration {
            return super::MigrationPlanStep::from_rank(rank - 1)
                .map(Self::MigrationPlan)
                .map_err(|_| RunRecordError::new("invalid Migration step"));
        }
        if kind == ManagedExecutionKind::Deletion {
            return Ok(Self::DeletionPlan(match rank {
                1 => DeletionPlanStep::EstablishSession,
                2 => DeletionPlanStep::LaunchCleanup,
                3 => DeletionPlanStep::AcceptCompletion,
                4 => DeletionPlanStep::AuthorizeFinalization,
                5 => DeletionPlanStep::FinalizeStorage,
                _ => unreachable!("validated step"),
            }));
        }
        Ok(Self::SnapshotPlan(match rank {
            1 => SnapshotPlanStep::EstablishSession,
            2 => SnapshotPlanStep::LaunchHook,
            3 => SnapshotPlanStep::AcceptCompletion,
            4 => SnapshotPlanStep::PublishManagedResult,
            5 => SnapshotPlanStep::Finalize,
            _ => unreachable!("validated historical step rank"),
        }))
    }
}

/// The exact staging session directory name that owns a Running Run.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ExecutionOwnerSession(String);

impl ExecutionOwnerSession {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, RunRecordError> {
        let value = value.into();
        if !Self::is_session_name(&value) {
            return Err(RunRecordError::new("invalid execution owner session name"));
        }
        Ok(Self(value))
    }

    pub(crate) fn is_session_name(name: &str) -> bool {
        name.len() == SESSION_NAME_PREFIX.len() + SESSION_NAME_HEX_LENGTH
            && name.starts_with(SESSION_NAME_PREFIX)
            && name[SESSION_NAME_PREFIX.len()..]
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunFailureRecord {
    pub(crate) error: PactrunErrorRefV1,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunPrimaryFailure {
    pub(crate) failure: RunFailureRecord,
    pub(crate) step: RunFailedStep,
}

/// The Hook-owned completion result. `message: Some(String::new())` is a
/// present empty message and is distinct from `None`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HookCompletionRecord {
    pub(crate) status: HookCompletionStatus,
    pub(crate) code: Option<HookCodeV1>,
    pub(crate) message: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ActionRunIdentity {
    pub(crate) revision: RevisionIdentity,
    pub(crate) action: ActionIdentity,
}

/// Exact persisted operation identity. Access is deliberately not a field:
/// it is resolved against the operation's authoritative Revision declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ManagedRunIdentity {
    Deletion {
        revision: RevisionIdentity,
        mode: super::DeletionMode,
    },
    Migration(super::MigrationRunIdentity),
    Action(ActionRunIdentity),
    Capture {
        revision: RevisionIdentity,
    },
    Restore {
        revision: RevisionIdentity,
        snapshot: super::SnapshotId,
    },
}

impl ManagedRunIdentity {
    pub(crate) fn kind(&self) -> ManagedExecutionKind {
        match self {
            Self::Deletion { .. } => ManagedExecutionKind::Deletion,
            Self::Migration(_) => ManagedExecutionKind::Migration,
            Self::Action(_) => ManagedExecutionKind::Action,
            Self::Capture { .. } => ManagedExecutionKind::SnapshotCapture,
            Self::Restore { .. } => ManagedExecutionKind::SnapshotRestore,
        }
    }
    pub(crate) fn revision(&self) -> &RevisionIdentity {
        match self {
            Self::Migration(migration) => migration.source(),
            Self::Action(action) => &action.revision,
            Self::Capture { revision }
            | Self::Restore { revision, .. }
            | Self::Deletion { revision, .. } => revision,
        }
    }

    pub(crate) fn authoritative_access(
        &self,
        core: &super::RevisionCoreV1,
    ) -> Result<super::OperationAccessV1, RunRecordError> {
        match self {
            Self::Migration(_) | Self::Deletion { .. } => Some(super::OperationAccessV1::Mutate),
            Self::Action(action) => core
                .actions()
                .iter()
                .find(|declaration| declaration.id == action.action)
                .map(|declaration| declaration.access),
            Self::Capture { .. } => core
                .snapshot()
                .and_then(|snapshot| snapshot.capture.as_ref())
                .map(|capture| capture.access),
            Self::Restore { .. } => core
                .snapshot()
                .and_then(|snapshot| snapshot.restore.as_ref())
                .map(|_| super::OperationAccessV1::Mutate),
        }
        .ok_or_else(|| RunRecordError::new("Run operation is not declared by its exact Revision"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunExecutionView {
    pub(crate) owner: ExecutionOwnerSession,
    pub(crate) boundary: ActionRunBoundary,
    pub(crate) risk_state: RecoveryRiskState,
}

/// Shared Running-state matrix; the historical function name is retained for
/// Action callers. Risk is not legal before the Admission boundary exists.
pub(crate) fn validate_running_action_state(
    boundary: ActionRunBoundary,
    risk_state: RecoveryRiskState,
) -> Result<(), RunRecordError> {
    match (boundary, risk_state) {
        (ActionRunBoundary::Accepted, RecoveryRiskState::Clear)
        | (ActionRunBoundary::Admitted, RecoveryRiskState::Clear)
        | (ActionRunBoundary::Admitted, RecoveryRiskState::Open) => Ok(()),
        (ActionRunBoundary::Accepted, RecoveryRiskState::Open) => Err(RunRecordError::new(
            "a Running managed execution cannot have open recovery risk before Admission",
        )),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunArtifactSummary {
    pub(crate) output: ManagedOutputIdentity,
    pub(crate) byte_length: u64,
}

/// Exact Artifact representation limit; independent of Managed Input policy.
pub(crate) const RUN_ARTIFACT_MAX_BYTES_V1: u64 = 536_870_912;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunOutcomeView {
    pub(crate) outcome: RunOutcome,
    pub(crate) boundary: ActionRunBoundary,
    pub(crate) terminal_risk: RecoveryRiskState,
    pub(crate) finished_at_unix_ms: u64,
    pub(crate) primary_failure: Option<RunPrimaryFailure>,
    pub(crate) secondary_failures: Vec<RunFailureRecord>,
    pub(crate) hook_completion: Option<HookCompletionRecord>,
    pub(crate) artifacts: Vec<RunArtifactSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RunState {
    Running(RunExecutionView),
    Finished(RunOutcomeView),
}

impl RunState {
    pub(crate) fn phase(&self) -> RunPhase {
        match self {
            Self::Running(_) => RunPhase::Running,
            Self::Finished(_) => RunPhase::Finished,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunView {
    pub(crate) id: RunId,
    pub(crate) instance: InstanceId,
    pub(crate) accepted_state_version: InstanceStateVersion,
    pub(crate) accepted_at_unix_ms: u64,
    pub(crate) action: ActionRunIdentity,
    pub(crate) state: RunState,
}

/// Operation-neutral durable view. The Action view remains a compatibility
/// projection for the existing M3 callers, never a Snapshot replay contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManagedRunView {
    pub(crate) id: RunId,
    pub(crate) instance: InstanceId,
    pub(crate) accepted_state_version: InstanceStateVersion,
    pub(crate) accepted_at_unix_ms: u64,
    pub(crate) operation: ManagedRunIdentity,
    pub(crate) state: RunState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManagedRunInspectionData {
    pub(crate) run: ManagedRunView,
    pub(crate) current_recovery_guard: Option<RecoveryGuardView>,
    pub(crate) capture_result: Option<super::SnapshotId>,
    pub(crate) migration_progress: Option<super::MigrationRunProgress>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PinnedInputBinding {
    pub(crate) input: super::InputIdentity,
    pub(crate) role: super::ManagedInputRole,
    pub(crate) payload: Option<super::ManagedInputPayloadId>,
    pub(crate) protection: super::ManagedInputProtection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunInspectionData {
    pub(crate) run: RunView,
    pub(crate) current_recovery_guard: Option<RecoveryGuardView>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunSummary {
    pub(crate) id: RunId,
    pub(crate) instance: InstanceId,
    pub(crate) action: ActionRunIdentity,
    pub(crate) phase: RunPhase,
    pub(crate) outcome: Option<RunOutcome>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryGuardView {
    pub(crate) instance: InstanceId,
    pub(crate) run: RunId,
    pub(crate) trigger: ManualRecoveryTrigger,
    pub(crate) entered_at_unix_ms: u64,
}

/// The terminal facts a caller submits when finishing a Run. Artifact bytes
/// are supplied separately by the persistence write contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunFinish {
    pub(crate) outcome: RunOutcome,
    pub(crate) primary_failure: Option<RunPrimaryFailure>,
    pub(crate) secondary_failures: Vec<RunFailureRecord>,
    pub(crate) hook_completion: Option<HookCompletionRecord>,
}

/// The compile-time facts an Admission transaction revalidates against
/// persisted state. Access mode and readiness are deliberately absent: both
/// are re-derived from persisted authority inside the transaction.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AdmissionFacts<'a> {
    pub(crate) expected_state_version: InstanceStateVersion,
    pub(crate) active_bindings: &'a [ActiveInstanceBindingReference],
    pub(crate) runtime_content: &'a [RuntimeFileV1],
    pub(crate) launch: &'a CompiledHookLaunch,
    /// Compiled association facts, not caller-granted authority. Admission
    /// re-derives the declaration and mapping inside its write transaction.
    pub(crate) service: Option<(
        &'a super::HookServiceContractV2,
        &'a super::ServiceHookBindings,
    )>,
}

/// The typed Admission refusal, evaluated in the precedence of PR-REQ-0279.
/// The conflicting `RunId` is typed detail; messages carry no normative
/// content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AdmissionRefusal {
    RecoveryGuardActive,
    PlanInvalidated(String),
    MutationConflict(RunId),
}

impl AdmissionRefusal {
    pub(crate) fn error_ref(&self) -> PactrunErrorRefV1 {
        let code = match self {
            Self::RecoveryGuardActive => "recovery_guard_active",
            Self::PlanInvalidated(_) => "plan_invalidated",
            Self::MutationConflict(_) => "mutation_conflict",
        };
        PactrunErrorRefV1::new(ADMISSION_ERROR_OWNER, code)
            .expect("admission error identities are lexically valid")
    }

    pub(crate) fn message(&self) -> String {
        match self {
            Self::RecoveryGuardActive => {
                "the Instance is in ManualRecoveryRequired and no recovery override was supplied"
                    .to_owned()
            }
            Self::PlanInvalidated(reason) => format!("the Plan is invalidated: {reason}"),
            Self::MutationConflict(_) => {
                "another Mutate execution is admitted on this Instance".to_owned()
            }
        }
    }

    pub(crate) fn primary_failure(&self) -> RunPrimaryFailure {
        RunPrimaryFailure {
            failure: RunFailureRecord {
                error: self.error_ref(),
                message: self.message(),
            },
            step: RunFailedStep::Admission,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RunTransitionError {
    SuccessWithOpenRisk,
    RiskAlreadyOpen,
    RiskAlreadyClear,
}

impl fmt::Display for RunTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SuccessWithOpenRisk => {
                formatter.write_str("a Run cannot succeed while recovery risk is open")
            }
            Self::RiskAlreadyOpen => formatter.write_str("recovery risk is already open"),
            Self::RiskAlreadyClear => formatter.write_str("recovery risk is already clear"),
        }
    }
}

impl std::error::Error for RunTransitionError {}

/// Validates one durable risk transition; only `Clear -> Open` and
/// `Open -> Clear` exist.
pub(crate) fn risk_transition(
    current: RecoveryRiskState,
    requested: RecoveryRiskState,
) -> Result<RecoveryRiskState, RunTransitionError> {
    match (current, requested) {
        (RecoveryRiskState::Clear, RecoveryRiskState::Open) => Ok(RecoveryRiskState::Open),
        (RecoveryRiskState::Open, RecoveryRiskState::Clear) => Ok(RecoveryRiskState::Clear),
        (RecoveryRiskState::Open, RecoveryRiskState::Open) => {
            Err(RunTransitionError::RiskAlreadyOpen)
        }
        (RecoveryRiskState::Clear, RecoveryRiskState::Clear) => {
            Err(RunTransitionError::RiskAlreadyClear)
        }
    }
}

/// Derives the Instance consequence of a terminal publication.
///
/// Precedence is fixed: clear risk never produces a consequence; open risk
/// with `Succeeded` is invalid; open risk with a successful Hook completion is
/// `SuccessWithOpenRisk`; otherwise open risk with `Interrupted` is owner loss
/// and every remaining open non-success is an open-risk failure.
pub(crate) fn terminal_consequence(
    outcome: RunOutcome,
    live_risk: RecoveryRiskState,
    hook_completion: Option<&HookCompletionRecord>,
) -> Result<Option<ManualRecoveryTrigger>, RunTransitionError> {
    if live_risk == RecoveryRiskState::Clear {
        return Ok(None);
    }
    if outcome == RunOutcome::Succeeded {
        return Err(RunTransitionError::SuccessWithOpenRisk);
    }
    if hook_completion.is_some_and(|completion| completion.status == HookCompletionStatus::Success)
    {
        return Ok(Some(ManualRecoveryTrigger::SuccessWithOpenRisk));
    }
    if outcome == RunOutcome::Interrupted {
        return Ok(Some(ManualRecoveryTrigger::OpenRiskOwnerLoss));
    }
    Ok(Some(ManualRecoveryTrigger::OpenRiskFailure))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completion(status: HookCompletionStatus) -> HookCompletionRecord {
        HookCompletionRecord {
            status,
            code: None,
            message: None,
        }
    }

    // Supporting domain coverage for PR-TEST-0084 and PR-TEST-0085.
    #[test]
    fn closed_ranks_round_trip_and_reject_foreign_values() {
        for rank in 0..5 {
            assert_eq!(RunOutcome::from_rank(rank).unwrap().rank(), rank);
        }
        assert!(RunOutcome::from_rank(5).is_err());
        assert!(RunOutcome::from_rank(-1).is_err());
        for rank in 0..6 {
            assert_eq!(RunFailedStep::from_rank(rank).unwrap().rank(), rank);
        }
        assert!(RunFailedStep::from_rank(6).is_err());
        assert!(ManualRecoveryTrigger::from_rank(3).is_err());
        assert!(ActionRunBoundary::from_rank(2).is_err());
        assert!(RecoveryRiskState::from_rank(2).is_err());
        assert!(HookCompletionStatus::from_rank(2).is_err());
        assert!(ExecutionOwnerSession::parse(format!("session-{}", "0".repeat(32))).is_ok());
        assert!(ExecutionOwnerSession::parse(format!("session-{}", "A".repeat(32))).is_err());
        assert!(ExecutionOwnerSession::parse("session-").is_err());
        assert!(ExecutionOwnerSession::parse("../session-0").is_err());
    }

    // Supporting domain coverage for PR-TEST-0085.
    #[test]
    fn terminal_consequence_precedence_is_exact() {
        let success = completion(HookCompletionStatus::Success);
        let failure = completion(HookCompletionStatus::Failure);
        for outcome in [
            RunOutcome::Succeeded,
            RunOutcome::Failed,
            RunOutcome::Cancelled,
            RunOutcome::TimedOut,
            RunOutcome::Interrupted,
        ] {
            for hook in [None, Some(&success), Some(&failure)] {
                assert_eq!(
                    terminal_consequence(outcome, RecoveryRiskState::Clear, hook),
                    Ok(None)
                );
            }
        }
        for hook in [None, Some(&success), Some(&failure)] {
            assert_eq!(
                terminal_consequence(RunOutcome::Succeeded, RecoveryRiskState::Open, hook),
                Err(RunTransitionError::SuccessWithOpenRisk)
            );
        }
        for outcome in [
            RunOutcome::Failed,
            RunOutcome::Cancelled,
            RunOutcome::TimedOut,
            RunOutcome::Interrupted,
        ] {
            assert_eq!(
                terminal_consequence(outcome, RecoveryRiskState::Open, Some(&success)),
                Ok(Some(ManualRecoveryTrigger::SuccessWithOpenRisk))
            );
        }
        for hook in [None, Some(&failure)] {
            assert_eq!(
                terminal_consequence(RunOutcome::Interrupted, RecoveryRiskState::Open, hook),
                Ok(Some(ManualRecoveryTrigger::OpenRiskOwnerLoss))
            );
            for outcome in [
                RunOutcome::Failed,
                RunOutcome::Cancelled,
                RunOutcome::TimedOut,
            ] {
                assert_eq!(
                    terminal_consequence(outcome, RecoveryRiskState::Open, hook),
                    Ok(Some(ManualRecoveryTrigger::OpenRiskFailure))
                );
            }
        }
        assert_eq!(
            risk_transition(RecoveryRiskState::Clear, RecoveryRiskState::Open),
            Ok(RecoveryRiskState::Open)
        );
        assert_eq!(
            risk_transition(RecoveryRiskState::Open, RecoveryRiskState::Clear),
            Ok(RecoveryRiskState::Clear)
        );
        assert_eq!(
            risk_transition(RecoveryRiskState::Open, RecoveryRiskState::Open),
            Err(RunTransitionError::RiskAlreadyOpen)
        );
        assert_eq!(
            risk_transition(RecoveryRiskState::Clear, RecoveryRiskState::Clear),
            Err(RunTransitionError::RiskAlreadyClear)
        );
    }

    // Test-ID: PR-TEST-0104
    // Verifies: PR-REQ-0275
    #[test]
    fn running_action_state_matrix_rejects_accepted_open_risk() {
        assert!(
            validate_running_action_state(ActionRunBoundary::Accepted, RecoveryRiskState::Clear,)
                .is_ok()
        );
        assert!(
            validate_running_action_state(ActionRunBoundary::Admitted, RecoveryRiskState::Clear,)
                .is_ok()
        );
        assert!(
            validate_running_action_state(ActionRunBoundary::Admitted, RecoveryRiskState::Open,)
                .is_ok()
        );
        assert!(
            validate_running_action_state(ActionRunBoundary::Accepted, RecoveryRiskState::Open,)
                .is_err()
        );
    }
}
