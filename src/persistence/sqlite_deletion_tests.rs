use super::*;
use crate::persistence::UnconditionalAcceptance;

fn fixture() -> (tempfile::TempDir, PactrunPersistence, InstanceView) {
    fixture_with_cleanup(false)
}
fn fixture_with_cleanup(cleanup: bool) -> (tempfile::TempDir, PactrunPersistence, InstanceView) {
    let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m7-deletion-tests");
    std::fs::create_dir_all(&parent).unwrap();
    let temp = tempfile::tempdir_in(parent).unwrap();
    for path in ["store/database", "store/runtime-content", "store/staging"] {
        std::fs::create_dir_all(temp.path().join(path)).unwrap();
    }
    let p = PactrunPersistence::open(temp.path().join("store")).unwrap();
    use sha2::{Digest, Sha256};
    let bytes = b"unused executable fixture; admission tests never launch it";
    let file = RuntimeFileV1 {
        id: ContentId::parse("cleanup").unwrap(),
        path: RuntimePath::parse("cleanup").unwrap(),
        kind: RuntimeFileKindV1::RegularFile,
        blob_digest: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
        executable: true,
    };
    let publications = if cleanup {
        vec![
            p.put_runtime_content(&file.blob_digest, &mut &bytes[..])
                .unwrap(),
        ]
    } else {
        vec![]
    };
    let core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
        inputs: vec![InputDeclarationV1 {
            id: InputIdentity::parse("required").unwrap(),
            required: true,
            protection: InputProtectionV1::Normal,
        }],
        actions: vec![],
        snapshot: None,
        migrations: vec![],
        cleanup: cleanup.then(|| CleanupV1 {
            requires: vec![],
            hook: HookV1 {
                protocol_version: PositiveVersion::new(1).unwrap(),
                launch: HookLaunchV1::Direct {
                    executable: file.id.clone(),
                },
                args: vec![],
                io: IOContractV1 {
                    terminal: TerminalContractV1::None,
                },
            },
        }),
    })
    .unwrap();
    let runtime = project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 {
        files: if cleanup { vec![file] } else { vec![] },
    })
    .unwrap();
    let content = validate_revision_content_v1(core, runtime).unwrap();
    let revision = p
        .persist_revision(PackageId::from_bytes([1; 16]), &content, &publications)
        .unwrap();
    let instance = p
        .create_instance(InstanceName::parse("retire-me").unwrap(), revision, &mut [])
        .unwrap();
    (temp, p, instance)
}
fn compile(p: &PactrunPersistence, instance: &InstanceView, mode: DeletionMode) -> DeletionPlan {
    let intent = DeleteInstance {
        instance: instance.id,
        expected: instance.state_version,
        mode,
    };
    crate::workflow::compile_deletion(
        &intent,
        p.observe_deletion(&intent).unwrap(),
        &crate::workflow::PlatformHostLauncherLookup,
        &[],
    )
    .unwrap()
}
fn accept(p: &PactrunPersistence, plan: &DeletionPlan) -> RunId {
    let run = RunId::generate().unwrap();
    p.create_accepted_managed_run(
        run,
        plan.instance,
        plan.expected,
        &ManagedRunIdentity::Deletion {
            revision: plan.revision.clone(),
            mode: plan.mode,
        },
        &p.staging_session().unwrap().owner(),
        &UnconditionalAcceptance,
    )
    .unwrap();
    run
}

fn service_fixture() -> (
    tempfile::TempDir,
    PactrunPersistence,
    InstanceView,
    Vec<(ServiceAllocationId, std::path::PathBuf)>,
) {
    let (temp, p, _) = fixture();
    let core = crate::revision_core_v2::project_revision_core_source_v2(
        br#"{
        "format_version":2,"inputs":[],"actions":[],"migrations":[],
        "service_storages":[{"id":"one"},{"id":"two"}],"service_resources":[]
    }"#,
    )
    .unwrap();
    let runtime =
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![] })
            .unwrap();
    let content = crate::revision_core_v2::validate_revision_content_v2(core, runtime).unwrap();
    let revision = p
        .persist_revision_internal(PackageId::from_bytes([2; 16]), &content.into(), &[], None)
        .unwrap();
    let instance = p
        .create_instance(InstanceName::parse("service").unwrap(), revision, &mut [])
        .unwrap();
    let roots = p
        .load_instance_service_state(instance.id)
        .unwrap()
        .unwrap()
        .storages
        .into_iter()
        .map(|storage| {
            let path = temp
                .path()
                .join("store/service-storage")
                .join(format!("alloc-{}", storage.allocation));
            std::fs::create_dir(path.join("nested")).unwrap();
            std::fs::write(path.join("nested/data"), b"live payload").unwrap();
            (storage.allocation, path)
        })
        .collect();
    (temp, p, instance, roots)
}
fn success() -> RunFinish {
    RunFinish {
        outcome: RunOutcome::Succeeded,
        primary_failure: None,
        secondary_failures: vec![],
        hook_completion: None,
    }
}

fn abandon(p: &PactrunPersistence, instance: &InstanceView) {
    let plan = compile(p, instance, DeletionMode::AbandonManagement);
    let run = accept(p, &plan);
    let owner = p.staging_session().unwrap().owner();
    p.admit_deletion(run, &owner, &plan, false, &|_| Ok(()))
        .unwrap()
        .unwrap();
    p.finish_instance_retirement(run, &owner, &success())
        .unwrap();
}

// Test-ID: PR-TEST-0446
// Verifies: PR-REQ-0036, PR-REQ-0336
#[test]
fn concurrent_delete_and_abandon_admission_share_the_mutation_gate() {
    let (temp, p, instance) = fixture();
    let other = PactrunPersistence::open(temp.path().join("store")).unwrap();
    let delete = compile(&p, &instance, DeletionMode::ManagedCleanup);
    let abandon = compile(&other, &instance, DeletionMode::AbandonManagement);
    let first = accept(&p, &delete);
    let second = accept(&other, &abandon);
    let first_owner = p.staging_session().unwrap().owner();
    let second_owner = other.staging_session().unwrap().owner();
    let start = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            start.wait();
            p.admit_deletion(first, &first_owner, &delete, false, &|_| Ok(()))
                .unwrap()
        });
        let b = scope.spawn(|| {
            start.wait();
            other
                .admit_deletion(second, &second_owner, &abandon, false, &|_| Ok(()))
                .unwrap()
        });
        [a.join().unwrap(), b.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(AdmissionRefusal::MutationConflict(_))))
            .count(),
        1
    );
    let runs = p.list_managed_runs(instance.id).unwrap();
    assert_eq!(
        runs.iter()
            .filter(|r| matches!(r.state, RunState::Running(_)))
            .count(),
        1
    );
    assert_eq!(
        runs.iter()
            .filter(|r| matches!(&r.state,RunState::Finished(o) if o.outcome==RunOutcome::Failed))
            .count(),
        1
    );
    assert_eq!(
        p.load_instance_by_id(instance.id).unwrap().unwrap(),
        instance
    );
}

// Test-ID: PR-TEST-0438
// Verifies: PR-REQ-0336, PR-REQ-0337, PR-REQ-0338
#[test]
fn legacy_incarnation_evidence_is_upgraded_before_new_backend_progress() {
    let (temp, p, instance, mut roots) = service_fixture();
    roots.sort_by_key(|(id, _)| *id.as_bytes());
    let id = roots[0].0;
    let physical = crate::retirement_fs::open_qualified(&temp.path().join("store"), id, None)
        .unwrap()
        .unwrap();
    let current = *physical.identity();
    drop(physical);
    let mut legacy = current;
    legacy[..8].copy_from_slice(&(if cfg!(windows) { 1u64 } else { 2u64 }).to_le_bytes());
    let plan = compile(&p, &instance, DeletionMode::ManagedCleanup);
    let run = accept(&p, &plan);
    let owner = p.staging_session().unwrap().owner();
    p.admit_deletion(run, &owner, &plan, false, &|_| Ok(()))
        .unwrap()
        .unwrap();
    p.prepare_deletion_finalization(run, &owner).unwrap();
    p.database
        .lock()
        .unwrap()
        .execute(
            "UPDATE deletion_finalization_allocations SET root_identity=?1 WHERE allocation_id=?2",
            params![legacy.as_slice(), id.as_bytes().as_slice()],
        )
        .unwrap();
    assert!(!p.finalize_next_allocation(run, &owner).unwrap());
    let saved: Vec<u8> = p
        .database
        .lock()
        .unwrap()
        .query_row(
            "SELECT root_identity FROM deletion_finalization_allocations WHERE allocation_id=?1",
            [id.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(saved.as_slice(), current.as_slice());
    let tag = u64::from_le_bytes(saved[..8].try_into().unwrap());
    assert!(matches!(tag, 3 | 4));
    assert!(
        !matches!(tag, 1 | 2),
        "the legacy decoder must refuse this capability rather than treat a relocated root as absent"
    );
    assert_eq!(
        &saved[8..],
        &legacy[8..],
        "the native incarnation was not changed"
    );
    while !p.finalize_next_allocation(run, &owner).unwrap() {}
    assert!(
        p.finish_instance_retirement(run, &owner, &success())
            .unwrap()
    );
}

// Test-ID: PR-TEST-0432
// Verifies: PR-REQ-0336, PR-REQ-0337
#[cfg(target_os = "linux")]
#[test]
fn linux_partial_claims_keep_abandon_handoff_and_explicit_discard_retryable() {
    let (temp, p, instance, roots) = service_fixture();
    let outside = temp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("sentinel"), b"outside custody").unwrap();
    for (_, path) in &roots {
        std::os::unix::fs::symlink(&outside, path.join("unsafe")).unwrap();
        std::fs::write(path.join("z-survives"), b"surviving bytes").unwrap();
    }
    let plan = compile(&p, &instance, DeletionMode::ManagedCleanup);
    let run = accept(&p, &plan);
    let owner = p.staging_session().unwrap().owner();
    p.admit_deletion(run, &owner, &plan, false, &|_| Ok(()))
        .unwrap()
        .unwrap();
    p.prepare_deletion_finalization(run, &owner).unwrap();
    assert!(matches!(
        p.finalize_next_allocation(run, &owner),
        Err(PersistenceError::ServiceStorageUnavailable(_))
    ));
    assert_eq!(
        p.deletion_obligation(instance.id).unwrap().unwrap().phase,
        DeletionPhase::FinalizationAuthorized
    );
    assert!(
        p.finish_instance_retirement(run, &owner, &success())
            .is_err()
    );
    p.finish_run_owned(
        &owner,
        run,
        &RunFinish {
            outcome: RunOutcome::Cancelled,
            ..success()
        },
        &[],
        &mut [],
    )
    .unwrap();
    abandon(&p, &instance);
    assert!(p.load_instance_by_id(instance.id).unwrap().is_none());
    let id = p.list_detached_allocations().unwrap()[0].allocation;
    assert!(matches!(
        p.discard_detached_allocation(id, true),
        Err(PersistenceError::ServiceStorageUnavailable(_))
    ));
    assert_eq!(
        p.detached_allocation(id).unwrap().unwrap().state,
        DetachedAllocationState::DiscardPending
    );
    drop(p);
    let p = PactrunPersistence::open(temp.path().join("store")).unwrap();
    assert_eq!(
        p.detached_allocation(id).unwrap().unwrap().state,
        DetachedAllocationState::DiscardPending
    );
    assert!(p.discard_detached_allocation(id, false).is_err());
    for allocation in p.list_detached_allocations().unwrap() {
        let locations = p.detached_handoff(allocation.allocation).unwrap();
        assert!(locations.iter().any(|location| {
            std::fs::read(location.path.join("z-survives"))
                .is_ok_and(|bytes| bytes == b"surviving bytes")
        }));
        // Explicit operator repair removes only the test-created link, never
        // its target. No ordinary startup operation performs this action.
        for location in locations {
            if location.path.join("unsafe").symlink_metadata().is_ok() {
                std::fs::remove_file(location.path.join("unsafe")).unwrap();
            }
        }
    }
    assert!(p.discard_detached_allocation(id, true).unwrap());
    assert_eq!(
        p.detached_allocation(id).unwrap().unwrap().state,
        DetachedAllocationState::Discarded
    );
    assert_eq!(
        std::fs::read(outside.join("sentinel")).unwrap(),
        b"outside custody"
    );
}

// Test-ID: PR-TEST-0428
// Verifies: PR-REQ-0065, PR-REQ-0074, PR-REQ-0336
#[test]
fn malformed_work_set_preserves_unexposed_evidence_without_authorizing_traversal() {
    let (temp, p, instance, _) = service_fixture();
    let allocation = ServiceAllocationId::from_bytes([0; 16]);
    let owner = p.staging_session().unwrap().owner();
    {
        let db = p.database.lock().unwrap();
        db.execute(
            "INSERT INTO service_storage_allocations VALUES(?1,?2,?3,?4,?5)",
            params![
                allocation.as_bytes().as_slice(),
                instance.id.as_bytes().as_slice(),
                instance.active_revision.package_id.as_bytes().as_slice(),
                instance
                    .active_revision
                    .content_digest
                    .as_bytes()
                    .as_slice(),
                b"one".as_slice()
            ],
        )
        .unwrap();
        db.execute(
            "INSERT INTO service_storage_preparations VALUES(?1,?2,?3,?4)",
            params![
                allocation.as_bytes().as_slice(),
                owner.as_str().as_bytes(),
                instance.active_revision.package_id.as_bytes().as_slice(),
                instance
                    .active_revision
                    .content_digest
                    .as_bytes()
                    .as_slice()
            ],
        )
        .unwrap();
    }
    let plan = compile(&p, &instance, DeletionMode::ManagedCleanup);
    let run = accept(&p, &plan);
    p.admit_deletion(run, &owner, &plan, false, &|_| Ok(()))
        .unwrap()
        .unwrap();
    p.prepare_deletion_finalization(run, &owner).unwrap();
    assert_eq!(
        p.database
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM deletion_finalization_allocations WHERE allocation_id=?1",
                [allocation.as_bytes().as_slice()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    // A corrupt reference must block maintenance from destroying its evidence,
    // but it must never turn an unexposed creation intent into owned storage.
    p.database.lock().unwrap().execute("INSERT INTO deletion_finalization_allocations(attempt_run_id,allocation_id,finished) VALUES(?1,?2,0)",params![run.as_bytes().as_slice(),allocation.as_bytes().as_slice()]).unwrap();
    drop(p);
    let p = PactrunPersistence::open(temp.path().join("store")).unwrap();
    assert_eq!(p.reconcile_absent_service_preparations().unwrap(), 0);
    assert!(p.deletion_obligation(instance.id).is_err());
    // Repair only the synthetic bad row in this isolated test database.
    p.database
        .lock()
        .unwrap()
        .execute(
            "DELETE FROM deletion_finalization_allocations WHERE allocation_id=?1",
            [allocation.as_bytes().as_slice()],
        )
        .unwrap();
    assert_eq!(p.reconcile_absent_service_preparations().unwrap(), 1);
    assert!(p.reconcile_managed_run(run, &owner).unwrap());
    let current = p.load_instance_by_id(instance.id).unwrap().unwrap();
    let plan = compile(&p, &current, DeletionMode::ManagedCleanup);
    let retry = accept(&p, &plan);
    let owner = p.staging_session().unwrap().owner();
    p.admit_deletion(retry, &owner, &plan, false, &|_| Ok(()))
        .unwrap()
        .unwrap();
    while !p.finalize_next_allocation(retry, &owner).unwrap() {}
    assert!(
        p.finish_instance_retirement(retry, &owner, &success())
            .unwrap()
    );
    assert_eq!(p.reconcile_absent_service_preparations().unwrap(), 0);
    assert!(p.load_instance_by_id(instance.id).unwrap().is_none());
}

// Test-ID: PR-TEST-0445
// Verifies: PR-REQ-0247, PR-REQ-0336, PR-REQ-0337, PR-REQ-0338
#[test]
fn retirement_never_adopts_an_existing_unexposed_preparation_path() {
    for mode in [
        DeletionMode::ManagedCleanup,
        DeletionMode::AbandonManagement,
    ] {
        let (temp, p, instance, _) = service_fixture();
        let unknown = ServiceAllocationId::from_bytes([0; 16]);
        let owner = p.staging_session().unwrap().owner();
        let path = temp
            .path()
            .join("store/service-storage")
            .join(format!("alloc-{unknown}"));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("foreign"), b"not admitted or exposed").unwrap();
        {
            let db = p.database.lock().unwrap();
            db.execute(
                "INSERT INTO service_storage_allocations VALUES(?1,?2,?3,?4,?5)",
                params![
                    unknown.as_bytes().as_slice(),
                    instance.id.as_bytes().as_slice(),
                    instance.active_revision.package_id.as_bytes().as_slice(),
                    instance
                        .active_revision
                        .content_digest
                        .as_bytes()
                        .as_slice(),
                    b"one".as_slice()
                ],
            )
            .unwrap();
            db.execute(
                "INSERT INTO service_storage_preparations VALUES(?1,?2,?3,?4)",
                params![
                    unknown.as_bytes().as_slice(),
                    owner.as_str().as_bytes(),
                    instance.active_revision.package_id.as_bytes().as_slice(),
                    instance
                        .active_revision
                        .content_digest
                        .as_bytes()
                        .as_slice()
                ],
            )
            .unwrap();
        }
        let plan = compile(&p, &instance, mode);
        let run = accept(&p, &plan);
        p.admit_deletion(run, &owner, &plan, false, &|_| Ok(()))
            .unwrap()
            .unwrap();
        if mode == DeletionMode::ManagedCleanup {
            p.prepare_deletion_finalization(run, &owner).unwrap();
            while !p.finalize_next_allocation(run, &owner).unwrap() {}
        }
        assert!(
            p.finish_instance_retirement(run, &owner, &success())
                .unwrap()
        );
        assert!(p.detached_allocation(unknown).unwrap().is_none());
        assert!(p.discard_detached_allocation(unknown, true).is_err());
        assert_eq!(
            p.database
                .lock()
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM service_storage_preparations WHERE allocation_id=?1",
                    [unknown.as_bytes().as_slice()],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            p.database
                .lock()
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM service_storage_protections WHERE allocation_id=?1",
                    [unknown.as_bytes().as_slice()],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        drop(p);
        let p = PactrunPersistence::open(temp.path().join("store")).unwrap();
        assert_eq!(p.reconcile_absent_service_preparations().unwrap(), 0);
        assert_eq!(
            std::fs::read(path.join("foreign")).unwrap(),
            b"not admitted or exposed"
        );
    }
}

// Test-ID: PR-TEST-0427
// Verifies: PR-REQ-0337, PR-REQ-0338
#[test]
fn competing_detached_discard_writers_share_one_durable_completion() {
    let (temp, p, instance, roots) = service_fixture();
    abandon(&p, &instance);
    let id = p.list_detached_allocations().unwrap()[0].allocation;
    let (_, path) = roots
        .iter()
        .find(|(allocation, _)| *allocation == id)
        .unwrap();
    let second = PactrunPersistence::open(temp.path().join("store")).unwrap();
    let start = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            start.wait();
            p.discard_detached_allocation(id, true).unwrap()
        });
        let b = scope.spawn(|| {
            start.wait();
            second.discard_detached_allocation(id, true).unwrap()
        });
        [a.join().unwrap(), b.join().unwrap()]
    });
    assert_eq!(
        results.into_iter().filter(|completed| *completed).count(),
        1
    );
    assert!(!path.exists());
    std::fs::create_dir(path).unwrap();
    std::fs::write(path.join("replacement"), b"not owned by receipt").unwrap();
    assert!(!p.discard_detached_allocation(id, true).unwrap());
    assert!(!second.discard_detached_allocation(id, true).unwrap());
    assert_eq!(
        std::fs::read(path.join("replacement")).unwrap(),
        b"not owned by receipt"
    );
    assert_eq!(
        p.detached_allocation(id).unwrap().unwrap().state,
        DetachedAllocationState::Discarded
    );
}

// Test-ID: PR-TEST-0422
// Verifies: PR-REQ-0036, PR-REQ-0336, PR-REQ-0338
#[test]
fn already_accepted_retirement_is_durably_refused_after_another_run_removes_instance() {
    let (_temp, p, instance) = fixture();
    let plan = compile(&p, &instance, DeletionMode::ManagedCleanup);
    let first = accept(&p, &plan);
    let second = accept(&p, &plan);
    let owner = p.staging_session().unwrap().owner();
    p.admit_deletion(first, &owner, &plan, false, &|_| Ok(()))
        .unwrap()
        .unwrap();
    p.prepare_deletion_finalization(first, &owner).unwrap();
    assert!(p.finalize_next_allocation(first, &owner).unwrap());
    assert!(
        p.finish_instance_retirement(first, &owner, &success())
            .unwrap()
    );
    let replacement = p
        .create_instance(
            instance.name.clone(),
            instance.active_revision.clone(),
            &mut [],
        )
        .unwrap();
    assert!(matches!(
        p.admit_deletion(second, &owner, &plan, true, &|_| Ok(()))
            .unwrap(),
        Err(AdmissionRefusal::PlanInvalidated(_))
    ));
    assert!(
        matches!(p.load_managed_run(second).unwrap().unwrap().state, RunState::Finished(outcome) if outcome.outcome == RunOutcome::Failed)
    );
    assert!(p.load_instance_by_id(replacement.id).unwrap().is_some());
    assert!(p.list_managed_runs(replacement.id).unwrap().is_empty());
}

// Test-ID: PR-TEST-0419
// Verifies: PR-REQ-0337, PR-REQ-0338
#[test]
fn corrupt_detached_custody_cannot_authorize_inspection_handoff_or_destruction() {
    for corruption in [
        "mode",
        "protection",
        "unknown_identity",
        "unqualified_intent",
    ] {
        let (_temp, p, instance, roots) = service_fixture();
        abandon(&p, &instance);
        let id = p.list_detached_allocations().unwrap()[0].allocation;
        let db = p.database.lock().unwrap();
        match corruption {
            "mode" => {
                db.execute("UPDATE run_deletion_invocations SET deletion_mode=0", [])
                    .unwrap();
            }
            "protection" => {
                db.execute(
                    "DELETE FROM service_storage_protections WHERE allocation_id=?1",
                    [id.as_bytes().as_slice()],
                )
                .unwrap();
            }
            "unknown_identity" => {
                db.execute(
                    "INSERT INTO allocation_discard_receipts VALUES(?1,0,0,?2)",
                    params![id.as_bytes().as_slice(), [9u8; 40].as_slice()],
                )
                .unwrap();
            }
            "unqualified_intent" => {
                db.execute(
                    "INSERT INTO allocation_discard_receipts VALUES(?1,0,0,NULL)",
                    [id.as_bytes().as_slice()],
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        drop(db);
        assert!(p.list_detached_allocations().is_err(), "{corruption}");
        assert!(p.detached_allocation(id).is_err(), "{corruption}");
        assert!(p.detached_handoff(id).is_err(), "{corruption}");
        assert!(
            p.discard_detached_allocation(id, true).is_err(),
            "{corruption}"
        );
        for (_, path) in roots {
            assert_eq!(
                std::fs::read(path.join("nested/data")).unwrap(),
                b"live payload"
            );
        }
    }
}

// Test-ID: PR-TEST-0420
// Verifies: PR-REQ-0336, PR-REQ-0338
#[test]
fn corrupt_finalization_work_set_cannot_adopt_another_instances_allocation() {
    let (_temp, p, instance, roots) = service_fixture();
    let other = p
        .create_instance(
            InstanceName::parse("other").unwrap(),
            instance.active_revision.clone(),
            &mut [],
        )
        .unwrap();
    let other_allocation = p
        .load_instance_service_state(other.id)
        .unwrap()
        .unwrap()
        .storages[0]
        .allocation;
    let plan = compile(&p, &instance, DeletionMode::ManagedCleanup);
    let run = accept(&p, &plan);
    let owner = p.staging_session().unwrap().owner();
    p.admit_deletion(run, &owner, &plan, false, &|_| Ok(()))
        .unwrap()
        .unwrap();
    p.prepare_deletion_finalization(run, &owner).unwrap();
    p.database
        .lock()
        .unwrap()
        .execute(
            "UPDATE deletion_finalization_allocations SET allocation_id=?1 WHERE allocation_id=?2",
            params![
                other_allocation.as_bytes().as_slice(),
                roots[0].0.as_bytes().as_slice()
            ],
        )
        .unwrap();
    assert!(p.deletion_obligation(instance.id).is_err());
    assert!(p.finalize_next_allocation(run, &owner).is_err());
    assert!(
        p.finish_instance_retirement(run, &owner, &success())
            .is_err()
    );
    assert!(p.load_instance_by_id(other.id).unwrap().is_some());
    for (_, path) in roots {
        assert_eq!(
            std::fs::read(path.join("nested/data")).unwrap(),
            b"live payload"
        );
    }
}

#[test]
fn discard_crash_worker() {
    let Some(root) = std::env::var_os("PACTRUN_M7_DISCARD_ROOT") else {
        return;
    };
    let p = PactrunPersistence::open(std::path::PathBuf::from(root)).unwrap();
    let id = p.list_detached_allocations().unwrap()[0].allocation;
    p.discard_detached_allocation(id, true).unwrap();
}

fn crash_discard(root: &std::path::Path, point: &str) {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "persistence::sqlite_deletions::tests::discard_crash_worker",
            "--nocapture",
        ])
        .env("PACTRUN_M7_DISCARD_ROOT", root)
        .env("PACTRUN_M4_FAULT", point)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(87),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn finalization_crash_worker() {
    let Some(root) = std::env::var_os("PACTRUN_M7_FINALIZE_ROOT") else {
        return;
    };
    let p = PactrunPersistence::open(std::path::PathBuf::from(root)).unwrap();
    let id = p
        .resolve_instance_name(&InstanceName::parse("service").unwrap())
        .unwrap()
        .unwrap();
    let instance = p.load_instance_by_id(id).unwrap().unwrap();
    let plan = compile(&p, &instance, DeletionMode::ManagedCleanup);
    let run = accept(&p, &plan);
    let owner = p.staging_session().unwrap().owner();
    p.admit_deletion(run, &owner, &plan, false, &|_| Ok(()))
        .unwrap()
        .unwrap();
    p.prepare_deletion_finalization(run, &owner).unwrap();
    p.finalize_next_allocation(run, &owner).unwrap();
}

// Test-ID: PR-TEST-0413
// Verifies: PR-REQ-0247, PR-REQ-0334, PR-REQ-0336
#[test]
fn finalization_crashes_preserve_incarnation_and_absence_evidence() {
    for (point, absent) in [
        ("after_finalization_identity_commit", false),
        ("after_finalization_removal", false),
        ("after_finalization_identity_commit", true),
    ] {
        let (temp, p, instance, mut roots) = service_fixture();
        roots.sort_by_key(|(id, _)| *id.as_bytes());
        let (id, path) = &roots[0];
        let preserved = temp.path().join("preserved-original");
        if absent {
            std::fs::rename(path, &preserved).unwrap();
        }
        drop(p);
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "persistence::sqlite_deletions::tests::finalization_crash_worker",
                "--nocapture",
            ])
            .env("PACTRUN_M7_FINALIZE_ROOT", temp.path().join("store"))
            .env("PACTRUN_M4_FAULT", point)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(87),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if path.exists() {
            std::fs::rename(path, &preserved).unwrap();
        }
        std::fs::create_dir(path).unwrap();
        std::fs::write(path.join("replacement"), b"not authorized").unwrap();
        let p = PactrunPersistence::open(temp.path().join("store")).unwrap();
        let old = p.list_managed_runs(instance.id).unwrap().pop().unwrap();
        let RunState::Running(execution) = &old.state else {
            panic!("expected crashed owner")
        };
        // The child has exited: its process lifetime is independently known.
        assert!(p.reconcile_managed_run(old.id, &execution.owner).unwrap());
        assert_eq!(
            p.deletion_obligation(instance.id).unwrap().unwrap().phase,
            DeletionPhase::FinalizationAuthorized
        );
        let current = p.load_instance_by_id(instance.id).unwrap().unwrap();
        let plan = compile(&p, &current, DeletionMode::ManagedCleanup);
        let run = accept(&p, &plan);
        let owner = p.staging_session().unwrap().owner();
        p.admit_deletion(run, &owner, &plan, false, &|_| Ok(()))
            .unwrap()
            .unwrap();
        if absent || (cfg!(target_os = "linux") && point == "after_finalization_removal") {
            while !p.finalize_next_allocation(run, &owner).unwrap() {}
            p.finish_instance_retirement(run, &owner, &success())
                .unwrap();
            assert!(p.load_instance_by_id(instance.id).unwrap().is_none());
        } else {
            assert!(p.finalize_next_allocation(run, &owner).is_err());
            assert!(
                p.finish_instance_retirement(run, &owner, &success())
                    .is_err()
            );
            assert!(p.load_instance_by_id(instance.id).unwrap().is_some());
        }
        assert_eq!(
            std::fs::read(path.join("replacement")).unwrap(),
            b"not authorized",
            "{id}"
        );
        assert!(
            matches!(p.load_managed_run(old.id).unwrap().unwrap().state, RunState::Finished(outcome) if outcome.outcome == RunOutcome::Interrupted)
        );
    }
}

// Test-ID: PR-TEST-0410
// Verifies: PR-REQ-0247, PR-REQ-0337
#[test]
fn discard_crash_recovery_never_deletes_a_replacement_or_runs_implicitly() {
    for point in [
        "after_discard_intent_commit",
        "after_discard_removal",
        "after_discard_completion_commit",
    ] {
        let (temp, p, instance, roots) = service_fixture();
        abandon(&p, &instance);
        let id = p.list_detached_allocations().unwrap()[0].allocation;
        let (_, path) = roots
            .iter()
            .find(|(allocation, _)| *allocation == id)
            .unwrap();
        drop(p);
        crash_discard(&temp.path().join("store"), point);
        let preserved = temp.path().join("preserved-original");
        if path.exists() {
            std::fs::rename(path, &preserved).unwrap();
        }
        std::fs::create_dir(path).unwrap();
        std::fs::write(path.join("replacement"), b"not authorized").unwrap();
        let p = PactrunPersistence::open(temp.path().join("store")).unwrap();
        assert_eq!(
            std::fs::read(path.join("replacement")).unwrap(),
            b"not authorized"
        );
        if point == "after_discard_completion_commit" {
            assert_eq!(
                p.detached_allocation(id).unwrap().unwrap().state,
                DetachedAllocationState::Discarded
            );
            assert!(!p.discard_detached_allocation(id, true).unwrap());
        } else if cfg!(target_os = "linux") && point == "after_discard_removal" {
            assert_eq!(
                p.detached_allocation(id).unwrap().unwrap().state,
                DetachedAllocationState::DiscardPending
            );
            assert!(p.discard_detached_allocation(id, true).unwrap());
            assert_eq!(
                p.detached_allocation(id).unwrap().unwrap().state,
                DetachedAllocationState::Discarded
            );
        } else {
            assert_eq!(
                p.detached_allocation(id).unwrap().unwrap().state,
                DetachedAllocationState::DiscardPending
            );
            assert!(p.discard_detached_allocation(id, true).is_err());
            assert!(p.detached_handoff(id).is_err());
        }
        assert_eq!(
            std::fs::read(path.join("replacement")).unwrap(),
            b"not authorized"
        );
        let (_, sibling) = roots
            .iter()
            .find(|(allocation, _)| *allocation != id)
            .unwrap();
        assert_eq!(
            std::fs::read(sibling.join("nested/data")).unwrap(),
            b"live payload"
        );
    }
}

// Test-ID: PR-TEST-0411
// Verifies: PR-REQ-0247, PR-REQ-0337
#[test]
fn absent_discard_target_is_completed_atomically_and_never_adopts_later_bytes() {
    let (temp, p, instance, roots) = service_fixture();
    abandon(&p, &instance);
    let id = p.list_detached_allocations().unwrap()[0].allocation;
    let (_, path) = roots
        .iter()
        .find(|(allocation, _)| *allocation == id)
        .unwrap();
    std::fs::rename(path, temp.path().join("original-moved-away")).unwrap();
    drop(p);
    crash_discard(&temp.path().join("store"), "after_discard_intent_commit");
    std::fs::create_dir(path).unwrap();
    std::fs::write(path.join("replacement"), b"not authorized").unwrap();
    let p = PactrunPersistence::open(temp.path().join("store")).unwrap();
    assert_eq!(
        p.detached_allocation(id).unwrap().unwrap().state,
        DetachedAllocationState::Discarded
    );
    assert!(!p.discard_detached_allocation(id, true).unwrap());
    assert_eq!(
        std::fs::read(path.join("replacement")).unwrap(),
        b"not authorized"
    );
    assert_eq!(
        std::fs::read(temp.path().join("original-moved-away/nested/data")).unwrap(),
        b"live payload"
    );
}

// Test-ID: PR-TEST-0412
// Verifies: PR-REQ-0247, PR-REQ-0337
#[test]
fn interrupted_discard_needs_explicit_retry_and_can_finish_absent_original() {
    for point in ["after_discard_intent_commit", "after_discard_removal"] {
        let (temp, p, instance, roots) = service_fixture();
        abandon(&p, &instance);
        let id = p.list_detached_allocations().unwrap()[0].allocation;
        let (_, path) = roots
            .iter()
            .find(|(allocation, _)| *allocation == id)
            .unwrap();
        drop(p);
        crash_discard(&temp.path().join("store"), point);
        let p = PactrunPersistence::open(temp.path().join("store")).unwrap();
        assert_eq!(
            p.detached_allocation(id).unwrap().unwrap().state,
            DetachedAllocationState::DiscardPending
        );
        assert_eq!(path.exists(), point == "after_discard_intent_commit");
        assert!(p.discard_detached_allocation(id, false).is_err());
        assert!(p.discard_detached_allocation(id, true).unwrap());
        assert!(!path.exists());
        assert_eq!(
            p.detached_allocation(id).unwrap().unwrap().state,
            DetachedAllocationState::Discarded
        );
    }
}

// Test-ID: PR-TEST-0408
// Verifies: PR-REQ-0111, PR-REQ-0247, PR-REQ-0336, PR-REQ-0338
#[test]
fn real_storage_lifetimes_end_before_instance_removal_and_preserve_history() {
    let (_temp, p, instance, roots) = service_fixture();
    let plan = compile(&p, &instance, DeletionMode::ManagedCleanup);
    let run = accept(&p, &plan);
    let owner = p.staging_session().unwrap().owner();
    p.admit_deletion(run, &owner, &plan, false, &|_| Ok(()))
        .unwrap()
        .unwrap();
    assert!(
        p.finish_instance_retirement(run, &owner, &success())
            .is_err()
    );
    assert!(
        roots
            .iter()
            .all(|(_, root)| root.join("nested/data").exists())
    );
    p.prepare_deletion_finalization(run, &owner).unwrap();
    assert!(!p.finalize_next_allocation(run, &owner).unwrap());
    assert_eq!(roots.iter().filter(|(_, root)| root.exists()).count(), 1);
    assert!(
        p.finish_instance_retirement(run, &owner, &success())
            .is_err()
    );
    while !p.finalize_next_allocation(run, &owner).unwrap() {}
    p.finish_instance_retirement(run, &owner, &success())
        .unwrap();
    assert!(roots.iter().all(|(_, root)| !root.exists()));
    assert!(p.load_instance_by_id(instance.id).unwrap().is_none());
    assert!(p.load_managed_run(run).unwrap().is_some());
    assert!(p.list_detached_allocations().unwrap().is_empty());
}

// Test-ID: PR-TEST-0409
// Verifies: PR-REQ-0181, PR-REQ-0247, PR-REQ-0336, PR-REQ-0337
#[test]
fn partial_finalization_can_be_abandoned_and_only_explicit_discard_destroys_survivors() {
    let (temp, p, instance, roots) = service_fixture();
    let plan = compile(&p, &instance, DeletionMode::ManagedCleanup);
    let run = accept(&p, &plan);
    let owner = p.staging_session().unwrap().owner();
    p.admit_deletion(run, &owner, &plan, false, &|_| Ok(()))
        .unwrap()
        .unwrap();
    p.prepare_deletion_finalization(run, &owner).unwrap();
    assert!(!p.finalize_next_allocation(run, &owner).unwrap());
    p.finish_run_owned(
        &owner,
        run,
        &RunFinish {
            outcome: RunOutcome::Cancelled,
            ..success()
        },
        &[],
        &mut [],
    )
    .unwrap();
    let plan = compile(&p, &instance, DeletionMode::AbandonManagement);
    let abandoned = accept(&p, &plan);
    p.admit_deletion(abandoned, &owner, &plan, false, &|_| Ok(()))
        .unwrap()
        .unwrap();
    assert!(p.prepare_deletion_finalization(abandoned, &owner).is_err());
    p.finish_instance_retirement(abandoned, &owner, &success())
        .unwrap();
    let detached = p.list_detached_allocations().unwrap();
    assert_eq!(detached.len(), 1);
    let id = detached[0].allocation;
    let (_, path) = roots
        .iter()
        .find(|(allocation, _)| *allocation == id)
        .unwrap();
    assert_eq!(
        p.detached_handoff(id).unwrap()[0].path,
        std::fs::canonicalize(path).unwrap()
    );
    assert!(!p.load_instance_by_id(instance.id).unwrap().is_some());
    drop(p);
    let p = PactrunPersistence::open(temp.path().join("store")).unwrap();
    assert_eq!(
        std::fs::read(path.join("nested/data")).unwrap(),
        b"live payload"
    );
    assert!(p.discard_detached_allocation(id, false).is_err());
    assert!(path.exists());
    assert!(p.discard_detached_allocation(id, true).unwrap());
    assert!(!path.exists());
    std::fs::create_dir(path).unwrap();
    std::fs::write(path.join("replacement"), b"must survive").unwrap();
    assert!(!p.discard_detached_allocation(id, true).unwrap());
    assert_eq!(
        std::fs::read(path.join("replacement")).unwrap(),
        b"must survive"
    );
    assert_eq!(
        p.detached_allocation(id).unwrap().unwrap().state,
        DetachedAllocationState::Discarded
    );
    assert!(p.detached_handoff(id).is_err());
}

// Test-ID: PR-TEST-0396
// Verifies: PR-REQ-0036, PR-REQ-0037, PR-REQ-0040, PR-REQ-0177
#[test]
fn deletion_compilation_is_read_only_and_no_hook_admission_ignores_ordinary_readiness() {
    let (_temp, p, instance) = fixture();
    assert!(!instance.required_inputs_satisfied);
    let plan = compile(&p, &instance, DeletionMode::ManagedCleanup);
    assert!(matches!(plan.work, DeletionWork::NoCleanup));
    let abandon = compile(&p, &instance, DeletionMode::AbandonManagement);
    assert!(matches!(abandon.work, DeletionWork::Abandon));
    assert!(p.list_managed_runs(instance.id).unwrap().is_empty());
    assert!(p.deletion_obligation(instance.id).unwrap().is_none());
    assert_eq!(
        p.load_instance_by_id(instance.id).unwrap().unwrap(),
        instance
    );
    let run = accept(&p, &plan);
    assert!(matches!(
        p.load_managed_run(run).unwrap().unwrap().state,
        RunState::Running(RunExecutionView {
            boundary: ActionRunBoundary::Accepted,
            ..
        })
    ));
    p.admit_deletion(
        run,
        &p.staging_session().unwrap().owner(),
        &plan,
        false,
        &|_| panic!("no Hook must not inspect a launcher"),
    )
    .unwrap()
    .unwrap();
    assert!(matches!(
        p.load_managed_run(run).unwrap().unwrap().state,
        RunState::Running(RunExecutionView {
            boundary: ActionRunBoundary::Admitted,
            ..
        })
    ));
    assert!(
        p.authorize_cleanup_launch(run, &p.staging_session().unwrap().owner())
            .is_err()
    );
}

// Test-ID: PR-TEST-0397
// Verifies: PR-REQ-0007, PR-REQ-0036, PR-REQ-0334
#[test]
fn deletion_refusal_is_durable_and_stale_compilation_never_launches() {
    let (_temp, p, instance) = fixture();
    let plan = compile(&p, &instance, DeletionMode::ManagedCleanup);
    let run = accept(&p, &plan);
    p.database
        .lock()
        .unwrap()
        .execute(
            "UPDATE instances SET instance_state_version=?1",
            [InstanceStateVersion::from_bytes([9; 16])
                .as_bytes()
                .as_slice()],
        )
        .unwrap();
    assert!(matches!(
        p.admit_deletion(
            run,
            &p.staging_session().unwrap().owner(),
            &plan,
            true,
            &|_| Ok(())
        )
        .unwrap(),
        Err(AdmissionRefusal::PlanInvalidated(_))
    ));
    let RunState::Finished(outcome) = p.load_managed_run(run).unwrap().unwrap().state else {
        panic!("refusal did not terminate accepted Run");
    };
    assert_eq!(outcome.outcome, RunOutcome::Failed);
    assert!(p.deletion_obligation(instance.id).unwrap().is_none());
}

// Test-ID: PR-TEST-0398
// Verifies: PR-REQ-0335
#[test]
fn confirmation_preserves_interrupted_history_and_atomically_authorizes_only_finalization() {
    let (temp, p, instance) = fixture_with_cleanup(true);
    let plan = compile(&p, &instance, DeletionMode::ManagedCleanup);
    let run = accept(&p, &plan);
    let owner = p.staging_session().unwrap().owner();
    p.admit_deletion(run, &owner, &plan, false, &|_| Ok(()))
        .unwrap()
        .unwrap();
    p.authorize_cleanup_launch(run, &owner).unwrap();
    assert!(p.authorize_cleanup_launch(run, &owner).is_err());
    drop(p);
    let p = PactrunPersistence::open(temp.path().join("store")).unwrap();
    assert_eq!(
        crate::managed_data::probe_session_owner(&temp.path().join("store"), &owner),
        crate::managed_data::SessionOwnerProbe::ConfirmedLoss
    );
    assert!(p.reconcile_managed_run(run, &owner).unwrap());
    let before = p.load_managed_run(run).unwrap().unwrap();
    let confirm = CleanupConfirmation {
        instance: instance.id,
        attempt: run,
        expected: instance.state_version,
    };
    assert!(
        p.confirm_cleanup_completion(CleanupConfirmation {
            attempt: RunId::generate().unwrap(),
            ..confirm
        })
        .is_err()
    );
    assert_eq!(
        p.load_instance_by_id(instance.id).unwrap().unwrap(),
        instance
    );
    let next = p.confirm_cleanup_completion(confirm).unwrap();
    assert_ne!(next, instance.state_version);
    assert_eq!(p.load_managed_run(run).unwrap().unwrap(), before);
    assert_eq!(
        p.deletion_obligation(instance.id).unwrap().unwrap().phase,
        DeletionPhase::FinalizationAuthorized
    );
    assert!(p.confirm_cleanup_completion(confirm).is_err());
    let current = p.load_instance_by_id(instance.id).unwrap().unwrap();
    assert!(
        matches!(compile(&p, &current, DeletionMode::ManagedCleanup).work,
        DeletionWork::FinalizationOnly { attempt } if attempt == run)
    );
    assert!(matches!(
        compile(&p, &current, DeletionMode::AbandonManagement).work,
        DeletionWork::Abandon
    ));
}
