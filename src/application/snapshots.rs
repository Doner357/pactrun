//! Snapshot lifecycle services and structural inspection for the human CLI.
#![allow(dead_code)]
use super::{ApplicationError, PactrunApplication};
use crate::{
    domain::{InstanceName, SnapshotId},
    persistence::{
        PersistenceError, SnapshotImportReceipt, SnapshotInspection, SnapshotVerification,
    },
    snapshot_bundle::{self, ValidatedSnapshotBundle},
};
use std::{io::Write, path::Path};

impl PactrunApplication {
    /// Structural, read-only preflight; no pin or launch eligibility is promised.
    pub(crate) fn create_restore_definition(
        &self,
        revision: &crate::domain::RevisionIdentity,
        snapshot: SnapshotId,
    ) -> Result<(Vec<crate::domain::ParameterV1>, crate::domain::HookV1), ApplicationError> {
        let inspection = self.inspect_snapshot(snapshot)?;
        if &inspection.producer != revision {
            return Err(crate::domain::SnapshotPlanError::Invalid(
                "Restore requires the exact Snapshot producer RevisionIdentity",
            )
            .into());
        }
        inspection
            .restore_capability
            .map_err(PersistenceError::from)?;
        let stored = self
            .persistence
            .load_revision(revision)?
            .ok_or(PersistenceError::MissingRevision(revision.clone()))?;
        let (parameters, hook) = crate::domain::snapshot_hook(
            stored.content.core.common(),
            crate::domain::SnapshotOperation::Restore(snapshot),
        )?;
        Ok((parameters.to_vec(), hook.clone()))
    }

    pub(crate) fn snapshot_definition(
        &self,
        name: &InstanceName,
        operation: crate::domain::SnapshotOperation,
    ) -> Result<
        (
            crate::domain::InstanceId,
            Vec<crate::domain::ParameterV1>,
            crate::domain::HookV1,
        ),
        ApplicationError,
    > {
        let id = self
            .persistence
            .resolve_instance_name(name)?
            .ok_or_else(|| PersistenceError::MissingInstance(name.as_str().to_owned()))?;
        let instance = self
            .persistence
            .load_instance_by_id(id)?
            .ok_or_else(|| PersistenceError::MissingInstance(name.as_str().to_owned()))?;
        let revision = self
            .persistence
            .load_revision(&instance.active_revision)?
            .ok_or(PersistenceError::MissingRevision(instance.active_revision))?;
        let (parameters, hook) =
            crate::domain::snapshot_hook(revision.content.core.common(), operation)?;
        Ok((id, parameters.to_vec(), hook.clone()))
    }

    pub(crate) fn managed_run_inspection(
        &self,
        run: crate::domain::RunId,
    ) -> Result<Option<crate::domain::ManagedRunInspectionData>, ApplicationError> {
        Ok(self.persistence.managed_run_inspection(run)?)
    }

    pub(crate) fn list_managed_runs(
        &self,
        instance: crate::domain::InstanceId,
    ) -> Result<Vec<crate::domain::ManagedRunView>, ApplicationError> {
        Ok(self.persistence.list_managed_runs(instance)?)
    }
    pub(crate) fn import_snapshot_file(
        &self,
        source: &Path,
    ) -> Result<SnapshotImportReceipt, ApplicationError> {
        if source.as_os_str() == "-" {
            return Err(ApplicationError::InvalidRequest(
                "Snapshot bundle stdin is not supported".to_owned(),
            ));
        }
        #[cfg(windows)]
        if !std::fs::metadata(source)
            .map_err(|source| ApplicationError::Io {
                operation: "inspect Snapshot input path",
                source,
            })?
            .is_file()
        {
            return Err(ApplicationError::InvalidRequest(
                "Snapshot bundle input must be a regular file".to_owned(),
            ));
        };
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // Qualify the opened object without waiting for a FIFO writer.
            options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
        }
        let mut file = options
            .open(source)
            .map_err(|source| ApplicationError::Io {
                operation: "open Snapshot bundle input",
                source,
            })?;
        let metadata = file.metadata().map_err(|source| ApplicationError::Io {
            operation: "inspect Snapshot bundle input",
            source,
        })?;
        if !metadata.is_file() {
            return Err(ApplicationError::InvalidRequest(
                "Snapshot bundle input must be a regular file".to_owned(),
            ));
        };
        crate::domain::SnapshotCapability::BundleBytes
            .check(metadata.len())
            .map_err(PersistenceError::from)?;
        let stage = snapshot_bundle::stage_bundle(self.staging()?, &mut file)
            .map_err(PersistenceError::from)?;
        let mut bundle = ValidatedSnapshotBundle::read(stage).map_err(PersistenceError::from)?;
        Ok(self.persistence.import_snapshot_bundle(&mut bundle)?)
    }
    pub(crate) fn export_snapshot_file(
        &self,
        id: SnapshotId,
        destination: &Path,
        authorize_sensitive: bool,
        warnings: &mut dyn Write,
    ) -> Result<(), ApplicationError> {
        if !authorize_sensitive {
            return Err(PersistenceError::UnauthorizedSnapshotExport.into());
        };
        if destination.as_os_str() == "-" {
            return Err(ApplicationError::InvalidRequest(
                "Snapshot bundle stdout is not supported".to_owned(),
            ));
        }
        warnings
            .write_all(b"Warning: Snapshot bundle is unencrypted and may contain sensitive data.\n")
            .map_err(|source| ApplicationError::Io {
                operation: "write sensitive Snapshot export warning",
                source,
            })?;
        let stage = self.persistence.export_snapshot_stage(id, true)?;
        crate::output_publication::publish(&stage, destination).map_err(|source| {
            ApplicationError::Io {
                operation: "publish Snapshot bundle without replacement",
                source,
            }
        })?;
        Ok(())
    }
    pub(crate) fn inspect_snapshot(
        &self,
        id: SnapshotId,
    ) -> Result<SnapshotInspection, ApplicationError> {
        Ok(self.persistence.inspect_snapshot(id)?)
    }
    pub(crate) fn verify_snapshot(
        &self,
        id: SnapshotId,
    ) -> Result<SnapshotVerification, ApplicationError> {
        Ok(self.persistence.verify_snapshot(id)?)
    }
    pub(crate) fn list_snapshots(
        &self,
        origin: Option<&InstanceName>,
    ) -> Result<Vec<SnapshotInspection>, ApplicationError> {
        let origin = origin
            .map(|name| {
                self.persistence.resolve_instance_name(name).and_then(|id| {
                    id.ok_or_else(|| PersistenceError::MissingInstance(name.as_str().to_owned()))
                })
            })
            .transpose()?;
        Ok(self.persistence.list_snapshots(origin)?)
    }
}
