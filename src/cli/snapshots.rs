//! Human Snapshot surfaces. Projections never serialize manifests or payloads.
use super::*;
use crate::domain::{
    SnapshotId, SnapshotIntent, SnapshotOperation, SnapshotPlanError,
    SnapshotRelationalVerification,
};
use crate::persistence::PersistenceError;

#[cfg(test)]
type CreationCallback = Box<dyn FnOnce(crate::domain::InstanceId)>;
#[cfg(test)]
std::thread_local! {
    static AFTER_CREATE:std::cell::RefCell<Option<CreationCallback>> = const {std::cell::RefCell::new(None)};
}
#[cfg(test)]
pub(super) struct CreationObserver;
#[cfg(test)]
impl Drop for CreationObserver {
    fn drop(&mut self) {
        AFTER_CREATE.with(|s| {
            s.borrow_mut().take();
        });
    }
}
#[cfg(test)]
pub(super) fn observe_creation(
    callback: impl FnOnce(crate::domain::InstanceId) + 'static,
) -> CreationObserver {
    AFTER_CREATE.with(|s| {
        assert!(s.borrow().is_none());
        *s.borrow_mut() = Some(Box::new(callback));
    });
    CreationObserver
}

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
    _stderr: &mut dyn Write,
    cancellation: &ActionCancellation,
    format: reply::OutputContext,
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
        if format.requires_noninteractive() {
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
    #[cfg(test)]
    if let Some(callback) = AFTER_CREATE.with(|s| s.borrow_mut().take()) {
        callback(created.id);
    }
    let mut run = None;
    let result = execute_intent(
        &app,
        SnapshotIntent {
            instance: created.id,
            operation: SnapshotOperation::Restore(snapshot),
            parameters,
        },
        options,
        ExecutionIo {
            stdin,
            format,
            publish_result: false,
        },
        cancellation,
        &mut run,
    );
    {
        let restore = run
            .and_then(|id| app.managed_run_inspection(id).ok().flatten())
            .as_ref()
            .map(execution_presentation::Inspection::from);
        let projection = transport_presentation::CreatedRestore {
            created_instance: (&created).into(),
            restore_run_id: run.map(|id| id.to_string()),
            restore,
        };
        match result {
            Ok(()) => presentation::emit_result(format, "instance create", &projection),
            Err(mut error) => {
                error.partial = Some(presentation::PartialResult::CreatedRestore(Box::new(
                    projection,
                )));
                Err(error)
            }
        }
    }
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
    _stderr: &mut dyn Write,
    cancellation: &ActionCancellation,
    format: reply::OutputContext,
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
                format,
                publish_result: true,
            },
            cancellation,
        ),
        SnapshotCommand::List { name, no_trunc: _ } => {
            let full = app
                .inspect_snapshots_complete(name.as_ref(), None)
                .map_err(safe_error)?;
            let value = transport_presentation::Snapshots {
                items: full
                    .items
                    .iter()
                    .map(|i| {
                        let mut v = transport_presentation::Snapshot::from(i);
                        v.unique_prefix_length = *full.selectors.get(&i.id).unwrap_or(&32);
                        v
                    })
                    .collect(),
            };
            presentation::emit_result(
                format,
                "snapshot list",
                &definitions::Related::new(value, &full.revisions, &full.unavailable_revisions),
            )
        }
        SnapshotCommand::Show(id) => {
            let id = id.full();
            {
                let full = app
                    .inspect_snapshots_complete(None, Some(id))
                    .map_err(safe_error)?;
                let value = transport_presentation::Snapshot::from(&full.items[0]);
                presentation::emit_result(
                    format,
                    "snapshot show",
                    &definitions::Related::new(value, &full.revisions, &full.unavailable_revisions),
                )
            }
        }
        SnapshotCommand::Verify(id) => {
            let id = id.full();
            let verified = app.verify_snapshot(id).map_err(safe_error)?;
            {
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
                presentation::emit_result(format, "snapshot verify", &result)
            }
        }
        SnapshotCommand::Import(path) => {
            let receipt = app.import_snapshot_file(&path).map_err(safe_error)?;
            {
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
                presentation::emit_result(format, "snapshot import", &result)
            }
        }
        SnapshotCommand::Export {
            id,
            path,
            authorized,
        } => {
            let id = id.full();
            app.export_snapshot_file(id, &path, authorized)
                .map_err(safe_error)?;
            {
                let result = transport_presentation::SnapshotExported {
                    snapshot_id: id.to_string(),
                    outcome: "published_without_replacement",
                    output: path.as_path().into(),
                };
                presentation::emit_result(format, "snapshot export", &result)
            }
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
    format: reply::OutputContext<'a>,
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
    if io.format.requires_noninteractive() && !options.plan {
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
        {
            return presentation::emit_result(
                format,
                match operation {
                    SnapshotOperation::Capture => "snapshot capture",
                    SnapshotOperation::Restore(_) => "snapshot restore",
                },
                &capability_presentation::Presented {
                    value: transport_presentation::SnapshotPlan::new(&plan, &options),
                    presentation: author,
                },
            );
        }
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
            retry_or_fail(error, run, cancellation)?;
        }
        match app.advance_owner_continuation(run) {
            Ok(true) => break,
            Ok(false) => {}
            Err(error) => retry_or_fail(error, run, cancellation)?,
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
    {
        let result = execution_presentation::Inspection::from(&inspection);
        if matches!(&inspection.run.state, RunState::Finished(o) if o.outcome == RunOutcome::Succeeded)
        {
            if !publish_result {
                return Ok(());
            }
            return presentation::emit_result(
                format,
                match operation {
                    SnapshotOperation::Capture => "snapshot capture",
                    SnapshotOperation::Restore(_) => "snapshot restore",
                },
                &result,
            );
        }
        let mut error = CliError::operation(
            "Snapshot Run did not succeed; inspect the typed outcome before retrying",
        );
        error.partial = Some(presentation::PartialResult::Inspection(Box::new(result)));
        Err(error)
    }
}
fn retry_or_fail(
    error: ApplicationError,
    run: RunId,
    cancellation: &ActionCancellation,
) -> Result<(), CliError> {
    if matches!(
        error,
        ApplicationError::Persistence(
            PersistenceError::Sqlite { .. }
                | PersistenceError::Io { .. }
                | PersistenceError::DatabaseLockPoisoned
        )
    ) {
        cancellation
            .delivery
            .progress(run, crate::hook::delivery::CorePhase::RetryingStorage);
        thread::sleep(Duration::from_secs(1));
        Ok(())
    } else {
        Err(safe_error(error).with_run_context(run))
    }
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
