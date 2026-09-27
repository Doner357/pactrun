// Included in cli::tests to reuse the real Hook worker and isolated stores.
fn described_action() -> (TempDir, PathBuf, PathBuf, String) {
    let (temp, root, source) = cli_action_roots();
    let path = source.join("pactrun.yaml");
    let mut text = fs::read_to_string(&path).unwrap()
        .to_owned()
        .replace("{ id: value, type: string, sensitive: false }", "{ id: value, type: string, sensitive: false, default: normal-default }\n        - { id: token, type: string, sensitive: true, default: private-default-sentinel }");
    text.push_str("\nportable_metadata:\n  presentation:\n    - target: { kind: action, action_id: inspect }\n      field: summary\n      value: Inspect the service\n    - target: { kind: action, action_id: inspect }\n      field: description\n      value: \"Read its status.\\nMore details here.\"\n");
    fs::write(path, text).unwrap();
    let revision = json_install(&root, &source);
    json_ok(&root, &["instance", "create", "node", "--revision", &revision]);
    (temp, root, source, revision)
}

// Test-ID: PR-TEST-0578
// Verifies: PR-REQ-0364
#[test]
fn human_queries_are_concise_and_keep_actionable_parameter_information() {
    let (_temp, root, _source, _) = described_action();
    for (args, detail) in [(vec!["action", "list", "node"], false), (vec!["action", "show", "node", "inspect"], true)] {
        let mut out = Vec::new(); let mut err = Vec::new();
        assert_eq!(run(args.iter().map(OsString::from).collect(), Some(root.as_os_str().into()), &mut io::empty(), &mut out, &mut err), 0);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("inspect") && text.contains("Inspect the service"));
        assert_eq!(text.contains("Read its status."), detail);
        assert_eq!(text.contains("default: normal-default") || text.contains("default: \"normal-default\""), detail);
        assert_eq!(text.contains("default: [redacted]"), detail);
        for forbidden in ["not verified", "not an execution", "not authentication", "hook_args_count", "protocol_version", "private-default-sentinel", "Absent"] { assert!(!text.contains(forbidden), "{text}"); }
    }
}

// Test-ID: PR-TEST-0579
// Verifies: PR-REQ-0365
#[test]
fn machine_queries_include_public_definitions_without_mutating_execution_state() {
    let (_temp, root, source, revision) = described_action();
    let database = root.join("database/pactrun.sqlite3");
    let before = fs::read(&database).unwrap();
    let staging = || { let mut entries: Vec<_> = fs::read_dir(root.join("staging")).unwrap().map(|e| e.unwrap().file_name()).collect(); entries.sort(); entries };
    let before_staging = staging();
    let shown = json_ok(&root, &["action", "show", "node", "inspect"]);
    let listed = json_ok(&root, &["action", "list", "node"]);
    assert_eq!(listed["items"][0], { let mut value = shown.clone(); value.as_object_mut().unwrap().remove("presentation"); value });
    let parameters = shown["parameters"].as_array().unwrap();
    let normal = parameters.iter().find(|p| p["parameter_id"] == "value").unwrap();
    let secret = parameters.iter().find(|p| p["parameter_id"] == "token").unwrap();
    assert_eq!(normal["default_value"]["value"], "normal-default");
    assert!(secret["default_value"].is_null());
    assert_eq!(secret["default_redacted"], true);
    assert!(!shown.to_string().contains("private-default-sentinel"));
    let detail = json_ok(&root, &["instance", "show", "node"]);
    let list = json_ok(&root, &["instance", "list"]);
    for (key, value) in list["items"][0].as_object().unwrap() { assert_eq!(value, &detail[key]); }
    assert_eq!(list["related_revisions"], detail["related_revisions"]);
    assert_eq!(detail["related_revisions"][0]["metadata_scope"], "current");
    let preview = json_ok(&root, &["invoke", "node", "inspect", "--plan"]);
    assert!(json_ok(&root, &["run", "list"])["items"].as_array().unwrap().is_empty());
    assert!(before == fs::read(database).unwrap(), "query changed database bytes");
    assert_eq!(before_staging, staging());
    let path = source.join("pactrun.yaml");
    fs::write(&path, fs::read_to_string(&path).unwrap().replace("Inspect the service", "Updated summary")).unwrap();
    let installed = json_ok(&root, &["pack", "install", source.to_str().unwrap(), "--metadata-conflict", "overwrite"]);
    assert_eq!(format!("exact:{}/{}", installed["revision"]["package_id"].as_str().unwrap(), installed["revision"]["content_digest"].as_str().unwrap()), revision);
    let mut updated = json_ok(&root, &["invoke", "node", "inspect", "--plan"]);
    let mut original = preview;
    assert_ne!(updated["presentation"], original["presentation"]);
    updated.as_object_mut().unwrap().remove("presentation");
    original.as_object_mut().unwrap().remove("presentation");
    assert_eq!(original, updated);
}

fn machine_args(marker: &Path, expected: &Path, sensitive: &Path) -> Vec<OsString> {
    let mut args = successful_invoke_args(marker, expected, sensitive);
    *args.iter_mut().find(|a| **a == "mode=success").unwrap() = "mode=machine_stream".into();
    args.push("--no-retain-hook-text".into());
    args
}

fn assert_machine_bytes(events: &[serde_json::Value]) {
    use base64::Engine as _;
    let mut streams = std::collections::BTreeMap::<String, Vec<u8>>::new();
    for (n, event) in events.iter().enumerate() {
        schema_tests::assert_event(event);
        assert_eq!(event["sequence"], (n + 1).to_string());
        assert_eq!(event["format"], "pactrun.cli");
        assert_eq!(event["format_version"], "1.0-alpha.1");
        if event["type"] != "output" { continue; }
        assert_eq!(event["encoding"], "base64");
        let bytes = base64::engine::general_purpose::STANDARD.decode(event["data"].as_str().unwrap()).unwrap();
        assert!(!bytes.is_empty() && bytes.len() <= 64 * 1024);
        assert_eq!(event["byte_length"], bytes.len().to_string());
        assert_eq!(event["context"]["operation"], "action");
        assert_eq!(event["context"]["hook_ordinal"], "1");
        let stream = streams.entry(event["channel"].as_str().unwrap().into()).or_default();
        assert_eq!(event["offset"], stream.len().to_string());
        stream.extend(bytes);
    }
    let expected: Vec<u8> = (0..512 * 1024).map(|n| (n % 256) as u8).collect();
    assert!(streams["stdout"].windows(expected.len()).any(|b| b == expected));
    assert_eq!(streams["stderr"], [0, 255, 13, 27, 123]);
    assert_eq!(events.iter().filter(|e| e["type"] == "run_accepted").count(), 1);
    assert!(events.iter().any(|e| e["type"] == "diagnostic" && e["message"] == "full-live-text".repeat(1024)));
}

// Test-ID: PR-TEST-0580
// Verifies: PR-REQ-0366, PR-REQ-0367
#[test]
fn machine_json_delivers_binary_streams_and_unretained_diagnostics() {
    let (temp, root, expected, sensitive) = prepared_cli_hook_instance_with_terminal("output");
    let marker = temp.path().join("machine-json");
    let (code, response, err) = json_invoke(&root, machine_args(&marker, &expected, &sensitive));
    assert_eq!(code, 0, "{response}; {err}");
    assert!(err.is_empty(), "{err}");
    assert_eq!(response["delivery"]["complete"], true);
    assert_machine_bytes(response["delivery"]["events"].as_array().unwrap());
    assert!(marker.with_extension("done").exists());
    let run = response["result"]["run"]["run_id"].as_str().unwrap();
    let history = json_ok(&root, &["run", "show", run]);
    assert!(!history.to_string().contains("full-live-text"));
    assert!(history.get("delivery").is_none());
}

// Test-ID: PR-TEST-0581
// Verifies: PR-REQ-0366, PR-REQ-0367, PR-REQ-0368
#[test]
fn machine_jsonl_is_live_and_closed_output_has_explicit_cancellation_policy() {
    struct Live { bytes: Vec<u8>, marker: PathBuf, observed: bool }
    impl Write for Live {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> { self.bytes.extend_from_slice(bytes); Ok(bytes.len()) }
        fn flush(&mut self) -> io::Result<()> {
            if !self.observed && self.bytes.windows(b"\"type\":\"output\"".len()).any(|b| b == b"\"type\":\"output\"") {
                assert!(!self.marker.with_extension("done").exists(), "output was buffered until completion");
                self.observed = true;
            }
            Ok(())
        }
    }
    let (temp, root, expected, sensitive) = prepared_cli_hook_instance_with_terminal("output");
    let marker = temp.path().join("live");
    let mut out = Live { bytes: Vec::new(), marker: marker.clone(), observed: false };
    let mut args: Vec<OsString> = ["--format", "jsonl"].map(Into::into).to_vec();
    args.extend(machine_args(&marker, &expected, &sensitive));
    let mut err = Vec::new();
    assert_eq!(run(args, Some(root.as_os_str().into()), &mut io::empty(), &mut out, &mut err), 0, "{}", String::from_utf8_lossy(&err));
    assert!(out.observed);
    let mut events: Vec<serde_json::Value> = out.bytes.split(|b| *b == b'\n').filter(|b| !b.is_empty()).map(|b| serde_json::from_slice(b).unwrap()).collect();
    let result = events.pop().unwrap();
    schema_tests::assert_event(&result);
    assert_eq!(result["type"], "result");
    assert_eq!(result["sequence"], (events.len() + 1).to_string());
    assert!(result["response"]["delivery"].get("events").is_none());
    schema_tests::assert_response(&result["response"]);
    assert_machine_bytes(&events);
    struct Closed;
    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> { Err(io::ErrorKind::BrokenPipe.into()) }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }
    for cancel in [false, true] {
        let marker = temp.path().join(if cancel { "cancel" } else { "continue" });
        let mut args: Vec<OsString> = ["--format", "jsonl"].map(Into::into).to_vec();
        args.extend(machine_args(&marker, &expected, &sensitive));
        if cancel { args.push("--cancel-on-output-close".into()); }
        assert_eq!(run(args, Some(root.as_os_str().into()), &mut io::empty(), &mut Closed, &mut err), 1);
        assert_eq!(marker.with_extension("done").exists(), !cancel);
        let list = json_ok(&root, &["run", "list"]);
        assert!(list["items"].as_array().unwrap().iter().all(|r| r["state"]["phase"] == "finished"));
    }
    struct FailedCapture { cancellation: ActionCancellation, bytes: Vec<u8> }
    impl Write for FailedCapture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.cancellation.delivery.get().unwrap().fail("storage_error");
            self.bytes.extend_from_slice(bytes); Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }
    let cancellation = ActionCancellation::default();
    let mut out = FailedCapture { cancellation: cancellation.clone(), bytes: Vec::new() };
    let marker = temp.path().join("incomplete-delivery");
    let mut args: Vec<OsString> = ["--format", "jsonl"].map(Into::into).to_vec();
    args.extend(machine_args(&marker, &expected, &sensitive));
    assert_eq!(run_with_cancellation(args, Some(root.as_os_str().into()), &mut io::empty(), &mut out, &mut err, &cancellation), 1);
    assert!(cancellation.delivery.get().is_none());
    let result: serde_json::Value = serde_json::from_slice(out.bytes.split(|b| *b == b'\n').rfind(|b| !b.is_empty()).unwrap()).unwrap();
    schema_tests::assert_event(&result);
    assert_eq!(result["response"]["status"], "failure");
    assert_eq!(result["response"]["delivery"]["complete"], false);
    assert_eq!(result["response"]["result"]["run"]["state"]["outcome"], "succeeded");
    assert!(marker.with_extension("done").exists());
}

// Test-ID: PR-TEST-0582
// Verifies: PR-REQ-0367, PR-REQ-0368
#[test]
fn machine_jsonl_queries_and_invalid_combinations_are_single_results() {
    let (temp, _, _) = cli_roots();
    let root = temp.path().join("uncreated");
    for (args, expected) in [
        (vec!["--format", "jsonl", "--version"], 0),
        (vec!["--format", "jsonl", "invoke", "node", "go", "--plan", "--cancel-on-output-close"], 2),
        (vec!["--format", "json", "invoke", "node", "go", "--cancel-on-output-close"], 2),
        (vec!["--format", "jsonl", "input", "export", "node", "x", "--output", "-"], 2),
    ] {
        let mut out = Vec::new(); let mut err = Vec::new();
        let code = run(args.iter().map(OsString::from).collect(), Some(root.as_os_str().into()), &mut io::empty(), &mut out, &mut err);
        assert_eq!(code, expected, "{args:?}: {} {}", String::from_utf8_lossy(&out), String::from_utf8_lossy(&err));
        let response: serde_json::Value = serde_json::from_slice(&out).unwrap();
        if args[1] == "jsonl" {
            schema_tests::assert_event(&response);
            assert_eq!(response["type"], "result");
            schema_tests::assert_response(&response["response"]);
        } else { schema_tests::assert_response(&response); }
        assert!(!root.exists());
    }
}

// Test-ID: PR-TEST-0584
// Verifies: PR-REQ-0368
#[test]
fn slow_machine_receiver_does_not_block_timeout_or_request_cancellation() {
    struct Slow { root: PathBuf, blocked: bool, bytes: Vec<u8> }
    impl Write for Slow {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.blocked {
                self.blocked = true;
                // Keep output blocked until durable finalization is observable.
                // A fixed sleep races parallel filesystem/SQLite work; the watchdog
                // still fails if finalization depends on this writer returning.
                let deadline = std::time::Instant::now() + Duration::from_secs(30);
                loop {
                    let list = json_ok(&self.root, &["run", "list"]);
                    assert_eq!(list["items"].as_array().unwrap().len(), 1);
                    if list["items"][0]["state"]["phase"] == "finished" {
                        assert_eq!(list["items"][0]["state"]["outcome"], "timed_out");
                        break;
                    }
                    assert!(std::time::Instant::now() < deadline,
                        "Run finalization must not wait for the blocked machine writer");
                    thread::sleep(Duration::from_millis(10));
                }
            }
            self.bytes.extend_from_slice(bytes); Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }
    let (temp, root, expected, sensitive) = prepared_cli_hook_instance_with_terminal("output");
    let marker = temp.path().join("slow");
    let mut args: Vec<OsString> = ["--format", "jsonl"].map(Into::into).to_vec();
    args.extend(machine_args(&marker, &expected, &sensitive));
    args.extend(["--cancel-on-output-close", "--action-timeout-ms", "30", "--termination-grace-ms", "10"].map(Into::into));
    let mut out = Slow { root: root.clone(), blocked: false, bytes: Vec::new() };
    assert_eq!(run(args, Some(root.as_os_str().into()), &mut io::empty(), &mut out, &mut Vec::new()), 1);
    assert!(out.blocked);
    let result: serde_json::Value = serde_json::from_slice(out.bytes.split(|b| *b == b'\n').rfind(|b| !b.is_empty()).unwrap()).unwrap();
    schema_tests::assert_event(&result);
    assert_eq!(result["response"]["result"]["run"]["state"]["outcome"], "timed_out");
}
