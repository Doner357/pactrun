//! Fresh-process coverage of the offline human retirement surface. All data is
//! created beneath this repository's dedicated test directory.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Fixture {
    _temp: tempfile::TempDir,
    store: PathBuf,
    revision: String,
    instance: String,
    allocation: String,
    data: PathBuf,
}
fn call(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pactrun"))
        .args(args)
        .env("PACTRUN_STORAGE_ROOT", root)
        .output()
        .unwrap()
}
fn success(root: &Path, args: &[&str]) -> String {
    let result = call(root, args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}
fn database(root: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        root.join("database/pactrun.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
}
fn installed_reference(root: &Path, source: &Path) -> String {
    let output = success(
        root,
        &[
            "--format",
            "json",
            "pack",
            "install",
            source.to_str().unwrap(),
        ],
    );
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    format!(
        "exact:{}/{}",
        value["result"]["revision"]["package_id"].as_str().unwrap(),
        value["result"]["revision"]["content_digest"]
            .as_str()
            .unwrap()
    )
}
fn fixture() -> Fixture {
    fixture_with_hooks(false)
}
fn fixture_with_hooks(hooks: bool) -> Fixture {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/retirement-cli");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let store = temp.path().join("store");
    for path in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(store.join(path)).unwrap();
    }
    let source = temp.path().join("pack");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("pactrun.yaml"),"source_format: 1.0-alpha.2\npackage_id: 00000000000000000000000000000097\nrevision:\n  service_storages: [{id: data}]\nruntime_content: {}\n").unwrap();
    if hooks {
        fs::write(source.join("script.sh"), "printf 'cleanup-marker\\n'\n").unwrap();
        fs::write(
            source.join("pactrun.yaml"),
            r#"source_format: 1.0-alpha.2
package_id: 00000000000000000000000000000097
revision:
  service_storages: [{id: data}]
  actions:
    - id: status
      access: observe
      parameters: []
      outputs: []
      hook:
        protocol_version: 1.0-alpha.1
        launch: {kind: shell_loader, shell: sh, command: sh, script: script}
        args: []
        io: {terminal: none}
  cleanup:
    requires: []
    hook:
      protocol_version: 1.0-alpha.1
      launch: {kind: shell_loader, shell: sh, command: sh, script: script}
      args: []
      io: {terminal: none}
runtime_content:
  files:
    - {id: script, source: script.sh, path: script.sh, executable: false}
"#,
        )
        .unwrap();
    }
    let revision = installed_reference(&store, &source);
    success(
        &store,
        &["instance", "create", "sample", "--revision", &revision],
    );
    let db = database(&store);
    let (instance, allocation): (Vec<u8>, Vec<u8>) = db
        .query_row(
            "SELECT instance_id,allocation_id FROM instance_service_storages",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let instance = hex::encode(instance);
    let allocation = hex::encode(allocation);
    let data = store
        .join("service-storage")
        .join(format!("alloc-{allocation}"));
    fs::write(data.join("private-data"), b"service-owned private bytes").unwrap();
    Fixture {
        _temp: temp,
        store,
        revision,
        instance,
        allocation,
        data,
    }
}

// Test-ID: PR-TEST-0652
// Verifies: PR-REQ-0336, PR-REQ-0337, PR-REQ-0359, PR-REQ-0360
#[cfg(target_os = "linux")]
#[test]
fn socket_finalization_diagnostics_preserve_cleanup_and_admission_boundaries() {
    use std::os::{fd::AsRawFd, unix::net::UnixListener};
    let f = fixture_with_hooks(true);
    let source = f._temp.path().join("pack");
    let yaml = fs::read_to_string(source.join("pactrun.yaml")).unwrap();
    let migration = format!(
        "  migrations:\n    - source_revision_digest: '{}'\n      transitions: []\n      requires_source: []\n      requires_target: []\n      produces_target: []\n      storage_transitions:\n        - {{kind: reuse, source: {{role: active, storage_id: data}}, target_storage_id: data}}\n",
        f.revision.rsplit('/').next().unwrap()
    );
    fs::write(
        source.join("pactrun.yaml"),
        yaml.replace("runtime_content:", &format!("{migration}runtime_content:")),
    )
    .unwrap();
    let target = installed_reference(&f.store, &source);
    let directory = fs::File::open(&f.data).unwrap();
    let held = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
    let socket = held.join("private-socket-sentinel");
    let listener = UnixListener::bind(&socket).unwrap();
    drop(listener); // Reproduce the stale socket left after a service exits.
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../docs/spec/interfaces/cli-machine.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let response = |output: &Output, format: &str| -> serde_json::Value {
        assert_eq!(output.status.code(), Some(1));
        let text = String::from_utf8(output.stdout.clone()).unwrap();
        let value = if format == "jsonl" {
            let records: Vec<serde_json::Value> = text
                .lines()
                .map(|s| serde_json::from_str(s).unwrap())
                .collect();
            records.last().unwrap()["response"].clone()
        } else {
            serde_json::from_str(&text).unwrap()
        };
        assert!(validator.is_valid(&value), "{value}");
        for private in [
            "private-socket-sentinel",
            "service-owned private bytes",
            f.store.to_str().unwrap(),
        ] {
            assert!(!text.contains(private), "{text}");
        }
        value
    };
    let output = call(
        &f.store,
        &["--format", "json", "instance", "delete", "sample"],
    );
    let failed = response(&output, "json");
    let state = &failed["result"]["run"]["state"];
    assert_eq!(state["hook_completion_status"], "success");
    assert_eq!(state["primary_failure"]["reason"], "unsupported_entry_kind");
    assert_eq!(
        state["primary_failure"]["reference"]["code"],
        "allocation_unavailable"
    );
    assert_eq!(state["primary_failure"]["step"], "finalize_storage");
    assert!(failed["result"]["current_recovery_guard"].is_null());
    let run = failed["result"]["run"]["run_id"].as_str().unwrap();
    let history = success(&f.store, &["--format", "json", "run", "show", run]);
    let history: serde_json::Value = serde_json::from_str(&history).unwrap();
    assert!(validator.is_valid(&history));
    assert_eq!(
        history["result"]["run"]["state"]["primary_failure"],
        state["primary_failure"]
    );
    assert!(success(&f.store, &["run", "show", run]).contains("unsupported entry kind"));

    for format in ["json", "jsonl"] {
        let blocked = response(
            &call(
                &f.store,
                &["--format", format, "invoke", "sample", "status"],
            ),
            format,
        );
        assert_eq!(blocked["error"]["reference"]["code"], "plan_invalidated");
        assert_eq!(
            blocked["error"]["deletion_obligation"]["instance_id"],
            f.instance
        );
        let blocked_run = blocked["result"]["run_id"].as_str().unwrap();
        assert_eq!(
            blocked["error"]["deletion_obligation"]["run_id"],
            blocked_run
        );
        let shown: serde_json::Value = serde_json::from_str(&success(
            &f.store,
            &["--format", "json", "run", "show", blocked_run],
        ))
        .unwrap();
        assert!(validator.is_valid(&shown));
        assert_eq!(
            shown["result"]["run"]["state"]["primary_failure"]["reason"],
            "deletion_obligation"
        );
        assert!(shown["result"]["run"]["state"]["hook_completion_status"].is_null());
        assert!(shown["result"]["current_recovery_guard"].is_null());
        assert!(
            blocked["error"]["message"]
                .as_str()
                .unwrap()
                .contains("instance deletion show")
        );
    }
    let human = call(&f.store, &["invoke", "sample", "status"]);
    assert_eq!(human.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&human.stderr).contains("independently of recovery guard"));
    let overridden = response(
        &call(
            &f.store,
            &[
                "--format",
                "json",
                "invoke",
                "sample",
                "status",
                "--authorize-recovery-override",
            ],
        ),
        "json",
    );
    assert_eq!(
        overridden["error"]["deletion_obligation"]["instance_id"],
        f.instance
    );
    let migration = response(
        &call(
            &f.store,
            &[
                "--format", "json", "instance", "migrate", "sample", "--to", &target,
            ],
        ),
        "json",
    );
    assert_eq!(
        migration["result"]["run"]["state"]["primary_failure"]["reason"],
        "deletion_obligation"
    );
    assert!(migration["result"]["run"]["state"]["hook_completion_status"].is_null());
    let current: serde_json::Value = serde_json::from_str(&success(
        &f.store,
        &["--format", "json", "instance", "show", "sample"],
    ))
    .unwrap();
    assert_eq!(
        current["result"]["active_revision"]["content_digest"],
        f.revision.rsplit('/').next().unwrap()
    );
    let retry = response(
        &call(
            &f.store,
            &["--format", "jsonl", "instance", "delete", "sample"],
        ),
        "jsonl",
    );
    assert!(
        retry["result"]["run"]["state"]["hook_completion_status"].is_null(),
        "Cleanup must not replay"
    );
    assert_eq!(
        retry["result"]["run"]["state"]["primary_failure"]["reason"],
        "unsupported_entry_kind"
    );
    assert!(
        socket.symlink_metadata().is_ok(),
        "no automatic socket removal"
    );
    success(&f.store, &["instance", "abandon", "sample"]);
    let discard = [
        "service-storage",
        "detached",
        "discard",
        &f.allocation,
        "--confirm-discard",
    ];
    let mut args = vec!["--format", "json"];
    args.extend(discard);
    let rejected = response(&call(&f.store, &args), "json");
    assert_eq!(
        rejected["error"]["retirement_reason"],
        "unsupported_entry_kind"
    );
    assert!(socket.symlink_metadata().is_ok());
    // Only the test owner repairs its own socket; the CLI never does so.
    fs::remove_file(&socket).unwrap();
    success(&f.store, &discard);
}

// Test-ID: PR-TEST-0436
// Verifies: PR-REQ-0023, PR-REQ-0036, PR-REQ-0115, PR-REQ-0336, PR-REQ-0338, PR-REQ-0340
#[test]
fn actual_cli_plan_delete_and_old_identity_inspection_preserve_name_reuse_boundary() {
    let f = fixture();
    let plan = success(&f.store, &["instance", "delete", "sample", "--plan"]);
    assert!(plan.contains("Mode: preview"));
    assert_eq!(
        database(&f.store)
            .query_row("SELECT count(*) FROM runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read(f.data.join("private-data")).unwrap(),
        b"service-owned private bytes"
    );
    let deleted = call(&f.store, &["instance", "delete", "sample"]);
    assert!(deleted.status.success());
    assert!(
        String::from_utf8_lossy(&deleted.stderr).contains(&format!("Instance: {}", f.instance))
    );
    assert!(!f.data.exists());
    success(
        &f.store,
        &["instance", "create", "sample", "--revision", &f.revision],
    );
    let old = success(&f.store, &["instance", "deletion", "show", &f.instance]);
    assert!(old.contains("Managed: No"));
    assert!(old.contains("instance_delete"));
    let current: Vec<u8> = database(&f.store)
        .query_row("SELECT instance_id FROM instances", [], |r| r.get(0))
        .unwrap();
    assert_ne!(hex::encode(current), f.instance);
    assert_eq!(
        database(&f.store)
            .query_row("SELECT count(*) FROM runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

// Test-ID: PR-TEST-0437
// Verifies: PR-REQ-0115, PR-REQ-0119, PR-REQ-0181, PR-REQ-0337, PR-REQ-0340
#[test]
fn actual_cli_abandon_handoff_and_confirmed_discard_do_not_infer_destruction() {
    let f = fixture();
    success(&f.store, &["instance", "abandon", "sample"]);
    let listing = success(
        &f.store,
        &["service-storage", "detached", "show", &f.allocation],
    );
    assert!(listing.contains("preserved"));
    assert!(!listing.contains("native_location"));
    assert!(!listing.contains("service-owned private bytes"));
    let handoff = success(
        &f.store,
        &[
            "service-storage",
            "detached",
            "show",
            &f.allocation,
            "--reveal-location",
        ],
    );
    assert!(handoff.contains("Native Location"));
    assert!(handoff.contains(&format!("alloc-{}", f.allocation)));
    assert_eq!(
        call(
            &f.store,
            &["service-storage", "detached", "discard", &f.allocation]
        )
        .status
        .code(),
        Some(2)
    );
    assert_eq!(
        database(&f.store)
            .query_row(
                "SELECT count(*) FROM allocation_discard_receipts",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read(f.data.join("private-data")).unwrap(),
        b"service-owned private bytes"
    );
    let args = [
        "service-storage",
        "detached",
        "discard",
        &f.allocation,
        "--confirm-discard",
    ];
    assert!(success(&f.store, &args).contains("New Completion: Yes"));
    assert!(!f.data.exists());
    fs::create_dir(&f.data).unwrap();
    fs::write(f.data.join("replacement"), b"new unrelated bytes").unwrap();
    assert!(success(&f.store, &args).contains("New Completion: No"));
    assert_eq!(
        fs::read(f.data.join("replacement")).unwrap(),
        b"new unrelated bytes"
    );
}
