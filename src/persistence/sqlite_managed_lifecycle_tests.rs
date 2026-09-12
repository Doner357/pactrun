use super::*;
use crate::domain::{ManagedInputProtection, ManagedInputRole, SnapshotPlanStep};

mod substrate {
    include!("sqlite_snapshot_substrate_tests.rs");
}

fn capture_run(
    p: &PactrunPersistence,
    view: &InstanceView,
    owner: &ExecutionOwnerSession,
) -> RunId {
    p.create_accepted_managed_run(
        RunId::generate().unwrap(),
        view.id,
        view.state_version,
        &ManagedRunIdentity::Capture {
            revision: view.active_revision.clone(),
        },
        owner,
        &crate::persistence::UnconditionalAcceptance,
    )
    .unwrap()
}

fn capture_admit(
    p: &PactrunPersistence,
    view: &InstanceView,
    run: RunId,
    owner: &ExecutionOwnerSession,
) -> Result<Result<(), AdmissionRefusal>, PersistenceError> {
    let stored = p.load_revision(&view.active_revision)?.unwrap();
    let files = stored.content.runtime_content.files();
    let launch = CompiledHookLaunch::Direct {
        executable: files[0].clone(),
    };
    let active = bindings(p, view.id);
    p.admit_managed_run(
        run,
        owner,
        &AdmissionFacts {
            expected_state_version: view.state_version,
            active_bindings: &active,
            runtime_content: files,
            launch: &launch,
        },
        &|_| Ok(()),
        false,
    )
}

fn input(id: &str, required: bool, protection: InputProtectionV1) -> InputDeclarationV1 {
    InputDeclarationV1 {
        id: InputIdentity::parse(id).unwrap(),
        required,
        protection,
    }
}

fn managed_finished(p: &PactrunPersistence, run: RunId) -> RunOutcomeView {
    match p.load_managed_run(run).unwrap().unwrap().state {
        RunState::Finished(outcome) => outcome,
        _ => panic!("expected Finished"),
    }
}

// Test-ID: PR-TEST-0224
// Verifies: PR-REQ-0289, PR-REQ-0298
#[test]
fn typed_acceptance_and_operation_aware_loading_preserve_action_ranks_and_cancellation() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision(&p, Operation::ObserveCapture);
    let view = empty_instance(&p, &revision, "typed-acceptance");
    let operations = [
        ManagedRunIdentity::Action(action(&revision)),
        ManagedRunIdentity::Capture {
            revision: revision.clone(),
        },
        ManagedRunIdentity::Restore {
            revision: revision.clone(),
            snapshot: SnapshotId::generate().unwrap(),
        },
    ];
    for operation in operations {
        let cancelled = RunId::generate().unwrap();
        assert!(matches!(
            p.create_accepted_managed_run(
                cancelled,
                view.id,
                view.state_version,
                &operation,
                &owner(),
                &CancelBeforeAcceptance
            ),
            Err(AcceptanceError::Cancelled { .. })
        ));
        assert_eq!(p.load_managed_run(cancelled).unwrap(), None);
        let uncertain = RunId::generate().unwrap();
        assert!(
            matches!(p.create_accepted_managed_run(uncertain, view.id, view.state_version,
            &operation, &owner(), &UncertainAfterAcceptance), Err(AcceptanceError::Uncertain { run, .. }) if run == uncertain)
        );
        assert_eq!(
            p.load_managed_run(uncertain).unwrap().unwrap().operation,
            operation
        );
        p.finish_run_owned(
            &owner(),
            uncertain,
            &plain_finish(RunOutcome::Cancelled),
            &[],
            &mut [],
        )
        .unwrap();
        let run = p
            .create_accepted_managed_run(
                RunId::generate().unwrap(),
                view.id,
                view.state_version,
                &operation,
                &owner(),
                &crate::persistence::UnconditionalAcceptance,
            )
            .unwrap();
        let loaded = p.load_managed_run(run).unwrap().unwrap();
        assert_eq!(loaded.operation, operation);
        assert_eq!(loaded.accepted_state_version, view.state_version);
        assert!(matches!(
            loaded.state,
            RunState::Running(RunExecutionView {
                boundary: ActionRunBoundary::Accepted,
                risk_state: RecoveryRiskState::Clear,
                ..
            })
        ));
        p.finish_run_owned(
            &owner(),
            run,
            &plain_finish(RunOutcome::Cancelled),
            &[],
            &mut [],
        )
        .unwrap();
        assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Cancelled);
    }
    let run = capture_run(&p, &view, &owner());
    capture_admit(&p, &view, run, &owner()).unwrap().unwrap();
    let mut finish = plain_finish(RunOutcome::Failed);
    finish.primary_failure = Some(RunPrimaryFailure {
        failure: RunFailureRecord {
            error: error("execution", "workspace_cleanup_failed"),
            message: "fixed diagnostic".to_owned(),
        },
        step: RunFailedStep::Plan(crate::domain::ActionPlanStep::PublishDeclaredOutputs),
    });
    assert!(matches!(
        p.finish_run_owned(&owner(), run, &finish, &[], &mut []),
        Err(PersistenceError::InvalidRunTransition(_))
    ));
    finish.primary_failure.as_mut().unwrap().step =
        RunFailedStep::SnapshotPlan(SnapshotPlanStep::PublishManagedResult);
    p.finish_run_owned(&owner(), run, &finish, &[], &mut [])
        .unwrap();
    let reopened = PactrunPersistence::open(&path).unwrap();
    assert_eq!(
        managed_finished(&reopened, run).primary_failure,
        finish.primary_failure
    );
    assert_eq!(finish.primary_failure.unwrap().step.rank(), 4);
    assert_eq!(
        RunFailedStep::from_rank(4).unwrap(),
        RunFailedStep::Plan(crate::domain::ActionPlanStep::PublishDeclaredOutputs)
    );
}

// Test-ID: PR-TEST-0225
// Verifies: PR-REQ-0289, PR-REQ-0091, PR-REQ-0191
#[test]
fn capture_admission_rechecks_readiness_owner_and_exact_facts_without_caller_authority() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision_with_inputs(
        &p,
        Operation::ObserveCapture,
        vec![
            input("required", true, InputProtectionV1::Normal),
            input("optional", false, InputProtectionV1::Secret),
        ],
    );
    let incomplete = empty_instance(&p, &revision, "incomplete");
    let run = capture_run(&p, &incomplete, &owner());
    assert!(
        matches!(capture_admit(&p, &incomplete, run, &owner()).unwrap(),
        Err(AdmissionRefusal::PlanInvalidated(message)) if message.contains("required Inputs") && message.contains("required"))
    );
    let outcome = managed_finished(&p, run);
    assert_eq!(outcome.boundary, ActionRunBoundary::Accepted);
    assert_eq!(outcome.outcome, RunOutcome::Failed);
    assert_eq!(outcome.hook_completion, None);
    assert!(!has_revision_pin(&p.database.lock().unwrap(), run).unwrap());
    let mut empty = Cursor::new(Vec::<u8>::new());
    let version = p
        .set_input(
            incomplete.id,
            incomplete.state_version,
            &mut ManagedInputWrite {
                input_id: InputIdentity::parse("required").unwrap(),
                byte_len: 0,
                reader: &mut empty,
            },
        )
        .unwrap();
    let view = p.load_instance_by_id(incomplete.id).unwrap().unwrap();
    assert_eq!(view.state_version, version);
    assert!(view.required_inputs_satisfied);
    let accepted = capture_run(&p, &view, &owner());
    let other_owner = ExecutionOwnerSession::parse(format!("session-{}", "2".repeat(32))).unwrap();
    assert!(matches!(
        capture_admit(&p, &view, accepted, &other_owner),
        Err(PersistenceError::InvalidRunTransition(_))
    ));
    capture_admit(&p, &view, accepted, &owner())
        .unwrap()
        .unwrap();
    let registry = p.admitted_capture_registry(accepted).unwrap();
    assert!(
        registry
            .iter()
            .find(|b| b.input.as_str() == "required")
            .unwrap()
            .payload
            .is_some()
    );
    let absent = registry
        .iter()
        .find(|b| b.input.as_str() == "optional")
        .unwrap();
    assert_eq!(absent.payload, None);
    assert_eq!(absent.protection, ManagedInputProtection::Secret);
    assert_eq!(token(&p, view.id), version);
    for defect in [
        "missing-runtime",
        "wrong-launch",
        "missing-active",
        "wrong-protection",
    ] {
        let run = capture_run(&p, &view, &owner());
        let stored = p.load_revision(&revision).unwrap().unwrap();
        let mut files = stored.content.runtime_content.files().to_vec();
        let mut launch = CompiledHookLaunch::Direct {
            executable: files[0].clone(),
        };
        let mut active = bindings(&p, view.id);
        match defect {
            "missing-runtime" => files.clear(),
            "wrong-launch" => {
                if let CompiledHookLaunch::Direct { executable } = &mut launch {
                    executable.path = RuntimePath::parse("wrong/tool").unwrap();
                }
            }
            "missing-active" => active.clear(),
            "wrong-protection" => active[0].protection = ManagedInputProtection::Secret,
            _ => unreachable!(),
        }
        assert!(
            matches!(
                p.admit_managed_run(
                    run,
                    &owner(),
                    &AdmissionFacts {
                        expected_state_version: version,
                        active_bindings: &active,
                        runtime_content: &files,
                        launch: &launch,
                    },
                    &|_| Ok(()),
                    false
                )
                .unwrap(),
                Err(AdmissionRefusal::PlanInvalidated(_))
            ),
            "{defect}"
        );
        assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Failed);
    }
}

// Test-ID: PR-TEST-0226
// Verifies: PR-REQ-0289, PR-REQ-0298
#[test]
fn capture_pins_complete_registry_and_never_reconstructs_it_from_current_bindings() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let original = managed_revision_with_inputs(
        &p,
        Operation::ObserveCapture,
        vec![
            input("required", true, InputProtectionV1::Normal),
            input("retired", false, InputProtectionV1::Secret),
        ],
    );
    let active = managed_revision_with_inputs(
        &p,
        Operation::ObserveCapture,
        vec![
            input("required", true, InputProtectionV1::Normal),
            input("optional", false, InputProtectionV1::Secret),
        ],
    );
    let secret = b"retained-secret";
    let mut required_reader = Cursor::new(Vec::<u8>::new());
    let mut secret_reader = Cursor::new(secret);
    let view = p
        .create_instance(
            InstanceName::parse("pinned-registry").unwrap(),
            original,
            &mut [
                ManagedInputWrite {
                    input_id: InputIdentity::parse("required").unwrap(),
                    byte_len: 0,
                    reader: &mut required_reader,
                },
                ManagedInputWrite {
                    input_id: InputIdentity::parse("retired").unwrap(),
                    byte_len: secret.len() as u64,
                    reader: &mut secret_reader,
                },
            ],
        )
        .unwrap();
    // Pre-existing retained-state fixture; this does not implement Migration execution.
    p.database.lock().unwrap().execute("UPDATE instances SET active_package_id=?2,active_revision_content_digest=?3 WHERE instance_id=?1",
        params![view.id.as_bytes().as_slice(),active.package_id.as_bytes().as_slice(),active.content_digest.as_bytes().as_slice()]).unwrap();
    let view = p.load_instance_by_id(view.id).unwrap().unwrap();
    let run = capture_run(&p, &view, &owner());
    capture_admit(&p, &view, run, &owner()).unwrap().unwrap();
    let registry = p.admitted_capture_registry(run).unwrap();
    assert_eq!(registry.len(), 3);
    let retained = registry
        .iter()
        .find(|b| b.input.as_str() == "retired")
        .unwrap();
    assert_eq!(retained.role, ManagedInputRole::Retained);
    assert_eq!(retained.protection, ManagedInputProtection::Secret);
    let mut changed = Cursor::new(b"new".to_vec());
    let next = p
        .set_input(
            view.id,
            view.state_version,
            &mut ManagedInputWrite {
                input_id: InputIdentity::parse("required").unwrap(),
                byte_len: 3,
                reader: &mut changed,
            },
        )
        .unwrap();
    p.delete_input(view.id, next, &retained.input).unwrap();
    assert_eq!(p.admitted_capture_registry(run).unwrap(), registry);
    let mut observed = Vec::new();
    p.stream_admitted_payload(run, &retained.input, &mut observed)
        .unwrap();
    assert_eq!(observed, secret);
    let mut empty = Vec::new();
    p.stream_admitted_payload(run, &InputIdentity::parse("required").unwrap(), &mut empty)
        .unwrap();
    assert!(empty.is_empty());
    let after_management = token(&p, view.id);
    p.finish_run_owned(
        &owner(),
        run,
        &plain_finish(RunOutcome::Cancelled),
        &[],
        &mut [],
    )
    .unwrap();
    assert_eq!(token(&p, view.id), after_management);
    assert!(p.admitted_capture_registry(run).is_err());
    assert_eq!(p.database.lock().unwrap().query_row("SELECT count(*) FROM managed_input_payloads WHERE instance_id=?1 AND payload_id=?2",
        params![view.id.as_bytes().as_slice(),retained.payload.unwrap().as_bytes().as_slice()], |row| row.get::<_,i64>(0)).unwrap(), 0);
}

// Test-ID: PR-TEST-0227
// Verifies: PR-REQ-0289, PR-REQ-0045, PR-REQ-0191
#[test]
fn capture_is_admitted_with_authored_access_against_action_capture_and_restore() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    for (i, incoming) in [Operation::ObserveCapture, Operation::MutateCapture]
        .into_iter()
        .enumerate()
    {
        for (j, competitor) in OPERATIONS.into_iter().enumerate() {
            let revision = managed_revision(&p, incoming);
            let pinned = managed_revision(&p, competitor);
            let view = empty_instance(&p, &revision, &format!("capture-admit-{i}-{j}"));
            // Historical competitor pin may belong to a different Revision.
            let other = fixture_run(&p, &view, &pinned, competitor, true);
            let run = capture_run(&p, &view, &owner());
            let result = capture_admit(&p, &view, run, &owner()).unwrap();
            let blocked = incoming.access() == OperationAccessV1::Mutate
                && competitor.access() == OperationAccessV1::Mutate;
            assert_eq!(
                result,
                if blocked {
                    Err(AdmissionRefusal::MutationConflict(other))
                } else {
                    Ok(())
                }
            );
            if blocked {
                assert_eq!(
                    managed_finished(&p, run).boundary,
                    ActionRunBoundary::Accepted
                );
            } else {
                assert!(matches!(
                    p.load_managed_run(run).unwrap().unwrap().state,
                    RunState::Running(RunExecutionView {
                        boundary: ActionRunBoundary::Admitted,
                        ..
                    })
                ));
            }
            assert_eq!(token(&p, view.id), view.state_version);
        }
    }
}

// Test-ID: PR-TEST-0229
// Verifies: PR-REQ-0289, PR-REQ-0298
#[test]
fn malformed_snapshot_references_block_loading_risk_and_reconciliation_without_repair() {
    for fault in [
        "missing-restore-pin",
        "foreign-snapshot",
        "wrong-state-token",
        "capture-restore-pin",
        "missing-execution",
    ] {
        let (_tmp, path) = root();
        let p = PactrunPersistence::open(&path).unwrap();
        let revision = managed_revision(&p, Operation::Restore);
        let view = empty_instance(&p, &revision, "corrupt-snapshot-reference");
        let run = fixture_run(&p, &view, &revision, Operation::Restore, true);
        assert!(p.load_managed_run(run).is_ok());
        {
            let db = p.database.lock().unwrap();
            match fault {
                "missing-execution" => {
                    db.execute(
                        "DELETE FROM run_executions WHERE run_id=?1",
                        [run.as_bytes().as_slice()],
                    )
                    .unwrap();
                }
                "missing-restore-pin" => {
                    db.execute(
                        "DELETE FROM run_restore_admissions WHERE run_id=?1",
                        [run.as_bytes().as_slice()],
                    )
                    .unwrap();
                }
                "foreign-snapshot" => {
                    db.execute(
                        "UPDATE run_restore_invocations SET snapshot_id=?2 WHERE run_id=?1",
                        params![
                            run.as_bytes().as_slice(),
                            SnapshotId::generate().unwrap().as_bytes().as_slice()
                        ],
                    )
                    .unwrap();
                }
                "wrong-state-token" => {
                    db.execute("UPDATE run_restore_admissions SET admitted_state_version=?2 WHERE run_id=?1",params![run.as_bytes().as_slice(),InstanceStateVersion::generate().unwrap().as_bytes().as_slice()]).unwrap();
                }
                "capture-restore-pin" => {
                    db.execute(
                        "DELETE FROM run_restore_invocations WHERE run_id=?1",
                        [run.as_bytes().as_slice()],
                    )
                    .unwrap();
                    db.execute(
                        "UPDATE run_operation_kinds SET operation_kind=1 WHERE run_id=?1",
                        [run.as_bytes().as_slice()],
                    )
                    .unwrap();
                    db.execute(
                        "INSERT INTO run_capture_invocations VALUES (?1,?2,?3)",
                        params![
                            run.as_bytes().as_slice(),
                            revision.package_id.as_bytes().as_slice(),
                            revision.content_digest.as_bytes().as_slice()
                        ],
                    )
                    .unwrap();
                }
                _ => unreachable!(),
            }
        }
        assert!(
            matches!(
                p.load_managed_run(run),
                Err(PersistenceError::CorruptRun(_))
            ),
            "{fault}"
        );
        assert!(
            matches!(
                p.open_recovery_risk(run),
                Err(PersistenceError::CorruptRun(_))
            ),
            "{fault}"
        );
        assert!(
            matches!(
                p.reconcile_managed_run(run, &owner()),
                Err(PersistenceError::CorruptRun(_))
            ),
            "{fault}"
        );
        assert!(
            outcome_row(&p.database.lock().unwrap(), run)
                .unwrap()
                .is_none()
        );
        assert_eq!(p.recovery_consequence_version(view.id).unwrap(), 0);
    }
}

// Supporting malformed-terminal coverage for PR-TEST-0229.
#[test]
fn snapshot_success_cannot_be_fabricated_at_the_accepted_boundary() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision(&p, Operation::ObserveCapture);
    let view = empty_instance(&p, &revision, "malformed-success");
    let run = capture_run(&p, &view, &owner());
    {
        let db = p.database.lock().unwrap();
        db.execute(
            "DELETE FROM run_executions WHERE run_id=?1",
            [run.as_bytes().as_slice()],
        )
        .unwrap();
        db.execute("INSERT INTO run_outcomes(run_id,outcome_rank,admitted_rank,risk_state,finished_at_unix_ms) VALUES (?1,0,0,0,0)", [run.as_bytes().as_slice()]).unwrap();
        db.execute(
            "INSERT INTO run_capture_results VALUES (?1,?2)",
            params![
                run.as_bytes().as_slice(),
                SnapshotId::generate().unwrap().as_bytes().as_slice()
            ],
        )
        .unwrap();
    }
    assert!(matches!(
        p.load_managed_run(run),
        Err(PersistenceError::CorruptRun(_))
    ));
    assert!(matches!(
        p.reconcile_managed_run(run, &owner()),
        Err(PersistenceError::CorruptRun(_))
    ));
    assert_eq!(count(&p, "SELECT count(*) FROM run_outcomes"), 1);
    assert_eq!(p.recovery_consequence_version(view.id).unwrap(), 0);
}

// Supporting authoritative-declaration/registry coverage for PR-TEST-0229.
#[test]
fn reconciliation_cannot_hide_an_undeclared_capture_or_missing_required_pin() {
    for missing_declaration in [false, true] {
        let (_tmp, path) = root();
        let p = PactrunPersistence::open(&path).unwrap();
        let run = if missing_declaration {
            let revision = revision(&p);
            let view = instance(&p, &revision, "undeclared-capture");
            let run = admitted(&p, &view);
            let db = p.database.lock().unwrap();
            db.execute(
                "DELETE FROM run_action_invocations WHERE run_id=?1",
                [run.as_bytes().as_slice()],
            )
            .unwrap();
            db.execute(
                "UPDATE run_operation_kinds SET operation_kind=1 WHERE run_id=?1",
                [run.as_bytes().as_slice()],
            )
            .unwrap();
            db.execute(
                "INSERT INTO run_capture_invocations VALUES (?1,?2,?3)",
                params![
                    run.as_bytes().as_slice(),
                    revision.package_id.as_bytes().as_slice(),
                    revision.content_digest.as_bytes().as_slice()
                ],
            )
            .unwrap();
            run
        } else {
            let revision = managed_revision_with_inputs(
                &p,
                Operation::ObserveCapture,
                vec![input("required", true, InputProtectionV1::Normal)],
            );
            let mut reader = Cursor::new(Vec::<u8>::new());
            let view = p
                .create_instance(
                    InstanceName::parse("missing-pin").unwrap(),
                    revision,
                    &mut [ManagedInputWrite {
                        input_id: InputIdentity::parse("required").unwrap(),
                        byte_len: 0,
                        reader: &mut reader,
                    }],
                )
                .unwrap();
            let run = capture_run(&p, &view, &owner());
            capture_admit(&p, &view, run, &owner()).unwrap().unwrap();
            p.database
                .lock()
                .unwrap()
                .execute(
                    "DELETE FROM run_payload_pins WHERE run_id=?1",
                    [run.as_bytes().as_slice()],
                )
                .unwrap();
            assert!(
                p.load_instance_by_id(view.id)
                    .unwrap()
                    .unwrap()
                    .required_inputs_satisfied
            );
            run
        };
        assert!(matches!(
            p.load_managed_run(run),
            Err(PersistenceError::CorruptRun(_))
        ));
        assert!(matches!(
            p.reconcile_managed_run(run, &owner()),
            Err(PersistenceError::CorruptRun(_))
        ));
        assert_eq!(count(&p, "SELECT count(*) FROM run_outcomes"), 0);
        assert_eq!(count(&p, "SELECT count(*) FROM run_executions"), 1);
    }
}

const ADMISSION_WORKER: &str =
    "persistence::sqlite_runs::tests::managed_access::lifecycle::managed_admission_worker";

fn wait_for_file(path: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
    while !path.is_file() {
        assert!(
            std::time::Instant::now() < deadline,
            "worker did not signal {}",
            path.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

struct ManagedWorker(std::process::Child);
impl Drop for ManagedWorker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn managed_admission_worker() {
    let Some(path) = std::env::var_os("PACTRUN_S4_ADMISSION_ROOT") else {
        return;
    };
    let path = PathBuf::from(path);
    let label = std::env::var("PACTRUN_S4_ADMISSION_KIND").unwrap();
    let p = PactrunPersistence::open(&path).unwrap();
    let owner = p.staging_session().unwrap().owner();
    let id = p
        .resolve_instance_name(&InstanceName::parse("process-race").unwrap())
        .unwrap()
        .unwrap();
    let view = p.load_instance_by_id(id).unwrap().unwrap();
    let operation = if label == "capture" {
        ManagedRunIdentity::Capture {
            revision: view.active_revision.clone(),
        }
    } else {
        ManagedRunIdentity::Action(action(&view.active_revision))
    };
    let run = p
        .create_accepted_managed_run(
            RunId::generate().unwrap(),
            view.id,
            view.state_version,
            &operation,
            &owner,
            &crate::persistence::UnconditionalAcceptance,
        )
        .unwrap();
    fs::write(path.join(format!("{label}.accepted")), run.to_string()).unwrap();
    wait_for_file(&path.join("go"));
    let result = capture_admit(&p, &view, run, &owner).unwrap();
    if result.is_ok() {
        p.open_recovery_risk(run).unwrap();
    }
    fs::write(
        path.join(format!("{label}.result")),
        if result.is_ok() {
            "admitted"
        } else {
            "refused"
        },
    )
    .unwrap();
    wait_for_file(&path.join("release"));
}

// Test-ID: PR-TEST-0228
// Verifies: PR-REQ-0289, PR-REQ-0298, PR-REQ-0045
#[test]
fn capture_and_action_race_across_real_owners_and_explicit_reconciliation_never_replays() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision(&p, Operation::MutateCapture);
    let view = empty_instance(&p, &revision, "process-race");
    let app = crate::application::PactrunApplication::open(&path).unwrap();
    let mut workers = Vec::new();
    for label in ["capture", "action"] {
        workers.push(ManagedWorker(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", ADMISSION_WORKER, "--nocapture"])
                .env("PACTRUN_S4_ADMISSION_ROOT", &path)
                .env("PACTRUN_S4_ADMISSION_KIND", label)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        ));
    }
    for label in ["capture", "action"] {
        wait_for_file(&path.join(format!("{label}.accepted")));
    }
    fs::write(path.join("go"), b"go").unwrap();
    for label in ["capture", "action"] {
        wait_for_file(&path.join(format!("{label}.result")));
    }
    let results = ["capture", "action"]
        .map(|label| fs::read_to_string(path.join(format!("{label}.result"))).unwrap());
    assert_eq!(
        results
            .iter()
            .filter(|result| result.as_str() == "admitted")
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| result.as_str() == "refused")
            .count(),
        1
    );
    let runs = ["capture", "action"].map(|label| {
        fs::read_to_string(path.join(format!("{label}.accepted")))
            .unwrap()
            .parse::<RunId>()
            .unwrap()
    });
    let winner = results
        .iter()
        .position(|result| result == "admitted")
        .unwrap();
    let loser = 1 - winner;
    assert_eq!(
        managed_finished(&p, runs[loser]).outcome,
        RunOutcome::Failed
    );
    assert!(matches!(
        managed_finished(&p, runs[loser]).primary_failure,
        Some(RunPrimaryFailure {
            step: RunFailedStep::Admission,
            ..
        })
    ));
    // A real live writer's lease prevents reconciliation despite its open risk.
    assert!(app.reconcile_lost_managed_owners().unwrap().is_empty());
    assert_eq!(p.recovery_consequence_version(view.id).unwrap(), 0);
    workers[winner].0.kill().unwrap();
    workers[winner].0.wait().unwrap();
    assert_eq!(
        app.reconcile_lost_managed_owners().unwrap(),
        vec![runs[winner]]
    );
    assert_eq!(
        managed_finished(&p, runs[winner]).outcome,
        RunOutcome::Interrupted
    );
    assert_eq!(p.recovery_consequence_version(view.id).unwrap(), 1);
    assert!(app.reconcile_lost_managed_owners().unwrap().is_empty());
    assert_eq!(p.recovery_consequence_version(view.id).unwrap(), 1);
    assert_eq!(count(&p, "SELECT count(*) FROM run_capture_results"), 0);
    assert_eq!(count(&p, "SELECT count(*) FROM run_executions"), 0);
    fs::write(path.join("release"), b"release").unwrap();
    assert!(workers[loser].0.wait().unwrap().success());
}

// Test-ID: PR-TEST-0230
// Verifies: PR-REQ-0289, PR-REQ-0298
#[test]
fn snapshot_unknown_owner_is_not_reconciled_and_accepted_restore_needs_no_target_readiness() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let revision = managed_revision_with_inputs(
        &p,
        Operation::ObserveCapture,
        vec![input("required", true, InputProtectionV1::Normal)],
    );
    let view = empty_instance(&p, &revision, "unknown-owner");
    assert!(!view.required_inputs_satisfied);
    let app = crate::application::PactrunApplication::open(&path).unwrap();
    let unknown =
        ExecutionOwnerSession::parse(format!("session-{}", RunId::generate().unwrap())).unwrap();
    let directory = path.join("staging").join(unknown.as_str());
    fs::create_dir(&directory).unwrap();
    let mut runs = Vec::new();
    for operation in [
        ManagedRunIdentity::Capture {
            revision: revision.clone(),
        },
        ManagedRunIdentity::Restore {
            revision: revision.clone(),
            snapshot: SnapshotId::generate().unwrap(),
        },
    ] {
        runs.push(
            p.create_accepted_managed_run(
                RunId::generate().unwrap(),
                view.id,
                view.state_version,
                &operation,
                &unknown,
                &crate::persistence::UnconditionalAcceptance,
            )
            .unwrap(),
        );
    }
    assert!(app.reconcile_lost_managed_owners().unwrap().is_empty());
    for run in &runs {
        assert!(matches!(
            p.load_managed_run(*run).unwrap().unwrap().state,
            RunState::Running(_)
        ));
    }
    // Confirmed directory loss is the evidence; age and PID are not consulted.
    fs::remove_dir(&directory).unwrap();
    let mut reconciled = app.reconcile_lost_managed_owners().unwrap();
    reconciled.sort();
    runs.sort();
    assert_eq!(reconciled, runs);
    for run in runs {
        let outcome = managed_finished(&p, run);
        assert_eq!(outcome.outcome, RunOutcome::Interrupted);
        assert_eq!(outcome.boundary, ActionRunBoundary::Accepted);
        assert_eq!(outcome.terminal_risk, RecoveryRiskState::Clear);
    }
    assert_eq!(p.recovery_consequence_version(view.id).unwrap(), 0);
    assert!(p.load_instance_recovery_guard(view.id).unwrap().is_none());
    assert_eq!(token(&p, view.id), view.state_version);
}
