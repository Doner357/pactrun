//! Exact admitted edge reads and invocation steps; no path re-resolution.
use super::*;
use std::io::Write;

impl PactrunPersistence {
    pub(crate) fn prepare_migration_edge(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
        edge: usize,
        operators: &BTreeSet<MigrationTargetInput>,
    ) -> Result<(MigrationEdgePlan, Vec<InputIdentity>, Vec<InputIdentity>), PersistenceError> {
        owner_matches(self, owner)?;
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("read pinned Migration edge", e))?;
        let view = load_managed_run_from(&tx, run)?;
        let progress = progress_view(&tx, run)?.ok_or_else(corrupt)?;
        let (_, active, version) = instance_header(&tx, view.instance)?;
        if active != progress.boundary_revision || version != progress.boundary_state_version {
            return Err(rejected(
                "Migration source boundary changed before Session materialization",
            ));
        }
        if !matches!(&view.state, RunState::Running(e) if e.owner == *owner && e.boundary == ActionRunBoundary::Admitted)
            || progress.committed_edges != edge
        {
            return Err(rejected("Migration edge is not owned and admitted"));
        }
        let ManagedRunIdentity::Migration(invocation) = &view.operation else {
            return Err(corrupt());
        };
        let source = load_revision_from(&tx, &invocation.path()[edge])?.ok_or_else(corrupt)?;
        let target = load_revision_from(&tx, &invocation.path()[edge + 1])?.ok_or_else(corrupt)?;
        let source_inputs = source
            .content
            .core
            .inputs()
            .iter()
            .map(|i| i.id.clone())
            .collect();
        let target_inputs = target
            .content
            .core
            .inputs()
            .iter()
            .map(|i| i.id.clone())
            .collect();
        let bindings = registry(&tx, view.instance)?;
        let plan = evaluate_migration_edge(
            &MigrationRevision {
                identity: invocation.path()[edge].clone(),
                content: source.content,
            },
            &MigrationRevision {
                identity: invocation.path()[edge + 1].clone(),
                content: target.content,
            },
            &bindings,
            operators,
            invocation.authorized(),
        )
        .map_err(|_| rejected("Migration edge requirements changed"))?;
        Ok((plan, source_inputs, target_inputs))
    }

    pub(crate) fn mark_migration_step(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
        edge: usize,
        step: MigrationPlanStep,
    ) -> Result<(), PersistenceError> {
        owner_matches(self, owner)?;
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin Migration invocation step", e))?;
        self.check_write_admission(&tx)?;
        let view = load_managed_run_from(&tx, run)?;
        let progress = progress_view(&tx, run)?.ok_or_else(corrupt)?;
        if !matches!(&view.state, RunState::Running(e) if e.owner == *owner && e.boundary == ActionRunBoundary::Admitted)
            || progress.committed_edges != edge
            || step == MigrationPlanStep::Finalize
        {
            return Err(rejected("Migration invocation step changed"));
        }
        tx.execute(
            "UPDATE run_migration_progress SET step_rank=?2 WHERE run_id=?1",
            params![run.as_bytes().as_slice(), step.rank()],
        )
        .map_err(|e| PersistenceError::sqlite("record Migration step", e))?;
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit Migration step", e))
    }

    pub(crate) fn copy_migration_payload(
        &self,
        run: RunId,
        payload: ManagedInputPayloadId,
        output: &mut dyn Write,
    ) -> Result<(), PersistenceError> {
        let db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let view = load_managed_run_from(&db, run)?;
        let pinned: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM run_migration_payload_pins WHERE run_id=?1 AND instance_id=?2 AND payload_id=?3)",
            params![run.as_bytes().as_slice(), view.instance.as_bytes().as_slice(), payload.as_bytes().as_slice()], |r| r.get(0))
            .map_err(|e| PersistenceError::sqlite("read Migration payload pin", e))?;
        if !pinned {
            return Err(rejected("Migration payload is not pinned"));
        }
        stream_payload(&db, &self.runtime_content, view.instance, payload, &mut {
            output
        })
    }

    pub(crate) fn copy_migration_runtime(
        &self,
        run: RunId,
        revision: &RevisionIdentity,
        file: &RuntimeFileV1,
        output: &mut dyn Write,
    ) -> Result<(), PersistenceError> {
        {
            let db = self
                .database
                .lock()
                .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
            let pinned: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM run_migration_revision_pins WHERE run_id=?1 AND package_id=?2 AND revision_content_digest=?3)",
                params![run.as_bytes().as_slice(), revision.package_id.as_bytes().as_slice(), revision.content_digest.as_bytes().as_slice()], |r| r.get(0))
                .map_err(|e| PersistenceError::sqlite("read Migration runtime pin", e))?;
            if !pinned
                || !load_revision_from(&db, revision)?
                    .ok_or_else(corrupt)?
                    .content
                    .runtime_content
                    .files()
                    .contains(file)
            {
                return Err(rejected("Migration runtime file is not pinned"));
            }
        }
        let mut source = self.runtime_content.open_verified(&file.blob_digest)?;
        std::io::copy(&mut source, output)
            .map_err(|_| rejected("Migration runtime materialization failed"))?;
        Ok(())
    }
}
