// Real version-selected operations through production installation and runtime.
use super::*;
const V2_WORKER: &str = "hook::tests::v2_runtime::v2_action_worker";

#[test]
fn v2_action_worker() {
    let Ok(transport) = env::var(TRANSPORT_ENVIRONMENT) else { return; };
    let mut stream = connect_hook(&transport, &env::var(ENDPOINT_ENVIRONMENT).unwrap());
    let mut preamble = [0; protocol::authority::PREAMBLE_V2.len()];
    stream.read_exact(&mut preamble).unwrap();
    assert_eq!(preamble, protocol::authority::PREAMBLE_V2);
    let session = read_frame(&mut stream).unwrap();
    assert_eq!(session["protocol_version"], "1.0-alpha.1");
    assert_eq!(session["operation"]["kind"], "action");
    assert!(session.get("target_commit").is_none());
    let mode = session["parameters"].as_array().unwrap().iter().find(|p| p["parameter_id"] == "mode").unwrap()["value"].as_str().unwrap();
    stream.write_all(protocol::authority::PREAMBLE_V2).unwrap();
    write_frame(&mut stream, &json!({"type":"session_ready","protocol_version":"1.0-alpha.1","session_id":session["session_id"]}));
    write_frame(&mut stream, &json!({"type":"request","request_id":1,"request":{"kind":"enter_recovery_risk"}}));
    assert_eq!(read_frame(&mut stream).unwrap(), json!({"type":"request_ack","request_id":1,"risk_state":"open"}));
    let outputs = if mode == "ordinary" {
        assert_eq!(session["service_authorities"], json!([]));
        let output = &session["operation"]["outputs"][0];
        fs::write(output["staged_path"].as_str().unwrap(), b"V2 ordinary output").unwrap();
        vec![output["handle"].clone()]
    } else {
        let grants = session["service_authorities"].as_array().unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0]["reference"], json!({"view":"current","role":"active","kind":"resource","id":"config"}));
        assert_eq!(grants[0]["mode"], "write");
        assert_eq!(grants[0]["resource_kind"], "file");
        let live = Path::new(grants[0]["path"].as_str().unwrap());
        assert!(!live.starts_with(session["workspace"]["root_path"].as_str().unwrap()));
        fs::write(live, b"service-owned V2 bytes").unwrap();
        vec![]
    };
    if mode != "open_success" {
        write_frame(&mut stream, &json!({"type":"request","request_id":2,"request":{"kind":"resolve_recovery_risk"}}));
        assert_eq!(read_frame(&mut stream).unwrap(), json!({"type":"request_ack","request_id":2,"risk_state":"clear"}));
    }
    write_frame(&mut stream, &json!({"type":"complete","operation":"action","status":"success","produced_outputs":outputs}));
    let receipt = read_frame(&mut stream).unwrap();
    assert_eq!(receipt["type"], if mode == "open_success" { "protocol_error" } else { "completion_accepted" });
}

#[test]
fn v2_snapshot_worker() {
    let Ok(transport) = env::var(TRANSPORT_ENVIRONMENT) else { return; };
    let mut stream = connect_hook(&transport, &env::var(ENDPOINT_ENVIRONMENT).unwrap());
    let mut preamble = [0; protocol::authority::PREAMBLE_V2.len()];
    stream.read_exact(&mut preamble).unwrap();
    assert_eq!(preamble, protocol::authority::PREAMBLE_V2);
    let session = read_frame(&mut stream).unwrap();
    assert_eq!(session["protocol_version"], "1.0-alpha.1");
    assert!(session.get("target_commit").is_none());
    let operation = session["operation"]["kind"].as_str().unwrap();
    let grants = session["service_authorities"].as_array().unwrap();
    assert_eq!(grants.len(), 1);
    let live = Path::new(grants[0]["path"].as_str().unwrap());
    stream.write_all(protocol::authority::PREAMBLE_V2).unwrap();
    write_frame(&mut stream, &json!({"type":"session_ready","protocol_version":"1.0-alpha.1","session_id":session["session_id"]}));
    if operation == "snapshot_capture" {
        assert_eq!(grants[0]["mode"], "read");
        let mut representation = b"snapshot-representation:".to_vec();
        representation.extend_from_slice(&fs::read(live).unwrap());
        let candidate = Path::new(session["operation"]["candidate"]["root_path"].as_str().unwrap());
        fs::write(candidate.join("chosen.bin"), representation).unwrap();
        write_frame(&mut stream, &json!({"type":"complete","operation":"snapshot_capture","status":"success", "service_content":[{"role":"backup","path":"service/config","candidate_path":"chosen.bin"}]}));
    } else {
        assert_eq!(operation, "snapshot_restore");
        assert_eq!(grants[0]["mode"], "write");
        let snapshot = &session["operation"]["snapshot_content"];
        let descriptor = &snapshot["logical_descriptors"][0];
        assert_eq!(descriptor["role"], "backup");
        assert_eq!(descriptor["path"], "service/config");
        let selected = fs::read(Path::new(snapshot["readonly_root_path"].as_str().unwrap()).join(descriptor["materialized_path"].as_str().unwrap())).unwrap();
        let bytes = selected.strip_prefix(b"snapshot-representation:").unwrap();
        write_frame(&mut stream, &json!({"type":"request","request_id":1,"request":{"kind":"enter_recovery_risk"}}));
        assert_eq!(read_frame(&mut stream).unwrap()["risk_state"], "open");
        fs::write(live, bytes).unwrap();
        write_frame(&mut stream, &json!({"type":"request","request_id":2,"request":{"kind":"resolve_recovery_risk"}}));
        assert_eq!(read_frame(&mut stream).unwrap()["risk_state"], "clear");
        write_frame(&mut stream, &json!({"type":"complete","operation":"snapshot_restore","status":"success"}));
    }
    assert_eq!(read_frame(&mut stream).unwrap()["type"], "completion_accepted");
}

#[test]
fn v2_transform_worker() {
    let Ok(transport)=env::var(TRANSPORT_ENVIRONMENT) else{return;};
    let mode=env::args().find_map(|arg|arg.strip_prefix("transform-mode:").map(str::to_owned)).unwrap_or_else(||"success".into());
    let mut stream=connect_hook(&transport,&env::var(ENDPOINT_ENVIRONMENT).unwrap());
    let mut preamble=[0;protocol::authority::PREAMBLE_V2.len()];
    stream.read_exact(&mut preamble).unwrap(); assert_eq!(preamble,protocol::authority::PREAMBLE_V2);
    let session=read_frame(&mut stream).unwrap();
    assert_eq!(session["operation"]["kind"],"migration");
    let grants=session["service_authorities"].as_array().unwrap();
    let sources=grants.iter().filter(|g|g["reference"]["view"]=="source").collect::<Vec<_>>();
    let targets=grants.iter().filter(|g|g["reference"]["view"]=="target").collect::<Vec<_>>();
    assert!(!sources.is_empty()); assert!(!targets.is_empty());
    for source in &sources { assert_eq!(source["mode"],"read"); }
    for target in &targets { assert_eq!(target["mode"],"write"); }
    stream.write_all(protocol::authority::PREAMBLE_V2).unwrap();
    write_frame(&mut stream,&json!({"type":"session_ready","protocol_version":"1.0-alpha.1","session_id":session["session_id"]}));
    write_frame(&mut stream,&json!({"type":"request","request_id":1,"request":{"kind":"enter_recovery_risk"}}));
    assert_eq!(read_frame(&mut stream).unwrap()["risk_state"],"open");
    let mut bytes=b"target:".to_vec();
    for source in &sources { bytes.extend_from_slice(&fs::read(source["path"].as_str().unwrap()).unwrap()); }
    for target in &targets {
        if target["resource_kind"] == "directory" {
            assert!(!Path::new(target["path"].as_str().unwrap()).exists(), "Pactrun must not create the service directory");
            fs::create_dir(target["path"].as_str().unwrap()).unwrap();
        }
    }
    for target in &targets {
        if target["resource_kind"] == "file" { fs::write(target["path"].as_str().unwrap(),&bytes).unwrap(); }
    }
    if mode=="failure" {
        write_frame(&mut stream,&json!({"type":"complete","operation":"migration","status":"failure","produced_target_outputs":[]}));
        assert_eq!(read_frame(&mut stream).unwrap()["type"],"completion_accepted"); return;
    }
    let mut outputs=Vec::new();
    for slot in session["operation"]["target_outputs"].as_array().unwrap() {
        if mode == "bad_output" { fs::create_dir(slot["staged_path"].as_str().unwrap()).unwrap(); }
        else { fs::write(slot["staged_path"].as_str().unwrap(),b"Pactrun-owned transform output").unwrap(); }
        outputs.push(slot["handle"].as_str().unwrap().to_owned());
    }
    outputs.sort();
    write_frame(&mut stream,&json!({"type":"target_ready","commit_handle":session["target_commit"]["handle"],"produced_target_outputs":outputs}));
    let receipt=read_frame(&mut stream).unwrap(); assert_eq!(receipt["type"],"target_ready_received"); assert!(receipt.get("status").is_none());
    if mode=="linger_receipt" {
        fs::write(Path::new(targets[0]["path"].as_str().unwrap()).with_file_name("receipt-observed"),b"receipt only; not committed").unwrap();
        thread::sleep(Duration::from_secs(30));
    }
    if mode=="extra" {write_frame(&mut stream,&json!({"type":"diagnostic","severity":"info","code":"late","message":"not allowed"}));}
    if mode=="nonzero" {std::process::exit(12);}
}

// Test-ID: PR-TEST-0369
// Verifies: PR-REQ-0077, PR-REQ-0173, PR-REQ-0240, PR-REQ-0321
#[test]
fn real_core_v1_action_selects_v2_with_unchanged_common_authority_and_durable_risk_ordering() {
    let fixture = RuntimeFixture::with_source(|_, _, manifest| {
        *manifest = manifest.to_owned().replace(WORKER_TEST, V2_WORKER);
    });
    let admitted = fixture.admit("direct", "ordinary", &fixture.marker("unused"));
    let run = admitted.run();
    fixture.application.execute_admitted_action(admitted, policy(None, None, None), ActionCancellation::default());
    assert_eq!(running_risk(&fixture.storage, run), RecoveryRiskState::Clear);
    assert!(fixture.application.advance_owner_continuation(run).unwrap());
    assert!(matches!(load_run(&fixture.storage, run).state, RunState::Finished(outcome) if outcome.outcome == RunOutcome::Succeeded));
    assert_eq!(persistence(&fixture.storage).load_revision(&fixture.instance.active_revision).unwrap().unwrap().content.core.version(), crate::domain::VersionDomain::Revision.current());
    assert!(!fixture.storage.join("service-storage").exists());
}

// Test-ID: PR-TEST-0385
// Verifies: PR-REQ-0077, PR-REQ-0240, PR-REQ-0321
#[test]
fn installed_core_v2_can_execute_unchanged_v1_hook_without_service_authority() {
    let fixture = RuntimeFixture::with_source(|_, _, manifest| {
        *manifest = manifest.to_owned();
    });
    let admitted = fixture.admit("direct", "success", &fixture.marker("unused"));
    let run = admitted.run();
    fixture.application.execute_admitted_action(admitted, policy(None, None, None), ActionCancellation::default());
    assert!(fixture.application.advance_owner_continuation(run).unwrap());
    assert!(matches!(load_run(&fixture.storage, run).state, RunState::Finished(outcome) if outcome.outcome == RunOutcome::Succeeded));
    assert_eq!(persistence(&fixture.storage).load_revision(&fixture.instance.active_revision).unwrap().unwrap().content.core.version(), crate::domain::VersionDomain::Revision.current());
    assert!(!fixture.storage.join("service-storage").exists());
}
