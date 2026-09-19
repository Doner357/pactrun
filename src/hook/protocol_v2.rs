//! Explicitly owner-selected V2 decoding and messaging state. Common V1 message
//! grammars/transitions are reused, but never its version negotiation or wire
//! preamble. An accepted target proposal is not Hook success or a durable commit.
use super::*;
use crate::domain::{
    ServiceAccessMode, ServiceAccessV2, ServiceResourceKind, ServiceRole, ServiceScope, ServiceView,
};

pub(in crate::hook) const PREAMBLE_V2: &[u8] = b"pactrun.hook-protocol\0\0\0\0\x02";

/// Declaration facts selected by admission, independently of materialized paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::hook) struct ExpectedAuthority {
    pub(in crate::hook) access: ServiceAccessV2,
    pub(in crate::hook) resource_kind: Option<ServiceResourceKind>,
}

pub(in crate::hook) struct MaterializedAuthority {
    pub(in crate::hook) declaration: ExpectedAuthority,
    pub(in crate::hook) handle: String,
    pub(in crate::hook) path: String,
}

/// Immutable wire bytes paired with the matching owner-selected messaging
/// state. Common fields come from existing typed materializers, not a Pack JSON
/// envelope. This builder qualifies the new authority set; it does not replace
/// filesystem admission or claim a native-path sandbox.
pub(in crate::hook) struct PreparedSession {
    payload: Vec<u8>,
    operation: SessionOperation,
    state: State,
}
pub(in crate::hook) struct PreparedTransport {
    payload: Vec<u8>,
    operation: SessionOperation,
}
impl PreparedSession {
    pub(in crate::hook) fn into_parts(self) -> (PreparedTransport, State) {
        (
            PreparedTransport {
                payload: self.payload,
                operation: self.operation,
            },
            self.state,
        )
    }
    pub(in crate::hook) fn new(
        mut common: Value,
        operation: SessionOperation,
        expected: &[ExpectedAuthority],
        mut authorities: Vec<MaterializedAuthority>,
        requires_target_commit: bool,
        target_commit: Option<String>,
    ) -> Result<Self, Failure> {
        let object = common
            .as_object()
            .ok_or_else(|| Failure::new("invalid_frame"))?;
        let keys = [
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
        if object.len() != keys.len()
            || keys.iter().any(|key| !object.contains_key(*key))
            || common["type"] != "session_start"
            || common["protocol_version"] != 2
            || common["operation"]["kind"] != operation.wire_name()
            || requires_target_commit != target_commit.is_some()
            || (requires_target_commit && operation != SessionOperation::Migration)
        {
            return Err(Failure::new("invalid_frame"));
        }
        let session_id = machine_value(&common["session_id"])?;
        let _ = machine_value(&common["run_id"])?;
        let mut handles = BTreeSet::new();
        insert_handle(&mut handles, &common["workspace"]["handle"])?;
        let op = &common["operation"];
        let mut outputs = BTreeSet::new();
        match operation {
            SessionOperation::Action => {
                array_handles(&mut handles, &op["bindings"])?;
                outputs = array_handles(&mut handles, &op["outputs"])?;
            }
            SessionOperation::Capture => {
                array_handles(&mut handles, &op["bindings"])?;
                insert_handle(&mut handles, &op["candidate"]["handle"])?;
            }
            SessionOperation::Restore => {
                array_handles(&mut handles, &op["bindings"])?;
                insert_handle(&mut handles, &op["snapshot_content"]["handle"])?;
            }
            SessionOperation::Cleanup => {
                array_handles(&mut handles, &op["bindings"])?;
            }
            SessionOperation::Migration => {
                array_handles(&mut handles, &op["source_bindings"])?;
                array_handles(&mut handles, &op["target_bindings"])?;
                outputs = array_handles(&mut handles, &op["target_outputs"])?;
            }
        }
        let mut declarations = BTreeMap::new();
        for declaration in expected {
            validate_declaration_shape(declaration)?;
            let reference = &declaration.access.reference;
            let valid_role = if operation == SessionOperation::Migration {
                reference.view == ServiceView::Source
                    || (reference.view == ServiceView::Target
                        && reference.role == ServiceRole::Active)
            } else {
                reference.view == ServiceView::Current
                    && (reference.role == ServiceRole::Active
                        || operation == SessionOperation::Cleanup)
            };
            if !valid_role
                || (matches!(
                    operation,
                    SessionOperation::Action | SessionOperation::Capture
                ) && op["access"] == "observe"
                    && declaration.access.mode == ServiceAccessMode::Write)
            {
                return Err(Failure::new("invalid_authority"));
            }
            if declarations
                .insert(&declaration.access.reference, declaration)
                .is_some()
            {
                return Err(Failure::new("invalid_authority"));
            }
        }
        if declarations.len() != authorities.len() {
            return Err(Failure::new("invalid_authority"));
        }
        authorities.sort_by(|a, b| {
            a.declaration
                .access
                .reference
                .cmp(&b.declaration.access.reference)
        });
        let mut references = BTreeSet::new();
        let mut wire = Vec::with_capacity(authorities.len());
        for authority in authorities {
            validate_declaration_shape(&authority.declaration)?;
            if declarations
                .get(&authority.declaration.access.reference)
                .copied()
                != Some(&authority.declaration)
                || !references.insert(authority.declaration.access.reference.clone())
                || authority.path.is_empty()
                || authority.path.contains('\0')
                || !std::path::Path::new(&authority.path).is_absolute()
            {
                return Err(Failure::new("invalid_authority"));
            }
            insert_handle(&mut handles, &Value::String(authority.handle.clone()))?;
            let mut value = serde_json::json!({"handle":authority.handle, "reference":authority.declaration.access.reference,
                "mode":authority.declaration.access.mode, "path":authority.path});
            if let Some(kind) = authority.declaration.resource_kind {
                value["resource_kind"] = serde_json::to_value(kind).expect("closed resource kind");
            }
            wire.push(value);
        }
        common["service_authorities"] = Value::Array(wire);
        if let Some(handle) = &target_commit {
            insert_handle(&mut handles, &Value::String(handle.clone()))?;
            common["target_commit"] = serde_json::json!({"handle":handle});
        }
        let state = State::new(session_id, operation, outputs, target_commit)?;
        let payload = encode_payload(&common).map_err(|_| Failure::new("invalid_frame"))?;
        Ok(Self {
            payload,
            operation,
            state,
        })
    }
}

fn validate_declaration_shape(declaration: &ExpectedAuthority) -> Result<(), Failure> {
    if matches!(
        (
            &declaration.access.reference.scope,
            declaration.resource_kind
        ),
        (ServiceScope::Storage(_), None) | (ServiceScope::Resource(_), Some(_))
    ) {
        Ok(())
    } else {
        Err(Failure::new("invalid_authority"))
    }
}
fn machine_value(value: &Value) -> Result<String, Failure> {
    value
        .as_str()
        .filter(|s| is_machine_id(s))
        .map(str::to_owned)
        .ok_or_else(|| Failure::new("invalid_authority"))
}
fn insert_handle(handles: &mut BTreeSet<String>, value: &Value) -> Result<String, Failure> {
    let handle = machine_value(value)?;
    if !handles.insert(handle.clone()) {
        return Err(Failure::new("invalid_authority"));
    }
    Ok(handle)
}
fn array_handles(
    handles: &mut BTreeSet<String>,
    value: &Value,
) -> Result<BTreeSet<String>, Failure> {
    value
        .as_array()
        .ok_or_else(|| Failure::new("invalid_frame"))?
        .iter()
        .map(|item| insert_handle(handles, &item["handle"]))
        .collect()
}

pub(in crate::hook) struct Connected {
    pub(in crate::hook) writer: ProtocolWriter,
    pub(in crate::hook) receiver: Receiver<Event>,
}
impl Connected {
    pub(in crate::hook) fn start(
        stream: ProtocolStream,
        prepared: PreparedSession,
    ) -> io::Result<(Self, State)> {
        let (transport, state) = prepared.into_parts();
        Ok((Self::start_transport(stream, transport)?, state))
    }
    pub(in crate::hook) fn start_transport(
        stream: ProtocolStream,
        transport: PreparedTransport,
    ) -> io::Result<Self> {
        let PreparedTransport { payload, operation } = transport;
        let mut reader = stream.try_clone()?;
        let mut writer = ProtocolWriter { stream };
        writer.stream.write_all(PREAMBLE_V2)?;
        writer.write_payload(&payload)?;
        let (sender, receiver) = mpsc::sync_channel(2);
        thread::spawn(move || {
            read_events(&mut reader, operation, |event| sender.send(event).is_ok())
        });
        Ok(Self { writer, receiver })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Failure {
    pub(in crate::hook) code: &'static str,
}
impl Failure {
    pub(in crate::hook) fn error_ref(&self) -> crate::domain::PactrunErrorRefV1 {
        crate::domain::PactrunErrorRefV1::new("hook_protocol_v2", self.code)
            .expect("registered V2 failure identity")
    }
    fn new(code: &'static str) -> Self {
        Self { code }
    }
    fn common(error: ProtocolFailure) -> Self {
        Self::new(match error.code {
            "invalid_authority" | "duplicate_semantic_key" => "invalid_authority",
            "unexpected_message"
            | "unsupported_protocol_version"
            | "request_in_flight"
            | "invalid_recovery_transition"
            | "completion_with_open_risk" => "unexpected_message",
            _ => "invalid_frame",
        })
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "hook_protocol_v2.{}", self.code)
    }
}
impl std::error::Error for Failure {}

#[derive(Debug)]
pub(in crate::hook) struct TargetProposal {
    commit_handle: String,
    outputs: Vec<String>,
}

/// Constructed only by the owner-selected messaging state after acknowledged
/// Open risk and exact target authority checks. It carries no persistence proof.
#[derive(Debug)]
pub(in crate::hook) struct AcceptedTargetProposal(TargetProposal);
impl AcceptedTargetProposal {
    pub(in crate::hook) fn receipt(&self) -> Value {
        serde_json::json!({"type":"target_ready_received", "commit_handle": self.0.commit_handle})
    }
    pub(in crate::hook) fn outputs(&self) -> &[String] {
        &self.0.outputs
    }
}

#[derive(Debug)]
pub(in crate::hook) enum Message {
    Common(HookMessage),
    TargetReady(TargetProposal),
}

pub(in crate::hook) fn parse_message(
    payload: &[u8],
    operation: SessionOperation,
) -> Result<Message, Failure> {
    let raw = parse_json(payload, MAX_PAYLOAD).map_err(|_| Failure::new("invalid_frame"))?;
    let mut object = raw_object(raw).map_err(Failure::common)?;
    let kind = take_string(&mut object, "type").map_err(Failure::common)?;
    if kind == "target_ready" {
        closed(&object, &["commit_handle", "produced_target_outputs"]).map_err(Failure::common)?;
        let commit_handle = take_machine_id(&mut object, "commit_handle")
            .map_err(|_| Failure::new("invalid_authority"))?;
        let values = take_array(&mut object, "produced_target_outputs").map_err(Failure::common)?;
        let mut outputs = Vec::with_capacity(values.len());
        for value in values {
            let RawJsonValue::String(handle) = value else {
                return Err(Failure::new("invalid_authority"));
            };
            if !is_machine_id(&handle) {
                return Err(Failure::new("invalid_authority"));
            }
            if outputs
                .last()
                .is_some_and(|previous: &String| previous >= &handle)
            {
                return Err(Failure::new("invalid_target_proposal"));
            }
            outputs.push(handle);
        }
        return Ok(Message::TargetReady(TargetProposal {
            commit_handle,
            outputs,
        }));
    }
    let message = match kind.as_str() {
        "session_ready" => parse_session_ready(object),
        "diagnostic" => parse_diagnostic(object),
        "request" => parse_request(object),
        "cancel_ack" => parse_cancel_ack(object),
        "complete" => parse_completion(object, operation),
        "protocol_error" => parse_hook_protocol_error(object),
        _ => return Err(Failure::new("unexpected_message")),
    }
    .map_err(Failure::common)?;
    Ok(Message::Common(message))
}

#[derive(Debug)]
pub(in crate::hook) enum Step {
    Common(ProtocolStep),
    TargetProposed(AcceptedTargetProposal),
}

#[derive(Debug)]
pub(in crate::hook) struct State {
    common: ProtocolState,
    target_commit: Option<String>,
}
impl State {
    pub(in crate::hook) fn new(
        session_id: String,
        operation: SessionOperation,
        outputs: BTreeSet<String>,
        target_commit: Option<String>,
    ) -> Result<Self, Failure> {
        if !is_machine_id(&session_id)
            || outputs.iter().any(|id| !is_machine_id(id))
            || target_commit
                .as_ref()
                .is_some_and(|id| !is_machine_id(id) || outputs.contains(id))
            || (target_commit.is_some() && operation != SessionOperation::Migration)
        {
            return Err(Failure::new("invalid_authority"));
        }
        let mut common = ProtocolState::new(session_id, outputs);
        common.operation = operation;
        Ok(Self {
            common,
            target_commit,
        })
    }

    pub(in crate::hook) fn accept(&mut self, message: Message) -> Result<Step, Failure> {
        if self.common.phase == Phase::Terminal {
            return Err(Failure::new("unexpected_message"));
        }
        match message {
            Message::Common(HookMessage::SessionReady {
                protocol_version,
                session_id,
            }) => {
                // No translation of a V1 handshake into V2 and no peer-selected
                // fallback: this constructor's owner selected version 2.
                if self.common.phase != Phase::AwaitReady
                    || protocol_version != 2
                    || session_id != self.common.session_id
                {
                    return Err(Failure::new("unexpected_message"));
                }
                self.common.phase = Phase::Active;
                Ok(Step::Common(ProtocolStep::Ready))
            }
            Message::TargetReady(proposal) => {
                let Some(handle) = &self.target_commit else {
                    return Err(Failure::new("unexpected_message"));
                };
                if self.common.phase != Phase::Active
                    || self.common.pending_request.is_some()
                    || self.common.risk != RecoveryRiskState::Open
                {
                    return Err(Failure::new("invalid_target_proposal"));
                }
                if &proposal.commit_handle != handle {
                    return Err(Failure::new("invalid_authority"));
                }
                if proposal.outputs.iter().collect::<BTreeSet<_>>()
                    != self.common.declared_outputs.iter().collect::<BTreeSet<_>>()
                {
                    return Err(Failure::new("invalid_target_proposal"));
                }
                self.common.phase = Phase::Terminal; // Messaging only; risk stays Open.
                Ok(Step::TargetProposed(AcceptedTargetProposal(proposal)))
            }
            Message::Common(HookMessage::Request {
                requested: RecoveryRiskState::Clear,
                ..
            }) if self.target_commit.is_some() => Err(Failure::new("unexpected_message")),
            Message::Common(HookMessage::Complete(ref completion))
                if self.target_commit.is_some()
                    && completion.status == HookCompletionStatus::Success =>
            {
                Err(Failure::new("unexpected_message"))
            }
            Message::Common(message) => self
                .common
                .accept(message)
                .map(Step::Common)
                .map_err(Failure::common),
        }
    }
    pub(in crate::hook) fn acknowledge_request(&mut self, id: u64, risk: RecoveryRiskState) {
        self.common.acknowledge_request(id, risk);
    }
    pub(in crate::hook) fn risk(&self) -> RecoveryRiskState {
        self.common.risk()
    }
    pub(in crate::hook) fn begin_cancel(&mut self) -> Option<u64> {
        self.common.begin_cancel()
    }
    pub(in crate::hook) fn is_ready(&self) -> bool {
        self.common.is_ready()
    }
    pub(in crate::hook) fn messaging_terminal(&self) -> bool {
        self.common.is_terminal()
    }
    pub(in crate::hook) fn end_of_stream(&self) -> Option<Failure> {
        self.common.end_of_stream().map(Failure::common)
    }
}

#[derive(Debug)]
pub(in crate::hook) enum Event {
    Message(Message),
    Failure(Failure),
    EndOfStream,
    TransportFailure,
}

pub(in crate::hook) fn read_events(
    reader: &mut impl Read,
    operation: SessionOperation,
    mut deliver: impl FnMut(Event) -> bool,
) {
    let mut preamble = [0; PREAMBLE_V2.len()];
    match read_fully(reader, &mut preamble) {
        Ok(ReadOutcome::Complete) if preamble == PREAMBLE_V2 => (),
        Err(_) => {
            deliver(Event::TransportFailure);
            return;
        }
        _ => {
            deliver(Event::Failure(Failure::new("invalid_frame")));
            return;
        }
    }
    loop {
        let mut length = [0; 4];
        match read_fully(reader, &mut length) {
            Ok(ReadOutcome::Complete) => (),
            Ok(ReadOutcome::EndOfStream) => {
                deliver(Event::EndOfStream);
                return;
            }
            Ok(ReadOutcome::Truncated) => {
                deliver(Event::Failure(Failure::new("invalid_frame")));
                return;
            }
            Err(_) => {
                deliver(Event::TransportFailure);
                return;
            }
        }
        let length = u32::from_be_bytes(length) as usize;
        if length > MAX_PAYLOAD {
            deliver(Event::Failure(Failure::new("invalid_frame")));
            return;
        }
        let mut payload = vec![0; length];
        match read_fully(reader, &mut payload) {
            Ok(ReadOutcome::Complete) => (),
            Err(_) => {
                deliver(Event::TransportFailure);
                return;
            }
            _ => {
                deliver(Event::Failure(Failure::new("invalid_frame")));
                return;
            }
        }
        match parse_message(&payload, operation) {
            Ok(message) => {
                if !deliver(Event::Message(message)) {
                    return;
                }
            }
            Err(failure) => {
                deliver(Event::Failure(failure));
                return;
            }
        }
    }
}

#[cfg(test)]
#[path = "protocol_v2_tests.rs"]
mod tests;
