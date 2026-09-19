use std::fs;

use super::support::{
    Scenario, assert_exit, assert_success, basic_source, command, instance_list_projections,
    instance_projection, run_command, run_count, run_details_projection, run_id, run_projections,
    startup_command,
};

// Test-ID: PR-TEST-0142
// Verifies: PR-REQ-0284
#[test]
fn cli_startup_commands_do_not_require_or_create_storage() {
    let temporary = tempfile::tempdir().unwrap();
    let absent_storage = temporary.path().join("absent-storage");
    for arguments in [
        vec!["--help"],
        vec!["--version"],
        vec!["pack", "generate-id"],
    ] {
        let output = super::support::run_command(startup_command(temporary.path(), arguments));
        assert_success(&output);
        assert!(!absent_storage.exists());
        assert!(!temporary.path().join(".pactrun").exists());
    }
    let output = run_command(command(
        &absent_storage,
        temporary.path(),
        ["instance", "list"],
    ));
    assert_exit(&output, 1);
    assert!(!absent_storage.exists());
    let output = run_command(command(&absent_storage, temporary.path(), ["--unknown"]));
    assert_exit(&output, 2);
}

// Test-ID: PR-TEST-0143
// Verifies: PR-REQ-0004, PR-REQ-0005, PR-REQ-0094, PR-REQ-0097
#[test]
fn installed_runtime_survives_source_removal_and_run_is_visible_to_a_new_process() {
    let source = basic_source().replace(
        "inputs: []",
        "inputs:\n    - { id: config, required: true, protection: normal }",
    );
    let scenario = Scenario::new(0x142, &source);
    let revision = scenario.install();
    let before_create = scenario.run(["instance", "list"]);
    assert_success(&before_create);
    assert!(instance_list_projections(&before_create).is_empty());
    let created = scenario.create_instance("node", &revision);
    assert_success(&created);
    let incomplete = scenario.run(["instance", "show", "node"]);
    assert_success(&incomplete);
    let incomplete = instance_projection(&incomplete);
    assert!(!incomplete.ready);
    let listed = scenario.run(["instance", "list"]);
    assert_success(&listed);
    let listed = instance_list_projections(&listed);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "node");
    let input = scenario.path("config");
    fs::write(&input, b"configured").unwrap();
    let provisioned = scenario.run([
        std::ffi::OsStr::new("input"),
        std::ffi::OsStr::new("set"),
        std::ffi::OsStr::new("node"),
        std::ffi::OsStr::new("config"),
        std::ffi::OsStr::new("--file"),
        input.as_os_str(),
    ]);
    assert_success(&provisioned);
    let ready = scenario.run(["instance", "show", "node"]);
    assert_success(&ready);
    let ready = instance_projection(&ready);
    assert!(ready.ready);
    assert_ne!(ready.state_version, incomplete.state_version);
    fs::remove_dir_all(&scenario.source).unwrap();
    let mut run_ids = Vec::new();
    for action in ["observe", "mutate"] {
        let invoked = scenario.invoke("node", action);
        assert_success(&invoked);
        let id = run_id(&invoked);
        let shown = scenario.run(["run", "show", &id]);
        assert_success(&shown);
        let run = run_details_projection(&shown);
        assert_eq!(run.action, action);
        assert_eq!(run.outcome, "succeeded");
        run_ids.push(id);
    }
    let listed_runs = scenario.run(["run", "list", "node"]);
    assert_success(&listed_runs);
    let listed_runs = run_projections(&listed_runs);
    assert!(
        run_ids
            .iter()
            .all(|id| listed_runs.iter().any(|run| run.id == *id))
    );
    assert!(listed_runs.iter().all(
        |run| matches!(run.action.as_str(), "observe" | "mutate") && run.outcome == "succeeded"
    ));
    assert_eq!(scenario.hook_launches(), 2);
}

// Test-ID: PR-TEST-0147
// Verifies: PR-REQ-0265, PR-REQ-0266, PR-REQ-0268, PR-REQ-0092
#[test]
fn input_and_secret_management_preserve_bytes_and_disclosure_boundaries() {
    let source = basic_source().replace(
        "inputs: []",
        "inputs:\n    - { id: normal, required: false, protection: normal }\n    - { id: stdin, required: false, protection: normal }\n    - { id: secret, required: false, protection: secret }",
    );
    let scenario = Scenario::new(0x147, &source);
    scenario.install_and_create("node");
    let initial = scenario.run(["instance", "show", "node"]);
    assert_success(&initial);
    let initial_version = instance_projection(&initial).state_version;

    let normal = b"normal\0bytes\r\n";
    let normal_source = scenario.path("normal-source");
    fs::write(&normal_source, normal).unwrap();
    let set_normal = scenario.run([
        std::ffi::OsStr::new("input"),
        std::ffi::OsStr::new("set"),
        std::ffi::OsStr::new("node"),
        std::ffi::OsStr::new("normal"),
        std::ffi::OsStr::new("--file"),
        normal_source.as_os_str(),
    ]);
    assert_success(&set_normal);

    let stdin = b"stdin\xef\xbb\xbf\n".to_vec();
    let set_stdin =
        scenario.run_with_stdin(["input", "set", "node", "stdin", "--stdin"], stdin.clone());
    assert_success(&set_stdin);

    let normal_export = scenario.path("normal-export");
    let exported = scenario.run([
        std::ffi::OsStr::new("input"),
        std::ffi::OsStr::new("export"),
        std::ffi::OsStr::new("node"),
        std::ffi::OsStr::new("normal"),
        std::ffi::OsStr::new("--output"),
        normal_export.as_os_str(),
    ]);
    assert_success(&exported);
    assert_eq!(fs::read(&normal_export).unwrap(), normal);
    let stdin_export = scenario.path("stdin-export");
    let exported = scenario.run([
        std::ffi::OsStr::new("input"),
        std::ffi::OsStr::new("export"),
        std::ffi::OsStr::new("node"),
        std::ffi::OsStr::new("stdin"),
        std::ffi::OsStr::new("--output"),
        stdin_export.as_os_str(),
    ]);
    assert_success(&exported);
    assert_eq!(fs::read(&stdin_export).unwrap(), stdin);

    let replacement = scenario.path("replacement");
    fs::write(&replacement, b"replacement").unwrap();
    let stale = scenario.run([
        std::ffi::OsStr::new("input"),
        std::ffi::OsStr::new("set"),
        std::ffi::OsStr::new("node"),
        std::ffi::OsStr::new("normal"),
        std::ffi::OsStr::new("--file"),
        replacement.as_os_str(),
        std::ffi::OsStr::new("--if-version"),
        std::ffi::OsStr::new(&initial_version),
    ]);
    assert_exit(&stale, 1);
    let after_stale_set = scenario.path("after-stale-set");
    let exported = scenario.run([
        std::ffi::OsStr::new("input"),
        std::ffi::OsStr::new("export"),
        std::ffi::OsStr::new("node"),
        std::ffi::OsStr::new("normal"),
        std::ffi::OsStr::new("--output"),
        after_stale_set.as_os_str(),
    ]);
    assert_success(&exported);
    assert_eq!(fs::read(&after_stale_set).unwrap(), normal);
    let stale_delete = scenario.run([
        std::ffi::OsStr::new("input"),
        std::ffi::OsStr::new("delete"),
        std::ffi::OsStr::new("node"),
        std::ffi::OsStr::new("normal"),
        std::ffi::OsStr::new("--if-version"),
        std::ffi::OsStr::new(&initial_version),
    ]);
    assert_exit(&stale_delete, 1);
    let after_stale_delete = scenario.path("after-stale-delete");
    let exported = scenario.run([
        std::ffi::OsStr::new("input"),
        std::ffi::OsStr::new("export"),
        std::ffi::OsStr::new("node"),
        std::ffi::OsStr::new("normal"),
        std::ffi::OsStr::new("--output"),
        after_stale_delete.as_os_str(),
    ]);
    assert_success(&exported);
    assert_eq!(fs::read(&after_stale_delete).unwrap(), normal);

    let secret = b"secret-canary";
    let secret_source = scenario.path("secret-source");
    fs::write(&secret_source, secret).unwrap();
    let set_secret = scenario.run([
        std::ffi::OsStr::new("input"),
        std::ffi::OsStr::new("set"),
        std::ffi::OsStr::new("node"),
        std::ffi::OsStr::new("secret"),
        std::ffi::OsStr::new("--file"),
        secret_source.as_os_str(),
    ]);
    assert_success(&set_secret);
    let secret_export = scenario.path("secret-export");
    let denied = scenario.run([
        std::ffi::OsStr::new("input"),
        std::ffi::OsStr::new("export"),
        std::ffi::OsStr::new("node"),
        std::ffi::OsStr::new("secret"),
        std::ffi::OsStr::new("--output"),
        secret_export.as_os_str(),
    ]);
    assert_exit(&denied, 1);
    assert!(!secret_export.exists());
    let authorized = scenario.run([
        std::ffi::OsStr::new("input"),
        std::ffi::OsStr::new("export"),
        std::ffi::OsStr::new("node"),
        std::ffi::OsStr::new("secret"),
        std::ffi::OsStr::new("--output"),
        secret_export.as_os_str(),
        std::ffi::OsStr::new("--authorize-secret-export"),
    ]);
    assert_success(&authorized);
    assert_eq!(fs::read(&secret_export).unwrap(), secret);
    let no_clobber = scenario.run([
        std::ffi::OsStr::new("input"),
        std::ffi::OsStr::new("export"),
        std::ffi::OsStr::new("node"),
        std::ffi::OsStr::new("secret"),
        std::ffi::OsStr::new("--output"),
        secret_export.as_os_str(),
        std::ffi::OsStr::new("--authorize-secret-export"),
    ]);
    assert_exit(&no_clobber, 1);
    assert_eq!(fs::read(&secret_export).unwrap(), secret);
}

// Test-ID: PR-TEST-0144
// Verifies: PR-REQ-0039, PR-REQ-0095, PR-REQ-0284
#[test]
fn action_inspection_and_plan_are_read_only_and_do_not_launch_a_hook() {
    let scenario = Scenario::new(0x144, basic_source());
    scenario.install_and_create("node");
    let actions = scenario.run(["action", "list", "node"]);
    assert_success(&actions);
    assert!(String::from_utf8_lossy(&actions.stdout).contains("observe"));
    let action = scenario.run(["action", "show", "node", "observe"]);
    assert_success(&action);
    let planned = scenario.run(["invoke", "node", "observe", "--plan"]);
    assert_success(&planned);
    let listed = scenario.run(["run", "list", "node"]);
    assert_success(&listed);
    assert_eq!(run_count(&listed), 0);
    assert_eq!(scenario.hook_launches(), 0);
}

// Test-ID: PR-TEST-0145
// Verifies: PR-REQ-0049, PR-REQ-0284, PR-REQ-0288
#[test]
fn pre_acceptance_errors_create_no_run_and_never_launch_the_hook() {
    let scenario = Scenario::new(0x145, basic_source());
    scenario.install_and_create("node");
    for arguments in [
        vec!["invoke", "missing", "observe"],
        vec!["invoke", "node", "missing"],
        vec!["invoke", "node", "observe", "--param", "missing=value"],
        vec![
            "invoke",
            "node",
            "observe",
            "--param-file",
            "missing=does-not-exist",
        ],
        vec![
            "invoke",
            "node",
            "observe",
            "--action-timeout-ms",
            "18446744073709551615",
        ],
    ] {
        let output = scenario.run(arguments);
        assert!(matches!(output.status.code(), Some(1 | 2)));
        let listed = scenario.run(["run", "list", "node"]);
        assert_success(&listed);
        assert_eq!(run_count(&listed), 0);
        assert_eq!(scenario.hook_launches(), 0);
    }
}

// Test-ID: PR-TEST-0146
// Verifies: PR-REQ-0094, PR-REQ-0097, PR-REQ-0285
#[test]
fn declared_hook_failure_is_durable_and_its_explanation_is_attributed() {
    let scenario = Scenario::new(0x146, basic_source());
    scenario.install_and_create("node");
    let invoked = scenario.invoke("node", "failure");
    assert_exit(&invoked, 1);
    let id = run_id(&invoked);
    let shown = scenario.run(["run", "show", &id]);
    assert_success(&shown);
    let run = run_details_projection(&shown);
    assert_eq!(run.outcome, "failed");
    let view = String::from_utf8_lossy(&shown.stdout);
    assert!(view.contains("fixture failure"));
    assert!(view.contains("kind=completion"));
    assert!(view.contains("hook completion: failure"));
}
