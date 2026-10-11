//! Linux-only executable system scenarios.

use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    path::Path,
    process::Command,
};

use super::support::{
    Scenario, assert_exit, assert_success, matrix_source, run_count, run_projections,
};

const LAUNCHER_NAME: &str = "pactrun-system-launcher";

fn interpreter_source() -> String {
    r#"source_format: 1.0-alpha.2
package_id: {package_id}
revision:
  inputs: []
  actions:
    - id: interpreter
      access: observe
      parameters: []
      hook:
        protocol_version: 1.0-alpha.1
        launch:
          kind: interpreter
          command: pactrun-system-launcher
          interpreter_args: ["--exact", "support::system_hook", "--nocapture", "--skip"]
          script: script
        args: ["system-marker:{hook_marker}", "system-mode:success"]
        io: { terminal: none }
      outputs: []
  migrations: []
runtime_content:
  files:
    - { id: script, source: {worker_name}, path: bin/script }
"#
    .to_owned()
}

fn interpreter_scenario(package_id: u16) -> Scenario {
    let scenario = Scenario::new(package_id, &interpreter_source());
    scenario.install_and_create("node");
    scenario
}

fn path_environment<P: AsRef<OsStr>>(
    directories: impl IntoIterator<Item = P>,
) -> Vec<(OsString, OsString)> {
    vec![(
        OsString::from("PATH"),
        env::join_paths(directories).expect("Linux PATH fixture directories are valid"),
    )]
}

fn shell_quote(path: &Path) -> String {
    let text = path
        .as_os_str()
        .to_str()
        .expect("test executable and marker paths are valid UTF-8");
    format!("'{}'", text.replace('\'', "'\"'\"'"))
}

fn write_functional_launcher(path: &Path, marker: &Path, label: &str) {
    let executable = env::current_exe().unwrap();
    fs::write(
        path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' {} >> {}\nexec {} \"$@\"\n",
            shell_quote(Path::new(label)),
            shell_quote(marker),
            shell_quote(&executable),
        ),
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn write_failing_launcher(path: &Path, marker: &Path) {
    fs::write(
        path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' 'launcher:first-failed' >> {}\nexit 71\n",
            shell_quote(marker),
        ),
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn marker_lines(scenario: &Scenario) -> String {
    String::from_utf8(fs::read(scenario.path("hook-launches")).unwrap_or_default()).unwrap()
}

// Test-ID: PR-TEST-0160
// Verifies: PR-REQ-0286, PR-REQ-0288
#[test]
fn sigint_during_parameter_stdin_acquisition_exits_without_creating_a_run() {
    let scenario = Scenario::new(0x160, &matrix_source());
    scenario.install_and_create("node");
    let driver = r#"
import fcntl, os, signal, struct, sys, time

cli, storage = sys.argv[1:]
read_fd, write_fd = os.pipe()
pid = os.fork()
if pid == 0:
    os.dup2(read_fd, 0)
    os.close(read_fd)
    os.close(write_fd)
    os.execve(cli, [cli, 'invoke', 'node', 'parameters', '--param-stdin', 'from_stdin'], {
        **os.environ,
        'PACTRUN_STORAGE_ROOT': storage,
    })

def wait_until_consumed(byte):
    os.write(write_fd, byte)
    deadline = time.monotonic() + 10
    while True:
        queued = struct.unpack('I', fcntl.ioctl(read_fd, termios.FIONREAD, struct.pack('I', 0)))[0]
        if queued == 0:
            return
        if time.monotonic() >= deadline:
            raise RuntimeError('Pactrun did not consume staged stdin input')
        time.sleep(0.01)

import termios
# The third observed pipe read establishes that the CLI has progressed through
# its cancellable stdin path (the worker cannot perform it until the main
# thread has accepted earlier buffered reads) while the writer remains open.
for byte in (b'a', b'b', b'c'):
    wait_until_consumed(byte)
os.kill(pid, signal.SIGINT)
os.close(write_fd)
_, status = os.waitpid(pid, 0)
os.close(read_fd)
if not os.WIFEXITED(status) or os.WEXITSTATUS(status) != 1:
    raise RuntimeError('stdin-acquisition SIGINT did not exit Pactrun with code 1: ' + repr(status))
"#;
    let driven = Command::new("python3")
        .args(["-c", driver])
        .arg(env!("CARGO_BIN_EXE_pactrun"))
        .arg(&scenario.storage)
        .output()
        .unwrap();
    assert_success(&driven);
    let runs = scenario.run(["run", "list", "node", "--no-trunc"]);
    assert_success(&runs);
    assert_eq!(run_count(&runs), 0);
    assert_eq!(scenario.hook_launches(), 0);
}

// Test-ID: PR-TEST-0176
// Verifies: PR-REQ-0050, PR-REQ-0052, PR-REQ-0097, PR-REQ-0216, PR-REQ-0281, PR-REQ-0286, PR-REQ-0284
#[test]
fn accepted_late_completion_after_sigint_stays_cancelled_and_publishes_eligible_output() {
    let scenario = Scenario::new(0x176, &matrix_source());
    scenario.install_and_create("node");
    let config = scenario.path("late-cancel-config");
    fs::write(&config, b"ready").unwrap();
    let configured = scenario.run([
        OsString::from("input"),
        OsString::from("set"),
        OsString::from("node"),
        OsString::from("config"),
        OsString::from("--file"),
        config.into_os_string(),
    ]);
    assert_success(&configured);
    let driver = r#"
import os, signal, sys, time

cli, storage, marker = sys.argv[1:]
pid = os.fork()
if pid == 0:
    os.execve(cli, [cli, 'invoke', 'node', 'late_cancel', '--termination-grace-ms', '500'], {
        **os.environ,
        'PACTRUN_STORAGE_ROOT': storage,
    })

deadline = time.monotonic() + 10
while True:
    try:
        ready = b'ready:late_cancel' in open(marker, 'rb').read()
    except FileNotFoundError:
        ready = False
    if ready:
        break
    waited, status = os.waitpid(pid, os.WNOHANG)
    if waited:
        raise RuntimeError('Pactrun exited before late-cancellation Hook readiness: ' + repr(status))
    if time.monotonic() >= deadline:
        raise RuntimeError('late-cancellation Hook did not become ready')
    time.sleep(0.01)
os.kill(pid, signal.SIGINT)
_, status = os.waitpid(pid, 0)
if not os.WIFEXITED(status) or os.WEXITSTATUS(status) != 1:
    raise RuntimeError('late-cancellation SIGINT did not exit Pactrun with code 1: ' + repr(status))
"#;
    let driven = Command::new("python3")
        .args(["-c", driver])
        .arg(env!("CARGO_BIN_EXE_pactrun"))
        .arg(&scenario.storage)
        .arg(scenario.path("hook-launches"))
        .output()
        .unwrap();
    assert_success(&driven);
    let listed = scenario.run(["run", "list", "node", "--no-trunc"]);
    assert_success(&listed);
    let run = run_projections(&listed)
        .into_iter()
        .find(|run| run.action == "late_cancel" && run.outcome == "cancelled")
        .expect("cancelled late-completion Run must be durable");
    let shown = scenario.run(["run", "show", &run.id]);
    assert_success(&shown);
    let shown = String::from_utf8_lossy(&shown.stdout);
    assert!(shown.contains("Phase: finished"));
    assert!(shown.contains("Outcome: cancelled"));
    assert!(shown.contains("Hook completion: failure"));
    assert!(shown.contains("Artifact: report (13 bytes)"));
}

// Test-ID: PR-TEST-0161
// Verifies: PR-REQ-0194, PR-REQ-0274
#[test]
fn interpreter_lookup_uses_the_first_eligible_path_candidate_and_never_falls_back() {
    let scenario = interpreter_scenario(0x161);
    let ineligible = scenario.path("ineligible-launcher-directory");
    let first = scenario.path("first-launcher-directory");
    let second = scenario.path("second-launcher-directory");
    for directory in [&ineligible, &first, &second] {
        fs::create_dir(directory).unwrap();
    }
    fs::write(ineligible.join(LAUNCHER_NAME), b"#!/bin/sh\nexit 99\n").unwrap();
    let marker = scenario.path("hook-launches");
    write_functional_launcher(&first.join(LAUNCHER_NAME), &marker, "launcher:first");
    write_functional_launcher(&second.join(LAUNCHER_NAME), &marker, "launcher:second");
    let environment = path_environment([&ineligible, &first, &second]);

    let planned =
        scenario.run_with_environment(["invoke", "node", "interpreter", "--plan"], &environment);
    assert_success(&planned);
    let planned = String::from_utf8_lossy(&planned.stdout);
    assert!(planned.contains(&first.join(LAUNCHER_NAME).display().to_string()));

    let selected = scenario.run_with_environment(["invoke", "node", "interpreter"], &environment);
    assert_success(&selected);
    let selected_markers = marker_lines(&scenario);
    assert!(
        selected_markers.contains("launcher:first"),
        "{selected_markers}"
    );
    assert!(
        !selected_markers.contains("launcher:second"),
        "{selected_markers}"
    );

    write_failing_launcher(&first.join(LAUNCHER_NAME), &marker);
    let failed = scenario.run_with_environment(["invoke", "node", "interpreter"], &environment);
    assert_exit(&failed, 1);
    let markers = marker_lines(&scenario);
    assert!(markers.contains("launcher:first-failed"), "{markers}");
    assert!(!markers.contains("launcher:second"), "{markers}");
    let runs = scenario.run(["run", "list", "node", "--no-trunc"]);
    assert_success(&runs);
    let runs = run_projections(&runs);
    assert!(runs.iter().any(|run| run.outcome == "succeeded"));
    assert!(runs.iter().any(|run| run.outcome == "failed"));
}

// Test-ID: PR-TEST-0162
// Verifies: PR-REQ-0274
#[test]
fn interpreter_lookup_requires_an_executable_candidate_and_preserves_invalid_utf8_path_bytes() {
    let scenario = interpreter_scenario(0x162);
    let no_execute = scenario.path("non-executable-launcher-directory");
    fs::create_dir(&no_execute).unwrap();
    fs::write(no_execute.join(LAUNCHER_NAME), b"#!/bin/sh\nexit 99\n").unwrap();
    let ineligible_environment = path_environment([&no_execute]);
    let rejected = scenario.run_with_environment(
        ["invoke", "node", "interpreter", "--plan"],
        &ineligible_environment,
    );
    assert_exit(&rejected, 1);
    let runs = scenario.run(["run", "list", "node", "--no-trunc"]);
    assert_success(&runs);
    assert_eq!(run_count(&runs), 0);

    let byte_directory = scenario
        .path("launcher-parent")
        .join(OsString::from_vec(b"launcher-\x80".to_vec()));
    fs::create_dir_all(&byte_directory).unwrap();
    let marker = scenario.path("hook-launches");
    write_functional_launcher(
        &byte_directory.join(LAUNCHER_NAME),
        &marker,
        "launcher:byte-path",
    );
    let byte_environment = path_environment([&byte_directory]);
    let planned = scenario.run_with_environment(
        ["invoke", "node", "interpreter", "--plan"],
        &byte_environment,
    );
    assert_success(&planned);
    let planned = String::from_utf8_lossy(&planned.stdout);
    assert!(planned.contains("Unix bytes ["), "{planned}");
    assert!(!planned.contains('\u{fffd}'), "{planned}");
    let invoked =
        scenario.run_with_environment(["invoke", "node", "interpreter"], &byte_environment);
    assert_success(&invoked);
    assert!(marker_lines(&scenario).contains("launcher:byte-path"));
}
