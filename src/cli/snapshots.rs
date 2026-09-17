//! Human Snapshot surfaces. Projections never serialize manifests or payloads.
use super::*;
use crate::domain::{
    ManagedRunIdentity, ManagedRunInspectionData, ManagedRunView, SnapshotId, SnapshotIntent,
    SnapshotOperation, SnapshotPlanError, SnapshotRelationalVerification,
};
use crate::persistence::{PersistenceError, SnapshotInspection};

pub(super) enum SnapshotCommand {
    Delete(SnapshotId),
    Execute {
        name: InstanceName,
        operation: SnapshotOperation,
        options: ExecutionOptions,
    },
    List(Option<InstanceName>),
    Show(SnapshotId),
    Verify(SnapshotId),
    Import(PathBuf),
    Export {
        id: SnapshotId,
        path: PathBuf,
        authorized: bool,
    },
}

pub(super) fn parse(parser: &mut Parser) -> Result<SnapshotCommand, CliError> {
    parse_inner(parser).map_err(|_|CliError::usage("invalid Snapshot command or arguments; use the documented snapshot forms and --execution-timeout-ms"))
}
fn id(parser: &mut Parser) -> Result<SnapshotId, CliError> {
    required_value_string(parser, "SnapshotId")?
        .parse()
        .map_err(CliError::usage)
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
                SnapshotOperation::Capture
            } else {
                SnapshotOperation::Restore(id(parser)?)
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
            while let Some(arg) = parser.next().map_err(lex_error)? {
                match arg {
                    Arg::Long("instance") => set_once(
                        &mut name,
                        parse_instance_name(value_string(parser, "Instance name")?)?,
                        "--instance",
                    )?,
                    _ => return Err(CliError::usage("unsupported option")),
                }
            }
            Ok(SnapshotCommand::List(name))
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
                        bundle_path(required_parser_value(parser, "bundle output")?)?,
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
        } | SnapshotCommand::List(_)
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
        SnapshotCommand::Delete(id) => lifecycle::write_deletion(
            stdout,
            app.delete_object(&crate::domain::ObjectDeletion::Snapshot(id))
                .map_err(app_error)?,
        ),
        SnapshotCommand::Execute {
            name,
            operation,
            options,
        } => execute_managed(
            &app,
            &name,
            operation,
            options,
            ExecutionIo {
                stdin,
                stdout,
                stderr,
            },
            cancellation,
        ),
        SnapshotCommand::List(name) => {
            for snapshot in app.list_snapshots(name.as_ref()).map_err(safe_error)? {
                write_inspection(stdout, &snapshot)?;
            }
            Ok(())
        }
        SnapshotCommand::Show(id) => {
            write_inspection(stdout, &app.inspect_snapshot(id).map_err(safe_error)?)
        }
        SnapshotCommand::Verify(id) => {
            let verified = app.verify_snapshot(id).map_err(safe_error)?;
            writeln!(stdout,"snapshot: {id}\nintegrity_format: {}\nintrinsic_verification: valid\ncontent_verification: valid\nrelational_verification: {}",verified.inspection.version.number(),if verified.relational==SnapshotRelationalVerification::Valid {"valid"}else{"not_evaluated"}).map_err(io_operation)
        }
        SnapshotCommand::Import(path) => {
            let receipt = app.import_snapshot_file(&path).map_err(safe_error)?;
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
            app.export_snapshot_file(id, &path, authorized, stderr)
                .map_err(safe_error)?;
            writeln!(
                stdout,
                "snapshot: {id}\nexport: published_without_replacement"
            )
            .map_err(io_operation)
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
}

fn execute_managed(
    app: &PactrunApplication,
    name: &InstanceName,
    operation: SnapshotOperation,
    options: ExecutionOptions,
    io: ExecutionIo<'_>,
    cancellation: &ActionCancellation,
) -> Result<(), CliError> {
    let ExecutionIo {
        stdin,
        stdout,
        stderr,
    } = io;
    let policy = policy(&options)?;
    let (instance, declarations, hook) = app
        .snapshot_definition(name, operation)
        .map_err(safe_error)?;
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
    let parameters =
        read_parameter_sources(&options.parameters, stdin, cancellation).map_err(|_| {
            CliError::operation(
                "Snapshot parameter source could not be read as UTF-8 or acquisition was cancelled",
            )
        })?;
    let plan = crate::workflow::compile_snapshot(
        app,
        &crate::workflow::PlatformHostLauncherLookup,
        &SnapshotIntent {
            instance,
            operation,
            parameters,
        },
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
        return write_plan(stdout, &plan, &options);
    }
    if matches!(operation, SnapshotOperation::Restore(_)) {
        writeln!(stderr,"Warning: Restore replaces complete target bindings and may change service state; failed publication does not replay the Hook.").map_err(io_operation)?;
    }
    let run = app
        .accept_snapshot_plan(
            plan,
            AdmissionOptions {
                recovery_override: options.recovery_override,
            },
            cancellation.clone(),
        )
        .map_err(safe_error)?;
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
        .map_err(safe_error)?
        .ok_or_else(|| {
            CliError::operation(
                "Snapshot acceptance was proven absent; no replacement Run was created",
            )
        })?;
    let _ = write_run(stderr, &inspection);
    match &inspection.run.state {
        RunState::Finished(outcome) if outcome.outcome == RunOutcome::Succeeded => {
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
        Err(safe_error(error))
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
    writeln!(out,"operation: {}\ninstance_id: {}\nrevision: {}\nexpected_state_version: {}\naccess: {}\nterminal: {}\nprotocol_version: {}",operation_name(plan.operation()),plan.instance(),format_revision(plan.operation().revision()),plan.expected_state_version(),access_name(plan.access()),terminal_name(plan.hook().io.terminal),plan.hook().protocol_version.get()).map_err(io_operation)?;
    if let ManagedRunIdentity::Restore { snapshot, .. } = plan.operation() {
        writeln!(
            out,
            "snapshot: {snapshot}\ntarget_eligibility: compiler_checks_passed_not_admitted"
        )
        .map_err(io_operation)?;
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
    for step in plan.steps() {
        writeln!(
            out,
            "step: {}",
            format_failed_step(crate::domain::RunFailedStep::SnapshotPlan(*step))
        )
        .map_err(io_operation)?;
    }
    writeln!(out,"startup_timeout_ms: {}\nexecution_timeout_ms: {}\ntermination_grace_ms: {}\nrecovery_override: {}\npreview: not_admitted\nwarning: Plan is not Admission and created no Run",options.startup_timeout_ms.map(|n|n.to_string()).unwrap_or_else(||"unlimited".to_owned()),options.action_timeout_ms.map(|n|n.to_string()).unwrap_or_else(||"unlimited".to_owned()),options.termination_grace_ms.unwrap_or(5_000),if options.recovery_override {"authorized_for_this_invocation"}else{"not_authorized"}).map_err(io_operation)
}
fn write_inspection(out: &mut dyn Write, s: &SnapshotInspection) -> Result<(), CliError> {
    writeln!(out,"snapshot: {}\nintegrity_format: {}\nproducer_revision: {}\norigin_instance_id: {}\ncaptured_at: {}.{:09}\npublication_verification: intrinsic_and_content_verified\npublication_relational_verification: not_recorded\ncurrent_content_verification: not_performed\nrestore_capacity: {}\ntarget_eligibility: target_not_specified",s.id,s.version.number(),format_revision(&s.producer),s.origin,s.captured_at.unix_seconds(),s.captured_at.nanoseconds(),if s.restore_capability.is_ok() {"within_profile_not_execution_approval"}else{"exceeds_build_profile"}).map_err(io_operation)
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
        ) => {
            "Snapshot build capability exceeded; verification incomplete, not a format-corruption verdict"
        }
        ApplicationError::Persistence(PersistenceError::SnapshotBundle(B::Integrity(
            C::Capability(_),
        ))) => {
            "Snapshot build capability exceeded; verification incomplete, not a format-corruption verdict"
        }
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
        ApplicationError::Persistence(PersistenceError::UpgradeRequired) => {
            "storage upgrade required; run pactrun storage upgrade with a supported exact V7 store"
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
        ApplicationError::Io { source, .. } if source.kind() == io::ErrorKind::AlreadyExists => {
            "output destination already exists; no-clobber publication refused replacement"
        }
        ApplicationError::Io { .. }
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
    CliError::operation(message)
}
