//! Platform seams: the owner-private protocol listener, supervised process
//! launch with process-tree control, and terminal stream wiring.

use std::{
    fmt, io,
    process::{Child, Command, ExitStatus, Stdio},
};

#[cfg(unix)]
use std::{fs, path::Path};

use crate::domain::TerminalContractV1;

const TRANSPORT_ENVIRONMENT: &str = "PACTRUN_HOOK_PROTOCOL_TRANSPORT";
const ENDPOINT_ENVIRONMENT: &str = "PACTRUN_HOOK_PROTOCOL_ENDPOINT";

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
    pub(super) fn inject_environment(&self, command: &mut Command) {
        command.env(TRANSPORT_ENVIRONMENT, platform_transport_name());
        command.env(ENDPOINT_ENVIRONMENT, &self.endpoint);
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
    child: Child,
    #[cfg(windows)]
    job: pactrun_windows_ntfs::ProcessJob,
}

impl ProcessSupervisor {
    pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let child = command.spawn()?;
        #[cfg(windows)]
        {
            let mut child = child;
            let job = match pactrun_windows_ntfs::ProcessJob::assign(&child) {
                Ok(job) => job,
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
            };
            Ok(Self { child, job })
        }
        #[cfg(unix)]
        {
            Ok(Self { child })
        }
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    pub(super) fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }

    pub(super) fn terminate_tree(&mut self) -> io::Result<()> {
        #[cfg(windows)]
        {
            self.job.terminate()
        }
        #[cfg(unix)]
        {
            rustix::process::kill_process_group(
                rustix::process::Pid::from_child(&self.child),
                rustix::process::Signal::KILL,
            )
            .map_err(io::Error::from)
        }
    }
}

pub(super) fn configure_terminal(command: &mut Command, terminal: TerminalContractV1) {
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
