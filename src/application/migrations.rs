//! Migration observation, bounded acquisition, and separate Run acceptance.
use super::PactrunApplication;
use crate::{
    domain::{InstanceId, MigrationCompilationObservation},
    persistence::PersistenceError,
};

impl PactrunApplication {
    pub(crate) fn accept_migration_inputs(
        &self,
        plan: crate::domain::MigrationExecutionPlan,
        override_guard: bool,
        cancellation: crate::hook::ActionCancellation,
        inputs: Vec<(crate::domain::MigrationTargetInput, std::fs::File)>,
        policy: crate::hook::HookRuntimePolicy,
    ) -> Result<crate::domain::RunId, super::ApplicationError> {
        let supplied = inputs
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<std::collections::BTreeSet<_>>();
        if supplied.len() != inputs.len() || supplied != plan.operator_inputs() {
            return Err(super::ApplicationError::InvalidRequest(
                "Migration acquisitions do not match the compiled plan".to_owned(),
            ));
        }
        let mut staged = std::collections::BTreeMap::new();
        for (key, mut source) in inputs {
            if cancellation.is_requested() {
                return Err(crate::executor::ExecutorError::CancelledBeforeAcceptance.into());
            }
            let bytes = self
                .staging()?
                .stage_managed_input(&mut source)
                .map_err(|error| {
                    super::ApplicationError::MigrationInputAcquisition(Box::new(
                        super::migration_input::Failure::new(
                            &plan,
                            &key,
                            super::migration_input::Phase::StageInput,
                            super::migration_input::Reason::from_staging(&error),
                        ),
                    ))
                })?;
            staged.insert(key, bytes);
        }
        let lock = self.mutation_lock(plan.instance())?;
        let _guard = lock
            .lock()
            .map_err(|_| super::ApplicationError::LockPoisoned)?;
        Ok(crate::hook::accept_migration_inputs(
            &self.persistence,
            self.staging()?,
            &self.continuations,
            plan,
            crate::hook::MigrationExecutionSettings {
                override_guard,
                cancellation,
                policy,
            },
            staged,
        )?)
    }
    #[cfg(test)]
    pub(crate) fn accept_declarative_migration(
        &self,
        plan: crate::domain::MigrationExecutionPlan,
        override_guard: bool,
        cancellation: crate::hook::ActionCancellation,
    ) -> Result<crate::domain::RunId, super::ApplicationError> {
        let lock = self.mutation_lock(plan.instance())?;
        let _guard = lock
            .lock()
            .map_err(|_| super::ApplicationError::LockPoisoned)?;
        Ok(crate::hook::accept_declarative_migration(
            &self.persistence,
            self.staging()?,
            &self.continuations,
            plan,
            override_guard,
            cancellation,
        )?)
    }
    pub(crate) fn verify_migration_runtime(
        &self,
        revision: &crate::domain::RevisionIdentity,
    ) -> Result<(), PersistenceError> {
        self.persistence.verify_revision_runtime_content(revision)
    }
    pub(crate) fn observe_migration_compilation(
        &self,
        instance: InstanceId,
    ) -> Result<MigrationCompilationObservation, PersistenceError> {
        self.persistence.observe_migration_compilation(instance)
    }
}
