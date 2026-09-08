//! Managed-data lifecycle and operation-local staging ownership.

use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use sha2::{Digest, Sha256};

use crate::{
    domain::{ExecutionOwnerSession, MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1, RunId, Sha256Digest},
    persistence::validate_supported_storage_root,
};

const BUFFER_BYTES: usize = 64 * 1024;
const STAGING_DIRECTORY: &str = "staging";
const LEASE_NAME: &str = ".lease";

#[cfg(test)]
static CLEANUP_FAILURES: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub(crate) struct StagingSession {
    root: PathBuf,
    session: PathBuf,
    lease: Option<File>,
}

#[derive(Debug)]
pub(crate) struct StagedFile {
    file: File,
    path: PathBuf,
    byte_len: u64,
}

#[derive(Debug)]
pub(crate) struct ExecutionDirectory {
    root: PathBuf,
}

impl ExecutionDirectory {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn create_directory(&self, relative: &Path) -> Result<PathBuf, StagingError> {
        let path = checked_descendant(&self.root, relative)?;
        create_private_directories(&self.root, relative)?;
        Ok(path)
    }

    pub(crate) fn create_file(&self, relative: &Path) -> Result<(PathBuf, File), StagingError> {
        let path = checked_descendant(&self.root, relative)?;
        if let Some(parent) = relative
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            create_private_directories(&self.root, parent)?;
        }
        let file = create_private_file(&path)
            .map_err(|error| StagingError::io("create execution file", error))?;
        Ok((path, file))
    }

    /// Removes only this Run's execution tree. The owner session, its lease,
    /// and operation-local staging files are outside this directory.
    pub(crate) fn cleanup(&self) -> Result<(), StagingError> {
        #[cfg(test)]
        if CLEANUP_FAILURES
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            return Err(StagingError::io(
                "clean execution workspace",
                io::Error::other("injected cleanup failure"),
            ));
        }
        if !self.root.exists() {
            return Ok(());
        }
        let metadata = fs::symlink_metadata(&self.root)
            .map_err(|error| StagingError::io("inspect execution cleanup root", error))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(StagingError::Unsupported(
                "execution cleanup root is not an owned directory".to_owned(),
            ));
        }
        remove_session_contents(&self.root)
            .map_err(|error| StagingError::io("clean execution workspace", error))?;
        fs::remove_dir(&self.root)
            .map_err(|error| StagingError::io("remove execution workspace", error))
    }
}

#[cfg(test)]
pub(crate) fn fail_next_execution_cleanup() {
    CLEANUP_FAILURES.fetch_add(1, Ordering::AcqRel);
}

impl StagedFile {
    pub(crate) fn byte_len(&self) -> u64 {
        self.byte_len
    }

    pub(crate) fn try_clone_reader(&self) -> Result<File, StagingError> {
        let mut file = self
            .file
            .try_clone()
            .map_err(|error| StagingError::io("clone staging handle", error))?;
        file.seek(SeekFrom::Start(0))
            .map_err(|error| StagingError::io("rewind cloned staging handle", error))?;
        Ok(file)
    }

    pub(crate) fn writer(&mut self) -> &mut File {
        &mut self.file
    }

    pub(crate) fn finish_managed_output(&mut self) -> Result<(), StagingError> {
        self.file
            .sync_all()
            .map_err(|error| StagingError::io("sync Managed Input output staging", error))?;
        self.byte_len = self
            .file
            .metadata()
            .map_err(|error| StagingError::io("inspect Managed Input output staging", error))?
            .len();
        if self.byte_len > MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1 {
            return Err(StagingError::ManagedInputTooLarge);
        }
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|error| StagingError::io("rewind Managed Input output staging", error))?;
        Ok(())
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug)]
pub(crate) struct StagedRuntimeSource {
    pub(crate) blob_digest: Sha256Digest,
    pub(crate) bytes: StagedFile,
}

#[derive(Debug)]
pub(crate) enum StagingError {
    Io {
        operation: &'static str,
        source: io::Error,
    },
    Unsupported(String),
    ManagedInputTooLarge,
    LengthOverflow,
}

impl StagingError {
    fn io(operation: &'static str, source: io::Error) -> Self {
        Self::Io { operation, source }
    }
}

impl fmt::Display for StagingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::Unsupported(message) => formatter.write_str(message),
            Self::ManagedInputTooLarge => write!(
                formatter,
                "Managed Input exceeds the {} byte M2 limit",
                MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1
            ),
            Self::LengthOverflow => formatter.write_str("staged byte length exceeds u64"),
        }
    }
}

impl std::error::Error for StagingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl StagingSession {
    pub(crate) fn open(storage_root: &Path) -> Result<Self, StagingError> {
        let staging_root = storage_root.join(STAGING_DIRECTORY);
        let root = validate_supported_storage_root(&staging_root)
            .map_err(|error| StagingError::Unsupported(error.to_string()))?;
        cleanup_stale_sessions(&root)?;
        for _ in 0..32 {
            let session = root.join(format!("session-{}", random_hex()?));
            match create_private_directory(&session) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(StagingError::io("create staging session", error)),
            }
            // Another process's stale-session scan may observe this directory
            // before its lease is locked and treat it as abandoned. Creation
            // therefore verifies that the locked lease and its directory still
            // exist and otherwise retries with a fresh name.
            let lease_path = session.join(LEASE_NAME);
            let lease = match create_private_file(&lease_path) {
                Ok(lease) => lease,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(StagingError::io("create staging lease", error)),
            };
            lease
                .lock()
                .map_err(|error| StagingError::io("lock staging session", error))?;
            if fs::metadata(&lease_path).is_err() || !session.is_dir() {
                let _ = File::unlock(&lease);
                drop(lease);
                let _ = fs::remove_dir(&session);
                continue;
            }
            return Ok(Self {
                root,
                session,
                lease: Some(lease),
            });
        }
        Err(StagingError::Unsupported(
            "could not allocate a unique staging session".to_owned(),
        ))
    }

    /// The exact session directory name; it is the execution owner identity
    /// of every Run this process accepts.
    pub(crate) fn owner(&self) -> ExecutionOwnerSession {
        let name = self
            .session
            .file_name()
            .and_then(|name| name.to_str())
            .expect("staging session names are generated ASCII");
        ExecutionOwnerSession::parse(name).expect("staging session names are valid owners")
    }

    pub(crate) fn create_execution_directory(
        &self,
        run: RunId,
    ) -> Result<ExecutionDirectory, StagingError> {
        let root = self.session.join(format!("execution-{run}"));
        create_private_directory(&root)
            .map_err(|error| StagingError::io("create execution directory", error))?;
        Ok(ExecutionDirectory { root })
    }

    pub(crate) fn stage_managed_input(
        &self,
        source: &mut impl Read,
    ) -> Result<StagedFile, StagingError> {
        self.stage(source, Some(MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1))
            .map(|(bytes, _)| bytes)
    }

    pub(crate) fn stage_runtime_source(
        &self,
        source: &mut impl Read,
    ) -> Result<StagedRuntimeSource, StagingError> {
        let (bytes, digest) = self.stage(source, None)?;
        Ok(StagedRuntimeSource {
            blob_digest: digest.expect("runtime staging always hashes bytes"),
            bytes,
        })
    }

    pub(crate) fn create_managed_output_stage(&self) -> Result<StagedFile, StagingError> {
        let (path, file) = self.create_operation_file()?;
        Ok(StagedFile {
            file,
            path,
            byte_len: 0,
        })
    }

    /// Copies an owner-authorized Action output into operation-local staging.
    /// The source is checked as a regular, non-link file before and after it
    /// is opened; the destination is always a newly-created private file.
    pub(crate) fn stage_action_output(&self, source: &Path) -> Result<StagedFile, StagingError> {
        let metadata = fs::symlink_metadata(source)
            .map_err(|error| StagingError::io("inspect Action output", error))?;
        if !is_safe_regular_file(&metadata) {
            return Err(StagingError::Unsupported(
                "Action output is not a regular non-reparse file".to_owned(),
            ));
        }
        let mut input =
            File::open(source).map_err(|error| StagingError::io("open Action output", error))?;
        let opened = input
            .metadata()
            .map_err(|error| StagingError::io("inspect opened Action output", error))?;
        if !is_safe_regular_file(&opened) {
            return Err(StagingError::Unsupported(
                "Action output changed to a non-regular file".to_owned(),
            ));
        }
        let staged = self.stage(&mut input, Some(MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1));
        let after = fs::symlink_metadata(source)
            .map_err(|error| StagingError::io("verify Action output", error))?;
        if !is_safe_regular_file(&after) {
            return Err(StagingError::Unsupported(
                "Action output changed to a link or reparse point".to_owned(),
            ));
        }
        staged.map(|(bytes, _)| bytes)
    }

    fn stage(
        &self,
        source: &mut impl Read,
        maximum: Option<u64>,
    ) -> Result<(StagedFile, Option<Sha256Digest>), StagingError> {
        let (path, mut file) = self.create_operation_file()?;
        let result = (|| {
            let mut hasher = maximum.is_none().then(Sha256::new);
            let mut byte_len = 0_u64;
            let mut buffer = [0_u8; BUFFER_BYTES];
            loop {
                let read = source
                    .read(&mut buffer)
                    .map_err(|error| StagingError::io("read staging source", error))?;
                if read == 0 {
                    break;
                }
                byte_len = byte_len
                    .checked_add(u64::try_from(read).expect("buffer size fits u64"))
                    .ok_or(StagingError::LengthOverflow)?;
                if maximum.is_some_and(|maximum| byte_len > maximum) {
                    return Err(StagingError::ManagedInputTooLarge);
                }
                file.write_all(&buffer[..read])
                    .map_err(|error| StagingError::io("write staging file", error))?;
                if let Some(hasher) = &mut hasher {
                    hasher.update(&buffer[..read]);
                }
            }
            file.sync_all()
                .map_err(|error| StagingError::io("sync staging file", error))?;
            file.seek(SeekFrom::Start(0))
                .map_err(|error| StagingError::io("rewind staging file", error))?;
            let digest = hasher.map(|hasher| Sha256Digest::from_bytes(hasher.finalize().into()));
            Ok((
                StagedFile {
                    file,
                    path: path.clone(),
                    byte_len,
                },
                digest,
            ))
        })();
        if result.is_err() {
            let _ = fs::remove_file(path);
        }
        result
    }

    fn create_operation_file(&self) -> Result<(PathBuf, File), StagingError> {
        for _ in 0..32 {
            let path = self.session.join(format!("op-{}", random_hex()?));
            match create_private_file(&path) {
                Ok(file) => return Ok((path, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(StagingError::io("create staging file", error)),
            }
        }
        Err(StagingError::Unsupported(
            "could not allocate a unique staging file".to_owned(),
        ))
    }
}

impl StagingSession {
    /// Simulates confirmed owner loss: the lease is released as the operating
    /// system would on process death, while the session directory and every
    /// execution tree beneath it are left behind for stale-session cleanup.
    #[cfg(test)]
    pub(crate) fn abandon(mut self) {
        if let Some(lease) = self.lease.take() {
            let _ = File::unlock(&lease);
            drop(lease);
        }
        std::mem::forget(self);
    }
}

impl Drop for StagingSession {
    fn drop(&mut self) {
        if let Some(lease) = self.lease.take() {
            let _ = File::unlock(&lease);
            drop(lease);
        }
        let _ = remove_session_contents(&self.session);
        let _ = fs::remove_dir(&self.session);
        let _ = &self.root;
    }
}

/// Observes whether the recorded owner session still holds its lease.
///
/// Only the lock observation decides: a missing session or an acquirable
/// lease is loss, a held lease is liveness. The probe releases any lock it
/// acquires immediately and never removes the session.
#[allow(dead_code)]
pub(crate) fn session_is_live(
    storage_root: &Path,
    owner: &ExecutionOwnerSession,
) -> Result<bool, StagingError> {
    let lease_path = storage_root
        .join(STAGING_DIRECTORY)
        .join(owner.as_str())
        .join(LEASE_NAME);
    let lease = match OpenOptions::new().read(true).write(true).open(&lease_path) {
        Ok(lease) => lease,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(StagingError::io("open execution owner lease", error)),
    };
    match lease.try_lock() {
        Ok(()) => {
            let _ = File::unlock(&lease);
            Ok(false)
        }
        Err(std::fs::TryLockError::WouldBlock) => Ok(true),
        Err(std::fs::TryLockError::Error(error)) => {
            Err(StagingError::io("probe execution owner lease", error))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SessionOwnerProbe {
    Live,
    ConfirmedLoss,
    Unknown,
}

/// Probes the recorded owner without using PID, timestamps, or host identity.
/// A missing lease is inconclusive while its session directory still exists.
pub(crate) fn probe_session_owner(
    storage_root: &Path,
    owner: &ExecutionOwnerSession,
) -> SessionOwnerProbe {
    let session = storage_root.join(STAGING_DIRECTORY).join(owner.as_str());
    let metadata = match fs::symlink_metadata(&session) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return SessionOwnerProbe::ConfirmedLoss;
        }
        Err(_) => return SessionOwnerProbe::Unknown,
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return SessionOwnerProbe::Unknown;
    }
    let lease_path = session.join(LEASE_NAME);
    let lease = match OpenOptions::new().read(true).write(true).open(&lease_path) {
        Ok(lease) => lease,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return if session.exists() {
                SessionOwnerProbe::Unknown
            } else {
                SessionOwnerProbe::ConfirmedLoss
            };
        }
        Err(_) => return SessionOwnerProbe::Unknown,
    };
    match lease.try_lock() {
        Ok(()) => {
            let _ = File::unlock(&lease);
            SessionOwnerProbe::ConfirmedLoss
        }
        Err(std::fs::TryLockError::WouldBlock) => SessionOwnerProbe::Live,
        Err(std::fs::TryLockError::Error(_)) => SessionOwnerProbe::Unknown,
    }
}

/// Best-effort cleanup after a confirmed owner loss. It is intentionally not
/// used for an inconclusive lease probe.
pub(crate) fn cleanup_lost_session(
    storage_root: &Path,
    owner: &ExecutionOwnerSession,
) -> Result<(), StagingError> {
    let session = storage_root.join(STAGING_DIRECTORY).join(owner.as_str());
    cleanup_session_entry(&session)
        .map_err(|error| StagingError::io("clean abandoned staging session", error))
}

/// Removes abandoned sessions whose lease is no longer held. The scan is
/// housekeeping: a session that appears, locks its lease, or vanishes while
/// the scan runs belongs to a concurrent live process, so per-entry failures
/// are skipped rather than failing the caller's own session creation.
fn cleanup_stale_sessions(root: &Path) -> Result<(), StagingError> {
    for entry in fs::read_dir(root).map_err(|error| StagingError::io("scan staging root", error))? {
        let entry = entry.map_err(|error| StagingError::io("read staging entry", error))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !is_session_name(name) {
            continue;
        }
        let _ = cleanup_session_entry(&entry.path());
    }
    Ok(())
}

fn cleanup_session_entry(session: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(session)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(());
    }
    let lease_path = session.join(LEASE_NAME);
    let lease = match OpenOptions::new().read(true).write(true).open(&lease_path) {
        Ok(lease) => lease,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // A session directory without its lease is inconclusive. It may
            // still be in the middle of creation, so do not remove it or
            // infer owner loss from this observation.
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    match lease.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Ok(()),
        Err(std::fs::TryLockError::Error(error)) => return Err(error),
    }
    remove_session_contents(session)?;
    drop(lease);
    match fs::remove_dir(session) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn remove_session_contents(session: &Path) -> io::Result<()> {
    for entry in fs::read_dir(session)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            if fs::remove_file(entry.path()).is_err() {
                fs::remove_dir(entry.path())?;
            }
        } else if metadata.is_dir() {
            remove_session_contents(&entry.path())?;
            fs::remove_dir(entry.path())?;
        } else if metadata.is_file() {
            remove_file_force(&entry.path())?;
        }
    }
    Ok(())
}

/// Materialized binding files are read-only; on Windows that attribute also
/// refuses deletion until it is cleared.
fn remove_file_force(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            let mut permissions = fs::metadata(path)?.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            fs::set_permissions(path, permissions)?;
            fs::remove_file(path)
        }
        Err(error) => Err(error),
    }
}

fn is_safe_regular_file(metadata: &fs::Metadata) -> bool {
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return false;
        }
    }
    true
}

fn checked_descendant(root: &Path, relative: &Path) -> Result<PathBuf, StagingError> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(StagingError::Unsupported(
            "execution path must contain only relative normal components".to_owned(),
        ));
    }
    Ok(root.join(relative))
}

fn create_private_directories(root: &Path, relative: &Path) -> Result<(), StagingError> {
    let _ = checked_descendant(root, relative)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            unreachable!("checked execution path contains only normal components")
        };
        current.push(component);
        match create_private_directory(&current) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && current.is_dir() => {}
            Err(error) => return Err(StagingError::io("create execution subdirectory", error)),
        }
    }
    Ok(())
}

fn is_session_name(name: &str) -> bool {
    ExecutionOwnerSession::is_session_name(name)
}

fn random_hex() -> Result<String, StagingError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| StagingError::Unsupported(format!("CSPRNG failed: {error}")))?;
    Ok(hex::encode(bytes))
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
}

fn create_private_directory(path: &Path) -> io::Result<()> {
    fs::create_dir(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Cursor, path::PathBuf};

    use tempfile::TempDir;

    use super::*;
    use crate::{
        application::{InputAcquisition, PactrunApplication},
        domain::{InputIdentity, InstanceName, RevisionMetadataMutationBatch},
    };

    struct PanicReader;

    impl Read for PanicReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            panic!("duplicate acquisition must be rejected before reading")
        }
    }

    fn root() -> (TempDir, PathBuf) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m2-staging-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("staging-")
            .tempdir_in(parent)
            .unwrap();
        let root = temporary.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join(STAGING_DIRECTORY)).unwrap();
        (temporary, root)
    }

    // Test-ID: PR-TEST-0074
    // Verifies: PR-REQ-0265, PR-REQ-0268
    #[test]
    fn managed_and_runtime_staging_have_distinct_size_and_identity_policies() {
        let (temporary, root) = root();
        let session = StagingSession::open(&root).unwrap();
        let managed = session
            .stage_managed_input(&mut Cursor::new(b"opaque"))
            .unwrap();
        assert_eq!(managed.byte_len(), 6);
        let runtime = session
            .stage_runtime_source(&mut Cursor::new(b"opaque"))
            .unwrap();
        assert_eq!(runtime.bytes.byte_len(), 6);
        assert_eq!(
            runtime.blob_digest,
            Sha256Digest::from_bytes(Sha256::digest(b"opaque").into())
        );
        assert!(matches!(
            session.stage(&mut Cursor::new(b"too-large"), Some(3)),
            Err(StagingError::ManagedInputTooLarge)
        ));

        let live_path = session.session.clone();
        let second = StagingSession::open(&root).unwrap();
        assert!(live_path.is_dir());
        drop(second);

        let stale = root
            .join(STAGING_DIRECTORY)
            .join("session-00000000000000000000000000000000");
        create_private_directory(&stale).unwrap();
        drop(create_private_file(&stale.join(LEASE_NAME)).unwrap());
        fs::write(
            stale.join("op-00000000000000000000000000000000"),
            b"residue",
        )
        .unwrap();
        let cleanup = StagingSession::open(&root).unwrap();
        assert!(!stale.exists());
        drop(cleanup);

        let source = temporary.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(
            source.join("pactrun.yaml"),
            r#"source_format: 1
package_id: 00000000000000000000000000000031
revision:
  inputs:
    - id: config
  actions: []
  migrations: []
runtime_content:
  files: []
"#,
        )
        .unwrap();
        fs::create_dir(root.join("database")).unwrap();
        fs::create_dir(root.join("runtime-content")).unwrap();
        let application = PactrunApplication::open(&root).unwrap();
        let empty_metadata = RevisionMetadataMutationBatch::new(Vec::new()).unwrap();
        let installed = application
            .install_pack_source(&source, &empty_metadata)
            .unwrap();
        let config = InputIdentity::parse("config").unwrap();
        assert!(
            application
                .create_instance(
                    InstanceName::parse("duplicate-acquisition").unwrap(),
                    installed.revision.clone(),
                    vec![
                        InputAcquisition {
                            input_id: config.clone(),
                            source: Box::new(PanicReader),
                        },
                        InputAcquisition {
                            input_id: config.clone(),
                            source: Box::new(PanicReader),
                        },
                    ],
                )
                .is_err()
        );
        assert!(application.list_instances().unwrap().is_empty());

        let created = application
            .create_instance(
                InstanceName::parse("atomic-instance").unwrap(),
                installed.revision.clone(),
                vec![InputAcquisition {
                    input_id: config.clone(),
                    source: Box::new(Cursor::new(b"committed")),
                }],
            )
            .unwrap();
        assert!(
            application
                .create_instance(
                    InstanceName::parse("atomic-instance").unwrap(),
                    installed.revision,
                    vec![InputAcquisition {
                        input_id: config.clone(),
                        source: Box::new(Cursor::new(b"must-rollback")),
                    }],
                )
                .is_err()
        );
        assert_eq!(application.list_instances().unwrap().len(), 1);
        let observation = application
            .export_input(created.id, &config, false)
            .unwrap();
        let mut committed = Vec::new();
        observation
            .bytes
            .try_clone_reader()
            .unwrap()
            .read_to_end(&mut committed)
            .unwrap();
        assert_eq!(committed, b"committed");
    }

    // Test-ID: PR-TEST-0111
    // Verifies: PR-REQ-0060, PR-REQ-0277
    #[test]
    fn missing_lease_with_existing_session_is_inconclusive_and_not_cleaned() {
        let (_temporary, root) = root();
        let owner = ExecutionOwnerSession::parse(format!("session-{}", "1".repeat(32))).unwrap();
        let session = root.join(STAGING_DIRECTORY).join(owner.as_str());
        fs::create_dir(&session).unwrap();
        fs::write(session.join("residue"), b"owner may still be creating").unwrap();

        assert_eq!(
            probe_session_owner(&root, &owner),
            SessionOwnerProbe::Unknown
        );
        let opened = StagingSession::open(&root).unwrap();
        assert!(session.is_dir());
        assert!(session.join("residue").is_file());
        drop(opened);
        assert!(session.is_dir());
    }
}
