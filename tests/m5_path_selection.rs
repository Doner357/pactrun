//! Fresh-process tests for path discovery/selection, not Migration execution.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    source: PathBuf,
    a: String,
    b: String,
    c: String,
}
fn command(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pactrun"))
        .args(args)
        .env("PACTRUN_STORAGE_ROOT", root)
        .output()
        .unwrap()
}
fn successful(root: &Path, args: &[&str]) -> String {
    let result = command(root, args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}
fn install(root: &Path, source: &Path, edges: &[(&str, bool)]) -> String {
    let edges = if edges.is_empty() {
        "  migrations: []\n".to_owned()
    } else {
        format!("  migrations:\n{}", edges.iter().map(|(revision, required)| format!(
            "    - source_revision_digest: {}\n      transitions: []\n      requires_source: {}\n      requires_target: []\n      produces_target: []\n",
            revision.rsplit('/').next().unwrap(), if *required { "[{role: active, input_id: config}]" } else { "[]" })).collect::<String>())
    };
    fs::write(source.join("pactrun.yaml"), format!("source_format: 1\npackage_id: 00000000000000000000000000000077\nrevision:\n  inputs:\n    - id: config\n      protection: secret\n  actions: []\n{edges}runtime_content:\n  files: []\n")).unwrap();
    successful(root, &["pack", "install", source.to_str().unwrap()])
        .lines()
        .next()
        .unwrap()
        .to_owned()
}
fn setup(direct_requires_input: bool) -> Fixture {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m5-path-cli-tests");
    fs::create_dir_all(&parent).unwrap();
    let temporary = tempfile::tempdir_in(parent).unwrap();
    let root = temporary.path().join("storage");
    for directory in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    let source = temporary.path().join("source with spaces");
    fs::create_dir(&source).unwrap();
    let a = install(&root, &source, &[]);
    let b = install(&root, &source, &[(&a, false)]);
    let c = install(&root, &source, &[(&a, direct_requires_input), (&b, false)]);
    successful(&root, &["instance", "create", "demo", "--revision", &a]);
    Fixture {
        _temporary: temporary,
        root,
        source,
        a,
        b,
        c,
    }
}
fn ids(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix("path_id: ").map(str::to_owned))
        .collect()
}

// Test-ID: PR-TEST-0318
// Verifies: PR-REQ-0315, PR-REQ-0307, PR-REQ-0310, PR-REQ-0162
#[test]
fn operator_plan_never_opens_files_and_conflicts_are_checked_before_acquisition() {
    let f = setup(false);
    let before = successful(&f.root, &["instance", "show", "demo"]);
    let paths = ids(&successful(
        &f.root,
        &["instance", "migration-paths", "demo", "--to", &f.c],
    ));
    let path = paths.iter().max_by_key(|p| p.len()).unwrap();
    let absent = f.source.join("not-present=still not present");
    let input = format!(
        "{}/config={}",
        f.b.rsplit('/').next().unwrap(),
        absent.display()
    );
    let plan = successful(
        &f.root,
        &[
            "instance",
            "migrate",
            "demo",
            "--to",
            &f.c,
            "--path",
            path,
            "--input-file",
            &input,
            "--plan",
        ],
    );
    assert!(plan.contains("operator_input: config (acquisition not performed)"));
    assert!(!plan.contains("not-present"));
    assert_no_execution(&f.root, &before);
    let conflict = format!(
        "{}/config={}",
        f.c.rsplit('/').next().unwrap(),
        absent.display()
    );
    let result = command(
        &f.root,
        &[
            "instance",
            "migrate",
            "demo",
            "--to",
            &f.c,
            "--path",
            path,
            "--input-file",
            &input,
            "--input-file",
            &conflict,
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("multiple writers")
    );
    assert_no_execution(&f.root, &before);
}

// Test-ID: PR-TEST-0319
// Verifies: PR-REQ-0315, PR-REQ-0307, PR-REQ-0304
#[test]
fn native_operator_file_paths_and_empty_values_survive_a_chained_migration() {
    for bytes in [b"operator secret value".as_slice(), b"".as_slice()] {
        let f = setup(false);
        let paths = ids(&successful(
            &f.root,
            &["instance", "migration-paths", "demo", "--to", &f.c],
        ));
        let path = paths.iter().max_by_key(|p| p.len()).unwrap();
        let file = f.source.join("operator=bytes with spaces");
        fs::write(&file, bytes).unwrap();
        let input = format!(
            "{}/config={}",
            f.b.rsplit('/').next().unwrap(),
            file.display()
        );
        let result = successful(
            &f.root,
            &[
                "instance",
                "migrate",
                "demo",
                "--to",
                &f.c,
                "--path",
                path,
                "--input-file",
                &input,
            ],
        );
        assert!(result.contains("required_inputs_satisfied: true"));
        assert!(!result.contains("operator secret value"));
        let exported = command(
            &f.root,
            &[
                "input",
                "export",
                "demo",
                "config",
                "--output",
                "-",
                "--authorize-secret-export",
            ],
        );
        assert!(exported.status.success());
        assert_eq!(exported.stdout, bytes);
        let denied = command(
            &f.root,
            &["input", "export", "demo", "config", "--output", "-"],
        );
        assert_eq!(denied.status.code(), Some(1));
    }
}

// Test-ID: PR-TEST-0320
// Verifies: PR-REQ-0315, PR-REQ-0307
#[test]
fn operator_acquisition_and_invalid_target_fail_without_acceptance() {
    let f = setup(false);
    let missing = f.source.join("missing");
    let input = format!(
        "{}/config={}",
        f.b.rsplit('/').next().unwrap(),
        missing.display()
    );
    let args = [
        "instance",
        "migrate",
        "demo",
        "--to",
        &f.b,
        "--input-file",
        &input,
    ];
    assert_eq!(command(&f.root, &args).status.code(), Some(1));
    let db = rusqlite::Connection::open(f.root.join("database/pactrun.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    let wrong = format!(
        "{}/config={}",
        f.a.rsplit('/').next().unwrap(),
        missing.display()
    );
    let result = command(
        &f.root,
        &[
            "instance",
            "migrate",
            "demo",
            "--to",
            &f.b,
            "--input-file",
            &wrong,
            "--plan",
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    let duplicate = command(
        &f.root,
        &[
            "instance",
            "migrate",
            "demo",
            "--to",
            &f.b,
            "--input-file",
            &input,
            "--input-file",
            &input,
        ],
    );
    assert_eq!(duplicate.status.code(), Some(2));
}

// Test-ID: PR-TEST-0300
// Verifies: PR-REQ-0309, PR-REQ-0345
#[test]
fn real_cli_requires_explicit_predecessor_upgrade_preserving_inputs_and_path_ids() {
    let f = setup(false);
    let input = f.source.join("upgrade-secret");
    fs::write(&input, b"preserved secret bytes").unwrap();
    successful(
        &f.root,
        &[
            "input",
            "set",
            "demo",
            "config",
            "--file",
            input.to_str().unwrap(),
        ],
    );
    let before = successful(&f.root, &["instance", "show", "demo"]);
    let paths = ids(&successful(
        &f.root,
        &["instance", "migration-paths", "demo", "--to", &f.c],
    ));
    {
        // Build the exact predecessor without discarding any product object.
        let mut db = rusqlite::Connection::open(f.root.join("database/pactrun.sqlite3")).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM writable_admissions", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let tx = db.transaction().unwrap();
        assert_eq!(
            tx.query_row::<i64, _, _>(
                "SELECT count(*) FROM snapshot_blobs WHERE storage_kind=1",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            tx.query_row::<i64, _, _>(
                "SELECT count(*) FROM managed_input_payloads WHERE content_digest IS NOT NULL",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            0
        );
        tx.execute_batch("ALTER TABLE snapshot_blobs DROP COLUMN storage_kind; ALTER TABLE managed_input_payloads DROP COLUMN content_digest;").unwrap();
        tx.execute_batch(
            include_str!("../src/persistence/persistence_schema_v8_additions.sql")
                .split("CREATE TABLE instance_history_identities")
                .next()
                .unwrap(),
        )
        .unwrap();
        tx.pragma_update(None, "user_version", 8).unwrap();
        tx.commit().unwrap();
    }
    let rejected = command(
        &f.root,
        &["instance", "migration-paths", "demo", "--to", &f.c],
    );
    assert_eq!(rejected.status.code(), Some(1));
    assert!(
        String::from_utf8(rejected.stderr)
            .unwrap()
            .contains("explicit pactrun storage upgrade")
    );
    assert_eq!(
        command(&f.root, &["storage", "upgrade", "--force"])
            .status
            .code(),
        Some(2)
    );
    let first = successful(&f.root, &["storage", "upgrade"]);
    assert!(first.contains("V10 (upgraded)"));
    assert!(successful(&f.root, &["storage", "upgrade"]).contains("V10 (already current)"));
    assert_eq!(
        paths,
        ids(&successful(
            &f.root,
            &["instance", "migration-paths", "demo", "--to", &f.c]
        ))
    );
    assert_no_execution(&f.root, &before);
    let exported = f.source.join("exported-secret");
    successful(
        &f.root,
        &[
            "input",
            "export",
            "demo",
            "config",
            "--output",
            exported.to_str().unwrap(),
            "--authorize-secret-export",
        ],
    );
    assert_eq!(fs::read(exported).unwrap(), b"preserved secret bytes");
}
fn assert_no_execution(root: &Path, before: &str) {
    assert_eq!(successful(root, &["instance", "show", "demo"]), before);
    let database = rusqlite::Connection::open_with_flags(
        root.join("database/pactrun.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    for table in ["runs", "run_revision_pins", "writable_admissions"] {
        let count: i64 = database
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }
    assert_eq!(fs::read_dir(root.join("staging")).unwrap().count(), 0);
    let version: i64 = database
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 10);
}

// Test-ID: PR-TEST-0290
// Verifies: PR-REQ-0303, PR-REQ-0309, PR-REQ-0310
#[test]
fn real_cli_lists_and_selects_direct_or_chained_paths_without_any_run() {
    let f = setup(false);
    let before = successful(&f.root, &["instance", "show", "demo"]);
    let listing = successful(
        &f.root,
        &["instance", "migration-paths", "demo", "--to", &f.c],
    );
    let choices = ids(&listing);
    assert_eq!(choices.len(), 2);
    let mut edge_counts = Vec::new();
    for path in &choices {
        let plan = successful(
            &f.root,
            &[
                "instance", "migrate", "demo", "--to", &f.c, "--path", path, "--plan",
            ],
        );
        assert!(plan.contains("no Run created"));
        assert!(plan.contains(path));
        edge_counts.push(plan.lines().filter(|l| l.starts_with("edge ")).count());
    }
    edge_counts.sort();
    assert_eq!(edge_counts, vec![1, 2]);
    let ambiguous = command(
        &f.root,
        &["instance", "migrate", "demo", "--to", &f.c, "--plan"],
    );
    assert_eq!(ambiguous.status.code(), Some(1));
    assert_eq!(ids(&String::from_utf8(ambiguous.stdout).unwrap()), choices);
    let first = successful(
        &f.root,
        &[
            "instance",
            "migration-paths",
            "demo",
            "--to",
            &f.c,
            "--limit",
            "1",
        ],
    );
    assert!(first.contains("more_paths: true"));
    let first_id = ids(&first).remove(0);
    let last = successful(
        &f.root,
        &[
            "instance",
            "migration-paths",
            "demo",
            "--to",
            &f.c,
            "--after",
            &first_id,
            "--limit",
            "1",
        ],
    );
    assert!(last.contains("more_paths: false"));
    assert_ne!(ids(&last)[0], first_id);
    assert!(
        successful(
            &f.root,
            &["instance", "migrate", "demo", "--to", &f.b, "--plan"]
        )
        .contains("edge 1:")
    );
    assert_no_execution(&f.root, &before);
}

// Test-ID: PR-TEST-0291
// Verifies: PR-REQ-0309, PR-REQ-0310
#[test]
fn candidates_do_not_hide_unready_routes_and_ids_are_not_binding_state_tokens() {
    let f = setup(true);
    let before = successful(&f.root, &["instance", "show", "demo"]);
    let listed = successful(
        &f.root,
        &["instance", "migration-paths", "demo", "--to", &f.c],
    );
    assert!(listed.contains("requirements, runtime and authorization not evaluated"));
    let choices = ids(&listed);
    assert_eq!(choices.len(), 2);
    let failures = choices
        .iter()
        .filter(|path| {
            !command(
                &f.root,
                &[
                    "instance", "migrate", "demo", "--to", &f.c, "--path", path, "--plan",
                ],
            )
            .status
            .success()
        })
        .count();
    assert_eq!(failures, 1);
    assert_eq!(
        command(
            &f.root,
            &["instance", "migrate", "demo", "--to", &f.c, "--plan"]
        )
        .status
        .code(),
        Some(1)
    );
    assert_no_execution(&f.root, &before);
    let secret = f.source.join("secret-input");
    fs::write(&secret, b"never-print-this-value").unwrap();
    successful(
        &f.root,
        &[
            "input",
            "set",
            "demo",
            "config",
            "--file",
            secret.to_str().unwrap(),
        ],
    );
    let relisted = successful(
        &f.root,
        &["instance", "migration-paths", "demo", "--to", &f.c],
    );
    assert_eq!(ids(&relisted), choices);
    for path in choices {
        let plan = successful(
            &f.root,
            &[
                "instance", "migrate", "demo", "--to", &f.c, "--path", &path, "--plan",
            ],
        );
        assert!(!plan.contains("never-print-this-value"));
    }
}

// Test-ID: PR-TEST-0292
// Verifies: PR-REQ-0310
#[test]
fn bad_selectors_obsolete_flags_and_missing_storage_fail_before_writes() {
    let f = setup(false);
    let before = successful(&f.root, &["instance", "show", "demo"]);
    for options in [
        vec!["--path", "1"],
        vec!["--via", &f.b],
        vec!["--direct"],
        vec!["--plan", "--plan"],
    ] {
        let mut args = vec!["instance", "migrate", "demo", "--to", &f.c];
        args.extend(options);
        assert_eq!(command(&f.root, &args).status.code(), Some(2));
    }
    assert_eq!(
        command(
            &f.root,
            &[
                "instance",
                "migration-paths",
                "demo",
                "--to",
                &f.c,
                "--limit",
                "0"
            ]
        )
        .status
        .code(),
        Some(2)
    );
    let listed = successful(
        &f.root,
        &["instance", "migration-paths", "demo", "--to", &f.c],
    );
    let path = ids(&listed).remove(0);
    let changed_target = command(
        &f.root,
        &[
            "instance", "migrate", "demo", "--to", &f.b, "--path", &path, "--plan",
        ],
    );
    assert_eq!(changed_target.status.code(), Some(1));
    assert!(
        String::from_utf8(changed_target.stderr)
            .unwrap()
            .contains("does not match")
    );
    let missing = f.source.join("must-not-create-storage");
    let unavailable = command(&missing, &["instance", "migrate", "demo", "--to", &f.a]);
    assert_eq!(unavailable.status.code(), Some(1));
    assert!(!unavailable.status.success());
    assert!(!missing.exists());
    assert_no_execution(&f.root, &before);
}

// Test-ID: PR-TEST-0307
// Verifies: PR-REQ-0313, PR-REQ-0314
#[test]
fn real_cli_executes_selected_direct_and_chained_paths_and_shows_durable_runs() {
    for edge_count in [1, 2] {
        let f = setup(false);
        let listing = successful(
            &f.root,
            &["instance", "migration-paths", "demo", "--to", &f.c],
        );
        let selected = ids(&listing)
            .into_iter()
            .find(|id| {
                successful(
                    &f.root,
                    &[
                        "instance", "migrate", "demo", "--to", &f.c, "--path", id, "--plan",
                    ],
                )
                .lines()
                .filter(|l| l.starts_with("edge "))
                .count()
                    == edge_count
            })
            .unwrap();
        let result = successful(
            &f.root,
            &[
                "instance", "migrate", "demo", "--to", &f.c, "--path", &selected,
            ],
        );
        assert!(result.contains("operation: migration"));
        assert!(result.contains("outcome: succeeded"));
        assert!(result.contains(&format!("committed_edges: {edge_count}")));
        let run = result
            .lines()
            .find_map(|line| line.strip_prefix("run: "))
            .unwrap();
        assert!(
            successful(&f.root, &["run", "show", run])
                .contains(&format!("last_committed_revision: {}", f.c))
        );
        assert!(successful(&f.root, &["instance", "show", "demo"]).contains(&f.c));
        assert!(successful(&f.root, &["run", "list", "demo"]).contains("migration"));
        let db = rusqlite::Connection::open_with_flags(
            f.root.join("database/pactrun.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM runs", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM run_migration_boundaries", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            edge_count as i64
        );
    }
}

// Test-ID: PR-TEST-0310
// Verifies: PR-REQ-0314, PR-REQ-0156
#[test]
fn unsupported_hook_protocol_suffix_does_not_execute_a_declarative_prefix() {
    let f = setup(false);
    let before = successful(&f.root, &["instance", "show", "demo"]);
    fs::write(f.source.join("tool"), b"must never be launched").unwrap();
    fs::write(f.source.join("pactrun.yaml"),format!("source_format: 1\npackage_id: 00000000000000000000000000000077\nrevision:\n  inputs: []\n  actions: []\n  migrations:\n    - source_revision_digest: {}\n      transitions: []\n      requires_source: []\n      requires_target: []\n      produces_target: []\n      hook:\n        protocol_version: 1\n        launch: {{ kind: direct, executable: tool }}\n        args: []\n        io: {{ terminal: none }}\nruntime_content:\n  files:\n    - {{ id: tool, source: tool, path: bin/tool, executable: true }}\n",f.c.rsplit('/').next().unwrap())).unwrap();
    let manifest = fs::read_to_string(f.source.join("pactrun.yaml")).unwrap();
    fs::write(
        f.source.join("pactrun.yaml"),
        manifest.replace("protocol_version: 1", "protocol_version: 3"),
    )
    .unwrap();
    let target = successful(&f.root, &["pack", "install", f.source.to_str().unwrap()])
        .lines()
        .next()
        .unwrap()
        .to_owned();
    let listed = successful(
        &f.root,
        &["instance", "migration-paths", "demo", "--to", &target],
    );
    let id = ids(&listed).pop().unwrap();
    let refused = command(
        &f.root,
        &[
            "instance", "migrate", "demo", "--to", &target, "--path", &id,
        ],
    );
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        String::from_utf8(refused.stderr)
            .unwrap()
            .contains("protocol is unsupported")
    );
    assert_no_execution(&f.root, &before);
}

// Test-ID: PR-TEST-0311
// Verifies: PR-REQ-0313, PR-REQ-0314, PR-REQ-0160
#[test]
fn real_cli_requires_declassification_authorization_and_releases_only_the_target_binding() {
    let f = setup(false);
    let data = f.source.join("secret-data");
    fs::write(&data, b"authorized release bytes").unwrap();
    successful(
        &f.root,
        &[
            "input",
            "set",
            "demo",
            "config",
            "--file",
            data.to_str().unwrap(),
        ],
    );
    fs::write(f.source.join("pactrun.yaml"),format!("source_format: 1\npackage_id: 00000000000000000000000000000077\nrevision:\n  inputs:\n    - {{ id: released, required: true }}\n  actions: []\n  migrations:\n    - source_revision_digest: {}\n      transitions:\n        - {{ kind: declassify, source: {{ role: active, input_id: config }}, target_input_id: released }}\n      requires_source: [{{role: active, input_id: config}}]\n      requires_target: []\n      produces_target: []\nruntime_content:\n  files: []\n",f.a.rsplit('/').next().unwrap())).unwrap();
    let target = successful(&f.root, &["pack", "install", f.source.to_str().unwrap()])
        .lines()
        .next()
        .unwrap()
        .to_owned();
    let before = successful(&f.root, &["instance", "show", "demo"]);
    let refused = command(&f.root, &["instance", "migrate", "demo", "--to", &target]);
    assert_eq!(refused.status.code(), Some(1));
    assert_no_execution(&f.root, &before);
    let result = successful(
        &f.root,
        &[
            "instance",
            "migrate",
            "demo",
            "--to",
            &target,
            "--authorize-declassification",
        ],
    );
    assert!(result.contains("outcome: succeeded"));
    assert!(!result.contains("authorized release bytes"));
    let export = f.source.join("released-output");
    successful(
        &f.root,
        &[
            "input",
            "export",
            "demo",
            "released",
            "--output",
            export.to_str().unwrap(),
        ],
    );
    assert_eq!(fs::read(export).unwrap(), b"authorized release bytes");
}
