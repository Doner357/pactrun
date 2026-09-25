//! Actual-process coverage for the unified Pack interface.
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
fn call(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pactrun"))
        .env("PACTRUN_STORAGE_ROOT", root)
        .args(args)
        .output()
        .unwrap()
}
fn ok(root: &Path, args: &[&str]) -> String {
    let output = call(root, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

// Test-ID: PR-TEST-0545
// Verifies: PR-REQ-0035, PR-REQ-0118, PR-REQ-0354, PR-REQ-0355, PR-REQ-0356
#[test]
fn unified_pack_cli_exports_imports_and_reports_metadata_conflicts() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/pack-process-tests");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    for root in [&a, &b] {
        for part in ["database", "runtime-content", "staging"] {
            fs::create_dir_all(root.join(part)).unwrap();
        }
    }
    let yaml = "source_format: 1\npackage_id: 00000000000000000000000000000011\nrevision: {}\nruntime_content: {}\nportable_metadata:\n  presentation: [{target: {kind: revision}, field: display_name, value: Published}]\n";
    fs::write(source.join("pactrun.yaml"), yaml).unwrap();
    let identity = ok(&a, &["pack", "install", source.to_str().unwrap()]);
    let reference = identity.trim().to_owned();
    let exported = temp.path().join("portable.pack");
    let export_base = temp.path().join("portable");
    let output = call(
        &a,
        &[
            "revision",
            "export",
            &reference,
            "--output",
            export_base.to_str().unwrap(),
            "--include-portable-metadata",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("unencrypted"));
    assert_eq!(
        ok(&b, &["pack", "install", exported.to_str().unwrap()]),
        identity
    );
    fs::write(
        source.join("pactrun.yaml"),
        yaml.replace("Published", "Local"),
    )
    .unwrap();
    let args = ["pack", "install", source.to_str().unwrap()];
    let rejected = call(&b, &args);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("--metadata-conflict"));
    let kept = ok(
        &b,
        &[
            "pack",
            "install",
            source.to_str().unwrap(),
            "--metadata-conflict",
            "keep",
        ],
    );
    assert!(kept.contains("not applied"));
    ok(
        &b,
        &[
            "pack",
            "install",
            source.to_str().unwrap(),
            "--metadata-conflict",
            "overwrite",
        ],
    );
    assert!(ok(&b, &["revision", "metadata", "show", &reference]).contains("Local"));
    let sentinel = fs::read(&exported).unwrap();
    assert!(
        !call(
            &a,
            &[
                "revision",
                "export",
                &reference,
                "--output",
                export_base.to_str().unwrap()
            ]
        )
        .status
        .success()
    );
    assert_eq!(fs::read(&exported).unwrap(), sentinel);
    assert!(
        !call(
            &b,
            &[
                "pack",
                "install",
                source.to_str().unwrap(),
                "--metadata-conflict",
                "invalid"
            ]
        )
        .status
        .success()
    );
}
