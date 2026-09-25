use std::{ffi::OsString, fs};

use super::support::{
    Scenario, assert_exit, assert_success, instance_projection, matrix_source, run_count, run_id,
    run_projections,
};

fn ready_matrix_scenario(package_id: u16) -> Scenario {
    let source = matrix_source();
    let scenario = Scenario::new(package_id, &source);
    scenario.install_and_create("node");
    set_config(&scenario, b"old");
    scenario
}

fn set_config(scenario: &Scenario, value: &[u8]) {
    set_input(scenario, "node", "config", value);
}

fn set_input(scenario: &Scenario, instance: &str, input: &str, value: &[u8]) {
    let source = scenario.path(&format!("{instance}-{input}-source"));
    fs::write(&source, value).unwrap();
    let output = scenario.run([
        OsString::from("input"),
        OsString::from("set"),
        OsString::from(instance),
        OsString::from(input),
        OsString::from("--file"),
        source.into_os_string(),
    ]);
    assert_success(&output);
}

fn instance_revision(scenario: &Scenario, instance: &str) -> String {
    let shown = scenario.run(["instance", "show", instance]);
    assert_success(&shown);
    String::from_utf8(shown.stdout)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("revision: "))
        .expect("instance show must render active revision")
        .to_owned()
}

fn parameter_arguments(file: &std::path::Path) -> Vec<OsString> {
    vec![
        OsString::from("invoke"),
        OsString::from("node"),
        OsString::from("parameters"),
        OsString::from("--param"),
        OsString::from("ordinary=first=equals"),
        OsString::from("--param-file"),
        OsString::from(format!("from_file={}", file.display())),
        OsString::from("--param-stdin"),
        OsString::from("from_stdin"),
        OsString::from("--param"),
        OsString::from("enabled=true"),
        OsString::from("--param"),
        OsString::from("count=7"),
        OsString::from("--param"),
        OsString::from("secret=secret-param-canary"),
    ]
}

// Test-ID: PR-TEST-0148
// Verifies: PR-REQ-0039, PR-REQ-0095, PR-REQ-0125, PR-REQ-0284
#[test]
fn action_inspection_and_plan_keep_parameterized_execution_read_only() {
    let scenario = ready_matrix_scenario(0x148);
    let file = scenario.path("parameter-source");
    fs::write(&file, b"\xEF\xBB\xBFfrom-file\n").unwrap();
    let shown = scenario.run(["action", "show", "node", "parameters"]);
    assert_success(&shown);
    assert!(String::from_utf8_lossy(&shown.stdout).contains("parameters"));
    let mut planned = parameter_arguments(&file);
    planned.push(OsString::from("--plan"));
    let planned = scenario.run_with_stdin(planned, b"stdin-value\n".to_vec());
    assert_success(&planned);
    let text = String::from_utf8_lossy(&planned.stdout);
    assert!(text.contains("action: parameters"));
    assert!(text.contains("Mode: preview"));
    let runs = scenario.run(["run", "list", "node", "--no-trunc"]);
    assert_success(&runs);
    assert_eq!(run_count(&runs), 0);
    assert_eq!(scenario.hook_launches(), 0);
}

// Test-ID: PR-TEST-0165
// Verifies: PR-REQ-0039, PR-REQ-0095, PR-REQ-0284, PR-REQ-0287
#[test]
fn action_inspection_and_plan_render_identity_types_and_launcher_without_reconciling_an_orphan() {
    let scenario = ready_matrix_scenario(0x165);
    let shown = scenario.run(["action", "show", "node", "parameters"]);
    assert_success(&shown);
    let shown = String::from_utf8_lossy(&shown.stdout);
    assert!(shown.contains("action: parameters"));
    assert!(shown.contains("parameter: ordinary\ttype: string"));
    assert!(shown.contains("parameter: enabled\ttype: boolean"));
    assert!(shown.contains("parameter: count\ttype: integer"));
    let machine = scenario.run(["--format", "json", "action", "show", "node", "parameters"]);
    assert_success(&machine);
    let machine: serde_json::Value = serde_json::from_slice(&machine.stdout).unwrap();
    assert_eq!(machine["result"]["launch"]["kind"], "direct");
    assert_eq!(machine["result"]["launch"]["content_id"], "worker");

    let owner = scenario.spawn(["invoke", "node", "hold_observe"]);
    let orphan = marker_run_id(&scenario);
    scenario.wait_for_marker("ready:hold_observe");
    let _ = owner.terminate();

    let inspected = scenario.run(["action", "show", "node", "parameters"]);
    assert_success(&inspected);
    let planned = scenario.run(["invoke", "node", "fast_observe", "--plan"]);
    assert_success(&planned);
    let planned = String::from_utf8_lossy(&planned.stdout);
    assert!(planned.contains("action: fast_observe"));
    assert!(planned.contains("instance_id: "));
    assert!(planned.contains("revision: "));
    assert!(planned.contains("launch: direct\truntime_path: "));
    assert!(planned.contains("Mode: preview"));
    let running = scenario.run(["run", "show", &orphan]);
    assert_success(&running);
    assert!(String::from_utf8_lossy(&running.stdout).contains("phase: running"));
    assert_eq!(scenario.hook_launches(), 1);
}

// Test-ID: PR-TEST-0149
// Verifies: PR-REQ-0049, PR-REQ-0284, PR-REQ-0288
#[test]
fn parameter_shape_and_acquisition_failures_do_not_accept_or_launch() {
    let scenario = ready_matrix_scenario(0x149);
    let initial_launches = scenario.hook_launches();
    for arguments in [
        vec!["invoke", "node", "parameters", "--param", "unknown=value"],
        vec![
            "invoke",
            "node",
            "parameters",
            "--param",
            "ordinary=one",
            "--param",
            "ordinary=two",
        ],
        vec![
            "invoke",
            "node",
            "parameters",
            "--param",
            "enabled=not-a-boolean",
        ],
        vec![
            "invoke",
            "node",
            "parameters",
            "--param-file",
            "from_file=does-not-exist",
        ],
        vec![
            "invoke",
            "node",
            "parameters",
            "--param-stdin",
            "from_stdin",
            "--param-stdin",
            "ordinary",
        ],
    ] {
        let output = scenario.run(arguments);
        assert!(matches!(output.status.code(), Some(1 | 2)));
        let runs = scenario.run(["run", "list", "node", "--no-trunc"]);
        assert_success(&runs);
        assert_eq!(run_count(&runs), 0);
        assert_eq!(scenario.hook_launches(), initial_launches);
    }
}

// Test-ID: PR-TEST-0166
// Verifies: PR-REQ-0049, PR-REQ-0094, PR-REQ-0284, PR-REQ-0288
#[test]
fn pre_acceptance_ordering_rejects_interactive_stdin_and_shape_errors_before_source_reads() {
    let scenario = ready_matrix_scenario(0x166);
    let missing = scenario.path("must-not-be-opened");
    let initial_launches = scenario.hook_launches();
    for arguments in [
        vec!["invoke", "missing", "parameters"],
        vec!["invoke", "node", "missing-action"],
        vec![
            "invoke",
            "node",
            "parameters",
            "--param-file",
            "ordinary=must-not-be-opened",
            "--param-file",
            "ordinary=must-not-be-opened",
        ],
        vec![
            "invoke",
            "node",
            "parameters",
            "--param-file",
            "unknown=must-not-be-opened",
        ],
        vec![
            "invoke",
            "node",
            "interactive_parameters",
            "--param-stdin",
            "from_stdin",
        ],
        vec![
            "invoke",
            "node",
            "parameters",
            "--action-timeout-ms",
            "18446744073709551616",
        ],
    ] {
        let output = scenario.run(arguments);
        assert!(matches!(output.status.code(), Some(1 | 2)));
        assert!(
            !missing.exists(),
            "pre-acceptance validation opened a source"
        );
        let runs = scenario.run(["run", "list", "node", "--no-trunc"]);
        assert_success(&runs);
        assert_eq!(run_count(&runs), 0);
        assert_eq!(scenario.hook_launches(), initial_launches);
    }
}

// Test-ID: PR-TEST-0150
// Verifies: PR-REQ-0094, PR-REQ-0133, PR-REQ-0284, PR-REQ-0288
#[test]
fn parameter_sources_reach_a_real_hook_without_text_rewriting() {
    let scenario = ready_matrix_scenario(0x150);
    let file = scenario.path("parameter-source");
    fs::write(&file, b"\xEF\xBB\xBFfrom-file\n").unwrap();
    let invoked = scenario.run_with_stdin(parameter_arguments(&file), b"stdin-value\n".to_vec());
    assert_success(&invoked);
    assert_eq!(scenario.hook_launches(), 1);
}

// Test-ID: PR-TEST-0167
// Verifies: PR-REQ-0094, PR-REQ-0098, PR-REQ-0133, PR-REQ-0284, PR-REQ-0288
#[test]
fn parameter_sources_preserve_empty_unicode_and_protected_values_exactly() {
    let scenario = ready_matrix_scenario(0x167);
    let file = scenario.path("unicode-parameter-source");
    fs::write(&file, "\u{feff}file-語\r\n").unwrap();
    let invoked = scenario.run_with_stdin(
        [
            OsString::from("invoke"),
            OsString::from("node"),
            OsString::from("parameters_edge"),
            OsString::from("--param"),
            OsString::from("ordinary="),
            OsString::from("--param-file"),
            OsString::from(format!("from_file={}", file.display())),
            OsString::from("--param-stdin"),
            OsString::from("from_stdin"),
            OsString::from("--param"),
            OsString::from("enabled=false"),
            OsString::from("--param"),
            OsString::from("count=-9"),
            OsString::from("--param"),
            OsString::from("secret=protected-source-canary"),
        ],
        "stdin-雪\r\n".as_bytes().to_vec(),
    );
    assert_success(&invoked);
    scenario.wait_for_marker("parameter-edge-values-verified");
    assert_eq!(scenario.hook_launches(), 1);
}

// Test-ID: PR-TEST-0168
// Verifies: PR-REQ-0049, PR-REQ-0097, PR-REQ-0125, PR-REQ-0284
#[test]
fn successful_observe_and_mutate_each_create_one_durable_terminal_run() {
    let scenario = ready_matrix_scenario(0x168);
    let before = scenario.run(["run", "list", "node", "--no-trunc"]);
    assert_success(&before);
    assert_eq!(run_count(&before), 0);

    for (action, expected_count) in [("fast_observe", 1), ("fast_mutate", 2)] {
        let invoked = scenario.invoke("node", action);
        assert_success(&invoked);
        let id = run_id(&invoked);
        let listed = scenario.run(["run", "list", "node", "--no-trunc"]);
        assert_success(&listed);
        assert_eq!(run_count(&listed), expected_count);
        let shown = scenario.run(["run", "show", &id]);
        assert_success(&shown);
        let shown = String::from_utf8_lossy(&shown.stdout);
        assert!(shown.contains(&format!("action: {action}")));
        assert!(shown.contains("phase: finished"));
        assert!(shown.contains("outcome: succeeded"));
        assert!(shown.contains("hook_completion_status: success"));
    }
    assert_eq!(scenario.hook_launches(), 2);
}

// Test-ID: PR-TEST-0151
// Verifies: PR-REQ-0097, PR-REQ-0212, PR-REQ-0281
#[test]
fn action_output_publication_is_subset_authorized_and_all_or_nothing() {
    let scenario = ready_matrix_scenario(0x151);
    let output = scenario.invoke("node", "outputs");
    assert_success(&output);
    let output_id = run_id(&output);
    let shown = scenario.run(["run", "show", &output_id]);
    assert_success(&shown);
    assert!(String::from_utf8_lossy(&shown.stdout).contains("artifact: report\tbytes: 13"));

    let unsubmitted = scenario.invoke("node", "output_unsubmitted");
    assert_success(&unsubmitted);
    let unsubmitted_id = run_id(&unsubmitted);
    let shown = scenario.run(["run", "show", &unsubmitted_id]);
    assert_success(&shown);
    assert!(!String::from_utf8_lossy(&shown.stdout).contains("artifact: report"));

    for action in ["output_missing", "invalid_output"] {
        let invoked = scenario.invoke("node", action);
        assert_exit(&invoked, 1);
        let id = run_id(&invoked);
        let shown = scenario.run(["run", "show", &id]);
        assert_success(&shown);
        assert!(!String::from_utf8_lossy(&shown.stdout).contains("artifact: report"));
    }
}

// Test-ID: PR-TEST-0169
// Verifies: PR-REQ-0097, PR-REQ-0212, PR-REQ-0216, PR-REQ-0281
#[test]
fn multiple_output_authorities_publish_only_the_submitted_complete_eligible_group() {
    let scenario = ready_matrix_scenario(0x169);
    let empty = scenario.invoke("node", "outputs_empty");
    assert_success(&empty);
    let empty = scenario.run(["run", "show", &run_id(&empty)]);
    assert_success(&empty);
    assert!(!String::from_utf8_lossy(&empty.stdout).contains("artifact: "));

    let subset = scenario.invoke("node", "outputs_subset");
    assert_success(&subset);
    let subset = scenario.run(["run", "show", &run_id(&subset)]);
    assert_success(&subset);
    let subset = String::from_utf8_lossy(&subset.stdout);
    assert!(subset.contains("artifact: report\tbytes: 13"));
    assert!(!subset.contains("artifact: summary"));

    let multiple = scenario.invoke("node", "outputs_multiple");
    assert_success(&multiple);
    let multiple = scenario.run(["run", "show", &run_id(&multiple)]);
    assert_success(&multiple);
    let multiple = String::from_utf8_lossy(&multiple.stdout);
    assert!(multiple.contains("artifact: report\tbytes: 13"));
    assert!(multiple.contains("artifact: summary\tbytes: 14"));

    for action in ["outputs_group_missing", "invalid_output"] {
        let invoked = scenario.invoke("node", action);
        assert_exit(&invoked, 1);
        let shown = scenario.run(["run", "show", &run_id(&invoked)]);
        assert_success(&shown);
        assert!(!String::from_utf8_lossy(&shown.stdout).contains("artifact: "));
    }

    let failure = scenario.invoke("node", "outputs_failure");
    assert_exit(&failure, 1);
    let failure = scenario.run(["run", "show", &run_id(&failure)]);
    assert_success(&failure);
    let failure = String::from_utf8_lossy(&failure.stdout);
    assert!(failure.contains("outcome: failed"));
    assert!(failure.contains("hook_completion_status: failure"));
    assert!(failure.contains("artifact: report\tbytes: 13"));
    assert!(failure.contains("artifact: summary\tbytes: 14"));
}

// Test-ID: PR-TEST-0152
// Verifies: PR-REQ-0050, PR-REQ-0051, PR-REQ-0052, PR-REQ-0094, PR-REQ-0097, PR-REQ-0216
#[test]
fn hook_transport_failures_and_timeouts_finish_non_success_runs() {
    let scenario = ready_matrix_scenario(0x152);
    for (action, options) in [
        ("exit_before_connect", Vec::new()),
        ("protocol_error", Vec::new()),
        ("eof", Vec::new()),
        (
            "preconnect_hang",
            vec!["--startup-timeout-ms", "50", "--termination-grace-ms", "10"],
        ),
        (
            "timeout",
            vec!["--action-timeout-ms", "50", "--termination-grace-ms", "10"],
        ),
    ] {
        let mut arguments = vec!["invoke", "node", action];
        arguments.extend(options);
        let invoked = scenario.run(arguments);
        assert_exit(&invoked, 1);
        let id = run_id(&invoked);
        let shown = scenario.run(["run", "show", &id]);
        assert_success(&shown);
        assert!(!String::from_utf8_lossy(&shown.stdout).contains("outcome: succeeded"));
    }
}

// Test-ID: PR-TEST-0170
// Verifies: PR-REQ-0050, PR-REQ-0051, PR-REQ-0052, PR-REQ-0097, PR-REQ-0216, PR-REQ-0217
#[test]
fn representative_failure_classes_expose_finished_terminal_phase_without_protocol_conflation() {
    let scenario = ready_matrix_scenario(0x170);
    for (action, options, outcome) in [
        ("declared_failure", Vec::new(), "failed"),
        ("exit_before_connect", Vec::new(), "failed"),
        ("wrong_protocol_version", Vec::new(), "failed"),
        ("malformed_framing", Vec::new(), "failed"),
        ("eof", Vec::new(), "failed"),
        (
            "preconnect_hang",
            vec!["--startup-timeout-ms", "50", "--termination-grace-ms", "10"],
            "timed_out",
        ),
        (
            "timeout",
            vec!["--action-timeout-ms", "50", "--termination-grace-ms", "10"],
            "timed_out",
        ),
    ] {
        let mut arguments = vec!["invoke", "node", action];
        arguments.extend(options);
        let invoked = scenario.run(arguments);
        assert_exit(&invoked, 1);
        let shown = scenario.run(["run", "show", &run_id(&invoked)]);
        assert_success(&shown);
        let shown = String::from_utf8_lossy(&shown.stdout);
        assert!(shown.contains("phase: finished"), "{action}: {shown}");
        assert!(
            shown.contains(&format!("outcome: {outcome}")),
            "{action}: {shown}"
        );
        if action == "declared_failure" {
            assert!(shown.contains("hook_completion_status: failure"), "{shown}");
        } else if outcome == "failed" {
            assert!(shown.contains("primary_failure: "), "{action}: {shown}");
        }
    }
}

// Test-ID: PR-TEST-0153
// Verifies: PR-REQ-0050, PR-REQ-0052, PR-REQ-0097, PR-REQ-0212, PR-REQ-0216, PR-REQ-0281
#[test]
fn accepted_late_completion_keeps_timeout_outcome_but_publishes_eligible_artifacts() {
    let scenario = ready_matrix_scenario(0x153);
    let invoked = scenario.run([
        "invoke",
        "node",
        "late_timeout",
        "--action-timeout-ms",
        "500",
        "--termination-grace-ms",
        "500",
    ]);
    assert_exit(&invoked, 1);
    let id = run_id(&invoked);
    let shown = scenario.run(["run", "show", &id]);
    assert_success(&shown);
    let text = String::from_utf8_lossy(&shown.stdout);
    assert!(text.contains("outcome: timed_out"));
    assert!(text.contains("artifact: report\tbytes: 13"));
}

// Test-ID: PR-TEST-0171
// Verifies: PR-REQ-0050, PR-REQ-0052, PR-REQ-0097, PR-REQ-0216, PR-REQ-0281
#[test]
fn rejected_completion_control_after_timeout_publishes_no_artifact() {
    let scenario = ready_matrix_scenario(0x171);
    let invoked = scenario.run([
        "invoke",
        "node",
        "late_timeout_unaccepted",
        "--action-timeout-ms",
        "500",
        "--termination-grace-ms",
        "500",
    ]);
    assert_exit(&invoked, 1);
    scenario.wait_for_marker("completion-rejected-control");
    let shown = scenario.run(["run", "show", &run_id(&invoked)]);
    assert_success(&shown);
    let shown = String::from_utf8_lossy(&shown.stdout);
    assert!(shown.contains("phase: finished"));
    assert!(shown.contains("outcome: timed_out"));
    assert!(!shown.contains("hook_completion_status:"));
    assert!(!shown.contains("artifact: "));
}

// Test-ID: PR-TEST-0154
// Verifies: PR-REQ-0098, PR-REQ-0285
#[test]
fn parameter_projection_is_redacted_but_hook_explanations_have_explicit_provenance() {
    let scenario = ready_matrix_scenario(0x154);
    let canary = "secret-param-canary";
    let planned = scenario.run([
        "invoke",
        "node",
        "sensitive",
        "--plan",
        "--param",
        "secret=secret-param-canary",
    ]);
    assert_success(&planned);
    assert!(!String::from_utf8_lossy(&planned.stdout).contains(canary));
    let invoked = scenario.run([
        "invoke",
        "node",
        "sensitive",
        "--param",
        "secret=secret-param-canary",
    ]);
    assert_success(&invoked);
    assert!(!String::from_utf8_lossy(&invoked.stdout).contains(canary));
    // A deliberately misbehaving Hook echoes a protected value. Validation
    // is not universal taint tracking; ordinary parameter projections stay redacted.
    let live = String::from_utf8_lossy(&invoked.stderr);
    assert!(live.contains(canary) && live.contains("hook event:"));
    for line in live
        .lines()
        .filter(|line| !line.starts_with("hook message:"))
    {
        assert!(!line.contains(canary));
    }
    let shown = scenario.run(["run", "show", &run_id(&invoked)]);
    assert_success(&shown);
    let text = String::from_utf8_lossy(&shown.stdout);
    assert!(text.contains("hook_completion_text: withheld"));
    assert!(text.contains(canary));
    assert!(text.contains("sensitive Hook free text"));
    let unretained = scenario.run([
        "invoke",
        "node",
        "sensitive",
        "--param",
        "secret=secret-param-canary",
        "--no-retain-hook-text",
    ]);
    assert_success(&unretained);
    assert!(String::from_utf8_lossy(&unretained.stderr).contains(canary));
    let private = scenario.run(["run", "show", &run_id(&unretained)]);
    assert_success(&private);
    let private = String::from_utf8_lossy(&private.stdout);
    assert!(!private.contains(canary));
    assert!(private.contains("retention disabled"));
    let original = scenario.run(["run", "show", &run_id(&invoked)]);
    assert!(String::from_utf8_lossy(&original.stdout).contains(canary));
}

// Test-ID: PR-TEST-0175
// Verifies: PR-REQ-0098, PR-REQ-0284, PR-REQ-0285
#[test]
fn secret_inputs_and_protected_sources_remain_redacted_from_all_action_and_run_projections() {
    let scenario = ready_matrix_scenario(0x175);
    let config_canary = "protected-source-canary";
    let secret_input_canary = "secret-input-canary";
    let parameter_canary = "secret-param-canary";
    set_config(&scenario, config_canary.as_bytes());
    set_input(
        &scenario,
        "node",
        "secret_input",
        secret_input_canary.as_bytes(),
    );

    let planned = scenario.run([
        "invoke",
        "node",
        "protected_inputs",
        "--plan",
        "--param",
        "secret=secret-param-canary",
    ]);
    assert_success(&planned);
    let planned = String::from_utf8_lossy(&planned.stdout);
    for canary in [config_canary, secret_input_canary, parameter_canary] {
        assert!(!planned.contains(canary), "plan leaked {canary}");
    }

    let invoked = scenario.run([
        "invoke",
        "node",
        "protected_inputs",
        "--param",
        "secret=secret-param-canary",
    ]);
    assert_success(&invoked);
    scenario.wait_for_marker("protected-values-verified");
    let invoke_stdout = String::from_utf8_lossy(&invoked.stdout);
    let invoke_stderr = String::from_utf8_lossy(&invoked.stderr);
    for canary in [config_canary, secret_input_canary, parameter_canary] {
        assert!(
            !invoke_stdout.contains(canary),
            "invoke stdout leaked {canary}"
        );
        assert!(
            !invoke_stderr.contains(canary),
            "invoke stderr leaked {canary}"
        );
    }
    let listed = scenario.run(["run", "list", "node", "--no-trunc"]);
    assert_success(&listed);
    let shown = scenario.run(["run", "show", &run_id(&invoked)]);
    assert_success(&shown);
    let listed = String::from_utf8_lossy(&listed.stdout);
    let shown = String::from_utf8_lossy(&shown.stdout);
    for canary in [config_canary, secret_input_canary, parameter_canary] {
        assert!(!listed.contains(canary), "run list leaked {canary}");
        assert!(!shown.contains(canary), "run show leaked {canary}");
    }
    assert!(shown.contains("hook_completion_text: withheld"));
    assert!(shown.contains("protected context verified"));
}

// Test-ID: PR-TEST-0155
// Verifies: PR-REQ-0172, PR-REQ-0280
#[test]
fn terminal_contract_keeps_none_isolated_and_streams_output_terminal_channels() {
    let scenario = ready_matrix_scenario(0x155);
    let none = scenario.invoke("node", "terminal_none");
    assert_success(&none);
    assert!(!String::from_utf8_lossy(&none.stdout).contains("system-hook-terminal-"));
    assert!(!String::from_utf8_lossy(&none.stderr).contains("system-hook-terminal-"));
    let output = scenario.invoke("node", "terminal_output");
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("system-hook-terminal-stdout"));
    assert!(!stdout.contains("system-hook-terminal-stderr"));
    assert!(stderr.contains("system-hook-terminal-stderr"));
    assert!(!stderr.contains("system-hook-terminal-stdout"));
    for protocol_text in ["pactrun.hook-protocol", "session_start", "session_ready"] {
        assert!(
            !stdout.contains(protocol_text),
            "stdout contained protocol traffic"
        );
        assert!(
            !stderr.contains(protocol_text),
            "stderr contained protocol traffic"
        );
    }
}

// Test-ID: PR-TEST-0156
// Verifies: PR-REQ-0057, PR-REQ-0097, PR-REQ-0110, PR-REQ-0215, PR-REQ-0284, PR-REQ-0287
#[test]
fn recovery_risk_blocks_ordinary_invocation_until_explicit_resolution() {
    let scenario = ready_matrix_scenario(0x156);
    let cleared = scenario.invoke("node", "risk");
    assert_success(&cleared);
    let opened = scenario.invoke("node", "risk_open_failure");
    assert_exit(&opened, 1);
    let opened_id = run_id(&opened);
    let shown = scenario.run(["run", "show", &opened_id]);
    assert_success(&shown);
    assert!(String::from_utf8_lossy(&shown.stdout).contains("current_recovery_guard: "));
    assert!(!String::from_utf8_lossy(&shown.stdout).contains("current_recovery_guard: none"));
    let before = scenario.hook_launches();
    let refused = scenario.invoke("node", "fast_observe");
    assert_exit(&refused, 1);
    assert_eq!(scenario.hook_launches(), before);
    let override_run = scenario.run([
        "invoke",
        "node",
        "fast_observe",
        "--authorize-recovery-override",
    ]);
    assert_success(&override_run);
    let state = scenario.run(["instance", "show", "node"]);
    assert_success(&state);
    let state = instance_projection(&state).state_version;
    let resolved = scenario.run([
        "instance",
        "resolve-manual-recovery",
        "node",
        "--if-version",
        &state,
    ]);
    assert_success(&resolved);
    let normal = scenario.invoke("node", "fast_observe");
    assert_success(&normal);
    let invalid_success = scenario.invoke("node", "risk_open_success");
    assert_exit(&invalid_success, 1);
    let invalid_success_id = run_id(&invalid_success);
    let invalid_success = scenario.run(["run", "show", &invalid_success_id]);
    assert_success(&invalid_success);
    let invalid_success = String::from_utf8_lossy(&invalid_success.stdout);
    assert!(!invalid_success.contains("outcome: succeeded"));
    assert!(invalid_success.contains("terminal_risk: open"));
    let runs = scenario.run(["run", "list", "node", "--no-trunc"]);
    assert_success(&runs);
    assert!(run_projections(&runs).iter().any(|run| run.id == opened_id));
}

// Test-ID: PR-TEST-0157
// Verifies: PR-REQ-0048
#[test]
fn observe_execution_keeps_a_pinned_binding_view_while_management_and_mutate_continue() {
    let scenario = ready_matrix_scenario(0x157);
    let first = scenario.spawn(["invoke", "node", "hold_observe"]);
    scenario.wait_for_marker("binding:before:old");
    scenario.wait_for_marker("ready:hold_observe");
    let second_observe = scenario.invoke("node", "fast_observe");
    assert_success(&second_observe);
    let mutate = scenario.invoke("node", "fast_mutate");
    assert_success(&mutate);
    set_config(&scenario, b"new");
    scenario.release_hook();
    let first = first.wait();
    assert_success(&first);
    scenario.wait_for_marker("binding:after:old");
    let fresh = scenario.invoke("node", "hold_observe");
    assert_success(&fresh);
    scenario.wait_for_marker("binding:before:new");
    scenario.wait_for_marker("binding:after:new");
}

// Test-ID: PR-TEST-0158
// Verifies: PR-REQ-0049
#[test]
fn mutate_conflict_creates_a_failed_run_without_launching_a_second_hook() {
    let scenario = ready_matrix_scenario(0x158);
    let first = scenario.spawn(["invoke", "node", "hold_mutate"]);
    scenario.wait_for_marker("ready:hold_mutate");
    let launches = scenario.hook_launches();
    let conflict = scenario.invoke("node", "hold_mutate");
    assert_exit(&conflict, 1);
    assert_eq!(scenario.hook_launches(), launches);
    let observe = scenario.invoke("node", "fast_observe");
    assert_success(&observe);
    scenario.release_hook();
    assert_success(&first.wait());
    let runs = scenario.run(["run", "list", "node", "--no-trunc"]);
    assert_success(&runs);
    assert!(
        run_projections(&runs)
            .iter()
            .any(|run| run.action == "hold_mutate" && run.outcome == "failed")
    );
}

// Test-ID: PR-TEST-0172
// Verifies: PR-REQ-0045, PR-REQ-0049, PR-REQ-0278, PR-REQ-0284
#[test]
fn mutation_conflict_is_exact_instance_scoped_and_cannot_be_bypassed_by_recovery_override() {
    let scenario = ready_matrix_scenario(0x172);
    let revision = instance_revision(&scenario, "node");
    let other = scenario.create_instance("other", &revision);
    assert_success(&other);
    set_input(&scenario, "other", "config", b"other");

    let first = scenario.spawn(["invoke", "node", "hold_mutate"]);
    scenario.wait_for_marker("ready:hold_mutate");
    let launches = scenario.hook_launches();
    for override_arguments in [Vec::new(), vec!["--authorize-recovery-override"]] {
        let mut arguments = vec!["invoke", "node", "hold_mutate"];
        arguments.extend(override_arguments);
        let conflict = scenario.run(arguments);
        assert_exit(&conflict, 1);
        assert!(
            String::from_utf8_lossy(&conflict.stderr).contains("admission.mutation_conflict"),
            "{}",
            String::from_utf8_lossy(&conflict.stderr)
        );
        let runs = scenario.run(["run", "list", "node", "--no-trunc"]);
        assert_success(&runs);
        let failed = run_projections(&runs)
            .into_iter()
            .find(|run| run.action == "hold_mutate" && run.outcome == "failed")
            .expect("admission refusal must create a failed Run");
        let shown = scenario.run(["run", "show", &failed.id]);
        assert_success(&shown);
        let shown = String::from_utf8_lossy(&shown.stdout);
        assert!(shown.contains("outcome: failed"));
        assert!(shown.contains("primary_failure: admission:mutation_conflict\tstep: admission"));
        assert_eq!(scenario.hook_launches(), launches);
    }
    let observe = scenario.invoke("node", "fast_observe");
    assert_success(&observe);
    let other_mutate = scenario.invoke("other", "fast_mutate");
    assert_success(&other_mutate);
    scenario.release_hook();
    assert_success(&first.wait());
}

// Test-ID: PR-TEST-0173
// Verifies: PR-REQ-0057, PR-REQ-0110, PR-REQ-0119, PR-REQ-0215, PR-REQ-0284, PR-REQ-0287
#[test]
fn recovery_override_leaves_guard_intact_and_manual_resolution_requires_a_fresh_token_without_execution()
 {
    let scenario = ready_matrix_scenario(0x173);
    let opened = scenario.invoke("node", "risk_open_failure");
    assert_exit(&opened, 1);
    let stale = scenario.run(["instance", "show", "node"]);
    assert_success(&stale);
    let stale = instance_projection(&stale).state_version;
    let initial_runs = scenario.run(["run", "list", "node", "--no-trunc"]);
    assert_success(&initial_runs);
    let launches = scenario.hook_launches();

    let overridden = scenario.run([
        "invoke",
        "node",
        "fast_observe",
        "--authorize-recovery-override",
    ]);
    assert_success(&overridden);
    let guarded = scenario.run(["run", "show", &run_id(&opened)]);
    assert_success(&guarded);
    assert!(!String::from_utf8_lossy(&guarded.stdout).contains("current_recovery_guard: none"));
    set_config(&scenario, b"state-version-after-guard");

    let before_stale_resolution = scenario.run(["run", "list", "node", "--no-trunc"]);
    assert_success(&before_stale_resolution);
    let stale_resolution = scenario.run([
        "instance",
        "resolve-manual-recovery",
        "node",
        "--if-version",
        &stale,
    ]);
    assert_exit(&stale_resolution, 1);
    let after_stale_resolution = scenario.run(["run", "list", "node", "--no-trunc"]);
    assert_success(&after_stale_resolution);
    assert_eq!(
        run_count(&before_stale_resolution),
        run_count(&after_stale_resolution)
    );
    let fresh = scenario.run(["instance", "show", "node"]);
    assert_success(&fresh);
    let fresh = instance_projection(&fresh).state_version;
    let before_resolutions = scenario.run(["run", "list", "node", "--no-trunc"]);
    assert_success(&before_resolutions);
    let resolved = scenario.run([
        "instance",
        "resolve-manual-recovery",
        "node",
        "--if-version",
        &fresh,
    ]);
    assert_success(&resolved);
    let after_resolutions = scenario.run(["run", "list", "node", "--no-trunc"]);
    assert_success(&after_resolutions);
    assert_eq!(
        run_count(&before_resolutions),
        run_count(&after_resolutions)
    );
    assert_eq!(scenario.hook_launches(), launches + 1);
    assert!(run_count(&after_resolutions) > run_count(&initial_runs));
    let cleared = scenario.run(["run", "show", &run_id(&opened)]);
    assert_success(&cleared);
    assert!(String::from_utf8_lossy(&cleared.stdout).contains("current_recovery_guard: none"));
}

fn marker_run_id(scenario: &Scenario) -> String {
    scenario
        .wait_for_marker("run:")
        .strip_prefix("run:")
        .unwrap()
        .to_owned()
}

// Test-ID: PR-TEST-0159
// Verifies: PR-REQ-0050, PR-REQ-0057, PR-REQ-0058, PR-REQ-0097, PR-REQ-0215, PR-REQ-0284, PR-REQ-0287
#[test]
fn reconciliation_only_terminalizes_confirmed_orphans_and_preserves_risk_consequences() {
    let live = ready_matrix_scenario(0x159);
    let live_owner = live.spawn(["invoke", "node", "hold_observe"]);
    let live_run = marker_run_id(&live);
    live.wait_for_marker("ready:hold_observe");
    let reconcile = live.run(["run", "reconcile"]);
    assert_success(&reconcile);
    let shown = live.run(["run", "show", &live_run]);
    assert_success(&shown);
    assert!(String::from_utf8_lossy(&shown.stdout).contains("phase: running"));
    live.release_hook();
    assert_success(&live_owner.wait());

    let clear = ready_matrix_scenario(0x15a);
    let clear_owner = clear.spawn(["invoke", "node", "hold_observe"]);
    let clear_run = marker_run_id(&clear);
    clear.wait_for_marker("ready:hold_observe");
    let _ = clear_owner.terminate();
    let before = clear.run(["run", "show", &clear_run]);
    assert_success(&before);
    assert!(String::from_utf8_lossy(&before.stdout).contains("phase: running"));
    assert_success(&clear.run(["run", "reconcile"]));
    let after = clear.run(["run", "show", &clear_run]);
    assert_success(&after);
    let after = String::from_utf8_lossy(&after.stdout);
    assert!(after.contains("outcome: interrupted"));
    assert!(after.contains("current_recovery_guard: none"));

    let risky = ready_matrix_scenario(0x15b);
    let risky_owner = risky.spawn(["invoke", "node", "hold_risk"]);
    let risky_run = marker_run_id(&risky);
    risky.wait_for_marker("ready:hold_risk");
    let _ = risky_owner.terminate();
    let before = risky.run(["run", "show", &risky_run]);
    assert_success(&before);
    assert!(String::from_utf8_lossy(&before.stdout).contains("phase: running"));
    assert_success(&risky.run(["run", "reconcile"]));
    let after = risky.run(["run", "show", &risky_run]);
    assert_success(&after);
    let after = String::from_utf8_lossy(&after.stdout);
    assert!(after.contains("outcome: interrupted"));
    assert!(after.contains("terminal_risk: open"));
    assert!(!after.contains("current_recovery_guard: none"));
}

// Test-ID: PR-TEST-0174
// Verifies: PR-REQ-0050, PR-REQ-0057, PR-REQ-0058, PR-REQ-0284, PR-REQ-0287
#[test]
fn repeated_reconcile_is_idempotent_and_does_not_launch_hooks() {
    let scenario = ready_matrix_scenario(0x174);
    let owner = scenario.spawn(["invoke", "node", "hold_observe"]);
    let run = marker_run_id(&scenario);
    scenario.wait_for_marker("ready:hold_observe");
    let launches = scenario.hook_launches();
    let _ = owner.terminate();
    assert_success(&scenario.run(["run", "reconcile"]));
    let terminal = scenario.run(["run", "show", &run]);
    assert_success(&terminal);
    let terminal = String::from_utf8(terminal.stdout).unwrap();
    assert!(terminal.contains("phase: finished"));
    assert!(terminal.contains("outcome: interrupted"));
    assert_success(&scenario.run(["run", "reconcile"]));
    let repeated = scenario.run(["run", "show", &run]);
    assert_success(&repeated);
    assert_eq!(String::from_utf8(repeated.stdout).unwrap(), terminal);
    assert_eq!(scenario.hook_launches(), launches);
}
