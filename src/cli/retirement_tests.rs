use super::*;
use crate::{domain::*, persistence::PactrunPersistence};

fn fixture() -> (
    tempfile::TempDir,
    PathBuf,
    InstanceView,
    ServiceAllocationId,
    PathBuf,
) {
    fixture_with_allocations(1)
}

fn fixture_with_allocations(
    count: usize,
) -> (
    tempfile::TempDir,
    PathBuf,
    InstanceView,
    ServiceAllocationId,
    PathBuf,
) {
    assert!(count > 0);
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/deletion-cli-tests");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let root = temp.path().join("storage");
    for part in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(part)).unwrap();
    }
    let p = PactrunPersistence::open(&root).unwrap();
    let storages: Vec<_> = (0..count).map(|index| serde_json::json!({"id":if index==0 {"data".to_owned()} else {format!("data{index}")}})).collect();
    let core = crate::revision_canonical::project_service_revision_source(
        &serde_json::to_vec(&serde_json::json!({
            "format_version":"1.0-alpha.1","inputs":[],"actions":[],"migrations":[],
            "service_storages":storages,"service_resources":[]
        }))
        .unwrap(),
    )
    .unwrap();
    let runtime =
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![] })
            .unwrap();
    let content =
        crate::revision_canonical::validate_service_revision_content(core, runtime).unwrap();
    let revision = p
        .persist_versioned_revision_with_metadata(
            PackageId::from_bytes([55; 16]),
            &content.into(),
            &[],
            &RevisionMetadataMutationBatch::new([]).unwrap(),
        )
        .unwrap();
    let instance = p
        .create_instance(InstanceName::parse("retire").unwrap(), revision, &mut [])
        .unwrap();
    let allocation = p
        .load_instance_service_state(instance.id)
        .unwrap()
        .unwrap()
        .storages[0]
        .allocation;
    let path = root
        .join("service-storage")
        .join(format!("alloc-{allocation}"));
    fs::write(path.join("secret"), b"service-owned-private-sentinel").unwrap();
    (temp, root, instance, allocation, path)
}

fn invoke(root: &Path, args: &[&str]) -> (i32, String, String) {
    let mut out = vec![];
    let mut err = vec![];
    let code = crate::cli::run(
        args.iter().map(OsString::from).collect(),
        Some(root.as_os_str().to_owned()),
        &mut io::empty(),
        &mut out,
        &mut err,
    );
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

// Test-ID: PR-TEST-0787
// Verifies: PR-REQ-0336
#[test]
#[ignore = "manual retirement timing probe; run explicitly on a qualified native filesystem"]
fn retirement_allocation_timing_probe() {
    for count in [1, 8, 24] {
        for sample in 0..3 {
            let (_temp, root, instance, _, _) = fixture_with_allocations(count);
            let p = PactrunPersistence::open(&root).unwrap();
            let paths: Vec<_> = p
                .load_instance_service_state(instance.id)
                .unwrap()
                .unwrap()
                .storages
                .iter()
                .map(|storage| {
                    root.join("service-storage")
                        .join(format!("alloc-{}", storage.allocation))
                })
                .collect();
            assert_eq!(paths.len(), count);
            assert!(paths.iter().all(|path| path.is_dir()));
            let started = std::time::Instant::now();
            let (code, out, err) =
                invoke(&root, &["--format", "json", "instance", "delete", "retire"]);
            let elapsed = started.elapsed();
            assert_eq!(code, 0, "{out}; {err}");
            let response: serde_json::Value = serde_json::from_str(&out).unwrap();
            assert_eq!(response["result"]["run"]["state"]["outcome"], "succeeded");
            assert!(p.load_instance_by_id(instance.id).unwrap().is_none());
            assert!(paths.iter().all(|path| !path.exists()));
            println!(
                "RETIREMENT_SAMPLE {}",
                serde_json::json!({"os":std::env::consts::OS,"allocations":count,"sample":sample,"elapsed_us":elapsed.as_micros().to_string()})
            );
        }
    }
}

// Test-ID: PR-TEST-0788
// Verifies: PR-REQ-0336, PR-REQ-0397
#[test]
fn ready_retirement_allocations_drain_without_foreground_polling_and_keep_phase_evidence() {
    let (_temp, root, instance, _, _) = fixture_with_allocations(8);
    let app = PactrunApplication::open(&root).unwrap();
    let token = ActionCancellation::default();
    token.trace.begin();
    token.trace.configure(
        None,
        Some(&root),
        TracePolicy::startup(),
        TracePolicy::default(),
    );
    token.trace.operation(TraceOperation::Retirement);
    let intent = app
        .resolve_deletion(&instance.name, None, DeletionMode::ManagedCleanup)
        .unwrap();
    let plan = app.compile_deletion(&intent, &[]).unwrap();
    let run = app
        .accept_deletion_plan(
            plan,
            AdmissionOptions::default(),
            token.clone(),
            HookRuntimePolicy::from_millis(None, None, None).unwrap(),
        )
        .unwrap();
    assert!(
        app.advance_owner_continuation(run).unwrap(),
        "ready work required another foreground polling interval"
    );
    assert!(app.load_instance(instance.id).unwrap().is_none());
    let inspection = app.managed_run_inspection(run).unwrap().unwrap();
    assert!(
        matches!(inspection.run.state,RunState::Finished(ref outcome) if outcome.outcome==RunOutcome::Succeeded)
    );
    token.trace.finish(0).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let trace = loop {
        if let Some(trace) =
            crate::trace_journal::read_run(&crate::trace_journal::Location::store(&root), run)
                .unwrap()
            && trace.closed
        {
            break trace;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "retirement trace did not close"
        );
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(trace.totals[&TracePhase::StorageFinalization].finished, 9);
    assert_eq!(trace.totals[&TracePhase::StorageFinalization].failed, 0);
    assert_eq!(trace.totals[&TracePhase::ResultCommit].finished, 1);
    assert!(
        !serde_json::to_string(&trace)
            .unwrap()
            .contains("service-owned-private-sentinel")
    );
}

// Test-ID: PR-TEST-0789
// Verifies: PR-REQ-0334, PR-REQ-0336
#[test]
fn retirement_drain_cancellation_between_allocations_keeps_retryable_progress() {
    use std::process::{Command, Stdio};
    let (temp, root, instance, _, _) = fixture_with_allocations(8);
    let control = temp.path().join("control");
    fs::create_dir(&control).unwrap();
    let p = PactrunPersistence::open(&root).unwrap();
    let paths: Vec<_> = p
        .load_instance_service_state(instance.id)
        .unwrap()
        .unwrap()
        .storages
        .iter()
        .map(|storage| {
            root.join("service-storage")
                .join(format!("alloc-{}", storage.allocation))
        })
        .collect();
    struct Worker(std::process::Child);
    impl Drop for Worker {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }
    let mut worker = Worker(
        Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "cli::retirements::tests::retirement_drain_cancel_worker",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("PACTRUN_RETIRE_CANCEL_ROOT", &root)
            .env("PACTRUN_RETIRE_CANCEL_CONTROL", &control)
            .env("PACTRUN_LIFECYCLE_SYNC", "after_finalization_removal")
            .env("PACTRUN_LIFECYCLE_SYNC_DIR", &control)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let wait = |name: &str| {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while !control.join(name).exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not reach {name}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    };
    wait("ready");
    fs::write(control.join("cancel"), b"cancel").unwrap();
    wait("cancelled");
    fs::write(control.join("release"), b"release").unwrap();
    assert!(worker.0.wait().unwrap().success());
    assert_eq!(
        paths.iter().filter(|path| path.exists()).count(),
        7,
        "cancellation did not stop after the first committed allocation step"
    );
    assert!(p.load_instance_by_id(instance.id).unwrap().is_some());
    assert_eq!(
        p.deletion_obligation(instance.id).unwrap().unwrap().phase,
        DeletionPhase::FinalizationAuthorized
    );
    let (code, out, err) = invoke(&root, &["--format", "json", "instance", "delete", "retire"]);
    assert_eq!(code, 0, "{out}; {err}");
    assert!(paths.iter().all(|path| !path.exists()));
}

#[test]
fn retirement_drain_cancel_worker() {
    let Some(root) = env::var_os("PACTRUN_RETIRE_CANCEL_ROOT") else {
        return;
    };
    let control = PathBuf::from(env::var_os("PACTRUN_RETIRE_CANCEL_CONTROL").unwrap());
    let app = PactrunApplication::open(Path::new(&root)).unwrap();
    let token = ActionCancellation::default();
    let request = token.clone();
    let sender = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while !control.join("cancel").exists() {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        request.request();
        fs::write(control.join("cancelled"), b"ack").unwrap();
    });
    let intent = app
        .resolve_deletion(
            &InstanceName::parse("retire").unwrap(),
            None,
            DeletionMode::ManagedCleanup,
        )
        .unwrap();
    let plan = app.compile_deletion(&intent, &[]).unwrap();
    let run = app
        .accept_deletion_plan(
            plan,
            AdmissionOptions::default(),
            token,
            HookRuntimePolicy::from_millis(None, None, None).unwrap(),
        )
        .unwrap();
    assert!(!app.advance_owner_continuation(run).unwrap());
    assert!(app.advance_owner_continuation(run).unwrap());
    let view = app.managed_run_inspection(run).unwrap().unwrap();
    assert!(
        matches!(view.run.state,RunState::Finished(ref outcome) if outcome.outcome==RunOutcome::Cancelled)
    );
    sender.join().unwrap();
}

// Test-ID: PR-TEST-0645
// Verifies: PR-REQ-0359, PR-REQ-0336, PR-REQ-0337
#[cfg(target_os = "linux")]
#[test]
fn permission_diagnostics_survive_failed_run_inspection_without_revealing_service_paths() {
    use std::os::unix::fs::PermissionsExt;
    let (_temp, root, _, allocation, path) = fixture();
    fs::create_dir(path.join("private-directory-sentinel")).unwrap();
    let directory = fs::File::open(path.join("private-directory-sentinel")).unwrap();
    directory
        .set_permissions(fs::Permissions::from_mode(0o000))
        .unwrap();
    let (code, out, _) = invoke(&root, &["--format", "json", "instance", "delete", "retire"]);
    assert_eq!(code, 1, "{out}");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    schema_tests::assert_response(&value);
    let run = value["result"]["run"]["run_id"].as_str().unwrap();
    let failure = &value["result"]["run"]["state"]["primary_failure"];
    assert_eq!(failure["reference"]["code"], "allocation_unavailable");
    assert_eq!(failure["detail"], crate::retirement_fs::PERMISSION_DENIED);
    assert_eq!(failure["step"], "finalize_storage");
    let (code, history, _) = invoke(&root, &["--format", "json", "run", "show", run]);
    assert_eq!(code, 0);
    let historical: serde_json::Value = serde_json::from_str(&history).unwrap();
    schema_tests::assert_response(&historical);
    assert!(history.contains(crate::retirement_fs::PERMISSION_DENIED));
    let (code, human, _) = invoke(&root, &["run", "show", run]);
    assert_eq!(code, 0);
    assert!(human.contains(crate::retirement_fs::PERMISSION_DENIED));
    for text in [&out, &history, &human] {
        assert!(!text.contains("private-directory-sentinel"));
        assert!(!text.contains("service-owned-private-sentinel"));
        assert!(!text.contains(&root.to_string_lossy().to_string()));
    }
    let (code, _, _) = invoke(&root, &["instance", "abandon", "retire"]);
    assert_eq!(code, 0);
    let id = allocation.to_string();
    let (code, _, _) = invoke(
        &root,
        &[
            "service-storage",
            "detached",
            "show",
            &id,
            "--reveal-location",
        ],
    );
    assert_eq!(code, 0);
    let (code, _, _) = invoke(
        &root,
        &[
            "service-storage",
            "detached",
            "discard",
            &id,
            "--confirm-discard",
        ],
    );
    assert_eq!(code, 1);
    assert_eq!(
        directory.metadata().unwrap().permissions().mode() & 0o777,
        0
    );
    directory
        .set_permissions(fs::Permissions::from_mode(0o700))
        .unwrap();
    let (code, _, _) = invoke(
        &root,
        &[
            "service-storage",
            "detached",
            "discard",
            &id,
            "--confirm-discard",
        ],
    );
    assert_eq!(code, 0);
}

// Test-ID: PR-TEST-0563
// Verifies: PR-REQ-0359, PR-REQ-0360, PR-REQ-0120
#[test]
fn json_retirement_handoff_and_storage_views_preserve_disclosure_boundaries() {
    let (_temp, root, instance, allocation, path) = fixture();
    let json = |args: &[&str]| {
        let mut all = vec!["--format", "json"];
        all.extend_from_slice(args);
        let (code, out, err) = invoke(&root, &all);
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        schema_tests::assert_response(&value);
        assert_eq!(code, 0, "{value}; {err}");
        assert!(!out.contains("service-owned-private-sentinel"));
        value["result"].clone()
    };
    let storages = json(&["service-storage", "list", "retire"]);
    assert_eq!(storages["items"].as_array().unwrap().len(), 1);
    assert!(
        json(&["resource", "list", "retire"])["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        json(&["service-storage", "detached", "list"])["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let plan = json(&["instance", "abandon", "retire", "--plan"]);
    assert_eq!(plan["work"], "abandon_without_hook");
    assert!(path.exists());
    let result = json(&["instance", "abandon", "retire"]);
    assert_eq!(result["run"]["state"]["outcome"], "succeeded");
    assert!(path.exists());
    let detail = json(&["instance", "deletion", "show", &instance.id.to_string()]);
    let list = json(&["instance", "deletion", "list"]);
    assert_eq!(list["items"][0]["inspection"], detail);
    let allocation_id = allocation.to_string();
    let hidden = json(&["service-storage", "detached", "show", &allocation_id]);
    assert!(hidden["locations"].is_null());
    let shown = json(&[
        "service-storage",
        "detached",
        "show",
        &allocation_id,
        "--reveal-location",
    ]);
    assert!(!shown["locations"].as_array().unwrap().is_empty());
    let discarded = json(&[
        "service-storage",
        "detached",
        "discard",
        &allocation_id,
        "--confirm-discard",
    ]);
    assert_eq!(discarded["new_completion"], true);
    assert!(!path.exists());
}

// Test-ID: PR-TEST-0415
// Verifies: PR-REQ-0335, PR-REQ-0337, PR-REQ-0340
#[test]
fn retirement_parsing_requires_exact_explicit_intent_before_opening_storage() {
    let (_temp, root, _, _, _) = fixture();
    let absent = root.join("not-initialized");
    let id = "00000000000000000000000000000001";
    for args in [
        vec!["service-storage", "detached", "discard", id],
        vec!["service-storage", "detached", "discard", id, "--force"],
        vec![
            "service-storage",
            "detached",
            "discard",
            id,
            "--confirm-discard",
            "--confirm-discard",
        ],
        vec!["service-storage", "detached", "show", id, "--json"],
        vec![
            "instance",
            "deletion",
            "confirm-complete",
            id,
            "--attempt",
            id,
            "--if-version",
            id,
        ],
        vec![
            "instance",
            "deletion",
            "confirm-complete",
            id,
            "--assert-cleanup-complete",
        ],
        vec!["instance", "delete", "retire", "--param", "x"],
        vec![
            "instance", "delete", "retire", "--param", "x=y", "--param", "x=z",
        ],
        vec!["instance", "abandon", "retire", "--param", "x=y"],
        vec!["instance", "delete", "retire", "--plan", "--plan"],
    ] {
        let (code, _, _) = invoke(&absent, &args);
        assert_eq!(code, 2, "{args:?}");
        assert!(!absent.exists());
    }
}

// Test-ID: PR-TEST-0722
// Verifies: PR-REQ-0390
#[test]
fn undeclared_cleanup_parameters_refuse_before_file_acquisition_or_retirement() {
    let (_temp, root, instance, _, path) = fixture();
    let (code, _, error) = invoke(
        &root,
        &[
            "instance",
            "delete",
            "retire",
            "--param-file",
            "unknown=nonexistent-private-parameter-file",
        ],
    );
    assert_eq!(code, 2);
    assert!(error.contains("unknown operation parameter"));
    assert!(!error.contains("nonexistent-private-parameter-file"));
    assert_eq!(
        fs::read(path.join("secret")).unwrap(),
        b"service-owned-private-sentinel"
    );
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    assert!(p.load_instance_by_id(instance.id).unwrap().is_some());
    assert!(p.list_managed_runs(instance.id).unwrap().is_empty());
}

// Test-ID: PR-TEST-0416
// Verifies: PR-REQ-0036, PR-REQ-0336, PR-REQ-0338, PR-REQ-0340
#[test]
fn deletion_cli_plans_without_runs_then_removes_storage_and_preserves_exact_history() {
    let (_temp, root, instance, allocation, path) = fixture();
    let (code, out, err) = invoke(&root, &["instance", "delete", "retire", "--plan"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("no_cleanup_declared"));
    assert!(out.contains("Mode: preview"));
    assert!(!out.contains("service-owned-private-sentinel"));
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    assert!(p.list_managed_runs(instance.id).unwrap().is_empty());
    assert!(p.deletion_obligation(instance.id).unwrap().is_none());
    drop(p);
    let (code, _, _) = invoke(
        &root,
        &[
            "service-storage",
            "detached",
            "discard",
            &allocation.to_string(),
            "--confirm-discard",
        ],
    );
    assert_ne!(code, 0);
    assert!(path.join("secret").exists());
    let (code, _, err) = invoke(
        &root,
        &[
            "instance",
            "delete",
            "retire",
            "--if-version",
            &instance.state_version.to_string(),
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(err.contains(&instance.id.to_string()));
    assert!(!path.exists());
    let p = PactrunPersistence::open(&root).unwrap();
    assert!(p.load_instance_by_id(instance.id).unwrap().is_none());
    let replacement = p
        .create_instance(
            instance.name.clone(),
            instance.active_revision.clone(),
            &mut [],
        )
        .unwrap();
    assert_ne!(instance.id, replacement.id);
    drop(p);
    let (code, out, err) = invoke(
        &root,
        &["instance", "deletion", "show", &instance.id.to_string()],
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Managed: No"));
    assert!(out.contains("instance_delete"));
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    assert!(p.load_instance_by_id(replacement.id).unwrap().is_some());
    assert!(p.list_managed_runs(replacement.id).unwrap().is_empty());
}

// Test-ID: PR-TEST-0417
// Verifies: PR-REQ-0181, PR-REQ-0336, PR-REQ-0337, PR-REQ-0340
#[test]
fn abandon_handoff_and_discard_cli_preserve_bytes_until_exact_confirmation() {
    let (_temp, root, instance, allocation, path) = fixture();
    let (code, _, err) = invoke(&root, &["instance", "abandon", "retire"]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("preserves remaining service data"));
    assert!(err.contains("Stop external service processes separately"));
    assert_eq!(
        fs::read(path.join("secret")).unwrap(),
        b"service-owned-private-sentinel"
    );
    for args in [
        vec!["service-storage", "detached", "list", "--no-trunc"],
        vec![
            "service-storage",
            "detached",
            "show",
            &allocation.to_string(),
        ],
    ] {
        let (code, out, err) = invoke(&root, &args);
        assert_eq!(code, 0, "{err}");
        assert!(out.contains(&allocation.to_string()));
        assert!(out.contains(&instance.id.to_string()));
        assert!(!out.contains("native_location"));
        assert!(!out.contains("service-owned-private-sentinel"));
        assert!(!out.contains("alloc-"));
    }
    let (code, out, err) = invoke(
        &root,
        &[
            "service-storage",
            "detached",
            "show",
            &allocation.to_string(),
            "--reveal-location",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Native Location"));
    assert!(out.contains(&format!("alloc-{allocation}")));
    let args = [
        "service-storage",
        "detached",
        "discard",
        &allocation.to_string(),
        "--confirm-discard",
    ];
    let (code, out, err) = invoke(&root, &args);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("New Completion: Yes"));
    assert!(!path.exists());
    fs::create_dir(&path).unwrap();
    fs::write(path.join("replacement"), b"must not be removed").unwrap();
    let (code, out, err) = invoke(&root, &args);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("New Completion: No"));
    assert_eq!(
        fs::read(path.join("replacement")).unwrap(),
        b"must not be removed"
    );
}
