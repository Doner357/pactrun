//! Platform seams: the owner-private protocol listener, supervised process
//! launch with process-tree control, and terminal stream wiring.

use std::{fmt, io, path::Path, process::ExitStatus};

#[cfg(unix)]
use std::io::{Read, Write};

#[cfg(any(unix, test))]
use std::process::Command;

#[cfg(any(unix, windows))]
use std::ffi::OsString;

#[cfg(unix)]
use std::{
    fs,
    process::{Child, Stdio},
    thread::JoinHandle,
};

use crate::domain::TerminalContractV1;

const TRANSPORT_ENVIRONMENT: &str = "PACTRUN_HOOK_PROTOCOL_TRANSPORT";
const ENDPOINT_ENVIRONMENT: &str = "PACTRUN_HOOK_PROTOCOL_ENDPOINT";
const ADAPTER_READY_ENVIRONMENT: &str = "PACTRUN_INTERNAL_INTERACTIVE_ADAPTER_READY";
#[cfg(test)]
const TEST_INTERACTIVE_PROGRAM_ENVIRONMENT: &str = "PACTRUN_TEST_INTERACTIVE_PROGRAM";
pub(crate) const INTERACTIVE_ADAPTER_ARGUMENT: &str = "--pactrun-internal-interactive-adapter";

#[cfg(windows)]
pub(super) type ProtocolStream = pactrun_windows_ntfs::NamedPipeStream;

#[cfg(unix)]
pub(super) type ProtocolStream = std::os::unix::net::UnixStream;

pub(super) struct ProtocolListener {
    endpoint: String,
    inner: PlatformProtocolListener,
}

impl fmt::Debug for ProtocolListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProtocolListener")
            .field("endpoint", &"<owner-private>")
            .finish()
    }
}

impl ProtocolListener {
    /// Creates the one-connection owner-private listener before launch.
    pub(super) fn bind() -> io::Result<Self> {
        platform_listener()
    }

    /// PR-REQ-0280: both discovery variables are replaced in the exact child
    /// environment; ambient values are never trusted.
    #[cfg(any(unix, test))]
    pub(super) fn inject_environment(&self, command: &mut Command) {
        command.envs(self.environment());
    }

    fn environment(&self) -> [(&str, &str); 2] {
        [
            (TRANSPORT_ENVIRONMENT, platform_transport_name()),
            (ENDPOINT_ENVIRONMENT, &self.endpoint),
        ]
    }

    pub(super) fn try_accept(&mut self) -> io::Result<Option<ProtocolStream>> {
        platform_try_accept(&mut self.inner)
    }

    #[cfg(test)]
    pub(super) fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

#[cfg(unix)]
impl Drop for ProtocolListener {
    fn drop(&mut self) {
        let path = Path::new(&self.endpoint);
        let _ = fs::remove_file(path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
}

#[derive(Debug)]
pub(super) struct ProcessSupervisor {
    #[cfg(unix)]
    child: Child,
    #[cfg(unix)]
    terminal: TerminalContractV1,
    #[cfg(unix)]
    output_relay: Option<JoinHandle<()>>,
    #[cfg(unix)]
    input_relay: Option<JoinHandle<()>>,
    #[cfg(windows)]
    child: pactrun_windows_ntfs::HookProcess,
}

impl ProcessSupervisor {
    pub(super) fn spawn(
        program: &Path,
        arguments: &[String],
        terminal: TerminalContractV1,
        listener: &ProtocolListener,
    ) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            if terminal == TerminalContractV1::Interactive {
                return spawn_interactive_adapter(program, arguments, listener);
            }
            let mut command = Command::new(program);
            command.args(arguments);
            listener.inject_environment(&mut command);
            configure_terminal(&mut command, terminal);
            command.process_group(0);
            Ok(Self {
                child: command.spawn()?,
                terminal,
                output_relay: None,
                input_relay: None,
            })
        }
        #[cfg(windows)]
        {
            use pactrun_windows_ntfs::{HookProcess, HookTerminal};
            let terminal = match terminal {
                TerminalContractV1::None => HookTerminal::None,
                TerminalContractV1::Output => HookTerminal::Output,
                TerminalContractV1::Interactive => HookTerminal::Interactive,
            };
            #[cfg(test)]
            let admitted_program = program;
            let (program, arguments) = if matches!(terminal, HookTerminal::Interactive) {
                #[cfg(test)]
                let adapter_arguments = {
                    let mut adapter_arguments = vec![
                        OsString::from("--exact"),
                        OsString::from(
                            "hook::platform::windows_adapter_tests::interactive_adapter",
                        ),
                        OsString::from("pactrun-test-adapter"),
                    ];
                    adapter_arguments.extend(
                        arguments
                            .iter()
                            .filter(|argument| argument.as_str() != "--exact")
                            .map(|argument| OsString::from(argument.as_str())),
                    );
                    adapter_arguments
                };
                #[cfg(not(test))]
                let adapter_arguments = {
                    let mut adapter_arguments = vec![OsString::from(INTERACTIVE_ADAPTER_ARGUMENT)];
                    adapter_arguments.push(program.as_os_str().to_os_string());
                    adapter_arguments.extend(
                        arguments
                            .iter()
                            .map(|argument| OsString::from(argument.as_str())),
                    );
                    adapter_arguments
                };
                (std::env::current_exe()?, adapter_arguments)
            } else {
                (
                    program.to_path_buf(),
                    arguments
                        .iter()
                        .map(|argument| OsString::from(argument.as_str()))
                        .collect(),
                )
            };
            #[cfg(test)]
            let child = if matches!(terminal, HookTerminal::Interactive) {
                HookProcess::spawn_with_extra_environment(
                    &program,
                    &arguments,
                    terminal,
                    listener.environment(),
                    (
                        TEST_INTERACTIVE_PROGRAM_ENVIRONMENT,
                        admitted_program.as_os_str(),
                    ),
                )?
            } else {
                HookProcess::spawn(&program, &arguments, terminal, listener.environment())?
            };
            #[cfg(not(test))]
            let child = HookProcess::spawn(&program, &arguments, terminal, listener.environment())?;
            Ok(Self { child })
        }
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let status = self.child.try_wait()?;
        #[cfg(unix)]
        if status.is_some() {
            // A descendant may still hold the PTY. Do not block the owner in
            // join(): it must continue observing cancellation and deadlines.
            if self
                .output_relay
                .as_ref()
                .is_some_and(|relay| !relay.is_finished())
            {
                return Ok(None);
            }
            self.join_output_relay();
        }
        Ok(status)
    }

    pub(super) fn wait(&mut self) -> io::Result<ExitStatus> {
        let status = self.child.wait()?;
        #[cfg(unix)]
        self.join_output_relay();
        Ok(status)
    }

    pub(super) fn terminate_tree(&mut self) -> io::Result<()> {
        #[cfg(windows)]
        {
            self.child.terminate_tree()
        }
        #[cfg(unix)]
        {
            let child = rustix::process::Pid::from_child(&self.child);
            rustix::process::kill_process_group(child, rustix::process::Signal::KILL)
                .map_err(io::Error::from)
        }
    }
    pub(super) fn tree_terminated(&self) -> io::Result<bool> {
        #[cfg(windows)]
        {
            self.child.tree_terminated()
        }
        #[cfg(unix)]
        {
            match rustix::process::test_kill_process_group(rustix::process::Pid::from_child(
                &self.child,
            )) {
                Err(rustix::io::Errno::SRCH) => Ok(true),
                Ok(()) => Ok(false),
                Err(error) => Err(error.into()),
            }
        }
    }

    #[cfg(unix)]
    fn join_output_relay(&mut self) {
        if let Some(relay) = self.output_relay.take() {
            let _ = relay.join();
        }
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
fn spawn_interactive_adapter(
    program: &Path,
    arguments: &[String],
    listener: &ProtocolListener,
) -> io::Result<ProcessSupervisor> {
    let mut command = Command::new(std::env::current_exe()?);
    #[cfg(test)]
    command.args([
        "--exact",
        "hook::platform::unix_adapter_tests::interactive_adapter",
        "--nocapture",
        "--",
    ]);
    #[cfg(not(test))]
    command.arg(INTERACTIVE_ADAPTER_ARGUMENT);
    #[cfg(test)]
    command.arg(INTERACTIVE_ADAPTER_ARGUMENT);
    command.arg(program);
    command.args(arguments);
    listener.inject_environment(&mut command);
    #[cfg(test)]
    let readiness_path = format!("{}.interactive-ready", listener.endpoint);
    #[cfg(test)]
    command.env(ADAPTER_READY_ENVIRONMENT, &readiness_path);
    command.stdin(Stdio::piped()).stdout(Stdio::piped());
    #[cfg(test)]
    command.stderr(Stdio::null());
    #[cfg(not(test))]
    command.stderr(Stdio::piped());
    // The adapter becomes its own session and process group before emitting
    // READY. Waiting for that marker closes the small window in which a
    // cancellation could otherwise attempt to kill a not-yet-isolated group.
    let mut child = command.spawn()?;
    #[cfg(test)]
    {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !Path::new(&readiness_path).exists() {
            if let Some(status) = child.try_wait()? {
                return Err(io::Error::other(format!(
                    "interactive adapter exited before readiness: {status}"
                )));
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "interactive adapter readiness timed out",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    #[cfg(not(test))]
    {
        let mut ready = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("interactive adapter has no readiness channel"))?;
        let mut marker = [0_u8; 1];
        if let Err(error) = io::Read::read_exact(&mut ready, &mut marker) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        if marker != [0] {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "interactive adapter readiness marker was invalid",
            ));
        }
    }
    let adapter_stdin = child.stdin.take().ok_or_else(|| {
        let _ = child.kill();
        let _ = child.wait();
        io::Error::other("interactive adapter has no input channel")
    })?;
    let adapter_stdout = child.stdout.take().ok_or_else(|| {
        let _ = child.kill();
        let _ = child.wait();
        io::Error::other("interactive adapter has no output channel")
    })?;
    let input_relay = std::thread::spawn(move || relay_terminal_input(adapter_stdin));
    let output_relay = std::thread::spawn(move || relay_terminal_output(adapter_stdout));
    Ok(ProcessSupervisor {
        child,
        terminal: TerminalContractV1::Interactive,
        output_relay: Some(output_relay),
        input_relay: Some(input_relay),
    })
}

#[cfg(unix)]
fn relay_terminal_input(mut destination: impl io::Write) {
    let mut source = io::stdin();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = match source.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => read,
        };
        let mut ordinary_start = 0;
        for index in 0..read {
            if buffer[index] != 0x03 {
                continue;
            }
            if ordinary_start < index
                && destination
                    .write_all(&buffer[ordinary_start..index])
                    .is_err()
            {
                return;
            }
            // If the owner's terminal is configured without ISIG, a literal
            // ^C would otherwise be forwarded into the Hook's private PTY and
            // terminate the Hook instead of cancelling the owner. Preserve
            // Ctrl+C semantics at this relay boundary and consume the byte.
            if let Some(owner) = rustix::process::Pid::from_raw(std::process::id() as i32) {
                let _ = rustix::process::kill_process(owner, rustix::process::Signal::INT);
            }
            ordinary_start = index + 1;
        }
        if ordinary_start < read
            && destination
                .write_all(&buffer[ordinary_start..read])
                .is_err()
        {
            return;
        }
        let _ = destination.flush();
    }
}

#[cfg(unix)]
fn relay_terminal_output(mut source: impl io::Read) {
    let mut destination = io::stdout();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = match source.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => read,
        };
        if destination.write_all(&buffer[..read]).is_err() {
            return;
        }
        let _ = destination.flush();
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
pub(super) fn run_interactive_adapter(arguments: &[OsString]) -> i32 {
    use std::{env, fs::File};

    use rustix::fs::{Mode, OFlags};

    let Some((program, hook_arguments)) = arguments.split_first() else {
        return 1;
    };
    let pty = match rustix_openpty::openpty(None, None) {
        Ok(pty) => pty,
        Err(_) => return 1,
    };
    let mut parent_input = match rustix::io::dup(rustix::stdio::stdin()) {
        Ok(fd) => File::from(fd),
        Err(_) => return 1,
    };
    let mut parent_output = match rustix::io::dup(rustix::stdio::stdout()) {
        Ok(fd) => File::from(fd),
        Err(_) => return 1,
    };
    let mut readiness = match env::var_os(ADAPTER_READY_ENVIRONMENT) {
        Some(_) => None,
        None => match rustix::io::dup(rustix::stdio::stderr()) {
            Ok(fd) => Some(File::from(fd)),
            Err(_) => return 1,
        },
    };
    let master = File::from(pty.controller);
    if rustix_openpty::login_tty(pty.user).is_err() {
        return 1;
    }

    let input_master = match master.try_clone() {
        Ok(master) => master,
        Err(_) => return 1,
    };
    std::thread::spawn(move || {
        let mut master = input_master;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let read = match parent_input.read(&mut buffer) {
                Ok(0) | Err(_) => return,
                Ok(read) => read,
            };
            if master.write_all(&buffer[..read]).is_err() {
                return;
            }
            let _ = master.flush();
        }
    });

    let output_master = match master.try_clone() {
        Ok(master) => master,
        Err(_) => return 1,
    };
    let output_relay = std::thread::spawn(move || {
        let mut master = output_master;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let read = match master.read(&mut buffer) {
                Ok(0) | Err(_) => return,
                Ok(read) => read,
            };
            if parent_output.write_all(&buffer[..read]).is_err() {
                return;
            }
            let _ = parent_output.flush();
        }
    });

    if let Some(path) = env::var_os(ADAPTER_READY_ENVIRONMENT) {
        if fs::write(path, b"ready").is_err() {
            return 1;
        }
    } else if let Some(readiness) = &mut readiness
        && (readiness.write_all(&[0]).is_err() || readiness.flush().is_err())
    {
        return 1;
    }
    let mut hook_command = Command::new(program);
    hook_command.args(hook_arguments);
    #[cfg(test)]
    hook_command.env_remove(ADAPTER_READY_ENVIRONMENT);
    let status = match hook_command.status() {
        Ok(status) => status,
        Err(error) => {
            eprintln!("interactive adapter could not launch Hook: {error}");
            return 1;
        }
    };
    // Close the slave-side standard descriptors by safely dup2'ing over them
    // with /dev/null. The adapter must then be able to drain the PTY master
    // before returning its Hook status to the owner.
    let slave_closed = rustix::fs::open("/dev/null", OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())
        .is_ok_and(|null| {
            rustix::stdio::dup2_stdin(&null).is_ok()
                && rustix::stdio::dup2_stdout(&null).is_ok()
                && rustix::stdio::dup2_stderr(&null).is_ok()
        });
    if !slave_closed {
        return status.code().unwrap_or(1);
    }
    drop(master);
    let _ = output_relay.join();
    status.code().unwrap_or(1)
}

#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    ))
))]
fn unsupported_interactive_adapter() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "interactive terminal Hooks are unsupported on this POSIX target",
    )
}

#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    ))
))]
fn spawn_interactive_adapter(
    _program: &Path,
    _arguments: &[String],
    _listener: &ProtocolListener,
) -> io::Result<ProcessSupervisor> {
    Err(unsupported_interactive_adapter())
}

#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    ))
))]
pub(super) fn run_interactive_adapter(_arguments: &[OsString]) -> i32 {
    1
}

#[cfg(windows)]
pub(super) fn run_interactive_adapter(arguments: &[OsString]) -> i32 {
    let Some((program, hook_arguments)) = arguments.split_first() else {
        return 1;
    };
    if pactrun_windows_ntfs::ignore_console_ctrl_c().is_err() {
        return 1;
    }
    match pactrun_windows_ntfs::run_exact_in_current_job(
        Path::new(program),
        hook_arguments,
        pactrun_windows_ntfs::HookTerminal::Interactive,
    ) {
        Ok(status) => status.code().unwrap_or(1),
        Err(_) => 1,
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::{
        env, fs,
        io::{self, Write},
        net::TcpListener,
        path::Path,
        process::{Command, Stdio},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::Duration,
    };

    use super::{ProcessSupervisor, ProtocolListener};
    use crate::domain::TerminalContractV1;
    use crate::hook::ActionCancellation;

    const WORKER_MARKER: &str = "pactrun-pty-worker";
    const HOOK_MARKER: &str = "pactrun-pty-hook";
    const DESCENDANT_MARKER: &str = "pactrun-pty-descendant";

    // Test-ID: PR-TEST-0132
    // Verifies: PR-REQ-0052, PR-REQ-0286
    #[test]
    fn linux_interactive_hook_uses_real_pty_and_foreground_ctrl_c() {
        let mut child = Command::new("python3")
            .arg("-c")
            .arg(PYTHON_PTY_DRIVER)
            .arg(env::current_exe().unwrap())
            .arg(WORKER_MARKER)
            .spawn()
            .expect("python3 is required for the real PTY harness");
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn pty_worker() {
        if !env::args().any(|argument| argument == WORKER_MARKER) {
            return;
        }
        let cancellation = ActionCancellation::default();
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&cancelled);
        ctrlc::set_handler(move || {
            signal.store(true, Ordering::Release);
        })
        .unwrap();
        let listener = ProtocolListener::bind().unwrap();
        let descendant_port_path = env::temp_dir().join(format!(
            "pactrun-pty-descendant-port-{}",
            std::process::id()
        ));
        let _ = fs::remove_file(&descendant_port_path);
        let hook_args = vec![
            "--exact".to_owned(),
            "hook::platform::tests::pty_hook".to_owned(),
            "--nocapture".to_owned(),
            HOOK_MARKER.to_owned(),
            descendant_port_path.to_str().unwrap().to_owned(),
        ];
        let mut hook = ProcessSupervisor::spawn(
            &env::current_exe().unwrap(),
            &hook_args,
            TerminalContractV1::Interactive,
            &listener,
        )
        .unwrap();
        let mut output = io::stdout();
        writeln!(output, "PTY_WORKER_READY").unwrap();
        output.flush().unwrap();
        while !cancelled.load(Ordering::Acquire) {
            assert!(
                hook.try_wait().unwrap().is_none(),
                "Hook exited before Ctrl+C"
            );
            thread::sleep(Duration::from_millis(10));
        }
        cancellation.request();
        hook.terminate_tree().unwrap();
        let _ = hook.wait();
        let port = fs::read_to_string(&descendant_port_path)
            .unwrap()
            .trim()
            .parse::<u16>()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if TcpListener::bind(("127.0.0.1", port)).is_ok() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "descendant survived tree termination"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let _ = fs::remove_file(&descendant_port_path);
        writeln!(output, "PTY_WORKER_CANCELLED").unwrap();
        output.flush().unwrap();
    }

    #[test]
    fn pty_hook() {
        if !env::args().any(|argument| argument == HOOK_MARKER) {
            return;
        }
        let descendant_port_path = env::args()
            .nth(5)
            .expect("the PTY worker passes the descendant marker path");
        let mut descendant = Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "hook::platform::tests::pty_descendant",
                "--nocapture",
                DESCENDANT_MARKER,
                &descendant_port_path,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !Path::new(&descendant_port_path).exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "descendant did not become ready"
            );
            thread::sleep(Duration::from_millis(10));
        }
        // The ordinary Hook deliberately installs no SIGINT handler. Its
        // private PTY is not the owner's foreground terminal; Pactrun owns
        // Ctrl+C and terminates the complete adapter process group.
        writeln!(io::stdout(), "PTY_HOOK_DESCENDANT_READY").unwrap();
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        assert_eq!(input, "pty-input\n");
        println!("PTY_HOOK_READ");
        io::stdout().flush().unwrap();
        let _ = descendant.try_wait();
        std::mem::forget(descendant);
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }

    #[test]
    fn pty_descendant() {
        if !env::args().any(|argument| argument == DESCENDANT_MARKER) {
            return;
        }
        let path = env::args().nth(5).expect("descendant marker path");
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        fs::write(&path, listener.local_addr().unwrap().port().to_string()).unwrap();
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }

    const PYTHON_PTY_DRIVER: &str = r#"
import os
import pty
import select
import sys
import time

exe = sys.argv[1]
marker = sys.argv[2]
pid, fd = pty.fork()
if pid == 0:
    os.execv(exe, [exe, '--exact', 'hook::platform::tests::pty_worker', '--nocapture', marker])

def wait_for(needle):
    data = b''
    deadline = time.time() + 30
    while needle not in data:
        if time.time() >= deadline:
            raise RuntimeError('PTY harness timeout waiting for ' + repr(needle) + ': ' + repr(data))
        ready, _, _ = select.select([fd], [], [], 0.1)
        if ready:
            try:
                data += os.read(fd, 4096)
            except OSError:
                break
    return data

wait_for(b'PTY_WORKER_READY')
os.write(fd, b'pty-input\n')
wait_for(b'PTY_HOOK_READ')
os.write(fd, b'\x03')
wait_for(b'PTY_WORKER_CANCELLED')
_, status = os.waitpid(pid, 0)
if not os.WIFEXITED(status) or os.WEXITSTATUS(status) != 0:
    raise RuntimeError('PTY worker failed: ' + repr(status))
"#;
}

#[cfg(all(
    test,
    any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )
))]
mod unix_adapter_tests {
    use std::env;

    use super::INTERACTIVE_ADAPTER_ARGUMENT;

    #[test]
    fn interactive_adapter() {
        let arguments: Vec<_> = env::args_os().skip(1).collect();
        let Some(index) = arguments
            .iter()
            .position(|argument| argument == INTERACTIVE_ADAPTER_ARGUMENT)
        else {
            return;
        };
        let status = super::run_interactive_adapter(&arguments[index + 1..]);
        std::process::exit(status);
    }
}

#[cfg(all(test, windows))]
mod windows_adapter_tests {
    use std::env;

    #[test]
    fn interactive_adapter() {
        let arguments: Vec<_> = env::args_os().skip(1).collect();
        let Some(index) = arguments
            .iter()
            .position(|argument| argument == "pactrun-test-adapter")
        else {
            return;
        };
        let Some(program) = env::var_os(super::TEST_INTERACTIVE_PROGRAM_ENVIRONMENT) else {
            return;
        };
        let mut adapter_arguments = Vec::with_capacity(arguments.len() - index);
        adapter_arguments.push(program);
        adapter_arguments.push("--exact".into());
        adapter_arguments.extend(arguments[index + 1..].iter().cloned());
        let status = super::run_interactive_adapter(&adapter_arguments);
        std::process::exit(status);
    }
}

#[cfg(unix)]
fn configure_terminal(command: &mut Command, terminal: TerminalContractV1) {
    match terminal {
        TerminalContractV1::None => {
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
        }
        TerminalContractV1::Output => {
            command
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
        }
        TerminalContractV1::Interactive => {
            command
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
        }
    }
}

#[cfg(windows)]
type PlatformProtocolListener = pactrun_windows_ntfs::NamedPipeListener;

#[cfg(unix)]
type PlatformProtocolListener = std::os::unix::net::UnixListener;

#[cfg(windows)]
fn platform_listener() -> io::Result<ProtocolListener> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random)
        .map_err(|error| io::Error::other(format!("generate named-pipe endpoint: {error}")))?;
    let endpoint = format!(r"\\.\pipe\pactrun-{}", hex::encode(random));
    let inner = pactrun_windows_ntfs::NamedPipeListener::bind(&endpoint)?;
    Ok(ProtocolListener { endpoint, inner })
}

/// `sun_path` is limited to roughly one hundred bytes, so the socket lives in
/// a fresh owner-private directory under the system temporary directory
/// rather than beneath the (arbitrarily deep) execution tree.
#[cfg(unix)]
fn platform_listener() -> io::Result<ProtocolListener> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    let mut random = [0_u8; 16];
    getrandom::fill(&mut random)
        .map_err(|error| io::Error::other(format!("generate socket directory: {error}")))?;
    let directory = std::env::temp_dir().join(format!("pactrun-{}", hex::encode(random)));
    fs::DirBuilder::new().mode(0o700).create(&directory)?;
    let path = directory.join("hook.sock");
    let endpoint = match path.to_str() {
        Some(endpoint) => endpoint.to_owned(),
        None => {
            let _ = fs::remove_dir(&directory);
            return Err(io::Error::other("socket path is not valid Unicode"));
        }
    };
    let inner = match std::os::unix::net::UnixListener::bind(&path) {
        Ok(inner) => inner,
        Err(error) => {
            let _ = fs::remove_dir(&directory);
            return Err(error);
        }
    };
    if let Err(error) = inner
        .set_nonblocking(true)
        .and_then(|()| fs::set_permissions(&path, fs::Permissions::from_mode(0o600)))
    {
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&directory);
        return Err(error);
    }
    Ok(ProtocolListener { endpoint, inner })
}

#[cfg(windows)]
fn platform_try_accept(
    listener: &mut PlatformProtocolListener,
) -> io::Result<Option<ProtocolStream>> {
    listener.try_accept()
}

#[cfg(unix)]
fn platform_try_accept(
    listener: &mut PlatformProtocolListener,
) -> io::Result<Option<ProtocolStream>> {
    match listener.accept() {
        Ok((stream, _)) => Ok(Some(stream)),
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn platform_transport_name() -> &'static str {
    "windows-named-pipe"
}

#[cfg(unix)]
fn platform_transport_name() -> &'static str {
    "unix-domain-socket"
}
