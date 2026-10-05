use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tempfile::TempDir;

mod file_backed {
    use super::*;
    include!("snapshot_file_tests.rs");
}

fn root() -> (TempDir, PathBuf) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/snapshot-store-tests");
    fs::create_dir_all(&parent).unwrap();
    let tmp = tempfile::Builder::new()
        .prefix("snapshots-")
        .tempdir_in(parent)
        .unwrap();
    for name in ["database", "runtime-content", "staging"] {
        fs::create_dir(tmp.path().join(name)).unwrap();
    }
    let path = tmp.path().to_owned();
    (tmp, path)
}
fn fixture(version: u32) -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(if version == 1 {
        "tests/vectors/snapshot_integrity/content.json"
    } else {
        "tests/vectors/snapshot_integrity/protection.json"
    });
    let corpus: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    corpus["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| {
            f["name"]
                == if version == 1 {
                    "complete_binding_state"
                } else {
                    "active_normal_sticky_secret"
                }
        })
        .unwrap()
        .clone()
}
fn prepared(p: &PactrunPersistence, version: u32) -> ValidatedSnapshotBundle {
    prepared_fixture(p, &fixture(version)).0
}
fn prepared_fixture(
    p: &PactrunPersistence,
    f: &serde_json::Value,
) -> (ValidatedSnapshotBundle, fs::File) {
    let version = SnapshotIntegrityVersion::from_text(
        f["normalized_manifest"]["format_version"].as_str().unwrap(),
    )
    .unwrap();
    let manifest = decode_snapshot_manifest(
        version,
        f["raw_manifest"].as_str().unwrap().as_bytes(),
        None,
    )
    .unwrap();
    let mut stage = p
        .staging_session()
        .unwrap()
        .create_snapshot_stage()
        .unwrap();
    {
        let mut writer = snapshot_bundle::start_bundle(&mut stage, &manifest).unwrap();
        for (digest, hex) in f["blob_contents"].as_object().unwrap() {
            let digest = Sha256Digest::parse(digest).unwrap();
            let bytes = hex::decode(hex.as_str().unwrap()).unwrap();
            writer.start_blob(&digest, bytes.len() as u64).unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    stage.finish_operation_file().unwrap();
    let alias = stage.try_clone_reader().unwrap();
    (ValidatedSnapshotBundle::read(stage).unwrap(), alias)
}

// Test-ID: PR-TEST-0208
// Verifies: PR-REQ-0035, PR-REQ-0085, PR-REQ-0292, PR-REQ-0297, PR-REQ-0298
#[test]
fn snapshot_storage_round_trips_both_versions_without_producer_or_origin_ownership() {
    for version in [1, 2] {
        let (_tmp, path) = root();
        let p = PactrunPersistence::open(&path).unwrap();
        let mut bundle = prepared(&p, version);
        let id = bundle.manifest().manifest().snapshot_id();
        let canonical = bundle.manifest().canonical_bytes().to_vec();
        let receipt = p.import_snapshot_bundle(&mut bundle).unwrap();
        assert!(receipt.inserted);
        assert_eq!(receipt.id, id);
        assert_eq!(
            receipt.relational,
            SnapshotRelationalVerification::NotEvaluated
        );
        assert!(!p.import_snapshot_bundle(&mut bundle).unwrap().inserted);
        let inspection = p.inspect_snapshot(id).unwrap();
        assert_eq!(inspection.version.as_str(), "1.0-alpha.1");
        assert_eq!(inspection.id, id);
        let verified = p.verify_snapshot(id).unwrap();
        assert_eq!(
            verified.relational,
            SnapshotRelationalVerification::NotEvaluated
        );
        let mut exported =
            ValidatedSnapshotBundle::read(p.export_snapshot_stage(id, true).unwrap()).unwrap();
        assert_eq!(exported.manifest().canonical_bytes(), canonical);
        let (_other_tmp, other) = root();
        let q = PactrunPersistence::open(&other).unwrap();
        q.import_snapshot_bundle(&mut exported).unwrap();
        q.verify_snapshot(id).unwrap();
        assert_eq!(q.list_snapshots(None).unwrap().len(), 1);
        let db = q.database.lock().unwrap();
        for table in ["runs", "instances", "revisions"] {
            assert_eq!(
                db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
}

// Test-ID: PR-TEST-0209
// Verifies: PR-REQ-0085, PR-REQ-0292, PR-REQ-0298, PR-REQ-0302
#[test]
fn corrupted_stored_bytes_are_not_repaired_by_idempotent_import_or_export() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let mut bundle = prepared(&p, 2);
    let id = bundle.manifest().manifest().snapshot_id();
    p.import_snapshot_bundle(&mut bundle).unwrap();
    super::corrupt_snapshot_for_test(&path, id);
    p.inspect_snapshot(id).unwrap(); // Structural inspection does not claim payload verification.
    assert!(matches!(
        p.verify_snapshot(id),
        Err(PersistenceError::CorruptSnapshot(_))
    ));
    assert!(p.import_snapshot_bundle(&mut bundle).is_err());
    assert!(p.export_snapshot_stage(id, true).is_err());
    assert!(matches!(
        p.export_snapshot_stage(SnapshotId::from_bytes([9; 16]), false),
        Err(PersistenceError::UnauthorizedSnapshotExport)
    ));
}

// Test-ID: PR-TEST-0210
// Verifies: PR-REQ-0035, PR-REQ-0292, PR-REQ-0298
#[test]
fn import_rolls_back_all_rows_on_failure_and_never_creates_a_run() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let (mut bundle, alias) = prepared_fixture(&p, &fixture(2));
    // Fault after initial validation: publication must re-read bytes and roll
    // back even the manifest/header rows it already inserted in its transaction.
    alias.set_len(0).unwrap();
    assert!(p.import_snapshot_bundle(&mut bundle).is_err());
    let db = p.database.lock().unwrap();
    for table in ["snapshots", "snapshot_blobs", "runs"] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

// Test-ID: PR-TEST-0211
// Verifies: PR-REQ-0292, PR-REQ-0302
#[test]
fn application_export_requires_one_shot_authorization_and_atomic_no_clobber() {
    let (tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let mut bundle = prepared(&p, 1);
    let id = bundle.manifest().manifest().snapshot_id();
    p.import_snapshot_bundle(&mut bundle).unwrap();
    drop(p);
    let app = crate::application::PactrunApplication::open(&path).unwrap();
    let output = tmp.path().join("export.zip");
    let mut warning = Vec::new();
    assert!(
        app.export_snapshot_file(id, &output, false, &mut warning)
            .is_err()
    );
    assert!(!output.exists());
    assert!(warning.is_empty());
    app.export_snapshot_file(id, &output, true, &mut warning)
        .unwrap();
    assert!(String::from_utf8(warning).unwrap().contains("unencrypted"));
    let before = fs::read(&output).unwrap();
    assert!(
        app.export_snapshot_file(id, &output, true, &mut Vec::new())
            .is_err()
    );
    assert_eq!(fs::read(&output).unwrap(), before);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&output).unwrap().permissions().mode() & 0o077,
            0
        );
    }
    let (_tmp2, path2) = root();
    let other = crate::application::PactrunApplication::open(&path2).unwrap();
    other.import_snapshot_file(&output).unwrap();
    other.verify_snapshot(id).unwrap();
    let ro = crate::application::PactrunApplication::open_read_only(&path2).unwrap();
    assert_eq!(
        ro.inspect_snapshot(id).unwrap().version,
        SnapshotIntegrityVersion::BASELINE
    );
    let mut damaged = before;
    let n = damaged.len();
    damaged[n - 1] ^= 1;
    fs::write(&output, damaged).unwrap();
    assert!(other.import_snapshot_file(&output).is_err()); // Existing ID cannot bypass input validation.
}

// Test-ID: PR-TEST-0213
// Verifies: PR-REQ-0085, PR-REQ-0148, PR-REQ-0292, PR-REQ-0298
#[test]
fn same_id_different_digest_is_a_collision_without_replacing_the_original() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let mut first = prepared(&p, 2);
    let id = first.manifest().manifest().snapshot_id();
    p.import_snapshot_bundle(&mut first).unwrap();
    let mut f = fixture(2);
    let mut raw: serde_json::Value =
        serde_json::from_str(f["raw_manifest"].as_str().unwrap()).unwrap();
    raw["captured_at"]["unix_seconds"] = serde_json::json!(42);
    f["raw_manifest"] = serde_json::json!(serde_json::to_string(&raw).unwrap());
    let (mut second, _) = prepared_fixture(&p, &f);
    assert!(
        matches!(p.import_snapshot_bundle(&mut second),Err(PersistenceError::SnapshotCollision(found)) if found==id)
    );
    let export = ValidatedSnapshotBundle::read(p.export_snapshot_stage(id, true).unwrap()).unwrap();
    assert_eq!(
        export.manifest().integrity_digest(),
        first.manifest().integrity_digest()
    );
}

// Test-ID: PR-TEST-0214
// Verifies: PR-REQ-0292, PR-REQ-0297, PR-REQ-0302
#[test]
fn producer_installation_validates_baseline_protection_without_rewriting_snapshot_bytes() {
    for invalid_protection in [false, true] {
        let (_tmp, path) = root();
        let p = PactrunPersistence::open(&path).unwrap();
        let mut f = fixture(2);
        let inputs = f["producer_context"]["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| InputDeclarationV1 {
                id: InputIdentity::parse(i["id"].as_str().unwrap()).unwrap(),
                required: false,
                protection: if i["protection"] == "secret" {
                    InputProtectionV1::Secret
                } else {
                    InputProtectionV1::Normal
                },
            })
            .collect();
        let core = project_revision_declarations(RevisionDeclarationInput {
            inputs,
            actions: vec![],
            snapshot: None,
            migrations: vec![],
            cleanup: None,
        })
        .unwrap();
        let runtime =
            project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![] })
                .unwrap();
        let content = validate_declaration_content(core, runtime).unwrap();
        let package = f["normalized_manifest"]["producer"]["package_id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let digest = crate::revision_declarations::calculate_service_free_digest(&content).unwrap();
        let mut raw: serde_json::Value =
            serde_json::from_str(f["raw_manifest"].as_str().unwrap()).unwrap();
        raw["producer"]["revision_content_digest"] = serde_json::json!(digest.to_string());
        if invalid_protection {
            let token = raw["managed_bindings"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|binding| binding["input_id"] == "token")
                .unwrap();
            token["protection"] = serde_json::json!("normal");
        }
        f["raw_manifest"] = serde_json::json!(serde_json::to_string(&raw).unwrap());
        let (mut bundle, _) = prepared_fixture(&p, &f);
        let id = bundle.manifest().manifest().snapshot_id();
        assert_eq!(
            p.import_snapshot_bundle(&mut bundle).unwrap().relational,
            SnapshotRelationalVerification::NotEvaluated
        );
        let canonical = bundle.manifest().canonical_bytes().to_vec();
        p.persist_revision(package, &content, &[]).unwrap();
        assert_eq!(bundle.manifest().canonical_bytes(), canonical);
        let result = p.verify_snapshot(id);
        if !invalid_protection {
            assert_eq!(
                result.unwrap().relational,
                SnapshotRelationalVerification::Valid
            );
        } else {
            assert!(matches!(
                result,
                Err(PersistenceError::SnapshotCodec(
                    SnapshotCodecError::Validation(
                        SnapshotValidationError::InvalidBindingProtection
                    )
                ))
            ));
        }
    }
}

// Test-ID: PR-TEST-0215
// Verifies: PR-REQ-0292, PR-REQ-0293, PR-REQ-0298, PR-REQ-0302
#[test]
fn real_service_payload_above_512_mib_round_trips_but_is_not_a_publishable_managed_input() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let length = 512_u64 * 1024 * 1024 + 1;
    let buffer = [0u8; 64 * 1024];
    let mut hash = Sha256::new();
    let mut remaining = length;
    while remaining > 0 {
        let n = remaining.min(buffer.len() as u64) as usize;
        hash.update(&buffer[..n]);
        remaining -= n as u64;
    }
    let digest = Sha256Digest::from_bytes(hash.finalize().into());
    let mut raw = serde_json::json!({"format_version":"1.0-alpha.1","snapshot_id":"11111111111111111111111111111111","producer":{"package_id":"22222222222222222222222222222222","revision_content_digest":format!("sha256:{}","3".repeat(64))},"origin_instance_id":"44444444444444444444444444444444","captured_at":{"unix_seconds":0,"nanoseconds":0},"managed_bindings":[],"service_content":[{"role":"database","path":"backup.bin","blob_digest":digest.as_str()}]});
    raw["managed_bindings"] = serde_json::json!([{"input_id":"large","role":"active","state":"bound","protection":"normal","blob_digest":digest.as_str()}]);
    let manifest = decode_snapshot_manifest(
        SnapshotIntegrityVersion::BASELINE,
        &serde_json::to_vec(&raw).unwrap(),
        None,
    )
    .unwrap();
    let id = manifest.manifest().snapshot_id();
    let mut stage = p
        .staging_session()
        .unwrap()
        .create_snapshot_stage()
        .unwrap();
    {
        let mut writer = snapshot_bundle::start_bundle(&mut stage, &manifest).unwrap();
        writer.start_blob(&digest, length).unwrap();
        let mut remaining = length;
        while remaining > 0 {
            let n = remaining.min(buffer.len() as u64) as usize;
            writer.write_all(&buffer[..n]).unwrap();
            remaining -= n as u64;
        }
        writer.finish().unwrap();
    }
    stage.finish_operation_file().unwrap();
    let mut bundle = ValidatedSnapshotBundle::read(stage).unwrap();
    p.import_snapshot_bundle(&mut bundle).unwrap();
    drop(bundle);
    assert!(matches!(
        p.inspect_snapshot(id).unwrap().restore_capability,
        Err(CapabilityRefusal {
            capability: SnapshotCapability::RestoreInput
        })
    ));
    p.verify_snapshot(id).unwrap();
    let exported =
        ValidatedSnapshotBundle::read(p.export_snapshot_stage(id, true).unwrap()).unwrap();
    assert_eq!(exported.lengths()[&digest], length);
    assert_eq!(
        exported.manifest().integrity_digest(),
        manifest.integrity_digest()
    );
}

#[test]
fn snapshot_import_worker() {
    let Some(path) = std::env::var_os("PACTRUN_S3_ROOT") else {
        return;
    };
    let p = PactrunPersistence::open(PathBuf::from(path)).unwrap();
    let mut bundle = prepared(&p, 2);
    p.import_snapshot_bundle(&mut bundle).unwrap();
}

// Test-ID: PR-TEST-0480
// Verifies: PR-REQ-0292, PR-REQ-0293, PR-REQ-0298, PR-REQ-0347
#[test]
fn snapshot_database_full_rolls_back_without_misclassifying_content() {
    let (_tmp, path) = root();
    let p = PactrunPersistence::open(&path).unwrap();
    let bytes = vec![0x31; 2 * 1024 * 1024];
    let digest = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
    let mut f = fixture(2);
    let mut raw: serde_json::Value =
        serde_json::from_str(f["raw_manifest"].as_str().unwrap()).unwrap();
    raw["managed_bindings"] = serde_json::json!([]);
    raw["service_content"] = serde_json::Value::Array((0..4096).map(|i| serde_json::json!({"role":"database","path":format!("data/item-{i}"),"blob_digest":digest.as_str()})).collect());
    f["raw_manifest"] = serde_json::json!(raw.to_string());
    f["blob_contents"] = serde_json::json!({digest.as_str():hex::encode(bytes)});
    let (mut bundle, _) = prepared_fixture(&p, &f);
    let id = bundle.manifest().manifest().snapshot_id();
    {
        let db = p.database.lock().unwrap();
        let pages: i64 = db.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap();
        db.pragma_update(None, "max_page_count", pages).unwrap();
    }
    let error = p.import_snapshot_bundle(&mut bundle).unwrap_err();
    assert!(matches!(error, PersistenceError::Sqlite { ref source, .. }
        if source.sqlite_error_code() == Some(rusqlite::ErrorCode::DiskFull)));
    assert!(p.list_snapshots(None).unwrap().is_empty());
    {
        let db = p.database.lock().unwrap();
        for table in ["snapshot_blobs", "runs"] {
            assert_eq!(
                db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        db.pragma_update(None, "max_page_count", 2147483646_i64)
            .unwrap();
    }
    // The identical validated content is usable after the resource is restored.
    assert!(p.import_snapshot_bundle(&mut bundle).unwrap().inserted);
    p.verify_snapshot(id).unwrap();
    drop(p);
    PactrunPersistence::open(&path)
        .unwrap()
        .verify_snapshot(id)
        .unwrap();
}

// Test-ID: PR-TEST-0212
// Verifies: PR-REQ-0292, PR-REQ-0298, PR-REQ-0347
#[test]
fn process_loss_before_and_after_import_commit_has_one_authoritative_boundary() {
    for (fault, count) in [
        ("before_snapshot_import_commit", 0),
        ("after_snapshot_import_commit", 1),
    ] {
        let (_tmp, path) = root();
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "persistence::sqlite_snapshots::tests::snapshot_import_worker",
                "--nocapture",
            ])
            .env("PACTRUN_S3_ROOT", &path)
            .env("PACTRUN_OPERATION_TEST_FAULT", fault)
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(87));
        let p = PactrunPersistence::open(&path).unwrap();
        assert_eq!(p.list_snapshots(None).unwrap().len(), count);
        drop(p);
        let collector = PactrunPersistence::open_for_collection(&path).unwrap();
        let report = collector.collect_content(true).unwrap();
        if count == 0 {
            assert!(report.removed > 0);
        } else {
            assert_eq!(report.removed, 0);
            assert!(report.retained > 0);
        }
        drop(collector);
        let p = PactrunPersistence::open(&path).unwrap();
        let mut bundle = prepared(&p, 2);
        p.import_snapshot_bundle(&mut bundle).unwrap();
        assert_eq!(p.list_snapshots(None).unwrap().len(), 1);
    }
}
