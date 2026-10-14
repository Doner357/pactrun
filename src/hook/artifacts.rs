//! Shared ordinary-output acquisition; specialized state publication stays with
//! each operation owner. Staging never claims to undo external side effects.
use super::{LiveOutputSlot, PreparedOutput, RuntimeTerminalFacts, safe_execution_failure};
use crate::{
    domain::{ManagedOutputIdentity, RunFailedStep, RunFinish, RunOutcome, RunPrimaryFailure},
    managed_data::StagingSession,
    persistence::RunArtifactWrite,
};
use std::{collections::BTreeMap, fs::File};

pub(super) fn stage_submitted(
    staging: &StagingSession,
    facts: &RuntimeTerminalFacts,
) -> Result<Vec<PreparedOutput>, ()> {
    stage_outputs(
        staging,
        facts.ordinary_outputs_accepted,
        &facts.submitted_outputs,
        &facts.outputs,
        &facts.trace,
    )
}

fn stage_outputs(
    staging: &StagingSession,
    accepted: bool,
    submitted: &[ManagedOutputIdentity],
    slots: &[LiveOutputSlot],
    trace: &std::sync::Arc<crate::trace::Trace>,
) -> Result<Vec<PreparedOutput>, ()> {
    if !accepted || submitted.is_empty() {
        return Ok(Vec::new());
    }
    let mut sources = slots
        .iter()
        .map(|slot| (&slot.output, &slot.path))
        .collect::<BTreeMap<_, _>>();
    if sources.len() != slots.len() {
        return Err(());
    }
    let mut prepared = Vec::new();
    for output in submitted {
        let path = sources.remove(output).ok_or(())?;
        let bytes = trace
            .measure(crate::domain::TracePhase::OutputValidation, || {
                staging.stage_action_output(path)
            })
            .map_err(|_| ())?;
        prepared.push(PreparedOutput {
            output: output.clone(),
            bytes,
        });
    }
    prepared.sort_by(|a, b| a.output.cmp(&b.output));
    Ok(prepared)
}

#[derive(Debug, Default)]
pub(super) struct ArtifactPreparation {
    prepared: Option<Vec<PreparedOutput>>,
    failed: bool,
}
impl ArtifactPreparation {
    pub(super) fn failed(&self) -> bool {
        self.failed
    }
    pub(super) fn prepare(&mut self, staging: &StagingSession, facts: &RuntimeTerminalFacts) {
        self.prepare_submission(
            staging,
            facts.ordinary_outputs_accepted,
            &facts.submitted_outputs,
            &facts.outputs,
            &facts.trace,
        );
    }
    pub(super) fn prepare_submission(
        &mut self,
        staging: &StagingSession,
        accepted: bool,
        submitted: &[ManagedOutputIdentity],
        slots: &[LiveOutputSlot],
        trace: &std::sync::Arc<crate::trace::Trace>,
    ) {
        if self.prepared.is_some() {
            return;
        }
        match stage_outputs(staging, accepted, submitted, slots, trace) {
            Ok(outputs) => self.prepared = Some(outputs),
            Err(()) => {
                self.failed = true;
                self.prepared = Some(Vec::new());
            }
        }
    }
    pub(super) fn readers(&mut self) -> Vec<File> {
        let readers = self
            .prepared
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|output| output.bytes.try_clone_reader())
            .collect::<Result<Vec<_>, _>>();
        match readers {
            Ok(readers) => readers,
            Err(_) => {
                self.failed = true;
                self.prepared = Some(Vec::new());
                Vec::new()
            }
        }
    }
    pub(super) fn writes<'a>(&'a self, readers: &'a mut [File]) -> Vec<RunArtifactWrite<'a>> {
        self.prepared
            .as_deref()
            .unwrap_or_default()
            .iter()
            .zip(readers)
            .map(|(output, reader)| RunArtifactWrite {
                output: output.output.clone(),
                byte_len: output.bytes.byte_len(),
                reader,
            })
            .collect()
    }
    pub(super) fn apply_failure(&self, finish: &mut RunFinish, step: RunFailedStep) {
        if !self.failed {
            return;
        }
        let failure = safe_execution_failure(
            "managed_output_publication_failed",
            "Managed output publication failed",
        );
        if finish
            .primary_failure
            .as_ref()
            .is_some_and(|primary| primary.failure == failure)
            || finish.secondary_failures.contains(&failure)
        {
            return;
        }
        if finish.outcome == RunOutcome::Succeeded && finish.primary_failure.is_none() {
            finish.outcome = RunOutcome::Failed;
            finish.primary_failure = Some(RunPrimaryFailure {
                cause: None,
                failure,
                step,
            });
        } else {
            finish.secondary_failures.push(failure);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0721
    // Verifies: PR-REQ-0391
    #[test]
    fn publication_failure_is_idempotent_and_preserves_locked_outcomes() {
        let prepared = ArtifactPreparation {
            failed: true,
            ..Default::default()
        };
        let step =
            RunFailedStep::SnapshotPlan(crate::domain::SnapshotPlanStep::PublishManagedResult);
        for outcome in [
            RunOutcome::Succeeded,
            RunOutcome::Failed,
            RunOutcome::Cancelled,
            RunOutcome::TimedOut,
        ] {
            let mut finish = RunFinish {
                outcome,
                primary_failure: None,
                secondary_failures: Vec::new(),
                hook_completion: None,
            };
            prepared.apply_failure(&mut finish, step);
            if outcome == RunOutcome::Succeeded {
                assert_eq!(finish.outcome, RunOutcome::Failed);
                assert!(finish.primary_failure.is_some());
                assert!(finish.secondary_failures.is_empty());
            } else {
                assert_eq!(finish.outcome, outcome);
                assert!(finish.primary_failure.is_none());
                assert_eq!(finish.secondary_failures.len(), 1);
            }
            let once = finish.clone();
            prepared.apply_failure(&mut finish, step);
            assert_eq!(finish, once);
        }
    }
}
