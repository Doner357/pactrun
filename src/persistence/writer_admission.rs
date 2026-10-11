//! Admission-aware writer qualification for the supported Persistence baseline.
//! Session preparation is not admission; SQLite is the shared serialized boundary.

#[cfg(test)]
use std::fs;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use super::sqlite_revision_store::{
    APPLICATION_ID, BASELINE_SQL, DatabaseState, FaultPoint, SCHEMA_VERSION, classify_database,
    configure_connection, establish_wal_mode, fault, validate_schema,
};
use super::{PactrunPersistence, PersistenceError};
use crate::{
    domain::{ExecutionOwnerSession, InstanceId, ManagedExecutionKind},
    managed_data::{SessionOwnerProbe, StagingSession, probe_session_owner},
};

fn unsupported_source(state: DatabaseState) -> PersistenceError {
    PersistenceError::DatabaseOwnership(format!(
        "unsupported persistence state {state:?}; development stores are not upgraded"
    ))
}

pub(super) fn open_writer_database(
    path: &Path,
    session: &StagingSession,
) -> Result<Connection, PersistenceError> {
    let mut database =
        Connection::open(path).map_err(|e| PersistenceError::sqlite("open writer database", e))?;
    configure_connection(&database)?;
    let advisory = classify_database(&database)?;
    if !matches!(advisory, DatabaseState::Pristine | DatabaseState::Baseline) {
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
            transaction
                .execute_batch(BASELINE_SQL)
                .map_err(|e| PersistenceError::sqlite("initialize persistence baseline", e))?;
            validate_schema(&transaction, SCHEMA_VERSION)?;
            transaction
                .pragma_update(None, "application_id", APPLICATION_ID)
                .map_err(|e| PersistenceError::sqlite("initialize application ID", e))?;
            transaction
                .pragma_update(None, "user_version", SCHEMA_VERSION)
                .map_err(|e| PersistenceError::sqlite("initialize current version", e))?;
            fault(FaultPoint::BeforeBootstrapCommit);
        }
        DatabaseState::Baseline => {}
        state @ (DatabaseState::Alpha1 | DatabaseState::Alpha2 | DatabaseState::Alpha3) => {
            return Err(unsupported_source(state));
        }
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
    if actual != DatabaseState::Baseline || expected != SCHEMA_VERSION {
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
    let deletion_count: i64 = database
        .query_row(
            "SELECT count(*) FROM run_deletion_invocations WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .map_err(|e| PersistenceError::sqlite("validate deletion invocation", e))?;
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
#[path = "writer_admission_tests.rs"]
mod tests;
