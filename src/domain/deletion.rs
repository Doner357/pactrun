//! Instance retirement policy. Evidence is not a replayable execution Plan.

use super::{InstanceId, InstanceStateVersion, RecoveryRiskState, RunId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DetachedAllocationState {
    Preserved,
    DiscardPending,
    Discarded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DetachedAllocationView {
    pub(crate) allocation: super::ServiceAllocationId,
    pub(crate) instance: InstanceId,
    pub(crate) former_name: super::InstanceName,
    pub(crate) origin_revision: super::RevisionIdentity,
    pub(crate) origin_storage: super::ServiceStorageIdentity,
    pub(crate) state: DetachedAllocationState,
}

#[derive(Clone, Debug)]
pub(crate) struct DeleteInstance {
    pub(crate) instance: InstanceId,
    pub(crate) expected: InstanceStateVersion,
    pub(crate) mode: DeletionMode,
}

pub(crate) struct DeletionCompilationObservation {
    pub(crate) instance: super::InstanceView,
    pub(crate) revision_identity: super::RevisionIdentity,
    pub(crate) revision: super::ValidatedRevisionContent,
    pub(crate) service_state: Option<super::InstanceServiceState>,
    pub(crate) obligation: Option<DeletionObligation>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DeletionWork {
    Abandon,
    NoCleanup,
    FinalizationOnly { attempt: RunId },
    Cleanup(Box<CompiledCleanup>),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompiledCleanup {
    pub(crate) hook: super::HookV1,
    pub(crate) launch: super::CompiledHookLaunch,
    pub(crate) bindings: Vec<super::ManagedInputBindingView>,
    pub(crate) runtime_content: Vec<super::RuntimeFileV1>,
    pub(crate) service_contract: super::HookServiceContractV2,
    pub(crate) service_bindings: super::ServiceHookBindings,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DeletionPlan {
    pub(crate) instance: InstanceId,
    pub(crate) expected: InstanceStateVersion,
    pub(crate) revision: super::RevisionIdentity,
    pub(crate) mode: DeletionMode,
    pub(crate) work: DeletionWork,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DeletionPlanError {
    ChangedObservation,
    MissingRequirement(super::InputBindingRefV1),
    UnresolvedAttempt(RunId),
    InvalidLaunch,
    ServiceBindings,
}

impl std::fmt::Display for DeletionPlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::ChangedObservation => "deletion observation changed",
            Self::MissingRequirement(_) => {
                "a Cleanup requirement is not bound with its declared role"
            }
            Self::UnresolvedAttempt(_) => {
                "Cleanup result is unresolved; externally verify completion or abandon management"
            }
            Self::InvalidLaunch => "Cleanup launcher or runtime content is invalid",
            Self::ServiceBindings => "Cleanup service associations changed",
        })
    }
}
impl std::error::Error for DeletionPlanError {}

/// Pure contract compilation. In particular, Abandon and no-Hook finalization
/// must not be forced through a fabricated launcher or ordinary readiness.
pub(crate) fn build_deletion_plan(
    intent: &DeleteInstance,
    observation: DeletionCompilationObservation,
    launch: Option<super::CompiledHookLaunch>,
) -> Result<DeletionPlan, DeletionPlanError> {
    use super::{InputBindingRoleV1, ManagedInputRole};
    if intent.instance != observation.instance.id
        || intent.expected != observation.instance.state_version
        || observation.revision_identity != observation.instance.active_revision
        || observation
            .obligation
            .is_some_and(|o| o.instance != intent.instance)
    {
        return Err(DeletionPlanError::ChangedObservation);
    }
    let work = if intent.mode == DeletionMode::AbandonManagement {
        DeletionWork::Abandon
    } else if let Some(obligation) = observation.obligation {
        if obligation.phase != DeletionPhase::FinalizationAuthorized {
            return Err(DeletionPlanError::UnresolvedAttempt(obligation.attempt));
        }
        DeletionWork::FinalizationOnly {
            attempt: obligation.attempt,
        }
    } else if let Some(cleanup) = observation.revision.core.cleanup() {
        for requirement in &cleanup.requires {
            let found = observation.instance.bindings.iter().any(|binding| {
                binding.input_id == requirement.input_id
                    && binding.present
                    && matches!(
                        (requirement.role, binding.role),
                        (InputBindingRoleV1::Active, ManagedInputRole::Active { .. })
                            | (InputBindingRoleV1::Retained, ManagedInputRole::Retained)
                    )
            });
            if !found {
                return Err(DeletionPlanError::MissingRequirement(requirement.clone()));
            }
        }
        let launch = launch.ok_or(DeletionPlanError::InvalidLaunch)?;
        if !matches!(cleanup.hook.protocol_version.get(), 1 | 2) {
            return Err(DeletionPlanError::InvalidLaunch);
        }
        let runtime = observation.revision.runtime_content.files();
        let launch_matches = match (&cleanup.hook.launch, &launch) {
            (
                super::HookLaunchV1::Direct { executable },
                super::CompiledHookLaunch::Direct { executable: file },
            ) => &file.id == executable && runtime.contains(file) && file.executable,
            (
                super::HookLaunchV1::Interpreter {
                    command,
                    interpreter_args,
                    script,
                },
                super::CompiledHookLaunch::Interpreter {
                    launcher,
                    interpreter_args: args,
                    script: file,
                },
            ) => {
                command == &launcher.command
                    && interpreter_args == args
                    && script == &file.id
                    && runtime.contains(file)
            }
            _ => false,
        };
        if !launch_matches {
            return Err(DeletionPlanError::InvalidLaunch);
        }
        let service_contract = observation
            .revision
            .core
            .service_hook(&super::ServiceHookSite::Cleanup);
        let service_bindings = super::bind_current_service_hook(
            &service_contract,
            observation.service_state.as_ref(),
            intent.instance,
            intent.expected,
            &observation.instance.active_revision,
        )
        .map_err(|_| DeletionPlanError::ServiceBindings)?;
        DeletionWork::Cleanup(Box::new(CompiledCleanup {
            hook: cleanup.hook.clone(),
            launch,
            bindings: observation.instance.bindings,
            runtime_content: observation.revision.runtime_content.files().to_vec(),
            service_contract,
            service_bindings,
        }))
    } else {
        DeletionWork::NoCleanup
    };
    Ok(DeletionPlan {
        instance: intent.instance,
        expected: intent.expected,
        revision: observation.instance.active_revision,
        mode: intent.mode,
        work,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeletionMode {
    ManagedCleanup,
    AbandonManagement,
}

impl DeletionMode {
    pub(crate) fn rank(self) -> i64 {
        match self {
            Self::ManagedCleanup => 0,
            Self::AbandonManagement => 1,
        }
    }

    pub(crate) fn from_rank(rank: i64) -> Result<Self, DeletionError> {
        match rank {
            0 => Ok(Self::ManagedCleanup),
            1 => Ok(Self::AbandonManagement),
            _ => Err(DeletionError::InvalidEvidence),
        }
    }
}

/// Durable lifecycle obligations, not public Instance lifecycle states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeletionPhase {
    LaunchAuthorized,
    ResultUnresolved,
    FinalizationAuthorized,
}

impl DeletionPhase {
    pub(crate) fn rank(self) -> i64 {
        match self {
            Self::LaunchAuthorized => 0,
            Self::ResultUnresolved => 1,
            Self::FinalizationAuthorized => 2,
        }
    }

    pub(crate) fn from_rank(rank: i64) -> Result<Self, DeletionError> {
        match rank {
            0 => Ok(Self::LaunchAuthorized),
            1 => Ok(Self::ResultUnresolved),
            2 => Ok(Self::FinalizationAuthorized),
            _ => Err(DeletionError::InvalidEvidence),
        }
    }

    pub(crate) fn after_owner_loss(self) -> Self {
        match self {
            Self::LaunchAuthorized => Self::ResultUnresolved,
            phase => phase,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FinalizationAuthority {
    HookCompleted,
    OperatorConfirmed,
    NoCleanupDeclared,
}

impl FinalizationAuthority {
    pub(crate) fn rank(self) -> i64 {
        match self {
            Self::HookCompleted => 0,
            Self::OperatorConfirmed => 1,
            Self::NoCleanupDeclared => 2,
        }
    }

    pub(crate) fn from_rank(rank: i64) -> Result<Self, DeletionError> {
        match rank {
            0 => Ok(Self::HookCompleted),
            1 => Ok(Self::OperatorConfirmed),
            2 => Ok(Self::NoCleanupDeclared),
            _ => Err(DeletionError::InvalidEvidence),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DeletionObligation {
    pub(crate) instance: InstanceId,
    pub(crate) attempt: RunId,
    pub(crate) phase: DeletionPhase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CleanupConfirmation {
    pub(crate) instance: InstanceId,
    pub(crate) attempt: RunId,
    pub(crate) expected: InstanceStateVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeletionError {
    InvalidEvidence,
    ResultUnresolved,
    FinalizationPending,
    StaleConfirmation,
    OpenRisk,
}

/// Clear risk never supplies missing evidence that Cleanup completed.
pub(crate) fn authorize_hook_finalization(
    success: bool,
    risk: RecoveryRiskState,
) -> Result<FinalizationAuthority, DeletionError> {
    if risk != RecoveryRiskState::Clear {
        return Err(DeletionError::OpenRisk);
    }
    if !success {
        return Err(DeletionError::InvalidEvidence);
    }
    Ok(FinalizationAuthority::HookCompleted)
}

pub(crate) fn authorize_confirmation(
    obligation: DeletionObligation,
    current: InstanceStateVersion,
    confirmation: CleanupConfirmation,
) -> Result<FinalizationAuthority, DeletionError> {
    if obligation.instance != confirmation.instance
        || obligation.attempt != confirmation.attempt
        || current != confirmation.expected
    {
        return Err(DeletionError::StaleConfirmation);
    }
    if obligation.phase != DeletionPhase::ResultUnresolved {
        return Err(DeletionError::InvalidEvidence);
    }
    Ok(FinalizationAuthority::OperatorConfirmed)
}

/// An ordinary recovery override must never bypass a deletion obligation.
pub(crate) fn require_no_deletion_obligation(
    obligation: Option<DeletionObligation>,
) -> Result<(), DeletionError> {
    match obligation.map(|o| o.phase) {
        None => Ok(()),
        Some(DeletionPhase::FinalizationAuthorized) => Err(DeletionError::FinalizationPending),
        Some(_) => Err(DeletionError::ResultUnresolved),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test-ID: PR-TEST-0389
    // Verifies: PR-REQ-0334
    #[test]
    fn owner_loss_never_authorizes_cleanup_replay_or_infers_success() {
        assert_eq!(
            DeletionPhase::LaunchAuthorized.after_owner_loss(),
            DeletionPhase::ResultUnresolved
        );
        assert_eq!(
            DeletionPhase::ResultUnresolved.after_owner_loss(),
            DeletionPhase::ResultUnresolved
        );
        assert_eq!(
            DeletionPhase::FinalizationAuthorized.after_owner_loss(),
            DeletionPhase::FinalizationAuthorized
        );
        for success in [false, true] {
            assert_eq!(
                authorize_hook_finalization(success, RecoveryRiskState::Open),
                Err(DeletionError::OpenRisk)
            );
        }
        assert!(authorize_hook_finalization(false, RecoveryRiskState::Clear).is_err());
        assert_eq!(
            authorize_hook_finalization(true, RecoveryRiskState::Clear),
            Ok(FinalizationAuthority::HookCompleted)
        );
    }

    // Test-ID: PR-TEST-0390
    // Verifies: PR-REQ-0335
    #[test]
    fn operator_assertion_is_exact_and_is_not_hook_success() {
        let instance = InstanceId::from_bytes([1; 16]);
        let attempt = RunId::from_bytes([2; 16]);
        let expected = InstanceStateVersion::from_bytes([3; 16]);
        let obligation = DeletionObligation {
            instance,
            attempt,
            phase: DeletionPhase::ResultUnresolved,
        };
        let confirmation = CleanupConfirmation {
            instance,
            attempt,
            expected,
        };
        assert_eq!(
            authorize_confirmation(obligation, expected, confirmation),
            Ok(FinalizationAuthority::OperatorConfirmed)
        );
        assert!(
            authorize_confirmation(
                obligation,
                InstanceStateVersion::from_bytes([4; 16]),
                confirmation
            )
            .is_err()
        );
        assert!(
            authorize_confirmation(
                obligation,
                expected,
                CleanupConfirmation {
                    attempt: RunId::from_bytes([5; 16]),
                    ..confirmation
                }
            )
            .is_err()
        );
        assert!(
            authorize_confirmation(
                obligation,
                expected,
                CleanupConfirmation {
                    instance: InstanceId::from_bytes([6; 16]),
                    ..confirmation
                }
            )
            .is_err()
        );
        assert!(
            authorize_confirmation(
                DeletionObligation {
                    phase: DeletionPhase::LaunchAuthorized,
                    ..obligation
                },
                expected,
                confirmation
            )
            .is_err()
        );
        assert!(require_no_deletion_obligation(Some(obligation)).is_err());
        assert!(require_no_deletion_obligation(None).is_ok());
    }

    // Test-ID: PR-TEST-0391
    // Verifies: PR-REQ-0334
    #[test]
    fn persisted_discriminants_are_closed() {
        for rank in [-1, 3, 99] {
            assert!(DeletionMode::from_rank(rank).is_err());
            assert!(DeletionPhase::from_rank(rank).is_err());
            assert!(FinalizationAuthority::from_rank(rank).is_err());
        }
        for rank in 0..=2 {
            assert_eq!(DeletionPhase::from_rank(rank).unwrap().rank(), rank);
            assert_eq!(FinalizationAuthority::from_rank(rank).unwrap().rank(), rank);
        }
        for rank in 0..=1 {
            assert_eq!(DeletionMode::from_rank(rank).unwrap().rank(), rank);
        }
    }
}
