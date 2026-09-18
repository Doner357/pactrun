// Managed S5 Capture tests: real Hooks, real candidate files and real publication.
use super::*;
use crate::domain::*;
use sha2::Digest;
use std::time::UNIX_EPOCH;
const CAPTURE_WORKER: &str = "hook::tests::capture_runtime::capture_hook_worker";
const SERVICE: &[u8] = b"captured-service-content";

fn fixture(access: &str) -> RuntimeFixture {
    RuntimeFixture::with_source(|_, _, manifest| {
        *manifest = manifest.replace(
            "  actions:",
            "    - { id: optional, required: false, protection: normal }\n  actions:",
        );
        *manifest = manifest.replace(
            "  migrations: []",
            &format!(
                r#"  snapshot:
    capture:
      access: {access}
      parameters:
        - {{ id: mode, type: string, sensitive: false }}
        - {{ id: marker, type: string, sensitive: false }}
        - {{ id: expected_binding, type: string, sensitive: true }}
        - {{ id: sensitive_value, type: string, sensitive: true }}
      hook:
        protocol_version: 1
        launch: {{ kind: direct, executable: worker }}
        args: ["--exact", "{CAPTURE_WORKER}", "--nocapture"]
        io: {{ terminal: none }}
  migrations: []"#
            ),
        );
    })
}

// Test-ID: PR-TEST-0329
// Verifies: PR-REQ-0067, PR-REQ-0068, PR-REQ-0070
#[test]
fn successful_action_and_capture_overrides_preserve_the_existing_trust_guard() {
    let f = fixture("observe");
    let first = f.admit("direct", "risk_open_failure", &f.marker("guard"));
    let first_run = first.run();
    f.application.execute_admitted_action(
        first,
        policy(None, None, None),
        ActionCancellation::default(),
    );
    assert!(f.application.advance_owner_continuation(first_run).unwrap());
    let p = persistence(&f.storage);
    let guard = p
        .load_instance_recovery_guard(f.instance.id)
        .unwrap()
        .unwrap();
    let guarded_version = f
        .application
        .load_instance(f.instance.id)
        .unwrap()
        .unwrap()
        .state_version;
    let intent = f
        .application
        .resolve_action(
            &InstanceName::parse("slice4").unwrap(),
            &ActionIdentity::parse("direct").unwrap(),
            parameters("success", &f.marker("action-override")),
        )
        .unwrap();
    let action_plan = f
        .application
        .compile_action(&intent, std::slice::from_ref(&f.launcher))
        .unwrap();
    let admitted = f
        .application
        .accept_and_admit_action(
            &action_plan,
            AdmissionOptions {
                recovery_override: true,
            },
        )
        .unwrap();
    let action_run = admitted.run();
    f.application.execute_admitted_action(
        admitted,
        policy(None, None, None),
        ActionCancellation::default(),
    );
    assert!(
        f.application
            .advance_owner_continuation(action_run)
            .unwrap()
    );
    assert!(
        matches!(load_run(&f.storage, action_run).state, RunState::Finished(o) if o.outcome == RunOutcome::Succeeded)
    );
    assert_eq!(
        p.load_instance_recovery_guard(f.instance.id).unwrap(),
        Some(guard.clone())
    );
    let plan = crate::workflow::compile_snapshot(
        &f.application,
        &crate::workflow::PlatformHostLauncherLookup,
        &SnapshotIntent {
            instance: f.instance.id,
            operation: SnapshotOperation::Capture,
            parameters: parameters("success", &f.marker("capture-override")),
        },
        &[],
    )
    .unwrap();
    let capture_run = f
        .application
        .accept_snapshot_plan(
            plan,
            AdmissionOptions {
                recovery_override: true,
            },
            ActionCancellation::default(),
        )
        .unwrap();
    assert!(
        !f.application
            .advance_owner_continuation(capture_run)
            .unwrap()
    );
    f.application
        .execute_admitted_capture(capture_run, policy(None, None, None))
        .unwrap();
    assert_eq!(finish(&f, capture_run).outcome, RunOutcome::Succeeded);
    assert_eq!(
        p.load_instance_recovery_guard(f.instance.id).unwrap(),
        Some(guard)
    );
    assert_eq!(
        f.application
            .load_instance(f.instance.id)
            .unwrap()
            .unwrap()
            .state_version,
        guarded_version
    );
    // The bypass is invocation-local; a later ordinary admission is blocked.
    let ordinary = f
        .application
        .accept_and_admit_action(&action_plan, AdmissionOptions::default());
    assert!(ordinary.is_err());
}
fn accept_capture(
    f: &RuntimeFixture,
    mode: &str,
    marker: &Path,
    cancellation: ActionCancellation,
) -> RunId {
    accept_capture_expected(f, mode, marker, cancellation, SECRET_BINDING)
}
fn accept_capture_expected(
    f: &RuntimeFixture,
    mode: &str,
    marker: &Path,
    cancellation: ActionCancellation,
    expected: &[u8],
) -> RunId {
    let mut parameters = parameters(mode, marker);
    parameters
        .iter_mut()
        .find(|p| p.id.as_str() == "expected_binding")
        .unwrap()
        .text = String::from_utf8(expected.to_vec()).unwrap();
    let plan = crate::workflow::compile_snapshot(
        &f.application,
        &crate::workflow::PlatformHostLauncherLookup,
        &SnapshotIntent {
            instance: f.instance.id,
            operation: SnapshotOperation::Capture,
            parameters,
        },
        &[],
    )
    .unwrap();
    let run = f
        .application
        .accept_snapshot_plan(plan, AdmissionOptions::default(), cancellation)
        .unwrap();
    assert!(!f.application.advance_owner_continuation(run).unwrap());
    run
}
pub(super) fn finish(f: &RuntimeFixture, run: RunId) -> RunOutcomeView {
    let deadline = Instant::now() + WAIT_LIMIT;
    loop {
        if f.application.advance_owner_continuation(run).unwrap() {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    match persistence(&f.storage)
        .load_managed_run(run)
        .unwrap()
        .unwrap()
        .state
    {
        RunState::Finished(outcome) => outcome,
        _ => panic!("not terminal"),
    }
}
fn db(f: &RuntimeFixture) -> rusqlite::Connection {
    rusqlite::Connection::open(f.storage.join("database/pactrun.sqlite3")).unwrap()
}
fn result_id(f: &RuntimeFixture, run: RunId) -> SnapshotId {
    SnapshotId::from_bytes(
        db(f)
            .query_row(
                "SELECT snapshot_id FROM run_capture_results WHERE run_id=?1",
                [run.as_bytes().as_slice()],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .unwrap()
            .try_into()
            .unwrap(),
    )
}
fn manifest(
    f: &RuntimeFixture,
    id: SnapshotId,
) -> crate::snapshot_integrity::VerifiedSnapshotManifest {
    let raw = db(f)
        .query_row(
            "SELECT canonical_manifest FROM snapshots WHERE snapshot_id=?1",
            [id.as_bytes().as_slice()],
            |r| r.get::<_, Vec<u8>>(0),
        )
        .unwrap();
    crate::snapshot_integrity::decode_snapshot_manifest(SnapshotIntegrityVersion::V2, &raw, None)
        .unwrap()
}

// Test-ID: PR-TEST-0273
// Verifies: PR-REQ-0301, PR-REQ-0290, PR-REQ-0289
#[test]
fn snapshot_cli_keeps_its_owner_when_admission_retries_and_diagnostic_output_breaks() {
    let f = fixture("observe");
    let marker = f.marker("cli-retry");
    let expected = f.marker("expected");
    let sensitive = f.marker("sensitive");
    fs::write(&expected, SECRET_BINDING).unwrap();
    fs::write(&sensitive, SENSITIVE_PARAMETER).unwrap();
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let args = vec![
        "snapshot".into(),
        "capture".into(),
        "slice4".into(),
        "--param".into(),
        "mode=success".into(),
        "--param".into(),
        format!("marker={}", marker.display()).into(),
        "--param-file".into(),
        format!("expected_binding={}", expected.display()).into(),
        "--param-file".into(),
        format!("sensitive_value={}", sensitive.display()).into(),
    ];
    let reads = super::super::capture::test_clock(None);
    crate::application::fail_next_finalization_advances_for_test(3);
    let mut output = Vec::new();
    assert_eq!(
        crate::cli::run(
            args,
            Some(f.storage.as_os_str().to_owned()),
            &mut io::empty(),
            &mut output,
            &mut Broken
        ),
        0
    );
    assert_eq!(super::super::capture::test_clock(None), reads + 1);
    let runs = persistence(&f.storage)
        .list_managed_runs(f.instance.id)
        .unwrap();
    assert_eq!(runs.len(), 1);
    assert!(
        matches!(&runs[0].state,RunState::Finished(outcome) if outcome.outcome==RunOutcome::Succeeded)
    );
    assert_eq!(
        String::from_utf8(output).unwrap().trim(),
        format!("snapshot: {}", result_id(&f, runs[0].id))
    );
}

// Test-ID: PR-TEST-0275
// Verifies: PR-REQ-0103, PR-REQ-0298, PR-REQ-0290
#[test]
fn published_capture_outlives_its_actual_creator_origin_and_producer_installation() {
    let f = fixture("observe");
    let marker = f.marker("independent-lifetime");
    let run = accept_capture(&f, "success", &marker, ActionCancellation::default());
    f.application
        .execute_admitted_capture(run, policy(None, None, Some(100)))
        .unwrap();
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    let snapshot = result_id(&f, run);
    let original = manifest(&f, snapshot).canonical_bytes().to_vec();
    let db = db(&f);
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE; DELETE FROM runs; DELETE FROM managed_input_bindings; DELETE FROM instances; DELETE FROM revision_runtime_content_refs; DELETE FROM revisions; COMMIT;").unwrap();
    for table in [
        "runs",
        "instances",
        "revisions",
        "instance_recovery_consequence_versions",
    ] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    let verified = persistence(&f.storage).verify_snapshot(snapshot).unwrap();
    assert_eq!(
        verified.relational,
        SnapshotRelationalVerification::NotEvaluated
    );
    assert_eq!(verified.inspection.origin, f.instance.id);
    assert_eq!(manifest(&f, snapshot).canonical_bytes(), original);
    assert_eq!(
        persistence(&f.storage).list_snapshots(None).unwrap().len(),
        1
    );
}

// Test-ID: PR-TEST-0421
// Verifies: PR-REQ-0072, PR-REQ-0073, PR-REQ-0103, PR-REQ-0338
#[test]
fn managed_retirement_preserves_real_capture_and_action_artifact_lifetimes() {
    for mode in [DeletionMode::ManagedCleanup, DeletionMode::AbandonManagement] {
        let f = fixture("observe");
        let capture = accept_capture(&f, "success", &f.marker("retirement-capture"), ActionCancellation::default());
        f.application.execute_admitted_capture(capture, policy(None, None, Some(100))).unwrap();
        assert_eq!(finish(&f, capture).outcome, RunOutcome::Succeeded);
        let snapshot = result_id(&f, capture);
        let original = manifest(&f, snapshot).canonical_bytes().to_vec();
        let admitted = f.admit("output_subset", "output_subset", &f.marker("retirement-artifact"));
        let action = admitted.run();
        f.application.execute_admitted_action(admitted, policy(None, None, Some(100)), ActionCancellation::default());
        assert_eq!(finish(&f, action).outcome, RunOutcome::Succeeded);
        let intent = f.application.resolve_deletion(&f.instance.name, None, mode).unwrap();
        let plan = f.application.compile_deletion(&intent, &[]).unwrap();
        let deletion = f.application.accept_deletion_plan(plan, AdmissionOptions::default(), ActionCancellation::default(), policy(None, None, Some(100))).unwrap();
        assert_eq!(finish(&f, deletion).outcome, RunOutcome::Succeeded);
        let p = persistence(&f.storage);
        assert!(p.load_instance_by_id(f.instance.id).unwrap().is_none());
        assert_eq!(p.list_managed_runs(f.instance.id).unwrap().len(), 3);
        assert!(p.managed_run_inspection(capture).unwrap().is_some());
        assert!(p.managed_run_inspection(action).unwrap().is_some());
        let verified = p.verify_snapshot(snapshot).unwrap();
        assert_eq!(verified.inspection.origin, f.instance.id);
        assert_eq!(manifest(&f, snapshot).canonical_bytes(), original);
        let mut bytes = Vec::new();
        p.open_run_artifact(action, &ManagedOutputIdentity::parse("report").unwrap(), &mut bytes).unwrap();
        assert_eq!(bytes, b"unpublished-output");
        assert_eq!(p.list_snapshots(Some(f.instance.id)).unwrap().len(), 1);
        assert_eq!(db(&f).query_row("SELECT count(*) FROM run_revision_pins", [], |r| r.get::<_,i64>(0)).unwrap(), 0);
    }
}

// Test-ID: PR-TEST-0246
// Verifies: PR-REQ-0100, PR-REQ-0145, PR-REQ-0146, PR-REQ-0147, PR-REQ-0150, PR-REQ-0152, PR-REQ-0191, PR-REQ-0239, PR-REQ-0289, PR-REQ-0290, PR-REQ-0298
#[test]
fn real_observe_and_mutate_capture_publish_v2_from_the_pinned_active_view() {
    for access in ["observe", "mutate"] {
        let f = fixture(access);
        let marker = f.marker("capture");
        let run = accept_capture(&f, "success", &marker, ActionCancellation::default());
        f.replace_binding();
        let before = f
            .application
            .load_instance(f.instance.id)
            .unwrap()
            .unwrap()
            .state_version;
        f.application
            .execute_admitted_capture(run, policy(Some(10_000), Some(10_000), Some(100)))
            .unwrap();
        let outcome = finish(&f, run);
        assert_eq!(outcome.outcome, RunOutcome::Succeeded);
        assert!(outcome.artifacts.is_empty());
        assert_eq!(outcome.hook_completion.unwrap().message, None);
        let id = result_id(&f, run);
        let saved = manifest(&f, id);
        assert_eq!(saved.manifest().version(), SnapshotIntegrityVersion::V2);
        let bindings = saved.manifest().managed_bindings();
        assert_eq!(bindings.len(), 2);
        assert!(
            bindings
                .iter()
                .any(|b| b.input_id.as_str() == "optional"
                    && b.state == SnapshotBindingState::Absent)
        );
        assert!(
            bindings
                .iter()
                .any(|b| b.input_id.as_str() == "secret_config"
                    && b.protection == ManagedInputProtection::Secret
                    && b.state
                        == SnapshotBindingState::Bound(Sha256Digest::from_bytes(
                            sha2::Sha256::digest(SECRET_BINDING).into()
                        )))
        );
        assert_eq!(saved.manifest().service_content().len(), 2);
        assert_eq!(
            saved.manifest().service_content()[0].blob_digest,
            saved.manifest().service_content()[1].blob_digest
        );
        assert_eq!(
            persistence(&f.storage)
                .verify_snapshot(id)
                .unwrap()
                .relational,
            SnapshotRelationalVerification::Valid
        );
        assert_eq!(
            f.application
                .load_instance(f.instance.id)
                .unwrap()
                .unwrap()
                .state_version,
            before
        );
        let session: Value =
            serde_json::from_slice(&fs::read(marker_variant(&marker, "session")).unwrap()).unwrap();
        assert_eq!(session["operation"]["access"], access);
        assert!(session["operation"].get("outputs").is_none());
        assert_eq!(
            session["operation"]["bindings"].as_array().unwrap().len(),
            1
        );
        assert!(!contains(
            &database_bytes(&f.storage),
            SENSITIVE_PARAMETER.as_bytes()
        ));
    }
}

// Test-ID: PR-TEST-0247
// Verifies: PR-REQ-0147, PR-REQ-0148, PR-REQ-0290, PR-REQ-0294, PR-REQ-0298
#[test]
fn capture_publication_retry_preserves_time_identity_and_distinct_source_accounting() {
    for mode in ["success", "aliases"] {
        let f = fixture("observe");
        let marker = f.marker(mode);
        let run = accept_capture(&f, mode, &marker, ActionCancellation::default());
        // Use an instant exactly representable by both POSIX and Windows'
        // 100ns SystemTime resolution; production retains the sampled clock.
        let at = UNIX_EPOCH + Duration::new(1_700_000_000, 123_456_700);
        let reads = super::super::capture::test_clock(Some(at));
        f.application
            .execute_admitted_capture(run, policy(Some(10_000), Some(10_000), Some(100)))
            .unwrap();
        assert_eq!(
            super::super::capture::test_clock(Some(at + Duration::from_secs(60))),
            reads + 1
        );
        crate::persistence::fail_next_capture_publication_for_test();
        assert!(f.application.advance_owner_continuation(run).is_err());
        assert_eq!(
            db(&f)
                .query_row("SELECT count(*) FROM snapshots", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        let guard = f.application.take_owner_continuation(run).unwrap();
        let OwnerContinuation::CaptureFinalization(state) = guard.continuation() else {
            panic!("Capture continuation")
        };
        let (id, time, acquired, blobs) = state.prepared_metrics().unwrap();
        assert_eq!(blobs, 2);
        assert_eq!(
            time,
            SnapshotTimestamp::new(1_700_000_000, 123_456_700).unwrap()
        );
        assert_eq!(
            acquired,
            SECRET_BINDING.len() as u64
                + SERVICE.len() as u64 * if mode == "aliases" { 2 } else { 1 }
        );
        drop(guard);
        assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
        assert_eq!(result_id(&f, run), id);
        assert_eq!(manifest(&f, id).manifest().captured_at(), time);
        assert_eq!(super::super::capture::test_clock(None), reads + 1);
    }
}

#[test]
fn capture_hook_worker() {
    let Ok(transport) = env::var(TRANSPORT_ENVIRONMENT) else {
        return;
    };
    let endpoint = env::var(ENDPOINT_ENVIRONMENT).unwrap();
    let mut stream = connect_hook(&transport, &endpoint);
    let mut preamble = [0; PREAMBLE.len()];
    stream.read_exact(&mut preamble).unwrap();
    assert_eq!(preamble, PREAMBLE);
    let session = read_frame(&mut stream).unwrap();
    assert_eq!(session["operation"]["kind"], "snapshot_capture");
    let params = session["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["parameter_id"].as_str().unwrap(),
                p["value"].as_str().unwrap(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mode = params["mode"];
    let marker = PathBuf::from(params["marker"]);
    fs::write(marker_variant(&marker, "session"), session.to_string()).unwrap();
    let bindings = session["operation"]["bindings"].as_array().unwrap();
    assert!(bindings.iter().all(|b| b["role"] == "active"));
    let binding = bindings
        .iter()
        .find(|b| b["input_id"] == "secret_config")
        .unwrap();
    assert_eq!(
        fs::read(binding["readonly_path"].as_str().unwrap()).unwrap(),
        params["expected_binding"].as_bytes()
    );
    stream.write_all(PREAMBLE).unwrap();
    if mode == "no_ready" {
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    write_frame(
        &mut stream,
        &json!({"type":"session_ready","protocol_version":1,"session_id":session["session_id"]}),
    );
    fs::write(marker_variant(&marker, "ready"), b"").unwrap();
    if mode == "hang" {
        let cancel = read_frame(&mut stream).unwrap();
        assert_eq!(cancel["type"], "cancel");
        write_frame(
            &mut stream,
            &json!({"type":"cancel_ack","control_id":cancel["control_id"]}),
        );
        return;
    }
    if mode.starts_with("open_") {
        write_frame(
            &mut stream,
            &json!({"type":"request","request_id":1,"request":{"kind":"enter_recovery_risk"}}),
        );
        let ack = read_frame(&mut stream).unwrap();
        assert_eq!(ack["risk_state"], "open");
        fs::write(marker_variant(&marker, "risk"), b"").unwrap();
        if mode == "open_resolved" {
            write_frame(
                &mut stream,
                &json!({"type":"request","request_id":2,"request":{"kind":"resolve_recovery_risk"}}),
            );
            assert_eq!(read_frame(&mut stream).unwrap()["risk_state"], "clear");
        }
    }
    let root = Path::new(
        session["operation"]["candidate"]["root_path"]
            .as_str()
            .unwrap(),
    );
    fs::create_dir(root.join("files")).unwrap();
    if mode == "directory" {
        fs::create_dir(root.join("files/one")).unwrap();
    } else if mode == "fifo" {
        #[cfg(target_os = "linux")]
        assert!(
            Command::new("mkfifo")
                .arg(root.join("files/one"))
                .status()
                .unwrap()
                .success()
        );
        #[cfg(not(target_os = "linux"))]
        panic!("FIFO fixture is Linux-only");
    } else if let Some(mib) = mode.strip_prefix("capacity-") {
        let mib: u64 = mib.parse().unwrap();
        for (name, byte) in [("one", 0x5a), ("two", 0xa5)] {
            let mut file = fs::File::create(root.join(format!("files/{name}"))).unwrap();
            let chunk = [byte; 64 * 1024];
            for _ in 0..mib * 16 {
                file.write_all(&chunk).unwrap();
            }
            file.write_all(&[0x7f]).unwrap();
            file.sync_all().unwrap();
        }
    } else if mode == "large" {
        let mut file = fs::File::create(root.join("files/one")).unwrap();
        let chunk = vec![0x5a; 1024 * 1024];
        for _ in 0..512 {
            file.write_all(&chunk).unwrap();
        }
        file.write_all(&[0x7f]).unwrap();
        file.sync_all().unwrap();
    } else {
        fs::write(root.join("files/one"), SERVICE).unwrap();
    }
    if mode == "reparse" {
        let workspace = Path::new(session["workspace"]["root_path"].as_str().unwrap());
        fs::write(workspace.join("outside"), b"outside-candidate-sentinel").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(workspace, root.join("alias")).unwrap();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let script = workspace.join("junction.ps1");
            fs::write(&script,b"param([string]$Link,[string]$Target)\n$ErrorActionPreference='Stop'\nNew-Item -ItemType Junction -Path $Link -Target $Target | Out-Null\n").unwrap();
            assert!(
                Command::new("powershell.exe")
                    .args([
                        "-NoProfile",
                        "-NonInteractive",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-File"
                    ])
                    .arg(script)
                    .arg(root.join("alias"))
                    .arg(workspace)
                    .creation_flags(0x08000000)
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }
    fs::write(root.join("unsubmitted"), b"must-not-be-captured").unwrap();
    let second = if mode.starts_with("capacity-") {
        "files/two"
    } else if mode == "aliases" {
        fs::hard_link(root.join("files/one"), root.join("files/two")).unwrap();
        "files/two"
    } else {
        "files/one"
    };
    let status = if matches!(mode, "failure" | "open_failure") {
        "failure"
    } else {
        "success"
    };
    let source = match mode {
        "escape" => "../outside",
        "missing" => "files/missing",
        "reparse" => "alias/outside",
        _ => "files/one",
    };
    let content = if status == "failure" {
        json!([])
    } else {
        json!([
        {"role":"database","path":"db/main","candidate_path":source}, {"role":"logs","path":"log/main","candidate_path":second}])
    };
    write_frame(
        &mut stream,
        &json!({"type":"complete","operation":"snapshot_capture","status":status,"code":SENSITIVE_PARAMETER,"message":SENSITIVE_PARAMETER,"service_content":content}),
    );
    if matches!(mode, "escape" | "open_success") {
        let _ = read_frame(&mut stream);
        return;
    }
    assert_eq!(
        read_frame(&mut stream).unwrap()["type"],
        "completion_accepted"
    );
    fs::write(marker_variant(&marker, "accepted"), b"").unwrap();
}

// Test-ID: PR-TEST-0248
// Verifies: PR-REQ-0290, PR-REQ-0289, PR-REQ-0215, PR-REQ-0216
#[test]
fn failed_cancelled_timed_out_and_invalid_captures_never_publish_a_snapshot() {
    for mode in [
        "failure",
        "open_failure",
        "open_success",
        "escape",
        "missing",
        "directory",
        "hang",
        "no_ready",
    ] {
        let f = fixture("mutate");
        let marker = f.marker(mode);
        let cancellation = ActionCancellation::default();
        let run = accept_capture(&f, mode, &marker, cancellation.clone());
        let cancelling = if mode == "hang" {
            Some(cancel_after(
                marker_variant(&marker, "ready"),
                &cancellation,
            ))
        } else {
            None
        };
        let timeout = if mode == "no_ready" { 100 } else { 10_000 };
        f.application
            .execute_admitted_capture(run, policy(Some(timeout), Some(10_000), Some(50)))
            .unwrap();
        if let Some(thread) = cancelling {
            thread.join().unwrap();
        }
        let outcome = finish(&f, run);
        assert_eq!(
            outcome.outcome,
            match mode {
                "hang" => RunOutcome::Cancelled,
                "no_ready" => RunOutcome::TimedOut,
                _ => RunOutcome::Failed,
            },
            "{mode}"
        );
        assert_eq!(
            db(&f)
                .query_row("SELECT count(*) FROM snapshots", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0,
            "{mode}"
        );
        assert_eq!(
            db(&f)
                .query_row("SELECT count(*) FROM run_capture_results", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let guard = persistence(&f.storage)
            .load_instance_recovery_guard(f.instance.id)
            .unwrap();
        assert_eq!(guard.is_some(), mode.starts_with("open_"));
        assert!(!contains(
            &database_bytes(&f.storage),
            SENSITIVE_PARAMETER.as_bytes()
        ));
    }
}

// Test-ID: PR-TEST-0249
// Verifies: PR-REQ-0091, PR-REQ-0100, PR-REQ-0150, PR-REQ-0289, PR-REQ-0290
#[test]
fn capture_records_retained_secret_and_empty_required_payload_without_exposing_retained_to_hook() {
    let f = fixture("observe");
    // A valid pre-existing retained binding aliases one immutable managed
    // payload source; the Hook must not receive this retained authority.
    db(&f).execute("INSERT INTO managed_input_bindings(instance_id,input_identity,payload_id) SELECT instance_id,?2,payload_id FROM managed_input_bindings WHERE instance_id=?1 AND input_identity=?3",rusqlite::params![f.instance.id.as_bytes().as_slice(),b"legacy".as_slice(),b"secret_config".as_slice()]).unwrap();
    let marker = f.marker("retained");
    let run = accept_capture(&f, "success", &marker, ActionCancellation::default());
    f.application
        .execute_admitted_capture(run, policy(Some(10_000), Some(10_000), Some(100)))
        .unwrap();
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    let saved = manifest(&f, result_id(&f, run));
    assert!(
        saved
            .manifest()
            .managed_bindings()
            .iter()
            .any(|b| b.input_id.as_str() == "legacy"
                && b.role == SnapshotBindingRole::Retained
                && b.protection == ManagedInputProtection::Secret)
    );
    let session: Value =
        serde_json::from_slice(&fs::read(marker_variant(&marker, "session")).unwrap()).unwrap();
    assert_eq!(
        session["operation"]["bindings"].as_array().unwrap().len(),
        1
    );
    let current = f.application.load_instance(f.instance.id).unwrap().unwrap();
    f.application
        .set_input(
            f.instance.id,
            InputIdentity::parse("secret_config").unwrap(),
            current.state_version,
            Box::new(Cursor::new(Vec::<u8>::new())),
        )
        .unwrap();
    let marker = f.marker("empty");
    let run = accept_capture_expected(&f, "success", &marker, ActionCancellation::default(), b"");
    f.application
        .execute_admitted_capture(run, policy(Some(10_000), Some(10_000), Some(100)))
        .unwrap();
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    assert!(
        manifest(&f, result_id(&f, run))
            .manifest()
            .managed_bindings()
            .iter()
            .any(|b| b.input_id.as_str() == "secret_config"
                && b.state
                    == SnapshotBindingState::Bound(Sha256Digest::from_bytes(
                        sha2::Sha256::digest([]).into()
                    )))
    );
}

// Test-ID: PR-TEST-0251
// Verifies: PR-REQ-0290, PR-REQ-0293
#[test]
fn capture_service_content_above_512_mib_uses_real_streaming_and_snapshot_chunks() {
    let f = fixture("observe");
    let marker = f.marker("large");
    let run = accept_capture(&f, "large", &marker, ActionCancellation::default());
    f.application
        .execute_admitted_capture(run, policy(Some(10_000), None, Some(100)))
        .unwrap();
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    let id = result_id(&f, run);
    let largest: i64 = db(&f)
        .query_row(
            "SELECT max(byte_length) FROM snapshot_blobs WHERE snapshot_id=?1",
            [id.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(largest, 512 * 1024 * 1024 + 1);
    assert_eq!(
        persistence(&f.storage)
            .verify_snapshot(id)
            .unwrap()
            .relational,
        SnapshotRelationalVerification::Valid
    );
}

// Test-ID: PR-TEST-0250
// Verifies: PR-REQ-0290, PR-REQ-0213
#[test]
fn capture_candidate_acquisition_rejects_intermediate_symlinks_or_junctions() {
    let f = fixture("observe");
    let marker = f.marker("reparse");
    let run = accept_capture(&f, "reparse", &marker, ActionCancellation::default());
    f.application
        .execute_admitted_capture(run, policy(Some(10_000), Some(30_000), Some(100)))
        .unwrap();
    assert!(marker_variant(&marker, "accepted").exists()); // protocol-valid locator; filesystem qualification rejects it.
    assert_eq!(finish(&f, run).outcome, RunOutcome::Failed);
    assert_eq!(
        db(&f)
            .query_row("SELECT count(*) FROM snapshots", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

const CAPTURE_ACTOR: &str = "hook::tests::capture_runtime::capture_process_owner";

// Test-ID: PR-TEST-0255
// Verifies: PR-REQ-0290, PR-REQ-0298
#[test]
fn capture_staging_corruption_on_publication_retry_never_publishes_a_result() {
    let f = fixture("observe");
    let marker = f.marker("corrupt-staging");
    let run = accept_capture(&f, "success", &marker, ActionCancellation::default());
    f.application
        .execute_admitted_capture(run, policy(Some(10_000), Some(10_000), Some(100)))
        .unwrap();
    crate::persistence::fail_next_capture_publication_for_test();
    assert!(f.application.advance_owner_continuation(run).is_err());
    let mut guard = f.application.take_owner_continuation(run).unwrap();
    let Some(OwnerContinuation::CaptureFinalization(state)) = guard.continuation.as_mut() else {
        panic!("Capture continuation")
    };
    state.corrupt_staging_for_test();
    drop(guard);
    assert_eq!(finish(&f, run).outcome, RunOutcome::Failed);
    for table in ["snapshots", "run_capture_results"] {
        assert_eq!(
            db(&f)
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

// Test-ID: PR-TEST-0256
// Verifies: PR-REQ-0290, PR-REQ-0213
#[cfg(target_os = "linux")]
#[test]
fn capture_fifo_candidate_is_rejected_without_blocking_acquisition() {
    let f = fixture("observe");
    let marker = f.marker("fifo");
    let run = accept_capture(&f, "fifo", &marker, ActionCancellation::default());
    f.application
        .execute_admitted_capture(run, policy(Some(10_000), Some(10_000), Some(100)))
        .unwrap();
    assert!(marker_variant(&marker, "accepted").exists());
    assert_eq!(finish(&f, run).outcome, RunOutcome::Failed);
    assert_eq!(
        db(&f)
            .query_row("SELECT count(*) FROM snapshots", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn capture_process_owner() {
    let Some(storage) = env::var_os("PACTRUN_S5_OWNER_ROOT") else {
        return;
    };
    let storage = PathBuf::from(storage);
    let marker = PathBuf::from(env::var_os("PACTRUN_S5_OWNER_MARKER").unwrap());
    let mode = env::var("PACTRUN_S5_OWNER_MODE").unwrap();
    let app = PactrunApplication::open(&storage).unwrap();
    let instance = app
        .resolve_instance_name(&InstanceName::parse("slice4").unwrap())
        .unwrap()
        .unwrap();
    let plan = crate::workflow::compile_snapshot(
        &app,
        &crate::workflow::PlatformHostLauncherLookup,
        &SnapshotIntent {
            instance,
            operation: SnapshotOperation::Capture,
            parameters: parameters(&mode, &marker),
        },
        &[],
    )
    .unwrap();
    let run = app
        .accept_snapshot_plan(
            plan,
            AdmissionOptions::default(),
            ActionCancellation::default(),
        )
        .unwrap();
    assert!(!app.advance_owner_continuation(run).unwrap());
    fs::write(marker_variant(&marker, "run"), run.to_string()).unwrap();
    app.execute_admitted_capture(run, policy(Some(10_000), None, Some(100)))
        .unwrap();
    fs::write(marker_variant(&marker, "before-publication"), b"").unwrap();
    if env::var_os("PACTRUN_S5_PAUSE").is_some() {
        wait_for(&marker_variant(&marker, "release"));
    }
    while !app.advance_owner_continuation(run).unwrap() {
        thread::sleep(Duration::from_millis(5));
    }
}
struct Actor(std::process::Child);
impl Drop for Actor {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn actor(f: &RuntimeFixture, marker: &Path, mode: &str, fault: Option<&str>, pause: bool) -> Actor {
    let mut command = Command::new(env::current_exe().unwrap());
    command
        .args(["--exact", CAPTURE_ACTOR, "--nocapture"])
        .env("PACTRUN_S5_OWNER_ROOT", &f.storage)
        .env("PACTRUN_S5_OWNER_MARKER", marker)
        .env("PACTRUN_S5_OWNER_MODE", mode)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit());
    if let Some(fault) = fault {
        command.env("PACTRUN_M4_FAULT", fault);
    }
    if pause {
        command.env("PACTRUN_S5_PAUSE", "1");
    }
    Actor(command.spawn().unwrap())
}

// Test-ID: PR-TEST-0252
// Verifies: PR-REQ-0290, PR-REQ-0298
#[test]
fn capture_owner_loss_and_commit_crashes_never_salvage_unpublished_candidates() {
    for mode in ["success", "open_failure"] {
        let f = fixture("mutate");
        let marker = f.marker("owner-loss");
        let mut actor = actor(&f, &marker, mode, None, true);
        wait_for(&marker_variant(&marker, "before-publication"));
        let run = fs::read_to_string(marker_variant(&marker, "run"))
            .unwrap()
            .parse::<RunId>()
            .unwrap();
        assert!(
            f.application
                .reconcile_lost_managed_owners()
                .unwrap()
                .is_empty()
        );
        actor.0.kill().unwrap();
        actor.0.wait().unwrap();
        assert_eq!(
            f.application.reconcile_lost_managed_owners().unwrap(),
            vec![run]
        );
        let RunState::Finished(outcome) = persistence(&f.storage)
            .load_managed_run(run)
            .unwrap()
            .unwrap()
            .state
        else {
            panic!("terminal")
        };
        assert_eq!(outcome.outcome, RunOutcome::Interrupted);
        assert_eq!(
            db(&f)
                .query_row("SELECT count(*) FROM snapshots", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            persistence(&f.storage)
                .recovery_consequence_version(f.instance.id)
                .unwrap(),
            i64::from(mode == "open_failure")
        );
    }
    for (fault, committed) in [
        ("before_run_finish_commit", false),
        ("after_run_finish_commit", true),
    ] {
        let f = fixture("observe");
        let marker = f.marker("crash");
        let mut actor = actor(&f, &marker, "success", Some(fault), false);
        assert_eq!(actor.0.wait().unwrap().code(), Some(87));
        let run = fs::read_to_string(marker_variant(&marker, "run"))
            .unwrap()
            .parse::<RunId>()
            .unwrap();
        assert_eq!(
            db(&f)
                .query_row("SELECT count(*) FROM snapshots", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            i64::from(committed)
        );
        assert_eq!(
            db(&f)
                .query_row("SELECT count(*) FROM run_capture_results", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            i64::from(committed)
        );
        let reconciled = f.application.reconcile_lost_managed_owners().unwrap();
        assert_eq!(reconciled, if committed { vec![] } else { vec![run] });
        let RunState::Finished(outcome) = persistence(&f.storage)
            .load_managed_run(run)
            .unwrap()
            .unwrap()
            .state
        else {
            panic!("terminal")
        };
        assert_eq!(
            outcome.outcome,
            if committed {
                RunOutcome::Succeeded
            } else {
                RunOutcome::Interrupted
            }
        );
        if committed {
            persistence(&f.storage)
                .verify_snapshot(result_id(&f, run))
                .unwrap();
        }
        assert!(
            f.application
                .reconcile_lost_managed_owners()
                .unwrap()
                .is_empty()
        );
    }
}

// Test-ID: PR-TEST-0253
// Verifies: PR-REQ-0290, PR-REQ-0215, PR-REQ-0289
#[test]
fn capture_risk_ack_waits_for_durable_retry_and_does_not_restart_the_hook() {
    let f = fixture("mutate");
    let marker = f.marker("risk-retry");
    let run = accept_capture(&f, "open_resolved", &marker, ActionCancellation::default());
    f.application
        .execute_capture_with_risk_failures(
            run,
            policy(Some(10_000), Some(10_000), Some(100)),
            &AtomicUsize::new(1),
        )
        .unwrap();
    assert!(!marker_variant(&marker, "risk").exists());
    // Inspect persisted state through the existing reader. Opening another
    // writer here creates a durable session/admission while the deliberately
    // paused Hook's original deadline is still ticking.
    let RunState::Running(view) = f.application
        .managed_run_inspection(run)
        .unwrap()
        .unwrap()
        .run.state
    else {
        panic!("retry must remain running")
    };
    assert_eq!(view.risk_state, RecoveryRiskState::Clear);
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    assert!(marker_variant(&marker, "risk").exists());
    assert_eq!(
        db(&f)
            .query_row("SELECT count(*) FROM run_capture_results", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(
        persistence(&f.storage)
            .load_instance_recovery_guard(f.instance.id)
            .unwrap()
            .is_none()
    );
    assert!(
        f.application
            .claim_snapshot_execution(run)
            .unwrap()
            .is_none()
    );
}
