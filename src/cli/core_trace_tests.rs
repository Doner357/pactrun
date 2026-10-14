use super::*;
use crate::domain::TracePolicy;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

struct Closed;
impl Write for Closed {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
}

// Test-ID: PR-TEST-0793
// Verifies: PR-REQ-0397, PR-REQ-0399
#[test]
fn reply_delivery_phase_does_not_confuse_operation_failure_with_output_failure() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/trace-delivery-tests");
    fs::create_dir_all(&parent).unwrap();
    for format in [
        presentation::Format::Human,
        presentation::Format::Json,
        presentation::Format::Jsonl,
    ] {
        for (operation_failed, output_failed) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let temp = tempfile::tempdir_in(&parent).unwrap();
            let location = Location::startup(temp.path().join("diagnostics.sqlite3"));
            let trace = Arc::new(crate::trace::Trace::default());
            trace.begin();
            trace.configure(
                Some(location.clone()),
                None,
                TracePolicy::startup(),
                TracePolicy::default(),
            );
            let capture =
                reply::Capture::new("pack generate-id", false, reply::DisplayOptions::default());
            if !operation_failed {
                capture
                    .publish(
                        "pack generate-id",
                        presentation::Package {
                            package_id: "00000000000000000000000000000001".into(),
                        },
                    )
                    .unwrap();
            }
            let reply = capture.finish(
                operation_failed.then(|| CliError::operation("private-output-sentinel")),
                false,
            );
            assert_eq!(reply.exit_code(), i32::from(operation_failed));
            let mut stdout: Box<dyn Write> = if output_failed {
                Box::new(Closed)
            } else {
                Box::new(Vec::<u8>::new())
            };
            let mut stderr: Box<dyn Write> = if output_failed {
                Box::new(Closed)
            } else {
                Box::new(Vec::<u8>::new())
            };
            let code = render_reply(reply, format, &mut stdout, &mut stderr, &trace);
            assert_eq!(code, i32::from(operation_failed || output_failed));
            let finished = trace.finish(code).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            let stored = loop {
                if let Ok(Some(document)) = trace_journal::read(&location, finished.id)
                    && document.closed
                {
                    break document;
                }
                assert!(Instant::now() < deadline, "delivery trace did not close");
                std::thread::sleep(Duration::from_millis(10));
            };
            let totals = &stored.totals[&TracePhase::OutputDelivery];
            assert_eq!(totals.started, 1);
            assert_eq!(totals.finished, 1);
            assert_eq!(totals.failed, u64::from(output_failed));
            assert_eq!(totals.unfinished, 0);
            assert!(
                !serde_json::to_string(&stored)
                    .unwrap()
                    .contains("private-output-sentinel")
            );
        }
    }
}

#[test]
fn output_observation_preserves_short_writes_and_interrupted_retry() {
    struct Short {
        interrupted: bool,
        bytes: Vec<u8>,
    }
    impl Write for Short {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::ErrorKind::Interrupted.into());
            }
            let count = bytes.len().min(2);
            self.bytes.extend_from_slice(&bytes[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut sink = Short {
        interrupted: false,
        bytes: Vec::new(),
    };
    let mut observed = DeliveryObservation {
        inner: &mut sink,
        failed: false,
    };
    observed.write_all(b"raw\0bytes\xff").unwrap();
    observed.flush().unwrap();
    assert!(!observed.failed);
    assert_eq!(sink.bytes, b"raw\0bytes\xff");
}
