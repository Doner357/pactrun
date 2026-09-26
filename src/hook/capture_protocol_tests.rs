// S5 protocol evidence, not an end-to-end managed Capture publisher.
use super::super::platform::{ProcessSupervisor, ProtocolListener};
use super::*;
use crate::domain::TerminalContractV1;
use serde_json::json;
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

const SESSION: &str = "00000000000000000000000000000021";
const WORKER: &str = "hook::protocol::capture_tests::capture_wire_worker";

fn ready(state: &mut ProtocolState) {
    state
        .accept(
            parse_capture_message(
                &serde_json::to_vec(
                    &json!({"type":"session_ready","protocol_version":"1.0-alpha.1","session_id":SESSION}),
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
}
fn complete(content: Value, status: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"type":"complete","operation":"snapshot_capture","status":status,"service_content":content})).unwrap()
}
fn values(content: &[CaptureServiceContentSubmission]) -> Value {
    json!(content.iter().map(|c| json!({"role":c.role.as_str(),"path":c.path.as_str(),"candidate_path":c.candidate_path.as_str()})).collect::<Vec<_>>())
}
fn corpus() -> Value {
    serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/vectors/hook_protocol/vectors.json"),
        )
        .unwrap(),
    )
    .unwrap()
}
fn transcript(messages: &[Value]) -> Result<Value, String> {
    let mut state = ProtocolState::new_capture(SESSION.to_owned());
    let mut completion = None;
    let mut content = Vec::new();
    let mut pending = None;
    for message in messages {
        let raw = message["raw_json"].as_str().unwrap();
        if message["direction"] == "hook_to_pactrun" {
            match state
                .accept(parse_capture_message(raw.as_bytes()).map_err(|e| e.code.to_owned())?)
                .map_err(|e| e.code.to_owned())?
            {
                ProtocolStep::Completed(done) => {
                    content = done.service_content;
                    completion = Some("submitted");
                }
                ProtocolStep::RiskRequest {
                    request_id,
                    requested,
                } => pending = Some((request_id, requested)),
                ProtocolStep::HookProtocolError(_) => completion = Some("protocol_error"),
                _ => {}
            }
        } else {
            let value: Value = serde_json::from_str(raw).unwrap();
            match value["type"].as_str().unwrap() {
                "completion_accepted" => {
                    assert_eq!(completion, Some("submitted"));
                    completion = Some("accepted");
                }
                "request_ack" => {
                    let (id, risk) = pending.take().unwrap();
                    assert_eq!(value["request_id"], id);
                    state.acknowledge_request(id, risk);
                }
                "cancel" => assert_eq!(value["control_id"], state.begin_cancel().unwrap()),
                other => panic!("unexpected owner message {other}"),
            }
        }
    }
    if let Some(error) = state.end_of_stream() {
        return Err(error.code.to_owned());
    }
    Ok(
        json!({"completion":completion.unwrap(),"operation":"snapshot_capture","risk_state":if state.risk()==RecoveryRiskState::Clear {"clear"} else {"open"},"service_content":values(&content)}),
    )
}

// Test-ID: PR-TEST-0242
// Verifies: PR-REQ-0213, PR-REQ-0216, PR-REQ-0206
#[test]
fn production_capture_decoder_and_state_match_unchanged_frozen_transcripts() {
    let corpus = corpus();
    let mut valid = 0;
    let mut invalid = 0;
    for vector in corpus["valid"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["session"] == "capture")
    {
        let actual = transcript(vector["messages"].as_array().unwrap()).unwrap();
        assert_eq!(actual, vector["expected_state"], "{}", vector["name"]);
        valid += 1;
    }
    for vector in corpus["invalid"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["session"] == "capture")
    {
        assert_eq!(
            transcript(vector["messages"].as_array().unwrap()).unwrap_err(),
            vector["expected_error"].as_str().unwrap(),
            "{}",
            vector["name"]
        );
        invalid += 1;
    }
    assert_eq!((valid, invalid), (3, 11));
}

// Test-ID: PR-TEST-0243
// Verifies: PR-REQ-0213, PR-REQ-0216, PR-REQ-0215
#[test]
fn capture_keeps_operation_authority_risk_ordering_and_semantic_set_rules() {
    let descriptor = json!({"role":"database","path":"db/main","candidate_path":"files/shared"});
    let other = json!({"role":"logs","path":"log/main","candidate_path":"files/shared"});
    let payload = complete(json!([other, descriptor]), "success");
    let parsed = parse_capture_message(&payload).unwrap();
    assert!(parse_hook_message(&payload).is_err());
    let mut state = ProtocolState::new_capture(SESSION.to_owned());
    ready(&mut state);
    let ProtocolStep::Completed(done) = state.accept(parsed).unwrap() else {
        panic!("completion")
    };
    assert_eq!(done.service_content.len(), 2);
    assert_eq!(done.service_content[0].role.as_str(), "database");
    assert_eq!(
        done.service_content[0].candidate_path,
        done.service_content[1].candidate_path
    );
    assert!(done.produced_outputs.is_empty());
    assert!(
        state
            .accept(parse_capture_message(&payload).unwrap())
            .is_err()
    );
    assert!(state.end_of_stream().is_none());

    let duplicate = complete(
        json!([
        {"role":"database","path":"db/main","candidate_path":"one"},
        {"role":"database","path":"db/main","candidate_path":"two"}]),
        "success",
    );
    assert_eq!(
        parse_capture_message(&duplicate).unwrap_err().code,
        "duplicate_semantic_key"
    );
    let digest_claim = complete(
        json!([{"role":"database","path":"db/main","candidate_path":"one","blob_digest":"sha256:untrusted"}]),
        "success",
    );
    assert_eq!(
        parse_capture_message(&digest_claim).unwrap_err().code,
        "unknown_field"
    );
    assert_eq!(
        parse_capture_message(&complete(json!([descriptor]), "failure"))
            .unwrap_err()
            .code,
        "invalid_completion"
    );
    let mut state = ProtocolState::new_capture(SESSION.to_owned());
    ready(&mut state);
    assert_eq!(state.accept(parse_hook_message(br#"{"type":"complete","operation":"action","status":"success","produced_outputs":[]}"#).unwrap()).unwrap_err().code,"invalid_completion");
    let request = br#"{"type":"request","request_id":1,"request":{"kind":"enter_recovery_risk"}}"#;
    assert!(matches!(
        state
            .accept(parse_capture_message(request).unwrap())
            .unwrap(),
        ProtocolStep::RiskRequest {
            requested: RecoveryRiskState::Open,
            ..
        }
    ));
    assert_eq!(state.risk(), RecoveryRiskState::Clear);
    assert_eq!(
        state
            .accept(parse_capture_message(&complete(json!([]), "success")).unwrap())
            .unwrap_err()
            .code,
        "request_in_flight"
    );
    state.acknowledge_request(1, RecoveryRiskState::Open);
    assert_eq!(
        state
            .accept(parse_capture_message(&complete(json!([]), "success")).unwrap())
            .unwrap_err()
            .code,
        "completion_with_open_risk"
    );
    assert!(matches!(
        state
            .accept(parse_capture_message(&complete(json!([]), "failure")).unwrap())
            .unwrap(),
        ProtocolStep::Completed(_)
    ));
}

// Test-ID: PR-TEST-0244
// Verifies: PR-REQ-0205, PR-REQ-0206
#[test]
fn both_wire_directions_keep_the_inclusive_frozen_frame_limit() {
    for length in [MAX_PAYLOAD - 1, MAX_PAYLOAD] {
        let value = json!({"x":"x".repeat(length-8)});
        assert_eq!(encode_payload(&value).unwrap().len(), length);
    }
    assert_eq!(
        encode_payload(&json!({"x":"x".repeat(MAX_PAYLOAD-7)}))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert!(encode_payload(&json!({"x":"\n".repeat(MAX_PAYLOAD/2)})).is_err());
    assert!(encode_payload(&json!([])).is_err());
    assert!(encode_payload(&json!("not an object")).is_err());
    let mut bytes = PREAMBLE.to_vec();
    bytes.extend_from_slice(&((MAX_PAYLOAD + 1) as u32).to_be_bytes());
    let mut events = Vec::new();
    read_wire_events_for(
        &mut io::Cursor::new(bytes),
        SessionOperation::Capture,
        |event| {
            events.push(event);
            true
        },
    );
    assert!(matches!(&events[..],[WireEvent::Failure(error)] if error.code=="frame_too_large"));
    let mut bytes = PREAMBLE.to_vec();
    let payload = complete(json!([]), "success");
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&payload);
    let mut events = Vec::new();
    read_wire_events_for(
        &mut io::Cursor::new(bytes),
        SessionOperation::Capture,
        |event| {
            events.push(event);
            true
        },
    );
    assert!(matches!(
        &events[..],
        [
            WireEvent::Message(HookMessage::Complete(_)),
            WireEvent::EndOfStream
        ]
    ));
}

struct OwnedProcess {
    process: ProcessSupervisor,
    exited: bool,
}
impl OwnedProcess {
    fn wait(&mut self, deadline: Instant) {
        loop {
            if let Some(status) = self.process.try_wait().unwrap() {
                self.exited = true;
                assert!(status.success());
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Capture wire worker did not exit"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if !self.exited {
            let _ = self.process.terminate_tree();
            let _ = self.process.wait();
        }
    }
}
fn worker(listener: &ProtocolListener, eof: bool) -> OwnedProcess {
    let mut args = vec![
        "--exact".to_owned(),
        WORKER.to_owned(),
        "--nocapture".to_owned(),
    ];
    if eof {
        args.extend(["--skip".to_owned(), "capture-expect-eof".to_owned()]);
    }
    OwnedProcess {
        process: ProcessSupervisor::spawn(
            &std::env::current_exe().unwrap(),
            &args,
            TerminalContractV1::None,
            listener,
        )
        .unwrap(),
        exited: false,
    }
}
fn connect(listener: &mut ProtocolListener, deadline: Instant) -> ProtocolStream {
    loop {
        if let Some(stream) = listener.try_accept().unwrap() {
            return stream;
        }
        assert!(
            Instant::now() < deadline,
            "Capture wire worker did not connect"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn capture_session() -> Value {
    let corpus = corpus();
    serde_json::from_str(
        corpus["session_specs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "capture")
            .unwrap()["raw_json"]
            .as_str()
            .unwrap(),
    )
    .unwrap()
}

// Test-ID: PR-TEST-0245
// Verifies: PR-REQ-0213, PR-REQ-0216
#[test]
fn capture_completion_crosses_the_real_private_transport_without_becoming_an_action() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m4-s5-protocol-tests");
    fs::create_dir_all(&parent).unwrap();
    let root = tempfile::Builder::new()
        .prefix("capture-")
        .tempdir_in(parent)
        .unwrap();
    let candidate = root.path().join("candidate");
    let workspace = root.path().join("workspace");
    fs::create_dir(&candidate).unwrap();
    fs::create_dir(&workspace).unwrap();
    let mut session = capture_session();
    session["workspace"]["root_path"] = json!(workspace);
    session["operation"]["candidate"]["root_path"] = json!(candidate);
    session["operation"]["bindings"] = json!([]);
    let mut listener = ProtocolListener::bind().unwrap();
    let mut process = worker(&listener, false);
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut connected =
        ConnectedProtocol::start_capture(connect(&mut listener, deadline), &session).unwrap();
    let mut state = ProtocolState::new_capture(SESSION.to_owned());
    loop {
        let WireEvent::Message(message) = connected
            .receiver
            .recv_timeout(Duration::from_secs(15))
            .unwrap()
        else {
            panic!("unexpected transport event")
        };
        match state.accept(message).unwrap() {
            ProtocolStep::Ready => {}
            ProtocolStep::Completed(done) => {
                assert_eq!(done.operation, SessionOperation::Capture);
                assert_eq!(done.service_content.len(), 2);
                assert!(done.produced_outputs.is_empty());
                for descriptor in done.service_content {
                    assert_eq!(descriptor.candidate_path.as_str(), "files/shared");
                }
                connected
                    .writer
                    .write_value(&json!({"type":"completion_accepted"}))
                    .unwrap();
                break;
            }
            _ => panic!("unexpected Capture protocol step"),
        }
    }
    process.wait(deadline);
    assert!(state.is_terminal());
    assert_eq!(
        fs::read(candidate.join("files/shared")).unwrap(),
        b"captured service bytes"
    );
}

// Supporting transport evidence for PR-TEST-0244.
#[test]
fn oversize_session_emits_no_preamble_or_partial_frame() {
    let mut session = capture_session();
    session["parameters"] = json!([{"parameter_id":"large","value":"x".repeat(MAX_PAYLOAD)}]);
    let mut listener = ProtocolListener::bind().unwrap();
    let mut process = worker(&listener, true);
    let deadline = Instant::now() + Duration::from_secs(30);
    assert_eq!(
        ConnectedProtocol::start_capture(connect(&mut listener, deadline), &session)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    process.wait(deadline);
}

fn read_frame(stream: &mut impl Read) -> Value {
    let mut size = [0; 4];
    stream.read_exact(&mut size).unwrap();
    let length = u32::from_be_bytes(size) as usize;
    assert!(length <= MAX_PAYLOAD);
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
fn send(stream: &mut impl Write, value: Value) {
    let bytes = encode_payload(&value).unwrap();
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .unwrap();
    stream.write_all(&bytes).unwrap();
    stream.flush().unwrap();
}

#[test]
fn capture_wire_worker() {
    let Ok(endpoint) = std::env::var("PACTRUN_HOOK_PROTOCOL_ENDPOINT") else {
        return;
    };
    #[cfg(windows)]
    let mut stream = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(endpoint)
        .unwrap();
    #[cfg(unix)]
    let mut stream = std::os::unix::net::UnixStream::connect(endpoint).unwrap();
    if std::env::args().any(|a| a == "capture-expect-eof") {
        assert_eq!(stream.read(&mut [0; 1]).unwrap(), 0);
        return;
    }
    let mut preamble = [0; PREAMBLE.len()];
    stream.read_exact(&mut preamble).unwrap();
    assert_eq!(preamble, PREAMBLE);
    let session = read_frame(&mut stream);
    assert_eq!(session["operation"]["kind"], "snapshot_capture");
    assert_eq!(session["operation"]["access"], "observe");
    assert!(session["operation"].get("outputs").is_none());
    let candidate = Path::new(
        session["operation"]["candidate"]["root_path"]
            .as_str()
            .unwrap(),
    );
    fs::create_dir(candidate.join("files")).unwrap();
    fs::write(candidate.join("files/shared"), b"captured service bytes").unwrap();
    stream.write_all(PREAMBLE).unwrap();
    send(
        &mut stream,
        json!({"type":"session_ready","protocol_version":"1.0-alpha.1","session_id":session["session_id"]}),
    );
    send(
        &mut stream,
        json!({"type":"complete","operation":"snapshot_capture","status":"success","service_content":[
        {"role":"logs","path":"log/main","candidate_path":"files/shared"},
        {"role":"database","path":"db/main","candidate_path":"files/shared"}]}),
    );
    assert_eq!(read_frame(&mut stream)["type"], "completion_accepted");
}
