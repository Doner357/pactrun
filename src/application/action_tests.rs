use super::*;
use std::{fs, io::Cursor};
use tempfile::TempDir;

fn roots() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/action-plan-tests");
    fs::create_dir_all(&parent).unwrap();
    let temporary = tempfile::Builder::new()
        .prefix("action-")
        .tempdir_in(parent)
        .unwrap();
    let storage = temporary.path().join("storage");
    let source = temporary.path().join("source");
    fs::create_dir(&storage).unwrap();
    fs::create_dir(storage.join("database")).unwrap();
    fs::create_dir(storage.join("runtime-content")).unwrap();
    fs::create_dir(storage.join("staging")).unwrap();
    fs::create_dir(&source).unwrap();
    (temporary, storage, source)
}

fn empty_metadata() -> RevisionMetadataMutationBatch {
    RevisionMetadataMutationBatch::new(Vec::<crate::domain::RevisionMetadataMutation>::new())
        .unwrap()
}

// Test-ID: PR-TEST-0115
// Verifies: PR-REQ-0008, PR-REQ-0038, PR-REQ-0096
#[test]
fn application_resolves_exact_action_and_compilation_is_persistently_read_only() {
    let (_temporary, storage, source) = roots();
    fs::write(source.join("tool.bin"), b"tool").unwrap();
    fs::write(source.join("config.bin"), b"secret config").unwrap();
    fs::write(
        source.join("pactrun.yaml"),
        r#"source_format: 1.0-alpha.2
package_id: 00000000000000000000000000000031
revision:
  inputs:
    - { id: config, required: true, protection: secret }
  actions:
    - id: inspect
      access: observe
      parameters:
        - { id: count, type: integer, sensitive: false }
      hook:
        protocol_version: 1.0-alpha.1
        launch: { kind: direct, executable: tool }
        args: [fixed]
        io: { terminal: output }
      outputs: [{ id: report }]
  migrations: []
runtime_content:
  files:
    - { id: tool, source: tool.bin, path: bin/tool, executable: true }
"#,
    )
    .unwrap();
    let application = PactrunApplication::open(&storage).unwrap();
    let installed = application
        .install_pack_source(&source, &empty_metadata())
        .unwrap();
    let name = InstanceName::parse("node").unwrap();
    assert!(matches!(
        application.resolve_action(
            &name,
            &ActionIdentity::parse("inspect").unwrap(),
            Vec::new(),
        ),
        Err(ApplicationError::ActionResolution(
            ActionResolutionError::InstanceNotFound
        ))
    ));
    let config = fs::File::open(source.join("config.bin")).unwrap();
    let instance = application
        .create_instance(
            name.clone(),
            installed.revision,
            vec![InputAcquisition {
                input_id: InputIdentity::parse("config").unwrap(),
                source: Box::new(config),
            }],
        )
        .unwrap();
    assert!(matches!(
        application.resolve_action(
            &name,
            &ActionIdentity::parse("missing").unwrap(),
            Vec::new(),
        ),
        Err(ApplicationError::ActionResolution(
            ActionResolutionError::ActionNotFound
        ))
    ));
    let database_path = storage.join("database/pactrun.sqlite3");
    let before_database = fs::read(&database_path).unwrap();
    let before_runtime = fs::read_dir(storage.join("runtime-content"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    let intent = application
        .resolve_action(
            &name,
            &ActionIdentity::parse("inspect").unwrap(),
            vec![RawParameterInput {
                id: crate::domain::ParameterIdentity::parse("count").unwrap(),
                text: "3".to_owned(),
                source: crate::domain::ParameterTextSource::Ordinary,
            }],
        )
        .unwrap();
    let plan = application.compile_action(&intent, &[]).unwrap();
    assert_eq!(plan.expected_state_version(), intent.expected_state_version);
    assert_eq!(plan.active_bindings().len(), 1);
    assert_eq!(plan.active_bindings()[0].input.as_str(), "config");
    assert_eq!(
        plan.active_bindings()[0].protection,
        crate::domain::ManagedInputProtection::Secret
    );
    assert_eq!(fs::read(&database_path).unwrap(), before_database);
    let after_runtime = fs::read_dir(storage.join("runtime-content"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(after_runtime, before_runtime);
    assert!(!storage.join("staging/workspace").exists());

    let read_only = PactrunApplication::open_read_only(&storage).unwrap();
    let (_, actions) = read_only
        .list_action_definitions(&name)
        .expect("read-only action inspection succeeds");
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].id.as_str(), "inspect");
    drop(read_only);
    assert_eq!(fs::read(&database_path).unwrap(), before_database);
    assert!(!storage.join("staging/workspace").exists());

    application
        .set_input(
            instance.id,
            InputIdentity::parse("config").unwrap(),
            instance.state_version,
            Box::new(Cursor::new(b"replacement config".to_vec())),
        )
        .unwrap();
    assert!(matches!(
        application.compile_action(&intent, &[]),
        Err(ApplicationError::PlanCompilation(
            PlanCompilationError::InconsistentFacts
        ))
    ));
}

// Test-ID: PR-TEST-0088
// Verifies: PR-REQ-0035, PR-REQ-0069
#[test]
fn resolve_manual_recovery_is_a_guarded_no_run_management_mutation() {
    use std::{
        sync::{Arc, mpsc},
        thread,
        time::Duration,
    };

    use crate::domain::{
        ActionRunIdentity, AdmissionFacts, CompiledHookLaunch, ExecutionOwnerSession, RunFinish,
        RunOutcome,
    };

    let (_temporary, storage, source) = roots();
    fs::write(source.join("config.bin"), b"config").unwrap();
    fs::write(source.join("tool.bin"), b"tool").unwrap();
    fs::write(
        source.join("pactrun.yaml"),
        r#"source_format: 1.0-alpha.2
package_id: 00000000000000000000000000000032
revision:
  inputs:
    - { id: config, required: true }
  actions:
    - id: deploy
      access: observe
      parameters: []
      hook:
        protocol_version: 1.0-alpha.1
        launch: { kind: direct, executable: tool }
        args: []
        io: { terminal: none }
      outputs: []
  migrations: []
runtime_content:
  files:
    - { id: tool, source: tool.bin, path: bin/tool, executable: true }
"#,
    )
    .unwrap();
    let application = Arc::new(PactrunApplication::open(&storage).unwrap());
    let installed = application
        .install_pack_source(&source, &empty_metadata())
        .unwrap();
    let instance = application
        .create_instance(
            InstanceName::parse("guarded").unwrap(),
            installed.revision.clone(),
            vec![InputAcquisition {
                input_id: InputIdentity::parse("config").unwrap(),
                source: Box::new(fs::File::open(source.join("config.bin")).unwrap()),
            }],
        )
        .unwrap();

    // Publish a ManualRecoveryRequired guard through the persistence
    // substrate: an admitted Run fails with open risk.
    let owner = application.execution_owner();
    assert!(ExecutionOwnerSession::is_session_name(owner.as_str()));
    let run = application
        .persistence
        .create_accepted_run(
            RunId::generate().unwrap(),
            instance.id,
            instance.state_version,
            &ActionRunIdentity {
                revision: installed.revision.clone(),
                action: ActionIdentity::parse("deploy").unwrap(),
            },
            &owner,
            &crate::persistence::UnconditionalAcceptance,
        )
        .unwrap();
    let bindings = application
        .persistence
        .observe_instance_compilation_state(instance.id)
        .unwrap()
        .unwrap()
        .active_bindings;
    let stored = application
        .persistence
        .load_revision(&installed.revision)
        .unwrap()
        .unwrap();
    let files = stored.content.runtime_content.files();
    let launch = CompiledHookLaunch::Direct {
        executable: files[0].clone(),
    };
    application
        .persistence
        .admit_run(
            run,
            &AdmissionFacts {
                service: None,
                expected_state_version: instance.state_version,
                active_bindings: &bindings,
                runtime_content: files,
                launch: &launch,
            },
            &|_| Ok(()),
            false,
        )
        .unwrap()
        .unwrap();
    application.persistence.open_recovery_risk(run).unwrap();
    let receipt = application
        .persistence
        .finish_run(
            run,
            &RunFinish {
                outcome: RunOutcome::Failed,
                primary_failure: None,
                secondary_failures: Vec::new(),
                hook_completion: None,
            },
            &mut [],
        )
        .unwrap();
    let guarded = receipt.published_state_version.unwrap();
    assert!(
        application
            .persistence
            .load_instance_recovery_guard(instance.id)
            .unwrap()
            .is_some()
    );
    let runs_before = application
        .persistence
        .list_runs(instance.id)
        .unwrap()
        .len();

    // A stale token is rejected before anything changes.
    assert!(matches!(
        application.resolve_manual_recovery(instance.id, instance.state_version),
        Err(ApplicationError::Persistence(
            PersistenceError::StaleInstanceState
        ))
    ));
    assert!(
        application
            .persistence
            .load_instance_recovery_guard(instance.id)
            .unwrap()
            .is_some()
    );

    // The mutation guard serializes ResolveManualRecovery with other
    // management mutations on the same Instance.
    let held = application.mutation_lock(instance.id).unwrap();
    let holder = held.lock().unwrap();
    let (started, observe) = mpsc::channel();
    let resolver = {
        let application = Arc::clone(&application);
        thread::spawn(move || {
            started.send(()).unwrap();
            application.resolve_manual_recovery(instance.id, guarded)
        })
    };
    observe.recv().unwrap();
    thread::sleep(Duration::from_millis(200));
    assert!(
        application
            .persistence
            .load_instance_recovery_guard(instance.id)
            .unwrap()
            .is_some(),
        "resolution must wait for the mutation guard"
    );
    drop(holder);
    let resolved = resolver.join().unwrap().unwrap();
    assert_ne!(resolved, guarded);
    assert_eq!(
        application
            .load_instance(instance.id)
            .unwrap()
            .unwrap()
            .state_version,
        resolved
    );
    assert!(
        application
            .persistence
            .load_instance_recovery_guard(instance.id)
            .unwrap()
            .is_none()
    );

    // No Run, Plan, or Hook was involved: the Run history is unchanged and
    // no staging workspace appeared.
    assert_eq!(
        application
            .persistence
            .list_runs(instance.id)
            .unwrap()
            .len(),
        runs_before
    );
    assert!(!storage.join("staging/workspace").exists());
    assert!(matches!(
        application.resolve_manual_recovery(instance.id, resolved),
        Err(ApplicationError::Persistence(
            PersistenceError::MissingRecoveryGuard
        ))
    ));
}
