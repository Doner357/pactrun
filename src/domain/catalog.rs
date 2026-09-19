//! Read-only catalog results, independent of storage and presentation.
use super::*;

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
}

#[derive(Clone, Debug)]
pub(crate) struct InstanceHistoryEntry {
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
