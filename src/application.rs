//! Application orchestration and dependency-resolution ownership.

mod deletions;
mod installation;
mod migrations;
mod service_storage;
mod snapshots;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex},
};

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static FAIL_FINALIZATION_ADVANCES_FOR_TEST: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn fail_next_finalization_advances_for_test(count: usize) {
    FAIL_FINALIZATION_ADVANCES_FOR_TEST.with(|remaining| remaining.set(count));
}

pub(crate) use installation::{InstallPackResult, MigrationRelationState};

use crate::{
    authoring::{AuthoringError, SourceAcquisitionError},
    domain::{
        ActionCompilationObservation, ActionExecutionPlan, ActionIdentity, ActionResolutionError,
        ActionV1, ExecutionOwnerSession, InputIdentity, InstanceId, InstanceName,
        InstanceStateVersion, InstanceSummary, InstanceView, InvokeAction, LocalAlias,
        ManagedOutputIdentity, PlanCompilationError, RawParameterInput, ReferenceLabel,
        RevisionCoreV1Error, RevisionIdentity, RevisionMetadataMutationBatch, RunFinish, RunId,
        RunInspectionData, RunOutcome, RunSummary, RunView, bind_action_parameters,
    },
    executor::{AdmissionOptions, AdmittedExecution, ExecutorError},
    hook::{ActionCancellation, ContinuationGuard, HookRuntimePolicy, OwnerContinuationRegistry},
    managed_data::{
        SessionOwnerProbe, StagedFile, StagingError, StagingSession, cleanup_lost_session,
        probe_session_owner,
    },
    persistence::{
        ManagedInputWrite, PactrunPersistence, PersistenceError, validate_supported_storage_root,
    },
};

pub(crate) struct InputAcquisition<'a> {
    pub(crate) input_id: InputIdentity,
    pub(crate) source: Box<dyn Read + 'a>,
}

#[derive(Debug)]
pub(crate) struct ExportObservation {
    pub(crate) state_version: InstanceStateVersion,
    pub(crate) bytes: StagedFile,
}

#[derive(Debug)]
pub(crate) enum ApplicationError {
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
    Configuration(String),
    Authoring(AuthoringError),
    SourceAcquisition(SourceAcquisitionError),
    Staging(StagingError),
    Revision(RevisionCoreV1Error),
    RevisionContent(crate::revision_content::RevisionContentError),
    Persistence(PersistenceError),
    ActionResolution(ActionResolutionError),
    PlanCompilation(PlanCompilationError),
    SnapshotCompilation(crate::domain::SnapshotPlanError),
    MigrationCompilation(crate::domain::MigrationError),
    DeletionCompilation(crate::domain::DeletionPlanError),
    Execution(ExecutorError),
    InvalidInstallation(String),
    InvalidRequest(String),
    ServiceStorage(crate::domain::ServiceAccessError),
    LockPoisoned,
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::Configuration(message)
            | Self::InvalidInstallation(message)
            | Self::InvalidRequest(message) => formatter.write_str(message),
            Self::Authoring(source) => write!(formatter, "Pack source: {source}"),
            Self::SourceAcquisition(source) => write!(formatter, "Pack source: {source}"),
            Self::Staging(source) => write!(formatter, "staging: {source}"),
            Self::Revision(source) => write!(formatter, "Revision candidate: {source}"),
            Self::RevisionContent(source) => write!(formatter, "Revision candidate: {source}"),
            Self::Persistence(source) => write!(formatter, "persistence: {source}"),
            Self::ActionResolution(source) => write!(formatter, "resolution: {source}"),
            Self::PlanCompilation(source) => write!(formatter, "compilation: {source}"),
            Self::SnapshotCompilation(source) => {
                write!(formatter, "Snapshot compilation: {source}")
            }
            Self::MigrationCompilation(source) => {
                write!(formatter, "Migration compilation: {source}")
            }
            Self::Execution(source) => write!(formatter, "execution: {source}"),
            Self::DeletionCompilation(source) => {
                write!(formatter, "deletion compilation: {source}")
            }
            Self::ServiceStorage(source) => source.fmt(formatter),
            Self::LockPoisoned => formatter.write_str("Instance mutation lock is poisoned"),
        }
    }
}

impl std::error::Error for ApplicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Authoring(source) => Some(source),
            Self::SourceAcquisition(source) => Some(source),
            Self::Staging(source) => Some(source),
            Self::Revision(source) => Some(source),
            Self::RevisionContent(source) => Some(source),
            Self::Persistence(source) => Some(source),
            Self::ActionResolution(source) => Some(source),
            Self::PlanCompilation(source) => Some(source),
            Self::SnapshotCompilation(source) => Some(source),
            Self::MigrationCompilation(source) => Some(source),
            Self::DeletionCompilation(source) => Some(source),
            Self::Execution(source) => Some(source),
            Self::ServiceStorage(source) => Some(source),
            Self::Configuration(_)
            | Self::InvalidInstallation(_)
            | Self::InvalidRequest(_)
            | Self::LockPoisoned => None,
        }
    }
}

impl From<AuthoringError> for ApplicationError {
    fn from(source: AuthoringError) -> Self {
        Self::Authoring(source)
    }
}

impl From<SourceAcquisitionError> for ApplicationError {
    fn from(source: SourceAcquisitionError) -> Self {
        Self::SourceAcquisition(source)
    }
}

impl From<StagingError> for ApplicationError {
    fn from(source: StagingError) -> Self {
        Self::Staging(source)
    }
}

impl From<RevisionCoreV1Error> for ApplicationError {
    fn from(source: RevisionCoreV1Error) -> Self {
        Self::Revision(source)
    }
}

impl From<PersistenceError> for ApplicationError {
    fn from(source: PersistenceError) -> Self {
        Self::Persistence(source)
    }
}

impl From<ActionResolutionError> for ApplicationError {
    fn from(source: ActionResolutionError) -> Self {
        Self::ActionResolution(source)
    }
}

impl From<PlanCompilationError> for ApplicationError {
    fn from(source: PlanCompilationError) -> Self {
        Self::PlanCompilation(source)
    }
}

impl From<crate::domain::SnapshotPlanError> for ApplicationError {
    fn from(error: crate::domain::SnapshotPlanError) -> Self {
        Self::SnapshotCompilation(error)
    }
}

impl From<crate::domain::MigrationError> for ApplicationError {
    fn from(error: crate::domain::MigrationError) -> Self {
        Self::MigrationCompilation(error)
    }
}

impl From<ExecutorError> for ApplicationError {
    fn from(source: ExecutorError) -> Self {
        Self::Execution(source)
    }
}
pub(crate) struct PactrunApplication {
    persistence: PactrunPersistence,
    storage_root: std::path::PathBuf,
    continuations: OwnerContinuationRegistry,
    mutation_locks: Mutex<BTreeMap<InstanceId, Arc<Mutex<()>>>>,
}

impl PactrunApplication {
    #[cfg(test)]
    pub(crate) fn execute_capture_with_risk_failures(
        &self,
        run: RunId,
        policy: HookRuntimePolicy,
        failures: &std::sync::atomic::AtomicUsize,
    ) -> Result<RunId, ApplicationError> {
        let claim = self.claim_snapshot_execution(run)?.ok_or_else(|| {
            ApplicationError::InvalidRequest("Capture was already claimed".to_owned())
        })?;
        Ok(crate::hook::execute_capture_with_risk_failures(
            &self.persistence,
            self.staging()?,
            claim,
            policy,
            failures,
        ))
    }
    #[allow(dead_code)]
    pub(crate) fn execute_admitted_capture(
        &self,
        run: RunId,
        policy: HookRuntimePolicy,
    ) -> Result<RunId, ApplicationError> {
        self.execute_admitted_snapshot(
            run,
            policy,
            crate::domain::ManagedExecutionKind::SnapshotCapture,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn execute_admitted_restore(
        &self,
        run: RunId,
        policy: HookRuntimePolicy,
    ) -> Result<RunId, ApplicationError> {
        self.execute_admitted_snapshot(
            run,
            policy,
            crate::domain::ManagedExecutionKind::SnapshotRestore,
        )
    }

    fn execute_admitted_snapshot(
        &self,
        run: RunId,
        policy: HookRuntimePolicy,
        kind: crate::domain::ManagedExecutionKind,
    ) -> Result<RunId, ApplicationError> {
        let view = self.persistence.load_managed_run(run)?.ok_or_else(|| {
            ApplicationError::InvalidRequest("Snapshot Run is unavailable".to_owned())
        })?;
        if view.operation.kind() != kind {
            return Err(ApplicationError::InvalidRequest(
                "Run has a different Snapshot operation kind".to_owned(),
            ));
        }
        if !self.execute_snapshot_if_ready(run, policy)? {
            return Err(ApplicationError::InvalidRequest(
                "Snapshot execution is not ready or was already claimed".to_owned(),
            ));
        }
        Ok(run)
    }

    pub(crate) fn execute_snapshot_if_ready(
        &self,
        run: RunId,
        policy: HookRuntimePolicy,
    ) -> Result<bool, ApplicationError> {
        for duration in [
            policy.startup_timeout,
            policy.action_timeout,
            policy.termination_grace,
        ]
        .into_iter()
        .flatten()
        {
            if duration.as_millis() > i64::MAX as u128
                || std::time::Instant::now().checked_add(duration).is_none()
            {
                return Err(ApplicationError::InvalidRequest(
                    "Snapshot deadline is not representable".to_owned(),
                ));
            }
        }
        let Some(claim) = self.claim_snapshot_execution(run)? else {
            return Ok(false);
        };
        crate::hook::execute_snapshot(&self.persistence, self.staging()?, claim, policy);
        Ok(true)
    }
    // Private S4 executor seams; human Snapshot dispatch is wired in S7.
    #[allow(dead_code)]
    pub(crate) fn accept_snapshot_plan(
        &self,
        plan: crate::domain::SnapshotExecutionPlan,
        options: AdmissionOptions,
        cancellation: ActionCancellation,
    ) -> Result<RunId, ApplicationError> {
        let lock = self.mutation_lock(plan.instance())?;
        let _guard = lock.lock().map_err(|_| ApplicationError::LockPoisoned)?;
        Ok(crate::hook::accept_snapshot(
            &self.persistence,
            self.staging()?,
            &self.continuations,
            plan,
            options,
            cancellation,
        )?)
    }
    #[allow(dead_code)]
    pub(crate) fn claim_snapshot_execution(
        &self,
        run: RunId,
    ) -> Result<Option<crate::hook::SnapshotExecutionClaim<'_>>, ApplicationError> {
        Ok(crate::hook::claim_snapshot(
            &self.persistence,
            self.staging()?,
            &self.continuations,
            run,
        )?)
    }
    #[allow(dead_code)]
    pub(crate) fn stop_snapshot_before_launch(
        &self,
        run: RunId,
        finish: RunFinish,
    ) -> Result<bool, ApplicationError> {
        Ok(crate::hook::stop_snapshot_before_launch(
            &self.continuations,
            run,
            finish,
        )?)
    }
    pub(crate) fn observe_snapshot_compilation(
        &self,
        intent: &crate::domain::SnapshotIntent,
    ) -> Result<crate::domain::SnapshotCompilationObservation, PersistenceError> {
        self.persistence.observe_snapshot_compilation(intent)
    }
    pub(crate) fn open(storage_root: impl AsRef<Path>) -> Result<Self, ApplicationError> {
        Self::open_for(storage_root.as_ref(), false)
    }

    pub(crate) fn open_for_collection(storage_root: &Path) -> Result<Self, ApplicationError> {
        Self::open_for(storage_root, true)
    }

    fn open_for(requested: &Path, collection: bool) -> Result<Self, ApplicationError> {
        if requested.as_os_str().is_empty() || !requested.is_absolute() {
            return Err(ApplicationError::Configuration(
                "PACTRUN_STORAGE_ROOT must be a non-empty absolute path".to_owned(),
            ));
        }
        let root = validate_supported_storage_root(requested)
            .map_err(|error| ApplicationError::Configuration(error.to_string()))?;
        for child in ["database", "runtime-content", "staging"] {
            validate_supported_storage_root(&root.join(child))
                .map_err(|error| ApplicationError::Configuration(error.to_string()))?;
        }
        let persistence = if collection {
            PactrunPersistence::open_for_collection(&root)?
        } else {
            PactrunPersistence::open(&root)?
        };
        Ok(Self {
            persistence,
            storage_root: root,
            continuations: OwnerContinuationRegistry::default(),
            mutation_locks: Mutex::new(BTreeMap::new()),
        })
    }

    /// Opens only the exact current store for inspection and plan preview.
    /// This path intentionally has no staging lease and performs no bootstrap,
    /// migration, cleanup, reservation, pin, or reconciliation work.
    pub(crate) fn open_read_only(storage_root: impl AsRef<Path>) -> Result<Self, ApplicationError> {
        let requested = storage_root.as_ref();
        if requested.as_os_str().is_empty() || !requested.is_absolute() {
            return Err(ApplicationError::Configuration(
                "PACTRUN_STORAGE_ROOT must be a non-empty absolute path".to_owned(),
            ));
        }
        let root = validate_supported_storage_root(requested)
            .map_err(|error| ApplicationError::Configuration(error.to_string()))?;
        for child in ["database", "runtime-content", "staging"] {
            validate_supported_storage_root(&root.join(child))
                .map_err(|error| ApplicationError::Configuration(error.to_string()))?;
        }
        let persistence = PactrunPersistence::open_read_only(&root)?;
        Ok(Self {
            persistence,
            storage_root: root,
            continuations: OwnerContinuationRegistry::default(),
            mutation_locks: Mutex::new(BTreeMap::new()),
        })
    }

    fn staging(&self) -> Result<&StagingSession, ApplicationError> {
        self.persistence.staging_session().ok_or_else(|| {
            ApplicationError::InvalidRequest(
                "this operation requires a writable Pactrun opening".to_owned(),
            )
        })
    }

    pub(crate) fn delete_object(
        &self,
        target: &crate::domain::ObjectDeletion,
    ) -> Result<crate::domain::ObjectDeletionResult, ApplicationError> {
        Ok(self.persistence.delete_object(target)?)
    }

    pub(crate) fn collect_content(
        &self,
        execute: bool,
    ) -> Result<crate::domain::CollectionReport, ApplicationError> {
        Ok(self.persistence.collect_content(execute)?)
    }

    pub(crate) fn install_pack_source(
        &self,
        source_root: &Path,
        explicit_local_metadata: &RevisionMetadataMutationBatch,
    ) -> Result<InstallPackResult, ApplicationError> {
        installation::install_pack_source(
            &self.persistence,
            self.staging()?,
            source_root,
            explicit_local_metadata,
        )
    }

    pub(crate) fn resolve_reference_label(
        &self,
        label: &ReferenceLabel,
    ) -> Result<Vec<RevisionIdentity>, ApplicationError> {
        Ok(self.persistence.lookup_reference_label(label)?)
    }

    pub(crate) fn resolve_local_alias(
        &self,
        alias: &LocalAlias,
    ) -> Result<Option<RevisionIdentity>, ApplicationError> {
        Ok(self.persistence.lookup_local_alias(alias)?)
    }

    pub(crate) fn revision_exists(
        &self,
        revision: &RevisionIdentity,
    ) -> Result<bool, ApplicationError> {
        Ok(self.persistence.load_revision(revision)?.is_some())
    }

    pub(crate) fn create_instance(
        &self,
        name: InstanceName,
        revision: RevisionIdentity,
        mut initial: Vec<InputAcquisition<'_>>,
    ) -> Result<InstanceView, ApplicationError> {
        reject_duplicate_acquisitions(&initial)?;
        initial.sort_by(|left, right| left.input_id.cmp(&right.input_id));
        let stored_revision = self.persistence.load_revision(&revision)?.ok_or_else(|| {
            ApplicationError::InvalidRequest("Revision is not installed".to_owned())
        })?;
        for acquisition in &initial {
            if !stored_revision
                .content
                .core
                .inputs()
                .iter()
                .any(|declaration| declaration.id == acquisition.input_id)
            {
                return Err(ApplicationError::InvalidRequest(format!(
                    "Input {} is not active in the selected Revision",
                    acquisition.input_id.as_str()
                )));
            }
        }
        let mut staged = Vec::with_capacity(initial.len());
        for mut acquisition in initial {
            let bytes = self
                .staging()?
                .stage_managed_input(&mut acquisition.source)?;
            staged.push((acquisition.input_id, bytes));
        }
        let mut readers = staged
            .iter()
            .map(|(_, bytes)| bytes.try_clone_reader())
            .collect::<Result<Vec<_>, _>>()?;
        let mut writes = staged
            .iter()
            .zip(&mut readers)
            .map(|((input_id, bytes), reader)| ManagedInputWrite {
                input_id: input_id.clone(),
                byte_len: bytes.byte_len(),
                reader,
            })
            .collect::<Vec<_>>();
        Ok(self
            .persistence
            .create_instance(name, revision, &mut writes)?)
    }

    pub(crate) fn list_instances(&self) -> Result<Vec<InstanceSummary>, ApplicationError> {
        Ok(self.persistence.list_instances()?)
    }

    pub(crate) fn resolve_instance_name(
        &self,
        name: &InstanceName,
    ) -> Result<Option<InstanceId>, ApplicationError> {
        Ok(self.persistence.resolve_instance_name(name)?)
    }

    pub(crate) fn load_instance(
        &self,
        instance: InstanceId,
    ) -> Result<Option<InstanceView>, ApplicationError> {
        Ok(self.persistence.load_instance_by_id(instance)?)
    }

    pub(crate) fn load_action_definition(
        &self,
        instance_name: &InstanceName,
        action_id: &ActionIdentity,
    ) -> Result<(InstanceView, ActionV1), ApplicationError> {
        let instance_id = self
            .persistence
            .resolve_instance_name(instance_name)?
            .ok_or(ActionResolutionError::InstanceNotFound)?;
        let instance = self
            .persistence
            .load_instance_by_id(instance_id)?
            .ok_or_else(|| {
                ActionResolutionError::RepositoryInvariant(
                    "resolved Instance disappeared while loading Action".to_owned(),
                )
            })?;
        let revision = self
            .persistence
            .load_revision(&instance.active_revision)?
            .ok_or_else(|| {
                ActionResolutionError::RepositoryInvariant("active Revision is missing".to_owned())
            })?;
        let action = revision
            .content
            .core
            .actions()
            .iter()
            .find(|candidate| &candidate.id == action_id)
            .cloned()
            .ok_or(ActionResolutionError::ActionNotFound)?;
        Ok((instance, action))
    }

    pub(crate) fn list_action_definitions(
        &self,
        instance_name: &InstanceName,
    ) -> Result<(InstanceView, Vec<ActionV1>), ApplicationError> {
        let instance_id = self
            .persistence
            .resolve_instance_name(instance_name)?
            .ok_or(ActionResolutionError::InstanceNotFound)?;
        let instance = self
            .persistence
            .load_instance_by_id(instance_id)?
            .ok_or_else(|| {
                ActionResolutionError::RepositoryInvariant(
                    "resolved Instance disappeared while listing Actions".to_owned(),
                )
            })?;
        let revision = self
            .persistence
            .load_revision(&instance.active_revision)?
            .ok_or_else(|| {
                ActionResolutionError::RepositoryInvariant("active Revision is missing".to_owned())
            })?;
        Ok((instance, revision.content.core.actions().to_vec()))
    }

    #[allow(dead_code)]
    pub(crate) fn list_runs(
        &self,
        instance: InstanceId,
    ) -> Result<Vec<RunSummary>, ApplicationError> {
        Ok(self.persistence.list_runs(instance)?)
    }

    #[allow(dead_code)]
    pub(crate) fn load_run(&self, run: RunId) -> Result<Option<RunView>, ApplicationError> {
        Ok(self.persistence.load_run(run)?)
    }

    #[allow(dead_code)]
    pub(crate) fn load_run_inspection(
        &self,
        run: RunId,
    ) -> Result<Option<RunInspectionData>, ApplicationError> {
        Ok(self.persistence.load_run_inspection(run)?)
    }

    #[allow(dead_code)]
    pub(crate) fn stream_run_artifact(
        &self,
        run: RunId,
        output: &ManagedOutputIdentity,
        destination: &mut impl Write,
    ) -> Result<(), ApplicationError> {
        Ok(self
            .persistence
            .open_run_artifact(run, output, destination)?)
    }

    pub(crate) fn export_run_artifact(
        &self,
        run: RunId,
        output: &ManagedOutputIdentity,
        authorize_sensitive: bool,
    ) -> Result<StagedFile, ApplicationError> {
        if !authorize_sensitive {
            return Err(ApplicationError::InvalidRequest(
                "Artifact export requires --authorize-sensitive-export".to_owned(),
            ));
        }
        let mut staged = self.staging()?.create_managed_output_stage()?;
        self.stream_run_artifact(run, output, staged.writer())?;
        staged
            .finish_operation_file()
            .map_err(|source| ApplicationError::Io {
                operation: "finish Artifact export staging",
                source,
            })?;
        Ok(staged)
    }

    #[allow(dead_code)]
    pub(crate) fn delete_run_artifact(
        &self,
        run: RunId,
        output: &ManagedOutputIdentity,
    ) -> Result<bool, ApplicationError> {
        Ok(self.persistence.delete_run_artifact(run, output)?)
    }

    #[allow(dead_code)]
    pub(crate) fn resolve_action(
        &self,
        instance_name: &InstanceName,
        action_id: &ActionIdentity,
        parameters: Vec<RawParameterInput>,
    ) -> Result<InvokeAction, ApplicationError> {
        let instance_id = self
            .persistence
            .resolve_instance_name(instance_name)?
            .ok_or(ActionResolutionError::InstanceNotFound)?;
        let instance = self
            .persistence
            .load_instance_by_id(instance_id)?
            .ok_or_else(|| {
                ActionResolutionError::RepositoryInvariant(
                    "resolved Instance disappeared while resolving Action".to_owned(),
                )
            })?;
        let revision = self
            .persistence
            .load_revision(&instance.active_revision)?
            .ok_or_else(|| {
                ActionResolutionError::RepositoryInvariant("active Revision is missing".to_owned())
            })?;
        let action = revision
            .content
            .core
            .actions()
            .iter()
            .find(|candidate| &candidate.id == action_id)
            .ok_or(ActionResolutionError::ActionNotFound)?;
        let parameters = bind_action_parameters(action, parameters)?;
        Ok(InvokeAction {
            instance: instance.id,
            expected_state_version: instance.state_version,
            active_revision: instance.active_revision,
            action: action_id.clone(),
            parameters,
        })
    }

    #[allow(dead_code)]
    pub(crate) fn compile_action(
        &self,
        intent: &InvokeAction,
        launcher_search_directories: &[std::path::PathBuf],
    ) -> Result<ActionExecutionPlan, ApplicationError> {
        crate::workflow::compile_action(
            self,
            &crate::workflow::PlatformHostLauncherLookup,
            intent,
            launcher_search_directories,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn observe_action_compilation(
        &self,
        intent: &InvokeAction,
    ) -> Result<ActionCompilationObservation, PersistenceError> {
        let before = self
            .persistence
            .observe_instance_compilation_state(intent.instance)?
            .ok_or_else(|| PersistenceError::MissingInstance(intent.instance.to_string()))?;
        let revision = self
            .persistence
            .load_revision(&before.active_revision)?
            .ok_or_else(|| PersistenceError::MissingRevision(before.active_revision.clone()))?;
        let service_state = if revision.content.core.service_core().is_some() {
            self.persistence
                .load_instance_service_state(intent.instance)?
        } else {
            None
        };
        let after = self
            .persistence
            .observe_instance_compilation_state(intent.instance)?
            .ok_or_else(|| PersistenceError::MissingInstance(intent.instance.to_string()))?;
        if before != after {
            return Err(PersistenceError::CompilationObservationChanged);
        }
        Ok(ActionCompilationObservation {
            instance: before.instance,
            state_version: before.state_version,
            active_revision: before.active_revision,
            active_bindings: before.active_bindings,
            required_inputs_satisfied: before.required_inputs_satisfied,
            revision_content: revision.content,
            service_state,
        })
    }
    pub(crate) fn set_input(
        &self,
        instance: InstanceId,
        input_id: InputIdentity,
        expected: InstanceStateVersion,
        mut source: Box<dyn Read + '_>,
    ) -> Result<InstanceStateVersion, ApplicationError> {
        let lock = self.mutation_lock(instance)?;
        let _guard = lock.lock().map_err(|_| ApplicationError::LockPoisoned)?;
        let staged = self.staging()?.stage_managed_input(&mut source)?;
        let mut reader = staged.try_clone_reader()?;
        let mut write = ManagedInputWrite {
            input_id,
            byte_len: staged.byte_len(),
            reader: &mut reader,
        };
        Ok(self.persistence.set_input(instance, expected, &mut write)?)
    }

    pub(crate) fn delete_input(
        &self,
        instance: InstanceId,
        input_id: &InputIdentity,
        expected: InstanceStateVersion,
    ) -> Result<InstanceStateVersion, ApplicationError> {
        let lock = self.mutation_lock(instance)?;
        let _guard = lock.lock().map_err(|_| ApplicationError::LockPoisoned)?;
        Ok(self
            .persistence
            .delete_input(instance, expected, input_id)?)
    }

    pub(crate) fn export_input(
        &self,
        instance: InstanceId,
        input_id: &InputIdentity,
        authorize_secret: bool,
    ) -> Result<ExportObservation, ApplicationError> {
        // Observe operations deliberately do not acquire the mutation guard.
        let mut staged = self.staging()?.create_managed_output_stage()?;
        let state_version =
            self.persistence
                .export_input(instance, input_id, authorize_secret, staged.writer())?;
        staged.finish_managed_output()?;
        Ok(ExportObservation {
            state_version,
            bytes: staged,
        })
    }

    /// Accepts a compiled Plan as an execution attempt and admits it. The
    /// per-Instance mutation guard is held for exactly this call, never across
    /// Hook execution (PR-REQ-0278); the authoritative decision and its
    /// transition happen inside one persistence transaction (PR-REQ-0279).
    #[allow(dead_code)]
    pub(crate) fn accept_and_admit_action(
        &self,
        plan: &ActionExecutionPlan,
        options: AdmissionOptions,
    ) -> Result<AdmittedExecution, ApplicationError> {
        let lock = self.mutation_lock(plan.instance())?;
        let _guard = lock.lock().map_err(|_| ApplicationError::LockPoisoned)?;
        Ok(crate::executor::accept_and_admit(
            &self.persistence,
            &crate::workflow::PlatformHostLauncherLookup,
            &self.staging()?.owner(),
            plan,
            options,
        )?)
    }

    /// The foreground CLI passes the cancellation arbiter into the acceptance
    /// transaction. The arbiter is acquired only for the final commit decision,
    /// never across Admission; a request that wins before that boundary rolls
    /// the prepared transaction back, while a committed Run remains owner-held.
    pub(crate) fn accept_and_admit_action_with_cancellation(
        &self,
        plan: &ActionExecutionPlan,
        options: AdmissionOptions,
        cancellation: &ActionCancellation,
    ) -> Result<AdmittedExecution, ApplicationError> {
        let lock = self.mutation_lock(plan.instance())?;
        let _guard = lock.lock().map_err(|_| ApplicationError::LockPoisoned)?;
        Ok(crate::executor::accept_and_admit_with_arbiter(
            &self.persistence,
            &crate::workflow::PlatformHostLauncherLookup,
            &self.staging()?.owner(),
            plan,
            options,
            cancellation,
        )?)
    }

    pub(crate) fn admit_accepted_action(
        &self,
        run: RunId,
        plan: &ActionExecutionPlan,
        options: AdmissionOptions,
    ) -> Result<AdmittedExecution, ApplicationError> {
        let lock = self.mutation_lock(plan.instance())?;
        let _guard = lock.lock().map_err(|_| ApplicationError::LockPoisoned)?;
        Ok(crate::executor::admit_existing(
            &self.persistence,
            &crate::workflow::PlatformHostLauncherLookup,
            run,
            plan,
            options,
        )?)
    }

    pub(crate) fn cancel_accepted_action(&self, run: RunId) -> Result<(), ApplicationError> {
        let view = self
            .persistence
            .load_run(run)?
            .ok_or_else(|| ApplicationError::InvalidRequest("Run is not persisted".to_owned()))?;
        if !matches!(view.state, crate::domain::RunState::Running(ref execution)
            if execution.boundary == crate::domain::ActionRunBoundary::Accepted)
        {
            return Err(ApplicationError::InvalidRequest(
                "Run is no longer in the Accepted boundary".to_owned(),
            ));
        }
        let lock = self.mutation_lock(view.instance)?;
        let _guard = lock.lock().map_err(|_| ApplicationError::LockPoisoned)?;
        let finish = RunFinish {
            outcome: RunOutcome::Cancelled,
            primary_failure: None,
            secondary_failures: Vec::new(),
            hook_completion: None,
        };
        self.persistence
            .finish_run_owned(&self.staging()?.owner(), run, &finish, &[], &mut [])?;
        Ok(())
    }

    /// Consumes one admitted Action and guarantees that every outward path
    /// leaves an owner-held continuation while this process owns the Run.
    #[allow(dead_code)]
    pub(crate) fn execute_admitted_action(
        &self,
        admitted: AdmittedExecution,
        policy: HookRuntimePolicy,
        cancellation: ActionCancellation,
    ) -> RunId {
        crate::hook::execute_admitted_action(
            &self.persistence,
            self.staging().expect("execution requires writable opening"),
            &self.continuations,
            admitted,
            policy,
            cancellation,
        )
    }

    #[cfg(test)]
    pub(crate) fn execute_admitted_action_with_risk_failures(
        &self,
        admitted: AdmittedExecution,
        policy: HookRuntimePolicy,
        cancellation: ActionCancellation,
        remaining_failures: &std::sync::atomic::AtomicUsize,
    ) -> RunId {
        crate::hook::execute_admitted_action_with_risk_failures(
            &self.persistence,
            self.staging().expect("execution requires writable opening"),
            &self.continuations,
            admitted,
            policy,
            cancellation,
            remaining_failures,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn take_owner_continuation(&self, run: RunId) -> Option<ContinuationGuard<'_>> {
        self.continuations.take(run)
    }

    /// Simulates this owner process dying: the volatile continuation registry
    /// disappears and the staging lease is released without cleanup.
    #[cfg(test)]
    pub(crate) fn abandon_execution_owner(self) {
        self.persistence.abandon_execution_owner();
    }

    #[allow(dead_code)]
    pub(crate) fn resume_owner_continuation(&self, run: RunId) -> bool {
        let Some(guard) = self.continuations.take(run) else {
            return false;
        };
        crate::hook::resume_owner_continuation(
            &self.persistence,
            self.staging().expect("execution requires writable opening"),
            guard,
        );
        true
    }

    /// Advances the owner-held runtime/finalization state once. The
    /// continuation remains registered until durable terminal publication is
    /// confirmed.
    #[allow(dead_code)]
    pub(crate) fn advance_owner_continuation(&self, run: RunId) -> Result<bool, ApplicationError> {
        #[cfg(test)]
        if FAIL_FINALIZATION_ADVANCES_FOR_TEST.with(|remaining| {
            let current = remaining.get();
            if current == 0 {
                false
            } else {
                remaining.set(current - 1);
                true
            }
        }) {
            return Err(ApplicationError::Persistence(
                PersistenceError::DatabaseLockPoisoned,
            ));
        }
        let Some(guard) = self.continuations.take(run) else {
            return Ok(false);
        };
        if let Some(instance) = guard.managed_mutation_instance() {
            let lock = self.mutation_lock(instance)?;
            let _mutation = lock.lock().map_err(|_| ApplicationError::LockPoisoned)?;
            return Ok(crate::hook::advance_owner_continuation(
                &self.persistence,
                self.staging()?,
                guard,
            )?);
        }
        Ok(crate::hook::advance_owner_continuation(
            &self.persistence,
            self.staging().expect("execution requires writable opening"),
            guard,
        )?)
    }

    /// Reconciles only Runs whose recorded staging lease is conclusively
    /// lost. An inconclusive probe is left untouched for a later scan.
    #[allow(dead_code)]
    pub(crate) fn reconcile_lost_action_owners(&self) -> Result<Vec<RunId>, ApplicationError> {
        self.reconcile_lost_managed_owners()
    }

    pub(crate) fn reconcile_lost_managed_owners(&self) -> Result<Vec<RunId>, ApplicationError> {
        let candidates = self.persistence.list_running_managed_runs()?;
        let mut reconciled = Vec::new();
        for (instance, run, owner) in candidates {
            if probe_session_owner(&self.storage_root, &owner) != SessionOwnerProbe::ConfirmedLoss {
                continue;
            }
            crate::persistence::fault(crate::persistence::FaultPoint::AfterRecoveryOwnerLossProbe);
            let lock = self.mutation_lock(instance)?;
            let _guard = lock.lock().map_err(|_| ApplicationError::LockPoisoned)?;
            if probe_session_owner(&self.storage_root, &owner) != SessionOwnerProbe::ConfirmedLoss {
                continue;
            }
            if self.persistence.reconcile_managed_run(run, &owner)? {
                reconciled.push(run);
                let _ = cleanup_lost_session(&self.storage_root, &owner);
            }
        }
        Ok(reconciled)
    }

    /// `ResolveManualRecovery` is a no-Hook, no-Compiler, no-Run management
    /// mutation: it takes the per-Instance mutation guard, clears the trust
    /// guard with a token-first comparison, and publishes a fresh state version.
    #[allow(dead_code)]
    pub(crate) fn resolve_manual_recovery(
        &self,
        instance: InstanceId,
        expected: InstanceStateVersion,
    ) -> Result<InstanceStateVersion, ApplicationError> {
        let lock = self.mutation_lock(instance)?;
        let _guard = lock.lock().map_err(|_| ApplicationError::LockPoisoned)?;
        Ok(self
            .persistence
            .resolve_manual_recovery(instance, expected)?)
    }

    /// The execution owner identity this process records on accepted Runs.
    #[allow(dead_code)]
    pub(crate) fn execution_owner(&self) -> ExecutionOwnerSession {
        self.persistence
            .staging_session()
            .expect("execution owner requires writable opening")
            .owner()
    }

    fn mutation_lock(&self, instance: InstanceId) -> Result<Arc<Mutex<()>>, ApplicationError> {
        let mut locks = self
            .mutation_locks
            .lock()
            .map_err(|_| ApplicationError::LockPoisoned)?;
        Ok(Arc::clone(
            locks
                .entry(instance)
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        ))
    }
}

fn reject_duplicate_acquisitions(initial: &[InputAcquisition<'_>]) -> Result<(), ApplicationError> {
    let mut ids = BTreeSet::new();
    for input in initial {
        if !ids.insert(&input.input_id) {
            return Err(ApplicationError::InvalidRequest(
                "duplicate initial Input identity".to_owned(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod action_tests {
    use super::*;
    use std::{fs, io::Cursor};
    use tempfile::TempDir;

    fn roots() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m3-slice1-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("action-")
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
        RevisionMetadataMutationBatch::new(Vec::<crate::domain::RevisionMetadataMutation>::new())
            .unwrap()
    }

    // Test-ID: PR-TEST-0115
    // Verifies: PR-REQ-0096
    #[test]
    fn application_resolves_exact_action_and_compilation_is_persistently_read_only() {
        let (_temporary, storage, source) = roots();
        fs::write(source.join("tool.bin"), b"tool").unwrap();
        fs::write(source.join("config.bin"), b"secret config").unwrap();
        fs::write(
            source.join("pactrun.yaml"),
            r#"source_format: 1
package_id: 00000000000000000000000000000031
revision:
  inputs:
    - { id: config, required: true, protection: secret }
  actions:
    - id: inspect
      access: observe
      parameters:
        - { id: count, type: integer, sensitive: false }
      hook:
        protocol_version: 1
        launch: { kind: direct, executable: tool }
        args: [fixed]
        io: { terminal: output }
      outputs: [{ id: report }]
  migrations: []
runtime_content:
  files:
    - { id: tool, source: tool.bin, path: bin/tool, executable: true }
"#,
        )
        .unwrap();
        let application = PactrunApplication::open(&storage).unwrap();
        let installed = application
            .install_pack_source(&source, &empty_metadata())
            .unwrap();
        let name = InstanceName::parse("node").unwrap();
        assert!(matches!(
            application.resolve_action(
                &name,
                &ActionIdentity::parse("inspect").unwrap(),
                Vec::new(),
            ),
            Err(ApplicationError::ActionResolution(
                ActionResolutionError::InstanceNotFound
            ))
        ));
        let config = fs::File::open(source.join("config.bin")).unwrap();
        let instance = application
            .create_instance(
                name.clone(),
                installed.revision,
                vec![InputAcquisition {
                    input_id: InputIdentity::parse("config").unwrap(),
                    source: Box::new(config),
                }],
            )
            .unwrap();
        assert!(matches!(
            application.resolve_action(
                &name,
                &ActionIdentity::parse("missing").unwrap(),
                Vec::new(),
            ),
            Err(ApplicationError::ActionResolution(
                ActionResolutionError::ActionNotFound
            ))
        ));
        let database_path = storage.join("database/pactrun.sqlite3");
        let before_database = fs::read(&database_path).unwrap();
        let before_runtime = fs::read_dir(storage.join("runtime-content"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        let intent = application
            .resolve_action(
                &name,
                &ActionIdentity::parse("inspect").unwrap(),
                vec![RawParameterInput {
                    id: crate::domain::ParameterIdentity::parse("count").unwrap(),
                    text: "3".to_owned(),
                    source: crate::domain::ParameterTextSource::Ordinary,
                }],
            )
            .unwrap();
        let plan = application.compile_action(&intent, &[]).unwrap();
        assert_eq!(plan.expected_state_version(), intent.expected_state_version);
        assert_eq!(plan.active_bindings().len(), 1);
        assert_eq!(plan.active_bindings()[0].input.as_str(), "config");
        assert_eq!(
            plan.active_bindings()[0].protection,
            crate::domain::ManagedInputProtection::Secret
        );
        assert_eq!(fs::read(&database_path).unwrap(), before_database);
        let after_runtime = fs::read_dir(storage.join("runtime-content"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(after_runtime, before_runtime);
        assert!(!storage.join("staging/workspace").exists());

        let read_only = PactrunApplication::open_read_only(&storage).unwrap();
        let (_, actions) = read_only
            .list_action_definitions(&name)
            .expect("read-only action inspection succeeds");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].id.as_str(), "inspect");
        drop(read_only);
        assert_eq!(fs::read(&database_path).unwrap(), before_database);
        assert!(!storage.join("staging/workspace").exists());

        application
            .set_input(
                instance.id,
                InputIdentity::parse("config").unwrap(),
                instance.state_version,
                Box::new(Cursor::new(b"replacement config".to_vec())),
            )
            .unwrap();
        assert!(matches!(
            application.compile_action(&intent, &[]),
            Err(ApplicationError::PlanCompilation(
                PlanCompilationError::InconsistentFacts
            ))
        ));
    }

    // Test-ID: PR-TEST-0088
    // Verifies: PR-REQ-0069
    #[test]
    fn resolve_manual_recovery_is_a_guarded_no_run_management_mutation() {
        use std::{
            sync::{Arc, mpsc},
            thread,
            time::Duration,
        };

        use crate::domain::{
            ActionRunIdentity, AdmissionFacts, CompiledHookLaunch, ExecutionOwnerSession,
            RunFinish, RunOutcome,
        };

        let (_temporary, storage, source) = roots();
        fs::write(source.join("config.bin"), b"config").unwrap();
        fs::write(source.join("tool.bin"), b"tool").unwrap();
        fs::write(
            source.join("pactrun.yaml"),
            r#"source_format: 1
package_id: 00000000000000000000000000000032
revision:
  inputs:
    - { id: config, required: true }
  actions:
    - id: deploy
      access: observe
      parameters: []
      hook:
        protocol_version: 1
        launch: { kind: direct, executable: tool }
        args: []
        io: { terminal: none }
      outputs: []
  migrations: []
runtime_content:
  files:
    - { id: tool, source: tool.bin, path: bin/tool, executable: true }
"#,
        )
        .unwrap();
        let application = Arc::new(PactrunApplication::open(&storage).unwrap());
        let installed = application
            .install_pack_source(&source, &empty_metadata())
            .unwrap();
        let instance = application
            .create_instance(
                InstanceName::parse("guarded").unwrap(),
                installed.revision.clone(),
                vec![InputAcquisition {
                    input_id: InputIdentity::parse("config").unwrap(),
                    source: Box::new(fs::File::open(source.join("config.bin")).unwrap()),
                }],
            )
            .unwrap();

        // Publish a ManualRecoveryRequired guard through the persistence
        // substrate: an admitted Run fails with open risk.
        let owner = application.execution_owner();
        assert!(ExecutionOwnerSession::is_session_name(owner.as_str()));
        let run = application
            .persistence
            .create_accepted_run(
                RunId::generate().unwrap(),
                instance.id,
                instance.state_version,
                &ActionRunIdentity {
                    revision: installed.revision.clone(),
                    action: ActionIdentity::parse("deploy").unwrap(),
                },
                &owner,
                &crate::persistence::UnconditionalAcceptance,
            )
            .unwrap();
        let bindings = application
            .persistence
            .observe_instance_compilation_state(instance.id)
            .unwrap()
            .unwrap()
            .active_bindings;
        let stored = application
            .persistence
            .load_revision(&installed.revision)
            .unwrap()
            .unwrap();
        let files = stored.content.runtime_content.files();
        let launch = CompiledHookLaunch::Direct {
            executable: files[0].clone(),
        };
        application
            .persistence
            .admit_run(
                run,
                &AdmissionFacts {
                    service: None,
                    expected_state_version: instance.state_version,
                    active_bindings: &bindings,
                    runtime_content: files,
                    launch: &launch,
                },
                &|_| Ok(()),
                false,
            )
            .unwrap()
            .unwrap();
        application.persistence.open_recovery_risk(run).unwrap();
        let receipt = application
            .persistence
            .finish_run(
                run,
                &RunFinish {
                    outcome: RunOutcome::Failed,
                    primary_failure: None,
                    secondary_failures: Vec::new(),
                    hook_completion: None,
                },
                &mut [],
            )
            .unwrap();
        let guarded = receipt.published_state_version.unwrap();
        assert!(
            application
                .persistence
                .load_instance_recovery_guard(instance.id)
                .unwrap()
                .is_some()
        );
        let runs_before = application
            .persistence
            .list_runs(instance.id)
            .unwrap()
            .len();

        // A stale token is rejected before anything changes.
        assert!(matches!(
            application.resolve_manual_recovery(instance.id, instance.state_version),
            Err(ApplicationError::Persistence(
                PersistenceError::StaleInstanceState
            ))
        ));
        assert!(
            application
                .persistence
                .load_instance_recovery_guard(instance.id)
                .unwrap()
                .is_some()
        );

        // The mutation guard serializes ResolveManualRecovery with other
        // management mutations on the same Instance.
        let held = application.mutation_lock(instance.id).unwrap();
        let holder = held.lock().unwrap();
        let (started, observe) = mpsc::channel();
        let resolver = {
            let application = Arc::clone(&application);
            thread::spawn(move || {
                started.send(()).unwrap();
                application.resolve_manual_recovery(instance.id, guarded)
            })
        };
        observe.recv().unwrap();
        thread::sleep(Duration::from_millis(200));
        assert!(
            application
                .persistence
                .load_instance_recovery_guard(instance.id)
                .unwrap()
                .is_some(),
            "resolution must wait for the mutation guard"
        );
        drop(holder);
        let resolved = resolver.join().unwrap().unwrap();
        assert_ne!(resolved, guarded);
        assert_eq!(
            application
                .load_instance(instance.id)
                .unwrap()
                .unwrap()
                .state_version,
            resolved
        );
        assert!(
            application
                .persistence
                .load_instance_recovery_guard(instance.id)
                .unwrap()
                .is_none()
        );

        // No Run, Plan, or Hook was involved: the Run history is unchanged and
        // no staging workspace appeared.
        assert_eq!(
            application
                .persistence
                .list_runs(instance.id)
                .unwrap()
                .len(),
            runs_before
        );
        assert!(!storage.join("staging/workspace").exists());
        assert!(matches!(
            application.resolve_manual_recovery(instance.id, resolved),
            Err(ApplicationError::Persistence(
                PersistenceError::MissingRecoveryGuard
            ))
        ));
    }
}

#[cfg(test)]
mod admission_tests {
    use std::{
        fs,
        io::Cursor,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::{Arc, mpsc},
        thread,
        time::Duration,
    };

    use tempfile::TempDir;

    use super::*;
    use crate::{
        domain::{
            ActionRunBoundary, AdmissionRefusal, RecoveryRiskState, RunFailedStep, RunFinish,
            RunId, RunOutcome, RunPhase, RunState, RunView,
        },
        executor::{AdmissionOptions, AdmittedExecution, ExecutorError},
        persistence::FaultPoint,
    };

    const WORKER_TEST: &str = "application::admission_tests::m3_admission_worker";

    struct Fixture {
        _temporary: TempDir,
        storage: PathBuf,
        first: PathBuf,
        second: PathBuf,
        source: PathBuf,
    }

    fn launcher_command() -> &'static str {
        if cfg!(windows) {
            "runtime.exe"
        } else {
            "runtime"
        }
    }

    #[cfg(unix)]
    fn write_eligible_launcher(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(path, b"not a prevalidated executable image").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[cfg(windows)]
    fn write_eligible_launcher(path: &Path) {
        fs::write(path, b"not a prevalidated executable image").unwrap();
    }

    fn fixture() -> Fixture {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m3-slice3-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("admission-")
            .tempdir_in(parent)
            .unwrap();
        let storage = temporary.path().join("storage");
        let source = temporary.path().join("source");
        let first = temporary.path().join("first");
        let second = temporary.path().join("second");
        for directory in [&storage, &source, &first, &second] {
            fs::create_dir(directory).unwrap();
        }
        for child in ["database", "runtime-content", "staging"] {
            fs::create_dir(storage.join(child)).unwrap();
        }
        write_eligible_launcher(&first.join(launcher_command()));
        write_eligible_launcher(&second.join(launcher_command()));
        fs::write(source.join("config.bin"), b"config bytes").unwrap();
        fs::write(source.join("tool.bin"), b"tool image").unwrap();
        fs::write(source.join("hook.txt"), b"hook script").unwrap();
        fs::write(
            source.join("pactrun.yaml"),
            format!(
                r#"source_format: 1
package_id: 00000000000000000000000000000033
revision:
  inputs:
    - {{ id: config, required: true }}
  actions:
    - id: inspect
      access: observe
      parameters: []
      hook:
        protocol_version: 1
        launch: {{ kind: direct, executable: tool }}
        args: []
        io: {{ terminal: output }}
      outputs: []
    - id: deploy
      access: mutate
      parameters: []
      hook:
        protocol_version: 1
        launch: {{ kind: interpreter, command: {command}, interpreter_args: [--strict], script: hook }}
        args: []
        io: {{ terminal: none }}
      outputs: []
    - id: audit
      access: observe
      parameters: []
      hook:
        protocol_version: 1
        launch: {{ kind: interpreter, command: {command}, interpreter_args: [], script: hook }}
        args: []
        io: {{ terminal: none }}
      outputs: []
  migrations: []
runtime_content:
  files:
    - {{ id: tool, source: tool.bin, path: bin/tool, executable: true }}
    - {{ id: hook, source: hook.txt, path: bin/hook }}
"#,
                command = launcher_command()
            ),
        )
        .unwrap();
        Fixture {
            _temporary: temporary,
            storage,
            first,
            second,
            source,
        }
    }

    fn metadata() -> RevisionMetadataMutationBatch {
        RevisionMetadataMutationBatch::new(Vec::<crate::domain::RevisionMetadataMutation>::new())
            .unwrap()
    }

    fn install(application: &PactrunApplication, fixture: &Fixture) -> RevisionIdentity {
        application
            .install_pack_source(&fixture.source, &metadata())
            .unwrap()
            .revision
    }

    fn create(
        application: &PactrunApplication,
        revision: &RevisionIdentity,
        name: &str,
        with_config: bool,
    ) -> InstanceView {
        let mut initial = Vec::new();
        if with_config {
            initial.push(InputAcquisition {
                input_id: InputIdentity::parse("config").unwrap(),
                source: Box::new(Cursor::new(b"config bytes".to_vec())),
            });
        }
        application
            .create_instance(
                InstanceName::parse(name).unwrap(),
                revision.clone(),
                initial,
            )
            .unwrap()
    }

    fn plan(
        application: &PactrunApplication,
        fixture: &Fixture,
        instance: &str,
        action: &str,
    ) -> ActionExecutionPlan {
        let intent = application
            .resolve_action(
                &InstanceName::parse(instance).unwrap(),
                &ActionIdentity::parse(action).unwrap(),
                Vec::new(),
            )
            .unwrap();
        application
            .compile_action(&intent, &[fixture.first.clone(), fixture.second.clone()])
            .unwrap()
    }

    fn admit(
        application: &PactrunApplication,
        plan: &ActionExecutionPlan,
        recovery_override: bool,
    ) -> Result<AdmittedExecution, ApplicationError> {
        application.accept_and_admit_action(plan, AdmissionOptions { recovery_override })
    }

    fn expect_refusal(
        result: Result<AdmittedExecution, ApplicationError>,
    ) -> (RunId, AdmissionRefusal) {
        match result {
            Err(ApplicationError::Execution(ExecutorError::Refused { run, refusal })) => {
                (run, refusal)
            }
            other => panic!("expected an Admission refusal, found {other:?}"),
        }
    }

    fn view(application: &PactrunApplication, run: RunId) -> RunView {
        application.persistence.load_run(run).unwrap().unwrap()
    }

    /// Counts through an independent read connection so the assertion observes
    /// only committed state.
    fn count(fixture: &Fixture, sql: &str) -> i64 {
        let database =
            rusqlite::Connection::open(fixture.storage.join("database/pactrun.sqlite3")).unwrap();
        database.query_row(sql, [], |row| row.get(0)).unwrap()
    }

    fn token(application: &PactrunApplication, instance: InstanceId) -> InstanceStateVersion {
        application
            .load_instance(instance)
            .unwrap()
            .unwrap()
            .state_version
    }

    fn replace_config(application: &PactrunApplication, instance: &InstanceView) {
        let current = token(application, instance.id);
        application
            .set_input(
                instance.id,
                InputIdentity::parse("config").unwrap(),
                current,
                Box::new(Cursor::new(b"replacement".to_vec())),
            )
            .unwrap();
    }

    fn assert_refused_run(
        application: &PactrunApplication,
        run: RunId,
        code: &str,
        boundary: ActionRunBoundary,
    ) {
        match view(application, run).state {
            RunState::Finished(outcome) => {
                assert_eq!(outcome.outcome, RunOutcome::Failed);
                assert_eq!(outcome.boundary, boundary);
                assert_eq!(outcome.terminal_risk, RecoveryRiskState::Clear);
                let primary = outcome
                    .primary_failure
                    .expect("refusal records a primary failure");
                assert_eq!(primary.failure.error.owner(), "admission");
                assert_eq!(primary.failure.error.code(), code);
                assert_eq!(primary.step, RunFailedStep::Admission);
            }
            other => panic!("expected a Finished refusal, found {other:?}"),
        }
    }

    fn finish(application: &PactrunApplication, run: RunId, outcome: RunOutcome) {
        application
            .persistence
            .finish_run(
                run,
                &RunFinish {
                    outcome,
                    primary_failure: None,
                    secondary_failures: Vec::new(),
                    hook_completion: None,
                },
                &mut [],
            )
            .unwrap();
    }

    /// Runs `accept_and_admit_action` in a separate process. The worker opens
    /// its own application (own staging session and owner), resolves and
    /// compiles the Plan itself because Plans are not serializable, and writes
    /// `admitted <run> <owner>` or `refused <code> <run> <owner>` to `result`.
    fn run_worker(
        fixture: &Fixture,
        operation: &str,
        instance: &str,
        action: &str,
        fault: Option<FaultPoint>,
        result: &Path,
    ) -> std::process::Child {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg(WORKER_TEST)
            .arg("--nocapture")
            .env("PACTRUN_M3_ADMIT_WORKER", operation)
            .env("PACTRUN_M3_ADMIT_STORAGE", &fixture.storage)
            .env("PACTRUN_M3_ADMIT_INSTANCE", instance)
            .env("PACTRUN_M3_ADMIT_ACTION", action)
            .env("PACTRUN_M3_ADMIT_FIRST", &fixture.first)
            .env("PACTRUN_M3_ADMIT_SECOND", &fixture.second)
            .env("PACTRUN_M3_ADMIT_RESULT", result)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(fault) = fault {
            command.env("PACTRUN_M3_FAULT", fault.name());
        }
        command.spawn().unwrap()
    }

    fn wait_worker(child: std::process::Child, expect_success: bool) {
        let output = child.wait_with_output().unwrap();
        if output.status.success() != expect_success {
            panic!(
                "admission worker exited with {}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[test]
    fn m3_admission_worker() {
        let Some(operation) = std::env::var_os("PACTRUN_M3_ADMIT_WORKER") else {
            return;
        };
        let storage = PathBuf::from(std::env::var_os("PACTRUN_M3_ADMIT_STORAGE").unwrap());
        let instance = std::env::var("PACTRUN_M3_ADMIT_INSTANCE").unwrap();
        let action = std::env::var("PACTRUN_M3_ADMIT_ACTION").unwrap();
        let first = PathBuf::from(std::env::var_os("PACTRUN_M3_ADMIT_FIRST").unwrap());
        let second = PathBuf::from(std::env::var_os("PACTRUN_M3_ADMIT_SECOND").unwrap());
        let result = PathBuf::from(std::env::var_os("PACTRUN_M3_ADMIT_RESULT").unwrap());
        let application = PactrunApplication::open(&storage).unwrap();
        let owner = application.execution_owner();
        let instance_name = InstanceName::parse(instance).unwrap();
        let intent = application
            .resolve_action(
                &instance_name,
                &ActionIdentity::parse(action).unwrap(),
                Vec::new(),
            )
            .unwrap();
        let plan = application
            .compile_action(&intent, &[first, second])
            .unwrap();
        if operation.to_string_lossy() == "admit_stale" {
            let instance = application.load_instance(plan.instance()).unwrap().unwrap();
            application
                .set_input(
                    instance.id,
                    InputIdentity::parse("config").unwrap(),
                    instance.state_version,
                    Box::new(Cursor::new(b"worker replacement".to_vec())),
                )
                .unwrap();
        }
        let report = match application.accept_and_admit_action(&plan, AdmissionOptions::default()) {
            Ok(admitted) => format!("admitted {} {}", admitted.run(), owner.as_str()),
            Err(ApplicationError::Execution(ExecutorError::Refused { run, refusal })) => {
                format!(
                    "refused {} {} {}",
                    refusal.error_ref().code(),
                    run,
                    owner.as_str()
                )
            }
            Err(error) => panic!("unexpected admission error {error}"),
        };
        fs::write(result, report).unwrap();
    }

    fn runs_for(application: &PactrunApplication, instance: InstanceId) -> Vec<RunView> {
        application
            .persistence
            .list_runs(instance)
            .unwrap()
            .into_iter()
            .map(|summary| view(application, summary.id))
            .collect()
    }

    // Test-ID: PR-TEST-0089
    // Verifies: PR-REQ-0039, PR-REQ-0041, PR-REQ-0042, PR-REQ-0049
    #[test]
    fn run_acceptance_precedes_admission_and_both_are_durable_boundaries() {
        let fixture = fixture();
        let application = PactrunApplication::open(&fixture.storage).unwrap();
        let revision = install(&application, &fixture);
        let instance = create(&application, &revision, "accept", true);
        let database_path = fixture.storage.join("database/pactrun.sqlite3");

        // Compilation is side-effect free: no Run, pin, or guard, identical
        // database bytes, and the mutation guard is free afterwards.
        let before = fs::read(&database_path).unwrap();
        let compiled = plan(&application, &fixture, "accept", "inspect");
        assert_eq!(fs::read(&database_path).unwrap(), before);
        assert_eq!(count(&fixture, "SELECT COUNT(*) FROM runs"), 0);
        assert_eq!(count(&fixture, "SELECT COUNT(*) FROM run_revision_pins"), 0);
        assert!(
            application
                .mutation_lock(instance.id)
                .unwrap()
                .try_lock()
                .is_ok()
        );

        // Acceptance creates the durable Run; Admission pins it in the same
        // process's ownership.
        let admitted = admit(&application, &compiled, false).unwrap();
        assert_eq!(admitted.plan(), &compiled);
        let admitted_view = view(&application, admitted.run());
        assert_eq!(admitted_view.instance, instance.id);
        assert_eq!(admitted_view.accepted_state_version, instance.state_version);
        assert_eq!(admitted_view.action.revision, revision);
        assert_eq!(admitted_view.action.action.as_str(), "inspect");
        match admitted_view.state {
            RunState::Running(execution) => {
                assert_eq!(execution.boundary, ActionRunBoundary::Admitted);
                assert_eq!(execution.owner, application.execution_owner());
            }
            other => panic!("expected an admitted Run, found {other:?}"),
        }
        assert_eq!(token(&application, instance.id), instance.state_version);

        // A refusal is recorded in the Run created at acceptance, never lost.
        let stale = plan(&application, &fixture, "accept", "inspect");
        replace_config(&application, &instance);
        let (refused, refusal) = expect_refusal(admit(&application, &stale, false));
        assert!(matches!(refusal, AdmissionRefusal::PlanInvalidated(_)));
        assert_refused_run(
            &application,
            refused,
            "plan_invalidated",
            ActionRunBoundary::Accepted,
        );
        assert_eq!(count(&fixture, "SELECT COUNT(*) FROM runs"), 2);

        // Crash injection around both durable boundaries.
        let cases: [(&str, &str, FaultPoint, RunPhase, ActionRunBoundary, bool); 4] = [
            (
                "crash-accept-before",
                "admit",
                FaultPoint::BeforeRunAcceptCommit,
                RunPhase::Running,
                ActionRunBoundary::Accepted,
                false,
            ),
            (
                "crash-accept-after",
                "admit",
                FaultPoint::AfterRunAcceptCommit,
                RunPhase::Running,
                ActionRunBoundary::Accepted,
                true,
            ),
            (
                "crash-admit-before",
                "admit",
                FaultPoint::BeforeRunAdmitCommit,
                RunPhase::Running,
                ActionRunBoundary::Accepted,
                true,
            ),
            (
                "crash-admit-after",
                "admit",
                FaultPoint::AfterRunAdmitCommit,
                RunPhase::Running,
                ActionRunBoundary::Admitted,
                true,
            ),
        ];
        for (name, operation, fault, phase, boundary, run_exists) in cases {
            let crashed = create(&application, &revision, name, true);
            let result = fixture._temporary.path().join(format!("{name}.result"));
            wait_worker(
                run_worker(&fixture, operation, name, "inspect", Some(fault), &result),
                false,
            );
            assert!(!result.exists());
            let runs = runs_for(&application, crashed.id);
            if !run_exists {
                assert!(
                    runs.is_empty(),
                    "{name}: no Run may exist before the accept commit"
                );
                continue;
            }
            assert_eq!(runs.len(), 1, "{name}");
            assert_eq!(runs[0].state.phase(), phase, "{name}");
            match &runs[0].state {
                RunState::Running(execution) => {
                    assert_eq!(execution.boundary, boundary, "{name}");
                    assert_ne!(execution.owner, application.execution_owner());
                }
                other => panic!("{name}: unexpected state {other:?}"),
            }
        }

        // The refusal outcome commits atomically with the decision.
        let refused_before = create(&application, &revision, "crash-refuse-before", true);
        let result = fixture._temporary.path().join("refuse-before.result");
        wait_worker(
            run_worker(
                &fixture,
                "admit_stale",
                "crash-refuse-before",
                "inspect",
                Some(FaultPoint::BeforeRunAdmitCommit),
                &result,
            ),
            false,
        );
        let runs = runs_for(&application, refused_before.id);
        assert_eq!(runs.len(), 1);
        assert!(
            matches!(&runs[0].state, RunState::Running(execution) if execution.boundary == ActionRunBoundary::Accepted)
        );
        let refused_after = create(&application, &revision, "crash-refuse-after", true);
        let result = fixture._temporary.path().join("refuse-after.result");
        wait_worker(
            run_worker(
                &fixture,
                "admit_stale",
                "crash-refuse-after",
                "inspect",
                Some(FaultPoint::AfterRunAdmitCommit),
                &result,
            ),
            false,
        );
        let runs = runs_for(&application, refused_after.id);
        assert_eq!(runs.len(), 1);
        assert_refused_run(
            &application,
            runs[0].id,
            "plan_invalidated",
            ActionRunBoundary::Accepted,
        );
    }

    // Test-ID: PR-TEST-0090
    // Verifies: PR-REQ-0043, PR-REQ-0194
    // Supporting coverage for the Action clause of PR-REQ-0091 (readiness at
    // Admission); the requirement itself remains Pending until its Capture,
    // Migration, Cleanup, and Restore clauses are implemented.
    #[test]
    fn admission_revalidates_every_compilation_fact_without_recompiling() {
        let fixture = fixture();
        let application = PactrunApplication::open(&fixture.storage).unwrap();
        let revision = install(&application, &fixture);
        let instance = create(&application, &revision, "facts", true);

        // Stale state token.
        let stale = plan(&application, &fixture, "facts", "inspect");
        replace_config(&application, &instance);
        let (run, refusal) = expect_refusal(admit(&application, &stale, false));
        assert!(matches!(refusal, AdmissionRefusal::PlanInvalidated(_)));
        assert_refused_run(
            &application,
            run,
            "plan_invalidated",
            ActionRunBoundary::Accepted,
        );
        let recorded = view(&application, run);
        assert_eq!(recorded.action.revision, *stale.active_revision());
        assert_eq!(recorded.action.action, *stale.action());

        // Readiness: a required Input is unbound.
        create(&application, &revision, "unready", false);
        let unready = plan(&application, &fixture, "unready", "inspect");
        let (run, refusal) = expect_refusal(admit(&application, &unready, false));
        match refusal {
            AdmissionRefusal::PlanInvalidated(reason) => assert!(reason.contains("config")),
            other => panic!("unexpected refusal {other:?}"),
        }
        assert_refused_run(
            &application,
            run,
            "plan_invalidated",
            ActionRunBoundary::Accepted,
        );

        // Runtime content availability.
        let fresh = plan(&application, &fixture, "facts", "inspect");
        let runtime_root = fixture.storage.join("runtime-content");
        let tool_blob =
            fresh.runtime_content()[0].blob_digest.as_str()["sha256:".len()..].to_owned();
        let blob_path = runtime_root.join(&tool_blob);
        let parked = runtime_root.join("parked");
        fs::rename(&blob_path, &parked).unwrap();
        let (run, refusal) = expect_refusal(admit(&application, &fresh, false));
        assert!(matches!(refusal, AdmissionRefusal::PlanInvalidated(_)));
        assert_refused_run(
            &application,
            run,
            "plan_invalidated",
            ActionRunBoundary::Accepted,
        );
        fs::rename(&parked, &blob_path).unwrap();

        // Interpreter launcher re-selection must reproduce the exact bound path.
        let audit = plan(&application, &fixture, "facts", "audit");
        let first_candidate = fixture.first.join(launcher_command());
        fs::remove_file(&first_candidate).unwrap();
        let (run, refusal) = expect_refusal(admit(&application, &audit, false));
        assert!(matches!(refusal, AdmissionRefusal::PlanInvalidated(_)));
        assert_refused_run(
            &application,
            run,
            "plan_invalidated",
            ActionRunBoundary::Accepted,
        );
        fs::create_dir(&first_candidate).unwrap();
        let (run, _) = expect_refusal(admit(&application, &audit, false));
        assert_refused_run(
            &application,
            run,
            "plan_invalidated",
            ActionRunBoundary::Accepted,
        );
        // A Direct launch is unaffected by launcher-search changes.
        let direct = admit(&application, &fresh, false).unwrap();
        finish(&application, direct.run(), RunOutcome::Interrupted);
        fs::remove_dir(&first_candidate).unwrap();
        write_eligible_launcher(&first_candidate);
        let admitted = admit(&application, &audit, false).unwrap();
        assert_eq!(admitted.plan(), &audit);
        let recorded = view(&application, admitted.run());
        assert_eq!(recorded.action.revision, *audit.active_revision());
        assert_eq!(recorded.action.action, *audit.action());
    }

    // Test-ID: PR-TEST-0091
    // Verifies: PR-REQ-0067, PR-REQ-0068, PR-REQ-0279
    #[test]
    fn trust_guard_blocks_ordinary_admission_and_override_bypasses_only_the_guard() {
        let fixture = fixture();
        let application = PactrunApplication::open(&fixture.storage).unwrap();
        let revision = install(&application, &fixture);
        let instance = create(&application, &revision, "guarded", true);
        let pre_guard = plan(&application, &fixture, "guarded", "inspect");

        // Publish the guard through an admitted Mutate Run that fails with open
        // risk.
        let mutate = admit(
            &application,
            &plan(&application, &fixture, "guarded", "deploy"),
            false,
        )
        .unwrap();
        application
            .persistence
            .open_recovery_risk(mutate.run())
            .unwrap();
        finish(&application, mutate.run(), RunOutcome::Failed);
        let guard = application
            .persistence
            .load_instance_recovery_guard(instance.id)
            .unwrap()
            .expect("open-risk failure publishes the guard");
        let guarded_token = token(&application, instance.id);
        assert_ne!(guarded_token, instance.state_version);

        // Ordinary admission is blocked; the guard is unchanged.
        let ordinary = plan(&application, &fixture, "guarded", "inspect");
        let (run, refusal) = expect_refusal(admit(&application, &ordinary, false));
        assert_eq!(refusal, AdmissionRefusal::RecoveryGuardActive);
        assert_refused_run(
            &application,
            run,
            "recovery_guard_active",
            ActionRunBoundary::Accepted,
        );
        assert_eq!(
            application
                .persistence
                .load_instance_recovery_guard(instance.id)
                .unwrap()
                .unwrap(),
            guard
        );
        // Precedence: the guard is reported before staleness.
        let (run, refusal) = expect_refusal(admit(&application, &pre_guard, false));
        assert_eq!(refusal, AdmissionRefusal::RecoveryGuardActive);
        assert_refused_run(
            &application,
            run,
            "recovery_guard_active",
            ActionRunBoundary::Accepted,
        );

        // Legal Input management continues while guarded.
        replace_config(&application, &instance);
        assert_ne!(token(&application, instance.id), guarded_token);

        // The override admits exactly one execution and leaves the guard.
        let overridden = admit(
            &application,
            &plan(&application, &fixture, "guarded", "inspect"),
            true,
        )
        .unwrap();
        assert!(matches!(
            view(&application, overridden.run()).state,
            RunState::Running(execution) if execution.boundary == ActionRunBoundary::Admitted
        ));
        assert!(
            application
                .persistence
                .load_instance_recovery_guard(instance.id)
                .unwrap()
                .is_some()
        );

        // The override bypasses neither staleness nor a Mutate conflict.
        let (run, refusal) = expect_refusal(admit(&application, &pre_guard, true));
        assert!(matches!(refusal, AdmissionRefusal::PlanInvalidated(_)));
        assert_refused_run(
            &application,
            run,
            "plan_invalidated",
            ActionRunBoundary::Accepted,
        );
        let mutate = admit(
            &application,
            &plan(&application, &fixture, "guarded", "deploy"),
            true,
        )
        .unwrap();
        let competing = plan(&application, &fixture, "guarded", "deploy");
        let (run, refusal) = expect_refusal(admit(&application, &competing, true));
        assert_eq!(refusal, AdmissionRefusal::MutationConflict(mutate.run()));
        assert_refused_run(
            &application,
            run,
            "mutation_conflict",
            ActionRunBoundary::Accepted,
        );
        // Precedence: staleness is reported before a conflict.
        replace_config(&application, &instance);
        let (run, refusal) = expect_refusal(admit(&application, &competing, true));
        assert!(matches!(refusal, AdmissionRefusal::PlanInvalidated(_)));
        assert_refused_run(
            &application,
            run,
            "plan_invalidated",
            ActionRunBoundary::Accepted,
        );
    }

    // Test-ID: PR-TEST-0092
    // Verifies: PR-REQ-0045, PR-REQ-0278
    #[test]
    fn mutate_admission_is_exclusive_across_threads_and_processes_while_observe_coexists() {
        let fixture = fixture();
        let application = Arc::new(PactrunApplication::open(&fixture.storage).unwrap());
        let revision = install(&application, &fixture);
        let instance = create(&application, &revision, "exclusive", true);

        // Running + Admitted Mutate excludes Mutate but not Observe.
        let first = admit(
            &application,
            &plan(&application, &fixture, "exclusive", "deploy"),
            false,
        )
        .unwrap();
        let second = plan(&application, &fixture, "exclusive", "deploy");
        let (run, refusal) = expect_refusal(admit(&application, &second, false));
        assert_eq!(refusal, AdmissionRefusal::MutationConflict(first.run()));
        assert_refused_run(
            &application,
            run,
            "mutation_conflict",
            ActionRunBoundary::Accepted,
        );
        let observe = admit(
            &application,
            &plan(&application, &fixture, "exclusive", "inspect"),
            false,
        )
        .unwrap();
        let another_observe = admit(
            &application,
            &plan(&application, &fixture, "exclusive", "audit"),
            false,
        )
        .unwrap();

        // Management mutation stays admissible; the pinned payload survives.
        let payloads_before = count(&fixture, "SELECT COUNT(*) FROM managed_input_payloads");
        replace_config(&application, &instance);
        assert_eq!(
            count(&fixture, "SELECT COUNT(*) FROM managed_input_payloads"),
            payloads_before + 1
        );
        assert_eq!(
            count(
                &fixture,
                "SELECT COUNT(*) FROM run_payload_pins p \
                 JOIN managed_input_payloads m \
                 ON m.instance_id = p.instance_id AND m.payload_id = p.payload_id",
            ),
            3
        );

        // Finishing frees the slot; an Observe Run never occupied it.
        finish(&application, first.run(), RunOutcome::Interrupted);
        let third = admit(
            &application,
            &plan(&application, &fixture, "exclusive", "deploy"),
            false,
        )
        .unwrap();
        finish(&application, third.run(), RunOutcome::Cancelled);
        finish(&application, observe.run(), RunOutcome::Succeeded);
        finish(&application, another_observe.run(), RunOutcome::Succeeded);

        // An Accepted-but-not-admitted Mutate Run occupies nothing.
        let result = fixture._temporary.path().join("accepted-only.result");
        wait_worker(
            run_worker(
                &fixture,
                "admit",
                "exclusive",
                "deploy",
                Some(FaultPoint::BeforeRunAdmitCommit),
                &result,
            ),
            false,
        );
        let accepted_only = runs_for(&application, instance.id)
            .into_iter()
            .filter(|run| {
                matches!(&run.state, RunState::Running(execution) if execution.boundary == ActionRunBoundary::Accepted)
            })
            .count();
        assert_eq!(accepted_only, 1);
        let fourth = admit(
            &application,
            &plan(&application, &fixture, "exclusive", "deploy"),
            false,
        )
        .unwrap();
        finish(&application, fourth.run(), RunOutcome::Interrupted);

        // The same in-process guard serializes admission with management
        // mutations: a held guard delays acceptance itself.
        let held = application.mutation_lock(instance.id).unwrap();
        let holder = held.lock().unwrap();
        let runs_before = count(&fixture, "SELECT COUNT(*) FROM runs");
        let (started, observe_start) = mpsc::channel();
        let blocked = {
            let application = Arc::clone(&application);
            let plan = plan(&application, &fixture, "exclusive", "inspect");
            thread::spawn(move || {
                started.send(()).unwrap();
                admit(&application, &plan, false)
            })
        };
        observe_start.recv().unwrap();
        thread::sleep(Duration::from_millis(200));
        assert_eq!(count(&fixture, "SELECT COUNT(*) FROM runs"), runs_before);
        drop(holder);
        let late = blocked.join().unwrap().unwrap();
        finish(&application, late.run(), RunOutcome::Succeeded);

        // Two threads on one application admit exactly one Mutate.
        let plans = [
            plan(&application, &fixture, "exclusive", "deploy"),
            plan(&application, &fixture, "exclusive", "deploy"),
        ];
        let racers = plans
            .into_iter()
            .map(|plan| {
                let application = Arc::clone(&application);
                thread::spawn(move || admit(&application, &plan, false))
            })
            .collect::<Vec<_>>();
        let outcomes = racers
            .into_iter()
            .map(|racer| racer.join().unwrap())
            .collect::<Vec<_>>();
        let winners = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
        assert_eq!(winners, 1);
        for outcome in outcomes {
            match outcome {
                Ok(admitted) => finish(&application, admitted.run(), RunOutcome::Interrupted),
                other => {
                    let (_, refusal) = expect_refusal(other);
                    assert!(matches!(refusal, AdmissionRefusal::MutationConflict(_)));
                }
            }
        }

        // Two processes race for the same Mutate slot; only the database
        // predicate decides, and exactly one durable winner exists.
        let race = create(&application, &revision, "race", true);
        let first_result = fixture._temporary.path().join("race-a.result");
        let second_result = fixture._temporary.path().join("race-b.result");
        let racer_a = run_worker(&fixture, "admit", "race", "deploy", None, &first_result);
        let racer_b = run_worker(&fixture, "admit", "race", "deploy", None, &second_result);
        wait_worker(racer_a, true);
        wait_worker(racer_b, true);
        let reports = [
            fs::read_to_string(&first_result).unwrap(),
            fs::read_to_string(&second_result).unwrap(),
        ];
        let admitted_reports = reports
            .iter()
            .filter(|report| report.starts_with("admitted "))
            .collect::<Vec<_>>();
        let refused_reports = reports
            .iter()
            .filter(|report| report.starts_with("refused mutation_conflict "))
            .collect::<Vec<_>>();
        assert_eq!(admitted_reports.len(), 1, "{reports:?}");
        assert_eq!(refused_reports.len(), 1, "{reports:?}");
        let winner_owner = admitted_reports[0].split(' ').nth(2).unwrap();
        drop(application);
        let reopened = PactrunApplication::open(&fixture.storage).unwrap();
        let runs = runs_for(&reopened, race.id);
        assert_eq!(runs.len(), 2);
        let admitted_runs = runs
            .iter()
            .filter(|run| {
                matches!(&run.state, RunState::Running(execution) if execution.boundary == ActionRunBoundary::Admitted)
            })
            .collect::<Vec<_>>();
        assert_eq!(admitted_runs.len(), 1);
        match &admitted_runs[0].state {
            RunState::Running(execution) => assert_eq!(execution.owner.as_str(), winner_owner),
            other => panic!("unexpected state {other:?}"),
        }
        assert_eq!(count(&fixture, "SELECT COUNT(*) FROM run_revision_pins"), 1);
        let loser = runs
            .iter()
            .find(|run| run.id != admitted_runs[0].id)
            .unwrap();
        assert_refused_run(
            &reopened,
            loser.id,
            "mutation_conflict",
            ActionRunBoundary::Accepted,
        );
    }
}
