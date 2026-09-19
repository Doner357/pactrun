use super::*;
use crate::persistence::sqlite_revision_store::{APPLICATION_ID, SCHEMA_LADDER, validate_schema};
use std::fs;
// Test-ID: PR-TEST-0524
// Verifies: PR-REQ-0352
#[test]
fn evidence_upgrade_accepts_exact_supported_sources_and_refuses_schema_drift() {
    let parent =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/evidence-upgrade-tests");
    fs::create_dir_all(&parent).unwrap();
    for source in [8, 9, 10] {
        let temp = tempfile::tempdir_in(&parent).unwrap();
        let root = temp.path();
        for name in ["database", "runtime-content", "staging"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        let path = root.join("database/pactrun.sqlite3");
        let db = rusqlite::Connection::open(&path).unwrap();
        for (sql, _) in &SCHEMA_LADDER[..source] {
            db.execute_batch(sql).unwrap();
        }
        db.pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        db.pragma_update(None, "user_version", source as i64)
            .unwrap();
        assert!(matches!(
            PactrunPersistence::open_read_only(root),
            Err(PersistenceError::UpgradeRequired)
        ));
        db.execute_batch("CREATE TABLE unexpected_evidence(value INTEGER);")
            .unwrap();
        assert!(PactrunPersistence::upgrade_storage(root).is_err());
        assert_eq!(
            db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            source as i64
        );
        db.execute_batch("DROP TABLE unexpected_evidence;").unwrap();
        assert!(PactrunPersistence::upgrade_storage(root).unwrap());
        validate_schema(&db, 11).unwrap();
        assert!(!PactrunPersistence::upgrade_storage(root).unwrap());
        assert_eq!(
            db.query_row("SELECT count(*) FROM run_diagnostic_collections", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
