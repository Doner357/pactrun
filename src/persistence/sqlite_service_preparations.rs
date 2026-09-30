//! Conservative maintenance of unexposed service allocation intents.
//!
//! A recorded intent does not prove that an existing directory was created by
//! that attempt: creation can fail on a pre-existing empty entry. After owner
//! loss, V7 has no durable filesystem-object identity receipt. Existing objects
//! therefore remain preserved, including empty ones. This maintenance only
//! retires definitively absent, unreferenced preparations; it never traverses or
//! removes service directories and never reconciles an execution outcome.
use super::{PactrunPersistence, PersistenceError};
use crate::{
    domain::{ExecutionOwnerSession, ServiceAllocationId},
    managed_data::{SessionOwnerProbe, probe_session_owner},
    service_storage as fs,
};
use rusqlite::TransactionBehavior;

impl PactrunPersistence {
    pub(super) fn reconcile_absent_service_preparations(&self) -> Result<usize, PersistenceError> {
        let root_path = self.database_path.parent().and_then(|p| p.parent()).ok_or(
            PersistenceError::ServiceStorageUnavailable("storage root is unavailable"),
        )?;
        let root = fs::open_root(root_path).map_err(|_| {
            PersistenceError::ServiceStorageUnavailable("storage root qualification failed")
        })?;
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        // This is the internal allocation maintenance exclusion. Every creating,
        // promoting or publishing writer also needs SQLite's immediate writer
        // transaction. No service execution occurs while holding this gate.
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin service preparation maintenance", e))?;
        self.check_write_admission(&tx)?;
        let corrupt: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_storage_preparations p \
             LEFT JOIN service_storage_allocations a ON a.allocation_id=p.allocation_id \
             WHERE a.allocation_id IS NULL OR p.package_id!=a.package_id \
             OR p.revision_content_digest!=a.origin_revision_digest \
             OR EXISTS(SELECT 1 FROM service_storage_protections x WHERE x.allocation_id=p.allocation_id))",
            [], |r| r.get(0),
        ).map_err(|e| PersistenceError::sqlite("validate preparation custody", e))?;
        if corrupt {
            return Err(PersistenceError::CorruptServiceStorage(
                "invalid preparation custody or provenance",
            ));
        }
        let candidates = {
            let mut statement = tx.prepare(
                "SELECT allocation_id, owner_session FROM service_storage_preparations ORDER BY allocation_id",
            ).map_err(|e| PersistenceError::sqlite("list service preparations", e))?;
            statement
                .query_map([], |r| {
                    Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
                })
                .map_err(|e| PersistenceError::sqlite("query service preparations", e))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| PersistenceError::sqlite("decode service preparations", e))?
        };
        let mut removed = 0;
        for (allocation, owner) in candidates {
            let id = ServiceAllocationId::from_bytes(allocation.as_slice().try_into().map_err(
                |_| {
                    PersistenceError::CorruptServiceStorage(
                        "invalid preparation allocation identity",
                    )
                },
            )?);
            let owner = String::from_utf8(owner)
                .ok()
                .and_then(|s| ExecutionOwnerSession::parse(s).ok())
                .ok_or(PersistenceError::CorruptServiceStorage(
                    "invalid service preparation owner",
                ))?;
            if probe_session_owner(root_path, &owner) != SessionOwnerProbe::ConfirmedLoss {
                continue;
            }
            let referenced: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM service_storage_protections WHERE allocation_id=?1) \
                 OR EXISTS(SELECT 1 FROM instance_service_storages WHERE allocation_id=?1) \
                 OR EXISTS(SELECT 1 FROM instance_service_resources WHERE allocation_id=?1) \
                 OR EXISTS(SELECT 1 FROM run_service_storage_pins WHERE allocation_id=?1) \
                 OR EXISTS(SELECT 1 FROM run_service_storage_targets WHERE allocation_id=?1) \
                 OR EXISTS(SELECT 1 FROM run_service_resource_targets WHERE allocation_id=?1)",
                [allocation.as_slice()], |r| r.get(0),
            ).map_err(|e| PersistenceError::sqlite("qualify preparation references", e))?;
            if referenced || super::sqlite_deletions::finalization_references_allocation(&tx, id)? {
                continue;
            }
            let absent = match fs::open_directory(&root, "service-storage") {
                Ok(base) => fs::open_directory(&base, &format!("alloc-{id}"))
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
                Err(e) => e.kind() == std::io::ErrorKind::NotFound,
            };
            if !absent {
                continue;
            }
            // The preparation/origin rows cascade; published references remain
            // RESTRICT barriers. No directory removal accompanies this write.
            removed += tx
                .execute(
                    "DELETE FROM service_storage_allocations WHERE allocation_id=?1",
                    [allocation.as_slice()],
                )
                .map_err(|e| PersistenceError::sqlite("retire absent service preparation", e))?;
        }
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit service preparation maintenance", e))?;
        Ok(removed)
    }
}
