//! Retained Run Artifact publication, reads and expiry.

use super::*;

impl PactrunPersistence {
    pub(crate) fn open_run_artifact(
        &self,
        run: RunId,
        output: &ManagedOutputIdentity,
        destination: &mut impl Write,
    ) -> Result<(), PersistenceError> {
        self.open_run_artifact_at(run, output, None, destination)
    }

    pub(crate) fn open_run_artifact_at(
        &self,
        run: RunId,
        output: &ManagedOutputIdentity,
        edge: Option<&crate::domain::MigrationEdgeIdentity>,
        destination: &mut impl Write,
    ) -> Result<(), PersistenceError> {
        let mut database = self.open_read_connection()?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| PersistenceError::sqlite("begin Run Artifact snapshot", error))?;
        run_header(&transaction, run)?;
        copy_artifact_from(&transaction, run, output, edge, destination)?;
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("close Run Artifact snapshot", error))?;
        Ok(())
    }
}

pub(in crate::persistence) fn copy_artifact_from(
    transaction: &Connection,
    run: RunId,
    output: &ManagedOutputIdentity,
    edge: Option<&crate::domain::MigrationEdgeIdentity>,
    destination: &mut impl Write,
) -> Result<(), PersistenceError> {
    let location = super::scoped_artifacts::lookup_artifact(transaction, run, output, edge)?
        .ok_or_else(|| {
            PersistenceError::InvalidRunArtifact("Run has no such Artifact".to_owned())
        })?;
    stream_chunks(
        transaction,
        location.chunks(),
        [run.as_bytes().as_slice(), location.key.as_slice()],
        location.byte_length,
        destination,
    )?;
    Ok(())
}

impl PactrunPersistence {
    pub(crate) fn delete_run_artifact(
        &self,
        run: RunId,
        output: &ManagedOutputIdentity,
    ) -> Result<bool, PersistenceError> {
        self.delete_run_artifact_at(run, output, None)
    }

    pub(crate) fn delete_run_artifact_at(
        &self,
        run: RunId,
        output: &ManagedOutputIdentity,
        edge: Option<&crate::domain::MigrationEdgeIdentity>,
    ) -> Result<bool, PersistenceError> {
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin Run Artifact expiry", error))?;
        self.check_write_admission(&transaction)?;
        run_header(&transaction, run)?;
        let deleted =
            match super::scoped_artifacts::lookup_artifact(&transaction, run, output, edge)? {
                Some(location) => transaction
                    .execute(
                        if location.scoped {
                            "DELETE FROM run_scoped_artifacts WHERE run_id=?1 AND artifact_key=?2"
                        } else {
                            "DELETE FROM run_artifacts WHERE run_id=?1 AND output_identity=?2"
                        },
                        params![run.as_bytes().as_slice(), location.key],
                    )
                    .map_err(|error| PersistenceError::sqlite("expire Run Artifact", error))?,
                None => 0,
            };
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit Run Artifact expiry", error))?;
        Ok(deleted > 0)
    }
}

pub(super) fn insert_artifact(
    transaction: &Transaction<'_>,
    run: RunId,
    artifact: &mut RunArtifactWrite<'_>,
) -> Result<(), PersistenceError> {
    if artifact.byte_len > RUN_ARTIFACT_MAX_BYTES_V1 {
        return Err(PersistenceError::InvalidRunArtifact(
            "Run Artifact exceeds the size limit".to_owned(),
        ));
    }
    let inserted = transaction
        .execute(
            "INSERT OR IGNORE INTO run_artifacts(run_id, output_identity, byte_length) \
             VALUES (?1, ?2, ?3)",
            params![
                run.as_bytes().as_slice(),
                artifact.output.as_str().as_bytes(),
                i64::try_from(artifact.byte_len).expect("Artifact length fits i64"),
            ],
        )
        .map_err(|error| PersistenceError::sqlite("insert Run Artifact", error))?;
    if inserted == 0 {
        return Err(PersistenceError::InvalidRunArtifact(format!(
            "duplicate Artifact for output {}",
            artifact.output.as_str()
        )));
    }
    insert_chunks(
        transaction,
        &ARTIFACT_CHUNKS,
        [
            run.as_bytes().as_slice(),
            artifact.output.as_str().as_bytes(),
        ],
        artifact.reader,
        artifact.byte_len,
    )
}
