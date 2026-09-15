//! Per-edge Migration ownership, reusing the shared supervisor and lease substrate.
use super::*;
use crate::{domain::*, executor::ExecutorError, persistence::AcceptanceError};
use std::collections::BTreeSet;

#[cfg(test)]
thread_local! { static ACCEPTANCE_FAULT:std::cell::Cell<u8> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
pub(crate) fn uncertain_next_migration_acceptance_for_test(committed: bool) {
    ACCEPTANCE_FAULT.with(|flag| flag.set(if committed { 2 } else { 1 }));
}

#[derive(Debug)]
pub(crate) struct MigrationOwnerState {
    run: RunId,
    owner: ExecutionOwnerSession,
    plan: MigrationExecutionPlan,
    override_guard: bool,
    cancellation: ActionCancellation,
    uncertain: bool,
    failure: Option<RunFinish>,
    inputs: BTreeMap<MigrationTargetInput, StagedFile>,
    policy: HookRuntimePolicy,
    hook: Option<Box<OwnerContinuation>>,
    hook_edge: Option<usize>,
    materialized: Option<super::materialize::MaterializedExecution>,
    hook_finish: Option<RunFinish>,
    hook_outputs: BTreeMap<InputIdentity, StagedFile>,
    secondary_failures: Vec<RunFailureRecord>,
    service_prepared: Option<(usize, ServiceMigrationEdge)>,
    target_commit: Option<TargetCommitPermit>,
}
/// Owner-memory proof only, created after the proposal receipt, zero exit/tree
/// termination, output acquisition and scratch cleanup. Never serialized.
#[derive(Debug)]
pub(crate) struct TargetCommitPermit {
    run: RunId,
    owner: ExecutionOwnerSession,
    edge: usize,
}
impl TargetCommitPermit {
    pub(crate) fn matches(&self, run: RunId, owner: &ExecutionOwnerSession, edge: usize) -> bool {
        self.run == run && &self.owner == owner && self.edge == edge
    }
}

pub(crate) struct MigrationExecutionSettings {
    pub(crate) override_guard: bool,
    pub(crate) cancellation: ActionCancellation,
    pub(crate) policy: HookRuntimePolicy,
}
impl MigrationOwnerState {
    pub(super) fn instance(&self) -> InstanceId {
        self.plan.instance()
    }
}

pub(crate) fn accept_declarative_migration(
    p: &PactrunPersistence,
    staging: &StagingSession,
    registry: &OwnerContinuationRegistry,
    plan: MigrationExecutionPlan,
    override_guard: bool,
    cancellation: ActionCancellation,
) -> Result<RunId, ExecutorError> {
    accept_migration_inputs(
        p,
        staging,
        registry,
        plan,
        MigrationExecutionSettings {
            override_guard,
            cancellation,
            policy: HookRuntimePolicy::default(),
        },
        BTreeMap::new(),
    )
}

pub(crate) fn accept_migration_inputs(
    p: &PactrunPersistence,
    staging: &StagingSession,
    registry: &OwnerContinuationRegistry,
    plan: MigrationExecutionPlan,
    settings: MigrationExecutionSettings,
    inputs: BTreeMap<MigrationTargetInput, StagedFile>,
) -> Result<RunId, ExecutorError> {
    let MigrationExecutionSettings {
        override_guard,
        cancellation,
        policy,
    } = settings;
    if inputs
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        != plan.operator_inputs()
    {
        return Err(ExecutorError::Persistence {
            run: None,
            source: PersistenceError::InvalidRunTransition(
                "Migration operator inputs do not match the compiled plan".to_owned(),
            ),
        });
    }
    let run = RunId::generate().map_err(|_| ExecutorError::Persistence {
        run: None,
        source: PersistenceError::CorruptRun("generate Migration RunId".to_owned()),
    })?;
    let mut guard = registry.begin(run);
    let owner = staging.owner();
    #[cfg(test)]
    let injection = ACCEPTANCE_FAULT.with(|flag| flag.replace(0));
    #[cfg(not(test))]
    let injection = 0;
    let result = if injection == 1 {
        Err(AcceptanceError::Uncertain {
            run,
            source: PersistenceError::DatabaseLockPoisoned,
        })
    } else {
        p.create_accepted_managed_run(
            run,
            plan.instance(),
            plan.expected_state_version(),
            &ManagedRunIdentity::Migration(plan.invocation()),
            &owner,
            &cancellation,
        )
    };
    let result = if injection == 2 && result.is_ok() {
        Err(AcceptanceError::Uncertain {
            run,
            source: PersistenceError::DatabaseLockPoisoned,
        })
    } else {
        result
    };
    let mut state = MigrationOwnerState {
        run,
        owner,
        plan,
        override_guard,
        cancellation,
        uncertain: false,
        failure: None,
        service_prepared: None,
        target_commit: None,
        inputs,
        policy,
        hook: None,
        hook_edge: None,
        materialized: None,
        hook_finish: None,
        hook_outputs: BTreeMap::new(),
        secondary_failures: Vec::new(),
    };
    match result {
        Ok(_) => {
            guard.finish(OwnerContinuation::Migration(Box::new(state)));
            Ok(run)
        }
        Err(AcceptanceError::Uncertain { source, .. }) => {
            state.uncertain = true;
            guard.finish(OwnerContinuation::Migration(Box::new(state)));
            Err(ExecutorError::Persistence {
                run: Some(run),
                source,
            })
        }
        Err(error) => {
            guard
                .registry
                .entries
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&run);
            guard.finished = true;
            match error {
                AcceptanceError::Cancelled { .. } => Err(ExecutorError::CancelledBeforeAcceptance),
                AcceptanceError::NotCommitted { source, .. } => {
                    Err(ExecutorError::Persistence { run: None, source })
                }
                AcceptanceError::Uncertain { .. } => unreachable!(),
            }
        }
    }
}

pub(super) fn advance(
    p: &PactrunPersistence,
    staging: &StagingSession,
    mut guard: ContinuationGuard<'_>,
) -> Result<bool, PersistenceError> {
    let OwnerContinuation::Migration(state) =
        guard.continuation.as_mut().expect("owned continuation")
    else {
        unreachable!()
    };
    if state.run != guard.run || state.owner != staging.owner() {
        return Err(PersistenceError::InvalidRunTransition(
            "Migration continuation owner changed".to_owned(),
        ));
    }
    let Some(view) = p.load_managed_run(state.run)? else {
        if state.uncertain {
            guard.complete();
            return Ok(true);
        }
        return Err(PersistenceError::CorruptRun(
            "accepted Migration Run disappeared".to_owned(),
        ));
    };
    if view.operation != ManagedRunIdentity::Migration(state.plan.invocation())
        || view.instance != state.plan.instance()
    {
        return Err(PersistenceError::CorruptRun(
            "Migration continuation does not match persisted invocation".to_owned(),
        ));
    }
    let RunState::Running(execution) = view.state else {
        guard.complete();
        return Ok(true);
    };
    if execution.owner != state.owner {
        return Err(PersistenceError::CorruptRun(
            "Migration execution owner changed".to_owned(),
        ));
    }
    state.uncertain = false;
    // Even cancellation must first observe termination of the retained child.
    if let Some(continuation) = state.hook.take() {
        let next = runtime::resume_registered(p, *continuation);
        state.accept_hook_progress(staging, next);
        return Ok(false);
    }
    let gate = state.cancellation.lock_acceptance_gate();
    if state.failure.is_some() || state.cancellation.is_requested() {
        if let Some(materialized) = state.materialized.take() {
            materialized.cleanup_unlaunched();
        }
        let cancelled = RunFinish {
            outcome: RunOutcome::Cancelled,
            primary_failure: None,
            secondary_failures: state.secondary_failures.clone(),
            hook_completion: None,
        };
        p.finish_run_owned(
            &state.owner,
            state.run,
            state.failure.as_ref().unwrap_or(&cancelled),
            &[],
            &mut [],
        )?;
        drop(gate);
        guard.complete();
        return Ok(true);
    }
    if execution.boundary == ActionRunBoundary::Accepted {
        let result = p.admit_migration(
            state.run,
            &state.owner,
            &state.plan,
            state.override_guard,
            &|observation| {
                crate::executor::reselect_launcher(
                    &crate::workflow::PlatformHostLauncherLookup,
                    observation,
                )
            },
        )?;
        drop(gate);
        if result.is_err() {
            guard.complete();
            return Ok(true);
        }
        return Ok(false);
    }
    let inspection = p
        .managed_run_inspection(state.run)?
        .ok_or_else(|| PersistenceError::CorruptRun("Migration inspection missing".to_owned()))?;
    let progress = inspection.migration_progress.ok_or_else(|| {
        PersistenceError::CorruptRun("admitted Migration has no checkpoint".to_owned())
    })?;
    if state
        .hook_edge
        .is_some_and(|edge| edge < progress.committed_edges)
    {
        state.hook_edge = None;
        state.hook_outputs.clear();
        state.hook_finish = None;
        state.target_commit = None;
    }
    let edge = &state.plan.edges()[progress.committed_edges];
    if state
        .service_prepared
        .as_ref()
        .is_some_and(|(index, _)| *index < progress.committed_edges)
    {
        state.service_prepared = None;
    }
    if edge.service.is_some() && state.service_prepared.is_none() {
        drop(gate);
        match p.prepare_migration_service_targets(state.run, &state.owner, progress.committed_edges)
        {
            Ok(prepared) => state.service_prepared = Some((progress.committed_edges, prepared)),
            Err(PersistenceError::ServiceStorageUnavailable(_)) => {
                state.failure = Some(RunFinish {
                    outcome: RunOutcome::Failed,
                    primary_failure: Some(RunPrimaryFailure {
                        failure: RunFailureRecord {
                            error: PactrunErrorRefV1::new(
                                "service_storage",
                                "allocation_unavailable",
                            )
                            .expect("registered error"),
                            message: String::new(),
                        },
                        step: RunFailedStep::MigrationPlan(MigrationPlanStep::EstablishSession),
                    }),
                    secondary_failures: vec![],
                    hook_completion: None,
                });
            }
            Err(error) => return Err(error),
        }
        return Ok(false);
    }
    if (state.hook_finish.is_some() || state.target_commit.is_some())
        && progress.step == MigrationPlanStep::LaunchHook
    {
        p.mark_migration_step(
            state.run,
            &state.owner,
            progress.committed_edges,
            MigrationPlanStep::AcceptCompletion,
        )?;
        return Ok(false);
    }
    if edge.launch.is_some() && state.hook_finish.is_none() && state.target_commit.is_none() {
        drop(gate);
        if state.materialized.is_none() {
            p.mark_migration_step(
                state.run,
                &state.owner,
                progress.committed_edges,
                MigrationPlanStep::EstablishSession,
            )?;
            state.hook_edge = Some(progress.committed_edges);
            match super::materialize::MaterializedExecution::create_migration(
                p,
                staging,
                state.run,
                progress.committed_edges,
                edge,
                &state.inputs,
                state.service_prepared.as_ref().map(|(_, edge)| edge),
            ) {
                Ok(materialized) => state.materialized = Some(materialized),
                Err(_) => {
                    state.accept_hook_progress(
                        staging,
                        ready_failure(
                            state.run,
                            FailureKind::SessionMaterialization,
                            Vec::new(),
                            None,
                        ),
                    );
                    return Ok(false);
                }
            }
        }
        p.mark_migration_step(
            state.run,
            &state.owner,
            progress.committed_edges,
            MigrationPlanStep::LaunchHook,
        )?;
        let next = runtime::execute_materialized(
            p,
            state.run,
            state.materialized.take().expect("retained Session"),
            state.policy,
            state.cancellation.clone(),
        );
        state.accept_hook_progress(staging, next);
        return Ok(false);
    }
    p.mark_migration_step(
        state.run,
        &state.owner,
        progress.committed_edges,
        MigrationPlanStep::PublishManagedResult,
    )?;
    let target = &state.plan.edges()[progress.committed_edges]
        .bindings
        .target()
        .content_digest;
    let files = state
        .inputs
        .iter()
        .filter(|(key, _)| key.revision == *target)
        .collect::<Vec<_>>();
    let mut readers = match files
        .iter()
        .map(|(_, file)| file.try_clone_reader())
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(readers) => readers,
        Err(_) => {
            state.failure = Some(migration_publication_failure());
            return Ok(false);
        }
    };
    let mut writes = files
        .iter()
        .zip(&mut readers)
        .map(
            |((key, file), reader)| crate::persistence::ManagedInputWrite {
                input_id: key.input.clone(),
                byte_len: file.byte_len(),
                reader,
            },
        )
        .collect::<Vec<_>>();
    let mut output_readers = match state
        .hook_outputs
        .values()
        .map(StagedFile::try_clone_reader)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(readers) => readers,
        Err(_) => {
            state.failure = Some(migration_publication_failure());
            return Ok(false);
        }
    };
    let mut output_writes = state
        .hook_outputs
        .iter()
        .zip(&mut output_readers)
        .map(
            |((input, file), reader)| crate::persistence::ManagedInputWrite {
                input_id: input.clone(),
                byte_len: file.byte_len(),
                reader,
            },
        )
        .collect::<Vec<_>>();
    let result = p.publish_migration_edge(
        state.run,
        &state.owner,
        progress.committed_edges,
        &mut crate::persistence::MigrationEdgePublication {
            target: state.target_commit.as_ref(),
            operator_inputs: &mut writes,
            hook_outputs: &mut output_writes,
            completion: state
                .hook_finish
                .as_ref()
                .and_then(|finish| finish.hook_completion.clone()),
            secondary_failures: state.secondary_failures.clone(),
        },
    );
    drop(gate);
    match result {
        Ok(_) => Ok(false),
        Err(
            PersistenceError::InvalidRunTransition(_)
            | PersistenceError::CorruptManagedInput(_)
            | PersistenceError::InvalidManagedInput(_),
        ) => {
            state.failure = Some(RunFinish {
                outcome: RunOutcome::Failed,
                primary_failure: Some(RunPrimaryFailure {
                    failure: RunFailureRecord {
                        error: PactrunErrorRefV1::new(
                            "execution",
                            "migration_publication_rejected",
                        )
                        .expect("registered error"),
                        message: String::new(),
                    },
                    step: RunFailedStep::MigrationPlan(MigrationPlanStep::PublishManagedResult),
                }),
                secondary_failures: vec![],
                hook_completion: None,
            });
            Ok(false)
        }
        Err(error) => Err(error), // Keep ownership and consult durable progress on retry.
    }
}

impl MigrationOwnerState {
    fn accept_hook_progress(&mut self, staging: &StagingSession, continuation: OwnerContinuation) {
        if let OwnerContinuation::TargetReady(facts) = continuation {
            let edge = self.hook_edge;
            let valid = facts.run == self.run
                && self
                    .service_prepared
                    .as_ref()
                    .is_some_and(|(index, service)| Some(*index) == edge && service.transform)
                && facts.proposal.outputs().iter().collect::<BTreeSet<_>>()
                    == facts
                        .outputs
                        .iter()
                        .map(|slot| &slot.handle)
                        .collect::<BTreeSet<_>>();
            let outputs = if valid {
                facts
                    .outputs
                    .iter()
                    .map(|slot| {
                        staging
                            .stage_action_output(&slot.path)
                            .map(|file| (slot.input.clone(), file))
                    })
                    .collect::<Result<BTreeMap<_, _>, _>>()
                    .ok()
            } else {
                None
            };
            let cleaned = facts.execution.cleanup().is_ok();
            if let Some(outputs) = outputs.filter(|_| cleaned) {
                self.hook_outputs = outputs;
                self.target_commit = Some(TargetCommitPermit {
                    run: self.run,
                    owner: self.owner.clone(),
                    edge: edge.expect("validated edge"),
                });
            } else {
                let mut failure = migration_publication_failure();
                if !cleaned {
                    failure.secondary_failures.push(safe_execution_failure(
                        "workspace_cleanup_failed",
                        "Migration workspace cleanup failed",
                    ));
                }
                self.failure = Some(failure);
            }
            return;
        }
        let OwnerContinuation::ReadyToFinalize(state) = continuation else {
            self.hook = Some(Box::new(continuation));
            return;
        };
        let facts = state.facts;
        let mut finish = RunFinish {
            outcome: facts.outcome,
            primary_failure: facts.primary_failure,
            secondary_failures: Vec::new(),
            hook_completion: facts
                .hook_completion
                .as_ref()
                .map(structural_hook_completion),
        };
        if let Some(primary) = &mut finish.primary_failure {
            primary.step =
                RunFailedStep::for_operation(primary.step.rank(), ManagedExecutionKind::Migration)
                    .expect("known runtime step");
        }
        if finish.outcome == RunOutcome::Succeeded {
            let outputs = facts
                .migration_outputs
                .iter()
                .map(|slot| {
                    staging
                        .stage_action_output(&slot.path)
                        .map(|file| (slot.input.clone(), file))
                })
                .collect::<Result<BTreeMap<_, _>, _>>();
            match outputs {
                Ok(outputs) if facts.completion_accepted && facts.process_terminated => {
                    self.hook_outputs = outputs
                }
                _ => {
                    let completion = finish.hook_completion.take();
                    finish = migration_publication_failure();
                    finish.hook_completion = completion;
                }
            }
        }
        if facts
            .execution
            .as_ref()
            .is_some_and(|directory| directory.cleanup().is_err())
        {
            finish.secondary_failures.push(safe_execution_failure(
                "workspace_cleanup_failed",
                "Migration workspace cleanup failed",
            ));
        }
        self.secondary_failures
            .extend(finish.secondary_failures.iter().cloned());
        finish.secondary_failures = self.secondary_failures.clone();
        if finish.outcome != RunOutcome::Succeeded {
            self.failure = Some(finish.clone());
        }
        self.hook_finish = Some(finish);
    }
}

fn migration_publication_failure() -> RunFinish {
    RunFinish {
        outcome: RunOutcome::Failed,
        primary_failure: Some(RunPrimaryFailure {
            failure: safe_execution_failure(
                "migration_publication_rejected",
                "Migration staged publication failed",
            ),
            step: RunFailedStep::MigrationPlan(MigrationPlanStep::PublishManagedResult),
        }),
        secondary_failures: vec![],
        hook_completion: None,
    }
}
