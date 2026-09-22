use super::*;
use crate::domain::{ObjectDeletion, ObjectDeletionResult};

pub(super) enum LifecycleCommand {
    Collect { plan: bool },
    Revision(RevisionReference),
    Run { run: RunId, delete_artifacts: bool },
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

pub(super) fn execute(
    command: LifecycleCommand,
    root: &Path,
    stdout: &mut dyn Write,
    format: presentation::Format,
) -> Result<(), CliError> {
    if let LifecycleCommand::Collect { plan } = command {
        let app = if plan {
            PactrunApplication::open_read_only(root)
        } else {
            PactrunApplication::open_for_collection(root)
        }
        .map_err(app_error)?;
        let report = app.collect_content(!plan).map_err(app_error)?;
        let result = presentation::Collection::new(plan, &report);
        if report.failed != 0 {
            if format == presentation::Format::Human {
                result.human(stdout)?;
            }
            let mut error = CliError::operation(
                "collection incomplete; some removals could not be confirmed and may already be absent; retry after resolving the storage problem",
            );
            error.partial = Some(presentation::PartialResult::Collection(Box::new(result)));
            return Err(error);
        }
        return presentation::render(
            format,
            "storage gc",
            &result,
            stdout,
            presentation::Collection::human,
        );
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
    let name = match target {
        ObjectDeletion::Run { .. } => "run delete",
        ObjectDeletion::Revision(_) => "revision delete",
        _ => unreachable!(),
    };
    render_deletion(
        format,
        name,
        stdout,
        app.delete_object(&target).map_err(app_error)?,
    )
}

pub(super) fn render_deletion(
    format: presentation::Format,
    command: &str,
    out: &mut dyn Write,
    result: ObjectDeletionResult,
) -> Result<(), CliError> {
    let outcome = match result {
        ObjectDeletionResult::Deleted => "deleted",
        ObjectDeletionResult::AlreadyAbsent => "already_absent",
        ObjectDeletionResult::Blocked(reason) => return Err(CliError::operation(reason.message())),
    };
    presentation::render(
        format,
        command,
        &presentation::Deletion { outcome },
        out,
        |value, out| {
            writeln!(
                out,
                "object {}",
                if value.outcome == "deleted" {
                    "deleted"
                } else {
                    "already absent"
                }
            )
            .map_err(io_operation)
        },
    )
}
