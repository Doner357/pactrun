//! Published service contracts and allocation custody, without live contents or
//! native paths. Presence is deliberately not part of this durable state view.
use super::{
    ActionIdentity, InstanceId, InstanceStateVersion, RevisionIdentity, RunId, ServiceAllocationId,
    ServiceResourceV2, ServiceRole, ServiceStorageIdentity, ServiceStorageV2,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ServiceStorageAssociation {
    pub(crate) declaration: ServiceStorageV2,
    pub(crate) declaration_revision: RevisionIdentity,
    pub(crate) allocation: ServiceAllocationId,
    pub(crate) role: ServiceRole,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ServiceAccessIntent {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ObservedServiceObjectKind {
    File,
    Directory,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ServiceObservationCause {
    AllocationUnavailable,
    PermissionDenied,
    UnsafePath,
    IoFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ServiceObservation {
    Present {
        kind: ObservedServiceObjectKind,
        kind_matches: bool,
    },
    Absent,
    Unknown(ServiceObservationCause),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ServiceAccessError {
    UnknownResource,
    ExposureDenied { action: Option<ActionIdentity> },
    Unknown(ServiceObservationCause),
    ResourceAbsent,
    KindMismatch,
    UnsafePath,
}
impl ServiceAccessError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::UnknownResource => "unknown_resource",
            Self::ExposureDenied { .. } => "exposure_denied",
            Self::Unknown(ServiceObservationCause::AllocationUnavailable) => {
                "allocation_unavailable"
            }
            Self::Unknown(ServiceObservationCause::UnsafePath) | Self::UnsafePath => "unsafe_path",
            Self::Unknown(_) => "observation_unknown",
            Self::ResourceAbsent => "resource_absent",
            Self::KindMismatch => "resource_kind_mismatch",
        }
    }
}
impl std::fmt::Display for ServiceAccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "service_storage.{}", self.code())?;
        if let Self::ExposureDenied {
            action: Some(action),
        } = self
        {
            write!(
                f,
                ": use declared Action {} through managed execution",
                action.as_str()
            )?;
        }
        Ok(())
    }
}
impl std::error::Error for ServiceAccessError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ServiceResourceAssociation {
    pub(crate) declaration: ServiceResourceV2,
    pub(crate) declaration_revision: RevisionIdentity,
    pub(crate) allocation: ServiceAllocationId,
    pub(crate) role: ServiceRole,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PreservedServiceAllocation {
    pub(crate) allocation: ServiceAllocationId,
    pub(crate) origin_revision: RevisionIdentity,
    pub(crate) origin_storage: ServiceStorageIdentity,
    pub(crate) origin_run: Option<RunId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstanceServiceState {
    pub(crate) instance: InstanceId,
    pub(crate) state_version: InstanceStateVersion,
    pub(crate) current_revision: RevisionIdentity,
    pub(crate) storages: Vec<ServiceStorageAssociation>,
    pub(crate) resources: Vec<ServiceResourceAssociation>,
    pub(crate) preserved: Vec<PreservedServiceAllocation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BoundServiceObject {
    Storage {
        allocation: ServiceAllocationId,
    },
    Resource {
        allocation: ServiceAllocationId,
        declaration: ServiceResourceV2,
    },
}
impl BoundServiceObject {
    pub(crate) fn allocation(&self) -> ServiceAllocationId {
        match self {
            Self::Storage { allocation } | Self::Resource { allocation, .. } => *allocation,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub(crate) struct ServiceHookBindings {
    pub(crate) grants: Vec<(super::ServiceAccessV2, BoundServiceObject)>,
    pub(crate) requires: Vec<(super::ServicePrerequisiteV2, BoundServiceObject)>,
}

/// Bind only durable association metadata, never resource presence or bytes.
/// Retaining this in the ephemeral Plan preserves the admitted mapping even if
/// an Observe Run overlaps a later Migration or its admission ack is lost.
pub(crate) fn bind_current_service_hook(
    contract: &super::HookServiceContractV2,
    observed: Option<&InstanceServiceState>,
    instance: InstanceId,
    version: InstanceStateVersion,
    revision: &RevisionIdentity,
) -> Result<ServiceHookBindings, super::PlanCompilationError> {
    use super::{
        PlanCompilationError::InconsistentFacts, ServiceReferenceV2, ServiceScope, ServiceView,
    };
    if contract.access.is_empty() && contract.requires.is_empty() {
        return Ok(ServiceHookBindings::default());
    }
    let observed = observed.ok_or(InconsistentFacts)?;
    if observed.instance != instance
        || observed.state_version != version
        || &observed.current_revision != revision
    {
        return Err(InconsistentFacts);
    }
    let bind = |reference: &ServiceReferenceV2| {
        if reference.view != ServiceView::Current || reference.role != ServiceRole::Active {
            return Err(InconsistentFacts);
        }
        match &reference.scope {
            ServiceScope::Storage(id) => observed
                .storages
                .iter()
                .find(|s| {
                    s.role == ServiceRole::Active
                        && &s.declaration.id == id
                        && &s.declaration_revision == revision
                })
                .map(|s| BoundServiceObject::Storage {
                    allocation: s.allocation,
                })
                .ok_or(InconsistentFacts),
            ServiceScope::Resource(id) => observed
                .resources
                .iter()
                .find(|r| {
                    r.role == ServiceRole::Active
                        && &r.declaration.id == id
                        && &r.declaration_revision == revision
                        && observed.storages.iter().any(|s| {
                            s.role == ServiceRole::Active
                                && s.declaration.id == r.declaration.storage_id
                                && &s.declaration_revision == revision
                                && s.allocation == r.allocation
                        })
                })
                .map(|r| BoundServiceObject::Resource {
                    allocation: r.allocation,
                    declaration: r.declaration.clone(),
                })
                .ok_or(InconsistentFacts),
        }
    };
    Ok(ServiceHookBindings {
        grants: contract
            .access
            .iter()
            .map(|a| Ok((a.clone(), bind(&a.reference)?)))
            .collect::<Result<_, _>>()?,
        requires: contract
            .requires
            .iter()
            .map(|r| Ok((r.clone(), bind(&r.reference)?)))
            .collect::<Result<_, _>>()?,
    })
}
