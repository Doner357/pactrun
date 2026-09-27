//! Actual offline CLI journeys through logical deletion and physical collection.
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
    let out = call(root, args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn hook_exit_without_protocol() {}

// Test-ID: PR-TEST-0469
// Verifies: PR-REQ-0035, PR-REQ-0076, PR-REQ-0341, PR-REQ-0343, PR-REQ-0344
#[test]
fn lifecycle_cli_deletes_history_and_installation_before_explicit_collection() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/lifecycle-process-tests");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let root = temp.path().join("store");
    for part in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(part)).unwrap();
    }
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let tool = if cfg!(windows) { "tool.exe" } else { "tool" };
    fs::copy(std::env::current_exe().unwrap(), source.join(tool)).unwrap();
    fs::write(source.join("pactrun.yaml"),format!("source_format: 1.0-alpha.1\npackage_id: 00000000000000000000000000000088\nrevision:\n  inputs: []\n  actions:\n    - id: inspect\n      access: observe\n      parameters: []\n      hook:\n        protocol_version: 1.0-alpha.1\n        launch: {{ kind: direct, executable: tool }}\n        args: ['--exact', 'hook_exit_without_protocol', '--nocapture']\n        io: {{ terminal: none }}\n      outputs: []\n  migrations: []\nruntime_content:\n  files:\n    - {{ id: tool, source: {tool}, path: bin/{tool}, executable: true }}\n")).unwrap();
    let reference = success(&root, &["pack", "install", source.to_str().unwrap()])
        .lines()
        .next()
        .unwrap()
        .to_owned();
    success(
        &root,
        &["instance", "create", "sample", "--revision", &reference],
    );
    assert_eq!(
        call(&root, &["revision", "delete", &reference])
            .status
            .code(),
        Some(1)
    );
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
    let db = rusqlite::Connection::open_with_flags(
        root.join("database/pactrun.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let run: Vec<u8> = db
        .query_row("SELECT run_id FROM run_outcomes", [], |r| r.get(0))
        .unwrap();
    let run = hex::encode(run);
    success(&root, &["run", "delete", &run]);
    success(&root, &["run", "delete", &run]);
    success(
        &root,
        &["snapshot", "delete", "00000000000000000000000000000001"],
    );
    success(&root, &["instance", "delete", "sample"]);
    let retired: Vec<u8> = db
        .query_row(
            "SELECT retirement_run_id FROM instance_retirement_receipts",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        call(
            &root,
            &["run", "delete", &hex::encode(retired), "--delete-artifacts"]
        )
        .status
        .code(),
        Some(1)
    );
    success(&root, &["revision", "delete", &reference]);
    success(&root, &["revision", "delete", &reference]);
    let preview = success(&root, &["storage", "gc", "--plan"]);
    assert!(preview.contains("candidates=1"));
    let collected = success(&root, &["storage", "gc"]);
    assert!(collected.contains("removed=1"));
    assert!(success(&root, &["storage", "gc"]).contains("removed=0"));
    assert_eq!(
        db.query_row("SELECT count(*) FROM packages", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT format_version FROM pactrun_metadata WHERE singleton=1",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "1.0-alpha.1"
    );
}
