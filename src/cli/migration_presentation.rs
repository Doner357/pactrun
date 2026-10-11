use super::*;
use crate::domain::*;
use serde::Serialize;

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Paths {
    candidates: Vec<Candidate>,
    has_more: bool,
    next_after: Option<String>,
    evaluation: &'static str,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Candidate {
    path_id: String,
    unique_prefix_length: usize,
    revisions: Vec<presentation::Revision>,
    edge_count: usize,
}
impl Paths {
    pub(super) fn new(p: &MigrationPathPage, widths: &[usize]) -> Self {
        Self {
            candidates: p
                .candidates
                .iter()
                .zip(widths)
                .map(|(c, width)| Candidate {
                    path_id: c.id.to_string(),
                    unique_prefix_length: *width,
                    revisions: c.revisions.iter().map(Into::into).collect(),
                    edge_count: c.revisions.len() - 1,
                })
                .collect(),
            has_more: p.has_more,
            next_after: if p.has_more {
                p.candidates.last().map(|c| c.id.to_string())
            } else {
                None
            },
            evaluation: "relational_edges_only",
        }
    }
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Plan {
    path_id: String,
    expected_state_version: String,
    preview: &'static str,
    operator_input_acquisition: &'static str,
    startup_timeout_ms: Option<String>,
    execution_timeout_ms: Option<String>,
    termination_grace_ms: Option<String>,
    edges: Vec<Edge>,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Edge {
    source: presentation::Revision,
    target: presentation::Revision,
    service: Option<Service>,
    operator_inputs: Vec<String>,
    requires_source: Vec<InputReference>,
    requires_target: Vec<String>,
    transitions: Vec<Transition>,
    mandatory_hook_outputs: Vec<String>,
    hook_present: bool,
    predicted_required_inputs_satisfied: bool,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct InputReference {
    role: &'static str,
    input_id: String,
}
impl From<&InputBindingRefV1> for InputReference {
    fn from(r: &InputBindingRefV1) -> Self {
        Self {
            role: match r.role {
                InputBindingRoleV1::Active => "active",
                InputBindingRoleV1::Retained => "retained",
            },
            input_id: r.input_id.as_str().into(),
        }
    }
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Transition {
    kind: &'static str,
    source: InputReference,
    target_input_id: Option<String>,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Service {
    transform: bool,
    live_observation: &'static str,
    created_storages: Vec<String>,
    consumed_storages: Vec<String>,
    consumed_resources: Vec<String>,
    created_resources: Vec<CreatedResource>,
    requirements: Vec<Requirement>,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct CreatedResource {
    resource_id: String,
    presence: &'static str,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Requirement {
    view: &'static str,
    role: &'static str,
    scope: Scope,
    presence: &'static str,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Scope {
    Resource { resource_id: String },
    Storage { storage_id: String },
}
impl Plan {
    pub(super) fn new(
        id: &MigrationPathId,
        plan: &MigrationExecutionPlan,
        policy: &HookRuntimePolicy,
    ) -> Self {
        let millis = |d: Option<Duration>| d.map(|d| d.as_millis().to_string());
        Self {
            path_id: id.to_string(),
            expected_state_version: plan.expected_state_version().to_string(),
            preview: "not_admitted",
            operator_input_acquisition: "not_performed",
            startup_timeout_ms: millis(policy.startup_timeout),
            execution_timeout_ms: millis(policy.action_timeout),
            termination_grace_ms: millis(policy.termination_grace),
            edges: plan
                .edges()
                .iter()
                .map(|e| {
                    let b = &e.bindings;
                    Edge {
                        source: b.source().into(),
                        target: b.target().into(),
                        operator_inputs: plan
                            .operator_inputs()
                            .iter()
                            .filter(|i| i.revision == b.target().content_digest)
                            .map(|i| i.input.as_str().into())
                            .collect(),
                        requires_source: b
                            .declaration()
                            .requires_source
                            .iter()
                            .map(Into::into)
                            .collect(),
                        requires_target: b
                            .declaration()
                            .requires_target
                            .iter()
                            .map(|i| i.as_str().into())
                            .collect(),
                        transitions: b
                            .declaration()
                            .transitions
                            .iter()
                            .map(|t| Transition {
                                kind: match t {
                                    MigrationTransitionV1::Carry { .. } => "carry",
                                    MigrationTransitionV1::Declassify { .. } => "declassify",
                                    MigrationTransitionV1::Keep { .. } => "keep",
                                    MigrationTransitionV1::Discard { .. } => "discard",
                                },
                                source: t.source().into(),
                                target_input_id: t.target().map(|i| i.as_str().into()),
                            })
                            .collect(),
                        mandatory_hook_outputs: b
                            .declaration()
                            .produces_target
                            .iter()
                            .map(|i| i.as_str().into())
                            .collect(),
                        hook_present: e.launch.is_some(),
                        predicted_required_inputs_satisfied: b.required_inputs_satisfied(),
                        service: e.service.as_ref().map(|s| Service {
                            transform: s.transform,
                            live_observation: "not_performed",
                            created_storages: s
                                .created_storages
                                .iter()
                                .map(|i| i.as_str().into())
                                .collect(),
                            consumed_storages: s
                                .consumed_storages
                                .iter()
                                .map(|i| i.as_str().into())
                                .collect(),
                            consumed_resources: s
                                .consumed_resources
                                .iter()
                                .map(|i| i.as_str().into())
                                .collect(),
                            created_resources: s
                                .create_presence
                                .iter()
                                .map(|(p, r)| CreatedResource {
                                    resource_id: r.declaration.id.as_str().into(),
                                    presence: match p {
                                        ServiceCreatePresence::Any => "any",
                                        ServiceCreatePresence::Present => "present",
                                        ServiceCreatePresence::Absent => "absent",
                                    },
                                })
                                .collect(),
                            requirements: s
                                .requires
                                .iter()
                                .map(|(r, _)| Requirement {
                                    view: match r.reference.view {
                                        ServiceView::Current => "current",
                                        ServiceView::Source => "source",
                                        ServiceView::Target => "target",
                                    },
                                    role: match r.reference.role {
                                        ServiceRole::Active => "active",
                                        ServiceRole::Retained => "retained",
                                    },
                                    scope: match &r.reference.scope {
                                        ServiceScope::Resource(i) => Scope::Resource {
                                            resource_id: i.as_str().into(),
                                        },
                                        ServiceScope::Storage(i) => Scope::Storage {
                                            storage_id: i.as_str().into(),
                                        },
                                    },
                                    presence: match r.presence {
                                        ServicePresenceRequirement::Present => "present",
                                        ServicePresenceRequirement::Absent => "absent",
                                    },
                                })
                                .collect(),
                        }),
                    }
                })
                .collect(),
        }
    }
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Completed {
    pub(super) inspection: execution_presentation::Inspection,
    pub(super) current_instance: presentation::Instance,
}
