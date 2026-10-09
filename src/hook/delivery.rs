//! Ephemeral execution delivery, independent of historical evidence and Run outcome.
use crate::{
    domain::{HookText, RunId},
    managed_data::{ExecutionDirectory, StagingSession},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) const CHUNK: usize = 64 * 1024;
const SEGMENT: u64 = 16 * 1024 * 1024;
const RECORD_LIMIT: u64 = 2 * super::protocol::MAX_PAYLOAD as u64 + 64 * 1024;

#[derive(Debug, Default)]
pub(crate) struct DeliverySlot(Mutex<Option<Arc<Delivery>>>);
impl DeliverySlot {
    pub(crate) fn set(&self, value: Arc<Delivery>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
    }
    pub(crate) fn clear(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take();
    }
    pub(crate) fn get(&self) -> Option<Arc<Delivery>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub(crate) fn accepted(&self, run: RunId) {
        if let Some(d) = self.get() {
            d.accepted(run);
        }
    }
    pub(crate) fn progress(&self, run: RunId, phase: CorePhase) {
        if let Some(d) = self.get() {
            d.progress(run, phase);
        }
    }
    pub(crate) fn acceptance_pending(&self, candidate: RunId) {
        if let Some(d) = self.get()
            && d.pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(candidate)
        {
            d.emit(
                Event::AcceptancePending {
                    candidate_run_id: candidate.to_string(),
                },
                false,
            );
        }
    }
    pub(super) fn scope(
        &self,
        run: RunId,
        session: &serde_json::Value,
        interactive: bool,
    ) -> Option<Scope> {
        self.get().map(|d| d.scope(run, session, interactive))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) struct Context {
    #[serde(default)]
    pub(crate) interactive: bool,
    pub(crate) run_id: String,
    pub(crate) hook_ordinal: String,
    pub(crate) operation: String,
    pub(crate) action_id: Option<String>,
    pub(crate) revision: Revision,
    pub(crate) source_revision: Option<Revision>,
    pub(crate) target_revision: Option<Revision>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) struct Revision {
    pub(crate) package_id: String,
    pub(crate) content_digest: String,
}
fn revision(value: &serde_json::Value) -> Option<Revision> {
    Some(Revision {
        package_id: value.get("package_id")?.as_str()?.into(),
        content_digest: value.get("revision_content_digest")?.as_str()?.into(),
    })
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum Channel {
    Stdout,
    Stderr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum CorePhase {
    WaitingForProcessTree,
    RetryingStorage,
    FinalizingExecution,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Event {
    AcceptancePending {
        candidate_run_id: String,
    },
    RunAccepted {
        run_id: String,
    },
    CoreProgress {
        run_id: String,
        phase: CorePhase,
        interactive: bool,
    },
    Output {
        context: Context,
        channel: Channel,
        offset: String,
        encoding: String,
        data: String,
        byte_length: String,
    },
    Diagnostic {
        context: Context,
        kind: String,
        severity: Option<String>,
        code: Option<String>,
        message: Option<String>,
    },
}
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) struct Record {
    pub(crate) format: String,
    pub(crate) format_version: String,
    pub(crate) sequence: String,
    pub(crate) received_at_unix_ms: Option<String>,
    pub(crate) command: String,
    #[serde(flatten)]
    pub(crate) event: Event,
}
pub(crate) fn now() -> Option<String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis().to_string())
}

#[derive(Clone, Debug, Serialize, Default)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) struct Summary {
    pub(crate) complete: bool,
    pub(crate) error: Option<String>,
    pub(crate) events: String,
    pub(crate) streams: Vec<StreamSummary>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) struct StreamSummary {
    context: Context,
    channel: Channel,
    byte_length: String,
}
#[derive(Clone, Copy, Debug, Default)]
struct Position {
    segment: u64,
    bytes: u64,
    sequence: u64,
}
#[derive(Debug, Default)]
struct Shared {
    published: Mutex<Position>,
    failed: Mutex<Option<String>>,
    discard: AtomicBool,
    finish_readers: AtomicBool,
    readers: AtomicUsize,
    writer_done: AtomicBool,
    streams: Mutex<BTreeMap<(String, Channel), (Context, u64)>>,
}
impl Shared {
    fn fail(&self, reason: &str) {
        let mut f = self.failed.lock().unwrap_or_else(|e| e.into_inner());
        if f.is_none() {
            *f = Some(reason.into());
        }
    }
}
#[derive(Debug)]
struct Storage {
    _lease: StagingSession,
    directory: ExecutionDirectory,
}
impl Storage {
    fn segment(&self, n: u64) -> PathBuf {
        self.directory.root().join(format!("events-{n:020}.jsonl"))
    }
}
#[derive(Debug)]
pub(crate) struct Delivery {
    storage: Arc<Storage>,
    shared: Arc<Shared>,
    sender: Mutex<Option<SyncSender<Event>>>,
    ordinal: AtomicU64,
    accepted: Mutex<BTreeSet<RunId>>,
    pending: Mutex<BTreeSet<RunId>>,
    progress: Mutex<BTreeMap<RunId, CorePhase>>,
    interactive: Mutex<BTreeSet<RunId>>,
}
impl Delivery {
    pub(crate) fn start(root: &Path, command: &str) -> io::Result<Arc<Self>> {
        let lease = StagingSession::prepare(root).map_err(io::Error::other)?;
        let directory = lease
            .create_delivery_directory()
            .map_err(io::Error::other)?;
        let storage = Arc::new(Storage {
            _lease: lease,
            directory,
        });
        let (_, file) = storage
            .directory
            .create_file(Path::new("events-00000000000000000000.jsonl"))
            .map_err(io::Error::other)?;
        let shared = Arc::new(Shared::default());
        let (sender, receiver) = mpsc::sync_channel(32);
        let s = shared.clone();
        let st = storage.clone();
        let command = command.to_owned();
        thread::Builder::new()
            .name("machine-delivery-spool".into())
            .spawn(move || {
                let mut file = file;
                let mut position = Position::default();
                for event in receiver {
                    if s.discard.load(Ordering::Acquire) {
                        continue;
                    }
                    let result = (|| -> io::Result<()> {
                        let sequence = position
                            .sequence
                            .checked_add(1)
                            .ok_or_else(|| io::Error::other("delivery counter overflow"))?;
                        let record = Record {
                            format: "pactrun.cli".into(),
                            format_version: crate::domain::VersionDomain::Machine
                                .current_text()
                                .into(),
                            sequence: sequence.to_string(),
                            received_at_unix_ms: now(),
                            command: command.clone(),
                            event,
                        };
                        let mut bytes = serde_json::to_vec(&record)?;
                        bytes.push(b'\n');
                        if bytes.len() as u64 > RECORD_LIMIT {
                            return Err(io::Error::other("delivery record exceeds frame profile"));
                        }
                        if position.bytes > 0 && position.bytes + bytes.len() as u64 > SEGMENT {
                            position.segment += 1;
                            position.bytes = 0;
                            let (_, next) = st
                                .directory
                                .create_file(Path::new(&format!(
                                    "events-{:020}.jsonl",
                                    position.segment
                                )))
                                .map_err(io::Error::other)?;
                            file = next;
                        }
                        file.write_all(&bytes)?;
                        file.flush()?;
                        if let Event::Output {
                            context,
                            channel,
                            byte_length,
                            ..
                        } = &record.event
                        {
                            let n = byte_length.parse::<u64>().map_err(io::Error::other)?;
                            let mut streams = s.streams.lock().unwrap_or_else(|e| e.into_inner());
                            let value = streams
                                .entry((context.hook_ordinal.clone(), *channel))
                                .or_insert((context.clone(), 0));
                            value.1 = value
                                .1
                                .checked_add(n)
                                .ok_or_else(|| io::Error::other("delivery size overflow"))?;
                        }
                        position.bytes += bytes.len() as u64;
                        position.sequence = sequence;
                        *s.published.lock().unwrap_or_else(|e| e.into_inner()) = position;
                        Ok(())
                    })();
                    if result.is_err() {
                        s.fail("storage_error");
                        s.discard.store(true, Ordering::Release);
                    }
                }
                s.writer_done.store(true, Ordering::Release);
            })?;
        Ok(Arc::new(Self {
            storage,
            shared,
            sender: Mutex::new(Some(sender)),
            ordinal: AtomicU64::new(0),
            accepted: Mutex::new(BTreeSet::new()),
            pending: Mutex::new(BTreeSet::new()),
            progress: Mutex::new(BTreeMap::new()),
            interactive: Mutex::new(BTreeSet::new()),
        }))
    }
    fn emit(&self, event: Event, blocking: bool) {
        if self.shared.discard.load(Ordering::Acquire) {
            return;
        }
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let Some(sender) = sender else {
            return;
        };
        if blocking {
            if sender.send(event).is_err() {
                self.shared.fail("collector_stopped");
            }
        } else {
            match sender.try_send(event) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => self.shared.fail("capture_overflow"),
                Err(TrySendError::Disconnected(_)) => self.shared.fail("collector_stopped"),
            }
        }
    }
    fn accepted(&self, run: RunId) {
        if self
            .accepted
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(run)
        {
            self.emit(
                Event::RunAccepted {
                    run_id: run.to_string(),
                },
                false,
            );
        }
    }
    fn progress(&self, run: RunId, phase: CorePhase) {
        // An uncertain candidate is not proof of durable acceptance.
        if !self
            .accepted
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&run)
        {
            return;
        }
        let interactive = self
            .interactive
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&run);
        let mut progress = self.progress.lock().unwrap_or_else(|e| e.into_inner());
        if progress.insert(run, phase) != Some(phase) {
            self.emit(
                Event::CoreProgress {
                    run_id: run.to_string(),
                    phase,
                    interactive,
                },
                false,
            );
        }
    }
    fn scope(self: Arc<Self>, run: RunId, session: &serde_json::Value, interactive: bool) -> Scope {
        self.accepted(run);
        if interactive {
            self.interactive
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(run);
        }
        let operation = &session["operation"];
        let context = Context {
            interactive,
            run_id: run.to_string(),
            hook_ordinal: (self.ordinal.fetch_add(1, Ordering::Relaxed) + 1).to_string(),
            operation: operation["kind"].as_str().unwrap_or("unknown").into(),
            action_id: operation["action_id"].as_str().map(str::to_owned),
            revision: revision(&session["revision"]).expect("validated Session Revision"),
            source_revision: revision(&operation["source_revision"]),
            target_revision: (operation["kind"] == "migration")
                .then(|| revision(&session["revision"]).expect("validated Migration target")),
        };
        Scope {
            delivery: self,
            context,
        }
    }
    pub(crate) fn output_closed(&self) {
        self.shared.fail("output_closed");
        self.shared.discard.store(true, Ordering::Release);
    }
    pub(crate) fn known_run(&self) -> Option<String> {
        self.accepted
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .last()
            .map(ToString::to_string)
    }
    pub(crate) fn fail(&self, reason: &str) {
        self.shared.fail(reason);
    }
    pub(crate) fn finish(&self) {
        self.shared.finish_readers.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.shared.readers.load(Ordering::Acquire) > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if self.shared.readers.load(Ordering::Acquire) > 0 {
            self.shared.fail("output_tail_unavailable");
        }
        self.sender.lock().unwrap_or_else(|e| e.into_inner()).take();
        while !self.shared.writer_done.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if !self.shared.writer_done.load(Ordering::Acquire) {
            self.shared.fail("collector_incomplete");
        }
    }
    pub(crate) fn summary(&self) -> Summary {
        let error = self
            .shared
            .failed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        Summary {
            complete: error.is_none() && self.shared.writer_done.load(Ordering::Acquire),
            error,
            events: self
                .shared
                .published
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .sequence
                .to_string(),
            streams: self
                .shared
                .streams
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .map(|((_, channel), (context, n))| StreamSummary {
                    context: context.clone(),
                    channel: *channel,
                    byte_length: n.to_string(),
                })
                .collect(),
        }
    }
    pub(crate) fn cursor(self: &Arc<Self>, reclaim: bool) -> Cursor {
        Cursor {
            delivery: self.clone(),
            segment: 0,
            offset: 0,
            reclaim,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Scope {
    delivery: Arc<Delivery>,
    context: Context,
}
impl Scope {
    pub(super) fn diagnostic(&self, text: &HookText) {
        use crate::domain::{DiagnosticKind, DiagnosticSeverity};
        self.delivery.emit(
            Event::Diagnostic {
                context: self.context.clone(),
                kind: match text.kind {
                    DiagnosticKind::Diagnostic => "diagnostic",
                    DiagnosticKind::Completion => "completion",
                    DiagnosticKind::ProtocolError => "protocol_error",
                }
                .into(),
                severity: text.severity.map(|s| {
                    match s {
                        DiagnosticSeverity::Info => "info",
                        DiagnosticSeverity::Warning => "warning",
                        DiagnosticSeverity::Error => "error",
                    }
                    .into()
                }),
                code: text.code.clone(),
                message: text.message.clone(),
            },
            false,
        );
    }
    pub(super) fn pipes(&self) -> io::Result<(File, File)> {
        let (out, write_out) = pipe()?;
        let (err, write_err) = pipe()?;
        self.drain(out, Channel::Stdout)?;
        self.drain(err, Channel::Stderr)?;
        Ok((write_out, write_err))
    }
    fn drain(&self, mut reader: File, channel: Channel) -> io::Result<()> {
        let scope = self.clone();
        self.delivery.shared.readers.fetch_add(1, Ordering::AcqRel);
        let spawned = thread::Builder::new()
            .name("hook-output-drain".into())
            .spawn(move || {
                let mut offset = 0u64;
                let mut buffer = vec![0u8; CHUNK];
                let mut finishing = None;
                loop {
                    match read_pipe(&mut reader, &mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            scope.delivery.emit(
                                Event::Output {
                                    context: scope.context.clone(),
                                    channel,
                                    offset: offset.to_string(),
                                    encoding: "base64".into(),
                                    data: STANDARD.encode(&buffer[..n]),
                                    byte_length: n.to_string(),
                                },
                                true,
                            );
                            let Some(next) = offset.checked_add(n as u64) else {
                                scope.delivery.shared.fail("size_overflow");
                                break;
                            };
                            offset = next;
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            if scope.delivery.shared.finish_readers.load(Ordering::Acquire) {
                                let start = finishing.get_or_insert_with(Instant::now);
                                if start.elapsed() >= Duration::from_secs(2) {
                                    scope.delivery.shared.fail("output_tail_unavailable");
                                    break;
                                }
                            }
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                        Err(_) => {
                            scope.delivery.shared.fail("reader_error");
                            break;
                        }
                    }
                }
                scope.delivery.shared.readers.fetch_sub(1, Ordering::AcqRel);
            });
        if let Err(error) = spawned {
            self.delivery.shared.readers.fetch_sub(1, Ordering::AcqRel);
            self.delivery.shared.fail("reader_unavailable");
            return Err(error);
        }
        Ok(())
    }
}

pub(crate) struct Cursor {
    delivery: Arc<Delivery>,
    segment: u64,
    offset: u64,
    reclaim: bool,
}
impl Cursor {
    pub(crate) fn next(&mut self) -> io::Result<Option<Record>> {
        loop {
            let published = *self
                .delivery
                .shared
                .published
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if self.segment > published.segment
                || (self.segment == published.segment && self.offset >= published.bytes)
            {
                return Ok(None);
            }
            let path = self.delivery.storage.segment(self.segment);
            let mut file = File::open(&path)?;
            use std::io::{Seek, SeekFrom};
            file.seek(SeekFrom::Start(self.offset))?;
            let mut line = Vec::new();
            let n = BufReader::new(file.take(RECORD_LIMIT + 1)).read_until(b'\n', &mut line)?;
            if n == 0 && self.segment < published.segment {
                if self.reclaim {
                    fs::remove_file(path)?;
                }
                self.segment += 1;
                self.offset = 0;
                continue;
            }
            if n == 0 {
                return Ok(None);
            }
            if n as u64 > RECORD_LIMIT || line.last() != Some(&b'\n') {
                return Err(io::Error::other("incomplete delivery record"));
            }
            self.offset += n as u64;
            return serde_json::from_slice(&line)
                .map(Some)
                .map_err(io::Error::other);
        }
    }
}
pub(crate) struct Events(pub(crate) Arc<Delivery>);
impl Serialize for Events {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = s.serialize_seq(None)?;
        let mut cursor = self.0.cursor(false);
        while let Some(record) = cursor.next().map_err(serde::ser::Error::custom)? {
            seq.serialize_element(&record)?;
        }
        seq.end()
    }
}

#[cfg(unix)]
fn pipe() -> io::Result<(File, File)> {
    let (read, write) = rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC)?;
    let read = File::from(read);
    let flags = rustix::fs::fcntl_getfl(&read)?;
    rustix::fs::fcntl_setfl(&read, flags | rustix::fs::OFlags::NONBLOCK)?;
    Ok((read, File::from(write)))
}
#[cfg(windows)]
fn pipe() -> io::Result<(File, File)> {
    pactrun_windows_ntfs::capture_pipe()
}
#[cfg(unix)]
fn read_pipe(file: &mut File, bytes: &mut [u8]) -> io::Result<usize> {
    file.read(bytes)
}
#[cfg(windows)]
fn read_pipe(file: &mut File, bytes: &mut [u8]) -> io::Result<usize> {
    pactrun_windows_ntfs::read_capture_pipe(file, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Supporting coverage for PR-TEST-0685: uncertain acceptance is not acceptance.
    #[test]
    fn core_progress_never_invents_acceptance_and_coalesces_repeated_phases() {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/delivery-tests");
        fs::create_dir_all(&parent).unwrap();
        let root = tempfile::tempdir_in(parent).unwrap();
        let delivery = Delivery::start(root.path(), "invoke").unwrap();
        let slot = DeliverySlot::default();
        slot.set(delivery.clone());
        let run = RunId::generate().unwrap();
        slot.acceptance_pending(run);
        slot.acceptance_pending(run);
        slot.progress(run, CorePhase::RetryingStorage);
        assert!(delivery.known_run().is_none());
        slot.accepted(run);
        slot.progress(run, CorePhase::RetryingStorage);
        slot.progress(run, CorePhase::RetryingStorage);
        delivery.finish();
        assert!(delivery.summary().complete);
        let mut cursor = delivery.cursor(false);
        assert!(matches!(
            cursor.next().unwrap().unwrap().event,
            Event::AcceptancePending { .. }
        ));
        assert!(matches!(
            cursor.next().unwrap().unwrap().event,
            Event::RunAccepted { .. }
        ));
        assert!(matches!(
            cursor.next().unwrap().unwrap().event,
            Event::CoreProgress { .. }
        ));
        assert!(cursor.next().unwrap().is_none());
    }

    // Test-ID: PR-TEST-0583
    // Verifies: PR-REQ-0366
    #[test]
    fn delivery_spool_reclaims_segments_and_reports_storage_and_reader_failures() {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/delivery-tests");
        fs::create_dir_all(&parent).unwrap();
        let root = tempfile::tempdir_in(parent).unwrap();
        let context = Context {
            interactive: false,
            run_id: RunId::generate().unwrap().to_string(),
            hook_ordinal: "1".into(),
            operation: "action".into(),
            action_id: Some("inspect".into()),
            revision: Revision {
                package_id: "00000000000000000000000000000001".into(),
                content_digest: format!("sha256:{}", "00".repeat(32)),
            },
            source_revision: None,
            target_revision: None,
        };
        let data = STANDARD.encode(vec![0xff; CHUNK]);
        for collision in [false, true] {
            let delivery = Delivery::start(root.path(), "invoke").unwrap();
            let session = delivery
                .storage
                .directory
                .root()
                .parent()
                .unwrap()
                .to_path_buf();
            if collision {
                fs::write(delivery.storage.segment(1), b"existing sentinel").unwrap();
            }
            for n in 0..300 {
                delivery.emit(
                    Event::Output {
                        context: context.clone(),
                        channel: Channel::Stdout,
                        offset: (n * CHUNK).to_string(),
                        encoding: "base64".into(),
                        data: data.clone(),
                        byte_length: CHUNK.to_string(),
                    },
                    true,
                );
            }
            delivery.finish();
            let summary = delivery.summary();
            if collision {
                assert!(!summary.complete);
                assert_eq!(summary.error.as_deref(), Some("storage_error"));
                assert_eq!(
                    fs::read(delivery.storage.segment(1)).unwrap(),
                    b"existing sentinel"
                );
            } else {
                assert!(summary.complete, "{summary:?}");
                let mut cursor = delivery.cursor(true);
                let mut count = 0;
                while let Some(record) = cursor.next().unwrap() {
                    count += 1;
                    assert_eq!(record.sequence, count.to_string());
                }
                assert_eq!(count, 300);
                assert!(!delivery.storage.segment(0).exists());
                assert!(delivery.storage.segment(1).exists());
                drop(cursor);
                fs::write(delivery.storage.segment(1), b"damaged\n").unwrap();
                let mut cursor = delivery.cursor(false);
                cursor.segment = 1;
                assert!(cursor.next().is_err());
                delivery.fail("delivery_read_error");
                assert!(!delivery.summary().complete);
            }
            drop(delivery);
            assert!(!session.exists());
        }
        let delivery = Delivery::start(root.path(), "invoke").unwrap();
        let scope = Scope {
            delivery: delivery.clone(),
            context,
        };
        let (_stdout, _stderr) = scope.pipes().unwrap();
        delivery.finish();
        assert_eq!(
            delivery.summary().error.as_deref(),
            Some("output_tail_unavailable")
        );
    }
}
