use super::*;
use crate::domain::*;
use crate::persistence::sqlite_revision_store::legacy_v4_open_database;
use crate::persistence::{RunArtifactWrite, UnconditionalAcceptance};
use std::{
    collections::BTreeMap,
    io::Cursor,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

const WORKER: &str = "persistence::sqlite_v5::tests::v5_worker";

fn root() -> (TempDir, PathBuf) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m4-s2-tests");
    fs::create_dir_all(&parent).unwrap();
    let tmp = tempfile::Builder::new()
        .prefix("v5-")
        .tempdir_in(parent)
        .unwrap();
    let root = tmp.path().join("store");
    for child in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(child)).unwrap();
    }
    (tmp, root)
}
fn db_path(root: &Path) -> PathBuf {
    root.join("database/pactrun.sqlite3")
}
fn version(root: &Path) -> i64 {
    Connection::open(db_path(root))
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap()
}
fn legacy(root: &Path) {
    drop(legacy_v4_open_database(&db_path(root)).unwrap());
}
fn count(root: &Path, table: &str) -> i64 {
    Connection::open(db_path(root))
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn wait_file(path: &Path) {
    let end = Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        assert!(Instant::now() < end, "test barrier timed out");
        thread::sleep(Duration::from_millis(10));
    }
}
struct Worker(Child);
impl Worker {
    fn wait(mut self, expected: i32) {
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
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", WORKER, "--nocapture"])
        .env("PACTRUN_M4_MODE", mode)
        .env("PACTRUN_M4_ROOT", root)
        .env_remove("PACTRUN_M4_FAULT")
        .env_remove("PACTRUN_M4_SYNC")
        .env_remove("PACTRUN_M4_SYNC_DIR")
        .env_remove("PACTRUN_M1C_FAULT")
        .env_remove("PACTRUN_M1D_FAULT")
        .env_remove("PACTRUN_M3_FAULT")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    for (key, value) in extra {
        command.env(key, value);
    }
    Worker(command.spawn().unwrap())
}

#[test]
fn v5_worker() {
    let Some(mode) = std::env::var_os("PACTRUN_M4_MODE") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("PACTRUN_M4_ROOT").unwrap());
    match mode.to_str().unwrap() {
        "open" => {
            let _p = PactrunPersistence::open(&root).unwrap();
        }
        "writer-held" => {
            let _p = PactrunPersistence::open(&root).unwrap();
            fs::write(root.join("held"), b"admitted writer").unwrap();
            wait_file(&root.join("release"));
        }
        "upgrade" => {
            assert!(PactrunPersistence::upgrade_legacy_v4_to_v5(&root).unwrap());
        }
        "legacy-held" => {
            let _session = StagingSession::open(&root).unwrap();
            let _db = legacy_v4_open_database(&db_path(&root)).unwrap();
            fs::write(root.join("held"), b"ready").unwrap();
            wait_file(&root.join("release"));
        }
        "legacy-late" => {
            let _session = StagingSession::open(&root).unwrap();
            let result = legacy_v4_open_database(&db_path(&root));
            let error = result.unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("legacy V4 binary rejects newer schema")
            );
            fs::write(root.join("legacy-refused"), b"no V4 write").unwrap();
        }
        "prepared-v5-late" => {
            let session = StagingSession::prepare(&root).unwrap();
            let result = PactrunPersistence::open_prepared(&root, session);
            assert!(result.unwrap_err().to_string().contains("newer"));
            fs::write(root.join("v5-refused"), b"no V5 write").unwrap();
        }
        other => panic!("unknown worker mode: {other}"),
    }
}

// Test-ID: PR-TEST-0195
// Verifies: PR-REQ-0298, PR-REQ-0299, PR-REQ-0300
#[test]
fn source_version_matrix_preserves_historical_v5_and_current_initialization() {
    let (_tmp, root) = root();
    assert!(PactrunPersistence::upgrade_storage(&root).is_err());
    assert!(!db_path(&root).exists());
    let p = PactrunPersistence::open(&root).unwrap();
    assert_eq!(version(&root), SCHEMA_VERSION);
    validate_schema(&p.database.lock().unwrap(), SCHEMA_VERSION).unwrap();
    assert!(!PactrunPersistence::upgrade_storage(&root).unwrap());
    assert_eq!(count(&root, "writable_admissions"), 1);
    drop(p);
    assert_eq!(count(&root, "writable_admissions"), 0);
    for v in 1..=4 {
        let (_tmp, old) = self::root();
        let db = Connection::open(db_path(&old)).unwrap();
        for (sql, _) in &SCHEMA_LADDER[..v] {
            db.execute_batch(sql).unwrap();
        }
        db.pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        db.pragma_update(None, "user_version", v as i64).unwrap();
        drop(db);
        let residue = old.join("staging/session-cccccccccccccccccccccccccccccccc");
        fs::create_dir(&residue).unwrap();
        fs::write(residue.join(".lease"), []).unwrap();
        fs::write(
            residue.join("sentinel"),
            b"unqualified opening must not clean other sessions",
        )
        .unwrap();
        assert!(PactrunPersistence::open(&old).is_err());
        assert!(residue.join("sentinel").exists());
        assert_eq!(version(&old), v as i64);
        if v == 4 {
            assert!(PactrunPersistence::upgrade_storage(&old).is_err());
            assert!(PactrunPersistence::upgrade_legacy_v4_to_v5(&old).unwrap());
            assert_eq!(version(&old), 5);
        } else {
            assert!(PactrunPersistence::upgrade_storage(&old).is_err());
            assert_eq!(version(&old), v as i64);
        }
    }
}

// Test-ID: PR-TEST-0196
// Verifies: PR-REQ-0299
#[test]
fn admission_is_durable_owned_and_checked_before_every_write() {
    let (_tmp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let owner = p.staging_session().unwrap().owner();
    assert_eq!(probe_session_owner(&root, &owner), SessionOwnerProbe::Live);
    assert_eq!(count(&root, "writable_admissions"), 1);
    let ro = PactrunPersistence::open_read_only(&root).unwrap();
    assert!(ro.staging_session().is_none());
    assert!(matches!(
        ro.put_runtime_content(
            &Sha256Digest::from_bytes([0; 32]),
            &mut Cursor::new(b"not published")
        ),
        Err(PersistenceError::WriterAdmissionRequired)
    ));
    assert_eq!(count(&root, "writable_admissions"), 1);
    drop(ro);
    let mut db = p.database.lock().unwrap();
    db.execute_batch("CREATE TABLE unexpected(value INTEGER) STRICT;")
        .unwrap();
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(matches!(
            p.check_write_admission(&tx),
            Err(PersistenceError::SchemaMismatch(_))
        ));
    }
    db.execute_batch("DROP TABLE unexpected;").unwrap();
    db.execute("DELETE FROM writable_admissions", []).unwrap();
    let transaction = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(matches!(
        p.check_write_admission(&transaction),
        Err(PersistenceError::WriterAdmissionRequired)
    ));
    drop(transaction);
    drop(db);
    drop(p);
    assert_eq!(
        probe_session_owner(&root, &owner),
        SessionOwnerProbe::ConfirmedLoss
    );
    let p = PactrunPersistence::open(&root).unwrap();
    let owner = p.staging_session().unwrap().owner();
    p.abandon_execution_owner();
    assert_eq!(count(&root, "writable_admissions"), 1);
    assert_eq!(
        probe_session_owner(&root, &owner),
        SessionOwnerProbe::ConfirmedLoss
    );
    let mut db = Connection::open(db_path(&root)).unwrap();
    configure_connection(&db).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    require_quiescent_admissions(&tx, &root).unwrap();
}

// Test-ID: PR-TEST-0197
// Verifies: PR-REQ-0299
#[test]
fn pre_admission_does_not_block_but_live_and_unknown_admitted_owners_do() {
    let (_tmp, root) = root();
    drop(PactrunPersistence::open(&root).unwrap());
    let prepared = StagingSession::open(&root).unwrap();
    let mut db = Connection::open(db_path(&root)).unwrap();
    configure_connection(&db).unwrap();
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        require_quiescent_admissions(&tx, &root).unwrap();
    }
    let p = PactrunPersistence::open_prepared(&root, prepared).unwrap();
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(matches!(
            require_quiescent_admissions(&tx, &root),
            Err(PersistenceError::ActiveWriters)
        ));
    }
    drop(p);
    let unknown = "session-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let child = worker(&root, "writer-held", &[]);
    wait_file(&root.join("held"));
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(matches!(
            require_quiescent_admissions(&tx, &root),
            Err(PersistenceError::ActiveWriters)
        ));
    }
    fs::write(root.join("release"), b"close writer").unwrap();
    child.wait(0);
    assert_eq!(count(&root, "writable_admissions"), 0);
    fs::create_dir(root.join("staging").join(unknown)).unwrap();
    db.execute(
        "INSERT INTO writable_admissions VALUES (?1,?2)",
        params![unknown.as_bytes(), SCHEMA_VERSION],
    )
    .unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(matches!(
        require_quiescent_admissions(&tx, &root),
        Err(PersistenceError::ActiveWriters)
    ));
}

// Test-ID: PR-TEST-0198
// Verifies: PR-REQ-0299
#[test]
fn prepared_writer_revalidates_after_the_serialized_boundary_in_another_process() {
    let (_tmp, root) = root();
    drop(PactrunPersistence::open(&root).unwrap());
    let sync = root.join("sync");
    fs::create_dir(&sync).unwrap();
    let child = worker(
        &root,
        "prepared-v5-late",
        &[
            ("PACTRUN_M4_SYNC", "before_writable_admission"),
            ("PACTRUN_M4_SYNC_DIR", sync.to_str().unwrap()),
        ],
    );
    wait_file(&sync.join("ready"));
    let mut db = Connection::open(db_path(&root)).unwrap();
    configure_connection(&db).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    require_quiescent_admissions(&tx, &root).unwrap();
    // Test-only unsupported successor marker, not a production V6 migration.
    tx.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
    tx.commit().unwrap();
    fs::write(sync.join("release"), b"continue").unwrap();
    child.wait(0);
    assert!(root.join("v5-refused").exists());
    assert_eq!(count(&root, "writable_admissions"), 0);
}

// Test-ID: PR-TEST-0199
// Verifies: PR-REQ-0300
#[test]
fn exact_v4_bootstrap_excludes_itself_but_refuses_other_live_or_unknown_legacy_sessions() {
    let (_tmp, root) = root();
    legacy(&root);
    let child = worker(&root, "legacy-held", &[]);
    wait_file(&root.join("held"));
    let error = PactrunPersistence::upgrade_legacy_v4_to_v5(&root).unwrap_err();
    assert!(matches!(error, PersistenceError::LegacySessionUncertain));
    assert!(!error.to_string().contains("admitted"));
    assert_eq!(version(&root), 4);
    fs::write(root.join("release"), b"done").unwrap();
    child.wait(0);
    let prepared = StagingSession::open(&root).unwrap();
    assert!(matches!(
        PactrunPersistence::upgrade_legacy_v4_to_v5(&root),
        Err(PersistenceError::LegacySessionUncertain)
    ));
    drop(prepared);
    let path = root.join("staging/session-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    fs::create_dir(&path).unwrap();
    assert!(matches!(
        PactrunPersistence::upgrade_legacy_v4_to_v5(&root),
        Err(PersistenceError::LegacySessionUncertain)
    ));
    fs::remove_dir(path).unwrap();
    assert!(PactrunPersistence::upgrade_legacy_v4_to_v5(&root).unwrap());
    assert_eq!(count(&root, "writable_admissions"), 0);
}

// Test-ID: PR-TEST-0200
// Verifies: PR-REQ-0300
#[test]
fn legacy_lease_created_after_inspection_cannot_write_after_v5_commit() {
    let (_tmp, root) = root();
    legacy(&root);
    let migration_sync = root.join("migration-sync");
    let legacy_sync = root.join("legacy-sync");
    fs::create_dir(&migration_sync).unwrap();
    fs::create_dir(&legacy_sync).unwrap();
    let upgrade = worker(
        &root,
        "upgrade",
        &[
            ("PACTRUN_M4_SYNC", "after_legacy_session_inspection"),
            ("PACTRUN_M4_SYNC_DIR", migration_sync.to_str().unwrap()),
        ],
    );
    wait_file(&migration_sync.join("ready"));
    let late = worker(
        &root,
        "legacy-late",
        &[
            ("PACTRUN_M4_SYNC", "after_wal_before_bootstrap"),
            ("PACTRUN_M4_SYNC_DIR", legacy_sync.to_str().unwrap()),
        ],
    );
    wait_file(&legacy_sync.join("ready"));
    fs::write(legacy_sync.join("release"), b"wait for SQL writer boundary").unwrap();
    fs::write(migration_sync.join("release"), b"commit V5").unwrap();
    upgrade.wait(0);
    late.wait(0);
    assert_eq!(version(&root), 5);
    assert!(root.join("legacy-refused").exists());
    assert_eq!(count(&root, "packages"), 0);
    assert_eq!(count(&root, "writable_admissions"), 0);
}

// Test-ID: PR-TEST-0201
// Verifies: PR-REQ-0299, PR-REQ-0300
#[test]
fn bootstrap_and_admission_crashes_leave_only_committed_boundaries() {
    for (point, expected) in [
        ("after_legacy_session_inspection", 4),
        ("before_schema_migration_commit", 4),
        ("after_schema_migration_commit", 5),
    ] {
        let (_tmp, root) = root();
        legacy(&root);
        worker(&root, "upgrade", &[("PACTRUN_M4_FAULT", point)]).wait(87);
        assert_eq!(version(&root), expected);
        validate_schema(&Connection::open(db_path(&root)).unwrap(), expected).unwrap();
        if expected == 4 {
            assert!(PactrunPersistence::upgrade_legacy_v4_to_v5(&root).unwrap());
        } else {
            assert!(!PactrunPersistence::upgrade_legacy_v4_to_v5(&root).unwrap());
        }
    }
    let (_tmp, root) = root();
    worker(
        &root,
        "open",
        &[("PACTRUN_M4_FAULT", "after_writable_admission")],
    )
    .wait(87);
    assert_eq!(version(&root), SCHEMA_VERSION);
    assert_eq!(count(&root, "writable_admissions"), 1);
    let mut db = Connection::open(db_path(&root)).unwrap();
    configure_connection(&db).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    require_quiescent_admissions(&tx, &root).unwrap();
}

fn instance_fixture(p: &PactrunPersistence) -> (InstanceView, ActionRunIdentity, RuntimeFileV1) {
    use sha2::{Digest, Sha256};
    let bytes = b"public unlaunched fixture";
    let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
    let publication = p
        .put_runtime_content(&digest, &mut Cursor::new(bytes))
        .unwrap();
    let action = ActionIdentity::parse("inspect").unwrap();
    let file = RuntimeFileV1 {
        id: ContentId::parse("tool").unwrap(),
        path: RuntimePath::parse("bin/tool").unwrap(),
        kind: RuntimeFileKindV1::RegularFile,
        blob_digest: digest,
        executable: true,
    };
    let core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
        inputs: vec![
            InputDeclarationV1 {
                id: InputIdentity::parse("required").unwrap(),
                required: true,
                protection: InputProtectionV1::Normal,
            },
            InputDeclarationV1 {
                id: InputIdentity::parse("secret").unwrap(),
                required: false,
                protection: InputProtectionV1::Secret,
            },
        ],
        actions: vec![ActionV1 {
            id: action.clone(),
            access: OperationAccessV1::Observe,
            parameters: vec![],
            outputs: vec![ManagedOutputV1 {
                id: ManagedOutputIdentity::parse("report").unwrap(),
            }],
            hook: HookV1 {
                protocol_version: PositiveVersion::new(1).unwrap(),
                launch: HookLaunchV1::Direct {
                    executable: file.id.clone(),
                },
                args: vec![],
                io: IOContractV1 {
                    terminal: TerminalContractV1::None,
                },
            },
        }],
        snapshot: None,
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
        .persist_revision(PackageId::from_bytes([9; 16]), &content, &[publication])
        .unwrap();
    let mut empty = Cursor::new(b"");
    let secret = b"public-secret-fixture";
    let mut secret_reader = Cursor::new(secret);
    let mut initial = [
        crate::persistence::ManagedInputWrite {
            input_id: InputIdentity::parse("required").unwrap(),
            byte_len: 0,
            reader: &mut empty,
        },
        crate::persistence::ManagedInputWrite {
            input_id: InputIdentity::parse("secret").unwrap(),
            byte_len: secret.len() as u64,
            reader: &mut secret_reader,
        },
    ];
    let instance = p
        .create_instance(
            InstanceName::parse("fixture").unwrap(),
            revision.clone(),
            &mut initial,
        )
        .unwrap();
    (instance, ActionRunIdentity { revision, action }, file)
}
fn admit(
    p: &PactrunPersistence,
    instance: &InstanceView,
    action: &ActionRunIdentity,
    file: &RuntimeFileV1,
    tag: u8,
) -> RunId {
    let run = RunId::from_bytes([tag; 16]);
    p.create_accepted_run(
        run,
        instance.id,
        instance.state_version,
        action,
        &p.staging_session().unwrap().owner(),
        &UnconditionalAcceptance,
    )
    .unwrap();
    let launch = CompiledHookLaunch::Direct {
        executable: file.clone(),
    };
    let files = [file.clone()];
    let observation = p
        .observe_instance_compilation_state(instance.id)
        .unwrap()
        .unwrap();
    let facts = AdmissionFacts {
        service: None,
        expected_state_version: instance.state_version,
        active_bindings: &observation.active_bindings,
        runtime_content: &files,
        launch: &launch,
    };
    p.admit_run(run, &facts, &|_| Ok(()), true)
        .unwrap()
        .unwrap();
    run
}
fn failed() -> RunFinish {
    RunFinish {
        outcome: RunOutcome::Failed,
        primary_failure: None,
        secondary_failures: vec![],
        hook_completion: None,
    }
}

fn legacy_data(db: &Connection) -> BTreeMap<String, Vec<String>> {
    let model = Connection::open_in_memory().unwrap();
    for (sql, _) in &SCHEMA_LADDER[..4] {
        model.execute_batch(sql).unwrap();
    }
    let tables: Vec<String> = model
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    tables
        .into_iter()
        .map(|table| {
            let mut query = db.prepare(&format!("SELECT * FROM {table}")).unwrap();
            let n = query.column_count();
            let mut rows: Vec<String> = query
                .query_map([], |row| {
                    let values = (0..n)
                        .map(|i| row.get::<_, rusqlite::types::Value>(i))
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(format!("{values:?}"))
                })
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            rows.sort();
            (table, rows)
        })
        .collect()
}

// Test-ID: PR-TEST-0202
// Verifies: PR-REQ-0078, PR-REQ-0298, PR-REQ-0300
#[test]
fn migration_preserves_legacy_rows_orphan_pins_risk_and_guard_without_reconciliation() {
    let (_tmp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (instance, action, file) = instance_fixture(&p);
    let first = admit(&p, &instance, &action, &file, 31);
    let orphan = admit(&p, &instance, &action, &file, 32);
    p.open_recovery_risk(first).unwrap();
    p.open_recovery_risk(orphan).unwrap();
    let mut bytes = Cursor::new(b"abc");
    let mut artifacts = [RunArtifactWrite {
        output: ManagedOutputIdentity::parse("report").unwrap(),
        byte_len: 3,
        reader: &mut bytes,
    }];
    p.finish_run(first, &failed(), &mut artifacts).unwrap();
    p.abandon_execution_owner();
    let mut db = Connection::open(db_path(&root)).unwrap();
    configure_connection(&db).unwrap();
    super::super::sqlite_v7::empty_v7_to_v6_fixture(&mut db);
    // Historical fixture only: remove empty V6 and V5 additions to recreate V4.
    for table in [
        "run_migration_checkpoint_bindings",
        "run_migration_payload_pins",
        "run_migration_revision_pins",
        "run_migration_boundaries",
        "run_migration_progress",
        "run_migration_edges",
        "run_migration_invocations",
        "run_restore_admissions",
        "run_capture_results",
        "snapshot_blob_chunks",
        "snapshot_blobs",
        "snapshots",
        "run_restore_invocations",
        "run_capture_invocations",
        "run_operation_kinds",
        "instance_recovery_consequence_versions",
        "writable_admissions",
    ] {
        db.execute_batch(&format!("DROP TABLE {table};")).unwrap();
    }
    db.pragma_update(None, "user_version", 4).unwrap();
    validate_schema(&db, 4).unwrap();
    let before = legacy_data(&db);
    drop(db);
    assert!(PactrunPersistence::upgrade_legacy_v4_to_v5(&root).unwrap());
    let db = Connection::open(db_path(&root)).unwrap();
    assert_eq!(legacy_data(&db), before);
    assert_eq!(count(&root, "run_executions"), 1);
    assert_eq!(count(&root, "instance_recovery_guards"), 1);
    assert_eq!(count(&root, "run_revision_pins"), 1);
    assert_eq!(count(&root, "writable_admissions"), 0);
    assert_eq!(count(&root, "run_payload_pins"), 2);
    assert_eq!(count(&root, "run_artifacts"), 1);
    let counter: i64 = db
        .query_row(
            "SELECT consequence_version FROM instance_recovery_consequence_versions",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(counter, 0);
    drop(db);
    assert!(PactrunPersistence::upgrade_v5_to_v6_fixture(&root).unwrap());
    assert!(PactrunPersistence::upgrade_storage(&root).unwrap());
    let app = crate::application::PactrunApplication::open(&root).unwrap();
    assert_eq!(app.reconcile_lost_action_owners().unwrap(), vec![orphan]);
    assert!(app.reconcile_lost_action_owners().unwrap().is_empty());
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    assert_eq!(p.recovery_consequence_version(instance.id).unwrap(), 1);
}

// Test-ID: PR-TEST-0203
// Verifies: PR-REQ-0291, PR-REQ-0298
#[test]
fn each_new_open_risk_consequence_advances_once_even_when_guard_already_exists() {
    let (_tmp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (instance, action, file) = instance_fixture(&p);
    assert_eq!(p.recovery_consequence_version(instance.id).unwrap(), 0);
    let first = admit(&p, &instance, &action, &file, 41);
    let second = admit(&p, &instance, &action, &file, 42);
    p.open_recovery_risk(first).unwrap();
    p.open_recovery_risk(second).unwrap();
    assert_eq!(p.recovery_consequence_version(instance.id).unwrap(), 0);
    p.finish_run(first, &failed(), &mut []).unwrap();
    assert_eq!(p.recovery_consequence_version(instance.id).unwrap(), 1);
    let guard = p.load_instance_recovery_guard(instance.id).unwrap();
    let state = p.load_instance_by_id(instance.id).unwrap().unwrap();
    p.finish_run(second, &failed(), &mut []).unwrap();
    assert_eq!(p.recovery_consequence_version(instance.id).unwrap(), 2);
    assert_eq!(p.load_instance_recovery_guard(instance.id).unwrap(), guard);
    assert_eq!(
        p.load_instance_by_id(instance.id)
            .unwrap()
            .unwrap()
            .state_version,
        state.state_version
    );
    assert!(p.finish_run(second, &failed(), &mut []).is_err());
    assert_eq!(p.recovery_consequence_version(instance.id).unwrap(), 2);
    let third = admit(&p, &state, &action, &file, 43);
    p.open_recovery_risk(third).unwrap();
    p.database
        .lock()
        .unwrap()
        .execute(
            "UPDATE instance_recovery_consequence_versions SET consequence_version=?1",
            [i64::MAX],
        )
        .unwrap();
    assert!(p.finish_run(third, &failed(), &mut []).is_err());
    assert_eq!(
        p.recovery_consequence_version(instance.id).unwrap(),
        i64::MAX
    );
    assert_eq!(count(&root, "run_executions"), 1);
    assert_eq!(count(&root, "run_outcomes"), 2);
    assert_eq!(p.load_instance_recovery_guard(instance.id).unwrap(), guard);
}

// Test-ID: PR-TEST-0204
// Verifies: PR-REQ-0298
#[test]
fn operation_discriminators_and_consequence_rows_are_required_not_inferred() {
    let (_tmp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (instance, action, file) = instance_fixture(&p);
    let run = admit(&p, &instance, &action, &file, 51);
    let db = p.database.lock().unwrap();
    db.execute(
        "DELETE FROM run_operation_kinds WHERE run_id=?1",
        [run.as_bytes().as_slice()],
    )
    .unwrap();
    drop(db);
    assert!(matches!(
        p.load_run(run),
        Err(PersistenceError::CorruptRun(_))
    ));
    let db = p.database.lock().unwrap();
    db.execute(
        "INSERT INTO run_operation_kinds VALUES (?1,1)",
        [run.as_bytes().as_slice()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO run_capture_invocations VALUES (?1,?2,?3)",
        params![
            run.as_bytes().as_slice(),
            action.revision.package_id.as_bytes().as_slice(),
            action.revision.content_digest.as_bytes().as_slice()
        ],
    )
    .unwrap();
    assert!(validate_run_operation(&db, run).is_err());
    db.execute(
        "DELETE FROM run_action_invocations WHERE run_id=?1",
        [run.as_bytes().as_slice()],
    )
    .unwrap();
    assert_eq!(
        validate_run_operation(&db, run).unwrap(),
        ManagedExecutionKind::SnapshotCapture
    );
    db.execute("DELETE FROM instance_recovery_consequence_versions", [])
        .unwrap();
    drop(db);
    assert!(matches!(
        p.load_instance_by_id(instance.id),
        Err(PersistenceError::CorruptInstance(_))
    ));
}

// Test-ID: PR-TEST-0205
// Verifies: PR-REQ-0300, PR-REQ-0301, PR-REQ-0325
#[test]
fn explicit_storage_upgrade_cli_does_not_reconcile_or_accept_extra_options() {
    let (_tmp, root) = root();
    legacy(&root);
    let invoke = |args: &[&str]| {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = crate::cli::run(
            args.iter().map(|s| (*s).into()).collect(),
            Some(root.as_os_str().to_owned()),
            &mut Cursor::new([]),
            &mut out,
            &mut err,
        );
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    };
    assert_eq!(invoke(&["storage", "upgrade", "--force"]).0, 2);
    assert_eq!(version(&root), 4);
    assert_eq!(invoke(&["storage", "upgrade"]).0, 1);
    assert!(PactrunPersistence::upgrade_legacy_v4_to_v5(&root).unwrap());
    assert_eq!(version(&root), 5);
    assert_eq!(invoke(&["storage", "upgrade"]).0, 1);
    assert_eq!(version(&root), 5);
    // Historical V4/V5 implementations are fixture helpers only. The current
    // CLI must not implicitly chain them before its exact V6-to-V7 upgrade.
    assert!(PactrunPersistence::upgrade_v5_to_v6_fixture(&root).unwrap());
    assert_eq!(version(&root), 6);
    let first = invoke(&["storage", "upgrade"]);
    assert_eq!(first.0, 0, "{}", first.2);
    assert!(first.1.contains("upgraded"));
    assert_eq!(version(&root), 7);
    let second = invoke(&["storage", "upgrade"]);
    assert_eq!(second.0, 0);
    assert!(second.1.contains("already current"));
    assert_eq!(count(&root, "runs"), 0);
    assert_eq!(count(&root, "writable_admissions"), 0);
}
