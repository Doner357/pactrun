//! Complete baseline Revision model. Identity writers use the owning canonical codec;
//! shared declarations do not carry a second version or select a legacy reader.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RevisionCore(pub(crate) ServiceRevision);
impl RevisionCore {
    pub(crate) fn version(&self) -> FormatVersion {
        VersionDomain::Revision.current()
    }
    pub(crate) fn common(&self) -> &RevisionDeclarations {
        self.0.common()
    }
    pub(crate) fn service_core(&self) -> Option<&ServiceRevision> {
        Some(&self.0)
    }
    pub(crate) fn service_hook(&self, site: &ServiceHookSite) -> HookServiceContractV2 {
        self.service_core()
            .and_then(|core| core.hooks().get(site))
            .cloned()
            .unwrap_or_default()
    }
    pub(crate) fn inputs(&self) -> &[InputDeclarationV1] {
        self.common().inputs()
    }
    pub(crate) fn actions(&self) -> &[ActionV1] {
        self.common().actions()
    }
    pub(crate) fn snapshot(&self) -> Option<&SnapshotCapabilityV1> {
        self.common().snapshot()
    }
    pub(crate) fn migrations(&self) -> &[MigrationV1] {
        self.common().migrations()
    }
    pub(crate) fn cleanup(&self) -> Option<&CleanupV1> {
        self.common().cleanup()
    }
    pub(crate) fn contains_presentation_target(&self, target: &PresentationTargetV1) -> bool {
        self.common().contains_presentation_target(target)
    }
    pub(crate) fn service_source(&self) -> ServiceSourceCore<'_> {
        ServiceSourceCore::Complete(&self.0)
    }
}
impl From<RevisionDeclarations> for RevisionCore {
    fn from(core: RevisionDeclarations) -> Self {
        Self(service_free_revision(core))
    }
}
impl From<ServiceRevision> for RevisionCore {
    fn from(core: ServiceRevision) -> Self {
        Self(core)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ValidatedRevisionContent {
    pub(crate) core: RevisionCore,
    pub(crate) runtime_content: RuntimeContentClosureIdentityV1,
}
impl From<DeclarationContent> for ValidatedRevisionContent {
    fn from(c: DeclarationContent) -> Self {
        Self {
            core: c.core.into(),
            runtime_content: c.runtime_content,
        }
    }
}
impl From<ServiceRevisionContent> for ValidatedRevisionContent {
    fn from(c: ServiceRevisionContent) -> Self {
        Self {
            core: c.core.into(),
            runtime_content: c.runtime_content,
        }
    }
}
