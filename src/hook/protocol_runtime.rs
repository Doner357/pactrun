//! Runtime adapter for the single owner-selected Hook baseline.
use super::{FailureKind, platform::ProtocolStream, protocol as common};
use crate::domain::RecoveryRiskState;
use authority as v2;
use common as v1;
use common::authority;
use std::{collections::BTreeSet, fmt, io, sync::mpsc, time::Duration};

pub(super) type Message = authority::Message;
impl Message {
    pub(super) fn completion(&self) -> Option<&common::HookCompletion> {
        match self {
            Self::Common(common::HookMessage::Complete(value)) => Some(value),
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
fn event(value: authority::Event) -> WireEvent {
    match value {
        authority::Event::Message(m) => WireEvent::Message(m),
        authority::Event::Failure(e) => WireEvent::Failure(FailureKind::ProtocolV2(e)),
        authority::Event::EndOfStream => WireEvent::EndOfStream,
        authority::Event::TransportFailure => WireEvent::TransportFailure,
    }
}
pub(super) struct Receiver(mpsc::Receiver<authority::Event>);
impl Receiver {
    pub(super) fn try_recv(&self) -> Result<WireEvent, mpsc::TryRecvError> {
        self.0.try_recv().map(event)
    }
    pub(super) fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<WireEvent, mpsc::RecvTimeoutError> {
        self.0.recv_timeout(timeout).map(event)
    }
}
pub(super) struct ConnectedProtocol {
    pub(super) writer: common::ProtocolWriter,
    pub(super) receiver: Receiver,
}
impl fmt::Debug for ConnectedProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ConnectedProtocol(<owner-private stream>)")
    }
}
impl ConnectedProtocol {
    fn start_for(
        stream: ProtocolStream,
        session: &serde_json::Value,
        operation: common::SessionOperation,
    ) -> io::Result<Self> {
        let prepared =
            authority::PreparedSession::new(session.clone(), operation, &[], vec![], false, None)
                .map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid owner-selected Session")
            })?;
        Self::start_prepared(stream, prepared.into_parts().0)
    }
    pub(super) fn start(stream: ProtocolStream, session: &serde_json::Value) -> io::Result<Self> {
        Self::start_for(stream, session, common::SessionOperation::Action)
    }
    pub(super) fn start_capture(
        stream: ProtocolStream,
        session: &serde_json::Value,
    ) -> io::Result<Self> {
        Self::start_for(stream, session, common::SessionOperation::Capture)
    }
    pub(super) fn start_restore(
        stream: ProtocolStream,
        session: &serde_json::Value,
    ) -> io::Result<Self> {
        Self::start_for(stream, session, common::SessionOperation::Restore)
    }
    pub(super) fn start_migration(
        stream: ProtocolStream,
        session: &serde_json::Value,
    ) -> io::Result<Self> {
        Self::start_for(stream, session, common::SessionOperation::Migration)
    }
    pub(super) fn start_cleanup(
        stream: ProtocolStream,
        session: &serde_json::Value,
    ) -> io::Result<Self> {
        Self::start_for(stream, session, common::SessionOperation::Cleanup)
    }
    pub(super) fn start_prepared(
        stream: ProtocolStream,
        transport: authority::PreparedTransport,
    ) -> io::Result<Self> {
        authority::Connected::start_transport(stream, transport).map(|c| Self {
            writer: c.writer,
            receiver: Receiver(c.receiver),
        })
    }
}
pub(super) enum ProtocolStep {
    Ready,
    Diagnostic(crate::domain::HookText),
    RiskRequest {
        request_id: u64,
        requested: RecoveryRiskState,
    },
    CancelAcknowledged,
    Completed(v1::HookCompletion),
    HookProtocolError(crate::domain::HookText),
    TargetProposed(v2::AcceptedTargetProposal),
}
fn step(common: v1::ProtocolStep) -> ProtocolStep {
    match common {
        v1::ProtocolStep::Ready => ProtocolStep::Ready,
        v1::ProtocolStep::Diagnostic(text) => ProtocolStep::Diagnostic(text),
        v1::ProtocolStep::RiskRequest {
            request_id,
            requested,
        } => ProtocolStep::RiskRequest {
            request_id,
            requested,
        },
        v1::ProtocolStep::CancelAcknowledged => ProtocolStep::CancelAcknowledged,
        v1::ProtocolStep::Completed(c) => ProtocolStep::Completed(c),
        v1::ProtocolStep::HookProtocolError(text) => ProtocolStep::HookProtocolError(text),
    }
}

#[derive(Debug)]
pub(super) struct ProtocolState(pub(super) authority::State);
impl ProtocolState {
    fn for_operation(
        id: String,
        outputs: BTreeSet<String>,
        operation: common::SessionOperation,
    ) -> Self {
        Self(
            authority::State::new(id, operation, outputs, None)
                .expect("typed owner Session identifiers"),
        )
    }
    pub(super) fn new(id: String, outputs: BTreeSet<String>) -> Self {
        Self::for_operation(id, outputs, common::SessionOperation::Action)
    }
    pub(super) fn new_migration(id: String, outputs: BTreeSet<String>) -> Self {
        Self::for_operation(id, outputs, common::SessionOperation::Migration)
    }
    pub(super) fn new_capture(id: String) -> Self {
        Self::for_operation(id, BTreeSet::new(), common::SessionOperation::Capture)
    }
    pub(super) fn new_restore(id: String) -> Self {
        Self::for_operation(id, BTreeSet::new(), common::SessionOperation::Restore)
    }
    pub(super) fn new_cleanup(id: String) -> Self {
        Self::for_operation(id, BTreeSet::new(), common::SessionOperation::Cleanup)
    }
    pub(super) fn accept(&mut self, message: Message) -> Result<ProtocolStep, FailureKind> {
        self.0
            .accept(message)
            .map(|result| match result {
                authority::Step::Common(value) => step(value),
                authority::Step::TargetProposed(p) => ProtocolStep::TargetProposed(p),
            })
            .map_err(FailureKind::ProtocolV2)
    }
    pub(super) fn is_ready(&self) -> bool {
        self.0.is_ready()
    }
    pub(super) fn is_terminal(&self) -> bool {
        self.0.messaging_terminal()
    }
    pub(super) fn risk(&self) -> RecoveryRiskState {
        self.0.risk()
    }
    pub(super) fn acknowledge_request(&mut self, id: u64, risk: RecoveryRiskState) {
        self.0.acknowledge_request(id, risk);
    }
    pub(super) fn begin_cancel(&mut self) -> Option<u64> {
        self.0.begin_cancel()
    }
    pub(super) fn end_of_stream(&self) -> Option<FailureKind> {
        self.0.end_of_stream().map(FailureKind::ProtocolV2)
    }
}
