//! Fixed M4 build capabilities, not Snapshot format-validity predicates.
//!
//! Keep acquisition, stored closure, parser, and Restore budgets independent.
//! These counters own no I/O and do not establish cryptographic verification.

use std::{collections::BTreeMap, fmt};

use super::{InstanceId, ManagedInputPayloadId, Sha256Digest};

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;
const GIB: u64 = 1024 * MIB;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotCapability {
    StoredBlob,
    StoredClosure,
    Descriptors,
    RawManifest,
    CanonicalManifest,
    CaptureServiceBlob,
    CaptureServiceClosure,
    CaptureAcquisition,
    RestoreInput,
    RestoreExpansion,
    BundleEnvelope,
    BundleEntries,
    BundleMetadata,
    BundleBytes,
    JsonDepth,
}

impl SnapshotCapability {
    pub(crate) const fn maximum(self) -> u64 {
        match self {
            Self::StoredBlob => 8 * GIB,
            Self::StoredClosure => 32 * GIB,
            Self::Descriptors => 65_536,
            Self::RawManifest => 16 * MIB,
            Self::CanonicalManifest => 16 * MIB,
            Self::CaptureServiceBlob => 8 * GIB,
            Self::CaptureServiceClosure => 16 * GIB,
            Self::CaptureAcquisition => 32 * GIB,
            Self::RestoreInput => super::MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1,
            Self::RestoreExpansion => 32 * GIB,
            Self::BundleEnvelope => 64 * KIB,
            Self::BundleEntries => 65_538,
            Self::BundleMetadata => 64 * MIB,
            Self::BundleBytes => 32 * GIB + 128 * MIB,
            Self::JsonDepth => 16,
        }
    }

    pub(crate) fn check(self, measured: u64) -> Result<(), CapabilityRefusal> {
        if measured > self.maximum() {
            Err(CapabilityRefusal { capability: self })
        } else {
            Ok(())
        }
    }

    fn add(self, current: u64, amount: u64) -> Result<u64, CapabilityRefusal> {
        let next = current
            .checked_add(amount)
            .ok_or(CapabilityRefusal { capability: self })?;
        self.check(next)?;
        Ok(next)
    }
}

/// Do not retain measured lengths, source identities, or digests in diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CapabilityRefusal {
    pub(crate) capability: SnapshotCapability,
}

impl fmt::Display for CapabilityRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("operation exceeds a fixed Snapshot build capability")
    }
}

impl std::error::Error for CapabilityRefusal {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AccountingError {
    Capability(CapabilityRefusal),
    InconsistentSourceLength,
}

impl From<CapabilityRefusal> for AccountingError {
    fn from(value: CapabilityRefusal) -> Self {
        Self::Capability(value)
    }
}

impl fmt::Display for AccountingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Capability(error) => error.fmt(f),
            Self::InconsistentSourceLength => f.write_str("source length is inconsistent"),
        }
    }
}

impl std::error::Error for AccountingError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BlobAccountingProfile {
    SnapshotStorage,
    CapturedServiceContent,
}

/// One Snapshot's distinct-digest closure, never a global CAS or ownership map.
pub(crate) struct SnapshotBlobBudget {
    profile: BlobAccountingProfile,
    blobs: BTreeMap<Sha256Digest, u64>,
    total: u64,
}

impl SnapshotBlobBudget {
    pub(crate) fn new(profile: BlobAccountingProfile) -> Self {
        Self {
            profile,
            blobs: BTreeMap::new(),
            total: 0,
        }
    }

    pub(crate) fn record(
        &mut self,
        digest: Sha256Digest,
        byte_length: u64,
    ) -> Result<(), AccountingError> {
        let (single, closure) = match self.profile {
            BlobAccountingProfile::SnapshotStorage => (
                SnapshotCapability::StoredBlob,
                SnapshotCapability::StoredClosure,
            ),
            BlobAccountingProfile::CapturedServiceContent => (
                SnapshotCapability::CaptureServiceBlob,
                SnapshotCapability::CaptureServiceClosure,
            ),
        };
        single.check(byte_length)?;
        if let Some(existing) = self.blobs.get(&digest) {
            return if *existing == byte_length {
                Ok(())
            } else {
                Err(AccountingError::InconsistentSourceLength)
            };
        }
        let next = closure.add(self.total, byte_length)?;
        self.blobs.insert(digest, byte_length);
        self.total = next;
        Ok(())
    }

    pub(crate) fn total(&self) -> u64 {
        self.total
    }
}

/// Allocated by the candidate materializer within one Capture operation.
/// No host path, filesystem object identity, digest, or wire field is an ID.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct CaptureSourceId(u64);

impl CaptureSourceId {
    pub(crate) fn from_operation_index(index: u64) -> Self {
        Self(index)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum CaptureAcquisitionSource {
    Managed {
        instance: InstanceId,
        payload: ManagedInputPayloadId,
    },
    Candidate(CaptureSourceId),
}

/// Allocate one counter per Capture. Repeated descriptors share a logical source;
/// different submitted sources never merge merely because their bytes match.
#[derive(Default)]
pub(crate) struct CaptureAcquisitionBudget {
    sources: BTreeMap<CaptureAcquisitionSource, u64>,
    total: u64,
}

impl CaptureAcquisitionBudget {
    pub(crate) fn record(
        &mut self,
        source: CaptureAcquisitionSource,
        byte_length: u64,
    ) -> Result<(), AccountingError> {
        if let Some(existing) = self.sources.get(&source) {
            return if *existing == byte_length {
                Ok(())
            } else {
                Err(AccountingError::InconsistentSourceLength)
            };
        }
        let next = SnapshotCapability::CaptureAcquisition.add(self.total, byte_length)?;
        self.sources.insert(source, byte_length);
        self.total = next;
        Ok(())
    }

    pub(crate) fn total(&self) -> u64 {
        self.total
    }
}

/// Charges every logical materialization, without digest deduplication.
#[derive(Default)]
pub(crate) struct RestoreExpansionBudget {
    total: u64,
}

impl RestoreExpansionBudget {
    pub(crate) fn record_managed_input(
        &mut self,
        byte_length: u64,
    ) -> Result<(), CapabilityRefusal> {
        SnapshotCapability::RestoreInput.check(byte_length)?;
        self.record_service_descriptor(byte_length)
    }

    pub(crate) fn record_service_descriptor(
        &mut self,
        byte_length: u64,
    ) -> Result<(), CapabilityRefusal> {
        self.total = SnapshotCapability::RestoreExpansion.add(self.total, byte_length)?;
        Ok(())
    }

    pub(crate) fn total(&self) -> u64 {
        self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: u8) -> Sha256Digest {
        Sha256Digest::parse(format!("sha256:{}", hex::encode([byte; 32]))).unwrap()
    }

    // Test-ID: PR-TEST-0178
    // Verifies: PR-REQ-0293
    #[test]
    fn all_capability_limits_are_exact_and_inclusive() {
        use SnapshotCapability::*;
        let limits = [
            (StoredBlob, 8 * GIB),
            (StoredClosure, 32 * GIB),
            (Descriptors, 65_536),
            (RawManifest, 16 * MIB),
            (CanonicalManifest, 16 * MIB),
            (CaptureServiceBlob, 8 * GIB),
            (CaptureServiceClosure, 16 * GIB),
            (CaptureAcquisition, 32 * GIB),
            (RestoreInput, 512 * MIB),
            (RestoreExpansion, 32 * GIB),
            (BundleEnvelope, 64 * KIB),
            (BundleEntries, 65_538),
            (BundleMetadata, 64 * MIB),
            (BundleBytes, 32 * GIB + 128 * MIB),
            (JsonDepth, 16),
        ];
        for (kind, expected) in limits {
            assert_eq!(kind.maximum(), expected);
            assert!(kind.check(0).is_ok());
            assert!(kind.check(expected - 1).is_ok());
            assert!(kind.check(expected).is_ok());
            assert_eq!(
                kind.check(expected + 1),
                Err(CapabilityRefusal { capability: kind })
            );
            assert!(kind.check(u64::MAX).is_err());
        }
    }

    // Test-ID: PR-TEST-0179
    // Verifies: PR-REQ-0294
    #[test]
    fn acquisition_counts_distinct_logical_sources_not_descriptors_or_digests() {
        let mut budget = CaptureAcquisitionBudget::default();
        let managed = CaptureAcquisitionSource::Managed {
            instance: InstanceId::from_bytes([1; 16]),
            payload: ManagedInputPayloadId::from_bytes([2; 16]),
        };
        budget.record(managed, 8 * GIB).unwrap();
        budget.record(managed, 8 * GIB).unwrap();
        assert_eq!(budget.total(), 8 * GIB);
        for index in 0..3 {
            let source =
                CaptureAcquisitionSource::Candidate(CaptureSourceId::from_operation_index(index));
            budget.record(source, 8 * GIB).unwrap();
            budget.record(source, 8 * GIB).unwrap();
        }
        assert_eq!(budget.total(), 32 * GIB);
        let another = CaptureAcquisitionSource::Candidate(CaptureSourceId::from_operation_index(3));
        assert!(matches!(
            budget.record(another, 1),
            Err(AccountingError::Capability(_))
        ));
        assert_eq!(budget.total(), 32 * GIB);
        assert!(budget.record(another, u64::MAX).is_err());
        assert_eq!(budget.total(), 32 * GIB);
        assert_eq!(
            budget.record(managed, 1),
            Err(AccountingError::InconsistentSourceLength)
        );
    }

    // Test-ID: PR-TEST-0180
    // Verifies: PR-REQ-0293
    #[test]
    fn import_capture_and_restore_profiles_remain_separate() {
        let mut stored = SnapshotBlobBudget::new(BlobAccountingProfile::SnapshotStorage);
        let mut capture = SnapshotBlobBudget::new(BlobAccountingProfile::CapturedServiceContent);
        for index in 0..5 {
            let blob = digest(index);
            stored.record(blob.clone(), 4 * GIB).unwrap();
            if index < 4 {
                capture.record(blob, 4 * GIB).unwrap();
            } else {
                assert!(capture.record(blob, 4 * GIB).is_err());
            }
        }
        assert_eq!(stored.total(), 20 * GIB);
        assert_eq!(capture.total(), 16 * GIB);
        let oversized_input = digest(8);
        stored.record(oversized_input.clone(), 600 * MIB).unwrap();
        let before = stored.total();
        stored.record(oversized_input, 600 * MIB).unwrap();
        assert_eq!(stored.total(), before);
        let mut restore = RestoreExpansionBudget::default();
        assert!(restore.record_managed_input(600 * MIB).is_err());
        assert_eq!(restore.total(), 0);
        // Repeated descriptors expand even when storage would deduplicate bytes.
        for _ in 0..32 {
            restore.record_service_descriptor(GIB).unwrap();
        }
        assert!(restore.record_service_descriptor(1).is_err());
        assert_eq!(restore.total(), 32 * GIB);
        assert!(restore.record_service_descriptor(u64::MAX).is_err());
    }

    // Test-ID: PR-TEST-0181
    // Verifies: PR-REQ-0293, PR-REQ-0294
    #[test]
    fn refusal_diagnostics_do_not_retain_source_values_and_rejections_are_atomic() {
        let mut stored = SnapshotBlobBudget::new(BlobAccountingProfile::SnapshotStorage);
        let empty_blob = digest(77);
        stored.record(empty_blob.clone(), 0).unwrap();
        assert_eq!(
            stored.record(empty_blob, 1),
            Err(AccountingError::InconsistentSourceLength)
        );
        assert_eq!(stored.total(), 0);
        let error = SnapshotCapability::StoredBlob.check(u64::MAX).unwrap_err();
        assert_eq!(
            error.to_string(),
            "operation exceeds a fixed Snapshot build capability"
        );
        assert!(!format!("{error:?}").contains(&u64::MAX.to_string()));
        for index in 0..4 {
            stored.record(digest(index), 8 * GIB).unwrap();
        }
        assert!(stored.record(digest(5), 1).is_err());
        assert_eq!(stored.total(), 32 * GIB);
    }
}
