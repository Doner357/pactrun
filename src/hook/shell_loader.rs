//! Script-side protocol adapter. Core alone owns authority and publication.

mod helper;
mod session;

use super::{
    platform::{ProtocolListener, ProtocolStream},
    protocol,
};
use crate::{domain::ShellKind, strict_json};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    env,
    ffi::OsString,
    fs,
    io::{self, Read, Write},
    path::Path,
    process::{Child, Command, ExitStatus},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

const HELPER_ENDPOINT: &str = "PACTRUN_SHELL_HELPER_ENDPOINT";
const EXECUTABLE: &str = "PACTRUN_EXECUTABLE";
const LIMIT: usize = protocol::MAX_PAYLOAD;
// Private request/result bookkeeping must not consume the canonical payload's
// allowance. This does not enlarge any Core Hook Protocol frame.
const HELPER_FRAME_LIMIT: usize = LIMIT + 1024;

fn failure() -> io::Error {
    io::Error::other("shell adapter operation failed")
}

fn read_frame(stream: &mut impl Read) -> io::Result<Value> {
    read_limited_frame(stream, LIMIT)
}

fn read_helper_frame(stream: &mut impl Read) -> io::Result<Value> {
    read_limited_frame(stream, HELPER_FRAME_LIMIT)
}

fn read_limited_frame(stream: &mut impl Read, limit: usize) -> io::Result<Value> {
    let mut size = [0; 4];
    stream.read_exact(&mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    if size > limit {
        return Err(failure());
    }
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    let strict_json::RawJsonValue::Object(fields) =
        strict_json::parse_json(&bytes, limit).map_err(|_| failure())?
    else {
        return Err(failure());
    };
    // Preserve the raw-token integer rule before serde_json can round a
    // fractional token such as 1.000000000000000001 into binary64 1.0.
    for (key, value) in fields {
        if matches!(
            key.as_str(),
            "protocol_version" | "request_id" | "control_id"
        ) && crate::revision_core_v1::exact_integer(value).map_err(|_| failure())? <= 0
        {
            return Err(failure());
        }
    }
    serde_json::from_slice(&bytes).map_err(|_| failure())
}

fn write_frame(stream: &mut impl Write, value: &Value) -> io::Result<()> {
    write_limited_frame(stream, value, LIMIT)
}

fn write_helper_frame(stream: &mut impl Write, value: &Value) -> io::Result<()> {
    write_limited_frame(stream, value, HELPER_FRAME_LIMIT)
}

fn write_limited_frame(stream: &mut impl Write, value: &Value, limit: usize) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(|_| failure())?;
    if bytes.len() > limit {
        return Err(failure());
    }
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(&bytes)?;
    stream.flush()
}

fn connect(endpoint: &str) -> io::Result<ProtocolStream> {
    #[cfg(unix)]
    {
        std::os::unix::net::UnixStream::connect(endpoint)
    }
    #[cfg(windows)]
    {
        pactrun_windows_ntfs::NamedPipeStream::connect(endpoint)
    }
}

enum Event {
    Core(io::Result<Value>),
    Helper(Request, Sender<Value>),
    HelperDone(bool),
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Session,
    Parameter {
        id: String,
    },
    Workspace,
    Input {
        id: String,
        view: String,
        role: String,
    },
    Resource {
        handle: String,
    },
    Output {
        id: String,
    },
    OutputRegister {
        id: String,
    },
    Candidate,
    SnapshotContent,
    Diagnostic {
        value: Value,
    },
    ProtocolError {
        value: Value,
    },
    Risk {
        enter: bool,
    },
    CaptureRegister {
        value: Value,
    },
    Completion {
        value: Value,
    },
    TargetReady,
}

enum Pending {
    Risk {
        request_id: u64,
        open: bool,
        reply: Sender<Value>,
    },
    Target {
        reply: Sender<Value>,
    },
}

struct State {
    session: Value,
    produced: BTreeSet<String>,
    content: Vec<Value>,
    completion: Value,
    open: bool,
    request_id: u64,
    pending: Option<Pending>,
    target_received: bool,
}

impl State {
    fn new(session: Value) -> Self {
        Self {
            session,
            produced: BTreeSet::new(),
            content: vec![],
            completion: json!({}),
            open: false,
            request_id: 0,
            pending: None,
            target_received: false,
        }
    }

    fn complete(&self, success: bool) -> Value {
        let kind = self.session["operation"]["kind"]
            .as_str()
            .expect("validated operation");
        let mut value = json!({"type":"complete", "operation":kind,
            "status":if success && self.completion["status"] != "failure" { "success" } else { "failure" }});
        for key in ["code", "message"] {
            if let Some(text) = self.completion.get(key) {
                value[key] = text.clone();
            }
        }
        match kind {
            "action" => value["produced_outputs"] = json!(self.produced),
            "migration" => {
                value["produced_target_outputs"] = if value["status"] == "success" {
                    json!(self.produced)
                } else {
                    json!([])
                }
            }
            "snapshot_capture" => {
                value["service_content"] = if value["status"] == "success" {
                    json!(self.content)
                } else {
                    json!([])
                }
            }
            _ => {}
        }
        value
    }

    fn validate_message(&self, value: &Value) -> bool {
        let Ok(bytes) = serde_json::to_vec(value) else {
            return false;
        };
        let operation = match self.session["operation"]["kind"].as_str() {
            Some("action") => protocol::SessionOperation::Action,
            Some("migration") => protocol::SessionOperation::Migration,
            Some("snapshot_capture") => protocol::SessionOperation::Capture,
            Some("snapshot_restore") => protocol::SessionOperation::Restore,
            Some("cleanup") => protocol::SessionOperation::Cleanup,
            _ => return false,
        };
        protocol::parse_hook_message_for(&bytes, operation).is_ok()
    }

    fn request(
        &mut self,
        request: Request,
        reply: Sender<Value>,
        wire: &mut ProtocolStream,
    ) -> io::Result<()> {
        if self.target_received || self.pending.is_some() {
            return reply.send(json!({"ok":false})).map_err(|_| failure());
        }
        let result = match request {
            Request::Session => Some(self.session.clone()),
            Request::Workspace => self.session["workspace"].get("root_path").cloned(),
            Request::Parameter { id } => self.session["parameters"]
                .as_array()
                .and_then(|items| items.iter().find(|v| v["parameter_id"] == id))
                .and_then(|v| v.get("value"))
                .cloned(),
            Request::Input { id, view, role } => session::input(&self.session, &id, &view, &role),
            Request::Resource { handle } => self.session["service_authorities"]
                .as_array()
                .and_then(|items| items.iter().find(|v| v["handle"] == handle))
                .and_then(|v| v.get("path"))
                .cloned(),
            Request::Output { id } => session::output(&self.session, &id)
                .and_then(|v| v.get("staged_path"))
                .cloned(),
            Request::OutputRegister { id } => session::output(&self.session, &id)
                .and_then(|v| v["handle"].as_str())
                .map(str::to_owned)
                .filter(|handle| self.produced.insert(handle.clone()))
                .map(|_| Value::Null),
            Request::Candidate => self.session["operation"]["candidate"]
                .get("root_path")
                .cloned(),
            Request::SnapshotContent => self.session["operation"].get("snapshot_content").cloned(),
            Request::Diagnostic { value } => {
                if value["type"] == "diagnostic" && self.validate_message(&value) {
                    write_frame(wire, &value)?;
                    Some(Value::Null)
                } else {
                    None
                }
            }
            Request::ProtocolError { value } => {
                if value["type"] == "protocol_error" && self.validate_message(&value) {
                    write_frame(wire, &value)?;
                    return Err(failure());
                } else {
                    None
                }
            }
            Request::Completion { value } => {
                if self.completion["status"] == "failure" && value["status"] != "failure" {
                    return reply.send(json!({"ok":false})).map_err(|_| failure());
                }
                let old = std::mem::replace(&mut self.completion, value);
                let valid = self.completion.as_object().is_some_and(|o| {
                    o.keys()
                        .all(|k| matches!(k.as_str(), "status" | "code" | "message"))
                }) && self.completion.get("status").is_none_or(|v| v == "failure")
                    && self.validate_message(&self.complete(false));
                if valid {
                    Some(Value::Null)
                } else {
                    self.completion = old;
                    None
                }
            }
            Request::CaptureRegister { value } => {
                if self.session["operation"]["kind"] != "snapshot_capture" {
                    None
                } else {
                    self.content.push(value);
                    let mut candidate = self.complete(true);
                    candidate["status"] = json!("success");
                    candidate["service_content"] = json!(self.content);
                    if self.validate_message(&candidate) {
                        Some(Value::Null)
                    } else {
                        self.content.pop();
                        None
                    }
                }
            }
            Request::Risk { enter } => {
                if self.open == enter || (!enter && self.session.get("target_commit").is_some()) {
                    None
                } else {
                    self.request_id = self
                        .request_id
                        .checked_add(1)
                        .filter(|v| *v <= 9_007_199_254_740_991)
                        .ok_or_else(failure)?;
                    write_frame(
                        wire,
                        &json!({"type":"request", "request_id":self.request_id,
                        "request":{"kind":if enter {"enter_recovery_risk"} else {"resolve_recovery_risk"}}}),
                    )?;
                    self.pending = Some(Pending::Risk {
                        request_id: self.request_id,
                        open: enter,
                        reply,
                    });
                    return Ok(());
                }
            }
            Request::TargetReady => {
                if !self.open
                    || self.session.get("target_commit").is_none()
                    || self.completion["status"] == "failure"
                {
                    None
                } else {
                    let expected = session::outputs(&self.session)
                        .iter()
                        .filter_map(|v| v["handle"].as_str())
                        .collect::<BTreeSet<_>>();
                    if expected != self.produced.iter().map(String::as_str).collect() {
                        None
                    } else {
                        write_frame(
                            wire,
                            &json!({"type":"target_ready", "commit_handle":self.session["target_commit"]["handle"],
                            "produced_target_outputs":self.produced}),
                        )?;
                        self.pending = Some(Pending::Target { reply });
                        return Ok(());
                    }
                }
            }
        };
        reply
            .send(match result {
                Some(value) => json!({"ok":true,"value":value}),
                None => json!({"ok":false}),
            })
            .map_err(|_| failure())
    }

    fn acknowledgment(&mut self, value: Value) -> io::Result<()> {
        let pending = self.pending.take().ok_or_else(failure)?;
        match pending {
            Pending::Risk {
                request_id,
                open,
                reply,
            } => {
                session::risk_ack(&value, request_id, open)?;
                self.open = open;
                reply
                    .send(json!({"ok":true,"value":value["risk_state"]}))
                    .map_err(|_| failure())
            }
            Pending::Target { reply } => {
                if value
                    != json!({"type":"target_ready_received","commit_handle":self.session["target_commit"]["handle"]})
                {
                    return Err(failure());
                }
                self.target_received = true;
                reply
                    .send(json!({"ok":true,"value":Value::Null}))
                    .map_err(|_| failure())
            }
        }
    }
}

fn serve_helper(mut stream: ProtocolStream, events: Sender<Event>) {
    thread::spawn(move || {
        let result = (|| {
            let value = read_helper_frame(&mut stream)?;
            let request = serde_json::from_value(value).map_err(|_| failure())?;
            let (send, recv) = mpsc::channel();
            events
                .send(Event::Helper(request, send))
                .map_err(|_| failure())?;
            write_helper_frame(&mut stream, &recv.recv().map_err(|_| failure())?)?;
            if read_helper_frame(&mut stream)? != json!({"received":true}) {
                return Err(failure());
            }
            Ok::<_, io::Error>(())
        })();
        let _ = events.send(Event::HelperDone(result.is_ok()));
    });
}

fn script_command(args: &[OsString], listener: &ProtocolListener) -> io::Result<Command> {
    if args.len() < 4 {
        return Err(failure());
    }
    let shell = args[0]
        .to_str()
        .and_then(ShellKind::parse)
        .filter(|v| v.supported_on_host())
        .ok_or_else(failure)?;
    if !Path::new(&args[1]).is_absolute() || !Path::new(&args[2]).is_absolute() {
        return Err(failure());
    }
    let mut command = Command::new(&args[1]);
    match shell {
        ShellKind::Sh => {
            command.arg(&args[2]);
        }
        ShellKind::Bash => {
            command.args(["--noprofile", "--norc", "--"]).arg(&args[2]);
        }
        ShellKind::Powershell7 | ShellKind::WindowsPowershell51 => {
            command
                .args(["-NoLogo", "-NoProfile", "-File"])
                .arg(powershell_file_path(&args[2])?);
        }
    }
    command.args(&args[4..]);
    command.env(HELPER_ENDPOINT, listener.endpoint());
    command.env(EXECUTABLE, env::current_exe()?);
    command.env_remove("PACTRUN_HOOK_PROTOCOL_ENDPOINT");
    command.env_remove("PACTRUN_HOOK_PROTOCOL_TRANSPORT");
    command.env_remove("PACTRUN_INTERNAL_SHELL_HELPER_DIRECTORY");
    command.env_remove(super::startup::STATUS_ENV);
    Ok(command)
}

fn powershell_file_path(path: &OsString) -> io::Result<OsString> {
    // PowerShell's authorization manager rejects the Win32 extended prefix.
    // Only translate drive-absolute paths whose components have identical DOS
    // and extended semantics; do not normalize arbitrary user path spellings.
    let Some(text) = path.to_str() else {
        return Err(failure());
    };
    if let Some(tail) = text.strip_prefix(r"\\?\") {
        if tail.len() < 3
            || !tail.as_bytes()[0].is_ascii_alphabetic()
            || tail.as_bytes().get(1..3) != Some(b":\\")
            || tail[3..]
                .split('\\')
                .any(|s| s.is_empty() || s.ends_with([' ', '.']) || s.contains(':'))
        {
            return Err(failure());
        }
        if fs::canonicalize(tail)? != fs::canonicalize(path)? {
            return Err(failure());
        }
        return Ok(tail.into());
    }
    Ok(path.clone())
}

fn execution(args: &[OsString]) -> io::Result<i32> {
    let version: u32 = match args.get(3).and_then(|v| v.to_str()) {
        Some("1") => 1,
        Some("2") => 2,
        _ => return Err(failure()),
    };
    let expected_transport = if cfg!(windows) {
        "windows-named-pipe"
    } else {
        "unix-domain-socket"
    };
    if env::var("PACTRUN_HOOK_PROTOCOL_TRANSPORT").ok().as_deref() != Some(expected_transport) {
        return Err(failure());
    }
    let endpoint = env::var("PACTRUN_HOOK_PROTOCOL_ENDPOINT").map_err(|_| failure())?;
    let mut wire = connect(&endpoint)?;
    let mut preamble = vec![0; protocol::PREAMBLE.len()];
    wire.read_exact(&mut preamble)?;
    let expected_preamble = if version == 1 {
        protocol::PREAMBLE
    } else {
        protocol::v2::PREAMBLE_V2
    };
    if preamble != expected_preamble {
        return Err(failure());
    }
    let session = read_frame(&mut wire)?;
    session::validate(&session, version)?;
    wire.write_all(&preamble)?;
    #[cfg(unix)]
    let listener = ProtocolListener::bind_helpers_in(Path::new(
        &env::var_os("PACTRUN_INTERNAL_SHELL_HELPER_DIRECTORY").ok_or_else(failure)?,
    ));
    #[cfg(windows)]
    let listener = ProtocolListener::bind_helpers();
    let mut listener = listener.inspect_err(|error| super::startup::publish(error, false))?;
    write_frame(
        &mut wire,
        &json!({"type":"session_ready", "protocol_version":version,"session_id":session["session_id"]}),
    )?;
    let mut child = script_command(args, &listener)
        .and_then(|mut command| command.spawn())
        .inspect_err(|error| super::startup::publish(error, true))?;
    let (send, recv) = mpsc::channel();
    let mut reader = wire.try_clone()?;
    let events = send.clone();
    thread::spawn(move || {
        loop {
            let value = read_frame(&mut reader);
            let end = value.is_err();
            if events.send(Event::Core(value)).is_err() || end {
                break;
            }
        }
    });
    let result = supervise(
        &mut child,
        &mut listener,
        &mut wire,
        State::new(session),
        send,
        recv,
    );
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

fn supervise(
    child: &mut Child,
    listener: &mut ProtocolListener,
    wire: &mut ProtocolStream,
    mut state: State,
    send: Sender<Event>,
    recv: Receiver<Event>,
) -> io::Result<i32> {
    let mut active_helper = false;
    let mut exit: Option<ExitStatus> = None;
    let mut completing = false;
    loop {
        match recv.recv_timeout(Duration::from_millis(5)) {
            Ok(Event::Helper(request, reply)) => state.request(request, reply, wire)?,
            Ok(Event::HelperDone(ok)) => {
                if !ok {
                    return Err(failure());
                }
                active_helper = false;
            }
            Ok(Event::Core(value)) => {
                let value = value?;
                match value["type"].as_str() {
                    Some("cancel") => {
                        session::cancel(&value)?;
                        write_frame(
                            wire,
                            &json!({"type":"cancel_ack","control_id":value["control_id"]}),
                        )?;
                        return Err(failure());
                    }
                    Some("completion_accepted")
                        if completing && value == json!({"type":"completion_accepted"}) =>
                    {
                        return Ok(if exit.is_some_and(|s| s.success()) {
                            0
                        } else {
                            1
                        });
                    }
                    Some("request_ack" | "target_ready_received") if !completing => {
                        state.acknowledgment(value)?
                    }
                    _ => return Err(failure()),
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err(failure()),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if exit.is_none() {
            exit = child.try_wait()?;
        }
        if let Some(status) = exit {
            if !active_helper && state.pending.is_none() && !completing {
                if state.target_received {
                    return if status.success() {
                        Ok(0)
                    } else {
                        Err(failure())
                    };
                }
                let wants_success = status.success() && state.completion["status"] != "failure";
                if wants_success && state.session.get("target_commit").is_some() {
                    return Err(failure());
                }
                if wants_success && state.open {
                    return Err(failure());
                }
                let message = state.complete(status.success());
                if !state.validate_message(&message) {
                    return Err(failure());
                }
                write_frame(wire, &message)?;
                completing = true;
            }
        } else if !active_helper && let Some(stream) = listener.try_accept()? {
            active_helper = true;
            serve_helper(stream, send.clone());
        }
    }
}

pub(crate) fn run(args: &[OsString]) -> i32 {
    match execution(args) {
        Ok(code) => code,
        Err(_) => {
            eprintln!("error: shell adapter execution failed");
            1
        }
    }
}

pub(crate) fn helper(args: &[OsString]) -> i32 {
    helper::run(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Value {
        json!({"type":"session_start","protocol_version":1,"session_id":"1".repeat(32),"run_id":"2".repeat(32),
            "revision":{"package_id":"3".repeat(32),"revision_content_digest":format!("sha256:{}", "4".repeat(64))},
            "parameters":[{"parameter_id":"flag","value":true}],"workspace":{"handle":"5".repeat(32),"root_path":env::current_dir().unwrap()},
            "io":{"terminal":"none"},"operation":{"kind":"action","action_id":"run","access":"mutate","bindings":[],"outputs":[]}})
    }

    fn pair() -> (ProtocolListener, ProtocolStream, ProtocolStream) {
        let mut listener = ProtocolListener::bind_helpers().unwrap();
        let peer = connect(listener.endpoint()).unwrap();
        let server = loop {
            if let Some(stream) = listener.try_accept().unwrap() {
                break stream;
            }
            thread::yield_now();
        };
        (listener, server, peer)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn loader_startup_failure_worker() {
        let Some(version) = env::var_os("PACTRUN_TEST_STARTUP_WORKER") else {
            return;
        };
        let args = [
            OsString::from("sh"),
            OsString::from("/bin/sh"),
            OsString::from("/never-start-this-script"),
            version,
        ];
        std::process::exit(run(&args));
    }

    // Test-ID: PR-TEST-0576
    // Verifies: PR-REQ-0362
    #[cfg(target_os = "linux")]
    #[test]
    fn real_loader_reports_helper_bind_failure_before_ready_or_user_script() {
        for version in [1, 2] {
            let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp");
            fs::create_dir_all(&parent).unwrap();
            let root = tempfile::tempdir_in(parent).unwrap();
            let setup = super::super::startup::Setup::new(root.path()).unwrap();
            let blocker = setup.helper.path.join("helper.sock");
            fs::write(&blocker, b"not a socket; never remove an unrelated file").unwrap();
            let mut listener = ProtocolListener::bind().unwrap();
            let mut child = Command::new(env::current_exe().unwrap())
                .args([
                    "--exact",
                    "hook::shell_loader::tests::loader_startup_failure_worker",
                    "--nocapture",
                ])
                .env("PACTRUN_TEST_STARTUP_WORKER", version.to_string())
                .env("PACTRUN_HOOK_PROTOCOL_TRANSPORT", "unix-domain-socket")
                .env("PACTRUN_HOOK_PROTOCOL_ENDPOINT", listener.endpoint())
                .env(super::super::startup::STATUS_ENV, &setup.status)
                .env(
                    "PACTRUN_INTERNAL_SHELL_HELPER_DIRECTORY",
                    &setup.helper.path,
                )
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                if let Some(stream) = listener.try_accept().unwrap() {
                    break stream;
                }
                if std::time::Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("loader did not connect");
                }
                thread::sleep(Duration::from_millis(5));
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let preamble = if version == 1 {
                protocol::PREAMBLE
            } else {
                protocol::v2::PREAMBLE_V2
            };
            stream.write_all(preamble).unwrap();
            let mut value = session();
            value["protocol_version"] = json!(version);
            if version == 2 {
                value["service_authorities"] = json!([]);
            }
            write_frame(&mut stream, &value).unwrap();
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).unwrap();
            assert!(!child.wait().unwrap().success());
            assert_eq!(bytes, preamble); // No session_ready was sent.
            assert!(matches!(
                setup.failure(),
                Some(super::super::FailureKind::LoaderInitialization(_))
            ));
            assert_eq!(
                fs::read(&blocker).unwrap(),
                b"not a socket; never remove an unrelated file"
            );
            fs::remove_file(blocker).unwrap();
        }
    }

    // Test-ID: PR-TEST-0515
    // Verifies: PR-REQ-0349
    #[test]
    fn helper_envelope_preserves_full_size_session_without_enlarging_core_frames() {
        let mut session = session();
        session["parameters"][0]["value"] = json!("");
        let overhead = serde_json::to_vec(&session).unwrap().len();
        session["parameters"][0]["value"] = json!("x".repeat(LIMIT - overhead));
        assert_eq!(serde_json::to_vec(&session).unwrap().len(), LIMIT);
        let reply = json!({"ok":true,"value":session});
        let mut frame = Vec::new();
        write_helper_frame(&mut frame, &reply).unwrap();
        assert!(frame.len() > LIMIT + 4);
        assert!(write_frame(&mut Vec::new(), &reply).is_err());
        assert_eq!(read_helper_frame(&mut frame.as_slice()).unwrap(), reply);
    }

    // Test-ID: PR-TEST-0491
    // Verifies: PR-REQ-0082, PR-REQ-0349
    #[test]
    fn helper_risk_returns_only_after_exact_ack_and_never_replays_uncertainty() {
        let (_listener, mut wire, mut peer) = pair();
        let mut state = State::new(session());
        let (send, recv) = mpsc::channel();
        state
            .request(Request::Risk { enter: true }, send, &mut wire)
            .unwrap();
        assert!(recv.try_recv().is_err());
        assert!(!state.open);
        assert_eq!(
            read_frame(&mut peer).unwrap(),
            json!({"type":"request","request_id":1,"request":{"kind":"enter_recovery_risk"}})
        );
        state
            .acknowledgment(json!({"type":"request_ack","request_id":1,"risk_state":"open"}))
            .unwrap();
        assert!(state.open);
        assert_eq!(recv.recv().unwrap()["ok"], true);
        let (send, recv) = mpsc::channel();
        state
            .request(Request::Risk { enter: false }, send, &mut wire)
            .unwrap();
        assert!(
            state
                .acknowledgment(json!({"type":"request_ack","request_id":1,"risk_state":"clear"}))
                .is_err()
        );
        assert!(recv.recv().is_err());
        assert!(state.open);
    }

    // Test-ID: PR-TEST-0511
    // Verifies: PR-REQ-0349
    #[test]
    fn shell_session_and_frame_validation_reject_unknown_duplicate_and_foreign_data() {
        let good = session();
        session::validate(&good, 1).unwrap();
        let mut equivalent = good.clone();
        equivalent["protocol_version"] = json!(1.0);
        session::validate(&equivalent, 1).unwrap();
        session::cancel(&json!({"type":"cancel","control_id":1.0,"reason":"requested"})).unwrap();
        session::risk_ack(
            &json!({"type":"request_ack","request_id":1.0,"risk_state":"open"}),
            1,
            true,
        )
        .unwrap();
        for (field, value) in [
            ("extra", json!(1)),
            ("protocol_version", json!(2)),
            ("session_id", json!("bad")),
        ] {
            let mut bad = good.clone();
            bad[field] = value;
            assert!(session::validate(&bad, 1).is_err());
        }
        let mut bad = good.clone();
        bad["operation"]["outputs"] = json!([{"handle":"5".repeat(32),"output_id":"out","staged_path":env::current_dir().unwrap()}]);
        assert!(session::validate(&bad, 1).is_err());
        for bytes in [
            b"{\"x\":1,\"x\":2}".as_slice(),
            b"\xff",
            b"{\"x\":\"\\ud800\"}",
            br#"{"type":"request_ack","request_id":1.000000000000000001,"risk_state":"open"}"#,
            br#"{"type":"cancel","control_id":9007199254740992,"reason":"requested"}"#,
            br#"{"type":"request_ack","request_id":0,"risk_state":"clear"}"#,
        ] {
            let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
            frame.extend(bytes);
            assert!(read_frame(&mut frame.as_slice()).is_err());
        }
        assert!(read_frame(&mut ((LIMIT + 1) as u32).to_be_bytes().as_slice()).is_err());
        assert!(read_frame(&mut [0_u8, 0, 0, 4, b'{'].as_slice()).is_err());
    }

    // Test-ID: PR-TEST-0512
    // Verifies: PR-REQ-0349
    #[test]
    fn explicit_registration_rejects_duplicates_and_failure_never_publishes_capture() {
        let (_listener, mut wire, _peer) = pair();
        let mut state = State::new(session());
        let (send, recv) = mpsc::channel();
        state
            .request(
                Request::OutputRegister {
                    id: "foreign".into(),
                },
                send,
                &mut wire,
            )
            .unwrap();
        assert_eq!(recv.recv().unwrap()["ok"], false);
        state.session["operation"] = json!({"kind":"snapshot_capture","bindings":[],"candidate":{"handle":"6".repeat(32),"root_path":env::current_dir().unwrap()}});
        let descriptor = json!({"role":"state","path":"data","candidate_path":"data"});
        for expected in [true, false] {
            let (send, recv) = mpsc::channel();
            state
                .request(
                    Request::CaptureRegister {
                        value: descriptor.clone(),
                    },
                    send,
                    &mut wire,
                )
                .unwrap();
            assert_eq!(recv.recv().unwrap()["ok"], expected);
        }
        assert_eq!(state.complete(false)["service_content"], json!([]));
        assert_eq!(state.complete(true)["service_content"], json!([descriptor]));
    }

    // Test-ID: PR-TEST-0513
    // Verifies: PR-REQ-0349
    #[test]
    fn helper_disconnect_after_issued_request_is_an_uncertain_failure() {
        let (_listener, server, mut peer) = pair();
        let (send, recv) = mpsc::channel();
        serve_helper(server, send);
        write_frame(&mut peer, &json!({"kind":"risk","enter":true})).unwrap();
        let Event::Helper(_, reply) = recv.recv_timeout(Duration::from_secs(2)).unwrap() else {
            panic!("expected helper request")
        };
        drop(peer);
        reply.send(json!({"ok":true,"value":"open"})).unwrap();
        assert!(matches!(
            recv.recv_timeout(Duration::from_secs(2)).unwrap(),
            Event::HelperDone(false)
        ));
    }

    // Test-ID: PR-TEST-0514
    // Verifies: PR-REQ-0349
    #[test]
    fn transform_receipt_is_not_completion_and_rejects_later_protocol_calls() {
        let (_listener, mut wire, mut peer) = pair();
        let mut state = State::new(session());
        state.open = true;
        state.session["operation"] = json!({"kind":"migration","target_outputs":[]});
        state.session["target_commit"] = json!({"handle":"9".repeat(32)});
        let (send, recv) = mpsc::channel();
        state
            .request(Request::TargetReady, send, &mut wire)
            .unwrap();
        assert!(recv.try_recv().is_err());
        assert!(!state.target_received);
        assert_eq!(read_frame(&mut peer).unwrap()["type"], "target_ready");
        state
            .acknowledgment(json!({"type":"target_ready_received","commit_handle":"9".repeat(32)}))
            .unwrap();
        assert_eq!(recv.recv().unwrap()["ok"], true);
        assert!(state.target_received);
        assert!(state.open);
        let (send, recv) = mpsc::channel();
        state
            .request(Request::Risk { enter: false }, send, &mut wire)
            .unwrap();
        assert_eq!(recv.recv().unwrap()["ok"], false);
        assert!(state.open);
    }
}
