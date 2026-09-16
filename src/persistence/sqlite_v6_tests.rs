use super::*;
use crate::domain::*;
use crate::persistence::sqlite_revision_store::{
    APPLICATION_ID, SCHEMA_LADDER, SCHEMA_V5_ADDITIONS_SQL,
};
use crate::persistence::{ManagedInputWrite, RunArtifactWrite, UnconditionalAcceptance};
use rusqlite::{params, types::Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Cursor,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const WORKER: &str = "persistence::sqlite_v6::tests::v6_worker";
const MIGRATION_TABLES: [&str; 7] = [
    "run_migration_checkpoint_bindings",
    "run_migration_payload_pins",
    "run_migration_revision_pins",
    "run_migration_boundaries",
    "run_migration_progress",
    "run_migration_edges",
    "run_migration_invocations",
];
fn root() -> (tempfile::TempDir, PathBuf) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m5-v6-tests");
    fs::create_dir_all(&parent).unwrap();
    let tmp = tempfile::tempdir_in(parent).unwrap();
    let root = tmp.path().join("store");
    for name in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    (tmp, root)
}
fn db_path(root: &Path) -> PathBuf {
    root.join("database/pactrun.sqlite3")
}
fn source(root: &Path, version: usize) {
    let db = Connection::open(db_path(root)).unwrap();
    configure_connection(&db).unwrap();
    for (sql, _) in &SCHEMA_LADDER[..version] {
        db.execute_batch(sql).unwrap();
    }
    db.pragma_update(None, "application_id", APPLICATION_ID)
        .unwrap();
    db.pragma_update(None, "user_version", version as i64)
        .unwrap();
    validate_schema(&db, version as i64).unwrap();
}
fn version(root: &Path) -> i64 {
    Connection::open(db_path(root))
        .unwrap()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}
fn contents(db: &Connection) -> BTreeMap<String, Vec<String>> {
    let tables: Vec<String> = db.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'run_migration_%' AND name <> 'writable_admissions' ORDER BY name")
        .unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
    tables
        .into_iter()
        .map(|name| {
            let mut query = db.prepare(&format!("SELECT * FROM {name}")).unwrap();
            let columns = query.column_count();
            let mut rows: Vec<String> = query
                .query_map([], |r| {
                    Ok(format!(
                        "{:?}",
                        (0..columns)
                            .map(|i| r.get::<_, Value>(i))
                            .collect::<Result<Vec<_>, _>>()?
                    ))
                })
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            rows.sort();
            (name, rows)
        })
        .collect()
}
fn wait(path: &Path) {
    let end = Instant::now() + Duration::from_secs(45);
    while !path.exists() {
        assert!(Instant::now() < end, "barrier timed out");
        thread::sleep(Duration::from_millis(10));
    }
}
struct Worker(Child);
impl Worker {
    fn finish(mut self, expected: i32) {
        assert_eq!(self.0.wait().unwrap().code(), Some(expected));
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn worker(root: &Path, mode: &str, extra: &[(&str, &str)]) -> Worker {
    let mut c = Command::new(std::env::current_exe().unwrap());
    c.args(["--exact", WORKER, "--nocapture"])
        .env("PACTRUN_V6_TEST_ROOT", root)
        .env("PACTRUN_V6_TEST_MODE", mode)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    for (key, value) in extra {
        c.env(key, value);
    }
    Worker(c.spawn().unwrap())
}

#[test]
fn v6_worker() {
    let Some(root) = std::env::var_os("PACTRUN_V6_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    match std::env::var("PACTRUN_V6_TEST_MODE").unwrap().as_str() {
        "upgrade" => {
            assert!(PactrunPersistence::upgrade_v5_to_v6_fixture(&root).unwrap());
        }
        "legacy-held" | "legacy-late" => {
            let session = StagingSession::prepare(&root).unwrap();
            let mut db = Connection::open(db_path(&root)).unwrap();
            configure_connection(&db).unwrap();
            let late = std::env::var("PACTRUN_V6_TEST_MODE").unwrap() == "legacy-late";
            if !late {
                let tx = db
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .unwrap();
                validate_schema(&tx, 5).unwrap();
                tx.execute(
                    "INSERT INTO writable_admissions VALUES (?1,5)",
                    [session.owner().as_str().as_bytes()],
                )
                .unwrap();
                tx.commit().unwrap();
            }
            fs::write(root.join("writer-ready"), []).unwrap();
            wait(&root.join("writer-release"));
            if late {
                // The original V5 writer's transaction-bound check, not the new
                // writer's check: a pre-lock observation never grants permission.
                let tx = db
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .unwrap();
                let marker: i64 = tx
                    .pragma_query_value(None, "user_version", |r| r.get(0))
                    .unwrap();
                assert_ne!(marker, 5);
                assert!(validate_schema(&tx, 5).is_err());
                fs::write(root.join("legacy-refused"), []).unwrap();
            } else {
                db.execute(
                    "DELETE FROM writable_admissions WHERE owner_session=?1",
                    [session.owner().as_str().as_bytes()],
                )
                .unwrap();
            }
        }
        _ => panic!("unknown worker mode"),
    }
}

// Test-ID: PR-TEST-0293
// Verifies: PR-REQ-0311
#[test]
fn v6_schema_preserves_v5_payload_tables_and_adds_exact_migration_structure() {
    let document =
        include_str!("../../docs/spec/persistence/persistence-schema-v6.md").replace("\r\n", "\n");
    let approved = document
        .split("```sql\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    assert_eq!(
        approved.trim(),
        SCHEMA_V6_ADDITIONS_SQL.replace("\r\n", "\n").trim()
    );
    let db = Connection::open_in_memory().unwrap();
    configure_connection(&db).unwrap();
    for (sql, _) in &SCHEMA_LADDER[..5] {
        db.execute_batch(sql).unwrap();
    }
    let before = contents(&db);
    db.execute_batch(SCHEMA_V6_ADDITIONS_SQL).unwrap();
    validate_schema(&db, 6).unwrap();
    assert_eq!(contents(&db), before);
    for table in MIGRATION_TABLES {
        let flags: (i64, i64) = db
            .query_row(
                "SELECT wr,strict FROM pragma_table_list WHERE schema='main' AND name=?1",
                [table],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(flags, (1, 1));
    }
    assert!(
        db.execute(
            "INSERT INTO writable_admissions VALUES (?1,5)",
            [b"session-00000000000000000000000000000000".as_slice()]
        )
        .is_err()
    );
    assert!(
        db.execute(
            "INSERT INTO run_migration_invocations VALUES (?1,?2,?3,?3,0)",
            params![
                [1u8; 16].as_slice(),
                [2u8; 16].as_slice(),
                [3u8; 32].as_slice()
            ]
        )
        .is_err()
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name LIKE 'v6_%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

// Test-ID: PR-TEST-0294
// Verifies: PR-REQ-0312
#[test]
fn only_exact_v5_upgrades_and_current_v6_is_a_read_only_noop() {
    let (_tmp, current) = root();
    assert!(PactrunPersistence::upgrade_v5_to_v6_fixture(&current).is_err());
    assert!(!db_path(&current).exists());
    source(&current, 6);
    assert_eq!(version(&current), 6);
    assert!(!PactrunPersistence::upgrade_v5_to_v6_fixture(&current).unwrap());
    for v in 1..=5 {
        let (_tmp, root) = root();
        source(&root, v);
        let sentinel = root.join("staging/session-eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");
        fs::create_dir(&sentinel).unwrap();
        fs::write(sentinel.join("sentinel"), b"not an admitted writer").unwrap();
        assert!(PactrunPersistence::open(&root).is_err());
        assert!(PactrunPersistence::open_read_only(&root).is_err());
        assert!(sentinel.join("sentinel").exists());
        if v == 5 {
            assert!(PactrunPersistence::upgrade_v5_to_v6_fixture(&root).unwrap());
            assert_eq!(version(&root), 6);
            assert!(sentinel.join("sentinel").exists());
        } else {
            assert!(PactrunPersistence::upgrade_v5_to_v6_fixture(&root).is_err());
            assert_eq!(version(&root), v as i64);
        }
    }
    for (application, version, drift) in [
        (APPLICATION_ID, 7, false),
        (17, 5, false),
        (APPLICATION_ID, 5, true),
        (0, 0, true),
    ] {
        let (_tmp, root) = root();
        source(&root, 5);
        let db = Connection::open(db_path(&root)).unwrap();
        db.pragma_update(None, "application_id", application)
            .unwrap();
        db.pragma_update(None, "user_version", version).unwrap();
        if drift {
            db.execute_batch("CREATE TABLE unexpected (value INTEGER);")
                .unwrap();
        }
        drop(db);
        assert!(PactrunPersistence::upgrade_v5_to_v6_fixture(&root).is_err());
        assert_eq!(self::version(&root), version);
    }
}

// Test-ID: PR-TEST-0295
// Verifies: PR-REQ-0312
#[test]
fn admitted_live_and_unknown_owners_block_but_unadmitted_sessions_do_not() {
    let (_tmp, root) = root();
    source(&root, 5);
    let child = worker(&root, "legacy-held", &[]);
    wait(&root.join("writer-ready"));
    assert!(matches!(
        PactrunPersistence::upgrade_v5_to_v6_fixture(&root),
        Err(PersistenceError::ActiveWriters)
    ));
    assert_eq!(version(&root), 5);
    fs::write(root.join("writer-release"), []).unwrap();
    child.finish(0);
    let unknown = "session-dddddddddddddddddddddddddddddddd";
    fs::create_dir(root.join("staging").join(unknown)).unwrap();
    let db = Connection::open(db_path(&root)).unwrap();
    db.execute(
        "INSERT INTO writable_admissions VALUES (?1,5)",
        [unknown.as_bytes()],
    )
    .unwrap();
    assert!(matches!(
        PactrunPersistence::upgrade_v5_to_v6_fixture(&root),
        Err(PersistenceError::ActiveWriters)
    ));
    db.execute("DELETE FROM writable_admissions", []).unwrap();
    drop(db);
    let _prepared = StagingSession::prepare(&root).unwrap();
    assert!(PactrunPersistence::upgrade_v5_to_v6_fixture(&root).unwrap());
}

// Test-ID: PR-TEST-0296
// Verifies: PR-REQ-0312
#[test]
fn late_v5_writer_cannot_write_after_atomic_upgrade() {
    let (_tmp, root) = root();
    source(&root, 5);
    let sync = root.join("upgrade-sync");
    fs::create_dir(&sync).unwrap();
    let upgrade = worker(
        &root,
        "upgrade",
        &[
            ("PACTRUN_M4_SYNC", "after_v5_admission_inspection"),
            ("PACTRUN_M4_SYNC_DIR", sync.to_str().unwrap()),
        ],
    );
    wait(&sync.join("ready"));
    let late = worker(&root, "legacy-late", &[]);
    wait(&root.join("writer-ready"));
    fs::write(root.join("writer-release"), []).unwrap();
    fs::write(sync.join("release"), []).unwrap();
    upgrade.finish(0);
    late.finish(0);
    assert!(root.join("legacy-refused").exists());
    assert_eq!(version(&root), 6);
    assert_eq!(
        Connection::open(db_path(&root))
            .unwrap()
            .query_row("SELECT count(*) FROM writable_admissions", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

// Test-ID: PR-TEST-0297
// Verifies: PR-REQ-0311, PR-REQ-0312
#[test]
fn upgrade_crashes_publish_only_complete_v5_or_v6_and_roll_back_failed_copy() {
    for (point, expected) in [
        ("after_v5_admission_inspection", 5),
        ("before_schema_migration_commit", 5),
        ("after_schema_migration_commit", 6),
    ] {
        let (_tmp, root) = root();
        source(&root, 5);
        worker(&root, "upgrade", &[("PACTRUN_M4_FAULT", point)]).finish(87);
        assert_eq!(version(&root), expected);
        validate_schema(&Connection::open(db_path(&root)).unwrap(), expected).unwrap();
        assert_eq!(
            PactrunPersistence::upgrade_v5_to_v6_fixture(&root).unwrap(),
            expected == 5
        );
    }
    let (_tmp, root) = root();
    source(&root, 5);
    let db = Connection::open(db_path(&root)).unwrap();
    // Foreign keys deliberately disabled only to construct a corrupted source.
    db.pragma_update(None, "foreign_keys", false).unwrap();
    db.execute(
        "INSERT INTO run_operation_kinds VALUES (?1,1)",
        [[8u8; 16].as_slice()],
    )
    .unwrap();
    let before = contents(&db);
    drop(db);
    assert!(PactrunPersistence::upgrade_v5_to_v6_fixture(&root).is_err());
    let db = Connection::open(db_path(&root)).unwrap();
    assert_eq!(version(&root), 5);
    validate_schema(&db, 5).unwrap();
    assert_eq!(contents(&db), before);
}

// Test-ID: PR-TEST-0299
// Verifies: PR-REQ-0311
#[test]
fn migration_references_retain_payloads_and_operation_kinds_are_not_inferred() {
    let mut db = Connection::open_in_memory().unwrap();
    configure_connection(&db).unwrap();
    for (sql, _) in &SCHEMA_LADDER[..6] {
        db.execute_batch(sql).unwrap();
    }
    let instance = InstanceId::from_bytes([2; 16]);
    let payload = ManagedInputPayloadId::from_bytes([3; 16]);
    let run = RunId::from_bytes([4; 16]);
    // SQL representation and reference-lifetime fixture only, not an executable Plan.
    db.execute("INSERT INTO packages VALUES (?1)", [[1u8; 16].as_slice()])
        .unwrap();
    db.execute(
        "INSERT INTO revisions VALUES (?1,?2,?3,?3)",
        params![[1u8; 16].as_slice(), [1u8; 32].as_slice(), b"{}".as_slice()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO instances VALUES (?1,?2,?3,?4,?5)",
        params![
            instance.as_bytes().as_slice(),
            b"fixture".as_slice(),
            [1u8; 16].as_slice(),
            [1u8; 32].as_slice(),
            [2u8; 16].as_slice()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO managed_input_payloads VALUES (?1,?2,1,1)",
        params![
            instance.as_bytes().as_slice(),
            payload.as_bytes().as_slice()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO managed_input_payload_chunks VALUES (?1,?2,0,?3)",
        params![
            instance.as_bytes().as_slice(),
            payload.as_bytes().as_slice(),
            b"x".as_slice()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO runs VALUES (?1,?2,?3,0)",
        params![
            run.as_bytes().as_slice(),
            instance.as_bytes().as_slice(),
            [2u8; 16].as_slice()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO run_operation_kinds VALUES (?1,3)",
        [run.as_bytes().as_slice()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO run_migration_invocations VALUES (?1,?2,?3,?4,0)",
        params![
            run.as_bytes().as_slice(),
            [1u8; 16].as_slice(),
            [1u8; 32].as_slice(),
            [2u8; 32].as_slice()
        ],
    )
    .unwrap();
    assert!(matches!(
        super::super::sqlite_v5::validate_run_operation(&db, run),
        Ok(ManagedExecutionKind::Migration)
    ));
    db.execute(
        "INSERT INTO run_migration_progress VALUES (?1,0,0,?2,?3)",
        params![
            run.as_bytes().as_slice(),
            [1u8; 32].as_slice(),
            [2u8; 16].as_slice()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO run_migration_payload_pins VALUES (?1,?2,?3)",
        params![
            run.as_bytes().as_slice(),
            instance.as_bytes().as_slice(),
            payload.as_bytes().as_slice()
        ],
    )
    .unwrap();
    let tx = db.transaction().unwrap();
    assert!(
        !super::super::sqlite_instances::reclaim_payload_if_unreferenced(&tx, instance, payload)
            .unwrap()
    );
    tx.execute(
        "INSERT INTO run_migration_checkpoint_bindings VALUES (?1,?2,?3,?4)",
        params![
            run.as_bytes().as_slice(),
            instance.as_bytes().as_slice(),
            b"config".as_slice(),
            payload.as_bytes().as_slice()
        ],
    )
    .unwrap();
    tx.execute("DELETE FROM run_migration_payload_pins", [])
        .unwrap();
    assert!(
        !super::super::sqlite_instances::reclaim_payload_if_unreferenced(&tx, instance, payload)
            .unwrap()
    );
    tx.execute("DELETE FROM run_migration_checkpoint_bindings", [])
        .unwrap();
    assert!(
        super::super::sqlite_instances::reclaim_payload_if_unreferenced(&tx, instance, payload)
            .unwrap()
    );
    tx.execute(
        "INSERT INTO run_action_invocations VALUES (?1,?2,?3,?4)",
        params![
            run.as_bytes().as_slice(),
            [1u8; 16].as_slice(),
            [1u8; 32].as_slice(),
            b"invalid-mixed-invocation".as_slice()
        ],
    )
    .unwrap();
    assert!(matches!(
        super::super::sqlite_v5::validate_run_operation(&tx, run),
        Err(PersistenceError::CorruptRun(_))
    ));
    tx.commit().unwrap();
}

// Test-only construction of an old database. This is not a production downgrade.
fn make_v5_fixture(root: &Path) {
    let mut db = Connection::open(db_path(root)).unwrap();
    configure_connection(&db).unwrap();
    if matches!(version(root), 7 | 8) {
        super::super::sqlite_v7::empty_v7_to_v6_fixture(&mut db);
    }
    validate_schema(&db, 6).unwrap();
    db.pragma_update(None, "foreign_keys", false).unwrap();
    let tx = db.transaction().unwrap();
    for table in MIGRATION_TABLES {
        assert_eq!(
            tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        tx.execute_batch(&format!("DROP TABLE {table};")).unwrap();
    }
    for table in [
        "run_capture_invocations",
        "run_restore_invocations",
        "run_operation_kinds",
        "writable_admissions",
    ] {
        tx.execute_batch(&format!(
            "CREATE TEMP TABLE saved_{table} AS SELECT * FROM {table}; DROP TABLE {table};"
        ))
        .unwrap();
    }
    tx.execute_batch(
        SCHEMA_V5_ADDITIONS_SQL
            .split("CREATE TABLE instance_recovery_consequence_versions")
            .next()
            .unwrap(),
    )
    .unwrap();
    for table in [
        "run_operation_kinds",
        "run_capture_invocations",
        "run_restore_invocations",
    ] {
        tx.execute_batch(&format!(
            "INSERT INTO {table} SELECT * FROM saved_{table}; DROP TABLE saved_{table};"
        ))
        .unwrap();
    }
    tx.execute_batch("INSERT INTO writable_admissions SELECT owner_session,5 FROM saved_writable_admissions; DROP TABLE saved_writable_admissions;").unwrap();
    tx.pragma_update(None, "user_version", 5).unwrap();
    validate_schema(&tx, 5).unwrap();
    tx.commit().unwrap();
}

// Test-ID: PR-TEST-0298
// Verifies: PR-REQ-0311, PR-REQ-0312, PR-REQ-0325
#[test]
fn populated_upgrade_preserves_all_old_rows_discriminators_orphans_and_bytes() {
    use sha2::{Digest, Sha256};
    let (_tmp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let bytes = b"never launched";
    let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
    let publication = p
        .put_runtime_content(&digest, &mut Cursor::new(bytes))
        .unwrap();
    let file = RuntimeFileV1 {
        id: ContentId::parse("tool").unwrap(),
        path: RuntimePath::parse("bin/tool").unwrap(),
        kind: RuntimeFileKindV1::RegularFile,
        blob_digest: digest,
        executable: true,
    };
    let hook = HookV1 {
        protocol_version: PositiveVersion::new(1).unwrap(),
        launch: HookLaunchV1::Direct {
            executable: file.id.clone(),
        },
        args: vec![],
        io: IOContractV1 {
            terminal: TerminalContractV1::None,
        },
    };
    let action = ActionIdentity::parse("inspect").unwrap();
    let input = InputIdentity::parse("config").unwrap();
    let core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
        inputs: vec![InputDeclarationV1 {
            id: input.clone(),
            required: false,
            protection: InputProtectionV1::Secret,
        }],
        actions: vec![ActionV1 {
            id: action.clone(),
            access: OperationAccessV1::Observe,
            parameters: vec![],
            hook: hook.clone(),
            outputs: vec![ManagedOutputV1 {
                id: ManagedOutputIdentity::parse("report").unwrap(),
            }],
        }],
        snapshot: Some(SnapshotCapabilityV1 {
            capture: Some(CaptureV1 {
                parameters: vec![],
                access: OperationAccessV1::Observe,
                hook: hook.clone(),
            }),
            restore: Some(RestoreV1 {
                parameters: vec![],
                hook,
            }),
        }),
        migrations: vec![],
        cleanup: None,
    })
    .unwrap();
    let content = validate_revision_content_v1(
        core,
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 {
            files: vec![file.clone()],
        })
        .unwrap(),
    )
    .unwrap();
    let revision = p
        .persist_revision(PackageId::from_bytes([1; 16]), &content, &[publication])
        .unwrap();
    let mut data = Cursor::new(b"old secret");
    let instance = p
        .create_instance(
            InstanceName::parse("v5-data").unwrap(),
            revision.clone(),
            &mut [ManagedInputWrite {
                input_id: input,
                byte_len: 10,
                reader: &mut data,
            }],
        )
        .unwrap();
    let owner = p.staging_session().unwrap().owner();
    for (tag, operation) in [
        (
            10,
            ManagedRunIdentity::Action(ActionRunIdentity {
                revision: revision.clone(),
                action: action.clone(),
            }),
        ),
        (
            11,
            ManagedRunIdentity::Action(ActionRunIdentity {
                revision: revision.clone(),
                action,
            }),
        ),
        (
            12,
            ManagedRunIdentity::Capture {
                revision: revision.clone(),
            },
        ),
        (
            13,
            ManagedRunIdentity::Restore {
                revision: revision.clone(),
                snapshot: SnapshotId::from_bytes([9; 16]),
            },
        ),
    ] {
        let run = RunId::from_bytes([tag; 16]);
        let current = p.load_instance_by_id(instance.id).unwrap().unwrap();
        p.create_accepted_managed_run(
            run,
            instance.id,
            current.state_version,
            &operation,
            &owner,
            &UnconditionalAcceptance,
        )
        .unwrap();
        if tag <= 11 {
            let observed = p
                .observe_instance_compilation_state(instance.id)
                .unwrap()
                .unwrap();
            let launch = CompiledHookLaunch::Direct {
                executable: file.clone(),
            };
            let files = [file.clone()];
            p.admit_run(
                run,
                &AdmissionFacts {
                    service: None,
                    expected_state_version: current.state_version,
                    active_bindings: &observed.active_bindings,
                    runtime_content: &files,
                    launch: &launch,
                },
                &|_| Ok(()),
                true,
            )
            .unwrap()
            .unwrap();
            p.open_recovery_risk(run).unwrap();
            if tag == 10 {
                let mut artifact = Cursor::new(b"artifact");
                p.finish_run(
                    run,
                    &RunFinish {
                        outcome: RunOutcome::Failed,
                        primary_failure: None,
                        secondary_failures: vec![],
                        hook_completion: None,
                    },
                    &mut [RunArtifactWrite {
                        output: ManagedOutputIdentity::parse("report").unwrap(),
                        byte_len: 8,
                        reader: &mut artifact,
                    }],
                )
                .unwrap();
            }
        }
    }
    // Opaque SQL Snapshot bytes are preservation evidence, not a validity claim.
    {
        let db = p.database.lock().unwrap();
        db.execute(
            "INSERT INTO snapshots VALUES (?1,2,?2,?3)",
            params![
                [9u8; 16].as_slice(),
                [7u8; 32].as_slice(),
                b"opaque historical bytes".as_slice()
            ],
        )
        .unwrap();
    }
    p.abandon_execution_owner();
    make_v5_fixture(&root);
    let before = contents(&Connection::open(db_path(&root)).unwrap());
    assert!(PactrunPersistence::upgrade_v5_to_v6_fixture(&root).unwrap());
    let db = Connection::open(db_path(&root)).unwrap();
    assert_eq!(contents(&db), before);
    assert_eq!(
        db.query_row("SELECT count(*) FROM writable_admissions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(db);
    let p = super::super::sqlite_v7::legacy_v6_read_fixture(&root);
    for tag in [11, 12, 13] {
        assert!(matches!(
            p.load_managed_run(RunId::from_bytes([tag; 16]))
                .unwrap()
                .unwrap()
                .state,
            RunState::Running(_)
        ));
    }
    assert_eq!(p.recovery_consequence_version(instance.id).unwrap(), 1);
    drop(p);
    // The historical V5 -> V6 result above is checked independently; the
    // historical V7 upgrade preserves every one of those legacy rows.
    assert!(PactrunPersistence::upgrade_storage_v7(&root).unwrap());
    let after = contents(&Connection::open(db_path(&root)).unwrap());
    for (table, rows) in &before {
        assert_eq!(after.get(table), Some(rows), "{table}");
    }
    for (table, rows) in &after {
        if !before.contains_key(table) {
            assert!(rows.is_empty(), "new service table {table} was synthesized");
        }
    }
}
