//! Versioned Revision model. Structural service views never select identity or
//! operation Input policy; callers use the exact owning contract.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RevisionCore {
    service: ServiceRevision,
    operation: Option<Box<OperationRevision>>,
}
impl RevisionCore {
    pub(crate) fn version(&self) -> FormatVersion {
        if self.operation.is_some() {
            OPERATION_CORE_FORMAT
                .parse()
                .expect("fixed operation Core version")
        } else {
            "1.0-alpha.1".parse().expect("fixed legacy Core version")
        }
    }
    pub(crate) fn operation(&self) -> Option<&OperationRevision> {
        self.operation.as_deref()
    }
    pub(crate) fn hook_protocol_matches(&self, version: FormatVersion) -> bool {
        version
            == if self.operation.is_some() {
                operation_hook_version()
            } else {
                FormatVersion::BASELINE
            }
    }
    pub(crate) fn parameters(&self, site: &ServiceHookSite) -> Option<&[ParameterV1]> {
        if let Some(core) = self.operation() {
            return core.parameters(site);
        }
        Some(match site {
            ServiceHookSite::Action(id) => &self.actions().iter().find(|a| &a.id == id)?.parameters,
            ServiceHookSite::Capture => &self.snapshot()?.capture.as_ref()?.parameters,
            ServiceHookSite::Restore => &self.snapshot()?.restore.as_ref()?.parameters,
            ServiceHookSite::Migration(digest) => {
                self.migrations()
                    .iter()
                    .find(|m| &m.source_revision_digest == digest)?;
                &[]
            }
            ServiceHookSite::Cleanup => {
                self.cleanup()?;
                &[]
            }
        })
    }
    pub(crate) fn ordinary_outputs(&self, site: &ServiceHookSite) -> Option<&[ManagedOutputV1]> {
        if let Some(core) = self.operation() {
            return core.ordinary_outputs(site);
        }
        self.parameters(site)?;
        Some(match site {
            ServiceHookSite::Action(id) => &self.actions().iter().find(|a| &a.id == id)?.outputs,
            _ => &[],
        })
    }
    pub(crate) fn common(&self) -> &RevisionDeclarations {
        self.service.common()
    }
    pub(crate) fn service_core(&self) -> Option<&ServiceRevision> {
        Some(&self.service)
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
    pub(crate) fn migration_declaration(&self, source: &Sha256Digest) -> Option<MigrationV1> {
        let mut declaration = self
            .migrations()
            .iter()
            .find(|edge| &edge.source_revision_digest == source)?
            .clone();
        if let Some(operation) = self.operation() {
            let OperationInputUse::Migration { source, target } =
                operation.input_use(&ServiceHookSite::Migration(source.clone()))?
            else {
                return None;
            };
            declaration.requires_source = source.required;
            declaration.requires_target = target.required;
        }
        Some(declaration)
    }
    pub(crate) fn cleanup(&self) -> Option<&CleanupV1> {
        self.common().cleanup()
    }
    pub(crate) fn contains_presentation_target(&self, target: &PresentationTargetV1) -> bool {
        self.common().contains_presentation_target(target)
    }
    pub(crate) fn service_source(&self) -> ServiceSourceCore<'_> {
        ServiceSourceCore::Complete(&self.service)
    }
}
impl From<RevisionDeclarations> for RevisionCore {
    fn from(core: RevisionDeclarations) -> Self {
        Self::from(service_free_revision(core))
    }
}
impl From<ServiceRevision> for RevisionCore {
    fn from(core: ServiceRevision) -> Self {
        Self {
            service: core,
            operation: None,
        }
    }
}
impl From<OperationRevision> for RevisionCore {
    fn from(core: OperationRevision) -> Self {
        Self {
            service: core.structural_service(),
            operation: Some(Box::new(core)),
        }
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
