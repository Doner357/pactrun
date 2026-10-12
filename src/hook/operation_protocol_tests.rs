use super::*;
use serde_json::json;

const SESSION: &str = "00000000000000000000000000000001";
const REPORT: &str = "00000000000000000000000000000002";
const UNUSED: &str = "00000000000000000000000000000003";
const TARGET: &str = "00000000000000000000000000000004";
const OPERATIONS: [SessionOperation; 5] = [
    SessionOperation::Action,
    SessionOperation::Capture,
    SessionOperation::Restore,
    SessionOperation::Migration,
    SessionOperation::Cleanup,
];

fn parse(value: Value, operation: SessionOperation) -> Result<HookMessage, ProtocolFailure> {
    parse_hook_message_for_contract(
        &serde_json::to_vec(&value).unwrap(),
        operation,
        crate::domain::operation_hook_version(),
    )
}

fn state(operation: SessionOperation) -> ProtocolState {
    let target = if operation == SessionOperation::Migration {
        BTreeSet::from([TARGET.to_owned()])
    } else {
        BTreeSet::new()
    };
    let mut state = ProtocolState::new_operation_contract(
        SESSION.into(),
        operation,
        BTreeSet::from([REPORT.into(), UNUSED.into()]),
        target,
    )
    .unwrap();
    state.accept(parse(json!({"type":"session_ready", "protocol_version":"1.0-alpha.2", "session_id":SESSION}), operation).unwrap()).unwrap();
    state
}

fn completion(operation: SessionOperation, success: bool) -> Value {
    let mut value = json!({"type":"complete", "operation":operation.wire_name(),
        "status": if success {"success"} else {"failure"}, "produced_outputs":[REPORT]});
    if operation == SessionOperation::Capture {
        value["service_content"] = json!([]);
    }
    if operation == SessionOperation::Migration {
        value["produced_target_outputs"] = if success { json!([TARGET]) } else { json!([]) };
    }
    value
}

// Test-ID: PR-TEST-0709
// Verifies: PR-REQ-0387, PR-REQ-0391
#[test]
fn every_operation_can_submit_ordinary_outputs_on_success_and_failure() {
    for operation in OPERATIONS {
        for success in [true, false] {
            let mut state = state(operation);
            let ProtocolStep::Completed(done) = state
                .accept(parse(completion(operation, success), operation).unwrap())
                .unwrap()
            else {
                panic!("not completed");
            };
            assert_eq!(done.produced_outputs, vec![REPORT]);
            assert_eq!(
                done.status,
                if success {
                    HookCompletionStatus::Success
                } else {
                    HookCompletionStatus::Failure
                }
            );
            assert_eq!(
                done.produced_target_outputs.len(),
                usize::from(operation == SessionOperation::Migration && success)
            );
            assert!(state.is_terminal());
        }
    }
}

// Test-ID: PR-TEST-0710
// Verifies: PR-REQ-0387, PR-REQ-0391
#[test]
fn common_outputs_remain_optional_but_submitted_handles_are_strict() {
    for operation in OPERATIONS {
        let mut omitted = completion(operation, true);
        omitted["produced_outputs"] = json!([]);
        assert!(
            state(operation)
                .accept(parse(omitted, operation).unwrap())
                .is_ok()
        );
        for handles in [json!([REPORT, REPORT]), json!([TARGET]), json!([SESSION])] {
            let mut invalid = completion(operation, true);
            invalid["produced_outputs"] = handles;
            assert!(
                state(operation)
                    .accept(parse(invalid, operation).unwrap())
                    .is_err()
            );
        }
        let mut missing = completion(operation, true);
        missing.as_object_mut().unwrap().remove("produced_outputs");
        assert!(parse(missing, operation).is_err());
        let mut invalid = completion(operation, true);
        invalid["produced_outputs"] = Value::Null;
        assert!(parse(invalid, operation).is_err());
    }
    for (success, targets) in [
        (true, json!([])),
        (true, json!([REPORT])),
        (true, json!([TARGET, TARGET])),
        (false, json!([TARGET])),
    ] {
        let mut invalid = completion(SessionOperation::Migration, success);
        invalid["produced_target_outputs"] = targets;
        assert!(
            state(SessionOperation::Migration)
                .accept(parse(invalid, SessionOperation::Migration).unwrap())
                .is_err()
        );
    }
}

// Test-ID: PR-TEST-0711
// Verifies: PR-REQ-0387
#[test]
fn owner_selects_exact_protocol_and_old_closed_completion_shapes_remain() {
    for operation in OPERATIONS {
        let bytes = serde_json::to_vec(&completion(operation, true)).unwrap();
        if operation != SessionOperation::Action {
            assert!(parse_hook_message_for(&bytes, operation).is_err());
        }
    }
    let mut old = ProtocolState::new(SESSION.into(), BTreeSet::new());
    assert!(old.accept(parse(json!({"type":"session_ready", "protocol_version":"1.0-alpha.2", "session_id":SESSION}), SessionOperation::Action).unwrap()).is_err());
    let mut new = ProtocolState::new_operation_contract(
        SESSION.into(),
        SessionOperation::Action,
        BTreeSet::new(),
        BTreeSet::new(),
    )
    .unwrap();
    assert!(new.accept(parse_hook_message(&serde_json::to_vec(&json!({"type":"session_ready", "protocol_version":"1.0-alpha.1", "session_id":SESSION})).unwrap()).unwrap()).is_err());
    assert!(
        ProtocolState::new_operation_contract(
            SESSION.into(),
            SessionOperation::Migration,
            BTreeSet::from([REPORT.into()]),
            BTreeSet::from([REPORT.into()])
        )
        .is_err()
    );
}

// Test-ID: PR-TEST-0712
// Verifies: PR-REQ-0387, PR-REQ-0391
#[test]
fn transform_proposal_keeps_ordinary_reports_separate_from_target_commit_authority() {
    use super::authority::{Message, State, Step, parse_message_for_contract};
    const COMMIT: &str = "00000000000000000000000000000005";
    let decode = |value: Value| {
        parse_message_for_contract(
            &serde_json::to_vec(&value).unwrap(),
            SessionOperation::Migration,
            crate::domain::operation_hook_version(),
        )
        .unwrap()
    };
    let ready = || {
        let mut state = State::new_operation_contract(
            SESSION.into(),
            SessionOperation::Migration,
            BTreeSet::from([REPORT.into(), UNUSED.into()]),
            BTreeSet::from([TARGET.into()]),
            Some(COMMIT.into()),
        )
        .unwrap();
        state.accept(decode(json!({"type":"session_ready","protocol_version":"1.0-alpha.2","session_id":SESSION}))).unwrap();
        state
            .accept(Message::Common(HookMessage::Request {
                request_id: 1,
                requested: RecoveryRiskState::Open,
            }))
            .unwrap();
        state.acknowledge_request(1, RecoveryRiskState::Open);
        state
    };
    let proposal = || json!({"type":"target_ready","commit_handle":COMMIT,"produced_target_outputs":[TARGET],"produced_outputs":[REPORT]});
    let mut state = ready();
    let Step::TargetProposed(accepted) = state.accept(decode(proposal())).unwrap() else {
        panic!("not proposed");
    };
    assert_eq!(accepted.outputs(), &[TARGET]);
    assert_eq!(accepted.ordinary_outputs(), &[REPORT]);
    assert_eq!(
        accepted.receipt(),
        json!({"type":"target_ready_received","commit_handle":COMMIT})
    );
    assert_eq!(state.risk(), RecoveryRiskState::Open);
    for handles in [json!([TARGET]), json!([REPORT, REPORT]), json!([COMMIT])] {
        let mut invalid = proposal();
        invalid["produced_outputs"] = handles;
        assert!(ready().accept(decode(invalid)).is_err());
    }
    let mut missing = proposal();
    missing.as_object_mut().unwrap().remove("produced_outputs");
    assert!(
        parse_message_for_contract(
            &serde_json::to_vec(&missing).unwrap(),
            SessionOperation::Migration,
            crate::domain::operation_hook_version()
        )
        .is_err()
    );
    assert!(
        authority::parse_message(
            &serde_json::to_vec(&proposal()).unwrap(),
            SessionOperation::Migration
        )
        .is_err()
    );
    assert!(matches!(
        ready()
            .accept(decode(completion(SessionOperation::Migration, false)))
            .unwrap(),
        Step::Common(ProtocolStep::Completed(_))
    ));
}

// Test-ID: PR-TEST-0713
// Verifies: PR-REQ-0387
#[test]
fn operation_framing_uses_exact_owner_version_in_both_readers() {
    let version = crate::domain::operation_hook_version();
    assert_eq!(preamble_for_version(version).unwrap(), OPERATION_PREAMBLE);
    for operation in OPERATIONS {
        let payload = serde_json::to_vec(&completion(operation, true)).unwrap();
        for preamble in [PREAMBLE, OPERATION_PREAMBLE] {
            let mut wire = preamble.to_vec();
            wire.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            wire.extend_from_slice(&payload);
            let mut common = Vec::new();
            read_wire_events_for_contract(
                &mut io::Cursor::new(&wire),
                operation,
                version,
                |event| {
                    common.push(event);
                    true
                },
            );
            let mut service = Vec::new();
            authority::read_events_for_contract(
                &mut io::Cursor::new(&wire),
                operation,
                version,
                |event| {
                    service.push(event);
                    true
                },
            );
            if preamble == OPERATION_PREAMBLE {
                assert!(matches!(
                    &common[..],
                    [
                        WireEvent::Message(HookMessage::Complete(_)),
                        WireEvent::EndOfStream
                    ]
                ));
                assert!(matches!(
                    &service[..],
                    [
                        authority::Event::Message(authority::Message::Common(
                            HookMessage::Complete(_)
                        )),
                        authority::Event::EndOfStream
                    ]
                ));
                let mut old = Vec::new();
                read_wire_events_for(&mut io::Cursor::new(&wire), operation, |event| {
                    old.push(event);
                    true
                });
                assert!(matches!(&old[..], [WireEvent::Failure(_)]));
            } else {
                assert!(matches!(&common[..], [WireEvent::Failure(_)]));
                assert!(matches!(&service[..], [authority::Event::Failure(_)]));
            }
        }
    }
}
