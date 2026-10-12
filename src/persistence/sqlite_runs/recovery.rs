//! Durable risk transitions, operator resolution and lost-owner reconciliation.

use super::*;

impl PactrunPersistence {
    pub(crate) fn open_recovery_risk(&self, run: RunId) -> Result<(), PersistenceError> {
        self.transition_recovery_risk(run, RecoveryRiskState::Open)
    }

    pub(crate) fn clear_recovery_risk(&self, run: RunId) -> Result<(), PersistenceError> {
        self.transition_recovery_risk(run, RecoveryRiskState::Clear)
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
        self.check_write_admission(&transaction)?;
        run_header(&transaction, run)?;
        load_managed_run_from(&transaction, run)?;
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
        if next == RecoveryRiskState::Clear {
            fault(FaultPoint::BeforeRecoveryResolutionCommit);
        }
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit recovery risk transition", error))?;
        fault(FaultPoint::AfterRecoveryRiskCommit);
        if next == RecoveryRiskState::Clear {
            fault(FaultPoint::AfterRecoveryResolutionCommit);
        }
        Ok(())
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
        self.check_write_admission(&transaction)?;
        let (_, _, current) = instance_header(&transaction, instance)?;
        if current != expected {
            return Err(PersistenceError::StaleInstanceState);
        }
        super::super::sqlite_migration_runs::require_no_migration_mutator(&transaction, instance)?;
        guard_row(&transaction, instance)?.ok_or(PersistenceError::MissingRecoveryGuard)?;
        transaction
            .execute(
                "DELETE FROM instance_recovery_guards WHERE instance_id=?1",
                [instance.as_bytes().as_slice()],
            )
            .map_err(|error| PersistenceError::sqlite("clear recovery guard", error))?;
        let next = fresh_state_version()?;
        update_state_version(&transaction, instance, next)?;
        fault(FaultPoint::BeforeManualRecoveryCommit);
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit ResolveManualRecovery", error))?;
        fault(FaultPoint::AfterManualRecoveryCommit);
        Ok(next)
    }

    /// Atomically converts a confirmed-lost owner Run into Interrupted. A
    /// second reconciler simply observes that the execution row is gone.
    pub(crate) fn reconcile_action_run(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
    ) -> Result<bool, PersistenceError> {
        self.reconcile_managed_run(run, owner)
    }

    /// The application must establish confirmed lease loss before calling.
    /// This transaction rechecks the recorded owner and never replays a Hook.
    pub(crate) fn reconcile_managed_run(
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
        self.check_write_admission(&transaction)?;
        let Some((actual_owner, _)) = execution_row(&transaction, run)? else {
            match run_header(&transaction, run) {
                Ok(_) => {
                    load_managed_run_from(&transaction, run)?;
                }
                Err(PersistenceError::MissingRun(_)) => {}
                Err(error) => return Err(error),
            }
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
        super::super::sqlite_deletions::reconcile_obligation(&transaction, run)?;
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
}
