use super::*;
use crate::domain::ManagedOutputIdentity;

pub(super) enum ArtifactCommand {
    Export {
        run: RunId,
        output: ManagedOutputIdentity,
        destination: PathBuf,
        authorized: bool,
    },
    Delete {
        run: RunId,
        output: ManagedOutputIdentity,
    },
}

pub(super) fn parse(parser: &mut Parser) -> Result<ArtifactCommand, CliError> {
    let operation = required_value_string(parser, "Artifact command")?;
    if operation != "export" && operation != "delete" {
        return Err(CliError::usage("unknown Artifact command"));
    }
    let run = parse_run_id(&value_string(parser, "RunId")?)?;
    let output = ManagedOutputIdentity::parse(required_value_string(parser, "output identity")?)
        .map_err(|_| CliError::usage("invalid output identity"))?;
    if operation == "delete" {
        require_end(parser)?;
        return Ok(ArtifactCommand::Delete { run, output });
    }
    let mut destination = None;
    let mut authorized = false;
    while let Some(argument) = parser.next().map_err(lex_error)? {
        match argument {
            Arg::Long("output") => {
                let path = required_parser_value(parser, "output path")?;
                if path == "-" {
                    return Err(CliError::usage(
                        "Artifact export requires a file destination",
                    ));
                }
                set_once(&mut destination, PathBuf::from(path), "--output")?;
            }
            Arg::Long("authorize-sensitive-export") if !authorized => authorized = true,
            _ => return Err(CliError::usage("unsupported or duplicate Artifact option")),
        }
    }
    Ok(ArtifactCommand::Export {
        run,
        output,
        destination: destination.ok_or_else(|| CliError::usage("missing --output"))?,
        authorized,
    })
}

#[cfg(test)]
#[path = "artifact_tests.rs"]
mod tests;

pub(super) fn execute(
    command: ArtifactCommand,
    root: &Path,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    if matches!(
        command,
        ArtifactCommand::Export {
            authorized: false,
            ..
        }
    ) {
        return Err(CliError::operation(
            "Artifact export requires --authorize-sensitive-export",
        ));
    }
    let application = PactrunApplication::open(root).map_err(app_error)?;
    match command {
        ArtifactCommand::Export {
            run,
            output,
            destination,
            authorized,
        } => {
            let staged = application
                .export_run_artifact(run, &output, authorized)
                .map_err(|error| match error {
                    ApplicationError::Persistence(
                        crate::persistence::PersistenceError::MissingRun(_),
                    ) => CliError::operation("parent Run is not persisted"),
                    _ => CliError::operation(
                        "Artifact export failed; check the Run, published output and storage",
                    ),
                })?;
            crate::output_publication::publish_reported(&staged, &destination)
                .map_err(|error| {
                    let message = if error.destination_published {
                        "Artifact destination was published but durability could not be confirmed; the file may already exist and was not removed"
                    } else if error.source.kind() == io::ErrorKind::AlreadyExists {
                        "Artifact export destination already exists; it was not overwritten"
                    } else {
                        "Artifact export destination could not be published"
                    };
                    CliError::operation(message)
                })?;
            writeln!(stdout, "Artifact exported").map_err(io_operation)
        }
        ArtifactCommand::Delete { run, output } => {
            let deleted =
                application
                    .delete_run_artifact(run, &output)
                    .map_err(|error| match error {
                        ApplicationError::Persistence(
                            crate::persistence::PersistenceError::MissingRun(_),
                        ) => CliError::operation("parent Run is not persisted"),
                        _ => CliError::operation(
                            "Artifact deletion failed; check that storage is writable",
                        ),
                    })?;
            writeln!(
                stdout,
                "Artifact {}",
                if deleted { "deleted" } else { "already absent" }
            )
            .map_err(io_operation)
        }
    }
}
