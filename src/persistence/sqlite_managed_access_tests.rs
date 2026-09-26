use super::*;
use crate::domain::{
    CaptureV1, RestoreV1, SnapshotCapabilityV1, SnapshotId, SnapshotIntegrityVersion,
    SnapshotManifest, SnapshotManifestParts, SnapshotTimestamp,
};

mod lifecycle {
    include!("sqlite_managed_lifecycle_tests.rs");
}

#[derive(Clone, Copy, Debug)]
enum Operation {
    ObserveAction,
    MutateAction,
    ObserveCapture,
    MutateCapture,
    Restore,
}

impl Operation {
    fn access(self) -> OperationAccessV1 {
        match self {
            Self::ObserveAction | Self::ObserveCapture => OperationAccessV1::Observe,
            _ => OperationAccessV1::Mutate,
        }
    }
    fn kind(self) -> i64 {
        match self {
            Self::ObserveAction | Self::MutateAction => 0,
            Self::ObserveCapture | Self::MutateCapture => 1,
            Self::Restore => 2,
        }
    }
}

const OPERATIONS: [Operation; 5] = [
    Operation::ObserveAction,
    Operation::MutateAction,
    Operation::ObserveCapture,
    Operation::MutateCapture,
    Operation::Restore,
];

fn managed_revision(p: &PactrunPersistence, op: Operation) -> RevisionIdentity {
    managed_revision_with_inputs(p, op, Vec::new())
}

fn managed_revision_with_inputs(
    p: &PactrunPersistence,
    op: Operation,
    inputs: Vec<InputDeclarationV1>,
) -> RevisionIdentity {
    let digest = Sha256Digest::from_bytes(Sha256::digest(TOOL_BYTES).into());
    let publication = p
        .put_runtime_content(&digest, &mut Cursor::new(TOOL_BYTES))
        .unwrap();
    let hook = HookV1 {
        protocol_version: crate::domain::FormatVersion::BASELINE,
        launch: HookLaunchV1::Direct {
            executable: ContentId::parse("tool").unwrap(),
        },
        args: Vec::new(),
        io: IOContractV1 {
            terminal: TerminalContractV1::None,
        },
    };
    let core = project_revision_declarations(RevisionDeclarationInput {
        inputs,
        actions: vec![ActionV1 {
            id: ActionIdentity::parse("deploy").unwrap(),
            access: op.access(),
            parameters: Vec::new(),
            hook: hook.clone(),
            outputs: Vec::new(),
        }],
        snapshot: Some(SnapshotCapabilityV1 {
            capture: Some(CaptureV1 {
                parameters: Vec::new(),
                access: op.access(),
                hook: hook.clone(),
            }),
            restore: Some(RestoreV1 {
                parameters: Vec::new(),
                hook,
            }),
        }),
        migrations: Vec::new(),
        cleanup: None,
    })
    .unwrap();
    let runtime = project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 {
        files: vec![RuntimeFileV1 {
            id: ContentId::parse("tool").unwrap(),
            path: RuntimePath::parse("bin/tool").unwrap(),
            kind: RuntimeFileKindV1::RegularFile,
            blob_digest: digest,
            executable: true,
        }],
    })
    .unwrap();
    p.persist_revision(
        crate::domain::PackageId::from_bytes([31; 16]),
        &validate_declaration_content(core, runtime).unwrap(),
        &[publication],
    )
    .unwrap()
}

fn empty_instance(p: &PactrunPersistence, revision: &RevisionIdentity, name: &str) -> InstanceView {
    p.create_instance(
        InstanceName::parse(name).unwrap(),
        revision.clone(),
        &mut [],
    )
    .unwrap()
}

/// Build a durable shape fixture, not a production Snapshot execution. S4's
/// remaining acceptance/Admission plumbing and S5/S6 Hooks are not exercised
/// by this helper. Restore fixtures have a real empty immutable Snapshot pin.
fn fixture_run(
    p: &PactrunPersistence,
    view: &InstanceView,
    revision: &RevisionIdentity,
    op: Operation,
    admitted: bool,
) -> RunId {
    let id = p
        .create_accepted_run(
            RunId::generate().unwrap(),
            view.id,
            view.state_version,
            &action(revision),
            &owner(),
            &crate::persistence::UnconditionalAcceptance,
        )
        .unwrap();
    let mut db = p.database.lock().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let snapshot = SnapshotId::generate().unwrap();
    if op.kind() != 0 {
        tx.execute(
            "DELETE FROM run_action_invocations WHERE run_id=?1",
            [id.as_bytes().as_slice()],
        )
        .unwrap();
        tx.execute(
            "UPDATE run_operation_kinds SET operation_kind=?2 WHERE run_id=?1",
            params![id.as_bytes().as_slice(), op.kind()],
        )
        .unwrap();
        if op.kind() == 1 {
            tx.execute(
                "INSERT INTO run_capture_invocations VALUES (?1,?2,?3)",
                params![
                    id.as_bytes().as_slice(),
                    revision.package_id.as_bytes().as_slice(),
                    revision.content_digest.as_bytes().as_slice()
                ],
            )
            .unwrap();
        } else {
            let manifest = SnapshotManifest::new(SnapshotManifestParts {
                version: SnapshotIntegrityVersion::BASELINE,
                snapshot_id: snapshot,
                producer: revision.clone(),
                origin_instance_id: view.id,
                captured_at: SnapshotTimestamp::new(0, 0).unwrap(),
                managed_bindings: Vec::new(),
                service_content: Vec::new(),
            })
            .unwrap();
            super::super::super::sqlite_snapshots::insert_snapshot_manifest(
                &tx,
                &crate::snapshot_integrity::encode_snapshot_manifest(manifest, None).unwrap(),
            )
            .unwrap();
            tx.execute(
                "INSERT INTO run_restore_invocations VALUES (?1,?2,?3,?4)",
                params![
                    id.as_bytes().as_slice(),
                    revision.package_id.as_bytes().as_slice(),
                    revision.content_digest.as_bytes().as_slice(),
                    snapshot.as_bytes().as_slice()
                ],
            )
            .unwrap();
        }
    }
    if admitted {
        tx.execute(
            "INSERT INTO run_revision_pins VALUES (?1,?2,?3)",
            params![
                id.as_bytes().as_slice(),
                revision.package_id.as_bytes().as_slice(),
                revision.content_digest.as_bytes().as_slice()
            ],
        )
        .unwrap();
        if op.kind() == 2 {
            tx.execute(
                "INSERT INTO run_restore_admissions VALUES (?1,?2,?3,0)",
                params![
                    id.as_bytes().as_slice(),
                    snapshot.as_bytes().as_slice(),
                    view.state_version.as_bytes().as_slice()
                ],
            )
            .unwrap();
        }
    }
    tx.commit().unwrap();
    id
}

fn conflict(
    p: &PactrunPersistence,
    view: &InstanceView,
    run: RunId,
) -> Result<Option<RunId>, PersistenceError> {
    let mut db = p.database.lock().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    managed_mutation_conflict(&tx, view.id, run)
}

// Test-ID: PR-TEST-0220
// Verifies: PR-REQ-0289
#[test]
fn all_operation_pairs_resolve_access_from_exact_persisted_revisions() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    for (i, incoming) in OPERATIONS.into_iter().enumerate() {
        for (j, existing) in OPERATIONS.into_iter().enumerate() {
            let incoming_revision = managed_revision(&p, incoming);
            let competitor_revision = managed_revision(&p, existing);
            let view = empty_instance(&p, &incoming_revision, &format!("pair-{i}-{j}"));
            let competitor = fixture_run(&p, &view, &competitor_revision, existing, false);
            let run = fixture_run(&p, &view, &incoming_revision, incoming, false);
            // Accepted-only never occupies the Mutate exclusion.
            assert_eq!(conflict(&p, &view, run).unwrap(), None);
            let admitted = fixture_run(&p, &view, &competitor_revision, existing, true);
            assert_eq!(
                conflict(&p, &view, run).unwrap(),
                (incoming.access() == OperationAccessV1::Mutate
                    && existing.access() == OperationAccessV1::Mutate)
                    .then_some(admitted),
                "incoming {incoming:?}; competitor {existing:?}; accepted {competitor:?}"
            );
            // The competitor may pin a different historical Revision than the
            // Instance's current one; its authored access still comes from that pin.
            assert_eq!(
                p.load_instance_by_id(view.id)
                    .unwrap()
                    .unwrap()
                    .active_revision,
                incoming_revision
            );
        }
    }
}

// Test-ID: PR-TEST-0221
// Verifies: PR-REQ-0289
#[test]
fn action_admission_includes_snapshot_competitors_and_publishes_refusal_atomically() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    for (i, incoming) in [Operation::ObserveAction, Operation::MutateAction]
        .into_iter()
        .enumerate()
    {
        for (j, existing) in [
            Operation::ObserveCapture,
            Operation::MutateCapture,
            Operation::Restore,
        ]
        .into_iter()
        .enumerate()
        {
            let revision = managed_revision(&p, incoming);
            let other_revision = managed_revision(&p, existing);
            let view = empty_instance(&p, &revision, &format!("admit-{i}-{j}"));
            let other = fixture_run(&p, &view, &other_revision, existing, true);
            let run = accepted(&p, &view);
            let result = admit(&p, run, view.state_version, &[], false).unwrap();
            if incoming.access() == OperationAccessV1::Mutate
                && existing.access() == OperationAccessV1::Mutate
            {
                assert_eq!(result, Err(AdmissionRefusal::MutationConflict(other)));
                let outcome = finished(&p, run);
                assert_eq!(outcome.outcome, RunOutcome::Failed);
                assert_eq!(outcome.boundary, ActionRunBoundary::Accepted);
                assert!(!has_revision_pin(&p.database.lock().unwrap(), run).unwrap());
            } else {
                assert_eq!(result, Ok(()));
                assert!(has_revision_pin(&p.database.lock().unwrap(), run).unwrap());
            }
            assert_eq!(token(&p, view.id), view.state_version);
        }
    }
}

// Test-ID: PR-TEST-0222
// Verifies: PR-REQ-0289, PR-REQ-0298
#[test]
fn malformed_competitor_is_not_hidden_by_action_only_joins() {
    for malformed in [
        "missing-invocation",
        "extra-invocation",
        "wrong-pin",
        "undeclared-capture",
    ] {
        let (_tmp, path) = root();
        let p = PactrunPersistence::open(&path).unwrap();
        let current = managed_revision(&p, Operation::MutateAction);
        let pinned = if malformed == "undeclared-capture" {
            revision(&p)
        } else {
            managed_revision(&p, Operation::ObserveCapture)
        };
        let view = empty_instance(&p, &current, "corrupt-competitor");
        let other = fixture_run(&p, &view, &pinned, Operation::ObserveCapture, true);
        let run = accepted(&p, &view);
        {
            let db = p.database.lock().unwrap();
            match malformed {
                "missing-invocation" => {
                    db.execute(
                        "DELETE FROM run_capture_invocations WHERE run_id=?1",
                        [other.as_bytes().as_slice()],
                    )
                    .unwrap();
                }
                "extra-invocation" => {
                    db.execute(
                        "INSERT INTO run_action_invocations VALUES (?1,?2,?3,?4)",
                        params![
                            other.as_bytes().as_slice(),
                            pinned.package_id.as_bytes().as_slice(),
                            pinned.content_digest.as_bytes().as_slice(),
                            b"deploy".as_slice()
                        ],
                    )
                    .unwrap();
                }
                "wrong-pin" => {
                    db.execute("UPDATE run_revision_pins SET package_id=?2,revision_content_digest=?3 WHERE run_id=?1", params![other.as_bytes().as_slice(), current.package_id.as_bytes().as_slice(), current.content_digest.as_bytes().as_slice()]).unwrap();
                }
                _ => {}
            }
        }
        assert!(
            matches!(
                admit(&p, run, view.state_version, &[], false),
                Err(PersistenceError::CorruptRun(_))
            ),
            "{malformed}"
        );
        assert!(matches!(
            p.load_run(run).unwrap().unwrap().state,
            RunState::Running(_)
        ));
        assert!(!has_revision_pin(&p.database.lock().unwrap(), run).unwrap());
        assert_eq!(token(&p, view.id), view.state_version);
    }
}

// Test-ID: PR-TEST-0223
// Verifies: PR-REQ-0289, PR-REQ-0298
#[test]
fn snapshot_failure_uses_owned_terminal_boundary_without_publishing_success_or_replaying() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    for (i, op) in [Operation::MutateCapture, Operation::Restore]
        .into_iter()
        .enumerate()
    {
        let revision = managed_revision(&p, op);
        let view = empty_instance(&p, &revision, &format!("terminal-{i}"));
        let run = fixture_run(&p, &view, &revision, op, true);
        assert!(matches!(
            p.finish_run_owned(
                &owner(),
                run,
                &plain_finish(RunOutcome::Succeeded),
                &[],
                &mut []
            ),
            Err(PersistenceError::InvalidRunTransition(_))
        ));
        assert!(
            execution_row(&p.database.lock().unwrap(), run)
                .unwrap()
                .is_some()
        );
        assert!(
            outcome_row(&p.database.lock().unwrap(), run)
                .unwrap()
                .is_none()
        );
        let wrong_owner =
            ExecutionOwnerSession::parse(format!("session-{}", "1".repeat(32))).unwrap();
        p.open_recovery_risk(run).unwrap();
        assert!(matches!(
            p.finish_run_owned(
                &wrong_owner,
                run,
                &plain_finish(RunOutcome::Interrupted),
                &[],
                &mut []
            ),
            Err(PersistenceError::InvalidRunTransition(_))
        ));
        assert_eq!(p.recovery_consequence_version(view.id).unwrap(), 0);
        p.finish_run_owned(
            &owner(),
            run,
            &plain_finish(RunOutcome::Interrupted),
            &[],
            &mut [],
        )
        .unwrap();
        let guard = p.load_instance_recovery_guard(view.id).unwrap().unwrap();
        assert_eq!(guard.trigger, ManualRecoveryTrigger::OpenRiskOwnerLoss);
        assert_eq!(p.recovery_consequence_version(view.id).unwrap(), 1);
        assert!(matches!(
            p.finish_run_owned(
                &owner(),
                run,
                &plain_finish(RunOutcome::Interrupted),
                &[],
                &mut []
            ),
            Err(PersistenceError::RunNotRunning)
        ));
        assert_eq!(p.recovery_consequence_version(view.id).unwrap(), 1);
        let db = p.database.lock().unwrap();
        assert!(!has_revision_pin(&db, run).unwrap());
        assert!(execution_row(&db, run).unwrap().is_none());
        assert_eq!(
            outcome_row(&db, run).unwrap().unwrap().outcome,
            RunOutcome::Interrupted
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM run_restore_admissions WHERE run_id=?1",
                [run.as_bytes().as_slice()],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM run_capture_results WHERE run_id=?1",
                [run.as_bytes().as_slice()],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}
