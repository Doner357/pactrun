//! One owned, typed command result. Rendering occurs after command execution.
use super::*;
use serde::Serialize;
use std::sync::Mutex;

#[derive(Clone, Copy)]
pub(super) struct OutputContext<'a>(pub(super) &'a Capture);
impl OutputContext<'_> {
    pub(super) fn requires_noninteractive(self) -> bool {
        !self.0.interactive
    }
    pub(super) fn publish<T: Serialize + Send + 'static>(
        self,
        command: &str,
        value: T,
    ) -> Result<(), CliError> {
        self.0.publish(command, value)
    }
}

pub(super) trait Prepared: Send {
    fn json(
        &self,
        envelope: presentation::Envelope<'_>,
        out: &mut dyn Write,
    ) -> Result<(), CliError>;
    fn human(
        &self,
        command: &str,
        display: &DisplayOptions,
        out: &mut dyn Write,
    ) -> Result<(), CliError>;
}
struct Value<T>(T);
impl<T: Serialize + Send> Prepared for Value<T> {
    fn json(
        &self,
        envelope: presentation::Envelope<'_>,
        out: &mut dyn Write,
    ) -> Result<(), CliError> {
        presentation::write_reply(envelope, Some(&self.0), out)
    }
    fn human(
        &self,
        command: &str,
        display: &DisplayOptions,
        out: &mut dyn Write,
    ) -> Result<(), CliError> {
        // This is a view tree, not encoded JSON or an input to command execution.
        let view = serde_json::to_value(&self.0)
            .map_err(|e| CliError::operation(format!("prepare human view: {e}")))?;
        human::render(command, &view, display, out)
    }
}
pub(super) struct Capture {
    expected: &'static str,
    pub(super) interactive: bool,
    value: Mutex<Option<Box<dyn Prepared>>>,
    display: DisplayOptions,
}
impl std::fmt::Debug for Capture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CommandResultCapture")
    }
}
impl Capture {
    pub(super) fn new(expected: &'static str, interactive: bool, display: DisplayOptions) -> Self {
        Self {
            expected,
            interactive,
            value: Mutex::new(None),
            display,
        }
    }
    pub(super) fn publish<T: Serialize + Send + 'static>(
        &self,
        command: &str,
        value: T,
    ) -> Result<(), CliError> {
        if command != self.expected {
            return Err(CliError::operation(
                "command result identity differs from the requested command",
            ));
        }
        let mut slot = self
            .value
            .lock()
            .map_err(|_| CliError::operation("command result unavailable"))?;
        if slot.is_some() {
            return Err(CliError::operation(
                "command produced more than one final result",
            ));
        }
        *slot = Some(Box::new(Value(value)));
        Ok(())
    }
    pub(super) fn finish(self, error: Option<CliError>, raw: bool) -> Reply {
        let value = self.value.into_inner().unwrap_or_else(|e| e.into_inner());
        let error = if value.is_none() && error.is_none() && !raw {
            Some(CliError::operation(
                "command completed without a final result; do not infer rollback",
            ))
        } else {
            error
        };
        Reply {
            command: self.expected,
            value,
            error,
            raw,
            display: self.display,
        }
    }
}

pub(super) struct Reply {
    pub(super) command: &'static str,
    value: Option<Box<dyn Prepared>>,
    pub(super) error: Option<CliError>,
    raw: bool,
    display: DisplayOptions,
}
impl Reply {
    pub(super) fn unavailable(command: &'static str, run: Option<String>) -> Self {
        let mut error = CliError::operation("Execution result unavailable; inspect retained Runs");
        error.partial =
            run.map(|run_id| presentation::PartialResult::Run(presentation::KnownRun { run_id }));
        Self {
            command,
            value: None,
            error: Some(error),
            raw: false,
            display: DisplayOptions::default(),
        }
    }
    pub(super) fn exit_code(&self) -> i32 {
        self.error
            .as_ref()
            .map_or(0, |e| if e.usage { 2 } else { 1 })
    }
    pub(super) fn json(
        &self,
        out: &mut dyn Write,
        delivery: Option<&streaming::DeliveryResult>,
        sequence: Option<&str>,
    ) -> Result<(), CliError> {
        let envelope = presentation::Envelope {
            command: self.command,
            error: self.error.as_ref(),
            delivery,
            sequence,
        };
        if let Some(partial) = self.error.as_ref().and_then(|e| e.partial.as_ref()) {
            presentation::write_reply(envelope, Some(partial), out)
        } else if let Some(value) = &self.value {
            value.json(envelope, out)
        } else {
            presentation::write_reply::<()>(envelope, None, out)
        }
    }
    pub(super) fn render(
        mut self,
        format: presentation::Format,
        stdout: &mut dyn Write,
        stderr: &mut dyn Write,
    ) -> i32 {
        let published = self.value.is_some()
            || matches!(self.error.as_ref().and_then(|e|e.partial.as_ref()),Some(presentation::PartialResult::Publication(p)) if p.destination_published);
        if published {
            let warning = match self.command {
                "revision export" => {
                    Some("Exported Pack is unencrypted and may contain sensitive authored content.")
                }
                "snapshot export" => {
                    Some("Snapshot bundle is unencrypted and may contain sensitive data.")
                }
                _ => None,
            };
            if let Some(warning) = warning
                && let Err(error) = writeln!(stderr, "Warning: {warning}")
                && self.error.is_none()
            {
                self.error = Some(presentation::output_error(error));
            }
        }
        let code = self.exit_code();
        if self.raw && self.error.is_none() {
            return code;
        }
        match format {
            presentation::Format::Human => {
                if let Some(error) = self.error {
                    if matches!(error.kind, presentation::ErrorKind::Output)
                        && let Some(value) = &self.value
                    {
                        let _ = value.human(self.command, &self.display, stdout);
                    }
                    if let Some(partial) = &error.partial
                        && let Ok(view) = serde_json::to_value(partial)
                    {
                        let _ = human::partial(&view, stderr);
                    }
                    return report_cli_failure(
                        format,
                        Some(self.command),
                        self.raw,
                        error,
                        stdout,
                        stderr,
                    );
                }
                if let Some(value) = self.value {
                    let out: &mut dyn Write = if !self.display.preview
                        && matches!(
                            self.command,
                            "invoke"
                                | "snapshot restore"
                                | "instance migrate"
                                | "instance delete"
                                | "instance abandon"
                                | "input export"
                        ) {
                        stderr
                    } else {
                        stdout
                    };
                    if value.human(self.command, &self.display, out).is_err() {
                        let message = if matches!(
                            self.command,
                            "revision export" | "snapshot export"
                        ) {
                            "Output delivery failed; the destination was published. Check it before repeating the export."
                        } else {
                            "The command completed, but its result could not be delivered. Do not infer rollback."
                        };
                        let _ = writeln!(stderr, "{message}");
                        return 1;
                    }
                }
                code
            }
            presentation::Format::Json | presentation::Format::Jsonl => {
                let out: &mut dyn Write = if self.raw { stderr } else { stdout };
                if self
                    .json(
                        out,
                        None,
                        (format == presentation::Format::Jsonl).then_some("1"),
                    )
                    .is_err()
                {
                    1
                } else {
                    code
                }
            }
        }
    }
}

#[derive(Default)]
pub(super) struct DisplayOptions {
    pub(super) no_trunc: bool,
    preview: bool,
    continuation: Option<(String, usize, Vec<String>)>,
}
impl DisplayOptions {
    pub(super) fn for_command(command: &Command) -> Self {
        use catalog::{CatalogCommand as C, RunSelector};
        let mut options = match command {
            Command::Migration(migrations::MigrationCommand::List {
                name,
                target,
                limit,
                no_trunc,
                ..
            }) => Self {
                no_trunc: *no_trunc,
                continuation: Some((
                    "instance migration-paths".into(),
                    *limit,
                    vec![
                        "--to".into(),
                        quote(&revision_reference(target)),
                        "--".into(),
                        quote(name.as_str()),
                    ],
                )),
                ..Self::default()
            },
            Command::ServiceStorage(command) => Self {
                no_trunc: command.no_trunc(),
                ..Self::default()
            },
            Command::Catalog(C::Revisions(o)) => Self {
                no_trunc: o.no_trunc,
                continuation: Some(("revision list".into(), o.limit, vec![])),
                ..Self::default()
            },
            Command::Catalog(C::History(o, deletion)) => Self {
                no_trunc: o.no_trunc,
                continuation: Some((
                    if *deletion {
                        "instance deletion list"
                    } else {
                        "instance history list"
                    }
                    .into(),
                    o.limit,
                    vec![],
                )),
                ..Self::default()
            },
            Command::Catalog(C::Runs(selector, o)) => {
                let tail = match selector {
                    RunSelector::All => vec![],
                    RunSelector::Name(n) => vec!["--".into(), quote(n.as_str())],
                    RunSelector::Identity(id) => vec!["--instance-id".into(), id.to_string()],
                };
                Self {
                    no_trunc: o.no_trunc,
                    continuation: Some(("run list".into(), o.limit, tail)),
                    ..Self::default()
                }
            }
            Command::Snapshot(snapshots::SnapshotCommand::List { no_trunc, .. })
            | Command::Retirement(retirements::RetirementCommand::DetachedList { no_trunc }) => {
                Self {
                    no_trunc: *no_trunc,
                    ..Self::default()
                }
            }
            _ => Self::default(),
        };
        options.preview = matches!(
            command,
            Command::Invoke { plan: true, .. }
                | Command::Migration(migrations::MigrationCommand::Plan { plan: true, .. })
                | Command::Snapshot(snapshots::SnapshotCommand::Execute {
                    options: ExecutionOptions { plan: true, .. },
                    ..
                })
                | Command::Retirement(retirements::RetirementCommand::Execute {
                    options: ExecutionOptions { plan: true, .. },
                    ..
                })
        );
        options
    }
    pub(super) fn continuation(&self, cursor: &str) -> Option<String> {
        self.continuation.as_ref().map(|(command, limit, tail)| {
            format!(
                "pactrun {command} --limit {limit} --after {}{}{}",
                quote(cursor),
                if self.no_trunc { " --no-trunc" } else { "" },
                if tail.is_empty() {
                    String::new()
                } else {
                    format!(" {}", tail.join(" "))
                }
            )
        })
    }
}
fn revision_reference(reference: &RevisionReference) -> String {
    match reference {
        RevisionReference::Named(reference) => {
            let (package, revision) = reference.components();
            format!("{}:{}", package.as_str(), revision.as_str())
        }
        RevisionReference::Exact(id) => format!(
            "{}:{}",
            id.package_id,
            hex::encode(id.content_digest.as_bytes())
        ),
        RevisionReference::Prefix { package, digest } => format!("{package}:{digest}"),
    }
}
fn quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
    {
        return text.into();
    }
    #[cfg(windows)]
    {
        format!("'{}'", text.replace('\'', "''"))
    }
    #[cfg(not(windows))]
    {
        format!("'{}'", text.replace('\'', "'\"'\"'"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0675
    // Verifies: PR-REQ-0375
    #[test]
    fn delivery_failure_does_not_change_the_collected_operation_result() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
        }
        for format in [
            presentation::Format::Human,
            presentation::Format::Json,
            presentation::Format::Jsonl,
        ] {
            let capture = Capture::new("pack generate-id", false, DisplayOptions::default());
            capture
                .publish(
                    "pack generate-id",
                    presentation::Package {
                        package_id: "00000000000000000000000000000001".into(),
                    },
                )
                .unwrap();
            let reply = capture.finish(None, false);
            assert_eq!(reply.exit_code(), 0);
            assert_eq!(reply.render(format, &mut Closed, &mut io::sink()), 1);
        }
    }
}
