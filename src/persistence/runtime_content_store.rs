//! Crate-private immutable runtime-content blob storage.
//!
//! The store is deliberately physical: it knows only SHA-256 blob identities
//! and opaque bytes. Runtime paths, ContentIds, and executable roles remain in
//! the RevisionCoreFormatV1 semantic closure.

use std::{
    collections::BTreeSet,
    fmt,
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use sha2::{Digest, Sha256};

use crate::domain::{RuntimeContentClosureIdentityV1, Sha256Digest};

#[cfg(target_os = "linux")]
#[path = "runtime_content_store/linux.rs"]
mod platform;
#[cfg(windows)]
#[path = "runtime_content_store/windows.rs"]
mod platform;

#[cfg(not(any(target_os = "linux", windows)))]
compile_error!("M1-B runtime-content storage currently supports Windows and Linux only");

const LOCK_NAME: &str = ".publish.lock";
const STREAM_BUFFER_BYTES: usize = 64 * 1024;
const STAGING_NAME_ATTEMPTS: usize = 32;

#[derive(Debug)]
pub(crate) struct RuntimeContentStore {
    root: PathBuf,
    root_file: File,
    lock_file: File,
    in_process_lock: Mutex<()>,
    instance: Arc<StoreInstanceMarker>,
    #[cfg(test)]
    observer: TestObserver,
}

#[derive(Debug)]
struct StoreInstanceMarker(u8);

#[derive(Clone, Debug)]
pub(crate) struct StoredRuntimeBlob {
    digest: Sha256Digest,
    byte_len: u64,
    store_instance: Arc<StoreInstanceMarker>,
}

impl StoredRuntimeBlob {
    pub(crate) fn digest(&self) -> &Sha256Digest {
        &self.digest
    }

    pub(crate) fn byte_len(&self) -> u64 {
        self.byte_len
    }
}

#[derive(Debug)]
pub(crate) struct VerifiedRuntimeBlob {
    digest: Sha256Digest,
    byte_len: u64,
    file: File,
}

impl VerifiedRuntimeBlob {
    pub(crate) fn digest(&self) -> &Sha256Digest {
        &self.digest
    }

    pub(crate) fn byte_len(&self) -> u64 {
        self.byte_len
    }
}

impl Read for VerifiedRuntimeBlob {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.file.read(buffer)
    }
}

impl Seek for VerifiedRuntimeBlob {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.file.seek(position)
    }
}

#[derive(Debug)]
pub(crate) enum RuntimeContentStoreError {
    Io {
        operation: &'static str,
        source: io::Error,
    },
    UnsupportedStorageProfile(String),
    UnacceptableStoreEntry {
        operation: &'static str,
        source: io::Error,
    },
    PersistenceBarrier {
        operation: &'static str,
        source: io::Error,
    },
    Lock {
        operation: &'static str,
        source: io::Error,
    },
    RandomStagingName(getrandom::Error),
    IncomingDigestMismatch {
        expected: Sha256Digest,
        actual: Sha256Digest,
    },
    MissingBlob(Sha256Digest),
    CorruptBlob {
        expected: Sha256Digest,
        actual: Sha256Digest,
    },
    LengthOverflow,
    LockPoisoned,
}

impl RuntimeContentStoreError {
    fn io(operation: &'static str, source: io::Error) -> Self {
        Self::Io { operation, source }
    }

    fn entry(operation: &'static str, source: io::Error) -> Self {
        Self::UnacceptableStoreEntry { operation, source }
    }

    fn persistence(operation: &'static str, source: io::Error) -> Self {
        Self::PersistenceBarrier { operation, source }
    }

    fn lock(operation: &'static str, source: io::Error) -> Self {
        Self::Lock { operation, source }
    }
}

impl fmt::Display for RuntimeContentStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::UnsupportedStorageProfile(message) => {
                write!(
                    formatter,
                    "unsupported runtime-content storage profile: {message}"
                )
            }
            Self::UnacceptableStoreEntry { operation, source } => {
                write!(formatter, "{operation}: {source}")
            }
            Self::PersistenceBarrier { operation, source } => {
                write!(formatter, "{operation}: {source}")
            }
            Self::Lock { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::RandomStagingName(source) => {
                write!(formatter, "generate staging name: {source}")
            }
            Self::IncomingDigestMismatch { expected, actual } => write!(
                formatter,
                "incoming runtime blob digest mismatch: expected {}, got {}",
                expected.as_str(),
                actual.as_str()
            ),
            Self::MissingBlob(digest) => {
                write!(formatter, "runtime blob {} is missing", digest.as_str())
            }
            Self::CorruptBlob { expected, actual } => write!(
                formatter,
                "runtime blob {} contains bytes hashing to {}",
                expected.as_str(),
                actual.as_str()
            ),
            Self::LengthOverflow => formatter.write_str("runtime blob length exceeds u64"),
            Self::LockPoisoned => formatter.write_str("runtime-content publication lock poisoned"),
        }
    }
}

impl std::error::Error for RuntimeContentStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. }
            | Self::UnacceptableStoreEntry { source, .. }
            | Self::PersistenceBarrier { source, .. }
            | Self::Lock { source, .. } => Some(source),
            Self::RandomStagingName(source) => Some(source),
            _ => None,
        }
    }
}

impl RuntimeContentStore {
    pub(crate) fn open(root: impl AsRef<Path>) -> Result<Self, RuntimeContentStoreError> {
        let (root, root_file) = open_supported_root(root.as_ref())?;

        #[cfg(windows)]
        let lock_file = platform::open_lock(&root)
            .map_err(|error| RuntimeContentStoreError::lock("open publication lock", error))?;
        #[cfg(target_os = "linux")]
        let lock_file = platform::open_lock(&root, &root_file)
            .map_err(|error| RuntimeContentStoreError::lock("open publication lock", error))?;

        Ok(Self {
            root,
            root_file,
            lock_file,
            in_process_lock: Mutex::new(()),
            instance: Arc::new(StoreInstanceMarker(0)),
            #[cfg(test)]
            observer: TestObserver::default(),
        })
    }

    pub(crate) fn put_verified<R: Read>(
        &self,
        expected: &Sha256Digest,
        source: &mut R,
    ) -> Result<StoredRuntimeBlob, RuntimeContentStoreError> {
        let (staging_name, mut staging) = self.create_staging()?;
        let staged = (|| {
            let (actual, byte_len) = copy_and_hash(source, &mut staging)?;
            self.fault(FaultPoint::AfterStageWrite)?;
            platform::sync_file(&staging).map_err(|error| {
                RuntimeContentStoreError::persistence("sync staging blob", error)
            })?;
            self.record(PersistenceEvent::StageFileSync);
            self.fault(FaultPoint::AfterStageSync)?;

            if &actual != expected {
                return Err(RuntimeContentStoreError::IncomingDigestMismatch {
                    expected: expected.clone(),
                    actual,
                });
            }
            self.fault(FaultPoint::AfterDigestVerification)?;
            Ok(byte_len)
        })();

        let byte_len = match staged {
            Ok(byte_len) => byte_len,
            Err(error) => {
                drop(staging);
                self.remove_staging_best_effort(&staging_name);
                return Err(error);
            }
        };

        let final_name = digest_basename(expected);
        let publication = self.with_publication_lock(|| {
            self.fault(FaultPoint::BeforePublication)?;
            match self.open_existing_durable(&final_name) {
                Ok(mut existing) => {
                    let existing_len = self.verify_open_file(expected, &mut existing)?;
                    platform::sync_file(&existing).map_err(|error| {
                        RuntimeContentStoreError::persistence("sync existing runtime blob", error)
                    })?;
                    self.record(PersistenceEvent::ExistingFileSync);
                    self.fault(FaultPoint::AfterPostPublicationFileSync)?;
                    self.sync_namespace()?;
                    self.fault(FaultPoint::AfterNamespaceSync)?;
                    Ok(StoredRuntimeBlob {
                        digest: expected.clone(),
                        byte_len: existing_len,
                        store_instance: Arc::clone(&self.instance),
                    })
                }
                Err(RuntimeContentStoreError::MissingBlob(_)) => {
                    match platform::publish_no_replace(
                        &self.root,
                        &self.root_file,
                        &staging_name,
                        &staging,
                        &final_name,
                    ) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                            let mut existing = self.open_existing_durable(&final_name)?;
                            let existing_len = self.verify_open_file(expected, &mut existing)?;
                            platform::sync_file(&existing).map_err(|error| {
                                RuntimeContentStoreError::persistence(
                                    "sync concurrently published runtime blob",
                                    error,
                                )
                            })?;
                            self.record(PersistenceEvent::ExistingFileSync);
                            self.sync_namespace()?;
                            return Ok(StoredRuntimeBlob {
                                digest: expected.clone(),
                                byte_len: existing_len,
                                store_instance: Arc::clone(&self.instance),
                            });
                        }
                        Err(error) => {
                            return Err(RuntimeContentStoreError::io(
                                "publish runtime blob without replacement",
                                error,
                            ));
                        }
                    }

                    self.record(PersistenceEvent::Published);
                    self.fault(FaultPoint::AfterPublication)?;
                    platform::sync_file(&staging).map_err(|error| {
                        RuntimeContentStoreError::persistence("sync published runtime blob", error)
                    })?;
                    self.record(PersistenceEvent::PublishedFileSync);
                    self.fault(FaultPoint::AfterPostPublicationFileSync)?;
                    self.sync_namespace()?;
                    self.fault(FaultPoint::AfterNamespaceSync)?;
                    Ok(StoredRuntimeBlob {
                        digest: expected.clone(),
                        byte_len,
                        store_instance: Arc::clone(&self.instance),
                    })
                }
                Err(error) => Err(error),
            }
        });

        drop(staging);
        self.remove_staging_best_effort(&staging_name);
        let stored = publication?;
        self.fault(FaultPoint::BeforeSuccess)?;
        Ok(stored)
    }

    pub(crate) fn open_verified(
        &self,
        digest: &Sha256Digest,
    ) -> Result<VerifiedRuntimeBlob, RuntimeContentStoreError> {
        let name = digest_basename(digest);
        let mut file = platform::open_existing_read(&self.root, &name, &self.root_file)
            .map_err(|error| self.classify_open_error(digest, "open runtime blob", error))?;
        let byte_len = self.verify_open_file(digest, &mut file)?;
        self.record(PersistenceEvent::VerifiedRead);
        file.seek(SeekFrom::Start(0))
            .map_err(|error| RuntimeContentStoreError::io("reset runtime blob cursor", error))?;
        Ok(VerifiedRuntimeBlob {
            digest: digest.clone(),
            byte_len,
            file,
        })
    }

    pub(crate) fn verify_runtime_content_available(
        &self,
        closure: &RuntimeContentClosureIdentityV1,
    ) -> Result<(), RuntimeContentStoreError> {
        let distinct: BTreeSet<_> = closure
            .files()
            .iter()
            .map(|file| file.blob_digest.clone())
            .collect();
        for digest in distinct {
            self.open_verified(&digest)?;
        }
        Ok(())
    }

    pub(super) fn owns_publication(&self, publication: &StoredRuntimeBlob) -> bool {
        Arc::ptr_eq(&self.instance, &publication.store_instance)
    }

    fn create_staging(&self) -> Result<(String, File), RuntimeContentStoreError> {
        for _ in 0..STAGING_NAME_ATTEMPTS {
            let mut random = [0_u8; 16];
            getrandom::fill(&mut random).map_err(RuntimeContentStoreError::RandomStagingName)?;
            let name = format!(".{}.tmp", hex::encode(random));
            match platform::create_staging(&self.root, &name, &self.root_file) {
                Ok(file) => return Ok((name, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(RuntimeContentStoreError::io(
                        "create runtime blob staging file",
                        error,
                    ));
                }
            }
        }
        Err(RuntimeContentStoreError::io(
            "create runtime blob staging file",
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "random staging-name collision limit reached",
            ),
        ))
    }

    fn open_existing_durable(&self, name: &str) -> Result<File, RuntimeContentStoreError> {
        platform::open_existing_durable(&self.root, name, &self.root_file).map_err(|error| {
            self.classify_open_error(
                &Sha256Digest::parse(format!("sha256:{name}"))
                    .expect("digest-derived final name is valid"),
                "open existing runtime blob for durability",
                error,
            )
        })
    }

    fn classify_open_error(
        &self,
        digest: &Sha256Digest,
        operation: &'static str,
        error: io::Error,
    ) -> RuntimeContentStoreError {
        if error.kind() == io::ErrorKind::NotFound {
            RuntimeContentStoreError::MissingBlob(digest.clone())
        } else {
            if error.kind() == io::ErrorKind::InvalidData {
                RuntimeContentStoreError::entry(operation, error)
            } else {
                RuntimeContentStoreError::io(operation, error)
            }
        }
    }

    fn verify_open_file(
        &self,
        expected: &Sha256Digest,
        file: &mut File,
    ) -> Result<u64, RuntimeContentStoreError> {
        file.seek(SeekFrom::Start(0))
            .map_err(|error| RuntimeContentStoreError::io("seek runtime blob", error))?;
        let (actual, byte_len) = hash_reader(file)?;
        if &actual != expected {
            return Err(RuntimeContentStoreError::CorruptBlob {
                expected: expected.clone(),
                actual,
            });
        }
        file.seek(SeekFrom::Start(0))
            .map_err(|error| RuntimeContentStoreError::io("reset runtime blob cursor", error))?;
        Ok(byte_len)
    }

    fn sync_namespace(&self) -> Result<(), RuntimeContentStoreError> {
        platform::sync_namespace(&self.root_file).map_err(|error| {
            RuntimeContentStoreError::persistence("sync store namespace", error)
        })?;
        self.record(PersistenceEvent::NamespaceSync);
        Ok(())
    }

    fn with_publication_lock<T>(
        &self,
        operation: impl FnOnce() -> Result<T, RuntimeContentStoreError>,
    ) -> Result<T, RuntimeContentStoreError> {
        let _process_guard = self
            .in_process_lock
            .lock()
            .map_err(|_| RuntimeContentStoreError::LockPoisoned)?;
        self.lock_file
            .lock()
            .map_err(|error| RuntimeContentStoreError::lock("acquire publication lock", error))?;
        let result = operation();
        let unlock = File::unlock(&self.lock_file)
            .map_err(|error| RuntimeContentStoreError::lock("release publication lock", error));
        match (result, unlock) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    fn remove_staging_best_effort(&self, name: &str) {
        let _ = platform::remove_staging(&self.root, name, &self.root_file);
    }

    #[cfg(not(test))]
    fn fault(&self, _point: FaultPoint) -> Result<(), RuntimeContentStoreError> {
        Ok(())
    }

    #[cfg(test)]
    fn fault(&self, point: FaultPoint) -> Result<(), RuntimeContentStoreError> {
        self.observer.fault(point)
    }

    #[cfg(not(test))]
    fn record(&self, _event: PersistenceEvent) {}

    #[cfg(test)]
    fn record(&self, event: PersistenceEvent) {
        self.observer.record(event);
    }

    #[cfg(test)]
    fn take_events(&self) -> Vec<PersistenceEvent> {
        std::mem::take(&mut *self.observer.events.lock().expect("test event mutex"))
    }
}

pub(crate) fn validate_supported_storage_root(
    root: &Path,
) -> Result<PathBuf, RuntimeContentStoreError> {
    open_supported_root(root).map(|(canonical, _)| canonical)
}

pub(crate) fn validate_existing_regular_entry(
    root: &Path,
    name: &str,
) -> Result<(), RuntimeContentStoreError> {
    let (canonical, root_file) = open_supported_root(root)?;
    match platform::open_existing_read(&canonical, name, &root_file) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => Err(
            RuntimeContentStoreError::entry("validate database entry", error),
        ),
        Err(error) => Err(RuntimeContentStoreError::io(
            "validate database entry",
            error,
        )),
    }
}

fn open_supported_root(requested_root: &Path) -> Result<(PathBuf, File), RuntimeContentStoreError> {
    let root_file = platform::open_root(requested_root).map_err(|error| {
        if error.kind() == io::ErrorKind::Unsupported {
            RuntimeContentStoreError::UnsupportedStorageProfile(error.to_string())
        } else if error.kind() == io::ErrorKind::InvalidData {
            RuntimeContentStoreError::entry("validate store root", error)
        } else {
            RuntimeContentStoreError::io("open supported store root", error)
        }
    })?;
    let root = requested_root
        .canonicalize()
        .map_err(|error| RuntimeContentStoreError::io("canonicalize trusted store root", error))?;
    Ok((root, root_file))
}

fn copy_and_hash(
    source: &mut impl Read,
    destination: &mut File,
) -> Result<(Sha256Digest, u64), RuntimeContentStoreError> {
    let mut hasher = Sha256::new();
    let mut byte_len = 0_u64;
    let mut buffer = [0_u8; STREAM_BUFFER_BYTES];
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|error| RuntimeContentStoreError::io("read incoming runtime blob", error))?;
        if read == 0 {
            break;
        }
        destination
            .write_all(&buffer[..read])
            .map_err(|error| RuntimeContentStoreError::io("write runtime blob staging", error))?;
        hasher.update(&buffer[..read]);
        byte_len = byte_len
            .checked_add(u64::try_from(read).expect("buffer length fits u64"))
            .ok_or(RuntimeContentStoreError::LengthOverflow)?;
    }
    Ok((digest_from_hasher(hasher), byte_len))
}

fn hash_reader(source: &mut impl Read) -> Result<(Sha256Digest, u64), RuntimeContentStoreError> {
    let mut hasher = Sha256::new();
    let mut byte_len = 0_u64;
    let mut buffer = [0_u8; STREAM_BUFFER_BYTES];
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|error| RuntimeContentStoreError::io("read stored runtime blob", error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        byte_len = byte_len
            .checked_add(u64::try_from(read).expect("buffer length fits u64"))
            .ok_or(RuntimeContentStoreError::LengthOverflow)?;
    }
    Ok((digest_from_hasher(hasher), byte_len))
}

fn digest_from_hasher(hasher: Sha256) -> Sha256Digest {
    Sha256Digest::parse(format!("sha256:{}", hex::encode(hasher.finalize())))
        .expect("SHA-256 output always satisfies the typed digest grammar")
}

fn digest_basename(digest: &Sha256Digest) -> String {
    digest
        .as_str()
        .strip_prefix("sha256:")
        .expect("typed SHA-256 digest has the required prefix")
        .to_owned()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaultPoint {
    AfterStageWrite,
    AfterStageSync,
    AfterDigestVerification,
    BeforePublication,
    AfterPublication,
    AfterPostPublicationFileSync,
    AfterNamespaceSync,
    BeforeSuccess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PersistenceEvent {
    StageFileSync,
    Published,
    PublishedFileSync,
    ExistingFileSync,
    NamespaceSync,
    VerifiedRead,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct TestObserver {
    events: Mutex<Vec<PersistenceEvent>>,
}

#[cfg(test)]
impl TestObserver {
    fn fault(&self, point: FaultPoint) -> Result<(), RuntimeContentStoreError> {
        if std::env::var_os("PACTRUN_M1B_FAULT")
            .is_some_and(|configured| configured == point.name())
        {
            std::process::exit(86);
        }
        Ok(())
    }

    fn record(&self, event: PersistenceEvent) {
        self.events.lock().expect("test event mutex").push(event);
    }
}

impl FaultPoint {
    fn name(self) -> &'static str {
        match self {
            Self::AfterStageWrite => "after_stage_write",
            Self::AfterStageSync => "after_stage_sync",
            Self::AfterDigestVerification => "after_digest_verification",
            Self::BeforePublication => "before_publication",
            Self::AfterPublication => "after_publication",
            Self::AfterPostPublicationFileSync => "after_post_publication_file_sync",
            Self::AfterNamespaceSync => "after_namespace_sync",
            Self::BeforeSuccess => "before_success",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Cursor, Read},
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::{Arc, Condvar, Mutex, mpsc},
        thread,
        time::Duration,
    };

    use tempfile::TempDir;

    use super::*;
    use crate::{
        domain::{
            ContentId, RevisionCoreProjectionInputV1, RuntimeContentProjectionInputV1,
            RuntimeFileKindV1, RuntimeFileV1, RuntimePath, Sha256Digest, project_revision_core_v1,
            project_runtime_content_closure_v1, validate_revision_content_v1,
        },
        revision_core_v1::{
            calculate_revision_content_digest_v1, encode_canonical_revision_core_v1,
            encode_canonical_runtime_content_v1,
        },
    };

    const WORKER_TEST: &str = "persistence::runtime_content_store::tests::m1b_subprocess_worker";

    fn digest(bytes: &[u8]) -> Sha256Digest {
        Sha256Digest::parse(format!("sha256:{}", hex::encode(Sha256::digest(bytes)))).unwrap()
    }

    fn test_root() -> (TempDir, PathBuf) {
        let parent = std::env::var_os("PACTRUN_M1B_TEST_PARENT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m1b-tests"));
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("runtime-content-")
            .tempdir_in(parent)
            .unwrap();
        let root = temporary.path().join("store");
        fs::create_dir(&root).unwrap();
        (temporary, root)
    }

    fn final_path(root: &Path, digest: &Sha256Digest) -> PathBuf {
        root.join(digest_basename(digest))
    }

    fn final_blob_names(root: &Path) -> Vec<String> {
        let mut names = fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| {
                name.len() == 64
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            })
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    fn closure_with_shared_digest(shared_digest: &Sha256Digest) -> RuntimeContentClosureIdentityV1 {
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 {
            files: vec![
                RuntimeFileV1 {
                    id: ContentId::parse("launcher").unwrap(),
                    path: RuntimePath::parse("bin/launcher").unwrap(),
                    kind: RuntimeFileKindV1::RegularFile,
                    blob_digest: shared_digest.clone(),
                    executable: true,
                },
                RuntimeFileV1 {
                    id: ContentId::parse("script").unwrap(),
                    path: RuntimePath::parse("hooks/script").unwrap(),
                    kind: RuntimeFileKindV1::RegularFile,
                    blob_digest: shared_digest.clone(),
                    executable: false,
                },
            ],
        })
        .unwrap()
    }

    fn run_worker(root: &Path, bytes: &[u8], fault: Option<FaultPoint>, marker: &Path) -> bool {
        let expected = digest(bytes);
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg(WORKER_TEST)
            .arg("--nocapture")
            .env("PACTRUN_M1B_WORKER", "put")
            .env("PACTRUN_M1B_ROOT", root)
            .env("PACTRUN_M1B_CONTENT_HEX", hex::encode(bytes))
            .env("PACTRUN_M1B_DIGEST", expected.as_str())
            .env("PACTRUN_M1B_SUCCESS_MARKER", marker)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(fault) = fault {
            command.env("PACTRUN_M1B_FAULT", fault.name());
        }
        command.status().unwrap().success()
    }

    struct CoordinatedReader {
        bytes: Cursor<Vec<u8>>,
        announced: bool,
        started: mpsc::Sender<()>,
        release: Arc<(Mutex<bool>, Condvar)>,
    }

    impl Read for CoordinatedReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if !self.announced {
                self.announced = true;
                self.started.send(()).unwrap();
                let (released, condition) = &*self.release;
                let mut released = released.lock().unwrap();
                while !*released {
                    released = condition.wait(released).unwrap();
                }
            }
            self.bytes.read(buffer)
        }
    }

    // Test-ID: PR-TEST-0047
    // Verifies: PR-REQ-0228, PR-REQ-0229
    #[test]
    fn basic_flat_immutable_storage_preserves_opaque_bytes() {
        let (_temporary, root) = test_root();
        let store = RuntimeContentStore::open(&root).unwrap();

        for bytes in [b"".as_slice(), b"opaque runtime bytes".as_slice()] {
            let expected = digest(bytes);
            let stored = store
                .put_verified(&expected, &mut Cursor::new(bytes))
                .unwrap();
            assert_eq!(stored.digest(), &expected);
            assert_eq!(stored.byte_len(), bytes.len() as u64);

            let mut verified = store.open_verified(&expected).unwrap();
            assert_eq!(verified.digest(), &expected);
            assert_eq!(verified.byte_len(), bytes.len() as u64);
            let mut read_back = Vec::new();
            verified.read_to_end(&mut read_back).unwrap();
            assert_eq!(read_back, bytes);
            assert_eq!(final_path(&root, &expected).file_name().unwrap().len(), 64);
        }

        let shared_bytes = b"physically shared";
        let shared_digest = digest(shared_bytes);
        store
            .put_verified(&shared_digest, &mut Cursor::new(shared_bytes))
            .unwrap();
        store
            .verify_runtime_content_available(&closure_with_shared_digest(&shared_digest))
            .unwrap();

        assert_eq!(final_blob_names(&root).len(), 3);
        assert!(root.join(LOCK_NAME).is_file());
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .all(|entry| !entry.unwrap().path().is_dir())
        );
        let names = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(!names.iter().any(|name| {
            name.contains("launcher")
                || name.contains("script")
                || name.contains("bin")
                || name.contains("executable")
        }));
    }

    // Test-ID: PR-TEST-0048
    // Verifies: PR-REQ-0228, PR-REQ-0229, PR-REQ-0230
    #[test]
    fn mismatches_corruption_and_redirected_entries_are_never_repaired() {
        let (_temporary, root) = test_root();
        let store = RuntimeContentStore::open(&root).unwrap();

        let expected = digest(b"expected");
        let mismatch = store
            .put_verified(&expected, &mut Cursor::new(b"different"))
            .unwrap_err();
        assert!(matches!(
            mismatch,
            RuntimeContentStoreError::IncomingDigestMismatch { .. }
        ));
        assert!(!final_path(&root, &expected).exists());

        fs::write(final_path(&root, &expected), b"corrupt").unwrap();
        assert!(matches!(
            store.open_verified(&expected).unwrap_err(),
            RuntimeContentStoreError::CorruptBlob { .. }
        ));
        assert!(matches!(
            store
                .put_verified(&expected, &mut Cursor::new(b"expected"))
                .unwrap_err(),
            RuntimeContentStoreError::CorruptBlob { .. }
        ));
        assert_eq!(fs::read(final_path(&root, &expected)).unwrap(), b"corrupt");

        let directory_digest = digest(b"directory");
        fs::create_dir(final_path(&root, &directory_digest)).unwrap();
        assert!(store.open_verified(&directory_digest).is_err());

        let link_digest = digest(b"redirected");
        let link_path = final_path(&root, &link_digest);
        let target = root.join("redirect-target");
        fs::write(&target, b"redirected").unwrap();
        if create_file_symlink(&target, &link_path).unwrap_or(false) {
            assert!(store.open_verified(&link_digest).is_err());
        }

        let missing_root = root.join("missing");
        assert!(RuntimeContentStore::open(missing_root).is_err());
        let invalid_root = root.join("not-a-directory");
        fs::write(&invalid_root, b"file").unwrap();
        assert!(RuntimeContentStore::open(invalid_root).is_err());
    }

    // Test-ID: PR-TEST-0049
    // Verifies: PR-REQ-0228, PR-REQ-0229
    #[test]
    fn concurrent_and_recovered_publication_is_idempotent() {
        let (temporary, root) = test_root();
        let store = Arc::new(RuntimeContentStore::open(&root).unwrap());
        let bytes = b"same digest across writers".to_vec();
        let expected = digest(&bytes);

        let (started_sender, started_receiver) = mpsc::channel();
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let coordinated = (0..2)
            .map(|_| {
                let store = Arc::clone(&store);
                let expected = expected.clone();
                let started = started_sender.clone();
                let release = Arc::clone(&release);
                let bytes = bytes.clone();
                thread::spawn(move || {
                    let mut reader = CoordinatedReader {
                        bytes: Cursor::new(bytes),
                        announced: false,
                        started,
                        release,
                    };
                    store.put_verified(&expected, &mut reader).unwrap();
                })
            })
            .collect::<Vec<_>>();
        let first_started = started_receiver
            .recv_timeout(Duration::from_secs(5))
            .is_ok();
        let second_started = started_receiver
            .recv_timeout(Duration::from_secs(5))
            .is_ok();
        {
            let (released, condition) = &*release;
            *released.lock().unwrap() = true;
            condition.notify_all();
        }
        for writer in coordinated {
            writer.join().unwrap();
        }
        assert!(
            first_started && second_started,
            "streaming was serialized by the publication lock"
        );

        let threads = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let bytes = bytes.clone();
                let expected = expected.clone();
                thread::spawn(move || {
                    store
                        .put_verified(&expected, &mut Cursor::new(bytes))
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        for writer in threads {
            assert_eq!(writer.join().unwrap().digest(), &expected);
        }

        let distinct = (0..4)
            .map(|index| {
                let store = Arc::clone(&store);
                thread::spawn(move || {
                    let bytes = format!("different digest {index}").into_bytes();
                    let expected = digest(&bytes);
                    store
                        .put_verified(&expected, &mut Cursor::new(bytes))
                        .unwrap();
                    expected
                })
            })
            .collect::<Vec<_>>();
        for writer in distinct {
            store.open_verified(&writer.join().unwrap()).unwrap();
        }

        drop(store);
        let process_root = temporary.path().join("process-store");
        fs::create_dir(&process_root).unwrap();
        let children = (0..4)
            .map(|index| {
                let root = process_root.clone();
                let marker = temporary.path().join(format!("process-{index}.success"));
                let bytes = bytes.clone();
                thread::spawn(move || (run_worker(&root, &bytes, None, &marker), marker))
            })
            .collect::<Vec<_>>();
        for child in children {
            let (success, marker) = child.join().unwrap();
            assert!(success);
            assert!(marker.exists());
        }
        let process_store = RuntimeContentStore::open(&process_root).unwrap();
        process_store.open_verified(&expected).unwrap();
        assert_eq!(
            final_blob_names(&process_root),
            vec![digest_basename(&expected)]
        );

        process_store.take_events();
        process_store
            .put_verified(&expected, &mut Cursor::new(&bytes))
            .unwrap();
        let events = process_store.take_events();
        assert!(events.contains(&PersistenceEvent::ExistingFileSync));
        assert!(events.contains(&PersistenceEvent::NamespaceSync));
    }

    // Test-ID: PR-TEST-0050
    // Verifies: PR-REQ-0229
    #[test]
    fn process_crash_api_ordering_and_filesystem_state_are_recoverable() {
        let bytes = b"fault boundary bytes";
        let expected = digest(bytes);
        for fault in [
            FaultPoint::AfterStageWrite,
            FaultPoint::AfterStageSync,
            FaultPoint::AfterDigestVerification,
            FaultPoint::BeforePublication,
            FaultPoint::AfterPublication,
            FaultPoint::AfterPostPublicationFileSync,
            FaultPoint::AfterNamespaceSync,
            FaultPoint::BeforeSuccess,
        ] {
            let (temporary, root) = test_root();
            let marker = temporary.path().join("success.marker");
            assert!(!run_worker(&root, bytes, Some(fault), &marker));
            assert!(
                !marker.exists(),
                "{} published a success token",
                fault.name()
            );

            let store = RuntimeContentStore::open(&root).unwrap();
            let final_path = final_path(&root, &expected);
            if final_path.exists() {
                let verified = store.open_verified(&expected).unwrap();
                assert_eq!(verified.byte_len(), bytes.len() as u64);
            }
            store
                .put_verified(&expected, &mut Cursor::new(bytes))
                .unwrap();
        }

        let (_temporary, root) = test_root();
        {
            let store = RuntimeContentStore::open(&root).unwrap();
            assert!(root.join(LOCK_NAME).is_file());
            drop(store);
        }
        fs::remove_file(root.join(LOCK_NAME)).unwrap();
        RuntimeContentStore::open(&root).unwrap();
        assert!(root.join(LOCK_NAME).is_file());
    }

    // Test-ID: PR-TEST-0051
    // Verifies: PR-REQ-0228, PR-REQ-0230
    #[test]
    fn semantic_identity_is_independent_from_deduplicated_physical_availability() {
        let bytes = b"one physical blob";
        let shared_digest = digest(bytes);
        let closure = closure_with_shared_digest(&shared_digest);
        let core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
            inputs: Vec::new(),
            actions: Vec::new(),
            snapshot: None,
            migrations: Vec::new(),
            cleanup: None,
        })
        .unwrap();
        let content = validate_revision_content_v1(core, closure.clone()).unwrap();
        let core_before = encode_canonical_revision_core_v1(&content.core).unwrap();
        let runtime_before = encode_canonical_runtime_content_v1(&content.runtime_content).unwrap();
        let digest_before = calculate_revision_content_digest_v1(&content).unwrap();

        let (_temporary, root) = test_root();
        let store = RuntimeContentStore::open(&root).unwrap();
        assert!(matches!(
            store
                .verify_runtime_content_available(&content.runtime_content)
                .unwrap_err(),
            RuntimeContentStoreError::MissingBlob(_)
        ));
        store
            .put_verified(&shared_digest, &mut Cursor::new(bytes))
            .unwrap();
        store.take_events();
        store
            .verify_runtime_content_available(&content.runtime_content)
            .unwrap();
        assert_eq!(
            store
                .take_events()
                .iter()
                .filter(|event| **event == PersistenceEvent::VerifiedRead)
                .count(),
            1
        );

        assert_eq!(
            encode_canonical_revision_core_v1(&content.core).unwrap(),
            core_before
        );
        assert_eq!(
            encode_canonical_runtime_content_v1(&content.runtime_content).unwrap(),
            runtime_before
        );
        assert_eq!(
            calculate_revision_content_digest_v1(&content).unwrap(),
            digest_before
        );

        fs::write(final_path(&root, &shared_digest), b"mutated externally").unwrap();
        assert!(matches!(
            store
                .verify_runtime_content_available(&content.runtime_content)
                .unwrap_err(),
            RuntimeContentStoreError::CorruptBlob { .. }
        ));
    }

    #[test]
    fn m1b_subprocess_worker() {
        if std::env::var_os("PACTRUN_M1B_WORKER").is_none() {
            return;
        }
        let root = PathBuf::from(std::env::var_os("PACTRUN_M1B_ROOT").unwrap());
        let bytes = hex::decode(std::env::var("PACTRUN_M1B_CONTENT_HEX").unwrap()).unwrap();
        let expected = Sha256Digest::parse(std::env::var("PACTRUN_M1B_DIGEST").unwrap()).unwrap();
        let marker = PathBuf::from(std::env::var_os("PACTRUN_M1B_SUCCESS_MARKER").unwrap());
        let store = RuntimeContentStore::open(root).unwrap();
        store
            .put_verified(&expected, &mut Cursor::new(bytes))
            .unwrap();
        fs::write(marker, b"stored-runtime-blob-returned").unwrap();
    }

    #[cfg(unix)]
    fn create_file_symlink(target: &Path, link: &Path) -> io::Result<bool> {
        std::os::unix::fs::symlink(target, link)?;
        Ok(true)
    }

    #[cfg(windows)]
    fn create_file_symlink(target: &Path, link: &Path) -> io::Result<bool> {
        match std::os::windows::fs::symlink_file(target, link) {
            Ok(()) => Ok(true),
            Err(error) if error.raw_os_error() == Some(1314) => Ok(false),
            Err(error) => Err(error),
        }
    }
}
