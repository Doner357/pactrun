//! Read-only diagnostic inspection, usable without valid runtime configuration.
use super::*;
use crate::{
    domain::{TraceId, TraceOperation, TracePhase, TracePolicy},
    trace_journal::{self, Location},
};

#[cfg(test)]
#[path = "core_trace_tests.rs"]
mod tests;

pub(super) enum Query {
    List(usize),
    Show(TraceId),
    Run(crate::domain::RunId),
}
pub(super) struct Request {
    query: Query,
    startup: bool,
}
impl Request {
    pub(super) fn command_name(&self) -> &'static str {
        match self.query {
            Query::List(_) => "diagnostic list",
            Query::Show(_) | Query::Run(_) => "diagnostic show",
        }
    }
}

pub(super) fn parse(parser: &mut Parser) -> Result<Request, CliError> {
    let action = required_value_string(parser, "diagnostic command")?;
    if !matches!(action.as_str(), "show" | "list") {
        return Err(CliError::usage("unknown diagnostic command"));
    }
    let mut id: Option<TraceId> = None;
    let mut run: Option<crate::domain::RunId> = None;
    let mut startup = false;
    let mut limit = None;
    while let Some(arg) = parser.next().map_err(lex_error)? {
        match arg {
            Arg::Value(value) if action == "show" => set_once(
                &mut id,
                os_string(value, "diagnostic identity")?
                    .parse()
                    .map_err(|_| {
                        CliError::usage(
                            "diagnostic identity requires 32 lowercase hexadecimal digits",
                        )
                    })?,
                "diagnostic identity",
            )?,
            Arg::Long("run") if action == "show" => set_once(
                &mut run,
                required_value_string(parser, "Run identity")?
                    .parse()
                    .map_err(|_| CliError::usage("--run requires a complete Run identity"))?,
                "--run",
            )?,
            Arg::Long("startup") if !startup => startup = true,
            Arg::Long("limit") if action == "list" => {
                let value = required_value_string(parser, "diagnostic limit")?
                    .parse::<usize>()
                    .map_err(|_| CliError::usage("diagnostic limit requires 1..100"))?;
                if !(1..=100).contains(&value) {
                    return Err(CliError::usage("diagnostic limit requires 1..100"));
                }
                set_once(&mut limit, value, "--limit")?;
            }
            _ => {
                return Err(CliError::usage(
                    "unsupported or duplicate diagnostic option",
                ));
            }
        }
    }
    let query = match (action.as_str(), id, run) {
        ("list", None, None) => Query::List(limit.unwrap_or(20)),
        ("show", Some(id), None) => Query::Show(id),
        ("show", None, Some(run)) if !startup => Query::Run(run),
        _ => {
            return Err(CliError::usage(
                "diagnostic show needs one diagnostic identity or --run; --run uses the Store journal",
            ));
        }
    };
    Ok(Request { query, startup })
}
pub(super) fn execute(
    request: Request,
    root: Option<OsString>,
    output: reply::OutputContext,
) -> Result<(), CliError> {
    let name = request.command_name();
    let location = if request.startup {
        startup_location()
            .ok_or_else(|| CliError::operation("startup diagnostic location is unavailable"))?
    } else {
        let root = root
            .filter(|v| !v.is_empty())
            .ok_or_else(|| CliError::operation("select a Store or use --startup"))?;
        Location::store(Path::new(&root))
    };
    match request.query {
        Query::List(limit) => presentation::emit_result(
            output,
            name,
            &trace_journal::list(&location, limit).map_err(journal_error)?,
        ),
        Query::Show(id) => {
            let document = trace_journal::read(&location, id).map_err(journal_error)?.ok_or_else(|| CliError::operation("diagnostic record is unavailable; it may not have been persisted or may no longer be retained"))?;
            presentation::emit_result(output, name, &document)
        }
        Query::Run(run) => {
            let document = trace_journal::read_run(&location, run)
                .map_err(journal_error)?
                .ok_or_else(|| {
                    CliError::operation(
                        "Run phase trace is unavailable; no historical evidence is fabricated",
                    )
                })?;
            presentation::emit_result(output, name, &document)
        }
    }
}
fn journal_error(e: trace_journal::Error) -> CliError {
    CliError::operation(e.to_string())
}
pub(super) fn startup_location() -> Option<Location> {
    crate::trace::startup_location(env::current_exe().ok().as_deref(), |key| env::var_os(key))
}
pub(super) fn configure(
    cancellation: &ActionCancellation,
    root: Option<&OsString>,
    settings: Option<&crate::configuration::Effective>,
) {
    let (startup, store) = settings
        .map(|s| {
            (
                TracePolicy {
                    detailed: s.startup_diagnostics.level.value
                        == crate::configuration::Level::Detailed,
                    retention_days: s.startup_diagnostics.retention_days.value,
                    max_bytes: s.startup_diagnostics.max_bytes.value,
                },
                TracePolicy {
                    detailed: s.diagnostics.level.value == crate::configuration::Level::Detailed,
                    retention_days: s.diagnostics.retention_days.value,
                    max_bytes: s.diagnostics.max_bytes.value,
                },
            )
        })
        .unwrap_or((TracePolicy::startup(), TracePolicy::default()));
    cancellation.diagnostics.configure_retention(store);
    cancellation.trace.configure(
        startup_location(),
        root.filter(|p| !p.is_empty()).map(Path::new),
        startup,
        store,
    );
}
pub(super) fn inspection_only(args: &[OsString]) -> bool {
    let mut rest = args;
    while let Some(key) = rest.first().and_then(|v| v.to_str()) {
        let width = if matches!(key, "--format" | "--machine-version") {
            Some(2)
        } else {
            configuration::global_width(key)
        };
        let Some(width) = width.filter(|n| rest.len() >= *n) else {
            break;
        };
        rest = &rest[width..];
    }
    rest.first()
        .is_some_and(|v| v == "config" || v == "diagnostic" || v == "--version")
        || help::request(rest).is_some()
}
pub(super) fn operation(command: &Command) -> TraceOperation {
    match command {
        Command::Install { .. } => TraceOperation::PackageInstall,
        Command::ExportRevision { .. } => TraceOperation::RevisionExport,
        Command::CreateInstance { .. } => TraceOperation::InstanceCreate,
        Command::CreateAndRestore(_) => TraceOperation::SnapshotRestore,
        Command::SetInput { .. } => TraceOperation::InputSet,
        Command::ExportInput { .. } => TraceOperation::InputExport,
        Command::DeleteInput { .. } => TraceOperation::InputDelete,
        Command::Invoke { .. } => TraceOperation::ActionInvoke,
        Command::Snapshot(snapshots::SnapshotCommand::Execute {
            operation: snapshots::Operation::Capture,
            ..
        }) => TraceOperation::SnapshotCapture,
        Command::Snapshot(snapshots::SnapshotCommand::Execute {
            operation: snapshots::Operation::Restore(_),
            ..
        }) => TraceOperation::SnapshotRestore,
        Command::Snapshot(_) => TraceOperation::SnapshotTransport,
        Command::Migration(_) => TraceOperation::Migration,
        Command::Retirement(_) => TraceOperation::Retirement,
        Command::ReconcileRuns | Command::ResolveManualRecovery { .. } => TraceOperation::Recovery,
        Command::Lifecycle(_) | Command::Artifact(artifacts::ArtifactCommand::Delete { .. }) => {
            TraceOperation::Collection
        }
        Command::Artifact(_) | Command::RunExport(_) => TraceOperation::RunExport,
        _ => TraceOperation::Catalog,
    }
}
pub(super) fn phase_result(code: i32) -> crate::domain::PhaseResult {
    if code == 0 {
        crate::domain::PhaseResult::Succeeded
    } else {
        crate::domain::PhaseResult::Failed
    }
}

pub(super) fn root_span(
    cancellation: &ActionCancellation,
    args: &[OsString],
) -> crate::trace::Span {
    if !inspection_only(args) {
        cancellation.trace.begin();
    }
    cancellation.trace.span(TracePhase::Command)
}

/// Observe delivery separately from the operation's exit code. An error reply
/// can be delivered successfully; an ignored writer error is still a failure.
pub(super) fn render_reply(
    reply: reply::Reply,
    format: presentation::Format,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    trace: &std::sync::Arc<crate::trace::Trace>,
) -> i32 {
    let span = trace.span(TracePhase::OutputDelivery);
    let mut out = DeliveryObservation {
        inner: stdout,
        failed: false,
    };
    let mut err = DeliveryObservation {
        inner: stderr,
        failed: false,
    };
    let code = reply.render(format, &mut out, &mut err);
    span.finish(if out.failed || err.failed {
        crate::domain::PhaseResult::Failed
    } else {
        crate::domain::PhaseResult::Succeeded
    });
    code
}

struct DeliveryObservation<'a> {
    inner: &'a mut dyn Write,
    failed: bool,
}
impl Write for DeliveryObservation<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = self.inner.write(bytes);
        self.failed |= matches!(&result, Err(e) if e.kind() != io::ErrorKind::Interrupted)
            || matches!(result, Ok(0) if !bytes.is_empty());
        result
    }
    fn flush(&mut self) -> io::Result<()> {
        let result = self.inner.flush();
        self.failed |= result.is_err();
        result
    }
}
