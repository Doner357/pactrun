//! Exact, disclosure-safe service facts; never re-observe live resources on read.
use super::PersistenceError;
use crate::domain::{
    ManagedExecutionKind, MigrationEdgeIdentity, RevisionContentDigest, RunId,
    ServicePresenceFailure, ServicePresenceRequirement, ServiceReferenceV2,
    ServiceResourceIdentity, ServiceRole, ServiceScope, ServiceView,
};
use rusqlite::{Connection, OptionalExtension, params};

fn invalid() -> PersistenceError {
    PersistenceError::CorruptRun("invalid service prerequisite cause attribution".into())
}
fn validate(
    db: &Connection,
    run: RunId,
    kind: ManagedExecutionKind,
    cause: &ServicePresenceFailure,
) -> Result<(), PersistenceError> {
    if !cause.valid_for(kind) {
        return Err(invalid());
    }
    if let Some(edge) = &cause.edge {
        let matches: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM run_migration_edges e JOIN run_migration_progress p USING(run_id) WHERE e.run_id=?1 AND e.edge_index=p.committed_edge_count AND e.source_revision_digest=?2 AND e.target_revision_digest=?3)",
            params![run.as_bytes().as_slice(), edge.source.as_bytes().as_slice(), edge.target.as_bytes().as_slice()], |r| r.get(0))
            .map_err(|e| PersistenceError::sqlite("qualify service failure edge", e))?;
        if !matches {
            return Err(invalid());
        }
    }
    Ok(())
}
pub(super) fn save(
    db: &Connection,
    run: RunId,
    kind: ManagedExecutionKind,
    cause: &ServicePresenceFailure,
) -> Result<(), PersistenceError> {
    validate(db, run, kind, cause)?;
    let ServiceScope::Resource(id) = &cause.reference.scope else {
        return Err(invalid());
    };
    let view = match cause.reference.view {
        ServiceView::Current => 0,
        ServiceView::Source => 1,
        ServiceView::Target => 2,
    };
    let role = match cause.reference.role {
        ServiceRole::Active => 0,
        ServiceRole::Retained => 1,
    };
    db.execute("INSERT INTO run_service_prerequisite_causes(run_id,view_rank,role_rank,resource_id,expected_presence,observed_presence,source_revision_digest,target_revision_digest) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![run.as_bytes().as_slice(), view, role, id.as_str().as_bytes(),
            i64::from(cause.expected == ServicePresenceRequirement::Present), i64::from(cause.observed == ServicePresenceRequirement::Present),
            cause.edge.as_ref().map(|e| e.source.as_bytes().as_slice()), cause.edge.as_ref().map(|e| e.target.as_bytes().as_slice())])
        .map_err(|e| PersistenceError::sqlite("save service prerequisite cause", e))?;
    Ok(())
}
pub(super) fn load(
    db: &Connection,
    run: RunId,
    kind: ManagedExecutionKind,
    owner: &[u8],
    code: &[u8],
    step: i64,
) -> Result<Option<ServicePresenceFailure>, PersistenceError> {
    let row = db.query_row("SELECT view_rank,role_rank,CASE WHEN length(resource_id)<=128 THEN resource_id ELSE X'' END,expected_presence,observed_presence,CASE WHEN source_revision_digest IS NULL OR length(source_revision_digest)=32 THEN source_revision_digest ELSE X'' END,CASE WHEN target_revision_digest IS NULL OR length(target_revision_digest)=32 THEN target_revision_digest ELSE X'' END FROM run_service_prerequisite_causes WHERE run_id=?1", [run.as_bytes().as_slice()],
        |r| Ok((r.get::<_,i64>(0)?, r.get::<_,i64>(1)?, r.get::<_,Vec<u8>>(2)?, r.get::<_,i64>(3)?, r.get::<_,i64>(4)?, r.get::<_,Option<Vec<u8>>>(5)?, r.get::<_,Option<Vec<u8>>>(6)?)))
        .optional().map_err(|e| PersistenceError::sqlite("read service prerequisite cause", e))?;
    let Some((view, role, id, expected, observed, source, target)) = row else {
        return Ok(None);
    };
    if owner != b"admission" || code != b"plan_invalidated" || step != 1 {
        return Err(invalid());
    }
    let presence = |value| match value {
        0 => Ok(ServicePresenceRequirement::Absent),
        1 => Ok(ServicePresenceRequirement::Present),
        _ => Err(invalid()),
    };
    let cause = ServicePresenceFailure {
        reference: ServiceReferenceV2 {
            view: match view {
                0 => ServiceView::Current,
                1 => ServiceView::Source,
                2 => ServiceView::Target,
                _ => return Err(invalid()),
            },
            role: match role {
                0 => ServiceRole::Active,
                1 => ServiceRole::Retained,
                _ => return Err(invalid()),
            },
            scope: ServiceScope::Resource(
                ServiceResourceIdentity::parse(String::from_utf8(id).map_err(|_| invalid())?)
                    .map_err(|_| invalid())?,
            ),
        },
        expected: presence(expected)?,
        observed: presence(observed)?,
        edge: match (source, target) {
            (Some(source), Some(target)) => Some(MigrationEdgeIdentity {
                source: RevisionContentDigest::from_bytes(
                    source.try_into().map_err(|_| invalid())?,
                ),
                target: RevisionContentDigest::from_bytes(
                    target.try_into().map_err(|_| invalid())?,
                ),
            }),
            (None, None) => None,
            _ => return Err(invalid()),
        },
    };
    validate(db, run, kind, &cause)?;
    Ok(Some(cause))
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0766
    // Verifies: PR-REQ-0400
    #[test]
    fn service_failure_roundtrip_rejects_corruption_and_wrong_migration_edge() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE run_primary_failures(run_id BLOB PRIMARY KEY); CREATE TABLE run_migration_edges(run_id BLOB,edge_index INTEGER,source_revision_digest BLOB,target_revision_digest BLOB); CREATE TABLE run_migration_progress(run_id BLOB,committed_edge_count INTEGER);").unwrap();
        db.execute_batch(super::super::sqlite_revision_store::SERVICE_FAILURE_CAUSES_SQL)
            .unwrap();
        let run = RunId::from_bytes([4; 16]);
        db.execute(
            "INSERT INTO run_primary_failures VALUES(?1)",
            [run.as_bytes().as_slice()],
        )
        .unwrap();
        let mut cause = ServicePresenceFailure {
            reference: ServiceReferenceV2 {
                view: ServiceView::Current,
                role: ServiceRole::Active,
                scope: ServiceScope::Resource(ServiceResourceIdentity::parse("uploads").unwrap()),
            },
            expected: ServicePresenceRequirement::Present,
            observed: ServicePresenceRequirement::Absent,
            edge: None,
        };
        save(&db, run, ManagedExecutionKind::Action, &cause).unwrap();
        assert_eq!(
            load(
                &db,
                run,
                ManagedExecutionKind::Action,
                b"admission",
                b"plan_invalidated",
                1
            )
            .unwrap(),
            Some(cause.clone())
        );
        for (owner, code, step) in [
            (b"execution".as_slice(), b"plan_invalidated".as_slice(), 1),
            (b"admission", b"other", 1),
            (b"admission", b"plan_invalidated", 0),
        ] {
            assert!(load(&db, run, ManagedExecutionKind::Action, owner, code, step).is_err());
        }
        db.execute_batch("PRAGMA ignore_check_constraints=ON; UPDATE run_service_prerequisite_causes SET observed_presence=expected_presence;").unwrap();
        assert!(
            load(
                &db,
                run,
                ManagedExecutionKind::Action,
                b"admission",
                b"plan_invalidated",
                1
            )
            .is_err()
        );
        db.execute_batch(
            "DELETE FROM run_service_prerequisite_causes; PRAGMA ignore_check_constraints=OFF;",
        )
        .unwrap();
        cause.reference.view = ServiceView::Source;
        cause.edge = Some(MigrationEdgeIdentity {
            source: RevisionContentDigest::from_bytes([1; 32]),
            target: RevisionContentDigest::from_bytes([2; 32]),
        });
        assert!(save(&db, run, ManagedExecutionKind::Migration, &cause).is_err());
        let edge = cause.edge.as_ref().unwrap();
        db.execute(
            "INSERT INTO run_migration_edges VALUES(?1,0,?2,?3)",
            params![
                run.as_bytes().as_slice(),
                edge.source.as_bytes().as_slice(),
                edge.target.as_bytes().as_slice()
            ],
        )
        .unwrap();
        db.execute(
            "INSERT INTO run_migration_progress VALUES(?1,0)",
            [run.as_bytes().as_slice()],
        )
        .unwrap();
        save(&db, run, ManagedExecutionKind::Migration, &cause).unwrap();
        assert_eq!(
            load(
                &db,
                run,
                ManagedExecutionKind::Migration,
                b"admission",
                b"plan_invalidated",
                1
            )
            .unwrap(),
            Some(cause)
        );
        db.execute(
            "UPDATE run_migration_progress SET committed_edge_count=1",
            [],
        )
        .unwrap();
        assert!(
            load(
                &db,
                run,
                ManagedExecutionKind::Migration,
                b"admission",
                b"plan_invalidated",
                1
            )
            .is_err()
        );
    }
}
