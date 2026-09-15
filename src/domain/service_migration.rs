//! Service association transitions, not filesystem transformations. Allocation
//! symbols make whole-path preflight possible without allocating future roots.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum ServiceAllocationOrigin {
    Existing(ServiceAllocationId),
    Created {
        edge: usize,
        storage: ServiceStorageIdentity,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlannedStorageAssociation {
    pub(crate) declaration_revision: RevisionIdentity,
    pub(crate) allocation: ServiceAllocationOrigin,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlannedResourceAssociation {
    pub(crate) declaration_revision: RevisionIdentity,
    pub(crate) declaration: ServiceResourceV2,
    pub(crate) allocation: ServiceAllocationOrigin,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ServiceMigrationState {
    pub(crate) revision: RevisionIdentity,
    pub(crate) storages: BTreeMap<ServiceStorageIdentity, PlannedStorageAssociation>,
    pub(crate) resources: BTreeMap<ServiceResourceIdentity, PlannedResourceAssociation>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PlannedServiceObject {
    Storage { allocation: ServiceAllocationOrigin },
    Resource(PlannedResourceAssociation),
}
impl PlannedServiceObject {
    pub(crate) fn allocation(&self) -> &ServiceAllocationOrigin {
        match self {
            Self::Storage { allocation } => allocation,
            Self::Resource(r) => &r.allocation,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ServiceMigrationEdge {
    pub(crate) before: ServiceMigrationState,
    pub(crate) after: ServiceMigrationState,
    pub(crate) consumed_storages: BTreeSet<ServiceStorageIdentity>,
    pub(crate) consumed_resources: BTreeSet<ServiceResourceIdentity>,
    pub(crate) created_storages: BTreeSet<ServiceStorageIdentity>,
    pub(crate) create_presence: Vec<(ServiceCreatePresence, PlannedResourceAssociation)>,
    pub(crate) grants: Vec<(ServiceAccessV2, PlannedServiceObject)>,
    pub(crate) requires: Vec<(ServicePrerequisiteV2, PlannedServiceObject)>,
    pub(crate) transform: bool,
}
impl ServiceMigrationEdge {
    pub(crate) fn hook_bindings(&self) -> Result<ServiceHookBindings> {
        fn object(value: &PlannedServiceObject) -> Result<BoundServiceObject> {
            let ServiceAllocationOrigin::Existing(allocation) = value.allocation() else {
                return Err(conflict("target allocation is not prepared"));
            };
            Ok(match value {
                PlannedServiceObject::Storage { .. } => BoundServiceObject::Storage {
                    allocation: *allocation,
                },
                PlannedServiceObject::Resource(r) => BoundServiceObject::Resource {
                    allocation: *allocation,
                    declaration: r.declaration.clone(),
                },
            })
        }
        let mut bindings = ServiceHookBindings {
            grants: self
                .grants
                .iter()
                .map(|(a, o)| Ok((a.clone(), object(o)?)))
                .collect::<Result<_>>()?,
            requires: self
                .requires
                .iter()
                .map(|(r, o)| Ok((r.clone(), object(o)?)))
                .collect::<Result<_>>()?,
        };
        for (presence, resource) in &self.create_presence {
            let presence = match presence {
                ServiceCreatePresence::Any => continue,
                ServiceCreatePresence::Present => ServicePresenceRequirement::Present,
                ServiceCreatePresence::Absent => ServicePresenceRequirement::Absent,
            };
            let requirement = ServicePrerequisiteV2 {
                reference: ServiceReferenceV2 {
                    view: ServiceView::Target,
                    role: ServiceRole::Active,
                    scope: ServiceScope::Resource(resource.declaration.id.clone()),
                },
                presence,
            };
            if !bindings.requires.iter().any(|(r, _)| r == &requirement) {
                bindings.requires.push((
                    requirement,
                    object(&PlannedServiceObject::Resource(resource.clone()))?,
                ));
            }
        }
        Ok(bindings)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ServiceMigrationError(pub(crate) &'static str);
impl std::fmt::Display for ServiceMigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "service_storage.mapping_conflict: {}", self.0)
    }
}
impl std::error::Error for ServiceMigrationError {}
type Result<T> = std::result::Result<T, ServiceMigrationError>;
fn conflict(message: &'static str) -> ServiceMigrationError {
    ServiceMigrationError(message)
}
fn role(declaration: &RevisionIdentity, current: &RevisionIdentity) -> ServiceRole {
    if declaration == current {
        ServiceRole::Active
    } else {
        ServiceRole::Retained
    }
}

impl ServiceMigrationState {
    pub(crate) fn from_observed(observed: &InstanceServiceState) -> Result<Self> {
        let mut state = Self {
            revision: observed.current_revision.clone(),
            storages: BTreeMap::new(),
            resources: BTreeMap::new(),
        };
        for s in &observed.storages {
            if s.declaration_revision.package_id != state.revision.package_id
                || role(&s.declaration_revision, &state.revision) != s.role
                || state
                    .storages
                    .insert(
                        s.declaration.id.clone(),
                        PlannedStorageAssociation {
                            declaration_revision: s.declaration_revision.clone(),
                            allocation: ServiceAllocationOrigin::Existing(s.allocation),
                        },
                    )
                    .is_some()
            {
                return Err(conflict("inconsistent storage association observation"));
            }
        }
        for r in &observed.resources {
            if r.declaration_revision.package_id != state.revision.package_id
                || role(&r.declaration_revision, &state.revision) != r.role
                || state
                    .resources
                    .insert(
                        r.declaration.id.clone(),
                        PlannedResourceAssociation {
                            declaration_revision: r.declaration_revision.clone(),
                            declaration: r.declaration.clone(),
                            allocation: ServiceAllocationOrigin::Existing(r.allocation),
                        },
                    )
                    .is_some()
            {
                return Err(conflict("inconsistent resource association observation"));
            }
        }
        Ok(state)
    }
    fn storage(&self, source: &StorageSourceV2) -> Result<&PlannedStorageAssociation> {
        self.storages
            .get(&source.storage_id)
            .filter(|s| role(&s.declaration_revision, &self.revision) == source.role)
            .ok_or_else(|| conflict("source storage is unavailable in the declared role"))
    }
    fn resource(&self, source: &ResourceSourceV2) -> Result<&PlannedResourceAssociation> {
        self.resources
            .get(&source.resource_id)
            .filter(|r| role(&r.declaration_revision, &self.revision) == source.role)
            .ok_or_else(|| conflict("source resource is unavailable in the declared role"))
    }
    fn object(&self, reference: &ServiceReferenceV2) -> Result<PlannedServiceObject> {
        match &reference.scope {
            ServiceScope::Storage(id) => Ok(PlannedServiceObject::Storage {
                allocation: self
                    .storage(&StorageSourceV2 {
                        role: reference.role,
                        storage_id: id.clone(),
                    })?
                    .allocation
                    .clone(),
            }),
            ServiceScope::Resource(id) => Ok(PlannedServiceObject::Resource(
                self.resource(&ResourceSourceV2 {
                    role: reference.role,
                    resource_id: id.clone(),
                })?
                .clone(),
            )),
        }
    }
}

fn validate_current(state: &ServiceMigrationState, core: &RevisionCore) -> Result<()> {
    let Some(core) = core.service_core() else {
        if state
            .storages
            .values()
            .any(|s| s.declaration_revision == state.revision)
            || state
                .resources
                .values()
                .any(|r| r.declaration_revision == state.revision)
        {
            return Err(conflict(
                "V1 source cannot have active service associations",
            ));
        }
        return Ok(());
    };
    let mut allocations = BTreeSet::new();
    for storage in core.storages() {
        let s = state.storage(&StorageSourceV2 {
            role: ServiceRole::Active,
            storage_id: storage.id.clone(),
        })?;
        if !allocations.insert(&s.allocation) {
            return Err(conflict("active storages alias one allocation"));
        }
    }
    for resource in core.resources() {
        let r = state.resource(&ResourceSourceV2 {
            role: ServiceRole::Active,
            resource_id: resource.id.clone(),
        })?;
        if &r.declaration != resource
            || r.allocation != state.storages[&resource.storage_id].allocation
        {
            return Err(conflict(
                "active resource association differs from its Core",
            ));
        }
    }
    if state
        .storages
        .iter()
        .any(|(id, s)| s.declaration_revision == state.revision && !core.has_storage(id))
        || state
            .resources
            .iter()
            .any(|(id, r)| r.declaration_revision == state.revision && core.resource(id).is_none())
    {
        return Err(conflict("undeclared active service association"));
    }
    Ok(())
}

pub(crate) fn evaluate_service_migration_edge(
    before: &ServiceMigrationState,
    source: &RevisionCore,
    target_revision: &RevisionIdentity,
    target: &RevisionCore,
    edge: usize,
) -> Result<ServiceMigrationEdge> {
    if before.revision.package_id != target_revision.package_id {
        return Err(conflict("service mappings cannot cross Package lineages"));
    }
    validate_current(before, source)?;
    let digest = Sha256Digest::from_bytes(*before.revision.content_digest.as_bytes());
    let empty = ServiceMigrationV2::default();
    let mapping = match target.service_core() {
        Some(core) => {
            validate_service_sources_v2(
                core,
                &BTreeMap::from([(digest.clone(), source.service_source())]),
            )
            .map_err(|_| conflict("invalid exact-source service semantics"))?;
            core.migrations()
                .get(&digest)
                .ok_or_else(|| conflict("target has no service mapping for this edge"))?
        }
        None => &empty,
    };
    let mut result = ServiceMigrationEdge {
        before: before.clone(),
        after: before.clone(),
        consumed_storages: BTreeSet::new(),
        consumed_resources: BTreeSet::new(),
        created_storages: BTreeSet::new(),
        create_presence: vec![],
        grants: vec![],
        requires: vec![],
        transform: false,
    };
    result.after.revision = target_revision.clone();
    let mut storages = BTreeMap::new();
    let mut active_allocations = BTreeSet::new();
    for transition in &mapping.storages {
        let id = transition.target();
        if before.storages.contains_key(id)
            && transition.source().is_none_or(|s| &s.storage_id != id)
        {
            return Err(conflict(
                "existing storage target must be its own mapping source",
            ));
        }
        let allocation = match transition {
            StorageTransitionV2::Create { .. } => {
                result.created_storages.insert(id.clone());
                ServiceAllocationOrigin::Created {
                    edge,
                    storage: id.clone(),
                }
            }
            StorageTransitionV2::Reuse { source, .. }
            | StorageTransitionV2::Reattach { source, .. } => {
                result.consumed_storages.insert(source.storage_id.clone());
                before.storage(source)?.allocation.clone()
            }
        };
        if !active_allocations.insert(allocation.clone()) {
            return Err(conflict("target storages share an allocation"));
        }
        storages.insert(
            id.clone(),
            PlannedStorageAssociation {
                declaration_revision: target_revision.clone(),
                allocation,
            },
        );
    }
    let mut resources = BTreeMap::new();
    for transition in &mapping.resources {
        for source in transition.sources() {
            before.resource(source)?;
            result.consumed_resources.insert(source.resource_id.clone());
        }
        result.transform |= matches!(transition, ResourceTransitionV2::Transform { .. });
        for id in transition.targets() {
            if before.resources.contains_key(id)
                && !transition.sources().iter().any(|s| &s.resource_id == id)
            {
                return Err(conflict(
                    "existing resource target must belong to its writer's source group",
                ));
            }
            let declaration = target
                .service_core()
                .and_then(|c| c.resource(id))
                .ok_or_else(|| conflict("target resource is not declared"))?
                .clone();
            let allocation = storages
                .get(&declaration.storage_id)
                .ok_or_else(|| conflict("target storage has no writer"))?
                .allocation
                .clone();
            for (old_id, old) in &before.resources {
                if old.allocation == allocation
                    && old.declaration.locator.portable_key() == declaration.locator.portable_key()
                    && !transition
                        .sources()
                        .iter()
                        .any(|s| &s.resource_id == old_id)
                {
                    return Err(conflict(
                        "target location belongs to another active or retained resource",
                    ));
                }
            }
            if let ResourceTransitionV2::Reuse { source, .. }
            | ResourceTransitionV2::Reattach { source, .. } = transition
            {
                let old = before.resource(source)?;
                if old.allocation != allocation
                    || old.declaration.kind != declaration.kind
                    || old.declaration.locator != declaration.locator
                {
                    return Err(conflict(
                        "compatible resource mapping changes allocation, locator or kind",
                    ));
                }
            }
            let bound = PlannedResourceAssociation {
                declaration_revision: target_revision.clone(),
                declaration,
                allocation,
            };
            if let ResourceTransitionV2::Create { presence, .. } = transition {
                result.create_presence.push((*presence, bound.clone()));
            }
            resources.insert(id.clone(), bound);
        }
    }
    for id in &result.consumed_storages {
        result.after.storages.remove(id);
    }
    for id in &result.consumed_resources {
        result.after.resources.remove(id);
    }
    result.after.storages.extend(storages);
    result.after.resources.extend(resources);
    validate_current(&result.after, target)?;
    let hook = target.service_hook(&ServiceHookSite::Migration(digest));
    let bind = |reference: &ServiceReferenceV2| match reference.view {
        ServiceView::Source => before.object(reference),
        ServiceView::Target if reference.role == ServiceRole::Active => {
            result.after.object(reference)
        }
        _ => Err(conflict("Migration authority has an invalid view or role")),
    };
    result.grants = hook
        .access
        .iter()
        .map(|a| Ok((a.clone(), bind(&a.reference)?)))
        .collect::<Result<_>>()?;
    result.requires = hook
        .requires
        .iter()
        .map(|r| Ok((r.clone(), bind(&r.reference)?)))
        .collect::<Result<_>>()?;
    for transition in &mapping.resources {
        if let ResourceTransitionV2::Transform { sources, targets } = transition {
            for s in sources {
                let r = before.resource(s)?;
                if !covers(
                    &result.grants,
                    ServiceView::Source,
                    s.role,
                    r,
                    ServiceAccessMode::Read,
                ) {
                    return Err(conflict("transform source lacks read authority"));
                }
            }
            for id in targets {
                if !covers(
                    &result.grants,
                    ServiceView::Target,
                    ServiceRole::Active,
                    &result.after.resources[id],
                    ServiceAccessMode::Write,
                ) {
                    return Err(conflict("transform target lacks write authority"));
                }
            }
        }
    }
    Ok(result)
}

fn covers(
    grants: &[(ServiceAccessV2, PlannedServiceObject)],
    view: ServiceView,
    role: ServiceRole,
    resource: &PlannedResourceAssociation,
    mode: ServiceAccessMode,
) -> bool {
    grants.iter().any(|(access, object)| {
        if access.reference.view != view
            || access.reference.role != role
            || !access.mode.covers(mode)
            || object.allocation() != &resource.allocation
        {
            return false;
        }
        match object {
            PlannedServiceObject::Storage { .. } => true,
            PlannedServiceObject::Resource(parent) => {
                parent.declaration.id == resource.declaration.id
                    || (parent.declaration.kind == ServiceResourceKind::Directory
                        && resource
                            .declaration
                            .locator
                            .as_str()
                            .strip_prefix(parent.declaration.locator.as_str())
                            .is_some_and(|suffix| suffix.starts_with('/')))
            }
        }
    })
}

#[cfg(test)]
#[path = "service_migration_tests.rs"]
mod tests;
