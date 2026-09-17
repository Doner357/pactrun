//! Exact V7 -> V8 transition. Activation waits for the complete M7 runtime.
//!
//! SQLite table replacement must not cascade through Run children. Foreign-key
//! enforcement is disabled on this private upgrade connection before BEGIN,
//! then the complete graph is checked before commit and enforcement restored.

use super::PactrunPersistence;
use super::PersistenceError;
use super::runtime_content_store::{
    validate_existing_regular_entry, validate_supported_storage_root,
};
use super::sqlite_revision_store::{
    DatabaseState, FaultPoint, classify_database, configure_connection, configure_read_connection,
    establish_wal_mode, fault, validate_schema,
};
use crate::managed_data::StagingSession;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::path::Path;

pub(super) const SCHEMA_V8_ADDITIONS_SQL: &str =
    include_str!("persistence_schema_v8_additions.sql");

impl PactrunPersistence {
    pub(super) fn upgrade_legacy_v7_to_v8(root: &Path) -> Result<bool, PersistenceError> {
        let root = validate_supported_storage_root(root)?;
        let directory = validate_supported_storage_root(&root.join("database"))?;
        validate_existing_regular_entry(&directory, "pactrun.sqlite3")?;
        let path = directory.join("pactrun.sqlite3");
        let reader = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| PersistenceError::sqlite("inspect V8 upgrade source", e))?;
        configure_read_connection(&reader)?;
        match classify_database(&reader)? {
            DatabaseState::V8 => return Ok(false),
            DatabaseState::V7 => {}
            _ => return Err(unsupported()),
        }
        drop(reader);
        let session = StagingSession::prepare(&root).map_err(|_| {
            PersistenceError::DatabaseOwnership("could not prepare V8 upgrade session".to_owned())
        })?;
        let mut database = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| PersistenceError::sqlite("open V8 maintenance connection", e))?;
        configure_connection(&database)?;
        establish_wal_mode(&database)?;
        database
            .pragma_update(None, "foreign_keys", false)
            .map_err(|e| PersistenceError::sqlite("prepare V8 table replacement", e))?;
        let tx = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("serialize V7 to V8 upgrade", e))?;
        match classify_database(&tx)? {
            DatabaseState::V8 => {
                tx.commit()
                    .map_err(|e| PersistenceError::sqlite("finish concurrent V8 observation", e))?;
                return Ok(false);
            }
            DatabaseState::V7 => {}
            _ => return Err(unsupported()),
        }
        super::sqlite_v5::require_quiescent_admissions_at_version(&tx, &root, 7)?;
        fault(FaultPoint::AfterV7AdmissionInspection);
        tx.execute("DELETE FROM writable_admissions", [])
            .map_err(|e| {
                PersistenceError::sqlite("remove confirmed-lost V7 writer admissions", e)
            })?;
        apply_v8(&tx)?;
        validate_schema(&tx, 8)?;
        tx.execute(
            "INSERT INTO writable_admissions VALUES(?1,8)",
            [session.owner().as_str().as_bytes()],
        )
        .map_err(|e| PersistenceError::sqlite("record V8 upgrade admission", e))?;
        tx.pragma_update(None, "user_version", 8)
            .map_err(|e| PersistenceError::sqlite("publish V8 version", e))?;
        fault(FaultPoint::BeforeSchemaMigrationCommit);
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit V7 to V8 upgrade", e))?;
        fault(FaultPoint::AfterSchemaMigrationCommit);
        // The connection is private and dropped on any return. Re-enable FKs
        // before any post-upgrade work. Housekeeping never changes commit truth.
        if database.pragma_update(None, "foreign_keys", true).is_ok()
            && let Ok(tx) = database.transaction_with_behavior(TransactionBehavior::Immediate)
            && classify_database(&tx).is_ok_and(|state| state == DatabaseState::V8)
            && tx
                .execute(
                    "DELETE FROM writable_admissions WHERE owner_session=?1",
                    [session.owner().as_str().as_bytes()],
                )
                .is_ok()
        {
            let _ = tx.commit();
        }
        Ok(true)
    }
}

fn unsupported() -> PersistenceError {
    PersistenceError::DatabaseOwnership(
        "V8 upgrade accepts exact V7 only; use a compatible build to reach V7 first".to_owned(),
    )
}

/// Historical-fixture adapter only; refuses any M7 state before restoring the
/// exact older table definitions. Never exposed as a production downgrade.
#[cfg(test)]
pub(super) fn empty_v8_to_v7_fixture(database: &mut Connection) {
    super::sqlite_v9::current_to_v8_fixture(database);
    use super::sqlite_revision_store::{
        SCHEMA_V4_ADDITIONS_SQL, SCHEMA_V6_ADDITIONS_SQL, SCHEMA_V7_ADDITIONS_SQL,
    };
    validate_schema(database, 8).unwrap();
    database.pragma_update(None, "foreign_keys", false).unwrap();
    let tx = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    for table in [
        "allocation_discard_receipts",
        "detached_service_allocations",
        "instance_retirement_receipts",
        "deletion_finalization_allocations",
        "deletion_finalization_authorizations",
        "instance_deletion_obligations",
        "run_deletion_invocations",
    ] {
        assert_eq!(
            tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0,
            "fixture cannot discard M7 evidence"
        );
        tx.execute_batch(&format!("DROP TABLE {table}")).unwrap();
    }
    tx.execute_batch("CREATE TEMP TABLE saved_runs AS SELECT * FROM runs; DROP TABLE runs;")
        .unwrap();
    tx.execute_batch(
        SCHEMA_V4_ADDITIONS_SQL
            .split("CREATE TABLE run_action_invocations")
            .next()
            .unwrap(),
    )
    .unwrap();
    tx.execute_batch("INSERT INTO runs SELECT * FROM saved_runs; DROP TABLE saved_runs; DROP TABLE instance_history_identities;").unwrap();
    tx.execute_batch("CREATE TEMP TABLE saved_kinds AS SELECT * FROM run_operation_kinds; DROP TABLE run_operation_kinds;").unwrap();
    let definition = SCHEMA_V6_ADDITIONS_SQL
        .split("CREATE TABLE v6_run_operation_kinds")
        .nth(1)
        .unwrap()
        .split("CREATE TABLE v6_run_capture_invocations")
        .next()
        .unwrap();
    // Only the CREATE statement, not subsequent data-bearing migration steps.
    let definition = definition.split(";").next().unwrap();
    tx.execute_batch(&format!("CREATE TABLE v6_run_operation_kinds{definition}; ALTER TABLE v6_run_operation_kinds RENAME TO run_operation_kinds;")).unwrap();
    tx.execute_batch(
        "INSERT INTO run_operation_kinds SELECT * FROM saved_kinds; DROP TABLE saved_kinds;",
    )
    .unwrap();
    tx.execute_batch(
        "CREATE TEMP TABLE saved_admissions AS SELECT owner_session FROM writable_admissions;",
    )
    .unwrap();
    tx.execute_batch(
        SCHEMA_V7_ADDITIONS_SQL
            .split("CREATE TABLE service_storage_allocations")
            .next()
            .unwrap(),
    )
    .unwrap();
    tx.execute_batch("INSERT INTO writable_admissions SELECT owner_session,7 FROM saved_admissions; DROP TABLE saved_admissions;").unwrap();
    tx.pragma_update(None, "user_version", 7).unwrap();
    validate_schema(&tx, 7).unwrap();
    assert!(
        tx.prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none()
    );
    tx.commit().unwrap();
    database.pragma_update(None, "foreign_keys", true).unwrap();
}

/// Call only on a dedicated, admission-qualified maintenance connection.
/// The caller must hold the same transaction's quiescent-admission gate.
pub(super) fn apply_v8(transaction: &rusqlite::Transaction<'_>) -> Result<(), PersistenceError> {
    let foreign_keys: bool = transaction
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .map_err(|e| PersistenceError::sqlite("check V8 maintenance connection", e))?;
    if foreign_keys {
        return Err(PersistenceError::SchemaMismatch(
            "V8 table replacement requires a dedicated FK-disabled maintenance connection"
                .to_owned(),
        ));
    }
    transaction
        .execute_batch(SCHEMA_V8_ADDITIONS_SQL)
        .map_err(|e| PersistenceError::sqlite("apply V8 lifecycle schema", e))?;
    let mut check = transaction
        .prepare("PRAGMA foreign_key_check")
        .map_err(|e| PersistenceError::sqlite("prepare V8 reference validation", e))?;
    if check
        .query([])
        .map_err(|e| PersistenceError::sqlite("validate V8 references", e))?
        .next()
        .map_err(|e| PersistenceError::sqlite("read V8 reference validation", e))?
        .is_some()
    {
        return Err(PersistenceError::SchemaMismatch(
            "V8 transition has invalid foreign-key references".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "sqlite_v8_upgrade_tests.rs"]
mod upgrade_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::sqlite_revision_store::{SCHEMA_LADDER, validate_schema};
    use rusqlite::{Connection, TransactionBehavior, params};

    fn v7() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        for (sql, _) in &SCHEMA_LADDER[..7] {
            connection.execute_batch(sql).unwrap();
        }
        validate_schema(&connection, 7).unwrap();
        connection.pragma_update(None, "user_version", 7).unwrap();
        connection
            .execute("INSERT INTO packages VALUES(?1)", [[1u8; 16].as_slice()])
            .unwrap();
        connection
            .execute(
                "INSERT INTO revisions VALUES(?1, ?2, ?3, ?3)",
                params![[1u8; 16].as_slice(), [2u8; 32].as_slice(), b"{}".as_slice()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO instances VALUES(?1, ?2, ?3, ?4, ?5)",
                params![
                    [3u8; 16].as_slice(),
                    b"sample".as_slice(),
                    [1u8; 16].as_slice(),
                    [2u8; 32].as_slice(),
                    [4u8; 16].as_slice()
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO runs VALUES(?1, ?2, ?3, 0)",
                params![
                    [5u8; 16].as_slice(),
                    [3u8; 16].as_slice(),
                    [4u8; 16].as_slice()
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO run_operation_kinds VALUES(?1, 0)",
                [[5u8; 16].as_slice()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO run_action_invocations VALUES(?1, ?2, ?3, ?4)",
                params![
                    [5u8; 16].as_slice(),
                    [1u8; 16].as_slice(),
                    [2u8; 32].as_slice(),
                    b"action".as_slice()
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO run_outcomes VALUES(?1, 0, 1, 0, 1)",
                [[5u8; 16].as_slice()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO run_artifacts VALUES(?1, ?2, 0)",
                params![[5u8; 16].as_slice(), b"output".as_slice()],
            )
            .unwrap();
        connection
    }

    fn count(connection: &Connection, table: &str) -> i64 {
        connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    // Test-ID: PR-TEST-0392
    // Verifies: PR-REQ-0338
    #[test]
    fn v8_table_replacement_preserves_run_children_and_artifacts_after_retirement() {
        let mut connection = v7();
        // Maintenance uses a dedicated connection; FK enforcement cannot be
        // toggled inside a transaction. Checking the whole graph precedes commit.
        connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        apply_v8(&transaction).unwrap();
        transaction.pragma_update(None, "user_version", 8).unwrap();
        transaction.commit().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        for table in [
            "runs",
            "run_action_invocations",
            "run_outcomes",
            "run_artifacts",
            "instance_history_identities",
        ] {
            assert_eq!(count(&connection, table), 1, "{table}");
        }
        connection.execute("DELETE FROM instances", []).unwrap();
        assert_eq!(count(&connection, "runs"), 1);
        assert_eq!(count(&connection, "run_artifacts"), 1);
        assert!(
            connection
                .prepare("PRAGMA foreign_key_check")
                .unwrap()
                .query([])
                .unwrap()
                .next()
                .unwrap()
                .is_none()
        );
        // New Instances can reuse a human name, not the old immutable identity.
        connection
            .execute(
                "INSERT INTO instances VALUES(?1, ?2, ?3, ?4, ?5)",
                params![
                    [6u8; 16].as_slice(),
                    b"sample".as_slice(),
                    [1u8; 16].as_slice(),
                    [2u8; 32].as_slice(),
                    [7u8; 16].as_slice()
                ],
            )
            .unwrap();
        assert_eq!(count(&connection, "instance_history_identities"), 1);
        assert!(
            connection
                .execute("DELETE FROM instance_history_identities", [])
                .is_err()
        );
    }

    // Test-ID: PR-TEST-0393
    // Verifies: PR-REQ-0339
    #[test]
    fn v8_rollback_leaves_exact_v7_and_no_inferred_lifecycle_records() {
        let mut connection = v7();
        connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        {
            let transaction = connection.transaction().unwrap();
            apply_v8(&transaction).unwrap();
            for table in [
                "instance_deletion_obligations",
                "deletion_finalization_authorizations",
                "detached_service_allocations",
                "allocation_discard_receipts",
            ] {
                assert_eq!(count(&transaction, table), 0);
            }
            // Simulate loss before the commit; no marker or evidence survives.
        }
        validate_schema(&connection, 7).unwrap();
        assert_eq!(
            connection
                .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            7
        );
        assert_eq!(count(&connection, "runs"), 1);
        assert_eq!(count(&connection, "run_artifacts"), 1);
    }

    // Test-ID: PR-TEST-0394
    // Verifies: PR-REQ-0338
    #[test]
    fn v8_rejects_corrupt_legacy_references_instead_of_blessing_them() {
        let mut connection = v7();
        connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        connection.execute("DELETE FROM instances", []).unwrap();
        let transaction = connection.transaction().unwrap();
        assert!(apply_v8(&transaction).is_err());
    }

    // Test-ID: PR-TEST-0395
    // Verifies: PR-REQ-0338
    #[test]
    fn v8_ddl_matches_the_approved_contract_and_current_bootstrap() {
        let document = include_str!("../../docs/spec/persistence/persistence-schema-v8.md")
            .replace("\r\n", "\n");
        let ddl = document
            .split("```sql\n")
            .nth(1)
            .unwrap()
            .split("```")
            .next()
            .unwrap();
        assert_eq!(ddl.trim(), SCHEMA_V8_ADDITIONS_SQL.trim());
        let mut database = v7();
        database.pragma_update(None, "foreign_keys", false).unwrap();
        let tx = database.transaction().unwrap();
        apply_v8(&tx).unwrap();
        validate_schema(&tx, 8).unwrap();
        tx.commit().unwrap();
        super::empty_v8_to_v7_fixture(&mut database);
        assert_eq!(count(&database, "runs"), 1);
        assert_eq!(count(&database, "run_artifacts"), 1);
    }
}
