use std::{cmp::Ordering, fmt};

use fluent_uri::Uri;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum PackMetadataConflict {
    #[default]
    Reject,
    Overwrite,
    Keep,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PortablePresentationTemplate {
    pub(crate) target: PresentationTargetV1,
    pub(crate) field: PresentationField,
    pub(crate) value: PresentationValue,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub(crate) struct PortableMetadataTemplate {
    pub(crate) reference_labels: Vec<(ReferenceLabel, ReferenceLabelSource)>,
    pub(crate) presentation: Vec<PortablePresentationTemplate>,
    pub(crate) provenance: Vec<ProvenanceClaim>,
}

impl PortableMetadataTemplate {
    pub(crate) fn from_view(view: RevisionMetadataView) -> Self {
        let mut result = Self::default();
        for item in view.items {
            match item {
                RevisionMetadataItem::ReferenceLabel(binding) => result
                    .reference_labels
                    .push((binding.label, binding.source)),
                RevisionMetadataItem::Presentation(p) => {
                    result.presentation.push(PortablePresentationTemplate {
                        target: p.target,
                        field: p.field,
                        value: p.value,
                    })
                }
                RevisionMetadataItem::Provenance { claim, .. } => result.provenance.push(claim),
                RevisionMetadataItem::LocalAlias { .. }
                | RevisionMetadataItem::LocalNote { .. }
                | RevisionMetadataItem::LocalTrust { .. } => {}
            }
        }
        result
    }
}

use super::{
    ActionIdentity, InputIdentity, ManagedOutputIdentity, ParameterIdentity, RevisionContentDigest,
    RevisionIdentity,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MetadataValueError {
    message: String,
}

impl MetadataValueError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MetadataValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for MetadataValueError {}

macro_rules! exact_text {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, PartialEq)]
        pub(crate) struct $name(String);

        impl $name {
            pub(crate) fn parse(value: impl Into<String>) -> Result<Self, MetadataValueError> {
                let value = value.into();
                if value.is_empty() {
                    return Err(MetadataValueError::new(concat!(
                        stringify!($name),
                        " must not be empty"
                    )));
                }
                Ok(Self(value))
            }

            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }

            pub(crate) fn as_bytes(&self) -> &[u8] {
                self.0.as_bytes()
            }
        }

        impl Ord for $name {
            fn cmp(&self, other: &Self) -> Ordering {
                self.as_bytes().cmp(other.as_bytes())
            }
        }

        impl PartialOrd for $name {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }
    };
}

exact_text!(ReferenceLabel);
exact_text!(PresentationValue);
exact_text!(PublisherName);
exact_text!(PublisherNamespace);
exact_text!(AttributionText);
exact_text!(LocalAlias);
exact_text!(LocalNote);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SourceUri(String);

impl SourceUri {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, MetadataValueError> {
        let value = value.into();
        if value.is_empty() || !value.is_ascii() || Uri::parse(value.as_str()).is_err() {
            return Err(MetadataValueError::new(
                "SourceUri must match the ASCII RFC 3986 URI production",
            ));
        }
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl Ord for SourceUri {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}

impl PartialOrd for SourceUri {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CurrentState<T> {
    Absent,
    Present(T),
}

impl<T: Ord> Ord for CurrentState<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Absent, Self::Absent) => Ordering::Equal,
            (Self::Absent, Self::Present(_)) => Ordering::Less,
            (Self::Present(_), Self::Absent) => Ordering::Greater,
            (Self::Present(left), Self::Present(right)) => left.cmp(right),
        }
    }
}

impl<T: Ord> PartialOrd for CurrentState<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn cmp_optional<T: Ord>(left: &Option<T>, right: &Option<T>) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => left.cmp(right),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ReferenceLabelSource {
    Unattributed,
    SourceUri(SourceUri),
    Publisher {
        name: PublisherName,
        namespace: Option<PublisherNamespace>,
    },
    PublisherSourceUri {
        name: PublisherName,
        namespace: Option<PublisherNamespace>,
        source_uri: SourceUri,
    },
}

impl ReferenceLabelSource {
    pub(crate) fn rank(&self) -> u8 {
        match self {
            Self::Unattributed => 0,
            Self::SourceUri(_) => 1,
            Self::Publisher { .. } => 2,
            Self::PublisherSourceUri { .. } => 3,
        }
    }

    pub(crate) fn publisher_name(&self) -> Option<&PublisherName> {
        match self {
            Self::Publisher { name, .. } | Self::PublisherSourceUri { name, .. } => Some(name),
            Self::Unattributed | Self::SourceUri(_) => None,
        }
    }

    pub(crate) fn publisher_namespace(&self) -> Option<&PublisherNamespace> {
        match self {
            Self::Publisher { namespace, .. } | Self::PublisherSourceUri { namespace, .. } => {
                namespace.as_ref()
            }
            Self::Unattributed | Self::SourceUri(_) => None,
        }
    }

    pub(crate) fn source_uri(&self) -> Option<&SourceUri> {
        match self {
            Self::SourceUri(uri)
            | Self::PublisherSourceUri {
                source_uri: uri, ..
            } => Some(uri),
            Self::Unattributed | Self::Publisher { .. } => None,
        }
    }
}

impl Ord for ReferenceLabelSource {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank()
            .cmp(&other.rank())
            .then_with(|| match (self, other) {
                (Self::Unattributed, Self::Unattributed) => Ordering::Equal,
                (Self::SourceUri(left), Self::SourceUri(right)) => left.cmp(right),
                (
                    Self::Publisher {
                        name: left_name,
                        namespace: left_namespace,
                    },
                    Self::Publisher {
                        name: right_name,
                        namespace: right_namespace,
                    },
                ) => left_name
                    .cmp(right_name)
                    .then_with(|| cmp_optional(left_namespace, right_namespace)),
                (
                    Self::PublisherSourceUri {
                        name: left_name,
                        namespace: left_namespace,
                        source_uri: left_uri,
                    },
                    Self::PublisherSourceUri {
                        name: right_name,
                        namespace: right_namespace,
                        source_uri: right_uri,
                    },
                ) => left_name
                    .cmp(right_name)
                    .then_with(|| cmp_optional(left_namespace, right_namespace))
                    .then_with(|| left_uri.cmp(right_uri)),
                _ => Ordering::Equal,
            })
    }
}

impl PartialOrd for ReferenceLabelSource {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReferenceLabelBinding {
    pub(crate) label: ReferenceLabel,
    pub(crate) revision: RevisionIdentity,
    pub(crate) source: ReferenceLabelSource,
}

impl Ord for ReferenceLabelBinding {
    fn cmp(&self, other: &Self) -> Ordering {
        self.label
            .cmp(&other.label)
            .then_with(|| self.revision.cmp(&other.revision))
            .then_with(|| self.source.cmp(&other.source))
    }
}

impl PartialOrd for ReferenceLabelBinding {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PresentationTargetV1 {
    Revision,
    Input(InputIdentity),
    Action(ActionIdentity),
    ActionParameter {
        action: ActionIdentity,
        parameter: ParameterIdentity,
    },
    ManagedOutput {
        action: ActionIdentity,
        output: ManagedOutputIdentity,
    },
    SnapshotCapture,
    SnapshotCaptureParameter(ParameterIdentity),
    SnapshotRestore,
    SnapshotRestoreParameter(ParameterIdentity),
    MigrationEdge(RevisionContentDigest),
    Cleanup,
}

impl PresentationTargetV1 {
    pub(crate) fn rank(&self) -> u8 {
        match self {
            Self::Revision => 0,
            Self::Input(_) => 1,
            Self::Action(_) => 2,
            Self::ActionParameter { .. } => 3,
            Self::ManagedOutput { .. } => 4,
            Self::SnapshotCapture => 5,
            Self::SnapshotCaptureParameter(_) => 6,
            Self::SnapshotRestore => 7,
            Self::SnapshotRestoreParameter(_) => 8,
            Self::MigrationEdge(_) => 9,
            Self::Cleanup => 10,
        }
    }
}

impl Ord for PresentationTargetV1 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank()
            .cmp(&other.rank())
            .then_with(|| match (self, other) {
                (Self::Revision, Self::Revision)
                | (Self::SnapshotCapture, Self::SnapshotCapture)
                | (Self::SnapshotRestore, Self::SnapshotRestore)
                | (Self::Cleanup, Self::Cleanup) => Ordering::Equal,
                (Self::Input(left), Self::Input(right)) => left.cmp(right),
                (Self::Action(left), Self::Action(right)) => left.cmp(right),
                (
                    Self::ActionParameter {
                        action: left_action,
                        parameter: left_parameter,
                    },
                    Self::ActionParameter {
                        action: right_action,
                        parameter: right_parameter,
                    },
                ) => left_action
                    .cmp(right_action)
                    .then_with(|| left_parameter.cmp(right_parameter)),
                (
                    Self::ManagedOutput {
                        action: left_action,
                        output: left_output,
                    },
                    Self::ManagedOutput {
                        action: right_action,
                        output: right_output,
                    },
                ) => left_action
                    .cmp(right_action)
                    .then_with(|| left_output.cmp(right_output)),
                (Self::SnapshotCaptureParameter(left), Self::SnapshotCaptureParameter(right))
                | (Self::SnapshotRestoreParameter(left), Self::SnapshotRestoreParameter(right)) => {
                    left.cmp(right)
                }
                (Self::MigrationEdge(left), Self::MigrationEdge(right)) => left.cmp(right),
                _ => Ordering::Equal,
            })
    }
}

impl PartialOrd for PresentationTargetV1 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PresentationField {
    DisplayName,
    Summary,
    Description,
    Help,
}

impl PresentationField {
    pub(crate) const ALL: [Self; 4] = [
        Self::DisplayName,
        Self::Summary,
        Self::Description,
        Self::Help,
    ];

    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::DisplayName => 0,
            Self::Summary => 1,
            Self::Description => 2,
            Self::Help => 3,
        }
    }
}

impl Ord for PresentationField {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank().cmp(&other.rank())
    }
}

impl PartialOrd for PresentationField {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PresentationMetadata {
    pub(crate) revision: RevisionIdentity,
    pub(crate) target: PresentationTargetV1,
    pub(crate) field: PresentationField,
    pub(crate) value: PresentationValue,
}

impl PresentationMetadata {
    pub(crate) fn cmp_key(&self, other: &Self) -> Ordering {
        self.revision
            .cmp(&other.revision)
            .then_with(|| self.target.cmp(&other.target))
            .then_with(|| self.field.cmp(&other.field))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProvenanceClaim {
    SourceUri(SourceUri),
    PublisherAttribution {
        publisher: PublisherName,
        namespace: Option<PublisherNamespace>,
        source_uri: Option<SourceUri>,
    },
    Attribution {
        text: AttributionText,
        source_uri: Option<SourceUri>,
    },
}

impl ProvenanceClaim {
    pub(crate) fn rank(&self) -> u8 {
        match self {
            Self::SourceUri(_) => 0,
            Self::PublisherAttribution { .. } => 1,
            Self::Attribution { .. } => 2,
        }
    }
}

impl Ord for ProvenanceClaim {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank()
            .cmp(&other.rank())
            .then_with(|| match (self, other) {
                (Self::SourceUri(left), Self::SourceUri(right)) => left.cmp(right),
                (
                    Self::PublisherAttribution {
                        publisher: left_publisher,
                        namespace: left_namespace,
                        source_uri: left_uri,
                    },
                    Self::PublisherAttribution {
                        publisher: right_publisher,
                        namespace: right_namespace,
                        source_uri: right_uri,
                    },
                ) => left_publisher
                    .cmp(right_publisher)
                    .then_with(|| cmp_optional(left_namespace, right_namespace))
                    .then_with(|| cmp_optional(left_uri, right_uri)),
                (
                    Self::Attribution {
                        text: left_text,
                        source_uri: left_uri,
                    },
                    Self::Attribution {
                        text: right_text,
                        source_uri: right_uri,
                    },
                ) => left_text
                    .cmp(right_text)
                    .then_with(|| cmp_optional(left_uri, right_uri)),
                _ => Ordering::Equal,
            })
    }
}

impl PartialOrd for ProvenanceClaim {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrustAssessment {
    Trusted,
    Distrusted,
}

impl TrustAssessment {
    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Trusted => 0,
            Self::Distrusted => 1,
        }
    }
}

impl Ord for TrustAssessment {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank().cmp(&other.rank())
    }
}

impl PartialOrd for TrustAssessment {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MetadataPortability {
    PortableCapable,
    LocalOnly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RevisionMetadataItem {
    ReferenceLabel(ReferenceLabelBinding),
    Presentation(PresentationMetadata),
    Provenance {
        revision: RevisionIdentity,
        claim: ProvenanceClaim,
    },
    LocalAlias {
        revision: RevisionIdentity,
        alias: LocalAlias,
    },
    LocalNote {
        revision: RevisionIdentity,
        note: LocalNote,
    },
    LocalTrust {
        revision: RevisionIdentity,
        trust: TrustAssessment,
    },
}

impl RevisionMetadataItem {
    pub(crate) fn portability(&self) -> MetadataPortability {
        match self {
            Self::ReferenceLabel(_) | Self::Presentation(_) | Self::Provenance { .. } => {
                MetadataPortability::PortableCapable
            }
            Self::LocalAlias { .. } | Self::LocalNote { .. } | Self::LocalTrust { .. } => {
                MetadataPortability::LocalOnly
            }
        }
    }

    pub(crate) fn kind_rank(&self) -> u8 {
        match self {
            Self::ReferenceLabel(_) => 0,
            Self::Presentation(_) => 1,
            Self::Provenance { claim, .. } => 2 + claim.rank(),
            Self::LocalAlias { .. } => 5,
            Self::LocalNote { .. } => 6,
            Self::LocalTrust { .. } => 7,
        }
    }

    pub(crate) fn cmp_key(&self, other: &Self) -> Ordering {
        self.kind_rank()
            .cmp(&other.kind_rank())
            .then_with(|| match (self, other) {
                (Self::ReferenceLabel(left), Self::ReferenceLabel(right)) => left.cmp(right),
                (Self::Presentation(left), Self::Presentation(right)) => left.cmp_key(right),
                (
                    Self::Provenance {
                        revision: left_revision,
                        claim: left_claim,
                    },
                    Self::Provenance {
                        revision: right_revision,
                        claim: right_claim,
                    },
                ) => left_revision
                    .cmp(right_revision)
                    .then_with(|| left_claim.cmp(right_claim)),
                (
                    Self::LocalAlias {
                        revision: left_revision,
                        alias: left_alias,
                    },
                    Self::LocalAlias {
                        revision: right_revision,
                        alias: right_alias,
                    },
                ) => left_alias
                    .cmp(right_alias)
                    .then_with(|| left_revision.cmp(right_revision)),
                (
                    Self::LocalNote {
                        revision: left_revision,
                        ..
                    },
                    Self::LocalNote {
                        revision: right_revision,
                        ..
                    },
                )
                | (
                    Self::LocalTrust {
                        revision: left_revision,
                        ..
                    },
                    Self::LocalTrust {
                        revision: right_revision,
                        ..
                    },
                ) => left_revision.cmp(right_revision),
                _ => Ordering::Equal,
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RevisionMetadataView {
    pub(crate) items: Vec<RevisionMetadataItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RevisionMetadataMutation {
    AddReferenceLabel(ReferenceLabelBinding),
    RemoveReferenceLabel(ReferenceLabelBinding),
    CompareAndSetPresentation {
        target: PresentationTargetV1,
        field: PresentationField,
        expected: CurrentState<PresentationValue>,
        desired: CurrentState<PresentationValue>,
    },
    AddProvenance(ProvenanceClaim),
    RemoveProvenance(ProvenanceClaim),
    CompareAndSetLocalAlias {
        alias: LocalAlias,
        expected: CurrentState<RevisionIdentity>,
        desired: CurrentState<RevisionIdentity>,
    },
    CompareAndSetLocalNote {
        expected: CurrentState<LocalNote>,
        desired: CurrentState<LocalNote>,
    },
    CompareAndSetLocalTrust {
        expected: CurrentState<TrustAssessment>,
        desired: CurrentState<TrustAssessment>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MetadataBatchError {
    message: String,
}

impl fmt::Display for MetadataBatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for MetadataBatchError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RevisionMetadataMutationBatch {
    operations: Vec<RevisionMetadataMutation>,
}

impl RevisionMetadataMutationBatch {
    pub(crate) fn new(
        operations: impl IntoIterator<Item = RevisionMetadataMutation>,
    ) -> Result<Self, MetadataBatchError> {
        let mut canonical = Vec::new();
        for operation in operations {
            if canonical.iter().any(|existing| existing == &operation) {
                continue;
            }
            if canonical
                .iter()
                .any(|existing| operations_contradict(existing, &operation))
            {
                return Err(MetadataBatchError {
                    message: "metadata batch contains contradictory operations".to_owned(),
                });
            }
            canonical.push(operation);
        }
        Ok(Self {
            operations: canonical,
        })
    }

    pub(crate) fn operations(&self) -> &[RevisionMetadataMutation] {
        &self.operations
    }
}

fn operations_contradict(
    left: &RevisionMetadataMutation,
    right: &RevisionMetadataMutation,
) -> bool {
    use RevisionMetadataMutation::*;
    match (left, right) {
        (AddReferenceLabel(left), RemoveReferenceLabel(right))
        | (RemoveReferenceLabel(left), AddReferenceLabel(right)) => left == right,
        (AddProvenance(left), RemoveProvenance(right))
        | (RemoveProvenance(left), AddProvenance(right)) => left == right,
        (
            CompareAndSetPresentation {
                target: left_target,
                field: left_field,
                ..
            },
            CompareAndSetPresentation {
                target: right_target,
                field: right_field,
                ..
            },
        ) => left_target == right_target && left_field == right_field,
        (
            CompareAndSetLocalAlias {
                alias: left_alias, ..
            },
            CompareAndSetLocalAlias {
                alias: right_alias, ..
            },
        ) => left_alias == right_alias,
        (CompareAndSetLocalNote { .. }, CompareAndSetLocalNote { .. })
        | (CompareAndSetLocalTrust { .. }, CompareAndSetLocalTrust { .. }) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{PackageId, RevisionContentDigest};

    fn revision(tag: u8) -> RevisionIdentity {
        RevisionIdentity::new(
            PackageId::from_bytes([tag; 16]),
            RevisionContentDigest::from_bytes([tag; 32]),
        )
    }

    // Test-ID: PR-TEST-0058
    // Verifies: PR-REQ-0249, PR-REQ-0250, PR-REQ-0251, PR-REQ-0252, PR-REQ-0253, PR-REQ-0254
    #[test]
    fn metadata_values_uri_ranks_and_portability_are_exact() {
        assert!(ReferenceLabel::parse("").is_err());
        let composed = ReferenceLabel::parse("é").unwrap();
        let decomposed = ReferenceLabel::parse("e\u{301}").unwrap();
        assert_ne!(composed, decomposed);
        assert_eq!(composed.as_bytes(), "é".as_bytes());
        assert_eq!(decomposed.as_bytes(), "e\u{301}".as_bytes());
        assert_eq!(
            composed.cmp(&decomposed),
            composed.as_bytes().cmp(decomposed.as_bytes())
        );

        let uri = SourceUri::parse("HTTPS://example.test/a/../b/%7e#section").unwrap();
        assert_eq!(uri.as_str(), "HTTPS://example.test/a/../b/%7e#section");
        assert!(SourceUri::parse("../relative").is_err());
        assert!(SourceUri::parse("//example.test/path").is_err());
        assert!(SourceUri::parse("https://例.example/path").is_err());

        assert!(CurrentState::Absent < CurrentState::Present(composed.clone()));
        assert_eq!(ReferenceLabelSource::Unattributed.rank(), 0);
        assert_eq!(ReferenceLabelSource::SourceUri(uri.clone()).rank(), 1);
        assert_eq!(PresentationTargetV1::Cleanup.rank(), 10);
        assert_eq!(PresentationField::Help.rank(), 3);
        assert_eq!(
            ProvenanceClaim::Attribution {
                text: AttributionText::parse("author").unwrap(),
                source_uri: Some(uri),
            }
            .rank(),
            2
        );

        let portable = RevisionMetadataItem::Provenance {
            revision: revision(1),
            claim: ProvenanceClaim::SourceUri(SourceUri::parse("urn:test:value#part").unwrap()),
        };
        let local = RevisionMetadataItem::LocalTrust {
            revision: revision(1),
            trust: TrustAssessment::Trusted,
        };
        assert_eq!(portable.portability(), MetadataPortability::PortableCapable);
        assert_eq!(local.portability(), MetadataPortability::LocalOnly);
    }

    // Test-ID: PR-TEST-0065
    // Verifies: PR-REQ-0255
    #[test]
    fn metadata_batch_coalesces_identity_and_rejects_order_independent_contradictions() {
        let binding = ReferenceLabelBinding {
            label: ReferenceLabel::parse("stable").unwrap(),
            revision: revision(1),
            source: ReferenceLabelSource::Unattributed,
        };
        let add = RevisionMetadataMutation::AddReferenceLabel(binding.clone());
        let remove = RevisionMetadataMutation::RemoveReferenceLabel(binding);
        let coalesced = RevisionMetadataMutationBatch::new([add.clone(), add.clone()]).unwrap();
        assert_eq!(coalesced.operations(), std::slice::from_ref(&add));
        assert!(RevisionMetadataMutationBatch::new([add.clone(), remove.clone()]).is_err());
        assert!(RevisionMetadataMutationBatch::new([remove, add]).is_err());

        let first = RevisionMetadataMutation::CompareAndSetLocalNote {
            expected: CurrentState::Absent,
            desired: CurrentState::Present(LocalNote::parse("first").unwrap()),
        };
        let second = RevisionMetadataMutation::CompareAndSetLocalNote {
            expected: CurrentState::Absent,
            desired: CurrentState::Present(LocalNote::parse("second").unwrap()),
        };
        assert!(RevisionMetadataMutationBatch::new([first.clone(), second.clone()]).is_err());
        assert!(RevisionMetadataMutationBatch::new([second, first]).is_err());
    }
}
