//! Real Windows process probes, with all files confined to the workspace.

use std::{
    env,
    ffi::OsString,
    fs,
    io::{Read, Write},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use super::platform::{ProcessSupervisor, ProtocolListener};
use crate::domain::TerminalContractV1;
use crate::hook::ActionCancellation;

const WORKER: &str = "hook::windows_tests::windows_process_probe";
const MODE: &str = "pactrun-process-probe:";
const TRANSPORT: &str = "PACTRUN_HOOK_PROTOCOL_TRANSPORT";
const ENDPOINT: &str = "PACTRUN_HOOK_PROTOCOL_ENDPOINT";

fn arguments(mode: &str, directory: &Path) -> Vec<String> {
    ["--exact", WORKER, "--nocapture", "--test-threads=1"]
        .into_iter()
        .map(str::to_owned)
        .chain([
            format!("{MODE}{mode}"),
            directory.to_str().unwrap().to_owned(),
        ])
        .collect()
}

fn temporary() -> tempfile::TempDir {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("windows-launch-tests");
    fs::create_dir_all(&parent).unwrap();
    tempfile::Builder::new()
        .prefix("launch-")
        .tempdir_in(parent)
        .unwrap()
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !predicate() {
        assert!(Instant::now() < deadline, "Windows process probe timed out");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn windows_process_probe() {
    let args: Vec<String> = env::args_os()
        .skip(1)
        .map(|argument| argument.into_string().expect("probe arguments are UTF-8"))
        .collect();
    let Some(index) = args.iter().position(|arg| arg.starts_with(MODE)) else {
        return;
    };
    let mode = args[index].strip_prefix(MODE).unwrap();
    let directory = PathBuf::from(&args[index + 1]);
    match mode {
        "report" => {
            let executable = env::current_exe().unwrap();
            fs::write(
                directory.join("report.json"),
                json!({
                    "args": args,
                    "executable_utf16": executable.as_os_str().encode_wide().collect::<Vec<_>>(),
                    "transport": env::var(TRANSPORT).unwrap(),
                    "endpoint": env::var(ENDPOINT).unwrap(),
                    "parent": env::var("PACTRUN_TEST_PARENT").ok(),
                    "cwd": env::current_dir().unwrap(),
                })
                .to_string(),
            )
            .unwrap();
            // 259 is STILL_ACTIVE only when a process has not terminated.
            std::process::exit(259);
        }
        "tree" => {
            // Spawn before protocol setup or any application-level handshake.
            let mut child = Command::new(env::current_exe().unwrap())
                .args(arguments("descendant", &directory))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let _ = child.wait();
        }
        "descendant" => {
            let _live = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .share_mode(0)
                .open(directory.join("live.lock"))
                .unwrap();
            fs::write(directory.join("ready"), b"ready").unwrap();
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }
        "terminal" => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input).unwrap();
            fs::write(directory.join("stdin"), input).unwrap();
            print!("HOOK-STDOUT");
            eprint!("HOOK-STDERR");
        }
        "driver" => {
            // An isolated parent supplies ambient values and real terminal
            // handles without mutating this test runner's global environment.
            let listener = ProtocolListener::bind().unwrap();
            let mut args = arguments("report", &directory);
            args.extend(
                [
                    "",
                    "two words",
                    "tab\tvalue",
                    "\"",
                    "x\\\"y",
                    "ends with slash\\",
                    "\\\\",
                    "\u{53f0}\u{7063}\u{1f680}",
                ]
                .into_iter()
                .map(str::to_owned),
            );
            let mut child = ProcessSupervisor::spawn(
                &env::current_exe().unwrap(),
                &args,
                TerminalContractV1::None,
                &listener,
            )
            .unwrap();
            assert_eq!(child.wait().unwrap().code(), Some(259));
            assert_eq!(child.try_wait().unwrap().unwrap().code(), Some(259));
            let report: Value =
                serde_json::from_slice(&fs::read(directory.join("report.json")).unwrap()).unwrap();
            assert_eq!(report["args"], json!(args));
            assert_eq!(report["transport"], "windows-named-pipe");
            assert_eq!(report["endpoint"], listener.endpoint());
            assert_eq!(report["parent"], "inherited-\u{53f0}\u{7063}");
            assert_eq!(report["cwd"], json!(env::current_dir().unwrap()));
            for (name, terminal) in [
                ("none", TerminalContractV1::None),
                ("output", TerminalContractV1::Output),
                ("interactive", TerminalContractV1::Interactive),
            ] {
                let directory = directory.join(name);
                fs::create_dir(&directory).unwrap();
                let mut child = ProcessSupervisor::spawn(
                    &env::current_exe().unwrap(),
                    &arguments("terminal", &directory),
                    terminal,
                    &listener,
                )
                .unwrap();
                assert!(child.wait().unwrap().success());
                assert_eq!(
                    fs::read_to_string(directory.join("stdin")).unwrap(),
                    if name == "interactive" {
                        "parent-input"
                    } else {
                        ""
                    }
                );
            }
        }
        "console-driver" => {
            // The outer test creates an isolated real console for this
            // process. The helper is idempotent for that already-attached
            // console and also makes the driver usable when run directly.
            pactrun_windows_ntfs::allocate_console().unwrap();
            let cancellation = ActionCancellation::default();
            let signal_cancellation = cancellation.clone();
            ctrlc::set_handler(move || signal_cancellation.request()).unwrap();
            let listener = ProtocolListener::bind().unwrap();
            let mut child = ProcessSupervisor::spawn(
                &env::current_exe().unwrap(),
                &arguments("console-hook", &directory),
                TerminalContractV1::Interactive,
                &listener,
            )
            .unwrap();
            fs::write(directory.join("console-driver-started"), b"started").unwrap();
            wait_until(|| directory.join("console-hook-ready").exists());
            pactrun_windows_ntfs::generate_console_ctrl_c().unwrap();
            wait_until(|| cancellation.is_requested());
            child.terminate_tree().unwrap();
            assert!(!child.wait().unwrap().success());
            fs::write(directory.join("console-driver-complete"), b"complete").unwrap();
        }
        "console-hook" => {
            // This ordinary Hook installs no CTRL+C handler. The internal
            // interactive adapter applies the inherited console-ignore flag;
            // only the owner receives and arbitrates the generated event.
            fs::write(directory.join("console-hook-ready"), b"ready").unwrap();
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }
        _ => panic!("unknown process probe mode"),
    }
}

// Test-ID: PR-TEST-0102
// Verifies: PR-REQ-0172, PR-REQ-0194, PR-REQ-0280
#[test]
fn windows_native_launch_preserves_arguments_environment_and_terminal_mappings() {
    let temporary = temporary();
    let output = fs::File::create(temporary.path().join("stdout")).unwrap();
    let error = fs::File::create(temporary.path().join("stderr")).unwrap();
    let mut driver = Command::new(env::current_exe().unwrap())
        .args(arguments("driver", temporary.path()))
        .env("pactrun_hook_protocol_transport", "ambient-transport")
        .env("pactrun_hook_protocol_endpoint", "ambient-endpoint")
        .env("PACTRUN_TEST_PARENT", "inherited-\u{53f0}\u{7063}")
        .stdin(Stdio::piped())
        .stdout(output)
        .stderr(error)
        .spawn()
        .unwrap();
    driver
        .stdin
        .take()
        .unwrap()
        .write_all(b"parent-input")
        .unwrap();
    assert!(
        driver.wait().unwrap().success(),
        "{}",
        fs::read_to_string(temporary.path().join("stderr")).unwrap()
    );
    let stdout = fs::read_to_string(temporary.path().join("stdout")).unwrap();
    let stderr = fs::read_to_string(temporary.path().join("stderr")).unwrap();
    assert_eq!(stdout.matches("HOOK-STDOUT").count(), 2);
    assert_eq!(stderr.matches("HOOK-STDERR").count(), 2);
}

// Test-ID: PR-TEST-0103
// Verifies: PR-REQ-0052
#[test]
fn windows_startup_descendant_terminates_with_the_supervised_tree() {
    let temporary = temporary();
    let listener = ProtocolListener::bind().unwrap();
    let mut child = ProcessSupervisor::spawn(
        &env::current_exe().unwrap(),
        &arguments("tree", temporary.path()),
        TerminalContractV1::None,
        &listener,
    )
    .unwrap();
    wait_until(|| temporary.path().join("ready").exists());
    assert!(child.try_wait().unwrap().is_none());
    let open = || {
        fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(temporary.path().join("live.lock"))
    };
    assert!(
        open().is_err(),
        "descendant holds its exclusive live-process handle"
    );
    child.terminate_tree().unwrap();
    assert!(!child.wait().unwrap().success());
    wait_until(|| open().is_ok());
}

// Test-ID: PR-TEST-0134
// Verifies: PR-REQ-0194
#[test]
fn windows_interactive_adapter_launches_exact_native_image() {
    let temporary = temporary();
    let listener = ProtocolListener::bind().unwrap();
    let args = arguments("report", temporary.path());
    let mut child = ProcessSupervisor::spawn(
        &env::current_exe().unwrap(),
        &args,
        TerminalContractV1::Interactive,
        &listener,
    )
    .unwrap();

    assert_eq!(child.wait().unwrap().code(), Some(259));
    let report: Value =
        serde_json::from_slice(&fs::read(temporary.path().join("report.json")).unwrap()).unwrap();
    assert_eq!(report["args"], json!(args));
}

// Test-ID: PR-TEST-0135
// Verifies: PR-REQ-0194
#[test]
fn windows_interactive_adapter_rejects_non_image_batch_without_running_body() {
    let temporary = temporary();
    let marker = temporary.path().join("batch-ran");
    let batch = temporary.path().join("hook.cmd");
    fs::write(
        &batch,
        format!(
            "@echo script-ran>\"{}\"\r\n@exit /b 0\r\n",
            marker.display()
        ),
    )
    .unwrap();
    let listener = ProtocolListener::bind().unwrap();
    let mut child = ProcessSupervisor::spawn(
        &batch,
        &arguments("report", temporary.path()),
        TerminalContractV1::Interactive,
        &listener,
    )
    .unwrap();

    assert_eq!(child.wait().unwrap().code(), Some(1));
    assert!(!marker.exists(), "the batch body was executed");
    assert!(!temporary.path().join("report.json").exists());
}

// Test-ID: PR-TEST-0136
// Verifies: PR-REQ-0194
#[test]
fn windows_interactive_adapter_launches_native_image_with_batch_suffix() {
    let temporary = temporary();
    let renamed = temporary.path().join("native-hook.bat");
    fs::copy(env::current_exe().unwrap(), &renamed).unwrap();
    let listener = ProtocolListener::bind().unwrap();
    let args = arguments("report", temporary.path());
    let mut child =
        ProcessSupervisor::spawn(&renamed, &args, TerminalContractV1::Interactive, &listener)
            .unwrap();

    assert_eq!(child.wait().unwrap().code(), Some(259));
    let report: Value =
        serde_json::from_slice(&fs::read(temporary.path().join("report.json")).unwrap()).unwrap();
    assert_eq!(report["args"], json!(args));
}

// Test-ID: PR-TEST-0137
// Verifies: PR-REQ-0194
#[test]
fn windows_interactive_adapter_does_not_fallback_to_batch_sibling() {
    let temporary = temporary();
    let marker = temporary.path().join("batch-ran");
    let candidate = temporary.path().join("foo.cmd");
    let sibling = temporary.path().join("foo.exe");
    fs::write(
        &candidate,
        format!(
            "@echo script-ran>\"{}\"\r\n@exit /b 0\r\n",
            marker.display()
        ),
    )
    .unwrap();
    fs::copy(env::current_exe().unwrap(), &sibling).unwrap();
    let listener = ProtocolListener::bind().unwrap();
    let mut child = ProcessSupervisor::spawn(
        &candidate,
        &arguments("report", temporary.path()),
        TerminalContractV1::Interactive,
        &listener,
    )
    .unwrap();

    assert_eq!(child.wait().unwrap().code(), Some(1));
    assert!(!marker.exists(), "the exact batch candidate was executed");
    assert!(!temporary.path().join("report.json").exists());
}

// Test-ID: PR-TEST-0138
// Verifies: PR-REQ-0194, PR-REQ-0274
// Lower-level exact-launch coverage; PR-TEST-0139 covers the production
// adapter argv -> env::args_os() boundary.
#[test]
fn windows_interactive_adapter_exact_launcher_preserves_non_utf8_roundtrippable_pathname() {
    let temporary = temporary();
    let native_name = OsString::from_wide(&[0x68, 0x6f, 0x6f, 0xd800, 0x2e, 0x65, 0x78, 0x65]);
    let native_path = temporary.path().join(native_name);
    assert!(native_path.to_str().is_none());

    // A replacement-character sibling makes lossy UTF-8 conversion observable
    // as a wrong executable identity rather than a false-positive launch.
    let fallback_path = temporary.path().join("hook-\u{fffd}.exe");
    fs::copy(env::current_exe().unwrap(), &fallback_path).unwrap();
    fs::copy(env::current_exe().unwrap(), &native_path).unwrap();

    let listener = ProtocolListener::bind().unwrap();
    let args = arguments("report", temporary.path());
    let mut child = ProcessSupervisor::spawn(
        &native_path,
        &args,
        TerminalContractV1::Interactive,
        &listener,
    )
    .unwrap();

    assert_eq!(child.wait().unwrap().code(), Some(259));
    let report: Value =
        serde_json::from_slice(&fs::read(temporary.path().join("report.json")).unwrap()).unwrap();
    let expected_executable = native_path.as_os_str().encode_wide().collect::<Vec<_>>();
    assert_eq!(report["args"], json!(args));
    assert_eq!(report["executable_utf16"], json!(expected_executable));
}

// Test-ID: PR-TEST-0125
// Verifies: PR-REQ-0194, PR-REQ-0286
#[test]
fn windows_real_console_ctrl_c_reaches_the_owner_and_not_the_interactive_hook() {
    use std::os::windows::process::CommandExt;

    let temporary = temporary();
    let output_path = temporary.path().join("console-stdout");
    let error_path = temporary.path().join("console-stderr");
    let output = fs::File::create(&output_path).unwrap();
    let error = fs::File::create(&error_path).unwrap();
    let mut driver = Command::new(env::current_exe().unwrap())
        .args(arguments("console-driver", temporary.path()))
        .creation_flags(0x00000010) // CREATE_NEW_CONSOLE
        .stdin(Stdio::null())
        .stdout(output)
        .stderr(error)
        .spawn()
        .unwrap();
    wait_until(|| temporary.path().join("console-driver-complete").exists());
    assert!(
        driver.wait().unwrap().success(),
        "{}",
        fs::read_to_string(error_path).unwrap()
    );
}
