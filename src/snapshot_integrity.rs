//! Production Snapshot V1/V2 codec. The original xtask V1 verifier is unchanged.
//! JSON is an adapter representation; callers receive typed Domain manifests.
#![allow(dead_code)] // S1 substrate; storage and execution callers arrive in S3/S5.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{self, Read},
};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    domain::{
        BlobAccountingProfile, CapabilityRefusal, ManagedInputProtection, RevisionIdentity,
        SNAPSHOT_TIMESTAMP_SAFE_INTEGER, Sha256Digest, SnapshotBinding, SnapshotBindingRole,
        SnapshotBindingState, SnapshotBlobBudget, SnapshotCapability, SnapshotContentPath,
        SnapshotIntegrityDigest, SnapshotIntegrityVersion, SnapshotManifest, SnapshotManifestParts,
        SnapshotProducerContext, SnapshotRelationalVerification, SnapshotServiceContent,
        SnapshotServiceRole, SnapshotTimestamp, SnapshotValidationError,
    },
    strict_json::{RawJsonValue, StrictJsonErrorKind, parse_json},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotCodecError {
    Validation(SnapshotValidationError),
    Capability(CapabilityRefusal),
    ContentIo(io::ErrorKind),
    Serialization,
}

impl From<SnapshotValidationError> for SnapshotCodecError {
    fn from(value: SnapshotValidationError) -> Self {
        Self::Validation(value)
    }
}
impl From<CapabilityRefusal> for SnapshotCodecError {
    fn from(value: CapabilityRefusal) -> Self {
        Self::Capability(value)
    }
}
impl fmt::Display for SnapshotCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(error) => error.fmt(f),
            Self::Capability(error) => error.fmt(f),
            Self::ContentIo(_) => f.write_str("Snapshot content I/O failed"),
            Self::Serialization => f.write_str("Snapshot canonical encoding failed"),
        }
    }
}
impl std::error::Error for SnapshotCodecError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotBlobReadError {
    Missing,
    Io(io::ErrorKind),
}

pub(crate) struct VerifiedSnapshotManifest {
    manifest: SnapshotManifest,
    canonical: Vec<u8>,
    digest: SnapshotIntegrityDigest,
    relational: SnapshotRelationalVerification,
}

impl fmt::Debug for VerifiedSnapshotManifest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerifiedSnapshotManifest")
            .field("manifest", &self.manifest)
            .field("relational", &self.relational)
            .finish_non_exhaustive()
    }
}

/// Full referenced-content verification, not a claim of evaluated producer semantics.
pub(crate) struct VerifiedSnapshotContent {
    lengths: BTreeMap<Sha256Digest, u64>,
}

impl VerifiedSnapshotContent {
    pub(crate) fn lengths(&self) -> &BTreeMap<Sha256Digest, u64> {
        &self.lengths
    }
}

impl fmt::Debug for VerifiedSnapshotContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VerifiedSnapshotContent([withheld])")
    }
}

impl VerifiedSnapshotManifest {
    pub(crate) fn manifest(&self) -> &SnapshotManifest {
        &self.manifest
    }
    pub(crate) fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }
    pub(crate) fn integrity_digest(&self) -> &SnapshotIntegrityDigest {
        &self.digest
    }
    pub(crate) fn relational_verification(&self) -> SnapshotRelationalVerification {
        self.relational
    }

    pub(crate) fn verify_content<R: Read>(
        &self,
        mut open: impl FnMut(&Sha256Digest) -> Result<R, SnapshotBlobReadError>,
    ) -> Result<VerifiedSnapshotContent, SnapshotCodecError> {
        let references: BTreeSet<_> = self
            .manifest
            .managed_bindings()
            .iter()
            .filter_map(|b| match &b.state {
                SnapshotBindingState::Absent => None,
                SnapshotBindingState::Bound(d) => Some(d),
            })
            .chain(
                self.manifest
                    .service_content()
                    .iter()
                    .map(|c| &c.blob_digest),
            )
            .collect();
        let mut budget = SnapshotBlobBudget::new(BlobAccountingProfile::SnapshotStorage);
        let mut lengths = BTreeMap::new();
        let mut buffer = [0_u8; 64 * 1024];
        for digest in references {
            let mut reader = open(digest).map_err(|error| match error {
                SnapshotBlobReadError::Missing => {
                    SnapshotCodecError::Validation(SnapshotValidationError::MissingContentBlob)
                }
                SnapshotBlobReadError::Io(kind) => SnapshotCodecError::ContentIo(kind),
            })?;
            let mut hasher = Sha256::new();
            let mut length = 0_u64;
            loop {
                let count = match reader.read(&mut buffer) {
                    Ok(count) => count,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(SnapshotCodecError::ContentIo(error.kind())),
                };
                if count == 0 {
                    break;
                }
                length = length.checked_add(count as u64).ok_or(CapabilityRefusal {
                    capability: SnapshotCapability::StoredBlob,
                })?;
                SnapshotCapability::StoredBlob.check(length)?;
                let closure = budget
                    .total()
                    .checked_add(length)
                    .ok_or(CapabilityRefusal {
                        capability: SnapshotCapability::StoredClosure,
                    })?;
                SnapshotCapability::StoredClosure.check(closure)?;
                hasher.update(&buffer[..count]);
            }
            let actual: [u8; 32] = hasher.finalize().into();
            if actual != digest.to_bytes() {
                return Err(SnapshotValidationError::ContentDigestMismatch.into());
            }
            budget
                .record(digest.clone(), length)
                .map_err(|error| match error {
                    crate::domain::AccountingError::Capability(error) => {
                        SnapshotCodecError::Capability(error)
                    }
                    crate::domain::AccountingError::InconsistentSourceLength => {
                        SnapshotCodecError::Validation(
                            SnapshotValidationError::ContentDigestMismatch,
                        )
                    }
                })?;
            lengths.insert(digest.clone(), length);
        }
        Ok(VerifiedSnapshotContent { lengths })
    }
}

pub(crate) fn decode_snapshot_manifest(
    selected: SnapshotIntegrityVersion,
    bytes: &[u8],
    producer: Option<&SnapshotProducerContext<'_>>,
) -> Result<VerifiedSnapshotManifest, SnapshotCodecError> {
    SnapshotCapability::RawManifest.check(bytes.len() as u64)?;
    check_json_depth(bytes)?;
    let value =
        parse_json(bytes, SnapshotCapability::RawManifest.maximum() as usize).map_err(|error| {
            match error.kind() {
                StrictJsonErrorKind::InvalidUtf8 | StrictJsonErrorKind::InvalidUnicodeScalar => {
                    SnapshotValidationError::InvalidUnicodeScalar
                }
                StrictJsonErrorKind::DuplicateProperty => {
                    SnapshotValidationError::DuplicateProperty
                }
                StrictJsonErrorKind::InvalidJson => SnapshotValidationError::InvalidJson,
            }
        })?;
    let mut object = take_object(value)?;
    check_fields(
        &object,
        &[
            "format_version",
            "snapshot_id",
            "producer",
            "origin_instance_id",
            "captured_at",
            "managed_bindings",
            "service_content",
        ],
    )?;
    if exact_integer(&take(&mut object, "format_version")?)? != i64::from(selected.number()) {
        return Err(SnapshotValidationError::InvalidFormatVersion.into());
    }
    let snapshot_id = take_string(&mut object, "snapshot_id")?
        .parse()
        .map_err(|_| SnapshotValidationError::InvalidResourceId)?;
    let origin_instance_id = take_string(&mut object, "origin_instance_id")?
        .parse()
        .map_err(|_| SnapshotValidationError::InvalidResourceId)?;
    let mut revision = take_object(take(&mut object, "producer")?)?;
    check_fields(&revision, &["package_id", "revision_content_digest"])?;
    let revision = RevisionIdentity::new(
        take_string(&mut revision, "package_id")?
            .parse()
            .map_err(|_| SnapshotValidationError::InvalidResourceId)?,
        take_string(&mut revision, "revision_content_digest")?
            .parse()
            .map_err(|_| SnapshotValidationError::InvalidDigest)?,
    );
    let mut time = take_object(take(&mut object, "captured_at")?)?;
    check_fields(&time, &["unix_seconds", "nanoseconds"])?;
    let captured_at = SnapshotTimestamp::new(
        exact_integer(&take(&mut time, "unix_seconds")?)?,
        exact_integer(&take(&mut time, "nanoseconds")?)?,
    )?;
    let bindings = take_array(take(&mut object, "managed_bindings")?)?;
    let content = take_array(take(&mut object, "service_content")?)?;
    let count = (bindings.len() as u64)
        .checked_add(content.len() as u64)
        .ok_or(CapabilityRefusal {
            capability: SnapshotCapability::Descriptors,
        })?;
    SnapshotCapability::Descriptors.check(count)?;
    let managed_bindings = bindings
        .into_iter()
        .map(decode_binding)
        .collect::<Result<Vec<_>, _>>()?;
    let service_content = content
        .into_iter()
        .map(decode_content)
        .collect::<Result<Vec<_>, _>>()?;
    encode_snapshot_manifest(
        SnapshotManifest::new(SnapshotManifestParts {
            version: selected,
            snapshot_id,
            producer: revision,
            origin_instance_id,
            captured_at,
            managed_bindings,
            service_content,
        })?,
        producer,
    )
}

pub(crate) fn encode_snapshot_manifest(
    manifest: SnapshotManifest,
    producer: Option<&SnapshotProducerContext<'_>>,
) -> Result<VerifiedSnapshotManifest, SnapshotCodecError> {
    let count = (manifest.managed_bindings().len() as u64)
        .checked_add(manifest.service_content().len() as u64)
        .ok_or(CapabilityRefusal {
            capability: SnapshotCapability::Descriptors,
        })?;
    SnapshotCapability::Descriptors.check(count)?;
    let relational = manifest.validate_producer(producer)?;
    let canonical = serde_jcs::to_vec(&json_manifest(&manifest))
        .map_err(|_| SnapshotCodecError::Serialization)?;
    SnapshotCapability::CanonicalManifest.check(canonical.len() as u64)?;
    let mut hasher = Sha256::new();
    hasher.update(b"pactrun.snapshot-integrity-digest\0");
    hasher.update(manifest.version().number().to_be_bytes());
    hasher.update(b"snapshot-integrity-manifest\0");
    hasher.update((canonical.len() as u64).to_be_bytes());
    hasher.update(&canonical);
    let digest = SnapshotIntegrityDigest::from_bytes(hasher.finalize().into());
    Ok(VerifiedSnapshotManifest {
        manifest,
        canonical,
        digest,
        relational,
    })
}

fn json_manifest(manifest: &SnapshotManifest) -> Value {
    let bindings: Vec<_> = manifest.managed_bindings().iter().map(|binding| {
        let mut value = json!({
            "input_id": binding.input_id.as_str(),
            "role": match binding.role { SnapshotBindingRole::Active => "active", SnapshotBindingRole::Retained => "retained" },
            "protection": match binding.protection { ManagedInputProtection::Normal => "normal", ManagedInputProtection::Secret => "secret" },
            "state": match binding.state { SnapshotBindingState::Absent => "absent", SnapshotBindingState::Bound(_) => "bound" },
        });
        if let SnapshotBindingState::Bound(digest) = &binding.state { value["blob_digest"] = json!(digest.as_str()); }
        value
    }).collect();
    let content: Vec<_> = manifest.service_content().iter().map(|c| json!({ "role": c.role.as_str(), "path": c.path.as_str(), "blob_digest": c.blob_digest.as_str() })).collect();
    json!({
        "format_version": manifest.version().number(), "snapshot_id": manifest.snapshot_id().to_string(),
        "producer": { "package_id": manifest.producer().package_id.to_string(), "revision_content_digest": manifest.producer().content_digest.to_string() },
        "origin_instance_id": manifest.origin_instance_id().to_string(),
        "captured_at": { "unix_seconds": manifest.captured_at().unix_seconds(), "nanoseconds": manifest.captured_at().nanoseconds() },
        "managed_bindings": bindings, "service_content": content,
    })
}

fn decode_binding(value: RawJsonValue) -> Result<SnapshotBinding, SnapshotValidationError> {
    let mut object = take_object(value)?;
    let state = take_string(&mut object, "state")?;
    let fields: &[&str] = match state.as_str() {
        "absent" => &["input_id", "role", "protection"],
        "bound" => &["input_id", "role", "protection", "blob_digest"],
        _ => return Err(SnapshotValidationError::InvalidEnum),
    };
    check_fields(&object, fields)?;
    let role = match take_string(&mut object, "role")?.as_str() {
        "active" => SnapshotBindingRole::Active,
        "retained" => SnapshotBindingRole::Retained,
        _ => return Err(SnapshotValidationError::InvalidEnum),
    };
    let protection = match take_string(&mut object, "protection")?.as_str() {
        "normal" => ManagedInputProtection::Normal,
        "secret" => ManagedInputProtection::Secret,
        _ => return Err(SnapshotValidationError::InvalidEnum),
    };
    Ok(SnapshotBinding {
        input_id: super::domain::InputIdentity::parse(take_string(&mut object, "input_id")?)
            .map_err(|_| SnapshotValidationError::InvalidIdentifier)?,
        role,
        protection,
        state: if state == "absent" {
            SnapshotBindingState::Absent
        } else {
            SnapshotBindingState::Bound(take_digest(&mut object)?)
        },
    })
}

fn decode_content(value: RawJsonValue) -> Result<SnapshotServiceContent, SnapshotValidationError> {
    let mut object = take_object(value)?;
    check_fields(&object, &["role", "path", "blob_digest"])?;
    Ok(SnapshotServiceContent {
        role: SnapshotServiceRole::parse(take_string(&mut object, "role")?)?,
        path: SnapshotContentPath::parse(take_string(&mut object, "path")?)?,
        blob_digest: take_digest(&mut object)?,
    })
}

type Object = BTreeMap<String, RawJsonValue>;
fn take_object(value: RawJsonValue) -> Result<Object, SnapshotValidationError> {
    match value {
        RawJsonValue::Object(fields) => Ok(fields.into_iter().collect()),
        _ => Err(SnapshotValidationError::InvalidType),
    }
}
fn take_array(value: RawJsonValue) -> Result<Vec<RawJsonValue>, SnapshotValidationError> {
    match value {
        RawJsonValue::Array(values) => Ok(values),
        _ => Err(SnapshotValidationError::InvalidType),
    }
}
fn check_fields(object: &Object, fields: &[&str]) -> Result<(), SnapshotValidationError> {
    if object.keys().any(|key| !fields.contains(&key.as_str())) {
        return Err(SnapshotValidationError::UnknownField);
    }
    if fields.iter().any(|key| !object.contains_key(*key)) {
        return Err(SnapshotValidationError::MissingField);
    }
    Ok(())
}
fn take(object: &mut Object, key: &str) -> Result<RawJsonValue, SnapshotValidationError> {
    object
        .remove(key)
        .ok_or(SnapshotValidationError::MissingField)
}
fn take_string(object: &mut Object, key: &str) -> Result<String, SnapshotValidationError> {
    match take(object, key)? {
        RawJsonValue::String(value) => Ok(value),
        _ => Err(SnapshotValidationError::InvalidType),
    }
}
fn take_digest(object: &mut Object) -> Result<Sha256Digest, SnapshotValidationError> {
    Sha256Digest::parse(take_string(object, "blob_digest")?)
        .map_err(|_| SnapshotValidationError::InvalidDigest)
}

/// Interpret the exact decimal token before any binary64 conversion.
pub(crate) fn exact_integer(value: &RawJsonValue) -> Result<i64, SnapshotValidationError> {
    let RawJsonValue::Number(token) = value else {
        return Err(SnapshotValidationError::InvalidType);
    };
    let negative = token.starts_with('-');
    let unsigned = token.strip_prefix('-').unwrap_or(token);
    let (mantissa, exponent) = unsigned
        .split_once(['e', 'E'])
        .map_or((unsigned, None), |(a, b)| (a, Some(b)));
    let exponent = exponent
        .map(|e| e.parse::<i64>())
        .transpose()
        .map_err(|_| SnapshotValidationError::InvalidNumber)?
        .unwrap_or(0);
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut digits = format!("{whole}{fraction}");
    if digits.bytes().all(|b| b == b'0') {
        return Ok(0);
    }
    let fraction_length =
        i64::try_from(fraction.len()).map_err(|_| SnapshotValidationError::InvalidNumber)?;
    let shift = exponent
        .checked_sub(fraction_length)
        .ok_or(SnapshotValidationError::InvalidNumber)?;
    if shift < 0 {
        let remove = shift
            .checked_neg()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or(SnapshotValidationError::InvalidNumber)?;
        if remove > digits.len() || !digits[digits.len() - remove..].bytes().all(|b| b == b'0') {
            return Err(SnapshotValidationError::InvalidNumber);
        }
        digits.truncate(digits.len() - remove);
    } else {
        let append = usize::try_from(shift).map_err(|_| SnapshotValidationError::InvalidNumber)?;
        if append > 32 {
            return Err(SnapshotValidationError::InvalidNumber);
        }
        digits.extend(std::iter::repeat_n('0', append));
    }
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return Ok(0);
    }
    if digits.len() > 16 {
        return Err(SnapshotValidationError::InvalidNumber);
    }
    let magnitude = digits
        .parse::<i64>()
        .map_err(|_| SnapshotValidationError::InvalidNumber)?;
    if magnitude > SNAPSHOT_TIMESTAMP_SAFE_INTEGER {
        return Err(SnapshotValidationError::InvalidNumber);
    }
    Ok(if negative { -magnitude } else { magnitude })
}

/// Bounded, allocation-free depth preflight. Full syntax/Unicode checks follow.
pub(crate) fn check_json_depth(bytes: &[u8]) -> Result<(), CapabilityRefusal> {
    let (mut depth, mut string, mut escaped) = (0_u64, false, false);
    for &byte in bytes {
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
        } else {
            match byte {
                b'"' => string = true,
                b'{' | b'[' => {
                    depth += 1;
                    SnapshotCapability::JsonDepth.check(depth)?;
                }
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "snapshot_integrity_tests.rs"]
mod tests;
