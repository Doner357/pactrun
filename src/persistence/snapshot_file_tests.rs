// Test-ID: PR-TEST-0483
// Verifies: PR-REQ-0347
#[test]
fn explicit_upgrade_preserves_real_inline_snapshots_and_refuses_implicit_upgrade() {
    let spec = include_str!("../../docs/spec/persistence/persistence-schema-v10.md");
    let ddl = spec
        .split_once("```sql\n")
        .unwrap()
        .1
        .split_once("\n```")
        .unwrap()
        .0;
    assert_eq!(
        ddl.trim(),
        include_str!("persistence_schema_v10_additions.sql").trim()
    );
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let mut bundle = prepared(&p, 2);
    let id = bundle.manifest().manifest().snapshot_id();
    let canonical = bundle.manifest().canonical_bytes().to_vec();
    p.import_snapshot_bundle(&mut bundle).unwrap();
    drop(bundle);
    drop(p);
    let mut db = Connection::open(path.join("database/pactrun.sqlite3")).unwrap();
    crate::persistence::sqlite_v9::current_to_v9_fixture(&mut db);
    assert!(matches!(
        PactrunPersistence::open_read_only(&path),
        Err(PersistenceError::UpgradeRequired)
    ));
    assert!(matches!(
        PactrunPersistence::open(&path),
        Err(PersistenceError::UpgradeRequired)
    ));
    let chunks: i64 = db
        .query_row("SELECT count(*) FROM snapshot_blob_chunks", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(chunks > 0);
    assert!(PactrunPersistence::upgrade_storage(&path).unwrap());
    assert!(!PactrunPersistence::upgrade_storage(&path).unwrap());
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        crate::persistence::SCHEMA_VERSION
    );
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT count(*) FROM snapshot_blob_chunks", [], |r| r
            .get(0))
            .unwrap(),
        chunks
    );
    assert_eq!(
        db.query_row::<i64, _, _>(
            "SELECT count(*) FROM snapshot_blobs WHERE storage_kind<>0",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
    let p = PactrunPersistence::open(&path).unwrap();
    p.verify_snapshot(id).unwrap();
    let exported =
        ValidatedSnapshotBundle::read(p.export_snapshot_stage(id, true).unwrap()).unwrap();
    assert_eq!(exported.manifest().canonical_bytes(), canonical);
}

// Supporting representation/GC corruption coverage for PR-TEST-0483.
#[test]
fn immutable_snapshot_reference_corruption_stops_collection_before_removal() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let mut bundle = prepared(&p, 2);
    p.import_snapshot_bundle(&mut bundle).unwrap();
    drop(bundle);
    let orphan_bytes = b"candidate with no reference";
    let orphan = Sha256Digest::from_bytes(Sha256::digest(orphan_bytes).into());
    p.put_runtime_content(&orphan, &mut std::io::Cursor::new(orphan_bytes))
        .unwrap();
    p.database
        .lock()
        .unwrap()
        .execute("UPDATE snapshot_blobs SET storage_kind=0", [])
        .unwrap();
    drop(p);
    let collector = PactrunPersistence::open_for_collection(&path).unwrap();
    assert!(collector.collect_content(true).is_err());
    assert!(
        path.join("runtime-content")
            .join(hex::encode(orphan.to_bytes()))
            .exists()
    );
}
