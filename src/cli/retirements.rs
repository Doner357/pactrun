//! Human retirement surfaces; inspection never reveals native storage paths
//! unless the operator explicitly requests detached handoff.
use super::*;
use crate::domain::{
    CleanupConfirmation, DeletionMode, DeletionPhase, DeletionWork, DetachedAllocationState,
    DetachedAllocationView, InstanceId, ServiceAllocationId,
};

#[cfg(test)]
#[path = "retirement_tests.rs"]
mod tests;

pub(super) enum RetirementCommand {
    Execute {
        name: InstanceName,
        expected: Option<InstanceStateVersion>,
        mode: DeletionMode,
        options: ExecutionOptions,
    },
    Show(Selector<InstanceId>),
    Confirm(ConfirmationIntent),
    DetachedList {
        no_trunc: bool,
    },
    DetachedShow {
        id: Selector<ServiceAllocationId>,
        reveal: bool,
    },
    Discard(Selector<ServiceAllocationId>),
}

pub(super) struct ConfirmationIntent {
    instance: Selector<InstanceId>,
    attempt: Selector<RunId>,
    expected: InstanceStateVersion,
}
impl RetirementCommand {
    pub(super) fn resolve_ids(&mut self, r: &mut short_ids::Resolver<'_>) -> Result<(), CliError> {
        match self {
            Self::Show(id) => id.resolve(r)?,
            Self::Confirm(c) => {
                c.instance.resolve(r)?;
                c.attempt.resolve(r)?;
            }
            Self::DetachedShow { id, .. } | Self::Discard(id) => id.resolve(r)?,
            _ => {}
        }
        Ok(())
    }
}

fn identifier<T: FromStr>(parser: &mut Parser, name: &str) -> Result<T, CliError> {
    required_value_string(parser, name)?
        .parse()
        .map_err(|_| CliError::usage(format!("invalid {name}")))
}

pub(super) fn parse_instance(
    parser: &mut Parser,
    operation: &str,
) -> Result<RetirementCommand, CliError> {
    if operation == "deletion" {
        let operation = required_value_string(parser, "deletion command")?;
        return parse_deletion_operation(parser, &operation);
    }
    parse_execution(parser, operation)
}

pub(super) fn parse_deletion_operation(
    parser: &mut Parser,
    operation: &str,
) -> Result<RetirementCommand, CliError> {
    let instance = identifier(parser, "InstanceId")?;
    if operation == "show" {
        require_end(parser)?;
        return Ok(RetirementCommand::Show(instance));
    }
    if operation != "confirm-complete" {
        return Err(CliError::usage("unknown deletion command"));
    }
    let mut attempt = None;
    let mut expected = None;
    let mut asserted = false;
    while let Some(arg) = parser.next().map_err(lex_error)? {
        match arg {
            Arg::Long("attempt") => set_once(
                &mut attempt,
                parse_run_id(&value_string(parser, "RunId")?)?,
                "--attempt",
            )?,
            Arg::Long("if-version") => set_once(
                &mut expected,
                parse_state_version(value_string(parser, "state version")?)?,
                "--if-version",
            )?,
            Arg::Long("assert-cleanup-complete") if !asserted => asserted = true,
            _ => {
                return Err(CliError::usage(
                    "unsupported or duplicate Cleanup assertion argument",
                ));
            }
        }
    }
    if !asserted {
        return Err(CliError::usage(
            "requires --assert-cleanup-complete after external verification/repair",
        ));
    }
    Ok(RetirementCommand::Confirm(ConfirmationIntent {
        instance,
        attempt: attempt.ok_or_else(|| CliError::usage("missing --attempt"))?,
        expected: expected.ok_or_else(|| CliError::usage("missing --if-version"))?,
    }))
}

fn parse_execution(parser: &mut Parser, operation: &str) -> Result<RetirementCommand, CliError> {
    let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
    let mut expected = None;
    let mut options = ExecutionOptions {
        no_retain_hook_text: false,
        cancel_on_output_close: false,
        parameters: vec![],
        plan: false,
        recovery_override: false,
        startup_timeout_ms: None,
        action_timeout_ms: None,
        termination_grace_ms: None,
    };
    while let Some(arg) = parser.next().map_err(lex_error)? {
        match arg {
            Arg::Long("cancel-on-output-close") if !options.cancel_on_output_close => {
                options.cancel_on_output_close = true
            }
            Arg::Long("if-version") => set_once(
                &mut expected,
                parse_state_version(value_string(parser, "state version")?)?,
                "--if-version",
            )?,
            Arg::Long("plan") if !options.plan => options.plan = true,
            Arg::Long("authorize-recovery-override") if !options.recovery_override => {
                options.recovery_override = true
            }
            Arg::Long("startup-timeout-ms") => set_once(
                &mut options.startup_timeout_ms,
                parse_timeout_ms(value_string(parser, "timeout")?)?,
                "--startup-timeout-ms",
            )?,
            Arg::Long("execution-timeout-ms") => set_once(
                &mut options.action_timeout_ms,
                parse_timeout_ms(value_string(parser, "timeout")?)?,
                "--execution-timeout-ms",
            )?,
            Arg::Long("termination-grace-ms") => set_once(
                &mut options.termination_grace_ms,
                parse_timeout_ms(value_string(parser, "grace")?)?,
                "--termination-grace-ms",
            )?,
            _ => {
                return Err(CliError::usage(
                    "unsupported or duplicate retirement argument; Cleanup has no parameters or force flag",
                ));
            }
        }
    }
    Ok(RetirementCommand::Execute {
        name,
        expected,
        mode: if operation == "abandon" {
            DeletionMode::AbandonManagement
        } else {
            DeletionMode::ManagedCleanup
        },
        options,
    })
}

pub(super) fn parse_detached(parser: &mut Parser) -> Result<RetirementCommand, CliError> {
    let operation = required_value_string(parser, "detached command")?;
    if operation == "list" {
        let no_trunc = match parser.next().map_err(lex_error)? {
            None => false,
            Some(Arg::Long("no-trunc")) => {
                require_end(parser)?;
                true
            }
            _ => return Err(CliError::usage("detached list supports --no-trunc")),
        };
        return Ok(RetirementCommand::DetachedList { no_trunc });
    }
    let id = identifier(parser, "AllocationId")?;
    match operation.as_str() {
        "show" => {
            let reveal = match parser.next().map_err(lex_error)? {
                None => false,
                Some(Arg::Long("reveal-location")) => {
                    require_end(parser)?;
                    true
                }
                _ => {
                    return Err(CliError::usage(
                        "detached show supports only --reveal-location",
                    ));
                }
            };
            Ok(RetirementCommand::DetachedShow { id, reveal })
        }
        "discard" => {
            if !matches!(
                parser.next().map_err(lex_error)?,
                Some(Arg::Long("confirm-discard"))
            ) {
                return Err(CliError::usage(
                    "discard requires --confirm-discard for the exact AllocationId",
                ));
            }
            require_end(parser)?;
            Ok(RetirementCommand::Discard(id))
        }
        _ => Err(CliError::usage("unknown detached command")),
    }
}

pub(super) fn execute(
    command: RetirementCommand,
    root: &Path,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    cancellation: &ActionCancellation,
    format: presentation::Format,
) -> Result<(), CliError> {
    let readonly = matches!(
        command,
        RetirementCommand::Show(_)
            | RetirementCommand::DetachedList { .. }
            | RetirementCommand::DetachedShow { .. }
            | RetirementCommand::Execute {
                options: ExecutionOptions { plan: true, .. },
                ..
            }
    );
    if cancellation.is_requested() && !readonly {
        return Err(CliError::operation(
            "retirement cancelled before acceptance",
        ));
    }
    let app = if readonly {
        PactrunApplication::open_read_only(root)
    } else {
        PactrunApplication::open(root)
    }
    .map_err(safe_error)?;
    match command {
        RetirementCommand::Execute {
            name,
            expected,
            mode,
            options,
        } => {
            let policy = HookRuntimePolicy::from_millis(
                options.startup_timeout_ms,
                options.action_timeout_ms,
                options.termination_grace_ms.or(Some(5000)),
            )
            .map_err(CliError::usage)?;
            let intent = app
                .resolve_deletion(&name, expected, mode)
                .map_err(safe_error)?;
            let plan = app
                .compile_deletion(&intent, &launcher_search_directories())
                .map_err(safe_error)?;
            let command_name = if mode == DeletionMode::AbandonManagement {
                "instance abandon"
            } else {
                "instance delete"
            };
            if format == presentation::Format::Json
                && !options.plan
                && let DeletionWork::Cleanup(cleanup) = &plan.work
            {
                require_json_terminal_free(cleanup.hook.io.terminal)?;
            }
            if options.plan {
                let author = if matches!(&plan.work, DeletionWork::Cleanup(_)) {
                    capability_presentation::load(
                        &app,
                        &[(
                            plan.revision.clone(),
                            capability_presentation::Selection::Cleanup,
                        )],
                    )?
                } else {
                    Vec::new()
                };
                if format == presentation::Format::Json {
                    let result = RetirementPlan {
                        instance_id: plan.instance.to_string(),
                        expected_state_version: plan.expected.to_string(),
                        revision: (&plan.revision).into(),
                        work: match &plan.work {
                            DeletionWork::Abandon => "abandon_without_hook",
                            DeletionWork::NoCleanup => "no_cleanup_declared",
                            DeletionWork::FinalizationOnly { .. } => "finalization_only",
                            DeletionWork::Cleanup(_) => "cleanup_then_finalize",
                        },
                        admission: "not_attempted",
                    };
                    return presentation::render(
                        format,
                        command_name,
                        &capability_presentation::Presented {
                            value: result,
                            presentation: author,
                        },
                        stdout,
                        |_, _| unreachable!(),
                    );
                }
                let work = match plan.work {
                    DeletionWork::Abandon => "abandon_without_hook",
                    DeletionWork::NoCleanup => "no_cleanup_declared",
                    DeletionWork::FinalizationOnly { .. } => "finalization_only",
                    DeletionWork::Cleanup(_) => "cleanup_then_finalize",
                };
                writeln!(stdout, "Mode: preview\ninstance_id: {}\nexpected_state_version: {}\nrevision: {}\nwork: {work}", plan.instance, plan.expected, format_revision(&plan.revision)).map_err(io_operation)?;
                return capability_presentation::write(stdout, &author, false);
            }
            if cancellation.is_requested() {
                return Err(CliError::operation(
                    "retirement cancelled before Run acceptance",
                ));
            }
            if mode == DeletionMode::AbandonManagement {
                if format == presentation::Format::Human {
                    writeln!(stderr, "Abandon will end management and preserve remaining service data. Stop external service processes separately.").map_err(io_operation)?;
                }
            } else {
                if format == presentation::Format::Human {
                    writeln!(
                        stderr,
                        "Deletion will permanently remove this Instance's owned storage."
                    )
                    .map_err(io_operation)?;
                }
            }
            let acceptance = app.accept_deletion_plan(
                plan,
                AdmissionOptions {
                    recovery_override: options.recovery_override,
                },
                cancellation.clone(),
                policy,
            );
            let run = match acceptance {
                Ok(run) => run,
                Err(ApplicationError::Execution(ExecutorError::Persistence {
                    run: Some(run),
                    ..
                })) => {
                    // The registry retains this exact candidate and its live
                    // owner. Resolve acceptance through that continuation;
                    // diagnostics must not drop ownership or create a new Run.
                    let _ = writeln!(
                        stderr,
                        "Run {run}: acceptance uncertain; retaining owner and inspecting the exact candidate"
                    );
                    run
                }
                Err(error) => return Err(safe_error(error)),
            };
            loop {
                if let Err(error) = app.execute_ready_deletion(run) {
                    retry_or_fail(error, run, stderr)?;
                }
                match app.advance_owner_continuation(run) {
                    Ok(true) => break,
                    Ok(false) => {}
                    Err(error) => retry_or_fail(error, run, stderr)?,
                }
                thread::sleep(Duration::from_millis(100));
            }
            let inspection = app
                .managed_run_inspection(run)
                .map_err(|error| safe_error(error).with_run_context(run))?
                .ok_or_else(|| {
                    CliError::operation(
                        "Run acceptance was proven absent; no replacement Run was created",
                    )
                    .with_run_context(run)
                })?;
            if format == presentation::Format::Json {
                let result = execution_presentation::Inspection::from(&inspection);
                if matches!(&inspection.run.state, RunState::Finished(o) if o.outcome == RunOutcome::Succeeded)
                {
                    return presentation::render(
                        format,
                        command_name,
                        &result,
                        stdout,
                        |_, _| unreachable!(),
                    );
                }
                let mut error = CliError::operation(
                    "retirement Run did not succeed; inspect the typed outcome before retrying",
                );
                error.partial = Some(presentation::PartialResult::Inspection(Box::new(result)));
                return Err(error);
            }
            if !matches!(&inspection.run.state, RunState::Finished(o) if o.outcome == RunOutcome::Succeeded)
            {
                snapshots::write_run(stderr, &inspection)?;
            }
            match inspection.run.state {
                RunState::Finished(outcome) if outcome.outcome == RunOutcome::Succeeded => {
                    writeln!(stdout, "retired_instance: {}\nrun: {run}", intent.instance)
                        .map_err(io_operation)
                }
                RunState::Finished(outcome) => Err(CliError::operation(format!(
                    "Run {run} finished with {}; inspect instance deletion show {} before retrying",
                    format_outcome(outcome.outcome),
                    intent.instance
                ))),
                RunState::Running(_) => Err(CliError::operation(
                    "retirement Run has not durably finished",
                )),
            }
        }
        RetirementCommand::Show(id) => {
            let id = id.full();
            let entry = app.inspect_retirement(id).map_err(safe_error)?;
            if format == presentation::Format::Json {
                return presentation::render(
                    format,
                    "instance deletion show",
                    &RetirementInspection::from(&entry),
                    stdout,
                    |_, _| unreachable!(),
                );
            }
            let runs = entry.runs;
            let live = entry.live;
            writeln!(stdout, "instance_id: {id}\nmanaged: {}", live.is_some())
                .map_err(io_operation)?;
            if let Some(live) = live {
                writeln!(stdout, "state_version: {}", live.state_version).map_err(io_operation)?;
            }
            if let Some(obligation) = entry.obligation {
                let phase = match obligation.phase {
                    DeletionPhase::LaunchAuthorized => "launch_authorized",
                    DeletionPhase::ResultUnresolved => "cleanup_unresolved",
                    DeletionPhase::FinalizationAuthorized => "finalization_authorized",
                };
                writeln!(
                    stdout,
                    "attempt: {}\nobligation: {phase}",
                    obligation.attempt
                )
                .map_err(io_operation)?;
            } else {
                writeln!(stdout, "obligation: none").map_err(io_operation)?;
            }
            for run in runs {
                if matches!(
                    run.operation,
                    crate::domain::ManagedRunIdentity::Deletion { .. }
                ) {
                    snapshots::write_summary(stdout, &run)?;
                }
            }
            Ok(())
        }
        RetirementCommand::Confirm(confirmation) => {
            let confirmation = CleanupConfirmation {
                instance: confirmation.instance.full(),
                attempt: confirmation.attempt.full(),
                expected: confirmation.expected,
            };
            let version = app
                .confirm_cleanup_completion(confirmation)
                .map_err(safe_error)?;
            if format == presentation::Format::Json {
                return presentation::render(
                    format,
                    "instance deletion confirm-complete",
                    &Confirmation {
                        state_version: version.to_string(),
                        authority: "operator_confirmed",
                    },
                    stdout,
                    |_, _| unreachable!(),
                );
            }
            writeln!(stdout, "state_version: {version}\nauthority: operator_confirmed\nOnly finalization is authorized; the original Run outcome and trust guard are unchanged.").map_err(io_operation)
        }
        RetirementCommand::DetachedList { no_trunc } => {
            let allocations = app.list_detached_allocations().map_err(safe_error)?;
            if format == presentation::Format::Json {
                let result = DetachedList {
                    items: allocations.iter().map(Detached::from).collect(),
                };
                return presentation::render(
                    format,
                    "service-storage detached list",
                    &result,
                    stdout,
                    |_, _| unreachable!(),
                );
            }
            if allocations.is_empty() {
                writeln!(stdout, "No detached allocations.").map_err(io_operation)?;
            }
            let ids = short_ids::labels(
                &app,
                crate::domain::CatalogIdentityKind::Allocation,
                allocations
                    .iter()
                    .map(|a| a.allocation.to_string())
                    .collect(),
                no_trunc,
            )?;
            for (allocation, id) in allocations.into_iter().zip(ids) {
                write_detached(stdout, &allocation, &id)?;
            }
            Ok(())
        }
        RetirementCommand::DetachedShow { id, reveal } => {
            let id = id.full();
            let allocation = app
                .detached_allocation(id)
                .map_err(safe_error)?
                .ok_or_else(|| CliError::operation("detached AllocationId not found"))?;
            if format == presentation::Format::Json {
                let result = DetachedInspection {
                    allocation: (&allocation).into(),
                    locations: if reveal {
                        Some(
                            app.detached_handoff(id)
                                .map_err(safe_error)?
                                .iter()
                                .map(|l| Location {
                                    native_location: l.path.as_path().into(),
                                    original_relative_location: l
                                        .original_relative_path
                                        .as_path()
                                        .into(),
                                })
                                .collect(),
                        )
                    } else {
                        None
                    },
                };
                return presentation::render(
                    format,
                    "service-storage detached show",
                    &result,
                    stdout,
                    |_, _| unreachable!(),
                );
            }
            write_detached(stdout, &allocation, &id.to_string())?;
            if reveal {
                let locations = app.detached_handoff(id).map_err(safe_error)?;
                if locations.len() > 1 {
                    writeln!(stderr,"Partial retirement left data in multiple locations. Review all listed locations before recovery or disposal.").map_err(io_operation)?;
                }
                for location in locations {
                    writeln!(
                        stdout,
                        "native_location: {}\noriginal_relative_location: {:?}",
                        location.path.display(),
                        location.original_relative_path
                    )
                    .map_err(io_operation)?;
                }
            }
            Ok(())
        }
        RetirementCommand::Discard(id) => {
            let id = id.full();
            if format == presentation::Format::Human {
                writeln!(stderr, "Discard will permanently delete this allocation. Stop any service using it first.").map_err(io_operation)?;
            }
            let changed = app
                .discard_detached_allocation(id, true)
                .map_err(safe_error)?;
            if format == presentation::Format::Json {
                return presentation::render(
                    format,
                    "service-storage detached discard",
                    &Discard {
                        allocation_id: id.to_string(),
                        state: "discarded",
                        new_completion: changed,
                    },
                    stdout,
                    |_, _| unreachable!(),
                );
            }
            writeln!(
                stdout,
                "allocation: {id}\nstate: discarded\nnew_completion: {changed}"
            )
            .map_err(io_operation)
        }
    }
}

fn write_detached(
    out: &mut dyn Write,
    allocation: &DetachedAllocationView,
    id: &str,
) -> Result<(), CliError> {
    let state = match allocation.state {
        DetachedAllocationState::Preserved => "preserved",
        DetachedAllocationState::DiscardPending => "discard_pending",
        DetachedAllocationState::Discarded => "discarded",
    };
    writeln!(out, "allocation: {id}\nformer_instance: {}\nstate: {state}\norigin_revision: {}\norigin_storage: {}", allocation.instance, format_revision(&allocation.origin_revision), allocation.origin_storage.as_str()).map_err(io_operation)
}

#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct RetirementPlan {
    instance_id: String,
    expected_state_version: String,
    revision: presentation::Revision,
    work: &'static str,
    admission: &'static str,
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Obligation {
    attempt_run_id: String,
    phase: &'static str,
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct RetirementInspection {
    instance_id: String,
    managed: bool,
    state_version: Option<String>,
    obligation: Option<Obligation>,
    runs: Vec<execution_presentation::Run>,
}
impl From<&crate::domain::RetirementCatalogEntry> for RetirementInspection {
    fn from(entry: &crate::domain::RetirementCatalogEntry) -> Self {
        Self {
            instance_id: entry.history.id.to_string(),
            managed: entry.live.is_some(),
            state_version: entry.live.as_ref().map(|v| v.state_version.to_string()),
            obligation: entry.obligation.as_ref().map(|o| Obligation {
                attempt_run_id: o.attempt.to_string(),
                phase: match o.phase {
                    DeletionPhase::LaunchAuthorized => "launch_authorized",
                    DeletionPhase::ResultUnresolved => "cleanup_unresolved",
                    DeletionPhase::FinalizationAuthorized => "finalization_authorized",
                },
            }),
            runs: entry.runs.iter().map(Into::into).collect(),
        }
    }
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Confirmation {
    state_version: String,
    authority: &'static str,
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Detached {
    allocation_id: String,
    former_instance_id: String,
    state: &'static str,
    origin_revision: presentation::Revision,
    origin_storage_id: String,
}
impl From<&DetachedAllocationView> for Detached {
    fn from(a: &DetachedAllocationView) -> Self {
        Self {
            allocation_id: a.allocation.to_string(),
            former_instance_id: a.instance.to_string(),
            state: match a.state {
                DetachedAllocationState::Preserved => "preserved",
                DetachedAllocationState::DiscardPending => "discard_pending",
                DetachedAllocationState::Discarded => "discarded",
            },
            origin_revision: (&a.origin_revision).into(),
            origin_storage_id: a.origin_storage.as_str().into(),
        }
    }
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct DetachedList {
    items: Vec<Detached>,
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Location {
    native_location: presentation::NativePath,
    original_relative_location: presentation::NativePath,
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct DetachedInspection {
    allocation: Detached,
    locations: Option<Vec<Location>>,
}
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Discard {
    allocation_id: String,
    state: &'static str,
    new_completion: bool,
}

fn retry_or_fail(
    error: ApplicationError,
    run: RunId,
    stderr: &mut dyn Write,
) -> Result<(), CliError> {
    if matches!(
        error,
        ApplicationError::Persistence(
            crate::persistence::PersistenceError::Sqlite { .. }
                | crate::persistence::PersistenceError::Io { .. }
                | crate::persistence::PersistenceError::DatabaseLockPoisoned
        )
    ) {
        let _ = writeln!(
            stderr,
            "Run {run}: saving execution state; retrying storage access"
        );
        thread::sleep(Duration::from_secs(1));
        Ok(())
    } else {
        Err(safe_error(error).with_run_context(run))
    }
}

fn safe_error(error: ApplicationError) -> CliError {
    use crate::{domain::DeletionPlanError, persistence::PersistenceError};
    let mut result = CliError::operation("");
    preserve_error_facts(&error, &mut result);
    if result.details.deletion_obligation.is_some() {
        return result;
    }
    let message = match error {
        ApplicationError::DeletionCompilation(DeletionPlanError::UnresolvedAttempt(run)) => {
            return CliError::operation(format!(
                "Cleanup attempt {run} is unresolved; reconcile lost owners, then explicitly confirm completion after external verification/repair, or abandon management"
            ));
        }
        ApplicationError::DeletionCompilation(DeletionPlanError::MissingRequirement(_)) => {
            "Cleanup requirements are missing; inspect bindings and retained resources, repair them, or explicitly abandon management"
        }
        ApplicationError::Persistence(PersistenceError::InvalidRunTransition(reason)) => {
            return CliError::operation(reason);
        }
        ApplicationError::Persistence(PersistenceError::ServiceStorageUnavailable(reason)) => {
            reason
        }
        _ => {
            "retirement operation could not be completed safely; inspect Instance identity, state version, Run history and storage health before retrying"
        }
    };
    result.message = message.into();
    result
}
