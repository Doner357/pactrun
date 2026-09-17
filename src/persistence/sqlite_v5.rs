//! Admission-aware writer qualification, introduced in V5 and retained in V6.
//! Session preparation is not admission; SQLite is the shared serialized boundary.

#[cfg(test)]
use std::fs;
use std::path::Path;

#[cfg(test)]
use rusqlite::OpenFlags;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

#[cfg(test)]
use super::runtime_content_store::{
    validate_existing_regular_entry, validate_supported_storage_root,
};
use super::sqlite_revision_store::{
    APPLICATION_ID, DatabaseState, FaultPoint, SCHEMA_LADDER, SCHEMA_VERSION, classify_database,
    configure_connection, establish_wal_mode, fault, validate_schema,
};
#[cfg(test)]
use super::sqlite_revision_store::{
    SCHEMA_V4_VERSION, SCHEMA_V5_ADDITIONS_SQL, SCHEMA_V5_VERSION, configure_read_connection,
};
use super::{PactrunPersistence, PersistenceError};
use crate::{
    domain::{ExecutionOwnerSession, InstanceId, ManagedExecutionKind},
    managed_data::{SessionOwnerProbe, StagingSession, probe_session_owner},
};

fn unsupported_source(state: DatabaseState) -> PersistenceError {
    match state {
        DatabaseState::V8 => PersistenceError::UpgradeRequired,
        _ => PersistenceError::DatabaseOwnership("this build accepts pristine or exact V9 storage; use a compatible build to reach exact V8 before explicit upgrade".to_owned()),
    }
}

pub(super) fn open_writer_database(
    path: &Path,
    session: &StagingSession,
) -> Result<Connection, PersistenceError> {
    let mut database =
        Connection::open(path).map_err(|e| PersistenceError::sqlite("open writer database", e))?;
    configure_connection(&database)?;
    let advisory = classify_database(&database)?;
    if !matches!(advisory, DatabaseState::Pristine | DatabaseState::V9) {
        return Err(unsupported_source(advisory));
    }
    establish_wal_mode(&database)?;
    configure_connection(&database)?;
    fault(FaultPoint::AfterWalBeforeBootstrap);
    fault(FaultPoint::BeforeWritableAdmission);
    let transaction = database
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| PersistenceError::sqlite("serialize writer admission", e))?;
    // The advisory check above never grants write permission.
    match classify_database(&transaction)? {
        DatabaseState::Pristine => {
            for (sql, _) in &SCHEMA_LADDER {
                transaction
                    .execute_batch(sql)
                    .map_err(|e| PersistenceError::sqlite("initialize current schema", e))?;
            }
            validate_schema(&transaction, SCHEMA_VERSION)?;
            transaction
                .pragma_update(None, "application_id", APPLICATION_ID)
                .map_err(|e| PersistenceError::sqlite("initialize application ID", e))?;
            transaction
                .pragma_update(None, "user_version", SCHEMA_VERSION)
                .map_err(|e| PersistenceError::sqlite("initialize current version", e))?;
            fault(FaultPoint::BeforeBootstrapCommit);
        }
        DatabaseState::V9 => {}
        other => return Err(unsupported_source(other)),
    }
    transaction.execute(
        "INSERT INTO writable_admissions(owner_session, admitted_schema_version) VALUES (?1, ?2)",
        params![session.owner().as_str().as_bytes(), SCHEMA_VERSION],
    ).map_err(|e| PersistenceError::sqlite("register writable admission", e))?;
    transaction
        .commit()
        .map_err(|e| PersistenceError::sqlite("commit writable admission", e))?;
    fault(FaultPoint::AfterWritableAdmission);
    Ok(database)
}

/// Called after acquiring a write transaction, never just before locking.
fn require_current_version(database: &Connection) -> Result<(), PersistenceError> {
    let application: i64 = database
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(|e| PersistenceError::sqlite("revalidate application ID", e))?;
    let version: i64 = database
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| PersistenceError::sqlite("revalidate writer schema version", e))?;
    if application != APPLICATION_ID || version != SCHEMA_VERSION {
        return Err(PersistenceError::SchemaMismatch(
            "writer does not support the current exact schema version".to_owned(),
        ));
    }
    // A matching marker is not proof of an exact schema: reject same-version drift.
    validate_schema(database, SCHEMA_VERSION)
}

impl PactrunPersistence {
    pub(crate) fn staging_session(&self) -> Option<&StagingSession> {
        self.session.as_ref()
    }

    pub(super) fn check_write_admission(
        &self,
        database: &Transaction<'_>,
    ) -> Result<(), PersistenceError> {
        require_current_version(database)?;
        let owner = self
            .session
            .as_ref()
            .ok_or(PersistenceError::WriterAdmissionRequired)?
            .owner();
        let admitted: Option<i64> = database
            .query_row(
                "SELECT admitted_schema_version FROM writable_admissions WHERE owner_session=?1",
                [owner.as_str().as_bytes()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| PersistenceError::sqlite("revalidate writable admission", e))?;
        if admitted != Some(SCHEMA_VERSION) {
            return Err(PersistenceError::WriterAdmissionRequired);
        }
        Ok(())
    }

    pub(crate) fn recovery_consequence_version(
        &self,
        instance: InstanceId,
    ) -> Result<i64, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        load_consequence_version(&database, instance)
    }

    /// Historical M4 implementation, retained only to verify V4->V5 fixtures.
    #[cfg(test)]
    pub(super) fn upgrade_legacy_v4_to_v5(root: &Path) -> Result<bool, PersistenceError> {
        let root = validate_supported_storage_root(root)?;
        let database_root = validate_supported_storage_root(&root.join("database"))?;
        validate_existing_regular_entry(&database_root, "pactrun.sqlite3")?;
        let path = database_root.join("pactrun.sqlite3");
        let advisory = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| PersistenceError::sqlite("inspect upgrade source", e))?;
        configure_read_connection(&advisory)?;
        match classify_database(&advisory)? {
            DatabaseState::V5 => return Ok(false),
            DatabaseState::V4 => {}
            other => return Err(unsupported_source(other)),
        }
        drop(advisory);
        let session = StagingSession::prepare(&root).map_err(|_| {
            PersistenceError::DatabaseOwnership("could not prepare migration session".to_owned())
        })?;
        let mut database = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| PersistenceError::sqlite("open legacy bootstrap", e))?;
        configure_connection(&database)?;
        // Do not trust the read-only precheck if another migrator won meanwhile.
        establish_wal_mode(&database)?;
        configure_connection(&database)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::sqlite("serialize exact V4 bootstrap", e))?;
        match classify_database(&transaction)? {
            DatabaseState::V5 => {
                transaction.commit().map_err(|e| {
                    PersistenceError::sqlite("close concurrent upgrade observation", e)
                })?;
                return Ok(false);
            }
            DatabaseState::V4 => {}
            other => return Err(unsupported_source(other)),
        }
        validate_schema(&transaction, SCHEMA_V4_VERSION)?;
        inspect_legacy_sessions(&root, &session.owner())?;
        fault(FaultPoint::AfterLegacySessionInspection);
        transaction
            .execute_batch(SCHEMA_V5_ADDITIONS_SQL)
            .map_err(|e| PersistenceError::sqlite("add V5 schema", e))?;
        transaction.execute("INSERT INTO run_operation_kinds(run_id, operation_kind) SELECT run_id, 0 FROM runs", [])
            .map_err(|e| PersistenceError::sqlite("preserve legacy Action operation kinds", e))?;
        transaction.execute("INSERT INTO instance_recovery_consequence_versions(instance_id, consequence_version) SELECT instance_id, 0 FROM instances", [])
            .map_err(|e| PersistenceError::sqlite("initialize consequence baselines", e))?;
        validate_schema(&transaction, SCHEMA_V5_VERSION)?;
        transaction
            .pragma_update(None, "user_version", SCHEMA_V5_VERSION)
            .map_err(|e| PersistenceError::sqlite("publish V5 version", e))?;
        fault(FaultPoint::BeforeSchemaMigrationCommit);
        transaction
            .commit()
            .map_err(|e| PersistenceError::sqlite("commit exact V4 bootstrap", e))?;
        fault(FaultPoint::AfterSchemaMigrationCommit);
        Ok(true)
    }

    #[cfg(test)]
    pub(crate) fn abandon_execution_owner(mut self) {
        // No writer can access this consumed value again. Keep the durable row
        // to model a process death; the unheld lease establishes owner loss.
        self.session.take().expect("writable owner").abandon();
    }
}

impl Drop for PactrunPersistence {
    fn drop(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        let database = self.database.get_mut().unwrap_or_else(|e| e.into_inner());
        // Exclusive Drop means no future schema-dependent call can occur.
        if let Ok(transaction) = database.transaction_with_behavior(TransactionBehavior::Immediate)
            && require_current_version(&transaction).is_ok()
            && transaction
                .execute(
                    "DELETE FROM writable_admissions WHERE owner_session=?1",
                    [session.owner().as_str().as_bytes()],
                )
                .is_ok()
        {
            let _ = transaction.commit();
        }
        // Connection fields are destroyed before the session/lease field.
    }
}

/// Only the exact V4->V5 bootstrap may conservatively inspect all session leases.
#[cfg(test)]
fn inspect_legacy_sessions(
    root: &Path,
    migrator: &ExecutionOwnerSession,
) -> Result<(), PersistenceError> {
    for entry in fs::read_dir(root.join("staging")).map_err(|source| PersistenceError::Io {
        operation: "inspect legacy sessions",
        source,
    })? {
        let entry = entry.map_err(|source| PersistenceError::Io {
            operation: "read legacy session entry",
            source,
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with("session-") {
            continue;
        }
        let owner = ExecutionOwnerSession::parse(name.to_owned())
            .map_err(|_| PersistenceError::LegacySessionUncertain)?;
        if owner != *migrator
            && probe_session_owner(root, &owner) != SessionOwnerProbe::ConfirmedLoss
        {
            return Err(PersistenceError::LegacySessionUncertain);
        }
    }
    Ok(())
}

/// Admission-aware migration gate. Must be called under its write transaction.
/// There is deliberately no filesystem enumeration and no missing-row fallback.
pub(super) fn require_quiescent_admissions(
    transaction: &Transaction<'_>,
    root: &Path,
) -> Result<(), PersistenceError> {
    require_quiescent_admissions_at_version(transaction, root, SCHEMA_VERSION)
}

pub(super) fn require_quiescent_admissions_at_version(
    transaction: &Transaction<'_>,
    root: &Path,
    expected: i64,
) -> Result<(), PersistenceError> {
    let actual = classify_database(transaction)?;
    if !matches!(
        (actual, expected),
        (DatabaseState::V5, 5)
            | (DatabaseState::V6, 6)
            | (DatabaseState::V7, 7)
            | (DatabaseState::V8, 8)
            | (DatabaseState::V9, 9)
    ) {
        return Err(PersistenceError::SchemaMismatch(
            "unexpected writer-admission source version".to_owned(),
        ));
    }
    let mut query = transaction.prepare("SELECT owner_session, admitted_schema_version FROM writable_admissions ORDER BY owner_session")
        .map_err(|e| PersistenceError::sqlite("inspect admitted writers", e))?;
    let rows = query
        .query_map([], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| PersistenceError::sqlite("read admitted writers", e))?;
    for row in rows {
        let (name, version) =
            row.map_err(|e| PersistenceError::sqlite("read admitted writer", e))?;
        let owner = String::from_utf8(name)
            .ok()
            .and_then(|s| ExecutionOwnerSession::parse(s).ok())
            .ok_or_else(|| {
                PersistenceError::SchemaMismatch("invalid durable writer identity".to_owned())
            })?;
        if version != expected {
            return Err(PersistenceError::SchemaMismatch(
                "invalid admitted schema version".to_owned(),
            ));
        }
        if probe_session_owner(root, &owner) != SessionOwnerProbe::ConfirmedLoss {
            return Err(PersistenceError::ActiveWriters);
        }
    }
    Ok(())
}

pub(super) fn load_consequence_version(
    database: &Connection,
    instance: InstanceId,
) -> Result<i64, PersistenceError> {
    let value: Option<i64> = database.query_row(
        "SELECT consequence_version FROM instance_recovery_consequence_versions WHERE instance_id=?1",
        [instance.as_bytes().as_slice()], |row| row.get(0),
    ).optional().map_err(|e| PersistenceError::sqlite("load recovery consequence version", e))?;
    value.filter(|value| *value >= 0).ok_or_else(|| {
        PersistenceError::CorruptInstance(
            "missing or invalid recovery consequence version".to_owned(),
        )
    })
}

pub(super) fn advance_consequence_version(
    transaction: &Transaction<'_>,
    instance: InstanceId,
) -> Result<(), PersistenceError> {
    let next = load_consequence_version(transaction, instance)?
        .checked_add(1)
        .ok_or_else(|| {
            PersistenceError::InvalidRunTransition(
                "recovery consequence version exhausted".to_owned(),
            )
        })?;
    transaction.execute("UPDATE instance_recovery_consequence_versions SET consequence_version=?2 WHERE instance_id=?1",
        params![instance.as_bytes().as_slice(), next])
        .map_err(|e| PersistenceError::sqlite("advance recovery consequence version", e))?;
    Ok(())
}

pub(super) fn validate_run_operation(
    database: &Connection,
    run: crate::domain::RunId,
) -> Result<ManagedExecutionKind, PersistenceError> {
    let row: Option<(i64, i64, i64, i64, i64)> = database.query_row(
        "SELECT operation_kind, (SELECT count(*) FROM run_action_invocations WHERE run_id=?1), (SELECT count(*) FROM run_capture_invocations WHERE run_id=?1), (SELECT count(*) FROM run_restore_invocations WHERE run_id=?1), (SELECT count(*) FROM run_migration_invocations WHERE run_id=?1) FROM run_operation_kinds WHERE run_id=?1",
        [run.as_bytes().as_slice()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
    ).optional().map_err(|e| PersistenceError::sqlite("validate Run operation kind", e))?;
    // Historical fixtures have no V8 table. Production readers already enforce
    // exact V9, while historical readers retain their original contract.
    let has_deletions: bool = database.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='run_deletion_invocations')", [], |r| r.get(0),
    ).map_err(|e| PersistenceError::sqlite("inspect deletion invocation schema", e))?;
    let deletion_count: i64 = if has_deletions {
        database
            .query_row(
                "SELECT count(*) FROM run_deletion_invocations WHERE run_id=?1",
                [run.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| PersistenceError::sqlite("validate deletion invocation", e))?
    } else {
        0
    };
    if deletion_count != 0 {
        return match (row, deletion_count) {
            (Some((4, 0, 0, 0, 0)), 1) => Ok(ManagedExecutionKind::Deletion),
            _ => Err(PersistenceError::CorruptRun(
                "deletion must have exactly one matching invocation".to_owned(),
            )),
        };
    }
    match row {
        Some((0, 1, 0, 0, 0)) => Ok(ManagedExecutionKind::Action),
        Some((1, 0, 1, 0, 0)) => Ok(ManagedExecutionKind::SnapshotCapture),
        Some((2, 0, 0, 1, 0)) => Ok(ManagedExecutionKind::SnapshotRestore),
        Some((3, 0, 0, 0, 1)) => Ok(ManagedExecutionKind::Migration),
        _ => Err(PersistenceError::CorruptRun(
            "Run must have exactly one matching operation and invocation".to_owned(),
        )),
    }
}

#[cfg(test)]
#[path = "sqlite_v5_tests.rs"]
mod tests;
