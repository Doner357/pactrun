//! Typed lifecycle evidence and operation-specific admission. No filesystem
//! destruction is authorized by a missing row or by an ordinary recovery flag.

use super::sqlite_instances::{fresh_state_version, instance_header, load_instance_view_from};
use super::sqlite_revision_store::load_revision_from;
use super::{PactrunPersistence, PersistenceError};
use crate::domain::*;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

fn invalid(message: &'static str) -> PersistenceError {
    PersistenceError::InvalidRunTransition(message.to_owned())
}
fn corrupt() -> PersistenceError {
    PersistenceError::CorruptRun("inconsistent deletion lifecycle evidence".to_owned())
}
fn run_id(bytes: Vec<u8>) -> Result<RunId, PersistenceError> {
    bytes
        .try_into()
        .map(RunId::from_bytes)
        .map_err(|_| corrupt())
}
fn now() -> Result<i64, PersistenceError> {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| invalid("system time precedes the Unix epoch"))?;
    i64::try_from(duration.as_millis()).map_err(|_| invalid("timestamp is out of range"))
}

fn physical_key(bytes: Option<Vec<u8>>) -> Result<Option<[u8; 40]>, PersistenceError> {
    bytes
        .map(|bytes| {
            let key: [u8; 40] = bytes.try_into().map_err(|_| corrupt())?;
            if !matches!(
                u64::from_le_bytes(key[..8].try_into().expect("fixed prefix")),
                1..=4
            ) {
                return Err(corrupt());
            }
            match key[0] {
                1 | 4 if key[32..] != [0; 8] => return Err(corrupt()),
                2 | 3
                    if u64::from_le_bytes(key[32..].try_into().expect("fixed nanoseconds"))
                        >= 1_000_000_000 =>
                {
                    return Err(corrupt());
                }
                _ => {}
            }
            Ok(key)
        })
        .transpose()
}

fn owned_deletion(
    tx: &Transaction<'_>,
    run: RunId,
    owner: &ExecutionOwnerSession,
) -> Result<ManagedRunView, PersistenceError> {
    let view = super::sqlite_runs::load_managed_run_from(tx, run)?;
    if !matches!(&view.state, RunState::Running(e) if e.owner == *owner && e.boundary == ActionRunBoundary::Admitted)
        || !matches!(&view.operation, ManagedRunIdentity::Deletion { .. })
    {
        return Err(invalid("operation requires its admitted deletion owner"));
    }
    Ok(view)
}

#[cfg(test)]
#[path = "sqlite_deletion_tests.rs"]
mod tests;

pub(super) fn obligation_from(
    database: &Connection,
    instance: InstanceId,
) -> Result<Option<DeletionObligation>, PersistenceError> {
    let row: Option<(Vec<u8>, i64)> = database
        .query_row(
            "SELECT attempt_run_id,phase FROM instance_deletion_obligations WHERE instance_id=?1",
            [instance.as_bytes().as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| PersistenceError::sqlite("read deletion obligation", e))?;
    let Some((attempt, phase)) = row else {
        return Ok(None);
    };
    let attempt = run_id(attempt)?;
    let phase = DeletionPhase::from_rank(phase).map_err(|_| corrupt())?;
    let (owner, mode): (Vec<u8>, i64) = database.query_row(
        "SELECT r.instance_id,d.deletion_mode FROM runs r JOIN run_deletion_invocations d ON d.run_id=r.run_id JOIN run_operation_kinds k ON k.run_id=r.run_id WHERE r.run_id=?1 AND k.operation_kind=4",
        [attempt.as_bytes().as_slice()], |r| Ok((r.get(0)?,r.get(1)?)),
    ).map_err(|_| corrupt())?;
    if owner != instance.as_bytes() || mode != DeletionMode::ManagedCleanup.rank() {
        return Err(corrupt());
    }
    let authority: Option<i64> = database.query_row(
        "SELECT authority_source FROM deletion_finalization_authorizations WHERE attempt_run_id=?1",
        [attempt.as_bytes().as_slice()], |r| r.get(0),
    ).optional().map_err(|e| PersistenceError::sqlite("read finalization authority", e))?;
    if authority.is_some() != (phase == DeletionPhase::FinalizationAuthorized) {
        return Err(corrupt());
    }
    if let Some(rank) = authority {
        FinalizationAuthority::from_rank(rank).map_err(|_| corrupt())?;
        let alien: bool = database.query_row(
            "SELECT EXISTS(SELECT 1 FROM deletion_finalization_allocations f LEFT JOIN service_storage_allocations a ON a.allocation_id=f.allocation_id WHERE f.attempt_run_id=?1 AND (a.instance_id IS NULL OR a.instance_id<>?2 OR NOT EXISTS(SELECT 1 FROM service_storage_protections p WHERE p.allocation_id=a.allocation_id)))",
            params![attempt.as_bytes().as_slice(),instance.as_bytes().as_slice()], |r| r.get(0),
        ).map_err(|_| corrupt())?;
        if alien {
            return Err(corrupt());
        }
    }
    Ok(Some(DeletionObligation {
        instance,
        attempt,
        phase,
    }))
}

pub(super) fn require_ordinary_lifecycle(
    database: &Connection,
    instance: InstanceId,
) -> Result<(), PersistenceError> {
    match require_no_deletion_obligation(obligation_from(database, instance)?) {
        Ok(()) => Ok(()),
        Err(DeletionError::FinalizationPending) => Err(invalid(
            "Instance has pending deletion finalization; retry deletion or abandon management",
        )),
        Err(_) => Err(invalid(
            "Cleanup result is unresolved; confirm externally completed cleanup or abandon management",
        )),
    }
}

pub(super) fn finalization_references_allocation(
    database: &Connection,
    allocation: ServiceAllocationId,
) -> Result<bool, PersistenceError> {
    database
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM deletion_finalization_allocations WHERE allocation_id=?1)",
            [allocation.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .map_err(|e| PersistenceError::sqlite("qualify finalization custody reference", e))
}

pub(super) fn reconcile_obligation(
    transaction: &Transaction<'_>,
    run: RunId,
) -> Result<(), PersistenceError> {
    transaction
        .execute(
            "UPDATE instance_deletion_obligations SET phase=1 WHERE attempt_run_id=?1 AND phase=0",
            [run.as_bytes().as_slice()],
        )
        .map_err(|e| PersistenceError::sqlite("retain unresolved Cleanup evidence", e))?;
    Ok(())
}

impl PactrunPersistence {
    pub(crate) fn list_detached_allocations(
        &self,
    ) -> Result<Vec<DetachedAllocationView>, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("inspect detached allocations", e))?;
        let mut query = tx
            .prepare(
                "SELECT allocation_id FROM detached_service_allocations ORDER BY allocation_id",
            )
            .map_err(|e| PersistenceError::sqlite("list detached identities", e))?;
        let ids = query
            .query_map([], |r| r.get::<_, Vec<u8>>(0))
            .map_err(|e| PersistenceError::sqlite("read detached identities", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PersistenceError::sqlite("read detached identity", e))?;
        ids.into_iter()
            .map(|id| {
                let id = ServiceAllocationId::from_bytes(id.try_into().map_err(|_| corrupt())?);
                detached_from(&tx, id)?.ok_or_else(corrupt)
            })
            .collect()
    }

    pub(crate) fn detached_allocation(
        &self,
        id: ServiceAllocationId,
    ) -> Result<Option<DetachedAllocationView>, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("inspect detached allocation", e))?;
        detached_from(&tx, id)
    }

    /// Explicit irreversible management operation; no Hook and no Instance Run.
    /// Intent survives I/O failure. The second immediate transaction is the
    /// maintenance exclusion and prevents concurrent physical discard attempts.
    pub(crate) fn discard_detached_allocation(
        &self,
        id: ServiceAllocationId,
        confirmed: bool,
    ) -> Result<bool, PersistenceError> {
        if !confirmed {
            return Err(invalid(
                "discard requires explicit destructive confirmation",
            ));
        }
        let root_path = self
            .database_path
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(corrupt)?;
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin explicit discard intent", e))?;
        self.check_write_admission(&tx)?;
        let view =
            detached_from(&tx, id)?.ok_or_else(|| invalid("detached allocation not found"))?;
        if view.state == DetachedAllocationState::Discarded {
            return Ok(false);
        }
        require_discard_unreferenced(&tx, id)?;
        let saved = detached_physical_key(&tx, id)?;
        let root =
            crate::retirement_fs::open_qualified(root_path, id, saved.as_ref()).map_err(|_| {
                PersistenceError::ServiceStorageUnavailable(
                    "detached allocation cannot be safely qualified",
                )
            })?;
        let key = match &root {
            Some(root) => {
                if saved.as_ref().is_some_and(|saved| {
                    !crate::retirement_fs::same_incarnation(saved, root.identity())
                }) {
                    return Err(PersistenceError::ServiceStorageUnavailable(
                        "detached allocation root was replaced",
                    ));
                }
                Some(*root.identity())
            }
            None => saved,
        };
        tx.execute("INSERT INTO allocation_discard_receipts(allocation_id,authorized_at_unix_ms,finished,root_identity) VALUES(?1,?2,0,?3) ON CONFLICT(allocation_id) DO UPDATE SET root_identity=COALESCE(excluded.root_identity,root_identity)",
            params![id.as_bytes().as_slice(),now()?,key.as_ref().map(|key| key.as_slice())])
            .map_err(|e| PersistenceError::sqlite("record discard authority", e))?;
        // An observed absence has no remaining I/O duty. Publish completion in
        // this transaction, so a crash cannot make a retry adopt a new object.
        if root.is_none() {
            tx.execute(
                "UPDATE allocation_discard_receipts SET finished=1 WHERE allocation_id=?1",
                [id.as_bytes().as_slice()],
            )
            .map_err(|e| PersistenceError::sqlite("record absent discard target", e))?;
        }
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit explicit discard intent", e))?;
        super::sqlite_revision_store::fault(super::FaultPoint::AfterDiscardIntentCommit);
        if root.is_none() {
            return Ok(true);
        }
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("exclude concurrent discard", e))?;
        self.check_write_admission(&tx)?;
        let current = detached_from(&tx, id)?.ok_or_else(corrupt)?;
        if current.state == DetachedAllocationState::Discarded {
            return Ok(false);
        }
        require_discard_unreferenced(&tx, id)?;
        if detached_physical_key(&tx, id)? != key {
            return Err(corrupt());
        }
        if let Some(root) = root {
            root.remove().map_err(|_| {
                PersistenceError::ServiceStorageUnavailable(
                    "discard could not safely end the allocation lifetime",
                )
            })?;
        }
        super::sqlite_revision_store::fault(super::FaultPoint::AfterDiscardRemoval);
        tx.execute(
            "UPDATE allocation_discard_receipts SET finished=1 WHERE allocation_id=?1",
            [id.as_bytes().as_slice()],
        )
        .map_err(|e| PersistenceError::sqlite("publish discard completion", e))?;
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit discard completion", e))?;
        super::sqlite_revision_store::fault(super::FaultPoint::AfterDiscardCompletionCommit);
        Ok(true)
    }

    pub(crate) fn detached_handoff(
        &self,
        id: ServiceAllocationId,
    ) -> Result<Vec<crate::retirement_fs::HandoffLocation>, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("inspect explicit detached handoff", e))?;
        let view =
            detached_from(&tx, id)?.ok_or_else(|| invalid("detached allocation not found"))?;
        if view.state == DetachedAllocationState::Discarded {
            return Err(invalid("discarded allocation has no live handoff"));
        }
        let root_path = self
            .database_path
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(corrupt)?;
        let saved = detached_physical_key(&tx, id)?;
        let locations =
            crate::retirement_fs::handoff(root_path, id, saved.as_ref()).map_err(|_| {
                PersistenceError::ServiceStorageUnavailable("detached location is unavailable")
            })?;
        if locations.is_empty() {
            return Err(PersistenceError::ServiceStorageUnavailable(
                "detached directory is absent",
            ));
        }
        Ok(locations)
    }

    /// One bounded allocation step. Durable authority and the physical
    /// incarnation are committed before destructive I/O; finished progress is
    /// committed only after the directory removal is durable.
    pub(crate) fn finalize_next_allocation(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
    ) -> Result<bool, PersistenceError> {
        let root_path = self
            .database_path
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(corrupt)?;
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin owned-storage finalization step", e))?;
        self.check_write_admission(&tx)?;
        let view = owned_deletion(&tx, run, owner)?;
        if !matches!(
            view.operation,
            ManagedRunIdentity::Deletion {
                mode: DeletionMode::ManagedCleanup,
                ..
            }
        ) {
            return Err(invalid("Abandon does not authorize storage destruction"));
        }
        let obligation = obligation_from(&tx, view.instance)?.ok_or_else(corrupt)?;
        if obligation.phase != DeletionPhase::FinalizationAuthorized {
            return Err(invalid("Cleanup has not been resolved"));
        }
        let peers: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM runs r JOIN run_executions e ON e.run_id=r.run_id JOIN run_revision_pins p ON p.run_id=r.run_id WHERE r.instance_id=?1 AND r.run_id<>?2)",
            params![view.instance.as_bytes().as_slice(),run.as_bytes().as_slice()], |r| r.get(0))
            .map_err(|e| PersistenceError::sqlite("check live retirement references", e))?;
        if peers {
            return Ok(false);
        }
        let pending: Option<(Vec<u8>,Option<Vec<u8>>)> = tx.query_row(
            "SELECT f.allocation_id,f.root_identity FROM deletion_finalization_allocations f JOIN service_storage_allocations a ON a.allocation_id=f.allocation_id WHERE f.attempt_run_id=?1 AND a.instance_id=?2 AND f.finished=0 ORDER BY f.allocation_id LIMIT 1",
            params![obligation.attempt.as_bytes().as_slice(),view.instance.as_bytes().as_slice()], |r| Ok((r.get(0)?,r.get(1)?)),
        ).optional().map_err(|e| PersistenceError::sqlite("select pending allocation lifetime", e))?;
        let Some((id, saved)) = pending else {
            return Ok(true);
        };
        let saved = physical_key(saved)?;
        let allocation = ServiceAllocationId::from_bytes(id.try_into().map_err(|_| corrupt())?);
        let root = crate::retirement_fs::open_qualified(root_path, allocation, saved.as_ref())
            .map_err(|_| {
                PersistenceError::ServiceStorageUnavailable(
                    "allocation root cannot be safely qualified for retirement",
                )
            })?;
        if let Some(root) = &root {
            if saved.as_ref().is_some_and(|saved| {
                !crate::retirement_fs::same_incarnation(saved, root.identity())
            }) {
                return Err(PersistenceError::ServiceStorageUnavailable(
                    "allocation root changed after retirement authorization",
                ));
            }
            if saved.as_ref() != Some(root.identity()) {
                tx.execute("UPDATE deletion_finalization_allocations SET root_identity=?3 WHERE attempt_run_id=?1 AND allocation_id=?2",
                    params![obligation.attempt.as_bytes().as_slice(),allocation.as_bytes().as_slice(),root.identity().as_slice()])
                    .map_err(|e| PersistenceError::sqlite("record allocation retirement incarnation", e))?;
            }
        }
        if root.is_none() {
            tx.execute("UPDATE deletion_finalization_allocations SET finished=1 WHERE attempt_run_id=?1 AND allocation_id=?2",
                params![obligation.attempt.as_bytes().as_slice(),allocation.as_bytes().as_slice()])
                .map_err(|e| PersistenceError::sqlite("record absent finalization target", e))?;
        }
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit physical retirement evidence", e))?;
        super::sqlite_revision_store::fault(super::FaultPoint::AfterFinalizationIdentityCommit);
        if root.is_none() {
            return Ok(false);
        }
        drop(db);
        if let Some(root) = root {
            root.remove().map_err(|_| {
                PersistenceError::ServiceStorageUnavailable(
                    "allocation storage lifetime could not be ended safely",
                )
            })?;
        }
        super::sqlite_revision_store::fault(super::FaultPoint::AfterFinalizationRemoval);
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin finalization progress publication", e))?;
        self.check_write_admission(&tx)?;
        let current = owned_deletion(&tx, run, owner)?;
        if current.instance != view.instance
            || obligation_from(&tx, current.instance)? != Some(obligation)
        {
            return Err(corrupt());
        }
        tx.execute("UPDATE deletion_finalization_allocations SET finished=1 WHERE attempt_run_id=?1 AND allocation_id=?2",
            params![obligation.attempt.as_bytes().as_slice(),allocation.as_bytes().as_slice()])
            .map_err(|e| PersistenceError::sqlite("record ended allocation lifetime", e))?;
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit finalization progress", e))?;
        Ok(false)
    }

    /// Preserve the distinction between a supervised Run outcome and evidence
    /// that permits another Cleanup. No received message implies a SQL commit.
    pub(crate) fn publish_cleanup_result(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
        finish: &RunFinish,
        may_have_run: bool,
        completion_accepted: bool,
    ) -> Result<bool, PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin Cleanup result publication", e))?;
        self.check_write_admission(&tx)?;
        let view = owned_deletion(&tx, run, owner)?;
        let obligation = obligation_from(&tx, view.instance)?;
        if let Some(o) = obligation {
            if o.attempt != run {
                return Err(invalid(
                    "Cleanup attempt does not own the lifecycle obligation",
                ));
            }
            if o.phase == DeletionPhase::FinalizationAuthorized {
                return Ok(true);
            }
        }
        let risk = match &view.state {
            RunState::Running(e) => e.risk_state,
            _ => unreachable!(),
        };
        if finish.outcome == RunOutcome::Succeeded {
            if !may_have_run
                || !completion_accepted
                || finish.hook_completion.as_ref().map(|c| c.status)
                    != Some(HookCompletionStatus::Success)
                || obligation.is_none()
            {
                return Err(invalid(
                    "Cleanup success lacks supervised completion evidence",
                ));
            }
            let authority = authorize_hook_finalization(true, risk)
                .map_err(|_| invalid("Cleanup success has Open risk"))?;
            let (_, _, version) = instance_header(&tx, view.instance)?;
            authorize_finalization(&tx, view.instance, run, version, authority)?;
            super::sqlite_revision_store::fault(super::FaultPoint::BeforeCleanupBoundaryCommit);
            tx.commit()
                .map_err(|e| PersistenceError::sqlite("commit Cleanup-completed boundary", e))?;
            super::sqlite_revision_store::fault(super::FaultPoint::AfterCleanupBoundaryCommit);
            return Ok(true);
        }
        let reliable_failure = completion_accepted
            && finish
                .hook_completion
                .as_ref()
                .is_some_and(|c| c.status == HookCompletionStatus::Failure);
        if !may_have_run || reliable_failure {
            tx.execute("DELETE FROM instance_deletion_obligations WHERE instance_id=?1 AND attempt_run_id=?2 AND phase<>2",
                params![view.instance.as_bytes().as_slice(),run.as_bytes().as_slice()])
                .map_err(|e| PersistenceError::sqlite("resolve known Cleanup non-success", e))?;
        } else {
            if obligation.is_none() {
                return Err(invalid(
                    "possibly launched Cleanup lacks launch authorization",
                ));
            }
            reconcile_obligation(&tx, run)?;
        }
        super::sqlite_runs::finish_deletion_in_transaction(&tx, run, owner, finish, false)?;
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit Cleanup non-success", e))?;
        Ok(false)
    }

    pub(crate) fn prepare_deletion_finalization(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
    ) -> Result<RunId, PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin deletion finalization authority", e))?;
        self.check_write_admission(&tx)?;
        let view = owned_deletion(&tx, run, owner)?;
        if !matches!(
            &view.operation,
            ManagedRunIdentity::Deletion {
                mode: DeletionMode::ManagedCleanup,
                ..
            }
        ) {
            return Err(invalid("Abandon cannot authorize destructive finalization"));
        }
        if let Some(obligation) = obligation_from(&tx, view.instance)? {
            if obligation.phase != DeletionPhase::FinalizationAuthorized {
                return Err(invalid("Cleanup remains unresolved"));
            }
            return Ok(obligation.attempt);
        }
        let content = load_revision_from(&tx, view.operation.revision())?.ok_or_else(corrupt)?;
        if content.content.core.cleanup().is_some() {
            return Err(invalid("normal deletion cannot skip declared Cleanup"));
        }
        let (_, _, version) = instance_header(&tx, view.instance)?;
        authorize_finalization(
            &tx,
            view.instance,
            run,
            version,
            FinalizationAuthority::NoCleanupDeclared,
        )?;
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit no-Cleanup finalization authority", e))?;
        Ok(run)
    }

    /// Metadata completion is independent of physical deletion. This method
    /// never performs I/O and cannot claim success while lifetime duties remain.
    pub(crate) fn finish_instance_retirement(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
        finish: &RunFinish,
    ) -> Result<bool, PersistenceError> {
        if finish.outcome != RunOutcome::Succeeded || finish.primary_failure.is_some() {
            return Err(invalid("retirement publication requires success"));
        }
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin Instance retirement", e))?;
        self.check_write_admission(&tx)?;
        let view = owned_deletion(&tx, run, owner)?;
        let ManagedRunIdentity::Deletion { mode, .. } = view.operation else {
            unreachable!()
        };
        super::sqlite_service_views::load_instance_service_state_from(&tx, view.instance)?
            .ok_or_else(corrupt)?;
        // Observe does not conflict at admission. Its pinned immutable context
        // nevertheless must outlive it: retirement waits for existing readers.
        let other: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM runs r JOIN run_executions e ON e.run_id=r.run_id JOIN run_revision_pins p ON p.run_id=r.run_id WHERE r.instance_id=?1 AND r.run_id<>?2)",
            params![view.instance.as_bytes().as_slice(),run.as_bytes().as_slice()], |r| r.get(0))
            .map_err(|e| PersistenceError::sqlite("check retirement reference lifetime", e))?;
        if other {
            return Ok(false);
        }
        let obligation = obligation_from(&tx, view.instance)?;
        if mode == DeletionMode::ManagedCleanup {
            let o = obligation
                .ok_or_else(|| invalid("normal retirement lacks finalization authority"))?;
            if o.phase != DeletionPhase::FinalizationAuthorized {
                return Err(invalid("Cleanup is unresolved"));
            }
            let unfinished: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM service_storage_allocations a JOIN service_storage_protections p ON p.allocation_id=a.allocation_id WHERE a.instance_id=?1 AND NOT EXISTS(SELECT 1 FROM deletion_finalization_allocations f WHERE f.allocation_id=a.allocation_id AND f.attempt_run_id=?2 AND f.finished=1))",
                params![view.instance.as_bytes().as_slice(),o.attempt.as_bytes().as_slice()], |r| r.get(0))
                .map_err(|e| PersistenceError::sqlite("verify ended storage lifetimes", e))?;
            if unfinished {
                return Err(invalid("Instance storage finalization is not complete"));
            }
        } else if finish.hook_completion.is_some() {
            return Err(invalid("Abandon cannot record a Hook completion"));
        }
        let partial: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM deletion_finalization_allocations f JOIN runs r ON r.run_id=f.attempt_run_id WHERE r.instance_id=?1 AND f.finished=1)",
            [view.instance.as_bytes().as_slice()], |r| r.get(0))
            .map_err(|e| PersistenceError::sqlite("inspect partial finalization", e))?;
        tx.execute(
            "INSERT INTO instance_retirement_receipts VALUES(?1,?2,?3,?4,?5)",
            params![
                view.instance.as_bytes().as_slice(),
                run.as_bytes().as_slice(),
                mode.rank(),
                i64::from(partial),
                now()?
            ],
        )
        .map_err(|e| PersistenceError::sqlite("record Instance retirement", e))?;
        // A creation intent is not proof of ownership of an existing path.
        // Keep unexposed preparations under their original conservative rules;
        // neither retirement nor Abandon may promote them into discard authority.
        if mode == DeletionMode::AbandonManagement {
            tx.execute("INSERT INTO detached_service_allocations SELECT a.allocation_id,a.instance_id FROM service_storage_allocations a JOIN service_storage_protections p ON p.allocation_id=a.allocation_id WHERE a.instance_id=?1 AND NOT EXISTS(SELECT 1 FROM deletion_finalization_allocations f WHERE f.allocation_id=a.allocation_id AND f.finished=1)", [view.instance.as_bytes().as_slice()])
                .map_err(|e| PersistenceError::sqlite("publish abandoned allocation handoff", e))?;
        }
        super::sqlite_runs::finish_deletion_in_transaction(&tx, run, owner, finish, true)?;
        for table in [
            "instance_deletion_obligations",
            "instance_service_resources",
            "instance_service_storages",
        ] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE instance_id=?1"),
                [view.instance.as_bytes().as_slice()],
            )
            .map_err(|e| PersistenceError::sqlite("remove retired Instance associations", e))?;
        }
        tx.execute(
            "DELETE FROM instances WHERE instance_id=?1",
            [view.instance.as_bytes().as_slice()],
        )
        .map_err(|e| PersistenceError::sqlite("remove retired Instance", e))?;
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit Instance retirement", e))?;
        Ok(true)
    }

    pub(crate) fn admit_deletion(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
        plan: &DeletionPlan,
        override_guard: bool,
        launcher_check: &dyn Fn(&InterpreterLauncherObservation) -> Result<(), String>,
    ) -> Result<Result<(), AdmissionRefusal>, PersistenceError> {
        if self
            .staging_session()
            .is_none_or(|session| session.owner() != *owner)
        {
            return Err(invalid(
                "deletion admission requires the accepting live owner",
            ));
        }
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin deletion admission", e))?;
        self.check_write_admission(&tx)?;
        let view = super::sqlite_runs::load_managed_run_from(&tx, run)?;
        let RunState::Running(execution) = &view.state else {
            return Err(PersistenceError::RunNotRunning);
        };
        if execution.owner != *owner
            || execution.boundary != ActionRunBoundary::Accepted
            || view.operation
                != (ManagedRunIdentity::Deletion {
                    revision: plan.revision.clone(),
                    mode: plan.mode,
                })
            || view.instance != plan.instance
        {
            return Err(invalid(
                "deletion plan does not match its owned accepted Run",
            ));
        }
        let decision = (|| {
            let guarded: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM instance_recovery_guards WHERE instance_id=?1)",
                    [view.instance.as_bytes().as_slice()],
                    |r| r.get(0),
                )
                .map_err(|e| PersistenceError::sqlite("read deletion trust guard", e))?;
            // Abandon does not assert trust; authorized finalization never runs
            // Package code. Only a new normal Cleanup needs the ordinary guard.
            if guarded
                && !override_guard
                && matches!(
                    plan.work,
                    DeletionWork::Cleanup(_) | DeletionWork::NoCleanup
                )
            {
                return Ok(Some(AdmissionRefusal::RecoveryGuardActive));
            }
            let (_, revision, version) = match instance_header(&tx, view.instance) {
                Ok(header) => header,
                Err(PersistenceError::MissingInstance(_)) => {
                    return Ok(Some(AdmissionRefusal::PlanInvalidated(
                        "Instance has been retired".to_owned(),
                    )));
                }
                Err(error) => return Err(error),
            };
            if version != plan.expected
                || version != view.accepted_state_version
                || revision != plan.revision
            {
                return Ok(Some(AdmissionRefusal::PlanInvalidated(
                    "deletion Instance state changed".to_owned(),
                )));
            }
            let current = load_instance_view_from(&tx, view.instance)?.ok_or_else(corrupt)?;
            let content = load_revision_from(&tx, &revision)?.ok_or_else(corrupt)?;
            let launch = match &plan.work {
                DeletionWork::Cleanup(cleanup) => Some(cleanup.launch.clone()),
                _ => None,
            };
            let rebuilt = build_deletion_plan(
                &DeleteInstance {
                    instance: plan.instance,
                    expected: plan.expected,
                    mode: plan.mode,
                },
                DeletionCompilationObservation {
                    instance: current,
                    revision_identity: revision,
                    revision: content.content,
                    service_state: super::sqlite_service_views::load_instance_service_state_from(
                        &tx,
                        view.instance,
                    )?,
                    obligation: obligation_from(&tx, view.instance)?,
                },
                launch,
            );
            if rebuilt.as_ref().ok() != Some(plan) {
                return Ok(Some(AdmissionRefusal::PlanInvalidated(
                    "deletion context or lifecycle evidence changed".to_owned(),
                )));
            }
            if let DeletionWork::Cleanup(cleanup) = &plan.work {
                if let Some(launcher) = cleanup.launch.launcher()
                    && launcher_check(launcher).is_err()
                {
                    return Ok(Some(AdmissionRefusal::PlanInvalidated(
                        "Cleanup launcher changed".to_owned(),
                    )));
                }
                for file in &cleanup.runtime_content {
                    if self
                        .runtime_content
                        .open_verified(&file.blob_digest)
                        .is_err()
                    {
                        return Ok(Some(AdmissionRefusal::PlanInvalidated(
                            "Cleanup runtime content is unavailable".to_owned(),
                        )));
                    }
                }
            }
            if let Some(other) =
                super::sqlite_runs::managed_mutation_conflict(&tx, view.instance, run)?
            {
                return Ok(Some(AdmissionRefusal::MutationConflict(other)));
            }
            Ok::<_, PersistenceError>(None)
        })()?;
        let result = if let Some(refusal) = decision {
            super::sqlite_runs::finish_deletion_refusal(&tx, run, &refusal)?;
            Err(refusal)
        } else {
            tx.execute(
                "INSERT INTO run_revision_pins VALUES(?1,?2,?3)",
                params![
                    run.as_bytes().as_slice(),
                    plan.revision.package_id.as_bytes().as_slice(),
                    plan.revision.content_digest.as_bytes().as_slice()
                ],
            )
            .map_err(|e| PersistenceError::sqlite("pin deletion Revision", e))?;
            if let DeletionWork::Cleanup(cleanup) = &plan.work {
                tx.execute("INSERT INTO run_payload_pins SELECT ?1,instance_id,input_identity,payload_id FROM managed_input_bindings WHERE instance_id=?2",
                    params![run.as_bytes().as_slice(),plan.instance.as_bytes().as_slice()])
                    .map_err(|e| PersistenceError::sqlite("pin Cleanup active and retained context", e))?;
                super::sqlite_service_admission::pin_service_bindings(
                    &tx,
                    run,
                    &cleanup.service_bindings,
                )?;
            }
            Ok(())
        };
        super::sqlite_revision_store::fault(super::FaultPoint::BeforeRunAdmitCommit);
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit deletion admission", e))?;
        super::sqlite_revision_store::fault(super::FaultPoint::AfterRunAdmitCommit);
        Ok(result)
    }

    pub(crate) fn authorize_cleanup_launch(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
    ) -> Result<(), PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin Cleanup launch authorization", e))?;
        self.check_write_admission(&tx)?;
        let view = super::sqlite_runs::load_managed_run_from(&tx, run)?;
        if !matches!(&view.state, RunState::Running(e) if e.owner == *owner && e.boundary == ActionRunBoundary::Admitted)
            || !matches!(
                &view.operation,
                ManagedRunIdentity::Deletion {
                    mode: DeletionMode::ManagedCleanup,
                    ..
                }
            )
        {
            return Err(invalid(
                "Cleanup launch requires its admitted deletion owner",
            ));
        }
        // A repeated call after uncertain commit is not a second launch permit.
        // The owner continuation must inspect its own durable boundary first.
        if obligation_from(&tx, view.instance)?.is_some() {
            return Err(invalid(
                "Cleanup launch has already been authorized or resolved",
            ));
        }
        let revision = load_revision_from(&tx, view.operation.revision())?.ok_or_else(corrupt)?;
        if revision.content.core.cleanup().is_none() {
            return Err(invalid("Revision has no Cleanup Hook"));
        }
        tx.execute(
            "INSERT INTO instance_deletion_obligations VALUES(?1,?2,0)",
            params![
                view.instance.as_bytes().as_slice(),
                run.as_bytes().as_slice()
            ],
        )
        .map_err(|e| PersistenceError::sqlite("record Cleanup launch authorization", e))?;
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit Cleanup launch authorization", e))
    }

    pub(crate) fn observe_deletion(
        &self,
        intent: &DeleteInstance,
    ) -> Result<DeletionCompilationObservation, PersistenceError> {
        let mut database = self.open_read_connection()?;
        let tx = database
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("observe deletion compilation", e))?;
        let instance = load_instance_view_from(&tx, intent.instance)?
            .ok_or_else(|| PersistenceError::MissingInstance(intent.instance.to_string()))?;
        let revision = load_revision_from(&tx, &instance.active_revision)?
            .ok_or_else(|| PersistenceError::MissingRevision(instance.active_revision.clone()))?;
        Ok(DeletionCompilationObservation {
            revision_identity: instance.active_revision.clone(),
            revision: revision.content,
            service_state: super::sqlite_service_views::load_instance_service_state_from(
                &tx,
                intent.instance,
            )?,
            obligation: obligation_from(&tx, intent.instance)?,
            instance,
        })
    }

    pub(crate) fn deletion_obligation(
        &self,
        instance: InstanceId,
    ) -> Result<Option<DeletionObligation>, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("inspect deletion lifecycle", e))?;
        obligation_from(&tx, instance)
    }

    pub(crate) fn confirm_cleanup_completion(
        &self,
        confirmation: CleanupConfirmation,
    ) -> Result<InstanceStateVersion, PersistenceError> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin Cleanup assertion", e))?;
        self.check_write_admission(&tx)?;
        let (_, _, current) = instance_header(&tx, confirmation.instance)?;
        let obligation = obligation_from(&tx, confirmation.instance)?
            .ok_or_else(|| invalid("there is no unresolved Cleanup attempt"))?;
        let authority =
            authorize_confirmation(obligation, current, confirmation).map_err(|_| {
                invalid("Cleanup confirmation is stale or does not name the unresolved attempt")
            })?;
        // The unresolved attempt must have been reconciled, not still owned.
        let active: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM run_executions e JOIN runs r ON r.run_id=e.run_id WHERE r.instance_id=?1)",
            [confirmation.instance.as_bytes().as_slice()], |r| r.get(0),
        ).map_err(|e| PersistenceError::sqlite("exclude live executions from Cleanup assertion", e))?;
        if active {
            return Err(invalid(
                "Cleanup confirmation requires all execution owners to finish or be reconciled",
            ));
        }
        let next = fresh_state_version()?;
        authorize_finalization(
            &tx,
            obligation.instance,
            obligation.attempt,
            next,
            authority,
        )?;
        tx.execute(
            "UPDATE instances SET instance_state_version=?2 WHERE instance_id=?1",
            params![
                confirmation.instance.as_bytes().as_slice(),
                next.as_bytes().as_slice()
            ],
        )
        .map_err(|e| PersistenceError::sqlite("publish Cleanup assertion version", e))?;
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit Cleanup assertion", e))?;
        Ok(next)
    }
}

fn detached_from(
    db: &Connection,
    id: ServiceAllocationId,
) -> Result<Option<DetachedAllocationView>, PersistenceError> {
    let row = db.query_row(
        "SELECT d.instance_id,a.instance_id,h.instance_name,a.package_id,a.origin_revision_digest,a.origin_storage_identity,c.finished,t.retirement_mode,o.outcome_rank,r.instance_id FROM detached_service_allocations d LEFT JOIN service_storage_allocations a ON a.allocation_id=d.allocation_id LEFT JOIN instance_history_identities h ON h.instance_id=d.instance_id LEFT JOIN instance_retirement_receipts t ON t.instance_id=d.instance_id LEFT JOIN runs r ON r.run_id=t.retirement_run_id LEFT JOIN run_outcomes o ON o.run_id=t.retirement_run_id LEFT JOIN allocation_discard_receipts c ON c.allocation_id=d.allocation_id WHERE d.allocation_id=?1",
        [id.as_bytes().as_slice()], |r| Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,Vec<u8>>(2)?,r.get::<_,Vec<u8>>(3)?,r.get::<_,Vec<u8>>(4)?,r.get::<_,Vec<u8>>(5)?,r.get::<_,Option<i64>>(6)?,r.get::<_,i64>(7)?,r.get::<_,i64>(8)?,r.get::<_,Vec<u8>>(9)?)),
    ).optional().map_err(|e| PersistenceError::sqlite("decode detached custody", e))?;
    let Some((
        instance,
        owner,
        name,
        package,
        digest,
        storage,
        discarded,
        mode,
        outcome,
        retired_owner,
    )) = row
    else {
        return Ok(None);
    };
    if instance != owner || instance != retired_owner || mode != 1 || outcome != 0 {
        return Err(corrupt());
    }
    let instance = InstanceId::from_bytes(instance.try_into().map_err(|_| corrupt())?);
    let live: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM instances WHERE instance_id=?1)",
            [instance.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .map_err(|e| PersistenceError::sqlite("validate detached owner absence", e))?;
    if live {
        return Err(corrupt());
    }
    let valid: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM detached_service_allocations d JOIN instance_retirement_receipts t ON t.instance_id=d.instance_id JOIN run_deletion_invocations i ON i.run_id=t.retirement_run_id JOIN run_operation_kinds k ON k.run_id=i.run_id JOIN service_storage_protections p ON p.allocation_id=d.allocation_id WHERE d.allocation_id=?1 AND i.deletion_mode=1 AND k.operation_kind=4 AND NOT EXISTS(SELECT 1 FROM run_executions e WHERE e.run_id=i.run_id) AND NOT EXISTS(SELECT 1 FROM deletion_finalization_allocations f WHERE f.allocation_id=d.allocation_id AND f.finished=1))",
        [id.as_bytes().as_slice()], |r| r.get(0),
    ).map_err(|_| corrupt())?;
    if !valid {
        return Err(corrupt());
    }
    let key = detached_physical_key(db, id)?;
    if discarded == Some(0) && key.is_none() {
        return Err(corrupt());
    }
    let state = match discarded {
        None => DetachedAllocationState::Preserved,
        Some(0) => DetachedAllocationState::DiscardPending,
        Some(1) => DetachedAllocationState::Discarded,
        _ => return Err(corrupt()),
    };
    Ok(Some(DetachedAllocationView {
        allocation: id,
        instance,
        former_name: InstanceName::parse(String::from_utf8(name).map_err(|_| corrupt())?)
            .map_err(|_| corrupt())?,
        origin_revision: super::sqlite_instances::revision_identity(package, digest)?,
        origin_storage: ServiceStorageIdentity::parse(
            String::from_utf8(storage).map_err(|_| corrupt())?,
        )
        .map_err(|_| corrupt())?,
        state,
    }))
}

fn detached_physical_key(
    db: &Connection,
    id: ServiceAllocationId,
) -> Result<Option<[u8; 40]>, PersistenceError> {
    let discard: Option<Vec<u8>> = db
        .query_row(
            "SELECT root_identity FROM allocation_discard_receipts WHERE allocation_id=?1",
            [id.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| PersistenceError::sqlite("read discard incarnation", e))?
        .flatten();
    let prior: Option<Vec<u8>> = db
        .query_row(
            "SELECT root_identity FROM deletion_finalization_allocations WHERE allocation_id=?1",
            [id.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| PersistenceError::sqlite("read prior finalization incarnation", e))?
        .flatten();
    let (discard, prior) = (physical_key(discard)?, physical_key(prior)?);
    if let (Some(a), Some(b)) = (&discard, &prior) {
        if !crate::retirement_fs::same_incarnation(a, b) {
            return Err(corrupt());
        }
        if matches!(b[0], 3 | 4) {
            return Ok(prior);
        }
    }
    Ok(discard.or(prior))
}

fn require_discard_unreferenced(
    db: &Connection,
    id: ServiceAllocationId,
) -> Result<(), PersistenceError> {
    let referenced: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM instance_service_storages WHERE allocation_id=?1 UNION ALL SELECT 1 FROM instance_service_resources WHERE allocation_id=?1 UNION ALL SELECT 1 FROM run_service_storage_pins WHERE allocation_id=?1 UNION ALL SELECT 1 FROM service_storage_preparations WHERE allocation_id=?1 UNION ALL SELECT 1 FROM deletion_finalization_allocations f JOIN instance_deletion_obligations o ON o.attempt_run_id=f.attempt_run_id WHERE f.allocation_id=?1)",
        [id.as_bytes().as_slice()], |r| r.get(0))
        .map_err(|e| PersistenceError::sqlite("validate discard reference lifetime", e))?;
    if referenced {
        return Err(invalid("detached allocation still has active references"));
    }
    Ok(())
}

pub(super) fn authorize_finalization(
    tx: &Transaction<'_>,
    instance: InstanceId,
    attempt: RunId,
    version: InstanceStateVersion,
    authority: FinalizationAuthority,
) -> Result<(), PersistenceError> {
    // This is structural custody validation, not ordinary configuration
    // readiness. In particular, a creation intent is not owned live storage.
    super::sqlite_service_views::load_instance_service_state_from(tx, instance)?
        .ok_or_else(corrupt)?;
    tx.execute(
        "INSERT INTO deletion_finalization_authorizations VALUES(?1,?2,?3,?4)",
        params![
            attempt.as_bytes().as_slice(),
            authority.rank(),
            version.as_bytes().as_slice(),
            now()?
        ],
    )
    .map_err(|e| PersistenceError::sqlite("publish finalization authorization", e))?;
    tx.execute("INSERT INTO instance_deletion_obligations VALUES(?1,?2,2) ON CONFLICT(instance_id) DO UPDATE SET phase=2 WHERE attempt_run_id=excluded.attempt_run_id",
        params![instance.as_bytes().as_slice(),attempt.as_bytes().as_slice()])
        .map_err(|e| PersistenceError::sqlite("publish finalization obligation", e))?;
    tx.execute("INSERT INTO deletion_finalization_allocations(attempt_run_id,allocation_id,finished) SELECT ?1,a.allocation_id,0 FROM service_storage_allocations a JOIN service_storage_protections p ON p.allocation_id=a.allocation_id WHERE a.instance_id=?2",
        params![attempt.as_bytes().as_slice(),instance.as_bytes().as_slice()])
        .map_err(|e| PersistenceError::sqlite("freeze finalization work set", e))?;
    let saved = obligation_from(tx, instance)?.ok_or_else(corrupt)?;
    if saved.attempt != attempt || saved.phase != DeletionPhase::FinalizationAuthorized {
        return Err(corrupt());
    }
    Ok(())
}
