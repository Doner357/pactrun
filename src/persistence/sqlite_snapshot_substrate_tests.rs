use super::*;
use crate::application::{ApplicationError, PactrunApplication};
use crate::domain::*;
use crate::executor::AdmissionOptions;
use crate::workflow::{PlatformHostLauncherLookup, compile_snapshot};
use std::io::Write;

mod recovery {
    include!("sqlite_recovery_tests.rs");
}

fn compile(
    app: &PactrunApplication,
    view: &InstanceView,
    operation: SnapshotOperation,
) -> SnapshotExecutionPlan {
    compile_snapshot(
        app,
        &PlatformHostLauncherLookup,
        &SnapshotIntent {
            instance: view.id,
            operation,
            parameters: Vec::new(),
        },
        &[],
    )
    .unwrap()
}
fn accept(app: &PactrunApplication, plan: SnapshotExecutionPlan) -> RunId {
    app.accept_snapshot_plan(
        plan,
        AdmissionOptions::default(),
        ActionCancellation::default(),
    )
    .unwrap()
}
fn stop(app: &PactrunApplication, run: RunId) {
    app.stop_snapshot_before_launch(run, plain_finish(RunOutcome::Cancelled))
        .unwrap();
    assert!(app.advance_owner_continuation(run).unwrap());
}
fn store(
    p: &PactrunPersistence,
    view: &InstanceView,
    bindings: Vec<SnapshotBinding>,
    service: bool,
    mut blobs: BTreeMap<Sha256Digest, Vec<u8>>,
) -> SnapshotId {
    let id = SnapshotId::generate().unwrap();
    let service_content = if service {
        let bytes = b"selected-service-content".to_vec();
        let digest = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
        blobs.insert(digest.clone(), bytes);
        vec![SnapshotServiceContent {
            role: SnapshotServiceRole::parse("database").unwrap(),
            path: SnapshotContentPath::parse("state/data.bin").unwrap(),
            blob_digest: digest,
        }]
    } else {
        Vec::new()
    };
    let manifest = SnapshotManifest::new(SnapshotManifestParts {
        version: SnapshotIntegrityVersion::BASELINE,
        snapshot_id: id,
        producer: view.active_revision.clone(),
        origin_instance_id: view.id,
        captured_at: SnapshotTimestamp::new(0, 0).unwrap(),
        managed_bindings: bindings,
        service_content,
    })
    .unwrap();
    let verified = crate::snapshot_integrity::encode_snapshot_manifest(manifest, None).unwrap();
    let mut staged = p
        .staging_session()
        .unwrap()
        .create_snapshot_stage()
        .unwrap();
    {
        let mut writer = crate::snapshot_bundle::start_bundle(&mut staged, &verified).unwrap();
        for (digest, bytes) in blobs {
            writer.start_blob(&digest, bytes.len() as u64).unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    staged.finish_operation_file().unwrap();
    p.import_snapshot_bundle(
        &mut crate::snapshot_bundle::ValidatedSnapshotBundle::read(staged).unwrap(),
    )
    .unwrap();
    id
}
fn absent(id: &str) -> SnapshotBinding {
    SnapshotBinding {
        input_id: InputIdentity::parse(id).unwrap(),
        role: SnapshotBindingRole::Active,
        state: SnapshotBindingState::Absent,
        protection: ManagedInputProtection::Normal,
    }
}

// Test-ID: PR-TEST-0231
// Verifies: PR-REQ-0040, PR-REQ-0091, PR-REQ-0129, PR-REQ-0136, PR-REQ-0191, PR-REQ-0289
#[test]
fn snapshot_compiler_is_read_only_and_capture_readiness_precedes_acceptance() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision_with_inputs(
        &p,
        Operation::ObserveCapture,
        vec![input("required", true, InputProtectionV1::Normal)],
    );
    let view = empty_instance(&p, &revision, "compiler");
    let app = PactrunApplication::open_read_only(&path).unwrap();
    let sessions = fs::read_dir(path.join("staging")).unwrap().count();
    let intent = SnapshotIntent {
        instance: view.id,
        operation: SnapshotOperation::Capture,
        parameters: Vec::new(),
    };
    assert!(matches!(
        compile_snapshot(&app, &PlatformHostLauncherLookup, &intent, &[]),
        Err(ApplicationError::SnapshotCompilation(
            SnapshotPlanError::MissingRequired(_)
        ))
    ));
    assert_eq!(count(&p, "SELECT count(*) FROM runs"), 0);
    assert_eq!(
        fs::read_dir(path.join("staging")).unwrap().count(),
        sessions
    );
    let mut empty = Cursor::new(Vec::<u8>::new());
    p.set_input(
        view.id,
        view.state_version,
        &mut ManagedInputWrite {
            input_id: InputIdentity::parse("required").unwrap(),
            byte_len: 0,
            reader: &mut empty,
        },
    )
    .unwrap();
    let view = p.load_instance_by_id(view.id).unwrap().unwrap();
    let plan = compile(&app, &view, SnapshotOperation::Capture);
    assert_eq!(plan.access(), OperationAccessV1::Observe);
    assert_eq!(
        plan.steps(),
        &[
            SnapshotPlanStep::EstablishSession,
            SnapshotPlanStep::LaunchHook,
            SnapshotPlanStep::AcceptCompletion,
            SnapshotPlanStep::PublishManagedResult,
            SnapshotPlanStep::Finalize,
        ]
    );
    assert_eq!(count(&p, "SELECT count(*) FROM runs"), 0);
    assert_eq!(
        fs::read_dir(path.join("staging")).unwrap().count(),
        sessions
    );
    let bad = SnapshotIntent {
        parameters: vec![RawParameterInput {
            id: ParameterIdentity::parse("unknown").unwrap(),
            text: "private-value".to_owned(),
            source: ParameterTextSource::Protected,
        }],
        ..intent
    };
    let error = compile_snapshot(&app, &PlatformHostLauncherLookup, &bad, &[]).unwrap_err();
    assert!(!error.to_string().contains("private-value"));
    assert_eq!(count(&p, "SELECT count(*) FROM runs"), 0);
}

// Test-ID: PR-TEST-0232
// Verifies: PR-REQ-0289, PR-REQ-0291, PR-REQ-0298, PR-REQ-0091
#[test]
fn restore_admission_pins_selected_content_and_both_tokens_without_target_readiness() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision_with_inputs(
        &p,
        Operation::ObserveCapture,
        vec![input("required", true, InputProtectionV1::Normal)],
    );
    let view = empty_instance(&p, &revision, "restore-incomplete");
    let id = store(&p, &view, vec![absent("required")], true, BTreeMap::new());
    let app = PactrunApplication::open(&path).unwrap();
    let plan = compile(&app, &view, SnapshotOperation::Restore(id));
    assert_eq!(plan.access(), OperationAccessV1::Mutate);
    let run = accept(&app, plan);
    assert!(!app.advance_owner_continuation(run).unwrap());
    {
        let db = p.database.lock().unwrap();
        let row:(Vec<u8>,Vec<u8>,i64)=db.query_row("SELECT snapshot_id,admitted_state_version,admitted_consequence_version FROM run_restore_admissions WHERE run_id=?1",[run.as_bytes().as_slice()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(
            row,
            (
                id.as_bytes().to_vec(),
                view.state_version.as_bytes().to_vec(),
                0
            )
        );
        assert!(
            db.execute(
                "DELETE FROM snapshots WHERE snapshot_id=?1",
                [id.as_bytes().as_slice()]
            )
            .is_err()
        );
    }
    assert_eq!(count(&p, "SELECT count(*) FROM run_payload_pins"), 0);
    assert!(
        !p.load_instance_by_id(view.id)
            .unwrap()
            .unwrap()
            .required_inputs_satisfied
    );
    assert_eq!(token(&p, view.id), view.state_version);
    // The owner-only launch seam is reachable despite target incompleteness.
    let claim = app.claim_snapshot_execution(run).unwrap().unwrap();
    let mut launched = 0;
    let attempt = claim
        .launch_once(|plan| {
            assert_eq!(
                plan.operation(),
                &ManagedRunIdentity::Restore {
                    revision: view.active_revision.clone(),
                    snapshot: id
                }
            );
            launched += 1;
            Ok::<_, ()>(())
        })
        .unwrap()
        .unwrap();
    assert_eq!(launched, 1);
    drop(attempt);
    assert!(app.claim_snapshot_execution(run).unwrap().is_none());
    // S4 never substitutes bare success for the S6 atomic result publisher.
    assert!(
        p.finish_run_owned(
            &app.execution_owner(),
            run,
            &plain_finish(RunOutcome::Succeeded),
            &[],
            &mut []
        )
        .is_err()
    );
    p.finish_run_owned(
        &app.execution_owner(),
        run,
        &plain_finish(RunOutcome::Cancelled),
        &[],
        &mut [],
    )
    .unwrap();
    assert!(app.advance_owner_continuation(run).unwrap());
    assert_eq!(count(&p, "SELECT count(*) FROM run_restore_admissions"), 0);
    p.database
        .lock()
        .unwrap()
        .execute(
            "DELETE FROM snapshots WHERE snapshot_id=?1",
            [id.as_bytes().as_slice()],
        )
        .unwrap();
    assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Cancelled);
}

// Test-ID: PR-TEST-0233
// Verifies: PR-REQ-0007, PR-REQ-0289, PR-REQ-0291, PR-REQ-0298
#[test]
fn restore_revalidates_transitions_stale_state_and_deleted_source_without_launch() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision_with_inputs(
        &p,
        Operation::ObserveCapture,
        vec![input("required", true, InputProtectionV1::Normal)],
    );
    let view = empty_instance(&p, &revision, "restore-transition");
    let id = store(&p, &view, vec![absent("required")], false, BTreeMap::new());
    let app = PactrunApplication::open(&path).unwrap();
    let plan = compile(&app, &view, SnapshotOperation::Restore(id));
    let mut empty = Cursor::new(Vec::<u8>::new());
    p.set_input(
        view.id,
        view.state_version,
        &mut ManagedInputWrite {
            input_id: InputIdentity::parse("required").unwrap(),
            byte_len: 0,
            reader: &mut empty,
        },
    )
    .unwrap();
    let current = p.load_instance_by_id(view.id).unwrap().unwrap();
    assert!(
        compile_snapshot(
            &app,
            &PlatformHostLauncherLookup,
            &SnapshotIntent {
                instance: view.id,
                operation: SnapshotOperation::Restore(id),
                parameters: Vec::new()
            },
            &[]
        )
        .is_err()
    );
    assert_eq!(count(&p, "SELECT count(*) FROM runs"), 0);
    let run = accept(&app, plan);
    assert!(app.advance_owner_continuation(run).unwrap());
    assert!(app.claim_snapshot_execution(run).unwrap().is_none());
    assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Failed);
    assert_eq!(token(&p, view.id), current.state_version);
    let other = empty_instance(&p, &revision, "source-deleted");
    let source = store(&p, &other, vec![absent("required")], false, BTreeMap::new());
    let plan = compile(&app, &other, SnapshotOperation::Restore(source));
    let run = accept(&app, plan);
    p.database
        .lock()
        .unwrap()
        .execute(
            "DELETE FROM snapshots WHERE snapshot_id=?1",
            [source.as_bytes().as_slice()],
        )
        .unwrap();
    assert!(app.advance_owner_continuation(run).unwrap());
    assert!(app.claim_snapshot_execution(run).unwrap().is_none());
    assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Failed);
    assert_eq!(count(&p, "SELECT count(*) FROM run_restore_admissions"), 0);
    let empty_revision = managed_revision(&p, Operation::ObserveCapture);
    let target = empty_instance(&p, &empty_revision, "source-damaged");
    let damaged = store(&p, &target, Vec::new(), true, BTreeMap::new());
    let plan = compile(&app, &target, SnapshotOperation::Restore(damaged));
    let run = accept(&app, plan);
    crate::persistence::sqlite_snapshots::corrupt_snapshot_for_test(&path, damaged);
    assert!(app.advance_owner_continuation(run).unwrap());
    assert!(app.claim_snapshot_execution(run).unwrap().is_none());
    assert!(
        managed_finished(&p, run)
            .primary_failure
            .unwrap()
            .failure
            .message
            .contains("integrity")
    );
    assert!(p.verify_snapshot(damaged).is_err());
}

// Test-ID: PR-TEST-0234
// Verifies: PR-REQ-0289, PR-REQ-0298, PR-REQ-0091
#[test]
fn snapshot_owner_retains_retry_state_and_launch_is_one_shot_and_cancellation_gated() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision(&p, Operation::ObserveCapture);
    let view = empty_instance(&p, &revision, "owner-continuation");
    let app = PactrunApplication::open(&path).unwrap();
    let cancelled = ActionCancellation::default();
    cancelled.request();
    assert!(
        app.accept_snapshot_plan(
            compile(&app, &view, SnapshotOperation::Capture),
            AdmissionOptions::default(),
            cancelled
        )
        .is_err()
    );
    assert_eq!(count(&p, "SELECT count(*) FROM runs"), 0);
    let cancellation = ActionCancellation::default();
    let run = app
        .accept_snapshot_plan(
            compile(&app, &view, SnapshotOperation::Capture),
            AdmissionOptions::default(),
            cancellation.clone(),
        )
        .unwrap();
    crate::executor::fail_next_admission_for_test();
    assert!(app.advance_owner_continuation(run).is_err());
    assert!(app.claim_snapshot_execution(run).unwrap().is_none());
    assert_eq!(count(&p, "SELECT count(*) FROM runs"), 1);
    assert!(!app.advance_owner_continuation(run).unwrap());
    let claim = app.claim_snapshot_execution(run).unwrap().unwrap();
    assert!(app.claim_snapshot_execution(run).unwrap().is_none());
    cancellation.request();
    let mut launches = 0;
    assert!(
        claim
            .launch_once(|_| {
                launches += 1;
                Ok::<_, ()>(())
            })
            .unwrap()
            .is_none()
    );
    assert_eq!(launches, 0);
    crate::application::fail_next_finalization_advances_for_test(1);
    assert!(app.advance_owner_continuation(run).is_err());
    assert!(matches!(
        p.load_managed_run(run).unwrap().unwrap().state,
        RunState::Running(_)
    ));
    assert!(app.advance_owner_continuation(run).unwrap());
    assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Cancelled);
    let run = accept(&app, compile(&app, &view, SnapshotOperation::Capture));
    assert!(!app.advance_owner_continuation(run).unwrap());
    app.claim_snapshot_execution(run)
        .unwrap()
        .unwrap()
        .stop_before_launch(plain_finish(RunOutcome::Failed))
        .unwrap();
    assert!(app.advance_owner_continuation(run).unwrap());
    assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Failed);
    let run = accept(&app, compile(&app, &view, SnapshotOperation::Capture));
    assert!(!app.advance_owner_continuation(run).unwrap());
    let claim = app.claim_snapshot_execution(run).unwrap().unwrap();
    let attempted = claim.launch_once(|_| Err::<(), _>("spawn failed"));
    assert!(attempted.is_err());
    drop(attempted);
    assert!(app.claim_snapshot_execution(run).unwrap().is_none());
    assert!(
        app.stop_snapshot_before_launch(run, plain_finish(RunOutcome::Failed))
            .is_err()
    );
    assert!(!app.advance_owner_continuation(run).unwrap());
    app.abandon_execution_owner();
    let next = PactrunApplication::open(&path).unwrap();
    assert_eq!(next.reconcile_lost_managed_owners().unwrap(), vec![run]);
    assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Interrupted);
    let first = accept(&next, compile(&next, &view, SnapshotOperation::Capture));
    let second = accept(&next, compile(&next, &view, SnapshotOperation::Capture));
    let guard = next.take_owner_continuation(first).unwrap();
    guard.replace(next.take_owner_continuation(second).unwrap().complete());
    assert!(next.advance_owner_continuation(first).is_err());
    assert!(next.claim_snapshot_execution(first).is_err());
}

fn specialized_revision(p: &PactrunPersistence, protocol: i64) -> RevisionIdentity {
    let base = managed_revision(p, Operation::ObserveCapture);
    let stored = p.load_revision(&base).unwrap().unwrap();
    let mut capability = stored.content.core.snapshot().unwrap().clone();
    capability.capture.as_mut().unwrap().hook.protocol_version =
        (if protocol == 1 || protocol == 2 { "1.0-alpha.1" } else { "1.0-alpha.2" }).parse().unwrap();
    capability.capture.as_mut().unwrap().parameters = vec![
        ParameterV1 {
            id: ParameterIdentity::parse("count").unwrap(),
            parameter_type: ParameterTypeV1::Integer,
            sensitive: false,
            default: Some(ParameterDefaultV1::Integer(SafeIntegerV1::new(7).unwrap())),
        },
        ParameterV1 {
            id: ParameterIdentity::parse("token").unwrap(),
            parameter_type: ParameterTypeV1::String,
            sensitive: true,
            default: None,
        },
    ];
    capability.restore.as_mut().unwrap().parameters = vec![ParameterV1 {
        id: ParameterIdentity::parse("enabled").unwrap(),
        parameter_type: ParameterTypeV1::Boolean,
        sensitive: false,
        default: Some(ParameterDefaultV1::Boolean(true)),
    }];
    let core = project_revision_declarations(RevisionDeclarationInput {
        inputs: Vec::new(),
        actions: stored.content.core.actions().to_vec(),
        snapshot: Some(capability),
        migrations: Vec::new(),
        cleanup: None,
    })
    .unwrap();
    let publication = p
        .put_runtime_content(
            &stored.content.runtime_content.files()[0].blob_digest,
            &mut Cursor::new(TOOL_BYTES),
        )
        .unwrap();
    p.persist_revision(
        base.package_id,
        &validate_declaration_content(core, stored.content.runtime_content).unwrap(),
        &[publication],
    )
    .unwrap()
}

// Test-ID: PR-TEST-0235
// Verifies: PR-REQ-0289, PR-REQ-0191
#[test]
fn snapshot_parameters_protocol_and_runtime_are_qualified_without_persisting_values() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = specialized_revision(&p, 1);
    let view = empty_instance(&p, &revision, "parameterized");
    let app = PactrunApplication::open(&path).unwrap();
    let intent = SnapshotIntent {
        instance: view.id,
        operation: SnapshotOperation::Capture,
        parameters: vec![RawParameterInput {
            id: ParameterIdentity::parse("token").unwrap(),
            text: "sensitive-snapshot-value".to_owned(),
            source: ParameterTextSource::Protected,
        }],
    };
    let plan = compile_snapshot(&app, &PlatformHostLauncherLookup, &intent, &[]).unwrap();
    assert!(
        plan.parameters()[0].value()
            == &InvocationParameterValue::Integer(SafeIntegerV1::new(7).unwrap())
    );
    assert!(plan.parameters()[1].effective_redaction);
    assert!(!format!("{plan:?}").contains("sensitive-snapshot-value"));
    let mut bad = intent.clone();
    bad.parameters.push(RawParameterInput {
        id: ParameterIdentity::parse("count").unwrap(),
        text: "not-an-integer".to_owned(),
        source: ParameterTextSource::Protected,
    });
    assert!(
        !compile_snapshot(&app, &PlatformHostLauncherLookup, &bad, &[])
            .unwrap_err()
            .to_string()
            .contains("not-an-integer")
    );
    bad.parameters = intent
        .parameters
        .iter()
        .cloned()
        .chain(intent.parameters.iter().cloned())
        .collect();
    assert!(compile_snapshot(&app, &PlatformHostLauncherLookup, &bad, &[]).is_err());
    assert_eq!(count(&p, "SELECT count(*) FROM runs"), 0);
    let run = accept(&app, plan);
    assert!(!app.advance_owner_continuation(run).unwrap());
    stop(&app, run);
    let source = store(&p, &view, Vec::new(), false, BTreeMap::new());
    let restore = compile(&app, &view, SnapshotOperation::Restore(source));
    assert_eq!(restore.parameters()[0].id.as_str(), "enabled");
    assert!(restore.parameters()[0].value() == &InvocationParameterValue::Boolean(true));
    let foreign = specialized_revision(&p, 3);
    let unsupported = empty_instance(&p, &foreign, "protocol-three");
    let mut future = intent.clone();
    future.instance = unsupported.id;
    assert!(compile_snapshot(&app, &PlatformHostLauncherLookup, &future, &[]).is_err());
    // The shared compiler's physical-runtime check cannot be waived by a
    // catalog entry. Remove only this fixture's exact immutable runtime file.
    let digest = p
        .load_revision(&revision)
        .unwrap()
        .unwrap()
        .content
        .runtime_content
        .files()[0]
        .blob_digest
        .clone();
    let ready = compile_snapshot(&app, &PlatformHostLauncherLookup, &intent, &[]).unwrap();
    let runtime_file = path
        .join("runtime-content")
        .join(digest.as_str().strip_prefix("sha256:").unwrap());
    fs::remove_file(&runtime_file).unwrap();
    assert!(compile_snapshot(&app, &PlatformHostLauncherLookup, &intent, &[]).is_err());
    let run = accept(&app, ready);
    assert!(app.advance_owner_continuation(run).unwrap());
    assert!(app.claim_snapshot_execution(run).unwrap().is_none());
    assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Failed);
}

// Test-ID: PR-TEST-0238
// Verifies: PR-REQ-0289, PR-REQ-0045, PR-REQ-0091
#[test]
fn forged_compiler_observations_cannot_waive_access_or_readiness_at_admission() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision(&p, Operation::MutateCapture);
    let view = empty_instance(&p, &revision, "forged-access");
    let app = PactrunApplication::open(&path).unwrap();
    let intent = SnapshotIntent {
        instance: view.id,
        operation: SnapshotOperation::Capture,
        parameters: Vec::new(),
    };
    let mut observed = p.observe_snapshot_compilation(&intent).unwrap();
    let mut capability = observed.revision.core.snapshot().unwrap().clone();
    capability.capture.as_mut().unwrap().access = OperationAccessV1::Observe;
    let core = project_revision_declarations(RevisionDeclarationInput {
        inputs: Vec::new(),
        actions: observed.revision.core.actions().to_vec(),
        snapshot: Some(capability),
        migrations: Vec::new(),
        cleanup: None,
    })
    .unwrap();
    observed.revision =
        validate_declaration_content(core, observed.revision.runtime_content).unwrap().into();
    let launch = CompiledHookLaunch::Direct {
        executable: observed.revision.runtime_content.files()[0].clone(),
    };
    let forged = build_snapshot_plan(&intent, observed, launch).unwrap();
    assert_eq!(forged.access(), OperationAccessV1::Observe);
    let run = accept(&app, forged);
    assert!(app.advance_owner_continuation(run).unwrap());
    assert!(app.claim_snapshot_execution(run).unwrap().is_none());
    assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Failed);
    let required = managed_revision_with_inputs(
        &p,
        Operation::ObserveCapture,
        vec![input("required", true, InputProtectionV1::Normal)],
    );
    let incomplete = empty_instance(&p, &required, "forged-ready");
    let intent = SnapshotIntent {
        instance: incomplete.id,
        ..intent
    };
    let mut observed = p.observe_snapshot_compilation(&intent).unwrap();
    observed.instance.required_inputs_satisfied = true;
    for binding in &mut observed.instance.bindings {
        binding.present = true;
    }
    let launch = CompiledHookLaunch::Direct {
        executable: observed.revision.runtime_content.files()[0].clone(),
    };
    let run = accept(
        &app,
        build_snapshot_plan(&intent, observed, launch).unwrap(),
    );
    assert!(app.advance_owner_continuation(run).unwrap());
    let mut launches = 0;
    if let Some(claim) = app.claim_snapshot_execution(run).unwrap() {
        let _ = claim.launch_once(|_| {
            launches += 1;
            Ok::<_, ()>(())
        });
    }
    assert_eq!(launches, 0);
    assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Failed);
}

// Test-ID: PR-TEST-0239
// Verifies: PR-REQ-0289, PR-REQ-0291, PR-REQ-0298
#[test]
fn restore_admission_captures_nonzero_consequence_without_clearing_or_refreshing_guards() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision(&p, Operation::ObserveCapture);
    let view = empty_instance(&p, &revision, "guard-tokens");
    let source = store(&p, &view, Vec::new(), false, BTreeMap::new());
    let guard_run = admitted(&p, &view);
    p.open_recovery_risk(guard_run).unwrap();
    p.finish_run(guard_run, &plain_finish(RunOutcome::Failed), &mut [])
        .unwrap();
    let guarded = p.load_instance_by_id(view.id).unwrap().unwrap();
    let app = PactrunApplication::open(&path).unwrap();
    let refused = accept(
        &app,
        compile(&app, &guarded, SnapshotOperation::Restore(source)),
    );
    assert!(app.advance_owner_continuation(refused).unwrap());
    assert_eq!(
        managed_finished(&p, refused)
            .primary_failure
            .unwrap()
            .failure
            .error,
        AdmissionRefusal::RecoveryGuardActive.error_ref()
    );
    let run = app
        .accept_snapshot_plan(
            compile(&app, &guarded, SnapshotOperation::Restore(source)),
            AdmissionOptions {
                recovery_override: true,
            },
            ActionCancellation::default(),
        )
        .unwrap();
    assert!(!app.advance_owner_continuation(run).unwrap());
    let row = || {
        p.database.lock().unwrap().query_row("SELECT admitted_state_version,admitted_consequence_version FROM run_restore_admissions WHERE run_id=?1",[run.as_bytes().as_slice()],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,i64>(1)?))).unwrap()
    };
    assert_eq!(row(), (guarded.state_version.as_bytes().to_vec(), 1));
    assert!(p.load_instance_recovery_guard(view.id).unwrap().is_some());
    p.resolve_manual_recovery(view.id, guarded.state_version)
        .unwrap();
    let current = p.load_instance_by_id(view.id).unwrap().unwrap();
    let other = admitted(&p, &current);
    p.open_recovery_risk(other).unwrap();
    p.finish_run(other, &plain_finish(RunOutcome::Failed), &mut [])
        .unwrap();
    assert_eq!(p.recovery_consequence_version(view.id).unwrap(), 2);
    assert_eq!(row(), (guarded.state_version.as_bytes().to_vec(), 1));
    stop(&app, run);
    assert_eq!(
        p.load_instance_recovery_guard(view.id)
            .unwrap()
            .unwrap()
            .run,
        other
    );
    assert_eq!(p.recovery_consequence_version(view.id).unwrap(), 2);
}

// Test-ID: PR-TEST-0237
// Verifies: PR-REQ-0289, PR-REQ-0293
#[test]
fn valid_import_beyond_former_restore_expansion_can_be_admitted() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision(&p, Operation::ObserveCapture);
    let view = empty_instance(&p, &revision, "expansion");
    let bytes = vec![0x2a; 1024 * 1024];
    let digest = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
    let bindings = (0..32769)
        .map(|n| SnapshotBinding {
            input_id: InputIdentity::parse(format!("old{n:05}")).unwrap(),
            role: SnapshotBindingRole::Retained,
            state: SnapshotBindingState::Bound(digest.clone()),
            protection: ManagedInputProtection::Normal,
        })
        .collect::<Vec<_>>();
    let source = store(
        &p,
        &view,
        bindings.clone(),
        false,
        BTreeMap::from([(digest, bytes)]),
    );
    assert_eq!(
        p.verify_snapshot(source).unwrap().relational,
        SnapshotRelationalVerification::Valid
    );
    let app = PactrunApplication::open(&path).unwrap();
    let intent = SnapshotIntent {
        instance: view.id,
        operation: SnapshotOperation::Restore(source),
        parameters: Vec::new(),
    };
    let plan = compile_snapshot(&app, &PlatformHostLauncherLookup, &intent, &[]).unwrap();
    assert_eq!(count(&p, "SELECT count(*) FROM runs"), 0);
    let run = accept(&app, plan);
    assert!(!app.advance_owner_continuation(run).unwrap());
    let claim = app.claim_snapshot_execution(run).unwrap().expect("former byte ceiling no longer refuses Admission");
    claim.stop_before_launch(plain_finish(RunOutcome::Cancelled)).unwrap();
    assert!(app.advance_owner_continuation(run).unwrap());
    assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Cancelled);
    assert_eq!(count(&p, "SELECT count(*) FROM run_restore_admissions"), 0);
}

const RESTORE_WORKER: &str =
    "persistence::sqlite_runs::tests::managed_access::lifecycle::substrate::restore_owner_worker";

// Supporting monotonic-token corruption coverage for PR-TEST-0239.
#[test]
fn restore_cannot_claim_a_consequence_version_from_the_future() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision(&p, Operation::ObserveCapture);
    let view = empty_instance(&p, &revision, "future-token");
    let source = store(&p, &view, Vec::new(), false, BTreeMap::new());
    let app = PactrunApplication::open(&path).unwrap();
    let run = accept(
        &app,
        compile(&app, &view, SnapshotOperation::Restore(source)),
    );
    assert!(!app.advance_owner_continuation(run).unwrap());
    p.database
        .lock()
        .unwrap()
        .execute(
            "UPDATE run_restore_admissions SET admitted_consequence_version=1 WHERE run_id=?1",
            [run.as_bytes().as_slice()],
        )
        .unwrap();
    assert!(matches!(
        p.load_managed_run(run),
        Err(PersistenceError::CorruptRun(_))
    ));
    assert!(app.advance_owner_continuation(run).is_err());
    assert!(app.claim_snapshot_execution(run).is_err());
    assert_eq!(count(&p, "SELECT count(*) FROM run_outcomes"), 0);
    assert_eq!(p.recovery_consequence_version(view.id).unwrap(), 0);
}

fn signal(path: &Path, value: &str) {
    let pending = path.with_extension("pending");
    fs::write(&pending, value).unwrap();
    fs::rename(pending, path).unwrap();
}
#[test]
fn restore_owner_worker() {
    let Some(path) = std::env::var_os("PACTRUN_S4_RESTORE_ROOT") else {
        return;
    };
    let path = PathBuf::from(path);
    let label = std::env::var("PACTRUN_S4_RESTORE_LABEL").unwrap();
    let source = std::env::var("PACTRUN_S4_RESTORE_SOURCE")
        .unwrap()
        .parse::<SnapshotId>()
        .unwrap();
    let p = PactrunPersistence::open(&path).unwrap();
    let id = p
        .resolve_instance_name(&InstanceName::parse("restore-race").unwrap())
        .unwrap()
        .unwrap();
    let view = p.load_instance_by_id(id).unwrap().unwrap();
    let app = PactrunApplication::open(&path).unwrap();
    let run = accept(
        &app,
        compile(&app, &view, SnapshotOperation::Restore(source)),
    );
    signal(&path.join(format!("{label}.accepted")), &run.to_string());
    wait_for_file(&path.join("restore-go"));
    let finished = app.advance_owner_continuation(run).unwrap();
    if !finished {
        p.open_recovery_risk(run).unwrap();
    }
    signal(
        &path.join(format!("{label}.result")),
        if finished { "refused" } else { "admitted" },
    );
    wait_for_file(&path.join("restore-release"));
}
fn spawn_restore(
    path: &Path,
    source: SnapshotId,
    label: &str,
    fault: Option<&str>,
) -> ManagedWorker {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", RESTORE_WORKER, "--nocapture"])
        .env("PACTRUN_S4_RESTORE_ROOT", path)
        .env("PACTRUN_S4_RESTORE_LABEL", label)
        .env("PACTRUN_S4_RESTORE_SOURCE", source.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    if let Some(fault) = fault {
        command.env("PACTRUN_M4_FAULT", fault);
    }
    ManagedWorker(command.spawn().unwrap())
}

// Test-ID: PR-TEST-0236
// Verifies: PR-REQ-0289, PR-REQ-0045, PR-REQ-0298
#[test]
fn real_restore_owners_share_exclusivity_with_observe_capture_and_reconcile_once() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision(&p, Operation::ObserveCapture);
    let view = empty_instance(&p, &revision, "restore-race");
    let source = store(&p, &view, Vec::new(), true, BTreeMap::new());
    let app = PactrunApplication::open(&path).unwrap();
    let mut children = [
        spawn_restore(&path, source, "left", None),
        spawn_restore(&path, source, "right", None),
    ];
    for label in ["left", "right"] {
        wait_for_file(&path.join(format!("{label}.accepted")));
    }
    signal(&path.join("restore-go"), "go");
    for label in ["left", "right"] {
        wait_for_file(&path.join(format!("{label}.result")));
    }
    let result = ["left", "right"]
        .map(|label| fs::read_to_string(path.join(format!("{label}.result"))).unwrap());
    assert_eq!(
        result.iter().filter(|r| r.as_str() == "admitted").count(),
        1
    );
    assert_eq!(result.iter().filter(|r| r.as_str() == "refused").count(), 1);
    let winner = result.iter().position(|r| r == "admitted").unwrap();
    let label = ["left", "right"][winner];
    let run = fs::read_to_string(path.join(format!("{label}.accepted")))
        .unwrap()
        .parse::<RunId>()
        .unwrap();
    let capture = accept(&app, compile(&app, &view, SnapshotOperation::Capture));
    assert!(!app.advance_owner_continuation(capture).unwrap());
    stop(&app, capture);
    assert!(app.reconcile_lost_managed_owners().unwrap().is_empty());
    children[winner].0.kill().unwrap();
    children[winner].0.wait().unwrap();
    assert_eq!(app.reconcile_lost_managed_owners().unwrap(), vec![run]);
    assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Interrupted);
    assert_eq!(p.recovery_consequence_version(view.id).unwrap(), 1);
    assert!(app.reconcile_lost_managed_owners().unwrap().is_empty());
    assert_eq!(count(&p, "SELECT count(*) FROM run_restore_admissions"), 0);
    assert!(
        p.load_instance_by_id(view.id)
            .unwrap()
            .unwrap()
            .bindings
            .is_empty()
    );
    signal(&path.join("restore-release"), "release");
    assert!(children[1 - winner].0.wait().unwrap().success());
}

// Test-ID: PR-TEST-0240
// Verifies: PR-REQ-0289, PR-REQ-0298
#[test]
fn restore_admission_crash_has_only_accepted_or_complete_admitted_pins() {
    for (fault, pinned) in [
        ("before_run_admit_commit", false),
        ("after_run_admit_commit", true),
    ] {
        let (_tmp, path) = root();
        let p = PactrunPersistence::open(&path).unwrap();
        let revision = managed_revision(&p, Operation::ObserveCapture);
        let view = empty_instance(&p, &revision, "restore-race");
        let source = store(&p, &view, Vec::new(), true, BTreeMap::new());
        let mut child = spawn_restore(&path, source, "crash", Some(fault));
        wait_for_file(&path.join("crash.accepted"));
        let run = fs::read_to_string(path.join("crash.accepted"))
            .unwrap()
            .parse::<RunId>()
            .unwrap();
        signal(&path.join("restore-go"), "go");
        assert_eq!(child.0.wait().unwrap().code(), Some(87));
        assert_eq!(
            count(&p, "SELECT count(*) FROM run_restore_admissions"),
            i64::from(pinned)
        );
        assert_eq!(
            has_revision_pin(&p.database.lock().unwrap(), run).unwrap(),
            pinned
        );
        assert_eq!(count(&p, "SELECT count(*) FROM run_outcomes"), 0);
        let app = PactrunApplication::open(&path).unwrap();
        assert_eq!(app.reconcile_lost_managed_owners().unwrap(), vec![run]);
        assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Interrupted);
        assert_eq!(p.recovery_consequence_version(view.id).unwrap(), 0);
        assert_eq!(count(&p, "SELECT count(*) FROM run_restore_admissions"), 0);
        assert_eq!(token(&p, view.id), view.state_version);
    }
}

// Test-ID: PR-TEST-0241
// Verifies: PR-REQ-0291, PR-REQ-0191
#[test]
fn restore_sticky_protection_is_checked_independently_of_tokens_without_declassification() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision_with_inputs(
        &p,
        Operation::ObserveCapture,
        vec![input("required", true, InputProtectionV1::Normal)],
    );
    let mut empty = Cursor::new(Vec::<u8>::new());
    let view = p
        .create_instance(
            InstanceName::parse("sticky-target").unwrap(),
            revision,
            &mut [ManagedInputWrite {
                input_id: InputIdentity::parse("required").unwrap(),
                byte_len: 0,
                reader: &mut empty,
            }],
        )
        .unwrap();
    let digest = Sha256Digest::from_bytes(Sha256::digest([]).into());
    let descriptor = SnapshotBinding {
        input_id: InputIdentity::parse("required").unwrap(),
        role: SnapshotBindingRole::Active,
        state: SnapshotBindingState::Bound(digest.clone()),
        protection: ManagedInputProtection::Normal,
    };
    let normal = store(
        &p,
        &view,
        vec![descriptor.clone()],
        false,
        BTreeMap::from([(digest.clone(), Vec::new())]),
    );
    let app = PactrunApplication::open(&path).unwrap();
    let previously_compiled = compile(&app, &view, SnapshotOperation::Restore(normal));
    // Inject a legal sticky-Secret target with the same token to prove that
    // Admission does not use token equality instead of the protection check.
    p.database
        .lock()
        .unwrap()
        .execute(
            "UPDATE managed_input_payloads SET protection_rank=1 WHERE instance_id=?1",
            [view.id.as_bytes().as_slice()],
        )
        .unwrap();
    let current = p.load_instance_by_id(view.id).unwrap().unwrap();
    assert!(
        compile_snapshot(
            &app,
            &PlatformHostLauncherLookup,
            &SnapshotIntent {
                instance: view.id,
                operation: SnapshotOperation::Restore(normal),
                parameters: Vec::new()
            },
            &[]
        )
        .is_err()
    );
    let run = accept(&app, previously_compiled);
    assert!(app.advance_owner_continuation(run).unwrap());
    assert!(app.claim_snapshot_execution(run).unwrap().is_none());
    assert!(
        managed_finished(&p, run)
            .primary_failure
            .unwrap()
            .failure
            .message
            .contains("Secret")
    );
    let secret = store(
        &p,
        &current,
        vec![SnapshotBinding {
            protection: ManagedInputProtection::Secret,
            ..descriptor
        }],
        false,
        BTreeMap::from([(digest, Vec::new())]),
    );
    let run = accept(
        &app,
        compile(&app, &current, SnapshotOperation::Restore(secret)),
    );
    assert!(!app.advance_owner_continuation(run).unwrap());
    stop(&app, run);
    assert_eq!(
        p.load_instance_by_id(view.id).unwrap().unwrap().bindings[0].protection,
        ManagedInputProtection::Secret
    );
    // Optional deletion and retained omission do not acquire declassification
    // authority, but remain legal removals rather than blanket Secret failures.
    let manifest = SnapshotManifest::new(SnapshotManifestParts {
        version: SnapshotIntegrityVersion::BASELINE,
        snapshot_id: SnapshotId::generate().unwrap(),
        producer: current.active_revision,
        origin_instance_id: view.id,
        captured_at: SnapshotTimestamp::new(0, 0).unwrap(),
        managed_bindings: vec![absent("required")],
        service_content: Vec::new(),
    })
    .unwrap();
    assert!(
        validate_restore_transition(
            &[
                ManagedInputBindingView {
                    input_id: InputIdentity::parse("required").unwrap(),
                    role: ManagedInputRole::Active { required: false },
                    present: true,
                    protection: ManagedInputProtection::Secret
                },
                ManagedInputBindingView {
                    input_id: InputIdentity::parse("retained").unwrap(),
                    role: ManagedInputRole::Retained,
                    present: true,
                    protection: ManagedInputProtection::Secret
                }
            ],
            &manifest
        )
        .is_ok()
    );
}
