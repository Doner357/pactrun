use super::*;
use crate::persistence::ManagedInputWrite;
use crate::revision_core_v2::{project_revision_core_source_v2, validate_revision_content_v2};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn root() -> (tempfile::TempDir, PathBuf) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m65-allocation-tests");
    fs::create_dir_all(&parent).unwrap();
    let temporary = tempfile::tempdir_in(parent).unwrap();
    let root = temporary.path().join("store");
    for name in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    (temporary, root)
}
fn fixture(p: &PactrunPersistence) -> RevisionIdentity {
    // Minimal no-runtime Revision fixture for storage-focused tests. Production
    // V2 source installation is exercised separately by the operation/CLI tests.
    let core=project_revision_core_source_v2(br#"{
      "format_version":2,"inputs":[],"actions":[],"migrations":[],
      "service_storages":[{"id":"state"}],
      "service_resources":[
        {"id":"config","storage_id":"state","locator":"config.json","kind":"file","read_exposure":"readable","user_mutation":{"kind":"direct"}},
        {"id":"database","storage_id":"state","locator":"db","kind":"directory","read_exposure":"hidden","user_mutation":{"kind":"unavailable"}}
      ]}"#).unwrap();
    let runtime =
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![] })
            .unwrap();
    let content = validate_revision_content_v2(core, runtime).unwrap();
    p.persist_revision_internal(PackageId::from_bytes([65; 16]), &content.into(), &[], None)
        .unwrap()
}
fn count(p: &PactrunPersistence, table: &str) -> i64 {
    p.database
        .lock()
        .unwrap()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn allocation(p: &PactrunPersistence, instance: InstanceId) -> ServiceAllocationId {
    let bytes: Vec<u8> = p
        .database
        .lock()
        .unwrap()
        .query_row(
            "SELECT allocation_id FROM instance_service_storages WHERE instance_id=?1",
            [instance.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    ServiceAllocationId::from_bytes(bytes.try_into().unwrap())
}
fn directory(root: &Path, id: ServiceAllocationId) -> PathBuf {
    root.join("service-storage").join(format!("alloc-{id}"))
}

// Test-ID: PR-TEST-0346
// Verifies: PR-REQ-0012, PR-REQ-0025, PR-REQ-0027, PR-REQ-0128, PR-REQ-0131, PR-REQ-0141, PR-REQ-0235, PR-REQ-0236, PR-REQ-0241, PR-REQ-0242, PR-REQ-0323, PR-REQ-0324
#[test]
fn eager_instance_storage_is_isolated_and_live_bytes_are_not_managed_inputs() {
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let revision = fixture(&p);
    let first = p
        .create_instance(
            InstanceName::parse("first").unwrap(),
            revision.clone(),
            &mut [],
        )
        .unwrap();
    let second = p
        .create_instance(
            InstanceName::parse("second").unwrap(),
            revision.clone(),
            &mut [],
        )
        .unwrap();
    let a = allocation(&p, first.id);
    let b = allocation(&p, second.id);
    assert_ne!(a, b);
    for id in [a, b] {
        let path = directory(&root, id);
        assert!(path.is_dir());
        assert!(!path.join("config.json").exists());
        assert!(!path.join("db").exists());
    }
    assert_eq!(count(&p, "service_storage_protections"), 2);
    assert_eq!(count(&p, "service_storage_preparations"), 0);
    assert_eq!(count(&p, "instance_service_resources"), 4);
    assert_eq!(count(&p, "managed_input_payloads"), 0);
    assert_eq!(count(&p, "runs"), 0);
    fs::write(
        directory(&root, a).join("config.json"),
        b"live service data",
    )
    .unwrap();
    assert!(!directory(&root, b).join("config.json").exists());
    assert_eq!(
        p.load_instance_by_id(first.id)
            .unwrap()
            .unwrap()
            .state_version,
        first.state_version
    );
    assert_eq!(count(&p, "managed_input_payloads"), 0);
    assert_eq!(count(&p, "snapshots"), 0);
    let mut bytes = Cursor::new(b"not an Input".to_vec());
    assert!(
        p.create_instance(
            InstanceName::parse("bad-input").unwrap(),
            revision,
            &mut [ManagedInputWrite {
                input_id: InputIdentity::parse("config").unwrap(),
                byte_len: 12,
                reader: &mut bytes
            }]
        )
        .is_err()
    );
    assert_eq!(count(&p, "service_storage_allocations"), 2);
    drop(p);
    let p = PactrunPersistence::open(&root).unwrap();
    assert_eq!(
        fs::read(directory(&root, a).join("config.json")).unwrap(),
        b"live service data"
    );
    assert_eq!(
        p.load_instance_by_id(first.id)
            .unwrap()
            .unwrap()
            .state_version,
        first.state_version
    );
    p.database
        .lock()
        .unwrap()
        .execute(
            "DELETE FROM service_storage_protections WHERE allocation_id=?1",
            [a.as_bytes().as_slice()],
        )
        .unwrap();
    assert!(matches!(
        p.load_instance_by_id(first.id),
        Err(PersistenceError::CorruptServiceStorage(_))
    ));
    assert_eq!(
        fs::read(directory(&root, a).join("config.json")).unwrap(),
        b"live service data"
    );
}

#[test]
fn allocation_worker() {
    let Some(root) = std::env::var_os("PACTRUN_ALLOCATION_ROOT") else {
        return;
    };
    let p = PactrunPersistence::open(PathBuf::from(root)).unwrap();
    if std::env::var_os("PACTRUN_ALLOCATION_MAINTENANCE").is_some() {
        p.reconcile_absent_service_preparations().unwrap();
        return;
    }
    let revision = fixture(&p);
    p.create_instance(InstanceName::parse("created").unwrap(), revision, &mut [])
        .unwrap();
}

// Test-ID: PR-TEST-0348
// Verifies: PR-REQ-0324
#[test]
fn publication_refuses_missing_or_replaced_prepared_namespace() {
    for change in ["missing", "allocation", "container"] {
        let (_temp, root) = root();
        let p = PactrunPersistence::open(&root).unwrap();
        let revision = fixture(&p);
        let instance = InstanceId::generate().unwrap();
        let prepared = p.prepare_instance_storage(instance, &revision).unwrap();
        let (_, allocation, _) = &prepared.roots[0];
        let original = directory(&root, *allocation);
        if change == "container" {
            let renamed = fs::rename(root.join("service-storage"), root.join("old-container"));
            // NTFS can reject renaming a parent with opened descendants. That
            // is namespace exclusion, not evidence that a replaced parent was
            // successfully qualified. Linux exercises the replacement below.
            #[cfg(windows)]
            if renamed
                .as_ref()
                .is_err_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied)
            {
                let (_, _, base) = prepared.namespace.as_ref().unwrap();
                let root_handle = crate::service_storage::open_root(&root).unwrap();
                let current =
                    crate::service_storage::open_directory(&root_handle, "service-storage")
                        .unwrap();
                assert!(crate::service_storage::same_opened_object(base, &current).unwrap());
                assert!(original.is_dir());
                assert!(!root.join("old-container").exists());
                continue;
            }
            renamed.unwrap();
            fs::create_dir(root.join("service-storage")).unwrap();
            fs::create_dir(&original).unwrap();
        } else {
            fs::rename(&original, original.with_file_name("old-allocation")).unwrap();
            if change == "allocation" {
                fs::create_dir(&original).unwrap();
            }
        }
        let mut db = p.database.lock().unwrap();
        let tx = db.transaction().unwrap();
        assert!(
            matches!(
                prepared.publish(&tx, instance, &revision),
                Err(PersistenceError::ServiceStorageUnavailable(_))
            ),
            "{change}"
        );
        tx.rollback().unwrap();
        drop(db);
        assert_eq!(count(&p, "service_storage_preparations"), 1);
        assert_eq!(count(&p, "service_storage_protections"), 0);
        assert_eq!(count(&p, "instance_service_storages"), 0);
        assert_eq!(count(&p, "instance_service_resources"), 0);
    }
}

// Test-ID: PR-TEST-0347
// Verifies: PR-REQ-0323, PR-REQ-0324
#[test]
fn allocation_crashes_leave_intent_or_complete_protected_instance_not_partial_associations() {
    for (point, intent, published) in [
        ("before_service_allocation_intent_commit", false, false),
        ("after_service_allocation_intent_commit", true, false),
        ("after_service_allocation_directory", true, false),
        ("before_service_instance_commit", true, false),
        ("after_service_instance_commit", true, true),
    ] {
        let (_temp, root) = root();
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "persistence::sqlite_service_storage::tests::allocation_worker",
                "--nocapture",
            ])
            .env("PACTRUN_ALLOCATION_ROOT", &root)
            .env("PACTRUN_M4_FAULT", point)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(87), "{point}");
        let p = PactrunPersistence::open(&root).unwrap();
        assert_eq!(count(&p, "service_storage_allocations"), i64::from(intent));
        assert_eq!(count(&p, "instances"), i64::from(published));
        assert_eq!(count(&p, "instance_service_storages"), i64::from(published));
        assert_eq!(
            count(&p, "instance_service_resources"),
            2 * i64::from(published)
        );
        assert_eq!(
            count(&p, "service_storage_protections"),
            i64::from(published)
        );
        assert_eq!(
            count(&p, "service_storage_preparations"),
            i64::from(intent && !published)
        );
        if published {
            let instance = p
                .resolve_instance_name(&InstanceName::parse("created").unwrap())
                .unwrap()
                .unwrap();
            assert!(directory(&root, allocation(&p, instance)).is_dir());
            assert!(p.load_instance_by_id(instance).unwrap().is_some());
        }
        assert_eq!(count(&p, "runs"), 0);
    }
}

// Test-ID: PR-TEST-0349
// Verifies: PR-REQ-0323
#[test]
fn strict_service_reader_rejects_undeclared_retained_and_wrong_owner_rows() {
    for corrupt in [
        "INSERT INTO instance_service_storages SELECT instance_id, CAST('ghost' AS BLOB), allocation_id, declaration_package_id, declaration_revision_digest FROM instance_service_storages",
        "INSERT INTO instance_service_resources SELECT instance_id, CAST('ghost' AS BLOB), allocation_id, declaration_package_id, declaration_revision_digest FROM instance_service_resources WHERE resource_identity=CAST('config' AS BLOB)",
        "UPDATE service_storage_allocations SET instance_id=zeroblob(16)",
        "UPDATE service_storage_allocations SET package_id=zeroblob(16)",
        "UPDATE instance_service_resources SET resource_identity=x'ff' WHERE resource_identity=CAST('config' AS BLOB)",
    ] {
        let (_temp, root) = root();
        let p = PactrunPersistence::open(&root).unwrap();
        let revision = fixture(&p);
        let instance = p
            .create_instance(InstanceName::parse("first").unwrap(), revision, &mut [])
            .unwrap();
        let live = directory(&root, allocation(&p, instance.id)).join("config.json");
        fs::write(&live, b"do not repair or delete").unwrap();
        p.database.lock().unwrap().execute(corrupt, []).unwrap();
        assert!(
            matches!(
                p.load_instance_by_id(instance.id),
                Err(PersistenceError::CorruptServiceStorage(_))
            ),
            "{corrupt}"
        );
        assert_eq!(fs::read(live).unwrap(), b"do not repair or delete");
    }
}

// Test-ID: PR-TEST-0350
// Verifies: PR-REQ-0323, PR-REQ-0328
#[test]
fn retained_contracts_remain_validated_with_a_v1_current_revision() {
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let revision = fixture(&p);
    let instance = p
        .create_instance(
            InstanceName::parse("first").unwrap(),
            revision.clone(),
            &mut [],
        )
        .unwrap();
    let old = p.load_revision(&revision).unwrap().unwrap();
    let legacy = validate_revision_content_v1(
        old.content.core.common().clone(),
        old.content.runtime_content.clone(),
    )
    .unwrap();
    let legacy = p
        .persist_revision(revision.package_id, &legacy, &[])
        .unwrap();
    // Construct a retained-state reader fixture; this is not Migration execution
    // or evidence that the runtime can yet publish a service-to-V1 transition.
    p.database
        .lock()
        .unwrap()
        .execute(
            "UPDATE instances SET active_revision_content_digest=?1 WHERE instance_id=?2",
            params![
                legacy.content_digest.as_bytes().as_slice(),
                instance.id.as_bytes().as_slice()
            ],
        )
        .unwrap();
    assert!(p.load_instance_by_id(instance.id).unwrap().is_some());
    let retained = p.load_instance_service_state(instance.id).unwrap().unwrap();
    assert!(
        retained
            .storages
            .iter()
            .all(|s| s.role == ServiceRole::Retained)
    );
    assert!(
        retained
            .resources
            .iter()
            .all(|r| r.role == ServiceRole::Retained)
    );
    assert_eq!(retained.resources.len(), 2);
    assert!(
        retained
            .resources
            .iter()
            .all(|r| r.declaration_revision == revision)
    );
    let live = directory(&root, allocation(&p, instance.id)).join("config.json");
    fs::write(&live, b"retained service data").unwrap();
    assert_eq!(
        service_cli(&root, &["resource", "show", "first", "config"]).0,
        1
    );
    let show = service_cli(
        &root,
        &["resource", "show", "first", "config", "--retained"],
    );
    assert_eq!(show.0, 0);
    assert!(show.1.contains("stored_mutation: direct"));
    assert!(
        show.1
            .contains("effective_mutation: unavailable_until_reattachment")
    );
    assert_eq!(
        service_cli(
            &root,
            &[
                "resource",
                "locate",
                "first",
                "config",
                "--retained",
                "--intent",
                "read"
            ]
        )
        .0,
        0
    );
    let write = service_cli(
        &root,
        &[
            "resource",
            "locate",
            "first",
            "config",
            "--retained",
            "--intent",
            "write",
        ],
    );
    assert_eq!(write.0, 1);
    assert!(write.1.is_empty());
    assert!(write.2.contains("exposure_denied"));
    assert_eq!(fs::read(live).unwrap(), b"retained service data");
    p.database
        .lock()
        .unwrap()
        .execute(
            "UPDATE instance_service_resources SET declaration_revision_digest=?1",
            [legacy.content_digest.as_bytes().as_slice()],
        )
        .unwrap();
    assert!(matches!(
        p.load_instance_by_id(instance.id),
        Err(PersistenceError::CorruptServiceStorage(_))
    ));
}

// Test-ID: PR-TEST-0351
// Verifies: PR-REQ-0324
#[test]
fn preparation_maintenance_preserves_live_unknown_and_existing_objects() {
    for case in ["live", "unknown", "empty", "contents", "absent"] {
        let (_temp, root) = root();
        let p = PactrunPersistence::open(&root).unwrap();
        let revision = fixture(&p);
        let instance = InstanceId::generate().unwrap();
        let prepared = p.prepare_instance_storage(instance, &revision).unwrap();
        let allocation = prepared.roots[0].1;
        let path = directory(&root, allocation);
        drop(prepared);
        match case {
            "empty" => (),
            "contents" => fs::write(path.join("sentinel"), b"service-owned").unwrap(),
            _ => fs::remove_dir(&path).unwrap(), // test-owned empty fixture only
        }
        if case == "unknown" {
            let unknown = format!("session-{}", "f".repeat(32));
            assert!(ExecutionOwnerSession::parse(&unknown).is_ok());
            fs::create_dir(root.join("staging").join(&unknown)).unwrap();
            p.database
                .lock()
                .unwrap()
                .execute(
                    "UPDATE service_storage_preparations SET owner_session=?1",
                    [unknown.as_bytes()],
                )
                .unwrap();
        }
        if case == "live" {
            let other = PactrunPersistence::open(&root).unwrap();
            assert_eq!(other.reconcile_absent_service_preparations().unwrap(), 0);
            assert_eq!(count(&other, "service_storage_preparations"), 1);
            continue;
        }
        drop(p);
        let p = PactrunPersistence::open(&root).unwrap();
        let expected = usize::from(case == "absent");
        assert_eq!(
            p.reconcile_absent_service_preparations().unwrap(),
            expected,
            "{case}"
        );
        assert_eq!(
            count(&p, "service_storage_preparations"),
            1 - expected as i64
        );
        assert_eq!(p.reconcile_absent_service_preparations().unwrap(), 0);
        if case == "contents" {
            assert_eq!(fs::read(path.join("sentinel")).unwrap(), b"service-owned");
        }
        if case == "empty" {
            assert!(path.is_dir());
        }
        assert_eq!(count(&p, "instances"), 0);
        assert_eq!(count(&p, "runs"), 0);
    }
}

// Test-ID: PR-TEST-0352
// Verifies: PR-REQ-0324
#[test]
fn concurrent_preparation_maintenance_is_serialized_and_idempotent() {
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let revision = fixture(&p);
    let prepared = p
        .prepare_instance_storage(InstanceId::generate().unwrap(), &revision)
        .unwrap();
    let path = directory(&root, prepared.roots[0].1);
    drop(prepared);
    fs::remove_dir(path).unwrap(); // test-owned empty fixture only
    drop(p);
    let mut children = Vec::new();
    for _ in 0..2 {
        children.push(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "persistence::sqlite_service_storage::tests::allocation_worker",
                    "--nocapture",
                ])
                .env("PACTRUN_ALLOCATION_ROOT", &root)
                .env("PACTRUN_ALLOCATION_MAINTENANCE", "1")
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
    }
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    let p = PactrunPersistence::open(&root).unwrap();
    assert_eq!(count(&p, "service_storage_preparations"), 0);
    assert_eq!(count(&p, "service_storage_allocations"), 0);
    assert_eq!(count(&p, "runs"), 0);
}

// Test-ID: PR-TEST-0353
// Verifies: PR-REQ-0323, PR-REQ-0328
#[test]
fn contract_views_are_read_only_and_do_not_probe_live_presence() {
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let revision = fixture(&p);
    let instance = p
        .create_instance(
            InstanceName::parse("first").unwrap(),
            revision.clone(),
            &mut [],
        )
        .unwrap();
    let allocation = allocation(&p, instance.id);
    let prepared = p.prepare_instance_storage(instance.id, &revision).unwrap();
    let preserved_id = prepared.roots[0].1;
    let origin_run = RunId::generate().unwrap();
    {
        // Construct protected target-attempt custody, not a published resource
        // and not evidence of the future Hook/Migration publication path.
        let mut db = p.database.lock().unwrap();
        let tx = db.transaction().unwrap();
        tx.execute(
            "DELETE FROM service_storage_preparations WHERE allocation_id=?1",
            [preserved_id.as_bytes().as_slice()],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO service_storage_protections VALUES(?1)",
            [preserved_id.as_bytes().as_slice()],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO service_storage_run_origins VALUES(?1,?2)",
            params![
                preserved_id.as_bytes().as_slice(),
                origin_run.as_bytes().as_slice()
            ],
        )
        .unwrap();
        tx.commit().unwrap();
    }
    drop(prepared);
    drop(p);
    fs::rename(
        directory(&root, allocation),
        root.join("unavailable-allocation"),
    )
    .unwrap();
    let stages = fs::read_dir(root.join("staging")).unwrap().count();
    let reader = PactrunPersistence::open_read_only(&root).unwrap();
    let before = [
        count(&reader, "runs"),
        count(&reader, "writable_admissions"),
        count(&reader, "service_storage_allocations"),
    ];
    let state = reader
        .load_instance_service_state(instance.id)
        .unwrap()
        .unwrap();
    assert_eq!(state.storages.len(), 1);
    assert_eq!(state.storages[0].role, ServiceRole::Active);
    assert_eq!(
        state
            .resources
            .iter()
            .map(|r| r.declaration.id.as_str())
            .collect::<Vec<_>>(),
        ["config", "database"]
    );
    assert_eq!(
        state.resources[1].declaration.read_exposure,
        ServiceReadExposure::Hidden
    );
    assert_eq!(state.preserved.len(), 1);
    assert_eq!(state.preserved[0].allocation, preserved_id);
    assert_eq!(state.preserved[0].origin_run, Some(origin_run));
    assert_eq!(state.preserved[0].origin_revision, revision);
    assert_eq!(
        before,
        [
            count(&reader, "runs"),
            count(&reader, "writable_admissions"),
            count(&reader, "service_storage_allocations")
        ]
    );
    assert_eq!(fs::read_dir(root.join("staging")).unwrap().count(), stages);
    assert_eq!(
        reader
            .load_instance_by_id(instance.id)
            .unwrap()
            .unwrap()
            .state_version,
        instance.state_version
    );
    assert!(!directory(&root, allocation).exists());
    assert!(
        reader
            .load_instance_service_state(InstanceId::generate().unwrap())
            .unwrap()
            .is_none()
    );
}

fn service_cli(root: &Path, args: &[&str]) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = crate::cli::run(
        args.iter().map(Into::into).collect(),
        Some(root.as_os_str().to_owned()),
        &mut std::io::empty(),
        &mut out,
        &mut err,
    );
    (
        status,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

// Test-ID: PR-TEST-0357
// Verifies: PR-REQ-0328, PR-REQ-0242, PR-REQ-0243
#[test]
fn service_cli_observes_real_resources_and_only_locate_discloses_native_paths() {
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let revision = fixture(&p);
    let instance = p
        .create_instance(InstanceName::parse("first").unwrap(), revision, &mut [])
        .unwrap();
    let allocation = allocation(&p, instance.id);
    let live = directory(&root, allocation);
    drop(p);
    let stages = fs::read_dir(root.join("staging")).unwrap().count();
    for args in [
        vec!["service-storage", "list", "first"],
        vec!["resource", "list", "first"],
        vec!["resource", "show", "first", "config"],
    ] {
        let (status, out, err) = service_cli(&root, &args);
        assert_eq!(status, 0, "{err}");
        assert!(!out.contains(&format!("alloc-{allocation}")));
        assert!(err.is_empty());
    }
    assert_eq!(
        service_cli(&root, &["resource", "list", "first", "--retained"]),
        (0, String::new(), String::new())
    );
    assert_eq!(
        service_cli(&root, &["resource", "observe", "first", "config"]),
        (0, "Absent\n".into(), String::new())
    );
    assert_eq!(
        service_cli(&root, &["resource", "observe", "first", "database"]).0,
        0
    ); // hidden metadata observation
    let read = service_cli(
        &root,
        &["resource", "locate", "first", "config", "--intent", "read"],
    );
    assert_eq!(read.0, 1);
    assert!(read.2.contains("resource_absent"));
    let write = service_cli(
        &root,
        &["resource", "locate", "first", "config", "--intent", "write"],
    );
    assert_eq!(write.0, 0, "{}", write.2);
    assert!(write.1.contains(&format!("alloc-{allocation}")));
    assert!(!live.join("config.json").exists());
    fs::write(live.join("config.json"), b"service owns these bytes").unwrap();
    let observed = service_cli(&root, &["resource", "observe", "first", "config"]);
    assert_eq!(
        observed,
        (
            0,
            "Present kind=file kind_matches=true\n".into(),
            String::new()
        )
    );
    assert_eq!(
        service_cli(
            &root,
            &["resource", "locate", "first", "config", "--intent", "read"]
        )
        .0,
        0
    );
    fs::hard_link(live.join("config.json"), live.join("second-link")).unwrap();
    let refused = service_cli(
        &root,
        &["resource", "locate", "first", "config", "--intent", "read"],
    );
    assert_eq!(refused.0, 1);
    assert!(refused.2.contains("unsafe_path"));
    assert!(refused.1.is_empty());
    assert!(!refused.2.contains(&format!("alloc-{allocation}")));
    let hidden = service_cli(
        &root,
        &[
            "resource", "locate", "first", "database", "--intent", "read",
        ],
    );
    assert_eq!(hidden.0, 1);
    assert!(hidden.2.contains("exposure_denied"));
    fs::rename(&live, root.join("missing-protected-root")).unwrap();
    let unknown = service_cli(&root, &["resource", "observe", "first", "config"]);
    assert_eq!(unknown.0, 1);
    assert_eq!(unknown.1, "Unknown cause=allocation_unavailable\n");
    assert!(!live.exists());
    assert_eq!(
        fs::read(root.join("missing-protected-root/config.json")).unwrap(),
        b"service owns these bytes"
    );
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    assert_eq!(
        p.load_instance_by_id(instance.id)
            .unwrap()
            .unwrap()
            .state_version,
        instance.state_version
    );
    assert_eq!(count(&p, "runs"), 0);
    assert_eq!(fs::read_dir(root.join("staging")).unwrap().count(), stages);
}

// Test-ID: PR-TEST-0358
// Verifies: PR-REQ-0328
#[test]
fn service_cli_rejects_unsupported_or_ambiguous_syntax_before_opening_store() {
    let (_temp, root) = root();
    for args in [
        vec!["resource", "locate", "first", "config"],
        vec![
            "resource", "locate", "first", "config", "--intent", "delete",
        ],
        vec![
            "resource", "locate", "first", "config", "--intent", "read", "--intent", "write",
        ],
        vec!["resource", "list", "first", "--retained", "--retained"],
        vec!["resource", "show", "first", "config", "--json"],
        vec!["resource", "list", "first", "--intent", "read"],
        vec!["service-storage", "delete", "first"],
        vec!["resource", "write", "first", "config"],
        vec!["resource", "observe", "first"],
    ] {
        let (status, out, _) = service_cli(&root, &args);
        assert_eq!(status, 2, "{args:?}");
        assert!(out.is_empty());
    }
    assert!(!root.join("database/pactrun.sqlite3").exists());
    assert!(!root.join("service-storage").exists());
    assert_eq!(fs::read_dir(root.join("staging")).unwrap().count(), 0);
}

// Test-ID: PR-TEST-0359
// Verifies: PR-REQ-0328, PR-REQ-0243
#[test]
fn service_cli_preserves_operation_routes_missing_parents_and_broken_output_boundaries() {
    use sha2::Digest;
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let payload = b"this fixture executable must never be invoked by resource locate";
    let digest = Sha256Digest::from_bytes(sha2::Sha256::digest(payload).into());
    let publication = p
        .put_runtime_content(&digest, &mut Cursor::new(payload))
        .unwrap();
    let core = project_revision_core_source_v2(br#"{
      "format_version":2,"inputs":[],"migrations":[],
      "service_storages":[{"id":"state"}],
      "service_resources":[
        {"id":"config","storage_id":"state","locator":"config.json","kind":"file","read_exposure":"readable","user_mutation":{"kind":"operation","action_id":"edit"}},
        {"id":"nested","storage_id":"state","locator":"parent/file","kind":"file","read_exposure":"readable","user_mutation":{"kind":"direct"}}
      ],
      "actions":[{"id":"edit","access":"mutate","parameters":[],"outputs":[],"hook":{
        "protocol_version":2,"launch":{"kind":"direct","executable":"tool"},"args":[],"io":{"terminal":"none"},
        "service_access":[{"reference":{"view":"current","role":"active","kind":"resource","id":"config"},"mode":"write"}],"service_requires":[]
      }}]
    }"#).unwrap();
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
    let content = validate_revision_content_v2(core, runtime).unwrap();
    let revision = p
        .persist_revision_internal(
            PackageId::from_bytes([65; 16]),
            &content.into(),
            &[publication],
            None,
        )
        .unwrap();
    let instance = p
        .create_instance(InstanceName::parse("first").unwrap(), revision, &mut [])
        .unwrap();
    let live = directory(&root, allocation(&p, instance.id));
    let denied = service_cli(
        &root,
        &["resource", "locate", "first", "config", "--intent", "write"],
    );
    assert_eq!(denied.0, 1);
    assert!(denied.1.is_empty());
    assert!(denied.2.contains("use declared Action edit"));
    assert!(!live.join("config.json").exists());
    let absent_parent = service_cli(
        &root,
        &["resource", "locate", "first", "nested", "--intent", "write"],
    );
    assert_eq!(absent_parent.0, 1);
    assert!(absent_parent.2.contains("unsafe_path"));
    assert!(!live.join("parent").exists());
    fs::create_dir(live.join("parent")).unwrap();
    assert_eq!(
        service_cli(
            &root,
            &["resource", "locate", "first", "nested", "--intent", "write"]
        )
        .0,
        0
    );
    assert!(!live.join("parent/file").exists());
    fs::create_dir(live.join("parent/file")).unwrap();
    assert_eq!(
        service_cli(&root, &["resource", "observe", "first", "nested"]).1,
        "Present kind=directory kind_matches=false\n"
    );
    let mismatch = service_cli(
        &root,
        &["resource", "locate", "first", "nested", "--intent", "write"],
    );
    assert_eq!(mismatch.0, 1);
    assert!(mismatch.2.contains("resource_kind_mismatch"));
    struct BrokenOutput;
    impl std::io::Write for BrokenOutput {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert_eq!(
        crate::cli::run(
            vec!["resource".into(), "list".into(), "first".into()],
            Some(root.as_os_str().to_owned()),
            &mut std::io::empty(),
            &mut BrokenOutput,
            &mut Vec::new()
        ),
        1
    );
    assert_eq!(count(&p, "runs"), 0);
    assert_eq!(
        p.load_instance_by_id(instance.id)
            .unwrap()
            .unwrap()
            .state_version,
        instance.state_version
    );
}

// Test-ID: PR-TEST-0368
// Verifies: PR-REQ-0321, PR-REQ-0323, PR-REQ-0326, PR-REQ-0244
#[test]
fn compiled_service_associations_are_revalidated_and_pins_are_atomic_and_owner_scoped() {
    use super::super::sqlite_service_admission::{
        pin_service_bindings, qualify_current_service_facts,
    };
    use serde_json::json;
    use sha2::Digest;
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let base = fixture(&p);
    let base = p.load_revision(&base).unwrap().unwrap();
    let mut source: serde_json::Value = serde_json::from_slice(
        &crate::revision_core_v2::encode_canonical_revision_core_v2(
            base.content.core.service_core().unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let reference = json!({"view":"current","role":"active","kind":"resource","id":"config"});
    source["actions"] = json!([{"id":"inspect","access":"observe","parameters":[],"outputs":[],"hook":{
        "protocol_version":2,"launch":{"kind":"direct","executable":"tool"},"args":[],"io":{"terminal":"none"},
        "service_access":[{"reference":reference,"mode":"read"}],"service_requires":[{"reference":reference,"presence":"absent"}]
    }}]);
    let core = project_revision_core_source_v2(&serde_json::to_vec(&source).unwrap()).unwrap();
    let bytes = b"metadata pin fixture; not executed";
    let digest = Sha256Digest::from_bytes(sha2::Sha256::digest(bytes).into());
    let publication = p
        .put_runtime_content(&digest, &mut Cursor::new(bytes))
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
    let content = validate_revision_content_v2(core, runtime).unwrap();
    let revision = p
        .persist_revision_internal(
            PackageId::from_bytes([65; 16]),
            &content.clone().into(),
            &[publication],
            None,
        )
        .unwrap();
    let instance = p
        .create_instance(
            InstanceName::parse("first").unwrap(),
            revision.clone(),
            &mut [],
        )
        .unwrap();
    let other = p
        .create_instance(
            InstanceName::parse("other").unwrap(),
            revision.clone(),
            &mut [],
        )
        .unwrap();
    let observed = p.load_instance_service_state(instance.id).unwrap().unwrap();
    let contract = content.core.hooks()
        [&ServiceHookSite::Action(ActionIdentity::parse("inspect").unwrap())]
        .clone();
    let bindings = bind_current_service_hook(
        &contract,
        Some(&observed),
        instance.id,
        instance.state_version,
        &revision,
    )
    .unwrap();
    let launch = CompiledHookLaunch::Direct {
        executable: content.runtime_content.files()[0].clone(),
    };
    let mut facts = AdmissionFacts {
        expected_state_version: instance.state_version,
        active_bindings: &[],
        runtime_content: content.runtime_content.files(),
        launch: &launch,
        service: Some((&contract, &bindings)),
    };
    let action = ActionRunIdentity {
        revision: revision.clone(),
        action: ActionIdentity::parse("inspect").unwrap(),
    };
    let invocation = ManagedRunIdentity::Action(action.clone());
    {
        let db = p.database.lock().unwrap();
        assert!(
            qualify_current_service_facts(&db, instance.id, &revision, &invocation, &facts)
                .unwrap()
                .is_none()
        );
        facts.service = None;
        assert!(
            qualify_current_service_facts(&db, instance.id, &revision, &invocation, &facts)
                .unwrap()
                .is_some()
        );
    }
    let mut foreign = bindings.clone();
    let foreign_allocation = allocation(&p, other.id);
    for (_, object) in &mut foreign.grants {
        match object {
            BoundServiceObject::Storage { allocation }
            | BoundServiceObject::Resource { allocation, .. } => *allocation = foreign_allocation,
        }
    }
    for (_, object) in &mut foreign.requires {
        match object {
            BoundServiceObject::Storage { allocation }
            | BoundServiceObject::Resource { allocation, .. } => *allocation = foreign_allocation,
        }
    }
    facts.service = Some((&contract, &foreign));
    assert!(
        qualify_current_service_facts(
            &p.database.lock().unwrap(),
            instance.id,
            &revision,
            &invocation,
            &facts
        )
        .unwrap()
        .is_some()
    );
    let owner = p.staging_session().unwrap().owner();
    let run = p
        .create_accepted_run(
            RunId::generate().unwrap(),
            instance.id,
            instance.state_version,
            &action,
            &owner,
            &super::super::UnconditionalAcceptance,
        )
        .unwrap();
    for commit in [false, true] {
        let mut db = p.database.lock().unwrap();
        let tx = db.transaction().unwrap();
        // Pin-publication transaction fixture, not proof of V2 Hook Admission:
        // protocol/physical authority integration remains in the supervisor path.
        tx.execute(
            "INSERT INTO run_revision_pins VALUES(?1,?2,?3)",
            params![
                run.as_bytes().as_slice(),
                revision.package_id.as_bytes().as_slice(),
                revision.content_digest.as_bytes().as_slice()
            ],
        )
        .unwrap();
        pin_service_bindings(&tx, run, &bindings).unwrap();
        assert_eq!(
            tx.query_row("SELECT COUNT(*) FROM run_service_storage_pins", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            1
        );
        if commit {
            tx.commit().unwrap();
        } else {
            tx.rollback().unwrap();
        }
        drop(db);
        assert_eq!(count(&p, "run_service_storage_pins"), i64::from(commit));
    }
    {
        let mut db = p.database.lock().unwrap();
        let tx = db.transaction().unwrap();
        assert!(matches!(
            pin_service_bindings(&tx, run, &foreign),
            Err(PersistenceError::CorruptServiceStorage(_))
        ));
        tx.rollback().unwrap();
    }
    let root_id = allocation(&p, instance.id);
    let live = directory(&root, root_id).join("config.json");
    let native = crate::hook::service_storage::prepare(&p, run, &owner, &bindings).unwrap();
    assert_eq!(native.grant_count(), 1);
    assert!(!live.exists()); // read authority is not an implicit Present predicate
    let other_owner = PactrunPersistence::open(&root).unwrap();
    assert!(
        other_owner
            .open_pinned_service_allocation(run, &owner, root_id)
            .is_err()
    );
    assert!(
        p.open_pinned_service_allocation(run, &owner, foreign_allocation)
            .is_err()
    );
    fs::write(&live, b"service bytes survive Run release").unwrap();
    assert!(matches!(
        crate::hook::service_storage::prepare(&p, run, &owner, &bindings),
        Err(crate::hook::service_storage::NativeServiceError::Prerequisite)
    ));
    fs::rename(directory(&root, root_id), root.join("moved-allocation")).unwrap();
    assert!(matches!(
        crate::hook::service_storage::prepare(&p, run, &owner, &bindings),
        Err(crate::hook::service_storage::NativeServiceError::Storage(
            PersistenceError::ServiceStorageUnavailable(_)
        ))
    ));
    assert!(!directory(&root, root_id).exists());
    fs::rename(root.join("moved-allocation"), directory(&root, root_id)).unwrap();
    p.finish_run_owned(
        &owner,
        run,
        &RunFinish {
            outcome: RunOutcome::Cancelled,
            primary_failure: None,
            secondary_failures: vec![],
            hook_completion: None,
        },
        &[],
        &mut [],
    )
    .unwrap();
    assert_eq!(count(&p, "run_service_storage_pins"), 0);
    assert!(
        p.open_pinned_service_allocation(run, &owner, root_id)
            .is_err()
    );
    assert_eq!(count(&p, "service_storage_protections"), 2);
    assert_eq!(
        fs::read(live).unwrap(),
        b"service bytes survive Run release"
    );
}

// Test-ID: PR-TEST-0370
// Verifies: PR-REQ-0321, PR-REQ-0326, PR-REQ-0235, PR-REQ-0239, PR-REQ-0244, PR-REQ-0057, PR-REQ-0070
#[test]
fn real_v2_service_operations_use_isolated_live_storage_and_preserve_recovery_boundaries() {
    use crate::{
        application::PactrunApplication,
        executor::AdmissionOptions,
        hook::{ActionCancellation, HookRuntimePolicy},
    };
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let executable = fs::read(std::env::current_exe().unwrap()).unwrap();
    let core = project_revision_core_source_v2(br#"{
      "format_version":2,"inputs":[],"migrations":[],"service_storages":[{"id":"state"}],
      "service_resources":[{"id":"config","storage_id":"state","locator":"config.json","kind":"file","read_exposure":"hidden","user_mutation":{"kind":"unavailable"}}],
      "actions":[{"id":"initialize","access":"mutate","parameters":[{"id":"mode","type":"string","sensitive":false,"default":"storage"}],"outputs":[],"hook":{
        "protocol_version":2,"launch":{"kind":"direct","executable":"tool"},"args":["--exact","hook::tests::v2_runtime::v2_action_worker","--nocapture","--test-threads=1"],"io":{"terminal":"none"},
        "service_access":[{"reference":{"view":"current","role":"active","kind":"resource","id":"config"},"mode":"write"}],
        "service_requires":[{"reference":{"view":"current","role":"active","kind":"resource","id":"config"},"presence":"absent"}]
      }}],
      "snapshot":{
        "capture":{"access":"observe","parameters":[],"hook":{
          "protocol_version":2,"launch":{"kind":"direct","executable":"tool"},"args":["--exact","hook::tests::v2_runtime::v2_snapshot_worker","--nocapture","--test-threads=1"],"io":{"terminal":"none"},
          "service_access":[{"reference":{"view":"current","role":"active","kind":"resource","id":"config"},"mode":"read"}],
          "service_requires":[{"reference":{"view":"current","role":"active","kind":"resource","id":"config"},"presence":"present"}]
        }},
        "restore":{"parameters":[],"hook":{
          "protocol_version":2,"launch":{"kind":"direct","executable":"tool"},"args":["--exact","hook::tests::v2_runtime::v2_snapshot_worker","--nocapture","--test-threads=1"],"io":{"terminal":"none"},
          "service_access":[{"reference":{"view":"current","role":"active","kind":"resource","id":"config"},"mode":"write"}],"service_requires":[]
        }}
      }
    }"#).unwrap();
    let mut definition: serde_json::Value = serde_json::from_slice(
        &crate::revision_core_v2::encode_canonical_revision_core_v2(&core).unwrap(),
    )
    .unwrap();
    definition.as_object_mut().unwrap().remove("format_version");
    let source = root.parent().unwrap().join("runtime-source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("tool"), executable).unwrap();
    fs::write(source.join("pactrun.yaml"), format!(
        "source_format: 2\npackage_id: 00000000000000000000000000000065\nrevision: {definition}\nruntime_content:\n  files: [{{id: tool, source: tool, path: bin/tool, executable: true}}]\n"
    )).unwrap();
    let app = PactrunApplication::open(&root).unwrap();
    let revision = app
        .install_pack_source(
            &source,
            &RevisionMetadataMutationBatch::new(vec![]).unwrap(),
        )
        .unwrap()
        .revision;
    assert!(!root.join("service-storage").exists());
    // Execute only acquired immutable content, never the original source file.
    fs::remove_file(source.join("tool")).unwrap();
    let first = app
        .create_instance(
            InstanceName::parse("first").unwrap(),
            revision.clone(),
            vec![],
        )
        .unwrap();
    let second = app
        .create_instance(
            InstanceName::parse("second").unwrap(),
            revision.clone(),
            vec![],
        )
        .unwrap();
    let first_path = directory(&root, allocation(&p, first.id)).join("config.json");
    let second_path = directory(&root, allocation(&p, second.id)).join("config.json");
    let run = |name: &str, mode: Option<&str>| {
        let params = mode
            .map(|mode| {
                vec![RawParameterInput {
                    id: ParameterIdentity::parse("mode").unwrap(),
                    text: mode.into(),
                    source: ParameterTextSource::Ordinary,
                }]
            })
            .unwrap_or_default();
        let intent = app
            .resolve_action(
                &InstanceName::parse(name).unwrap(),
                &ActionIdentity::parse("initialize").unwrap(),
                params,
            )
            .unwrap();
        let plan = app.compile_action(&intent, &[]).unwrap();
        let admitted = app
            .accept_and_admit_action(&plan, AdmissionOptions::default())
            .unwrap();
        let run = admitted.run();
        assert_eq!(count(&p, "run_service_storage_pins"), 1);
        app.execute_admitted_action(
            admitted,
            HookRuntimePolicy::from_millis(Some(10000), Some(10000), Some(5000)).unwrap(),
            ActionCancellation::default(),
        );
        assert!(app.advance_owner_continuation(run).unwrap());
        assert_eq!(count(&p, "run_service_storage_pins"), 0);
        p.load_run(run).unwrap().unwrap()
    };
    assert!(
        matches!(run("first", None).state, RunState::Finished(o) if o.outcome == RunOutcome::Succeeded && o.terminal_risk == RecoveryRiskState::Clear)
    );
    assert_eq!(fs::read(&first_path).unwrap(), b"service-owned V2 bytes");
    assert!(!second_path.exists());
    assert_eq!(
        app.load_instance(first.id).unwrap().unwrap().state_version,
        first.state_version
    );
    assert_eq!(count(&p, "managed_input_payloads"), 0);
    assert_eq!(count(&p, "snapshots"), 0);
    let snapshot_run = |instance: InstanceId,
                        operation: SnapshotOperation,
                        recovery_override: bool| {
        let plan = crate::workflow::compile_snapshot(
            &app,
            &crate::workflow::PlatformHostLauncherLookup,
            &SnapshotIntent {
                instance,
                operation,
                parameters: vec![],
            },
            &[],
        )
        .unwrap();
        let run = app
            .accept_snapshot_plan(
                plan,
                AdmissionOptions { recovery_override },
                ActionCancellation::default(),
            )
            .unwrap();
        assert!(!app.advance_owner_continuation(run).unwrap());
        assert_eq!(count(&p, "run_service_storage_pins"), 1);
        let policy = HookRuntimePolicy::from_millis(Some(10000), Some(10000), Some(5000)).unwrap();
        match operation {
            SnapshotOperation::Capture => app.execute_admitted_capture(run, policy).unwrap(),
            SnapshotOperation::Restore(_) => app.execute_admitted_restore(run, policy).unwrap(),
        };
        if recovery_override {
            assert!(p.load_instance_recovery_guard(instance).unwrap().is_some());
        }
        assert!(app.advance_owner_continuation(run).unwrap());
        assert!(
            matches!(p.load_managed_run(run).unwrap().unwrap().state, RunState::Finished(o) if o.outcome == RunOutcome::Succeeded)
        );
        assert_eq!(count(&p, "run_service_storage_pins"), 0);
    };
    snapshot_run(first.id, SnapshotOperation::Capture, false);
    let snapshot = p.list_snapshots(Some(first.id)).unwrap()[0].id;
    let refused = run("first", None);
    assert!(
        matches!(refused.state, RunState::Finished(o) if o.outcome == RunOutcome::Failed && o.hook_completion.is_none())
    );
    assert_eq!(fs::read(&first_path).unwrap(), b"service-owned V2 bytes");
    assert!(
        matches!(run("second", None).state, RunState::Finished(o) if o.outcome == RunOutcome::Succeeded)
    );
    assert_eq!(fs::read(second_path).unwrap(), b"service-owned V2 bytes");
    let risky = app
        .create_instance(InstanceName::parse("risky").unwrap(), revision, vec![])
        .unwrap();
    assert!(
        matches!(run("risky", Some("open_success")).state, RunState::Finished(o)
        if o.outcome == RunOutcome::Failed && o.terminal_risk == RecoveryRiskState::Open && o.hook_completion.as_ref().is_some_and(|c| c.status == HookCompletionStatus::Success))
    );
    assert!(p.load_instance_recovery_guard(risky.id).unwrap().is_some());
    assert_eq!(
        fs::read(directory(&root, allocation(&p, risky.id)).join("config.json")).unwrap(),
        b"service-owned V2 bytes"
    );
    snapshot_run(risky.id, SnapshotOperation::Capture, true);
    assert!(p.load_instance_recovery_guard(risky.id).unwrap().is_some());
    let risky_path = directory(&root, allocation(&p, risky.id)).join("config.json");
    fs::write(&risky_path, b"damaged live representation").unwrap();
    snapshot_run(risky.id, SnapshotOperation::Restore(snapshot), true);
    assert!(p.load_instance_recovery_guard(risky.id).unwrap().is_none());
    assert_eq!(fs::read(risky_path).unwrap(), b"service-owned V2 bytes");
}

fn compatible_service_targets(p: &PactrunPersistence) -> [RevisionIdentity; 3] {
    use serde_json::{Value, json};
    let base = fixture(p);
    let stored = p.load_revision(&base).unwrap().unwrap();
    let resources = stored.content.core.service_core().unwrap().resources();
    let mut settings = serde_json::to_value(
        resources
            .iter()
            .find(|r| r.id.as_str() == "config")
            .unwrap(),
    )
    .unwrap();
    settings["id"] = json!("settings");
    settings["storage_id"] = json!("data");
    let mut database = serde_json::to_value(
        resources
            .iter()
            .find(|r| r.id.as_str() == "database")
            .unwrap(),
    )
    .unwrap();
    database["storage_id"] = json!("data");
    let install = |source: &RevisionIdentity,
                   storage_source: &str,
                   resources: Vec<Value>,
                   transitions: Value| {
        let core = project_revision_core_source_v2(&serde_json::to_vec(&json!({"format_version":2,"inputs":[],"actions":[],
            "service_storages":[{"id":"data"}],"service_resources":resources,
            "migrations":[{"source_revision_digest":source.content_digest.to_string(),"transitions":[],"requires_source":[],"requires_target":[],"produces_target":[],
                "storage_transitions":[{"kind":"reuse","source":{"role":"active","storage_id":storage_source},"target_storage_id":"data"}],"resource_transitions":transitions}]})).unwrap()).unwrap();
        let content = validate_revision_content_v2(
            core,
            project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![] })
                .unwrap(),
        )
        .unwrap();
        p.persist_revision_internal(base.package_id, &content.into(), &[], None)
            .unwrap()
    };
    let renamed = install(
        &base,
        "state",
        vec![settings.clone()],
        json!([{"kind":"reuse","source":{"role":"active","resource_id":"config"},"target_resource_id":"settings"}]),
    );
    let attached = install(
        &renamed,
        "data",
        vec![settings, database],
        json!([
            {"kind":"reuse","source":{"role":"active","resource_id":"settings"},"target_resource_id":"settings"},
            {"kind":"reattach","source":{"role":"retained","resource_id":"database"},"target_resource_id":"database"}
        ]),
    );
    [base, renamed, attached]
}

// Test-ID: PR-TEST-0374
// Verifies: PR-REQ-0166, PR-REQ-0237, PR-REQ-0245, PR-REQ-0319, PR-REQ-0323, PR-REQ-0326, PR-REQ-0327
#[test]
fn real_declarative_service_migrations_consume_renames_retain_unmapped_and_reattach_explicitly() {
    use crate::{
        application::PactrunApplication,
        hook::{ActionCancellation, HookRuntimePolicy},
    };
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let path = compatible_service_targets(&p);
    let app = PactrunApplication::open(&root).unwrap();
    let instance = app
        .create_instance(
            InstanceName::parse("first").unwrap(),
            path[0].clone(),
            vec![],
        )
        .unwrap();
    let allocation = allocation(&p, instance.id);
    let live = directory(&root, allocation);
    fs::write(
        live.join("config.json"),
        b"not JSON: \0\xff unchanged service bytes",
    )
    .unwrap();
    fs::create_dir(live.join("db")).unwrap();
    fs::write(live.join("db/table"), b"unmapped source-only contents").unwrap();
    let migrate = |target: &RevisionIdentity| {
        let current = app.load_instance(instance.id).unwrap().unwrap();
        let plan = crate::workflow::compile_migration(
            &app,
            &crate::workflow::PlatformHostLauncherLookup,
            &TransitionRevision {
                instance: instance.id,
                expected_state_version: current.state_version,
                source: current.active_revision.clone(),
                target: target.clone(),
                path: MigrationPathSelection::Exact(vec![current.active_revision, target.clone()]),
                operator_inputs: vec![],
                authorize_declassification: false,
            },
            &[],
        )
        .unwrap();
        assert!(plan.edges()[0].service.is_some());
        assert!(
            plan.edges()[0].launch.is_none(),
            "declarative service continuity must not launch a Hook"
        );
        let run = app
            .accept_migration_inputs(
                plan,
                false,
                ActionCancellation::default(),
                vec![],
                HookRuntimePolicy::default(),
            )
            .unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !app.advance_owner_continuation(run).unwrap() {
            assert!(std::time::Instant::now() < until);
        }
        assert!(
            matches!(p.load_managed_run(run).unwrap().unwrap().state, RunState::Finished(o) if o.outcome==RunOutcome::Succeeded)
        );
    };
    migrate(&path[1]);
    let renamed = p.load_instance_service_state(instance.id).unwrap().unwrap();
    assert_eq!(renamed.current_revision, path[1]);
    assert_eq!(renamed.storages.len(), 1);
    assert_eq!(renamed.storages[0].declaration.id.as_str(), "data");
    assert!(
        !renamed
            .resources
            .iter()
            .any(|r| r.declaration.id.as_str() == "config")
    );
    assert!(
        renamed
            .resources
            .iter()
            .any(|r| r.declaration.id.as_str() == "settings" && r.role == ServiceRole::Active)
    );
    assert!(
        renamed
            .resources
            .iter()
            .any(|r| r.declaration.id.as_str() == "database"
                && r.role == ServiceRole::Retained
                && r.allocation == allocation)
    );
    assert_eq!(
        service_cli(
            &root,
            &["resource", "show", "first", "config", "--retained"]
        )
        .0,
        1
    );
    migrate(&path[2]);
    let attached = p.load_instance_service_state(instance.id).unwrap().unwrap();
    assert_eq!(attached.current_revision, path[2]);
    assert!(
        attached
            .resources
            .iter()
            .all(|r| r.role == ServiceRole::Active && r.allocation == allocation)
    );
    assert_eq!(
        fs::read(live.join("config.json")).unwrap(),
        b"not JSON: \0\xff unchanged service bytes"
    );
    assert_eq!(
        fs::read(live.join("db/table")).unwrap(),
        b"unmapped source-only contents"
    );
    assert_eq!(count(&p, "service_storage_allocations"), 1);
    assert_eq!(count(&p, "service_storage_protections"), 1);
    assert_eq!(count(&p, "run_service_storage_pins"), 0);
    assert_eq!(count(&p, "run_service_edge_commits"), 0); // compatible edges are not transforms
}

#[test]
fn service_migration_worker() {
    let Some(root) = std::env::var_os("PACTRUN_SERVICE_MIGRATION_ROOT") else {
        return;
    };
    let target = std::env::var("PACTRUN_SERVICE_MIGRATION_TARGET").unwrap();
    let result = service_cli(
        Path::new(&root),
        &["instance", "migrate", "first", "--to", &target],
    );
    assert_eq!(result.0, 0, "{}", result.2);
}

// Test-ID: PR-TEST-0375
// Verifies: PR-REQ-0323, PR-REQ-0326, PR-REQ-0327
#[test]
fn service_association_commit_crashes_leave_only_complete_old_or_new_boundaries() {
    for (point, committed) in [
        ("before_migration_edge_commit", false),
        ("after_migration_edge_commit", true),
    ] {
        let (_temp, root) = root();
        let p = PactrunPersistence::open(&root).unwrap();
        let path = compatible_service_targets(&p);
        let instance = p
            .create_instance(
                InstanceName::parse("first").unwrap(),
                path[0].clone(),
                &mut [],
            )
            .unwrap();
        let live = directory(&root, allocation(&p, instance.id)).join("config.json");
        fs::write(&live, b"live bytes are not rolled back").unwrap();
        drop(p);
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "persistence::sqlite_service_storage::tests::service_migration_worker",
                "--nocapture",
            ])
            .env("PACTRUN_SERVICE_MIGRATION_ROOT", &root)
            .env(
                "PACTRUN_SERVICE_MIGRATION_TARGET",
                format!("exact:{}/{}", path[1].package_id, path[1].content_digest),
            )
            .env("PACTRUN_M4_FAULT", point)
            .env("PACTRUN_MIGRATION_FAULT_EDGE", "0")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(87), "{point}");
        let app = crate::application::PactrunApplication::open(&root).unwrap();
        app.reconcile_lost_action_owners().unwrap();
        let p = PactrunPersistence::open_read_only(&root).unwrap();
        let state = p.load_instance_service_state(instance.id).unwrap().unwrap();
        assert_eq!(state.current_revision, path[usize::from(committed)]);
        assert_eq!(state.storages.len(), 1);
        assert_eq!(state.resources.len(), 2);
        assert_eq!(
            state.storages[0].declaration.id.as_str(),
            if committed { "data" } else { "state" }
        );
        assert!(state.resources.iter().any(|r| r.declaration.id.as_str()==if committed {"settings"} else {"config"}));
        assert_eq!(count(&p, "run_migration_boundaries"), i64::from(committed));
        assert_eq!(count(&p, "run_service_storage_pins"), 0);
        assert_eq!(count(&p, "service_storage_protections"), 1);
        assert_eq!(fs::read(&live).unwrap(), b"live bytes are not rolled back");
    }
}

// Test-ID: PR-TEST-0376
// Verifies: PR-REQ-0323, PR-REQ-0324, PR-REQ-0326, PR-REQ-0327
#[test]
fn new_target_storage_is_protected_before_publication_and_retry_keeps_its_selection() {
    use serde_json::json;
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let source = fixture(&p);
    let core = project_revision_core_source_v2(&serde_json::to_vec(&json!({"format_version":2,"inputs":[],"actions":[],
        "service_storages":[{"id":"fresh"}],"service_resources":[{"id":"fresh_config","storage_id":"fresh","locator":"config.json","kind":"file","read_exposure":"readable","user_mutation":{"kind":"direct"}}],
        "migrations":[{"source_revision_digest":source.content_digest.to_string(),"transitions":[],"requires_source":[],"requires_target":[],"produces_target":[],
            "storage_transitions":[{"kind":"create","target_storage_id":"fresh"}],"resource_transitions":[{"kind":"create","target_resource_id":"fresh_config","presence":"absent"}]}]})).unwrap()).unwrap();
    let content = validate_revision_content_v2(
        core,
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![] })
            .unwrap(),
    )
    .unwrap();
    let target = p
        .persist_revision_internal(source.package_id, &content.into(), &[], None)
        .unwrap();
    let instance = p
        .create_instance(
            InstanceName::parse("first").unwrap(),
            source.clone(),
            &mut [],
        )
        .unwrap();
    let old = allocation(&p, instance.id);
    let app = crate::application::PactrunApplication::open_read_only(&root).unwrap();
    let plan = crate::workflow::compile_migration(
        &app,
        &crate::workflow::PlatformHostLauncherLookup,
        &TransitionRevision {
            instance: instance.id,
            expected_state_version: instance.state_version,
            source: source.clone(),
            target: target.clone(),
            path: MigrationPathSelection::Exact(vec![source.clone(), target.clone()]),
            operator_inputs: vec![],
            authorize_declassification: false,
        },
        &[],
    )
    .unwrap();
    let owner = p.staging_session().unwrap().owner();
    let run = p
        .create_accepted_managed_run(
            RunId::generate().unwrap(),
            instance.id,
            instance.state_version,
            &ManagedRunIdentity::Migration(plan.invocation()),
            &owner,
            &super::super::UnconditionalAcceptance,
        )
        .unwrap();
    p.admit_migration(run, &owner, &plan, false, &|_| Ok(()))
        .unwrap()
        .unwrap();
    assert_eq!(count(&p, "service_storage_allocations"), 1);
    let prepared = p.prepare_migration_service_targets(run, &owner, 0).unwrap();
    let selected =
        &prepared.after.storages[&ServiceStorageIdentity::parse("fresh").unwrap()].allocation;
    let ServiceAllocationOrigin::Existing(new) = selected else {
        panic!("target root must be concrete before exposure")
    };
    assert_ne!(*new, old);
    assert!(directory(&root, *new).is_dir());
    assert!(!directory(&root, *new).join("config.json").exists());
    assert_eq!(count(&p, "service_storage_protections"), 2);
    assert_eq!(count(&p, "run_service_storage_targets"), 1);
    assert_eq!(
        p.load_instance_by_id(instance.id)
            .unwrap()
            .unwrap()
            .active_revision,
        source
    );
    assert!(
        !p.load_instance_service_state(instance.id)
            .unwrap()
            .unwrap()
            .resources
            .iter()
            .any(|r| r.declaration.id.as_str() == "fresh_config")
    );
    assert_eq!(
        p.prepare_migration_service_targets(run, &owner, 0).unwrap(),
        prepared
    );
    assert_eq!(count(&p, "service_storage_allocations"), 2);
    // A remembered target selection is not sufficient authority if its pin or
    // protection has disappeared. Corruption must not be normalized on retry.
    for table in ["run_service_storage_pins", "service_storage_protections"] {
        {
            let db = p.database.lock().unwrap();
            db.execute(
                &format!("DELETE FROM {table} WHERE allocation_id=?1"),
                [new.as_bytes().as_slice()],
            )
            .unwrap();
        }
        assert!(matches!(
            p.prepare_migration_service_targets(run, &owner, 0),
            Err(PersistenceError::CorruptServiceStorage(_))
        ));
        let db = p.database.lock().unwrap();
        if table == "run_service_storage_pins" {
            db.execute(
                "INSERT INTO run_service_storage_pins VALUES(?1,?2)",
                params![run.as_bytes().as_slice(), new.as_bytes().as_slice()],
            )
            .unwrap();
        } else {
            db.execute(
                "INSERT INTO service_storage_protections VALUES(?1)",
                [new.as_bytes().as_slice()],
            )
            .unwrap();
        }
    }
    p.publish_declarative_migration_edge(run, &owner, 0)
        .unwrap();
    let published = p.load_instance_service_state(instance.id).unwrap().unwrap();
    assert_eq!(published.current_revision, target);
    assert!(
        published
            .resources
            .iter()
            .any(|r| r.declaration.id.as_str() == "fresh_config"
                && r.allocation == *new
                && r.role == ServiceRole::Active)
    );
    assert!(
        published
            .resources
            .iter()
            .any(|r| r.declaration.id.as_str() == "config"
                && r.allocation == old
                && r.role == ServiceRole::Retained)
    );
    assert_eq!(count(&p, "run_service_storage_targets"), 0);
    assert_eq!(count(&p, "run_service_resource_targets"), 0);
    assert_eq!(count(&p, "run_service_storage_pins"), 0);
    assert_eq!(count(&p, "service_storage_protections"), 2);
    assert!(!directory(&root, *new).join("config.json").exists());
}

fn transform_target(
    p: &PactrunPersistence,
    source: &RevisionIdentity,
    mode: &str,
) -> RevisionIdentity {
    use serde_json::json;
    let bytes = fs::read(std::env::current_exe().unwrap()).unwrap();
    let mut definition = json!({"format_version":2,"inputs":[],"actions":[],
        "service_storages":[{"id":"target_store"}],"service_resources":[{"id":"settings","storage_id":"target_store","locator":"settings.json","kind":"file","read_exposure":"readable","user_mutation":{"kind":"direct"}}],
        "migrations":[{"source_revision_digest":source.content_digest.to_string(),"transitions":[],"requires_source":[],"requires_target":[],"produces_target":[],
            "storage_transitions":[{"kind":"create","target_storage_id":"target_store"}],
            "resource_transitions":[{"kind":"transform","sources":[{"role":"active","resource_id":"config"}],"targets":["settings"]}],
            "hook":{"protocol_version":2,"launch":{"kind":"direct","executable":"tool"},"args":["--exact","hook::tests::v2_runtime::v2_transform_worker","--nocapture","--test-threads=1","--skip",format!("transform-mode:{mode}")],"io":{"terminal":"none"},"service_requires":[],
                "service_access":[{"reference":{"view":"source","role":"active","kind":"resource","id":"config"},"mode":"read"},{"reference":{"view":"target","role":"active","kind":"resource","id":"settings"},"mode":"write"}]}}]});
    if matches!(mode, "outputs" | "bad_output") {
        definition["inputs"] = json!([{"id":"result","required":true,"protection":"normal"}]);
        definition["migrations"][0]["produces_target"] = json!(["result"]);
    }
    if mode == "nested" {
        definition["service_resources"] = json!([
            {"id":"bundle","storage_id":"target_store","locator":"bundle","kind":"directory","read_exposure":"readable","user_mutation":{"kind":"direct"}},
            {"id":"index","storage_id":"target_store","locator":"bundle/index.json","kind":"file","read_exposure":"readable","user_mutation":{"kind":"direct"}}
        ]);
        definition["migrations"][0]["resource_transitions"][0]["targets"] =
            json!(["bundle", "index"]);
        definition["migrations"][0]["hook"]["service_access"] = json!([
            {"reference":{"view":"source","role":"active","kind":"resource","id":"config"},"mode":"read"},
            {"reference":{"view":"target","role":"active","kind":"resource","id":"bundle"},"mode":"write"},
            {"reference":{"view":"target","role":"active","kind":"resource","id":"index"},"mode":"write"}
        ]);
    }
    definition.as_object_mut().unwrap().remove("format_version");
    let root = p.database_path.parent().unwrap().parent().unwrap();
    let source_directory = tempfile::tempdir_in(root.parent().unwrap()).unwrap();
    fs::write(source_directory.path().join("tool"), bytes).unwrap();
    fs::write(source_directory.path().join("pactrun.yaml"), format!(
        "source_format: 2\npackage_id: {}\nrevision: {definition}\nruntime_content:\n  files: [{{id: tool, source: tool, path: bin/tool, executable: true}}]\n", source.package_id
    )).unwrap();
    crate::application::PactrunApplication::open(root)
        .unwrap()
        .install_pack_source(
            source_directory.path(),
            &RevisionMetadataMutationBatch::new(vec![]).unwrap(),
        )
        .unwrap()
        .revision
}

// Test-ID: PR-TEST-0377
// Verifies: PR-REQ-0026, PR-REQ-0246, PR-REQ-0322, PR-REQ-0323, PR-REQ-0326
#[test]
fn transform_receipt_keeps_open_risk_until_atomic_target_publication() {
    use crate::{
        application::PactrunApplication,
        hook::{ActionCancellation, HookRuntimePolicy},
    };
    for mode in [
        "success",
        "outputs",
        "failure",
        "extra",
        "nonzero",
        "bad_output",
    ] {
        let (_temp, root) = root();
        let p = PactrunPersistence::open(&root).unwrap();
        let source = fixture(&p);
        let target = transform_target(&p, &source, mode);
        let app = PactrunApplication::open(&root).unwrap();
        let instance = app
            .create_instance(
                InstanceName::parse("first").unwrap(),
                source.clone(),
                vec![],
            )
            .unwrap();
        let source_path = directory(&root, allocation(&p, instance.id)).join("config.json");
        fs::write(&source_path, b"old service state").unwrap();
        let plan = crate::workflow::compile_migration(
            &app,
            &crate::workflow::PlatformHostLauncherLookup,
            &TransitionRevision {
                instance: instance.id,
                expected_state_version: instance.state_version,
                source: source.clone(),
                target: target.clone(),
                path: MigrationPathSelection::Exact(vec![source.clone(), target.clone()]),
                operator_inputs: vec![],
                authorize_declassification: false,
            },
            &[],
        )
        .unwrap();
        let run = app
            .accept_migration_inputs(
                plan,
                false,
                ActionCancellation::default(),
                vec![],
                HookRuntimePolicy::from_millis(Some(10000), Some(10000), Some(500)).unwrap(),
            )
            .unwrap();
        let mut observed_proposal_window = false;
        let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !app.advance_owner_continuation(run).unwrap() {
            if let RunState::Running(e) = p.load_managed_run(run).unwrap().unwrap().state
                && e.risk_state == RecoveryRiskState::Open
            {
                observed_proposal_window = true;
                assert_eq!(
                    p.load_instance_by_id(instance.id)
                        .unwrap()
                        .unwrap()
                        .active_revision,
                    source
                );
                assert_eq!(count(&p, "run_service_edge_commits"), 0);
            }
            assert!(std::time::Instant::now() < until, "{mode} stalled");
        }
        assert!(observed_proposal_window, "{mode}");
        let RunState::Finished(outcome) = p.load_managed_run(run).unwrap().unwrap().state else {
            panic!("terminal Run")
        };
        let success = matches!(mode, "success" | "outputs");
        assert_eq!(
            outcome.outcome,
            if success {
                RunOutcome::Succeeded
            } else {
                RunOutcome::Failed
            },
            "{mode}: {outcome:?}"
        );
        assert_eq!(
            outcome.terminal_risk,
            if success {
                RecoveryRiskState::Clear
            } else {
                RecoveryRiskState::Open
            }
        );
        assert!(outcome.hook_completion.is_none() || mode == "failure");
        assert_eq!(count(&p, "run_service_edge_commits"), i64::from(success));
        assert_eq!(count(&p, "service_storage_protections"), 2);
        assert_eq!(count(&p, "run_service_storage_targets"), 0);
        assert_eq!(fs::read(source_path).unwrap(), b"old service state");
        let state = p.load_instance_service_state(instance.id).unwrap().unwrap();
        assert_eq!(
            state.current_revision,
            if success { target } else { source }
        );
        assert_eq!(
            p.load_instance_recovery_guard(instance.id)
                .unwrap()
                .is_some(),
            !success
        );
        if success {
            if mode == "outputs" {
                let mut bytes = Vec::new();
                p.export_input(
                    instance.id,
                    &InputIdentity::parse("result").unwrap(),
                    false,
                    &mut bytes,
                )
                .unwrap();
                assert_eq!(bytes, b"Pactrun-owned transform output");
                assert!(
                    p.load_instance_by_id(instance.id)
                        .unwrap()
                        .unwrap()
                        .required_inputs_satisfied
                );
            }
            let resource = state
                .resources
                .iter()
                .find(|r| r.declaration.id.as_str() == "settings")
                .unwrap();
            assert_eq!(
                fs::read(directory(&root, resource.allocation).join("settings.json")).unwrap(),
                b"target:old service state"
            );
            assert!(
                !state
                    .resources
                    .iter()
                    .any(|r| r.declaration.id.as_str() == "config")
            );
        } else {
            assert_eq!(state.preserved.len(), 1);
            assert_eq!(
                fs::read(directory(&root, state.preserved[0].allocation).join("settings.json"))
                    .unwrap(),
                b"target:old service state"
            );
        }
    }
}

// Test-ID: PR-TEST-0378
// Verifies: PR-REQ-0322, PR-REQ-0323, PR-REQ-0326, PR-REQ-0246, PR-REQ-0238
#[test]
fn transformation_owner_crashes_never_promote_a_proposal_or_rollback_service_bytes() {
    for (point, committed) in [
        ("after_target_proposal_receipt", false),
        ("before_migration_edge_commit", false),
        ("after_migration_edge_commit", true),
    ] {
        let (_temp, root) = root();
        let p = PactrunPersistence::open(&root).unwrap();
        let source = fixture(&p);
        let target = transform_target(&p, &source, "success");
        let instance = p
            .create_instance(
                InstanceName::parse("first").unwrap(),
                source.clone(),
                &mut [],
            )
            .unwrap();
        fs::write(
            directory(&root, allocation(&p, instance.id)).join("config.json"),
            b"source bytes",
        )
        .unwrap();
        drop(p);
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "persistence::sqlite_service_storage::tests::service_migration_worker",
                "--nocapture",
            ])
            .env("PACTRUN_SERVICE_MIGRATION_ROOT", &root)
            .env(
                "PACTRUN_SERVICE_MIGRATION_TARGET",
                format!("exact:{}/{}", target.package_id, target.content_digest),
            )
            .env("PACTRUN_M4_FAULT", point)
            .env("PACTRUN_MIGRATION_FAULT_EDGE", "0")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(87), "{point}");
        let app = crate::application::PactrunApplication::open(&root).unwrap();
        app.reconcile_lost_action_owners().unwrap();
        let p = PactrunPersistence::open_read_only(&root).unwrap();
        let state = p.load_instance_service_state(instance.id).unwrap().unwrap();
        assert_eq!(
            state.current_revision,
            if committed { target } else { source }
        );
        assert_eq!(count(&p, "run_service_edge_commits"), i64::from(committed));
        assert_eq!(count(&p, "service_storage_protections"), 2);
        assert_eq!(
            p.load_instance_recovery_guard(instance.id)
                .unwrap()
                .is_some(),
            !committed
        );
        let allocation = if committed {
            state
                .resources
                .iter()
                .find(|r| r.declaration.id.as_str() == "settings")
                .unwrap()
                .allocation
        } else {
            assert_eq!(state.preserved.len(), 1);
            state.preserved[0].allocation
        };
        assert_eq!(
            fs::read(directory(&root, allocation).join("settings.json")).unwrap(),
            b"target:source bytes"
        );
        assert_eq!(count(&p, "run_service_storage_targets"), 0);
        assert_eq!(count(&p, "run_service_resource_targets"), 0);
    }
}

// Test-ID: PR-TEST-0380
// Verifies: PR-REQ-0322, PR-REQ-0323, PR-REQ-0326, PR-REQ-0246
#[test]
fn committed_transform_output_and_service_boundary_survive_a_later_edge_crash() {
    use serde_json::json;
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let source = fixture(&p);
    let intermediate = transform_target(&p, &source, "outputs");
    let core=project_revision_core_source_v2(&serde_json::to_vec(&json!({"format_version":2,
        "inputs":[{"id":"final","required":true,"protection":"normal"}],"actions":[],
        "service_storages":[{"id":"target_store"}],"service_resources":[{"id":"settings","storage_id":"target_store","locator":"settings.json","kind":"file","read_exposure":"readable","user_mutation":{"kind":"direct"}}],
        "migrations":[{"source_revision_digest":intermediate.content_digest.to_string(),"transitions":[{"kind":"carry","source":{"role":"active","input_id":"result"},"target_input_id":"final"}],"requires_source":[],"requires_target":["final"],"produces_target":[],
        "storage_transitions":[{"kind":"reuse","source":{"role":"active","storage_id":"target_store"},"target_storage_id":"target_store"}],
        "resource_transitions":[{"kind":"reuse","source":{"role":"active","resource_id":"settings"},"target_resource_id":"settings"}]}]})).unwrap()).unwrap();
    let content = validate_revision_content_v2(
        core,
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![] })
            .unwrap(),
    )
    .unwrap();
    let target = p
        .persist_revision_internal(source.package_id, &content.into(), &[], None)
        .unwrap();
    let instance = p
        .create_instance(
            InstanceName::parse("first").unwrap(),
            source.clone(),
            &mut [],
        )
        .unwrap();
    fs::write(
        directory(&root, allocation(&p, instance.id)).join("config.json"),
        b"source bytes",
    )
    .unwrap();
    drop(p);
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "persistence::sqlite_service_storage::tests::service_migration_worker",
            "--nocapture",
        ])
        .env("PACTRUN_SERVICE_MIGRATION_ROOT", &root)
        .env(
            "PACTRUN_SERVICE_MIGRATION_TARGET",
            format!("exact:{}/{}", target.package_id, target.content_digest),
        )
        .env("PACTRUN_M4_FAULT", "before_migration_edge_commit")
        .env("PACTRUN_MIGRATION_FAULT_EDGE", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(87));
    let app = crate::application::PactrunApplication::open(&root).unwrap();
    app.reconcile_lost_action_owners().unwrap();
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    let state = p.load_instance_service_state(instance.id).unwrap().unwrap();
    assert_eq!(state.current_revision, intermediate);
    let mut output = Vec::new();
    p.export_input(
        instance.id,
        &InputIdentity::parse("result").unwrap(),
        false,
        &mut output,
    )
    .unwrap();
    assert_eq!(output, b"Pactrun-owned transform output");
    assert_eq!(count(&p, "run_service_edge_commits"), 1);
    assert_eq!(count(&p, "run_migration_boundaries"), 1);
    assert!(
        p.load_instance_recovery_guard(instance.id)
            .unwrap()
            .is_none()
    );
    let settings = state
        .resources
        .iter()
        .find(|r| r.declaration.id.as_str() == "settings")
        .unwrap();
    assert_eq!(
        fs::read(directory(&root, settings.allocation).join("settings.json")).unwrap(),
        b"target:source bytes"
    );
    assert!(
        !state
            .resources
            .iter()
            .any(|r| r.declaration.id.as_str() == "config")
    );
}

// Test-ID: PR-TEST-0381
// Verifies: PR-REQ-0322, PR-REQ-0326, PR-REQ-0246
#[test]
fn cancellation_after_receipt_timeout_and_cleanup_failure_cannot_publish_transform() {
    use crate::{
        application::PactrunApplication,
        hook::{ActionCancellation, HookRuntimePolicy},
    };
    for expected in [
        RunOutcome::Failed,
        RunOutcome::Cancelled,
        RunOutcome::TimedOut,
    ] {
        let cancelled = expected == RunOutcome::Cancelled;
        let (_temp, root) = root();
        let p = PactrunPersistence::open(&root).unwrap();
        let source = fixture(&p);
        let target = transform_target(
            &p,
            &source,
            if expected != RunOutcome::Failed {
                "linger_receipt"
            } else {
                "success"
            },
        );
        let app = PactrunApplication::open(&root).unwrap();
        let instance = app
            .create_instance(
                InstanceName::parse("first").unwrap(),
                source.clone(),
                vec![],
            )
            .unwrap();
        fs::write(
            directory(&root, allocation(&p, instance.id)).join("config.json"),
            b"source bytes",
        )
        .unwrap();
        let plan = crate::workflow::compile_migration(
            &app,
            &crate::workflow::PlatformHostLauncherLookup,
            &TransitionRevision {
                instance: instance.id,
                expected_state_version: instance.state_version,
                source: source.clone(),
                target: target.clone(),
                path: MigrationPathSelection::Exact(vec![source.clone(), target]),
                operator_inputs: vec![],
                authorize_declassification: false,
            },
            &[],
        )
        .unwrap();
        let cancellation = ActionCancellation::default();
        let cancel_thread = if cancelled {
            let root = root.clone();
            let cancellation = cancellation.clone();
            Some(std::thread::spawn(move || {
                let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
                loop {
                    let found = fs::read_dir(root.join("service-storage"))
                        .unwrap()
                        .filter_map(Result::ok)
                        .any(|entry| entry.path().join("receipt-observed").is_file());
                    if found {
                        cancellation.request();
                        return true;
                    }
                    if std::time::Instant::now() > until {
                        cancellation.request();
                        return false;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }))
        } else {
            if expected == RunOutcome::Failed {
                crate::managed_data::fail_next_execution_cleanup();
            }
            None
        };
        let run = app
            .accept_migration_inputs(
                plan,
                false,
                cancellation,
                vec![],
                HookRuntimePolicy::from_millis(
                    Some(10000),
                    Some(if expected == RunOutcome::TimedOut {
                        5000
                    } else {
                        25000
                    }),
                    Some(100),
                )
                .unwrap(),
            )
            .unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !app.advance_owner_continuation(run).unwrap() {
            assert!(std::time::Instant::now() < until);
        }
        if let Some(worker) = cancel_thread {
            assert!(
                worker.join().unwrap(),
                "cancellation must follow actual receipt"
            );
        }
        let RunState::Finished(outcome) = p.load_managed_run(run).unwrap().unwrap().state else {
            panic!("terminal Run")
        };
        assert_eq!(outcome.outcome, expected);
        assert_eq!(outcome.terminal_risk, RecoveryRiskState::Open);
        assert!(outcome.hook_completion.is_none());
        assert_eq!(count(&p, "run_service_edge_commits"), 0);
        let state = p.load_instance_service_state(instance.id).unwrap().unwrap();
        assert_eq!(state.current_revision, source);
        assert_eq!(state.preserved.len(), 1);
        if expected == RunOutcome::TimedOut {
            assert!(
                directory(&root, state.preserved[0].allocation)
                    .join("receipt-observed")
                    .is_file(),
                "deadline must expire after an actual proposal receipt"
            );
        }
        assert_eq!(
            fs::read(directory(&root, state.preserved[0].allocation).join("settings.json"))
                .unwrap(),
            b"target:source bytes"
        );
        assert!(
            p.load_instance_recovery_guard(instance.id)
                .unwrap()
                .is_some()
        );
    }
}

// Test-ID: PR-TEST-0382
// Verifies: PR-REQ-0319, PR-REQ-0322, PR-REQ-0323, PR-REQ-0326, PR-REQ-0327, PR-REQ-0246, PR-REQ-0238
#[test]
fn real_split_merge_chain_publishes_each_edge_and_preserves_a_failed_second_target() {
    use serde_json::json;
    for second_mode in ["success", "nonzero"] {
        let (_temp, root) = root();
        let p = PactrunPersistence::open(&root).unwrap();
        let source = fixture(&p);
        let template = transform_target(&p, &source, "success");
        let stored = p.load_revision(&template).unwrap().unwrap();
        let template_json: serde_json::Value = serde_json::from_slice(
            &crate::revision_core_v2::encode_canonical_revision_core_v2(
                stored.content.core.service_core().unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let make_target = |from: &RevisionIdentity,
                           sources: &[&str],
                           targets: &[&str],
                           storage: &str,
                           mode: &str| {
            let mut definition = template_json.clone();
            definition["service_storages"] = json!([{"id":storage}]);
            definition["service_resources"] =
                json!(targets.iter().map(|id| json!({
                "id":id,"storage_id":storage,"locator":format!("{id}.json"),"kind":"file",
                "read_exposure":"readable","user_mutation":{"kind":"direct"}
            })).collect::<Vec<_>>());
            let migration = &mut definition["migrations"][0];
            migration["source_revision_digest"] = json!(from.content_digest.to_string());
            migration["storage_transitions"] =
                json!([{"kind":"create","target_storage_id":storage}]);
            migration["resource_transitions"] = json!([{"kind":"transform",
                "sources":sources.iter().map(|id| json!({"role":"active","resource_id":id})).collect::<Vec<_>>(),
                "targets":targets}]);
            *migration["hook"]["args"]
                .as_array_mut()
                .unwrap()
                .last_mut()
                .unwrap() = json!(format!("transform-mode:{mode}"));
            migration["hook"]["service_access"] = json!(sources.iter().map(|id| json!({
                "reference":{"view":"source","role":"active","kind":"resource","id":id},"mode":"read"
            })).chain(targets.iter().map(|id| json!({
                "reference":{"view":"target","role":"active","kind":"resource","id":id},"mode":"write"
            }))).collect::<Vec<_>>());
            let core =
                project_revision_core_source_v2(&serde_json::to_vec(&definition).unwrap()).unwrap();
            let content =
                validate_revision_content_v2(core, stored.content.runtime_content.clone()).unwrap();
            let bytes = fs::read(std::env::current_exe().unwrap()).unwrap();
            let publication = p
                .put_runtime_content(
                    &stored.content.runtime_content.files()[0].blob_digest,
                    &mut Cursor::new(bytes),
                )
                .unwrap();
            p.persist_revision_internal(source.package_id, &content.into(), &[publication], None)
                .unwrap()
        };
        let split = make_target(
            &source,
            &["config"],
            &["left", "right"],
            "split_store",
            "success",
        );
        let merged = make_target(
            &split,
            &["left", "right"],
            &["merged"],
            "merged_store",
            second_mode,
        );
        let instance = p
            .create_instance(
                InstanceName::parse("first").unwrap(),
                source.clone(),
                &mut [],
            )
            .unwrap();
        let original = directory(&root, allocation(&p, instance.id)).join("config.json");
        fs::write(&original, b"source bytes").unwrap();
        let target = format!("exact:{}/{}", merged.package_id, merged.content_digest);
        let result = service_cli(&root, &["instance", "migrate", "first", "--to", &target]);
        let succeeded = second_mode == "success";
        assert_eq!(result.0, if succeeded { 0 } else { 1 }, "{}", result.2);
        let state = p.load_instance_service_state(instance.id).unwrap().unwrap();
        assert_eq!(
            state.current_revision,
            if succeeded { merged } else { split }
        );
        assert_eq!(
            count(&p, "run_service_edge_commits"),
            if succeeded { 2 } else { 1 }
        );
        assert_eq!(
            count(&p, "run_migration_boundaries"),
            if succeeded { 2 } else { 1 }
        );
        assert_eq!(count(&p, "run_service_storage_pins"), 0);
        assert_eq!(count(&p, "service_storage_allocations"), 3);
        assert_eq!(count(&p, "service_storage_protections"), 3);
        assert_eq!(fs::read(original).unwrap(), b"source bytes");
        assert!(
            !state
                .resources
                .iter()
                .any(|r| r.declaration.id.as_str() == "config")
        );
        assert!(state.resources.iter().any(|r| r.declaration.id.as_str() == "database" && r.role == ServiceRole::Retained));
        assert_eq!(
            p.load_instance_recovery_guard(instance.id)
                .unwrap()
                .is_some(),
            !succeeded
        );
        if succeeded {
            assert!(state.preserved.is_empty());
            assert!(
                !state
                    .resources
                    .iter()
                    .any(|r| matches!(r.declaration.id.as_str(), "left" | "right"))
            );
            let resource = state
                .resources
                .iter()
                .find(|r| r.declaration.id.as_str() == "merged")
                .unwrap();
            assert_eq!(
                fs::read(directory(&root, resource.allocation).join("merged.json")).unwrap(),
                b"target:target:source bytestarget:source bytes"
            );
        } else {
            assert!(
                !state
                    .resources
                    .iter()
                    .any(|r| r.declaration.id.as_str() == "merged")
            );
            for id in ["left", "right"] {
                let resource = state
                    .resources
                    .iter()
                    .find(|r| r.declaration.id.as_str() == id)
                    .unwrap();
                assert_eq!(resource.role, ServiceRole::Active);
                assert_eq!(
                    fs::read(directory(&root, resource.allocation).join(format!("{id}.json")))
                        .unwrap(),
                    b"target:source bytes"
                );
            }
            assert_eq!(state.preserved.len(), 1);
            assert_eq!(
                fs::read(directory(&root, state.preserved[0].allocation).join("merged.json"))
                    .unwrap(),
                b"target:target:source bytestarget:source bytes"
            );
        }
    }
}

// Test-ID: PR-TEST-0388
// Verifies: PR-REQ-0166, PR-REQ-0242, PR-REQ-0245, PR-REQ-0319, PR-REQ-0326
#[test]
fn declarative_resource_creation_checks_explicit_presence_without_adopting_or_copying_bytes() {
    use serde_json::json;
    for predicate in ["present", "absent", "any"] {
        for present in [false, true] {
            let (_temp, root) = root();
            let p = PactrunPersistence::open(&root).unwrap();
            let source = fixture(&p);
            let core = project_revision_core_source_v2(&serde_json::to_vec(&json!({
                "format_version":2,"inputs":[],"actions":[],"service_storages":[{"id":"state"}],
                "service_resources":[{"id":"imported","storage_id":"state","locator":"unmapped.bin","kind":"file","read_exposure":"readable","user_mutation":{"kind":"direct"}}],
                "migrations":[{"source_revision_digest":source.content_digest.to_string(),"transitions":[],"requires_source":[],"requires_target":[],"produces_target":[],
                    "storage_transitions":[{"kind":"reuse","source":{"role":"active","storage_id":"state"},"target_storage_id":"state"}],
                    "resource_transitions":[{"kind":"create","target_resource_id":"imported","presence":predicate}]}]
            })).unwrap()).unwrap();
            let content = validate_revision_content_v2(
                core,
                project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 {
                    files: vec![],
                })
                .unwrap(),
            )
            .unwrap();
            let target = p
                .persist_revision_internal(source.package_id, &content.into(), &[], None)
                .unwrap();
            let instance = p
                .create_instance(
                    InstanceName::parse("first").unwrap(),
                    source.clone(),
                    &mut [],
                )
                .unwrap();
            let allocated = allocation(&p, instance.id);
            let live = directory(&root, allocated).join("unmapped.bin");
            if present {
                fs::write(&live, b"already service-owned").unwrap();
            }
            let reference = format!("exact:{}/{}", target.package_id, target.content_digest);
            let planned = service_cli(
                &root,
                &["instance", "migrate", "first", "--to", &reference, "--plan"],
            );
            assert_eq!(planned.0, 0, "{}", planned.2);
            assert_eq!(count(&p, "runs"), 0);
            assert_eq!(count(&p, "service_storage_allocations"), 1);
            let result = service_cli(&root, &["instance", "migrate", "first", "--to", &reference]);
            let success = predicate == "any" || (predicate == "present") == present;
            assert_eq!(
                result.0,
                if success { 0 } else { 1 },
                "{predicate}/{present}: {}",
                result.2
            );
            let state = p.load_instance_service_state(instance.id).unwrap().unwrap();
            assert_eq!(
                state.current_revision,
                if success { target } else { source }
            );
            assert_eq!(
                state
                    .resources
                    .iter()
                    .any(|r| r.declaration.id.as_str() == "imported"
                        && r.role == ServiceRole::Active),
                success
            );
            assert!(state.resources.iter().all(|r| r.allocation == allocated));
            assert_eq!(live.exists(), present);
            if present {
                assert_eq!(fs::read(live).unwrap(), b"already service-owned");
            }
            assert_eq!(count(&p, "service_storage_allocations"), 1);
            assert_eq!(count(&p, "run_service_storage_pins"), 0);
            assert_eq!(count(&p, "managed_input_payloads"), 0);
            assert!(
                p.load_instance_recovery_guard(instance.id)
                    .unwrap()
                    .is_none()
            );
        }
    }
}

// Test-ID: PR-TEST-0386
// Verifies: PR-REQ-0317, PR-REQ-0319, PR-REQ-0321, PR-REQ-0322, PR-REQ-0326, PR-REQ-0238, PR-REQ-0244
#[test]
fn real_file_to_directory_transform_uses_explicit_nested_grants_without_implicit_service_parents() {
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let source = fixture(&p);
    let target = transform_target(&p, &source, "nested");
    let instance = p
        .create_instance(InstanceName::parse("first").unwrap(), source, &mut [])
        .unwrap();
    let original = directory(&root, allocation(&p, instance.id)).join("config.json");
    fs::write(&original, b"source bytes").unwrap();
    let target_reference = format!("exact:{}/{}", target.package_id, target.content_digest);
    let result = service_cli(
        &root,
        &["instance", "migrate", "first", "--to", &target_reference],
    );
    assert_eq!(result.0, 0, "{}", result.2);
    let state = p.load_instance_service_state(instance.id).unwrap().unwrap();
    assert_eq!(state.current_revision, target);
    let bundle = state
        .resources
        .iter()
        .find(|r| r.declaration.id.as_str() == "bundle")
        .unwrap();
    let index = state
        .resources
        .iter()
        .find(|r| r.declaration.id.as_str() == "index")
        .unwrap();
    assert_eq!(bundle.allocation, index.allocation);
    assert_eq!(bundle.declaration.kind, ServiceResourceKind::Directory);
    assert_eq!(index.declaration.kind, ServiceResourceKind::File);
    assert_eq!(
        fs::read(directory(&root, index.allocation).join("bundle/index.json")).unwrap(),
        b"target:source bytes"
    );
    assert_eq!(fs::read(original).unwrap(), b"source bytes");
    assert!(
        !state
            .resources
            .iter()
            .any(|r| r.declaration.id.as_str() == "config")
    );
    assert_eq!(count(&p, "run_service_edge_commits"), 1);
    assert_eq!(count(&p, "run_service_storage_pins"), 0);
    assert!(
        p.load_instance_recovery_guard(instance.id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        service_cli(&root, &["resource", "observe", "first", "bundle"]).1,
        "Present kind=directory kind_matches=true\n"
    );
}

// Test-ID: PR-TEST-0384
// Verifies: PR-REQ-0317, PR-REQ-0320
#[test]
fn public_source_version_dispatch_rejects_invalid_v2_before_durable_blob_or_revision_publication() {
    let (_temp, root) = root();
    let source = root.parent().unwrap().join("invalid-pack");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("payload.bin"), b"must never be published").unwrap();
    let p = PactrunPersistence::open(&root).unwrap();
    let blob_entries = || {
        let mut names = fs::read_dir(root.join("runtime-content"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        names.sort();
        names
    };
    let before = blob_entries();
    let valid = "source_format: 2\npackage_id: 00000000000000000000000000000065\nrevision:\n  service_storages: [{id: state}]\nruntime_content:\n  files: [{id: payload, source: payload.bin, path: payload.bin}]\n";
    for invalid in [
        valid.replace("source_format: 2", "source_format: 1"),
        valid.replace("source_format: 2", "source_format: 2.0"),
        valid.replace("source_format: 2", "source_format: '2'"),
        // V3 is explicitly supported; V4 must still fail before publication.
        valid.replace("source_format: 2", "source_format: 4"),
        valid.replace("source_format: 2", "source_format: 2\nsource_format: 1"),
        valid.replace(
            "  service_storages:",
            "  unknown_field: true\n  service_storages:",
        ),
        valid.replace("[{id: state}]", "[{id: state}, {id: state}]"),
    ] {
        fs::write(source.join("pactrun.yaml"), invalid).unwrap();
        let result = service_cli(&root, &["pack", "install", source.to_str().unwrap()]);
        assert_eq!(result.0, 1, "{}", result.2);
        assert!(result.1.is_empty());
        assert_eq!(count(&p, "revisions"), 0);
        assert_eq!(count(&p, "service_storage_allocations"), 0);
        assert_eq!(count(&p, "runs"), 0);
        assert_eq!(blob_entries(), before);
        assert!(!root.join("service-storage").exists());
    }
}

// Test-ID: PR-TEST-0383
// Verifies: PR-REQ-0318, PR-REQ-0319, PR-REQ-0320, PR-REQ-0323, PR-REQ-0326, PR-REQ-0327, PR-REQ-0328
#[test]
fn public_v2_installation_and_v1_roundtrip_preserve_identity_live_storage_and_explicit_reattachment()
 {
    let (_temp, root) = root();
    let source = root.parent().unwrap().join("pack-source");
    fs::create_dir(&source).unwrap();
    let install = |version: u8, revision: &str| {
        let yaml = format!(
            "source_format: {version}\npackage_id: 00000000000000000000000000000065\nrevision:\n{revision}\nruntime_content: {{}}\n"
        );
        fs::write(source.join("pactrun.yaml"), yaml).unwrap();
        let result = service_cli(&root, &["pack", "install", source.to_str().unwrap()]);
        assert_eq!(result.0, 0, "{}", result.2);
        result.1.lines().next().unwrap().to_owned()
    };
    let legacy = install(1, "  inputs: []");
    let result = service_cli(
        &root,
        &["instance", "create", "first", "--revision", &legacy],
    );
    assert_eq!(result.0, 0, "{}", result.2);
    assert!(!root.join("service-storage").exists());
    let declarations = "  service_storages: [{id: state}]\n  service_resources:\n    - {id: config, storage_id: state, locator: config.json, kind: file, read_exposure: readable, user_mutation: {kind: direct}}";
    let input_mapping = "      transitions: []\n      requires_source: []\n      requires_target: []\n      produces_target: []";
    let v2_revision = format!(
        "{declarations}\n  migrations:\n    - source_revision_digest: {}\n{input_mapping}\n      storage_transitions: [{{kind: create, target_storage_id: state}}]\n      resource_transitions: [{{kind: create, target_resource_id: config, presence: absent}}]",
        legacy.split_once('/').unwrap().1
    );
    let version_two = install(2, &v2_revision);
    assert_ne!(legacy, version_two);
    assert!(
        !root.join("service-storage").exists(),
        "installation must not allocate"
    );
    let migrate = |target: &str| {
        let result = service_cli(&root, &["instance", "migrate", "first", "--to", target]);
        assert_eq!(result.0, 0, "{}", result.2);
    };
    migrate(&version_two);
    let locate = |retained: bool, intent: &str| {
        let mut args = vec!["resource", "locate", "first", "config", "--intent", intent];
        if retained {
            args.push("--retained");
        }
        service_cli(&root, &args)
    };
    assert_eq!(
        service_cli(&root, &["resource", "observe", "first", "config"]).1,
        "Absent\n"
    );
    let path_result = locate(false, "write");
    assert_eq!(path_result.0, 0, "{}", path_result.2);
    // CLI output is human-escaped, not a machine-readable native path. Locate
    // authorizes disclosure; resolve the fixture's physical path independently.
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    let instance = p
        .resolve_instance_name(&InstanceName::parse("first").unwrap())
        .unwrap()
        .unwrap();
    let allocated = allocation(&p, instance);
    assert!(path_result.1.contains(&format!("alloc-{allocated}")));
    let live = directory(&root, allocated).join("config.json");
    drop(p);
    fs::write(&live, b"service-owned bytes").unwrap();
    assert_eq!(
        install(2, &v2_revision),
        version_two,
        "live bytes cannot affect Revision identity"
    );
    let v1_target = install(
        1,
        &format!(
            "  migrations:\n    - source_revision_digest: {}\n{input_mapping}",
            version_two.split_once('/').unwrap().1
        ),
    );
    migrate(&v1_target);
    assert_eq!(service_cli(&root, &["resource", "list", "first"]).1, "");
    assert_eq!(
        locate(true, "read"),
        (0, path_result.1.clone(), String::new())
    );
    assert_eq!(locate(true, "write").0, 1);
    let attached = install(
        2,
        &format!(
            "{declarations}\n  migrations:\n    - source_revision_digest: {}\n{input_mapping}\n      storage_transitions: [{{kind: reattach, source: {{role: retained, storage_id: state}}, target_storage_id: state}}]\n      resource_transitions: [{{kind: reattach, source: {{role: retained, resource_id: config}}, target_resource_id: config}}]",
            v1_target.split_once('/').unwrap().1
        ),
    );
    migrate(&attached);
    assert_eq!(locate(false, "write"), path_result);
    assert_eq!(fs::read(live).unwrap(), b"service-owned bytes");
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    let instance = p
        .resolve_instance_name(&InstanceName::parse("first").unwrap())
        .unwrap()
        .unwrap();
    let state = p.load_instance_service_state(instance).unwrap().unwrap();
    assert!(
        state
            .resources
            .iter()
            .all(|r| r.role == ServiceRole::Active)
    );
    assert_eq!(count(&p, "service_storage_allocations"), 1);
    assert_eq!(count(&p, "managed_input_payloads"), 0);
    assert_eq!(count(&p, "run_service_storage_pins"), 0);
}
