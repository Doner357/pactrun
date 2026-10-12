//! Operation-scoped identity and capability model.
use super::*;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
#[path = "operation_contract_tests.rs"]
mod tests;

pub(crate) const OPERATION_SOURCE_FORMAT: &str = "1.0-alpha.3";
pub(crate) const OPERATION_CORE_FORMAT: &str = "1.0-alpha.2";
pub(crate) const OPERATION_HOOK_FORMAT: &str = "1.0-alpha.2";

pub(crate) fn operation_hook_version() -> FormatVersion {
    OPERATION_HOOK_FORMAT.parse().expect("fixed Hook version")
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub(crate) struct InputUse<T> {
    pub(crate) required: Vec<T>,
    pub(crate) optional: Vec<T>,
}
impl<T: Ord + Clone> InputUse<T> {
    pub(crate) fn all(&self) -> Vec<T> {
        let mut all = self.required.clone();
        all.extend(self.optional.iter().cloned());
        all.sort();
        all
    }
    fn normalize(&mut self) -> Result<(), ServiceContractError> {
        self.required.sort();
        self.optional.sort();
        let all = self.all();
        if all.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(invalid(
                "duplicate_semantic_key",
                "duplicate or overlapping Input grants",
            ));
        }
        Ok(())
    }
}
pub(crate) type ActiveInputUse = InputUse<InputIdentity>;
pub(crate) type BindingInputUse = InputUse<InputBindingRefV1>;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OperationInputUse {
    Active(ActiveInputUse),
    Binding(BindingInputUse),
    Migration {
        source: BindingInputUse,
        target: ActiveInputUse,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct OperationInput {
    pub(crate) id: InputIdentity,
    pub(crate) protection: InputProtectionV1,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct InputOperation<T> {
    #[serde(flatten)]
    pub(crate) definition: T,
    pub(crate) inputs: ActiveInputUse,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct SnapshotHookOperation<T> {
    #[serde(flatten)]
    pub(crate) operation: InputOperation<T>,
    pub(crate) outputs: Vec<ManagedOutputV1>,
}

/// Additional common capabilities not represented by the legacy shape validator.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(crate) struct OperationCapabilities {
    pub(crate) parameters: Vec<ParameterV1>,
    pub(crate) outputs: Vec<ManagedOutputV1>,
}
impl OperationCapabilities {
    fn normalize(&mut self) -> Result<(), ServiceContractError> {
        super::revision_declarations::normalize_parameters(&mut self.parameters)?;
        self.outputs.sort_by(|a, b| a.id.cmp(&b.id));
        if self.outputs.windows(2).any(|p| p[0].id == p[1].id) {
            return Err(invalid("duplicate_semantic_key", "duplicate ManagedOutput"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct OperationSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) capture: Option<SnapshotHookOperation<CaptureV1>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) restore: Option<SnapshotHookOperation<RestoreV1>>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct OperationMigration {
    #[serde(flatten)]
    pub(crate) capabilities: OperationCapabilities,
    pub(crate) source_revision_digest: Sha256Digest,
    pub(crate) transitions: Vec<MigrationTransitionV1>,
    pub(crate) source_inputs: BindingInputUse,
    pub(crate) target_inputs: ActiveInputUse,
    pub(crate) produces_target: Vec<InputIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) hook: Option<HookV1>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct OperationCleanup {
    #[serde(flatten)]
    pub(crate) capabilities: OperationCapabilities,
    pub(crate) inputs: BindingInputUse,
    pub(crate) hook: HookV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct OperationRevision {
    format_version: &'static str,
    inputs: Vec<OperationInput>,
    actions: Vec<InputOperation<ActionV1>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot: Option<OperationSnapshot>,
    migrations: Vec<OperationMigration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cleanup: Option<OperationCleanup>,
    service_storages: Vec<ServiceStorageV2>,
    service_resources: Vec<ServiceResourceV2>,
    #[serde(skip)]
    service_hooks: BTreeMap<ServiceHookSite, HookServiceContractV2>,
    #[serde(skip)]
    service_migrations: BTreeMap<Sha256Digest, ServiceMigrationV2>,
}

fn invalid(code: &str, message: &str) -> ServiceContractError {
    ServiceContractError::new(code, message)
}
fn take_capabilities(
    capabilities: &mut BTreeMap<ServiceHookSite, OperationCapabilities>,
    site: ServiceHookSite,
) -> Result<OperationCapabilities, ServiceContractError> {
    let extra = capabilities
        .remove(&site)
        .ok_or_else(|| invalid("missing_field", "missing operation capabilities"))?;
    if matches!(site, ServiceHookSite::Capture | ServiceHookSite::Restore)
        && !extra.parameters.is_empty()
    {
        return Err(invalid(
            "invalid_reference",
            "Snapshot parameters belong to its operation declaration",
        ));
    }
    Ok(extra)
}
fn active(
    uses: &mut BTreeMap<ServiceHookSite, OperationInputUse>,
    site: ServiceHookSite,
    ids: &BTreeSet<InputIdentity>,
) -> Result<ActiveInputUse, ServiceContractError> {
    let Some(OperationInputUse::Active(mut inputs)) = uses.remove(&site) else {
        return Err(invalid(
            "invalid_reference",
            "missing or wrong operation Input context",
        ));
    };
    inputs.normalize()?;
    if inputs.all().iter().any(|id| !ids.contains(id)) {
        return Err(invalid(
            "invalid_reference",
            "undeclared active Input grant",
        ));
    }
    Ok(inputs)
}

/// The shared shape is a transient structural validator, never an identity or
/// runtime policy for this format. Its requirement unions also validate optional
/// reference roles, writer conflicts and transition consistency.
pub(crate) fn project_operation_revision(
    shape: ServiceRevision,
    mut uses: BTreeMap<ServiceHookSite, OperationInputUse>,
    mut capabilities: BTreeMap<ServiceHookSite, OperationCapabilities>,
) -> Result<OperationRevision, ServiceContractError> {
    for extra in capabilities.values_mut() {
        extra.normalize()?;
    }
    let common = shape.common();
    if common.inputs().iter().any(|input| input.required) {
        return Err(invalid(
            "unknown_field",
            "operation Inputs have no global required flag",
        ));
    }
    let ids = common.inputs().iter().map(|i| i.id.clone()).collect();
    let mut actions = Vec::new();
    for action in common.actions() {
        actions.push(InputOperation {
            definition: action.clone(),
            inputs: active(&mut uses, ServiceHookSite::Action(action.id.clone()), &ids)?,
        });
    }
    let snapshot = common
        .snapshot()
        .map(|s| -> Result<_, ServiceContractError> {
            Ok(OperationSnapshot {
                capture: s
                    .capture
                    .as_ref()
                    .map(|definition| -> Result<_, ServiceContractError> {
                        Ok(SnapshotHookOperation {
                            operation: InputOperation {
                                definition: definition.clone(),
                                inputs: active(&mut uses, ServiceHookSite::Capture, &ids)?,
                            },
                            outputs: take_capabilities(
                                &mut capabilities,
                                ServiceHookSite::Capture,
                            )?
                            .outputs,
                        })
                    })
                    .transpose()?,
                restore: s
                    .restore
                    .as_ref()
                    .map(|definition| -> Result<_, ServiceContractError> {
                        Ok(SnapshotHookOperation {
                            operation: InputOperation {
                                definition: definition.clone(),
                                inputs: active(&mut uses, ServiceHookSite::Restore, &ids)?,
                            },
                            outputs: take_capabilities(
                                &mut capabilities,
                                ServiceHookSite::Restore,
                            )?
                            .outputs,
                        })
                    })
                    .transpose()?,
            })
        })
        .transpose()?;
    let mut migrations = Vec::new();
    for migration in common.migrations() {
        let Some(OperationInputUse::Migration {
            mut source,
            mut target,
        }) = uses.remove(&ServiceHookSite::Migration(
            migration.source_revision_digest.clone(),
        ))
        else {
            return Err(invalid(
                "invalid_reference",
                "missing Migration Input contexts",
            ));
        };
        source.normalize()?;
        target.normalize()?;
        if source.all() != migration.requires_source || target.all() != migration.requires_target {
            return Err(invalid(
                "invalid_reference",
                "Migration grants do not match validated references",
            ));
        }
        let extra = take_capabilities(
            &mut capabilities,
            ServiceHookSite::Migration(migration.source_revision_digest.clone()),
        )?;
        if migration.hook.is_none() && (!extra.parameters.is_empty() || !extra.outputs.is_empty()) {
            return Err(invalid(
                "invalid_reference",
                "Hook-free Migration cannot declare parameters or ordinary outputs",
            ));
        }
        migrations.push(OperationMigration {
            capabilities: extra,
            source_revision_digest: migration.source_revision_digest.clone(),
            transitions: migration.transitions.clone(),
            source_inputs: source,
            target_inputs: target,
            produces_target: migration.produces_target.clone(),
            hook: migration.hook.clone(),
        });
    }
    let cleanup = common
        .cleanup()
        .map(|cleanup| -> Result<_, ServiceContractError> {
            let Some(OperationInputUse::Binding(mut inputs)) =
                uses.remove(&ServiceHookSite::Cleanup)
            else {
                return Err(invalid(
                    "invalid_reference",
                    "missing Cleanup Input context",
                ));
            };
            inputs.normalize()?;
            if inputs.all() != cleanup.requires {
                return Err(invalid(
                    "invalid_reference",
                    "Cleanup grants do not match validated references",
                ));
            }
            Ok(OperationCleanup {
                capabilities: take_capabilities(&mut capabilities, ServiceHookSite::Cleanup)?,
                inputs,
                hook: cleanup.hook.clone(),
            })
        })
        .transpose()?;
    if !uses.is_empty() || !capabilities.is_empty() {
        return Err(invalid(
            "invalid_reference",
            "Input grants have no operation owner",
        ));
    }
    let result = OperationRevision {
        format_version: OPERATION_CORE_FORMAT,
        inputs: common
            .inputs()
            .iter()
            .map(|i| OperationInput {
                id: i.id.clone(),
                protection: i.protection,
            })
            .collect(),
        actions,
        snapshot,
        migrations,
        cleanup,
        service_storages: shape.storages().to_vec(),
        service_resources: shape.resources().to_vec(),
        service_hooks: shape.hooks().clone(),
        service_migrations: shape.migrations().clone(),
    };
    if result
        .hooks()
        .iter()
        .any(|h| h.protocol_version != operation_hook_version())
    {
        return Err(invalid(
            "unsupported_protocol_version",
            "operation Core requires Hook 1.0-alpha.2",
        ));
    }
    Ok(result)
}

impl OperationRevision {
    pub(crate) fn inputs(&self) -> &[OperationInput] {
        &self.inputs
    }
    pub(crate) fn actions(&self) -> &[InputOperation<ActionV1>] {
        &self.actions
    }
    pub(crate) fn snapshot(&self) -> Option<&OperationSnapshot> {
        self.snapshot.as_ref()
    }
    pub(crate) fn migrations(&self) -> &[OperationMigration] {
        &self.migrations
    }
    pub(crate) fn cleanup(&self) -> Option<&OperationCleanup> {
        self.cleanup.as_ref()
    }
    pub(crate) fn service_hooks(&self) -> &BTreeMap<ServiceHookSite, HookServiceContractV2> {
        &self.service_hooks
    }
    pub(crate) fn service_migrations(&self) -> &BTreeMap<Sha256Digest, ServiceMigrationV2> {
        &self.service_migrations
    }

    pub(crate) fn input_use(&self, site: &ServiceHookSite) -> Option<OperationInputUse> {
        Some(match site {
            ServiceHookSite::Action(id) => OperationInputUse::Active(
                self.actions
                    .iter()
                    .find(|a| &a.definition.id == id)?
                    .inputs
                    .clone(),
            ),
            ServiceHookSite::Capture => OperationInputUse::Active(
                self.snapshot
                    .as_ref()?
                    .capture
                    .as_ref()?
                    .operation
                    .inputs
                    .clone(),
            ),
            ServiceHookSite::Restore => OperationInputUse::Active(
                self.snapshot
                    .as_ref()?
                    .restore
                    .as_ref()?
                    .operation
                    .inputs
                    .clone(),
            ),
            ServiceHookSite::Migration(digest) => {
                let migration = self
                    .migrations
                    .iter()
                    .find(|m| &m.source_revision_digest == digest)?;
                OperationInputUse::Migration {
                    source: migration.source_inputs.clone(),
                    target: migration.target_inputs.clone(),
                }
            }
            ServiceHookSite::Cleanup => {
                OperationInputUse::Binding(self.cleanup.as_ref()?.inputs.clone())
            }
        })
    }

    pub(crate) fn ordinary_outputs(&self, site: &ServiceHookSite) -> Option<&[ManagedOutputV1]> {
        Some(match site {
            ServiceHookSite::Action(id) => {
                &self
                    .actions
                    .iter()
                    .find(|a| &a.definition.id == id)?
                    .definition
                    .outputs
            }
            ServiceHookSite::Capture => &self.snapshot.as_ref()?.capture.as_ref()?.outputs,
            ServiceHookSite::Restore => &self.snapshot.as_ref()?.restore.as_ref()?.outputs,
            ServiceHookSite::Migration(digest) => {
                &self
                    .migrations
                    .iter()
                    .find(|m| &m.source_revision_digest == digest)?
                    .capabilities
                    .outputs
            }
            ServiceHookSite::Cleanup => &self.cleanup.as_ref()?.capabilities.outputs,
        })
    }

    pub(crate) fn parameters(&self, site: &ServiceHookSite) -> Option<&[ParameterV1]> {
        Some(match site {
            ServiceHookSite::Action(id) => {
                &self
                    .actions
                    .iter()
                    .find(|a| &a.definition.id == id)?
                    .definition
                    .parameters
            }
            ServiceHookSite::Capture => {
                &self
                    .snapshot
                    .as_ref()?
                    .capture
                    .as_ref()?
                    .operation
                    .definition
                    .parameters
            }
            ServiceHookSite::Restore => {
                &self
                    .snapshot
                    .as_ref()?
                    .restore
                    .as_ref()?
                    .operation
                    .definition
                    .parameters
            }
            ServiceHookSite::Migration(digest) => {
                &self
                    .migrations
                    .iter()
                    .find(|m| &m.source_revision_digest == digest)?
                    .capabilities
                    .parameters
            }
            ServiceHookSite::Cleanup => &self.cleanup.as_ref()?.capabilities.parameters,
        })
    }

    fn hooks(&self) -> Vec<&HookV1> {
        let mut hooks: Vec<_> = self.actions.iter().map(|a| &a.definition.hook).collect();
        if let Some(snapshot) = &self.snapshot {
            if let Some(capture) = &snapshot.capture {
                hooks.push(&capture.operation.definition.hook);
            }
            if let Some(restore) = &snapshot.restore {
                hooks.push(&restore.operation.definition.hook);
            }
        }
        hooks.extend(self.migrations.iter().filter_map(|m| m.hook.as_ref()));
        if let Some(cleanup) = &self.cleanup {
            hooks.push(&cleanup.hook);
        }
        hooks
    }

    // Structural reference view only. Required/optional read policy and common
    // capabilities remain owned by this operation contract, not this view.
    fn validation_declarations(&self) -> RevisionDeclarations {
        project_revision_declarations(RevisionDeclarationInput {
            inputs: self
                .inputs
                .iter()
                .map(|i| InputDeclarationV1 {
                    id: i.id.clone(),
                    required: false,
                    protection: i.protection,
                })
                .collect(),
            actions: self.actions.iter().map(|a| a.definition.clone()).collect(),
            snapshot: self.snapshot.as_ref().map(|s| SnapshotCapabilityV1 {
                capture: s.capture.as_ref().map(|c| c.operation.definition.clone()),
                restore: s.restore.as_ref().map(|r| r.operation.definition.clone()),
            }),
            migrations: self
                .migrations
                .iter()
                .map(|m| MigrationV1 {
                    source_revision_digest: m.source_revision_digest.clone(),
                    transitions: m.transitions.clone(),
                    requires_source: m.source_inputs.all(),
                    requires_target: m.target_inputs.all(),
                    produces_target: m.produces_target.clone(),
                    hook: m.hook.clone(),
                })
                .collect(),
            cleanup: self.cleanup.as_ref().map(|c| CleanupV1 {
                requires: c.inputs.all(),
                hook: c.hook.clone(),
            }),
        })
        .expect("validated operation references retain their structural invariants")
    }
    pub(super) fn structural_service(&self) -> ServiceRevision {
        project_service_revision_for_protocol(
            self.validation_declarations(),
            self.service_storages.clone(),
            self.service_resources.clone(),
            self.service_hooks.clone(),
            self.service_migrations.clone(),
            operation_hook_version(),
        )
        .expect("validated operation service references retain their invariants")
    }
    pub(crate) fn contains_presentation_target(&self, target: &PresentationTargetV1) -> bool {
        self.validation_declarations()
            .contains_presentation_target(target)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OperationRevisionContent {
    core: OperationRevision,
    runtime_content: RuntimeContentClosureIdentityV1,
}
impl OperationRevisionContent {
    pub(crate) fn core(&self) -> &OperationRevision {
        &self.core
    }
    pub(crate) fn runtime_content(&self) -> &RuntimeContentClosureIdentityV1 {
        &self.runtime_content
    }
}
pub(crate) fn validate_operation_content(
    core: OperationRevision,
    runtime_content: RuntimeContentClosureIdentityV1,
) -> Result<OperationRevisionContent, ServiceContractError> {
    validate_declaration_content(core.validation_declarations(), runtime_content.clone())?;
    Ok(OperationRevisionContent {
        core,
        runtime_content,
    })
}

#[derive(Clone, Copy)]
pub(crate) enum OperationSource<'a> {
    Legacy(&'a ServiceRevision),
    Operation(&'a OperationRevision),
}
pub(crate) fn validate_operation_sources(
    target: &OperationRevision,
    sources: &BTreeMap<Sha256Digest, OperationSource<'_>>,
) -> Result<RelationalValidationV1, ServiceContractError> {
    let owned: BTreeMap<_, _> = sources
        .iter()
        .map(|(digest, source)| {
            (
                digest.clone(),
                match source {
                    OperationSource::Legacy(source) => (*source).clone(),
                    OperationSource::Operation(source) => source.structural_service(),
                },
            )
        })
        .collect();
    let views = owned
        .iter()
        .map(|(digest, source)| (digest.clone(), ServiceSourceCore::Complete(source)))
        .collect();
    validate_service_sources_v2(&target.structural_service(), &views)
}
