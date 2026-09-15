//! Current-edge target allocation and durable lifetime references. Target rows
//! are addresses, never a replayable target-ready or success receipt.
use super::{
    PactrunPersistence, PersistenceError, sqlite_migration_runs::progress_view,
    sqlite_revision_store::load_revision_from, sqlite_runs::load_managed_run_from,
};
use crate::domain::*;
use rusqlite::{Connection, TransactionBehavior, params};
use std::collections::BTreeMap;

fn invalid() -> PersistenceError {
    PersistenceError::CorruptServiceStorage("inconsistent prepared service targets")
}
struct Context {
    instance: InstanceId,
    token: InstanceStateVersion,
    revision: RevisionIdentity,
    edge: ServiceMigrationEdge,
}
fn context(
    db: &Connection,
    run: RunId,
    owner: &ExecutionOwnerSession,
    index: usize,
) -> Result<Context, PersistenceError> {
    let view = load_managed_run_from(db, run)?;
    let ManagedRunIdentity::Migration(invocation) = &view.operation else {
        return Err(invalid());
    };
    let RunState::Running(execution) = &view.state else {
        return Err(PersistenceError::RunNotRunning);
    };
    let progress = progress_view(db, run)?.ok_or_else(invalid)?;
    if execution.owner != *owner
        || execution.boundary != ActionRunBoundary::Admitted
        || progress.committed_edges != index
    {
        return Err(PersistenceError::InvalidRunTransition(
            "service preparation owner or edge changed".into(),
        ));
    }
    let state = super::sqlite_service_views::load_instance_service_state_from(db, view.instance)?
        .ok_or_else(invalid)?;
    if state.current_revision != progress.boundary_revision
        || state.state_version != progress.boundary_state_version
    {
        return Err(invalid());
    }
    let source = load_revision_from(db, &invocation.path()[index])?.ok_or_else(invalid)?;
    let revision = invocation.path()[index + 1].clone();
    let target = load_revision_from(db, &revision)?.ok_or_else(invalid)?;
    let before = ServiceMigrationState::from_observed(&state).map_err(|_| invalid())?;
    let edge = evaluate_service_migration_edge(
        &before,
        &source.content.core,
        &revision,
        &target.content.core,
        index,
    )
    .map_err(|_| invalid())?;
    Ok(Context {
        instance: view.instance,
        token: state.state_version,
        revision,
        edge,
    })
}
fn rows(
    db: &Connection,
    run: RunId,
    index: usize,
    resources: bool,
) -> Result<BTreeMap<String, ServiceAllocationId>, PersistenceError> {
    let (table, id) = if resources {
        ("run_service_resource_targets", "resource_identity")
    } else {
        ("run_service_storage_targets", "storage_identity")
    };
    let mut q = db
        .prepare(&format!(
            "SELECT {id},allocation_id,edge_index FROM {table} WHERE run_id=?1 ORDER BY {id}"
        ))
        .map_err(|e| PersistenceError::sqlite("read service target rows", e))?;
    let found = q
        .query_map([run.as_bytes().as_slice()], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| PersistenceError::sqlite("query service targets", e))?;
    let mut result = BTreeMap::new();
    for row in found {
        let (id, allocation, edge) =
            row.map_err(|e| PersistenceError::sqlite("decode service target", e))?;
        if edge != index as i64 {
            return Err(invalid());
        }
        let id = String::from_utf8(id).map_err(|_| invalid())?;
        let allocation =
            ServiceAllocationId::from_bytes(allocation.try_into().map_err(|_| invalid())?);
        if result.insert(id, allocation).is_some() {
            return Err(invalid());
        }
    }
    Ok(result)
}
fn resolved(
    edge: &ServiceMigrationEdge,
    index: usize,
    created: &BTreeMap<ServiceStorageIdentity, ServiceAllocationId>,
) -> Result<ServiceMigrationEdge, PersistenceError> {
    let origin = |value: &mut ServiceAllocationOrigin| -> Result<(), PersistenceError> {
        if let ServiceAllocationOrigin::Created { edge, storage } = value {
            if *edge != index {
                return Err(invalid());
            }
            *value = ServiceAllocationOrigin::Existing(*created.get(storage).ok_or_else(invalid)?);
        }
        Ok(())
    };
    let mut result = edge.clone();
    for value in result.after.storages.values_mut() {
        origin(&mut value.allocation)?;
    }
    for value in result.after.resources.values_mut() {
        origin(&mut value.allocation)?;
    }
    for (_, value) in &mut result.create_presence {
        origin(&mut value.allocation)?;
    }
    for object in result
        .grants
        .iter_mut()
        .map(|(_, o)| o)
        .chain(result.requires.iter_mut().map(|(_, o)| o))
    {
        match object {
            PlannedServiceObject::Storage { allocation } => origin(allocation)?,
            PlannedServiceObject::Resource(r) => origin(&mut r.allocation)?,
        }
    }
    Ok(result)
}
pub(super) fn load_prepared(
    db: &Connection,
    run: RunId,
    index: usize,
    edge: &ServiceMigrationEdge,
) -> Result<ServiceMigrationEdge, PersistenceError> {
    let storages = rows(db, run, index, false)?;
    let resources = rows(db, run, index, true)?;
    let active: Vec<_> = edge
        .after
        .storages
        .iter()
        .filter(|(_, s)| s.declaration_revision == edge.after.revision)
        .collect();
    if storages.len() != active.len() {
        return Err(invalid());
    }
    let mut created = BTreeMap::new();
    for (id, expected) in active {
        let actual = *storages.get(id.as_str()).ok_or_else(invalid)?;
        match &expected.allocation {
            ServiceAllocationOrigin::Existing(old) if *old != actual => return Err(invalid()),
            ServiceAllocationOrigin::Created { edge: at, storage } => {
                if *at != index || storage != id {
                    return Err(invalid());
                }
                let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM service_storage_allocations a JOIN service_storage_run_origins o ON o.allocation_id=a.allocation_id WHERE a.allocation_id=?1 AND o.origin_run_id=?2 AND a.origin_revision_digest=?3 AND a.origin_storage_identity=?4)",params![actual.as_bytes().as_slice(),run.as_bytes().as_slice(),edge.after.revision.content_digest.as_bytes().as_slice(),id.as_str().as_bytes()],|r|r.get(0)).map_err(|e|PersistenceError::sqlite("validate target allocation origin",e))?;
                if !valid {
                    return Err(invalid());
                }
                created.insert(id.clone(), actual);
            }
            _ => (),
        }
    }
    let result = resolved(edge, index, &created)?;
    let expected: BTreeMap<_, _> = result
        .after
        .resources
        .iter()
        .filter(|(_, r)| r.declaration_revision == result.after.revision)
        .map(|(id, r)| match r.allocation {
            ServiceAllocationOrigin::Existing(a) => Ok((id.as_str().to_owned(), a)),
            _ => Err(invalid()),
        })
        .collect::<Result<_, _>>()?;
    if resources != expected {
        return Err(invalid());
    }
    for allocation in super::sqlite_service_migrations::existing_dependencies(&result) {
        let valid:bool=db.query_row(
            "SELECT EXISTS(SELECT 1 FROM run_service_storage_pins s \
             JOIN run_executions e ON e.run_id=s.run_id JOIN runs r ON r.run_id=e.run_id \
             JOIN service_storage_allocations a ON a.allocation_id=s.allocation_id \
             JOIN service_storage_protections p ON p.allocation_id=a.allocation_id \
             WHERE s.run_id=?1 AND s.allocation_id=?2 AND a.instance_id=r.instance_id AND a.package_id=?3 \
             AND NOT EXISTS(SELECT 1 FROM service_storage_preparations t WHERE t.allocation_id=a.allocation_id))",
            params![run.as_bytes().as_slice(),allocation.as_bytes().as_slice(),result.after.revision.package_id.as_bytes().as_slice()],|r|r.get(0))
            .map_err(|e|PersistenceError::sqlite("validate prepared target custody and pins",e))?;
        if !valid {
            return Err(invalid());
        }
    }
    Ok(result)
}

impl PactrunPersistence {
    pub(crate) fn prepare_migration_service_targets(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
        index: usize,
    ) -> Result<ServiceMigrationEdge, PersistenceError> {
        if self.staging_session().is_none_or(|s| s.owner() != *owner) {
            return Err(PersistenceError::InvalidRunTransition(
                "service target preparation requires its live owner".into(),
            ));
        }
        let initial = {
            let db = self
                .database
                .lock()
                .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
            let tx = db
                .unchecked_transaction()
                .map_err(|e| PersistenceError::sqlite("observe service target preparation", e))?;
            let ctx = context(&tx, run, owner, index)?;
            if !rows(&tx, run, index, false)?.is_empty() || !rows(&tx, run, index, true)?.is_empty()
            {
                return load_prepared(&tx, run, index, &ctx.edge);
            }
            ctx
        };
        let prepared = self.prepare_service_storage_subset(
            initial.instance,
            &initial.revision,
            Some(&initial.edge.created_storages),
            Some(run),
        )?;
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("publish pending service targets", e))?;
        self.check_write_admission(&tx)?;
        let current = context(&tx, run, owner, index)?;
        if current.instance != initial.instance
            || current.token != initial.token
            || current.edge != initial.edge
        {
            return Err(invalid());
        }
        if !rows(&tx, run, index, false)?.is_empty() || !rows(&tx, run, index, true)?.is_empty() {
            return load_prepared(&tx, run, index, &current.edge);
        }
        let created = prepared.protect_targets(&tx, current.instance, &current.revision)?;
        let result = resolved(&current.edge, index, &created)?;
        let dependencies = super::sqlite_service_migrations::existing_dependencies(&result);
        self.qualify_service_roots(&tx, current.instance, &dependencies)?;
        super::sqlite_service_admission::pin_service_allocations(&tx, run, &dependencies)?;
        for (id, s) in result
            .after
            .storages
            .iter()
            .filter(|(_, s)| s.declaration_revision == result.after.revision)
        {
            let ServiceAllocationOrigin::Existing(a) = s.allocation else {
                return Err(invalid());
            };
            tx.execute(
                "INSERT INTO run_service_storage_targets VALUES(?1,?2,?3,?4)",
                params![
                    run.as_bytes().as_slice(),
                    index as i64,
                    id.as_str().as_bytes(),
                    a.as_bytes().as_slice()
                ],
            )
            .map_err(|e| PersistenceError::sqlite("record target storage selection", e))?;
        }
        for (id, r) in result
            .after
            .resources
            .iter()
            .filter(|(_, r)| r.declaration_revision == result.after.revision)
        {
            let ServiceAllocationOrigin::Existing(a) = r.allocation else {
                return Err(invalid());
            };
            tx.execute(
                "INSERT INTO run_service_resource_targets VALUES(?1,?2,?3,?4)",
                params![
                    run.as_bytes().as_slice(),
                    index as i64,
                    id.as_str().as_bytes(),
                    a.as_bytes().as_slice()
                ],
            )
            .map_err(|e| PersistenceError::sqlite("record target resource selection", e))?;
        }
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit pending service targets", e))?;
        Ok(result)
    }
}
