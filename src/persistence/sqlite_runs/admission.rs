//! Run acceptance, serialized admission and mutation conflict qualification.

use super::*;

impl PactrunPersistence {
    pub(crate) fn create_accepted_run(
        &self,
        run: RunId,
        instance: InstanceId,
        accepted_state_version: InstanceStateVersion,
        action: &ActionRunIdentity,
        owner: &ExecutionOwnerSession,
        arbiter: &impl AcceptanceArbiter,
    ) -> Result<RunId, AcceptanceError> {
        self.create_accepted_managed_run(
            run,
            instance,
            accepted_state_version,
            &ManagedRunIdentity::Action(action.clone()),
            owner,
            arbiter,
        )
    }

    pub(crate) fn create_accepted_managed_run(
        &self,
        run: RunId,
        instance: InstanceId,
        accepted_state_version: InstanceStateVersion,
        operation: &ManagedRunIdentity,
        owner: &ExecutionOwnerSession,
        arbiter: &impl AcceptanceArbiter,
    ) -> Result<RunId, AcceptanceError> {
        if matches!(
            operation,
            ManagedRunIdentity::Migration(_) | ManagedRunIdentity::Deletion { .. }
        ) && self.staging_session().is_none_or(|s| s.owner() != *owner)
        {
            return Err(AcceptanceError::NotCommitted {
                run,
                source: PersistenceError::InvalidRunTransition(
                    "Migration acceptance requires its own live session".to_owned(),
                ),
            });
        }
        let mut database = self
            .database
            .lock()
            .map_err(|_| AcceptanceError::NotCommitted {
                run,
                source: PersistenceError::DatabaseLockPoisoned,
            })?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| AcceptanceError::NotCommitted {
                run,
                source: PersistenceError::sqlite("begin Run acceptance", error),
            })?;
        self.check_write_admission(&transaction)
            .map_err(|source| AcceptanceError::NotCommitted { run, source })?;
        instance_header(&transaction, instance)
            .map_err(|source| AcceptanceError::NotCommitted { run, source })?;
        let accepted_at_unix_ms =
            unix_ms_now().map_err(|source| AcceptanceError::NotCommitted { run, source })?;
        transaction
            .execute(
                "INSERT INTO runs(run_id, instance_id, accepted_state_version, accepted_at_unix_ms) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    run.as_bytes().as_slice(),
                    instance.as_bytes().as_slice(),
                    accepted_state_version.as_bytes().as_slice(),
                    accepted_at_unix_ms,
                ],
            )
            .map_err(|error| AcceptanceError::NotCommitted {
                run,
                source: PersistenceError::sqlite("insert Run", error),
            })?;
        transaction
            .execute(
                "INSERT INTO run_operation_kinds(run_id, operation_kind) VALUES (?1, ?2)",
                params![run.as_bytes().as_slice(), operation.kind().rank()],
            )
            .map_err(|error| AcceptanceError::NotCommitted {
                run,
                source: PersistenceError::sqlite("insert Run operation kind", error),
            })?;
        transaction
            .execute(
                "INSERT INTO run_diagnostic_collections(run_id,retain_text) VALUES (?1,?2)",
                params![run.as_bytes().as_slice(), arbiter.retain_hook_text()],
            )
            .map_err(|error| AcceptanceError::NotCommitted {
                run,
                source: PersistenceError::sqlite("initialize diagnostic policy", error),
            })?;
        transaction.execute("INSERT INTO run_core_diagnostic_collections(run_id,observed,closed,failed) VALUES (?1,0,0,0)", [run.as_bytes().as_slice()])
            .map_err(|error| AcceptanceError::NotCommitted { run, source: PersistenceError::sqlite("initialize Core diagnostic collection", error) })?;
        insert_managed_invocation(&transaction, run, operation)
            .map_err(|source| AcceptanceError::NotCommitted { run, source })?;
        transaction
            .execute(
                "INSERT INTO run_executions(run_id, owner_session, risk_state) VALUES (?1, ?2, ?3)",
                params![
                    run.as_bytes().as_slice(),
                    owner.as_str().as_bytes(),
                    RecoveryRiskState::Clear.rank(),
                ],
            )
            .map_err(|error| AcceptanceError::NotCommitted {
                run,
                source: PersistenceError::sqlite("insert Run execution owner", error),
            })?;
        fault(FaultPoint::BeforeRunAcceptCommit);
        let commit = || {
            transaction
                .commit()
                .map_err(|error| PersistenceError::sqlite("commit Run acceptance", error))
        };
        match arbiter.before_durable_acceptance(commit) {
            AcceptanceCommitResult::Cancelled => {
                return Err(AcceptanceError::Cancelled { run });
            }
            AcceptanceCommitResult::Committed(Ok(())) => {}
            AcceptanceCommitResult::Committed(Err(source))
            | AcceptanceCommitResult::Uncertain(source) => {
                return Err(AcceptanceError::Uncertain { run, source });
            }
        }
        fault(FaultPoint::AfterRunAcceptCommit);
        arbiter.accepted(run);
        Ok(run)
    }

    /// The authoritative Admission transaction of PR-REQ-0279. Every check reads
    /// persisted state (plus the launcher closure) inside one `BEGIN IMMEDIATE`;
    /// a refusal publishes the terminal `Failed` outcome and success inserts
    /// the pins in that same transaction. Checks outside it are advisory only.
    pub(crate) fn admit_run(
        &self,
        run: RunId,
        facts: &AdmissionFacts<'_>,
        launcher_check: &LauncherCheck<'_>,
        override_guard: bool,
    ) -> Result<Result<(), AdmissionRefusal>, PersistenceError> {
        self.admit_run_with_owner(
            run,
            None,
            AdmissionQualification { facts, plan: None },
            launcher_check,
            override_guard,
        )
    }

    pub(crate) fn admit_managed_run(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
        facts: &AdmissionFacts<'_>,
        launcher_check: &LauncherCheck<'_>,
        override_guard: bool,
    ) -> Result<Result<(), AdmissionRefusal>, PersistenceError> {
        self.admit_run_with_owner(
            run,
            Some(owner),
            AdmissionQualification { facts, plan: None },
            launcher_check,
            override_guard,
        )
    }

    pub(crate) fn admit_snapshot_plan(
        &self,
        run: RunId,
        owner: &ExecutionOwnerSession,
        plan: &crate::domain::SnapshotExecutionPlan,
        launcher_check: &LauncherCheck<'_>,
        override_guard: bool,
    ) -> Result<Result<(), AdmissionRefusal>, PersistenceError> {
        self.admit_run_with_owner(
            run,
            Some(owner),
            AdmissionQualification {
                facts: &plan.admission_facts(),
                plan: Some(plan),
            },
            launcher_check,
            override_guard,
        )
    }

    fn admit_run_with_owner(
        &self,
        run: RunId,
        expected_owner: Option<&ExecutionOwnerSession>,
        qualification: AdmissionQualification<'_>,
        launcher_check: &LauncherCheck<'_>,
        override_guard: bool,
    ) -> Result<Result<(), AdmissionRefusal>, PersistenceError> {
        let mut database = self
            .database
            .lock()
            .map_err(|_| PersistenceError::DatabaseLockPoisoned)?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::sqlite("begin Run admission", error))?;
        self.check_write_admission(&transaction)?;
        let header = run_header(&transaction, run)?;
        let (live_owner, risk_state) =
            execution_row(&transaction, run)?.ok_or(PersistenceError::RunNotRunning)?;
        let operation = managed_invocation_row(&transaction, run)?;
        load_managed_run_from(&transaction, run)?;
        if expected_owner.is_some_and(|owner| *owner != live_owner)
            || (expected_owner.is_none() && !matches!(operation, ManagedRunIdentity::Action(_)))
        {
            return Err(PersistenceError::InvalidRunTransition(
                "Admission owner does not match the persisted owner".to_owned(),
            ));
        }
        let facts = qualification.facts;
        if matches!(operation, ManagedRunIdentity::Restore { .. }) && qualification.plan.is_none() {
            return Err(PersistenceError::InvalidRunTransition(
                "Restore requires staged Snapshot admission; Action/Capture admission cannot qualify it".to_owned(),
            ));
        }
        if matches!(
            operation,
            ManagedRunIdentity::Migration(_) | ManagedRunIdentity::Deletion { .. }
        ) {
            return Err(PersistenceError::InvalidRunTransition(
                "Migration and deletion require operation-specific admission".to_owned(),
            ));
        }
        let boundary = if has_revision_pin(&transaction, run)? {
            ActionRunBoundary::Admitted
        } else {
            ActionRunBoundary::Accepted
        };
        validate_running_action_state(boundary, risk_state)
            .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
        if boundary == ActionRunBoundary::Admitted {
            return Err(PersistenceError::InvalidRunTransition(
                "Run is already admitted".to_owned(),
            ));
        }
        let decision = admission_decision(
            &transaction,
            &self.runtime_content,
            run,
            &header,
            qualification,
            launcher_check,
            override_guard,
        )?;
        match &decision {
            Ok(active_revision) => {
                transaction
                    .execute(
                        "INSERT INTO run_revision_pins(run_id, package_id, revision_content_digest) \
                         VALUES (?1, ?2, ?3)",
                        params![
                            run.as_bytes().as_slice(),
                            active_revision.package_id.as_bytes().as_slice(),
                            active_revision.content_digest.as_bytes().as_slice(),
                        ],
                    )
                    .map_err(|error| PersistenceError::sqlite("insert Run Revision pin", error))?;
                if let Some((_, bindings)) = facts.service {
                    super::super::sqlite_service_admission::pin_service_bindings(
                        &transaction,
                        run,
                        bindings,
                    )?;
                }
                if matches!(operation, ManagedRunIdentity::Capture { .. }) {
                    transaction.execute(
                        "INSERT INTO run_payload_pins(run_id,instance_id,input_identity,payload_id) \
                         SELECT ?1,instance_id,input_identity,payload_id FROM managed_input_bindings WHERE instance_id=?2",
                        params![run.as_bytes().as_slice(),header.instance.as_bytes().as_slice()],
                    ).map_err(|error| PersistenceError::sqlite("pin complete Capture registry", error))?;
                } else if let ManagedRunIdentity::Restore { snapshot, .. } = &operation {
                    let consequence = super::super::writer_admission::load_consequence_version(
                        &transaction,
                        header.instance,
                    )?;
                    transaction.execute("INSERT INTO run_restore_admissions(run_id,snapshot_id,admitted_state_version,admitted_consequence_version) VALUES (?1,?2,?3,?4)",
                        params![run.as_bytes().as_slice(),snapshot.as_bytes().as_slice(),header.accepted_state_version.as_bytes().as_slice(),consequence])
                        .map_err(|error|PersistenceError::sqlite("establish Restore Snapshot pin and tokens",error))?;
                } else {
                    for binding in facts.active_bindings {
                        transaction
                            .execute(
                                "INSERT INTO run_payload_pins(\
                                run_id, instance_id, input_identity, payload_id\
                             ) VALUES (?1, ?2, ?3, ?4)",
                                params![
                                    run.as_bytes().as_slice(),
                                    header.instance.as_bytes().as_slice(),
                                    binding.input.as_str().as_bytes(),
                                    binding.payload.as_bytes().as_slice(),
                                ],
                            )
                            .map_err(|error| {
                                PersistenceError::sqlite("insert Run payload pin", error)
                            })?;
                    }
                }
            }
            Err(refusal) => {
                let finish = RunFinish {
                    outcome: RunOutcome::Failed,
                    primary_failure: Some(refusal.primary_failure()),
                    secondary_failures: Vec::new(),
                    hook_completion: None,
                };
                finish_run_in_transaction(&transaction, run, &header, &finish, &mut [])?;
            }
        }
        fault(FaultPoint::BeforeRunAdmitCommit);
        transaction
            .commit()
            .map_err(|error| PersistenceError::sqlite("commit Run admission", error))?;
        fault(FaultPoint::AfterRunAdmitCommit);
        Ok(decision.map(|_| ()))
    }
}

/// Evaluates the PR-REQ-0279 refusal precedence against persisted state. On
/// success returns the active Revision to pin.
#[derive(Clone, Copy)]
struct AdmissionQualification<'a> {
    facts: &'a AdmissionFacts<'a>,
    plan: Option<&'a crate::domain::SnapshotExecutionPlan>,
}

fn admission_decision(
    transaction: &Transaction<'_>,
    runtime_content: &RuntimeContentStore,
    run: RunId,
    header: &RunHeader,
    qualification: AdmissionQualification<'_>,
    launcher_check: &LauncherCheck<'_>,
    override_guard: bool,
) -> Result<Result<RevisionIdentity, AdmissionRefusal>, PersistenceError> {
    let facts = qualification.facts;
    // 1. Trust guard.
    if !override_guard && guard_row(transaction, header.instance)?.is_some() {
        return Ok(Err(AdmissionRefusal::RecoveryGuardActive));
    }

    if super::super::sqlite_deletions::obligation_from(transaction, header.instance)?.is_some() {
        return Ok(Err(AdmissionRefusal::DeletionObligation(header.instance)));
    }

    // 2. Stale compilation or state facts.
    let (_, active_revision, current) = match instance_header(transaction, header.instance) {
        Ok(header) => header,
        Err(PersistenceError::MissingInstance(_)) => {
            return Ok(Err(AdmissionRefusal::PlanInvalidated(
                "Instance has been retired".to_owned(),
            )));
        }
        Err(error) => return Err(error),
    };
    if current != facts.expected_state_version || current != header.accepted_state_version {
        return Ok(Err(AdmissionRefusal::PlanInvalidated(
            "the Instance state version changed since compilation".to_owned(),
        )));
    }
    let invocation = managed_invocation_row(transaction, run)?;
    if invocation.revision() != &active_revision {
        return Ok(Err(AdmissionRefusal::PlanInvalidated(
            "the active Revision changed since compilation".to_owned(),
        )));
    }
    let contract =
        super::super::sqlite_instances::load_revision_contract(transaction, &active_revision)?;
    let core = contract.common();
    if let Some(reason) = super::super::sqlite_service_admission::qualify_current_service_facts(
        transaction,
        header.instance,
        &active_revision,
        &invocation,
        facts,
    )? {
        return Ok(Err(AdmissionRefusal::PlanInvalidated(reason)));
    }
    if let Some(plan) = qualification.plan {
        if plan.operation() != &invocation || plan.instance() != header.instance {
            return Ok(Err(AdmissionRefusal::PlanInvalidated(
                "Snapshot Plan does not match its accepted Run".to_owned(),
            )));
        }
        if let Some(reason) = validate_snapshot_plan_declaration(transaction, &contract, plan)? {
            return Ok(Err(AdmissionRefusal::PlanInvalidated(reason)));
        }
    }
    if let ManagedRunIdentity::Restore { snapshot, .. } = &invocation {
        let target =
            super::super::sqlite_instances::load_instance_view_from(transaction, header.instance)?
                .ok_or_else(|| PersistenceError::MissingInstance(header.instance.to_string()))?;
        match super::super::sqlite_snapshots::qualify_restore_snapshot(
            transaction,
            runtime_content,
            &target,
            *snapshot,
        ) {
            Ok(_) => {}
            Err(PersistenceError::MissingSnapshot(_)) => {
                return Ok(Err(AdmissionRefusal::PlanInvalidated(
                    "selected Snapshot is unavailable".to_owned(),
                )));
            }
            Err(PersistenceError::InvalidRunTransition(message)) => {
                return Ok(Err(AdmissionRefusal::PlanInvalidated(message)));
            }
            Err(PersistenceError::SnapshotCodec(
                crate::snapshot_integrity::SnapshotCodecError::Capability(_),
            )) => {
                return Ok(Err(AdmissionRefusal::PlanInvalidated(
                    "selected Snapshot exceeds this build's Restore capability".to_owned(),
                )));
            }
            Err(PersistenceError::CorruptSnapshot(_)) => {
                return Ok(Err(AdmissionRefusal::PlanInvalidated(
                    "selected Snapshot fails integrity verification".to_owned(),
                )));
            }
            Err(PersistenceError::SnapshotCodec(
                crate::snapshot_integrity::SnapshotCodecError::Validation(_),
            )) => {
                return Ok(Err(AdmissionRefusal::PlanInvalidated(
                    "selected Snapshot fails required producer checks".to_owned(),
                )));
            }
            Err(error) => return Err(error),
        }
    }
    for binding in facts.active_bindings {
        let current_payload = binding_payload(transaction, header.instance, &binding.input)?
            .map(|(payload, _)| payload);
        if current_payload != Some(binding.payload) {
            return Ok(Err(AdmissionRefusal::PlanInvalidated(format!(
                "the current binding of Input {} changed since compilation",
                binding.input.as_str()
            ))));
        }
    }
    let mut unbound = Vec::new();
    let current_site = match &invocation {
        ManagedRunIdentity::Action(action) => Some(crate::domain::ServiceHookSite::Action(
            action.action.clone(),
        )),
        ManagedRunIdentity::Capture { .. } => Some(crate::domain::ServiceHookSite::Capture),
        _ => None,
    };
    let current_grants = if let (Some(operation), Some(site)) = (contract.operation(), current_site)
    {
        match operation.input_use(&site) {
            Some(crate::domain::OperationInputUse::Active(inputs)) => Some(inputs),
            _ => {
                return Ok(Err(AdmissionRefusal::PlanInvalidated(
                    "operation Input declaration is unavailable".to_owned(),
                )));
            }
        }
    } else {
        None
    };
    if let Some(grants) = &current_grants {
        for input in &grants.required {
            if binding_payload(transaction, header.instance, input)?.is_none() {
                unbound.push(input.clone());
            }
        }
    } else {
        for declaration in core.inputs() {
            if !matches!(invocation, ManagedRunIdentity::Restore { .. })
                && declaration.required
                && binding_payload(transaction, header.instance, &declaration.id)?.is_none()
            {
                unbound.push(declaration.id.clone());
            }
        }
    }
    if !unbound.is_empty() {
        return Ok(Err(AdmissionRefusal::MissingRequiredInputs(unbound)));
    }
    if matches!(invocation, ManagedRunIdentity::Action(_))
        && let Some(grants) = current_grants
    {
        let mut expected = Vec::new();
        for input in grants.all() {
            if let Some((payload, protection)) =
                binding_payload(transaction, header.instance, &input)?
            {
                expected.push(crate::domain::ActiveInstanceBindingReference {
                    input,
                    payload,
                    protection,
                });
            }
        }
        if expected != facts.active_bindings {
            return Ok(Err(AdmissionRefusal::PlanInvalidated(
                "Action binding facts do not match its exact Input read grants".to_owned(),
            )));
        }
    }
    if matches!(invocation, ManagedRunIdentity::Capture { .. })
        && let Some(reason) =
            validate_capture_facts(transaction, header.instance, &active_revision, core, facts)?
    {
        return Ok(Err(AdmissionRefusal::PlanInvalidated(reason)));
    }
    let digests = facts
        .runtime_content
        .iter()
        .map(|file| file.blob_digest.clone())
        .collect::<BTreeSet<_>>();
    for digest in digests {
        match runtime_content.open_verified(&digest) {
            Ok(_) => {}
            Err(RuntimeContentStoreError::MissingBlob(_))
            | Err(RuntimeContentStoreError::CorruptBlob { .. }) => {
                return Ok(Err(AdmissionRefusal::PlanInvalidated(format!(
                    "runtime content {} is unavailable",
                    digest.as_str()
                ))));
            }
            Err(error) => return Err(PersistenceError::RuntimeContent(error)),
        }
    }
    if let Some(launcher) = facts.launch.launcher()
        && let Err(reason) = launcher_check(launcher)
    {
        return Ok(Err(AdmissionRefusal::PlanInvalidated(format!(
            "the interpreter launcher selection changed since compilation: {reason}"
        ))));
    }

    // 3. Mutate exclusivity, with both access modes read from persisted
    //    Revision declarations.
    if let Some(competitor) = managed_mutation_conflict(transaction, header.instance, run)? {
        return Ok(Err(AdmissionRefusal::MutationConflict(competitor)));
    }
    Ok(Ok(active_revision))
}

fn validate_snapshot_plan_declaration(
    database: &Connection,
    contract: &crate::domain::RevisionCore,
    plan: &crate::domain::SnapshotExecutionPlan,
) -> Result<Option<String>, PersistenceError> {
    use crate::domain::{
        HookLaunchV1, InvocationParameterValue as V, ParameterTypeV1 as T, SnapshotOperation,
    };
    let core = contract.common();
    let operation = match plan.operation() {
        ManagedRunIdentity::Capture { .. } => SnapshotOperation::Capture,
        ManagedRunIdentity::Restore { snapshot, .. } => SnapshotOperation::Restore(*snapshot),
        ManagedRunIdentity::Action(_)
        | ManagedRunIdentity::Migration(_)
        | ManagedRunIdentity::Deletion { .. } => {
            return Ok(Some("Snapshot Plan contains another operation".to_owned()));
        }
    };
    let (parameters, hook) = crate::domain::snapshot_hook(core, operation)
        .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
    let site = match operation {
        SnapshotOperation::Capture => crate::domain::ServiceHookSite::Capture,
        SnapshotOperation::Restore(_) => crate::domain::ServiceHookSite::Restore,
    };
    let grants = crate::domain::snapshot_input_grants(contract, &site)
        .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
    let outputs: Vec<_> = contract
        .ordinary_outputs(&site)
        .unwrap_or_default()
        .iter()
        .map(|output| output.id.clone())
        .collect();
    if grants.as_ref() != plan.input_grants() || outputs != plan.outputs() {
        return Ok(Some(
            "Snapshot Input or output authority differs from its declaration".to_owned(),
        ));
    }
    let access = plan
        .operation()
        .authoritative_access(core)
        .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
    if hook != plan.hook()
        || !crate::domain::VersionDomain::Hook.supports(hook.protocol_version)
        || access != plan.access()
        || parameters.len() != plan.parameters().len()
    {
        return Ok(Some(
            "Snapshot Plan declaration changed or is inconsistent".to_owned(),
        ));
    }
    for (declaration, binding) in parameters.iter().zip(plan.parameters()) {
        if declaration.id != binding.id
            || (declaration.sensitive && !binding.effective_redaction)
            || !matches!(
                (declaration.parameter_type, binding.value()),
                (T::Integer, V::Integer(_))
                    | (T::Float, V::Float(_))
                    | (T::Boolean, V::Boolean(_))
                    | (T::String, V::String(_))
            )
        {
            return Ok(Some(
                "Snapshot parameters do not match the exact declaration".to_owned(),
            ));
        }
    }
    let stored = super::super::sqlite_revision_store::load_revision_from(
        database,
        plan.operation().revision(),
    )?
    .ok_or_else(|| PersistenceError::MissingRevision(plan.operation().revision().clone()))?;
    let files = stored.content.runtime_content.files();
    if files != plan.runtime_content() {
        return Ok(Some(
            "Snapshot runtime closure changed or is incomplete".to_owned(),
        ));
    }
    let matches = plan.launch().matches_shell(&hook.launch, files)
        || match (&hook.launch, plan.launch()) {
            (
                HookLaunchV1::Direct { executable: id },
                CompiledHookLaunch::Direct { executable },
            ) => files
                .iter()
                .any(|file| &file.id == id && file == executable),
            (
                HookLaunchV1::Interpreter {
                    command,
                    interpreter_args,
                    script: id,
                },
                CompiledHookLaunch::Interpreter {
                    launcher,
                    interpreter_args: args,
                    script,
                },
            ) => {
                command == &launcher.command
                    && interpreter_args == args
                    && files.iter().any(|file| &file.id == id && file == script)
            }
            _ => false,
        };
    Ok((!matches).then(|| "Snapshot launch does not match its declared runtime".to_owned()))
}

fn validate_capture_facts(
    database: &Connection,
    instance: InstanceId,
    revision: &RevisionIdentity,
    core: &crate::domain::RevisionDeclarations,
    facts: &AdmissionFacts<'_>,
) -> Result<Option<String>, PersistenceError> {
    use crate::domain::{
        ActiveInstanceBindingReference, HookLaunchV1, InputProtectionV1, ManagedInputProtection,
    };
    let capture = core
        .snapshot()
        .and_then(|snapshot| snapshot.capture.as_ref())
        .ok_or_else(|| {
            PersistenceError::CorruptRun(
                "Run Capture is not declared by its exact Revision".to_owned(),
            )
        })?;
    let stored = super::super::sqlite_revision_store::load_revision_from(database, revision)?
        .ok_or_else(|| {
            PersistenceError::CorruptRun("Capture Revision is unavailable".to_owned())
        })?;
    let files = stored.content.runtime_content.files();
    if facts.runtime_content != files {
        return Ok(Some(
            "Capture runtime closure changed or is incomplete".to_owned(),
        ));
    }
    let launch_matches = facts.launch.matches_shell(&capture.hook.launch, files)
        || match (&capture.hook.launch, facts.launch) {
            (
                HookLaunchV1::Direct { executable: id },
                CompiledHookLaunch::Direct { executable },
            ) => files
                .iter()
                .any(|file| &file.id == id && file == executable),
            (
                HookLaunchV1::Interpreter {
                    command,
                    interpreter_args,
                    script: id,
                },
                CompiledHookLaunch::Interpreter {
                    launcher,
                    interpreter_args: actual_args,
                    script,
                },
            ) => {
                command == &launcher.command
                    && interpreter_args == actual_args
                    && files.iter().any(|file| &file.id == id && file == script)
            }
            _ => false,
        };
    if !launch_matches
        || !crate::domain::VersionDomain::Hook.supports(capture.hook.protocol_version)
    {
        return Ok(Some(
            "Capture Hook launch or protocol is not supported by the exact compiled facts"
                .to_owned(),
        ));
    }
    let mut active = Vec::new();
    for declaration in core.inputs() {
        if let Some((payload, protection)) = binding_payload(database, instance, &declaration.id)? {
            if declaration.protection == InputProtectionV1::Secret
                && protection == ManagedInputProtection::Normal
            {
                return Err(PersistenceError::CorruptManagedInput(
                    "active Secret references a Normal payload".to_owned(),
                ));
            }
            active.push(ActiveInstanceBindingReference {
                input: declaration.id.clone(),
                payload,
                protection,
            });
        }
    }
    if active != facts.active_bindings {
        return Ok(Some(
            "Capture active binding facts changed or are incomplete".to_owned(),
        ));
    }
    Ok(None)
}

/// Shared predicate for all managed operation kinds. The caller has already
/// revalidated the incoming exact Revision inside this same write transaction.
/// Accepted-only Runs never enter the competitor set; access is never supplied
/// by the caller or inferred from the operation discriminator alone.
pub(in crate::persistence) fn managed_mutation_conflict(
    transaction: &Transaction<'_>,
    instance: InstanceId,
    run: RunId,
) -> Result<Option<RunId>, PersistenceError> {
    let invocation = managed_invocation_row(transaction, run)?;
    let core = load_revision_core(transaction, invocation.revision())?;
    let access = invocation
        .authoritative_access(&core)
        .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?;
    if access == OperationAccessV1::Observe {
        return Ok(None);
    }
    let mut cores = BTreeMap::new();
    for competitor in admitted_competitors(transaction, instance, run)? {
        let revision = competitor.operation.revision();
        if !cores.contains_key(revision) {
            cores.insert(revision.clone(), load_revision_core(transaction, revision)?);
        }
        if competitor
            .operation
            .authoritative_access(&cores[revision])
            .map_err(|error| PersistenceError::CorruptRun(error.to_string()))?
            == OperationAccessV1::Mutate
        {
            return Ok(Some(competitor.run));
        }
    }
    Ok(None)
}

struct AdmittedCompetitor {
    run: RunId,
    operation: ManagedRunIdentity,
}

/// Every other Run on the Instance that is both Running and Admitted, in
/// `run_id` order.
fn admitted_competitors(
    transaction: &Transaction<'_>,
    instance: InstanceId,
    run: RunId,
) -> Result<Vec<AdmittedCompetitor>, PersistenceError> {
    let mut statement = transaction
        .prepare(
            "SELECT r.run_id, p.package_id, p.revision_content_digest \
             FROM runs r \
             JOIN run_executions e ON e.run_id = r.run_id \
             JOIN run_revision_pins p ON p.run_id = r.run_id \
             WHERE r.instance_id = ?1 AND r.run_id <> ?2 \
             ORDER BY r.run_id",
        )
        .map_err(|error| PersistenceError::sqlite("prepare admitted Run enumeration", error))?;
    let rows = statement
        .query_map(
            params![instance.as_bytes().as_slice(), run.as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .map_err(|error| PersistenceError::sqlite("enumerate admitted Runs", error))?;
    let mut competitors = Vec::new();
    for row in rows {
        let (id, package, digest) =
            row.map_err(|error| PersistenceError::sqlite("read admitted Run", error))?;
        let id = run_id(id)?;
        let operation = load_managed_run_from(transaction, id)?.operation;
        if operation.revision() != &revision_identity(package, digest)?
            || outcome_row(transaction, id)?.is_some()
        {
            return Err(PersistenceError::CorruptRun(
                "admitted competitor has inconsistent Revision pin or terminal state".to_owned(),
            ));
        }
        competitors.push(AdmittedCompetitor { run: id, operation });
    }
    Ok(competitors)
}
