//! Restore terminal ownership reuses the shared runtime and V5 atomic boundary.
use super::*;
use crate::domain::{ManagedExecutionKind, RunFailedStep, RunFinish, RunOutcome, RunState};

#[derive(Debug)]
pub(crate) struct RestoreFinalization {
    facts: RuntimeTerminalFacts,
    artifacts: super::artifacts::ArtifactPreparation,
    cleanup_attempted: bool,
    cleanup_failed: bool,
    publication_failure: Option<&'static str>,
}

pub(super) fn terminal(mut facts: RuntimeTerminalFacts) -> OwnerContinuation {
    if let Some(primary) = &mut facts.primary_failure {
        primary.step = RunFailedStep::for_operation(
            primary.step.rank(),
            ManagedExecutionKind::SnapshotRestore,
        )
        .expect("known step");
    }
    OwnerContinuation::RestoreFinalization(Box::new(RestoreFinalization {
        facts,
        artifacts: Default::default(),
        cleanup_attempted: false,
        cleanup_failed: false,
        publication_failure: None,
    }))
}

pub(super) fn advance(
    p: &PactrunPersistence,
    staging: &StagingSession,
    mut guard: ContinuationGuard<'_>,
) -> Result<bool, PersistenceError> {
    let Some(OwnerContinuation::RestoreFinalization(state)) = guard.continuation.as_mut() else {
        unreachable!()
    };
    if matches!(
        p.load_managed_run(state.facts.run)?.map(|v| v.state),
        Some(RunState::Finished(_))
    ) {
        guard.complete();
        return Ok(true);
    }
    if state.facts.process_started && !state.facts.process_terminated {
        return Err(PersistenceError::InvalidRunTransition(
            "Restore process is still live".to_owned(),
        ));
    }
    state.artifacts.prepare(staging, &state.facts);
    if !state.cleanup_attempted {
        state.cleanup_attempted = true;
        if let Some(execution) = &state.facts.execution {
            state.cleanup_failed = execution.cleanup().is_err();
        }
    }
    let mut artifact_readers = state.artifacts.readers();
    let mut finish = RunFinish {
        outcome: state.facts.outcome,
        primary_failure: state.facts.primary_failure.clone(),
        secondary_failures: Vec::new(),
        hook_completion: state
            .facts
            .hook_completion
            .as_ref()
            .map(structural_hook_completion),
    };
    if let Some(message) = state.publication_failure {
        finish.outcome = RunOutcome::Failed;
        finish.primary_failure = Some(crate::domain::RunPrimaryFailure {
            cause: None,
            failure: safe_execution_failure("managed_output_publication_failed", message),
            step: RunFailedStep::SnapshotPlan(
                crate::domain::SnapshotPlanStep::PublishManagedResult,
            ),
        });
    }
    if state.cleanup_failed {
        finish.secondary_failures.push(safe_execution_failure(
            "workspace_cleanup_failed",
            "Restore execution workspace cleanup failed",
        ));
    }
    state.artifacts.apply_failure(
        &mut finish,
        RunFailedStep::SnapshotPlan(crate::domain::SnapshotPlanStep::PublishManagedResult),
    );
    let mut artifact_writes = state.artifacts.writes(&mut artifact_readers);
    let result = if finish.outcome == RunOutcome::Succeeded {
        state
            .facts
            .trace
            .measure(crate::domain::TracePhase::ResultCommit, || {
                p.publish_restore_with_outputs(
                    &staging.owner(),
                    state.facts.run,
                    &finish,
                    crate::persistence::RunArtifactSubmission {
                        submitted: &state.facts.submitted_outputs,
                        artifacts: &mut artifact_writes,
                    },
                )
            })
    } else {
        state
            .facts
            .trace
            .measure(crate::domain::TracePhase::ResultCommit, || {
                p.finish_run_owned(
                    &staging.owner(),
                    state.facts.run,
                    &finish,
                    &state.facts.submitted_outputs,
                    &mut artifact_writes,
                )
            })
    };
    match result {
        Ok(_) => {
            guard.complete();
            Ok(true)
        }
        Err(PersistenceError::CorruptSnapshot(_)) => {
            state.publication_failure =
                Some("Restore Snapshot content failed integrity verification");
            Ok(false)
        }
        Err(PersistenceError::SnapshotCodec(
            crate::snapshot_integrity::SnapshotCodecError::Capability(_),
        )) => {
            state.publication_failure =
                Some("Restore exceeds a structural or representation capability");
            Ok(false)
        }
        Err(error) => Err(error),
    }
}
