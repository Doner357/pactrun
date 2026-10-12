//! Reads from the exact admitted Input and runtime-content pins.

use super::*;

impl PactrunPersistence {
    pub(crate) fn admitted_payload_length(
        &self,
        run: RunId,
        input: &InputIdentity,
    ) -> Result<u64, PersistenceError> {
        let mut database = self.open_read_connection()?;
        let tx = database
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|e| PersistenceError::sqlite("read admitted payload length", e))?;
        let view = load_managed_run_from(&tx, run)?;
        if !matches!(
            view.state,
            RunState::Running(RunExecutionView {
                boundary: ActionRunBoundary::Admitted,
                ..
            })
        ) {
            return Err(PersistenceError::RunNotRunning);
        }
        let length:i64=tx.query_row("SELECT p.byte_length FROM run_payload_pins pins JOIN managed_input_payloads p ON p.instance_id=pins.instance_id AND p.payload_id=pins.payload_id WHERE pins.run_id=?1 AND pins.input_identity=?2",params![run.as_bytes().as_slice(),input.as_str().as_bytes()],|r|r.get(0)).map_err(|e|PersistenceError::sqlite("read pinned payload header",e))?;
        u64::try_from(length)
            .map_err(|_| PersistenceError::CorruptRun("negative pinned payload length".to_owned()))
    }

    pub(crate) fn stream_admitted_payload(
        &self,
        run: RunId,
        input: &InputIdentity,
        destination: &mut impl Write,
    ) -> Result<(), PersistenceError> {
        let mut database = self.open_read_connection()?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| PersistenceError::sqlite("begin admitted payload read", error))?;
        execution_row(&transaction, run)?.ok_or(PersistenceError::RunNotRunning)?;
        if !has_revision_pin(&transaction, run)? {
            return Err(PersistenceError::InvalidRunTransition(
                "Run is not admitted".to_owned(),
            ));
        }
        let row = transaction
            .query_row(
                "SELECT instance_id, payload_id FROM run_payload_pins \
                 WHERE run_id=?1 AND input_identity=?2",
                params![run.as_bytes().as_slice(), input.as_str().as_bytes()],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(|error| PersistenceError::sqlite("load admitted payload pin", error))?
            .ok_or_else(|| {
                PersistenceError::CorruptRun(format!(
                    "admitted Run has no payload pin for {}",
                    input.as_str()
                ))
            })?;
        stream_payload(
            &transaction,
            &self.runtime_content,
            instance_id(row.0)?,
            payload_id(row.1)?,
            destination,
        )?;
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("close admitted payload read", error))?;
        Ok(())
    }

    /// Complete Capture view from the pinned Revision and payload registry.
    /// No current Instance bindings participate in this read.
    pub(crate) fn admitted_capture_registry(
        &self,
        run: RunId,
    ) -> Result<Vec<crate::domain::PinnedInputBinding>, PersistenceError> {
        self.admitted_current_registry(run, false)
    }

    pub(crate) fn admitted_cleanup_registry(
        &self,
        run: RunId,
    ) -> Result<Vec<crate::domain::PinnedInputBinding>, PersistenceError> {
        self.admitted_current_registry(run, true)
    }

    fn admitted_current_registry(
        &self,
        run: RunId,
        cleanup: bool,
    ) -> Result<Vec<crate::domain::PinnedInputBinding>, PersistenceError> {
        use crate::domain::{
            InputProtectionV1, ManagedInputProtection, ManagedInputRole, PinnedInputBinding,
        };
        let mut database = self.open_read_connection()?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| {
                PersistenceError::sqlite("begin pinned Capture registry read", error)
            })?;
        let view = load_managed_run_from(&transaction, run)?;
        let matching = if cleanup {
            matches!(
                view.operation,
                ManagedRunIdentity::Deletion {
                    mode: crate::domain::DeletionMode::ManagedCleanup,
                    ..
                }
            )
        } else {
            matches!(view.operation, ManagedRunIdentity::Capture { .. })
        };
        if !matching
            || !matches!(
                view.state,
                RunState::Running(RunExecutionView {
                    boundary: ActionRunBoundary::Admitted,
                    ..
                })
            )
        {
            return Err(PersistenceError::InvalidRunTransition(
                "Run is not an admitted Capture".to_owned(),
            ));
        }
        let core = load_revision_core(&transaction, view.operation.revision())?;
        let mut statement = transaction.prepare(
            "SELECT pins.input_identity,pins.payload_id,p.protection_rank FROM run_payload_pins pins \
             LEFT JOIN managed_input_payloads p ON p.instance_id=pins.instance_id AND p.payload_id=pins.payload_id \
             WHERE pins.run_id=?1 ORDER BY pins.input_identity",
        ).map_err(|error| PersistenceError::sqlite("prepare pinned Capture registry", error))?;
        let rows = statement
            .query_map([run.as_bytes().as_slice()], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|error| PersistenceError::sqlite("read pinned Capture registry", error))?;
        let mut pinned = BTreeMap::new();
        for row in rows {
            let (input, payload, protection) = row
                .map_err(|error| PersistenceError::sqlite("read pinned Capture binding", error))?;
            let input = InputIdentity::parse(utf8_message(input)?)
                .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
            let protection = ManagedInputProtection::from_rank(protection)
                .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
            pinned.insert(input, (payload_id(payload)?, protection));
        }
        let mut registry = Vec::new();
        for declaration in core.inputs() {
            let declared = match declaration.protection {
                InputProtectionV1::Normal => ManagedInputProtection::Normal,
                InputProtectionV1::Secret => ManagedInputProtection::Secret,
            };
            let (payload, protection) = match pinned.remove(&declaration.id) {
                Some((payload, stored)) if stored >= declared => (Some(payload), stored),
                None if !declaration.required || cleanup => (None, declared),
                _ => {
                    return Err(PersistenceError::CorruptRun(
                        "Capture active pin is missing or violates declared protection".to_owned(),
                    ));
                }
            };
            registry.push(PinnedInputBinding {
                input: declaration.id.clone(),
                role: ManagedInputRole::Active {
                    required: declaration.required,
                },
                payload,
                protection,
            });
        }
        registry.extend(pinned.into_iter().map(|(input, (payload, protection))| {
            PinnedInputBinding {
                input,
                role: ManagedInputRole::Retained,
                payload: Some(payload),
                protection,
            }
        }));
        registry.sort_by(|a, b| a.input.cmp(&b.input));
        Ok(registry)
    }

    pub(crate) fn open_admitted_runtime_blob(
        &self,
        run: RunId,
        file: &RuntimeFileV1,
    ) -> Result<VerifiedRuntimeBlob, PersistenceError> {
        let digest = {
            let database = self
                .database
                .lock()
                .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
            execution_row(&database, run)?.ok_or(PersistenceError::RunNotRunning)?;
            let bytes = database
                .query_row(
                    "SELECT refs.blob_digest \
                     FROM run_revision_pins AS pins \
                     JOIN revision_runtime_content_refs AS refs \
                       ON refs.package_id=pins.package_id \
                      AND refs.revision_content_digest=pins.revision_content_digest \
                     WHERE pins.run_id=?1 AND refs.content_id=?2",
                    params![run.as_bytes().as_slice(), file.id.as_str()],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()
                .map_err(|error| PersistenceError::sqlite("load admitted runtime pin", error))?
                .ok_or_else(|| {
                    PersistenceError::CorruptRun(format!(
                        "admitted Run has no runtime pin for {}",
                        file.id.as_str()
                    ))
                })?;
            let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
                PersistenceError::CorruptRun("runtime pin digest length is invalid".to_owned())
            })?;
            Sha256Digest::from_bytes(bytes)
        };
        if digest != file.blob_digest {
            return Err(PersistenceError::CorruptRun(format!(
                "admitted runtime pin changed for {}",
                file.id.as_str()
            )));
        }
        self.runtime_content
            .open_verified(&digest)
            .map_err(PersistenceError::from)
    }
}
