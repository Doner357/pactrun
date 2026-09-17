//! Exact coordination activation; no object data or identity is rewritten.
use super::runtime_content_store::{
    validate_existing_regular_entry, validate_supported_storage_root,
};
use super::sqlite_revision_store::{
    DatabaseState, FaultPoint, SCHEMA_VERSION, classify_database, configure_connection,
    configure_read_connection, establish_wal_mode, fault, validate_schema,
};
use super::{PactrunPersistence, PersistenceError, RuntimeContentStore};
use crate::managed_data::StagingSession;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::path::Path;

impl PactrunPersistence {
    pub(crate) fn upgrade_storage(root: &Path) -> Result<bool, PersistenceError> {
        let root = validate_supported_storage_root(root)?;
        let directory = validate_supported_storage_root(&root.join("database"))?;
        validate_existing_regular_entry(&directory, "pactrun.sqlite3")?;
        let path = directory.join("pactrun.sqlite3");
        let reader = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| PersistenceError::sqlite("inspect coordination upgrade", e))?;
        configure_read_connection(&reader)?;
        match classify_database(&reader)? {
            DatabaseState::V9 => return Ok(false),
            DatabaseState::V8 => {}
            _ => return Err(unsupported()),
        }
        drop(reader);
        // The OS guard precedes the serialized upgrade and every content operation.
        let _content = RuntimeContentStore::open(root.join("runtime-content"))?
            .with_collection_coordination(true, true)?;
        let session = StagingSession::prepare(&root).map_err(|_| {
            PersistenceError::DatabaseOwnership("could not prepare coordination upgrade".to_owned())
        })?;
        let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| PersistenceError::sqlite("open coordination upgrade", e))?;
        configure_connection(&db)?;
        establish_wal_mode(&db)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("serialize coordination upgrade", e))?;
        match classify_database(&tx)? {
            DatabaseState::V9 => return Ok(false),
            DatabaseState::V8 => {}
            _ => return Err(unsupported()),
        }
        super::sqlite_v5::require_quiescent_admissions_at_version(&tx, &root, 8)?;
        fault(FaultPoint::AfterCoordinationAdmissionInspection);
        tx.execute_batch(include_str!("persistence_schema_v9_additions.sql"))
            .map_err(|e| PersistenceError::sqlite("replace coordination admission", e))?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(|e| PersistenceError::sqlite("publish coordination version", e))?;
        validate_schema(&tx, SCHEMA_VERSION)?;
        fault(FaultPoint::BeforeSchemaMigrationCommit);
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit coordination upgrade", e))?;
        fault(FaultPoint::AfterSchemaMigrationCommit);
        drop(session);
        Ok(true)
    }
}

fn unsupported() -> PersistenceError {
    PersistenceError::DatabaseOwnership(
        "V9 upgrade accepts exact V8 only; no implicit upgrade chain".to_owned(),
    )
}

#[cfg(test)]
pub(super) fn current_to_v8_fixture(db: &mut Connection) {
    if db
        .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
        .unwrap()
        != 9
    {
        return;
    }
    validate_schema(db, 9).unwrap();
    db.execute_batch(
        include_str!("persistence_schema_v8_additions.sql")
            .split("CREATE TABLE instance_history_identities")
            .next()
            .unwrap(),
    )
    .unwrap();
    db.pragma_update(None, "user_version", 8).unwrap();
    validate_schema(db, 8).unwrap();
}
