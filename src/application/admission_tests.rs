use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

use tempfile::TempDir;

use super::*;
use crate::{
    domain::{
        ActionRunBoundary, AdmissionRefusal, RecoveryRiskState, RunFailedStep, RunFinish, RunId,
        RunOutcome, RunPhase, RunState, RunView,
    },
    executor::{AdmissionOptions, AdmittedExecution, ExecutorError},
    persistence::FaultPoint,
};

const WORKER_TEST: &str = "application::admission_tests::admission_worker";

struct Fixture {
    _temporary: TempDir,
    storage: PathBuf,
    first: PathBuf,
    second: PathBuf,
    source: PathBuf,
}

fn launcher_command() -> &'static str {
    if cfg!(windows) {
        "runtime.exe"
    } else {
        "runtime"
    }
}

#[cfg(unix)]
fn write_eligible_launcher(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, b"not a prevalidated executable image").unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(windows)]
fn write_eligible_launcher(path: &Path) {
    fs::write(path, b"not a prevalidated executable image").unwrap();
}

fn fixture() -> Fixture {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/admission-tests");
    fs::create_dir_all(&parent).unwrap();
    let temporary = tempfile::Builder::new()
        .prefix("admission-")
        .tempdir_in(parent)
        .unwrap();
    let storage = temporary.path().join("storage");
    let source = temporary.path().join("source");
    let first = temporary.path().join("first");
    let second = temporary.path().join("second");
    for directory in [&storage, &source, &first, &second] {
        fs::create_dir(directory).unwrap();
    }
    for child in ["database", "runtime-content", "staging"] {
        fs::create_dir(storage.join(child)).unwrap();
    }
    write_eligible_launcher(&first.join(launcher_command()));
    write_eligible_launcher(&second.join(launcher_command()));
    fs::write(source.join("config.bin"), b"config bytes").unwrap();
    fs::write(source.join("tool.bin"), b"tool image").unwrap();
    fs::write(source.join("hook.txt"), b"hook script").unwrap();
    fs::write(
            source.join("pactrun.yaml"),
            format!(
                r#"source_format: 1.0-alpha.2
package_id: 00000000000000000000000000000033
revision:
  inputs:
    - {{ id: config, required: true }}
  actions:
    - id: inspect
      access: observe
      parameters: []
      hook:
        protocol_version: 1.0-alpha.1
        launch: {{ kind: direct, executable: tool }}
        args: []
        io: {{ terminal: output }}
      outputs: []
    - id: deploy
      access: mutate
      parameters: []
      hook:
        protocol_version: 1.0-alpha.1
        launch: {{ kind: interpreter, command: {command}, interpreter_args: [--strict], script: hook }}
        args: []
        io: {{ terminal: none }}
      outputs: []
    - id: audit
      access: observe
      parameters: []
      hook:
        protocol_version: 1.0-alpha.1
        launch: {{ kind: interpreter, command: {command}, interpreter_args: [], script: hook }}
        args: []
        io: {{ terminal: none }}
      outputs: []
  migrations: []
runtime_content:
  files:
    - {{ id: tool, source: tool.bin, path: bin/tool, executable: true }}
    - {{ id: hook, source: hook.txt, path: bin/hook }}
"#,
                command = launcher_command()
            ),
        )
        .unwrap();
    Fixture {
        _temporary: temporary,
        storage,
        first,
        second,
        source,
    }
}

fn metadata() -> RevisionMetadataMutationBatch {
    RevisionMetadataMutationBatch::new(Vec::<crate::domain::RevisionMetadataMutation>::new())
        .unwrap()
}

fn install(application: &PactrunApplication, fixture: &Fixture) -> RevisionIdentity {
    application
        .install_pack_source(&fixture.source, &metadata())
        .unwrap()
        .revision
}

fn create(
    application: &PactrunApplication,
    revision: &RevisionIdentity,
    name: &str,
    with_config: bool,
) -> InstanceView {
    let mut initial = Vec::new();
    if with_config {
        initial.push(InputAcquisition {
            input_id: InputIdentity::parse("config").unwrap(),
            source: Box::new(Cursor::new(b"config bytes".to_vec())),
        });
    }
    application
        .create_instance(
            InstanceName::parse(name).unwrap(),
            revision.clone(),
            initial,
        )
        .unwrap()
}

fn plan(
    application: &PactrunApplication,
    fixture: &Fixture,
    instance: &str,
    action: &str,
) -> ActionExecutionPlan {
    let intent = application
        .resolve_action(
            &InstanceName::parse(instance).unwrap(),
            &ActionIdentity::parse(action).unwrap(),
            Vec::new(),
        )
        .unwrap();
    application
        .compile_action(&intent, &[fixture.first.clone(), fixture.second.clone()])
        .unwrap()
}

fn admit(
    application: &PactrunApplication,
    plan: &ActionExecutionPlan,
    recovery_override: bool,
) -> Result<AdmittedExecution, ApplicationError> {
    application.accept_and_admit_action(plan, AdmissionOptions { recovery_override })
}

fn expect_refusal(
    result: Result<AdmittedExecution, ApplicationError>,
) -> (RunId, AdmissionRefusal) {
    match result {
        Err(ApplicationError::Execution(ExecutorError::Refused { run, refusal })) => (run, refusal),
        other => panic!("expected an Admission refusal, found {other:?}"),
    }
}

fn view(application: &PactrunApplication, run: RunId) -> RunView {
    application.persistence.load_run(run).unwrap().unwrap()
}

/// Counts through an independent read connection so the assertion observes
/// only committed state.
fn count(fixture: &Fixture, sql: &str) -> i64 {
    let database =
        rusqlite::Connection::open(fixture.storage.join("database/pactrun.sqlite3")).unwrap();
    database.query_row(sql, [], |row| row.get(0)).unwrap()
}

fn token(application: &PactrunApplication, instance: InstanceId) -> InstanceStateVersion {
    application
        .load_instance(instance)
        .unwrap()
        .unwrap()
        .state_version
}

fn replace_config(application: &PactrunApplication, instance: &InstanceView) {
    let current = token(application, instance.id);
    application
        .set_input(
            instance.id,
            InputIdentity::parse("config").unwrap(),
            current,
            Box::new(Cursor::new(b"replacement".to_vec())),
        )
        .unwrap();
}

fn assert_refused_run(
    application: &PactrunApplication,
    run: RunId,
    code: &str,
    boundary: ActionRunBoundary,
) {
    match view(application, run).state {
        RunState::Finished(outcome) => {
            assert_eq!(outcome.outcome, RunOutcome::Failed);
            assert_eq!(outcome.boundary, boundary);
            assert_eq!(outcome.terminal_risk, RecoveryRiskState::Clear);
            let primary = outcome
                .primary_failure
                .expect("refusal records a primary failure");
            assert_eq!(primary.failure.error.owner(), "admission");
            assert_eq!(primary.failure.error.code(), code);
            assert_eq!(primary.step, RunFailedStep::Admission);
        }
        other => panic!("expected a Finished refusal, found {other:?}"),
    }
}

fn finish(application: &PactrunApplication, run: RunId, outcome: RunOutcome) {
    application
        .persistence
        .finish_run(
            run,
            &RunFinish {
                outcome,
                primary_failure: None,
                secondary_failures: Vec::new(),
                hook_completion: None,
            },
            &mut [],
        )
        .unwrap();
}

/// Runs `accept_and_admit_action` in a separate process. The worker opens
/// its own application (own staging session and owner), resolves and
/// compiles the Plan itself because Plans are not serializable, and writes
/// `admitted <run> <owner>` or `refused <code> <run> <owner>` to `result`.
fn run_worker(
    fixture: &Fixture,
    operation: &str,
    instance: &str,
    action: &str,
    fault: Option<FaultPoint>,
    result: &Path,
) -> std::process::Child {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg(WORKER_TEST)
        .arg("--nocapture")
        .env("PACTRUN_ADMISSION_TEST_WORKER", operation)
        .env("PACTRUN_ADMISSION_TEST_STORAGE", &fixture.storage)
        .env("PACTRUN_ADMISSION_TEST_INSTANCE", instance)
        .env("PACTRUN_ADMISSION_TEST_ACTION", action)
        .env("PACTRUN_ADMISSION_TEST_FIRST", &fixture.first)
        .env("PACTRUN_ADMISSION_TEST_SECOND", &fixture.second)
        .env("PACTRUN_ADMISSION_TEST_RESULT", result)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(fault) = fault {
        command.env("PACTRUN_RUN_TEST_FAULT", fault.name());
    }
    command.spawn().unwrap()
}

fn wait_worker(child: std::process::Child, expect_success: bool) {
    let output = child.wait_with_output().unwrap();
    if output.status.success() != expect_success {
        panic!(
            "admission worker exited with {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn admission_worker() {
    let Some(operation) = std::env::var_os("PACTRUN_ADMISSION_TEST_WORKER") else {
        return;
    };
    let storage = PathBuf::from(std::env::var_os("PACTRUN_ADMISSION_TEST_STORAGE").unwrap());
    let instance = std::env::var("PACTRUN_ADMISSION_TEST_INSTANCE").unwrap();
    let action = std::env::var("PACTRUN_ADMISSION_TEST_ACTION").unwrap();
    let first = PathBuf::from(std::env::var_os("PACTRUN_ADMISSION_TEST_FIRST").unwrap());
    let second = PathBuf::from(std::env::var_os("PACTRUN_ADMISSION_TEST_SECOND").unwrap());
    let result = PathBuf::from(std::env::var_os("PACTRUN_ADMISSION_TEST_RESULT").unwrap());
    let application = PactrunApplication::open(&storage).unwrap();
    let owner = application.execution_owner();
    let instance_name = InstanceName::parse(instance).unwrap();
    let intent = application
        .resolve_action(
            &instance_name,
            &ActionIdentity::parse(action).unwrap(),
            Vec::new(),
        )
        .unwrap();
    let plan = application
        .compile_action(&intent, &[first, second])
        .unwrap();
    if operation.to_string_lossy() == "admit_stale" {
        let instance = application.load_instance(plan.instance()).unwrap().unwrap();
        application
            .set_input(
                instance.id,
                InputIdentity::parse("config").unwrap(),
                instance.state_version,
                Box::new(Cursor::new(b"worker replacement".to_vec())),
            )
            .unwrap();
    }
    let report = match application.accept_and_admit_action(&plan, AdmissionOptions::default()) {
        Ok(admitted) => format!("admitted {} {}", admitted.run(), owner.as_str()),
        Err(ApplicationError::Execution(ExecutorError::Refused { run, refusal })) => {
            format!(
                "refused {} {} {}",
                refusal.error_ref().code(),
                run,
                owner.as_str()
            )
        }
        Err(error) => panic!("unexpected admission error {error}"),
    };
    fs::write(result, report).unwrap();
}

fn runs_for(application: &PactrunApplication, instance: InstanceId) -> Vec<RunView> {
    application
        .persistence
        .list_runs(instance)
        .unwrap()
        .into_iter()
        .map(|summary| view(application, summary.id))
        .collect()
}

// Test-ID: PR-TEST-0089
// Verifies: PR-REQ-0007, PR-REQ-0039, PR-REQ-0041, PR-REQ-0042, PR-REQ-0049
#[test]
fn run_acceptance_precedes_admission_and_both_are_durable_boundaries() {
    let fixture = fixture();
    let application = PactrunApplication::open(&fixture.storage).unwrap();
    let revision = install(&application, &fixture);
    let instance = create(&application, &revision, "accept", true);
    let database_path = fixture.storage.join("database/pactrun.sqlite3");

    // Compilation is side-effect free: no Run, pin, or guard, identical
    // database bytes, and the mutation guard is free afterwards.
    let before = fs::read(&database_path).unwrap();
    let compiled = plan(&application, &fixture, "accept", "inspect");
    assert_eq!(fs::read(&database_path).unwrap(), before);
    assert_eq!(count(&fixture, "SELECT COUNT(*) FROM runs"), 0);
    assert_eq!(count(&fixture, "SELECT COUNT(*) FROM run_revision_pins"), 0);
    assert!(
        application
            .mutation_lock(instance.id)
            .unwrap()
            .try_lock()
            .is_ok()
    );

    // Acceptance creates the durable Run; Admission pins it in the same
    // process's ownership.
    let admitted = admit(&application, &compiled, false).unwrap();
    assert_eq!(admitted.plan(), &compiled);
    let admitted_view = view(&application, admitted.run());
    assert_eq!(admitted_view.instance, instance.id);
    assert_eq!(admitted_view.accepted_state_version, instance.state_version);
    assert_eq!(admitted_view.action.revision, revision);
    assert_eq!(admitted_view.action.action.as_str(), "inspect");
    match admitted_view.state {
        RunState::Running(execution) => {
            assert_eq!(execution.boundary, ActionRunBoundary::Admitted);
            assert_eq!(execution.owner, application.execution_owner());
        }
        other => panic!("expected an admitted Run, found {other:?}"),
    }
    assert_eq!(token(&application, instance.id), instance.state_version);

    // A refusal is recorded in the Run created at acceptance, never lost.
    let stale = plan(&application, &fixture, "accept", "inspect");
    replace_config(&application, &instance);
    let (refused, refusal) = expect_refusal(admit(&application, &stale, false));
    assert!(matches!(refusal, AdmissionRefusal::PlanInvalidated(_)));
    assert_refused_run(
        &application,
        refused,
        "plan_invalidated",
        ActionRunBoundary::Accepted,
    );
    assert_eq!(count(&fixture, "SELECT COUNT(*) FROM runs"), 2);

    // Crash injection around both durable boundaries.
    let cases: [(&str, &str, FaultPoint, RunPhase, ActionRunBoundary, bool); 4] = [
        (
            "crash-accept-before",
            "admit",
            FaultPoint::BeforeRunAcceptCommit,
            RunPhase::Running,
            ActionRunBoundary::Accepted,
            false,
        ),
        (
            "crash-accept-after",
            "admit",
            FaultPoint::AfterRunAcceptCommit,
            RunPhase::Running,
            ActionRunBoundary::Accepted,
            true,
        ),
        (
            "crash-admit-before",
            "admit",
            FaultPoint::BeforeRunAdmitCommit,
            RunPhase::Running,
            ActionRunBoundary::Accepted,
            true,
        ),
        (
            "crash-admit-after",
            "admit",
            FaultPoint::AfterRunAdmitCommit,
            RunPhase::Running,
            ActionRunBoundary::Admitted,
            true,
        ),
    ];
    for (name, operation, fault, phase, boundary, run_exists) in cases {
        let crashed = create(&application, &revision, name, true);
        let result = fixture._temporary.path().join(format!("{name}.result"));
        wait_worker(
            run_worker(&fixture, operation, name, "inspect", Some(fault), &result),
            false,
        );
        assert!(!result.exists());
        let runs = runs_for(&application, crashed.id);
        if !run_exists {
            assert!(
                runs.is_empty(),
                "{name}: no Run may exist before the accept commit"
            );
            continue;
        }
        assert_eq!(runs.len(), 1, "{name}");
        assert_eq!(runs[0].state.phase(), phase, "{name}");
        match &runs[0].state {
            RunState::Running(execution) => {
                assert_eq!(execution.boundary, boundary, "{name}");
                assert_ne!(execution.owner, application.execution_owner());
            }
            other => panic!("{name}: unexpected state {other:?}"),
        }
    }

    // The refusal outcome commits atomically with the decision.
    let refused_before = create(&application, &revision, "crash-refuse-before", true);
    let result = fixture._temporary.path().join("refuse-before.result");
    wait_worker(
        run_worker(
            &fixture,
            "admit_stale",
            "crash-refuse-before",
            "inspect",
            Some(FaultPoint::BeforeRunAdmitCommit),
            &result,
        ),
        false,
    );
    let runs = runs_for(&application, refused_before.id);
    assert_eq!(runs.len(), 1);
    assert!(
        matches!(&runs[0].state, RunState::Running(execution) if execution.boundary == ActionRunBoundary::Accepted)
    );
    let refused_after = create(&application, &revision, "crash-refuse-after", true);
    let result = fixture._temporary.path().join("refuse-after.result");
    wait_worker(
        run_worker(
            &fixture,
            "admit_stale",
            "crash-refuse-after",
            "inspect",
            Some(FaultPoint::AfterRunAdmitCommit),
            &result,
        ),
        false,
    );
    let runs = runs_for(&application, refused_after.id);
    assert_eq!(runs.len(), 1);
    assert_refused_run(
        &application,
        runs[0].id,
        "plan_invalidated",
        ActionRunBoundary::Accepted,
    );
}

// Test-ID: PR-TEST-0090
// Verifies: PR-REQ-0008, PR-REQ-0043, PR-REQ-0091, PR-REQ-0194
// Supporting coverage for the Action clause of PR-REQ-0091 (readiness at
// Admission); the requirement itself remains Pending until its Capture,
// Migration, Cleanup, and Restore clauses are implemented.
#[test]
fn admission_revalidates_every_compilation_fact_without_recompiling() {
    let fixture = fixture();
    let application = PactrunApplication::open(&fixture.storage).unwrap();
    let revision = install(&application, &fixture);
    let instance = create(&application, &revision, "facts", true);

    // Stale state token.
    let stale = plan(&application, &fixture, "facts", "inspect");
    replace_config(&application, &instance);
    let (run, refusal) = expect_refusal(admit(&application, &stale, false));
    assert!(matches!(refusal, AdmissionRefusal::PlanInvalidated(_)));
    assert_refused_run(
        &application,
        run,
        "plan_invalidated",
        ActionRunBoundary::Accepted,
    );
    let recorded = view(&application, run);
    assert_eq!(recorded.action.revision, *stale.active_revision());
    assert_eq!(recorded.action.action, *stale.action());

    // Readiness: a required Input is unbound.
    create(&application, &revision, "unready", false);
    let unready = plan(&application, &fixture, "unready", "inspect");
    let (run, refusal) = expect_refusal(admit(&application, &unready, false));
    match refusal {
        AdmissionRefusal::MissingRequiredInputs(ids) => {
            assert_eq!(ids, vec![InputIdentity::parse("config").unwrap()])
        }
        other => panic!("unexpected refusal {other:?}"),
    }
    assert_refused_run(
        &application,
        run,
        "plan_invalidated",
        ActionRunBoundary::Accepted,
    );

    // Runtime content availability.
    let fresh = plan(&application, &fixture, "facts", "inspect");
    let runtime_root = fixture.storage.join("runtime-content");
    let tool_blob = fresh.runtime_content()[0].blob_digest.as_str()["sha256:".len()..].to_owned();
    let blob_path = runtime_root.join(&tool_blob);
    let parked = runtime_root.join("parked");
    fs::rename(&blob_path, &parked).unwrap();
    let (run, refusal) = expect_refusal(admit(&application, &fresh, false));
    assert!(matches!(refusal, AdmissionRefusal::PlanInvalidated(_)));
    assert_refused_run(
        &application,
        run,
        "plan_invalidated",
        ActionRunBoundary::Accepted,
    );
    fs::rename(&parked, &blob_path).unwrap();

    // Interpreter launcher re-selection must reproduce the exact bound path.
    let audit = plan(&application, &fixture, "facts", "audit");
    let first_candidate = fixture.first.join(launcher_command());
    fs::remove_file(&first_candidate).unwrap();
    let (run, refusal) = expect_refusal(admit(&application, &audit, false));
    assert!(matches!(refusal, AdmissionRefusal::PlanInvalidated(_)));
    assert_refused_run(
        &application,
        run,
        "plan_invalidated",
        ActionRunBoundary::Accepted,
    );
    fs::create_dir(&first_candidate).unwrap();
    let (run, _) = expect_refusal(admit(&application, &audit, false));
    assert_refused_run(
        &application,
        run,
        "plan_invalidated",
        ActionRunBoundary::Accepted,
    );
    // A Direct launch is unaffected by launcher-search changes.
    let direct = admit(&application, &fresh, false).unwrap();
    finish(&application, direct.run(), RunOutcome::Interrupted);
    fs::remove_dir(&first_candidate).unwrap();
    write_eligible_launcher(&first_candidate);
    let admitted = admit(&application, &audit, false).unwrap();
    assert_eq!(admitted.plan(), &audit);
    let recorded = view(&application, admitted.run());
    assert_eq!(recorded.action.revision, *audit.active_revision());
    assert_eq!(recorded.action.action, *audit.action());
}

// Test-ID: PR-TEST-0091
// Verifies: PR-REQ-0067, PR-REQ-0068, PR-REQ-0279
#[test]
fn trust_guard_blocks_ordinary_admission_and_override_bypasses_only_the_guard() {
    let fixture = fixture();
    let application = PactrunApplication::open(&fixture.storage).unwrap();
    let revision = install(&application, &fixture);
    let instance = create(&application, &revision, "guarded", true);
    let pre_guard = plan(&application, &fixture, "guarded", "inspect");

    // Publish the guard through an admitted Mutate Run that fails with open
    // risk.
    let mutate = admit(
        &application,
        &plan(&application, &fixture, "guarded", "deploy"),
        false,
    )
    .unwrap();
    application
        .persistence
        .open_recovery_risk(mutate.run())
        .unwrap();
    finish(&application, mutate.run(), RunOutcome::Failed);
    let guard = application
        .persistence
        .load_instance_recovery_guard(instance.id)
        .unwrap()
        .expect("open-risk failure publishes the guard");
    let guarded_token = token(&application, instance.id);
    assert_ne!(guarded_token, instance.state_version);

    // Ordinary admission is blocked; the guard is unchanged.
    let ordinary = plan(&application, &fixture, "guarded", "inspect");
    let (run, refusal) = expect_refusal(admit(&application, &ordinary, false));
    assert_eq!(refusal, AdmissionRefusal::RecoveryGuardActive);
    assert_refused_run(
        &application,
        run,
        "recovery_guard_active",
        ActionRunBoundary::Accepted,
    );
    assert_eq!(
        application
            .persistence
            .load_instance_recovery_guard(instance.id)
            .unwrap()
            .unwrap(),
        guard
    );
    // Precedence: the guard is reported before staleness.
    let (run, refusal) = expect_refusal(admit(&application, &pre_guard, false));
    assert_eq!(refusal, AdmissionRefusal::RecoveryGuardActive);
    assert_refused_run(
        &application,
        run,
        "recovery_guard_active",
        ActionRunBoundary::Accepted,
    );

    // Legal Input management continues while guarded.
    replace_config(&application, &instance);
    assert_ne!(token(&application, instance.id), guarded_token);

    // The override admits exactly one execution and leaves the guard.
    let overridden = admit(
        &application,
        &plan(&application, &fixture, "guarded", "inspect"),
        true,
    )
    .unwrap();
    assert!(matches!(
        view(&application, overridden.run()).state,
        RunState::Running(execution) if execution.boundary == ActionRunBoundary::Admitted
    ));
    assert!(
        application
            .persistence
            .load_instance_recovery_guard(instance.id)
            .unwrap()
            .is_some()
    );

    // The override bypasses neither staleness nor a Mutate conflict.
    let (run, refusal) = expect_refusal(admit(&application, &pre_guard, true));
    assert!(matches!(refusal, AdmissionRefusal::PlanInvalidated(_)));
    assert_refused_run(
        &application,
        run,
        "plan_invalidated",
        ActionRunBoundary::Accepted,
    );
    let mutate = admit(
        &application,
        &plan(&application, &fixture, "guarded", "deploy"),
        true,
    )
    .unwrap();
    let competing = plan(&application, &fixture, "guarded", "deploy");
    let (run, refusal) = expect_refusal(admit(&application, &competing, true));
    assert_eq!(refusal, AdmissionRefusal::MutationConflict(mutate.run()));
    assert_refused_run(
        &application,
        run,
        "mutation_conflict",
        ActionRunBoundary::Accepted,
    );
    // Precedence: staleness is reported before a conflict.
    replace_config(&application, &instance);
    let (run, refusal) = expect_refusal(admit(&application, &competing, true));
    assert!(matches!(refusal, AdmissionRefusal::PlanInvalidated(_)));
    assert_refused_run(
        &application,
        run,
        "plan_invalidated",
        ActionRunBoundary::Accepted,
    );
}

// Test-ID: PR-TEST-0092
// Verifies: PR-REQ-0045, PR-REQ-0278
#[test]
fn mutate_admission_is_exclusive_across_threads_and_processes_while_observe_coexists() {
    let fixture = fixture();
    let application = Arc::new(PactrunApplication::open(&fixture.storage).unwrap());
    let revision = install(&application, &fixture);
    let instance = create(&application, &revision, "exclusive", true);

    // Running + Admitted Mutate excludes Mutate but not Observe.
    let first = admit(
        &application,
        &plan(&application, &fixture, "exclusive", "deploy"),
        false,
    )
    .unwrap();
    let second = plan(&application, &fixture, "exclusive", "deploy");
    let (run, refusal) = expect_refusal(admit(&application, &second, false));
    assert_eq!(refusal, AdmissionRefusal::MutationConflict(first.run()));
    assert_refused_run(
        &application,
        run,
        "mutation_conflict",
        ActionRunBoundary::Accepted,
    );
    let observe = admit(
        &application,
        &plan(&application, &fixture, "exclusive", "inspect"),
        false,
    )
    .unwrap();
    let another_observe = admit(
        &application,
        &plan(&application, &fixture, "exclusive", "audit"),
        false,
    )
    .unwrap();

    // Management mutation stays admissible; the pinned payload survives.
    let payloads_before = count(&fixture, "SELECT COUNT(*) FROM managed_input_payloads");
    replace_config(&application, &instance);
    assert_eq!(
        count(&fixture, "SELECT COUNT(*) FROM managed_input_payloads"),
        payloads_before + 1
    );
    assert_eq!(
        count(
            &fixture,
            "SELECT COUNT(*) FROM run_payload_pins p \
                 JOIN managed_input_payloads m \
                 ON m.instance_id = p.instance_id AND m.payload_id = p.payload_id",
        ),
        3
    );

    // Finishing frees the slot; an Observe Run never occupied it.
    finish(&application, first.run(), RunOutcome::Interrupted);
    let third = admit(
        &application,
        &plan(&application, &fixture, "exclusive", "deploy"),
        false,
    )
    .unwrap();
    finish(&application, third.run(), RunOutcome::Cancelled);
    finish(&application, observe.run(), RunOutcome::Succeeded);
    finish(&application, another_observe.run(), RunOutcome::Succeeded);

    // An Accepted-but-not-admitted Mutate Run occupies nothing.
    let result = fixture._temporary.path().join("accepted-only.result");
    wait_worker(
        run_worker(
            &fixture,
            "admit",
            "exclusive",
            "deploy",
            Some(FaultPoint::BeforeRunAdmitCommit),
            &result,
        ),
        false,
    );
    let accepted_only = runs_for(&application, instance.id)
            .into_iter()
            .filter(|run| {
                matches!(&run.state, RunState::Running(execution) if execution.boundary == ActionRunBoundary::Accepted)
            })
            .count();
    assert_eq!(accepted_only, 1);
    let fourth = admit(
        &application,
        &plan(&application, &fixture, "exclusive", "deploy"),
        false,
    )
    .unwrap();
    finish(&application, fourth.run(), RunOutcome::Interrupted);

    // The same in-process guard serializes admission with management
    // mutations: a held guard delays acceptance itself.
    let held = application.mutation_lock(instance.id).unwrap();
    let holder = held.lock().unwrap();
    let runs_before = count(&fixture, "SELECT COUNT(*) FROM runs");
    let (started, observe_start) = mpsc::channel();
    let blocked = {
        let application = Arc::clone(&application);
        let plan = plan(&application, &fixture, "exclusive", "inspect");
        thread::spawn(move || {
            started.send(()).unwrap();
            admit(&application, &plan, false)
        })
    };
    observe_start.recv().unwrap();
    thread::sleep(Duration::from_millis(200));
    assert_eq!(count(&fixture, "SELECT COUNT(*) FROM runs"), runs_before);
    drop(holder);
    let late = blocked.join().unwrap().unwrap();
    finish(&application, late.run(), RunOutcome::Succeeded);

    // Two threads on one application admit exactly one Mutate.
    let plans = [
        plan(&application, &fixture, "exclusive", "deploy"),
        plan(&application, &fixture, "exclusive", "deploy"),
    ];
    let racers = plans
        .into_iter()
        .map(|plan| {
            let application = Arc::clone(&application);
            thread::spawn(move || admit(&application, &plan, false))
        })
        .collect::<Vec<_>>();
    let outcomes = racers
        .into_iter()
        .map(|racer| racer.join().unwrap())
        .collect::<Vec<_>>();
    let winners = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
    assert_eq!(winners, 1);
    for outcome in outcomes {
        match outcome {
            Ok(admitted) => finish(&application, admitted.run(), RunOutcome::Interrupted),
            other => {
                let (_, refusal) = expect_refusal(other);
                assert!(matches!(refusal, AdmissionRefusal::MutationConflict(_)));
            }
        }
    }

    // Two processes race for the same Mutate slot; only the database
    // predicate decides, and exactly one durable winner exists.
    let race = create(&application, &revision, "race", true);
    let first_result = fixture._temporary.path().join("race-a.result");
    let second_result = fixture._temporary.path().join("race-b.result");
    let racer_a = run_worker(&fixture, "admit", "race", "deploy", None, &first_result);
    let racer_b = run_worker(&fixture, "admit", "race", "deploy", None, &second_result);
    wait_worker(racer_a, true);
    wait_worker(racer_b, true);
    let reports = [
        fs::read_to_string(&first_result).unwrap(),
        fs::read_to_string(&second_result).unwrap(),
    ];
    let admitted_reports = reports
        .iter()
        .filter(|report| report.starts_with("admitted "))
        .collect::<Vec<_>>();
    let refused_reports = reports
        .iter()
        .filter(|report| report.starts_with("refused mutation_conflict "))
        .collect::<Vec<_>>();
    assert_eq!(admitted_reports.len(), 1, "{reports:?}");
    assert_eq!(refused_reports.len(), 1, "{reports:?}");
    let winner_owner = admitted_reports[0].split(' ').nth(2).unwrap();
    drop(application);
    let reopened = PactrunApplication::open(&fixture.storage).unwrap();
    let runs = runs_for(&reopened, race.id);
    assert_eq!(runs.len(), 2);
    let admitted_runs = runs
            .iter()
            .filter(|run| {
                matches!(&run.state, RunState::Running(execution) if execution.boundary == ActionRunBoundary::Admitted)
            })
            .collect::<Vec<_>>();
    assert_eq!(admitted_runs.len(), 1);
    match &admitted_runs[0].state {
        RunState::Running(execution) => assert_eq!(execution.owner.as_str(), winner_owner),
        other => panic!("unexpected state {other:?}"),
    }
    assert_eq!(count(&fixture, "SELECT COUNT(*) FROM run_revision_pins"), 1);
    let loser = runs
        .iter()
        .find(|run| run.id != admitted_runs[0].id)
        .unwrap();
    assert_refused_run(
        &reopened,
        loser.id,
        "mutation_conflict",
        ActionRunBoundary::Accepted,
    );
}
