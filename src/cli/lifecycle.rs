use super::*;
use crate::domain::{ObjectDeletion, ObjectDeletionResult};

pub(super) enum LifecycleCommand {
    Collect { plan: bool },
    Revision(RevisionReference),
    Run { run: RunId, delete_artifacts: bool },
}

pub(super) fn parse_revision(parser: &mut Parser) -> Result<LifecycleCommand, CliError> {
    if required_value_string(parser, "Revision command")? != "delete" {
        return Err(CliError::usage("unknown Revision command"));
    }
    let reference = parse_revision_reference(required_value_string(parser, "Revision reference")?)?;
    require_end(parser)?;
    Ok(LifecycleCommand::Revision(reference))
}

pub(super) fn parse_run_delete(parser: &mut Parser) -> Result<LifecycleCommand, CliError> {
    let run = parse_run_id(&value_string(parser, "RunId")?)?;
    let mut delete_artifacts = false;
    while let Some(arg) = parser.next().map_err(lex_error)? {
        match arg {
            Arg::Long("delete-artifacts") if !delete_artifacts => delete_artifacts = true,
            _ => {
                return Err(CliError::usage(
                    "unsupported or duplicate Run deletion option",
                ));
            }
        }
    }
    Ok(LifecycleCommand::Run {
        run,
        delete_artifacts,
    })
}

pub(super) fn write_deletion(
    stdout: &mut dyn Write,
    result: ObjectDeletionResult,
) -> Result<(), CliError> {
    match result {
        ObjectDeletionResult::Deleted => writeln!(stdout, "object deleted").map_err(io_operation),
        ObjectDeletionResult::AlreadyAbsent => {
            writeln!(stdout, "object already absent").map_err(io_operation)
        }
        ObjectDeletionResult::Blocked(reason) => Err(CliError::operation(reason.message())),
    }
}

pub(super) fn execute(
    command: LifecycleCommand,
    root: &Path,
    stdout: &mut dyn Write,
) -> Result<(), CliError> {
    if let LifecycleCommand::Collect { plan } = command {
        let app = if plan {
            PactrunApplication::open_read_only(root)
        } else {
            PactrunApplication::open_for_collection(root)
        }
        .map_err(app_error)?;
        let report = app.collect_content(!plan).map_err(app_error)?;
        writeln!(
            stdout,
            "{}: candidates={}, removed={}, retained={}, unsupported={}, failed={}",
            if plan {
                "collection preview (not reserved)"
            } else {
                "collection"
            },
            report.candidates,
            report.removed,
            report.retained,
            report.unsupported,
            report.failed
        )
        .map_err(io_operation)?;
        if report.failed != 0 {
            return Err(CliError::operation(
                "collection incomplete; some removals could not be confirmed and may already be absent; retry after resolving the storage problem",
            ));
        }
        return Ok(());
    }
    let app = PactrunApplication::open(root).map_err(app_error)?;
    let target = match command {
        LifecycleCommand::Collect { .. } => unreachable!(),
        LifecycleCommand::Run {
            run,
            delete_artifacts,
        } => ObjectDeletion::Run {
            run,
            delete_artifacts,
        },
        LifecycleCommand::Revision(reference) => ObjectDeletion::Revision(match reference {
            RevisionReference::Exact(identity) => identity,
            other => resolve_revision(&app, other)?,
        }),
    };
    write_deletion(stdout, app.delete_object(&target).map_err(app_error)?)
}
