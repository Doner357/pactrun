use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::Path,
    process::Command,
};

const VECTOR_PATH: &str = "tests/vectors/hook_protocol_v1/vectors.json";
const NODE_ORACLE_PATH: &str = "tests/oracles/hook_protocol_v1.mjs";
const PREAMBLE: &[u8] = b"pactrun.hook-protocol\0\0\0\0\x01";
const MAX_PAYLOAD: u32 = 16 * 1024 * 1024;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Deserialize)]
struct VectorManifest {
    format: String,
    status: String,
    preamble_hex: String,
    session_specs: Vec<SessionFixture>,
    valid: Vec<ValidVector>,
    invalid: Vec<InvalidVector>,
}

#[derive(Debug, Deserialize)]
struct SessionFixture {
    name: String,
    raw_json: String,
}

#[derive(Debug, Deserialize)]
struct ValidVector {
    name: String,
    session: String,
    messages: Vec<WireMessage>,
    expected_state: Value,
}

#[derive(Debug, Deserialize)]
struct InvalidVector {
    name: String,
    #[serde(default)]
    session: Option<String>,
    #[serde(default)]
    messages: Vec<WireMessage>,
    #[serde(default)]
    transport: Option<TransportFault>,
    expected_error: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct WireMessage {
    direction: Direction,
    raw_json: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Direction {
    PactrunToHook,
    HookToPactrun,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TransportFault {
    Preamble { hex: String },
    DeclaredLength { length: u32 },
    PayloadHex { hex: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ParityReport {
    preamble_hex: String,
    vectors: Vec<ParityVector>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ParityVector {
    name: String,
    frames_hex: Vec<String>,
    final_state: Value,
}

#[derive(Clone, Debug, PartialEq)]
enum Phase {
    AwaitSession,
    AwaitReady,
    Active,
    AwaitCompletionAccepted,
    Terminal,
}

#[derive(Clone, Debug)]
struct OperationContext {
    kind: String,
    action_outputs: BTreeSet<String>,
    migration_outputs: BTreeSet<String>,
}

#[derive(Clone, Debug)]
struct SessionContext {
    session_id: String,
    operation: OperationContext,
}

#[derive(Clone, Debug)]
struct PendingRequest {
    id: u64,
    kind: String,
}

#[derive(Debug)]
struct ProtocolState {
    phase: Phase,
    session: Option<SessionContext>,
    risk_open: bool,
    pending_request: Option<PendingRequest>,
    used_request_ids: BTreeSet<u64>,
    used_control_ids: BTreeSet<u64>,
    pending_controls: BTreeSet<u64>,
    final_state: Option<Value>,
}

impl Default for ProtocolState {
    fn default() -> Self {
        Self {
            phase: Phase::AwaitSession,
            session: None,
            risk_open: false,
            pending_request: None,
            used_request_ids: BTreeSet::new(),
            used_control_ids: BTreeSet::new(),
            pending_controls: BTreeSet::new(),
            final_state: None,
        }
    }
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

pub fn verify(workspace_root: &Path) -> Result<(), String> {
    let vectors = load_vectors(workspace_root)?;
    verify_metadata(&vectors)?;

    let sessions = session_map(&vectors)?;
    let mut parity_vectors = Vec::new();
    for vector in &vectors.valid {
        let transcript = expand_transcript(Some(&vector.session), &vector.messages, &sessions)?;
        let final_state =
            run_transcript(&transcript).map_err(|error| format_error(&vector.name, error))?;
        if final_state != vector.expected_state {
            return Err(format!(
                "valid fixture {} final state mismatch\nexpected: {}\nactual: {}",
                vector.name, vector.expected_state, final_state
            ));
        }
        parity_vectors.push(ParityVector {
            name: vector.name.clone(),
            frames_hex: transcript
                .iter()
                .map(|message| hex::encode(frame(message.raw_json.as_bytes())))
                .collect(),
            final_state,
        });
    }

    for vector in &vectors.invalid {
        let result = if let Some(fault) = &vector.transport {
            validate_transport_fault(fault)
        } else {
            let transcript =
                expand_transcript(vector.session.as_deref(), &vector.messages, &sessions)?;
            run_transcript(&transcript).map(|_| ())
        };
        match result {
            Ok(()) => {
                return Err(format!(
                    "invalid fixture {} unexpectedly passed",
                    vector.name
                ));
            }
            Err(error) if error.code == vector.expected_error => {}
            Err(error) => {
                return Err(format!(
                    "invalid fixture {} expected {}, got {}: {}",
                    vector.name, vector.expected_error, error.code, error.message
                ));
            }
        }
    }

    let rust_report = ParityReport {
        preamble_hex: hex::encode(PREAMBLE),
        vectors: parity_vectors,
    };
    verify_node_oracle(workspace_root, &rust_report)?;
    super::revision_core_v1::verify_traceability(workspace_root)?;
    eprintln!(
        "HookProtocolV1: {} valid and {} single-fault invalid fixtures passed Rust, traceability, and Node 24 parity",
        vectors.valid.len(),
        vectors.invalid.len()
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

fn verify_metadata(vectors: &VectorManifest) -> Result<(), String> {
    if vectors.format != "hook_protocol_v1_fixtures" {
        return Err("unexpected HookProtocolV1 fixture manifest kind".to_owned());
    }
    if vectors.status != "candidate" && vectors.status != "frozen" {
        return Err("HookProtocolV1 fixture status must be candidate or frozen".to_owned());
    }
    if vectors.preamble_hex != hex::encode(PREAMBLE) {
        return Err("HookProtocolV1 fixture preamble differs from the specification".to_owned());
    }
    let mut names = BTreeSet::new();
    for name in vectors
        .session_specs
        .iter()
        .map(|fixture| &fixture.name)
        .chain(vectors.valid.iter().map(|vector| &vector.name))
        .chain(vectors.invalid.iter().map(|vector| &vector.name))
    {
        if !names.insert(name) {
            return Err(format!("duplicate HookProtocolV1 fixture name {name}"));
        }
    }
    Ok(())
}

fn session_map(vectors: &VectorManifest) -> Result<BTreeMap<String, String>, String> {
    let sessions: BTreeMap<_, _> = vectors
        .session_specs
        .iter()
        .map(|fixture| (fixture.name.clone(), fixture.raw_json.clone()))
        .collect();
    if sessions.len() != vectors.session_specs.len() {
        return Err("duplicate HookProtocolV1 Session fixture name".to_owned());
    }
    Ok(sessions)
}

fn expand_transcript(
    session: Option<&str>,
    messages: &[WireMessage],
    sessions: &BTreeMap<String, String>,
) -> Result<Vec<WireMessage>, String> {
    let mut transcript = Vec::new();
    if let Some(session) = session {
        let raw_json = sessions
            .get(session)
            .ok_or_else(|| format!("unknown Session fixture {session}"))?;
        transcript.push(WireMessage {
            direction: Direction::PactrunToHook,
            raw_json: raw_json.clone(),
        });
    }
    transcript.extend_from_slice(messages);
    Ok(transcript)
}

fn validate_transport_fault(fault: &TransportFault) -> Result<(), VerifyError> {
    match fault {
        TransportFault::Preamble { hex: value } => {
            let bytes = hex::decode(value)
                .map_err(|_| VerifyError::new("invalid_preamble", "preamble is not hex"))?;
            if bytes != PREAMBLE {
                Err(VerifyError::new(
                    "invalid_preamble",
                    "directional preamble does not select HookProtocolV1",
                ))
            } else {
                Ok(())
            }
        }
        TransportFault::DeclaredLength { length } if *length > MAX_PAYLOAD => Err(
            VerifyError::new("frame_too_large", "declared JSON payload exceeds 16 MiB"),
        ),
        TransportFault::DeclaredLength { .. } => Ok(()),
        TransportFault::PayloadHex { hex: value } => {
            let bytes = hex::decode(value)
                .map_err(|_| VerifyError::new("invalid_utf8", "payload fixture is not hex"))?;
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| VerifyError::new("invalid_utf8", "frame payload is not UTF-8"))?;
            parse_json(text).map(|_| ())
        }
    }
}

fn run_transcript(messages: &[WireMessage]) -> Result<Value, VerifyError> {
    let mut state = ProtocolState::default();
    for message in messages {
        if message.raw_json.len() > MAX_PAYLOAD as usize {
            return Err(VerifyError::new(
                "frame_too_large",
                "JSON payload exceeds 16 MiB",
            ));
        }
        let value = parse_json(&message.raw_json)?;
        process_message(&mut state, message.direction, value)?;
    }
    if state.phase != Phase::Terminal {
        return Err(VerifyError::new(
            "unexpected_message",
            "valid transcript did not reach completion_accepted",
        ));
    }
    state.final_state.ok_or_else(|| {
        VerifyError::new(
            "unexpected_message",
            "terminal transcript has no accepted completion",
        )
    })
}

fn process_message(
    state: &mut ProtocolState,
    direction: Direction,
    value: Value,
) -> Result<(), VerifyError> {
    let object = expect_object(&value, "message")?;
    let message_type = string_field(object, "type")?;
    match (direction, message_type.as_str()) {
        (Direction::PactrunToHook, "session_start") => process_session_start(state, object),
        (Direction::HookToPactrun, "session_ready") => process_session_ready(state, object),
        (Direction::HookToPactrun, "diagnostic") => process_diagnostic(state, object),
        (Direction::HookToPactrun, "request") => process_request(state, object),
        (Direction::PactrunToHook, "request_ack") => process_request_ack(state, object),
        (Direction::PactrunToHook, "cancel") => process_cancel(state, object),
        (Direction::HookToPactrun, "cancel_ack") => process_cancel_ack(state, object),
        (Direction::HookToPactrun, "complete") => process_complete(state, object),
        (Direction::PactrunToHook, "completion_accepted") => {
            process_completion_accepted(state, object)
        }
        (Direction::PactrunToHook, "protocol_error") => {
            process_pactrun_protocol_error(state, object)
        }
        (Direction::HookToPactrun, "protocol_error") => process_hook_protocol_error(state, object),
        _ => Err(VerifyError::new(
            "unexpected_message",
            format!("message {message_type} is invalid in this direction"),
        )),
    }
}

fn process_session_start(
    state: &mut ProtocolState,
    object: &Map<String, Value>,
) -> Result<(), VerifyError> {
    if state.phase != Phase::AwaitSession {
        return Err(unexpected("session_start"));
    }
    let version = safe_integer(required(object, "protocol_version")?)?;
    if version != 1 {
        return Err(VerifyError::new(
            "unsupported_protocol_version",
            "HookProtocolV1 requires protocol_version 1",
        ));
    }
    closed_fields(
        object,
        &[
            "type",
            "protocol_version",
            "session_id",
            "run_id",
            "revision",
            "parameters",
            "workspace",
            "io",
            "operation",
        ],
        &[],
    )?;
    let session_id = machine_id(string_field(object, "session_id")?)?;
    machine_id(string_field(object, "run_id")?)?;
    validate_revision(required(object, "revision")?)?;
    validate_parameters(required(object, "parameters")?)?;
    let workspace_handle = validate_workspace(required(object, "workspace")?)?;
    validate_io(required(object, "io")?)?;
    let operation = validate_operation(required(object, "operation")?, workspace_handle)?;
    state.session = Some(SessionContext {
        session_id,
        operation,
    });
    state.phase = Phase::AwaitReady;
    Ok(())
}

fn process_session_ready(
    state: &mut ProtocolState,
    object: &Map<String, Value>,
) -> Result<(), VerifyError> {
    if state.phase != Phase::AwaitReady {
        return Err(unexpected("session_ready"));
    }
    closed_fields(object, &["type", "protocol_version", "session_id"], &[])?;
    if safe_integer(required(object, "protocol_version")?)? != 1 {
        return Err(VerifyError::new(
            "unsupported_protocol_version",
            "Hook must confirm exact protocol version 1",
        ));
    }
    let session_id = machine_id(string_field(object, "session_id")?)?;
    if state.session.as_ref().map(|value| &value.session_id) != Some(&session_id) {
        return Err(VerifyError::new(
            "invalid_message",
            "session_ready identifies another Session",
        ));
    }
    state.phase = Phase::Active;
    Ok(())
}

fn process_diagnostic(
    state: &ProtocolState,
    object: &Map<String, Value>,
) -> Result<(), VerifyError> {
    require_active(state, "diagnostic")?;
    closed_fields(object, &["type", "severity", "code", "message"], &[])?;
    enum_string(
        string_field(object, "severity")?,
        &["info", "warning", "error"],
    )?;
    hook_code(string_field(object, "code")?)?;
    let _ = string_field(object, "message")?;
    Ok(())
}

fn process_request(
    state: &mut ProtocolState,
    object: &Map<String, Value>,
) -> Result<(), VerifyError> {
    require_active(state, "request")?;
    if state.pending_request.is_some() {
        return Err(VerifyError::new(
            "request_in_flight",
            "only one Hook-originated request may be outstanding",
        ));
    }
    closed_fields(object, &["type", "request_id", "request"], &[])?;
    let request_id = positive_safe_integer(required(object, "request_id")?)?;
    if !state.used_request_ids.insert(request_id) {
        return Err(VerifyError::new("invalid_message", "request_id was reused"));
    }
    let request = expect_object(required(object, "request")?, "request")?;
    closed_fields(request, &["kind"], &[])?;
    let kind = enum_string(
        string_field(request, "kind")?,
        &["enter_recovery_risk", "resolve_recovery_risk"],
    )?;
    if (kind == "enter_recovery_risk" && state.risk_open)
        || (kind == "resolve_recovery_risk" && !state.risk_open)
    {
        return Err(VerifyError::new(
            "invalid_recovery_transition",
            "recovery-risk request is invalid from the current state",
        ));
    }
    state.pending_request = Some(PendingRequest {
        id: request_id,
        kind,
    });
    Ok(())
}

fn process_request_ack(
    state: &mut ProtocolState,
    object: &Map<String, Value>,
) -> Result<(), VerifyError> {
    require_active(state, "request_ack")?;
    closed_fields(object, &["type", "request_id", "risk_state"], &[])?;
    let request_id = positive_safe_integer(required(object, "request_id")?)?;
    let pending = state.pending_request.as_ref().ok_or_else(|| {
        VerifyError::new("unexpected_message", "request_ack has no pending request")
    })?;
    if pending.id != request_id {
        return Err(VerifyError::new(
            "request_id_mismatch",
            "request_ack does not identify the outstanding request",
        ));
    }
    let expected = if pending.kind == "enter_recovery_risk" {
        "open"
    } else {
        "clear"
    };
    if enum_string(string_field(object, "risk_state")?, &["clear", "open"])? != expected {
        return Err(VerifyError::new(
            "invalid_recovery_transition",
            "request_ack risk state does not match the requested transition",
        ));
    }
    state.risk_open = expected == "open";
    state.pending_request = None;
    Ok(())
}

fn process_cancel(
    state: &mut ProtocolState,
    object: &Map<String, Value>,
) -> Result<(), VerifyError> {
    require_active(state, "cancel")?;
    closed_fields(object, &["type", "control_id", "reason"], &[])?;
    let control_id = positive_safe_integer(required(object, "control_id")?)?;
    enum_string(string_field(object, "reason")?, &["requested", "timeout"])?;
    if !state.used_control_ids.insert(control_id) {
        return Err(VerifyError::new("invalid_message", "control_id was reused"));
    }
    state.pending_controls.insert(control_id);
    Ok(())
}

fn process_cancel_ack(
    state: &mut ProtocolState,
    object: &Map<String, Value>,
) -> Result<(), VerifyError> {
    require_active(state, "cancel_ack")?;
    closed_fields(object, &["type", "control_id"], &[])?;
    let control_id = positive_safe_integer(required(object, "control_id")?)?;
    if !state.pending_controls.remove(&control_id) {
        return Err(VerifyError::new(
            "unexpected_message",
            "cancel_ack has no matching cancellation control",
        ));
    }
    Ok(())
}

fn process_complete(
    state: &mut ProtocolState,
    object: &Map<String, Value>,
) -> Result<(), VerifyError> {
    require_active(state, "complete")?;
    if state.pending_request.is_some() {
        return Err(VerifyError::new(
            "request_in_flight",
            "completion is invalid while a Hook request is outstanding",
        ));
    }
    let session = state.session.as_ref().expect("active Session exists");
    let operation = string_field(object, "operation")?;
    if operation != session.operation.kind {
        return Err(VerifyError::new(
            "invalid_completion",
            "completion operation differs from Session operation",
        ));
    }
    let status = enum_string(string_field(object, "status")?, &["success", "failure"])?;
    if status == "success" && state.risk_open {
        return Err(VerifyError::new(
            "completion_with_open_risk",
            "success is invalid while recovery risk remains open",
        ));
    }
    validate_optional_hook_text(object)?;
    let mut summary = Map::new();
    summary.insert(
        "completion".to_owned(),
        Value::String("submitted".to_owned()),
    );
    summary.insert(
        "operation".to_owned(),
        Value::String(session.operation.kind.clone()),
    );
    summary.insert(
        "risk_state".to_owned(),
        Value::String(if state.risk_open { "open" } else { "clear" }.to_owned()),
    );
    match operation.as_str() {
        "action" => {
            closed_fields(
                object,
                &["type", "operation", "status", "produced_outputs"],
                &["code", "message"],
            )?;
            let outputs = authority_handle_set(required(object, "produced_outputs")?)?;
            if !outputs.is_subset(&session.operation.action_outputs) {
                return Err(VerifyError::new(
                    "invalid_authority",
                    "Action submitted an unallocated output handle",
                ));
            }
            summary.insert("submitted_outputs".to_owned(), string_set_value(&outputs));
        }
        "migration" => {
            closed_fields(
                object,
                &["type", "operation", "status", "produced_target_outputs"],
                &["code", "message"],
            )?;
            let outputs = authority_handle_set(required(object, "produced_target_outputs")?)?;
            if !outputs.is_subset(&session.operation.migration_outputs) {
                return Err(VerifyError::new(
                    "invalid_authority",
                    "Migration submitted an unallocated target output handle",
                ));
            }
            if (status == "success" && outputs != session.operation.migration_outputs)
                || (status == "failure" && !outputs.is_empty())
            {
                return Err(VerifyError::new(
                    "invalid_completion",
                    "Migration completion violates mandatory target-output rules",
                ));
            }
            summary.insert("submitted_outputs".to_owned(), string_set_value(&outputs));
        }
        "snapshot_capture" => {
            closed_fields(
                object,
                &["type", "operation", "status", "service_content"],
                &["code", "message"],
            )?;
            let content = normalize_capture_content(required(object, "service_content")?)?;
            if status == "failure" && !content.is_empty() {
                return Err(VerifyError::new(
                    "invalid_completion",
                    "failed Capture cannot submit service content",
                ));
            }
            summary.insert("service_content".to_owned(), Value::Array(content));
        }
        "snapshot_restore" | "cleanup" => {
            closed_fields(
                object,
                &["type", "operation", "status"],
                &["code", "message"],
            )?;
        }
        _ => return Err(VerifyError::new("invalid_completion", "unknown operation")),
    }
    state.final_state = Some(Value::Object(summary));
    state.phase = Phase::AwaitCompletionAccepted;
    Ok(())
}

fn process_completion_accepted(
    state: &mut ProtocolState,
    object: &Map<String, Value>,
) -> Result<(), VerifyError> {
    if state.phase != Phase::AwaitCompletionAccepted {
        return Err(unexpected("completion_accepted"));
    }
    closed_fields(object, &["type"], &[])?;
    state
        .final_state
        .as_mut()
        .and_then(Value::as_object_mut)
        .expect("submitted completion has summary")
        .insert(
            "completion".to_owned(),
            Value::String("accepted".to_owned()),
        );
    state.phase = Phase::Terminal;
    Ok(())
}

fn process_pactrun_protocol_error(
    state: &mut ProtocolState,
    object: &Map<String, Value>,
) -> Result<(), VerifyError> {
    if matches!(state.phase, Phase::AwaitSession | Phase::Terminal) {
        return Err(unexpected("protocol_error"));
    }
    closed_fields(object, &["type", "code"], &["message"])?;
    enum_string(
        string_field(object, "code")?,
        &[
            "invalid_preamble",
            "frame_too_large",
            "invalid_utf8",
            "invalid_json",
            "duplicate_property",
            "invalid_unicode_scalar",
            "unsupported_protocol_version",
            "unknown_field",
            "invalid_message",
            "unexpected_message",
            "request_in_flight",
            "request_id_mismatch",
            "invalid_recovery_transition",
            "invalid_session_relative_path",
            "duplicate_semantic_key",
            "invalid_authority",
            "invalid_completion",
            "completion_with_open_risk",
        ],
    )?;
    optional_string(object, "message")?;
    state.phase = Phase::Terminal;
    Ok(())
}

fn process_hook_protocol_error(
    state: &mut ProtocolState,
    object: &Map<String, Value>,
) -> Result<(), VerifyError> {
    require_active(state, "protocol_error")?;
    closed_fields(object, &["type", "code", "message"], &[])?;
    hook_code(string_field(object, "code")?)?;
    let _ = string_field(object, "message")?;
    let session = state.session.as_ref().expect("active Session exists");
    state.final_state = Some(object_value([
        ("completion", Value::String("protocol_error".to_owned())),
        ("operation", Value::String(session.operation.kind.clone())),
        (
            "risk_state",
            Value::String(if state.risk_open { "open" } else { "clear" }.to_owned()),
        ),
    ]));
    state.phase = Phase::Terminal;
    Ok(())
}

fn validate_revision(value: &Value) -> Result<(), VerifyError> {
    let object = expect_object(value, "RevisionIdentityV1")?;
    closed_fields(object, &["package_id", "revision_content_digest"], &[])?;
    machine_id(string_field(object, "package_id")?)?;
    sha256_digest(string_field(object, "revision_content_digest")?)?;
    Ok(())
}

fn validate_parameters(value: &Value) -> Result<(), VerifyError> {
    let mut ids = BTreeSet::new();
    for value in expect_array(value, "parameters")? {
        let object = expect_object(value, "ParameterBindingV1")?;
        closed_fields(object, &["parameter_id", "value"], &[])?;
        let id = identifier(string_field(object, "parameter_id")?)?;
        if !ids.insert(id) {
            return Err(duplicate("parameter_id"));
        }
        match required(object, "value")? {
            Value::Bool(_) | Value::String(_) | Value::Number(_) => {}
            _ => {
                return Err(VerifyError::new(
                    "invalid_message",
                    "parameter value must be a scalar",
                ));
            }
        }
    }
    Ok(())
}

fn validate_workspace(value: &Value) -> Result<String, VerifyError> {
    let object = expect_object(value, "WorkspaceAuthorityV1")?;
    closed_fields(object, &["handle", "root_path"], &[])?;
    let handle = machine_id(string_field(object, "handle")?)?;
    host_path(string_field(object, "root_path")?)?;
    Ok(handle)
}

fn validate_io(value: &Value) -> Result<(), VerifyError> {
    let object = expect_object(value, "IOContractV1")?;
    closed_fields(object, &["terminal"], &[])?;
    enum_string(
        string_field(object, "terminal")?,
        &["none", "output", "interactive"],
    )?;
    Ok(())
}

fn validate_operation(
    value: &Value,
    workspace_handle: String,
) -> Result<OperationContext, VerifyError> {
    let object = expect_object(value, "OperationSessionV1")?;
    let kind = string_field(object, "kind")?;
    let mut context = OperationContext {
        kind: kind.clone(),
        action_outputs: BTreeSet::new(),
        migration_outputs: BTreeSet::new(),
    };
    let mut handles = BTreeSet::from([workspace_handle]);
    match kind.as_str() {
        "action" => {
            closed_fields(
                object,
                &["kind", "action_id", "access", "bindings", "outputs"],
                &[],
            )?;
            identifier(string_field(object, "action_id")?)?;
            enum_string(string_field(object, "access")?, &["observe", "mutate"])?;
            validate_bindings(
                required(object, "bindings")?,
                RoleProfile::ActiveOnly,
                &mut handles,
            )?;
            context.action_outputs =
                validate_action_outputs(required(object, "outputs")?, &mut handles)?;
        }
        "snapshot_capture" => {
            closed_fields(object, &["kind", "access", "bindings", "candidate"], &[])?;
            enum_string(string_field(object, "access")?, &["observe", "mutate"])?;
            validate_bindings(
                required(object, "bindings")?,
                RoleProfile::ActiveOnly,
                &mut handles,
            )?;
            let candidate = expect_object(
                required(object, "candidate")?,
                "SnapshotCandidateAuthorityV1",
            )?;
            closed_fields(candidate, &["handle", "root_path"], &[])?;
            insert_handle(
                &mut handles,
                machine_id(string_field(candidate, "handle")?)?,
            )?;
            host_path(string_field(candidate, "root_path")?)?;
        }
        "snapshot_restore" => {
            closed_fields(
                object,
                &["kind", "snapshot_id", "bindings", "snapshot_content"],
                &[],
            )?;
            machine_id(string_field(object, "snapshot_id")?)?;
            validate_bindings(
                required(object, "bindings")?,
                RoleProfile::ActiveOnly,
                &mut handles,
            )?;
            validate_snapshot_content(required(object, "snapshot_content")?, &mut handles)?;
        }
        "migration" => {
            closed_fields(
                object,
                &[
                    "kind",
                    "source_revision",
                    "source_bindings",
                    "target_bindings",
                    "target_outputs",
                ],
                &[],
            )?;
            validate_revision(required(object, "source_revision")?)?;
            validate_bindings(
                required(object, "source_bindings")?,
                RoleProfile::ActiveAndRetained,
                &mut handles,
            )?;
            validate_bindings(
                required(object, "target_bindings")?,
                RoleProfile::ActiveOnly,
                &mut handles,
            )?;
            context.migration_outputs =
                validate_migration_outputs(required(object, "target_outputs")?, &mut handles)?;
        }
        "cleanup" => {
            closed_fields(object, &["kind", "bindings"], &[])?;
            validate_bindings(
                required(object, "bindings")?,
                RoleProfile::ActiveAndRetained,
                &mut handles,
            )?;
        }
        _ => {
            return Err(VerifyError::new(
                "invalid_message",
                "unknown operation Session kind",
            ));
        }
    }
    Ok(context)
}

#[derive(Clone, Copy)]
enum RoleProfile {
    ActiveOnly,
    ActiveAndRetained,
}

fn validate_bindings(
    value: &Value,
    profile: RoleProfile,
    handles: &mut BTreeSet<String>,
) -> Result<Vec<(String, String)>, VerifyError> {
    let mut keys = BTreeSet::new();
    for value in expect_array(value, "bindings")? {
        let object = expect_object(value, "BindingAuthorityV1")?;
        closed_fields(
            object,
            &["handle", "input_id", "role", "readonly_path"],
            &[],
        )?;
        insert_handle(handles, machine_id(string_field(object, "handle")?)?)?;
        let input_id = identifier(string_field(object, "input_id")?)?;
        let role = enum_string(string_field(object, "role")?, &["active", "retained"])?;
        if matches!(profile, RoleProfile::ActiveOnly) && role != "active" {
            return Err(VerifyError::new(
                "invalid_message",
                "this operation exposes active bindings only",
            ));
        }
        host_path(string_field(object, "readonly_path")?)?;
        if !keys.insert((role, input_id)) {
            return Err(duplicate("binding role/input_id"));
        }
    }
    Ok(keys.into_iter().collect())
}

fn validate_action_outputs(
    value: &Value,
    handles: &mut BTreeSet<String>,
) -> Result<BTreeSet<String>, VerifyError> {
    let mut output_ids = BTreeSet::new();
    let mut output_handles = BTreeSet::new();
    for value in expect_array(value, "outputs")? {
        let object = expect_object(value, "ActionOutputAuthorityV1")?;
        closed_fields(object, &["handle", "output_id", "staged_path"], &[])?;
        let handle = machine_id(string_field(object, "handle")?)?;
        insert_handle(handles, handle.clone())?;
        if !output_ids.insert(identifier(string_field(object, "output_id")?)?) {
            return Err(duplicate("Action output_id"));
        }
        host_path(string_field(object, "staged_path")?)?;
        output_handles.insert(handle);
    }
    Ok(output_handles)
}

fn validate_migration_outputs(
    value: &Value,
    handles: &mut BTreeSet<String>,
) -> Result<BTreeSet<String>, VerifyError> {
    let mut input_ids = BTreeSet::new();
    let mut output_handles = BTreeSet::new();
    for value in expect_array(value, "target_outputs")? {
        let object = expect_object(value, "MigrationOutputAuthorityV1")?;
        closed_fields(object, &["handle", "input_id", "staged_path"], &[])?;
        let handle = machine_id(string_field(object, "handle")?)?;
        insert_handle(handles, handle.clone())?;
        if !input_ids.insert(identifier(string_field(object, "input_id")?)?) {
            return Err(duplicate("Migration target input_id"));
        }
        host_path(string_field(object, "staged_path")?)?;
        output_handles.insert(handle);
    }
    Ok(output_handles)
}

fn validate_snapshot_content(
    value: &Value,
    handles: &mut BTreeSet<String>,
) -> Result<(), VerifyError> {
    let object = expect_object(value, "SnapshotContentRootAuthorityV1")?;
    closed_fields(
        object,
        &["handle", "readonly_root_path", "logical_descriptors"],
        &[],
    )?;
    insert_handle(handles, machine_id(string_field(object, "handle")?)?)?;
    host_path(string_field(object, "readonly_root_path")?)?;
    let mut keys = BTreeSet::new();
    for value in expect_array(
        required(object, "logical_descriptors")?,
        "logical_descriptors",
    )? {
        let descriptor = expect_object(value, "SnapshotContentDescriptorV1")?;
        closed_fields(
            descriptor,
            &["role", "path", "blob_digest", "materialized_path"],
            &[],
        )?;
        let key = (
            identifier(string_field(descriptor, "role")?)?,
            portable_path(string_field(descriptor, "path")?)?,
        );
        if !keys.insert(key) {
            return Err(duplicate("Snapshot content role/path"));
        }
        sha256_digest(string_field(descriptor, "blob_digest")?)?;
        portable_path(string_field(descriptor, "materialized_path")?)?;
    }
    Ok(())
}

fn normalize_capture_content(value: &Value) -> Result<Vec<Value>, VerifyError> {
    let mut content = BTreeMap::new();
    for value in expect_array(value, "service_content")? {
        let object = expect_object(value, "CaptureServiceContentSubmissionV1")?;
        closed_fields(object, &["role", "path", "candidate_path"], &[])?;
        let role = identifier(string_field(object, "role")?)?;
        let path = portable_path(string_field(object, "path")?)?;
        let candidate_path = portable_path(string_field(object, "candidate_path")?)?;
        if content
            .insert(
                (role.clone(), path.clone()),
                object_value([
                    ("role", Value::String(role)),
                    ("path", Value::String(path)),
                    ("candidate_path", Value::String(candidate_path)),
                ]),
            )
            .is_some()
        {
            return Err(duplicate("Capture service-content role/path"));
        }
    }
    Ok(content.into_values().collect())
}

fn validate_optional_hook_text(object: &Map<String, Value>) -> Result<(), VerifyError> {
    if let Some(value) = object.get("code") {
        hook_code(
            value
                .as_str()
                .ok_or_else(|| {
                    VerifyError::new("invalid_message", "completion code must be a string")
                })?
                .to_owned(),
        )?;
    }
    optional_string(object, "message")?;
    Ok(())
}

fn authority_handle_set(value: &Value) -> Result<BTreeSet<String>, VerifyError> {
    let values = expect_array(value, "authority handles")?;
    let mut output = BTreeSet::new();
    for value in values {
        let handle = machine_id(
            value
                .as_str()
                .ok_or_else(|| VerifyError::new("invalid_authority", "handle must be a string"))?
                .to_owned(),
        )?;
        if !output.insert(handle) {
            return Err(duplicate("submitted authority handle"));
        }
    }
    Ok(output)
}

fn string_set_value(values: &BTreeSet<String>) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}

fn insert_handle(handles: &mut BTreeSet<String>, handle: String) -> Result<(), VerifyError> {
    if handles.insert(handle) {
        Ok(())
    } else {
        Err(duplicate("Session authority handle"))
    }
}

fn frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

fn machine_id(value: String) -> Result<String, VerifyError> {
    if value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(value)
    } else {
        Err(VerifyError::new(
            "invalid_message",
            "machine identity or authority handle must be 32 lowercase hex characters",
        ))
    }
}

fn sha256_digest(value: String) -> Result<String, VerifyError> {
    let Some(hex_value) = value.strip_prefix("sha256:") else {
        return Err(VerifyError::new(
            "invalid_message",
            "digest must use sha256 prefix",
        ));
    };
    if hex_value.len() == 64
        && hex_value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(value)
    } else {
        Err(VerifyError::new(
            "invalid_message",
            "digest must contain 64 lowercase hex characters",
        ))
    }
}

fn identifier(value: String) -> Result<String, VerifyError> {
    if crate::lexical_v1::is_semantic_identifier(&value) {
        Ok(value)
    } else {
        Err(VerifyError::new(
            "invalid_message",
            format!("invalid semantic identifier {value:?}"),
        ))
    }
}

fn hook_code(value: String) -> Result<String, VerifyError> {
    identifier(value)
}

fn portable_path(value: String) -> Result<String, VerifyError> {
    if crate::lexical_v1::is_runtime_path(&value) {
        Ok(value)
    } else {
        Err(VerifyError::new(
            "invalid_session_relative_path",
            format!("invalid PortableSessionRelativePathV1 {value:?}"),
        ))
    }
}

fn host_path(value: String) -> Result<String, VerifyError> {
    if value.is_empty() || value.contains('\0') {
        Err(VerifyError::new(
            "invalid_message",
            "host-native absolute path must be non-empty and contain no NUL",
        ))
    } else {
        Ok(value)
    }
}

fn positive_safe_integer(value: &Value) -> Result<u64, VerifyError> {
    let value = safe_integer(value)?;
    if value == 0 {
        Err(VerifyError::new(
            "invalid_message",
            "identifier integer must be positive",
        ))
    } else {
        Ok(value)
    }
}

fn safe_integer(value: &Value) -> Result<u64, VerifyError> {
    value
        .as_u64()
        .filter(|value| *value <= MAX_SAFE_INTEGER)
        .ok_or_else(|| VerifyError::new("invalid_message", "expected a nonnegative safe integer"))
}

fn enum_string(value: String, allowed: &[&str]) -> Result<String, VerifyError> {
    if allowed.contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(VerifyError::new(
            "invalid_message",
            format!("unknown enum token {value}"),
        ))
    }
}

fn require_active(state: &ProtocolState, message: &str) -> Result<(), VerifyError> {
    if state.phase == Phase::Active {
        Ok(())
    } else {
        Err(unexpected(message))
    }
}

fn unexpected(message: &str) -> VerifyError {
    VerifyError::new(
        "unexpected_message",
        format!("{message} is invalid in the current state"),
    )
}

fn duplicate(what: &str) -> VerifyError {
    VerifyError::new(
        "duplicate_semantic_key",
        format!("duplicate semantic key: {what}"),
    )
}

fn expect_object<'a>(value: &'a Value, name: &str) -> Result<&'a Map<String, Value>, VerifyError> {
    value
        .as_object()
        .ok_or_else(|| VerifyError::new("invalid_message", format!("{name} must be an object")))
}

fn expect_array<'a>(value: &'a Value, name: &str) -> Result<&'a [Value], VerifyError> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| VerifyError::new("invalid_message", format!("{name} must be an array")))
}

fn required<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a Value, VerifyError> {
    object
        .get(field)
        .ok_or_else(|| VerifyError::new("invalid_message", format!("missing field {field}")))
}

fn string_field(object: &Map<String, Value>, field: &str) -> Result<String, VerifyError> {
    required(object, field)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| VerifyError::new("invalid_message", format!("{field} must be a string")))
}

fn optional_string(object: &Map<String, Value>, field: &str) -> Result<(), VerifyError> {
    if object.get(field).is_some_and(|value| !value.is_string()) {
        Err(VerifyError::new(
            "invalid_message",
            format!("{field} must be a string"),
        ))
    } else {
        Ok(())
    }
}

fn closed_fields(
    object: &Map<String, Value>,
    required_fields: &[&str],
    optional_fields: &[&str],
) -> Result<(), VerifyError> {
    for field in object.keys() {
        if !required_fields.contains(&field.as_str()) && !optional_fields.contains(&field.as_str())
        {
            return Err(VerifyError::new(
                "unknown_field",
                format!("unknown field {field}"),
            ));
        }
    }
    for field in required_fields {
        if !object.contains_key(*field) {
            return Err(VerifyError::new(
                "invalid_message",
                format!("missing field {field}"),
            ));
        }
    }
    Ok(())
}

fn object_value<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn format_error(name: &str, error: VerifyError) -> String {
    format!(
        "fixture {name} failed with {}: {}",
        error.code, error.message
    )
}

fn verify_node_oracle(workspace_root: &Path, expected: &ParityReport) -> Result<(), String> {
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
            "HookProtocolV1 oracle requires Node 24, found {}",
            version.trim()
        ));
    }
    let output = Command::new(&node)
        .arg(workspace_root.join(NODE_ORACLE_PATH))
        .arg(workspace_root.join(VECTOR_PATH))
        .arg("--report")
        .current_dir(workspace_root)
        .output()
        .map_err(|error| format!("could not start Node oracle: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Node oracle failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let actual: ParityReport = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("could not parse Node oracle report: {error}"))?;
    if &actual != expected {
        return Err(format!(
            "Node parity mismatch\nexpected: {}\nactual: {}",
            serde_json::to_string(expected).unwrap_or_default(),
            serde_json::to_string(&actual).unwrap_or_default()
        ));
    }
    Ok(())
}

fn parse_json(input: &str) -> Result<Value, VerifyError> {
    let mut parser = JsonParser {
        bytes: input.as_bytes(),
        position: 0,
    };
    let value = parser.parse_value()?;
    parser.skip_whitespace();
    if parser.position != parser.bytes.len() {
        return Err(VerifyError::new(
            "invalid_json",
            "trailing data after JSON value",
        ));
    }
    if !value.is_object() {
        return Err(VerifyError::new(
            "invalid_message",
            "frame payload must be a JSON object",
        ));
    }
    Ok(value)
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl JsonParser<'_> {
    fn parse_value(&mut self) -> Result<Value, VerifyError> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'n') => {
                self.keyword(b"null")?;
                Ok(Value::Null)
            }
            Some(b't') => {
                self.keyword(b"true")?;
                Ok(Value::Bool(true))
            }
            Some(b'f') => {
                self.keyword(b"false")?;
                Ok(Value::Bool(false))
            }
            Some(b'"') => self.parse_string().map(Value::String),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            _ => Err(VerifyError::new("invalid_json", "expected a JSON value")),
        }
    }

    fn parse_array(&mut self) -> Result<Value, VerifyError> {
        self.position += 1;
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.take(b']') {
            return Ok(Value::Array(values));
        }
        loop {
            values.push(self.parse_value()?);
            self.skip_whitespace();
            if self.take(b']') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(Value::Array(values))
    }

    fn parse_object(&mut self) -> Result<Value, VerifyError> {
        self.position += 1;
        let mut fields = Map::new();
        self.skip_whitespace();
        if self.take(b'}') {
            return Ok(Value::Object(fields));
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect(b':')?;
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
            self.expect(b',')?;
        }
        Ok(Value::Object(fields))
    }

    fn parse_string(&mut self) -> Result<String, VerifyError> {
        self.expect(b'"')?;
        let mut output = String::new();
        loop {
            match self
                .peek()
                .ok_or_else(|| VerifyError::new("invalid_json", "unterminated JSON string"))?
            {
                b'"' => {
                    self.position += 1;
                    return Ok(output);
                }
                b'\\' => {
                    self.position += 1;
                    let escaped = self.peek().ok_or_else(|| {
                        VerifyError::new("invalid_json", "unterminated JSON escape")
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
                            return Err(VerifyError::new("invalid_json", "unknown JSON escape"));
                        }
                    }
                }
                0x00..=0x1f => {
                    return Err(VerifyError::new(
                        "invalid_json",
                        "unescaped control character",
                    ));
                }
                0x20..=0x7f => {
                    output.push(char::from(self.bytes[self.position]));
                    self.position += 1;
                }
                _ => {
                    let rest = std::str::from_utf8(&self.bytes[self.position..])
                        .map_err(|_| VerifyError::new("invalid_unicode_scalar", "invalid UTF-8"))?;
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
        let first = self.hex_quad()?;
        let scalar = if (0xd800..=0xdbff).contains(&first) {
            if self.peek() != Some(b'\\') || self.bytes.get(self.position + 1) != Some(&b'u') {
                return Err(VerifyError::new(
                    "invalid_unicode_scalar",
                    "lone high surrogate",
                ));
            }
            self.position += 2;
            let second = self.hex_quad()?;
            if !(0xdc00..=0xdfff).contains(&second) {
                return Err(VerifyError::new(
                    "invalid_unicode_scalar",
                    "invalid surrogate pair",
                ));
            }
            0x10000 + (((u32::from(first) - 0xd800) << 10) | (u32::from(second) - 0xdc00))
        } else if (0xdc00..=0xdfff).contains(&first) {
            return Err(VerifyError::new(
                "invalid_unicode_scalar",
                "lone low surrogate",
            ));
        } else {
            u32::from(first)
        };
        output.push(
            char::from_u32(scalar).ok_or_else(|| {
                VerifyError::new("invalid_unicode_scalar", "invalid Unicode scalar")
            })?,
        );
        Ok(())
    }

    fn hex_quad(&mut self) -> Result<u16, VerifyError> {
        if self.position + 4 > self.bytes.len() {
            return Err(VerifyError::new("invalid_json", "short Unicode escape"));
        }
        let mut value = 0_u16;
        for _ in 0..4 {
            let digit = self.bytes[self.position];
            self.position += 1;
            let digit = match digit {
                b'0'..=b'9' => digit - b'0',
                b'a'..=b'f' => digit - b'a' + 10,
                b'A'..=b'F' => digit - b'A' + 10,
                _ => return Err(VerifyError::new("invalid_json", "invalid Unicode escape")),
            };
            value = value * 16 + u16::from(digit);
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<Value, VerifyError> {
        let start = self.position;
        self.take(b'-');
        match self.peek() {
            Some(b'0') => {
                self.position += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return Err(VerifyError::new("invalid_json", "leading zero"));
                }
            }
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.position += 1;
                }
            }
            _ => return Err(VerifyError::new("invalid_json", "invalid number")),
        }
        if self.take(b'.') {
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(VerifyError::new("invalid_json", "invalid fraction"));
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
                return Err(VerifyError::new("invalid_json", "invalid exponent"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.position += 1;
            }
        }
        let token = std::str::from_utf8(&self.bytes[start..self.position])
            .map_err(|_| VerifyError::new("invalid_utf8", "invalid UTF-8 number"))?;
        let number: Number = token.parse().map_err(|_| {
            VerifyError::new("invalid_message", "JSON number is not finite binary64")
        })?;
        Ok(Value::Number(number))
    }

    fn keyword(&mut self, keyword: &[u8]) -> Result<(), VerifyError> {
        if self.bytes.get(self.position..self.position + keyword.len()) == Some(keyword) {
            self.position += keyword.len();
            Ok(())
        } else {
            Err(VerifyError::new("invalid_json", "invalid JSON keyword"))
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.position += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), VerifyError> {
        if self.take(byte) {
            Ok(())
        } else {
            Err(VerifyError::new("invalid_json", "unexpected JSON token"))
        }
    }

    fn take(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
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
        load_vectors(&workspace_root()).expect("load HookProtocolV1 fixtures")
    }

    fn run_valid(name: &str) -> Value {
        let vectors = vectors();
        let sessions = session_map(&vectors).unwrap();
        let vector = vectors
            .valid
            .iter()
            .find(|value| value.name == name)
            .unwrap();
        run_transcript(
            &expand_transcript(Some(&vector.session), &vector.messages, &sessions).unwrap(),
        )
        .unwrap()
    }

    fn invalid_code(name: &str) -> String {
        let vectors = vectors();
        let sessions = session_map(&vectors).unwrap();
        let vector = vectors
            .invalid
            .iter()
            .find(|value| value.name == name)
            .unwrap();
        let error = if let Some(fault) = &vector.transport {
            validate_transport_fault(fault).unwrap_err()
        } else {
            run_transcript(
                &expand_transcript(vector.session.as_deref(), &vector.messages, &sessions).unwrap(),
            )
            .unwrap_err()
        };
        error.code.to_owned()
    }

    // Test-ID: PR-TEST-0020
    // Verifies: PR-REQ-0204, PR-REQ-0205
    #[test]
    fn directional_preamble_and_frame_limit_are_exact() {
        assert_eq!(hex::encode(PREAMBLE), vectors().preamble_hex);
        assert_eq!(invalid_code("wrong_preamble"), "invalid_preamble");
        assert_eq!(invalid_code("oversized_frame"), "frame_too_large");
    }

    // Test-ID: PR-TEST-0021
    // Verifies: PR-REQ-0206, PR-REQ-0217
    #[test]
    fn strict_json_rejects_duplicate_properties_and_invalid_scalars() {
        assert_eq!(invalid_code("duplicate_property"), "duplicate_property");
        assert_eq!(invalid_code("invalid_utf8_payload"), "invalid_utf8");
        assert_eq!(invalid_code("malformed_json"), "invalid_json");
        assert_eq!(
            invalid_code("lone_high_surrogate"),
            "invalid_unicode_scalar"
        );
    }

    // Test-ID: PR-TEST-0022
    // Verifies: PR-REQ-0207, PR-REQ-0209
    #[test]
    fn handshake_confirms_exact_version_without_features() {
        assert_eq!(
            invalid_code("unsupported_version"),
            "unsupported_protocol_version"
        );
        assert_eq!(invalid_code("features_field_removed"), "unknown_field");
    }

    // Test-ID: PR-TEST-0023
    // Verifies: PR-REQ-0208, PR-REQ-0217
    #[test]
    fn requests_serialize_while_cancellation_is_independent() {
        assert_eq!(
            invalid_code("second_outstanding_request"),
            "request_in_flight"
        );
        assert_eq!(
            run_valid("asynchronous_cancellation")["completion"],
            "accepted"
        );
    }

    // Test-ID: PR-TEST-0024
    // Verifies: PR-REQ-0209, PR-REQ-0214
    #[test]
    fn diagnostics_and_hook_owned_codes_have_closed_schema() {
        assert_eq!(
            run_valid("action_success_subset_observe_workspace")["completion"],
            "accepted"
        );
        assert!(hook_code("report_ready".to_owned()).is_ok());
        assert!(hook_code("PactrunError".to_owned()).is_err());
    }

    // Test-ID: PR-TEST-0025
    // Verifies: PR-REQ-0210, PR-REQ-0218
    #[test]
    fn observe_session_keeps_workspace_authority_writable_by_contract() {
        let vectors = vectors();
        let raw = &vectors
            .session_specs
            .iter()
            .find(|value| value.name == "action_observe")
            .unwrap()
            .raw_json;
        let mut state = ProtocolState::default();
        process_message(
            &mut state,
            Direction::PactrunToHook,
            parse_json(raw).unwrap(),
        )
        .unwrap();
        let message = parse_json(raw).unwrap();
        assert_eq!(message["operation"]["access"], "observe");
        assert_eq!(message["workspace"]["root_path"], "/pactrun/workspace");
        assert_eq!(message["workspace"]["handle"].as_str().unwrap().len(), 32);
    }

    // Test-ID: PR-TEST-0026
    // Verifies: PR-REQ-0211, PR-REQ-0218
    #[test]
    fn operation_contexts_encode_their_binding_visibility() {
        let vectors = vectors();
        for (session_name, retained_allowed) in [
            ("action_observe", false),
            ("capture", false),
            ("restore", false),
            ("migration", true),
            ("cleanup", true),
        ] {
            let raw = &vectors
                .session_specs
                .iter()
                .find(|value| value.name == session_name)
                .unwrap()
                .raw_json;
            let mut state = ProtocolState::default();
            process_message(
                &mut state,
                Direction::PactrunToHook,
                parse_json(raw).unwrap(),
            )
            .unwrap();
            let message = parse_json(raw).unwrap();
            let operation = message["operation"].as_object().unwrap();
            let binding_fields = if session_name == "migration" {
                vec!["source_bindings", "target_bindings"]
            } else {
                vec!["bindings"]
            };
            let has_retained = binding_fields.into_iter().any(|field| {
                operation
                    .get(field)
                    .and_then(Value::as_array)
                    .is_some_and(|values| values.iter().any(|value| value["role"] == "retained"))
            });
            assert_eq!(has_retained, retained_allowed, "{session_name}");
        }
        assert_eq!(
            invalid_code("restore_retained_binding_exposed"),
            "invalid_message"
        );
    }

    // Test-ID: PR-TEST-0027
    // Verifies: PR-REQ-0210, PR-REQ-0212, PR-REQ-0216
    #[test]
    fn action_subsets_and_migration_mandatory_outputs_are_distinct() {
        assert_eq!(
            run_valid("action_failure_artifact_subset")["completion"],
            "accepted"
        );
        assert_eq!(
            invalid_code("migration_success_missing_output"),
            "invalid_completion"
        );
        assert_eq!(
            invalid_code("migration_failure_submits_output"),
            "invalid_completion"
        );
    }

    // Test-ID: PR-TEST-0028
    // Verifies: PR-REQ-0215, PR-REQ-0216, PR-REQ-0217
    #[test]
    fn recovery_risk_changes_only_after_matching_ack() {
        assert_eq!(run_valid("recovery_risk_round_trip")["risk_state"], "clear");
        assert_eq!(
            invalid_code("request_ack_id_mismatch"),
            "request_id_mismatch"
        );
        assert_eq!(
            invalid_code("resolve_risk_while_clear"),
            "invalid_recovery_transition"
        );
        assert_eq!(
            invalid_code("success_with_open_risk"),
            "completion_with_open_risk"
        );
        assert_eq!(
            run_valid("failure_with_open_recovery_risk")["risk_state"],
            "open"
        );
    }

    // Test-ID: PR-TEST-0029
    // Verifies: PR-REQ-0210, PR-REQ-0213, PR-REQ-0218
    #[test]
    fn snapshot_authorities_reuse_frozen_paths_and_semantic_sets() {
        let empty = run_valid("capture_empty_service_content");
        assert_eq!(empty["service_content"], Value::Array(Vec::new()));
        assert_eq!(
            run_valid("capture_service_content_order_a"),
            run_valid("capture_service_content_order_b")
        );
        for name in [
            "candidate_absolute_path",
            "candidate_leading_slash",
            "candidate_trailing_slash",
            "candidate_empty_segment",
            "candidate_dot_segment",
            "candidate_dot_dot_segment",
            "candidate_backslash_separator",
            "candidate_nul",
            "restore_bad_materialized_path",
        ] {
            assert_eq!(
                invalid_code(name),
                "invalid_session_relative_path",
                "{name}"
            );
        }
        assert_eq!(
            invalid_code("candidate_malformed_unicode"),
            "invalid_unicode_scalar"
        );
    }

    // Test-ID: PR-TEST-0030
    // Verifies: PR-REQ-0208, PR-REQ-0216, PR-REQ-0217
    #[test]
    fn completion_handshake_rejects_invalid_authority_and_state() {
        assert_eq!(
            invalid_code("undeclared_action_output"),
            "invalid_authority"
        );
        assert_eq!(
            invalid_code("capture_failure_submits_content"),
            "invalid_completion"
        );
        assert_eq!(
            run_valid("hook_protocol_error")["completion"],
            "protocol_error"
        );
        assert_eq!(invalid_code("eof_before_completion"), "unexpected_message");
    }

    // Test-ID: PR-TEST-0032
    // Verifies: PR-REQ-0204, PR-REQ-0218
    #[test]
    fn candidate_or_frozen_metadata_and_traceability_are_valid() {
        let vectors = vectors();
        assert!(matches!(vectors.status.as_str(), "candidate" | "frozen"));
        super::super::revision_core_v1::verify_traceability(&workspace_root()).unwrap();
    }
}
