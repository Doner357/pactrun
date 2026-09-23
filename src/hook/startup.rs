//! Private built-in Loader startup evidence, never a Hook protocol extension.
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub(super) const STATUS_ENV: &str = "PACTRUN_INTERNAL_LOADER_STARTUP";

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Reason {
    PathUnavailable,
    PermissionDenied,
    ResourceUnavailable,
    InvalidEnvironment,
    Other,
}

impl Reason {
    pub(super) fn from_io(error: &io::Error) -> Self {
        #[cfg(unix)]
        if let Some(raw) = error.raw_os_error() {
            use rustix::io::Errno;
            if [
                Errno::MFILE,
                Errno::NFILE,
                Errno::NOSPC,
                Errno::NOMEM,
                Errno::DQUOT,
            ]
            .iter()
            .any(|e| e.raw_os_error() == raw)
            {
                return Self::ResourceUnavailable;
            }
            if raw == Errno::LOOP.raw_os_error() {
                return Self::InvalidEnvironment;
            }
        }
        match error.kind() {
            io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => Self::PathUnavailable,
            io::ErrorKind::InvalidInput
            | io::ErrorKind::InvalidData
            | io::ErrorKind::AlreadyExists => Self::InvalidEnvironment,
            io::ErrorKind::OutOfMemory
            | io::ErrorKind::StorageFull
            | io::ErrorKind::QuotaExceeded => Self::ResourceUnavailable,
            _ => Self::Other,
        }
    }
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::PathUnavailable => {
                "IPC initialization: temporary location unavailable; restore the temporary filesystem before retrying."
            }
            Self::PermissionDenied => {
                "IPC initialization: permission denied; check execution-owner access to the temporary location."
            }
            Self::ResourceUnavailable => {
                "IPC initialization: system resources unavailable; free disk or process resources before retrying."
            }
            Self::InvalidEnvironment => {
                "IPC initialization: unsafe or unsupported temporary location; check its ownership and filesystem configuration."
            }
            Self::Other => {
                "IPC initialization: operating-system setup failed before the user script started; inspect host resource availability."
            }
        }
    }
}

pub(crate) fn safe_detail(message: &str) -> Option<&str> {
    [
        Reason::PathUnavailable,
        Reason::PermissionDenied,
        Reason::ResourceUnavailable,
        Reason::InvalidEnvironment,
        Reason::Other,
    ]
    .into_iter()
    .any(|r| r.message() == message)
    .then_some(message)
}

#[derive(Debug)]
pub(super) struct Setup {
    directory: PathBuf,
    pub(super) status: String,
    #[cfg(unix)]
    pub(super) helper: PrivateDirectory,
}

impl Setup {
    pub(super) fn new(execution_root: &Path) -> io::Result<Self> {
        let directory = execution_root.join("loader-startup");
        let status = directory
            .join("failure")
            .to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "startup path"))?
            .to_owned();
        #[cfg(unix)]
        let helper = private_directory("pactrun-shell-", "helper.sock")?;
        let mut value = Self {
            directory,
            status,
            #[cfg(unix)]
            helper,
        };
        #[allow(unused_mut)]
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        if let Err(error) = builder.create(&value.directory) {
            // Never adopt or remove an existing caller object.
            value.directory = PathBuf::new();
            value.status.clear();
            return Err(error);
        }
        Ok(value)
    }

    pub(super) fn failure(&self) -> Option<super::FailureKind> {
        let path = Path::new(&self.status);
        let meta = fs::symlink_metadata(path).ok()?;
        if !meta.is_file() || meta.len() > 256 {
            return None;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
            if meta.uid() != rustix::process::geteuid().as_raw()
                || meta.nlink() != 1
                || meta.mode() & 0o077 != 0
            {
                return None;
            }
            let mut file = fs::OpenOptions::new()
                .read(true)
                .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
                .open(path)
                .ok()?;
            let opened = file.metadata().ok()?;
            if opened.dev() != meta.dev() || opened.ino() != meta.ino() {
                return None;
            }
            let mut bytes = Vec::new();
            Read::by_ref(&mut file)
                .take(257)
                .read_to_end(&mut bytes)
                .ok()?;
            decode(&bytes)
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
            if meta.file_attributes() & 0x400 != 0 {
                return None;
            }
            let file = fs::OpenOptions::new()
                .read(true)
                .custom_flags(0x0020_0000)
                .open(path)
                .ok()?;
            if file.metadata().ok()?.file_attributes() & 0x400 != 0 {
                return None;
            }
            let mut bytes = Vec::new();
            file.take(257).read_to_end(&mut bytes).ok()?;
            decode(&bytes)
        }
    }
}

impl Drop for Setup {
    fn drop(&mut self) {
        if !self.directory.as_os_str().is_empty() {
            let _ = fs::remove_file(self.directory.join("failure.pending"));
            let _ = fs::remove_file(self.directory.join("failure"));
            let _ = fs::remove_dir(&self.directory);
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    version: u8,
    script_launch: bool,
    reason: Reason,
}

fn decode(bytes: &[u8]) -> Option<super::FailureKind> {
    if bytes.len() > 256 {
        return None;
    }
    let evidence: Evidence = serde_json::from_slice(bytes).ok()?;
    if evidence.version != 1 {
        return None;
    }
    Some(if evidence.script_launch {
        super::FailureKind::Launch
    } else {
        super::FailureKind::LoaderInitialization(evidence.reason)
    })
}

pub(super) fn publish(error: &io::Error, script_launch: bool) {
    let Some(path) = std::env::var_os(STATUS_ENV).map(PathBuf::from) else {
        return;
    };
    if !path.is_absolute() || path.file_name() != Some(std::ffi::OsStr::new("failure")) {
        return;
    }
    let result = (|| -> io::Result<()> {
        let pending = path.with_extension("pending");
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&pending)?;
        let bytes = serde_json::to_vec(&Evidence {
            version: 1,
            script_launch,
            reason: Reason::from_io(error),
        })?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(pending, path)
    })();
    let _ = result; // Absence is never interpreted as successful initialization.
}

#[cfg(unix)]
fn selected_root(root: &Path, prefix: &str, leaf: &str) -> io::Result<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    // Linux sun_path has 108 bytes including the trailing NUL. Other supported
    // Unix implementations use the conservative 104-byte sockaddr layout.
    let capacity = if cfg!(target_os = "linux") { 107 } else { 103 };
    let fits = |p: &Path| {
        p.is_absolute()
            && p.to_str().is_some()
            && p.join(format!("{prefix}{}", "0".repeat(32)))
                .join(leaf)
                .as_os_str()
                .as_bytes()
                .len()
                <= capacity
    };
    if fits(root) {
        return Ok(root.to_owned());
    }
    #[cfg(target_os = "linux")]
    if fits(Path::new("/tmp")) {
        return Ok(PathBuf::from("/tmp"));
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "IPC endpoint cannot be represented",
    ))
}

#[cfg(unix)]
pub(super) fn private_directory(prefix: &str, leaf: &str) -> io::Result<PrivateDirectory> {
    private_directory_under(&std::env::temp_dir(), prefix, leaf)
}

#[cfg(unix)]
fn private_directory_under(
    preferred: &Path,
    prefix: &str,
    leaf: &str,
) -> io::Result<PrivateDirectory> {
    use std::os::unix::fs::MetadataExt;
    let root = selected_root(preferred, prefix, leaf)?;
    let parent = open_directory(&root)?;
    let before = parent.metadata()?;
    let owner = before.uid();
    let mode = before.mode();
    let private = owner == rustix::process::geteuid().as_raw() && mode & 0o022 == 0;
    let system = owner == 0 && mode & 0o1000 != 0 && mode & 0o002 != 0;
    if !before.is_dir() || !(private || system) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsafe IPC root",
        ));
    }
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    let name = std::ffi::OsString::from(format!("{prefix}{}", hex::encode(random)));
    rustix::fs::mkdirat(&parent, &name, rustix::fs::Mode::RWXU)?;
    let opened = rustix::fs::openat(
        &parent,
        &name,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    );
    let fd = match opened {
        Ok(fd) => fd,
        Err(error) => {
            // mkdir succeeded, but no directory handle was available. Only
            // remove this fresh empty name, never traverse or recursively delete.
            let _ = rustix::fs::unlinkat(&parent, &name, rustix::fs::AtFlags::REMOVEDIR);
            return Err(error.into());
        }
    };
    let directory = PrivateDirectory {
        path: root.join(&name),
        parent,
        file: fs::File::from(fd),
        name,
    };
    let after = fs::symlink_metadata(&root)?;
    if before.dev() != after.dev() || before.ino() != after.ino() || !after.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "IPC root replaced",
        ));
    }
    directory.qualify()?;
    Ok(directory)
}

#[cfg(unix)]
fn open_directory(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(
            (rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC)
                .bits() as i32,
        )
        .open(path)
}

#[cfg(unix)]
#[derive(Debug)]
pub(super) struct PrivateDirectory {
    pub(super) path: PathBuf,
    parent: fs::File,
    file: fs::File,
    name: std::ffi::OsString,
}

#[cfg(unix)]
impl PrivateDirectory {
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        use std::os::unix::fs::MetadataExt;
        let parent_path = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "IPC parent"))?;
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "IPC name"))?
            .to_owned();
        let parent = open_directory(parent_path)?;
        let file = open_directory(path)?;
        let meta = file.metadata()?;
        if meta.uid() != rustix::process::geteuid().as_raw() || meta.mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unqualified IPC directory",
            ));
        }
        let value = Self {
            path: path.to_owned(),
            parent,
            file,
            name,
        };
        value.qualify()?;
        Ok(value)
    }
    pub(super) fn qualify(&self) -> io::Result<()> {
        use std::os::unix::fs::MetadataExt;
        let path = fs::symlink_metadata(&self.path)?;
        let pinned = self.file.metadata()?;
        if !path.is_dir()
            || path.dev() != pinned.dev()
            || path.ino() != pinned.ino()
            || pinned.uid() != rustix::process::geteuid().as_raw()
            || pinned.mode() & 0o077 != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "IPC directory changed",
            ));
        }
        Ok(())
    }
}

#[cfg(unix)]
impl Drop for PrivateDirectory {
    fn drop(&mut self) {
        use rustix::fs::{AtFlags, FileType};
        use std::os::unix::fs::MetadataExt;
        // Unlink only sockets through the original pinned directory; a namespace
        // replacement cannot redirect cleanup into a foreign directory.
        for leaf in ["hook.sock", "helper.sock"] {
            if let Ok(stat) = rustix::fs::statat(&self.file, leaf, AtFlags::SYMLINK_NOFOLLOW)
                && FileType::from_raw_mode(stat.st_mode) == FileType::Socket
            {
                let _ = rustix::fs::unlinkat(&self.file, leaf, AtFlags::empty());
            }
        }
        if let (Ok(current), Ok(pinned)) = (
            rustix::fs::statat(&self.parent, &self.name, AtFlags::SYMLINK_NOFOLLOW),
            self.file.metadata(),
        ) && current.st_dev == pinned.dev()
            && current.st_ino == pinned.ino()
        {
            let _ = rustix::fs::unlinkat(&self.parent, &self.name, AtFlags::REMOVEDIR);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0577
    // Verifies: PR-REQ-0361
    #[cfg(target_os = "linux")]
    #[test]
    fn ipc_roots_refuse_unsafe_access_and_cleanup_never_follows_replacement() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = tempfile::Builder::new()
            .prefix("ipc-")
            .tempdir_in("/tmp")
            .unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let make = || private_directory_under(root.path(), "pactrun-", "hook.sock");
        let dir = make().unwrap();
        let socket = std::os::unix::net::UnixListener::bind(dir.path.join("hook.sock")).unwrap();
        let moved = root.path().join("moved");
        let original = dir.path.clone();
        let foreign = root.path().join("foreign");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("hook.sock"), b"preserve").unwrap();
        fs::rename(&original, &moved).unwrap();
        symlink(&foreign, &original).unwrap();
        assert!(dir.qualify().is_err());
        drop(socket);
        drop(dir);
        assert_eq!(fs::read(foreign.join("hook.sock")).unwrap(), b"preserve");
        assert!(!moved.join("hook.sock").exists());
        fs::remove_file(&original).unwrap();
        fs::remove_dir(&moved).unwrap();
        let missing = root.path().join("absent");
        assert_eq!(
            private_directory_under(&missing, "pactrun-", "hook.sock")
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o777)).unwrap();
        assert!(make().is_err());
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o500)).unwrap();
        if !rustix::process::geteuid().is_root() {
            assert_eq!(make().unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        }
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let path = root.path();
                scope.spawn(move || {
                    let a = private_directory_under(path, "pactrun-", "hook.sock").unwrap();
                    let b = private_directory_under(path, "pactrun-", "hook.sock").unwrap();
                    assert_ne!(a.path, b.path);
                });
            }
        });
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1); // Only foreign fixture remains.
    }
    // Test-ID: PR-TEST-0571
    // Verifies: PR-REQ-0362
    #[test]
    fn private_loader_evidence_is_bounded_and_not_success_authority() {
        assert!(decode(b"{}").is_none());
        assert!(decode(&vec![b'x'; 257]).is_none());
        let bytes = serde_json::to_vec(&Evidence {
            version: 1,
            script_launch: false,
            reason: Reason::PermissionDenied,
        })
        .unwrap();
        assert_eq!(
            decode(&bytes),
            Some(super::super::FailureKind::LoaderInitialization(
                Reason::PermissionDenied
            ))
        );
        let failure = decode(&bytes).unwrap();
        let (owner, code, step, _) = failure.record();
        assert_eq!(
            (owner, code),
            ("execution", "shell_loader_initialization_failed")
        );
        assert_eq!(step, crate::domain::ActionPlanStep::EstablishSession);
        assert!(safe_detail("secret arbitrary text").is_none());
    }
    // Test-ID: PR-TEST-0570
    // Verifies: PR-REQ-0361
    #[cfg(target_os = "linux")]
    #[test]
    fn endpoint_selection_accounts_for_helper_and_native_bytes() {
        use std::os::unix::ffi::OsStringExt;
        for length in [32, 47, 48, 49, 55, 56, 57, 68, 96] {
            let root = PathBuf::from(format!("/{}", "x".repeat(length - 1)));
            let chosen = selected_root(&root, "pactrun-shell-", "helper.sock").unwrap();
            assert_eq!(chosen == root, length <= 48);
        }
        assert_eq!(
            selected_root(
                Path::new(&std::ffi::OsString::from_vec(vec![b'/', 255])),
                "pactrun-",
                "hook.sock"
            )
            .unwrap(),
            Path::new("/tmp")
        );
        assert_eq!(
            selected_root(
                Path::new(&format!("/{}", "\u{00e9}".repeat(30))),
                "pactrun-",
                "hook.sock"
            )
            .unwrap(),
            Path::new("/tmp")
        );
    }
}
