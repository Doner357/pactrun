use super::*;
use crate::cli::{self, Command};
use std::{
    fs,
    io::{self, Write},
};

// Test-ID: PR-TEST-0550
// Verifies: PR-REQ-0357
#[test]
fn envelope_export_paths_append_without_suffix_detection_or_native_loss() {
    for (base, pack, snapshot) in [
        ("rv", "rv.pack", "rv.snapshot"),
        ("rv.pack", "rv.pack.pack", "rv.pack.snapshot"),
        ("rv.snapshot", "rv.snapshot.pack", "rv.snapshot.snapshot"),
        (
            "dir/archive.tar.gz",
            "dir/archive.tar.gz.pack",
            "dir/archive.tar.gz.snapshot",
        ),
        ("dir/rv.PACK", "dir/rv.PACK.pack", "dir/rv.PACK.snapshot"),
        (
            "dir/space \u{8cc7}\u{6599}",
            "dir/space \u{8cc7}\u{6599}.pack",
            "dir/space \u{8cc7}\u{6599}.snapshot",
        ),
    ] {
        assert_eq!(
            destination(base.into(), Format::Pack).unwrap(),
            Path::new(pack)
        );
        assert_eq!(
            destination(base.into(), Format::Snapshot).unwrap(),
            Path::new(snapshot)
        );
    }
    for bad in ["", "-", ".", "..", "/", "dir/", "dir/.", "dir/.."] {
        assert!(destination(bad.into(), Format::Pack).is_err(), "{bad}");
        assert!(destination(bad.into(), Format::Snapshot).is_err(), "{bad}");
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        for bad in ["C:\\", "dir\\", "dir\\.", "dir\\.."] {
            assert!(destination(bad.into(), Format::Pack).is_err());
        }
        let raw = [b'x' as u16, 0xd800];
        let base = OsString::from_wide(&raw);
        let expected = raw
            .into_iter()
            .chain(".pack".encode_utf16())
            .collect::<Vec<_>>();
        assert_eq!(
            destination(base, Format::Pack)
                .unwrap()
                .as_os_str()
                .encode_wide()
                .collect::<Vec<_>>(),
            expected
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let base = OsString::from_vec(vec![b'x', 0xff]);
        assert_eq!(
            destination(base, Format::Pack)
                .unwrap()
                .as_os_str()
                .as_bytes(),
            b"x\xff.pack"
        );
    }
}

const ID: &str = "11111111111111111111111111111111";
fn run(root: &Path, args: &[OsString]) -> (i32, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let status = cli::run(
        args.to_vec(),
        Some(root.as_os_str().to_owned()),
        &mut io::empty(),
        &mut stdout,
        &mut stderr,
    );
    (
        status,
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

// Test-ID: PR-TEST-0551
// Verifies: PR-REQ-0357
#[test]
fn revision_and_snapshot_exports_use_final_names_for_publication_and_import() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/export-naming-tests");
    fs::create_dir_all(&parent).unwrap();
    let temp = tempfile::tempdir_in(parent).unwrap();
    let root = temp.path().join("store");
    for child in ["database", "runtime-content", "staging"] {
        fs::create_dir_all(root.join(child)).unwrap();
    }
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(
        source.join("pactrun.yaml"),
        format!(
            "source_format: 1.0-alpha.1\npackage_id: {ID}\nrevision: {{}}\nruntime_content: {{}}\n"
        ),
    )
    .unwrap();
    let (code, reference, error) = run(
        &root,
        &["pack".into(), "install".into(), source.into_os_string()],
    );
    assert_eq!(code, 0, "{error}");
    let reference = reference.trim();
    let manifest = serde_json::json!({"format_version":"1.0-alpha.1","snapshot_id":ID,"producer":{"package_id":ID,"revision_content_digest":format!("sha256:{}","0".repeat(64))},"origin_instance_id":ID,"captured_at":{"unix_seconds":0,"nanoseconds":0},"managed_bindings":[],"service_content":[]});
    let verified = crate::snapshot_integrity::decode_snapshot_manifest(
        crate::domain::SnapshotIntegrityVersion::BASELINE,
        &serde_json::to_vec(&manifest).unwrap(),
        None,
    )
    .unwrap();
    let legacy = temp.path().join("legacy.zip");
    let mut writer = zip::ZipWriter::new(fs::File::create(&legacy).unwrap());
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("bundle.json", options).unwrap();
    writer.write_all(&serde_json::to_vec(&serde_json::json!({"kind":"pactrun_snapshot_bundle","bundle_version":"1.0-alpha.1","integrity_format":"1.0-alpha.1","integrity_digest":verified.integrity_digest().as_str()})).unwrap()).unwrap();
    writer.start_file("manifest.json", options).unwrap();
    writer.write_all(verified.canonical_bytes()).unwrap();
    writer.finish().unwrap();
    let (code, _, error) = run(
        &root,
        &["snapshot".into(), "import".into(), legacy.into_os_string()],
    );
    assert_eq!(code, 0, "{error}");
    for (kind, id, suffix) in [
        ("revision", reference, "pack"),
        ("snapshot", ID, "snapshot"),
    ] {
        let base = temp.path().join(kind);
        fs::write(
            &base,
            b"base must not be overwritten or treated as destination",
        )
        .unwrap();
        for input in [
            base.clone(),
            base.with_extension(suffix),
            temp.path().join(format!("{kind}.other")),
        ] {
            let mut name = input.as_os_str().to_owned();
            name.push(format!(".{suffix}"));
            let output = PathBuf::from(name);
            let mut args = vec![
                kind.into(),
                "export".into(),
                id.into(),
                "--output".into(),
                input.as_os_str().to_owned(),
            ];
            if kind == "snapshot" {
                let (code, _, _) = run(&root, &args);
                assert_eq!(code, 1);
                assert!(!output.exists());
                args.push("--authorize-sensitive-export".into());
            }
            let (code, text, error) = run(&root, &args);
            assert_eq!(code, 0, "{error}");
            assert!(output.exists());
            assert!(
                text.contains(&format!("output: {}", cli::format_path(&output))),
                "{text}"
            );
            let bytes = fs::read(&output).unwrap();
            let (code, _, _) = run(&root, &args);
            assert_eq!(code, 1);
            assert_eq!(fs::read(&output).unwrap(), bytes);
            let import = if kind == "revision" {
                vec!["pack".into(), "install".into(), output.into_os_string()]
            } else {
                vec!["snapshot".into(), "import".into(), output.into_os_string()]
            };
            let (code, _, error) = run(&root, &import);
            assert_eq!(code, 0, "{error}");
        }
        assert_eq!(
            fs::read(base).unwrap(),
            b"base must not be overwritten or treated as destination"
        );
        let missing = temp.path().join("missing-parent").join(kind);
        let mut args = vec![
            kind.into(),
            "export".into(),
            id.into(),
            "--output".into(),
            missing.into_os_string(),
        ];
        if kind == "snapshot" {
            args.push("--authorize-sensitive-export".into());
        }
        assert_eq!(run(&root, &args).0, 1);
        assert!(!temp.path().join("missing-parent").exists());
    }
}

// Test-ID: PR-TEST-0552
// Verifies: PR-REQ-0357
#[test]
fn raw_exports_and_import_paths_do_not_acquire_envelope_suffixes() {
    for path in ["report.pdf", "raw.pack", "-"] {
        let args = ["input", "export", "sample", "config", "--output", path]
            .map(OsString::from)
            .to_vec();
        let Command::ExportInput { output, .. } = cli::parse_command(args).unwrap() else {
            panic!("expected Input export")
        };
        assert_eq!(output, path);
    }
    let args = [
        "run",
        "artifact",
        "export",
        ID,
        "report",
        "--output",
        "report.pdf",
        "--authorize-sensitive-export",
    ]
    .map(OsString::from)
    .to_vec();
    let Command::Artifact(cli::artifacts::ArtifactCommand::Export { destination, .. }) =
        cli::parse_command(args).unwrap()
    else {
        panic!("expected Artifact export")
    };
    assert_eq!(destination, Path::new("report.pdf"));
    for path in ["legacy.zip", "backup.snapshot", "custom.extension"] {
        let args = ["snapshot", "import", path].map(OsString::from).to_vec();
        let Command::Snapshot(cli::snapshots::SnapshotCommand::Import(input)) =
            cli::parse_command(args).unwrap()
        else {
            panic!("expected Snapshot import")
        };
        assert_eq!(input, Path::new(path));
    }
}
