//! PersistenceSchemaV3 Instance and Managed Input repository.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use super::{
    PactrunPersistence, PersistenceError,
    chunked_blob::{ChunkedBlobTable, insert_chunks, stream_chunks},
};
use crate::{
    domain::{
        ActiveInstanceBindingReference, InputDeclarationV1, InputIdentity, InputProtectionV1,
        InstanceCompilationState, InstanceId, InstanceName, InstanceStateVersion, InstanceSummary,
        InstanceView, MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1, ManagedInputBindingView,
        ManagedInputPayloadId, ManagedInputProtection, ManagedInputRole, RevisionContentDigest,
        RevisionCoreV1, RevisionIdentity,
    },
    revision_core_v1::decode_canonical_revision_core_v1,
};

const PAYLOAD_CHUNKS: ChunkedBlobTable = ChunkedBlobTable {
    representation_maximum: crate::domain::MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1,
    insert_chunk_sql: "INSERT INTO managed_input_payload_chunks(        instance_id, payload_id, chunk_index, chunk_bytes     ) VALUES (?1, ?2, ?3, ?4)",
    select_chunks_sql: "SELECT chunk_index, chunk_bytes FROM managed_input_payload_chunks          WHERE instance_id=?1 AND payload_id=?2 ORDER BY chunk_index",
    read_operation: "read Managed Input staging",
    write_operation: "write Managed Input chunk",
    invalid: PersistenceError::InvalidManagedInput,
    corrupt: PersistenceError::CorruptManagedInput,
};

pub(crate) struct ManagedInputWrite<'a> {
    pub(crate) input_id: InputIdentity,
    pub(crate) byte_len: u64,
    pub(crate) reader: &'a mut dyn Read,
}

impl PactrunPersistence {
    pub(crate) fn create_instance(
        &self,
        name: InstanceName,
        active_revision: RevisionIdentity,
        initial: &mut [ManagedInputWrite<'_>],
    ) -> Result<InstanceView, PersistenceError> {
        initial.sort_by(|left, right| left.input_id.cmp(&right.input_id));
        if initial
            .windows(2)
            .any(|pair| pair[0].input_id == pair[1].input_id)
        {
            return Err(PersistenceError::InvalidManagedInput(
                "duplicate initial Input identity".to_owned(),
            ));
        }
        let instance_id = InstanceId::generate().map_err(|error| {
            PersistenceError::InvalidManagedInput(format!("generate InstanceId: {error}"))
        })?;
        let state_version = InstanceStateVersion::generate().map_err(|error| {
            PersistenceError::InvalidManagedInput(format!("generate state version: {error}"))
        })?;
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin Instance creation", error))?;
        self.check_write_admission(&transaction)?;
        let core = load_revision_core(&transaction, &active_revision)?;
        for input in initial.iter() {
            if input_declaration(&core, &input.input_id).is_none() {
                return Err(PersistenceError::InvalidManagedInput(format!(
                    "Input {} is not active in the selected Revision",
                    input.input_id.as_str()
                )));
            }
        }
        transaction
            .execute(
                "INSERT INTO instances( \
                    instance_id, instance_name, active_package_id, \
                    active_revision_content_digest, instance_state_version \
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    instance_id.as_bytes().as_slice(),
                    name.as_bytes(),
                    active_revision.package_id.as_bytes().as_slice(),
                    active_revision.content_digest.as_bytes().as_slice(),
                    state_version.as_bytes().as_slice(),
                ],
            )
            .map_err(|error| PersistenceError::sqlite("insert Instance", error))?;
        transaction.execute(
            "INSERT INTO instance_recovery_consequence_versions(instance_id, consequence_version) VALUES (?1, 0)",
            [instance_id.as_bytes().as_slice()],
        ).map_err(|error| PersistenceError::sqlite("initialize Instance consequence version", error))?;
        for input in initial.iter_mut() {
            let declaration = input_declaration(&core, &input.input_id)
                .expect("initial declarations were validated");
            let protection = declaration_protection(declaration.protection);
            let payload_id = insert_payload(&transaction, instance_id, protection, input)?;
            insert_binding(&transaction, instance_id, &input.input_id, payload_id)?;
        }
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit Instance creation", error))?;
        drop(database);
        self.load_instance_by_id(instance_id)?.ok_or_else(|| {
            PersistenceError::CorruptManagedInput("created Instance disappeared".to_owned())
        })
    }

    pub(crate) fn resolve_instance_name(
        &self,
        name: &InstanceName,
    ) -> Result<Option<InstanceId>, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        resolve_instance_name_from(&database, name)
    }

    pub(crate) fn list_instances(&self) -> Result<Vec<InstanceSummary>, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let mut statement = database
            .prepare(
                "SELECT instance_id, instance_name, active_package_id, \
                        active_revision_content_digest, instance_state_version \
                 FROM instances ORDER BY instance_name, instance_id",
            )
            .map_err(|error| PersistenceError::sqlite("prepare Instance enumeration", error))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                ))
            })
            .map_err(|error| PersistenceError::sqlite("enumerate Instances", error))?;
        let mut result = Vec::new();
        for row in rows {
            let (id, name, package, digest, version) =
                row.map_err(|error| PersistenceError::sqlite("read Instance row", error))?;
            let id = instance_id(id)?;
            let view = load_instance_from(&database, id, name, package, digest, version)?;
            result.push(InstanceSummary {
                id: view.id,
                name: view.name,
                active_revision: view.active_revision,
                state_version: view.state_version,
                required_inputs_satisfied: view.required_inputs_satisfied,
            });
        }
        ensure_ordered(&result, |left, right| {
            left.name.cmp(&right.name).then(left.id.cmp(&right.id))
        })?;
        Ok(result)
    }

    pub(crate) fn load_instance_by_id(
        &self,
        id: InstanceId,
    ) -> Result<Option<InstanceView>, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        load_instance_view_from(&database, id)
    }

    pub(crate) fn observe_instance_compilation_state(
        &self,
        id: InstanceId,
    ) -> Result<Option<InstanceCompilationState>, PersistenceError> {
        let database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let row = database
            .query_row(
                "SELECT active_package_id, active_revision_content_digest, instance_state_version \
                 FROM instances WHERE instance_id=?1",
                [id.as_bytes().as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| {
                PersistenceError::sqlite("observe Instance compilation state", error)
            })?;
        let Some((package, digest, version)) = row else {
            return Ok(None);
        };
        let active_revision = revision_identity(package, digest)?;
        super::sqlite_v5::load_consequence_version(&database, id)?;
        let state_version = state_version(version)?;
        let core = load_revision_core(&database, &active_revision)?;
        let mut active_bindings = Vec::new();
        let mut required_inputs_satisfied = true;
        for declaration in core.inputs() {
            match binding_payload(&database, id, &declaration.id)? {
                Some((payload, stored)) => active_bindings.push(ActiveInstanceBindingReference {
                    input: declaration.id.clone(),
                    payload,
                    protection: stored.sticky(declaration_protection(declaration.protection)),
                }),
                None if declaration.required => required_inputs_satisfied = false,
                None => {}
            }
        }
        Ok(Some(InstanceCompilationState {
            instance: id,
            state_version,
            active_revision,
            active_bindings,
            required_inputs_satisfied,
        }))
    }
    pub(crate) fn set_input(
        &self,
        instance: InstanceId,
        expected: InstanceStateVersion,
        input: &mut ManagedInputWrite<'_>,
    ) -> Result<InstanceStateVersion, PersistenceError> {
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin Managed Input set", error))?;
        self.check_write_admission(&transaction)?;
        let (_, revision, current) = instance_header(&transaction, instance)?;
        if current != expected {
            return Err(PersistenceError::StaleInstanceState);
        }
        super::sqlite_migration_runs::require_no_migration_mutator(&transaction, instance)?;
        let core = load_revision_core(&transaction, &revision)?;
        let declaration = input_declaration(&core, &input.input_id).ok_or_else(|| {
            PersistenceError::InvalidManagedInput(
                "retained or unknown Input cannot be set in M2".to_owned(),
            )
        })?;
        let previous = binding_payload(&transaction, instance, &input.input_id)?;
        let declared = declaration_protection(declaration.protection);
        if previous.as_ref().is_some_and(|(_, stored)| {
            declared == ManagedInputProtection::Secret && *stored == ManagedInputProtection::Normal
        }) {
            return Err(PersistenceError::CorruptManagedInput(
                "active Secret references a Normal payload".to_owned(),
            ));
        }
        let protection = previous
            .as_ref()
            .map(|(_, stored)| stored.sticky(declared))
            .unwrap_or(declared);
        let payload_id = insert_payload(&transaction, instance, protection, input)?;
        transaction
            .execute(
                "INSERT INTO managed_input_bindings(instance_id, input_identity, payload_id) \
                 VALUES (?1, ?2, ?3) \
                 ON CONFLICT(instance_id, input_identity) DO UPDATE SET payload_id=excluded.payload_id",
                params![instance.as_bytes().as_slice(), input.input_id.as_str().as_bytes(), payload_id.as_bytes().as_slice()],
            )
            .map_err(|error| PersistenceError::sqlite("publish Managed Input binding", error))?;
        if let Some((old, _)) = previous {
            reclaim_payload_if_unreferenced(&transaction, instance, old)?;
        }
        let next = fresh_state_version()?;
        update_state_version(&transaction, instance, next)?;
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit Managed Input set", error))?;
        Ok(next)
    }

    pub(crate) fn delete_input(
        &self,
        instance: InstanceId,
        expected: InstanceStateVersion,
        input: &InputIdentity,
    ) -> Result<InstanceStateVersion, PersistenceError> {
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin Managed Input delete", error))?;
        self.check_write_admission(&transaction)?;
        let (_, revision, current) = instance_header(&transaction, instance)?;
        if current != expected {
            return Err(PersistenceError::StaleInstanceState);
        }
        super::sqlite_migration_runs::require_no_migration_mutator(&transaction, instance)?;
        let core = load_revision_core(&transaction, &revision)?;
        let Some((payload, _)) = binding_payload(&transaction, instance, input)? else {
            if input_declaration(&core, input).is_none() {
                return Err(PersistenceError::InvalidManagedInput(
                    "unknown absent Input cannot be deleted".to_owned(),
                ));
            }
            transaction.commit().map_err(|error| {
                PersistenceError::sqlite("commit idempotent Input delete", error)
            })?;
            return Ok(current);
        };
        if input_declaration(&core, input).is_some_and(|declaration| declaration.required) {
            return Err(PersistenceError::InvalidManagedInput(
                "a bound active required Input cannot be deleted".to_owned(),
            ));
        }
        transaction
            .execute(
                "DELETE FROM managed_input_bindings WHERE instance_id=?1 AND input_identity=?2",
                params![instance.as_bytes().as_slice(), input.as_str().as_bytes()],
            )
            .map_err(|error| PersistenceError::sqlite("delete Managed Input binding", error))?;
        reclaim_payload_if_unreferenced(&transaction, instance, payload)?;
        let next = fresh_state_version()?;
        update_state_version(&transaction, instance, next)?;
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit Managed Input delete", error))?;
        Ok(next)
    }

    pub(crate) fn export_input(
        &self,
        instance: InstanceId,
        input: &InputIdentity,
        authorize_secret: bool,
        destination: &mut impl Write,
    ) -> Result<InstanceStateVersion, PersistenceError> {
        let mut database = self.open_read_connection()?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| PersistenceError::sqlite("begin ExportInput snapshot", error))?;
        let (_, revision, version) = instance_header(&transaction, instance)?;
        let (payload, stored) =
            binding_payload(&transaction, instance, input)?.ok_or_else(|| {
                PersistenceError::InvalidManagedInput("Input has no managed binding".to_owned())
            })?;
        let core = load_revision_core(&transaction, &revision)?;
        let effective = match input_declaration(&core, input) {
            Some(declaration) => {
                let declared = declaration_protection(declaration.protection);
                if declared == ManagedInputProtection::Secret
                    && stored == ManagedInputProtection::Normal
                {
                    return Err(PersistenceError::CorruptManagedInput(
                        "active Secret references a Normal payload".to_owned(),
                    ));
                }
                stored.sticky(declared)
            }
            None => stored,
        };
        if effective == ManagedInputProtection::Secret && !authorize_secret {
            return Err(PersistenceError::UnauthorizedSecretExport);
        }
        stream_payload(&transaction, instance, payload, destination)?;
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("close ExportInput snapshot", error))?;
        Ok(version)
    }
}

fn resolve_instance_name_from(
    database: &Connection,
    name: &InstanceName,
) -> Result<Option<InstanceId>, PersistenceError> {
    let mut statement = database
        .prepare("SELECT instance_id FROM instances WHERE instance_name=?1")
        .map_err(|error| PersistenceError::sqlite("prepare Instance name resolution", error))?;
    let mut rows = statement
        .query([name.as_bytes()])
        .map_err(|error| PersistenceError::sqlite("resolve Instance name", error))?;
    let Some(first) = rows
        .next()
        .map_err(|error| PersistenceError::sqlite("read Instance name resolution", error))?
    else {
        return Ok(None);
    };
    let id = first
        .get::<_, Vec<u8>>(0)
        .map_err(|error| PersistenceError::sqlite("read resolved Instance id", error))?;
    if rows
        .next()
        .map_err(|error| PersistenceError::sqlite("check Instance name uniqueness", error))?
        .is_some()
    {
        return Err(PersistenceError::CorruptInstance(format!(
            "multiple rows have InstanceName {:?}",
            name.as_str()
        )));
    }
    Ok(Some(instance_id(id)?))
}

pub(super) fn insert_payload(
    transaction: &Transaction<'_>,
    instance: InstanceId,
    protection: ManagedInputProtection,
    input: &mut ManagedInputWrite<'_>,
) -> Result<ManagedInputPayloadId, PersistenceError> {
    if input.byte_len > MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1 {
        return Err(PersistenceError::InvalidManagedInput(
            "Managed Input exceeds the M2 size limit".to_owned(),
        ));
    }
    let payload = ManagedInputPayloadId::generate().map_err(|error| {
        PersistenceError::InvalidManagedInput(format!("generate payload identity: {error}"))
    })?;
    transaction
        .execute(
            "INSERT INTO managed_input_payloads(instance_id, payload_id, protection_rank, byte_length) \
             VALUES (?1, ?2, ?3, ?4)",
            params![instance.as_bytes().as_slice(), payload.as_bytes().as_slice(), i64::from(protection.rank()), i64::try_from(input.byte_len).expect("M2 payload length fits i64")],
        )
        .map_err(|error| PersistenceError::sqlite("insert Managed Input payload", error))?;
    insert_chunks(
        transaction,
        &PAYLOAD_CHUNKS,
        [
            instance.as_bytes().as_slice(),
            payload.as_bytes().as_slice(),
        ],
        input.reader,
        input.byte_len,
    )?;
    Ok(payload)
}

pub(super) fn stream_payload(
    database: &Connection,
    instance: InstanceId,
    payload: ManagedInputPayloadId,
    destination: &mut impl Write,
) -> Result<(), PersistenceError> {
    let (declared_length, protection_rank): (i64, i64) = database.query_row(
        "SELECT byte_length, protection_rank FROM managed_input_payloads WHERE instance_id=?1 AND payload_id=?2",
        params![instance.as_bytes().as_slice(), payload.as_bytes().as_slice()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).map_err(|error| PersistenceError::sqlite("load Managed Input payload header", error))?;
    ManagedInputProtection::from_rank(protection_rank)
        .map_err(|error| PersistenceError::CorruptManagedInput(error.to_string()))?;
    let declared_length = u64::try_from(declared_length)
        .map_err(|_| PersistenceError::CorruptManagedInput("negative payload length".to_owned()))?;
    if declared_length > MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1 {
        return Err(PersistenceError::CorruptManagedInput(
            "oversize payload header".to_owned(),
        ));
    }
    stream_chunks(
        database,
        &PAYLOAD_CHUNKS,
        [
            instance.as_bytes().as_slice(),
            payload.as_bytes().as_slice(),
        ],
        declared_length,
        destination,
    )?;
    Ok(())
}

pub(super) fn load_instance_view_from(
    database: &Connection,
    id: InstanceId,
) -> Result<Option<InstanceView>, PersistenceError> {
    let row = database.query_row("SELECT instance_name,active_package_id,active_revision_content_digest,instance_state_version FROM instances WHERE instance_id=?1",
        [id.as_bytes().as_slice()], |row| Ok((row.get::<_,Vec<u8>>(0)?,row.get::<_,Vec<u8>>(1)?,row.get::<_,Vec<u8>>(2)?,row.get::<_,Vec<u8>>(3)?)))
        .optional().map_err(|error|PersistenceError::sqlite("load Instance",error))?;
    row.map(|(name, package, digest, version)| {
        load_instance_from(database, id, name, package, digest, version)
    })
    .transpose()
}

fn load_instance_from(
    database: &Connection,
    id: InstanceId,
    name: Vec<u8>,
    package: Vec<u8>,
    digest: Vec<u8>,
    version: Vec<u8>,
) -> Result<InstanceView, PersistenceError> {
    super::sqlite_v5::load_consequence_version(database, id)?;
    let name = String::from_utf8(name).map_err(|_| {
        PersistenceError::CorruptManagedInput("InstanceName is not UTF-8".to_owned())
    })?;
    let name = InstanceName::parse(name)
        .map_err(|error| PersistenceError::CorruptManagedInput(error.to_string()))?;
    let revision = revision_identity(package, digest)?;
    let state_version = state_version(version)?;
    let core = load_revision_core(database, &revision)?;
    let active = core
        .inputs()
        .iter()
        .map(|declaration| (declaration.id.clone(), declaration))
        .collect::<BTreeMap<_, _>>();
    let mut stored = BTreeMap::new();
    let mut statement = database.prepare(
        "SELECT b.input_identity, p.protection_rank FROM managed_input_bindings b \
         JOIN managed_input_payloads p ON p.instance_id=b.instance_id AND p.payload_id=b.payload_id \
         WHERE b.instance_id=?1 ORDER BY b.input_identity",
    ).map_err(|error| PersistenceError::sqlite("prepare Instance bindings", error))?;
    let rows = statement
        .query_map([id.as_bytes().as_slice()], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|error| PersistenceError::sqlite("query Instance bindings", error))?;
    for row in rows {
        let (input, rank) =
            row.map_err(|error| PersistenceError::sqlite("read Instance binding", error))?;
        let input = String::from_utf8(input).map_err(|_| {
            PersistenceError::CorruptManagedInput("InputIdentity is not UTF-8".to_owned())
        })?;
        let input = InputIdentity::parse(input)
            .map_err(|error| PersistenceError::CorruptManagedInput(error.to_string()))?;
        let protection = ManagedInputProtection::from_rank(rank)
            .map_err(|error| PersistenceError::CorruptManagedInput(error.to_string()))?;
        if stored.insert(input, protection).is_some() {
            return Err(PersistenceError::CorruptManagedInput(
                "duplicate binding".to_owned(),
            ));
        }
    }
    let mut ids = active.keys().cloned().collect::<BTreeSet<_>>();
    ids.extend(stored.keys().cloned());
    let mut bindings = Vec::new();
    let mut required_inputs_satisfied = true;
    for input in ids {
        match (active.get(&input), stored.get(&input)) {
            (Some(declaration), Some(stored)) => {
                let declared = declaration_protection(declaration.protection);
                if declared == ManagedInputProtection::Secret
                    && *stored == ManagedInputProtection::Normal
                {
                    return Err(PersistenceError::CorruptManagedInput(
                        "active Secret references a Normal payload".to_owned(),
                    ));
                }
                bindings.push(ManagedInputBindingView {
                    input_id: input,
                    role: ManagedInputRole::Active {
                        required: declaration.required,
                    },
                    present: true,
                    protection: stored.sticky(declared),
                });
            }
            (Some(declaration), None) => {
                required_inputs_satisfied &= !declaration.required;
                bindings.push(ManagedInputBindingView {
                    input_id: input,
                    role: ManagedInputRole::Active {
                        required: declaration.required,
                    },
                    present: false,
                    protection: declaration_protection(declaration.protection),
                });
            }
            (None, Some(stored)) => bindings.push(ManagedInputBindingView {
                input_id: input,
                role: ManagedInputRole::Retained,
                present: true,
                protection: *stored,
            }),
            (None, None) => unreachable!(),
        }
    }
    Ok(InstanceView {
        id,
        name,
        active_revision: revision,
        state_version,
        required_inputs_satisfied,
        bindings,
    })
}

pub(super) fn instance_header(
    database: &Connection,
    instance: InstanceId,
) -> Result<(InstanceName, RevisionIdentity, InstanceStateVersion), PersistenceError> {
    let row = database.query_row(
        "SELECT instance_name, active_package_id, active_revision_content_digest, instance_state_version \
         FROM instances WHERE instance_id=?1",
        [instance.as_bytes().as_slice()],
        |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?, row.get::<_, Vec<u8>>(2)?, row.get::<_, Vec<u8>>(3)?)),
    ).optional().map_err(|error| PersistenceError::sqlite("load Instance header", error))?
        .ok_or_else(|| PersistenceError::MissingInstance(instance.to_string()))?;
    super::sqlite_v5::load_consequence_version(database, instance)?;
    let name = InstanceName::parse(String::from_utf8(row.0).map_err(|_| {
        PersistenceError::CorruptManagedInput("InstanceName is not UTF-8".to_owned())
    })?)
    .map_err(|error| PersistenceError::CorruptManagedInput(error.to_string()))?;
    Ok((
        name,
        revision_identity(row.1, row.2)?,
        state_version(row.3)?,
    ))
}

pub(super) fn load_revision_core(
    database: &Connection,
    identity: &RevisionIdentity,
) -> Result<RevisionCoreV1, PersistenceError> {
    let bytes = database
        .query_row(
            "SELECT core_jcs FROM revisions WHERE package_id=?1 AND revision_content_digest=?2",
            params![
                identity.package_id.as_bytes().as_slice(),
                identity.content_digest.as_bytes().as_slice()
            ],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(|error| PersistenceError::sqlite("load active Revision Core", error))?
        .ok_or_else(|| PersistenceError::MissingRevision(identity.clone()))?;
    decode_canonical_revision_core_v1(&bytes)
        .map_err(|error| PersistenceError::CorruptRevision(error.to_string()))
}

fn input_declaration<'a>(
    core: &'a RevisionCoreV1,
    input: &InputIdentity,
) -> Option<&'a InputDeclarationV1> {
    core.inputs()
        .iter()
        .find(|declaration| &declaration.id == input)
}

fn declaration_protection(value: InputProtectionV1) -> ManagedInputProtection {
    match value {
        InputProtectionV1::Normal => ManagedInputProtection::Normal,
        InputProtectionV1::Secret => ManagedInputProtection::Secret,
    }
}

pub(super) fn binding_payload(
    database: &Connection,
    instance: InstanceId,
    input: &InputIdentity,
) -> Result<Option<(ManagedInputPayloadId, ManagedInputProtection)>, PersistenceError> {
    database.query_row(
        "SELECT b.payload_id, p.protection_rank FROM managed_input_bindings b \
         JOIN managed_input_payloads p ON p.instance_id=b.instance_id AND p.payload_id=b.payload_id \
         WHERE b.instance_id=?1 AND b.input_identity=?2",
        params![instance.as_bytes().as_slice(), input.as_str().as_bytes()],
        |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?)),
    ).optional().map_err(|error| PersistenceError::sqlite("load Managed Input binding", error))?
        .map(|(payload, rank)| Ok((payload_id(payload)?, ManagedInputProtection::from_rank(rank).map_err(|error| PersistenceError::CorruptManagedInput(error.to_string()))?)))
        .transpose()
}

fn insert_binding(
    transaction: &Transaction<'_>,
    instance: InstanceId,
    input: &InputIdentity,
    payload: ManagedInputPayloadId,
) -> Result<(), PersistenceError> {
    transaction.execute(
        "INSERT INTO managed_input_bindings(instance_id, input_identity, payload_id) VALUES (?1, ?2, ?3)",
        params![instance.as_bytes().as_slice(), input.as_str().as_bytes(), payload.as_bytes().as_slice()],
    ).map_err(|error| PersistenceError::sqlite("insert Managed Input binding", error))?;
    Ok(())
}

/// Reclaims a payload only when no current binding and no execution pin
/// references it. The decision is an explicit lookup: a foreign-key failure
/// would abort the enclosing mutation instead of retaining the payload.
pub(super) fn reclaim_payload_if_unreferenced(
    transaction: &Transaction<'_>,
    instance: InstanceId,
    payload: ManagedInputPayloadId,
) -> Result<bool, PersistenceError> {
    let referenced: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM managed_input_bindings WHERE instance_id=?1 AND payload_id=?2) \
             OR EXISTS(SELECT 1 FROM run_payload_pins WHERE instance_id=?1 AND payload_id=?2) \
             OR EXISTS(SELECT 1 FROM run_migration_payload_pins WHERE instance_id=?1 AND payload_id=?2) \
             OR EXISTS(SELECT 1 FROM run_migration_checkpoint_bindings WHERE instance_id=?1 AND payload_id=?2)",
            params![
                instance.as_bytes().as_slice(),
                payload.as_bytes().as_slice()
            ],
            |row| row.get(0),
        )
        .map_err(|error| PersistenceError::sqlite("check Managed Input payload references", error))?;
    if referenced {
        return Ok(false);
    }
    transaction
        .execute(
            "DELETE FROM managed_input_payloads WHERE instance_id=?1 AND payload_id=?2",
            params![
                instance.as_bytes().as_slice(),
                payload.as_bytes().as_slice()
            ],
        )
        .map_err(|error| PersistenceError::sqlite("reclaim Managed Input payload", error))?;
    Ok(true)
}

pub(super) fn update_state_version(
    transaction: &Transaction<'_>,
    instance: InstanceId,
    version: InstanceStateVersion,
) -> Result<(), PersistenceError> {
    transaction
        .execute(
            "UPDATE instances SET instance_state_version=?2 WHERE instance_id=?1",
            params![
                instance.as_bytes().as_slice(),
                version.as_bytes().as_slice()
            ],
        )
        .map_err(|error| PersistenceError::sqlite("publish InstanceStateVersion", error))?;
    Ok(())
}

pub(super) fn fresh_state_version() -> Result<InstanceStateVersion, PersistenceError> {
    InstanceStateVersion::generate().map_err(|error| {
        PersistenceError::InvalidManagedInput(format!("generate state version: {error}"))
    })
}

pub(super) fn revision_identity(
    package: Vec<u8>,
    digest: Vec<u8>,
) -> Result<RevisionIdentity, PersistenceError> {
    let package: [u8; 16] = package.try_into().map_err(|_| {
        PersistenceError::CorruptManagedInput("invalid PackageId length".to_owned())
    })?;
    let digest: [u8; 32] = digest.try_into().map_err(|_| {
        PersistenceError::CorruptManagedInput("invalid Revision digest length".to_owned())
    })?;
    Ok(RevisionIdentity::new(
        crate::domain::PackageId::from_bytes(package),
        RevisionContentDigest::from_bytes(digest),
    ))
}

pub(super) fn instance_id(bytes: Vec<u8>) -> Result<InstanceId, PersistenceError> {
    bytes
        .try_into()
        .map(InstanceId::from_bytes)
        .map_err(|_| PersistenceError::CorruptManagedInput("invalid InstanceId length".to_owned()))
}
pub(super) fn payload_id(bytes: Vec<u8>) -> Result<ManagedInputPayloadId, PersistenceError> {
    bytes
        .try_into()
        .map(ManagedInputPayloadId::from_bytes)
        .map_err(|_| PersistenceError::CorruptManagedInput("invalid payload ID length".to_owned()))
}
pub(super) fn state_version(bytes: Vec<u8>) -> Result<InstanceStateVersion, PersistenceError> {
    bytes
        .try_into()
        .map(InstanceStateVersion::from_bytes)
        .map_err(|_| {
            PersistenceError::CorruptManagedInput("invalid state-version length".to_owned())
        })
}

pub(super) fn ensure_ordered<T>(
    values: &[T],
    compare: impl Fn(&T, &T) -> std::cmp::Ordering,
) -> Result<(), PersistenceError> {
    if values
        .windows(2)
        .all(|pair| compare(&pair[0], &pair[1]).is_lt())
    {
        Ok(())
    } else {
        Err(PersistenceError::CorruptManagedInput(
            "SQL/domain ordering mismatch".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Cursor, Write},
        path::Path,
        sync::{Arc, mpsc},
        thread,
    };

    use tempfile::TempDir;

    use super::*;
    use crate::{
        domain::{
            MANAGED_INPUT_CHUNK_BYTES_V1, RevisionCoreProjectionInputV1,
            RuntimeContentProjectionInputV1, ValidatedRevisionContentV1, project_revision_core_v1,
            project_runtime_content_closure_v1, validate_revision_content_v1,
        },
        persistence::PactrunPersistence,
    };

    fn root() -> (TempDir, std::path::PathBuf) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m2-instance-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("instance-")
            .tempdir_in(parent)
            .unwrap();
        let root = temporary.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("database")).unwrap();
        fs::create_dir(root.join("runtime-content")).unwrap();
        fs::create_dir(root.join("staging")).unwrap();
        (temporary, root)
    }

    #[test]
    fn duplicate_instance_name_rows_are_repository_corruption() {
        let database = Connection::open_in_memory().unwrap();
        database
            .execute_batch(
                "CREATE TABLE instances( \
                     instance_id BLOB NOT NULL, \
                     instance_name BLOB NOT NULL \
                 ) STRICT;",
            )
            .unwrap();
        let name = InstanceName::parse("duplicate-name").unwrap();
        database
            .execute(
                "INSERT INTO instances(instance_id, instance_name) VALUES(?1, ?2)",
                params![vec![1_u8; 16], name.as_bytes()],
            )
            .unwrap();
        database
            .execute(
                "INSERT INTO instances(instance_id, instance_name) VALUES(?1, ?2)",
                params![vec![2_u8; 16], name.as_bytes()],
            )
            .unwrap();

        assert!(matches!(
            resolve_instance_name_from(&database, &name),
            Err(PersistenceError::CorruptInstance(_))
        ));
    }

    fn revision(persistence: &PactrunPersistence) -> RevisionIdentity {
        let core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
            inputs: vec![
                InputDeclarationV1 {
                    id: InputIdentity::parse("required").unwrap(),
                    required: true,
                    protection: InputProtectionV1::Normal,
                },
                InputDeclarationV1 {
                    id: InputIdentity::parse("secret").unwrap(),
                    required: false,
                    protection: InputProtectionV1::Secret,
                },
            ],
            actions: Vec::new(),
            snapshot: None,
            migrations: Vec::new(),
            cleanup: None,
        })
        .unwrap();
        let runtime_content = project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 {
            files: Vec::new(),
        })
        .unwrap();
        let content: ValidatedRevisionContentV1 =
            validate_revision_content_v1(core, runtime_content).unwrap();
        persistence
            .persist_revision(crate::domain::PackageId::from_bytes([9; 16]), &content, &[])
            .unwrap()
    }

    // Test-ID: PR-TEST-0075
    // Verifies: PR-REQ-0264, PR-REQ-0269
    #[test]
    fn chunked_payloads_round_trip_and_corruption_is_rejected() {
        let (_temporary, root) = root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let revision = revision(&persistence);
        let mut bytes = Cursor::new(vec![7_u8; MANAGED_INPUT_CHUNK_BYTES_V1 + 3]);
        let mut empty = Cursor::new(Vec::new());
        let mut initial = [
            ManagedInputWrite {
                input_id: InputIdentity::parse("required").unwrap(),
                byte_len: (MANAGED_INPUT_CHUNK_BYTES_V1 + 3) as u64,
                reader: &mut bytes,
            },
            ManagedInputWrite {
                input_id: InputIdentity::parse("secret").unwrap(),
                byte_len: 0,
                reader: &mut empty,
            },
        ];
        let view = persistence
            .create_instance(
                InstanceName::parse("node-a").unwrap(),
                revision,
                &mut initial,
            )
            .unwrap();
        let mut output = Vec::new();
        persistence
            .export_input(
                view.id,
                &InputIdentity::parse("required").unwrap(),
                false,
                &mut output,
            )
            .unwrap();
        assert_eq!(output.len(), MANAGED_INPUT_CHUNK_BYTES_V1 + 3);
        let mut empty_output = Vec::new();
        persistence
            .export_input(
                view.id,
                &InputIdentity::parse("secret").unwrap(),
                true,
                &mut empty_output,
            )
            .unwrap();
        assert!(empty_output.is_empty());

        let database = persistence.database.lock().unwrap();
        let payload_id = database
            .query_row(
                "SELECT payload_id FROM managed_input_bindings \
                 WHERE instance_id=?1 AND input_identity=?2",
                params![
                    view.id.as_bytes().as_slice(),
                    InputIdentity::parse("required")
                        .unwrap()
                        .as_str()
                        .as_bytes(),
                ],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .unwrap();
        database
            .execute(
                "DELETE FROM managed_input_payload_chunks \
                 WHERE instance_id=?1 AND payload_id=?2 AND chunk_index=0",
                params![view.id.as_bytes().as_slice(), payload_id],
            )
            .unwrap();
        drop(database);
        assert!(matches!(
            persistence.export_input(
                view.id,
                &InputIdentity::parse("required").unwrap(),
                false,
                &mut Vec::new(),
            ),
            Err(PersistenceError::CorruptManagedInput(_))
        ));
    }

    // Test-ID: PR-TEST-0076
    // Verifies: PR-REQ-0033, PR-REQ-0034, PR-REQ-0266, PR-REQ-0267, PR-REQ-0268
    #[test]
    fn strict_cas_required_delete_and_secret_authorization_hold() {
        let (_temporary, root) = root();
        let persistence = PactrunPersistence::open(&root).unwrap();
        let revision = revision(&persistence);
        let mut initial: [ManagedInputWrite<'_>; 0] = [];
        let view = persistence
            .create_instance(
                InstanceName::parse("node-b").unwrap(),
                revision,
                &mut initial,
            )
            .unwrap();
        let mut secret = Cursor::new(b"secret".to_vec());
        let mut write = ManagedInputWrite {
            input_id: InputIdentity::parse("secret").unwrap(),
            byte_len: 6,
            reader: &mut secret,
        };
        let next = persistence
            .set_input(view.id, view.state_version, &mut write)
            .unwrap();
        assert!(matches!(
            persistence.set_input(view.id, view.state_version, &mut write),
            Err(PersistenceError::StaleInstanceState)
        ));
        assert!(matches!(
            persistence.export_input(view.id, &write.input_id, false, &mut Vec::new()),
            Err(PersistenceError::UnauthorizedSecretExport)
        ));
        let mut output = Vec::new();
        persistence
            .export_input(view.id, &write.input_id, true, &mut output)
            .unwrap();
        assert_eq!(output, b"secret");

        let database = persistence.database.lock().unwrap();
        database
            .execute(
                "UPDATE managed_input_payloads SET protection_rank=0 \
                 WHERE instance_id=?1 AND payload_id=( \
                   SELECT payload_id FROM managed_input_bindings \
                   WHERE instance_id=?1 AND input_identity=?2 \
                 )",
                params![
                    view.id.as_bytes().as_slice(),
                    write.input_id.as_str().as_bytes(),
                ],
            )
            .unwrap();
        drop(database);
        assert!(matches!(
            persistence.load_instance_by_id(view.id),
            Err(PersistenceError::CorruptManagedInput(_))
        ));
        assert!(matches!(
            persistence.export_input(view.id, &write.input_id, true, &mut Vec::new()),
            Err(PersistenceError::CorruptManagedInput(_))
        ));
        assert_eq!(
            persistence
                .delete_input(view.id, next, &InputIdentity::parse("required").unwrap())
                .unwrap(),
            next
        );
        assert!(matches!(
            persistence.delete_input(view.id, next, &InputIdentity::parse("unknown").unwrap()),
            Err(PersistenceError::InvalidManagedInput(_))
        ));

        let mut required = Cursor::new(b"required-value".to_vec());
        let mut required_write = ManagedInputWrite {
            input_id: InputIdentity::parse("required").unwrap(),
            byte_len: 14,
            reader: &mut required,
        };
        let bound = persistence
            .set_input(view.id, next, &mut required_write)
            .unwrap();
        assert!(matches!(
            persistence.delete_input(view.id, bound, &InputIdentity::parse("required").unwrap()),
            Err(PersistenceError::InvalidManagedInput(_))
        ));
    }

    struct BlockingWriter {
        bytes: Vec<u8>,
        started: Option<mpsc::Sender<()>>,
        release: mpsc::Receiver<()>,
    }

    impl Write for BlockingWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            if let Some(started) = self.started.take() {
                started.send(()).unwrap();
                self.release.recv().unwrap();
            }
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    // Test-ID: PR-TEST-0077
    // Verifies: PR-REQ-0266, PR-REQ-0267
    #[test]
    fn wal_export_snapshot_does_not_block_mutation_and_never_switches_payload() {
        let (_temporary, root) = root();
        let persistence = Arc::new(PactrunPersistence::open(&root).unwrap());
        let revision = revision(&persistence);
        let old_bytes = vec![0x41; MANAGED_INPUT_CHUNK_BYTES_V1 + 31];
        let mut old_reader = Cursor::new(old_bytes.clone());
        let mut initial = [ManagedInputWrite {
            input_id: InputIdentity::parse("secret").unwrap(),
            byte_len: old_bytes.len() as u64,
            reader: &mut old_reader,
        }];
        let view = persistence
            .create_instance(
                InstanceName::parse("export-race").unwrap(),
                revision,
                &mut initial,
            )
            .unwrap();

        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let export_persistence = Arc::clone(&persistence);
        let instance = view.id;
        let export = thread::spawn(move || {
            let mut destination = BlockingWriter {
                bytes: Vec::new(),
                started: Some(started_tx),
                release: release_rx,
            };
            export_persistence
                .export_input(
                    instance,
                    &InputIdentity::parse("secret").unwrap(),
                    true,
                    &mut destination,
                )
                .unwrap();
            destination.bytes
        });

        started_rx.recv().unwrap();
        let new_bytes = b"replacement".to_vec();
        let mut new_reader = Cursor::new(new_bytes.clone());
        let mut replacement = ManagedInputWrite {
            input_id: InputIdentity::parse("secret").unwrap(),
            byte_len: new_bytes.len() as u64,
            reader: &mut new_reader,
        };
        persistence
            .set_input(view.id, view.state_version, &mut replacement)
            .unwrap();
        release_tx.send(()).unwrap();
        assert_eq!(export.join().unwrap(), old_bytes);

        let mut current = Vec::new();
        persistence
            .export_input(
                view.id,
                &InputIdentity::parse("secret").unwrap(),
                true,
                &mut current,
            )
            .unwrap();
        assert_eq!(current, new_bytes);
    }
}
