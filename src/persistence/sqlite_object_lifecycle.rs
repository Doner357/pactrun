use super::{PactrunPersistence, PersistenceError};
use crate::domain::{DeletionFacts, ObjectDeletion, ObjectDeletionResult, RunState};
use rusqlite::{Connection, Params, TransactionBehavior};

pub(super) fn validate_reference_graph(db: &Connection) -> Result<(), PersistenceError> {
    if exists(
        db,
        "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
        [],
    )? {
        return Err(PersistenceError::SchemaMismatch(
            "managed reference graph has dangling references".to_owned(),
        ));
    }
    Ok(())
}

fn exists(db: &Connection, sql: &str, params: impl Params) -> Result<bool, PersistenceError> {
    db.query_row(sql, params, |r| r.get(0))
        .map_err(|e| PersistenceError::sqlite("inspect object lifetime", e))
}

impl PactrunPersistence {
    pub(crate) fn collect_content(
        &self,
        execute: bool,
    ) -> Result<crate::domain::CollectionReport, PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(if execute {
                TransactionBehavior::Immediate
            } else {
                TransactionBehavior::Deferred
            })
            .map_err(|e| PersistenceError::sqlite("observe collection roots", e))?;
        if execute {
            self.check_write_admission(&tx)?;
        }
        validate_reference_graph(&tx)?;
        let mut roots = std::collections::BTreeSet::new();
        let mut query = tx.prepare("SELECT package_id,revision_content_digest FROM revisions ORDER BY package_id,revision_content_digest")
            .map_err(|e| PersistenceError::sqlite("enumerate installed roots", e))?;
        let ids = query
            .query_map([], |r| {
                Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .map_err(|e| PersistenceError::sqlite("read installed roots", e))?;
        for id in ids {
            let (package, digest) =
                id.map_err(|e| PersistenceError::sqlite("read installed identity", e))?;
            let id = super::sqlite_instances::revision_identity(package, digest)?;
            let stored =
                super::sqlite_revision_store::load_revision_from(&tx, &id)?.ok_or_else(|| {
                    PersistenceError::CorruptRevision("missing collection root".to_owned())
                })?;
            for file in stored.content.runtime_content.files() {
                roots.insert(file.blob_digest.clone());
            }
        }
        drop(query);
        // Validate referenced files before deleting anything. SQL references alone
        // cannot bless a mismatched canonical closure or missing/corrupt root.
        for digest in &roots {
            self.runtime_content.open_verified(digest)?;
        }
        let result = self.runtime_content.collect_unreferenced(&roots, execute)?;
        // Collection changes only files. Releasing a read-only SQL observation is
        // not a second publication boundary and must not hide a partial report.
        drop(tx);
        Ok(result)
    }

    pub(crate) fn delete_object(
        &self,
        target: &ObjectDeletion,
    ) -> Result<ObjectDeletionResult, PersistenceError> {
        use ObjectDeletionResult::{AlreadyAbsent, Blocked, Deleted};
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin object deletion", e))?;
        self.check_write_admission(&tx)?;
        validate_reference_graph(&tx)?;
        match target {
            ObjectDeletion::Snapshot(id) => {
                let args = [id.as_bytes().as_slice()];
                if !exists(
                    &tx,
                    "SELECT EXISTS(SELECT 1 FROM snapshots WHERE snapshot_id=?1)",
                    args,
                )? {
                    return Ok(AlreadyAbsent);
                }
                // Validate identity and closure before deleting any owned content.
                super::sqlite_snapshots::snapshot_producer(&tx, *id)?;
                let facts = DeletionFacts::Snapshot {
                    retained_for_restore: exists(
                        &tx,
                        "SELECT EXISTS(SELECT 1 FROM run_restore_admissions WHERE snapshot_id=?1 UNION ALL SELECT 1 FROM run_restore_invocations i JOIN run_executions e ON e.run_id=i.run_id WHERE i.snapshot_id=?1 UNION ALL SELECT 1 FROM run_restore_invocations i JOIN instance_recovery_guards g ON g.run_id=i.run_id WHERE i.snapshot_id=?1)",
                        args,
                    )?,
                };
                if let Some(reason) = facts.block() {
                    return Ok(Blocked(reason));
                }
                tx.execute("DELETE FROM snapshots WHERE snapshot_id=?1", args)
                    .map_err(|e| PersistenceError::sqlite("delete Snapshot", e))?;
            }
            ObjectDeletion::Revision(id) => {
                if super::sqlite_revision_store::load_revision_from(&tx, id)?.is_none() {
                    return Ok(AlreadyAbsent);
                }
                let args = [
                    id.package_id.as_bytes().as_slice(),
                    id.content_digest.as_bytes().as_slice(),
                ];
                let facts = DeletionFacts::Revision {
                    active_instance: exists(
                        &tx,
                        "SELECT EXISTS(SELECT 1 FROM instances WHERE active_package_id=?1 AND active_revision_content_digest=?2)",
                        args,
                    )?,
                    execution_pin: exists(
                        &tx,
                        "SELECT EXISTS(SELECT 1 FROM run_revision_pins WHERE package_id=?1 AND revision_content_digest=?2 UNION ALL SELECT 1 FROM run_migration_revision_pins WHERE package_id=?1 AND revision_content_digest=?2)",
                        args,
                    )?,
                    service_reference: exists(
                        &tx,
                        "SELECT EXISTS(SELECT 1 FROM service_storage_preparations WHERE package_id=?1 AND revision_content_digest=?2 UNION ALL SELECT 1 FROM instance_service_storages WHERE declaration_package_id=?1 AND declaration_revision_digest=?2 UNION ALL SELECT 1 FROM instance_service_resources WHERE declaration_package_id=?1 AND declaration_revision_digest=?2)",
                        args,
                    )?,
                };
                if let Some(reason) = facts.block() {
                    return Ok(Blocked(reason));
                }
                tx.execute("DELETE FROM revision_runtime_content_refs WHERE package_id=?1 AND revision_content_digest=?2", args)
                    .map_err(|e| PersistenceError::sqlite("release Revision content references", e))?;
                tx.execute(
                    "DELETE FROM revisions WHERE package_id=?1 AND revision_content_digest=?2",
                    args,
                )
                .map_err(|e| PersistenceError::sqlite("delete Revision installation", e))?;
            }
            ObjectDeletion::Run {
                run,
                delete_artifacts,
            } => {
                let args = [run.as_bytes().as_slice()];
                if !exists(
                    &tx,
                    "SELECT EXISTS(SELECT 1 FROM runs WHERE run_id=?1)",
                    args,
                )? {
                    return Ok(AlreadyAbsent);
                }
                let view = super::sqlite_runs::load_managed_run_from(&tx, *run)?;
                let facts = DeletionFacts::Run {
                    running: matches!(view.state, RunState::Running(_)),
                    retirement_evidence: exists(
                        &tx,
                        "SELECT EXISTS(SELECT 1 FROM instance_deletion_obligations WHERE attempt_run_id=?1 UNION ALL SELECT 1 FROM deletion_finalization_authorizations WHERE attempt_run_id=?1 UNION ALL SELECT 1 FROM instance_retirement_receipts WHERE retirement_run_id=?1)",
                        args,
                    )?,
                    recovery_evidence: exists(
                        &tx,
                        "SELECT EXISTS(SELECT 1 FROM instance_recovery_guards WHERE run_id=?1 UNION ALL SELECT 1 FROM run_revision_pins WHERE run_id=?1 UNION ALL SELECT 1 FROM run_payload_pins WHERE run_id=?1 UNION ALL SELECT 1 FROM run_migration_revision_pins WHERE run_id=?1 UNION ALL SELECT 1 FROM run_migration_payload_pins WHERE run_id=?1 UNION ALL SELECT 1 FROM run_migration_checkpoint_bindings WHERE run_id=?1 UNION ALL SELECT 1 FROM run_service_storage_pins WHERE run_id=?1 UNION ALL SELECT 1 FROM run_service_storage_targets WHERE run_id=?1 UNION ALL SELECT 1 FROM run_service_resource_targets WHERE run_id=?1 UNION ALL SELECT 1 FROM run_restore_admissions WHERE run_id=?1)",
                        args,
                    )?,
                    artifacts: exists(
                        &tx,
                        "SELECT EXISTS(SELECT 1 FROM run_artifacts WHERE run_id=?1)",
                        args,
                    )?,
                    authorize_artifacts: *delete_artifacts,
                };
                if let Some(reason) = facts.block() {
                    return Ok(Blocked(reason));
                }
                tx.execute("DELETE FROM runs WHERE run_id=?1", args)
                    .map_err(|e| PersistenceError::sqlite("delete Run history", e))?;
            }
        }
        super::fault(super::FaultPoint::BeforeObjectDeletionCommit);
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit object deletion", e))?;
        super::fault(super::FaultPoint::AfterObjectDeletionCommit);
        Ok(Deleted)
    }
}
