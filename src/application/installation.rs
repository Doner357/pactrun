#[cfg(test)]
use std::{path::Path, sync::Arc};

#[cfg(test)]
use crate::{
    authoring::{
        NormalizedPackDefinition, SecureSourceRoot, SourceRelativePathV1,
        VersionedPackSourceCandidate,
    },
    domain::{
        RuntimeContentProjectionInputV1, RuntimeFileKindV1, RuntimeFileV1,
        project_runtime_content_closure_v1,
    },
    managed_data::{StagedRuntimeSource, StagingSession},
    revision_content::validate_revision_content,
};
use crate::{
    domain::{
        CurrentState, PortableMetadataTemplate, ReferenceLabelBinding, RevisionContentDigest,
        RevisionIdentity, RevisionMetadataMutation, RevisionMetadataMutationBatch, Sha256Digest,
        ValidatedRevisionContent, validate_revision_sources_v1,
    },
    persistence::PactrunPersistence,
    revision_content::calculate_revision_content_digest,
};
use std::collections::{BTreeMap, BTreeSet};

use super::ApplicationError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MigrationRelationState {
    NotEvaluated,
    Valid,
    Invalid(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationRelationView {
    pub(crate) source_revision_digest: Sha256Digest,
    pub(crate) state: MigrationRelationState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstallPackResult {
    pub(crate) revision: RevisionIdentity,
    pub(crate) migrations: Vec<MigrationRelationView>,
    pub(crate) kept_metadata: Vec<(
        crate::domain::PresentationTargetV1,
        crate::domain::PresentationField,
    )>,
}

#[cfg(test)]
struct StagedRuntimeEntry {
    id: crate::domain::ContentId,
    blob_digest: Sha256Digest,
    byte_len: u64,
    source: Arc<StagedRuntimeSource>,
}

#[cfg(test)]
struct StagedRuntimeContentSet {
    entries: Vec<StagedRuntimeEntry>,
}

#[cfg(test)]
struct RevisionCandidate {
    definition: NormalizedPackDefinition,
    staged: StagedRuntimeContentSet,
}

#[cfg(test)]
pub(super) fn install_pack_source(
    persistence: &PactrunPersistence,
    staging: &StagingSession,
    source_root: &Path,
    explicit_local_metadata: &RevisionMetadataMutationBatch,
) -> Result<InstallPackResult, ApplicationError> {
    let candidate = crate::pack_transport::acquire(
        source_root,
        staging,
        &crate::hook::ActionCancellation::default(),
    )
    .map_err(ApplicationError::InvalidInstallation)?;
    install_prepared(
        persistence,
        candidate,
        explicit_local_metadata,
        crate::domain::PackMetadataConflict::Reject,
        &crate::hook::ActionCancellation::default(),
    )
}

pub(super) fn install_prepared(
    persistence: &PactrunPersistence,
    candidate: crate::revision_installation::PreparedRevision,
    local: &RevisionMetadataMutationBatch,
    policy: crate::domain::PackMetadataConflict,
    cancellation: &crate::hook::ActionCancellation,
) -> Result<InstallPackResult, ApplicationError> {
    let crate::revision_installation::PreparedRevision {
        identity,
        content,
        metadata,
        blobs,
    } = candidate;
    let expected: BTreeSet<_> = content
        .runtime_content
        .files()
        .iter()
        .map(|f| f.blob_digest.clone())
        .collect();
    if expected != blobs.keys().cloned().collect()
        || calculate_revision_content_digest(&content).map_err(ApplicationError::RevisionContent)?
            != identity.content_digest
    {
        return Err(ApplicationError::InvalidInstallation(
            "Pack candidate identity or blob closure mismatch".into(),
        ));
    }
    let batch = metadata_batch(&identity, &metadata, local)?;
    validate_metadata_plan(&identity, &content, &batch)?;
    let mut publications = Vec::new();
    for (digest, blob) in blobs {
        crate::pack_transport::check_cancel(cancellation).map_err(|source| {
            ApplicationError::Io {
                operation: "install Pack",
                source,
            }
        })?;
        let mut reader = crate::pack_transport::CancelReader {
            inner: blob.bytes.try_clone_reader()?,
            cancellation,
        };
        let witness = persistence.put_runtime_content(&digest, &mut reader)?;
        if witness.digest() != &digest || witness.byte_len() != blob.bytes.byte_len() {
            return Err(ApplicationError::InvalidInstallation(
                "Pack publication witness mismatch".into(),
            ));
        }
        publications.push(witness);
    }
    let (revision, kept_metadata) = persistence.persist_pack_revision(
        identity.package_id,
        &content,
        &publications,
        Some(&batch),
        Some(policy),
        cancellation,
    )?;
    let migrations = inspect_migrations(persistence, &revision, &content)?;
    Ok(InstallPackResult {
        revision,
        migrations,
        kept_metadata,
    })
}

#[cfg(test)]
fn resolve_candidate(
    staging: &StagingSession,
    root: &SecureSourceRoot,
    candidate: VersionedPackSourceCandidate,
) -> Result<RevisionCandidate, ApplicationError> {
    let mut acquired = BTreeMap::<SourceRelativePathV1, Arc<StagedRuntimeSource>>::new();
    for record in &candidate.runtime_sources {
        if acquired.contains_key(&record.source) {
            continue;
        }
        let mut source = root.open_runtime_source(&record.source)?;
        let staged = Arc::new(staging.stage_runtime_source(&mut source)?);
        acquired.insert(record.source.clone(), staged);
    }

    let mut files = Vec::with_capacity(candidate.runtime_sources.len());
    let mut entries = Vec::with_capacity(candidate.runtime_sources.len());
    for record in candidate.runtime_sources {
        let source = Arc::clone(
            acquired
                .get(&record.source)
                .expect("each source locator was staged"),
        );
        files.push(RuntimeFileV1 {
            id: record.id.clone(),
            path: record.path,
            kind: RuntimeFileKindV1::RegularFile,
            blob_digest: source.blob_digest.clone(),
            executable: record.executable,
        });
        entries.push(StagedRuntimeEntry {
            id: record.id,
            blob_digest: source.blob_digest.clone(),
            byte_len: source.bytes.byte_len(),
            source,
        });
    }
    let runtime_content =
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files })?;
    entries.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(RevisionCandidate {
        definition: NormalizedPackDefinition {
            package_id: candidate.package_id,
            revision: candidate.revision,
            runtime_content,
            portable_metadata: candidate.portable_metadata,
        },
        staged: StagedRuntimeContentSet { entries },
    })
}

#[cfg(test)]
fn validate_staged_coverage(
    content: &ValidatedRevisionContent,
    staged: &StagedRuntimeContentSet,
) -> Result<(), ApplicationError> {
    if content.runtime_content.files().len() != staged.entries.len() {
        return Err(ApplicationError::InvalidInstallation(
            "runtime-content staged coverage is incomplete".to_owned(),
        ));
    }
    for (descriptor, entry) in content.runtime_content.files().iter().zip(&staged.entries) {
        if descriptor.id != entry.id
            || descriptor.blob_digest != entry.blob_digest
            || entry.byte_len != entry.source.bytes.byte_len()
        {
            return Err(ApplicationError::InvalidInstallation(
                "runtime-content staged coverage differs from the semantic descriptor".to_owned(),
            ));
        }
    }
    Ok(())
}

fn metadata_batch(
    revision: &RevisionIdentity,
    source: &PortableMetadataTemplate,
    local: &RevisionMetadataMutationBatch,
) -> Result<RevisionMetadataMutationBatch, ApplicationError> {
    let mut operations = Vec::new();
    operations.extend(source.reference_labels.iter().map(|(label, source)| {
        RevisionMetadataMutation::AddReferenceLabel(ReferenceLabelBinding {
            label: label.clone(),
            revision: revision.clone(),
            source: source.clone(),
        })
    }));
    operations.extend(source.presentation.iter().map(|presentation| {
        RevisionMetadataMutation::CompareAndSetPresentation {
            target: presentation.target.clone(),
            field: presentation.field,
            expected: CurrentState::Absent,
            desired: CurrentState::Present(presentation.value.clone()),
        }
    }));
    operations.extend(
        source
            .provenance
            .iter()
            .cloned()
            .map(RevisionMetadataMutation::AddProvenance),
    );
    operations.extend(local.operations().iter().cloned());
    RevisionMetadataMutationBatch::new(operations)
        .map_err(|error| ApplicationError::InvalidInstallation(error.to_string()))
}

fn validate_metadata_plan(
    revision: &RevisionIdentity,
    content: &ValidatedRevisionContent,
    batch: &RevisionMetadataMutationBatch,
) -> Result<(), ApplicationError> {
    for operation in batch.operations() {
        match operation {
            RevisionMetadataMutation::AddReferenceLabel(binding)
            | RevisionMetadataMutation::RemoveReferenceLabel(binding)
                if &binding.revision != revision =>
            {
                return Err(ApplicationError::InvalidInstallation(
                    "reference-label metadata targets a different Revision".to_owned(),
                ));
            }
            RevisionMetadataMutation::CompareAndSetPresentation { target, .. }
                if !content.core.contains_presentation_target(target) =>
            {
                return Err(ApplicationError::InvalidInstallation(
                    "presentation metadata targets an absent Revision member".to_owned(),
                ));
            }
            RevisionMetadataMutation::CompareAndSetLocalAlias {
                expected, desired, ..
            } if [expected, desired].into_iter().any(
                |state| matches!(state, CurrentState::Present(identity) if identity != revision),
            ) =>
            {
                return Err(ApplicationError::InvalidInstallation(
                    "local alias mutation is addressed to a different Revision".to_owned(),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn inspect_migrations(
    persistence: &PactrunPersistence,
    target: &RevisionIdentity,
    content: &ValidatedRevisionContent,
) -> Result<Vec<MigrationRelationView>, ApplicationError> {
    let mut result = Vec::new();
    for migration in content.core.migrations() {
        let source_digest =
            RevisionContentDigest::from_bytes(migration.source_revision_digest.to_bytes());
        let source_identity = RevisionIdentity::new(target.package_id, source_digest);
        let state = match persistence.load_revision(&source_identity)? {
            None => MigrationRelationState::NotEvaluated,
            Some(source) => {
                let ids = source
                    .content
                    .core
                    .inputs()
                    .iter()
                    .map(|input| input.id.clone())
                    .collect::<BTreeSet<_>>();
                let context = BTreeMap::from([(migration.source_revision_digest.clone(), ids)]);
                let relation = if let Some(core) = content.core.service_core() {
                    crate::domain::validate_service_sources_v2(
                        core,
                        &BTreeMap::from([(
                            migration.source_revision_digest.clone(),
                            source.content.core.service_source(),
                        )]),
                    )
                    .map_err(|e| e.to_string())
                } else {
                    validate_revision_sources_v1(content.core.common(), &context)
                        .map_err(|e| e.to_string())
                };
                match relation {
                    Ok(_) => MigrationRelationState::Valid,
                    Err(error) => MigrationRelationState::Invalid(error.to_string()),
                }
            }
        };
        result.push(MigrationRelationView {
            source_revision_digest: migration.source_revision_digest.clone(),
            state,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::TempDir;

    use super::*;
    use crate::{
        domain::{LocalNote, RevisionMetadataItem},
        managed_data::StagingSession,
        persistence::PactrunPersistence,
    };

    fn roots() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m2-install-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("install-")
            .tempdir_in(parent)
            .unwrap();
        let storage = temporary.path().join("storage");
        let source = temporary.path().join("source");
        fs::create_dir(&storage).unwrap();
        fs::create_dir(storage.join("database")).unwrap();
        fs::create_dir(storage.join("runtime-content")).unwrap();
        fs::create_dir(storage.join("staging")).unwrap();
        fs::create_dir(&source).unwrap();
        (temporary, storage, source)
    }

    fn empty_metadata() -> RevisionMetadataMutationBatch {
        RevisionMetadataMutationBatch::new(Vec::new()).unwrap()
    }

    fn blob_names(root: &Path) -> Vec<String> {
        let mut names = fs::read_dir(root.join("runtime-content"))
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
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

    // Test-ID: PR-TEST-0379
    // Verifies: PR-REQ-0318, PR-REQ-0320
    #[test]
    fn versioned_source_acquisition_and_publication_do_not_erase_core_identity() {
        let (_temporary, storage, source) = roots();
        let yaml=b"source_format: 2\npackage_id: 00000000000000000000000000000065\nrevision:\n  service_storages: [{id: state}]\n  service_resources: [{id: config, storage_id: state, locator: Config.json, kind: file}]\nruntime_content: {}\n";
        fs::write(source.join("pactrun.yaml"), yaml).unwrap();
        let p = PactrunPersistence::open(&storage).unwrap();
        let candidate = crate::authoring::parse_pack_source_yaml(yaml).unwrap();
        assert_eq!(candidate.revision.version(), 2);
        let root = SecureSourceRoot::open(&source).unwrap();
        let staged = resolve_candidate(p.staging_session().unwrap(), &root, candidate).unwrap();
        let content = validate_revision_content(
            staged.definition.revision,
            staged.definition.runtime_content,
        )
        .unwrap();
        validate_staged_coverage(&content, &staged.staged).unwrap();
        let digest = calculate_revision_content_digest(&content).unwrap();
        let installed = p
            .persist_versioned_revision_with_metadata(
                staged.definition.package_id,
                &content,
                &[],
                &empty_metadata(),
            )
            .unwrap();
        assert_eq!(installed.content_digest, digest);
        let loaded = p.load_revision(&installed).unwrap().unwrap();
        assert_eq!(loaded.content.core.version(), 2);
        assert_eq!(
            loaded.content.core.service_core().unwrap().resources()[0]
                .locator
                .as_str(),
            "Config.json"
        );
        assert!(!storage.join("service-storage").exists()); // installation is not allocation
        // V3 is explicitly supported; V4 remains the unknown-version case.
        for token in ["2.0", "'2'", "4"] {
            let invalid = String::from_utf8(yaml.to_vec())
                .unwrap()
                .replace("source_format: 2", &format!("source_format: {token}"));
            assert!(crate::authoring::parse_pack_source_yaml(invalid.as_bytes()).is_err());
        }
        let legacy=b"source_format: 1\npackage_id: 00000000000000000000000000000065\nrevision: {}\nruntime_content: {}\n";
        assert_eq!(
            crate::authoring::parse_pack_source_yaml(legacy)
                .unwrap()
                .revision
                .version(),
            1
        );
    }

    // Supporting coverage for PR-TEST-0069.
    // Verifies: PR-REQ-0121, PR-REQ-0122, PR-REQ-0260
    #[test]
    fn authored_content_ids_are_semantic_while_blob_digest_is_physical() {
        let (_temporary, storage, source) = roots();
        fs::write(source.join("same.bin"), b"same bytes").unwrap();
        fs::write(
            source.join("pactrun.yaml"),
            r#"source_format: 1
package_id: 00000000000000000000000000000011
revision:
  inputs: []
  actions: []
  migrations: []
runtime_content:
  files:
    - id: alpha
      source: same.bin
      path: lib/alpha.bin
    - id: beta
      source: same.bin
      path: lib/beta.bin
portable_metadata:
  reference_labels:
    - label: stable
      source: { kind: unattributed }
"#,
        )
        .unwrap();
        let persistence = PactrunPersistence::open(&storage).unwrap();
        let staging = StagingSession::open(&storage).unwrap();
        let installed =
            install_pack_source(&persistence, &staging, &source, &empty_metadata()).unwrap();
        let stored = persistence
            .load_revision(&installed.revision)
            .unwrap()
            .unwrap();
        let files = stored.content.runtime_content.files();
        assert_eq!(files.len(), 2);
        assert_ne!(files[0].id, files[1].id);
        assert_eq!(files[0].blob_digest, files[1].blob_digest);
        assert_eq!(blob_names(&storage).len(), 1);
        assert!(
            persistence
                .load_revision_metadata(&installed.revision)
                .unwrap()
                .items
                .iter()
                .any(|item| matches!(item, RevisionMetadataItem::ReferenceLabel(_)))
        );
    }

    // Test-ID: PR-TEST-0072
    // Verifies: PR-REQ-0003, PR-REQ-0153, PR-REQ-0260, PR-REQ-0261, PR-REQ-0262
    #[test]
    fn invalid_semantic_runtime_closure_creates_no_durable_blob() {
        let (_temporary, storage, source) = roots();
        fs::write(source.join("one.bin"), b"one").unwrap();
        fs::write(source.join("two.bin"), b"two").unwrap();
        fs::write(
            source.join("pactrun.yaml"),
            r#"source_format: 1
package_id: 00000000000000000000000000000012
revision:
  inputs: []
  actions: []
  migrations: []
runtime_content:
  files:
    - id: one
      source: one.bin
      path: duplicate.bin
    - id: two
      source: two.bin
      path: duplicate.bin
"#,
        )
        .unwrap();
        let persistence = PactrunPersistence::open(&storage).unwrap();
        let staging = StagingSession::open(&storage).unwrap();
        assert!(install_pack_source(&persistence, &staging, &source, &empty_metadata()).is_err());
        assert!(blob_names(&storage).is_empty());

        fs::write(
            source.join("pactrun.yaml"),
            r#"source_format: 1
package_id: 00000000000000000000000000000012
revision:
  inputs: []
  actions: []
  migrations: []
runtime_content:
  files:
    - id: one
      source: one.bin
      path: one.bin
"#,
        )
        .unwrap();
        let wrong_target = RevisionIdentity::new(
            crate::domain::PackageId::from_bytes([0x44; 16]),
            RevisionContentDigest::from_bytes([0x55; 32]),
        );
        let invalid_metadata =
            RevisionMetadataMutationBatch::new([RevisionMetadataMutation::AddReferenceLabel(
                ReferenceLabelBinding {
                    label: crate::domain::ReferenceLabel::parse("wrong-target").unwrap(),
                    revision: wrong_target,
                    source: crate::domain::ReferenceLabelSource::Unattributed,
                },
            )])
            .unwrap();
        assert!(install_pack_source(&persistence, &staging, &source, &invalid_metadata).is_err());
        assert!(blob_names(&storage).is_empty());

        let post_publication_conflict = RevisionMetadataMutationBatch::new([
            RevisionMetadataMutation::CompareAndSetLocalNote {
                expected: CurrentState::Present(LocalNote::parse("not-current").unwrap()),
                desired: CurrentState::Present(LocalNote::parse("desired").unwrap()),
            },
        ])
        .unwrap();
        assert!(
            install_pack_source(&persistence, &staging, &source, &post_publication_conflict,)
                .is_err()
        );
        let blobs_after_repository_failure = blob_names(&storage);
        assert_eq!(blobs_after_repository_failure.len(), 1);
        assert_eq!(
            fs::read(
                storage
                    .join("runtime-content")
                    .join(&blobs_after_repository_failure[0])
            )
            .unwrap(),
            b"one"
        );

        let root = SecureSourceRoot::open(&source).unwrap();
        let candidate = crate::authoring::parse_pack_source_yaml_v1(
            &fs::read(source.join("pactrun.yaml")).unwrap(),
        )
        .unwrap();
        let candidate = resolve_candidate(&staging, &root, candidate.into()).unwrap();
        let content = validate_revision_content(
            candidate.definition.revision,
            candidate.definition.runtime_content,
        )
        .unwrap();
        let expected_revision = RevisionIdentity::new(
            candidate.definition.package_id,
            calculate_revision_content_digest(&content).unwrap(),
        );
        assert!(
            persistence
                .load_revision(&expected_revision)
                .unwrap()
                .is_none()
        );
        let retry =
            install_pack_source(&persistence, &staging, &source, &empty_metadata()).unwrap();
        assert_eq!(retry.revision, expected_revision);
        assert_eq!(blob_names(&storage), blobs_after_repository_failure);

        let (_relation_temporary, relation_storage, relation_source) = roots();
        fs::write(
            relation_source.join("pactrun.yaml"),
            r#"source_format: 1
package_id: 00000000000000000000000000000013
revision:
  inputs: []
  actions: []
  migrations: []
runtime_content:
  files: []
"#,
        )
        .unwrap();
        let relation_persistence = PactrunPersistence::open(&relation_storage).unwrap();
        let relation_staging = StagingSession::open(&relation_storage).unwrap();
        let installed_source = install_pack_source(
            &relation_persistence,
            &relation_staging,
            &relation_source,
            &empty_metadata(),
        )
        .unwrap();

        let install_target = |digest: &str, source_requirement: &str| {
            fs::write(
                relation_source.join("pactrun.yaml"),
                format!(
                    r#"source_format: 1
package_id: 00000000000000000000000000000013
revision:
  inputs: []
  actions: []
  migrations:
    - source_revision_digest: {digest}
      transitions: []
      requires_source: {source_requirement}
      requires_target: []
      produces_target: []
runtime_content:
  files: []
"#
                ),
            )
            .unwrap();
            install_pack_source(
                &relation_persistence,
                &relation_staging,
                &relation_source,
                &empty_metadata(),
            )
            .unwrap()
        };

        let valid = install_target(&installed_source.revision.content_digest.to_string(), "[]");
        assert!(matches!(
            valid.migrations[0].state,
            MigrationRelationState::Valid
        ));
        let invalid = install_target(
            &installed_source.revision.content_digest.to_string(),
            "[{ role: active, input_id: absent }]",
        );
        assert!(matches!(
            invalid.migrations[0].state,
            MigrationRelationState::Invalid(_)
        ));
        let not_evaluated = install_target(
            "sha256:9999999999999999999999999999999999999999999999999999999999999999",
            "[]",
        );
        assert!(matches!(
            not_evaluated.migrations[0].state,
            MigrationRelationState::NotEvaluated
        ));
    }
}
