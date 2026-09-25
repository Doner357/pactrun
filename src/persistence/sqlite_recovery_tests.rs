// M6 cross-operation recovery evidence using real process owners and the
// production acceptance/admission/persistence/reconciliation paths. No Hook
// is launched here; operation-specific Hook/commit tests remain complementary.

use super::*;
use crate::managed_data::{SessionOwnerProbe, probe_session_owner};
use crate::persistence::UnconditionalAcceptance;

const WORKER: &str = "persistence::sqlite_runs::tests::managed_access::lifecycle::substrate::recovery::recovery_worker";
const CASES: [&str; 4] = ["action", "capture", "restore", "migration"];

enum Preparation {
    Action(RuntimeFileV1),
    Snapshot(Box<SnapshotExecutionPlan>),
    Migration(MigrationExecutionPlan),
}

fn prepare(p: &PactrunPersistence, path: &Path, case: &str, admitted: bool) -> RunId {
    let revision = managed_revision_with_inputs(
        p,
        Operation::ObserveCapture,
        vec![input("config", false, InputProtectionV1::Normal)],
    );
    let view = p
        .create_instance(
            InstanceName::parse("recovery").unwrap(),
            revision.clone(),
            &mut [ManagedInputWrite {
                input_id: InputIdentity::parse("config").unwrap(),
                byte_len: 8,
                reader: &mut Cursor::new(b"boundary"),
            }],
        )
        .unwrap();
    let stored = p.load_revision(&revision).unwrap().unwrap();
    let (identity, preparation) = match case {
        "action" => (
            ManagedRunIdentity::Action(action(&revision)),
            Preparation::Action(stored.content.runtime_content.files()[0].clone()),
        ),
        "capture" | "restore" => {
            let operation = if case == "capture" {
                SnapshotOperation::Capture
            } else {
                // A real immutable Snapshot supplies the Restore admission pin.
                let bytes = b"boundary".to_vec();
                let digest = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
                SnapshotOperation::Restore(store(
                    p,
                    &view,
                    vec![SnapshotBinding {
                        input_id: InputIdentity::parse("config").unwrap(),
                        role: SnapshotBindingRole::Active,
                        state: SnapshotBindingState::Bound(digest.clone()),
                        protection: ManagedInputProtection::Normal,
                    }],
                    false,
                    BTreeMap::from([(digest, bytes)]),
                ))
            };
            let app = PactrunApplication::open_read_only(path).unwrap();
            let plan = compile(&app, &view, operation);
            let identity = match operation {
                SnapshotOperation::Capture => ManagedRunIdentity::Capture { revision },
                SnapshotOperation::Restore(snapshot) => {
                    ManagedRunIdentity::Restore { revision, snapshot }
                }
            };
            (identity, Preparation::Snapshot(Box::new(plan)))
        }
        "migration" => {
            let core = &stored.content.core;
            let target_core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
                inputs: core.inputs().to_vec(),
                actions: core.actions().to_vec(),
                snapshot: core.snapshot().cloned(),
                cleanup: None,
                migrations: vec![MigrationV1 {
                    source_revision_digest: Sha256Digest::from_bytes(
                        *revision.content_digest.as_bytes(),
                    ),
                    transitions: vec![],
                    requires_source: vec![],
                    requires_target: vec![],
                    produces_target: vec![],
                    hook: None,
                }],
            })
            .unwrap();
            let content =
                validate_revision_content_v1(target_core, stored.content.runtime_content.clone())
                    .unwrap();
            let publication = p
                .put_runtime_content(
                    &content.runtime_content.files()[0].blob_digest,
                    &mut Cursor::new(TOOL_BYTES),
                )
                .unwrap();
            let target = p
                .persist_revision(revision.package_id, &content, &[publication])
                .unwrap();
            let observed = p.observe_migration_compilation(view.id).unwrap();
            let intent = TransitionRevision {
                instance: view.id,
                expected_state_version: view.state_version,
                source: revision.clone(),
                target: target.clone(),
                path: MigrationPathSelection::Exact(vec![revision, target]),
                operator_inputs: vec![],
                authorize_declassification: false,
            };
            let bindings =
                build_migration_binding_plan(&intent, &observed.revisions, &observed.bindings)
                    .unwrap();
            let edges = bindings
                .edges()
                .iter()
                .map(|edge| MigrationCompiledEdge {
                    service: None,
                    bindings: edge.clone(),
                    runtime: content.runtime_content.files().to_vec(),
                    launch: None,
                })
                .collect();
            let plan = MigrationExecutionPlan::new(bindings, edges).unwrap();
            (
                ManagedRunIdentity::Migration(plan.invocation()),
                Preparation::Migration(plan),
            )
        }
        _ => panic!("unknown recovery case"),
    };
    let owner = p.staging_session().unwrap().owner();
    let run = p
        .create_accepted_managed_run(
            RunId::generate().unwrap(),
            view.id,
            view.state_version,
            &identity,
            &owner,
            &UnconditionalAcceptance,
        )
        .unwrap();
    if admitted {
        match preparation {
            Preparation::Action(file) => {
                p.admit_managed_run(
                    run,
                    &owner,
                    &AdmissionFacts {
                        service: None,
                        expected_state_version: view.state_version,
                        active_bindings: &bindings(p, view.id),
                        runtime_content: std::slice::from_ref(&file),
                        launch: &CompiledHookLaunch::Direct {
                            executable: file.clone(),
                        },
                    },
                    &|_| Ok(()),
                    false,
                )
                .unwrap()
                .unwrap();
            }
            Preparation::Snapshot(plan) => p
                .admit_snapshot_plan(run, &owner, &plan, &|_| Ok(()), false)
                .unwrap()
                .unwrap(),
            Preparation::Migration(plan) => p
                .admit_declarative_migration(run, &owner, &plan, false)
                .unwrap()
                .unwrap(),
        }
    }
    run
}

fn worker(path: &Path, case: &str, mode: &str, fault: Option<&str>) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", WORKER, "--nocapture"])
        .env("PACTRUN_M6_ROOT", path)
        .env("PACTRUN_M6_CASE", case)
        .env("PACTRUN_M6_MODE", mode)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    if let Some(fault) = fault {
        command.env("PACTRUN_M4_FAULT", fault);
    }
    command
}

fn synchronized_reconciler(path: &Path, case: &str, label: &str) -> (ManagedWorker, PathBuf) {
    let barrier = path.join(format!("{label}-sync"));
    fs::create_dir(&barrier).unwrap();
    let child = worker(path, case, label, None)
        .env("PACTRUN_M4_SYNC", "after_recovery_owner_loss_probe")
        .env("PACTRUN_M4_SYNC_DIR", &barrier)
        .spawn()
        .unwrap();
    (ManagedWorker(child), barrier)
}

#[test]
fn recovery_worker() {
    let Some(path) = std::env::var_os("PACTRUN_M6_ROOT") else {
        return;
    };
    let path = PathBuf::from(path);
    let mode = std::env::var("PACTRUN_M6_MODE").unwrap();
    if mode.starts_with("reconcile-") {
        let app = PactrunApplication::open(&path).unwrap();
        let reconciled = app.reconcile_lost_managed_owners().unwrap();
        assert!(app.reconcile_lost_managed_owners().unwrap().is_empty());
        signal(&path.join(mode), &reconciled.len().to_string());
        return;
    }
    let p = PactrunPersistence::open(&path).unwrap();
    if mode == "manual" {
        let view = only_instance(&p);
        PactrunApplication::open(&path)
            .unwrap()
            .resolve_manual_recovery(view.id, view.state_version)
            .unwrap();
        return;
    }
    let case = std::env::var("PACTRUN_M6_CASE").unwrap();
    let run = prepare(&p, &path, &case, mode != "accepted");
    if mode != "accepted" {
        p.open_recovery_risk(run).unwrap();
    }
    if mode == "resolve" {
        p.clear_recovery_risk(run).unwrap();
    }
    if mode == "finish" {
        p.finish_run_owned(
            &p.staging_session().unwrap().owner(),
            run,
            &plain_finish(RunOutcome::Failed),
            &[],
            &mut [],
        )
        .unwrap();
        return;
    }
    signal(&path.join("ready"), &run.to_string());
    wait_for_file(&path.join("release"));
}

fn only_instance(p: &PactrunPersistence) -> InstanceView {
    let id = p
        .resolve_instance_name(&InstanceName::parse("recovery").unwrap())
        .unwrap()
        .unwrap();
    p.load_instance_by_id(id).unwrap().unwrap()
}

fn assert_preserved(p: &PactrunPersistence, before: &InstanceView, run: RunId, open: bool) {
    let after = p.load_instance_by_id(before.id).unwrap().unwrap();
    assert_eq!(after.active_revision, before.active_revision);
    let mut bytes = Vec::new();
    p.export_input(
        before.id,
        &InputIdentity::parse("config").unwrap(),
        false,
        &mut bytes,
    )
    .unwrap();
    assert_eq!(bytes, b"boundary");
    let outcome = managed_finished(p, run);
    assert_eq!(
        outcome.terminal_risk,
        if open {
            RecoveryRiskState::Open
        } else {
            RecoveryRiskState::Clear
        }
    );
    let guard = p.load_instance_recovery_guard(before.id).unwrap();
    assert_eq!(guard.is_some(), open);
    assert_eq!(
        p.recovery_consequence_version(before.id).unwrap(),
        i64::from(open)
    );
    if open {
        assert_eq!(guard.unwrap().run, run);
        assert_ne!(after.state_version, before.state_version);
    } else {
        assert_eq!(after.state_version, before.state_version);
    }
}

// Test-ID: PR-TEST-0325
// Verifies: PR-REQ-0060, PR-REQ-0064, PR-REQ-0066, PR-REQ-0277
#[test]
fn all_operations_require_confirmed_loss_and_competing_processes_reconcile_once() {
    for case in CASES {
        for mode in ["accepted", "open"] {
            let (_tmp, path) = root();
            let mut child = ManagedWorker(worker(&path, case, mode, None).spawn().unwrap());
            wait_for_file(&path.join("ready"));
            let run: RunId = fs::read_to_string(path.join("ready"))
                .unwrap()
                .parse()
                .unwrap();
            let p = PactrunPersistence::open(&path).unwrap();
            let before = only_instance(&p);
            let RunState::Running(execution) = p.load_managed_run(run).unwrap().unwrap().state
            else {
                panic!("running")
            };
            let app = PactrunApplication::open(&path).unwrap();
            assert_eq!(
                probe_session_owner(&path, &execution.owner),
                SessionOwnerProbe::Live
            );
            assert!(app.reconcile_lost_managed_owners().unwrap().is_empty());
            child.0.kill().unwrap();
            child.0.wait().unwrap();
            // An extant owner directory with no readable lease is inconclusive.
            let lease = path
                .join("staging")
                .join(execution.owner.as_str())
                .join(".lease");
            let held = lease.with_extension("unavailable");
            fs::rename(&lease, &held).unwrap();
            assert_eq!(
                probe_session_owner(&path, &execution.owner),
                SessionOwnerProbe::Unknown
            );
            assert!(app.reconcile_lost_managed_owners().unwrap().is_empty());
            assert!(matches!(
                p.load_managed_run(run).unwrap().unwrap().state,
                RunState::Running(_)
            ));
            fs::rename(&held, &lease).unwrap();
            // Establish a held lease after the first probe. The mandatory
            // second probe under the mutation lock must refuse interruption.
            let (mut recheck, barrier) = synchronized_reconciler(&path, case, "reconcile-recheck");
            wait_for_file(&barrier.join("ready"));
            // Opening a new writer may already clean the abandoned session.
            // Recreate only this test-owned path to make the second observation
            // differ from the first; no live owner is displaced.
            fs::create_dir_all(lease.parent().unwrap()).unwrap();
            let lease_handle = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&lease)
                .unwrap();
            lease_handle.lock().unwrap();
            fs::write(barrier.join("release"), b"").unwrap();
            assert!(recheck.0.wait().unwrap().success());
            assert_eq!(
                fs::read_to_string(path.join("reconcile-recheck")).unwrap(),
                "0"
            );
            assert!(matches!(
                p.load_managed_run(run).unwrap().unwrap().state,
                RunState::Running(_)
            ));
            fs::File::unlock(&lease_handle).unwrap();
            drop(lease_handle);
            // Both processes must observe the same Running candidate before
            // either is released to compete for terminal publication.
            let (mut a, barrier_a) = synchronized_reconciler(&path, case, "reconcile-a");
            let (mut b, barrier_b) = synchronized_reconciler(&path, case, "reconcile-b");
            wait_for_file(&barrier_a.join("ready"));
            wait_for_file(&barrier_b.join("ready"));
            fs::write(barrier_a.join("release"), b"").unwrap();
            fs::write(barrier_b.join("release"), b"").unwrap();
            assert!(a.0.wait().unwrap().success());
            assert!(b.0.wait().unwrap().success());
            let winners: usize = ["reconcile-a", "reconcile-b"]
                .iter()
                .map(|label| {
                    fs::read_to_string(path.join(label))
                        .unwrap()
                        .parse::<usize>()
                        .unwrap()
                })
                .sum();
            assert_eq!(winners, 1);
            assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Interrupted);
            assert_preserved(&p, &before, run, mode == "open");
            assert!(app.reconcile_lost_managed_owners().unwrap().is_empty());
        }
    }
}

// Test-ID: PR-TEST-0326
// Verifies: PR-REQ-0055, PR-REQ-0056, PR-REQ-0057, PR-REQ-0061, PR-REQ-0062, PR-REQ-0063
#[test]
fn every_existing_operation_recovers_from_durable_writes_without_the_owner_plan() {
    for case in CASES {
        for (point, mode, admitted, open, terminal) in [
            ("before_run_accept_commit", "open", false, false, false),
            ("after_run_accept_commit", "open", false, false, false),
            ("before_run_admit_commit", "open", false, false, false),
            ("after_run_admit_commit", "open", true, false, false),
            ("before_recovery_risk_commit", "open", true, false, false),
            ("after_recovery_risk_commit", "open", true, true, false),
            (
                "before_recovery_resolution_commit",
                "resolve",
                true,
                true,
                false,
            ),
            (
                "after_recovery_resolution_commit",
                "resolve",
                true,
                false,
                false,
            ),
            ("before_run_finish_commit", "finish", true, true, false),
            ("after_run_finish_commit", "finish", true, true, true),
        ] {
            let (_tmp, path) = root();
            let status = worker(&path, case, mode, Some(point)).status().unwrap();
            assert_eq!(status.code(), Some(87), "{case}/{point}");
            let p = PactrunPersistence::open(&path).unwrap();
            let before = only_instance(&p);
            let runs = p.list_managed_runs(before.id).unwrap();
            if point == "before_run_accept_commit" {
                assert!(runs.is_empty());
                let app = PactrunApplication::open(&path).unwrap();
                assert!(app.reconcile_lost_managed_owners().unwrap().is_empty());
                assert_eq!(only_instance(&p).state_version, before.state_version);
                assert!(p.load_instance_recovery_guard(before.id).unwrap().is_none());
                continue;
            }
            assert_eq!(runs.len(), 1);
            let run = runs[0].id;
            let loaded = p.load_managed_run(run).unwrap().unwrap();
            match loaded.state {
                RunState::Running(e) => {
                    assert!(!terminal);
                    assert_eq!(
                        e.boundary,
                        if admitted {
                            ActionRunBoundary::Admitted
                        } else {
                            ActionRunBoundary::Accepted
                        }
                    );
                    assert_eq!(
                        e.risk_state,
                        if open {
                            RecoveryRiskState::Open
                        } else {
                            RecoveryRiskState::Clear
                        }
                    );
                }
                RunState::Finished(o) => {
                    assert!(terminal);
                    assert_eq!(o.outcome, RunOutcome::Failed);
                }
            }
            // A separate process gets no Plan, operator inputs, or continuation.
            assert!(
                worker(&path, case, "reconcile-a", None)
                    .status()
                    .unwrap()
                    .success()
            );
            assert_eq!(
                managed_finished(&p, run).outcome,
                if terminal {
                    RunOutcome::Failed
                } else {
                    RunOutcome::Interrupted
                }
            );
            if terminal {
                assert_eq!(
                    p.load_instance_recovery_guard(before.id)
                        .unwrap()
                        .unwrap()
                        .run,
                    run
                );
                assert_eq!(p.recovery_consequence_version(before.id).unwrap(), 1);
                assert_eq!(
                    managed_finished(&p, run).terminal_risk,
                    RecoveryRiskState::Open
                );
                assert_eq!(
                    p.load_instance_by_id(before.id)
                        .unwrap()
                        .unwrap()
                        .state_version,
                    before.state_version
                );
            } else {
                assert_preserved(&p, &before, run, open);
            }
            assert_eq!(count(&p, "SELECT COUNT(*) FROM run_executions"), 0);
            assert_eq!(count(&p, "SELECT COUNT(*) FROM run_revision_pins"), 0);
            assert_eq!(count(&p, "SELECT COUNT(*) FROM run_payload_pins"), 0);
            assert_eq!(count(&p, "SELECT COUNT(*) FROM run_restore_admissions"), 0);
            assert_eq!(
                count(&p, "SELECT COUNT(*) FROM run_migration_checkpoint_bindings"),
                0
            );
        }
    }
}

// Test-ID: PR-TEST-0327
// Verifies: PR-REQ-0053, PR-REQ-0058, PR-REQ-0065, PR-REQ-0067, PR-REQ-0069, PR-REQ-0071, PR-REQ-0308
#[test]
fn unresolved_obligations_root_diagnostics_not_replayable_input_history() {
    for case in CASES {
        let (_tmp, path) = root();
        let status = worker(&path, case, "open", Some("after_recovery_risk_commit"))
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(87));
        let p = PactrunPersistence::open(&path).unwrap();
        let before = only_instance(&p);
        let run = p.list_managed_runs(before.id).unwrap()[0].id;
        let app = PactrunApplication::open(&path).unwrap();
        assert_eq!(app.reconcile_lost_managed_owners().unwrap(), vec![run]);
        let guarded = only_instance(&p);
        let guard = p.load_instance_recovery_guard(before.id).unwrap().unwrap();
        assert_eq!(guard.run, run);
        // The unresolved guard is a strong FK root for its terminal provenance.
        assert!(
            p.database
                .lock()
                .unwrap()
                .execute(
                    "DELETE FROM run_outcomes WHERE run_id=?1",
                    [run.as_bytes().as_slice()]
                )
                .is_err()
        );
        assert_eq!(count(&p, "SELECT COUNT(*) FROM run_payload_pins"), 0);
        assert_eq!(
            count(&p, "SELECT COUNT(*) FROM run_migration_payload_pins"),
            0
        );
        assert_eq!(
            count(&p, "SELECT COUNT(*) FROM run_migration_revision_pins"),
            0
        );
        assert_eq!(
            count(&p, "SELECT COUNT(*) FROM run_migration_checkpoint_bindings"),
            0
        );
        // Legal later Input management is a new state, not recovery rollback.
        p.set_input(
            before.id,
            guarded.state_version,
            &mut ManagedInputWrite {
                input_id: InputIdentity::parse("config").unwrap(),
                byte_len: 5,
                reader: &mut Cursor::new(b"later"),
            },
        )
        .unwrap();
        assert!(app.reconcile_lost_managed_owners().unwrap().is_empty());
        let mut bytes = Vec::new();
        p.export_input(
            before.id,
            &InputIdentity::parse("config").unwrap(),
            false,
            &mut bytes,
        )
        .unwrap();
        assert_eq!(bytes, b"later");
        assert_eq!(
            p.load_instance_recovery_guard(before.id).unwrap(),
            Some(guard)
        );
        let inspection = p.managed_run_inspection(run).unwrap().unwrap();
        assert!(matches!(inspection.run.state, RunState::Finished(o)
            if o.outcome == RunOutcome::Interrupted && o.terminal_risk == RecoveryRiskState::Open));
        let current = only_instance(&p);
        assert!(
            app.resolve_manual_recovery(before.id, guarded.state_version)
                .is_err()
        );
        let resolved = app
            .resolve_manual_recovery(before.id, current.state_version)
            .unwrap();
        assert_ne!(resolved, current.state_version);
        assert!(p.load_instance_recovery_guard(before.id).unwrap().is_none());
        assert_eq!(
            p.list_managed_runs(before.id).unwrap().len(),
            1,
            "resolution is not a Run"
        );
        assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Interrupted);
    }
}

// Test-ID: PR-TEST-0330
// Verifies: PR-REQ-0025, PR-REQ-0026, PR-REQ-0063, PR-REQ-0064, PR-REQ-0069, PR-REQ-0071
#[test]
fn reconciliation_and_manual_resolution_publish_atomic_guard_boundaries() {
    for case in CASES {
        for (terminal_point, resolution_point, after) in [
            (
                "before_run_finish_commit",
                "before_manual_recovery_commit",
                false,
            ),
            (
                "after_run_finish_commit",
                "after_manual_recovery_commit",
                true,
            ),
        ] {
            let (_tmp, path) = root();
            assert_eq!(
                worker(&path, case, "open", Some("after_recovery_risk_commit"))
                    .status()
                    .unwrap()
                    .code(),
                Some(87)
            );
            let p = PactrunPersistence::open(&path).unwrap();
            let before = only_instance(&p);
            let run = p.list_managed_runs(before.id).unwrap()[0].id;
            assert_eq!(
                worker(&path, case, "reconcile-a", Some(terminal_point))
                    .status()
                    .unwrap()
                    .code(),
                Some(87)
            );
            let state = p.load_managed_run(run).unwrap().unwrap().state;
            assert_eq!(matches!(state, RunState::Finished(_)), after);
            assert_eq!(
                p.load_instance_recovery_guard(before.id).unwrap().is_some(),
                after
            );
            assert_eq!(
                p.recovery_consequence_version(before.id).unwrap(),
                i64::from(after)
            );
            assert_eq!(
                only_instance(&p).state_version == before.state_version,
                !after
            );
            let app = PactrunApplication::open(&path).unwrap();
            assert_eq!(
                app.reconcile_lost_managed_owners().unwrap().len(),
                usize::from(!after)
            );
            assert_eq!(managed_finished(&p, run).outcome, RunOutcome::Interrupted);
            let guarded = only_instance(&p);
            let guard = p.load_instance_recovery_guard(before.id).unwrap().unwrap();
            assert_eq!(
                worker(&path, case, "manual", Some(resolution_point))
                    .status()
                    .unwrap()
                    .code(),
                Some(87)
            );
            let current = only_instance(&p);
            assert_eq!(current.state_version == guarded.state_version, !after);
            assert_eq!(
                p.load_instance_recovery_guard(before.id).unwrap(),
                if after { None } else { Some(guard) }
            );
            assert_eq!(p.recovery_consequence_version(before.id).unwrap(), 1);
            assert_eq!(p.list_managed_runs(before.id).unwrap().len(), 1);
            assert_eq!(current.active_revision, before.active_revision);
            assert!(app.reconcile_lost_managed_owners().unwrap().is_empty());
        }
    }
}
