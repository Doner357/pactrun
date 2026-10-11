//! Migration Run persistence. Whole-path admission and one atomic edge per call.
use super::sqlite_instances::{
    binding_payload, fresh_state_version, insert_payload, instance_header, load_instance_view_from,
    reclaim_payload_if_unreferenced, revision_identity, state_version, stream_payload,
};
use super::sqlite_revision_store::{FaultPoint, fault, load_revision_from};
use super::sqlite_runs::{
    finish_migration_in_transaction, load_managed_run_from, managed_mutation_conflict,
};
use super::{PactrunPersistence, PersistenceError};
use crate::domain::*;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use std::collections::BTreeSet;

#[path = "sqlite_migration_hook.rs"]
mod hook;

pub(crate) struct MigrationEdgePublication<'a, 'b> {
    pub(crate) target: Option<&'a crate::hook::TargetCommitPermit>,
    pub(crate) operator_inputs: &'a mut [super::ManagedInputWrite<'b>],
    pub(crate) hook_outputs: &'a mut [super::ManagedInputWrite<'b>],
    pub(crate) completion: Option<HookCompletionRecord>,
    pub(crate) secondary_failures: Vec<RunFailureRecord>,
}

#[cfg(test)]
thread_local! { static FAIL_EDGE_ACK:std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(test)]
pub(crate) fn fail_next_edge_ack_for_test() {
    FAIL_EDGE_ACK.with(|flag| flag.set(true));
}

fn corrupt() -> PersistenceError {
    PersistenceError::CorruptRun(
        "Migration invocation, progress, checkpoint or pins disagree".to_owned(),
    )
}
fn rejected(message: &str) -> PersistenceError {
    PersistenceError::InvalidRunTransition(message.to_owned())
}
fn digest(bytes: Vec<u8>) -> Result<RevisionContentDigest, PersistenceError> {
    Ok(RevisionContentDigest::from_bytes(
        bytes.try_into().map_err(|_| corrupt())?,
    ))
}

pub(super) fn insert_invocation(
    tx: &Transaction<'_>,
    run: RunId,
    invocation: &MigrationRunIdentity,
) -> Result<(), PersistenceError> {
    tx.execute(
        "INSERT INTO run_migration_invocations VALUES (?1,?2,?3,?4,?5)",
        params![
            run.as_bytes().as_slice(),
            invocation.source().package_id.as_bytes().as_slice(),
            invocation.source().content_digest.as_bytes().as_slice(),
            invocation.target().content_digest.as_bytes().as_slice(),
            i64::from(invocation.authorized())
        ],
    )
    .map_err(|e| PersistenceError::sqlite("insert Migration invocation", e))?;
    for (index, pair) in invocation.path().windows(2).enumerate() {
        tx.execute(
            "INSERT INTO run_migration_edges VALUES (?1,?2,?3,?4)",
            params![
                run.as_bytes().as_slice(),
                index as i64,
                pair[0].content_digest.as_bytes().as_slice(),
                pair[1].content_digest.as_bytes().as_slice()
            ],
        )
        .map_err(|e| PersistenceError::sqlite("insert exact Migration edge", e))?;
    }
    Ok(())
}

pub(super) fn load_invocation(
    db: &Connection,
    run: RunId,
) -> Result<MigrationRunIdentity, PersistenceError> {
    let (package, source, target, authorize): (Vec<u8>,Vec<u8>,Vec<u8>,i64) = db.query_row("SELECT package_id,source_revision_digest,target_revision_digest,authorize_declassification FROM run_migration_invocations WHERE run_id=?1", [run.as_bytes().as_slice()], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))
        .map_err(|e|PersistenceError::sqlite("read Migration invocation",e))?;
    if !matches!(authorize, 0 | 1) {
        return Err(corrupt());
    }
    let source = revision_identity(package, source)?;
    let target = digest(target)?;
    let mut path = vec![source.clone()];
    let mut query = db.prepare("SELECT edge_index,source_revision_digest,target_revision_digest FROM run_migration_edges WHERE run_id=?1 ORDER BY edge_index").map_err(|e|PersistenceError::sqlite("prepare Migration path",e))?;
    let rows = query
        .query_map([run.as_bytes().as_slice()], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(|e| PersistenceError::sqlite("read Migration path", e))?;
    for row in rows {
        let (index, from, to) =
            row.map_err(|e| PersistenceError::sqlite("decode Migration path", e))?;
        if index != (path.len() - 1) as i64
            || digest(from)? != path.last().expect("source").content_digest
        {
            return Err(corrupt());
        }
        path.push(RevisionIdentity::new(source.package_id, digest(to)?));
    }
    if path.last().expect("source").content_digest != target {
        return Err(corrupt());
    }
    MigrationRunIdentity::new(path, authorize == 1).map_err(|_| corrupt())
}

pub(super) fn progress_view(
    db: &Connection,
    run: RunId,
) -> Result<Option<MigrationRunProgress>, PersistenceError> {
    let row: Option<(i64,i64,Vec<u8>,Vec<u8>)> = db.query_row("SELECT committed_edge_count,step_rank,boundary_revision_digest,boundary_state_version FROM run_migration_progress WHERE run_id=?1",[run.as_bytes().as_slice()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|e|PersistenceError::sqlite("read Migration progress",e))?;
    row.map(|(count, step, revision, version)| {
        let invocation = load_invocation(db, run)?;
        Ok(MigrationRunProgress {
            committed_edges: usize::try_from(count).map_err(|_| corrupt())?,
            step: MigrationPlanStep::from_rank(step).map_err(|_| corrupt())?,
            boundary_revision: RevisionIdentity::new(
                invocation.source().package_id,
                digest(revision)?,
            ),
            boundary_state_version: state_version(version)?,
        })
    })
    .transpose()
}

fn count(db: &Connection, table: &str, run: RunId) -> Result<i64, PersistenceError> {
    db.query_row(
        &format!("SELECT count(*) FROM {table} WHERE run_id=?1"),
        [run.as_bytes().as_slice()],
        |r| r.get(0),
    )
    .map_err(|e| PersistenceError::sqlite("count Migration references", e))
}

pub(super) fn require_no_migration_mutator(
    tx: &Transaction<'_>,
    instance: InstanceId,
) -> Result<(), PersistenceError> {
    super::sqlite_deletions::require_ordinary_lifecycle(tx, instance)?;
    let run:Option<Vec<u8>>=tx.query_row("SELECT r.run_id FROM runs r JOIN run_executions e ON e.run_id=r.run_id JOIN run_revision_pins p ON p.run_id=r.run_id JOIN run_operation_kinds k ON k.run_id=r.run_id WHERE r.instance_id=?1 AND k.operation_kind=3 ORDER BY r.run_id LIMIT 1",[instance.as_bytes().as_slice()],|r|r.get(0)).optional().map_err(|e|PersistenceError::sqlite("check Migration management exclusion",e))?;
    if let Some(run) = run {
        let run = RunId::from_bytes(run.try_into().map_err(|_| corrupt())?);
        load_managed_run_from(tx, run)?;
        return Err(PersistenceError::MigrationMutationConflict(run));
    }
    Ok(())
}

fn registry(
    db: &Connection,
    instance: InstanceId,
) -> Result<Vec<MigrationBinding>, PersistenceError> {
    let view = load_instance_view_from(db, instance)?.ok_or_else(corrupt)?;
    view.bindings
        .into_iter()
        .filter(|b| b.present)
        .map(|b| {
            let (payload, _) = binding_payload(db, instance, &b.input_id)?.ok_or_else(corrupt)?;
            Ok(MigrationBinding {
                input: b.input_id,
                origin: MigrationValueOrigin::Existing(payload),
                protection: b.protection,
            })
        })
        .collect()
}

fn checkpoint(
    db: &Connection,
    run: RunId,
    instance: InstanceId,
) -> Result<Vec<(InputIdentity, ManagedInputPayloadId)>, PersistenceError> {
    let mut query=db.prepare("SELECT instance_id,input_identity,payload_id FROM run_migration_checkpoint_bindings WHERE run_id=?1 ORDER BY input_identity").map_err(|e|PersistenceError::sqlite("prepare Migration checkpoint",e))?;
    query
        .query_map([run.as_bytes().as_slice()], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(|e| PersistenceError::sqlite("read Migration checkpoint", e))?
        .map(|row| {
            let (owner, input, payload) =
                row.map_err(|e| PersistenceError::sqlite("decode Migration checkpoint", e))?;
            if owner != instance.as_bytes() {
                return Err(corrupt());
            }
            Ok((
                InputIdentity::parse(String::from_utf8(input).map_err(|_| corrupt())?)
                    .map_err(|_| corrupt())?,
                ManagedInputPayloadId::from_bytes(payload.try_into().map_err(|_| corrupt())?),
            ))
        })
        .collect()
}

fn registry_refs(
    bindings: &[MigrationBinding],
) -> Result<Vec<(InputIdentity, ManagedInputPayloadId)>, PersistenceError> {
    bindings
        .iter()
        .map(|b| match b.origin {
            MigrationValueOrigin::Existing(payload) => Ok((b.input.clone(), payload)),
            _ => Err(corrupt()),
        })
        .collect()
}

pub(super) fn validate_links(
    db: &Connection,
    run: RunId,
    instance: InstanceId,
    accepted: InstanceStateVersion,
    operation: &ManagedRunIdentity,
    state: &RunState,
) -> Result<(), PersistenceError> {
    let progress = progress_view(db, run)?;
    let tables = [
        "run_migration_boundaries",
        "run_migration_revision_pins",
        "run_migration_payload_pins",
        "run_migration_checkpoint_bindings",
    ];
    let ManagedRunIdentity::Migration(invocation) = operation else {
        if progress.is_some() {
            return Err(corrupt());
        }
        for table in tables {
            if count(db, table, run)? != 0 {
                return Err(corrupt());
            }
        }
        return Ok(());
    };
    let admitted = match state {
        RunState::Running(e) => e.boundary == ActionRunBoundary::Admitted,
        RunState::Finished(e) => e.boundary == ActionRunBoundary::Admitted,
    };
    if !admitted {
        if progress.is_some() {
            return Err(corrupt());
        }
        for table in tables {
            if count(db, table, run)? != 0 {
                return Err(corrupt());
            }
        }
        return Ok(());
    }
    let progress = progress.ok_or_else(corrupt)?;
    if progress.committed_edges > invocation.edge_count() {
        return Err(corrupt());
    }
    let mut previous = accepted;
    let mut seen = BTreeSet::from([accepted]);
    let mut committed = 0;
    let mut query=db.prepare("SELECT edge_index,revision_content_digest,instance_state_version FROM run_migration_boundaries WHERE run_id=?1 ORDER BY edge_index").map_err(|e|PersistenceError::sqlite("prepare edge evidence",e))?;
    let rows = query
        .query_map([run.as_bytes().as_slice()], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(|e| PersistenceError::sqlite("read edge evidence", e))?;
    for row in rows {
        let (index, revision, version) =
            row.map_err(|e| PersistenceError::sqlite("decode edge evidence", e))?;
        if index != committed as i64
            || committed >= invocation.edge_count()
            || digest(revision)? != invocation.path()[committed + 1].content_digest
        {
            return Err(corrupt());
        }
        previous = state_version(version)?;
        if !seen.insert(previous) {
            return Err(corrupt());
        }
        committed += 1;
    }
    if committed != progress.committed_edges
        || progress.boundary_revision != invocation.path()[committed]
        || progress.boundary_state_version != previous
    {
        return Err(corrupt());
    }
    match state {
        RunState::Running(_) => {
            if committed == invocation.edge_count() {
                return Err(corrupt());
            }
            let (_, active, _) = instance_header(db, instance)?;
            if active != progress.boundary_revision
                || checkpoint(db, run, instance)? != registry_refs(&registry(db, instance)?)?
            {
                return Err(corrupt());
            }
            let mut query=db.prepare("SELECT package_id,revision_content_digest FROM run_migration_revision_pins WHERE run_id=?1").map_err(|e|PersistenceError::sqlite("prepare Migration Revision pins",e))?;
            let actual = query
                .query_map([run.as_bytes().as_slice()], |r| {
                    Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
                })
                .map_err(|e| PersistenceError::sqlite("read Migration Revision pins", e))?
                .map(|row| {
                    let (p, d) = row.map_err(|e| {
                        PersistenceError::sqlite("decode Migration Revision pin", e)
                    })?;
                    revision_identity(p, d)
                })
                .collect::<Result<BTreeSet<_>, _>>()?;
            if actual != invocation.path().iter().cloned().collect() {
                return Err(corrupt());
            }
            let bad:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM run_migration_payload_pins p LEFT JOIN managed_input_payloads b ON b.instance_id=p.instance_id AND b.payload_id=p.payload_id WHERE p.run_id=?1 AND (p.instance_id<>?2 OR b.payload_id IS NULL)) OR EXISTS(SELECT 1 FROM run_migration_checkpoint_bindings c WHERE c.run_id=?1 AND NOT EXISTS(SELECT 1 FROM run_migration_payload_pins p WHERE p.run_id=c.run_id AND p.instance_id=c.instance_id AND p.payload_id=c.payload_id))",params![run.as_bytes().as_slice(),instance.as_bytes().as_slice()],|r|r.get(0)).map_err(|e|PersistenceError::sqlite("validate Migration payload roots",e))?;
            if bad {
                return Err(corrupt());
            }
        }
        RunState::Finished(outcome) => {
            for table in &tables[1..] {
                if count(db, table, run)? != 0 {
                    return Err(corrupt());
                }
            }
            if outcome.outcome == RunOutcome::Succeeded {
                if committed != invocation.edge_count()
                    || progress.step != MigrationPlanStep::Finalize
                {
                    return Err(corrupt());
                }
            } else if committed == invocation.edge_count() {
                return Err(corrupt());
            }
        }
    }
    for index in 0..committed {
        if let Some(target) = load_revision_from(db, &invocation.path()[index + 1])? {
            let source =
                Sha256Digest::from_bytes(*invocation.path()[index].content_digest.as_bytes());
            let transformed = target
                .content
                .core
                .service_core()
                .and_then(|c| c.migrations().get(&source))
                .is_some_and(|m| {
                    m.resources
                        .iter()
                        .any(|r| matches!(r, ResourceTransitionV2::Transform { .. }))
                });
            let recorded:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM run_service_edge_commits WHERE run_id=?1 AND edge_index=?2)",params![run.as_bytes().as_slice(),index as i64],|r|r.get(0)).map_err(|e|PersistenceError::sqlite("validate committed service evidence",e))?;
            if transformed != recorded {
                return Err(corrupt());
            }
        }
    }
    let dangling:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM run_service_edge_commits s LEFT JOIN run_migration_boundaries b ON b.run_id=s.run_id AND b.edge_index=s.edge_index WHERE s.run_id=?1 AND b.run_id IS NULL)",[run.as_bytes().as_slice()],|r|r.get(0)).map_err(|e|PersistenceError::sqlite("validate service boundary references",e))?;
    if dangling {
        return Err(corrupt());
    }
    Ok(())
}

pub(super) fn release_references(
    tx: &Transaction<'_>,
    run: RunId,
    instance: InstanceId,
) -> Result<(), PersistenceError> {
    let mut query = tx
        .prepare(
            "SELECT payload_id FROM run_migration_payload_pins WHERE run_id=?1 AND instance_id=?2",
        )
        .map_err(|e| PersistenceError::sqlite("prepare Migration pin release", e))?;
    let payloads = query
        .query_map(
            params![run.as_bytes().as_slice(), instance.as_bytes().as_slice()],
            |r| r.get::<_, Vec<u8>>(0),
        )
        .map_err(|e| PersistenceError::sqlite("read Migration pin release", e))?
        .map(|row| {
            Ok(ManagedInputPayloadId::from_bytes(
                row.map_err(|e| PersistenceError::sqlite("decode released pin", e))?
                    .try_into()
                    .map_err(|_| corrupt())?,
            ))
        })
        .collect::<Result<Vec<_>, PersistenceError>>()?;
    drop(query);
    for table in [
        "run_migration_checkpoint_bindings",
        "run_migration_payload_pins",
        "run_migration_revision_pins",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE run_id=?1"),
            [run.as_bytes().as_slice()],
        )
        .map_err(|e| PersistenceError::sqlite("release Migration references", e))?;
    }
    for payload in payloads {
        reclaim_payload_if_unreferenced(tx, instance, payload)?;
    }
    Ok(())
}

fn keep_checkpoint(
    tx: &Transaction<'_>,
    run: RunId,
    instance: InstanceId,
) -> Result<(), PersistenceError> {
    tx.execute(
        "DELETE FROM run_migration_checkpoint_bindings WHERE run_id=?1",
        [run.as_bytes().as_slice()],
    )
    .map_err(|e| PersistenceError::sqlite("replace Migration checkpoint", e))?;
    tx.execute("INSERT INTO run_migration_checkpoint_bindings SELECT ?1,instance_id,input_identity,payload_id FROM managed_input_bindings WHERE instance_id=?2",params![run.as_bytes().as_slice(),instance.as_bytes().as_slice()]).map_err(|e|PersistenceError::sqlite("publish Migration checkpoint",e))?;
    tx.execute("INSERT OR IGNORE INTO run_migration_payload_pins SELECT ?1,instance_id,payload_id FROM managed_input_bindings WHERE instance_id=?2",params![run.as_bytes().as_slice(),instance.as_bytes().as_slice()]).map_err(|e|PersistenceError::sqlite("pin Migration values",e))?;
    Ok(())
}

fn owner_matches(
    p: &PactrunPersistence,
    owner: &ExecutionOwnerSession,
) -> Result<(), PersistenceError> {
    if p.staging_session().is_none_or(|s| s.owner() != *owner) {
        return Err(rejected(
            "Migration operation requires its live accepting session",
        ));
    }
    Ok(())
}
fn failed(outcome: RunOutcome) -> RunFinish {
    RunFinish {
        outcome,
        primary_failure: None,
        secondary_failures: vec![],
        hook_completion: None,
    }
}
fn invalidated(step: RunFailedStep) -> RunFinish {
    RunFinish {
        outcome: RunOutcome::Failed,
        primary_failure: Some(RunPrimaryFailure {
            cause: None,
            failure: RunFailureRecord {
                error: PactrunErrorRef::new("execution", "migration_publication_rejected")
                    .expect("registered Migration error"),
                message: String::new(),
            },
            step,
        }),
        secondary_failures: vec![],
        hook_completion: None,
    }
}

impl PactrunPersistence {
    pub(crate) fn admit_declarative_migration(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
        plan: &MigrationExecutionPlan,
        override_guard: bool,
    ) -> Result<Result<(), AdmissionRefusal>, PersistenceError> {
        self.admit_migration(run, owner, plan, override_guard, &|_| Ok(()))
    }

    pub(crate) fn admit_migration(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
        plan: &MigrationExecutionPlan,
        override_guard: bool,
        launcher_check: &dyn Fn(&InterpreterLauncherObservation) -> Result<(), String>,
    ) -> Result<Result<(), AdmissionRefusal>, PersistenceError> {
        owner_matches(self, owner)?;
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin Migration admission", e))?;
        self.check_write_admission(&tx)?;
        let view = load_managed_run_from(&tx, run)?;
        let RunState::Running(execution) = &view.state else {
            return Err(PersistenceError::RunNotRunning);
        };
        if execution.owner != *owner || execution.boundary != ActionRunBoundary::Accepted {
            return Err(rejected("Migration is not an owned accepted Run"));
        }
        let ManagedRunIdentity::Migration(invocation) = &view.operation else {
            return Err(rejected("not a Migration Run"));
        };
        let mut service_dependencies = BTreeSet::new();
        let decision = (|| {
            if *invocation != plan.invocation() || view.instance != plan.instance() {
                return Ok(Some(AdmissionRefusal::PlanInvalidated(
                    "Migration plan mismatch or unsupported Hook/operator input".to_owned(),
                )));
            }
            let guarded: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM instance_recovery_guards WHERE instance_id=?1)",
                    [view.instance.as_bytes().as_slice()],
                    |r| r.get(0),
                )
                .map_err(|e| PersistenceError::sqlite("read Migration trust guard", e))?;
            if guarded && !override_guard {
                return Ok(Some(AdmissionRefusal::RecoveryGuardActive));
            }
            if super::sqlite_deletions::obligation_from(&tx, view.instance)?.is_some() {
                return Ok(Some(AdmissionRefusal::DeletionObligation(view.instance)));
            }
            let (_, active, version) = match instance_header(&tx, view.instance) {
                Ok(header) => header,
                Err(PersistenceError::MissingInstance(_)) => {
                    return Ok(Some(AdmissionRefusal::PlanInvalidated(
                        "Instance has been retired".to_owned(),
                    )));
                }
                Err(error) => return Err(error),
            };
            if active != *invocation.source()
                || version != plan.expected_state_version()
                || version != view.accepted_state_version
            {
                return Ok(Some(AdmissionRefusal::PlanInvalidated(
                    "Migration source state changed".to_owned(),
                )));
            }
            let bindings = registry(&tx, view.instance)?;
            if bindings != plan.edges()[0].bindings.source_bindings() {
                return Ok(Some(AdmissionRefusal::PlanInvalidated(
                    "Migration registry changed".to_owned(),
                )));
            }
            for b in &bindings {
                if let MigrationValueOrigin::Existing(id) = b.origin {
                    stream_payload(
                        &tx,
                        &self.runtime_content,
                        view.instance,
                        id,
                        &mut std::io::sink(),
                    )?;
                }
            }
            let mut service_state =
                super::sqlite_service_migrations::state_from(&tx, view.instance)?;
            let mut previous_core = None;
            for (index, identity) in invocation.path().iter().enumerate() {
                let Some(stored) = load_revision_from(&tx, identity)? else {
                    return Ok(Some(AdmissionRefusal::PlanInvalidated(
                        "Migration Revision unavailable".to_owned(),
                    )));
                };
                if self
                    .runtime_content
                    .verify_runtime_content_available(&stored.content.runtime_content)
                    .is_err()
                {
                    return Ok(Some(AdmissionRefusal::PlanInvalidated(
                        "Migration runtime unavailable".to_owned(),
                    )));
                }
                if index > 0 {
                    let compiled = &plan.edges()[index - 1];
                    if let Some(launcher) = compiled
                        .launch
                        .as_ref()
                        .and_then(CompiledHookLaunch::launcher)
                        && launcher_check(launcher).is_err()
                    {
                        return Ok(Some(AdmissionRefusal::PlanInvalidated(
                            "Migration interpreter selection changed".to_owned(),
                        )));
                    }
                    let declaration = stored.content.core.migrations().iter().find(|e| {
                        e.source_revision_digest.to_bytes()
                            == *invocation.path()[index - 1].content_digest.as_bytes()
                    });
                    if declaration != Some(compiled.bindings.declaration())
                        || stored.content.runtime_content.files() != compiled.runtime
                    {
                        return Ok(Some(AdmissionRefusal::PlanInvalidated(
                            "Migration declaration changed".to_owned(),
                        )));
                    }
                    if let Some(before) = &service_state {
                        let evaluated = match evaluate_service_migration_edge(
                            before,
                            previous_core.as_ref().expect("preceding path node"),
                            identity,
                            &stored.content.core,
                            index - 1,
                        ) {
                            Ok(evaluated) => evaluated,
                            Err(_) => {
                                return Ok(Some(AdmissionRefusal::PlanInvalidated(
                                    "Migration service mapping is no longer valid".into(),
                                )));
                            }
                        };
                        if compiled.service.as_ref()
                            != super::sqlite_service_migrations::needed(&evaluated)
                                .then_some(&evaluated)
                        {
                            return Ok(Some(AdmissionRefusal::PlanInvalidated(
                                "Migration service association facts changed".into(),
                            )));
                        }
                        service_dependencies.extend(
                            super::sqlite_service_migrations::existing_dependencies(&evaluated),
                        );
                        service_state = Some(evaluated.after);
                    } else if compiled.service.is_some() {
                        return Ok(Some(AdmissionRefusal::PlanInvalidated(
                            "Migration service observation unavailable".into(),
                        )));
                    }
                }
                previous_core = Some(stored.content.core);
            }
            match self.qualify_service_roots(&tx, view.instance, &service_dependencies) {
                Ok(_) => (),
                Err(PersistenceError::ServiceStorageUnavailable(_)) => {
                    return Ok(Some(AdmissionRefusal::PlanInvalidated(
                        "Migration source allocation unavailable".into(),
                    )));
                }
                Err(error) => return Err(error),
            }
            if let Some(other) = managed_mutation_conflict(&tx, view.instance, run)? {
                return Ok(Some(AdmissionRefusal::MutationConflict(other)));
            }
            Ok::<_, PersistenceError>(None)
        })()?;
        let result = if let Some(refusal) = decision {
            let finish = RunFinish {
                outcome: RunOutcome::Failed,
                primary_failure: Some(refusal.primary_failure()),
                secondary_failures: vec![],
                hook_completion: None,
            };
            finish_migration_in_transaction(&tx, run, owner, &finish, false)?;
            Err(refusal)
        } else {
            tx.execute(
                "INSERT INTO run_revision_pins VALUES (?1,?2,?3)",
                params![
                    run.as_bytes().as_slice(),
                    invocation.source().package_id.as_bytes().as_slice(),
                    invocation.source().content_digest.as_bytes().as_slice()
                ],
            )
            .map_err(|e| PersistenceError::sqlite("anchor Migration admission", e))?;
            super::sqlite_service_admission::pin_service_allocations(
                &tx,
                run,
                &service_dependencies,
            )?;
            for identity in invocation.path() {
                tx.execute(
                    "INSERT INTO run_migration_revision_pins VALUES (?1,?2,?3)",
                    params![
                        run.as_bytes().as_slice(),
                        identity.package_id.as_bytes().as_slice(),
                        identity.content_digest.as_bytes().as_slice()
                    ],
                )
                .map_err(|e| PersistenceError::sqlite("pin Migration path", e))?;
            }
            tx.execute(
                "INSERT INTO run_migration_progress VALUES (?1,0,0,?2,?3)",
                params![
                    run.as_bytes().as_slice(),
                    invocation.source().content_digest.as_bytes().as_slice(),
                    view.accepted_state_version.as_bytes().as_slice()
                ],
            )
            .map_err(|e| PersistenceError::sqlite("establish Migration boundary", e))?;
            keep_checkpoint(&tx, run, view.instance)?;
            load_managed_run_from(&tx, run)?;
            Ok(())
        };
        fault(FaultPoint::BeforeRunAdmitCommit);
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit Migration admission", e))?;
        fault(FaultPoint::AfterRunAdmitCommit);
        Ok(result)
    }

    pub(crate) fn publish_declarative_migration_edge(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
        edge_index: usize,
    ) -> Result<MigrationRunProgress, PersistenceError> {
        self.publish_migration_edge_inputs(run, owner, edge_index, &mut [])
    }

    pub(crate) fn publish_migration_edge_inputs(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
        edge_index: usize,
        inputs: &mut [super::ManagedInputWrite<'_>],
    ) -> Result<MigrationRunProgress, PersistenceError> {
        self.publish_migration_edge(
            run,
            owner,
            edge_index,
            &mut MigrationEdgePublication {
                target: None,
                operator_inputs: inputs,
                hook_outputs: &mut [],
                completion: None,
                secondary_failures: vec![],
            },
        )
    }

    pub(crate) fn publish_migration_edge(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
        edge_index: usize,
        publication: &mut MigrationEdgePublication<'_, '_>,
    ) -> Result<MigrationRunProgress, PersistenceError> {
        let inputs = &mut *publication.operator_inputs;
        owner_matches(self, owner)?;
        let mut db = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("begin Migration edge publication", e))?;
        self.check_write_admission(&tx)?;
        let view = load_managed_run_from(&tx, run)?;
        let ManagedRunIdentity::Migration(invocation) = &view.operation else {
            return Err(rejected("not a Migration Run"));
        };
        let progress =
            progress_view(&tx, run)?.ok_or_else(|| rejected("Migration is not admitted"))?;
        if edge_index < progress.committed_edges {
            return Ok(progress);
        } // Read-only acknowledgment after uncertain commit.
        let RunState::Running(execution) = &view.state else {
            return Err(PersistenceError::RunNotRunning);
        };
        if execution.owner != *owner
            || execution.boundary != ActionRunBoundary::Admitted
            || edge_index != progress.committed_edges
        {
            return Err(rejected("Migration edge or owner mismatch"));
        }
        let (_, active, current) = instance_header(&tx, view.instance)?;
        let target_permitted = publication
            .target
            .is_some_and(|permit| permit.matches(run, owner, edge_index));
        if active != progress.boundary_revision
            || current != progress.boundary_state_version
            || (execution.risk_state != RecoveryRiskState::Clear && !target_permitted)
        {
            finish_migration_in_transaction(
                &tx,
                run,
                owner,
                &invalidated(RunFailedStep::MigrationPlan(
                    MigrationPlanStep::PublishManagedResult,
                )),
                false,
            )?;
            tx.commit()
                .map_err(|e| PersistenceError::sqlite("record Migration publication refusal", e))?;
            return Err(rejected(
                "Migration boundary changed or recovery risk remains open; no target was published",
            ));
        }
        let source =
            load_revision_from(&tx, &invocation.path()[edge_index])?.ok_or_else(corrupt)?;
        let target =
            load_revision_from(&tx, &invocation.path()[edge_index + 1])?.ok_or_else(corrupt)?;
        let service = super::sqlite_service_migrations::state_from(&tx, view.instance)?
            .map(|before| {
                evaluate_service_migration_edge(
                    &before,
                    &source.content.core,
                    &invocation.path()[edge_index + 1],
                    &target.content.core,
                    edge_index,
                )
            })
            .transpose()
            .map_err(|_| rejected("Migration service mapping is invalid"))?;
        let service = service
            .map(|edge| {
                if super::sqlite_service_migrations::needed(&edge) {
                    super::sqlite_service_targets::load_prepared(&tx, run, edge_index, &edge)
                } else {
                    Ok(edge)
                }
            })
            .transpose()?;
        let transforming = service.as_ref().is_some_and(|edge| edge.transform);
        if transforming != publication.target.is_some()
            || (transforming
                && (!target_permitted
                    || execution.risk_state != RecoveryRiskState::Open
                    || publication.completion.is_some()))
        {
            return Err(rejected(
                "transform publication requires its owner-held target permit and Open risk",
            ));
        }
        let bindings = registry(&tx, view.instance)?;
        let operators: BTreeSet<_> = inputs
            .iter()
            .map(|input| MigrationTargetInput {
                revision: invocation.path()[edge_index + 1].content_digest,
                input: input.input_id.clone(),
            })
            .collect();
        if operators.len() != inputs.len() {
            return Err(rejected("duplicate Migration operator input"));
        }
        let evaluated = evaluate_migration_edge(
            &MigrationRevision {
                identity: invocation.path()[edge_index].clone(),
                content: source.content,
            },
            &MigrationRevision {
                identity: invocation.path()[edge_index + 1].clone(),
                content: target.content,
            },
            &bindings,
            &operators,
            invocation.authorized(),
        )
        .map_err(|_| rejected("Migration edge completion is invalid"))?;
        if evaluated.declaration().hook.is_some() {
            if !transforming
                && publication.completion.as_ref().map(|c| c.status)
                    != Some(HookCompletionStatus::Success)
            {
                return Err(rejected("Migration Hook success is required"));
            }
        } else if publication.completion.is_some() || !publication.hook_outputs.is_empty() {
            return Err(rejected("declarative Migration has no output authority"));
        }
        validate_migration_completion(
            evaluated.declaration(),
            true,
            &publication
                .hook_outputs
                .iter()
                .map(|i| i.input_id.clone())
                .collect::<Vec<_>>(),
        )
        .map_err(|_| rejected("Migration output set is invalid"))?;
        if transforming {
            tx.execute("UPDATE run_executions SET risk_state=0 WHERE run_id=?1 AND owner_session=?2 AND risk_state=1",
                params![run.as_bytes().as_slice(),owner.as_str().as_bytes()]).map_err(|e|PersistenceError::sqlite("clear transform risk with target commit",e))?;
        }
        let final_edge = edge_index + 1 == invocation.edge_count();
        if final_edge {
            let mut finish = failed(RunOutcome::Succeeded);
            finish.hook_completion = publication.completion.clone();
            finish.secondary_failures = publication.secondary_failures.clone();
            finish_migration_in_transaction(&tx, run, owner, &finish, true)?;
        }
        let mut outputs = Vec::new();
        for binding in evaluated.committed_bindings() {
            if let MigrationValueOrigin::Hook(target) = &binding.origin {
                let input = publication
                    .hook_outputs
                    .iter_mut()
                    .find(|i| i.input_id == target.input)
                    .ok_or_else(|| rejected("Migration output is absent"))?;
                let payload = insert_payload(&tx, view.instance, binding.protection, input)?;
                outputs.push((binding.input.clone(), payload));
                continue;
            }
            if let MigrationValueOrigin::Operator(target) = &binding.origin {
                let input = inputs
                    .iter_mut()
                    .find(|i| i.input_id == target.input)
                    .ok_or_else(|| rejected("Migration operator input is absent"))?;
                let payload = insert_payload(&tx, view.instance, binding.protection, input)?;
                outputs.push((binding.input.clone(), payload));
                continue;
            }
            let MigrationValueOrigin::Existing(payload) = binding.origin else {
                return Err(rejected("unsupported declarative output"));
            };
            stream_payload(
                &tx,
                &self.runtime_content,
                view.instance,
                payload,
                &mut std::io::sink(),
            )?;
            let stored:i64=tx.query_row("SELECT protection_rank FROM managed_input_payloads WHERE instance_id=?1 AND payload_id=?2",params![view.instance.as_bytes().as_slice(),payload.as_bytes().as_slice()],|r|r.get(0)).map_err(|e|PersistenceError::sqlite("read Migration protection floor",e))?;
            let payload = if stored == i64::from(binding.protection.rank()) {
                payload
            } else {
                let copy = ManagedInputPayloadId::generate().map_err(|_| corrupt())?;
                tx.execute("INSERT INTO managed_input_payloads SELECT instance_id,?3,?4,byte_length,content_digest FROM managed_input_payloads WHERE instance_id=?1 AND payload_id=?2",params![view.instance.as_bytes().as_slice(),payload.as_bytes().as_slice(),copy.as_bytes().as_slice(),i64::from(binding.protection.rank())]).map_err(|e|PersistenceError::sqlite("materialize changed protection",e))?;
                tx.execute("INSERT INTO managed_input_payload_chunks SELECT instance_id,?3,chunk_index,chunk_bytes FROM managed_input_payload_chunks WHERE instance_id=?1 AND payload_id=?2",params![view.instance.as_bytes().as_slice(),payload.as_bytes().as_slice(),copy.as_bytes().as_slice()]).map_err(|e|PersistenceError::sqlite("copy immutable Migration payload",e))?;
                copy
            };
            outputs.push((binding.input.clone(), payload));
        }
        tx.execute(
            "DELETE FROM managed_input_bindings WHERE instance_id=?1",
            [view.instance.as_bytes().as_slice()],
        )
        .map_err(|e| PersistenceError::sqlite("replace Migration registry", e))?;
        for (input, payload) in outputs {
            tx.execute(
                "INSERT INTO managed_input_bindings VALUES (?1,?2,?3)",
                params![
                    view.instance.as_bytes().as_slice(),
                    input.as_str().as_bytes(),
                    payload.as_bytes().as_slice()
                ],
            )
            .map_err(|e| PersistenceError::sqlite("publish Migration binding", e))?;
        }
        let next = fresh_state_version()?;
        let _service_roots = service
            .as_ref()
            .map(|edge| {
                self.publish_existing_service_edge(
                    &tx,
                    view.instance,
                    edge,
                    evaluated.declaration().hook.is_some(),
                )
            })
            .transpose()?;
        if service.is_some() {
            tx.execute(
                "DELETE FROM run_service_resource_targets WHERE run_id=?1 AND edge_index=?2",
                params![run.as_bytes().as_slice(), edge_index as i64],
            )
            .map_err(|e| {
                PersistenceError::sqlite("release published resource target selections", e)
            })?;
            tx.execute(
                "DELETE FROM run_service_storage_targets WHERE run_id=?1 AND edge_index=?2",
                params![run.as_bytes().as_slice(), edge_index as i64],
            )
            .map_err(|e| {
                PersistenceError::sqlite("release published storage target selections", e)
            })?;
        }
        tx.execute("UPDATE instances SET active_revision_content_digest=?2,instance_state_version=?3 WHERE instance_id=?1",params![view.instance.as_bytes().as_slice(),evaluated.target().content_digest.as_bytes().as_slice(),next.as_bytes().as_slice()]).map_err(|e|PersistenceError::sqlite("publish Migration target",e))?;
        tx.execute(
            "INSERT INTO run_migration_boundaries VALUES (?1,?2,?3,?4)",
            params![
                run.as_bytes().as_slice(),
                edge_index as i64,
                evaluated.target().content_digest.as_bytes().as_slice(),
                next.as_bytes().as_slice()
            ],
        )
        .map_err(|e| PersistenceError::sqlite("publish edge evidence", e))?;
        if transforming {
            tx.execute(
                "INSERT INTO run_service_edge_commits VALUES(?1,?2)",
                params![run.as_bytes().as_slice(), edge_index as i64],
            )
            .map_err(|e| PersistenceError::sqlite("record committed service transform", e))?;
        }
        tx.execute("UPDATE run_migration_progress SET committed_edge_count=?2,step_rank=?3,boundary_revision_digest=?4,boundary_state_version=?5 WHERE run_id=?1",params![run.as_bytes().as_slice(),(edge_index+1) as i64,if final_edge {4}else{0},evaluated.target().content_digest.as_bytes().as_slice(),next.as_bytes().as_slice()]).map_err(|e|PersistenceError::sqlite("advance recovery boundary",e))?;
        if !final_edge {
            keep_checkpoint(&tx, run, view.instance)?;
        }
        for (_, payload) in registry_refs(&bindings)? {
            reclaim_payload_if_unreferenced(&tx, view.instance, payload)?;
        }
        load_managed_run_from(&tx, run)?;
        let result = progress_view(&tx, run)?.ok_or_else(corrupt)?;
        edge_fault(FaultPoint::BeforeMigrationEdgeCommit, edge_index);
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("commit Migration edge", e))?;
        #[cfg(test)]
        if FAIL_EDGE_ACK.with(|flag| flag.replace(false)) {
            return Err(PersistenceError::DatabaseLockPoisoned);
        }
        edge_fault(FaultPoint::AfterMigrationEdgeCommit, edge_index);
        Ok(result)
    }
}

fn edge_fault(point: FaultPoint, edge: usize) {
    #[cfg(test)]
    if std::env::var("PACTRUN_MIGRATION_FAULT_EDGE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .is_some_and(|selected| selected != edge)
    {
        return;
    }
    let _ = edge;
    fault(point);
}

#[cfg(test)]
#[path = "sqlite_migration_run_tests.rs"]
mod tests;
