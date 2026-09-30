//! Human catalog journey using actual processes and no saved install response.
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn call(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pactrun"))
        .args(args)
        .env("PACTRUN_STORAGE_ROOT", root)
        .output()
        .unwrap()
}
fn success(root: &Path, args: &[&str]) -> String {
    let output = call(root, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn catalog_hook_without_protocol() {}

// Test-ID: PR-TEST-0535
// Verifies: PR-REQ-0353, PR-REQ-0088, PR-REQ-0118, PR-REQ-0253
#[test]
fn catalog_cli_discovers_installed_content_and_retired_history_without_install_receipts() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/catalog-process-tests");
    fs::create_dir_all(&parent).unwrap();
    let temp = tempfile::tempdir_in(parent).unwrap();
    let root = temp.path().join("store");
    for part in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(part)).unwrap();
    }
    let source = temp.path().join("pack");
    fs::create_dir(&source).unwrap();
    let tool = if cfg!(windows) { "tool.exe" } else { "tool" };
    fs::copy(std::env::current_exe().unwrap(), source.join(tool)).unwrap();
    fs::write(source.join("pactrun.yaml"), format!("source_format: 1.0-alpha.1\npackage_id: 00000000000000000000000000000089\nrevision:\n  inputs: []\n  actions:\n    - id: inspect\n      access: observe\n      parameters: []\n      hook:\n        protocol_version: 1.0-alpha.1\n        launch: {{ kind: direct, executable: tool }}\n        args: ['--exact', 'catalog_hook_without_protocol', '--nocapture']\n        io: {{ terminal: none }}\n      outputs: []\n  migrations: []\nruntime_content:\n  files:\n    - {{ id: tool, source: {tool}, path: bin/{tool}, executable: true }}\n")).unwrap();
    success(&root, &["pack", "install", source.to_str().unwrap()]);
    // Discard installation output; discover using only the catalog.
    let listing = success(&root, &["revision", "list", "--no-trunc"]);
    let columns: Vec<_> = listing.lines().nth(1).unwrap().split_whitespace().collect();
    let reference = format!("exact:{}/{}", columns[0], columns[1]);
    assert!(success(&root, &["revision", "show", &reference]).contains("Action: inspect"));
    success(
        &root,
        &[
            "revision",
            "alias",
            "set",
            "production",
            &reference,
            "--expect-absent",
        ],
    );
    success(
        &root,
        &[
            "revision",
            "trust",
            "set",
            &reference,
            "distrusted",
            "--expect-absent",
        ],
    );
    success(
        &root,
        &[
            "instance",
            "create",
            "sample",
            "--revision",
            "alias:production",
        ],
    );
    // Descriptive distrust does not reject invocation: this Hook runs and fails
    // only because the fixture deliberately does not speak the Hook protocol.
    assert_eq!(
        call(
            &root,
            &[
                "invoke",
                "sample",
                "inspect",
                "--startup-timeout-ms",
                "1000"
            ]
        )
        .status
        .code(),
        Some(1)
    );
    assert!(success(&root, &["run", "list", "sample"]).contains("inspect"));
    success(&root, &["instance", "delete", "sample"]);
    let history = success(&root, &["instance", "history", "list", "--no-trunc"]);
    let old = history
        .lines()
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    assert!(history.contains("Retired"));
    success(
        &root,
        &[
            "instance",
            "create",
            "sample",
            "--revision",
            "alias:production",
        ],
    );
    assert!(success(&root, &["run", "list", "sample"]).contains("0 records shown."));
    let runs = success(
        &root,
        &[
            "run",
            "list",
            "--instance-id",
            &old,
            "--limit",
            "1",
            "--no-trunc",
        ],
    );
    assert!(runs.contains("More results"));
    let continuation = runs
        .lines()
        .find(|line| line.trim_start().starts_with("pactrun "))
        .unwrap();
    let args: Vec<_> = continuation.split_whitespace().skip(1).collect();
    let next = success(&root, &args);
    assert!(next.contains("1 records shown."));
    assert!(!next.contains("More results"));
    let first_id = runs
        .lines()
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    assert!(!next.lines().nth(1).unwrap().starts_with(first_id));
    let metadata = success(&root, &["revision", "metadata", "show", "alias:production"]);
    assert!(metadata.contains("Local alias: production"));
    assert!(metadata.contains("Distrusted"));
    assert!(success(&root, &["instance", "deletion", "list", "--no-trunc"]).contains(&old));
}
