// Real Frozen Migration Hooks use the same native test worker and supervisor.
use super::*;
use crate::domain::*;
const WORKER: &str = "hook::tests::migration_runtime::migration_hook_worker";

fn declaration(id: &str, required: bool, secret: bool) -> InputDeclarationV1 {
    InputDeclarationV1 { id: InputIdentity::parse(id).unwrap(), required,
        protection: if secret {InputProtectionV1::Secret} else {InputProtectionV1::Normal} }
}

fn install_edge(f: &RuntimeFixture, source: &RevisionIdentity, inputs: Vec<InputDeclarationV1>,
    transitions: Vec<MigrationTransitionV1>, with_hook: bool) -> RevisionIdentity {
    let p = persistence(&f.storage);
    let original = p.load_revision(&f.instance.active_revision).unwrap().unwrap().content;
    let hook = with_hook.then(|| HookV1 {
        protocol_version: PositiveVersion::new(1).unwrap(),
        launch: HookLaunchV1::Direct { executable: ContentId::parse("worker").unwrap() },
        args: vec!["--exact".to_owned(), WORKER.to_owned(), "--nocapture".to_owned()],
        io: IOContractV1 { terminal: TerminalContractV1::None },
    });
    let core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
        inputs, actions: vec![], snapshot: None, cleanup: None,
        migrations: vec![MigrationV1 {
            source_revision_digest: Sha256Digest::from_bytes(*source.content_digest.as_bytes()),
            transitions,
            requires_source: if with_hook { vec![InputBindingRefV1 {role:InputBindingRoleV1::Retained, input_id:InputIdentity::parse("secret_config").unwrap()}] } else {vec![]},
            requires_target: if with_hook {vec![InputIdentity::parse("request").unwrap()]} else {vec![]},
            produces_target: if with_hook {vec![InputIdentity::parse("result").unwrap()]} else {vec![]}, hook,
        }],
    }).unwrap();
    let content = validate_revision_content_v1(core, original.runtime_content).unwrap();
    let witnesses = content.runtime_content.files().iter().map(|file| {
        let path = f.temporary.path().join("source").join(file.path.as_str().rsplit('/').next().unwrap());
        p.put_runtime_content(&file.blob_digest, &mut fs::File::open(path).unwrap()).unwrap()
    }).collect::<Vec<_>>();
    p.persist_revision(source.package_id, &content, &witnesses).unwrap()
}

fn chain(f: &RuntimeFixture) -> Vec<RevisionIdentity> {
    let a = f.instance.active_revision.clone();
    let b = install_edge(f, &a, vec![], vec![], false);
    let c = install_edge(f, &b, vec![declaration("request", true, false), declaration("result", true, true), declaration("missing", true, false)], vec![], true);
    let d = install_edge(f, &c, vec![declaration("final", true, true)], vec![MigrationTransitionV1::Carry {
        source: InputBindingRefV1 {role:InputBindingRoleV1::Active, input_id:InputIdentity::parse("result").unwrap()},
        target_input_id:InputIdentity::parse("final").unwrap(),
    }], false);
    vec![a,b,c,d]
}

fn start(f: &RuntimeFixture, path: &[RevisionIdentity], mode: &str, cancel: ActionCancellation) -> RunId {
    start_with_policy(f, path, mode, cancel, policy(Some(5_000), Some(5_000), Some(10)))
}
fn start_with_policy(f: &RuntimeFixture, path: &[RevisionIdentity], mode: &str, cancel: ActionCancellation, policy: HookRuntimePolicy) -> RunId {
    let key = MigrationTargetInput {revision:path[2].content_digest, input:InputIdentity::parse("request").unwrap()};
    let plan = crate::workflow::compile_migration(&f.application, &crate::workflow::PlatformHostLauncherLookup,
        &TransitionRevision {instance:f.instance.id, expected_state_version:f.instance.state_version,
            source:path[0].clone(), target:path.last().unwrap().clone(), path:MigrationPathSelection::Exact(path.to_vec()),
            operator_inputs:vec![key.clone()], authorize_declassification:false}, &[]).unwrap();
    let request = f.marker("request.json");
    fs::write(&request, serde_json::to_vec(&json!({"mode":mode,"marker":f.marker("invoked")})).unwrap()).unwrap();
    let run = f.application.accept_migration_inputs(plan, false, cancel,
        vec![(key, fs::File::open(&request).unwrap())], policy).unwrap();
    // The original host path is never authoritative once acquisition succeeds.
    fs::remove_file(request).unwrap();
    run
}

fn finish(f: &RuntimeFixture, run: RunId) -> RunOutcomeView {
    let until = Instant::now() + WAIT_LIMIT;
    loop {
        if f.application.advance_owner_continuation(run).unwrap() { break; }
        assert!(Instant::now() < until, "Migration continuation stalled");
    }
    match persistence(&f.storage).load_managed_run(run).unwrap().unwrap().state {
        RunState::Finished(outcome) => outcome, _ => panic!("not terminal"),
    }
}

// Test-ID: PR-TEST-0312
// Verifies: PR-REQ-0315, PR-REQ-0316, PR-REQ-0306, PR-REQ-0154, PR-REQ-0161
#[test]
fn mixed_chain_materializes_full_context_and_commits_detached_inputs_and_hook_outputs() {
    let f = RuntimeFixture::new();
    let path = chain(&f);
    let run = start(&f, &path, "success", ActionCancellation::default());
    let outcome = finish(&f, run);
    assert_eq!(outcome.outcome, RunOutcome::Succeeded, "{outcome:?}");
    let p = persistence(&f.storage);
    let view = p.load_instance_by_id(f.instance.id).unwrap().unwrap();
    assert_eq!(view.active_revision, path[3]);
    assert!(view.required_inputs_satisfied);
    let mut bytes = Vec::new();
    p.export_input(f.instance.id, &InputIdentity::parse("final").unwrap(), true, &mut bytes).unwrap();
    assert_eq!(bytes, SECRET_BINDING);
    assert_eq!(fs::read(f.marker("invoked")).unwrap(), b"once\n");
    assert!(!contains(format!("{:?}",p.managed_run_inspection(run).unwrap()).as_bytes(), SECRET_BINDING));
    assert_eq!(p.managed_run_inspection(run).unwrap().unwrap().migration_progress.unwrap().committed_edges, 3);
}

// Test-ID: PR-TEST-0313
// Verifies: PR-REQ-0316, PR-REQ-0306, PR-REQ-0313, PR-REQ-0154, PR-REQ-0160, PR-REQ-0163
#[test]
fn invalid_migration_completions_and_outputs_never_publish_the_hook_edge() {
    for mode in ["failure", "open_failure", "success_open", "missing", "duplicate", "undeclared", "failure_outputs", "protection", "oversize"] {
        let f = RuntimeFixture::new();
        let path = chain(&f);
        let run = start(&f, &path, mode, ActionCancellation::default());
        let outcome = finish(&f, run);
        assert_eq!(outcome.outcome, RunOutcome::Failed, "{mode}: {outcome:?}");
        let p = persistence(&f.storage);
        assert_eq!(p.load_instance_by_id(f.instance.id).unwrap().unwrap().active_revision, path[1], "{mode}");
        let inspection = p.managed_run_inspection(run).unwrap().unwrap();
        assert_eq!(inspection.migration_progress.unwrap().committed_edges, 1);
        assert_eq!(inspection.current_recovery_guard.is_some(), matches!(mode, "open_failure" | "success_open"), "{mode}");
        assert_eq!(fs::read(f.marker("invoked")).unwrap(), b"once\n");
    }
}

// Test-ID: PR-TEST-0314
// Verifies: PR-REQ-0316, PR-REQ-0306
#[test]
fn cancelled_hook_is_terminated_before_migration_terminalization() {
    let f = RuntimeFixture::new();
    let path = chain(&f);
    let cancellation = ActionCancellation::default();
    let run = start(&f, &path, "hang", cancellation.clone());
    let cancel = cancel_after(f.marker("invoked"), &cancellation);
    assert_eq!(finish(&f, run).outcome, RunOutcome::Cancelled);
    cancel.join().unwrap();
    assert_eq!(persistence(&f.storage).load_instance_by_id(f.instance.id).unwrap().unwrap().active_revision, path[1]);
}

#[test]
fn migration_hook_worker() {
    let Ok(transport) = env::var(TRANSPORT_ENVIRONMENT) else {return;};
    let mut stream = connect_hook(&transport, &env::var(ENDPOINT_ENVIRONMENT).unwrap());
    let mut preamble = [0; PREAMBLE.len()];
    stream.read_exact(&mut preamble).unwrap();
    assert_eq!(preamble, PREAMBLE);
    let session = read_frame(&mut stream).unwrap();
    let op = &session["operation"];
    assert_eq!(op["kind"], "migration");
    assert_eq!(session["parameters"], json!([]));
    let sources = op["source_bindings"].as_array().unwrap();
    assert!(sources.windows(2).all(|pair| {
        (pair[0]["role"].as_str(),pair[0]["input_id"].as_str()) < (pair[1]["role"].as_str(),pair[1]["input_id"].as_str())
    }));
    let source = sources.iter().find(|b|b["input_id"]=="secret_config").unwrap();
    assert_eq!(source["role"], "retained");
    assert_eq!(source["input_id"], "secret_config");
    let secret = fs::read(source["readonly_path"].as_str().unwrap()).unwrap();
    assert_eq!(secret, SECRET_BINDING);
    assert_eq!(op["target_bindings"].as_array().unwrap().len(), 1);
    let target = &op["target_bindings"][0];
    assert_eq!(target["role"], "active");
    assert_eq!(target["input_id"], "request");
    let request: Value = serde_json::from_slice(&fs::read(target["readonly_path"].as_str().unwrap()).unwrap()).unwrap();
    let mode = request["mode"].as_str().unwrap();
    let previous_invocation = Path::new(request["marker"].as_str().unwrap()).exists();
    if previous_invocation {
        assert_eq!(sources.len(),3);
        assert!(sources.iter().any(|b|b["role"]=="active" && b["input_id"]=="final"));
        assert!(sources.iter().any(|b|b["role"]=="retained" && b["input_id"]=="request"));
    } else {assert_eq!(sources.len(),1);}
    fs::OpenOptions::new().create(true).append(true).open(request["marker"].as_str().unwrap()).unwrap().write_all(b"once\n").unwrap();
    stream.write_all(PREAMBLE).unwrap();
    if mode == "no_ready" { loop { thread::sleep(Duration::from_secs(1)); } }
    write_frame(&mut stream, &json!({"type":"session_ready","protocol_version":1,"session_id":session["session_id"]}));
    if mode == "hang" { loop { thread::sleep(Duration::from_secs(1)); } }
    if matches!(mode, "success" | "open_failure" | "success_open") {
        write_frame(&mut stream, &json!({"type":"request","request_id":1,"request":{"kind":"enter_recovery_risk"}}));
        assert!(read_frame(&mut stream).is_some());
        if mode == "success" {
            write_frame(&mut stream, &json!({"type":"request","request_id":2,"request":{"kind":"resolve_recovery_risk"}}));
            assert!(read_frame(&mut stream).is_some());
        }
    }
    let slot = &op["target_outputs"][0];
    if mode == "oversize" {
        fs::OpenOptions::new().write(true).open(slot["staged_path"].as_str().unwrap()).unwrap().set_len(MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1+1).unwrap();
    } else { fs::write(slot["staged_path"].as_str().unwrap(), secret).unwrap(); }
    let outputs = match mode {
        "failure" | "open_failure" | "missing" => json!([]),
        "duplicate" => json!([slot["handle"],slot["handle"]]),
        "undeclared" => json!(["00000000000000000000000000000000"]),
        _ => json!([slot["handle"]]),
    };
    let mut completion = json!({"type":"complete","operation":"migration","status":if matches!(mode,"failure"|"open_failure"|"failure_outputs") {"failure"} else {"success"},"produced_target_outputs":outputs});
    if mode == "protection" { completion["protection"] = json!("normal"); }
    write_frame(&mut stream, &completion);
    let response = read_frame(&mut stream);
    if mode == "success" { assert_eq!(response.unwrap()["type"], "completion_accepted"); }
}

// Test-ID: PR-TEST-0315
// Verifies: PR-REQ-0316, PR-REQ-0306
#[test]
fn migration_deadlines_are_per_invocation_and_preserve_the_preceding_edge() {
    for mode in ["no_ready", "hang"] {
        let f = RuntimeFixture::new();
        let path = chain(&f);
        let limits = if mode == "no_ready" {policy(Some(1_000),None,Some(10))} else {policy(None,Some(1_000),Some(10))};
        let run = start_with_policy(&f, &path, mode, ActionCancellation::default(), limits);
        assert_eq!(finish(&f, run).outcome, RunOutcome::TimedOut);
        assert_eq!(persistence(&f.storage).load_instance_by_id(f.instance.id).unwrap().unwrap().active_revision, path[1]);
    }
}

// Test-ID: PR-TEST-0316
// Verifies: PR-REQ-0316, PR-REQ-0306, PR-REQ-0314
#[test]
fn lost_hook_edge_commit_ack_is_not_a_second_hook_invocation() {
    let f = RuntimeFixture::new();
    let path = chain(&f);
    let run = start(&f, &path, "success", ActionCancellation::default());
    assert!(!f.application.advance_owner_continuation(run).unwrap()); // admission
    assert!(!f.application.advance_owner_continuation(run).unwrap()); // first declarative commit
    while f.application.managed_run_inspection(run).unwrap().unwrap().migration_progress.unwrap().step != MigrationPlanStep::AcceptCompletion {
        assert!(!f.application.advance_owner_continuation(run).unwrap());
    }
    crate::persistence::fail_next_migration_edge_ack_for_test();
    assert!(f.application.advance_owner_continuation(run).is_err());
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    assert_eq!(fs::read(f.marker("invoked")).unwrap(), b"once\n");
}

const OWNER_WORKER: &str = "hook::tests::migration_runtime::migration_owner_worker";

// Test-ID: PR-TEST-0321
// Verifies: PR-REQ-0316, PR-REQ-0306, PR-REQ-0154, PR-REQ-0155
#[test]
fn final_hook_publishes_atomic_success_even_with_incomplete_target_readiness() {
    let f = RuntimeFixture::new();
    let path = chain(&f);
    let run = start(&f, &path[..3], "success", ActionCancellation::default());
    let outcome = finish(&f, run);
    assert_eq!(outcome.outcome, RunOutcome::Succeeded, "{outcome:?}");
    assert_eq!(outcome.hook_completion.unwrap().status, HookCompletionStatus::Success);
    let p = persistence(&f.storage);
    let view = p.load_instance_by_id(f.instance.id).unwrap().unwrap();
    assert_eq!(view.active_revision,path[2]);
    assert!(!view.required_inputs_satisfied);
    assert_eq!(p.managed_run_inspection(run).unwrap().unwrap().migration_progress.unwrap().committed_edges,2);
}

// Test-ID: PR-TEST-0322
// Verifies: PR-REQ-0316, PR-REQ-0306, PR-REQ-0161
#[test]
fn multiple_hook_edges_get_independent_sessions_and_full_active_and_retained_source_views() {
    let f = RuntimeFixture::new();
    let mut path = chain(&f);
    let next = install_edge(&f,path.last().unwrap(),vec![declaration("request",true,false),declaration("result",true,true)],vec![],true);
    path.push(next);
    let run = start(&f,&path,"success",ActionCancellation::default());
    assert_eq!(finish(&f,run).outcome,RunOutcome::Succeeded);
    assert_eq!(fs::read(f.marker("invoked")).unwrap(),b"once\nonce\n");
    let p = persistence(&f.storage);
    assert_eq!(p.managed_run_inspection(run).unwrap().unwrap().migration_progress.unwrap().committed_edges,4);
    let mut bytes=Vec::new();
    p.export_input(f.instance.id,&InputIdentity::parse("result").unwrap(),true,&mut bytes).unwrap();
    assert_eq!(bytes,SECRET_BINDING);
}

#[test]
fn migration_owner_worker() {
    let Some(root) = env::var_os("PACTRUN_M5_HOOK_TEST_ROOT") else { return; };
    let root = PathBuf::from(root);
    let app = PactrunApplication::open(&root).unwrap();
    if env::var_os("PACTRUN_M5_RECONCILE").is_some() {
        app.reconcile_lost_action_owners().unwrap();
        return;
    }
    let instance = app.resolve_instance_name(&InstanceName::parse("slice4").unwrap()).unwrap().unwrap();
    let observation = app.observe_migration_compilation(instance).unwrap();
    let target = RevisionIdentity::new(observation.active_revision.package_id, env::var("PACTRUN_M5_TARGET").unwrap().parse().unwrap());
    let key = MigrationTargetInput {revision: env::var("PACTRUN_M5_INPUT_TARGET").unwrap().parse().unwrap(), input:InputIdentity::parse("request").unwrap()};
    let plan = crate::workflow::compile_migration(&app, &crate::workflow::PlatformHostLauncherLookup,
        &TransitionRevision {instance, expected_state_version:observation.state_version,
            source:observation.active_revision, target, path:MigrationPathSelection::Automatic,
            operator_inputs:vec![key.clone()], authorize_declassification:false}, &[]).unwrap();
    let run = app.accept_migration_inputs(plan, false, ActionCancellation::default(),
        vec![(key, fs::File::open(root.parent().unwrap().join("request.json")).unwrap())], policy(Some(5_000),Some(5_000),Some(10))).unwrap();
    fs::write(root.parent().unwrap().join("accepted-run"), run.to_string()).unwrap();
    while !app.advance_owner_continuation(run).unwrap() {}
}

// Test-ID: PR-TEST-0317
// Verifies: PR-REQ-0316, PR-REQ-0306, PR-REQ-0313, PR-REQ-0155, PR-REQ-0165, PR-REQ-0053, PR-REQ-0061, PR-REQ-0062
#[test]
fn mixed_chain_crashes_reconcile_in_a_new_process_without_files_or_hook_replay() {
    for (edge, fault, committed, open_risk) in [
        (0,"before_migration_edge_commit",0,false),
        (0,"after_migration_edge_commit",1,false),
        (1,"before_migration_edge_commit",1,false),
        (1,"after_migration_edge_commit",2,false),
        (2,"before_migration_edge_commit",2,false),
        (2,"after_migration_edge_commit",3,false),
        (1,"before_recovery_risk_commit",1,false),
        (1,"after_recovery_risk_commit",1,true),
    ] {
        let f = RuntimeFixture::new();
        let path = chain(&f);
        let request = f.marker("request.json");
        fs::write(&request, serde_json::to_vec(&json!({"mode":"success","marker":f.marker("invoked")})).unwrap()).unwrap();
        let status = Command::new(env::current_exe().unwrap()).args(["--exact",OWNER_WORKER,"--nocapture"])
            .env("PACTRUN_M5_HOOK_TEST_ROOT",&f.storage).env("PACTRUN_M5_TARGET",path[3].content_digest.to_string())
            .env("PACTRUN_M5_INPUT_TARGET",path[2].content_digest.to_string())
            .env("PACTRUN_M4_FAULT",fault).env("PACTRUN_MIGRATION_FAULT_EDGE",edge.to_string())
            .stdout(std::process::Stdio::null()).status().unwrap();
        assert_eq!(status.code(),Some(87),"{fault}");
        let run:RunId = fs::read_to_string(f.marker("accepted-run")).unwrap().parse().unwrap();
        fs::remove_file(request).unwrap();
        assert!(Command::new(env::current_exe().unwrap()).args(["--exact",OWNER_WORKER,"--nocapture"])
            .env("PACTRUN_M5_HOOK_TEST_ROOT",&f.storage).env("PACTRUN_M5_RECONCILE","1")
            .stdout(std::process::Stdio::null()).status().unwrap().success());
        let p = persistence(&f.storage);
        let inspection = p.managed_run_inspection(run).unwrap().unwrap();
        assert_eq!(inspection.migration_progress.unwrap().committed_edges,committed,"{fault}");
        assert_eq!(p.load_instance_by_id(f.instance.id).unwrap().unwrap().active_revision,path[committed]);
        let RunState::Finished(outcome) = inspection.run.state else {panic!("orphan stayed Running")};
        assert_eq!(outcome.outcome,if committed==3 {RunOutcome::Succeeded} else {RunOutcome::Interrupted});
        assert_eq!(inspection.current_recovery_guard.is_some(),open_risk,"{fault}");
        if edge >= 1 { assert_eq!(fs::read(f.marker("invoked")).unwrap(),b"once\n"); }
        else {assert!(!f.marker("invoked").exists());}
    }
}
