// Test-ID: PR-TEST-0483
// Verifies: PR-REQ-0347
#[test]
fn reopening_preserves_real_snapshot_identity_and_immutable_bytes() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let mut bundle = prepared(&p, 2);
    let id = bundle.manifest().manifest().snapshot_id();
    let canonical = bundle.manifest().canonical_bytes().to_vec();
    p.import_snapshot_bundle(&mut bundle).unwrap();
    drop(bundle);
    drop(p);
    let before = std::fs::read(path.join("database/pactrun.sqlite3")).unwrap();
    let reader = PactrunPersistence::open_read_only(&path).unwrap();
    reader.verify_snapshot(id).unwrap();
    drop(reader);
    assert_eq!(std::fs::read(path.join("database/pactrun.sqlite3")).unwrap(), before);
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
        .execute("UPDATE snapshot_blobs SET blob_digest=zeroblob(32) WHERE (snapshot_id,blob_digest)=(SELECT snapshot_id,blob_digest FROM snapshot_blobs ORDER BY snapshot_id,blob_digest LIMIT 1)", [])
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
