use std::{
    collections::BTreeMap,
    env,
    ffi::OsStr,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use tempfile::TempDir;

const TRANSPORT_ENVIRONMENT: &str = "PACTRUN_HOOK_PROTOCOL_TRANSPORT";
const ENDPOINT_ENVIRONMENT: &str = "PACTRUN_HOOK_PROTOCOL_ENDPOINT";
const PREAMBLE: &[u8] = b"pactrun.hook-protocol\0\0\0\0\x01";
const COMMAND_DEADLINE: Duration = Duration::from_secs(30);
const SCENARIO_DEADLINE: Duration = Duration::from_secs(90);

pub(crate) struct Scenario {
    started: Instant,
    temporary: TempDir,
    pub(crate) storage: PathBuf,
    pub(crate) source: PathBuf,
    hook_marker: PathBuf,
}

impl Scenario {
    pub(crate) fn new(package_id: u16, source: &str) -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("system-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("run-")
            .tempdir_in(parent)
            .unwrap();
        let storage = temporary.path().join("storage");
        fs::create_dir(&storage).unwrap();
        for child in ["database", "runtime-content", "staging"] {
            fs::create_dir(storage.join(child)).unwrap();
        }
        let source_root = temporary.path().join("source");
        fs::create_dir(&source_root).unwrap();
        let worker_name = if cfg!(windows) {
            "worker.exe"
        } else {
            "worker"
        };
        fs::copy(env::current_exe().unwrap(), source_root.join(worker_name)).unwrap();
        let marker = temporary.path().join("hook-launches");
        fs::write(
            source_root.join("pactrun.yaml"),
            source
                .replace("{package_id}", &format!("{package_id:032x}"))
                .replace("{worker_name}", worker_name)
                .replace(
                    "{hook_marker}",
                    &marker.to_string_lossy().replace('\\', "/"),
                ),
        )
        .unwrap();
        Self {
            started: Instant::now(),
            temporary,
            storage,
            source: source_root,
            hook_marker: marker,
        }
    }

    pub(crate) fn install(&self) -> String {
        let installed = self.run([
            OsStr::new("pack"),
            OsStr::new("install"),
            self.source.as_os_str(),
        ]);
        assert_success(&installed);
        first_line(&installed.stdout).to_owned()
    }

    pub(crate) fn create_instance(&self, name: &str, revision: &str) -> Output {
        self.run(["instance", "create", name, "--revision", revision])
    }

    pub(crate) fn install_and_create(&self, name: &str) -> String {
        let revision = self.install();
        let created = self.create_instance(name, &revision);
        assert_success(&created);
        revision
    }

    pub(crate) fn path(&self, name: &str) -> PathBuf {
        self.temporary.path().join(name)
    }

    pub(crate) fn run<I, S>(&self, arguments: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        assert!(
            self.started.elapsed() < SCENARIO_DEADLINE,
            "system scenario exceeded its {} second deadline",
            SCENARIO_DEADLINE.as_secs()
        );
        run_scenario_command(
            command(&self.storage, self.temporary.path(), arguments),
            &self.hook_marker,
        )
    }

    pub(crate) fn run_with_stdin<I, S>(&self, arguments: I, stdin: Vec<u8>) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        assert!(
            self.started.elapsed() < SCENARIO_DEADLINE,
            "system scenario exceeded its {} second deadline",
            SCENARIO_DEADLINE.as_secs()
        );
        let mut command = command(&self.storage, self.temporary.path(), arguments);
        command.stdin(Stdio::piped());
        run_scenario_command_with_stdin(command, &self.hook_marker, stdin)
    }

    pub(crate) fn run_with_environment<I, S>(
        &self,
        arguments: I,
        environment: &[(std::ffi::OsString, std::ffi::OsString)],
    ) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        assert!(
            self.started.elapsed() < SCENARIO_DEADLINE,
            "system scenario exceeded its {} second deadline",
            SCENARIO_DEADLINE.as_secs()
        );
        let mut command = command(&self.storage, self.temporary.path(), arguments);
        for (key, value) in environment {
            command.env(key, value);
        }
        run_scenario_command(command, &self.hook_marker)
    }

    pub(crate) fn invoke(&self, instance: &str, action: &str) -> Output {
        self.run(["invoke", instance, action])
    }

    pub(crate) fn spawn<I, S>(&self, arguments: I) -> RunningCommand
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        assert!(
            self.started.elapsed() < SCENARIO_DEADLINE,
            "system scenario exceeded its {} second deadline",
            SCENARIO_DEADLINE.as_secs()
        );
        RunningCommand::spawn(
            command(&self.storage, self.temporary.path(), arguments),
            Some(self.hook_marker.clone()),
        )
    }

    pub(crate) fn wait_for_marker(&self, prefix: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(marker) = fs::read_to_string(&self.hook_marker)
                && let Some(line) = marker.lines().find(|line| line.starts_with(prefix))
            {
                return line.to_owned();
            }
            assert!(
                Instant::now() < deadline,
                "Hook marker {prefix:?} was not written within ten seconds"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    pub(crate) fn release_hook(&self) {
        fs::write(format!("{}.release", self.hook_marker.display()), []).unwrap();
    }

    pub(crate) fn hook_launches(&self) -> usize {
        fs::read_to_string(&self.hook_marker)
            .map(|value| {
                value
                    .lines()
                    .filter(|line| line.starts_with("launch:"))
                    .count()
            })
            .unwrap_or(0)
    }
}

pub(crate) fn command<I, S>(storage: &Path, cwd: &Path, arguments: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(env!("CARGO_BIN_EXE_pactrun"));
    command
        .args(arguments)
        .current_dir(cwd)
        .env("PACTRUN_STORAGE_ROOT", storage)
        .env_remove(TRANSPORT_ENVIRONMENT)
        .env_remove(ENDPOINT_ENVIRONMENT)
        .stdin(Stdio::null());
    command
}

pub(crate) fn startup_command<I, S>(cwd: &Path, arguments: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(env!("CARGO_BIN_EXE_pactrun"));
    command
        .args(arguments)
        .current_dir(cwd)
        .env_remove("PACTRUN_STORAGE_ROOT");
    command
}

pub(crate) fn run_command(mut command: Command) -> Output {
    run_owned_command(&mut command, None, None)
}

fn run_scenario_command(mut command: Command, hook_marker: &Path) -> Output {
    run_owned_command(&mut command, Some(hook_marker.to_path_buf()), None)
}

fn run_scenario_command_with_stdin(
    mut command: Command,
    hook_marker: &Path,
    stdin: Vec<u8>,
) -> Output {
    run_owned_command(&mut command, Some(hook_marker.to_path_buf()), Some(stdin))
}

fn run_owned_command(
    command: &mut Command,
    hook_marker: Option<PathBuf>,
    stdin: Option<Vec<u8>>,
) -> Output {
    let mut owned = OwnedChild::spawn(command, hook_marker);
    let stdin_writer = stdin.map(|bytes| {
        let mut stdin = owned.child.stdin.take().unwrap();
        thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        })
    });
    let mut stdout = owned.child.stdout.take().unwrap();
    let mut stderr = owned.child.stderr.take().unwrap();
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + COMMAND_DEADLINE;
    let status = loop {
        if let Some(status) = owned.child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            panic!("system CLI command exceeded {COMMAND_DEADLINE:?}");
        }
        thread::sleep(Duration::from_millis(20));
    };
    owned.reaped = true;
    let stdout = stdout_reader.join().unwrap();
    let stderr = stderr_reader.join().unwrap();
    if let Some(writer) = stdin_writer {
        let _ = writer.join();
    }
    Output {
        status,
        stdout,
        stderr,
    }
}

pub(crate) struct RunningCommand {
    owned: OwnedChild,
    stdout_reader: Option<thread::JoinHandle<Vec<u8>>>,
    stderr_reader: Option<thread::JoinHandle<Vec<u8>>>,
}

impl RunningCommand {
    fn spawn(mut command: Command, hook_marker: Option<PathBuf>) -> Self {
        let mut owned = OwnedChild::spawn(&mut command, hook_marker);
        let mut stdout = owned.child.stdout.take().unwrap();
        let mut stderr = owned.child.stderr.take().unwrap();
        let stdout_reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes).unwrap();
            bytes
        });
        let stderr_reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).unwrap();
            bytes
        });
        Self {
            owned,
            stdout_reader: Some(stdout_reader),
            stderr_reader: Some(stderr_reader),
        }
    }

    pub(crate) fn wait(mut self) -> Output {
        let deadline = Instant::now() + COMMAND_DEADLINE;
        let status = loop {
            if let Some(status) = self.owned.child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                panic!("system CLI command exceeded {COMMAND_DEADLINE:?}");
            }
            thread::sleep(Duration::from_millis(20));
        };
        self.owned.reaped = true;
        self.output(status)
    }

    pub(crate) fn terminate(mut self) -> Output {
        self.owned.terminate_tree_and_reap();
        let status = self.owned.child.try_wait().unwrap().unwrap();
        self.output(status)
    }

    fn output(&mut self, status: std::process::ExitStatus) -> Output {
        Output {
            status,
            stdout: self.stdout_reader.take().unwrap().join().unwrap(),
            stderr: self.stderr_reader.take().unwrap().join().unwrap(),
        }
    }
}

impl Drop for RunningCommand {
    fn drop(&mut self) {
        if !self.owned.reaped {
            self.owned.terminate_tree_and_reap();
        }
        if let Some(reader) = self.stdout_reader.take() {
            let _ = reader.join();
        }
        if let Some(reader) = self.stderr_reader.take() {
            let _ = reader.join();
        }
    }
}

struct OwnedChild {
    child: Child,
    reaped: bool,
    hook_marker: Option<PathBuf>,
}

impl OwnedChild {
    fn spawn(command: &mut Command, hook_marker: Option<PathBuf>) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        Self {
            child: command.spawn().unwrap(),
            reaped: false,
            hook_marker,
        }
    }

    fn terminate_tree_and_reap(&mut self) {
        terminate_process_tree(self.child.id());
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.reaped = true;
        if let Some(hook_marker) = &self.hook_marker {
            terminate_recorded_hook_trees(hook_marker);
        }
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            self.terminate_tree_and_reap();
        }
    }
}

fn terminate_recorded_hook_trees(hook_marker: &Path) {
    let Ok(marker) = fs::read_to_string(hook_marker) else {
        return;
    };
    for process_id in marker
        .lines()
        .filter_map(|line| line.strip_prefix("launch:")?.parse::<u32>().ok())
    {
        terminate_process_tree(process_id);
    }
}

#[cfg(unix)]
fn terminate_process_tree(process_id: u32) {
    let _ = Command::new("/bin/kill")
        .args(["-KILL", "--", &format!("-{process_id}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(windows)]
fn terminate_process_tree(process_id: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &process_id.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub(crate) fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

pub(crate) fn assert_empty_json_result(output: &Output, field: &str) {
    assert_success(output);
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["format"], "pactrun.cli.v1");
    assert_eq!(response["status"], "success");
    assert!(response["error"].is_null());
    assert!(
        response["result"][field]
            .as_array()
            .expect("typed collection")
            .is_empty(),
        "unexpected result: {response}"
    );
}

pub(crate) fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

pub(crate) fn first_line(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap().lines().next().unwrap()
}

pub(crate) fn run_count(output: &Output) -> usize {
    run_projections(output).len()
}

pub(crate) fn run_id(output: &Output) -> String {
    std::str::from_utf8(&output.stderr)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("run: "))
        .expect("invoke must report the accepted Run id")
        .to_owned()
}

pub(crate) struct InstanceProjection {
    pub(crate) state_version: String,
    pub(crate) ready: bool,
}

pub(crate) struct InstanceListProjection {
    pub(crate) name: String,
}

pub(crate) struct RunProjection {
    pub(crate) id: String,
    pub(crate) action: String,
    pub(crate) outcome: String,
}

pub(crate) struct RunDetailsProjection {
    pub(crate) action: String,
    pub(crate) outcome: String,
}

pub(crate) fn instance_projection(output: &Output) -> InstanceProjection {
    let fields = fields(
        &output.stdout,
        &["state_version", "required_inputs_satisfied"],
    );
    InstanceProjection {
        state_version: required(&fields, "state_version").to_owned(),
        ready: required(&fields, "required_inputs_satisfied") == "true",
    }
}

pub(crate) fn instance_list_projections(output: &Output) -> Vec<InstanceListProjection> {
    let text = std::str::from_utf8(&output.stdout).unwrap();
    if text.trim() == "No managed Instances." {
        return Vec::new();
    }
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let columns = line.split('\t').collect::<Vec<_>>();
            assert_eq!(columns.len(), 6, "malformed instance-list row: {line}");
            assert!(columns[5].starts_with("guard="));
            assert_ne!(columns[4], "ready");
            InstanceListProjection {
                name: columns[1].to_owned(),
            }
        })
        .collect()
}

pub(crate) fn run_projections(output: &Output) -> Vec<RunProjection> {
    let text = std::str::from_utf8(&output.stdout).unwrap();
    assert!(text.starts_with("RUN ID  INSTANCE ID  OPERATION  PHASE  OUTCOME\n"));
    text.lines()
        .skip(1)
        .take_while(|line| !line.is_empty())
        .map(|line| {
            let columns = line.split_whitespace().collect::<Vec<_>>();
            assert_eq!(columns.len(), 5, "malformed Run table row: {line}");
            assert_eq!(
                columns[0].len(),
                32,
                "tests needing exact IDs request --no-trunc"
            );
            assert_eq!(columns[1].len(), 32);
            RunProjection {
                id: columns[0].to_owned(),
                action: columns[2].to_owned(),
                outcome: columns[4].to_owned(),
            }
        })
        .collect()
}

pub(crate) fn run_details_projection(output: &Output) -> RunDetailsProjection {
    let fields = fields(&output.stdout, &["action", "outcome"]);
    RunDetailsProjection {
        action: required(&fields, "action").to_owned(),
        outcome: required(&fields, "outcome").to_owned(),
    }
}

fn fields<'a>(output: &'a [u8], selected: &[&str]) -> std::collections::BTreeMap<&'a str, &'a str> {
    let mut fields = std::collections::BTreeMap::new();
    for line in std::str::from_utf8(output).unwrap().lines() {
        let Some((key, value)) = line.split_once(": ") else {
            continue;
        };
        if !selected.contains(&key) {
            continue;
        }
        assert!(
            fields.insert(key, value).is_none(),
            "duplicate CLI field {key}"
        );
    }
    fields
}

fn required<'a>(fields: &'a std::collections::BTreeMap<&str, &str>, key: &str) -> &'a str {
    fields
        .get(key)
        .copied()
        .unwrap_or_else(|| panic!("missing CLI field {key}"))
}

pub(crate) fn basic_source() -> &'static str {
    r#"source_format: 1
package_id: {package_id}
revision:
  inputs: []
  actions:
    - id: observe
      access: observe
      parameters: []
      hook:
        protocol_version: 1
        launch: { kind: direct, executable: worker }
        args: ["--exact", "support::system_hook_success", "--nocapture", "system-marker:{hook_marker}"]
        io: { terminal: none }
      outputs: []
    - id: mutate
      access: mutate
      parameters: []
      hook:
        protocol_version: 1
        launch: { kind: direct, executable: worker }
        args: ["--exact", "support::system_hook_success", "--nocapture", "system-marker:{hook_marker}"]
        io: { terminal: none }
      outputs: []
    - id: failure
      access: observe
      parameters: []
      hook:
        protocol_version: 1
        launch: { kind: direct, executable: worker }
        args: ["--exact", "support::system_hook_declared_failure", "--nocapture", "system-marker:{hook_marker}"]
        io: { terminal: none }
      outputs: []
  migrations: []
runtime_content:
  files:
    - { id: worker, source: {worker_name}, path: bin/{worker_name}, executable: true }
portable_metadata:
  reference_labels:
    - label: stable
      source: { kind: unattributed }
"#
}

pub(crate) fn matrix_source() -> String {
    let action = |id: &str,
                  access: &str,
                  mode: &str,
                  terminal: &str,
                  parameters: &str,
                  outputs: &str| {
        format!(
            r#"    - id: {id}
      access: {access}
      parameters: {parameters}
      hook:
        protocol_version: 1
        launch: {{ kind: direct, executable: worker }}
        args: ["--exact", "support::system_hook", "--nocapture", "system-marker:{{hook_marker}}", "system-mode:{mode}"]
        io: {{ terminal: {terminal} }}
      outputs: {outputs}
"#
        )
    };
    let parameter_declarations = r#"[{ id: ordinary, type: string, sensitive: false }, { id: from_file, type: string, sensitive: true }, { id: from_stdin, type: string, sensitive: true }, { id: enabled, type: boolean, sensitive: false }, { id: count, type: integer, sensitive: false }, { id: secret, type: string, sensitive: true }]"#;
    let sensitive_declarations = r#"[{ id: secret, type: string, sensitive: true }]"#;
    let mut actions = String::new();
    for (id, access, mode, terminal, parameters, outputs) in [
        (
            "parameters",
            "observe",
            "parameters",
            "none",
            parameter_declarations,
            "[]",
        ),
        (
            "outputs",
            "observe",
            "output",
            "none",
            "[]",
            "[{ id: report }]",
        ),
        (
            "outputs_empty",
            "observe",
            "output_empty",
            "none",
            "[]",
            "[{ id: report }, { id: summary }]",
        ),
        (
            "outputs_subset",
            "observe",
            "output_subset",
            "none",
            "[]",
            "[{ id: report }, { id: summary }]",
        ),
        (
            "outputs_multiple",
            "observe",
            "output_multiple",
            "none",
            "[]",
            "[{ id: report }, { id: summary }]",
        ),
        (
            "outputs_group_missing",
            "observe",
            "output_group_missing",
            "none",
            "[]",
            "[{ id: report }, { id: summary }]",
        ),
        (
            "outputs_failure",
            "observe",
            "output_failure",
            "none",
            "[]",
            "[{ id: report }, { id: summary }]",
        ),
        (
            "output_missing",
            "observe",
            "output_missing",
            "none",
            "[]",
            "[{ id: report }]",
        ),
        (
            "output_unsubmitted",
            "observe",
            "output_unsubmitted",
            "none",
            "[]",
            "[{ id: report }]",
        ),
        (
            "invalid_output",
            "observe",
            "invalid_output",
            "none",
            "[]",
            "[{ id: report }]",
        ),
        (
            "exit_before_connect",
            "observe",
            "exit_before_connect",
            "none",
            "[]",
            "[]",
        ),
        (
            "preconnect_hang",
            "observe",
            "hang_before_connect",
            "none",
            "[]",
            "[]",
        ),
        (
            "protocol_error",
            "observe",
            "protocol_error",
            "none",
            "[]",
            "[]",
        ),
        (
            "wrong_protocol_version",
            "observe",
            "wrong_protocol_version",
            "none",
            "[]",
            "[]",
        ),
        (
            "malformed_framing",
            "observe",
            "malformed_framing",
            "none",
            "[]",
            "[]",
        ),
        (
            "declared_failure",
            "observe",
            "declared_failure",
            "none",
            "[]",
            "[]",
        ),
        ("eof", "observe", "eof_after_ready", "none", "[]", "[]"),
        ("timeout", "observe", "hang_after_ready", "none", "[]", "[]"),
        (
            "late_timeout",
            "observe",
            "late_timeout",
            "none",
            "[]",
            "[{ id: report }]",
        ),
        (
            "late_timeout_unaccepted",
            "observe",
            "late_timeout_unaccepted",
            "none",
            "[]",
            "[{ id: report }]",
        ),
        (
            "late_cancel",
            "observe",
            "late_cancel",
            "none",
            "[]",
            "[{ id: report }]",
        ),
        (
            "hold_observe",
            "observe",
            "hold_observe",
            "none",
            "[]",
            "[]",
        ),
        ("hold_mutate", "mutate", "hold_mutate", "none", "[]", "[]"),
        ("hold_risk", "mutate", "hold_risk", "none", "[]", "[]"),
        ("fast_observe", "observe", "success", "none", "[]", "[]"),
        ("fast_mutate", "mutate", "success", "none", "[]", "[]"),
        ("risk", "mutate", "risk", "none", "[]", "[]"),
        (
            "risk_open_failure",
            "mutate",
            "risk_open_failure",
            "none",
            "[]",
            "[]",
        ),
        (
            "risk_open_success",
            "mutate",
            "risk_open_success",
            "none",
            "[]",
            "[]",
        ),
        (
            "sensitive",
            "observe",
            "sensitive",
            "none",
            sensitive_declarations,
            "[]",
        ),
        (
            "protected_inputs",
            "observe",
            "protected_inputs",
            "none",
            sensitive_declarations,
            "[]",
        ),
        (
            "parameters_edge",
            "observe",
            "parameters_edge",
            "none",
            parameter_declarations,
            "[]",
        ),
        (
            "interactive_parameters",
            "observe",
            "success",
            "interactive",
            parameter_declarations,
            "[]",
        ),
        (
            "terminal_none",
            "observe",
            "terminal_noise",
            "none",
            "[]",
            "[]",
        ),
        (
            "terminal_output",
            "observe",
            "terminal_noise",
            "output",
            "[]",
            "[]",
        ),
    ] {
        actions.push_str(&action(id, access, mode, terminal, parameters, outputs));
    }
    format!(
        r#"source_format: 1
package_id: {{package_id}}
revision:
  inputs:
    - {{ id: config, required: true, protection: normal }}
    - {{ id: secret_input, required: false, protection: secret }}
  actions:
{actions}  migrations: []
runtime_content:
  files:
    - {{ id: worker, source: {{worker_name}}, path: bin/{{worker_name}}, executable: true }}
portable_metadata:
  reference_labels:
    - label: stable
      source: {{ kind: unattributed }}
"#
    )
}

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
    stream.write_all(&(payload.len() as u32).to_be_bytes())?;
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
                thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(windows)]
fn connect(endpoint: &str) -> io::Result<fs::File> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match fs::OpenOptions::new().read(true).write(true).open(endpoint) {
            Ok(stream) => return Ok(stream),
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(error),
        }
    }
}

#[test]
pub(crate) fn system_hook_success() {
    hook_worker(false);
}

#[test]
pub(crate) fn system_hook_declared_failure() {
    hook_worker(true);
}

#[test]
pub(crate) fn system_hook() {
    let arguments = env::args().collect::<Vec<_>>();
    let Some(marker) = arguments.iter().find_map(|argument| {
        argument
            .strip_prefix("system-marker:")
            .map(ToOwned::to_owned)
    }) else {
        return;
    };
    let mode = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("system-mode:"))
        .expect("matrix Hook mode is required");
    append_marker(&marker, &format!("launch:{}\n", std::process::id()));
    let Some(transport) = env::var_os(TRANSPORT_ENVIRONMENT) else {
        return;
    };
    if mode == "hang_before_connect" {
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    let endpoint = env::var(ENDPOINT_ENVIRONMENT).unwrap();
    let mut stream = connect(&endpoint).unwrap();
    let mut preamble = [0_u8; PREAMBLE.len()];
    stream.read_exact(&mut preamble).unwrap();
    assert_eq!(preamble, PREAMBLE);
    let session = read_frame(&mut stream).unwrap();
    assert_eq!(session["type"], "session_start");
    let session_id = session["session_id"].as_str().unwrap();
    stream.write_all(PREAMBLE).unwrap();
    if mode == "wrong_protocol_version" {
        write_frame(
            &mut stream,
            &json!({
                "type": "session_ready",
                "protocol_version": 2,
                "session_id": session_id,
            }),
        )
        .unwrap();
        return;
    }
    if mode == "malformed_framing" {
        stream.write_all(&3_u32.to_be_bytes()).unwrap();
        stream.write_all(b"{x}").unwrap();
        stream.flush().unwrap();
        return;
    }
    write_frame(
        &mut stream,
        &json!({
            "type": "session_ready",
            "protocol_version": 1,
            "session_id": session_id,
        }),
    )
    .unwrap();
    append_marker(
        &marker,
        &format!("run:{}\n", session["run_id"].as_str().unwrap()),
    );
    let parameters = session["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|binding| {
            (
                binding["parameter_id"].as_str().unwrap().to_owned(),
                binding["value"].clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let outputs = session["operation"]["outputs"].as_array().unwrap();
    let report = outputs.first();
    let summary = outputs.get(1);
    match mode {
        "success" => finish(&mut stream, "success", Vec::new(), true),
        "parameters" => {
            assert_eq!(parameters["ordinary"], json!("first=equals"));
            assert_eq!(parameters["from_file"], json!("\u{feff}from-file\n"));
            assert_eq!(parameters["from_stdin"], json!("stdin-value\n"));
            assert_eq!(parameters["enabled"], json!(true));
            assert_eq!(parameters["count"], json!(7));
            assert_eq!(parameters["secret"], json!("secret-param-canary"));
            finish(&mut stream, "success", Vec::new(), true);
        }
        "parameters_edge" => {
            assert_eq!(parameters["ordinary"], json!(""));
            assert_eq!(parameters["from_file"], json!("\u{feff}file-語\r\n"));
            assert_eq!(parameters["from_stdin"], json!("stdin-雪\r\n"));
            assert_eq!(parameters["enabled"], json!(false));
            assert_eq!(parameters["count"], json!(-9));
            assert_eq!(parameters["secret"], json!("protected-source-canary"));
            append_marker(&marker, "parameter-edge-values-verified\n");
            finish(&mut stream, "success", Vec::new(), true);
        }
        "output" => {
            let handle = write_report(report);
            finish(&mut stream, "success", vec![handle], true);
        }
        "output_empty" => finish(&mut stream, "success", Vec::new(), true),
        "output_subset" => {
            let handle = write_report(report);
            finish(&mut stream, "success", vec![handle], true);
        }
        "output_multiple" => {
            let report = write_report(report);
            let summary = write_output(summary, b"system-summary");
            finish(&mut stream, "success", vec![report, summary], true);
        }
        "output_group_missing" => {
            let report = write_report(report);
            let summary =
                summary.expect("multi-output action has a summary authority")["handle"].clone();
            finish(&mut stream, "success", vec![report, summary], false);
        }
        "output_failure" => {
            let report = write_report(report);
            let summary = write_output(summary, b"system-summary");
            finish(&mut stream, "failure", vec![report, summary], true);
        }
        "output_missing" => {
            let handle = report.expect("output action has an output authority")["handle"].clone();
            finish(&mut stream, "success", vec![handle], false);
        }
        "output_unsubmitted" => {
            let _ = write_report(report);
            finish(&mut stream, "success", Vec::new(), true);
        }
        "invalid_output" => finish(
            &mut stream,
            "success",
            vec![json!("ffffffffffffffffffffffffffffffff")],
            false,
        ),
        "exit_before_connect" => unreachable!("handled before transport connection"),
        "protocol_error" => {
            write_frame(
                &mut stream,
                &json!({
                    "type": "protocol_error",
                    "code": "sdk_failure",
                    "message": "system Hook protocol failure",
                }),
            )
            .unwrap();
        }
        "declared_failure" => finish(&mut stream, "failure", Vec::new(), true),
        "eof_after_ready" => drop(stream),
        "hang_after_ready" | "late_timeout" | "late_timeout_unaccepted" | "late_cancel" => {
            append_marker(&marker, &format!("ready:{mode}\n"));
            let cancel = read_frame(&mut stream).unwrap();
            assert_eq!(cancel["type"], "cancel");
            let expected_reason = if mode == "late_cancel" {
                "requested"
            } else {
                "timeout"
            };
            assert_eq!(cancel["reason"], expected_reason);
            write_frame(
                &mut stream,
                &json!({"type": "cancel_ack", "control_id": cancel["control_id"]}),
            )
            .unwrap();
            if matches!(mode, "late_timeout" | "late_cancel") {
                finish(&mut stream, "failure", vec![write_report(report)], true);
            } else if mode == "late_timeout_unaccepted" {
                append_marker(&marker, "completion-rejected-control\n");
                finish(
                    &mut stream,
                    "failure",
                    vec![json!("ffffffffffffffffffffffffffffffff")],
                    false,
                );
            } else {
                loop {
                    thread::sleep(Duration::from_secs(1));
                }
            }
        }
        "hold_observe" | "hold_mutate" | "hold_risk" => {
            if mode == "hold_risk" {
                request_risk(&mut stream, 1, "enter_recovery_risk", "open");
            }
            if mode == "hold_observe" {
                append_binding_marker(&session, &marker, "before");
            }
            append_marker(&marker, &format!("ready:{mode}\n"));
            while !Path::new(&format!("{marker}.release")).exists() {
                thread::sleep(Duration::from_millis(20));
            }
            if mode == "hold_observe" {
                append_binding_marker(&session, &marker, "after");
            }
            finish(
                &mut stream,
                if mode == "hold_risk" {
                    "failure"
                } else {
                    "success"
                },
                Vec::new(),
                true,
            );
        }
        "risk" => {
            request_risk(&mut stream, 1, "enter_recovery_risk", "open");
            request_risk(&mut stream, 2, "resolve_recovery_risk", "clear");
            finish(&mut stream, "success", Vec::new(), true);
        }
        "risk_open_failure" | "risk_open_success" => {
            request_risk(&mut stream, 1, "enter_recovery_risk", "open");
            finish(
                &mut stream,
                if mode == "risk_open_failure" {
                    "failure"
                } else {
                    "success"
                },
                Vec::new(),
                mode == "risk_open_failure",
            );
        }
        "sensitive" => {
            let secret = parameters["secret"].as_str().unwrap();
            write_frame(
                &mut stream,
                &json!({
                    "type": "diagnostic",
                    "severity": "warning",
                    "code": "leak_attempt",
                    "message": format!("sensitive Hook free text: {secret}"),
                }),
            )
            .unwrap();
            finish(&mut stream, "success", Vec::new(), true);
        }
        "protected_inputs" => {
            let bindings = session["operation"]["bindings"].as_array().unwrap();
            let binding_bytes = bindings
                .iter()
                .map(|binding| {
                    (
                        binding["input_id"].as_str().unwrap(),
                        fs::read(binding["readonly_path"].as_str().unwrap()).unwrap(),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            assert_eq!(binding_bytes["config"], b"protected-source-canary");
            assert_eq!(binding_bytes["secret_input"], b"secret-input-canary");
            let secret = parameters["secret"].as_str().unwrap();
            assert_eq!(secret, "secret-param-canary");
            write_frame(
                &mut stream,
                &json!({
                    "type": "diagnostic",
                    "severity": "warning",
                    "code": "leak_attempt",
                    // This fixture verifies Pactrun-owned projections, not a
                    // nonexistent universal detector for Hook-authored secrets.
                    "message": "protected context verified",
                }),
            )
            .unwrap();
            append_marker(&marker, "protected-values-verified\n");
            finish(&mut stream, "success", Vec::new(), true);
        }
        "terminal_noise" => {
            let stdout = b"system-hook-terminal-stdout\n";
            let stderr = b"system-hook-terminal-stderr\n";
            io::stdout().write_all(stdout).unwrap();
            io::stdout().flush().unwrap();
            io::stderr().write_all(stderr).unwrap();
            io::stderr().flush().unwrap();
            finish(&mut stream, "success", Vec::new(), true);
        }
        other => panic!("unknown system Hook mode {other}"),
    }
    assert!(!transport.is_empty());
}

fn finish(
    stream: &mut impl ReadWrite,
    status: &str,
    produced_outputs: Vec<Value>,
    expect_accepted: bool,
) {
    write_frame(
        stream,
        &json!({
            "type": "complete",
            "operation": "action",
            "status": status,
            "produced_outputs": produced_outputs,
        }),
    )
    .unwrap();
    if expect_accepted {
        assert_eq!(read_frame(stream).unwrap()["type"], "completion_accepted");
    }
}

fn write_report(report: Option<&Value>) -> Value {
    write_output(report, b"system-report")
}

fn write_output(output: Option<&Value>, bytes: &[u8]) -> Value {
    let output = output.expect("output action has an output authority");
    fs::write(output["staged_path"].as_str().unwrap(), bytes).unwrap();
    output["handle"].clone()
}

fn append_binding_marker(session: &Value, marker: &str, phase: &str) {
    let binding = &session["operation"]["bindings"][0];
    let bytes = fs::read(binding["readonly_path"].as_str().unwrap()).unwrap();
    append_marker(
        marker,
        &format!(
            "binding:{phase}:{}\n",
            String::from_utf8(bytes).expect("matrix binding is UTF-8")
        ),
    );
}

fn append_marker(marker: &str, value: &str) {
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(marker)
        .unwrap()
        .write_all(value.as_bytes())
        .unwrap();
}

fn request_risk(stream: &mut impl ReadWrite, request_id: u64, kind: &str, expected: &str) {
    write_frame(
        stream,
        &json!({"type": "request", "request_id": request_id, "request": {"kind": kind}}),
    )
    .unwrap();
    let acknowledgement = read_frame(stream).unwrap();
    assert_eq!(acknowledgement["type"], "request_ack");
    assert_eq!(acknowledgement["request_id"], request_id);
    assert_eq!(acknowledgement["risk_state"], expected);
}

trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

fn hook_worker(declared_failure: bool) {
    let Some(marker) = env::args().find_map(|argument| {
        argument
            .strip_prefix("system-marker:")
            .map(ToOwned::to_owned)
    }) else {
        return;
    };
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(marker)
        .unwrap()
        .write_all(format!("launch:{}\n", std::process::id()).as_bytes())
        .unwrap();
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
    if declared_failure {
        write_frame(
            &mut stream,
            &json!({
                "type": "complete",
                "operation": "action",
                "status": "failure",
                "code": "fixture.declared_failure",
                "message": "fixture failure",
                "produced_outputs": [],
            }),
        )
        .unwrap();
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
    }
    let accepted = read_frame(&mut stream).unwrap();
    assert_eq!(accepted["type"], "completion_accepted");
    assert!(!transport.is_empty());
}
