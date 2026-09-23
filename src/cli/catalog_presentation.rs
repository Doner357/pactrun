//! Catalog-specific public projections. Metadata claims are not authentication.
use super::*;
use crate::domain::*;
use serde::Serialize;

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Page<T: Serialize> {
    pub(super) items: Vec<T>,
    pub(super) next: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct History {
    instance_id: String,
    recorded_name: String,
    current_name: Option<String>,
    retired: bool,
    deletion_phase: Option<&'static str>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Retirement {
    #[serde(flatten)]
    pub(super) history: History,
    pub(super) inspection: retirements::RetirementInspection,
}

impl From<&InstanceHistoryEntry> for History {
    fn from(row: &InstanceHistoryEntry) -> Self {
        Self {
            instance_id: row.id.to_string(),
            recorded_name: row.recorded_name.as_str().into(),
            current_name: row.current_name.as_ref().map(|v| v.as_str().into()),
            retired: row.retired,
            deletion_phase: row.deletion_phase.map(|p| match p {
                DeletionPhase::LaunchAuthorized => "launch_authorized",
                DeletionPhase::ResultUnresolved => "result_unresolved",
                DeletionPhase::FinalizationAuthorized => "finalization_authorized",
            }),
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Metadata {
    ReferenceLabel {
        label: String,
        publisher: Option<String>,
        namespace: Option<String>,
        source_uri: Option<String>,
    },
    Presentation {
        target: Target,
        field: &'static str,
        value: String,
    },
    SourceUri {
        source_uri: String,
    },
    PublisherAttribution {
        publisher: String,
        namespace: Option<String>,
        source_uri: Option<String>,
    },
    Attribution {
        text: String,
        source_uri: Option<String>,
    },
    LocalAlias {
        alias: String,
    },
    LocalNote {
        note: String,
    },
    LocalTrust {
        assessment: &'static str,
    },
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Target {
    Revision,
    Input {
        input_id: String,
    },
    Action {
        action_id: String,
    },
    ActionParameter {
        action_id: String,
        parameter_id: String,
    },
    ManagedOutput {
        action_id: String,
        output_id: String,
    },
    SnapshotCapture,
    SnapshotRestore,
    SnapshotCaptureParameter {
        parameter_id: String,
    },
    SnapshotRestoreParameter {
        parameter_id: String,
    },
    MigrationEdge {
        source_digest: String,
    },
    Cleanup,
}

impl From<&PresentationTargetV1> for Target {
    fn from(target: &PresentationTargetV1) -> Self {
        match target {
            PresentationTargetV1::Revision => Self::Revision,
            PresentationTargetV1::Input(id) => Self::Input {
                input_id: id.as_str().into(),
            },
            PresentationTargetV1::Action(id) => Self::Action {
                action_id: id.as_str().into(),
            },
            PresentationTargetV1::ActionParameter { action, parameter } => Self::ActionParameter {
                action_id: action.as_str().into(),
                parameter_id: parameter.as_str().into(),
            },
            PresentationTargetV1::ManagedOutput { action, output } => Self::ManagedOutput {
                action_id: action.as_str().into(),
                output_id: output.as_str().into(),
            },
            PresentationTargetV1::SnapshotCapture => Self::SnapshotCapture,
            PresentationTargetV1::SnapshotRestore => Self::SnapshotRestore,
            PresentationTargetV1::SnapshotCaptureParameter(id) => Self::SnapshotCaptureParameter {
                parameter_id: id.as_str().into(),
            },
            PresentationTargetV1::SnapshotRestoreParameter(id) => Self::SnapshotRestoreParameter {
                parameter_id: id.as_str().into(),
            },
            PresentationTargetV1::MigrationEdge(id) => Self::MigrationEdge {
                source_digest: id.to_string(),
            },
            PresentationTargetV1::Cleanup => Self::Cleanup,
        }
    }
}

pub(super) fn trust(value: TrustAssessment) -> &'static str {
    match value {
        TrustAssessment::Trusted => "trusted",
        TrustAssessment::Distrusted => "distrusted",
    }
}

impl From<&RevisionMetadataItem> for Metadata {
    fn from(item: &RevisionMetadataItem) -> Self {
        match item {
            RevisionMetadataItem::ReferenceLabel(b) => Self::ReferenceLabel {
                label: b.label.as_str().into(),
                publisher: b.source.publisher_name().map(|s| s.as_str().into()),
                namespace: b.source.publisher_namespace().map(|s| s.as_str().into()),
                source_uri: b.source.source_uri().map(|s| s.as_str().into()),
            },
            RevisionMetadataItem::Presentation(p) => Self::Presentation {
                target: (&p.target).into(),
                field: match p.field {
                    PresentationField::DisplayName => "display_name",
                    PresentationField::Summary => "summary",
                    PresentationField::Description => "description",
                    PresentationField::Help => "help",
                },
                value: p.value.as_str().into(),
            },
            RevisionMetadataItem::Provenance { claim, .. } => match claim {
                ProvenanceClaim::SourceUri(uri) => Self::SourceUri {
                    source_uri: uri.as_str().into(),
                },
                ProvenanceClaim::PublisherAttribution {
                    publisher,
                    namespace,
                    source_uri,
                } => Self::PublisherAttribution {
                    publisher: publisher.as_str().into(),
                    namespace: namespace.as_ref().map(|s| s.as_str().into()),
                    source_uri: source_uri.as_ref().map(|s| s.as_str().into()),
                },
                ProvenanceClaim::Attribution { text, source_uri } => Self::Attribution {
                    text: text.as_str().into(),
                    source_uri: source_uri.as_ref().map(|s| s.as_str().into()),
                },
            },
            RevisionMetadataItem::LocalAlias { alias, .. } => Self::LocalAlias {
                alias: alias.as_str().into(),
            },
            RevisionMetadataItem::LocalNote { note, .. } => Self::LocalNote {
                note: note.as_str().into(),
            },
            RevisionMetadataItem::LocalTrust { trust: value, .. } => Self::LocalTrust {
                assessment: trust(*value),
            },
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct RevisionEntry {
    revision: presentation::Revision,
    core_version: u8,
    metadata: Vec<Metadata>,
    declarations: Option<Declarations>,
    metadata_scope: &'static str,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Declarations {
    actions: Vec<execution_presentation::Action>,
    inputs: Vec<InputDeclaration>,
    service_storage_count: usize,
    service_resource_count: usize,
    snapshot_declared: bool,
    migration_edge_count: usize,
    cleanup_declared: bool,
    capabilities: definitions::Capabilities,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct InputDeclaration {
    input_id: String,
    required: bool,
    protection: &'static str,
}

impl RevisionEntry {
    pub(super) fn new(row: &RevisionCatalogEntry, declarations: bool) -> Self {
        Self {
            revision: (&row.identity).into(),
            metadata_scope: "current",
            core_version: row.core.version(),
            metadata: row.metadata.items.iter().map(Into::into).collect(),
            declarations: declarations.then(|| Declarations {
                actions: row
                    .core
                    .actions()
                    .iter()
                    .map(|a| execution_presentation::Action::defined(a, row))
                    .collect(),
                inputs: row
                    .core
                    .inputs()
                    .iter()
                    .map(|i| InputDeclaration {
                        input_id: i.id.as_str().into(),
                        required: i.required,
                        protection: match i.protection {
                            InputProtectionV1::Normal => "normal",
                            InputProtectionV1::Secret => "secret",
                        },
                    })
                    .collect(),
                service_storage_count: row.core.service_core().map_or(0, |s| s.storages().len()),
                service_resource_count: row.core.service_core().map_or(0, |s| s.resources().len()),
                snapshot_declared: row.core.snapshot().is_some(),
                migration_edge_count: row.core.migrations().len(),
                cleanup_declared: row.core.cleanup().is_some(),
                capabilities: definitions::Capabilities::new(row),
            }),
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Alias {
    pub(super) alias: String,
    pub(super) target: Option<presentation::Revision>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Local {
    pub(super) revision: presentation::Revision,
    pub(super) note: Option<String>,
    pub(super) trust: Option<&'static str>,
}

impl Local {
    pub(super) fn new(row: &RevisionCatalogEntry) -> Self {
        Self {
            revision: (&row.identity).into(),
            note: row.metadata.items.iter().find_map(|m| {
                if let RevisionMetadataItem::LocalNote { note, .. } = m {
                    Some(note.as_str().into())
                } else {
                    None
                }
            }),
            trust: row.metadata.items.iter().find_map(|m| {
                if let RevisionMetadataItem::LocalTrust { trust: value, .. } = m {
                    Some(trust(*value))
                } else {
                    None
                }
            }),
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "field", rename_all = "snake_case")]
pub(super) enum MutationValue {
    Alias {
        alias: String,
        target: Option<presentation::Revision>,
    },
    Note {
        value: Option<String>,
    },
    Trust {
        value: Option<&'static str>,
    },
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Mutation {
    pub(super) revision: presentation::Revision,
    pub(super) outcome: &'static str,
    pub(super) desired: MutationValue,
}

impl Mutation {
    pub(super) fn new(revision: &RevisionIdentity, operation: &RevisionMetadataMutation) -> Self {
        let desired = match operation {
            RevisionMetadataMutation::CompareAndSetLocalAlias { alias, desired, .. } => {
                MutationValue::Alias {
                    alias: alias.as_str().into(),
                    target: match desired {
                        CurrentState::Absent => None,
                        CurrentState::Present(v) => Some(v.into()),
                    },
                }
            }
            RevisionMetadataMutation::CompareAndSetLocalNote { desired, .. } => {
                MutationValue::Note {
                    value: match desired {
                        CurrentState::Absent => None,
                        CurrentState::Present(v) => Some(v.as_str().into()),
                    },
                }
            }
            RevisionMetadataMutation::CompareAndSetLocalTrust { desired, .. } => {
                MutationValue::Trust {
                    value: match desired {
                        CurrentState::Absent => None,
                        CurrentState::Present(v) => Some(trust(*v)),
                    },
                }
            }
            _ => unreachable!("local metadata CLI only"),
        };
        Self {
            revision: revision.into(),
            outcome: "applied",
            desired,
        }
    }
}
