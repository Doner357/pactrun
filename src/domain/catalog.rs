//! Read-only catalog results, independent of storage and presentation.
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CatalogIdentityKind {
    Run,
    Snapshot,
    Instance,
    Allocation,
}

pub(crate) struct IdentityMatches {
    pub(crate) candidates: Vec<String>,
    pub(crate) has_more: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CatalogPage<T, K> {
    pub(crate) items: Vec<T>,
    pub(crate) next: Option<K>,
}

#[derive(Clone, Debug)]
pub(crate) struct RevisionCatalogEntry {
    pub(crate) identity: RevisionIdentity,
    pub(crate) core: RevisionCore,
    pub(crate) metadata: RevisionMetadataView,
    pub(crate) local: LocalRevisionFacts,
    pub(crate) reference: String,
}

pub(crate) struct InstanceCatalog {
    pub(crate) items: Vec<InstanceView>,
    pub(crate) revisions: Vec<RevisionCatalogEntry>,
}
pub(crate) struct RunCatalog {
    pub(crate) selectors: std::collections::BTreeMap<RunId, (usize, usize)>,
    pub(crate) page: CatalogPage<ManagedRunView, RunId>,
    pub(crate) inspections: Vec<ManagedRunInspectionData>,
    pub(crate) revisions: Vec<RevisionCatalogEntry>,
    pub(crate) unavailable_revisions: Vec<RevisionIdentity>,
}

pub(crate) struct RetirementCatalogEntry {
    pub(crate) history: InstanceHistoryEntry,
    pub(crate) live: Option<InstanceView>,
    pub(crate) obligation: Option<DeletionObligation>,
    pub(crate) runs: Vec<ManagedRunView>,
}

#[derive(Clone, Debug)]
pub(crate) struct InstanceHistoryEntry {
    pub(crate) unique_prefix_length: usize,
    pub(crate) id: InstanceId,
    pub(crate) recorded_name: InstanceName,
    pub(crate) current_name: Option<InstanceName>,
    pub(crate) deletion_phase: Option<DeletionPhase>,
    pub(crate) retired: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum CatalogRunSelector {
    All,
    Name(InstanceName),
    Identity(InstanceId),
}

#[derive(Clone, Debug)]
pub(crate) enum CatalogRevisionSelector {
    Exact(RevisionIdentity),
    Label(ReferenceLabel),
    Alias(LocalAlias),
}
