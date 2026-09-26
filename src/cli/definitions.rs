//! Explicit public declaration projections; executable arguments remain private.
use super::*;
use crate::domain::*;
use serde::Serialize;

#[derive(Clone, Default, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Text {
    display_name: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    help: Option<String>,
}
impl Text {
    pub(super) fn new(view: &RevisionMetadataView, target: PresentationTargetV1) -> Self {
        let mut text = Self::default();
        for item in &view.items {
            if let RevisionMetadataItem::Presentation(p) = item
                && p.target == target
            {
                let value = Some(p.value.as_str().to_owned());
                match p.field {
                    PresentationField::DisplayName => text.display_name = value,
                    PresentationField::Summary => text.summary = value,
                    PresentationField::Description => text.description = value,
                    PresentationField::Help => text.help = value,
                }
            }
        }
        text
    }
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub(super) enum DefaultValue {
    Integer(String),
    Float(String),
    Boolean(bool),
    String(String),
}
impl From<&ParameterDefaultV1> for DefaultValue {
    fn from(p: &ParameterDefaultV1) -> Self {
        match p {
            ParameterDefaultV1::Integer(v) => Self::Integer(v.get().to_string()),
            ParameterDefaultV1::Float(v) => Self::Float(v.get().to_string()),
            ParameterDefaultV1::Boolean(v) => Self::Boolean(*v),
            ParameterDefaultV1::String(v) => Self::String(v.clone()),
        }
    }
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Parameter {
    pub(super) parameter_id: String,
    pub(super) parameter_type: &'static str,
    pub(super) sensitive: bool,
    pub(super) default_present: bool,
    pub(super) default_redacted: bool,
    pub(super) default_value: Option<DefaultValue>,
    pub(super) metadata: Text,
}
impl Parameter {
    pub(super) fn new(p: &ParameterV1, metadata: Text) -> Self {
        Self {
            parameter_id: p.id.as_str().into(),
            parameter_type: parameter_type_name(p.parameter_type),
            sensitive: p.sensitive,
            default_present: p.default.is_some(),
            default_redacted: p.sensitive && p.default.is_some(),
            default_value: if p.sensitive {
                None
            } else {
                p.default.as_ref().map(Into::into)
            },
            metadata,
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Output {
    pub(super) output_id: String,
    pub(super) metadata: Text,
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Input {
    input_id: String,
    required: bool,
    protection: &'static str,
    metadata: Text,
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Hook {
    protocol_version: String,
    terminal: &'static str,
    launch: execution_presentation::Launch,
    hook_args_count: usize,
    service_access: Vec<Grant>,
    service_requires: Vec<Requirement>,
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Reference {
    view: &'static str,
    role: &'static str,
    kind: &'static str,
    id: String,
}
impl From<&ServiceReferenceV2> for Reference {
    fn from(r: &ServiceReferenceV2) -> Self {
        let (kind, id) = match &r.scope {
            ServiceScope::Storage(id) => ("storage", id.as_str()),
            ServiceScope::Resource(id) => ("resource", id.as_str()),
        };
        Self {
            view: match r.view {
                ServiceView::Current => "current",
                ServiceView::Source => "source",
                ServiceView::Target => "target",
            },
            role: role(r.role),
            kind,
            id: id.into(),
        }
    }
}
fn role(r: ServiceRole) -> &'static str {
    match r {
        ServiceRole::Active => "active",
        ServiceRole::Retained => "retained",
    }
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Grant {
    reference: Reference,
    mode: &'static str,
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Requirement {
    reference: Reference,
    presence: &'static str,
}
impl Hook {
    pub(super) fn new(h: &HookV1, core: &RevisionCore, site: ServiceHookSite) -> Self {
        let contract = core.service_hook(&site);
        Self {
            protocol_version: h.protocol_version.to_string(),
            terminal: terminal_name(h.io.terminal),
            launch: execution_presentation::Launch::from(&h.launch),
            hook_args_count: h.args.len(),
            service_access: contract
                .access
                .iter()
                .map(|a| Grant {
                    reference: (&a.reference).into(),
                    mode: match a.mode {
                        ServiceAccessMode::Read => "read",
                        ServiceAccessMode::Write => "write",
                    },
                })
                .collect(),
            service_requires: contract
                .requires
                .iter()
                .map(|r| Requirement {
                    reference: (&r.reference).into(),
                    presence: match r.presence {
                        ServicePresenceRequirement::Present => "present",
                        ServicePresenceRequirement::Absent => "absent",
                    },
                })
                .collect(),
        }
    }
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Capability {
    access: &'static str,
    parameters: Vec<Parameter>,
    hook: Hook,
    metadata: Text,
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct InputRef {
    role: &'static str,
    input_id: String,
}
impl From<&InputBindingRefV1> for InputRef {
    fn from(i: &InputBindingRefV1) -> Self {
        Self {
            role: match i.role {
                InputBindingRoleV1::Active => "active",
                InputBindingRoleV1::Retained => "retained",
            },
            input_id: i.input_id.as_str().into(),
        }
    }
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Transition {
    kind: &'static str,
    source: InputRef,
    target_input_id: Option<String>,
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct ServiceSource {
    role: &'static str,
    id: String,
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct ServiceTransition {
    kind: &'static str,
    sources: Vec<ServiceSource>,
    targets: Vec<String>,
    presence: Option<&'static str>,
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Migration {
    source_revision_digest: String,
    transitions: Vec<Transition>,
    requires_source: Vec<InputRef>,
    requires_target: Vec<String>,
    produces_target: Vec<String>,
    hook: Option<Hook>,
    storage_transitions: Vec<ServiceTransition>,
    resource_transitions: Vec<ServiceTransition>,
    metadata: Text,
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Cleanup {
    requires: Vec<InputRef>,
    hook: Hook,
    metadata: Text,
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Storage {
    storage_id: String,
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Resource {
    resource_id: String,
    storage_id: String,
    locator: String,
    kind: &'static str,
    read_exposure: &'static str,
    user_mutation: service_storage::Mutation,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Capabilities {
    metadata: Text,
    inputs: Vec<Input>,
    capture: Option<Capability>,
    restore: Option<Capability>,
    migrations: Vec<Migration>,
    cleanup: Option<Cleanup>,
    service_storages: Vec<Storage>,
    service_resources: Vec<Resource>,
}
impl Capabilities {
    pub(super) fn new(row: &RevisionCatalogEntry) -> Self {
        let core = &row.core;
        let text = |target| Text::new(&row.metadata, target);
        let capture = core
            .snapshot()
            .and_then(|s| s.capture.as_ref())
            .map(|c| Capability {
                access: access_name(c.access),
                parameters: c
                    .parameters
                    .iter()
                    .map(|p| {
                        Parameter::new(
                            p,
                            text(PresentationTargetV1::SnapshotCaptureParameter(p.id.clone())),
                        )
                    })
                    .collect(),
                hook: Hook::new(&c.hook, core, ServiceHookSite::Capture),
                metadata: text(PresentationTargetV1::SnapshotCapture),
            });
        let restore = core
            .snapshot()
            .and_then(|s| s.restore.as_ref())
            .map(|c| Capability {
                access: "mutate",
                parameters: c
                    .parameters
                    .iter()
                    .map(|p| {
                        Parameter::new(
                            p,
                            text(PresentationTargetV1::SnapshotRestoreParameter(p.id.clone())),
                        )
                    })
                    .collect(),
                hook: Hook::new(&c.hook, core, ServiceHookSite::Restore),
                metadata: text(PresentationTargetV1::SnapshotRestore),
            });
        let migrations = core
            .migrations()
            .iter()
            .map(|m| {
                let service = core
                    .service_core()
                    .and_then(|c| c.migrations().get(&m.source_revision_digest));
                Migration {
                    source_revision_digest: m.source_revision_digest.as_str().into(),
                    transitions: m
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
                            target_input_id: t.target().map(|id| id.as_str().into()),
                        })
                        .collect(),
                    requires_source: m.requires_source.iter().map(Into::into).collect(),
                    requires_target: m
                        .requires_target
                        .iter()
                        .map(|id| id.as_str().into())
                        .collect(),
                    produces_target: m
                        .produces_target
                        .iter()
                        .map(|id| id.as_str().into())
                        .collect(),
                    hook: m.hook.as_ref().map(|h| {
                        Hook::new(
                            h,
                            core,
                            ServiceHookSite::Migration(m.source_revision_digest.clone()),
                        )
                    }),
                    storage_transitions: service
                        .map(|s| s.storages.iter().map(storage_transition).collect())
                        .unwrap_or_default(),
                    resource_transitions: service
                        .map(|s| s.resources.iter().map(resource_transition).collect())
                        .unwrap_or_default(),
                    metadata: text(PresentationTargetV1::MigrationEdge(
                        RevisionContentDigest::from_bytes(m.source_revision_digest.to_bytes()),
                    )),
                }
            })
            .collect();
        Self {
            metadata: text(PresentationTargetV1::Revision),
            inputs: core
                .inputs()
                .iter()
                .map(|i| Input {
                    input_id: i.id.as_str().into(),
                    required: i.required,
                    protection: match i.protection {
                        InputProtectionV1::Normal => "normal",
                        InputProtectionV1::Secret => "secret",
                    },
                    metadata: text(PresentationTargetV1::Input(i.id.clone())),
                })
                .collect(),
            capture,
            restore,
            migrations,
            cleanup: core.cleanup().map(|c| Cleanup {
                requires: c.requires.iter().map(Into::into).collect(),
                hook: Hook::new(&c.hook, core, ServiceHookSite::Cleanup),
                metadata: text(PresentationTargetV1::Cleanup),
            }),
            service_storages: core
                .service_core()
                .map(|c| {
                    c.storages()
                        .iter()
                        .map(|s| Storage {
                            storage_id: s.id.as_str().into(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            service_resources: core
                .service_core()
                .map(|c| {
                    c.resources()
                        .iter()
                        .map(|r| Resource {
                            resource_id: r.id.as_str().into(),
                            storage_id: r.storage_id.as_str().into(),
                            locator: r.locator.as_str().into(),
                            kind: match r.kind {
                                ServiceResourceKind::File => "file",
                                ServiceResourceKind::Directory => "directory",
                            },
                            read_exposure: match r.read_exposure {
                                ServiceReadExposure::Hidden => "hidden",
                                ServiceReadExposure::Readable => "readable",
                            },
                            user_mutation: (&r.user_mutation).into(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}
fn storage_transition(t: &StorageTransitionV2) -> ServiceTransition {
    let (kind, source) = match t {
        StorageTransitionV2::Create { .. } => ("create", None),
        StorageTransitionV2::Reuse { source, .. } => ("reuse", Some(source)),
        StorageTransitionV2::Reattach { source, .. } => ("reattach", Some(source)),
    };
    ServiceTransition {
        kind,
        sources: source
            .into_iter()
            .map(|s| ServiceSource {
                role: role(s.role),
                id: s.storage_id.as_str().into(),
            })
            .collect(),
        targets: vec![t.target().as_str().into()],
        presence: None,
    }
}
fn resource_transition(t: &ResourceTransitionV2) -> ServiceTransition {
    let (kind, presence) = match t {
        ResourceTransitionV2::Create { presence, .. } => (
            "create",
            Some(match presence {
                ServiceCreatePresence::Any => "any",
                ServiceCreatePresence::Present => "present",
                ServiceCreatePresence::Absent => "absent",
            }),
        ),
        ResourceTransitionV2::Reuse { .. } => ("reuse", None),
        ResourceTransitionV2::Reattach { .. } => ("reattach", None),
        ResourceTransitionV2::Transform { .. } => ("transform", None),
    };
    ServiceTransition {
        kind,
        presence,
        sources: t
            .sources()
            .iter()
            .map(|s| ServiceSource {
                role: role(s.role),
                id: s.resource_id.as_str().into(),
            })
            .collect(),
        targets: t.targets().iter().map(|id| id.as_str().into()).collect(),
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Related<T> {
    #[serde(flatten)]
    pub(super) value: T,
    pub(super) related_revisions: Vec<catalog_presentation::RevisionEntry>,
    pub(super) unavailable_revisions: Vec<presentation::Revision>,
}
impl<T> Related<T> {
    pub(super) fn new(
        value: T,
        rows: &[RevisionCatalogEntry],
        missing: &[RevisionIdentity],
    ) -> Self {
        Self {
            value,
            related_revisions: rows
                .iter()
                .map(|r| catalog_presentation::RevisionEntry::new(r, true))
                .collect(),
            unavailable_revisions: missing.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Instances {
    pub(super) items: Vec<presentation::Instance>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct InspectedRun {
    #[serde(flatten)]
    pub(super) value: execution_presentation::Run,
    pub(super) inspection: execution_presentation::Inspection,
}
