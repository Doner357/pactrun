use super::*;
use crate::persistence::sqlite_revision_store::{APPLICATION_ID, SCHEMA_LADDER};
use rusqlite::params;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const WORKER: &str = "persistence::sqlite_v8::upgrade_tests::v8_worker";
const TABLES: [&str; 10] = [
    "service_storage_allocations",
    "service_storage_preparations",
    "service_storage_protections",
    "service_storage_run_origins",
    "instance_service_storages",
    "instance_service_resources",
    "run_service_storage_pins",
    "run_service_storage_targets",
    "run_service_resource_targets",
    "run_service_edge_commits",
];
fn root() -> (tempfile::TempDir, PathBuf) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m7-v8-tests");
    fs::create_dir_all(&parent).unwrap();
    let temp = tempfile::tempdir_in(parent).unwrap();
    let root = temp.path().join("store");
    for p in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(p)).unwrap();
    }
    (temp, root)
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
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap()
}
fn wait(path: &Path) {
    let end = Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        assert!(Instant::now() < end, "barrier timeout");
        thread::sleep(Duration::from_millis(5));
    }
}
struct Worker(std::process::Child);
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn worker(root: &Path, mode: &str, extra: &[(&str, &str)]) -> Worker {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", WORKER, "--nocapture"])
        .env("PACTRUN_V8_ROOT", root)
        .env("PACTRUN_V8_MODE", mode)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    for (k, v) in extra {
        cmd.env(k, v);
    }
    Worker(cmd.spawn().unwrap())
}
#[test]
fn v8_worker() {
    let Some(root) = std::env::var_os("PACTRUN_V8_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    match std::env::var("PACTRUN_V8_MODE").unwrap().as_str() {
        "upgrade" => {
            assert!(PactrunPersistence::upgrade_legacy_v7_to_v8(&root).unwrap());
        }
        "held" | "late" => {
            let session = StagingSession::prepare(&root).unwrap();
            let mut db = Connection::open(db_path(&root)).unwrap();
            configure_connection(&db).unwrap();
            let late = std::env::var("PACTRUN_V8_MODE").unwrap() == "late";
            if !late {
                let tx = db
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .unwrap();
                validate_schema(&tx, 7).unwrap();
                tx.execute(
                    "INSERT INTO writable_admissions VALUES(?1,7)",
                    [session.owner().as_str().as_bytes()],
                )
                .unwrap();
                tx.commit().unwrap();
            }
            fs::write(root.join("writer-ready"), []).unwrap();
            wait(&root.join("writer-release"));
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            if late {
                assert!(validate_schema(&tx, 7).is_err());
                fs::write(root.join("legacy-refused"), []).unwrap();
            } else {
                tx.execute(
                    "DELETE FROM writable_admissions WHERE owner_session=?1",
                    [session.owner().as_str().as_bytes()],
                )
                .unwrap();
                tx.commit().unwrap();
            }
        }
        _ => panic!("unknown worker mode"),
    }
}

// Test-ID: PR-TEST-0400
// Verifies: PR-REQ-0338
#[test]
fn v8_exact_ddl_preserves_legacy_tables_and_protects_allocation_custody() {
    let document =
        include_str!("../../docs/spec/persistence/persistence-schema-v8.md").replace("\r\n", "\n");
    let approved = document
        .split("```sql\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    assert_eq!(
        approved.trim(),
        SCHEMA_V8_ADDITIONS_SQL.replace("\r\n", "\n").trim()
    );
    let (_temp, root) = root();
    source(&root, 8);
    assert_eq!(version(&root), 8);
    let db = Connection::open(db_path(&root)).unwrap();
    configure_connection(&db).unwrap();
    validate_schema(&db, 8).unwrap();
    for t in TABLES {
        assert_eq!(
            db.query_row(
                "SELECT wr,strict FROM pragma_table_list WHERE name=?1",
                [t],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            )
            .unwrap(),
            (1, 1)
        );
    }
    // SQL edge probes only; Domain ownership/contract validation is separate.
    db.execute(
        "INSERT INTO service_storage_allocations VALUES(?1,?2,?3,?4,?5)",
        params![
            [1u8; 16].as_slice(),
            [2u8; 16].as_slice(),
            [3u8; 16].as_slice(),
            [4u8; 32].as_slice(),
            b"state".as_slice()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO service_storage_protections VALUES(?1)",
        [[1u8; 16].as_slice()],
    )
    .unwrap();
    assert!(
        db.execute("DELETE FROM service_storage_allocations", [])
            .is_err()
    );
    assert!(
        db.execute(
            "INSERT INTO run_service_storage_pins VALUES(?1,?2)",
            params![[9u8; 16].as_slice(), [1u8; 16].as_slice()]
        )
        .is_err()
    );
}

// Test-ID: PR-TEST-0401
// Verifies: PR-REQ-0339
#[test]
fn only_exact_v7_upgrades_without_allocating_or_replaying_any_service() {
    let (_temp, root) = root();
    assert!(PactrunPersistence::upgrade_legacy_v7_to_v8(&root).is_err());
    assert!(!db_path(&root).exists());
    for old in 1..=8 {
        let (_temp, root) = self::root();
        source(&root, old);
        let result = PactrunPersistence::upgrade_legacy_v7_to_v8(&root);
        match old {
            7 => assert!(result.unwrap()),
            8 => assert!(!result.unwrap()),
            _ => assert!(result.is_err()),
        }
        assert_eq!(version(&root), if old >= 7 { 8 } else { old as i64 });
        assert!(!root.join("service-storage").exists());
        if old == 7 {
            let db = Connection::open(db_path(&root)).unwrap();
            configure_connection(&db).unwrap();
            for t in TABLES {
                assert_eq!(
                    db.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r
                        .get::<_, i64>(0))
                        .unwrap(),
                    0
                );
            }
            assert_eq!(
                db.query_row("SELECT count(*) FROM writable_admissions", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
    for (app, v, drift) in [
        (APPLICATION_ID, 9, false),
        (17, 7, false),
        (APPLICATION_ID, 7, true),
        (0, 0, true),
    ] {
        let (_temp, root) = self::root();
        source(&root, 7);
        let db = Connection::open(db_path(&root)).unwrap();
        db.pragma_update(None, "application_id", app).unwrap();
        db.pragma_update(None, "user_version", v).unwrap();
        if drift {
            db.execute_batch("CREATE TABLE drift(value INTEGER)")
                .unwrap();
        }
        drop(db);
        assert!(PactrunPersistence::upgrade_legacy_v7_to_v8(&root).is_err());
        assert_eq!(version(&root), v);
    }
}

// Test-ID: PR-TEST-0402
// Verifies: PR-REQ-0339
#[test]
fn upgrade_rechecks_live_unknown_and_late_v7_writer_admissions() {
    let (_temp, root) = root();
    source(&root, 7);
    let mut held = worker(&root, "held", &[]);
    wait(&root.join("writer-ready"));
    assert!(matches!(
        PactrunPersistence::upgrade_legacy_v7_to_v8(&root),
        Err(PersistenceError::ActiveWriters)
    ));
    assert_eq!(version(&root), 7);
    fs::write(root.join("writer-release"), []).unwrap();
    assert!(held.0.wait().unwrap().success());
    let unknown = "session-dddddddddddddddddddddddddddddddd";
    fs::create_dir(root.join("staging").join(unknown)).unwrap();
    let db = Connection::open(db_path(&root)).unwrap();
    db.execute(
        "INSERT INTO writable_admissions VALUES(?1,7)",
        [unknown.as_bytes()],
    )
    .unwrap();
    assert!(matches!(
        PactrunPersistence::upgrade_legacy_v7_to_v8(&root),
        Err(PersistenceError::ActiveWriters)
    ));
    db.execute("DELETE FROM writable_admissions", []).unwrap();
    drop(db);
    assert!(PactrunPersistence::upgrade_legacy_v7_to_v8(&root).unwrap());
    let (_temp, root) = self::root();
    source(&root, 7);
    let barrier = root.join("upgrade-barrier");
    fs::create_dir(&barrier).unwrap();
    let mut upgrader = worker(
        &root,
        "upgrade",
        &[
            ("PACTRUN_M4_SYNC", "after_v7_admission_inspection"),
            ("PACTRUN_M4_SYNC_DIR", barrier.to_str().unwrap()),
        ],
    );
    wait(&barrier.join("ready"));
    let mut late = worker(&root, "late", &[]);
    wait(&root.join("writer-ready"));
    fs::write(root.join("writer-release"), []).unwrap();
    fs::write(barrier.join("release"), []).unwrap();
    assert!(upgrader.0.wait().unwrap().success());
    assert!(late.0.wait().unwrap().success());
    assert!(root.join("legacy-refused").exists());
    assert_eq!(version(&root), 8);
}

// Test-ID: PR-TEST-0403
// Verifies: PR-REQ-0339
#[test]
fn upgrade_crashes_leave_exact_versions_without_rewriting_or_inventing_history() {
    for (point, expected) in [
        ("before_schema_migration_commit", 7),
        ("after_schema_migration_commit", 8),
    ] {
        let (_temp, root) = root();
        source(&root, 7);
        let db = Connection::open(db_path(&root)).unwrap();
        db.execute("INSERT INTO packages VALUES(?1)", [[5u8; 16].as_slice()])
            .unwrap();
        db.execute(
            "INSERT INTO revisions VALUES(?1,?2,?3,?4)",
            params![
                [5u8; 16].as_slice(),
                [6u8; 32].as_slice(),
                b"opaque core bytes".as_slice(),
                b"opaque closure bytes".as_slice()
            ],
        )
        .unwrap();
        drop(db);
        let mut child = worker(&root, "upgrade", &[("PACTRUN_M4_FAULT", point)]);
        assert_eq!(child.0.wait().unwrap().code(), Some(87));
        let db = Connection::open(db_path(&root)).unwrap();
        validate_schema(&db, expected).unwrap();
        assert_eq!(version(&root), expected);
        let bytes: (Vec<u8>, Vec<u8>) = db
            .query_row(
                "SELECT core_jcs,runtime_content_jcs FROM revisions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            bytes,
            (
                b"opaque core bytes".to_vec(),
                b"opaque closure bytes".to_vec()
            )
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM runs", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(!root.join("service-storage").exists());
    }
}
