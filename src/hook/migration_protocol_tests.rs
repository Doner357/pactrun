use super::*;
use serde_json::json;

fn transcript(session: &Value, messages: &[Value]) -> Result<Value, String> {
    let outputs = session["operation"]["target_outputs"].as_array().unwrap().iter()
        .map(|output| output["handle"].as_str().unwrap().to_owned()).collect();
    let mut state = ProtocolState::new_migration(session["session_id"].as_str().unwrap().to_owned(), outputs);
    let mut pending = None;
    let mut completion = None;
    let mut submitted = Vec::new();
    for frame in messages {
        let raw = frame["raw_json"].as_str().unwrap();
        if frame["direction"] == "hook_to_pactrun" {
            let decoded = parse_hook_message_for(raw.as_bytes(), SessionOperation::Migration).map_err(|e|e.code.to_owned())?;
            match state.accept(decoded).map_err(|e|e.code.to_owned())? {
                ProtocolStep::RiskRequest {request_id, requested} => pending = Some((request_id, requested)),
                ProtocolStep::Completed(done) => {submitted = done.produced_outputs; completion = Some("submitted");},
                ProtocolStep::HookProtocolError(_) => completion=Some("protocol_error"),
                _ => {},
            }
        } else {
            let value:Value = serde_json::from_str(raw).unwrap();
            match value["type"].as_str().unwrap() {
                "request_ack" => { let (id,risk)=pending.take().unwrap(); assert_eq!(value["request_id"],id); state.acknowledge_request(id,risk); },
                "cancel" => assert_eq!(value["control_id"],state.begin_cancel().unwrap()),
                "completion_accepted" => {assert_eq!(completion,Some("submitted"));completion=Some("accepted");},
                other => panic!("unexpected owner frame {other}"),
            }
        }
    }
    if let Some(error)=state.end_of_stream(){return Err(error.code.to_owned());}
    submitted.sort();
    Ok(json!({"operation":"migration","completion":completion.unwrap(),"risk_state":if state.risk()==RecoveryRiskState::Clear {"clear"} else {"open"},"submitted_outputs":submitted}))
}

// Test-ID: PR-TEST-0323
// Verifies: PR-REQ-0316, PR-REQ-0212
#[test]
fn production_migration_decoder_matches_unchanged_frozen_transcripts() {
    let corpus:Value=serde_json::from_str(include_str!("../../tests/vectors/hook_protocol_v1/vectors.json")).unwrap();
    let session:Value=serde_json::from_str(corpus["session_specs"].as_array().unwrap().iter().find(|s|s["name"]=="migration").unwrap()["raw_json"].as_str().unwrap()).unwrap();
    let mut counts=(0,0);
    for vector in corpus["valid"].as_array().unwrap().iter().filter(|v|v["session"]=="migration") {
        assert_eq!(transcript(&session,vector["messages"].as_array().unwrap()).unwrap(),vector["expected_state"],"{}",vector["name"]);
        counts.0+=1;
    }
    for vector in corpus["invalid"].as_array().unwrap().iter().filter(|v|v["session"]=="migration") {
        assert_eq!(transcript(&session,vector["messages"].as_array().unwrap()).unwrap_err(),vector["expected_error"].as_str().unwrap(),"{}",vector["name"]);
        counts.1+=1;
    }
    assert_eq!(counts,(1,2));
}
