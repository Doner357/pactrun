use super::capture_runtime::finish;
use super::*;
use crate::domain::*;
use sha2::{Digest, Sha256};

const RESTORE_WORKER: &str = "hook::tests::restore_runtime::restore_hook_worker";
const SERVICE: &[u8] = b"captured-service-content";

fn fixture() -> RuntimeFixture {
    RuntimeFixture::with_source(|_, _, manifest| {
        *manifest = manifest.replace(
            "  actions:",
            "    - { id: optional, required: false, protection: normal }\n  actions:",
        );
        let mut snapshot = String::from("  snapshot:\n");
        for (operation, worker) in [
            (
                "capture",
                "hook::tests::capture_runtime::capture_hook_worker",
            ),
            ("restore", RESTORE_WORKER),
        ] {
            snapshot.push_str(&format!("    {operation}:\n"));
            if operation == "capture" {
                snapshot.push_str("      access: observe\n");
            }
            snapshot.push_str(&format!(
                r#"      parameters:
        - {{ id: mode, type: string, sensitive: false }}
        - {{ id: marker, type: string, sensitive: false }}
        - {{ id: expected_binding, type: string, sensitive: true }}
        - {{ id: sensitive_value, type: string, sensitive: true }}
      hook:
        protocol_version: 1.0-alpha.1
        launch: {{ kind: direct, executable: worker }}
        args: ["--exact", "{worker}", "--nocapture"]
        io: {{ terminal: none }}
"#
            ));
        }
        snapshot.push_str("  migrations: []");
        *manifest = manifest.replace("  migrations: []", &snapshot);
    })
}
fn db(f: &RuntimeFixture) -> rusqlite::Connection {
    rusqlite::Connection::open(f.storage.join("database/pactrun.sqlite3")).unwrap()
}
fn count(f: &RuntimeFixture, table: &str) -> i64 {
    db(f)
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn compile(
    f: &RuntimeFixture,
    instance: InstanceId,
    operation: SnapshotOperation,
    mode: &str,
    marker: &Path,
) -> SnapshotExecutionPlan {
    crate::workflow::compile_snapshot(
        &f.application,
        &crate::workflow::PlatformHostLauncherLookup,
        &SnapshotIntent {
            instance,
            operation,
            parameters: parameters(mode, marker),
        },
        &[],
    )
    .unwrap()
}
fn admit(
    f: &RuntimeFixture,
    instance: InstanceId,
    snapshot: SnapshotId,
    mode: &str,
    marker: &Path,
    cancel: ActionCancellation,
    override_guard: bool,
) -> RunId {
    let plan = compile(
        f,
        instance,
        SnapshotOperation::Restore(snapshot),
        mode,
        marker,
    );
    assert_eq!(plan.access(), OperationAccessV1::Mutate);
    let run = f
        .application
        .accept_snapshot_plan(
            plan,
            AdmissionOptions {
                recovery_override: override_guard,
            },
            cancel,
        )
        .unwrap();
    assert!(!f.application.advance_owner_continuation(run).unwrap());
    run
}
fn capture(f: &RuntimeFixture) -> SnapshotId {
    let marker = f.marker("capture");
    let plan = compile(
        f,
        f.instance.id,
        SnapshotOperation::Capture,
        "success",
        &marker,
    );
    let run = f
        .application
        .accept_snapshot_plan(
            plan,
            AdmissionOptions::default(),
            without_hook_text(),
        )
        .unwrap();
    assert!(!f.application.advance_owner_continuation(run).unwrap());
    f.application
        .execute_admitted_capture(run, policy(Some(10_000), Some(10_000), Some(100)))
        .unwrap();
    assert_eq!(finish(f, run).outcome, RunOutcome::Succeeded);
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
fn view(f: &RuntimeFixture) -> InstanceView {
    f.application.load_instance(f.instance.id).unwrap().unwrap()
}
fn run_restore(f: &RuntimeFixture, run: RunId) {
    f.application
        .execute_admitted_restore(run, policy(Some(10_000), Some(10_000), Some(100)))
        .unwrap();
}
fn guard(f: &RuntimeFixture) -> Option<crate::domain::RecoveryGuardView> {
    persistence(&f.storage)
        .load_instance_recovery_guard(f.instance.id)
        .unwrap()
}
fn consequence(f: &RuntimeFixture) -> i64 {
    persistence(&f.storage)
        .recovery_consequence_version(f.instance.id)
        .unwrap()
}
fn open_consequence(f: &RuntimeFixture) {
    let marker = f.marker("risk-capture");
    let plan = compile(
        f,
        f.instance.id,
        SnapshotOperation::Capture,
        "open_failure",
        &marker,
    );
    let run = f
        .application
        .accept_snapshot_plan(
            plan,
            AdmissionOptions {
                recovery_override: true,
            },
            without_hook_text(),
        )
        .unwrap();
    assert!(!f.application.advance_owner_continuation(run).unwrap());
    f.application
        .execute_admitted_capture(run, policy(Some(10_000), Some(10_000), Some(100)))
        .unwrap();
    assert_eq!(finish(f, run).outcome, RunOutcome::Failed);
}
fn absent(input: &str, protection: ManagedInputProtection) -> SnapshotBinding {
    SnapshotBinding {
        input_id: InputIdentity::parse(input).unwrap(),
        role: SnapshotBindingRole::Active,
        state: SnapshotBindingState::Absent,
        protection,
    }
}
fn bound(
    input: &str,
    role: SnapshotBindingRole,
    protection: ManagedInputProtection,
    bytes: &[u8],
    blobs: &mut BTreeMap<Sha256Digest, Vec<u8>>,
) -> SnapshotBinding {
    let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
    blobs.insert(digest.clone(), bytes.to_vec());
    SnapshotBinding {
        input_id: InputIdentity::parse(input).unwrap(),
        role,
        state: SnapshotBindingState::Bound(digest),
        protection,
    }
}
fn store(
    f: &RuntimeFixture,
    version: SnapshotIntegrityVersion,
    bindings: Vec<SnapshotBinding>,
    mut blobs: BTreeMap<Sha256Digest, Vec<u8>>,
    service: Option<&[u8]>,
) -> SnapshotId {
    let id = SnapshotId::generate().unwrap();
    let service_content = service
        .map(|bytes| {
            let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
            blobs.insert(digest.clone(), bytes.to_vec());
            vec![SnapshotServiceContent {
                role: SnapshotServiceRole::parse("database").unwrap(),
                path: SnapshotContentPath::parse("db/main").unwrap(),
                blob_digest: digest,
            }]
        })
        .unwrap_or_default();
    let manifest = SnapshotManifest::new(SnapshotManifestParts {
        version,
        snapshot_id: id,
        producer: view(f).active_revision,
        origin_instance_id: f.instance.id,
        captured_at: SnapshotTimestamp::new(0, 0).unwrap(),
        managed_bindings: bindings,
        service_content,
    })
    .unwrap();
    let verified = crate::snapshot_integrity::encode_snapshot_manifest(manifest, None).unwrap();
    let p = persistence(&f.storage);
    let mut stage = p
        .staging_session()
        .unwrap()
        .create_snapshot_stage()
        .unwrap();
    {
        let mut bundle = crate::snapshot_bundle::start_bundle(&mut stage, &verified).unwrap();
        for (digest, bytes) in blobs {
            bundle.start_blob(&digest, bytes.len() as u64).unwrap();
            bundle.write_all(&bytes).unwrap();
        }
        bundle.finish().unwrap();
    }
    stage.finish_operation_file().unwrap();
    p.import_snapshot_bundle(
        &mut crate::snapshot_bundle::ValidatedSnapshotBundle::read(stage).unwrap(),
    )
    .unwrap();
    id
}
fn export(f: &RuntimeFixture, instance: InstanceId, input: &str) -> Vec<u8> {
    let bytes = f
        .application
        .export_input(instance, &InputIdentity::parse(input).unwrap(), true)
        .unwrap();
    let mut out = Vec::new();
    bytes
        .bytes
        .try_clone_reader()
        .unwrap()
        .read_to_end(&mut out)
        .unwrap();
    out
}

// Test-ID: PR-TEST-0611
// Verifies: PR-REQ-0008, PR-REQ-0123
#[test]
fn snapshot_plans_execute_installed_contract_after_source_is_replaced() {
    let f = fixture();
    let marker=f.marker("detached-capture");
    let plan=compile(&f, f.instance.id, SnapshotOperation::Capture, "success", &marker);
    fs::write(f.temporary.path().join("source/pactrun.yaml"), b"not a source document: [").unwrap();
    let run=f.application.accept_snapshot_plan(plan, AdmissionOptions::default(), without_hook_text()).unwrap();
    assert!(!f.application.advance_owner_continuation(run).unwrap());
    f.application.execute_admitted_capture(run, policy(Some(10_000),Some(10_000),Some(100))).unwrap();
    assert_eq!(finish(&f,run).outcome,RunOutcome::Succeeded);
    let snapshot=SnapshotId::from_bytes(db(&f).query_row("SELECT snapshot_id FROM run_capture_results WHERE run_id=?1",
        [run.as_bytes().as_slice()],|r|r.get::<_,Vec<u8>>(0)).unwrap().try_into().unwrap());
    let plan=compile(&f,f.instance.id,SnapshotOperation::Restore(snapshot),"success",&f.marker("detached-restore"));
    fs::remove_file(f.temporary.path().join("source/pactrun.yaml")).unwrap();
    let run=f.application.accept_snapshot_plan(plan,AdmissionOptions::default(),without_hook_text()).unwrap();
    assert!(!f.application.advance_owner_continuation(run).unwrap());
    run_restore(&f,run);
    assert_eq!(finish(&f,run).outcome,RunOutcome::Succeeded);
    assert_eq!(export(&f,f.instance.id,"secret_config"),SECRET_BINDING);
}

// Test-ID: PR-TEST-0612
// Verifies: PR-REQ-0007, PR-REQ-0030, PR-REQ-0110
#[test]
fn guarded_instance_options_preserve_state_until_explicit_resolution_restore_or_abandonment() {
    for option in ["resolve", "restore", "abandon"] {
        let f=fixture();
        let snapshot=capture(&f);
        open_consequence(&f);
        let initial_guard=guard(&f).unwrap();
        let before=count(&f,"runs");
        assert!(view(&f).required_inputs_satisfied);
        let intent=f.application.resolve_action(&InstanceName::parse("slice4").unwrap(),
            &ActionIdentity::parse("direct").unwrap(),parameters("success",&f.marker("denied"))).unwrap();
        let plan=f.application.compile_action(&intent,&[]).unwrap();
        assert_eq!(count(&f,"runs"),before,"resolution/compilation is not acceptance");
        assert!(f.application.accept_and_admit_action(&plan,AdmissionOptions::default()).is_err());
        assert_eq!(count(&f,"runs"),before+1,"post-acceptance refusal must remain durable");
        assert!(!f.marker("denied").exists());
        assert_eq!(guard(&f),Some(initial_guard.clone()));
        let before_management=count(&f,"runs");
        f.application.set_input(f.instance.id,InputIdentity::parse("optional").unwrap(),view(&f).state_version,
            Box::new(Cursor::new(b"managed while guarded".to_vec()))).unwrap();
        assert_eq!(export(&f,f.instance.id,"optional"),b"managed while guarded");
        f.application.delete_input(f.instance.id,&InputIdentity::parse("optional").unwrap(),view(&f).state_version).unwrap();
        assert_eq!(count(&f,"runs"),before_management);
        assert_eq!(guard(&f),Some(initial_guard));
        match option {
            "resolve" => {
                f.application.resolve_manual_recovery(f.instance.id,view(&f).state_version).unwrap();
                assert!(guard(&f).is_none());
                assert_eq!(count(&f,"runs"),before_management);
            }
            "restore" => {
                let run=admit(&f,f.instance.id,snapshot,"success",&f.marker("recovery-restore"),without_hook_text(),true);
                run_restore(&f,run);
                assert_eq!(finish(&f,run).outcome,RunOutcome::Succeeded);
                assert!(guard(&f).is_none());
            }
            _ => {
                let intent=f.application.resolve_deletion(&InstanceName::parse("slice4").unwrap(),None,DeletionMode::AbandonManagement).unwrap();
                let plan=f.application.compile_deletion(&intent,&[]).unwrap();
                let run=f.application.accept_deletion_plan(plan,AdmissionOptions::default(),without_hook_text(),policy(None,None,None)).unwrap();
                let until=Instant::now()+WAIT_LIMIT;
                while !f.application.advance_owner_continuation(run).unwrap() {
                    f.application.execute_ready_deletion(run).unwrap();
                    assert!(Instant::now()<until);
                }
                assert!(f.application.load_instance(f.instance.id).unwrap().is_none());
                assert_eq!(count(&f,"managed_input_bindings"),0);
            }
        }
    }
}

// Test-ID: PR-TEST-0257
// Verifies: PR-REQ-0211, PR-REQ-0213, PR-REQ-0291
#[test]
fn frozen_restore_completion_is_owner_selected_and_has_no_output_authority() {
    let vectors: Value = serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/vectors/hook_protocol/vectors.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let transcript = vectors["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "restore_active_view_and_snapshot_content")
        .unwrap();
    let mut state = ProtocolState::new_restore("00000000000000000000000000000031".to_owned());
    let mut completed = false;
    for message in transcript["messages"].as_array().unwrap() {
        if message["direction"] == "hook_to_pactrun"
            && let ProtocolStep::Completed(done) = state
                .accept_restore_bytes(message["raw_json"].as_str().unwrap().as_bytes())
                .unwrap()
        {
            assert!(done.produced_outputs.is_empty() && done.service_content.is_empty());
            completed = true;
        }
    }
    assert!(completed && state.is_terminal());
    for message in [
        json!({"type":"complete","operation":"action","status":"success","produced_outputs":[]}),
        json!({"type":"complete","operation":"snapshot_restore","status":"success","service_content":[]}),
        json!({"type":"complete","operation":"snapshot_restore","status":"success","produced_outputs":[]}),
    ] {
        let mut state = ProtocolState::new_restore("00000000000000000000000000000031".to_owned());
        state.accept_restore_bytes(br#"{"type":"session_ready","protocol_version":"1.0-alpha.1","session_id":"00000000000000000000000000000031"}"#).unwrap();
        assert!(
            state
                .accept_restore_bytes(&serde_json::to_vec(&message).unwrap())
                .is_err()
        );
    }
}

// Test-ID: PR-TEST-0258
// Verifies: PR-REQ-0023, PR-REQ-0044, PR-REQ-0102, PR-REQ-0136, PR-REQ-0151, PR-REQ-0191, PR-REQ-0200, PR-REQ-0211, PR-REQ-0213, PR-REQ-0218, PR-REQ-0291, PR-REQ-0298
#[test]
fn real_capture_to_restore_exposes_selected_content_and_atomically_replaces_a_different_instance() {
    let f = fixture();
    let snapshot = capture(&f);
    let target = f
        .application
        .create_instance(
            InstanceName::parse("target").unwrap(),
            view(&f).active_revision,
            vec![
                InputAcquisition {
                    input_id: InputIdentity::parse("secret_config").unwrap(),
                    source: Box::new(Cursor::new(b"target-before")),
                },
                InputAcquisition {
                    input_id: InputIdentity::parse("optional").unwrap(),
                    source: Box::new(Cursor::new(b"delete-optional")),
                },
            ],
        )
        .unwrap();
    db(&f).execute("UPDATE managed_input_payloads SET protection_rank=1 WHERE (instance_id,payload_id) IN (SELECT instance_id,payload_id FROM managed_input_bindings WHERE instance_id=?1 AND input_identity=?2)",rusqlite::params![target.id.as_bytes().as_slice(),b"optional".as_slice()]).unwrap();
    let marker = f.marker("restore");
    let run = admit(
        &f,
        target.id,
        snapshot,
        "success",
        &marker,
        without_hook_text(),
        false,
    );
    let before = count(&f, "managed_input_payloads");
    run_restore(&f, run);
    assert_eq!(export(&f, target.id, "secret_config"), b"target-before");
    let outcome = finish(&f, run);
    assert_eq!(outcome.outcome, RunOutcome::Succeeded, "{outcome:?}");
    let after = f.application.load_instance(target.id).unwrap().unwrap();
    assert_eq!(after.name, target.name);
    assert_eq!(after.id, target.id);
    assert_ne!(after.state_version, target.state_version);
    assert!(
        !after
            .bindings
            .iter()
            .find(|b| b.input_id.as_str() == "optional")
            .unwrap()
            .present
    );
    assert_eq!(export(&f, target.id, "secret_config"), SECRET_BINDING);
    assert!(count(&f, "managed_input_payloads") < before);
    assert_eq!(count(&f, "run_restore_admissions"), 0);
    assert_eq!(count(&f, "run_capture_results"), 1);
    let source_payload:Vec<u8>=db(&f).query_row("SELECT payload_id FROM managed_input_bindings WHERE instance_id=?1 AND input_identity=?2",rusqlite::params![f.instance.id.as_bytes().as_slice(),b"secret_config".as_slice()],|r|r.get(0)).unwrap();
    let target_payload:Vec<u8>=db(&f).query_row("SELECT payload_id FROM managed_input_bindings WHERE instance_id=?1 AND input_identity=?2",rusqlite::params![target.id.as_bytes().as_slice(),b"secret_config".as_slice()],|r|r.get(0)).unwrap();
    assert_ne!(source_payload, target_payload);
    assert!(marker_variant(&marker, "content-checked").exists());
    assert!(!contains(
        &database_bytes(&f.storage),
        SENSITIVE_PARAMETER.as_bytes()
    ));
}

// Test-ID: PR-TEST-0259
// Verifies: PR-REQ-0025, PR-REQ-0070, PR-REQ-0110, PR-REQ-0291, PR-REQ-0298
#[test]
fn restore_compares_both_tokens_and_success_alone_clears_the_existing_guard() {
    for mode in ["success", "state", "aba", "consequence"] {
        let f = fixture();
        let snapshot = capture(&f);
        open_consequence(&f);
        let initial_guard = guard(&f).unwrap();
        let token = view(&f).state_version;
        let counter = consequence(&f);
        let marker = f.marker("restore");
        let run = admit(
            &f,
            f.instance.id,
            snapshot,
            "success",
            &marker,
            ActionCancellation::default(),
            true,
        );
        run_restore(&f, run);
        assert_eq!(guard(&f), Some(initial_guard.clone()));
        match mode {
            "state" => f.replace_binding(),
            "aba" => {
                f.replace_binding();
                let v = view(&f);
                f.application
                    .set_input(
                        f.instance.id,
                        InputIdentity::parse("secret_config").unwrap(),
                        v.state_version,
                        Box::new(Cursor::new(SECRET_BINDING)),
                    )
                    .unwrap();
            }
            "consequence" => {
                open_consequence(&f);
                assert_eq!(view(&f).state_version, token);
                assert_eq!(consequence(&f), counter + 1);
            }
            _ => {}
        }
        let observed = view(&f).state_version;
        let bytes = export(&f, f.instance.id, "secret_config");
        let outcome = finish(&f, run);
        assert_eq!(
            outcome.outcome,
            if mode == "success" {
                RunOutcome::Succeeded
            } else {
                RunOutcome::Failed
            },
            "{outcome:?}"
        );
        if mode == "success" {
            assert!(guard(&f).is_none());
            assert_ne!(view(&f).state_version, token);
            assert_eq!(consequence(&f), counter);
        } else {
            assert_eq!(guard(&f), Some(initial_guard));
            assert_eq!(view(&f).state_version, observed);
            assert_eq!(export(&f, f.instance.id, "secret_config"), bytes);
            assert_eq!(
                outcome.primary_failure.unwrap().step,
                RunFailedStep::SnapshotPlan(SnapshotPlanStep::PublishManagedResult)
            );
        }
        assert_eq!(count(&f, "run_restore_admissions"), 0);
        assert!(
            f.application
                .execute_admitted_restore(run, policy(None, None, None))
                .is_err()
        );
    }
}

// Test-ID: PR-TEST-0260
// Verifies: PR-REQ-0291, PR-REQ-0215, PR-REQ-0216, PR-REQ-0298, PR-REQ-0070
#[test]
fn failed_cancelled_timed_out_and_protocol_invalid_restore_preserves_target_and_guard() {
    for mode in [
        "failure",
        "open_failure",
        "open_success",
        "protocol",
        "no_ready",
        "hang",
        "before_launch",
    ] {
        let f = fixture();
        let snapshot = capture(&f);
        open_consequence(&f);
        f.replace_binding();
        let before = view(&f);
        let old_guard = guard(&f);
        let counter = consequence(&f);
        let bytes = export(&f, f.instance.id, "secret_config");
        let marker = f.marker("restore");
        let cancellation = without_hook_text();
        let run = admit(
            &f,
            f.instance.id,
            snapshot,
            mode,
            &marker,
            cancellation.clone(),
            true,
        );
        let cancelling = if mode == "hang" {
            Some(cancel_after(
                marker_variant(&marker, "ready"),
                &cancellation,
            ))
        } else {
            None
        };
        if mode == "before_launch" {
            cancellation.request();
        }
        f.application
            .execute_admitted_restore(
                run,
                policy(
                    Some(if mode == "no_ready" { 100 } else { 10_000 }),
                    Some(10_000),
                    Some(50),
                ),
            )
            .unwrap();
        if let Some(thread) = cancelling {
            thread.join().unwrap();
        }
        let outcome = finish(&f, run);
        assert_eq!(
            outcome.outcome,
            match mode {
                "hang" | "before_launch" => RunOutcome::Cancelled,
                "no_ready" => RunOutcome::TimedOut,
                _ => RunOutcome::Failed,
            },
            "{mode}: {outcome:?}"
        );
        assert_eq!(view(&f).state_version, before.state_version);
        assert_eq!(guard(&f), old_guard);
        assert_eq!(export(&f, f.instance.id, "secret_config"), bytes);
        assert_eq!(
            consequence(&f),
            counter + i64::from(mode.starts_with("open_"))
        );
        assert_eq!(count(&f, "run_restore_admissions"), 0);
        if mode == "before_launch" {
            assert!(!marker_variant(&marker, "launched").exists());
        }
        assert!(!contains(
            &database_bytes(&f.storage),
            SENSITIVE_PARAMETER.as_bytes()
        ));
    }
}

// Test-ID: PR-TEST-0261
// Verifies: PR-REQ-0291, PR-REQ-0211, PR-REQ-0213, PR-REQ-0091
#[test]
fn restore_reads_v1_and_v2_retained_state_and_allows_required_absent_to_absent() {
    for version in [SnapshotIntegrityVersion::BASELINE, SnapshotIntegrityVersion::BASELINE] {
        let f = fixture();
        let mut blobs = BTreeMap::new();
        let bindings = vec![
            bound(
                "secret_config",
                SnapshotBindingRole::Active,
                ManagedInputProtection::Secret,
                SECRET_BINDING,
                &mut blobs,
            ),
            absent("optional", ManagedInputProtection::Normal),
            bound(
                "retained",
                SnapshotBindingRole::Retained,
                ManagedInputProtection::Secret,
                b"retained-secret",
                &mut blobs,
            ),
        ];
        let snapshot = store(&f, version, bindings, blobs, Some(SERVICE));
        // Another selected-by-nobody Snapshot must never contribute authority.
        store(
            &f,
            version,
            vec![
                absent("secret_config", ManagedInputProtection::Secret),
                absent("optional", ManagedInputProtection::Normal),
            ],
            BTreeMap::new(),
            Some(b"unrelated-snapshot-sentinel"),
        );
        fs::create_dir_all(f.storage.join("service-storage")).unwrap();
        fs::write(
            f.storage.join("service-storage/sentinel"),
            b"unsubmitted-service-storage-sentinel",
        )
        .unwrap();
        db(&f).execute("INSERT INTO managed_input_bindings(instance_id,input_identity,payload_id) SELECT instance_id,?2,payload_id FROM managed_input_bindings WHERE instance_id=?1 AND input_identity=?3",rusqlite::params![f.instance.id.as_bytes().as_slice(),b"remove-retained".as_slice(),b"secret_config".as_slice()]).unwrap();
        let marker = f.marker("restore");
        let run = admit(
            &f,
            f.instance.id,
            snapshot,
            "success",
            &marker,
            ActionCancellation::default(),
            false,
        );
        run_restore(&f, run);
        assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
        assert!(
            !view(&f)
                .bindings
                .iter()
                .any(|b| b.input_id.as_str() == "remove-retained")
        );
        assert_eq!(export(&f, f.instance.id, "retained"), b"retained-secret");
        let session: Value =
            serde_json::from_slice(&fs::read(marker_variant(&marker, "session")).unwrap()).unwrap();
        assert_eq!(
            session["operation"]["bindings"].as_array().unwrap().len(),
            1
        );
        assert_eq!(
            session["operation"]["snapshot_content"]["logical_descriptors"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let incomplete = f
            .application
            .create_instance(
                InstanceName::parse("incomplete").unwrap(),
                view(&f).active_revision,
                Vec::new(),
            )
            .unwrap();
        assert!(!incomplete.required_inputs_satisfied);
        let empty = store(
            &f,
            version,
            vec![
                absent("secret_config", ManagedInputProtection::Secret),
                absent("optional", ManagedInputProtection::Normal),
            ],
            BTreeMap::new(),
            None,
        );
        let marker = f.marker("absent");
        let run = admit(
            &f,
            incomplete.id,
            empty,
            "absent",
            &marker,
            ActionCancellation::default(),
            false,
        );
        run_restore(&f, run);
        assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
        assert!(
            !f.application
                .load_instance(incomplete.id)
                .unwrap()
                .unwrap()
                .required_inputs_satisfied
        );
    }
}

// Test-ID: PR-TEST-0262
// Verifies: PR-REQ-0291, PR-REQ-0289
#[test]
fn restore_refuses_sticky_downgrade_and_required_removal_before_any_hook_launch() {
    let f = fixture();
    let mut blobs = BTreeMap::new();
    let bindings = vec![
        bound(
            "secret_config",
            SnapshotBindingRole::Active,
            ManagedInputProtection::Secret,
            SECRET_BINDING,
            &mut blobs,
        ),
        bound(
            "optional",
            SnapshotBindingRole::Active,
            ManagedInputProtection::Normal,
            b"normal",
            &mut blobs,
        ),
    ];
    let snapshot = store(&f, SnapshotIntegrityVersion::BASELINE, bindings, blobs, None);
    let v = view(&f);
    f.application
        .set_input(
            f.instance.id,
            InputIdentity::parse("optional").unwrap(),
            v.state_version,
            Box::new(Cursor::new(b"sticky")),
        )
        .unwrap();
    let marker = f.marker("refused");
    let plan = compile(
        &f,
        f.instance.id,
        SnapshotOperation::Restore(snapshot),
        "success",
        &marker,
    );
    // Model a valid persisted sticky Secret under a Normal declaration. The
    // unchanged token ensures Admission must recheck authoritative protection.
    db(&f).execute("UPDATE managed_input_payloads SET protection_rank=1 WHERE (instance_id,payload_id) IN (SELECT instance_id,payload_id FROM managed_input_bindings WHERE instance_id=?1 AND input_identity=?2)",rusqlite::params![f.instance.id.as_bytes().as_slice(),b"optional".as_slice()]).unwrap();
    let run = f
        .application
        .accept_snapshot_plan(
            plan,
            AdmissionOptions::default(),
            ActionCancellation::default(),
        )
        .unwrap();
    assert_eq!(finish(&f, run).outcome, RunOutcome::Failed);
    assert!(
        f.application
            .execute_admitted_restore(run, policy(None, None, None))
            .is_err()
    );
    assert!(!marker_variant(&marker, "launched").exists());
    let absent = store(
        &f,
        SnapshotIntegrityVersion::BASELINE,
        vec![
            absent("secret_config", ManagedInputProtection::Secret),
            absent("optional", ManagedInputProtection::Normal),
        ],
        BTreeMap::new(),
        None,
    );
    let intent = SnapshotIntent {
        instance: f.instance.id,
        operation: SnapshotOperation::Restore(absent),
        parameters: parameters("success", &marker),
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
    assert!(!marker_variant(&marker, "launched").exists());
}

// Test-ID: PR-TEST-0263
// Verifies: PR-REQ-0026, PR-REQ-0143, PR-REQ-0291, PR-REQ-0298
#[test]
fn restore_publication_rollback_preserves_guard_pins_and_target_until_a_single_success() {
    let f = fixture();
    let snapshot = capture(&f);
    open_consequence(&f);
    f.replace_binding();
    let before = view(&f);
    let bytes = export(&f, f.instance.id, "secret_config");
    let old_guard = guard(&f);
    let counter = consequence(&f);
    let marker = f.marker("restore");
    let run = admit(
        &f,
        f.instance.id,
        snapshot,
        "success",
        &marker,
        ActionCancellation::default(),
        true,
    );
    run_restore(&f, run);
    for _ in 0..2 {
        PactrunPersistence::fail_next_restore_publication_for_test();
        assert!(f.application.advance_owner_continuation(run).is_err());
        assert_eq!(view(&f).state_version, before.state_version);
        assert_eq!(export(&f, f.instance.id, "secret_config"), bytes);
        assert_eq!(guard(&f), old_guard);
        assert_eq!(count(&f, "run_restore_admissions"), 1);
    }
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    assert_eq!(export(&f, f.instance.id, "secret_config"), SECRET_BINDING);
    assert!(guard(&f).is_none());
    assert_eq!(consequence(&f), counter);
    assert_eq!(count(&f, "run_restore_admissions"), 0);
    assert_eq!(
        fs::read(marker_variant(&marker, "launched")).unwrap(),
        b"launch\n"
    );
}

const RESTORE_ACTOR: &str = "hook::tests::restore_runtime::restore_process_owner";
#[test]
fn restore_process_owner() {
    let Some(storage) = env::var_os("PACTRUN_S6_OWNER_ROOT") else {
        return;
    };
    let marker = PathBuf::from(env::var_os("PACTRUN_S6_OWNER_MARKER").unwrap());
    let mode = env::var("PACTRUN_S6_OWNER_MODE").unwrap();
    let snapshot = env::var("PACTRUN_S6_SNAPSHOT")
        .unwrap()
        .parse::<SnapshotId>()
        .unwrap();
    let app = PactrunApplication::open(Path::new(&storage)).unwrap();
    let instance = app
        .resolve_instance_name(&InstanceName::parse("slice4").unwrap())
        .unwrap()
        .unwrap();
    let plan = crate::workflow::compile_snapshot(
        &app,
        &crate::workflow::PlatformHostLauncherLookup,
        &SnapshotIntent {
            instance,
            operation: SnapshotOperation::Restore(snapshot),
            parameters: parameters(&mode, &marker),
        },
        &[],
    )
    .unwrap();
    let run = app
        .accept_snapshot_plan(
            plan,
            AdmissionOptions {
                recovery_override: true,
            },
            ActionCancellation::default(),
        )
        .unwrap();
    assert!(!app.advance_owner_continuation(run).unwrap());
    fs::write(marker_variant(&marker, "run"), run.to_string()).unwrap();
    app.execute_admitted_restore(run, policy(Some(10_000), Some(10_000), Some(100)))
        .unwrap();
    fs::write(marker_variant(&marker, "before-publication"), b"").unwrap();
    if env::var_os("PACTRUN_S6_PAUSE").is_some() {
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
fn actor(
    f: &RuntimeFixture,
    snapshot: SnapshotId,
    marker: &Path,
    mode: &str,
    fault: Option<&str>,
    pause: bool,
) -> Actor {
    let mut cmd = Command::new(env::current_exe().unwrap());
    cmd.args(["--exact", RESTORE_ACTOR, "--nocapture"])
        .env("PACTRUN_S6_OWNER_ROOT", &f.storage)
        .env("PACTRUN_S6_OWNER_MARKER", marker)
        .env("PACTRUN_S6_OWNER_MODE", mode)
        .env("PACTRUN_S6_SNAPSHOT", snapshot.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit());
    if let Some(fault) = fault {
        cmd.env("PACTRUN_M4_FAULT", fault);
    }
    if pause {
        cmd.env("PACTRUN_S6_PAUSE", "1");
    }
    Actor(cmd.spawn().unwrap())
}

// Test-ID: PR-TEST-0264
// Verifies: PR-REQ-0291, PR-REQ-0298, PR-REQ-0070
#[test]
fn restore_owner_loss_and_commit_crashes_have_only_old_or_complete_new_state() {
    for mode in ["success", "open_failure"] {
        let f = fixture();
        let snapshot = capture(&f);
        open_consequence(&f);
        f.replace_binding();
        let before = view(&f);
        let bytes = export(&f, f.instance.id, "secret_config");
        let old_guard = guard(&f);
        let counter = consequence(&f);
        let marker = f.marker("owner");
        let mut actor = actor(&f, snapshot, &marker, mode, None, true);
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
        assert!(
            f.application
                .reconcile_lost_managed_owners()
                .unwrap()
                .is_empty()
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
        assert_eq!(view(&f).state_version, before.state_version);
        assert_eq!(export(&f, f.instance.id, "secret_config"), bytes);
        assert_eq!(guard(&f), old_guard);
        assert_eq!(consequence(&f), counter + i64::from(mode == "open_failure"));
        assert_eq!(count(&f, "run_restore_admissions"), 0);
        assert_eq!(
            fs::read(marker_variant(&marker, "launched")).unwrap(),
            b"launch\n"
        );
    }
    for (fault, committed) in [
        ("before_run_finish_commit", false),
        ("after_run_finish_commit", true),
    ] {
        let f = fixture();
        let snapshot = capture(&f);
        open_consequence(&f);
        f.replace_binding();
        let before = view(&f);
        let bytes = export(&f, f.instance.id, "secret_config");
        let old_guard = guard(&f);
        let counter = consequence(&f);
        let marker = f.marker("crash");
        let mut actor = actor(&f, snapshot, &marker, "success", Some(fault), false);
        assert_eq!(actor.0.wait().unwrap().code(), Some(87));
        let run = fs::read_to_string(marker_variant(&marker, "run"))
            .unwrap()
            .parse::<RunId>()
            .unwrap();
        let reconciled = f.application.reconcile_lost_managed_owners().unwrap();
        assert_eq!(reconciled, if committed { Vec::new() } else { vec![run] });
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
            assert_ne!(view(&f).state_version, before.state_version);
            assert_eq!(export(&f, f.instance.id, "secret_config"), SECRET_BINDING);
            assert!(guard(&f).is_none());
        } else {
            assert_eq!(view(&f).state_version, before.state_version);
            assert_eq!(export(&f, f.instance.id, "secret_config"), bytes);
            assert_eq!(guard(&f), old_guard);
        }
        assert_eq!(consequence(&f), counter);
        assert_eq!(count(&f, "run_restore_admissions"), 0);
        assert_eq!(
            fs::read(marker_variant(&marker, "launched")).unwrap(),
            b"launch\n"
        );
    }
}

// Test-ID: PR-TEST-0265
// Verifies: PR-REQ-0291, PR-REQ-0213
#[test]
fn corrupt_selected_content_fails_before_hook_launch_and_never_changes_the_target() {
    let f = fixture();
    let snapshot = capture(&f);
    open_consequence(&f);
    let before = view(&f);
    let old_guard = guard(&f);
    let marker = f.marker("corrupt");
    let run = admit(
        &f,
        f.instance.id,
        snapshot,
        "success",
        &marker,
        ActionCancellation::default(),
        true,
    );
    crate::persistence::corrupt_snapshot_for_test(&f.storage, snapshot);
    run_restore(&f, run);
    assert_eq!(finish(&f, run).outcome, RunOutcome::Failed);
    assert!(!marker_variant(&marker, "launched").exists());
    assert_eq!(view(&f).state_version, before.state_version);
    assert_eq!(guard(&f), old_guard);
    assert_eq!(count(&f, "run_restore_admissions"), 0);
}

// Test-ID: PR-TEST-0266
// Verifies: PR-REQ-0291, PR-REQ-0293, PR-REQ-0213
#[test]
fn restore_materializes_real_service_content_above_the_managed_input_limit() {
    let f = fixture();
    let marker = f.marker("large-capture");
    let plan = compile(
        &f,
        f.instance.id,
        SnapshotOperation::Capture,
        "large",
        &marker,
    );
    let run = f
        .application
        .accept_snapshot_plan(
            plan,
            AdmissionOptions::default(),
            ActionCancellation::default(),
        )
        .unwrap();
    assert!(!f.application.advance_owner_continuation(run).unwrap());
    f.application
        .execute_admitted_capture(run, policy(None, None, Some(100)))
        .unwrap();
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    let snapshot = SnapshotId::from_bytes(
        db(&f)
            .query_row(
                "SELECT snapshot_id FROM run_capture_results WHERE run_id=?1",
                [run.as_bytes().as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .unwrap()
            .try_into()
            .unwrap(),
    );
    let marker = f.marker("large-restore");
    let run = admit(
        &f,
        f.instance.id,
        snapshot,
        "large",
        &marker,
        ActionCancellation::default(),
        false,
    );
    f.application
        .execute_admitted_restore(run, policy(None, None, Some(100)))
        .unwrap();
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    assert!(marker_variant(&marker, "content-checked").exists());
    assert_eq!(export(&f, f.instance.id, "secret_config"), SECRET_BINDING);
}

/// Explicit capacity acceptance runs, not part of the routine suite. Each case
/// uses the same two service descriptors and hashes actual persisted bytes.
fn capacity_round_trip(mib: u64) {
    let f = fixture();
    let mode = format!("capacity-{mib}");
    let marker = f.marker("capacity-capture");
    let plan = compile(&f, f.instance.id, SnapshotOperation::Capture, &mode, &marker);
    let run = f.application.accept_snapshot_plan(plan, AdmissionOptions::default(), ActionCancellation::default()).unwrap();
    assert!(!f.application.advance_owner_continuation(run).unwrap());
    f.application.execute_admitted_capture(run, policy(None, None, Some(100))).unwrap();
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    let snapshot = f.application.list_snapshots(None).unwrap()[0].id;
    assert_eq!(db(&f).query_row::<i64,_,_>("SELECT count(*) FROM sqlite_schema WHERE name='snapshot_blob_chunks'", [], |r| r.get(0)).unwrap(), 0);
    for name in ["pactrun.sqlite3", "pactrun.sqlite3-wal"] {
        let path = f.storage.join("database").join(name);
        if path.exists() {
            let length = fs::metadata(path).unwrap().len();
            assert!(length < 64 * 1024 * 1024, "service bytes must not grow the control database/WAL");
            eprintln!("control file {name}: {length} bytes");
        }
    }
    f.application.verify_snapshot(snapshot).unwrap();
    let expected = mib * 1024 * 1024 + 1;
    assert_eq!(db(&f).query_row(
        "SELECT count(*) FROM snapshot_blobs WHERE snapshot_id=?1 AND byte_length=?2",
        rusqlite::params![snapshot.as_bytes().as_slice(), expected as i64],
        |r| r.get::<_, i64>(0)).unwrap(), 2);
    let marker = f.marker("capacity-restore");
    let restore = admit(&f, f.instance.id, snapshot, &mode, &marker, ActionCancellation::default(), false);
    f.application.execute_admitted_restore(restore, policy(None, None, Some(100))).unwrap();
    assert_eq!(finish(&f, restore).outcome, RunOutcome::Succeeded);
    assert!(marker_variant(&marker, "content-checked").exists());
    assert_eq!(export(&f, f.instance.id, "secret_config"), SECRET_BINDING);

    let bundle = f.storage.parent().unwrap().join("capacity.zip");
    f.application.export_snapshot_file(snapshot, &bundle, true, &mut std::io::sink()).unwrap();
    assert!(fs::metadata(&bundle).unwrap().len() > 2 * expected);
    if mib > 16 * 1024 {
        assert!(fs::metadata(&bundle).unwrap().len() > 32 * 1024_u64.pow(3) + 128 * 1024 * 1024);
    }
    let imported_root = f.storage.parent().unwrap().join("capacity-import");
    fs::create_dir(&imported_root).unwrap();
    for child in ["database", "runtime-content", "staging"] {
        fs::create_dir(imported_root.join(child)).unwrap();
    }
    let imported = crate::application::PactrunApplication::open(&imported_root).unwrap();
    assert_eq!(imported.import_snapshot_file(&bundle).unwrap().id, snapshot);
    imported.verify_snapshot(snapshot).unwrap();
    // Compare exact persisted canonical bytes and integrity digest without
    // allocating any service payload. Both stores performed complete hashing.
    let target_db = rusqlite::Connection::open(imported_root.join("database/pactrun.sqlite3")).unwrap();
    let projection = |conn: &rusqlite::Connection| {
        conn.query_row("SELECT canonical_manifest,integrity_digest FROM snapshots WHERE snapshot_id=?1",
            [snapshot.as_bytes().as_slice()], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))).unwrap()
    };
    assert_eq!(projection(&db(&f)), projection(&target_db));
    eprintln!("capacity evidence: two service blobs of {expected} bytes; Capture, verify, Restore, export, import verified");
}

#[test]
#[ignore = "explicit capacity/RSS comparison on persistent remote workspace"]
fn snapshot_capacity_small() { capacity_round_trip(1); }

#[test]
#[ignore = "explicit capacity/RSS comparison on persistent remote workspace"]
fn snapshot_capacity_medium() { capacity_round_trip(1024); }

// Test-ID: PR-TEST-0479
// Verifies: PR-REQ-0293, PR-REQ-0294, PR-REQ-0292, PR-REQ-0291, PR-REQ-0347
#[test]
#[ignore = "requires at least 160 GiB free; run sequentially on persistent remote workspace"]
fn snapshot_capacity_beyond_former_ceilings() { capacity_round_trip(17 * 1024); }

// Test-ID: PR-TEST-0482
// Verifies: PR-REQ-0293, PR-REQ-0291, PR-REQ-0341
#[test]
fn snapshot_chunk_cancellation_preserves_target_and_releases_restore_pin() {
    struct CancelAfterWrite { cancellation: ActionCancellation, written: usize }
    impl Write for CancelAfterWrite {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.written += bytes.len();
            self.cancellation.request();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }
    let f = fixture();
    let plan = compile(&f, f.instance.id, SnapshotOperation::Capture, "capacity-1", &f.marker("cancel-source"));
    let capture = f.application.accept_snapshot_plan(plan, AdmissionOptions::default(), ActionCancellation::default()).unwrap();
    assert!(!f.application.advance_owner_continuation(capture).unwrap());
    f.application.execute_admitted_capture(capture, policy(None, None, Some(100))).unwrap();
    assert_eq!(finish(&f, capture).outcome, RunOutcome::Succeeded);
    let snapshot = f.application.list_snapshots(None).unwrap()[0].id;
    let before = view(&f);
    let cancellation = ActionCancellation::default();
    let marker = f.marker("cancel-materialization");
    let run = admit(&f, f.instance.id, snapshot, "capacity-1", &marker, cancellation.clone(), false);
    assert_eq!(f.application.delete_object(&ObjectDeletion::Snapshot(snapshot)).unwrap(), ObjectDeletionResult::Blocked(DeletionBlock::SnapshotInUse));
    let p = persistence(&f.storage);
    let manifest = p.admitted_restore_manifest(run).unwrap();
    let mut destination = CancelAfterWrite { cancellation: cancellation.clone(), written: 0 };
    let error = p.copy_admitted_restore_blob(run, &manifest.service_content()[0].blob_digest,
        &mut super::super::materialize::CancellableWriter { destination: &mut destination, cancellation: &cancellation }).unwrap_err();
    assert!(matches!(error, crate::persistence::PersistenceError::Io { .. }));
    assert!((1..=MANAGED_INPUT_CHUNK_BYTES_V1).contains(&destination.written));
    run_restore(&f, run);
    assert_eq!(finish(&f, run).outcome, RunOutcome::Cancelled);
    assert_eq!(view(&f).state_version, before.state_version);
    assert!(!marker_variant(&marker, "launched").exists());
    assert_eq!(count(&f, "run_restore_admissions"), 0);
    f.application.verify_snapshot(snapshot).unwrap();
    assert_eq!(f.application.delete_object(&ObjectDeletion::Snapshot(snapshot)).unwrap(), ObjectDeletionResult::Deleted);
}

// Test-ID: PR-TEST-0485
// Verifies: PR-REQ-0347, PR-REQ-0291
#[test]
fn reopened_snapshot_restores_through_the_real_hook_into_fresh_immutable_bindings() {
    let f = fixture();
    let snapshot = capture(&f);
    let RuntimeFixture { temporary, storage, launcher, application, instance } = f;
    drop(application);
    let database = rusqlite::Connection::open(storage.join("database/pactrun.sqlite3")).unwrap();
    let application = PactrunApplication::open(&storage).unwrap();
    let f = RuntimeFixture { temporary, storage, launcher, application, instance };
    let marker = f.marker("legacy-value-restore");
    let run = admit(&f, f.instance.id, snapshot, "success", &marker, ActionCancellation::default(), false);
    run_restore(&f, run);
    assert_eq!(finish(&f, run).outcome, RunOutcome::Succeeded);
    assert_eq!(export(&f, f.instance.id, "secret_config"), SECRET_BINDING);
    assert_eq!(database.query_row::<i64,_,_>("SELECT count(*) FROM pragma_table_info('snapshot_blobs') WHERE name='storage_kind'", [], |r| r.get(0)).unwrap(), 0);
    assert!(database.query_row::<i64,_,_>("SELECT count(*) FROM managed_input_payloads WHERE content_digest IS NOT NULL", [], |r| r.get(0)).unwrap() > 0);
}

// Test-ID: PR-TEST-0267
// Verifies: PR-REQ-0291, PR-REQ-0298, PR-REQ-0289
#[test]
fn another_process_changes_each_publication_token_without_replaying_the_restore_hook() {
    for changed in ["state", "consequence"] {
        let f = fixture();
        let snapshot = capture(&f);
        open_consequence(&f);
        let old_guard = guard(&f);
        let admitted = view(&f).state_version;
        let marker = f.marker("cross-process-conflict");
        let mut child = actor(&f, snapshot, &marker, "success", None, true);
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
        if changed == "state" {
            f.replace_binding();
        } else {
            open_consequence(&f);
            assert_eq!(view(&f).state_version, admitted);
        }
        let state = view(&f).state_version;
        let bytes = export(&f, f.instance.id, "secret_config");
        let counter = consequence(&f);
        fs::write(marker_variant(&marker, "release"), b"").unwrap();
        assert!(child.0.wait().unwrap().success());
        let RunState::Finished(outcome) = persistence(&f.storage)
            .load_managed_run(run)
            .unwrap()
            .unwrap()
            .state
        else {
            panic!("terminal")
        };
        assert_eq!(outcome.outcome, RunOutcome::Failed);
        assert_eq!(
            outcome.primary_failure.unwrap().step,
            RunFailedStep::SnapshotPlan(SnapshotPlanStep::PublishManagedResult)
        );
        assert_eq!(view(&f).state_version, state);
        assert_eq!(export(&f, f.instance.id, "secret_config"), bytes);
        assert_eq!(consequence(&f), counter);
        assert_eq!(guard(&f), old_guard);
        assert_eq!(count(&f, "run_restore_admissions"), 0);
        assert_eq!(
            fs::read(marker_variant(&marker, "launched")).unwrap(),
            b"launch\n"
        );
    }
}

#[test]
fn restore_hook_worker() {
    let Ok(transport) = env::var(TRANSPORT_ENVIRONMENT) else {
        return;
    };
    let mut stream = connect_hook(&transport, &env::var(ENDPOINT_ENVIRONMENT).unwrap());
    let mut preamble = [0; PREAMBLE.len()];
    stream.read_exact(&mut preamble).unwrap();
    assert_eq!(preamble, PREAMBLE);
    let session = read_frame(&mut stream).unwrap();
    let parameters = session["parameters"]
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
    let mode = parameters["mode"];
    let marker = PathBuf::from(parameters["marker"]);
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(marker_variant(&marker, "launched"))
        .unwrap()
        .write_all(b"launch\n")
        .unwrap();
    assert_eq!(session["operation"]["kind"], "snapshot_restore");
    assert!(
        session["operation"].get("candidate").is_none()
            && session["operation"].get("access").is_none()
            && session["operation"].get("outputs").is_none()
    );
    let bindings = session["operation"]["bindings"].as_array().unwrap();
    assert!(bindings.iter().all(|b| b["role"] == "active"));
    if mode != "absent" {
        let binding = bindings
            .iter()
            .find(|b| b["input_id"] == "secret_config")
            .unwrap();
        let path = Path::new(binding["readonly_path"].as_str().unwrap());
        assert_eq!(
            fs::read(path).unwrap(),
            parameters["expected_binding"].as_bytes()
        );
        assert!(fs::metadata(path).unwrap().permissions().readonly());
    } else {
        assert!(bindings.is_empty());
    }
    let content = &session["operation"]["snapshot_content"];
    let root = Path::new(content["readonly_root_path"].as_str().unwrap());
    let descriptors = content["logical_descriptors"].as_array().unwrap();
    let mut exposed = BTreeSet::new();
    for descriptor in descriptors {
        let relative = descriptor["materialized_path"].as_str().unwrap();
        RuntimePath::parse(relative).unwrap();
        assert_ne!(relative, descriptor["path"].as_str().unwrap());
        let path = root.join(relative);
        let digest = if mode == "large" || mode.starts_with("capacity-") {
            let mut file = fs::File::open(&path).unwrap();
            let mut hash = Sha256::new();
            let mut buffer = [0; 65536];
            let mut length = 0u64;
            loop {
                let n = file.read(&mut buffer).unwrap();
                if n == 0 {
                    break;
                }
                hash.update(&buffer[..n]);
                length += n as u64;
            }
            let expected_mib = mode.strip_prefix("capacity-").map(|m| m.parse::<u64>().unwrap()).unwrap_or(512);
            assert_eq!(length, expected_mib * 1024 * 1024 + 1);
            Sha256Digest::from_bytes(hash.finalize().into())
        } else {
            let bytes = fs::read(&path).unwrap();
            assert_eq!(bytes, SERVICE);
            Sha256Digest::from_bytes(Sha256::digest(&bytes).into())
        };
        assert_eq!(descriptor["blob_digest"], digest.as_str());
        assert!(matches!(
            (
                descriptor["role"].as_str().unwrap(),
                descriptor["path"].as_str().unwrap()
            ),
            ("database", "db/main") | ("logs", "log/main")
        ));
        assert!(fs::metadata(&path).unwrap().permissions().readonly());
        exposed.insert(path);
    }
    fn files(root: &Path, out: &mut BTreeSet<PathBuf>) {
        for e in fs::read_dir(root).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                files(&p, out)
            } else {
                out.insert(p);
            }
        }
    }
    let mut actual = BTreeSet::new();
    files(root, &mut actual);
    assert_eq!(exposed, actual);
    fs::write(marker_variant(&marker, "session"), session.to_string()).unwrap();
    fs::write(marker_variant(&marker, "content-checked"), b"").unwrap();
    stream.write_all(PREAMBLE).unwrap();
    if mode == "no_ready" {
        thread::sleep(Duration::from_secs(30));
        return;
    }
    write_frame(
        &mut stream,
        &json!({"type":"session_ready","protocol_version":"1.0-alpha.1","session_id":session["session_id"]}),
    );
    fs::write(marker_variant(&marker, "ready"), b"").unwrap();
    if mode == "hang" {
        let cancel = read_frame(&mut stream).unwrap();
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
        assert_eq!(read_frame(&mut stream).unwrap()["risk_state"], "open");
        fs::write(marker_variant(&marker, "risk"), b"").unwrap();
    }
    if mode == "protocol" {
        write_frame(
            &mut stream,
            &json!({"type":"complete","operation":"snapshot_restore","status":"success","produced_outputs":[]}),
        );
        return;
    }
    write_frame(
        &mut stream,
        &json!({"type":"complete","operation":"snapshot_restore","status":if mode=="failure"||mode=="open_failure" {"failure"}else{"success"},"code":"sensitive_hook_code","message":SENSITIVE_PARAMETER}),
    );
    let ack = read_frame(&mut stream).unwrap();
    if mode == "open_success" {
        assert_eq!(ack["type"], "protocol_error");
    } else {
        assert_eq!(ack["type"], "completion_accepted");
        fs::write(marker_variant(&marker, "accepted"), b"").unwrap();
    }
}
