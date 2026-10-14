//! Non-pinning provenance: these rows never participate in content reachability.
use super::{PactrunPersistence, PersistenceError};
use crate::domain::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::collections::{BTreeMap, BTreeSet};

fn invalid() -> PersistenceError {
    PersistenceError::CorruptRun("invalid historical Input context".into())
}

fn validate_context(
    db: &Connection,
    run: RunId,
    context: &HistoricalInputContext,
) -> Result<(), PersistenceError> {
    let kind: i64 = db
        .query_row(
            "SELECT operation_kind FROM run_operation_kinds WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .map_err(|_| invalid())?;
    let allowed = match context.scope {
        InputHistoryScope::Current => {
            matches!(kind, 0 | 1 | 4)
                && context.edge_index.is_none()
                && context.snapshot.is_none()
                && context.coverage
                    == if kind == 0 {
                        InputHistoryCoverage::AdmittedBindings
                    } else {
                        InputHistoryCoverage::ManagedContext
                    }
        }
        InputHistoryScope::Snapshot => {
            kind == 2
                && context.edge_index.is_none()
                && context.snapshot.is_some()
                && context.coverage == InputHistoryCoverage::ManagedContext
        }
        _ => {
            kind == 3
                && context.edge_index.is_some()
                && context.snapshot.is_none()
                && context.coverage == InputHistoryCoverage::ManagedContext
        }
    };
    if !allowed || context.snapshot.is_some() != context.snapshot_integrity.is_some() {
        return Err(invalid());
    }
    if let Some(edge) = context.edge_index {
        let edge = i64::try_from(edge).map_err(|_| invalid())?;
        let exists:bool = db.query_row("SELECT EXISTS(SELECT 1 FROM run_migration_edges WHERE run_id=?1 AND edge_index=?2)", params![run.as_bytes().as_slice(),edge], |r|r.get(0)).map_err(|_|invalid())?;
        if !exists {
            return Err(invalid());
        }
    }
    if let Some(snapshot) = context.snapshot {
        let matches:bool = db.query_row("SELECT EXISTS(SELECT 1 FROM run_restore_invocations WHERE run_id=?1 AND snapshot_id=?2)", params![run.as_bytes().as_slice(),snapshot.as_bytes().as_slice()], |r|r.get(0)).map_err(|_|invalid())?;
        if !matches {
            return Err(invalid());
        }
    }
    let mut seen = BTreeSet::new();
    for input in &context.inputs {
        if !seen.insert(input.input.clone()) {
            return Err(invalid());
        }
        if input.role == InputHistoryRole::Retained
            && matches!(
                input.origin,
                InputHistoryOrigin::Absent
                    | InputHistoryOrigin::Transient
                    | InputHistoryOrigin::PendingOutput
            )
        {
            return Err(invalid());
        }
        if context.coverage == InputHistoryCoverage::AdmittedBindings
            && (input.role != InputHistoryRole::Active
                || !matches!(input.origin, InputHistoryOrigin::Managed(_)))
        {
            return Err(invalid());
        }
        if matches!(input.origin, InputHistoryOrigin::Snapshot)
            && context.scope != InputHistoryScope::Snapshot
        {
            return Err(invalid());
        }
        if context.scope == InputHistoryScope::Snapshot
            && !matches!(
                input.origin,
                InputHistoryOrigin::Snapshot | InputHistoryOrigin::Absent
            )
        {
            return Err(invalid());
        }
        if matches!(
            input.origin,
            InputHistoryOrigin::Transient | InputHistoryOrigin::PendingOutput
        ) && context.scope != InputHistoryScope::TargetPrepared
        {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(super) fn record(
    db: &Connection,
    run: RunId,
    mut context: HistoricalInputContext,
) -> Result<(), PersistenceError> {
    context
        .inputs
        .sort_by(|a, b| (&a.input, a.role.rank()).cmp(&(&b.input, b.role.rank())));
    validate_context(db, run, &context)?;
    let edge = context.edge_index.map(|v| v as i64).unwrap_or(-1);
    let existing = read_context(db, run, edge, context.scope)?;
    if let Some(existing) = existing {
        return if existing == context {
            Ok(())
        } else {
            Err(invalid())
        };
    }
    let integrity = context.snapshot_integrity.as_ref().map(|digest| {
        hex::decode(&digest.as_str()[7..]).expect("validated Snapshot integrity identity")
    });
    db.execute(
        "INSERT INTO run_input_contexts VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            run.as_bytes().as_slice(),
            edge,
            context.scope.rank(),
            context.coverage.rank(),
            context.snapshot.as_ref().map(|id| id.as_bytes().as_slice()),
            integrity
        ],
    )
    .map_err(|e| PersistenceError::sqlite("record historical Input context", e))?;
    for input in &context.inputs {
        let payload = input.origin.payload();
        db.execute(
            "INSERT INTO run_input_references VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                run.as_bytes().as_slice(),
                edge,
                context.scope.rank(),
                input.input.as_str().as_bytes(),
                input.role.rank(),
                i64::from(input.protection.rank()),
                input.origin.rank(),
                payload.as_ref().map(|id| id.as_bytes().as_slice())
            ],
        )
        .map_err(|e| PersistenceError::sqlite("record historical Input reference", e))?;
    }
    Ok(())
}

fn read_context(
    db: &Connection,
    run: RunId,
    edge: i64,
    scope: InputHistoryScope,
) -> Result<Option<HistoricalInputContext>, PersistenceError> {
    let header = db.query_row("SELECT coverage_rank,CASE WHEN snapshot_id IS NULL OR length(snapshot_id)=16 THEN snapshot_id ELSE X'' END,CASE WHEN snapshot_integrity IS NULL OR length(snapshot_integrity)=32 THEN snapshot_integrity ELSE X'' END FROM run_input_contexts WHERE run_id=?1 AND edge_index=?2 AND scope_rank=?3", params![run.as_bytes().as_slice(),edge,scope.rank()], |r|Ok((r.get::<_,i64>(0)?,r.get::<_,Option<Vec<u8>>>(1)?,r.get::<_,Option<Vec<u8>>>(2)?))).optional().map_err(|e|PersistenceError::sqlite("read historical Input context",e))?;
    let Some((coverage, snapshot, integrity)) = header else {
        return Ok(None);
    };
    let mut context = HistoricalInputContext {
        snapshot_integrity: integrity
            .map(|bytes| {
                bytes
                    .try_into()
                    .map(SnapshotIntegrityDigest::from_bytes)
                    .map_err(|_| invalid())
            })
            .transpose()?,
        edge_index: if edge == -1 {
            None
        } else {
            Some(usize::try_from(edge).map_err(|_| invalid())?)
        },
        scope,
        coverage: match coverage {
            0 => InputHistoryCoverage::AdmittedBindings,
            1 => InputHistoryCoverage::ManagedContext,
            _ => return Err(invalid()),
        },
        snapshot: snapshot
            .map(|bytes| {
                bytes
                    .try_into()
                    .map(SnapshotId::from_bytes)
                    .map_err(|_| invalid())
            })
            .transpose()?,
        inputs: vec![],
    };
    let mut query=db.prepare("SELECT CASE WHEN length(input_id)<=128 THEN input_id ELSE X'' END,role_rank,protection_rank,origin_rank,CASE WHEN payload_id IS NULL OR length(payload_id)=16 THEN payload_id ELSE X'' END FROM run_input_references WHERE run_id=?1 AND edge_index=?2 AND scope_rank=?3 ORDER BY input_id,role_rank").map_err(|e|PersistenceError::sqlite("prepare historical Input read",e))?;
    let rows = query
        .query_map(
            params![run.as_bytes().as_slice(), edge, scope.rank()],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, Option<Vec<u8>>>(4)?,
                ))
            },
        )
        .map_err(|e| PersistenceError::sqlite("query historical Inputs", e))?;
    for row in rows {
        let (id, role, protection, origin, payload) =
            row.map_err(|e| PersistenceError::sqlite("read historical Input", e))?;
        let origin = match (origin, payload) {
            (0, None) => InputHistoryOrigin::Absent,
            (1, Some(bytes)) => InputHistoryOrigin::Managed(ManagedInputPayloadId::from_bytes(
                bytes.try_into().map_err(|_| invalid())?,
            )),
            (2, None) => InputHistoryOrigin::Snapshot,
            (3, None) => InputHistoryOrigin::Transient,
            (4, None) => InputHistoryOrigin::PendingOutput,
            _ => return Err(invalid()),
        };
        context.inputs.push(HistoricalInput {
            input: InputIdentity::parse(String::from_utf8(id).map_err(|_| invalid())?)
                .map_err(|_| invalid())?,
            role: match role {
                0 => InputHistoryRole::Active,
                1 => InputHistoryRole::Retained,
                _ => return Err(invalid()),
            },
            protection: ManagedInputProtection::from_rank(protection).map_err(|_| invalid())?,
            origin,
        });
    }
    validate_context(db, run, &context)?;
    Ok(Some(context))
}

pub(super) fn load(
    db: &Connection,
    run: RunId,
) -> Result<Vec<HistoricalInputContext>, PersistenceError> {
    let orphaned:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM run_input_references r LEFT JOIN run_input_contexts c USING(run_id,edge_index,scope_rank) WHERE r.run_id=?1 AND c.run_id IS NULL)",[run.as_bytes().as_slice()],|r|r.get(0)).map_err(|e|PersistenceError::sqlite("validate historical Input ownership",e))?;
    if orphaned {
        return Err(invalid());
    }
    let mut query=db.prepare("SELECT edge_index,scope_rank FROM run_input_contexts WHERE run_id=?1 ORDER BY edge_index,scope_rank").map_err(|e|PersistenceError::sqlite("enumerate historical Input contexts",e))?;
    let keys = query
        .query_map([run.as_bytes().as_slice()], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(|e| PersistenceError::sqlite("query historical Input contexts", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| PersistenceError::sqlite("read historical Input context key", e))?;
    keys.into_iter()
        .map(|(edge, scope)| {
            read_context(
                db,
                run,
                edge,
                InputHistoryScope::from_rank(scope).ok_or_else(invalid)?,
            )?
            .ok_or_else(invalid)
        })
        .collect()
}

impl PactrunPersistence {
    pub(crate) fn input_history(
        &self,
        run: RunId,
    ) -> Result<Vec<HistoricalInputContext>, PersistenceError> {
        let mut db = self.open_read_connection()?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("begin Input history read", e))?;
        super::sqlite_runs::load_managed_run_from(&tx, run)?;
        load(&tx, run)
    }
}

fn declared_protection(input: &InputDeclarationV1) -> ManagedInputProtection {
    match input.protection {
        InputProtectionV1::Normal => ManagedInputProtection::Normal,
        InputProtectionV1::Secret => ManagedInputProtection::Secret,
    }
}
fn complete_context(
    declarations: &[InputDeclarationV1],
    bindings: impl IntoIterator<Item = (InputIdentity, InputHistoryOrigin, ManagedInputProtection)>,
    include_absent: bool,
) -> Vec<HistoricalInput> {
    let mut values: BTreeMap<_, _> = bindings
        .into_iter()
        .map(|(id, origin, protection)| (id, (origin, protection)))
        .collect();
    let mut rows = Vec::new();
    for declaration in declarations {
        if let Some((origin, protection)) = values.remove(&declaration.id) {
            rows.push(HistoricalInput {
                input: declaration.id.clone(),
                role: InputHistoryRole::Active,
                protection,
                origin,
            });
        } else if include_absent {
            rows.push(HistoricalInput {
                input: declaration.id.clone(),
                role: InputHistoryRole::Active,
                protection: declared_protection(declaration),
                origin: InputHistoryOrigin::Absent,
            });
        }
    }
    rows.extend(
        values
            .into_iter()
            .map(|(input, (origin, protection))| HistoricalInput {
                input,
                role: InputHistoryRole::Retained,
                protection,
                origin,
            }),
    );
    rows
}

pub(super) fn record_pins(
    db: &Connection,
    run: RunId,
    revision: &RevisionIdentity,
    complete: bool,
) -> Result<(), PersistenceError> {
    let stored =
        super::sqlite_revision_store::load_revision_from(db, revision)?.ok_or_else(invalid)?;
    let mut query=db.prepare("SELECT b.input_identity,b.payload_id,p.protection_rank FROM run_payload_pins b JOIN managed_input_payloads p ON p.instance_id=b.instance_id AND p.payload_id=b.payload_id WHERE b.run_id=?1 ORDER BY b.input_identity").map_err(|e|PersistenceError::sqlite("read admitted history pins",e))?;
    let rows = query
        .query_map([run.as_bytes().as_slice()], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| PersistenceError::sqlite("query admitted history pins", e))?;
    let mut bindings = Vec::new();
    for row in rows {
        let (id, payload, protection) =
            row.map_err(|e| PersistenceError::sqlite("read admitted history pin", e))?;
        bindings.push((
            InputIdentity::parse(String::from_utf8(id).map_err(|_| invalid())?)
                .map_err(|_| invalid())?,
            InputHistoryOrigin::Managed(ManagedInputPayloadId::from_bytes(
                payload.try_into().map_err(|_| invalid())?,
            )),
            ManagedInputProtection::from_rank(protection).map_err(|_| invalid())?,
        ));
    }
    record(
        db,
        run,
        HistoricalInputContext {
            edge_index: None,
            scope: InputHistoryScope::Current,
            coverage: if complete {
                InputHistoryCoverage::ManagedContext
            } else {
                InputHistoryCoverage::AdmittedBindings
            },
            snapshot: None,
            snapshot_integrity: None,
            inputs: complete_context(stored.content.core.inputs(), bindings, complete),
        },
    )
}

pub(super) fn record_snapshot(
    db: &Connection,
    run: RunId,
    snapshot: SnapshotId,
) -> Result<(), PersistenceError> {
    let (integrity, bindings) = super::sqlite_snapshots::history_bindings(db, snapshot)?;
    let inputs = bindings
        .into_iter()
        .map(|b| HistoricalInput {
            input: b.input_id,
            role: match b.role {
                SnapshotBindingRole::Active => InputHistoryRole::Active,
                SnapshotBindingRole::Retained => InputHistoryRole::Retained,
            },
            protection: b.protection,
            origin: match b.state {
                SnapshotBindingState::Absent => InputHistoryOrigin::Absent,
                SnapshotBindingState::Bound(_) => InputHistoryOrigin::Snapshot,
            },
        })
        .collect();
    record(
        db,
        run,
        HistoricalInputContext {
            edge_index: None,
            scope: InputHistoryScope::Snapshot,
            coverage: InputHistoryCoverage::ManagedContext,
            snapshot: Some(snapshot),
            snapshot_integrity: Some(integrity),
            inputs,
        },
    )
}

pub(super) fn record_migration(
    db: &Connection,
    run: RunId,
    edge: usize,
    scope: InputHistoryScope,
    declarations: &[InputDeclarationV1],
    bindings: &[MigrationBinding],
    pending_outputs: &[InputIdentity],
) -> Result<(), PersistenceError> {
    let mut bindings: Vec<_> = bindings
        .iter()
        .map(|b| {
            (
                b.input.clone(),
                match b.origin {
                    MigrationValueOrigin::Existing(id) => InputHistoryOrigin::Managed(id),
                    MigrationValueOrigin::Operator(_) => InputHistoryOrigin::Transient,
                    MigrationValueOrigin::Hook(_) => InputHistoryOrigin::PendingOutput,
                },
                b.protection,
            )
        })
        .collect();
    for id in pending_outputs {
        if scope != InputHistoryScope::TargetPrepared {
            return Err(invalid());
        }
        if !bindings.iter().any(|(input, _, _)| input == id) {
            let declaration = declarations
                .iter()
                .find(|d| &d.id == id)
                .ok_or_else(invalid)?;
            bindings.push((
                id.clone(),
                InputHistoryOrigin::PendingOutput,
                declared_protection(declaration),
            ));
        }
    }
    record(
        db,
        run,
        HistoricalInputContext {
            edge_index: Some(edge),
            scope,
            coverage: InputHistoryCoverage::ManagedContext,
            snapshot: None,
            snapshot_integrity: None,
            inputs: complete_context(declarations, bindings, true),
        },
    )
}

#[cfg(test)]
#[path = "sqlite_input_history_tests.rs"]
mod tests;
