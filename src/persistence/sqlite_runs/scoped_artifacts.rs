//! Run-owned Cleanup and exact-edge Migration artifacts. Physical keys are
//! private; callers select authored output identities and exact edge digests.
use super::*;
use crate::domain::{DeletionMode, MigrationEdgeIdentity, ServiceHookSite};

const CHUNKS: ChunkedBlobTable = ChunkedBlobTable {
    representation_maximum: RUN_ARTIFACT_MAX_BYTES_V1,
    insert_chunk_sql: "INSERT INTO run_scoped_artifact_chunks(run_id,artifact_key,chunk_index,chunk_bytes) VALUES(?1,?2,?3,?4)",
    select_chunks_sql: "SELECT chunk_index,chunk_bytes FROM run_scoped_artifact_chunks WHERE run_id=?1 AND artifact_key=?2 ORDER BY chunk_index",
    read_operation: "read scoped artifact staging",
    write_operation: "write scoped artifact chunk",
    invalid: PersistenceError::InvalidRunArtifact,
    corrupt: PersistenceError::CorruptRun,
};

fn invalid(message: &str) -> PersistenceError {
    PersistenceError::InvalidRunArtifact(message.to_owned())
}
fn corrupt() -> PersistenceError {
    PersistenceError::CorruptRun(
        "invalid scoped artifact identity or lifecycle evidence".to_owned(),
    )
}
fn key(index: i64, output: &ManagedOutputIdentity) -> Vec<u8> {
    let mut bytes = index.to_be_bytes().to_vec();
    bytes.extend_from_slice(output.as_str().as_bytes());
    bytes
}

fn edge_at(
    operation: &ManagedRunIdentity,
    index: i64,
) -> Result<Option<MigrationEdgeIdentity>, PersistenceError> {
    match operation {
        ManagedRunIdentity::Capture { .. } | ManagedRunIdentity::Restore { .. } if index == -1 => {
            Ok(None)
        }
        ManagedRunIdentity::Deletion {
            mode: DeletionMode::ManagedCleanup,
            ..
        } if index == -1 => Ok(None),
        ManagedRunIdentity::Migration(invocation) if index >= 0 => {
            let index = usize::try_from(index).map_err(|_| corrupt())?;
            let source = invocation.path().get(index).ok_or_else(corrupt)?;
            let target = invocation
                .path()
                .get(index.checked_add(1).ok_or_else(corrupt)?)
                .ok_or_else(corrupt)?;
            Ok(Some(MigrationEdgeIdentity {
                source: source.content_digest,
                target: target.content_digest,
            }))
        }
        _ => Err(corrupt()),
    }
}

fn validate_boundary(db: &Connection, run: RunId, index: i64) -> Result<(), PersistenceError> {
    let terminal: Option<i64> = db
        .query_row(
            "SELECT outcome_rank FROM run_outcomes WHERE run_id=?1",
            [run.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| PersistenceError::sqlite("read artifact Run outcome", e))?;
    if index == -1 {
        if terminal.is_some() {
            return Ok(());
        }
        if !matches!(
            managed_invocation_row(db, run)?,
            ManagedRunIdentity::Deletion { .. }
        ) {
            return Err(corrupt());
        }
        let authorized:bool = db.query_row("SELECT EXISTS(SELECT 1 FROM instance_deletion_obligations WHERE attempt_run_id=?1 AND phase=2)",[run.as_bytes().as_slice()],|r|r.get(0)).map_err(|e|PersistenceError::sqlite("qualify Cleanup artifact boundary",e))?;
        if authorized {
            return Ok(());
        }
    } else {
        let committed: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM run_migration_boundaries WHERE run_id=?1 AND edge_index=?2)",params![run.as_bytes().as_slice(),index],|r|r.get(0)).map_err(|e|PersistenceError::sqlite("qualify Migration artifact boundary",e))?;
        if committed {
            return Ok(());
        }
        let current: Option<i64> = db
            .query_row(
                "SELECT committed_edge_count FROM run_migration_progress WHERE run_id=?1",
                [run.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| PersistenceError::sqlite("read Migration artifact progress", e))?;
        if terminal.is_some_and(|outcome| outcome != RunOutcome::Succeeded.rank())
            && current == Some(index)
        {
            return Ok(());
        }
    }
    Err(corrupt())
}

pub(in crate::persistence) fn insert_scoped_artifacts(
    tx: &Transaction<'_>,
    run: RunId,
    index: i64,
    outputs: RunArtifactSubmission<'_, '_>,
    successful: bool,
) -> Result<(), PersistenceError> {
    if outputs.submitted.is_empty() && outputs.artifacts.is_empty() {
        return Ok(());
    }
    let operation = managed_invocation_row(tx, run)?;
    let edge = edge_at(&operation, index)?;
    let (revision, site) = if let Some(edge) = edge {
        (
            RevisionIdentity {
                package_id: operation.revision().package_id,
                content_digest: edge.target,
            },
            ServiceHookSite::Migration(Sha256Digest::from_bytes(*edge.source.as_bytes())),
        )
    } else {
        let site = match &operation {
            ManagedRunIdentity::Capture { .. } => ServiceHookSite::Capture,
            ManagedRunIdentity::Restore { .. } => ServiceHookSite::Restore,
            ManagedRunIdentity::Deletion { .. } => ServiceHookSite::Cleanup,
            _ => return Err(corrupt()),
        };
        (operation.revision().clone(), site)
    };
    let core = super::super::sqlite_instances::load_revision_contract(tx, &revision)?;
    if core.operation().is_none() {
        return Err(invalid(
            "old-format operations do not declare ordinary scoped outputs",
        ));
    }
    let declarations = core
        .ordinary_outputs(&site)
        .ok_or_else(|| invalid("operation outputs are unavailable"))?;
    let submitted: BTreeSet<_> = outputs.submitted.iter().collect();
    if submitted.len() != outputs.submitted.len()
        || submitted
            .iter()
            .any(|id| !declarations.iter().any(|output| &output.id == *id))
        || outputs
            .artifacts
            .iter()
            .any(|artifact| !submitted.contains(&artifact.output))
        || ((!outputs.artifacts.is_empty() || successful)
            && outputs.artifacts.len() != submitted.len())
    {
        return Err(invalid(
            "scoped artifacts must match the complete declared submitted set",
        ));
    }
    let old: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM run_artifacts WHERE run_id=?1)",
            [run.as_bytes().as_slice()],
            |r| r.get(0),
        )
        .map_err(|e| PersistenceError::sqlite("exclude conflicting artifact representation", e))?;
    if old {
        return Err(corrupt());
    }
    for artifact in outputs.artifacts {
        if artifact.byte_len > RUN_ARTIFACT_MAX_BYTES_V1 {
            return Err(invalid("Run Artifact exceeds the size limit"));
        }
        let key = key(index, &artifact.output);
        tx.execute("INSERT INTO run_scoped_artifacts(run_id,artifact_key,edge_index,output_identity,byte_length) VALUES(?1,?2,?3,?4,?5)",params![run.as_bytes().as_slice(),&key,index,artifact.output.as_str().as_bytes(),artifact.byte_len as i64]).map_err(|e|PersistenceError::sqlite("insert scoped artifact",e))?;
        insert_chunks(
            tx,
            &CHUNKS,
            [run.as_bytes().as_slice(), &key],
            artifact.reader,
            artifact.byte_len,
        )?;
    }
    Ok(())
}

pub(super) struct ArtifactLocation {
    pub(super) byte_length: u64,
    pub(super) key: Vec<u8>,
    pub(super) scoped: bool,
}
impl ArtifactLocation {
    pub(super) fn chunks(&self) -> &'static ChunkedBlobTable {
        if self.scoped {
            &CHUNKS
        } else {
            &ARTIFACT_CHUNKS
        }
    }
}

pub(super) fn lookup_artifact(
    db: &Connection,
    run: RunId,
    output: &ManagedOutputIdentity,
    edge: Option<&MigrationEdgeIdentity>,
) -> Result<Option<ArtifactLocation>, PersistenceError> {
    let operation = managed_invocation_row(db, run)?;
    let index = if let Some(edge) = edge {
        let ManagedRunIdentity::Migration(invocation) = &operation else {
            return Err(invalid("artifact edge requires a Migration Run"));
        };
        let index = invocation
            .path()
            .windows(2)
            .position(|pair| {
                pair[0].content_digest == edge.source && pair[1].content_digest == edge.target
            })
            .ok_or_else(|| invalid("artifact edge is not on the Run path"))?;
        i64::try_from(index).map_err(|_| corrupt())?
    } else {
        -1
    };
    let legacy: Option<i64> = if edge.is_none() {
        db.query_row(
            "SELECT byte_length FROM run_artifacts WHERE run_id=?1 AND output_identity=?2",
            params![run.as_bytes().as_slice(), output.as_str().as_bytes()],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| PersistenceError::sqlite("read legacy artifact header", e))?
    } else {
        None
    };
    let key = key(index, output);
    let scoped:Option<(i64,Vec<u8>,i64)>=db.query_row("SELECT edge_index,output_identity,byte_length FROM run_scoped_artifacts WHERE run_id=?1 AND artifact_key=?2",params![run.as_bytes().as_slice(),&key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|PersistenceError::sqlite("read scoped artifact header",e))?;
    match (legacy, scoped) {
        (Some(_), Some(_)) => Err(corrupt()),
        (Some(_), None) if !matches!(operation, ManagedRunIdentity::Action(_)) => Err(corrupt()),
        (Some(length), None) => Ok(Some(ArtifactLocation {
            byte_length: u64::try_from(length).map_err(|_| corrupt())?,
            key: output.as_str().as_bytes().to_vec(),
            scoped: false,
        })),
        (None, Some((stored, identity, length))) => {
            if stored != index || identity != output.as_str().as_bytes() {
                return Err(corrupt());
            }
            edge_at(&operation, index)?;
            validate_boundary(db, run, index)?;
            Ok(Some(ArtifactLocation {
                byte_length: u64::try_from(length).map_err(|_| corrupt())?,
                key,
                scoped: true,
            }))
        }
        (None, None) => Ok(None),
    }
}

pub(super) fn load_scoped_artifacts(
    db: &Connection,
    run: RunId,
) -> Result<Vec<RunArtifactSummary>, PersistenceError> {
    let mut statement=db.prepare("SELECT artifact_key,edge_index,output_identity,byte_length FROM run_scoped_artifacts WHERE run_id=?1 ORDER BY edge_index,output_identity").map_err(|e|PersistenceError::sqlite("list scoped artifacts",e))?;
    let rows = statement
        .query_map([run.as_bytes().as_slice()], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .map_err(|e| PersistenceError::sqlite("query scoped artifacts", e))?;
    let mut result = Vec::new();
    for row in rows {
        let (stored, index, identity, length) =
            row.map_err(|e| PersistenceError::sqlite("read scoped artifact", e))?;
        let output =
            ManagedOutputIdentity::parse(utf8_message(identity)?).map_err(|_| corrupt())?;
        if key(index, &output) != stored {
            return Err(corrupt());
        }
        let edge = edge_at(&managed_invocation_row(db, run)?, index)?;
        validate_boundary(db, run, index)?;
        result.push(RunArtifactSummary {
            edge,
            output,
            byte_length: u64::try_from(length).map_err(|_| corrupt())?,
        });
    }
    Ok(result)
}
