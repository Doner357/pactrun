//! Closed incoming Session validation and granted-view selection.
use super::*;

fn object(value: &Value, required: &[&str], optional: &[&str]) -> io::Result<()> {
    let o = value.as_object().ok_or_else(failure)?;
    if required.iter().any(|k| !o.contains_key(*k))
        || o.keys()
            .any(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
    {
        return Err(failure());
    }
    Ok(())
}
fn one_of(value: &Value, choices: &[&str]) -> io::Result<()> {
    if value.as_str().is_some_and(|s| choices.contains(&s)) {
        Ok(())
    } else {
        Err(failure())
    }
}
fn id(value: &Value) -> io::Result<()> {
    let text = value.as_str().ok_or_else(failure)?;
    crate::domain::ContentId::parse(text)
        .map(|_| ())
        .map_err(|_| failure())
}
fn opaque(value: &Value) -> io::Result<()> {
    if value.as_str().is_some_and(|s| {
        s.len() == 32
            && s.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    }) {
        Ok(())
    } else {
        Err(failure())
    }
}
fn absolute(value: &Value) -> io::Result<()> {
    if value
        .as_str()
        .is_some_and(|s| !s.contains('\0') && Path::new(s).is_absolute())
    {
        Ok(())
    } else {
        Err(failure())
    }
}
fn array(value: &Value) -> io::Result<&Vec<Value>> {
    value.as_array().ok_or_else(failure)
}
fn revision(value: &Value) -> io::Result<()> {
    object(value, &["package_id", "revision_content_digest"], &[])?;
    opaque(&value["package_id"])?;
    crate::domain::Sha256Digest::parse(
        value["revision_content_digest"]
            .as_str()
            .ok_or_else(failure)?,
    )
    .map(|_| ())
    .map_err(|_| failure())
}
fn handle(value: &Value, handles: &mut BTreeSet<String>) -> io::Result<()> {
    opaque(value)?;
    if handles.insert(value.as_str().expect("validated handle").to_owned()) {
        Ok(())
    } else {
        Err(failure())
    }
}
fn bindings(value: &Value, retained: bool, handles: &mut BTreeSet<String>) -> io::Result<()> {
    let mut previous = None;
    for binding in array(value)? {
        object(
            binding,
            &["handle", "input_id", "role", "readonly_path"],
            &[],
        )?;
        handle(&binding["handle"], handles)?;
        id(&binding["input_id"])?;
        absolute(&binding["readonly_path"])?;
        one_of(
            &binding["role"],
            if retained {
                &["active", "retained"]
            } else {
                &["active"]
            },
        )?;
        let key = (
            binding["role"].as_str().unwrap(),
            binding["input_id"].as_str().unwrap(),
        );
        if previous.is_some_and(|p| p >= key) {
            return Err(failure());
        }
        previous = Some(key);
    }
    Ok(())
}

pub(super) fn validate(value: &Value, version: u32) -> io::Result<()> {
    let mut fields = vec![
        "type",
        "protocol_version",
        "session_id",
        "run_id",
        "revision",
        "parameters",
        "workspace",
        "io",
        "operation",
    ];
    if version == 2 {
        fields.push("service_authorities");
    }
    object(
        value,
        &fields,
        if version == 2 {
            &["target_commit"]
        } else {
            &[]
        },
    )?;
    if value["type"] != "session_start"
        || positive_integer(&value["protocol_version"]) != Some(u64::from(version))
    {
        return Err(failure());
    }
    opaque(&value["session_id"])?;
    opaque(&value["run_id"])?;
    revision(&value["revision"])?;
    let mut handles = BTreeSet::new();
    object(&value["workspace"], &["handle", "root_path"], &[])?;
    handle(&value["workspace"]["handle"], &mut handles)?;
    absolute(&value["workspace"]["root_path"])?;
    object(&value["io"], &["terminal"], &[])?;
    one_of(&value["io"]["terminal"], &["none", "output", "interactive"])?;
    let mut previous = None;
    for parameter in array(&value["parameters"])? {
        object(parameter, &["parameter_id", "value"], &[])?;
        id(&parameter["parameter_id"])?;
        let key = parameter["parameter_id"].as_str().unwrap();
        if previous.is_some_and(|p| p >= key) {
            return Err(failure());
        }
        previous = Some(key);
        if !matches!(
            parameter["value"],
            Value::String(_) | Value::Bool(_) | Value::Number(_)
        ) {
            return Err(failure());
        }
    }
    let op = &value["operation"];
    match op["kind"].as_str() {
        Some("action") => {
            object(
                op,
                &["kind", "action_id", "access", "bindings", "outputs"],
                &[],
            )?;
            id(&op["action_id"])?;
            one_of(&op["access"], &["observe", "mutate"])?;
            bindings(&op["bindings"], false, &mut handles)?;
        }
        Some("migration") => {
            object(
                op,
                &[
                    "kind",
                    "source_revision",
                    "source_bindings",
                    "target_bindings",
                    "target_outputs",
                ],
                &[],
            )?;
            revision(&op["source_revision"])?;
            bindings(&op["source_bindings"], true, &mut handles)?;
            bindings(&op["target_bindings"], false, &mut handles)?;
        }
        Some("snapshot_capture") => {
            object(op, &["kind", "access", "bindings", "candidate"], &[])?;
            one_of(&op["access"], &["observe", "mutate"])?;
            bindings(&op["bindings"], false, &mut handles)?;
            object(&op["candidate"], &["handle", "root_path"], &[])?;
            handle(&op["candidate"]["handle"], &mut handles)?;
            absolute(&op["candidate"]["root_path"])?;
        }
        Some("snapshot_restore") => {
            object(
                op,
                &["kind", "snapshot_id", "bindings", "snapshot_content"],
                &[],
            )?;
            opaque(&op["snapshot_id"])?;
            bindings(&op["bindings"], false, &mut handles)?;
            let content = &op["snapshot_content"];
            object(
                content,
                &["handle", "readonly_root_path", "logical_descriptors"],
                &[],
            )?;
            handle(&content["handle"], &mut handles)?;
            absolute(&content["readonly_root_path"])?;
            let mut keys = BTreeSet::new();
            for descriptor in array(&content["logical_descriptors"])? {
                object(
                    descriptor,
                    &["role", "path", "blob_digest", "materialized_path"],
                    &[],
                )?;
                crate::domain::SnapshotServiceRole::parse(
                    descriptor["role"].as_str().ok_or_else(failure)?,
                )
                .map_err(|_| failure())?;
                crate::domain::SnapshotContentPath::parse(
                    descriptor["path"].as_str().ok_or_else(failure)?,
                )
                .map_err(|_| failure())?;
                crate::domain::Sha256Digest::parse(
                    descriptor["blob_digest"].as_str().ok_or_else(failure)?,
                )
                .map_err(|_| failure())?;
                crate::domain::RuntimePath::parse(
                    descriptor["materialized_path"]
                        .as_str()
                        .ok_or_else(failure)?,
                )
                .map_err(|_| failure())?;
                if !keys.insert((descriptor["role"].as_str(), descriptor["path"].as_str())) {
                    return Err(failure());
                }
            }
        }
        Some("cleanup") => {
            object(op, &["kind", "bindings"], &[])?;
            bindings(&op["bindings"], true, &mut handles)?;
        }
        _ => return Err(failure()),
    }
    let key = if op["kind"] == "migration" {
        "input_id"
    } else {
        "output_id"
    };
    let mut output_ids = BTreeSet::new();
    for output in outputs(value) {
        object(output, &["handle", key, "staged_path"], &[])?;
        handle(&output["handle"], &mut handles)?;
        id(&output[key])?;
        absolute(&output["staged_path"])?;
        let key = output[key].as_str().unwrap();
        if !output_ids.insert(key) {
            return Err(failure());
        }
    }
    if op["kind"] == "action" {
        array(&op["outputs"])?;
    }
    if op["kind"] == "migration" {
        array(&op["target_outputs"])?;
    }
    if version == 2 {
        let mut keys = BTreeSet::new();
        let mut previous = None;
        for authority in array(&value["service_authorities"])? {
            let reference = &authority["reference"];
            object(reference, &["view", "role", "kind", "id"], &[])?;
            one_of(&reference["view"], &["current", "source", "target"])?;
            one_of(&reference["role"], &["active", "retained"])?;
            one_of(&reference["kind"], &["storage", "resource"])?;
            id(&reference["id"])?;
            let mut required = vec!["handle", "reference", "mode", "path"];
            if reference["kind"] == "resource" {
                required.push("resource_kind");
                one_of(&authority["resource_kind"], &["file", "directory"])?;
            }
            object(authority, &required, &[])?;
            one_of(&authority["mode"], &["read", "write"])?;
            absolute(&authority["path"])?;
            handle(&authority["handle"], &mut handles)?;
            let key = (
                reference["view"].as_str(),
                reference["role"].as_str(),
                reference["kind"].as_str(),
                reference["id"].as_str(),
            );
            if !keys.insert(key) {
                return Err(failure());
            }
            if previous.is_some_and(|p| p >= key) {
                return Err(failure());
            }
            previous = Some(key);
        }
    }
    if let Some(target) = value.get("target_commit") {
        if op["kind"] != "migration" {
            return Err(failure());
        }
        object(target, &["handle"], &[])?;
        handle(&target["handle"], &mut handles)?;
    }
    Ok(())
}

pub(super) fn cancel(value: &Value) -> io::Result<()> {
    object(value, &["type", "control_id", "reason"], &[])?;
    one_of(&value["reason"], &["requested", "timeout"])?;
    if positive_integer(&value["control_id"]).is_none() {
        return Err(failure());
    }
    Ok(())
}

pub(super) fn positive_integer(value: &Value) -> Option<u64> {
    value
        .as_f64()
        .filter(|n| n.fract() == 0.0 && (1.0..=9_007_199_254_740_991.0).contains(n))
        .map(|n| n as u64)
}

pub(super) fn risk_ack(value: &Value, request_id: u64, open: bool) -> io::Result<()> {
    object(value, &["type", "request_id", "risk_state"], &[])?;
    if value["type"] != "request_ack"
        || positive_integer(&value["request_id"]) != Some(request_id)
        || value["risk_state"] != if open { "open" } else { "clear" }
    {
        return Err(failure());
    }
    Ok(())
}

pub(super) fn outputs(session: &Value) -> &[Value] {
    let op = &session["operation"];
    op[if op["kind"] == "migration" {
        "target_outputs"
    } else {
        "outputs"
    }]
    .as_array()
    .map(Vec::as_slice)
    .unwrap_or_default()
}
pub(super) fn output<'a>(session: &'a Value, id: &str) -> Option<&'a Value> {
    outputs(session).iter().find(|v| {
        v[if session["operation"]["kind"] == "migration" {
            "input_id"
        } else {
            "output_id"
        }] == id
    })
}
pub(super) fn input(session: &Value, id: &str, view: &str, role: &str) -> Option<Value> {
    let op = &session["operation"];
    let field = match (op["kind"].as_str(), view) {
        (Some("migration"), "source") => "source_bindings",
        (Some("migration"), "target") => "target_bindings",
        (Some("migration"), _) => return None,
        (_, "current") => "bindings",
        _ => return None,
    };
    op[field]
        .as_array()?
        .iter()
        .find(|v| v["input_id"] == id && v["role"] == role)?
        .get("readonly_path")
        .cloned()
}
