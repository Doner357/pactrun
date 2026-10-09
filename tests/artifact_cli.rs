//! Exercise the real CLI authorization boundary before any storage bootstrap.
use std::{fs, path::Path, process::Command};

// Test-ID: PR-TEST-0455
// Verifies: PR-REQ-0342, PR-REQ-0344
#[test]
fn artifact_cli_process_rejects_implicit_disclosure_before_storage_open() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/artifact-process-tests");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let root = temp.path().join("uninitialized");
    let destination = temp.path().join("not-published");
    let executable = env!("CARGO_BIN_EXE_pactrun");
    let call = |args: &[&str]| {
        Command::new(executable)
            .args(args)
            .env("PACTRUN_STORAGE_ROOT", &root)
            .output()
            .unwrap()
    };
    let run = "00000000000000000000000000000001";
    let output = call(&[
        "run",
        "artifact",
        "export",
        run,
        "report",
        "--output",
        destination.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--authorize-sensitive-export"));
    let invalid = call(&[
        "run",
        "artifact",
        "export",
        run,
        "report",
        "--output",
        "-",
        "--authorize-sensitive-export",
    ]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(!root.exists());
    assert!(!destination.exists());
    let help = call(&["run", "artifact", "export", "--help"]);
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("pactrun run artifact export"));
    let help = call(&["run", "artifact", "delete", "--help"]);
    assert!(help.status.success());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("pactrun run artifact delete")
    );
}
