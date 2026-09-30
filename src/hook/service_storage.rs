//! Pre-launch native service authority qualification. Paths stay outside the
//! execution directory; presence is observed once per physical resource for
//! this invocation, never cached as durable service state or readiness.
use super::protocol::authority::{ExpectedAuthority, MaterializedAuthority};
use crate::{
    domain::*,
    persistence::{PactrunPersistence, PersistenceError},
    service_storage::{self as fs, ResourceLookup},
};
use std::{collections::BTreeMap, fs::File, path::PathBuf};

#[derive(Debug)]
pub(crate) enum NativeServiceError {
    Storage(PersistenceError),
    Access(ServiceAccessError),
    Prerequisite,
}
impl From<PersistenceError> for NativeServiceError {
    fn from(e: PersistenceError) -> Self {
        Self::Storage(e)
    }
}
impl From<ServiceAccessError> for NativeServiceError {
    fn from(e: ServiceAccessError) -> Self {
        Self::Access(e)
    }
}

pub(crate) struct PreparedServiceAccess {
    _roots: Vec<File>,
    authorities: Vec<MaterializedAuthority>,
}
impl PreparedServiceAccess {
    pub(crate) fn grant_count(&self) -> usize {
        self.authorities.len()
    }
    pub(super) fn declarations(&self) -> Vec<ExpectedAuthority> {
        self.authorities
            .iter()
            .map(|a| a.declaration.clone())
            .collect()
    }
    pub(super) fn take_authorities(&mut self) -> Vec<MaterializedAuthority> {
        std::mem::take(&mut self.authorities)
    }
}

pub(crate) fn prepare(
    p: &PactrunPersistence,
    run: RunId,
    owner: &ExecutionOwnerSession,
    bindings: &ServiceHookBindings,
) -> Result<PreparedServiceAccess, NativeServiceError> {
    let mut roots = BTreeMap::<ServiceAllocationId, (File, PathBuf)>::new();
    for object in bindings
        .grants
        .iter()
        .map(|(_, o)| o)
        .chain(bindings.requires.iter().map(|(_, o)| o))
    {
        if let std::collections::btree_map::Entry::Vacant(entry) = roots.entry(object.allocation())
        {
            entry.insert(p.open_pinned_service_allocation(run, owner, object.allocation())?);
        }
    }
    let mut observations =
        BTreeMap::<(ServiceAllocationId, ServiceLocatorV2), ResourceLookup>::new();
    for object in bindings
        .grants
        .iter()
        .map(|(_, o)| o)
        .chain(bindings.requires.iter().map(|(_, o)| o))
    {
        if let BoundServiceObject::Resource {
            allocation,
            declaration,
        } = object
        {
            let key = (*allocation, declaration.locator.clone());
            if let std::collections::btree_map::Entry::Vacant(entry) = observations.entry(key) {
                let observed = fs::observe_resource(&roots[allocation].0, &declaration.locator)
                    .map_err(|e| ServiceAccessError::Unknown(fs::observation_cause(e)))?;
                entry.insert(observed);
            }
        }
    }
    for (required, object) in &bindings.requires {
        let BoundServiceObject::Resource {
            allocation,
            declaration,
        } = object
        else {
            return Err(NativeServiceError::Prerequisite);
        };
        let observed = observations[&(*allocation, declaration.locator.clone())];
        if !matches!(
            (required.presence, observed),
            (
                ServicePresenceRequirement::Present,
                ResourceLookup::Present { .. }
            ) | (
                ServicePresenceRequirement::Absent,
                ResourceLookup::Absent { .. }
            )
        ) {
            return Err(NativeServiceError::Prerequisite);
        }
    }
    let mut authorities = Vec::new();
    for (access, object) in &bindings.grants {
        let allocation = object.allocation();
        let (path, resource_kind) = match object {
            BoundServiceObject::Storage { .. } => (roots[&allocation].1.clone(), None),
            BoundServiceObject::Resource { declaration, .. } => {
                match observations[&(allocation, declaration.locator.clone())] {
                    ResourceLookup::Present { kind, links } => {
                        if !matches!(
                            (kind, declaration.kind),
                            (ObservedServiceObjectKind::File, ServiceResourceKind::File)
                                | (
                                    ObservedServiceObjectKind::Directory,
                                    ServiceResourceKind::Directory
                                )
                        ) {
                            return Err(ServiceAccessError::KindMismatch.into());
                        }
                        if kind == ObservedServiceObjectKind::File && links != 1 {
                            return Err(ServiceAccessError::UnsafePath.into());
                        }
                    }
                    ResourceLookup::Absent { parent_exists } => {
                        // Read authority does not imply a Present prerequisite.
                        // Resource-only write never creates missing ancestors.
                        if access.mode == ServiceAccessMode::Write
                            && !parent_exists
                            && !has_parent_write(bindings, object)
                        {
                            return Err(ServiceAccessError::UnsafePath.into());
                        }
                    }
                }
                (
                    roots[&allocation].1.join(declaration.locator.as_str()),
                    Some(declaration.kind),
                )
            }
        };
        let path = path
            .to_str()
            .ok_or(ServiceAccessError::UnsafePath)?
            .to_owned();
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes)
            .map_err(|_| ServiceAccessError::Unknown(ServiceObservationCause::IoFailure))?;
        authorities.push(MaterializedAuthority {
            declaration: ExpectedAuthority {
                access: access.clone(),
                resource_kind,
            },
            handle: hex::encode(bytes),
            path,
        });
    }
    Ok(PreparedServiceAccess {
        _roots: roots.into_values().map(|(file, _)| file).collect(),
        authorities,
    })
}

fn has_parent_write(bindings: &ServiceHookBindings, target: &BoundServiceObject) -> bool {
    let BoundServiceObject::Resource {
        allocation,
        declaration,
    } = target
    else {
        return false;
    };
    bindings.grants.iter().any(|(access, object)| {
        if access.mode != ServiceAccessMode::Write || object.allocation() != *allocation {
            return false;
        }
        match object {
            BoundServiceObject::Storage { .. } => true,
            BoundServiceObject::Resource {
                declaration: parent,
                ..
            } => {
                parent.kind == ServiceResourceKind::Directory
                    && declaration
                        .locator
                        .as_str()
                        .strip_prefix(parent.locator.as_str())
                        .is_some_and(|suffix| suffix.starts_with('/'))
            }
        }
    })
}
