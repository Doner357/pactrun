//! Service associations join the existing per-edge transaction. This module
//! never moves, copies or deletes service-owned contents.
use super::{PactrunPersistence, PersistenceError};
use crate::{domain::*, service_storage as fs};
use rusqlite::{Connection, Transaction, params};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
};

pub(super) fn state_from(
    db: &Connection,
    instance: InstanceId,
) -> Result<Option<ServiceMigrationState>, PersistenceError> {
    let state = super::sqlite_service_views::load_instance_service_state_from(db, instance)?
        .ok_or_else(|| PersistenceError::MissingInstance(instance.to_string()))?;
    ServiceMigrationState::from_observed(&state)
        .map(Some)
        .map_err(|_| {
            PersistenceError::CorruptServiceStorage("invalid Migration service observation")
        })
}
pub(super) fn needed(edge: &ServiceMigrationEdge) -> bool {
    !edge.before.storages.is_empty()
        || !edge.before.resources.is_empty()
        || !edge.after.storages.is_empty()
        || !edge.after.resources.is_empty()
        || !edge.grants.is_empty()
        || !edge.requires.is_empty()
}
pub(super) fn existing_dependencies(edge: &ServiceMigrationEdge) -> BTreeSet<ServiceAllocationId> {
    edge.after
        .storages
        .values()
        .filter(|s| s.declaration_revision == edge.after.revision)
        .map(|s| &s.allocation)
        .chain(edge.grants.iter().map(|(_, o)| o.allocation()))
        .chain(edge.requires.iter().map(|(_, o)| o.allocation()))
        .filter_map(|origin| match origin {
            ServiceAllocationOrigin::Existing(id) => Some(*id),
            ServiceAllocationOrigin::Created { .. } => None,
        })
        .collect()
}

impl PactrunPersistence {
    pub(super) fn qualify_service_roots(
        &self,
        db: &Connection,
        instance: InstanceId,
        allocations: &BTreeSet<ServiceAllocationId>,
    ) -> Result<BTreeMap<ServiceAllocationId, File>, PersistenceError> {
        if allocations.is_empty() {
            return Ok(BTreeMap::new());
        }
        let root_path = self.database_path.parent().and_then(|p| p.parent()).ok_or(
            PersistenceError::ServiceStorageUnavailable("storage root unavailable"),
        )?;
        let root = fs::open_root(root_path)
            .and_then(|root| fs::open_directory(&root, "service-storage"))
            .map_err(|_| {
                PersistenceError::ServiceStorageUnavailable("service container unavailable")
            })?;
        let mut opened = BTreeMap::new();
        for allocation in allocations {
            let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM service_storage_allocations a JOIN service_storage_protections p ON p.allocation_id=a.allocation_id JOIN instances i ON i.instance_id=a.instance_id WHERE a.allocation_id=?1 AND a.instance_id=?2 AND a.package_id=i.active_package_id AND NOT EXISTS(SELECT 1 FROM service_storage_preparations t WHERE t.allocation_id=a.allocation_id))",
                params![allocation.as_bytes().as_slice(), instance.as_bytes().as_slice()], |r| r.get(0))
                .map_err(|e| PersistenceError::sqlite("qualify Migration service allocation", e))?;
            if !valid {
                return Err(PersistenceError::CorruptServiceStorage(
                    "Migration allocation has invalid custody",
                ));
            }
            let file = fs::open_directory(&root, &format!("alloc-{allocation}")).map_err(|_| {
                PersistenceError::ServiceStorageUnavailable("Migration allocation unavailable")
            })?;
            opened.insert(*allocation, file);
        }
        Ok(opened)
    }

    pub(super) fn publish_existing_service_edge(
        &self,
        tx: &Transaction<'_>,
        instance: InstanceId,
        edge: &ServiceMigrationEdge,
        has_hook: bool,
    ) -> Result<Vec<File>, PersistenceError> {
        let roots = self.qualify_service_roots(tx, instance, &existing_dependencies(edge))?;
        for (predicate, resource) in &edge.create_presence {
            if *predicate == ServiceCreatePresence::Any {
                continue;
            }
            if has_hook {
                continue; // Qualified once before launch, not after Hook writes.
            }
            let ServiceAllocationOrigin::Existing(id) = resource.allocation else {
                return Err(PersistenceError::CorruptServiceStorage(
                    "unresolved target allocation",
                ));
            };
            let observed = fs::observe_resource(&roots[&id], &resource.declaration.locator)
                .map_err(|_| {
                    PersistenceError::ServiceStorageUnavailable("target create presence is Unknown")
                })?;
            if !matches!(
                (predicate, observed),
                (
                    ServiceCreatePresence::Present,
                    fs::ResourceLookup::Present { .. }
                ) | (
                    ServiceCreatePresence::Absent,
                    fs::ResourceLookup::Absent { .. }
                )
            ) {
                return Err(PersistenceError::InvalidRunTransition(
                    "target create presence predicate is not satisfied".into(),
                ));
            }
        }
        for id in &edge.consumed_resources {
            tx.execute("DELETE FROM instance_service_resources WHERE instance_id=?1 AND resource_identity=?2", params![instance.as_bytes().as_slice(), id.as_str().as_bytes()])
                .map_err(|e| PersistenceError::sqlite("consume source resource association", e))?;
        }
        for id in &edge.consumed_storages {
            tx.execute("DELETE FROM instance_service_storages WHERE instance_id=?1 AND storage_identity=?2", params![instance.as_bytes().as_slice(), id.as_str().as_bytes()])
                .map_err(|e| PersistenceError::sqlite("consume source storage association", e))?;
        }
        for (id, storage) in edge
            .after
            .storages
            .iter()
            .filter(|(_, s)| s.declaration_revision == edge.after.revision)
        {
            let ServiceAllocationOrigin::Existing(allocation) = storage.allocation else {
                return Err(PersistenceError::CorruptServiceStorage(
                    "unprepared storage allocation",
                ));
            };
            tx.execute(
                "INSERT INTO instance_service_storages VALUES(?1,?2,?3,?4,?5)",
                params![
                    instance.as_bytes().as_slice(),
                    id.as_str().as_bytes(),
                    allocation.as_bytes().as_slice(),
                    edge.after.revision.package_id.as_bytes().as_slice(),
                    edge.after.revision.content_digest.as_bytes().as_slice()
                ],
            )
            .map_err(|e| PersistenceError::sqlite("publish target storage association", e))?;
        }
        for (id, resource) in edge
            .after
            .resources
            .iter()
            .filter(|(_, r)| r.declaration_revision == edge.after.revision)
        {
            let ServiceAllocationOrigin::Existing(allocation) = resource.allocation else {
                return Err(PersistenceError::CorruptServiceStorage(
                    "unprepared resource allocation",
                ));
            };
            tx.execute(
                "INSERT INTO instance_service_resources VALUES(?1,?2,?3,?4,?5)",
                params![
                    instance.as_bytes().as_slice(),
                    id.as_str().as_bytes(),
                    allocation.as_bytes().as_slice(),
                    edge.after.revision.package_id.as_bytes().as_slice(),
                    edge.after.revision.content_digest.as_bytes().as_slice()
                ],
            )
            .map_err(|e| PersistenceError::sqlite("publish target resource association", e))?;
        }
        Ok(roots.into_values().collect())
    }
}
