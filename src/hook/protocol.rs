//! Frozen HookProtocolV1 framing, strict Action message decoding, and the
//! Pactrun-side Action Session state machine.
//!
//! The state machine is pure so the Frozen Action vectors can drive it
//! directly; the runtime applies its typed steps (durable risk writes,
//! acknowledgments, completion acceptance) around it.

use std::{
    collections::BTreeSet,
    fmt,
    io::{self, Read, Write},
    sync::mpsc::{self, Receiver},
    thread,
};

use serde_json::Value;

use crate::{
    domain::{HookCodeV1, HookCompletionStatus, RecoveryRiskState},
    strict_json::{RawJsonValue, StrictJsonErrorKind, parse_json},
};

use super::platform::ProtocolStream;

pub(super) const PREAMBLE: &[u8] = b"pactrun.hook-protocol\0\0\0\0\x01";
pub(super) const MAX_PAYLOAD: usize = 16 * 1024 * 1024;

/// A Pactrun-detected protocol failure using one closed PR-REQ-0217 code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProtocolFailure {
    pub(super) code: &'static str,
    pub(super) message: &'static str,
}

impl ProtocolFailure {
    pub(super) const fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }
}

pub(super) struct ConnectedProtocol {
    pub(super) writer: ProtocolWriter,
    pub(super) receiver: Receiver<WireEvent>,
}

impl fmt::Debug for ConnectedProtocol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ConnectedProtocol(<owner-private stream>)")
    }
}

impl ConnectedProtocol {
    pub(super) fn start(stream: ProtocolStream, session: &Value) -> io::Result<Self> {
        let reader = stream.try_clone()?;
        let mut writer = ProtocolWriter { stream };
        writer.write_preamble()?;
        writer.write_value(session)?;
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || read_hook_messages(reader, &sender));
        Ok(Self { writer, receiver })
    }
}

pub(super) struct ProtocolWriter {
    stream: ProtocolStream,
}

impl ProtocolWriter {
    fn write_preamble(&mut self) -> io::Result<()> {
        self.stream.write_all(PREAMBLE)?;
        self.stream.flush()
    }

    pub(super) fn write_value(&mut self, value: &Value) -> io::Result<()> {
        let payload = serde_json::to_vec(value)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let length = u32::try_from(payload.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "protocol frame too large"))?;
        self.stream.write_all(&length.to_be_bytes())?;
        self.stream.write_all(&payload)?;
        self.stream.flush()
    }
}

/// One observation from the Hook-to-Pactrun direction of the stream.
#[derive(Debug)]
pub(super) enum WireEvent {
    Message(HookMessage),
    /// A Pactrun-detected framing or message fault.
    Failure(ProtocolFailure),
    /// The Hook closed its direction at a frame boundary.
    EndOfStream,
    /// The transport itself failed with an OS error that is not a disconnect.
    TransportFailure,
}

fn read_hook_messages(mut stream: ProtocolStream, sender: &mpsc::Sender<WireEvent>) {
    read_wire_events(&mut stream, |event| sender.send(event).is_ok());
}

/// Decodes the Hook direction of a Frozen V1 stream until the first fault,
/// end of stream, or a refused delivery. `deliver` returns whether reading
/// should continue.
pub(super) fn read_wire_events(reader: &mut impl Read, mut deliver: impl FnMut(WireEvent) -> bool) {
    let mut preamble = [0_u8; PREAMBLE.len()];
    match read_fully(reader, &mut preamble) {
        Ok(ReadOutcome::Complete) => {}
        Ok(ReadOutcome::EndOfStream | ReadOutcome::Truncated) => {
            deliver(WireEvent::Failure(invalid_preamble()));
            return;
        }
        Err(_) => {
            deliver(WireEvent::TransportFailure);
            return;
        }
    }
    if preamble != PREAMBLE {
        deliver(WireEvent::Failure(invalid_preamble()));
        return;
    }
    loop {
        let mut length = [0_u8; 4];
        match read_fully(reader, &mut length) {
            Ok(ReadOutcome::Complete) => {}
            Ok(ReadOutcome::EndOfStream) => {
                deliver(WireEvent::EndOfStream);
                return;
            }
            Ok(ReadOutcome::Truncated) => {
                deliver(WireEvent::Failure(truncated_frame()));
                return;
            }
            Err(_) => {
                deliver(WireEvent::TransportFailure);
                return;
            }
        }
        let length = u32::from_be_bytes(length) as usize;
        if length > MAX_PAYLOAD {
            deliver(WireEvent::Failure(ProtocolFailure::new(
                "frame_too_large",
                "Hook Protocol frame exceeds 16 MiB",
            )));
            return;
        }
        let mut payload = vec![0_u8; length];
        match read_fully(reader, &mut payload) {
            Ok(ReadOutcome::Complete) => {}
            Ok(ReadOutcome::EndOfStream | ReadOutcome::Truncated) => {
                deliver(WireEvent::Failure(truncated_frame()));
                return;
            }
            Err(_) => {
                deliver(WireEvent::TransportFailure);
                return;
            }
        }
        match parse_hook_message(&payload) {
            Ok(message) => {
                if !deliver(WireEvent::Message(message)) {
                    return;
                }
            }
            Err(failure) => {
                deliver(WireEvent::Failure(failure));
                return;
            }
        }
    }
}

enum ReadOutcome {
    Complete,
    EndOfStream,
    Truncated,
}

fn read_fully(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<ReadOutcome> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => {
                return Ok(if filled == 0 {
                    ReadOutcome::EndOfStream
                } else {
                    ReadOutcome::Truncated
                });
            }
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if is_disconnect(&error) => {
                return Ok(if filled == 0 {
                    ReadOutcome::EndOfStream
                } else {
                    ReadOutcome::Truncated
                });
            }
            Err(error) => return Err(error),
        }
    }
    Ok(ReadOutcome::Complete)
}

/// Peer closure surfaces as an error rather than a zero-length read on some
/// transports (Windows named pipes report `ERROR_BROKEN_PIPE`).
fn is_disconnect(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::NotConnected
            | io::ErrorKind::UnexpectedEof
    )
}

#[derive(Debug)]
pub(super) enum HookMessage {
    SessionReady {
        protocol_version: u64,
        session_id: String,
    },
    Diagnostic,
    Request {
        request_id: u64,
        requested: RecoveryRiskState,
    },
    CancelAck {
        control_id: u64,
    },
    Complete(HookCompletion),
    ProtocolError,
}

#[derive(Debug)]
pub(super) struct HookCompletion {
    pub(super) status: HookCompletionStatus,
    pub(super) code: Option<HookCodeV1>,
    pub(super) message: Option<String>,
    pub(super) produced_outputs: Vec<String>,
}

pub(super) fn parse_hook_message(payload: &[u8]) -> Result<HookMessage, ProtocolFailure> {
    let raw = parse_json(payload, MAX_PAYLOAD).map_err(|error| {
        let code = match error.kind() {
            StrictJsonErrorKind::InvalidUtf8 => "invalid_utf8",
            StrictJsonErrorKind::InvalidJson => "invalid_json",
            StrictJsonErrorKind::DuplicateProperty => "duplicate_property",
            StrictJsonErrorKind::InvalidUnicodeScalar => "invalid_unicode_scalar",
        };
        ProtocolFailure::new(code, "Hook Protocol JSON is invalid")
    })?;
    let mut object = raw_object(raw)?;
    let message_type = take_string(&mut object, "type")?;
    match message_type.as_str() {
        "session_ready" => parse_session_ready(object),
        "diagnostic" => parse_diagnostic(object),
        "request" => parse_request(object),
        "cancel_ack" => parse_cancel_ack(object),
        "complete" => parse_completion(object),
        "protocol_error" => parse_hook_protocol_error(object),
        _ => Err(ProtocolFailure::new(
            "unexpected_message",
            "message type is invalid in the Hook direction",
        )),
    }
}

fn parse_session_ready(mut object: Fields) -> Result<HookMessage, ProtocolFailure> {
    closed(&object, &["protocol_version", "session_id"])?;
    Ok(HookMessage::SessionReady {
        protocol_version: take_safe_integer(&mut object, "protocol_version")?,
        session_id: take_machine_id(&mut object, "session_id")?,
    })
}

fn parse_diagnostic(mut object: Fields) -> Result<HookMessage, ProtocolFailure> {
    closed(&object, &["severity", "code", "message"])?;
    let severity = take_string(&mut object, "severity")?;
    if !matches!(severity.as_str(), "info" | "warning" | "error") {
        return Err(invalid_message());
    }
    take_hook_code(&mut object, "code")?;
    let _ = take_string(&mut object, "message")?;
    Ok(HookMessage::Diagnostic)
}

fn parse_request(mut object: Fields) -> Result<HookMessage, ProtocolFailure> {
    closed(&object, &["request_id", "request"])?;
    let request_id = take_positive_integer(&mut object, "request_id")?;
    let mut request = raw_object(take(&mut object, "request")?)?;
    closed(&request, &["kind"])?;
    let requested = match take_string(&mut request, "kind")?.as_str() {
        "enter_recovery_risk" => RecoveryRiskState::Open,
        "resolve_recovery_risk" => RecoveryRiskState::Clear,
        _ => return Err(invalid_message()),
    };
    Ok(HookMessage::Request {
        request_id,
        requested,
    })
}

fn parse_cancel_ack(mut object: Fields) -> Result<HookMessage, ProtocolFailure> {
    closed(&object, &["control_id"])?;
    Ok(HookMessage::CancelAck {
        control_id: take_positive_integer(&mut object, "control_id")?,
    })
}

fn parse_completion(mut object: Fields) -> Result<HookMessage, ProtocolFailure> {
    closed(
        &object,
        &["operation", "status", "produced_outputs", "code", "message"],
    )?;
    if take_string(&mut object, "operation")? != "action" {
        return Err(ProtocolFailure::new(
            "invalid_completion",
            "completion operation differs from the Action Session",
        ));
    }
    let status = match take_string(&mut object, "status")?.as_str() {
        "success" => HookCompletionStatus::Success,
        "failure" => HookCompletionStatus::Failure,
        _ => return Err(invalid_message()),
    };
    let code = match remove_field(&mut object, "code") {
        None => None,
        Some(RawJsonValue::String(value)) => {
            Some(HookCodeV1::parse(value).map_err(|_| invalid_message())?)
        }
        Some(_) => return Err(invalid_message()),
    };
    let message = match remove_field(&mut object, "message") {
        None => None,
        Some(RawJsonValue::String(value)) => Some(value),
        Some(_) => return Err(invalid_message()),
    };
    let produced_outputs = take_array(&mut object, "produced_outputs")?
        .into_iter()
        .map(|value| match value {
            RawJsonValue::String(value) if is_machine_id(&value) => Ok(value),
            _ => Err(ProtocolFailure::new(
                "invalid_authority",
                "output authority handle is not an AuthorityHandleV1",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(HookMessage::Complete(HookCompletion {
        status,
        code,
        message,
        produced_outputs,
    }))
}

fn parse_hook_protocol_error(mut object: Fields) -> Result<HookMessage, ProtocolFailure> {
    closed(&object, &["code", "message"])?;
    take_hook_code(&mut object, "code")?;
    let _ = take_string(&mut object, "message")?;
    Ok(HookMessage::ProtocolError)
}

type Fields = Vec<(String, RawJsonValue)>;

fn raw_object(value: RawJsonValue) -> Result<Fields, ProtocolFailure> {
    match value {
        RawJsonValue::Object(entries) => Ok(entries),
        _ => Err(invalid_message()),
    }
}

fn closed(object: &Fields, allowed: &[&str]) -> Result<(), ProtocolFailure> {
    if object
        .iter()
        .any(|(key, _)| !allowed.contains(&key.as_str()))
    {
        return Err(ProtocolFailure::new(
            "unknown_field",
            "Hook Protocol message has an unknown field",
        ));
    }
    Ok(())
}

fn remove_field(object: &mut Fields, key: &str) -> Option<RawJsonValue> {
    let index = object.iter().position(|(candidate, _)| candidate == key)?;
    Some(object.swap_remove(index).1)
}

fn take(object: &mut Fields, key: &str) -> Result<RawJsonValue, ProtocolFailure> {
    remove_field(object, key).ok_or_else(invalid_message)
}

fn take_string(object: &mut Fields, key: &str) -> Result<String, ProtocolFailure> {
    match take(object, key)? {
        RawJsonValue::String(value) => Ok(value),
        _ => Err(invalid_message()),
    }
}

fn take_array(object: &mut Fields, key: &str) -> Result<Vec<RawJsonValue>, ProtocolFailure> {
    match take(object, key)? {
        RawJsonValue::Array(value) => Ok(value),
        _ => Err(invalid_message()),
    }
}

fn take_hook_code(object: &mut Fields, key: &str) -> Result<HookCodeV1, ProtocolFailure> {
    HookCodeV1::parse(take_string(object, key)?).map_err(|_| invalid_message())
}

fn take_safe_integer(object: &mut Fields, key: &str) -> Result<u64, ProtocolFailure> {
    let RawJsonValue::Number(value) = take(object, key)? else {
        return Err(invalid_message());
    };
    let value = value.parse::<u64>().map_err(|_| invalid_message())?;
    if value > 9_007_199_254_740_991 {
        return Err(invalid_message());
    }
    Ok(value)
}

fn take_positive_integer(object: &mut Fields, key: &str) -> Result<u64, ProtocolFailure> {
    let value = take_safe_integer(object, key)?;
    if value == 0 {
        return Err(invalid_message());
    }
    Ok(value)
}

fn take_machine_id(object: &mut Fields, key: &str) -> Result<String, ProtocolFailure> {
    let value = take_string(object, key)?;
    if !is_machine_id(&value) {
        return Err(invalid_message());
    }
    Ok(value)
}

fn is_machine_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

const fn invalid_message() -> ProtocolFailure {
    ProtocolFailure::new("invalid_message", "Hook Protocol message is invalid")
}

const fn invalid_preamble() -> ProtocolFailure {
    ProtocolFailure::new("invalid_preamble", "Hook Protocol preamble is invalid")
}

const fn truncated_frame() -> ProtocolFailure {
    ProtocolFailure::new("invalid_message", "Hook Protocol frame is truncated")
}

const fn unexpected_message() -> ProtocolFailure {
    ProtocolFailure::new(
        "unexpected_message",
        "message is invalid in the current Session state",
    )
}

/// The Pactrun-side Frozen V1 Action Session state machine after
/// `session_start` has been sent.
#[derive(Debug)]
pub(super) struct ProtocolState {
    session_id: String,
    declared_outputs: BTreeSet<String>,
    phase: Phase,
    risk: RecoveryRiskState,
    pending_request: Option<u64>,
    used_request_ids: BTreeSet<u64>,
    next_control_id: u64,
    pending_controls: BTreeSet<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    AwaitReady,
    Active,
    Terminal,
}

/// A typed transition the runtime applies after a valid Hook message.
#[derive(Debug)]
pub(super) enum ProtocolStep {
    Ready,
    Diagnostic,
    RiskRequest {
        request_id: u64,
        requested: RecoveryRiskState,
    },
    CancelAcknowledged,
    Completed(HookCompletion),
    HookProtocolError,
}

impl ProtocolState {
    pub(super) fn new(session_id: String, declared_outputs: BTreeSet<String>) -> Self {
        Self {
            session_id,
            declared_outputs,
            phase: Phase::AwaitReady,
            risk: RecoveryRiskState::Clear,
            pending_request: None,
            used_request_ids: BTreeSet::new(),
            next_control_id: 1,
            pending_controls: BTreeSet::new(),
        }
    }

    pub(super) fn is_ready(&self) -> bool {
        self.phase == Phase::Active
    }

    pub(super) fn is_terminal(&self) -> bool {
        self.phase == Phase::Terminal
    }

    pub(super) fn risk(&self) -> RecoveryRiskState {
        self.risk
    }

    pub(super) fn accept(&mut self, message: HookMessage) -> Result<ProtocolStep, ProtocolFailure> {
        match message {
            HookMessage::SessionReady {
                protocol_version,
                session_id,
            } => {
                if self.phase != Phase::AwaitReady {
                    return Err(unexpected_message());
                }
                if protocol_version != 1 {
                    return Err(ProtocolFailure::new(
                        "unsupported_protocol_version",
                        "Hook must confirm exact protocol version 1",
                    ));
                }
                if session_id != self.session_id {
                    return Err(ProtocolFailure::new(
                        "invalid_message",
                        "session_ready identifies another Session",
                    ));
                }
                self.phase = Phase::Active;
                Ok(ProtocolStep::Ready)
            }
            HookMessage::Diagnostic => {
                self.require_active()?;
                Ok(ProtocolStep::Diagnostic)
            }
            HookMessage::Request {
                request_id,
                requested,
            } => {
                self.require_active()?;
                if self.pending_request.is_some() {
                    return Err(ProtocolFailure::new(
                        "request_in_flight",
                        "only one Hook-originated request may be outstanding",
                    ));
                }
                if !self.used_request_ids.insert(request_id) {
                    return Err(ProtocolFailure::new(
                        "invalid_message",
                        "request_id was reused",
                    ));
                }
                if self.risk == requested {
                    return Err(ProtocolFailure::new(
                        "invalid_recovery_transition",
                        "recovery-risk request is invalid from the current state",
                    ));
                }
                self.pending_request = Some(request_id);
                Ok(ProtocolStep::RiskRequest {
                    request_id,
                    requested,
                })
            }
            HookMessage::CancelAck { control_id } => {
                self.require_active()?;
                if !self.pending_controls.remove(&control_id) {
                    return Err(ProtocolFailure::new(
                        "unexpected_message",
                        "cancel_ack has no matching cancellation control",
                    ));
                }
                Ok(ProtocolStep::CancelAcknowledged)
            }
            HookMessage::Complete(completion) => {
                self.require_active()?;
                if self.pending_request.is_some() {
                    return Err(ProtocolFailure::new(
                        "request_in_flight",
                        "completion is invalid while a Hook request is outstanding",
                    ));
                }
                if completion.status == HookCompletionStatus::Success
                    && self.risk == RecoveryRiskState::Open
                {
                    return Err(ProtocolFailure::new(
                        "completion_with_open_risk",
                        "success is invalid while recovery risk remains open",
                    ));
                }
                let mut submitted = BTreeSet::new();
                for handle in &completion.produced_outputs {
                    if !submitted.insert(handle.as_str()) {
                        return Err(ProtocolFailure::new(
                            "duplicate_semantic_key",
                            "duplicate submitted authority handle",
                        ));
                    }
                    if !self.declared_outputs.contains(handle) {
                        return Err(ProtocolFailure::new(
                            "invalid_authority",
                            "Action submitted an unallocated output handle",
                        ));
                    }
                }
                self.phase = Phase::Terminal;
                Ok(ProtocolStep::Completed(completion))
            }
            HookMessage::ProtocolError => {
                self.require_active()?;
                self.phase = Phase::Terminal;
                Ok(ProtocolStep::HookProtocolError)
            }
        }
    }

    /// Records that the outstanding request was durably applied and
    /// acknowledged with the requested risk state.
    pub(super) fn acknowledge_request(&mut self, request_id: u64, risk: RecoveryRiskState) {
        debug_assert_eq!(self.pending_request, Some(request_id));
        self.pending_request = None;
        self.risk = risk;
    }

    /// Allocates the next cancellation control when the Session can carry
    /// one; cancellation before readiness or after the terminal boundary is
    /// owned by process supervision instead.
    pub(super) fn begin_cancel(&mut self) -> Option<u64> {
        if self.phase != Phase::Active {
            return None;
        }
        let control_id = self.next_control_id;
        self.next_control_id += 1;
        self.pending_controls.insert(control_id);
        Some(control_id)
    }

    /// End of the Hook direction before accepted completion is the Frozen
    /// `unexpected_message` fault; after the terminal boundary it is ordinary.
    pub(super) fn end_of_stream(&self) -> Option<ProtocolFailure> {
        (self.phase != Phase::Terminal).then(|| {
            ProtocolFailure::new(
                "unexpected_message",
                "stream ended before accepted completion",
            )
        })
    }

    fn require_active(&self) -> Result<(), ProtocolFailure> {
        if self.phase == Phase::Active {
            Ok(())
        } else {
            Err(unexpected_message())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_message_parser_is_strict_and_does_not_retain_diagnostics() {
        assert!(matches!(
            parse_hook_message(
                br#"{"type":"diagnostic","severity":"info","code":"ready","message":"sensitive text"}"#
            ),
            Ok(HookMessage::Diagnostic)
        ));
        assert_eq!(
            parse_hook_message(br#"{"type":"session_ready","type":"session_ready"}"#)
                .unwrap_err()
                .code,
            "duplicate_property"
        );
        assert_eq!(
            parse_hook_message(br#"{"type":"unknown"}"#)
                .unwrap_err()
                .code,
            "unexpected_message"
        );
        assert_eq!(
            parse_hook_message(br#"{"type":"cancel","control_id":1,"reason":"requested"}"#)
                .unwrap_err()
                .code,
            "unexpected_message"
        );
    }

    #[test]
    fn framing_reader_classifies_preamble_boundary_and_truncation_faults() {
        let mut events = Vec::new();
        read_wire_events(
            &mut io::Cursor::new(b"pactrun.hook-protocol\0\0\0\0\x02"),
            |event| {
                events.push(event);
                true
            },
        );
        assert!(
            matches!(&events[..], [WireEvent::Failure(failure)] if failure.code == "invalid_preamble")
        );

        let mut events = Vec::new();
        read_wire_events(&mut io::Cursor::new(PREAMBLE.to_vec()), |event| {
            events.push(event);
            true
        });
        assert!(matches!(&events[..], [WireEvent::EndOfStream]));

        let mut truncated = PREAMBLE.to_vec();
        truncated.extend_from_slice(&[0, 0, 0, 9, b'{']);
        let mut events = Vec::new();
        read_wire_events(&mut io::Cursor::new(truncated), |event| {
            events.push(event);
            true
        });
        assert!(
            matches!(&events[..], [WireEvent::Failure(failure)] if failure.code == "invalid_message")
        );
    }
}
