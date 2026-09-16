//! Owner-selected runtime dispatch. V1 messages/states remain unchanged; the
//! dedicated V2 decoder never routes through a negotiated or guessed version.
use super::{FailureKind, platform::ProtocolStream, protocol as v1};
use crate::domain::RecoveryRiskState;
use std::{collections::BTreeSet, fmt, io, sync::mpsc, time::Duration};
use v1::v2;

pub(super) enum Message {
    V1(v1::HookMessage),
    V2(v2::Message),
}
impl Message {
    pub(super) fn completion(&self) -> Option<&v1::HookCompletion> {
        match self {
            Self::V1(v1::HookMessage::Complete(c))
            | Self::V2(v2::Message::Common(v1::HookMessage::Complete(c))) => Some(c),
            _ => None,
        }
    }
}
pub(super) enum WireEvent {
    Message(Message),
    Failure(FailureKind),
    EndOfStream,
    TransportFailure,
}
fn event_v1(event: v1::WireEvent) -> WireEvent {
    match event {
        v1::WireEvent::Message(m) => WireEvent::Message(Message::V1(m)),
        v1::WireEvent::Failure(e) => WireEvent::Failure(FailureKind::Protocol(e)),
        v1::WireEvent::EndOfStream => WireEvent::EndOfStream,
        v1::WireEvent::TransportFailure => WireEvent::TransportFailure,
    }
}
fn event_v2(event: v2::Event) -> WireEvent {
    match event {
        v2::Event::Message(m) => WireEvent::Message(Message::V2(m)),
        v2::Event::Failure(e) => WireEvent::Failure(FailureKind::ProtocolV2(e)),
        v2::Event::EndOfStream => WireEvent::EndOfStream,
        v2::Event::TransportFailure => WireEvent::TransportFailure,
    }
}
pub(super) enum Receiver {
    V1(mpsc::Receiver<v1::WireEvent>),
    V2(mpsc::Receiver<v2::Event>),
}
impl Receiver {
    pub(super) fn try_recv(&self) -> Result<WireEvent, mpsc::TryRecvError> {
        match self {
            Self::V1(r) => r.try_recv().map(event_v1),
            Self::V2(r) => r.try_recv().map(event_v2),
        }
    }
    pub(super) fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<WireEvent, mpsc::RecvTimeoutError> {
        match self {
            Self::V1(r) => r.recv_timeout(timeout).map(event_v1),
            Self::V2(r) => r.recv_timeout(timeout).map(event_v2),
        }
    }
}
pub(super) struct ConnectedProtocol {
    pub(super) writer: v1::ProtocolWriter,
    pub(super) receiver: Receiver,
}
impl fmt::Debug for ConnectedProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ConnectedProtocol(<owner-private stream>)")
    }
}
impl ConnectedProtocol {
    fn v1(connected: v1::ConnectedProtocol) -> Self {
        Self {
            writer: connected.writer,
            receiver: Receiver::V1(connected.receiver),
        }
    }
    pub(super) fn start(stream: ProtocolStream, session: &serde_json::Value) -> io::Result<Self> {
        v1::ConnectedProtocol::start(stream, session).map(Self::v1)
    }
    pub(super) fn start_capture(
        stream: ProtocolStream,
        session: &serde_json::Value,
    ) -> io::Result<Self> {
        v1::ConnectedProtocol::start_capture(stream, session).map(Self::v1)
    }
    pub(super) fn start_restore(
        stream: ProtocolStream,
        session: &serde_json::Value,
    ) -> io::Result<Self> {
        v1::ConnectedProtocol::start_restore(stream, session).map(Self::v1)
    }
    pub(super) fn start_migration(
        stream: ProtocolStream,
        session: &serde_json::Value,
    ) -> io::Result<Self> {
        v1::ConnectedProtocol::start_migration(stream, session).map(Self::v1)
    }
    pub(super) fn start_v2(
        stream: ProtocolStream,
        transport: v2::PreparedTransport,
    ) -> io::Result<Self> {
        v2::Connected::start_transport(stream, transport).map(|c| Self {
            writer: c.writer,
            receiver: Receiver::V2(c.receiver),
        })
    }
    pub(super) fn start_cleanup(
        stream: ProtocolStream,
        session: &serde_json::Value,
    ) -> io::Result<Self> {
        v1::ConnectedProtocol::start_cleanup(stream, session).map(Self::v1)
    }
}
pub(super) enum ProtocolStep {
    Ready,
    Diagnostic,
    RiskRequest {
        request_id: u64,
        requested: RecoveryRiskState,
    },
    CancelAcknowledged,
    Completed(v1::HookCompletion),
    HookProtocolError,
    TargetProposed(v2::AcceptedTargetProposal),
}
fn step(common: v1::ProtocolStep) -> ProtocolStep {
    match common {
        v1::ProtocolStep::Ready => ProtocolStep::Ready,
        v1::ProtocolStep::Diagnostic => ProtocolStep::Diagnostic,
        v1::ProtocolStep::RiskRequest {
            request_id,
            requested,
        } => ProtocolStep::RiskRequest {
            request_id,
            requested,
        },
        v1::ProtocolStep::CancelAcknowledged => ProtocolStep::CancelAcknowledged,
        v1::ProtocolStep::Completed(c) => ProtocolStep::Completed(c),
        v1::ProtocolStep::HookProtocolError => ProtocolStep::HookProtocolError,
    }
}
#[derive(Debug)]
pub(super) enum ProtocolState {
    V1(v1::ProtocolState),
    V2(v2::State),
}
impl ProtocolState {
    pub(super) fn new(id: String, outputs: BTreeSet<String>) -> Self {
        Self::V1(v1::ProtocolState::new(id, outputs))
    }
    pub(super) fn new_capture(id: String) -> Self {
        Self::V1(v1::ProtocolState::new_capture(id))
    }
    pub(super) fn new_restore(id: String) -> Self {
        Self::V1(v1::ProtocolState::new_restore(id))
    }
    pub(super) fn new_cleanup(id: String) -> Self {
        Self::V1(v1::ProtocolState::new_cleanup(id))
    }
    pub(super) fn new_migration(id: String, outputs: BTreeSet<String>) -> Self {
        Self::V1(v1::ProtocolState::new_migration(id, outputs))
    }
    pub(super) fn accept(&mut self, message: Message) -> Result<ProtocolStep, FailureKind> {
        match (self, message) {
            (Self::V1(s), Message::V1(m)) => s.accept(m).map(step).map_err(FailureKind::Protocol),
            (Self::V2(s), Message::V2(m)) => s
                .accept(m)
                .map(|result| match result {
                    v2::Step::Common(c) => step(c),
                    v2::Step::TargetProposed(p) => ProtocolStep::TargetProposed(p),
                })
                .map_err(FailureKind::ProtocolV2),
            (Self::V1(_), _) => Err(FailureKind::Protocol(v1::ProtocolFailure::new(
                "unexpected_message",
                "protocol decoder differs from selected owner",
            ))),
            (Self::V2(_), _) => Err(FailureKind::ProtocolV2(v2::Failure {
                code: "unexpected_message",
            })),
        }
    }
    pub(super) fn is_ready(&self) -> bool {
        match self {
            Self::V1(s) => s.is_ready(),
            Self::V2(s) => s.is_ready(),
        }
    }
    pub(super) fn is_terminal(&self) -> bool {
        match self {
            Self::V1(s) => s.is_terminal(),
            Self::V2(s) => s.messaging_terminal(),
        }
    }
    pub(super) fn risk(&self) -> RecoveryRiskState {
        match self {
            Self::V1(s) => s.risk(),
            Self::V2(s) => s.risk(),
        }
    }
    pub(super) fn acknowledge_request(&mut self, id: u64, risk: RecoveryRiskState) {
        match self {
            Self::V1(s) => s.acknowledge_request(id, risk),
            Self::V2(s) => s.acknowledge_request(id, risk),
        }
    }
    pub(super) fn begin_cancel(&mut self) -> Option<u64> {
        match self {
            Self::V1(s) => s.begin_cancel(),
            Self::V2(s) => s.begin_cancel(),
        }
    }
    pub(super) fn end_of_stream(&self) -> Option<FailureKind> {
        match self {
            Self::V1(s) => s.end_of_stream().map(FailureKind::Protocol),
            Self::V2(s) => s.end_of_stream().map(FailureKind::ProtocolV2),
        }
    }
}
