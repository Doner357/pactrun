//! Verify configuration discovery at the actual executable process boundary.
use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn isolated_command(binary: &Path, config: &Path, data: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .env_remove("PACTRUN_STORAGE_ROOT")
        .env("APPDATA", config)
        .env("XDG_CONFIG_HOME", config)
        .env("LOCALAPPDATA", data)
        .env("XDG_DATA_HOME", data);
    command
}

// Test-ID: PR-TEST-0752
// Verifies: PR-REQ-0393, PR-REQ-0395, PR-REQ-0396
#[test]
fn real_entries_discover_separate_user_files_and_explicit_overrides_without_stores() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/configuration-cli-tests");
    fs::create_dir_all(&parent).unwrap();
    let temporary = tempfile::tempdir_in(parent).unwrap();
    let config = temporary.path().join("configuration");
    let data = temporary.path().join("must-not-create-data");
    for (entry, verbosity) in [("pactrun", "quiet"), ("pactrun-test", "detailed")] {
        fs::create_dir_all(config.join(entry)).unwrap();
        fs::write(
            config.join(entry).join("config.toml"),
            format!("[presentation]\nverbosity = '{verbosity}'\n"),
        )
        .unwrap();
    }
    let cli = Path::new(env!("CARGO_BIN_EXE_pactrun"));
    let test_cli = temporary.path().join(if cfg!(windows) {
        "pactrun-test.exe"
    } else {
        "pactrun-test"
    });
    fs::copy(cli, &test_cli).unwrap();
    for (binary, verbosity) in [(cli, "quiet"), (test_cli.as_path(), "detailed")] {
        let output = isolated_command(binary, &config, &data)
            .args(["--format", "json", "config", "show"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value["result"]["settings"]["presentation"]["verbosity"]["value"],
            verbosity
        );
        assert_eq!(
            value["result"]["settings"]["presentation"]["verbosity"]["source"],
            "user_file"
        );
        assert!(!data.exists());
    }
    fs::write(
        config.join("pactrun/config.toml"),
        "unknown = 'private-config-sentinel'",
    )
    .unwrap();
    let refused = isolated_command(cli, &config, &data)
        .args(["--format", "json", "pack", "generate-id"])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    let value: Value = serde_json::from_slice(&refused.stdout).unwrap();
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("user_file configuration")
    );
    assert!(!String::from_utf8_lossy(&refused.stdout).contains("private-config-sentinel"));
    let version = isolated_command(cli, &config, &data)
        .arg("--version")
        .output()
        .unwrap();
    assert!(
        version.status.success(),
        "version must not read invalid configuration"
    );
    let replacement = temporary.path().join("replacement.toml");
    fs::write(&replacement, "[presentation]\nverbosity = 'normal'").unwrap();
    let output = isolated_command(cli, &config, &data)
        .arg("--config")
        .arg(&replacement)
        .args([
            "--verbosity",
            "detailed",
            "--format",
            "json",
            "config",
            "show",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["result"]["settings"]["presentation"]["verbosity"]["value"],
        "detailed"
    );
    assert_eq!(
        value["result"]["settings"]["presentation"]["verbosity"]["source"],
        "command_line"
    );
    assert!(!data.exists());
}
