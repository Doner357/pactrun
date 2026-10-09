use super::{
    shell_loader::{execute, helper_calls, shells, source},
    support::{Scenario, assert_success},
};
use serde_json::{Value, json};
use std::fs;

// Supporting coverage for PR-TEST-0690.
#[test]
fn core_facts_are_retained_for_capture_restore_and_cleanup() {
    for (shell, executable) in shells() {
        let mut yaml = source(shell, executable, &[]);
        let hook = format!(
            "        protocol_version: 1.0-alpha.1\n        launch: {{kind: shell_loader, shell: {shell}, command: {executable}, script: script}}\n        args: []\n        io: {{terminal: none}}\n"
        );
        let capabilities = format!(
            "  snapshot:\n    capture:\n      parameters: []\n      access: observe\n      hook:\n{hook}    restore:\n      parameters: []\n      hook:\n{hook}  cleanup:\n    requires: []\n    hook:\n{}",
            hook.lines()
                .map(|l| format!("{}\n", &l[2..]))
                .collect::<String>()
        );
        yaml = yaml.replace(
            "runtime_content:",
            &format!("{capabilities}runtime_content:"),
        );
        let scenario = Scenario::new(795, &yaml);
        let script = if cfg!(windows) {
            "& $env:PACTRUN_EXECUTABLE hook parameter undeclared\nexit 0\n"
        } else {
            "\"$PACTRUN_EXECUTABLE\" hook parameter undeclared || :\nexit 0\n"
        };
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        scenario.install_and_create("sample");
        let capture = execute(
            &scenario,
            &[
                "--format",
                "json",
                "snapshot",
                "capture",
                "sample",
                "--no-retain-hook-text",
            ],
        );
        assert_success(&capture);
        let capture: Value = serde_json::from_slice(&capture.stdout).unwrap();
        let snapshot = capture["result"]["capture_result"].as_str().unwrap();
        let restore = execute(
            &scenario,
            &[
                "--format",
                "json",
                "snapshot",
                "restore",
                "sample",
                snapshot,
                "--no-retain-hook-text",
            ],
        );
        assert_success(&restore);
        let cleanup = execute(
            &scenario,
            &[
                "--format",
                "json",
                "instance",
                "delete",
                "sample",
                "--no-retain-hook-text",
            ],
        );
        assert_success(&cleanup);
        for result in [
            capture,
            serde_json::from_slice(&restore.stdout).unwrap(),
            serde_json::from_slice(&cleanup.stdout).unwrap(),
        ] {
            let id = result["result"]["run"]["run_id"].as_str().unwrap();
            let output = execute(&scenario, &["--format", "json", "run", "show", id]);
            assert_success(&output);
            let history: Value = serde_json::from_slice(&output.stdout).unwrap();
            let facts = &history["result"]["diagnostics"]["pactrun"];
            assert_eq!(history["result"]["run"]["state"]["outcome"], "succeeded");
            assert_eq!(facts["observed"], "1", "{history}");
            assert_eq!(facts["collection_closed"], true);
            assert_eq!(
                facts["events"][0]["failure"]["reason"],
                "parameter_unavailable"
            );
            assert_eq!(result["result"]["diagnostics"]["pactrun"], *facts);
            assert_eq!(history["result"]["diagnostics"]["retain_text"], false);
            assert!(
                history["result"]["diagnostics"]["events"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }
    }
}

// Supporting coverage for PR-TEST-0690.
#[test]
fn migration_core_facts_match_history_on_success_and_failure() {
    for (shell, executable) in shells() {
        for exit in [0, 1] {
            let scenario = Scenario::new(796, &source(shell, executable, &[]));
            fs::write(scenario.source.join("script.txt"), "exit 0\n").unwrap();
            let initial = scenario.install_and_create("sample");
            let digest = initial.rsplit('/').next().unwrap();
            let yaml = fs::read_to_string(scenario.source.join("pactrun.yaml")).unwrap();
            let migration = format!(
                "  migrations:\n    - source_revision_digest: '{digest}'\n      transitions: []\n      requires_source: []\n      requires_target: []\n      produces_target: []\n      hook:\n        protocol_version: 1.0-alpha.1\n        launch: {{kind: shell_loader, shell: {shell}, command: {executable}, script: script}}\n        args: []\n        io: {{terminal: none}}\n"
            );
            fs::write(
                scenario.source.join("pactrun.yaml"),
                yaml.replace("runtime_content:", &format!("{migration}runtime_content:")),
            )
            .unwrap();
            let script = if cfg!(windows) {
                format!("& $env:PACTRUN_EXECUTABLE hook parameter undeclared\nexit {exit}\n")
            } else {
                format!("\"$PACTRUN_EXECUTABLE\" hook parameter undeclared || :\nexit {exit}\n")
            };
            fs::write(scenario.source.join("script.txt"), script).unwrap();
            let target = scenario.install();
            let output = execute(
                &scenario,
                &[
                    "--format",
                    "json",
                    "instance",
                    "migrate",
                    "sample",
                    "--to",
                    &target,
                    "--no-retain-hook-text",
                ],
            );
            assert_eq!(
                output.status.code(),
                Some(exit),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let response: Value = serde_json::from_slice(&output.stdout).unwrap();
            let live = if exit == 0 {
                &response["result"]["inspection"]
            } else {
                &response["result"]
            };
            let run = live["run"]["run_id"].as_str().unwrap();
            let output = execute(&scenario, &["--format", "json", "run", "show", run]);
            assert_success(&output);
            let history: Value = serde_json::from_slice(&output.stdout).unwrap();
            let facts = &history["result"]["diagnostics"]["pactrun"];
            assert_eq!(facts["collection_closed"], true);
            assert_eq!(facts["observed"], "1");
            assert_eq!(
                facts["events"][0]["failure"]["reason"],
                "parameter_unavailable"
            );
            assert_eq!(live["diagnostics"]["pactrun"], *facts);
            assert_eq!(
                live["run"]["state"]["outcome"],
                if exit == 0 { "succeeded" } else { "failed" }
            );
        }
    }
}

// Supporting coverage for PR-TEST-0690.
#[test]
fn intentional_hook_protocol_error_is_not_a_core_helper_failure() {
    for (shell, executable) in shells() {
        let scenario = Scenario::new(792, &source(shell, executable, &[]));
        let path = scenario.path("protocol-error.json");
        fs::write(&path, serde_json::to_vec(&json!({"type":"protocol_error","code":"sdk_failure","message":"intentional Hook report"})).unwrap()).unwrap();
        fs::write(
            scenario.source.join("script.txt"),
            helper_calls(&[&format!("protocol-error --file '{}'", path.display())], 0),
        )
        .unwrap();
        scenario.install_and_create("sample");
        let result = execute(&scenario, &["--format", "json", "invoke", "sample", "run"]);
        assert!(!result.status.success());
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        let events = value["result"]["diagnostics"]["events"].as_array().unwrap();
        assert!(
            events.iter().any(|e| e["source"] == "hook"
                && e["kind"] == "protocol_error"
                && e["message"] == "intentional Hook report"),
            "{value}"
        );
        assert!(
            value["result"]["diagnostics"]["pactrun"]["events"]
                .as_array()
                .unwrap()
                .is_empty(),
            "{value}"
        );
    }
}

// Supporting coverage for PR-TEST-0693.
#[test]
fn unavailable_diagnostic_transport_does_not_poison_the_helper_session() {
    for (shell, executable) in shells() {
        let scenario = Scenario::new(793, &source(shell, executable, &[]));
        let path = scenario.path("absent.json");
        let script = if cfg!(windows) {
            format!(
                "$env:PACTRUN_INTERNAL_CORE_DIAGNOSTICS = 'disabled'\n& $env:PACTRUN_EXECUTABLE hook diagnostic --file '{}'\n& $env:PACTRUN_EXECUTABLE hook workspace | Out-Null\nexit $LASTEXITCODE\n",
                path.display()
            )
        } else {
            format!(
                "PACTRUN_INTERNAL_CORE_DIAGNOSTICS=disabled \"$PACTRUN_EXECUTABLE\" hook diagnostic --file '{}'\n\"$PACTRUN_EXECUTABLE\" hook workspace >/dev/null || exit 73\nexit 0\n",
                path.display()
            )
        };
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        scenario.install_and_create("sample");
        let result = execute(&scenario, &["--format", "json", "invoke", "sample", "run"]);
        assert_success(&result);
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(value["result"]["run"]["state"]["outcome"], "succeeded");
        assert_eq!(
            value["result"]["run"]["state"]["hook_completion_status"],
            "success"
        );
    }
}

// Supporting coverage for PR-TEST-0692.
#[test]
fn alpha3_real_run_history_survives_upgrade_without_invented_core_events() {
    let (shell, executable) = shells()[0];
    let scenario = Scenario::new(794, &source(shell, executable, &[]));
    fs::write(scenario.source.join("script.txt"), "exit 0\n").unwrap();
    scenario.install_and_create("sample");
    let output = execute(&scenario, &["--format", "json", "invoke", "sample", "run"]);
    assert_success(&output);
    let before: Value = serde_json::from_slice(&output.stdout).unwrap();
    let run = before["result"]["run"]["run_id"].as_str().unwrap();
    let db = rusqlite::Connection::open(scenario.storage.join("database/pactrun.sqlite3")).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; DROP TABLE run_core_diagnostic_events; DROP TABLE run_core_diagnostic_collections; DROP TABLE writable_admissions;").unwrap();
    let old = include_str!("../../src/persistence/persistence_alpha3.sql");
    let admission = old
        .split_once("CREATE TABLE writable_admissions (")
        .unwrap()
        .1
        .split_once(';')
        .unwrap()
        .0;
    db.execute_batch(&format!("CREATE TABLE writable_admissions ({admission}; UPDATE pactrun_metadata SET format_version='1.0-alpha.3'; PRAGMA user_version=2;")).unwrap();
    drop(db);
    let output = execute(&scenario, &["--format", "json", "run", "show", run]);
    assert_success(&output);
    let after: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(after["result"]["run"], before["result"]["run"]);
    assert_eq!(
        after["result"]["diagnostics"]["events"],
        before["result"]["diagnostics"]["events"]
    );
    assert!(after["result"]["diagnostics"]["pactrun"].is_null());
}
