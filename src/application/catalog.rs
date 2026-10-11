//! Typed catalog orchestration; no execution or reconciliation side effects.
use super::*;
use crate::domain::*;

impl PactrunApplication {
    pub(crate) fn identity_prefix(
        &self,
        kind: CatalogIdentityKind,
        prefix: &str,
    ) -> Result<IdentityMatches, ApplicationError> {
        Ok(self.persistence.identity_prefix(kind, prefix)?)
    }
    pub(crate) fn revision_prefix(
        &self,
        package: &str,
        digest: &str,
    ) -> Result<IdentityMatches, ApplicationError> {
        Ok(self.persistence.revision_prefix(package, digest)?)
    }

    pub(crate) fn inspect_retirements(
        &self,
        limit: usize,
        after: Option<InstanceId>,
    ) -> Result<CatalogPage<RetirementCatalogEntry, InstanceId>, ApplicationError> {
        Ok(self.persistence.catalog_retirements(limit, after)?)
    }
    pub(crate) fn inspect_retirement(
        &self,
        id: InstanceId,
    ) -> Result<RetirementCatalogEntry, ApplicationError> {
        Ok(self.persistence.catalog_retirement(id)?)
    }
    pub(crate) fn inspect_run_complete(&self, id: RunId) -> Result<RunCatalog, ApplicationError> {
        Ok(self.persistence.catalog_run_complete(id)?)
    }
    pub(crate) fn inspect_snapshots_complete(
        &self,
        name: Option<&InstanceName>,
        id: Option<SnapshotId>,
    ) -> Result<crate::persistence::SnapshotCatalog, ApplicationError> {
        Ok(self.persistence.catalog_snapshots_complete(name, id)?)
    }
    pub(crate) fn inspect_instances_complete(&self) -> Result<InstanceCatalog, ApplicationError> {
        Ok(self.persistence.catalog_instances_complete()?)
    }
    pub(crate) fn inspect_runs_complete(
        &self,
        selector: &CatalogRunSelector,
        limit: usize,
        after: Option<RunId>,
    ) -> Result<RunCatalog, ApplicationError> {
        Ok(self
            .persistence
            .catalog_runs_complete(selector, limit, after, true)?)
    }
    pub(crate) fn inspect_instance_definition(
        &self,
        name: &InstanceName,
    ) -> Result<(InstanceView, RevisionCatalogEntry), ApplicationError> {
        self.persistence
            .catalog_instance_definition(name)?
            .ok_or_else(|| ActionResolutionError::InstanceNotFound.into())
    }
    pub(crate) fn inspect_revision_definitions(
        &self,
        ids: &[RevisionIdentity],
    ) -> Result<Vec<RevisionCatalogEntry>, ApplicationError> {
        Ok(self.persistence.catalog_revision_set(ids)?)
    }
    pub(crate) fn catalog_resolve_revision(
        &self,
        selector: &CatalogRevisionSelector,
    ) -> Result<RevisionCatalogEntry, ApplicationError> {
        Ok(self.persistence.catalog_resolve_revision(selector)?)
    }
    pub(crate) fn catalog_revisions(
        &self,
        limit: usize,
        after: Option<&RevisionCursor>,
    ) -> Result<CatalogPage<RevisionCatalogEntry, RevisionCursor>, ApplicationError> {
        Ok(self.persistence.catalog_revisions(limit, after)?)
    }
    pub(crate) fn catalog_history(
        &self,
        limit: usize,
        after: Option<InstanceId>,
        deletions: bool,
    ) -> Result<CatalogPage<InstanceHistoryEntry, InstanceId>, ApplicationError> {
        Ok(self.persistence.catalog_history(limit, after, deletions)?)
    }
    pub(crate) fn catalog_instance(
        &self,
        id: InstanceId,
    ) -> Result<InstanceHistoryEntry, ApplicationError> {
        Ok(self.persistence.catalog_instance(id)?)
    }

    pub(crate) fn mutate_local_metadata(
        &self,
        id: &RevisionIdentity,
        operation: RevisionMetadataMutation,
    ) -> Result<(), ApplicationError> {
        let batch = RevisionMetadataMutationBatch::new([operation])
            .map_err(|e| ApplicationError::InvalidRequest(e.to_string()))?;
        Ok(self.persistence.apply_revision_metadata_batch(id, &batch)?)
    }
}
