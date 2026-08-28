use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::Path,
    process::Command,
};

const VECTOR_PATH: &str = "tests/vectors/snapshot_integrity_format_v1/vectors.json";
const NODE_ORACLE_PATH: &str = "tests/oracles/snapshot_integrity_format_v1.mjs";
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
enum RelationalExpectation {
    Valid,
    NotEvaluated,
}

#[derive(Debug, Deserialize)]
struct VectorManifest {
    format: String,
    status: String,
    valid: Vec<ValidVector>,
    rfc8785: Vec<Rfc8785Vector>,
    invalid: Vec<InvalidVector>,
}

#[derive(Debug, Deserialize)]
struct Rfc8785Vector {
    name: String,
    input: Value,
    expected: String,
}

#[derive(Debug, Deserialize)]
struct ValidVector {
    name: String,
    raw_manifest: String,
    normalized_manifest: Value,
    #[serde(default)]
    producer_context: Option<ProducerContext>,
    #[serde(default)]
    blob_contents: BTreeMap<String, String>,
    relational: RelationalExpectation,
    expected: ExpectedOutput,
}

#[derive(Debug, Deserialize)]
struct InvalidVector {
    name: String,
    raw_manifest: String,
    #[serde(default)]
    producer_context: Option<ProducerContext>,
    #[serde(default)]
    blob_contents: BTreeMap<String, String>,
    expected_error: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ProducerContext {
    package_id: String,
    revision_content_digest: String,
    inputs: Vec<ProducerInput>,
}

#[derive(Clone, Debug, Deserialize)]
struct ProducerInput {
    id: String,
    protection: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
struct ExpectedOutput {
    manifest_jcs_hex: String,
    frame_hex: String,
    digest: String,
}

#[derive(Clone, Debug, PartialEq)]
enum RawValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<RawValue>),
    Object(BTreeMap<String, RawValue>),
}

#[derive(Debug)]
struct VerifyError {
    code: &'static str,
    message: String,
}

impl VerifyError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug)]
struct NormalizedManifest {
    manifest: Value,
    relational: RelationalExpectation,
}

pub fn verify(workspace_root: &Path) -> Result<(), String> {
    let vectors = load_vectors(workspace_root)?;
    verify_vector_metadata(&vectors)?;

    for vector in &vectors.valid {
        let normalized = normalize(
            &vector.raw_manifest,
            vector.producer_context.as_ref(),
            &vector.blob_contents,
        )
        .map_err(|error| format_vector_error(&vector.name, error))?;

        if jcs_bytes(&normalized.manifest)? != jcs_bytes(&vector.normalized_manifest)? {
            return Err(format!(
                "valid vector {} normalized manifest mismatch\nexpected: {}\nactual: {}",
                vector.name,
                serde_json::to_string(&vector.normalized_manifest).unwrap_or_default(),
                serde_json::to_string(&normalized.manifest).unwrap_or_default()
            ));
        }
        if normalized.relational != vector.relational {
            return Err(format!(
                "valid vector {} relational result mismatch: expected {:?}, got {:?}",
                vector.name, vector.relational, normalized.relational
            ));
        }

        let actual = calculate_output(&normalized.manifest)?;
        if actual != vector.expected {
            return Err(format!(
                "valid vector {} byte/digest mismatch\nexpected: {:?}\nactual: {:?}",
                vector.name, vector.expected, actual
            ));
        }
    }

    for vector in &vectors.invalid {
        match normalize(
            &vector.raw_manifest,
            vector.producer_context.as_ref(),
            &vector.blob_contents,
        ) {
            Ok(_) => {
                return Err(format!(
                    "invalid vector {} unexpectedly passed",
                    vector.name
                ));
            }
            Err(error) if error.code == vector.expected_error => {}
            Err(error) => {
                return Err(format!(
                    "invalid vector {} expected {}, got {}: {}",
                    vector.name, vector.expected_error, error.code, error.message
                ));
            }
        }
    }

    for vector in &vectors.rfc8785 {
        let actual = String::from_utf8(jcs_bytes(&vector.input)?)
            .map_err(|error| format!("RFC 8785 vector {} is not UTF-8: {error}", vector.name))?;
        if actual != vector.expected {
            return Err(format!(
                "RFC 8785 vector {} mismatch\nexpected: {}\nactual: {}",
                vector.name, vector.expected, actual
            ));
        }
    }

    super::revision_core_v1::verify_traceability(workspace_root)?;
    verify_node_oracle(workspace_root)?;
    eprintln!(
        "SnapshotIntegrityFormatV1: {} valid, {} invalid, and {} RFC 8785 vectors passed Rust, traceability, and Node parity",
        vectors.valid.len(),
        vectors.invalid.len(),
        vectors.rfc8785.len()
    );
    Ok(())
}

pub fn calculate(workspace_root: &Path) -> Result<(), String> {
    let vectors = load_vectors(workspace_root)?;
    let mut outputs = BTreeMap::new();
    for vector in &vectors.valid {
        let normalized = normalize(
            &vector.raw_manifest,
            vector.producer_context.as_ref(),
            &vector.blob_contents,
        )
        .map_err(|error| format_vector_error(&vector.name, error))?;
        outputs.insert(vector.name.clone(), calculate_output(&normalized.manifest)?);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&outputs).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn load_vectors(workspace_root: &Path) -> Result<VectorManifest, String> {
    let path = workspace_root.join(VECTOR_PATH);
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))
}

fn verify_vector_metadata(vectors: &VectorManifest) -> Result<(), String> {
    if vectors.format != "snapshot_integrity_format_v1_golden_vectors" {
        return Err("unexpected SnapshotIntegrityFormatV1 vector manifest kind".to_owned());
    }
    if vectors.status != "candidate" && vectors.status != "frozen" {
        return Err("vector manifest status must be candidate or frozen".to_owned());
    }
    let mut names = BTreeSet::new();
    for name in vectors
        .valid
        .iter()
        .map(|vector| &vector.name)
        .chain(vectors.invalid.iter().map(|vector| &vector.name))
    {
        if !names.insert(name) {
            return Err(format!("duplicate vector name: {name}"));
        }
    }
    Ok(())
}

fn format_vector_error(name: &str, error: VerifyError) -> String {
    format!(
        "vector {name} failed with {}: {}",
        error.code, error.message
    )
}

fn normalize(
    raw_manifest: &str,
    producer_context: Option<&ProducerContext>,
    blob_contents: &BTreeMap<String, String>,
) -> Result<NormalizedManifest, VerifyError> {
    let manifest = normalize_manifest(parse_raw_json(raw_manifest)?)?;
    let relational = validate_producer_context(&manifest, producer_context)?;
    validate_blob_contents(&manifest, blob_contents)?;
    Ok(NormalizedManifest {
        manifest,
        relational,
    })
}

fn normalize_manifest(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "SnapshotIntegrityManifestV1")?;
    validate_fields(
        &object,
        &[
            "format_version",
            "snapshot_id",
            "producer",
            "origin_instance_id",
            "captured_at",
            "managed_bindings",
            "service_content",
        ],
        &[],
    )?;

    let format_version = exact_integer(required(&object, "format_version")?)?;
    if format_version != 1 {
        return Err(VerifyError::new(
            "invalid_format_version",
            "SnapshotIntegrityManifestV1 format_version must be 1",
        ));
    }

    let mut managed_bindings =
        expect_array_ref(required(&object, "managed_bindings")?, "managed_bindings")?
            .iter()
            .cloned()
            .map(normalize_binding)
            .collect::<Result<Vec<_>, _>>()?;
    sort_unique_bindings(&mut managed_bindings)?;

    let mut service_content =
        expect_array_ref(required(&object, "service_content")?, "service_content")?
            .iter()
            .cloned()
            .map(normalize_service_content)
            .collect::<Result<Vec<_>, _>>()?;
    sort_unique_service_content(&mut service_content)?;

    object_value([
        ("format_version", Value::Number(Number::from(1))),
        (
            "snapshot_id",
            Value::String(resource_id(required_string(&object, "snapshot_id")?)?),
        ),
        (
            "producer",
            normalize_revision_identity(required(&object, "producer")?.clone())?,
        ),
        (
            "origin_instance_id",
            Value::String(resource_id(required_string(
                &object,
                "origin_instance_id",
            )?)?),
        ),
        (
            "captured_at",
            normalize_timestamp(required(&object, "captured_at")?.clone())?,
        ),
        ("managed_bindings", Value::Array(managed_bindings)),
        ("service_content", Value::Array(service_content)),
    ])
}

fn normalize_revision_identity(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "RevisionIdentityV1")?;
    validate_fields(&object, &["package_id", "revision_content_digest"], &[])?;
    object_value([
        (
            "package_id",
            Value::String(resource_id(required_string(&object, "package_id")?)?),
        ),
        (
            "revision_content_digest",
            Value::String(sha256_digest(required_string(
                &object,
                "revision_content_digest",
            )?)?),
        ),
    ])
}

fn normalize_timestamp(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "TimestampV1")?;
    validate_fields(&object, &["unix_seconds", "nanoseconds"], &[])?;
    let unix_seconds = exact_integer(required(&object, "unix_seconds")?)?;
    let nanoseconds = exact_integer(required(&object, "nanoseconds")?)?;
    if !(0..=999_999_999).contains(&nanoseconds) {
        return Err(VerifyError::new(
            "invalid_number",
            "TimestampV1 nanoseconds must be in 0..=999999999",
        ));
    }
    object_value([
        ("unix_seconds", Value::Number(Number::from(unix_seconds))),
        ("nanoseconds", Value::Number(Number::from(nanoseconds))),
    ])
}

fn normalize_binding(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "ManagedBindingStateV1")?;
    let state = enum_string(required_string(&object, "state")?, &["absent", "bound"])?;
    let role = enum_string(required_string(&object, "role")?, &["active", "retained"])?;
    let input_id = semantic_identifier(required_string(&object, "input_id")?)?;
    let protection = enum_string(
        required_string(&object, "protection")?,
        &["normal", "secret"],
    )?;

    if state == "absent" {
        validate_fields(&object, &["input_id", "role", "state", "protection"], &[])?;
        if role == "retained" {
            return Err(VerifyError::new(
                "invalid_binding_state",
                "a retained binding cannot be absent",
            ));
        }
        object_value([
            ("input_id", Value::String(input_id)),
            ("role", Value::String(role)),
            ("state", Value::String(state)),
            ("protection", Value::String(protection)),
        ])
    } else {
        validate_fields(
            &object,
            &["input_id", "role", "state", "protection", "blob_digest"],
            &[],
        )?;
        object_value([
            ("input_id", Value::String(input_id)),
            ("role", Value::String(role)),
            ("state", Value::String(state)),
            ("protection", Value::String(protection)),
            (
                "blob_digest",
                Value::String(sha256_digest(required_string(&object, "blob_digest")?)?),
            ),
        ])
    }
}

fn normalize_service_content(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "ServiceContentV1")?;
    validate_fields(&object, &["role", "path", "blob_digest"], &[])?;
    object_value([
        (
            "role",
            Value::String(semantic_identifier(required_string(&object, "role")?)?),
        ),
        (
            "path",
            Value::String(snapshot_path(required_string(&object, "path")?)?),
        ),
        (
            "blob_digest",
            Value::String(sha256_digest(required_string(&object, "blob_digest")?)?),
        ),
    ])
}

fn sort_unique_bindings(bindings: &mut [Value]) -> Result<(), VerifyError> {
    bindings
        .sort_by(|left, right| string_field(left, "input_id").cmp(string_field(right, "input_id")));
    for pair in bindings.windows(2) {
        if string_field(&pair[0], "input_id") == string_field(&pair[1], "input_id") {
            return Err(VerifyError::new(
                "duplicate_semantic_key",
                "duplicate managed binding input_id",
            ));
        }
    }
    Ok(())
}

fn sort_unique_service_content(content: &mut [Value]) -> Result<(), VerifyError> {
    content.sort_by(|left, right| service_content_key(left).cmp(&service_content_key(right)));
    for pair in content.windows(2) {
        if service_content_key(&pair[0]) == service_content_key(&pair[1]) {
            return Err(VerifyError::new(
                "duplicate_semantic_key",
                "duplicate service content role/path",
            ));
        }
    }
    Ok(())
}

fn service_content_key(value: &Value) -> (&str, &str) {
    (string_field(value, "role"), string_field(value, "path"))
}

fn string_field<'a>(value: &'a Value, field: &str) -> &'a str {
    value.get(field).and_then(Value::as_str).unwrap_or_default()
}

fn validate_producer_context(
    manifest: &Value,
    context: Option<&ProducerContext>,
) -> Result<RelationalExpectation, VerifyError> {
    let Some(context) = context else {
        return Ok(RelationalExpectation::NotEvaluated);
    };

    let package_id = resource_id(context.package_id.clone())?;
    let revision_digest = sha256_digest(context.revision_content_digest.clone())?;
    if manifest["producer"]["package_id"].as_str() != Some(&package_id)
        || manifest["producer"]["revision_content_digest"].as_str() != Some(&revision_digest)
    {
        return Err(VerifyError::new(
            "invalid_producer_context",
            "supplied producer context does not identify the manifest producer",
        ));
    }

    let mut declarations = BTreeMap::new();
    for input in &context.inputs {
        let id = semantic_identifier(input.id.clone())?;
        let protection = enum_string(input.protection.clone(), &["normal", "secret"])?;
        if declarations.insert(id, protection).is_some() {
            return Err(VerifyError::new(
                "invalid_producer_context",
                "producer context contains duplicate Input identities",
            ));
        }
    }

    let bindings = manifest["managed_bindings"]
        .as_array()
        .expect("normalized bindings are an array");
    let by_id: BTreeMap<_, _> = bindings
        .iter()
        .map(|binding| (string_field(binding, "input_id"), binding))
        .collect();

    for (id, protection) in &declarations {
        let Some(binding) = by_id.get(id.as_str()) else {
            return Err(VerifyError::new(
                "incomplete_binding_state",
                format!("missing active descriptor for producer Input {id}"),
            ));
        };
        if binding["role"] != "active" {
            return Err(VerifyError::new(
                "invalid_binding_role",
                format!("producer Input {id} is not represented as active"),
            ));
        }
        if binding["protection"].as_str() != Some(protection) {
            return Err(VerifyError::new(
                "invalid_binding_protection",
                format!("active Input {id} protection differs from producer semantics"),
            ));
        }
    }

    for binding in bindings {
        let id = string_field(binding, "input_id");
        let declared = declarations.contains_key(id);
        match string_field(binding, "role") {
            "active" if !declared => {
                return Err(VerifyError::new(
                    "invalid_binding_role",
                    format!("active Input {id} is not declared by the producer"),
                ));
            }
            "retained" if declared => {
                return Err(VerifyError::new(
                    "invalid_binding_role",
                    format!("retained Input {id} is declared by the producer"),
                ));
            }
            _ => {}
        }
    }

    Ok(RelationalExpectation::Valid)
}

fn validate_blob_contents(
    manifest: &Value,
    blob_contents: &BTreeMap<String, String>,
) -> Result<(), VerifyError> {
    for (digest, bytes_hex) in blob_contents {
        sha256_digest(digest.clone())?;
        let bytes = hex::decode(bytes_hex).map_err(|_| {
            VerifyError::new(
                "invalid_blob_fixture",
                "blob fixture must be lowercase hexadecimal",
            )
        })?;
        if bytes_hex.bytes().any(|byte| matches!(byte, b'A'..=b'F')) {
            return Err(VerifyError::new(
                "invalid_blob_fixture",
                "blob fixture must use lowercase hexadecimal",
            ));
        }
        let actual = format!("sha256:{}", hex::encode(Sha256::digest(bytes)));
        if &actual != digest {
            return Err(VerifyError::new(
                "content_digest_mismatch",
                format!("blob fixture does not match declared digest {digest}"),
            ));
        }
    }

    let binding_digests = manifest["managed_bindings"]
        .as_array()
        .expect("normalized bindings are an array")
        .iter()
        .filter_map(|binding| binding.get("blob_digest").and_then(Value::as_str));
    let service_digests = manifest["service_content"]
        .as_array()
        .expect("normalized service content is an array")
        .iter()
        .filter_map(|content| content.get("blob_digest").and_then(Value::as_str));
    for digest in binding_digests.chain(service_digests) {
        if !blob_contents.contains_key(digest) {
            return Err(VerifyError::new(
                "missing_content_blob",
                format!("no blob fixture supplied for {digest}"),
            ));
        }
    }
    Ok(())
}

fn calculate_output(manifest: &Value) -> Result<ExpectedOutput, String> {
    let manifest_jcs = jcs_bytes(manifest)?;
    let frame = frame(&manifest_jcs);
    let digest = Sha256::digest(&frame);
    Ok(ExpectedOutput {
        manifest_jcs_hex: hex::encode(manifest_jcs),
        frame_hex: hex::encode(frame),
        digest: format!("sha256:{}", hex::encode(digest)),
    })
}

fn jcs_bytes(value: &Value) -> Result<Vec<u8>, String> {
    serde_jcs::to_vec(value).map_err(|error| error.to_string())
}

fn frame(manifest_jcs: &[u8]) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(b"pactrun.snapshot-integrity-digest\0");
    frame.extend_from_slice(&1_u32.to_be_bytes());
    frame.extend_from_slice(b"snapshot-integrity-manifest\0");
    frame.extend_from_slice(&(manifest_jcs.len() as u64).to_be_bytes());
    frame.extend_from_slice(manifest_jcs);
    frame
}

fn resource_id(value: String) -> Result<String, VerifyError> {
    if value.len() != 32
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(VerifyError::new(
            "invalid_resource_id",
            "resource identity must contain exactly 32 lowercase hexadecimal characters",
        ));
    }
    Ok(value)
}

fn semantic_identifier(value: String) -> Result<String, VerifyError> {
    if value.is_empty() || value.len() > 128 || !value.is_ascii() {
        return Err(VerifyError::new(
            "invalid_identifier",
            format!("invalid identifier {value:?}"),
        ));
    }
    let bytes = value.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return Err(VerifyError::new(
            "invalid_identifier",
            format!("invalid identifier {value:?}"),
        ));
    }
    let mut previous_separator = false;
    for &byte in &bytes[1..] {
        let separator = matches!(byte, b'.' | b'_' | b'-');
        if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || separator)
            || (separator && previous_separator)
        {
            return Err(VerifyError::new(
                "invalid_identifier",
                format!("invalid identifier {value:?}"),
            ));
        }
        previous_separator = separator;
    }
    if previous_separator {
        return Err(VerifyError::new(
            "invalid_identifier",
            format!("invalid identifier {value:?}"),
        ));
    }
    Ok(value)
}

fn snapshot_path(value: String) -> Result<String, VerifyError> {
    if value.is_empty() || value.len() > 1024 || !value.is_ascii() {
        return Err(VerifyError::new(
            "invalid_snapshot_path",
            format!("invalid Snapshot content path {value:?}"),
        ));
    }
    for segment in value.split('/') {
        if semantic_identifier(segment.to_owned()).is_err() || is_windows_reserved(segment) {
            return Err(VerifyError::new(
                "invalid_snapshot_path",
                format!("invalid Snapshot content path {value:?}"),
            ));
        }
    }
    Ok(value)
}

fn is_windows_reserved(value: &str) -> bool {
    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

fn sha256_digest(value: String) -> Result<String, VerifyError> {
    let Some(hex_value) = value.strip_prefix("sha256:") else {
        return Err(VerifyError::new(
            "invalid_digest",
            "digest must use sha256 prefix",
        ));
    };
    if hex_value.len() != 64
        || !hex_value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(VerifyError::new(
            "invalid_digest",
            "digest must contain 64 lowercase hexadecimal characters",
        ));
    }
    Ok(value)
}

fn exact_integer(value: &RawValue) -> Result<i64, VerifyError> {
    let RawValue::Number(token) = value else {
        return Err(VerifyError::new("invalid_type", "expected a JSON number"));
    };
    let negative = token.starts_with('-');
    let unsigned = token.strip_prefix('-').unwrap_or(token);
    let (mantissa, exponent_text) = unsigned
        .split_once(['e', 'E'])
        .map_or((unsigned, None), |(left, right)| (left, Some(right)));
    let exponent = exponent_text
        .map(|value| value.parse::<i64>())
        .transpose()
        .map_err(|_| VerifyError::new("invalid_number", "integer exponent is too large"))?
        .unwrap_or(0);
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut digits = format!("{whole}{fraction}");
    if digits.bytes().all(|byte| byte == b'0') {
        return Ok(0);
    }
    let fraction_len = i64::try_from(fraction.len())
        .map_err(|_| VerifyError::new("invalid_number", "integer fraction is too long"))?;
    let decimal_shift = exponent.checked_sub(fraction_len).ok_or_else(|| {
        VerifyError::new("invalid_number", "integer decimal shift is out of range")
    })?;
    if decimal_shift < 0 {
        let remove = decimal_shift
            .checked_neg()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| {
                VerifyError::new("invalid_number", "integer decimal shift is out of range")
            })?;
        if remove > digits.len()
            || !digits[digits.len() - remove..]
                .bytes()
                .all(|byte| byte == b'0')
        {
            return Err(VerifyError::new(
                "invalid_number",
                "integer value is not mathematically integral",
            ));
        }
        digits.truncate(digits.len() - remove);
    } else {
        let append = usize::try_from(decimal_shift).unwrap_or(usize::MAX);
        if append > 32 {
            return Err(VerifyError::new(
                "invalid_number",
                "integer value exceeds the safe range",
            ));
        }
        digits.extend(std::iter::repeat_n('0', append));
    }
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return Ok(0);
    }
    if digits.len() > 16 {
        return Err(VerifyError::new(
            "invalid_number",
            "integer value exceeds the safe range",
        ));
    }
    let magnitude = digits
        .parse::<i64>()
        .map_err(|_| VerifyError::new("invalid_number", "integer value exceeds the safe range"))?;
    if magnitude > MAX_SAFE_INTEGER {
        return Err(VerifyError::new(
            "invalid_number",
            "integer value exceeds the safe range",
        ));
    }
    Ok(if negative { -magnitude } else { magnitude })
}

fn expect_object(value: RawValue, name: &str) -> Result<BTreeMap<String, RawValue>, VerifyError> {
    match value {
        RawValue::Object(value) => Ok(value),
        _ => Err(VerifyError::new(
            "invalid_type",
            format!("{name} must be an object"),
        )),
    }
}

fn expect_array_ref<'a>(value: &'a RawValue, name: &str) -> Result<&'a [RawValue], VerifyError> {
    match value {
        RawValue::Array(value) => Ok(value),
        _ => Err(VerifyError::new(
            "invalid_type",
            format!("{name} must be an array"),
        )),
    }
}

fn validate_fields(
    object: &BTreeMap<String, RawValue>,
    required_fields: &[&str],
    optional_fields: &[&str],
) -> Result<(), VerifyError> {
    for key in object.keys() {
        if !required_fields.contains(&key.as_str()) && !optional_fields.contains(&key.as_str()) {
            return Err(VerifyError::new(
                "unknown_field",
                format!("unknown field {key}"),
            ));
        }
    }
    for field in required_fields {
        if !object.contains_key(*field) {
            return Err(VerifyError::new(
                "missing_field",
                format!("missing field {field}"),
            ));
        }
    }
    Ok(())
}

fn required<'a>(
    object: &'a BTreeMap<String, RawValue>,
    field: &str,
) -> Result<&'a RawValue, VerifyError> {
    object
        .get(field)
        .ok_or_else(|| VerifyError::new("missing_field", format!("missing field {field}")))
}

fn required_string(
    object: &BTreeMap<String, RawValue>,
    field: &str,
) -> Result<String, VerifyError> {
    match required(object, field)? {
        RawValue::String(value) => Ok(value.clone()),
        _ => Err(VerifyError::new(
            "invalid_type",
            format!("{field} must be a string"),
        )),
    }
}

fn enum_string(value: String, allowed: &[&str]) -> Result<String, VerifyError> {
    if allowed.contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(VerifyError::new(
            "invalid_type",
            format!("unknown enum token {value}"),
        ))
    }
}

fn object_value<const N: usize>(fields: [(&str, Value); N]) -> Result<Value, VerifyError> {
    Ok(Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect::<Map<_, _>>(),
    ))
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    position: usize,
}

fn parse_raw_json(input: &str) -> Result<RawValue, VerifyError> {
    if std::str::from_utf8(input.as_bytes()).is_err() {
        return Err(VerifyError::new(
            "invalid_unicode_scalar",
            "JSON text is not valid UTF-8",
        ));
    }
    let mut parser = JsonParser {
        bytes: input.as_bytes(),
        position: 0,
    };
    let value = parser.parse_value()?;
    parser.skip_whitespace();
    if parser.position != parser.bytes.len() {
        return Err(VerifyError::new(
            "invalid_json_syntax",
            "trailing data after JSON value",
        ));
    }
    Ok(value)
}

impl JsonParser<'_> {
    fn parse_value(&mut self) -> Result<RawValue, VerifyError> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'n') => {
                self.expect_keyword(b"null")?;
                Ok(RawValue::Null)
            }
            Some(b't') => {
                self.expect_keyword(b"true")?;
                Ok(RawValue::Bool(true))
            }
            Some(b'f') => {
                self.expect_keyword(b"false")?;
                Ok(RawValue::Bool(false))
            }
            Some(b'"') => self.parse_string().map(RawValue::String),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-' | b'0'..=b'9') => self.parse_number().map(RawValue::Number),
            _ => Err(VerifyError::new(
                "invalid_json_syntax",
                "expected a JSON value",
            )),
        }
    }

    fn parse_array(&mut self) -> Result<RawValue, VerifyError> {
        self.position += 1;
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.take(b']') {
            return Ok(RawValue::Array(values));
        }
        loop {
            values.push(self.parse_value()?);
            self.skip_whitespace();
            if self.take(b']') {
                break;
            }
            self.expect_byte(b',')?;
        }
        Ok(RawValue::Array(values))
    }

    fn parse_object(&mut self) -> Result<RawValue, VerifyError> {
        self.position += 1;
        let mut fields = BTreeMap::new();
        self.skip_whitespace();
        if self.take(b'}') {
            return Ok(RawValue::Object(fields));
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                return Err(VerifyError::new(
                    "invalid_json_syntax",
                    "object key must be a string",
                ));
            }
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect_byte(b':')?;
            let value = self.parse_value()?;
            if fields.insert(key.clone(), value).is_some() {
                return Err(VerifyError::new(
                    "duplicate_property",
                    format!("duplicate object property {key:?}"),
                ));
            }
            self.skip_whitespace();
            if self.take(b'}') {
                break;
            }
            self.expect_byte(b',')?;
        }
        Ok(RawValue::Object(fields))
    }

    fn parse_string(&mut self) -> Result<String, VerifyError> {
        self.expect_byte(b'"')?;
        let mut output = String::new();
        loop {
            let byte = self.peek().ok_or_else(|| {
                VerifyError::new("invalid_json_syntax", "unterminated JSON string")
            })?;
            match byte {
                b'"' => {
                    self.position += 1;
                    return Ok(output);
                }
                b'\\' => {
                    self.position += 1;
                    let escaped = self.peek().ok_or_else(|| {
                        VerifyError::new("invalid_json_syntax", "unterminated JSON escape")
                    })?;
                    self.position += 1;
                    match escaped {
                        b'"' => output.push('"'),
                        b'\\' => output.push('\\'),
                        b'/' => output.push('/'),
                        b'b' => output.push('\u{0008}'),
                        b'f' => output.push('\u{000c}'),
                        b'n' => output.push('\n'),
                        b'r' => output.push('\r'),
                        b't' => output.push('\t'),
                        b'u' => self.parse_unicode_escape(&mut output)?,
                        _ => {
                            return Err(VerifyError::new(
                                "invalid_json_syntax",
                                "unknown JSON string escape",
                            ));
                        }
                    }
                }
                0x00..=0x1f => {
                    return Err(VerifyError::new(
                        "invalid_json_syntax",
                        "unescaped control character in string",
                    ));
                }
                0x20..=0x7f => {
                    self.position += 1;
                    output.push(char::from(byte));
                }
                _ => {
                    let rest = std::str::from_utf8(&self.bytes[self.position..]).map_err(|_| {
                        VerifyError::new("invalid_unicode_scalar", "invalid UTF-8 in JSON string")
                    })?;
                    let character = rest.chars().next().ok_or_else(|| {
                        VerifyError::new("invalid_unicode_scalar", "invalid Unicode scalar")
                    })?;
                    self.position += character.len_utf8();
                    output.push(character);
                }
            }
        }
    }

    fn parse_unicode_escape(&mut self, output: &mut String) -> Result<(), VerifyError> {
        let first = self.parse_hex_quad()?;
        let scalar = if (0xd800..=0xdbff).contains(&first) {
            if self.peek() != Some(b'\\') || self.bytes.get(self.position + 1) != Some(&b'u') {
                return Err(VerifyError::new(
                    "invalid_unicode_scalar",
                    "lone high surrogate",
                ));
            }
            self.position += 2;
            let second = self.parse_hex_quad()?;
            if !(0xdc00..=0xdfff).contains(&second) {
                return Err(VerifyError::new(
                    "invalid_unicode_scalar",
                    "high surrogate is not followed by a low surrogate",
                ));
            }
            0x10000 + (((first as u32 - 0xd800) << 10) | (second as u32 - 0xdc00))
        } else if (0xdc00..=0xdfff).contains(&first) {
            return Err(VerifyError::new(
                "invalid_unicode_scalar",
                "lone low surrogate",
            ));
        } else {
            first as u32
        };
        output.push(
            char::from_u32(scalar).ok_or_else(|| {
                VerifyError::new("invalid_unicode_scalar", "invalid Unicode scalar")
            })?,
        );
        Ok(())
    }

    fn parse_hex_quad(&mut self) -> Result<u16, VerifyError> {
        if self.position + 4 > self.bytes.len() {
            return Err(VerifyError::new(
                "invalid_json_syntax",
                "short Unicode escape",
            ));
        }
        let mut value = 0_u16;
        for _ in 0..4 {
            let digit = self.bytes[self.position];
            self.position += 1;
            value = value
                .checked_mul(16)
                .and_then(|current| {
                    let value = match digit {
                        b'0'..=b'9' => digit - b'0',
                        b'a'..=b'f' => digit - b'a' + 10,
                        b'A'..=b'F' => digit - b'A' + 10,
                        _ => return None,
                    };
                    current.checked_add(u16::from(value))
                })
                .ok_or_else(|| VerifyError::new("invalid_json_syntax", "invalid Unicode escape"))?;
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<String, VerifyError> {
        let start = self.position;
        self.take(b'-');
        match self.peek() {
            Some(b'0') => {
                self.position += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return Err(VerifyError::new(
                        "invalid_json_syntax",
                        "leading zero in JSON number",
                    ));
                }
            }
            Some(b'1'..=b'9') => {
                self.position += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.position += 1;
                }
            }
            _ => {
                return Err(VerifyError::new(
                    "invalid_json_syntax",
                    "invalid JSON number integer part",
                ));
            }
        }
        if self.take(b'.') {
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(VerifyError::new(
                    "invalid_json_syntax",
                    "fraction requires a digit",
                ));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.position += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.position += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.position += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(VerifyError::new(
                    "invalid_json_syntax",
                    "exponent requires a digit",
                ));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.position += 1;
            }
        }
        std::str::from_utf8(&self.bytes[start..self.position])
            .map(str::to_owned)
            .map_err(|_| VerifyError::new("invalid_json_syntax", "invalid number token"))
    }

    fn expect_keyword(&mut self, keyword: &[u8]) -> Result<(), VerifyError> {
        if self.bytes.get(self.position..self.position + keyword.len()) == Some(keyword) {
            self.position += keyword.len();
            Ok(())
        } else {
            Err(VerifyError::new(
                "invalid_json_syntax",
                "invalid JSON keyword",
            ))
        }
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), VerifyError> {
        self.skip_whitespace();
        if self.take(expected) {
            Ok(())
        } else {
            Err(VerifyError::new(
                "invalid_json_syntax",
                format!("expected byte {expected:?}"),
            ))
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.position += 1;
        }
    }

    fn take(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }
}

fn verify_node_oracle(workspace_root: &Path) -> Result<(), String> {
    let node = env::var_os("PACTRUN_NODE").unwrap_or_else(|| "node".into());
    let version_output = Command::new(&node)
        .arg("--version")
        .current_dir(workspace_root)
        .output()
        .map_err(|error| format!("could not start Node 24 oracle: {error}"))?;
    if !version_output.status.success() {
        return Err("node --version failed".to_owned());
    }
    let version = String::from_utf8_lossy(&version_output.stdout);
    if !version.trim_start().starts_with("v24.") {
        return Err(format!(
            "SnapshotIntegrityFormatV1 oracle requires Node 24, found {}",
            version.trim()
        ));
    }
    let status = Command::new(&node)
        .arg(workspace_root.join(NODE_ORACLE_PATH))
        .arg(workspace_root.join(VECTOR_PATH))
        .current_dir(workspace_root)
        .status()
        .map_err(|error| format!("could not start Node oracle: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Node oracle exited with {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask is a direct workspace child")
            .to_path_buf()
    }

    fn vectors() -> VectorManifest {
        load_vectors(&workspace_root()).expect("load vectors")
    }

    // Test-ID: PR-TEST-0013
    // Verifies: PR-REQ-0080, PR-REQ-0195, PR-REQ-0196, PR-REQ-0197
    #[test]
    fn frozen_schema_and_minimal_vector_are_closed() {
        let vectors = vectors();
        assert_eq!(vectors.status, "frozen");
        let minimal = vectors
            .valid
            .iter()
            .find(|vector| vector.name == "minimal_manifest")
            .unwrap();
        let normalized = normalize(
            &minimal.raw_manifest,
            minimal.producer_context.as_ref(),
            &minimal.blob_contents,
        )
        .unwrap();
        assert_eq!(normalized.manifest, minimal.normalized_manifest);
        assert!(normalized.manifest.get("integrity_digest").is_none());
        let original = calculate_output(&normalized.manifest).unwrap();
        let mut changed_id = normalized.manifest;
        changed_id["snapshot_id"] = Value::String("000102030405060708090a0b0c0d0e10".to_owned());
        assert_ne!(
            calculate_output(&changed_id).unwrap().digest,
            original.digest
        );
    }

    // Test-ID: PR-TEST-0014
    // Verifies: PR-REQ-0198
    #[test]
    fn resource_ids_and_timestamps_have_one_spelling() {
        assert!(resource_id("0123456789abcdef0123456789abcdef".to_owned()).is_ok());
        assert!(resource_id("01234567-89ab-cdef-0123-456789abcdef".to_owned()).is_err());
        for token in ["1", "1.0", "1e0"] {
            assert_eq!(
                exact_integer(&RawValue::Number(token.to_owned())).unwrap(),
                1
            );
        }
        assert_eq!(
            exact_integer(&RawValue::Number(MAX_SAFE_INTEGER.to_string())).unwrap(),
            MAX_SAFE_INTEGER
        );
        for token in ["9007199254740992", "1e-9223372036854775808"] {
            assert!(exact_integer(&RawValue::Number(token.to_owned())).is_err());
        }
    }

    // Test-ID: PR-TEST-0015
    // Verifies: PR-REQ-0080, PR-REQ-0199
    #[test]
    fn binding_roles_are_relational_and_absence_is_explicit() {
        let vectors = vectors();
        let full = vectors
            .valid
            .iter()
            .find(|vector| vector.name == "complete_binding_state")
            .unwrap();
        let unresolved = vectors
            .valid
            .iter()
            .find(|vector| vector.name == "minimal_manifest")
            .unwrap();
        assert_eq!(full.relational, RelationalExpectation::Valid);
        assert_eq!(unresolved.relational, RelationalExpectation::NotEvaluated);
        let absent = full.normalized_manifest["managed_bindings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|binding| binding["input_id"] == "cache")
            .unwrap();
        assert_eq!(absent["state"], "absent");
    }

    // Test-ID: PR-TEST-0016
    // Verifies: PR-REQ-0080, PR-REQ-0196, PR-REQ-0200
    #[test]
    fn service_content_and_payload_digests_are_verified() {
        let vectors = vectors();
        let full = vectors
            .valid
            .iter()
            .find(|vector| vector.name == "complete_binding_state")
            .unwrap();
        assert!(
            normalize(
                &full.raw_manifest,
                full.producer_context.as_ref(),
                &full.blob_contents,
            )
            .is_ok()
        );
        assert_eq!(
            full.normalized_manifest["service_content"][0]["role"],
            "database"
        );
    }

    // Test-ID: PR-TEST-0017
    // Verifies: PR-REQ-0201, PR-REQ-0203
    #[test]
    fn strict_single_fault_vectors_have_stable_codes() {
        let vectors = vectors();
        for vector in &vectors.invalid {
            let error = normalize(
                &vector.raw_manifest,
                vector.producer_context.as_ref(),
                &vector.blob_contents,
            )
            .expect_err("negative vector must fail");
            assert_eq!(error.code, vector.expected_error, "{}", vector.name);
        }
    }

    // Test-ID: PR-TEST-0018
    // Verifies: PR-REQ-0080, PR-REQ-0195, PR-REQ-0196, PR-REQ-0202
    #[test]
    fn jcs_framing_and_sha_match_checked_in_vectors() {
        let vectors = vectors();
        for vector in &vectors.rfc8785 {
            assert_eq!(
                String::from_utf8(jcs_bytes(&vector.input).unwrap()).unwrap(),
                vector.expected,
                "{}",
                vector.name
            );
        }
        for vector in &vectors.valid {
            let normalized = normalize(
                &vector.raw_manifest,
                vector.producer_context.as_ref(),
                &vector.blob_contents,
            )
            .unwrap();
            assert_eq!(
                calculate_output(&normalized.manifest).unwrap(),
                vector.expected,
                "{}",
                vector.name
            );
        }
    }
}
