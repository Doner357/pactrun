//! Read-only service contract inspection. A single SQLite snapshot selects the
//! active/retained roles; no filesystem observation or writer admission occurs.
use super::sqlite_instances::load_instance_view_from;
use super::sqlite_revision_store::load_revision_from;
use super::{PactrunPersistence, PersistenceError};
use crate::domain::*;
use rusqlite::{Connection, TransactionBehavior};

fn corrupt(message: &'static str) -> PersistenceError {
    PersistenceError::CorruptServiceStorage(message)
}
fn bytes<const N: usize>(value: Vec<u8>) -> Result<[u8; N], PersistenceError> {
    value
        .try_into()
        .map_err(|_| corrupt("invalid service identity width"))
}
fn text(value: Vec<u8>) -> Result<String, PersistenceError> {
    String::from_utf8(value).map_err(|_| corrupt("invalid service identity UTF-8"))
}

impl PactrunPersistence {
    pub(crate) fn load_instance_service_state(
        &self,
        instance: InstanceId,
    ) -> Result<Option<InstanceServiceState>, PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("begin service inspection", e))?;
        let result = load_instance_service_state_from(&tx, instance)?;
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("finish service inspection", e))?;
        Ok(result)
    }
}

pub(super) fn load_instance_service_state_from(
    tx: &Connection,
    instance: InstanceId,
) -> Result<Option<InstanceServiceState>, PersistenceError> {
    let Some(view) = load_instance_view_from(tx, instance)? else {
        return Ok(None);
    };
    // load_instance_view_from has already enforced both current and retained
    // typed contract, owner, protection and active allocation invariants.
    let mut result = InstanceServiceState {
        instance,
        state_version: view.state_version,
        current_revision: view.active_revision.clone(),
        storages: vec![],
        resources: vec![],
        preserved: vec![],
    };
    for (table, identity, storage) in [
        ("instance_service_storages", "storage_identity", true),
        ("instance_service_resources", "resource_identity", false),
    ] {
        // All identifiers are closed constants, not author-controlled SQL.
        let mut statement = tx
            .prepare(&format!(
                "SELECT {identity}, allocation_id, declaration_revision_digest FROM {table} \
                 WHERE instance_id=?1 ORDER BY {identity}",
            ))
            .map_err(|e| PersistenceError::sqlite("prepare service contract view", e))?;
        let rows = statement
            .query_map([instance.as_bytes().as_slice()], |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, Vec<u8>>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(|e| PersistenceError::sqlite("query service contract view", e))?;
        for row in rows {
            let (id, allocation, digest) =
                row.map_err(|e| PersistenceError::sqlite("read service contract view", e))?;
            let id = text(id)?;
            let allocation = ServiceAllocationId::from_bytes(bytes(allocation)?);
            let declaration_revision = RevisionIdentity::new(
                view.active_revision.package_id,
                RevisionContentDigest::from_bytes(bytes(digest)?),
            );
            let stored = load_revision_from(tx, &declaration_revision)?
                .ok_or(corrupt("missing service contract"))?;
            let core = stored
                .content
                .core
                .service_core()
                .ok_or(corrupt("service contract is not V2"))?;
            let role = if declaration_revision == view.active_revision {
                ServiceRole::Active
            } else {
                ServiceRole::Retained
            };
            if storage {
                let declaration = core
                    .storages()
                    .iter()
                    .find(|s| s.id.as_str() == id)
                    .ok_or(corrupt("storage is not declared"))?
                    .clone();
                result.storages.push(ServiceStorageAssociation {
                    declaration,
                    declaration_revision,
                    allocation,
                    role,
                });
            } else {
                let declaration = core
                    .resources()
                    .iter()
                    .find(|r| r.id.as_str() == id)
                    .ok_or(corrupt("resource is not declared"))?
                    .clone();
                result.resources.push(ServiceResourceAssociation {
                    declaration,
                    declaration_revision,
                    allocation,
                    role,
                });
            }
        }
    }
    result.preserved = preserved_allocations(tx, instance, view.active_revision.package_id)?;
    Ok(Some(result))
}

fn preserved_allocations(
    db: &Connection,
    instance: InstanceId,
    package: PackageId,
) -> Result<Vec<PreservedServiceAllocation>, PersistenceError> {
    let mut statement = db.prepare(
        "SELECT a.allocation_id, a.package_id, a.origin_revision_digest, a.origin_storage_identity, \
         o.origin_run_id, p.allocation_id, t.allocation_id, \
         EXISTS(SELECT 1 FROM instance_service_storages s WHERE s.allocation_id=a.allocation_id) \
         OR EXISTS(SELECT 1 FROM instance_service_resources r WHERE r.allocation_id=a.allocation_id) \
         FROM service_storage_allocations a \
         LEFT JOIN service_storage_protections p ON p.allocation_id=a.allocation_id \
         LEFT JOIN service_storage_preparations t ON t.allocation_id=a.allocation_id \
         LEFT JOIN service_storage_run_origins o ON o.allocation_id=a.allocation_id \
         WHERE a.instance_id=?1 ORDER BY a.allocation_id",
    ).map_err(|e| PersistenceError::sqlite("prepare allocation custody view", e))?;
    let mut rows = statement
        .query([instance.as_bytes().as_slice()])
        .map_err(|e| PersistenceError::sqlite("query allocation custody view", e))?;
    let mut result = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| PersistenceError::sqlite("read allocation custody view", e))?
    {
        let read = || -> rusqlite::Result<_> {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, Option<Vec<u8>>>(4)?,
                row.get::<_, Option<Vec<u8>>>(5)?.is_some(),
                row.get::<_, Option<Vec<u8>>>(6)?.is_some(),
                row.get::<_, bool>(7)?,
            ))
        };
        let (allocation, owner_package, digest, storage, run, protected, preparing, associated) =
            read().map_err(|e| PersistenceError::sqlite("decode allocation custody", e))?;
        if owner_package != package.as_bytes() || protected == preparing {
            return Err(corrupt(
                "invalid allocation owner or preparation/protection state",
            ));
        }
        let allocation = ServiceAllocationId::from_bytes(bytes(allocation)?);
        let origin_revision =
            RevisionIdentity::new(package, RevisionContentDigest::from_bytes(bytes(digest)?));
        let origin_storage = ServiceStorageIdentity::parse(text(storage)?)
            .map_err(|_| corrupt("invalid origin storage identity"))?;
        let origin_run = run.map(bytes).transpose()?.map(RunId::from_bytes);
        if protected && !associated {
            result.push(PreservedServiceAllocation {
                allocation,
                origin_revision,
                origin_storage,
                origin_run,
            });
        }
    }
    Ok(result)
}
