//! Slice 4 contract tests.
//!
//! Real Hooks are this test binary launched as an Action Hook: the `direct`
//! and `interpreted` Actions execute a materialized copy of `current_exe()`
//! filtered to `m3_hook_worker`, which speaks Frozen HookProtocolV1 over the
//! discovered transport and acts according to the `mode` parameter.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{self, Cursor, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use tempfile::TempDir;

use super::{
    ActionCancellation, HookRuntimePolicy, OwnerContinuation, RuntimeTerminalFacts,
    checked_deadline,
    platform::ProtocolListener,
    protocol::{self, PREAMBLE, ProtocolState, ProtocolStep, WireEvent},
};
use crate::{
    application::{InputAcquisition, PactrunApplication},
    domain::{
        ActionIdentity, ActionPlanStep, ActionRunBoundary, HookCompletionStatus, InputIdentity,
        InstanceName, InstanceView, ManagedOutputIdentity, ParameterIdentity, ParameterTextSource,
        RawParameterInput, RecoveryRiskState, RevisionMetadataMutationBatch, RunFailedStep, RunId,
        RunOutcome, RunState,
    },
    executor::{AdmissionOptions, AdmittedExecution},
    managed_data::{fail_next_execution_cleanup, session_is_live},
    persistence::PactrunPersistence,
};

const WORKER_TEST: &str = "hook::tests::m3_hook_worker";
const TRANSPORT_ENVIRONMENT: &str = "PACTRUN_HOOK_PROTOCOL_TRANSPORT";
const ENDPOINT_ENVIRONMENT: &str = "PACTRUN_HOOK_PROTOCOL_ENDPOINT";
const SECRET_BINDING: &[u8] = b"slice4-secret-binding";
const SENSITIVE_PARAMETER: &str = "slice4-sensitive-parameter";
const ARGUMENT_TAIL: &str = "slice4-argument-tail";
const MODE_PREFIX: &str = "pactrun-hook-mode:";
const LINGER: Duration = Duration::from_millis(1500);
const WAIT_LIMIT: Duration = Duration::from_secs(60);

// Test-ID: PR-TEST-0122
// Verifies: PR-REQ-0286
#[test]
fn launch_cancellation_gate_has_deterministic_concurrent_ordering() {
    let spawned = Arc::new(AtomicUsize::new(0));
    let cancellation = ActionCancellation::default();
    let before = Arc::new(Barrier::new(2));
    let after = Arc::new(Barrier::new(2));
    let _gate_entered = cancellation.install_launch_test_hooks(Arc::clone(&before), after.clone());
    let launch = {
        let cancellation = cancellation.clone();
        let spawned = Arc::clone(&spawned);
        thread::spawn(move || {
            cancellation.arbitrate_launch(|| {
                spawned.fetch_add(1, Ordering::AcqRel);
                Ok::<_, ()>(())
            })
        })
    };
    // The request is concurrent with the launch attempt, but the launch
    // thread is held at its pre-gate seam, so this ordering is deterministic.
    let request = {
        let cancellation = cancellation.clone();
        thread::spawn(move || cancellation.request())
    };
    request.join().unwrap();
    before.wait();
    after.wait();
    assert_eq!(launch.join().unwrap().unwrap(), None);
    assert_eq!(spawned.load(Ordering::Acquire), 0);

    let spawned = Arc::new(AtomicUsize::new(0));
    let cancellation = ActionCancellation::default();
    let before = Arc::new(Barrier::new(2));
    let after = Arc::new(Barrier::new(2));
    let gate_entered = cancellation.install_launch_test_hooks(before.clone(), after.clone());
    let launch = {
        let cancellation = cancellation.clone();
        let spawned = Arc::clone(&spawned);
        thread::spawn(move || {
            cancellation.arbitrate_launch(|| {
                spawned.fetch_add(1, Ordering::AcqRel);
                Ok::<_, ()>("supervised-process")
            })
        })
    };
    before.wait();
    while !gate_entered.load(Ordering::Acquire) {
        thread::yield_now();
    }
    let (started, ready) = mpsc::channel();
    let request = {
        let cancellation = cancellation.clone();
        thread::spawn(move || {
            started.send(()).unwrap();
            cancellation.request();
        })
    };
    ready.recv().unwrap();
    after.wait();
    assert_eq!(launch.join().unwrap().unwrap(), Some("supervised-process"));
    request.join().unwrap();
    assert_eq!(spawned.load(Ordering::Acquire), 1);
    assert!(cancellation.is_requested());
}

// Test-ID: PR-TEST-0129
// Verifies: PR-REQ-0286, PR-REQ-0288
#[test]
fn runtime_deadline_validation_accepts_zero_and_real_boundary_only() {
    assert!(HookRuntimePolicy::from_millis(Some(0), Some(1), Some(5_000)).is_ok());
    assert!(HookRuntimePolicy::from_millis(Some(5_000), Some(60_000), Some(5_000)).is_ok());
    let maximum = i64::MAX as u64;
    assert!(HookRuntimePolicy::from_millis(Some(maximum), None, None).is_ok());
    assert!(HookRuntimePolicy::from_millis(Some(maximum + 1), None, None).is_err());
    assert!(HookRuntimePolicy::from_millis(Some(u64::MAX), None, None).is_err());

    let start = Instant::now();
    assert!(checked_deadline(start, Duration::from_millis(maximum)).is_some());
    assert!(checked_deadline(start, Duration::MAX).is_none());
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

struct RuntimeFixture {
    temporary: TempDir,
    storage: PathBuf,
    launcher: PathBuf,
    application: PactrunApplication,
    instance: InstanceView,
}

impl RuntimeFixture {
    fn new() -> Self {
        Self::with_source(|_, _, _| {})
    }

    fn with_source(configure: impl FnOnce(&Path, &Path, &mut String)) -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m3-slice4-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("hook-")
            .tempdir_in(parent)
            .unwrap();
        let storage = temporary.path().join("storage");
        let source = temporary.path().join("source");
        let launcher = temporary.path().join("launcher");
        for directory in [&storage, &source, &launcher] {
            fs::create_dir(directory).unwrap();
        }
        for child in ["database", "runtime-content", "staging"] {
            fs::create_dir(storage.join(child)).unwrap();
        }
        let current_exe = env::current_exe().unwrap();
        fs::copy(&current_exe, source.join(worker_file_name())).unwrap();
        fs::copy(&current_exe, launcher.join(launcher_command())).unwrap();
        fs::write(
            source.join("hook-script.txt"),
            b"interpreter script authority",
        )
        .unwrap();
        fs::write(source.join("broken.bin"), b"not an executable image").unwrap();
        let mut manifest = manifest();
        configure(&source, &launcher, &mut manifest);
        fs::write(source.join("pactrun.yaml"), manifest).unwrap();

        let application = PactrunApplication::open(&storage).unwrap();
        let metadata = RevisionMetadataMutationBatch::new(Vec::new()).unwrap();
        let revision = application
            .install_pack_source(&source, &metadata)
            .unwrap()
            .revision;
        let instance = application
            .create_instance(
                InstanceName::parse("slice4").unwrap(),
                revision,
                vec![InputAcquisition {
                    input_id: InputIdentity::parse("secret_config").unwrap(),
                    source: Box::new(Cursor::new(SECRET_BINDING.to_vec())),
                }],
            )
            .unwrap();
        Self {
            temporary,
            storage,
            launcher,
            application,
            instance,
        }
    }

    fn marker(&self, name: &str) -> PathBuf {
        self.temporary.path().join(name)
    }

    fn admit(&self, action: &str, mode: &str, marker: &Path) -> AdmittedExecution {
        let intent = self
            .application
            .resolve_action(
                &InstanceName::parse("slice4").unwrap(),
                &ActionIdentity::parse(action).unwrap(),
                parameters(mode, marker),
            )
            .unwrap();
        let plan = self
            .application
            .compile_action(&intent, std::slice::from_ref(&self.launcher))
            .unwrap();
        self.application
            .accept_and_admit_action(&plan, AdmissionOptions::default())
            .unwrap()
    }

    fn execute(
        &self,
        action: &str,
        mode: &str,
        marker: &Path,
        policy: HookRuntimePolicy,
    ) -> RuntimeTerminalFacts {
        self.execute_with(action, mode, marker, policy, ActionCancellation::default())
    }

    fn execute_with(
        &self,
        action: &str,
        mode: &str,
        marker: &Path,
        policy: HookRuntimePolicy,
        cancellation: ActionCancellation,
    ) -> RuntimeTerminalFacts {
        let admitted = self.admit(action, mode, marker);
        let run = self
            .application
            .execute_admitted_action(admitted, policy, cancellation);
        take_facts(&self.application, run)
    }

    /// Replaces the current binding so a later Hook can only see the original
    /// bytes through its pinned view.
    fn replace_binding(&self) {
        let current = self
            .application
            .load_instance(self.instance.id)
            .unwrap()
            .unwrap()
            .state_version;
        self.application
            .set_input(
                self.instance.id,
                InputIdentity::parse("secret_config").unwrap(),
                current,
                Box::new(Cursor::new(b"replacement-binding".to_vec())),
            )
            .unwrap();
    }
}

fn worker_file_name() -> &'static str {
    if cfg!(windows) {
        "worker.exe"
    } else {
        "worker"
    }
}

fn launcher_command() -> &'static str {
    if cfg!(windows) {
        "pactrun-hook-test.exe"
    } else {
        "pactrun-hook-test"
    }
}

fn direct_action(id: &str, terminal: &str, extra_arg: Option<&str>) -> String {
    direct_action_with_outputs(id, terminal, extra_arg, &["report"])
}

fn direct_action_with_outputs(
    id: &str,
    terminal: &str,
    extra_arg: Option<&str>,
    output_ids: &[&str],
) -> String {
    let extra = extra_arg
        .map(|arg| format!(", \"{arg}\""))
        .unwrap_or_default();
    let outputs = output_ids
        .iter()
        .map(|id| format!("{{ id: {id} }}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"    - id: {id}
      access: observe
      parameters:
        - {{ id: mode, type: string, sensitive: false }}
        - {{ id: marker, type: string, sensitive: false }}
        - {{ id: expected_binding, type: string, sensitive: true }}
        - {{ id: sensitive_value, type: string, sensitive: true }}
      hook:
        protocol_version: 1
        launch: {{ kind: direct, executable: worker }}
        args: ["--exact", "{WORKER_TEST}", "--nocapture", "--test-threads=1", "{ARGUMENT_TAIL}"{extra}]
        io: {{ terminal: {terminal} }}
      outputs: [{outputs}]
"#
    )
}

fn manifest() -> String {
    let mut actions = String::new();
    actions.push_str(&direct_action("direct", "none", None));
    actions.push_str(&direct_action_with_outputs(
        "output_subset",
        "none",
        None,
        &["report", "secondary"],
    ));
    actions.push_str(&direct_action("output_terminal", "output", None));
    actions.push_str(&direct_action("interactive_terminal", "interactive", None));
    actions.push_str(&direct_action(
        "exit_before_connect",
        "none",
        Some(&format!("{MODE_PREFIX}exit_before_connect")),
    ));
    actions.push_str(&direct_action(
        "hang_before_connect",
        "none",
        Some(&format!("{MODE_PREFIX}hang_before_connect")),
    ));
    actions.push_str(&format!(
        r#"    - id: interpreted
      access: observe
      parameters:
        - {{ id: mode, type: string, sensitive: false }}
        - {{ id: marker, type: string, sensitive: false }}
        - {{ id: expected_binding, type: string, sensitive: true }}
        - {{ id: sensitive_value, type: string, sensitive: true }}
      hook:
        protocol_version: 1
        launch: {{ kind: interpreter, command: {launcher}, interpreter_args: ["--exact", "{WORKER_TEST}", "--nocapture", "--skip"], script: hook_script }}
        args: ["--test-threads=1", "{ARGUMENT_TAIL}"]
        io: {{ terminal: none }}
      outputs: [{{ id: report }}]
    - id: broken_launch
      access: observe
      parameters:
        - {{ id: mode, type: string, sensitive: false }}
        - {{ id: marker, type: string, sensitive: false }}
        - {{ id: expected_binding, type: string, sensitive: true }}
        - {{ id: sensitive_value, type: string, sensitive: true }}
      hook:
        protocol_version: 1
        launch: {{ kind: direct, executable: broken }}
        args: []
        io: {{ terminal: none }}
      outputs: [{{ id: report }}]
"#,
        launcher = launcher_command(),
    ));
    format!(
        r#"source_format: 1
package_id: 00000000000000000000000000000034
revision:
  inputs:
    - {{ id: secret_config, required: true, protection: secret }}
  actions:
{actions}  migrations: []
runtime_content:
  files:
    - {{ id: worker, source: {worker}, path: bin/{worker}, executable: true }}
    - {{ id: hook_script, source: hook-script.txt, path: bin/hook-script.txt }}
    - {{ id: broken, source: broken.bin, path: bin/broken.bin, executable: true }}
"#,
        worker = worker_file_name(),
    )
}

fn parameters(mode: &str, marker: &Path) -> Vec<RawParameterInput> {
    [
        ("mode", mode.to_owned(), ParameterTextSource::Ordinary),
        (
            "marker",
            marker.to_str().unwrap().to_owned(),
            ParameterTextSource::Ordinary,
        ),
        (
            "expected_binding",
            String::from_utf8(SECRET_BINDING.to_vec()).unwrap(),
            ParameterTextSource::Protected,
        ),
        (
            "sensitive_value",
            SENSITIVE_PARAMETER.to_owned(),
            ParameterTextSource::Protected,
        ),
    ]
    .into_iter()
    .map(|(id, text, source)| RawParameterInput {
        id: ParameterIdentity::parse(id).unwrap(),
        text,
        source,
    })
    .collect()
}

fn take_facts(application: &PactrunApplication, run: RunId) -> RuntimeTerminalFacts {
    let guard = application
        .take_owner_continuation(run)
        .expect("Run has an owner continuation");
    match guard.complete() {
        OwnerContinuation::ReadyToFinalize(state) => state.facts,
        other => panic!("expected terminal facts, found {other:?}"),
    }
}

fn failure_ref(facts: &RuntimeTerminalFacts) -> Option<(&str, &str, RunFailedStep)> {
    facts.primary_failure.as_ref().map(|failure| {
        (
            failure.failure.error.owner(),
            failure.failure.error.code(),
            failure.step,
        )
    })
}

fn persistence(storage: &Path) -> PactrunPersistence {
    PactrunPersistence::open(storage).unwrap()
}

fn load_run(storage: &Path, run: RunId) -> crate::domain::RunView {
    persistence(storage).load_run(run).unwrap().unwrap()
}

fn running_risk(storage: &Path, run: RunId) -> RecoveryRiskState {
    match load_run(storage, run).state {
        RunState::Running(execution) => {
            assert_eq!(execution.boundary, ActionRunBoundary::Admitted);
            execution.risk_state
        }
        other => panic!("expected Running Run, found {other:?}"),
    }
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + WAIT_LIMIT;
    while !path.exists() {
        assert!(Instant::now() < deadline, "timed out waiting for {path:?}");
        thread::sleep(Duration::from_millis(5));
    }
}

fn marker_variant(marker: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}.{}", marker.to_str().unwrap(), suffix))
}

fn execution_root(facts: &RuntimeTerminalFacts) -> PathBuf {
    facts.outputs[0]
        .path
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Requests cancellation from another thread once the Hook reports the
/// named marker, so cancellation is asynchronous to the runtime loop. The
/// handle yields the instant at which cancellation was requested.
fn cancel_after(marker: PathBuf, cancellation: &ActionCancellation) -> thread::JoinHandle<Instant> {
    let cancellation = cancellation.clone();
    thread::spawn(move || {
        wait_for(&marker);
        cancellation.request();
        Instant::now()
    })
}

fn policy(
    startup_timeout: Option<u64>,
    action_timeout: Option<u64>,
    termination_grace: Option<u64>,
) -> HookRuntimePolicy {
    HookRuntimePolicy {
        startup_timeout: startup_timeout.map(Duration::from_millis),
        action_timeout: action_timeout.map(Duration::from_millis),
        termination_grace: termination_grace.map(Duration::from_millis),
    }
}

fn database_bytes(storage: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in fs::read_dir(storage.join("database")).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            bytes.extend(fs::read(&path).unwrap());
        }
    }
    bytes
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

// ---------------------------------------------------------------------------
// The Hook side: this test binary launched as the Action Hook.
// ---------------------------------------------------------------------------

#[cfg(windows)]
type HookStream = fs::File;

#[cfg(unix)]
type HookStream = std::os::unix::net::UnixStream;

#[cfg(windows)]
fn connect_hook(transport: &str, endpoint: &str) -> HookStream {
    assert_eq!(transport, "windows-named-pipe");
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(endpoint)
        .unwrap()
}

#[cfg(unix)]
fn connect_hook(transport: &str, endpoint: &str) -> HookStream {
    assert_eq!(transport, "unix-domain-socket");
    HookStream::connect(endpoint).unwrap()
}

fn write_frame(stream: &mut impl Write, value: &Value) {
    let payload = serde_json::to_vec(value).unwrap();
    stream
        .write_all(&u32::try_from(payload.len()).unwrap().to_be_bytes())
        .unwrap();
    stream.write_all(&payload).unwrap();
    stream.flush().unwrap();
}

fn read_frame(stream: &mut impl Read) -> Option<Value> {
    let mut length = [0_u8; 4];
    stream.read_exact(&mut length).ok()?;
    let mut payload = vec![0_u8; u32::from_be_bytes(length) as usize];
    stream.read_exact(&mut payload).ok()?;
    Some(serde_json::from_slice(&payload).unwrap())
}

fn frame_bytes(value: &Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_frame(&mut bytes, value);
    bytes
}

#[test]
fn m3_hook_worker() {
    let arguments: Vec<String> = env::args().skip(1).collect();
    // The Unix interactive test adapter is itself a libtest process. It
    // inherits the owner's protocol environment while running the adapter
    // test, but that process is not the Hook and must not compete for the
    // owner's one-connection listener.
    if env::var_os("PACTRUN_INTERNAL_INTERACTIVE_ADAPTER_READY").is_some() {
        return;
    }
    let Some(transport) = env::var_os(TRANSPORT_ENVIRONMENT) else {
        return;
    };
    if let Some(mode) = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix(MODE_PREFIX))
    {
        match mode {
            "exit_before_connect" => return,
            "hang_before_connect" => loop {
                thread::sleep(Duration::from_secs(1));
            },
            other => panic!("unknown pre-connect mode {other}"),
        }
    }
    let transport = transport.into_string().unwrap();
    let endpoint = env::var(ENDPOINT_ENVIRONMENT).unwrap();
    let stream = connect_hook(&transport, &endpoint);
    HookWorker {
        stream,
        arguments,
        transport,
        endpoint,
    }
    .run();
}

struct HookWorker {
    stream: HookStream,
    arguments: Vec<String>,
    transport: String,
    endpoint: String,
}

impl HookWorker {
    fn run(mut self) {
        let mut preamble = [0_u8; PREAMBLE.len()];
        self.stream.read_exact(&mut preamble).unwrap();
        assert_eq!(preamble, PREAMBLE);
        let session = read_frame(&mut self.stream).expect("session_start");
        assert_eq!(session["type"], "session_start");
        assert_eq!(session["protocol_version"], 1);
        let session_id = session["session_id"].as_str().unwrap().to_owned();
        self.stream.write_all(PREAMBLE).unwrap();
        write_frame(
            &mut self.stream,
            &json!({"type": "session_ready", "protocol_version": 1, "session_id": session_id}),
        );

        let parameters: BTreeMap<String, Value> = session["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|binding| {
                (
                    binding["parameter_id"].as_str().unwrap().to_owned(),
                    binding["value"].clone(),
                )
            })
            .collect();
        let mode = parameters["mode"].as_str().unwrap().to_owned();
        let marker = PathBuf::from(parameters["marker"].as_str().unwrap());
        fs::write(marker_variant(&marker, "session"), session.to_string()).unwrap();
        fs::write(marker_variant(&marker, "args"), self.arguments.join("\n")).unwrap();
        fs::write(
            marker_variant(&marker, "env"),
            format!("{}\n{}", self.transport, self.endpoint),
        )
        .unwrap();

        let binding = &session["operation"]["bindings"][0];
        assert_eq!(binding["input_id"], "secret_config");
        assert_eq!(binding["role"], "active");
        let pinned = fs::read(binding["readonly_path"].as_str().unwrap()).unwrap();
        let expected = parameters["expected_binding"].as_str().unwrap().as_bytes();
        fs::write(
            marker_variant(&marker, "binding"),
            if pinned == expected {
                "pinned"
            } else {
                "replaced"
            },
        )
        .unwrap();

        let terminal = session["io"]["terminal"].as_str().unwrap().to_owned();
        let report = session["operation"]["outputs"][0].clone();
        assert_eq!(report["output_id"], "report");
        let report_handle = report["handle"].as_str().unwrap().to_owned();
        let sensitive = parameters["sensitive_value"].as_str().unwrap().to_owned();

        match mode.as_str() {
            "success" => {
                self.complete(json!({"status": "success", "produced_outputs": []}));
                self.expect_accepted();
            }
            "output" | "output_subset" => {
                fs::write(
                    report["staged_path"].as_str().unwrap(),
                    b"unpublished-output",
                )
                .unwrap();
                self.complete(json!({"status": "success", "produced_outputs": [report_handle]}));
                self.expect_accepted();
            }
            "hook_failure" => {
                self.complete(json!({
                    "status": "failure",
                    "code": "service_failed",
                    "message": "diagnostic artifact retained",
                    "produced_outputs": [],
                }));
                self.expect_accepted();
            }
            "finalization_text_marker" => {
                write_frame(
                    &mut self.stream,
                    &json!({
                        "type": "diagnostic",
                        "severity": "warning",
                        "code": "hook_diagnostic_marker",
                        "message": "slice5_hook_diagnostic_marker",
                    }),
                );
                self.complete(json!({
                    "status": "failure",
                    "code": "hook_completion_marker",
                    "message": "slice5_hook_completion_marker",
                    "produced_outputs": [],
                }));
                self.expect_accepted();
            }
            "risk" => {
                fs::write(marker_variant(&marker, "request"), b"").unwrap();
                self.request(1, "enter_recovery_risk", "open");
                fs::write(marker_variant(&marker, "ack"), b"").unwrap();
                self.request(2, "resolve_recovery_risk", "clear");
                self.complete(json!({"status": "success", "produced_outputs": []}));
                self.expect_accepted();
            }
            "risk_open_failure" => {
                self.request(1, "enter_recovery_risk", "open");
                self.complete(json!({
                    "status": "failure",
                    "code": "service_incoherent",
                    "produced_outputs": [],
                }));
                self.expect_accepted();
            }
            "invalid_output" => {
                self.complete(json!({
                    "status": "success",
                    "produced_outputs": ["ffffffffffffffffffffffffffffffff"],
                }));
                if let Some(reply) = read_frame(&mut self.stream) {
                    fs::write(marker_variant(&marker, "protocol_error"), reply.to_string())
                        .unwrap();
                }
            }
            "sensitive_diagnostic" => {
                write_frame(
                    &mut self.stream,
                    &json!({
                        "type": "diagnostic",
                        "severity": "warning",
                        "code": "leak_attempt",
                        "message": format!("{} {sensitive}", String::from_utf8_lossy(expected)),
                    }),
                );
                self.complete(json!({"status": "success", "produced_outputs": []}));
                self.expect_accepted();
            }
            "hang_after_ready" => {
                fs::write(marker_variant(&marker, "ready"), b"").unwrap();
                let cancel = self.expect_cancel();
                fs::write(
                    marker_variant(&marker, "cancel"),
                    cancel["reason"].as_str().unwrap(),
                )
                .unwrap();
                self.cancel_ack(&cancel);
                loop {
                    thread::sleep(Duration::from_secs(1));
                }
            }
            "cancel_then_complete" => {
                fs::write(marker_variant(&marker, "ready"), b"").unwrap();
                let cancel = self.expect_cancel();
                fs::write(
                    marker_variant(&marker, "cancel"),
                    cancel["reason"].as_str().unwrap(),
                )
                .unwrap();
                self.cancel_ack(&cancel);
                self.complete(json!({
                    "status": "failure",
                    "code": "cancelled",
                    "produced_outputs": [],
                }));
                self.expect_accepted();
                fs::write(marker_variant(&marker, "accepted"), b"").unwrap();
            }
            "cancel_then_output" => {
                fs::write(marker_variant(&marker, "ready"), b"").unwrap();
                let cancel = self.expect_cancel();
                fs::write(
                    marker_variant(&marker, "cancel"),
                    cancel["reason"].as_str().unwrap(),
                )
                .unwrap();
                self.cancel_ack(&cancel);
                fs::write(report["staged_path"].as_str().unwrap(), b"late-output").unwrap();
                self.complete(json!({
                    "status": "failure",
                    "code": "cancelled",
                    "produced_outputs": [report_handle],
                }));
                self.expect_accepted();
                fs::write(marker_variant(&marker, "accepted"), b"").unwrap();
            }
            "eof_after_ready" => {
                fs::write(marker_variant(&marker, "ready"), b"").unwrap();
                drop(self.stream);
            }
            "crash_after_ready" => {
                fs::write(marker_variant(&marker, "ready"), b"").unwrap();
                std::process::exit(3);
            }
            "complete_then_linger" => {
                self.complete(json!({"status": "success", "produced_outputs": []}));
                self.expect_accepted();
                fs::write(marker_variant(&marker, "lingering"), b"").unwrap();
                thread::sleep(LINGER);
                fs::write(marker_variant(&marker, "exiting"), b"").unwrap();
            }
            "protocol_error" => {
                write_frame(
                    &mut self.stream,
                    &json!({
                        "type": "protocol_error",
                        "code": "sdk_failure",
                        "message": "the Hook cannot continue",
                    }),
                );
            }
            "terminal_noise" => {
                // Frozen protocol bytes on the terminal channels must be
                // ignored by the runtime: they are not the protocol stream.
                let mut noise = b"pactrun-hook-terminal-noise\n".to_vec();
                noise.extend_from_slice(PREAMBLE);
                noise.extend(frame_bytes(&json!({
                    "type": "complete",
                    "operation": "action",
                    "status": "failure",
                    "produced_outputs": [],
                })));
                io::stdout().write_all(&noise).unwrap();
                io::stdout().flush().unwrap();
                io::stderr().write_all(&noise).unwrap();
                io::stderr().flush().unwrap();
                if terminal != "interactive" {
                    let mut buffer = [0_u8; 16];
                    let observation = match io::stdin().read(&mut buffer) {
                        Ok(0) => "eof".to_owned(),
                        Ok(read) => format!("data:{read}"),
                        Err(error) => format!("error:{}", error.kind()),
                    };
                    fs::write(marker_variant(&marker, "stdin"), observation).unwrap();
                }
                self.complete(json!({"status": "success", "produced_outputs": []}));
                self.expect_accepted();
            }
            other => panic!("unknown Hook mode {other}"),
        }
    }

    fn complete(&mut self, mut completion: Value) {
        let object = completion.as_object_mut().unwrap();
        object.insert("type".to_owned(), json!("complete"));
        object.insert("operation".to_owned(), json!("action"));
        write_frame(&mut self.stream, &completion);
    }

    fn expect_accepted(&mut self) {
        let reply = read_frame(&mut self.stream).expect("completion_accepted");
        assert_eq!(reply["type"], "completion_accepted");
    }

    fn request(&mut self, request_id: u64, kind: &str, expected_risk: &str) {
        write_frame(
            &mut self.stream,
            &json!({"type": "request", "request_id": request_id, "request": {"kind": kind}}),
        );
        let ack = read_frame(&mut self.stream).expect("request_ack");
        assert_eq!(ack["type"], "request_ack");
        assert_eq!(ack["request_id"], request_id);
        assert_eq!(ack["risk_state"], expected_risk);
    }

    fn expect_cancel(&mut self) -> Value {
        let cancel = read_frame(&mut self.stream).expect("cancel");
        assert_eq!(cancel["type"], "cancel");
        cancel
    }

    fn cancel_ack(&mut self, cancel: &Value) {
        write_frame(
            &mut self.stream,
            &json!({"type": "cancel_ack", "control_id": cancel["control_id"]}),
        );
    }
}

// ---------------------------------------------------------------------------
// Frozen vector conformance of the production Action state machine.
// ---------------------------------------------------------------------------

fn vectors() -> Value {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/hook_protocol_v1/vectors.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// Session fixtures whose operation is an Action, keyed by fixture name and
/// reduced to what the production state machine needs.
fn action_sessions(vectors: &Value) -> BTreeMap<String, (String, BTreeSet<String>)> {
    vectors["session_specs"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|spec| {
            let session: Value = serde_json::from_str(spec["raw_json"].as_str().unwrap()).unwrap();
            (session["operation"]["kind"] == "action").then(|| {
                let outputs = session["operation"]["outputs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|output| output["handle"].as_str().unwrap().to_owned())
                    .collect();
                (
                    spec["name"].as_str().unwrap().to_owned(),
                    (session["session_id"].as_str().unwrap().to_owned(), outputs),
                )
            })
        })
        .collect()
}

/// Drives one transcript through the production decoder and state machine.
/// Pactrun-direction messages are checked against what the runtime would
/// emit at that point. Returns the Frozen `expected_state` shape or the
/// failing code.
fn run_action_transcript(
    session_id: &str,
    outputs: &BTreeSet<String>,
    messages: &[Value],
) -> Result<Value, String> {
    let mut state = ProtocolState::new(session_id.to_owned(), outputs.clone());
    let mut completion = None;
    let mut submitted = Vec::new();
    let mut pending = None;
    for message in messages {
        let raw = message["raw_json"].as_str().unwrap();
        if message["direction"] == "hook_to_pactrun" {
            let decoded =
                protocol::parse_hook_message(raw.as_bytes()).map_err(|f| f.code.to_owned())?;
            match state.accept(decoded).map_err(|f| f.code.to_owned())? {
                ProtocolStep::RiskRequest {
                    request_id,
                    requested,
                } => pending = Some((request_id, requested)),
                ProtocolStep::Completed(done) => {
                    submitted = done.produced_outputs.clone();
                    completion = Some("submitted");
                }
                ProtocolStep::HookProtocolError => completion = Some("protocol_error"),
                ProtocolStep::Ready
                | ProtocolStep::Diagnostic
                | ProtocolStep::CancelAcknowledged => {}
            }
        } else {
            let value: Value = serde_json::from_str(raw).unwrap();
            match value["type"].as_str().unwrap() {
                "request_ack" => {
                    let (request_id, requested) = pending.take().expect("outstanding request");
                    if value["request_id"] != request_id {
                        return Err("request_id_mismatch".to_owned());
                    }
                    state.acknowledge_request(request_id, requested);
                }
                "cancel" => {
                    let control_id = state.begin_cancel().expect("Session accepts cancellation");
                    assert_eq!(value["control_id"], control_id);
                }
                "completion_accepted" => {
                    assert_eq!(completion, Some("submitted"));
                    completion = Some("accepted");
                }
                other => panic!("unexpected Pactrun message {other} in vector"),
            }
        }
    }
    if let Some(failure) = state.end_of_stream() {
        return Err(failure.code.to_owned());
    }
    let risk = match state.risk() {
        RecoveryRiskState::Clear => "clear",
        RecoveryRiskState::Open => "open",
    };
    let mut summary = json!({
        "completion": completion.expect("terminal transcript"),
        "operation": "action",
        "risk_state": risk,
    });
    if completion == Some("accepted") {
        submitted.sort();
        summary["submitted_outputs"] = json!(submitted);
    }
    Ok(summary)
}

fn transport_fault_code(vector: &Value) -> String {
    let fault = &vector["transport"];
    let mut bytes = Vec::new();
    match fault["kind"].as_str().unwrap() {
        "preamble" => bytes = hex::decode(fault["hex"].as_str().unwrap()).unwrap(),
        "declared_length" => {
            bytes.extend_from_slice(PREAMBLE);
            bytes.extend_from_slice(
                &u32::try_from(fault["length"].as_u64().unwrap())
                    .unwrap()
                    .to_be_bytes(),
            );
        }
        "payload_hex" => {
            bytes.extend_from_slice(PREAMBLE);
            let payload = hex::decode(fault["hex"].as_str().unwrap()).unwrap();
            bytes.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
            bytes.extend_from_slice(&payload);
        }
        other => panic!("unknown transport fault {other}"),
    }
    let mut events = Vec::new();
    protocol::read_wire_events(&mut Cursor::new(bytes), |event| {
        events.push(event);
        true
    });
    match events.first() {
        Some(WireEvent::Failure(failure)) => failure.code.to_owned(),
        other => panic!("transport fault vector produced {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Slice 4 contract tests
// ---------------------------------------------------------------------------

// Test-ID: PR-TEST-0093
// Verifies: PR-REQ-0168, PR-REQ-0205, PR-REQ-0206, PR-REQ-0207, PR-REQ-0208, PR-REQ-0215, PR-REQ-0216, PR-REQ-0217
#[test]
fn production_action_protocol_matches_frozen_vectors_and_real_hooks() {
    // Part 1: every Frozen Action vector through the production decoder and
    // state machine.
    let vectors = vectors();
    assert_eq!(
        hex::decode(vectors["preamble_hex"].as_str().unwrap()).unwrap(),
        PREAMBLE
    );
    let sessions = action_sessions(&vectors);
    let mut executed = BTreeSet::new();
    for vector in vectors["valid"].as_array().unwrap() {
        let Some((session_id, outputs)) = vector["session"].as_str().and_then(|s| sessions.get(s))
        else {
            continue;
        };
        let name = vector["name"].as_str().unwrap();
        let summary =
            run_action_transcript(session_id, outputs, vector["messages"].as_array().unwrap())
                .unwrap_or_else(|code| panic!("valid vector {name} failed with {code}"));
        for (key, expected) in vector["expected_state"].as_object().unwrap() {
            assert_eq!(&summary[key], expected, "vector {name} field {key}");
        }
        executed.insert(name.to_owned());
    }
    for vector in vectors["invalid"].as_array().unwrap() {
        let name = vector["name"].as_str().unwrap();
        let expected = vector["expected_error"].as_str().unwrap();
        let code = if vector.get("transport").is_some() {
            transport_fault_code(vector)
        } else if let Some((session_id, outputs)) =
            vector["session"].as_str().and_then(|s| sessions.get(s))
        {
            match run_action_transcript(session_id, outputs, vector["messages"].as_array().unwrap())
            {
                Err(code) => code,
                Ok(summary) => panic!("invalid vector {name} was accepted as {summary}"),
            }
        } else if matches!(
            expected,
            "invalid_json" | "duplicate_property" | "invalid_unicode_scalar"
        ) {
            // Strict JSON profile faults are direction-independent: the same
            // decoder rejects them before any message type is considered.
            vector["messages"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|message| {
                    protocol::parse_hook_message(message["raw_json"].as_str().unwrap().as_bytes())
                        .err()
                        .map(|failure| failure.code.to_owned())
                })
                .unwrap_or_else(|| panic!("vector {name} was accepted"))
        } else {
            continue;
        };
        assert_eq!(code, expected, "invalid vector {name}");
        executed.insert(name.to_owned());
    }
    let expected: BTreeSet<String> = [
        "action_success_subset_observe_workspace",
        "action_failure_artifact_subset",
        "recovery_risk_round_trip",
        "asynchronous_cancellation",
        "failure_with_open_recovery_risk",
        "hook_protocol_error",
        "wrong_preamble",
        "oversized_frame",
        "invalid_utf8_payload",
        "malformed_json",
        "duplicate_property",
        "lone_high_surrogate",
        "candidate_malformed_unicode",
        "features_field_removed",
        "second_outstanding_request",
        "request_ack_id_mismatch",
        "resolve_risk_while_clear",
        "success_with_open_risk",
        "undeclared_action_output",
        "eof_before_completion",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(
        executed, expected,
        "Action vectors exercised by the production runtime"
    );

    // Part 2: the same machine over a real transport with real Hooks.
    let fixture = RuntimeFixture::new();
    let success = fixture.execute(
        "direct",
        "success",
        &fixture.marker("success"),
        policy(None, None, None),
    );
    assert_eq!(success.outcome, RunOutcome::Succeeded);
    assert!(success.completion_accepted && success.process_terminated);
    assert_eq!(
        success.hook_completion.as_ref().unwrap().status,
        HookCompletionStatus::Success
    );
    assert!(success.primary_failure.is_none());

    let failure = fixture.execute(
        "direct",
        "hook_failure",
        &fixture.marker("failure"),
        policy(None, None, None),
    );
    assert_eq!(failure.outcome, RunOutcome::Failed);
    assert!(
        failure.primary_failure.is_none(),
        "a Hook failure is the Hook's completion, not a Pactrun failure"
    );
    let completion = failure.hook_completion.as_ref().unwrap();
    assert_eq!(completion.status, HookCompletionStatus::Failure);
    assert_eq!(completion.code.as_ref().unwrap().as_str(), "service_failed");
    assert_eq!(
        completion.message.as_deref(),
        Some("diagnostic artifact retained")
    );

    let risk_marker = fixture.marker("risk");
    let admitted = fixture.admit("direct", "risk", &risk_marker);
    let run = admitted.run();
    fixture.application.execute_admitted_action(
        admitted,
        policy(None, None, None),
        ActionCancellation::default(),
    );
    let risk = take_facts(&fixture.application, run);
    assert_eq!(risk.outcome, RunOutcome::Succeeded);
    assert_eq!(
        running_risk(&fixture.storage, run),
        RecoveryRiskState::Clear
    );

    let open_marker = fixture.marker("open");
    let admitted = fixture.admit("direct", "risk_open_failure", &open_marker);
    let run = admitted.run();
    fixture.application.execute_admitted_action(
        admitted,
        policy(None, None, None),
        ActionCancellation::default(),
    );
    let open = take_facts(&fixture.application, run);
    assert_eq!(open.outcome, RunOutcome::Failed);
    assert_eq!(
        open.hook_completion.as_ref().unwrap().status,
        HookCompletionStatus::Failure
    );
    assert_eq!(
        running_risk(&fixture.storage, run),
        RecoveryRiskState::Open,
        "open risk stays durable for Slice 5"
    );

    let invalid = fixture.execute(
        "direct",
        "invalid_output",
        &fixture.marker("invalid"),
        policy(None, None, None),
    );
    assert_eq!(invalid.outcome, RunOutcome::Failed);
    assert_eq!(
        failure_ref(&invalid),
        Some((
            "hook_protocol_v1",
            "invalid_authority",
            RunFailedStep::Plan(ActionPlanStep::AcceptCompletion)
        ))
    );
    assert!(!invalid.completion_accepted && invalid.hook_completion.is_none());

    let reported = fixture.execute(
        "direct",
        "protocol_error",
        &fixture.marker("reported"),
        policy(None, None, None),
    );
    assert_eq!(reported.outcome, RunOutcome::Failed);
    assert_eq!(
        failure_ref(&reported),
        Some((
            "execution",
            "hook_reported_protocol_error",
            RunFailedStep::Plan(ActionPlanStep::AcceptCompletion)
        ))
    );
}

// Test-ID: PR-TEST-0094
// Verifies: PR-REQ-0046, PR-REQ-0169, PR-REQ-0210, PR-REQ-0211, PR-REQ-0212
#[test]
fn execution_materialization_uses_pinned_copies_and_live_owner_output_slots() {
    let fixture = RuntimeFixture::new();
    let marker = fixture.marker("materialized");
    let admitted = fixture.admit("direct", "output", &marker);
    let run = admitted.run();
    fixture.replace_binding();
    fixture.application.execute_admitted_action(
        admitted,
        policy(None, None, None),
        ActionCancellation::default(),
    );
    let facts = take_facts(&fixture.application, run);
    assert_eq!(facts.outcome, RunOutcome::Succeeded);

    // The Hook saw the pinned bytes through its read-only binding authority
    // even though the current binding was replaced after Admission.
    assert_eq!(
        fs::read_to_string(marker_variant(&marker, "binding")).unwrap(),
        "pinned"
    );
    let session: Value =
        serde_json::from_str(&fs::read_to_string(marker_variant(&marker, "session")).unwrap())
            .unwrap();
    assert_eq!(session["run_id"], run.to_string());
    assert_eq!(session["operation"]["kind"], "action");
    assert_eq!(session["operation"]["action_id"], "direct");
    assert_eq!(session["operation"]["access"], "observe");
    assert_eq!(session["io"]["terminal"], "none");
    let workspace = PathBuf::from(session["workspace"]["root_path"].as_str().unwrap());
    assert!(workspace.is_absolute() && workspace.is_dir());
    let names: Vec<&str> = session["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["parameter_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["expected_binding", "marker", "mode", "sensitive_value"],
        "parameters are a set sorted by id"
    );
    for handle in [
        &session["session_id"],
        &session["workspace"]["handle"],
        &session["operation"]["bindings"][0]["handle"],
        &session["operation"]["outputs"][0]["handle"],
    ] {
        let handle = handle.as_str().unwrap();
        assert!(
            handle.len() == 32
                && handle
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
    }

    // Output slots are live-owner-only files below the ephemeral execution
    // tree; nothing was published into Run Artifacts.
    assert_eq!(facts.outputs.len(), 1);
    assert_eq!(facts.outputs[0].output.as_str(), "report");
    assert_eq!(
        fs::read(&facts.outputs[0].path).unwrap(),
        b"unpublished-output"
    );
    let root = execution_root(&facts);
    assert!(root.file_name().unwrap().to_str().unwrap() == format!("execution-{run}"));
    assert!(
        root.parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("session-")
    );
    for child in ["runtime", "workspace", "bindings", "outputs"] {
        assert!(root.join(child).is_dir(), "{child}");
    }
    assert_eq!(
        fs::read(root.join("bindings/secret_config")).unwrap(),
        SECRET_BINDING
    );
    assert!(
        fs::metadata(root.join("bindings/secret_config"))
            .unwrap()
            .permissions()
            .readonly()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(root.join("bindings/secret_config"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o400
        );
        assert_eq!(
            fs::metadata(root.join("runtime/bin").join(worker_file_name()))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
    let database =
        rusqlite::Connection::open(fixture.storage.join("database/pactrun.sqlite3")).unwrap();
    let artifacts: i64 = database
        .query_row("SELECT COUNT(*) FROM run_artifacts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(artifacts, 0);

    // Materialization copies verified pinned bytes: corrupting one execution
    // tree cannot reach the store or a later execution.
    let materialized_worker = root.join("runtime/bin").join(worker_file_name());
    let mut permissions = fs::metadata(&materialized_worker).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&materialized_worker, permissions).unwrap();
    fs::write(&materialized_worker, b"ephemeral-corruption").unwrap();
    let second = fixture.execute(
        "direct",
        "success",
        &fixture.marker("copy-proof"),
        policy(None, None, None),
    );
    assert_eq!(second.outcome, RunOutcome::Succeeded);
}

// Test-ID: PR-TEST-0101
// Verifies: PR-REQ-0194
#[cfg(windows)]
#[test]
fn windows_launches_extensionless_images_and_never_substitutes_exe_or_cmd() {
    for shadow in [false, true] {
        let fixture = RuntimeFixture::with_source(|source, launcher, manifest| {
            fs::rename(source.join(worker_file_name()), source.join("worker")).unwrap();
            fs::rename(
                launcher.join(launcher_command()),
                launcher.join("pactrun-hook-test"),
            )
            .unwrap();
            *manifest = manifest
                .replace(worker_file_name(), "worker")
                .replace(launcher_command(), "pactrun-hook-test");
            if shadow {
                fs::write(source.join("shadow.bin"), b"unadmitted sibling").unwrap();
                fs::write(launcher.join(launcher_command()), b"unadmitted sibling").unwrap();
                manifest.push_str(
                    "    - { id: shadow, source: shadow.bin, path: bin/worker.exe, executable: true }\n",
                );
            }
        });
        for action in ["direct", "interpreted"] {
            let facts = fixture.execute(
                action,
                "success",
                &fixture.marker(action),
                policy(Some(10_000), None, None),
            );
            assert_eq!(
                facts.outcome,
                RunOutcome::Succeeded,
                "{action}, shadow={shadow}: {:?}",
                failure_ref(&facts)
            );
            assert!(facts.process_terminated && facts.completion_accepted);
        }
    }

    // A batch suffix does not disqualify a native image. Only the Executor's
    // no-shell guard rejects non-image batch files; Admission does not probe PE.
    let fixture = RuntimeFixture::with_source(|source, launcher, manifest| {
        fs::rename(source.join(worker_file_name()), source.join("worker.cmd")).unwrap();
        fs::rename(
            launcher.join(launcher_command()),
            launcher.join("launcher.bat"),
        )
        .unwrap();
        *manifest = manifest
            .replace(worker_file_name(), "worker.cmd")
            .replace(launcher_command(), "launcher.bat");
    });
    for action in ["direct", "interpreted"] {
        let facts = fixture.execute(
            action,
            "success",
            &fixture.marker(action),
            policy(Some(10_000), None, None),
        );
        assert_eq!(facts.outcome, RunOutcome::Succeeded);
    }

    // Refuse batch script content without executing the script body.
    for extension in ["cmd", "bat"] {
        let fixture = RuntimeFixture::with_source(|source, launcher, manifest| {
            *manifest = manifest.replace("broken.bin", &format!("hook.{extension}"));
            let script = format!(
                "@echo off\r\necho implicit-shell>\"{}\"\r\n",
                source.join("shell-ran").display()
            );
            fs::write(source.join(format!("hook.{extension}")), &script).unwrap();
            fs::write(launcher.join(format!("launcher.{extension}")), script).unwrap();
            *manifest = manifest.replace(launcher_command(), &format!("launcher.{extension}"));
        });
        for action in ["broken_launch", "interpreted"] {
            let facts = fixture.execute(
                action,
                "success",
                &fixture.marker(action),
                policy(Some(10_000), None, None),
            );
            assert!(
                !fixture.temporary.path().join("source/shell-ran").exists(),
                "batch candidate {action} .{extension} reached shell handling"
            );
            assert_eq!(
                failure_ref(&facts),
                Some((
                    "execution",
                    "launch_failed",
                    RunFailedStep::Plan(ActionPlanStep::LaunchHook)
                ))
            );
        }
        assert!(!fixture.temporary.path().join("source/shell-ran").exists());
    }
}

// Test-ID: PR-TEST-0095
// Verifies: PR-REQ-0167, PR-REQ-0194, PR-REQ-0280
#[test]
fn direct_and_interpreter_launches_use_exact_arguments_and_discovery_environment() {
    let fixture = RuntimeFixture::new();
    let transport_name = if cfg!(windows) {
        "windows-named-pipe"
    } else {
        "unix-domain-socket"
    };

    let direct = fixture.execute(
        "direct",
        "success",
        &fixture.marker("direct"),
        policy(None, None, None),
    );
    assert_eq!(direct.outcome, RunOutcome::Succeeded);
    let arguments = fs::read_to_string(fixture.marker("direct.args")).unwrap();
    assert_eq!(
        arguments.lines().collect::<Vec<_>>(),
        [
            "--exact",
            WORKER_TEST,
            "--nocapture",
            "--test-threads=1",
            ARGUMENT_TAIL
        ],
        "direct launch delivers exactly the declared argument tail"
    );
    let environment = fs::read_to_string(fixture.marker("direct.env")).unwrap();
    let mut lines = environment.lines();
    assert_eq!(lines.next(), Some(transport_name));
    let endpoint = lines.next().unwrap();
    #[cfg(windows)]
    assert!(endpoint.starts_with(r"\\.\pipe\pactrun-"));
    #[cfg(unix)]
    {
        assert!(Path::new(endpoint).is_absolute());
        assert!(
            !Path::new(endpoint).exists(),
            "listener is removed on the terminal path"
        );
    }

    let interpreted = fixture.execute(
        "interpreted",
        "success",
        &fixture.marker("interpreted"),
        policy(None, None, None),
    );
    assert_eq!(interpreted.outcome, RunOutcome::Succeeded);
    let script = execution_root(&interpreted).join("runtime/bin/hook-script.txt");
    let arguments = fs::read_to_string(fixture.marker("interpreted.args")).unwrap();
    assert_eq!(
        arguments.lines().collect::<Vec<_>>(),
        [
            "--exact",
            WORKER_TEST,
            "--nocapture",
            "--skip",
            script.to_str().unwrap(),
            "--test-threads=1",
            ARGUMENT_TAIL
        ],
        "interpreter launch delivers interpreter_args, the materialized script, then the Hook args"
    );

    // Launching the exact admitted path either succeeds or is reported as a
    // launch failure; there is no fallback search.
    let broken = fixture.execute(
        "broken_launch",
        "success",
        &fixture.marker("broken"),
        policy(None, None, None),
    );
    assert_eq!(broken.outcome, RunOutcome::Failed);
    assert_eq!(
        failure_ref(&broken),
        Some((
            "execution",
            "launch_failed",
            RunFailedStep::Plan(ActionPlanStep::LaunchHook)
        ))
    );
    assert!(broken.process_terminated && !broken.completion_accepted);
    assert_eq!(
        broken.outputs.len(),
        1,
        "preallocated slots stay with the live owner"
    );

    // PR-REQ-0280: both discovery variables are replaced in the exact child
    // environment and the endpoint is a fresh owner-private locator.
    let listener = ProtocolListener::bind().unwrap();
    let endpoint = listener.endpoint().to_owned();
    let mut command = Command::new(env::current_exe().unwrap());
    command.env(TRANSPORT_ENVIRONMENT, "ambient-transport");
    command.env(ENDPOINT_ENVIRONMENT, "ambient-endpoint");
    listener.inject_environment(&mut command);
    let environment: BTreeMap<_, _> = command
        .get_envs()
        .map(|(key, value)| (key.to_os_string(), value.map(|v| v.to_os_string())))
        .collect();
    assert_eq!(
        environment[std::ffi::OsStr::new(TRANSPORT_ENVIRONMENT)].as_deref(),
        Some(std::ffi::OsStr::new(transport_name))
    );
    assert_eq!(
        environment[std::ffi::OsStr::new(ENDPOINT_ENVIRONMENT)].as_deref(),
        Some(std::ffi::OsStr::new(&endpoint))
    );
    let other = ProtocolListener::bind().unwrap();
    assert_ne!(other.endpoint(), endpoint, "one listener per Session");
    drop(other);
    drop(listener);
    #[cfg(unix)]
    assert!(!Path::new(&endpoint).exists());
}

// Test-ID: PR-TEST-0096
// Verifies: PR-REQ-0055, PR-REQ-0056
#[test]
fn durable_risk_failure_retains_retry_continuation_and_never_acks_first() {
    let fixture = RuntimeFixture::new();
    let marker = fixture.marker("durable-retry");
    let admitted = fixture.admit("direct", "risk", &marker);
    let run = admitted.run();
    let failures = AtomicUsize::new(1);
    fixture
        .application
        .execute_admitted_action_with_risk_failures(
            admitted,
            policy(None, None, None),
            ActionCancellation::default(),
            &failures,
        );
    assert_eq!(
        failures.load(Ordering::Acquire),
        0,
        "the durable write was attempted"
    );
    wait_for(&marker_variant(&marker, "request"));
    thread::sleep(Duration::from_millis(50));
    assert!(
        !marker_variant(&marker, "ack").exists(),
        "no acknowledgment before durable publication"
    );
    assert_eq!(
        running_risk(&fixture.storage, run),
        RecoveryRiskState::Clear
    );
    {
        let guard = fixture.application.take_owner_continuation(run).unwrap();
        assert!(matches!(
            guard.continuation(),
            OwnerContinuation::RetryDurableOperation(_)
        ));
        assert!(
            fixture.application.take_owner_continuation(run).is_none(),
            "a taken continuation is exclusively held"
        );
    }
    assert!(
        fixture.application.take_owner_continuation(run).is_some(),
        "dropping the guard reinserts the continuation"
    );

    assert!(fixture.application.resume_owner_continuation(run));
    wait_for(&marker_variant(&marker, "ack"));
    let facts = take_facts(&fixture.application, run);
    assert_eq!(facts.outcome, RunOutcome::Succeeded);
    assert!(facts.completion_accepted && facts.process_terminated);
    assert_eq!(
        running_risk(&fixture.storage, run),
        RecoveryRiskState::Clear,
        "risk was durably cleared before its acknowledgment"
    );
    assert!(fixture.application.take_owner_continuation(run).is_none());
}

// Test-ID: PR-TEST-0097
// Verifies: PR-REQ-0052, PR-REQ-0216
#[test]
fn deadlines_cancellation_eof_and_process_loss_lock_outcomes_only_after_exit() {
    let fixture = RuntimeFixture::new();

    // Startup deadline before any connection: TimedOut after forced exit.
    let started = Instant::now();
    let facts = fixture.execute(
        "hang_before_connect",
        "success",
        &fixture.marker("startup"),
        policy(Some(200), None, Some(200)),
    );
    assert_eq!(facts.outcome, RunOutcome::TimedOut);
    assert!(started.elapsed() >= Duration::from_millis(400));
    assert!(
        facts.process_terminated && !facts.completion_accepted && facts.primary_failure.is_none()
    );

    // Action deadline with cooperative termination: cancel(reason=timeout)
    // is delivered, the late completion is protocol-accepted, and the
    // disposition stays TimedOut without waiting for the grace deadline.
    let marker = fixture.marker("action-deadline");
    let started = Instant::now();
    let facts = fixture.execute(
        "direct",
        "cancel_then_complete",
        &marker,
        policy(None, Some(300), Some(20_000)),
    );
    assert_eq!(facts.outcome, RunOutcome::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(20));
    assert_eq!(
        fs::read_to_string(marker_variant(&marker, "cancel")).unwrap(),
        "timeout"
    );
    assert!(facts.completion_accepted);
    assert_eq!(
        facts
            .hook_completion
            .as_ref()
            .unwrap()
            .code
            .as_ref()
            .unwrap()
            .as_str(),
        "cancelled"
    );
    assert!(marker_variant(&marker, "accepted").exists());

    // Requested cancellation with cooperative completion and no grace bound.
    let marker = fixture.marker("cancel-cooperative");
    let cancellation = ActionCancellation::default();
    let canceller = cancel_after(marker_variant(&marker, "ready"), &cancellation);
    let facts = fixture.execute_with(
        "direct",
        "cancel_then_complete",
        &marker,
        policy(None, None, None),
        cancellation,
    );
    canceller.join().unwrap();
    assert_eq!(facts.outcome, RunOutcome::Cancelled);
    assert_eq!(
        fs::read_to_string(marker_variant(&marker, "cancel")).unwrap(),
        "requested"
    );
    assert!(facts.completion_accepted && facts.process_terminated);
    assert_eq!(
        facts.hook_completion.as_ref().unwrap().status,
        HookCompletionStatus::Failure
    );

    // Requested cancellation that the Hook acknowledges but never honours:
    // forced termination after the grace deadline.
    let marker = fixture.marker("cancel-forced");
    let cancellation = ActionCancellation::default();
    let canceller = cancel_after(marker_variant(&marker, "ready"), &cancellation);
    let facts = fixture.execute_with(
        "direct",
        "hang_after_ready",
        &marker,
        policy(None, None, Some(200)),
        cancellation,
    );
    canceller.join().unwrap();
    assert_eq!(facts.outcome, RunOutcome::Cancelled);
    assert!(marker_variant(&marker, "cancel").exists());
    assert!(
        facts.process_terminated && !facts.completion_accepted && facts.hook_completion.is_none()
    );

    // Direct EOF after readiness is the Frozen unexpected_message fault.
    let facts = fixture.execute(
        "direct",
        "eof_after_ready",
        &fixture.marker("eof"),
        policy(None, None, None),
    );
    assert_eq!(facts.outcome, RunOutcome::Failed);
    assert_eq!(
        failure_ref(&facts),
        Some((
            "hook_protocol_v1",
            "unexpected_message",
            RunFailedStep::Plan(ActionPlanStep::AcceptCompletion)
        ))
    );

    // Child loss after readiness observes the same protocol boundary.
    let facts = fixture.execute(
        "direct",
        "crash_after_ready",
        &fixture.marker("crash"),
        policy(None, None, None),
    );
    assert_eq!(facts.outcome, RunOutcome::Failed);
    assert_eq!(
        failure_ref(&facts),
        Some((
            "hook_protocol_v1",
            "unexpected_message",
            RunFailedStep::Plan(ActionPlanStep::AcceptCompletion)
        ))
    );

    // Child exit before any connection is a transport failure, never
    // TimedOut and never Interrupted.
    let facts = fixture.execute(
        "exit_before_connect",
        "success",
        &fixture.marker("exit"),
        policy(Some(20_000), None, None),
    );
    assert_eq!(facts.outcome, RunOutcome::Failed);
    assert_eq!(
        failure_ref(&facts),
        Some((
            "execution",
            "protocol_transport_failed",
            RunFailedStep::Plan(ActionPlanStep::AcceptCompletion)
        ))
    );

    // An accepted completion is terminalization-ready only after exit.
    let marker = fixture.marker("linger");
    let started = Instant::now();
    let facts = fixture.execute(
        "direct",
        "complete_then_linger",
        &marker,
        policy(None, None, None),
    );
    assert_eq!(facts.outcome, RunOutcome::Succeeded);
    assert!(started.elapsed() >= LINGER);
    assert!(marker_variant(&marker, "exiting").exists());

    // A cancellation after the outcome is locked cannot change it, but the
    // live owner still forces a lingering tree to exit after the grace.
    let marker = fixture.marker("linger-cancel");
    let cancellation = ActionCancellation::default();
    let canceller = cancel_after(marker_variant(&marker, "lingering"), &cancellation);
    let facts = fixture.execute_with(
        "direct",
        "complete_then_linger",
        &marker,
        policy(None, None, Some(50)),
        cancellation,
    );
    let finished = Instant::now();
    let requested = canceller.join().unwrap();
    assert_eq!(facts.outcome, RunOutcome::Succeeded);
    assert!(facts.completion_accepted && facts.process_terminated);
    assert!(
        finished.duration_since(requested) < LINGER,
        "forced termination follows the grace deadline"
    );
    assert!(!marker_variant(&marker, "exiting").exists());
}

// Test-ID: PR-TEST-0098
// Verifies: PR-REQ-0172, PR-REQ-0280
#[test]
fn terminal_modes_keep_the_protocol_off_stdio() {
    let fixture = RuntimeFixture::new();
    for (action, terminal) in [
        ("direct", "none"),
        ("output_terminal", "output"),
        ("interactive_terminal", "interactive"),
    ] {
        let marker = fixture.marker(action);
        let facts = fixture.execute(action, "terminal_noise", &marker, policy(None, None, None));
        assert_eq!(facts.outcome, RunOutcome::Succeeded, "{action}");
        assert!(facts.completion_accepted);
        let session: Value =
            serde_json::from_str(&fs::read_to_string(marker_variant(&marker, "session")).unwrap())
                .unwrap();
        assert_eq!(session["io"]["terminal"], terminal);
        if terminal == "interactive" {
            assert!(!marker_variant(&marker, "stdin").exists());
        } else {
            assert_eq!(
                fs::read_to_string(marker_variant(&marker, "stdin")).unwrap(),
                "eof",
                "{terminal}: stdin is the null device"
            );
        }
    }
}

// Supporting runtime non-retention coverage for the production finalization
// boundary is provided by PR-TEST-0113 below.
// Test-ID: PR-TEST-0099
// Verifies: PR-REQ-0098, PR-REQ-0280
#[test]
fn sensitive_values_endpoints_and_raw_frames_are_not_retained() {
    let fixture = RuntimeFixture::new();
    let marker = fixture.marker("sensitive");
    let facts = fixture.execute(
        "direct",
        "sensitive_diagnostic",
        &marker,
        policy(None, None, None),
    );
    assert_eq!(facts.outcome, RunOutcome::Succeeded);
    let environment = fs::read_to_string(marker_variant(&marker, "env")).unwrap();
    let endpoint = environment.lines().nth(1).unwrap().to_owned();
    let session_id = {
        let session: Value =
            serde_json::from_str(&fs::read_to_string(marker_variant(&marker, "session")).unwrap())
                .unwrap();
        session["session_id"].as_str().unwrap().to_owned()
    };

    let retry_marker = fixture.marker("sensitive-retry");
    let admitted = fixture.admit("direct", "risk", &retry_marker);
    let run = admitted.run();
    let failures = AtomicUsize::new(1);
    fixture
        .application
        .execute_admitted_action_with_risk_failures(
            admitted,
            policy(None, None, None),
            ActionCancellation::default(),
            &failures,
        );
    let retained = {
        let guard = fixture.application.take_owner_continuation(run).unwrap();
        format!("{:?}", guard.continuation())
    };
    assert!(fixture.application.resume_owner_continuation(run));
    let retry_facts = take_facts(&fixture.application, run);
    assert_eq!(retry_facts.outcome, RunOutcome::Succeeded);
    let retry_endpoint = fs::read_to_string(marker_variant(&retry_marker, "env"))
        .unwrap()
        .lines()
        .nth(1)
        .unwrap()
        .to_owned();

    let secret = String::from_utf8(SECRET_BINDING.to_vec()).unwrap();
    for (label, text) in [
        ("terminal facts", format!("{facts:?}")),
        ("retained continuation", retained),
        ("retry facts", format!("{retry_facts:?}")),
    ] {
        for needle in [
            secret.as_str(),
            SENSITIVE_PARAMETER,
            endpoint.as_str(),
            retry_endpoint.as_str(),
            "leak_attempt",
            "unpublished-output",
        ] {
            assert!(!text.contains(needle), "{label} retains {needle:?}: {text}");
        }
        assert!(
            !text.contains(&format!("execution-{run}")),
            "{label} exposes the execution path"
        );
    }

    let database = database_bytes(&fixture.storage);
    for needle in [
        SENSITIVE_PARAMETER.as_bytes(),
        endpoint.as_bytes(),
        retry_endpoint.as_bytes(),
        session_id.as_bytes(),
        b"leak_attempt".as_slice(),
        PREAMBLE,
    ] {
        assert!(
            !contains(&database, needle),
            "durable storage retains {:?}",
            String::from_utf8_lossy(needle)
        );
    }
}

// Test-ID: PR-TEST-0113
// Verifies: PR-REQ-0283
#[test]
fn production_finalization_drops_hook_text_but_keeps_structural_status() {
    const CODE_MARKER: &str = "hook_completion_marker";
    const COMPLETION_MESSAGE_MARKER: &str = "slice5_hook_completion_marker";
    const DIAGNOSTIC_MARKER: &str = "slice5_hook_diagnostic_marker";

    let fixture = RuntimeFixture::new();
    let marker = fixture.marker("finalization-text-marker");
    let admitted = fixture.admit("direct", "finalization_text_marker", &marker);
    let run = admitted.run();
    fixture.application.execute_admitted_action(
        admitted,
        policy(None, None, None),
        ActionCancellation::default(),
    );
    assert!(fixture.application.advance_owner_continuation(run).unwrap());

    let storage = fixture.storage.clone();
    drop(fixture.application);
    let reopened = persistence(&storage);
    let view = reopened.load_run(run).unwrap().unwrap();
    let RunState::Finished(outcome) = &view.state else {
        panic!("expected a Finished Run");
    };
    let completion = outcome
        .hook_completion
        .as_ref()
        .expect("production Run retains Hook completion status");
    assert_eq!(completion.status, HookCompletionStatus::Failure);
    assert!(completion.code.is_none());
    assert!(completion.message.is_none());

    let debug = format!("{view:?}");
    for marker in [CODE_MARKER, COMPLETION_MESSAGE_MARKER, DIAGNOSTIC_MARKER] {
        assert!(!debug.contains(marker), "Run Debug output retains {marker}");
    }
    let outcome_debug = format!("{outcome:?}");
    for marker in [CODE_MARKER, COMPLETION_MESSAGE_MARKER, DIAGNOSTIC_MARKER] {
        assert!(
            !outcome_debug.contains(marker),
            "Run diagnostics retain {marker}"
        );
    }

    let database = rusqlite::Connection::open(storage.join("database/pactrun.sqlite3")).unwrap();
    let (status, code_present, code, message_present, message): (i64, i64, Vec<u8>, i64, Vec<u8>) =
        database
            .query_row(
                "SELECT status_rank, code_present, code_utf8, message_present, message_utf8 \
                 FROM run_hook_completions WHERE run_id=?1",
                [run.as_bytes().as_slice()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
    assert_eq!(status, HookCompletionStatus::Failure.rank());
    assert_eq!(code_present, 0);
    assert!(code.is_empty());
    assert_eq!(message_present, 0);
    assert!(message.is_empty());

    let durable = database_bytes(&storage);
    for marker in [CODE_MARKER, COMPLETION_MESSAGE_MARKER, DIAGNOSTIC_MARKER] {
        assert!(
            !contains(&durable, marker.as_bytes()),
            "durable Run persistence retains {marker}"
        );
    }
}

// Test-ID: PR-TEST-0100
// Verifies: PR-REQ-0047, PR-REQ-0060
#[test]
fn confirmed_owner_loss_cleanup_removes_execution_bytes_but_not_durable_run_state() {
    let fixture = RuntimeFixture::new();
    let marker = fixture.marker("owner-loss");
    let admitted = fixture.admit("direct", "risk_open_failure", &marker);
    let run = admitted.run();
    fixture.replace_binding();
    fixture.application.execute_admitted_action(
        admitted,
        policy(None, None, None),
        ActionCancellation::default(),
    );
    let facts = take_facts(&fixture.application, run);
    assert_eq!(facts.outcome, RunOutcome::Failed);
    assert_eq!(running_risk(&fixture.storage, run), RecoveryRiskState::Open);
    let root = execution_root(&facts);
    assert!(root.is_dir());
    let session_directory = root.parent().unwrap().to_path_buf();
    let owner = fixture.application.execution_owner();
    assert!(session_is_live(&fixture.storage, &owner).unwrap());

    let RuntimeFixture {
        temporary: _temporary,
        storage,
        application,
        ..
    } = fixture;
    application.abandon_execution_owner();
    assert!(
        !session_is_live(&storage, &owner).unwrap(),
        "the lease is released on owner loss"
    );
    assert!(
        root.is_dir(),
        "execution bytes may outlive the owner until cleanup runs"
    );

    let count = |sql: &str| -> i64 {
        rusqlite::Connection::open(storage.join("database/pactrun.sqlite3"))
            .unwrap()
            .query_row(sql, [run.as_bytes().as_slice()], |row| row.get(0))
            .unwrap()
    };
    let revision_pins = count("SELECT COUNT(*) FROM run_revision_pins WHERE run_id=?1");
    let payload_pins = count("SELECT COUNT(*) FROM run_payload_pins WHERE run_id=?1");
    assert!(revision_pins == 1 && payload_pins == 1);

    // A later owner's stale-session scan removes the whole abandoned tree.
    let reopened = PactrunApplication::open(&storage).unwrap();
    assert!(
        !root.exists(),
        "execution tree is ephemeral after confirmed owner loss"
    );
    assert!(!session_directory.exists());
    assert!(
        reopened.take_owner_continuation(run).is_none(),
        "continuations are volatile and owner-scoped"
    );

    // Durable Run, ownership record, risk, and pins are untouched: only
    // Slice 5 reconciliation may change them.
    match load_run(&storage, run).state {
        RunState::Running(execution) => {
            assert_eq!(execution.owner, owner);
            assert_eq!(execution.boundary, ActionRunBoundary::Admitted);
            assert_eq!(execution.risk_state, RecoveryRiskState::Open);
        }
        other => panic!("expected Running Run, found {other:?}"),
    }
    assert_eq!(
        count("SELECT COUNT(*) FROM run_revision_pins WHERE run_id=?1"),
        revision_pins
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM run_payload_pins WHERE run_id=?1"),
        payload_pins
    );
    let mut pinned = Vec::new();
    persistence(&storage)
        .stream_admitted_payload(
            run,
            &InputIdentity::parse("secret_config").unwrap(),
            &mut pinned,
        )
        .unwrap();
    assert_eq!(
        pinned, SECRET_BINDING,
        "pins outlive the owner and ignore the replaced current binding"
    );

    // Once lease loss is confirmed, reconciliation is the only operation that
    // changes the durable Running record: it records Interrupted and releases
    // the admitted pins in the same terminal transaction.
    let reconciled = reopened.reconcile_lost_action_owners().unwrap();
    assert_eq!(reconciled, vec![run]);
    match load_run(&storage, run).state {
        RunState::Finished(outcome) => {
            assert_eq!(outcome.outcome, RunOutcome::Interrupted);
            assert_eq!(outcome.boundary, ActionRunBoundary::Admitted);
            assert_eq!(outcome.terminal_risk, RecoveryRiskState::Open);
            assert!(outcome.primary_failure.is_none());
            assert!(outcome.hook_completion.is_none());
        }
        other => panic!("expected reconciled Finished Run, found {other:?}"),
    }
    assert_eq!(
        count("SELECT COUNT(*) FROM run_revision_pins WHERE run_id=?1"),
        0
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM run_payload_pins WHERE run_id=?1"),
        0
    );
    let instance = load_run(&storage, run).instance;
    let guard = persistence(&storage)
        .load_instance_recovery_guard(instance)
        .unwrap()
        .unwrap();
    assert_eq!(guard.run, run);
}

// Test-ID: PR-TEST-0106
// Verifies: PR-REQ-0073, PR-REQ-0144, PR-REQ-0281
#[test]
fn accepted_output_subset_is_published_only_after_cleanup() {
    let fixture = RuntimeFixture::new();
    let marker = fixture.marker("output-subset");
    let admitted = fixture.admit("output_subset", "output_subset", &marker);
    let run = admitted.run();
    fixture.application.execute_admitted_action(
        admitted,
        policy(None, None, None),
        ActionCancellation::default(),
    );
    let session: Value =
        serde_json::from_str(&fs::read_to_string(marker_variant(&marker, "session")).unwrap())
            .unwrap();
    let workspace = PathBuf::from(session["workspace"]["root_path"].as_str().unwrap());
    let execution_root = workspace.parent().unwrap().to_path_buf();
    assert!(execution_root.is_dir());

    assert!(fixture.application.advance_owner_continuation(run).unwrap());
    let view = load_run(&fixture.storage, run);
    let RunState::Finished(outcome) = view.state else {
        panic!("expected a Finished Run");
    };
    assert_eq!(outcome.outcome, RunOutcome::Succeeded);
    assert_eq!(outcome.artifacts.len(), 1);
    assert_eq!(outcome.artifacts[0].output.as_str(), "report");
    assert!(
        !execution_root.exists(),
        "cleanup precedes terminal publication"
    );

    let mut bytes = Vec::new();
    persistence(&fixture.storage)
        .open_run_artifact(
            run,
            &ManagedOutputIdentity::parse("report").unwrap(),
            &mut bytes,
        )
        .unwrap();
    assert_eq!(bytes, b"unpublished-output");
    assert!(
        outcome
            .artifacts
            .iter()
            .all(|artifact| artifact.output.as_str() != "secondary")
    );
}

// Test-ID: PR-TEST-0109
// Verifies: PR-REQ-0144, PR-REQ-0281
#[test]
fn late_completion_keeps_timeout_outcome_but_publishes_eligible_output() {
    let fixture = RuntimeFixture::new();
    let marker = fixture.marker("late-output");
    let admitted = fixture.admit("direct", "cancel_then_output", &marker);
    let run = admitted.run();
    fixture.application.execute_admitted_action(
        admitted,
        policy(None, Some(300), Some(20_000)),
        ActionCancellation::default(),
    );
    let session: Value =
        serde_json::from_str(&fs::read_to_string(marker_variant(&marker, "session")).unwrap())
            .unwrap();
    let workspace = PathBuf::from(session["workspace"]["root_path"].as_str().unwrap());
    assert!(fixture.application.advance_owner_continuation(run).unwrap());
    let RunState::Finished(outcome) = load_run(&fixture.storage, run).state else {
        panic!("expected a Finished Run");
    };
    assert_eq!(outcome.outcome, RunOutcome::TimedOut);
    assert_eq!(outcome.artifacts.len(), 1);
    assert_eq!(outcome.artifacts[0].output.as_str(), "report");
    let mut bytes = Vec::new();
    persistence(&fixture.storage)
        .open_run_artifact(
            run,
            &ManagedOutputIdentity::parse("report").unwrap(),
            &mut bytes,
        )
        .unwrap();
    assert_eq!(bytes, b"late-output");
    assert!(!workspace.parent().unwrap().exists());
}

// Test-ID: PR-TEST-0107
// Verifies: PR-REQ-0051, PR-REQ-0144, PR-REQ-0281
#[test]
fn output_publication_failure_is_primary_when_no_earlier_failure_exists() {
    let fixture = RuntimeFixture::new();
    let marker = fixture.marker("output-publication-failure");
    let admitted = fixture.admit("direct", "output", &marker);
    let run = admitted.run();
    fixture.application.execute_admitted_action(
        admitted,
        policy(None, None, None),
        ActionCancellation::default(),
    );
    let session: Value =
        serde_json::from_str(&fs::read_to_string(marker_variant(&marker, "session")).unwrap())
            .unwrap();
    let workspace = PathBuf::from(session["workspace"]["root_path"].as_str().unwrap());
    fs::remove_file(workspace.parent().unwrap().join("outputs").join("report")).unwrap();

    assert!(fixture.application.advance_owner_continuation(run).unwrap());
    let RunState::Finished(outcome) = load_run(&fixture.storage, run).state else {
        panic!("expected a Finished Run");
    };
    assert_eq!(outcome.outcome, RunOutcome::Failed);
    let primary = outcome.primary_failure.unwrap();
    assert_eq!(primary.failure.error.owner(), "execution");
    assert_eq!(
        primary.failure.error.code(),
        "managed_output_publication_failed"
    );
    assert_eq!(
        primary.step,
        RunFailedStep::Plan(ActionPlanStep::PublishDeclaredOutputs),
        "publication is the primary causal failure"
    );
    assert!(outcome.secondary_failures.is_empty());
    assert!(outcome.artifacts.is_empty());
}

// Test-ID: PR-TEST-0110
// Verifies: PR-REQ-0051, PR-REQ-0073, PR-REQ-0282
#[test]
fn cleanup_failure_leaves_residue_but_does_not_block_terminal_publication() {
    let fixture = RuntimeFixture::new();
    let marker = fixture.marker("cleanup-failure");
    let admitted = fixture.admit("direct", "hook_failure", &marker);
    let run = admitted.run();
    fixture.application.execute_admitted_action(
        admitted,
        policy(None, None, None),
        ActionCancellation::default(),
    );
    let session: Value =
        serde_json::from_str(&fs::read_to_string(marker_variant(&marker, "session")).unwrap())
            .unwrap();
    let workspace = PathBuf::from(session["workspace"]["root_path"].as_str().unwrap());
    let execution_root = workspace.parent().unwrap().to_path_buf();
    fail_next_execution_cleanup();

    assert!(fixture.application.advance_owner_continuation(run).unwrap());
    let RunState::Finished(outcome) = load_run(&fixture.storage, run).state else {
        panic!("expected a Finished Run");
    };
    assert_eq!(outcome.outcome, RunOutcome::Failed);
    assert!(outcome.primary_failure.is_none());
    assert_eq!(
        outcome.hook_completion.as_ref().unwrap().status,
        HookCompletionStatus::Failure
    );
    assert_eq!(outcome.secondary_failures.len(), 1);
    assert_eq!(
        outcome.secondary_failures[0].error.code(),
        "workspace_cleanup_failed"
    );
    assert!(
        execution_root.is_dir(),
        "cleanup failure leaves non-authoritative residue"
    );
    assert!(
        execution_root.join("outputs/report").is_file(),
        "cleanup residue remains non-authoritative"
    );
    assert!(outcome.artifacts.is_empty());
}

// Test-ID: PR-TEST-0117
// Verifies: PR-REQ-0286
#[test]
fn cancellation_before_hook_launch_creates_no_child_process() {
    let fixture = RuntimeFixture::new();
    let marker = fixture.marker("cancel-before-launch");
    let cancellation = ActionCancellation::default();
    cancellation.request();

    let facts = fixture.execute_with(
        "direct",
        "success",
        &marker,
        policy(None, None, None),
        cancellation,
    );
    assert_eq!(facts.outcome, RunOutcome::Cancelled);
    assert!(!facts.process_started && !facts.process_terminated);
    assert!(!marker.exists(), "cancellation launched the Hook child");
}
