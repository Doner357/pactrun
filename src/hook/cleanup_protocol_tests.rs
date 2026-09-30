use super::*;
use serde_json::json;

fn accept(state: &mut ProtocolState, value: Value) -> Result<ProtocolStep, ProtocolFailure> {
    state.accept(parse_hook_message_for(&serde_json::to_vec(&value).unwrap(), SessionOperation::Cleanup)?)
}
fn ready(state: &mut ProtocolState, session: &str) {
    accept(state, json!({"type":"session_ready","protocol_version":"1.0-alpha.1","session_id":session})).unwrap();
}

// Test-ID: PR-TEST-0399
// Verifies: PR-REQ-0216, PR-REQ-0334
#[test]
fn cleanup_decoder_matches_frozen_vector_and_never_accepts_outputs_or_open_risk_success() {
    let corpus: Value = serde_json::from_str(include_str!("../../tests/vectors/hook_protocol/vectors.json")).unwrap();
    let session: Value = serde_json::from_str(corpus["session_specs"].as_array().unwrap().iter()
        .find(|s| s["name"] == "cleanup").unwrap()["raw_json"].as_str().unwrap()).unwrap();
    let id = session["session_id"].as_str().unwrap();
    let vectors: Vec<_> = corpus["valid"].as_array().unwrap().iter().filter(|v| v["session"] == "cleanup").collect();
    assert_eq!(vectors.len(), 1);
    for vector in vectors {
        let mut state = ProtocolState::new_cleanup(id.to_owned());
        let mut pending = None;
        let mut submitted = false;
        let mut acknowledged = false;
        for frame in vector["messages"].as_array().unwrap() {
            let value: Value = serde_json::from_str(frame["raw_json"].as_str().unwrap()).unwrap();
            if frame["direction"] == "hook_to_pactrun" {
                match accept(&mut state, value).unwrap() {
                    ProtocolStep::RiskRequest { request_id, requested } => pending = Some((request_id, requested)),
                    ProtocolStep::Completed(done) => { assert!(done.produced_outputs.is_empty()); submitted = true; },
                    _ => {},
                }
            } else {
                match value["type"].as_str().unwrap() {
                    "request_ack" => { let (request, risk) = pending.take().unwrap(); assert_eq!(value["request_id"], request); state.acknowledge_request(request, risk); },
                    "completion_accepted" => { assert!(submitted); acknowledged = true; },
                    other => panic!("unexpected owner frame {other}"),
                }
            }
        }
        assert!(state.end_of_stream().is_none());
        assert!(acknowledged);
        assert_eq!(json!({"operation":"cleanup","completion":"accepted","risk_state":"clear"}), vector["expected_state"]);
        assert_eq!(state.risk(), RecoveryRiskState::Clear);
    }
    for extra in [json!({"produced_outputs":[]}), json!({"service_content":[]}), json!({"produced_target_outputs":[]})] {
        let mut complete = json!({"type":"complete","operation":"cleanup","status":"success"});
        complete.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        assert!(parse_hook_message_for(&serde_json::to_vec(&complete).unwrap(), SessionOperation::Cleanup).is_err());
    }
    for status in ["success", "failure"] {
        let mut state = ProtocolState::new_cleanup(id.to_owned());
        ready(&mut state, id);
        // Construct the typed request to independently exercise risk-state
        // policy; wire grammar remains covered by the frozen transcripts.
        state.accept(HookMessage::Request { request_id: 1, requested: RecoveryRiskState::Open }).unwrap();
        assert!(accept(&mut state, json!({"type":"complete","operation":"cleanup","status":status})).is_err());
        state.acknowledge_request(1, RecoveryRiskState::Open);
        let completion = accept(&mut state, json!({"type":"complete","operation":"cleanup","status":status}));
        assert_eq!(completion.is_ok(), status == "failure");
    }
}
