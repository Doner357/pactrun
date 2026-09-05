//! Managed-data lifecycle and operation-local staging ownership.

use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::{
    domain::{ExecutionOwnerSession, MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1, Sha256Digest},
    persistence::validate_supported_storage_root,
};

const BUFFER_BYTES: usize = 64 * 1024;
const STAGING_DIRECTORY: &str = "staging";
const LEASE_NAME: &str = ".lease";

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
                Ok(()) => {
                    let lease_path = session.join(LEASE_NAME);
                    let lease = create_private_file(&lease_path)
                        .map_err(|error| StagingError::io("create staging lease", error))?;
                    lease
                        .lock()
                        .map_err(|error| StagingError::io("lock staging session", error))?;
                    return Ok(Self {
                        root,
                        session,
                        lease: Some(lease),
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(StagingError::io("create staging session", error)),
            }
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

fn cleanup_stale_sessions(root: &Path) -> Result<(), StagingError> {
    for entry in fs::read_dir(root).map_err(|error| StagingError::io("scan staging root", error))? {
        let entry = entry.map_err(|error| StagingError::io("read staging entry", error))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !is_session_name(name) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| StagingError::io("inspect staging session", error))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        let lease_path = entry.path().join(LEASE_NAME);
        let lease = match OpenOptions::new().read(true).write(true).open(&lease_path) {
            Ok(lease) => lease,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(StagingError::io("open stale staging lease", error)),
        };
        match lease.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => continue,
            Err(std::fs::TryLockError::Error(error)) => {
                return Err(StagingError::io("lock stale staging session", error));
            }
        }
        remove_session_contents(&entry.path())
            .map_err(|error| StagingError::io("remove stale staging contents", error))?;
        drop(lease);
        match fs::remove_dir(entry.path()) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(StagingError::io("remove stale staging session", error)),
        }
    }
    Ok(())
}

fn remove_session_contents(session: &Path) -> io::Result<()> {
    for entry in fs::read_dir(session)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_file() && !metadata.file_type().is_symlink() {
            fs::remove_file(entry.path())?;
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
}
