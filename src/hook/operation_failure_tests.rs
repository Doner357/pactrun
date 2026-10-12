use super::*;
use std::process::{Command, Stdio};

// Test-ID: PR-TEST-0733
// Verifies: PR-REQ-0387, PR-REQ-0391, PR-REQ-0392
#[test]
fn transform_report_is_distinct_from_commit_and_survives_native_failure_after_receipt() {
    for mode in ["transform_success", "transform_nonzero"] {
        let f = RuntimeFixture::with_source(|_, _, manifest| {
            let mut value = revision();
            value["service_storages"] = json!([{"id":"data"}]);
            value["service_resources"] =
                json!([{"id":"config","storage_id":"data","locator":"config","kind":"file"}]);
            *manifest = source(value);
        });
        let state = persistence(&f.storage)
            .load_instance_service_state(f.instance.id)
            .unwrap()
            .unwrap();
        let original = f
            .storage
            .join("service-storage")
            .join(format!("alloc-{}", state.storages[0].allocation))
            .join("config");
        fs::write(&original, b"original").unwrap();
        let mut target = revision();
        target["service_storages"] = json!([{"id":"new_data"}]);
        target["service_resources"] =
            json!([{"id":"settings","storage_id":"new_data","locator":"settings","kind":"file"}]);
        let mut migration_hook = hook();
        migration_hook["service_access"] = json!([
            {"reference":{"view":"source","role":"active","kind":"resource","id":"config"},"mode":"read"},
            {"reference":{"view":"target","role":"active","kind":"resource","id":"settings"},"mode":"write"}]);
        target["migrations"] = json!([{"source_revision_digest":f.instance.active_revision.content_digest.to_string(),"transitions":[],"produces_target":[],
            "parameters":declarations(),"outputs":[{"id":"report"}],"hook":migration_hook,
            "storage_transitions":[{"kind":"create","target_storage_id":"new_data"}],
            "resource_transitions":[{"kind":"transform","sources":[{"role":"active","resource_id":"config"}],"targets":["settings"]}]}]);
        let directory = f.temporary.path().join("source");
        fs::write(directory.join("pactrun.yaml"), source(target)).unwrap();
        let target = f
            .application
            .install_pack_source(
                &directory,
                &RevisionMetadataMutationBatch::new(vec![]).unwrap(),
            )
            .unwrap()
            .revision;
        let edge = MigrationEdgeIdentity {
            source: f.instance.active_revision.content_digest,
            target: target.content_digest,
        };
        let marker = f.marker(mode);
        let intent = TransitionRevision {
            instance: f.instance.id,
            expected_state_version: f.instance.state_version,
            source: f.instance.active_revision.clone(),
            target: target.clone(),
            path: MigrationPathSelection::Exact(vec![
                f.instance.active_revision.clone(),
                target.clone(),
            ]),
            operator_inputs: vec![],
            authorize_declassification: false,
            parameters: params(mode, &marker)
                .into_iter()
                .map(|parameter| MigrationParameterInput {
                    edge: edge.clone(),
                    parameter,
                })
                .collect(),
        };
        let plan = crate::workflow::compile_migration(
            &f.application,
            &crate::workflow::PlatformHostLauncherLookup,
            &intent,
            &[],
        )
        .unwrap();
        let run = f
            .application
            .accept_migration_inputs(
                plan,
                false,
                ActionCancellation::default(),
                vec![],
                policy(Some(20_000), Some(20_000), Some(500)),
            )
            .unwrap();
        let outcome = finish(&f, run);
        assert_eq!(
            outcome.outcome,
            if mode == "transform_success" {
                RunOutcome::Succeeded
            } else {
                RunOutcome::Failed
            }
        );
        assert!(
            outcome.hook_completion.is_none(),
            "target_ready is not a successful complete"
        );
        assert_eq!(
            report(&f, run, Some(&edge)),
            format!("ordinary-report:migration:{mode}:").as_bytes()
        );
        let current = f.application.load_instance(f.instance.id).unwrap().unwrap();
        assert_eq!(
            current.active_revision,
            if mode == "transform_success" {
                target
            } else {
                f.instance.active_revision.clone()
            }
        );
        assert_eq!(
            current.recovery_guard.is_some(),
            mode == "transform_nonzero"
        );
        assert_eq!(fs::read(original).unwrap(), b"original");
        let session = session_at(&marker);
        let target_path = session["service_authorities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["reference"]["view"] == "target")
            .unwrap()["path"]
            .as_str()
            .unwrap();
        assert_eq!(
            fs::read(target_path).unwrap(),
            b"transformed:original",
            "external effects are not rolled back"
        );
    }
}

// Test-ID: PR-TEST-0734
// Verifies: PR-REQ-0386, PR-REQ-0391, PR-REQ-0392
#[test]
fn capture_artifact_publication_retry_never_replays_hook_or_duplicates_result() {
    let f = fixture();
    let marker = f.marker("retry-capture");
    let run = snapshot_run(&f, SnapshotOperation::Capture, "success", &marker);
    crate::persistence::fail_next_capture_publication_for_test();
    assert!(f.application.advance_owner_continuation(run).is_err());
    let outcome = finish(&f, run);
    assert_eq!(outcome.outcome, RunOutcome::Succeeded);
    assert_eq!(outcome.artifacts.len(), 1);
    assert_eq!(
        report(&f, run, None),
        b"ordinary-report:snapshot_capture:success:"
    );
    assert_eq!(
        fs::read(marker.with_extension("invocations")).unwrap(),
        b"x"
    );
    let db = rusqlite::Connection::open(f.storage.join("database/pactrun.sqlite3")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM run_capture_results WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let snapshot = captured_snapshot(&f, run);
    assert_eq!(
        f.application
            .delete_object(&ObjectDeletion::Run {
                run,
                delete_artifacts: false
            })
            .unwrap(),
        ObjectDeletionResult::Blocked(DeletionBlock::ArtifactsRemain)
    );
    assert_eq!(
        f.application
            .delete_object(&ObjectDeletion::Run {
                run,
                delete_artifacts: true
            })
            .unwrap(),
        ObjectDeletionResult::Deleted
    );
    assert!(f.application.verify_snapshot(snapshot).is_ok());
}

// Test-ID: PR-TEST-0735
// Verifies: PR-REQ-0390, PR-REQ-0391, PR-REQ-0392
#[test]
fn cleanup_reports_survive_owner_loss_and_finalization_only_needs_no_parameters() {
    let f = fixture();
    let marker = f.marker("cleanup-crash");
    let status = Command::new(env::current_exe().unwrap())
        .args([
            "--exact",
            "hook::tests::operation_runtime::failures::operation_owner_worker",
            "--nocapture",
        ])
        .env("PACTRUN_OPERATION_OWNER_ROOT", &f.storage)
        .env("PACTRUN_OPERATION_OWNER_MARKER", &marker)
        .env(
            "PACTRUN_OPERATION_TEST_FAULT",
            "after_cleanup_boundary_commit",
        )
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(87));
    let runs = f.application.reconcile_lost_action_owners().unwrap();
    assert_eq!(runs.len(), 1);
    let old = runs[0];
    assert_eq!(report(&f, old, None), b"ordinary-report:cleanup:success:");
    let intent = f
        .application
        .resolve_deletion(&f.instance.name, None, DeletionMode::ManagedCleanup)
        .unwrap();
    let plan = f.application.compile_deletion(&intent, &[]).unwrap();
    assert!(matches!(plan.work, DeletionWork::FinalizationOnly { .. }));
    let run = f
        .application
        .accept_deletion_plan(
            plan,
            AdmissionOptions::default(),
            ActionCancellation::default(),
            HookRuntimePolicy::default(),
        )
        .unwrap();
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    assert!(
        f.application
            .load_instance(f.instance.id)
            .unwrap()
            .is_none()
    );
    assert_eq!(report(&f, old, None), b"ordinary-report:cleanup:success:");
    assert_eq!(
        fs::read(marker.with_extension("invocations")).unwrap(),
        b"x"
    );
    assert_eq!(
        f.application
            .delete_object(&ObjectDeletion::Revision(
                f.instance.active_revision.clone()
            ))
            .unwrap(),
        ObjectDeletionResult::Deleted
    );
    assert_eq!(report(&f, old, None), b"ordinary-report:cleanup:success:");
}

#[test]
fn operation_owner_worker() {
    let Some(root) = env::var_os("PACTRUN_OPERATION_OWNER_ROOT") else {
        return;
    };
    let marker = env::var_os("PACTRUN_OPERATION_OWNER_MARKER").unwrap();
    let args = vec![
        "--format".into(),
        "json".into(),
        "instance".into(),
        "delete".into(),
        "hook-fixture".into(),
        "--param".into(),
        format!("marker={}", Path::new(&marker).display()),
    ];
    let (code, output, error) = cli(Path::new(&root), &args);
    assert_eq!(code, 0, "{} {error}", String::from_utf8_lossy(&output));
}

// Test-ID: PR-TEST-0736
// Verifies: PR-REQ-0391, PR-REQ-0392
#[test]
fn cancelled_capture_keeps_accepted_diagnostic_report_without_publishing_snapshot() {
    let f = fixture();
    let marker = f.marker("cancelled-capture");
    let cancellation = ActionCancellation::default();
    let intent = SnapshotIntent {
        instance: f.instance.id,
        operation: SnapshotOperation::Capture,
        parameters: params("wait_cancel", &marker),
    };
    let plan = crate::workflow::compile_snapshot(
        &f.application,
        &crate::workflow::PlatformHostLauncherLookup,
        &intent,
        &[],
    )
    .unwrap();
    let run = f
        .application
        .accept_snapshot_plan(plan, AdmissionOptions::default(), cancellation.clone())
        .unwrap();
    assert!(!f.application.advance_owner_continuation(run).unwrap());
    thread::scope(|scope| {
        let ready = marker.with_extension("ready");
        let cancellation = cancellation.clone();
        scope.spawn(move || {
            let deadline = Instant::now() + WAIT_LIMIT;
            while !ready.exists() {
                assert!(Instant::now() < deadline, "Hook did not become ready");
                thread::sleep(Duration::from_millis(5));
            }
            cancellation.request();
        });
        assert!(
            f.application
                .execute_snapshot_if_ready(run, policy(Some(20_000), None, Some(5_000)))
                .unwrap()
        );
    });
    let outcome = finish(&f, run);
    assert_eq!(outcome.outcome, RunOutcome::Cancelled);
    assert_eq!(outcome.artifacts.len(), 1);
    assert_eq!(
        report(&f, run, None),
        b"ordinary-report:snapshot_capture:wait_cancel:"
    );
    let db = rusqlite::Connection::open(f.storage.join("database/pactrun.sqlite3")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM run_capture_results WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

// Test-ID: PR-TEST-0737
// Verifies: PR-REQ-0389
#[test]
fn explicit_old_new_old_migration_preserves_payloads_and_original_canonical_bytes() {
    let f = legacy_fixture();
    let p = persistence(&f.storage);
    let before = p
        .load_revision(&f.instance.active_revision)
        .unwrap()
        .unwrap();
    let bytes =
        crate::revision_content::encode_canonical_revision_core(&before.content.core).unwrap();
    let mut path = vec![f.instance.active_revision.clone()];
    let mut new = revision();
    new["migrations"] = json!([{"source_revision_digest":path[0].content_digest.to_string(),"transitions":[],"produces_target":[],"source_inputs":{"required":[{"role":"active","input_id":"secret_config"}]}}]);
    let directory = f.temporary.path().join("source");
    fs::write(directory.join("pactrun.yaml"), source(new)).unwrap();
    path.push(
        f.application
            .install_pack_source(
                &directory,
                &RevisionMetadataMutationBatch::new(vec![]).unwrap(),
            )
            .unwrap()
            .revision,
    );
    let mut old:Value=serde_json::from_str(&source(json!({"inputs":[{"id":"secret_config","required":true,"protection":"secret"}],
        "migrations":[{"source_revision_digest":path[1].content_digest.to_string(),"transitions":[],"requires_source":[{"role":"active","input_id":"secret_config"}],"requires_target":[],"produces_target":[]}]}))).unwrap();
    old["source_format"] = json!("1.0-alpha.2");
    fs::write(directory.join("pactrun.yaml"), old.to_string()).unwrap();
    path.push(
        f.application
            .install_pack_source(
                &directory,
                &RevisionMetadataMutationBatch::new(vec![]).unwrap(),
            )
            .unwrap()
            .revision,
    );
    let intent = TransitionRevision {
        instance: f.instance.id,
        expected_state_version: f.instance.state_version,
        source: path[0].clone(),
        target: path[2].clone(),
        path: MigrationPathSelection::Exact(path.clone()),
        operator_inputs: vec![],
        parameters: vec![],
        authorize_declassification: false,
    };
    let plan = crate::workflow::compile_migration(
        &f.application,
        &crate::workflow::PlatformHostLauncherLookup,
        &intent,
        &[],
    )
    .unwrap();
    let run = f
        .application
        .accept_migration_inputs(
            plan,
            false,
            ActionCancellation::default(),
            vec![],
            HookRuntimePolicy::default(),
        )
        .unwrap();
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    assert_eq!(
        f.application
            .load_instance(f.instance.id)
            .unwrap()
            .unwrap()
            .active_revision,
        path[2]
    );
    let exported = f
        .application
        .export_input(
            f.instance.id,
            &InputIdentity::parse("secret_config").unwrap(),
            true,
        )
        .unwrap();
    let mut actual = Vec::new();
    exported
        .bytes
        .try_clone_reader()
        .unwrap()
        .read_to_end(&mut actual)
        .unwrap();
    assert_eq!(actual, SECRET_BINDING);
    assert_eq!(
        crate::revision_content::encode_canonical_revision_core(
            &p.load_revision(&path[0]).unwrap().unwrap().content.core
        )
        .unwrap(),
        bytes
    );
    assert_eq!(
        p.load_revision(&path[0])
            .unwrap()
            .unwrap()
            .content
            .core
            .version(),
        FormatVersion::BASELINE
    );
    assert_eq!(
        p.load_revision(&path[1])
            .unwrap()
            .unwrap()
            .content
            .core
            .version()
            .to_string(),
        OPERATION_CORE_FORMAT
    );
    assert_eq!(
        p.load_revision(&path[2])
            .unwrap()
            .unwrap()
            .content
            .core
            .version(),
        FormatVersion::BASELINE
    );
}
