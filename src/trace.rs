//! Invocation-owned phase capture. Producers touch bounded memory, never journals.
use crate::{
    domain::{PhaseResult, RunId, TraceId, TraceOperation, TracePhase, TracePolicy, TraceWindow},
    trace_journal::{Error, Journal, Location},
};
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
struct Targets {
    startup: Option<Location>,
    store: Option<Location>,
    store_root: Option<PathBuf>,
    store_policy: TracePolicy,
}
struct State {
    window: TraceWindow,
    started: Instant,
    generation: u128,
    targets: Option<Targets>,
    associated: bool,
    stopping: bool,
}
#[derive(Default)]
struct Shared {
    state: Mutex<Option<State>>,
    progress: Mutex<Progress>,
}
#[derive(Default)]
struct Progress {
    persisted: u128,
    last_error: Option<Error>,
}
struct Worker {
    wake: mpsc::SyncSender<()>,
    done: mpsc::Receiver<()>,
}

#[derive(Default)]
pub(crate) struct Trace {
    shared: Arc<Shared>,
    worker: Mutex<Option<Worker>>,
}
impl std::fmt::Debug for Trace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CoreTrace { bounded evidence }")
    }
}

#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum PersistenceStatus {
    Complete,
    Unavailable,
    Pending,
}
#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) struct Completion {
    pub(crate) id: TraceId,
    pub(crate) persistence: PersistenceStatus,
}

impl Trace {
    pub(crate) fn reader<R: Read>(self: &Arc<Self>, source: R) -> TracedReader<R> {
        TracedReader {
            source,
            trace: self.clone(),
        }
    }
    pub(crate) fn id(&self) -> Option<TraceId> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|s| s.window.id())
    }
    pub(crate) fn begin(&self) {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.is_some() {
            return;
        }
        if let Ok(id) = TraceId::generate() {
            *state = Some(State {
                window: TraceWindow::new(
                    id,
                    TraceOperation::Unknown,
                    now(),
                    TracePolicy::startup(),
                ),
                started: Instant::now(),
                generation: 1,
                targets: None,
                associated: false,
                stopping: false,
            });
        }
    }
    pub(crate) fn suppress(&self) {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }
    pub(crate) fn operation(&self, operation: TraceOperation) {
        if let Some(state) = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
            && !state.stopping
        {
            state.window.set_operation(operation);
            state.generation += 1;
        }
    }
    pub(crate) fn configure(
        &self,
        startup: Option<Location>,
        store: Option<&Path>,
        startup_policy: TracePolicy,
        store_policy: TracePolicy,
    ) {
        if !startup_policy.valid() || !store_policy.valid() {
            return;
        }
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            let Some(state) = state.as_mut() else {
                return;
            };
            if state.targets.is_some() {
                return;
            }
            state.window.set_policy(if state.associated {
                store_policy
            } else {
                startup_policy
            });
            state.targets = Some(Targets {
                startup,
                store: store.map(Location::store),
                store_root: store.map(Path::to_owned),
                store_policy,
            });
            state.generation += 1;
        }
        self.start_worker();
        self.wake();
    }
    pub(crate) fn configured(&self) -> bool {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|s| s.targets.is_some())
    }
    pub(crate) fn associate(&self, run: RunId) {
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            let Some(state) = state.as_mut() else {
                return;
            };
            if state.stopping {
                return;
            }
            state.associated = true;
            state.window.associate(run);
            if let Some(targets) = &state.targets {
                state.window.set_policy(targets.store_policy);
            }
            state.generation += 1;
        }
        self.wake();
    }
    pub(crate) fn span(self: &Arc<Self>, phase: TracePhase) -> Span {
        let started = Instant::now();
        let span = {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            state.as_mut().filter(|s| !s.stopping).and_then(|s| {
                s.generation += 1;
                s.window
                    .begin(phase, micros(started.saturating_duration_since(s.started)))
            })
        };
        self.wake();
        Span {
            trace: self.clone(),
            phase,
            span,
            started,
            finished: false,
        }
    }
    pub(crate) fn measure<T, E>(
        self: &Arc<Self>,
        phase: TracePhase,
        f: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        let span = self.span(phase);
        let result = f();
        span.finish(if result.is_ok() {
            PhaseResult::Succeeded
        } else {
            PhaseResult::Failed
        });
        result
    }
    fn end(&self, span: u64, phase: TracePhase, started: Instant, result: PhaseResult) {
        let at = Instant::now();
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(s) = state.as_mut().filter(|s| !s.stopping) {
                s.window.end(
                    span,
                    phase,
                    micros(at.saturating_duration_since(s.started)),
                    micros(at.saturating_duration_since(started)),
                    result,
                );
                s.generation += 1;
            }
        }
        self.wake();
    }
    fn wake(&self) {
        if let Some(w) = self
            .worker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            let _ = w.wake.try_send(());
        }
    }
    fn start_worker(&self) {
        let mut worker = self.worker.lock().unwrap_or_else(|e| e.into_inner());
        if worker.is_some() {
            return;
        }
        let (wake, receiver) = mpsc::sync_channel(1);
        let (done_tx, done) = mpsc::sync_channel(1);
        let shared = self.shared.clone();
        if thread::Builder::new()
            .name("core-trace-writer".into())
            .spawn(move || {
                write_loop(shared, receiver);
                let _ = done_tx.send(());
            })
            .is_ok()
        {
            *worker = Some(Worker { wake, done });
        } else {
            self.shared
                .progress
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .last_error = Some(Error::Io);
        }
    }
    pub(crate) fn finish(&self, code: i32) -> Option<Completion> {
        let (id, generation) = {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            let state = state.as_mut()?;
            if !state.stopping {
                state.window.close(code);
                state.stopping = true;
                state.generation += 1;
            }
            (state.window.snapshot().id, state.generation)
        };
        let worker = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(worker) = worker {
            let _ = worker.wake.try_send(());
            let _ = worker.done.recv_timeout(Duration::from_secs(1));
        }
        let progress = self
            .shared
            .progress
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let persistence = if progress.persisted >= generation {
            PersistenceStatus::Complete
        } else if progress.last_error.is_some() {
            PersistenceStatus::Unavailable
        } else {
            PersistenceStatus::Pending
        };
        Some(Completion { id, persistence })
    }
}

/// Measures read attempts only. It never copies, examines or retains the bytes.
pub(crate) struct TracedReader<R> {
    source: R,
    trace: Arc<Trace>,
}
impl<R: Read> Read for TracedReader<R> {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        self.trace
            .measure(TracePhase::InputRead, || self.source.read(destination))
    }
    fn read_vectored(&mut self, destinations: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        self.trace.measure(TracePhase::InputRead, || {
            self.source.read_vectored(destinations)
        })
    }
}
impl Drop for Trace {
    fn drop(&mut self) {
        if let Some(s) = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            s.stopping = true;
        }
        self.wake();
    }
}

pub(crate) struct Span {
    trace: Arc<Trace>,
    phase: TracePhase,
    span: Option<u64>,
    started: Instant,
    finished: bool,
}
impl Span {
    pub(crate) fn finish(mut self, result: PhaseResult) {
        if let Some(span) = self.span {
            self.trace.end(span, self.phase, self.started, result);
        }
        self.finished = true;
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        if !self.finished
            && let Some(span) = self.span
        {
            self.trace
                .end(span, self.phase, self.started, PhaseResult::Unfinished);
        }
    }
}

enum Destination {
    Startup(Journal),
    Store(Box<crate::diagnostic_retention::StoreWriter>),
}
impl Destination {
    fn save(&mut self, document: &crate::domain::TraceDocument, now: u64) -> Result<(), Error> {
        match self {
            Self::Startup(journal) => {
                let mut document = document.clone();
                document.retention = crate::domain::TraceRetention::Complete;
                journal.save(&document, now)
            }
            Self::Store(writer) => writer.save_trace(document, now),
        }
    }
}
fn write_loop(shared: Arc<Shared>, receiver: mpsc::Receiver<()>) {
    let mut active: Option<(Location, Destination)> = None;
    let mut attempted = 0;
    let mut last_write = Instant::now() - Duration::from_millis(250);
    loop {
        let _ = receiver.recv_timeout(Duration::from_millis(250));
        // Coalesce producer wakeups; writing each event would amplify filesystem work.
        if let Some(delay) = Duration::from_millis(250).checked_sub(last_write.elapsed()) {
            thread::sleep(delay);
        }
        let data = {
            let state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            let Some(s) = state.as_ref() else {
                return;
            };
            let location = s.targets.as_ref().and_then(|t| {
                if s.associated {
                    t.store.clone()
                } else {
                    t.startup.clone()
                }
            });
            let forbidden = if s.associated {
                None
            } else {
                s.targets.as_ref().and_then(|t| t.store_root.clone())
            };
            let store_root = if s.associated {
                s.targets.as_ref().and_then(|t| t.store_root.clone())
            } else {
                None
            };
            (
                s.window.snapshot(),
                location,
                forbidden,
                store_root,
                s.generation,
                s.stopping,
            )
        };
        let (document, location, forbidden, store_root, generation, stopping) = data;
        if generation != attempted || stopping {
            attempted = generation;
            let result = (|| {
                let location = location.ok_or(Error::Missing)?;
                if forbidden
                    .as_ref()
                    .is_some_and(|root| location.overlaps_store(root))
                {
                    return Err(Error::UnsafePath);
                }
                if active.as_ref().is_none_or(|(old, _)| old != &location) {
                    let destination = if let Some(root) = &store_root {
                        Destination::Store(Box::new(
                            crate::diagnostic_retention::StoreWriter::open(root)?,
                        ))
                    } else {
                        Destination::Startup(Journal::open(&location)?)
                    };
                    active = Some((location.clone(), destination));
                }
                active
                    .as_mut()
                    .expect("opened journal")
                    .1
                    .save(&document, now().ok_or(Error::Io)?)
            })();
            last_write = Instant::now();
            if result.is_err() {
                active = None;
            }
            let mut progress = shared.progress.lock().unwrap_or_else(|e| e.into_inner());
            match result {
                Ok(()) => {
                    progress.persisted = generation;
                    progress.last_error = None;
                }
                Err(e) => {
                    progress.last_error = Some(e);
                }
            }
        }
        if stopping {
            return;
        }
    }
}
fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}
fn now() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_millis()).ok())
}

pub(crate) fn startup_location(
    executable: Option<&Path>,
    lookup: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Option<Location> {
    let entry = if executable
        .and_then(Path::file_stem)
        .is_some_and(|stem| stem == "pactrun-test")
    {
        "pactrun-test-diagnostics"
    } else {
        "pactrun-diagnostics"
    };
    #[cfg(windows)]
    let base = lookup("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = lookup("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| lookup("HOME").map(|h| PathBuf::from(h).join(".local/state")));
    base.filter(|p| p.is_absolute())
        .map(|p| Location::startup(p.join(entry).join("diagnostics.sqlite3")))
}

#[cfg(test)]
mod tests;
