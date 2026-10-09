//! Atomic, additive upgrades of the exact supported earlier storage schemas.
use super::{
    PersistenceError, local_catalog,
    sqlite_revision_store::{
        APPLICATION_ID, DatabaseState, SCHEMA_VERSION, classify_database, configure_connection,
        validate_schema,
    },
};
use crate::{
    domain::{ExecutionOwnerSession, LocalName},
    managed_data::{SessionOwnerProbe, probe_session_owner},
};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::path::Path;

fn sql(error: rusqlite::Error) -> PersistenceError {
    PersistenceError::sqlite("upgrade local management catalog", error)
}

#[cfg(test)]
#[path = "schema_upgrade_tests.rs"]
mod tests;

pub(super) fn upgrade(root: &Path) -> Result<(), PersistenceError> {
    let mut db = Connection::open_with_flags(
        root.join("database/pactrun.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .map_err(sql)?;
    configure_connection(&db)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql)?;
    let old_version = match classify_database(&tx)? {
        DatabaseState::Baseline => return Ok(()),
        DatabaseState::Alpha1 => 0,
        DatabaseState::Alpha2 => 1,
        DatabaseState::Pristine => {
            return Err(PersistenceError::SchemaMismatch(
                "upgrade requires an existing supported Store".into(),
            ));
        }
    };
    // Old admitted writers validate their version again under every write lock.
    // Do not cross the schema boundary while a live or unprobeable owner exists.
    {
        let mut statement = tx
            .prepare("SELECT owner_session,admitted_schema_version FROM writable_admissions")
            .map_err(sql)?;
        let rows = statement
            .query_map([], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?)))
            .map_err(sql)?;
        for row in rows {
            let (bytes, version) = row.map_err(sql)?;
            let owner = String::from_utf8(bytes)
                .ok()
                .and_then(|v| ExecutionOwnerSession::parse(v).ok())
                .ok_or_else(|| {
                    PersistenceError::SchemaMismatch("invalid old writer identity".into())
                })?;
            if version != old_version {
                return Err(PersistenceError::SchemaMismatch(
                    "invalid old writer version".into(),
                ));
            }
            if probe_session_owner(root, &owner) != SessionOwnerProbe::ConfirmedLoss {
                return Err(PersistenceError::ActiveWriters);
            }
        }
    }
    // Reject lossy mappings before creating tables. The old Store remains usable.
    if old_version == 0 {
        let mut statement=tx.prepare("SELECT package_id,revision_content_digest,alias_utf8 FROM revision_local_aliases ORDER BY package_id,revision_content_digest,alias_utf8").map_err(sql)?;
        let rows = statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, Vec<u8>>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(sql)?;
        let mut previous = None;
        for row in rows {
            let (p, r, alias) = row.map_err(sql)?;
            if previous.as_ref() == Some(&(p.clone(), r.clone())) {
                return Err(PersistenceError::MetadataConflict("Store upgrade needs one alias per Revision; remove extra aliases with the previous Pactrun version".into()));
            }
            String::from_utf8(alias).ok().and_then(|s|LocalName::parse(s).ok()).ok_or_else(||PersistenceError::MetadataConflict("Store upgrade needs 1..31-character ASCII local names; rename unsupported aliases with the previous Pactrun version".into()))?;
            previous = Some((p, r));
        }
    }
    if old_version == 0 {
        tx.execute_batch(local_catalog::SCHEMA).map_err(sql)?;
        tx.execute("INSERT INTO revision_local_names(package_id,revision_content_digest,name) SELECT package_id,revision_content_digest,CAST(alias_utf8 AS TEXT) FROM revision_local_aliases",[]).map_err(sql)?;
        tx.execute("INSERT INTO revision_installations(package_id,revision_content_digest,installed_at_unix_ms) SELECT package_id,revision_content_digest,NULL FROM revisions",[]).map_err(sql)?;
    }
    tx.execute_batch(super::sqlite_revision_store::FAILURE_CAUSES_SQL)
        .map_err(sql)?;
    // Empty owner rows are bookkeeping, not Run history. Live owners were refused.
    tx.execute_batch("DROP TABLE writable_admissions;")
        .map_err(sql)?;
    let admission_schema = super::sqlite_revision_store::BASELINE_SQL
        .split_once("CREATE TABLE writable_admissions (")
        .expect("baseline writer table")
        .1
        .split_once(";")
        .expect("baseline writer table terminator")
        .0;
    tx.execute_batch(&format!(
        "CREATE TABLE writable_admissions ({admission_schema};"
    ))
    .map_err(sql)?;
    tx.execute(
        "UPDATE pactrun_metadata SET format_version='1.0-alpha.3' WHERE singleton=1",
        [],
    )
    .map_err(sql)?;
    tx.pragma_update(None, "application_id", APPLICATION_ID)
        .map_err(sql)?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(sql)?;
    validate_schema(&tx, SCHEMA_VERSION)?;
    tx.commit().map_err(sql)
}
