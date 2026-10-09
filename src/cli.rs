//! Closed CLI parsing and composition with human and versioned JSON presentation.

use std::{
    collections::BTreeSet,
    env,
    ffi::{OsStr, OsString},
    fmt,
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread::{self, JoinHandle},
    time::Duration,
};

#[cfg(test)]
use std::fs;

use lexopt::{Arg, Parser};
mod artifacts;
mod async_stderr;
mod capability_presentation;
mod catalog;
mod catalog_presentation;
mod definitions;
mod empty_queries;
mod execution_presentation;
mod export_paths;
mod help;
mod human;
mod lifecycle;
mod migration_presentation;
mod migrations;
mod names;
mod presentation;
mod reply;
mod retirements;
#[cfg(test)]
mod schema_tests;
mod service_storage;
mod snapshots;
mod streaming;
mod transport_presentation;

use crate::{
    application::{ApplicationError, InputAcquisition, MigrationRelationState, PactrunApplication},
    domain::{
        ActionExecutionPlan, ActionIdentity, ActionPlanStep, ActionRunBoundary, ActionV1,
        CompiledHookLaunch, HookLaunchV1, InputIdentity, InstanceName, InstanceStateVersion,
        ManagedInputProtection, ManagedInputRole, OperationAccessV1, PackageId, ParameterIdentity,
        ParameterTypeV1, RawParameterInput, RevisionContentDigest, RevisionIdentity, RunId,
        RunOutcome, RunState,
    },
    executor::{AdmissionOptions, AdmittedExecution, ExecutorError},
    hook::{ActionCancellation, HookRuntimePolicy},
    managed_data::StagedFile,
};

#[cfg(test)]
use crate::domain::RevisionMetadataMutationBatch;

const STORAGE_ROOT_ENV: &str = "PACTRUN_STORAGE_ROOT";

mod short_ids;
use short_ids::Selector;

enum Command {
    Catalog(catalog::CatalogCommand),
    Lifecycle(lifecycle::LifecycleCommand),
    Artifact(artifacts::ArtifactCommand),
    Retirement(retirements::RetirementCommand),
    ServiceStorage(service_storage::ServiceCommand),
    Migration(migrations::MigrationCommand),
    Snapshot(snapshots::SnapshotCommand),
    CreateAndRestore(snapshots::CreateRestoreCommand),
    Help(help::Request),
    Version,
    GeneratePackageId,
    Name(names::NameCommand),
    Install {
        source_root: PathBuf,
        metadata_conflict: crate::domain::PackMetadataConflict,
        names: crate::domain::InstallNames,
    },
    ExportRevision {
        revision: RevisionReference,
        output: PathBuf,
        include_metadata: bool,
    },
    CreateInstance {
        name: InstanceName,
        revision: RevisionReference,
        inputs: Vec<InitialInputSpec>,
    },
    ListInstances,
    ShowInstance {
        name: InstanceName,
    },
    ResolveManualRecovery {
        name: InstanceName,
        expected: Option<InstanceStateVersion>,
    },
    ListInputs {
        name: InstanceName,
    },
    SetInput {
        name: InstanceName,
        input: InputIdentity,
        source: InputSourceSpec,
        expected: Option<InstanceStateVersion>,
    },
    ExportInput {
        name: InstanceName,
        input: InputIdentity,
        output: OsString,
        authorize_secret: bool,
    },
    DeleteInput {
        name: InstanceName,
        input: InputIdentity,
        expected: Option<InstanceStateVersion>,
    },
    ListActions {
        name: InstanceName,
    },
    ShowAction {
        name: InstanceName,
        action: ActionIdentity,
    },
    Invoke {
        no_retain_hook_text: bool,
        cancel_on_output_close: bool,
        name: InstanceName,
        action: ActionIdentity,
        parameters: Vec<ParameterSourceSpec>,
        plan: bool,
        recovery_override: bool,
        startup_timeout_ms: Option<u64>,
        action_timeout_ms: Option<u64>,
        termination_grace_ms: Option<u64>,
    },
    ShowRun {
        run: Selector<RunId>,
    },
    ReconcileRuns,
}

#[derive(Clone)]
enum RevisionReference {
    Named(crate::domain::LocalRevisionReference),
    Exact(RevisionIdentity),
    Prefix { package: String, digest: String },
}

enum InitialInputSpec {
    File(InputIdentity, PathBuf),
    Stdin(InputIdentity),
}

enum InputSourceSpec {
    File(PathBuf),
    Stdin,
}

enum ParameterSourceSpec {
    Text(ParameterIdentity, String),
    File(ParameterIdentity, PathBuf),
    Stdin(ParameterIdentity),
}

#[derive(Default)]
struct ExecutionOptions {
    no_retain_hook_text: bool,
    cancel_on_output_close: bool,
    parameters: Vec<ParameterSourceSpec>,
    plan: bool,
    recovery_override: bool,
    startup_timeout_ms: Option<u64>,
    action_timeout_ms: Option<u64>,
    termination_grace_ms: Option<u64>,
}

#[derive(Debug)]
struct CliError {
    message: String,
    usage: bool,
    kind: presentation::ErrorKind,
    reference: Option<presentation::ErrorReference>,
    partial: Option<presentation::PartialResult>,
    details: Box<ErrorDetails>,
}

#[derive(Debug, Default)]
struct ErrorDetails {
    diagnostic: Option<presentation::AcquisitionDiagnostic>,
    deletion_obligation: Option<presentation::DeletionObligationDiagnostic>,
    retirement_reason: Option<&'static str>,
}

trait CancellationHandlerInstaller {
    fn install(&self, cancellation: ActionCancellation) -> Result<(), ()>;
}

struct SystemCancellationHandler;

impl CancellationHandlerInstaller for SystemCancellationHandler {
    fn install(&self, cancellation: ActionCancellation) -> Result<(), ()> {
        let signal_cancellation = cancellation;
        ctrlc::set_handler(move || signal_cancellation.request()).map_err(|_| ())
    }
}

const CANCELLATION_HANDLER_FAILURE: &str = "foreground Ctrl+C handler could not be installed; no Run was created; retry after checking console signal support";

impl CliError {
    fn with_run_context(mut self, run: RunId) -> Self {
        if self.partial.is_none() {
            self.partial = Some(presentation::PartialResult::Run(presentation::KnownRun {
                run_id: run.to_string(),
            }));
        }
        self
    }

    fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            usage: true,
            kind: presentation::ErrorKind::Usage,
            reference: None,
            partial: None,
            details: Box::default(),
        }
    }

    fn operation(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            usage: false,
            kind: presentation::ErrorKind::Operation,
            reference: None,
            partial: None,
            details: Box::default(),
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

pub(crate) fn run_from_env() -> i32 {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if args
        .first()
        .is_some_and(|arg| arg == "--pactrun-internal-shell-loader")
    {
        return crate::hook::shell_loader::run(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "hook") && !help::is_query(&args) {
        return crate::hook::shell_loader::helper(&args[1..]);
    }
    #[cfg(any(unix, windows))]
    if args
        .first()
        .is_some_and(|argument| argument == crate::hook::INTERACTIVE_ADAPTER_ARGUMENT)
    {
        return crate::hook::run_interactive_adapter(&args[1..]);
    }
    let storage_root = env::var_os(STORAGE_ROOT_ENV);
    let cancellation = ActionCancellation::default();
    if presentation::recognize(&args) == presentation::Format::Human {
        cancellation.diagnostics.enable_live();
    }
    let mut stdout = io::stdout();
    let mut stderr = async_stderr::AsyncStderr::new();
    if let Some(exit) = preflight_presentation(&args, &mut stdout, &mut stderr) {
        return exit;
    }
    if SystemCancellationHandler
        .install(cancellation.clone())
        .is_err()
    {
        return report_startup_failure(&args, &mut stdout, &mut stderr);
    }
    let code = if args.iter().any(argument_reads_stdin) {
        let mut stdin = CancellableStdin::new(io::stdin(), cancellation.clone());
        run_with_cancellation(
            args,
            storage_root,
            &mut stdin,
            &mut stdout,
            &mut stderr,
            &cancellation,
        )
    } else {
        let mut stdin = io::stdin();
        run_with_cancellation(
            args,
            storage_root,
            &mut stdin,
            &mut stdout,
            &mut stderr,
            &cancellation,
        )
    };
    if stderr.finish().is_err() && code == 0 {
        1
    } else {
        code
    }
}

#[cfg(test)]
fn run_with_handler_installer<I: CancellationHandlerInstaller>(
    installer: &I,
    args: Vec<OsString>,
    storage_root: Option<OsString>,
    stdin: &mut (dyn Read + Send),
    stdout: &mut dyn Write,
    stderr: &mut (dyn Write + Send),
) -> i32 {
    let cancellation = ActionCancellation::default();
    if let Some(exit) = preflight_presentation(&args, stdout, stderr) {
        return exit;
    }
    if installer.install(cancellation.clone()).is_err() {
        return report_startup_failure(&args, stdout, stderr);
    }
    run_with_cancellation(args, storage_root, stdin, stdout, stderr, &cancellation)
}

fn argument_reads_stdin(argument: &OsString) -> bool {
    argument == OsStr::new("--param-stdin")
        || argument == OsStr::new("--input-stdin")
        || argument.to_string_lossy().starts_with("--param-stdin=")
        || argument.to_string_lossy().starts_with("--input-stdin=")
}

enum StdinReadMessage {
    Bytes(Vec<u8>),
    End,
    Error(io::Error),
}

struct CancellableStdin {
    source: Option<(Box<dyn Read + Send>, mpsc::SyncSender<StdinReadMessage>)>,
    receiver: Receiver<StdinReadMessage>,
    pending: Vec<u8>,
    pending_offset: usize,
    worker: Option<JoinHandle<()>>,
    cancellation: ActionCancellation,
    ended: bool,
}

impl CancellableStdin {
    fn new<R>(source: R, cancellation: ActionCancellation) -> Self
    where
        R: Read + Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(1);
        Self {
            source: Some((Box::new(source), sender)),
            receiver,
            pending: Vec::new(),
            pending_offset: 0,
            worker: None,
            cancellation,
            ended: false,
        }
    }
    fn start(&mut self) {
        let Some((mut source, sender)) = self.source.take() else {
            return;
        };
        self.worker = Some(thread::spawn(move || {
            loop {
                let mut buffer = vec![0_u8; 64 * 1024];
                match source.read(&mut buffer) {
                    Ok(0) => {
                        let _ = sender.send(StdinReadMessage::End);
                        break;
                    }
                    Ok(count) => {
                        buffer.truncate(count);
                        if sender.send(StdinReadMessage::Bytes(buffer)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(StdinReadMessage::Error(error));
                        break;
                    }
                }
            }
        }));
    }
}

impl Read for CancellableStdin {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        loop {
            if self.cancellation.is_requested() {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "stdin read cancelled",
                ));
            }
            self.start();
            if self.pending_offset < self.pending.len() {
                let count = (self.pending.len() - self.pending_offset).min(buffer.len());
                buffer[..count].copy_from_slice(
                    &self.pending[self.pending_offset..self.pending_offset + count],
                );
                self.pending_offset += count;
                if self.pending_offset == self.pending.len() {
                    self.pending.clear();
                    self.pending_offset = 0;
                }
                return Ok(count);
            }
            if self.ended {
                return Ok(0);
            }
            match self.receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(StdinReadMessage::Bytes(bytes)) => {
                    self.pending = bytes;
                    self.pending_offset = 0;
                }
                Ok(StdinReadMessage::End) => self.ended = true,
                Ok(StdinReadMessage::Error(error)) => {
                    self.ended = true;
                    return Err(error);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.ended = true;
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "stdin reader stopped unexpectedly",
                    ));
                }
            }
        }
    }
}

impl Drop for CancellableStdin {
    fn drop(&mut self) {
        if self.ended
            && let Some(worker) = self.worker.take()
        {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
pub(crate) fn run(
    args: Vec<OsString>,
    storage_root: Option<OsString>,
    stdin: &mut (dyn Read + Send),
    stdout: &mut dyn Write,
    stderr: &mut (dyn Write + Send),
) -> i32 {
    let cancellation = ActionCancellation::default();
    run_with_cancellation(args, storage_root, stdin, stdout, stderr, &cancellation)
}

fn run_with_cancellation(
    args: Vec<OsString>,
    storage_root: Option<OsString>,
    stdin: &mut (dyn Read + Send),
    stdout: &mut dyn Write,
    stderr: &mut (dyn Write + Send),
    cancellation: &ActionCancellation,
) -> i32 {
    let recognized = presentation::recognize(&args);
    let (format, args) = match presentation::select(args) {
        Ok(selected) => selected,
        Err(error) => return report_cli_failure(recognized, None, false, error, stdout, stderr),
    };
    let raw_request = presentation::raw_input_request(&args);
    let mut command = match parse_command(args) {
        Ok(command) => command,
        Err(error) => {
            return report_cli_failure(format, None, raw_request, error, stdout, stderr);
        }
    };
    let command_name = presentation::command_name(&command);
    let raw = matches!(&command, Command::ExportInput { output, .. } if output == "-");
    let (executing, cancel_on_close) = streaming::execution_options(&command);
    if cancel_on_close && (format != presentation::Format::Jsonl || !executing) {
        return report_cli_failure(
            format,
            Some(command_name),
            false,
            CliError::usage("--cancel-on-output-close requires a JSONL execution"),
            stdout,
            stderr,
        );
    }
    if raw && format == presentation::Format::Jsonl {
        return report_cli_failure(
            format,
            Some(command_name),
            false,
            CliError::usage("Choose a file destination with --output <path> for JSONL export"),
            stdout,
            stderr,
        );
    }
    if let Err(error) = short_ids::resolve(&mut command, storage_root.as_ref(), cancellation) {
        return report_cli_failure(format, Some(command_name), raw, error, stdout, stderr);
    }
    let disable_text = match &command {
        Command::Invoke {
            no_retain_hook_text,
            ..
        } => *no_retain_hook_text,
        Command::Snapshot(snapshots::SnapshotCommand::Execute { options, .. }) => {
            options.no_retain_hook_text
        }
        Command::CreateAndRestore(command) => command.options.no_retain_hook_text,
        Command::Retirement(retirements::RetirementCommand::Execute { options, .. }) => {
            options.no_retain_hook_text
        }
        Command::Migration(migrations::MigrationCommand::Plan {
            no_retain_hook_text,
            ..
        }) => *no_retain_hook_text,
        _ => false,
    };
    cancellation
        .diagnostics
        .disabled
        .store(disable_text, std::sync::atomic::Ordering::Release);
    if executing {
        return streaming::execute_stream(
            command,
            storage_root,
            stdin,
            stdout,
            stderr,
            cancellation,
            format,
            cancel_on_close,
        );
    }
    if format == presentation::Format::Jsonl {
        let mut writer = streaming::ResultWriter::new(stdout, Some(command_name));
        let code = run_selected(
            command,
            storage_root,
            stdin,
            &mut writer,
            stderr,
            cancellation,
            presentation::Format::Json,
        );
        return if writer.finish().is_err() { 1 } else { code };
    }
    run_selected(
        command,
        storage_root,
        stdin,
        stdout,
        stderr,
        cancellation,
        format,
    )
}

fn run_selected(
    command: Command,
    storage_root: Option<OsString>,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    cancellation: &ActionCancellation,
    format: presentation::Format,
) -> i32 {
    collect_reply(
        command,
        storage_root,
        stdin,
        stdout,
        stderr,
        cancellation,
        !format.requires_noninteractive(),
    )
    .render(format, stdout, stderr)
}

fn collect_reply(
    command: Command,
    storage_root: Option<OsString>,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    cancellation: &ActionCancellation,
    interactive: bool,
) -> reply::Reply {
    let command_name = presentation::command_name(&command);
    let raw = matches!(&command,Command::ExportInput {output,..} if output=="-");
    let capture = reply::Capture::new(
        command_name,
        interactive,
        reply::DisplayOptions::for_command(&command),
    );
    let result = execute(
        command,
        storage_root,
        stdin,
        stdout,
        stderr,
        cancellation,
        reply::OutputContext(&capture),
    );
    cancellation.diagnostics.finish();
    capture.finish(result.err(), raw)
}

fn preflight_presentation(
    args: &[OsString],
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Option<i32> {
    let parsed = presentation::select(args.to_vec()).and_then(|(_, args)| parse_command(args));
    parsed.err().map(|error| {
        report_cli_failure(
            presentation::recognize(args),
            None,
            presentation::raw_input_request(args),
            error,
            stdout,
            stderr,
        )
    })
}

fn report_startup_failure(
    args: &[OsString],
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut error = CliError::operation(CANCELLATION_HANDLER_FAILURE);
    error.kind = presentation::ErrorKind::Startup;
    // Parsing is pure and identifies the raw channel without opening storage.
    let raw = presentation::select(args.to_vec())
        .ok()
        .and_then(|(_, args)| parse_command(args).ok())
        .is_some_and(
            |command| matches!(command, Command::ExportInput { output, .. } if output == "-"),
        );
    report_cli_failure(
        presentation::recognize(args),
        None,
        raw,
        error,
        stdout,
        stderr,
    )
}

fn report_cli_failure(
    format: presentation::Format,
    command: Option<&str>,
    raw: bool,
    error: CliError,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    if format == presentation::Format::Jsonl {
        let mut writer = streaming::ResultWriter::new(stdout, command);
        let code = report_cli_failure(
            presentation::Format::Json,
            command,
            false,
            error,
            &mut writer,
            stderr,
        );
        return if writer.finish().is_err() { 1 } else { code };
    }
    let exit = if error.usage { 2 } else { 1 };
    if matches!(format, presentation::Format::Json)
        && !matches!(error.kind, presentation::ErrorKind::Output)
    {
        let destination: &mut dyn Write = if raw { &mut *stderr } else { &mut *stdout };
        if let Err(output_error) =
            presentation::failure(command, error.partial.as_ref(), &error, destination)
        {
            if !raw {
                let _ = writeln!(
                    stderr,
                    "error: response output failed: {output_error}; operation state is not inferred"
                );
            }
            return 1;
        }
    } else {
        let _ = writeln!(stderr, "error: {error}");
        if let Some(presentation::PartialResult::Publication(publication)) = &error.partial {
            let _ = writeln!(
                stderr,
                "publication: destination_published={} (this attempt only; no rollback or durability guarantee)",
                publication.destination_published
            );
        }
    }
    exit
}

fn parse_command(args: Vec<OsString>) -> Result<Command, CliError> {
    if help::is_query(&args) {
        let request = help::request(&args).ok_or_else(|| CliError::usage(
            "unknown help path or extra operands; use command words followed by --help, without operation operands"))?;
        return Ok(Command::Help(request));
    }
    let mut parser = Parser::from_args(args);
    let first = match parser.next().map_err(lex_error)? {
        None => return Ok(Command::Help(help::root())),
        Some(Arg::Long("help")) => {
            require_end(&mut parser)?;
            return Ok(Command::Help(help::root()));
        }
        Some(Arg::Long("version")) => {
            require_end(&mut parser)?;
            return Ok(Command::Version);
        }
        Some(Arg::Value(value)) => os_string(value, "command")?,
        Some(argument) => return Err(CliError::usage(argument.unexpected().to_string())),
    };
    match first.as_str() {
        "pack" => parse_pack(&mut parser),
        "package" => names::parse_package(&mut parser),
        "revision" => catalog::parse_revision(&mut parser),
        "instance" => parse_instance(&mut parser),
        "input" => parse_input(&mut parser),
        "action" => parse_action(&mut parser),
        "invoke" => parse_invoke(&mut parser),
        "snapshot" => snapshots::parse(&mut parser).map(Command::Snapshot),
        "service-storage" => service_storage::parse(&mut parser, true),
        "resource" => service_storage::parse(&mut parser, false),
        "run" => parse_run(&mut parser),
        "storage" => {
            let operation = required_value_string(&mut parser, "storage command")?;
            if operation == "gc" {
                let mut plan = false;
                while let Some(arg) = parser.next().map_err(lex_error)? {
                    match arg {
                        Arg::Long("plan") if !plan => plan = true,
                        _ => {
                            return Err(CliError::usage(
                                "unsupported or duplicate collection option",
                            ));
                        }
                    }
                }
                return Ok(Command::Lifecycle(lifecycle::LifecycleCommand::Collect {
                    plan,
                }));
            }
            Err(CliError::usage("unknown storage command"))
        }
        _ => Err(CliError::usage(format!("unknown command {first:?}"))),
    }
}

fn parse_pack(parser: &mut Parser) -> Result<Command, CliError> {
    match required_value_string(parser, "pack command")?.as_str() {
        "generate-id" => {
            require_end(parser)?;
            Ok(Command::GeneratePackageId)
        }
        "install" => {
            let source_root = PathBuf::from(required_value(parser, "source root")?);
            if source_root == Path::new("-") {
                return Err(CliError::usage(
                    "Pack stdin is not supported; specify a filesystem path",
                ));
            }
            let mut policy = None;
            let mut names = crate::domain::InstallNames::default();
            while let Some(arg) = parser.next().map_err(lex_error)? {
                match arg {
                    Arg::Long("package-name") if names.package.is_none() => {
                        names.package =
                            Some(names::parse_name(value_string(parser, "Package name")?)?);
                    }
                    Arg::Long("revision-name") if names.revision.is_none() => {
                        names.revision =
                            Some(names::parse_name(value_string(parser, "Revision name")?)?);
                    }
                    Arg::Long("metadata-conflict") if policy.is_none() => {
                        policy = Some(
                            match value_string(parser, "metadata conflict policy")?.as_str() {
                                "overwrite" => crate::domain::PackMetadataConflict::Overwrite,
                                "keep" => crate::domain::PackMetadataConflict::Keep,
                                _ => {
                                    return Err(CliError::usage(
                                        "--metadata-conflict must be overwrite or keep",
                                    ));
                                }
                            },
                        )
                    }
                    _ => return Err(CliError::usage("unsupported or duplicate Pack option")),
                }
            }
            Ok(Command::Install {
                source_root,
                metadata_conflict: policy.unwrap_or_default(),
                names,
            })
        }
        other => Err(CliError::usage(format!("unknown pack command {other:?}"))),
    }
}

fn parse_instance(parser: &mut Parser) -> Result<Command, CliError> {
    match required_value_string(parser, "instance command")?.as_str() {
        "history" => catalog::parse_history(parser),
        "deletion" => catalog::parse_deletion(parser),
        operation @ ("delete" | "abandon") => {
            retirements::parse_instance(parser, operation).map(Command::Retirement)
        }
        "migration-paths" => migrations::parse(parser, true).map(Command::Migration),
        "migrate" => migrations::parse(parser, false).map(Command::Migration),
        "create" => {
            let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
            let mut revision = None;
            let mut snapshot = None;
            let mut restore_options = ExecutionOptions::default();
            let mut restore_option_seen = false;
            let mut inputs = Vec::new();
            let mut stdin_seen = false;
            while let Some(argument) = parser.next().map_err(lex_error)? {
                match argument {
                    Arg::Long("revision") => {
                        let value = value_string(parser, "Revision reference")?;
                        set_once(
                            &mut revision,
                            parse_revision_reference(value)?,
                            "--revision",
                        )?;
                    }
                    Arg::Long("input-file") => {
                        let value = required_parser_value(parser, "Input file binding")?;
                        let (input, path) = split_initial_input_file(value)?;
                        inputs.push(InitialInputSpec::File(input, path));
                    }
                    Arg::Long("input-stdin") => {
                        if stdin_seen {
                            return Err(CliError::usage(
                                "only one --input-stdin declaration is allowed",
                            ));
                        }
                        stdin_seen = true;
                        let value = value_string(parser, "stdin Input identity")?;
                        inputs.push(InitialInputSpec::Stdin(parse_input_id(&value)?));
                    }
                    Arg::Long("restore-from") => set_once(
                        &mut snapshot,
                        value_string(parser, "SnapshotId")?.parse()?,
                        "--restore-from",
                    )?,
                    Arg::Long(option) => {
                        let option = option.to_owned();
                        parse_execution_option(
                            &option,
                            parser,
                            "execution-timeout-ms",
                            &mut restore_options,
                        )?;
                        restore_option_seen = true;
                    }
                    argument => return Err(CliError::usage(argument.unexpected().to_string())),
                }
            }
            if let Some(snapshot) = snapshot {
                if !inputs.is_empty() || restore_options.plan {
                    return Err(CliError::usage(
                        "--restore-from cannot be combined with initial Input options or --plan",
                    ));
                }
                validate_parameter_source_shape(&restore_options.parameters)?;
                return Ok(Command::CreateAndRestore(snapshots::CreateRestoreCommand {
                    name,
                    revision: revision.ok_or_else(|| CliError::usage("missing --revision"))?,
                    snapshot,
                    options: restore_options,
                }));
            }
            if restore_option_seen {
                return Err(CliError::usage(
                    "Restore execution options require --restore-from",
                ));
            }
            let mut input_ids = BTreeSet::new();
            for input in &inputs {
                let id = match input {
                    InitialInputSpec::File(id, _) | InitialInputSpec::Stdin(id) => id,
                };
                if !input_ids.insert(id) {
                    return Err(CliError::usage("duplicate initial Input identity"));
                }
            }
            Ok(Command::CreateInstance {
                name,
                revision: revision.ok_or_else(|| CliError::usage("missing --revision"))?,
                inputs,
            })
        }
        "list" => {
            require_end(parser)?;
            Ok(Command::ListInstances)
        }
        "show" => {
            let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
            require_end(parser)?;
            Ok(Command::ShowInstance { name })
        }
        "resolve-manual-recovery" => {
            let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
            let mut expected = None;
            while let Some(argument) = parser.next().map_err(lex_error)? {
                match argument {
                    Arg::Long("if-version") => {
                        let value = value_string(parser, "state version")?;
                        set_once(&mut expected, parse_state_version(value)?, "--if-version")?;
                    }
                    argument => return Err(CliError::usage(argument.unexpected().to_string())),
                }
            }
            Ok(Command::ResolveManualRecovery { name, expected })
        }
        other => Err(CliError::usage(format!(
            "unknown instance command {other:?}"
        ))),
    }
}

fn parse_input(parser: &mut Parser) -> Result<Command, CliError> {
    match required_value_string(parser, "input command")?.as_str() {
        "list" => {
            let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
            require_end(parser)?;
            Ok(Command::ListInputs { name })
        }
        "set" => {
            let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
            let input = parse_input_id(&required_value_string(parser, "Input identity")?)?;
            let mut source = None;
            let mut expected = None;
            while let Some(argument) = parser.next().map_err(lex_error)? {
                match argument {
                    Arg::Long("file") => {
                        let value = required_parser_value(parser, "Input file")?;
                        set_once(
                            &mut source,
                            InputSourceSpec::File(PathBuf::from(value)),
                            "--file/--stdin",
                        )?;
                    }
                    Arg::Long("stdin") => {
                        set_once(&mut source, InputSourceSpec::Stdin, "--file/--stdin")?
                    }
                    Arg::Long("if-version") => {
                        let value = value_string(parser, "state version")?;
                        set_once(&mut expected, parse_state_version(value)?, "--if-version")?;
                    }
                    argument => return Err(CliError::usage(argument.unexpected().to_string())),
                }
            }
            Ok(Command::SetInput {
                name,
                input,
                source: source.ok_or_else(|| CliError::usage("missing --file or --stdin"))?,
                expected,
            })
        }
        "export" => {
            let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
            let input = parse_input_id(&required_value_string(parser, "Input identity")?)?;
            let mut output = None;
            let mut authorize_secret = false;
            while let Some(argument) = parser.next().map_err(lex_error)? {
                match argument {
                    Arg::Long("output") => {
                        let value = required_parser_value(parser, "output destination")?;
                        set_once(&mut output, value, "--output")?;
                    }
                    Arg::Long("authorize-secret-export") if !authorize_secret => {
                        authorize_secret = true;
                    }
                    Arg::Long("authorize-secret-export") => {
                        return Err(CliError::usage("duplicate --authorize-secret-export"));
                    }
                    argument => return Err(CliError::usage(argument.unexpected().to_string())),
                }
            }
            Ok(Command::ExportInput {
                name,
                input,
                output: output.ok_or_else(|| CliError::usage("missing --output"))?,
                authorize_secret,
            })
        }
        "delete" => {
            let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
            let input = parse_input_id(&required_value_string(parser, "Input identity")?)?;
            let mut expected = None;
            while let Some(argument) = parser.next().map_err(lex_error)? {
                match argument {
                    Arg::Long("if-version") => {
                        let value = value_string(parser, "state version")?;
                        set_once(&mut expected, parse_state_version(value)?, "--if-version")?;
                    }
                    argument => return Err(CliError::usage(argument.unexpected().to_string())),
                }
            }
            Ok(Command::DeleteInput {
                name,
                input,
                expected,
            })
        }
        other => Err(CliError::usage(format!("unknown input command {other:?}"))),
    }
}

fn parse_action(parser: &mut Parser) -> Result<Command, CliError> {
    match required_value_string(parser, "action command")?.as_str() {
        "list" => {
            let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
            require_end(parser)?;
            Ok(Command::ListActions { name })
        }
        "show" => {
            let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
            let action = parse_action_id(&required_value_string(parser, "Action identity")?)?;
            require_end(parser)?;
            Ok(Command::ShowAction { name, action })
        }
        other => Err(CliError::usage(format!("unknown action command {other:?}"))),
    }
}

fn parse_invoke(parser: &mut Parser) -> Result<Command, CliError> {
    let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
    let action = parse_action_id(&required_value_string(parser, "Action identity")?)?;
    let options = parse_execution_options(parser, "action-timeout-ms")?;
    Ok(Command::Invoke {
        no_retain_hook_text: options.no_retain_hook_text,
        cancel_on_output_close: options.cancel_on_output_close,
        name,
        action,
        parameters: options.parameters,
        plan: options.plan,
        recovery_override: options.recovery_override,
        startup_timeout_ms: options.startup_timeout_ms,
        action_timeout_ms: options.action_timeout_ms,
        termination_grace_ms: options.termination_grace_ms,
    })
}

fn parse_execution_options(
    parser: &mut Parser,
    timeout_option: &'static str,
) -> Result<ExecutionOptions, CliError> {
    let mut options = ExecutionOptions::default();
    while let Some(argument) = parser.next().map_err(lex_error)? {
        let option = match argument {
            Arg::Long(option) => option.to_owned(),
            argument => return Err(CliError::usage(argument.unexpected().to_string())),
        };
        parse_execution_option(&option, parser, timeout_option, &mut options)?;
    }
    validate_parameter_source_shape(&options.parameters)?;
    Ok(options)
}

fn parse_execution_option(
    argument: &str,
    parser: &mut Parser,
    timeout_option: &'static str,
    options: &mut ExecutionOptions,
) -> Result<(), CliError> {
    match argument {
        "no-retain-hook-text" if !options.no_retain_hook_text => options.no_retain_hook_text = true,
        "cancel-on-output-close" if !options.cancel_on_output_close => {
            options.cancel_on_output_close = true
        }
        "no-retain-hook-text" => return Err(CliError::usage("duplicate --no-retain-hook-text")),
        "param" => {
            let value = required_parser_value(parser, "parameter assignment")?;
            let (id, text) = split_parameter_assignment(value, "--param", true)?;
            options.parameters.push(ParameterSourceSpec::Text(
                parse_parameter_id(&id)?,
                os_string(text, "parameter text")?,
            ));
        }
        "param-file" => {
            let value = required_parser_value(parser, "parameter file assignment")?;
            let (id, path) = split_parameter_assignment(value, "--param-file", false)?;
            options.parameters.push(ParameterSourceSpec::File(
                parse_parameter_id(&id)?,
                PathBuf::from(path),
            ));
        }
        "param-stdin" => {
            options
                .parameters
                .push(ParameterSourceSpec::Stdin(parse_parameter_id(
                    &value_string(parser, "stdin parameter identity")?,
                )?));
        }
        "plan" if !options.plan => options.plan = true,
        "plan" => return Err(CliError::usage("duplicate --plan")),
        "authorize-recovery-override" if !options.recovery_override => {
            options.recovery_override = true;
        }
        "authorize-recovery-override" => {
            return Err(CliError::usage("duplicate --authorize-recovery-override"));
        }
        "startup-timeout-ms" => {
            set_once(
                &mut options.startup_timeout_ms,
                parse_timeout_ms(value_string(parser, "startup timeout")?)?,
                "--startup-timeout-ms",
            )?;
        }
        option if option == timeout_option => {
            set_once(
                &mut options.action_timeout_ms,
                parse_timeout_ms(value_string(parser, "Action timeout")?)?,
                timeout_option,
            )?;
        }
        "termination-grace-ms" => {
            set_once(
                &mut options.termination_grace_ms,
                parse_timeout_ms(value_string(parser, "termination grace")?)?,
                "--termination-grace-ms",
            )?;
        }
        argument => {
            return Err(CliError::usage(format!(
                "unsupported execution option --{argument}"
            )));
        }
    }
    Ok(())
}

fn parse_run(parser: &mut Parser) -> Result<Command, CliError> {
    match required_value_string(parser, "run command")?.as_str() {
        "delete" => lifecycle::parse_run_delete(parser).map(Command::Lifecycle),
        "artifact" => artifacts::parse(parser).map(Command::Artifact),
        "list" => catalog::parse_runs(parser),
        "show" => {
            let run = parse_run_id(&value_string(parser, "RunId")?)?;
            require_end(parser)?;
            Ok(Command::ShowRun { run })
        }
        "reconcile" => {
            require_end(parser)?;
            Ok(Command::ReconcileRuns)
        }
        other => Err(CliError::usage(format!("unknown run command {other:?}"))),
    }
}

fn execute(
    command: Command,
    storage_root: Option<OsString>,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    cancellation: &ActionCancellation,
    format: reply::OutputContext,
) -> Result<(), CliError> {
    use presentation::emit_result;
    match command {
        Command::Help(request) => {
            return emit_result(format, "help", &request.result());
        }
        Command::Version => {
            return emit_result(
                format,
                "version",
                &presentation::Version {
                    product_version: env!("CARGO_PKG_VERSION"),
                    build_target: env!("PACTRUN_BUILD_TARGET"),
                    rustc: env!("PACTRUN_BUILD_RUSTC"),
                    source_commit: option_env!("PACTRUN_BUILD_SOURCE_COMMIT"),
                    source_manifest_sha256: option_env!("PACTRUN_BUILD_SOURCE_MANIFEST"),
                    supported_formats: crate::domain::VersionDomain::ALL
                        .into_iter()
                        .map(|domain| (domain.name(), vec![domain.current_text()]))
                        .collect(),
                    default_formats: crate::domain::VersionDomain::ALL
                        .into_iter()
                        .map(|domain| (domain.name(), domain.current_text()))
                        .collect(),
                },
            );
        }
        Command::GeneratePackageId => {
            let id = PackageId::generate()
                .map_err(|error| CliError::operation(format!("CSPRNG failed: {error}")))?;
            return emit_result(
                format,
                "pack generate-id",
                &presentation::Package {
                    package_id: id.to_string(),
                },
            );
        }
        _ => {}
    }
    let storage_root = storage_root
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::operation(format!("{STORAGE_ROOT_ENV} is required")))?;
    if cancellation.is_requested() && matches!(&command, Command::Invoke { .. }) {
        return Err(CliError::operation(
            "invoke cancelled before Run acceptance; no Run was created",
        ));
    }
    let storage_root = PathBuf::from(storage_root);
    if empty_queries::collect_missing(&command, &storage_root, format)? {
        return Ok(());
    }
    if let Command::Name(command) = command {
        return names::execute(command, &storage_root, format);
    }
    if format.requires_noninteractive()
        && let Command::Invoke {
            name,
            action,
            plan: false,
            ..
        } = &command
    {
        let app = PactrunApplication::open_read_only(&storage_root).map_err(app_error)?;
        let (_, definition) = app
            .load_action_definition(name, action)
            .map_err(app_error)?;
        require_json_terminal_free(definition.hook.io.terminal)?;
    }
    if let Command::Lifecycle(command) = command {
        return lifecycle::execute(command, &storage_root, format);
    }
    if let Command::Artifact(command) = command {
        return artifacts::execute(command, &storage_root, format);
    }
    if let Command::Retirement(command) = command {
        return retirements::execute(command, &storage_root, stderr, cancellation, format);
    }
    if let Command::Migration(command) = command {
        return migrations::execute(command, &storage_root, cancellation, format);
    }
    if let Command::Snapshot(command) = command {
        return snapshots::execute(command, &storage_root, stdin, stderr, cancellation, format);
    }
    if let Command::CreateAndRestore(command) = command {
        return snapshots::create_and_restore(
            command,
            &storage_root,
            stdin,
            stderr,
            cancellation,
            format,
        );
    }
    if let Command::ServiceStorage(command) = command {
        return service_storage::execute(command, &storage_root, format);
    }
    if let Command::Catalog(command) = command {
        return catalog::execute(command, &storage_root, format);
    }
    let readonly = matches!(
        command,
        Command::ListActions { .. }
            | Command::ShowAction { .. }
            | Command::ListInstances
            | Command::ShowInstance { .. }
            | Command::ListInputs { .. }
            | Command::Invoke { plan: true, .. }
            | Command::ShowRun { .. }
    );
    let application = if readonly {
        PactrunApplication::open_read_only(&storage_root)
    } else {
        PactrunApplication::open(&storage_root)
    }
    .map_err(app_error)?;
    match command {
        Command::ExportRevision {
            revision,
            output,
            include_metadata,
        } => {
            let identity = resolve_revision(&application, revision)?;
            application
                .export_revision_pack(&identity, &output, include_metadata, cancellation)
                .map_err(app_error)?;
            {
                let result = transport_presentation::PackExport {
                    revision: (&identity).into(),
                    output: output.as_path().into(),
                    portable_metadata_included: include_metadata,
                };
                return emit_result(format, "revision export", &result);
            }
        }
        Command::Install {
            source_root,
            metadata_conflict,
            names,
        } => {
            let installed = application
                .install_named_pack(&source_root, metadata_conflict, &names, cancellation)
                .map_err(app_error)?;
            emit_result(
                format,
                "pack install",
                &transport_presentation::PackInstall::from(&installed),
            )?;
        }
        Command::CreateInstance {
            name,
            revision,
            inputs,
        } => {
            let revision = resolve_revision(&application, revision)?;
            let mut acquisitions = Vec::new();
            let mut stdin_id = None;
            for input in inputs {
                match input {
                    InitialInputSpec::File(id, path) => {
                        let file = File::open(&path).map_err(|source| {
                            CliError::operation(format!(
                                "open initial Input file {}: {source}",
                                path.display()
                            ))
                        })?;
                        acquisitions.push(InputAcquisition {
                            input_id: id,
                            source: Box::new(file),
                        });
                    }
                    InitialInputSpec::Stdin(id) => stdin_id = Some(id),
                }
            }
            if let Some(id) = stdin_id {
                acquisitions.push(InputAcquisition {
                    input_id: id,
                    source: Box::new(&mut *stdin),
                });
            }
            let view = application
                .create_instance(name, revision, acquisitions)
                .map_err(app_error)?;
            emit_result(
                format,
                "instance create",
                &presentation::Instance::from(&view),
            )?;
        }
        Command::ListInstances => {
            let full = application
                .inspect_instances_complete()
                .map_err(app_error)?;
            let value = definitions::Related::new(
                definitions::Instances {
                    items: full.items.iter().map(Into::into).collect(),
                },
                &full.revisions,
                &[],
            );
            emit_result(format, "instance list", &value)?;
        }
        Command::ShowInstance { name } => {
            let (view, definition) = application
                .inspect_instance_definition(&name)
                .map_err(app_error)?;
            let value =
                definitions::Related::new(presentation::Instance::from(&view), &[definition], &[]);
            emit_result(format, "instance show", &value)?;
        }
        Command::ResolveManualRecovery { name, expected } => {
            let (instance, view) = resolve_instance(&application, &name)?;
            let expected = expected.unwrap_or(view.state_version);
            let next = application
                .resolve_manual_recovery(instance, expected)
                .map_err(app_error)?;
            emit_result(
                format,
                "instance resolve-manual-recovery",
                &presentation::StateVersion {
                    state_version: next.to_string(),
                },
            )?;
        }
        Command::ListInputs { name } => {
            let (view, definition) = application
                .inspect_instance_definition(&name)
                .map_err(app_error)?;
            let value = definitions::Related::new(
                presentation::Inputs::from_view(&view),
                &[definition],
                &[],
            );
            emit_result(format, "input list", &value)?;
        }
        Command::SetInput {
            name,
            input,
            source,
            expected,
        } => {
            let (instance, view) = resolve_instance(&application, &name)?;
            let expected = expected.unwrap_or(view.state_version);
            let source: Box<dyn Read + '_> = match source {
                InputSourceSpec::File(path) => Box::new(File::open(&path).map_err(|source| {
                    CliError::operation(format!("open Input file {}: {source}", path.display()))
                })?),
                InputSourceSpec::Stdin => Box::new(&mut *stdin),
            };
            let version = application
                .set_input(instance, input.clone(), expected, source)
                .map_err(app_error)?;
            emit_result(
                format,
                "input set",
                &presentation::InputMutation {
                    instance_id: instance.to_string(),
                    input_id: input.as_str().to_owned(),
                    state_version: version.to_string(),
                },
            )?;
        }
        Command::ExportInput {
            name,
            input,
            output,
            authorize_secret,
        } => {
            let (instance, _) = resolve_instance(&application, &name)?;
            let observation = application
                .export_input(instance, &input, authorize_secret)
                .map_err(app_error)?;
            if output == OsStr::new("-") {
                let mut reader = observation
                    .bytes
                    .try_clone_reader()
                    .map_err(|error| CliError::operation(error.to_string()))?;
                io::copy(&mut reader, stdout).map_err(io_operation)?;
                stdout.flush().map_err(io_operation)?;
            } else {
                publish_output_file(&observation.bytes, Path::new(&output))?;
                emit_result(
                    format,
                    "input export",
                    &presentation::InputExport {
                        input_id: input.as_str().into(),
                        state_version: observation.state_version.to_string(),
                    },
                )?;
            }
        }
        Command::DeleteInput {
            name,
            input,
            expected,
        } => {
            let (instance, view) = resolve_instance(&application, &name)?;
            let expected = expected.unwrap_or(view.state_version);
            let version = application
                .delete_input(instance, &input, expected)
                .map_err(app_error)?;
            emit_result(
                format,
                "input delete",
                &presentation::InputMutation {
                    instance_id: instance.to_string(),
                    input_id: input.as_str().to_owned(),
                    state_version: version.to_string(),
                },
            )?;
        }
        Command::ListActions { name } => {
            let (_, definition) = application
                .inspect_instance_definition(&name)
                .map_err(app_error)?;
            let actions = definition.core.actions();
            let author = capability_presentation::project(
                &definition,
                &[&capability_presentation::Selection::Actions],
            );
            {
                let result = execution_presentation::Actions {
                    items: actions
                        .iter()
                        .map(|a| execution_presentation::Action::defined(a, &definition))
                        .collect(),
                };
                return emit_result(
                    format,
                    "action list",
                    &capability_presentation::Presented {
                        value: result,
                        presentation: author,
                    },
                );
            }
        }
        Command::ShowAction { name, action } => {
            let (_, definition) = application
                .inspect_instance_definition(&name)
                .map_err(app_error)?;
            let author = capability_presentation::project(
                &definition,
                &[&capability_presentation::Selection::Action(action.clone())],
            );
            let action = definition
                .core
                .actions()
                .iter()
                .find(|value| value.id == action)
                .ok_or_else(|| {
                    app_error(crate::domain::ActionResolutionError::ActionNotFound.into())
                })?;
            {
                return emit_result(
                    format,
                    "action show",
                    &capability_presentation::Presented {
                        value: execution_presentation::Action::defined(action, &definition),
                        presentation: author,
                    },
                );
            }
        }
        Command::Invoke {
            no_retain_hook_text: _,
            cancel_on_output_close: _,
            name,
            action,
            parameters,
            plan,
            recovery_override,
            startup_timeout_ms,
            action_timeout_ms,
            termination_grace_ms,
        } => {
            let policy = HookRuntimePolicy::from_millis(
                startup_timeout_ms,
                action_timeout_ms,
                termination_grace_ms.or(Some(5_000)),
            )
            .map_err(CliError::usage)?;
            let (_, action_definition) = application
                .load_action_definition(&name, &action)
                .map_err(app_error)?;
            validate_parameter_sources(&action_definition, &parameters)?;
            if action_definition.hook.io.terminal == crate::domain::TerminalContractV1::Interactive
                && parameters
                    .iter()
                    .any(|source| matches!(source, ParameterSourceSpec::Stdin(_)))
            {
                return Err(CliError::usage(
                    "interactive Actions do not accept --param-stdin",
                ));
            }
            let raw_parameters = read_parameter_sources(&parameters, stdin, cancellation)?;
            if cancellation.is_requested() {
                return Err(CliError::operation(
                    "invoke cancelled before Run acceptance; no Run was created",
                ));
            }
            let intent = application
                .resolve_action(&name, &action, raw_parameters)
                .map_err(app_error)?;
            if cancellation.is_requested() {
                return Err(CliError::operation(
                    "invoke cancelled before Run acceptance; no Run was created",
                ));
            }
            let launcher_directories = launcher_search_directories();
            let compiled = application
                .compile_action(&intent, &launcher_directories)
                .map_err(app_error)?;
            if cancellation.is_requested() {
                return Err(CliError::operation(
                    "invoke cancelled before Run acceptance; no Run was created",
                ));
            }
            if plan {
                let author = capability_presentation::load(
                    &application,
                    &[(
                        compiled.active_revision().clone(),
                        capability_presentation::Selection::Action(action.clone()),
                    )],
                )?;
                {
                    let result = execution_presentation::Plan::new(
                        &compiled,
                        recovery_override,
                        startup_timeout_ms,
                        action_timeout_ms,
                        termination_grace_ms,
                    );
                    return emit_result(
                        format,
                        "invoke",
                        &capability_presentation::Presented {
                            value: result,
                            presentation: author,
                        },
                    );
                }
            }
            if cancellation.is_requested() {
                return Err(CliError::operation(
                    "invoke cancelled before Run acceptance; no Run was created",
                ));
            }
            let admission_options = crate::executor::AdmissionOptions { recovery_override };
            let admitted = match application.accept_and_admit_action_with_cancellation(
                &compiled,
                admission_options,
                cancellation,
            ) {
                Ok(admitted) => admitted,
                Err(error @ ApplicationError::Execution(ExecutorError::Refused { .. })) => {
                    let ApplicationError::Execution(ExecutorError::Refused { run, .. }) = &error
                    else {
                        unreachable!()
                    };
                    let run = *run;
                    let mut error = app_error(error);
                    // The accepted no-launch Run is already durable. Return the
                    // same safe historical projection as a subsequent query.
                    if let Ok(Some(inspection)) = application.managed_run_inspection(run) {
                        error.partial = Some(presentation::PartialResult::RefusedExecution(
                            Box::new(execution_presentation::RefusedExecution {
                                run_id: run.to_string(),
                                inspection: execution_presentation::Inspection::from(&inspection),
                            }),
                        ));
                    }
                    return Err(error);
                }
                Err(ApplicationError::Execution(ExecutorError::CancelledBeforeAcceptance)) => {
                    return Err(CliError::operation(
                        "invoke cancelled before Run acceptance; no Run was created",
                    ));
                }
                Err(ApplicationError::Execution(ExecutorError::Persistence {
                    run: Some(candidate),
                    ..
                })) => match recover_candidate_admission(
                    &application,
                    &compiled,
                    admission_options,
                    candidate,
                    cancellation,
                )
                .map_err(|error| error.with_run_context(candidate))?
                {
                    AdmissionRecovery::Admitted(admitted) => *admitted,
                    AdmissionRecovery::Terminal(run) => {
                        write_terminal_run_best_effort(&application, run, stderr);
                        return Err(CliError::operation(format!(
                            "Run {run} became terminal while Admission was being resolved"
                        ))
                        .with_run_context(run));
                    }
                },
                Err(error) => return Err(app_error(error)),
            };
            let run = application.execute_admitted_action(admitted, policy, cancellation.clone());
            loop {
                match application.advance_owner_continuation(run) {
                    Ok(true) => break,
                    Ok(false) => std::thread::sleep(std::time::Duration::from_millis(1_000)),
                    Err(_error) => {
                        cancellation
                            .delivery
                            .progress(run, crate::hook::delivery::CorePhase::RetryingStorage);
                        std::thread::sleep(std::time::Duration::from_millis(1_000));
                    }
                }
            }
            cancellation.diagnostics.finish();
            let managed = loop {
                match application.managed_run_inspection(run) {
                    Ok(Some(inspection))
                        if matches!(inspection.run.state, RunState::Finished(_)) =>
                    {
                        break inspection;
                    }
                    Ok(Some(_)) | Ok(None) | Err(_) => {
                        cancellation
                            .delivery
                            .progress(run, crate::hook::delivery::CorePhase::RetryingStorage);
                        thread::sleep(Duration::from_millis(1_000));
                    }
                }
            };
            {
                let result = execution_presentation::Inspection::from(&managed);
                if matches!(&managed.run.state, RunState::Finished(outcome) if outcome.outcome == RunOutcome::Succeeded)
                {
                    return emit_result(format, "invoke", &result);
                }
                let mut error = CliError::operation(
                    "Run did not succeed; inspect the typed Run outcome before deciding what to do next",
                );
                error.partial = Some(presentation::PartialResult::Inspection(Box::new(result)));
                return Err(error);
            }
        }
        Command::ShowRun { run } => {
            let run = run.full();
            {
                let full = application.inspect_run_complete(run).map_err(app_error)?;
                let inspection = full.inspections.first().expect("one requested Run");
                return emit_result(
                    format,
                    "run show",
                    &definitions::Related::new(
                        execution_presentation::Inspection::from(inspection),
                        &full.revisions,
                        &full.unavailable_revisions,
                    ),
                );
            }
        }
        Command::ReconcileRuns => {
            let runs = application
                .reconcile_lost_action_owners()
                .map_err(app_error)?;
            let result = execution_presentation::Reconciled {
                run_ids: runs.iter().map(ToString::to_string).collect(),
            };
            emit_result(format, "run reconcile", &result)?;
        }
        Command::Help(_)
        | Command::Name(_)
        | Command::Catalog(_)
        | Command::Version
        | Command::GeneratePackageId
        | Command::Artifact(_)
        | Command::Lifecycle(_)
        | Command::Migration(_)
        | Command::Snapshot(_)
        | Command::CreateAndRestore(_)
        | Command::Retirement(_)
        | Command::ServiceStorage(_) => {
            unreachable!()
        }
    }
    Ok(())
}

enum AdmissionRecovery {
    Admitted(Box<AdmittedExecution>),
    Terminal(RunId),
}

fn require_json_terminal_free(terminal: crate::domain::TerminalContractV1) -> Result<(), CliError> {
    if terminal == crate::domain::TerminalContractV1::Interactive {
        return Err(CliError::usage("Use human mode for interactive Hooks"));
    }
    Ok(())
}

/// Continues an exact candidate Run after an uncertain acceptance or Admission
/// persistence result. This function deliberately has no outward retry error:
/// while the candidate is nonterminal, the live CLI remains its owner.
fn recover_candidate_admission(
    application: &PactrunApplication,
    plan: &ActionExecutionPlan,
    options: AdmissionOptions,
    candidate: RunId,
    cancellation: &ActionCancellation,
) -> Result<AdmissionRecovery, CliError> {
    cancellation.delivery.acceptance_pending(candidate);
    loop {
        match application.load_run(candidate) {
            Ok(Some(view)) => match view.state {
                RunState::Running(execution)
                    if execution.boundary == ActionRunBoundary::Admitted =>
                {
                    return Ok(AdmissionRecovery::Admitted(Box::new(
                        AdmittedExecution::from_durable(candidate, plan.clone()),
                    )));
                }
                RunState::Running(execution)
                    if execution.boundary == ActionRunBoundary::Accepted =>
                {
                    if cancellation.is_requested() {
                        let _ = application.cancel_accepted_action(candidate);
                    } else {
                        match application.admit_accepted_action(candidate, plan, options) {
                            Ok(admitted) => {
                                return Ok(AdmissionRecovery::Admitted(Box::new(admitted)));
                            }
                            Err(ApplicationError::Execution(ExecutorError::Refused { .. })) => {
                                return Ok(AdmissionRecovery::Terminal(candidate));
                            }
                            Err(_) => {}
                        }
                    }
                }
                RunState::Finished(_) => return Ok(AdmissionRecovery::Terminal(candidate)),
                RunState::Running(_) => {}
            },
            Ok(None) => {
                return Err(CliError::operation(
                    "accepted Run candidate was proven absent; no replacement Run was created",
                ));
            }
            Err(_) => {}
        }
        cancellation
            .delivery
            .progress(candidate, crate::hook::delivery::CorePhase::RetryingStorage);
        thread::sleep(Duration::from_millis(1_000));
    }
}

fn write_terminal_run_best_effort(
    application: &PactrunApplication,
    run: RunId,
    stderr: &mut dyn Write,
) {
    if let Ok(Some(inspection)) = application.load_run_inspection(run) {
        let _ = write_run(stderr, &inspection);
    }
}

fn resolve_revision(
    application: &PactrunApplication,
    reference: RevisionReference,
) -> Result<RevisionIdentity, CliError> {
    match reference {
        RevisionReference::Named(reference) => application
            .resolve_named_revision(&reference)
            .map_err(app_error),
        RevisionReference::Prefix { .. } => unreachable!("resolved CLI selector"),
        RevisionReference::Exact(identity) => {
            if application.revision_exists(&identity).map_err(app_error)? {
                Ok(identity)
            } else {
                Err(CliError::operation("exact Revision is not installed"))
            }
        }
    }
}

fn access_name(access: OperationAccessV1) -> &'static str {
    match access {
        OperationAccessV1::Observe => "observe",
        OperationAccessV1::Mutate => "mutate",
    }
}

fn parameter_type_name(parameter_type: ParameterTypeV1) -> &'static str {
    match parameter_type {
        ParameterTypeV1::Integer => "integer",
        ParameterTypeV1::Float => "float",
        ParameterTypeV1::Boolean => "boolean",
        ParameterTypeV1::String => "string",
    }
}

fn terminal_name(terminal: crate::domain::TerminalContractV1) -> &'static str {
    match terminal {
        crate::domain::TerminalContractV1::None => "none",
        crate::domain::TerminalContractV1::Output => "output",
        crate::domain::TerminalContractV1::Interactive => "interactive",
    }
}

fn plan_step_name(step: ActionPlanStep) -> &'static str {
    match step {
        ActionPlanStep::EstablishSession => "establish_session",
        ActionPlanStep::LaunchHook => "launch_hook",
        ActionPlanStep::AcceptCompletion => "accept_completion",
        ActionPlanStep::PublishDeclaredOutputs => "publish_declared_outputs",
        ActionPlanStep::Finalize => "finalize",
    }
}

fn format_outcome(outcome: RunOutcome) -> &'static str {
    match outcome {
        RunOutcome::Succeeded => "succeeded",
        RunOutcome::Failed => "failed",
        RunOutcome::Cancelled => "cancelled",
        RunOutcome::TimedOut => "timed_out",
        RunOutcome::Interrupted => "interrupted",
    }
}

fn format_boundary(boundary: crate::domain::ActionRunBoundary) -> &'static str {
    match boundary {
        crate::domain::ActionRunBoundary::Accepted => "accepted",
        crate::domain::ActionRunBoundary::Admitted => "admitted",
    }
}

fn format_risk(risk: crate::domain::RecoveryRiskState) -> &'static str {
    match risk {
        crate::domain::RecoveryRiskState::Clear => "clear",
        crate::domain::RecoveryRiskState::Open => "open",
    }
}

fn format_trigger(trigger: crate::domain::ManualRecoveryTrigger) -> &'static str {
    match trigger {
        crate::domain::ManualRecoveryTrigger::OpenRiskFailure => "open_risk_failure",
        crate::domain::ManualRecoveryTrigger::OpenRiskOwnerLoss => "open_risk_owner_loss",
        crate::domain::ManualRecoveryTrigger::SuccessWithOpenRisk => "success_with_open_risk",
    }
}

fn format_hook_status(status: crate::domain::HookCompletionStatus) -> &'static str {
    match status {
        crate::domain::HookCompletionStatus::Success => "success",
        crate::domain::HookCompletionStatus::Failure => "failure",
    }
}

fn format_failed_step(step: crate::domain::RunFailedStep) -> String {
    match step {
        crate::domain::RunFailedStep::DeletionPlan(step) => match step {
            crate::domain::DeletionPlanStep::EstablishSession => "establish_session",
            crate::domain::DeletionPlanStep::LaunchCleanup => "launch_cleanup",
            crate::domain::DeletionPlanStep::AcceptCompletion => "accept_completion",
            crate::domain::DeletionPlanStep::AuthorizeFinalization => "authorize_finalization",
            crate::domain::DeletionPlanStep::FinalizeStorage => "finalize_storage",
        }
        .to_owned(),
        crate::domain::RunFailedStep::MigrationPlan(step) => match step {
            crate::domain::MigrationPlanStep::EstablishSession => "establish_session",
            crate::domain::MigrationPlanStep::LaunchHook => "launch_hook",
            crate::domain::MigrationPlanStep::AcceptCompletion => "accept_completion",
            crate::domain::MigrationPlanStep::PublishManagedResult => "publish_managed_result",
            crate::domain::MigrationPlanStep::Finalize => "finalize",
        }
        .to_owned(),
        crate::domain::RunFailedStep::Admission => "admission".to_owned(),
        crate::domain::RunFailedStep::Plan(step) => plan_step_name(step).to_owned(),
        crate::domain::RunFailedStep::SnapshotPlan(step) => match step {
            crate::domain::SnapshotPlanStep::EstablishSession => "establish_session",
            crate::domain::SnapshotPlanStep::LaunchHook => "launch_hook",
            crate::domain::SnapshotPlanStep::AcceptCompletion => "accept_completion",
            crate::domain::SnapshotPlanStep::PublishManagedResult => "publish_managed_result",
            crate::domain::SnapshotPlanStep::Finalize => "finalize",
        }
        .to_owned(),
    }
}

fn write_run(
    output: &mut dyn Write,
    inspection: &crate::domain::RunInspectionData,
) -> Result<(), CliError> {
    let run = &inspection.run;
    writeln!(output, "run: {}", run.id).map_err(io_operation)?;
    writeln!(output, "instance_id: {}", run.instance).map_err(io_operation)?;
    writeln!(output, "action: {}", run.action.action.as_str()).map_err(io_operation)?;
    writeln!(
        output,
        "revision: {}",
        format_revision(&run.action.revision)
    )
    .map_err(io_operation)?;
    writeln!(
        output,
        "accepted_state_version: {}",
        run.accepted_state_version
    )
    .map_err(io_operation)?;
    writeln!(output, "accepted_at_unix_ms: {}", run.accepted_at_unix_ms).map_err(io_operation)?;
    write_run_state(
        output,
        &run.state,
        inspection.current_recovery_guard.as_ref(),
    )
}

fn write_run_state(
    output: &mut dyn Write,
    state: &RunState,
    current_recovery_guard: Option<&crate::domain::RecoveryGuardView>,
) -> Result<(), CliError> {
    match state {
        crate::domain::RunState::Running(execution) => {
            writeln!(output, "phase: running").map_err(io_operation)?;
            writeln!(output, "boundary: {}", format_boundary(execution.boundary))
                .map_err(io_operation)?;
            writeln!(
                output,
                "terminal_risk: {}",
                format_risk(execution.risk_state)
            )
            .map_err(io_operation)?;
        }
        crate::domain::RunState::Finished(outcome) => {
            writeln!(output, "phase: finished").map_err(io_operation)?;
            writeln!(output, "outcome: {}", format_outcome(outcome.outcome))
                .map_err(io_operation)?;
            writeln!(output, "boundary: {}", format_boundary(outcome.boundary))
                .map_err(io_operation)?;
            writeln!(
                output,
                "terminal_risk: {}",
                format_risk(outcome.terminal_risk)
            )
            .map_err(io_operation)?;
            writeln!(
                output,
                "finished_at_unix_ms: {}",
                outcome.finished_at_unix_ms
            )
            .map_err(io_operation)?;
            if let Some(failure) = &outcome.primary_failure {
                writeln!(
                    output,
                    "primary_failure: {}:{}\tstep: {}\nexplanation: {}",
                    failure.failure.error.owner(),
                    failure.failure.error.code(),
                    format_failed_step(failure.step),
                    failure_explanation(&failure.failure.error)
                )
                .map_err(io_operation)?;
                if let Some(detail) = failure_detail(&failure.failure) {
                    writeln!(output, "detail: {detail}").map_err(io_operation)?;
                }
            }
            for failure in &outcome.secondary_failures {
                writeln!(
                    output,
                    "secondary_failure: {}:{}\nexplanation: {}",
                    failure.error.owner(),
                    failure.error.code(),
                    failure_explanation(&failure.error)
                )
                .map_err(io_operation)?;
                if let Some(detail) = failure_detail(failure) {
                    writeln!(output, "detail: {detail}").map_err(io_operation)?;
                }
            }
            if let Some(completion) = &outcome.hook_completion {
                writeln!(
                    output,
                    "hook_completion_status: {}",
                    format_hook_status(completion.status)
                )
                .map_err(io_operation)?;
                writeln!(output, "legacy_hook_completion_text: withheld").map_err(io_operation)?;
            }
            for artifact in &outcome.artifacts {
                writeln!(
                    output,
                    "artifact: {}\tbytes: {}",
                    artifact.output.as_str(),
                    artifact.byte_length
                )
                .map_err(io_operation)?;
            }
        }
    }
    if let Some(guard) = current_recovery_guard {
        writeln!(
            output,
            "current_recovery_guard: {}\trun: {}\tentered_at_unix_ms: {}",
            format_trigger(guard.trigger),
            guard.run,
            guard.entered_at_unix_ms
        )
        .map_err(io_operation)?;
        writeln!(
            output,
            "Inspect the triggering Run: pactrun run show {}",
            guard.run
        )
        .map_err(io_operation)?;
    } else {
        writeln!(output, "current_recovery_guard: none").map_err(io_operation)?;
    }
    Ok(())
}

fn parse_action_id(value: &str) -> Result<ActionIdentity, CliError> {
    ActionIdentity::parse(value.to_owned()).map_err(|error| CliError::usage(error.to_string()))
}

fn parse_parameter_id(value: &str) -> Result<ParameterIdentity, CliError> {
    ParameterIdentity::parse(value.to_owned()).map_err(|error| CliError::usage(error.to_string()))
}

fn parse_run_id(value: &str) -> Result<Selector<RunId>, CliError> {
    value.parse()
}

fn parse_timeout_ms(value: String) -> Result<u64, CliError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(CliError::usage(
            "timeout values must be non-negative decimal integers",
        ));
    }
    value
        .parse::<u64>()
        .map_err(|_| CliError::usage("timeout value is too large for a supported deadline"))
}

fn validate_parameter_source_shape(parameters: &[ParameterSourceSpec]) -> Result<(), CliError> {
    let mut ids = BTreeSet::new();
    let mut stdin_count = 0;
    for parameter in parameters {
        let id = match parameter {
            ParameterSourceSpec::Text(id, _)
            | ParameterSourceSpec::File(id, _)
            | ParameterSourceSpec::Stdin(id) => id,
        };
        if !ids.insert(id) {
            return Err(CliError::usage("duplicate parameter identity"));
        }
        if matches!(parameter, ParameterSourceSpec::Stdin(_)) {
            stdin_count += 1;
        }
    }
    if stdin_count > 1 {
        return Err(CliError::usage("only one --param-stdin source is allowed"));
    }
    Ok(())
}

fn validate_parameter_sources(
    action: &ActionV1,
    parameters: &[ParameterSourceSpec],
) -> Result<(), CliError> {
    for parameter in parameters {
        let id = match parameter {
            ParameterSourceSpec::Text(id, _)
            | ParameterSourceSpec::File(id, _)
            | ParameterSourceSpec::Stdin(id) => id,
        };
        if !action
            .parameters
            .iter()
            .any(|declaration| declaration.id == *id)
        {
            return Err(CliError::usage(format!(
                "unknown parameter {}",
                id.as_str()
            )));
        }
    }
    Ok(())
}

fn read_parameter_sources(
    parameters: &[ParameterSourceSpec],
    stdin: &mut dyn Read,
    cancellation: &ActionCancellation,
) -> Result<Vec<RawParameterInput>, CliError> {
    parameters
        .iter()
        .map(|parameter| {
            if cancellation.is_requested() {
                return Err(CliError::operation(
                    "invoke cancelled before Run acceptance; no Run was created",
                ));
            }
            match parameter {
                ParameterSourceSpec::Text(id, text) => Ok(RawParameterInput {
                    id: id.clone(),
                    text: text.clone(),
                    source: crate::domain::ParameterTextSource::Ordinary,
                }),
                ParameterSourceSpec::File(id, path) => {
                    let mut file = File::open(path).map_err(|error| {
                        CliError::operation(format!(
                            "open parameter file {}: {error}",
                            path.display()
                        ))
                    })?;
                    let bytes = read_cancellable_bytes(&mut file, cancellation)?;
                    let text = String::from_utf8(bytes)
                        .map_err(|_| CliError::operation("parameter source is not valid UTF-8"))?;
                    Ok(RawParameterInput {
                        id: id.clone(),
                        text,
                        source: crate::domain::ParameterTextSource::Protected,
                    })
                }
                ParameterSourceSpec::Stdin(id) => {
                    let bytes = read_cancellable_bytes(stdin, cancellation)?;
                    let text = String::from_utf8(bytes)
                        .map_err(|_| CliError::operation("parameter source is not valid UTF-8"))?;
                    Ok(RawParameterInput {
                        id: id.clone(),
                        text,
                        source: crate::domain::ParameterTextSource::Protected,
                    })
                }
            }
        })
        .collect()
}

fn read_cancellable_bytes(
    source: &mut dyn Read,
    cancellation: &ActionCancellation,
) -> Result<Vec<u8>, CliError> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        if cancellation.is_requested() {
            return Err(CliError::operation(
                "invoke cancelled before Run acceptance; no Run was created",
            ));
        }
        let count = source.read(&mut buffer).map_err(|error| {
            if cancellation.is_requested() {
                CliError::operation("invoke cancelled before Run acceptance; no Run was created")
            } else {
                CliError::operation(format!("read parameter stdin: {error}"))
            }
        })?;
        if count == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

#[cfg(windows)]
fn split_parameter_assignment(
    value: OsString,
    option: &str,
    allow_empty_right: bool,
) -> Result<(String, OsString), CliError> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let units = value.encode_wide().collect::<Vec<_>>();
    let separator = units.iter().position(|unit| *unit == u16::from(b'='));
    let Some(separator) = separator else {
        return Err(CliError::usage(format!(
            "{option} requires <parameter-id>=<value>"
        )));
    };
    if separator == 0 || (!allow_empty_right && separator + 1 == units.len()) {
        return Err(CliError::usage(format!(
            "{option} requires a non-empty parameter identity and value"
        )));
    }
    let id = String::from_utf16(&units[..separator])
        .map_err(|_| CliError::usage("parameter identity must be valid Unicode"))?;
    Ok((id, OsString::from_wide(&units[separator + 1..])))
}

#[cfg(unix)]
fn split_parameter_assignment(
    value: OsString,
    option: &str,
    allow_empty_right: bool,
) -> Result<(String, OsString), CliError> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = value.as_os_str().as_bytes();
    let separator = bytes.iter().position(|byte| *byte == b'=');
    let Some(separator) = separator else {
        return Err(CliError::usage(format!(
            "{option} requires <parameter-id>=<value>"
        )));
    };
    if separator == 0 || (!allow_empty_right && separator + 1 == bytes.len()) {
        return Err(CliError::usage(format!(
            "{option} requires a non-empty parameter identity and value"
        )));
    }
    let id = std::str::from_utf8(&bytes[..separator])
        .map_err(|_| CliError::usage("parameter identity must be valid UTF-8"))?;
    Ok((
        id.to_owned(),
        OsString::from_vec(bytes[separator + 1..].to_vec()),
    ))
}

fn launcher_search_directories() -> Vec<PathBuf> {
    env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect())
        .unwrap_or_default()
}

fn resolve_instance(
    application: &PactrunApplication,
    name: &InstanceName,
) -> Result<(crate::domain::InstanceId, crate::domain::InstanceView), CliError> {
    let id = application
        .resolve_instance_name(name)
        .map_err(app_error)?
        .ok_or_else(|| CliError::operation("Instance name does not resolve"))?;
    let view = application
        .load_instance(id)
        .map_err(app_error)?
        .ok_or_else(|| CliError::operation("resolved Instance disappeared"))?;
    Ok((id, view))
}

fn parse_revision_reference(value: String) -> Result<RevisionReference, CliError> {
    if let Some(exact) = value.strip_prefix("exact:").filter(|v| v.contains('/')) {
        let (package, digest) = exact.split_once('/').ok_or_else(|| {
            CliError::usage("exact Revision requires <PackageId>/sha256:<digest>")
        })?;
        let digits = digest
            .strip_prefix("sha256:")
            .ok_or_else(|| CliError::usage("Revision digest requires sha256:"))?;
        if !short_ids::valid_hex(package, 32) || !short_ids::valid_hex(digits, 64) {
            return Err(CliError::usage(
                "Revision requires full IDs or lowercase hexadecimal prefixes of at least 8 digits",
            ));
        }
        if package.len() != 32 || digits.len() != 64 {
            return Ok(RevisionReference::Prefix {
                package: package.into(),
                digest: digits.into(),
            });
        }
        let package_id = PackageId::from_str(package).map_err(CliError::usage)?;
        let content_digest = RevisionContentDigest::from_str(digest).map_err(CliError::usage)?;
        return Ok(RevisionReference::Exact(RevisionIdentity::new(
            package_id,
            content_digest,
        )));
    }
    let reference = crate::domain::LocalRevisionReference::parse(&value)
        .map_err(|e| CliError::usage(e.to_string()))?;
    let (package, revision) = reference.components();
    if package.as_str().len() == 32 && revision.as_str().len() == 64 {
        // Complete IDs cannot be local names. Keep exact-object semantics,
        // including idempotent deletion after the installed row is gone.
        return Ok(RevisionReference::Exact(RevisionIdentity::new(
            PackageId::from_str(package.as_str()).map_err(CliError::usage)?,
            RevisionContentDigest::from_str(&format!("sha256:{}", revision.as_str()))
                .map_err(CliError::usage)?,
        )));
    }
    Ok(RevisionReference::Named(reference))
}

fn parse_instance_name(value: String) -> Result<InstanceName, CliError> {
    InstanceName::parse(value).map_err(|error| CliError::usage(error.to_string()))
}

fn parse_input_id(value: &str) -> Result<InputIdentity, CliError> {
    InputIdentity::parse(value.to_owned()).map_err(|error| CliError::usage(error.to_string()))
}

fn split_initial_input_file(value: OsString) -> Result<(InputIdentity, PathBuf), CliError> {
    let (input, path) = split_input_file(value)?;
    Ok((parse_input_id(&input)?, path))
}

#[cfg(windows)]
fn split_input_file(value: OsString) -> Result<(String, PathBuf), CliError> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    let units = value.encode_wide().collect::<Vec<_>>();
    let separator = units
        .iter()
        .position(|unit| *unit == u16::from(b'='))
        .ok_or_else(|| CliError::usage("--input-file requires <input-id>=<host-path>"))?;
    if separator == 0 || separator + 1 == units.len() {
        return Err(CliError::usage(
            "--input-file requires non-empty <input-id> and <host-path>",
        ));
    }
    let input = String::from_utf16(&units[..separator])
        .map_err(|_| CliError::usage("Input identity must be valid Unicode"))?;
    let path = OsString::from_wide(&units[separator + 1..]);
    Ok((input, PathBuf::from(path)))
}

#[cfg(unix)]
fn split_input_file(value: OsString) -> Result<(String, PathBuf), CliError> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let bytes = value.as_bytes();
    let separator = bytes
        .iter()
        .position(|byte| *byte == b'=')
        .ok_or_else(|| CliError::usage("--input-file requires <input-id>=<host-path>"))?;
    if separator == 0 || separator + 1 == bytes.len() {
        return Err(CliError::usage(
            "--input-file requires non-empty <input-id> and <host-path>",
        ));
    }
    let input = std::str::from_utf8(&bytes[..separator])
        .map_err(|_| CliError::usage("Input identity must be valid UTF-8"))?;
    let path = OsString::from_vec(bytes[separator + 1..].to_vec());
    Ok((input.to_owned(), PathBuf::from(path)))
}

fn parse_state_version(value: String) -> Result<InstanceStateVersion, CliError> {
    InstanceStateVersion::from_str(&value).map_err(CliError::usage)
}

fn format_revision(identity: &RevisionIdentity) -> String {
    format!("exact:{}/{}", identity.package_id, identity.content_digest)
}

fn required_value(parser: &mut Parser, name: &str) -> Result<OsString, CliError> {
    match parser.next().map_err(lex_error)? {
        Some(Arg::Value(value)) => Ok(value),
        Some(argument) => Err(CliError::usage(argument.unexpected().to_string())),
        None => Err(CliError::usage(format!("missing {name}"))),
    }
}

fn required_parser_value(parser: &mut Parser, name: &str) -> Result<OsString, CliError> {
    parser.value().map_err(lex_error).and_then(|value| {
        (!value.is_empty())
            .then_some(value)
            .ok_or_else(|| CliError::usage(format!("empty {name}")))
    })
}

fn required_value_string(parser: &mut Parser, name: &str) -> Result<String, CliError> {
    os_string(required_value(parser, name)?, name)
}

fn value_string(parser: &mut Parser, name: &str) -> Result<String, CliError> {
    os_string(required_parser_value(parser, name)?, name)
}

fn os_string(value: OsString, name: &str) -> Result<String, CliError> {
    value
        .into_string()
        .map_err(|_| CliError::usage(format!("{name} must be valid Unicode")))
}

fn require_end(parser: &mut Parser) -> Result<(), CliError> {
    match parser.next().map_err(lex_error)? {
        None => Ok(()),
        Some(argument) => Err(CliError::usage(argument.unexpected().to_string())),
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T, name: &str) -> Result<(), CliError> {
    if slot.replace(value).is_some() {
        Err(CliError::usage(format!("duplicate {name}")))
    } else {
        Ok(())
    }
}

fn lex_error(error: lexopt::Error) -> CliError {
    CliError::usage(error.to_string())
}

fn app_error(error: ApplicationError) -> CliError {
    let mut result = CliError::operation(error.to_string());
    preserve_error_facts(&error, &mut result);
    result
}

fn preserve_error_facts(error: &ApplicationError, result: &mut CliError) {
    if let ApplicationError::Persistence(
        crate::persistence::PersistenceError::ServiceStorageUnavailable(message),
    ) = error
    {
        result.details.retirement_reason = crate::retirement_fs::failure_reason(message);
    }
    if let ApplicationError::MigrationInputAcquisition(failure) = error {
        result.details.diagnostic = Some(failure.as_ref().into());
    }
    if let ApplicationError::Publication {
        destination,
        destination_published,
        ..
    } = error
    {
        result.partial = Some(presentation::PartialResult::Publication(Box::new(
            presentation::Publication {
                destination: destination.as_path().into(),
                destination_published: *destination_published,
            },
        )));
    }
    let reference = match error {
        ApplicationError::Revision(e) => e.stable_ref(),
        ApplicationError::RevisionContent(
            crate::revision_content::RevisionContentError::Declarations(e),
        ) => e.stable_ref(),
        _ => {
            let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
            let mut reference = None;
            while let Some(current) = source {
                if let Some(core) = current.downcast_ref::<crate::domain::RevisionError>() {
                    reference = core.stable_ref();
                    break;
                }
                source = current.source();
            }
            reference
        }
    };
    result.reference = reference.map(|r| presentation::ErrorReference {
        owner: r.owner().into(),
        code: r.code().into(),
    });
    if let ApplicationError::Execution(ExecutorError::Refused { run, refusal }) = &error {
        if let crate::domain::AdmissionRefusal::DeletionObligation(instance) = refusal {
            result.message = format!(
                "{} Instance: {instance}. Run: {run}.",
                DELETION_OBLIGATION_GUIDANCE
            );
            result.details.deletion_obligation = Some(presentation::DeletionObligationDiagnostic {
                instance_id: instance.to_string(),
                run_id: run.to_string(),
            });
        }
        let reference = refusal.error_ref();
        result.reference = Some(presentation::ErrorReference {
            owner: reference.owner().into(),
            code: reference.code().into(),
        });
        result.partial = Some(presentation::PartialResult::Run(presentation::KnownRun {
            run_id: run.to_string(),
        }));
    } else if let ApplicationError::Execution(ExecutorError::Persistence {
        run: Some(run), ..
    }) = &error
    {
        result.partial = Some(presentation::PartialResult::Run(presentation::KnownRun {
            run_id: run.to_string(),
        }));
    }
}

fn io_operation(error: io::Error) -> CliError {
    CliError::operation(error.to_string())
}

#[cfg(test)]
use crate::output_publication::OutputPublicationPoint;

fn publish_output_file(source: &StagedFile, destination: &Path) -> Result<(), CliError> {
    crate::output_publication::publish_reported(source, destination)
        .map_err(|failure| publication_error(failure, destination))
}

fn publication_error(
    failure: crate::output_publication::PublicationFailure,
    destination: &Path,
) -> CliError {
    let mut error = CliError::operation(failure.source.to_string());
    error.partial = Some(presentation::PartialResult::Publication(Box::new(
        presentation::Publication {
            destination: destination.into(),
            destination_published: failure.destination_published,
        },
    )));
    error
}

#[cfg(test)]
fn publish_output_file_with_fault(
    source: &StagedFile,
    destination: &Path,
    mut fault: impl FnMut(OutputPublicationPoint) -> Result<(), CliError>,
) -> Result<(), CliError> {
    crate::output_publication::publish_with_fault(source, destination, |point| {
        fault(point).map_err(|error| io::Error::other(error.message))
    })
    .map_err(io_operation)
}

const DELETION_OBLIGATION_GUIDANCE: &str = "Admission was refused because a deletion obligation was unresolved, independently of recovery guard state. Inspect pactrun instance deletion show <instance-id> for current evidence. Finalization may already have removed data; do not assume an intact service or replay completed Cleanup. This diagnostic does not authorize retry, repair or abandonment.";

fn failure_detail(record: &crate::domain::RunFailureRecord) -> Option<&str> {
    if record.error.owner() == "admission"
        && record.error.code() == "plan_invalidated"
        && record.message == crate::domain::DELETION_OBLIGATION_MESSAGE
    {
        return Some(DELETION_OBLIGATION_GUIDANCE);
    }
    if record.error.owner() == "execution"
        && matches!(
            record.error.code(),
            "ipc_initialization_failed" | "shell_loader_initialization_failed"
        )
    {
        crate::hook::startup::safe_detail(&record.message)
    } else if record.error.owner() == "service_storage"
        && record.error.code() == "allocation_unavailable"
    {
        crate::retirement_fs::safe_failure_detail(&record.message)
    } else {
        None
    }
}

fn failure_explanation(error: &crate::domain::PactrunErrorRef) -> &'static str {
    match (error.owner(), error.code()) {
        ("admission", "recovery_guard_active") => {
            "Admission was refused because a recovery guard was active. Inspect the reported current guard and its triggering Run, if available, plus the last committed boundary and current Instance. This refusal can have an accepted Run without starting a Hook; it does not authorize override, acknowledgment or retry."
        }
        ("execution", "ipc_initialization_failed") => {
            "IPC could not be initialized before the user Hook started."
        }
        ("execution", "shell_loader_initialization_failed") => {
            "The built-in Shell Loader could not initialize before the user script started."
        }
        ("execution", "launch_failed") => "The Hook process could not be launched.",
        ("execution", "session_materialization_failed") => {
            "The Hook execution context could not be prepared or verified."
        }
        ("execution", "protocol_transport_failed") => {
            "Communication with the Hook ended or failed before the required completion."
        }
        ("execution", "hook_reported_protocol_error") => {
            "The Hook reported a protocol failure; inspect the attributed Hook explanation if retained."
        }
        ("execution", "managed_output_publication_failed") => {
            "Managed results could not be published; inspect the Run outcome and retained result references."
        }
        ("execution", "workspace_cleanup_failed") => {
            "Execution workspace cleanup failed; this is separate from the primary operation result."
        }
        ("admission", "plan_invalidated") => {
            "The current state no longer satisfies the accepted plan's admission requirements."
        }
        ("hook_protocol", "unexpected_message") => {
            "A Hook message was not permitted at this protocol stage."
        }
        ("hook_protocol", "invalid_message") => {
            "A Hook message did not satisfy the protocol contract; its raw content is not displayed."
        }
        ("hook_protocol", _) => {
            "Hook protocol validation failed; the exact taxonomy code identifies the violated rule."
        }
        ("service_storage", _) => {
            "The required service-storage authority or resource could not be qualified."
        }
        _ => {
            "Pactrun could not complete the indicated step; the exact taxonomy identity is retained above."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    include!("cli/create_restore_tests.rs");
    include!("cli/names_tests.rs");
    include!("cli/json_tests.rs");
    include!("cli/machine_delivery_tests.rs");
    include!("cli/short_id_tests.rs");

    // Test-ID: PR-TEST-0593
    // Verifies: PR-REQ-0038, PR-REQ-0116, PR-REQ-0119
    #[test]
    fn generic_confirmation_never_grants_disclosure_declassification_or_recovery_authority() {
        let (temporary, _, _) = cli_roots();
        let missing = temporary.path().join("must-not-open");
        let id = "00000000000000000000000000000001";
        let revision = "exact:00000000000000000000000000000001/sha256:0000000000000000000000000000000000000000000000000000000000000001";
        let export = temporary.path().join("must-not-export");
        for flag in ["--force", "--yes"] {
            for mut args in [
                vec!["input", "export", "node", "secret", "--output", "-"],
                vec![
                    "snapshot",
                    "export",
                    id,
                    "--output",
                    export.to_str().unwrap(),
                ],
                vec!["instance", "migrate", "node", "--to", revision],
                vec!["invoke", "node", "recover"],
                vec!["instance", "abandon", "node"],
            ] {
                assert!(
                    parse_command(args.iter().map(OsString::from).collect()).is_ok(),
                    "invalid test command: {args:?}"
                );
                args.push(flag);
                let mut output = Vec::new();
                let mut errors = Vec::new();
                let code = run(
                    args.iter().map(OsString::from).collect(),
                    Some(missing.as_os_str().into()),
                    &mut io::empty(),
                    &mut output,
                    &mut errors,
                );
                assert_eq!(code, 2, "{args:?}: {}", String::from_utf8_lossy(&errors));
                assert!(!missing.exists());
                assert!(!export.exists());
                assert!(output.is_empty());
            }
        }
        for flag in ["--secret", "--normal", "--protection"] {
            let mut output = Vec::new();
            let mut errors = Vec::new();
            assert_eq!(
                run(
                    ["input", "set", "node", "secret", "--stdin", flag]
                        .map(OsString::from)
                        .to_vec(),
                    Some(missing.as_os_str().into()),
                    &mut io::empty(),
                    &mut output,
                    &mut errors
                ),
                2
            );
            assert!(!missing.exists());
        }
    }

    // Test-ID: PR-TEST-0599
    // Verifies: PR-REQ-0031, PR-REQ-0032, PR-REQ-0115, PR-REQ-0265
    #[test]
    fn initial_input_acquisition_failure_is_atomic_at_the_public_create_boundary() {
        let (temporary, root, source) = cli_roots();
        let manifest = source.join("pactrun.yaml");
        let text = fs::read_to_string(&manifest).unwrap();
        fs::write(
            &manifest,
            text.replace("    - id: config", "    - id: config\n      required: true"),
        )
        .unwrap();
        let revision = json_install(&root, &source);
        let good = temporary.path().join("opaque.json");
        fs::write(&good, [0, 255, 13, 10]).unwrap();
        let missing = temporary.path().join("missing-secret");
        let args = vec![
            "instance".into(),
            "create".into(),
            "atomic-create".into(),
            "--revision".into(),
            revision.clone().into(),
            "--input-file".into(),
            format!("config={}", good.display()).into(),
            "--input-file".into(),
            format!("secret={}", missing.display()).into(),
        ];
        let (code, _, _) = json_invoke(&root, args);
        assert_eq!(code, 1);
        assert!(
            PactrunApplication::open(&root)
                .unwrap()
                .list_instances()
                .unwrap()
                .is_empty()
        );
        let database = rusqlite::Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
        for table in [
            "instances",
            "managed_input_bindings",
            "managed_input_payloads",
            "runs",
        ] {
            let count: i64 = database
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "partial create in {table}");
        }
        drop(database);
        let incomplete = json_ok(
            &root,
            &["instance", "create", "incomplete", "--revision", &revision],
        );
        assert_eq!(incomplete["required_inputs_satisfied"], false);
        assert_eq!(
            json_ok(&root, &["instance", "show", "incomplete"])["instance_id"],
            incomplete["instance_id"]
        );
    }

    struct FailingCancellationHandler;

    impl CancellationHandlerInstaller for FailingCancellationHandler {
        fn install(&self, _cancellation: ActionCancellation) -> Result<(), ()> {
            Err(())
        }
    }

    struct PrefixFailWriter {
        bytes: Vec<u8>,
        remaining: usize,
    }

    impl Write for PrefixFailWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"));
            }
            let written = self.remaining.min(bytes.len());
            self.bytes.extend_from_slice(&bytes[..written]);
            self.remaining -= written;
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn cli_roots() -> (TempDir, PathBuf, PathBuf) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/input-cli-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("cli-")
            .tempdir_in(parent)
            .unwrap();
        let storage = temporary.path().join("storage");
        let source = temporary.path().join("source");
        fs::create_dir(&storage).unwrap();
        fs::create_dir(storage.join("database")).unwrap();
        fs::create_dir(storage.join("runtime-content")).unwrap();
        fs::create_dir(storage.join("staging")).unwrap();
        fs::create_dir(&source).unwrap();
        fs::write(
            source.join("pactrun.yaml"),
            r#"source_format: 1.0-alpha.2
package_id: 00000000000000000000000000000021
revision:
  inputs:
    - id: config
    - id: secret
      protection: secret
  actions: []
  migrations: []
runtime_content:
  files: []
"#,
        )
        .unwrap();
        (temporary, storage, source)
    }

    fn cli_action_roots() -> (TempDir, PathBuf, PathBuf) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/invoke-cli-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("cli-action-")
            .tempdir_in(parent)
            .unwrap();
        let storage = temporary.path().join("storage");
        let source = temporary.path().join("source");
        fs::create_dir(&storage).unwrap();
        fs::create_dir(storage.join("database")).unwrap();
        fs::create_dir(storage.join("runtime-content")).unwrap();
        fs::create_dir(storage.join("staging")).unwrap();
        fs::create_dir(&source).unwrap();
        let worker_name = if cfg!(windows) {
            "worker.exe"
        } else {
            "worker"
        };
        fs::copy(env::current_exe().unwrap(), source.join(worker_name)).unwrap();
        fs::write(
            source.join("pactrun.yaml"),
            format!(
                r#"source_format: 1.0-alpha.2
package_id: 00000000000000000000000000000036
revision:
  inputs: []
  actions:
    - id: inspect
      access: observe
      parameters:
        - {{ id: value, type: string, sensitive: false }}
      hook:
        protocol_version: 1.0-alpha.1
        launch: {{ kind: direct, executable: worker }}
        args: []
        io: {{ terminal: none }}
      outputs: []
  migrations: []
runtime_content:
  files:
    - {{ id: worker, source: {worker_name}, path: bin/{worker_name}, executable: true }}
"#
            ),
        )
        .unwrap();
        (temporary, storage, source)
    }

    fn cli_hook_roots() -> (TempDir, PathBuf, PathBuf) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/invoke-cli-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("cli-hook-")
            .tempdir_in(parent)
            .unwrap();
        let storage = temporary.path().join("storage");
        let source = temporary.path().join("source");
        fs::create_dir(&storage).unwrap();
        fs::create_dir(storage.join("database")).unwrap();
        fs::create_dir(storage.join("runtime-content")).unwrap();
        fs::create_dir(storage.join("staging")).unwrap();
        fs::create_dir(&source).unwrap();
        let worker_name = if cfg!(windows) {
            "worker.exe"
        } else {
            "worker"
        };
        fs::copy(env::current_exe().unwrap(), source.join(worker_name)).unwrap();
        fs::write(
            source.join("pactrun.yaml"),
            format!(
                r#"source_format: 1.0-alpha.2
package_id: 00000000000000000000000000000037
revision:
  inputs:
    - {{ id: secret_config, required: true, protection: secret }}
  actions:
    - id: execute
      access: observe
      parameters:
        - {{ id: mode, type: string, sensitive: false }}
        - {{ id: marker, type: string, sensitive: false }}
        - {{ id: expected_binding, type: string, sensitive: true }}
        - {{ id: sensitive_value, type: string, sensitive: true }}
      hook:
        protocol_version: 1.0-alpha.1
        launch: {{ kind: direct, executable: worker }}
        args: ["--exact", "hook::tests::hook_protocol_worker", "--nocapture", "--test-threads=1", "hook-fixture-argument-tail"]
        io: {{ terminal: none }}
      outputs: [{{ id: report }}]
  migrations: []
runtime_content:
  files:
    - {{ id: worker, source: {worker_name}, path: bin/{worker_name}, executable: true }}
"#
            ),
        )
        .unwrap();
        (temporary, storage, source)
    }

    fn prepared_cli_hook_instance() -> (TempDir, PathBuf, PathBuf, PathBuf) {
        prepared_cli_hook_instance_with_terminal("none")
    }

    fn prepared_cli_hook_instance_with_terminal(
        terminal: &str,
    ) -> (TempDir, PathBuf, PathBuf, PathBuf) {
        let (temporary, storage, source) = cli_hook_roots();
        let manifest = source.join("pactrun.yaml");
        let text = fs::read_to_string(&manifest)
            .unwrap()
            .replace("terminal: none", &format!("terminal: {terminal}"));
        fs::write(manifest, text).unwrap();
        let storage_env = Some(storage.as_os_str().to_owned());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(
                vec![
                    "--format".into(),
                    "json".into(),
                    "pack".into(),
                    "install".into(),
                    source.into_os_string()
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let revision = installed_reference(&stdout);
        let expected_path = temporary.path().join("expected-binding.txt");
        let sensitive_path = temporary.path().join("sensitive-value.txt");
        fs::write(&expected_path, b"hook-fixture-secret-binding").unwrap();
        fs::write(&sensitive_path, b"hook-fixture-sensitive-parameter").unwrap();
        assert_eq!(
            run(
                vec![
                    "instance".into(),
                    "create".into(),
                    "node".into(),
                    "--revision".into(),
                    revision.into(),
                    "--input-file".into(),
                    format!("secret_config={}", expected_path.display()).into(),
                ],
                storage_env,
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        (temporary, storage, expected_path, sensitive_path)
    }

    fn successful_invoke_args(
        marker: &Path,
        expected_path: &Path,
        sensitive_path: &Path,
    ) -> Vec<OsString> {
        vec![
            "invoke".into(),
            "node".into(),
            "execute".into(),
            "--param".into(),
            "mode=success".into(),
            "--param".into(),
            format!("marker={}", marker.display()).into(),
            "--param-file".into(),
            format!("expected_binding={}", expected_path.display()).into(),
            "--param-file".into(),
            format!("sensitive_value={}", sensitive_path.display()).into(),
        ]
    }

    // Test-ID: PR-TEST-0079
    // Verifies: PR-REQ-0263, PR-REQ-0265, PR-REQ-0271, PR-REQ-0272
    #[test]
    fn closed_cli_references_and_storage_root_boundary_are_exact() {
        assert!(matches!(
            parse_command(vec!["instance".into(), "show".into(), "Node".into()]).unwrap(),
            Command::ShowInstance { .. }
        ));
        assert!(
            parse_command(vec![
                "instance".into(),
                "show".into(),
                "00".into(),
                "extra".into()
            ])
            .is_err()
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::{OsStrExt, OsStringExt};

            let (_, path) = split_initial_input_file(OsString::from_vec(
                b"config=non-unicode-\xff.bin".to_vec(),
            ))
            .unwrap();
            assert_eq!(path.as_os_str().as_bytes(), b"non-unicode-\xff.bin");
        }
        assert!(parse_revision_reference("bare".to_owned()).is_err());
        assert!(
            parse_revision_reference(format!(
                "exact:{}/sha256:{}",
                "00".repeat(16),
                "11".repeat(32)
            ))
            .is_ok()
        );
        assert!(
            parse_command(vec![
                "instance".into(),
                "create".into(),
                "node".into(),
                "--revision".into(),
                format!("exact:{}/sha256:{}", "00".repeat(16), "11".repeat(32)).into(),
                "--input-file".into(),
                "config=missing-a".into(),
                "--input-file".into(),
                "config=missing-b".into(),
            ])
            .is_err()
        );

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(
                vec!["pack".into(), "generate-id".into()],
                None,
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0
        );
        assert_eq!(stdout.len(), 33);
        stdout.clear();
        assert_eq!(
            run(
                vec!["instance".into(), "list".into()],
                None,
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            1
        );
    }

    // Test-ID: PR-TEST-0114
    // Verifies: PR-REQ-0097, PR-REQ-0114, PR-REQ-0284, PR-REQ-0285
    #[test]
    fn commands_and_run_projection_are_structural_and_redacted() {
        assert!(matches!(
            parse_command(vec!["action".into(), "list".into(), "node".into()]).unwrap(),
            Command::ListActions { .. }
        ));
        assert!(matches!(
            parse_command(vec![
                "invoke".into(),
                "node".into(),
                "deploy".into(),
                "--plan".into(),
                "--authorize-recovery-override".into(),
            ])
            .unwrap(),
            Command::Invoke {
                plan: true,
                recovery_override: true,
                ..
            }
        ));
        assert!(matches!(
            parse_command(vec!["run".into(), "reconcile".into()]).unwrap(),
            Command::ReconcileRuns
        ));
        assert!(matches!(
            parse_command(vec![
                "instance".into(),
                "resolve-manual-recovery".into(),
                "node".into(),
            ])
            .unwrap(),
            Command::ResolveManualRecovery { expected: None, .. }
        ));

        let run = RunId::from_bytes([1; 16]);
        let view = crate::domain::RunInspectionData {
            run: crate::domain::RunView {
                id: run,
                instance: crate::domain::InstanceId::from_bytes([2; 16]),
                accepted_state_version: crate::domain::InstanceStateVersion::from_bytes([3; 16]),
                accepted_at_unix_ms: 10,
                action: crate::domain::ActionRunIdentity {
                    revision: RevisionIdentity::new(
                        PackageId::from_bytes([4; 16]),
                        RevisionContentDigest::from_bytes([5; 32]),
                    ),
                    action: ActionIdentity::parse("deploy").unwrap(),
                },
                state: crate::domain::RunState::Finished(crate::domain::RunOutcomeView {
                    outcome: RunOutcome::Failed,
                    boundary: crate::domain::ActionRunBoundary::Admitted,
                    terminal_risk: crate::domain::RecoveryRiskState::Clear,
                    finished_at_unix_ms: 20,
                    primary_failure: Some(crate::domain::RunPrimaryFailure {
                        cause: None,
                        failure: crate::domain::RunFailureRecord {
                            error: crate::domain::PactrunErrorRef::new(
                                "execution",
                                "launch_failed",
                            )
                            .unwrap(),
                            message: "persisted-secret-message".to_owned(),
                        },
                        step: crate::domain::RunFailedStep::Plan(ActionPlanStep::LaunchHook),
                    }),
                    secondary_failures: vec![crate::domain::RunFailureRecord {
                        error: crate::domain::PactrunErrorRef::new(
                            "execution",
                            "workspace_cleanup_failed",
                        )
                        .unwrap(),
                        message: "another-persisted-secret-message".to_owned(),
                    }],
                    hook_completion: Some(crate::domain::HookCompletionRecord {
                        status: crate::domain::HookCompletionStatus::Failure,
                        code: Some(crate::domain::HookCodeV1::parse("hook_code").unwrap()),
                        message: Some("hook-free-text-secret".to_owned()),
                    }),
                    artifacts: vec![crate::domain::RunArtifactSummary {
                        output: crate::domain::ManagedOutputIdentity::parse("report").unwrap(),
                        byte_length: 7,
                    }],
                }),
            },
            current_recovery_guard: None,
        };
        let mut output = Vec::new();
        write_run(&mut output, &view).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("primary_failure: execution:launch_failed"));
        assert!(output.contains("hook_completion_text: withheld"));
        for secret in [
            "persisted-secret-message",
            "another-persisted-secret-message",
            "hook_code",
            "hook-free-text-secret",
        ] {
            assert!(
                !output.contains(secret),
                "structural Run output leaked {secret}"
            );
        }
    }

    // Test-ID: PR-TEST-0123
    // Verifies: PR-REQ-0049, PR-REQ-0286
    #[test]
    fn cancellation_handler_installation_failure_precedes_storage_and_acceptance() {
        let temporary = tempfile::tempdir().unwrap();
        let storage = temporary.path().join("storage");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let status = run_with_handler_installer(
            &FailingCancellationHandler,
            vec![
                "invoke".into(),
                "node".into(),
                "deploy".into(),
                "--param".into(),
                "secret=must-not-appear".into(),
            ],
            Some(storage.as_os_str().to_owned()),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(status, 1);
        assert_eq!(stdout, Vec::<u8>::new());
        assert!(String::from_utf8_lossy(&stderr).contains(CANCELLATION_HANDLER_FAILURE));
        assert!(!String::from_utf8_lossy(&stderr).contains("must-not-appear"));
        assert!(!storage.exists(), "handler failure must not open storage");
    }

    // Test-ID: PR-TEST-0116
    // Verifies: PR-REQ-0135, PR-REQ-0288
    #[test]
    fn parameter_sources_preserve_exact_text_and_protected_channel() {
        let command = parse_command(vec![
            "invoke".into(),
            "node".into(),
            "deploy".into(),
            "--param".into(),
            "ordinary=a=b".into(),
            "--param-file".into(),
            "protected=host=path".into(),
            "--startup-timeout-ms".into(),
            "0".into(),
        ])
        .unwrap();
        let Command::Invoke { parameters, .. } = command else {
            panic!("expected Invoke");
        };
        assert!(matches!(
            &parameters[0],
            ParameterSourceSpec::Text(id, value)
                if id.as_str() == "ordinary" && value == "a=b"
        ));
        assert!(matches!(
            &parameters[1],
            ParameterSourceSpec::File(id, path)
                if id.as_str() == "protected" && path == Path::new("host=path")
        ));

        let temporary = tempfile::tempdir().unwrap();
        let file = temporary.path().join("parameter.txt");
        fs::write(&file, b"\xEF\xBB\xBF protected\n").unwrap();
        let protected = ParameterIdentity::parse("protected").unwrap();
        let mut stdin = io::Cursor::new(b"stdin\n".to_vec());
        let values = read_parameter_sources(
            &[
                ParameterSourceSpec::File(protected.clone(), file),
                ParameterSourceSpec::Stdin(ParameterIdentity::parse("from_stdin").unwrap()),
            ],
            &mut stdin,
            &ActionCancellation::default(),
        )
        .unwrap();
        assert_eq!(values[0].text, "\u{feff} protected\n");
        assert_eq!(values[1].text, "stdin\n");
        assert!(matches!(
            values[0].source,
            crate::domain::ParameterTextSource::Protected
        ));
        assert!(matches!(
            values[1].source,
            crate::domain::ParameterTextSource::Protected
        ));
    }

    // Test-ID: PR-TEST-0130
    // Verifies: PR-REQ-0286, PR-REQ-0288
    #[test]
    fn invoke_rejects_unrepresentable_deadlines_before_run_acceptance() {
        let (_temporary, storage, _expected_path, _sensitive_path) = prepared_cli_hook_instance();
        let storage_env = Some(storage.as_os_str().to_owned());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let status = run(
            vec![
                "invoke".into(),
                "node".into(),
                "execute".into(),
                "--startup-timeout-ms".into(),
                u64::MAX.to_string().into(),
            ],
            storage_env.clone(),
            &mut io::empty(),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(status, 2);
        assert!(String::from_utf8_lossy(&stderr).contains("cannot be represented"));
        stdout.clear();
        stderr.clear();
        assert_eq!(
            run(
                vec![
                    "run".into(),
                    "list".into(),
                    "node".into(),
                    "--no-trunc".into()
                ],
                storage_env,
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0
        );
        assert!(
            json_ok(&storage, &["run", "list"])["items"]
                .as_array()
                .unwrap()
                .is_empty(),
            "invalid timeout must create no Run"
        );
    }

    // Test-ID: PR-TEST-0118
    // Verifies: PR-REQ-0284, PR-REQ-0287
    #[test]
    fn recovery_commands_have_explicit_operator_spellings() {
        let expected = InstanceStateVersion::from_bytes([9; 16]).to_string();
        assert!(matches!(
            parse_command(vec![
                "instance".into(),
                "resolve-manual-recovery".into(),
                "node".into(),
                "--if-version".into(),
                expected.into(),
            ])
            .unwrap(),
            Command::ResolveManualRecovery {
                expected: Some(_),
                ..
            }
        ));
        assert!(parse_command(vec!["run".into(), "reconcile".into(), "--force".into(),]).is_err());
    }

    // Test-ID: PR-TEST-0120
    // Verifies: PR-REQ-0095, PR-REQ-0284
    #[test]
    fn human_action_and_plan_commands_use_the_read_only_path() {
        let (_temporary, storage, source) = cli_action_roots();
        let storage_env = Some(storage.as_os_str().to_owned());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(
                vec![
                    "--format".into(),
                    "json".into(),
                    "pack".into(),
                    "install".into(),
                    source.into_os_string()
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let revision = installed_reference(&stdout);
        stdout.clear();
        stderr.clear();
        assert_eq!(
            run(
                vec![
                    "instance".into(),
                    "create".into(),
                    "node".into(),
                    "--revision".into(),
                    revision.into(),
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        stdout.clear();
        stderr.clear();
        assert_eq!(
            run(
                vec!["action".into(), "list".into(), "node".into()],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0
        );
        assert!(String::from_utf8_lossy(&stdout).contains("ACTION"));
        assert!(
            String::from_utf8_lossy(&stdout)
                .lines()
                .any(|line| line.starts_with("inspect  "))
        );
        stdout.clear();
        let before_database = fs::read(storage.join("database/pactrun.sqlite3")).unwrap();
        let mut before_staging = fs::read_dir(storage.join("staging"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        before_staging.sort();
        assert_eq!(
            run(
                vec![
                    "invoke".into(),
                    "node".into(),
                    "inspect".into(),
                    "--param".into(),
                    "value=exact text".into(),
                    "--plan".into(),
                ],
                storage_env,
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let plan = String::from_utf8_lossy(&stdout);
        assert!(plan.contains("Mode: preview"));
        assert!(plan.contains("Launch"));
        let typed = json_ok(
            &storage,
            &[
                "invoke",
                "node",
                "inspect",
                "--param",
                "value=exact text",
                "--plan",
            ],
        );
        assert!(
            typed["steps"]
                .as_array()
                .unwrap()
                .iter()
                .any(|step| step == "launch_hook")
        );
        assert!(!plan.contains("exact text"));
        assert_eq!(
            fs::read(storage.join("database/pactrun.sqlite3")).unwrap(),
            before_database
        );
        let mut after_staging = fs::read_dir(storage.join("staging"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        after_staging.sort();
        assert_eq!(after_staging, before_staging);
    }

    // Test-ID: PR-TEST-0119
    // Verifies: PR-REQ-0094, PR-REQ-0284
    #[test]
    fn human_invoke_creates_and_finishes_a_managed_run() {
        let (temporary, storage, source) = cli_hook_roots();
        let storage_env = Some(storage.as_os_str().to_owned());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(
                vec![
                    "--format".into(),
                    "json".into(),
                    "pack".into(),
                    "install".into(),
                    source.into_os_string()
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let revision = installed_reference(&stdout);
        stdout.clear();
        stderr.clear();
        let expected_path = temporary.path().join("expected-binding.txt");
        fs::write(&expected_path, b"hook-fixture-secret-binding").unwrap();
        assert_eq!(
            run(
                vec![
                    "instance".into(),
                    "create".into(),
                    "node".into(),
                    "--revision".into(),
                    revision.into(),
                    "--input-file".into(),
                    format!("secret_config={}", expected_path.display()).into(),
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );

        let marker = temporary.path().join("invoke-marker");
        let sensitive_path = temporary.path().join("sensitive-value.txt");
        fs::write(&sensitive_path, b"hook-fixture-sensitive-parameter").unwrap();
        stdout.clear();
        stderr.clear();
        assert_eq!(
            run(
                vec![
                    "invoke".into(),
                    "node".into(),
                    "execute".into(),
                    "--param".into(),
                    "mode=success".into(),
                    "--param".into(),
                    format!("marker={}", marker.display()).into(),
                    "--param-file".into(),
                    format!("expected_binding={}", expected_path.display()).into(),
                    "--param-file".into(),
                    format!("sensitive_value={}", sensitive_path.display()).into(),
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let summary = String::from_utf8_lossy(&stderr);
        let run_id = summary
            .lines()
            .find_map(|line| line.strip_prefix("Run: "))
            .expect("invoke prints the completed RunId")
            .to_owned();
        assert!(!summary.contains("hook-fixture-sensitive-parameter"));
        assert!(!summary.contains("hook-fixture-secret-binding"));
        stdout.clear();
        stderr.clear();
        assert_eq!(
            run(
                vec![
                    "run".into(),
                    "list".into(),
                    "node".into(),
                    "--no-trunc".into()
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0
        );
        assert!(String::from_utf8_lossy(&stdout).contains(&run_id));
        stdout.clear();
        assert_eq!(
            run(
                vec!["run".into(), "show".into(), run_id.into()],
                storage_env,
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0
        );
        assert!(String::from_utf8_lossy(&stdout).contains("Outcome: succeeded"));
    }

    // Test-ID: PR-TEST-0124
    // Verifies: PR-REQ-0052, PR-REQ-0286
    #[test]
    fn broken_final_diagnostic_writer_does_not_change_finished_run_ownership() {
        let (_temporary, storage, source) = cli_hook_roots();
        let storage_env = Some(storage.as_os_str().to_owned());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(
                vec![
                    "--format".into(),
                    "json".into(),
                    "pack".into(),
                    "install".into(),
                    source.into_os_string()
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let revision = installed_reference(&stdout);
        let expected_path = storage.parent().unwrap().join("expected-binding.txt");
        let sensitive_path = storage.parent().unwrap().join("sensitive-value.txt");
        fs::write(&expected_path, b"hook-fixture-secret-binding").unwrap();
        fs::write(&sensitive_path, b"hook-fixture-sensitive-parameter").unwrap();
        stdout.clear();
        stderr.clear();
        assert_eq!(
            run(
                vec![
                    "instance".into(),
                    "create".into(),
                    "node".into(),
                    "--revision".into(),
                    revision.into(),
                    "--input-file".into(),
                    format!("secret_config={}", expected_path.display()).into(),
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );

        let marker = storage.parent().unwrap().join("broken-diagnostic-marker");
        let mut broken_stderr = PrefixFailWriter {
            bytes: Vec::new(),
            remaining: 0,
        };
        assert_eq!(
            run(
                vec![
                    "invoke".into(),
                    "node".into(),
                    "execute".into(),
                    "--param".into(),
                    "mode=success".into(),
                    "--param".into(),
                    format!("marker={}", marker.display()).into(),
                    "--param-file".into(),
                    format!("expected_binding={}", expected_path.display()).into(),
                    "--param-file".into(),
                    format!("sensitive_value={}", sensitive_path.display()).into(),
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut broken_stderr,
            ),
            1
        );
        assert!(marker.with_extension("session").exists());

        stdout.clear();
        stderr.clear();
        assert_eq!(
            run(
                vec![
                    "run".into(),
                    "list".into(),
                    "node".into(),
                    "--no-trunc".into()
                ],
                storage_env,
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let listing = String::from_utf8_lossy(&stdout);
        assert!(
            listing
                .lines()
                .skip(1)
                .any(|line| line.ends_with("  succeeded"))
        );
    }

    // Test-ID: PR-TEST-0126
    // Verifies: PR-REQ-0049, PR-REQ-0286
    #[test]
    fn admission_retry_keeps_the_exact_run_with_a_broken_diagnostic_writer() {
        let (_temporary, storage, expected_path, sensitive_path) = prepared_cli_hook_instance();
        let storage_env = Some(storage.as_os_str().to_owned());
        crate::executor::fail_next_admission_for_test();
        let marker = storage.parent().unwrap().join("admission-retry-marker");
        let mut stdout = Vec::new();
        let mut broken_stderr = PrefixFailWriter {
            bytes: Vec::new(),
            remaining: 0,
        };
        assert_eq!(
            run(
                successful_invoke_args(&marker, &expected_path, &sensitive_path),
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut broken_stderr,
            ),
            1
        );
        assert!(marker.with_extension("session").exists());

        let mut listing = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(
                vec![
                    "run".into(),
                    "list".into(),
                    "node".into(),
                    "--no-trunc".into()
                ],
                storage_env,
                &mut io::empty(),
                &mut listing,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let listing = String::from_utf8_lossy(&listing);
        let rows: Vec<_> = listing
            .lines()
            .skip(1)
            .take_while(|line| !line.is_empty())
            .collect();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].ends_with("  succeeded"));
    }

    // Test-ID: PR-TEST-0127
    // Verifies: PR-REQ-0052, PR-REQ-0286
    #[test]
    fn repeated_finalization_failures_keep_owner_until_terminal_commit() {
        let (_temporary, storage, expected_path, sensitive_path) = prepared_cli_hook_instance();
        let storage_env = Some(storage.as_os_str().to_owned());
        let faults = crate::application::fail_next_finalization_advances_for_test(3);
        let marker = storage.parent().unwrap().join("finalization-retry-marker");
        let mut stdout = Vec::new();
        let mut broken_stderr = PrefixFailWriter {
            bytes: Vec::new(),
            remaining: 0,
        };
        assert_eq!(
            run(
                successful_invoke_args(&marker, &expected_path, &sensitive_path),
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut broken_stderr,
            ),
            1
        );
        assert_eq!(faults.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(marker.with_extension("session").exists());

        let runs = json_ok(&storage, &["run", "list", "node"]);
        assert_eq!(runs["items"].as_array().unwrap().len(), 1);
        assert_eq!(runs["items"][0]["state"]["phase"], "finished");
        assert_eq!(runs["items"][0]["state"]["outcome"], "succeeded");
    }

    // Test-ID: PR-TEST-0078
    // Verifies: PR-REQ-0092, PR-REQ-0119, PR-REQ-0263, PR-REQ-0266, PR-REQ-0268, PR-REQ-0271
    #[test]
    fn cli_install_instance_secret_export_and_atomic_no_clobber_are_end_to_end() {
        let (_temporary, storage, source) = cli_roots();
        let storage_env = Some(storage.as_os_str().to_owned());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(
                vec![
                    "--format".into(),
                    "json".into(),
                    "pack".into(),
                    "install".into(),
                    source.into_os_string()
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let revision = installed_reference(&stdout);
        assert!(revision.starts_with("exact:"));

        let config_path = storage.parent().unwrap().join("config.bin");
        fs::write(&config_path, b"configuration").unwrap();
        stdout.clear();
        stderr.clear();
        assert_eq!(
            run(
                vec![
                    "instance".into(),
                    "create".into(),
                    "node".into(),
                    "--revision".into(),
                    revision.into(),
                    "--input-file".into(),
                    format!("config={}", config_path.display()).into(),
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        assert!(!String::from_utf8_lossy(&stdout).contains("configuration"));
        stdout.clear();
        stderr.clear();
        assert_eq!(
            run(
                vec!["instance".into(), "show".into(), "Node".into()],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            1
        );

        stdout.clear();
        stderr.clear();
        assert_eq!(
            run(
                vec![
                    "input".into(),
                    "set".into(),
                    "node".into(),
                    "secret".into(),
                    "--stdin".into(),
                ],
                storage_env.clone(),
                &mut io::Cursor::new(b"top-secret"),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );

        stdout.clear();
        stderr.clear();
        assert_eq!(
            run(
                vec![
                    "input".into(),
                    "export".into(),
                    "node".into(),
                    "secret".into(),
                    "--output".into(),
                    "-".into(),
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            1
        );
        assert!(stdout.is_empty());

        stdout.clear();
        stderr.clear();
        assert_eq!(
            run(
                vec![
                    "input".into(),
                    "export".into(),
                    "node".into(),
                    "secret".into(),
                    "--output".into(),
                    "-".into(),
                    "--authorize-secret-export".into(),
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        assert_eq!(stdout, b"top-secret");

        let mut partial = PrefixFailWriter {
            bytes: Vec::new(),
            remaining: 3,
        };
        stderr.clear();
        assert_eq!(
            run(
                vec![
                    "input".into(),
                    "export".into(),
                    "node".into(),
                    "secret".into(),
                    "--output".into(),
                    "-".into(),
                    "--authorize-secret-export".into(),
                ],
                storage_env.clone(),
                &mut io::empty(),
                &mut partial,
                &mut stderr,
            ),
            1
        );
        assert_eq!(partial.bytes, b"top");

        let output = storage.parent().unwrap().join("export.bin");
        stdout.clear();
        stderr.clear();
        let export_args = vec![
            "input".into(),
            "export".into(),
            "node".into(),
            "secret".into(),
            "--output".into(),
            output.as_os_str().to_owned(),
            "--authorize-secret-export".into(),
        ];
        assert_eq!(
            run(
                export_args.clone(),
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        assert_eq!(fs::read(&output).unwrap(), b"top-secret");
        assert_eq!(
            run(
                export_args,
                storage_env,
                &mut io::empty(),
                &mut Vec::new(),
                &mut Vec::new(),
            ),
            1
        );
        assert_eq!(fs::read(&output).unwrap(), b"top-secret");

        let staging = crate::managed_data::StagingSession::open(&storage).unwrap();
        let staged = staging
            .stage_managed_input(&mut io::Cursor::new(b"publication-window"))
            .unwrap();
        let before = storage.parent().unwrap().join("before-publication.bin");
        assert!(
            publish_output_file_with_fault(&staged, &before, |point| {
                if point == OutputPublicationPoint::BeforeFinalNamePublication {
                    Err(CliError::operation("injected pre-publication failure"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        assert!(!before.exists());
        assert!(
            !fs::read_dir(before.parent().unwrap())
                .unwrap()
                .any(|entry| {
                    entry
                        .ok()
                        .and_then(|entry| entry.file_name().into_string().ok())
                        .is_some_and(|name| {
                            name.starts_with(".pactrun-export-") && name.ends_with(".tmp")
                        })
                })
        );

        let after = storage.parent().unwrap().join("after-publication.bin");
        assert!(
            publish_output_file_with_fault(&staged, &after, |point| {
                if point == OutputPublicationPoint::AfterFinalNamePublication {
                    Err(CliError::operation("injected post-publication failure"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        assert_eq!(fs::read(after).unwrap(), b"publication-window");
    }
}
