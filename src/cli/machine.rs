//! Invocation-owned presentation version selection. It never selects runtime policy.
use super::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum Version {
    Legacy,
    #[default]
    Operations,
}
impl Version {
    pub(super) fn require_run(self, run: &crate::domain::ManagedRunView) -> Result<(), CliError> {
        if let crate::domain::RunState::Finished(outcome) = &run.state {
            if self == Self::Legacy
                && outcome.primary_failure.as_ref().is_some_and(|failure| {
                    matches!(
                        failure.cause,
                        Some(crate::domain::RunFailureCause::ServicePresenceRequirement(
                            _
                        ))
                    )
                })
            {
                return Err(CliError::operation(
                    "machine interface 1.0-alpha.3 cannot represent service prerequisite failure facts; select --machine-version 1.0-alpha.4",
                ));
            }
            self.require_artifacts(&outcome.artifacts)?;
            if self == Self::Legacy
                && !matches!(run.operation, crate::domain::ManagedRunIdentity::Action(_))
                && !outcome.artifacts.is_empty()
            {
                return Err(CliError::operation(
                    "machine interface 1.0-alpha.3 cannot represent this operation's artifacts; select --machine-version 1.0-alpha.4",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn require_artifacts(
        self,
        artifacts: &[crate::domain::RunArtifactSummary],
    ) -> Result<(), CliError> {
        if self == Self::Legacy && artifacts.iter().any(|artifact| artifact.edge.is_some()) {
            Err(CliError::operation(
                "machine interface 1.0-alpha.3 cannot represent edge-scoped artifacts; select --machine-version 1.0-alpha.4",
            ))
        } else {
            Ok(())
        }
    }
    pub(super) fn require_hook(
        self,
        protocol: crate::domain::FormatVersion,
    ) -> Result<(), CliError> {
        if self == Self::Legacy && protocol == crate::domain::operation_hook_version() {
            Err(CliError::operation(
                "machine interface 1.0-alpha.3 cannot represent this operation; select --machine-version 1.0-alpha.4",
            ))
        } else {
            Ok(())
        }
    }
    pub(super) fn require_core(self, core: &crate::domain::RevisionCore) -> Result<(), CliError> {
        if self == Self::Legacy && core.operation().is_some() {
            Err(CliError::operation(
                "machine interface 1.0-alpha.3 cannot represent this operation-scoped Revision; select --machine-version 1.0-alpha.4",
            ))
        } else {
            Ok(())
        }
    }
    pub(super) const fn text(self) -> &'static str {
        match self {
            Self::Legacy => "1.0-alpha.3",
            Self::Operations => "1.0-alpha.4",
        }
    }
    fn parse(text: &str) -> Option<Self> {
        match text {
            "1.0-alpha.3" => Some(Self::Legacy),
            "1.0-alpha.4" => Some(Self::Operations),
            _ => None,
        }
    }
}

#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct ActiveInputs {
    pub(super) required: Vec<String>,
    pub(super) optional: Vec<String>,
}
impl From<&crate::domain::ActiveInputUse> for ActiveInputs {
    fn from(inputs: &crate::domain::ActiveInputUse) -> Self {
        Self {
            required: inputs
                .required
                .iter()
                .map(|id| id.as_str().into())
                .collect(),
            optional: inputs
                .optional
                .iter()
                .map(|id| id.as_str().into())
                .collect(),
        }
    }
}
#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct InputReference {
    role: InputRole,
    input_id: String,
}
impl From<&crate::domain::InputBindingRefV1> for InputReference {
    fn from(reference: &crate::domain::InputBindingRefV1) -> Self {
        Self {
            role: match reference.role {
                crate::domain::InputBindingRoleV1::Active => InputRole::Active,
                crate::domain::InputBindingRoleV1::Retained => InputRole::Retained,
            },
            input_id: reference.input_id.as_str().into(),
        }
    }
}
#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct BindingInputs {
    required: Vec<InputReference>,
    optional: Vec<InputReference>,
}
impl From<&crate::domain::BindingInputUse> for BindingInputs {
    fn from(inputs: &crate::domain::BindingInputUse) -> Self {
        Self {
            required: inputs.required.iter().map(Into::into).collect(),
            optional: inputs.optional.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Operation {
    Action {
        action_id: String,
    },
    SnapshotCapture,
    SnapshotRestore {
        snapshot_id: Option<String>,
    },
    Migration {
        source_revision: presentation::Revision,
        target_revision: presentation::Revision,
    },
    Cleanup,
}
impl Operation {
    fn sort_key(&self) -> (&str, &str) {
        match self {
            Self::Action { action_id } => ("action", action_id),
            Self::Cleanup => ("cleanup", ""),
            Self::SnapshotCapture => ("snapshot_capture", ""),
            Self::SnapshotRestore { .. } => ("snapshot_restore", ""),
            Self::Migration { .. } => ("migration", ""),
        }
    }
}
#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(super) enum AssessmentState {
    Satisfied,
    Missing,
    NotEvaluated,
}
#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct MissingInput {
    context: InputContext,
    role: InputRole,
    input_id: String,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(super) enum InputRole {
    Active,
    Retained,
}
// The complete contract vocabulary also covers contexts not yet selected by a query.
#[allow(dead_code)]
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(super) enum InputContext {
    Current,
    Snapshot,
    Source,
    Target,
}
#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Assessment {
    operation: Operation,
    state: AssessmentState,
    missing: Vec<MissingInput>,
}
impl Assessment {
    pub(super) fn migration(edge: &crate::domain::MigrationCompiledEdge, future: bool) -> Self {
        use crate::domain::MigrationValueOrigin;
        let declaration = edge.bindings.declaration();
        let has_requirements =
            !declaration.requires_source.is_empty() || !declaration.requires_target.is_empty();
        let unknown = (future && has_requirements)
            || declaration.requires_source.iter().any(|reference| {
                edge.bindings
                    .source_bindings()
                    .iter()
                    .find(|binding| binding.input == reference.input_id)
                    .is_none_or(|binding| {
                        !matches!(binding.origin, MigrationValueOrigin::Existing(_))
                    })
            })
            || declaration.requires_target.iter().any(|id| {
                edge.bindings
                    .staged_bindings()
                    .iter()
                    .find(|binding| &binding.input == id)
                    .is_none_or(|binding| {
                        !matches!(binding.origin, MigrationValueOrigin::Existing(_))
                    })
            });
        Self {
            operation: Operation::Migration {
                source_revision: edge.bindings.source().into(),
                target_revision: edge.bindings.target().into(),
            },
            state: if unknown {
                AssessmentState::NotEvaluated
            } else {
                AssessmentState::Satisfied
            },
            missing: vec![],
        }
    }
    pub(super) fn satisfied(operation: Operation) -> Self {
        Self {
            operation,
            state: AssessmentState::Satisfied,
            missing: vec![],
        }
    }
    fn current(
        operation: Operation,
        required: &[crate::domain::InputBindingRefV1],
        bindings: &[crate::domain::ManagedInputBindingView],
    ) -> Self {
        use crate::domain::{InputBindingRoleV1 as D, ManagedInputRole as R};
        let mut missing: Vec<_> = required
            .iter()
            .filter(|r| {
                !bindings.iter().any(|b| {
                    b.input_id == r.input_id
                        && b.present
                        && matches!(
                            (r.role, b.role),
                            (D::Active, R::Active { .. }) | (D::Retained, R::Retained)
                        )
                })
            })
            .map(|r| MissingInput {
                context: InputContext::Current,
                role: if r.role == D::Active {
                    InputRole::Active
                } else {
                    InputRole::Retained
                },
                input_id: r.input_id.as_str().into(),
            })
            .collect();
        missing.sort_by(|a, b| {
            (a.context, a.role, &a.input_id).cmp(&(b.context, b.role, &b.input_id))
        });
        Self {
            operation,
            state: if missing.is_empty() {
                AssessmentState::Satisfied
            } else {
                AssessmentState::Missing
            },
            missing,
        }
    }
    pub(super) fn action(plan: &ActionExecutionPlan) -> Self {
        let missing: Vec<_> = plan
            .missing_input_ids()
            .iter()
            .map(|id| MissingInput {
                context: InputContext::Current,
                role: InputRole::Active,
                input_id: id.as_str().into(),
            })
            .collect();
        Self {
            operation: Operation::Action {
                action_id: plan.action().as_str().into(),
            },
            state: if !missing.is_empty() {
                AssessmentState::Missing
            } else if plan.required_inputs_satisfied() {
                AssessmentState::Satisfied
            } else {
                AssessmentState::NotEvaluated
            },
            missing,
        }
    }
}
#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct LegacyRequiredInputs {
    satisfied: bool,
    missing_input_ids: Vec<String>,
}
pub(super) fn legacy_required(
    view: &crate::domain::InstanceView,
    core: &crate::domain::RevisionCore,
) -> Option<LegacyRequiredInputs> {
    core.operation().is_none().then(|| LegacyRequiredInputs {
        satisfied: view.required_inputs_satisfied,
        missing_input_ids: core
            .inputs()
            .iter()
            .filter(|input| {
                input.required
                    && !view
                        .bindings
                        .iter()
                        .any(|binding| binding.input_id == input.id && binding.present)
            })
            .map(|input| input.id.as_str().into())
            .collect(),
    })
}
pub(super) fn requirements(
    view: &crate::domain::InstanceView,
    core: &crate::domain::RevisionCore,
) -> Vec<Assessment> {
    use crate::domain::*;
    let required = |site: ServiceHookSite| -> Vec<InputBindingRefV1> {
        if let Some(operation) = core.operation() {
            match operation.input_use(&site) {
                Some(OperationInputUse::Active(inputs)) => inputs
                    .required
                    .into_iter()
                    .map(|input_id| InputBindingRefV1 {
                        role: InputBindingRoleV1::Active,
                        input_id,
                    })
                    .collect(),
                Some(OperationInputUse::Binding(inputs)) => inputs.required,
                _ => Vec::new(),
            }
        } else if site == ServiceHookSite::Cleanup {
            core.cleanup()
                .map(|cleanup| cleanup.requires.clone())
                .unwrap_or_default()
        } else {
            core.inputs()
                .iter()
                .filter(|input| input.required)
                .map(|input| InputBindingRefV1 {
                    role: InputBindingRoleV1::Active,
                    input_id: input.id.clone(),
                })
                .collect()
        }
    };
    let mut assessments = Vec::new();
    for action in core.actions() {
        assessments.push(Assessment::current(
            Operation::Action {
                action_id: action.id.as_str().into(),
            },
            &required(ServiceHookSite::Action(action.id.clone())),
            &view.bindings,
        ));
    }
    if let Some(snapshot) = core.snapshot() {
        if snapshot.capture.is_some() {
            assessments.push(Assessment::current(
                Operation::SnapshotCapture,
                &required(ServiceHookSite::Capture),
                &view.bindings,
            ));
        }
        if snapshot.restore.is_some() {
            assessments.push(Assessment {
                operation: Operation::SnapshotRestore { snapshot_id: None },
                state: AssessmentState::NotEvaluated,
                missing: vec![],
            });
        }
    }
    if core.cleanup().is_some() {
        assessments.push(Assessment::current(
            Operation::Cleanup,
            &required(ServiceHookSite::Cleanup),
            &view.bindings,
        ));
    }
    assessments.sort_by(|a, b| a.operation.sort_key().cmp(&b.operation.sort_key()));
    assessments
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Selection {
    pub(super) diagnostic_id: Option<crate::domain::TraceId>,
    pub(super) format: presentation::Format,
    pub(super) version: Version,
    pub(super) human: configuration::HumanPreferences,
}
impl Selection {
    pub(super) fn with_format(self, format: presentation::Format) -> Self {
        Self { format, ..self }
    }
}
impl From<presentation::Format> for Selection {
    fn from(format: presentation::Format) -> Self {
        Self {
            format,
            diagnostic_id: None,
            version: Version::default(),
            human: configuration::HumanPreferences::default(),
        }
    }
}

pub(super) fn recognize(args: &[OsString]) -> Selection {
    let mut selected = Selection::default();
    let (mut format_seen, mut version_seen) = (false, false);
    let mut index = 0;
    while let Some(key) = args.get(index).and_then(|key| key.to_str()) {
        if let Some(width) = configuration::global_width(key) {
            index += width;
            continue;
        }
        let Some(value) = args.get(index + 1).and_then(|value| value.to_str()) else {
            break;
        };
        match key {
            "--format" => {
                if !format_seen {
                    selected.format = match value {
                        "json" => presentation::Format::Json,
                        "jsonl" => presentation::Format::Jsonl,
                        _ => presentation::Format::Human,
                    };
                    format_seen = true;
                }
            }
            "--machine-version" => {
                if !version_seen {
                    selected.version = Version::parse(value).unwrap_or_default();
                    version_seen = true;
                }
            }
            _ => break,
        }
        index += 2;
    }
    selected
}

pub(super) fn select(mut args: Vec<OsString>) -> Result<(Selection, Vec<OsString>), CliError> {
    let mut selected = Selection::default();
    let (mut format_seen, mut version_seen) = (false, false);
    let mut index = 0;
    while let Some(key) = args.get(index).and_then(|key| key.to_str()) {
        match key {
            "--format" => {
                if format_seen {
                    return Err(CliError::usage("duplicate --format selector"));
                }
                selected.format = match args.get(index + 1).and_then(|value| value.to_str()) {
                    Some("human") => presentation::Format::Human,
                    Some("json") => presentation::Format::Json,
                    Some("jsonl") => presentation::Format::Jsonl,
                    _ => {
                        return Err(CliError::usage(
                            "--format requires human, json or jsonl before the command",
                        ));
                    }
                };
                format_seen = true;
            }
            "--machine-version" => {
                if version_seen {
                    return Err(CliError::usage("duplicate --machine-version selector"));
                }
                selected.version=args.get(index+1).and_then(|value|value.to_str()).and_then(Version::parse)
                    .ok_or_else(||CliError::usage("--machine-version requires 1.0-alpha.3 or 1.0-alpha.4 before the command"))?;
                version_seen = true;
            }
            _ => break,
        }
        index += 2;
    }
    if version_seen && (!format_seen || selected.format == presentation::Format::Human) {
        return Err(CliError::usage(
            "--machine-version requires explicit --format json or --format jsonl",
        ));
    }
    Ok((selected, args.split_off(index)))
}
