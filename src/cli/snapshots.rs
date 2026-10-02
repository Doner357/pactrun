//! Human Snapshot surfaces. Projections never serialize manifests or payloads.
use super::*;
use crate::domain::{
    ManagedRunIdentity, ManagedRunInspectionData, ManagedRunView, SnapshotId, SnapshotIntent,
    SnapshotOperation, SnapshotPlanError, SnapshotRelationalVerification,
};
use crate::persistence::{PersistenceError, SnapshotInspection};

pub(super) enum SnapshotCommand {
    Delete(Selector<SnapshotId>),
    Execute {
        name: InstanceName,
        operation: Operation,
        options: ExecutionOptions,
    },
    List {
        name: Option<InstanceName>,
        no_trunc: bool,
    },
    Show(Selector<SnapshotId>),
    Verify(Selector<SnapshotId>),
    Import(PathBuf),
    Export {
        id: Selector<SnapshotId>,
        path: PathBuf,
        authorized: bool,
    },
}

pub(super) struct CreateRestoreCommand {
    pub(super) name: InstanceName,
    pub(super) revision: RevisionReference,
    pub(super) snapshot: Selector<SnapshotId>,
    pub(super) options: ExecutionOptions,
}

pub(super) enum Operation {
    Capture,
    Restore(Selector<SnapshotId>),
}
impl Operation {
    fn full(self) -> SnapshotOperation {
        match self {
            Self::Capture => SnapshotOperation::Capture,
            Self::Restore(id) => SnapshotOperation::Restore(id.full()),
        }
    }
}
impl SnapshotCommand {
    pub(super) fn resolve_ids(&mut self, r: &mut short_ids::Resolver<'_>) -> Result<(), CliError> {
        match self {
            Self::Delete(id)
            | Self::Show(id)
            | Self::Verify(id)
            | Self::Export { id, .. }
            | Self::Execute {
                operation: Operation::Restore(id),
                ..
            } => id.resolve(r)?,
            _ => {}
        }
        Ok(())
    }
}

pub(super) fn create_and_restore(
    command: CreateRestoreCommand,
    root: &Path,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    cancellation: &ActionCancellation,
    format: presentation::Format,
) -> Result<(), CliError> {
    let CreateRestoreCommand {
        name,
        revision,
        snapshot,
        options,
    } = command;
    let snapshot = snapshot.full();
    policy(&options)?;
    // Read parameter sources once, before Create; stdin cannot be replayed.
    let (revision, parameters) = {
        let app = PactrunApplication::open_read_only(root).map_err(safe_error)?;
        let revision = resolve_revision(&app, revision)?;
        let (declarations, hook) = app
            .create_restore_definition(&revision, snapshot)
            .map_err(safe_error)?;
        if format == presentation::Format::Json {
            require_json_terminal_free(hook.io.terminal)?;
        }
        let parameters =
            read_snapshot_parameters(&declarations, &hook, &options, stdin, cancellation)?;
        crate::domain::bind_operation_parameters(&declarations, parameters.clone()).map_err(
            |_| CliError::operation("Restore parameters do not satisfy the authored declarations"),
        )?;
        (revision, parameters)
    };
    if cancellation.is_requested() {
        return Err(CliError::operation(
            "create-and-restore cancelled before Instance creation",
        ));
    }
    let app = PactrunApplication::open(root).map_err(safe_error)?;
    let created = app
        .create_instance(name, revision, Vec::new())
        .map_err(safe_error)?;
    let mut run = None;
    let result = (|| {
        // A reporting failure must also preserve and identify the created object.
        if format == presentation::Format::Human {
            write_instance(stdout, &created)?;
        }
        execute_intent(
            &app,
            SnapshotIntent {
                instance: created.id,
                operation: SnapshotOperation::Restore(snapshot),
                parameters,
            },
            options,
            ExecutionIo {
                stdin,
                stdout,
                stderr,
                format,
                publish_result: false,
            },
            cancellation,
            &mut run,
        )
    })();
    if format == presentation::Format::Json {
        let restore = run
            .and_then(|id| app.managed_run_inspection(id).ok().flatten())
            .as_ref()
            .map(execution_presentation::Inspection::from);
        let projection = transport_presentation::CreatedRestore {
            created_instance: (&created).into(),
            restore_run_id: run.map(|id| id.to_string()),
            restore,
        };
        return match result {
            Ok(()) => presentation::render(
                format,
                "instance create",
                &projection,
                stdout,
                |_, _| unreachable!(),
            ),
            Err(mut error) => {
                error.partial = Some(presentation::PartialResult::CreatedRestore(Box::new(
                    projection,
                )));
                Err(error)
            }
        };
    }
    if let Err(error) = result {
        let _ = writeln!(
            stderr,
            "partial_completion: Instance created; Restore did not report durable success\ninstance_id: {}",
            created.id
        );
        if let Some(run) = run {
            let _ = writeln!(stderr, "restore_run: {run}");
        }
        if let Ok(Some(current)) = app.load_instance(created.id) {
            let _ = write_instance(stderr, &current);
        }
        return Err(CliError::operation(format!(
            "Instance {} was created; Restore incomplete{}: {}",
            created.id,
            run.map(|id| format!(" (Run {id})")).unwrap_or_default(),
            error.message,
        )));
    }
    Ok(())
}

pub(super) fn parse(parser: &mut Parser) -> Result<SnapshotCommand, CliError> {
    parse_inner(parser).map_err(|_|CliError::usage("invalid Snapshot command or arguments; use the documented snapshot forms and --execution-timeout-ms"))
}
fn id(parser: &mut Parser) -> Result<Selector<SnapshotId>, CliError> {
    required_value_string(parser, "SnapshotId")?.parse()
}
fn bundle_path(value: OsString) -> Result<PathBuf, CliError> {
    if value == "-" || value.is_empty() {
        return Err(CliError::usage("bundle requires a filesystem path"));
    }
    Ok(PathBuf::from(value))
}
fn parse_inner(parser: &mut Parser) -> Result<SnapshotCommand, CliError> {
    match required_value_string(parser, "Snapshot command")?.as_str() {
        operation @ ("capture" | "restore") => {
            let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
            let operation = if operation == "capture" {
                Operation::Capture
            } else {
                Operation::Restore(id(parser)?)
            };
            let options = parse_execution_options(parser, "execution-timeout-ms")?;
            Ok(SnapshotCommand::Execute {
                name,
                operation,
                options,
            })
        }
        "list" => {
            let mut name = None;
            let mut no_trunc = false;
            while let Some(arg) = parser.next().map_err(lex_error)? {
                match arg {
                    Arg::Long("no-trunc") if !no_trunc => no_trunc = true,
                    Arg::Long("instance") => set_once(
                        &mut name,
                        parse_instance_name(value_string(parser, "Instance name")?)?,
                        "--instance",
                    )?,
                    _ => return Err(CliError::usage("unsupported option")),
                }
            }
            Ok(SnapshotCommand::List { name, no_trunc })
        }
        "delete" => {
            let id = id(parser)?;
            require_end(parser)?;
            Ok(SnapshotCommand::Delete(id))
        }
        operation @ ("show" | "verify") => {
            let id = id(parser)?;
            require_end(parser)?;
            Ok(if operation == "show" {
                SnapshotCommand::Show(id)
            } else {
                SnapshotCommand::Verify(id)
            })
        }
        "import" => {
            let path = bundle_path(required_value(parser, "bundle path")?)?;
            require_end(parser)?;
            Ok(SnapshotCommand::Import(path))
        }
        "export" => {
            let id = id(parser)?;
            let mut path = None;
            let mut authorized = false;
            while let Some(arg) = parser.next().map_err(lex_error)? {
                match arg {
                    Arg::Long("output") => set_once(
                        &mut path,
                        export_paths::destination(
                            required_parser_value(parser, "Snapshot output base")?,
                            export_paths::Format::Snapshot,
                        )?,
                        "--output",
                    )?,
                    Arg::Long("authorize-sensitive-export") if !authorized => authorized = true,
                    _ => return Err(CliError::usage("unsupported or duplicate option")),
                }
            }
            Ok(SnapshotCommand::Export {
                id,
                path: path.ok_or_else(|| CliError::usage("missing --output"))?,
                authorized,
            })
        }
        _ => Err(CliError::usage("unknown Snapshot command")),
    }
}

pub(super) fn execute(
    command: SnapshotCommand,
    root: &Path,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    cancellation: &ActionCancellation,
    format: presentation::Format,
) -> Result<(), CliError> {
    if matches!(
        command,
        SnapshotCommand::Export {
            authorized: false,
            ..
        }
    ) {
        return Err(CliError::operation(
            "Snapshot export requires --authorize-sensitive-export for this operation",
        ));
    }
    if let SnapshotCommand::Execute { options, .. } = &command {
        policy(options)?;
        if cancellation.is_requested() {
            return Err(CliError::operation(
                "Snapshot execution cancelled before acceptance; no Run was created",
            ));
        }
    }
    let readonly = matches!(
        command,
        SnapshotCommand::Execute {
            options: ExecutionOptions { plan: true, .. },
            ..
        } | SnapshotCommand::List { .. }
            | SnapshotCommand::Show(_)
            | SnapshotCommand::Verify(_)
    );
    let app = if readonly {
        PactrunApplication::open_read_only(root)
    } else {
        PactrunApplication::open(root)
    }
    .map_err(safe_error)?;
    match command {
        SnapshotCommand::Delete(id) => lifecycle::render_deletion(
            format,
            "snapshot delete",
            stdout,
            app.delete_object(&crate::domain::ObjectDeletion::Snapshot(id.full()))
                .map_err(app_error)?,
        ),
        SnapshotCommand::Execute {
            name,
            operation,
            options,
        } => execute_managed(
            &app,
            &name,
            operation.full(),
            options,
            ExecutionIo {
                stdin,
                stdout,
                stderr,
                format,
                publish_result: true,
            },
            cancellation,
        ),
        SnapshotCommand::List { name, no_trunc } => {
            if format == presentation::Format::Json {
                let full = app
                    .inspect_snapshots_complete(name.as_ref(), None)
                    .map_err(safe_error)?;
                let value = transport_presentation::Snapshots {
                    items: full.items.iter().map(Into::into).collect(),
                };
                return presentation::render(
                    format,
                    "snapshot list",
                    &definitions::Related::new(value, &full.revisions, &full.unavailable_revisions),
                    stdout,
                    |_, _| unreachable!(),
                );
            }
            let values = app.list_snapshots(name.as_ref()).map_err(safe_error)?;
            if values.is_empty() {
                writeln!(stdout, "No Snapshots.").map_err(io_operation)?;
            } else {
                writeln!(stdout, "SNAPSHOT ID  ORIGIN INSTANCE  CAPTURED AT")
                    .map_err(io_operation)?;
            }
            let ids = short_ids::labels(
                &app,
                crate::domain::CatalogIdentityKind::Snapshot,
                values.iter().map(|s| s.id.to_string()).collect(),
                no_trunc,
            )?;
            // Snapshot origins can outlive every managed Instance history record.
            // Keep these references complete rather than abbreviating an unavailable object.
            for (snapshot, id) in values.into_iter().zip(ids) {
                writeln!(
                    stdout,
                    "{}  {}  {}.{:09}",
                    id,
                    snapshot.origin,
                    snapshot.captured_at.unix_seconds(),
                    snapshot.captured_at.nanoseconds()
                )
                .map_err(io_operation)?;
            }
            Ok(())
        }
        SnapshotCommand::Show(id) => {
            let id = id.full();
            if format == presentation::Format::Json {
                let full = app
                    .inspect_snapshots_complete(None, Some(id))
                    .map_err(safe_error)?;
                let value = transport_presentation::Snapshot::from(&full.items[0]);
                return presentation::render(
                    format,
                    "snapshot show",
                    &definitions::Related::new(value, &full.revisions, &full.unavailable_revisions),
                    stdout,
                    |_, _| unreachable!(),
                );
            }
            let value = app.inspect_snapshot(id).map_err(safe_error)?;
            presentation::render(
                format,
                "snapshot show",
                &transport_presentation::Snapshot::from(&value),
                stdout,
                |_, out| write_inspection(out, &value),
            )
        }
        SnapshotCommand::Verify(id) => {
            let id = id.full();
            let verified = app.verify_snapshot(id).map_err(safe_error)?;
            if format == presentation::Format::Json {
                let result = transport_presentation::SnapshotVerified {
                    snapshot_id: id.to_string(),
                    integrity_format: verified.inspection.version.as_str().into(),
                    intrinsic_verification: "valid",
                    content_verification: "valid",
                    relational_verification: if verified.relational
                        == SnapshotRelationalVerification::Valid
                    {
                        "valid"
                    } else {
                        "not_evaluated"
                    },
                };
                return presentation::render(
                    format,
                    "snapshot verify",
                    &result,
                    stdout,
                    |_, _| unreachable!(),
                );
            }
            writeln!(stdout,"snapshot: {id}\nintegrity_format: {}\nintrinsic_verification: valid\ncontent_verification: valid\nrelational_verification: {}",verified.inspection.version.as_str(),if verified.relational==SnapshotRelationalVerification::Valid {"valid"}else{"not_evaluated"}).map_err(io_operation)
        }
        SnapshotCommand::Import(path) => {
            let receipt = app.import_snapshot_file(&path).map_err(safe_error)?;
            if format == presentation::Format::Json {
                let result = transport_presentation::SnapshotImported {
                    snapshot_id: receipt.id.to_string(),
                    outcome: if receipt.inserted {
                        "published"
                    } else {
                        "already_present"
                    },
                    relational_verification: if receipt.relational
                        == SnapshotRelationalVerification::Valid
                    {
                        "valid"
                    } else {
                        "not_evaluated"
                    },
                };
                return presentation::render(
                    format,
                    "snapshot import",
                    &result,
                    stdout,
                    |_, _| unreachable!(),
                );
            }
            writeln!(
                stdout,
                "snapshot: {}\nimport: {}\nrelational_verification: {}",
                receipt.id,
                if receipt.inserted {
                    "published"
                } else {
                    "already_present"
                },
                if receipt.relational == SnapshotRelationalVerification::Valid {
                    "valid"
                } else {
                    "not_evaluated"
                }
            )
            .map_err(io_operation)
        }
        SnapshotCommand::Export {
            id,
            path,
            authorized,
        } => {
            let id = id.full();
            app.export_snapshot_file(id, &path, authorized, stderr)
                .map_err(safe_error)?;
            if format == presentation::Format::Json {
                let result = transport_presentation::SnapshotExported {
                    snapshot_id: id.to_string(),
                    outcome: "published_without_replacement",
                    output: path.as_path().into(),
                };
                return presentation::render(
                    format,
                    "snapshot export",
                    &result,
                    stdout,
                    |_, _| unreachable!(),
                );
            }
            writeln!(
                stdout,
                "snapshot: {id}\nexport: published_without_replacement\noutput: {}",
                format_path(&path)
            )
            .map_err(|error| {
                CliError::operation(format!(
                    "Snapshot destination was published, but reporting success failed: {error}"
                ))
            })
        }
    }
}
fn policy(options: &ExecutionOptions) -> Result<HookRuntimePolicy, CliError> {
    HookRuntimePolicy::from_millis(
        options.startup_timeout_ms,
        options.action_timeout_ms,
        options.termination_grace_ms.or(Some(5_000)),
    )
    .map_err(CliError::usage)
}

struct ExecutionIo<'a> {
    stdin: &'a mut dyn Read,
    stdout: &'a mut dyn Write,
    stderr: &'a mut dyn Write,
    format: presentation::Format,
    publish_result: bool,
}

fn execute_managed(
    app: &PactrunApplication,
    name: &InstanceName,
    operation: SnapshotOperation,
    options: ExecutionOptions,
    io: ExecutionIo<'_>,
    cancellation: &ActionCancellation,
) -> Result<(), CliError> {
    policy(&options)?;
    let (instance, declarations, hook) = app
        .snapshot_definition(name, operation)
        .map_err(safe_error)?;
    if io.format == presentation::Format::Json && !options.plan {
        require_json_terminal_free(hook.io.terminal)?;
    }
    let parameters =
        read_snapshot_parameters(&declarations, &hook, &options, io.stdin, cancellation)?;
    execute_intent(
        app,
        SnapshotIntent {
            instance,
            operation,
            parameters,
        },
        options,
        io,
        cancellation,
        &mut None,
    )
}

fn read_snapshot_parameters(
    declarations: &[crate::domain::ParameterV1],
    hook: &crate::domain::HookV1,
    options: &ExecutionOptions,
    stdin: &mut dyn Read,
    cancellation: &ActionCancellation,
) -> Result<Vec<RawParameterInput>, CliError> {
    for source in &options.parameters {
        let id = match source {
            ParameterSourceSpec::Text(id, _)
            | ParameterSourceSpec::File(id, _)
            | ParameterSourceSpec::Stdin(id) => id,
        };
        if !declarations.iter().any(|p| p.id == *id) {
            return Err(CliError::usage(
                "unknown Snapshot parameter; inspect the authored capability",
            ));
        }
    }
    let parameter_stdin = options
        .parameters
        .iter()
        .any(|p| matches!(p, ParameterSourceSpec::Stdin(_)));
    if hook.io.terminal == crate::domain::TerminalContractV1::Interactive && parameter_stdin {
        return Err(CliError::usage(
            "interactive Snapshot Hooks do not accept --param-stdin",
        ));
    }
    read_parameter_sources(&options.parameters, stdin, cancellation).map_err(|_| {
        CliError::operation(
            "Snapshot parameter source could not be read as UTF-8 or acquisition was cancelled",
        )
    })
}

fn execute_intent(
    app: &PactrunApplication,
    intent: SnapshotIntent,
    options: ExecutionOptions,
    io: ExecutionIo<'_>,
    cancellation: &ActionCancellation,
    accepted_run: &mut Option<RunId>,
) -> Result<(), CliError> {
    let ExecutionIo {
        stdout,
        stderr,
        format,
        publish_result,
        ..
    } = io;
    let policy = policy(&options)?;
    let operation = intent.operation;
    let parameter_stdin = options
        .parameters
        .iter()
        .any(|p| matches!(p, ParameterSourceSpec::Stdin(_)));
    let plan = crate::workflow::compile_snapshot(
        app,
        &crate::workflow::PlatformHostLauncherLookup,
        &intent,
        &launcher_search_directories(),
    )
    .map_err(safe_error)?;
    if plan.hook().io.terminal == crate::domain::TerminalContractV1::Interactive && parameter_stdin
    {
        return Err(CliError::usage(
            "interactive Snapshot Hooks do not accept --param-stdin",
        ));
    }
    if cancellation.is_requested() {
        return Err(CliError::operation(
            "Snapshot execution cancelled before acceptance; no Run was created",
        ));
    }
    if options.plan {
        let selection = match operation {
            SnapshotOperation::Capture => capability_presentation::Selection::Capture,
            SnapshotOperation::Restore(_) => capability_presentation::Selection::Restore,
        };
        let author = capability_presentation::load(
            app,
            &[(plan.operation().revision().clone(), selection)],
        )?;
        if format == presentation::Format::Json {
            return presentation::render(
                format,
                match operation {
                    SnapshotOperation::Capture => "snapshot capture",
                    SnapshotOperation::Restore(_) => "snapshot restore",
                },
                &capability_presentation::Presented {
                    value: transport_presentation::SnapshotPlan::new(&plan, &options),
                    presentation: author,
                },
                stdout,
                |_, _| unreachable!(),
            );
        }
        write_plan(stdout, &plan, &options)?;
        return capability_presentation::write(stdout, &author, false);
    }
    if format == presentation::Format::Human && matches!(operation, SnapshotOperation::Restore(_)) {
        writeln!(
            stderr,
            "Warning: Restore replaces target bindings and may change service state."
        )
        .map_err(io_operation)?;
    }
    let run = app
        .accept_snapshot_plan(
            plan,
            AdmissionOptions {
                recovery_override: options.recovery_override,
            },
            cancellation.clone(),
        )
        .map_err(|error| {
            if let ApplicationError::Execution(ExecutorError::Persistence { run, .. }) = &error {
                *accepted_run = *run;
            }
            safe_error(error)
        })?;
    *accepted_run = Some(run);
    loop {
        if let Err(error) = app.execute_snapshot_if_ready(run, policy) {
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
                "Snapshot acceptance was proven absent; no replacement Run was created",
            )
            .with_run_context(run)
        })?;
    if format == presentation::Format::Json {
        let result = execution_presentation::Inspection::from(&inspection);
        if matches!(&inspection.run.state, RunState::Finished(o) if o.outcome == RunOutcome::Succeeded)
        {
            if !publish_result {
                return Ok(());
            }
            return presentation::render(
                format,
                match operation {
                    SnapshotOperation::Capture => "snapshot capture",
                    SnapshotOperation::Restore(_) => "snapshot restore",
                },
                &result,
                stdout,
                |_, _| unreachable!(),
            );
        }
        let mut error = CliError::operation(
            "Snapshot Run did not succeed; inspect the typed outcome before retrying",
        );
        error.partial = Some(presentation::PartialResult::Inspection(Box::new(result)));
        return Err(error);
    }
    if !matches!(&inspection.run.state, RunState::Finished(o) if o.outcome == RunOutcome::Succeeded)
    {
        let _ = write_run(stderr, &inspection);
    }
    match &inspection.run.state {
        RunState::Finished(outcome) if outcome.outcome == RunOutcome::Succeeded => {
            let _ = writeln!(stderr, "run: {run}\noutcome: succeeded");
            if let Some(id) = inspection.capture_result {
                writeln!(stdout, "snapshot: {id}").map_err(io_operation)?;
            }
            Ok(())
        }
        RunState::Finished(outcome) => Err(CliError::operation(format!(
            "Run {run} finished with outcome {}; inspect run show and input list before retrying",
            format_outcome(outcome.outcome)
        ))),
        RunState::Running(_) => Err(CliError::operation("Snapshot Run was not durably finished")),
    }
}
fn retry_or_fail(
    error: ApplicationError,
    run: RunId,
    stderr: &mut dyn Write,
) -> Result<(), CliError> {
    if matches!(
        error,
        ApplicationError::Persistence(
            PersistenceError::Sqlite { .. }
                | PersistenceError::Io { .. }
                | PersistenceError::DatabaseLockPoisoned
        )
    ) {
        let _ = writeln!(
            stderr,
            "Run {run}: owner retained; retrying persistence without Hook replay"
        );
        thread::sleep(Duration::from_secs(1));
        Ok(())
    } else {
        Err(safe_error(error).with_run_context(run))
    }
}
fn operation_name(operation: &ManagedRunIdentity) -> &'static str {
    match operation {
        ManagedRunIdentity::Deletion {
            mode: crate::domain::DeletionMode::ManagedCleanup,
            ..
        } => "instance_delete",
        ManagedRunIdentity::Deletion {
            mode: crate::domain::DeletionMode::AbandonManagement,
            ..
        } => "instance_abandon",
        ManagedRunIdentity::Migration(_) => "migration",
        ManagedRunIdentity::Action(_) => "action",
        ManagedRunIdentity::Capture { .. } => "snapshot_capture",
        ManagedRunIdentity::Restore { .. } => "snapshot_restore",
    }
}
fn write_plan(
    out: &mut dyn Write,
    plan: &crate::domain::SnapshotExecutionPlan,
    options: &ExecutionOptions,
) -> Result<(), CliError> {
    writeln!(
        out,
        "operation: {}\ninstance_id: {}\nrevision: {}\naccess: {}\nterminal: {}",
        operation_name(plan.operation()),
        plan.instance(),
        format_revision(plan.operation().revision()),
        access_name(plan.access()),
        terminal_name(plan.hook().io.terminal)
    )
    .map_err(io_operation)?;
    if let ManagedRunIdentity::Restore { snapshot, .. } = plan.operation() {
        writeln!(out, "snapshot: {snapshot}").map_err(io_operation)?;
    } else {
        writeln!(out, "required_inputs_satisfied: true").map_err(io_operation)?;
    }
    for p in plan.parameters() {
        writeln!(
            out,
            "parameter: {}\teffective_redaction: {}",
            p.id.as_str(),
            p.effective_redaction
        )
        .map_err(io_operation)?;
    }
    writeln!(out, "step: launch_hook").map_err(io_operation)?;
    writeln!(out,"startup_timeout_ms: {}\nexecution_timeout_ms: {}\ntermination_grace_ms: {}\nrecovery_override: {}\nMode: preview",options.startup_timeout_ms.map(|n|n.to_string()).unwrap_or_else(||"unlimited".to_owned()),options.action_timeout_ms.map(|n|n.to_string()).unwrap_or_else(||"unlimited".to_owned()),options.termination_grace_ms.unwrap_or(5_000),if options.recovery_override {"authorized_for_this_invocation"}else{"not_authorized"}).map_err(io_operation)
}
fn write_inspection(out: &mut dyn Write, s: &SnapshotInspection) -> Result<(), CliError> {
    writeln!(
        out,
        "Snapshot: {}\nProducer revision: {}\nOrigin instance: {}\nCaptured at: {}.{:09}",
        s.id,
        format_revision(&s.producer),
        s.origin,
        s.captured_at.unix_seconds(),
        s.captured_at.nanoseconds()
    )
    .map_err(io_operation)?;
    if s.restore_capability.is_err() {
        writeln!(
            out,
            "Restore unavailable: snapshot exceeds this build's capacity."
        )
        .map_err(io_operation)?;
    }
    Ok(())
}

pub(super) fn write_summary(out: &mut dyn Write, run: &ManagedRunView) -> Result<(), CliError> {
    if let ManagedRunIdentity::Action(action) = &run.operation {
        return super::write_run_summary(
            out,
            &crate::domain::RunSummary {
                id: run.id,
                instance: run.instance,
                action: action.clone(),
                phase: run.state.phase(),
                outcome: match &run.state {
                    RunState::Finished(o) => Some(o.outcome),
                    _ => None,
                },
            },
        );
    }
    writeln!(
        out,
        "run: {}\toperation: {}\tphase: {}\toutcome: {}",
        run.id,
        operation_name(&run.operation),
        format_phase(run.state.phase()),
        match &run.state {
            RunState::Finished(o) => format_outcome(o.outcome),
            _ => "running",
        }
    )
    .map_err(io_operation)
}
pub(super) fn write_run(
    out: &mut dyn Write,
    inspection: &ManagedRunInspectionData,
) -> Result<(), CliError> {
    let run = &inspection.run;
    if let ManagedRunIdentity::Action(action) = &run.operation {
        return super::write_run(
            out,
            &crate::domain::RunInspectionData {
                run: crate::domain::RunView {
                    id: run.id,
                    instance: run.instance,
                    accepted_state_version: run.accepted_state_version,
                    accepted_at_unix_ms: run.accepted_at_unix_ms,
                    action: action.clone(),
                    state: run.state.clone(),
                },
                current_recovery_guard: inspection.current_recovery_guard.clone(),
            },
        );
    }
    writeln!(out,"run: {}\ninstance_id: {}\noperation: {}\nrevision: {}\naccepted_state_version: {}\naccepted_at_unix_ms: {}",run.id,run.instance,operation_name(&run.operation),format_revision(run.operation.revision()),run.accepted_state_version,run.accepted_at_unix_ms).map_err(io_operation)?;
    if let ManagedRunIdentity::Restore { snapshot, .. } = &run.operation {
        writeln!(out, "source_snapshot: {snapshot}").map_err(io_operation)?;
    }
    if let Some(snapshot) = inspection.capture_result {
        writeln!(out, "snapshot: {snapshot}").map_err(io_operation)?;
    }
    if let ManagedRunIdentity::Migration(invocation) = &run.operation {
        writeln!(
            out,
            "target_revision: {}\nedge_count: {}",
            format_revision(invocation.target()),
            invocation.edge_count()
        )
        .map_err(io_operation)?;
        if let Some(progress) = &inspection.migration_progress {
            writeln!(out,"committed_edges: {}\nlast_committed_revision: {}\nlast_committed_state_version: {}", progress.committed_edges,format_revision(&progress.boundary_revision),progress.boundary_state_version).map_err(io_operation)?;
        } else {
            writeln!(out, "committed_edges: 0\nadmission: not completed").map_err(io_operation)?;
        }
    }
    super::write_run_state(out, &run.state, inspection.current_recovery_guard.as_ref())
}

pub(super) fn safe_error(error: ApplicationError) -> CliError {
    use crate::snapshot_bundle::BundleError as B;
    use crate::snapshot_integrity::SnapshotCodecError as C;
    let mut result = CliError::operation("");
    preserve_error_facts(&error, &mut result);
    if result.details.deletion_obligation.is_some() {
        return result;
    }
    match &error {
        ApplicationError::SnapshotCompilation(SnapshotPlanError::UnsupportedProtocol(version)) => {
            result.message = crate::domain::VersionDomain::Hook.unsupported_message(*version);
            return result;
        }
        ApplicationError::Persistence(PersistenceError::SnapshotCodec(C::UnsupportedVersion(
            version,
        ))) => {
            result.message = crate::domain::VersionDomain::Snapshot.unsupported_message(*version);
            return result;
        }
        ApplicationError::Persistence(PersistenceError::SnapshotBundle(B::UnsupportedVersion(
            domain,
            version,
        ))) => {
            result.message = domain.unsupported_message(*version);
            return result;
        }
        ApplicationError::Persistence(PersistenceError::SnapshotBundle(B::Integrity(
            C::UnsupportedVersion(version),
        ))) => {
            result.message = crate::domain::VersionDomain::Snapshot.unsupported_message(*version);
            return result;
        }
        _ => {}
    }
    let message = match error {
        ApplicationError::SnapshotCompilation(SnapshotPlanError::MissingRequired(_)) => {
            "Capture requires all required active bindings; use input list and input set before retrying"
        }
        ApplicationError::SnapshotCompilation(SnapshotPlanError::Parameters(_)) => {
            "Snapshot parameters are invalid; check declared types/defaults and use Protected file/stdin sources for sensitive parameters"
        }
        ApplicationError::SnapshotCompilation(SnapshotPlanError::Invalid(reason)) => reason,
        ApplicationError::Persistence(PersistenceError::InvalidRunTransition(reason))
            if reason == "Restore requires the exact producer Revision" =>
        {
            "Restore requires the exact producer Revision; choose an exact-compatible target"
        }
        ApplicationError::InvalidRequest(reason)
            if reason == "Snapshot bundle input must be a regular file" =>
        {
            "Snapshot bundle input must be a regular file"
        }
        ApplicationError::Persistence(
            PersistenceError::SnapshotBundle(B::Capability(_))
            | PersistenceError::SnapshotCodec(C::Capability(_)),
        ) => "Snapshot exceeds this build's capacity; verification incomplete",
        ApplicationError::Persistence(PersistenceError::SnapshotBundle(B::Integrity(
            C::Capability(_),
        ))) => "Snapshot exceeds this build's capacity; verification incomplete",
        ApplicationError::Persistence(PersistenceError::SnapshotBundle(B::Profile(_))) => {
            "Snapshot bundle profile validation failed"
        }
        ApplicationError::Persistence(
            PersistenceError::SnapshotCodec(C::Validation(_))
            | PersistenceError::SnapshotBundle(B::Integrity(C::Validation(_))),
        ) => "Snapshot integrity validation failed under the selected V1/V2 verifier",
        ApplicationError::Persistence(PersistenceError::CorruptSnapshot(_)) => {
            "stored Snapshot content or representation is corrupt; no repair was performed"
        }
        ApplicationError::Persistence(
            PersistenceError::SchemaMismatch(_) | PersistenceError::DatabaseOwnership(_),
        ) => "storage schema or ownership is unsupported; no implicit upgrade was performed",
        ApplicationError::Persistence(PersistenceError::MissingSnapshot(_)) => {
            "Snapshot is not stored"
        }
        ApplicationError::Persistence(PersistenceError::MissingInstance(_)) => {
            "Instance name does not resolve to a live Instance"
        }
        ApplicationError::Persistence(PersistenceError::SnapshotCollision(_)) => {
            "SnapshotId collision: stored identity has a different integrity digest"
        }
        ApplicationError::Persistence(PersistenceError::MissingRevision(_)) => {
            "exact compatible Revision is unavailable; no producer was installed"
        }
        ApplicationError::Persistence(PersistenceError::UnauthorizedSnapshotExport) => {
            "Snapshot export requires --authorize-sensitive-export for this operation"
        }
        ApplicationError::Io { source, .. } | ApplicationError::Publication { source, .. }
            if source.kind() == io::ErrorKind::AlreadyExists =>
        {
            "output destination already exists; no-clobber publication refused replacement"
        }
        ApplicationError::Io { .. }
        | ApplicationError::Publication { .. }
        | ApplicationError::Staging(_)
        | ApplicationError::Persistence(
            PersistenceError::Io { .. }
            | PersistenceError::Sqlite { .. }
            | PersistenceError::SnapshotBundle(B::Io(_) | B::Integrity(C::ContentIo(_)))
            | PersistenceError::SnapshotCodec(C::ContentIo(_)),
        ) => {
            "Snapshot host I/O or resource failure; verification may be incomplete and no corruption verdict is implied"
        }
        _ => {
            "Snapshot operation could not complete safely; inspect the exact Instance, Revision, storage and Run state"
        }
    };
    result.message = message.into();
    result
}
