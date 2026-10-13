//! Historical metadata never confers payload retention or Hook/read authority.
use super::{
    InputIdentity, ManagedInputPayloadId, ManagedInputProtection, SnapshotId,
    SnapshotIntegrityDigest,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputHistoryScope {
    Current,
    Snapshot,
    Source,
    TargetPrepared,
    TargetCommitted,
}
impl InputHistoryScope {
    pub(crate) fn rank(self) -> i64 {
        match self {
            Self::Current => 0,
            Self::Snapshot => 1,
            Self::Source => 2,
            Self::TargetPrepared => 3,
            Self::TargetCommitted => 4,
        }
    }
    pub(crate) fn from_rank(rank: i64) -> Option<Self> {
        match rank {
            0 => Some(Self::Current),
            1 => Some(Self::Snapshot),
            2 => Some(Self::Source),
            3 => Some(Self::TargetPrepared),
            4 => Some(Self::TargetCommitted),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputHistoryCoverage {
    AdmittedBindings,
    ManagedContext,
}
impl InputHistoryCoverage {
    pub(crate) fn rank(self) -> i64 {
        match self {
            Self::AdmittedBindings => 0,
            Self::ManagedContext => 1,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputHistoryRole {
    Active,
    Retained,
}
impl InputHistoryRole {
    pub(crate) fn rank(self) -> i64 {
        match self {
            Self::Active => 0,
            Self::Retained => 1,
        }
    }
}
#[derive(Clone, Eq, PartialEq)]
pub(crate) enum InputHistoryOrigin {
    Absent,
    Managed(ManagedInputPayloadId),
    Snapshot,
    Transient,
    PendingOutput,
}
impl InputHistoryOrigin {
    pub(crate) fn rank(&self) -> i64 {
        match self {
            Self::Absent => 0,
            Self::Managed(_) => 1,
            Self::Snapshot => 2,
            Self::Transient => 3,
            Self::PendingOutput => 4,
        }
    }
    pub(crate) fn payload(&self) -> Option<ManagedInputPayloadId> {
        if let Self::Managed(id) = self {
            Some(*id)
        } else {
            None
        }
    }
}
impl std::fmt::Debug for InputHistoryOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Absent => "Absent",
            Self::Managed(_) => "Managed { identity withheld }",
            Self::Snapshot => "Snapshot",
            Self::Transient => "Transient",
            Self::PendingOutput => "PendingOutput",
        })
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HistoricalInput {
    pub(crate) input: InputIdentity,
    pub(crate) role: InputHistoryRole,
    pub(crate) protection: ManagedInputProtection,
    pub(crate) origin: InputHistoryOrigin,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HistoricalInputContext {
    pub(crate) edge_index: Option<usize>,
    pub(crate) scope: InputHistoryScope,
    pub(crate) coverage: InputHistoryCoverage,
    pub(crate) snapshot: Option<SnapshotId>,
    pub(crate) snapshot_integrity: Option<SnapshotIntegrityDigest>,
    pub(crate) inputs: Vec<HistoricalInput>,
}
