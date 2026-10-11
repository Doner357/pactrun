use super::*;
use crate::{application::PactrunApplication, persistence::PactrunPersistence};
use std::fs;
use std::path::PathBuf;

fn roots() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/pack-transport-tests");
    fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}
fn store(parent: &Path, name: &str) -> PathBuf {
    let root = parent.join(name);
    for part in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(part)).unwrap();
    }
    root
}
fn source(parent: &Path, version: u8) -> PathBuf {
    let root = parent.join(format!("source-{version}"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("data.bin"), b"exact runtime bytes\0\xff").unwrap();
    fs::write(root.join("empty.bin"), b"").unwrap();
    fs::write(root.join("pactrun.yaml"),"source_format: 1.0-alpha.2\npackage_id: 00000000000000000000000000000011\nrevision: {}\nruntime_content:\n  files:\n    - {id: alpha, source: data.bin, path: lib/alpha.bin, executable: true}\n    - {id: beta, source: data.bin, path: lib/beta.bin}\n    - {id: empty, source: empty.bin, path: empty.bin}\nportable_metadata:\n  presentation: [{target: {kind: revision}, field: summary, value: Published}]\n  provenance: [{kind: source_uri, source_uri: 'https://example.test/source'}]\n").unwrap();
    if version == 3 {
        let manifest = fs::read_to_string(root.join("pactrun.yaml")).unwrap();
        fs::write(root.join("pactrun.yaml"),manifest.replace("revision: {}","revision:\n  actions:\n    - id: run\n      access: observe\n      parameters: []\n      outputs: []\n      hook:\n        protocol_version: 1.0-alpha.1\n        launch: {kind: shell_loader, shell: sh, command: sh, script: alpha}\n        args: []\n        io: {terminal: none}")).unwrap();
    }
    root
}
fn zip_files(path: &Path, files: &[(String, Vec<u8>)], method: zip::CompressionMethod, wide: bool) {
    let mut writer = zip::ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in files {
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(method)
                    .large_file(wide),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}
fn source_archive(source: &Path, path: &Path, method: zip::CompressionMethod, wide: bool) {
    let files = ["pactrun.yaml", "data.bin", "empty.bin"]
        .map(|n| (n.into(), fs::read(source.join(n)).unwrap()));
    zip_files(path, &files, method, wide);
}
fn unpack(path: &Path, root: &Path) {
    fs::create_dir_all(root).unwrap();
    let mut zip = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).unwrap();
        let target = root.join(file.name());
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        let mut out = File::create(target).unwrap();
        io::copy(&mut file, &mut out).unwrap();
    }
}
fn install(app: &PactrunApplication, path: &Path) -> crate::application::InstallPackResult {
    app.install_pack(
        path,
        PackMetadataConflict::Reject,
        &ActionCancellation::default(),
    )
    .unwrap()
}

// Test-ID: PR-TEST-0594
// Verifies: PR-REQ-0011, PR-REQ-0012, PR-REQ-0013, PR-REQ-0084
#[test]
fn transport_preserves_distinct_lineages_and_revisions_with_equal_or_changed_content() {
    for version in [1, 2, 3] {
        let temp = roots();
        let a = store(temp.path(), "identity-source");
        let b = store(temp.path(), "identity-destination");
        let source = source(temp.path(), version);
        let app = PactrunApplication::open(&a).unwrap();
        let first = install(&app, &source).revision;
        let manifest = source.join("pactrun.yaml");
        let text = fs::read_to_string(&manifest).unwrap();
        fs::write(
            &manifest,
            text.replace(
                "00000000000000000000000000000011",
                "00000000000000000000000000000012",
            ),
        )
        .unwrap();
        let fork = install(&app, &source).revision;
        assert_ne!(first.package_id, fork.package_id);
        assert_eq!(first.content_digest, fork.content_digest);
        fs::write(
            source.join("data.bin"),
            b"changed opaque runtime bytes\0\xff",
        )
        .unwrap();
        let changed = install(&app, &source).revision;
        assert_eq!(fork.package_id, changed.package_id);
        assert_ne!(fork.content_digest, changed.content_digest);
        let destination = PactrunApplication::open(&b).unwrap();
        for (index, expected) in [first, fork, changed].iter().enumerate() {
            let bundle = temp.path().join(format!("identity-{index}.pack"));
            app.export_revision_pack(expected, &bundle, false, &ActionCancellation::default())
                .unwrap();
            assert_eq!(install(&destination, &bundle).revision, *expected);
            assert_eq!(install(&destination, &bundle).revision, *expected);
            let before = PactrunPersistence::open(&a)
                .unwrap()
                .load_revision(expected)
                .unwrap()
                .unwrap();
            let after = PactrunPersistence::open(&b)
                .unwrap()
                .load_revision(expected)
                .unwrap()
                .unwrap();
            assert_eq!(before, after);
        }
    }
}

// Test-ID: PR-TEST-0600
// Verifies: PR-REQ-0011, PR-REQ-0012, PR-REQ-0014, PR-REQ-0015, PR-REQ-0138, PR-REQ-0142
#[test]
fn installed_digest_tracks_logical_roles_not_source_paths_or_portable_metadata() {
    for version in [1, 2, 3] {
        let temp = roots();
        let source = source(temp.path(), version);
        let manifest = source.join("pactrun.yaml");
        let original = fs::read_to_string(&manifest).unwrap();
        fs::copy(source.join("data.bin"), source.join("renamed.bin")).unwrap();
        let baseline = install(
            &PactrunApplication::open(store(temp.path(), "baseline")).unwrap(),
            &source,
        )
        .revision;
        for (index, (text, identity_changes)) in [
            (
                original.replace("source: data.bin", "source: renamed.bin"),
                false,
            ),
            (
                original
                    .replace("Published", "Different presentation")
                    .replace("label: stable", "label: other")
                    .replace(
                        "https://example.test/source",
                        "https://example.test/elsewhere",
                    ),
                false,
            ),
            (
                original
                    .split("portable_metadata:")
                    .next()
                    .unwrap()
                    .to_owned(),
                false,
            ),
            (
                original.replace("path: lib/alpha.bin", "path: lib/different.bin"),
                true,
            ),
            (original.replace("id: beta", "id: different_role"), true),
            (
                original.replace("executable: true", "executable: false"),
                true,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            fs::write(&manifest, text).unwrap();
            let app =
                PactrunApplication::open(store(temp.path(), &format!("case-{index}"))).unwrap();
            let actual = install(&app, &source).revision;
            assert_eq!(actual.package_id, baseline.package_id);
            assert_eq!(
                actual.content_digest != baseline.content_digest,
                identity_changes,
                "Core {version} case {index}"
            );
        }
    }
}

// Test-ID: PR-TEST-0538
// Verifies: PR-REQ-0022, PR-REQ-0077, PR-REQ-0081, PR-REQ-0084, PR-REQ-0118, PR-REQ-0354, PR-REQ-0356
#[test]
fn all_pack_forms_preserve_canonical_identity_without_original_source() {
    for version in [1, 2, 3] {
        let temp = roots();
        let a = store(temp.path(), "a");
        let b = store(temp.path(), "b");
        let source = source(temp.path(), version);
        let app = PactrunApplication::open(&a).unwrap();
        let installed = install(&app, &source);
        let archive = temp.path().join("source.pack");
        for method in [
            zip::CompressionMethod::Stored,
            zip::CompressionMethod::Deflated,
        ] {
            for wide in [false, true] {
                source_archive(&source, &archive, method, wide);
                assert_eq!(install(&app, &archive).revision, installed.revision);
            }
        }
        let distribution = temp.path().join("distribution.pack");
        app.export_revision_pack(
            &installed.revision,
            &distribution,
            false,
            &ActionCancellation::default(),
        )
        .unwrap();
        fs::remove_dir_all(&source).unwrap();
        let other = PactrunApplication::open(&b).unwrap();
        assert_eq!(install(&other, &distribution).revision, installed.revision);
        assert_eq!(install(&other, &distribution).revision, installed.revision);
        let unpacked = temp.path().join("unpacked");
        unpack(&distribution, &unpacked);
        assert_eq!(install(&other, &unpacked).revision, installed.revision);
        let p = PactrunPersistence::open(&a).unwrap();
        let q = PactrunPersistence::open(&b).unwrap();
        assert_eq!(
            p.load_revision(&installed.revision).unwrap(),
            q.load_revision(&installed.revision).unwrap()
        );
        assert!(
            q.load_revision_metadata(&installed.revision)
                .unwrap()
                .items
                .is_empty()
        );
        let runtime = q
            .load_revision(&installed.revision)
            .unwrap()
            .unwrap()
            .content
            .runtime_content;
        assert_eq!(runtime.files().len(), 3);
        assert!(runtime.files()[0].executable);
        let copy = temp.path().join("copy.pack");
        other
            .export_revision_pack(
                &installed.revision,
                &copy,
                false,
                &ActionCancellation::default(),
            )
            .unwrap();
        let c = store(temp.path(), "c");
        assert_eq!(
            install(&PactrunApplication::open(&c).unwrap(), &copy).revision,
            installed.revision
        );
    }
}

// Test-ID: PR-TEST-0539
// Verifies: PR-REQ-0021, PR-REQ-0022, PR-REQ-0084, PR-REQ-0355
#[test]
fn portable_metadata_is_opt_in_and_conflicts_are_transactional() {
    let temp = roots();
    let a = store(temp.path(), "a");
    let b = store(temp.path(), "b");
    let source = source(temp.path(), 1);
    let app = PactrunApplication::open(&a).unwrap();
    let installed = install(&app, &source);
    let full = temp.path().join("full.pack");
    app.export_revision_pack(
        &installed.revision,
        &full,
        true,
        &ActionCancellation::default(),
    )
    .unwrap();
    let other = PactrunApplication::open(&b).unwrap();
    install(&other, &full);
    install(&other, &full);
    let p = PactrunPersistence::open(&b).unwrap();
    let before = p.load_revision_metadata(&installed.revision).unwrap();
    assert_eq!(before.items.len(), 2);
    let batch = RevisionMetadataMutationBatch::new([
        RevisionMetadataMutation::CompareAndSetPresentation {
            target: PresentationTargetV1::Revision,
            field: PresentationField::Summary,
            expected: CurrentState::Present(PresentationValue::parse("Published").unwrap()),
            desired: CurrentState::Present(PresentationValue::parse("Local").unwrap()),
        },
        RevisionMetadataMutation::CompareAndSetLocalNote {
            expected: CurrentState::Absent,
            desired: CurrentState::Present(LocalNote::parse("private local note").unwrap()),
        },
    ])
    .unwrap();
    p.apply_revision_metadata_batch(&installed.revision, &batch)
        .unwrap();
    let local = p.load_revision_metadata(&installed.revision).unwrap();
    assert!(
        other
            .install_pack(
                &full,
                PackMetadataConflict::Reject,
                &ActionCancellation::default()
            )
            .is_err()
    );
    assert_eq!(
        p.load_revision_metadata(&installed.revision).unwrap(),
        local
    );
    for input in [&full, &source] {
        let result = other
            .install_pack(
                input,
                PackMetadataConflict::Keep,
                &ActionCancellation::default(),
            )
            .unwrap();
        assert_eq!(result.kept_metadata.len(), 1);
        assert_eq!(
            p.load_revision_metadata(&installed.revision).unwrap(),
            local
        );
    }
    other
        .install_pack(
            &full,
            PackMetadataConflict::Overwrite,
            &ActionCancellation::default(),
        )
        .unwrap();
    let after = p.load_revision_metadata(&installed.revision).unwrap();
    assert_eq!(after.items.len(), 3);
    assert!(after.items.iter().any(
        |i| matches!(i,RevisionMetadataItem::Presentation(p) if p.value.as_str()=="Published")
    ));
    assert!(
        after
            .items
            .iter()
            .any(|i| matches!(i, RevisionMetadataItem::LocalNote { .. }))
    );
    let exported = temp.path().join("local-export.pack");
    other
        .export_revision_pack(
            &installed.revision,
            &exported,
            true,
            &ActionCancellation::default(),
        )
        .unwrap();
    let third = PactrunApplication::open(store(temp.path(), "c")).unwrap();
    install(&third, &exported);
    let p = PactrunPersistence::open(temp.path().join("c")).unwrap();
    assert_eq!(
        p.load_revision_metadata(&installed.revision).unwrap().items,
        before.items
    );
}

// Test-ID: PR-TEST-0540
// Verifies: PR-REQ-0022, PR-REQ-0081, PR-REQ-0084, PR-REQ-0354
#[test]
fn malformed_pack_never_bypasses_validation_on_repeat_install() {
    let temp = roots();
    let root = store(temp.path(), "a");
    let source = source(temp.path(), 1);
    let app = PactrunApplication::open(&root).unwrap();
    let id = install(&app, &source).revision;
    let distribution = temp.path().join("valid.pack");
    app.export_revision_pack(&id, &distribution, false, &ActionCancellation::default())
        .unwrap();
    let unpacked = temp.path().join("unpacked");
    unpack(&distribution, &unpacked);
    fs::write(unpacked.join("pactrun.yaml"), b"source_format: 1.0-alpha.2").unwrap();
    assert!(
        app.install_pack(
            &unpacked,
            PackMetadataConflict::Reject,
            &ActionCancellation::default()
        )
        .is_err()
    );
    fs::remove_file(unpacked.join("pactrun.yaml")).unwrap();
    fs::write(unpacked.join("extra"), b"unreferenced").unwrap();
    assert!(
        app.install_pack(
            &unpacked,
            PackMetadataConflict::Overwrite,
            &ActionCancellation::default()
        )
        .is_err()
    );
    fs::remove_file(unpacked.join("extra")).unwrap();
    let blob = fs::read_dir(unpacked.join("blobs/sha256"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(blob, b"corrupt").unwrap();
    assert!(
        app.install_pack(
            &unpacked,
            PackMetadataConflict::Keep,
            &ActionCancellation::default()
        )
        .is_err()
    );
    let manifest = fs::read(source.join("pactrun.yaml")).unwrap();
    for name in [
        "../escape",
        "/absolute",
        "a/../bad",
        "C:/absolute",
        "a\\b",
        "./bad",
        "x:stream",
    ] {
        let bad = temp.path().join("bad.pack");
        zip_files(
            &bad,
            &[
                ("pactrun.yaml".into(), manifest.clone()),
                (name.into(), vec![1]),
            ],
            zip::CompressionMethod::Stored,
            false,
        );
        assert!(
            app.install_pack(
                &bad,
                PackMetadataConflict::Reject,
                &ActionCancellation::default()
            )
            .is_err(),
            "{name}"
        );
    }
    let mut bytes = fs::read(&distribution).unwrap();
    bytes.truncate(bytes.len() - 5);
    let bad = temp.path().join("truncated.pack");
    fs::write(&bad, bytes).unwrap();
    assert!(
        app.install_pack(
            &bad,
            PackMetadataConflict::Reject,
            &ActionCancellation::default()
        )
        .is_err()
    );
    assert!(!temp.path().join("escape").exists());
}

// Test-ID: PR-TEST-0541
// Verifies: PR-REQ-0356
#[test]
fn pack_publication_never_clobbers_and_cancellation_never_installs() {
    let temp = roots();
    let root = store(temp.path(), "a");
    let source = source(temp.path(), 1);
    let app = PactrunApplication::open(&root).unwrap();
    let cancelled = ActionCancellation::default();
    cancelled.request();
    assert!(
        app.install_pack(&source, PackMetadataConflict::Reject, &cancelled)
            .is_err()
    );
    let id = install(&app, &source).revision;
    let output = temp.path().join("sentinel.pack");
    fs::write(&output, b"existing").unwrap();
    assert!(
        app.export_revision_pack(&id, &output, false, &ActionCancellation::default())
            .is_err()
    );
    assert_eq!(fs::read(&output).unwrap(), b"existing");
    let output = temp.path().join("cancelled.pack");
    assert!(
        app.export_revision_pack(&id, &output, false, &cancelled)
            .is_err()
    );
    assert!(!output.exists());
}

// Test-ID: PR-TEST-0542
// Verifies: PR-REQ-0354, PR-REQ-0081
#[test]
fn hostile_zip_records_and_distribution_descriptors_are_rejected() {
    let temp = roots();
    let root = store(temp.path(), "store");
    let source = source(temp.path(), 1);
    let session = StagingSession::prepare(&root).unwrap();
    let c = ActionCancellation::default();
    let valid = temp.path().join("valid.pack");
    source_archive(&source, &valid, zip::CompressionMethod::Deflated, true);
    let bytes = fs::read(&valid).unwrap();
    let central = bytes.windows(4).position(|s| s == b"PK\x01\x02").unwrap();
    let end = bytes.windows(4).rposition(|s| s == b"PK\x05\x06").unwrap();
    let patches = [
        (central + 10, vec![99, 0]),
        (central + 8, vec![1, 0]),
        (central + 38, 0xa1ff0000u32.to_le_bytes().to_vec()),
        (central + 42, 4u32.to_le_bytes().to_vec()),
        (8, vec![0, 0]),
        (end + 4, vec![1, 0]),
        (end + 10, vec![255, 255]),
        (central + 16, vec![0, 0, 0, 0]),
    ];
    for (offset, value) in patches {
        let mut corrupt = bytes.clone();
        corrupt[offset..offset + value.len()].copy_from_slice(&value);
        let path = temp.path().join("bad.pack");
        fs::write(&path, corrupt).unwrap();
        assert!(
            acquire(&path, &session, &c).is_err(),
            "accepted mutation at {offset}"
        );
    }
    for members in [
        vec![
            (
                "pactrun.yaml".into(),
                fs::read(source.join("pactrun.yaml")).unwrap(),
            ),
            ("pactrun-distribution.json".into(), b"{}".to_vec()),
        ],
        vec![("x".into(), vec![]), ("x/y".into(), vec![])],
        vec![("x".into(), vec![]), ("X".into(), vec![])],
    ] {
        let path = temp.path().join("conflict.pack");
        zip_files(&path, &members, zip::CompressionMethod::Stored, false);
        assert!(acquire(&path, &session, &c).is_err());
    }
    let app = PactrunApplication::open(&root).unwrap();
    let id = install(&app, &source).revision;
    let exported = temp.path().join("exported.pack");
    app.export_revision_pack(&id, &exported, true, &c).unwrap();
    let folder = temp.path().join("distribution");
    unpack(&exported, &folder);
    let descriptor = fs::read(folder.join(DESCRIPTOR)).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&descriptor).unwrap();
    for (key, value) in [
        ("format_version", serde_json::json!(2)),
        ("format_version", serde_json::json!(1.0)),
        ("kind", serde_json::json!("source")),
        (
            "revision_digest",
            serde_json::json!(format!("sha256:{}", "0".repeat(64))),
        ),
        ("unknown", serde_json::json!(true)),
    ] {
        let mut changed = original.clone();
        changed[key] = value;
        fs::write(
            folder.join(DESCRIPTOR),
            serde_json::to_vec(&changed).unwrap(),
        )
        .unwrap();
        assert!(acquire(&folder, &session, &c).is_err());
    }
    let text = String::from_utf8(descriptor).unwrap();
    fs::write(
        folder.join(DESCRIPTOR),
        text.replacen('{', "{\"kind\":\"pactrun_distribution\",", 1),
    )
    .unwrap();
    assert!(acquire(&folder, &session, &c).is_err());
}

// Test-ID: PR-TEST-0543
// Verifies: PR-REQ-0355, PR-REQ-0356
#[test]
fn concurrent_metadata_import_and_precommit_cancellation_preserve_atomicity() {
    use std::sync::{Arc, Barrier};
    let temp = roots();
    let root = store(temp.path(), "store");
    let one = temp.path().join("one");
    let two = temp.path().join("two");
    fs::create_dir(&one).unwrap();
    fs::create_dir(&two).unwrap();
    let s1 = source(&one, 1);
    let s2 = source(&two, 1);
    let original = fs::read_to_string(s2.join("pactrun.yaml")).unwrap();
    fs::write(
        s2.join("pactrun.yaml"),
        original.replace("Published", "Other"),
    )
    .unwrap();
    drop(PactrunApplication::open(&root).unwrap());
    let barrier = Arc::new(Barrier::new(2));
    let workers = [s1.clone(), s2]
        .into_iter()
        .map(|source| {
            let root = root.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let app = PactrunApplication::open(&root).unwrap();
                barrier.wait();
                app.install_pack(
                    &source,
                    PackMetadataConflict::Reject,
                    &ActionCancellation::default(),
                )
            })
        })
        .collect::<Vec<_>>();
    let results = workers
        .into_iter()
        .map(|w| w.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(results.iter().filter(|r| r.is_err()).count(), 1);
    let target = store(temp.path(), "cancel-target");
    struct CancelBeforeCommit;
    impl crate::persistence::AcceptanceArbiter for CancelBeforeCommit {
        fn before_durable_acceptance(
            &self,
            _commit: impl FnOnce() -> std::result::Result<(), crate::persistence::PersistenceError>,
        ) -> crate::persistence::AcceptanceCommitResult {
            crate::persistence::AcceptanceCommitResult::Cancelled
        }
    }
    let p = PactrunPersistence::open(&target).unwrap();
    let candidate = acquire(
        &s1,
        p.staging_session().unwrap(),
        &ActionCancellation::default(),
    )
    .unwrap();
    let publications = candidate
        .blobs
        .iter()
        .map(|(digest, blob)| {
            p.put_runtime_content(digest, &mut blob.bytes.try_clone_reader().unwrap())
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(
        p.persist_pack_revision(
            candidate.identity.package_id,
            &candidate.content,
            &publications,
            Some(&RevisionMetadataMutationBatch::new([]).unwrap()),
            Some(PackMetadataConflict::Reject),
            &InstallNames::default(),
            &CancelBeforeCommit
        )
        .is_err()
    );
    let db = rusqlite::Connection::open(target.join("database/pactrun.sqlite3")).unwrap();
    for table in ["packages", "revisions", "revision_reference_label_bindings"] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

// Test-ID: PR-TEST-0544
// Verifies: PR-REQ-0084, PR-REQ-0354, PR-REQ-0356
#[test]
fn pack_metadata_dto_roundtrips_every_portable_variant() {
    let name = PublisherName::parse("publisher").unwrap();
    let namespace = PublisherNamespace::parse("scope").unwrap();
    let uri = SourceUri::parse("https://example.test/source").unwrap();
    let mut metadata = PortableMetadataTemplate::default();
    let action = ActionIdentity::parse("run").unwrap();
    let parameter = ParameterIdentity::parse("option").unwrap();
    let targets = [
        PresentationTargetV1::Revision,
        PresentationTargetV1::Input(InputIdentity::parse("input").unwrap()),
        PresentationTargetV1::Action(action.clone()),
        PresentationTargetV1::ActionParameter {
            action: action.clone(),
            parameter: parameter.clone(),
        },
        PresentationTargetV1::ManagedOutput {
            action,
            output: ManagedOutputIdentity::parse("result").unwrap(),
        },
        PresentationTargetV1::SnapshotCapture,
        PresentationTargetV1::SnapshotCaptureParameter(parameter.clone()),
        PresentationTargetV1::SnapshotRestore,
        PresentationTargetV1::SnapshotRestoreParameter(parameter),
        PresentationTargetV1::MigrationEdge(RevisionContentDigest::from_bytes([1; 32])),
        PresentationTargetV1::Cleanup,
    ];
    for target in targets {
        for field in PresentationField::ALL {
            if target == PresentationTargetV1::Revision && field == PresentationField::DisplayName {
                continue;
            }
            metadata.presentation.push(PortablePresentationTemplate {
                target: target.clone(),
                field,
                value: PresentationValue::parse("description \u{4e2d}").unwrap(),
            });
        }
    }
    metadata.provenance = vec![
        ProvenanceClaim::SourceUri(uri.clone()),
        ProvenanceClaim::PublisherAttribution {
            publisher: name,
            namespace: Some(namespace),
            source_uri: Some(uri.clone()),
        },
        ProvenanceClaim::Attribution {
            text: AttributionText::parse("credit").unwrap(),
            source_uri: Some(uri),
        },
    ];
    metadata.provenance.sort();
    let encoded = serde_json::to_vec(&crate::pack_metadata::Metadata::encode(&metadata)).unwrap();
    let decoded: crate::pack_metadata::Metadata = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded.decode().unwrap(), metadata);
}

#[test]
fn pack_process_worker() {
    let Some(mode) = std::env::var_os("PACTRUN_PACK_TEST_MODE") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("PACTRUN_PACK_TEST_ROOT").unwrap());
    let input = PathBuf::from(std::env::var_os("PACTRUN_PACK_TEST_INPUT").unwrap());
    let app = PactrunApplication::open(root).unwrap();
    if mode == "install" {
        install(&app, &input);
    } else {
        let revision = std::env::var("PACTRUN_PACK_TEST_ID").unwrap();
        let (package, digest) = revision.split_once('/').unwrap();
        app.export_revision_pack(
            &RevisionIdentity::new(package.parse().unwrap(), digest.parse().unwrap()),
            &input,
            true,
            &ActionCancellation::default(),
        )
        .unwrap();
    }
}
fn worker(root: &Path, input: &Path, mode: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "--exact",
        "pack_transport::tests::pack_process_worker",
        "--nocapture",
    ])
    .env("PACTRUN_PACK_TEST_MODE", mode)
    .env("PACTRUN_PACK_TEST_ROOT", root)
    .env("PACTRUN_PACK_TEST_INPUT", input);
    cmd
}

// Test-ID: PR-TEST-0546
// Verifies: PR-REQ-0355, PR-REQ-0356
#[test]
fn pack_process_loss_preserves_atomic_revision_and_metadata_publication() {
    let temp = roots();
    let origin = store(temp.path(), "origin");
    let source = source(temp.path(), 1);
    let app = PactrunApplication::open(&origin).unwrap();
    let id = install(&app, &source).revision;
    let pack = temp.path().join("portable.pack");
    app.export_revision_pack(&id, &pack, true, &ActionCancellation::default())
        .unwrap();
    for point in ["before_revision_commit", "after_revision_commit"] {
        let target = store(temp.path(), point);
        drop(PactrunApplication::open(&target).unwrap());
        let status = worker(&target, &pack, "install")
            .env("PACTRUN_LIFECYCLE_FAULT", point)
            .output()
            .unwrap();
        assert_eq!(
            status.status.code(),
            Some(87),
            "{}",
            String::from_utf8_lossy(&status.stderr)
        );
        let p = PactrunPersistence::open(&target).unwrap();
        let stored = p.load_revision(&id).unwrap();
        assert_eq!(stored.is_some(), point == "after_revision_commit");
        if stored.is_some() {
            assert_eq!(p.load_revision_metadata(&id).unwrap().items.len(), 2);
        }
        drop(p);
        assert!(
            worker(&target, &pack, "install")
                .output()
                .unwrap()
                .status
                .success()
        );
        let p = PactrunPersistence::open(&target).unwrap();
        assert_eq!(p.load_revision_metadata(&id).unwrap().items.len(), 2);
    }
}

// Test-ID: PR-TEST-0547
// Verifies: PR-REQ-0356
#[test]
fn export_serializes_with_revision_deletion_and_collection() {
    let temp = roots();
    let root = store(temp.path(), "store");
    let source = source(temp.path(), 1);
    let app = PactrunApplication::open(&root).unwrap();
    let id = install(&app, &source).revision;
    drop(app);
    let sync = temp.path().join("sync");
    fs::create_dir(&sync).unwrap();
    let output = temp.path().join("export.pack");
    let mut child = worker(&root, &output, "export")
        .env(
            "PACTRUN_PACK_TEST_ID",
            format!("{}/{}", id.package_id, id.content_digest),
        )
        .env("PACTRUN_LIFECYCLE_SYNC", "after_pack_export_read")
        .env("PACTRUN_LIFECYCLE_SYNC_DIR", &sync)
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !sync.join("ready").exists() {
        if child.try_wait().unwrap().is_some() {
            panic!("export exited before establishing read");
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("export acquisition timed out");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let p = PactrunPersistence::open_read_only(&root).unwrap();
    assert!(p.load_revision(&id).unwrap().is_some());
    drop(p);
    let other = root.clone();
    let revision = id.clone();
    let (send, receive) = std::sync::mpsc::channel();
    let deletion = std::thread::spawn(move || {
        let app = PactrunApplication::open(&other).unwrap();
        let result = app.delete_object(&ObjectDeletion::Revision(revision));
        send.send(result).unwrap();
    });
    assert!(
        receive
            .recv_timeout(std::time::Duration::from_millis(150))
            .is_err()
    );
    fs::write(sync.join("release"), b"go").unwrap();
    assert!(child.wait().unwrap().success());
    let deleted = receive
        .recv_timeout(std::time::Duration::from_secs(30))
        .unwrap();
    assert!(deleted.is_ok());
    deletion.join().unwrap();
    PactrunApplication::open_for_collection(&root)
        .unwrap()
        .collect_content(true)
        .unwrap();
    let target = store(temp.path(), "target");
    assert_eq!(
        install(&PactrunApplication::open(&target).unwrap(), &output).revision,
        id
    );
}

// Test-ID: PR-TEST-0548
// Verifies: PR-REQ-0354, PR-REQ-0356
#[test]
#[ignore = "explicit capacity acceptance: streams more than 512 MiB and uses multiple GiB of isolated test disk"]
fn pack_roundtrip_exceeds_managed_input_limit_without_a_runtime_quota() {
    let temp = roots();
    let root = store(temp.path(), "origin");
    let source = source(temp.path(), 1);
    File::create(source.join("data.bin"))
        .unwrap()
        .set_len(MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1 + 1)
        .unwrap();
    let app = PactrunApplication::open(&root).unwrap();
    let id = install(&app, &source).revision;
    let output = temp.path().join("large.pack");
    app.export_revision_pack(&id, &output, false, &ActionCancellation::default())
        .unwrap();
    assert!(fs::metadata(&output).unwrap().len() < MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1);
    let target = store(temp.path(), "target");
    let other = PactrunApplication::open(&target).unwrap();
    assert_eq!(install(&other, &output).revision, id);
    let again = temp.path().join("again.pack");
    other
        .export_revision_pack(&id, &again, false, &ActionCancellation::default())
        .unwrap();
}

// Test-ID: PR-TEST-0549
// Verifies: PR-REQ-0356
#[test]
fn pack_cli_output_failure_reports_publication_truth() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let temp = roots();
    let root = store(temp.path(), "store");
    let source = source(temp.path(), 1);
    let app = PactrunApplication::open(&root).unwrap();
    let id = install(&app, &source).revision;
    drop(app);
    let output = temp.path().join("published.pack");
    let output_base = temp.path().join("published");
    let reference = format!("exact:{}/{}", id.package_id, id.content_digest);
    let args = vec![
        "revision".into(),
        "export".into(),
        reference.into(),
        "--output".into(),
        output_base.into_os_string(),
    ];
    let mut error = Vec::new();
    assert_eq!(
        crate::cli::run(
            args,
            Some(root.as_os_str().to_owned()),
            &mut io::empty(),
            &mut Broken,
            &mut error
        ),
        1
    );
    assert!(output.exists());
    assert!(
        String::from_utf8(error)
            .unwrap()
            .contains("destination was published")
    );
    let failed = temp.path().join("not-published.pack");
    let failed_base = temp.path().join("not-published");
    let args = vec![
        "revision".into(),
        "export".into(),
        format!("exact:{}/{}", id.package_id, id.content_digest).into(),
        "--output".into(),
        failed_base.into_os_string(),
    ];
    assert_eq!(
        crate::cli::run(
            args,
            Some(root.as_os_str().to_owned()),
            &mut io::empty(),
            &mut Vec::new(),
            &mut Broken
        ),
        1
    );
    assert!(
        failed.exists(),
        "warning delivery failure must not cancel publication"
    );
}

// Supporting coverage for PR-TEST-0542: real ZIP64 entry count, not a mocked size.
#[test]
fn pack_zip64_entry_count_is_bounded_before_payload_acquisition() {
    let temp = roots();
    let path = temp.path().join("many.pack");
    let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
    for index in 0..65_536 {
        writer
            .start_file(
                format!("entry-{index}"),
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
    }
    writer.finish().unwrap();
    let (entries, _) = crate::pack_zip::preflight(&mut File::open(&path).unwrap()).unwrap();
    assert_eq!(entries.len(), 65_536);
}

// Supporting coverage for PR-TEST-0542: streaming ZIP data descriptors.
#[test]
fn pack_streaming_zip_descriptors_preserve_separate_compressed_lengths() {
    let temp = roots();
    let root = store(temp.path(), "store");
    let session = StagingSession::prepare(&root).unwrap();
    let yaml=b"source_format: 1.0-alpha.2\npackage_id: 00000000000000000000000000000011\nrevision: {}\nruntime_content: {}\n";
    for method in [
        zip::CompressionMethod::Stored,
        zip::CompressionMethod::Deflated,
    ] {
        let path = temp.path().join("streamed.pack");
        zip_files(
            &path,
            &[("pactrun.yaml".into(), yaml.to_vec())],
            method,
            false,
        );
        let mut bytes = fs::read(&path).unwrap();
        let central = bytes.windows(4).position(|s| s == b"PK\x01\x02").unwrap();
        let mut descriptor = b"PK\x07\x08".to_vec();
        descriptor.extend_from_slice(&bytes[central + 16..central + 28]);
        bytes[6] |= 8;
        bytes[14..26].fill(0);
        bytes[central + 8] |= 8;
        bytes.splice(central..central, descriptor);
        let end = bytes.len() - 22;
        bytes[end + 16..end + 20].copy_from_slice(&((central + 16) as u32).to_le_bytes());
        fs::write(&path, &bytes).unwrap();
        assert!(acquire(&path, &session, &ActionCancellation::default()).is_ok());
        bytes[central + 4] ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(acquire(&path, &session, &ActionCancellation::default()).is_err());
    }
}

// Supporting coverage for PR-TEST-0542: reject manifest bombs before decompression.
#[test]
fn pack_manifest_sizes_and_implicit_directory_aliases_are_preflighted() {
    let temp = roots();
    let root = store(temp.path(), "store");
    let session = StagingSession::prepare(&root).unwrap();
    let path = temp.path().join("hostile.pack");
    let manifest=b"source_format: 1.0-alpha.2\npackage_id: 00000000000000000000000000000011\nrevision: {}\nruntime_content: {}\n".to_vec();
    zip_files(
        &path,
        &[("pactrun.yaml".into(), manifest.clone())],
        zip::CompressionMethod::Deflated,
        false,
    );
    let mut bytes = fs::read(&path).unwrap();
    let central = bytes.windows(4).position(|s| s == b"PK\x01\x02").unwrap();
    bytes[22..26].copy_from_slice(&((JSON_LIMIT + 1) as u32).to_le_bytes());
    bytes[central + 24..central + 28].copy_from_slice(&((JSON_LIMIT + 1) as u32).to_le_bytes());
    fs::write(&path, bytes).unwrap();
    assert!(crate::pack_zip::preflight(&mut File::open(&path).unwrap()).is_ok());
    let failure = acquire(&path, &session, &ActionCancellation::default())
        .err()
        .unwrap();
    assert!(failure.contains("before decompression"), "{failure}");
    zip_files(
        &path,
        &[
            ("pactrun.yaml".into(), manifest),
            ("parent/a".into(), vec![]),
            ("PARENT/b".into(), vec![]),
        ],
        zip::CompressionMethod::Stored,
        false,
    );
    assert!(
        acquire(&path, &session, &ActionCancellation::default())
            .err()
            .unwrap()
            .contains("ancestor")
    );
}
