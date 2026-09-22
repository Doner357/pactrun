//! Pack installation policy is resolved inside the authoritative transaction.
use super::{AcceptanceArbiter, AcceptanceCommitResult, PactrunPersistence, PersistenceError};
use crate::domain::*;
use rusqlite::{Transaction, TransactionBehavior};

pub(super) fn commit_install(
    tx: Transaction<'_>,
    cancellation: &impl AcceptanceArbiter,
) -> Result<(), PersistenceError> {
    match cancellation.before_durable_acceptance(|| {
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit Pack installation", e))
    }) {
        AcceptanceCommitResult::Committed(result) => result,
        AcceptanceCommitResult::Uncertain(error) => Err(error),
        AcceptanceCommitResult::Cancelled => Err(PersistenceError::Io {
            operation: "Pack installation cancelled before publication",
            source: std::io::Error::from(std::io::ErrorKind::Interrupted),
        }),
    }
}

pub(super) fn apply_install_metadata(
    tx: &Transaction<'_>,
    identity: &RevisionIdentity,
    batch: &RevisionMetadataMutationBatch,
    policy: Option<PackMetadataConflict>,
) -> Result<Vec<(PresentationTargetV1, PresentationField)>, PersistenceError> {
    use super::sqlite_revision_metadata::*;
    let Some(policy) = policy else {
        apply_revision_metadata_in_transaction(tx, identity, batch)?;
        return Ok(Vec::new());
    };
    let core = load_revision_core(tx, identity)?;
    let view = load_revision_metadata_from(tx, identity, &core)?;
    let mut operations = Vec::new();
    let mut kept = Vec::new();
    for operation in batch.operations() {
        if let RevisionMetadataMutation::CompareAndSetPresentation {
            target,
            field,
            desired: CurrentState::Present(value),
            ..
        } = operation
        {
            let current = view.items.iter().find_map(|item| match item {
                RevisionMetadataItem::Presentation(p)
                    if &p.target == target && p.field == *field =>
                {
                    Some(p.value.clone())
                }
                _ => None,
            });
            if current.as_ref().is_some_and(|v| v != value) {
                match policy {
                    PackMetadataConflict::Reject => {
                        return Err(PersistenceError::MetadataConflict(format!(
                            "presentation {target:?}/{field:?} differs for {identity:?}; retry with --metadata-conflict overwrite or --metadata-conflict keep"
                        )));
                    }
                    PackMetadataConflict::Keep => {
                        kept.push((target.clone(), *field));
                        continue;
                    }
                    PackMetadataConflict::Overwrite => {}
                }
            }
            operations.push(RevisionMetadataMutation::CompareAndSetPresentation {
                target: target.clone(),
                field: *field,
                expected: current.map_or(CurrentState::Absent, CurrentState::Present),
                desired: CurrentState::Present(value.clone()),
            });
        } else {
            operations.push(operation.clone());
        }
    }
    let planned = RevisionMetadataMutationBatch::new(operations)
        .map_err(|e| PersistenceError::InvalidMetadata(e.to_string()))?;
    apply_revision_metadata_in_transaction(tx, identity, &planned)?;
    Ok(kept)
}

impl PactrunPersistence {
    pub(crate) fn stage_pack_export(
        &self,
        identity: &RevisionIdentity,
        session: &crate::managed_data::StagingSession,
        include_metadata: bool,
        cancellation: &crate::hook::ActionCancellation,
    ) -> Result<crate::revision_installation::PreparedRevision, PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        // All deletion and collection writes serialize against this bounded acquisition.
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("acquire Revision export", e))?;
        self.check_write_admission(&tx)?;
        let stored = super::sqlite_revision_store::load_revision_from(&tx, identity)?
            .ok_or_else(|| PersistenceError::MissingRevision(identity.clone()))?;
        super::fault(super::FaultPoint::AfterPackExportRead);
        let metadata = if include_metadata {
            PortableMetadataTemplate::from_view(
                super::sqlite_revision_metadata::load_revision_metadata_from(
                    &tx,
                    identity,
                    stored.content.core.common(),
                )?,
            )
        } else {
            PortableMetadataTemplate::default()
        };
        let mut blobs = std::collections::BTreeMap::new();
        for file in stored.content.runtime_content.files() {
            if blobs.contains_key(&file.blob_digest) {
                continue;
            }
            let mut stage = session
                .create_snapshot_stage()
                .map_err(|e| PersistenceError::InvalidMetadata(e.to_string()))?;
            let mut writer = crate::pack_transport::CancelWriter {
                inner: stage.writer(),
                cancellation,
            };
            self.runtime_content
                .stream_verified(&file.blob_digest, &mut writer)?;
            stage
                .finish_operation_file()
                .map_err(|source| PersistenceError::Io {
                    operation: "finish Pack export content",
                    source,
                })?;
            blobs.insert(
                file.blob_digest.clone(),
                crate::managed_data::StagedRuntimeSource {
                    blob_digest: file.blob_digest.clone(),
                    bytes: stage,
                },
            );
        }
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("finish Revision export acquisition", e))?;
        Ok(crate::revision_installation::PreparedRevision {
            identity: identity.clone(),
            content: stored.content,
            metadata,
            blobs,
        })
    }
}
