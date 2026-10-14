use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Barrier, atomic::Ordering, mpsc},
    thread,
    time::Duration,
};

use sha2::{Digest, Sha256};
use tempfile::TempDir;

use super::*;
use crate::{
    domain::{
        ActionV1, ActiveInstanceBindingReference, ContentId, DeclarationContent, HookLaunchV1,
        HookV1, IOContractV1, InputDeclarationV1, InputIdentity, InputProtectionV1, InstanceName,
        InstanceView, MANAGED_INPUT_CHUNK_BYTES_V1, RevisionDeclarationInput, RevisionIdentity,
        RunPhase, RuntimeContentProjectionInputV1, RuntimeFileKindV1, RuntimeFileV1, RuntimePath,
        Sha256Digest, TerminalContractV1, project_revision_declarations,
        project_runtime_content_closure_v1, validate_declaration_content,
    },
    hook::ActionCancellation,
    managed_data::{StagingSession, session_is_live},
    persistence::ManagedInputWrite,
};

const TOOL_BYTES: &[u8] = b"deploy tool";

const WORKER_TEST: &str = "persistence::sqlite_runs::tests::run_store_worker";

mod managed_access {
    include!("sqlite_managed_access_tests.rs");
}

fn root() -> (TempDir, PathBuf) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/run-store-tests");
    fs::create_dir_all(&parent).unwrap();
    let temporary = tempfile::Builder::new()
        .prefix("runs-")
        .tempdir_in(parent)
        .unwrap();
    let root = temporary.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("database")).unwrap();
    fs::create_dir(root.join("runtime-content")).unwrap();
    fs::create_dir(root.join("staging")).unwrap();
    (temporary, root)
}

/// A Revision with two Inputs and one Observe Direct Action `deploy`; the
/// Action is real so admission can read its access mode from the core.
fn revision(persistence: &PactrunPersistence) -> RevisionIdentity {
    let blob_digest = Sha256Digest::from_bytes(Sha256::digest(TOOL_BYTES).into());
    let publication = persistence
        .put_runtime_content(&blob_digest, &mut Cursor::new(TOOL_BYTES))
        .unwrap();
    let core = project_revision_declarations(RevisionDeclarationInput {
        inputs: vec![
            InputDeclarationV1 {
                id: InputIdentity::parse("required").unwrap(),
                required: true,
                protection: InputProtectionV1::Normal,
            },
            InputDeclarationV1 {
                id: InputIdentity::parse("secret").unwrap(),
                required: false,
                protection: InputProtectionV1::Secret,
            },
        ],
        actions: vec![ActionV1 {
            id: ActionIdentity::parse("deploy").unwrap(),
            access: OperationAccessV1::Observe,
            parameters: Vec::new(),
            hook: HookV1 {
                protocol_version: crate::domain::FormatVersion::BASELINE,
                launch: HookLaunchV1::Direct {
                    executable: ContentId::parse("tool").unwrap(),
                },
                args: Vec::new(),
                io: IOContractV1 {
                    terminal: TerminalContractV1::None,
                },
            },
            outputs: Vec::new(),
        }],
        snapshot: None,
        migrations: Vec::new(),
        cleanup: None,
    })
    .unwrap();
    let runtime_content = project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 {
        files: vec![RuntimeFileV1 {
            id: ContentId::parse("tool").unwrap(),
            path: RuntimePath::parse("bin/tool").unwrap(),
            kind: RuntimeFileKindV1::RegularFile,
            blob_digest,
            executable: true,
        }],
    })
    .unwrap();
    let content: DeclarationContent = validate_declaration_content(core, runtime_content).unwrap();
    persistence
        .persist_revision(
            crate::domain::PackageId::from_bytes([9; 16]),
            &content,
            &[publication],
        )
        .unwrap()
}

/// Runs the authoritative Admission transaction with facts read from the
/// stored Revision (Direct launch, so the launcher closure is never used).
fn admit(
    persistence: &PactrunPersistence,
    run: RunId,
    expected: InstanceStateVersion,
    active_bindings: &[ActiveInstanceBindingReference],
    override_guard: bool,
) -> Result<Result<(), AdmissionRefusal>, PersistenceError> {
    let view = persistence.load_run(run)?.expect("Run exists");
    let stored = persistence
        .load_revision(&view.action.revision)?
        .expect("Run Revision is persisted");
    let files = stored.content.runtime_content.files();
    let launch = CompiledHookLaunch::Direct {
        executable: files[0].clone(),
    };
    let facts = AdmissionFacts {
        service: None,
        expected_state_version: expected,
        active_bindings,
        runtime_content: files,
        launch: &launch,
    };
    persistence.admit_run(run, &facts, &|_| Ok(()), override_guard)
}

fn instance(
    persistence: &PactrunPersistence,
    revision: &RevisionIdentity,
    name: &str,
) -> InstanceView {
    let mut required = Cursor::new(b"required-bytes".to_vec());
    let mut secret = Cursor::new(b"secret-bytes".to_vec());
    let mut initial = [
        ManagedInputWrite {
            input_id: InputIdentity::parse("required").unwrap(),
            byte_len: 14,
            reader: &mut required,
        },
        ManagedInputWrite {
            input_id: InputIdentity::parse("secret").unwrap(),
            byte_len: 12,
            reader: &mut secret,
        },
    ];
    persistence
        .create_instance(
            InstanceName::parse(name).unwrap(),
            revision.clone(),
            &mut initial,
        )
        .unwrap()
}

fn bindings(
    persistence: &PactrunPersistence,
    instance: InstanceId,
) -> Vec<ActiveInstanceBindingReference> {
    persistence
        .observe_instance_compilation_state(instance)
        .unwrap()
        .unwrap()
        .active_bindings
}

fn token(persistence: &PactrunPersistence, instance: InstanceId) -> InstanceStateVersion {
    persistence
        .load_instance_by_id(instance)
        .unwrap()
        .unwrap()
        .state_version
}

fn owner() -> ExecutionOwnerSession {
    ExecutionOwnerSession::parse(format!("session-{}", "0".repeat(32))).unwrap()
}

struct CancelBeforeAcceptance;

impl AcceptanceArbiter for CancelBeforeAcceptance {
    fn before_durable_acceptance(
        &self,
        _commit: impl FnOnce() -> Result<(), PersistenceError>,
    ) -> AcceptanceCommitResult {
        AcceptanceCommitResult::Cancelled
    }
}

struct UncertainAfterAcceptance;

impl AcceptanceArbiter for UncertainAfterAcceptance {
    fn before_durable_acceptance(
        &self,
        commit: impl FnOnce() -> Result<(), PersistenceError>,
    ) -> AcceptanceCommitResult {
        match commit() {
            Ok(()) => AcceptanceCommitResult::Uncertain(PersistenceError::PublicationWitness(
                "injected acceptance commit uncertainty".to_owned(),
            )),
            Err(error) => AcceptanceCommitResult::Committed(Err(error)),
        }
    }
}

fn action(revision: &RevisionIdentity) -> ActionRunIdentity {
    ActionRunIdentity {
        revision: revision.clone(),
        action: ActionIdentity::parse("deploy").unwrap(),
    }
}

fn accepted(persistence: &PactrunPersistence, view: &InstanceView) -> RunId {
    persistence
        .create_accepted_run(
            RunId::generate().unwrap(),
            view.id,
            view.state_version,
            &action(&view.active_revision),
            &owner(),
            &crate::persistence::UnconditionalAcceptance,
        )
        .unwrap()
}

// Test-ID: PR-TEST-0121
// Verifies: PR-REQ-0049, PR-REQ-0286
#[test]
fn acceptance_boundary_orders_cancellation_and_preserves_candidate_identity() {
    let (_temporary, root) = root();
    let persistence = PactrunPersistence::open(&root).unwrap();
    let revision = revision(&persistence);
    let view = instance(&persistence, &revision, "acceptance-boundary");
    let action = action(&revision);
    let owner = owner();

    let cancelled_candidate = RunId::from_bytes([0x11; 16]);
    assert!(matches!(
        persistence.create_accepted_run(
            cancelled_candidate,
            view.id,
            view.state_version,
            &action,
            &owner,
            &CancelBeforeAcceptance,
        ),
        Err(AcceptanceError::Cancelled { run }) if run == cancelled_candidate
    ));
    assert_eq!(persistence.load_run(cancelled_candidate).unwrap(), None);
    assert_eq!(persistence.list_runs(view.id).unwrap().len(), 0);

    let committed_candidate = RunId::from_bytes([0x22; 16]);
    assert_eq!(
        persistence
            .create_accepted_run(
                committed_candidate,
                view.id,
                view.state_version,
                &action,
                &owner,
                &crate::persistence::UnconditionalAcceptance,
            )
            .unwrap(),
        committed_candidate
    );
    assert_eq!(
        persistence
            .load_run(committed_candidate)
            .unwrap()
            .unwrap()
            .id,
        committed_candidate
    );

    let uncertain_candidate = RunId::from_bytes([0x33; 16]);
    assert!(matches!(
        persistence.create_accepted_run(
            uncertain_candidate,
            view.id,
            view.state_version,
            &action,
            &owner,
            &UncertainAfterAcceptance,
        ),
        Err(AcceptanceError::Uncertain { run, .. }) if run == uncertain_candidate
    ));
    assert_eq!(
        persistence
            .load_run(uncertain_candidate)
            .unwrap()
            .unwrap()
            .id,
        uncertain_candidate
    );
    assert_eq!(persistence.list_runs(view.id).unwrap().len(), 2);

    // A retry is an exact readback/retry of the candidate and cannot
    // create a replacement Run.
    let duplicate = persistence.create_accepted_run(
        uncertain_candidate,
        view.id,
        view.state_version,
        &action,
        &owner,
        &crate::persistence::UnconditionalAcceptance,
    );
    assert!(
        matches!(duplicate, Err(AcceptanceError::NotCommitted { run, .. }) if run == uncertain_candidate)
    );
    assert_eq!(persistence.list_runs(view.id).unwrap().len(), 2);
}

// Test-ID: PR-TEST-0128
// Verifies: PR-REQ-0049, PR-REQ-0286
#[test]
fn acceptance_cancellation_race_is_concurrent_and_deterministic() {
    let (_temporary, root) = root();
    let persistence = Arc::new(PactrunPersistence::open(&root).unwrap());
    let revision = revision(&persistence);
    let view = instance(&persistence, &revision, "acceptance-race");
    let action = action(&revision);
    let owner = owner();
    let instance_id = view.id;
    let state_version = view.state_version;

    let cancelled = ActionCancellation::default();
    let before = Arc::new(Barrier::new(2));
    let after = Arc::new(Barrier::new(2));
    cancelled.install_acceptance_test_hooks(before.clone(), after.clone());
    let candidate = RunId::from_bytes([0x44; 16]);
    let acceptance = {
        let persistence = Arc::clone(&persistence);
        let cancelled = cancelled.clone();
        let action = action.clone();
        let owner = owner.clone();
        thread::spawn(move || {
            persistence.create_accepted_run(
                candidate,
                instance_id,
                state_version,
                &action,
                &owner,
                &cancelled,
            )
        })
    };
    let cancellation = {
        let cancelled = cancelled.clone();
        thread::spawn(move || cancelled.request())
    };
    cancellation.join().unwrap();
    before.wait();
    after.wait();
    assert!(matches!(
        acceptance.join().unwrap(),
        Err(AcceptanceError::Cancelled { run }) if run == candidate
    ));
    assert_eq!(persistence.load_run(candidate).unwrap(), None);

    let committed = ActionCancellation::default();
    let before = Arc::new(Barrier::new(2));
    let after = Arc::new(Barrier::new(2));
    let gate_entered = committed.install_acceptance_test_hooks(before.clone(), after.clone());
    let candidate = RunId::from_bytes([0x55; 16]);
    let acceptance = {
        let persistence = Arc::clone(&persistence);
        let committed = committed.clone();
        let action = action.clone();
        let owner = owner.clone();
        thread::spawn(move || {
            persistence.create_accepted_run(
                candidate,
                instance_id,
                state_version,
                &action,
                &owner,
                &committed,
            )
        })
    };
    before.wait();
    while !gate_entered.load(Ordering::Acquire) {
        thread::yield_now();
    }
    let (started, ready) = mpsc::channel();
    let cancellation = {
        let committed = committed.clone();
        thread::spawn(move || {
            started.send(()).unwrap();
            committed.request();
        })
    };
    ready.recv().unwrap();
    after.wait();
    assert_eq!(acceptance.join().unwrap().unwrap(), candidate);
    cancellation.join().unwrap();
    assert!(committed.is_requested());
    assert_eq!(persistence.list_runs(view.id).unwrap().len(), 1);
    assert_eq!(
        persistence.load_run(candidate).unwrap().unwrap().id,
        candidate
    );
    persistence
        .finish_run_owned(
            &owner,
            candidate,
            &RunFinish {
                outcome: RunOutcome::Cancelled,
                primary_failure: None,
                secondary_failures: Vec::new(),
                hook_completion: None,
            },
            &[],
            &mut [],
        )
        .unwrap();
    assert!(matches!(
        persistence.load_run(candidate).unwrap().unwrap().state,
        RunState::Finished(outcome) if outcome.outcome == RunOutcome::Cancelled
    ));
}

fn admitted(persistence: &PactrunPersistence, view: &InstanceView) -> RunId {
    let run = accepted(persistence, view);
    admit(
        persistence,
        run,
        view.state_version,
        &bindings(persistence, view.id),
        false,
    )
    .unwrap()
    .unwrap();
    run
}

fn count(persistence: &PactrunPersistence, sql: &str) -> i64 {
    persistence
        .database
        .lock()
        .unwrap()
        .query_row(sql, [], |row| row.get(0))
        .unwrap()
}

fn execute(
    persistence: &PactrunPersistence,
    sql: &str,
    parameters: &[&[u8]],
) -> rusqlite::Result<usize> {
    let database = persistence.database.lock().unwrap();
    database.execute(sql, rusqlite::params_from_iter(parameters.iter()))
}

fn error(owner: &str, code: &str) -> PactrunErrorRef {
    PactrunErrorRef::new(owner, code).unwrap()
}

fn plain_finish(outcome: RunOutcome) -> RunFinish {
    RunFinish {
        outcome,
        primary_failure: None,
        secondary_failures: Vec::new(),
        hook_completion: None,
    }
}

// Supporting coverage for the Run inspection substrate. This is not
// verification of the user-visible Run-detail requirement.
#[test]
fn run_inspection_uses_one_read_snapshot_for_run_and_current_guard() {
    let (_temporary, root) = root();
    let persistence = PactrunPersistence::open(&root).unwrap();
    let revision = revision(&persistence);
    let view = instance(&persistence, &revision, "inspection-snapshot");

    let historical_run = admitted(&persistence, &view);
    persistence
        .finish_run(
            historical_run,
            &plain_finish(RunOutcome::Cancelled),
            &mut [],
        )
        .unwrap();
    let guard_run = admitted(&persistence, &view);
    persistence.open_recovery_risk(guard_run).unwrap();
    persistence
        .finish_run(guard_run, &plain_finish(RunOutcome::Failed), &mut [])
        .unwrap();
    let guarded_token = token(&persistence, view.id);

    let inspection_persistence = PactrunPersistence::open(&root).unwrap();
    let (snapshot_started_tx, snapshot_started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let inspection = thread::spawn(move || {
        inspection_persistence.load_run_inspection_with_hook(historical_run, || {
            snapshot_started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        })
    });
    snapshot_started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("inspection acquired its read snapshot");

    let resolver_root = root.clone();
    let (mutation_started_tx, mutation_started_rx) = mpsc::channel();
    let (mutation_done_tx, mutation_done_rx) = mpsc::channel();
    let resolver = thread::spawn(move || {
        mutation_started_tx.send(()).unwrap();
        let resolver = PactrunPersistence::open(&resolver_root).unwrap();
        mutation_done_tx
            .send(
                resolver
                    .resolve_manual_recovery(view.id, guarded_token)
                    .is_ok(),
            )
            .unwrap();
    });
    mutation_started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("recovery-guard resolver started");
    assert!(
        mutation_done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("recovery-guard mutation committed while inspection stayed open")
    );
    release_tx.send(()).unwrap();

    let inspection = inspection.join().unwrap().unwrap().unwrap();
    resolver.join().unwrap();
    let RunState::Finished(outcome) = &inspection.run.state else {
        panic!("expected the inspected Run to be terminal");
    };
    assert_eq!(outcome.outcome, RunOutcome::Cancelled);
    assert_eq!(outcome.terminal_risk, RecoveryRiskState::Clear);
    assert_eq!(
        inspection
            .current_recovery_guard
            .as_ref()
            .expect("the old snapshot still contains the current guard")
            .run,
        guard_run
    );

    let next = persistence
        .load_run_inspection(historical_run)
        .unwrap()
        .unwrap();
    assert_eq!(next.run.state, RunState::Finished(outcome.clone()));
    assert!(next.current_recovery_guard.is_none());
}

// Test-ID: PR-TEST-0105
// Verifies: PR-REQ-0275
#[test]
fn accepted_open_risk_corruption_fails_closed_without_mutation() {
    let (_temporary, root) = root();
    let persistence = PactrunPersistence::open(&root).unwrap();
    let revision = revision(&persistence);
    let view = instance(&persistence, &revision, "accepted-open-corrupt");
    let run = accepted(&persistence, &view);
    let stored = persistence
        .load_revision(&revision)
        .unwrap()
        .expect("revision is persisted");
    let files = stored.content.runtime_content.files();
    let launch = CompiledHookLaunch::Direct {
        executable: files[0].clone(),
    };
    let active_bindings = bindings(&persistence, view.id);
    let facts = AdmissionFacts {
        service: None,
        expected_state_version: view.state_version,
        active_bindings: &active_bindings,
        runtime_content: files,
        launch: &launch,
    };
    let before_version = token(&persistence, view.id);
    execute(
        &persistence,
        "UPDATE run_executions SET risk_state=1 WHERE run_id=?1",
        &[run.as_bytes().as_slice()],
    )
    .unwrap();

    assert!(matches!(
        persistence.load_run(run),
        Err(PersistenceError::CorruptRun(_))
    ));
    assert!(matches!(
        persistence.admit_run(run, &facts, &|_| Ok(()), false),
        Err(PersistenceError::CorruptRun(_))
    ));
    assert_eq!(token(&persistence, view.id), before_version);
    assert_eq!(count(&persistence, "SELECT COUNT(*) FROM run_outcomes"), 0);
    assert_eq!(
        count(
            &persistence,
            "SELECT COUNT(*) FROM instance_recovery_guards"
        ),
        0
    );
    assert_eq!(
        count(&persistence, "SELECT COUNT(*) FROM run_revision_pins"),
        0
    );
    assert_eq!(
        persistence
            .database
            .lock()
            .unwrap()
            .query_row(
                "SELECT risk_state FROM run_executions WHERE run_id=?1",
                [run.as_bytes().as_slice()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}

// Test-ID: PR-TEST-0112
// Verifies: PR-REQ-0064, PR-REQ-0275, PR-REQ-0287
#[test]
fn owner_reconciliation_finishes_clear_risk_runs_by_boundary() {
    let (_temporary, root) = root();
    let persistence = PactrunPersistence::open(&root).unwrap();
    let revision = revision(&persistence);

    let accepted_view = instance(&persistence, &revision, "reconcile-accepted");
    let accepted_run = accepted(&persistence, &accepted_view);
    assert!(
        persistence
            .reconcile_action_run(accepted_run, &owner())
            .unwrap()
    );
    let accepted_outcome = finished(&persistence, accepted_run);
    assert_eq!(accepted_outcome.outcome, RunOutcome::Interrupted);
    assert_eq!(accepted_outcome.boundary, ActionRunBoundary::Accepted);
    assert_eq!(accepted_outcome.terminal_risk, RecoveryRiskState::Clear);
    assert!(
        !persistence
            .reconcile_action_run(accepted_run, &owner())
            .unwrap()
    );

    let admitted_view = instance(&persistence, &revision, "reconcile-admitted");
    let admitted_run = admitted(&persistence, &admitted_view);
    assert!(
        persistence
            .reconcile_action_run(admitted_run, &owner())
            .unwrap()
    );
    let admitted_outcome = finished(&persistence, admitted_run);
    assert_eq!(admitted_outcome.outcome, RunOutcome::Interrupted);
    assert_eq!(admitted_outcome.boundary, ActionRunBoundary::Admitted);
    assert_eq!(admitted_outcome.terminal_risk, RecoveryRiskState::Clear);
    assert_eq!(
        count(&persistence, "SELECT COUNT(*) FROM run_revision_pins"),
        0
    );
    assert_eq!(
        count(&persistence, "SELECT COUNT(*) FROM run_payload_pins"),
        0
    );
    assert!(
        persistence
            .load_instance_recovery_guard(admitted_view.id)
            .unwrap()
            .is_none()
    );
}

fn state(persistence: &PactrunPersistence, run: RunId) -> RunState {
    persistence.load_run(run).unwrap().unwrap().state
}

fn running(persistence: &PactrunPersistence, run: RunId) -> RunExecutionView {
    match state(persistence, run) {
        RunState::Running(execution) => execution,
        other => panic!("expected a Running Run, found {other:?}"),
    }
}

fn finished(persistence: &PactrunPersistence, run: RunId) -> RunOutcomeView {
    match state(persistence, run) {
        RunState::Finished(outcome) => outcome,
        other => panic!("expected a Finished Run, found {other:?}"),
    }
}

fn run_worker(
    root: &Path,
    operation: &str,
    run: RunId,
    fault: Option<FaultPoint>,
    marker: &Path,
) -> bool {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg(WORKER_TEST)
        .arg("--nocapture")
        .env("PACTRUN_RUN_TEST_WORKER", operation)
        .env("PACTRUN_RUN_TEST_ROOT", root)
        .env("PACTRUN_RUN_TEST_RUN", run.to_string())
        .env("PACTRUN_RUN_TEST_SUCCESS_MARKER", marker)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(fault) = fault {
        command.env("PACTRUN_RUN_TEST_FAULT", fault.name());
    }
    let output = command.output().unwrap();
    if !output.status.success() && fault.is_none() {
        eprintln!(
            "Run worker failed with {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    output.status.success()
}

#[test]
fn run_store_worker() {
    let Some(operation) = std::env::var_os("PACTRUN_RUN_TEST_WORKER") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("PACTRUN_RUN_TEST_ROOT").unwrap());
    let marker = PathBuf::from(std::env::var_os("PACTRUN_RUN_TEST_SUCCESS_MARKER").unwrap());
    let run = std::env::var("PACTRUN_RUN_TEST_RUN")
        .unwrap()
        .parse::<RunId>()
        .unwrap();
    let persistence = PactrunPersistence::open(&root).unwrap();
    match operation.to_string_lossy().as_ref() {
        "pin" => {
            let view = persistence.load_run(run).unwrap().unwrap();
            let expected = token(&persistence, view.instance);
            admit(
                &persistence,
                run,
                expected,
                &bindings(&persistence, view.instance),
                false,
            )
            .unwrap()
            .unwrap();
        }
        "open_risk" => persistence.open_recovery_risk(run).unwrap(),
        "finish_failed" => {
            persistence
                .finish_run(run, &plain_finish(RunOutcome::Failed), &mut [])
                .unwrap();
        }
        other => panic!("unknown Run worker operation {other}"),
    }
    fs::write(marker, b"run-operation-returned-success").unwrap();
}

// Test-ID: PR-TEST-0083
// Verifies: PR-REQ-0047, PR-REQ-0048
#[test]
fn execution_pins_are_durable_release_only_at_terminal_and_never_guard() {
    let (temporary, root) = root();
    let persistence = PactrunPersistence::open(&root).unwrap();
    let revision = revision(&persistence);
    let view = instance(&persistence, &revision, "pins");
    let stale_run = accepted(&persistence, &view);
    assert_eq!(token(&persistence, view.id), view.state_version);
    let active = bindings(&persistence, view.id);
    assert_eq!(active.len(), 2);

    // A stale expected token is refused inside the Admission transaction and
    // the refusal is that Run's durable terminal outcome; no pin exists.
    let stale = InstanceStateVersion::generate().unwrap();
    assert!(matches!(
        admit(&persistence, stale_run, stale, &active, false),
        Ok(Err(AdmissionRefusal::PlanInvalidated(_)))
    ));
    assert_eq!(
        count(&persistence, "SELECT COUNT(*) FROM run_revision_pins"),
        0
    );
    assert_eq!(
        finished(&persistence, stale_run).outcome,
        RunOutcome::Failed
    );
    let run = accepted(&persistence, &view);
    admit(&persistence, run, view.state_version, &active, false)
        .unwrap()
        .unwrap();
    assert_eq!(token(&persistence, view.id), view.state_version);
    assert!(matches!(
        admit(&persistence, run, view.state_version, &active, false),
        Err(PersistenceError::InvalidRunTransition(_))
    ));
    let execution = running(&persistence, run);
    assert_eq!(execution.boundary, ActionRunBoundary::Admitted);
    assert_eq!(execution.risk_state, RecoveryRiskState::Clear);
    assert_eq!(execution.owner, owner());
    assert_eq!(
        count(&persistence, "SELECT COUNT(*) FROM run_payload_pins"),
        2
    );

    // A pin is lifetime bookkeeping, not a guard: current-binding
    // replacement succeeds while the pinned payload stays physically
    // present and unreachable from the Instance.
    let old_secret = active
        .iter()
        .find(|binding| binding.input.as_str() == "secret")
        .unwrap()
        .payload;
    let mut replacement = Cursor::new(b"replacement".to_vec());
    let mut write = ManagedInputWrite {
        input_id: InputIdentity::parse("secret").unwrap(),
        byte_len: 11,
        reader: &mut replacement,
    };
    let replaced = persistence
        .set_input(view.id, view.state_version, &mut write)
        .unwrap();
    assert_ne!(replaced, view.state_version);
    assert_eq!(
        count(&persistence, "SELECT COUNT(*) FROM managed_input_payloads"),
        3
    );
    assert!(
        execute(
            &persistence,
            "DELETE FROM managed_input_payloads WHERE payload_id=?1",
            &[old_secret.as_bytes().as_slice()],
        )
        .is_err()
    );
    let mut current = Vec::new();
    persistence
        .export_input(
            view.id,
            &InputIdentity::parse("secret").unwrap(),
            true,
            &mut current,
        )
        .unwrap();
    assert_eq!(current, b"replacement");

    // Pins survive process failure after the pin commit and never exist
    // before it.
    let crash_view = instance(&persistence, &revision, "pins-crash");
    let before_run = accepted(&persistence, &crash_view);
    let before_marker = temporary.path().join("pins-before.success");
    assert!(!run_worker(
        &root,
        "pin",
        before_run,
        Some(FaultPoint::BeforeRunAdmitCommit),
        &before_marker,
    ));
    assert!(!before_marker.exists());
    let after_run = accepted(&persistence, &crash_view);
    let after_marker = temporary.path().join("pins-after.success");
    assert!(!run_worker(
        &root,
        "pin",
        after_run,
        Some(FaultPoint::AfterRunAdmitCommit),
        &after_marker,
    ));
    drop(persistence);
    let reopened = PactrunPersistence::open(&root).unwrap();
    assert_eq!(
        running(&reopened, before_run).boundary,
        ActionRunBoundary::Accepted
    );
    assert_eq!(
        running(&reopened, after_run).boundary,
        ActionRunBoundary::Admitted
    );
    assert_eq!(count(&reopened, "SELECT COUNT(*) FROM run_payload_pins"), 4);

    // Release happens only in the terminal transaction; the unbound
    // payload is reclaimed then, the bound one is kept, and a clear-risk
    // finish publishes no state version.
    let receipt = reopened
        .finish_run(run, &plain_finish(RunOutcome::Failed), &mut [])
        .unwrap();
    assert_eq!(receipt.published_state_version, None);
    assert_eq!(token(&reopened, view.id), replaced);
    assert_eq!(count(&reopened, "SELECT COUNT(*) FROM run_payload_pins"), 2);
    assert_eq!(
        count(
            &reopened,
            "SELECT COUNT(*) FROM managed_input_payloads p \
                 JOIN instances i ON i.instance_id = p.instance_id \
                 WHERE i.instance_name = CAST('pins' AS BLOB)",
        ),
        2
    );
    let mut current = Vec::new();
    reopened
        .export_input(
            view.id,
            &InputIdentity::parse("secret").unwrap(),
            true,
            &mut current,
        )
        .unwrap();
    assert_eq!(current, b"replacement");
    let outcome = finished(&reopened, run);
    assert_eq!(outcome.outcome, RunOutcome::Failed);
    assert_eq!(outcome.boundary, ActionRunBoundary::Admitted);
}

// Test-ID: PR-TEST-0084
// Verifies: PR-REQ-0050, PR-REQ-0066, PR-REQ-0275, PR-REQ-0283
#[test]
fn run_phase_outcome_and_failure_records_are_exact() {
    let (_temporary, root) = root();
    let persistence = PactrunPersistence::open(&root).unwrap();
    let revision = revision(&persistence);
    let view = instance(&persistence, &revision, "outcomes");

    let accepted_run = accepted(&persistence, &view);
    assert_eq!(
        count(&persistence, "SELECT COUNT(*) FROM run_executions"),
        1
    );
    assert_eq!(count(&persistence, "SELECT COUNT(*) FROM run_outcomes"), 0);
    assert_eq!(
        running(&persistence, accepted_run).boundary,
        ActionRunBoundary::Accepted
    );
    assert!(
        execute(
            &persistence,
            "INSERT INTO run_artifacts(run_id, output_identity, byte_length) \
                 VALUES (?1, ?2, 0)",
            &[accepted_run.as_bytes().as_slice(), b"report"],
        )
        .is_err()
    );

    // An Admission failure finishes a never-admitted Run: the boundary is
    // recorded, not derived from (absent) pins.
    let admission_failure = RunFinish {
        outcome: RunOutcome::Failed,
        primary_failure: Some(RunPrimaryFailure {
            cause: None,
            failure: RunFailureRecord {
                error: error("admission", "plan_invalidated"),
                message: "stale InstanceStateVersion".to_owned(),
            },
            step: RunFailedStep::Admission,
        }),
        secondary_failures: vec![RunFailureRecord {
            error: error("execution", "workspace_cleanup_failed"),
            message: String::new(),
        }],
        hook_completion: None,
    };
    persistence
        .finish_run(accepted_run, &admission_failure, &mut [])
        .unwrap();
    assert!(matches!(
        persistence.finish_run(accepted_run, &plain_finish(RunOutcome::Failed), &mut []),
        Err(PersistenceError::RunNotRunning)
    ));
    let outcome = finished(&persistence, accepted_run);
    assert_eq!(outcome.outcome, RunOutcome::Failed);
    assert_eq!(outcome.boundary, ActionRunBoundary::Accepted);
    assert_eq!(outcome.terminal_risk, RecoveryRiskState::Clear);
    assert_eq!(outcome.primary_failure, admission_failure.primary_failure);
    assert_eq!(
        outcome.secondary_failures,
        admission_failure.secondary_failures
    );
    assert_eq!(outcome.hook_completion, None);

    let completions = [
        None,
        Some(HookCompletionRecord {
            status: HookCompletionStatus::Success,
            code: None,
            message: Some(String::new()),
        }),
        Some(HookCompletionRecord {
            status: HookCompletionStatus::Success,
            code: Some(HookCodeV1::parse("report_ready").unwrap()),
            message: Some("done".to_owned()),
        }),
    ];
    let mut legacy_completions = Vec::new();
    for completion in &completions {
        let run = admitted(&persistence, &view);
        let finish = RunFinish {
            outcome: RunOutcome::Succeeded,
            primary_failure: None,
            secondary_failures: Vec::new(),
            hook_completion: completion.clone(),
        };
        persistence.finish_run(run, &finish, &mut []).unwrap();
        let outcome = finished(&persistence, run);
        assert_eq!(outcome.outcome, RunOutcome::Succeeded);
        assert_eq!(outcome.boundary, ActionRunBoundary::Admitted);
        assert_eq!(outcome.hook_completion, *completion);
        legacy_completions.push((run, completion.clone()));
    }
    let reopened = PactrunPersistence::open(&root).unwrap();
    for (run, completion) in legacy_completions {
        assert_eq!(finished(&reopened, run).hook_completion, completion);
    }
    for outcome in [
        RunOutcome::Cancelled,
        RunOutcome::TimedOut,
        RunOutcome::Interrupted,
    ] {
        let run = admitted(&persistence, &view);
        let finish = RunFinish {
            outcome,
            primary_failure: None,
            secondary_failures: Vec::new(),
            hook_completion: Some(HookCompletionRecord {
                status: HookCompletionStatus::Failure,
                code: Some(HookCodeV1::parse("aborted").unwrap()),
                message: None,
            }),
        };
        persistence.finish_run(run, &finish, &mut []).unwrap();
        let view = finished(&persistence, run);
        assert_eq!(view.outcome, outcome);
        assert_eq!(view.hook_completion, finish.hook_completion);
    }

    // Success cannot carry a failure record, and ManualRecoveryRequired
    // is not an outcome rank.
    let rejected = admitted(&persistence, &view);
    assert!(matches!(
        persistence.finish_run(
            rejected,
            &RunFinish {
                outcome: RunOutcome::Succeeded,
                primary_failure: admission_failure.primary_failure.clone(),
                secondary_failures: Vec::new(),
                hook_completion: None,
            },
            &mut [],
        ),
        Err(PersistenceError::InvalidRunTransition(_))
    ));
    running(&persistence, rejected);
    assert!(
        execute(
            &persistence,
            "INSERT INTO run_outcomes(\
                    run_id, outcome_rank, admitted_rank, risk_state, finished_at_unix_ms\
                 ) VALUES (?1, 5, 1, 0, 0)",
            &[rejected.as_bytes().as_slice()],
        )
        .is_err()
    );

    let summaries = persistence.list_runs(view.id).unwrap();
    assert_eq!(summaries.len(), 8);
    assert!(summaries.windows(2).all(|pair| pair[0].id < pair[1].id));
    assert_eq!(
        summaries
            .iter()
            .filter(|summary| summary.phase == RunPhase::Running)
            .count(),
        1
    );
    assert!(
        summaries
            .iter()
            .all(|summary| summary.action.action.as_str() == "deploy")
    );

    // Current Run metadata tables have no dedicated sensitive-value fields.
    let database = persistence.database.lock().unwrap();
    let mut statement = database
        .prepare(
            "SELECT t.name, c.name FROM pragma_table_list t \
                 JOIN pragma_table_info(t.name) c \
                 WHERE t.schema='main' AND (t.name LIKE 'run_%' OR t.name = 'runs' \
                    OR t.name = 'instance_recovery_guards')",
        )
        .unwrap();
    let columns = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        columns
            .iter()
            .map(|(table, _)| table.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        39 // Run relations, including scoped artifacts and non-pinning Input history.
    );
    assert!(
        columns
            .iter()
            .all(|(_, column)| !column.contains("parameter") && !column.contains("secret"))
    );
    assert_eq!(
        columns
            .iter()
            .filter(|(table, _)| table == "run_action_invocations")
            .count(),
        4
    );
}

// Test-ID: PR-TEST-0085
// Verifies: PR-REQ-0054, PR-REQ-0063, PR-REQ-0065, PR-REQ-0071, PR-REQ-0109, PR-REQ-0174, PR-REQ-0175
#[test]
fn recovery_risk_is_durable_and_terminal_consequence_is_atomic() {
    let (temporary, root) = root();
    let persistence = PactrunPersistence::open(&root).unwrap();
    let revision = revision(&persistence);
    let view = instance(&persistence, &revision, "recovery");

    let accepted_only = accepted(&persistence, &view);
    assert!(matches!(
        persistence.open_recovery_risk(accepted_only),
        Err(PersistenceError::InvalidRunTransition(_))
    ));
    let run = admitted(&persistence, &view);
    assert!(matches!(
        persistence.clear_recovery_risk(run),
        Err(PersistenceError::InvalidRunTransition(_))
    ));
    persistence.open_recovery_risk(run).unwrap();
    assert!(matches!(
        persistence.open_recovery_risk(run),
        Err(PersistenceError::InvalidRunTransition(_))
    ));
    persistence.clear_recovery_risk(run).unwrap();
    assert_eq!(token(&persistence, view.id), view.state_version);

    // Risk markers are durable exactly at their commit.
    let before_marker = temporary.path().join("risk-before.success");
    assert!(!run_worker(
        &root,
        "open_risk",
        run,
        Some(FaultPoint::BeforeRecoveryRiskCommit),
        &before_marker,
    ));
    assert_eq!(
        running(&persistence, run).risk_state,
        RecoveryRiskState::Clear
    );
    let after_marker = temporary.path().join("risk-after.success");
    assert!(!run_worker(
        &root,
        "open_risk",
        run,
        Some(FaultPoint::AfterRecoveryRiskCommit),
        &after_marker,
    ));
    assert_eq!(
        running(&persistence, run).risk_state,
        RecoveryRiskState::Open
    );

    // Open risk rejects success and keeps the Run Running.
    assert!(matches!(
        persistence.finish_run(run, &plain_finish(RunOutcome::Succeeded), &mut []),
        Err(PersistenceError::InvalidRunTransition(_))
    ));
    running(&persistence, run);

    // A clear-risk non-success publishes no guard and no state version.
    persistence.clear_recovery_risk(run).unwrap();
    let receipt = persistence
        .finish_run(run, &plain_finish(RunOutcome::Cancelled), &mut [])
        .unwrap();
    assert_eq!(receipt.published_state_version, None);
    assert_eq!(token(&persistence, view.id), view.state_version);
    assert_eq!(
        persistence.load_instance_recovery_guard(view.id).unwrap(),
        None
    );

    // An open-risk failure crash before the terminal commit leaves the Run
    // Running, no guard, and the old token.
    let open_run = admitted(&persistence, &view);
    persistence.open_recovery_risk(open_run).unwrap();
    let finish_before = temporary.path().join("finish-before.success");
    assert!(!run_worker(
        &root,
        "finish_failed",
        open_run,
        Some(FaultPoint::BeforeRunFinishCommit),
        &finish_before,
    ));
    running(&persistence, open_run);
    assert_eq!(
        persistence.load_instance_recovery_guard(view.id).unwrap(),
        None
    );
    assert_eq!(token(&persistence, view.id), view.state_version);

    // The consequence and the new state version are one commit.
    let finish_after = temporary.path().join("finish-after.success");
    assert!(!run_worker(
        &root,
        "finish_failed",
        open_run,
        Some(FaultPoint::AfterRunFinishCommit),
        &finish_after,
    ));
    drop(persistence);

    // Reopen: trigger, triggering Run, Action identity, outcome, terminal
    // risk, and diagnostics are reconstructed from durable rows alone.
    let reopened = PactrunPersistence::open(&root).unwrap();
    let guard = reopened
        .load_instance_recovery_guard(view.id)
        .unwrap()
        .expect("open-risk failure publishes a guard");
    assert_eq!(guard.instance, view.id);
    assert_eq!(guard.run, open_run);
    assert_eq!(guard.trigger, ManualRecoveryTrigger::OpenRiskFailure);
    let guarded_token = token(&reopened, view.id);
    assert_ne!(guarded_token, view.state_version);
    let triggering = reopened.load_run(guard.run).unwrap().unwrap();
    assert_eq!(triggering.action, action(&revision));
    assert_eq!(triggering.accepted_state_version, view.state_version);
    let outcome = finished(&reopened, guard.run);
    assert_eq!(outcome.outcome, RunOutcome::Failed);
    assert_eq!(outcome.terminal_risk, RecoveryRiskState::Open);
    assert_eq!(outcome.boundary, ActionRunBoundary::Admitted);
    assert!(
        execute(
            &reopened,
            "DELETE FROM runs WHERE run_id=?1",
            &[guard.run.as_bytes().as_slice()],
        )
        .is_err()
    );

    // The guard blocks ordinary admission; an override admits exactly one
    // execution, and a second open-risk finish leaves the guard unchanged.
    let blocked = reopened
        .create_accepted_run(
            RunId::generate().unwrap(),
            view.id,
            guarded_token,
            &action(&revision),
            &owner(),
            &crate::persistence::UnconditionalAcceptance,
        )
        .unwrap();
    let active = bindings(&reopened, view.id);
    assert!(matches!(
        admit(&reopened, blocked, guarded_token, &active, false),
        Ok(Err(AdmissionRefusal::RecoveryGuardActive))
    ));
    assert_eq!(finished(&reopened, blocked).outcome, RunOutcome::Failed);
    let overridden = reopened
        .create_accepted_run(
            RunId::generate().unwrap(),
            view.id,
            guarded_token,
            &action(&revision),
            &owner(),
            &crate::persistence::UnconditionalAcceptance,
        )
        .unwrap();
    admit(&reopened, overridden, guarded_token, &active, true)
        .unwrap()
        .unwrap();
    reopened.open_recovery_risk(overridden).unwrap();
    let receipt = reopened
        .finish_run(overridden, &plain_finish(RunOutcome::TimedOut), &mut [])
        .unwrap();
    assert_eq!(receipt.published_state_version, None);
    assert_eq!(token(&reopened, view.id), guarded_token);
    assert_eq!(
        reopened
            .load_instance_recovery_guard(view.id)
            .unwrap()
            .unwrap(),
        guard
    );

    // ResolveManualRecovery is token-first and publishes a fresh version.
    assert!(matches!(
        reopened.resolve_manual_recovery(view.id, view.state_version),
        Err(PersistenceError::StaleInstanceState)
    ));
    let resolved = reopened
        .resolve_manual_recovery(view.id, guarded_token)
        .unwrap();
    assert_ne!(resolved, guarded_token);
    assert_eq!(token(&reopened, view.id), resolved);
    assert_eq!(
        reopened.load_instance_recovery_guard(view.id).unwrap(),
        None
    );
    assert!(matches!(
        reopened.resolve_manual_recovery(view.id, resolved),
        Err(PersistenceError::MissingRecoveryGuard)
    ));

    // Owner loss and success-with-open-risk derive their own triggers.
    let loss_view = instance(&reopened, &revision, "owner-loss");
    let loss_run = admitted(&reopened, &loss_view);
    reopened.open_recovery_risk(loss_run).unwrap();
    reopened
        .finish_run(loss_run, &plain_finish(RunOutcome::Interrupted), &mut [])
        .unwrap();
    assert_eq!(
        reopened
            .load_instance_recovery_guard(loss_view.id)
            .unwrap()
            .unwrap()
            .trigger,
        ManualRecoveryTrigger::OpenRiskOwnerLoss
    );
    let success_view = instance(&reopened, &revision, "success-open");
    let success_run = admitted(&reopened, &success_view);
    reopened.open_recovery_risk(success_run).unwrap();
    reopened
        .finish_run(
            success_run,
            &RunFinish {
                outcome: RunOutcome::Failed,
                primary_failure: None,
                secondary_failures: Vec::new(),
                hook_completion: Some(HookCompletionRecord {
                    status: HookCompletionStatus::Success,
                    code: None,
                    message: None,
                }),
            },
            &mut [],
        )
        .unwrap();
    assert_eq!(
        reopened
            .load_instance_recovery_guard(success_view.id)
            .unwrap()
            .unwrap()
            .trigger,
        ManualRecoveryTrigger::SuccessWithOpenRisk
    );
}

// Test-ID: PR-TEST-0086
// Verifies: PR-REQ-0277
#[test]
fn run_owner_record_matches_session_lease_liveness() {
    let (_temporary, root) = root();
    let persistence = PactrunPersistence::open(&root).unwrap();
    let revision = revision(&persistence);
    let view = instance(&persistence, &revision, "owner");
    let session = StagingSession::open(&root).unwrap();
    let owner = session.owner();
    assert!(session_is_live(&root, &owner).unwrap());

    let run = persistence
        .create_accepted_run(
            RunId::generate().unwrap(),
            view.id,
            view.state_version,
            &action(&revision),
            &owner,
            &crate::persistence::UnconditionalAcceptance,
        )
        .unwrap();
    assert_eq!(running(&persistence, run).owner, owner);
    assert_eq!(
        count(&persistence, "SELECT COUNT(*) FROM run_executions"),
        1
    );

    // Another live session is not the owner and does not affect the probe;
    // a session that never existed is loss.
    let other = StagingSession::open(&root).unwrap();
    assert!(session_is_live(&root, &owner).unwrap());
    assert!(session_is_live(&root, &other.owner()).unwrap());
    drop(other);
    let never = ExecutionOwnerSession::parse(format!("session-{}", "f".repeat(32))).unwrap();
    assert!(!session_is_live(&root, &never).unwrap());

    // An unlocked lease file is loss even though the directory exists,
    // and the probe never removes it.
    let stale_name = format!("session-{}", "1".repeat(32));
    let stale = root.join("staging").join(&stale_name);
    fs::create_dir(&stale).unwrap();
    fs::write(stale.join(".lease"), b"").unwrap();
    let stale_owner = ExecutionOwnerSession::parse(stale_name).unwrap();
    assert!(!session_is_live(&root, &stale_owner).unwrap());
    assert!(stale.join(".lease").exists());

    // Dropping the owner releases the lease; the durable owner record is
    // removed only by terminal publication.
    drop(session);
    assert!(!session_is_live(&root, &owner).unwrap());
    assert_eq!(running(&persistence, run).owner, owner);
    persistence
        .finish_run(run, &plain_finish(RunOutcome::Interrupted), &mut [])
        .unwrap();
    assert_eq!(
        count(&persistence, "SELECT COUNT(*) FROM run_executions"),
        0
    );
    finished(&persistence, run);
}

// Test-ID: PR-TEST-0087
// Verifies: PR-REQ-0275
#[test]
fn run_artifacts_are_chunked_run_owned_and_expire_independently() {
    let (_temporary, root) = root();
    let persistence = PactrunPersistence::open(&root).unwrap();
    let revision = revision(&persistence);
    let view = instance(&persistence, &revision, "artifacts");
    let report = ManagedOutputIdentity::parse("report").unwrap();
    let empty = ManagedOutputIdentity::parse("empty").unwrap();
    let report_bytes = vec![0x5a_u8; MANAGED_INPUT_CHUNK_BYTES_V1 + 3];

    let run = admitted(&persistence, &view);
    let mut report_reader = Cursor::new(report_bytes.clone());
    let mut empty_reader = Cursor::new(Vec::new());
    let mut duplicate_reader = Cursor::new(b"dup".to_vec());
    let mut artifacts = [
        RunArtifactWrite {
            output: report.clone(),
            byte_len: report_bytes.len() as u64,
            reader: &mut report_reader,
        },
        RunArtifactWrite {
            output: empty.clone(),
            byte_len: 0,
            reader: &mut empty_reader,
        },
        RunArtifactWrite {
            output: report.clone(),
            byte_len: 3,
            reader: &mut duplicate_reader,
        },
    ];
    assert!(matches!(
        persistence.finish_run(run, &plain_finish(RunOutcome::Succeeded), &mut artifacts),
        Err(PersistenceError::InvalidRunArtifact(_))
    ));
    running(&persistence, run);
    assert_eq!(count(&persistence, "SELECT COUNT(*) FROM run_artifacts"), 0);

    let mut report_reader = Cursor::new(report_bytes.clone());
    let mut empty_reader = Cursor::new(Vec::new());
    let mut artifacts = [
        RunArtifactWrite {
            output: report.clone(),
            byte_len: report_bytes.len() as u64,
            reader: &mut report_reader,
        },
        RunArtifactWrite {
            output: empty.clone(),
            byte_len: 0,
            reader: &mut empty_reader,
        },
    ];
    persistence
        .finish_run(run, &plain_finish(RunOutcome::Succeeded), &mut artifacts)
        .unwrap();
    assert_eq!(
        finished(&persistence, run).artifacts,
        vec![
            RunArtifactSummary {
                edge: None,
                output: empty.clone(),
                byte_length: 0,
            },
            RunArtifactSummary {
                edge: None,
                output: report.clone(),
                byte_length: report_bytes.len() as u64,
            },
        ]
    );
    assert_eq!(
        count(&persistence, "SELECT COUNT(*) FROM run_artifact_chunks"),
        2
    );
    let mut output = Vec::new();
    persistence
        .open_run_artifact(run, &report, &mut output)
        .unwrap();
    assert_eq!(output, report_bytes);
    let mut output = Vec::new();
    persistence
        .open_run_artifact(run, &empty, &mut output)
        .unwrap();
    assert!(output.is_empty());
    assert!(matches!(
        persistence.open_run_artifact(
            run,
            &ManagedOutputIdentity::parse("missing").unwrap(),
            &mut Vec::new()
        ),
        Err(PersistenceError::InvalidRunArtifact(_))
    ));

    // A failed Run retains submitted files as diagnostic Artifacts.
    let failed = admitted(&persistence, &view);
    let mut diagnostic = Cursor::new(b"partial".to_vec());
    let mut artifacts = [RunArtifactWrite {
        output: report.clone(),
        byte_len: 7,
        reader: &mut diagnostic,
    }];
    persistence
        .finish_run(failed, &plain_finish(RunOutcome::Failed), &mut artifacts)
        .unwrap();
    let mut output = Vec::new();
    persistence
        .open_run_artifact(failed, &report, &mut output)
        .unwrap();
    assert_eq!(output, b"partial");

    // Independent expiry leaves the Run record intact.
    assert!(persistence.delete_run_artifact(failed, &report).unwrap());
    assert!(!persistence.delete_run_artifact(failed, &report).unwrap());
    let outcome = finished(&persistence, failed);
    assert_eq!(outcome.outcome, RunOutcome::Failed);
    assert!(outcome.artifacts.is_empty());

    // Chunk corruption is rejected on read; Run deletion cascades.
    execute(
        &persistence,
        "DELETE FROM run_artifact_chunks WHERE run_id=?1 AND chunk_index=0",
        &[run.as_bytes().as_slice()],
    )
    .unwrap();
    assert!(matches!(
        persistence.open_run_artifact(run, &report, &mut Vec::new()),
        Err(PersistenceError::CorruptRun(_))
    ));
    execute(
        &persistence,
        "DELETE FROM runs WHERE run_id=?1",
        &[run.as_bytes().as_slice()],
    )
    .unwrap();
    assert_eq!(count(&persistence, "SELECT COUNT(*) FROM run_artifacts"), 0);
    assert_eq!(
        count(&persistence, "SELECT COUNT(*) FROM run_artifact_chunks"),
        0
    );
    assert_eq!(persistence.load_run(run).unwrap(), None);
}
