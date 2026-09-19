//! Explicit exact-V6 upgrade; no implicit chain, service allocation or replay.
use super::runtime_content_store::{
    validate_existing_regular_entry, validate_supported_storage_root,
};
use super::sqlite_revision_store::{
    DatabaseState, FaultPoint, SCHEMA_V6_VERSION, SCHEMA_V7_ADDITIONS_SQL,
    SCHEMA_V7_VERSION as SCHEMA_VERSION, classify_database, configure_connection,
    configure_read_connection, establish_wal_mode, fault, validate_schema,
};
use super::{PactrunPersistence, PersistenceError};
use crate::managed_data::StagingSession;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::path::Path;

fn unsupported() -> PersistenceError {
    PersistenceError::DatabaseOwnership(
        "V7 upgrade accepts exact V6 only; use a compatible build to reach V6 first".to_owned(),
    )
}
impl PactrunPersistence {
    pub(crate) fn upgrade_storage_v7(root: &Path) -> Result<bool, PersistenceError> {
        let root = validate_supported_storage_root(root)?;
        let database_root = validate_supported_storage_root(&root.join("database"))?;
        validate_existing_regular_entry(&database_root, "pactrun.sqlite3")?;
        let path = database_root.join("pactrun.sqlite3");
        let reader = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| PersistenceError::sqlite("inspect V7 upgrade source", e))?;
        configure_read_connection(&reader)?;
        match classify_database(&reader)? {
            DatabaseState::V7 => return Ok(false),
            DatabaseState::V6 => {}
            _ => return Err(unsupported()),
        }
        drop(reader);
        let session = StagingSession::prepare(&root).map_err(|_| {
            PersistenceError::DatabaseOwnership("could not prepare V7 upgrade session".to_owned())
        })?;
        let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| PersistenceError::sqlite("open V7 upgrade", e))?;
        configure_connection(&db)?;
        establish_wal_mode(&db)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("serialize V6 to V7 upgrade", e))?;
        match classify_database(&tx)? {
            DatabaseState::V7 => {
                tx.commit()
                    .map_err(|e| PersistenceError::sqlite("finish concurrent V7 observation", e))?;
                return Ok(false);
            }
            DatabaseState::V6 => {}
            _ => return Err(unsupported()),
        }
        super::sqlite_v5::require_quiescent_admissions_at_version(&tx, &root, SCHEMA_V6_VERSION)?;
        fault(FaultPoint::AfterV6AdmissionInspection);
        // All these rows have been proved lost under this same serialized gate.
        // Removing writer admission never terminates its associated Runs.
        tx.execute("DELETE FROM writable_admissions", [])
            .map_err(|e| PersistenceError::sqlite("remove lost V6 admissions", e))?;
        tx.execute_batch(SCHEMA_V7_ADDITIONS_SQL)
            .map_err(|e| PersistenceError::sqlite("apply exact V7 schema", e))?;
        validate_schema(&tx, SCHEMA_VERSION)?;
        let bad = tx
            .prepare("PRAGMA foreign_key_check")
            .map_err(|e| PersistenceError::sqlite("prepare V7 FK validation", e))?
            .query([])
            .map_err(|e| PersistenceError::sqlite("validate V7 FKs", e))?
            .next()
            .map_err(|e| PersistenceError::sqlite("read V7 FK validation", e))?
            .is_some();
        if bad {
            return Err(PersistenceError::SchemaMismatch(
                "V7 upgrade found invalid foreign-key references".to_owned(),
            ));
        }
        tx.execute(
            "INSERT INTO writable_admissions VALUES(?1,7)",
            [session.owner().as_str().as_bytes()],
        )
        .map_err(|e| PersistenceError::sqlite("publish V7 upgrader admission", e))?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(|e| PersistenceError::sqlite("publish V7 version", e))?;
        fault(FaultPoint::BeforeSchemaMigrationCommit);
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit V6 to V7 upgrade", e))?;
        fault(FaultPoint::AfterSchemaMigrationCommit);
        // Committed upgrade success cannot become rollback because admission
        // cleanup fails. A leftover row has the same safe lost-owner meaning as
        // a post-commit process crash and is re-probed at the next maintenance gate.
        if let Ok(tx) = db.transaction_with_behavior(TransactionBehavior::Immediate)
            && classify_database(&tx).is_ok_and(|s| s == DatabaseState::V7)
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

#[cfg(test)]
#[path = "sqlite_v7_tests.rs"]
mod tests;

/// Historical test construction only. No production downgrade entry exists.
/// The explicit empty-table checks prevent throwing away service state when
/// newer fixture builders seed older format preservation tests.
#[cfg(test)]
pub(super) fn empty_v7_to_v6_fixture(db: &mut Connection) {
    use super::sqlite_revision_store::SCHEMA_V6_ADDITIONS_SQL;
    if matches!(
        classify_database(db).unwrap(),
        DatabaseState::V8 | DatabaseState::V9 | DatabaseState::V10 | DatabaseState::V11
    ) {
        super::sqlite_v8::empty_v8_to_v7_fixture(db);
    }
    validate_schema(db, 7).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    for table in [
        "run_service_edge_commits",
        "run_service_resource_targets",
        "run_service_storage_targets",
        "run_service_storage_pins",
        "instance_service_resources",
        "instance_service_storages",
        "service_storage_run_origins",
        "service_storage_preparations",
        "service_storage_protections",
        "service_storage_allocations",
    ] {
        assert_eq!(
            tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0,
            "historical fixture has live service data"
        );
        tx.execute_batch(&format!("DROP TABLE {table}")).unwrap();
    }
    tx.execute_batch("CREATE TEMP TABLE saved_admissions AS SELECT owner_session FROM writable_admissions; DROP TABLE writable_admissions;").unwrap();
    tx.execute_batch(
        SCHEMA_V6_ADDITIONS_SQL
            .split("CREATE TABLE v6_run_operation_kinds")
            .next()
            .unwrap()
            .split_once("CREATE TABLE writable_admissions")
            .map(|(_, s)| format!("CREATE TABLE writable_admissions{s}"))
            .as_deref()
            .unwrap(),
    )
    .unwrap();
    tx.execute_batch("INSERT INTO writable_admissions SELECT owner_session,6 FROM saved_admissions; DROP TABLE saved_admissions;").unwrap();
    tx.pragma_update(None, "user_version", 6).unwrap();
    validate_schema(&tx, 6).unwrap();
    tx.commit().unwrap();
}

#[cfg(test)]
pub(super) fn legacy_v6_read_fixture(root: &Path) -> PactrunPersistence {
    let path = root.join("database/pactrun.sqlite3");
    let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    configure_read_connection(&db).unwrap();
    validate_schema(&db, 6).unwrap();
    PactrunPersistence {
        database: std::sync::Mutex::new(db),
        database_path: path,
        runtime_content: super::runtime_content_store::RuntimeContentStore::open_read_only(
            root.join("runtime-content"),
        )
        .unwrap(),
        session: None,
    }
}
