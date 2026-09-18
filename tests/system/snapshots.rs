//! M4 journeys use a fresh executable for every public product operation.
use super::support::{Scenario, assert_exit, assert_success, first_line, instance_projection};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{self, Read, Write},
    path::Path,
    process::Output,
    thread,
    time::{Duration, Instant},
};

const SECRET: &[u8] = b"m4-managed-secret-bytes";
const SERVICE: &[u8] = b"m4-captured-service-bytes";
const PRIVATE: &str = "m4-sensitive-parameter";
const PREAMBLE: &[u8] = b"pactrun.hook-protocol\0\0\0\0\x01";

fn source(access: &str) -> String {
    let mut text = String::from(
        "source_format: 1\npackage_id: {package_id}\nrevision:\n  inputs:\n    - { id: secret, required: true, protection: secret }\n    - { id: optional, required: false, protection: normal }\n  actions: []\n  snapshot:\n",
    );
    for operation in ["capture", "restore"] {
        text.push_str(&format!("    {operation}:\n"));
        if operation == "capture" {
            text.push_str(&format!("      access: {access}\n"));
        }
        text.push_str(
            r#"      parameters:
        - { id: mode, type: string, sensitive: false, default: success }
        - { id: marker, type: string, sensitive: false, default: '{hook_marker}' }
        - { id: secret_text, type: string, sensitive: true, default: m4-sensitive-parameter }
        - { id: enabled, type: boolean, sensitive: false, default: true }
      hook:
        protocol_version: 1
        launch: { kind: direct, executable: worker }
        args: ["--exact", "snapshots::snapshot_hook_worker", "--nocapture"]
        io: { terminal: none }
"#,
        );
    }
    text.push_str("  migrations: []\nruntime_content:\n  files:\n    - { id: worker, source: '{worker_name}', path: 'bin/{worker_name}', executable: true }\n");
    text
}
fn initialize(s: &Scenario) -> String {
    let revision = s.install_and_create("service");
    assert_success(&s.run_with_stdin(
        ["input", "set", "service", "secret", "--stdin"],
        SECRET.to_vec(),
    ));
    revision
}
fn field(output: &Output, name: &str) -> String {
    let prefix = format!("{name}: ");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|l| l.strip_prefix(&prefix))
        .or_else(|| {
            std::str::from_utf8(&output.stderr)
                .ok()?
                .lines()
                .find_map(|l| l.strip_prefix(&prefix))
        })
        .unwrap_or_else(|| panic!("missing {name}: {output:?}"))
        .to_owned()
}
fn safe(output: &Output) {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for value in [
        String::from_utf8_lossy(SECRET).into_owned(),
        String::from_utf8_lossy(SERVICE).into_owned(),
        PRIVATE.to_owned(),
        hex::encode(Sha256::digest(SECRET)),
        hex::encode(Sha256::digest(SERVICE)),
    ] {
        assert!(!text.contains(&value), "private value in ordinary output");
    }
    for field in [
        "blob_digest:",
        "byte_length:",
        "integrity_digest:",
        "raw_manifest:",
    ] {
        assert!(!text.contains(field));
    }
}
fn capture(s: &Scenario) -> String {
    let output = s.run(["snapshot", "capture", "service"]);
    assert_success(&output);
    safe(&output);
    field(&output, "snapshot")
}

// Test-ID: PR-TEST-0476
// Verifies: PR-REQ-0346
#[test]
fn create_and_restore_matches_two_operations_and_preserves_partial_completion() {
    let s = Scenario::new(0x91, &source("observe"));
    let revision = initialize(&s);
    let id = capture(&s);
    let combined = s.run_with_stdin(
        [
            "instance",
            "create",
            "combined",
            "--revision",
            &revision,
            "--restore-from",
            &id,
            "--param-stdin",
            "enabled",
        ],
        b"true".to_vec(),
    );
    assert_success(&combined);
    safe(&combined);
    assert_success(&s.create_instance("separate", &revision));
    assert_success(&s.run(["snapshot", "restore", "separate", &id]));
    for name in ["combined", "separate"] {
        assert_eq!(
            s.run([
                "input",
                "export",
                name,
                "secret",
                "--output",
                "-",
                "--authorize-secret-export"
            ])
            .stdout,
            SECRET
        );
        assert_eq!(run_count(&s.run(["run", "list", name])), 1);
    }
    let before = s.hook_launches();
    let duplicate = s.run([
        "instance",
        "create",
        "combined",
        "--revision",
        &revision,
        "--restore-from",
        &id,
    ]);
    assert_exit(&duplicate, 1);
    assert_eq!(s.hook_launches(), before);
    assert_eq!(run_count(&s.run(["run", "list", "combined"])), 1);
    let failed = s.run([
        "instance",
        "create",
        "partial",
        "--revision",
        &revision,
        "--restore-from",
        &id,
        "--param",
        "mode=failure",
    ]);
    assert_exit(&failed, 1);
    safe(&failed);
    assert!(String::from_utf8_lossy(&failed.stderr).contains("partial_completion:"));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("restore_run:"));
    assert_success(&s.run(["instance", "show", "partial"]));
    assert_eq!(run_count(&s.run(["run", "list", "partial"])), 1);
    assert_exit(
        &s.run([
            "input",
            "export",
            "partial",
            "secret",
            "--output",
            "-",
            "--authorize-secret-export",
        ]),
        1,
    );
}

// Test-ID: PR-TEST-0477
// Verifies: PR-REQ-0346
#[test]
fn create_and_restore_rejects_invalid_preflight_and_options_without_creating() {
    let s = Scenario::new(0x92, &source("observe"));
    let revision = initialize(&s);
    let id = capture(&s);
    let original = s.run(["instance", "list"]).stdout;
    let before = s.hook_launches();
    for (tail, code) in [
        (
            vec!["--restore-from", "ffffffffffffffffffffffffffffffff"],
            1,
        ),
        (vec!["--restore-from", &id, "--param", "enabled=invalid"], 1),
        (vec!["--restore-from", &id, "--param", "unknown=value"], 2),
        (vec!["--restore-from", &id, "--plan"], 2),
        (
            vec!["--restore-from", &id, "--input-file", "secret=not-opened"],
            2,
        ),
        (vec!["--restore-from", &id, "--input-stdin", "secret"], 2),
        (vec!["--restore-from", &id, "--restore-from", &id], 2),
        (vec!["--param", "mode=success"], 2),
        (vec!["--execution-timeout-ms", "1"], 2),
    ] {
        let mut args = vec!["instance", "create", "rejected", "--revision", &revision];
        args.extend(tail);
        let result = s.run(args);
        assert_exit(&result, code);
        safe(&result);
        assert_eq!(s.run(["instance", "list"]).stdout, original);
    }
    assert_exit(
        &s.run(["instance", "create", "rejected", "--restore-from", &id]),
        2,
    );
    let other = Scenario::new(0x93, &source("observe"));
    let other_revision = other.install();
    let bundle = s.path("preflight.zip");
    assert_success(&export(&s, &id, &bundle));
    assert_success(&other.run(["snapshot", "import", bundle.to_str().unwrap()]));
    assert_exit(
        &other.run([
            "instance",
            "create",
            "rejected",
            "--revision",
            &other_revision,
            "--restore-from",
            &id,
        ]),
        1,
    );
    assert!(other.run(["instance", "list"]).stdout.is_empty());
    assert_eq!(s.hook_launches(), before);
}

// Supporting preflight and cancellation coverage for PR-TEST-0476/0477.
#[test]
fn create_restore_missing_capability_and_timeout_keep_the_documented_boundaries() {
    let mut text = source("observe");
    let start = text.find("    restore:\n").unwrap();
    let end = text[start..].find("  migrations:").unwrap() + start;
    text.replace_range(start..end, "");
    let s = Scenario::new(0x95, &text);
    let revision = s.install();
    let bundle = s.path("no-restore.zip");
    let id = v1_bundle(&bundle, &revision);
    assert_success(&s.run(["snapshot", "import", bundle.to_str().unwrap()]));
    assert_exit(
        &s.run([
            "instance",
            "create",
            "rejected",
            "--revision",
            &revision,
            "--restore-from",
            &id,
        ]),
        1,
    );
    assert!(s.run(["instance", "list"]).stdout.is_empty());
    assert_eq!(s.hook_launches(), 0);

    let s = Scenario::new(0x96, &source("observe"));
    let revision = initialize(&s);
    let id = capture(&s);
    let failed = s.run([
        "instance",
        "create",
        "timed-out",
        "--revision",
        &revision,
        "--restore-from",
        &id,
        "--execution-timeout-ms",
        "0",
    ]);
    assert_exit(&failed, 1);
    safe(&failed);
    assert!(String::from_utf8_lossy(&failed.stderr).contains("partial_completion:"));
    assert_success(&s.run(["instance", "show", "timed-out"]));
    assert_eq!(run_count(&s.run(["run", "list", "timed-out"])), 1);
}
fn bind(s: &Scenario, bytes: &[u8]) {
    assert_success(&s.run_with_stdin(
        ["input", "set", "service", "secret", "--stdin"],
        bytes.to_vec(),
    ));
}

// Test-ID: PR-TEST-0484
// Verifies: PR-REQ-0347, PR-REQ-0341, PR-REQ-0343
#[test]
fn snapshot_deletion_and_gc_preserve_restored_input_value_until_its_owner_retires() {
    let s = Scenario::new(0x97, &source("observe"));
    let revision = initialize(&s);
    let id = capture(&s);
    assert_success(&s.run([
        "instance",
        "create",
        "restored",
        "--revision",
        &revision,
        "--restore-from",
        &id,
    ]));
    assert_success(&s.run(["snapshot", "delete", &id]));
    assert_success(&s.run(["storage", "gc"]));
    let value = s.run([
        "input",
        "export",
        "restored",
        "secret",
        "--output",
        "-",
        "--authorize-secret-export",
    ]);
    assert_success(&value);
    assert_eq!(value.stdout, SECRET);
    for name in ["restored", "service"] {
        assert_success(&s.run(["instance", "abandon", name]));
    }
    let collected = s.run(["storage", "gc"]);
    assert_success(&collected);
    assert!(String::from_utf8_lossy(&collected.stdout).contains("removed=1"));
}
fn run_count(output: &Output) -> usize {
    assert_success(output);
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.starts_with("run: "))
        .count()
}
fn manifest(path: &Path) -> Vec<u8> {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let mut out = Vec::new();
    archive
        .by_name("manifest.json")
        .unwrap()
        .read_to_end(&mut out)
        .unwrap();
    out
}
fn export(s: &Scenario, id: &str, path: &Path) -> Output {
    s.run([
        "snapshot",
        "export",
        id,
        "--output",
        path.to_str().unwrap(),
        "--authorize-sensitive-export",
    ])
}

// Test-ID: PR-TEST-0268
// Verifies: PR-REQ-0099, PR-REQ-0102, PR-REQ-0104, PR-REQ-0117, PR-REQ-0151, PR-REQ-0152, PR-REQ-0213, PR-REQ-0291, PR-REQ-0301, PR-REQ-0302
#[test]
fn public_v2_journey_preserves_identity_and_restores_real_service_content_across_isolated_roots() {
    let a = Scenario::new(0x70, &source("observe"));
    let revision = initialize(&a);
    let id = capture(&a);
    assert_eq!(a.hook_launches(), 1);
    for args in [
        vec!["snapshot", "show", &id],
        vec!["snapshot", "verify", &id],
        vec!["snapshot", "list", "--instance", "service"],
    ] {
        let out = a.run(args);
        assert_success(&out);
        safe(&out);
        assert!(String::from_utf8_lossy(&out.stdout).contains(&id));
    }
    let bundle = a.path("snapshot.zip");
    let refused = a.run([
        "snapshot",
        "export",
        &id,
        "--output",
        bundle.to_str().unwrap(),
    ]);
    assert_exit(&refused, 1);
    assert!(!bundle.exists());
    safe(&refused);
    let exported = export(&a, &id, &bundle);
    assert_success(&exported);
    safe(&exported);
    assert!(String::from_utf8_lossy(&exported.stderr).contains("unencrypted"));
    let canonical = manifest(&bundle);
    let refused = export(&a, &id, &bundle);
    assert_exit(&refused, 1);
    assert_eq!(manifest(&bundle), canonical);
    let b = Scenario::new(0x71, &source("mutate"));
    let imported = b.run(["snapshot", "import", bundle.to_str().unwrap()]);
    assert_success(&imported);
    safe(&imported);
    assert_eq!(field(&imported, "snapshot"), id);
    assert_eq!(field(&imported, "relational_verification"), "not_evaluated");
    let verified = b.run(["snapshot", "verify", &id]);
    assert_success(&verified);
    assert_eq!(field(&verified, "relational_verification"), "not_evaluated");
    let show = b.run(["snapshot", "show", &id]);
    safe(&show);
    assert_eq!(
        field(&show, "current_content_verification"),
        "not_performed"
    );
    assert_eq!(field(&show, "target_eligibility"), "target_not_specified");
    assert_success(&b.run(["snapshot", "import", bundle.to_str().unwrap()]));
    let installed = b.run(["pack", "install", a.source.to_str().unwrap()]);
    assert_success(&installed);
    assert_eq!(first_line(&installed.stdout), revision);
    assert_success(&b.create_instance("service", &revision));
    bind(&b, b"pre-restore-target");
    let before = instance_projection(&b.run(["instance", "show", "service"]));
    let restored = b.run(["snapshot", "restore", "service", &id]);
    assert_success(&restored);
    safe(&restored);
    let run = field(&restored, "run");
    let details = b.run(["run", "show", &run]);
    assert_success(&details);
    safe(&details);
    assert_eq!(field(&details, "operation"), "snapshot_restore");
    assert_eq!(field(&details, "source_snapshot"), id);
    assert_ne!(
        before.state_version,
        instance_projection(&b.run(["instance", "show", "service"])).state_version
    );
    assert_eq!(
        b.run([
            "input",
            "export",
            "service",
            "secret",
            "--output",
            "-",
            "--authorize-secret-export"
        ])
        .stdout,
        SECRET
    );
    assert_eq!(run_count(&b.run(["run", "list", "service"])), 1);
    assert!(
        b.run(["snapshot", "list", "--instance", "service"])
            .stdout
            .is_empty()
    );
    let roundtrip = b.path("roundtrip.zip");
    assert_success(&export(&b, &id, &roundtrip));
    assert_eq!(manifest(&roundtrip), canonical);
    let markers = fs::read_to_string(a.path("hook-launches")).unwrap();
    assert!(markers.contains("restore-content-verified"));
}

fn v1_bundle(path: &Path, revision: &str) -> String {
    let (package, digest) = revision
        .strip_prefix("exact:")
        .unwrap()
        .split_once('/')
        .unwrap();
    let id = "00000000000000000000000000000072";
    let secret = format!("sha256:{}", hex::encode(Sha256::digest(SECRET)));
    let service = format!("sha256:{}", hex::encode(Sha256::digest(SERVICE)));
    let manifest = json!({"format_version":1,"snapshot_id":id,"producer":{"package_id":package,"revision_content_digest":digest},"origin_instance_id":"00000000000000000000000000000073","captured_at":{"unix_seconds":0,"nanoseconds":0},"managed_bindings":[{"input_id":"optional","role":"active","state":"absent","protection":"normal"},{"input_id":"secret","role":"active","state":"bound","protection":"secret","blob_digest":secret}],"service_content":[{"role":"database","path":"state/main","blob_digest":service}]});
    let canonical = serde_jcs::to_vec(&manifest).unwrap();
    let mut hash = Sha256::new();
    hash.update(b"pactrun.snapshot-integrity-digest\0");
    hash.update(1u32.to_be_bytes());
    hash.update(b"snapshot-integrity-manifest\0");
    hash.update((canonical.len() as u64).to_be_bytes());
    hash.update(&canonical);
    let envelope = json!({"kind":"pactrun_snapshot_bundle","bundle_version":1,"integrity_format":1,"integrity_digest":format!("sha256:{}",hex::encode(hash.finalize()))});
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in [
        (
            "bundle.json".to_owned(),
            serde_json::to_vec(&envelope).unwrap(),
        ),
        ("manifest.json".to_owned(), canonical),
        (
            format!("blobs/sha256/{}", secret.trim_start_matches("sha256:")),
            SECRET.to_vec(),
        ),
        (
            format!("blobs/sha256/{}", service.trim_start_matches("sha256:")),
            SERVICE.to_vec(),
        ),
    ] {
        zip.start_file(name, options).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    id.to_owned()
}

// Test-ID: PR-TEST-0269
// Verifies: PR-REQ-0301, PR-REQ-0302, PR-REQ-0291, PR-REQ-0213
#[test]
fn public_v1_import_verify_export_and_restore_never_upgrade_the_snapshot_format() {
    let s = Scenario::new(0x72, &source("mutate"));
    let revision = initialize(&s);
    let input = s.path("v1.zip");
    let id = v1_bundle(&input, &revision);
    let original = manifest(&input);
    for args in [
        vec!["snapshot", "import", input.to_str().unwrap()],
        vec!["snapshot", "verify", &id],
        vec!["snapshot", "restore", "service", &id],
    ] {
        let output = s.run(args);
        assert_success(&output);
        safe(&output);
    }
    assert_eq!(
        field(&s.run(["snapshot", "show", &id]), "integrity_format"),
        "1"
    );
    let output = s.path("v1-out.zip");
    assert_success(&export(&s, &id, &output));
    assert_eq!(manifest(&output), original);
    assert!(
        fs::read_to_string(s.path("hook-launches"))
            .unwrap()
            .contains("restore-content-verified")
    );
}

// Test-ID: PR-TEST-0270
// Verifies: PR-REQ-0301, PR-REQ-0302, PR-REQ-0289, PR-REQ-0091, PR-REQ-0191
#[test]
fn snapshot_plans_access_readiness_parameters_and_parser_are_safe_and_read_only() {
    for (n, access) in [(0x73, "observe"), (0x74, "mutate")] {
        let s = Scenario::new(n, &source(access));
        let revision = s.install_and_create("service");
        let before = fs::read_dir(s.storage.join("staging")).unwrap().count();
        let incomplete = s.run(["snapshot", "capture", "service", "--plan"]);
        assert_exit(&incomplete, 1);
        safe(&incomplete);
        assert_eq!(s.hook_launches(), 0);
        assert_eq!(run_count(&s.run(["run", "list", "service"])), 0);
        assert_eq!(
            fs::read_dir(s.storage.join("staging")).unwrap().count(),
            before
        );
        bind(&s, b"");
        let before = fs::read_dir(s.storage.join("staging")).unwrap().count();
        let plan = s.run([
            "snapshot",
            "capture",
            "service",
            "--plan",
            "--execution-timeout-ms",
            "0",
        ]);
        assert_success(&plan);
        safe(&plan);
        assert_eq!(field(&plan, "access"), access);
        assert_eq!(field(&plan, "execution_timeout_ms"), "0");
        assert_eq!(field(&plan, "termination_grace_ms"), "5000");
        assert_eq!(s.hook_launches(), 0);
        assert_eq!(
            fs::read_dir(s.storage.join("staging")).unwrap().count(),
            before
        );
        for args in [
            vec!["snapshot", "capture", "service", "--access", "observe"],
            vec!["snapshot", "capture", "service", "--action-timeout-ms", "1"],
            vec![
                "snapshot",
                "capture",
                "service",
                "--execution-timeout-ms",
                "18446744073709551615",
            ],
            vec!["snapshot", "show", "abc"],
            vec!["snapshot", "import", "-"],
            vec![
                "snapshot",
                "export",
                "00000000000000000000000000000000",
                "--output",
                "-",
                "--authorize-sensitive-export",
            ],
            vec![
                "snapshot",
                "restore",
                "service",
                "00000000000000000000000000000000",
                "--force",
            ],
        ] {
            let output = s.run(args);
            assert_exit(&output, 2);
            safe(&output);
        }
        let assignment = format!("enabled={PRIVATE}");
        let output = s.run(["snapshot", "capture", "service", "--param", &assignment]);
        assert_exit(&output, 1);
        safe(&output);
        let assignment = format!("secret_text={PRIVATE}");
        let output = s.run([
            "snapshot",
            "capture",
            "service",
            "--plan",
            "--param",
            &assignment,
        ]);
        assert_success(&output);
        safe(&output);
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains("parameter: secret_text\teffective_redaction: true")
        );
        assert_eq!(run_count(&s.run(["run", "list", "service"])), 0);
        assert_eq!(s.hook_launches(), 0);
        bind(&s, SECRET);
        let secret = s.path("secret-parameter");
        fs::write(&secret, PRIVATE).unwrap();
        let binding = format!("secret_text={}", secret.display());
        let output = s.run(["snapshot", "capture", "service", "--param-file", &binding]);
        assert_success(&output);
        safe(&output);
        let id = field(&output, "snapshot");
        assert_eq!(s.hook_launches(), 1);
        let show = s.run(["snapshot", "show", &id]);
        safe(&show);
        let sessions = fs::read_dir(s.storage.join("staging")).unwrap().count();
        assert_success(&s.run(["snapshot", "verify", &id]));
        assert_eq!(
            sessions,
            fs::read_dir(s.storage.join("staging")).unwrap().count()
        );
        assert!(revision.starts_with("exact:"));
    }
}

// Test-ID: PR-TEST-0271
// Verifies: PR-REQ-0301, PR-REQ-0302, PR-REQ-0291, PR-REQ-0289
#[test]
fn snapshot_failures_recovery_timeouts_and_mixed_run_inspection_cross_real_processes() {
    let s = Scenario::new(0x75, &source("observe"));
    initialize(&s);
    let id = capture(&s);
    for mode in ["failure", "protocol", "no_ready"] {
        let mode = format!("mode={mode}");
        let result = s.run([
            "snapshot",
            "capture",
            "service",
            "--param",
            &mode,
            "--startup-timeout-ms",
            "200",
            "--execution-timeout-ms",
            "1000",
            "--termination-grace-ms",
            "0",
        ]);
        assert_exit(&result, 1);
        safe(&result);
        let run = field(&result, "run");
        let show = s.run(["run", "show", &run]);
        assert_success(&show);
        safe(&show);
        assert_eq!(field(&show, "operation"), "snapshot_capture");
        assert_eq!(field(&show, "phase"), "finished");
    }
    let failure = s.run([
        "snapshot",
        "capture",
        "service",
        "--param",
        "mode=open_failure",
    ]);
    assert_exit(&failure, 1);
    safe(&failure);
    let refused = s.run(["snapshot", "restore", "service", &id]);
    assert_exit(&refused, 1);
    safe(&refused);
    let restored = s.run([
        "snapshot",
        "restore",
        "service",
        &id,
        "--authorize-recovery-override",
    ]);
    assert_success(&restored);
    safe(&restored);
    assert_eq!(field(&restored, "current_recovery_guard"), "none");
    let actor = s.spawn(["snapshot", "capture", "service", "--param", "mode=hold"]);
    s.wait_for_marker("ready:hold");
    let live = s.run(["run", "list", "service"]);
    assert_success(&live);
    safe(&live);
    assert!(String::from_utf8_lossy(&live.stdout).contains("phase: running"));
    assert!(s.run(["run", "reconcile"]).stdout.is_empty());
    let _ = actor.terminate();
    let reconciled = s.run(["run", "reconcile"]);
    assert_success(&reconciled);
    let run = first_line(&reconciled.stdout);
    let show = s.run(["run", "show", run]);
    assert_success(&show);
    assert_eq!(field(&show, "outcome"), "interrupted");
    assert!(s.run(["run", "reconcile"]).stdout.is_empty());
}

// Test-ID: PR-TEST-0272
// Verifies: PR-REQ-0301, PR-REQ-0302
#[test]
fn snapshot_cli_keeps_interactive_stdin_and_hostile_input_errors_out_of_execution() {
    let s = Scenario::new(
        0x76,
        &source("observe").replace("terminal: none", "terminal: interactive"),
    );
    initialize(&s);
    let output = s.run_with_stdin(
        [
            "snapshot",
            "capture",
            "service",
            "--plan",
            "--param-stdin",
            "secret_text",
        ],
        PRIVATE.as_bytes().to_vec(),
    );
    assert_exit(&output, 2);
    safe(&output);
    assert_eq!(s.hook_launches(), 0);
    assert_eq!(run_count(&s.run(["run", "list", "service"])), 0);
    let path = s.path(PRIVATE);
    let error = s.run(["snapshot", "import", path.to_str().unwrap()]);
    assert_exit(&error, 1);
    safe(&error);
    assert!(String::from_utf8_lossy(&error.stderr).contains("host I/O"));
    fs::write(&path, b"not a ZIP archive").unwrap();
    let error = s.run(["snapshot", "import", path.to_str().unwrap()]);
    assert_exit(&error, 1);
    safe(&error);
    assert!(String::from_utf8_lossy(&error.stderr).contains("bundle profile"));
    assert_eq!(run_count(&s.run(["run", "list", "service"])), 0);
}

// Test-ID: PR-TEST-0274
// Verifies: PR-REQ-0301, PR-REQ-0302, PR-REQ-0091, PR-REQ-0101, PR-REQ-0293
#[test]
fn public_snapshot_closure_distinguishes_readiness_compatibility_capacity_and_source_safety() {
    let s = Scenario::new(0x77, &source("observe"));
    let revision = s.install_and_create("service");
    let failed = s.run(["snapshot", "capture", "service"]);
    assert_exit(&failed, 1);
    safe(&failed);
    assert_eq!(run_count(&s.run(["run", "list", "service"])), 0);
    assert_eq!(s.hook_launches(), 0);
    bind(&s, SECRET);
    let plan = s.run_with_stdin(
        [
            "snapshot",
            "capture",
            "service",
            "--plan",
            "--param-stdin",
            "mode",
        ],
        b"success".to_vec(),
    );
    assert_success(&plan);
    assert!(
        String::from_utf8_lossy(&plan.stdout)
            .contains("parameter: mode\teffective_redaction: true")
    );
    let base = s.path("base.zip");
    let id = v1_bundle(&base, &revision);
    let mut archive = zip::ZipArchive::new(fs::File::open(&base).unwrap()).unwrap();
    let over = s.path("oversize.zip");
    let mut writer = zip::ZipWriter::new(fs::File::create(&over).unwrap());
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let name = entry.name().to_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if name == "bundle.json" {
            bytes.resize(64 * 1024 + 1, b' ');
        }
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap();
    let error = s.run(["snapshot", "import", over.to_str().unwrap()]);
    assert_exit(&error, 1);
    safe(&error);
    assert!(String::from_utf8_lossy(&error.stderr).contains("verification incomplete"));
    assert!(s.run(["snapshot", "list"]).stdout.is_empty());
    let other = Scenario::new(0x78, &source("mutate"));
    initialize(&other);
    assert_success(&other.run(["snapshot", "import", base.to_str().unwrap()]));
    let failed = other.run(["snapshot", "restore", "service", &id]);
    assert_exit(&failed, 1);
    safe(&failed);
    assert!(String::from_utf8_lossy(&failed.stderr).contains("exact producer Revision"));
    assert_eq!(other.hook_launches(), 0);
    assert_eq!(run_count(&other.run(["run", "list", "service"])), 0);
    #[cfg(target_os = "linux")]
    {
        let fifo = s.path("bundle-fifo");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        let error = s.run(["snapshot", "import", fifo.to_str().unwrap()]);
        assert_exit(&error, 1);
        assert!(String::from_utf8_lossy(&error.stderr).contains("regular file"));
    }
}

#[cfg(unix)]
fn connect(endpoint: &str) -> io::Result<std::os::unix::net::UnixStream> {
    std::os::unix::net::UnixStream::connect(endpoint)
}
#[cfg(windows)]
fn connect(endpoint: &str) -> io::Result<fs::File> {
    fs::OpenOptions::new().read(true).write(true).open(endpoint)
}
fn read_frame(stream: &mut impl Read) -> Value {
    let mut size = [0; 4];
    stream.read_exact(&mut size).unwrap();
    let size = u32::from_be_bytes(size) as usize;
    assert!(size <= 16 * 1024 * 1024);
    let mut payload = vec![0; size];
    stream.read_exact(&mut payload).unwrap();
    serde_json::from_slice(&payload).unwrap()
}
fn write_frame(stream: &mut impl Write, value: Value) {
    let bytes = serde_json::to_vec(&value).unwrap();
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .unwrap();
    stream.write_all(&bytes).unwrap();
    stream.flush().unwrap();
}
fn append(path: &Path, text: &str) {
    fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
}

#[test]
fn snapshot_hook_worker() {
    let Ok(endpoint) = env::var("PACTRUN_HOOK_PROTOCOL_ENDPOINT") else {
        return;
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stream = loop {
        match connect(&endpoint) {
            Ok(stream) => break stream,
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Err(error) => panic!("transport: {error}"),
        }
    };
    let mut preamble = [0; PREAMBLE.len()];
    stream.read_exact(&mut preamble).unwrap();
    assert_eq!(preamble, PREAMBLE);
    let session = read_frame(&mut stream);
    let params = session["parameters"].as_array().unwrap();
    let param = |name: &str| {
        params.iter().find(|p| p["parameter_id"] == name).unwrap()["value"]
            .as_str()
            .unwrap()
    };
    let mode = param("mode");
    let marker = Path::new(param("marker"));
    append(
        marker,
        &format!(
            "launch:{}\nrun:{}\n",
            std::process::id(),
            session["run_id"].as_str().unwrap()
        ),
    );
    assert_eq!(param("secret_text"), PRIVATE);
    stream.write_all(PREAMBLE).unwrap();
    if mode == "no_ready" {
        thread::sleep(Duration::from_secs(20));
        return;
    }
    write_frame(
        &mut stream,
        json!({"type":"session_ready","protocol_version":1,"session_id":session["session_id"]}),
    );
    if mode == "hold" {
        append(marker, "ready:hold\n");
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    if mode == "open_failure" {
        write_frame(
            &mut stream,
            json!({"type":"request","request_id":1,"request":{"kind":"enter_recovery_risk"}}),
        );
        assert_eq!(read_frame(&mut stream)["risk_state"], "open");
    }
    if mode == "protocol" {
        write_frame(
            &mut stream,
            json!({"type":"complete","operation":"snapshot_restore","status":"success","produced_outputs":[]}),
        );
        return;
    }
    let operation = session["operation"]["kind"].as_str().unwrap();
    let bindings = session["operation"]["bindings"].as_array().unwrap();
    assert!(bindings.iter().all(|b| b["role"] == "active"));
    let secret = bindings.iter().find(|b| b["input_id"] == "secret").unwrap();
    assert_eq!(
        fs::read(secret["readonly_path"].as_str().unwrap()).unwrap(),
        SECRET
    );
    let mut done = json!({"type":"complete","operation":operation,"status":if mode=="failure"||mode=="open_failure" {"failure"}else{"success"},"code":"private-hook-code","message":PRIVATE});
    if operation == "snapshot_capture" {
        let root = Path::new(
            session["operation"]["candidate"]["root_path"]
                .as_str()
                .unwrap(),
        );
        fs::write(root.join("submitted"), SERVICE).unwrap();
        fs::write(root.join("unsubmitted"), b"unrelated-service-sentinel").unwrap();
        done["service_content"] = if done["status"] == "success" {
            json!([{"role":"database","path":"state/main","candidate_path":"submitted"}])
        } else {
            json!([])
        };
    } else {
        assert_eq!(operation, "snapshot_restore");
        assert!(session["operation"].get("candidate").is_none());
        let content = &session["operation"]["snapshot_content"];
        let root = Path::new(content["readonly_root_path"].as_str().unwrap());
        let descriptors = content["logical_descriptors"].as_array().unwrap();
        assert_eq!(descriptors.len(), 1);
        let descriptor = &descriptors[0];
        assert_eq!(descriptor["role"], "database");
        assert_eq!(descriptor["path"], "state/main");
        let bytes = fs::read(root.join(descriptor["materialized_path"].as_str().unwrap())).unwrap();
        assert_eq!(bytes, SERVICE);
        assert_eq!(
            descriptor["blob_digest"],
            format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
        );
        assert!(!root.join("unsubmitted").exists());
        append(marker, "restore-content-verified\n");
    }
    write_frame(&mut stream, done);
    assert_eq!(read_frame(&mut stream)["type"], "completion_accepted");
}
