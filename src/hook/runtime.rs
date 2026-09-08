//! Synchronous one-shot Action Hook supervision.
//!
//! One outcome arbiter locks the first winning event (accepted completion,
//! cancellation, startup or action deadline, or premature Hook loss). Every
//! post-Admission return leaves an owner continuation: either complete
//! terminal facts after the supervised process tree has been observed
//! terminated, or a retained live execution whose pending process-control or
//! durable operation the live owner retries.

use std::{
    fmt,
    process::ExitStatus,
    sync::mpsc::{RecvTimeoutError, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use serde_json::json;

use crate::{
    domain::{HookCompletionRecord, HookCompletionStatus, RecoveryRiskState, RunId},
    executor::AdmittedExecution,
    managed_data::StagingSession,
    persistence::PactrunPersistence,
};

use super::{
    ActionCancellation, DurableOperationRetry, FailureKind, FinalizationState, HookRuntimePolicy,
    OutcomeWinner, OwnerContinuation, PendingDurableOperation, ProcessControlOperation,
    ProcessControlRetry, RuntimeTerminalFacts,
    materialize::MaterializedAction,
    outcome_and_failure,
    platform::{ProcessSupervisor, ProtocolListener},
    protocol::{ConnectedProtocol, ProtocolState, ProtocolStep, WireEvent},
    ready_cancelled, ready_failure,
};

const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// After the direct child has exited, messages it wrote before exiting may
/// still be in flight from the reader thread. Draining waits for the reader
/// to reach end of stream and gives up after this quiet window so a
/// descendant that inherited the stream cannot stall terminalization.
const EXIT_DRAIN_WINDOW: Duration = Duration::from_millis(250);

pub(super) trait RecoveryRiskPersistence {
    fn set_recovery_risk(&self, run: RunId, requested: RecoveryRiskState) -> Result<(), ()>;
}

impl RecoveryRiskPersistence for PactrunPersistence {
    fn set_recovery_risk(&self, run: RunId, requested: RecoveryRiskState) -> Result<(), ()> {
        match requested {
            RecoveryRiskState::Open => self.open_recovery_risk(run),
            RecoveryRiskState::Clear => self.clear_recovery_risk(run),
        }
        .map_err(|_| ())
    }
}

pub(crate) struct LiveExecution {
    run: RunId,
    materialized: MaterializedAction,
    supervisor: ProcessSupervisor,
    protocol: LiveProtocol,
    state: ProtocolState,
    winner: OutcomeArbiter,
    hook_completion: Option<HookCompletionRecord>,
    completion_accepted: bool,
    submitted_handles: Vec<String>,
    exit_status: Option<ExitStatus>,
    startup_deadline: Option<Instant>,
    action_deadline: Option<Instant>,
    termination: Option<Termination>,
    policy: HookRuntimePolicy,
    cancellation: ActionCancellation,
}

impl fmt::Debug for LiveExecution {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LiveExecution")
            .field("run", &self.run)
            .field("materialized", &self.materialized)
            .field("state", &self.state)
            .field("winner", &self.winner)
            .field("completion_accepted", &self.completion_accepted)
            .field("exited", &self.exit_status.is_some())
            .field("policy", &self.policy)
            .finish()
    }
}

#[derive(Debug)]
enum LiveProtocol {
    Listening(ProtocolListener),
    Connected(ConnectedProtocol),
    /// The Session reached its terminal boundary or the stream was abandoned
    /// after a fault; later Hook bytes are irrelevant.
    Closed,
}

#[derive(Clone, Copy, Debug)]
struct Termination {
    reason: CancelReason,
    forced_at: Option<Instant>,
}

#[derive(Debug, Default)]
struct OutcomeArbiter {
    winner: Option<OutcomeWinner>,
}

impl OutcomeArbiter {
    fn claim(&mut self, winner: OutcomeWinner) -> bool {
        if self.winner.is_some() {
            return false;
        }
        self.winner = Some(winner);
        true
    }
}

pub(super) fn execute_registered(
    persistence: &PactrunPersistence,
    risk_persistence: &dyn RecoveryRiskPersistence,
    staging: &StagingSession,
    admitted: AdmittedExecution,
    policy: HookRuntimePolicy,
    cancellation: ActionCancellation,
) -> OwnerContinuation {
    let run = admitted.run();
    if cancellation.is_requested() {
        return ready_cancelled(run, Vec::new(), None);
    }
    let materialized = match MaterializedAction::create(persistence, staging, &admitted) {
        Ok(materialized) => materialized,
        Err(_) => return ready_failure(run, FailureKind::SessionMaterialization, Vec::new(), None),
    };
    if cancellation.is_requested() {
        let (execution, outputs) = materialized.into_execution();
        return ready_cancelled(run, outputs, Some(execution));
    }
    let listener = match ProtocolListener::bind() {
        Ok(listener) => listener,
        Err(_) => {
            let (execution, outputs) = materialized.into_execution();
            return ready_failure(
                run,
                FailureKind::ProtocolTransport,
                outputs,
                Some(execution),
            );
        }
    };
    let supervisor = match cancellation.arbitrate_launch(|| {
        ProcessSupervisor::spawn(
            materialized.program(),
            materialized.arguments(),
            admitted.plan().terminal(),
            &listener,
        )
    }) {
        Ok(Some(supervisor)) => supervisor,
        Ok(None) => {
            let (execution, outputs) = materialized.into_execution();
            return ready_cancelled(run, outputs, Some(execution));
        }
        Err(_) => {
            let (execution, outputs) = materialized.into_execution();
            return ready_failure(run, FailureKind::Launch, outputs, Some(execution));
        }
    };
    let started = Instant::now();
    let state = ProtocolState::new(
        materialized.session_id().to_owned(),
        materialized.output_handle_set(),
    );
    drive_execution(
        risk_persistence,
        LiveExecution {
            run,
            materialized,
            supervisor,
            protocol: LiveProtocol::Listening(listener),
            state,
            winner: OutcomeArbiter::default(),
            hook_completion: None,
            completion_accepted: false,
            submitted_handles: Vec::new(),
            exit_status: None,
            startup_deadline: policy
                .startup_timeout
                .map(|timeout| checked_deadline(started, timeout)),
            action_deadline: policy
                .action_timeout
                .map(|timeout| checked_deadline(started, timeout)),
            termination: None,
            policy,
            cancellation,
        },
    )
}

fn drive_execution(
    risk_persistence: &dyn RecoveryRiskPersistence,
    mut live: LiveExecution,
) -> OwnerContinuation {
    loop {
        while let Some(event) = next_protocol_event(&mut live) {
            match handle_wire_event(risk_persistence, &mut live, event) {
                Flow::Continue => {}
                Flow::RetryDurable(operation) => {
                    return OwnerContinuation::RetryDurableOperation(DurableOperationRetry {
                        live,
                        operation,
                    });
                }
                Flow::Fail(failure) => {
                    return terminate_for_failure(risk_persistence, live, failure);
                }
            }
        }

        if live.exit_status.is_none() {
            match live.supervisor.try_wait() {
                Ok(Some(status)) => live.exit_status = Some(status),
                Ok(None) => {}
                Err(_) => {
                    return OwnerContinuation::RetryProcessControl(ProcessControlRetry {
                        live,
                        operation: ProcessControlOperation::AwaitExit,
                    });
                }
            }
        }
        if live.exit_status.is_some() {
            return finish_after_exit(risk_persistence, live);
        }

        observe_control(&mut live);
        if forced_termination_due(&live) {
            return force_termination(risk_persistence, live);
        }

        if let LiveProtocol::Listening(listener) = &mut live.protocol {
            match listener.try_accept() {
                Ok(Some(stream)) => {
                    match ConnectedProtocol::start(stream, live.materialized.session()) {
                        Ok(connected) => live.protocol = LiveProtocol::Connected(connected),
                        Err(_) => {
                            return terminate_for_failure(
                                risk_persistence,
                                live,
                                FailureKind::ProtocolTransport,
                            );
                        }
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    return terminate_for_failure(
                        risk_persistence,
                        live,
                        FailureKind::ProtocolTransport,
                    );
                }
            }
        }

        thread::sleep(POLL_INTERVAL);
    }
}

fn next_protocol_event(live: &mut LiveExecution) -> Option<WireEvent> {
    let LiveProtocol::Connected(connected) = &mut live.protocol else {
        return None;
    };
    match connected.receiver.try_recv() {
        Ok(event) => Some(event),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => {
            live.protocol = LiveProtocol::Closed;
            Some(WireEvent::TransportFailure)
        }
    }
}

enum Flow {
    Continue,
    RetryDurable(PendingDurableOperation),
    Fail(FailureKind),
}

fn handle_wire_event(
    risk_persistence: &dyn RecoveryRiskPersistence,
    live: &mut LiveExecution,
    event: WireEvent,
) -> Flow {
    match event {
        WireEvent::Message(message) => {
            if let super::protocol::HookMessage::Complete(completion) = &message
                && completion.status == HookCompletionStatus::Success
                && live.state.risk() == RecoveryRiskState::Open
            {
                // Preserve the Hook-owned structural fact for the terminal
                // recovery consequence, but reject the completion at the
                // Frozen protocol boundary and publish no submitted handles.
                live.hook_completion = Some(HookCompletionRecord {
                    status: completion.status,
                    code: completion.code.clone(),
                    message: completion.message.clone(),
                });
                live.submitted_handles.clear();
            }
            let step = match live.state.accept(message) {
                Ok(step) => step,
                Err(failure) => return Flow::Fail(FailureKind::Protocol(failure)),
            };
            apply_step(risk_persistence, live, step)
        }
        WireEvent::Failure(failure) => Flow::Fail(FailureKind::Protocol(failure)),
        WireEvent::EndOfStream => match live.state.end_of_stream() {
            Some(failure) => Flow::Fail(FailureKind::Protocol(failure)),
            None => {
                live.protocol = LiveProtocol::Closed;
                Flow::Continue
            }
        },
        WireEvent::TransportFailure => Flow::Fail(FailureKind::ProtocolTransport),
    }
}

fn apply_step(
    risk_persistence: &dyn RecoveryRiskPersistence,
    live: &mut LiveExecution,
    step: ProtocolStep,
) -> Flow {
    match step {
        ProtocolStep::Ready => {
            if let Some(termination) = live.termination {
                send_cancel(live, termination.reason);
            }
            Flow::Continue
        }
        ProtocolStep::Diagnostic | ProtocolStep::CancelAcknowledged => Flow::Continue,
        ProtocolStep::RiskRequest {
            request_id,
            requested,
        } => {
            if risk_persistence
                .set_recovery_risk(live.run, requested)
                .is_err()
            {
                return Flow::RetryDurable(PendingDurableOperation::SetRisk {
                    request_id,
                    requested,
                });
            }
            acknowledge_risk(live, request_id, requested)
        }
        ProtocolStep::Completed(completion) => {
            let LiveProtocol::Connected(connected) = &mut live.protocol else {
                return Flow::Fail(FailureKind::ProtocolTransport);
            };
            if connected
                .writer
                .write_value(&json!({"type": "completion_accepted"}))
                .is_err()
            {
                return Flow::Fail(FailureKind::ProtocolTransport);
            }
            live.completion_accepted = true;
            live.submitted_handles = completion.produced_outputs.clone();
            live.hook_completion = Some(HookCompletionRecord {
                status: completion.status,
                code: completion.code,
                message: completion.message,
            });
            live.winner.claim(match completion.status {
                HookCompletionStatus::Success => OutcomeWinner::Succeeded,
                HookCompletionStatus::Failure => OutcomeWinner::Failed(FailureKind::HookFailure),
            });
            live.protocol = LiveProtocol::Closed;
            Flow::Continue
        }
        ProtocolStep::HookProtocolError => {
            live.protocol = LiveProtocol::Closed;
            Flow::Fail(FailureKind::HookReportedProtocol)
        }
    }
}

/// The durable transition is already published; only the acknowledgment
/// remains. A failed acknowledgment write is a transport failure.
fn acknowledge_risk(
    live: &mut LiveExecution,
    request_id: u64,
    requested: RecoveryRiskState,
) -> Flow {
    live.state.acknowledge_request(request_id, requested);
    let LiveProtocol::Connected(connected) = &mut live.protocol else {
        return Flow::Fail(FailureKind::ProtocolTransport);
    };
    if connected
        .writer
        .write_value(&json!({
            "type": "request_ack",
            "request_id": request_id,
            "risk_state": risk_name(requested),
        }))
        .is_err()
    {
        return Flow::Fail(FailureKind::ProtocolTransport);
    }
    Flow::Continue
}

fn observe_control(live: &mut LiveExecution) {
    let now = Instant::now();
    if live.cancellation.is_requested() {
        // A cancellation that arrives after the outcome is locked cannot
        // change it, but the live owner still propagates it as forced
        // termination so a lingering process tree cannot strand the Run.
        if live.winner.claim(OutcomeWinner::Cancelled) || live.termination.is_none() {
            begin_termination(live, now, CancelReason::Requested);
        }
    }
    let startup_expired = !live.state.is_ready()
        && live
            .startup_deadline
            .is_some_and(|deadline| now >= deadline);
    let action_expired =
        !live.completion_accepted && live.action_deadline.is_some_and(|deadline| now >= deadline);
    if (startup_expired || action_expired) && live.winner.claim(OutcomeWinner::TimedOut) {
        begin_termination(live, now, CancelReason::Timeout);
    }
}

fn begin_termination(live: &mut LiveExecution, now: Instant, reason: CancelReason) {
    if live.termination.is_some() {
        return;
    }
    live.termination = Some(Termination {
        reason,
        forced_at: live
            .policy
            .termination_grace
            .map(|grace| checked_deadline(now, grace)),
    });
    send_cancel(live, reason);
}

fn checked_deadline(start: Instant, duration: Duration) -> Instant {
    start
        .checked_add(duration)
        .expect("runtime deadline was validated before execution")
}

fn send_cancel(live: &mut LiveExecution, reason: CancelReason) {
    let Some(control_id) = live.state.begin_cancel() else {
        return;
    };
    if let LiveProtocol::Connected(connected) = &mut live.protocol {
        let _ = connected.writer.write_value(&json!({
            "type": "cancel",
            "control_id": control_id,
            "reason": reason.as_str(),
        }));
    }
}

fn forced_termination_due(live: &LiveExecution) -> bool {
    live.termination
        .and_then(|termination| termination.forced_at)
        .is_some_and(|deadline| Instant::now() >= deadline)
}

pub(super) fn resume_registered(
    risk_persistence: &dyn RecoveryRiskPersistence,
    continuation: OwnerContinuation,
) -> OwnerContinuation {
    match continuation {
        OwnerContinuation::ReadyToFinalize(state) => OwnerContinuation::ReadyToFinalize(state),
        OwnerContinuation::RetryFinalization(state) => OwnerContinuation::RetryFinalization(state),
        OwnerContinuation::RetryProcessControl(retry) => {
            resume_process_control(risk_persistence, retry)
        }
        OwnerContinuation::RetryDurableOperation(retry) => {
            resume_durable_operation(risk_persistence, retry)
        }
    }
}

fn resume_process_control(
    risk_persistence: &dyn RecoveryRiskPersistence,
    retry: ProcessControlRetry,
) -> OwnerContinuation {
    match retry.operation {
        ProcessControlOperation::AwaitExit => await_exit(risk_persistence, retry.live),
        ProcessControlOperation::TerminateTree => force_termination(risk_persistence, retry.live),
    }
}

fn resume_durable_operation(
    risk_persistence: &dyn RecoveryRiskPersistence,
    mut retry: DurableOperationRetry,
) -> OwnerContinuation {
    match retry.operation {
        PendingDurableOperation::SetRisk {
            request_id,
            requested,
        } => {
            if risk_persistence
                .set_recovery_risk(retry.live.run, requested)
                .is_err()
            {
                return OwnerContinuation::RetryDurableOperation(retry);
            }
            match acknowledge_risk(&mut retry.live, request_id, requested) {
                Flow::Continue => drive_execution(risk_persistence, retry.live),
                Flow::Fail(failure) => terminate_for_failure(risk_persistence, retry.live, failure),
                Flow::RetryDurable(_) => unreachable!("acknowledgment has no durable operation"),
            }
        }
    }
}

/// Locks the failure, tells a connected Hook which Pactrun code terminated
/// the Session, and forces process-tree termination so exit is observed.
fn terminate_for_failure(
    risk_persistence: &dyn RecoveryRiskPersistence,
    mut live: LiveExecution,
    failure: FailureKind,
) -> OwnerContinuation {
    live.winner.claim(OutcomeWinner::Failed(failure.clone()));
    if let (LiveProtocol::Connected(connected), FailureKind::Protocol(protocol)) =
        (&mut live.protocol, &failure)
    {
        let _ = connected.writer.write_value(&json!({
            "type": "protocol_error",
            "code": protocol.code,
        }));
    }
    live.protocol = LiveProtocol::Closed;
    force_termination(risk_persistence, live)
}

fn force_termination(
    risk_persistence: &dyn RecoveryRiskPersistence,
    mut live: LiveExecution,
) -> OwnerContinuation {
    if live.exit_status.is_some() {
        return finish_after_exit(risk_persistence, live);
    }
    if live.supervisor.terminate_tree().is_err() {
        // Termination can only be refused for a tree that no longer exists;
        // anything else stays a retained process-control retry.
        return match live.supervisor.try_wait() {
            Ok(Some(status)) => {
                live.exit_status = Some(status);
                finish_after_exit(risk_persistence, live)
            }
            _ => OwnerContinuation::RetryProcessControl(ProcessControlRetry {
                live,
                operation: ProcessControlOperation::TerminateTree,
            }),
        };
    }
    await_exit(risk_persistence, live)
}

fn await_exit(
    risk_persistence: &dyn RecoveryRiskPersistence,
    mut live: LiveExecution,
) -> OwnerContinuation {
    if live.exit_status.is_none() {
        match live.supervisor.wait() {
            Ok(status) => live.exit_status = Some(status),
            Err(_) => {
                return OwnerContinuation::RetryProcessControl(ProcessControlRetry {
                    live,
                    operation: ProcessControlOperation::AwaitExit,
                });
            }
        }
    }
    finish_after_exit(risk_persistence, live)
}

/// The direct child has exited. Messages it wrote before exiting are still
/// protocol-valid, so the stream is drained before the outcome is read.
fn finish_after_exit(
    risk_persistence: &dyn RecoveryRiskPersistence,
    mut live: LiveExecution,
) -> OwnerContinuation {
    debug_assert!(live.exit_status.is_some());
    let quiet_until = Instant::now() + EXIT_DRAIN_WINDOW;
    while let Some(event) = next_drained_event(&live, quiet_until) {
        match handle_wire_event(risk_persistence, &mut live, event) {
            Flow::Continue => {}
            Flow::RetryDurable(operation) => {
                return OwnerContinuation::RetryDurableOperation(DurableOperationRetry {
                    live,
                    operation,
                });
            }
            Flow::Fail(failure) => {
                live.winner.claim(OutcomeWinner::Failed(failure));
                break;
            }
        }
    }
    live.protocol = LiveProtocol::Closed;
    if live.winner.winner.is_none() {
        live.winner
            .claim(OutcomeWinner::Failed(FailureKind::ProtocolTransport));
    }
    let winner = live
        .winner
        .winner
        .take()
        .expect("process exit establishes an outcome winner");
    let (outcome, primary_failure) = outcome_and_failure(winner);
    let submitted_outputs = live.materialized.submitted_outputs(&live.submitted_handles);
    let (execution, outputs) = live.materialized.into_execution();
    OwnerContinuation::ReadyToFinalize(FinalizationState {
        facts: RuntimeTerminalFacts {
            run: live.run,
            outcome,
            primary_failure,
            hook_completion: live.hook_completion,
            completion_accepted: live.completion_accepted,
            process_started: true,
            process_terminated: true,
            submitted_outputs,
            outputs,
            execution: Some(execution),
        },
        prepared: None,
        cleanup_attempted: false,
        publication_failed: false,
        cleanup_failed: false,
    })
}

/// Waits briefly for the reader thread to deliver whatever the exited child
/// wrote before it exited; `None` once the stream is closed or quiet.
fn next_drained_event(live: &LiveExecution, quiet_until: Instant) -> Option<WireEvent> {
    let LiveProtocol::Connected(connected) = &live.protocol else {
        return None;
    };
    loop {
        match connected.receiver.recv_timeout(POLL_INTERVAL) {
            Ok(event) => return Some(event),
            Err(RecvTimeoutError::Timeout) if Instant::now() < quiet_until => {}
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum CancelReason {
    Requested,
    Timeout,
}

impl CancelReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Timeout => "timeout",
        }
    }
}

fn risk_name(risk: RecoveryRiskState) -> &'static str {
    match risk {
        RecoveryRiskState::Clear => "clear",
        RecoveryRiskState::Open => "open",
    }
}
