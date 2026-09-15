use super::*;
use serde_json::json;

const SESSION: &str = "00000000000000000000000000000001";
const COMMIT: &str = "00000000000000000000000000000002";
const FIRST: &str = "00000000000000000000000000000003";
const SECOND: &str = "00000000000000000000000000000004";
fn message(value: Value) -> Message {
    parse_message(
        &serde_json::to_vec(&value).unwrap(),
        SessionOperation::Migration,
    )
    .unwrap()
}
fn ready(state: &mut State) {
    assert!(matches!(
        state
            .accept(message(
                json!({"type":"session_ready", "protocol_version":2,"session_id":SESSION})
            ))
            .unwrap(),
        Step::Common(ProtocolStep::Ready)
    ));
}
fn state(transform: bool) -> State {
    State::new(
        SESSION.into(),
        SessionOperation::Migration,
        [FIRST.into(), SECOND.into()].into(),
        transform.then(|| COMMIT.into()),
    )
    .unwrap()
}
fn proposal() -> Value {
    json!({"type":"target_ready","commit_handle":COMMIT,"produced_target_outputs":[FIRST,SECOND]})
}
fn enter(state: &mut State) {
    let step = state
        .accept(Message::Common(HookMessage::Request {
            request_id: 1,
            requested: RecoveryRiskState::Open,
        }))
        .unwrap();
    assert!(matches!(
        step,
        Step::Common(ProtocolStep::RiskRequest {
            request_id: 1,
            requested: RecoveryRiskState::Open
        })
    ));
}

// Test-ID: PR-TEST-0360
// Verifies: PR-REQ-0321, PR-REQ-0322
#[test]
fn target_proposal_wire_is_closed_and_canonically_ordered_without_changing_v1_completion() {
    assert!(matches!(message(proposal()), Message::TargetReady(_)));
    for mutate in 0..7 {
        let mut value = proposal();
        match mutate {
            0 => value["produced_target_outputs"] = json!([SECOND, FIRST]),
            1 => value["produced_target_outputs"] = json!([FIRST, FIRST]),
            2 => value["produced_target_outputs"] = json!([{"handle":FIRST}]),
            3 => value["status"] = json!("success"),
            4 => value["risk_state"] = json!("clear"),
            5 => {
                value
                    .as_object_mut()
                    .unwrap()
                    .remove("produced_target_outputs");
            }
            6 => value["commit_handle"] = json!("foreign-host-path"),
            _ => unreachable!(),
        }
        assert!(
            parse_message(
                &serde_json::to_vec(&value).unwrap(),
                SessionOperation::Migration
            )
            .is_err(),
            "{mutate}"
        );
    }
    assert_eq!(
        parse_message(
            br#"{"type":"target_ready","type":"target_ready"}"#,
            SessionOperation::Migration
        )
        .unwrap_err()
        .code,
        "invalid_frame"
    );
    assert!(
        super::super::parse_hook_message_for(
            &serde_json::to_vec(&proposal()).unwrap(),
            SessionOperation::Migration
        )
        .is_err()
    );
    // V2 common completion retains the V1 set semantics: only the NEW proposal
    // array is sorted on the wire, not historical completion arrays.
    let mut ordinary = state(false);
    ready(&mut ordinary);
    let completed = ordinary.accept(message(json!({"type":"complete","operation":"migration","status":"success","produced_target_outputs":[SECOND,FIRST]}))).unwrap();
    assert!(matches!(
        completed,
        Step::Common(ProtocolStep::Completed(_))
    ));
}

// Test-ID: PR-TEST-0361
// Verifies: PR-REQ-0322
#[test]
fn target_proposal_requires_acknowledged_open_and_receipt_never_clears_risk_or_reports_success() {
    let mut transform = state(true);
    assert!(transform.accept(message(proposal())).is_err());
    ready(&mut transform);
    assert!(transform.accept(message(proposal())).is_err());
    let success = || {
        message(
            json!({"type":"complete","operation":"migration","status":"success","produced_target_outputs":[FIRST,SECOND]}),
        )
    };
    assert!(transform.accept(success()).is_err());
    enter(&mut transform);
    assert!(transform.accept(message(proposal())).is_err());
    assert_eq!(transform.risk(), RecoveryRiskState::Clear);
    transform.acknowledge_request(1, RecoveryRiskState::Open);
    assert!(transform.accept(success()).is_err());
    assert!(
        transform
            .accept(Message::Common(HookMessage::Request {
                request_id: 2,
                requested: RecoveryRiskState::Clear
            }))
            .is_err()
    );
    let mut foreign = proposal();
    foreign["commit_handle"] = json!(FIRST);
    assert_eq!(
        transform.accept(message(foreign)).unwrap_err().code,
        "invalid_authority"
    );
    let mut missing = proposal();
    missing["produced_target_outputs"] = json!([FIRST]);
    assert_eq!(
        transform.accept(message(missing)).unwrap_err().code,
        "invalid_target_proposal"
    );
    let Step::TargetProposed(accepted) = transform.accept(message(proposal())).unwrap() else {
        panic!("proposal must not be a completion");
    };
    assert_eq!(
        accepted.receipt(),
        json!({"type":"target_ready_received","commit_handle":COMMIT})
    );
    assert_eq!(accepted.outputs(), &[FIRST, SECOND]);
    assert_eq!(transform.risk(), RecoveryRiskState::Open);
    assert!(transform.messaging_terminal());
    assert!(transform.end_of_stream().is_none());
    assert!(transform.begin_cancel().is_none());
    assert!(transform.accept(message(proposal())).is_err());
    assert!(
        transform
            .accept(Message::Common(HookMessage::Diagnostic))
            .is_err()
    );
    let mut failed = state(true);
    ready(&mut failed);
    enter(&mut failed);
    failed.acknowledge_request(1, RecoveryRiskState::Open);
    assert!(matches!(failed.accept(message(json!({"type":"complete","operation":"migration","status":"failure","produced_target_outputs":[]}))).unwrap(), Step::Common(ProtocolStep::Completed(HookCompletion { status: HookCompletionStatus::Failure, .. }))));
    assert_eq!(failed.risk(), RecoveryRiskState::Open);
    let mut ordinary = state(false);
    ready(&mut ordinary);
    assert_eq!(
        ordinary.accept(message(proposal())).unwrap_err().code,
        "unexpected_message"
    );
}

// Test-ID: PR-TEST-0362
// Verifies: PR-REQ-0321
#[test]
fn v2_framing_and_handshake_are_owner_selected_without_fallback() {
    // Literal contract bytes, independent of the implementation constant used
    // by the normal runtime and most worker fixtures.
    const FROZEN_PREAMBLE: &[u8] = b"pactrun.hook-protocol\0\0\0\0\x02";
    assert_eq!(PREAMBLE_V2, FROZEN_PREAMBLE);
    let bytes = serde_json::to_vec(
        &json!({"type":"session_ready","protocol_version":2,"session_id":SESSION}),
    )
    .unwrap();
    let mut wire = FROZEN_PREAMBLE.to_vec();
    wire.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    wire.extend_from_slice(&bytes);
    let mut events = Vec::new();
    read_events(
        &mut io::Cursor::new(&wire),
        SessionOperation::Migration,
        |e| {
            events.push(e);
            true
        },
    );
    assert!(matches!(
        &events[..],
        [
            Event::Message(Message::Common(HookMessage::SessionReady {
                protocol_version: 2,
                ..
            })),
            Event::EndOfStream
        ]
    ));
    let mut old_events = Vec::new();
    super::super::read_wire_events_for(
        &mut io::Cursor::new(&wire),
        SessionOperation::Migration,
        |e| {
            old_events.push(e);
            true
        },
    );
    assert!(matches!(&old_events[..], [WireEvent::Failure(_)]));
    for invalid in [
        PREAMBLE.to_vec(),
        PREAMBLE_V2[..5].to_vec(),
        [PREAMBLE_V2, &u32::MAX.to_be_bytes()].concat(),
        wire[..wire.len() - 1].to_vec(),
    ] {
        let mut events = Vec::new();
        read_events(
            &mut io::Cursor::new(invalid),
            SessionOperation::Migration,
            |e| {
                events.push(e);
                true
            },
        );
        assert!(matches!(
            &events[..],
            [Event::Failure(Failure {
                code: "invalid_frame"
            })]
        ));
    }
    let mut selected = state(false);
    assert!(
        selected
            .accept(message(
                json!({"type":"session_ready","protocol_version":1,"session_id":SESSION})
            ))
            .is_err()
    );
    assert!(!selected.is_ready());
    ready(&mut selected);
}

fn native_path() -> String {
    if cfg!(windows) {
        r"C:\pactrun-protocol-fixture\resource".into()
    } else {
        "/pactrun-protocol-fixture/resource".into()
    }
}
fn common_session(operation: SessionOperation) -> Value {
    let binding = json!({"handle":"00000000000000000000000000000011","input_id":"config","role":"active","readonly_path":native_path()});
    let mut outputs = json!([
        {"handle":FIRST,"input_id":"first","output_id":"first","staged_path":native_path()},
        {"handle":SECOND,"input_id":"second","output_id":"second","staged_path":native_path()}
    ]);
    for output in outputs.as_array_mut().unwrap() {
        output
            .as_object_mut()
            .unwrap()
            .remove(if operation == SessionOperation::Action {
                "input_id"
            } else {
                "output_id"
            });
    }
    let op = match operation {
        SessionOperation::Action => {
            json!({"kind":"action","action_id":"run","access":"observe","bindings":[binding],"outputs":outputs})
        }
        SessionOperation::Migration => {
            json!({"kind":"migration","source_revision":{"package_id":SESSION,"revision_content_digest":format!("sha256:{}", "0".repeat(64))},"source_bindings":[binding],"target_bindings":[],"target_outputs":outputs})
        }
        SessionOperation::Capture => {
            json!({"kind":"snapshot_capture","access":"observe","bindings":[binding],"candidate":{"handle":"00000000000000000000000000000012","root_path":native_path()}})
        }
        SessionOperation::Restore => {
            json!({"kind":"snapshot_restore","snapshot_id":SESSION,"bindings":[binding],"snapshot_content":{"handle":"00000000000000000000000000000012","readonly_root_path":native_path(),"logical_descriptors":[]}})
        }
    };
    json!({"type":"session_start","protocol_version":2,"session_id":SESSION,"run_id":SESSION,
        "revision":{"package_id":SESSION,"revision_content_digest":format!("sha256:{}", "0".repeat(64))},
        "parameters":[],"workspace":{"handle":"00000000000000000000000000000010","root_path":native_path()},"io":{"terminal":"none"},"operation":op})
}
fn declared() -> ExpectedAuthority {
    use crate::domain::*;
    ExpectedAuthority {
        access: ServiceAccessV2 {
            reference: ServiceReferenceV2 {
                view: ServiceView::Current,
                role: ServiceRole::Active,
                scope: ServiceScope::Resource(ServiceResourceIdentity::parse("config").unwrap()),
            },
            mode: ServiceAccessMode::Read,
        },
        resource_kind: Some(ServiceResourceKind::File),
    }
}
fn materialized() -> MaterializedAuthority {
    MaterializedAuthority {
        declaration: declared(),
        handle: "00000000000000000000000000000020".into(),
        path: native_path(),
    }
}

// Test-ID: PR-TEST-0364
// Verifies: PR-REQ-0321, PR-REQ-0244
#[test]
fn outgoing_session_authorities_match_admission_and_have_global_handle_uniqueness() {
    for operation in [
        SessionOperation::Action,
        SessionOperation::Capture,
        SessionOperation::Restore,
    ] {
        let prepared = PreparedSession::new(
            common_session(operation),
            operation,
            &[declared()],
            vec![materialized()],
            false,
            None,
        )
        .unwrap();
        let wire: Value = serde_json::from_slice(&prepared.payload).unwrap();
        assert_eq!(wire["protocol_version"], 2);
        assert_eq!(wire["service_authorities"][0]["resource_kind"], "file");
        assert!(wire.get("target_commit").is_none());
        for collision in [
            "00000000000000000000000000000010",
            "00000000000000000000000000000011",
            if operation == SessionOperation::Action {
                FIRST
            } else {
                "00000000000000000000000000000012"
            },
        ] {
            let mut authority = materialized();
            authority.handle = collision.into();
            assert!(
                PreparedSession::new(
                    common_session(operation),
                    operation,
                    &[declared()],
                    vec![authority],
                    false,
                    None
                )
                .is_err()
            );
        }
    }
    for fault in 0..7 {
        let mut authority = materialized();
        match fault {
            0 => authority.path = "relative/path".into(),
            1 => authority.path.push('\0'),
            2 => authority.declaration.resource_kind = None,
            3 => authority.declaration.access.mode = crate::domain::ServiceAccessMode::Write,
            4 => authority.declaration.access.reference.role = crate::domain::ServiceRole::Retained,
            5 => authority.handle = "short".into(),
            6 => authority.declaration.access.reference.view = crate::domain::ServiceView::Target,
            _ => unreachable!(),
        }
        assert!(
            PreparedSession::new(
                common_session(SessionOperation::Action),
                SessionOperation::Action,
                &[declared()],
                vec![authority],
                false,
                None
            )
            .is_err(),
            "{fault}"
        );
    }
    assert!(
        PreparedSession::new(
            common_session(SessionOperation::Action),
            SessionOperation::Action,
            &[],
            vec![materialized()],
            false,
            None
        )
        .is_err()
    );
    assert!(
        PreparedSession::new(
            common_session(SessionOperation::Action),
            SessionOperation::Action,
            &[declared()],
            vec![],
            false,
            None
        )
        .is_err()
    );
    let empty = PreparedSession::new(
        common_session(SessionOperation::Action),
        SessionOperation::Action,
        &[],
        vec![],
        false,
        None,
    )
    .unwrap();
    let wire: Value = serde_json::from_slice(&empty.payload).unwrap();
    assert_eq!(wire["service_authorities"], json!([]));
    let mut v1 = common_session(SessionOperation::Action);
    v1["protocol_version"] = json!(1);
    assert!(PreparedSession::new(v1, SessionOperation::Action, &[], vec![], false, None).is_err());
    let mut oversized = common_session(SessionOperation::Action);
    oversized["parameters"] = json!([{"parameter_id":"large","value":"x".repeat(MAX_PAYLOAD)}]);
    assert!(
        PreparedSession::new(
            oversized,
            SessionOperation::Action,
            &[],
            vec![],
            false,
            None
        )
        .is_err()
    );
    let migration = PreparedSession::new(
        common_session(SessionOperation::Migration),
        SessionOperation::Migration,
        &[],
        vec![],
        true,
        Some(COMMIT.into()),
    )
    .unwrap();
    let wire: Value = serde_json::from_slice(&migration.payload).unwrap();
    assert_eq!(wire["target_commit"], json!({"handle":COMMIT}));
    for collision in [
        FIRST,
        "00000000000000000000000000000010",
        "00000000000000000000000000000011",
    ] {
        assert!(
            PreparedSession::new(
                common_session(SessionOperation::Migration),
                SessionOperation::Migration,
                &[],
                vec![],
                true,
                Some(collision.into())
            )
            .is_err()
        );
    }
    assert!(
        PreparedSession::new(
            common_session(SessionOperation::Migration),
            SessionOperation::Migration,
            &[],
            vec![],
            true,
            None
        )
        .is_err()
    );
    assert!(
        PreparedSession::new(
            common_session(SessionOperation::Action),
            SessionOperation::Action,
            &[],
            vec![],
            true,
            Some(COMMIT.into())
        )
        .is_err()
    );
}

fn write_frame(writer: &mut impl Write, value: Value) {
    let payload = serde_json::to_vec(&value).unwrap();
    writer
        .write_all(&(payload.len() as u32).to_be_bytes())
        .unwrap();
    writer.write_all(&payload).unwrap();
    writer.flush().unwrap();
}
fn read_frame(reader: &mut impl Read) -> Value {
    let mut length = [0; 4];
    reader.read_exact(&mut length).unwrap();
    let length = u32::from_be_bytes(length) as usize;
    assert!(length <= MAX_PAYLOAD);
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload).unwrap();
    serde_json::from_slice(&payload).unwrap()
}

#[test]
fn v2_wire_worker() {
    let Ok(endpoint) = std::env::var("PACTRUN_HOOK_PROTOCOL_ENDPOINT") else {
        return;
    };
    #[cfg(windows)]
    let mut stream = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(endpoint)
        .unwrap();
    #[cfg(unix)]
    let mut stream = std::os::unix::net::UnixStream::connect(endpoint).unwrap();
    let mut preamble = [0; PREAMBLE_V2.len()];
    stream.read_exact(&mut preamble).unwrap();
    assert_eq!(preamble, PREAMBLE_V2);
    let session = read_frame(&mut stream);
    assert_eq!(session["protocol_version"], 2);
    assert_eq!(session["service_authorities"], json!([]));
    assert_eq!(session["target_commit"]["handle"], COMMIT);
    stream.write_all(PREAMBLE_V2).unwrap();
    write_frame(
        &mut stream,
        json!({"type":"session_ready","protocol_version":2,"session_id":SESSION}),
    );
    write_frame(
        &mut stream,
        json!({"type":"request","request_id":1,"request":{"kind":"enter_recovery_risk"}}),
    );
    assert_eq!(
        read_frame(&mut stream),
        json!({"type":"request_ack","request_id":1,"risk_state":"open"})
    );
    write_frame(&mut stream, proposal());
    let receipt = read_frame(&mut stream);
    assert_eq!(
        receipt,
        json!({"type":"target_ready_received","commit_handle":COMMIT})
    );
    assert!(receipt.get("status").is_none());
    assert!(receipt.get("risk_state").is_none());
}

// Test-ID: PR-TEST-0365
// Verifies: PR-REQ-0321, PR-REQ-0322
#[test]
fn target_receipt_crosses_a_real_private_channel_without_becoming_completion() {
    use crate::hook::platform::{ProcessSupervisor, ProtocolListener};
    use std::time::{Duration, Instant};
    struct Worker {
        process: ProcessSupervisor,
        exited: bool,
    }
    impl Drop for Worker {
        fn drop(&mut self) {
            if !self.exited {
                let _ = self.process.terminate_tree();
                let _ = self.process.wait();
            }
        }
    }
    let prepared = PreparedSession::new(
        common_session(SessionOperation::Migration),
        SessionOperation::Migration,
        &[],
        vec![],
        true,
        Some(COMMIT.into()),
    )
    .unwrap();
    let mut listener = ProtocolListener::bind().unwrap();
    let mut worker = Worker {
        process: ProcessSupervisor::spawn(
            &std::env::current_exe().unwrap(),
            &[
                "--exact".into(),
                "hook::protocol::v2::tests::v2_wire_worker".into(),
                "--nocapture".into(),
            ],
            crate::domain::TerminalContractV1::None,
            &listener,
        )
        .unwrap(),
        exited: false,
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    let stream = loop {
        if let Some(stream) = listener.try_accept().unwrap() {
            break stream;
        }
        assert!(Instant::now() < deadline, "V2 peer did not connect");
        std::thread::sleep(Duration::from_millis(5));
    };
    let (mut connected, mut state) = Connected::start(stream, prepared).unwrap();
    loop {
        let Event::Message(message) = connected
            .receiver
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
        else {
            panic!("unexpected V2 transport event");
        };
        match state.accept(message).unwrap() {
            Step::Common(ProtocolStep::Ready) => (),
            Step::Common(ProtocolStep::RiskRequest {
                request_id: 1,
                requested: RecoveryRiskState::Open,
            }) => {
                // This is transport/state evidence, not a durable-risk writer
                // test. The integrated owner must perform SQLite Open first.
                connected
                    .writer
                    .write_value(&json!({"type":"request_ack","request_id":1,"risk_state":"open"}))
                    .unwrap();
                state.acknowledge_request(1, RecoveryRiskState::Open);
            }
            Step::TargetProposed(proposal) => {
                connected.writer.write_value(&proposal.receipt()).unwrap();
                assert_eq!(state.risk(), RecoveryRiskState::Open);
                break;
            }
            _ => panic!("target proposal must not be an ordinary completion"),
        }
    }
    loop {
        if let Some(status) = worker.process.try_wait().unwrap() {
            worker.exited = true;
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "V2 peer did not terminate");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(
        connected
            .receiver
            .recv_timeout(Duration::from_secs(30))
            .unwrap(),
        Event::EndOfStream
    ));
    assert_eq!(state.risk(), RecoveryRiskState::Open);
    assert!(state.end_of_stream().is_none());
}
