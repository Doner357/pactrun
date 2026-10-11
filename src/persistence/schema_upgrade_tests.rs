use super::*;
use crate::{
    application::PactrunApplication,
    domain::*,
    hook::ActionCancellation,
    persistence::{PactrunPersistence, sqlite_revision_store::ALPHA1_SQL},
};
use rusqlite::params;
use std::{fs, path::PathBuf};

fn legacy() -> (tempfile::TempDir, PathBuf, RevisionIdentity) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/schema-upgrade-tests");
    fs::create_dir_all(&parent).unwrap();
    let temp = tempfile::tempdir_in(parent).unwrap();
    let root = temp.path().join("store");
    for child in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(child)).unwrap();
    }
    let source = temp.path().join("pack");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("pactrun.yaml"),"source_format: 1.0-alpha.2\npackage_id: 00000000000000000000000000000042\nrevision: {}\nruntime_content: {}\n").unwrap();
    let app = PactrunApplication::open(&root).unwrap();
    let id = app
        .install_pack(
            &source,
            PackMetadataConflict::Reject,
            &ActionCancellation::default(),
        )
        .unwrap()
        .revision;
    app.create_instance(InstanceName::parse("kept").unwrap(), id.clone(), vec![])
        .unwrap();
    drop(app);
    downgrade_fixture(&root);
    (temp, root, id)
}

fn downgrade_fixture(root: &Path) {
    let db = Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
    db.execute_batch(
        "DROP TABLE run_core_diagnostic_events; DROP TABLE run_core_diagnostic_collections;",
    )
    .unwrap();
    db.execute_batch("DROP TABLE run_missing_input_causes;")
        .unwrap();
    db.execute_batch("DROP TABLE package_local_names; DROP TABLE revision_local_names; DROP TABLE revision_installations; DROP TABLE writable_admissions;").unwrap();
    let admission = ALPHA1_SQL
        .split_once("CREATE TABLE writable_admissions (")
        .unwrap()
        .1
        .split_once(';')
        .unwrap()
        .0;
    db.execute_batch(&format!("CREATE TABLE writable_admissions ({admission};"))
        .unwrap();
    db.execute(
        "UPDATE pactrun_metadata SET format_version='1.0-alpha.1'",
        [],
    )
    .unwrap();
    db.pragma_update(None, "user_version", 0).unwrap();
    validate_schema(&db, 0).unwrap();
}

// Test-ID: PR-TEST-0687
// Verifies: PR-REQ-0373, PR-REQ-0078
#[test]
fn alpha2_upgrade_preserves_names_and_refuses_live_old_writers() {
    for live_writer in [false, true] {
        let (_temp, root, revision) = legacy();
        let app = PactrunApplication::open(&root).unwrap();
        app.rename_package(
            revision.package_id,
            Some(&LocalName::parse("kept-package").unwrap()),
        )
        .unwrap();
        drop(app);
        let session =
            live_writer.then(|| crate::managed_data::StagingSession::prepare(&root).unwrap());
        let db = Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
        db.execute_batch(
            "DROP TABLE run_core_diagnostic_events; DROP TABLE run_core_diagnostic_collections;",
        )
        .unwrap();
        db.execute_batch("DROP TABLE run_missing_input_causes; DROP TABLE writable_admissions;")
            .unwrap();
        let admission = super::super::sqlite_revision_store::ALPHA2_SQL
            .split_once("CREATE TABLE writable_admissions (")
            .unwrap()
            .1
            .split_once(';')
            .unwrap()
            .0;
        db.execute_batch(&format!("CREATE TABLE writable_admissions ({admission}; UPDATE pactrun_metadata SET format_version='1.0-alpha.2'; PRAGMA user_version=1;")).unwrap();
        if let Some(session) = &session {
            db.execute(
                "INSERT INTO writable_admissions VALUES (?1,1)",
                [session.owner().as_str().as_bytes()],
            )
            .unwrap();
            assert!(matches!(
                PactrunPersistence::open_read_only(&root),
                Err(PersistenceError::ActiveWriters)
            ));
            validate_schema(&db, 1).unwrap();
        }
        drop(session);
        std::thread::scope(|scope| {
            let readers: Vec<_> = (0..3)
                .map(|_| scope.spawn(|| PactrunPersistence::open_read_only(&root).map(drop)))
                .collect();
            for reader in readers {
                reader.join().unwrap().unwrap();
            }
        });
        validate_schema(&db, SCHEMA_VERSION).unwrap();
        assert_eq!(
            db.query_row("SELECT name FROM package_local_names", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "kept-package"
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM run_missing_input_causes", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0,
            "upgrade does not invent historical causes"
        );
        let app = PactrunApplication::open(&root).unwrap();
        let instance = app
            .resolve_instance_name(&InstanceName::parse("kept").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            app.load_instance(instance)
                .unwrap()
                .unwrap()
                .active_revision,
            revision
        );
    }
}
fn alias(db: &Connection, id: &RevisionIdentity, value: &str) {
    db.execute("INSERT INTO revision_local_aliases(alias_utf8,package_id,revision_content_digest) VALUES (?1,?2,?3)",params![value.as_bytes(),id.package_id.as_bytes().as_slice(),id.content_digest.as_bytes().as_slice()]).unwrap();
}

// Test-ID: PR-TEST-0692
// Verifies: PR-REQ-0373, PR-REQ-0078, PR-REQ-0378, PR-REQ-0352
#[test]
fn alpha3_upgrade_adds_empty_core_history_and_preserves_identity() {
    for live_writer in [false, true] {
        let (_temp, root, revision) = legacy();
        let app = PactrunApplication::open(&root).unwrap();
        let name = InstanceName::parse("kept").unwrap();
        let instance = app.resolve_instance_name(&name).unwrap().unwrap();
        drop(app);
        let session =
            live_writer.then(|| crate::managed_data::StagingSession::prepare(&root).unwrap());
        let db = Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
        db.execute_batch("DROP TABLE run_core_diagnostic_events; DROP TABLE run_core_diagnostic_collections; DROP TABLE writable_admissions;").unwrap();
        let ddl = super::super::sqlite_revision_store::ALPHA3_SQL
            .split_once("CREATE TABLE writable_admissions (")
            .unwrap()
            .1
            .split_once(';')
            .unwrap()
            .0;
        db.execute_batch(&format!("CREATE TABLE writable_admissions ({ddl}; UPDATE pactrun_metadata SET format_version='1.0-alpha.3'; PRAGMA user_version=2;")).unwrap();
        validate_schema(&db, 2).unwrap();
        if let Some(session) = &session {
            db.execute(
                "INSERT INTO writable_admissions VALUES (?1,2)",
                [session.owner().as_str().as_bytes()],
            )
            .unwrap();
            assert!(matches!(
                PactrunPersistence::open_read_only(&root),
                Err(PersistenceError::ActiveWriters)
            ));
            validate_schema(&db, 2).unwrap();
        }
        drop(session);
        let reopened = PactrunApplication::open(&root).unwrap();
        assert_eq!(
            reopened.resolve_instance_name(&name).unwrap(),
            Some(instance)
        );
        assert_eq!(
            reopened
                .load_instance(instance)
                .unwrap()
                .unwrap()
                .active_revision,
            revision
        );
        validate_schema(&db, SCHEMA_VERSION).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM run_core_diagnostic_collections",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}

// Test-ID: PR-TEST-0665
// Verifies: PR-REQ-0373, PR-REQ-0372
#[test]
fn upgrade_on_read_preserves_instance_identity_and_unknown_installation_time() {
    let (_temp, root, id) = legacy();
    let db = Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
    alias(&db, &id, "stable");
    drop(db);
    let app = PactrunApplication::open_read_only(&root).unwrap();
    let facts = app.local_revision_facts(&id).unwrap();
    assert!(facts.package_name.is_none());
    assert_eq!(facts.revision_name.unwrap().as_str(), "stable");
    assert!(facts.installed_at_unix_ms.is_none());
    let reference = LocalRevisionReference::parse(&format!("{}:stable", id.package_id)).unwrap();
    assert_eq!(app.resolve_named_revision(&reference).unwrap(), id);
    let instance = app
        .resolve_instance_name(&InstanceName::parse("kept").unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        app.load_instance(instance)
            .unwrap()
            .unwrap()
            .active_revision,
        id
    );
    drop(app);
    assert!(PactrunPersistence::open_read_only(&root).is_ok());
    let db = Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
    validate_schema(&db, SCHEMA_VERSION).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM writable_admissions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

// Test-ID: PR-TEST-0666
// Verifies: PR-REQ-0373
#[test]
fn upgrade_refuses_lossy_alias_mapping_without_changing_old_schema() {
    for aliases in [
        vec!["stable", "testing"],
        vec!["\u{4e2d}\u{6587}"],
        vec!["with space"],
        vec!["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
    ] {
        let (_temp, root, id) = legacy();
        let db = Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
        for value in &aliases {
            alias(&db, &id, value);
        }
        assert!(PactrunPersistence::open_read_only(&root).is_err());
        validate_schema(&db, 0).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM revision_local_aliases", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            aliases.len() as i64
        );
    }
}

// Test-ID: PR-TEST-0667
// Verifies: PR-REQ-0373
#[test]
fn upgrade_refuses_live_legacy_writer_and_succeeds_after_confirmed_owner_loss() {
    let (_temp, root, _id) = legacy();
    let session = crate::managed_data::StagingSession::prepare(&root).unwrap();
    // Preparing with the new executable upgrades; restore the old schema while
    // retaining a real live owner lease to model an old executable's admission.
    downgrade_fixture(&root);
    let db = Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
    db.execute(
        "INSERT INTO writable_admissions VALUES (?1,0)",
        [session.owner().as_str().as_bytes()],
    )
    .unwrap();
    assert!(matches!(
        PactrunPersistence::open_read_only(&root),
        Err(PersistenceError::ActiveWriters)
    ));
    validate_schema(&db, 0).unwrap();
    drop(session);
    assert!(PactrunPersistence::open_read_only(&root).is_ok());
    validate_schema(&db, SCHEMA_VERSION).unwrap();
}

// Test-ID: PR-TEST-0668
// Verifies: PR-REQ-0373
#[test]
fn concurrent_upgrade_openings_observe_one_complete_schema() {
    let (_temp, root, _id) = legacy();
    std::thread::scope(|scope| {
        let threads: Vec<_> = (0..3)
            .map(|_| scope.spawn(|| PactrunPersistence::open_read_only(&root).map(drop)))
            .collect();
        for thread in threads {
            thread.join().unwrap().unwrap();
        }
    });
    let db = Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
    validate_schema(&db, SCHEMA_VERSION).unwrap();
}
