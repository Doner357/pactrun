//! Explicit exact-V5 upgrade. No implicit chain, replay, or owner reconciliation.
use super::runtime_content_store::{
    validate_existing_regular_entry, validate_supported_storage_root,
};
use super::sqlite_revision_store::{
    DatabaseState, FaultPoint, SCHEMA_V5_VERSION, SCHEMA_V6_ADDITIONS_SQL, SCHEMA_VERSION,
    classify_database, configure_connection, configure_read_connection, establish_wal_mode, fault,
    validate_schema,
};
use super::{PactrunPersistence, PersistenceError};
use crate::managed_data::StagingSession;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::path::Path;

fn unsupported() -> PersistenceError {
    PersistenceError::DatabaseOwnership(
        "V6 upgrade accepts exact V5 only; use a compatible build to reach V5 first".to_owned(),
    )
}

impl PactrunPersistence {
    pub(crate) fn upgrade_storage(root: &Path) -> Result<bool, PersistenceError> {
        let root = validate_supported_storage_root(root)?;
        let database_root = validate_supported_storage_root(&root.join("database"))?;
        validate_existing_regular_entry(&database_root, "pactrun.sqlite3")?;
        let path = database_root.join("pactrun.sqlite3");
        let reader = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| PersistenceError::sqlite("inspect V6 upgrade source", e))?;
        configure_read_connection(&reader)?;
        match classify_database(&reader)? {
            DatabaseState::V6 => return Ok(false),
            DatabaseState::V5 => {}
            _ => return Err(unsupported()),
        }
        drop(reader);
        // Preparation is not writer admission. The maintenance lease must not
        // make an otherwise quiescent admission-aware database block itself.
        let _session = StagingSession::prepare(&root).map_err(|_| {
            PersistenceError::DatabaseOwnership("could not prepare V6 upgrade session".to_owned())
        })?;
        let mut database = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| PersistenceError::sqlite("open V6 upgrade", e))?;
        configure_connection(&database)?;
        establish_wal_mode(&database)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("serialize V5 to V6 upgrade", e))?;
        match classify_database(&transaction)? {
            DatabaseState::V6 => {
                transaction.commit().map_err(|e| {
                    PersistenceError::sqlite("finish concurrent upgrade observation", e)
                })?;
                return Ok(false);
            }
            DatabaseState::V5 => {}
            _ => return Err(unsupported()),
        }
        super::sqlite_v5::require_quiescent_admissions_at_version(
            &transaction,
            &root,
            SCHEMA_V5_VERSION,
        )?;
        fault(FaultPoint::AfterV5AdmissionInspection);
        transaction
            .execute_batch(SCHEMA_V6_ADDITIONS_SQL)
            .map_err(|e| PersistenceError::sqlite("apply exact V6 schema", e))?;
        validate_schema(&transaction, SCHEMA_VERSION)?;
        let bad_fk = transaction
            .prepare("PRAGMA foreign_key_check")
            .map_err(|e| PersistenceError::sqlite("prepare V6 foreign-key validation", e))?
            .query([])
            .map_err(|e| PersistenceError::sqlite("validate V6 foreign keys", e))?
            .next()
            .map_err(|e| PersistenceError::sqlite("read V6 foreign-key validation", e))?
            .is_some();
        if bad_fk {
            return Err(PersistenceError::SchemaMismatch(
                "V6 upgrade found invalid foreign-key references".to_owned(),
            ));
        }
        transaction
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(|e| PersistenceError::sqlite("publish V6 version", e))?;
        fault(FaultPoint::BeforeSchemaMigrationCommit);
        transaction
            .commit()
            .map_err(|e| PersistenceError::sqlite("commit V5 to V6 upgrade", e))?;
        fault(FaultPoint::AfterSchemaMigrationCommit);
        Ok(true)
    }
}

#[cfg(test)]
#[path = "sqlite_v6_tests.rs"]
mod tests;
