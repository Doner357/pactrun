//! M3 Slice 6: exercise the real `pactrun invoke` process boundary.
//!
//! The Hook is this integration-test executable, launched from a materialized
//! runtime copy. The test then reopens the storage with a separate Pactrun
//! process and inspects the durable Run.

use std::{
    env, fs,
    io::{self, Read, Write},
    net::TcpListener,
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::os::windows::{
    ffi::{OsStrExt, OsStringExt},
    process::CommandExt,
};

#[cfg(target_os = "linux")]
use std::os::unix::{
    ffi::{OsStrExt, OsStringExt},
    fs::PermissionsExt,
};

use serde_json::{Value, json};
use tempfile::TempDir;

const TRANSPORT_ENVIRONMENT: &str = "PACTRUN_HOOK_PROTOCOL_TRANSPORT";
const ENDPOINT_ENVIRONMENT: &str = "PACTRUN_HOOK_PROTOCOL_ENDPOINT";
const INTERACTIVE_ENVIRONMENT: &str = "PACTRUN_E2E_INTERACTIVE";
const READY_ENVIRONMENT: &str = "PACTRUN_E2E_READY";
const DESCENDANT_ENVIRONMENT: &str = "PACTRUN_E2E_DESCENDANT";
const CANCEL_ENVIRONMENT: &str = "PACTRUN_E2E_CANCEL_ACK";
#[cfg(windows)]
const LAUNCHER_REPORT_ENVIRONMENT: &str = "PACTRUN_E2E_LAUNCHER_REPORT";
const PREAMBLE: &[u8] = b"pactrun.hook-protocol\0\0\0\0\x01";

fn read_frame(stream: &mut impl Read) -> io::Result<Value> {
    let mut length = [0_u8; 4];
    stream.read_exact(&mut length)?;
    let mut payload = vec![0_u8; u32::from_be_bytes(length) as usize];
    stream.read_exact(&mut payload)?;
    serde_json::from_slice(&payload)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn write_frame(stream: &mut impl Write, value: &Value) -> io::Result<()> {
    let payload = serde_json::to_vec(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let length = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame too large"))?;
    stream.write_all(&length.to_be_bytes())?;
    stream.write_all(&payload)?;
    stream.flush()
}

#[cfg(unix)]
fn connect(endpoint: &str) -> io::Result<std::os::unix::net::UnixStream> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match std::os::unix::net::UnixStream::connect(endpoint) {
            Ok(stream) => return Ok(stream),
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(windows)]
fn connect(endpoint: &str) -> io::Result<std::fs::File> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match fs::OpenOptions::new().read(true).write(true).open(endpoint) {
            Ok(stream) => return Ok(stream),
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
}

#[test]
fn e2e_hook_worker() {
    let Some(transport) = env::var_os(TRANSPORT_ENVIRONMENT) else {
        return;
    };
    let endpoint = env::var(ENDPOINT_ENVIRONMENT).unwrap();
    let mut stream = connect(&endpoint).unwrap();
    let mut preamble = [0_u8; PREAMBLE.len()];
    stream.read_exact(&mut preamble).unwrap();
    assert_eq!(preamble, PREAMBLE);
    let session = read_frame(&mut stream).unwrap();
    assert_eq!(session["type"], "session_start");
    let session_id = session["session_id"].as_str().unwrap();
    stream.write_all(PREAMBLE).unwrap();
    write_frame(
        &mut stream,
        &json!({
            "type": "session_ready",
            "protocol_version": 1,
            "session_id": session_id,
        }),
    )
    .unwrap();
    #[cfg(windows)]
    if let Some(report_path) = env::var_os(LAUNCHER_REPORT_ENVIRONMENT) {
        let executable = env::current_exe().unwrap();
        fs::write(
            report_path,
            json!({
                "executable_utf16": executable.as_os_str().encode_wide().collect::<Vec<_>>(),
            })
            .to_string(),
        )
        .unwrap();
    }

    if env::var_os(INTERACTIVE_ENVIRONMENT).is_some() {
        println!("E2E_HOOK_READY");
        io::stdout().flush().unwrap();
        if let Some(path) = env::var_os(READY_ENVIRONMENT) {
            fs::write(path, b"ready").unwrap();
        }
        let descendant_path = env::var_os(DESCENDANT_ENVIRONMENT).unwrap();
        let descendant = Command::new(env::current_exe().unwrap())
            .args(["--exact", "e2e_descendant", "--nocapture"])
            .arg(&descendant_path)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !std::path::Path::new(&descendant_path).exists() {
            assert!(Instant::now() < deadline, "descendant did not become ready");
            thread::sleep(Duration::from_millis(10));
        }
        println!("E2E_HOOK_DESCENDANT_READY");
        io::stdout().flush().unwrap();
        #[cfg(unix)]
        {
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            assert_eq!(input, "e2e-input\n");
            println!("E2E_HOOK_READ");
            io::stdout().flush().unwrap();
        }
        // The owner must terminate this descendant through its process-tree
        // boundary; waiting here would hide that containment assertion.
        std::mem::forget(descendant);
    }
    if env::var_os(INTERACTIVE_ENVIRONMENT).is_some() {
        loop {
            let message = read_frame(&mut stream).unwrap();
            if message["type"] == "cancel" {
                if let Some(path) = env::var_os(CANCEL_ENVIRONMENT) {
                    fs::write(path, b"ack").unwrap();
                }
                write_frame(
                    &mut stream,
                    &json!({
                        "type": "cancel_ack",
                        "control_id": message["control_id"],
                    }),
                )
                .unwrap();
                break;
            }
        }
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    } else {
        write_frame(
            &mut stream,
            &json!({
                "type": "complete",
                "operation": "action",
                "status": "success",
                "produced_outputs": [],
            }),
        )
        .unwrap();
        let accepted = read_frame(&mut stream).unwrap();
        assert_eq!(accepted["type"], "completion_accepted");
    }
    assert!(!transport.into_string().unwrap().is_empty());
}

#[test]
fn e2e_descendant() {
    let Some(path) = env::args().nth(4) else {
        return;
    };
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    fs::write(path, listener.local_addr().unwrap().port().to_string()).unwrap();
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn setup_source(root: &std::path::Path) {
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    let worker_name = if cfg!(windows) {
        "worker.exe"
    } else {
        "worker"
    };
    fs::copy(env::current_exe().unwrap(), source.join(worker_name)).unwrap();
    fs::write(
        source.join("pactrun.yaml"),
        format!(
            r#"source_format: 1
package_id: 00000000000000000000000000000038
revision:
  inputs: []
  actions:
    - id: interactive
      access: observe
      parameters: []
      hook:
        protocol_version: 1
        launch: {{ kind: direct, executable: worker }}
        args: ["--exact", "e2e_hook_worker", "--nocapture"]
        io: {{ terminal: interactive }}
      outputs: []
  migrations: []
runtime_content:
  files:
    - {{ id: worker, source: {worker_name}, path: bin/{worker_name}, executable: true }}
portable_metadata:
  reference_labels:
    - label: stable
      source: {{ kind: unattributed }}
"#
        ),
    )
    .unwrap();
}

#[cfg(windows)]
fn setup_interpreter_source(root: &std::path::Path) {
    let source = root.join("source");
    fs::write(source.join("hook-script.txt"), b"interpreter hook script").unwrap();
    fs::write(
        source.join("pactrun.yaml"),
        r#"source_format: 1
package_id: 00000000000000000000000000000039
revision:
  inputs: []
  actions:
    - id: interactive
      access: observe
      parameters: []
      hook:
        protocol_version: 1
        launch:
          kind: interpreter
          command: pactrun-e2e-launcher.exe
          interpreter_args: ["--exact", "e2e_hook_worker", "--nocapture", "--skip"]
          script: hook_script
        args: []
        io: { terminal: interactive }
      outputs: []
  migrations: []
runtime_content:
  files:
    - { id: hook_script, source: hook-script.txt, path: bin/hook-script.txt }
portable_metadata:
  reference_labels:
    - label: stable
      source: { kind: unattributed }
"#,
    )
    .unwrap();
}

#[cfg(target_os = "linux")]
fn setup_interpreter_source(root: &std::path::Path) {
    let source = root.join("source");
    fs::write(source.join("hook-script.txt"), b"interpreter hook script").unwrap();
    fs::write(
        source.join("pactrun.yaml"),
        r#"source_format: 1
package_id: 00000000000000000000000000000040
revision:
  inputs: []
  actions:
    - id: interactive
      access: observe
      parameters: []
      hook:
        protocol_version: 1
        launch:
          kind: interpreter
          command: pactrun-e2e-launcher
          interpreter_args: ["--exact", "e2e_hook_worker", "--nocapture", "--skip"]
          script: hook_script
        args: []
        io: { terminal: interactive }
      outputs: []
  migrations: []
runtime_content:
  files:
    - { id: hook_script, source: hook-script.txt, path: bin/hook-script.txt }
portable_metadata:
  reference_labels:
    - label: stable
      source: { kind: unattributed }
"#,
    )
    .unwrap();
}

#[cfg(windows)]
// Test-ID: PR-TEST-0139
// Verifies: PR-REQ-0194, PR-REQ-0274
#[test]
fn real_cli_interpreter_launch_preserves_non_utf8_launcher_directory() {
    let (temporary, storage, source) = prepare_storage();
    setup_interpreter_source(temporary.path());

    let launcher_directory = temporary.path().join(std::ffi::OsString::from_wide(&[
        0x6c, 0x61, 0x75, 0x6e, 0xd800, 0x68, 0x65, 0x72,
    ]));
    assert!(launcher_directory.to_str().is_none());
    fs::create_dir(&launcher_directory).unwrap();
    let launcher = launcher_directory.join("pactrun-e2e-launcher.exe");
    fs::copy(env::current_exe().unwrap(), &launcher).unwrap();

    let installed = run_cli(
        &std::path::PathBuf::from(env!("CARGO_BIN_EXE_pactrun")),
        &storage,
        &["pack", "install", source.to_str().unwrap()],
    );
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let revision = String::from_utf8_lossy(&installed.stdout)
        .lines()
        .next()
        .unwrap()
        .to_owned();
    let created = run_cli(
        &std::path::PathBuf::from(env!("CARGO_BIN_EXE_pactrun")),
        &storage,
        &["instance", "create", "node", "--revision", &revision],
    );
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );

    let report_path = temporary.path().join("launcher-report.json");
    let path = env::join_paths([launcher_directory.as_os_str()]).unwrap();
    let cli = std::path::PathBuf::from(env!("CARGO_BIN_EXE_pactrun"));
    let invoked = Command::new(&cli)
        .args(["invoke", "node", "interactive"])
        .env("PACTRUN_STORAGE_ROOT", &storage)
        .env("PATH", &path)
        .env(LAUNCHER_REPORT_ENVIRONMENT, &report_path)
        .output()
        .unwrap();
    assert!(
        invoked.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&invoked.stdout),
        String::from_utf8_lossy(&invoked.stderr)
    );

    let report: Value = serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    assert_eq!(
        report["executable_utf16"],
        json!(launcher.as_os_str().encode_wide().collect::<Vec<_>>())
    );

    let listed = run_cli(&cli, &storage, &["run", "list", "node"]);
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let listing = String::from_utf8_lossy(&listed.stdout);
    assert_eq!(
        listing
            .lines()
            .skip(1)
            .take_while(|line| !line.is_empty())
            .count(),
        1,
        "{listing}"
    );
    assert!(listing.contains("succeeded"), "{listing}");
}

#[cfg(windows)]
// Test-ID: PR-TEST-0140
// Verifies: PR-REQ-0095, PR-REQ-0274, PR-REQ-0284
#[test]
fn real_cli_interpreter_plan_renders_non_utf8_launcher_directory_losslessly() {
    let (temporary, storage, source) = prepare_storage();
    setup_interpreter_source(temporary.path());

    let launcher_directory = temporary.path().join(std::ffi::OsString::from_wide(&[
        0x6c, 0x61, 0x6e, 0x75, 0xd800, 0x68, 0x65, 0x72,
    ]));
    assert!(launcher_directory.to_str().is_none());
    fs::create_dir(&launcher_directory).unwrap();
    let launcher = launcher_directory.join("pactrun-e2e-launcher.exe");
    fs::copy(env::current_exe().unwrap(), &launcher).unwrap();

    let cli = std::path::PathBuf::from(env!("CARGO_BIN_EXE_pactrun"));
    install_instance(&cli, &storage, &source);
    let before_database = fs::read(storage.join("database/pactrun.sqlite3")).unwrap();
    let before_staging = staging_entries(&storage);
    let path = env::join_paths([launcher_directory.as_os_str()]).unwrap();
    let planned = Command::new(&cli)
        .args(["invoke", "node", "interactive", "--plan"])
        .env("PACTRUN_STORAGE_ROOT", &storage)
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&planned.stdout),
        String::from_utf8_lossy(&planned.stderr)
    );
    let output = String::from_utf8_lossy(&planned.stdout);
    let expected = format!(
        "windows-utf16[{}]",
        launcher
            .as_os_str()
            .encode_wide()
            .map(|unit| format!("{unit:04x}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    assert!(
        output.contains(&format!("resolved_path: {expected}")),
        "{output}"
    );
    assert!(output.contains("windows-utf16["), "{output}");
    assert!(!output.contains('\u{fffd}'), "{output}");
    assert!(!temporary.path().join("launcher-report.json").exists());
    assert_eq!(
        fs::read(storage.join("database/pactrun.sqlite3")).unwrap(),
        before_database
    );
    assert_eq!(staging_entries(&storage), before_staging);

    let listed = run_cli(&cli, &storage, &["run", "list", "node"]);
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    assert!(String::from_utf8_lossy(&listed.stdout).contains("0 records shown."));
}

#[cfg(target_os = "linux")]
// Test-ID: PR-TEST-0141
// Verifies: PR-REQ-0095, PR-REQ-0274, PR-REQ-0284
#[test]
fn real_cli_interpreter_plan_renders_invalid_utf8_launcher_directory_losslessly() {
    let (temporary, storage, source) = prepare_storage();
    setup_interpreter_source(temporary.path());

    let launcher_directory = temporary.path().join(std::ffi::OsString::from_vec(vec![
        b'l', b'a', b'n', b'u', 0xff, b'h', b'e', b'r',
    ]));
    assert!(launcher_directory.to_str().is_none());
    fs::create_dir(&launcher_directory).unwrap();
    let launcher = launcher_directory.join("pactrun-e2e-launcher");
    fs::copy(env::current_exe().unwrap(), &launcher).unwrap();
    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755)).unwrap();

    let cli = std::path::PathBuf::from(env!("CARGO_BIN_EXE_pactrun"));
    install_instance(&cli, &storage, &source);
    let before_database = fs::read(storage.join("database/pactrun.sqlite3")).unwrap();
    let before_staging = staging_entries(&storage);
    let path = env::join_paths([launcher_directory.as_os_str()]).unwrap();
    let planned = Command::new(&cli)
        .args(["invoke", "node", "interactive", "--plan"])
        .env("PACTRUN_STORAGE_ROOT", &storage)
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&planned.stdout),
        String::from_utf8_lossy(&planned.stderr)
    );
    let output = String::from_utf8_lossy(&planned.stdout);
    let expected = format!(
        "unix-bytes[{}]",
        launcher
            .as_os_str()
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    assert!(
        output.contains(&format!("resolved_path: {expected}")),
        "{output}"
    );
    assert!(output.contains("unix-bytes["), "{output}");
    assert!(!output.contains('\u{fffd}'), "{output}");
    assert_eq!(
        fs::read(storage.join("database/pactrun.sqlite3")).unwrap(),
        before_database
    );
    assert_eq!(staging_entries(&storage), before_staging);

    let listed = run_cli(&cli, &storage, &["run", "list", "node"]);
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    assert!(String::from_utf8_lossy(&listed.stdout).contains("0 records shown."));
}

fn install_instance(cli: &std::path::Path, storage: &std::path::Path, source: &std::path::Path) {
    let installed = run_cli(cli, storage, &["pack", "install", source.to_str().unwrap()]);
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let revision = String::from_utf8_lossy(&installed.stdout)
        .lines()
        .next()
        .unwrap()
        .to_owned();
    let created = run_cli(
        cli,
        storage,
        &["instance", "create", "node", "--revision", &revision],
    );
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
}

fn staging_entries(storage: &std::path::Path) -> Vec<std::ffi::OsString> {
    let mut entries = fs::read_dir(storage.join("staging"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn run_cli(
    cli: &std::path::Path,
    storage: &std::path::Path,
    args: &[&str],
) -> std::process::Output {
    Command::new(cli)
        .args(args)
        .env("PACTRUN_STORAGE_ROOT", storage)
        .output()
        .unwrap()
}

fn assert_descendant_is_gone(path: &std::path::Path) {
    let port = fs::read_to_string(path)
        .unwrap()
        .trim()
        .parse::<u16>()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "descendant survived tree termination"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn prepare_storage() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("m3-real-cli-e2e");
    fs::create_dir_all(&parent).unwrap();
    let temporary = tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(parent)
        .unwrap();
    let storage = temporary.path().join("storage");
    fs::create_dir(&storage).unwrap();
    fs::create_dir(storage.join("database")).unwrap();
    fs::create_dir(storage.join("runtime-content")).unwrap();
    fs::create_dir(storage.join("staging")).unwrap();
    setup_source(temporary.path());
    let source = temporary.path().join("source");
    (temporary, storage, source)
}

#[cfg(target_os = "linux")]
// Test-ID: PR-TEST-0131
// Verifies: PR-REQ-0094, PR-REQ-0284, PR-REQ-0286
#[test]
fn real_cli_interactive_invoke_reopens_a_durable_run() {
    let (temporary, storage, source) = prepare_storage();
    let cli = std::path::PathBuf::from(env!("CARGO_BIN_EXE_pactrun"));
    let installed = run_cli(
        &cli,
        &storage,
        &["pack", "install", source.to_str().unwrap()],
    );
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let revision = String::from_utf8_lossy(&installed.stdout)
        .lines()
        .next()
        .unwrap()
        .to_owned();
    let created = run_cli(
        &cli,
        &storage,
        &["instance", "create", "node", "--revision", &revision],
    );
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );

    let ready_path = temporary.path().join("hook-ready");
    let descendant_path = temporary.path().join("descendant-port");
    let cancel_path = temporary.path().join("cancel-ack");
    let driver = r#"
import os, pty, select, sys, time
cli, storage, ready, descendant, cancel_ack = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5]
pid, fd = pty.fork()
if pid == 0:
    os.environ['PACTRUN_STORAGE_ROOT'] = storage
    os.environ['PACTRUN_E2E_INTERACTIVE'] = '1'
    os.environ['PACTRUN_E2E_READY'] = ready
    os.environ['PACTRUN_E2E_DESCENDANT'] = descendant
    os.environ['PACTRUN_E2E_CANCEL_ACK'] = cancel_ack
    os.execv(cli, [cli, 'invoke', 'node', 'interactive', '--termination-grace-ms', '50'])

pending = b''
def wait_for(needle):
    global pending
    deadline = time.monotonic() + 30
    while needle not in pending:
        if time.monotonic() >= deadline:
            raise RuntimeError('PTY timeout waiting for ' + repr(needle) + ': ' + repr(pending))
        ready, _, _ = select.select([fd], [], [], 0.1)
        if ready:
            try:
                chunk = os.read(fd, 4096)
            except OSError as error:
                raise RuntimeError('PTY closed before ' + repr(needle)) from error
            if not chunk:
                raise RuntimeError('PTY EOF before ' + repr(needle))
            pending += chunk
    end = pending.index(needle) + len(needle)
    matched, pending = pending[:end], pending[end:]
    return matched

# A read may coalesce both readiness lines. Consume only the requested prefix.
pending = b'FIRST\nSECOND\n'
assert wait_for(b'FIRST') == b'FIRST'
assert wait_for(b'SECOND') == b'\nSECOND'
assert pending == b'\n'
pending = b''

wait_for(b'E2E_HOOK_READY')
wait_for(b'E2E_HOOK_DESCENDANT_READY')
os.write(fd, b'e2e-input\n')
wait_for(b'E2E_HOOK_READ')
os.write(fd, b'\x03')
_, status = os.waitpid(pid, 0)
if not os.WIFEXITED(status) or os.WEXITSTATUS(status) != 1:
    raise RuntimeError('pactrun invoke failed: ' + repr(status))
"#;
    let result = Command::new("python3")
        .arg("-c")
        .arg(driver)
        .arg(&cli)
        .arg(&storage)
        .arg(&ready_path)
        .arg(&descendant_path)
        .arg(&cancel_path)
        .env("PACTRUN_E2E_INTERACTIVE", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_descendant_is_gone(&descendant_path);
    assert!(
        cancel_path.exists(),
        "the owner did not send a cancel frame"
    );

    let listed = run_cli(&cli, &storage, &["run", "list", "node"]);
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let listing = String::from_utf8_lossy(&listed.stdout);
    assert_eq!(
        listing
            .lines()
            .skip(1)
            .take_while(|line| !line.is_empty())
            .count(),
        1,
        "{listing}"
    );
    assert!(listing.contains("cancelled"), "{listing}");
    drop(temporary);
}

#[cfg(windows)]
// Test-ID: PR-TEST-0133
// Verifies: PR-REQ-0094, PR-REQ-0284, PR-REQ-0286
#[test]
fn real_cli_invoke_reopens_a_durable_run() {
    let (temporary, storage, source) = prepare_storage();
    let cli = std::path::PathBuf::from(env!("CARGO_BIN_EXE_pactrun"));
    let installed = run_cli(
        &cli,
        &storage,
        &["pack", "install", source.to_str().unwrap()],
    );
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let revision = String::from_utf8_lossy(&installed.stdout)
        .lines()
        .next()
        .unwrap()
        .to_owned();
    let created = run_cli(
        &cli,
        &storage,
        &["instance", "create", "node", "--revision", &revision],
    );
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let ready_path = temporary.path().join("hook-ready");
    let descendant_path = temporary.path().join("descendant-port");
    let cancel_path = temporary.path().join("cancel-ack");
    let mut driver = Command::new(env::current_exe().unwrap())
        .args(["--exact", "e2e_windows_console_driver", "--nocapture"])
        .arg(&cli)
        .arg(&storage)
        .arg(&ready_path)
        .arg(&descendant_path)
        .arg(&cancel_path)
        .creation_flags(0x00000010)
        .spawn()
        .unwrap();
    let driver_status = driver.wait().unwrap();
    assert!(driver_status.success());
    assert_descendant_is_gone(&descendant_path);
    let listed = run_cli(&cli, &storage, &["run", "list", "node"]);
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let listing = String::from_utf8_lossy(&listed.stdout);
    assert_eq!(
        listing
            .lines()
            .skip(1)
            .take_while(|line| !line.is_empty())
            .count(),
        1,
        "{listing}"
    );
    assert!(listing.contains("cancelled"), "{listing}");
}

#[cfg(windows)]
#[test]
fn e2e_windows_console_driver() {
    let args: Vec<_> = env::args().skip(1).collect();
    let Some(index) = args.iter().position(|arg| arg == "--nocapture") else {
        return;
    };
    // This test is also the child driver launched by the real E2E test. When
    // the target is run directly, the ordinary libtest invocation contains
    // `--nocapture` but no driver payload; that invocation must be a no-op.
    let driver_args = &args[index + 1..];
    if driver_args.is_empty() {
        return;
    }
    assert_eq!(
        driver_args.len(),
        5,
        "console driver payload must contain cli, storage, ready, descendant, and cancel paths"
    );
    let cli = &driver_args[0];
    let storage = &driver_args[1];
    let ready = &driver_args[2];
    let descendant = &driver_args[3];

    pactrun_windows_ntfs::allocate_console().unwrap();
    // Keep this driver alive after the generated event. The real Pactrun
    // process installs its own handler after it starts, while its internal
    // interactive adapter applies the child-side ignore policy.
    ctrlc::set_handler(|| {}).unwrap();
    let mut invoked = Command::new(cli)
        .args([
            "invoke",
            "node",
            "interactive",
            "--termination-grace-ms",
            "50",
        ])
        .env("PACTRUN_STORAGE_ROOT", storage)
        .env(INTERACTIVE_ENVIRONMENT, "1")
        .env(READY_ENVIRONMENT, ready)
        .env(DESCENDANT_ENVIRONMENT, descendant)
        .env(CANCEL_ENVIRONMENT, &driver_args[4])
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !std::path::Path::new(ready).exists() {
        assert!(
            Instant::now() < deadline,
            "console Hook did not become ready"
        );
        thread::sleep(Duration::from_millis(10));
    }
    while !std::path::Path::new(descendant).exists() {
        assert!(
            Instant::now() < deadline,
            "console descendant did not become ready"
        );
        thread::sleep(Duration::from_millis(10));
    }
    pactrun_windows_ntfs::generate_console_ctrl_c().unwrap();
    let status = invoked.wait().unwrap();
    assert_eq!(status.code(), Some(1));
}
