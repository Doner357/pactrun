//! Closed M2 human CLI parsing, composition, and terminal presentation.

use std::{
    collections::BTreeSet,
    env,
    ffi::{OsStr, OsString},
    fmt,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use lexopt::{Arg, Parser};

use crate::{
    application::{ApplicationError, InputAcquisition, MigrationRelationState, PactrunApplication},
    domain::{
        InputIdentity, InstanceName, InstanceStateVersion, LocalAlias, ManagedInputProtection,
        ManagedInputRole, PackageId, ReferenceLabel, RevisionContentDigest, RevisionIdentity,
        RevisionMetadataMutationBatch,
    },
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
  pactrun input list <instance>\n\
  pactrun input set <instance> <input-id> (--file <path> | --stdin) [--if-version <token>]\n\
  pactrun input export <instance> <input-id> --output <path|-> [--authorize-secret-export]\n\
  pactrun input delete <instance> <input-id> [--if-version <token>]\n\
\n\
Revision references: label:<label>, alias:<alias>, or exact:<package-id>/sha256:<digest>.\n";

enum Command {
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
}

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

#[derive(Debug)]
struct CliError {
    message: String,
    usage: bool,
}

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
    let storage_root = env::var_os(STORAGE_ROOT_ENV);
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    run(args, storage_root, &mut stdin, &mut stdout, &mut stderr)
}

fn run(
    args: Vec<OsString>,
    storage_root: Option<OsString>,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let command = match parse_command(args) {
        Ok(command) => command,
        Err(error) => {
            let _ = writeln!(stderr, "error: {error}");
            return 2;
        }
    };
    match execute(command, storage_root, stdin, stdout, stderr) {
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

fn execute(
    command: Command,
    storage_root: Option<OsString>,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
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
    let application = PactrunApplication::open(PathBuf::from(storage_root)).map_err(app_error)?;
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
        Command::Help | Command::Version | Command::GeneratePackageId => unreachable!(),
    }
    Ok(())
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

#[cfg(windows)]
fn split_initial_input_file(value: OsString) -> Result<(InputIdentity, PathBuf), CliError> {
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
    Ok((parse_input_id(&input)?, PathBuf::from(path)))
}

#[cfg(unix)]
fn split_initial_input_file(value: OsString) -> Result<(InputIdentity, PathBuf), CliError> {
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
    Ok((parse_input_id(input)?, PathBuf::from(path)))
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputPublicationPoint {
    BeforeFinalNamePublication,
    AfterFinalNamePublication,
}

fn publish_output_file(source: &StagedFile, destination: &Path) -> Result<(), CliError> {
    publish_output_file_with_fault(source, destination, |_| Ok(()))
}

fn publish_output_file_with_fault(
    source: &StagedFile,
    destination: &Path,
    mut fault: impl FnMut(OutputPublicationPoint) -> Result<(), CliError>,
) -> Result<(), CliError> {
    if destination.file_name().is_none() {
        return Err(CliError::operation("output path has no final file name"));
    }
    match fs::symlink_metadata(destination) {
        Ok(_) => return Err(CliError::operation("output destination already exists")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(CliError::operation(error.to_string())),
    }
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let (temporary_path, mut temporary) = create_output_temporary(parent)?;
    let result = (|| {
        let mut reader = source
            .try_clone_reader()
            .map_err(|error| CliError::operation(error.to_string()))?;
        io::copy(&mut reader, &mut temporary).map_err(io_operation)?;
        temporary.sync_all().map_err(io_operation)?;
        fault(OutputPublicationPoint::BeforeFinalNamePublication)?;
        publish_no_replace(&temporary, &temporary_path, destination)?;
        fault(OutputPublicationPoint::AfterFinalNamePublication)?;
        sync_output_parent(parent)?;
        Ok(())
    })();
    drop(temporary);
    if temporary_path.exists() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn create_output_temporary(parent: &Path) -> Result<(PathBuf, File), CliError> {
    for _ in 0..32 {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random)
            .map_err(|error| CliError::operation(format!("CSPRNG failed: {error}")))?;
        let path = parent.join(format!(".pactrun-export-{}.tmp", hex::encode(random)));
        #[cfg(windows)]
        let result = pactrun_windows_ntfs::create_staging(&path);
        #[cfg(unix)]
        let result = {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
        };
        match result {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(CliError::operation(error.to_string())),
        }
    }
    Err(CliError::operation(
        "could not allocate output temporary file",
    ))
}

#[cfg(windows)]
fn publish_no_replace(
    staging: &File,
    _staging_path: &Path,
    final_path: &Path,
) -> Result<(), CliError> {
    pactrun_windows_ntfs::rename_no_replace(staging, final_path)
        .map_err(|error| CliError::operation(error.to_string()))
}

#[cfg(target_os = "linux")]
fn publish_no_replace(
    _staging: &File,
    staging_path: &Path,
    final_path: &Path,
) -> Result<(), CliError> {
    fs::hard_link(staging_path, final_path)
        .map_err(|error| CliError::operation(error.to_string()))?;
    fs::remove_file(staging_path).map_err(|error| CliError::operation(error.to_string()))
}

#[cfg(windows)]
fn sync_output_parent(_parent: &Path) -> Result<(), CliError> {
    // The write-through NTFS handle and no-replace rename provide the supported
    // Windows publication profile used by the existing M1-B store.
    Ok(())
}

#[cfg(target_os = "linux")]
fn sync_output_parent(parent: &Path) -> Result<(), CliError> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| CliError::operation(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
