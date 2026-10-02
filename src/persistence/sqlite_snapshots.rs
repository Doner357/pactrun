//! Snapshot-owned immutable content, independent of Instance/Run/producer lifetime.
use super::{
    PactrunPersistence, PersistenceError, RuntimeContentStore, StoredRuntimeBlob,
    sqlite_revision_store::{FaultPoint, fault, load_revision_from},
};
use crate::{
    domain::*,
    managed_data::StagedFile,
    snapshot_bundle::{self, BundleError, ValidatedSnapshotBundle},
    snapshot_integrity::{SnapshotCodecError, VerifiedSnapshotManifest, decode_snapshot_manifest},
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use std::{
    collections::BTreeMap,
    io::{self, Write},
};

impl From<BundleError> for PersistenceError {
    fn from(e: BundleError) -> Self {
        Self::SnapshotBundle(e)
    }
}
impl From<SnapshotCodecError> for PersistenceError {
    fn from(e: SnapshotCodecError) -> Self {
        Self::SnapshotCodec(e)
    }
}
impl From<CapabilityRefusal> for PersistenceError {
    fn from(e: CapabilityRefusal) -> Self {
        Self::SnapshotCodec(SnapshotCodecError::Capability(e))
    }
}
#[derive(Debug)]
pub(crate) struct SnapshotImportReceipt {
    pub(crate) id: SnapshotId,
    pub(crate) inserted: bool,
    pub(crate) relational: SnapshotRelationalVerification,
}
#[derive(Debug)]
pub(crate) struct SnapshotInspection {
    pub(crate) id: SnapshotId,
    pub(crate) version: SnapshotIntegrityVersion,
    pub(crate) producer: RevisionIdentity,
    pub(crate) origin: InstanceId,
    pub(crate) captured_at: SnapshotTimestamp,
    pub(crate) restore_capability: Result<(), CapabilityRefusal>,
}
#[derive(Debug)]
pub(crate) struct SnapshotVerification {
    pub(crate) inspection: SnapshotInspection,
    pub(crate) relational: SnapshotRelationalVerification,
}
struct StoredSnapshot {
    manifest: VerifiedSnapshotManifest,
    lengths: BTreeMap<Sha256Digest, u64>,
}

fn admitted_restore_snapshot(db: &Connection, run: RunId) -> Result<SnapshotId, PersistenceError> {
    let view = super::sqlite_runs::load_managed_run_from(db, run)?;
    match (view.operation, view.state) {
        (ManagedRunIdentity::Restore { snapshot, .. }, RunState::Running(state))
            if state.boundary == ActionRunBoundary::Admitted =>
        {
            Ok(snapshot)
        }
        _ => Err(PersistenceError::InvalidRunTransition(
            "Restore content requires an admitted Restore Run".to_owned(),
        )),
    }
}

pub(super) fn replace_from_restore_snapshot(
    content: &RuntimeContentStore,
    tx: &Transaction<'_>,
    run: RunId,
    instance: InstanceId,
) -> Result<InstanceStateVersion, PersistenceError> {
    use super::sqlite_instances::{
        fresh_state_version, load_instance_view_from, payload_id, reclaim_payload_if_unreferenced,
        update_state_version,
    };
    let snapshot = admitted_restore_snapshot(tx, run)?;
    let target = load_instance_view_from(tx, instance)?
        .ok_or_else(|| PersistenceError::MissingInstance(instance.to_string()))?;
    let manifest = qualify_restore_snapshot(tx, content, &target, snapshot)?;
    let stored = load_snapshot(tx, snapshot)?;
    verify_stored(tx, content, &stored)?;
    let mut statement = tx
        .prepare("SELECT payload_id FROM managed_input_bindings WHERE instance_id=?1")
        .map_err(|e| PersistenceError::sqlite("read replaced payloads", e))?;
    let old = statement
        .query_map([instance.as_bytes().as_slice()], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .map_err(|e| PersistenceError::sqlite("read replaced payloads", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| PersistenceError::sqlite("read replaced payload", e))?;
    drop(statement);
    tx.execute(
        "DELETE FROM managed_input_bindings WHERE instance_id=?1",
        [instance.as_bytes().as_slice()],
    )
    .map_err(|e| PersistenceError::sqlite("replace Restore bindings", e))?;
    for binding in manifest.managed_bindings() {
        let SnapshotBindingState::Bound(digest) = &binding.state else {
            continue;
        };
        let payload = ManagedInputPayloadId::generate().map_err(|_| {
            PersistenceError::InvalidManagedInput(
                "Restore payload identity allocation failed".to_owned(),
            )
        })?;
        let length = stored.lengths[digest];
        SnapshotCapability::RestoreInput.check(length)?;
        tx.execute("INSERT INTO managed_input_payloads(instance_id,payload_id,protection_rank,byte_length,content_digest) VALUES (?1,?2,?3,?4,?5)",params![instance.as_bytes().as_slice(),payload.as_bytes().as_slice(),i64::from(binding.protection.rank()),i64::try_from(length).expect("qualified Managed Input"),digest.to_bytes().as_slice()]).map_err(|e|PersistenceError::sqlite("publish Restore payload reference",e))?;
        tx.execute("INSERT INTO managed_input_bindings(instance_id,input_identity,payload_id) VALUES (?1,?2,?3)",params![instance.as_bytes().as_slice(),binding.input_id.as_str().as_bytes(),payload.as_bytes().as_slice()]).map_err(|e|PersistenceError::sqlite("publish Restore binding",e))?;
    }
    for old in old {
        reclaim_payload_if_unreferenced(tx, instance, payload_id(old)?)?;
    }
    let next = fresh_state_version()?;
    update_state_version(tx, instance, next)?;
    tx.execute(
        "DELETE FROM instance_recovery_guards WHERE instance_id=?1",
        [instance.as_bytes().as_slice()],
    )
    .map_err(|e| PersistenceError::sqlite("resolve guard by Restore", e))?;
    Ok(next)
}

pub(super) fn snapshot_producer(
    db: &Connection,
    id: SnapshotId,
) -> Result<RevisionIdentity, PersistenceError> {
    Ok(load_snapshot(db, id)?
        .manifest
        .manifest()
        .producer()
        .clone())
}

fn load_snapshot(db: &Connection, id: SnapshotId) -> Result<StoredSnapshot, PersistenceError> {
    let header:Option<(String,Vec<u8>,i64)>=db.query_row("SELECT integrity_format,integrity_digest,length(canonical_manifest) FROM snapshots WHERE snapshot_id=?1",[id.as_bytes().as_slice()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|PersistenceError::sqlite("load Snapshot header",e))?;
    let (version, digest, length) = header.ok_or(PersistenceError::MissingSnapshot(id))?;
    fault(FaultPoint::AfterSnapshotReadEstablished);
    let length = u64::try_from(length)
        .map_err(|_| PersistenceError::CorruptSnapshot("invalid manifest length"))?;
    SnapshotCapability::CanonicalManifest.check(length)?;
    let version = SnapshotIntegrityVersion::from_text(&version)
        .map_err(|_| PersistenceError::CorruptSnapshot("invalid integrity version"))?;
    let raw: Vec<u8> = db
        .query_row(
            "SELECT canonical_manifest FROM snapshots WHERE snapshot_id=?1",
            [id.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .map_err(|e| PersistenceError::sqlite("load canonical Snapshot manifest", e))?;
    let manifest = decode_snapshot_manifest(version, &raw, None).map_err(|e| match e {
        SnapshotCodecError::Capability(_) => PersistenceError::SnapshotCodec(e),
        _ => PersistenceError::CorruptSnapshot("invalid canonical manifest"),
    })?;
    if manifest.manifest().snapshot_id() != id
        || manifest.canonical_bytes() != raw
        || digest.len() != 32
        || manifest.integrity_digest().as_str() != format!("sha256:{}", hex::encode(digest))
    {
        return Err(PersistenceError::CorruptSnapshot(
            "manifest identity or integrity disagreement",
        ));
    };
    let refs = snapshot_bundle::references(manifest.manifest());
    let mut budget = SnapshotBlobBudget::new(BlobAccountingProfile::SnapshotStorage);
    let mut lengths = BTreeMap::new();
    let mut query=db.prepare("SELECT blob_digest,byte_length FROM snapshot_blobs WHERE snapshot_id=?1 ORDER BY blob_digest").map_err(|e|PersistenceError::sqlite("inspect Snapshot closure",e))?;
    let rows = query
        .query_map([id.as_bytes().as_slice()], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(|e| PersistenceError::sqlite("read Snapshot closure", e))?;
    for row in rows {
        if lengths.len() >= refs.len() {
            return Err(PersistenceError::CorruptSnapshot("extra payload reference"));
        };
        let (digest, length) =
            row.map_err(|e| PersistenceError::sqlite("read Snapshot blob header", e))?;
        if digest.len() != 32 {
            return Err(PersistenceError::CorruptSnapshot("invalid payload digest"));
        };
        let digest = Sha256Digest::parse(format!("sha256:{}", hex::encode(digest)))
            .map_err(|_| PersistenceError::CorruptSnapshot("invalid payload digest"))?;
        let length = u64::try_from(length)
            .map_err(|_| PersistenceError::CorruptSnapshot("invalid payload length"))?;
        if !refs.contains(&digest) {
            return Err(PersistenceError::CorruptSnapshot("unreferenced payload"));
        };
        budget.record(digest.clone(), length).map_err(|e| match e {
            AccountingError::Capability(e) => PersistenceError::from(e),
            _ => PersistenceError::CorruptSnapshot("inconsistent payload"),
        })?;
        lengths.insert(digest, length);
    }
    if lengths.len() != refs.len() {
        return Err(PersistenceError::CorruptSnapshot(
            "missing payload reference",
        ));
    };
    Ok(StoredSnapshot { manifest, lengths })
}

pub(super) fn validate_collection_catalog(db: &Connection) -> Result<(), PersistenceError> {
    let mut query = db
        .prepare("SELECT snapshot_id FROM snapshots")
        .map_err(|e| PersistenceError::sqlite("inspect Snapshot collection roots", e))?;
    for row in query
        .query_map([], |r| r.get::<_, Vec<u8>>(0))
        .map_err(|e| PersistenceError::sqlite("inspect Snapshot collection roots", e))?
    {
        let raw =
            row.map_err(|e| PersistenceError::sqlite("inspect Snapshot collection root", e))?;
        load_snapshot(
            db,
            SnapshotId::from_bytes(raw.try_into().map_err(|_| {
                PersistenceError::CorruptSnapshot("invalid collection Snapshot identity")
            })?),
        )?;
    }
    let sql = "SELECT EXISTS(SELECT 1 FROM managed_input_payloads p WHERE (content_digest IS NOT NULL AND EXISTS(SELECT 1 FROM managed_input_payload_chunks c WHERE c.instance_id=p.instance_id AND c.payload_id=p.payload_id)) OR (content_digest IS NULL AND byte_length != (SELECT coalesce(sum(length(chunk_bytes)),0) FROM managed_input_payload_chunks c WHERE c.instance_id=p.instance_id AND c.payload_id=p.payload_id)))";
    if db
        .query_row::<bool, _, _>(sql, [], |r| r.get(0))
        .map_err(|e| PersistenceError::sqlite("validate immutable data representation", e))?
    {
        return Err(PersistenceError::CorruptSnapshot(
            "unreliable immutable data reference catalog",
        ));
    }
    Ok(())
}
fn producer_verification(
    db: &Connection,
    manifest: &SnapshotManifest,
) -> Result<SnapshotRelationalVerification, PersistenceError> {
    let producer = load_revision_from(db, manifest.producer())?;
    let context = producer.as_ref().map(|p| SnapshotProducerContext {
        revision: &p.identity,
        inputs: p.content.core.inputs(),
    });
    manifest
        .validate_producer(context.as_ref())
        .map_err(|e| PersistenceError::SnapshotCodec(e.into()))
}
fn stream_blob(
    db: &Connection,
    content: &RuntimeContentStore,
    id: SnapshotId,
    digest: &Sha256Digest,
    length: u64,
    target: &mut dyn Write,
) -> Result<(), PersistenceError> {
    SnapshotCapability::StoredBlob.check(length)?;
    let recorded: i64 = db
        .query_row(
            "SELECT byte_length FROM snapshot_blobs WHERE snapshot_id=?1 AND blob_digest=?2",
            params![id.as_bytes().as_slice(), digest.to_bytes().as_slice()],
            |r| r.get(0),
        )
        .map_err(|e| PersistenceError::sqlite("read immutable Snapshot reference", e))?;
    if u64::try_from(recorded).ok() != Some(length) {
        return Err(PersistenceError::CorruptSnapshot(
            "immutable Snapshot length mismatch",
        ));
    }
    super::immutable_data::stream(content, digest, length, target)
}
fn verify_stored(
    db: &Connection,
    content: &RuntimeContentStore,
    snapshot: &StoredSnapshot,
) -> Result<SnapshotRelationalVerification, PersistenceError> {
    let relational = producer_verification(db, snapshot.manifest.manifest())?;
    for (digest, length) in &snapshot.lengths {
        stream_blob(
            db,
            content,
            snapshot.manifest.manifest().snapshot_id(),
            digest,
            *length,
            &mut io::sink(),
        )?;
    }
    Ok(relational)
}
pub(super) fn inspection_from(
    db: &Connection,
    id: SnapshotId,
) -> Result<SnapshotInspection, PersistenceError> {
    Ok(inspection(&load_snapshot(db, id)?))
}

fn inspection(snapshot: &StoredSnapshot) -> SnapshotInspection {
    let manifest = snapshot.manifest.manifest();
    let mut budget = RestoreExpansionBudget::default();
    let capability = (|| {
        for b in manifest.managed_bindings() {
            if let SnapshotBindingState::Bound(d) = &b.state {
                budget.record_managed_input(snapshot.lengths[d])?;
            }
        }
        for c in manifest.service_content() {
            budget.record_service_descriptor(snapshot.lengths[&c.blob_digest])?;
        }
        Ok(())
    })();
    SnapshotInspection {
        id: manifest.snapshot_id(),
        version: manifest.version(),
        producer: manifest.producer().clone(),
        origin: manifest.origin_instance_id(),
        captured_at: manifest.captured_at(),
        restore_capability: capability,
    }
}

/// Reused by the read-only Compiler and by serialized Restore Admission.
/// It validates the selected immutable closure, not any ServiceStorage state.
pub(super) fn qualify_restore_snapshot(
    db: &Connection,
    content: &RuntimeContentStore,
    target: &InstanceView,
    id: SnapshotId,
) -> Result<SnapshotManifest, PersistenceError> {
    let snapshot = load_snapshot(db, id)?;
    if snapshot.manifest.manifest().producer() != &target.active_revision {
        return Err(PersistenceError::InvalidRunTransition(
            "Restore requires the exact producer Revision".to_owned(),
        ));
    }
    inspection(&snapshot).restore_capability?;
    if verify_stored(db, content, &snapshot)? != SnapshotRelationalVerification::Valid {
        return Err(PersistenceError::InvalidRunTransition(
            "Restore producer validation is unavailable".to_owned(),
        ));
    }
    validate_restore_transition(&target.bindings, snapshot.manifest.manifest())
        .map_err(|error| PersistenceError::InvalidRunTransition(error.to_string()))?;
    Ok(snapshot.manifest.manifest().clone())
}

impl PactrunPersistence {
    pub(crate) fn admitted_restore_manifest(
        &self,
        run: RunId,
    ) -> Result<SnapshotManifest, PersistenceError> {
        let db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let id = admitted_restore_snapshot(&db, run)?;
        let stored = load_snapshot(&db, id)?;
        if verify_stored(&db, &self.runtime_content, &stored)?
            != SnapshotRelationalVerification::Valid
        {
            return Err(PersistenceError::CorruptSnapshot(
                "Restore producer validation is unavailable",
            ));
        }
        inspection(&stored).restore_capability?;
        Ok(stored.manifest.manifest().clone())
    }

    pub(crate) fn copy_admitted_restore_blob(
        &self,
        run: RunId,
        digest: &Sha256Digest,
        target: &mut dyn Write,
    ) -> Result<(), PersistenceError> {
        let db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let id = admitted_restore_snapshot(&db, run)?;
        let length: i64 = db
            .query_row(
                "SELECT byte_length FROM snapshot_blobs WHERE snapshot_id=?1 AND blob_digest=?2",
                params![id.as_bytes().as_slice(), digest.to_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(|e| PersistenceError::sqlite("read admitted Restore blob", e))?;
        let length = u64::try_from(length)
            .map_err(|_| PersistenceError::CorruptSnapshot("negative blob length"))?;
        stream_blob(&db, &self.runtime_content, id, digest, length, target)
    }
    pub(crate) fn observe_snapshot_compilation(
        &self,
        intent: &SnapshotIntent,
    ) -> Result<SnapshotCompilationObservation, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| {
                PersistenceError::sqlite("begin Snapshot compilation observation", error)
            })?;
        let instance = super::sqlite_instances::load_instance_view_from(&tx, intent.instance)?
            .ok_or_else(|| PersistenceError::MissingInstance(intent.instance.to_string()))?;
        let stored = load_revision_from(&tx, &instance.active_revision)?
            .ok_or_else(|| PersistenceError::MissingRevision(instance.active_revision.clone()))?;
        let mut active_bindings = Vec::new();
        for declaration in stored.content.core.inputs() {
            if let Some((payload, protection)) =
                super::sqlite_instances::binding_payload(&tx, intent.instance, &declaration.id)?
            {
                active_bindings.push(ActiveInstanceBindingReference {
                    input: declaration.id.clone(),
                    payload,
                    protection,
                });
            }
        }
        for file in stored.content.runtime_content.files() {
            self.runtime_content.open_verified(&file.blob_digest)?;
        }
        let snapshot = match intent.operation {
            SnapshotOperation::Capture => None,
            SnapshotOperation::Restore(id) => Some(qualify_restore_snapshot(
                &tx,
                &self.runtime_content,
                &instance,
                id,
            )?),
        };
        Ok(SnapshotCompilationObservation {
            service_state: if stored.content.core.service_core().is_some() {
                super::sqlite_service_views::load_instance_service_state_from(&tx, intent.instance)?
            } else {
                None
            },
            instance,
            active_bindings,
            revision: stored.content,
            snapshot,
        })
    }
}

pub(super) fn insert_snapshot_manifest(
    tx: &Transaction<'_>,
    manifest: &VerifiedSnapshotManifest,
) -> Result<(), PersistenceError> {
    let digest = hex::decode(
        manifest
            .integrity_digest()
            .as_str()
            .strip_prefix("sha256:")
            .unwrap(),
    )
    .expect("typed digest");
    tx.execute("INSERT INTO snapshots(snapshot_id,integrity_format,integrity_digest,canonical_manifest) VALUES (?1,?2,?3,?4)",params![manifest.manifest().snapshot_id().as_bytes().as_slice(),manifest.manifest().version().as_str(),digest,manifest.canonical_bytes()]).map_err(|e|PersistenceError::sqlite("publish Snapshot manifest",e))?;
    Ok(())
}
impl PactrunPersistence {
    pub(crate) fn import_snapshot_bundle(
        &self,
        bundle: &mut ValidatedSnapshotBundle,
    ) -> Result<SnapshotImportReceipt, PersistenceError> {
        let id = bundle.manifest().manifest().snapshot_id();
        if self.session.is_none() {
            return Err(PersistenceError::WriterAdmissionRequired);
        }
        let mut files = Vec::new();
        for (digest, length) in bundle.lengths().clone() {
            files.push(super::immutable_data::publish(
                &self.runtime_content,
                &digest,
                length,
                &mut bundle.open_blob(&digest)?,
            )?);
        }
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin Snapshot import", e))?;
        self.check_write_admission(&tx)?;
        let relational = producer_verification(&tx, bundle.manifest().manifest())?;
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM snapshots WHERE snapshot_id=?1)",
                [id.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| PersistenceError::sqlite("resolve Snapshot identity", e))?;
        if exists {
            let old = load_snapshot(&tx, id)?;
            if old.manifest.integrity_digest() != bundle.manifest().integrity_digest() {
                return Err(PersistenceError::SnapshotCollision(id));
            };
            verify_stored(&tx, &self.runtime_content, &old)?;
            tx.commit()
                .map_err(|e| PersistenceError::sqlite("close idempotent import", e))?;
            return Ok(SnapshotImportReceipt {
                id,
                inserted: false,
                relational,
            });
        }
        insert_snapshot_manifest(&tx, bundle.manifest())?;
        for file in &files {
            insert_snapshot_file(&tx, id, file)?;
        }
        // Header closure validation is mandatory before committing even a validated bundle.
        load_snapshot(&tx, id)?;
        fault(FaultPoint::BeforeSnapshotImportCommit);
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit Snapshot import", e))?;
        fault(FaultPoint::AfterSnapshotImportCommit);
        Ok(SnapshotImportReceipt {
            id,
            inserted: true,
            relational,
        })
    }
    pub(crate) fn inspect_snapshot(
        &self,
        id: SnapshotId,
    ) -> Result<SnapshotInspection, PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("observe Snapshot", e))?;
        let snapshot = load_snapshot(&tx, id)?;
        Ok(inspection(&snapshot))
    }
    pub(crate) fn list_snapshots(
        &self,
        origin: Option<InstanceId>,
    ) -> Result<Vec<SnapshotInspection>, PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("observe Snapshot list", e))?;
        let mut query = tx
            .prepare("SELECT snapshot_id FROM snapshots ORDER BY snapshot_id")
            .map_err(|e| PersistenceError::sqlite("list Snapshot identities", e))?;
        let rows = query
            .query_map([], |r| r.get::<_, Vec<u8>>(0))
            .map_err(|e| PersistenceError::sqlite("read Snapshot identities", e))?;
        let mut result = Vec::new();
        for row in rows {
            let bytes: [u8; 16] = row
                .map_err(|e| PersistenceError::sqlite("read Snapshot identity", e))?
                .try_into()
                .map_err(|_| PersistenceError::CorruptSnapshot("invalid SnapshotId"))?;
            let snapshot = load_snapshot(&tx, SnapshotId::from_bytes(bytes))?;
            if origin.is_none_or(|o| o == snapshot.manifest.manifest().origin_instance_id()) {
                result.push(inspection(&snapshot));
            }
        }
        Ok(result)
    }
    pub(crate) fn verify_snapshot(
        &self,
        id: SnapshotId,
    ) -> Result<SnapshotVerification, PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("verify Snapshot read view", e))?;
        let snapshot = load_snapshot(&tx, id)?;
        let relational = verify_stored(&tx, &self.runtime_content, &snapshot)?;
        Ok(SnapshotVerification {
            inspection: inspection(&snapshot),
            relational,
        })
    }
    pub(crate) fn export_snapshot_stage(
        &self,
        id: SnapshotId,
        authorize_sensitive: bool,
    ) -> Result<StagedFile, PersistenceError> {
        if !authorize_sensitive {
            return Err(PersistenceError::UnauthorizedSnapshotExport);
        }
        let session = self
            .staging_session()
            .ok_or(PersistenceError::WriterAdmissionRequired)?;
        let mut stage = session
            .create_snapshot_stage()
            .map_err(|_| PersistenceError::SnapshotBundle(BundleError::Io(io::ErrorKind::Other)))?;
        {
            let mut db = self
                .database
                .lock()
                .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(|e| PersistenceError::sqlite("observe Snapshot export", e))?;
            let snapshot = load_snapshot(&tx, id)?;
            producer_verification(&tx, snapshot.manifest.manifest())?;
            let mut writer = snapshot_bundle::start_bundle(&mut stage, &snapshot.manifest)?;
            for (digest, length) in &snapshot.lengths {
                writer.start_blob(digest, *length)?;
                stream_blob(&tx, &self.runtime_content, id, digest, *length, &mut writer)?;
            }
            writer.finish()?;
        }
        let length = stage
            .finish_operation_file()
            .map_err(|source| PersistenceError::Io {
                operation: "finish Snapshot export",
                source,
            })?;
        SnapshotCapability::BundleBytes.check(length)?;
        Ok(stage)
    }
}

#[cfg(test)]
#[path = "sqlite_snapshot_tests.rs"]
mod tests;

#[cfg(test)]
pub(crate) fn corrupt_snapshot_for_test(root: &std::path::Path, id: SnapshotId) {
    let db = Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
    let mut query = db
        .prepare("SELECT blob_digest FROM snapshot_blobs WHERE snapshot_id=?1")
        .unwrap();
    let rows = query
        .query_map([id.as_bytes().as_slice()], |r| r.get::<_, Vec<u8>>(0))
        .unwrap();
    for row in rows {
        std::fs::OpenOptions::new()
            .append(true)
            .open(root.join("runtime-content").join(hex::encode(row.unwrap())))
            .unwrap()
            .write_all(&[0])
            .unwrap();
    }
}

pub(super) fn insert_snapshot_file(
    tx: &Transaction<'_>,
    id: SnapshotId,
    file: &StoredRuntimeBlob,
) -> Result<(), PersistenceError> {
    SnapshotCapability::StoredBlob.check(file.byte_len())?;
    tx.execute(
        "INSERT INTO snapshot_blobs(snapshot_id,blob_digest,byte_length) VALUES (?1,?2,?3)",
        params![
            id.as_bytes().as_slice(),
            file.digest().to_bytes().as_slice(),
            file.byte_len() as i64
        ],
    )
    .map_err(|e| PersistenceError::sqlite("publish immutable Snapshot reference", e))?;
    Ok(())
}
