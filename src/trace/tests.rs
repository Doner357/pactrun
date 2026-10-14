use super::*;

fn scratch() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/alpha8-trace-tests");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}

fn await_completion(trace: &Trace, code: i32) -> Completion {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let completed = trace.finish(code).unwrap();
        if !matches!(completed.persistence, PersistenceStatus::Pending) {
            return completed;
        }
        assert!(
            Instant::now() < deadline,
            "diagnostic writer did not reach a terminal receipt"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

// Test-ID: PR-TEST-0804
// Verifies: PR-REQ-0397
#[test]
fn traced_read_preserves_bytes_retries_and_vectored_errors_without_recording_content() {
    struct Source {
        interrupted: bool,
        bytes: io::Cursor<Vec<u8>>,
    }
    impl Read for Source {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::ErrorKind::Interrupted.into());
            }
            let count = bytes.len().min(3);
            self.bytes.read(&mut bytes[..count])
        }
        fn read_vectored(&mut self, _: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private-source-sentinel",
            ))
        }
    }
    let bytes = b"private-input-sentinel\0\xff";
    for detailed in [false, true] {
        let temp = scratch();
        let location = Location::startup(temp.path().join("reads.sqlite3"));
        let trace = Arc::new(Trace::default());
        trace.begin();
        trace.configure(
            Some(location.clone()),
            None,
            TracePolicy {
                detailed,
                ..TracePolicy::startup()
            },
            TracePolicy::default(),
        );
        let mut source = trace.reader(Source {
            interrupted: false,
            bytes: io::Cursor::new(bytes.to_vec()),
        });
        let mut actual = Vec::new();
        source.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, bytes);
        let mut destination = [0; 8];
        let error = source
            .read_vectored(&mut [io::IoSliceMut::new(&mut destination)])
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "private-source-sentinel");
        let completion = await_completion(&trace, 0);
        let document = crate::trace_journal::read(&location, completion.id)
            .unwrap()
            .unwrap();
        let totals = &document.totals[&TracePhase::InputRead];
        assert!(totals.started > 2);
        assert_eq!(totals.finished, totals.started);
        assert_eq!(totals.failed, 2);
        assert_eq!(
            document
                .events
                .iter()
                .any(|event| event.phase == TracePhase::InputRead),
            detailed
        );
        let text = serde_json::to_string(&document).unwrap();
        assert!(
            !text.contains("private-input-sentinel") && !text.contains("private-source-sentinel")
        );
    }
}

// Test-ID: PR-TEST-0760
// Verifies: PR-REQ-0397, PR-REQ-0398, PR-REQ-0399
#[test]
fn core_trace_writer_records_safe_failures_and_storage_failure_does_not_change_results() {
    let temp = scratch();
    let location = Location::startup(temp.path().join("startup/diagnostics.sqlite3"));
    let trace = Arc::new(Trace::default());
    trace.begin();
    trace.operation(TraceOperation::ActionInvoke);
    trace.configure(
        Some(location.clone()),
        None,
        TracePolicy::startup(),
        TracePolicy::default(),
    );
    let value: Result<u8, ()> = trace.measure(TracePhase::StoreOpen, || Err(()));
    assert!(value.is_err());
    {
        let _unfinished = trace.span(TracePhase::Command);
    }
    let finished = await_completion(&trace, 1);
    assert!(matches!(finished.persistence, PersistenceStatus::Complete));
    let stored = crate::trace_journal::read(&location, finished.id)
        .unwrap()
        .unwrap();
    assert!(stored.closed && stored.valid());
    assert_eq!(stored.exit_code, Some(1));
    assert_eq!(stored.totals[&TracePhase::StoreOpen].failed, 1);
    assert_eq!(stored.totals[&TracePhase::Command].unfinished, 1);
    let foreign = temp.path().join("foreign");
    std::fs::write(&foreign, b"do not overwrite").unwrap();
    let trace = Arc::new(Trace::default());
    trace.begin();
    trace.configure(
        Some(Location::startup(foreign.clone())),
        None,
        TracePolicy::startup(),
        TracePolicy::default(),
    );
    assert_eq!(
        trace
            .measure::<_, ()>(TracePhase::Command, || Ok(7))
            .unwrap(),
        7
    );
    assert!(matches!(
        await_completion(&trace, 0).persistence,
        PersistenceStatus::Unavailable
    ));
    assert_eq!(std::fs::read(foreign).unwrap(), b"do not overwrite");
}

// Test-ID: PR-TEST-0761
// Verifies: PR-REQ-0399
#[test]
fn core_trace_suppression_and_store_association_do_not_provision_a_store() {
    let temp = scratch();
    let startup = Location::startup(temp.path().join("startup/log.sqlite3"));
    let trace = Arc::new(Trace::default());
    trace.begin();
    trace.suppress();
    trace.configure(
        Some(startup.clone()),
        None,
        TracePolicy::startup(),
        TracePolicy::default(),
    );
    assert!(trace.finish(0).is_none());
    assert!(!temp.path().join("startup").exists());
    let root = temp.path().join("store");
    std::fs::create_dir_all(root.join("database")).unwrap();
    let trace = Arc::new(Trace::default());
    trace.begin();
    trace.configure(
        Some(startup),
        Some(&root),
        TracePolicy::startup(),
        TracePolicy::default(),
    );
    let run = RunId::from_bytes([3; 16]);
    trace.associate(run);
    trace
        .measure::<_, ()>(TracePhase::Command, || Ok(()))
        .unwrap();
    let finished = await_completion(&trace, 0);
    assert!(matches!(finished.persistence, PersistenceStatus::Complete));
    assert_eq!(
        crate::trace_journal::read_run(&Location::store(&root), run)
            .unwrap()
            .unwrap()
            .id,
        finished.id
    );
    assert!(!root.join("database/pactrun.sqlite3").exists());
    assert_eq!(
        crate::trace_journal::read_run(&Location::store(&root), run)
            .unwrap()
            .unwrap()
            .retention,
        crate::domain::TraceRetention::Unavailable,
    );
    assert!(!root.join("staging").exists());
    let unsafe_location = Location::startup(root.join("nested/../logs/log.sqlite3"));
    assert!(unsafe_location.overlaps_store(&root));
    assert!(Location::startup(root.join("log.sqlite3")).overlaps_store(&root));
}
