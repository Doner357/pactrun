//! Scoped help uses the normal CLI renderer, including helper help without a Session.
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};

// Supporting coverage for PR-TEST-0649.
#[test]
fn real_help_scopes_are_available_in_all_formats_without_storage_or_session() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/help-process-tests");
    fs::create_dir_all(&parent).unwrap();
    let temp = tempfile::tempdir_in(parent).unwrap();
    let root = temp.path().join("absent");
    for scope in [
        vec![],
        vec!["instance"],
        vec!["instance", "create"],
        vec!["invoke"],
        vec!["hook"],
        vec!["hook", "parameter"],
        vec!["hook", "risk", "enter"],
    ] {
        let mut human = None;
        for format in ["human", "json", "jsonl"] {
            let output = Command::new(env!("CARGO_BIN_EXE_pactrun"))
                .args(["--format", format])
                .args(&scope)
                .arg("--help")
                .env("PACTRUN_STORAGE_ROOT", &root)
                .env_remove("PACTRUN_SHELL_HELPER_ENDPOINT")
                .stdin(Stdio::null())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{scope:?}/{format}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stderr.is_empty());
            let text = String::from_utf8(output.stdout).unwrap();
            if format == "human" {
                human = Some(text);
            } else {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                let response = if format == "jsonl" {
                    &value["response"]
                } else {
                    &value
                };
                assert_eq!(response["command"], "help");
                assert_eq!(response["result"]["scope"], serde_json::json!(scope));
                assert_eq!(
                    response["result"]["usage"],
                    human.as_ref().unwrap().as_str()
                );
            }
            assert!(!root.exists());
        }
    }
    // Unprefixed helper help must also bypass the Session-only execution path.
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_pactrun"))
            .args(["hook", flag])
            .env_remove("PACTRUN_SHELL_HELPER_ENDPOINT")
            .env("PACTRUN_STORAGE_ROOT", &root)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("pactrun hook"));
        assert!(!root.exists());
    }
    for format in ["human", "json", "jsonl"] {
        for scope in [
            vec!["hook", "unknown", "--help"],
            vec!["hook", "--help", "extra"],
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_pactrun"))
                .args(["--format", format])
                .args(scope)
                .env_remove("PACTRUN_SHELL_HELPER_ENDPOINT")
                .env("PACTRUN_STORAGE_ROOT", &root)
                .stdin(Stdio::null())
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(2));
            assert!(!root.exists());
        }
    }
}
