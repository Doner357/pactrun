//! Offline runner regressions and an opt-in real-manager gate.
use std::{env, path::Path, process::Command};

// Test-ID: PR-TEST-0637
// Verifies: PR-REQ-0329, PR-REQ-0330, PR-REQ-0332, PR-REQ-0333
#[test]
fn native_acceptance_runner_offline_regressions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(if cfg!(windows) { "python" } else { "python3" })
        .args(["-B", "-m", "unittest", "tools.test_native_package_acceptance"])
        .current_dir(root)
        .output()
        .expect("run offline native acceptance regressions");
    assert!(
        output.status.success(),
        "offline native acceptance regressions failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

// Test-ID: PR-TEST-0637
// Verifies: PR-REQ-0329, PR-REQ-0330, PR-REQ-0332, PR-REQ-0333
#[test]
#[ignore = "requires source-qualified native artifacts, isolated manager template and explicit Scoop PATH authorization"]
fn real_native_packages_preserve_baseline_objects_and_running_hooks() {
    let args: Vec<String> = serde_json::from_str(
        &env::var("PACTRUN_NATIVE_ACCEPTANCE_ARGS")
            .expect("supply JSON argv for tools/native_package_acceptance.py"),
    )
    .expect("JSON string array");
    if cfg!(windows) {
        assert_eq!(
            env::var("PACTRUN_NATIVE_ACCEPTANCE_ALLOW_USER_PATH").as_deref(),
            Ok("1"),
            "explicit authorization is required before temporary Scoop shim PATH changes"
        );
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(if cfg!(windows) { "python" } else { "python3" });
    command.args(["-B", "tools/native_package_acceptance.py"]);
    command.args(args).current_dir(root);
    if cfg!(windows) {
        command.arg("--allow-user-path-change");
    }
    assert!(
        command
            .status()
            .expect("run native qualification")
            .success()
    );
}
