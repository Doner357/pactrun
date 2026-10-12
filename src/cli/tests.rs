use super::*;
use tempfile::TempDir;

include!("create_restore_tests.rs");
include!("names_tests.rs");
include!("json_tests.rs");
include!("machine_delivery_tests.rs");
include!("short_id_tests.rs");

// Test-ID: PR-TEST-0593
// Verifies: PR-REQ-0038, PR-REQ-0116, PR-REQ-0119
#[test]
fn generic_confirmation_never_grants_disclosure_declassification_or_recovery_authority() {
    let (temporary, _, _) = cli_roots();
    let missing = temporary.path().join("must-not-open");
    let id = "00000000000000000000000000000001";
    let revision = "exact:00000000000000000000000000000001/sha256:0000000000000000000000000000000000000000000000000000000000000001";
    let export = temporary.path().join("must-not-export");
    for flag in ["--force", "--yes"] {
        for mut args in [
            vec!["input", "export", "node", "secret", "--output", "-"],
            vec![
                "snapshot",
                "export",
                id,
                "--output",
                export.to_str().unwrap(),
            ],
            vec!["instance", "migrate", "node", "--to", revision],
            vec!["invoke", "node", "recover"],
            vec!["instance", "abandon", "node"],
        ] {
            assert!(
                parse_command(args.iter().map(OsString::from).collect()).is_ok(),
                "invalid test command: {args:?}"
            );
            args.push(flag);
            let mut output = Vec::new();
            let mut errors = Vec::new();
            let code = run(
                args.iter().map(OsString::from).collect(),
                Some(missing.as_os_str().into()),
                &mut io::empty(),
                &mut output,
                &mut errors,
            );
            assert_eq!(code, 2, "{args:?}: {}", String::from_utf8_lossy(&errors));
            assert!(!missing.exists());
            assert!(!export.exists());
            assert!(output.is_empty());
        }
    }
    for flag in ["--secret", "--normal", "--protection"] {
        let mut output = Vec::new();
        let mut errors = Vec::new();
        assert_eq!(
            run(
                ["input", "set", "node", "secret", "--stdin", flag]
                    .map(OsString::from)
                    .to_vec(),
                Some(missing.as_os_str().into()),
                &mut io::empty(),
                &mut output,
                &mut errors
            ),
            2
        );
        assert!(!missing.exists());
    }
}

// Test-ID: PR-TEST-0599
// Verifies: PR-REQ-0031, PR-REQ-0032, PR-REQ-0115, PR-REQ-0265
#[test]
fn initial_input_acquisition_failure_is_atomic_at_the_public_create_boundary() {
    let (temporary, root, source) = cli_roots();
    let manifest = source.join("pactrun.yaml");
    let text = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        text.replace("    - id: config", "    - id: config\n      required: true"),
    )
    .unwrap();
    let revision = json_install(&root, &source);
    let good = temporary.path().join("opaque.json");
    fs::write(&good, [0, 255, 13, 10]).unwrap();
    let missing = temporary.path().join("missing-secret");
    let args = vec![
        "instance".into(),
        "create".into(),
        "atomic-create".into(),
        "--revision".into(),
        revision.clone().into(),
        "--input-file".into(),
        format!("config={}", good.display()).into(),
        "--input-file".into(),
        format!("secret={}", missing.display()).into(),
    ];
    let (code, _, _) = json_invoke(&root, args);
    assert_eq!(code, 1);
    assert!(
        PactrunApplication::open(&root)
            .unwrap()
            .list_instances()
            .unwrap()
            .is_empty()
    );
    let database = rusqlite::Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
    for table in [
        "instances",
        "managed_input_bindings",
        "managed_input_payloads",
        "runs",
    ] {
        let count: i64 = database
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "partial create in {table}");
    }
    drop(database);
    let incomplete = json_ok(
        &root,
        &["instance", "create", "incomplete", "--revision", &revision],
    );
    assert!(incomplete.get("required_inputs_satisfied").is_none());
    assert_eq!(incomplete["legacy_required_inputs"]["satisfied"], false);
    assert_eq!(
        json_ok(&root, &["instance", "show", "incomplete"])["instance_id"],
        incomplete["instance_id"]
    );
}

struct FailingCancellationHandler;

impl CancellationHandlerInstaller for FailingCancellationHandler {
    fn install(&self, _cancellation: ActionCancellation) -> Result<(), ()> {
        Err(())
    }
}

struct PrefixFailWriter {
    bytes: Vec<u8>,
    remaining: usize,
}

impl Write for PrefixFailWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"));
        }
        let written = self.remaining.min(bytes.len());
        self.bytes.extend_from_slice(&bytes[..written]);
        self.remaining -= written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn cli_roots() -> (TempDir, PathBuf, PathBuf) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/input-cli-tests");
    fs::create_dir_all(&parent).unwrap();
    let temporary = tempfile::Builder::new()
        .prefix("cli-")
        .tempdir_in(parent)
        .unwrap();
    let storage = temporary.path().join("storage");
    let source = temporary.path().join("source");
    fs::create_dir(&storage).unwrap();
    fs::create_dir(storage.join("database")).unwrap();
    fs::create_dir(storage.join("runtime-content")).unwrap();
    fs::create_dir(storage.join("staging")).unwrap();
    fs::create_dir(&source).unwrap();
    fs::write(
        source.join("pactrun.yaml"),
        r#"source_format: 1.0-alpha.2
package_id: 00000000000000000000000000000021
revision:
  inputs:
    - id: config
    - id: secret
      protection: secret
  actions: []
  migrations: []
runtime_content:
  files: []
"#,
    )
    .unwrap();
    (temporary, storage, source)
}

fn cli_action_roots() -> (TempDir, PathBuf, PathBuf) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/invoke-cli-tests");
    fs::create_dir_all(&parent).unwrap();
    let temporary = tempfile::Builder::new()
        .prefix("cli-action-")
        .tempdir_in(parent)
        .unwrap();
    let storage = temporary.path().join("storage");
    let source = temporary.path().join("source");
    fs::create_dir(&storage).unwrap();
    fs::create_dir(storage.join("database")).unwrap();
    fs::create_dir(storage.join("runtime-content")).unwrap();
    fs::create_dir(storage.join("staging")).unwrap();
    fs::create_dir(&source).unwrap();
    let worker_name = if cfg!(windows) {
        "worker.exe"
    } else {
        "worker"
    };
    fs::copy(env::current_exe().unwrap(), source.join(worker_name)).unwrap();
    fs::write(
        source.join("pactrun.yaml"),
        format!(
            r#"source_format: 1.0-alpha.2
package_id: 00000000000000000000000000000036
revision:
  inputs: []
  actions:
    - id: inspect
      access: observe
      parameters:
        - {{ id: value, type: string, sensitive: false }}
      hook:
        protocol_version: 1.0-alpha.1
        launch: {{ kind: direct, executable: worker }}
        args: []
        io: {{ terminal: none }}
      outputs: []
  migrations: []
runtime_content:
  files:
    - {{ id: worker, source: {worker_name}, path: bin/{worker_name}, executable: true }}
"#
        ),
    )
    .unwrap();
    (temporary, storage, source)
}

fn cli_hook_roots() -> (TempDir, PathBuf, PathBuf) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/invoke-cli-tests");
    fs::create_dir_all(&parent).unwrap();
    let temporary = tempfile::Builder::new()
        .prefix("cli-hook-")
        .tempdir_in(parent)
        .unwrap();
    let storage = temporary.path().join("storage");
    let source = temporary.path().join("source");
    fs::create_dir(&storage).unwrap();
    fs::create_dir(storage.join("database")).unwrap();
    fs::create_dir(storage.join("runtime-content")).unwrap();
    fs::create_dir(storage.join("staging")).unwrap();
    fs::create_dir(&source).unwrap();
    let worker_name = if cfg!(windows) {
        "worker.exe"
    } else {
        "worker"
    };
    fs::copy(env::current_exe().unwrap(), source.join(worker_name)).unwrap();
    fs::write(
            source.join("pactrun.yaml"),
            format!(
                r#"source_format: 1.0-alpha.2
package_id: 00000000000000000000000000000037
revision:
  inputs:
    - {{ id: secret_config, required: true, protection: secret }}
  actions:
    - id: execute
      access: observe
      parameters:
        - {{ id: mode, type: string, sensitive: false }}
        - {{ id: marker, type: string, sensitive: false }}
        - {{ id: expected_binding, type: string, sensitive: true }}
        - {{ id: sensitive_value, type: string, sensitive: true }}
      hook:
        protocol_version: 1.0-alpha.1
        launch: {{ kind: direct, executable: worker }}
        args: ["--exact", "hook::tests::hook_protocol_worker", "--nocapture", "--test-threads=1", "hook-fixture-argument-tail"]
        io: {{ terminal: none }}
      outputs: [{{ id: report }}]
  migrations: []
runtime_content:
  files:
    - {{ id: worker, source: {worker_name}, path: bin/{worker_name}, executable: true }}
"#
            ),
        )
        .unwrap();
    (temporary, storage, source)
}

fn prepared_cli_hook_instance() -> (TempDir, PathBuf, PathBuf, PathBuf) {
    prepared_cli_hook_instance_with_terminal("none")
}

fn prepared_cli_hook_instance_with_terminal(
    terminal: &str,
) -> (TempDir, PathBuf, PathBuf, PathBuf) {
    let (temporary, storage, source) = cli_hook_roots();
    let manifest = source.join("pactrun.yaml");
    let text = fs::read_to_string(&manifest)
        .unwrap()
        .replace("terminal: none", &format!("terminal: {terminal}"));
    fs::write(manifest, text).unwrap();
    let storage_env = Some(storage.as_os_str().to_owned());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(
        run(
            vec![
                "--format".into(),
                "json".into(),
                "pack".into(),
                "install".into(),
                source.into_os_string()
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    let revision = installed_reference(&stdout);
    let expected_path = temporary.path().join("expected-binding.txt");
    let sensitive_path = temporary.path().join("sensitive-value.txt");
    fs::write(&expected_path, b"hook-fixture-secret-binding").unwrap();
    fs::write(&sensitive_path, b"hook-fixture-sensitive-parameter").unwrap();
    assert_eq!(
        run(
            vec![
                "instance".into(),
                "create".into(),
                "node".into(),
                "--revision".into(),
                revision.into(),
                "--input-file".into(),
                format!("secret_config={}", expected_path.display()).into(),
            ],
            storage_env,
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    (temporary, storage, expected_path, sensitive_path)
}

fn successful_invoke_args(
    marker: &Path,
    expected_path: &Path,
    sensitive_path: &Path,
) -> Vec<OsString> {
    vec![
        "invoke".into(),
        "node".into(),
        "execute".into(),
        "--param".into(),
        "mode=success".into(),
        "--param".into(),
        format!("marker={}", marker.display()).into(),
        "--param-file".into(),
        format!("expected_binding={}", expected_path.display()).into(),
        "--param-file".into(),
        format!("sensitive_value={}", sensitive_path.display()).into(),
    ]
}

// Test-ID: PR-TEST-0079
// Verifies: PR-REQ-0263, PR-REQ-0265, PR-REQ-0271, PR-REQ-0272
#[test]
fn closed_cli_references_and_storage_root_boundary_are_exact() {
    assert!(matches!(
        parse_command(vec!["instance".into(), "show".into(), "Node".into()]).unwrap(),
        Command::ShowInstance { .. }
    ));
    assert!(
        parse_command(vec![
            "instance".into(),
            "show".into(),
            "00".into(),
            "extra".into()
        ])
        .is_err()
    );
    #[cfg(unix)]
    {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let (_, path) =
            split_initial_input_file(OsString::from_vec(b"config=non-unicode-\xff.bin".to_vec()))
                .unwrap();
        assert_eq!(path.as_os_str().as_bytes(), b"non-unicode-\xff.bin");
    }
    assert!(parse_revision_reference("bare".to_owned()).is_err());
    assert!(
        parse_revision_reference(format!(
            "exact:{}/sha256:{}",
            "00".repeat(16),
            "11".repeat(32)
        ))
        .is_ok()
    );
    assert!(
        parse_command(vec![
            "instance".into(),
            "create".into(),
            "node".into(),
            "--revision".into(),
            format!("exact:{}/sha256:{}", "00".repeat(16), "11".repeat(32)).into(),
            "--input-file".into(),
            "config=missing-a".into(),
            "--input-file".into(),
            "config=missing-b".into(),
        ])
        .is_err()
    );

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(
        run(
            vec!["pack".into(), "generate-id".into()],
            None,
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0
    );
    assert_eq!(stdout.len(), 33);
    stdout.clear();
    assert_eq!(
        run(
            vec!["instance".into(), "list".into()],
            None,
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        1
    );
}

// Test-ID: PR-TEST-0114
// Verifies: PR-REQ-0097, PR-REQ-0114, PR-REQ-0284, PR-REQ-0285
#[test]
fn commands_and_run_projection_are_structural_and_redacted() {
    assert!(matches!(
        parse_command(vec!["action".into(), "list".into(), "node".into()]).unwrap(),
        Command::ListActions { .. }
    ));
    assert!(matches!(
        parse_command(vec![
            "invoke".into(),
            "node".into(),
            "deploy".into(),
            "--plan".into(),
            "--authorize-recovery-override".into(),
        ])
        .unwrap(),
        Command::Invoke {
            plan: true,
            recovery_override: true,
            ..
        }
    ));
    assert!(matches!(
        parse_command(vec!["run".into(), "reconcile".into()]).unwrap(),
        Command::ReconcileRuns
    ));
    assert!(matches!(
        parse_command(vec![
            "instance".into(),
            "resolve-manual-recovery".into(),
            "node".into(),
        ])
        .unwrap(),
        Command::ResolveManualRecovery { expected: None, .. }
    ));

    let run = RunId::from_bytes([1; 16]);
    let view = crate::domain::RunInspectionData {
        run: crate::domain::RunView {
            id: run,
            instance: crate::domain::InstanceId::from_bytes([2; 16]),
            accepted_state_version: crate::domain::InstanceStateVersion::from_bytes([3; 16]),
            accepted_at_unix_ms: 10,
            action: crate::domain::ActionRunIdentity {
                revision: RevisionIdentity::new(
                    PackageId::from_bytes([4; 16]),
                    RevisionContentDigest::from_bytes([5; 32]),
                ),
                action: ActionIdentity::parse("deploy").unwrap(),
            },
            state: crate::domain::RunState::Finished(crate::domain::RunOutcomeView {
                outcome: RunOutcome::Failed,
                boundary: crate::domain::ActionRunBoundary::Admitted,
                terminal_risk: crate::domain::RecoveryRiskState::Clear,
                finished_at_unix_ms: 20,
                primary_failure: Some(crate::domain::RunPrimaryFailure {
                    cause: None,
                    failure: crate::domain::RunFailureRecord {
                        error: crate::domain::PactrunErrorRef::new("execution", "launch_failed")
                            .unwrap(),
                        message: "persisted-secret-message".to_owned(),
                    },
                    step: crate::domain::RunFailedStep::Plan(ActionPlanStep::LaunchHook),
                }),
                secondary_failures: vec![crate::domain::RunFailureRecord {
                    error: crate::domain::PactrunErrorRef::new(
                        "execution",
                        "workspace_cleanup_failed",
                    )
                    .unwrap(),
                    message: "another-persisted-secret-message".to_owned(),
                }],
                hook_completion: Some(crate::domain::HookCompletionRecord {
                    status: crate::domain::HookCompletionStatus::Failure,
                    code: Some(crate::domain::HookCodeV1::parse("hook_code").unwrap()),
                    message: Some("hook-free-text-secret".to_owned()),
                }),
                artifacts: vec![crate::domain::RunArtifactSummary {
                    edge: None,
                    output: crate::domain::ManagedOutputIdentity::parse("report").unwrap(),
                    byte_length: 7,
                }],
            }),
        },
        current_recovery_guard: None,
    };
    let mut output = Vec::new();
    write_run(&mut output, &view).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("primary_failure: execution:launch_failed"));
    assert!(output.contains("hook_completion_text: withheld"));
    for secret in [
        "persisted-secret-message",
        "another-persisted-secret-message",
        "hook_code",
        "hook-free-text-secret",
    ] {
        assert!(
            !output.contains(secret),
            "structural Run output leaked {secret}"
        );
    }
}

// Test-ID: PR-TEST-0123
// Verifies: PR-REQ-0049, PR-REQ-0286
#[test]
fn cancellation_handler_installation_failure_precedes_storage_and_acceptance() {
    let temporary = tempfile::tempdir().unwrap();
    let storage = temporary.path().join("storage");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let status = run_with_handler_installer(
        &FailingCancellationHandler,
        vec![
            "invoke".into(),
            "node".into(),
            "deploy".into(),
            "--param".into(),
            "secret=must-not-appear".into(),
        ],
        Some(storage.as_os_str().to_owned()),
        &mut io::empty(),
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(status, 1);
    assert_eq!(stdout, Vec::<u8>::new());
    assert!(String::from_utf8_lossy(&stderr).contains(CANCELLATION_HANDLER_FAILURE));
    assert!(!String::from_utf8_lossy(&stderr).contains("must-not-appear"));
    assert!(!storage.exists(), "handler failure must not open storage");
}

// Test-ID: PR-TEST-0116
// Verifies: PR-REQ-0135, PR-REQ-0288
#[test]
fn parameter_sources_preserve_exact_text_and_protected_channel() {
    let command = parse_command(vec![
        "invoke".into(),
        "node".into(),
        "deploy".into(),
        "--param".into(),
        "ordinary=a=b".into(),
        "--param-file".into(),
        "protected=host=path".into(),
        "--startup-timeout-ms".into(),
        "0".into(),
    ])
    .unwrap();
    let Command::Invoke { parameters, .. } = command else {
        panic!("expected Invoke");
    };
    assert!(matches!(
        &parameters[0],
        ParameterSourceSpec::Text(id, value)
            if id.as_str() == "ordinary" && value == "a=b"
    ));
    assert!(matches!(
        &parameters[1],
        ParameterSourceSpec::File(id, path)
            if id.as_str() == "protected" && path == Path::new("host=path")
    ));

    let temporary = tempfile::tempdir().unwrap();
    let file = temporary.path().join("parameter.txt");
    fs::write(&file, b"\xEF\xBB\xBF protected\n").unwrap();
    let protected = ParameterIdentity::parse("protected").unwrap();
    let mut stdin = io::Cursor::new(b"stdin\n".to_vec());
    let values = read_parameter_sources(
        &[
            ParameterSourceSpec::File(protected.clone(), file),
            ParameterSourceSpec::Stdin(ParameterIdentity::parse("from_stdin").unwrap()),
        ],
        &mut stdin,
        &ActionCancellation::default(),
    )
    .unwrap();
    assert_eq!(values[0].text, "\u{feff} protected\n");
    assert_eq!(values[1].text, "stdin\n");
    assert!(matches!(
        values[0].source,
        crate::domain::ParameterTextSource::Protected
    ));
    assert!(matches!(
        values[1].source,
        crate::domain::ParameterTextSource::Protected
    ));
}

// Test-ID: PR-TEST-0130
// Verifies: PR-REQ-0286, PR-REQ-0288
#[test]
fn invoke_rejects_unrepresentable_deadlines_before_run_acceptance() {
    let (_temporary, storage, _expected_path, _sensitive_path) = prepared_cli_hook_instance();
    let storage_env = Some(storage.as_os_str().to_owned());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let status = run(
        vec![
            "invoke".into(),
            "node".into(),
            "execute".into(),
            "--startup-timeout-ms".into(),
            u64::MAX.to_string().into(),
        ],
        storage_env.clone(),
        &mut io::empty(),
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(status, 2);
    assert!(String::from_utf8_lossy(&stderr).contains("cannot be represented"));
    stdout.clear();
    stderr.clear();
    assert_eq!(
        run(
            vec![
                "run".into(),
                "list".into(),
                "node".into(),
                "--no-trunc".into()
            ],
            storage_env,
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0
    );
    assert!(
        json_ok(&storage, &["run", "list"])["items"]
            .as_array()
            .unwrap()
            .is_empty(),
        "invalid timeout must create no Run"
    );
}

// Test-ID: PR-TEST-0118
// Verifies: PR-REQ-0284, PR-REQ-0287
#[test]
fn recovery_commands_have_explicit_operator_spellings() {
    let expected = InstanceStateVersion::from_bytes([9; 16]).to_string();
    assert!(matches!(
        parse_command(vec![
            "instance".into(),
            "resolve-manual-recovery".into(),
            "node".into(),
            "--if-version".into(),
            expected.into(),
        ])
        .unwrap(),
        Command::ResolveManualRecovery {
            expected: Some(_),
            ..
        }
    ));
    assert!(parse_command(vec!["run".into(), "reconcile".into(), "--force".into(),]).is_err());
}

// Test-ID: PR-TEST-0120
// Verifies: PR-REQ-0095, PR-REQ-0284
#[test]
fn human_action_and_plan_commands_use_the_read_only_path() {
    let (_temporary, storage, source) = cli_action_roots();
    let storage_env = Some(storage.as_os_str().to_owned());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(
        run(
            vec![
                "--format".into(),
                "json".into(),
                "pack".into(),
                "install".into(),
                source.into_os_string()
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    let revision = installed_reference(&stdout);
    stdout.clear();
    stderr.clear();
    assert_eq!(
        run(
            vec![
                "instance".into(),
                "create".into(),
                "node".into(),
                "--revision".into(),
                revision.into(),
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    stdout.clear();
    stderr.clear();
    assert_eq!(
        run(
            vec!["action".into(), "list".into(), "node".into()],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0
    );
    assert!(String::from_utf8_lossy(&stdout).contains("ACTION"));
    assert!(
        String::from_utf8_lossy(&stdout)
            .lines()
            .any(|line| line.starts_with("inspect  "))
    );
    stdout.clear();
    let before_database = fs::read(storage.join("database/pactrun.sqlite3")).unwrap();
    let mut before_staging = fs::read_dir(storage.join("staging"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    before_staging.sort();
    assert_eq!(
        run(
            vec![
                "invoke".into(),
                "node".into(),
                "inspect".into(),
                "--param".into(),
                "value=exact text".into(),
                "--plan".into(),
            ],
            storage_env,
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    let plan = String::from_utf8_lossy(&stdout);
    assert!(plan.contains("Mode: preview"));
    assert!(plan.contains("Launch"));
    let typed = json_ok(
        &storage,
        &[
            "invoke",
            "node",
            "inspect",
            "--param",
            "value=exact text",
            "--plan",
        ],
    );
    assert!(
        typed["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| step == "launch_hook")
    );
    assert!(!plan.contains("exact text"));
    assert_eq!(
        fs::read(storage.join("database/pactrun.sqlite3")).unwrap(),
        before_database
    );
    let mut after_staging = fs::read_dir(storage.join("staging"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    after_staging.sort();
    assert_eq!(after_staging, before_staging);
}

// Test-ID: PR-TEST-0119
// Verifies: PR-REQ-0094, PR-REQ-0284
#[test]
fn human_invoke_creates_and_finishes_a_managed_run() {
    let (temporary, storage, source) = cli_hook_roots();
    let storage_env = Some(storage.as_os_str().to_owned());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(
        run(
            vec![
                "--format".into(),
                "json".into(),
                "pack".into(),
                "install".into(),
                source.into_os_string()
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    let revision = installed_reference(&stdout);
    stdout.clear();
    stderr.clear();
    let expected_path = temporary.path().join("expected-binding.txt");
    fs::write(&expected_path, b"hook-fixture-secret-binding").unwrap();
    assert_eq!(
        run(
            vec![
                "instance".into(),
                "create".into(),
                "node".into(),
                "--revision".into(),
                revision.into(),
                "--input-file".into(),
                format!("secret_config={}", expected_path.display()).into(),
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );

    let marker = temporary.path().join("invoke-marker");
    let sensitive_path = temporary.path().join("sensitive-value.txt");
    fs::write(&sensitive_path, b"hook-fixture-sensitive-parameter").unwrap();
    stdout.clear();
    stderr.clear();
    assert_eq!(
        run(
            vec![
                "invoke".into(),
                "node".into(),
                "execute".into(),
                "--param".into(),
                "mode=success".into(),
                "--param".into(),
                format!("marker={}", marker.display()).into(),
                "--param-file".into(),
                format!("expected_binding={}", expected_path.display()).into(),
                "--param-file".into(),
                format!("sensitive_value={}", sensitive_path.display()).into(),
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    let summary = String::from_utf8_lossy(&stderr);
    let run_id = summary
        .lines()
        .find_map(|line| line.strip_prefix("Run: "))
        .expect("invoke prints the completed RunId")
        .to_owned();
    assert!(!summary.contains("hook-fixture-sensitive-parameter"));
    assert!(!summary.contains("hook-fixture-secret-binding"));
    stdout.clear();
    stderr.clear();
    assert_eq!(
        run(
            vec![
                "run".into(),
                "list".into(),
                "node".into(),
                "--no-trunc".into()
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0
    );
    assert!(String::from_utf8_lossy(&stdout).contains(&run_id));
    stdout.clear();
    assert_eq!(
        run(
            vec!["run".into(), "show".into(), run_id.into()],
            storage_env,
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0
    );
    assert!(String::from_utf8_lossy(&stdout).contains("Outcome: succeeded"));
}

// Test-ID: PR-TEST-0124
// Verifies: PR-REQ-0052, PR-REQ-0286
#[test]
fn broken_final_diagnostic_writer_does_not_change_finished_run_ownership() {
    let (_temporary, storage, source) = cli_hook_roots();
    let storage_env = Some(storage.as_os_str().to_owned());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(
        run(
            vec![
                "--format".into(),
                "json".into(),
                "pack".into(),
                "install".into(),
                source.into_os_string()
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    let revision = installed_reference(&stdout);
    let expected_path = storage.parent().unwrap().join("expected-binding.txt");
    let sensitive_path = storage.parent().unwrap().join("sensitive-value.txt");
    fs::write(&expected_path, b"hook-fixture-secret-binding").unwrap();
    fs::write(&sensitive_path, b"hook-fixture-sensitive-parameter").unwrap();
    stdout.clear();
    stderr.clear();
    assert_eq!(
        run(
            vec![
                "instance".into(),
                "create".into(),
                "node".into(),
                "--revision".into(),
                revision.into(),
                "--input-file".into(),
                format!("secret_config={}", expected_path.display()).into(),
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );

    let marker = storage.parent().unwrap().join("broken-diagnostic-marker");
    let mut broken_stderr = PrefixFailWriter {
        bytes: Vec::new(),
        remaining: 0,
    };
    assert_eq!(
        run(
            vec![
                "invoke".into(),
                "node".into(),
                "execute".into(),
                "--param".into(),
                "mode=success".into(),
                "--param".into(),
                format!("marker={}", marker.display()).into(),
                "--param-file".into(),
                format!("expected_binding={}", expected_path.display()).into(),
                "--param-file".into(),
                format!("sensitive_value={}", sensitive_path.display()).into(),
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut broken_stderr,
        ),
        1
    );
    assert!(marker.with_extension("session").exists());

    stdout.clear();
    stderr.clear();
    assert_eq!(
        run(
            vec![
                "run".into(),
                "list".into(),
                "node".into(),
                "--no-trunc".into()
            ],
            storage_env,
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    let listing = String::from_utf8_lossy(&stdout);
    assert!(
        listing
            .lines()
            .skip(1)
            .any(|line| line.ends_with("  succeeded"))
    );
}

// Test-ID: PR-TEST-0126
// Verifies: PR-REQ-0049, PR-REQ-0286
#[test]
fn admission_retry_keeps_the_exact_run_with_a_broken_diagnostic_writer() {
    let (_temporary, storage, expected_path, sensitive_path) = prepared_cli_hook_instance();
    let storage_env = Some(storage.as_os_str().to_owned());
    crate::executor::fail_next_admission_for_test();
    let marker = storage.parent().unwrap().join("admission-retry-marker");
    let mut stdout = Vec::new();
    let mut broken_stderr = PrefixFailWriter {
        bytes: Vec::new(),
        remaining: 0,
    };
    assert_eq!(
        run(
            successful_invoke_args(&marker, &expected_path, &sensitive_path),
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut broken_stderr,
        ),
        1
    );
    assert!(marker.with_extension("session").exists());

    let mut listing = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(
        run(
            vec![
                "run".into(),
                "list".into(),
                "node".into(),
                "--no-trunc".into()
            ],
            storage_env,
            &mut io::empty(),
            &mut listing,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    let listing = String::from_utf8_lossy(&listing);
    let rows: Vec<_> = listing
        .lines()
        .skip(1)
        .take_while(|line| !line.is_empty())
        .collect();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].ends_with("  succeeded"));
}

// Test-ID: PR-TEST-0127
// Verifies: PR-REQ-0052, PR-REQ-0286
#[test]
fn repeated_finalization_failures_keep_owner_until_terminal_commit() {
    let (_temporary, storage, expected_path, sensitive_path) = prepared_cli_hook_instance();
    let storage_env = Some(storage.as_os_str().to_owned());
    let faults = crate::application::fail_next_finalization_advances_for_test(3);
    let marker = storage.parent().unwrap().join("finalization-retry-marker");
    let mut stdout = Vec::new();
    let mut broken_stderr = PrefixFailWriter {
        bytes: Vec::new(),
        remaining: 0,
    };
    assert_eq!(
        run(
            successful_invoke_args(&marker, &expected_path, &sensitive_path),
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut broken_stderr,
        ),
        1
    );
    assert_eq!(faults.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(marker.with_extension("session").exists());

    let runs = json_ok(&storage, &["run", "list", "node"]);
    assert_eq!(runs["items"].as_array().unwrap().len(), 1);
    assert_eq!(runs["items"][0]["state"]["phase"], "finished");
    assert_eq!(runs["items"][0]["state"]["outcome"], "succeeded");
}

// Test-ID: PR-TEST-0078
// Verifies: PR-REQ-0092, PR-REQ-0119, PR-REQ-0263, PR-REQ-0266, PR-REQ-0268, PR-REQ-0271
#[test]
fn cli_install_instance_secret_export_and_atomic_no_clobber_are_end_to_end() {
    let (_temporary, storage, source) = cli_roots();
    let storage_env = Some(storage.as_os_str().to_owned());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    assert_eq!(
        run(
            vec![
                "--format".into(),
                "json".into(),
                "pack".into(),
                "install".into(),
                source.into_os_string()
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    let revision = installed_reference(&stdout);
    assert!(revision.starts_with("exact:"));

    let config_path = storage.parent().unwrap().join("config.bin");
    fs::write(&config_path, b"configuration").unwrap();
    stdout.clear();
    stderr.clear();
    assert_eq!(
        run(
            vec![
                "instance".into(),
                "create".into(),
                "node".into(),
                "--revision".into(),
                revision.into(),
                "--input-file".into(),
                format!("config={}", config_path.display()).into(),
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(!String::from_utf8_lossy(&stdout).contains("configuration"));
    stdout.clear();
    stderr.clear();
    assert_eq!(
        run(
            vec!["instance".into(), "show".into(), "Node".into()],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        1
    );

    stdout.clear();
    stderr.clear();
    assert_eq!(
        run(
            vec![
                "input".into(),
                "set".into(),
                "node".into(),
                "secret".into(),
                "--stdin".into(),
            ],
            storage_env.clone(),
            &mut io::Cursor::new(b"top-secret"),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );

    stdout.clear();
    stderr.clear();
    assert_eq!(
        run(
            vec![
                "input".into(),
                "export".into(),
                "node".into(),
                "secret".into(),
                "--output".into(),
                "-".into(),
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        1
    );
    assert!(stdout.is_empty());

    stdout.clear();
    stderr.clear();
    assert_eq!(
        run(
            vec![
                "input".into(),
                "export".into(),
                "node".into(),
                "secret".into(),
                "--output".into(),
                "-".into(),
                "--authorize-secret-export".into(),
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    assert_eq!(stdout, b"top-secret");

    let mut partial = PrefixFailWriter {
        bytes: Vec::new(),
        remaining: 3,
    };
    stderr.clear();
    assert_eq!(
        run(
            vec![
                "input".into(),
                "export".into(),
                "node".into(),
                "secret".into(),
                "--output".into(),
                "-".into(),
                "--authorize-secret-export".into(),
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut partial,
            &mut stderr,
        ),
        1
    );
    assert_eq!(partial.bytes, b"top");

    let output = storage.parent().unwrap().join("export.bin");
    stdout.clear();
    stderr.clear();
    let export_args = vec![
        "input".into(),
        "export".into(),
        "node".into(),
        "secret".into(),
        "--output".into(),
        output.as_os_str().to_owned(),
        "--authorize-secret-export".into(),
    ];
    assert_eq!(
        run(
            export_args.clone(),
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        ),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    assert_eq!(fs::read(&output).unwrap(), b"top-secret");
    assert_eq!(
        run(
            export_args,
            storage_env,
            &mut io::empty(),
            &mut Vec::new(),
            &mut Vec::new(),
        ),
        1
    );
    assert_eq!(fs::read(&output).unwrap(), b"top-secret");

    let staging = crate::managed_data::StagingSession::open(&storage).unwrap();
    let staged = staging
        .stage_managed_input(&mut io::Cursor::new(b"publication-window"))
        .unwrap();
    let before = storage.parent().unwrap().join("before-publication.bin");
    assert!(
        publish_output_file_with_fault(&staged, &before, |point| {
            if point == OutputPublicationPoint::BeforeFinalNamePublication {
                Err(CliError::operation("injected pre-publication failure"))
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    assert!(!before.exists());
    assert!(
        !fs::read_dir(before.parent().unwrap())
            .unwrap()
            .any(|entry| {
                entry
                    .ok()
                    .and_then(|entry| entry.file_name().into_string().ok())
                    .is_some_and(|name| {
                        name.starts_with(".pactrun-export-") && name.ends_with(".tmp")
                    })
            })
    );

    let after = storage.parent().unwrap().join("after-publication.bin");
    assert!(
        publish_output_file_with_fault(&staged, &after, |point| {
            if point == OutputPublicationPoint::AfterFinalNamePublication {
                Err(CliError::operation("injected post-publication failure"))
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    assert_eq!(fs::read(after).unwrap(), b"publication-window");
}
