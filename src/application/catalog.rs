//! Typed catalog orchestration; no execution or reconciliation side effects.
use super::*;
use crate::domain::*;

impl PactrunApplication {
    pub(crate) fn catalog_alias(
        &self,
        alias: &LocalAlias,
    ) -> Result<Option<RevisionIdentity>, ApplicationError> {
        Ok(self.persistence.catalog_alias(alias)?)
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
        after: Option<&RevisionIdentity>,
    ) -> Result<CatalogPage<RevisionCatalogEntry, RevisionIdentity>, ApplicationError> {
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
    pub(crate) fn catalog_runs(
        &self,
        selector: &CatalogRunSelector,
        limit: usize,
        after: Option<RunId>,
    ) -> Result<CatalogPage<ManagedRunView, RunId>, ApplicationError> {
        Ok(self.persistence.catalog_runs(selector, limit, after)?)
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
