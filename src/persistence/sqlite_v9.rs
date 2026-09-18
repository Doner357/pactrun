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
            DatabaseState::V10 => return Ok(false),
            DatabaseState::V8 | DatabaseState::V9 => {}
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
            DatabaseState::V10 => return Ok(false),
            DatabaseState::V8 | DatabaseState::V9 => {}
            _ => return Err(unsupported()),
        }
        let source: i64 = tx
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(|e| PersistenceError::sqlite("read upgrade source", e))?;
        super::sqlite_v5::require_quiescent_admissions_at_version(&tx, &root, source)?;
        fault(FaultPoint::AfterCoordinationAdmissionInspection);
        if source == 8 {
            tx.execute_batch(include_str!("persistence_schema_v9_additions.sql"))
                .map_err(|e| PersistenceError::sqlite("activate coordination", e))?;
        }
        tx.execute_batch(include_str!("persistence_schema_v10_additions.sql"))
            .map_err(|e| PersistenceError::sqlite("activate immutable data references", e))?;
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
        "V10 upgrade accepts exact V8 or V9 only; no implicit older upgrade chain".to_owned(),
    )
}

#[cfg(test)]
pub(super) fn current_to_v8_fixture(db: &mut Connection) {
    current_to_v9_fixture(db);
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

#[cfg(test)]
pub(crate) fn current_to_v9_fixture(db: &mut Connection) {
    if db
        .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
        .unwrap()
        != 10
    {
        return;
    }
    // Construct real old-format bytes, not a marker-only fake downgrade. This
    // helper is test-only and never part of production upgrade/recovery.
    let root = Path::new(db.path().unwrap())
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let store = RuntimeContentStore::open_read_only(root.join("runtime-content")).unwrap();
    let tx = db.transaction().unwrap();
    for (sql, table) in [
        (
            "SELECT snapshot_id,blob_digest,blob_digest,byte_length FROM snapshot_blobs WHERE storage_kind=1",
            &super::sqlite_snapshots::CHUNKS,
        ),
        (
            "SELECT instance_id,payload_id,content_digest,byte_length FROM managed_input_payloads WHERE content_digest IS NOT NULL",
            &super::sqlite_instances::PAYLOAD_CHUNKS,
        ),
    ] {
        let mut q = tx.prepare(sql).unwrap();
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, Vec<u8>>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .unwrap();
        for row in rows {
            let (key0, key1, digest, length) = row.unwrap();
            let digest = crate::domain::Sha256Digest::from_bytes(digest.try_into().unwrap());
            let mut reader = store.open_verified(&digest).unwrap();
            super::chunked_blob::insert_chunks(
                &tx,
                table,
                [&key0, &key1],
                &mut reader,
                length as u64,
            )
            .unwrap();
        }
    }
    tx.execute_batch("UPDATE snapshot_blobs SET storage_kind=0; UPDATE managed_input_payloads SET content_digest=NULL;").unwrap();
    tx.commit().unwrap();
    db.execute_batch("ALTER TABLE snapshot_blobs DROP COLUMN storage_kind; ALTER TABLE managed_input_payloads DROP COLUMN content_digest;").unwrap();
    db.execute_batch(include_str!("persistence_schema_v9_additions.sql"))
        .unwrap();
    db.pragma_update(None, "user_version", 9).unwrap();
    validate_schema(db, 9).unwrap();
}
