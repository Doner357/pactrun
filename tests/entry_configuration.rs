//! Real, byte-identical entrypoints; all default data is isolated below target/.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn command(binary: &Path, base: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .current_dir(base)
        .env_remove("PACTRUN_STORAGE_ROOT")
        .env("LOCALAPPDATA", base.join("data"))
        .env("XDG_DATA_HOME", base.join("data"))
        .env("HOME", base.join("home"))
        .env("TEMP", base)
        .env("TMP", base);
    command
}
fn success(mut command: Command, args: &[&str]) -> Output {
    let result = command.args(args).output().unwrap();
    assert!(
        result.status.success(),
        "{args:?}: {}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    result
}
fn entry(base: &Path, name: &str) -> PathBuf {
    let file = base.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    fs::copy(env!("CARGO_BIN_EXE_pactrun"), &file).unwrap();
    file
}

// Test-ID: PR-TEST-0696
// Verifies: PR-REQ-0271, PR-REQ-0333, PR-REQ-0349
#[test]
fn single_executable_entries_isolate_defaults_and_forward_the_selected_store_to_hooks() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/entry-process-tests");
    fs::create_dir_all(&parent).unwrap();
    let temp = tempfile::tempdir_in(parent).unwrap();
    let base = temp.path();
    let normal = entry(base, "pactrun");
    let test = entry(base, "pactrun-test");
    let renamed = entry(base, "renamed");
    assert_eq!(fs::read(&normal).unwrap(), fs::read(&test).unwrap());
    for binary in [&normal, &test, &renamed] {
        success(command(binary, base), &["--format", "json", "--version"]);
        success(command(binary, base), &["--help"]);
    }
    for binary in [&normal, &test] {
        let result = success(
            command(binary, base),
            &["--format", "json", "instance", "list"],
        );
        let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert!(value["result"]["items"].as_array().unwrap().is_empty());
    }
    assert!(
        !base.join("data").exists(),
        "Read-only entry probes created a default Store"
    );
    assert!(
        !command(&renamed, base)
            .args(["instance", "list"])
            .status()
            .unwrap()
            .success()
    );
    for explicit in ["", "relative"] {
        assert!(
            !command(&test, base)
                .env("PACTRUN_STORAGE_ROOT", explicit)
                .args(["instance", "list"])
                .status()
                .unwrap()
                .success()
        );
    }
    assert!(
        !base.join("data").exists(),
        "Invalid configuration fell back to a default"
    );

    let source = base.join("pack");
    fs::create_dir(&source).unwrap();
    let (shell, executable, script, body) = if cfg!(windows) {
        (
            "powershell_7",
            "pwsh.exe",
            "inspect.ps1",
            "param([string]$program,[string]$instance)\n& $env:PACTRUN_EXECUTABLE hook session\nif ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n& $program --format json instance show $instance\nexit $LASTEXITCODE\n",
        )
    } else {
        (
            "sh",
            "sh",
            "inspect.sh",
            "\"$PACTRUN_EXECUTABLE\" hook session || exit 1\n\"$1\" --format json instance show \"$2\"\n",
        )
    };
    fs::write(source.join(script), body).unwrap();
    let args = serde_json::to_string(&[normal.to_str().unwrap(), "test-entry"]).unwrap();
    fs::write(source.join("pactrun.yaml"), format!("source_format: '1.0-alpha.2'\npackage_id: '00000000000000000000000000000696'\nrevision:\n  actions:\n    - id: inspect\n      access: observe\n      parameters: []\n      outputs: []\n      hook:\n        protocol_version: '1.0-alpha.1'\n        launch: {{kind: shell_loader, shell: {shell}, command: {executable}, script: script}}\n        args: {args}\n        io: {{terminal: output}}\nruntime_content:\n  files: [{{id: script, source: {script}, path: {script}, executable: false}}]\n")).unwrap();
    for (binary, name) in [(&normal, "normal-entry"), (&test, "test-entry")] {
        let result = success(
            command(binary, base),
            &[
                "--format",
                "json",
                "pack",
                "install",
                source.to_str().unwrap(),
            ],
        );
        let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        let revision = &value["result"]["revision"];
        let reference = format!(
            "{}:{}",
            revision["package_id"].as_str().unwrap(),
            revision["content_digest"]
                .as_str()
                .unwrap()
                .strip_prefix("sha256:")
                .unwrap()
        );
        success(
            command(binary, base),
            &["instance", "create", name, "--revision", &reference],
        );
    }
    assert!(base.join("data/pactrun/database/pactrun.sqlite3").is_file());
    assert!(
        base.join("data/pactrun-test/database/pactrun.sqlite3")
            .is_file()
    );
    assert!(
        !command(&normal, base)
            .args(["instance", "show", "test-entry"])
            .status()
            .unwrap()
            .success()
    );
    // The script deliberately enters the NORMAL executable from a TEST Run.
    // Without explicit child-root propagation this would inspect the wrong Store.
    success(command(&test, base), &["invoke", "test-entry", "inspect"]);
    for format in ["json", "jsonl"] {
        success(
            command(&test, base),
            &["--format", format, "invoke", "test-entry", "inspect"],
        );
    }
    for binary in [&test, &renamed] {
        let mut explicit = command(binary, base);
        explicit.env("PACTRUN_STORAGE_ROOT", base.join("data/pactrun"));
        success(explicit, &["instance", "show", "normal-entry"]);
    }
    #[cfg(unix)]
    {
        use std::os::unix::{fs::symlink, process::CommandExt};
        let mut spoofed = command(&normal, base);
        spoofed.arg0("pactrun-test");
        success(spoofed, &["instance", "show", "normal-entry"]);
        fs::create_dir(base.join("aliases")).unwrap();
        let alias = base.join("aliases/pactrun-test");
        symlink(&normal, &alias).unwrap();
        success(command(&alias, base), &["instance", "show", "normal-entry"]);
    }
}
