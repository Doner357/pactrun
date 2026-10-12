//! Persisted Run record decoding and relational integrity validation.

use super::*;

pub(super) fn unix_ms_now() -> Result<i64, PersistenceError> {
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

pub(super) fn run_id(bytes: Vec<u8>) -> Result<RunId, PersistenceError> {
    bytes
        .try_into()
        .map(RunId::from_bytes)
        .map_err(|_| PersistenceError::CorruptRun("invalid RunId length".to_owned()))
}

fn timestamp(value: i64) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::CorruptRun("negative timestamp".to_owned()))
}

pub(super) fn run_header(database: &Connection, run: RunId) -> Result<RunHeader, PersistenceError> {
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
    super::super::writer_admission::validate_run_operation(database, run)?;
    let instance = instance_id(row.0)?;
    let live: bool = database
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM instances WHERE instance_id=?1)",
            [instance.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .map_err(|e| PersistenceError::sqlite("qualify historical Run Instance", e))?;
    if live {
        super::super::writer_admission::load_consequence_version(database, instance)?;
    } else {
        let retired: bool = database.query_row(
            "SELECT EXISTS(SELECT 1 FROM instance_retirement_receipts t JOIN instance_history_identities h ON h.instance_id=t.instance_id JOIN runs r ON r.run_id=t.retirement_run_id JOIN run_outcomes o ON o.run_id=r.run_id JOIN run_deletion_invocations d ON d.run_id=r.run_id JOIN run_operation_kinds k ON k.run_id=r.run_id WHERE t.instance_id=?1 AND r.instance_id=t.instance_id AND o.outcome_rank=0 AND k.operation_kind=4 AND d.deletion_mode=t.retirement_mode)",
            [instance.as_bytes().as_slice()], |r| r.get(0),
        ).map_err(|_| PersistenceError::CorruptRun("missing live Instance or valid retirement evidence".to_owned()))?;
        if !retired {
            return Err(PersistenceError::CorruptRun(
                "missing live Instance or valid retirement evidence".to_owned(),
            ));
        }
    }
    Ok(RunHeader {
        instance,
        accepted_state_version: state_version(row.1)?,
        accepted_at_unix_ms: timestamp(row.2)?,
    })
}

pub(super) fn insert_managed_invocation(
    transaction: &Transaction<'_>,
    run: RunId,
    operation: &ManagedRunIdentity,
) -> Result<(), PersistenceError> {
    if let ManagedRunIdentity::Migration(invocation) = operation {
        return super::super::sqlite_migration_runs::insert_invocation(
            transaction,
            run,
            invocation,
        );
    }
    let revision = operation.revision();
    let common = (
        run.as_bytes().as_slice(),
        revision.package_id.as_bytes().as_slice(),
        revision.content_digest.as_bytes().as_slice(),
    );
    let result = match operation {
        ManagedRunIdentity::Deletion { mode, .. } => transaction.execute(
            "INSERT INTO run_deletion_invocations(run_id,package_id,revision_content_digest,deletion_mode) VALUES (?1,?2,?3,?4)",
            params![common.0,common.1,common.2,mode.rank()]),
        ManagedRunIdentity::Migration(_) => unreachable!("handled exact Migration invocation"),
        ManagedRunIdentity::Action(action) => transaction.execute(
            "INSERT INTO run_action_invocations(run_id,package_id,revision_content_digest,action_identity) VALUES (?1,?2,?3,?4)",
            params![common.0,common.1,common.2,action.action.as_str().as_bytes()]),
        ManagedRunIdentity::Capture { .. } => transaction.execute(
            "INSERT INTO run_capture_invocations(run_id,package_id,revision_content_digest) VALUES (?1,?2,?3)",
            params![common.0,common.1,common.2]),
        ManagedRunIdentity::Restore { snapshot, .. } => transaction.execute(
            "INSERT INTO run_restore_invocations(run_id,package_id,revision_content_digest,snapshot_id) VALUES (?1,?2,?3,?4)",
            params![common.0,common.1,common.2,snapshot.as_bytes().as_slice()]),
    };
    result
        .map(|_| ())
        .map_err(|error| PersistenceError::sqlite("insert managed Run invocation", error))
}

pub(super) fn managed_invocation_row(
    database: &Connection,
    run: RunId,
) -> Result<ManagedRunIdentity, PersistenceError> {
    use crate::domain::ManagedExecutionKind;
    let kind = super::super::writer_admission::validate_run_operation(database, run)?;
    if kind == ManagedExecutionKind::Deletion {
        let (package, digest, mode) = database.query_row(
            "SELECT package_id, revision_content_digest, deletion_mode FROM run_deletion_invocations WHERE run_id=?1",
            [run.as_bytes().as_slice()], |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?, row.get::<_, i64>(2)?)),
        ).map_err(|error| PersistenceError::sqlite("load deletion invocation", error))?;
        return Ok(ManagedRunIdentity::Deletion {
            revision: revision_identity(package, digest)?,
            mode: crate::domain::DeletionMode::from_rank(mode)
                .map_err(|_| PersistenceError::CorruptRun("invalid deletion mode".to_owned()))?,
        });
    }
    if kind == ManagedExecutionKind::Migration {
        return super::super::sqlite_migration_runs::load_invocation(database, run)
            .map(ManagedRunIdentity::Migration);
    }
    if kind != ManagedExecutionKind::Action {
        let sql = match kind {
            ManagedExecutionKind::SnapshotCapture => {
                "SELECT package_id, revision_content_digest, NULL FROM run_capture_invocations WHERE run_id=?1"
            }
            ManagedExecutionKind::SnapshotRestore => {
                "SELECT package_id, revision_content_digest, snapshot_id FROM run_restore_invocations WHERE run_id=?1"
            }
            ManagedExecutionKind::Action
            | ManagedExecutionKind::Migration
            | ManagedExecutionKind::Deletion => unreachable!(),
        };
        let (package, digest, snapshot) = database
            .query_row(sql, [run.as_bytes().as_slice()], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                ))
            })
            .map_err(|error| PersistenceError::sqlite("load Snapshot Run invocation", error))?;
        let revision = revision_identity(package, digest)?;
        return match kind {
            ManagedExecutionKind::SnapshotCapture => Ok(ManagedRunIdentity::Capture { revision }),
            ManagedExecutionKind::SnapshotRestore => Ok(ManagedRunIdentity::Restore {
                revision,
                snapshot: crate::domain::SnapshotId::from_bytes(
                    snapshot
                        .ok_or_else(|| {
                            PersistenceError::CorruptRun(
                                "Restore invocation has no SnapshotId".to_owned(),
                            )
                        })?
                        .try_into()
                        .map_err(|_| {
                            PersistenceError::CorruptRun(
                                "invalid Restore SnapshotId length".to_owned(),
                            )
                        })?,
                ),
            }),
            ManagedExecutionKind::Action
            | ManagedExecutionKind::Migration
            | ManagedExecutionKind::Deletion => unreachable!(),
        };
    }
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
    Ok(ManagedRunIdentity::Action(ActionRunIdentity {
        revision: revision_identity(row.0, row.1)?,
        action: ActionIdentity::parse(action)
            .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
    }))
}

pub(super) fn execution_row(
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

pub(super) fn has_revision_pin(
    database: &Connection,
    run: RunId,
) -> Result<bool, PersistenceError> {
    database
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM run_revision_pins WHERE run_id=?1)",
            [run.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| PersistenceError::sqlite("load Run Revision pin", error))
}

pub(super) fn pinned_payloads(
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

pub(super) fn outcome_row(
    database: &Connection,
    run: RunId,
) -> Result<Option<OutcomeRow>, PersistenceError> {
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

pub(super) fn guard_row(
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

pub(in crate::persistence) fn inspect_run_from(
    db: &Connection,
    view: crate::domain::ManagedRunView,
) -> Result<crate::domain::ManagedRunInspectionData, PersistenceError> {
    let run = view.id;
    let guard = load_instance_recovery_guard_from(db, view.instance)?;
    let result: Option<Vec<u8>> = db
        .query_row(
            "SELECT snapshot_id FROM run_capture_results WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| PersistenceError::sqlite("read Capture result reference", e))?;
    let capture_result = result
        .map(|id| {
            id.try_into()
                .map(crate::domain::SnapshotId::from_bytes)
                .map_err(|_| PersistenceError::CorruptRun("invalid Capture result identity".into()))
        })
        .transpose()?;
    let published_artifacts = match &view.state {
        RunState::Finished(outcome) => outcome.artifacts.clone(),
        RunState::Running(_) => super::scoped_artifacts::load_scoped_artifacts(db, run)?,
    };
    Ok(crate::domain::ManagedRunInspectionData {
        published_artifacts,
        run: view,
        current_recovery_guard: guard,
        capture_result,
        diagnostics: super::super::sqlite_diagnostics::load_diagnostics_from(db, run)?,
        migration_progress: super::super::sqlite_migration_runs::progress_view(db, run)?,
    })
}

pub(in crate::persistence) fn load_instance_recovery_guard_from(
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

fn error_ref(owner: Vec<u8>, code: Vec<u8>) -> Result<PactrunErrorRef, PersistenceError> {
    let owner = String::from_utf8(owner)
        .map_err(|_| PersistenceError::CorruptRun("error owner is not UTF-8".to_owned()))?;
    let code = String::from_utf8(code)
        .map_err(|_| PersistenceError::CorruptRun("error code is not UTF-8".to_owned()))?;
    PactrunErrorRef::new(owner, code).map_err(PersistenceError::CorruptRun)
}

pub(super) fn utf8_message(bytes: Vec<u8>) -> Result<String, PersistenceError> {
    String::from_utf8(bytes)
        .map_err(|_| PersistenceError::CorruptRun("message is not UTF-8".to_owned()))
}

pub(super) fn load_run_from(
    database: &Connection,
    run: RunId,
) -> Result<RunView, PersistenceError> {
    let managed = load_managed_run_from(database, run)?;
    let ManagedRunIdentity::Action(action) = managed.operation else {
        return Err(PersistenceError::InvalidRunTransition(
            "Action view cannot represent a Snapshot Run".to_owned(),
        ));
    };
    Ok(RunView {
        id: managed.id,
        instance: managed.instance,
        accepted_state_version: managed.accepted_state_version,
        accepted_at_unix_ms: managed.accepted_at_unix_ms,
        action,
        state: managed.state,
    })
}

pub(in crate::persistence) fn load_managed_run_from(
    database: &Connection,
    run: RunId,
) -> Result<crate::domain::ManagedRunView, PersistenceError> {
    let header = run_header(database, run)?;
    let operation = managed_invocation_row(database, run)?;
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
            RunState::Finished(load_outcome_view(database, run, operation.kind(), outcome)?)
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
    validate_managed_run_links(database, run, &header, &operation, &state, admitted)?;
    super::super::sqlite_migration_runs::validate_links(
        database,
        run,
        header.instance,
        header.accepted_state_version,
        &operation,
        &state,
    )?;
    Ok(crate::domain::ManagedRunView {
        id: run,
        instance: header.instance,
        accepted_state_version: header.accepted_state_version,
        accepted_at_unix_ms: header.accepted_at_unix_ms,
        operation,
        state,
    })
}

fn validate_managed_run_links(
    database: &Connection,
    run: RunId,
    header: &RunHeader,
    operation: &ManagedRunIdentity,
    state: &RunState,
    admitted: bool,
) -> Result<(), PersistenceError> {
    let invalid = || {
        PersistenceError::CorruptRun(
            "Run operation, state and execution references disagree".to_owned(),
        )
    };
    let (payload_count, result_count, restore_count): (i64, i64, i64) = database
        .query_row(
            "SELECT (SELECT count(*) FROM run_payload_pins WHERE run_id=?1), \
         (SELECT count(*) FROM run_capture_results WHERE run_id=?1), \
         (SELECT count(*) FROM run_restore_admissions WHERE run_id=?1)",
            [run.as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| PersistenceError::sqlite("validate managed Run references", error))?;
    if admitted {
        let (package, digest): (Vec<u8>, Vec<u8>) = database
            .query_row(
                "SELECT package_id,revision_content_digest FROM run_revision_pins WHERE run_id=?1",
                [run.as_bytes().as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| PersistenceError::sqlite("load managed Revision pin", error))?;
        if operation.revision() != &revision_identity(package, digest)? {
            return Err(invalid());
        }
        if !matches!(operation, ManagedRunIdentity::Action(_)) {
            let core = load_revision_core(database, operation.revision())?;
            operation
                .authoritative_access(&core)
                .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
            if matches!(operation, ManagedRunIdentity::Capture { .. }) {
                for declaration in core.inputs() {
                    let protection: Option<i64> = database.query_row(
                    "SELECT p.protection_rank FROM run_payload_pins pins \
                     JOIN managed_input_payloads p ON p.instance_id=pins.instance_id AND p.payload_id=pins.payload_id \
                     WHERE pins.run_id=?1 AND pins.input_identity=?2",
                    params![run.as_bytes().as_slice(), declaration.id.as_str().as_bytes()], |row| row.get(0),
                ).optional().map_err(|error| PersistenceError::sqlite("validate Capture active pin", error))?;
                    match protection {
                        None if declaration.required => return Err(invalid()),
                        Some(rank) => {
                            let stored = crate::domain::ManagedInputProtection::from_rank(rank)
                                .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
                            if declaration.protection == crate::domain::InputProtectionV1::Secret
                                && stored != crate::domain::ManagedInputProtection::Secret
                            {
                                return Err(invalid());
                            }
                        }
                        None => {}
                    }
                }
            }
        }
    } else if payload_count != 0 {
        return Err(invalid());
    }
    let capture_success = matches!(operation, ManagedRunIdentity::Capture { .. })
        && matches!(state, RunState::Finished(outcome) if outcome.outcome == RunOutcome::Succeeded);
    if !matches!(operation, ManagedRunIdentity::Action(_))
        && matches!(state, RunState::Finished(outcome)
            if outcome.outcome == RunOutcome::Succeeded && outcome.boundary != ActionRunBoundary::Admitted)
    {
        return Err(invalid());
    }
    if result_count != i64::from(capture_success) {
        return Err(invalid());
    }
    if capture_success {
        let result: Vec<u8> = database
            .query_row(
                "SELECT snapshot_id FROM run_capture_results WHERE run_id=?1",
                [run.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(|error| PersistenceError::sqlite("load Capture result identity", error))?;
        let _: [u8; 16] = result.try_into().map_err(|_| invalid())?;
    }
    let restore_admitted = matches!(operation, ManagedRunIdentity::Restore { .. }) && admitted;
    if restore_count != i64::from(restore_admitted) {
        return Err(invalid());
    }
    if let ManagedRunIdentity::Restore { revision, snapshot } = operation {
        if payload_count != 0 {
            return Err(invalid());
        }
        if admitted {
            let (id, token, consequence): (Vec<u8>,Vec<u8>,i64) = database.query_row(
                "SELECT snapshot_id,admitted_state_version,admitted_consequence_version FROM run_restore_admissions WHERE run_id=?1",
                [run.as_bytes().as_slice()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
            ).map_err(|error| PersistenceError::sqlite("load Restore admission", error))?;
            if id != snapshot.as_bytes()
                || state_version(token)? != header.accepted_state_version
                || consequence < 0
                || consequence
                    > super::super::writer_admission::load_consequence_version(
                        database,
                        header.instance,
                    )?
                || &super::super::sqlite_snapshots::snapshot_producer(database, *snapshot).map_err(
                    |error| match error {
                        PersistenceError::MissingSnapshot(_) => invalid(),
                        other => other,
                    },
                )? != revision
            {
                return Err(invalid());
            }
        }
    }
    if !matches!(operation, ManagedRunIdentity::Action(_))
        && matches!(state, RunState::Finished(outcome) if !outcome.artifacts.is_empty())
    {
        let legacy: bool = database
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM run_artifacts WHERE run_id=?1)",
                [run.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| {
                PersistenceError::sqlite("validate operation artifact representation", e)
            })?;
        if legacy {
            return Err(invalid());
        }
    }
    Ok(())
}

fn validate_pin_instances(
    database: &Connection,
    run: RunId,
    instance: InstanceId,
) -> Result<(), PersistenceError> {
    let foreign: bool = database
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM run_payload_pins pins \
             LEFT JOIN managed_input_payloads p ON p.instance_id=pins.instance_id AND p.payload_id=pins.payload_id \
             WHERE pins.run_id=?1 AND (pins.instance_id<>?2 OR p.payload_id IS NULL))",
            params![run.as_bytes().as_slice(), instance.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| PersistenceError::sqlite("validate Run payload pins", error))?;
    if foreign {
        return Err(PersistenceError::CorruptRun(
            "payload pin references another Instance or a missing payload".to_owned(),
        ));
    }
    Ok(())
}

fn load_missing_input_cause(
    database: &Connection,
    run: RunId,
    owner: &[u8],
    code: &[u8],
    step: i64,
) -> Result<Option<crate::domain::RunFailureCause>, PersistenceError> {
    let mut statement = database.prepare("SELECT ordinal,input_id FROM run_missing_input_causes WHERE run_id=?1 ORDER BY ordinal")
        .map_err(|e| PersistenceError::sqlite("load missing-Input cause", e))?;
    let rows = statement
        .query_map([run.as_bytes().as_slice()], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))
        })
        .map_err(|e| PersistenceError::sqlite("query missing-Input cause", e))?;
    let mut ids = Vec::new();
    for row in rows {
        let (ordinal, bytes) =
            row.map_err(|e| PersistenceError::sqlite("read missing-Input cause", e))?;
        if ordinal != ids.len() as i64
            || owner != b"admission"
            || code != b"plan_invalidated"
            || step != RunFailedStep::Admission.rank()
        {
            return Err(PersistenceError::CorruptRun(
                "invalid missing-Input cause attribution".into(),
            ));
        }
        ids.push(
            InputIdentity::parse(utf8_message(bytes)?)
                .map_err(|e| PersistenceError::CorruptRun(e.to_string()))?,
        );
    }
    Ok((!ids.is_empty()).then_some(crate::domain::RunFailureCause::MissingRequiredInputs(ids)))
}

fn load_outcome_view(
    database: &Connection,
    run: RunId,
    kind: crate::domain::ManagedExecutionKind,
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
                cause: load_missing_input_cause(database, run, &owner, &code, step)?,
                failure: RunFailureRecord {
                    error: error_ref(owner, code)?,
                    message: utf8_message(message)?,
                },
                step: RunFailedStep::for_operation(step, kind)
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
            edge: None,
            output: ManagedOutputIdentity::parse(utf8_message(output)?)
                .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?,
            byte_length: u64::try_from(byte_length)
                .map_err(|_| PersistenceError::CorruptRun("negative Artifact length".to_owned()))?,
        });
    }
    ensure_ordered(&artifacts, |left, right| left.output.cmp(&right.output))?;
    artifacts.extend(super::scoped_artifacts::load_scoped_artifacts(
        database, run,
    )?);
    let mut artifact_keys = BTreeSet::new();
    if artifacts
        .iter()
        .any(|artifact| !artifact_keys.insert((artifact.edge.clone(), artifact.output.clone())))
    {
        return Err(PersistenceError::CorruptRun(
            "duplicate logical artifact identity".to_owned(),
        ));
    }

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
