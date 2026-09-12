//! Snapshot identity, normalized manifest, and versioned relational semantics.
//! No CLI, persistence, JSON, filesystem, or protocol-adapter types belong here.

use std::{collections::BTreeMap, fmt};

use super::{
    InputDeclarationV1, InputIdentity, InputProtectionV1, InstanceId, ManagedInputProtection,
    RevisionIdentity, RuntimePath, Sha256Digest, SnapshotId,
};

pub(crate) const SNAPSHOT_TIMESTAMP_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotIntegrityVersion {
    V1,
    V2,
}

impl SnapshotIntegrityVersion {
    pub(crate) fn from_number(value: i64) -> Result<Self, SnapshotValidationError> {
        match value {
            1 => Ok(Self::V1),
            2 => Ok(Self::V2),
            _ => Err(SnapshotValidationError::InvalidFormatVersion),
        }
    }

    pub(crate) const fn number(self) -> u32 {
        match self {
            Self::V1 => 1,
            Self::V2 => 2,
        }
    }

    pub(crate) const fn current_writer() -> Self {
        Self::V2
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotValidationError {
    InvalidFormatVersion,
    InvalidJson,
    InvalidType,
    MissingField,
    UnknownField,
    DuplicateProperty,
    InvalidUnicodeScalar,
    InvalidResourceId,
    InvalidIdentifier,
    InvalidSnapshotPath,
    InvalidDigest,
    InvalidNumber,
    InvalidEnum,
    InvalidBindingState,
    DuplicateSemanticKey,
    InvalidProducerContext,
    IncompleteBindingState,
    InvalidBindingRole,
    InvalidBindingProtection,
    MissingContentBlob,
    ContentDigestMismatch,
}

impl SnapshotValidationError {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::InvalidFormatVersion => "invalid_format_version",
            Self::InvalidJson => "invalid_json",
            Self::InvalidType => "invalid_type",
            Self::MissingField => "missing_field",
            Self::UnknownField => "unknown_field",
            Self::DuplicateProperty => "duplicate_property",
            Self::InvalidUnicodeScalar => "invalid_unicode_scalar",
            Self::InvalidResourceId => "invalid_resource_id",
            Self::InvalidIdentifier => "invalid_identifier",
            Self::InvalidSnapshotPath => "invalid_snapshot_path",
            Self::InvalidDigest => "invalid_digest",
            Self::InvalidNumber => "invalid_number",
            Self::InvalidEnum => "invalid_enum",
            Self::InvalidBindingState => "invalid_binding_state",
            Self::DuplicateSemanticKey => "duplicate_semantic_key",
            Self::InvalidProducerContext => "invalid_producer_context",
            Self::IncompleteBindingState => "incomplete_binding_state",
            Self::InvalidBindingRole => "invalid_binding_role",
            Self::InvalidBindingProtection => "invalid_binding_protection",
            Self::MissingContentBlob => "missing_content_blob",
            Self::ContentDigestMismatch => "content_digest_mismatch",
        }
    }
}

impl fmt::Display for SnapshotValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for SnapshotValidationError {}

/// Deliberately distinct from SnapshotId and payload digests.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct SnapshotIntegrityDigest(Sha256Digest);

impl SnapshotIntegrityDigest {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, SnapshotValidationError> {
        Sha256Digest::parse(value)
            .map(Self)
            .map_err(|_| SnapshotValidationError::InvalidDigest)
    }

    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self::parse(format!("sha256:{}", hex::encode(bytes)))
            .expect("SHA-256 bytes have an exact digest spelling")
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SnapshotIntegrityDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SnapshotIntegrityDigest([withheld])")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SnapshotTimestamp {
    unix_seconds: i64,
    nanoseconds: u32,
}

impl SnapshotTimestamp {
    pub(crate) fn new(seconds: i64, nanoseconds: i64) -> Result<Self, SnapshotValidationError> {
        if !(-SNAPSHOT_TIMESTAMP_SAFE_INTEGER..=SNAPSHOT_TIMESTAMP_SAFE_INTEGER).contains(&seconds)
            || !(0..=999_999_999).contains(&nanoseconds)
        {
            return Err(SnapshotValidationError::InvalidNumber);
        }
        Ok(Self {
            unix_seconds: seconds,
            nanoseconds: nanoseconds as u32,
        })
    }

    pub(crate) fn unix_seconds(self) -> i64 {
        self.unix_seconds
    }
    pub(crate) fn nanoseconds(self) -> u32 {
        self.nanoseconds
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotBindingRole {
    Active,
    Retained,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) enum SnapshotBindingState {
    Absent,
    Bound(Sha256Digest),
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct SnapshotBinding {
    pub(crate) input_id: InputIdentity,
    pub(crate) role: SnapshotBindingRole,
    pub(crate) state: SnapshotBindingState,
    pub(crate) protection: ManagedInputProtection,
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct SnapshotServiceRole(InputIdentity);

impl SnapshotServiceRole {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, SnapshotValidationError> {
        InputIdentity::parse(value)
            .map(Self)
            .map_err(|_| SnapshotValidationError::InvalidIdentifier)
    }
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct SnapshotContentPath(RuntimePath);

impl SnapshotContentPath {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, SnapshotValidationError> {
        RuntimePath::parse(value)
            .map(Self)
            .map_err(|_| SnapshotValidationError::InvalidSnapshotPath)
    }
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct SnapshotServiceContent {
    pub(crate) role: SnapshotServiceRole,
    pub(crate) path: SnapshotContentPath,
    pub(crate) blob_digest: Sha256Digest,
}

/// Shape used at construction; only SnapshotManifest::new validates it.
pub(crate) struct SnapshotManifestParts {
    pub(crate) version: SnapshotIntegrityVersion,
    pub(crate) snapshot_id: SnapshotId,
    pub(crate) producer: RevisionIdentity,
    pub(crate) origin_instance_id: InstanceId,
    pub(crate) captured_at: SnapshotTimestamp,
    pub(crate) managed_bindings: Vec<SnapshotBinding>,
    pub(crate) service_content: Vec<SnapshotServiceContent>,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct SnapshotManifest {
    version: SnapshotIntegrityVersion,
    snapshot_id: SnapshotId,
    producer: RevisionIdentity,
    origin_instance_id: InstanceId,
    captured_at: SnapshotTimestamp,
    managed_bindings: Vec<SnapshotBinding>,
    service_content: Vec<SnapshotServiceContent>,
}

impl fmt::Debug for SnapshotManifest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SnapshotManifest")
            .field("version", &self.version)
            .field("snapshot_id", &self.snapshot_id)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotRelationalVerification {
    NotEvaluated,
    Valid,
}

pub(crate) struct SnapshotProducerContext<'a> {
    pub(crate) revision: &'a RevisionIdentity,
    pub(crate) inputs: &'a [InputDeclarationV1],
}

impl SnapshotManifest {
    pub(crate) fn new(mut parts: SnapshotManifestParts) -> Result<Self, SnapshotValidationError> {
        parts
            .managed_bindings
            .sort_by(|a, b| a.input_id.cmp(&b.input_id));
        if parts
            .managed_bindings
            .windows(2)
            .any(|pair| pair[0].input_id == pair[1].input_id)
        {
            return Err(SnapshotValidationError::DuplicateSemanticKey);
        }
        if parts.managed_bindings.iter().any(|b| {
            b.role == SnapshotBindingRole::Retained && b.state == SnapshotBindingState::Absent
        }) {
            return Err(SnapshotValidationError::InvalidBindingState);
        }
        parts
            .service_content
            .sort_by(|a, b| (&a.role, &a.path).cmp(&(&b.role, &b.path)));
        if parts
            .service_content
            .windows(2)
            .any(|pair| pair[0].role == pair[1].role && pair[0].path == pair[1].path)
        {
            return Err(SnapshotValidationError::DuplicateSemanticKey);
        }
        Ok(Self {
            version: parts.version,
            snapshot_id: parts.snapshot_id,
            producer: parts.producer,
            origin_instance_id: parts.origin_instance_id,
            captured_at: parts.captured_at,
            managed_bindings: parts.managed_bindings,
            service_content: parts.service_content,
        })
    }

    pub(crate) fn version(&self) -> SnapshotIntegrityVersion {
        self.version
    }
    pub(crate) fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }
    pub(crate) fn producer(&self) -> &RevisionIdentity {
        &self.producer
    }
    pub(crate) fn origin_instance_id(&self) -> InstanceId {
        self.origin_instance_id
    }
    pub(crate) fn captured_at(&self) -> SnapshotTimestamp {
        self.captured_at
    }
    pub(crate) fn managed_bindings(&self) -> &[SnapshotBinding] {
        &self.managed_bindings
    }
    pub(crate) fn service_content(&self) -> &[SnapshotServiceContent] {
        &self.service_content
    }

    pub(crate) fn validate_producer(
        &self,
        context: Option<&SnapshotProducerContext<'_>>,
    ) -> Result<SnapshotRelationalVerification, SnapshotValidationError> {
        let Some(context) = context else {
            return Ok(SnapshotRelationalVerification::NotEvaluated);
        };
        if context.revision != &self.producer {
            return Err(SnapshotValidationError::InvalidProducerContext);
        }
        let mut declarations = BTreeMap::new();
        for input in context.inputs {
            if declarations.insert(&input.id, input.protection).is_some() {
                return Err(SnapshotValidationError::InvalidProducerContext);
            }
        }
        let bindings: BTreeMap<_, _> = self
            .managed_bindings
            .iter()
            .map(|b| (&b.input_id, b))
            .collect();
        for (id, declaration) in &declarations {
            let binding = bindings
                .get(id)
                .ok_or(SnapshotValidationError::IncompleteBindingState)?;
            if binding.role != SnapshotBindingRole::Active {
                return Err(SnapshotValidationError::InvalidBindingRole);
            }
            let declared = match declaration {
                InputProtectionV1::Normal => ManagedInputProtection::Normal,
                InputProtectionV1::Secret => ManagedInputProtection::Secret,
            };
            let sticky_v2 = self.version == SnapshotIntegrityVersion::V2
                && matches!(binding.state, SnapshotBindingState::Bound(_))
                && declared == ManagedInputProtection::Normal
                && binding.protection == ManagedInputProtection::Secret;
            if binding.protection != declared && !sticky_v2 {
                return Err(SnapshotValidationError::InvalidBindingProtection);
            }
        }
        for binding in &self.managed_bindings {
            let declared = declarations.contains_key(&binding.input_id);
            if (binding.role == SnapshotBindingRole::Active) != declared {
                return Err(SnapshotValidationError::InvalidBindingRole);
            }
        }
        Ok(SnapshotRelationalVerification::Valid)
    }
}
