//! Typed storage custody and Instance association publication. Service bytes
//! never enter SQLite, Managed Inputs, metadata, or execution scratch.
use super::sqlite_revision_store::{FaultPoint, fault, load_revision_from};
use super::{PactrunPersistence, PersistenceError};
use crate::{domain::*, service_storage as fs};
use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    path::PathBuf,
};

pub(super) struct PreparedInstanceStorage {
    instance: InstanceId,
    revision: RevisionIdentity,
    owner: ExecutionOwnerSession,
    roots: Vec<(ServiceStorageIdentity, ServiceAllocationId, File)>,
    resources: Vec<ServiceResourceV2>,
    namespace: Option<(PathBuf, File, File)>,
}

impl PactrunPersistence {
    pub(super) fn prepare_instance_storage(
        &self,
        instance: InstanceId,
        revision: &RevisionIdentity,
    ) -> Result<PreparedInstanceStorage, PersistenceError> {
        self.prepare_service_storage_subset(instance, revision, None, None)
    }
    pub(super) fn prepare_service_storage_subset(
        &self,
        instance: InstanceId,
        revision: &RevisionIdentity,
        selected: Option<&BTreeSet<ServiceStorageIdentity>>,
        origin_run: Option<RunId>,
    ) -> Result<PreparedInstanceStorage, PersistenceError> {
        let stored = self
            .load_revision(revision)?
            .ok_or_else(|| PersistenceError::MissingRevision(revision.clone()))?;
        let owner = self
            .staging_session()
            .ok_or(PersistenceError::WriterAdmissionRequired)?
            .owner();
        let Some(core) = stored.content.core.service_core() else {
            if selected.is_some_and(|s| !s.is_empty()) {
                return Err(PersistenceError::CorruptServiceStorage(
                    "V1 has no declared storage allocation",
                ));
            }
            return Ok(PreparedInstanceStorage {
                instance,
                revision: revision.clone(),
                owner,
                roots: vec![],
                resources: vec![],
                namespace: None,
            });
        };
        let mut allocations = Vec::new();
        if selected.is_some_and(|ids| ids.iter().any(|id| !core.has_storage(id))) {
            return Err(PersistenceError::CorruptServiceStorage(
                "allocation subset is not declared",
            ));
        }
        for storage in core
            .storages()
            .iter()
            .filter(|s| selected.is_none_or(|ids| ids.contains(&s.id)))
        {
            let id = ServiceAllocationId::generate().map_err(|_| {
                PersistenceError::ServiceStorageUnavailable("allocation identity generation failed")
            })?;
            allocations.push((storage.id.clone(), id));
        }
        if allocations.is_empty() {
            return Ok(PreparedInstanceStorage {
                instance,
                revision: revision.clone(),
                owner,
                roots: vec![],
                resources: core.resources().to_vec(),
                namespace: None,
            });
        }
        self.reconcile_absent_service_preparations()?;
        {
            let mut db = self
                .database
                .lock()
                .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|e| PersistenceError::sqlite("begin service allocation intent", e))?;
            self.check_write_admission(&tx)?;
            for (storage, id) in &allocations {
                tx.execute(
                    "INSERT INTO service_storage_allocations VALUES(?1,?2,?3,?4,?5)",
                    params![
                        id.as_bytes().as_slice(),
                        instance.as_bytes().as_slice(),
                        revision.package_id.as_bytes().as_slice(),
                        revision.content_digest.as_bytes().as_slice(),
                        storage.as_str().as_bytes()
                    ],
                )
                .map_err(|e| PersistenceError::sqlite("publish service allocation identity", e))?;
                tx.execute(
                    "INSERT INTO service_storage_preparations VALUES(?1,?2,?3,?4)",
                    params![
                        id.as_bytes().as_slice(),
                        owner.as_str().as_bytes(),
                        revision.package_id.as_bytes().as_slice(),
                        revision.content_digest.as_bytes().as_slice()
                    ],
                )
                .map_err(|e| PersistenceError::sqlite("publish service preparation owner", e))?;
                if let Some(run) = origin_run {
                    tx.execute(
                        "INSERT INTO service_storage_run_origins VALUES(?1,?2)",
                        params![id.as_bytes().as_slice(), run.as_bytes().as_slice()],
                    )
                    .map_err(|e| PersistenceError::sqlite("record allocation origin Run", e))?;
                }
            }
            fault(FaultPoint::BeforeServiceAllocationIntentCommit);
            tx.commit()
                .map_err(|e| PersistenceError::sqlite("commit service allocation intent", e))?;
            fault(FaultPoint::AfterServiceAllocationIntentCommit);
        }
        let root_path = self.database_path.parent().and_then(|p| p.parent()).ok_or(
            PersistenceError::ServiceStorageUnavailable("storage root is unavailable"),
        )?;
        let root = fs::open_root(root_path).map_err(|_| {
            PersistenceError::ServiceStorageUnavailable("storage root qualification failed")
        })?;
        let base = match fs::open_directory(&root, "service-storage") {
            Ok(base) => base,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_directory(&root, "service-storage") {
                    Ok(base) => base,
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        fs::open_directory(&root, "service-storage").map_err(|_| {
                            PersistenceError::ServiceStorageUnavailable(
                                "service container qualification failed",
                            )
                        })?
                    }
                    Err(_) => {
                        return Err(PersistenceError::ServiceStorageUnavailable(
                            "service container creation failed",
                        ));
                    }
                }
            }
            Err(_) => {
                return Err(PersistenceError::ServiceStorageUnavailable(
                    "service container qualification failed",
                ));
            }
        };
        let mut roots = Vec::new();
        for (storage, id) in allocations {
            let directory = fs::create_directory(&base, &format!("alloc-{id}")).map_err(|_| {
                PersistenceError::ServiceStorageUnavailable(
                    "allocation creation failed; preparation is preserved",
                )
            })?;
            roots.push((storage, id, directory));
            fault(FaultPoint::AfterServiceAllocationDirectory);
        }
        Ok(PreparedInstanceStorage {
            instance,
            revision: revision.clone(),
            owner,
            roots,
            resources: core.resources().to_vec(),
            namespace: Some((root_path.to_owned(), root, base)),
        })
    }
}

impl PreparedInstanceStorage {
    pub(super) fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }
    pub(super) fn publish(
        &self,
        tx: &Transaction<'_>,
        instance: InstanceId,
        revision: &RevisionIdentity,
    ) -> Result<(), PersistenceError> {
        self.promote(tx, instance, revision, true)
    }
    pub(super) fn protect_targets(
        &self,
        tx: &Transaction<'_>,
        instance: InstanceId,
        revision: &RevisionIdentity,
    ) -> Result<BTreeMap<ServiceStorageIdentity, ServiceAllocationId>, PersistenceError> {
        self.promote(tx, instance, revision, false)?;
        Ok(self
            .roots
            .iter()
            .map(|(name, id, _)| (name.clone(), *id))
            .collect())
    }
    fn promote(
        &self,
        tx: &Transaction<'_>,
        instance: InstanceId,
        revision: &RevisionIdentity,
        publish: bool,
    ) -> Result<(), PersistenceError> {
        if self.instance != instance || &self.revision != revision {
            return Err(PersistenceError::CorruptServiceStorage(
                "prepared Instance context mismatch",
            ));
        }
        // A held handle alone can point at a renamed/unlinked directory. Reopen
        // every namespace level and compare object identity before publication.
        // No live contents are inspected or adopted by this qualification.
        let current_base = self
            .namespace
            .as_ref()
            .map(|(path, root, base)| {
                let current_root = fs::open_root(path)?;
                let current_base = fs::open_directory(&current_root, "service-storage")?;
                if !fs::same_opened_object(root, &current_root)?
                    || !fs::same_opened_object(base, &current_base)?
                {
                    return Err(std::io::Error::other("prepared namespace changed"));
                }
                Ok(current_base)
            })
            .transpose()
            .map_err(|_: std::io::Error| {
                PersistenceError::ServiceStorageUnavailable(
                    "prepared namespace qualification failed",
                )
            })?;
        let allocations: BTreeMap<_, _> =
            self.roots.iter().map(|(name, id, _)| (name, id)).collect();
        for (name, id, directory) in &self.roots {
            let base = current_base
                .as_ref()
                .ok_or(PersistenceError::CorruptServiceStorage(
                    "prepared namespace is missing",
                ))?;
            let current = fs::open_directory(base, &format!("alloc-{id}")).map_err(|_| {
                PersistenceError::ServiceStorageUnavailable("prepared allocation lookup failed")
            })?;
            if !fs::same_opened_object(directory, &current).map_err(|_| {
                PersistenceError::ServiceStorageUnavailable(
                    "prepared allocation identity unavailable",
                )
            })? {
                return Err(PersistenceError::ServiceStorageUnavailable(
                    "prepared allocation was replaced",
                ));
            }
            if !directory
                .metadata()
                .map_err(|_| {
                    PersistenceError::ServiceStorageUnavailable("prepared root observation failed")
                })?
                .is_dir()
            {
                return Err(PersistenceError::ServiceStorageUnavailable(
                    "prepared allocation is not a directory",
                ));
            }
            let owner: Vec<u8> = tx
                .query_row(
                    "SELECT owner_session FROM service_storage_preparations WHERE allocation_id=?1",
                    [id.as_bytes().as_slice()],
                    |r| r.get(0),
                )
                .map_err(|e| PersistenceError::sqlite("qualify prepared storage owner", e))?;
            if owner != self.owner.as_str().as_bytes() {
                return Err(PersistenceError::CorruptServiceStorage(
                    "prepared storage owner mismatch",
                ));
            }
            tx.execute(
                "INSERT INTO service_storage_protections VALUES(?1)",
                [id.as_bytes().as_slice()],
            )
            .map_err(|e| PersistenceError::sqlite("protect published storage", e))?;
            tx.execute(
                "DELETE FROM service_storage_preparations WHERE allocation_id=?1",
                [id.as_bytes().as_slice()],
            )
            .map_err(|e| PersistenceError::sqlite("finish storage preparation", e))?;
            if publish {
                tx.execute(
                    "INSERT INTO instance_service_storages VALUES(?1,?2,?3,?4,?5)",
                    params![
                        instance.as_bytes().as_slice(),
                        name.as_str().as_bytes(),
                        id.as_bytes().as_slice(),
                        revision.package_id.as_bytes().as_slice(),
                        revision.content_digest.as_bytes().as_slice()
                    ],
                )
                .map_err(|e| PersistenceError::sqlite("publish Instance storage association", e))?;
            }
        }
        if !publish {
            return Ok(());
        }
        for resource in &self.resources {
            let id = allocations.get(&resource.storage_id).ok_or(
                PersistenceError::CorruptServiceStorage("resource storage is not prepared"),
            )?;
            tx.execute(
                "INSERT INTO instance_service_resources VALUES(?1,?2,?3,?4,?5)",
                params![
                    instance.as_bytes().as_slice(),
                    resource.id.as_str().as_bytes(),
                    id.as_bytes().as_slice(),
                    revision.package_id.as_bytes().as_slice(),
                    revision.content_digest.as_bytes().as_slice()
                ],
            )
            .map_err(|e| PersistenceError::sqlite("publish Instance service resource", e))?;
        }
        Ok(())
    }
}

// Full association/retention readers will be shared by S3 and edge publication.
// This check is intentionally independent of live resource presence.
pub(super) fn validate_current_service_associations(
    db: &Connection,
    instance: InstanceId,
    revision: &RevisionIdentity,
) -> Result<(), PersistenceError> {
    validate_all_service_associations(db, instance, revision)?;
    let stored = load_revision_from(db, revision)?
        .ok_or_else(|| PersistenceError::MissingRevision(revision.clone()))?;
    let Some(core) = stored.content.core.service_core() else {
        return Ok(());
    };
    let mut active_allocations = std::collections::BTreeSet::new();
    for storage in core.storages() {
        let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM instance_service_storages s JOIN service_storage_allocations a ON a.allocation_id=s.allocation_id JOIN service_storage_protections p ON p.allocation_id=a.allocation_id WHERE s.instance_id=?1 AND s.storage_identity=?2 AND s.declaration_package_id=?3 AND s.declaration_revision_digest=?4 AND a.instance_id=?1 AND a.package_id=?3 AND NOT EXISTS(SELECT 1 FROM service_storage_preparations t WHERE t.allocation_id=a.allocation_id))",params![instance.as_bytes().as_slice(),storage.id.as_str().as_bytes(),revision.package_id.as_bytes().as_slice(),revision.content_digest.as_bytes().as_slice()],|r|r.get(0)).map_err(|e|PersistenceError::sqlite("validate current storage association",e))?;
        if !valid {
            return Err(PersistenceError::CorruptServiceStorage(
                "current storage association is missing or invalid",
            ));
        }
        let allocation: Vec<u8> = db.query_row(
            "SELECT allocation_id FROM instance_service_storages WHERE instance_id=?1 AND storage_identity=?2",
            params![instance.as_bytes().as_slice(), storage.id.as_str().as_bytes()],
            |r| r.get(0),
        ).map_err(|e| PersistenceError::sqlite("read active storage allocation", e))?;
        if !active_allocations.insert(allocation) {
            return Err(PersistenceError::CorruptServiceStorage(
                "active storages alias one allocation",
            ));
        }
    }
    for resource in core.resources() {
        let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM instance_service_resources r JOIN instance_service_storages s ON s.instance_id=r.instance_id AND s.storage_identity=?5 WHERE r.instance_id=?1 AND r.resource_identity=?2 AND r.declaration_package_id=?3 AND r.declaration_revision_digest=?4 AND r.allocation_id=s.allocation_id)",params![instance.as_bytes().as_slice(),resource.id.as_str().as_bytes(),revision.package_id.as_bytes().as_slice(),revision.content_digest.as_bytes().as_slice(),resource.storage_id.as_str().as_bytes()],|r|r.get(0)).map_err(|e|PersistenceError::sqlite("validate current resource association",e))?;
        if !valid {
            return Err(PersistenceError::CorruptServiceStorage(
                "current resource association is missing or invalid",
            ));
        }
    }
    Ok(())
}

// Retained associations remain authoritative contracts even when the current
// Revision has no service declarations (including a V1 current Revision).
fn validate_all_service_associations(
    db: &Connection,
    instance: InstanceId,
    current: &RevisionIdentity,
) -> Result<(), PersistenceError> {
    for (table, identity, storage) in [
        ("instance_service_storages", "storage_identity", true),
        ("instance_service_resources", "resource_identity", false),
    ] {
        // Identifiers above are closed implementation constants, never input.
        let sql = format!(
            "SELECT s.{identity}, s.declaration_package_id, s.declaration_revision_digest, \
             a.instance_id, a.package_id, \
             EXISTS(SELECT 1 FROM service_storage_protections p WHERE p.allocation_id=s.allocation_id), \
             EXISTS(SELECT 1 FROM service_storage_preparations p WHERE p.allocation_id=s.allocation_id) \
             FROM {table} s LEFT JOIN service_storage_allocations a ON a.allocation_id=s.allocation_id \
             WHERE s.instance_id=?1 ORDER BY s.{identity}"
        );
        let mut statement = db
            .prepare(&sql)
            .map_err(|e| PersistenceError::sqlite("prepare service association validation", e))?;
        let mut rows = statement
            .query([instance.as_bytes().as_slice()])
            .map_err(|e| PersistenceError::sqlite("read service associations", e))?;
        while let Some(row) = rows
            .next()
            .map_err(|e| PersistenceError::sqlite("read service association", e))?
        {
            let read = || -> rusqlite::Result<_> {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, bool>(6)?,
                ))
            };
            let (name, package, digest, owner, allocation_package, protected, preparing) =
                read().map_err(|e| PersistenceError::sqlite("decode service association", e))?;
            if package != current.package_id.as_bytes()
                || owner.as_deref() != Some(instance.as_bytes().as_slice())
                || allocation_package.as_deref() != Some(current.package_id.as_bytes().as_slice())
                || !protected
                || preparing
            {
                return Err(PersistenceError::CorruptServiceStorage(
                    "invalid published service allocation custody",
                ));
            }
            let digest: [u8; 32] = digest.try_into().map_err(|_| {
                PersistenceError::CorruptServiceStorage("invalid service declaration digest")
            })?;
            let declaration = RevisionIdentity {
                package_id: current.package_id,
                content_digest: RevisionContentDigest::from_bytes(digest),
            };
            let stored = load_revision_from(db, &declaration)?.ok_or(
                PersistenceError::CorruptServiceStorage("missing service declaration Revision"),
            )?;
            let core = stored.content.core.service_core().ok_or(
                PersistenceError::CorruptServiceStorage(
                    "service association refers to a V1 declaration",
                ),
            )?;
            let name = String::from_utf8(name).map_err(|_| {
                PersistenceError::CorruptServiceStorage("invalid service identity encoding")
            })?;
            let declared = if storage {
                let id = ServiceStorageIdentity::parse(name).map_err(|_| {
                    PersistenceError::CorruptServiceStorage("invalid storage identity")
                })?;
                core.storages().iter().any(|s| s.id == id)
            } else {
                let id = ServiceResourceIdentity::parse(name).map_err(|_| {
                    PersistenceError::CorruptServiceStorage("invalid resource identity")
                })?;
                core.resources().iter().any(|r| r.id == id)
            };
            if !declared {
                return Err(PersistenceError::CorruptServiceStorage(
                    "association identity is not declared by its Revision",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "sqlite_service_storage_tests.rs"]
mod tests;
