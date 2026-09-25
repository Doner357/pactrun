use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const VECTOR_PATH: &str = "tests/vectors/revision_core_format_v1/vectors.json";
const NODE_ORACLE_PATH: &str = "tests/oracles/revision_core_format_v1.mjs";
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
    raw_core: String,
    raw_content: String,
    normalized_core: Value,
    normalized_content: Value,
    #[serde(default)]
    source_input_ids: BTreeMap<String, Vec<String>>,
    relational: RelationalExpectation,
    expected: ExpectedOutput,
}

#[derive(Debug, Deserialize)]
struct InvalidVector {
    name: String,
    raw_core: String,
    raw_content: String,
    #[serde(default)]
    source_input_ids: BTreeMap<String, Vec<String>>,
    expected_error: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
struct ExpectedOutput {
    core_jcs_hex: String,
    content_jcs_hex: String,
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
struct NormalizedPair {
    core: Value,
    content: Value,
    relational: RelationalExpectation,
}

pub fn verify(workspace_root: &Path) -> Result<(), String> {
    let manifest = load_manifest(workspace_root)?;
    verify_manifest_metadata(&manifest)?;

    for vector in &manifest.valid {
        let normalized = normalize_pair(
            &vector.raw_core,
            &vector.raw_content,
            &vector.source_input_ids,
        )
        .map_err(|error| format_vector_error(&vector.name, error))?;

        if jcs_bytes(&normalized.core)? != jcs_bytes(&vector.normalized_core)? {
            return Err(format!(
                "valid vector {} normalized core does not match its checked-in semantic input\nexpected: {}\nactual: {}",
                vector.name,
                serde_json::to_string(&vector.normalized_core).unwrap_or_default(),
                serde_json::to_string(&normalized.core).unwrap_or_default()
            ));
        }
        if jcs_bytes(&normalized.content)? != jcs_bytes(&vector.normalized_content)? {
            return Err(format!(
                "valid vector {} normalized content does not match its checked-in semantic input",
                vector.name
            ));
        }
        if normalized.relational != vector.relational {
            return Err(format!(
                "valid vector {} relational result mismatch: expected {:?}, got {:?}",
                vector.name, vector.relational, normalized.relational
            ));
        }

        let actual = calculate_output(&normalized.core, &normalized.content)?;
        if actual != vector.expected {
            return Err(format!(
                "valid vector {} byte/digest mismatch\nexpected: {:?}\nactual: {:?}",
                vector.name, vector.expected, actual
            ));
        }
    }

    for vector in &manifest.invalid {
        match normalize_pair(
            &vector.raw_core,
            &vector.raw_content,
            &vector.source_input_ids,
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

    for vector in &manifest.rfc8785 {
        let actual = String::from_utf8(jcs_bytes(&vector.input)?)
            .map_err(|error| format!("RFC 8785 vector {} is not UTF-8: {error}", vector.name))?;
        if actual != vector.expected {
            return Err(format!(
                "RFC 8785 vector {} mismatch\nexpected: {}\nactual: {}",
                vector.name, vector.expected, actual
            ));
        }
    }

    verify_traceability(workspace_root)?;
    verify_node_oracle(workspace_root)?;
    eprintln!(
        "RevisionCoreFormatV1: {} valid, {} invalid, and {} RFC 8785 vectors passed Rust, traceability, and Node parity",
        manifest.valid.len(),
        manifest.invalid.len(),
        manifest.rfc8785.len()
    );
    Ok(())
}

pub fn calculate(workspace_root: &Path) -> Result<(), String> {
    let manifest = load_manifest(workspace_root)?;
    let mut outputs = BTreeMap::new();
    for vector in &manifest.valid {
        let normalized = normalize_pair(
            &vector.raw_core,
            &vector.raw_content,
            &vector.source_input_ids,
        )
        .map_err(|error| format_vector_error(&vector.name, error))?;
        outputs.insert(
            vector.name.clone(),
            calculate_output(&normalized.core, &normalized.content)?,
        );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&outputs).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn load_manifest(workspace_root: &Path) -> Result<VectorManifest, String> {
    let path = workspace_root.join(VECTOR_PATH);
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))
}

fn verify_manifest_metadata(manifest: &VectorManifest) -> Result<(), String> {
    if manifest.format != "revision_core_format_v1_golden_vectors" {
        return Err("unexpected RevisionCoreFormatV1 vector manifest kind".to_owned());
    }
    if manifest.status != "candidate" && manifest.status != "frozen" {
        return Err("vector manifest status must be candidate or frozen".to_owned());
    }
    let mut names = BTreeSet::new();
    for name in manifest
        .valid
        .iter()
        .map(|vector| &vector.name)
        .chain(manifest.invalid.iter().map(|vector| &vector.name))
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

fn normalize_pair(
    raw_core: &str,
    raw_content: &str,
    source_input_ids: &BTreeMap<String, Vec<String>>,
) -> Result<NormalizedPair, VerifyError> {
    let core = normalize_core(parse_raw_json(raw_core)?)?;
    let content = normalize_content(parse_raw_json(raw_content)?)?;
    validate_content_references(&core, &content)?;
    let relational = validate_relational_sources(&core, source_input_ids)?;
    Ok(NormalizedPair {
        core,
        content,
        relational,
    })
}

fn calculate_output(core: &Value, content: &Value) -> Result<ExpectedOutput, String> {
    let core_jcs = jcs_bytes(core)?;
    let content_jcs = jcs_bytes(content)?;
    let frame = frame(&core_jcs, &content_jcs);
    let digest = Sha256::digest(&frame);
    Ok(ExpectedOutput {
        core_jcs_hex: hex::encode(core_jcs),
        content_jcs_hex: hex::encode(content_jcs),
        frame_hex: hex::encode(frame),
        digest: format!("sha256:{}", hex::encode(digest)),
    })
}

fn jcs_bytes(value: &Value) -> Result<Vec<u8>, String> {
    serde_jcs::to_vec(value).map_err(|error| error.to_string())
}

fn frame(core_jcs: &[u8], content_jcs: &[u8]) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(b"pactrun.revision-content-digest\0");
    frame.extend_from_slice(&1_u32.to_be_bytes());
    frame.extend_from_slice(b"revision-core\0");
    frame.extend_from_slice(&(core_jcs.len() as u64).to_be_bytes());
    frame.extend_from_slice(core_jcs);
    frame.extend_from_slice(b"runtime-content-closure\0");
    frame.extend_from_slice(&(content_jcs.len() as u64).to_be_bytes());
    frame.extend_from_slice(content_jcs);
    frame
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

fn normalize_core(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "RevisionCoreV1")?;
    validate_fields(
        &object,
        &["format_version", "inputs", "actions", "migrations"],
        &["snapshot", "cleanup"],
    )?;
    let version = exact_integer(required(&object, "format_version")?)?;
    if version != 1 {
        return Err(VerifyError::new(
            "unsupported_format_version",
            "RevisionCoreV1 format_version must be 1",
        ));
    }

    let inputs = normalize_sorted_objects(required(&object, "inputs")?, normalize_input, "id")?;
    let input_ids = collect_ids(&inputs, "Input")?;
    let actions = normalize_sorted_objects(required(&object, "actions")?, normalize_action, "id")?;
    let migrations = normalize_migrations(required(&object, "migrations")?, &input_ids)?;

    let mut normalized = Map::new();
    normalized.insert("format_version".to_owned(), Value::Number(Number::from(1)));
    normalized.insert("inputs".to_owned(), Value::Array(inputs));
    normalized.insert("actions".to_owned(), Value::Array(actions));
    if let Some(snapshot) = object.get("snapshot") {
        normalized.insert("snapshot".to_owned(), normalize_snapshot(snapshot.clone())?);
    }
    normalized.insert("migrations".to_owned(), Value::Array(migrations));
    if let Some(cleanup) = object.get("cleanup") {
        normalized.insert(
            "cleanup".to_owned(),
            normalize_cleanup(cleanup.clone(), &input_ids)?,
        );
    }
    Ok(Value::Object(normalized))
}

fn normalize_input(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "InputDeclarationV1")?;
    validate_fields(&object, &["id", "required", "protection"], &[])?;
    let id = semantic_identifier(required_string(&object, "id")?)?;
    let required_value = required_bool(&object, "required")?;
    let protection = enum_string(
        required_string(&object, "protection")?,
        &["normal", "secret"],
    )?;
    object_value([
        ("id", Value::String(id)),
        ("required", Value::Bool(required_value)),
        ("protection", Value::String(protection)),
    ])
}

fn normalize_action(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "ActionV1")?;
    validate_fields(
        &object,
        &["id", "access", "parameters", "hook", "outputs"],
        &[],
    )?;
    let id = semantic_identifier(required_string(&object, "id")?)?;
    let access = enum_string(required_string(&object, "access")?, &["observe", "mutate"])?;
    let parameters =
        normalize_sorted_objects(required(&object, "parameters")?, normalize_parameter, "id")?;
    let outputs = normalize_sorted_objects(required(&object, "outputs")?, normalize_output, "id")?;
    object_value([
        ("id", Value::String(id)),
        ("access", Value::String(access)),
        ("parameters", Value::Array(parameters)),
        ("hook", normalize_hook(required(&object, "hook")?.clone())?),
        ("outputs", Value::Array(outputs)),
    ])
}

fn normalize_output(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "ManagedOutputV1")?;
    validate_fields(&object, &["id"], &[])?;
    object_value([(
        "id",
        Value::String(semantic_identifier(required_string(&object, "id")?)?),
    )])
}

fn normalize_parameter(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "ParameterV1")?;
    validate_fields(&object, &["id", "type", "sensitive"], &["default"])?;
    let id = semantic_identifier(required_string(&object, "id")?)?;
    let parameter_type = enum_string(
        required_string(&object, "type")?,
        &["integer", "float", "boolean", "string"],
    )?;
    let sensitive = required_bool(&object, "sensitive")?;
    let mut normalized = Map::new();
    normalized.insert("id".to_owned(), Value::String(id));
    normalized.insert("type".to_owned(), Value::String(parameter_type.clone()));
    normalized.insert("sensitive".to_owned(), Value::Bool(sensitive));
    if let Some(default) = object.get("default") {
        normalized.insert(
            "default".to_owned(),
            normalize_parameter_default(default, &parameter_type)?,
        );
    }
    Ok(Value::Object(normalized))
}

fn normalize_parameter_default(
    value: &RawValue,
    parameter_type: &str,
) -> Result<Value, VerifyError> {
    match (parameter_type, value) {
        ("integer", RawValue::Number(_)) => Ok(Value::Number(Number::from(exact_integer(value)?))),
        ("float", RawValue::Number(token)) => {
            let mut number = token
                .parse::<f64>()
                .map_err(|_| VerifyError::new("invalid_number", "float default is not binary64"))?;
            if !number.is_finite() {
                return Err(VerifyError::new(
                    "invalid_number",
                    "float default is not finite binary64",
                ));
            }
            if number == 0.0 {
                number = 0.0;
            }
            Ok(Value::Number(Number::from_f64(number).ok_or_else(
                || VerifyError::new("invalid_number", "float default is not finite"),
            )?))
        }
        ("boolean", RawValue::Bool(value)) => Ok(Value::Bool(*value)),
        ("string", RawValue::String(value)) => Ok(Value::String(value.clone())),
        _ => Err(VerifyError::new(
            "invalid_type",
            format!("default does not match parameter type {parameter_type}"),
        )),
    }
}

fn normalize_hook(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "HookV1")?;
    validate_fields(&object, &["protocol_version", "launch", "args", "io"], &[])?;
    let protocol_version = exact_integer(required(&object, "protocol_version")?)?;
    if protocol_version <= 0 {
        return Err(VerifyError::new(
            "invalid_number",
            "Hook protocol_version must be positive",
        ));
    }
    let args = string_array(required(&object, "args")?)?;
    let io = normalize_io(required(&object, "io")?.clone())?;
    object_value([
        (
            "protocol_version",
            Value::Number(Number::from(protocol_version)),
        ),
        (
            "launch",
            normalize_launch(required(&object, "launch")?.clone())?,
        ),
        ("args", Value::Array(args)),
        ("io", io),
    ])
}

fn normalize_launch(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "HookLaunchV1")?;
    let kind = required_string(&object, "kind")?;
    match kind.as_str() {
        "direct" => {
            validate_fields(&object, &["kind", "executable"], &[])?;
            object_value([
                ("kind", Value::String("direct".to_owned())),
                (
                    "executable",
                    Value::String(semantic_identifier(required_string(
                        &object,
                        "executable",
                    )?)?),
                ),
            ])
        }
        "interpreter" => {
            validate_fields(
                &object,
                &["kind", "command", "interpreter_args", "script"],
                &[],
            )?;
            let command = host_executable_name(required_string(&object, "command")?)?;
            object_value([
                ("kind", Value::String("interpreter".to_owned())),
                ("command", Value::String(command)),
                (
                    "interpreter_args",
                    Value::Array(string_array(required(&object, "interpreter_args")?)?),
                ),
                (
                    "script",
                    Value::String(semantic_identifier(required_string(&object, "script")?)?),
                ),
            ])
        }
        _ => Err(VerifyError::new(
            "invalid_hook_launch",
            "Hook launch kind must be direct or interpreter",
        )),
    }
}

fn normalize_io(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "IOContractV1")?;
    validate_fields(&object, &["terminal"], &[])?;
    object_value([(
        "terminal",
        Value::String(enum_string(
            required_string(&object, "terminal")?,
            &["none", "output", "interactive"],
        )?),
    )])
}

fn normalize_snapshot(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "SnapshotCapabilityV1")?;
    validate_fields(&object, &[], &["capture", "restore"])?;
    if object.is_empty() {
        return Err(VerifyError::new(
            "invalid_snapshot",
            "SnapshotCapabilityV1 must contain capture or restore",
        ));
    }
    let mut normalized = Map::new();
    if let Some(capture) = object.get("capture") {
        normalized.insert("capture".to_owned(), normalize_capture(capture.clone())?);
    }
    if let Some(restore) = object.get("restore") {
        normalized.insert("restore".to_owned(), normalize_restore(restore.clone())?);
    }
    Ok(Value::Object(normalized))
}

fn normalize_capture(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "CaptureV1")?;
    validate_fields(&object, &["parameters", "access", "hook"], &[])?;
    let parameters =
        normalize_sorted_objects(required(&object, "parameters")?, normalize_parameter, "id")?;
    object_value([
        ("parameters", Value::Array(parameters)),
        (
            "access",
            Value::String(enum_string(
                required_string(&object, "access")?,
                &["observe", "mutate"],
            )?),
        ),
        ("hook", normalize_hook(required(&object, "hook")?.clone())?),
    ])
}

fn normalize_restore(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "RestoreV1")?;
    validate_fields(&object, &["parameters", "hook"], &[])?;
    let parameters =
        normalize_sorted_objects(required(&object, "parameters")?, normalize_parameter, "id")?;
    object_value([
        ("parameters", Value::Array(parameters)),
        ("hook", normalize_hook(required(&object, "hook")?.clone())?),
    ])
}

fn normalize_migrations(
    value: &RawValue,
    target_inputs: &BTreeSet<String>,
) -> Result<Vec<Value>, VerifyError> {
    let mut migrations = Vec::new();
    for value in expect_array_ref(value, "migrations")? {
        migrations.push(normalize_migration(value.clone(), target_inputs)?);
    }
    sort_unique_by_field(&mut migrations, "source_revision_digest", "Migration")?;
    Ok(migrations)
}

fn normalize_migration(
    value: RawValue,
    target_inputs: &BTreeSet<String>,
) -> Result<Value, VerifyError> {
    let object = expect_object(value, "MigrationV1")?;
    validate_fields(
        &object,
        &[
            "source_revision_digest",
            "transitions",
            "requires_source",
            "requires_target",
            "produces_target",
        ],
        &["hook"],
    )?;
    let source_digest = sha256_digest(required_string(&object, "source_revision_digest")?)?;
    let mut source_roles = BTreeMap::new();
    let mut requires_source = normalize_binding_refs(required(&object, "requires_source")?)?;
    for reference in &requires_source {
        record_source_role(
            reference.as_object().expect("normalized binding ref"),
            &mut source_roles,
        )?;
    }
    sort_binding_refs(&mut requires_source);

    let requires_target =
        normalize_target_ids(required(&object, "requires_target")?, target_inputs)?;
    let produces_target =
        normalize_target_ids(required(&object, "produces_target")?, target_inputs)?;
    let required_targets = value_strings(&requires_target);
    let produced_targets = value_strings(&produces_target);
    if !required_targets.is_disjoint(&produced_targets) {
        return Err(VerifyError::new(
            "invalid_transition",
            "requires_target and produces_target overlap",
        ));
    }

    let mut transitions = Vec::new();
    let mut transitioned_sources = BTreeSet::new();
    let mut target_writers = produced_targets.clone();
    for transition in expect_array_ref(required(&object, "transitions")?, "transitions")? {
        let normalized = normalize_transition(transition.clone(), target_inputs)?;
        let source = normalized
            .get("source")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                VerifyError::new("invalid_transition", "transition source is missing")
            })?;
        let source_key = format!(
            "{}:{}",
            source
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            source
                .get("input_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
        );
        if !transitioned_sources.insert(source_key) {
            return Err(VerifyError::new(
                "invalid_transition",
                "source binding has more than one disposition",
            ));
        }
        record_source_role(source, &mut source_roles)?;
        if let Some(target) = normalized.get("target_input_id").and_then(Value::as_str)
            && !target_writers.insert(target.to_owned())
        {
            return Err(VerifyError::new(
                "invalid_transition",
                "target Input has multiple writers",
            ));
        }
        transitions.push(normalized);
    }
    transitions.sort_by_key(transition_sort_key);

    let mut normalized = Map::new();
    normalized.insert(
        "source_revision_digest".to_owned(),
        Value::String(source_digest),
    );
    normalized.insert("transitions".to_owned(), Value::Array(transitions));
    normalized.insert("requires_source".to_owned(), Value::Array(requires_source));
    normalized.insert("requires_target".to_owned(), Value::Array(requires_target));
    normalized.insert("produces_target".to_owned(), Value::Array(produces_target));
    if let Some(hook) = object.get("hook") {
        normalized.insert("hook".to_owned(), normalize_hook(hook.clone())?);
    }
    Ok(Value::Object(normalized))
}

fn normalize_transition(
    value: RawValue,
    target_inputs: &BTreeSet<String>,
) -> Result<Value, VerifyError> {
    let object = expect_object(value, "MigrationTransitionV1")?;
    let kind = required_string(&object, "kind")?;
    match kind.as_str() {
        "carry" | "declassify" => {
            validate_fields(&object, &["kind", "source", "target_input_id"], &[])?;
            let target = semantic_identifier(required_string(&object, "target_input_id")?)?;
            if !target_inputs.contains(&target) {
                return Err(VerifyError::new(
                    "invalid_reference",
                    format!("target Input {target} is not declared"),
                ));
            }
            object_value([
                ("kind", Value::String(kind)),
                (
                    "source",
                    normalize_binding_ref(required(&object, "source")?.clone())?,
                ),
                ("target_input_id", Value::String(target)),
            ])
        }
        "keep" | "discard" => {
            validate_fields(&object, &["kind", "source"], &[])?;
            object_value([
                ("kind", Value::String(kind)),
                (
                    "source",
                    normalize_binding_ref(required(&object, "source")?.clone())?,
                ),
            ])
        }
        _ => Err(VerifyError::new(
            "invalid_transition",
            "unknown Migration transition kind",
        )),
    }
}

fn normalize_binding_refs(value: &RawValue) -> Result<Vec<Value>, VerifyError> {
    let mut references = Vec::new();
    let mut keys = BTreeSet::new();
    for value in expect_array_ref(value, "InputBindingRefV1[]")? {
        let reference = normalize_binding_ref(value.clone())?;
        let key = binding_ref_key(&reference);
        if !keys.insert(key) {
            return Err(VerifyError::new(
                "duplicate_semantic_key",
                "duplicate source binding reference",
            ));
        }
        references.push(reference);
    }
    Ok(references)
}

fn normalize_binding_ref(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "InputBindingRefV1")?;
    validate_fields(&object, &["role", "input_id"], &[])?;
    object_value([
        (
            "role",
            Value::String(enum_string(
                required_string(&object, "role")?,
                &["active", "retained"],
            )?),
        ),
        (
            "input_id",
            Value::String(semantic_identifier(required_string(&object, "input_id")?)?),
        ),
    ])
}

fn normalize_target_ids(
    value: &RawValue,
    target_inputs: &BTreeSet<String>,
) -> Result<Vec<Value>, VerifyError> {
    let mut ids = Vec::new();
    let mut seen = BTreeSet::new();
    for value in expect_array_ref(value, "InputIdentity[]")? {
        let id = match value {
            RawValue::String(value) => semantic_identifier(value.clone())?,
            _ => {
                return Err(VerifyError::new(
                    "invalid_type",
                    "target Input identity must be string",
                ));
            }
        };
        if !target_inputs.contains(&id) {
            return Err(VerifyError::new(
                "invalid_reference",
                format!("target Input {id} is not declared"),
            ));
        }
        if !seen.insert(id.clone()) {
            return Err(VerifyError::new(
                "duplicate_semantic_key",
                format!("duplicate target Input {id}"),
            ));
        }
        ids.push(Value::String(id));
    }
    ids.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
    Ok(ids)
}

fn normalize_cleanup(
    value: RawValue,
    current_inputs: &BTreeSet<String>,
) -> Result<Value, VerifyError> {
    let object = expect_object(value, "CleanupV1")?;
    validate_fields(&object, &["requires", "hook"], &[])?;
    let mut requires = normalize_binding_refs(required(&object, "requires")?)?;
    let mut roles = BTreeMap::new();
    for reference in &requires {
        let reference = reference.as_object().expect("normalized binding ref");
        record_source_role(reference, &mut roles)?;
        let role = reference
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let id = reference
            .get("input_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if (role == "active") != current_inputs.contains(id) {
            return Err(VerifyError::new(
                "invalid_source_binding_role",
                format!("Cleanup Input {id} has role {role} relative to the current Revision"),
            ));
        }
    }
    sort_binding_refs(&mut requires);
    object_value([
        ("requires", Value::Array(requires)),
        ("hook", normalize_hook(required(&object, "hook")?.clone())?),
    ])
}

fn normalize_content(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "RuntimeContentClosureIdentityV1")?;
    validate_fields(&object, &["files"], &[])?;
    let mut files =
        normalize_sorted_objects(required(&object, "files")?, normalize_runtime_file, "id")?;
    let mut paths = BTreeSet::new();
    for file in &files {
        let path = file.get("path").and_then(Value::as_str).unwrap_or_default();
        if !paths.insert(path.to_owned()) {
            return Err(VerifyError::new(
                "duplicate_semantic_key",
                format!("duplicate runtime path {path}"),
            ));
        }
    }
    object_value([("files", Value::Array(std::mem::take(&mut files)))])
}

fn normalize_runtime_file(value: RawValue) -> Result<Value, VerifyError> {
    let object = expect_object(value, "RuntimeFileV1")?;
    validate_fields(
        &object,
        &["id", "path", "kind", "blob_digest", "executable"],
        &[],
    )?;
    let kind = enum_string(required_string(&object, "kind")?, &["regular_file"])?;
    object_value([
        (
            "id",
            Value::String(semantic_identifier(required_string(&object, "id")?)?),
        ),
        (
            "path",
            Value::String(runtime_path(required_string(&object, "path")?)?),
        ),
        ("kind", Value::String(kind)),
        (
            "blob_digest",
            Value::String(sha256_digest(required_string(&object, "blob_digest")?)?),
        ),
        (
            "executable",
            Value::Bool(required_bool(&object, "executable")?),
        ),
    ])
}

fn validate_content_references(core: &Value, content: &Value) -> Result<(), VerifyError> {
    let files = content
        .get("files")
        .and_then(Value::as_array)
        .expect("normalized content files");
    let file_map: BTreeMap<_, _> = files
        .iter()
        .map(|file| {
            (
                file.get("id").and_then(Value::as_str).unwrap_or_default(),
                file,
            )
        })
        .collect();
    walk_hooks(core, &mut |hook| {
        let launch = hook
            .get("launch")
            .and_then(Value::as_object)
            .expect("normalized launch");
        match launch.get("kind").and_then(Value::as_str) {
            Some("direct") => {
                let id = launch
                    .get("executable")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let file = file_map.get(id).ok_or_else(|| {
                    VerifyError::new(
                        "invalid_reference",
                        format!("unknown executable ContentId {id}"),
                    )
                })?;
                if file.get("executable").and_then(Value::as_bool) != Some(true) {
                    return Err(VerifyError::new(
                        "invalid_hook_launch",
                        format!("direct ContentId {id} is not executable"),
                    ));
                }
            }
            Some("interpreter") => {
                let id = launch
                    .get("script")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !file_map.contains_key(id) {
                    return Err(VerifyError::new(
                        "invalid_reference",
                        format!("unknown script ContentId {id}"),
                    ));
                }
            }
            _ => unreachable!("normalized launch kind"),
        }
        Ok(())
    })
}

fn walk_hooks(
    core: &Value,
    visitor: &mut impl FnMut(&Value) -> Result<(), VerifyError>,
) -> Result<(), VerifyError> {
    for action in core
        .get("actions")
        .and_then(Value::as_array)
        .expect("normalized actions")
    {
        visitor(action.get("hook").expect("normalized action hook"))?;
    }
    if let Some(snapshot) = core.get("snapshot") {
        if let Some(capture) = snapshot.get("capture") {
            visitor(capture.get("hook").expect("normalized capture hook"))?;
        }
        if let Some(restore) = snapshot.get("restore") {
            visitor(restore.get("hook").expect("normalized restore hook"))?;
        }
    }
    for migration in core
        .get("migrations")
        .and_then(Value::as_array)
        .expect("normalized migrations")
    {
        if let Some(hook) = migration.get("hook") {
            visitor(hook)?;
        }
    }
    if let Some(cleanup) = core.get("cleanup") {
        visitor(cleanup.get("hook").expect("normalized cleanup hook"))?;
    }
    Ok(())
}

fn validate_relational_sources(
    core: &Value,
    source_input_ids: &BTreeMap<String, Vec<String>>,
) -> Result<RelationalExpectation, VerifyError> {
    let migrations = core
        .get("migrations")
        .and_then(Value::as_array)
        .expect("normalized migrations");
    if migrations.is_empty() {
        return Ok(RelationalExpectation::Valid);
    }
    let mut evaluated_all = true;
    for migration in migrations {
        let digest = migration
            .get("source_revision_digest")
            .and_then(Value::as_str)
            .expect("normalized source digest");
        let Some(source_ids) = source_input_ids.get(digest) else {
            evaluated_all = false;
            continue;
        };
        let source_ids: BTreeSet<_> = source_ids.iter().map(String::as_str).collect();
        let mut references = Vec::new();
        references.extend(
            migration
                .get("requires_source")
                .and_then(Value::as_array)
                .expect("normalized source requirements")
                .iter(),
        );
        for transition in migration
            .get("transitions")
            .and_then(Value::as_array)
            .expect("normalized transitions")
        {
            references.push(
                transition
                    .get("source")
                    .expect("normalized transition source"),
            );
        }
        for reference in references {
            let role = reference
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let id = reference
                .get("input_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let declared = source_ids.contains(id);
            if (role == "active") != declared {
                return Err(VerifyError::new(
                    "invalid_source_binding_role",
                    format!("source Input {id} is declared={declared} but role is {role}"),
                ));
            }
        }
    }
    if evaluated_all {
        Ok(RelationalExpectation::Valid)
    } else {
        Ok(RelationalExpectation::NotEvaluated)
    }
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
    let decimal_shift = exponent - i64::try_from(fraction.len()).unwrap_or(i64::MAX);
    if decimal_shift < 0 {
        let remove = usize::try_from(-decimal_shift).unwrap_or(usize::MAX);
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

fn host_executable_name(value: String) -> Result<String, VerifyError> {
    let valid_length = !value.is_empty() && value.len() <= 128;
    let valid_ascii = value.is_ascii();
    let valid_edges = value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric);
    let valid_characters = value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if !valid_length
        || !valid_ascii
        || !valid_edges
        || !valid_characters
        || value == "."
        || value == ".."
        || is_windows_reserved(&value)
    {
        return Err(VerifyError::new(
            "invalid_host_executable_name",
            format!("invalid host executable name {value:?}"),
        ));
    }
    Ok(value)
}

fn runtime_path(value: String) -> Result<String, VerifyError> {
    if !crate::lexical_v1::is_runtime_path(&value) {
        return Err(VerifyError::new(
            "invalid_runtime_path",
            format!("invalid runtime path {value:?}"),
        ));
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

fn required_bool(object: &BTreeMap<String, RawValue>, field: &str) -> Result<bool, VerifyError> {
    match required(object, field)? {
        RawValue::Bool(value) => Ok(*value),
        _ => Err(VerifyError::new(
            "invalid_type",
            format!("{field} must be boolean"),
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

fn string_array(value: &RawValue) -> Result<Vec<Value>, VerifyError> {
    expect_array_ref(value, "string[]")?
        .iter()
        .map(|value| match value {
            RawValue::String(value) => Ok(Value::String(value.clone())),
            _ => Err(VerifyError::new(
                "invalid_type",
                "array item must be string",
            )),
        })
        .collect()
}

fn normalize_sorted_objects(
    value: &RawValue,
    normalizer: fn(RawValue) -> Result<Value, VerifyError>,
    key: &str,
) -> Result<Vec<Value>, VerifyError> {
    let mut values = expect_array_ref(value, "semantic set")?
        .iter()
        .cloned()
        .map(normalizer)
        .collect::<Result<Vec<_>, _>>()?;
    sort_unique_by_field(&mut values, key, "semantic object")?;
    Ok(values)
}

fn sort_unique_by_field(values: &mut [Value], field: &str, name: &str) -> Result<(), VerifyError> {
    values.sort_by(|left, right| {
        left.get(field)
            .and_then(Value::as_str)
            .cmp(&right.get(field).and_then(Value::as_str))
    });
    for pair in values.windows(2) {
        if pair[0].get(field) == pair[1].get(field) {
            return Err(VerifyError::new(
                "duplicate_semantic_key",
                format!("duplicate {name} key"),
            ));
        }
    }
    Ok(())
}

fn collect_ids(values: &[Value], name: &str) -> Result<BTreeSet<String>, VerifyError> {
    let ids: BTreeSet<_> = values
        .iter()
        .filter_map(|value| value.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect();
    if ids.len() != values.len() {
        return Err(VerifyError::new(
            "duplicate_semantic_key",
            format!("duplicate {name} identity"),
        ));
    }
    Ok(ids)
}

fn object_value<const N: usize>(fields: [(&str, Value); N]) -> Result<Value, VerifyError> {
    Ok(Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    ))
}

fn binding_ref_key(reference: &Value) -> (String, String) {
    (
        reference
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        reference
            .get("input_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    )
}

fn sort_binding_refs(references: &mut [Value]) {
    references.sort_by_key(binding_ref_key);
}

fn record_source_role(
    reference: &Map<String, Value>,
    roles: &mut BTreeMap<String, String>,
) -> Result<(), VerifyError> {
    let role = reference
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let id = reference
        .get("input_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if let Some(previous) = roles.insert(id.to_owned(), role.to_owned())
        && previous != role
    {
        return Err(VerifyError::new(
            "invalid_source_binding_role",
            format!("source Input {id} has conflicting roles"),
        ));
    }
    Ok(())
}

fn transition_sort_key(value: &Value) -> (String, String, String, String) {
    let source = value.get("source").and_then(Value::as_object);
    (
        source
            .and_then(|source| source.get("role"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        source
            .and_then(|source| source.get("input_id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        value
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        value
            .get("target_input_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    )
}

fn value_strings(values: &[Value]) -> BTreeSet<String> {
    values
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
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
            "RevisionCoreFormatV1 oracle requires Node 24, found {}",
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

pub(crate) fn verify_traceability(workspace_root: &Path) -> Result<(), String> {
    let markdown = normative_markdown_files(&workspace_root.join("docs"))?;
    let mut requirements = BTreeMap::<String, BTreeSet<String>>::new();
    for path in markdown {
        let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let mut current_requirement = None;
        let mut verification_requirement = None;
        for line in text.lines() {
            if let Some(id) = token_after(line, "### PR-REQ-") {
                let id = format!("PR-REQ-{id}");
                if requirements.insert(id.clone(), BTreeSet::new()).is_some() {
                    return Err(format!("duplicate requirement ID {id}"));
                }
                current_requirement = Some(id);
            }
            if line.starts_with("**Verification:") {
                verification_requirement = if line.contains("Pending automated coverage") {
                    None
                } else {
                    Some(current_requirement.clone().ok_or_else(|| {
                        format!(
                            "verification metadata has no owning requirement in {}",
                            path.display()
                        )
                    })?)
                };
            }
            if let Some(requirement) = &verification_requirement {
                requirements
                    .get_mut(requirement)
                    .expect("current requirement was inserted")
                    .extend(extract_ids(line, "PR-TEST-"));
                if line.ends_with("**") {
                    verification_requirement = None;
                }
            }
        }
    }

    let mut sources = Vec::new();
    collect_files(&workspace_root.join("src"), "rs", &mut sources)?;
    collect_files(&workspace_root.join("tests"), "rs", &mut sources)?;
    collect_files(&workspace_root.join("xtask/src"), "rs", &mut sources)?;
    collect_files(&workspace_root.join("tests/oracles"), "mjs", &mut sources)?;
    let mut tests = BTreeMap::<String, BTreeSet<String>>::new();
    for path in sources {
        let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let lines: Vec<_> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            if let Some(id) = extract_ids(line, "PR-TEST-").into_iter().next()
                && line.contains("Test-ID:")
            {
                if tests.contains_key(&id) {
                    return Err(format!("duplicate test ID {id}"));
                }
                let verifies_line = lines.get(index + 1).copied().unwrap_or_default();
                let verifies = extract_ids(verifies_line, "PR-REQ-");
                if verifies.is_empty() {
                    return Err(format!("test {id} has no Verifies metadata"));
                }
                tests.insert(id, verifies);
            }
        }
    }

    for (requirement, referenced_tests) in &requirements {
        for test in referenced_tests {
            let verified_requirements = tests
                .get(test)
                .ok_or_else(|| format!("requirement references nonexistent test {test}"))?;
            if !verified_requirements.contains(requirement) {
                return Err(format!(
                    "requirement {requirement} references {test}, but the test does not reference the requirement"
                ));
            }
        }
    }
    for (test, verified_requirements) in &tests {
        for requirement in verified_requirements {
            let referenced_tests = requirements.get(requirement).ok_or_else(|| {
                format!("test {test} references nonexistent requirement {requirement}")
            })?;
            if !referenced_tests.contains(test) {
                return Err(format!(
                    "test {test} references {requirement}, but the requirement does not reference the test"
                ));
            }
        }
    }
    Ok(())
}

// Spec is the sole product-rule tree. Old locations are forwarding entries only.
fn is_normative_markdown(relative: &Path) -> bool {
    relative.extension().and_then(|value| value.to_str()) == Some("md")
        && relative.starts_with("spec")
}

fn has_numbered_requirement_definition(text: &str) -> bool {
    text.lines().any(|line| {
        line.starts_with('#')
            && line
                .trim_start_matches('#')
                .trim_start()
                .strip_prefix("PR-REQ-")
                .and_then(|suffix| suffix.as_bytes().first())
                .is_some_and(u8::is_ascii_digit)
    })
}

pub(crate) fn normative_markdown_files(docs_root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_files(docs_root, "md", &mut files)?;
    let mut normative = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(docs_root)
            .map_err(|error| error.to_string())?;
        if is_normative_markdown(relative) {
            normative.push(path);
        } else {
            let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
            if has_numbered_requirement_definition(&text) {
                return Err(format!(
                    "product requirement definition outside Spec: {}",
                    relative.display()
                ));
            }
        }
    }
    normative.sort();
    Ok(normative)
}

fn collect_files(root: &Path, extension: &str, output: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            collect_files(&path, extension, output)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            output.push(path);
        }
    }
    Ok(())
}

fn token_after(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    (!digits.is_empty()).then_some(digits)
}

fn extract_ids(line: &str, prefix: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let mut remaining = line;
    while let Some(position) = remaining.find(prefix) {
        let rest = &remaining[position + prefix.len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            ids.insert(format!("{prefix}{digits}"));
        }
        remaining = &rest[digits.len()..];
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask is a direct workspace child")
            .to_path_buf()
    }

    #[test]
    fn normative_document_scope_excludes_non_authoritative_copies() {
        for included in [
            "spec/behavior/resources.md",
            "spec/foundations/system-model.md",
            "spec/execution/recovery-and-reconciliation.md",
            "spec/contracts/hook-protocol-v1.md",
        ] {
            assert!(is_normative_markdown(Path::new(included)), "{included}");
        }
        for excluded in [
            "agents/spec-copy.md",
            "archive/old.md",
            "proposals/new.md",
            "guides/intro.md",
            "development/implementation-roadmap.md",
            "pactrun-developers/architecture/system-model.md",
            "pactrun-developers/product-behavior/actions-plans-and-runs.md",
            "pactrun-developers/package-contracts/hook-protocol-v1.md",
            "agent-docs/spec/copy.md",
            "specification/copy.md",
            "spec/data.json",
        ] {
            assert!(!is_normative_markdown(Path::new(excluded)), "{excluded}");
        }
    }

    #[test]
    fn numbered_definitions_are_not_confused_with_references_or_templates() {
        assert!(has_numbered_requirement_definition(
            "### PR-REQ-0001 - Rule"
        ));
        assert!(has_numbered_requirement_definition("## PR-REQ-0001 - Rule"));
        assert!(!has_numbered_requirement_definition("See PR-REQ-0001."));
        assert!(!has_numbered_requirement_definition(
            "### PR-REQ-NNNN - Template"
        ));
    }

    fn manifest() -> VectorManifest {
        load_manifest(&workspace_root()).expect("load vectors")
    }

    // Test-ID: PR-TEST-0001
    // Verifies: PR-REQ-0017, PR-REQ-0182, PR-REQ-0184, PR-REQ-0189, PR-REQ-0192
    #[test]
    fn minimal_direct_vector_is_stable() {
        let vector = &manifest().valid[0];
        let normalized = normalize_pair(
            &vector.raw_core,
            &vector.raw_content,
            &vector.source_input_ids,
        )
        .expect("normalize minimal vector");
        assert_eq!(
            calculate_output(&normalized.core, &normalized.content).unwrap(),
            vector.expected
        );
    }

    // Test-ID: PR-TEST-0002
    // Verifies: PR-REQ-0183, PR-REQ-0189
    #[test]
    fn hook_protocol_version_is_independent() {
        let manifest = manifest();
        let one = manifest
            .valid
            .iter()
            .find(|value| value.name == "minimal_direct")
            .unwrap();
        let two = manifest
            .valid
            .iter()
            .find(|value| value.name == "protocol_version_two")
            .unwrap();
        assert_ne!(one.expected.digest, two.expected.digest);
        assert_eq!(two.normalized_core["format_version"], 1);
        assert_eq!(
            two.normalized_core["actions"][0]["hook"]["protocol_version"],
            2
        );
    }

    // Test-ID: PR-TEST-0003
    // Verifies: PR-REQ-0184, PR-REQ-0191
    #[test]
    fn full_snapshot_schema_is_closed() {
        let manifest = manifest();
        let full = manifest
            .valid
            .iter()
            .find(|value| value.name == "full_interpreter")
            .unwrap();
        assert!(
            full.normalized_core["snapshot"]["capture"]
                .get("access")
                .is_some()
        );
        assert!(
            full.normalized_core["snapshot"]["restore"]
                .get("access")
                .is_none()
        );
    }

    // Test-ID: PR-TEST-0004
    // Verifies: PR-REQ-0186, PR-REQ-0190
    #[test]
    fn source_roles_have_explicit_relational_state() {
        let manifest = manifest();
        let evaluated = manifest
            .valid
            .iter()
            .find(|value| value.name == "full_interpreter")
            .unwrap();
        let unresolved = manifest
            .valid
            .iter()
            .find(|value| value.name == "source_not_evaluated")
            .unwrap();
        assert_eq!(evaluated.relational, RelationalExpectation::Valid);
        assert_eq!(unresolved.relational, RelationalExpectation::NotEvaluated);
    }

    // Test-ID: PR-TEST-0005
    // Verifies: PR-REQ-0187
    #[test]
    fn unicode_vectors_preserve_scalars_and_reject_lone_surrogates() {
        let manifest = manifest();
        let nfc = manifest
            .valid
            .iter()
            .find(|value| value.name == "unicode_nfc")
            .unwrap();
        let nfd = manifest
            .valid
            .iter()
            .find(|value| value.name == "unicode_nfd")
            .unwrap();
        assert_ne!(nfc.expected.digest, nfd.expected.digest);
        for name in ["lone_high_surrogate", "lone_low_surrogate"] {
            assert_eq!(
                manifest
                    .invalid
                    .iter()
                    .find(|value| value.name == name)
                    .unwrap()
                    .expected_error,
                "invalid_unicode_scalar"
            );
        }
    }

    // Test-ID: PR-TEST-0006
    // Verifies: PR-REQ-0188
    #[test]
    fn integer_number_matrix_is_exact() {
        assert_eq!(exact_integer(&RawValue::Number("1".to_owned())).unwrap(), 1);
        assert_eq!(
            exact_integer(&RawValue::Number("1.0".to_owned())).unwrap(),
            1
        );
        assert_eq!(
            exact_integer(&RawValue::Number("1e0".to_owned())).unwrap(),
            1
        );
        assert_eq!(
            exact_integer(&RawValue::Number(MAX_SAFE_INTEGER.to_string())).unwrap(),
            MAX_SAFE_INTEGER
        );
        for token in ["9007199254740992", "9007199254740993"] {
            assert_eq!(
                exact_integer(&RawValue::Number(token.to_owned()))
                    .unwrap_err()
                    .code,
                "invalid_number"
            );
        }
    }

    // Test-ID: PR-TEST-0007
    // Verifies: PR-REQ-0188
    #[test]
    fn float_number_matrix_normalizes_negative_zero() {
        for token in ["-0", "-0.0"] {
            let value =
                normalize_parameter_default(&RawValue::Number(token.to_owned()), "float").unwrap();
            assert_eq!(jcs_bytes(&value).unwrap(), b"0");
            assert!(value.as_f64().unwrap().is_sign_positive());
        }
        assert!(
            normalize_parameter_default(&RawValue::Number("1e400".to_owned()), "float").is_err()
        );
    }

    // Test-ID: PR-TEST-0008
    // Verifies: PR-REQ-0185, PR-REQ-0189, PR-REQ-0192
    #[test]
    fn portable_names_paths_and_content_are_validated() {
        assert!(host_executable_name("pwsh.exe".to_owned()).is_ok());
        assert!(host_executable_name("python3.13".to_owned()).is_ok());
        for value in [".", "..", ".hidden", "pwsh.", "CON.exe", "dir/pwsh.exe"] {
            assert!(host_executable_name(value.to_owned()).is_err(), "{value}");
        }
        assert!(runtime_path("bin/hook.exe".to_owned()).is_ok());
        assert!(runtime_path("bin/CON.exe".to_owned()).is_err());
    }

    // Test-ID: PR-TEST-0009
    // Verifies: PR-REQ-0184, PR-REQ-0186, PR-REQ-0189
    #[test]
    fn semantic_sets_sort_but_hook_arguments_do_not() {
        let manifest = manifest();
        let full = manifest
            .valid
            .iter()
            .find(|value| value.name == "full_interpreter")
            .unwrap();
        assert_eq!(full.normalized_core["inputs"][0]["id"], "config");
        assert_eq!(
            full.normalized_core["actions"][0]["hook"]["args"][0],
            "--second"
        );
    }

    // Test-ID: PR-TEST-0010
    // Verifies: PR-REQ-0193
    #[test]
    fn strict_raw_errors_are_stable_for_single_fault_vectors() {
        let manifest = manifest();
        for vector in &manifest.invalid {
            let error = normalize_pair(
                &vector.raw_core,
                &vector.raw_content,
                &vector.source_input_ids,
            )
            .expect_err("negative vector must fail");
            assert_eq!(error.code, vector.expected_error, "{}", vector.name);
        }
    }

    // Test-ID: PR-TEST-0011
    // Verifies: PR-REQ-0182, PR-REQ-0183, PR-REQ-0192, PR-REQ-0193
    #[test]
    fn jcs_framing_and_sha_match_checked_in_vectors() {
        for vector in &manifest().rfc8785 {
            assert_eq!(
                String::from_utf8(jcs_bytes(&vector.input).unwrap()).unwrap(),
                vector.expected,
                "{}",
                vector.name
            );
        }
        for vector in &manifest().valid {
            let normalized = normalize_pair(
                &vector.raw_core,
                &vector.raw_content,
                &vector.source_input_ids,
            )
            .unwrap();
            assert_eq!(
                calculate_output(&normalized.core, &normalized.content).unwrap(),
                vector.expected,
                "{}",
                vector.name
            );
        }
    }
}
