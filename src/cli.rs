//! Closed M2 human CLI parsing, composition, and terminal presentation.

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
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

use lexopt::{Arg, Parser};
mod migrations;
mod service_storage;
mod snapshots;

use crate::{
    application::{ApplicationError, InputAcquisition, MigrationRelationState, PactrunApplication},
    domain::{
        ActionExecutionPlan, ActionIdentity, ActionPlanStep, ActionRunBoundary, ActionV1,
        CompiledHookLaunch, HookLaunchV1, InputIdentity, InstanceName, InstanceStateVersion,
        LocalAlias, ManagedInputProtection, ManagedInputRole, OperationAccessV1, PackageId,
        ParameterIdentity, ParameterTypeV1, RawParameterInput, ReferenceLabel,
        RevisionContentDigest, RevisionIdentity, RevisionMetadataMutationBatch, RunId, RunOutcome,
        RunState,
    },
    executor::{AdmissionOptions, AdmittedExecution, ExecutorError},
    hook::{ActionCancellation, HookRuntimePolicy},
    managed_data::StagedFile,
};

const STORAGE_ROOT_ENV: &str = "PACTRUN_STORAGE_ROOT";
const HELP: &str = "Pactrun 0.1.0\n\
Usage:\n\
  pactrun pack generate-id\n\
  pactrun pack install <source-root>\n\
  pactrun instance create <name> --revision <reference> [--input-file <id>=<path>]... [--input-stdin <id>]\n\
  pactrun instance list\n\
  pactrun instance show <instance>\n\
  pactrun instance migration-paths <instance> --to <reference> [--limit <1..100>] [--after <path-id>]\n\
  pactrun instance migrate <instance> --to <reference> [--plan] [--path <path-id>] [--input-file <target-digest>/<input-id>=<path>]... [--authorize-declassification] [--authorize-recovery-override] [--startup-timeout-ms <ms>] [--execution-timeout-ms <ms>] [--termination-grace-ms <ms>]\n\
  pactrun instance resolve-manual-recovery <instance> [--if-version <token>]\n\
  pactrun input list <instance>\n\
  pactrun service-storage list <instance> [--retained]\n\
  pactrun resource list <instance> [--retained]\n\
  pactrun resource show <instance> <resource-id> [--retained]\n\
  pactrun resource observe <instance> <resource-id> [--retained]\n\
  pactrun resource locate <instance> <resource-id> --intent <read|write> [--retained]\n\
  pactrun input set <instance> <input-id> (--file <path> | --stdin) [--if-version <token>]\n\
  pactrun input export <instance> <input-id> --output <path|-> [--authorize-secret-export]\n\
  pactrun input delete <instance> <input-id> [--if-version <token>]\n\
  pactrun action list <instance>\n\
  pactrun action show <instance> <action>\n\
  pactrun invoke <instance> <action> [options]\n\
  pactrun run list <instance>\n\
  pactrun run show <run-id>\n\
  pactrun run reconcile\n\
  pactrun snapshot capture <instance> [execution-options]\n\
  pactrun snapshot restore <instance> <snapshot-id> [execution-options]\n\
  pactrun snapshot list [--instance <instance>]\n\
  pactrun snapshot show <snapshot-id>\n\
  pactrun snapshot verify <snapshot-id>\n\
  pactrun snapshot import <bundle-path>\n\
  pactrun snapshot export <snapshot-id> --output <bundle-path> --authorize-sensitive-export\n\
  pactrun storage upgrade\n\
\n\
Snapshot execution options: --param, --param-file, --param-stdin, --plan, --authorize-recovery-override,\n\
  --startup-timeout-ms, --execution-timeout-ms, --termination-grace-ms.\n\
Omitted Snapshot startup/execution timeouts are unlimited; termination grace defaults to 5000ms.\n\
Snapshot bundles use filesystem paths only, never stdin/stdout.\n\
Migration supports declared paths, Hook edges and explicit per-target Input files.\n\
\n\
Revision references: label:<label>, alias:<alias>, or exact:<package-id>/sha256:<digest>.\n";

enum Command {
    ServiceStorage(service_storage::ServiceCommand),
    Migration(migrations::MigrationCommand),
    Snapshot(snapshots::SnapshotCommand),
    Help,
    Version,
    GeneratePackageId,
    Install {
        source_root: PathBuf,
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
        name: InstanceName,
        action: ActionIdentity,
        parameters: Vec<ParameterSourceSpec>,
        plan: bool,
        recovery_override: bool,
        startup_timeout_ms: Option<u64>,
        action_timeout_ms: Option<u64>,
        termination_grace_ms: Option<u64>,
    },
    ListRuns {
        name: InstanceName,
    },
    ShowRun {
        run: RunId,
    },
    ReconcileRuns,
    UpgradeStorage,
}

#[derive(Clone)]
enum RevisionReference {
    Label(ReferenceLabel),
    Alias(LocalAlias),
    Exact(RevisionIdentity),
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

struct ExecutionOptions {
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
    fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            usage: true,
        }
    }

    fn operation(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            usage: false,
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
    #[cfg(any(unix, windows))]
    if args
        .first()
        .is_some_and(|argument| argument == crate::hook::INTERACTIVE_ADAPTER_ARGUMENT)
    {
        return crate::hook::run_interactive_adapter(&args[1..]);
    }
    let storage_root = env::var_os(STORAGE_ROOT_ENV);
    let cancellation = ActionCancellation::default();
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    if SystemCancellationHandler
        .install(cancellation.clone())
        .is_err()
    {
        let _ = writeln!(stderr, "error: {CANCELLATION_HANDLER_FAILURE}");
        return 1;
    }
    if args.iter().any(argument_reads_stdin) {
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
    }
}

#[cfg(test)]
fn run_with_handler_installer<I: CancellationHandlerInstaller>(
    installer: &I,
    args: Vec<OsString>,
    storage_root: Option<OsString>,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let cancellation = ActionCancellation::default();
    if installer.install(cancellation.clone()).is_err() {
        let _ = writeln!(stderr, "error: {CANCELLATION_HANDLER_FAILURE}");
        return 1;
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
    receiver: Receiver<StdinReadMessage>,
    pending: Vec<u8>,
    pending_offset: usize,
    worker: Option<JoinHandle<()>>,
    cancellation: ActionCancellation,
    ended: bool,
}

impl CancellableStdin {
    fn new<R>(mut source: R, cancellation: ActionCancellation) -> Self
    where
        R: Read + Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
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
        });
        Self {
            receiver,
            pending: Vec::new(),
            pending_offset: 0,
            worker: Some(worker),
            cancellation,
            ended: false,
        }
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
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let cancellation = ActionCancellation::default();
    run_with_cancellation(args, storage_root, stdin, stdout, stderr, &cancellation)
}

fn run_with_cancellation(
    args: Vec<OsString>,
    storage_root: Option<OsString>,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    cancellation: &ActionCancellation,
) -> i32 {
    let command = match parse_command(args) {
        Ok(command) => command,
        Err(error) => {
            let _ = writeln!(stderr, "error: {error}");
            return 2;
        }
    };
    match execute(command, storage_root, stdin, stdout, stderr, cancellation) {
        Ok(()) => 0,
        Err(error) => {
            let _ = writeln!(stderr, "error: {error}");
            if error.usage { 2 } else { 1 }
        }
    }
}

fn parse_command(args: Vec<OsString>) -> Result<Command, CliError> {
    let mut parser = Parser::from_args(args);
    let first = match parser.next().map_err(lex_error)? {
        None => return Ok(Command::Help),
        Some(Arg::Long("help")) => {
            require_end(&mut parser)?;
            return Ok(Command::Help);
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
        "instance" => parse_instance(&mut parser),
        "input" => parse_input(&mut parser),
        "action" => parse_action(&mut parser),
        "invoke" => parse_invoke(&mut parser),
        "snapshot" => snapshots::parse(&mut parser).map(Command::Snapshot),
        "service-storage" => service_storage::parse(&mut parser, true).map(Command::ServiceStorage),
        "resource" => service_storage::parse(&mut parser, false).map(Command::ServiceStorage),
        "run" => parse_run(&mut parser),
        "storage" => {
            let operation = required_value_string(&mut parser, "storage command")?;
            if operation != "upgrade" {
                return Err(CliError::usage("unknown storage command"));
            }
            require_end(&mut parser)?;
            Ok(Command::UpgradeStorage)
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
            require_end(parser)?;
            Ok(Command::Install { source_root })
        }
        other => Err(CliError::usage(format!("unknown pack command {other:?}"))),
    }
}

fn parse_instance(parser: &mut Parser) -> Result<Command, CliError> {
    match required_value_string(parser, "instance command")?.as_str() {
        "migration-paths" => migrations::parse(parser, true).map(Command::Migration),
        "migrate" => migrations::parse(parser, false).map(Command::Migration),
        "create" => {
            let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
            let mut revision = None;
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
                    argument => return Err(CliError::usage(argument.unexpected().to_string())),
                }
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
    let mut parameters = Vec::new();
    let mut plan = false;
    let mut recovery_override = false;
    let mut startup_timeout_ms = None;
    let mut action_timeout_ms = None;
    let mut termination_grace_ms = None;
    while let Some(argument) = parser.next().map_err(lex_error)? {
        match argument {
            Arg::Long("param") => {
                let value = required_parser_value(parser, "parameter assignment")?;
                let (id, text) = split_parameter_assignment(value, "--param", true)?;
                parameters.push(ParameterSourceSpec::Text(
                    parse_parameter_id(&id)?,
                    os_string(text, "parameter text")?,
                ));
            }
            Arg::Long("param-file") => {
                let value = required_parser_value(parser, "parameter file assignment")?;
                let (id, path) = split_parameter_assignment(value, "--param-file", false)?;
                parameters.push(ParameterSourceSpec::File(
                    parse_parameter_id(&id)?,
                    PathBuf::from(path),
                ));
            }
            Arg::Long("param-stdin") => {
                parameters.push(ParameterSourceSpec::Stdin(parse_parameter_id(
                    &value_string(parser, "stdin parameter identity")?,
                )?));
            }
            Arg::Long("plan") if !plan => plan = true,
            Arg::Long("plan") => return Err(CliError::usage("duplicate --plan")),
            Arg::Long("authorize-recovery-override") if !recovery_override => {
                recovery_override = true;
            }
            Arg::Long("authorize-recovery-override") => {
                return Err(CliError::usage("duplicate --authorize-recovery-override"));
            }
            Arg::Long("startup-timeout-ms") => {
                set_once(
                    &mut startup_timeout_ms,
                    parse_timeout_ms(value_string(parser, "startup timeout")?)?,
                    "--startup-timeout-ms",
                )?;
            }
            Arg::Long(option) if option == timeout_option => {
                set_once(
                    &mut action_timeout_ms,
                    parse_timeout_ms(value_string(parser, "Action timeout")?)?,
                    timeout_option,
                )?;
            }
            Arg::Long("termination-grace-ms") => {
                set_once(
                    &mut termination_grace_ms,
                    parse_timeout_ms(value_string(parser, "termination grace")?)?,
                    "--termination-grace-ms",
                )?;
            }
            argument => return Err(CliError::usage(argument.unexpected().to_string())),
        }
    }
    validate_parameter_source_shape(&parameters)?;
    Ok(ExecutionOptions {
        parameters,
        plan,
        recovery_override,
        startup_timeout_ms,
        action_timeout_ms,
        termination_grace_ms,
    })
}

fn parse_run(parser: &mut Parser) -> Result<Command, CliError> {
    match required_value_string(parser, "run command")?.as_str() {
        "list" => {
            let name = parse_instance_name(required_value_string(parser, "Instance name")?)?;
            require_end(parser)?;
            Ok(Command::ListRuns { name })
        }
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
) -> Result<(), CliError> {
    match command {
        Command::Help => return stdout.write_all(HELP.as_bytes()).map_err(io_operation),
        Command::Version => {
            return writeln!(stdout, "pactrun {}", env!("CARGO_PKG_VERSION")).map_err(io_operation);
        }
        Command::GeneratePackageId => {
            let id = PackageId::generate()
                .map_err(|error| CliError::operation(format!("CSPRNG failed: {error}")))?;
            return writeln!(stdout, "{id}").map_err(io_operation);
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
    if let Command::Migration(command) = command {
        return migrations::execute(command, &storage_root, stdout, cancellation);
    }
    if let Command::Snapshot(command) = command {
        return snapshots::execute(command, &storage_root, stdin, stdout, stderr, cancellation);
    }
    if let Command::ServiceStorage(command) = command {
        return service_storage::execute(command, &storage_root, stdout);
    }
    if matches!(command, Command::UpgradeStorage) {
        if !storage_root.is_absolute() {
            return Err(CliError::operation("PACTRUN_STORAGE_ROOT must be absolute"));
        }
        let upgraded = crate::persistence::PactrunPersistence::upgrade_storage(&storage_root)
            .map_err(|error| CliError::operation(error.to_string()))?;
        return writeln!(
            stdout,
            "storage schema: V{} ({})",
            crate::persistence::SCHEMA_VERSION,
            if upgraded {
                "upgraded"
            } else {
                "already current"
            }
        )
        .map_err(io_operation);
    }
    let readonly = matches!(
        command,
        Command::ListActions { .. }
            | Command::ShowAction { .. }
            | Command::Invoke { plan: true, .. }
            | Command::ListRuns { .. }
            | Command::ShowRun { .. }
    );
    let application = if readonly {
        PactrunApplication::open_read_only(&storage_root)
    } else {
        PactrunApplication::open(&storage_root)
    }
    .map_err(app_error)?;
    match command {
        Command::Install { source_root } => {
            let empty = RevisionMetadataMutationBatch::new(Vec::new())
                .expect("an empty metadata batch is valid");
            let installed = application
                .install_pack_source(&source_root, &empty)
                .map_err(app_error)?;
            writeln!(stdout, "{}", format_revision(&installed.revision)).map_err(io_operation)?;
            for migration in installed.migrations {
                match migration.state {
                    MigrationRelationState::NotEvaluated => writeln!(
                        stdout,
                        "migration {}: not_evaluated",
                        migration.source_revision_digest.as_str()
                    ),
                    MigrationRelationState::Valid => writeln!(
                        stdout,
                        "migration {}: valid",
                        migration.source_revision_digest.as_str()
                    ),
                    MigrationRelationState::Invalid(reason) => writeln!(
                        stdout,
                        "migration {}: invalid: {reason}",
                        migration.source_revision_digest.as_str()
                    ),
                }
                .map_err(io_operation)?;
            }
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
            write_instance(stdout, &view)?;
        }
        Command::ListInstances => {
            for instance in application.list_instances().map_err(app_error)? {
                writeln!(
                    stdout,
                    "{}\t{}\t{}\t{}",
                    instance.name.as_str(),
                    format_revision(&instance.active_revision),
                    instance.state_version,
                    if instance.required_inputs_satisfied {
                        "ready"
                    } else {
                        "missing_required_inputs"
                    }
                )
                .map_err(io_operation)?;
            }
        }
        Command::ShowInstance { name } => {
            let (_, view) = resolve_instance(&application, &name)?;
            write_instance(stdout, &view)?;
        }
        Command::ResolveManualRecovery { name, expected } => {
            let (instance, view) = resolve_instance(&application, &name)?;
            let expected = expected.unwrap_or(view.state_version);
            let next = application
                .resolve_manual_recovery(instance, expected)
                .map_err(app_error)?;
            writeln!(stdout, "{next}").map_err(io_operation)?;
        }
        Command::ListInputs { name } => {
            let (_, view) = resolve_instance(&application, &name)?;
            write_inputs(stdout, &view)?;
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
                .set_input(instance, input, expected, source)
                .map_err(app_error)?;
            writeln!(stdout, "{version}").map_err(io_operation)?;
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
                writeln!(
                    stderr,
                    "exported {} at InstanceStateVersion {}",
                    input.as_str(),
                    observation.state_version
                )
                .map_err(io_operation)?;
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
            writeln!(stdout, "{version}").map_err(io_operation)?;
        }
        Command::ListActions { name } => {
            let (_, actions) = application
                .list_action_definitions(&name)
                .map_err(app_error)?;
            for action in &actions {
                write_action_summary(stdout, action)?;
            }
        }
        Command::ShowAction { name, action } => {
            let (_, action) = application
                .load_action_definition(&name, &action)
                .map_err(app_error)?;
            write_action_detail(stdout, &action)?;
        }
        Command::Invoke {
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
                write_plan(
                    stdout,
                    &compiled,
                    recovery_override,
                    startup_timeout_ms,
                    action_timeout_ms,
                    termination_grace_ms,
                )?;
                return Ok(());
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
                )? {
                    AdmissionRecovery::Admitted(admitted) => *admitted,
                    AdmissionRecovery::Terminal(run) => {
                        write_terminal_run_best_effort(&application, run, stderr);
                        return Err(CliError::operation(format!(
                            "Run {run} became terminal while Admission was being resolved"
                        )));
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
                        let _ = writeln!(
                            stderr,
                            "run {run}: finalization retry: durable state retained"
                        );
                        std::thread::sleep(std::time::Duration::from_millis(1_000));
                    }
                }
            }
            let inspection = loop {
                match application.load_run_inspection(run) {
                    Ok(Some(inspection))
                        if matches!(inspection.run.state, RunState::Finished(_)) =>
                    {
                        break inspection;
                    }
                    Ok(Some(_)) | Ok(None) | Err(_) => {
                        thread::sleep(Duration::from_millis(1_000));
                    }
                }
            };
            let _ = write_run(stderr, &inspection);
            match inspection.run.state {
                crate::domain::RunState::Finished(outcome)
                    if outcome.outcome == RunOutcome::Succeeded => {}
                crate::domain::RunState::Finished(outcome) => {
                    return Err(CliError::operation(format!(
                        "Run {} finished with outcome {}",
                        run,
                        format_outcome(outcome.outcome)
                    )));
                }
                crate::domain::RunState::Running(_) => {
                    return Err(CliError::operation("Run was not durably finished"));
                }
            }
        }
        Command::ListRuns { name } => {
            let (instance, _) = resolve_instance(&application, &name)?;
            for run in application.list_managed_runs(instance).map_err(app_error)? {
                snapshots::write_summary(stdout, &run)?;
            }
        }
        Command::ShowRun { run } => {
            let inspection = application
                .managed_run_inspection(run)
                .map_err(app_error)?
                .ok_or_else(|| CliError::operation("Run is not persisted"))?;
            snapshots::write_run(stdout, &inspection)?;
        }
        Command::ReconcileRuns => {
            for run in application
                .reconcile_lost_action_owners()
                .map_err(app_error)?
            {
                writeln!(stdout, "{run}").map_err(io_operation)?;
            }
        }
        Command::Help
        | Command::Version
        | Command::GeneratePackageId
        | Command::UpgradeStorage
        | Command::Migration(_)
        | Command::Snapshot(_)
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
        RevisionReference::Exact(identity) => {
            if application.revision_exists(&identity).map_err(app_error)? {
                Ok(identity)
            } else {
                Err(CliError::operation("exact Revision is not installed"))
            }
        }
        RevisionReference::Alias(alias) => application
            .resolve_local_alias(&alias)
            .map_err(app_error)?
            .ok_or_else(|| CliError::operation("local alias does not resolve")),
        RevisionReference::Label(label) => {
            let revisions = application
                .resolve_reference_label(&label)
                .map_err(app_error)?;
            match revisions.as_slice() {
                [revision] => Ok(revision.clone()),
                [] => Err(CliError::operation("reference label does not resolve")),
                _ => Err(CliError::operation("resolution.ambiguous_reference")),
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

fn format_phase(phase: crate::domain::RunPhase) -> &'static str {
    match phase {
        crate::domain::RunPhase::Running => "running",
        crate::domain::RunPhase::Finished => "finished",
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

fn format_path(path: &Path) -> String {
    match path.to_str() {
        Some(path) => path.chars().flat_map(char::escape_default).collect(),
        None => format_native_path(path),
    }
}

#[cfg(unix)]
fn format_native_path(path: &Path) -> String {
    let bytes = path
        .as_os_str()
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("unix-bytes[{bytes}]")
}

#[cfg(windows)]
fn format_native_path(path: &Path) -> String {
    let units = path
        .as_os_str()
        .encode_wide()
        .map(|unit| format!("{unit:04x}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("windows-utf16[{units}]")
}

fn format_failed_step(step: crate::domain::RunFailedStep) -> String {
    match step {
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

fn write_action_summary(output: &mut dyn Write, action: &ActionV1) -> Result<(), CliError> {
    writeln!(
        output,
        "action: {}\taccess: {}\tparameters: {}\toutputs: {}",
        action.id.as_str(),
        access_name(action.access),
        action.parameters.len(),
        action.outputs.len()
    )
    .map_err(io_operation)
}

fn write_action_detail(output: &mut dyn Write, action: &ActionV1) -> Result<(), CliError> {
    writeln!(output, "action: {}", action.id.as_str()).map_err(io_operation)?;
    writeln!(output, "access: {}", access_name(action.access)).map_err(io_operation)?;
    writeln!(
        output,
        "terminal: {}",
        terminal_name(action.hook.io.terminal)
    )
    .map_err(io_operation)?;
    writeln!(
        output,
        "protocol_version: {}",
        action.hook.protocol_version.get()
    )
    .map_err(io_operation)?;
    writeln!(output, "hook_args_count: {}", action.hook.args.len()).map_err(io_operation)?;
    match &action.hook.launch {
        HookLaunchV1::Direct { executable } => {
            writeln!(
                output,
                "launch: direct\tcontent_id: {}",
                executable.as_str()
            )
            .map_err(io_operation)?;
        }
        HookLaunchV1::Interpreter {
            command,
            interpreter_args,
            script,
        } => {
            writeln!(
                output,
                "launch: interpreter\tcommand: {}\tinterpreter_args_count: {}\tscript: {}",
                command.as_str(),
                interpreter_args.len(),
                script.as_str()
            )
            .map_err(io_operation)?;
        }
    }
    for parameter in &action.parameters {
        writeln!(
            output,
            "parameter: {}\ttype: {}\tsensitive: {}\tdefault: {}",
            parameter.id.as_str(),
            parameter_type_name(parameter.parameter_type),
            parameter.sensitive,
            if parameter.default.is_some() {
                "present"
            } else {
                "absent"
            }
        )
        .map_err(io_operation)?;
    }
    for output_definition in &action.outputs {
        writeln!(output, "output: {}", output_definition.id.as_str()).map_err(io_operation)?;
    }
    Ok(())
}

fn write_plan(
    output: &mut dyn Write,
    plan: &crate::domain::ActionExecutionPlan,
    recovery_override: bool,
    startup_timeout_ms: Option<u64>,
    action_timeout_ms: Option<u64>,
    termination_grace_ms: Option<u64>,
) -> Result<(), CliError> {
    writeln!(output, "action: {}", plan.action().as_str()).map_err(io_operation)?;
    writeln!(output, "instance_id: {}", plan.instance()).map_err(io_operation)?;
    writeln!(
        output,
        "revision: {}",
        format_revision(plan.active_revision())
    )
    .map_err(io_operation)?;
    writeln!(
        output,
        "expected_state_version: {}",
        plan.expected_state_version()
    )
    .map_err(io_operation)?;
    writeln!(output, "access: {}", access_name(plan.access())).map_err(io_operation)?;
    writeln!(
        output,
        "required_inputs_satisfied: {}",
        plan.required_inputs_satisfied()
    )
    .map_err(io_operation)?;
    writeln!(output, "terminal: {}", terminal_name(plan.terminal())).map_err(io_operation)?;
    writeln!(
        output,
        "protocol_version: {}",
        plan.protocol_version().get()
    )
    .map_err(io_operation)?;
    writeln!(output, "parameter_count: {}", plan.parameters().len()).map_err(io_operation)?;
    for parameter in plan.parameters() {
        writeln!(
            output,
            "parameter: {}\teffective_redaction: {}",
            parameter.id.as_str(),
            parameter.effective_redaction
        )
        .map_err(io_operation)?;
    }
    match plan.launch() {
        CompiledHookLaunch::Direct { executable } => {
            writeln!(
                output,
                "launch: direct\truntime_path: {}\texecutable: {}",
                executable.path.as_str(),
                executable.executable
            )
            .map_err(io_operation)?;
        }
        CompiledHookLaunch::Interpreter {
            launcher,
            interpreter_args,
            script,
        } => {
            writeln!(
                output,
                "launch: interpreter\tcommand: {}\tresolved_path: {}\tsearch_directories: {}\tinterpreter_args_count: {}\tscript: {}",
                launcher.command.as_str(),
                format_path(&launcher.resolved_absolute_path),
                launcher.search_directories.len(),
                interpreter_args.len(),
                script.path.as_str()
            )
            .map_err(io_operation)?;
        }
    }
    writeln!(output, "hook_args_count: {}", plan.hook_args().len()).map_err(io_operation)?;
    for step in plan.steps() {
        writeln!(output, "step: {}", plan_step_name(*step)).map_err(io_operation)?;
    }
    writeln!(output, "output_count: {}", plan.outputs().len()).map_err(io_operation)?;
    for output_id in plan.outputs() {
        writeln!(output, "output: {}", output_id.as_str()).map_err(io_operation)?;
    }
    writeln!(
        output,
        "startup_timeout_ms: {}",
        startup_timeout_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unlimited".to_owned())
    )
    .map_err(io_operation)?;
    writeln!(
        output,
        "action_timeout_ms: {}",
        action_timeout_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unlimited".to_owned())
    )
    .map_err(io_operation)?;
    writeln!(
        output,
        "termination_grace_ms: {}",
        termination_grace_ms.unwrap_or(5_000)
    )
    .map_err(io_operation)?;
    writeln!(
        output,
        "recovery_override: {}",
        if recovery_override {
            "authorized_for_this_invocation"
        } else {
            "not_authorized"
        }
    )
    .map_err(io_operation)?;
    writeln!(output, "preview: not_admitted").map_err(io_operation)?;
    writeln!(output, "warning: Plan is not Admission and created no Run").map_err(io_operation)
}

fn write_run_summary(
    output: &mut dyn Write,
    summary: &crate::domain::RunSummary,
) -> Result<(), CliError> {
    writeln!(
        output,
        "run: {}\taction: {}\tphase: {}\toutcome: {}",
        summary.id,
        summary.action.action.as_str(),
        format_phase(summary.phase),
        summary.outcome.map(format_outcome).unwrap_or("running")
    )
    .map_err(io_operation)
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
                    "primary_failure: {}:{}\tstep: {}",
                    failure.failure.error.owner(),
                    failure.failure.error.code(),
                    format_failed_step(failure.step)
                )
                .map_err(io_operation)?;
            }
            for failure in &outcome.secondary_failures {
                writeln!(
                    output,
                    "secondary_failure: {}:{}",
                    failure.error.owner(),
                    failure.error.code()
                )
                .map_err(io_operation)?;
            }
            if let Some(completion) = &outcome.hook_completion {
                writeln!(
                    output,
                    "hook_completion_status: {}",
                    format_hook_status(completion.status)
                )
                .map_err(io_operation)?;
                writeln!(output, "hook_completion_text: withheld").map_err(io_operation)?;
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

fn parse_run_id(value: &str) -> Result<RunId, CliError> {
    RunId::from_str(value).map_err(CliError::usage)
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

fn write_instance(
    output: &mut dyn Write,
    view: &crate::domain::InstanceView,
) -> Result<(), CliError> {
    writeln!(output, "name: {}", view.name.as_str()).map_err(io_operation)?;
    writeln!(
        output,
        "revision: {}",
        format_revision(&view.active_revision)
    )
    .map_err(io_operation)?;
    writeln!(output, "state_version: {}", view.state_version).map_err(io_operation)?;
    writeln!(
        output,
        "required_inputs_satisfied: {}",
        view.required_inputs_satisfied
    )
    .map_err(io_operation)?;
    write_inputs(output, view)
}

fn write_inputs(
    output: &mut dyn Write,
    view: &crate::domain::InstanceView,
) -> Result<(), CliError> {
    for binding in &view.bindings {
        let role = match binding.role {
            ManagedInputRole::Active { required: true } => "active_required",
            ManagedInputRole::Active { required: false } => "active_optional",
            ManagedInputRole::Retained => "retained",
        };
        let protection = match binding.protection {
            ManagedInputProtection::Normal => "normal",
            ManagedInputProtection::Secret => "secret",
        };
        writeln!(
            output,
            "input: {}\t{}\t{}\t{}",
            binding.input_id.as_str(),
            role,
            if binding.present { "present" } else { "absent" },
            protection
        )
        .map_err(io_operation)?;
    }
    Ok(())
}

fn parse_revision_reference(value: String) -> Result<RevisionReference, CliError> {
    if let Some(label) = value.strip_prefix("label:") {
        return ReferenceLabel::parse(label.to_owned())
            .map(RevisionReference::Label)
            .map_err(|error| CliError::usage(error.to_string()));
    }
    if let Some(alias) = value.strip_prefix("alias:") {
        return LocalAlias::parse(alias.to_owned())
            .map(RevisionReference::Alias)
            .map_err(|error| CliError::usage(error.to_string()));
    }
    if let Some(exact) = value.strip_prefix("exact:") {
        let (package, digest) = exact.split_once('/').ok_or_else(|| {
            CliError::usage("exact Revision requires <PackageId>/sha256:<digest>")
        })?;
        let package_id = PackageId::from_str(package).map_err(CliError::usage)?;
        let content_digest = RevisionContentDigest::from_str(digest).map_err(CliError::usage)?;
        return Ok(RevisionReference::Exact(RevisionIdentity::new(
            package_id,
            content_digest,
        )));
    }
    Err(CliError::usage(
        "Revision reference must use label:, alias:, or exact:",
    ))
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
    CliError::operation(error.to_string())
}

fn io_operation(error: io::Error) -> CliError {
    CliError::operation(error.to_string())
}

#[cfg(test)]
use crate::output_publication::OutputPublicationPoint;

fn publish_output_file(source: &StagedFile, destination: &Path) -> Result<(), CliError> {
    crate::output_publication::publish(source, destination).map_err(io_operation)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m2-cli-tests");
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
            r#"source_format: 1
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
portable_metadata:
  reference_labels:
    - label: stable
      source: { kind: unattributed }
"#,
        )
        .unwrap();
        (temporary, storage, source)
    }

    fn cli_action_roots() -> (TempDir, PathBuf, PathBuf) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m3-cli-tests");
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
                r#"source_format: 1
package_id: 00000000000000000000000000000036
revision:
  inputs: []
  actions:
    - id: inspect
      access: observe
      parameters:
        - {{ id: value, type: string, sensitive: false }}
      hook:
        protocol_version: 1
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
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m3-cli-tests");
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
                r#"source_format: 1
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
        protocol_version: 1
        launch: {{ kind: direct, executable: worker }}
        args: ["--exact", "hook::tests::m3_hook_worker", "--nocapture", "--test-threads=1", "slice4-argument-tail"]
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
        let (temporary, storage, source) = cli_hook_roots();
        let storage_env = Some(storage.as_os_str().to_owned());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(
                vec!["pack".into(), "install".into(), source.into_os_string()],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let revision = String::from_utf8(stdout.clone()).unwrap().trim().to_owned();
        let expected_path = temporary.path().join("expected-binding.txt");
        let sensitive_path = temporary.path().join("sensitive-value.txt");
        fs::write(&expected_path, b"slice4-secret-binding").unwrap();
        fs::write(&sensitive_path, b"slice4-sensitive-parameter").unwrap();
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
    fn m3_commands_and_run_projection_are_structural_and_redacted() {
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
                        failure: crate::domain::RunFailureRecord {
                            error: crate::domain::PactrunErrorRefV1::new(
                                "execution",
                                "launch_failed",
                            )
                            .unwrap(),
                            message: "persisted-secret-message".to_owned(),
                        },
                        step: crate::domain::RunFailedStep::Plan(ActionPlanStep::LaunchHook),
                    }),
                    secondary_failures: vec![crate::domain::RunFailureRecord {
                        error: crate::domain::PactrunErrorRefV1::new(
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
    fn m3_parameter_sources_preserve_exact_text_and_protected_channel() {
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
                vec!["run".into(), "list".into(), "node".into()],
                storage_env,
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0
        );
        assert!(stdout.is_empty(), "invalid timeout must create no Run");
    }

    // Test-ID: PR-TEST-0118
    // Verifies: PR-REQ-0284, PR-REQ-0287
    #[test]
    fn m3_recovery_commands_have_explicit_operator_spellings() {
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
    fn m3_human_action_and_plan_commands_use_the_read_only_path() {
        let (_temporary, storage, source) = cli_action_roots();
        let storage_env = Some(storage.as_os_str().to_owned());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(
                vec!["pack".into(), "install".into(), source.into_os_string()],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let revision = String::from_utf8(stdout.clone()).unwrap().trim().to_owned();
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
        assert!(String::from_utf8_lossy(&stdout).contains("action: inspect"));
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
        assert!(plan.contains("preview: not_admitted"));
        assert!(plan.contains("step: launch_hook"));
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
    fn m3_human_invoke_creates_and_finishes_a_managed_run() {
        let (temporary, storage, source) = cli_hook_roots();
        let storage_env = Some(storage.as_os_str().to_owned());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(
                vec!["pack".into(), "install".into(), source.into_os_string()],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let revision = String::from_utf8(stdout.clone()).unwrap().trim().to_owned();
        stdout.clear();
        stderr.clear();
        let expected_path = temporary.path().join("expected-binding.txt");
        fs::write(&expected_path, b"slice4-secret-binding").unwrap();
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
        fs::write(&sensitive_path, b"slice4-sensitive-parameter").unwrap();
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
            .find_map(|line| line.strip_prefix("run: "))
            .expect("invoke prints the completed RunId")
            .to_owned();
        assert!(!summary.contains("slice4-sensitive-parameter"));
        assert!(!summary.contains("slice4-secret-binding"));
        stdout.clear();
        stderr.clear();
        assert_eq!(
            run(
                vec!["run".into(), "list".into(), "node".into()],
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
        assert!(String::from_utf8_lossy(&stdout).contains("outcome: succeeded"));
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
                vec!["pack".into(), "install".into(), source.into_os_string()],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let revision = String::from_utf8(stdout.clone()).unwrap().trim().to_owned();
        let expected_path = storage.parent().unwrap().join("expected-binding.txt");
        let sensitive_path = storage.parent().unwrap().join("sensitive-value.txt");
        fs::write(&expected_path, b"slice4-secret-binding").unwrap();
        fs::write(&sensitive_path, b"slice4-sensitive-parameter").unwrap();
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
            0
        );
        assert!(marker.with_extension("session").exists());

        stdout.clear();
        stderr.clear();
        assert_eq!(
            run(
                vec!["run".into(), "list".into(), "node".into()],
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
        assert!(listing.contains("phase: finished"));
        assert!(listing.contains("outcome: succeeded"));
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
            0
        );
        assert!(marker.with_extension("session").exists());

        let mut listing = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(
                vec!["run".into(), "list".into(), "node".into()],
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
        assert_eq!(
            listing
                .lines()
                .filter(|line| line.starts_with("run: "))
                .count(),
            1
        );
        assert!(listing.contains("phase: finished"));
        assert!(listing.contains("outcome: succeeded"));
    }

    // Test-ID: PR-TEST-0127
    // Verifies: PR-REQ-0052, PR-REQ-0286
    #[test]
    fn repeated_finalization_failures_keep_owner_until_terminal_commit() {
        let (_temporary, storage, expected_path, sensitive_path) = prepared_cli_hook_instance();
        let storage_env = Some(storage.as_os_str().to_owned());
        crate::application::fail_next_finalization_advances_for_test(3);
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
            0
        );
        assert!(marker.with_extension("session").exists());

        let mut listing = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(
                vec!["run".into(), "list".into(), "node".into()],
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
        assert_eq!(
            listing
                .lines()
                .filter(|line| line.starts_with("run: "))
                .count(),
            1
        );
        assert!(listing.contains("phase: finished"));
        assert!(listing.contains("outcome: succeeded"));
    }

    // Test-ID: PR-TEST-0078
    // Verifies: PR-REQ-0092, PR-REQ-0263, PR-REQ-0266, PR-REQ-0268, PR-REQ-0271
    #[test]
    fn cli_install_instance_secret_export_and_atomic_no_clobber_are_end_to_end() {
        let (_temporary, storage, source) = cli_roots();
        let storage_env = Some(storage.as_os_str().to_owned());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(
                vec!["pack".into(), "install".into(), source.into_os_string()],
                storage_env.clone(),
                &mut io::empty(),
                &mut stdout,
                &mut stderr,
            ),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let revision = String::from_utf8(stdout.clone()).unwrap();
        let revision = revision.trim().to_owned();
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
