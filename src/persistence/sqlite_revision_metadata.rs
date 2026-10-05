use std::cmp::Ordering;

use rusqlite::{
    Connection, OptionalExtension, Transaction, TransactionBehavior, params, params_from_iter,
    types::Value,
};

use super::sqlite_revision_store::{FaultPoint, fault};
use super::{PactrunPersistence, PersistenceError};
use crate::{
    domain::{
        ActionIdentity, AttributionText, CurrentState, InputIdentity, LocalAlias, LocalNote,
        ManagedOutputIdentity, MetadataPortability, ParameterIdentity, PresentationField,
        PresentationMetadata, PresentationTargetV1, PresentationValue, ProvenanceClaim,
        PublisherName, PublisherNamespace, ReferenceLabel, ReferenceLabelBinding,
        ReferenceLabelSource, RevisionContentDigest, RevisionDeclarations, RevisionIdentity,
        RevisionMetadataItem, RevisionMetadataMutation, RevisionMetadataMutationBatch,
        RevisionMetadataView, SourceUri, TrustAssessment,
    },
    revision_content::{calculate_revision_content_digest, decode_canonical_revision_content},
};

impl PactrunPersistence {
    pub(crate) fn apply_revision_metadata_batch(
        &self,
        revision: &RevisionIdentity,
        batch: &RevisionMetadataMutationBatch,
    ) -> Result<(), PersistenceError> {
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin metadata transaction", error))?;
        self.check_write_admission(&transaction)?;
        apply_revision_metadata_in_transaction(&transaction, revision, batch)?;
        fault(FaultPoint::BeforeMetadataCommit);
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit metadata transaction", error))?;
        fault(FaultPoint::AfterMetadataCommit);
        Ok(())
    }

    pub(crate) fn load_revision_metadata(
        &self,
        revision: &RevisionIdentity,
    ) -> Result<RevisionMetadataView, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let core = load_revision_core(&database, revision)?;
        load_revision_metadata_from(&database, revision, &core)
    }

    pub(crate) fn lookup_reference_label(
        &self,
        label: &ReferenceLabel,
    ) -> Result<Vec<RevisionIdentity>, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        validate_reference_label_lookup_rows(&database, label)?;
        let mut statement = database
            .prepare(
                "SELECT DISTINCT package_id, revision_content_digest \
                 FROM revision_reference_label_bindings \
                 WHERE label_utf8 = ?1 \
                 ORDER BY package_id, revision_content_digest",
            )
            .map_err(|error| PersistenceError::sqlite("prepare reference-label lookup", error))?;
        let identities = statement
            .query_map([label.as_bytes()], |row| {
                let package: Vec<u8> = row.get(0)?;
                let digest: Vec<u8> = row.get(1)?;
                Ok((package, digest))
            })
            .map_err(|error| PersistenceError::sqlite("query reference-label lookup", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| PersistenceError::sqlite("read reference-label lookup", error))?
            .into_iter()
            .map(|(package, digest)| revision_identity(package, digest))
            .collect::<Result<Vec<_>, _>>()?;
        ensure_strictly_ordered(&identities, RevisionIdentity::cmp, "reference-label lookup")?;
        Ok(identities)
    }

    pub(crate) fn lookup_local_alias(
        &self,
        alias: &LocalAlias,
    ) -> Result<Option<RevisionIdentity>, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let stored = database
            .query_row(
                "SELECT package_id, revision_content_digest \
                 FROM revision_local_aliases WHERE alias_utf8 = ?1",
                [alias.as_bytes()],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(|error| PersistenceError::sqlite("lookup local alias", error))?;
        let identity = stored
            .map(|(package, digest)| revision_identity(package, digest))
            .transpose()?;
        if let Some(identity) = &identity
            && !revision_exists(&database, identity)?
        {
            return Err(PersistenceError::CorruptMetadata(
                "local alias targets a missing Revision".to_owned(),
            ));
        }
        Ok(identity)
    }
}

pub(super) fn apply_revision_metadata_in_transaction(
    transaction: &Transaction<'_>,
    revision: &RevisionIdentity,
    batch: &RevisionMetadataMutationBatch,
) -> Result<(), PersistenceError> {
    let core = load_revision_core(transaction, revision)?;
    // Reject pre-existing non-canonical metadata before applying another mutation.
    load_revision_metadata_from(transaction, revision, &core)?;
    for operation in batch.operations() {
        apply_operation(transaction, revision, &core, operation)?;
    }
    load_revision_metadata_from(transaction, revision, &core)?;
    Ok(())
}

pub(super) fn validate_reference_label_lookup_rows(
    database: &Connection,
    label: &ReferenceLabel,
) -> Result<(), PersistenceError> {
    let mut statement = database
        .prepare(
            "SELECT package_id, revision_content_digest, source_rank, \
             publisher_name_utf8, publisher_namespace_present, \
             publisher_namespace_utf8, source_uri_ascii \
             FROM revision_reference_label_bindings \
             WHERE label_utf8 = ?1 \
             ORDER BY package_id, revision_content_digest, source_rank, \
             publisher_name_utf8, publisher_namespace_present, \
             publisher_namespace_utf8, source_uri_ascii",
        )
        .map_err(|error| PersistenceError::sqlite("prepare label-row validation", error))?;
    let rows = statement
        .query_map([label.as_bytes()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Vec<u8>>(5)?,
                row.get::<_, Vec<u8>>(6)?,
            ))
        })
        .map_err(|error| PersistenceError::sqlite("query label-row validation", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read label-row validation", error))?;
    let mut previous = None;
    for (package, digest, rank, publisher, namespace_present, namespace, source_uri) in rows {
        let binding = ReferenceLabelBinding {
            label: label.clone(),
            revision: revision_identity(package, digest)?,
            source: decode_label_source(rank, publisher, namespace_present, namespace, source_uri)?,
        };
        if previous
            .as_ref()
            .is_some_and(|previous: &ReferenceLabelBinding| previous >= &binding)
        {
            return Err(PersistenceError::CorruptMetadata(
                "reference-label lookup rows are not in canonical typed order".to_owned(),
            ));
        }
        previous = Some(binding);
    }
    Ok(())
}

fn apply_operation(
    transaction: &Transaction<'_>,
    revision: &RevisionIdentity,
    core: &RevisionDeclarations,
    operation: &RevisionMetadataMutation,
) -> Result<(), PersistenceError> {
    match operation {
        RevisionMetadataMutation::AddReferenceLabel(binding) => {
            require_binding_target(revision, binding)?;
            mutate_reference_label(transaction, binding, true)
        }
        RevisionMetadataMutation::RemoveReferenceLabel(binding) => {
            require_binding_target(revision, binding)?;
            mutate_reference_label(transaction, binding, false)
        }
        RevisionMetadataMutation::CompareAndSetPresentation {
            target,
            field,
            expected,
            desired,
        } => {
            if !core.contains_presentation_target(target) {
                return Err(PersistenceError::InvalidMetadata(
                    "presentation target is absent from the strict-decoded RevisionDeclarations"
                        .to_owned(),
                ));
            }
            compare_and_set_presentation(transaction, revision, target, *field, expected, desired)
        }
        RevisionMetadataMutation::AddProvenance(claim) => {
            mutate_provenance(transaction, revision, claim, true)
        }
        RevisionMetadataMutation::RemoveProvenance(claim) => {
            mutate_provenance(transaction, revision, claim, false)
        }
        RevisionMetadataMutation::CompareAndSetLocalAlias {
            alias,
            expected,
            desired,
        } => compare_and_set_alias(transaction, revision, alias, expected, desired),
        RevisionMetadataMutation::CompareAndSetLocalNote { expected, desired } => {
            compare_and_set_note(transaction, revision, expected, desired)
        }
        RevisionMetadataMutation::CompareAndSetLocalTrust { expected, desired } => {
            compare_and_set_trust(transaction, revision, expected, desired)
        }
    }
}

fn require_binding_target(
    revision: &RevisionIdentity,
    binding: &ReferenceLabelBinding,
) -> Result<(), PersistenceError> {
    if &binding.revision != revision {
        return Err(PersistenceError::InvalidMetadata(
            "reference-label binding targets a different Revision than its batch".to_owned(),
        ));
    }
    Ok(())
}

fn mutate_reference_label(
    transaction: &Transaction<'_>,
    binding: &ReferenceLabelBinding,
    add: bool,
) -> Result<(), PersistenceError> {
    let (source_rank, publisher_name, namespace_present, namespace, source_uri) =
        encode_label_source(&binding.source);
    let parameters: [&dyn rusqlite::ToSql; 8] = [
        &binding.revision.package_id.as_bytes().as_slice(),
        &binding.revision.content_digest.as_bytes().as_slice(),
        &binding.label.as_bytes(),
        &source_rank,
        &publisher_name,
        &namespace_present,
        &namespace,
        &source_uri,
    ];
    let (sql, operation) = if add {
        (
            "INSERT OR IGNORE INTO revision_reference_label_bindings(\
             package_id, revision_content_digest, label_utf8, source_rank, \
             publisher_name_utf8, publisher_namespace_present, \
             publisher_namespace_utf8, source_uri_ascii\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            "add reference-label binding",
        )
    } else {
        (
            "DELETE FROM revision_reference_label_bindings \
             WHERE package_id = ?1 AND revision_content_digest = ?2 \
             AND label_utf8 = ?3 AND source_rank = ?4 \
             AND publisher_name_utf8 = ?5 AND publisher_namespace_present = ?6 \
             AND publisher_namespace_utf8 = ?7 AND source_uri_ascii = ?8",
            "remove reference-label binding",
        )
    };
    transaction
        .execute(sql, parameters)
        .map_err(|error| PersistenceError::sqlite(operation, error))?;
    Ok(())
}

fn encode_label_source(source: &ReferenceLabelSource) -> (i64, Vec<u8>, i64, Vec<u8>, Vec<u8>) {
    let publisher = source
        .publisher_name()
        .map_or_else(Vec::new, |value| value.as_bytes().to_vec());
    let namespace = source.publisher_namespace();
    let uri = source
        .source_uri()
        .map_or_else(Vec::new, |value| value.as_bytes().to_vec());
    (
        i64::from(source.rank()),
        publisher,
        i64::from(namespace.is_some()),
        namespace.map_or_else(Vec::new, |value| value.as_bytes().to_vec()),
        uri,
    )
}

fn mutate_provenance(
    transaction: &Transaction<'_>,
    revision: &RevisionIdentity,
    claim: &ProvenanceClaim,
    add: bool,
) -> Result<(), PersistenceError> {
    let package = revision.package_id.as_bytes().as_slice();
    let digest = revision.content_digest.as_bytes().as_slice();
    match claim {
        ProvenanceClaim::SourceUri(uri) => {
            let verb = if add { "INSERT OR IGNORE" } else { "DELETE" };
            let sql = if add {
                format!(
                    "{verb} INTO revision_source_uri_claims(\
                     package_id, revision_content_digest, source_uri_ascii\
                     ) VALUES (?1, ?2, ?3)"
                )
            } else {
                "DELETE FROM revision_source_uri_claims WHERE package_id = ?1 \
                 AND revision_content_digest = ?2 AND source_uri_ascii = ?3"
                    .to_owned()
            };
            transaction
                .execute(&sql, params![package, digest, uri.as_bytes()])
                .map_err(|error| PersistenceError::sqlite("mutate source-URI claim", error))?;
        }
        ProvenanceClaim::PublisherAttribution {
            publisher,
            namespace,
            source_uri,
        } => {
            let namespace_present = i64::from(namespace.is_some());
            let namespace = namespace
                .as_ref()
                .map_or_else(Vec::new, |value| value.as_bytes().to_vec());
            let source_uri_present = i64::from(source_uri.is_some());
            let source_uri = source_uri
                .as_ref()
                .map_or_else(Vec::new, |value| value.as_bytes().to_vec());
            let sql = if add {
                "INSERT OR IGNORE INTO revision_publisher_attribution_claims(\
                 package_id, revision_content_digest, publisher_name_utf8, \
                 publisher_namespace_present, publisher_namespace_utf8, \
                 source_uri_present, source_uri_ascii\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
            } else {
                "DELETE FROM revision_publisher_attribution_claims \
                 WHERE package_id = ?1 AND revision_content_digest = ?2 \
                 AND publisher_name_utf8 = ?3 AND publisher_namespace_present = ?4 \
                 AND publisher_namespace_utf8 = ?5 AND source_uri_present = ?6 \
                 AND source_uri_ascii = ?7"
            };
            transaction
                .execute(
                    sql,
                    params![
                        package,
                        digest,
                        publisher.as_bytes(),
                        namespace_present,
                        namespace,
                        source_uri_present,
                        source_uri,
                    ],
                )
                .map_err(|error| {
                    PersistenceError::sqlite("mutate publisher-attribution claim", error)
                })?;
        }
        ProvenanceClaim::Attribution { text, source_uri } => {
            let source_uri_present = i64::from(source_uri.is_some());
            let source_uri = source_uri
                .as_ref()
                .map_or_else(Vec::new, |value| value.as_bytes().to_vec());
            let sql = if add {
                "INSERT OR IGNORE INTO revision_attribution_claims(\
                 package_id, revision_content_digest, attribution_text_utf8, \
                 source_uri_present, source_uri_ascii\
                 ) VALUES (?1, ?2, ?3, ?4, ?5)"
            } else {
                "DELETE FROM revision_attribution_claims \
                 WHERE package_id = ?1 AND revision_content_digest = ?2 \
                 AND attribution_text_utf8 = ?3 AND source_uri_present = ?4 \
                 AND source_uri_ascii = ?5"
            };
            transaction
                .execute(
                    sql,
                    params![
                        package,
                        digest,
                        text.as_bytes(),
                        source_uri_present,
                        source_uri,
                    ],
                )
                .map_err(|error| PersistenceError::sqlite("mutate attribution claim", error))?;
        }
    }
    Ok(())
}

struct PresentationStorageKey {
    table: &'static str,
    key_columns: &'static [&'static str],
    key_values: Vec<Value>,
}

fn presentation_storage_key(target: &PresentationTargetV1) -> PresentationStorageKey {
    match target {
        PresentationTargetV1::Revision => PresentationStorageKey {
            table: "revision_presentation_current",
            key_columns: &[],
            key_values: vec![],
        },
        PresentationTargetV1::Input(input) => PresentationStorageKey {
            table: "revision_input_presentation_current",
            key_columns: &["input_id_utf8"],
            key_values: vec![Value::Blob(input.as_str().as_bytes().to_vec())],
        },
        PresentationTargetV1::Action(action) => PresentationStorageKey {
            table: "revision_action_presentation_current",
            key_columns: &["action_id_utf8"],
            key_values: vec![Value::Blob(action.as_str().as_bytes().to_vec())],
        },
        PresentationTargetV1::ActionParameter { action, parameter } => PresentationStorageKey {
            table: "revision_action_parameter_presentation_current",
            key_columns: &["action_id_utf8", "parameter_id_utf8"],
            key_values: vec![
                Value::Blob(action.as_str().as_bytes().to_vec()),
                Value::Blob(parameter.as_str().as_bytes().to_vec()),
            ],
        },
        PresentationTargetV1::ManagedOutput { action, output } => PresentationStorageKey {
            table: "revision_managed_output_presentation_current",
            key_columns: &["action_id_utf8", "output_id_utf8"],
            key_values: vec![
                Value::Blob(action.as_str().as_bytes().to_vec()),
                Value::Blob(output.as_str().as_bytes().to_vec()),
            ],
        },
        PresentationTargetV1::SnapshotCapture => PresentationStorageKey {
            table: "revision_snapshot_operation_presentation_current",
            key_columns: &["operation_rank"],
            key_values: vec![Value::Integer(0)],
        },
        PresentationTargetV1::SnapshotCaptureParameter(parameter) => PresentationStorageKey {
            table: "revision_snapshot_parameter_presentation_current",
            key_columns: &["operation_rank", "parameter_id_utf8"],
            key_values: vec![
                Value::Integer(0),
                Value::Blob(parameter.as_str().as_bytes().to_vec()),
            ],
        },
        PresentationTargetV1::SnapshotRestore => PresentationStorageKey {
            table: "revision_snapshot_operation_presentation_current",
            key_columns: &["operation_rank"],
            key_values: vec![Value::Integer(1)],
        },
        PresentationTargetV1::SnapshotRestoreParameter(parameter) => PresentationStorageKey {
            table: "revision_snapshot_parameter_presentation_current",
            key_columns: &["operation_rank", "parameter_id_utf8"],
            key_values: vec![
                Value::Integer(1),
                Value::Blob(parameter.as_str().as_bytes().to_vec()),
            ],
        },
        PresentationTargetV1::MigrationEdge(digest) => PresentationStorageKey {
            table: "revision_migration_presentation_current",
            key_columns: &["source_revision_content_digest"],
            key_values: vec![Value::Blob(digest.as_bytes().to_vec())],
        },
        PresentationTargetV1::Cleanup => PresentationStorageKey {
            table: "revision_cleanup_presentation_current",
            key_columns: &[],
            key_values: vec![],
        },
    }
}

fn compare_and_set_presentation(
    transaction: &Transaction<'_>,
    revision: &RevisionIdentity,
    target: &PresentationTargetV1,
    field: PresentationField,
    expected: &CurrentState<PresentationValue>,
    desired: &CurrentState<PresentationValue>,
) -> Result<(), PersistenceError> {
    let storage = presentation_storage_key(target);
    let mut key_values = vec![
        Value::Blob(revision.package_id.as_bytes().to_vec()),
        Value::Blob(revision.content_digest.as_bytes().to_vec()),
    ];
    key_values.extend(storage.key_values.iter().cloned());
    let where_clause = std::iter::once("package_id")
        .chain(std::iter::once("revision_content_digest"))
        .chain(storage.key_columns.iter().copied())
        .enumerate()
        .map(|(index, column)| format!("{column} = ?{}", index + 1))
        .collect::<Vec<_>>()
        .join(" AND ");
    let query = format!(
        "SELECT display_name_utf8, summary_utf8, description_utf8, help_utf8 \
         FROM {} WHERE {where_clause}",
        storage.table
    );
    let mut values = transaction
        .query_row(&query, params_from_iter(key_values.iter()), |row| {
            Ok([
                row.get::<_, Option<Vec<u8>>>(0)?,
                row.get::<_, Option<Vec<u8>>>(1)?,
                row.get::<_, Option<Vec<u8>>>(2)?,
                row.get::<_, Option<Vec<u8>>>(3)?,
            ])
        })
        .optional()
        .map_err(|error| PersistenceError::sqlite("load current presentation", error))?
        .unwrap_or([None, None, None, None]);
    let index = usize::from(field.rank());
    let current = values[index]
        .as_ref()
        .map(|bytes| parse_text(bytes, PresentationValue::parse, "presentation value"))
        .transpose()?
        .map_or(CurrentState::Absent, CurrentState::Present);
    if !cas_requires_change(&current, expected, desired)? {
        return Ok(());
    }
    values[index] = match desired {
        CurrentState::Absent => None,
        CurrentState::Present(value) => Some(value.as_bytes().to_vec()),
    };
    if values.iter().all(Option::is_none) {
        transaction
            .execute(
                &format!("DELETE FROM {} WHERE {where_clause}", storage.table),
                params_from_iter(key_values.iter()),
            )
            .map_err(|error| PersistenceError::sqlite("clear presentation row", error))?;
        return Ok(());
    }

    let key_columns = std::iter::once("package_id")
        .chain(std::iter::once("revision_content_digest"))
        .chain(storage.key_columns.iter().copied())
        .collect::<Vec<_>>();
    let mut all_columns = key_columns.clone();
    all_columns.extend([
        "display_name_utf8",
        "summary_utf8",
        "description_utf8",
        "help_utf8",
    ]);
    let placeholders = (1..=all_columns.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let conflict_columns = key_columns.join(", ");
    let update = [
        "display_name_utf8",
        "summary_utf8",
        "description_utf8",
        "help_utf8",
    ]
    .map(|column| format!("{column} = excluded.{column}"))
    .join(", ");
    let sql = format!(
        "INSERT INTO {}({}) VALUES ({placeholders}) \
         ON CONFLICT ({conflict_columns}) DO UPDATE SET {update}",
        storage.table,
        all_columns.join(", ")
    );
    let mut parameters = key_values;
    parameters.extend(values.into_iter().map(|value| match value {
        Some(bytes) => Value::Blob(bytes),
        None => Value::Null,
    }));
    transaction
        .execute(&sql, params_from_iter(parameters.iter()))
        .map_err(|error| PersistenceError::sqlite("persist current presentation", error))?;
    Ok(())
}

fn compare_and_set_alias(
    transaction: &Transaction<'_>,
    batch_revision: &RevisionIdentity,
    alias: &LocalAlias,
    expected: &CurrentState<RevisionIdentity>,
    desired: &CurrentState<RevisionIdentity>,
) -> Result<(), PersistenceError> {
    if let CurrentState::Present(desired_revision) = desired
        && desired_revision != batch_revision
    {
        return Err(PersistenceError::InvalidMetadata(
            "alias desired target differs from the batch Revision".to_owned(),
        ));
    }
    if matches!(desired, CurrentState::Absent)
        && let CurrentState::Present(expected_revision) = expected
        && expected_revision != batch_revision
    {
        return Err(PersistenceError::InvalidMetadata(
            "alias clear is addressed to a different batch Revision".to_owned(),
        ));
    }
    for state in [expected, desired] {
        if let CurrentState::Present(identity) = state
            && !revision_exists(transaction, identity)?
        {
            return Err(PersistenceError::InvalidMetadata(
                "alias CAS references a missing Revision".to_owned(),
            ));
        }
    }
    let current = transaction
        .query_row(
            "SELECT package_id, revision_content_digest FROM revision_local_aliases \
             WHERE alias_utf8 = ?1",
            [alias.as_bytes()],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load current local alias", error))?
        .map(|(package, digest)| revision_identity(package, digest))
        .transpose()?
        .map_or(CurrentState::Absent, CurrentState::Present);
    if !cas_requires_change(&current, expected, desired)? {
        return Ok(());
    }
    match desired {
        CurrentState::Absent => {
            transaction
                .execute(
                    "DELETE FROM revision_local_aliases WHERE alias_utf8 = ?1",
                    [alias.as_bytes()],
                )
                .map_err(|error| PersistenceError::sqlite("clear local alias", error))?;
        }
        CurrentState::Present(identity) => {
            transaction
                .execute(
                    "INSERT INTO revision_local_aliases(\
                     alias_utf8, package_id, revision_content_digest\
                     ) VALUES (?1, ?2, ?3) \
                     ON CONFLICT(alias_utf8) DO UPDATE SET \
                     package_id = excluded.package_id, \
                     revision_content_digest = excluded.revision_content_digest",
                    params![
                        alias.as_bytes(),
                        identity.package_id.as_bytes().as_slice(),
                        identity.content_digest.as_bytes().as_slice(),
                    ],
                )
                .map_err(|error| PersistenceError::sqlite("persist local alias", error))?;
        }
    }
    Ok(())
}

fn compare_and_set_note(
    transaction: &Transaction<'_>,
    revision: &RevisionIdentity,
    expected: &CurrentState<LocalNote>,
    desired: &CurrentState<LocalNote>,
) -> Result<(), PersistenceError> {
    let current = transaction
        .query_row(
            "SELECT note_utf8 FROM revision_local_current_notes \
             WHERE package_id = ?1 AND revision_content_digest = ?2",
            params![
                revision.package_id.as_bytes().as_slice(),
                revision.content_digest.as_bytes().as_slice(),
            ],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load current local note", error))?
        .map(|bytes| parse_text(&bytes, LocalNote::parse, "local note"))
        .transpose()?
        .map_or(CurrentState::Absent, CurrentState::Present);
    if !cas_requires_change(&current, expected, desired)? {
        return Ok(());
    }
    match desired {
        CurrentState::Absent => {
            transaction
                .execute(
                    "DELETE FROM revision_local_current_notes \
                     WHERE package_id = ?1 AND revision_content_digest = ?2",
                    params![
                        revision.package_id.as_bytes().as_slice(),
                        revision.content_digest.as_bytes().as_slice(),
                    ],
                )
                .map_err(|error| PersistenceError::sqlite("clear local note", error))?;
        }
        CurrentState::Present(note) => {
            transaction
                .execute(
                    "INSERT INTO revision_local_current_notes(\
                     package_id, revision_content_digest, note_utf8\
                     ) VALUES (?1, ?2, ?3) \
                     ON CONFLICT(package_id, revision_content_digest) DO UPDATE SET \
                     note_utf8 = excluded.note_utf8",
                    params![
                        revision.package_id.as_bytes().as_slice(),
                        revision.content_digest.as_bytes().as_slice(),
                        note.as_bytes(),
                    ],
                )
                .map_err(|error| PersistenceError::sqlite("persist local note", error))?;
        }
    }
    Ok(())
}

fn compare_and_set_trust(
    transaction: &Transaction<'_>,
    revision: &RevisionIdentity,
    expected: &CurrentState<TrustAssessment>,
    desired: &CurrentState<TrustAssessment>,
) -> Result<(), PersistenceError> {
    let current = transaction
        .query_row(
            "SELECT trust_rank FROM revision_local_current_trust \
             WHERE package_id = ?1 AND revision_content_digest = ?2",
            params![
                revision.package_id.as_bytes().as_slice(),
                revision.content_digest.as_bytes().as_slice(),
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load current local trust", error))?
        .map(parse_trust)
        .transpose()?
        .map_or(CurrentState::Absent, CurrentState::Present);
    if !cas_requires_change(&current, expected, desired)? {
        return Ok(());
    }
    match desired {
        CurrentState::Absent => {
            transaction
                .execute(
                    "DELETE FROM revision_local_current_trust \
                     WHERE package_id = ?1 AND revision_content_digest = ?2",
                    params![
                        revision.package_id.as_bytes().as_slice(),
                        revision.content_digest.as_bytes().as_slice(),
                    ],
                )
                .map_err(|error| PersistenceError::sqlite("clear local trust", error))?;
        }
        CurrentState::Present(trust) => {
            transaction
                .execute(
                    "INSERT INTO revision_local_current_trust(\
                     package_id, revision_content_digest, trust_rank\
                     ) VALUES (?1, ?2, ?3) \
                     ON CONFLICT(package_id, revision_content_digest) DO UPDATE SET \
                     trust_rank = excluded.trust_rank",
                    params![
                        revision.package_id.as_bytes().as_slice(),
                        revision.content_digest.as_bytes().as_slice(),
                        i64::from(trust.rank()),
                    ],
                )
                .map_err(|error| PersistenceError::sqlite("persist local trust", error))?;
        }
    }
    Ok(())
}

fn cas_requires_change<T: Eq>(
    current: &CurrentState<T>,
    expected: &CurrentState<T>,
    desired: &CurrentState<T>,
) -> Result<bool, PersistenceError> {
    if current == desired {
        Ok(false)
    } else if current == expected {
        Ok(true)
    } else {
        Err(PersistenceError::MetadataConflict(
            "current typed semantic state matches neither desired nor expected".to_owned(),
        ))
    }
}

pub(super) fn load_revision_metadata_from(
    database: &Connection,
    revision: &RevisionIdentity,
    core: &RevisionDeclarations,
) -> Result<RevisionMetadataView, PersistenceError> {
    let mut items = Vec::new();
    load_reference_labels(database, revision, &mut items)?;
    load_presentations(database, revision, core, &mut items)?;
    load_provenance(database, revision, &mut items)?;
    load_local_metadata(database, revision, &mut items)?;
    ensure_strictly_ordered(&items, RevisionMetadataItem::cmp_key, "Revision metadata")?;
    debug_assert!(items.iter().all(|item| matches!(
        item.portability(),
        MetadataPortability::PortableCapable | MetadataPortability::LocalOnly
    )));
    Ok(RevisionMetadataView { items })
}

fn load_reference_labels(
    database: &Connection,
    revision: &RevisionIdentity,
    output: &mut Vec<RevisionMetadataItem>,
) -> Result<(), PersistenceError> {
    let mut statement = database
        .prepare(
            "SELECT label_utf8, source_rank, publisher_name_utf8, \
             publisher_namespace_present, publisher_namespace_utf8, source_uri_ascii \
             FROM revision_reference_label_bindings \
             WHERE package_id = ?1 AND revision_content_digest = ?2 \
             ORDER BY label_utf8, source_rank, publisher_name_utf8, \
             publisher_namespace_present, publisher_namespace_utf8, source_uri_ascii",
        )
        .map_err(|error| PersistenceError::sqlite("prepare reference-label enumeration", error))?;
    let rows = statement
        .query_map(
            params![
                revision.package_id.as_bytes().as_slice(),
                revision.content_digest.as_bytes().as_slice(),
            ],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                ))
            },
        )
        .map_err(|error| PersistenceError::sqlite("query reference-label enumeration", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read reference-label enumeration", error))?;
    for (label, rank, publisher, namespace_present, namespace, source_uri) in rows {
        output.push(RevisionMetadataItem::ReferenceLabel(
            ReferenceLabelBinding {
                label: parse_text(&label, ReferenceLabel::parse, "reference label")?,
                revision: revision.clone(),
                source: decode_label_source(
                    rank,
                    publisher,
                    namespace_present,
                    namespace,
                    source_uri,
                )?,
            },
        ));
    }
    Ok(())
}

fn decode_label_source(
    rank: i64,
    publisher: Vec<u8>,
    namespace_present: i64,
    namespace: Vec<u8>,
    source_uri: Vec<u8>,
) -> Result<ReferenceLabelSource, PersistenceError> {
    let namespace = decode_optional_text(
        namespace_present,
        namespace,
        PublisherNamespace::parse,
        "publisher namespace",
    )?;
    match rank {
        0 if publisher.is_empty() && namespace.is_none() && source_uri.is_empty() => {
            Ok(ReferenceLabelSource::Unattributed)
        }
        1 if publisher.is_empty() && namespace.is_none() => Ok(ReferenceLabelSource::SourceUri(
            parse_text(&source_uri, SourceUri::parse, "source URI")?,
        )),
        2 if source_uri.is_empty() => Ok(ReferenceLabelSource::Publisher {
            name: parse_text(&publisher, PublisherName::parse, "publisher name")?,
            namespace,
        }),
        3 => Ok(ReferenceLabelSource::PublisherSourceUri {
            name: parse_text(&publisher, PublisherName::parse, "publisher name")?,
            namespace,
            source_uri: parse_text(&source_uri, SourceUri::parse, "source URI")?,
        }),
        _ => Err(PersistenceError::CorruptMetadata(
            "invalid reference-label source tuple".to_owned(),
        )),
    }
}

fn load_presentations(
    database: &Connection,
    revision: &RevisionIdentity,
    core: &RevisionDeclarations,
    output: &mut Vec<RevisionMetadataItem>,
) -> Result<(), PersistenceError> {
    let sql = r#"
SELECT 0 AS target_rank, CAST('' AS BLOB) AS component_1,
       CAST('' AS BLOB) AS component_2,
       display_name_utf8, summary_utf8, description_utf8, help_utf8
FROM revision_presentation_current
WHERE package_id = ?1 AND revision_content_digest = ?2
UNION ALL
SELECT 1, input_id_utf8, CAST('' AS BLOB),
       display_name_utf8, summary_utf8, description_utf8, help_utf8
FROM revision_input_presentation_current
WHERE package_id = ?1 AND revision_content_digest = ?2
UNION ALL
SELECT 2, action_id_utf8, CAST('' AS BLOB),
       display_name_utf8, summary_utf8, description_utf8, help_utf8
FROM revision_action_presentation_current
WHERE package_id = ?1 AND revision_content_digest = ?2
UNION ALL
SELECT 3, action_id_utf8, parameter_id_utf8,
       display_name_utf8, summary_utf8, description_utf8, help_utf8
FROM revision_action_parameter_presentation_current
WHERE package_id = ?1 AND revision_content_digest = ?2
UNION ALL
SELECT 4, action_id_utf8, output_id_utf8,
       display_name_utf8, summary_utf8, description_utf8, help_utf8
FROM revision_managed_output_presentation_current
WHERE package_id = ?1 AND revision_content_digest = ?2
UNION ALL
SELECT CASE operation_rank WHEN 0 THEN 5 ELSE 7 END,
       CAST('' AS BLOB), CAST('' AS BLOB),
       display_name_utf8, summary_utf8, description_utf8, help_utf8
FROM revision_snapshot_operation_presentation_current
WHERE package_id = ?1 AND revision_content_digest = ?2
UNION ALL
SELECT CASE operation_rank WHEN 0 THEN 6 ELSE 8 END,
       parameter_id_utf8, CAST('' AS BLOB),
       display_name_utf8, summary_utf8, description_utf8, help_utf8
FROM revision_snapshot_parameter_presentation_current
WHERE package_id = ?1 AND revision_content_digest = ?2
UNION ALL
SELECT 9, source_revision_content_digest, CAST('' AS BLOB),
       display_name_utf8, summary_utf8, description_utf8, help_utf8
FROM revision_migration_presentation_current
WHERE package_id = ?1 AND revision_content_digest = ?2
UNION ALL
SELECT 10, CAST('' AS BLOB), CAST('' AS BLOB),
       display_name_utf8, summary_utf8, description_utf8, help_utf8
FROM revision_cleanup_presentation_current
WHERE package_id = ?1 AND revision_content_digest = ?2
ORDER BY target_rank, component_1, component_2
"#;
    let mut statement = database
        .prepare(sql)
        .map_err(|error| PersistenceError::sqlite("prepare presentation enumeration", error))?;
    let rows = statement
        .query_map(
            params![
                revision.package_id.as_bytes().as_slice(),
                revision.content_digest.as_bytes().as_slice(),
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    [
                        row.get::<_, Option<Vec<u8>>>(3)?,
                        row.get::<_, Option<Vec<u8>>>(4)?,
                        row.get::<_, Option<Vec<u8>>>(5)?,
                        row.get::<_, Option<Vec<u8>>>(6)?,
                    ],
                ))
            },
        )
        .map_err(|error| PersistenceError::sqlite("query presentation enumeration", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read presentation enumeration", error))?;
    for (rank, component_1, component_2, fields) in rows {
        let target = decode_presentation_target(rank, &component_1, &component_2)?;
        if !core.contains_presentation_target(&target) {
            return Err(PersistenceError::CorruptMetadata(
                "stored presentation target is absent from RevisionDeclarations".to_owned(),
            ));
        }
        if fields.iter().all(Option::is_none) {
            return Err(PersistenceError::CorruptMetadata(
                "all-absent presentation row is non-canonical".to_owned(),
            ));
        }
        for (field, value) in PresentationField::ALL.into_iter().zip(fields) {
            if let Some(value) = value {
                output.push(RevisionMetadataItem::Presentation(PresentationMetadata {
                    revision: revision.clone(),
                    target: target.clone(),
                    field,
                    value: parse_text(&value, PresentationValue::parse, "presentation value")?,
                }));
            }
        }
    }
    Ok(())
}

fn decode_presentation_target(
    rank: i64,
    component_1: &[u8],
    component_2: &[u8],
) -> Result<PresentationTargetV1, PersistenceError> {
    match rank {
        0 if component_1.is_empty() && component_2.is_empty() => Ok(PresentationTargetV1::Revision),
        1 if component_2.is_empty() => Ok(PresentationTargetV1::Input(parse_semantic_id(
            component_1,
            InputIdentity::parse,
            "Input identity",
        )?)),
        2 if component_2.is_empty() => Ok(PresentationTargetV1::Action(parse_semantic_id(
            component_1,
            ActionIdentity::parse,
            "Action identity",
        )?)),
        3 => Ok(PresentationTargetV1::ActionParameter {
            action: parse_semantic_id(component_1, ActionIdentity::parse, "Action identity")?,
            parameter: parse_semantic_id(
                component_2,
                ParameterIdentity::parse,
                "Parameter identity",
            )?,
        }),
        4 => Ok(PresentationTargetV1::ManagedOutput {
            action: parse_semantic_id(component_1, ActionIdentity::parse, "Action identity")?,
            output: parse_semantic_id(
                component_2,
                ManagedOutputIdentity::parse,
                "ManagedOutput identity",
            )?,
        }),
        5 if component_1.is_empty() && component_2.is_empty() => {
            Ok(PresentationTargetV1::SnapshotCapture)
        }
        6 if component_2.is_empty() => Ok(PresentationTargetV1::SnapshotCaptureParameter(
            parse_semantic_id(component_1, ParameterIdentity::parse, "Parameter identity")?,
        )),
        7 if component_1.is_empty() && component_2.is_empty() => {
            Ok(PresentationTargetV1::SnapshotRestore)
        }
        8 if component_2.is_empty() => Ok(PresentationTargetV1::SnapshotRestoreParameter(
            parse_semantic_id(component_1, ParameterIdentity::parse, "Parameter identity")?,
        )),
        9 if component_2.is_empty() => Ok(PresentationTargetV1::MigrationEdge(revision_digest(
            component_1.to_vec(),
        )?)),
        10 if component_1.is_empty() && component_2.is_empty() => Ok(PresentationTargetV1::Cleanup),
        _ => Err(PersistenceError::CorruptMetadata(
            "invalid presentation target tuple".to_owned(),
        )),
    }
}

fn load_provenance(
    database: &Connection,
    revision: &RevisionIdentity,
    output: &mut Vec<RevisionMetadataItem>,
) -> Result<(), PersistenceError> {
    let parameters = params![
        revision.package_id.as_bytes().as_slice(),
        revision.content_digest.as_bytes().as_slice(),
    ];
    let mut source_statement = database
        .prepare(
            "SELECT source_uri_ascii FROM revision_source_uri_claims \
             WHERE package_id = ?1 AND revision_content_digest = ?2 \
             ORDER BY source_uri_ascii",
        )
        .map_err(|error| PersistenceError::sqlite("prepare source-URI claims", error))?;
    let source_rows = source_statement
        .query_map(parameters, |row| row.get::<_, Vec<u8>>(0))
        .map_err(|error| PersistenceError::sqlite("query source-URI claims", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read source-URI claims", error))?;
    for uri in source_rows {
        output.push(RevisionMetadataItem::Provenance {
            revision: revision.clone(),
            claim: ProvenanceClaim::SourceUri(parse_text(&uri, SourceUri::parse, "source URI")?),
        });
    }

    let mut publisher_statement = database
        .prepare(
            "SELECT publisher_name_utf8, publisher_namespace_present, \
             publisher_namespace_utf8, source_uri_present, source_uri_ascii \
             FROM revision_publisher_attribution_claims \
             WHERE package_id = ?1 AND revision_content_digest = ?2 \
             ORDER BY publisher_name_utf8, publisher_namespace_present, \
             publisher_namespace_utf8, source_uri_present, source_uri_ascii",
        )
        .map_err(|error| PersistenceError::sqlite("prepare publisher claims", error))?;
    let publisher_rows = publisher_statement
        .query_map(
            params![
                revision.package_id.as_bytes().as_slice(),
                revision.content_digest.as_bytes().as_slice(),
            ],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                ))
            },
        )
        .map_err(|error| PersistenceError::sqlite("query publisher claims", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read publisher claims", error))?;
    for (publisher, namespace_present, namespace, uri_present, uri) in publisher_rows {
        output.push(RevisionMetadataItem::Provenance {
            revision: revision.clone(),
            claim: ProvenanceClaim::PublisherAttribution {
                publisher: parse_text(&publisher, PublisherName::parse, "publisher name")?,
                namespace: decode_optional_text(
                    namespace_present,
                    namespace,
                    PublisherNamespace::parse,
                    "publisher namespace",
                )?,
                source_uri: decode_optional_text(uri_present, uri, SourceUri::parse, "source URI")?,
            },
        });
    }

    let mut attribution_statement = database
        .prepare(
            "SELECT attribution_text_utf8, source_uri_present, source_uri_ascii \
             FROM revision_attribution_claims \
             WHERE package_id = ?1 AND revision_content_digest = ?2 \
             ORDER BY attribution_text_utf8, source_uri_present, source_uri_ascii",
        )
        .map_err(|error| PersistenceError::sqlite("prepare attribution claims", error))?;
    let attribution_rows = attribution_statement
        .query_map(
            params![
                revision.package_id.as_bytes().as_slice(),
                revision.content_digest.as_bytes().as_slice(),
            ],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .map_err(|error| PersistenceError::sqlite("query attribution claims", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read attribution claims", error))?;
    for (text, uri_present, uri) in attribution_rows {
        output.push(RevisionMetadataItem::Provenance {
            revision: revision.clone(),
            claim: ProvenanceClaim::Attribution {
                text: parse_text(&text, AttributionText::parse, "attribution text")?,
                source_uri: decode_optional_text(uri_present, uri, SourceUri::parse, "source URI")?,
            },
        });
    }
    Ok(())
}

fn load_local_metadata(
    database: &Connection,
    revision: &RevisionIdentity,
    output: &mut Vec<RevisionMetadataItem>,
) -> Result<(), PersistenceError> {
    let mut alias_statement = database
        .prepare(
            "SELECT alias_utf8 FROM revision_local_aliases \
             WHERE package_id = ?1 AND revision_content_digest = ?2 \
             ORDER BY alias_utf8, package_id, revision_content_digest",
        )
        .map_err(|error| PersistenceError::sqlite("prepare local aliases", error))?;
    let aliases = alias_statement
        .query_map(
            params![
                revision.package_id.as_bytes().as_slice(),
                revision.content_digest.as_bytes().as_slice(),
            ],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(|error| PersistenceError::sqlite("query local aliases", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PersistenceError::sqlite("read local aliases", error))?;
    for alias in aliases {
        output.push(RevisionMetadataItem::LocalAlias {
            revision: revision.clone(),
            alias: parse_text(&alias, LocalAlias::parse, "local alias")?,
        });
    }

    let note = database
        .query_row(
            "SELECT note_utf8 FROM revision_local_current_notes \
             WHERE package_id = ?1 AND revision_content_digest = ?2 \
             ORDER BY package_id, revision_content_digest",
            params![
                revision.package_id.as_bytes().as_slice(),
                revision.content_digest.as_bytes().as_slice(),
            ],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load local note", error))?;
    if let Some(note) = note {
        output.push(RevisionMetadataItem::LocalNote {
            revision: revision.clone(),
            note: parse_text(&note, LocalNote::parse, "local note")?,
        });
    }

    let trust = database
        .query_row(
            "SELECT trust_rank FROM revision_local_current_trust \
             WHERE package_id = ?1 AND revision_content_digest = ?2 \
             ORDER BY package_id, revision_content_digest",
            params![
                revision.package_id.as_bytes().as_slice(),
                revision.content_digest.as_bytes().as_slice(),
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load local trust", error))?;
    if let Some(trust) = trust {
        output.push(RevisionMetadataItem::LocalTrust {
            revision: revision.clone(),
            trust: parse_trust(trust)?,
        });
    }
    Ok(())
}

pub(super) fn load_revision_core(
    database: &Connection,
    revision: &RevisionIdentity,
) -> Result<RevisionDeclarations, PersistenceError> {
    let raw = database
        .query_row(
            "SELECT core_jcs, runtime_content_jcs FROM revisions \
             WHERE package_id = ?1 AND revision_content_digest = ?2",
            params![
                revision.package_id.as_bytes().as_slice(),
                revision.content_digest.as_bytes().as_slice(),
            ],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load metadata target Revision", error))?
        .ok_or_else(|| PersistenceError::MissingRevision(revision.clone()))?;
    super::sqlite_revision_store::validate_core_storage_version(database, &raw.0)?;
    let content = decode_canonical_revision_content(&raw.0, &raw.1)
        .map_err(|error| PersistenceError::CorruptRevision(error.to_string()))?;
    let digest = calculate_revision_content_digest(&content)
        .map_err(|error| PersistenceError::CorruptRevision(error.to_string()))?;
    if digest != revision.content_digest {
        return Err(PersistenceError::CorruptRevision(
            "stored canonical components do not match the metadata target Revision".to_owned(),
        ));
    }
    // Presentation targets use the shared declarations, not service authority
    // or an identity re-encoding. The full versioned digest was checked above.
    Ok(content.core.common().clone())
}

fn revision_exists(
    database: &Connection,
    revision: &RevisionIdentity,
) -> Result<bool, PersistenceError> {
    database
        .query_row(
            "SELECT 1 FROM revisions WHERE package_id = ?1 AND revision_content_digest = ?2",
            params![
                revision.package_id.as_bytes().as_slice(),
                revision.content_digest.as_bytes().as_slice(),
            ],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(|error| PersistenceError::sqlite("check Revision existence", error))
}

fn parse_text<T, E>(
    bytes: &[u8],
    parse: impl FnOnce(String) -> Result<T, E>,
    kind: &str,
) -> Result<T, PersistenceError>
where
    E: fmt::Display,
{
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|_| PersistenceError::CorruptMetadata(format!("{kind} contains invalid UTF-8")))?;
    parse(text).map_err(|error| PersistenceError::CorruptMetadata(error.to_string()))
}

fn parse_semantic_id<T, E>(
    bytes: &[u8],
    parse: impl FnOnce(String) -> Result<T, E>,
    kind: &str,
) -> Result<T, PersistenceError>
where
    E: fmt::Display,
{
    parse_text(bytes, parse, kind)
}

fn decode_optional_text<T, E>(
    present: i64,
    bytes: Vec<u8>,
    parse: impl FnOnce(String) -> Result<T, E>,
    kind: &str,
) -> Result<Option<T>, PersistenceError>
where
    E: fmt::Display,
{
    match (present, bytes.is_empty()) {
        (0, true) => Ok(None),
        (1, false) => parse_text(&bytes, parse, kind).map(Some),
        _ => Err(PersistenceError::CorruptMetadata(format!(
            "non-canonical optional {kind}"
        ))),
    }
}

fn parse_trust(rank: i64) -> Result<TrustAssessment, PersistenceError> {
    match rank {
        0 => Ok(TrustAssessment::Trusted),
        1 => Ok(TrustAssessment::Distrusted),
        _ => Err(PersistenceError::CorruptMetadata(
            "invalid trust rank".to_owned(),
        )),
    }
}

fn revision_identity(
    package: Vec<u8>,
    digest: Vec<u8>,
) -> Result<RevisionIdentity, PersistenceError> {
    let package = package.try_into().map_err(|_| {
        PersistenceError::CorruptMetadata(
            "Revision identity has invalid PackageId bytes".to_owned(),
        )
    })?;
    Ok(RevisionIdentity::new(
        crate::domain::PackageId::from_bytes(package),
        revision_digest(digest)?,
    ))
}

fn revision_digest(bytes: Vec<u8>) -> Result<RevisionContentDigest, PersistenceError> {
    bytes
        .try_into()
        .map(RevisionContentDigest::from_bytes)
        .map_err(|_| {
            PersistenceError::CorruptMetadata(
                "Revision identity has invalid digest bytes".to_owned(),
            )
        })
}

fn ensure_strictly_ordered<T>(
    values: &[T],
    compare: impl Fn(&T, &T) -> Ordering,
    kind: &str,
) -> Result<(), PersistenceError> {
    if values
        .windows(2)
        .all(|window| compare(&window[0], &window[1]) == Ordering::Less)
    {
        Ok(())
    } else {
        Err(PersistenceError::CorruptMetadata(format!(
            "{kind} is not in canonical typed order"
        )))
    }
}

use std::fmt;

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Cursor,
        path::{Path, PathBuf},
        process::{Command, Stdio},
    };

    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;
    use crate::domain::{
        ActionV1, CaptureV1, CleanupV1, ContentId, DeclarationContent, HookLaunchV1, HookV1,
        IOContractV1, InputDeclarationV1, InputProtectionV1, ManagedOutputV1, MigrationV1,
        OperationAccessV1, ParameterTypeV1, ParameterV1, RestoreV1, RevisionDeclarationInput,
        RuntimeContentProjectionInputV1, RuntimeFileKindV1, RuntimeFileV1, RuntimePath,
        Sha256Digest, SnapshotCapabilityV1, TerminalContractV1, project_revision_declarations,
        project_runtime_content_closure_v1, validate_declaration_content,
    };
    use crate::persistence::sqlite_revision_store::SCHEMA_VERSION;

    const WORKER_TEST: &str = "persistence::sqlite_revision_metadata::tests::metadata_store_worker";

    fn package(tag: u8) -> crate::domain::PackageId {
        crate::domain::PackageId::from_bytes([tag; 16])
    }

    fn blob_digest(bytes: &[u8]) -> Sha256Digest {
        Sha256Digest::from_bytes(Sha256::digest(bytes).into())
    }

    fn test_root() -> (TempDir, PathBuf) {
        let parent = std::env::var_os("PACTRUN_METADATA_TEST_TEST_PARENT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/metadata-tests"));
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("metadata-persistence-")
            .tempdir_in(parent)
            .unwrap();
        let root = temporary.path().join("pactrun");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("database")).unwrap();
        fs::create_dir(root.join("runtime-content")).unwrap();
        (temporary, root)
    }

    fn database_path(root: &Path) -> PathBuf {
        root.join("database").join("pactrun.sqlite3")
    }

    fn hook() -> HookV1 {
        HookV1 {
            protocol_version: crate::domain::FormatVersion::BASELINE,
            launch: HookLaunchV1::Direct {
                executable: ContentId::parse("launcher").unwrap(),
            },
            args: Vec::new(),
            io: IOContractV1 {
                terminal: TerminalContractV1::None,
            },
        }
    }

    fn full_content(digest: &Sha256Digest) -> DeclarationContent {
        let core = project_revision_declarations(RevisionDeclarationInput {
            inputs: vec![InputDeclarationV1 {
                id: InputIdentity::parse("config").unwrap(),
                required: true,
                protection: InputProtectionV1::Normal,
            }],
            actions: vec![ActionV1 {
                id: ActionIdentity::parse("inspect").unwrap(),
                access: OperationAccessV1::Observe,
                parameters: vec![ParameterV1 {
                    id: ParameterIdentity::parse("detail").unwrap(),
                    parameter_type: ParameterTypeV1::String,
                    sensitive: false,
                    default: None,
                }],
                hook: hook(),
                outputs: vec![ManagedOutputV1 {
                    id: ManagedOutputIdentity::parse("report").unwrap(),
                }],
            }],
            snapshot: Some(SnapshotCapabilityV1 {
                capture: Some(CaptureV1 {
                    parameters: vec![ParameterV1 {
                        id: ParameterIdentity::parse("capture_level").unwrap(),
                        parameter_type: ParameterTypeV1::String,
                        sensitive: false,
                        default: None,
                    }],
                    access: OperationAccessV1::Observe,
                    hook: hook(),
                }),
                restore: Some(RestoreV1 {
                    parameters: vec![ParameterV1 {
                        id: ParameterIdentity::parse("restore_mode").unwrap(),
                        parameter_type: ParameterTypeV1::String,
                        sensitive: false,
                        default: None,
                    }],
                    hook: hook(),
                }),
            }),
            migrations: vec![MigrationV1 {
                source_revision_digest: Sha256Digest::from_bytes([0x44; 32]),
                transitions: Vec::new(),
                requires_source: Vec::new(),
                requires_target: Vec::new(),
                produces_target: Vec::new(),
                hook: None,
            }],
            cleanup: Some(CleanupV1 {
                requires: Vec::new(),
                hook: hook(),
            }),
        })
        .unwrap();
        let runtime_content = project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 {
            files: vec![RuntimeFileV1 {
                id: ContentId::parse("launcher").unwrap(),
                path: RuntimePath::parse("bin/launcher").unwrap(),
                kind: RuntimeFileKindV1::RegularFile,
                blob_digest: digest.clone(),
                executable: true,
            }],
        })
        .unwrap();
        validate_declaration_content(core, runtime_content).unwrap()
    }

    fn persist_full(
        persistence: &PactrunPersistence,
        package_tag: u8,
    ) -> (RevisionIdentity, DeclarationContent) {
        let bytes = b"revision metadata fixture";
        let digest = blob_digest(bytes);
        let publication = persistence
            .put_runtime_content(&digest, &mut Cursor::new(bytes))
            .unwrap();
        let content = full_content(&digest);
        let identity = persistence
            .persist_revision(package(package_tag), &content, &[publication])
            .unwrap();
        (identity, content)
    }

    fn run_worker(
        root: &Path,
        operation: &str,
        fault: Option<&str>,
        revision: Option<&RevisionIdentity>,
    ) -> bool {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg(WORKER_TEST)
            .arg("--nocapture")
            .env("PACTRUN_METADATA_TEST_WORKER", operation)
            .env("PACTRUN_METADATA_TEST_ROOT", root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(fault) = fault {
            command.env("PACTRUN_METADATA_TEST_FAULT", fault);
        }
        if let Some(revision) = revision {
            command
                .env(
                    "PACTRUN_METADATA_TEST_PACKAGE",
                    hex::encode(revision.package_id.as_bytes()),
                )
                .env(
                    "PACTRUN_METADATA_TEST_REVISION",
                    hex::encode(revision.content_digest.as_bytes()),
                );
        }
        let output = command.output().unwrap();
        if !output.status.success() && fault.is_none() {
            eprintln!(
                "Metadata worker failed with {}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        output.status.success()
    }

    #[test]
    fn metadata_store_worker() {
        let Some(operation) = std::env::var_os("PACTRUN_METADATA_TEST_WORKER") else {
            return;
        };
        let root = PathBuf::from(std::env::var_os("PACTRUN_METADATA_TEST_ROOT").unwrap());
        let persistence = PactrunPersistence::open(&root).unwrap();
        if operation == "metadata" {
            let package: [u8; 16] =
                hex::decode(std::env::var("PACTRUN_METADATA_TEST_PACKAGE").unwrap())
                    .unwrap()
                    .try_into()
                    .unwrap();
            let digest: [u8; 32] =
                hex::decode(std::env::var("PACTRUN_METADATA_TEST_REVISION").unwrap())
                    .unwrap()
                    .try_into()
                    .unwrap();
            let revision = RevisionIdentity::new(
                crate::domain::PackageId::from_bytes(package),
                RevisionContentDigest::from_bytes(digest),
            );
            let batch = RevisionMetadataMutationBatch::new([
                RevisionMetadataMutation::CompareAndSetLocalNote {
                    expected: CurrentState::Absent,
                    desired: CurrentState::Present(LocalNote::parse("retry-safe").unwrap()),
                },
            ])
            .unwrap();
            persistence
                .apply_revision_metadata_batch(&revision, &batch)
                .unwrap();
        }
    }

    // Test-ID: PR-TEST-0059
    // Verifies: PR-REQ-0248, PR-REQ-0256
    #[test]
    fn current_schema_preserves_v2_metadata_and_rejects_noncanonical_physical_states() {
        let (_temporary, root) = test_root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let (revision, _) = persist_full(&persistence, 1);
        let database = persistence.database.lock().unwrap();
        assert_eq!(
            database
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        assert_eq!(
            database
                .query_row(
                    "SELECT count(*) FROM pragma_table_list \
                     WHERE schema = 'main' AND name NOT LIKE 'sqlite_%'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            72 // Baseline metadata; obsolete inline Snapshot storage is absent.
        );
        let identity = params![
            revision.package_id.as_bytes().as_slice(),
            revision.content_digest.as_bytes().as_slice(),
        ];
        assert!(
            database
                .execute(
                    "INSERT INTO revision_presentation_current(\
                     package_id, revision_content_digest\
                     ) VALUES (?1, ?2)",
                    identity,
                )
                .is_err()
        );
        assert!(
            database
                .execute(
                    "INSERT INTO revision_reference_label_bindings(\
                     package_id, revision_content_digest, label_utf8, source_rank, \
                     publisher_name_utf8, publisher_namespace_present, \
                     publisher_namespace_utf8, source_uri_ascii\
                     ) VALUES (?1, ?2, X'6c', 0, X'', 0, X'', X'75')",
                    params![
                        revision.package_id.as_bytes().as_slice(),
                        revision.content_digest.as_bytes().as_slice(),
                    ],
                )
                .is_err()
        );
        assert!(
            database
                .execute(
                    "INSERT INTO revision_local_current_notes(\
                     package_id, revision_content_digest, note_utf8\
                     ) VALUES (?1, ?2, NULL)",
                    params![
                        revision.package_id.as_bytes().as_slice(),
                        revision.content_digest.as_bytes().as_slice(),
                    ],
                )
                .is_err()
        );
        assert!(
            database
                .execute(
                    "INSERT INTO revision_local_current_trust(\
                     package_id, revision_content_digest, trust_rank\
                     ) VALUES (?1, ?2, NULL)",
                    params![
                        revision.package_id.as_bytes().as_slice(),
                        revision.content_digest.as_bytes().as_slice(),
                    ],
                )
                .is_err()
        );
    }

    // Test-ID: PR-TEST-0062
    // Verifies: PR-REQ-0020, PR-REQ-0086, PR-REQ-0250, PR-REQ-0252, PR-REQ-0255
    #[test]
    fn label_and_provenance_sets_are_idempotent_and_lookup_deduplicates_targets() {
        let (_temporary, root) = test_root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let (first, _) = persist_full(&persistence, 1);
        let (second, _) = persist_full(&persistence, 2);
        let label = ReferenceLabel::parse("release").unwrap();
        let first_unattributed = ReferenceLabelBinding {
            label: label.clone(),
            revision: first.clone(),
            source: ReferenceLabelSource::Unattributed,
        };
        let first_sourced = ReferenceLabelBinding {
            label: label.clone(),
            revision: first.clone(),
            source: ReferenceLabelSource::PublisherSourceUri {
                name: PublisherName::parse("Publisher").unwrap(),
                namespace: None,
                source_uri: SourceUri::parse("https://example.test/source#first").unwrap(),
            },
        };
        let first_batch = RevisionMetadataMutationBatch::new([
            RevisionMetadataMutation::AddReferenceLabel(first_unattributed.clone()),
            RevisionMetadataMutation::AddReferenceLabel(first_sourced),
            RevisionMetadataMutation::AddProvenance(ProvenanceClaim::SourceUri(
                SourceUri::parse("urn:example:first#claim").unwrap(),
            )),
        ])
        .unwrap();
        persistence
            .apply_revision_metadata_batch(&first, &first_batch)
            .unwrap();
        persistence
            .apply_revision_metadata_batch(&first, &first_batch)
            .unwrap();
        let second_batch =
            RevisionMetadataMutationBatch::new([RevisionMetadataMutation::AddReferenceLabel(
                ReferenceLabelBinding {
                    label: label.clone(),
                    revision: second.clone(),
                    source: ReferenceLabelSource::Unattributed,
                },
            )])
            .unwrap();
        persistence
            .apply_revision_metadata_batch(&second, &second_batch)
            .unwrap();

        assert_eq!(
            persistence.lookup_reference_label(&label).unwrap(),
            vec![first.clone(), second]
        );
        let first_view = persistence.load_revision_metadata(&first).unwrap();
        assert_eq!(
            first_view
                .items
                .iter()
                .filter(|item| matches!(item, RevisionMetadataItem::ReferenceLabel(_)))
                .count(),
            2
        );
        assert_eq!(
            first_view
                .items
                .iter()
                .filter(|item| matches!(item, RevisionMetadataItem::Provenance { .. }))
                .count(),
            1
        );
        let remove_twice =
            RevisionMetadataMutationBatch::new([RevisionMetadataMutation::RemoveReferenceLabel(
                first_unattributed,
            )])
            .unwrap();
        persistence
            .apply_revision_metadata_batch(&first, &remove_twice)
            .unwrap();
        persistence
            .apply_revision_metadata_batch(&first, &remove_twice)
            .unwrap();
    }

    fn all_targets() -> Vec<PresentationTargetV1> {
        vec![
            PresentationTargetV1::Revision,
            PresentationTargetV1::Input(InputIdentity::parse("config").unwrap()),
            PresentationTargetV1::Action(ActionIdentity::parse("inspect").unwrap()),
            PresentationTargetV1::ActionParameter {
                action: ActionIdentity::parse("inspect").unwrap(),
                parameter: ParameterIdentity::parse("detail").unwrap(),
            },
            PresentationTargetV1::ManagedOutput {
                action: ActionIdentity::parse("inspect").unwrap(),
                output: ManagedOutputIdentity::parse("report").unwrap(),
            },
            PresentationTargetV1::SnapshotCapture,
            PresentationTargetV1::SnapshotCaptureParameter(
                ParameterIdentity::parse("capture_level").unwrap(),
            ),
            PresentationTargetV1::SnapshotRestore,
            PresentationTargetV1::SnapshotRestoreParameter(
                ParameterIdentity::parse("restore_mode").unwrap(),
            ),
            PresentationTargetV1::MigrationEdge(RevisionContentDigest::from_bytes([0x44; 32])),
            PresentationTargetV1::Cleanup,
        ]
    }

    fn presentation_value(
        target: &PresentationTargetV1,
        field: PresentationField,
    ) -> PresentationValue {
        PresentationValue::parse(format!("{}-{}", target.rank(), field.rank())).unwrap()
    }

    // Test-ID: PR-TEST-0063
    // Verifies: PR-REQ-0020, PR-REQ-0251, PR-REQ-0255, PR-REQ-0256
    #[test]
    fn every_presentation_target_accepts_every_field_and_total_absence_deletes_the_row() {
        let (_temporary, root) = test_root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let (revision, _) = persist_full(&persistence, 3);
        let operations = all_targets()
            .iter()
            .flat_map(|target| {
                PresentationField::ALL.map(|field| {
                    RevisionMetadataMutation::CompareAndSetPresentation {
                        target: target.clone(),
                        field,
                        expected: CurrentState::Absent,
                        desired: CurrentState::Present(presentation_value(target, field)),
                    }
                })
            })
            .collect::<Vec<_>>();
        persistence
            .apply_revision_metadata_batch(
                &revision,
                &RevisionMetadataMutationBatch::new(operations).unwrap(),
            )
            .unwrap();
        assert_eq!(
            persistence
                .load_revision_metadata(&revision)
                .unwrap()
                .items
                .iter()
                .filter(|item| matches!(item, RevisionMetadataItem::Presentation(_)))
                .count(),
            44
        );

        let clear_root = PresentationField::ALL.map(|field| {
            RevisionMetadataMutation::CompareAndSetPresentation {
                target: PresentationTargetV1::Revision,
                field,
                expected: CurrentState::Present(presentation_value(
                    &PresentationTargetV1::Revision,
                    field,
                )),
                desired: CurrentState::Absent,
            }
        });
        persistence
            .apply_revision_metadata_batch(
                &revision,
                &RevisionMetadataMutationBatch::new(clear_root).unwrap(),
            )
            .unwrap();
        assert_eq!(
            persistence
                .database
                .lock()
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM revision_presentation_current \
                     WHERE package_id = ?1 AND revision_content_digest = ?2",
                    params![
                        revision.package_id.as_bytes().as_slice(),
                        revision.content_digest.as_bytes().as_slice(),
                    ],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );

        let invalid = RevisionMetadataMutationBatch::new([
            RevisionMetadataMutation::CompareAndSetLocalNote {
                expected: CurrentState::Absent,
                desired: CurrentState::Present(LocalNote::parse("must roll back").unwrap()),
            },
            RevisionMetadataMutation::CompareAndSetPresentation {
                target: PresentationTargetV1::Action(
                    ActionIdentity::parse("missing_action").unwrap(),
                ),
                field: PresentationField::DisplayName,
                expected: CurrentState::Absent,
                desired: CurrentState::Present(PresentationValue::parse("invalid").unwrap()),
            },
        ])
        .unwrap();
        assert!(
            persistence
                .apply_revision_metadata_batch(&revision, &invalid)
                .is_err()
        );
        assert!(
            !persistence
                .load_revision_metadata(&revision)
                .unwrap()
                .items
                .iter()
                .any(|item| matches!(item, RevisionMetadataItem::LocalNote { .. }))
        );
    }

    fn one(operation: RevisionMetadataMutation) -> RevisionMetadataMutationBatch {
        RevisionMetadataMutationBatch::new([operation]).unwrap()
    }

    // Test-ID: PR-TEST-0064
    // Verifies: PR-REQ-0253, PR-REQ-0255
    #[test]
    fn local_metadata_cas_conflicts_roll_back_and_aba_is_intentionally_undetected() {
        let (_temporary, root) = test_root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let (first, _) = persist_full(&persistence, 4);
        let (second, _) = persist_full(&persistence, 5);
        let alias = LocalAlias::parse("current").unwrap();
        persistence
            .apply_revision_metadata_batch(
                &first,
                &one(RevisionMetadataMutation::CompareAndSetLocalAlias {
                    alias: alias.clone(),
                    expected: CurrentState::Absent,
                    desired: CurrentState::Present(first.clone()),
                }),
            )
            .unwrap();
        assert_eq!(
            persistence.lookup_local_alias(&alias).unwrap(),
            Some(first.clone())
        );
        persistence
            .apply_revision_metadata_batch(
                &second,
                &one(RevisionMetadataMutation::CompareAndSetLocalAlias {
                    alias: alias.clone(),
                    expected: CurrentState::Present(first.clone()),
                    desired: CurrentState::Present(second.clone()),
                }),
            )
            .unwrap();
        assert_eq!(
            persistence.lookup_local_alias(&alias).unwrap(),
            Some(second)
        );

        let absent = CurrentState::Absent;
        let a = LocalNote::parse("A").unwrap();
        let b = LocalNote::parse("B").unwrap();
        for (expected, desired) in [
            (absent.clone(), CurrentState::Present(a.clone())),
            (
                CurrentState::Present(a.clone()),
                CurrentState::Present(b.clone()),
            ),
            (CurrentState::Present(b), CurrentState::Present(a.clone())),
        ] {
            persistence
                .apply_revision_metadata_batch(
                    &first,
                    &one(RevisionMetadataMutation::CompareAndSetLocalNote { expected, desired }),
                )
                .unwrap();
        }
        persistence
            .apply_revision_metadata_batch(
                &first,
                &one(RevisionMetadataMutation::CompareAndSetLocalNote {
                    expected: CurrentState::Present(a.clone()),
                    desired: CurrentState::Present(LocalNote::parse("C").unwrap()),
                }),
            )
            .unwrap();

        let conflict = RevisionMetadataMutationBatch::new([
            RevisionMetadataMutation::CompareAndSetLocalNote {
                expected: CurrentState::Present(a),
                desired: CurrentState::Present(LocalNote::parse("D").unwrap()),
            },
            RevisionMetadataMutation::AddProvenance(ProvenanceClaim::SourceUri(
                SourceUri::parse("urn:must:rollback").unwrap(),
            )),
        ])
        .unwrap();
        assert!(matches!(
            persistence.apply_revision_metadata_batch(&first, &conflict),
            Err(PersistenceError::MetadataConflict(_))
        ));
        assert!(
            !persistence
                .load_revision_metadata(&first)
                .unwrap()
                .items
                .iter()
                .any(|item| matches!(item, RevisionMetadataItem::Provenance { .. }))
        );

        persistence
            .apply_revision_metadata_batch(
                &first,
                &one(RevisionMetadataMutation::CompareAndSetLocalTrust {
                    expected: CurrentState::Absent,
                    desired: CurrentState::Present(TrustAssessment::Distrusted),
                }),
            )
            .unwrap();
        persistence
            .apply_revision_metadata_batch(
                &first,
                &one(RevisionMetadataMutation::CompareAndSetLocalTrust {
                    expected: CurrentState::Present(TrustAssessment::Distrusted),
                    desired: CurrentState::Absent,
                }),
            )
            .unwrap();
    }

    fn ordering_operations(revision: &RevisionIdentity) -> Vec<RevisionMetadataMutation> {
        vec![
            RevisionMetadataMutation::AddReferenceLabel(ReferenceLabelBinding {
                label: ReferenceLabel::parse("é").unwrap(),
                revision: revision.clone(),
                source: ReferenceLabelSource::Publisher {
                    name: PublisherName::parse("p").unwrap(),
                    namespace: Some(PublisherNamespace::parse("z").unwrap()),
                },
            }),
            RevisionMetadataMutation::AddReferenceLabel(ReferenceLabelBinding {
                label: ReferenceLabel::parse("e\u{301}").unwrap(),
                revision: revision.clone(),
                source: ReferenceLabelSource::Publisher {
                    name: PublisherName::parse("p").unwrap(),
                    namespace: None,
                },
            }),
            RevisionMetadataMutation::AddProvenance(ProvenanceClaim::PublisherAttribution {
                publisher: PublisherName::parse("publisher").unwrap(),
                namespace: None,
                source_uri: None,
            }),
            RevisionMetadataMutation::AddProvenance(ProvenanceClaim::PublisherAttribution {
                publisher: PublisherName::parse("publisher").unwrap(),
                namespace: Some(PublisherNamespace::parse("namespace").unwrap()),
                source_uri: Some(SourceUri::parse("https://example.test/#fragment").unwrap()),
            }),
        ]
    }

    // Test-ID: PR-TEST-0066
    // Verifies: PR-REQ-0086, PR-REQ-0249, PR-REQ-0250, PR-REQ-0251, PR-REQ-0252, PR-REQ-0253, PR-REQ-0255
    #[test]
    fn sql_and_domain_order_are_parity_equivalent_and_insertion_independent() {
        let (_first_temporary, first_root) = test_root();
        let first_persistence = PactrunPersistence::open(&first_root).unwrap();
        let (first_revision, _) = persist_full(&first_persistence, 6);
        let operations = ordering_operations(&first_revision);
        first_persistence
            .apply_revision_metadata_batch(
                &first_revision,
                &RevisionMetadataMutationBatch::new(operations.clone()).unwrap(),
            )
            .unwrap();
        let first_view = first_persistence
            .load_revision_metadata(&first_revision)
            .unwrap();

        let (_second_temporary, second_root) = test_root();
        let second_persistence = PactrunPersistence::open(&second_root).unwrap();
        let (second_revision, _) = persist_full(&second_persistence, 6);
        assert_eq!(first_revision, second_revision);
        second_persistence
            .apply_revision_metadata_batch(
                &second_revision,
                &RevisionMetadataMutationBatch::new(operations.into_iter().rev()).unwrap(),
            )
            .unwrap();
        second_persistence
            .database
            .lock()
            .unwrap()
            .pragma_update(None, "reverse_unordered_selects", "ON")
            .unwrap();
        assert_eq!(
            first_view,
            second_persistence
                .load_revision_metadata(&second_revision)
                .unwrap()
        );
    }

    // Test-ID: PR-TEST-0067
    // Verifies: PR-REQ-0011, PR-REQ-0012, PR-REQ-0248, PR-REQ-0255, PR-REQ-0256
    #[test]
    fn committed_retry_reload_cascade_and_identity_boundaries_hold() {
        let (_temporary, root) = test_root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let (first, _) = persist_full(&persistence, 7);
        let (second, _) = persist_full(&persistence, 8);
        let before = persistence.load_revision(&first).unwrap().unwrap();
        let second_label = ReferenceLabelBinding {
            label: ReferenceLabel::parse("survivor").unwrap(),
            revision: second.clone(),
            source: ReferenceLabelSource::Unattributed,
        };
        persistence
            .apply_revision_metadata_batch(
                &second,
                &one(RevisionMetadataMutation::AddReferenceLabel(second_label)),
            )
            .unwrap();
        drop(persistence);

        assert!(!run_worker(
            &root,
            "metadata",
            Some("after_metadata_commit"),
            Some(&first),
        ));
        let persistence = PactrunPersistence::open(&root).unwrap();
        let retry = one(RevisionMetadataMutation::CompareAndSetLocalNote {
            expected: CurrentState::Absent,
            desired: CurrentState::Present(LocalNote::parse("retry-safe").unwrap()),
        });
        persistence
            .apply_revision_metadata_batch(&first, &retry)
            .unwrap();
        assert_eq!(persistence.load_revision(&first).unwrap().unwrap(), before);

        let mut database = persistence.database.lock().unwrap();
        let schema_names = database
            .prepare(
                "SELECT lower(name) FROM sqlite_schema \
                 WHERE name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        // Service custody is a separate contract, not a descriptive metadata
        // payload. Exempt only its exact approved relations from the historical
        // no-surrogate-schema check; arbitrary resource/JSON/orphan tables are
        // still forbidden here.
        let service_tables = [
            "service_storage_allocations",
            "service_storage_preparations",
            "service_storage_protections",
            "service_storage_run_origins",
            "instance_service_storages",
            "instance_service_resources",
            "run_service_storage_pins",
            "run_service_storage_targets",
            "run_service_resource_targets",
            "run_service_edge_commits",
        ];
        assert!(
            service_tables
                .iter()
                .all(|table| schema_names.iter().any(|name| name == table))
        );
        assert!(
            schema_names
                .iter()
                .filter(|name| !service_tables.contains(&name.as_str()))
                .all(|name| {
                    !name.contains("json")
                        && !name.contains("service_storage")
                        && !name.contains("resource")
                        && !name.contains("orphan")
                })
        );
        let deletion = database.transaction().unwrap();
        deletion
            .execute(
                "DELETE FROM revision_runtime_content_refs WHERE package_id = ?1 \
                 AND revision_content_digest = ?2",
                params![
                    first.package_id.as_bytes().as_slice(),
                    first.content_digest.as_bytes().as_slice(),
                ],
            )
            .unwrap();
        deletion
            .execute(
                "DELETE FROM revisions WHERE package_id = ?1 \
                 AND revision_content_digest = ?2",
                params![
                    first.package_id.as_bytes().as_slice(),
                    first.content_digest.as_bytes().as_slice(),
                ],
            )
            .unwrap();
        deletion.commit().unwrap();
        assert_eq!(
            database
                .query_row(
                    "SELECT count(*) FROM revision_local_current_notes \
                     WHERE package_id = ?1 AND revision_content_digest = ?2",
                    params![
                        first.package_id.as_bytes().as_slice(),
                        first.content_digest.as_bytes().as_slice(),
                    ],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        assert_eq!(
            database
                .query_row(
                    "SELECT count(*) FROM revision_reference_label_bindings \
                     WHERE package_id = ?1 AND revision_content_digest = ?2",
                    params![
                        second.package_id.as_bytes().as_slice(),
                        second.content_digest.as_bytes().as_slice(),
                    ],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
    }
}
