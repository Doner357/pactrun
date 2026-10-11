//! One execution event stream and typed final result, with format-specific views.
use super::*;
use crate::hook::delivery::{self, Delivery};
use serde::Serialize;
use std::sync::Arc;

pub(super) fn execution_options(command: &Command) -> (bool, bool) {
    match command {
        Command::Invoke {
            plan,
            cancel_on_output_close,
            ..
        } => (!*plan, *cancel_on_output_close),
        Command::Snapshot(snapshots::SnapshotCommand::Execute { options, .. }) => {
            (!options.plan, options.cancel_on_output_close)
        }
        Command::CreateAndRestore(c) => (true, c.options.cancel_on_output_close),
        Command::Retirement(retirements::RetirementCommand::Execute { options, .. }) => {
            (!options.plan, options.cancel_on_output_close)
        }
        Command::Migration(migrations::MigrationCommand::Plan {
            plan,
            cancel_on_output_close,
            ..
        }) => (!*plan, *cancel_on_output_close),
        _ => (false, false),
    }
}

pub(super) struct ResultWriter<'a> {
    out: &'a mut dyn Write,
    command: Option<&'a str>,
    started: bool,
    failed: bool,
}
impl<'a> ResultWriter<'a> {
    pub(super) fn new(out: &'a mut dyn Write, command: Option<&'a str>) -> Self {
        Self {
            out,
            command,
            started: false,
            failed: false,
        }
    }
    pub(super) fn finish(&mut self) -> io::Result<()> {
        if self.failed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if self.started {
            self.out.write_all(b"}\n")?;
            self.out.flush()?;
        }
        Ok(())
    }
}
impl Write for ResultWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = (|| {
            if !self.started {
                let mut header = serde_json::to_vec(
                    &serde_json::json!({"format":"pactrun.cli","format_version":presentation::FORMAT_VERSION,"sequence":"1","received_at_unix_ms":delivery::now(),"command":self.command,"type":"result"}),
                )?;
                header.pop();
                header.extend_from_slice(b",\"response\":");
                self.out.write_all(&header)?;
                self.started = true;
            }
            // Responses are compact UTF-8 documents with exactly one final LF.
            // JSON string newlines are escaped and remain untouched.
            for part in bytes.split(|b| *b == b'\n') {
                self.out.write_all(part)?;
            }
            Ok(bytes.len())
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn flush(&mut self) -> io::Result<()> {
        let r = self.out.flush();
        if r.is_err() {
            self.failed = true;
        }
        r
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct DeliveryResult {
    pub(super) complete: bool,
    error: Option<String>,
    event_count: String,
    streams: Vec<delivery::StreamSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, schemars(with = "Option<Vec<delivery::Record>>"))]
    events: Option<delivery::Events>,
}

fn write_json<T: Serialize>(out: &mut dyn Write, value: &T) -> io::Result<()> {
    serde_json::to_writer(&mut *out, value)
        .map_err(|e| io::Error::new(e.io_error_kind().unwrap_or(io::ErrorKind::Other), e))?;
    out.write_all(b"\n")?;
    out.flush()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_stream(
    command: Command,
    storage_root: Option<OsString>,
    stdin: &mut (dyn Read + Send),
    stdout: &mut dyn Write,
    stderr: &mut (dyn Write + Send),
    cancellation: &ActionCancellation,
    format: presentation::Format,
    cancel_on_close: bool,
) -> i32 {
    let name = presentation::command_name(&command);
    let Some(root) = storage_root.as_ref().map(PathBuf::from) else {
        // Preserve ordinary missing-storage diagnostics without creating a spool.
        if format == presentation::Format::Jsonl {
            let mut writer = ResultWriter::new(stdout, Some(name));
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
        return run_selected(
            command,
            storage_root,
            stdin,
            stdout,
            stderr,
            cancellation,
            format,
        );
    };
    let prepared = (|| {
        let _store = PactrunApplication::open_read_only(&root).map_err(app_error)?;
        Delivery::start(&root, name).map_err(|_| {
            CliError::operation(
                "Unable to prepare output delivery; check storage access and free space",
            )
        })
    })();
    let delivery = match prepared {
        Ok(d) => d,
        Err(error) => return report_cli_failure(format, Some(name), false, error, stdout, stderr),
    };
    cancellation.delivery.set(delivery.clone());
    cancellation.diagnostics.disable_live();
    struct ResetDelivery<'a>(&'a ActionCancellation);
    impl Drop for ResetDelivery<'_> {
        fn drop(&mut self) {
            self.0.delivery.clear();
        }
    }
    let _reset = ResetDelivery(cancellation);
    let human = format == presentation::Format::Human;
    // Interactive diagnostics are displayed after the terminal is released.
    // Keep their disk-backed records until then, never an unbounded RAM queue.
    let mut cursor = delivery.cursor(!human);
    let mut output_failed = false;
    let mut read_failed = false;
    #[cfg(test)]
    let faults = crate::application::finalization_faults_for_test();
    #[cfg(test)]
    let admission_fault = crate::executor::take_admission_fault_for_test();
    #[cfg(test)]
    let capture_clock = crate::hook::capture_clock_for_test();
    let response = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            #[cfg(test)]
            crate::application::install_finalization_faults_for_test(faults);
            #[cfg(test)]
            crate::hook::install_capture_clock_for_test(capture_clock);
            #[cfg(test)]
            if admission_fault {
                crate::executor::fail_next_admission_for_test();
            }
            // Machine diagnostics travel in typed records and the final response.
            // Backend human progress messages must not leak into this interface.
            let response = collect_reply(
                command,
                storage_root,
                stdin,
                &mut io::sink(),
                &mut io::sink(),
                cancellation,
                human,
            );
            delivery.finish();
            response
        });
        loop {
            if (human || format == presentation::Format::Jsonl) && !output_failed && !read_failed {
                loop {
                    match cursor.next() {
                        Ok(Some(record)) => {
                            let written = if human {
                                write_human_event(&record, stdout, stderr, false)
                            } else {
                                write_json(stdout, &record)
                            };
                            if let Err(error) = written {
                                output_failed = true;
                                delivery.output_closed();
                                if cancel_on_close && error.kind() == io::ErrorKind::BrokenPipe {
                                    cancellation.request();
                                }
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(_) => {
                            delivery.fail("delivery_read_error");
                            read_failed = true;
                            break;
                        }
                    }
                }
            }
            if worker.is_finished() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        worker.join().unwrap_or_else(|_| {
            delivery.fail("execution_response_unavailable");
            delivery.finish();
            reply::Reply::unavailable(name, delivery.known_run())
        })
    });
    if output_failed {
        if human {
            let _ = writeln!(stderr, "Output delivery incomplete; inspect retained Runs.");
            if let Some(run) = delivery.known_run() {
                let _ = writeln!(stderr, "pactrun run show {run}");
            }
        }
        return 1;
    }
    if (human || format == presentation::Format::Jsonl) && !read_failed {
        loop {
            match cursor.next() {
                Ok(Some(record)) => {
                    let written = if human {
                        write_human_event(&record, stdout, stderr, false)
                    } else {
                        write_json(stdout, &record)
                    };
                    if written.is_err() {
                        return 1;
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    delivery.fail("delivery_read_error");
                    break;
                }
            }
        }
    }
    let code = response.exit_code();
    let summary = delivery.summary();
    let failed = !summary.complete;
    if human {
        let mut deferred = delivery.cursor(false);
        loop {
            match deferred.next() {
                Ok(Some(record)) => {
                    if write_human_event(&record, stdout, stderr, true).is_err() {
                        return 1;
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    let _ = writeln!(stderr, "Deferred diagnostics could not be fully delivered.");
                    return 1;
                }
            }
        }
        let rendered = response.render(format, stdout, stderr);
        if failed {
            let _ = writeln!(
                stderr,
                "Output delivery incomplete; retained operation outcomes are unchanged."
            );
            return 1;
        }
        return rendered;
    }
    let sequence = summary
        .events
        .parse::<u64>()
        .unwrap_or(0)
        .saturating_add(1)
        .to_string();
    let result = DeliveryResult {
        complete: summary.complete,
        error: summary.error,
        event_count: summary.events,
        streams: summary.streams,
        events: (format == presentation::Format::Json)
            .then(|| delivery::Events(Arc::clone(&delivery))),
    };
    let written = response.json(
        stdout,
        Some(&result),
        (format == presentation::Format::Jsonl).then_some(sequence.as_str()),
    );
    if written.is_err() || failed { 1 } else { code }
}

fn write_human_event(
    record: &delivery::Record,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    deferred: bool,
) -> io::Result<()> {
    use base64::Engine as _;
    match &record.event {
        delivery::Event::AcceptancePending { candidate_run_id } if !deferred => {
            writeln!(
                stderr,
                "Run candidate {candidate_run_id}: confirming acceptance; owner retained, no replacement Run"
            )?;
            stderr.flush()
        }
        delivery::Event::CoreProgress {
            run_id,
            phase,
            interactive,
        } if *interactive == deferred => {
            let text = match phase {
                delivery::CorePhase::WaitingForProcessTree => "waiting for Hook processes to exit",
                delivery::CorePhase::RetryingStorage => {
                    "retrying storage access; execution ownership retained"
                }
                delivery::CorePhase::FinalizingExecution => "finishing execution",
            };
            writeln!(stderr, "Run {run_id}: {text}")?;
            stderr.flush()
        }
        delivery::Event::Output { channel, data, .. } if !deferred => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let out: &mut dyn Write = if *channel == delivery::Channel::Stdout {
                stdout
            } else {
                stderr
            };
            out.write_all(&bytes)?;
            out.flush()
        }
        delivery::Event::CoreDiagnostic {
            context,
            diagnostic,
        } if context.interactive == deferred => {
            match diagnostic {
                crate::domain::CoreDiagnostic::HelperFailure { failure } => {
                    writeln!(stderr, "Pactrun {failure}")?
                }
                crate::domain::CoreDiagnostic::CollectionIncomplete { reason } => {
                    writeln!(stderr, "Pactrun: {}", reason.text())?
                }
            }
            stderr.flush()
        }
        delivery::Event::Diagnostic {
            context,
            kind,
            severity,
            completion_status,
            code,
            message,
        } if context.interactive == deferred => {
            if kind == "completion" && code.is_none() && message.is_none() {
                return Ok(());
            }
            let label = if kind == "completion" {
                match completion_status.as_deref() {
                    Some("success") => "completed",
                    Some("failure") => "failed",
                    _ => "completion",
                }
            } else {
                severity.as_deref().unwrap_or(kind)
            };
            writeln!(
                stderr,
                "Hook {}{}{}",
                catalog::safe(label),
                code.as_ref()
                    .map(|s| format!(" [{}]", catalog::safe(s)))
                    .unwrap_or_default(),
                message
                    .as_ref()
                    .map(|s| format!(": {}", catalog::safe(s)))
                    .unwrap_or_default()
            )?;
            stderr.flush()
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Supporting coverage for PR-TEST-0690.
    #[test]
    fn core_diagnostics_wait_for_the_interactive_terminal() {
        use crate::domain::{
            CoreDiagnostic, HelperCommand, HelperFailure, HelperReason, HelperStage,
        };
        let context = delivery::Context {
            interactive: true,
            run_id: "0".repeat(32),
            hook_ordinal: "1".into(),
            operation: "action".into(),
            action_id: Some("inspect".into()),
            revision: delivery::Revision {
                package_id: "0".repeat(32),
                content_digest: format!("sha256:{}", "0".repeat(64)),
            },
            source_revision: None,
            target_revision: None,
        };
        let record = delivery::Record {
            format: presentation::FORMAT_KIND.into(),
            format_version: presentation::FORMAT_VERSION.into(),
            sequence: "1".into(),
            received_at_unix_ms: None,
            command: "invoke".into(),
            event: delivery::Event::CoreDiagnostic {
                context,
                diagnostic: CoreDiagnostic::HelperFailure {
                    failure: HelperFailure {
                        command: HelperCommand::Diagnostic,
                        stage: HelperStage::ReadJson,
                        reason: HelperReason::NotFound,
                    },
                },
            },
        };
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        write_human_event(&record, &mut stdout, &mut stderr, false).unwrap();
        assert!(stdout.is_empty() && stderr.is_empty());
        write_human_event(&record, &mut stdout, &mut stderr, true).unwrap();
        assert!(
            String::from_utf8(stderr)
                .unwrap()
                .contains("Pactrun helper diagnostic: read JSON file")
        );
    }
}
