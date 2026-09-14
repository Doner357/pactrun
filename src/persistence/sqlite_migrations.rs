//! Coherent read-only Migration observations. No Run, pin, or write admission.
use super::sqlite_instances::load_instance_view_from;
use super::sqlite_revision_store::load_revision_from;
use super::{PactrunPersistence, PersistenceError};
use crate::domain::*;

impl PactrunPersistence {
    pub(crate) fn observe_migration_compilation(
        &self,
        id: InstanceId,
    ) -> Result<MigrationCompilationObservation, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        // A short SQLite read transaction keeps the state token, full registry,
        // and graph in one observation. This is not an execution lifetime pin.
        let tx = database
            .unchecked_transaction()
            .map_err(|e| PersistenceError::sqlite("begin Migration observation", e))?;
        let instance = load_instance_view_from(&tx, id)?
            .ok_or_else(|| PersistenceError::MissingInstance(id.to_string()))?;
        let mut bindings = Vec::new();
        {
            let mut statement = tx.prepare("SELECT input_identity, payload_id FROM managed_input_bindings WHERE instance_id=?1 ORDER BY input_identity")
                .map_err(|e| PersistenceError::sqlite("prepare Migration bindings", e))?;
            let rows = statement
                .query_map([id.as_bytes().as_slice()], |row| {
                    Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
                })
                .map_err(|e| PersistenceError::sqlite("query Migration bindings", e))?;
            for row in rows {
                let (input, payload) =
                    row.map_err(|e| PersistenceError::sqlite("read Migration binding", e))?;
                let input = InputIdentity::parse(String::from_utf8(input).map_err(|_| {
                    PersistenceError::CorruptManagedInput(
                        "invalid Migration Input identity".to_owned(),
                    )
                })?)
                .map_err(|_| {
                    PersistenceError::CorruptManagedInput(
                        "invalid Migration Input identity".to_owned(),
                    )
                })?;
                let payload =
                    ManagedInputPayloadId::from_bytes(payload.try_into().map_err(|_| {
                        PersistenceError::CorruptManagedInput(
                            "invalid Migration payload identity".to_owned(),
                        )
                    })?);
                let view = instance
                    .bindings
                    .iter()
                    .find(|b| b.input_id == input && b.present)
                    .ok_or_else(|| {
                        PersistenceError::CorruptManagedInput(
                            "inconsistent Migration binding view".to_owned(),
                        )
                    })?;
                bindings.push(MigrationBinding {
                    input,
                    origin: MigrationValueOrigin::Existing(payload),
                    protection: view.protection,
                });
            }
        }
        let mut revisions = Vec::new();
        {
            let mut statement = tx.prepare("SELECT revision_content_digest FROM revisions WHERE package_id=?1 ORDER BY revision_content_digest")
                .map_err(|e| PersistenceError::sqlite("prepare Migration Revisions", e))?;
            let rows = statement
                .query_map(
                    [instance.active_revision.package_id.as_bytes().as_slice()],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .map_err(|e| PersistenceError::sqlite("query Migration Revisions", e))?;
            for row in rows {
                let digest =
                    row.map_err(|e| PersistenceError::sqlite("read Migration Revision", e))?;
                let digest =
                    RevisionContentDigest::from_bytes(digest.try_into().map_err(|_| {
                        PersistenceError::CorruptManagedInput(
                            "invalid Migration Revision digest".to_owned(),
                        )
                    })?);
                let identity = RevisionIdentity::new(instance.active_revision.package_id, digest);
                let revision = load_revision_from(&tx, &identity)?
                    .ok_or_else(|| PersistenceError::MissingRevision(identity.clone()))?;
                revisions.push(MigrationRevision {
                    identity,
                    content: revision.content,
                });
            }
        }
        tx.commit()
            .map_err(|e| PersistenceError::sqlite("finish Migration observation", e))?;
        Ok(MigrationCompilationObservation {
            instance: id,
            state_version: instance.state_version,
            active_revision: instance.active_revision,
            revisions,
            bindings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        project_revision_core_v1, project_runtime_content_closure_v1, validate_revision_content_v1,
    };
    use std::{fs, io::Cursor, path::Path};

    // Test-ID: PR-TEST-0285
    // Verifies: PR-REQ-0305
    #[test]
    fn migration_observation_is_read_only_and_includes_retained_bindings() {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m5-observation-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::tempdir_in(parent).unwrap();
        for directory in ["database", "runtime-content", "staging"] {
            fs::create_dir(temporary.path().join(directory)).unwrap();
        }
        let p = PactrunPersistence::open(temporary.path()).unwrap();
        let input = InputIdentity::parse("config").unwrap();
        let core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
            inputs: vec![InputDeclarationV1 {
                id: input.clone(),
                required: false,
                protection: InputProtectionV1::Secret,
            }],
            actions: vec![],
            snapshot: None,
            migrations: vec![],
            cleanup: None,
        })
        .unwrap();
        let runtime =
            project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![] })
                .unwrap();
        let content = validate_revision_content_v1(core, runtime.clone()).unwrap();
        let package = PackageId::from_bytes([1; 16]);
        let source = p.persist_revision(package, &content, &[]).unwrap();
        let mut bytes = Cursor::new(b"private bytes".to_vec());
        let instance = p
            .create_instance(
                InstanceName::parse("migration-test").unwrap(),
                source.clone(),
                &mut [super::super::ManagedInputWrite {
                    input_id: input.clone(),
                    byte_len: 13,
                    reader: &mut bytes,
                }],
            )
            .unwrap();
        let core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
            inputs: vec![],
            actions: vec![],
            snapshot: None,
            migrations: vec![MigrationV1 {
                source_revision_digest: Sha256Digest::from_bytes(*source.content_digest.as_bytes()),
                transitions: vec![],
                requires_source: vec![],
                requires_target: vec![],
                produces_target: vec![],
                hook: None,
            }],
            cleanup: None,
        })
        .unwrap();
        let content = validate_revision_content_v1(core, runtime).unwrap();
        let target = p.persist_revision(package, &content, &[]).unwrap();
        // A fixture boundary makes the binding retained without requiring the
        // not-yet-implemented Migration publisher to manufacture test state.
        p.database
            .lock()
            .unwrap()
            .execute(
                "UPDATE instances SET active_revision_content_digest=?1 WHERE instance_id=?2",
                rusqlite::params![
                    target.content_digest.as_bytes().as_slice(),
                    instance.id.as_bytes().as_slice()
                ],
            )
            .unwrap();
        drop(p);
        let reader = PactrunPersistence::open_read_only(temporary.path()).unwrap();
        let before = fs::read_dir(temporary.path().join("staging"))
            .unwrap()
            .count();
        let observation = reader.observe_migration_compilation(instance.id).unwrap();
        assert_eq!(observation.revisions.len(), 2);
        assert_eq!(observation.active_revision, target);
        assert_eq!(observation.bindings.len(), 1);
        assert_eq!(observation.bindings[0].input, input);
        assert_eq!(
            observation.bindings[0].protection,
            ManagedInputProtection::Secret
        );
        assert!(!format!("{observation:?}").contains("private bytes"));
        assert_eq!(
            before,
            fs::read_dir(temporary.path().join("staging"))
                .unwrap()
                .count()
        );
        let database = reader.database.lock().unwrap();
        for table in ["runs", "run_revision_pins", "writable_admissions"] {
            let count: i64 = database
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0);
        }
    }
}
