use super::*;
use crate::persistence::{ManagedInputWrite, UnconditionalAcceptance};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn input(name: &str, required: bool, secret: bool) -> InputDeclarationV1 {
    InputDeclarationV1 {
        id: InputIdentity::parse(name).unwrap(),
        required,
        protection: if secret {
            InputProtectionV1::Secret
        } else {
            InputProtectionV1::Normal
        },
    }
}
fn root() -> (tempfile::TempDir, PathBuf) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/migration-run-tests");
    fs::create_dir_all(&parent).unwrap();
    let tmp = tempfile::tempdir_in(parent).unwrap();
    let root = tmp.path().join("store");
    for d in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(d)).unwrap();
    }
    (tmp, root)
}
fn fixture(p: &PactrunPersistence) -> (InstanceView, Vec<RevisionIdentity>, RuntimeFileV1) {
    let bytes = b"unlaunched observer fixture";
    let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
    let publication = p
        .put_runtime_content(&digest, &mut Cursor::new(bytes))
        .unwrap();
    let file = RuntimeFileV1 {
        id: ContentId::parse("observer").unwrap(),
        path: RuntimePath::parse("observer").unwrap(),
        kind: RuntimeFileKindV1::RegularFile,
        blob_digest: digest,
        executable: true,
    };
    let hook = HookV1 {
        protocol_version: crate::domain::FormatVersion::BASELINE,
        launch: HookLaunchV1::Direct {
            executable: file.id.clone(),
        },
        args: vec![],
        io: IOContractV1 {
            terminal: TerminalContractV1::None,
        },
    };
    let mut path: Vec<RevisionIdentity> = Vec::new();
    for index in 0..3 {
        let inputs = match index {
            0 => vec![input("old", true, true), input("inert", false, true)],
            1 => vec![input("middle", true, false), input("missing", true, false)],
            _ => vec![input("new", true, false)],
        };
        let migrations = if index == 0 {
            vec![]
        } else {
            let source = InputBindingRefV1 {
                role: InputBindingRoleV1::Active,
                input_id: InputIdentity::parse(if index == 1 { "old" } else { "middle" }).unwrap(),
            };
            let target = InputIdentity::parse(if index == 1 { "middle" } else { "new" }).unwrap();
            vec![MigrationV1 {
                source_revision_digest: Sha256Digest::from_bytes(
                    *path[index - 1].content_digest.as_bytes(),
                ),
                transitions: vec![if index == 1 {
                    MigrationTransitionV1::Carry {
                        source: source.clone(),
                        target_input_id: target,
                    }
                } else {
                    MigrationTransitionV1::Declassify {
                        source: source.clone(),
                        target_input_id: target,
                    }
                }],
                requires_source: vec![source],
                requires_target: vec![],
                produces_target: vec![],
                hook: None,
            }]
        };
        let core = project_revision_declarations(RevisionDeclarationInput {
            inputs,
            actions: vec![ActionV1 {
                id: ActionIdentity::parse("inspect").unwrap(),
                access: OperationAccessV1::Observe,
                parameters: vec![],
                hook: hook.clone(),
                outputs: vec![],
            }],
            snapshot: None,
            migrations,
            cleanup: None,
        })
        .unwrap();
        let content = validate_declaration_content(
            core,
            project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 {
                files: vec![file.clone()],
            })
            .unwrap(),
        )
        .unwrap();
        path.push(
            p.persist_revision(
                PackageId::from_bytes([7; 16]),
                &content,
                std::slice::from_ref(&publication),
            )
            .unwrap(),
        );
    }
    let mut secret = Cursor::new(b"secret bytes");
    let mut inert = Cursor::new(b"kept");
    let instance = p
        .create_instance(
            InstanceName::parse("migration").unwrap(),
            path[0].clone(),
            &mut [
                ManagedInputWrite {
                    input_id: InputIdentity::parse("old").unwrap(),
                    byte_len: 12,
                    reader: &mut secret,
                },
                ManagedInputWrite {
                    input_id: InputIdentity::parse("inert").unwrap(),
                    byte_len: 4,
                    reader: &mut inert,
                },
            ],
        )
        .unwrap();
    (instance, path, file)
}
fn plan(
    p: &PactrunPersistence,
    instance: InstanceId,
    path: &[RevisionIdentity],
) -> MigrationExecutionPlan {
    let observed = p.observe_migration_compilation(instance).unwrap();
    let intent = TransitionRevision {
        instance,
        expected_state_version: observed.state_version,
        source: path[0].clone(),
        target: path.last().unwrap().clone(),
        path: MigrationPathSelection::Exact(path.to_vec()),
        operator_inputs: vec![],
        authorize_declassification: true,
    };
    let bindings =
        build_migration_binding_plan(&intent, &observed.revisions, &observed.bindings).unwrap();
    let compiled = bindings
        .edges()
        .iter()
        .map(|e| MigrationCompiledEdge {
            service: None,
            bindings: e.clone(),
            runtime: observed
                .revisions
                .iter()
                .find(|r| &r.identity == e.target())
                .unwrap()
                .content
                .runtime_content
                .files()
                .to_vec(),
            launch: None,
        })
        .collect();
    MigrationExecutionPlan::new(bindings, compiled).unwrap()
}
fn accepted(p: &PactrunPersistence, plan: &MigrationExecutionPlan, tag: u8) -> RunId {
    let run = RunId::from_bytes([tag; 16]);
    p.create_accepted_managed_run(
        run,
        plan.instance(),
        plan.expected_state_version(),
        &ManagedRunIdentity::Migration(plan.invocation()),
        &p.staging_session().unwrap().owner(),
        &UnconditionalAcceptance,
    )
    .unwrap();
    run
}
fn observe(p: &PactrunPersistence, instance: InstanceId, file: &RuntimeFileV1, tag: u8) -> RunId {
    let state = p
        .observe_instance_compilation_state(instance)
        .unwrap()
        .unwrap();
    let run = RunId::from_bytes([tag; 16]);
    p.create_accepted_run(
        run,
        instance,
        state.state_version,
        &ActionRunIdentity {
            revision: state.active_revision,
            action: ActionIdentity::parse("inspect").unwrap(),
        },
        &p.staging_session().unwrap().owner(),
        &UnconditionalAcceptance,
    )
    .unwrap();
    p.admit_run(
        run,
        &AdmissionFacts {
            service: None,
            expected_state_version: state.state_version,
            active_bindings: &state.active_bindings,
            runtime_content: std::slice::from_ref(file),
            launch: &CompiledHookLaunch::Direct {
                executable: file.clone(),
            },
        },
        &|_| Ok(()),
        true,
    )
    .unwrap()
    .unwrap();
    run
}
fn assert_released(p: &PactrunPersistence, run: RunId) {
    let db = p.database.lock().unwrap();
    for table in [
        "run_executions",
        "run_revision_pins",
        "run_migration_revision_pins",
        "run_migration_payload_pins",
        "run_migration_checkpoint_bindings",
    ] {
        assert_eq!(count(&db, table, run).unwrap(), 0, "{table}");
    }
}

// Test-ID: PR-TEST-0344
// Verifies: PR-REQ-0078
#[test]
fn reopening_preserves_open_migration_and_checkpoint_without_reconciliation() {
    let (_tmp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (instance, path, _) = fixture(&p);
    let plan = plan(&p, instance.id, &path);
    let owner = p.staging_session().unwrap().owner();
    let run = accepted(&p, &plan, 95);
    p.admit_declarative_migration(run, &owner, &plan, false)
        .unwrap()
        .unwrap();
    p.publish_declarative_migration_edge(run, &owner, 0)
        .unwrap();
    p.open_recovery_risk(run).unwrap();
    let before = p.load_managed_run(run).unwrap().unwrap();
    let before_instance = p.load_instance_by_id(instance.id).unwrap().unwrap();
    p.abandon_execution_owner();
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    assert_eq!(p.load_managed_run(run).unwrap().unwrap(), before);
    assert_eq!(
        p.load_instance_by_id(instance.id)
            .unwrap()
            .unwrap()
            .state_version,
        before_instance.state_version
    );
    assert!(
        p.load_instance_recovery_guard(instance.id)
            .unwrap()
            .is_none()
    );
    let app = crate::application::PactrunApplication::open(&root).unwrap();
    assert_eq!(app.reconcile_lost_managed_owners().unwrap(), vec![run]);
    assert_eq!(
        p.load_instance_by_id(instance.id)
            .unwrap()
            .unwrap()
            .active_revision,
        path[1]
    );
    assert!(
        p.load_instance_recovery_guard(instance.id)
            .unwrap()
            .is_some()
    );
    assert_released(&p, run);
}

// Test-ID: PR-TEST-0301
// Verifies: PR-REQ-0025, PR-REQ-0026, PR-REQ-0044, PR-REQ-0107, PR-REQ-0155, PR-REQ-0158, PR-REQ-0165, PR-REQ-0313, PR-REQ-0347
#[test]
fn declarative_chain_commits_incomplete_middle_then_atomic_success_and_copies_protection() {
    for external in [false, true] {
        let (_tmp, root) = root();
        let p = PactrunPersistence::open(&root).unwrap();
        let (instance, path, file) = fixture(&p);
        if external {
            let digest = Sha256Digest::from_bytes(Sha256::digest(b"secret bytes").into());
            p.put_runtime_content(&digest, &mut Cursor::new(b"secret bytes"))
                .unwrap();
            let mut db = p.database.lock().unwrap();
            let payload = binding_payload(&db, instance.id, &InputIdentity::parse("old").unwrap())
                .unwrap()
                .unwrap()
                .0;
            let tx = db.transaction().unwrap();
            tx.execute("UPDATE managed_input_payloads SET content_digest=?3 WHERE instance_id=?1 AND payload_id=?2", params![instance.id.as_bytes().as_slice(),payload.as_bytes().as_slice(),digest.to_bytes().as_slice()]).unwrap();
            tx.execute(
                "DELETE FROM managed_input_payload_chunks WHERE instance_id=?1 AND payload_id=?2",
                params![
                    instance.id.as_bytes().as_slice(),
                    payload.as_bytes().as_slice()
                ],
            )
            .unwrap();
            tx.commit().unwrap();
        }

        let plan = plan(&p, instance.id, &path);
        let reader = observe(&p, instance.id, &file, 30);
        let original = binding_payload(
            &p.database.lock().unwrap(),
            instance.id,
            &InputIdentity::parse("old").unwrap(),
        )
        .unwrap()
        .unwrap()
        .0;
        let run = accepted(&p, &plan, 31);
        let owner = p.staging_session().unwrap().owner();
        p.admit_declarative_migration(run, &owner, &plan, false)
            .unwrap()
            .unwrap();
        assert!(
            p.finish_run_owned(&owner, run, &failed(RunOutcome::Succeeded), &[], &mut [])
                .is_err()
        );
        let first = p
            .publish_declarative_migration_edge(run, &owner, 0)
            .unwrap();
        assert_eq!(first.committed_edges, 1);
        let middle = p.load_instance_by_id(instance.id).unwrap().unwrap();
        assert_eq!(middle.active_revision, path[1]);
        assert!(!middle.required_inputs_satisfied);
        assert!(matches!(
            p.load_managed_run(run).unwrap().unwrap().state,
            RunState::Running(_)
        ));
        assert_eq!(
            p.publish_declarative_migration_edge(run, &owner, 0)
                .unwrap(),
            first
        );
        let last = p
            .publish_declarative_migration_edge(run, &owner, 1)
            .unwrap();
        assert_eq!(last.committed_edges, 2);
        assert_ne!(last.boundary_state_version, first.boundary_state_version);
        let current = p.load_instance_by_id(instance.id).unwrap().unwrap();
        assert_eq!(current.active_revision, path[2]);
        assert!(current.required_inputs_satisfied);
        assert!(
            matches!(p.load_managed_run(run).unwrap().unwrap().state,RunState::Finished(o) if o.outcome==RunOutcome::Succeeded)
        );
        let new = binding_payload(
            &p.database.lock().unwrap(),
            instance.id,
            &InputIdentity::parse("new").unwrap(),
        )
        .unwrap()
        .unwrap();
        assert_ne!(new.0, original);
        assert_eq!(new.1, ManagedInputProtection::Normal);
        let old_rank:i64=p.database.lock().unwrap().query_row("SELECT protection_rank FROM managed_input_payloads WHERE instance_id=?1 AND payload_id=?2",params![instance.id.as_bytes().as_slice(),original.as_bytes().as_slice()],|r|r.get(0)).unwrap();
        assert_eq!(old_rank, 1);
        let mut bytes = Vec::new();
        p.export_input(
            instance.id,
            &InputIdentity::parse("new").unwrap(),
            false,
            &mut bytes,
        )
        .unwrap();
        assert_eq!(bytes, b"secret bytes");
        assert!(
            current
                .bindings
                .iter()
                .any(|b| b.input_id.as_str() == "inert"
                    && b.role == ManagedInputRole::Retained
                    && b.protection == ManagedInputProtection::Secret)
        );
        assert_released(&p, run);
        p.finish_run(reader, &failed(RunOutcome::Cancelled), &mut [])
            .unwrap();
    }
}

// Test-ID: PR-TEST-0302
// Verifies: PR-REQ-0007, PR-REQ-0165, PR-REQ-0313
#[test]
fn migration_admission_revalidates_source_serializes_mutators_and_does_not_launch() {
    let (_tmp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (instance, path, _) = fixture(&p);
    let plan = plan(&p, instance.id, &path);
    let owner = p.staging_session().unwrap().owner();
    let run = accepted(&p, &plan, 40);
    p.admit_declarative_migration(run, &owner, &plan, false)
        .unwrap()
        .unwrap();
    assert!(
        p.finish_run(run, &failed(RunOutcome::Cancelled), &mut [])
            .is_err()
    );
    let foreign = PactrunPersistence::open(&root).unwrap();
    assert!(
        foreign
            .finish_run_owned(&owner, run, &failed(RunOutcome::Cancelled), &[], &mut [])
            .is_err()
    );
    drop(foreign);
    let other = accepted(&p, &plan, 41);
    assert!(
        matches!(p.admit_declarative_migration(other,&owner,&plan,false).unwrap(),Err(AdmissionRefusal::MutationConflict(id)) if id==run)
    );
    let mut replacement = Cursor::new(b"replacement");
    assert!(
        p.set_input(
            instance.id,
            instance.state_version,
            &mut ManagedInputWrite {
                input_id: InputIdentity::parse("old").unwrap(),
                byte_len: 11,
                reader: &mut replacement
            }
        )
        .is_err()
    );
    p.finish_run_owned(&owner, run, &failed(RunOutcome::Cancelled), &[], &mut [])
        .unwrap();
    let stale = accepted(&p, &plan, 42);
    {
        let mut db = p.database.lock().unwrap();
        let tx = db.transaction().unwrap();
        super::super::sqlite_instances::update_state_version(
            &tx,
            instance.id,
            fresh_state_version().unwrap(),
        )
        .unwrap();
        tx.commit().unwrap();
    }
    assert!(matches!(
        p.admit_declarative_migration(stale, &owner, &plan, false)
            .unwrap(),
        Err(AdmissionRefusal::PlanInvalidated(_))
    ));
    assert_released(&p, stale);
    assert_released(&p, other);
    let reopened = PactrunPersistence::open(&root).unwrap();
    for refused in [stale, other] {
        let RunState::Finished(outcome) =
            reopened.load_managed_run(refused).unwrap().unwrap().state
        else {
            panic!("post-acceptance refusal was not durably terminal");
        };
        assert_eq!(outcome.outcome, RunOutcome::Failed);
        assert_eq!(outcome.boundary, ActionRunBoundary::Accepted);
        assert!(outcome.hook_completion.is_none());
    }
}

// Test-ID: PR-TEST-0303
// Verifies: PR-REQ-0106, PR-REQ-0313
#[test]
fn later_cancellation_or_observe_consequence_preserves_the_committed_middle() {
    for cancellation in [true, false] {
        let (_tmp, root) = root();
        let p = PactrunPersistence::open(&root).unwrap();
        let (instance, path, file) = fixture(&p);
        let plan = plan(&p, instance.id, &path);
        let owner = p.staging_session().unwrap().owner();
        let observer = observe(&p, instance.id, &file, 50);
        let run = accepted(&p, &plan, 51);
        p.admit_declarative_migration(run, &owner, &plan, false)
            .unwrap()
            .unwrap();
        p.publish_declarative_migration_edge(run, &owner, 0)
            .unwrap();
        if cancellation {
            p.finish_run_owned(&owner, run, &failed(RunOutcome::Cancelled), &[], &mut [])
                .unwrap();
        } else {
            p.open_recovery_risk(observer).unwrap();
            p.finish_run(observer, &failed(RunOutcome::Failed), &mut [])
                .unwrap();
            assert!(
                p.publish_declarative_migration_edge(run, &owner, 1)
                    .is_err()
            );
        }
        assert_eq!(
            p.load_instance_by_id(instance.id)
                .unwrap()
                .unwrap()
                .active_revision,
            path[1]
        );
        assert_eq!(
            p.managed_run_inspection(run)
                .unwrap()
                .unwrap()
                .migration_progress
                .unwrap()
                .committed_edges,
            1
        );
        assert_released(&p, run);
    }
}

const WORKER: &str = "persistence::sqlite_migration_runs::tests::migration_worker";

// Test-ID: PR-TEST-0305
// Verifies: PR-REQ-0044, PR-REQ-0313, PR-REQ-0314
#[test]
fn owner_continuation_retries_lost_commit_ack_without_repeating_an_edge() {
    for index in [0, 1] {
        let (_tmp, root) = root();
        let p = PactrunPersistence::open(&root).unwrap();
        let (instance, path, _) = fixture(&p);
        let plan = plan(&p, instance.id, &path);
        drop(p);
        let app = crate::application::PactrunApplication::open(&root).unwrap();
        let run = app
            .accept_declarative_migration(plan, false, crate::hook::ActionCancellation::default())
            .unwrap();
        assert!(!app.advance_owner_continuation(run).unwrap());
        if index == 1 {
            assert!(!app.advance_owner_continuation(run).unwrap());
        }
        fail_next_edge_ack_for_test();
        assert!(app.advance_owner_continuation(run).is_err());
        assert_eq!(
            app.managed_run_inspection(run)
                .unwrap()
                .unwrap()
                .migration_progress
                .unwrap()
                .committed_edges,
            index + 1
        );
        while !app.advance_owner_continuation(run).unwrap() {}
        let view = app.managed_run_inspection(run).unwrap().unwrap();
        assert!(matches!(view.run.state,RunState::Finished(o) if o.outcome==RunOutcome::Succeeded));
        assert_eq!(view.migration_progress.unwrap().committed_edges, 2);
        assert_eq!(app.list_managed_runs(instance.id).unwrap().len(), 1);
    }
}

// Test-ID: PR-TEST-0306
// Verifies: PR-REQ-0313, PR-REQ-0070
#[test]
fn open_risk_never_publishes_success_and_terminal_recovery_preserves_current_boundary() {
    let (_tmp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (instance, path, _) = fixture(&p);
    let plan = plan(&p, instance.id, &path);
    let owner = p.staging_session().unwrap().owner();
    let run = accepted(&p, &plan, 70);
    p.admit_declarative_migration(run, &owner, &plan, false)
        .unwrap()
        .unwrap();
    p.publish_declarative_migration_edge(run, &owner, 0)
        .unwrap();
    p.open_recovery_risk(run).unwrap();
    assert!(
        p.publish_declarative_migration_edge(run, &owner, 1)
            .is_err()
    );
    let view = p.managed_run_inspection(run).unwrap().unwrap();
    assert!(view.current_recovery_guard.is_some());
    assert!(
        matches!(view.run.state,RunState::Finished(o) if o.outcome==RunOutcome::Failed && o.terminal_risk==RecoveryRiskState::Open)
    );
    let current = p.load_instance_by_id(instance.id).unwrap().unwrap();
    assert_eq!(current.active_revision, path[1]);
    assert_released(&p, run);
    let mut data = Vec::new();
    p.export_input(
        instance.id,
        &InputIdentity::parse("middle").unwrap(),
        true,
        &mut data,
    )
    .unwrap();
    assert_eq!(data, b"secret bytes");
    let followup = self::plan(&p, instance.id, &path[1..]);
    let blocked = accepted(&p, &followup, 71);
    assert_eq!(
        p.admit_declarative_migration(blocked, &owner, &followup, false)
            .unwrap(),
        Err(AdmissionRefusal::RecoveryGuardActive)
    );
    let override_run = accepted(&p, &followup, 72);
    p.admit_declarative_migration(override_run, &owner, &followup, true)
        .unwrap()
        .unwrap();
    p.publish_declarative_migration_edge(override_run, &owner, 0)
        .unwrap();
    assert!(
        p.managed_run_inspection(override_run)
            .unwrap()
            .unwrap()
            .current_recovery_guard
            .is_some()
    );
    assert_eq!(
        p.load_instance_by_id(instance.id)
            .unwrap()
            .unwrap()
            .active_revision,
        path[2]
    );
}
#[test]
fn migration_worker() {
    let Some(root) = std::env::var_os("PACTRUN_MIGRATION_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let p = PactrunPersistence::open(&root).unwrap();
    if std::env::var("PACTRUN_MIGRATION_TEST_MODE").as_deref() == Ok("competing-management") {
        let id = p
            .resolve_instance_name(&InstanceName::parse("migration").unwrap())
            .unwrap()
            .unwrap();
        let view = p.load_instance_by_id(id).unwrap().unwrap();
        let mut data = Cursor::new(b"blocked");
        assert!(matches!(
            p.set_input(
                id,
                view.state_version,
                &mut ManagedInputWrite {
                    input_id: InputIdentity::parse("old").unwrap(),
                    byte_len: 7,
                    reader: &mut data
                }
            ),
            Err(PersistenceError::MigrationMutationConflict(_))
        ));
        return;
    }
    let (instance, path, _) = fixture(&p);
    let plan = plan(&p, instance.id, &path);
    let run = accepted(&p, &plan, 60);
    let owner = p.staging_session().unwrap().owner();
    p.admit_declarative_migration(run, &owner, &plan, false)
        .unwrap()
        .unwrap();
    p.publish_declarative_migration_edge(run, &owner, 0)
        .unwrap();
    p.publish_declarative_migration_edge(run, &owner, 1)
        .unwrap();
}

// Test-ID: PR-TEST-0304
// Verifies: PR-REQ-0053, PR-REQ-0061, PR-REQ-0062, PR-REQ-0106, PR-REQ-0155, PR-REQ-0165, PR-REQ-0308, PR-REQ-0313
#[test]
fn edge_crashes_reconcile_without_plan_or_replay_and_final_edge_has_no_running_window() {
    for (edge, point, committed, succeeded) in [
        (0, "before_migration_edge_commit", 0, false),
        (0, "after_migration_edge_commit", 1, false),
        (1, "before_migration_edge_commit", 1, false),
        (1, "after_migration_edge_commit", 2, true),
    ] {
        let (_tmp, root) = root();
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", WORKER, "--nocapture"])
            .env("PACTRUN_MIGRATION_TEST_ROOT", &root)
            .env("PACTRUN_OPERATION_TEST_FAULT", point)
            .env("PACTRUN_MIGRATION_FAULT_EDGE", edge.to_string())
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(87));
        let app = crate::application::PactrunApplication::open(&root).unwrap();
        let run = RunId::from_bytes([60; 16]);
        let before = app.managed_run_inspection(run).unwrap().unwrap();
        assert_eq!(
            before.migration_progress.as_ref().unwrap().committed_edges,
            committed
        );
        if succeeded {
            assert!(
                matches!(before.run.state,RunState::Finished(o) if o.outcome==RunOutcome::Succeeded)
            );
            assert!(app.reconcile_lost_action_owners().unwrap().is_empty());
        } else {
            assert!(matches!(before.run.state, RunState::Running(_)));
            assert_eq!(app.reconcile_lost_action_owners().unwrap(), vec![run]);
            assert!(
                matches!(app.managed_run_inspection(run).unwrap().unwrap().run.state,RunState::Finished(o) if o.outcome==RunOutcome::Interrupted)
            );
        }
        assert!(app.reconcile_lost_action_owners().unwrap().is_empty());
    }
}

// Test-ID: PR-TEST-0308
// Verifies: PR-REQ-0313, PR-REQ-0314
#[test]
fn uncertain_acceptance_preserves_the_owner_and_never_creates_a_replacement_run() {
    for committed in [false, true] {
        let (_tmp, root) = root();
        let p = PactrunPersistence::open(&root).unwrap();
        let (instance, path, _) = fixture(&p);
        let plan = plan(&p, instance.id, &path);
        drop(p);
        let app = crate::application::PactrunApplication::open(&root).unwrap();
        crate::hook::uncertain_next_migration_acceptance_for_test(committed);
        let error = app
            .accept_declarative_migration(plan, false, crate::hook::ActionCancellation::default())
            .unwrap_err();
        let crate::application::ApplicationError::Execution(
            crate::executor::ExecutorError::Persistence { run: Some(run), .. },
        ) = error
        else {
            panic!("expected uncertain acceptance")
        };
        while !app.advance_owner_continuation(run).unwrap() {}
        let runs = app.list_managed_runs(instance.id).unwrap();
        assert_eq!(runs.len(), usize::from(committed));
        if committed {
            assert_eq!(runs[0].id, run);
            assert!(
                matches!(&runs[0].state,RunState::Finished(o) if o.outcome==RunOutcome::Succeeded)
            );
        } else {
            assert!(app.managed_run_inspection(run).unwrap().is_none());
        }
    }
}

// Test-ID: PR-TEST-0309
// Verifies: PR-REQ-0313, PR-REQ-0165
#[test]
fn cross_process_management_cannot_bypass_an_admitted_migration() {
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let (_tmp, root) = root();
    let sync = root.join("sync");
    fs::create_dir(&sync).unwrap();
    let mut child = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", WORKER, "--nocapture"])
            .env("PACTRUN_MIGRATION_TEST_ROOT", &root)
            .env("PACTRUN_OPERATION_TEST_SYNC", "after_run_admit_commit")
            .env("PACTRUN_OPERATION_TEST_SYNC_DIR", &sync)
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !sync.join("ready").exists() {
        assert!(
            std::time::Instant::now() < end,
            "admission barrier timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", WORKER, "--nocapture"])
        .env("PACTRUN_MIGRATION_TEST_ROOT", &root)
        .env("PACTRUN_MIGRATION_TEST_MODE", "competing-management")
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let app = crate::application::PactrunApplication::open(&root).unwrap();
    assert!(app.reconcile_lost_action_owners().unwrap().is_empty());
    fs::write(sync.join("release"), []).unwrap();
    assert!(child.0.wait().unwrap().success());
    assert!(
        matches!(app.managed_run_inspection(RunId::from_bytes([60;16])).unwrap().unwrap().run.state,RunState::Finished(o) if o.outcome==RunOutcome::Succeeded)
    );
}
