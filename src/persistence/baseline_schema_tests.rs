//! Fresh-schema bootstrap and refusal-before-side-effects contract proofs.
use crate::domain::VersionDomain;
use rusqlite::{Connection, params};

// Test-ID: PR-TEST-0621
// Verifies: PR-REQ-0078
#[test]
fn fresh_baseline_ddl_is_complete_and_matches_owning_specification() {
    let sql = include_str!("persistence_baseline.sql");
    let spec = include_str!("../../docs/spec/persistence/persistence-baseline.md");
    let block = spec
        .split_once("```sql\n")
        .unwrap()
        .1
        .split_once("```")
        .unwrap()
        .0;
    assert_eq!(sql.trim(), block.trim());
    assert!(!sql.contains("ALTER TABLE"));
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    let tx = db.unchecked_transaction().unwrap();
    tx.execute_batch(sql).unwrap();
    let version: String = tx
        .query_row(
            "SELECT format_version FROM pactrun_metadata WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, VersionDomain::Persistence.current_text());
    assert!(
        tx.execute("INSERT INTO pactrun_metadata VALUES (2, '1.0-alpha.1')", [])
            .is_err()
    );
    assert!(
        tx.execute("INSERT INTO pactrun_metadata VALUES (1, '1.0-alpha.1')", [])
            .is_err()
    );
    let owner = b"0123456789012345678901234567890123456789";
    assert!(
        tx.execute(
            "INSERT INTO writable_admissions VALUES (?1,11)",
            params![owner.as_slice()]
        )
        .is_err()
    );
    tx.execute(
        "INSERT INTO writable_admissions VALUES (?1,?2)",
        params![
            owner.as_slice(),
            super::sqlite_revision_store::SCHEMA_VERSION
        ],
    )
    .unwrap();
    for table in [
        "packages",
        "revisions",
        "instances",
        "runs",
        "writable_admissions",
        "run_diagnostic_collections",
        "run_diagnostic_events",
        "run_missing_input_causes",
    ] {
        let present: i64 = tx
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(present, 1, "{table}");
    }
    let integrity: String = tx
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
    tx.rollback().unwrap();
    let count: i64 = db
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 0,
        "interrupted initialization must not expose partial schema"
    );
}

fn root() -> tempfile::TempDir {
    let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/e-implementation/baseline-tests");
    std::fs::create_dir_all(&parent).unwrap();
    let temp = tempfile::tempdir_in(parent).unwrap();
    for name in ["database", "runtime-content"] {
        std::fs::create_dir(temp.path().join(name)).unwrap();
    }
    temp
}

// Test-ID: PR-TEST-0622
// Verifies: PR-REQ-0078, PR-REQ-0299
#[test]
fn baseline_bootstraps_reopens_and_rechecks_metadata_before_admitted_writes() {
    use super::{PactrunPersistence, sqlite_revision_store::APPLICATION_ID};
    let temp = root();
    let store = PactrunPersistence::open(temp.path()).unwrap();
    let path = temp.path().join("database/pactrun.sqlite3");
    let mut db = Connection::open(&path).unwrap();
    let application: i64 = db
        .pragma_query_value(None, "application_id", |r| r.get(0))
        .unwrap();
    let marker: i64 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(application, APPLICATION_ID);
    assert_eq!(marker, super::sqlite_revision_store::SCHEMA_VERSION);
    let admissions: i64 = db
        .query_row(
            "SELECT count(*) FROM writable_admissions WHERE admitted_schema_version=?1",
            [super::sqlite_revision_store::SCHEMA_VERSION],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(admissions, 1);
    let reader = PactrunPersistence::open_read_only(temp.path()).unwrap();
    assert!(reader.staging_session().is_none());
    drop(reader);
    db.execute(
        "UPDATE pactrun_metadata SET format_version='1.0-alpha.99'",
        [],
    )
    .unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert!(store.check_write_admission(&tx).is_err());
    tx.rollback().unwrap();
    assert!(PactrunPersistence::open_read_only(temp.path()).is_err());
    assert!(PactrunPersistence::open(temp.path()).is_err());
    db.execute(
        "UPDATE pactrun_metadata SET format_version='1.0-alpha.4'",
        [],
    )
    .unwrap();
    drop(store);
    let admissions: i64 = db
        .query_row("SELECT count(*) FROM writable_admissions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(admissions, 0);
    drop(db);
    let before = std::fs::read(&path).unwrap();
    drop(PactrunPersistence::open_read_only(temp.path()).unwrap());
    assert_eq!(before, std::fs::read(&path).unwrap());
    drop(PactrunPersistence::open(temp.path()).unwrap());
}

// Test-ID: PR-TEST-0623
// Verifies: PR-REQ-0078, PR-REQ-0299
#[test]
fn unsupported_storage_is_untouched_before_staging_or_coordination() {
    use super::{
        PactrunPersistence,
        sqlite_revision_store::{APPLICATION_ID, BASELINE_SQL, SCHEMA_VERSION},
    };
    use crate::managed_data::StagingSession;
    for case in 0..21 {
        let temp = root();
        let path = temp.path().join("database/pactrun.sqlite3");
        let db = Connection::open(&path).unwrap();
        if case < 11 {
            // Rejection depends on unsupported ownership/version, not an old codec.
            db.execute_batch("CREATE TABLE user_payload(value BLOB); INSERT INTO user_payload VALUES (x'010203');").unwrap();
            db.pragma_update(None, "application_id", APPLICATION_ID)
                .unwrap();
            db.pragma_update(None, "user_version", case + 1).unwrap();
        } else {
            db.execute_batch(BASELINE_SQL).unwrap();
            db.pragma_update(None, "application_id", APPLICATION_ID)
                .unwrap();
            db.pragma_update(None, "user_version", SCHEMA_VERSION)
                .unwrap();
            match case {
                11 => {
                    db.execute("UPDATE pactrun_metadata SET format_version='1.0'", [])
                        .unwrap();
                }
                12 => {
                    db.execute(
                        "UPDATE pactrun_metadata SET format_version='1.0-alpha.99'",
                        [],
                    )
                    .unwrap();
                }
                13 => {
                    db.execute("DELETE FROM pactrun_metadata", []).unwrap();
                }
                14 => {
                    db.execute_batch("CREATE TABLE unexpected(value BLOB)")
                        .unwrap();
                }
                15 => {
                    db.pragma_update(None, "application_id", 123).unwrap();
                }
                16 => {
                    db.execute(
                        "UPDATE pactrun_metadata SET format_version=?1",
                        ["x".repeat(4096)],
                    )
                    .unwrap();
                }
                17 => {
                    db.execute("UPDATE pactrun_metadata SET format_version='1'", [])
                        .unwrap();
                }
                18 => {
                    db.execute(
                        "UPDATE pactrun_metadata SET format_version='1.0-alpha.01'",
                        [],
                    )
                    .unwrap();
                }
                19 => {
                    db.execute(
                        "UPDATE pactrun_metadata SET format_version='1.0-alpha.99'",
                        [],
                    )
                    .unwrap();
                }
                20 => {
                    db.pragma_update(None, "user_version", 11).unwrap();
                }
                _ => unreachable!(),
            }
            if case >= 19 {
                let mode: String = db
                    .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
                    .unwrap();
                assert_eq!(mode, "wal");
            }
        }
        drop(db);
        let before = std::fs::read(&path).unwrap();
        let service = temp.path().join("service-owned-sentinel");
        std::fs::write(&service, b"untouched service data").unwrap();
        assert!(StagingSession::prepare(temp.path()).is_err(), "case {case}");
        assert!(
            PactrunPersistence::open(temp.path()).is_err(),
            "case {case}"
        );
        assert!(
            PactrunPersistence::open_read_only(temp.path()).is_err(),
            "case {case}"
        );
        assert!(
            PactrunPersistence::open_for_collection(temp.path()).is_err(),
            "case {case}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before, "case {case}");
        assert_eq!(std::fs::read(&service).unwrap(), b"untouched service data");
        assert!(!temp.path().join("staging").exists(), "case {case}");
        assert_eq!(
            std::fs::read_dir(temp.path().join("runtime-content"))
                .unwrap()
                .count(),
            0
        );
        for entry in std::fs::read_dir(temp.path().join("database")).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let allowed = name == "pactrun.sqlite3"
                || (case >= 19 && (name == "pactrun.sqlite3-wal" || name == "pactrun.sqlite3-shm"));
            assert!(allowed, "case {case}: unexpected database entry {name:?}");
            assert!(entry.file_type().unwrap().is_file());
            if name == "pactrun.sqlite3-wal" {
                assert!(std::fs::read(entry.path()).unwrap().is_empty());
            }
        }
    }
}

// Test-ID: PR-TEST-0639
// Verifies: PR-REQ-0078, PR-REQ-0299
#[test]
fn unsupported_wal_inspection_preserves_committed_frames_and_the_live_writer() {
    use super::{
        PactrunPersistence,
        sqlite_revision_store::{APPLICATION_ID, BASELINE_SQL},
    };
    use crate::managed_data::StagingSession;
    let temp = root();
    let database_path = temp.path().join("database/pactrun.sqlite3");
    let wal_path = temp.path().join("database/pactrun.sqlite3-wal");
    let writer = Connection::open(&database_path).unwrap();
    writer.execute_batch(BASELINE_SQL).unwrap();
    writer
        .pragma_update(None, "application_id", APPLICATION_ID)
        .unwrap();
    writer.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let main_before = std::fs::read(&database_path).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE; UPDATE pactrun_metadata SET format_version='1.0-alpha.4'; INSERT INTO packages VALUES(zeroblob(16)); COMMIT;").unwrap();
    let committed_wal = std::fs::read(&wal_path).unwrap();
    assert!(!committed_wal.is_empty());
    assert_eq!(std::fs::read(&database_path).unwrap(), main_before);
    // The main file still describes the supported baseline. The committed WAL
    // must be read, not ignored through immutable mode or removed as temporary.
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(StagingSession::prepare(temp.path()).is_err());
    assert!(PactrunPersistence::open(temp.path()).is_err());
    assert!(PactrunPersistence::open_read_only(temp.path()).is_err());
    assert!(PactrunPersistence::open_for_collection(temp.path()).is_err());
    assert_eq!(std::fs::read(&database_path).unwrap(), main_before);
    assert_eq!(std::fs::read(&wal_path).unwrap(), committed_wal);
    assert!(!temp.path().join("staging").exists());
    assert_eq!(
        std::fs::read_dir(temp.path().join("runtime-content"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        writer
            .query_row("SELECT count(*) FROM packages", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    writer
        .execute("INSERT INTO packages VALUES(?1)", [[1_u8; 16].as_slice()])
        .unwrap();
    writer.execute_batch("COMMIT").unwrap();
    assert_eq!(
        writer
            .query_row("SELECT count(*) FROM packages", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
