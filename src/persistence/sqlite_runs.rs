//! PersistenceSchemaV4 Run, execution pin, Action recovery, and Artifact
//! repository.
//!
//! Every mutation is one `BEGIN IMMEDIATE` transaction. Run creation, pin
//! establishment, risk transitions, and a terminal publication without an
//! Instance consequence never publish a new `InstanceStateVersion`; a
//! `ManualRecoveryRequired` guard and `ResolveManualRecovery` always do.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use super::{
    PactrunPersistence, PersistenceError, VerifiedRuntimeBlob,
    chunked_blob::{ChunkedBlobTable, insert_chunks, stream_chunks},
    runtime_content_store::{RuntimeContentStore, RuntimeContentStoreError},
    sqlite_instances::{
        binding_payload, ensure_ordered, fresh_state_version, instance_header, instance_id,
        load_revision_core, payload_id, reclaim_payload_if_unreferenced, revision_identity,
        state_version, stream_payload, update_state_version,
    },
    sqlite_revision_store::{FaultPoint, fault},
};
use crate::domain::{
    ActionIdentity, ActionRunBoundary, ActionRunIdentity, AdmissionFacts, AdmissionRefusal,
    CompiledHookLaunch, ExecutionOwnerSession, HookCodeV1, HookCompletionRecord,
    HookCompletionStatus, InputIdentity, InstanceId, InstanceStateVersion,
    InterpreterLauncherObservation, MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1, ManagedInputPayloadId,
    ManagedOutputIdentity, ManualRecoveryTrigger, OperationAccessV1, PactrunErrorRefV1,
    RecoveryGuardView, RecoveryRiskState, RevisionCoreV1, RevisionIdentity, RunArtifactSummary,
    RunExecutionView, RunFailedStep, RunFailureRecord, RunFinish, RunId, RunInspectionData,
    RunOutcome, RunOutcomeView, RunPrimaryFailure, RunState, RunSummary, RunView, RuntimeFileV1,
    Sha256Digest, risk_transition, terminal_consequence, validate_running_action_state,
};

/// Re-runs the ordered interpreter launcher selection bound into a Plan. The
/// closure returns `Err(reason)` when the selection no longer yields the exact
/// bound path; the reason is diagnostic text only.
pub(crate) type LauncherCheck<'a> =
    dyn Fn(&InterpreterLauncherObservation) -> Result<(), String> + 'a;

pub(crate) struct RunArtifactWrite<'a> {
    pub(crate) output: ManagedOutputIdentity,
    pub(crate) byte_len: u64,
    pub(crate) reader: &'a mut dyn Read,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RunFinishReceipt {
    pub(crate) published_state_version: Option<InstanceStateVersion>,
}

const ARTIFACT_CHUNKS: ChunkedBlobTable = ChunkedBlobTable {
    insert_chunk_sql: "INSERT INTO run_artifact_chunks(\
        run_id, output_identity, chunk_index, chunk_bytes\
     ) VALUES (?1, ?2, ?3, ?4)",
    select_chunks_sql: "SELECT chunk_index, chunk_bytes FROM run_artifact_chunks \
         WHERE run_id=?1 AND output_identity=?2 ORDER BY chunk_index",
    read_operation: "read Run Artifact staging",
    write_operation: "write Run Artifact chunk",
    invalid: PersistenceError::InvalidRunArtifact,
    corrupt: PersistenceError::CorruptRun,
};

struct RunHeader {
    instance: InstanceId,
    accepted_state_version: InstanceStateVersion,
    accepted_at_unix_ms: u64,
}

struct OutcomeRow {
    outcome: RunOutcome,
    boundary: ActionRunBoundary,
    terminal_risk: RecoveryRiskState,
    finished_at_unix_ms: u64,
}

impl PactrunPersistence {
    pub(crate) fn create_accepted_run(
        &self,
        instance: InstanceId,
        accepted_state_version: InstanceStateVersion,
        action: &ActionRunIdentity,
        owner: &ExecutionOwnerSession,
    ) -> Result<RunId, PersistenceError> {
        let run = RunId::generate()
            .map_err(|error| PersistenceError::CorruptRun(format!("generate RunId: {error}")))?;
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin Run acceptance", error))?;
        instance_header(&transaction, instance)?;
        transaction
            .execute(
                "INSERT INTO runs(run_id, instance_id, accepted_state_version, accepted_at_unix_ms) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    run.as_bytes().as_slice(),
                    instance.as_bytes().as_slice(),
                    accepted_state_version.as_bytes().as_slice(),
                    unix_ms_now()?,
                ],
            )
            .map_err(|error| PersistenceError::sqlite("insert Run", error))?;
        transaction
            .execute(
                "INSERT INTO run_action_invocations(\
                    run_id, package_id, revision_content_digest, action_identity\
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    run.as_bytes().as_slice(),
                    action.revision.package_id.as_bytes().as_slice(),
                    action.revision.content_digest.as_bytes().as_slice(),
                    action.action.as_str().as_bytes(),
                ],
            )
            .map_err(|error| PersistenceError::sqlite("insert Run Action invocation", error))?;
        transaction
            .execute(
                "INSERT INTO run_executions(run_id, owner_session, risk_state) VALUES (?1, ?2, ?3)",
                params![
                    run.as_bytes().as_slice(),
                    owner.as_str().as_bytes(),
                    RecoveryRiskState::Clear.rank(),
                ],
            )
            .map_err(|error| PersistenceError::sqlite("insert Run execution owner", error))?;
        fault(FaultPoint::BeforeRunAcceptCommit);
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit Run acceptance", error))?;
        fault(FaultPoint::AfterRunAcceptCommit);
        Ok(run)
    }

    /// The authoritative Admission transaction of PR-REQ-0279. Every check reads
    /// persisted state (plus the launcher closure) inside one `BEGIN IMMEDIATE`;
    /// a refusal publishes the terminal `Failed` outcome and success inserts
    /// the pins in that same transaction. Checks outside it are advisory only.
    pub(crate) fn admit_run(
        &self,
        run: RunId,
        facts: &AdmissionFacts<'_>,
        launcher_check: &LauncherCheck<'_>,
        override_guard: bool,
    ) -> Result<Result<(), AdmissionRefusal>, PersistenceError> {
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin Run admission", error))?;
        let header = run_header(&transaction, run)?;
        let (_, risk_state) =
            execution_row(&transaction, run)?.ok_or(PersistenceError::RunNotRunning)?;
        let boundary = if has_revision_pin(&transaction, run)? {
            ActionRunBoundary::Admitted
        } else {
            ActionRunBoundary::Accepted
        };
        validate_running_action_state(boundary, risk_state)
            .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
        if boundary == ActionRunBoundary::Admitted {
            return Err(PersistenceError::InvalidRunTransition(
                "Run is already admitted".to_owned(),
            ));
        }
        let decision = admission_decision(
            &transaction,
            &self.runtime_content,
            run,
            &header,
            facts,
            launcher_check,
            override_guard,
        )?;
        match &decision {
            Ok(active_revision) => {
                transaction
                    .execute(
                        "INSERT INTO run_revision_pins(run_id, package_id, revision_content_digest) \
                         VALUES (?1, ?2, ?3)",
                        params![
                            run.as_bytes().as_slice(),
                            active_revision.package_id.as_bytes().as_slice(),
                            active_revision.content_digest.as_bytes().as_slice(),
                        ],
                    )
                    .map_err(|error| PersistenceError::sqlite("insert Run Revision pin", error))?;
                for binding in facts.active_bindings {
                    transaction
                        .execute(
                            "INSERT INTO run_payload_pins(\
                                run_id, instance_id, input_identity, payload_id\
                             ) VALUES (?1, ?2, ?3, ?4)",
                            params![
                                run.as_bytes().as_slice(),
                                header.instance.as_bytes().as_slice(),
                                binding.input.as_str().as_bytes(),
                                binding.payload.as_bytes().as_slice(),
                            ],
                        )
                        .map_err(|error| {
                            PersistenceError::sqlite("insert Run payload pin", error)
                        })?;
                }
            }
            Err(refusal) => {
                let finish = RunFinish {
                    outcome: RunOutcome::Failed,
                    primary_failure: Some(refusal.primary_failure()),
                    secondary_failures: Vec::new(),
                    hook_completion: None,
                };
                finish_run_in_transaction(&transaction, run, &header, &finish, &mut [])?;
            }
        }
        fault(FaultPoint::BeforeRunAdmitCommit);
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit Run admission", error))?;
        fault(FaultPoint::AfterRunAdmitCommit);
        Ok(decision.map(|_| ()))
    }

    pub(crate) fn open_recovery_risk(&self, run: RunId) -> Result<(), PersistenceError> {
        self.transition_recovery_risk(run, RecoveryRiskState::Open)
    }

    pub(crate) fn clear_recovery_risk(&self, run: RunId) -> Result<(), PersistenceError> {
        self.transition_recovery_risk(run, RecoveryRiskState::Clear)
    }

    pub(crate) fn stream_admitted_payload(
        &self,
        run: RunId,
        input: &InputIdentity,
        destination: &mut impl Write,
    ) -> Result<(), PersistenceError> {
        let mut database = self.open_read_connection()?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| PersistenceError::sqlite("begin admitted payload read", error))?;
        execution_row(&transaction, run)?.ok_or(PersistenceError::RunNotRunning)?;
        if !has_revision_pin(&transaction, run)? {
            return Err(PersistenceError::InvalidRunTransition(
                "Run is not admitted".to_owned(),
            ));
        }
        let row = transaction
            .query_row(
                "SELECT instance_id, payload_id FROM run_payload_pins \
                 WHERE run_id=?1 AND input_identity=?2",
                params![run.as_bytes().as_slice(), input.as_str().as_bytes()],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(|error| PersistenceError::sqlite("load admitted payload pin", error))?
            .ok_or_else(|| {
                PersistenceError::CorruptRun(format!(
                    "admitted Run has no payload pin for {}",
                    input.as_str()
                ))
            })?;
        stream_payload(
            &transaction,
            instance_id(row.0)?,
            payload_id(row.1)?,
            destination,
        )?;
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("close admitted payload read", error))?;
        Ok(())
    }

    pub(crate) fn open_admitted_runtime_blob(
        &self,
        run: RunId,
        file: &RuntimeFileV1,
    ) -> Result<VerifiedRuntimeBlob, PersistenceError> {
        let digest = {
            let database = self
                .database
                .lock()
                .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
            execution_row(&database, run)?.ok_or(PersistenceError::RunNotRunning)?;
            let bytes = database
                .query_row(
                    "SELECT refs.blob_digest \
                     FROM run_revision_pins AS pins \
                     JOIN revision_runtime_content_refs AS refs \
                       ON refs.package_id=pins.package_id \
                      AND refs.revision_content_digest=pins.revision_content_digest \
                     WHERE pins.run_id=?1 AND refs.content_id=?2",
                    params![run.as_bytes().as_slice(), file.id.as_str()],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()
                .map_err(|error| PersistenceError::sqlite("load admitted runtime pin", error))?
                .ok_or_else(|| {
                    PersistenceError::CorruptRun(format!(
                        "admitted Run has no runtime pin for {}",
                        file.id.as_str()
                    ))
                })?;
            let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
                PersistenceError::CorruptRun("runtime pin digest length is invalid".to_owned())
            })?;
            Sha256Digest::from_bytes(bytes)
        };
        if digest != file.blob_digest {
            return Err(PersistenceError::CorruptRun(format!(
                "admitted runtime pin changed for {}",
                file.id.as_str()
            )));
        }
        self.runtime_content
            .open_verified(&digest)
            .map_err(PersistenceError::from)
    }

    fn transition_recovery_risk(
        &self,
        run: RunId,
        requested: RecoveryRiskState,
    ) -> Result<(), PersistenceError> {
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin recovery risk transition", error))?;
        run_header(&transaction, run)?;
        let (_, current) =
            execution_row(&transaction, run)?.ok_or(PersistenceError::RunNotRunning)?;
        let boundary = if has_revision_pin(&transaction, run)? {
            ActionRunBoundary::Admitted
        } else {
            ActionRunBoundary::Accepted
        };
        validate_running_action_state(boundary, current)
            .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
        if boundary != ActionRunBoundary::Admitted {
            return Err(PersistenceError::InvalidRunTransition(
                "recovery risk requires an admitted Run".to_owned(),
            ));
        }
        let next = risk_transition(current, requested)
            .map_err(|error| PersistenceError::InvalidRunTransition(error.to_string()))?;
        transaction
            .execute(
                "UPDATE run_executions SET risk_state=?2 WHERE run_id=?1",
                params![run.as_bytes().as_slice(), next.rank()],
            )
            .map_err(|error| PersistenceError::sqlite("publish recovery risk state", error))?;
        fault(FaultPoint::BeforeRecoveryRiskCommit);
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit recovery risk transition", error))?;
        fault(FaultPoint::AfterRecoveryRiskCommit);
        Ok(())
    }

    pub(crate) fn finish_run(
        &self,
        run: RunId,
        finish: &RunFinish,
        artifacts: &mut [RunArtifactWrite<'_>],
    ) -> Result<RunFinishReceipt, PersistenceError> {
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin Run finish", error))?;
        let header = run_header(&transaction, run)?;
        let receipt = finish_run_in_transaction(&transaction, run, &header, finish, artifacts)?;
        fault(FaultPoint::BeforeRunFinishCommit);
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit Run finish", error))?;
        fault(FaultPoint::AfterRunFinishCommit);
        Ok(receipt)
    }

    /// Owner-authorized terminal publication used by the Action finalizer.
    /// The owner check, Running-state matrix, and submitted-output eligibility
    /// are all evaluated inside the same transaction as terminal publication.
    pub(crate) fn finish_run_owned(
        &self,
        owner: &ExecutionOwnerSession,
        run: RunId,
        finish: &RunFinish,
        submitted_outputs: &[ManagedOutputIdentity],
        artifacts: &mut [RunArtifactWrite<'_>],
    ) -> Result<RunFinishReceipt, PersistenceError> {
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin owned Run finish", error))?;
        let header = run_header(&transaction, run)?;
        let receipt = finish_run_in_transaction_owned(
            &transaction,
            run,
            &header,
            finish,
            artifacts,
            Some(owner),
            Some(submitted_outputs),
        )?;
        fault(FaultPoint::BeforeRunFinishCommit);
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit owned Run finish", error))?;
        fault(FaultPoint::AfterRunFinishCommit);
        Ok(receipt)
    }

    pub(crate) fn resolve_manual_recovery(
        &self,
        instance: InstanceId,
        expected: InstanceStateVersion,
    ) -> Result<InstanceStateVersion, PersistenceError> {
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin ResolveManualRecovery", error))?;
        let (_, _, current) = instance_header(&transaction, instance)?;
        if current != expected {
            return Err(PersistenceError::StaleInstanceState);
        }
        guard_row(&transaction, instance)?.ok_or(PersistenceError::MissingRecoveryGuard)?;
        transaction
            .execute(
                "DELETE FROM instance_recovery_guards WHERE instance_id=?1",
                [instance.as_bytes().as_slice()],
            )
            .map_err(|error| PersistenceError::sqlite("clear recovery guard", error))?;
        let next = fresh_state_version()?;
        update_state_version(&transaction, instance, next)?;
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit ResolveManualRecovery", error))?;
        Ok(next)
    }

    pub(crate) fn list_runs(
        &self,
        instance: InstanceId,
    ) -> Result<Vec<RunSummary>, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let mut statement = database
            .prepare("SELECT run_id FROM runs WHERE instance_id=?1 ORDER BY instance_id, run_id")
            .map_err(|error| PersistenceError::sqlite("prepare Run enumeration", error))?;
        let rows = statement
            .query_map([instance.as_bytes().as_slice()], |row| {
                row.get::<_, Vec<u8>>(0)
            })
            .map_err(|error| PersistenceError::sqlite("enumerate Runs", error))?;
        let mut result = Vec::new();
        for row in rows {
            let run =
                run_id(row.map_err(|error| PersistenceError::sqlite("read Run row", error))?)?;
            let view = load_run_from(&database, run)?;
            let outcome = match &view.state {
                RunState::Running(_) => None,
                RunState::Finished(outcome) => Some(outcome.outcome),
            };
            result.push(RunSummary {
                id: run,
                instance: view.instance,
                action: view.action,
                phase: view.state.phase(),
                outcome,
            });
        }
        ensure_ordered(&result, |left, right| left.id.cmp(&right.id))?;
        Ok(result)
    }

    pub(crate) fn load_run(&self, run: RunId) -> Result<Option<RunView>, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        match run_header(&database, run) {
            Ok(_) => load_run_from(&database, run).map(Some),
            Err(PersistenceError::MissingRun(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn load_run_inspection(
        &self,
        run: RunId,
    ) -> Result<Option<RunInspectionData>, PersistenceError> {
        self.load_run_inspection_with_hook(run, || {})
    }

    fn load_run_inspection_with_hook(
        &self,
        run: RunId,
        after_run_loaded: impl FnOnce(),
    ) -> Result<Option<RunInspectionData>, PersistenceError> {
        let mut database = self.open_read_connection()?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| PersistenceError::sqlite("begin Run inspection snapshot", error))?;
        let view = match load_run_from(&transaction, run) {
            Ok(view) => view,
            Err(PersistenceError::MissingRun(_)) => {
                transaction.commit().map_err(|error| {
                    PersistenceError::sqlite("close Run inspection snapshot", error)
                })?;
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        after_run_loaded();
        let current_recovery_guard =
            load_instance_recovery_guard_from(&transaction, view.instance)?;
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("close Run inspection snapshot", error))?;
        Ok(Some(RunInspectionData {
            run: view,
            current_recovery_guard,
        }))
    }

    /// Lists the durable owner records that may need owner-loss
    /// reconciliation. The caller must confirm lease loss before mutating any
    /// candidate returned here.
    pub(crate) fn list_running_action_runs(
        &self,
    ) -> Result<Vec<(InstanceId, RunId, ExecutionOwnerSession)>, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let mut statement = database
            .prepare(
                "SELECT runs.instance_id, runs.run_id, run_executions.owner_session \
                 FROM runs JOIN run_executions ON run_executions.run_id=runs.run_id \
                 ORDER BY runs.instance_id, runs.run_id",
            )
            .map_err(|error| {
                PersistenceError::sqlite("prepare Running Action enumeration", error)
            })?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(|error| PersistenceError::sqlite("enumerate Running Actions", error))?;
        let mut result = Vec::new();
        for row in rows {
            let (instance, run, owner) =
                row.map_err(|error| PersistenceError::sqlite("read Running Action", error))?;
            let owner = String::from_utf8(owner).map_err(|_| {
                PersistenceError::CorruptRun("owner session is not UTF-8".to_owned())
            })?;
            let run = run_id(run)?;
            let admitted = has_revision_pin(&database, run)?;
            let (_, risk_state) = execution_row(&database, run)?.ok_or_else(|| {
                PersistenceError::CorruptRun(
                    "Running Action enumeration lost its execution row".to_owned(),
                )
            })?;
            let boundary = if admitted {
                ActionRunBoundary::Admitted
            } else {
                ActionRunBoundary::Accepted
            };
            validate_running_action_state(boundary, risk_state)
                .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
            result.push((
                instance_id(instance)?,
                run,
                ExecutionOwnerSession::parse(owner)
                    .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
            ));
        }
        Ok(result)
    }

    /// Atomically converts a confirmed-lost owner Run into Interrupted. A
    /// second reconciler simply observes that the execution row is gone.
    pub(crate) fn reconcile_action_run(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
    ) -> Result<bool, PersistenceError> {
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                PersistenceError::sqlite("begin Action owner reconciliation", error)
            })?;
        let Some((actual_owner, _)) = execution_row(&transaction, run)? else {
            return Ok(false);
        };
        if actual_owner != *owner {
            return Ok(false);
        }
        let header = run_header(&transaction, run)?;
        let finish = RunFinish {
            outcome: RunOutcome::Interrupted,
            primary_failure: None,
            secondary_failures: Vec::new(),
            hook_completion: None,
        };
        finish_run_in_transaction_owned(
            &transaction,
            run,
            &header,
            &finish,
            &mut [],
            Some(owner),
            Some(&[]),
        )?;
        fault(FaultPoint::BeforeRunFinishCommit);
        transaction.commit().map_err(|error| {
            PersistenceError::sqlite("commit Action owner reconciliation", error)
        })?;
        fault(FaultPoint::AfterRunFinishCommit);
        Ok(true)
    }

    pub(crate) fn load_instance_recovery_guard(
        &self,
        instance: InstanceId,
    ) -> Result<Option<RecoveryGuardView>, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        load_instance_recovery_guard_from(&database, instance)
    }

    pub(crate) fn open_run_artifact(
        &self,
        run: RunId,
        output: &ManagedOutputIdentity,
        destination: &mut impl Write,
    ) -> Result<(), PersistenceError> {
        let mut database = self.open_read_connection()?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| PersistenceError::sqlite("begin Run Artifact snapshot", error))?;
        run_header(&transaction, run)?;
        let byte_length: i64 = transaction
            .query_row(
                "SELECT byte_length FROM run_artifacts WHERE run_id=?1 AND output_identity=?2",
                params![run.as_bytes().as_slice(), output.as_str().as_bytes()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| PersistenceError::sqlite("load Run Artifact header", error))?
            .ok_or_else(|| {
                PersistenceError::InvalidRunArtifact("Run has no such Artifact".to_owned())
            })?;
        let byte_length = u64::try_from(byte_length)
            .map_err(|_| PersistenceError::CorruptRun("negative Artifact length".to_owned()))?;
        stream_chunks(
            &transaction,
            &ARTIFACT_CHUNKS,
            [run.as_bytes().as_slice(), output.as_str().as_bytes()],
            byte_length,
            destination,
        )?;
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("close Run Artifact snapshot", error))?;
        Ok(())
    }

    pub(crate) fn delete_run_artifact(
        &self,
        run: RunId,
        output: &ManagedOutputIdentity,
    ) -> Result<bool, PersistenceError> {
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin Run Artifact expiry", error))?;
        run_header(&transaction, run)?;
        let deleted = transaction
            .execute(
                "DELETE FROM run_artifacts WHERE run_id=?1 AND output_identity=?2",
                params![run.as_bytes().as_slice(), output.as_str().as_bytes()],
            )
            .map_err(|error| PersistenceError::sqlite("expire Run Artifact", error))?;
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit Run Artifact expiry", error))?;
        Ok(deleted > 0)
    }
}

/// Evaluates the PR-REQ-0279 refusal precedence against persisted state. On
/// success returns the active Revision to pin.
fn admission_decision(
    transaction: &Transaction<'_>,
    runtime_content: &RuntimeContentStore,
    run: RunId,
    header: &RunHeader,
    facts: &AdmissionFacts<'_>,
    launcher_check: &LauncherCheck<'_>,
    override_guard: bool,
) -> Result<Result<RevisionIdentity, AdmissionRefusal>, PersistenceError> {
    // 1. Trust guard.
    if !override_guard && guard_row(transaction, header.instance)?.is_some() {
        return Ok(Err(AdmissionRefusal::RecoveryGuardActive));
    }

    // 2. Stale compilation or state facts.
    let (_, active_revision, current) = instance_header(transaction, header.instance)?;
    if current != facts.expected_state_version || current != header.accepted_state_version {
        return Ok(Err(AdmissionRefusal::PlanInvalidated(
            "the Instance state version changed since compilation".to_owned(),
        )));
    }
    let invocation = invocation_row(transaction, run)?;
    if invocation.revision != active_revision {
        return Ok(Err(AdmissionRefusal::PlanInvalidated(
            "the active Revision changed since compilation".to_owned(),
        )));
    }
    let core = load_revision_core(transaction, &active_revision)?;
    for binding in facts.active_bindings {
        let current_payload = binding_payload(transaction, header.instance, &binding.input)?
            .map(|(payload, _)| payload);
        if current_payload != Some(binding.payload) {
            return Ok(Err(AdmissionRefusal::PlanInvalidated(format!(
                "the current binding of Input {} changed since compilation",
                binding.input.as_str()
            ))));
        }
    }
    let mut unbound = Vec::new();
    for declaration in core.inputs() {
        if declaration.required
            && binding_payload(transaction, header.instance, &declaration.id)?.is_none()
        {
            unbound.push(declaration.id.as_str().to_owned());
        }
    }
    if !unbound.is_empty() {
        return Ok(Err(AdmissionRefusal::PlanInvalidated(format!(
            "required Inputs are not bound: {}",
            unbound.join(", ")
        ))));
    }
    let digests = facts
        .runtime_content
        .iter()
        .map(|file| file.blob_digest.clone())
        .collect::<BTreeSet<_>>();
    for digest in digests {
        match runtime_content.open_verified(&digest) {
            Ok(_) => {}
            Err(RuntimeContentStoreError::MissingBlob(_))
            | Err(RuntimeContentStoreError::CorruptBlob { .. }) => {
                return Ok(Err(AdmissionRefusal::PlanInvalidated(format!(
                    "runtime content {} is unavailable",
                    digest.as_str()
                ))));
            }
            Err(error) => return Err(PersistenceError::RuntimeContent(error)),
        }
    }
    if let CompiledHookLaunch::Interpreter { launcher, .. } = facts.launch
        && let Err(reason) = launcher_check(launcher)
    {
        return Ok(Err(AdmissionRefusal::PlanInvalidated(format!(
            "the interpreter launcher selection changed since compilation: {reason}"
        ))));
    }

    // 3. Mutate exclusivity, with both access modes read from persisted
    //    Revision declarations.
    if action_access(&core, &invocation.action, "the active Revision")? == OperationAccessV1::Mutate
    {
        let mut cores = BTreeMap::new();
        cores.insert(active_revision.clone(), core);
        for competitor in admitted_competitors(transaction, header.instance, run)? {
            if !cores.contains_key(&competitor.action.revision) {
                let pinned = load_revision_core(transaction, &competitor.action.revision)?;
                cores.insert(competitor.action.revision.clone(), pinned);
            }
            let pinned = &cores[&competitor.action.revision];
            if action_access(pinned, &competitor.action.action, "its pinned Revision")?
                == OperationAccessV1::Mutate
            {
                return Ok(Err(AdmissionRefusal::MutationConflict(competitor.run)));
            }
        }
    }
    Ok(Ok(active_revision))
}

fn action_access(
    core: &RevisionCoreV1,
    action: &ActionIdentity,
    source: &str,
) -> Result<OperationAccessV1, PersistenceError> {
    core.actions()
        .iter()
        .find(|declaration| &declaration.id == action)
        .map(|declaration| declaration.access)
        .ok_or_else(|| {
            PersistenceError::CorruptRun(format!(
                "Run Action {} is not declared by {source}",
                action.as_str()
            ))
        })
}

struct AdmittedCompetitor {
    run: RunId,
    action: ActionRunIdentity,
}

/// Every other Run on the Instance that is both Running and Admitted, in
/// `run_id` order.
fn admitted_competitors(
    transaction: &Transaction<'_>,
    instance: InstanceId,
    run: RunId,
) -> Result<Vec<AdmittedCompetitor>, PersistenceError> {
    let mut statement = transaction
        .prepare(
            "SELECT r.run_id, p.package_id, p.revision_content_digest, i.action_identity \
             FROM runs r \
             JOIN run_executions e ON e.run_id = r.run_id \
             JOIN run_revision_pins p ON p.run_id = r.run_id \
             JOIN run_action_invocations i ON i.run_id = r.run_id \
             WHERE r.instance_id = ?1 AND r.run_id <> ?2 \
             ORDER BY r.run_id",
        )
        .map_err(|error| PersistenceError::sqlite("prepare admitted Run enumeration", error))?;
    let rows = statement
        .query_map(
            params![instance.as_bytes().as_slice(), run.as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            },
        )
        .map_err(|error| PersistenceError::sqlite("enumerate admitted Runs", error))?;
    let mut competitors = Vec::new();
    for row in rows {
        let (id, package, digest, action) =
            row.map_err(|error| PersistenceError::sqlite("read admitted Run", error))?;
        let action = String::from_utf8(action)
            .map_err(|_| PersistenceError::CorruptRun("ActionIdentity is not UTF-8".to_owned()))?;
        competitors.push(AdmittedCompetitor {
            run: run_id(id)?,
            action: ActionRunIdentity {
                revision: revision_identity(package, digest)?,
                action: ActionIdentity::parse(action)
                    .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
            },
        });
    }
    Ok(competitors)
}

/// Publishes a terminal outcome inside an existing transaction: removes the
/// execution owner, records the outcome and its records, releases pins,
/// reclaims unreferenced payloads, and publishes the guard and a fresh state
/// version only when a consequence exists.
fn finish_run_in_transaction(
    transaction: &Transaction<'_>,
    run: RunId,
    header: &RunHeader,
    finish: &RunFinish,
    artifacts: &mut [RunArtifactWrite<'_>],
) -> Result<RunFinishReceipt, PersistenceError> {
    finish_run_in_transaction_owned(transaction, run, header, finish, artifacts, None, None)
}

fn finish_run_in_transaction_owned(
    transaction: &Transaction<'_>,
    run: RunId,
    header: &RunHeader,
    finish: &RunFinish,
    artifacts: &mut [RunArtifactWrite<'_>],
    expected_owner: Option<&ExecutionOwnerSession>,
    submitted_outputs: Option<&[ManagedOutputIdentity]>,
) -> Result<RunFinishReceipt, PersistenceError> {
    let (live_owner, live_risk) =
        execution_row(transaction, run)?.ok_or(PersistenceError::RunNotRunning)?;
    let boundary = if has_revision_pin(transaction, run)? {
        ActionRunBoundary::Admitted
    } else {
        ActionRunBoundary::Accepted
    };
    validate_running_action_state(boundary, live_risk)
        .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
    if let Some(expected_owner) = expected_owner
        && live_owner != *expected_owner
    {
        return Err(PersistenceError::InvalidRunTransition(
            "Run finalization owner does not match the persisted owner".to_owned(),
        ));
    }
    if let Some(submitted_outputs) = submitted_outputs {
        let submitted = submitted_outputs.iter().collect::<BTreeSet<_>>();
        if submitted.len() != submitted_outputs.len() {
            return Err(PersistenceError::InvalidRunArtifact(
                "duplicate submitted output identity".to_owned(),
            ));
        }
        if artifacts
            .iter()
            .any(|artifact| !submitted.contains(&artifact.output))
        {
            return Err(PersistenceError::InvalidRunArtifact(
                "Run Artifact was not submitted by the Hook".to_owned(),
            ));
        }
    }
    let trigger = terminal_consequence(finish.outcome, live_risk, finish.hook_completion.as_ref())
        .map_err(|error| PersistenceError::InvalidRunTransition(error.to_string()))?;
    if finish.outcome == RunOutcome::Succeeded
        && (finish.primary_failure.is_some()
            || finish
                .hook_completion
                .as_ref()
                .is_some_and(|completion| completion.status == HookCompletionStatus::Failure))
    {
        return Err(PersistenceError::InvalidRunTransition(
            "a succeeded Run cannot record a failure".to_owned(),
        ));
    }
    let now = unix_ms_now()?;

    transaction
        .execute(
            "DELETE FROM run_executions WHERE run_id=?1",
            [run.as_bytes().as_slice()],
        )
        .map_err(|error| PersistenceError::sqlite("remove Run execution owner", error))?;
    transaction
        .execute(
            "INSERT INTO run_outcomes(\
                run_id, outcome_rank, admitted_rank, risk_state, finished_at_unix_ms\
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                run.as_bytes().as_slice(),
                finish.outcome.rank(),
                boundary.rank(),
                live_risk.rank(),
                now,
            ],
        )
        .map_err(|error| PersistenceError::sqlite("insert Run outcome", error))?;
    if let Some(primary) = &finish.primary_failure {
        transaction
            .execute(
                "INSERT INTO run_primary_failures(\
                    run_id, error_owner, error_code, failed_step, message_utf8\
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    run.as_bytes().as_slice(),
                    primary.failure.error.owner().as_bytes(),
                    primary.failure.error.code().as_bytes(),
                    primary.step.rank(),
                    primary.failure.message.as_bytes(),
                ],
            )
            .map_err(|error| PersistenceError::sqlite("insert Run primary failure", error))?;
    }
    for (ordinal, secondary) in finish.secondary_failures.iter().enumerate() {
        transaction
            .execute(
                "INSERT INTO run_secondary_failures(\
                    run_id, ordinal, error_owner, error_code, message_utf8\
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    run.as_bytes().as_slice(),
                    i64::try_from(ordinal).expect("failure count fits i64"),
                    secondary.error.owner().as_bytes(),
                    secondary.error.code().as_bytes(),
                    secondary.message.as_bytes(),
                ],
            )
            .map_err(|error| PersistenceError::sqlite("insert Run secondary failure", error))?;
    }
    if let Some(completion) = &finish.hook_completion {
        let code = completion
            .code
            .as_ref()
            .map(HookCodeV1::as_str)
            .unwrap_or_default();
        let message = completion.message.as_deref().unwrap_or_default();
        transaction
            .execute(
                "INSERT INTO run_hook_completions(\
                    run_id, status_rank, code_present, code_utf8, message_present, message_utf8\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    run.as_bytes().as_slice(),
                    completion.status.rank(),
                    i64::from(completion.code.is_some()),
                    code.as_bytes(),
                    i64::from(completion.message.is_some()),
                    message.as_bytes(),
                ],
            )
            .map_err(|error| PersistenceError::sqlite("insert Run Hook completion", error))?;
    }
    for artifact in artifacts.iter_mut() {
        insert_artifact(transaction, run, artifact)?;
    }

    let pinned_payloads = pinned_payloads(transaction, run)?;
    transaction
        .execute(
            "DELETE FROM run_payload_pins WHERE run_id=?1",
            [run.as_bytes().as_slice()],
        )
        .map_err(|error| PersistenceError::sqlite("release Run payload pins", error))?;
    transaction
        .execute(
            "DELETE FROM run_revision_pins WHERE run_id=?1",
            [run.as_bytes().as_slice()],
        )
        .map_err(|error| PersistenceError::sqlite("release Run Revision pin", error))?;
    for payload in pinned_payloads {
        reclaim_payload_if_unreferenced(transaction, header.instance, payload)?;
    }

    let mut published_state_version = None;
    if let Some(trigger) = trigger
        && guard_row(transaction, header.instance)?.is_none()
    {
        transaction
            .execute(
                "INSERT INTO instance_recovery_guards(\
                    instance_id, run_id, trigger_rank, entered_at_unix_ms\
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    header.instance.as_bytes().as_slice(),
                    run.as_bytes().as_slice(),
                    trigger.rank(),
                    now,
                ],
            )
            .map_err(|error| PersistenceError::sqlite("publish recovery guard", error))?;
        let next = fresh_state_version()?;
        update_state_version(transaction, header.instance, next)?;
        published_state_version = Some(next);
    }
    Ok(RunFinishReceipt {
        published_state_version,
    })
}

fn insert_artifact(
    transaction: &Transaction<'_>,
    run: RunId,
    artifact: &mut RunArtifactWrite<'_>,
) -> Result<(), PersistenceError> {
    if artifact.byte_len > MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1 {
        return Err(PersistenceError::InvalidRunArtifact(
            "Run Artifact exceeds the size limit".to_owned(),
        ));
    }
    let inserted = transaction
        .execute(
            "INSERT OR IGNORE INTO run_artifacts(run_id, output_identity, byte_length) \
             VALUES (?1, ?2, ?3)",
            params![
                run.as_bytes().as_slice(),
                artifact.output.as_str().as_bytes(),
                i64::try_from(artifact.byte_len).expect("Artifact length fits i64"),
            ],
        )
        .map_err(|error| PersistenceError::sqlite("insert Run Artifact", error))?;
    if inserted == 0 {
        return Err(PersistenceError::InvalidRunArtifact(format!(
            "duplicate Artifact for output {}",
            artifact.output.as_str()
        )));
    }
    insert_chunks(
        transaction,
        &ARTIFACT_CHUNKS,
        [
            run.as_bytes().as_slice(),
            artifact.output.as_str().as_bytes(),
        ],
        artifact.reader,
        artifact.byte_len,
    )
}

fn unix_ms_now() -> Result<i64, PersistenceError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| PersistenceError::Io {
            operation: "read wall clock",
            source: std::io::Error::other(error),
        })?;
    i64::try_from(elapsed.as_millis()).map_err(|_| PersistenceError::Io {
        operation: "read wall clock",
        source: std::io::Error::other("wall clock exceeds i64 milliseconds"),
    })
}

fn run_id(bytes: Vec<u8>) -> Result<RunId, PersistenceError> {
    bytes
        .try_into()
        .map(RunId::from_bytes)
        .map_err(|_| PersistenceError::CorruptRun("invalid RunId length".to_owned()))
}

fn timestamp(value: i64) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::CorruptRun("negative timestamp".to_owned()))
}

fn run_header(database: &Connection, run: RunId) -> Result<RunHeader, PersistenceError> {
    let row = database
        .query_row(
            "SELECT instance_id, accepted_state_version, accepted_at_unix_ms \
             FROM runs WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load Run header", error))?
        .ok_or(PersistenceError::MissingRun(run))?;
    Ok(RunHeader {
        instance: instance_id(row.0)?,
        accepted_state_version: state_version(row.1)?,
        accepted_at_unix_ms: timestamp(row.2)?,
    })
}

fn invocation_row(
    database: &Connection,
    run: RunId,
) -> Result<ActionRunIdentity, PersistenceError> {
    let row = database
        .query_row(
            "SELECT package_id, revision_content_digest, action_identity \
             FROM run_action_invocations WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load Run Action invocation", error))?
        .ok_or_else(|| PersistenceError::CorruptRun("Run has no Action invocation".to_owned()))?;
    let action = String::from_utf8(row.2)
        .map_err(|_| PersistenceError::CorruptRun("ActionIdentity is not UTF-8".to_owned()))?;
    Ok(ActionRunIdentity {
        revision: revision_identity(row.0, row.1)?,
        action: ActionIdentity::parse(action)
            .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
    })
}

fn execution_row(
    database: &Connection,
    run: RunId,
) -> Result<Option<(ExecutionOwnerSession, RecoveryRiskState)>, PersistenceError> {
    database
        .query_row(
            "SELECT owner_session, risk_state FROM run_executions WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load Run execution owner", error))?
        .map(|(owner, risk)| {
            let owner = String::from_utf8(owner).map_err(|_| {
                PersistenceError::CorruptRun("owner session is not UTF-8".to_owned())
            })?;
            Ok((
                ExecutionOwnerSession::parse(owner)
                    .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
                RecoveryRiskState::from_rank(risk)
                    .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
            ))
        })
        .transpose()
}

fn has_revision_pin(database: &Connection, run: RunId) -> Result<bool, PersistenceError> {
    database
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM run_revision_pins WHERE run_id=?1)",
            [run.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| PersistenceError::sqlite("load Run Revision pin", error))
}

fn pinned_payloads(
    database: &Connection,
    run: RunId,
) -> Result<Vec<ManagedInputPayloadId>, PersistenceError> {
    let mut statement = database
        .prepare("SELECT payload_id FROM run_payload_pins WHERE run_id=?1 ORDER BY input_identity")
        .map_err(|error| PersistenceError::sqlite("prepare Run payload pins", error))?;
    let rows = statement
        .query_map([run.as_bytes().as_slice()], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| PersistenceError::sqlite("query Run payload pins", error))?;
    rows.map(|row| {
        payload_id(row.map_err(|error| PersistenceError::sqlite("read Run payload pin", error))?)
    })
    .collect()
}

fn outcome_row(database: &Connection, run: RunId) -> Result<Option<OutcomeRow>, PersistenceError> {
    database
        .query_row(
            "SELECT outcome_rank, admitted_rank, risk_state, finished_at_unix_ms \
             FROM run_outcomes WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load Run outcome", error))?
        .map(|(outcome, boundary, risk, finished)| {
            Ok(OutcomeRow {
                outcome: RunOutcome::from_rank(outcome)
                    .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
                boundary: ActionRunBoundary::from_rank(boundary)
                    .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
                terminal_risk: RecoveryRiskState::from_rank(risk)
                    .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
                finished_at_unix_ms: timestamp(finished)?,
            })
        })
        .transpose()
}

fn guard_row(
    database: &Connection,
    instance: InstanceId,
) -> Result<Option<RecoveryGuardView>, PersistenceError> {
    database
        .query_row(
            "SELECT run_id, trigger_rank, entered_at_unix_ms \
             FROM instance_recovery_guards WHERE instance_id=?1",
            [instance.as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load recovery guard", error))?
        .map(|(run, trigger, entered)| {
            Ok(RecoveryGuardView {
                instance,
                run: run_id(run)?,
                trigger: ManualRecoveryTrigger::from_rank(trigger)
                    .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
                entered_at_unix_ms: timestamp(entered)?,
            })
        })
        .transpose()
}

fn load_instance_recovery_guard_from(
    database: &Connection,
    instance: InstanceId,
) -> Result<Option<RecoveryGuardView>, PersistenceError> {
    let Some(guard) = guard_row(database, instance)? else {
        return Ok(None);
    };
    let triggering_header = run_header(database, guard.run)?;
    if triggering_header.instance != instance {
        return Err(PersistenceError::CorruptRun(
            "recovery guard references a Run from another Instance".to_owned(),
        ));
    }
    let outcome = outcome_row(database, guard.run)?.ok_or_else(|| {
        PersistenceError::CorruptRun(
            "recovery guard references a Run without an outcome".to_owned(),
        )
    })?;
    if outcome.terminal_risk != RecoveryRiskState::Open {
        return Err(PersistenceError::CorruptRun(
            "recovery guard references a Run with clear terminal risk".to_owned(),
        ));
    }
    Ok(Some(guard))
}

fn error_ref(owner: Vec<u8>, code: Vec<u8>) -> Result<PactrunErrorRefV1, PersistenceError> {
    let owner = String::from_utf8(owner)
        .map_err(|_| PersistenceError::CorruptRun("error owner is not UTF-8".to_owned()))?;
    let code = String::from_utf8(code)
        .map_err(|_| PersistenceError::CorruptRun("error code is not UTF-8".to_owned()))?;
    PactrunErrorRefV1::new(owner, code).map_err(PersistenceError::CorruptRun)
}

fn utf8_message(bytes: Vec<u8>) -> Result<String, PersistenceError> {
    String::from_utf8(bytes)
        .map_err(|_| PersistenceError::CorruptRun("message is not UTF-8".to_owned()))
}

fn load_run_from(database: &Connection, run: RunId) -> Result<RunView, PersistenceError> {
    let header = run_header(database, run)?;
    let action = invocation_row(database, run)?;
    let execution = execution_row(database, run)?;
    let outcome = outcome_row(database, run)?;
    let admitted = has_revision_pin(database, run)?;
    validate_pin_instances(database, run, header.instance)?;
    let state = match (execution, outcome) {
        (Some((owner, risk_state)), None) => {
            let boundary = if admitted {
                ActionRunBoundary::Admitted
            } else {
                ActionRunBoundary::Accepted
            };
            validate_running_action_state(boundary, risk_state)
                .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
            RunState::Running(RunExecutionView {
                owner,
                boundary,
                risk_state,
            })
        }
        (None, Some(outcome)) => {
            if admitted {
                return Err(PersistenceError::CorruptRun(
                    "finished Run still holds execution pins".to_owned(),
                ));
            }
            RunState::Finished(load_outcome_view(database, run, outcome)?)
        }
        (Some(_), Some(_)) => {
            return Err(PersistenceError::CorruptRun(
                "Run is both Running and Finished".to_owned(),
            ));
        }
        (None, None) => {
            return Err(PersistenceError::CorruptRun(
                "Run is neither Running nor Finished".to_owned(),
            ));
        }
    };
    Ok(RunView {
        id: run,
        instance: header.instance,
        accepted_state_version: header.accepted_state_version,
        accepted_at_unix_ms: header.accepted_at_unix_ms,
        action,
        state,
    })
}

fn validate_pin_instances(
    database: &Connection,
    run: RunId,
    instance: InstanceId,
) -> Result<(), PersistenceError> {
    let foreign: bool = database
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM run_payload_pins WHERE run_id=?1 AND instance_id<>?2)",
            params![run.as_bytes().as_slice(), instance.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| PersistenceError::sqlite("validate Run payload pins", error))?;
    if foreign {
        return Err(PersistenceError::CorruptRun(
            "payload pin references another Instance".to_owned(),
        ));
    }
    Ok(())
}

fn load_outcome_view(
    database: &Connection,
    run: RunId,
    outcome: OutcomeRow,
) -> Result<RunOutcomeView, PersistenceError> {
    let primary_failure = database
        .query_row(
            "SELECT error_owner, error_code, failed_step, message_utf8 \
             FROM run_primary_failures WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load Run primary failure", error))?
        .map(|(owner, code, step, message)| {
            Ok::<_, PersistenceError>(RunPrimaryFailure {
                failure: RunFailureRecord {
                    error: error_ref(owner, code)?,
                    message: utf8_message(message)?,
                },
                step: RunFailedStep::from_rank(step)
                    .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
            })
        })
        .transpose()?;

    let mut statement = database
        .prepare(
            "SELECT ordinal, error_owner, error_code, message_utf8 \
             FROM run_secondary_failures WHERE run_id=?1 ORDER BY ordinal",
        )
        .map_err(|error| PersistenceError::sqlite("prepare Run secondary failures", error))?;
    let rows = statement
        .query_map([run.as_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        })
        .map_err(|error| PersistenceError::sqlite("query Run secondary failures", error))?;
    let mut secondary_failures = Vec::new();
    let mut ordinals = Vec::new();
    for row in rows {
        let (ordinal, owner, code, message) =
            row.map_err(|error| PersistenceError::sqlite("read Run secondary failure", error))?;
        ordinals.push(ordinal);
        secondary_failures.push(RunFailureRecord {
            error: error_ref(owner, code)?,
            message: utf8_message(message)?,
        });
    }
    ensure_ordered(&ordinals, i64::cmp)?;

    let hook_completion = database
        .query_row(
            "SELECT status_rank, code_present, code_utf8, message_present, message_utf8 \
             FROM run_hook_completions WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load Run Hook completion", error))?
        .map(|(status, code_present, code, message_present, message)| {
            let code = match code_present {
                0 if code.is_empty() => None,
                1 => Some(
                    HookCodeV1::parse(utf8_message(code)?)
                        .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
                ),
                _ => {
                    return Err(PersistenceError::CorruptRun(
                        "invalid Hook completion code presence".to_owned(),
                    ));
                }
            };
            let message = match message_present {
                0 if message.is_empty() => None,
                1 => Some(utf8_message(message)?),
                _ => {
                    return Err(PersistenceError::CorruptRun(
                        "invalid Hook completion message presence".to_owned(),
                    ));
                }
            };
            Ok::<_, PersistenceError>(HookCompletionRecord {
                status: HookCompletionStatus::from_rank(status)
                    .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
                code,
                message,
            })
        })
        .transpose()?;

    let mut statement = database
        .prepare(
            "SELECT output_identity, byte_length FROM run_artifacts \
             WHERE run_id=?1 ORDER BY output_identity",
        )
        .map_err(|error| PersistenceError::sqlite("prepare Run Artifacts", error))?;
    let rows = statement
        .query_map([run.as_bytes().as_slice()], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|error| PersistenceError::sqlite("query Run Artifacts", error))?;
    let mut artifacts = Vec::new();
    for row in rows {
        let (output, byte_length) =
            row.map_err(|error| PersistenceError::sqlite("read Run Artifact", error))?;
        artifacts.push(RunArtifactSummary {
            output: ManagedOutputIdentity::parse(utf8_message(output)?)
                .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
            byte_length: u64::try_from(byte_length)
                .map_err(|_| PersistenceError::CorruptRun("negative Artifact length".to_owned()))?,
        });
    }
    ensure_ordered(&artifacts, |left, right| left.output.cmp(&right.output))?;

    if outcome.outcome == RunOutcome::Succeeded
        && (primary_failure.is_some()
            || outcome.terminal_risk == RecoveryRiskState::Open
            || hook_completion
                .as_ref()
                .is_some_and(|completion| completion.status == HookCompletionStatus::Failure))
    {
        return Err(PersistenceError::CorruptRun(
            "succeeded Run carries failure or open-risk records".to_owned(),
        ));
    }
    Ok(RunOutcomeView {
        outcome: outcome.outcome,
        boundary: outcome.boundary,
        terminal_risk: outcome.terminal_risk,
        finished_at_unix_ms: outcome.finished_at_unix_ms,
        primary_failure,
        secondary_failures,
        hook_completion,
        artifacts,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Cursor,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::mpsc,
        thread,
        time::Duration,
    };

    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;
    use crate::{
        domain::{
            ActionV1, ActiveInstanceBindingReference, ContentId, HookLaunchV1, HookV1,
            IOContractV1, InputDeclarationV1, InputIdentity, InputProtectionV1, InstanceName,
            InstanceView, MANAGED_INPUT_CHUNK_BYTES_V1, PositiveVersion,
            RevisionCoreProjectionInputV1, RevisionIdentity, RunPhase,
            RuntimeContentProjectionInputV1, RuntimeFileKindV1, RuntimeFileV1, RuntimePath,
            Sha256Digest, TerminalContractV1, ValidatedRevisionContentV1, project_revision_core_v1,
            project_runtime_content_closure_v1, validate_revision_content_v1,
        },
        managed_data::{StagingSession, session_is_live},
        persistence::ManagedInputWrite,
    };

    const TOOL_BYTES: &[u8] = b"deploy tool";

    const WORKER_TEST: &str = "persistence::sqlite_runs::tests::m3_subprocess_worker";

    fn root() -> (TempDir, PathBuf) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m3-slice2-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("runs-")
            .tempdir_in(parent)
            .unwrap();
        let root = temporary.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("database")).unwrap();
        fs::create_dir(root.join("runtime-content")).unwrap();
        fs::create_dir(root.join("staging")).unwrap();
        (temporary, root)
    }

    /// A Revision with two Inputs and one Observe Direct Action `deploy`; the
    /// Action is real so admission can read its access mode from the core.
    fn revision(persistence: &PactrunPersistence) -> RevisionIdentity {
        let blob_digest = Sha256Digest::from_bytes(Sha256::digest(TOOL_BYTES).into());
        let publication = persistence
            .put_runtime_content(&blob_digest, &mut Cursor::new(TOOL_BYTES))
            .unwrap();
        let core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
            inputs: vec![
                InputDeclarationV1 {
                    id: InputIdentity::parse("required").unwrap(),
                    required: true,
                    protection: InputProtectionV1::Normal,
                },
                InputDeclarationV1 {
                    id: InputIdentity::parse("secret").unwrap(),
                    required: false,
                    protection: InputProtectionV1::Secret,
                },
            ],
            actions: vec![ActionV1 {
                id: ActionIdentity::parse("deploy").unwrap(),
                access: OperationAccessV1::Observe,
                parameters: Vec::new(),
                hook: HookV1 {
                    protocol_version: PositiveVersion::new(1).unwrap(),
                    launch: HookLaunchV1::Direct {
                        executable: ContentId::parse("tool").unwrap(),
                    },
                    args: Vec::new(),
                    io: IOContractV1 {
                        terminal: TerminalContractV1::None,
                    },
                },
                outputs: Vec::new(),
            }],
            snapshot: None,
            migrations: Vec::new(),
            cleanup: None,
        })
        .unwrap();
        let runtime_content = project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 {
            files: vec![RuntimeFileV1 {
                id: ContentId::parse("tool").unwrap(),
                path: RuntimePath::parse("bin/tool").unwrap(),
                kind: RuntimeFileKindV1::RegularFile,
                blob_digest,
                executable: true,
            }],
        })
        .unwrap();
        let content: ValidatedRevisionContentV1 =
            validate_revision_content_v1(core, runtime_content).unwrap();
        persistence
            .persist_revision(
                crate::domain::PackageId::from_bytes([9; 16]),
                &content,
                &[publication],
            )
            .unwrap()
    }

    /// Runs the authoritative Admission transaction with facts read from the
    /// stored Revision (Direct launch, so the launcher closure is never used).
    fn admit(
        persistence: &PactrunPersistence,
        run: RunId,
        expected: InstanceStateVersion,
        active_bindings: &[ActiveInstanceBindingReference],
        override_guard: bool,
    ) -> Result<Result<(), AdmissionRefusal>, PersistenceError> {
        let view = persistence.load_run(run)?.expect("Run exists");
        let stored = persistence
            .load_revision(&view.action.revision)?
            .expect("Run Revision is persisted");
        let files = stored.content.runtime_content.files();
        let launch = CompiledHookLaunch::Direct {
            executable: files[0].clone(),
        };
        let facts = AdmissionFacts {
            expected_state_version: expected,
            active_bindings,
            runtime_content: files,
            launch: &launch,
        };
        persistence.admit_run(run, &facts, &|_| Ok(()), override_guard)
    }

    fn instance(
        persistence: &PactrunPersistence,
        revision: &RevisionIdentity,
        name: &str,
    ) -> InstanceView {
        let mut required = Cursor::new(b"required-bytes".to_vec());
        let mut secret = Cursor::new(b"secret-bytes".to_vec());
        let mut initial = [
            ManagedInputWrite {
                input_id: InputIdentity::parse("required").unwrap(),
                byte_len: 14,
                reader: &mut required,
            },
            ManagedInputWrite {
                input_id: InputIdentity::parse("secret").unwrap(),
                byte_len: 12,
                reader: &mut secret,
            },
        ];
        persistence
            .create_instance(
                InstanceName::parse(name).unwrap(),
                revision.clone(),
                &mut initial,
            )
            .unwrap()
    }

    fn bindings(
        persistence: &PactrunPersistence,
        instance: InstanceId,
    ) -> Vec<ActiveInstanceBindingReference> {
        persistence
            .observe_instance_compilation_state(instance)
            .unwrap()
            .unwrap()
            .active_bindings
    }

    fn token(persistence: &PactrunPersistence, instance: InstanceId) -> InstanceStateVersion {
        persistence
            .load_instance_by_id(instance)
            .unwrap()
            .unwrap()
            .state_version
    }

    fn owner() -> ExecutionOwnerSession {
        ExecutionOwnerSession::parse(format!("session-{}", "0".repeat(32))).unwrap()
    }

    fn action(revision: &RevisionIdentity) -> ActionRunIdentity {
        ActionRunIdentity {
            revision: revision.clone(),
            action: ActionIdentity::parse("deploy").unwrap(),
        }
    }

    fn accepted(persistence: &PactrunPersistence, view: &InstanceView) -> RunId {
        persistence
            .create_accepted_run(
                view.id,
                view.state_version,
                &action(&view.active_revision),
                &owner(),
            )
            .unwrap()
    }

    fn admitted(persistence: &PactrunPersistence, view: &InstanceView) -> RunId {
        let run = accepted(persistence, view);
        admit(
            persistence,
            run,
            view.state_version,
            &bindings(persistence, view.id),
            false,
        )
        .unwrap()
        .unwrap();
        run
    }

    fn count(persistence: &PactrunPersistence, sql: &str) -> i64 {
        persistence
            .database
            .lock()
            .unwrap()
            .query_row(sql, [], |row| row.get(0))
            .unwrap()
    }

    fn execute(
        persistence: &PactrunPersistence,
        sql: &str,
        parameters: &[&[u8]],
    ) -> rusqlite::Result<usize> {
        let database = persistence.database.lock().unwrap();
        database.execute(sql, rusqlite::params_from_iter(parameters.iter()))
    }

    fn error(owner: &str, code: &str) -> PactrunErrorRefV1 {
        PactrunErrorRefV1::new(owner, code).unwrap()
    }

    fn plain_finish(outcome: RunOutcome) -> RunFinish {
        RunFinish {
            outcome,
            primary_failure: None,
            secondary_failures: Vec::new(),
            hook_completion: None,
        }
    }

    // Supporting coverage for the Slice 5 inspection substrate. This is not
    // verification of the user-visible Run-detail requirement.
    #[test]
    fn run_inspection_uses_one_read_snapshot_for_run_and_current_guard() {
        let (_temporary, root) = root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let revision = revision(&persistence);
        let view = instance(&persistence, &revision, "inspection-snapshot");

        let historical_run = admitted(&persistence, &view);
        persistence
            .finish_run(
                historical_run,
                &plain_finish(RunOutcome::Cancelled),
                &mut [],
            )
            .unwrap();
        let guard_run = admitted(&persistence, &view);
        persistence.open_recovery_risk(guard_run).unwrap();
        persistence
            .finish_run(guard_run, &plain_finish(RunOutcome::Failed), &mut [])
            .unwrap();
        let guarded_token = token(&persistence, view.id);

        let inspection_persistence = PactrunPersistence::open(&root).unwrap();
        let (snapshot_started_tx, snapshot_started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let inspection = thread::spawn(move || {
            inspection_persistence.load_run_inspection_with_hook(historical_run, || {
                snapshot_started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
        });
        snapshot_started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("inspection acquired its read snapshot");

        let resolver_root = root.clone();
        let (mutation_started_tx, mutation_started_rx) = mpsc::channel();
        let (mutation_done_tx, mutation_done_rx) = mpsc::channel();
        let resolver = thread::spawn(move || {
            mutation_started_tx.send(()).unwrap();
            let resolver = PactrunPersistence::open(&resolver_root).unwrap();
            mutation_done_tx
                .send(
                    resolver
                        .resolve_manual_recovery(view.id, guarded_token)
                        .is_ok(),
                )
                .unwrap();
        });
        mutation_started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("recovery-guard resolver started");
        assert!(
            mutation_done_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("recovery-guard mutation committed while inspection stayed open")
        );
        release_tx.send(()).unwrap();

        let inspection = inspection.join().unwrap().unwrap().unwrap();
        resolver.join().unwrap();
        let RunState::Finished(outcome) = &inspection.run.state else {
            panic!("expected the inspected Run to be terminal");
        };
        assert_eq!(outcome.outcome, RunOutcome::Cancelled);
        assert_eq!(outcome.terminal_risk, RecoveryRiskState::Clear);
        assert_eq!(
            inspection
                .current_recovery_guard
                .as_ref()
                .expect("the old snapshot still contains the current guard")
                .run,
            guard_run
        );

        let next = persistence
            .load_run_inspection(historical_run)
            .unwrap()
            .unwrap();
        assert_eq!(next.run.state, RunState::Finished(outcome.clone()));
        assert!(next.current_recovery_guard.is_none());
    }

    // Test-ID: PR-TEST-0105
    // Verifies: PR-REQ-0275
    #[test]
    fn accepted_open_risk_corruption_fails_closed_without_mutation() {
        let (_temporary, root) = root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let revision = revision(&persistence);
        let view = instance(&persistence, &revision, "accepted-open-corrupt");
        let run = accepted(&persistence, &view);
        let stored = persistence
            .load_revision(&revision)
            .unwrap()
            .expect("revision is persisted");
        let files = stored.content.runtime_content.files();
        let launch = CompiledHookLaunch::Direct {
            executable: files[0].clone(),
        };
        let active_bindings = bindings(&persistence, view.id);
        let facts = AdmissionFacts {
            expected_state_version: view.state_version,
            active_bindings: &active_bindings,
            runtime_content: files,
            launch: &launch,
        };
        let before_version = token(&persistence, view.id);
        execute(
            &persistence,
            "UPDATE run_executions SET risk_state=1 WHERE run_id=?1",
            &[run.as_bytes().as_slice()],
        )
        .unwrap();

        assert!(matches!(
            persistence.load_run(run),
            Err(PersistenceError::CorruptRun(_))
        ));
        assert!(matches!(
            persistence.admit_run(run, &facts, &|_| Ok(()), false),
            Err(PersistenceError::CorruptRun(_))
        ));
        assert_eq!(token(&persistence, view.id), before_version);
        assert_eq!(count(&persistence, "SELECT COUNT(*) FROM run_outcomes"), 0);
        assert_eq!(
            count(
                &persistence,
                "SELECT COUNT(*) FROM instance_recovery_guards"
            ),
            0
        );
        assert_eq!(
            count(&persistence, "SELECT COUNT(*) FROM run_revision_pins"),
            0
        );
        assert_eq!(
            persistence
                .database
                .lock()
                .unwrap()
                .query_row(
                    "SELECT risk_state FROM run_executions WHERE run_id=?1",
                    [run.as_bytes().as_slice()],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
    }

    // Test-ID: PR-TEST-0112
    // Verifies: PR-REQ-0064, PR-REQ-0275
    #[test]
    fn owner_reconciliation_finishes_clear_risk_runs_by_boundary() {
        let (_temporary, root) = root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let revision = revision(&persistence);

        let accepted_view = instance(&persistence, &revision, "reconcile-accepted");
        let accepted_run = accepted(&persistence, &accepted_view);
        assert!(
            persistence
                .reconcile_action_run(accepted_run, &owner())
                .unwrap()
        );
        let accepted_outcome = finished(&persistence, accepted_run);
        assert_eq!(accepted_outcome.outcome, RunOutcome::Interrupted);
        assert_eq!(accepted_outcome.boundary, ActionRunBoundary::Accepted);
        assert_eq!(accepted_outcome.terminal_risk, RecoveryRiskState::Clear);
        assert!(
            !persistence
                .reconcile_action_run(accepted_run, &owner())
                .unwrap()
        );

        let admitted_view = instance(&persistence, &revision, "reconcile-admitted");
        let admitted_run = admitted(&persistence, &admitted_view);
        assert!(
            persistence
                .reconcile_action_run(admitted_run, &owner())
                .unwrap()
        );
        let admitted_outcome = finished(&persistence, admitted_run);
        assert_eq!(admitted_outcome.outcome, RunOutcome::Interrupted);
        assert_eq!(admitted_outcome.boundary, ActionRunBoundary::Admitted);
        assert_eq!(admitted_outcome.terminal_risk, RecoveryRiskState::Clear);
        assert_eq!(
            count(&persistence, "SELECT COUNT(*) FROM run_revision_pins"),
            0
        );
        assert_eq!(
            count(&persistence, "SELECT COUNT(*) FROM run_payload_pins"),
            0
        );
        assert!(
            persistence
                .load_instance_recovery_guard(admitted_view.id)
                .unwrap()
                .is_none()
        );
    }

    fn state(persistence: &PactrunPersistence, run: RunId) -> RunState {
        persistence.load_run(run).unwrap().unwrap().state
    }

    fn running(persistence: &PactrunPersistence, run: RunId) -> RunExecutionView {
        match state(persistence, run) {
            RunState::Running(execution) => execution,
            other => panic!("expected a Running Run, found {other:?}"),
        }
    }

    fn finished(persistence: &PactrunPersistence, run: RunId) -> RunOutcomeView {
        match state(persistence, run) {
            RunState::Finished(outcome) => outcome,
            other => panic!("expected a Finished Run, found {other:?}"),
        }
    }

    fn run_worker(
        root: &Path,
        operation: &str,
        run: RunId,
        fault: Option<FaultPoint>,
        marker: &Path,
    ) -> bool {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg(WORKER_TEST)
            .arg("--nocapture")
            .env("PACTRUN_M3_WORKER", operation)
            .env("PACTRUN_M3_ROOT", root)
            .env("PACTRUN_M3_RUN", run.to_string())
            .env("PACTRUN_M3_SUCCESS_MARKER", marker)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(fault) = fault {
            command.env("PACTRUN_M3_FAULT", fault.name());
        }
        let output = command.output().unwrap();
        if !output.status.success() && fault.is_none() {
            eprintln!(
                "M3 worker failed with {}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        output.status.success()
    }

    #[test]
    fn m3_subprocess_worker() {
        let Some(operation) = std::env::var_os("PACTRUN_M3_WORKER") else {
            return;
        };
        let root = PathBuf::from(std::env::var_os("PACTRUN_M3_ROOT").unwrap());
        let marker = PathBuf::from(std::env::var_os("PACTRUN_M3_SUCCESS_MARKER").unwrap());
        let run = std::env::var("PACTRUN_M3_RUN")
            .unwrap()
            .parse::<RunId>()
            .unwrap();
        let persistence = PactrunPersistence::open(&root).unwrap();
        match operation.to_string_lossy().as_ref() {
            "pin" => {
                let view = persistence.load_run(run).unwrap().unwrap();
                let expected = token(&persistence, view.instance);
                admit(
                    &persistence,
                    run,
                    expected,
                    &bindings(&persistence, view.instance),
                    false,
                )
                .unwrap()
                .unwrap();
            }
            "open_risk" => persistence.open_recovery_risk(run).unwrap(),
            "finish_failed" => {
                persistence
                    .finish_run(run, &plain_finish(RunOutcome::Failed), &mut [])
                    .unwrap();
            }
            other => panic!("unknown M3 worker operation {other}"),
        }
        fs::write(marker, b"m3-operation-returned-success").unwrap();
    }

    // Test-ID: PR-TEST-0083
    // Verifies: PR-REQ-0047, PR-REQ-0048
    #[test]
    fn execution_pins_are_durable_release_only_at_terminal_and_never_guard() {
        let (temporary, root) = root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let revision = revision(&persistence);
        let view = instance(&persistence, &revision, "pins");
        let stale_run = accepted(&persistence, &view);
        assert_eq!(token(&persistence, view.id), view.state_version);
        let active = bindings(&persistence, view.id);
        assert_eq!(active.len(), 2);

        // A stale expected token is refused inside the Admission transaction and
        // the refusal is that Run's durable terminal outcome; no pin exists.
        let stale = InstanceStateVersion::generate().unwrap();
        assert!(matches!(
            admit(&persistence, stale_run, stale, &active, false),
            Ok(Err(AdmissionRefusal::PlanInvalidated(_)))
        ));
        assert_eq!(
            count(&persistence, "SELECT COUNT(*) FROM run_revision_pins"),
            0
        );
        assert_eq!(
            finished(&persistence, stale_run).outcome,
            RunOutcome::Failed
        );
        let run = accepted(&persistence, &view);
        admit(&persistence, run, view.state_version, &active, false)
            .unwrap()
            .unwrap();
        assert_eq!(token(&persistence, view.id), view.state_version);
        assert!(matches!(
            admit(&persistence, run, view.state_version, &active, false),
            Err(PersistenceError::InvalidRunTransition(_))
        ));
        let execution = running(&persistence, run);
        assert_eq!(execution.boundary, ActionRunBoundary::Admitted);
        assert_eq!(execution.risk_state, RecoveryRiskState::Clear);
        assert_eq!(execution.owner, owner());
        assert_eq!(
            count(&persistence, "SELECT COUNT(*) FROM run_payload_pins"),
            2
        );

        // A pin is lifetime bookkeeping, not a guard: current-binding
        // replacement succeeds while the pinned payload stays physically
        // present and unreachable from the Instance.
        let old_secret = active
            .iter()
            .find(|binding| binding.input.as_str() == "secret")
            .unwrap()
            .payload;
        let mut replacement = Cursor::new(b"replacement".to_vec());
        let mut write = ManagedInputWrite {
            input_id: InputIdentity::parse("secret").unwrap(),
            byte_len: 11,
            reader: &mut replacement,
        };
        let replaced = persistence
            .set_input(view.id, view.state_version, &mut write)
            .unwrap();
        assert_ne!(replaced, view.state_version);
        assert_eq!(
            count(&persistence, "SELECT COUNT(*) FROM managed_input_payloads"),
            3
        );
        assert!(
            execute(
                &persistence,
                "DELETE FROM managed_input_payloads WHERE payload_id=?1",
                &[old_secret.as_bytes().as_slice()],
            )
            .is_err()
        );
        let mut current = Vec::new();
        persistence
            .export_input(
                view.id,
                &InputIdentity::parse("secret").unwrap(),
                true,
                &mut current,
            )
            .unwrap();
        assert_eq!(current, b"replacement");

        // Pins survive process failure after the pin commit and never exist
        // before it.
        let crash_view = instance(&persistence, &revision, "pins-crash");
        let before_run = accepted(&persistence, &crash_view);
        let before_marker = temporary.path().join("pins-before.success");
        assert!(!run_worker(
            &root,
            "pin",
            before_run,
            Some(FaultPoint::BeforeRunAdmitCommit),
            &before_marker,
        ));
        assert!(!before_marker.exists());
        let after_run = accepted(&persistence, &crash_view);
        let after_marker = temporary.path().join("pins-after.success");
        assert!(!run_worker(
            &root,
            "pin",
            after_run,
            Some(FaultPoint::AfterRunAdmitCommit),
            &after_marker,
        ));
        drop(persistence);
        let reopened = PactrunPersistence::open(&root).unwrap();
        assert_eq!(
            running(&reopened, before_run).boundary,
            ActionRunBoundary::Accepted
        );
        assert_eq!(
            running(&reopened, after_run).boundary,
            ActionRunBoundary::Admitted
        );
        assert_eq!(count(&reopened, "SELECT COUNT(*) FROM run_payload_pins"), 4);

        // Release happens only in the terminal transaction; the unbound
        // payload is reclaimed then, the bound one is kept, and a clear-risk
        // finish publishes no state version.
        let receipt = reopened
            .finish_run(run, &plain_finish(RunOutcome::Failed), &mut [])
            .unwrap();
        assert_eq!(receipt.published_state_version, None);
        assert_eq!(token(&reopened, view.id), replaced);
        assert_eq!(count(&reopened, "SELECT COUNT(*) FROM run_payload_pins"), 2);
        assert_eq!(
            count(
                &reopened,
                "SELECT COUNT(*) FROM managed_input_payloads p \
                 JOIN instances i ON i.instance_id = p.instance_id \
                 WHERE i.instance_name = CAST('pins' AS BLOB)",
            ),
            2
        );
        let mut current = Vec::new();
        reopened
            .export_input(
                view.id,
                &InputIdentity::parse("secret").unwrap(),
                true,
                &mut current,
            )
            .unwrap();
        assert_eq!(current, b"replacement");
        let outcome = finished(&reopened, run);
        assert_eq!(outcome.outcome, RunOutcome::Failed);
        assert_eq!(outcome.boundary, ActionRunBoundary::Admitted);
    }

    // Test-ID: PR-TEST-0084
    // Verifies: PR-REQ-0050, PR-REQ-0066, PR-REQ-0275, PR-REQ-0283
    #[test]
    fn run_phase_outcome_and_failure_records_are_exact() {
        let (_temporary, root) = root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let revision = revision(&persistence);
        let view = instance(&persistence, &revision, "outcomes");

        let accepted_run = accepted(&persistence, &view);
        assert_eq!(
            count(&persistence, "SELECT COUNT(*) FROM run_executions"),
            1
        );
        assert_eq!(count(&persistence, "SELECT COUNT(*) FROM run_outcomes"), 0);
        assert_eq!(
            running(&persistence, accepted_run).boundary,
            ActionRunBoundary::Accepted
        );
        assert!(
            execute(
                &persistence,
                "INSERT INTO run_artifacts(run_id, output_identity, byte_length) \
                 VALUES (?1, ?2, 0)",
                &[accepted_run.as_bytes().as_slice(), b"report"],
            )
            .is_err()
        );

        // An Admission failure finishes a never-admitted Run: the boundary is
        // recorded, not derived from (absent) pins.
        let admission_failure = RunFinish {
            outcome: RunOutcome::Failed,
            primary_failure: Some(RunPrimaryFailure {
                failure: RunFailureRecord {
                    error: error("admission", "plan_invalidated"),
                    message: "stale InstanceStateVersion".to_owned(),
                },
                step: RunFailedStep::Admission,
            }),
            secondary_failures: vec![RunFailureRecord {
                error: error("execution", "workspace_cleanup_failed"),
                message: String::new(),
            }],
            hook_completion: None,
        };
        persistence
            .finish_run(accepted_run, &admission_failure, &mut [])
            .unwrap();
        assert!(matches!(
            persistence.finish_run(accepted_run, &plain_finish(RunOutcome::Failed), &mut []),
            Err(PersistenceError::RunNotRunning)
        ));
        let outcome = finished(&persistence, accepted_run);
        assert_eq!(outcome.outcome, RunOutcome::Failed);
        assert_eq!(outcome.boundary, ActionRunBoundary::Accepted);
        assert_eq!(outcome.terminal_risk, RecoveryRiskState::Clear);
        assert_eq!(outcome.primary_failure, admission_failure.primary_failure);
        assert_eq!(
            outcome.secondary_failures,
            admission_failure.secondary_failures
        );
        assert_eq!(outcome.hook_completion, None);

        let completions = [
            None,
            Some(HookCompletionRecord {
                status: HookCompletionStatus::Success,
                code: None,
                message: Some(String::new()),
            }),
            Some(HookCompletionRecord {
                status: HookCompletionStatus::Success,
                code: Some(HookCodeV1::parse("report_ready").unwrap()),
                message: Some("done".to_owned()),
            }),
        ];
        let mut legacy_completions = Vec::new();
        for completion in &completions {
            let run = admitted(&persistence, &view);
            let finish = RunFinish {
                outcome: RunOutcome::Succeeded,
                primary_failure: None,
                secondary_failures: Vec::new(),
                hook_completion: completion.clone(),
            };
            persistence.finish_run(run, &finish, &mut []).unwrap();
            let outcome = finished(&persistence, run);
            assert_eq!(outcome.outcome, RunOutcome::Succeeded);
            assert_eq!(outcome.boundary, ActionRunBoundary::Admitted);
            assert_eq!(outcome.hook_completion, *completion);
            legacy_completions.push((run, completion.clone()));
        }
        let reopened = PactrunPersistence::open(&root).unwrap();
        for (run, completion) in legacy_completions {
            assert_eq!(finished(&reopened, run).hook_completion, completion);
        }
        for outcome in [
            RunOutcome::Cancelled,
            RunOutcome::TimedOut,
            RunOutcome::Interrupted,
        ] {
            let run = admitted(&persistence, &view);
            let finish = RunFinish {
                outcome,
                primary_failure: None,
                secondary_failures: Vec::new(),
                hook_completion: Some(HookCompletionRecord {
                    status: HookCompletionStatus::Failure,
                    code: Some(HookCodeV1::parse("aborted").unwrap()),
                    message: None,
                }),
            };
            persistence.finish_run(run, &finish, &mut []).unwrap();
            let view = finished(&persistence, run);
            assert_eq!(view.outcome, outcome);
            assert_eq!(view.hook_completion, finish.hook_completion);
        }

        // Success cannot carry a failure record, and ManualRecoveryRequired
        // is not an outcome rank.
        let rejected = admitted(&persistence, &view);
        assert!(matches!(
            persistence.finish_run(
                rejected,
                &RunFinish {
                    outcome: RunOutcome::Succeeded,
                    primary_failure: admission_failure.primary_failure.clone(),
                    secondary_failures: Vec::new(),
                    hook_completion: None,
                },
                &mut [],
            ),
            Err(PersistenceError::InvalidRunTransition(_))
        ));
        running(&persistence, rejected);
        assert!(
            execute(
                &persistence,
                "INSERT INTO run_outcomes(\
                    run_id, outcome_rank, admitted_rank, risk_state, finished_at_unix_ms\
                 ) VALUES (?1, 5, 1, 0, 0)",
                &[rejected.as_bytes().as_slice()],
            )
            .is_err()
        );

        let summaries = persistence.list_runs(view.id).unwrap();
        assert_eq!(summaries.len(), 8);
        assert!(summaries.windows(2).all(|pair| pair[0].id < pair[1].id));
        assert_eq!(
            summaries
                .iter()
                .filter(|summary| summary.phase == RunPhase::Running)
                .count(),
            1
        );
        assert!(
            summaries
                .iter()
                .all(|summary| summary.action.action.as_str() == "deploy")
        );

        // The V4 Run tables have no dedicated sensitive-value fields.
        let database = persistence.database.lock().unwrap();
        let mut statement = database
            .prepare(
                "SELECT t.name, c.name FROM pragma_table_list t \
                 JOIN pragma_table_info(t.name) c \
                 WHERE t.schema='main' AND (t.name LIKE 'run_%' OR t.name = 'runs' \
                    OR t.name = 'instance_recovery_guards')",
            )
            .unwrap();
        let columns = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            columns
                .iter()
                .map(|(table, _)| table.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            12
        );
        assert!(
            columns
                .iter()
                .all(|(_, column)| !column.contains("parameter") && !column.contains("secret"))
        );
        assert_eq!(
            columns
                .iter()
                .filter(|(table, _)| table == "run_action_invocations")
                .count(),
            4
        );
    }

    // Test-ID: PR-TEST-0085
    // Verifies: PR-REQ-0054, PR-REQ-0063, PR-REQ-0065, PR-REQ-0071, PR-REQ-0109
    #[test]
    fn recovery_risk_is_durable_and_terminal_consequence_is_atomic() {
        let (temporary, root) = root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let revision = revision(&persistence);
        let view = instance(&persistence, &revision, "recovery");

        let accepted_only = accepted(&persistence, &view);
        assert!(matches!(
            persistence.open_recovery_risk(accepted_only),
            Err(PersistenceError::InvalidRunTransition(_))
        ));
        let run = admitted(&persistence, &view);
        assert!(matches!(
            persistence.clear_recovery_risk(run),
            Err(PersistenceError::InvalidRunTransition(_))
        ));
        persistence.open_recovery_risk(run).unwrap();
        assert!(matches!(
            persistence.open_recovery_risk(run),
            Err(PersistenceError::InvalidRunTransition(_))
        ));
        persistence.clear_recovery_risk(run).unwrap();
        assert_eq!(token(&persistence, view.id), view.state_version);

        // Risk markers are durable exactly at their commit.
        let before_marker = temporary.path().join("risk-before.success");
        assert!(!run_worker(
            &root,
            "open_risk",
            run,
            Some(FaultPoint::BeforeRecoveryRiskCommit),
            &before_marker,
        ));
        assert_eq!(
            running(&persistence, run).risk_state,
            RecoveryRiskState::Clear
        );
        let after_marker = temporary.path().join("risk-after.success");
        assert!(!run_worker(
            &root,
            "open_risk",
            run,
            Some(FaultPoint::AfterRecoveryRiskCommit),
            &after_marker,
        ));
        assert_eq!(
            running(&persistence, run).risk_state,
            RecoveryRiskState::Open
        );

        // Open risk rejects success and keeps the Run Running.
        assert!(matches!(
            persistence.finish_run(run, &plain_finish(RunOutcome::Succeeded), &mut []),
            Err(PersistenceError::InvalidRunTransition(_))
        ));
        running(&persistence, run);

        // A clear-risk non-success publishes no guard and no state version.
        persistence.clear_recovery_risk(run).unwrap();
        let receipt = persistence
            .finish_run(run, &plain_finish(RunOutcome::Cancelled), &mut [])
            .unwrap();
        assert_eq!(receipt.published_state_version, None);
        assert_eq!(token(&persistence, view.id), view.state_version);
        assert_eq!(
            persistence.load_instance_recovery_guard(view.id).unwrap(),
            None
        );

        // An open-risk failure crash before the terminal commit leaves the Run
        // Running, no guard, and the old token.
        let open_run = admitted(&persistence, &view);
        persistence.open_recovery_risk(open_run).unwrap();
        let finish_before = temporary.path().join("finish-before.success");
        assert!(!run_worker(
            &root,
            "finish_failed",
            open_run,
            Some(FaultPoint::BeforeRunFinishCommit),
            &finish_before,
        ));
        running(&persistence, open_run);
        assert_eq!(
            persistence.load_instance_recovery_guard(view.id).unwrap(),
            None
        );
        assert_eq!(token(&persistence, view.id), view.state_version);

        // The consequence and the new state version are one commit.
        let finish_after = temporary.path().join("finish-after.success");
        assert!(!run_worker(
            &root,
            "finish_failed",
            open_run,
            Some(FaultPoint::AfterRunFinishCommit),
            &finish_after,
        ));
        drop(persistence);

        // Reopen: trigger, triggering Run, Action identity, outcome, terminal
        // risk, and diagnostics are reconstructed from durable rows alone.
        let reopened = PactrunPersistence::open(&root).unwrap();
        let guard = reopened
            .load_instance_recovery_guard(view.id)
            .unwrap()
            .expect("open-risk failure publishes a guard");
        assert_eq!(guard.instance, view.id);
        assert_eq!(guard.run, open_run);
        assert_eq!(guard.trigger, ManualRecoveryTrigger::OpenRiskFailure);
        let guarded_token = token(&reopened, view.id);
        assert_ne!(guarded_token, view.state_version);
        let triggering = reopened.load_run(guard.run).unwrap().unwrap();
        assert_eq!(triggering.action, action(&revision));
        assert_eq!(triggering.accepted_state_version, view.state_version);
        let outcome = finished(&reopened, guard.run);
        assert_eq!(outcome.outcome, RunOutcome::Failed);
        assert_eq!(outcome.terminal_risk, RecoveryRiskState::Open);
        assert_eq!(outcome.boundary, ActionRunBoundary::Admitted);
        assert!(
            execute(
                &reopened,
                "DELETE FROM runs WHERE run_id=?1",
                &[guard.run.as_bytes().as_slice()],
            )
            .is_err()
        );

        // The guard blocks ordinary admission; an override admits exactly one
        // execution, and a second open-risk finish leaves the guard unchanged.
        let blocked = reopened
            .create_accepted_run(view.id, guarded_token, &action(&revision), &owner())
            .unwrap();
        let active = bindings(&reopened, view.id);
        assert!(matches!(
            admit(&reopened, blocked, guarded_token, &active, false),
            Ok(Err(AdmissionRefusal::RecoveryGuardActive))
        ));
        assert_eq!(finished(&reopened, blocked).outcome, RunOutcome::Failed);
        let overridden = reopened
            .create_accepted_run(view.id, guarded_token, &action(&revision), &owner())
            .unwrap();
        admit(&reopened, overridden, guarded_token, &active, true)
            .unwrap()
            .unwrap();
        reopened.open_recovery_risk(overridden).unwrap();
        let receipt = reopened
            .finish_run(overridden, &plain_finish(RunOutcome::TimedOut), &mut [])
            .unwrap();
        assert_eq!(receipt.published_state_version, None);
        assert_eq!(token(&reopened, view.id), guarded_token);
        assert_eq!(
            reopened
                .load_instance_recovery_guard(view.id)
                .unwrap()
                .unwrap(),
            guard
        );

        // ResolveManualRecovery is token-first and publishes a fresh version.
        assert!(matches!(
            reopened.resolve_manual_recovery(view.id, view.state_version),
            Err(PersistenceError::StaleInstanceState)
        ));
        let resolved = reopened
            .resolve_manual_recovery(view.id, guarded_token)
            .unwrap();
        assert_ne!(resolved, guarded_token);
        assert_eq!(token(&reopened, view.id), resolved);
        assert_eq!(
            reopened.load_instance_recovery_guard(view.id).unwrap(),
            None
        );
        assert!(matches!(
            reopened.resolve_manual_recovery(view.id, resolved),
            Err(PersistenceError::MissingRecoveryGuard)
        ));

        // Owner loss and success-with-open-risk derive their own triggers.
        let loss_view = instance(&reopened, &revision, "owner-loss");
        let loss_run = admitted(&reopened, &loss_view);
        reopened.open_recovery_risk(loss_run).unwrap();
        reopened
            .finish_run(loss_run, &plain_finish(RunOutcome::Interrupted), &mut [])
            .unwrap();
        assert_eq!(
            reopened
                .load_instance_recovery_guard(loss_view.id)
                .unwrap()
                .unwrap()
                .trigger,
            ManualRecoveryTrigger::OpenRiskOwnerLoss
        );
        let success_view = instance(&reopened, &revision, "success-open");
        let success_run = admitted(&reopened, &success_view);
        reopened.open_recovery_risk(success_run).unwrap();
        reopened
            .finish_run(
                success_run,
                &RunFinish {
                    outcome: RunOutcome::Failed,
                    primary_failure: None,
                    secondary_failures: Vec::new(),
                    hook_completion: Some(HookCompletionRecord {
                        status: HookCompletionStatus::Success,
                        code: None,
                        message: None,
                    }),
                },
                &mut [],
            )
            .unwrap();
        assert_eq!(
            reopened
                .load_instance_recovery_guard(success_view.id)
                .unwrap()
                .unwrap()
                .trigger,
            ManualRecoveryTrigger::SuccessWithOpenRisk
        );
    }

    // Test-ID: PR-TEST-0086
    // Verifies: PR-REQ-0277
    #[test]
    fn run_owner_record_matches_session_lease_liveness() {
        let (_temporary, root) = root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let revision = revision(&persistence);
        let view = instance(&persistence, &revision, "owner");
        let session = StagingSession::open(&root).unwrap();
        let owner = session.owner();
        assert!(session_is_live(&root, &owner).unwrap());

        let run = persistence
            .create_accepted_run(view.id, view.state_version, &action(&revision), &owner)
            .unwrap();
        assert_eq!(running(&persistence, run).owner, owner);
        assert_eq!(
            count(&persistence, "SELECT COUNT(*) FROM run_executions"),
            1
        );

        // Another live session is not the owner and does not affect the probe;
        // a session that never existed is loss.
        let other = StagingSession::open(&root).unwrap();
        assert!(session_is_live(&root, &owner).unwrap());
        assert!(session_is_live(&root, &other.owner()).unwrap());
        drop(other);
        let never = ExecutionOwnerSession::parse(format!("session-{}", "f".repeat(32))).unwrap();
        assert!(!session_is_live(&root, &never).unwrap());

        // An unlocked lease file is loss even though the directory exists,
        // and the probe never removes it.
        let stale_name = format!("session-{}", "1".repeat(32));
        let stale = root.join("staging").join(&stale_name);
        fs::create_dir(&stale).unwrap();
        fs::write(stale.join(".lease"), b"").unwrap();
        let stale_owner = ExecutionOwnerSession::parse(stale_name).unwrap();
        assert!(!session_is_live(&root, &stale_owner).unwrap());
        assert!(stale.join(".lease").exists());

        // Dropping the owner releases the lease; the durable owner record is
        // removed only by terminal publication.
        drop(session);
        assert!(!session_is_live(&root, &owner).unwrap());
        assert_eq!(running(&persistence, run).owner, owner);
        persistence
            .finish_run(run, &plain_finish(RunOutcome::Interrupted), &mut [])
            .unwrap();
        assert_eq!(
            count(&persistence, "SELECT COUNT(*) FROM run_executions"),
            0
        );
        finished(&persistence, run);
    }

    // Test-ID: PR-TEST-0087
    // Verifies: PR-REQ-0275
    #[test]
    fn run_artifacts_are_chunked_run_owned_and_expire_independently() {
        let (_temporary, root) = root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let revision = revision(&persistence);
        let view = instance(&persistence, &revision, "artifacts");
        let report = ManagedOutputIdentity::parse("report").unwrap();
        let empty = ManagedOutputIdentity::parse("empty").unwrap();
        let report_bytes = vec![0x5a_u8; MANAGED_INPUT_CHUNK_BYTES_V1 + 3];

        let run = admitted(&persistence, &view);
        let mut report_reader = Cursor::new(report_bytes.clone());
        let mut empty_reader = Cursor::new(Vec::new());
        let mut duplicate_reader = Cursor::new(b"dup".to_vec());
        let mut artifacts = [
            RunArtifactWrite {
                output: report.clone(),
                byte_len: report_bytes.len() as u64,
                reader: &mut report_reader,
            },
            RunArtifactWrite {
                output: empty.clone(),
                byte_len: 0,
                reader: &mut empty_reader,
            },
            RunArtifactWrite {
                output: report.clone(),
                byte_len: 3,
                reader: &mut duplicate_reader,
            },
        ];
        assert!(matches!(
            persistence.finish_run(run, &plain_finish(RunOutcome::Succeeded), &mut artifacts),
            Err(PersistenceError::InvalidRunArtifact(_))
        ));
        running(&persistence, run);
        assert_eq!(count(&persistence, "SELECT COUNT(*) FROM run_artifacts"), 0);

        let mut report_reader = Cursor::new(report_bytes.clone());
        let mut empty_reader = Cursor::new(Vec::new());
        let mut artifacts = [
            RunArtifactWrite {
                output: report.clone(),
                byte_len: report_bytes.len() as u64,
                reader: &mut report_reader,
            },
            RunArtifactWrite {
                output: empty.clone(),
                byte_len: 0,
                reader: &mut empty_reader,
            },
        ];
        persistence
            .finish_run(run, &plain_finish(RunOutcome::Succeeded), &mut artifacts)
            .unwrap();
        assert_eq!(
            finished(&persistence, run).artifacts,
            vec![
                RunArtifactSummary {
                    output: empty.clone(),
                    byte_length: 0,
                },
                RunArtifactSummary {
                    output: report.clone(),
                    byte_length: report_bytes.len() as u64,
                },
            ]
        );
        assert_eq!(
            count(&persistence, "SELECT COUNT(*) FROM run_artifact_chunks"),
            2
        );
        let mut output = Vec::new();
        persistence
            .open_run_artifact(run, &report, &mut output)
            .unwrap();
        assert_eq!(output, report_bytes);
        let mut output = Vec::new();
        persistence
            .open_run_artifact(run, &empty, &mut output)
            .unwrap();
        assert!(output.is_empty());
        assert!(matches!(
            persistence.open_run_artifact(
                run,
                &ManagedOutputIdentity::parse("missing").unwrap(),
                &mut Vec::new()
            ),
            Err(PersistenceError::InvalidRunArtifact(_))
        ));

        // A failed Run retains submitted files as diagnostic Artifacts.
        let failed = admitted(&persistence, &view);
        let mut diagnostic = Cursor::new(b"partial".to_vec());
        let mut artifacts = [RunArtifactWrite {
            output: report.clone(),
            byte_len: 7,
            reader: &mut diagnostic,
        }];
        persistence
            .finish_run(failed, &plain_finish(RunOutcome::Failed), &mut artifacts)
            .unwrap();
        let mut output = Vec::new();
        persistence
            .open_run_artifact(failed, &report, &mut output)
            .unwrap();
        assert_eq!(output, b"partial");

        // Independent expiry leaves the Run record intact.
        assert!(persistence.delete_run_artifact(failed, &report).unwrap());
        assert!(!persistence.delete_run_artifact(failed, &report).unwrap());
        let outcome = finished(&persistence, failed);
        assert_eq!(outcome.outcome, RunOutcome::Failed);
        assert!(outcome.artifacts.is_empty());

        // Chunk corruption is rejected on read; Run deletion cascades.
        execute(
            &persistence,
            "DELETE FROM run_artifact_chunks WHERE run_id=?1 AND chunk_index=0",
            &[run.as_bytes().as_slice()],
        )
        .unwrap();
        assert!(matches!(
            persistence.open_run_artifact(run, &report, &mut Vec::new()),
            Err(PersistenceError::CorruptRun(_))
        ));
        execute(
            &persistence,
            "DELETE FROM runs WHERE run_id=?1",
            &[run.as_bytes().as_slice()],
        )
        .unwrap();
        assert_eq!(count(&persistence, "SELECT COUNT(*) FROM run_artifacts"), 0);
        assert_eq!(
            count(&persistence, "SELECT COUNT(*) FROM run_artifact_chunks"),
            0
        );
        assert_eq!(persistence.load_run(run).unwrap(), None);
    }
}
