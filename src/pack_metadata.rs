//! Closed transport DTOs, deliberately separate from the private Domain model.
use crate::domain::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

type Result<T> = std::result::Result<T, String>;
fn parsed<T, E: std::fmt::Display>(v: std::result::Result<T, E>) -> Result<T> {
    v.map_err(|e| e.to_string())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Metadata {
    #[serde(default)]
    reference_labels: Vec<Label>,
    #[serde(default)]
    presentation: Vec<Presentation>,
    #[serde(default)]
    provenance: Vec<Claim>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Label {
    label: String,
    source: LabelSource,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum LabelSource {
    Unattributed,
    SourceUri {
        source_uri: String,
    },
    Publisher {
        publisher_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        publisher_namespace: Option<String>,
    },
    PublisherSourceUri {
        publisher_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        publisher_namespace: Option<String>,
        source_uri: String,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Presentation {
    target: Target,
    field: Field,
    value: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Field {
    DisplayName,
    Summary,
    Description,
    Help,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Target {
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
    SnapshotCaptureParameter {
        parameter_id: String,
    },
    SnapshotRestore,
    SnapshotRestoreParameter {
        parameter_id: String,
    },
    Migration {
        source_revision_digest: String,
    },
    Cleanup,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Claim {
    SourceUri {
        source_uri: String,
    },
    PublisherAttribution {
        publisher_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        publisher_namespace: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_uri: Option<String>,
    },
    Attribution {
        attribution_text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_uri: Option<String>,
    },
}

impl Metadata {
    pub(crate) fn decode(self) -> Result<PortableMetadataTemplate> {
        let mut result = PortableMetadataTemplate::default();
        let mut labels = BTreeSet::new();
        for l in self.reference_labels {
            let source = match l.source {
                LabelSource::Unattributed => ReferenceLabelSource::Unattributed,
                LabelSource::SourceUri { source_uri } => {
                    ReferenceLabelSource::SourceUri(parsed(SourceUri::parse(source_uri))?)
                }
                LabelSource::Publisher {
                    publisher_name,
                    publisher_namespace,
                } => ReferenceLabelSource::Publisher {
                    name: parsed(PublisherName::parse(publisher_name))?,
                    namespace: parsed(
                        publisher_namespace
                            .map(PublisherNamespace::parse)
                            .transpose(),
                    )?,
                },
                LabelSource::PublisherSourceUri {
                    publisher_name,
                    publisher_namespace,
                    source_uri,
                } => ReferenceLabelSource::PublisherSourceUri {
                    name: parsed(PublisherName::parse(publisher_name))?,
                    namespace: parsed(
                        publisher_namespace
                            .map(PublisherNamespace::parse)
                            .transpose(),
                    )?,
                    source_uri: parsed(SourceUri::parse(source_uri))?,
                },
            };
            if !labels.insert((parsed(ReferenceLabel::parse(l.label))?, source)) {
                return Err("duplicate reference-label tuple".into());
            }
        }
        result.reference_labels = labels.into_iter().collect();
        let mut keys = BTreeSet::new();
        for p in self.presentation {
            let target = match p.target {
                Target::Revision => PresentationTargetV1::Revision,
                Target::Input { input_id } => {
                    PresentationTargetV1::Input(parsed(InputIdentity::parse(input_id))?)
                }
                Target::Action { action_id } => {
                    PresentationTargetV1::Action(parsed(ActionIdentity::parse(action_id))?)
                }
                Target::ActionParameter {
                    action_id,
                    parameter_id,
                } => PresentationTargetV1::ActionParameter {
                    action: parsed(ActionIdentity::parse(action_id))?,
                    parameter: parsed(ParameterIdentity::parse(parameter_id))?,
                },
                Target::ManagedOutput {
                    action_id,
                    output_id,
                } => PresentationTargetV1::ManagedOutput {
                    action: parsed(ActionIdentity::parse(action_id))?,
                    output: parsed(ManagedOutputIdentity::parse(output_id))?,
                },
                Target::SnapshotCapture => PresentationTargetV1::SnapshotCapture,
                Target::SnapshotCaptureParameter { parameter_id } => {
                    PresentationTargetV1::SnapshotCaptureParameter(parsed(
                        ParameterIdentity::parse(parameter_id),
                    )?)
                }
                Target::SnapshotRestore => PresentationTargetV1::SnapshotRestore,
                Target::SnapshotRestoreParameter { parameter_id } => {
                    PresentationTargetV1::SnapshotRestoreParameter(parsed(
                        ParameterIdentity::parse(parameter_id),
                    )?)
                }
                Target::Migration {
                    source_revision_digest,
                } => PresentationTargetV1::MigrationEdge(source_revision_digest.parse()?),
                Target::Cleanup => PresentationTargetV1::Cleanup,
            };
            let field = match p.field {
                Field::DisplayName => PresentationField::DisplayName,
                Field::Summary => PresentationField::Summary,
                Field::Description => PresentationField::Description,
                Field::Help => PresentationField::Help,
            };
            if !keys.insert((target.clone(), field)) {
                return Err("duplicate presentation key".into());
            }
            result.presentation.push(PortablePresentationTemplate {
                target,
                field,
                value: parsed(PresentationValue::parse(p.value))?,
            });
        }
        result
            .presentation
            .sort_by(|a, b| (&a.target, a.field).cmp(&(&b.target, b.field)));
        let mut claims = BTreeSet::new();
        for p in self.provenance {
            let p = match p {
                Claim::SourceUri { source_uri } => {
                    ProvenanceClaim::SourceUri(parsed(SourceUri::parse(source_uri))?)
                }
                Claim::PublisherAttribution {
                    publisher_name,
                    publisher_namespace,
                    source_uri,
                } => ProvenanceClaim::PublisherAttribution {
                    publisher: parsed(PublisherName::parse(publisher_name))?,
                    namespace: parsed(
                        publisher_namespace
                            .map(PublisherNamespace::parse)
                            .transpose(),
                    )?,
                    source_uri: parsed(source_uri.map(SourceUri::parse).transpose())?,
                },
                Claim::Attribution {
                    attribution_text,
                    source_uri,
                } => ProvenanceClaim::Attribution {
                    text: parsed(AttributionText::parse(attribution_text))?,
                    source_uri: parsed(source_uri.map(SourceUri::parse).transpose())?,
                },
            };
            if !claims.insert(p) {
                return Err("duplicate provenance tuple".into());
            }
        }
        result.provenance = claims.into_iter().collect();
        Ok(result)
    }
    pub(crate) fn encode(value: &PortableMetadataTemplate) -> Self {
        Self {
            reference_labels: value
                .reference_labels
                .iter()
                .map(|(l, s)| Label {
                    label: l.as_str().into(),
                    source: match s {
                        ReferenceLabelSource::Unattributed => LabelSource::Unattributed,
                        ReferenceLabelSource::SourceUri(uri) => LabelSource::SourceUri {
                            source_uri: uri.as_str().into(),
                        },
                        ReferenceLabelSource::Publisher { name, namespace } => {
                            LabelSource::Publisher {
                                publisher_name: name.as_str().into(),
                                publisher_namespace: namespace.as_ref().map(|x| x.as_str().into()),
                            }
                        }
                        ReferenceLabelSource::PublisherSourceUri {
                            name,
                            namespace,
                            source_uri,
                        } => LabelSource::PublisherSourceUri {
                            publisher_name: name.as_str().into(),
                            publisher_namespace: namespace.as_ref().map(|x| x.as_str().into()),
                            source_uri: source_uri.as_str().into(),
                        },
                    },
                })
                .collect(),
            presentation: value
                .presentation
                .iter()
                .map(|p| Presentation {
                    target: match &p.target {
                        PresentationTargetV1::Revision => Target::Revision,
                        PresentationTargetV1::Input(id) => Target::Input {
                            input_id: id.as_str().into(),
                        },
                        PresentationTargetV1::Action(id) => Target::Action {
                            action_id: id.as_str().into(),
                        },
                        PresentationTargetV1::ActionParameter { action, parameter } => {
                            Target::ActionParameter {
                                action_id: action.as_str().into(),
                                parameter_id: parameter.as_str().into(),
                            }
                        }
                        PresentationTargetV1::ManagedOutput { action, output } => {
                            Target::ManagedOutput {
                                action_id: action.as_str().into(),
                                output_id: output.as_str().into(),
                            }
                        }
                        PresentationTargetV1::SnapshotCapture => Target::SnapshotCapture,
                        PresentationTargetV1::SnapshotCaptureParameter(id) => {
                            Target::SnapshotCaptureParameter {
                                parameter_id: id.as_str().into(),
                            }
                        }
                        PresentationTargetV1::SnapshotRestore => Target::SnapshotRestore,
                        PresentationTargetV1::SnapshotRestoreParameter(id) => {
                            Target::SnapshotRestoreParameter {
                                parameter_id: id.as_str().into(),
                            }
                        }
                        PresentationTargetV1::MigrationEdge(id) => Target::Migration {
                            source_revision_digest: id.to_string(),
                        },
                        PresentationTargetV1::Cleanup => Target::Cleanup,
                    },
                    field: match p.field {
                        PresentationField::DisplayName => Field::DisplayName,
                        PresentationField::Summary => Field::Summary,
                        PresentationField::Description => Field::Description,
                        PresentationField::Help => Field::Help,
                    },
                    value: p.value.as_str().into(),
                })
                .collect(),
            provenance: value
                .provenance
                .iter()
                .map(|p| match p {
                    ProvenanceClaim::SourceUri(uri) => Claim::SourceUri {
                        source_uri: uri.as_str().into(),
                    },
                    ProvenanceClaim::PublisherAttribution {
                        publisher,
                        namespace,
                        source_uri,
                    } => Claim::PublisherAttribution {
                        publisher_name: publisher.as_str().into(),
                        publisher_namespace: namespace.as_ref().map(|x| x.as_str().into()),
                        source_uri: source_uri.as_ref().map(|x| x.as_str().into()),
                    },
                    ProvenanceClaim::Attribution { text, source_uri } => Claim::Attribution {
                        attribution_text: text.as_str().into(),
                        source_uri: source_uri.as_ref().map(|x| x.as_str().into()),
                    },
                })
                .collect(),
        }
    }
}
