//! Read-only Run queries and consistent inspection snapshots.

use super::*;

impl PactrunPersistence {
    pub(crate) fn list_managed_runs(
        &self,
        instance: InstanceId,
    ) -> Result<Vec<crate::domain::ManagedRunView>, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("begin managed Run enumeration", e))?;
        let mut statement = tx
            .prepare("SELECT run_id FROM runs WHERE instance_id=?1 ORDER BY run_id")
            .map_err(|e| PersistenceError::sqlite("enumerate managed Runs", e))?;
        let ids = statement
            .query_map([instance.as_bytes().as_slice()], |row| {
                row.get::<_, Vec<u8>>(0)
            })
            .map_err(|e| PersistenceError::sqlite("enumerate managed Runs", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PersistenceError::sqlite("read managed Run identity", e))?;
        ids.into_iter()
            .map(|id| load_managed_run_from(&tx, run_id(id)?))
            .collect()
    }

    pub(crate) fn managed_run_inspection(
        &self,
        run: RunId,
    ) -> Result<Option<crate::domain::ManagedRunInspectionData>, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("begin managed Run inspection", e))?;
        let view = match load_managed_run_from(&tx, run) {
            Ok(view) => view,
            Err(PersistenceError::MissingRun(_)) => return Ok(None),
            Err(e) => return Err(e),
        };
        inspect_run_from(&tx, view).map(Some)
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

    pub(crate) fn load_managed_run(
        &self,
        run: RunId,
    ) -> Result<Option<crate::domain::ManagedRunView>, PersistenceError> {
        let mut database = self.open_read_connection()?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| PersistenceError::sqlite("begin managed Run inspection", error))?;
        match run_header(&transaction, run) {
            Ok(_) => load_managed_run_from(&transaction, run).map(Some),
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

    pub(super) fn load_run_inspection_with_hook(
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
        self.list_running_managed_runs()
    }

    pub(crate) fn list_running_managed_runs(
        &self,
    ) -> Result<Vec<(InstanceId, RunId, ExecutionOwnerSession)>, PersistenceError> {
        let mut connection = self.open_read_connection()?;
        let database = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| {
                PersistenceError::sqlite("begin managed owner enumeration snapshot", error)
            })?;
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
            load_managed_run_from(&database, run)?;
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
}
