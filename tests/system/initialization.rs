use super::support::{Scenario, assert_success};
#[cfg(target_os = "linux")]
use super::support::{command, run_command};
use serde_json::{Value, json};
#[cfg(target_os = "linux")]
use std::path::Path;
use std::{fs, process::Output};

fn source(version: u8) -> String {
    let (shell, command) = if cfg!(windows) {
        ("powershell_7", "pwsh.exe")
    } else {
        ("sh", "sh")
    };
    let hook = json!({"protocol_version":version,"launch":{"kind":"shell_loader","shell":shell,"command":command,"script":"script"},"args":[],"io":{"terminal":"none"}});
    let parameter = json!({"id":"note","type":"string","sensitive":false,"default":"default"});
    json!({"source_format":3,"package_id":"{package_id}","revision":{
        "actions":[{"id":"inspect","access":"observe","parameters":[parameter.clone()],"outputs":[{"id":"report"}],"hook":hook.clone()}],
        "snapshot":{"capture":{"access":"observe","parameters":[parameter.clone()],"hook":hook.clone()},"restore":{"parameters":[parameter],"hook":hook.clone()}},
        "cleanup":{"requires":[],"hook":hook}},
        "runtime_content":{"files":[{"id":"script","source":"script.txt","path":if cfg!(windows) {"scripts/script.ps1"} else {"scripts/script.sh"},"executable":false}]}}).to_string()
}

fn query(s: &Scenario, args: &[&str]) -> Value {
    let mut all = vec!["--format", "json"];
    all.extend_from_slice(args);
    let output = s.run(all);
    assert_success(&output);
    serde_json::from_slice::<Value>(&output.stdout).unwrap()["result"].clone()
}
fn field(output: &Output, key: &str) -> String {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .chain(String::from_utf8_lossy(&output.stderr).lines())
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))
        .unwrap()
        .to_owned()
}
fn entries(value: &Value) -> Vec<&Value> {
    value["presentation"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["entries"].as_array().unwrap())
        .collect()
}
fn explain(target: Value, text: &str) -> Value {
    json!({"target":target,"field":"description","value":text})
}

// Test-ID: PR-TEST-0588
// Verifies: PR-REQ-0086, PR-REQ-0369
#[test]
fn short_ids_cover_snapshot_artifact_migration_and_retirement_operations() {
    let s = Scenario::new(819, &source(2));
    fs::write(s.source.join("script.txt"), "exit 0\n").unwrap();
    let first = s.install_and_create("sample");
    let invoked = query(&s, &["invoke", "sample", "inspect"]);
    let run = invoked["run"]["run_id"].as_str().unwrap();
    assert_eq!(query(&s, &["run", "show", &run[..8]])["run"]["run_id"], run);
    query(&s, &["run", "artifact", "delete", &run[..9], "report"]);
    query(&s, &["run", "delete", &run[..8], "--delete-artifacts"]);
    let captured = query(&s, &["snapshot", "capture", "sample"]);
    let snapshot = captured["capture_result"].as_str().unwrap();
    let short = &snapshot[..8];
    let listed = s.run(["snapshot", "list"]);
    assert_success(&listed);
    let listed = String::from_utf8(listed.stdout).unwrap();
    let displayed = listed
        .lines()
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    assert_eq!(displayed.len(), 12);
    assert_eq!(
        query(&s, &["snapshot", "show", displayed])["snapshot_id"],
        snapshot
    );
    assert_eq!(
        query(&s, &["snapshot", "show", short])["snapshot_id"],
        snapshot
    );
    query(&s, &["snapshot", "verify", short]);
    query(&s, &["snapshot", "restore", "sample", short]);
    let (package, digest) = first
        .strip_prefix("exact:")
        .unwrap()
        .split_once('/')
        .unwrap();
    let short_revision = format!("exact:{}/sha256:{}", &package[..8], &digest[7..15]);
    query(
        &s,
        &[
            "instance",
            "create",
            "restored",
            "--revision",
            &short_revision,
            "--restore-from",
            short,
        ],
    );
    let destination = s.path("short-export");
    query(
        &s,
        &[
            "snapshot",
            "export",
            short,
            "--output",
            destination.to_str().unwrap(),
            "--authorize-sensitive-export",
        ],
    );
    let path = s.source.join("pactrun.yaml");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["revision"]["migrations"] = json!([{"source_revision_digest":digest,"transitions":[],"requires_source":[],"requires_target":[],"produces_target":[]}]);
    fs::write(&path, manifest.to_string()).unwrap();
    let target = s.install();
    let paths = query(
        &s,
        &["instance", "migration-paths", "sample", "--to", &target],
    );
    let id = paths["candidates"][0]["path_id"].as_str().unwrap();
    let short_path = &id[..12]; // mp1- plus eight digits.
    assert!(
        query(
            &s,
            &[
                "instance",
                "migration-paths",
                "sample",
                "--to",
                &target,
                "--after",
                short_path
            ]
        )["candidates"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    query(
        &s,
        &[
            "instance", "migrate", "sample", "--to", &target, "--path", short_path,
        ],
    );
    query(&s, &["snapshot", "delete", short]);

    // Failed Cleanup retains its original attempt; short selectors still require exact CAS.
    let t = Scenario::new(820, &source(2));
    fs::write(
        t.source.join("script.txt"),
        if cfg!(windows) {
            "[Threading.Thread]::Sleep(10000)\n"
        } else {
            "sleep 10\n"
        },
    )
    .unwrap();
    t.install_and_create("sample");
    let failed = t.run([
        "instance",
        "delete",
        "sample",
        "--execution-timeout-ms",
        "1500",
        "--termination-grace-ms",
        "0",
    ]);
    assert!(!failed.status.success());
    let view = query(&t, &["instance", "show", "sample"]);
    let instance = view["instance_id"].as_str().unwrap();
    let deletion = query(&t, &["instance", "deletion", "show", &instance[..8]]);
    assert!(deletion["obligation"].is_object(), "{deletion}; {failed:?}");
    let attempt = deletion["obligation"]["attempt_run_id"].as_str().unwrap();
    query(
        &t,
        &[
            "instance",
            "deletion",
            "confirm-complete",
            &instance[..8],
            "--attempt",
            &attempt[..8],
            "--if-version",
            view["state_version"].as_str().unwrap(),
            "--assert-cleanup-complete",
        ],
    );
    query(&t, &["instance", "delete", "sample"]);
    assert_eq!(
        query(&t, &["run", "show", &attempt[..8]])["run"]["state"]["outcome"],
        "timed_out"
    );

    let a = Scenario::new(821, &source(2));
    fs::write(a.source.join("script.txt"), "exit 0\n").unwrap();
    let path = a.source.join("pactrun.yaml");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["revision"]["service_storages"] = json!([{"id":"state"}]);
    fs::write(&path, manifest.to_string()).unwrap();
    a.install_and_create("sample");
    query(&a, &["instance", "abandon", "sample"]);
    let allocations = query(&a, &["service-storage", "detached", "list"]);
    let allocation = allocations["items"][0]["allocation_id"].as_str().unwrap();
    query(
        &a,
        &["service-storage", "detached", "show", &allocation[..8]],
    );
    query(
        &a,
        &[
            "service-storage",
            "detached",
            "discard",
            &allocation[..8],
            "--confirm-discard",
        ],
    );
    assert_eq!(
        query(
            &a,
            &[
                "service-storage",
                "detached",
                "discard",
                &allocation[..8],
                "--confirm-discard"
            ]
        )["new_completion"],
        false
    );
}

// Test-ID: PR-TEST-0585
// Verifies: PR-REQ-0006, PR-REQ-0126, PR-REQ-0366, PR-REQ-0367
#[test]
fn machine_delivery_covers_shell_loader_lifecycle_and_each_migration_hook() {
    use base64::Engine as _;
    let s = Scenario::new(
        815,
        &source(2).replace("\"terminal\":\"none\"", "\"terminal\":\"output\""),
    );
    fs::write(
        s.source.join("script.txt"),
        if cfg!(windows) {
            "[Console]::Out.Write('out'); [Console]::Error.Write('err'); exit 0\n"
        } else {
            "printf out; printf err >&2; exit 0\n"
        },
    )
    .unwrap();
    let first = s.install_and_create("sample");
    let invoke = |args: &[&str], hooks: usize| {
        let mut all = vec!["--format", "jsonl"];
        all.extend_from_slice(args);
        let output = s.run(all);
        assert_success(&output);
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut events: Vec<Value> = output
            .stdout
            .split(|b| *b == b'\n')
            .filter(|b| !b.is_empty())
            .map(|b| serde_json::from_slice(b).unwrap())
            .collect();
        for (index, event) in events.iter().enumerate() {
            assert_eq!(event["sequence"], (index + 1).to_string());
            assert_eq!(event["format"], "pactrun.cli");
            assert_eq!(event["format_version"], "1.0-alpha.1");
        }
        let result = events.pop().unwrap();
        assert_eq!(result["type"], "result");
        assert_eq!(result["response"]["delivery"]["complete"], true);
        assert!(result["response"]["delivery"].get("events").is_none());
        assert_eq!(
            events
                .iter()
                .filter(|e| e["type"] == "run_accepted")
                .count(),
            1
        );
        let mut streams = std::collections::BTreeMap::<(String, String), Vec<u8>>::new();
        for event in &events {
            if event["type"] != "output" {
                continue;
            }
            let ordinal = event["context"]["hook_ordinal"]
                .as_str()
                .unwrap()
                .to_owned();
            let channel = event["channel"].as_str().unwrap().to_owned();
            let stream = streams.entry((ordinal, channel)).or_default();
            assert_eq!(event["offset"], stream.len().to_string());
            stream.extend(
                base64::engine::general_purpose::STANDARD
                    .decode(event["data"].as_str().unwrap())
                    .unwrap(),
            );
            if hooks > 1 {
                assert!(event["context"]["source_revision"].is_object());
                assert_eq!(
                    event["context"]["target_revision"],
                    event["context"]["revision"]
                );
            }
        }
        assert_eq!(streams.len(), hooks * 2);
        for ordinal in 1..=hooks {
            assert_eq!(streams[&(ordinal.to_string(), "stdout".into())], b"out");
            assert_eq!(streams[&(ordinal.to_string(), "stderr".into())], b"err");
        }
        result["response"]["result"].clone()
    };
    invoke(&["invoke", "sample", "inspect"], 1);
    let captured = invoke(&["snapshot", "capture", "sample"], 1);
    let snapshot = captured["capture_result"].as_str().unwrap();
    invoke(&["snapshot", "restore", "sample", snapshot], 1);
    invoke(
        &[
            "instance",
            "create",
            "restored",
            "--revision",
            &first,
            "--restore-from",
            snapshot,
        ],
        1,
    );
    let file = s.source.join("pactrun.yaml");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    let mut previous = first;
    for _ in 0..2 {
        let digest = previous.split('/').next_back().unwrap();
        manifest["revision"]["migrations"] = json!([{"source_revision_digest":digest,"transitions":[],"requires_source":[],"requires_target":[],"produces_target":[],"hook":manifest["revision"]["actions"][0]["hook"].clone()}]);
        fs::write(&file, manifest.to_string()).unwrap();
        previous = s.install();
    }
    invoke(&["instance", "migrate", "sample", "--to", &previous], 2);
    invoke(&["instance", "delete", "sample"], 1);
}

// Test-ID: PR-TEST-0575
// Verifies: PR-REQ-0363
#[test]
fn capability_metadata_follows_exact_revisions_without_execution_or_history_mutation() {
    let s = Scenario::new(811, &source(2));
    fs::write(s.source.join("script.txt"), "exit 0\n").unwrap();
    let file = s.source.join("pactrun.yaml");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    manifest["portable_metadata"] = json!({"presentation":[
        explain(json!({"kind":"revision"}),"overall"),
        explain(json!({"kind":"action","action_id":"inspect"}),"old action\nsecond line\u{001b}[31m"),
        json!({"target":{"kind":"action","action_id":"inspect"},"field":"summary","value":"short summary"}),
        explain(json!({"kind":"action_parameter","action_id":"inspect","parameter_id":"note"}),"action parameter"),
        explain(json!({"kind":"managed_output","action_id":"inspect","output_id":"report"}),"output description"),
        explain(json!({"kind":"snapshot_capture"}),"capture only"),
        explain(json!({"kind":"snapshot_capture_parameter","parameter_id":"note"}),"capture parameter"),
        explain(json!({"kind":"snapshot_restore"}),"restore only"),
        explain(json!({"kind":"snapshot_restore_parameter","parameter_id":"note"}),"restore parameter"),
        explain(json!({"kind":"cleanup"}),"cleanup only")
    ]});
    fs::write(&file, manifest.to_string()).unwrap();
    let first = s.install_and_create("sample");
    let short = s.run(["action", "list", "sample"]);
    assert_success(&short);
    assert!(String::from_utf8_lossy(&short.stdout).contains("short summary"));
    assert!(!String::from_utf8_lossy(&short.stdout).contains("old action"));
    let before = query(&s, &["instance", "show", "sample"]);
    assert_eq!(entries(&query(&s, &["action", "list", "sample"])).len(), 1);
    assert_eq!(
        entries(&query(&s, &["action", "show", "sample", "inspect"])).len(),
        3
    );
    let shown = s.run(["action", "show", "sample", "inspect"]);
    assert_success(&shown);
    assert!(!shown.stdout.contains(&0x1b));
    assert!(String::from_utf8_lossy(&shown.stdout).contains("second line"));
    assert_eq!(
        entries(&query(&s, &["invoke", "sample", "inspect", "--plan"])).len(),
        3
    );
    assert_eq!(
        entries(&query(&s, &["snapshot", "capture", "sample", "--plan"])).len(),
        2
    );
    assert_eq!(
        entries(&query(&s, &["instance", "delete", "sample", "--plan"])).len(),
        1
    );
    assert!(entries(&query(&s, &["instance", "abandon", "sample", "--plan"])).is_empty());
    assert_eq!(before, query(&s, &["instance", "show", "sample"]));
    assert!(
        query(&s, &["run", "list"])["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let all = query(&s, &["revision", "show", &first]);
    assert_eq!(entries(&all).len(), 9);
    assert!(
        query(&s, &["revision", "metadata", "show", &first])
            .get("presentation")
            .is_none()
    );

    let captured = s.run(["snapshot", "capture", "sample"]);
    assert_success(&captured);
    let snapshot = field(&captured, "snapshot");
    for entry in manifest["portable_metadata"]["presentation"]
        .as_array_mut()
        .unwrap()
    {
        if entry["target"]["kind"] == "snapshot_restore" {
            entry["value"] = json!("restore updated after Capture");
        }
    }
    fs::write(&file, manifest.to_string()).unwrap();
    let updated = s.run([
        "pack",
        "install",
        s.source.to_str().unwrap(),
        "--metadata-conflict",
        "overwrite",
    ]);
    assert_success(&updated);
    assert_eq!(
        String::from_utf8_lossy(&updated.stdout)
            .lines()
            .next()
            .unwrap(),
        first
    ); // Descriptions are not Revision identity.
    let restore = query(&s, &["snapshot", "restore", "sample", &snapshot, "--plan"]);
    assert_eq!(entries(&restore).len(), 2);
    assert!(
        entries(&restore)
            .iter()
            .any(|entry| entry["description"] == "restore updated after Capture")
    );
    assert!(
        entries(&restore)
            .iter()
            .all(|e| e["description"].as_str().unwrap().starts_with("restore"))
    );
    let run = field(&captured, "run");
    assert!(
        query(&s, &["run", "show", &run])
            .get("presentation")
            .is_none()
    );
    assert!(
        query(&s, &["snapshot", "show", &snapshot])
            .get("presentation")
            .is_none()
    );
    let before_migration = query(&s, &["run", "list"]);
    let mut previous = first.clone();
    for label in ["edge-one", "edge-two"] {
        let digest = previous.split('/').next_back().unwrap();
        manifest["revision"]["migrations"] = json!([{"source_revision_digest":digest,"transitions":[],"requires_source":[],"requires_target":[],"produces_target":[]}]);
        manifest["portable_metadata"]["presentation"] = json!([
            explain(
                json!({"kind":"migration","source_revision_digest":digest}),
                label
            ),
            explain(json!({"kind":"action","action_id":"inspect"}), label)
        ]);
        fs::write(&file, manifest.to_string()).unwrap();
        previous = s.install();
    }
    let paths = query(
        &s,
        &["instance", "migration-paths", "sample", "--to", &previous],
    );
    assert_eq!(paths["candidates"][0]["edge_count"], 2);
    assert_eq!(
        entries(&paths)
            .iter()
            .map(|e| e["description"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["edge-one", "edge-two"]
    );
    let migration = query(
        &s,
        &["instance", "migrate", "sample", "--to", &previous, "--plan"],
    );
    assert_eq!(entries(&migration).len(), 2);
    assert_eq!(before_migration, query(&s, &["run", "list"]));
    assert!(
        entries(&query(&s, &["action", "show", "sample", "inspect"]))[0]["description"]
            .as_str()
            .unwrap()
            .starts_with("old action")
    );
}

// Test-ID: PR-TEST-0574
// Verifies: PR-REQ-0361, PR-REQ-0362
#[cfg(target_os = "linux")]
#[test]
fn long_tmpdir_fallback_preserves_child_environment_and_initialization_step_for_every_operation() {
    use std::os::unix::{ffi::OsStringExt, fs::PermissionsExt};
    let tmp = tempfile::Builder::new()
        .prefix("ip-")
        .tempdir_in("/tmp")
        .unwrap();
    fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o700)).unwrap();
    for version in [1, 2] {
        let s = Scenario::new(812 + u16::from(version), &source(version));
        fs::write(
            s.source.join("script.txt"),
            format!(
                "printf '%s' \"$TMPDIR\" > '{}'\nexit 0\n",
                s.path("marker").display()
            ),
        )
        .unwrap();
        let first = s.install_and_create("sample");
        let invoke = |args: &[&str], path: &Path| {
            let mut cmd = command(&s.storage, &s.path(""), args.iter().copied());
            cmd.env("TMPDIR", path);
            run_command(cmd)
        };
        for length in [32usize, 47, 48, 49, 55, 56, 57, 68, 96] {
            let path = tmp
                .path()
                .join("x".repeat(length - tmp.path().as_os_str().len() - 1));
            if !path.exists() {
                fs::create_dir(&path).unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            }
            assert_success(&invoke(&["invoke", "sample", "inspect"], &path));
            assert_eq!(
                fs::read(s.path("marker")).unwrap(),
                path.as_os_str().as_encoded_bytes()
            );
            assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
        }
        let opaque = tmp
            .path()
            .join(std::ffi::OsString::from_vec(vec![b'x', 255]));
        if !opaque.exists() {
            fs::create_dir(&opaque).unwrap();
        }
        assert_success(&invoke(&["invoke", "sample", "inspect"], &opaque));
        assert_eq!(
            fs::read(s.path("marker")).unwrap(),
            opaque.as_os_str().as_encoded_bytes()
        );
        let captured = invoke(&["snapshot", "capture", "sample"], tmp.path());
        assert_success(&captured);
        let snapshot = field(&captured, "snapshot");
        let missing = tmp.path().join("missing");
        let mut yaml: Value =
            serde_json::from_slice(&fs::read(s.source.join("pactrun.yaml")).unwrap()).unwrap();
        let digest = first.split('/').next_back().unwrap();
        yaml["revision"]["migrations"] = json!([{"source_revision_digest":digest,"transitions":[],"requires_source":[],"requires_target":[],"produces_target":[],"hook":yaml["revision"]["actions"][0]["hook"].clone()}]);
        fs::write(s.source.join("pactrun.yaml"), yaml.to_string()).unwrap();
        let target = s.install();
        for args in [
            vec!["invoke", "sample", "inspect"],
            vec!["snapshot", "capture", "sample"],
            vec!["snapshot", "restore", "sample", &snapshot],
            vec!["instance", "migrate", "sample", "--to", &target],
            vec!["instance", "delete", "sample"],
        ] {
            fs::remove_file(s.path("marker")).unwrap_or(());
            let failed = invoke(&args, &missing);
            assert!(!failed.status.success());
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&failed.stdout),
                String::from_utf8_lossy(&failed.stderr)
            );
            assert!(
                text.contains("ipc_initialization_failed") && text.contains("establish_session"),
                "{text}"
            );
            assert!(!s.path("marker").exists());
            let inspected = query(&s, &["run", "show", &field(&failed, "run")]);
            assert_eq!(
                inspected["run"]["state"]["primary_failure"]["step"],
                "establish_session"
            );
            assert!(
                inspected["run"]["state"]["primary_failure"]["detail"]
                    .as_str()
                    .unwrap()
                    .contains("temporary location unavailable")
            );
        }
    }
}
