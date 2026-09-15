//! Core V2 codec. Strict raw numbers go to the existing typed common-field
//! validator before any JSON Value conversion. Domain owns service policy.

use crate::{
    domain::*,
    revision_core_v1,
    strict_json::{self, RawJsonValue as Raw, StrictJsonErrorKind},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
type Error = ServiceContractError;
type Object = BTreeMap<String, Raw>;

fn error(code: &str, message: &str) -> Error {
    Error::new(code, message)
}
fn raw(bytes: &[u8]) -> Result<Raw, Error> {
    strict_json::parse_json(bytes, 16 * 1024 * 1024).map_err(|e| {
        Error::new(
            match e.kind() {
                StrictJsonErrorKind::DuplicateProperty => "duplicate_property",
                StrictJsonErrorKind::InvalidUtf8 | StrictJsonErrorKind::InvalidUnicodeScalar => {
                    "invalid_unicode_scalar"
                }
                StrictJsonErrorKind::InvalidJson => "invalid_json_syntax",
            },
            e.message(),
        )
    })
}
fn object(value: Raw, required: &[&str], optional: &[&str]) -> Result<Object, Error> {
    let Raw::Object(entries) = value else {
        return Err(error("invalid_type", "expected object"));
    };
    let map: Object = entries.into_iter().collect();
    for key in map.keys() {
        if !required.contains(&key.as_str()) && !optional.contains(&key.as_str()) {
            return Err(Error::new("unknown_field", format!("unknown field {key}")));
        }
    }
    for key in required {
        if !map.contains_key(*key) {
            return Err(Error::new("missing_field", format!("missing field {key}")));
        }
    }
    Ok(map)
}
fn value(map: &mut Object, key: &str) -> Result<Raw, Error> {
    map.remove(key)
        .ok_or_else(|| Error::new("missing_field", format!("missing field {key}")))
}
fn string(map: &mut Object, key: &str) -> Result<String, Error> {
    match value(map, key)? {
        Raw::String(s) => Ok(s),
        _ => Err(error("invalid_type", "expected string")),
    }
}
fn peek_string(map: &Object, key: &str) -> Result<String, Error> {
    match map.get(key) {
        Some(Raw::String(s)) => Ok(s.clone()),
        _ => Err(error("invalid_type", "expected identity string")),
    }
}
fn array<T>(v: Raw, parse: impl FnMut(Raw) -> Result<T, Error>) -> Result<Vec<T>, Error> {
    let Raw::Array(items) = v else {
        return Err(error("invalid_type", "expected array"));
    };
    items.into_iter().map(parse).collect()
}
fn raw_object(map: Object) -> Raw {
    Raw::Object(map.into_iter().collect())
}
fn enum_error() -> Error {
    error("invalid_enum", "unknown closed enum token")
}
fn role(s: &str) -> Result<ServiceRole, Error> {
    match s {
        "active" => Ok(ServiceRole::Active),
        "retained" => Ok(ServiceRole::Retained),
        _ => Err(enum_error()),
    }
}

fn parse_storage(v: Raw) -> Result<ServiceStorageV2, Error> {
    let mut o = object(v, &["id"], &[])?;
    Ok(ServiceStorageV2 {
        id: ServiceStorageIdentity::parse(string(&mut o, "id")?)?,
    })
}
fn parse_resource(v: Raw) -> Result<ServiceResourceV2, Error> {
    let mut o = object(
        v,
        &[
            "id",
            "storage_id",
            "locator",
            "kind",
            "read_exposure",
            "user_mutation",
        ],
        &[],
    )?;
    Ok(ServiceResourceV2 {
        id: ServiceResourceIdentity::parse(string(&mut o, "id")?)?,
        storage_id: ServiceStorageIdentity::parse(string(&mut o, "storage_id")?)?,
        locator: ServiceLocatorV2::parse(string(&mut o, "locator")?)?,
        kind: match string(&mut o, "kind")?.as_str() {
            "file" => ServiceResourceKind::File,
            "directory" => ServiceResourceKind::Directory,
            _ => return Err(enum_error()),
        },
        read_exposure: match string(&mut o, "read_exposure")?.as_str() {
            "hidden" => ServiceReadExposure::Hidden,
            "readable" => ServiceReadExposure::Readable,
            _ => return Err(enum_error()),
        },
        user_mutation: parse_mutation(value(&mut o, "user_mutation")?)?,
    })
}
fn parse_mutation(v: Raw) -> Result<ServiceUserMutation, Error> {
    let mut o = object(v, &["kind"], &["action_id"])?;
    let result = match string(&mut o, "kind")?.as_str() {
        "unavailable" => ServiceUserMutation::Unavailable,
        "direct" => ServiceUserMutation::Direct,
        "operation" => ServiceUserMutation::Operation {
            action_id: ActionIdentity::parse(string(&mut o, "action_id")?)?,
        },
        _ => return Err(enum_error()),
    };
    if !o.is_empty() {
        return Err(error("unknown_field", "unexpected mutation variant field"));
    }
    Ok(result)
}
fn parse_reference(v: Raw) -> Result<ServiceReferenceV2, Error> {
    let mut o = object(v, &["view", "role", "kind", "id"], &[])?;
    let view = match string(&mut o, "view")?.as_str() {
        "current" => ServiceView::Current,
        "source" => ServiceView::Source,
        "target" => ServiceView::Target,
        _ => return Err(enum_error()),
    };
    let role = role(&string(&mut o, "role")?)?;
    let id = string(&mut o, "id")?;
    let scope = match string(&mut o, "kind")?.as_str() {
        "storage" => ServiceScope::Storage(ServiceStorageIdentity::parse(id)?),
        "resource" => ServiceScope::Resource(ServiceResourceIdentity::parse(id)?),
        _ => return Err(enum_error()),
    };
    Ok(ServiceReferenceV2 { view, role, scope })
}
fn parse_access(v: Raw) -> Result<ServiceAccessV2, Error> {
    let mut o = object(v, &["reference", "mode"], &[])?;
    let reference = parse_reference(value(&mut o, "reference")?)?;
    let mode = match string(&mut o, "mode")?.as_str() {
        "read" => ServiceAccessMode::Read,
        "write" => ServiceAccessMode::Write,
        _ => return Err(enum_error()),
    };
    Ok(ServiceAccessV2 { reference, mode })
}
fn parse_prerequisite(v: Raw) -> Result<ServicePrerequisiteV2, Error> {
    let mut o = object(v, &["reference", "presence"], &[])?;
    let reference = parse_reference(value(&mut o, "reference")?)?;
    let presence = match string(&mut o, "presence")?.as_str() {
        "present" => ServicePresenceRequirement::Present,
        "absent" => ServicePresenceRequirement::Absent,
        _ => return Err(enum_error()),
    };
    Ok(ServicePrerequisiteV2 {
        reference,
        presence,
    })
}
fn parse_storage_source(v: Raw) -> Result<StorageSourceV2, Error> {
    let mut o = object(v, &["role", "storage_id"], &[])?;
    Ok(StorageSourceV2 {
        role: role(&string(&mut o, "role")?)?,
        storage_id: ServiceStorageIdentity::parse(string(&mut o, "storage_id")?)?,
    })
}
fn parse_resource_source(v: Raw) -> Result<ResourceSourceV2, Error> {
    let mut o = object(v, &["role", "resource_id"], &[])?;
    Ok(ResourceSourceV2 {
        role: role(&string(&mut o, "role")?)?,
        resource_id: ServiceResourceIdentity::parse(string(&mut o, "resource_id")?)?,
    })
}
fn parse_storage_transition(v: Raw) -> Result<StorageTransitionV2, Error> {
    let mut o = object(v, &["kind", "target_storage_id"], &["source"])?;
    let target_storage_id = ServiceStorageIdentity::parse(string(&mut o, "target_storage_id")?)?;
    let result = match string(&mut o, "kind")?.as_str() {
        "create" => StorageTransitionV2::Create { target_storage_id },
        "reuse" => StorageTransitionV2::Reuse {
            source: parse_storage_source(value(&mut o, "source")?)?,
            target_storage_id,
        },
        "reattach" => StorageTransitionV2::Reattach {
            source: parse_storage_source(value(&mut o, "source")?)?,
            target_storage_id,
        },
        _ => return Err(enum_error()),
    };
    if !o.is_empty() {
        return Err(error(
            "unknown_field",
            "unexpected storage transition variant field",
        ));
    }
    Ok(result)
}
fn parse_resource_transition(v: Raw) -> Result<ResourceTransitionV2, Error> {
    let mut o = object(
        v,
        &["kind"],
        &[
            "source",
            "sources",
            "targets",
            "target_resource_id",
            "presence",
        ],
    )?;
    let result = match string(&mut o, "kind")?.as_str() {
        "create" => ResourceTransitionV2::Create {
            target_resource_id: ServiceResourceIdentity::parse(string(
                &mut o,
                "target_resource_id",
            )?)?,
            presence: match string(&mut o, "presence")?.as_str() {
                "any" => ServiceCreatePresence::Any,
                "present" => ServiceCreatePresence::Present,
                "absent" => ServiceCreatePresence::Absent,
                _ => return Err(enum_error()),
            },
        },
        "reuse" => ResourceTransitionV2::Reuse {
            source: parse_resource_source(value(&mut o, "source")?)?,
            target_resource_id: ServiceResourceIdentity::parse(string(
                &mut o,
                "target_resource_id",
            )?)?,
        },
        "reattach" => ResourceTransitionV2::Reattach {
            source: parse_resource_source(value(&mut o, "source")?)?,
            target_resource_id: ServiceResourceIdentity::parse(string(
                &mut o,
                "target_resource_id",
            )?)?,
        },
        "transform" => ResourceTransitionV2::Transform {
            sources: array(value(&mut o, "sources")?, parse_resource_source)?,
            targets: array(value(&mut o, "targets")?, |v| match v {
                Raw::String(s) => ServiceResourceIdentity::parse(s),
                _ => Err(error("invalid_type", "expected resource target identity")),
            })?,
        },
        _ => return Err(enum_error()),
    };
    if !o.is_empty() {
        return Err(error(
            "unknown_field",
            "unexpected resource transition variant field",
        ));
    }
    Ok(result)
}

fn split_hook(
    v: Raw,
    site: ServiceHookSite,
    hooks: &mut BTreeMap<ServiceHookSite, HookServiceContractV2>,
) -> Result<Raw, Error> {
    let mut o = object(
        v,
        &[
            "protocol_version",
            "launch",
            "args",
            "io",
            "service_access",
            "service_requires",
        ],
        &[],
    )?;
    let h = HookServiceContractV2 {
        access: array(value(&mut o, "service_access")?, parse_access)?,
        requires: array(value(&mut o, "service_requires")?, parse_prerequisite)?,
    };
    if hooks.insert(site, h).is_some() {
        return Err(error("duplicate_semantic_key", "duplicate Hook owner"));
    }
    Ok(raw_object(o))
}

pub(crate) fn project_revision_core_source_v2(bytes: &[u8]) -> Result<RevisionCoreV2, Error> {
    let mut o = object(
        raw(bytes)?,
        &[
            "format_version",
            "inputs",
            "actions",
            "migrations",
            "service_storages",
            "service_resources",
        ],
        &["snapshot", "cleanup"],
    )?;
    if revision_core_v1::exact_integer(value(&mut o, "format_version")?)? != 2 {
        return Err(error(
            "unsupported_format_version",
            "Core V2 requires format version 2",
        ));
    }
    let storages = array(value(&mut o, "service_storages")?, parse_storage)?;
    let resources = array(value(&mut o, "service_resources")?, parse_resource)?;
    let mut hooks = BTreeMap::new();
    let actions = array(value(&mut o, "actions")?, |v| {
        let mut a = object(v, &["id", "access", "parameters", "hook", "outputs"], &[])?;
        let id = ActionIdentity::parse(peek_string(&a, "id")?)?;
        let hook = split_hook(
            value(&mut a, "hook")?,
            ServiceHookSite::Action(id),
            &mut hooks,
        )?;
        a.insert("hook".into(), hook);
        Ok(raw_object(a))
    })?;
    o.insert("actions".into(), Raw::Array(actions));
    if let Some(snapshot) = o.remove("snapshot") {
        let mut s = object(snapshot, &[], &["capture", "restore"])?;
        for (name, site, required) in [
            (
                "capture",
                ServiceHookSite::Capture,
                &["parameters", "access", "hook"][..],
            ),
            (
                "restore",
                ServiceHookSite::Restore,
                &["parameters", "hook"][..],
            ),
        ] {
            if let Some(v) = s.remove(name) {
                let mut c = object(v, required, &[])?;
                let h = split_hook(value(&mut c, "hook")?, site, &mut hooks)?;
                c.insert("hook".into(), h);
                s.insert(name.into(), raw_object(c));
            }
        }
        o.insert("snapshot".into(), raw_object(s));
    }
    if let Some(cleanup) = o.remove("cleanup") {
        let mut c = object(cleanup, &["requires", "hook"], &[])?;
        let h = split_hook(value(&mut c, "hook")?, ServiceHookSite::Cleanup, &mut hooks)?;
        c.insert("hook".into(), h);
        o.insert("cleanup".into(), raw_object(c));
    }
    let mut mappings = BTreeMap::new();
    let migrations = array(value(&mut o, "migrations")?, |v| {
        let mut m = object(
            v,
            &[
                "source_revision_digest",
                "transitions",
                "requires_source",
                "requires_target",
                "produces_target",
                "storage_transitions",
                "resource_transitions",
            ],
            &["hook"],
        )?;
        let source = Sha256Digest::parse(peek_string(&m, "source_revision_digest")?)?;
        let mapping = ServiceMigrationV2 {
            storages: array(
                value(&mut m, "storage_transitions")?,
                parse_storage_transition,
            )?,
            resources: array(
                value(&mut m, "resource_transitions")?,
                parse_resource_transition,
            )?,
        };
        if mappings.insert(source.clone(), mapping).is_some() {
            return Err(error("duplicate_semantic_key", "duplicate Migration"));
        }
        if let Some(h) = m.remove("hook") {
            m.insert(
                "hook".into(),
                split_hook(h, ServiceHookSite::Migration(source), &mut hooks)?,
            );
        }
        Ok(raw_object(m))
    })?;
    o.insert("migrations".into(), Raw::Array(migrations));
    o.insert("format_version".into(), Raw::Number("1".into()));
    let common = revision_core_v1::project_core_value_v1(raw_object(o))?;
    project_revision_core_v2(common, storages, resources, hooks, mappings)
}

pub(crate) fn encode_canonical_revision_core_v2(core: &RevisionCoreV2) -> Result<Vec<u8>, Error> {
    let mut value = serde_json::to_value(core.common())
        .map_err(|e| Error::new("canonicalization_failed", e.to_string()))?;
    let root = value.as_object_mut().expect("typed Core object");
    root.insert("format_version".into(), 2.into());
    root.insert("service_storages".into(), json(core.storages())?);
    root.insert("service_resources".into(), json(core.resources())?);
    for action in root["actions"].as_array_mut().expect("typed actions") {
        let id =
            ActionIdentity::parse(action["id"].as_str().expect("typed id")).expect("validated id");
        add_hook(
            &mut action["hook"],
            &core.hooks()[&ServiceHookSite::Action(id)],
        )?;
    }
    if let Some(snapshot) = root.get_mut("snapshot") {
        for (name, site) in [
            ("capture", ServiceHookSite::Capture),
            ("restore", ServiceHookSite::Restore),
        ] {
            if let Some(operation) = snapshot.get_mut(name) {
                add_hook(&mut operation["hook"], &core.hooks()[&site])?;
            }
        }
    }
    if let Some(cleanup) = root.get_mut("cleanup") {
        add_hook(
            &mut cleanup["hook"],
            &core.hooks()[&ServiceHookSite::Cleanup],
        )?;
    }
    for migration in root["migrations"].as_array_mut().expect("typed migrations") {
        let digest = Sha256Digest::parse(
            migration["source_revision_digest"]
                .as_str()
                .expect("typed digest"),
        )
        .expect("validated digest");
        let m = &core.migrations()[&digest];
        let object = migration.as_object_mut().expect("typed migration");
        object.insert("storage_transitions".into(), json(&m.storages)?);
        object.insert("resource_transitions".into(), json(&m.resources)?);
        if let Some(h) = object.get_mut("hook") {
            add_hook(h, &core.hooks()[&ServiceHookSite::Migration(digest)])?;
        }
    }
    serde_jcs::to_vec(&value).map_err(|e| Error::new("canonicalization_failed", e.to_string()))
}
fn json(value: &(impl serde::Serialize + ?Sized)) -> Result<serde_json::Value, Error> {
    serde_json::to_value(value).map_err(|e| Error::new("canonicalization_failed", e.to_string()))
}
fn add_hook(value: &mut serde_json::Value, h: &HookServiceContractV2) -> Result<(), Error> {
    let object = value.as_object_mut().expect("typed Hook");
    object.insert("service_access".into(), json(&h.access)?);
    object.insert("service_requires".into(), json(&h.requires)?);
    Ok(())
}
pub(crate) fn decode_canonical_revision_core_v2(bytes: &[u8]) -> Result<RevisionCoreV2, Error> {
    let core = project_revision_core_source_v2(bytes)?;
    if encode_canonical_revision_core_v2(&core)? != bytes {
        return Err(error(
            "noncanonical_json",
            "Core V2 bytes are not exact JCS",
        ));
    }
    Ok(core)
}
pub(crate) fn validate_revision_content_v2(
    core: RevisionCoreV2,
    runtime_content: RuntimeContentClosureIdentityV1,
) -> Result<ValidatedRevisionContentV2, Error> {
    project_revision_content_v2(core, runtime_content)
}
pub(crate) fn decode_canonical_revision_content_v2(
    core: &[u8],
    content: &[u8],
) -> Result<ValidatedRevisionContentV2, Error> {
    validate_revision_content_v2(
        decode_canonical_revision_core_v2(core)?,
        revision_core_v1::decode_canonical_runtime_content_v1(content)?,
    )
}
pub(crate) fn frame_revision_content_v2(core: &[u8], content: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"pactrun.revision-content-digest\0");
    out.extend_from_slice(&2_u32.to_be_bytes());
    out.extend_from_slice(b"revision-core\0");
    out.extend_from_slice(&(core.len() as u64).to_be_bytes());
    out.extend_from_slice(core);
    out.extend_from_slice(b"runtime-content-closure\0");
    out.extend_from_slice(&(content.len() as u64).to_be_bytes());
    out.extend_from_slice(content);
    out
}
pub(crate) fn calculate_revision_content_digest_v2(
    content: &ValidatedRevisionContentV2,
) -> Result<RevisionContentDigest, Error> {
    let core = encode_canonical_revision_core_v2(&content.core)?;
    let runtime = revision_core_v1::encode_canonical_runtime_content_v1(&content.runtime_content)?;
    Ok(RevisionContentDigest::from_bytes(
        Sha256::digest(frame_revision_content_v2(&core, &runtime)).into(),
    ))
}

#[cfg(test)]
#[path = "revision_core_v2_tests.rs"]
mod tests;
