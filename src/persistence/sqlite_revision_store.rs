//! Crate-private SQLite persistence for exact Revision content.
//!
//! This module owns database reference publication only. Runtime blob bytes are
//! published durably before a new relational reference is committed.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension, TransactionBehavior, params};

use super::runtime_content_store::{
    RuntimeContentStore, RuntimeContentStoreError, StoredRuntimeBlob,
    validate_existing_regular_entry, validate_supported_storage_root,
};
#[cfg(test)]
use crate::revision_declarations::{calculate_service_free_digest, encode_service_free_revision};
use crate::{
    domain::{
        DeclarationContent, PackageId, RevisionIdentity, RevisionMetadataMutationBatch, RunId,
        Sha256Digest, ValidatedRevisionContent,
    },
    revision_content::{
        calculate_revision_content_digest, core_format_version, decode_canonical_revision_content,
        encode_canonical_revision_core,
    },
    revision_declarations::encode_canonical_runtime_content,
};

const DATABASE_DIRECTORY: &str = "database";
const RUNTIME_CONTENT_DIRECTORY: &str = "runtime-content";
const DATABASE_NAME: &str = "pactrun.sqlite3";
pub(super) const APPLICATION_ID: i64 = 0x5041_4354;
// Private SQLite bootstrap/admission marker, not the public format version.
pub(crate) const SCHEMA_VERSION: i64 = 2;
pub(super) const BASELINE_SQL: &str = include_str!("persistence_baseline.sql");
pub(super) const ALPHA1_SQL: &str = include_str!("persistence_alpha1.sql");
pub(super) const ALPHA2_SQL: &str = include_str!("persistence_alpha2.sql");
pub(super) const FAILURE_CAUSES_SQL: &str = include_str!("failure_causes.sql");
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) struct PactrunPersistence {
    pub(super) database: Mutex<Connection>,
    pub(super) database_path: PathBuf,
    pub(super) runtime_content: RuntimeContentStore,
    // Dropped after the connection: a writer cannot outlive its owner lease.
    pub(super) session: Option<crate::managed_data::StagingSession>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StoredRevisionContent {
    pub(crate) identity: RevisionIdentity,
    pub(crate) content: ValidatedRevisionContent,
}

#[derive(Debug)]
pub(crate) enum PersistenceError {
    RuntimeContent(RuntimeContentStoreError),
    Sqlite {
        operation: &'static str,
        source: rusqlite::Error,
    },
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
    DatabaseOwnership(String),
    SchemaMismatch(String),
    PublicationWitness(String),
    CorruptRevision(String),
    InvalidMetadata(String),
    MetadataConflict(String),
    CorruptMetadata(String),
    CorruptInstance(String),
    MissingInstance(String),
    StaleInstanceState,
    CompilationObservationChanged,
    InvalidManagedInput(String),
    UnauthorizedSecretExport,
    CorruptManagedInput(String),
    MissingRevision(RevisionIdentity),
    MissingRun(RunId),
    CorruptRun(String),
    InvalidRunTransition(String),
    RunNotRunning,
    RecoveryGuardActive,
    MissingRecoveryGuard,
    InvalidRunArtifact(String),
    DatabaseLockPoisoned,
    WriterAdmissionRequired,
    ActiveWriters,
    MigrationMutationConflict(RunId),
    MissingSnapshot(crate::domain::SnapshotId),
    SnapshotCollision(crate::domain::SnapshotId),
    SnapshotBundle(crate::snapshot_bundle::BundleError),
    SnapshotCodec(crate::snapshot_integrity::SnapshotCodecError),
    CorruptSnapshot(&'static str),
    UnauthorizedSnapshotExport,
    ServiceStorageUnavailable(&'static str),
    CorruptServiceStorage(&'static str),
}

impl PersistenceError {
    pub(super) fn sqlite(operation: &'static str, source: rusqlite::Error) -> Self {
        Self::Sqlite { operation, source }
    }
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ServiceStorageUnavailable(message) => {
                write!(formatter, "service storage unavailable: {message}")
            }
            Self::CorruptServiceStorage(message) => {
                write!(formatter, "corrupt service storage: {message}")
            }
            Self::RuntimeContent(source) => write!(formatter, "runtime content: {source}"),
            Self::Sqlite { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::Io { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::DatabaseOwnership(message) => {
                write!(formatter, "database ownership: {message}")
            }
            Self::SchemaMismatch(message) => write!(formatter, "schema mismatch: {message}"),
            Self::PublicationWitness(message) => {
                write!(formatter, "publication witness: {message}")
            }
            Self::CorruptRevision(message) => write!(formatter, "corrupt Revision: {message}"),
            Self::InvalidMetadata(message) => write!(formatter, "invalid metadata: {message}"),
            Self::MetadataConflict(message) => write!(formatter, "metadata conflict: {message}"),
            Self::CorruptMetadata(message) => write!(formatter, "corrupt metadata: {message}"),
            Self::CorruptInstance(message) => write!(formatter, "corrupt Instance: {message}"),
            Self::MissingInstance(name) => write!(formatter, "Instance {name:?} is not persisted"),
            Self::StaleInstanceState => formatter.write_str("stale InstanceStateVersion"),
            Self::CompilationObservationChanged => {
                formatter.write_str("Instance compilation observation changed")
            }
            Self::InvalidManagedInput(message) => {
                write!(formatter, "invalid Managed Input: {message}")
            }
            Self::UnauthorizedSecretExport => {
                formatter.write_str("Secret export is not authorized")
            }
            Self::CorruptManagedInput(message) => {
                write!(formatter, "corrupt Managed Input: {message}")
            }
            Self::MissingRevision(identity) => write!(
                formatter,
                "Revision {} / {} is not persisted",
                identity.package_id, identity.content_digest
            ),
            Self::MissingRun(run) => write!(formatter, "Run {run} is not persisted"),
            Self::CorruptRun(message) => write!(formatter, "corrupt Run: {message}"),
            Self::InvalidRunTransition(message) => {
                write!(formatter, "invalid Run transition: {message}")
            }
            Self::RunNotRunning => formatter.write_str("Run is not Running"),
            Self::RecoveryGuardActive => {
                formatter.write_str("Instance is in ManualRecoveryRequired")
            }
            Self::MissingRecoveryGuard => {
                formatter.write_str("Instance is not in ManualRecoveryRequired")
            }
            Self::InvalidRunArtifact(message) => {
                write!(formatter, "invalid Run Artifact: {message}")
            }
            Self::DatabaseLockPoisoned => formatter.write_str("database mutex poisoned"),
            Self::MissingSnapshot(id) => write!(formatter, "Snapshot {id} is not persisted"),
            Self::SnapshotCollision(id) => write!(formatter, "Snapshot identity collision: {id}"),
            Self::SnapshotBundle(error) => error.fmt(formatter),
            Self::SnapshotCodec(error) => error.fmt(formatter),
            Self::CorruptSnapshot(reason) => write!(formatter, "corrupt Snapshot: {reason}"),
            Self::UnauthorizedSnapshotExport => formatter.write_str(
                "Snapshot export requires --authorize-sensitive-export for this operation",
            ),
            Self::WriterAdmissionRequired => {
                formatter.write_str("current writable admission is required")
            }
            Self::ActiveWriters => formatter
                .write_str("exclusive maintenance is blocked by a live or unknown admitted writer"),
            Self::MigrationMutationConflict(run) => write!(
                formatter,
                "mutation_conflict: admitted Migration Run {run} holds this Instance"
            ),
        }
    }
}

impl std::error::Error for PersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::RuntimeContent(source) => Some(source),
            Self::Sqlite { source, .. } => Some(source),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<RuntimeContentStoreError> for PersistenceError {
    fn from(source: RuntimeContentStoreError) -> Self {
        Self::RuntimeContent(source)
    }
}

impl PactrunPersistence {
    /// Advisory read before staging, coordination, journal changes or publication.
    /// Admission must repeat the exact contract check inside its transaction.
    pub(crate) fn preflight_storage(root: &Path, collection: bool) -> Result<(), PersistenceError> {
        let root = validate_supported_storage_root(root)?;
        let directory = root.join(DATABASE_DIRECTORY);
        match std::fs::symlink_metadata(&directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !collection => {
                return Ok(());
            }
            Err(source) => {
                return Err(PersistenceError::Io {
                    operation: "inspect database directory",
                    source,
                });
            }
            Ok(_) => {}
        }
        let directory = validate_supported_storage_root(&directory)?;
        validate_existing_regular_entry(&directory, DATABASE_NAME)?;
        let path = directory.join(DATABASE_NAME);
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !collection => {
                return Ok(());
            }
            Err(source) => {
                return Err(PersistenceError::Io {
                    operation: "inspect database entry",
                    source,
                });
            }
            Ok(_) => {}
        }
        let reader = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| PersistenceError::sqlite("inspect persistence contract", error))?;
        configure_read_connection(&reader)?;
        let state = {
            let snapshot = reader.unchecked_transaction().map_err(|error| {
                PersistenceError::sqlite("inspect Store version snapshot", error)
            })?;
            classify_database(&snapshot)?
        };
        match state {
            DatabaseState::Baseline => Ok(()),
            DatabaseState::Alpha1 | DatabaseState::Alpha2 => {
                drop(reader);
                super::schema_upgrade::upgrade(&root)
            }
            DatabaseState::Pristine if !collection => Ok(()),
            _ => Err(PersistenceError::DatabaseOwnership(
                "operation requires a supported persistence baseline".to_owned(),
            )),
        }
    }

    pub(crate) fn open(root: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        let root = validate_supported_storage_root(root.as_ref())?;
        let session = crate::managed_data::StagingSession::prepare(&root).map_err(|_| {
            PersistenceError::DatabaseOwnership("could not prepare writer session".to_owned())
        })?;
        Self::open_prepared(&root, session)
    }

    pub(crate) fn open_prepared(
        root: &Path,
        session: crate::managed_data::StagingSession,
    ) -> Result<Self, PersistenceError> {
        Self::open_prepared_for(root, session, false)
    }

    pub(crate) fn open_for_collection(root: &Path) -> Result<Self, PersistenceError> {
        let root = validate_supported_storage_root(root)?;
        Self::preflight_storage(&root, true)?;
        let session = crate::managed_data::StagingSession::prepare(&root).map_err(|_| {
            PersistenceError::DatabaseOwnership("could not prepare collection session".to_owned())
        })?;
        Self::open_prepared_for(&root, session, true)
    }

    fn open_prepared_for(
        root: &Path,
        session: crate::managed_data::StagingSession,
        collection: bool,
    ) -> Result<Self, PersistenceError> {
        let root = validate_supported_storage_root(root)?;
        if !session.belongs_to_storage(&root) {
            return Err(PersistenceError::DatabaseOwnership(
                "writer session belongs to another storage root".to_owned(),
            ));
        }
        let database_root = validate_supported_storage_root(&root.join(DATABASE_DIRECTORY))?;
        let runtime_root = root.join(RUNTIME_CONTENT_DIRECTORY);

        validate_existing_regular_entry(&database_root, DATABASE_NAME)?;
        let database_path = database_root.join(DATABASE_NAME);
        Self::preflight_storage(&root, collection)?;
        let runtime_content = RuntimeContentStore::open(&runtime_root)?
            .with_collection_coordination(collection, true)?;
        let database = super::writer_admission::open_writer_database(&database_path, &session)?;
        validate_existing_regular_entry(&database_root, DATABASE_NAME)?;
        // Cross-session cleanup and publisher maintenance require a supported,
        // durably admitted schema; pre-admission preparation cannot authorize them.
        let _ = session.cleanup_abandoned();

        Ok(Self {
            database: Mutex::new(database),
            database_path,
            runtime_content,
            session: Some(session),
        })
    }

    /// Opens an existing current store without creating a staging session, changing
    /// SQLite journal/schema state, or opening the runtime-content publication
    /// lock. Inspection and plan preview use this narrower path.
    pub(crate) fn open_read_only(root: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        let root = validate_supported_storage_root(root.as_ref())?;
        Self::preflight_storage(&root, false)?;
        let database_root = validate_supported_storage_root(&root.join(DATABASE_DIRECTORY))?;
        let runtime_root = validate_supported_storage_root(&root.join(RUNTIME_CONTENT_DIRECTORY))?;
        validate_existing_regular_entry(&database_root, DATABASE_NAME)?;
        let database_path = database_root.join(DATABASE_NAME);
        let database =
            Connection::open_with_flags(&database_path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(
                |error| PersistenceError::sqlite("open SQLite read-only database", error),
            )?;
        configure_read_connection(&database)?;
        let state = classify_database(&database)?;
        if state != DatabaseState::Baseline {
            return Err(PersistenceError::SchemaMismatch(
                "read-only opening requires an existing supported persistence baseline".to_owned(),
            ));
        }
        validate_schema(&database, SCHEMA_VERSION)?;
        let runtime_content = RuntimeContentStore::open_read_only(&runtime_root)?
            .with_collection_coordination(false, false)?;
        Ok(Self {
            database: Mutex::new(database),
            database_path,
            runtime_content,
            session: None,
        })
    }

    pub(crate) fn put_runtime_content<R: Read>(
        &self,
        expected: &Sha256Digest,
        source: &mut R,
    ) -> Result<StoredRuntimeBlob, PersistenceError> {
        if self.session.is_none() {
            return Err(PersistenceError::WriterAdmissionRequired);
        }
        self.runtime_content
            .put_verified(expected, source)
            .map_err(PersistenceError::from)
    }

    pub(crate) fn persist_revision(
        &self,
        package_id: PackageId,
        content: &DeclarationContent,
        publications: &[StoredRuntimeBlob],
    ) -> Result<RevisionIdentity, PersistenceError> {
        self.persist_revision_internal(package_id, &content.clone().into(), publications, None)
    }

    pub(crate) fn persist_revision_with_metadata(
        &self,
        package_id: PackageId,
        content: &DeclarationContent,
        publications: &[StoredRuntimeBlob],
        metadata: &RevisionMetadataMutationBatch,
    ) -> Result<RevisionIdentity, PersistenceError> {
        self.persist_revision_internal(
            package_id,
            &content.clone().into(),
            publications,
            Some(metadata),
        )
    }

    pub(crate) fn persist_versioned_revision_with_metadata(
        &self,
        package_id: PackageId,
        content: &ValidatedRevisionContent,
        publications: &[StoredRuntimeBlob],
        metadata: &RevisionMetadataMutationBatch,
    ) -> Result<RevisionIdentity, PersistenceError> {
        self.persist_revision_internal(package_id, content, publications, Some(metadata))
    }

    pub(super) fn persist_revision_internal(
        &self,
        package_id: PackageId,
        content: &ValidatedRevisionContent,
        publications: &[StoredRuntimeBlob],
        metadata: Option<&RevisionMetadataMutationBatch>,
    ) -> Result<RevisionIdentity, PersistenceError> {
        self.persist_pack_revision(
            package_id,
            content,
            publications,
            metadata,
            None,
            &crate::domain::InstallNames::default(),
            &super::UnconditionalAcceptance,
        )
        .map(|r| r.0.identity)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn persist_pack_revision(
        &self,
        package_id: PackageId,
        content: &ValidatedRevisionContent,
        publications: &[StoredRuntimeBlob],
        metadata: Option<&RevisionMetadataMutationBatch>,
        policy: Option<crate::domain::PackMetadataConflict>,
        names: &crate::domain::InstallNames,
        cancellation: &impl super::AcceptanceArbiter,
    ) -> Result<
        (
            crate::domain::RevisionInstallReceipt,
            Vec<(
                crate::domain::PresentationTargetV1,
                crate::domain::PresentationField,
            )>,
        ),
        PersistenceError,
    > {
        let core_jcs = encode_canonical_revision_core(&content.core)
            .map_err(|error| PersistenceError::CorruptRevision(error.to_string()))?;
        let runtime_content_jcs = encode_canonical_runtime_content(&content.runtime_content)
            .map_err(|error| PersistenceError::CorruptRevision(error.to_string()))?;
        let content_digest = calculate_revision_content_digest(content)
            .map_err(|error| PersistenceError::CorruptRevision(error.to_string()))?;
        let identity = RevisionIdentity::new(package_id, content_digest);
        let expected_references = derive_references(&content.runtime_content);

        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin Revision transaction", error))?;
        self.check_write_admission(&transaction)?;
        let mut kept = Vec::new();

        if let Some(raw) = load_raw_revision(&transaction, &identity)? {
            let actual_references = load_reference_rows(&transaction, &identity)?;
            ensure_package_exists(&transaction, &identity.package_id)?;
            if raw.core_jcs != core_jcs
                || raw.runtime_content_jcs != runtime_content_jcs
                || actual_references != expected_references
            {
                return Err(PersistenceError::CorruptRevision(
                    "existing identity has different canonical bytes or references".to_owned(),
                ));
            }
            validate_raw_revision(&identity, raw, actual_references)?;
            if let Some(metadata) = metadata {
                kept = super::pack_publication::apply_install_metadata(
                    &transaction,
                    &identity,
                    metadata,
                    policy,
                )?;
            }
            super::local_catalog::install_names(&transaction, &identity, names)?;
            let receipt = super::local_catalog::install_receipt(&transaction, &identity, false)?;
            super::pack_publication::commit_install(transaction, cancellation)?;
            return Ok((receipt, kept));
        }

        validate_publications(
            &self.runtime_content,
            publications,
            &distinct_blob_digests(&content.runtime_content),
        )?;

        transaction
            .execute(
                "INSERT OR IGNORE INTO packages(package_id) VALUES (?1)",
                params![identity.package_id.as_bytes().as_slice()],
            )
            .map_err(|error| PersistenceError::sqlite("persist Package identity", error))?;
        transaction
            .execute(
                "INSERT INTO revisions(\
                    package_id, revision_content_digest, core_jcs, runtime_content_jcs\
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    identity.package_id.as_bytes().as_slice(),
                    identity.content_digest.as_bytes().as_slice(),
                    core_jcs,
                    runtime_content_jcs,
                ],
            )
            .map_err(|error| PersistenceError::sqlite("persist Revision content", error))?;
        for (content_id, blob_digest) in &expected_references {
            transaction
                .execute(
                    "INSERT INTO revision_runtime_content_refs(\
                        package_id, revision_content_digest, content_id, blob_digest\
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        identity.package_id.as_bytes().as_slice(),
                        identity.content_digest.as_bytes().as_slice(),
                        content_id,
                        blob_digest.as_slice(),
                    ],
                )
                .map_err(|error| {
                    PersistenceError::sqlite("persist Revision runtime-content reference", error)
                })?;
        }
        if let Some(metadata) = metadata {
            kept = super::pack_publication::apply_install_metadata(
                &transaction,
                &identity,
                metadata,
                policy,
            )?;
        }
        super::local_catalog::install_names(&transaction, &identity, names)?;
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_millis()).ok())
            .ok_or_else(|| {
                PersistenceError::InvalidMetadata(
                    "system clock cannot represent installation time".into(),
                )
            })?;
        super::local_catalog::record_install(&transaction, &identity, millis)?;
        let receipt = super::local_catalog::install_receipt(&transaction, &identity, true)?;
        fault(FaultPoint::BeforeRevisionCommit);
        super::pack_publication::commit_install(transaction, cancellation)?;
        fault(FaultPoint::AfterRevisionCommit);
        Ok((receipt, kept))
    }

    pub(crate) fn load_revision(
        &self,
        identity: &RevisionIdentity,
    ) -> Result<Option<StoredRevisionContent>, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        load_revision_from(&database, identity)
    }

    pub(crate) fn verify_revision_runtime_content(
        &self,
        identity: &RevisionIdentity,
    ) -> Result<(), PersistenceError> {
        let stored = self
            .load_revision(identity)?
            .ok_or_else(|| PersistenceError::MissingRevision(identity.clone()))?;
        self.runtime_content
            .verify_runtime_content_available(&stored.content.runtime_content)
            .map_err(PersistenceError::from)
    }
}

pub(super) fn load_revision_from(
    database: &Connection,
    identity: &RevisionIdentity,
) -> Result<Option<StoredRevisionContent>, PersistenceError> {
    let Some(raw) = load_raw_revision(database, identity)? else {
        return Ok(None);
    };
    ensure_package_exists(database, &identity.package_id)?;
    validate_core_storage_version(database, &raw.core_jcs)?;
    let references = load_reference_rows(database, identity)?;
    validate_raw_revision(identity, raw, references).map(Some)
}

pub(super) fn validate_core_storage_version(
    database: &Connection,
    core: &[u8],
) -> Result<(), PersistenceError> {
    core_format_version(core).map_err(|e| PersistenceError::CorruptRevision(e.to_string()))?;
    let marker: i64 = database
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| PersistenceError::sqlite("check revision storage marker", e))?;
    if marker != SCHEMA_VERSION {
        return Err(PersistenceError::CorruptRevision(
            "unsupported revision storage marker".to_owned(),
        ));
    }
    require_persistence_version(database, SCHEMA_VERSION)?;
    Ok(())
}

impl PactrunPersistence {
    pub(super) fn open_read_connection(&self) -> Result<Connection, PersistenceError> {
        let database =
            Connection::open_with_flags(&self.database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|error| PersistenceError::sqlite("open SQLite read connection", error))?;
        configure_read_connection(&database)?;
        let journal_mode: String = database
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(|error| PersistenceError::sqlite("verify WAL journal mode", error))?;
        if journal_mode != "wal" {
            return Err(PersistenceError::DatabaseOwnership(format!(
                "read connection journal_mode is {journal_mode:?}, expected WAL"
            )));
        }
        Ok(database)
    }
}

pub(super) fn configure_read_connection(database: &Connection) -> Result<(), PersistenceError> {
    database
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|error| PersistenceError::sqlite("set SQLite read busy timeout", error))?;
    database
        .execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")
        .map_err(|error| PersistenceError::sqlite("configure SQLite read connection", error))?;
    let foreign_keys: i64 = database
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .map_err(|error| PersistenceError::sqlite("verify read foreign keys", error))?;
    let synchronous: i64 = database
        .pragma_query_value(None, "synchronous", |row| row.get(0))
        .map_err(|error| PersistenceError::sqlite("verify read synchronous mode", error))?;
    if foreign_keys != 1 || synchronous != 2 {
        return Err(PersistenceError::DatabaseOwnership(format!(
            "read connection settings are foreign_keys={foreign_keys}, synchronous={synchronous}"
        )));
    }
    Ok(())
}

pub(super) fn establish_wal_mode(database: &Connection) -> Result<(), PersistenceError> {
    let started = Instant::now();
    loop {
        match database.query_row("PRAGMA journal_mode=WAL", [], |row| row.get::<_, String>(0)) {
            Ok(journal_mode) if journal_mode == "wal" => return Ok(()),
            Ok(journal_mode) => {
                return Err(PersistenceError::DatabaseOwnership(format!(
                    "journal_mode=WAL returned {journal_mode:?}"
                )));
            }
            Err(error)
                if is_transient_journal_mode_race(&error) && started.elapsed() < BUSY_TIMEOUT =>
            {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                return Err(PersistenceError::sqlite(
                    "establish WAL journal mode",
                    error,
                ));
            }
        }
    }
}

fn is_transient_journal_mode_race(error: &rusqlite::Error) -> bool {
    let rusqlite::Error::SqliteFailure(sqlite, _) = error else {
        return false;
    };
    // Concurrent first-time WAL transitions can contend on SQLite's journal
    // files. Keep the retry narrow and bounded; a persistent result is still
    // reported as a bootstrap failure rather than treated as successful WAL.
    matches!(
        sqlite.code,
        ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked
    ) || sqlite.extended_code == rusqlite::ffi::SQLITE_IOERR_FSTAT
}

pub(super) fn configure_connection(database: &Connection) -> Result<(), PersistenceError> {
    database
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|error| PersistenceError::sqlite("set SQLite busy timeout", error))?;
    database
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|error| PersistenceError::sqlite("enable foreign keys", error))?;
    database
        .pragma_update(None, "synchronous", "FULL")
        .map_err(|error| PersistenceError::sqlite("set synchronous FULL", error))?;

    let foreign_keys: i64 = database
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .map_err(|error| PersistenceError::sqlite("verify foreign keys", error))?;
    let synchronous: i64 = database
        .pragma_query_value(None, "synchronous", |row| row.get(0))
        .map_err(|error| PersistenceError::sqlite("verify synchronous mode", error))?;
    if foreign_keys != 1 || synchronous != 2 {
        return Err(PersistenceError::DatabaseOwnership(format!(
            "connection settings are foreign_keys={foreign_keys}, synchronous={synchronous}"
        )));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DatabaseState {
    Pristine,
    Alpha1,
    Alpha2,
    Baseline,
}

pub(super) fn classify_database(database: &Connection) -> Result<DatabaseState, PersistenceError> {
    let application_id: i64 = database
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(|error| PersistenceError::sqlite("read application_id", error))?;
    let user_version: i64 = database
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| PersistenceError::sqlite("read user_version", error))?;
    let has_user_objects = !schema_objects(database)?.is_empty();

    match (application_id, user_version, has_user_objects) {
        (0, 0, false) => Ok(DatabaseState::Pristine),
        (APPLICATION_ID, 0, true) => {
            validate_schema(database, 0)?;
            Ok(DatabaseState::Alpha1)
        }
        (APPLICATION_ID, SCHEMA_VERSION, true) => {
            validate_schema(database, SCHEMA_VERSION)?;
            Ok(DatabaseState::Baseline)
        }
        (APPLICATION_ID, 1, true) => {
            validate_schema(database, 1)?;
            Ok(DatabaseState::Alpha2)
        }
        (APPLICATION_ID, version, _) => Err(PersistenceError::DatabaseOwnership(format!(
            "unsupported persistence bootstrap marker {version}; development stores are not upgraded"
        ))),
        (application, _, _) if application != 0 && application != APPLICATION_ID => {
            Err(PersistenceError::DatabaseOwnership(format!(
                "foreign application_id 0x{application:08x}"
            )))
        }
        (0, 0, true) => Err(PersistenceError::DatabaseOwnership(
            "unmarked database contains user schema objects".to_owned(),
        )),
        _ => Err(PersistenceError::DatabaseOwnership(format!(
            "incompatible ownership markers application_id=0x{application_id:08x}, user_version={user_version}"
        ))),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SchemaObject {
    object_type: String,
    name: String,
    table_name: String,
    sql: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TableListRow {
    name: String,
    object_type: String,
    columns: i64,
    without_rowid: i64,
    strict: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ColumnRow {
    position: i64,
    name: String,
    declared_type: String,
    not_null: i64,
    default_value: Option<String>,
    primary_key_ordinal: i64,
    hidden: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ForeignKeyRow {
    id: i64,
    sequence: i64,
    parent_table: String,
    child_column: String,
    parent_column: String,
    on_update: String,
    on_delete: String,
    match_clause: String,
}

/// Keep even corrupt metadata diagnostics bounded; 129 characters cannot parse as
/// a supported identifier (the shared grammar is bounded to 128 ASCII bytes).
fn require_persistence_version(
    database: &Connection,
    version: i64,
) -> Result<(), PersistenceError> {
    let text: String = database
        .query_row(
            "SELECT substr(format_version,1,129) FROM pactrun_metadata WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| PersistenceError::sqlite("read persistence format version", error))?;
    if version <= 1 {
        return if text
            == if version == 0 {
                "1.0-alpha.1"
            } else {
                "1.0-alpha.2"
            } {
            Ok(())
        } else {
            Err(PersistenceError::SchemaMismatch(
                "invalid earlier persistence format".into(),
            ))
        };
    }
    crate::domain::VersionDomain::Persistence
        .require(&text)
        .map(|_| ())
        .map_err(PersistenceError::SchemaMismatch)
}

pub(super) fn validate_schema(database: &Connection, version: i64) -> Result<(), PersistenceError> {
    let expected = Connection::open_in_memory()
        .map_err(|error| PersistenceError::sqlite("open expected schema database", error))?;
    if version == SCHEMA_VERSION {
        expected
            .execute_batch(BASELINE_SQL)
            .map_err(|error| PersistenceError::sqlite("construct expected baseline", error))?;
        require_persistence_version(database, version)?;
    } else if version <= 1 {
        expected
            .execute_batch(if version == 0 { ALPHA1_SQL } else { ALPHA2_SQL })
            .map_err(|error| {
                PersistenceError::sqlite("construct expected alpha.1 schema", error)
            })?;
        require_persistence_version(database, version)?;
    } else {
        return Err(PersistenceError::SchemaMismatch(
            "unsupported persistence marker".to_owned(),
        ));
    }

    if schema_objects(database)? != schema_objects(&expected)? {
        return Err(PersistenceError::SchemaMismatch(format!(
            "sqlite_schema manifest differs from the supported Persistence baseline ({version})"
        )));
    }
    if table_list(database)? != table_list(&expected)? {
        return Err(PersistenceError::SchemaMismatch(
            "STRICT or WITHOUT ROWID table structure differs".to_owned(),
        ));
    }
    let expected_tables = table_list(&expected)?
        .into_iter()
        .filter(|row| row.object_type == "table")
        .map(|row| row.name)
        .collect::<Vec<_>>();
    for table in expected_tables {
        if table_columns(database, &table)? != table_columns(&expected, &table)? {
            return Err(PersistenceError::SchemaMismatch(format!(
                "column structure differs for {table}"
            )));
        }
        if foreign_keys(database, &table)? != foreign_keys(&expected, &table)? {
            return Err(PersistenceError::SchemaMismatch(format!(
                "foreign keys differ for {table}"
            )));
        }
    }
    Ok(())
}

fn schema_objects(database: &Connection) -> Result<Vec<SchemaObject>, PersistenceError> {
    let mut statement = database
        .prepare(
            "SELECT type, name, tbl_name, sql \
             FROM main.sqlite_schema \
             WHERE name NOT LIKE 'sqlite_%' \
             ORDER BY type, name",
        )
        .map_err(|error| PersistenceError::sqlite("prepare schema manifest", error))?;
    statement
        .query_map([], |row| {
            Ok(SchemaObject {
                object_type: row.get(0)?,
                name: row.get(1)?,
                table_name: row.get(2)?,
                sql: row.get(3)?,
            })
        })
        .map_err(|error| PersistenceError::sqlite("query schema manifest", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read schema manifest", error))
}

fn table_list(database: &Connection) -> Result<Vec<TableListRow>, PersistenceError> {
    let mut statement = database
        .prepare(
            "SELECT name, type, ncol, wr, strict \
             FROM pragma_table_list \
             WHERE schema = 'main' AND name NOT LIKE 'sqlite_%' \
             ORDER BY name",
        )
        .map_err(|error| PersistenceError::sqlite("prepare table list", error))?;
    statement
        .query_map([], |row| {
            Ok(TableListRow {
                name: row.get(0)?,
                object_type: row.get(1)?,
                columns: row.get(2)?,
                without_rowid: row.get(3)?,
                strict: row.get(4)?,
            })
        })
        .map_err(|error| PersistenceError::sqlite("query table list", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read table list", error))
}

fn table_columns(database: &Connection, table: &str) -> Result<Vec<ColumnRow>, PersistenceError> {
    let mut statement = database
        .prepare(
            "SELECT cid, name, type, \"notnull\", dflt_value, pk, hidden \
             FROM pragma_table_xinfo(?1) ORDER BY cid",
        )
        .map_err(|error| PersistenceError::sqlite("prepare table columns", error))?;
    statement
        .query_map([table], |row| {
            Ok(ColumnRow {
                position: row.get(0)?,
                name: row.get(1)?,
                declared_type: row.get(2)?,
                not_null: row.get(3)?,
                default_value: row.get(4)?,
                primary_key_ordinal: row.get(5)?,
                hidden: row.get(6)?,
            })
        })
        .map_err(|error| PersistenceError::sqlite("query table columns", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read table columns", error))
}

fn foreign_keys(
    database: &Connection,
    table: &str,
) -> Result<Vec<ForeignKeyRow>, PersistenceError> {
    let mut statement = database
        .prepare(
            "SELECT id, seq, \"table\", \"from\", \"to\", on_update, on_delete, \"match\" \
             FROM pragma_foreign_key_list(?1) ORDER BY id, seq",
        )
        .map_err(|error| PersistenceError::sqlite("prepare foreign keys", error))?;
    statement
        .query_map([table], |row| {
            Ok(ForeignKeyRow {
                id: row.get(0)?,
                sequence: row.get(1)?,
                parent_table: row.get(2)?,
                child_column: row.get(3)?,
                parent_column: row.get(4)?,
                on_update: row.get(5)?,
                on_delete: row.get(6)?,
                match_clause: row.get(7)?,
            })
        })
        .map_err(|error| PersistenceError::sqlite("query foreign keys", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read foreign keys", error))
}

#[derive(Debug)]
struct RawRevision {
    core_jcs: Vec<u8>,
    runtime_content_jcs: Vec<u8>,
}

fn load_raw_revision(
    database: &Connection,
    identity: &RevisionIdentity,
) -> Result<Option<RawRevision>, PersistenceError> {
    database
        .query_row(
            "SELECT core_jcs, runtime_content_jcs \
             FROM revisions \
             WHERE package_id = ?1 AND revision_content_digest = ?2",
            params![
                identity.package_id.as_bytes().as_slice(),
                identity.content_digest.as_bytes().as_slice(),
            ],
            |row| {
                Ok(RawRevision {
                    core_jcs: row.get(0)?,
                    runtime_content_jcs: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load Revision row", error))
}

fn ensure_package_exists(
    database: &Connection,
    package_id: &PackageId,
) -> Result<(), PersistenceError> {
    let exists: bool = database
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM packages WHERE package_id = ?1)",
            params![package_id.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| PersistenceError::sqlite("load Package identity", error))?;
    if exists {
        Ok(())
    } else {
        Err(PersistenceError::CorruptRevision(
            "Revision refers to a missing Package identity".to_owned(),
        ))
    }
}

fn load_reference_rows(
    database: &Connection,
    identity: &RevisionIdentity,
) -> Result<BTreeMap<String, [u8; 32]>, PersistenceError> {
    let mut statement = database
        .prepare(
            "SELECT content_id, blob_digest \
             FROM revision_runtime_content_refs \
             WHERE package_id = ?1 AND revision_content_digest = ?2 \
             ORDER BY content_id COLLATE BINARY",
        )
        .map_err(|error| PersistenceError::sqlite("prepare runtime references", error))?;
    let rows = statement
        .query_map(
            params![
                identity.package_id.as_bytes().as_slice(),
                identity.content_digest.as_bytes().as_slice(),
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .map_err(|error| PersistenceError::sqlite("query runtime references", error))?;
    let mut references = BTreeMap::new();
    for row in rows {
        let (content_id, digest) =
            row.map_err(|error| PersistenceError::sqlite("read runtime reference", error))?;
        let digest: [u8; 32] = digest.try_into().map_err(|_| {
            PersistenceError::CorruptRevision(
                "runtime reference contains a non-SHA-256 blob digest".to_owned(),
            )
        })?;
        if references.insert(content_id, digest).is_some() {
            return Err(PersistenceError::CorruptRevision(
                "runtime reference index contains a duplicate ContentId".to_owned(),
            ));
        }
    }
    Ok(references)
}

fn validate_raw_revision(
    identity: &RevisionIdentity,
    raw: RawRevision,
    actual_references: BTreeMap<String, [u8; 32]>,
) -> Result<StoredRevisionContent, PersistenceError> {
    let content = decode_canonical_revision_content(&raw.core_jcs, &raw.runtime_content_jcs)
        .map_err(|error| PersistenceError::CorruptRevision(error.to_string()))?;
    let calculated = calculate_revision_content_digest(&content)
        .map_err(|error| PersistenceError::CorruptRevision(error.to_string()))?;
    if calculated != identity.content_digest {
        return Err(PersistenceError::CorruptRevision(
            "stored canonical components do not match the Revision identity".to_owned(),
        ));
    }
    if derive_references(&content.runtime_content) != actual_references {
        return Err(PersistenceError::CorruptRevision(
            "runtime reference index differs from the canonical closure".to_owned(),
        ));
    }
    Ok(StoredRevisionContent {
        identity: identity.clone(),
        content,
    })
}

fn derive_references(
    content: &crate::domain::RuntimeContentClosureIdentityV1,
) -> BTreeMap<String, [u8; 32]> {
    content
        .files()
        .iter()
        .map(|file| (file.id.as_str().to_owned(), file.blob_digest.to_bytes()))
        .collect()
}

fn distinct_blob_digests(
    content: &crate::domain::RuntimeContentClosureIdentityV1,
) -> BTreeSet<Sha256Digest> {
    content
        .files()
        .iter()
        .map(|file| file.blob_digest.clone())
        .collect()
}

fn validate_publications(
    store: &RuntimeContentStore,
    publications: &[StoredRuntimeBlob],
    required: &BTreeSet<Sha256Digest>,
) -> Result<(), PersistenceError> {
    let mut supplied = BTreeSet::new();
    for publication in publications {
        if !store.owns_publication(publication) {
            return Err(PersistenceError::PublicationWitness(
                "token belongs to another RuntimeContentStore instance".to_owned(),
            ));
        }
        if !supplied.insert(publication.digest().clone()) {
            return Err(PersistenceError::PublicationWitness(format!(
                "duplicate token for {}",
                publication.digest().as_str()
            )));
        }
    }
    if &supplied != required {
        let missing = required
            .difference(&supplied)
            .map(Sha256Digest::as_str)
            .collect::<Vec<_>>();
        let extra = supplied
            .difference(required)
            .map(Sha256Digest::as_str)
            .collect::<Vec<_>>();
        return Err(PersistenceError::PublicationWitness(format!(
            "digest set mismatch; missing={missing:?}, extra={extra:?}"
        )));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FaultPoint {
    AfterPackExportRead,
    BeforeObjectDeletionCommit,
    AfterObjectDeletionCommit,
    BeforeCollectionRemoval,
    AfterCollectionRemoval,
    AfterCollectionClaim,
    AfterSnapshotReadEstablished,
    BeforeSnapshotImportCommit,
    AfterSnapshotImportCommit,
    BeforeWritableAdmission,
    AfterWritableAdmission,
    BeforeCleanupBoundaryCommit,
    AfterCleanupBoundaryCommit,
    AfterDiscardIntentCommit,
    AfterDiscardRemoval,
    AfterDiscardCompletionCommit,
    AfterFinalizationIdentityCommit,
    AfterFinalizationRemoval,
    BeforeServiceAllocationIntentCommit,
    AfterServiceAllocationIntentCommit,
    AfterServiceAllocationDirectory,
    BeforeServiceInstanceCommit,
    AfterServiceInstanceCommit,
    AfterTargetProposalReceipt,
    BeforeMigrationEdgeCommit,
    AfterMigrationEdgeCommit,
    AfterWalBeforeBootstrap,
    BeforeBootstrapCommit,
    BeforeMetadataCommit,
    AfterMetadataCommit,
    BeforeRevisionCommit,
    AfterRevisionCommit,
    BeforeRunAcceptCommit,
    AfterRunAcceptCommit,
    BeforeRunAdmitCommit,
    AfterRunAdmitCommit,
    BeforeRecoveryRiskCommit,
    AfterRecoveryRiskCommit,
    BeforeRecoveryResolutionCommit,
    AfterRecoveryResolutionCommit,
    BeforeManualRecoveryCommit,
    AfterManualRecoveryCommit,
    AfterRecoveryOwnerLossProbe,
    BeforeRunFinishCommit,
    AfterRunFinishCommit,
}

impl FaultPoint {
    #[cfg(test)]
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::AfterPackExportRead => "after_pack_export_read",
            Self::BeforeObjectDeletionCommit => "before_object_deletion_commit",
            Self::AfterObjectDeletionCommit => "after_object_deletion_commit",
            Self::BeforeCollectionRemoval => "before_collection_removal",
            Self::AfterCollectionRemoval => "after_collection_removal",
            Self::AfterCollectionClaim => "after_collection_claim",
            Self::AfterSnapshotReadEstablished => "after_snapshot_read_established",
            Self::BeforeSnapshotImportCommit => "before_snapshot_import_commit",
            Self::AfterSnapshotImportCommit => "after_snapshot_import_commit",
            Self::BeforeWritableAdmission => "before_writable_admission",
            Self::AfterWritableAdmission => "after_writable_admission",
            Self::BeforeCleanupBoundaryCommit => "before_cleanup_boundary_commit",
            Self::AfterCleanupBoundaryCommit => "after_cleanup_boundary_commit",
            Self::AfterDiscardIntentCommit => "after_discard_intent_commit",
            Self::AfterDiscardRemoval => "after_discard_removal",
            Self::AfterDiscardCompletionCommit => "after_discard_completion_commit",
            Self::AfterFinalizationIdentityCommit => "after_finalization_identity_commit",
            Self::AfterFinalizationRemoval => "after_finalization_removal",
            Self::BeforeServiceAllocationIntentCommit => "before_service_allocation_intent_commit",
            Self::AfterServiceAllocationIntentCommit => "after_service_allocation_intent_commit",
            Self::AfterServiceAllocationDirectory => "after_service_allocation_directory",
            Self::BeforeServiceInstanceCommit => "before_service_instance_commit",
            Self::AfterServiceInstanceCommit => "after_service_instance_commit",
            Self::AfterTargetProposalReceipt => "after_target_proposal_receipt",
            Self::BeforeMigrationEdgeCommit => "before_migration_edge_commit",
            Self::AfterMigrationEdgeCommit => "after_migration_edge_commit",
            Self::AfterWalBeforeBootstrap => "after_wal_before_bootstrap",
            Self::BeforeBootstrapCommit => "before_bootstrap_commit",
            Self::BeforeMetadataCommit => "before_metadata_commit",
            Self::AfterMetadataCommit => "after_metadata_commit",
            Self::BeforeRevisionCommit => "before_revision_commit",
            Self::AfterRevisionCommit => "after_revision_commit",
            Self::BeforeRunAcceptCommit => "before_run_accept_commit",
            Self::AfterRunAcceptCommit => "after_run_accept_commit",
            Self::BeforeRunAdmitCommit => "before_run_admit_commit",
            Self::AfterRunAdmitCommit => "after_run_admit_commit",
            Self::BeforeRecoveryRiskCommit => "before_recovery_risk_commit",
            Self::AfterRecoveryRiskCommit => "after_recovery_risk_commit",
            Self::BeforeRecoveryResolutionCommit => "before_recovery_resolution_commit",
            Self::AfterRecoveryResolutionCommit => "after_recovery_resolution_commit",
            Self::BeforeManualRecoveryCommit => "before_manual_recovery_commit",
            Self::AfterManualRecoveryCommit => "after_manual_recovery_commit",
            Self::AfterRecoveryOwnerLossProbe => "after_recovery_owner_loss_probe",
            Self::BeforeRunFinishCommit => "before_run_finish_commit",
            Self::AfterRunFinishCommit => "after_run_finish_commit",
        }
    }
}

#[cfg(not(test))]
pub(crate) fn fault(_point: FaultPoint) {}

#[cfg(test)]
pub(crate) fn fault(point: FaultPoint) {
    if std::env::var_os("PACTRUN_LIFECYCLE_SYNC").is_some_and(|value| value == point.name()) {
        let root = PathBuf::from(
            std::env::var_os("PACTRUN_LIFECYCLE_SYNC_DIR")
                .expect("lifecycle synchronization directory"),
        );
        std::fs::write(root.join("ready"), point.name()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        while !root.join("release").exists() {
            assert!(
                Instant::now() < deadline,
                "lifecycle synchronization timed out"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    if std::env::var_os("PACTRUN_OPERATION_TEST_SYNC").is_some_and(|value| value == point.name()) {
        let root = PathBuf::from(
            std::env::var_os("PACTRUN_OPERATION_TEST_SYNC_DIR")
                .expect("test synchronization directory"),
        );
        std::fs::write(root.join("ready"), point.name()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while !root.join("release").exists() {
            assert!(Instant::now() < deadline, "test synchronization timed out");
            thread::sleep(Duration::from_millis(10));
        }
    }
    if [
        "PACTRUN_LIFECYCLE_FAULT",
        "PACTRUN_REVISION_TEST_FAULT",
        "PACTRUN_METADATA_TEST_FAULT",
        "PACTRUN_RUN_TEST_FAULT",
        "PACTRUN_OPERATION_TEST_FAULT",
    ]
    .iter()
    .any(|variable| std::env::var_os(variable).is_some_and(|value| value == point.name()))
    {
        std::process::exit(87);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Cursor,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::Arc,
        thread,
    };

    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;
    use crate::domain::{
        ContentId, RevisionDeclarationInput, RuntimeContentProjectionInputV1, RuntimeFileKindV1,
        RuntimeFileV1, RuntimePath, project_revision_declarations,
        project_runtime_content_closure_v1, validate_declaration_content,
    };

    const WORKER_TEST: &str = "persistence::sqlite_revision_store::tests::revision_store_worker";

    // Test-ID: PR-TEST-0339
    // Verifies: PR-REQ-0018, PR-REQ-0318
    #[test]
    fn unsupported_core_is_not_interpreted_as_a_supported_revision() {
        let (_temporary, root) = test_root();
        drop(PactrunPersistence::open(&root).unwrap());
        let p = PactrunPersistence::open_read_only(&root).unwrap();
        let id = RevisionIdentity::new(
            package(65),
            crate::domain::RevisionContentDigest::from_bytes([65; 32]),
        );
        let db = Connection::open(database_path(&root)).unwrap();
        db.execute(
            "INSERT INTO packages VALUES(?1)",
            [id.package_id.as_bytes().as_slice()],
        )
        .unwrap();
        db.execute(
            "INSERT INTO revisions VALUES(?1,?2,?3,?4)",
            params![
                id.package_id.as_bytes().as_slice(),
                id.content_digest.as_bytes().as_slice(),
                br#"{"format_version":99}"#.as_slice(),
                br#"{"files":[]}"#.as_slice()
            ],
        )
        .unwrap();
        assert!(matches!(
            p.load_revision(&id),
            Err(PersistenceError::CorruptRevision(_))
        ));
        assert!(matches!(
            p.load_revision_metadata(&id),
            Err(PersistenceError::CorruptRevision(_))
        ));
    }

    fn digest(bytes: &[u8]) -> Sha256Digest {
        Sha256Digest::from_bytes(Sha256::digest(bytes).into())
    }

    fn package(tag: u8) -> PackageId {
        PackageId::from_bytes([tag; 16])
    }

    fn test_root() -> (TempDir, PathBuf) {
        let parent = std::env::var_os("PACTRUN_REVISION_TEST_TEST_PARENT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/revision-tests"));
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("sqlite-persistence-")
            .tempdir_in(parent)
            .unwrap();
        let root = temporary.path().join("pactrun");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join(DATABASE_DIRECTORY)).unwrap();
        fs::create_dir(root.join(RUNTIME_CONTENT_DIRECTORY)).unwrap();
        (temporary, root)
    }

    fn database_path(root: &Path) -> PathBuf {
        root.join(DATABASE_DIRECTORY).join(DATABASE_NAME)
    }

    fn final_blob_path(root: &Path, digest: &Sha256Digest) -> PathBuf {
        root.join(RUNTIME_CONTENT_DIRECTORY).join(
            digest
                .as_str()
                .strip_prefix("sha256:")
                .expect("typed digest prefix"),
        )
    }

    fn content_with_files(files: Vec<RuntimeFileV1>) -> DeclarationContent {
        let core = project_revision_declarations(RevisionDeclarationInput {
            inputs: Vec::new(),
            actions: Vec::new(),
            snapshot: None,
            migrations: Vec::new(),
            cleanup: None,
        })
        .unwrap();
        let runtime_content =
            project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files }).unwrap();
        validate_declaration_content(core, runtime_content).unwrap()
    }

    fn shared_content(blob_digest: &Sha256Digest) -> DeclarationContent {
        content_with_files(vec![
            RuntimeFileV1 {
                id: ContentId::parse("launcher").unwrap(),
                path: RuntimePath::parse("bin/launcher").unwrap(),
                kind: RuntimeFileKindV1::RegularFile,
                blob_digest: blob_digest.clone(),
                executable: true,
            },
            RuntimeFileV1 {
                id: ContentId::parse("script").unwrap(),
                path: RuntimePath::parse("hooks/script").unwrap(),
                kind: RuntimeFileKindV1::RegularFile,
                blob_digest: blob_digest.clone(),
                executable: false,
            },
        ])
    }

    fn one_file_content(id: &str, path: &str, blob_digest: &Sha256Digest) -> DeclarationContent {
        content_with_files(vec![RuntimeFileV1 {
            id: ContentId::parse(id).unwrap(),
            path: RuntimePath::parse(path).unwrap(),
            kind: RuntimeFileKindV1::RegularFile,
            blob_digest: blob_digest.clone(),
            executable: false,
        }])
    }

    fn raw_database(root: &Path) -> Connection {
        let database = Connection::open(database_path(root)).unwrap();
        database.pragma_update(None, "foreign_keys", "ON").unwrap();
        database
    }

    fn initialize_direct(root: &Path, application_id: i64, user_version: i64, sql: &str) {
        let database = Connection::open(database_path(root)).unwrap();
        database.execute_batch(sql).unwrap();
        database
            .pragma_update(None, "application_id", application_id)
            .unwrap();
        database
            .pragma_update(None, "user_version", user_version)
            .unwrap();
    }

    fn run_worker(root: &Path, operation: &str, fault: Option<FaultPoint>, marker: &Path) -> bool {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg(WORKER_TEST)
            .arg("--nocapture")
            .env("PACTRUN_REVISION_TEST_WORKER", operation)
            .env("PACTRUN_REVISION_TEST_ROOT", root)
            .env("PACTRUN_REVISION_TEST_SUCCESS_MARKER", marker)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(fault) = fault {
            command.env("PACTRUN_REVISION_TEST_FAULT", fault.name());
        }
        let output = command.output().unwrap();
        if !output.status.success() && fault.is_none() {
            eprintln!(
                "Revision worker failed with {}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        output.status.success()
    }

    fn persist_sample(
        persistence: &PactrunPersistence,
        package_id: PackageId,
        bytes: &[u8],
    ) -> (DeclarationContent, StoredRuntimeBlob, RevisionIdentity) {
        let blob_digest = digest(bytes);
        let publication = persistence
            .put_runtime_content(&blob_digest, &mut Cursor::new(bytes))
            .unwrap();
        let content = shared_content(&blob_digest);
        let identity = persistence
            .persist_revision(package_id, &content, std::slice::from_ref(&publication))
            .unwrap();
        (content, publication, identity)
    }

    // Test-ID: PR-TEST-0052
    // Verifies: PR-REQ-0231, PR-REQ-0234
    #[test]
    fn schema_bootstrap_is_exact_owned_and_concurrency_safe() {
        let (_temporary, root) = test_root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let database = persistence.database.lock().unwrap();
        assert_eq!(
            database
                .pragma_query_value(None, "application_id", |row| row.get::<_, i64>(0))
                .unwrap(),
            APPLICATION_ID
        );
        assert_eq!(
            database
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        assert_eq!(
            database
                .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "wal"
        );
        assert_eq!(
            database
                .pragma_query_value(None, "synchronous", |row| row.get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            database
                .pragma_query_value(None, "foreign_keys", |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        validate_schema(&database, SCHEMA_VERSION).unwrap();
        assert!(
            database
                .execute(
                    "INSERT INTO packages(package_id) VALUES (?1)",
                    [vec![0_u8; 15]],
                )
                .is_err()
        );
        database
            .execute("INSERT INTO packages(package_id) VALUES (?1)", [[7_u8; 16]])
            .unwrap();
        assert!(
            database
                .execute(
                    "INSERT INTO revisions(\
                    package_id, revision_content_digest, core_jcs, runtime_content_jcs\
                 ) VALUES (?1, ?2, ?3, ?4)",
                    params![[7_u8; 16], vec![0_u8; 31], b"{}", b"{}"],
                )
                .is_err()
        );
        drop(database);

        let bytes = b"schema length fixture";
        let (_, _, identity) = persist_sample(&persistence, package(8), bytes);
        let database = persistence.database.lock().unwrap();
        assert!(
            database
                .execute(
                    "INSERT INTO revision_runtime_content_refs(\
                    package_id, revision_content_digest, content_id, blob_digest\
                 ) VALUES (?1, ?2, 'invalid_length', ?3)",
                    params![
                        identity.package_id.as_bytes().as_slice(),
                        identity.content_digest.as_bytes().as_slice(),
                        vec![0_u8; 31],
                    ],
                )
                .is_err()
        );
        drop(database);

        let (_foreign_temporary, foreign_root) = test_root();
        initialize_direct(
            &foreign_root,
            0x1234,
            1,
            "CREATE TABLE foreign_data(value TEXT);",
        );
        assert!(PactrunPersistence::open(&foreign_root).is_err());

        let (_unmarked_temporary, unmarked_root) = test_root();
        initialize_direct(&unmarked_root, 0, 0, "CREATE TABLE unmarked(value TEXT);");
        assert!(PactrunPersistence::open(&unmarked_root).is_err());

        let (_newer_temporary, newer_root) = test_root();
        initialize_direct(
            &newer_root,
            APPLICATION_ID,
            SCHEMA_VERSION + 1,
            BASELINE_SQL,
        );
        assert!(PactrunPersistence::open(&newer_root).is_err());

        let (_drift_temporary, drift_root) = test_root();
        initialize_direct(
            &drift_root,
            APPLICATION_ID,
            SCHEMA_VERSION,
            &format!("{BASELINE_SQL}\nCREATE TABLE drift(value TEXT) STRICT;"),
        );
        assert!(PactrunPersistence::open(&drift_root).is_err());

        for fault in [
            FaultPoint::AfterWalBeforeBootstrap,
            FaultPoint::BeforeBootstrapCommit,
        ] {
            let (temporary, crash_root) = test_root();
            let marker = temporary.path().join("bootstrap.success");
            assert!(!run_worker(&crash_root, "open", Some(fault), &marker));
            assert!(!marker.exists());
            PactrunPersistence::open(&crash_root).unwrap();
        }

        let (concurrent_temporary, concurrent_root) = test_root();
        let workers = (0..2)
            .map(|index| {
                let root = concurrent_root.clone();
                let marker = concurrent_temporary
                    .path()
                    .join(format!("bootstrap-{index}.success"));
                thread::spawn(move || (run_worker(&root, "open", None, &marker), marker))
            })
            .collect::<Vec<_>>();
        for worker in workers {
            let (success, marker) = worker.join().unwrap();
            assert!(success);
            assert!(marker.exists());
        }
        PactrunPersistence::open(&concurrent_root).unwrap();
    }

    // Test-ID: PR-TEST-0053
    // Verifies: PR-REQ-0013, PR-REQ-0232
    #[test]
    fn exact_revision_components_and_derived_references_round_trip() {
        let (_temporary, root) = test_root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let bytes = b"shared canonical blob";
        let (content, publication, identity) = persist_sample(&persistence, package(1), bytes);

        let expected_core = encode_service_free_revision(&content.core).unwrap();
        let expected_runtime = encode_canonical_runtime_content(&content.runtime_content).unwrap();
        let database = persistence.database.lock().unwrap();
        let stored: (Vec<u8>, Vec<u8>) = database
            .query_row(
                "SELECT core_jcs, runtime_content_jcs FROM revisions \
                 WHERE package_id=?1 AND revision_content_digest=?2",
                params![
                    identity.package_id.as_bytes().as_slice(),
                    identity.content_digest.as_bytes().as_slice(),
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (expected_core.clone(), expected_runtime.clone()));
        assert_eq!(
            load_reference_rows(&database, &identity).unwrap(),
            derive_references(&content.runtime_content)
        );
        drop(database);

        let loaded = persistence.load_revision(&identity).unwrap().unwrap();
        assert_eq!(loaded.identity, identity);
        assert_eq!(loaded.content, content.clone().into());
        assert_eq!(
            calculate_revision_content_digest(&loaded.content).unwrap(),
            identity.content_digest
        );

        let other_package = persistence
            .persist_revision(package(2), &content, std::slice::from_ref(&publication))
            .unwrap();
        assert_eq!(other_package.content_digest, identity.content_digest);
        assert_ne!(other_package.package_id, identity.package_id);

        let other_bytes = b"different canonical blob";
        let other_digest = digest(other_bytes);
        let other_publication = persistence
            .put_runtime_content(&other_digest, &mut Cursor::new(other_bytes))
            .unwrap();
        let other_content = one_file_content("other", "lib/other", &other_digest);
        let other_revision = persistence
            .persist_revision(
                identity.package_id,
                &other_content,
                std::slice::from_ref(&other_publication),
            )
            .unwrap();
        assert_ne!(other_revision.content_digest, identity.content_digest);
        assert_eq!(
            expected_core,
            encode_service_free_revision(&content.core).unwrap()
        );
        assert_eq!(
            expected_runtime,
            encode_canonical_runtime_content(&content.runtime_content).unwrap()
        );
    }

    // Test-ID: PR-TEST-0054
    // Verifies: PR-REQ-0013, PR-REQ-0232, PR-REQ-0234
    #[test]
    fn persisted_revision_records_are_immutable_and_exactly_idempotent() {
        let (_temporary, root) = test_root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let (content, _, identity) = persist_sample(&persistence, package(3), b"immutable");
        assert_eq!(
            persistence
                .persist_revision(identity.package_id, &content, &[])
                .unwrap(),
            identity
        );

        for mutation in ["core", "runtime", "reference"] {
            let (_temporary, root) = test_root();
            let persistence = PactrunPersistence::open(&root).unwrap();
            let (content, _, identity) =
                persist_sample(&persistence, package(4), mutation.as_bytes());
            let database = persistence.database.lock().unwrap();
            match mutation {
                "core" => {
                    database
                        .execute(
                            "UPDATE revisions SET core_jcs=?3 \
                             WHERE package_id=?1 AND revision_content_digest=?2",
                            params![
                                identity.package_id.as_bytes().as_slice(),
                                identity.content_digest.as_bytes().as_slice(),
                                b"{}",
                            ],
                        )
                        .unwrap();
                }
                "runtime" => {
                    database
                        .execute(
                            "UPDATE revisions SET runtime_content_jcs=?3 \
                             WHERE package_id=?1 AND revision_content_digest=?2",
                            params![
                                identity.package_id.as_bytes().as_slice(),
                                identity.content_digest.as_bytes().as_slice(),
                                b"{\"files\":[]}",
                            ],
                        )
                        .unwrap();
                }
                "reference" => {
                    database
                        .execute(
                            "UPDATE revision_runtime_content_refs SET blob_digest=?3 \
                             WHERE package_id=?1 AND revision_content_digest=?2",
                            params![
                                identity.package_id.as_bytes().as_slice(),
                                identity.content_digest.as_bytes().as_slice(),
                                [0x55_u8; 32],
                            ],
                        )
                        .unwrap();
                }
                _ => unreachable!(),
            }
            drop(database);
            assert!(
                persistence
                    .persist_revision(identity.package_id, &content, &[])
                    .is_err()
            );
        }
    }

    // Test-ID: PR-TEST-0055
    // Verifies: PR-REQ-0232, PR-REQ-0233
    #[test]
    fn publication_witnesses_authorize_only_distinct_physical_digests() {
        let (_temporary, root) = test_root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let shared_bytes = b"one witness two content ids";
        let shared_digest = digest(shared_bytes);
        let publication = persistence
            .put_runtime_content(&shared_digest, &mut Cursor::new(shared_bytes))
            .unwrap();
        let content = shared_content(&shared_digest);

        let exact = persistence
            .persist_revision(package(10), &content, std::slice::from_ref(&publication))
            .unwrap();
        assert_eq!(
            load_reference_rows(&persistence.database.lock().unwrap(), &exact)
                .unwrap()
                .len(),
            2
        );
        assert!(
            persistence
                .persist_revision(package(11), &content, &[])
                .is_err()
        );
        assert!(
            persistence
                .persist_revision(
                    package(12),
                    &content,
                    &[publication.clone(), publication.clone()]
                )
                .is_err()
        );

        let extra_bytes = b"extra witness";
        let extra_digest = digest(extra_bytes);
        let extra = persistence
            .put_runtime_content(&extra_digest, &mut Cursor::new(extra_bytes))
            .unwrap();
        assert!(
            persistence
                .persist_revision(package(13), &content, &[publication.clone(), extra])
                .is_err()
        );

        let (_other_temporary, other_root) = test_root();
        let other = PactrunPersistence::open(&other_root).unwrap();
        let wrong_store = other
            .put_runtime_content(&shared_digest, &mut Cursor::new(shared_bytes))
            .unwrap();
        assert!(
            persistence
                .persist_revision(package(14), &content, &[wrong_store])
                .is_err()
        );

        let empty = content_with_files(Vec::new());
        persistence
            .persist_revision(package(15), &empty, &[])
            .unwrap();
        assert!(
            schema_objects(&persistence.database.lock().unwrap())
                .unwrap()
                .iter()
                .all(|object| !object
                    .sql
                    .as_deref()
                    .unwrap_or_default()
                    .contains("witness"))
        );
    }

    // Test-ID: PR-TEST-0056
    // Verifies: PR-REQ-0231, PR-REQ-0233, PR-REQ-0234
    #[test]
    fn concurrent_and_crashed_transactions_recover_without_old_witnesses() {
        let (_temporary, root) = test_root();
        let persistence = Arc::new(PactrunPersistence::open(&root).unwrap());
        let bytes = b"threaded Revision";
        let blob_digest = digest(bytes);
        let publication = persistence
            .put_runtime_content(&blob_digest, &mut Cursor::new(bytes))
            .unwrap();
        let content = shared_content(&blob_digest);
        let writers = (0..8)
            .map(|_| {
                let persistence = Arc::clone(&persistence);
                let content = content.clone();
                let publication = publication.clone();
                thread::spawn(move || {
                    persistence.persist_revision(package(20), &content, &[publication])
                })
            })
            .collect::<Vec<_>>();
        let identities = writers
            .into_iter()
            .map(|writer| writer.join().unwrap().unwrap())
            .collect::<Vec<_>>();
        assert!(identities.windows(2).all(|pair| pair[0] == pair[1]));

        let (process_temporary, process_root) = test_root();
        let processes = (0..3)
            .map(|index| {
                let root = process_root.clone();
                let marker = process_temporary
                    .path()
                    .join(format!("writer-{index}.success"));
                thread::spawn(move || (run_worker(&root, "persist", None, &marker), marker))
            })
            .collect::<Vec<_>>();
        for process in processes {
            let (success, marker) = process.join().unwrap();
            assert!(success);
            assert!(marker.exists());
        }

        let expected_content = shared_content(&digest(b"revision worker content"));
        let expected_identity = RevisionIdentity::new(
            package(42),
            calculate_service_free_digest(&expected_content).unwrap(),
        );
        let reopened = PactrunPersistence::open(&process_root).unwrap();
        assert!(
            reopened
                .load_revision(&expected_identity)
                .unwrap()
                .is_some()
        );

        for fault in [
            FaultPoint::BeforeRevisionCommit,
            FaultPoint::AfterRevisionCommit,
        ] {
            let (temporary, crash_root) = test_root();
            let marker = temporary.path().join("revision.success");
            assert!(!run_worker(&crash_root, "persist", Some(fault), &marker));
            assert!(!marker.exists());
            let reopened = PactrunPersistence::open(&crash_root).unwrap();
            let content = shared_content(&digest(b"revision worker content"));
            let identity = RevisionIdentity::new(
                package(42),
                calculate_service_free_digest(&content).unwrap(),
            );
            match fault {
                FaultPoint::BeforeRevisionCommit => {
                    assert!(reopened.load_revision(&identity).unwrap().is_none());
                    assert!(
                        final_blob_path(&crash_root, &digest(b"revision worker content")).exists()
                    );
                }
                FaultPoint::AfterRevisionCommit => {
                    assert!(reopened.load_revision(&identity).unwrap().is_some());
                    assert_eq!(
                        reopened
                            .persist_revision(identity.package_id, &content, &[])
                            .unwrap(),
                        identity
                    );
                }
                _ => unreachable!(),
            }
        }
    }

    // Test-ID: PR-TEST-0057
    // Verifies: PR-REQ-0232, PR-REQ-0234
    #[test]
    fn logical_revision_integrity_is_separate_from_physical_blob_availability() {
        for physical_state in ["missing", "corrupt"] {
            let (_temporary, root) = test_root();
            let persistence = PactrunPersistence::open(&root).unwrap();
            let bytes = physical_state.as_bytes();
            let (content, _, identity) = persist_sample(&persistence, package(30), bytes);
            let blob_path = final_blob_path(&root, &digest(bytes));
            if physical_state == "missing" {
                fs::remove_file(blob_path).unwrap();
            } else {
                fs::write(blob_path, b"externally corrupted").unwrap();
            }
            let loaded = persistence.load_revision(&identity).unwrap().unwrap();
            assert_eq!(loaded.content, content.clone().into());
            assert!(
                persistence
                    .verify_revision_runtime_content(&identity)
                    .is_err()
            );
        }

        let (_canonical_temporary, canonical_root) = test_root();
        let canonical = PactrunPersistence::open(&canonical_root).unwrap();
        let (content, _, identity) = persist_sample(&canonical, package(31), b"canonical");
        let before_core = encode_service_free_revision(&content.core).unwrap();
        let before_runtime = encode_canonical_runtime_content(&content.runtime_content).unwrap();
        let before_digest = calculate_service_free_digest(&content).unwrap();
        let loaded = canonical.load_revision(&identity).unwrap().unwrap();
        assert_eq!(
            encode_canonical_revision_core(&loaded.content.core).unwrap(),
            before_core
        );
        assert_eq!(
            encode_canonical_runtime_content(&loaded.content.runtime_content).unwrap(),
            before_runtime
        );
        assert_eq!(
            calculate_revision_content_digest(&loaded.content).unwrap(),
            before_digest
        );

        for corruption in ["canonical", "references"] {
            let (_temporary, root) = test_root();
            let persistence = PactrunPersistence::open(&root).unwrap();
            let (_, _, identity) = persist_sample(&persistence, package(32), corruption.as_bytes());
            let database = persistence.database.lock().unwrap();
            if corruption == "canonical" {
                database
                    .execute(
                        "UPDATE revisions SET core_jcs=?3 \
                         WHERE package_id=?1 AND revision_content_digest=?2",
                        params![
                            identity.package_id.as_bytes().as_slice(),
                            identity.content_digest.as_bytes().as_slice(),
                            b"{}",
                        ],
                    )
                    .unwrap();
            } else {
                database
                    .execute(
                        "UPDATE revision_runtime_content_refs SET blob_digest=?3 \
                         WHERE package_id=?1 AND revision_content_digest=?2",
                        params![
                            identity.package_id.as_bytes().as_slice(),
                            identity.content_digest.as_bytes().as_slice(),
                            [0x77_u8; 32],
                        ],
                    )
                    .unwrap();
            }
            drop(database);
            assert!(persistence.load_revision(&identity).is_err());
        }

        let vectors: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/vectors/revision_canonical/declarations.json"
        ))
        .unwrap();
        let catalog: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/vectors/error_taxonomy_v1/catalog.json"
        ))
        .unwrap();
        assert_eq!(vectors["status"], "baseline");
        assert_eq!(catalog["status"], "frozen");
    }

    #[test]
    fn revision_store_worker() {
        let Some(operation) = std::env::var_os("PACTRUN_REVISION_TEST_WORKER") else {
            return;
        };
        let root = PathBuf::from(std::env::var_os("PACTRUN_REVISION_TEST_ROOT").unwrap());
        let marker =
            PathBuf::from(std::env::var_os("PACTRUN_REVISION_TEST_SUCCESS_MARKER").unwrap());
        let persistence = PactrunPersistence::open(&root).unwrap();
        match operation.to_string_lossy().as_ref() {
            "open" => {}
            "persist" => {
                let bytes = b"revision worker content";
                let blob_digest = digest(bytes);
                let publication = persistence
                    .put_runtime_content(&blob_digest, &mut Cursor::new(bytes))
                    .unwrap();
                let content = shared_content(&blob_digest);
                persistence
                    .persist_revision(package(42), &content, &[publication])
                    .unwrap();
            }
            other => panic!("unknown Revision worker operation {other}"),
        }
        fs::write(marker, b"revision-operation-returned-success").unwrap();
    }
}
