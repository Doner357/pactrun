//! Observe-only service queries and declared path disclosure. These methods do
//! not acquire execution admission or mutate live content, guards or state.
use super::{ApplicationError, PactrunApplication};
use crate::{
    domain::*,
    service_storage::{self as fs, ResourceLookup},
};
use std::{io, path::PathBuf};

impl From<ServiceAccessError> for ApplicationError {
    fn from(value: ServiceAccessError) -> Self {
        Self::ServiceStorage(value)
    }
}

impl PactrunApplication {
    pub(crate) fn load_instance_services(
        &self,
        name: &InstanceName,
    ) -> Result<InstanceServiceState, ApplicationError> {
        let id = self
            .persistence
            .resolve_instance_name(name)?
            .ok_or(ActionResolutionError::InstanceNotFound)?;
        self.persistence
            .load_instance_service_state(id)?
            .ok_or_else(|| ActionResolutionError::InstanceNotFound.into())
    }

    pub(crate) fn load_service_resource(
        &self,
        name: &InstanceName,
        id: &ServiceResourceIdentity,
        role: ServiceRole,
    ) -> Result<ServiceResourceAssociation, ApplicationError> {
        self.load_instance_services(name)?
            .resources
            .into_iter()
            .find(|r| r.declaration.id == *id && r.role == role)
            .ok_or_else(|| ServiceAccessError::UnknownResource.into())
    }

    pub(crate) fn observe_service_resource(
        &self,
        name: &InstanceName,
        id: &ServiceResourceIdentity,
        role: ServiceRole,
    ) -> Result<ServiceObservation, ApplicationError> {
        let resource = self.load_service_resource(name, id, role)?;
        Ok(match self.lookup_service_resource(&resource) {
            Ok(ResourceLookup::Present { kind, .. }) => ServiceObservation::Present {
                kind,
                kind_matches: kind_matches(kind, resource.declaration.kind),
            },
            Ok(ResourceLookup::Absent { .. }) => ServiceObservation::Absent,
            Err(cause) => ServiceObservation::Unknown(cause),
        })
    }

    pub(crate) fn locate_service_resource(
        &self,
        name: &InstanceName,
        id: &ServiceResourceIdentity,
        role: ServiceRole,
        intent: ServiceAccessIntent,
    ) -> Result<PathBuf, ApplicationError> {
        let resource = self.load_service_resource(name, id, role)?;
        match intent {
            ServiceAccessIntent::Read
                if resource.declaration.read_exposure != ServiceReadExposure::Readable =>
            {
                return Err(ServiceAccessError::ExposureDenied { action: None }.into());
            }
            ServiceAccessIntent::Write => {
                if role != ServiceRole::Active {
                    return Err(ServiceAccessError::ExposureDenied { action: None }.into());
                }
                match &resource.declaration.user_mutation {
                    ServiceUserMutation::Direct => (),
                    ServiceUserMutation::Unavailable => {
                        return Err(ServiceAccessError::ExposureDenied { action: None }.into());
                    }
                    ServiceUserMutation::Operation { action_id } => {
                        return Err(ServiceAccessError::ExposureDenied {
                            action: Some(action_id.clone()),
                        }
                        .into());
                    }
                }
            }
            _ => (),
        }
        match self
            .lookup_service_resource(&resource)
            .map_err(ServiceAccessError::Unknown)?
        {
            ResourceLookup::Present { kind, links } => {
                if !kind_matches(kind, resource.declaration.kind) {
                    return Err(ServiceAccessError::KindMismatch.into());
                }
                if kind == ObservedServiceObjectKind::File && links != 1 {
                    return Err(ServiceAccessError::UnsafePath.into());
                }
            }
            ResourceLookup::Absent { parent_exists } => {
                if intent == ServiceAccessIntent::Read {
                    return Err(ServiceAccessError::ResourceAbsent.into());
                }
                if !parent_exists {
                    return Err(ServiceAccessError::UnsafePath.into());
                }
            }
        }
        Ok(self
            .storage_root
            .join("service-storage")
            .join(format!("alloc-{}", resource.allocation))
            .join(resource.declaration.locator.as_str()))
    }

    fn lookup_service_resource(
        &self,
        resource: &ServiceResourceAssociation,
    ) -> Result<ResourceLookup, ServiceObservationCause> {
        // Missing protected roots are Unknown, never absence or a create request.
        let allocation = (|| -> io::Result<_> {
            let root = fs::open_root(&self.storage_root)?;
            let base = fs::open_directory(&root, "service-storage")?;
            fs::open_directory(&base, &format!("alloc-{}", resource.allocation))
        })()
        .map_err(|_| ServiceObservationCause::AllocationUnavailable)?;
        fs::observe_resource(&allocation, &resource.declaration.locator)
            .map_err(fs::observation_cause)
    }
}

fn kind_matches(observed: ObservedServiceObjectKind, declared: ServiceResourceKind) -> bool {
    matches!(
        (observed, declared),
        (ObservedServiceObjectKind::File, ServiceResourceKind::File)
            | (
                ObservedServiceObjectKind::Directory,
                ServiceResourceKind::Directory
            )
    )
}
