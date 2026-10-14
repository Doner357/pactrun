//! Actual-process Core evidence, without using the caller's diagnostic locations.
use serde_json::Value;
use std::{
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{Duration, Instant},
};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    config: PathBuf,
    data: PathBuf,
    state: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/core-trace-cli-tests");
        fs::create_dir_all(&parent).unwrap();
        let temp = tempfile::tempdir_in(parent).unwrap();
        Self {
            root: temp.path().join("store"),
            config: temp.path().join("config"),
            data: temp.path().join("data"),
            state: temp.path().join("state"),
            _temp: temp,
        }
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_pactrun"));
        c.env("PACTRUN_STORAGE_ROOT", &self.root)
            .env("APPDATA", &self.config)
            .env("XDG_CONFIG_HOME", &self.config)
            .env("LOCALAPPDATA", &self.data)
            .env("XDG_DATA_HOME", &self.data)
            .env("XDG_STATE_HOME", &self.state);
        c
    }
    fn json(&self, args: &[&str]) -> (Output, Value) {
        let output = self
            .command()
            .args(["--format", "json"])
            .args(args)
            .output()
            .unwrap();
        let value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&output.stdout)));
        (output, value)
    }
}

// Test-ID: PR-TEST-0762
// Verifies: PR-REQ-0397, PR-REQ-0398, PR-REQ-0399
#[test]
fn startup_trace_records_refusal_without_store_creation_or_raw_arguments() {
    let fixture = Fixture::new();
    let (out, _) = fixture.json(&["diagnostic", "list", "--startup"]);
    assert!(out.status.success());
    assert!(!fixture.root.exists() && !fixture.data.exists() && !fixture.state.exists());
    let (out, refused) = fixture.json(&["instance", "show", "private-argument-sentinel"]);
    assert!(!out.status.success());
    assert!(!fixture.root.exists());
    let (out, listed) = fixture.json(&["diagnostic", "list", "--startup"]);
    assert!(out.status.success());
    let items = listed["result"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let id = items[0]["id"].as_str().unwrap();
    assert_eq!(refused["diagnostic_id"], id);
    let (out, shown) = fixture.json(&["diagnostic", "show", id, "--startup"]);
    assert!(out.status.success());
    let record = &shown["result"];
    assert_eq!(record["closed"], true);
    assert_eq!(record["exit_code"], 1);
    assert_eq!(record["totals"]["store_open"]["failed"], "1");
    assert!(record["run_id"].is_null());
    assert!(!record.to_string().contains("private-argument-sentinel"));
    fs::create_dir_all(fixture.config.join("pactrun")).unwrap();
    fs::write(
        fixture.config.join("pactrun/config.toml"),
        "unknown = 'private-config-sentinel'",
    )
    .unwrap();
    let (out, listed) = fixture.json(&["diagnostic", "list", "--startup"]);
    assert!(
        out.status.success(),
        "diagnostic inspection must work with invalid configuration"
    );
    assert_eq!(
        listed["result"]["items"].as_array().unwrap().len(),
        1,
        "inspection logged itself"
    );
    assert!(!fixture.root.exists());
    let (out, refused) = fixture.json(&["--unknown-private-argument"]);
    assert_eq!(out.status.code(), Some(2));
    let (out, shown) = fixture.json(&[
        "diagnostic",
        "show",
        refused["diagnostic_id"].as_str().unwrap(),
        "--startup",
    ]);
    assert!(out.status.success(), "{shown}");
    assert_eq!(shown["result"]["closed"], true);
    assert_eq!(shown["result"]["exit_code"], 2);
    assert_eq!(
        shown["result"]["totals"]["output_delivery"]["finished"],
        "1"
    );
    assert_eq!(shown["result"]["totals"]["output_delivery"]["failed"], "0");
    assert!(
        !shown["result"]
            .to_string()
            .contains("unknown-private-argument")
    );
}

// Test-ID: PR-TEST-0763
// Verifies: PR-REQ-0397, PR-REQ-0399
#[test]
fn accepted_run_links_core_trace_without_recording_hook_terminal_text() {
    let fixture = Fixture::new();
    let source = fixture._temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let worker = if cfg!(windows) {
        "worker.exe"
    } else {
        "worker"
    };
    fs::copy(env::current_exe().unwrap(), source.join(worker)).unwrap();
    fs::write(source.join("pactrun.yaml"), format!("source_format: 1.0-alpha.3\npackage_id: 000000000000000000000000000000a8\nrevision:\n  inputs: []\n  actions:\n    - id: inspect\n      access: observe\n      parameters: []\n      outputs: []\n      hook:\n        protocol_version: 1.0-alpha.2\n        launch: {{kind: direct, executable: worker}}\n        args: ['--exact', 'trace_native_worker', '--nocapture', '--test-threads=1']\n        io: {{terminal: output}}\nruntime_content:\n  files:\n    - {{id: worker, source: {worker}, path: bin/{worker}, executable: true}}\n")).unwrap();
    let installed = fixture
        .command()
        .args(["--format", "json", "pack", "install"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stdout)
    );
    let installed: Value = serde_json::from_slice(&installed.stdout).unwrap();
    let revision = installed["result"]["reference"].as_str().unwrap();
    assert!(
        fixture
            .json(&["instance", "create", "trace-node", "--revision", revision])
            .0
            .status
            .success()
    );
    let (out, planned) = fixture.json(&["invoke", "trace-node", "inspect", "--plan"]);
    assert!(out.status.success(), "{planned}");
    let (out, shown) = fixture.json(&[
        "diagnostic",
        "show",
        planned["diagnostic_id"].as_str().unwrap(),
        "--startup",
    ]);
    assert!(out.status.success(), "{shown}");
    assert_eq!(shown["result"]["totals"]["compilation"]["finished"], "1");
    assert!(shown["result"]["totals"]["admission"].is_null());
    assert!(shown["result"]["run_id"].is_null());
    let (out, invoked) = fixture.json(&["invoke", "trace-node", "inspect"]);
    assert!(out.status.success(), "{invoked}");
    let run = invoked["result"]["run"]["run_id"].as_str().unwrap();
    let (out, shown) = fixture.json(&["diagnostic", "show", "--run", run]);
    assert!(out.status.success(), "{shown}");
    let trace = &shown["result"];
    assert_eq!(invoked["diagnostic_id"], trace["id"]);
    assert_eq!(trace["run_id"], run);
    assert_eq!(trace["closed"], true);
    for phase in [
        "compilation",
        "admission",
        "output_publication",
        "result_commit",
    ] {
        assert_eq!(trace["totals"][phase]["finished"], "1");
        assert_eq!(trace["totals"][phase]["failed"], "0");
        assert_eq!(trace["totals"][phase]["unfinished"], "0");
    }
    assert_eq!(trace["totals"]["hook_launch"]["finished"], "1");
    assert_eq!(trace["totals"]["hook_execution"]["finished"], "1");
    assert!(!trace.to_string().contains("private-hook-output-sentinel"));
    assert_eq!(
        fixture.json(&["run", "show", run]).1["result"]["run"]["state"]["outcome"],
        "succeeded"
    );
}

// Native fixture avoids altering host PowerShell execution policy.
#[test]
fn trace_native_worker() {
    let Ok(endpoint) = env::var("PACTRUN_HOOK_PROTOCOL_ENDPOINT") else {
        return;
    };
    #[cfg(windows)]
    let mut stream = {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&endpoint)
            {
                Ok(stream) => break stream,
                Err(e) if e.raw_os_error() == Some(231) && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("native protocol connection failed: {e}"),
            }
        }
    };
    #[cfg(unix)]
    let mut stream = {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match std::os::unix::net::UnixStream::connect(&endpoint) {
                Ok(stream) => break stream,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                    ) && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("native protocol connection failed: {e}"),
            }
        }
    };
    fn read(stream: &mut impl Read) -> Value {
        let mut length = [0; 4];
        stream.read_exact(&mut length).unwrap();
        let length = u32::from_be_bytes(length) as usize;
        assert!(length <= 16 * 1024 * 1024);
        let mut bytes = vec![0; length];
        stream.read_exact(&mut bytes).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
    fn write(stream: &mut impl Write, value: Value) {
        let bytes = serde_json::to_vec(&value).unwrap();
        stream
            .write_all(&(bytes.len() as u32).to_be_bytes())
            .unwrap();
        stream.write_all(&bytes).unwrap();
        stream.flush().unwrap();
    }
    const PREAMBLE: &[u8] = b"pactrun.hook-protocol\0\0\x0b1.0-alpha.2";
    let mut preamble = vec![0; PREAMBLE.len()];
    stream.read_exact(&mut preamble).unwrap();
    assert_eq!(preamble, PREAMBLE);
    let session = read(&mut stream);
    assert_eq!(session["type"], "session_start");
    stream.write_all(PREAMBLE).unwrap();
    write(
        &mut stream,
        serde_json::json!({"type":"session_ready","protocol_version":"1.0-alpha.2","session_id":session["session_id"]}),
    );
    println!("private-hook-output-sentinel");
    std::io::stdout().flush().unwrap();
    write(
        &mut stream,
        serde_json::json!({"type":"complete","operation":"action","status":"success","produced_outputs":[]}),
    );
    assert_eq!(read(&mut stream)["type"], "completion_accepted");
}
