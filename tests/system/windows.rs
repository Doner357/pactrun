//! Windows-only executable system scenarios.

use std::{
    env,
    ffi::OsString,
    fs,
    os::windows::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use super::support::{
    Scenario, assert_exit, assert_success, matrix_source, run_count, run_projections,
};

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
const LAUNCHER_NAME: &str = "pactrun-system-launcher.bat";

fn windows_launcher_source() -> String {
    r#"source_format: 1
package_id: {package_id}
revision:
  inputs: []
  actions:
    - id: direct
      access: observe
      parameters: []
      hook:
        protocol_version: 1
        launch: { kind: direct, executable: worker }
        args: ["--exact", "support::system_hook", "--nocapture", "system-marker:{hook_marker}", "system-mode:success"]
        io: { terminal: none }
      outputs: []
    - id: interpreter
      access: observe
      parameters: []
      hook:
        protocol_version: 1
        launch:
          kind: interpreter
          command: pactrun-system-launcher.bat
          interpreter_args: ["--exact", "support::system_hook", "--nocapture", "--skip"]
          script: script
        args: ["system-marker:{hook_marker}", "system-mode:success"]
        io: { terminal: none }
      outputs: []
  migrations: []
runtime_content:
  files:
    - { id: worker, source: {worker_name}, path: bin/{worker_name}, executable: true }
    - { id: script, source: {worker_name}, path: scripts/script }
portable_metadata:
  reference_labels:
    - label: stable
      source: { kind: unattributed }
"#
    .to_owned()
}

fn launcher_scenario(package_id: u16) -> Scenario {
    let scenario = Scenario::new(package_id, &windows_launcher_source());
    scenario.install_and_create("node");
    scenario
}

fn path_environment<P: AsRef<std::ffi::OsStr>>(
    directories: impl IntoIterator<Item = P>,
) -> Vec<(OsString, OsString)> {
    vec![(
        OsString::from("PATH"),
        env::join_paths(directories).expect("Windows PATH fixture directories are valid"),
    )]
}

fn copy_native_image(path: &Path) {
    fs::copy(env::current_exe().unwrap(), path).unwrap();
}

fn marker_lines(scenario: &Scenario) -> String {
    String::from_utf8(fs::read(scenario.path("hook-launches")).unwrap_or_default()).unwrap()
}

fn cli_path_text(path: &Path) -> String {
    path.to_str()
        .expect("Windows platform fixture paths are Unicode")
        .chars()
        .flat_map(char::escape_default)
        .collect()
}

// Test-ID: PR-TEST-0163
// Verifies: PR-REQ-0286, PR-REQ-0288
#[test]
fn console_ctrl_c_during_parameter_stdin_acquisition_exits_without_creating_a_run() {
    let scenario = Scenario::new(0x163, &matrix_source());
    scenario.install_and_create("node");
    let mut driver = Command::new(env::current_exe().unwrap())
        .args([
            "--exact",
            "windows::windows_stdin_ctrl_c_console_driver",
            "--nocapture",
        ])
        .arg(env!("CARGO_BIN_EXE_pactrun"))
        .arg(&scenario.storage)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .unwrap();
    assert!(driver.wait().unwrap().success());
    let runs = scenario.run(["run", "list", "node"]);
    assert_success(&runs);
    assert_eq!(run_count(&runs), 0);
    assert_eq!(scenario.hook_launches(), 0);
}

// Test-ID: PR-TEST-0177
// Verifies: PR-REQ-0050, PR-REQ-0052, PR-REQ-0097, PR-REQ-0216, PR-REQ-0281, PR-REQ-0286, PR-REQ-0284
#[test]
fn accepted_late_completion_after_console_cancellation_stays_cancelled_and_publishes_eligible_output()
 {
    let scenario = Scenario::new(0x177, &matrix_source());
    scenario.install_and_create("node");
    let config = scenario.path("late-cancel-config");
    fs::write(&config, b"ready").unwrap();
    let configured = scenario.run([
        "input",
        "set",
        "node",
        "config",
        "--file",
        config.to_str().unwrap(),
    ]);
    assert_success(&configured);
    let mut driver = Command::new(env::current_exe().unwrap())
        .args([
            "--exact",
            "windows::windows_late_cancel_console_driver",
            "--nocapture",
        ])
        .arg(env!("CARGO_BIN_EXE_pactrun"))
        .arg(&scenario.storage)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .unwrap();
    assert!(driver.wait().unwrap().success());
    let listed = scenario.run(["run", "list", "node"]);
    assert_success(&listed);
    let run = run_projections(&listed)
        .into_iter()
        .find(|run| run.action == "late_cancel" && run.outcome == "cancelled")
        .expect("cancelled late-completion Run must be durable");
    let shown = scenario.run(["run", "show", &run.id]);
    assert_success(&shown);
    let shown = String::from_utf8_lossy(&shown.stdout);
    assert!(shown.contains("phase: finished"));
    assert!(shown.contains("outcome: cancelled"));
    assert!(shown.contains("hook_completion_status: failure"));
    assert!(shown.contains("artifact: report\tbytes: 13"));
}

// Test-ID: PR-TEST-0164
// Verifies: PR-REQ-0194, PR-REQ-0274
#[test]
fn native_images_and_batch_suffixes_use_exact_paths_without_shell_or_sibling_fallback() {
    let scenario = launcher_scenario(0x164);
    let direct = scenario.invoke("node", "direct");
    assert_success(&direct);
    assert_eq!(scenario.hook_launches(), 1);

    let first = scenario.path("first-launcher-directory");
    let second = scenario.path("second-launcher-directory");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    let sentinel = scenario.path("batch-shell-ran");
    fs::write(
        first.join(LAUNCHER_NAME),
        format!(
            "@echo off\r\necho shell-ran>\"{}\"\r\nexit /b 0\r\n",
            sentinel.display()
        ),
    )
    .unwrap();
    copy_native_image(&second.join(LAUNCHER_NAME));
    let environment = path_environment([&first, &second]);

    let planned =
        scenario.run_with_environment(["invoke", "node", "interpreter", "--plan"], &environment);
    assert_success(&planned);
    let planned_text = String::from_utf8_lossy(&planned.stdout);
    assert!(
        planned_text.contains(&cli_path_text(&first.join(LAUNCHER_NAME))),
        "{planned_text}"
    );
    let rejected = scenario.run_with_environment(["invoke", "node", "interpreter"], &environment);
    assert_exit(&rejected, 1);
    assert!(!sentinel.exists(), "batch launcher reached shell execution");
    assert_eq!(
        scenario.hook_launches(),
        1,
        "a later PATH sibling was launched"
    );

    fs::remove_file(first.join(LAUNCHER_NAME)).unwrap();
    copy_native_image(&first.join(LAUNCHER_NAME));
    let planned =
        scenario.run_with_environment(["invoke", "node", "interpreter", "--plan"], &environment);
    assert_success(&planned);
    let planned_text = String::from_utf8_lossy(&planned.stdout);
    assert!(
        planned_text.contains(&cli_path_text(&first.join(LAUNCHER_NAME))),
        "{planned_text}"
    );
    let accepted = scenario.run_with_environment(["invoke", "node", "interpreter"], &environment);
    assert_success(&accepted);
    assert_eq!(scenario.hook_launches(), 2);
    let runs = scenario.run(["run", "list", "node"]);
    assert_success(&runs);
    let runs = run_projections(&runs);
    assert!(runs.iter().any(|run| run.outcome == "failed"));
    assert!(runs.iter().any(|run| run.outcome == "succeeded"));
    assert!(!marker_lines(&scenario).contains("shell-ran"));
}

#[test]
fn windows_stdin_ctrl_c_console_driver() {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let Some(index) = arguments
        .iter()
        .position(|argument| argument == "--nocapture")
    else {
        return;
    };
    let payload = &arguments[index + 1..];
    if payload.is_empty() {
        return;
    }
    assert_eq!(
        payload.len(),
        2,
        "console driver payload must contain the Pactrun executable and storage root"
    );

    pactrun_windows_ntfs::allocate_console().unwrap();
    ctrlc::set_handler(|| {}).unwrap();
    let pipe_name = format!("pactrun-system-stdin-{}", std::process::id());
    let pipe_path = format!(r"\\.\pipe\{pipe_name}");
    let consumed = Path::new(&payload[1]).join("stdin-acquisition-consumed");
    let server_script = r#"
param([string]$pipe_name, [string]$consumed)
$pipe = [System.IO.Pipes.NamedPipeServerStream]::new(
    $pipe_name,
    [System.IO.Pipes.PipeDirection]::Out,
    1,
    [System.IO.Pipes.PipeTransmissionMode]::Byte,
    [System.IO.Pipes.PipeOptions]::None,
    1,
    1
)
$pipe.WaitForConnection()
$pipe.WriteByte(97)
$pipe.Flush()
$pipe.WaitForPipeDrain()
[System.IO.File]::WriteAllText($consumed, 'consumed')
while ($true) { Start-Sleep -Seconds 1 }
"#;
    let server_path = Path::new(&payload[1]).join("stdin-acquisition-server.ps1");
    fs::write(&server_path, server_script).unwrap();
    let mut server = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&server_path)
        .arg(&pipe_name)
        .arg(&consumed)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let stdin = loop {
        match fs::OpenOptions::new().read(true).open(&pipe_path) {
            Ok(pipe) => break pipe,
            Err(error) if Instant::now() < deadline => {
                assert!(
                    server.try_wait().unwrap().is_none(),
                    "named-pipe stdin server exited before Pactrun connected: {error}"
                );
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("open deterministic stdin pipe {pipe_path}: {error}"),
        }
    };
    let mut invoked = Command::new(&payload[0])
        .args([
            "invoke",
            "node",
            "parameters",
            "--param-stdin",
            "from_stdin",
        ])
        .env("PACTRUN_STORAGE_ROOT", &payload[1])
        .stdin(Stdio::from(stdin))
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !consumed.exists() {
        assert!(
            invoked.try_wait().unwrap().is_none(),
            "Pactrun exited before consuming the deterministic stdin barrier"
        );
        assert!(
            server.try_wait().unwrap().is_none(),
            "named-pipe stdin server exited before consumption was observed"
        );
        assert!(
            Instant::now() < deadline,
            "Pactrun did not consume the deterministic stdin barrier"
        );
        thread::sleep(Duration::from_millis(20));
    }
    // WaitForPipeDrain proves the CancellableStdin worker consumed the staged
    // byte and returned to its still-open pipe read before Ctrl+C is sent.
    pactrun_windows_ntfs::generate_console_ctrl_c().unwrap();
    let status = invoked.wait().unwrap();
    let _ = server.kill();
    let _ = server.wait();
    assert_eq!(status.code(), Some(1));
}

#[test]
fn windows_late_cancel_console_driver() {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let Some(index) = arguments
        .iter()
        .position(|argument| argument == "--nocapture")
    else {
        return;
    };
    let payload = &arguments[index + 1..];
    if payload.is_empty() {
        return;
    }
    assert_eq!(payload.len(), 2);
    pactrun_windows_ntfs::allocate_console().unwrap();
    ctrlc::set_handler(|| {}).unwrap();
    let marker = Path::new(&payload[1])
        .parent()
        .expect("storage root has a scenario parent")
        .join("hook-launches");
    let mut invoked = Command::new(&payload[0])
        .args([
            "invoke",
            "node",
            "late_cancel",
            "--termination-grace-ms",
            "500",
        ])
        .env("PACTRUN_STORAGE_ROOT", &payload[1])
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !fs::read_to_string(&marker)
        .unwrap_or_default()
        .contains("ready:late_cancel")
    {
        assert!(
            invoked.try_wait().unwrap().is_none(),
            "Pactrun exited before the late-cancellation Hook became ready"
        );
        assert!(
            Instant::now() < deadline,
            "late-cancellation Hook did not become ready"
        );
        thread::sleep(Duration::from_millis(20));
    }
    pactrun_windows_ntfs::generate_console_ctrl_c().unwrap();
    assert_eq!(invoked.wait().unwrap().code(), Some(1));
}
