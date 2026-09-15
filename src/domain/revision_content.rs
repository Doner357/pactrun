//! Version-neutral typed content. There is deliberately no Deref to V1 and no
//! serialization impl: identity writers must select the exact version codec.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RevisionCore {
    V1(Box<RevisionCoreV1>),
    V2(Box<RevisionCoreV2>),
}
impl RevisionCore {
    pub(crate) fn version(&self) -> u8 {
        match self {
            Self::V1(_) => 1,
            Self::V2(_) => 2,
        }
    }
    /// Shared declaration semantics only, never a canonical identity projection.
    pub(crate) fn common(&self) -> &RevisionCoreV1 {
        match self {
            Self::V1(c) => c,
            Self::V2(c) => c.common(),
        }
    }
    pub(crate) fn service_core(&self) -> Option<&RevisionCoreV2> {
        match self {
            Self::V1(_) => None,
            Self::V2(c) => Some(c),
        }
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
        match self {
            Self::V1(c) => ServiceSourceCore::V1(c),
            Self::V2(c) => ServiceSourceCore::V2(c),
        }
    }
}
impl From<RevisionCoreV1> for RevisionCore {
    fn from(c: RevisionCoreV1) -> Self {
        Self::V1(Box::new(c))
    }
}
impl From<RevisionCoreV2> for RevisionCore {
    fn from(c: RevisionCoreV2) -> Self {
        Self::V2(Box::new(c))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ValidatedRevisionContent {
    pub(crate) core: RevisionCore,
    pub(crate) runtime_content: RuntimeContentClosureIdentityV1,
}
impl From<ValidatedRevisionContentV1> for ValidatedRevisionContent {
    fn from(c: ValidatedRevisionContentV1) -> Self {
        Self {
            core: c.core.into(),
            runtime_content: c.runtime_content,
        }
    }
}
impl From<ValidatedRevisionContentV2> for ValidatedRevisionContent {
    fn from(c: ValidatedRevisionContentV2) -> Self {
        Self {
            core: c.core.into(),
            runtime_content: c.runtime_content,
        }
    }
}
