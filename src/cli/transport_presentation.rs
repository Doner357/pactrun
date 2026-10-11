//! Transport receipts are Pactrun facts, not transformations of transported bytes.
use super::*;
use serde::Serialize;

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct PackExport {
    pub(super) revision: presentation::Revision,
    pub(super) output: presentation::NativePath,
    pub(super) portable_metadata_included: bool,
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct PackInstall {
    revision: presentation::Revision,
    reference: String,
    local: catalog_presentation::LocalRevision,
    newly_installed: bool,
    migrations: Vec<Relation>,
    kept_metadata: Vec<KeptMetadata>,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Relation {
    source_digest: String,
    state: &'static str,
    explanation: Option<String>,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct KeptMetadata {
    target: catalog_presentation::Target,
    field: &'static str,
}
impl From<&crate::application::InstallPackResult> for PackInstall {
    fn from(value: &crate::application::InstallPackResult) -> Self {
        Self {
            revision: (&value.revision).into(),
            reference: value.reference.clone(),
            local: (&value.local).into(),
            newly_installed: value.newly_installed,
            migrations: value
                .migrations
                .iter()
                .map(|m| {
                    let (state, explanation) = match &m.state {
                        MigrationRelationState::NotEvaluated => ("not_evaluated", None),
                        MigrationRelationState::Valid => ("valid", None),
                        MigrationRelationState::Invalid(reason) => {
                            ("invalid", Some(reason.to_string()))
                        }
                    };
                    Relation {
                        source_digest: m.source_revision_digest.as_str().into(),
                        state,
                        explanation,
                    }
                })
                .collect(),
            kept_metadata: value
                .kept_metadata
                .iter()
                .map(|(target, field)| KeptMetadata {
                    target: target.into(),
                    field: match field {
                        crate::domain::PresentationField::DisplayName => "display_name",
                        crate::domain::PresentationField::Summary => "summary",
                        crate::domain::PresentationField::Description => "description",
                        crate::domain::PresentationField::Help => "help",
                    },
                })
                .collect(),
        }
    }
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Snapshot {
    pub(super) unique_prefix_length: usize,
    snapshot_id: String,
    integrity_format: String,
    producer_revision: presentation::Revision,
    origin_instance_id: String,
    captured_at: Timestamp,
    publication_verification: &'static str,
    publication_relational_verification: &'static str,
    current_content_verification: &'static str,
    restore_capacity: &'static str,
    target_eligibility: &'static str,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Timestamp {
    unix_seconds: String,
    nanoseconds: u32,
}
impl From<&crate::persistence::SnapshotInspection> for Snapshot {
    fn from(v: &crate::persistence::SnapshotInspection) -> Self {
        Self {
            snapshot_id: v.id.to_string(),
            unique_prefix_length: 32,
            integrity_format: v.version.as_str().into(),
            producer_revision: (&v.producer).into(),
            origin_instance_id: v.origin.to_string(),
            captured_at: Timestamp {
                unix_seconds: v.captured_at.unix_seconds().to_string(),
                nanoseconds: v.captured_at.nanoseconds(),
            },
            publication_verification: "intrinsic_and_content_verified",
            publication_relational_verification: "not_recorded",
            current_content_verification: "not_performed",
            restore_capacity: if v.restore_capability.is_ok() {
                "within_profile_not_execution_approval"
            } else {
                "exceeds_build_profile"
            },
            target_eligibility: "target_not_specified",
        }
    }
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Snapshots {
    pub(super) items: Vec<Snapshot>,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct SnapshotVerified {
    pub(super) snapshot_id: String,
    pub(super) integrity_format: String,
    pub(super) intrinsic_verification: &'static str,
    pub(super) content_verification: &'static str,
    pub(super) relational_verification: &'static str,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct SnapshotImported {
    pub(super) snapshot_id: String,
    pub(super) outcome: &'static str,
    pub(super) relational_verification: &'static str,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct SnapshotExported {
    pub(super) snapshot_id: String,
    pub(super) outcome: &'static str,
    pub(super) output: presentation::NativePath,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct CreatedRestore {
    pub(super) created_instance: presentation::Instance,
    pub(super) restore_run_id: Option<String>,
    pub(super) restore: Option<execution_presentation::Inspection>,
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct SnapshotPlan {
    operation: execution_presentation::Operation,
    eligibility: SnapshotPlanEligibility,
    instance_id: String,
    revision: presentation::Revision,
    expected_state_version: String,
    access: &'static str,
    terminal: &'static str,
    protocol_version: String,
    parameters: Vec<execution_presentation::PlanParameter>,
    steps: Vec<String>,
    startup_timeout_ms: Option<String>,
    execution_timeout_ms: Option<String>,
    termination_grace_ms: String,
    recovery_override: bool,
    preview: &'static str,
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SnapshotPlanEligibility {
    Capture { required_inputs_satisfied: bool },
    Restore { target_eligibility: &'static str },
}

impl SnapshotPlan {
    pub(super) fn new(
        plan: &crate::domain::SnapshotExecutionPlan,
        options: &ExecutionOptions,
    ) -> Self {
        Self {
            operation: plan.operation().into(),
            eligibility: match plan.operation() {
                crate::domain::ManagedRunIdentity::Capture { .. } => {
                    SnapshotPlanEligibility::Capture {
                        required_inputs_satisfied: true,
                    }
                }
                crate::domain::ManagedRunIdentity::Restore { .. } => {
                    SnapshotPlanEligibility::Restore {
                        target_eligibility: "compiler_checks_passed_not_admitted",
                    }
                }
                _ => unreachable!("Snapshot compiler emits Capture or Restore only"),
            },
            instance_id: plan.instance().to_string(),
            revision: plan.operation().revision().into(),
            expected_state_version: plan.expected_state_version().to_string(),
            access: access_name(plan.access()),
            terminal: terminal_name(plan.hook().io.terminal),
            protocol_version: plan.hook().protocol_version.to_string(),
            parameters: plan
                .parameters()
                .iter()
                .map(|p| execution_presentation::PlanParameter {
                    parameter_id: p.id.as_str().into(),
                    effective_redaction: p.effective_redaction,
                })
                .collect(),
            steps: plan
                .steps()
                .iter()
                .map(|s| format_failed_step(crate::domain::RunFailedStep::SnapshotPlan(*s)))
                .collect(),
            startup_timeout_ms: options.startup_timeout_ms.map(|n| n.to_string()),
            execution_timeout_ms: options.action_timeout_ms.map(|n| n.to_string()),
            termination_grace_ms: options.termination_grace_ms.unwrap_or(5_000).to_string(),
            recovery_override: options.recovery_override,
            preview: "not_admitted",
        }
    }
}
