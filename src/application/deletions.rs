//! Instance retirement orchestration. Inspection/compilation has no side effects.
// The production CLI is activated only after the complete destructive path.
use super::{ApplicationError, PactrunApplication};
use crate::domain::*;

impl PactrunApplication {
    pub(crate) fn list_detached_allocations(
        &self,
    ) -> Result<Vec<DetachedAllocationView>, ApplicationError> {
        Ok(self.persistence.list_detached_allocations()?)
    }

    pub(crate) fn detached_allocation(
        &self,
        id: ServiceAllocationId,
    ) -> Result<Option<DetachedAllocationView>, ApplicationError> {
        Ok(self.persistence.detached_allocation(id)?)
    }

    pub(crate) fn detached_handoff(
        &self,
        id: ServiceAllocationId,
    ) -> Result<Vec<crate::retirement_fs::HandoffLocation>, ApplicationError> {
        Ok(self.persistence.detached_handoff(id)?)
    }

    pub(crate) fn discard_detached_allocation(
        &self,
        id: ServiceAllocationId,
        confirmed: bool,
    ) -> Result<bool, ApplicationError> {
        Ok(self
            .persistence
            .discard_detached_allocation(id, confirmed)?)
    }
    pub(crate) fn accept_deletion_plan(
        &self,
        plan: DeletionPlan,
        options: crate::executor::AdmissionOptions,
        cancellation: crate::hook::ActionCancellation,
        policy: crate::hook::HookRuntimePolicy,
    ) -> Result<RunId, ApplicationError> {
        let lock = self.mutation_lock(plan.instance)?;
        let _guard = lock.lock().map_err(|_| ApplicationError::LockPoisoned)?;
        Ok(crate::hook::accept_deletion(
            &self.persistence,
            self.staging()?,
            &self.continuations,
            plan,
            options,
            cancellation,
            policy,
        )?)
    }

    pub(crate) fn execute_ready_deletion(&self, run: RunId) -> Result<bool, ApplicationError> {
        Ok(crate::hook::execute_ready_deletion(
            &self.persistence,
            self.staging()?,
            &self.continuations,
            run,
        )?)
    }

    pub(crate) fn resolve_deletion(
        &self,
        name: &InstanceName,
        expected: Option<InstanceStateVersion>,
        mode: DeletionMode,
    ) -> Result<DeleteInstance, ApplicationError> {
        let instance = self
            .persistence
            .resolve_instance_name(name)?
            .ok_or_else(|| {
                crate::persistence::PersistenceError::MissingInstance(name.as_str().to_owned())
            })?;
        let view = self
            .persistence
            .load_instance_by_id(instance)?
            .ok_or_else(|| {
                crate::persistence::PersistenceError::MissingInstance(instance.to_string())
            })?;
        Ok(DeleteInstance {
            instance,
            expected: expected.unwrap_or(view.state_version),
            mode,
        })
    }

    pub(crate) fn compile_deletion(
        &self,
        intent: &DeleteInstance,
        search: &[std::path::PathBuf],
    ) -> Result<DeletionPlan, ApplicationError> {
        crate::workflow::compile_deletion(
            intent,
            self.persistence.observe_deletion(intent)?,
            &crate::workflow::PlatformHostLauncherLookup,
            search,
        )
    }

    #[cfg(test)]
    pub(crate) fn inspect_deletion(
        &self,
        instance: InstanceId,
    ) -> Result<Option<DeletionObligation>, ApplicationError> {
        Ok(self.persistence.deletion_obligation(instance)?)
    }

    pub(crate) fn confirm_cleanup_completion(
        &self,
        confirmation: CleanupConfirmation,
    ) -> Result<InstanceStateVersion, ApplicationError> {
        Ok(self.persistence.confirm_cleanup_completion(confirmation)?)
    }
}
