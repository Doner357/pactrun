//! Snapshot execution contracts. No transport, persistence, or replay authority.
use super::*;
use std::fmt;

/// A Frozen Capture submission carries a logical service key and a Session
/// locator, never a caller-authoritative digest or a persistent source identity.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct CaptureServiceContentSubmission {
    pub(crate) role: SnapshotServiceRole,
    pub(crate) path: SnapshotContentPath,
    pub(crate) candidate_path: RuntimePath,
}
impl fmt::Debug for CaptureServiceContentSubmission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CaptureServiceContentSubmission(<owner-only>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotOperation {
    Capture,
    Restore(SnapshotId),
}

#[derive(Clone, Debug)]
pub(crate) struct SnapshotIntent {
    pub(crate) instance: InstanceId,
    pub(crate) operation: SnapshotOperation,
    pub(crate) parameters: Vec<RawParameterInput>,
}

pub(crate) struct SnapshotCompilationObservation {
    pub(crate) instance: InstanceView,
    pub(crate) active_bindings: Vec<ActiveInstanceBindingReference>,
    pub(crate) revision: ValidatedRevisionContentV1,
    pub(crate) snapshot: Option<SnapshotManifest>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SnapshotExecutionPlan {
    instance: InstanceId,
    state: InstanceStateVersion,
    operation: ManagedRunIdentity,
    access: OperationAccessV1,
    parameters: Vec<ParameterBinding>,
    active_bindings: Vec<ActiveInstanceBindingReference>,
    runtime: Vec<RuntimeFileV1>,
    hook: HookV1,
    launch: CompiledHookLaunch,
}

impl SnapshotExecutionPlan {
    pub(crate) fn steps(&self) -> &'static [SnapshotPlanStep] {
        &[
            SnapshotPlanStep::EstablishSession,
            SnapshotPlanStep::LaunchHook,
            SnapshotPlanStep::AcceptCompletion,
            SnapshotPlanStep::PublishManagedResult,
            SnapshotPlanStep::Finalize,
        ]
    }
    pub(crate) fn instance(&self) -> InstanceId {
        self.instance
    }
    pub(crate) fn expected_state_version(&self) -> InstanceStateVersion {
        self.state
    }
    pub(crate) fn operation(&self) -> &ManagedRunIdentity {
        &self.operation
    }
    pub(crate) fn access(&self) -> OperationAccessV1 {
        self.access
    }
    pub(crate) fn parameters(&self) -> &[ParameterBinding] {
        &self.parameters
    }
    pub(crate) fn hook(&self) -> &HookV1 {
        &self.hook
    }
    pub(crate) fn launch(&self) -> &CompiledHookLaunch {
        &self.launch
    }
    pub(crate) fn runtime_content(&self) -> &[RuntimeFileV1] {
        &self.runtime
    }
    pub(crate) fn admission_facts(&self) -> AdmissionFacts<'_> {
        AdmissionFacts {
            expected_state_version: self.state,
            active_bindings: &self.active_bindings,
            runtime_content: &self.runtime,
            launch: &self.launch,
        }
    }
}

#[derive(Debug)]
pub(crate) enum SnapshotPlanError {
    Parameters(ActionResolutionError),
    MissingRequired(Vec<InputIdentity>),
    Invalid(&'static str),
}
impl fmt::Display for SnapshotPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parameters(error) => error.fmt(f),
            Self::MissingRequired(ids) => write!(
                f,
                "required Inputs are not bound: {}",
                ids.iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Invalid(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for SnapshotPlanError {}

pub(crate) fn snapshot_hook(
    core: &RevisionCoreV1,
    operation: SnapshotOperation,
) -> Result<(&[ParameterV1], &HookV1), SnapshotPlanError> {
    let capability = core.snapshot().ok_or(SnapshotPlanError::Invalid(
        "Revision does not declare Snapshot capability",
    ))?;
    match operation {
        SnapshotOperation::Capture => capability
            .capture
            .as_ref()
            .map(|c| (c.parameters.as_slice(), &c.hook)),
        SnapshotOperation::Restore(_) => capability
            .restore
            .as_ref()
            .map(|r| (r.parameters.as_slice(), &r.hook)),
    }
    .ok_or(SnapshotPlanError::Invalid(
        "Revision does not declare the selected Snapshot operation",
    ))
}

/// Only removals/transitions are checked here; target completeness is not a
/// Restore prerequisite. The manifest separately validates its producer view.
pub(crate) fn validate_restore_transition(
    target: &[ManagedInputBindingView],
    snapshot: &SnapshotManifest,
) -> Result<(), SnapshotPlanError> {
    let selected = snapshot
        .managed_bindings()
        .iter()
        .map(|b| (&b.input_id, b))
        .collect::<std::collections::BTreeMap<_, _>>();
    for binding in target {
        let bound = selected
            .get(&binding.input_id)
            .filter(|b| matches!(b.state, SnapshotBindingState::Bound(_)));
        if binding.present
            && binding.protection == ManagedInputProtection::Secret
            && bound.is_some_and(|b| b.protection == ManagedInputProtection::Normal)
        {
            return Err(SnapshotPlanError::Invalid(
                "Restore cannot replace a Secret binding with Normal protection",
            ));
        }
        if binding.present
            && binding.role == (ManagedInputRole::Active { required: true })
            && bound.is_none()
        {
            return Err(SnapshotPlanError::Invalid(
                "Restore cannot remove a bound required active Input",
            ));
        }
    }
    Ok(())
}

pub(crate) fn build_snapshot_plan(
    intent: &SnapshotIntent,
    observation: SnapshotCompilationObservation,
    launch: CompiledHookLaunch,
) -> Result<SnapshotExecutionPlan, SnapshotPlanError> {
    if intent.instance != observation.instance.id {
        return Err(SnapshotPlanError::Invalid(
            "Snapshot compilation observations changed",
        ));
    }
    let core = &observation.revision.core;
    let (parameters, hook) = snapshot_hook(core, intent.operation)?;
    if hook.protocol_version.get() != 1 {
        return Err(SnapshotPlanError::Invalid(
            "Snapshot Hook protocol is unsupported",
        ));
    }
    let parameters = bind_operation_parameters(parameters, intent.parameters.clone())
        .map_err(SnapshotPlanError::Parameters)?;
    let operation = match intent.operation {
        SnapshotOperation::Capture => {
            let missing = observation
                .instance
                .bindings
                .iter()
                .filter(|b| b.role == (ManagedInputRole::Active { required: true }) && !b.present)
                .map(|b| b.input_id.clone())
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                return Err(SnapshotPlanError::MissingRequired(missing));
            }
            if !observation.instance.required_inputs_satisfied {
                return Err(SnapshotPlanError::Invalid(
                    "Capture requires complete active inputs",
                ));
            }
            ManagedRunIdentity::Capture {
                revision: observation.instance.active_revision.clone(),
            }
        }
        SnapshotOperation::Restore(id) => {
            let snapshot = observation
                .snapshot
                .as_ref()
                .ok_or(SnapshotPlanError::Invalid(
                    "selected Snapshot is unavailable",
                ))?;
            if snapshot.snapshot_id() != id
                || snapshot.producer() != &observation.instance.active_revision
            {
                return Err(SnapshotPlanError::Invalid(
                    "Restore requires the exact producer Revision",
                ));
            }
            validate_restore_transition(&observation.instance.bindings, snapshot)?;
            ManagedRunIdentity::Restore {
                revision: observation.instance.active_revision.clone(),
                snapshot: id,
            }
        }
    };
    let access = operation
        .authoritative_access(core)
        .map_err(|_| SnapshotPlanError::Invalid("Snapshot capability is unavailable"))?;
    Ok(SnapshotExecutionPlan {
        instance: intent.instance,
        state: observation.instance.state_version,
        operation,
        access,
        parameters,
        active_bindings: if matches!(intent.operation, SnapshotOperation::Capture) {
            observation.active_bindings
        } else {
            Vec::new()
        },
        runtime: observation.revision.runtime_content.files().to_vec(),
        hook: hook.clone(),
        launch,
    })
}
