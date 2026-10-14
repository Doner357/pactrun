// Reuse cli::tests fixtures to exercise the ordinary dispatcher and real Hooks.
use super::*;

// Test-ID: PR-TEST-0746
// Verifies: PR-REQ-0395, PR-REQ-0396
#[test]
fn configuration_global_prefix_is_closed_and_preserves_operands() {
    let args = [
        "--config",
        "settings.toml",
        "--format",
        "json",
        "--verbosity",
        "detailed",
        "--no-phase-messages",
        "--diagnostic-level",
        "detailed",
        "invoke",
        "node",
        "inspect",
        "--param",
        "text=--verbosity",
    ];
    let (globals, args) =
        configuration::select(args.into_iter().map(Into::into).collect()).unwrap();
    assert_eq!(globals.user_file, Some(PathBuf::from("settings.toml")));
    assert_eq!(
        globals.overrides.verbosity,
        Some(crate::configuration::Verbosity::Detailed)
    );
    assert_eq!(globals.overrides.phase_messages, Some(false));
    let (format, rest) = machine::select(args).unwrap();
    assert_eq!(format.format, presentation::Format::Json);
    assert_eq!(rest.last().unwrap(), "text=--verbosity");
    for args in [
        vec!["--verbosity", "quiet", "--verbosity", "normal", "version"],
        vec!["--phase-messages", "--no-phase-messages", "--version"],
        vec!["--config", "a", "--config", "b", "--version"],
        vec!["--config", "-", "--version"],
        vec!["--diagnostic-level", "raw-secret-sentinel", "--version"],
    ] {
        let error = match configuration::select(args.into_iter().map(Into::into).collect()) {
            Err(error) => error,
            Ok(_) => panic!("invalid prefix accepted"),
        };
        assert!(!error.to_string().contains("raw-secret-sentinel"));
    }
    let recognized = machine::recognize(
        &[
            "--no-phase-messages",
            "--config",
            "file",
            "--format",
            "jsonl",
            "--version",
        ]
        .map(Into::into),
    );
    assert_eq!(recognized.format, presentation::Format::Jsonl);
    assert!(presentation::raw_input_request(
        &[
            "--verbosity",
            "quiet",
            "--no-phase-messages",
            "--format",
            "json",
            "input",
            "export",
            "node",
            "secret",
            "--output",
            "-"
        ]
        .map(Into::into)
    ));
}

// Test-ID: PR-TEST-0747
// Verifies: PR-REQ-0393, PR-REQ-0395, PR-REQ-0396
#[test]
fn configuration_errors_precede_acquisition_and_do_not_break_help_or_raw_channels() {
    struct NoRead;
    impl Read for NoRead {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("configuration refusal read stdin")
        }
    }
    let (temp, _, _) = cli_roots();
    let root = temp.path().join("uncreated");
    let config = temp.path().join("invalid.toml");
    fs::write(&config, "[diagnostics]\nsecret_sentinel = 'private'").unwrap();
    for format in ["human", "json", "jsonl"] {
        let mut args: Vec<OsString> = vec![
            "--config".into(),
            config.clone().into_os_string(),
            "--format".into(),
            format.into(),
        ];
        args.extend(["input", "set", "node", "config", "--stdin"].map(Into::into));
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(
            run(
                args,
                Some(root.clone().into_os_string()),
                &mut NoRead,
                &mut out,
                &mut err
            ),
            2
        );
        assert!(!root.exists());
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out),
            String::from_utf8_lossy(&err)
        );
        assert!(text.contains("user_file configuration"));
        assert!(!text.contains("secret_sentinel") && !text.contains("private"));
    }
    for tail in [vec!["--help"], vec!["--version"]] {
        let mut args = vec!["--config".into(), config.clone().into_os_string()];
        args.extend(tail.into_iter().map(OsString::from));
        assert_eq!(
            run(
                args,
                Some(root.clone().into_os_string()),
                &mut NoRead,
                &mut Vec::new(),
                &mut Vec::new()
            ),
            0
        );
        assert!(!root.exists());
    }
    let mut args = vec![
        "--config".into(),
        config.into_os_string(),
        "--format".into(),
        "json".into(),
    ];
    args.extend(["input", "export", "node", "config", "--output", "-"].map(Into::into));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(
        run(
            args,
            Some(root.into_os_string()),
            &mut NoRead,
            &mut out,
            &mut err
        ),
        2
    );
    assert!(
        out.is_empty(),
        "configuration error contaminated raw payload stdout"
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&err).unwrap()["status"],
        "failure"
    );
}

// Test-ID: PR-TEST-0748
// Verifies: PR-REQ-0396
#[test]
fn configuration_retention_overrides_are_tristate_for_every_operation() {
    let id = "00000000000000000000000000000001";
    let revision = "exact:00000000000000000000000000000001/sha256:0000000000000000000000000000000000000000000000000000000000000001";
    for base in [
        vec!["invoke", "node", "inspect"],
        vec!["snapshot", "capture", "node"],
        vec!["snapshot", "restore", "node", id],
        vec!["instance", "migrate", "node", "--to", revision],
        vec!["instance", "delete", "node"],
        vec!["instance", "abandon", "node"],
        vec![
            "instance",
            "create",
            "node",
            "--revision",
            revision,
            "--restore-from",
            id,
        ],
    ] {
        let args: Vec<OsString> = base.iter().map(OsString::from).collect();
        assert_eq!(
            parse_command(args.clone()).unwrap().hook_text_retention(),
            None,
            "{base:?}"
        );
        for (flag, value) in [
            ("--retain-hook-text", true),
            ("--no-retain-hook-text", false),
        ] {
            let mut selected = args.clone();
            selected.push(flag.into());
            assert_eq!(
                parse_command(selected.clone())
                    .unwrap()
                    .hook_text_retention(),
                Some(value),
                "{base:?}"
            );
            selected.push(flag.into());
            assert!(parse_command(selected).is_err());
        }
        let mut conflicting = args;
        conflicting.extend(["--retain-hook-text", "--no-retain-hook-text"].map(Into::into));
        assert!(parse_command(conflicting).is_err());
    }
}

// Test-ID: PR-TEST-0749
// Verifies: PR-REQ-0395, PR-REQ-0396
#[test]
fn configuration_store_retention_defaults_and_explicit_overrides_reach_real_runs() {
    let (temp, root, expected, sensitive) = prepared_cli_hook_instance_with_terminal("output");
    let config = root.join("pactrun.toml");
    for (index, (store_default, flag, retained)) in [
        (false, None, false),
        (false, Some("--retain-hook-text"), true),
        (true, None, true),
        (true, Some("--no-retain-hook-text"), false),
    ]
    .into_iter()
    .enumerate()
    {
        let source = format!("[diagnostics]\nretain_hook_text = {store_default}\n");
        fs::write(&config, &source).unwrap();
        let marker = temp.path().join(format!("config-run-{index}"));
        let mut args = machine_args(&marker, &expected, &sensitive);
        args.retain(|arg| arg != "--no-retain-hook-text");
        if let Some(flag) = flag {
            args.push(flag.into());
        }
        let (code, response, _) = json_invoke(&root, args);
        assert_eq!(code, 0, "{response}");
        let id = response["result"]["run"]["run_id"].as_str().unwrap();
        let shown = json_ok(&root, &["run", "show", id]);
        assert_eq!(shown["diagnostics"]["retain_text"], retained, "{shown}");
        let events = shown["diagnostics"]["events"].as_array().unwrap();
        assert_eq!(events.is_empty(), !retained);
        assert!(
            response["delivery"]["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["type"] == "diagnostic"
                    && event["message"] == "full-live-text".repeat(1024)),
            "retention changed live delivery"
        );
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            source,
            "CLI override rewrote Store configuration"
        );
    }
}

// Test-ID: PR-TEST-0798
// Verifies: PR-REQ-0394, PR-REQ-0395, PR-REQ-0398
#[test]
fn configured_store_byte_limit_reclaims_prior_hook_evidence_without_changing_runs() {
    use crate::domain::{
        DiagnosticKind, DiagnosticSeverity, DiagnosticWindow, HookEvidence, HookText,
    };
    let (temp, root, expected, sensitive) = prepared_cli_hook_instance_with_terminal("output");
    let invoke = |marker: &Path| {
        let mut args = machine_args(marker, &expected, &sensitive);
        args.retain(|arg| arg != "--no-retain-hook-text");
        json_invoke(&root, args)
    };
    let (code, response, _) = invoke(&temp.path().join("first-limit-run"));
    assert_eq!(code, 0, "{response}");
    let run = response["result"]["run"]["run_id"]
        .as_str()
        .unwrap()
        .parse::<RunId>()
        .unwrap();
    let p = crate::persistence::PactrunPersistence::open(&root).unwrap();
    let before = p.load_managed_run(run).unwrap();
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let mut window = DiagnosticWindow::default();
    for sequence in 1..=80 {
        window.push(HookEvidence {
            sequence,
            received_at_unix_ms: Some(at),
            stage: "action".into(),
            text: HookText {
                kind: DiagnosticKind::Diagnostic,
                severity: Some(DiagnosticSeverity::Info),
                completion_status: None,
                code: None,
                message: Some("x".repeat(16 * 1024)),
                truncated: false,
                truncated_prefix_bytes: 0,
            },
        });
    }
    p.save_diagnostics(run, &window, true, true, false).unwrap();
    assert!(p.diagnostic_retention_inventory(None, None).unwrap().bytes > 1_048_576);
    let source =
        "[diagnostics]\nmax_bytes = 1048576\nretention_days = 3650\nretain_hook_text = true\n";
    let config = root.join("pactrun.toml");
    fs::write(&config, source).unwrap();
    let (code, response, _) = invoke(&temp.path().join("second-limit-run"));
    assert_eq!(code, 0, "{response}");
    let journal =
        crate::trace_journal::Journal::open(&crate::trace_journal::Location::store(&root)).unwrap();
    assert!(
        p.diagnostic_retention_inventory(None, None).unwrap().bytes
            + journal.retention_inventory(None, None).unwrap().0
            <= 1_048_576
    );
    let evidence = p.inspect_diagnostics(run).unwrap().unwrap();
    assert_eq!(evidence.observed, 80);
    assert!(evidence.events.len() < 80);
    assert_eq!(p.load_managed_run(run).unwrap(), before);
    assert_eq!(fs::read_to_string(config).unwrap(), source);
}

// Test-ID: PR-TEST-0751
// Verifies: PR-REQ-0395, PR-REQ-0396
#[test]
fn configuration_is_fixed_before_a_running_hook_and_reloaded_next_invocation() {
    struct ChangeOnOutput {
        bytes: Vec<u8>,
        config: PathBuf,
        release: PathBuf,
        changed: bool,
    }
    impl Write for ChangeOnOutput {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            if !self.changed
                && self
                    .bytes
                    .windows(b"\"type\":\"output\"".len())
                    .any(|b| b == b"\"type\":\"output\"")
            {
                fs::write(&self.config, "[diagnostics]\nretain_hook_text = true\n")?;
                fs::write(&self.release, b"continue")?;
                self.changed = true;
            }
            Ok(())
        }
    }
    let (temp, root, expected, sensitive) = prepared_cli_hook_instance_with_terminal("output");
    let config = root.join("pactrun.toml");
    fs::write(&config, "[diagnostics]\nretain_hook_text = false\n").unwrap();
    let marker = temp.path().join("configuration-latch");
    let mut command = successful_invoke_args(&marker, &expected, &sensitive);
    *command.iter_mut().find(|a| **a == "mode=success").unwrap() =
        "mode=configuration_latch".into();
    let mut args = vec!["--format".into(), "jsonl".into()];
    args.extend(command);
    let mut out = ChangeOnOutput {
        bytes: Vec::new(),
        config,
        release: marker.with_extension("continue"),
        changed: false,
    };
    assert_eq!(
        run(
            args,
            Some(root.clone().into_os_string()),
            &mut io::empty(),
            &mut out,
            &mut Vec::new()
        ),
        0
    );
    assert!(out.changed);
    let events: Vec<serde_json::Value> = out
        .bytes
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_slice(l).unwrap())
        .collect();
    assert!(
        events
            .iter()
            .any(|e| e["type"] == "diagnostic" && e["code"] == "after_config_change")
    );
    let result = &events.last().unwrap()["response"]["result"];
    assert_eq!(result["diagnostics"]["retain_text"], false);
    assert!(
        result["diagnostics"]["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut next = machine_args(&temp.path().join("next-config"), &expected, &sensitive);
    next.retain(|a| a != "--no-retain-hook-text");
    let (code, response, _) = json_invoke(&root, next);
    assert_eq!(code, 0);
    assert_eq!(response["result"]["diagnostics"]["retain_text"], true);
    assert!(
        !response["result"]["diagnostics"]["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
