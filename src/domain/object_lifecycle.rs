//! Object lifetime policy, distinct from service retirement and physical collection.
use super::{RevisionIdentity, RunId, SnapshotId};

#[derive(Clone, Debug)]
pub(crate) enum ObjectDeletion {
    Snapshot(SnapshotId),
    Revision(RevisionIdentity),
    Run { run: RunId, delete_artifacts: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeletionBlock {
    ActiveInstance,
    RevisionPin,
    ServiceReference,
    SnapshotInUse,
    RunningRun,
    RecoveryEvidence,
    RetirementEvidence,
    ArtifactsRemain,
}

impl DeletionBlock {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::ActiveInstance => "Revision is retained by an active Instance",
            Self::RevisionPin => "Revision is retained by an accepted Run or transition",
            Self::ServiceReference => "Revision is retained by service associations or preparation",
            Self::SnapshotInUse => {
                "Snapshot is retained by an accepted Restore or recovery obligation"
            }
            Self::RunningRun => "Run has not finished execution",
            Self::RecoveryEvidence => {
                "Run is retained by execution, checkpoint or recovery evidence"
            }
            Self::RetirementEvidence => "Run is retained by a retirement obligation or receipt",
            Self::ArtifactsRemain => {
                "Run still owns Artifacts; explicitly use --delete-artifacts to remove them"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObjectDeletionResult {
    Deleted,
    AlreadyAbsent,
    Blocked(DeletionBlock),
}

/// Adapter-observed facts; the Domain owns eligibility and authorization policy.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DeletionFacts {
    Snapshot {
        retained_for_restore: bool,
    },
    Revision {
        active_instance: bool,
        execution_pin: bool,
        service_reference: bool,
    },
    Run {
        running: bool,
        recovery_evidence: bool,
        retirement_evidence: bool,
        artifacts: bool,
        authorize_artifacts: bool,
    },
}

impl DeletionFacts {
    pub(crate) fn block(self) -> Option<DeletionBlock> {
        match self {
            Self::Snapshot {
                retained_for_restore: true,
            } => Some(DeletionBlock::SnapshotInUse),
            Self::Revision {
                active_instance: true,
                ..
            } => Some(DeletionBlock::ActiveInstance),
            Self::Revision {
                execution_pin: true,
                ..
            } => Some(DeletionBlock::RevisionPin),
            Self::Revision {
                service_reference: true,
                ..
            } => Some(DeletionBlock::ServiceReference),
            Self::Run { running: true, .. } => Some(DeletionBlock::RunningRun),
            Self::Run {
                retirement_evidence: true,
                ..
            } => Some(DeletionBlock::RetirementEvidence),
            Self::Run {
                recovery_evidence: true,
                ..
            } => Some(DeletionBlock::RecoveryEvidence),
            Self::Run {
                artifacts: true,
                authorize_artifacts: false,
                ..
            } => Some(DeletionBlock::ArtifactsRemain),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0472
    // Verifies: PR-REQ-0341
    #[test]
    fn artifact_authorization_never_overrides_lifetime_obligations() {
        for bits in 0..16 {
            for authorize_artifacts in [false, true] {
                let running = bits & 1 != 0;
                let recovery_evidence = bits & 2 != 0;
                let retirement_evidence = bits & 4 != 0;
                let artifacts = bits & 8 != 0;
                let blocked = DeletionFacts::Run {
                    running,
                    recovery_evidence,
                    retirement_evidence,
                    artifacts,
                    authorize_artifacts,
                }
                .block();
                assert_eq!(
                    blocked.is_some(),
                    running
                        || recovery_evidence
                        || retirement_evidence
                        || (artifacts && !authorize_artifacts)
                );
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CollectionReport {
    pub(crate) candidates: u64,
    pub(crate) removed: u64,
    pub(crate) retained: u64,
    pub(crate) unsupported: u64,
    pub(crate) failed: u64,
}
