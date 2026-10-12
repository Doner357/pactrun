//! Application orchestration and dependency-resolution ownership.

mod catalog;
mod deletions;
mod installation;
pub(crate) mod migration_input;
mod migrations;
mod service_storage;
mod snapshots;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[cfg(test)]
use std::cell::RefCell;

#[cfg(test)]
thread_local! {
    static FAIL_FINALIZATION_ADVANCES_FOR_TEST: RefCell<Arc<std::sync::atomic::AtomicUsize>> = RefCell::new(Arc::new(std::sync::atomic::AtomicUsize::new(0)));
}

#[cfg(test)]
pub(crate) fn fail_next_finalization_advances_for_test(
    count: usize,
) -> Arc<std::sync::atomic::AtomicUsize> {
    let remaining = Arc::new(std::sync::atomic::AtomicUsize::new(count));
    install_finalization_faults_for_test(remaining.clone());
    remaining
}
#[cfg(test)]
pub(crate) fn finalization_faults_for_test() -> Arc<std::sync::atomic::AtomicUsize> {
    FAIL_FINALIZATION_ADVANCES_FOR_TEST.with(|r| r.borrow().clone())
}
#[cfg(test)]
pub(crate) fn install_finalization_faults_for_test(value: Arc<std::sync::atomic::AtomicUsize>) {
    FAIL_FINALIZATION_ADVANCES_FOR_TEST.with(|r| *r.borrow_mut() = value);
}

pub(crate) use installation::{InstallPackResult, MigrationRelationState};

use crate::{
    authoring::{AuthoringError, SourceAcquisitionError},
    domain::{
        ActionCompilationObservation, ActionExecutionPlan, ActionIdentity, ActionResolutionError,
        ActionV1, ExecutionOwnerSession, InputIdentity, InstanceId, InstanceName,
        InstanceStateVersion, InstanceView, InvokeAction, ManagedOutputIdentity,
        PlanCompilationError, RawParameterInput, RevisionError, RevisionIdentity,
        RevisionMetadataMutationBatch, RunFinish, RunId, RunInspectionData, RunOutcome, RunSummary,
        RunView, bind_action_parameters,
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
    Publication {
        operation: &'static str,
        destination: PathBuf,
        destination_published: bool,
        source: std::io::Error,
    },
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
    Configuration(String),
    Authoring(AuthoringError),
    SourceAcquisition(SourceAcquisitionError),
    Staging(StagingError),
    MigrationInputAcquisition(Box<migration_input::Failure>),
    Revision(RevisionError),
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
            Self::Publication {
                operation,
                destination_published,
                source,
                ..
            } => write!(
                formatter,
                "{operation}; destination_published={destination_published}: {source}"
            ),
            Self::Io { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::Configuration(message)
            | Self::InvalidInstallation(message)
            | Self::InvalidRequest(message) => formatter.write_str(message),
            Self::Authoring(source) => write!(formatter, "Pack source: {source}"),
            Self::SourceAcquisition(source) => write!(formatter, "Pack source: {source}"),
            Self::Staging(source) => write!(formatter, "staging: {source}"),
            Self::MigrationInputAcquisition(failure) => failure.fmt(formatter),
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
            Self::Io { source, .. } | Self::Publication { source, .. } => Some(source),
            Self::Authoring(source) => Some(source),
            Self::SourceAcquisition(source) => Some(source),
            Self::Staging(source) => Some(source),
            Self::MigrationInputAcquisition(failure) => Some(failure.as_ref()),
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

impl From<RevisionError> for ApplicationError {
    fn from(source: RevisionError) -> Self {
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
        crate::persistence::prepare_new_store(requested)?;
        let root = validate_supported_storage_root(requested)
            .map_err(|error| ApplicationError::Configuration(error.to_string()))?;
        for child in ["database", "runtime-content", "staging"] {
            validate_supported_storage_root(&root.join(child))
                .map_err(|error| ApplicationError::Configuration(error.to_string()))?;
        }
        let mut persistence = if collection {
            PactrunPersistence::open_for_collection(&root)?
        } else {
            PactrunPersistence::open(&root)?
        };
        persistence.configure_child_root(requested);
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

    #[cfg(test)]
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

    #[cfg(test)]
    pub(crate) fn install_pack(
        &self,
        path: &Path,
        policy: crate::domain::PackMetadataConflict,
        cancellation: &ActionCancellation,
    ) -> Result<InstallPackResult, ApplicationError> {
        self.install_named_pack(
            path,
            policy,
            &crate::domain::InstallNames::default(),
            cancellation,
        )
    }

    pub(crate) fn install_named_pack(
        &self,
        path: &Path,
        policy: crate::domain::PackMetadataConflict,
        names: &crate::domain::InstallNames,
        cancellation: &ActionCancellation,
    ) -> Result<InstallPackResult, ApplicationError> {
        let candidate = crate::pack_transport::acquire(path, self.staging()?, cancellation)
            .map_err(ApplicationError::InvalidInstallation)?;
        let empty = RevisionMetadataMutationBatch::new(Vec::new()).expect("empty metadata plan");
        installation::install_prepared(
            &self.persistence,
            candidate,
            &empty,
            policy,
            names,
            cancellation,
        )
    }

    pub(crate) fn resolve_named_revision(
        &self,
        reference: &crate::domain::LocalRevisionReference,
    ) -> Result<RevisionIdentity, ApplicationError> {
        Ok(self.persistence.resolve_named_revision(reference)?)
    }

    pub(crate) fn resolve_named_package(
        &self,
        reference: &crate::domain::LocalSelector,
    ) -> Result<crate::domain::PackageId, ApplicationError> {
        Ok(self.persistence.resolve_named_package(reference)?)
    }

    pub(crate) fn rename_package(
        &self,
        id: crate::domain::PackageId,
        name: Option<&crate::domain::LocalName>,
    ) -> Result<(), ApplicationError> {
        Ok(self.persistence.rename_package(id, name)?)
    }

    pub(crate) fn rename_revision(
        &self,
        id: &RevisionIdentity,
        name: Option<&crate::domain::LocalName>,
    ) -> Result<(), ApplicationError> {
        Ok(self.persistence.rename_revision(id, name)?)
    }

    #[cfg(test)]
    pub(crate) fn local_revision_facts(
        &self,
        id: &RevisionIdentity,
    ) -> Result<crate::domain::LocalRevisionFacts, ApplicationError> {
        Ok(self.persistence.local_revision_facts(id)?)
    }

    pub(crate) fn export_revision_pack(
        &self,
        revision: &RevisionIdentity,
        destination: &Path,
        include_metadata: bool,
        cancellation: &ActionCancellation,
    ) -> Result<(), ApplicationError> {
        let staged = self.persistence.stage_pack_export(
            revision,
            self.staging()?,
            include_metadata,
            cancellation,
        )?;
        let archive =
            crate::pack_transport::encode(&staged, include_metadata, self.staging()?, cancellation)
                .map_err(ApplicationError::InvalidInstallation)?;
        crate::output_publication::publish_pack(&archive, destination, cancellation).map_err(
            |failure| ApplicationError::Publication {
                operation: "Pack export failed",
                destination: destination.to_path_buf(),
                destination_published: failure.destination_published,
                source: failure.source,
            },
        )
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

    #[cfg(test)]
    pub(crate) fn list_instances(
        &self,
    ) -> Result<Vec<crate::domain::InstanceSummary>, ApplicationError> {
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

    #[cfg(test)]
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
    pub(crate) fn inspect_diagnostics(
        &self,
        run: RunId,
    ) -> Result<Option<crate::persistence::DiagnosticInspection>, ApplicationError> {
        Ok(self.persistence.inspect_diagnostics(run)?)
    }

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
        self.export_run_artifact_at(run, output, None, authorize_sensitive)
    }
    pub(crate) fn export_run_artifact_at(
        &self,
        run: RunId,
        output: &ManagedOutputIdentity,
        edge: Option<&crate::domain::MigrationEdgeIdentity>,
        authorize_sensitive: bool,
    ) -> Result<StagedFile, ApplicationError> {
        if !authorize_sensitive {
            return Err(ApplicationError::InvalidRequest(
                "Artifact export requires --authorize-sensitive-export".to_owned(),
            ));
        }
        let mut staged = self.staging()?.create_managed_output_stage()?;
        self.persistence
            .open_run_artifact_at(run, output, edge, staged.writer())?;
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
    pub(crate) fn delete_run_artifact_at(
        &self,
        run: RunId,
        output: &ManagedOutputIdentity,
        edge: Option<&crate::domain::MigrationEdgeIdentity>,
    ) -> Result<bool, ApplicationError> {
        Ok(self.persistence.delete_run_artifact_at(run, output, edge)?)
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
            remaining
                .borrow()
                .try_update(
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                    |n| n.checked_sub(1),
                )
                .is_ok()
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
mod action_tests;
#[cfg(test)]
mod admission_tests;
