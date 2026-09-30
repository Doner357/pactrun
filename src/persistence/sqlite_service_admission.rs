//! Atomic revalidation/pinning of compiled service association facts. No native
//! path or live content is stored here; the owner-held Plan retains the mapping.
use super::{
    PersistenceError, sqlite_revision_store::load_revision_from,
    sqlite_service_views::load_instance_service_state_from,
};
use crate::domain::*;
use rusqlite::{Connection, Transaction, params};
use std::collections::BTreeSet;

impl super::PactrunPersistence {
    pub(crate) fn open_pinned_service_allocation(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
        allocation: ServiceAllocationId,
    ) -> Result<(std::fs::File, std::path::PathBuf), PersistenceError> {
        if self
            .staging_session()
            .is_none_or(|session| &session.owner() != owner)
        {
            return Err(PersistenceError::InvalidRunTransition(
                "service access requires this live owner session".into(),
            ));
        }
        {
            let db = self
                .database
                .lock()
                .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
            let (owned, valid): (bool, bool) = db.query_row(
                "WITH owned AS (SELECT s.allocation_id,r.instance_id,v.package_id FROM run_service_storage_pins s \
                 JOIN run_executions e ON e.run_id=s.run_id JOIN runs r ON r.run_id=e.run_id \
                 JOIN run_revision_pins v ON v.run_id=r.run_id \
                 WHERE s.run_id=?1 AND s.allocation_id=?2 AND e.owner_session=?3) \
                 SELECT EXISTS(SELECT 1 FROM owned), EXISTS(SELECT 1 FROM owned o \
                 JOIN service_storage_allocations a ON a.allocation_id=o.allocation_id \
                 JOIN service_storage_protections p ON p.allocation_id=a.allocation_id \
                 JOIN instances i ON i.instance_id=o.instance_id \
                 WHERE a.instance_id=o.instance_id AND a.package_id=o.package_id AND a.package_id=i.active_package_id \
                 AND NOT EXISTS(SELECT 1 FROM service_storage_preparations t WHERE t.allocation_id=a.allocation_id))",
                params![run.as_bytes().as_slice(), allocation.as_bytes().as_slice(), owner.as_str().as_bytes()], |r| Ok((r.get(0)?,r.get(1)?)),
            ).map_err(|e| PersistenceError::sqlite("qualify owned service access pin", e))?;
            if !owned {
                return Err(PersistenceError::InvalidRunTransition(
                    "service allocation is not pinned by this admitted owner".into(),
                ));
            }
            if !valid {
                return Err(PersistenceError::CorruptServiceStorage(
                    "pinned allocation custody is inconsistent",
                ));
            }
        }
        let root_path = self.database_path.parent().and_then(|p| p.parent()).ok_or(
            PersistenceError::ServiceStorageUnavailable("storage root is unavailable"),
        )?;
        let opened = (|| -> std::io::Result<_> {
            let root = crate::service_storage::open_root(root_path)?;
            let base = crate::service_storage::open_directory(&root, "service-storage")?;
            crate::service_storage::open_directory(&base, &format!("alloc-{allocation}"))
        })()
        .map_err(|_| {
            PersistenceError::ServiceStorageUnavailable(
                "protected service allocation is unavailable",
            )
        })?;
        Ok((
            opened,
            root_path
                .join("service-storage")
                .join(format!("alloc-{allocation}")),
        ))
    }
}

pub(super) fn qualify_current_service_facts(
    db: &Connection,
    instance: InstanceId,
    revision: &RevisionIdentity,
    invocation: &ManagedRunIdentity,
    facts: &AdmissionFacts<'_>,
) -> Result<Option<String>, PersistenceError> {
    let site = match invocation {
        ManagedRunIdentity::Action(action) => ServiceHookSite::Action(action.action.clone()),
        ManagedRunIdentity::Capture { .. } => ServiceHookSite::Capture,
        ManagedRunIdentity::Restore { .. } => ServiceHookSite::Restore,
        ManagedRunIdentity::Deletion { .. } => ServiceHookSite::Cleanup,
        ManagedRunIdentity::Migration(_) => {
            return Err(PersistenceError::InvalidRunTransition(
                "Migration requires edge-specific service admission".into(),
            ));
        }
    };
    let stored = load_revision_from(db, revision)?
        .ok_or_else(|| PersistenceError::MissingRevision(revision.clone()))?;
    let contract = stored.content.core.service_hook(&site);
    let empty = contract.access.is_empty() && contract.requires.is_empty();
    let Some((compiled_contract, compiled_bindings)) = facts.service else {
        return Ok(
            (!empty).then(|| "service authority facts are missing from the compiled Plan".into())
        );
    };
    if compiled_contract != &contract {
        return Ok(Some(
            "service declarations differ from the exact Revision".into(),
        ));
    }
    let state = if empty {
        None
    } else {
        load_instance_service_state_from(db, instance)?
    };
    let current = match bind_current_service_hook(
        &contract,
        state.as_ref(),
        instance,
        facts.expected_state_version,
        revision,
    ) {
        Ok(current) => current,
        Err(_) => {
            return Ok(Some(
                "service associations changed since compilation".into(),
            ));
        }
    };
    Ok((&current != compiled_bindings)
        .then(|| "compiled service allocations differ from current associations".into()))
}

pub(super) fn pin_service_bindings(
    tx: &Transaction<'_>,
    run: RunId,
    bindings: &ServiceHookBindings,
) -> Result<(), PersistenceError> {
    let allocations = bindings
        .grants
        .iter()
        .map(|(_, object)| object.allocation())
        .chain(
            bindings
                .requires
                .iter()
                .map(|(_, object)| object.allocation()),
        )
        .collect::<BTreeSet<_>>();
    pin_service_allocations(tx, run, &allocations)
}

pub(super) fn pin_service_allocations(
    tx: &Transaction<'_>,
    run: RunId,
    allocations: &BTreeSet<ServiceAllocationId>,
) -> Result<(), PersistenceError> {
    for allocation in allocations {
        let valid: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_storage_allocations a \
             JOIN service_storage_protections p ON p.allocation_id=a.allocation_id \
             JOIN runs r ON r.run_id=?2 JOIN run_executions e ON e.run_id=r.run_id \
             JOIN run_revision_pins v ON v.run_id=r.run_id \
             WHERE a.allocation_id=?1 AND a.instance_id=r.instance_id AND a.package_id=v.package_id \
             AND NOT EXISTS(SELECT 1 FROM service_storage_preparations t WHERE t.allocation_id=a.allocation_id))",
            params![allocation.as_bytes().as_slice(), run.as_bytes().as_slice()], |r| r.get(0),
        ).map_err(|e| PersistenceError::sqlite("qualify admitted service pin custody", e))?;
        if !valid {
            return Err(PersistenceError::CorruptServiceStorage(
                "service pin has missing protection, wrong owner, or no admitted Run",
            ));
        }
        tx.execute(
            "INSERT INTO run_service_storage_pins(run_id,allocation_id) VALUES(?1,?2) ON CONFLICT(run_id,allocation_id) DO NOTHING",
            params![run.as_bytes().as_slice(), allocation.as_bytes().as_slice()],
        )
        .map_err(|e| PersistenceError::sqlite("pin admitted service allocation", e))?;
    }
    Ok(())
}
