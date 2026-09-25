use super::*;
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

fn session() -> (TempDir, StagingSession) {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m4-s3-tests");
    fs::create_dir_all(&parent).unwrap();
    let temp = tempfile::Builder::new()
        .prefix("bundle-")
        .tempdir_in(parent)
        .unwrap();
    let session = StagingSession::prepare(temp.path()).unwrap();
    (temp, session)
}

// Test-ID: PR-TEST-0481
// Verifies: PR-REQ-0292, PR-REQ-0293
#[test]
fn snapshot_staging_and_copy_preserve_resource_errors_and_do_not_publish() {
    struct FailingReader(bool);
    impl Read for FailingReader {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if self.0 {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            self.0 = true;
            bytes[..16].fill(7);
            Ok(16)
        }
    }
    struct Full;
    impl Write for Full {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::StorageFull.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let (_temp, staging) = session();
    assert!(matches!(
        stage_bundle(&staging, &mut FailingReader(false)),
        Err(BundleError::Io(io::ErrorKind::PermissionDenied))
    ));
    assert!(matches!(
        copy_bounded(
            &mut Cursor::new([1; 1024]),
            &mut Full,
            SnapshotCapability::BundleBytes
        ),
        Err(BundleError::Io(io::ErrorKind::StorageFull))
    ));
}
fn fixture(version: u32, name: &str) -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "tests/vectors/snapshot_integrity_format_v{version}/vectors.json"
    ));
    let corpus: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    corpus["valid"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap()
        .clone()
}
fn bundle_bytes(f: &serde_json::Value) -> Vec<u8> {
    let version = SnapshotIntegrityVersion::from_number(
        f["normalized_manifest"]["format_version"].as_i64().unwrap(),
    )
    .unwrap();
    let manifest = snapshot_integrity::decode_snapshot_manifest(
        version,
        f["raw_manifest"].as_str().unwrap().as_bytes(),
        None,
    )
    .unwrap();
    let envelope = serde_json::json!({"kind":"pactrun_snapshot_bundle","bundle_version":1,"integrity_format":version.number(),"integrity_digest":manifest.integrity_digest().as_str()});
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o600);
    // Intentionally not the production writer's member order.
    writer.start_file("manifest.json", options).unwrap();
    writer.write_all(manifest.canonical_bytes()).unwrap();
    for (digest, bytes) in f["blob_contents"].as_object().unwrap() {
        let digest = Sha256Digest::parse(digest).unwrap();
        let bytes = hex::decode(bytes.as_str().unwrap()).unwrap();
        start_blob(&mut writer, &digest, bytes.len() as u64).unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.start_file("bundle.json", options).unwrap();
    writer
        .write_all(&serde_json::to_vec(&envelope).unwrap())
        .unwrap();
    writer.finish().unwrap().into_inner()
}
fn parse(session: &StagingSession, bytes: &[u8]) -> Result<ValidatedSnapshotBundle, BundleError> {
    ValidatedSnapshotBundle::read(stage_bundle(session, &mut Cursor::new(bytes))?)
}

// Test-ID: PR-TEST-0206
// Verifies: PR-REQ-0085, PR-REQ-0292, PR-REQ-0297
#[test]
fn stored_bundle_preserves_v1_and_v2_and_verifies_exact_payload_closure() {
    let (_temp, s) = session();
    for (version, name) in [
        (1, "complete_binding_state"),
        (2, "active_normal_sticky_secret"),
        (2, "v2_minimal_manifest"),
        (2, "empty_bound_is_not_absence"),
    ] {
        let f = fixture(version, name);
        let bytes = bundle_bytes(&f);
        let bundle = parse(&s, &bytes).unwrap();
        assert_eq!(bundle.manifest().manifest().version().number(), version);
        assert_eq!(
            bundle.manifest().integrity_digest().as_str(),
            f["expected"]["digest"].as_str().unwrap()
        );
        assert_eq!(
            bundle.lengths().len(),
            f["blob_contents"].as_object().unwrap().len()
        );
    }
}

// Test-ID: PR-TEST-0207
// Verifies: PR-REQ-0292, PR-REQ-0293
#[test]
fn bounded_preflight_rejects_compression_links_duplicates_gaps_and_false_sizes() {
    let (_temp, s) = session();
    let bytes = bundle_bytes(&fixture(2, "v2_complete_binding_state"));
    let mut file = stage_bundle(&s, &mut Cursor::new(&bytes))
        .unwrap()
        .try_clone_reader()
        .unwrap();
    let (entries, directory) = preflight(&mut file).unwrap();
    for mutate in 0..7 {
        let mut bad = bytes.clone();
        let p = directory as usize;
        match mutate {
            0 => bad[p + 10..p + 12].copy_from_slice(&8u16.to_le_bytes()),
            1 => bad[p + 8..p + 10].copy_from_slice(&1u16.to_le_bytes()),
            2 => bad[p + 38..p + 42].copy_from_slice(&(0o120600u32 << 16).to_le_bytes()),
            3 => bad[entries[0].header as usize + 6..entries[0].header as usize + 8]
                .copy_from_slice(&1u16.to_le_bytes()),
            4 => bad[p + 28..p + 30].copy_from_slice(&65535u16.to_le_bytes()),
            5 => {
                bad.insert(0, 0);
            }
            _ => {
                bad.truncate(bad.len() - 1);
            }
        }
        assert!(parse(&s, &bad).is_err(), "mutation {mutate} accepted");
    }
    let mut bad = bytes.clone();
    bad[entries[1].data as usize] ^= 1;
    assert!(parse(&s, &bad).is_err());
    let blob_indices: Vec<_> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.name.starts_with("blobs/"))
        .map(|(i, _)| i)
        .collect();
    let mut position = directory as usize;
    let mut central = Vec::new();
    for _ in &entries {
        central.push(position);
        position += 46
            + u16le(&bytes, position + 28) as usize
            + u16le(&bytes, position + 30) as usize
            + u16le(&bytes, position + 32) as usize;
    }
    let mut bad = bytes.clone();
    let (first, second) = (blob_indices[0], blob_indices[1]);
    let start = central[second] + 46;
    bad[start..start + entries[first].name.len()].copy_from_slice(entries[first].name.as_bytes());
    assert!(parse(&s, &bad).is_err());
}

// Test-ID: PR-TEST-0216
// Verifies: PR-REQ-0292, PR-REQ-0293
#[test]
fn zip64_entry_count_boundary_is_real_and_not_a_32_bit_mock() {
    let (_temp, s) = session();
    let mut raw = fixture(2, "v2_minimal_manifest")["normalized_manifest"].clone();
    let mut descriptors = Vec::new();
    let mut blobs = Vec::new();
    for index in 0..65_536u64 {
        let bytes = index.to_le_bytes();
        let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
        descriptors.push(serde_json::json!({"role":"state","path":format!("p{index}"),"blob_digest":digest.as_str()}));
        blobs.push((digest, bytes));
    }
    raw["service_content"] = serde_json::json!(descriptors);
    let manifest = snapshot_integrity::decode_snapshot_manifest(
        SnapshotIntegrityVersion::V2,
        &serde_json::to_vec(&raw).unwrap(),
        None,
    )
    .unwrap();
    let mut stage = s.create_snapshot_stage().unwrap();
    {
        let mut writer = start_bundle(&mut stage, &manifest).unwrap();
        for (digest, bytes) in &blobs {
            writer.start_blob(digest, 8).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    stage.finish_operation_file().unwrap();
    let mut handle = stage.try_clone_reader().unwrap();
    let length = handle.metadata().unwrap().len();
    let tail = variable(&mut handle, length - 98, 98, length).unwrap();
    assert!(tail.windows(4).any(|b| b == 0x07064b50u32.to_le_bytes()));
    let bundle = ValidatedSnapshotBundle::read(stage).unwrap();
    assert_eq!(bundle.lengths().len(), 65_536);
}

// Test-ID: PR-TEST-0276
// Verifies: PR-REQ-0292, PR-REQ-0293
#[test]
fn zip64_large_member_and_directory_offsets_use_a_real_file_beyond_u32() {
    use std::io::{Seek, SeekFrom};
    fn set16(b: &mut [u8], p: usize, n: u16) {
        b[p..p + 2].copy_from_slice(&n.to_le_bytes());
    }
    fn set32(b: &mut [u8], p: usize, n: u32) {
        b[p..p + 4].copy_from_slice(&n.to_le_bytes());
    }
    let (_temp, session) = session();
    let mut stage = session.create_snapshot_stage().unwrap();
    let large = u32::MAX as u64 + 2;
    let blob = format!("blobs/sha256/{}", "0".repeat(64));
    let mut entries = Vec::new();
    for (name, size) in [
        ("bundle.json", 0),
        ("manifest.json", 0),
        (blob.as_str(), large),
    ] {
        let offset = stage.writer().stream_position().unwrap();
        let wide = size > u32::MAX as u64;
        let mut extra = Vec::new();
        if wide {
            extra.extend_from_slice(&1u16.to_le_bytes());
            extra.extend_from_slice(&16u16.to_le_bytes());
            extra.extend_from_slice(&size.to_le_bytes());
            extra.extend_from_slice(&size.to_le_bytes());
        }
        let mut h = [0; 30];
        set32(&mut h, 0, 0x04034b50);
        set16(&mut h, 4, if wide { 45 } else { 20 });
        set32(&mut h, 18, if wide { u32::MAX } else { size as u32 });
        set32(&mut h, 22, if wide { u32::MAX } else { size as u32 });
        set16(&mut h, 26, name.len() as u16);
        set16(&mut h, 28, extra.len() as u16);
        stage.writer().write_all(&h).unwrap();
        stage.writer().write_all(name.as_bytes()).unwrap();
        stage.writer().write_all(&extra).unwrap();
        let data = stage.writer().stream_position().unwrap();
        stage.writer().seek(SeekFrom::Start(data + size)).unwrap();
        entries.push((name.to_owned(), size, offset, extra));
    }
    let directory = stage.writer().stream_position().unwrap();
    assert!(directory > u32::MAX as u64);
    for (name, size, offset, extra) in &entries {
        let mut h = [0; 46];
        set32(&mut h, 0, 0x02014b50);
        set16(&mut h, 4, 45);
        set16(&mut h, 6, if *size > u32::MAX as u64 { 45 } else { 20 });
        set32(
            &mut h,
            20,
            if *size > u32::MAX as u64 {
                u32::MAX
            } else {
                *size as u32
            },
        );
        set32(
            &mut h,
            24,
            if *size > u32::MAX as u64 {
                u32::MAX
            } else {
                *size as u32
            },
        );
        set16(&mut h, 28, name.len() as u16);
        set16(&mut h, 30, extra.len() as u16);
        set32(&mut h, 42, *offset as u32);
        stage.writer().write_all(&h).unwrap();
        stage.writer().write_all(name.as_bytes()).unwrap();
        stage.writer().write_all(extra).unwrap();
    }
    let zip64_offset = stage.writer().stream_position().unwrap();
    let directory_size = zip64_offset - directory;
    let mut record = Vec::new();
    record.extend_from_slice(&0x06064b50u32.to_le_bytes());
    record.extend_from_slice(&44u64.to_le_bytes());
    record.extend_from_slice(&45u16.to_le_bytes());
    record.extend_from_slice(&45u16.to_le_bytes());
    record.extend_from_slice(&[0; 8]);
    for n in [3, 3, directory_size, directory] {
        record.extend_from_slice(&n.to_le_bytes());
    }
    stage.writer().write_all(&record).unwrap();
    let locator = stage.writer().stream_position().unwrap();
    stage
        .writer()
        .write_all(&0x07064b50u32.to_le_bytes())
        .unwrap();
    stage.writer().write_all(&0u32.to_le_bytes()).unwrap();
    stage
        .writer()
        .write_all(&zip64_offset.to_le_bytes())
        .unwrap();
    stage.writer().write_all(&1u32.to_le_bytes()).unwrap();
    let mut end = [0; 22];
    set32(&mut end, 0, 0x06054b50);
    set16(&mut end, 8, u16::MAX);
    set16(&mut end, 10, u16::MAX);
    set32(&mut end, 12, u32::MAX);
    set32(&mut end, 16, u32::MAX);
    stage.writer().write_all(&end).unwrap();
    stage.finish_operation_file().unwrap();
    let (parsed, length) = preflight(&mut stage.try_clone_reader().unwrap()).unwrap();
    assert!(length > u32::MAX as u64);
    assert_eq!(parsed.iter().find(|e| e.name == blob).unwrap().size, large);
    let mut archive = zip::ZipArchive::new(stage.try_clone_reader().unwrap()).unwrap();
    assert_eq!(archive.by_name(&blob).unwrap().size(), large);
    drop(archive);
    // This is transport-profile evidence only: sparse payload bytes are not
    // claimed to be a verified Snapshot closure. Actual streaming is covered by
    // PR-TEST-0215/0251/0266. Corrupt offsets must fail before allocation/read.
    stage.writer().seek(SeekFrom::Start(locator + 8)).unwrap();
    stage.writer().write_all(&u64::MAX.to_le_bytes()).unwrap();
    stage.writer().flush().unwrap();
    assert!(matches!(
        preflight(&mut stage.try_clone_reader().unwrap()),
        Err(BundleError::Profile(_))
    ));
}

// Test-ID: PR-TEST-0217
// Verifies: PR-REQ-0292, PR-REQ-0293
#[test]
fn parser_limits_and_data_descriptors_do_not_require_canonical_archive_bytes() {
    let (_temp, s) = session();
    let f = fixture(2, "v2_minimal_manifest");
    let manifest = snapshot_integrity::decode_snapshot_manifest(
        SnapshotIntegrityVersion::V2,
        f["raw_manifest"].as_str().unwrap().as_bytes(),
        None,
    )
    .unwrap();
    let base=serde_json::to_vec(&serde_json::json!({"kind":"pactrun_snapshot_bundle","bundle_version":1,"integrity_format":2,"integrity_digest":manifest.integrity_digest().as_str()})).unwrap();
    for extra in [0, 1] {
        let mut envelope = base.clone();
        envelope.resize(
            SnapshotCapability::BundleEnvelope.maximum().unwrap() as usize + extra,
            b' ',
        );
        let mut writer = ZipWriter::new_stream(Vec::new());
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer.start_file("bundle.json", options).unwrap();
        writer.write_all(&envelope).unwrap();
        writer.start_file("manifest.json", options).unwrap();
        writer.write_all(manifest.canonical_bytes()).unwrap();
        writer
            .set_comment("noncanonical metadata is not Snapshot authority")
            .unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let result = parse(&s, &bytes);
        if extra == 0 {
            assert!(result.is_ok(), "{result:?}");
        } else {
            assert!(matches!(
                result,
                Err(BundleError::Capability(CapabilityRefusal {
                    capability: SnapshotCapability::BundleEnvelope
                }))
            ));
        }
    }
    let bytes = bundle_bytes(&f);
    let mut bad = bytes.clone();
    let e = bad.len() - 22;
    // A forged ZIP64 locator is rejected before any offset can reach an allocation.
    bad[e + 10..e + 12].copy_from_slice(&65535u16.to_le_bytes());
    bad[e + 8..e + 10].copy_from_slice(&65535u16.to_le_bytes());
    assert!(parse(&s, &bad).is_err());
    // An unsigned descriptor's CRC may equal the optional signature itself.
    let mut descriptor = s.create_snapshot_stage().unwrap();
    for value in [0x08074b50u32, 7, 7, 0] {
        descriptor.writer().write_all(&value.to_le_bytes()).unwrap();
    }
    descriptor.finish_operation_file().unwrap();
    assert_eq!(
        descriptor_length(
            &mut descriptor.try_clone_reader().unwrap(),
            0,
            16,
            false,
            0x08074b50,
            7
        )
        .unwrap(),
        12,
    );
}

struct FailingWriter;
impl Write for FailingWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::StorageFull,
            "do-not-leak-private-error",
        ))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Seek for FailingWriter {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        Ok(0)
    }
}
#[test]
fn zip_failure_worker() {
    if std::env::var_os("PACTRUN_S3_ZIP_FAILURE").is_none() {
        return;
    };
    let failed = std::rc::Rc::new(std::cell::Cell::new(None));
    let sink = DropSafeWriter {
        inner: FailingWriter,
        failed: failed.clone(),
        position: 0,
        length: 0,
    };
    let mut zip = ZipWriter::new(sink);
    assert!(
        zip.start_file(
            "bundle.json",
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored)
        )
        .is_err()
    );
    assert_eq!(failed.get(), Some(io::ErrorKind::StorageFull));
    drop(zip);
    let (temporary, _session) = session();
    let path = temporary.path().join("readonly-output");
    fs::write(&path, []).unwrap();
    let mut file = File::open(&path).unwrap();
    let failed = std::rc::Rc::new(std::cell::Cell::new(None));
    let writer = ZipWriter::new(DropSafeWriter {
        inner: &mut file,
        failed: failed.clone(),
        position: 0,
        length: 0,
    });
    let mut facade = SnapshotBundleWriter { writer, failed };
    assert!(
        facade
            .start_blob(&Sha256Digest::from_bytes([0; 32]), 0)
            .is_err()
    );
    // Even a caller that ignores the first error cannot turn cleanup into success.
    assert!(facade.finish().is_err());
    assert_eq!(fs::metadata(path).unwrap().len(), 0);
}

// Test-ID: PR-TEST-0218
// Verifies: PR-REQ-0292, PR-REQ-0302
#[test]
fn zip_failure_is_not_logged_by_dependency_drop_or_reported_as_success() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "snapshot_bundle::tests::zip_failure_worker",
            "--nocapture",
        ])
        .env("PACTRUN_S3_ZIP_FAILURE", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        result.stderr.is_empty(),
        "ZIP dependency must not log its errors"
    );
}

// Test-ID: PR-TEST-0219
// Verifies: PR-REQ-0292
#[test]
fn envelope_and_exact_closure_are_checked_independently_of_valid_zip_crc() {
    let (_temp, s) = session();
    let original = bundle_bytes(&fixture(2, "v2_complete_binding_state"));
    let mut zip = ZipArchive::new(Cursor::new(&original)).unwrap();
    let mut members = Vec::new();
    for index in 0..zip.len() {
        let mut file = zip.by_index(index).unwrap();
        let name = file.name().to_owned();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        members.push((name, bytes));
    }
    for mutation in 0..6 {
        let mut modified = members.clone();
        match mutation {
            0 => {
                let i = modified
                    .iter()
                    .position(|(n, _)| n == "bundle.json")
                    .unwrap();
                let mut e: serde_json::Value = serde_json::from_slice(&modified[i].1).unwrap();
                e["integrity_digest"] = serde_json::json!(format!("sha256:{}", "0".repeat(64)));
                modified[i].1 = serde_json::to_vec(&e).unwrap();
            }
            1 => {
                let i = modified
                    .iter()
                    .position(|(n, _)| n == "bundle.json")
                    .unwrap();
                let mut e: serde_json::Value = serde_json::from_slice(&modified[i].1).unwrap();
                e["integrity_format"] = serde_json::json!(1);
                modified[i].1 = serde_json::to_vec(&e).unwrap();
            }
            2 => {
                let i = modified
                    .iter()
                    .position(|(n, _)| n.starts_with("blobs/"))
                    .unwrap();
                modified.remove(i);
            }
            3 => {
                let d = hex::encode(Sha256::digest(b"unrelated"));
                modified.push((format!("blobs/sha256/{d}"), b"unrelated".to_vec()));
            }
            4 => {
                let i = modified
                    .iter()
                    .position(|(n, b)| n.starts_with("blobs/") && !b.is_empty())
                    .unwrap();
                modified[i].1[0] ^= 1;
            }
            _ => {
                let i = modified
                    .iter()
                    .position(|(n, _)| n == "bundle.json")
                    .unwrap();
                modified[i].1 = format!("{}0{}", "[".repeat(17), "]".repeat(17)).into_bytes();
            }
        }
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in modified {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        let bytes = writer.finish().unwrap().into_inner();
        let result = parse(&s, &bytes);
        assert!(result.is_err(), "mutation {mutation} accepted");
        if mutation == 4 {
            assert!(matches!(
                result,
                Err(BundleError::Integrity(SnapshotCodecError::Validation(
                    SnapshotValidationError::ContentDigestMismatch
                )))
            ));
        }
        if mutation == 5 {
            assert!(matches!(
                result,
                Err(BundleError::Capability(CapabilityRefusal {
                    capability: SnapshotCapability::JsonDepth
                }))
            ));
        }
    }
}
