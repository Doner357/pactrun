//! Application orchestration and dependency-resolution ownership.

mod installation;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
};

pub(crate) use installation::{InstallPackResult, MigrationRelationState};

use crate::{
    authoring::{AuthoringError, SourceAcquisitionError},
    domain::{
        InputIdentity, InstanceId, InstanceName, InstanceStateVersion, InstanceSummary,
        InstanceView, LocalAlias, ReferenceLabel, RevisionCoreV1Error, RevisionIdentity,
        RevisionMetadataMutationBatch,
    },
    managed_data::{StagedFile, StagingError, StagingSession},
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
    Persistence(PersistenceError),
    InvalidInstallation(String),
    InvalidRequest(String),
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
            Self::Persistence(source) => write!(formatter, "persistence: {source}"),
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
            Self::Persistence(source) => Some(source),
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

pub(crate) struct PactrunApplication {
    persistence: PactrunPersistence,
    staging: StagingSession,
    mutation_locks: Mutex<BTreeMap<InstanceId, Arc<Mutex<()>>>>,
}

impl PactrunApplication {
    pub(crate) fn open(storage_root: impl AsRef<Path>) -> Result<Self, ApplicationError> {
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
        let staging = StagingSession::open(&root)?;
        let persistence = PactrunPersistence::open(&root)?;
        Ok(Self {
            persistence,
            staging,
            mutation_locks: Mutex::new(BTreeMap::new()),
        })
    }

    pub(crate) fn install_pack_source(
        &self,
        source_root: &Path,
        explicit_local_metadata: &RevisionMetadataMutationBatch,
    ) -> Result<InstallPackResult, ApplicationError> {
        installation::install_pack_source(
            &self.persistence,
            &self.staging,
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
            let bytes = self.staging.stage_managed_input(&mut acquisition.source)?;
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

    pub(crate) fn set_input(
        &self,
        instance: InstanceId,
        input_id: InputIdentity,
        expected: InstanceStateVersion,
        mut source: Box<dyn Read + '_>,
    ) -> Result<InstanceStateVersion, ApplicationError> {
        let lock = self.mutation_lock(instance)?;
        let _guard = lock.lock().map_err(|_| ApplicationError::LockPoisoned)?;
        let staged = self.staging.stage_managed_input(&mut source)?;
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
        let mut staged = self.staging.create_managed_output_stage()?;
        let state_version =
            self.persistence
                .export_input(instance, input_id, authorize_secret, staged.writer())?;
        staged.finish_managed_output()?;
        Ok(ExportObservation {
            state_version,
            bytes: staged,
        })
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
