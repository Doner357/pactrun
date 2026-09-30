use super::*;

// Test-ID: PR-TEST-0475
// Verifies: PR-REQ-0343
#[test]
#[cfg(target_os = "linux")]
fn collection_reports_partial_io_failure_without_losing_completed_counts() {
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (rev, _) = revision(&p, 33, b"collectible");
    p.delete_object(&ObjectDeletion::Revision(rev)).unwrap();
    drop(p);
    let foreign = root.join("runtime-content/.gc-00000000000000000000000000000000");
    fs::write(&foreign, b"not a directory; retain these bytes").unwrap();
    let gc = PactrunPersistence::open_for_collection(&root).unwrap();
    let result = gc.collect_content(true).unwrap();
    assert_eq!((result.removed, result.failed), (1, 1));
    drop(gc);
    assert_eq!(
        fs::read(&foreign).unwrap(),
        b"not a directory; retain these bytes"
    );
    assert_eq!(cli(&root, &["storage", "gc"]).0, 1);
}

// Test-ID: PR-TEST-0474
// Verifies: PR-REQ-0076, PR-REQ-0343, PR-REQ-0344
#[test]
fn collection_never_bootstraps_a_missing_reference_catalog() {
    for empty_database in [false, true] {
        let (_temp, root) = root();
        let digest = Sha256Digest::from_bytes(Sha256::digest(b"unclassified").into());
        let blob = root
            .join("runtime-content")
            .join(digest.as_str().strip_prefix("sha256:").unwrap());
        fs::write(&blob, b"unclassified").unwrap();
        let database = root.join("database/pactrun.sqlite3");
        if empty_database {
            fs::write(&database, b"").unwrap();
        }
        for args in [vec!["storage", "gc"], vec!["storage", "gc", "--plan"]] {
            assert_eq!(cli(&root, &args).0, 1);
            assert_eq!(fs::read(&blob).unwrap(), b"unclassified");
            assert!(!root.join("runtime-content/.collection.lock").exists());
            assert_eq!(database.exists(), empty_database);
            if empty_database {
                assert_eq!(fs::metadata(&database).unwrap().len(), 0);
            }
        }
    }
}

// Test-ID: PR-TEST-0473
// Verifies: PR-REQ-0345
#[test]
fn reopening_preserves_populated_objects_and_service_bytes_exactly() {
    fn rows(db: &Connection) -> std::collections::BTreeMap<String, Vec<String>> {
        let names=db.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name<>'writable_admissions' ORDER BY name").unwrap()
            .query_map([],|r|r.get::<_,String>(0)).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
        let mut result = std::collections::BTreeMap::new();
        for name in names {
            let projection = "*";
            let mut query = db
                .prepare(&format!("SELECT {projection} FROM {name}"))
                .unwrap();
            let columns = query.column_count();
            let mut data = query
                .query_map([], |r| {
                    let values = (0..columns)
                        .map(|i| r.get::<_, rusqlite::types::Value>(i))
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(format!("{values:?}"))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            data.sort();
            result.insert(name, data);
        }
        result
    }
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (rev, digest) = revision(&p, 31, b"preserved-runtime");
    let i = instance(&p, &rev);
    let r = run(&p, &i);
    finish(&p, r, RunOutcome::Succeeded);
    let s = snapshot(&p, &rev, i.id);
    p.apply_revision_metadata_batch(
        &rev,
        &RevisionMetadataMutationBatch::new([RevisionMetadataMutation::CompareAndSetLocalAlias {
            alias: LocalAlias::parse("retained").unwrap(),
            expected: CurrentState::Absent,
            desired: CurrentState::Present(rev.clone()),
        }])
        .unwrap(),
    )
    .unwrap();
    let core=crate::revision_canonical::project_service_revision_source(br#"{"format_version":"1.0-alpha.1","inputs":[],"actions":[],"migrations":[],"service_storages":[{"id":"data"}],"service_resources":[]}"#).unwrap();
    let runtime =
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![] })
            .unwrap();
    let content =
        crate::revision_canonical::validate_service_revision_content(core, runtime).unwrap();
    let service_rev = p
        .persist_versioned_revision_with_metadata(
            PackageId::from_bytes([32; 16]),
            &content.into(),
            &[],
            &RevisionMetadataMutationBatch::new([]).unwrap(),
        )
        .unwrap();
    let service = p
        .create_instance(
            InstanceName::parse("live-service").unwrap(),
            service_rev,
            &mut [],
        )
        .unwrap();
    let allocation = p
        .load_instance_service_state(service.id)
        .unwrap()
        .unwrap()
        .storages[0]
        .allocation;
    let live = root
        .join("service-storage")
        .join(format!("alloc-{allocation}"))
        .join("live");
    fs::write(&live, b"service-owned-sentinel").unwrap();
    drop(p);
    let db = Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
    let before = rows(&db);
    drop(PactrunPersistence::open_read_only(&root).unwrap());
    assert_eq!(rows(&db), before);
    assert_eq!(fs::read(&live).unwrap(), b"service-owned-sentinel");
    assert_eq!(
        fs::read(
            root.join("runtime-content")
                .join(digest.as_str().strip_prefix("sha256:").unwrap())
        )
        .unwrap(),
        b"preserved-runtime"
    );
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    p.verify_snapshot(s).unwrap();
    assert!(p.load_managed_run(r).unwrap().is_some());
    let mut bytes = vec![];
    p.open_run_artifact(
        r,
        &ManagedOutputIdentity::parse("report").unwrap(),
        &mut bytes,
    )
    .unwrap();
    assert_eq!(bytes, b"payload");
}

// Test-ID: PR-TEST-0470
// Verifies: PR-REQ-0075, PR-REQ-0341
#[test]
fn revision_guards_include_pins_and_retained_service_declarations() {
    let (_temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (old, _) = revision(&p, 28, b"old");
    let (new, _) = revision(&p, 28, b"new");
    let i = instance(&p, &old);
    let id = run(&p, &i);
    // Isolate the durable pin guard from the active-Instance guard with an exact
    // stored transition boundary; foreign keys remain enabled.
    p.database.lock().unwrap().execute("UPDATE instances SET active_revision_content_digest=?1,instance_state_version=?2 WHERE instance_id=?3",params![new.content_digest.as_bytes().as_slice(),InstanceStateVersion::generate().unwrap().as_bytes().as_slice(),i.id.as_bytes().as_slice()]).unwrap();
    assert_eq!(
        p.delete_object(&ObjectDeletion::Revision(old.clone()))
            .unwrap(),
        ObjectDeletionResult::Blocked(DeletionBlock::RevisionPin)
    );
    finish(&p, id, RunOutcome::Succeeded);
    assert_eq!(
        p.delete_object(&ObjectDeletion::Revision(old)).unwrap(),
        ObjectDeletionResult::Deleted
    );
    let core=crate::revision_canonical::project_service_revision_source(br#"{"format_version":"1.0-alpha.1","inputs":[],"actions":[],"migrations":[],"service_storages":[{"id":"data"}],"service_resources":[]}"#).unwrap();
    let runtime =
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![] })
            .unwrap();
    let content =
        crate::revision_canonical::validate_service_revision_content(core, runtime).unwrap();
    let rev = p
        .persist_versioned_revision_with_metadata(
            PackageId::from_bytes([29; 16]),
            &content.into(),
            &[],
            &RevisionMetadataMutationBatch::new([]).unwrap(),
        )
        .unwrap();
    let view = p
        .create_instance(
            InstanceName::parse("service").unwrap(),
            rev.clone(),
            &mut [],
        )
        .unwrap();
    let (target, _) = revision(&p, 29, b"target");
    p.database.lock().unwrap().execute("UPDATE instances SET active_revision_content_digest=?1,instance_state_version=?2 WHERE instance_id=?3",params![target.content_digest.as_bytes().as_slice(),InstanceStateVersion::generate().unwrap().as_bytes().as_slice(),view.id.as_bytes().as_slice()]).unwrap();
    assert_eq!(
        p.delete_object(&ObjectDeletion::Revision(rev)).unwrap(),
        ObjectDeletionResult::Blocked(DeletionBlock::ServiceReference)
    );
}

// Test-ID: PR-TEST-0471
// Verifies: PR-REQ-0076, PR-REQ-0343
#[test]
#[cfg(target_os = "linux")]
fn collection_refuses_links_and_fifos_without_blocking_or_following_them() {
    use std::os::unix::fs::symlink;
    let (temp, root) = root();
    let p = PactrunPersistence::open(&root).unwrap();
    let (rev, digest) = revision(&p, 30, b"data");
    let path = root
        .join("runtime-content")
        .join(digest.as_str().strip_prefix("sha256:").unwrap());
    p.delete_object(&ObjectDeletion::Revision(rev)).unwrap();
    drop(p);
    fs::remove_file(&path).unwrap();
    let outside = temp.path().join("outside");
    fs::write(&outside, b"data").unwrap();
    symlink(&outside, &path).unwrap();
    let gc = PactrunPersistence::open_for_collection(&root).unwrap();
    assert_eq!(gc.collect_content(true).unwrap().failed, 1);
    drop(gc);
    assert_eq!(fs::read(&outside).unwrap(), b"data");
    fs::remove_file(&path).unwrap();
    let parent = fs::File::open(root.join("runtime-content")).unwrap();
    rustix::fs::mkfifoat(
        &parent,
        path.file_name().unwrap(),
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    let gc = PactrunPersistence::open_for_collection(&root).unwrap();
    assert_eq!(gc.collect_content(true).unwrap().failed, 1);
}
