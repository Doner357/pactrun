//! Capture and Restore publication using the shared terminal transaction.

use super::*;

impl PactrunPersistence {
    #[cfg(test)]
    pub(crate) fn fail_next_restore_publication_for_test() {
        FAIL_RESTORE_PUBLICATION.with(|v| v.set(true));
    }

    pub(crate) fn publish_restore(
        &self,
        owner: &ExecutionOwnerSession,
        run: RunId,
        finish: &RunFinish,
    ) -> Result<RunFinishReceipt, PersistenceError> {
        self.publish_restore_with_outputs(
            owner,
            run,
            finish,
            RunArtifactSubmission {
                submitted: &[],
                artifacts: &mut [],
            },
        )
    }

    pub(crate) fn publish_restore_with_outputs(
        &self,
        owner: &ExecutionOwnerSession,
        run: RunId,
        finish: &RunFinish,
        outputs: RunArtifactSubmission<'_, '_>,
    ) -> Result<RunFinishReceipt, PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin Restore publication", e))?;
        self.check_write_admission(&tx)?;
        let header = run_header(&tx, run)?;
        let view = load_managed_run_from(&tx, run)?;
        if !matches!(view.operation, ManagedRunIdentity::Restore { .. }) {
            return Err(PersistenceError::InvalidRunTransition(
                "Run is not Restore".to_owned(),
            ));
        }
        if matches!(view.state, RunState::Finished(_)) {
            return Ok(RunFinishReceipt {
                published_state_version: None,
            });
        }
        let (state,consequence):(Vec<u8>,i64)=tx.query_row("SELECT admitted_state_version,admitted_consequence_version FROM run_restore_admissions WHERE run_id=?1",[run.as_bytes().as_slice()],|row|Ok((row.get(0)?,row.get(1)?))).map_err(|e|PersistenceError::sqlite("load Restore publication tokens",e))?;
        let (_, _, current) = instance_header(&tx, header.instance)?;
        let conflict = current != state_version(state)?
            || consequence
                != super::super::writer_admission::load_consequence_version(&tx, header.instance)?;
        let mut finish = finish.clone();
        if conflict {
            finish.outcome = RunOutcome::Failed;
            finish.primary_failure=Some(RunPrimaryFailure { cause: None,failure:crate::domain::RunFailureRecord {error:crate::domain::PactrunErrorRef::new("execution","managed_output_publication_failed").expect("registered error"),message:"Restore publication conflicted with a changed Instance or recovery consequence".to_owned()},step:RunFailedStep::SnapshotPlan(crate::domain::SnapshotPlanStep::PublishManagedResult)});
        }
        let next = if conflict {
            None
        } else {
            Some(
                super::super::sqlite_snapshots::replace_from_restore_snapshot(
                    &self.runtime_content,
                    &tx,
                    run,
                    header.instance,
                )?,
            )
        };
        let mut receipt = finish_run_authorized(
            &tx,
            run,
            &header,
            &finish,
            outputs.artifacts,
            FinishAuthority {
                owner: Some(owner),
                outputs: Some(outputs.submitted),
                capture: None,
                restore: !conflict,
                migration: false,
                deletion: false,
            },
        )?;
        if next.is_some() {
            receipt.published_state_version = next;
        }
        #[cfg(test)]
        if FAIL_RESTORE_PUBLICATION.with(|v| v.replace(false)) {
            return Err(PersistenceError::DatabaseLockPoisoned);
        }
        fault(FaultPoint::BeforeRunFinishCommit);
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit Restore publication", e))?;
        fault(FaultPoint::AfterRunFinishCommit);
        Ok(receipt)
    }

    pub(crate) fn publish_capture<'a>(
        &'a self,
        owner: &'a ExecutionOwnerSession,
        run: RunId,
        finish: &RunFinish,
        manifest: &'a crate::snapshot_integrity::VerifiedSnapshotManifest,
        blobs: &'a mut [SnapshotBlobWrite<'a>],
    ) -> Result<RunFinishReceipt, PersistenceError> {
        self.publish_capture_with_outputs(
            owner,
            run,
            finish,
            manifest,
            blobs,
            RunArtifactSubmission {
                submitted: &[],
                artifacts: &mut [],
            },
        )
    }

    pub(crate) fn publish_capture_with_outputs<'a>(
        &'a self,
        owner: &'a ExecutionOwnerSession,
        run: RunId,
        finish: &RunFinish,
        manifest: &'a crate::snapshot_integrity::VerifiedSnapshotManifest,
        blobs: &'a mut [SnapshotBlobWrite<'a>],
        outputs: RunArtifactSubmission<'_, '_>,
    ) -> Result<RunFinishReceipt, PersistenceError> {
        if self.session.is_none() {
            return Err(PersistenceError::WriterAdmissionRequired);
        }
        let mut files = Vec::new();
        for blob in blobs.iter_mut() {
            files.push(super::super::immutable_data::publish(
                &self.runtime_content,
                &blob.digest,
                blob.byte_len,
                blob.reader,
            )?);
        }
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin Capture publication", e))?;
        self.check_write_admission(&tx)?;
        let header = run_header(&tx, run)?;
        let receipt = finish_run_authorized(
            &tx,
            run,
            &header,
            finish,
            outputs.artifacts,
            FinishAuthority {
                owner: Some(owner),
                outputs: Some(outputs.submitted),
                capture: Some(CapturePublication {
                    manifest,
                    blobs,
                    files,
                    content: &self.runtime_content,
                }),
                restore: false,
                migration: false,
                deletion: false,
            },
        )?;
        #[cfg(test)]
        if FAIL_CAPTURE_PUBLICATION.with(|v| v.replace(false)) {
            return Err(PersistenceError::DatabaseLockPoisoned);
        }
        fault(FaultPoint::BeforeRunFinishCommit);
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit Capture publication", e))?;
        fault(FaultPoint::AfterRunFinishCommit);
        Ok(receipt)
    }
}

pub(super) fn validate_capture_publication(
    tx: &Transaction<'_>,
    run: RunId,
    header: &RunHeader,
    revision: &RevisionIdentity,
    publication: &CapturePublication<'_>,
) -> Result<(), PersistenceError> {
    use crate::domain::{
        BlobAccountingProfile, InputProtectionV1, ManagedInputProtection, SnapshotBindingRole,
        SnapshotBindingState, SnapshotBlobBudget, SnapshotIntegrityVersion,
        SnapshotProducerContext,
    };
    use sha2::{Digest, Sha256};
    let invalid = || {
        PersistenceError::CorruptSnapshot(
            "Capture publication does not match its authoritative pinned view",
        )
    };
    let manifest = publication.manifest.manifest();
    if manifest.version() != SnapshotIntegrityVersion::BASELINE
        || manifest.producer() != revision
        || manifest.origin_instance_id() != header.instance
    {
        return Err(invalid());
    }
    let core = load_revision_core(tx, revision)?;
    manifest
        .validate_producer(Some(&SnapshotProducerContext {
            revision,
            inputs: core.inputs(),
        }))
        .map_err(|_| invalid())?;
    let mut statement=tx.prepare("SELECT input_identity,payload_id FROM run_payload_pins WHERE run_id=?1 ORDER BY input_identity").map_err(|e|PersistenceError::sqlite("read Capture pins",e))?;
    let rows = statement
        .query_map([run.as_bytes().as_slice()], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
        })
        .map_err(|e| PersistenceError::sqlite("read Capture pins", e))?;
    let mut pins = BTreeMap::new();
    for row in rows {
        let (input, payload) = row.map_err(|e| PersistenceError::sqlite("read Capture pin", e))?;
        pins.insert(
            InputIdentity::parse(utf8_message(input)?).map_err(|_| invalid())?,
            payload_id(payload)?,
        );
    }
    let declarations = core
        .inputs()
        .iter()
        .map(|d| (&d.id, d))
        .collect::<BTreeMap<_, _>>();
    let expected_keys = core
        .inputs()
        .iter()
        .map(|d| d.id.clone())
        .chain(pins.keys().cloned())
        .collect::<BTreeSet<_>>();
    if expected_keys.len() != manifest.managed_bindings().len() {
        return Err(invalid());
    }
    struct HashSink(Sha256);
    impl Write for HashSink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut digests = BTreeMap::new();
    for binding in manifest.managed_bindings() {
        let declaration = declarations.get(&binding.input_id);
        if binding.role
            != if declaration.is_some() {
                SnapshotBindingRole::Active
            } else {
                SnapshotBindingRole::Retained
            }
        {
            return Err(invalid());
        }
        match pins.get(&binding.input_id) {
            Some(payload) => {
                let rank:i64=tx.query_row("SELECT protection_rank FROM managed_input_payloads WHERE instance_id=?1 AND payload_id=?2",params![header.instance.as_bytes().as_slice(),payload.as_bytes().as_slice()],|r|r.get(0)).map_err(|e|PersistenceError::sqlite("read pinned protection",e))?;
                if binding.protection
                    != ManagedInputProtection::from_rank(rank).map_err(|_| invalid())?
                {
                    return Err(invalid());
                }
                if !digests.contains_key(payload) {
                    let mut sink = HashSink(Sha256::new());
                    stream_payload(
                        tx,
                        publication.content,
                        header.instance,
                        *payload,
                        &mut sink,
                    )?;
                    digests.insert(*payload, Sha256Digest::from_bytes(sink.0.finalize().into()));
                }
                if binding.state != SnapshotBindingState::Bound(digests[payload].clone()) {
                    return Err(invalid());
                }
            }
            None => {
                let declaration = declaration.ok_or_else(invalid)?;
                let protection = if declaration.protection == InputProtectionV1::Secret {
                    ManagedInputProtection::Secret
                } else {
                    ManagedInputProtection::Normal
                };
                if binding.state != SnapshotBindingState::Absent || binding.protection != protection
                {
                    return Err(invalid());
                }
            }
        }
    }
    let mut budget = SnapshotBlobBudget::new(BlobAccountingProfile::SnapshotStorage);
    let mut keys = BTreeSet::new();
    let accounting = |error| match error {
        crate::domain::AccountingError::Capability(error) => PersistenceError::from(error),
        crate::domain::AccountingError::InconsistentSourceLength => invalid(),
    };
    for blob in publication.blobs.iter() {
        if !keys.insert(blob.digest.clone()) {
            return Err(invalid());
        }
        budget
            .record(blob.digest.clone(), blob.byte_len)
            .map_err(accounting)?;
    }
    let refs = manifest
        .managed_bindings()
        .iter()
        .filter_map(|b| {
            if let SnapshotBindingState::Bound(d) = &b.state {
                Some(d.clone())
            } else {
                None
            }
        })
        .chain(
            manifest
                .service_content()
                .iter()
                .map(|c| c.blob_digest.clone()),
        )
        .collect::<BTreeSet<_>>();
    if keys != refs {
        return Err(invalid());
    }
    let lengths = publication
        .blobs
        .iter()
        .map(|b| (&b.digest, b.byte_len))
        .collect::<BTreeMap<_, _>>();
    let mut service_budget = SnapshotBlobBudget::new(BlobAccountingProfile::CapturedServiceContent);
    for descriptor in manifest.service_content() {
        service_budget
            .record(
                descriptor.blob_digest.clone(),
                lengths[&descriptor.blob_digest],
            )
            .map_err(accounting)?;
    }
    Ok(())
}
