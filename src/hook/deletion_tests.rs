use super::*;
use crate::domain::*;
const WORKER: &str = "hook::tests::deletion_runtime::cleanup_hook_worker";
const OWNER_WORKER: &str = "hook::tests::deletion_runtime::deletion_owner_worker";

fn fixture(mode: &str, version: u8) -> RuntimeFixture {
    RuntimeFixture::with_source(|source, _, manifest| {
        let args = serde_json::to_string(&vec!["--exact".to_owned(),WORKER.to_owned(),"--nocapture".to_owned(),"--skip".to_owned(),
            format!("cleanup-mode:{mode}"),"--skip".to_owned(),format!("cleanup-marker:{}",source.parent().unwrap().join("cleanup-count").display())]).unwrap();
        *manifest = manifest.replace("  migrations: []", &format!(r#"  cleanup:
    requires: [{{ role: active, input_id: secret_config }}]
    hook:
      protocol_version: {version}
      launch: {{ kind: direct, executable: worker }}
      args: {args}
      io: {{ terminal: none }}
  migrations: []"#));
        if mode == "retained" {
            *manifest = manifest.replace("requires: [{ role: active, input_id: secret_config }]", "requires: [{ role: retained, input_id: legacy }]");
        }
        if mode == "service" {
            *manifest = manifest.replace("source_format: 1", "source_format: 2")
                .replace("revision:\n", "revision:\n  service_storages: [{id: state}]\n  service_resources: [{id: config, storage_id: state, locator: config.json, kind: file}]\n")
                .replace("protocol_version: 2\n      launch:", "protocol_version: 2\n      service_access: [{reference: {view: current, role: active, kind: resource, id: config}, mode: read}]\n      service_requires: [{reference: {view: current, role: active, kind: resource, id: config}, presence: present}]\n      launch:");
        }
    })
}
fn start(f: &RuntimeFixture, mode: DeletionMode) -> RunId {
    let intent = f.application.resolve_deletion(&f.instance.name, None, mode).unwrap();
    let plan = f.application.compile_deletion(&intent, &[]).unwrap();
    f.application.accept_deletion_plan(plan, AdmissionOptions::default(), ActionCancellation::default(), policy(Some(5000),Some(5000),Some(20))).unwrap()
}
fn finish(f: &RuntimeFixture, run: RunId) -> crate::domain::RunOutcomeView {
    let deadline = Instant::now() + WAIT_LIMIT;
    loop {
        if f.application.advance_owner_continuation(run).unwrap() { break; }
        f.application.execute_ready_deletion(run).unwrap();
        assert!(Instant::now() < deadline, "deletion owner stalled");
    }
    match persistence(&f.storage).load_managed_run(run).unwrap().unwrap().state {
        RunState::Finished(outcome) => outcome,
        _ => panic!("not terminal"),
    }
}

// Test-ID: PR-TEST-0404
// Verifies: PR-REQ-0111, PR-REQ-0176, PR-REQ-0177, PR-REQ-0334
#[test]
fn real_cleanup_v1_and_v2_complete_before_instance_removal_with_retained_history() {
    for version in [1,2] {
        let f = fixture("success", version);
        let run = start(&f, DeletionMode::ManagedCleanup);
        assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
        assert!(f.application.load_instance(f.instance.id).unwrap().is_none());
        assert_eq!(fs::read(f.marker("cleanup-count")).unwrap(), b"x");
        let p = persistence(&f.storage);
        assert_eq!(p.list_managed_runs(f.instance.id).unwrap().len(), 1);
        assert!(p.managed_run_inspection(run).unwrap().is_some());
        assert!(matches!(p.load_managed_run(run).unwrap().unwrap().operation,
            ManagedRunIdentity::Deletion { mode: DeletionMode::ManagedCleanup, .. }));
    }
}

// Test-ID: PR-TEST-0424
// Verifies: PR-REQ-0177, PR-REQ-0178
#[test]
fn cleanup_receives_required_retained_secret_without_inheriting_active_readiness() {
    for version in [1, 2] {
        let f = fixture("retained", version);
        let intent = f.application.resolve_deletion(&f.instance.name, None, DeletionMode::ManagedCleanup).unwrap();
        assert!(matches!(f.application.compile_deletion(&intent, &[]), Err(crate::application::ApplicationError::DeletionCompilation(DeletionPlanError::MissingRequirement(_)))));
        // Seed the same valid detached binding shape used by Migration: this
        // fixture tests Cleanup context, not the already-covered migration path.
        let db = rusqlite::Connection::open(f.storage.join("database/pactrun.sqlite3")).unwrap();
        db.execute("INSERT INTO managed_input_bindings(instance_id,input_identity,payload_id) SELECT instance_id,?2,payload_id FROM managed_input_bindings WHERE instance_id=?1 AND input_identity=?3", rusqlite::params![f.instance.id.as_bytes().as_slice(), b"legacy".as_slice(), b"secret_config".as_slice()]).unwrap();
        db.execute("DELETE FROM managed_input_bindings WHERE instance_id=?1 AND input_identity=?2", rusqlite::params![f.instance.id.as_bytes().as_slice(), b"secret_config".as_slice()]).unwrap();
        drop(db);
        assert!(!f.application.load_instance(f.instance.id).unwrap().unwrap().required_inputs_satisfied);
        let run = start(&f, DeletionMode::ManagedCleanup);
        assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
        assert_eq!(fs::read(f.marker("cleanup-count")).unwrap(), b"x");
    }
}

// Test-ID: PR-TEST-0405
// Verifies: PR-REQ-0112, PR-REQ-0179, PR-REQ-0334
#[test]
fn real_cleanup_failure_risk_and_ambiguous_loss_never_remove_the_instance() {
    for mode in ["failure", "open_failure", "open_success", "lost_completion"] {
        let f = fixture(mode, 1);
        let run = start(&f, DeletionMode::ManagedCleanup);
        assert_eq!(finish(&f, run).outcome, RunOutcome::Failed, "{mode}");
        assert!(f.application.load_instance(f.instance.id).unwrap().is_some());
        assert_eq!(fs::read(f.marker("cleanup-count")).unwrap(), b"x");
        let p = persistence(&f.storage);
        assert_eq!(p.load_instance_recovery_guard(f.instance.id).unwrap().is_some(), mode.starts_with("open_"));
        assert_eq!(p.deletion_obligation(f.instance.id).unwrap().is_some(), matches!(mode, "open_success" | "lost_completion"));
    }
}

// Test-ID: PR-TEST-0426
// Verifies: PR-REQ-0176, PR-REQ-0321, PR-REQ-0336
#[test]
fn v2_cleanup_qualifies_live_service_prerequisites_before_launch_and_ends_storage_after_completion() {
    let f = fixture("service", 2);
    let p = persistence(&f.storage);
    let allocation = p.load_instance_service_state(f.instance.id).unwrap().unwrap().storages[0].allocation;
    let path = f.storage.join("service-storage").join(format!("alloc-{allocation}"));
    let missing = start(&f, DeletionMode::ManagedCleanup);
    assert_eq!(finish(&f, missing).outcome, RunOutcome::Failed);
    assert!(!f.marker("cleanup-count").exists());
    assert!(p.deletion_obligation(f.instance.id).unwrap().is_none());
    assert!(path.exists());
    fs::write(path.join("config.json"), b"live config").unwrap();
    let run = start(&f, DeletionMode::ManagedCleanup);
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    assert_eq!(fs::read(f.marker("cleanup-count")).unwrap(), b"x");
    assert!(!path.exists());
    assert!(p.load_instance_by_id(f.instance.id).unwrap().is_none());
}

// Test-ID: PR-TEST-0406
// Verifies: PR-REQ-0113, PR-REQ-0181, PR-REQ-0336
#[test]
fn abandon_skips_declared_cleanup_but_retains_distinct_run_history() {
    let f = fixture("success", 1);
    let run = start(&f, DeletionMode::AbandonManagement);
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    assert!(!f.marker("cleanup-count").exists());
    assert!(f.application.load_instance(f.instance.id).unwrap().is_none());
    assert!(matches!(persistence(&f.storage).load_managed_run(run).unwrap().unwrap().operation,
        ManagedRunIdentity::Deletion { mode: DeletionMode::AbandonManagement, .. }));
}

// Test-ID: PR-TEST-0447
// Verifies: PR-REQ-0334, PR-REQ-0336
#[test]
fn unresolved_cleanup_blocks_ordinary_mutation_even_with_recovery_override() {
    let f=fixture("lost_completion",1);
    let cleanup=start(&f,DeletionMode::ManagedCleanup);
    assert_eq!(finish(&f,cleanup).outcome,RunOutcome::Failed);
    let before=f.application.load_instance(f.instance.id).unwrap().unwrap();
    assert!(f.application.set_input(f.instance.id,InputIdentity::parse("secret_config").unwrap(),before.state_version,Box::new(std::io::Cursor::new(b"not committed".to_vec()))).is_err());
    let intent=f.application.resolve_action(&f.instance.name,&ActionIdentity::parse("direct").unwrap(),parameters("success",&f.marker("blocked-action"))).unwrap();
    let plan=f.application.compile_action(&intent,&[]).unwrap();
    let refusal=f.application.accept_and_admit_action(&plan,AdmissionOptions {recovery_override:true});
    assert!(matches!(refusal,Err(crate::application::ApplicationError::Execution(crate::executor::ExecutorError::Refused { .. }))));
    assert!(f.application.resolve_manual_recovery(f.instance.id,before.state_version).is_err());
    assert_eq!(f.application.load_instance(f.instance.id).unwrap().unwrap(),before);
    assert_eq!(fs::read(f.marker("cleanup-count")).unwrap(),b"x");
    assert!(!f.marker("blocked-action").exists());
    assert_eq!(f.application.inspect_deletion(f.instance.id).unwrap().unwrap().attempt,cleanup);
}

// Test-ID: PR-TEST-0418
// Verifies: PR-REQ-0177, PR-REQ-0178, PR-REQ-0181, PR-REQ-0340
#[test]
fn missing_cleanup_requirement_refuses_cli_before_run_or_hook_and_abandon_remains_available() {
    let mut f = fixture("success", 1);
    f.instance = persistence(&f.storage).create_instance(InstanceName::parse("missing-cleanup-input").unwrap(), f.instance.active_revision.clone(), &mut []).unwrap();
    let version = f.instance.state_version;
    for plan in [true, false] {
        let mut args = vec!["instance", "delete", f.instance.name.as_str()];
        if plan { args.push("--plan"); }
        let mut out = vec![];
        let mut errors = vec![];
        let code = crate::cli::run(args.into_iter().map(Into::into).collect(), Some(f.storage.as_os_str().to_owned()), &mut std::io::empty(), &mut out, &mut errors);
        assert_ne!(code, 0);
        assert!(String::from_utf8(errors).unwrap().contains("Cleanup requirements are missing"));
        assert!(!f.marker("cleanup-count").exists());
        assert!(persistence(&f.storage).list_managed_runs(f.instance.id).unwrap().is_empty());
        assert_eq!(f.application.load_instance(f.instance.id).unwrap().unwrap().state_version, version);
    }
    let run = start(&f, DeletionMode::AbandonManagement);
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    assert!(!f.marker("cleanup-count").exists());
}

// Test-ID: PR-TEST-0423
// Verifies: PR-REQ-0065, PR-REQ-0074, PR-REQ-0336
#[test]
fn retirement_waits_for_admitted_observers_without_losing_owner_or_pins() {
    for mode in [DeletionMode::ManagedCleanup, DeletionMode::AbandonManagement] {
        let f = RuntimeFixture::new();
        let observed = f.admit("direct", "success", &f.marker("observer"));
        let observation = observed.run();
        let deletion = start(&f, mode);
        assert!(!f.application.advance_owner_continuation(deletion).unwrap());
        assert!(!f.application.advance_owner_continuation(deletion).unwrap());
        let p = persistence(&f.storage);
        assert!(p.load_instance_by_id(f.instance.id).unwrap().is_some());
        assert!(matches!(p.load_managed_run(deletion).unwrap().unwrap().state, RunState::Running(_)));
        assert!(matches!(p.load_managed_run(observation).unwrap().unwrap().state, RunState::Running(_)));
        f.application.execute_admitted_action(observed, policy(Some(5000),Some(5000),Some(100)), ActionCancellation::default());
        assert!(f.application.advance_owner_continuation(observation).unwrap());
        assert_eq!(finish(&f, deletion).outcome, RunOutcome::Succeeded);
        assert!(p.load_instance_by_id(f.instance.id).unwrap().is_none());
        assert!(matches!(p.load_managed_run(observation).unwrap().unwrap().state, RunState::Finished(outcome) if outcome.outcome == RunOutcome::Succeeded));
    }
}

#[test]
fn deletion_owner_worker() {
    let Some(root) = env::var_os("PACTRUN_M7_OWNER_ROOT") else { return; };
    let app = PactrunApplication::open(PathBuf::from(root)).unwrap();
    let intent = app.resolve_deletion(&InstanceName::parse("slice4").unwrap(), None, DeletionMode::ManagedCleanup).unwrap();
    let plan = app.compile_deletion(&intent, &[]).unwrap();
    let run = app.accept_deletion_plan(plan, AdmissionOptions::default(), ActionCancellation::default(), policy(Some(5000),Some(5000),Some(20))).unwrap();
    let deadline = Instant::now() + WAIT_LIMIT;
    loop {
        if app.advance_owner_continuation(run).unwrap() { break; }
        app.execute_ready_deletion(run).unwrap();
        assert!(Instant::now() < deadline);
    }
}

// Test-ID: PR-TEST-0425
// Verifies: PR-REQ-0036, PR-REQ-0179, PR-REQ-0180, PR-REQ-0334
#[test]
fn cleanup_cancellation_distinguishes_no_launch_from_ambiguous_service_execution() {
    for stage in ["accepted", "admitted", "launch_gate", "hang_clear", "open_hang"] {
        let f = fixture(stage, 1);
        let cancellation = ActionCancellation::default();
        let intent = f.application.resolve_deletion(&f.instance.name, None, DeletionMode::ManagedCleanup).unwrap();
        let plan = f.application.compile_deletion(&intent, &[]).unwrap();
        let run = f.application.accept_deletion_plan(plan, AdmissionOptions::default(), cancellation.clone(), policy(Some(5000),Some(15000),Some(20))).unwrap();
        if stage != "accepted" { assert!(!f.application.advance_owner_continuation(run).unwrap()); }
        if matches!(stage, "hang_clear" | "open_hang") {
            thread::scope(|scope| {
                let worker = scope.spawn(|| f.application.execute_ready_deletion(run).unwrap());
                let deadline = Instant::now() + WAIT_LIMIT;
                while !f.marker("cleanup-count").with_extension("ready").exists() {
                    assert!(Instant::now() < deadline, "Cleanup failed to reach cancellation boundary");
                    thread::sleep(Duration::from_millis(10));
                }
                cancellation.request();
                assert!(worker.join().unwrap());
            });
        } else {
            cancellation.request();
            if stage == "launch_gate" { assert!(f.application.execute_ready_deletion(run).unwrap()); }
        }
        assert_eq!(finish(&f, run).outcome, RunOutcome::Cancelled, "{stage}");
        let p = persistence(&f.storage);
        assert!(p.load_instance_by_id(f.instance.id).unwrap().is_some());
        if matches!(stage, "hang_clear" | "open_hang") {
            assert_eq!(fs::read(f.marker("cleanup-count")).unwrap(), b"x");
            assert_eq!(p.deletion_obligation(f.instance.id).unwrap().unwrap().phase, DeletionPhase::ResultUnresolved);
            assert_eq!(p.load_instance_recovery_guard(f.instance.id).unwrap().is_some(), stage == "open_hang");
            let intent = f.application.resolve_deletion(&f.instance.name, None, DeletionMode::ManagedCleanup).unwrap();
            assert!(f.application.compile_deletion(&intent, &[]).is_err());
        } else {
            assert!(!f.marker("cleanup-count").exists());
            assert!(p.deletion_obligation(f.instance.id).unwrap().is_none());
            assert!(p.load_instance_recovery_guard(f.instance.id).unwrap().is_none());
        }
    }
}

// Test-ID: PR-TEST-0407
// Verifies: PR-REQ-0334, PR-REQ-0335, PR-REQ-0247, PR-REQ-0180, PR-REQ-0340
#[test]
fn cleanup_boundary_crashes_require_confirmation_or_finalization_only_never_hook_replay() {
    for (point, phase) in [
        ("before_cleanup_boundary_commit", DeletionPhase::ResultUnresolved),
        ("after_cleanup_boundary_commit", DeletionPhase::FinalizationAuthorized),
    ] {
        let f = fixture("success", 1);
        let status = Command::new(env::current_exe().unwrap())
            .args(["--exact", OWNER_WORKER, "--nocapture"])
            .env("PACTRUN_M7_OWNER_ROOT", &f.storage)
            .env("PACTRUN_M4_FAULT", point)
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .status().unwrap();
        assert_eq!(status.code(), Some(87), "{point}");
        let p = persistence(&f.storage);
        let old_run = p.list_managed_runs(f.instance.id).unwrap()[0].id;
        assert_eq!(fs::read(f.marker("cleanup-count")).unwrap(), b"x");
        assert_eq!(f.application.reconcile_lost_managed_owners().unwrap(), vec![old_run]);
        let obligation = f.application.inspect_deletion(f.instance.id).unwrap().unwrap();
        assert_eq!(obligation.phase, phase);
        if phase == DeletionPhase::ResultUnresolved {
            let intent = f.application.resolve_deletion(&f.instance.name, None, DeletionMode::ManagedCleanup).unwrap();
            assert!(f.application.compile_deletion(&intent, &[]).is_err());
            let current = f.application.load_instance(f.instance.id).unwrap().unwrap();
            let command = |version: InstanceStateVersion| {
                let args = ["instance".to_owned(), "deletion".to_owned(), "confirm-complete".to_owned(), f.instance.id.to_string(),
                    "--attempt".to_owned(), old_run.to_string(), "--if-version".to_owned(), version.to_string(), "--assert-cleanup-complete".to_owned()];
                let mut output = vec![];
                let mut errors = vec![];
                let code = crate::cli::run(args.into_iter().map(Into::into).collect(), Some(f.storage.as_os_str().to_owned()), &mut std::io::empty(), &mut output, &mut errors);
                (code, String::from_utf8(output).unwrap(), String::from_utf8(errors).unwrap())
            };
            let (code, _, _) = command(InstanceStateVersion::from_bytes([91; 16]));
            assert_ne!(code, 0);
            assert_eq!(f.application.inspect_deletion(f.instance.id).unwrap().unwrap().phase, DeletionPhase::ResultUnresolved);
            let (code, output, errors) = command(current.state_version);
            assert_eq!(code, 0, "{errors}");
            assert!(output.contains("operator_confirmed"));
            assert!(output.contains("trust guard are unchanged"));
            assert_ne!(f.application.load_instance(f.instance.id).unwrap().unwrap().state_version, current.state_version);
            assert_ne!(command(current.state_version).0, 0);
        }
        let retry = start(&f, DeletionMode::ManagedCleanup);
        assert_ne!(retry, old_run);
        assert_eq!(finish(&f, retry).outcome, RunOutcome::Succeeded);
        assert_eq!(fs::read(f.marker("cleanup-count")).unwrap(), b"x");
        assert!(matches!(p.load_managed_run(old_run).unwrap().unwrap().state, RunState::Finished(outcome) if outcome.outcome == RunOutcome::Interrupted));
        assert!(f.application.load_instance(f.instance.id).unwrap().is_none());
    }
}

#[test]
fn cleanup_hook_worker() {
    let Some(transport) = env::var_os(TRANSPORT_ENVIRONMENT) else { return; };
    let mode = env::args().find_map(|arg| arg.strip_prefix("cleanup-mode:").map(str::to_owned)).unwrap();
    let marker = env::args().find_map(|arg| arg.strip_prefix("cleanup-marker:").map(PathBuf::from)).unwrap();
    let mut stream = connect_hook(&transport.into_string().unwrap(), &env::var(ENDPOINT_ENVIRONMENT).unwrap());
    let mut preamble = vec![0; PREAMBLE.len()];
    stream.read_exact(&mut preamble).unwrap();
    assert_eq!(&preamble[..preamble.len()-1], &PREAMBLE[..PREAMBLE.len()-1]);
    let session = read_frame(&mut stream).unwrap();
    assert_eq!(session["operation"]["kind"], "cleanup");
    assert!(session["parameters"].as_array().unwrap().is_empty());
    assert!(session["operation"].get("outputs").is_none());
    let bindings = session["operation"]["bindings"].as_array().unwrap();
    let required = if mode == "retained" { "legacy" } else { "secret_config" };
    let binding = bindings.iter().find(|b| b["input_id"] == required).unwrap();
    if mode == "retained" {
        assert_eq!(binding["role"], "retained");
        assert!(bindings.iter().all(|b| b["input_id"] != "secret_config"));
    }
    assert_eq!(fs::read(binding["readonly_path"].as_str().unwrap()).unwrap(), SECRET_BINDING);
    if mode == "service" {
        let grants = session["service_authorities"].as_array().unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0]["reference"], json!({"view":"current","role":"active","kind":"resource","id":"config"}));
        assert_eq!(grants[0]["mode"], "read");
        assert_eq!(grants[0]["resource_kind"], "file");
        let live = Path::new(grants[0]["path"].as_str().unwrap());
        assert!(!live.starts_with(session["workspace"]["root_path"].as_str().unwrap()));
        assert_eq!(fs::read(live).unwrap(), b"live config");
    }
    stream.write_all(&preamble).unwrap();
    write_frame(&mut stream, &json!({"type":"session_ready","protocol_version":session["protocol_version"],"session_id":session["session_id"]}));
    let mut count = fs::OpenOptions::new().create(true).append(true).open(&marker).unwrap();
    count.write_all(b"x").unwrap(); count.sync_all().unwrap();
    if mode == "lost_completion" { return; }
    if mode.starts_with("open_") {
        write_frame(&mut stream, &json!({"type":"request","request_id":1,"request":{"kind":"enter_recovery_risk"}}));
        assert_eq!(read_frame(&mut stream).unwrap()["risk_state"], "open");
    }
    if matches!(mode.as_str(), "hang_clear" | "open_hang") {
        fs::write(marker.with_extension("ready"), b"ready").unwrap();
        thread::sleep(Duration::from_secs(60));
        return;
    }
    let status = if mode.ends_with("failure") {"failure"} else {"success"};
    write_frame(&mut stream, &json!({"type":"complete","operation":"cleanup","status":status}));
    if mode != "open_success" { assert_eq!(read_frame(&mut stream).unwrap()["type"], "completion_accepted"); }
}
