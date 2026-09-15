//! Typed service declarations and Core V2 policy. No filesystem or adapter state.

use super::{
    ActionIdentity, HookV1, InputIdentity, OperationAccessV1, RevisionCoreV1, RevisionCoreV1Error,
    Sha256Digest,
};
use serde::Serialize;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fmt,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ServiceContractError {
    code: String,
    message: String,
}
impl ServiceContractError {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
    pub(crate) fn code(&self) -> &str {
        &self.code
    }
}
impl From<RevisionCoreV1Error> for ServiceContractError {
    fn from(e: RevisionCoreV1Error) -> Self {
        Self::new(e.internal_code(), e.to_string())
    }
}
impl fmt::Display for ServiceContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ServiceContractError {}
fn invalid(message: &str) -> ServiceContractError {
    ServiceContractError::new("invalid_service_declaration", message)
}
fn mapping(message: &str) -> ServiceContractError {
    ServiceContractError::new("invalid_service_mapping", message)
}

macro_rules! service_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub(crate) struct $name(InputIdentity);
        impl $name {
            pub(crate) fn parse(value: impl Into<String>) -> Result<Self, ServiceContractError> {
                InputIdentity::parse(value).map(Self).map_err(Into::into)
            }
            pub(crate) fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }
    };
}
service_id!(ServiceStorageIdentity);
service_id!(ServiceResourceIdentity);

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(transparent)]
pub(crate) struct ServiceLocatorV2(String);
impl ServiceLocatorV2 {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, ServiceContractError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 1024
            || !value.is_ascii()
            || value.split('/').any(|s| {
                s.is_empty()
                    || s.len() > 128
                    || s == "."
                    || s == ".."
                    || s.ends_with('.')
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
                    || super::revision_core_v1::is_windows_reserved(s)
            })
        {
            return Err(invalid("invalid portable service locator"));
        }
        Ok(Self(value))
    }
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
    pub(crate) fn portable_key(&self) -> String {
        self.0.to_ascii_lowercase()
    }
    fn contains(&self, other: &Self) -> bool {
        self == other
            || other
                .0
                .strip_prefix(&self.0)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServiceResourceKind {
    File,
    Directory,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServiceReadExposure {
    Hidden,
    Readable,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ServiceUserMutation {
    Unavailable,
    Direct,
    Operation { action_id: ActionIdentity },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ServiceStorageV2 {
    pub(crate) id: ServiceStorageIdentity,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ServiceResourceV2 {
    pub(crate) id: ServiceResourceIdentity,
    pub(crate) storage_id: ServiceStorageIdentity,
    pub(crate) locator: ServiceLocatorV2,
    pub(crate) kind: ServiceResourceKind,
    pub(crate) read_exposure: ServiceReadExposure,
    pub(crate) user_mutation: ServiceUserMutation,
}

// Variant declaration order is the specified ASCII token order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServiceView {
    Current,
    Source,
    Target,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServiceRole {
    Active,
    Retained,
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub(crate) enum ServiceScope {
    Resource(ServiceResourceIdentity),
    Storage(ServiceStorageIdentity),
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub(crate) struct ServiceReferenceV2 {
    pub(crate) view: ServiceView,
    pub(crate) role: ServiceRole,
    #[serde(flatten)]
    pub(crate) scope: ServiceScope,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServiceAccessMode {
    Read,
    Write,
}
impl ServiceAccessMode {
    pub(crate) fn covers(self, required: Self) -> bool {
        self == Self::Write || required == Self::Read
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ServiceAccessV2 {
    pub(crate) reference: ServiceReferenceV2,
    pub(crate) mode: ServiceAccessMode,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServicePresenceRequirement {
    Present,
    Absent,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServiceCreatePresence {
    Any,
    Present,
    Absent,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ServicePrerequisiteV2 {
    pub(crate) reference: ServiceReferenceV2,
    pub(crate) presence: ServicePresenceRequirement,
}
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub(crate) struct HookServiceContractV2 {
    pub(crate) access: Vec<ServiceAccessV2>,
    pub(crate) requires: Vec<ServicePrerequisiteV2>,
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum ServiceHookSite {
    Action(ActionIdentity),
    Capture,
    Restore,
    Migration(Sha256Digest),
    Cleanup,
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub(crate) struct StorageSourceV2 {
    pub(crate) role: ServiceRole,
    pub(crate) storage_id: ServiceStorageIdentity,
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub(crate) struct ResourceSourceV2 {
    pub(crate) role: ServiceRole,
    pub(crate) resource_id: ServiceResourceIdentity,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum StorageTransitionV2 {
    Create {
        target_storage_id: ServiceStorageIdentity,
    },
    Reuse {
        source: StorageSourceV2,
        target_storage_id: ServiceStorageIdentity,
    },
    Reattach {
        source: StorageSourceV2,
        target_storage_id: ServiceStorageIdentity,
    },
}
impl StorageTransitionV2 {
    pub(crate) fn target(&self) -> &ServiceStorageIdentity {
        match self {
            Self::Create { target_storage_id }
            | Self::Reuse {
                target_storage_id, ..
            }
            | Self::Reattach {
                target_storage_id, ..
            } => target_storage_id,
        }
    }
    pub(crate) fn source(&self) -> Option<&StorageSourceV2> {
        match self {
            Self::Create { .. } => None,
            Self::Reuse { source, .. } | Self::Reattach { source, .. } => Some(source),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ResourceTransitionV2 {
    Create {
        target_resource_id: ServiceResourceIdentity,
        presence: ServiceCreatePresence,
    },
    Reuse {
        source: ResourceSourceV2,
        target_resource_id: ServiceResourceIdentity,
    },
    Reattach {
        source: ResourceSourceV2,
        target_resource_id: ServiceResourceIdentity,
    },
    Transform {
        sources: Vec<ResourceSourceV2>,
        targets: Vec<ServiceResourceIdentity>,
    },
}
impl ResourceTransitionV2 {
    pub(crate) fn targets(&self) -> &[ServiceResourceIdentity] {
        match self {
            Self::Create {
                target_resource_id, ..
            }
            | Self::Reuse {
                target_resource_id, ..
            }
            | Self::Reattach {
                target_resource_id, ..
            } => std::slice::from_ref(target_resource_id),
            Self::Transform { targets, .. } => targets,
        }
    }
    pub(crate) fn sources(&self) -> &[ResourceSourceV2] {
        match self {
            Self::Create { .. } => &[],
            Self::Reuse { source, .. } | Self::Reattach { source, .. } => {
                std::slice::from_ref(source)
            }
            Self::Transform { sources, .. } => sources,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub(crate) struct ServiceMigrationV2 {
    pub(crate) storages: Vec<StorageTransitionV2>,
    pub(crate) resources: Vec<ResourceTransitionV2>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RevisionCoreV2 {
    common: RevisionCoreV1,
    storages: Vec<ServiceStorageV2>,
    resources: Vec<ServiceResourceV2>,
    hooks: BTreeMap<ServiceHookSite, HookServiceContractV2>,
    migrations: BTreeMap<Sha256Digest, ServiceMigrationV2>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ValidatedRevisionContentV2 {
    pub(crate) core: RevisionCoreV2,
    pub(crate) runtime_content: super::RuntimeContentClosureIdentityV1,
}
pub(crate) fn project_revision_content_v2(
    core: RevisionCoreV2,
    runtime_content: super::RuntimeContentClosureIdentityV1,
) -> Result<ValidatedRevisionContentV2, ServiceContractError> {
    super::validate_revision_content_v1(core.common().clone(), runtime_content.clone())?;
    Ok(ValidatedRevisionContentV2 {
        core,
        runtime_content,
    })
}
impl RevisionCoreV2 {
    pub(crate) fn common(&self) -> &RevisionCoreV1 {
        &self.common
    }
    pub(crate) fn storages(&self) -> &[ServiceStorageV2] {
        &self.storages
    }
    pub(crate) fn resources(&self) -> &[ServiceResourceV2] {
        &self.resources
    }
    pub(crate) fn hooks(&self) -> &BTreeMap<ServiceHookSite, HookServiceContractV2> {
        &self.hooks
    }
    pub(crate) fn migrations(&self) -> &BTreeMap<Sha256Digest, ServiceMigrationV2> {
        &self.migrations
    }
    pub(crate) fn resource(&self, id: &ServiceResourceIdentity) -> Option<&ServiceResourceV2> {
        self.resources
            .binary_search_by(|r| r.id.cmp(id))
            .ok()
            .map(|index| &self.resources[index])
    }
    pub(crate) fn has_storage(&self, id: &ServiceStorageIdentity) -> bool {
        self.storages.binary_search_by(|s| s.id.cmp(id)).is_ok()
    }
}

fn set<T>(
    values: &mut [T],
    compare: impl Fn(&T, &T) -> Ordering,
) -> Result<(), ServiceContractError> {
    values.sort_by(&compare);
    if values
        .windows(2)
        .any(|w| compare(&w[0], &w[1]) == Ordering::Equal)
    {
        return Err(ServiceContractError::new(
            "duplicate_semantic_key",
            "duplicate service semantic key",
        ));
    }
    Ok(())
}

pub(crate) fn project_revision_core_v2(
    common: RevisionCoreV1,
    storages: Vec<ServiceStorageV2>,
    resources: Vec<ServiceResourceV2>,
    hooks: BTreeMap<ServiceHookSite, HookServiceContractV2>,
    migrations: BTreeMap<Sha256Digest, ServiceMigrationV2>,
) -> Result<RevisionCoreV2, ServiceContractError> {
    let mut core = RevisionCoreV2 {
        common,
        storages,
        resources,
        hooks,
        migrations,
    };
    set(&mut core.storages, |a, b| a.id.cmp(&b.id))?;
    set(&mut core.resources, |a, b| a.id.cmp(&b.id))?;
    let mut locations = BTreeMap::new();
    for r in &core.resources {
        if !core.has_storage(&r.storage_id) {
            return Err(invalid("resource refers to an undeclared storage"));
        }
        if locations
            .insert((&r.storage_id, r.locator.portable_key()), r.kind)
            .is_some()
        {
            return Err(invalid("portable resource locator alias"));
        }
    }
    for (storage, path) in locations.keys() {
        for (offset, _) in path.match_indices('/') {
            if locations.get(&(*storage, path[..offset].to_owned()))
                == Some(&ServiceResourceKind::File)
            {
                return Err(invalid("file resource cannot contain another resource"));
            }
        }
    }
    let owners = hook_owners(&core.common);
    if core.hooks.keys().collect::<BTreeSet<_>>() != owners.keys().collect() {
        return Err(invalid(
            "service Hook contracts must exactly cover Hook positions",
        ));
    }
    for (site, h) in &mut core.hooks {
        set(&mut h.access, |a, b| a.reference.cmp(&b.reference))?;
        set(&mut h.requires, |a, b| a.reference.cmp(&b.reference))?;
        let (hook, access) = owners[site];
        if (!h.access.is_empty() || !h.requires.is_empty()) && hook.protocol_version.get() != 2 {
            return Err(invalid(
                "persistent authority/prerequisites require protocol 2",
            ));
        }
        if h.access.iter().any(|a| a.mode == ServiceAccessMode::Write)
            && access != OperationAccessV1::Mutate
        {
            return Err(invalid("service write authority requires Mutate"));
        }
        for p in &h.requires {
            if !matches!(p.reference.scope, ServiceScope::Resource(_)) {
                return Err(invalid(
                    "presence prerequisites name resources, not storages",
                ));
            }
        }
    }
    for (site, h) in &core.hooks {
        for reference in h
            .access
            .iter()
            .map(|a| &a.reference)
            .chain(h.requires.iter().map(|p| &p.reference))
        {
            validate_reference(&core, site, reference)?;
        }
        reject_redundant_grants(&core, h)?;
    }
    for r in &core.resources {
        if let ServiceUserMutation::Operation { action_id } = &r.user_mutation {
            let action = core
                .common
                .actions()
                .iter()
                .find(|a| &a.id == action_id)
                .ok_or_else(|| invalid("unknown user mutation Action"))?;
            if action.access != OperationAccessV1::Mutate {
                return Err(invalid("user mutation Action must be Mutate"));
            }
            let h = &core.hooks[&ServiceHookSite::Action(action_id.clone())];
            let reference = ServiceReferenceV2 {
                view: ServiceView::Current,
                role: ServiceRole::Active,
                scope: ServiceScope::Resource(r.id.clone()),
            };
            if !has_resource_authority(&core, h, &reference, ServiceAccessMode::Write) {
                return Err(invalid(
                    "user mutation Action lacks resource write authority",
                ));
            }
        }
    }
    let expected: BTreeSet<_> = core
        .common
        .migrations()
        .iter()
        .map(|m| &m.source_revision_digest)
        .collect();
    if core.migrations.keys().collect::<BTreeSet<_>>() != expected {
        return Err(mapping("service mappings must exactly cover inbound edges"));
    }
    let mut mappings = std::mem::take(&mut core.migrations);
    for (source, m) in &mut mappings {
        normalize_mappings(&core, source, m)?;
    }
    core.migrations = mappings;
    Ok(core)
}

fn hook_owners(core: &RevisionCoreV1) -> BTreeMap<ServiceHookSite, (&HookV1, OperationAccessV1)> {
    let mut owners = BTreeMap::new();
    for a in core.actions() {
        owners.insert(ServiceHookSite::Action(a.id.clone()), (&a.hook, a.access));
    }
    if let Some(s) = core.snapshot() {
        if let Some(c) = &s.capture {
            owners.insert(ServiceHookSite::Capture, (&c.hook, c.access));
        }
        if let Some(r) = &s.restore {
            owners.insert(
                ServiceHookSite::Restore,
                (&r.hook, OperationAccessV1::Mutate),
            );
        }
    }
    for m in core.migrations() {
        if let Some(h) = &m.hook {
            owners.insert(
                ServiceHookSite::Migration(m.source_revision_digest.clone()),
                (h, OperationAccessV1::Mutate),
            );
        }
    }
    if let Some(c) = core.cleanup() {
        owners.insert(
            ServiceHookSite::Cleanup,
            (&c.hook, OperationAccessV1::Mutate),
        );
    }
    owners
}

fn validate_reference(
    core: &RevisionCoreV2,
    site: &ServiceHookSite,
    r: &ServiceReferenceV2,
) -> Result<(), ServiceContractError> {
    let migration = matches!(site, ServiceHookSite::Migration(_));
    let valid = if migration {
        r.view == ServiceView::Source
            || (r.view == ServiceView::Target && r.role == ServiceRole::Active)
    } else {
        r.view == ServiceView::Current
            && (r.role == ServiceRole::Active || matches!(site, ServiceHookSite::Cleanup))
    };
    if !valid {
        return Err(invalid("invalid service reference view/role for Hook"));
    }
    if r.view == ServiceView::Source {
        return Ok(());
    }
    let exists = match &r.scope {
        ServiceScope::Storage(id) => core.has_storage(id),
        ServiceScope::Resource(id) => core.resource(id).is_some(),
    };
    if exists != (r.role == ServiceRole::Active) {
        return Err(invalid(
            "service reference role does not match local declaration",
        ));
    }
    Ok(())
}

pub(crate) fn has_resource_authority(
    core: &RevisionCoreV2,
    h: &HookServiceContractV2,
    r: &ServiceReferenceV2,
    mode: ServiceAccessMode,
) -> bool {
    h.access.iter().any(|grant| {
        if grant.reference.view != r.view
            || grant.reference.role != r.role
            || !grant.mode.covers(mode)
        {
            return false;
        }
        if grant.reference.scope == r.scope {
            return true;
        }
        let ServiceScope::Resource(id) = &r.scope else {
            return false;
        };
        let Some(target) = core.resource(id) else {
            return false;
        };
        match &grant.reference.scope {
            ServiceScope::Storage(id) => id == &target.storage_id,
            ServiceScope::Resource(id) => core.resource(id).is_some_and(|parent| {
                parent.kind == ServiceResourceKind::Directory
                    && parent.storage_id == target.storage_id
                    && parent.locator.contains(&target.locator)
            }),
        }
    })
}

fn reject_redundant_grants(
    core: &RevisionCoreV2,
    h: &HookServiceContractV2,
) -> Result<(), ServiceContractError> {
    for a in &h.access {
        if a.reference.view == ServiceView::Source || a.reference.role == ServiceRole::Retained {
            continue;
        }
        let ServiceScope::Resource(id) = &a.reference.scope else {
            continue;
        };
        let r = core
            .resource(id)
            .ok_or_else(|| invalid("unknown granted resource"))?;
        if h.access.iter().any(|s| {
            s.reference.view == a.reference.view
                && s.reference.role == a.reference.role
                && s.reference.scope == ServiceScope::Storage(r.storage_id.clone())
                && s.mode.covers(a.mode)
        }) {
            return Err(invalid("redundant whole-storage/resource grant"));
        }
    }
    Ok(())
}

fn record_role(
    roles: &mut BTreeMap<ServiceScope, ServiceRole>,
    scope: ServiceScope,
    role: ServiceRole,
) -> Result<(), ServiceContractError> {
    if roles.insert(scope, role).is_some_and(|old| old != role) {
        return Err(mapping("one source identity has contradictory roles"));
    }
    Ok(())
}

fn normalize_mappings(
    core: &RevisionCoreV2,
    source: &Sha256Digest,
    m: &mut ServiceMigrationV2,
) -> Result<(), ServiceContractError> {
    let mut roles = BTreeMap::new();
    let mut consumed = BTreeSet::new();
    for s in &m.storages {
        if let Some(from) = s.source() {
            let scope = ServiceScope::Storage(from.storage_id.clone());
            record_role(&mut roles, scope.clone(), from.role)?;
            if !consumed.insert(scope) {
                return Err(mapping("duplicate source storage disposition"));
            }
        }
        if matches!(s,StorageTransitionV2::Reuse{source,..} if source.role!=ServiceRole::Active)
            || matches!(s,StorageTransitionV2::Reattach{source,..} if source.role!=ServiceRole::Retained)
        {
            return Err(mapping("wrong storage mapping source role"));
        }
    }
    set(&mut m.storages, |a, b| a.target().cmp(b.target()))?;
    if m.storages
        .iter()
        .map(|s| s.target())
        .collect::<BTreeSet<_>>()
        != core.storages.iter().map(|s| &s.id).collect()
    {
        return Err(mapping("storage target writers do not cover declarations"));
    }
    let hook = core.hooks.get(&ServiceHookSite::Migration(source.clone()));
    let mut written = BTreeSet::new();
    for r in &mut m.resources {
        if let ResourceTransitionV2::Transform { sources, targets } = r {
            if sources.is_empty() || targets.is_empty() {
                return Err(mapping("transform needs nonempty sources and targets"));
            }
            set(sources, Ord::cmp)?;
            set(targets, Ord::cmp)?;
            let Some(h) = hook else {
                return Err(mapping("transform requires a Hook"));
            };
            let declaration = core
                .common
                .migrations()
                .iter()
                .find(|m| &m.source_revision_digest == source)
                .and_then(|m| m.hook.as_ref())
                .expect("validated Hook owner");
            if declaration.protocol_version.get() != 2 {
                return Err(mapping("transform requires protocol 2"));
            }
            for target in targets.iter() {
                let reference = ServiceReferenceV2 {
                    view: ServiceView::Target,
                    role: ServiceRole::Active,
                    scope: ServiceScope::Resource(target.clone()),
                };
                if !has_resource_authority(core, h, &reference, ServiceAccessMode::Write) {
                    return Err(mapping("transform lacks target write authority"));
                }
            }
            // Source storage/directory coverage needs exact source semantics;
            // it is checked relationally, never guessed from target declarations.
        }
        if matches!(r,ResourceTransitionV2::Reuse{source,..} if source.role!=ServiceRole::Active)
            || matches!(r,ResourceTransitionV2::Reattach{source,..} if source.role!=ServiceRole::Retained)
        {
            return Err(mapping("wrong resource mapping source role"));
        }
        for s in r.sources() {
            let scope = ServiceScope::Resource(s.resource_id.clone());
            record_role(&mut roles, scope.clone(), s.role)?;
            if !consumed.insert(scope) {
                return Err(mapping("duplicate source resource disposition"));
            }
        }
        for t in r.targets() {
            if core.resource(t).is_none() || !written.insert(t.clone()) {
                return Err(mapping("unknown or multiply written resource target"));
            }
        }
        if let ResourceTransitionV2::Create {
            target_resource_id,
            presence,
        } = r
            && let Some(h) = hook
        {
            for p in &h.requires {
                if p.reference.view == ServiceView::Target
                    && p.reference.scope == ServiceScope::Resource(target_resource_id.clone())
                    && matches!(
                        (*presence, p.presence),
                        (
                            ServiceCreatePresence::Present,
                            ServicePresenceRequirement::Absent
                        ) | (
                            ServiceCreatePresence::Absent,
                            ServicePresenceRequirement::Present
                        )
                    )
                {
                    return Err(mapping("create presence contradicts Hook prerequisite"));
                }
            }
        }
    }
    if written != core.resources.iter().map(|r| r.id.clone()).collect() {
        return Err(mapping("resource target writers do not cover declarations"));
    }
    set(&mut m.resources, |a, b| a.targets()[0].cmp(&b.targets()[0]))?;
    if let Some(h) = hook {
        for r in h
            .access
            .iter()
            .map(|a| &a.reference)
            .chain(h.requires.iter().map(|p| &p.reference))
        {
            if r.view == ServiceView::Source {
                record_role(&mut roles, r.scope.clone(), r.role)?;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) enum ServiceSourceCore<'a> {
    V1(&'a RevisionCoreV1),
    V2(&'a RevisionCoreV2),
}
impl ServiceSourceCore<'_> {
    fn common(&self) -> &RevisionCoreV1 {
        match self {
            Self::V1(c) => c,
            Self::V2(c) => c.common(),
        }
    }
    fn resources(&self) -> &[ServiceResourceV2] {
        match self {
            Self::V1(_) => &[],
            Self::V2(c) => c.resources(),
        }
    }
    fn has(&self, scope: &ServiceScope) -> bool {
        match self {
            Self::V1(_) => false,
            Self::V2(c) => match scope {
                ServiceScope::Resource(id) => c.resource(id).is_some(),
                ServiceScope::Storage(id) => c.has_storage(id),
            },
        }
    }
}

/// Exact-source format checks only. A Valid result never certifies actual
/// retained availability, physical allocation equality, presence, or coherence.
/// Those facts must still be qualified from the Instance at Admission.
pub(crate) fn validate_service_sources_v2(
    target: &RevisionCoreV2,
    sources: &BTreeMap<Sha256Digest, ServiceSourceCore<'_>>,
) -> Result<super::RelationalValidationV1, ServiceContractError> {
    let input_ids = sources
        .iter()
        .map(|(id, c)| {
            (
                id.clone(),
                c.common().inputs().iter().map(|i| i.id.clone()).collect(),
            )
        })
        .collect();
    let mut all = super::validate_revision_sources_v1(target.common(), &input_ids)?
        == super::RelationalValidationV1::Valid;
    for (digest, m) in target.migrations() {
        let Some(source) = sources.get(digest) else {
            all = false;
            continue;
        };
        let check_role =
            |scope: &ServiceScope, role: ServiceRole| -> Result<(), ServiceContractError> {
                if source.has(scope) != (role == ServiceRole::Active) {
                    return Err(mapping(
                        "source role disagrees with exact source declarations",
                    ));
                }
                Ok(())
            };
        for t in &m.storages {
            if let Some(s) = t.source() {
                check_role(&ServiceScope::Storage(s.storage_id.clone()), s.role)?;
            }
            if source.has(&ServiceScope::Storage(t.target().clone()))
                && t.source().is_none_or(|s| &s.storage_id != t.target())
            {
                return Err(mapping(
                    "an existing storage target must be consumed by its own mapping",
                ));
            }
        }
        let h = target
            .hooks()
            .get(&ServiceHookSite::Migration(digest.clone()));
        if let Some(h) = h {
            for r in h
                .access
                .iter()
                .map(|a| &a.reference)
                .chain(h.requires.iter().map(|p| &p.reference))
            {
                if r.view == ServiceView::Source {
                    check_role(&r.scope, r.role)?;
                }
            }
            if let ServiceSourceCore::V2(source) = source {
                for grant in &h.access {
                    if grant.reference.view != ServiceView::Source
                        || grant.reference.role != ServiceRole::Active
                    {
                        continue;
                    }
                    let ServiceScope::Resource(id) = &grant.reference.scope else {
                        continue;
                    };
                    let resource = source.resource(id).expect("source role was checked");
                    if h.access.iter().any(|s| {
                        s.reference.view == ServiceView::Source
                            && s.reference.role == ServiceRole::Active
                            && s.reference.scope
                                == ServiceScope::Storage(resource.storage_id.clone())
                            && s.mode.covers(grant.mode)
                    }) {
                        return Err(invalid("redundant source whole-storage/resource grant"));
                    }
                }
            }
        }
        for t in &m.resources {
            for s in t.sources() {
                check_role(&ServiceScope::Resource(s.resource_id.clone()), s.role)?;
            }
            for id in t.targets() {
                if source.has(&ServiceScope::Resource(id.clone()))
                    && !t.sources().iter().any(|s| &s.resource_id == id)
                {
                    return Err(mapping(
                        "an existing resource target must be consumed by its own mapping",
                    ));
                }
                let destination = target.resource(id).expect("validated target");
                let storage = m
                    .storages
                    .iter()
                    .find(|s| s.target() == &destination.storage_id)
                    .expect("validated storage coverage");
                if let Some(origin) = storage.source().filter(|s| s.role == ServiceRole::Active) {
                    for old in source.resources() {
                        if old.storage_id == origin.storage_id
                            && old.locator.portable_key() == destination.locator.portable_key()
                            && !t
                                .sources()
                                .iter()
                                .any(|s| s.role == ServiceRole::Active && s.resource_id == old.id)
                        {
                            return Err(mapping(
                                "target location already belongs to an unconsumed source resource",
                            ));
                        }
                    }
                }
            }
            match t {
                ResourceTransitionV2::Reuse {
                    source: s,
                    target_resource_id,
                } => {
                    let old = source
                        .resources()
                        .iter()
                        .find(|r| r.id == s.resource_id)
                        .expect("active source was checked");
                    let new = target
                        .resource(target_resource_id)
                        .expect("validated target");
                    let storage = m
                        .storages
                        .iter()
                        .find(|s| s.target() == &new.storage_id)
                        .expect("storage coverage");
                    if old.kind != new.kind || old.locator != new.locator {
                        return Err(mapping("resource reuse changes kind or locator"));
                    }
                    if !storage.source().is_some_and(|s| {
                        s.role == ServiceRole::Active && s.storage_id == old.storage_id
                    }) {
                        // A retained origin requires Instance facts. A freshly
                        // created root definitely cannot be the same allocation.
                        if matches!(storage, StorageTransitionV2::Create { .. })
                            || storage
                                .source()
                                .is_some_and(|s| s.role == ServiceRole::Active)
                        {
                            return Err(mapping(
                                "resource reuse changes its storage allocation origin",
                            ));
                        }
                    }
                }
                ResourceTransitionV2::Transform { sources: refs, .. } => {
                    let h = h.expect("transform Hook checked intrinsically");
                    for s in refs.iter().filter(|s| s.role == ServiceRole::Active) {
                        let ServiceSourceCore::V2(source) = source else {
                            return Err(mapping("V1 source has no active service resource"));
                        };
                        let reference = ServiceReferenceV2 {
                            view: ServiceView::Source,
                            role: s.role,
                            scope: ServiceScope::Resource(s.resource_id.clone()),
                        };
                        if !has_resource_authority(source, h, &reference, ServiceAccessMode::Read) {
                            return Err(mapping("transform lacks source read authority"));
                        }
                    }
                    for s in refs.iter().filter(|s| s.role == ServiceRole::Retained) {
                        let exact = h.access.iter().any(|g| {
                            g.reference.view == ServiceView::Source
                                && g.reference.role == ServiceRole::Retained
                                && g.reference.scope
                                    == ServiceScope::Resource(s.resource_id.clone())
                        });
                        let indirect = h.access.iter().any(|g| {
                            g.reference.view == ServiceView::Source
                                && matches!(
                                    g.reference.scope,
                                    ServiceScope::Storage(_) | ServiceScope::Resource(_)
                                )
                        });
                        if !exact && !indirect {
                            return Err(mapping(
                                "transform lacks any potential retained source authority",
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(if all {
        super::RelationalValidationV1::Valid
    } else {
        super::RelationalValidationV1::NotEvaluated
    })
}
