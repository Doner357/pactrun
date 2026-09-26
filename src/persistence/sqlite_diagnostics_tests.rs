use super::*;
use crate::persistence::sqlite_revision_store::APPLICATION_ID;
use std::fs;
// Test-ID: PR-TEST-0524
// Verifies: PR-REQ-0352
#[test]
fn diagnostics_baseline_refuses_unsupported_storage_without_inventing_evidence() {
    let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/e-implementation/diagnostic-baseline");
    fs::create_dir_all(&parent).unwrap();
    for version in 1..=11 {
        let temp = tempfile::tempdir_in(&parent).unwrap();
        let root = temp.path();
        for name in ["database", "runtime-content"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        let path = root.join("database/pactrun.sqlite3");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE retained(value BLOB); INSERT INTO retained VALUES(x'01');")
            .unwrap();
        db.pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        db.pragma_update(None, "user_version", version).unwrap();
        drop(db);
        let before = fs::read(&path).unwrap();
        assert!(PactrunPersistence::open_read_only(root).is_err());
        assert!(PactrunPersistence::open(root).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(!root.join("staging").exists());
    }
}
