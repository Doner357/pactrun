//! Shared synchronous one-shot Hook supervision.
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

use super::diagnostic_scope::diagnostic_scope;
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
    protocol_runtime::{ConnectedProtocol, ProtocolState, ProtocolStep, WireEvent},
    ready_cancelled, ready_failure,
};

const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// After the direct child has exited, messages it wrote before exiting may
/// still be in flight from the reader thread. Draining waits for the reader
/// to reach end of stream and gives up after this quiet window so a
/// descendant that inherited the stream cannot stall terminalization.
const EXIT_DRAIN_WINDOW: Duration = Duration::from_millis(250);

pub(crate) struct TargetRuntimeFacts {
    pub(super) run: RunId,
    pub(super) proposal: super::protocol::authority::AcceptedTargetProposal,
    pub(super) execution: crate::managed_data::ExecutionDirectory,
    pub(super) outputs: Vec<super::MigrationOutputSlot>,
}
impl fmt::Debug for TargetRuntimeFacts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TargetRuntimeFacts")
            .field("run", &self.run)
            .finish_non_exhaustive()
    }
}

pub(super) trait RecoveryRiskPersistence {
    fn diagnostic_root(&self) -> Option<std::path::PathBuf> {
        None
    }
    fn set_recovery_risk(&self, run: RunId, requested: RecoveryRiskState) -> Result<(), ()>;
}

impl RecoveryRiskPersistence for PactrunPersistence {
    fn diagnostic_root(&self) -> Option<std::path::PathBuf> {
        Some(self.diagnostic_root())
    }
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
    // Rust drops fields in declaration order: return terminal ownership before
    // enabling deferred diagnostic presentation.
    diagnostics: Option<super::diagnostics::DiagnosticScope>,
    protocol: LiveProtocol,
    state: ProtocolState,
    winner: OutcomeArbiter,
    hook_completion: Option<HookCompletionRecord>,
    completion_accepted: bool,
    target_proposal: Option<super::protocol::authority::AcceptedTargetProposal>,
    protocol_eof: bool,
    submitted_handles: Vec<String>,
    captured_at: Option<crate::domain::SnapshotTimestamp>,
    capture_submitted: Vec<crate::domain::CaptureServiceContentSubmission>,
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
        Err(error) => {
            return ready_failure(run, FailureKind::materialization(&error), Vec::new(), None);
        }
    };
    execute_materialized(risk_persistence, run, materialized, policy, cancellation)
}

pub(super) fn execute_materialized(
    risk_persistence: &dyn RecoveryRiskPersistence,
    run: RunId,
    mut materialized: MaterializedAction,
    policy: HookRuntimePolicy,
    cancellation: ActionCancellation,
) -> OwnerContinuation {
    let cleanup = materialized.operation() == super::protocol::SessionOperation::Cleanup;
    if cancellation.is_requested() {
        let (execution, outputs) = materialized.into_execution();
        return super::deletions::adapt_early(
            ready_cancelled(run, outputs, Some(execution)),
            cleanup,
            false,
        );
    }
    let listener = match ProtocolListener::for_execution(
        materialized.program(),
        materialized.arguments(),
        materialized.execution_root(),
    ) {
        Ok(listener) => listener,
        Err(error) => {
            let (execution, outputs) = materialized.into_execution();
            return super::deletions::adapt_early(
                ready_failure(
                    run,
                    FailureKind::IpcInitialization(super::startup::Reason::from_io(&error)),
                    outputs,
                    Some(execution),
                ),
                cleanup,
                false,
            );
        }
    };
    let delivery = cancellation.delivery.scope(run, materialized.session());
    let mut diagnostics = diagnostic_scope(risk_persistence, run, &materialized, &cancellation);
    if let Some(diagnostics) = &mut diagnostics {
        diagnostics.delivery = delivery.clone();
    }
    let supervisor = match cancellation.arbitrate_launch(|| {
        ProcessSupervisor::spawn_delivered(
            materialized.program(),
            materialized.arguments(),
            materialized.terminal(),
            &listener,
            delivery.as_ref(),
        )
    }) {
        Ok(Some(supervisor)) => supervisor,
        Ok(None) => {
            let (execution, outputs) = materialized.into_execution();
            return super::deletions::adapt_early(
                ready_cancelled(run, outputs, Some(execution)),
                cleanup,
                false,
            );
        }
        Err(_) => {
            let (execution, outputs) = materialized.into_execution();
            return super::deletions::adapt_early(
                ready_failure(run, FailureKind::Launch, outputs, Some(execution)),
                cleanup,
                true,
            );
        }
    };
    let started = Instant::now();
    let state = if let Some(state) = materialized.take_protocol_state() {
        ProtocolState(state)
    } else if cleanup {
        ProtocolState::new_cleanup(materialized.session_id().to_owned())
    } else if materialized.operation() == super::protocol::SessionOperation::Migration {
        ProtocolState::new_migration(
            materialized.session_id().to_owned(),
            materialized.output_handle_set(),
        )
    } else {
        ProtocolState::new(
            materialized.session_id().to_owned(),
            materialized.output_handle_set(),
        )
    };
    drive_execution(
        risk_persistence,
        LiveExecution {
            diagnostics,
            run,
            materialized,
            supervisor,
            protocol: LiveProtocol::Listening(listener),
            state,
            winner: OutcomeArbiter::default(),
            hook_completion: None,
            completion_accepted: false,
            target_proposal: None,
            protocol_eof: false,
            submitted_handles: Vec::new(),
            captured_at: None,
            capture_submitted: Vec::new(),
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

pub(super) fn execute_snapshot(
    persistence: &PactrunPersistence,
    staging: &StagingSession,
    claim: super::SnapshotExecutionClaim<'_>,
    policy: HookRuntimePolicy,
) -> RunId {
    execute_snapshot_with_risk(persistence, persistence, staging, claim, policy)
}

pub(super) fn execute_snapshot_with_risk(
    persistence: &PactrunPersistence,
    risk: &dyn RecoveryRiskPersistence,
    staging: &StagingSession,
    claim: super::SnapshotExecutionClaim<'_>,
    policy: HookRuntimePolicy,
) -> RunId {
    let run = claim.run();
    let cancellation = claim.cancellation();
    let kind = claim.plan().operation().kind();
    if cancellation.is_requested() {
        claim
            .stop_before_launch(crate::domain::RunFinish {
                outcome: crate::domain::RunOutcome::Cancelled,
                primary_failure: None,
                secondary_failures: Vec::new(),
                hook_completion: None,
            })
            .expect("valid cancellation");
        return run;
    }
    let materialized = match if kind == crate::domain::ManagedExecutionKind::SnapshotRestore {
        MaterializedAction::create_restore(persistence, staging, run, claim.plan(), &cancellation)
    } else {
        MaterializedAction::create_capture(persistence, staging, run, claim.plan(), &cancellation)
    } {
        Ok(value) => value,
        Err(error) => {
            let (outcome, mut primary_failure) =
                outcome_and_failure(OutcomeWinner::Failed(FailureKind::materialization(&error)));
            if let Some(primary) = &mut primary_failure {
                primary.step =
                    crate::domain::RunFailedStep::for_operation(primary.step.rank(), kind)
                        .expect("known step");
                primary.failure.message = match error {
                    super::materialize::MaterializationError::Capture(error) => error.message(),
                    _ => "Snapshot session materialization or content validation failed",
                }
                .to_owned();
            }
            let finish = crate::domain::RunFinish {
                outcome: if cancellation.is_requested() {
                    crate::domain::RunOutcome::Cancelled
                } else {
                    outcome
                },
                primary_failure: if cancellation.is_requested() {
                    None
                } else {
                    primary_failure
                },
                secondary_failures: Vec::new(),
                hook_completion: None,
            };
            claim
                .stop_before_launch(finish)
                .expect("non-success Snapshot execution");
            return run;
        }
    };
    let listener = match ProtocolListener::for_execution(
        materialized.program(),
        materialized.arguments(),
        materialized.execution_root(),
    ) {
        Ok(listener) => listener,
        Err(error) => {
            claim.replace(snapshot_before_launch(
                run,
                materialized,
                FailureKind::IpcInitialization(super::startup::Reason::from_io(&error)),
            ));
            return run;
        }
    };
    let delivery = cancellation.delivery.scope(run, materialized.session());
    let mut diagnostics = diagnostic_scope(risk, run, &materialized, &cancellation);
    if let Some(diagnostics) = &mut diagnostics {
        diagnostics.delivery = delivery.clone();
    }
    let mut pending_materialization = Some(materialized);
    match claim.launch_once(|_| {
        let mut materialized = pending_materialization.take().expect("one launch attempt");
        match ProcessSupervisor::spawn_delivered(
            materialized.program(),
            materialized.arguments(),
            materialized.terminal(),
            &listener,
            delivery.as_ref(),
        ) {
            Ok(supervisor) => {
                let started = Instant::now();
                let state = if let Some(state) = materialized.take_protocol_state() {
                    ProtocolState(state)
                } else if kind == crate::domain::ManagedExecutionKind::SnapshotRestore {
                    ProtocolState::new_restore(materialized.session_id().to_owned())
                } else {
                    ProtocolState::new_capture(materialized.session_id().to_owned())
                };
                Ok(LiveExecution {
                    diagnostics,
                    run,
                    materialized,
                    supervisor,
                    protocol: LiveProtocol::Listening(listener),
                    state,
                    winner: OutcomeArbiter::default(),
                    hook_completion: None,
                    completion_accepted: false,
                    target_proposal: None,
                    protocol_eof: false,
                    submitted_handles: Vec::new(),
                    captured_at: None,
                    capture_submitted: Vec::new(),
                    exit_status: None,
                    startup_deadline: policy.startup_timeout.map(|d| checked_deadline(started, d)),
                    action_deadline: policy.action_timeout.map(|d| checked_deadline(started, d)),
                    termination: None,
                    policy,
                    cancellation,
                })
            }
            Err(_) => Err(Box::new(snapshot_before_launch(
                run,
                materialized,
                FailureKind::Launch,
            ))),
        }
    }) {
        Ok(Some(attempt)) => attempt.replace(|live| drive_execution(risk, live)),
        Ok(None) => {
            if let Some(materialized) = pending_materialization.as_ref() {
                materialized.cleanup_unlaunched();
            }
        }
        Err(attempt) => attempt.replace(|continuation| *continuation),
    }
    run
}

fn snapshot_before_launch(
    run: RunId,
    mut materialized: MaterializedAction,
    failure: FailureKind,
) -> OwnerContinuation {
    let restore = materialized.operation() == super::protocol::SessionOperation::Restore;
    let acquisition = materialized.take_capture();
    let (execution, outputs) = materialized.into_execution();
    let (outcome, primary_failure) = outcome_and_failure(OutcomeWinner::Failed(failure));
    let facts = RuntimeTerminalFacts {
        run,
        outcome,
        primary_failure,
        hook_completion: None,
        completion_accepted: false,
        process_started: false,
        process_terminated: false,
        submitted_outputs: Vec::new(),
        outputs,
        migration_outputs: Vec::new(),
        execution: Some(execution),
    };
    if restore {
        super::restore::terminal(facts)
    } else {
        super::capture::terminal(facts, acquisition, None, Vec::new())
    }
}

fn drive_execution(
    risk_persistence: &dyn RecoveryRiskPersistence,
    mut live: LiveExecution,
) -> OwnerContinuation {
    loop {
        // Bound per-turn protocol work so a diagnostic flood cannot starve
        // cancellation, deadline observation or process supervision.
        for _ in 0..32 {
            let Some(event) = next_protocol_event(&mut live) else {
                break;
            };
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
                    let connected =
                        if let Some(transport) = live.materialized.take_protocol_transport() {
                            ConnectedProtocol::start_prepared(stream, transport)
                        } else {
                            match live.materialized.operation() {
                                super::protocol::SessionOperation::Action => {
                                    ConnectedProtocol::start(stream, live.materialized.session())
                                }
                                super::protocol::SessionOperation::Capture => {
                                    ConnectedProtocol::start_capture(
                                        stream,
                                        live.materialized.session(),
                                    )
                                }
                                super::protocol::SessionOperation::Restore => {
                                    ConnectedProtocol::start_restore(
                                        stream,
                                        live.materialized.session(),
                                    )
                                }
                                super::protocol::SessionOperation::Cleanup => {
                                    ConnectedProtocol::start_cleanup(
                                        stream,
                                        live.materialized.session(),
                                    )
                                }
                                super::protocol::SessionOperation::Migration => {
                                    ConnectedProtocol::start_migration(
                                        stream,
                                        live.materialized.session(),
                                    )
                                }
                            }
                        };
                    match connected {
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
            if let Some(completion) = message.completion()
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
                Err(failure) => return Flow::Fail(failure),
            };
            apply_step(risk_persistence, live, step)
        }
        WireEvent::Failure(failure) => Flow::Fail(failure),
        WireEvent::EndOfStream => match live.state.end_of_stream() {
            Some(failure) => Flow::Fail(failure),
            None => {
                live.protocol_eof = true;
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
        ProtocolStep::TargetProposed(proposal) => {
            let LiveProtocol::Connected(connected) = &mut live.protocol else {
                return Flow::Fail(FailureKind::ProtocolTransport);
            };
            if connected.writer.write_value(&proposal.receipt()).is_err() {
                return Flow::Fail(FailureKind::ProtocolTransport);
            }
            live.target_proposal = Some(proposal);
            crate::persistence::fault(crate::persistence::FaultPoint::AfterTargetProposalReceipt);
            // Keep reading through EOF: any message after the proposal fails
            // the operation. No successful Hook completion or risk clear exists.
            Flow::Continue
        }
        ProtocolStep::Ready => {
            live.startup_deadline = None;
            if let Some(termination) = live.termination {
                send_cancel(live, termination.reason);
            }
            Flow::Continue
        }
        ProtocolStep::Diagnostic(text) => {
            if let Some(scope) = &live.diagnostics {
                scope.record(text);
            }
            Flow::Continue
        }
        ProtocolStep::CancelAcknowledged => Flow::Continue,
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
            if let Some(scope) = &live.diagnostics {
                scope.record(crate::domain::HookText {
                    kind: crate::domain::DiagnosticKind::Completion,
                    severity: None,
                    code: completion.code.as_ref().map(|c| c.as_str().to_owned()),
                    message: completion.message.clone(),
                    completion_status: Some(completion.status),
                    truncated: false,
                    truncated_prefix_bytes: 0,
                });
            }
            if live.materialized.operation() == super::protocol::SessionOperation::Capture {
                if completion.status == HookCompletionStatus::Success && live.captured_at.is_none()
                {
                    live.captured_at = match super::capture::completion_time() {
                        Ok(at) => Some(at),
                        Err(_) => {
                            return Flow::Fail(FailureKind::CapturePublication(
                                "Capture completion time is unavailable",
                            ));
                        }
                    };
                }
                live.capture_submitted = completion.service_content.clone();
            }
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
        ProtocolStep::HookProtocolError(text) => {
            if let Some(scope) = &live.diagnostics {
                scope.record(text);
            }
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
        state @ (OwnerContinuation::Deletion(_) | OwnerContinuation::CleanupFinalization(_)) => {
            state
        }
        OwnerContinuation::TargetReady(facts) => OwnerContinuation::TargetReady(facts),
        OwnerContinuation::Migration(state) => OwnerContinuation::Migration(state),
        OwnerContinuation::RestoreFinalization(state) => {
            OwnerContinuation::RestoreFinalization(state)
        }
        OwnerContinuation::CaptureFinalization(state) => {
            OwnerContinuation::CaptureFinalization(state)
        }
        OwnerContinuation::Snapshot(state) => OwnerContinuation::Snapshot(state),
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
        ProcessControlOperation::AwaitTree => finish_after_exit(risk_persistence, retry.live),
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
    let failure = live.supervisor.startup_failure().unwrap_or(failure);
    live.winner.claim(OutcomeWinner::Failed(failure.clone()));
    if let LiveProtocol::Connected(connected) = &mut live.protocol {
        let code = match &failure {
            FailureKind::Protocol(p) => Some(p.code),
            FailureKind::ProtocolV2(p) => Some(p.code),
            _ => None,
        };
        if let Some(code) = code {
            let _ = connected
                .writer
                .write_value(&json!({"type":"protocol_error", "code":code}));
        }
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
    if let Some(failure) = live.supervisor.startup_failure() {
        live.winner.claim(OutcomeWinner::Failed(failure));
    }
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
    if live.target_proposal.is_some() {
        observe_control(&mut live);
        if !live.exit_status.as_ref().is_some_and(ExitStatus::success) || !live.protocol_eof {
            live.winner
                .claim(OutcomeWinner::Failed(FailureKind::ProtocolTransport));
        }
        if !live.supervisor.tree_terminated().unwrap_or(false) {
            if live.winner.winner.is_some() {
                let _ = live.supervisor.terminate_tree();
            }
            return OwnerContinuation::RetryProcessControl(ProcessControlRetry {
                live,
                operation: ProcessControlOperation::AwaitTree,
            });
        }
        if live.winner.winner.is_none() {
            let proposal = live.target_proposal.take().expect("validated proposal");
            let outputs = live.materialized.take_migration_outputs();
            let (execution, _) = live.materialized.into_execution();
            return OwnerContinuation::TargetReady(TargetRuntimeFacts {
                run: live.run,
                proposal,
                execution,
                outputs,
            });
        }
    }
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
    let capture_mode = live.materialized.operation() == super::protocol::SessionOperation::Capture;
    let restore_mode = live.materialized.operation() == super::protocol::SessionOperation::Restore;
    let cleanup_mode = live.materialized.operation() == super::protocol::SessionOperation::Cleanup;
    let capture = live.materialized.take_capture();
    let migration_outputs = live.materialized.take_migration_outputs();
    let (execution, outputs) = live.materialized.into_execution();
    let facts = RuntimeTerminalFacts {
        run: live.run,
        outcome,
        primary_failure,
        hook_completion: live.hook_completion,
        completion_accepted: live.completion_accepted,
        process_started: true,
        process_terminated: true,
        submitted_outputs,
        outputs,
        migration_outputs,
        execution: Some(execution),
    };
    if capture_mode {
        return super::capture::terminal(facts, capture, live.captured_at, live.capture_submitted);
    }
    if restore_mode {
        return super::restore::terminal(facts);
    }
    if cleanup_mode {
        return super::deletions::terminal_with_cancellation(facts, true, live.cancellation);
    }
    OwnerContinuation::ReadyToFinalize(FinalizationState {
        facts,
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
