//! Shared lexical/declaration projection and canonical runtime-content encoding.
//!
//! This module is intentionally independent from `xtask` implementation code.

#![allow(dead_code)]

mod raw_json;

use std::collections::BTreeMap;

use raw_json::RawJsonValue;
#[cfg(test)]
use sha2::{Digest, Sha256};

use crate::domain::*;

const REVISION_DIGEST_DOMAIN: &[u8] = b"pactrun.revision-content-digest\0";
const REVISION_CORE_LABEL: &[u8] = b"revision-core\0";
const RUNTIME_CONTENT_LABEL: &[u8] = b"runtime-content-closure\0";

#[cfg(test)]
pub(crate) fn encode_service_free_revision(
    core: &RevisionDeclarations,
) -> Result<Vec<u8>, RevisionError> {
    let core = crate::domain::service_free_revision(core.clone());
    crate::revision_canonical::encode_service_core(&core)
        .map_err(|e| RevisionError::internal("canonicalization_failed", e.to_string()))
}

pub(crate) fn encode_canonical_runtime_content(
    runtime_content: &RuntimeContentClosureIdentityV1,
) -> Result<Vec<u8>, RevisionError> {
    jcs_bytes(runtime_content)
}

#[cfg(test)]
pub(crate) fn decode_service_free_revision(
    bytes: &[u8],
) -> Result<RevisionDeclarations, RevisionError> {
    let core = crate::revision_canonical::decode_canonical_service_revision(bytes)
        .map_err(|e| RevisionError::internal("noncanonical_json", e.to_string()))?;
    if !core.storages().is_empty()
        || !core.resources().is_empty()
        || core
            .hooks()
            .values()
            .any(|h| !h.access.is_empty() || !h.requires.is_empty())
    {
        return Err(RevisionError::internal(
            "invalid_service_declaration",
            "service-free declaration builder cannot discard authority",
        ));
    }
    Ok(core.common().clone())
}

/// Project schema-directed authoring JSON through the Frozen semantic model
/// without requiring the source bytes themselves to be canonical JSON.
#[cfg(test)]
pub(crate) fn project_service_free_revision_source(
    bytes: &[u8],
) -> Result<RevisionDeclarations, RevisionError> {
    decode_semantic_revision_core_internal_v1(bytes)
}

pub(crate) fn decode_canonical_runtime_content(
    bytes: &[u8],
) -> Result<RuntimeContentClosureIdentityV1, RevisionError> {
    let content = decode_semantic_runtime_content_internal_v1(bytes)?;
    if encode_canonical_runtime_content(&content)? != bytes {
        return Err(RevisionError::internal(
            "noncanonical_json",
            "RuntimeContentClosureIdentityV1 bytes are not exact Frozen JCS",
        ));
    }
    Ok(content)
}

#[cfg(test)]
pub(crate) fn decode_service_free_content(
    core_bytes: &[u8],
    content_bytes: &[u8],
) -> Result<DeclarationContent, RevisionError> {
    validate_declaration_content(
        decode_service_free_revision(core_bytes)?,
        decode_canonical_runtime_content(content_bytes)?,
    )
}

#[cfg(test)]
pub(crate) fn calculate_service_free_digest(
    content: &DeclarationContent,
) -> Result<RevisionContentDigest, RevisionError> {
    let core_jcs = encode_service_free_revision(&content.core)?;
    let runtime_content_jcs = encode_canonical_runtime_content(&content.runtime_content)?;
    let frame = frame_revision_content_v1(&core_jcs, &runtime_content_jcs);
    let digest: [u8; 32] = Sha256::digest(frame).into();
    Ok(RevisionContentDigest::from_bytes(digest))
}

#[cfg(test)]
fn decode_semantic_revision_content_internal_v1(
    core_bytes: &[u8],
    content_bytes: &[u8],
) -> Result<DeclarationContent, RevisionError> {
    validate_declaration_content(
        decode_semantic_revision_core_internal_v1(core_bytes)?,
        decode_semantic_runtime_content_internal_v1(content_bytes)?,
    )
}

#[cfg(test)]
fn decode_semantic_revision_core_internal_v1(
    bytes: &[u8],
) -> Result<RevisionDeclarations, RevisionError> {
    project_core_value_v1(raw_json::parse_json(bytes)?)
}

/// Shared typed V1-field validation; the caller must already have strict-parsed
/// the AST. V2 uses this only for its unchanged common fields, never its identity.
pub(crate) fn project_core_value_v1(
    value: RawJsonValue,
) -> Result<RevisionDeclarations, RevisionError> {
    project_common_core(value, false)
}

pub(crate) fn project_common_core(
    value: RawJsonValue,
    _shell_loader: bool,
) -> Result<RevisionDeclarations, RevisionError> {
    let mut object = closed_object(
        value,
        "Revision declarations",
        &["format_version", "inputs", "actions", "migrations"],
        &["snapshot", "cleanup"],
    )?;
    let RawJsonValue::String(version) = take_required(&mut object, "format_version")? else {
        return Err(RevisionError::internal(
            "unsupported_format_version",
            "Revision format_version must be a supported string",
        ));
    };
    VersionDomain::Revision
        .require(&version)
        .map_err(|e| RevisionError::internal("unsupported_format_version", e))?;
    project_declarations(RawJsonValue::Object(object.into_iter().collect()))
}
pub(crate) fn project_declarations(
    value: RawJsonValue,
) -> Result<RevisionDeclarations, RevisionError> {
    let shell_loader = true;
    let mut object = closed_object(
        value,
        "Revision declarations",
        &["inputs", "actions", "migrations"],
        &["snapshot", "cleanup"],
    )?;
    let inputs = parse_array(take_required(&mut object, "inputs")?, parse_input)?;
    let actions = parse_array(take_required(&mut object, "actions")?, |v| {
        parse_action(v, shell_loader)
    })?;
    let snapshot = object
        .remove("snapshot")
        .map(|v| parse_snapshot(v, shell_loader))
        .transpose()?;
    let migrations = parse_array(take_required(&mut object, "migrations")?, |v| {
        parse_migration(v, shell_loader)
    })?;
    let cleanup = object
        .remove("cleanup")
        .map(|v| parse_cleanup(v, shell_loader))
        .transpose()?;
    project_revision_declarations(RevisionDeclarationInput {
        inputs,
        actions,
        snapshot,
        migrations,
        cleanup,
    })
}

fn decode_semantic_runtime_content_internal_v1(
    bytes: &[u8],
) -> Result<RuntimeContentClosureIdentityV1, RevisionError> {
    let mut object = closed_object(
        raw_json::parse_json(bytes)?,
        "RuntimeContentClosureIdentityV1",
        &["files"],
        &[],
    )?;
    let files = parse_array(take_required(&mut object, "files")?, parse_runtime_file)?;
    project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files })
}

fn parse_input(value: RawJsonValue) -> Result<InputDeclarationV1, RevisionError> {
    let mut object = closed_object(
        value,
        "InputDeclarationV1",
        &["id", "required", "protection"],
        &[],
    )?;
    Ok(InputDeclarationV1 {
        id: InputIdentity::parse(take_string(&mut object, "id")?)?,
        required: take_bool(&mut object, "required")?,
        protection: match take_string(&mut object, "protection")?.as_str() {
            "normal" => InputProtectionV1::Normal,
            "secret" => InputProtectionV1::Secret,
            token => return Err(invalid_enum("InputProtectionV1", token)),
        },
    })
}

fn parse_action(value: RawJsonValue, shell_loader: bool) -> Result<ActionV1, RevisionError> {
    let mut object = closed_object(
        value,
        "ActionV1",
        &["id", "access", "parameters", "hook", "outputs"],
        &[],
    )?;
    Ok(ActionV1 {
        id: ActionIdentity::parse(take_string(&mut object, "id")?)?,
        access: take_access(&mut object)?,
        parameters: parse_array(take_required(&mut object, "parameters")?, parse_parameter)?,
        hook: parse_hook(take_required(&mut object, "hook")?, shell_loader)?,
        outputs: parse_array(take_required(&mut object, "outputs")?, parse_output)?,
    })
}

fn parse_output(value: RawJsonValue) -> Result<ManagedOutputV1, RevisionError> {
    let mut object = closed_object(value, "ManagedOutputV1", &["id"], &[])?;
    Ok(ManagedOutputV1 {
        id: ManagedOutputIdentity::parse(take_string(&mut object, "id")?)?,
    })
}

fn parse_parameter(value: RawJsonValue) -> Result<ParameterV1, RevisionError> {
    let mut object = closed_object(
        value,
        "ParameterV1",
        &["id", "type", "sensitive"],
        &["default"],
    )?;
    let parameter_type = match take_string(&mut object, "type")?.as_str() {
        "integer" => ParameterTypeV1::Integer,
        "float" => ParameterTypeV1::Float,
        "boolean" => ParameterTypeV1::Boolean,
        "string" => ParameterTypeV1::String,
        token => return Err(invalid_enum("ParameterTypeV1", token)),
    };
    let default = object
        .remove("default")
        .map(|value| parse_parameter_default(value, parameter_type))
        .transpose()?;
    Ok(ParameterV1 {
        id: ParameterIdentity::parse(take_string(&mut object, "id")?)?,
        parameter_type,
        sensitive: take_bool(&mut object, "sensitive")?,
        default,
    })
}

fn parse_parameter_default(
    value: RawJsonValue,
    parameter_type: ParameterTypeV1,
) -> Result<ParameterDefaultV1, RevisionError> {
    match (parameter_type, value) {
        (ParameterTypeV1::Integer, value @ RawJsonValue::Number(_)) => Ok(
            ParameterDefaultV1::Integer(SafeIntegerV1::new(exact_integer(value)?)?),
        ),
        (ParameterTypeV1::Float, RawJsonValue::Number(token)) => {
            let number = token
                .parse::<f64>()
                .map_err(|_| RevisionError::stable("invalid_number", "invalid binary64 number"))?;
            Ok(ParameterDefaultV1::Float(FiniteF64::new(number)?))
        }
        (ParameterTypeV1::Boolean, RawJsonValue::Bool(value)) => {
            Ok(ParameterDefaultV1::Boolean(value))
        }
        (ParameterTypeV1::String, RawJsonValue::String(value)) => {
            Ok(ParameterDefaultV1::String(value))
        }
        _ => Err(RevisionError::internal(
            "invalid_type",
            "parameter default does not match its declared type",
        )),
    }
}

fn parse_hook(value: RawJsonValue, shell_loader: bool) -> Result<HookV1, RevisionError> {
    let mut object = closed_object(
        value,
        "HookV1",
        &["protocol_version", "launch", "args", "io"],
        &[],
    )?;
    Ok(HookV1 {
        protocol_version: match take_required(&mut object, "protocol_version")? {
            RawJsonValue::String(text) => text.parse::<FormatVersion>().map_err(|e| {
                RevisionError::internal("unsupported_protocol_version", e.to_string())
            })?,
            _ => {
                return Err(RevisionError::internal(
                    "unsupported_protocol_version",
                    "Hook version must be a string",
                ));
            }
        },
        launch: parse_launch(take_required(&mut object, "launch")?, shell_loader)?,
        args: parse_string_array(take_required(&mut object, "args")?)?,
        io: parse_io(take_required(&mut object, "io")?)?,
    })
}

fn parse_launch(value: RawJsonValue, shell_loader: bool) -> Result<HookLaunchV1, RevisionError> {
    let mut object = into_object(value, "HookLaunchV1")?;
    let kind = take_string(&mut object, "kind")?;
    match kind.as_str() {
        "shell_loader" if shell_loader => {
            validate_remaining_fields(
                &object,
                "ShellLoader",
                &["shell", "command", "script"],
                &[],
            )?;
            let shell = ShellKind::parse(&take_string(&mut object, "shell")?).ok_or_else(|| {
                RevisionError::internal("invalid_hook_launch", "unsupported shell kind")
            })?;
            Ok(HookLaunchV1::ShellLoader {
                shell,
                command: HostExecutableName::parse(take_string(&mut object, "command")?)?,
                script: ContentId::parse(take_string(&mut object, "script")?)?,
            })
        }
        "direct" => {
            validate_remaining_fields(&object, "HookLaunchV1", &["executable"], &[])?;
            Ok(HookLaunchV1::Direct {
                executable: ContentId::parse(take_string(&mut object, "executable")?)?,
            })
        }
        "interpreter" => {
            validate_remaining_fields(
                &object,
                "HookLaunchV1",
                &["command", "interpreter_args", "script"],
                &[],
            )?;
            Ok(HookLaunchV1::Interpreter {
                command: HostExecutableName::parse(take_string(&mut object, "command")?)?,
                interpreter_args: parse_string_array(take_required(
                    &mut object,
                    "interpreter_args",
                )?)?,
                script: ContentId::parse(take_string(&mut object, "script")?)?,
            })
        }
        _ => Err(RevisionError::stable(
            "invalid_hook_launch",
            "Hook launch kind must be direct or interpreter",
        )),
    }
}

fn parse_io(value: RawJsonValue) -> Result<IOContractV1, RevisionError> {
    let mut object = closed_object(value, "IOContractV1", &["terminal"], &[])?;
    let terminal = match take_string(&mut object, "terminal")?.as_str() {
        "none" => TerminalContractV1::None,
        "output" => TerminalContractV1::Output,
        "interactive" => TerminalContractV1::Interactive,
        token => return Err(invalid_enum("TerminalContractV1", token)),
    };
    Ok(IOContractV1 { terminal })
}

fn parse_snapshot(
    value: RawJsonValue,
    shell_loader: bool,
) -> Result<SnapshotCapabilityV1, RevisionError> {
    let mut object = closed_object(value, "SnapshotCapabilityV1", &[], &["capture", "restore"])?;
    Ok(SnapshotCapabilityV1 {
        capture: object
            .remove("capture")
            .map(|v| parse_capture(v, shell_loader))
            .transpose()?,
        restore: object
            .remove("restore")
            .map(|v| parse_restore(v, shell_loader))
            .transpose()?,
    })
}

fn parse_capture(value: RawJsonValue, shell_loader: bool) -> Result<CaptureV1, RevisionError> {
    let mut object = closed_object(value, "CaptureV1", &["parameters", "access", "hook"], &[])?;
    Ok(CaptureV1 {
        parameters: parse_array(take_required(&mut object, "parameters")?, parse_parameter)?,
        access: take_access(&mut object)?,
        hook: parse_hook(take_required(&mut object, "hook")?, shell_loader)?,
    })
}

fn parse_restore(value: RawJsonValue, shell_loader: bool) -> Result<RestoreV1, RevisionError> {
    let mut object = closed_object(value, "RestoreV1", &["parameters", "hook"], &[])?;
    Ok(RestoreV1 {
        parameters: parse_array(take_required(&mut object, "parameters")?, parse_parameter)?,
        hook: parse_hook(take_required(&mut object, "hook")?, shell_loader)?,
    })
}

fn parse_migration(value: RawJsonValue, shell_loader: bool) -> Result<MigrationV1, RevisionError> {
    let mut object = closed_object(
        value,
        "MigrationV1",
        &[
            "source_revision_digest",
            "transitions",
            "requires_source",
            "requires_target",
            "produces_target",
        ],
        &["hook"],
    )?;
    Ok(MigrationV1 {
        source_revision_digest: Sha256Digest::parse(take_string(
            &mut object,
            "source_revision_digest",
        )?)?,
        transitions: parse_array(take_required(&mut object, "transitions")?, parse_transition)?,
        requires_source: parse_array(
            take_required(&mut object, "requires_source")?,
            parse_binding_ref,
        )?,
        requires_target: parse_id_array(take_required(&mut object, "requires_target")?)?,
        produces_target: parse_id_array(take_required(&mut object, "produces_target")?)?,
        hook: object
            .remove("hook")
            .map(|v| parse_hook(v, shell_loader))
            .transpose()?,
    })
}

fn parse_binding_ref(value: RawJsonValue) -> Result<InputBindingRefV1, RevisionError> {
    let mut object = closed_object(value, "InputBindingRefV1", &["role", "input_id"], &[])?;
    let role = match take_string(&mut object, "role")?.as_str() {
        "active" => InputBindingRoleV1::Active,
        "retained" => InputBindingRoleV1::Retained,
        token => return Err(invalid_enum("InputBindingRoleV1", token)),
    };
    Ok(InputBindingRefV1 {
        role,
        input_id: InputIdentity::parse(take_string(&mut object, "input_id")?)?,
    })
}

fn parse_transition(value: RawJsonValue) -> Result<MigrationTransitionV1, RevisionError> {
    let mut object = into_object(value, "MigrationTransitionV1")?;
    let kind = take_string(&mut object, "kind")?;
    match kind.as_str() {
        "carry" | "declassify" => {
            validate_remaining_fields(
                &object,
                "MigrationTransitionV1",
                &["source", "target_input_id"],
                &[],
            )?;
            let source = parse_binding_ref(take_required(&mut object, "source")?)?;
            let target_input_id =
                InputIdentity::parse(take_string(&mut object, "target_input_id")?)?;
            Ok(if kind == "carry" {
                MigrationTransitionV1::Carry {
                    source,
                    target_input_id,
                }
            } else {
                MigrationTransitionV1::Declassify {
                    source,
                    target_input_id,
                }
            })
        }
        "keep" | "discard" => {
            validate_remaining_fields(&object, "MigrationTransitionV1", &["source"], &[])?;
            let source = parse_binding_ref(take_required(&mut object, "source")?)?;
            Ok(if kind == "keep" {
                MigrationTransitionV1::Keep { source }
            } else {
                MigrationTransitionV1::Discard { source }
            })
        }
        _ => Err(RevisionError::internal(
            "invalid_transition",
            "unknown Migration transition kind",
        )),
    }
}

fn parse_cleanup(value: RawJsonValue, shell_loader: bool) -> Result<CleanupV1, RevisionError> {
    let mut object = closed_object(value, "CleanupV1", &["requires", "hook"], &[])?;
    Ok(CleanupV1 {
        requires: parse_array(take_required(&mut object, "requires")?, parse_binding_ref)?,
        hook: parse_hook(take_required(&mut object, "hook")?, shell_loader)?,
    })
}

fn parse_runtime_file(value: RawJsonValue) -> Result<RuntimeFileV1, RevisionError> {
    let mut object = closed_object(
        value,
        "RuntimeFileV1",
        &["id", "path", "kind", "blob_digest", "executable"],
        &[],
    )?;
    let kind = match take_string(&mut object, "kind")?.as_str() {
        "regular_file" => RuntimeFileKindV1::RegularFile,
        token => return Err(invalid_enum("RuntimeFileKindV1", token)),
    };
    Ok(RuntimeFileV1 {
        id: ContentId::parse(take_string(&mut object, "id")?)?,
        path: RuntimePath::parse(take_string(&mut object, "path")?)?,
        kind,
        blob_digest: Sha256Digest::parse(take_string(&mut object, "blob_digest")?)?,
        executable: take_bool(&mut object, "executable")?,
    })
}

pub(crate) fn exact_integer(value: RawJsonValue) -> Result<i64, RevisionError> {
    let RawJsonValue::Number(token) = value else {
        return Err(RevisionError::internal(
            "invalid_type",
            "expected a JSON number",
        ));
    };
    let negative = token.starts_with('-');
    let unsigned = token.strip_prefix('-').unwrap_or(&token);
    let (mantissa, exponent_text) = unsigned
        .split_once(['e', 'E'])
        .map_or((unsigned, None), |(left, right)| (left, Some(right)));
    let exponent = exponent_text
        .map(str::parse::<i64>)
        .transpose()
        .map_err(|_| RevisionError::stable("invalid_number", "number exponent is too large"))?
        .unwrap_or(0);
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut digits = format!("{whole}{fraction}");
    if digits.bytes().all(|byte| byte == b'0') {
        return Ok(0);
    }
    let fraction_length = i64::try_from(fraction.len())
        .map_err(|_| RevisionError::stable("invalid_number", "integer fraction is too large"))?;
    let decimal_shift = exponent
        .checked_sub(fraction_length)
        .ok_or_else(|| RevisionError::stable("invalid_number", "integer scale is out of range"))?;
    if decimal_shift < 0 {
        let remove = usize::try_from(decimal_shift.unsigned_abs()).unwrap_or(usize::MAX);
        if remove > digits.len()
            || !digits[digits.len() - remove..]
                .bytes()
                .all(|byte| byte == b'0')
        {
            return Err(RevisionError::stable(
                "invalid_number",
                "integer value is not mathematically integral",
            ));
        }
        digits.truncate(digits.len() - remove);
    } else {
        let append = usize::try_from(decimal_shift).unwrap_or(usize::MAX);
        if append > 16 {
            return Err(RevisionError::stable(
                "invalid_number",
                "integer value exceeds the safe range",
            ));
        }
        digits.extend(std::iter::repeat_n('0', append));
    }
    let digits = digits.trim_start_matches('0');
    if digits.len() > 16 {
        return Err(RevisionError::stable(
            "invalid_number",
            "integer value exceeds the safe range",
        ));
    }
    let magnitude = digits.parse::<i64>().map_err(|_| {
        RevisionError::stable("invalid_number", "integer value exceeds the safe range")
    })?;
    if magnitude > MAX_SAFE_INTEGER {
        return Err(RevisionError::stable(
            "invalid_number",
            "integer value exceeds the safe range",
        ));
    }
    Ok(if negative { -magnitude } else { magnitude })
}

fn closed_object(
    value: RawJsonValue,
    name: &str,
    required: &[&str],
    optional: &[&str],
) -> Result<BTreeMap<String, RawJsonValue>, RevisionError> {
    let object = into_object(value, name)?;
    validate_remaining_fields(&object, name, required, optional)?;
    Ok(object)
}

fn into_object(
    value: RawJsonValue,
    name: &str,
) -> Result<BTreeMap<String, RawJsonValue>, RevisionError> {
    match value {
        RawJsonValue::Object(entries) => Ok(entries.into_iter().collect()),
        _ => Err(RevisionError::internal(
            "invalid_type",
            format!("{name} must be an object"),
        )),
    }
}

fn validate_remaining_fields(
    object: &BTreeMap<String, RawJsonValue>,
    name: &str,
    required: &[&str],
    optional: &[&str],
) -> Result<(), RevisionError> {
    for key in object.keys() {
        if !required.contains(&key.as_str()) && !optional.contains(&key.as_str()) {
            return Err(RevisionError::stable(
                "unknown_field",
                format!("unknown {name} field {key}"),
            ));
        }
    }
    for field in required {
        if !object.contains_key(*field) {
            return Err(RevisionError::internal(
                "missing_field",
                format!("missing {name} field {field}"),
            ));
        }
    }
    Ok(())
}

fn take_required(
    object: &mut BTreeMap<String, RawJsonValue>,
    field: &str,
) -> Result<RawJsonValue, RevisionError> {
    object
        .remove(field)
        .ok_or_else(|| RevisionError::internal("missing_field", format!("missing field {field}")))
}

fn take_string(
    object: &mut BTreeMap<String, RawJsonValue>,
    field: &str,
) -> Result<String, RevisionError> {
    match take_required(object, field)? {
        RawJsonValue::String(value) => Ok(value),
        _ => Err(RevisionError::internal(
            "invalid_type",
            format!("{field} must be a string"),
        )),
    }
}

fn take_bool(
    object: &mut BTreeMap<String, RawJsonValue>,
    field: &str,
) -> Result<bool, RevisionError> {
    match take_required(object, field)? {
        RawJsonValue::Bool(value) => Ok(value),
        _ => Err(RevisionError::internal(
            "invalid_type",
            format!("{field} must be a boolean"),
        )),
    }
}

fn take_access(
    object: &mut BTreeMap<String, RawJsonValue>,
) -> Result<OperationAccessV1, RevisionError> {
    match take_string(object, "access")?.as_str() {
        "observe" => Ok(OperationAccessV1::Observe),
        "mutate" => Ok(OperationAccessV1::Mutate),
        token => Err(invalid_enum("OperationAccessV1", token)),
    }
}

fn parse_array<T>(
    value: RawJsonValue,
    parse: impl FnMut(RawJsonValue) -> Result<T, RevisionError>,
) -> Result<Vec<T>, RevisionError> {
    match value {
        RawJsonValue::Array(values) => values.into_iter().map(parse).collect(),
        _ => Err(RevisionError::internal(
            "invalid_type",
            "expected a JSON array",
        )),
    }
}

fn parse_string_array(value: RawJsonValue) -> Result<Vec<String>, RevisionError> {
    parse_array(value, |value| match value {
        RawJsonValue::String(value) => Ok(value),
        _ => Err(RevisionError::internal(
            "invalid_type",
            "array item must be a string",
        )),
    })
}

fn parse_id_array(value: RawJsonValue) -> Result<Vec<InputIdentity>, RevisionError> {
    parse_array(value, |value| match value {
        RawJsonValue::String(value) => InputIdentity::parse(value),
        _ => Err(RevisionError::internal(
            "invalid_type",
            "target Input identity must be a string",
        )),
    })
}

fn invalid_enum(name: &str, token: &str) -> RevisionError {
    RevisionError::internal("invalid_type", format!("unknown {name} token {token:?}"))
}

fn jcs_bytes(value: &impl serde::Serialize) -> Result<Vec<u8>, RevisionError> {
    serde_jcs::to_vec(value).map_err(|error| {
        RevisionError::internal(
            "canonicalization_failed",
            format!("RFC 8785 serialization failed: {error}"),
        )
    })
}

#[cfg(test)]
fn frame_revision_content_v1(core_jcs: &[u8], content_jcs: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(
        REVISION_DIGEST_DOMAIN.len()
            + 4
            + REVISION_CORE_LABEL.len()
            + 8
            + core_jcs.len()
            + RUNTIME_CONTENT_LABEL.len()
            + 8
            + content_jcs.len(),
    );
    frame.extend_from_slice(REVISION_DIGEST_DOMAIN);
    frame.extend_from_slice(REVISION_CORE_LABEL);
    frame.extend_from_slice(&(core_jcs.len() as u64).to_be_bytes());
    frame.extend_from_slice(core_jcs);
    frame.extend_from_slice(RUNTIME_CONTENT_LABEL);
    frame.extend_from_slice(&(content_jcs.len() as u64).to_be_bytes());
    frame.extend_from_slice(content_jcs);
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::Value;
    use std::collections::BTreeSet;

    const VECTORS: &str = include_str!("../../tests/vectors/revision_canonical/declarations.json");
    const ERROR_CATALOG: &str = include_str!("../../tests/vectors/error_taxonomy_v1/catalog.json");

    #[derive(Deserialize)]
    struct Manifest {
        status: String,
        valid: Vec<ValidVector>,
        invalid: Vec<InvalidVector>,
    }

    #[derive(Deserialize)]
    struct ValidVector {
        name: String,
        raw_core: String,
        raw_content: String,
        normalized_core: Value,
        normalized_content: Value,
        #[serde(default)]
        source_input_ids: BTreeMap<String, Vec<String>>,
        relational: RelationalExpected,
        expected: Expected,
    }

    #[derive(Deserialize)]
    struct InvalidVector {
        name: String,
        raw_core: String,
        raw_content: String,
        #[serde(default)]
        source_input_ids: BTreeMap<String, Vec<String>>,
        expected_error: String,
    }

    #[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
    #[serde(tag = "status", rename_all = "snake_case")]
    enum RelationalExpected {
        Valid,
        NotEvaluated,
    }

    #[derive(Deserialize)]
    struct Expected {
        core_jcs_hex: String,
        content_jcs_hex: String,
        frame_hex: String,
        digest: String,
    }

    fn manifest() -> Manifest {
        serde_json::from_str(VECTORS).unwrap()
    }

    fn source_context(
        raw: &BTreeMap<String, Vec<String>>,
    ) -> BTreeMap<Sha256Digest, BTreeSet<InputIdentity>> {
        raw.iter()
            .map(|(digest, ids)| {
                (
                    Sha256Digest::parse(digest).unwrap(),
                    ids.iter()
                        .map(|id| InputIdentity::parse(id).unwrap())
                        .collect(),
                )
            })
            .collect()
    }

    fn decode_invalid(vector: &InvalidVector) -> RevisionError {
        let result = decode_semantic_revision_content_internal_v1(
            vector.raw_core.as_bytes(),
            vector.raw_content.as_bytes(),
        );
        match result {
            Err(error) => error,
            Ok(content) => validate_revision_sources_v1(
                &content.core,
                &source_context(&vector.source_input_ids),
            )
            .expect_err("Frozen invalid vector unexpectedly passed production validation"),
        }
    }

    // Test-ID: PR-TEST-0040
    // Verifies: PR-REQ-0014, PR-REQ-0226, PR-REQ-0227
    #[test]
    fn revision_core_and_runtime_content_are_sibling_wire_components() {
        let vector = &manifest().valid[0];
        let content = decode_semantic_revision_content_internal_v1(
            vector.raw_core.as_bytes(),
            vector.raw_content.as_bytes(),
        )
        .unwrap();
        let core: Value =
            serde_json::from_slice(&encode_service_free_revision(&content.core).unwrap()).unwrap();
        let runtime: Value = serde_json::from_slice(
            &encode_canonical_runtime_content(&content.runtime_content).unwrap(),
        )
        .unwrap();
        assert_eq!(
            core.as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            [
                "actions",
                "format_version",
                "inputs",
                "migrations",
                "service_resources",
                "service_storages"
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        );
        assert!(core.get("files").is_none());
        assert_eq!(runtime["files"][0]["kind"], "regular_file");
        assert!(runtime.get("format_version").is_none());
    }

    // Test-ID: PR-TEST-0041
    // Verifies: PR-REQ-0010, PR-REQ-0014, PR-REQ-0016, PR-REQ-0226
    #[test]
    fn internal_semantic_projection_matches_frozen_normalized_values() {
        for vector in manifest().valid {
            let content = decode_semantic_revision_content_internal_v1(
                vector.raw_core.as_bytes(),
                vector.raw_content.as_bytes(),
            )
            .unwrap_or_else(|error| panic!("{}: {error}", vector.name));
            assert_eq!(
                encode_service_free_revision(&content.core).unwrap(),
                serde_jcs::to_vec(&vector.normalized_core).unwrap(),
                "{} core",
                vector.name
            );
            assert_eq!(
                encode_canonical_runtime_content(&content.runtime_content).unwrap(),
                serde_jcs::to_vec(&vector.normalized_content).unwrap(),
                "{} content",
                vector.name
            );
            let relational = validate_revision_sources_v1(
                &content.core,
                &source_context(&vector.source_input_ids),
            )
            .unwrap();
            assert_eq!(
                relational == RelationalValidationV1::Valid,
                vector.relational == RelationalExpected::Valid,
                "{} relational",
                vector.name
            );
        }
    }

    // Test-ID: PR-TEST-0042
    // Verifies: PR-REQ-0226
    #[test]
    fn hook_content_validation_is_semantic_closure_membership_only() {
        let direct = br#"{"format_version":"1.0-alpha.1","inputs":[],"actions":[{"id":"a","access":"observe","parameters":[],"hook":{"protocol_version":"1.0-alpha.1","launch":{"kind":"direct","executable":"hook"},"args":[],"io":{"terminal":"none"}},"outputs":[]}],"migrations":[]}"#;
        let non_executable = br#"{"files":[{"id":"hook","path":"bin/hook","kind":"regular_file","blob_digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","executable":false}]}"#;
        let error =
            decode_semantic_revision_content_internal_v1(direct, non_executable).unwrap_err();
        assert_eq!(error.internal_code(), "invalid_hook_launch");

        let interpreter = br#"{"format_version":"1.0-alpha.1","inputs":[],"actions":[{"id":"a","access":"observe","parameters":[],"hook":{"protocol_version":"1.0-alpha.1","launch":{"kind":"interpreter","command":"python.exe","interpreter_args":[],"script":"hook"},"args":[],"io":{"terminal":"none"}},"outputs":[]}],"migrations":[]}"#;
        assert!(decode_semantic_revision_content_internal_v1(interpreter, non_executable).is_ok());
        let missing = br#"{"files":[]}"#;
        assert_eq!(
            decode_semantic_revision_content_internal_v1(interpreter, missing)
                .unwrap_err()
                .internal_code(),
            "invalid_reference"
        );
    }

    // Test-ID: PR-TEST-0043
    // Verifies: PR-REQ-0227
    #[test]
    fn semantic_parser_is_internal_and_canonical_decoder_is_strict() {
        assert!(SafeIntegerV1::new(MAX_SAFE_INTEGER).is_ok());
        assert!(SafeIntegerV1::new(-MAX_SAFE_INTEGER).is_ok());
        assert_eq!(
            SafeIntegerV1::new(MAX_SAFE_INTEGER + 1)
                .unwrap_err()
                .internal_code(),
            "invalid_number"
        );

        let invalid_utf8 = decode_semantic_revision_core_internal_v1(&[0xff]).unwrap_err();
        assert_eq!(invalid_utf8.internal_code(), "invalid_unicode_scalar");

        let duplicate = decode_semantic_revision_core_internal_v1(
            br#"{"format_version":"1.0-alpha.1","format_version":"1.0-alpha.1","inputs":[],"actions":[],"migrations":[]}"#,
        )
        .unwrap_err();
        assert_eq!(duplicate.internal_code(), "duplicate_property");
        assert_eq!(duplicate.stable_ref().unwrap().code(), "duplicate_property");
        let invalid_scalar = decode_semantic_revision_core_internal_v1(
            br#"{"format_version":"1.0-alpha.1","inputs":[],"actions":[],"migrations":[],"bad":"\uD800"}"#,
        )
        .unwrap_err();
        assert_eq!(invalid_scalar.internal_code(), "invalid_unicode_scalar");

        let decoded_duplicate = decode_semantic_revision_core_internal_v1(
            br#"{"format_version":"1.0-alpha.1","inputs":[],"actions":[],"migrations":[],"x":1,"\u0078":2}"#,
        )
        .unwrap_err();
        assert_eq!(decoded_duplicate.internal_code(), "duplicate_property");

        let mut excessive_depth = String::from(
            r#"{"format_version":"1.0-alpha.1","inputs":[],"actions":[],"migrations":[],"x":"#,
        );
        excessive_depth.push_str(&"[".repeat(66));
        excessive_depth.push_str("null");
        excessive_depth.push_str(&"]".repeat(66));
        excessive_depth.push('}');
        assert_eq!(
            decode_semantic_revision_core_internal_v1(excessive_depth.as_bytes())
                .unwrap_err()
                .internal_code(),
            "invalid_json_syntax"
        );

        let vector = &manifest().valid[0];
        let content = decode_semantic_revision_content_internal_v1(
            vector.raw_core.as_bytes(),
            vector.raw_content.as_bytes(),
        )
        .unwrap();
        let core_jcs = encode_service_free_revision(&content.core).unwrap();
        assert!(decode_service_free_revision(&core_jcs).is_ok());
        assert!(decode_service_free_revision(vector.raw_core.as_bytes()).is_err());

        let numbers = manifest()
            .valid
            .into_iter()
            .find(|item| item.name == "number_matrix")
            .unwrap();
        let normalized = decode_semantic_revision_content_internal_v1(
            numbers.raw_core.as_bytes(),
            numbers.raw_content.as_bytes(),
        )
        .unwrap();
        assert_eq!(
            encode_service_free_revision(&normalized.core).unwrap(),
            hex::decode(numbers.expected.core_jcs_hex).unwrap()
        );
    }

    // Test-ID: PR-TEST-0044
    // Verifies: PR-REQ-0012, PR-REQ-0015, PR-REQ-0017, PR-REQ-0018, PR-REQ-0079, PR-REQ-0138, PR-REQ-0227
    #[test]
    fn production_codec_reproduces_all_frozen_bytes_frames_and_digests() {
        for vector in manifest().valid {
            let content = decode_semantic_revision_content_internal_v1(
                vector.raw_core.as_bytes(),
                vector.raw_content.as_bytes(),
            )
            .unwrap_or_else(|error| panic!("{}: {error}", vector.name));
            let core = encode_service_free_revision(&content.core).unwrap();
            let runtime = encode_canonical_runtime_content(&content.runtime_content).unwrap();
            assert_eq!(
                hex::encode(&core),
                vector.expected.core_jcs_hex,
                "{}",
                vector.name
            );
            assert_eq!(
                hex::encode(&runtime),
                vector.expected.content_jcs_hex,
                "{}",
                vector.name
            );
            assert_eq!(
                hex::encode(frame_revision_content_v1(&core, &runtime)),
                vector.expected.frame_hex,
                "{}",
                vector.name
            );
            assert_eq!(
                calculate_service_free_digest(&content).unwrap().to_string(),
                vector.expected.digest,
                "{}",
                vector.name
            );
            assert_eq!(
                decode_service_free_content(&core, &runtime).unwrap(),
                content,
                "{} strict round trip",
                vector.name
            );
        }
    }

    // Test-ID: PR-TEST-0045
    // Verifies: PR-REQ-0015, PR-REQ-0016, PR-REQ-0017, PR-REQ-0079, PR-REQ-0227
    #[test]
    fn frozen_negative_vectors_reject_with_only_catalog_backed_stable_refs() {
        let catalog: Value = serde_json::from_str(ERROR_CATALOG).unwrap();
        let registered: BTreeSet<_> = catalog["codes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["owner"] == "revision_core")
            .map(|entry| entry["code"].as_str().unwrap().to_owned())
            .collect();
        for vector in manifest().invalid {
            let error = decode_invalid(&vector);
            assert_eq!(
                error.internal_code(),
                vector.expected_error,
                "{}",
                vector.name
            );
            if registered.contains(&vector.expected_error) {
                let stable = error.stable_ref().unwrap_or_else(|| {
                    panic!("{} should expose catalog-backed stable error", vector.name)
                });
                assert_eq!(stable.owner(), "revision_core");
                assert_eq!(stable.code(), vector.expected_error);
            } else {
                assert!(error.stable_ref().is_none(), "{}", vector.name);
            }
        }
    }

    // Test-ID: PR-TEST-0046
    // Verifies: PR-REQ-0227
    #[test]
    fn frozen_manifest_and_codec_independence_contract_are_explicit() {
        assert_eq!(manifest().status, "baseline");
        let cargo = include_str!("../../Cargo.toml");
        assert!(!cargo.contains("xtask ="));
        let source = include_str!("mod.rs");
        let forbidden_owned_module_path = ["xtask", "::"].concat();
        assert!(!source.contains(&forbidden_owned_module_path));
    }
}
