//! Atomic Run terminal publication and managed recovery consequences.

use super::*;

impl PactrunPersistence {
    pub(crate) fn finish_migration_with_outputs(
        &self,
        owner: &ExecutionOwnerSession,
        run: RunId,
        finish: &RunFinish,
        edge: Option<usize>,
        outputs: RunArtifactSubmission<'_, '_>,
    ) -> Result<RunFinishReceipt, PersistenceError> {
        if finish.outcome == RunOutcome::Succeeded
            || self
                .staging_session()
                .is_none_or(|session| session.owner() != *owner)
        {
            return Err(PersistenceError::InvalidRunTransition(
                "Migration diagnostic publication requires its non-success owner".into(),
            ));
        }
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin Migration diagnostic publication", e))?;
        self.check_write_admission(&tx)?;
        let header = run_header(&tx, run)?;
        if !matches!(
            managed_invocation_row(&tx, run)?,
            ManagedRunIdentity::Migration(_)
        ) {
            return Err(PersistenceError::InvalidRunTransition(
                "diagnostic edge requires a Migration Run".into(),
            ));
        }
        if let Some(edge) = edge {
            let current: Option<i64> = tx
                .query_row(
                    "SELECT committed_edge_count FROM run_migration_progress WHERE run_id=?1",
                    [run.as_bytes().as_slice()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| PersistenceError::sqlite("qualify diagnostic edge", e))?;
            if current
                != Some(i64::try_from(edge).map_err(|_| {
                    PersistenceError::InvalidRunArtifact("invalid diagnostic edge".into())
                })?)
            {
                return Err(PersistenceError::InvalidRunArtifact(
                    "diagnostic edge is not the current uncommitted edge".into(),
                ));
            }
        } else if !outputs.submitted.is_empty() || !outputs.artifacts.is_empty() {
            return Err(PersistenceError::InvalidRunArtifact(
                "Migration artifacts require an edge".into(),
            ));
        }
        let receipt = finish_run_in_transaction_owned(
            &tx,
            run,
            &header,
            finish,
            &mut [],
            Some(owner),
            Some(&[]),
        )?;
        if let Some(edge) = edge {
            super::scoped_artifacts::insert_scoped_artifacts(
                &tx,
                run,
                edge as i64,
                outputs,
                false,
            )?;
        }
        fault(FaultPoint::BeforeRunFinishCommit);
        tx.commit().map_err(|e| {
            PersistenceError::sqlite("commit Migration diagnostics and terminal state", e)
        })?;
        fault(FaultPoint::AfterRunFinishCommit);
        Ok(receipt)
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
        self.check_write_admission(&transaction)?;
        let header = run_header(&transaction, run)?;
        if matches!(
            managed_invocation_row(&transaction, run)?,
            ManagedRunIdentity::Migration(_)
        ) {
            return Err(PersistenceError::InvalidRunTransition(
                "Migration finish requires its owner or confirmed-loss reconciliation".to_owned(),
            ));
        }
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
        self.check_write_admission(&transaction)?;
        let header = run_header(&transaction, run)?;
        if matches!(
            managed_invocation_row(&transaction, run)?,
            ManagedRunIdentity::Migration(_)
        ) && self.staging_session().is_none_or(|s| s.owner() != *owner)
        {
            return Err(PersistenceError::InvalidRunTransition(
                "Migration finish belongs to another owner".to_owned(),
            ));
        }
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
}

pub(in crate::persistence) fn finish_deletion_refusal(
    transaction: &Transaction<'_>,
    run: RunId,
    refusal: &AdmissionRefusal,
) -> Result<(), PersistenceError> {
    let header = run_header(transaction, run)?;
    let finish = RunFinish {
        outcome: RunOutcome::Failed,
        primary_failure: Some(refusal.primary_failure()),
        secondary_failures: Vec::new(),
        hook_completion: None,
    };
    finish_run_in_transaction(transaction, run, &header, &finish, &mut [])?;
    Ok(())
}

pub(in crate::persistence) fn finish_deletion_in_transaction(
    transaction: &Transaction<'_>,
    run: RunId,
    owner: &ExecutionOwnerSession,
    finish: &RunFinish,
    retirement: bool,
) -> Result<(), PersistenceError> {
    let header = run_header(transaction, run)?;
    if !matches!(
        managed_invocation_row(transaction, run)?,
        ManagedRunIdentity::Deletion { .. }
    ) {
        return Err(PersistenceError::InvalidRunTransition(
            "not a deletion Run".to_owned(),
        ));
    }
    finish_run_authorized(
        transaction,
        run,
        &header,
        finish,
        &mut [],
        FinishAuthority {
            owner: Some(owner),
            outputs: Some(&[]),
            capture: None,
            restore: false,
            migration: false,
            deletion: retirement,
        },
    )?;
    Ok(())
}

pub(super) fn finish_run_in_transaction(
    transaction: &Transaction<'_>,
    run: RunId,
    header: &RunHeader,
    finish: &RunFinish,
    artifacts: &mut [RunArtifactWrite<'_>],
) -> Result<RunFinishReceipt, PersistenceError> {
    finish_run_in_transaction_owned(transaction, run, header, finish, artifacts, None, None)
}

pub(super) fn finish_run_in_transaction_owned(
    transaction: &Transaction<'_>,
    run: RunId,
    header: &RunHeader,
    finish: &RunFinish,
    artifacts: &mut [RunArtifactWrite<'_>],
    expected_owner: Option<&ExecutionOwnerSession>,
    submitted_outputs: Option<&[ManagedOutputIdentity]>,
) -> Result<RunFinishReceipt, PersistenceError> {
    finish_run_authorized(
        transaction,
        run,
        header,
        finish,
        artifacts,
        FinishAuthority {
            owner: expected_owner,
            outputs: submitted_outputs,
            capture: None,
            restore: false,
            migration: false,
            deletion: false,
        },
    )
}

/// Publishes a terminal outcome inside an existing transaction: removes the
/// execution owner, records the outcome and its records, releases pins,
/// reclaims unreferenced payloads, and publishes the guard and a fresh state
/// version only when a consequence exists.
pub(super) fn finish_run_authorized(
    transaction: &Transaction<'_>,
    run: RunId,
    header: &RunHeader,
    finish: &RunFinish,
    artifacts: &mut [RunArtifactWrite<'_>],
    mut authority: FinishAuthority<'_, '_>,
) -> Result<RunFinishReceipt, PersistenceError> {
    let expected_owner = authority.owner;
    let submitted_outputs = authority.outputs;
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
    let operation = load_managed_run_from(transaction, run)?.operation;
    if let Some(primary) = &finish.primary_failure {
        let expected = RunFailedStep::for_operation(primary.step.rank(), operation.kind())
            .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
        if primary.step != expected {
            return Err(PersistenceError::InvalidRunTransition(
                "failed step belongs to another operation kind".to_owned(),
            ));
        }
    }
    if !matches!(operation, ManagedRunIdentity::Action(_)) {
        // This shared finalizer can fail/interrupt Snapshot operations, but
        // cannot stand in for S5/S6's atomic operation-specific result publisher.
        if finish.outcome == RunOutcome::Succeeded
            && authority.capture.is_none()
            && !authority.restore
            && !authority.migration
            && !authority.deletion
        {
            return Err(PersistenceError::InvalidRunTransition(
                "managed success requires atomic operation-specific result publication".to_owned(),
            ));
        }
        if !artifacts.is_empty() || submitted_outputs.is_some_and(|ids| !ids.is_empty()) {
            use crate::domain::ServiceHookSite;
            let site =
                match &operation {
                    ManagedRunIdentity::Capture { .. } => ServiceHookSite::Capture,
                    ManagedRunIdentity::Restore { .. } => ServiceHookSite::Restore,
                    _ => return Err(PersistenceError::InvalidRunArtifact(
                        "Cleanup and Migration artifacts require their lifecycle publication scope"
                            .to_owned(),
                    )),
                };
            let contract = super::super::sqlite_instances::load_revision_contract(
                transaction,
                operation.revision(),
            )?;
            if contract.operation().is_none()
                || expected_owner.is_none()
                || finish.hook_completion.is_none()
            {
                return Err(PersistenceError::InvalidRunArtifact(
                    "ordinary artifacts require an owned operation-scoped Hook completion"
                        .to_owned(),
                ));
            }
            let declared = contract.ordinary_outputs(&site).ok_or_else(|| {
                PersistenceError::InvalidRunArtifact(
                    "operation output declaration is unavailable".to_owned(),
                )
            })?;
            let submitted = submitted_outputs.ok_or_else(|| {
                PersistenceError::InvalidRunArtifact(
                    "output submission authority is unavailable".to_owned(),
                )
            })?;
            if submitted
                .iter()
                .any(|id| !declared.iter().any(|output| &output.id == id))
                || ((!artifacts.is_empty() || finish.outcome == RunOutcome::Succeeded)
                    && artifacts.len() != submitted.len())
            {
                return Err(PersistenceError::InvalidRunArtifact(
                    "ordinary output publication must match the complete declared submitted set"
                        .to_owned(),
                ));
            }
        }
    }
    if authority.capture.is_some()
        && (!matches!(operation, ManagedRunIdentity::Capture { .. })
            || finish.outcome != RunOutcome::Succeeded
            || boundary != ActionRunBoundary::Admitted
            || expected_owner.is_none()
            || finish.hook_completion.as_ref().map(|c| c.status)
                != Some(HookCompletionStatus::Success))
    {
        return Err(PersistenceError::InvalidRunTransition(
            "Capture publication requires an owned admitted successful completion".to_owned(),
        ));
    }
    if authority.deletion {
        let receipt: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM instance_retirement_receipts WHERE instance_id=?1 AND retirement_run_id=?2)",
            params![header.instance.as_bytes().as_slice(),run.as_bytes().as_slice()], |r| r.get(0),
        ).map_err(|e| PersistenceError::sqlite("qualify deletion terminal authority", e))?;
        if !receipt
            || !matches!(operation, ManagedRunIdentity::Deletion { .. })
            || finish.outcome != RunOutcome::Succeeded
            || boundary != ActionRunBoundary::Admitted
            || expected_owner.is_none()
            || live_risk != RecoveryRiskState::Clear
        {
            return Err(PersistenceError::InvalidRunTransition(
                "deletion success requires owned atomic retirement".to_owned(),
            ));
        }
    }
    if authority.restore
        && (!matches!(operation, ManagedRunIdentity::Restore { .. })
            || finish.outcome != RunOutcome::Succeeded
            || boundary != ActionRunBoundary::Admitted
            || expected_owner.is_none()
            || finish.hook_completion.as_ref().map(|c| c.status)
                != Some(HookCompletionStatus::Success))
    {
        return Err(PersistenceError::InvalidRunTransition(
            "Restore publication requires an owned admitted successful completion".to_owned(),
        ));
    }
    if authority.migration
        && (!matches!(operation, ManagedRunIdentity::Migration(_))
            || finish.outcome != RunOutcome::Succeeded
            || boundary != ActionRunBoundary::Admitted
            || expected_owner.is_none()
            || live_risk != RecoveryRiskState::Clear)
    {
        return Err(PersistenceError::InvalidRunTransition(
            "Migration success requires owned admitted atomic publication".to_owned(),
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

    if let Some(publication) = authority.capture.as_mut() {
        validate_capture_publication(transaction, run, header, operation.revision(), publication)?;
        super::super::sqlite_snapshots::insert_snapshot_manifest(
            transaction,
            publication.manifest,
        )?;
        for file in &publication.files {
            if !publication.content.owns_publication(file) {
                return Err(PersistenceError::CorruptSnapshot(
                    "foreign data publication",
                ));
            }
            super::super::sqlite_snapshots::insert_snapshot_file(
                transaction,
                publication.manifest.manifest().snapshot_id(),
                file,
            )?;
        }
    }

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
    if let Some(publication) = authority.capture.as_ref() {
        transaction
            .execute(
                "INSERT INTO run_capture_results(run_id,snapshot_id) VALUES (?1,?2)",
                params![
                    run.as_bytes().as_slice(),
                    publication
                        .manifest
                        .manifest()
                        .snapshot_id()
                        .as_bytes()
                        .as_slice()
                ],
            )
            .map_err(|e| PersistenceError::sqlite("publish Capture result reference", e))?;
    }
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
        if let Some(crate::domain::RunFailureCause::MissingRequiredInputs(ids)) = &primary.cause {
            if ids.is_empty()
                || primary.failure.error.owner() != "admission"
                || primary.failure.error.code() != "plan_invalidated"
                || primary.step != RunFailedStep::Admission
            {
                return Err(PersistenceError::CorruptRun(
                    "invalid missing-Input cause".into(),
                ));
            }
            for (ordinal, id) in ids.iter().enumerate() {
                transaction.execute("INSERT INTO run_missing_input_causes(run_id,ordinal,input_id) VALUES (?1,?2,?3)", params![run.as_bytes().as_slice(), ordinal as i64, id.as_str().as_bytes()])
                    .map_err(|error| PersistenceError::sqlite("insert Run missing-Input cause", error))?;
            }
        }
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
    if matches!(operation, ManagedRunIdentity::Action(_)) {
        for artifact in artifacts.iter_mut() {
            insert_artifact(transaction, run, artifact)?;
        }
    } else if !artifacts.is_empty() || submitted_outputs.is_some_and(|outputs| !outputs.is_empty())
    {
        super::scoped_artifacts::insert_scoped_artifacts(
            transaction,
            run,
            -1,
            RunArtifactSubmission {
                submitted: submitted_outputs.unwrap_or_default(),
                artifacts,
            },
            finish.outcome == RunOutcome::Succeeded,
        )?;
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

    if matches!(operation, ManagedRunIdentity::Migration(_)) {
        super::super::sqlite_migration_runs::release_references(transaction, run, header.instance)?;
    }
    let mut published_state_version = None;
    if trigger.is_some() {
        super::super::writer_admission::advance_consequence_version(transaction, header.instance)?;
    }
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

pub(in crate::persistence) fn finish_migration_in_transaction(
    transaction: &Transaction<'_>,
    run: RunId,
    owner: &ExecutionOwnerSession,
    finish: &RunFinish,
    publishing_success: bool,
) -> Result<RunFinishReceipt, PersistenceError> {
    let header = run_header(transaction, run)?;
    if !matches!(
        managed_invocation_row(transaction, run)?,
        ManagedRunIdentity::Migration(_)
    ) {
        return Err(PersistenceError::InvalidRunTransition(
            "not a Migration Run".to_owned(),
        ));
    }
    finish_run_authorized(
        transaction,
        run,
        &header,
        finish,
        &mut [],
        FinishAuthority {
            owner: Some(owner),
            outputs: Some(&[]),
            capture: None,
            restore: false,
            migration: publishing_success,
            deletion: false,
        },
    )
}
