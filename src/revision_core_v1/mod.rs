//! Production RevisionCoreFormatV1 model projection, codec, and digest path.
//!
//! This module is intentionally independent from `xtask` implementation code.

#![allow(dead_code)]

mod raw_json;

use std::collections::BTreeMap;

use raw_json::RawJsonValue;
use sha2::{Digest, Sha256};

use crate::domain::*;

const REVISION_DIGEST_DOMAIN: &[u8] = b"pactrun.revision-content-digest\0";
const REVISION_CORE_LABEL: &[u8] = b"revision-core\0";
const RUNTIME_CONTENT_LABEL: &[u8] = b"runtime-content-closure\0";

pub(crate) fn encode_canonical_revision_core_v1(
    core: &RevisionCoreV1,
) -> Result<Vec<u8>, RevisionCoreV1Error> {
    jcs_bytes(core)
}

pub(crate) fn encode_canonical_runtime_content_v1(
    runtime_content: &RuntimeContentClosureIdentityV1,
) -> Result<Vec<u8>, RevisionCoreV1Error> {
    jcs_bytes(runtime_content)
}

pub(crate) fn decode_canonical_revision_core_v1(
    bytes: &[u8],
) -> Result<RevisionCoreV1, RevisionCoreV1Error> {
    let core = decode_semantic_revision_core_internal_v1(bytes)?;
    if encode_canonical_revision_core_v1(&core)? != bytes {
        return Err(RevisionCoreV1Error::internal(
            "noncanonical_json",
            "RevisionCoreV1 bytes are not exact Frozen JCS",
        ));
    }
    Ok(core)
}

/// Project schema-directed authoring JSON through the Frozen semantic model
/// without requiring the source bytes themselves to be canonical JSON.
pub(crate) fn project_revision_core_source_v1(
    bytes: &[u8],
) -> Result<RevisionCoreV1, RevisionCoreV1Error> {
    decode_semantic_revision_core_internal_v1(bytes)
}

pub(crate) fn decode_canonical_runtime_content_v1(
    bytes: &[u8],
) -> Result<RuntimeContentClosureIdentityV1, RevisionCoreV1Error> {
    let content = decode_semantic_runtime_content_internal_v1(bytes)?;
    if encode_canonical_runtime_content_v1(&content)? != bytes {
        return Err(RevisionCoreV1Error::internal(
            "noncanonical_json",
            "RuntimeContentClosureIdentityV1 bytes are not exact Frozen JCS",
        ));
    }
    Ok(content)
}

pub(crate) fn decode_canonical_revision_content_v1(
    core_bytes: &[u8],
    content_bytes: &[u8],
) -> Result<ValidatedRevisionContentV1, RevisionCoreV1Error> {
    validate_revision_content_v1(
        decode_canonical_revision_core_v1(core_bytes)?,
        decode_canonical_runtime_content_v1(content_bytes)?,
    )
}

pub(crate) fn calculate_revision_content_digest_v1(
    content: &ValidatedRevisionContentV1,
) -> Result<RevisionContentDigest, RevisionCoreV1Error> {
    let core_jcs = encode_canonical_revision_core_v1(&content.core)?;
    let runtime_content_jcs = encode_canonical_runtime_content_v1(&content.runtime_content)?;
    let frame = frame_revision_content_v1(&core_jcs, &runtime_content_jcs);
    let digest: [u8; 32] = Sha256::digest(frame).into();
    Ok(RevisionContentDigest::from_bytes(digest))
}

fn decode_semantic_revision_content_internal_v1(
    core_bytes: &[u8],
    content_bytes: &[u8],
) -> Result<ValidatedRevisionContentV1, RevisionCoreV1Error> {
    validate_revision_content_v1(
        decode_semantic_revision_core_internal_v1(core_bytes)?,
        decode_semantic_runtime_content_internal_v1(content_bytes)?,
    )
}

fn decode_semantic_revision_core_internal_v1(
    bytes: &[u8],
) -> Result<RevisionCoreV1, RevisionCoreV1Error> {
    project_core_value_v1(raw_json::parse_json(bytes)?)
}

/// Shared typed V1-field validation; the caller must already have strict-parsed
/// the AST. V2 uses this only for its unchanged common fields, never its identity.
pub(crate) fn project_core_value_v1(
    value: RawJsonValue,
) -> Result<RevisionCoreV1, RevisionCoreV1Error> {
    let mut object = closed_object(
        value,
        "RevisionCoreV1",
        &["format_version", "inputs", "actions", "migrations"],
        &["snapshot", "cleanup"],
    )?;
    let version = exact_integer(take_required(&mut object, "format_version")?)?;
    if version != 1 {
        return Err(RevisionCoreV1Error::internal(
            "unsupported_format_version",
            "RevisionCoreV1 format_version must be 1",
        ));
    }
    let inputs = parse_array(take_required(&mut object, "inputs")?, parse_input)?;
    let actions = parse_array(take_required(&mut object, "actions")?, parse_action)?;
    let snapshot = object.remove("snapshot").map(parse_snapshot).transpose()?;
    let migrations = parse_array(take_required(&mut object, "migrations")?, parse_migration)?;
    let cleanup = object.remove("cleanup").map(parse_cleanup).transpose()?;
    project_revision_core_v1(RevisionCoreProjectionInputV1 {
        inputs,
        actions,
        snapshot,
        migrations,
        cleanup,
    })
}

fn decode_semantic_runtime_content_internal_v1(
    bytes: &[u8],
) -> Result<RuntimeContentClosureIdentityV1, RevisionCoreV1Error> {
    let mut object = closed_object(
        raw_json::parse_json(bytes)?,
        "RuntimeContentClosureIdentityV1",
        &["files"],
        &[],
    )?;
    let files = parse_array(take_required(&mut object, "files")?, parse_runtime_file)?;
    project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files })
}

fn parse_input(value: RawJsonValue) -> Result<InputDeclarationV1, RevisionCoreV1Error> {
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

fn parse_action(value: RawJsonValue) -> Result<ActionV1, RevisionCoreV1Error> {
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
        hook: parse_hook(take_required(&mut object, "hook")?)?,
        outputs: parse_array(take_required(&mut object, "outputs")?, parse_output)?,
    })
}

fn parse_output(value: RawJsonValue) -> Result<ManagedOutputV1, RevisionCoreV1Error> {
    let mut object = closed_object(value, "ManagedOutputV1", &["id"], &[])?;
    Ok(ManagedOutputV1 {
        id: ManagedOutputIdentity::parse(take_string(&mut object, "id")?)?,
    })
}

fn parse_parameter(value: RawJsonValue) -> Result<ParameterV1, RevisionCoreV1Error> {
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
) -> Result<ParameterDefaultV1, RevisionCoreV1Error> {
    match (parameter_type, value) {
        (ParameterTypeV1::Integer, value @ RawJsonValue::Number(_)) => Ok(
            ParameterDefaultV1::Integer(SafeIntegerV1::new(exact_integer(value)?)?),
        ),
        (ParameterTypeV1::Float, RawJsonValue::Number(token)) => {
            let number = token.parse::<f64>().map_err(|_| {
                RevisionCoreV1Error::stable("invalid_number", "invalid binary64 number")
            })?;
            Ok(ParameterDefaultV1::Float(FiniteF64::new(number)?))
        }
        (ParameterTypeV1::Boolean, RawJsonValue::Bool(value)) => {
            Ok(ParameterDefaultV1::Boolean(value))
        }
        (ParameterTypeV1::String, RawJsonValue::String(value)) => {
            Ok(ParameterDefaultV1::String(value))
        }
        _ => Err(RevisionCoreV1Error::internal(
            "invalid_type",
            "parameter default does not match its declared type",
        )),
    }
}

fn parse_hook(value: RawJsonValue) -> Result<HookV1, RevisionCoreV1Error> {
    let mut object = closed_object(
        value,
        "HookV1",
        &["protocol_version", "launch", "args", "io"],
        &[],
    )?;
    Ok(HookV1 {
        protocol_version: PositiveVersion::new(exact_integer(take_required(
            &mut object,
            "protocol_version",
        )?)?)?,
        launch: parse_launch(take_required(&mut object, "launch")?)?,
        args: parse_string_array(take_required(&mut object, "args")?)?,
        io: parse_io(take_required(&mut object, "io")?)?,
    })
}

fn parse_launch(value: RawJsonValue) -> Result<HookLaunchV1, RevisionCoreV1Error> {
    let mut object = into_object(value, "HookLaunchV1")?;
    let kind = take_string(&mut object, "kind")?;
    match kind.as_str() {
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
        _ => Err(RevisionCoreV1Error::stable(
            "invalid_hook_launch",
            "Hook launch kind must be direct or interpreter",
        )),
    }
}

fn parse_io(value: RawJsonValue) -> Result<IOContractV1, RevisionCoreV1Error> {
    let mut object = closed_object(value, "IOContractV1", &["terminal"], &[])?;
    let terminal = match take_string(&mut object, "terminal")?.as_str() {
        "none" => TerminalContractV1::None,
        "output" => TerminalContractV1::Output,
        "interactive" => TerminalContractV1::Interactive,
        token => return Err(invalid_enum("TerminalContractV1", token)),
    };
    Ok(IOContractV1 { terminal })
}

fn parse_snapshot(value: RawJsonValue) -> Result<SnapshotCapabilityV1, RevisionCoreV1Error> {
    let mut object = closed_object(value, "SnapshotCapabilityV1", &[], &["capture", "restore"])?;
    Ok(SnapshotCapabilityV1 {
        capture: object.remove("capture").map(parse_capture).transpose()?,
        restore: object.remove("restore").map(parse_restore).transpose()?,
    })
}

fn parse_capture(value: RawJsonValue) -> Result<CaptureV1, RevisionCoreV1Error> {
    let mut object = closed_object(value, "CaptureV1", &["parameters", "access", "hook"], &[])?;
    Ok(CaptureV1 {
        parameters: parse_array(take_required(&mut object, "parameters")?, parse_parameter)?,
        access: take_access(&mut object)?,
        hook: parse_hook(take_required(&mut object, "hook")?)?,
    })
}

fn parse_restore(value: RawJsonValue) -> Result<RestoreV1, RevisionCoreV1Error> {
    let mut object = closed_object(value, "RestoreV1", &["parameters", "hook"], &[])?;
    Ok(RestoreV1 {
        parameters: parse_array(take_required(&mut object, "parameters")?, parse_parameter)?,
        hook: parse_hook(take_required(&mut object, "hook")?)?,
    })
}

fn parse_migration(value: RawJsonValue) -> Result<MigrationV1, RevisionCoreV1Error> {
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
        hook: object.remove("hook").map(parse_hook).transpose()?,
    })
}

fn parse_binding_ref(value: RawJsonValue) -> Result<InputBindingRefV1, RevisionCoreV1Error> {
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

fn parse_transition(value: RawJsonValue) -> Result<MigrationTransitionV1, RevisionCoreV1Error> {
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
        _ => Err(RevisionCoreV1Error::internal(
            "invalid_transition",
            "unknown Migration transition kind",
        )),
    }
}

fn parse_cleanup(value: RawJsonValue) -> Result<CleanupV1, RevisionCoreV1Error> {
    let mut object = closed_object(value, "CleanupV1", &["requires", "hook"], &[])?;
    Ok(CleanupV1 {
        requires: parse_array(take_required(&mut object, "requires")?, parse_binding_ref)?,
        hook: parse_hook(take_required(&mut object, "hook")?)?,
    })
}

fn parse_runtime_file(value: RawJsonValue) -> Result<RuntimeFileV1, RevisionCoreV1Error> {
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

pub(crate) fn exact_integer(value: RawJsonValue) -> Result<i64, RevisionCoreV1Error> {
    let RawJsonValue::Number(token) = value else {
        return Err(RevisionCoreV1Error::internal(
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
        .map_err(|_| RevisionCoreV1Error::stable("invalid_number", "number exponent is too large"))?
        .unwrap_or(0);
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut digits = format!("{whole}{fraction}");
    if digits.bytes().all(|byte| byte == b'0') {
        return Ok(0);
    }
    let fraction_length = i64::try_from(fraction.len()).map_err(|_| {
        RevisionCoreV1Error::stable("invalid_number", "integer fraction is too large")
    })?;
    let decimal_shift = exponent.checked_sub(fraction_length).ok_or_else(|| {
        RevisionCoreV1Error::stable("invalid_number", "integer scale is out of range")
    })?;
    if decimal_shift < 0 {
        let remove = usize::try_from(decimal_shift.unsigned_abs()).unwrap_or(usize::MAX);
        if remove > digits.len()
            || !digits[digits.len() - remove..]
                .bytes()
                .all(|byte| byte == b'0')
        {
            return Err(RevisionCoreV1Error::stable(
                "invalid_number",
                "integer value is not mathematically integral",
            ));
        }
        digits.truncate(digits.len() - remove);
    } else {
        let append = usize::try_from(decimal_shift).unwrap_or(usize::MAX);
        if append > 16 {
            return Err(RevisionCoreV1Error::stable(
                "invalid_number",
                "integer value exceeds the safe range",
            ));
        }
        digits.extend(std::iter::repeat_n('0', append));
    }
    let digits = digits.trim_start_matches('0');
    if digits.len() > 16 {
        return Err(RevisionCoreV1Error::stable(
            "invalid_number",
            "integer value exceeds the safe range",
        ));
    }
    let magnitude = digits.parse::<i64>().map_err(|_| {
        RevisionCoreV1Error::stable("invalid_number", "integer value exceeds the safe range")
    })?;
    if magnitude > MAX_SAFE_INTEGER {
        return Err(RevisionCoreV1Error::stable(
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
) -> Result<BTreeMap<String, RawJsonValue>, RevisionCoreV1Error> {
    let object = into_object(value, name)?;
    validate_remaining_fields(&object, name, required, optional)?;
    Ok(object)
}

fn into_object(
    value: RawJsonValue,
    name: &str,
) -> Result<BTreeMap<String, RawJsonValue>, RevisionCoreV1Error> {
    match value {
        RawJsonValue::Object(entries) => Ok(entries.into_iter().collect()),
        _ => Err(RevisionCoreV1Error::internal(
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
) -> Result<(), RevisionCoreV1Error> {
    for key in object.keys() {
        if !required.contains(&key.as_str()) && !optional.contains(&key.as_str()) {
            return Err(RevisionCoreV1Error::stable(
                "unknown_field",
                format!("unknown {name} field {key}"),
            ));
        }
    }
    for field in required {
        if !object.contains_key(*field) {
            return Err(RevisionCoreV1Error::internal(
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
) -> Result<RawJsonValue, RevisionCoreV1Error> {
    object.remove(field).ok_or_else(|| {
        RevisionCoreV1Error::internal("missing_field", format!("missing field {field}"))
    })
}

fn take_string(
    object: &mut BTreeMap<String, RawJsonValue>,
    field: &str,
) -> Result<String, RevisionCoreV1Error> {
    match take_required(object, field)? {
        RawJsonValue::String(value) => Ok(value),
        _ => Err(RevisionCoreV1Error::internal(
            "invalid_type",
            format!("{field} must be a string"),
        )),
    }
}

fn take_bool(
    object: &mut BTreeMap<String, RawJsonValue>,
    field: &str,
) -> Result<bool, RevisionCoreV1Error> {
    match take_required(object, field)? {
        RawJsonValue::Bool(value) => Ok(value),
        _ => Err(RevisionCoreV1Error::internal(
            "invalid_type",
            format!("{field} must be a boolean"),
        )),
    }
}

fn take_access(
    object: &mut BTreeMap<String, RawJsonValue>,
) -> Result<OperationAccessV1, RevisionCoreV1Error> {
    match take_string(object, "access")?.as_str() {
        "observe" => Ok(OperationAccessV1::Observe),
        "mutate" => Ok(OperationAccessV1::Mutate),
        token => Err(invalid_enum("OperationAccessV1", token)),
    }
}

fn parse_array<T>(
    value: RawJsonValue,
    parse: impl FnMut(RawJsonValue) -> Result<T, RevisionCoreV1Error>,
) -> Result<Vec<T>, RevisionCoreV1Error> {
    match value {
        RawJsonValue::Array(values) => values.into_iter().map(parse).collect(),
        _ => Err(RevisionCoreV1Error::internal(
            "invalid_type",
            "expected a JSON array",
        )),
    }
}

fn parse_string_array(value: RawJsonValue) -> Result<Vec<String>, RevisionCoreV1Error> {
    parse_array(value, |value| match value {
        RawJsonValue::String(value) => Ok(value),
        _ => Err(RevisionCoreV1Error::internal(
            "invalid_type",
            "array item must be a string",
        )),
    })
}

fn parse_id_array(value: RawJsonValue) -> Result<Vec<InputIdentity>, RevisionCoreV1Error> {
    parse_array(value, |value| match value {
        RawJsonValue::String(value) => InputIdentity::parse(value),
        _ => Err(RevisionCoreV1Error::internal(
            "invalid_type",
            "target Input identity must be a string",
        )),
    })
}

fn invalid_enum(name: &str, token: &str) -> RevisionCoreV1Error {
    RevisionCoreV1Error::internal("invalid_type", format!("unknown {name} token {token:?}"))
}

fn jcs_bytes(value: &impl serde::Serialize) -> Result<Vec<u8>, RevisionCoreV1Error> {
    serde_jcs::to_vec(value).map_err(|error| {
        RevisionCoreV1Error::internal(
            "canonicalization_failed",
            format!("RFC 8785 serialization failed: {error}"),
        )
    })
}

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
    frame.extend_from_slice(&1_u32.to_be_bytes());
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

    const VECTORS: &str = include_str!("../../tests/vectors/revision_core_format_v1/vectors.json");
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

    fn decode_invalid(vector: &InvalidVector) -> RevisionCoreV1Error {
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
    // Verifies: PR-REQ-0226, PR-REQ-0227
    #[test]
    fn revision_core_and_runtime_content_are_sibling_wire_components() {
        let vector = &manifest().valid[0];
        let content = decode_semantic_revision_content_internal_v1(
            vector.raw_core.as_bytes(),
            vector.raw_content.as_bytes(),
        )
        .unwrap();
        let core: Value =
            serde_json::from_slice(&encode_canonical_revision_core_v1(&content.core).unwrap())
                .unwrap();
        let runtime: Value = serde_json::from_slice(
            &encode_canonical_runtime_content_v1(&content.runtime_content).unwrap(),
        )
        .unwrap();
        assert_eq!(
            core.as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            ["actions", "format_version", "inputs", "migrations"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
        assert!(core.get("files").is_none());
        assert_eq!(runtime["files"][0]["kind"], "regular_file");
        assert!(runtime.get("format_version").is_none());
    }

    // Test-ID: PR-TEST-0041
    // Verifies: PR-REQ-0226
    #[test]
    fn internal_semantic_projection_matches_frozen_normalized_values() {
        for vector in manifest().valid {
            let content = decode_semantic_revision_content_internal_v1(
                vector.raw_core.as_bytes(),
                vector.raw_content.as_bytes(),
            )
            .unwrap_or_else(|error| panic!("{}: {error}", vector.name));
            assert_eq!(
                encode_canonical_revision_core_v1(&content.core).unwrap(),
                serde_jcs::to_vec(&vector.normalized_core).unwrap(),
                "{} core",
                vector.name
            );
            assert_eq!(
                encode_canonical_runtime_content_v1(&content.runtime_content).unwrap(),
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
        let direct = br#"{"format_version":1,"inputs":[],"actions":[{"id":"a","access":"observe","parameters":[],"hook":{"protocol_version":1,"launch":{"kind":"direct","executable":"hook"},"args":[],"io":{"terminal":"none"}},"outputs":[]}],"migrations":[]}"#;
        let non_executable = br#"{"files":[{"id":"hook","path":"bin/hook","kind":"regular_file","blob_digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","executable":false}]}"#;
        let error =
            decode_semantic_revision_content_internal_v1(direct, non_executable).unwrap_err();
        assert_eq!(error.internal_code(), "invalid_hook_launch");

        let interpreter = br#"{"format_version":1,"inputs":[],"actions":[{"id":"a","access":"observe","parameters":[],"hook":{"protocol_version":1,"launch":{"kind":"interpreter","command":"python.exe","interpreter_args":[],"script":"hook"},"args":[],"io":{"terminal":"none"}},"outputs":[]}],"migrations":[]}"#;
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
            br#"{"format_version":1,"format_version":1,"inputs":[],"actions":[],"migrations":[]}"#,
        )
        .unwrap_err();
        assert_eq!(duplicate.internal_code(), "duplicate_property");
        assert_eq!(duplicate.stable_ref().unwrap().code(), "duplicate_property");
        let invalid_scalar = decode_semantic_revision_core_internal_v1(
            br#"{"format_version":1,"inputs":[],"actions":[],"migrations":[],"bad":"\uD800"}"#,
        )
        .unwrap_err();
        assert_eq!(invalid_scalar.internal_code(), "invalid_unicode_scalar");

        let decoded_duplicate = decode_semantic_revision_core_internal_v1(
            br#"{"format_version":1,"inputs":[],"actions":[],"migrations":[],"x":1,"\u0078":2}"#,
        )
        .unwrap_err();
        assert_eq!(decoded_duplicate.internal_code(), "duplicate_property");

        let mut excessive_depth =
            String::from(r#"{"format_version":1,"inputs":[],"actions":[],"migrations":[],"x":"#);
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
        let core_jcs = encode_canonical_revision_core_v1(&content.core).unwrap();
        assert!(decode_canonical_revision_core_v1(&core_jcs).is_ok());
        assert!(decode_canonical_revision_core_v1(vector.raw_core.as_bytes()).is_err());

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
            encode_canonical_revision_core_v1(&normalized.core).unwrap(),
            hex::decode(numbers.expected.core_jcs_hex).unwrap()
        );
    }

    // Test-ID: PR-TEST-0044
    // Verifies: PR-REQ-0227
    #[test]
    fn production_codec_reproduces_all_frozen_bytes_frames_and_digests() {
        for vector in manifest().valid {
            let content = decode_semantic_revision_content_internal_v1(
                vector.raw_core.as_bytes(),
                vector.raw_content.as_bytes(),
            )
            .unwrap_or_else(|error| panic!("{}: {error}", vector.name));
            let core = encode_canonical_revision_core_v1(&content.core).unwrap();
            let runtime = encode_canonical_runtime_content_v1(&content.runtime_content).unwrap();
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
                calculate_revision_content_digest_v1(&content)
                    .unwrap()
                    .to_string(),
                vector.expected.digest,
                "{}",
                vector.name
            );
            assert_eq!(
                decode_canonical_revision_content_v1(&core, &runtime).unwrap(),
                content,
                "{} strict round trip",
                vector.name
            );
        }
    }

    // Test-ID: PR-TEST-0045
    // Verifies: PR-REQ-0227
    #[test]
    fn frozen_negative_vectors_reject_with_only_catalog_backed_stable_refs() {
        let catalog: Value = serde_json::from_str(ERROR_CATALOG).unwrap();
        let registered: BTreeSet<_> = catalog["codes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["owner"] == "revision_core_format_v1")
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
                assert_eq!(stable.owner(), "revision_core_format_v1");
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
        assert_eq!(manifest().status, "frozen");
        let cargo = include_str!("../../Cargo.toml");
        assert!(!cargo.contains("xtask ="));
        let source = include_str!("mod.rs");
        let forbidden_owned_module_path = ["xtask", "::"].concat();
        assert!(!source.contains(&forbidden_owned_module_path));
    }
}
