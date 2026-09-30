//! One-shot and incremental machine delivery share the same final V1 response.
use super::*;
use crate::hook::delivery::{self, Delivery};
use serde::{Deserialize, Serialize};
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

#[derive(Serialize, Deserialize)]
struct WireResponse {
    format: String,
    format_version: String,
    command: Option<String>,
    status: String,
    result: Option<Box<serde_json::value::RawValue>>,
    error: Option<Box<serde_json::value::RawValue>>,
}
#[derive(Serialize)]
struct DeliveredResponse {
    #[serde(flatten)]
    response: WireResponse,
    delivery: DeliveryResult,
}
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct DeliveryResult {
    complete: bool,
    error: Option<String>,
    event_count: String,
    streams: Vec<delivery::StreamSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, schemars(with = "Option<Vec<delivery::Record>>"))]
    events: Option<delivery::Events>,
}
#[derive(Serialize)]
struct ResultEvent<'a> {
    format: &'static str,
    format_version: &'static str,
    sequence: String,
    received_at_unix_ms: Option<String>,
    command: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    response: &'a DeliveredResponse,
}

fn write_json<T: Serialize>(out: &mut dyn Write, value: &T) -> io::Result<()> {
    serde_json::to_writer(&mut *out, value)
        .map_err(|e| io::Error::new(e.io_error_kind().unwrap_or(io::ErrorKind::Other), e))?;
    out.write_all(b"\n")?;
    out.flush()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_machine(
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
            presentation::Format::Json,
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
    struct ResetDelivery<'a>(&'a ActionCancellation);
    impl Drop for ResetDelivery<'_> {
        fn drop(&mut self) {
            self.0.delivery.clear();
        }
    }
    let _reset = ResetDelivery(cancellation);
    let mut cursor = delivery.cursor(true);
    let mut output_failed = false;
    let mut read_failed = false;
    let (code, bytes) = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let mut bytes = Vec::new();
            // Machine diagnostics travel in typed records and the final response.
            // Backend human progress messages must not leak into this interface.
            let code = run_selected(
                command,
                storage_root,
                stdin,
                &mut bytes,
                &mut io::sink(),
                cancellation,
                presentation::Format::Json,
            );
            delivery.finish();
            (code, bytes)
        });
        loop {
            if format == presentation::Format::Jsonl && !output_failed && !read_failed {
                loop {
                    match cursor.next() {
                        Ok(Some(record)) => {
                            if let Err(error) = write_json(stdout, &record) {
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
            (1, Vec::new())
        })
    });
    if output_failed {
        return 1;
    }
    if format == presentation::Format::Jsonl && !read_failed {
        loop {
            match cursor.next() {
                Ok(Some(record)) => {
                    if write_json(stdout, &record).is_err() {
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
    let mut response: WireResponse = match serde_json::from_slice(&bytes) {
        Ok(response) => response,
        Err(_) => WireResponse {
            format: presentation::FORMAT_KIND.into(), format_version: presentation::FORMAT_VERSION.into(), command: Some(name.into()), status: "failure".into(),
            result: delivery.known_run().map(|run| serde_json::value::to_raw_value(&serde_json::json!({"run_id":run})).expect("known Run projection")),
            error: Some(serde_json::value::to_raw_value(&serde_json::json!({"kind":"operation","message":"Execution response unavailable; inspect retained Runs","reference":null})).expect("fixed error")),
        },
    };
    let summary = delivery.summary();
    let failed = !summary.complete;
    if failed {
        response.status = "failure".into();
        if response.error.is_none() {
            response.error=Some(serde_json::value::to_raw_value(&serde_json::json!({"kind":"output","message":"Output delivery incomplete","reference":null})).expect("fixed error"));
        }
    }
    let sequence = summary
        .events
        .parse::<u64>()
        .unwrap_or(0)
        .saturating_add(1)
        .to_string();
    let result = DeliveredResponse {
        response,
        delivery: DeliveryResult {
            complete: summary.complete,
            error: summary.error,
            event_count: summary.events,
            streams: summary.streams,
            events: (format == presentation::Format::Json)
                .then(|| delivery::Events(Arc::clone(&delivery))),
        },
    };
    let written = if format == presentation::Format::Jsonl {
        write_json(
            stdout,
            &ResultEvent {
                format: presentation::FORMAT_KIND,
                format_version: presentation::FORMAT_VERSION,
                sequence,
                received_at_unix_ms: delivery::now(),
                command: name,
                kind: "result",
                response: &result,
            },
        )
    } else {
        write_json(stdout, &result)
    };
    if written.is_err() || failed { 1 } else { code }
}
