//! Codec-only operation contract. Installed/runtime readers remain fail-closed.
#![allow(dead_code)]
use super::*;

fn ids(v: Raw) -> Result<Vec<InputIdentity>, Error> {
    array(v, |v| match v {
        Raw::String(id) => InputIdentity::parse(id).map_err(Error::from),
        _ => Err(error("invalid_type", "expected Input identity")),
    })
}
fn references(v: Raw) -> Result<Vec<InputBindingRefV1>, Error> {
    array(v, |v| {
        let mut o = object(v, &["role", "input_id"], &[])?;
        Ok(InputBindingRefV1 {
            role: match string(&mut o, "role")?.as_str() {
                "active" => InputBindingRoleV1::Active,
                "retained" => InputBindingRoleV1::Retained,
                _ => return Err(enum_error()),
            },
            input_id: InputIdentity::parse(string(&mut o, "input_id")?)?,
        })
    })
}
fn uses<T>(v: Raw, parse: impl Fn(Raw) -> Result<Vec<T>, Error>) -> Result<InputUse<T>, Error> {
    let mut o = object(v, &["required", "optional"], &[])?;
    Ok(InputUse {
        required: parse(value(&mut o, "required")?)?,
        optional: parse(value(&mut o, "optional")?)?,
    })
}
fn fields(v: Raw) -> Result<Object, Error> {
    match v {
        Raw::Object(o) => Ok(o.into_iter().collect()),
        _ => Err(error("invalid_type", "expected operation object")),
    }
}
fn forbid(o: &Object, keys: &[&str]) -> Result<(), Error> {
    if keys.iter().any(|key| o.contains_key(*key)) {
        return Err(error(
            "unknown_field",
            "old requirement field is not valid in operation Core",
        ));
    }
    Ok(())
}
fn raw_references(value: &impl serde::Serialize) -> Result<Raw, Error> {
    raw(&serde_json::to_vec(value)
        .map_err(|e| Error::new("canonicalization_failed", e.to_string()))?)
}
fn insert_use(
    map: &mut BTreeMap<ServiceHookSite, OperationInputUse>,
    site: ServiceHookSite,
    inputs: OperationInputUse,
) -> Result<(), Error> {
    if map.insert(site, inputs).is_some() {
        return Err(error(
            "duplicate_semantic_key",
            "duplicate operation identity",
        ));
    }
    Ok(())
}

fn extra(op: &mut Object, parameters: bool) -> Result<OperationCapabilities, Error> {
    Ok(OperationCapabilities {
        parameters: if parameters {
            array(value(op, "parameters")?, |v| {
                revision_declarations::parse_parameter(v).map_err(Error::from)
            })?
        } else {
            Vec::new()
        },
        outputs: array(value(op, "outputs")?, |v| {
            revision_declarations::parse_output(v).map_err(Error::from)
        })?,
    })
}

pub(crate) fn project(bytes: &[u8]) -> Result<OperationRevision, Error> {
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
    if string(&mut o, "format_version")? != OPERATION_CORE_FORMAT {
        return Err(error(
            "unsupported_format_version",
            "expected operation Core 1.0-alpha.2",
        ));
    }
    let inputs = array(value(&mut o, "inputs")?, |v| {
        let mut input = object(v, &["id", "protection"], &[])?;
        // Shared structural validation only. This flag never enters the new model.
        input.insert("required".into(), Raw::Bool(false));
        Ok(raw_object(input))
    })?;
    o.insert("inputs".into(), Raw::Array(inputs));
    let mut grants = BTreeMap::new();
    let mut capabilities = BTreeMap::new();
    let actions = array(value(&mut o, "actions")?, |v| {
        let mut action = fields(v)?;
        let id = ActionIdentity::parse(peek_string(&action, "id")?)?;
        let inputs = uses(value(&mut action, "inputs")?, ids)?;
        insert_use(
            &mut grants,
            ServiceHookSite::Action(id),
            OperationInputUse::Active(inputs),
        )?;
        Ok(raw_object(action))
    })?;
    o.insert("actions".into(), Raw::Array(actions));
    if let Some(snapshot) = o.remove("snapshot") {
        let mut snapshot = object(snapshot, &[], &["capture", "restore"])?;
        for (key, site) in [
            ("capture", ServiceHookSite::Capture),
            ("restore", ServiceHookSite::Restore),
        ] {
            if let Some(op) = snapshot.remove(key) {
                let mut op = fields(op)?;
                capabilities.insert(site.clone(), extra(&mut op, false)?);
                let inputs = uses(value(&mut op, "inputs")?, ids)?;
                insert_use(&mut grants, site, OperationInputUse::Active(inputs))?;
                snapshot.insert(key.into(), raw_object(op));
            }
        }
        o.insert("snapshot".into(), raw_object(snapshot));
    }
    let migrations = array(value(&mut o, "migrations")?, |v| {
        let mut migration = fields(v)?;
        forbid(&migration, &["requires_source", "requires_target"])?;
        let digest = Sha256Digest::parse(peek_string(&migration, "source_revision_digest")?)?;
        let site = ServiceHookSite::Migration(digest.clone());
        if capabilities
            .insert(site, extra(&mut migration, true)?)
            .is_some()
        {
            return Err(error(
                "duplicate_semantic_key",
                "duplicate Migration capabilities",
            ));
        }
        let source = uses(value(&mut migration, "source_inputs")?, references)?;
        let target = uses(value(&mut migration, "target_inputs")?, ids)?;
        migration.insert("requires_source".into(), raw_references(&source.all())?);
        migration.insert("requires_target".into(), raw_references(&target.all())?);
        insert_use(
            &mut grants,
            ServiceHookSite::Migration(digest),
            OperationInputUse::Migration { source, target },
        )?;
        Ok(raw_object(migration))
    })?;
    o.insert("migrations".into(), Raw::Array(migrations));
    if let Some(cleanup) = o.remove("cleanup") {
        let mut cleanup = fields(cleanup)?;
        capabilities.insert(ServiceHookSite::Cleanup, extra(&mut cleanup, true)?);
        forbid(&cleanup, &["requires"])?;
        let inputs = uses(value(&mut cleanup, "inputs")?, references)?;
        cleanup.insert("requires".into(), raw_references(&inputs.all())?);
        insert_use(
            &mut grants,
            ServiceHookSite::Cleanup,
            OperationInputUse::Binding(inputs),
        )?;
        o.insert("cleanup".into(), raw_object(cleanup));
    }
    let shape = project_service_fields(o, operation_hook_version())?;
    project_operation_revision(shape, grants, capabilities)
}

pub(crate) fn encode(core: &OperationRevision) -> Result<Vec<u8>, Error> {
    let mut encoded = json(core)?;
    add_service_operations(
        encoded.as_object_mut().expect("typed operation Core"),
        core.service_hooks(),
        core.service_migrations(),
    )?;
    serde_jcs::to_vec(&encoded).map_err(|e| Error::new("canonicalization_failed", e.to_string()))
}
pub(crate) fn decode(bytes: &[u8]) -> Result<OperationRevision, Error> {
    let core = project(bytes)?;
    if encode(&core)? != bytes {
        return Err(error(
            "noncanonical_json",
            "operation Core bytes are not exact normalized JCS",
        ));
    }
    Ok(core)
}
pub(crate) fn decode_content(
    core: &[u8],
    runtime: &[u8],
) -> Result<OperationRevisionContent, Error> {
    validate_operation_content(
        decode(core)?,
        revision_declarations::decode_canonical_runtime_content(runtime)?,
    )
}
pub(crate) fn digest(content: &OperationRevisionContent) -> Result<RevisionContentDigest, Error> {
    let core = encode(content.core())?;
    let runtime =
        revision_declarations::encode_canonical_runtime_content(content.runtime_content())?;
    Ok(RevisionContentDigest::from_bytes(
        Sha256::digest(frame_revision_content(&core, &runtime)).into(),
    ))
}

#[cfg(test)]
mod tests;
