use super::*;
use crate::domain::*;
use std::ffi::OsString;

const WORKER: &str = "hook::tests::operation_runtime::operation_hook_worker";
#[path = "operation_failure_tests.rs"]
mod failures;
fn hook() -> Value {
    json!({"launch":{"kind":"direct","executable":"worker"},"args":["--exact",WORKER,"--nocapture"],"io":{"terminal":"none"}})
}
fn declarations() -> Value {
    json!([{"id":"marker","type":"string","sensitive":false},{"id":"mode","type":"string","sensitive":false,"default":"success"}])
}
fn source(revision: Value) -> String {
    json!({"source_format":"1.0-alpha.3","package_id":"00000000000000000000000000000071","revision":revision,
        "runtime_content":{"files":[{"id":"worker","source":worker_file_name(),"path":format!("bin/{}",worker_file_name()),"executable":true}]}}).to_string()
}
fn revision() -> Value {
    json!({"inputs":[{"id":"secret_config","protection":"secret"},{"id":"configuration"}],
        "actions":[
            {"id":"precheck","access":"observe","parameters":declarations(),"hook":hook(),"outputs":[{"id":"report"},{"id":"unused"}]},
            {"id":"needs_configuration","access":"observe","parameters":declarations(),"inputs":{"required":["configuration"]},"hook":hook(),"outputs":[]}],
        "snapshot":{
            "capture":{"access":"observe","parameters":declarations(),"hook":hook(),"outputs":[{"id":"report"}]},
            "restore":{"parameters":declarations(),"hook":hook(),"outputs":[{"id":"report"}]}},
        "cleanup":{"parameters":declarations(),"hook":hook(),"outputs":[{"id":"report"}]}})
}
fn fixture() -> RuntimeFixture {
    RuntimeFixture::with_source(|_, _, manifest| *manifest = source(revision()))
}
fn legacy_fixture() -> RuntimeFixture {
    RuntimeFixture::with_source(|_, _, manifest| {
        let mut legacy_hook = hook();
        legacy_hook["protocol_version"] = json!("1.0-alpha.1");
        let mut value:Value=serde_json::from_str(&source(json!({"inputs":[{"id":"secret_config","required":true,"protection":"secret"}],
            "actions":[{"id":"precheck","access":"observe","parameters":declarations(),"hook":legacy_hook,"outputs":[{"id":"report"}]}]}))).unwrap();
        value["source_format"] = json!("1.0-alpha.2");
        *manifest = value.to_string();
    })
}
fn cli(root: &Path, args: &[String]) -> (i32, Vec<u8>, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = crate::cli::run(
        args.iter().map(OsString::from).collect(),
        Some(root.as_os_str().to_owned()),
        &mut io::empty(),
        &mut out,
        &mut err,
    );
    (code, out, String::from_utf8(err).unwrap())
}
fn params(mode: &str, marker: &Path) -> Vec<RawParameterInput> {
    [
        ("mode", mode.to_owned()),
        ("marker", marker.to_str().unwrap().to_owned()),
    ]
    .into_iter()
    .map(|(id, text)| RawParameterInput {
        id: ParameterIdentity::parse(id).unwrap(),
        text,
        source: ParameterTextSource::Ordinary,
    })
    .collect()
}
fn finish(f: &RuntimeFixture, run: RunId) -> RunOutcomeView {
    let deadline = Instant::now() + WAIT_LIMIT;
    loop {
        if let RunState::Finished(outcome) = persistence(&f.storage)
            .load_managed_run(run)
            .unwrap()
            .unwrap()
            .state
        {
            return outcome;
        }
        assert!(Instant::now() < deadline, "operation did not terminalize");
        f.application.advance_owner_continuation(run).unwrap();
    }
}
fn report(f: &RuntimeFixture, run: RunId, edge: Option<&MigrationEdgeIdentity>) -> Vec<u8> {
    let mut bytes = Vec::new();
    persistence(&f.storage)
        .open_run_artifact_at(
            run,
            &ManagedOutputIdentity::parse("report").unwrap(),
            edge,
            &mut bytes,
        )
        .unwrap();
    bytes
}
fn session_at(marker: &Path) -> Value {
    serde_json::from_slice(&fs::read(marker).unwrap()).unwrap()
}
fn captured_snapshot(f: &RuntimeFixture, run: RunId) -> SnapshotId {
    let db = rusqlite::Connection::open(f.storage.join("database/pactrun.sqlite3")).unwrap();
    SnapshotId::from_bytes(
        db.query_row(
            "SELECT snapshot_id FROM run_capture_results WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |r| r.get::<_, Vec<u8>>(0),
        )
        .unwrap()
        .try_into()
        .unwrap(),
    )
}

// Test-ID: PR-TEST-0774
// Verifies: PR-REQ-0402, PR-REQ-0403
#[test]
fn input_history_restore_records_snapshot_context_without_a_snapshot_pin() {
    let f = fixture();
    let capture = snapshot_run(
        &f,
        SnapshotOperation::Capture,
        "success",
        &f.marker("history-capture"),
    );
    assert_eq!(finish(&f, capture).outcome, RunOutcome::Succeeded);
    let snapshot = captured_snapshot(&f, capture);
    let restore = snapshot_run(
        &f,
        SnapshotOperation::Restore(snapshot),
        "success",
        &f.marker("history-restore"),
    );
    assert_eq!(finish(&f, restore).outcome, RunOutcome::Succeeded);
    let p = persistence(&f.storage);
    let history = p.input_history(restore).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].scope, InputHistoryScope::Snapshot);
    assert_eq!(history[0].snapshot, Some(snapshot));
    assert!(history[0].snapshot_integrity.is_some());
    let integrity = history[0].snapshot_integrity.as_ref().unwrap().as_str();
    assert!(!format!("{history:?}").contains(integrity));
    assert!(
        history[0].inputs.iter().any(
            |i| i.input.as_str() == "secret_config" && i.origin == InputHistoryOrigin::Snapshot
        )
    );
    assert!(
        history[0]
            .inputs
            .iter()
            .any(|i| i.input.as_str() == "configuration" && i.origin == InputHistoryOrigin::Absent)
    );
    assert_eq!(
        f.application
            .delete_object(&ObjectDeletion::Snapshot(snapshot))
            .unwrap(),
        ObjectDeletionResult::Deleted
    );
    assert_eq!(p.input_history(restore).unwrap(), history);
}
fn snapshot_run(
    f: &RuntimeFixture,
    operation: SnapshotOperation,
    mode: &str,
    marker: &Path,
) -> RunId {
    let intent = SnapshotIntent {
        instance: f.instance.id,
        operation,
        parameters: params(mode, marker),
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
        .accept_snapshot_plan(
            plan,
            AdmissionOptions::default(),
            ActionCancellation::default(),
        )
        .unwrap();
    assert!(!f.application.advance_owner_continuation(run).unwrap());
    assert!(
        f.application
            .execute_snapshot_if_ready(run, policy(Some(20_000), Some(20_000), Some(500)))
            .unwrap()
    );
    run
}

// Test-ID: PR-TEST-0723
// Verifies: PR-REQ-0379, PR-REQ-0381, PR-REQ-0383, PR-REQ-0387, PR-REQ-0391
#[test]
fn native_precheck_ignores_unrelated_inputs_and_materializes_only_its_grants() {
    let f = fixture();
    let marker = f.marker("precheck-session");
    let intent = f
        .application
        .resolve_action(
            &f.instance.name,
            &ActionIdentity::parse("precheck").unwrap(),
            params("success", &marker),
        )
        .unwrap();
    let plan = f.application.compile_action(&intent, &[]).unwrap();
    assert!(plan.required_inputs_satisfied());
    assert!(plan.active_bindings().is_empty());
    let admitted = f
        .application
        .accept_and_admit_action(&plan, AdmissionOptions::default())
        .unwrap();
    let run = admitted.run();
    f.application.execute_admitted_action(
        admitted,
        policy(Some(20_000), Some(20_000), Some(500)),
        ActionCancellation::default(),
    );
    let outcome = finish(&f, run);
    assert_eq!(outcome.outcome, RunOutcome::Succeeded);
    assert_eq!(outcome.artifacts.len(), 1);
    assert_eq!(report(&f, run, None), b"ordinary-report:action:success:");
    let session = session_at(&marker);
    assert_eq!(session["protocol_version"], "1.0-alpha.2");
    assert_eq!(session["operation"]["bindings"], json!([]));
    let denied = f.marker("must-not-launch");
    let intent = f
        .application
        .resolve_action(
            &f.instance.name,
            &ActionIdentity::parse("needs_configuration").unwrap(),
            params("success", &denied),
        )
        .unwrap();
    let plan = f.application.compile_action(&intent, &[]).unwrap();
    assert!(!plan.required_inputs_satisfied());
    assert!(
        f.application
            .accept_and_admit_action(&plan, AdmissionOptions::default())
            .is_err()
    );
    assert!(!denied.exists());
}

// Test-ID: PR-TEST-0724
// Verifies: PR-REQ-0381, PR-REQ-0383, PR-REQ-0384, PR-REQ-0385, PR-REQ-0386, PR-REQ-0391
#[test]
fn native_capture_and_restore_keep_complete_state_outside_the_hook_read_view() {
    let f = fixture();
    let captured = f.marker("capture-session");
    let run = snapshot_run(&f, SnapshotOperation::Capture, "success", &captured);
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    assert_eq!(
        report(&f, run, None),
        b"ordinary-report:snapshot_capture:success:"
    );
    let db = rusqlite::Connection::open(f.storage.join("database/pactrun.sqlite3")).unwrap();
    let id = SnapshotId::from_bytes(
        db.query_row(
            "SELECT snapshot_id FROM run_capture_results WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |r| r.get::<_, Vec<u8>>(0),
        )
        .unwrap()
        .try_into()
        .unwrap(),
    );
    let bytes = db
        .query_row(
            "SELECT canonical_manifest FROM snapshots WHERE snapshot_id=?1",
            [id.as_bytes().as_slice()],
            |r| r.get::<_, Vec<u8>>(0),
        )
        .unwrap();
    let manifest = crate::snapshot_integrity::decode_snapshot_manifest(
        SnapshotIntegrityVersion::BASELINE,
        &bytes,
        None,
    )
    .unwrap();
    assert_eq!(manifest.manifest().managed_bindings().len(), 2);
    assert!(
        manifest
            .manifest()
            .managed_bindings()
            .iter()
            .any(|b| b.input_id.as_str() == "secret_config"
                && matches!(b.state, SnapshotBindingState::Bound(_)))
    );
    assert_eq!(session_at(&captured)["operation"]["bindings"], json!([]));
    let current = f.application.load_instance(f.instance.id).unwrap().unwrap();
    f.application
        .set_input(
            f.instance.id,
            InputIdentity::parse("secret_config").unwrap(),
            current.state_version,
            Box::new(Cursor::new(b"changed".to_vec())),
        )
        .unwrap();
    let restored = f.marker("restore-session");
    let restore = snapshot_run(&f, SnapshotOperation::Restore(id), "success", &restored);
    assert_eq!(finish(&f, restore).outcome, RunOutcome::Succeeded);
    let exported = f
        .application
        .export_input(
            f.instance.id,
            &InputIdentity::parse("secret_config").unwrap(),
            true,
        )
        .unwrap();
    let mut bytes = Vec::new();
    exported
        .bytes
        .try_clone_reader()
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, SECRET_BINDING);
    assert_eq!(session_at(&restored)["operation"]["bindings"], json!([]));
    assert_eq!(
        report(&f, restore, None),
        b"ordinary-report:snapshot_restore:success:"
    );
}

// Test-ID: PR-TEST-0725
// Verifies: PR-REQ-0390, PR-REQ-0391, PR-REQ-0392
#[test]
fn native_cleanup_reports_survive_retirement_and_failed_cleanup_can_retry() {
    let f = fixture();
    for mode in ["failure", "success"] {
        let marker = f.marker(&format!("cleanup-{mode}"));
        let mut intent = f
            .application
            .resolve_deletion(&f.instance.name, None, DeletionMode::ManagedCleanup)
            .unwrap();
        intent.parameters = params(mode, &marker);
        let plan = f.application.compile_deletion(&intent, &[]).unwrap();
        let run = f
            .application
            .accept_deletion_plan(
                plan,
                AdmissionOptions::default(),
                ActionCancellation::default(),
                policy(Some(20_000), Some(20_000), Some(500)),
            )
            .unwrap();
        assert!(!f.application.advance_owner_continuation(run).unwrap());
        assert!(f.application.execute_ready_deletion(run).unwrap());
        let outcome = finish(&f, run);
        assert_eq!(
            outcome.outcome,
            if mode == "failure" {
                RunOutcome::Failed
            } else {
                RunOutcome::Succeeded
            }
        );
        assert_eq!(
            report(&f, run, None),
            format!("ordinary-report:cleanup:{mode}:").as_bytes()
        );
        assert_eq!(
            f.application
                .load_instance(f.instance.id)
                .unwrap()
                .is_some(),
            mode == "failure"
        );
    }
}

// Test-ID: PR-TEST-0726
// Verifies: PR-REQ-0381, PR-REQ-0383, PR-REQ-0388, PR-REQ-0390, PR-REQ-0391, PR-REQ-0392
#[test]
fn native_migration_keeps_edge_parameters_and_same_name_reports_independent() {
    for second_mode in ["success", "failure"] {
        let f = fixture();
        let mut path = vec![f.instance.active_revision.clone()];
        for first in [true, false] {
            let mut target = revision();
            target["inputs"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"report"}));
            let mut parameters = declarations();
            parameters
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"choice","type":"string","sensitive":false}));
            target["migrations"] = json!([{"source_revision_digest":path.last().unwrap().content_digest.to_string(),"transitions":[],
                "source_inputs":{"optional":[{"role":"active","input_id":"configuration"}]},"target_inputs":{"optional":["configuration"]},
                "produces_target":if first{vec!["report"]}else{vec![]},"parameters":parameters,"hook":hook(),"outputs":[{"id":"report"}]}]);
            let source_dir = f.temporary.path().join("source");
            fs::write(source_dir.join("pactrun.yaml"), source(target)).unwrap();
            path.push(
                f.application
                    .install_pack_source(
                        &source_dir,
                        &RevisionMetadataMutationBatch::new(vec![]).unwrap(),
                    )
                    .unwrap()
                    .revision,
            );
        }
        let markers = [f.marker("first-edge"), f.marker("second-edge")];
        let mut intent = TransitionRevision {
            instance: f.instance.id,
            expected_state_version: f.instance.state_version,
            source: path[0].clone(),
            target: path[2].clone(),
            path: MigrationPathSelection::Exact(path.clone()),
            operator_inputs: Vec::new(),
            authorize_declassification: false,
            parameters: Vec::new(),
        };
        assert!(f.application.list_runs(f.instance.id).unwrap().is_empty());
        assert!(
            crate::workflow::compile_migration(
                &f.application,
                &crate::workflow::PlatformHostLauncherLookup,
                &intent,
                &[]
            )
            .is_err()
        );
        assert!(markers.iter().all(|marker| !marker.exists()));
        for index in 0..2 {
            let edge = MigrationEdgeIdentity {
                source: path[index].content_digest,
                target: path[index + 1].content_digest,
            };
            let mut values = params(
                if index == 0 { "success" } else { second_mode },
                &markers[index],
            );
            values.push(RawParameterInput {
                id: ParameterIdentity::parse("choice").unwrap(),
                text: if index == 0 { "first" } else { "second" }.into(),
                source: ParameterTextSource::Ordinary,
            });
            intent
                .parameters
                .extend(values.into_iter().map(|parameter| MigrationParameterInput {
                    edge: edge.clone(),
                    parameter,
                }));
        }
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
                Vec::new(),
                policy(Some(20_000), Some(20_000), Some(500)),
            )
            .unwrap();
        let outcome = finish(&f, run);
        assert_eq!(
            outcome.outcome,
            if second_mode == "success" {
                RunOutcome::Succeeded
            } else {
                RunOutcome::Failed
            }
        );
        assert_eq!(outcome.artifacts.len(), 2);
        for index in 0..2 {
            let edge = MigrationEdgeIdentity {
                source: path[index].content_digest,
                target: path[index + 1].content_digest,
            };
            assert_eq!(
                report(&f, run, Some(&edge)),
                format!(
                    "ordinary-report:migration:{}:{}",
                    if index == 0 { "success" } else { second_mode },
                    if index == 0 { "first" } else { "second" }
                )
                .as_bytes()
            );
            let session = session_at(&markers[index]);
            assert_eq!(session["operation"]["source_bindings"], json!([]));
            assert_eq!(session["operation"]["target_bindings"], json!([]));
        }
        let view = f.application.load_instance(f.instance.id).unwrap().unwrap();
        assert_eq!(
            view.active_revision,
            path[if second_mode == "success" { 2 } else { 1 }]
        );
        let exported = f
            .application
            .export_input(
                f.instance.id,
                &InputIdentity::parse("report").unwrap(),
                false,
            )
            .unwrap();
        let mut bytes = Vec::new();
        exported
            .bytes
            .try_clone_reader()
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, b"converted-input");
        if second_mode == "success" {
            let edge = MigrationEdgeIdentity {
                source: path[0].content_digest,
                target: path[1].content_digest,
            };
            let selected = format!("{}/{}", edge.source, edge.target);
            let output = f.marker("exported-report");
            let (code, body, err) = cli(
                &f.storage,
                &[
                    "--format".into(),
                    "json".into(),
                    "run".into(),
                    "artifact".into(),
                    "export".into(),
                    run.to_string(),
                    "report".into(),
                    "--edge".into(),
                    selected.clone(),
                    "--output".into(),
                    output.to_str().unwrap().into(),
                    "--authorize-sensitive-export".into(),
                ],
            );
            assert_eq!(code, 0, "{} {err}", String::from_utf8_lossy(&body));
            let body: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body["format_version"], "1.0-alpha.4");
            assert_eq!(
                body["result"]["edge"]["source_revision_digest"],
                edge.source.to_string()
            );
            assert_eq!(
                fs::read(output).unwrap(),
                b"ordinary-report:migration:success:first"
            );
            let args: Vec<String> = vec![
                "--format".into(),
                "json".into(),
                "--machine-version".into(),
                "1.0-alpha.3".into(),
                "run".into(),
                "artifact".into(),
                "delete".into(),
                run.to_string(),
                "report".into(),
                "--edge".into(),
                selected.clone(),
            ];
            let (code, body, _) = cli(&f.storage, &args);
            assert_eq!(code, 1);
            assert_eq!(
                serde_json::from_slice::<Value>(&body).unwrap()["format_version"],
                "1.0-alpha.3"
            );
            assert_eq!(
                report(&f, run, Some(&edge)),
                b"ordinary-report:migration:success:first"
            );
            let args: Vec<String> = vec![
                "--format".into(),
                "json".into(),
                "run".into(),
                "artifact".into(),
                "delete".into(),
                run.to_string(),
                "report".into(),
                "--edge".into(),
                selected,
            ];
            assert_eq!(cli(&f.storage, &args).0, 0);
            let second = MigrationEdgeIdentity {
                source: path[1].content_digest,
                target: path[2].content_digest,
            };
            assert_eq!(
                report(&f, run, Some(&second)),
                b"ordinary-report:migration:success:second"
            );
        }
    }
}

// Test-ID: PR-TEST-0727
// Verifies: PR-REQ-0388
#[test]
fn machine_version_prefix_errors_and_jsonl_keep_the_selected_envelope_without_opening_storage() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    let tmp = tempfile::tempdir_in(parent).unwrap();
    let root = tmp.path().join("absent");
    for (args, code, version) in [
        (vec!["--format", "json", "--version"], 0, "1.0-alpha.4"),
        (
            vec![
                "--machine-version",
                "1.0-alpha.3",
                "--format",
                "json",
                "--version",
            ],
            0,
            "1.0-alpha.3",
        ),
        (
            vec![
                "--format",
                "json",
                "--machine-version",
                "1.0-alpha.3",
                "unknown-command",
            ],
            2,
            "1.0-alpha.3",
        ),
        (
            vec![
                "--format",
                "json",
                "--machine-version",
                "1.0-alpha.99",
                "--version",
            ],
            2,
            "1.0-alpha.4",
        ),
        (
            vec![
                "--format",
                "json",
                "--machine-version",
                "1.0-alpha.3",
                "--machine-version",
                "1.0-alpha.4",
                "--version",
            ],
            2,
            "1.0-alpha.3",
        ),
    ] {
        let (actual, body, err) = cli(&root, &args.iter().map(|a| (*a).into()).collect::<Vec<_>>());
        assert_eq!(actual, code, "{args:?}: {err}");
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap()["format_version"],
            version
        );
        assert!(!root.exists());
    }
    let (code, body, _) = cli(
        &root,
        &[
            "--machine-version",
            "1.0-alpha.3",
            "--format",
            "jsonl",
            "--version",
        ]
        .map(String::from),
    );
    assert_eq!(code, 0);
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["format_version"], "1.0-alpha.3");
    assert_eq!(value["response"]["format_version"], "1.0-alpha.3");
    let (code, body, error) = cli(
        &root,
        &["--machine-version", "1.0-alpha.3", "--version"].map(String::from),
    );
    assert_eq!(code, 2);
    assert!(body.is_empty());
    assert!(error.contains("requires explicit"));
    assert!(!root.exists());
}

// Test-ID: PR-TEST-0728
// Verifies: PR-REQ-0381, PR-REQ-0383, PR-REQ-0388
#[test]
fn new_cli_readiness_is_per_operation_and_legacy_invocation_refuses_before_a_run() {
    let f = fixture();
    let (code, body, err) = cli(
        &f.storage,
        &["--format", "json", "instance", "show", "hook-fixture"].map(String::from),
    );
    assert_eq!(code, 0, "{err}");
    let value: Value = serde_json::from_slice(&body).unwrap();
    let result = &value["result"];
    assert!(result.get("required_inputs_satisfied").is_none());
    assert!(result.get("legacy_required_inputs").is_none());
    let requirements = result["input_requirements"].as_array().unwrap();
    assert_eq!(
        requirements
            .iter()
            .find(|r| r["operation"]["action_id"] == "precheck")
            .unwrap()["state"],
        "satisfied"
    );
    assert_eq!(
        requirements
            .iter()
            .find(|r| r["operation"]["action_id"] == "needs_configuration")
            .unwrap()["missing"][0]["input_id"],
        "configuration"
    );
    assert_eq!(
        requirements
            .iter()
            .find(|r| r["operation"]["kind"] == "snapshot_restore")
            .unwrap()["state"],
        "not_evaluated"
    );
    assert!(
        result["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|input| input.get("required").is_none())
    );
    assert!(
        result["related_revisions"][0]["declarations"]["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|input| input.get("required").is_none())
    );
    let marker = f.marker("legacy-must-not-launch");
    let (code, body, _) = cli(
        &f.storage,
        &[
            "--format".into(),
            "json".into(),
            "--machine-version".into(),
            "1.0-alpha.3".into(),
            "invoke".into(),
            "hook-fixture".into(),
            "precheck".into(),
            "--param".into(),
            format!("marker={}", marker.display()),
        ],
    );
    assert_eq!(code, 1);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap()["format_version"],
        "1.0-alpha.3"
    );
    assert!(!marker.exists());
    assert!(f.application.list_runs(f.instance.id).unwrap().is_empty());
    let (code, body, err) = cli(
        &f.storage,
        &[
            "--format".into(),
            "json".into(),
            "invoke".into(),
            "hook-fixture".into(),
            "precheck".into(),
            "--plan".into(),
            "--param".into(),
            format!("marker={}", marker.display()),
        ],
    );
    assert_eq!(code, 0, "{err}");
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert!(value["result"].get("required_inputs_satisfied").is_none());
    assert!(value["result"].get("missing_input_ids").is_none());
    assert_eq!(
        value["result"]["input_requirements"][0]["state"],
        "satisfied"
    );
    assert!(!marker.exists());
}

// Test-ID: PR-TEST-0729
// Verifies: PR-REQ-0388, PR-REQ-0389
#[test]
fn legacy_machine_keeps_old_fields_and_all_live_event_envelopes() {
    let f = legacy_fixture();
    let (code, body, err) = cli(
        &f.storage,
        &[
            "--format",
            "json",
            "--machine-version",
            "1.0-alpha.3",
            "instance",
            "show",
            "hook-fixture",
        ]
        .map(String::from),
    );
    assert_eq!(code, 0, "{err}");
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["result"]["required_inputs_satisfied"], true);
    assert!(value["result"].get("input_requirements").is_none());
    let (code, body, err) = cli(
        &f.storage,
        &["--format", "json", "instance", "show", "hook-fixture"].map(String::from),
    );
    assert_eq!(code, 0, "{err}");
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert!(value["result"].get("required_inputs_satisfied").is_none());
    assert_eq!(value["result"]["legacy_required_inputs"]["satisfied"], true);
    let marker = f.marker("legacy-live");
    let (code, body, err) = cli(
        &f.storage,
        &[
            "--format".into(),
            "jsonl".into(),
            "--machine-version".into(),
            "1.0-alpha.3".into(),
            "invoke".into(),
            "hook-fixture".into(),
            "precheck".into(),
            "--param".into(),
            format!("marker={}", marker.display()),
        ],
    );
    assert_eq!(code, 0, "{} {err}", String::from_utf8_lossy(&body));
    let records: Vec<Value> = String::from_utf8(body)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(records.len() > 1);
    assert!(
        records
            .iter()
            .all(|record| record["format_version"] == "1.0-alpha.3")
    );
    assert_eq!(
        records.last().unwrap()["response"]["format_version"],
        "1.0-alpha.3"
    );
    assert_eq!(session_at(&marker)["protocol_version"], "1.0-alpha.1");
}

// Test-ID: PR-TEST-0730
// Verifies: PR-REQ-0381, PR-REQ-0382
#[test]
fn operation_requiredness_allows_removal_and_counts_zero_length_bindings_as_present() {
    let f = fixture();
    let input = InputIdentity::parse("configuration").unwrap();
    let version = f
        .application
        .set_input(
            f.instance.id,
            input.clone(),
            f.instance.state_version,
            Box::new(Cursor::new(Vec::<u8>::new())),
        )
        .unwrap();
    let marker = f.marker("not-invoked");
    let resolve = || {
        f.application
            .resolve_action(
                &f.instance.name,
                &ActionIdentity::parse("needs_configuration").unwrap(),
                params("success", &marker),
            )
            .unwrap()
    };
    assert!(
        f.application
            .compile_action(&resolve(), &[])
            .unwrap()
            .required_inputs_satisfied()
    );
    f.application
        .delete_input(f.instance.id, &input, version)
        .unwrap();
    let plan = f.application.compile_action(&resolve(), &[]).unwrap();
    assert!(!plan.required_inputs_satisfied());
    assert_eq!(plan.missing_input_ids(), &[input]);
    assert!(!marker.exists());
}

// Test-ID: PR-TEST-0731
// Verifies: PR-REQ-0384, PR-REQ-0385, PR-REQ-0386
#[test]
fn restore_refuses_missing_snapshot_inputs_before_initialization_and_keeps_exact_revision_rule() {
    let f = RuntimeFixture::with_source(|_, _, manifest| {
        let mut value = revision();
        value["snapshot"]["restore"]["inputs"] = json!({"required":["configuration"]});
        *manifest = source(value);
    });
    let run = snapshot_run(
        &f,
        SnapshotOperation::Capture,
        "success",
        &f.marker("captured"),
    );
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    let id = captured_snapshot(&f, run);
    f.application
        .set_input(
            f.instance.id,
            InputIdentity::parse("configuration").unwrap(),
            f.instance.state_version,
            Box::new(Cursor::new(b"current-is-not-snapshot".to_vec())),
        )
        .unwrap();
    let marker = f.marker("must-not-restore");
    let intent = SnapshotIntent {
        instance: f.instance.id,
        operation: SnapshotOperation::Restore(id),
        parameters: params("success", &marker),
    };
    assert!(
        crate::workflow::compile_snapshot(
            &f.application,
            &crate::workflow::PlatformHostLauncherLookup,
            &intent,
            &[]
        )
        .is_err()
    );
    let name = InstanceName::parse("must-not-create").unwrap();
    assert!(
        f.application
            .create_instance_for_restore(name.clone(), f.instance.active_revision.clone(), id)
            .is_err()
    );
    assert!(
        f.application
            .resolve_instance_name(&name)
            .unwrap()
            .is_none()
    );
    assert!(!marker.exists());
    let source_dir = f.temporary.path().join("source");
    fs::write(source_dir.join("pactrun.yaml"), source(revision())).unwrap();
    let other = f
        .application
        .install_pack_source(
            &source_dir,
            &RevisionMetadataMutationBatch::new(vec![]).unwrap(),
        )
        .unwrap()
        .revision;
    assert_ne!(other, f.instance.active_revision);
    let target = f
        .application
        .create_instance(
            InstanceName::parse("different-revision").unwrap(),
            other,
            vec![],
        )
        .unwrap();
    let intent = SnapshotIntent {
        instance: target.id,
        operation: SnapshotOperation::Restore(id),
        parameters: params("success", &marker),
    };
    assert!(
        crate::workflow::compile_snapshot(
            &f.application,
            &crate::workflow::PlatformHostLauncherLookup,
            &intent,
            &[]
        )
        .is_err()
    );
    assert!(!marker.exists());
}

// Test-ID: PR-TEST-0732
// Verifies: PR-REQ-0386, PR-REQ-0391, PR-REQ-0392
#[test]
fn missing_submitted_report_prevents_snapshot_publication_and_restore_replacement() {
    let f = fixture();
    let failed = snapshot_run(
        &f,
        SnapshotOperation::Capture,
        "missing_report",
        &f.marker("capture-report-failure"),
    );
    let outcome = finish(&f, failed);
    assert_eq!(outcome.outcome, RunOutcome::Failed);
    assert!(outcome.artifacts.is_empty());
    assert_eq!(
        outcome.primary_failure.unwrap().failure.error.code(),
        "managed_output_publication_failed"
    );
    let db = rusqlite::Connection::open(f.storage.join("database/pactrun.sqlite3")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM run_capture_results WHERE run_id=?1",
            [failed.as_bytes().as_slice()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    let good = snapshot_run(
        &f,
        SnapshotOperation::Capture,
        "success",
        &f.marker("good-capture"),
    );
    assert_eq!(finish(&f, good).outcome, RunOutcome::Succeeded);
    let id = captured_snapshot(&f, good);
    let version = f
        .application
        .load_instance(f.instance.id)
        .unwrap()
        .unwrap()
        .state_version;
    f.application
        .set_input(
            f.instance.id,
            InputIdentity::parse("secret_config").unwrap(),
            version,
            Box::new(Cursor::new(b"must-remain".to_vec())),
        )
        .unwrap();
    let failed = snapshot_run(
        &f,
        SnapshotOperation::Restore(id),
        "missing_report",
        &f.marker("restore-report-failure"),
    );
    assert_eq!(finish(&f, failed).outcome, RunOutcome::Failed);
    let exported = f
        .application
        .export_input(
            f.instance.id,
            &InputIdentity::parse("secret_config").unwrap(),
            true,
        )
        .unwrap();
    let mut bytes = Vec::new();
    exported
        .bytes
        .try_clone_reader()
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, b"must-remain");
}

#[test]
fn operation_hook_worker() {
    let Ok(transport) = env::var(TRANSPORT_ENVIRONMENT) else {
        return;
    };
    let mut stream = connect_hook(&transport, &env::var(ENDPOINT_ENVIRONMENT).unwrap());
    let mut preamble = vec![0; crate::hook::protocol::PREAMBLE.len()];
    stream.read_exact(&mut preamble).unwrap();
    assert!(
        preamble == crate::hook::protocol::PREAMBLE
            || preamble == crate::hook::protocol::OPERATION_PREAMBLE
    );
    let session = read_frame(&mut stream).unwrap();
    let params = session["parameters"].as_array().unwrap();
    let value = |id: &str| {
        params
            .iter()
            .find(|p| p["parameter_id"] == id)
            .and_then(|p| p["value"].as_str())
            .unwrap_or("")
    };
    let mode = value("mode");
    let choice = value("choice");
    fs::write(value("marker"), serde_json::to_vec(&session).unwrap()).unwrap();
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(Path::new(value("marker")).with_extension("invocations"))
        .unwrap()
        .write_all(b"x")
        .unwrap();
    assert_eq!(
        session["protocol_version"],
        if preamble == crate::hook::protocol::PREAMBLE {
            "1.0-alpha.1"
        } else {
            "1.0-alpha.2"
        }
    );
    stream.write_all(&preamble).unwrap();
    write_frame(
        &mut stream,
        &json!({"type":"session_ready","protocol_version":session["protocol_version"],"session_id":session["session_id"]}),
    );
    let kind = session["operation"]["kind"].as_str().unwrap();
    let empty_reads = if kind == "migration" {
        session["operation"]["source_bindings"]
            .as_array()
            .unwrap()
            .is_empty()
            && session["operation"]["target_bindings"]
                .as_array()
                .unwrap()
                .is_empty()
    } else {
        session["operation"]["bindings"]
            .as_array()
            .unwrap()
            .is_empty()
    };
    if empty_reads {
        let parent = Path::new(session["workspace"]["root_path"].as_str().unwrap())
            .parent()
            .unwrap();
        let leaves: &[&str] = if kind == "migration" {
            &["source", "target"]
        } else {
            &["bindings"]
        };
        for leaf in leaves {
            let root = parent.join(leaf);
            if root.exists() {
                assert!(
                    fs::read_dir(root).unwrap().next().is_none(),
                    "ungranted Input materialized in Hook-facing staging"
                );
            }
        }
    }
    let output = session["operation"]["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["output_id"] == "report")
        .unwrap();
    fs::write(
        output["staged_path"].as_str().unwrap(),
        format!("ordinary-report:{kind}:{mode}:{choice}"),
    )
    .unwrap();
    if mode == "missing_report" {
        fs::remove_file(output["staged_path"].as_str().unwrap()).unwrap();
    }
    if mode == "wait_cancel" {
        fs::write(Path::new(value("marker")).with_extension("ready"), b"ready").unwrap();
        let control = read_frame(&mut stream).unwrap();
        assert_eq!(control["type"], "cancel");
        write_frame(
            &mut stream,
            &json!({"type":"cancel_ack","control_id":control["control_id"]}),
        );
    }
    if mode.starts_with("transform_") {
        write_frame(
            &mut stream,
            &json!({"type":"request","request_id":1,"request":{"kind":"enter_recovery_risk"}}),
        );
        assert_eq!(read_frame(&mut stream).unwrap()["risk_state"], "open");
        let authorities = session["service_authorities"].as_array().unwrap();
        let source = authorities
            .iter()
            .find(|a| a["reference"]["view"] == "source")
            .unwrap();
        let target = authorities
            .iter()
            .find(|a| a["reference"]["view"] == "target")
            .unwrap();
        let mut bytes = b"transformed:".to_vec();
        bytes.extend(fs::read(source["path"].as_str().unwrap()).unwrap());
        fs::write(target["path"].as_str().unwrap(), bytes).unwrap();
        write_frame(
            &mut stream,
            &json!({"type":"target_ready","commit_handle":session["target_commit"]["handle"],"produced_target_outputs":[],"produced_outputs":[output["handle"]]}),
        );
        let receipt = read_frame(&mut stream).unwrap();
        assert_eq!(receipt["type"], "target_ready_received");
        assert!(receipt.get("status").is_none());
        if mode == "transform_nonzero" {
            std::process::exit(12);
        }
        return;
    }
    let failed = matches!(mode, "failure" | "wait_cancel");
    let mut completed = json!({"type":"complete","operation":kind,"status":if failed{"failure"}else{"success"},"produced_outputs":[output["handle"]]});
    if kind == "snapshot_capture" {
        completed["service_content"] = json!([]);
    }
    if kind == "migration" {
        let mut targets = Vec::new();
        if !failed {
            for output in session["operation"]["target_outputs"].as_array().unwrap() {
                fs::write(output["staged_path"].as_str().unwrap(), b"converted-input").unwrap();
                targets.push(output["handle"].clone());
            }
        }
        completed["produced_target_outputs"] = json!(targets);
    }
    write_frame(&mut stream, &completed);
    assert_eq!(
        read_frame(&mut stream).unwrap()["type"],
        "completion_accepted"
    );
}
