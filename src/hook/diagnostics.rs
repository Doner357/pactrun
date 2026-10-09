//! Bounded evidence collection is independent of process supervision and outcome.
use crate::{
    domain::{DiagnosticKind, DiagnosticWindow, HookEvidence, HookText, RunId},
    persistence::PactrunPersistence,
};
use std::{
    collections::BTreeMap,
    io::Write,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Default)]
pub(crate) struct Diagnostics {
    pub(crate) disabled: AtomicBool,
    #[cfg(test)]
    pub(crate) fail_persistence: AtomicBool,
    live: AtomicBool,
    runs: Mutex<BTreeMap<RunId, Arc<Journal>>>,
}
impl Diagnostics {
    pub(crate) fn enable_live(&self) {
        self.live.store(true, Ordering::Release);
    }
    pub(crate) fn disable_live(&self) {
        self.live.store(false, Ordering::Release);
    }
    pub(crate) fn retain(&self) -> bool {
        !self.disabled.load(Ordering::Acquire)
    }
    pub(super) fn scope(
        &self,
        root: PathBuf,
        run: RunId,
        stage: String,
        interactive: bool,
    ) -> DiagnosticScope {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        let journal = runs
            .entry(run)
            .or_insert_with(|| {
                #[cfg(test)]
                let fail = self.fail_persistence.load(Ordering::Acquire);
                #[cfg(not(test))]
                let fail = false;
                Journal::start(
                    root,
                    run,
                    self.retain(),
                    self.live.load(Ordering::Acquire),
                    fail,
                )
            })
            .clone();
        journal
            .shared
            .interactive
            .store(interactive, Ordering::Release);
        let ordinal = journal.next_hook.fetch_add(1, Ordering::Relaxed) + 1;
        DiagnosticScope {
            journal,
            stage: format!("{stage} hook_ordinal={ordinal}"),
            delivery: None,
        }
    }
    pub(crate) fn finish(&self) {
        let runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        for journal in runs.values() {
            journal.shared.close.store(true, Ordering::Release);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        for journal in runs.values() {
            journal.wait(deadline.saturating_duration_since(Instant::now()));
        }
    }
}
impl Drop for Diagnostics {
    fn drop(&mut self) {
        self.finish();
    }
}
#[derive(Debug, Default)]
struct Shared {
    window: Mutex<DiagnosticWindow>,
    close: AtomicBool,
    interactive: AtomicBool,
    failed: AtomicBool,
    terminal_generation: AtomicU64,
    writer_done: AtomicBool,
}
#[derive(Debug)]
struct Journal {
    shared: Arc<Shared>,
    done: Mutex<mpsc::Receiver<()>>,
    presentation_done: Mutex<Option<mpsc::Receiver<()>>>,
    next_hook: AtomicU64,
}
impl Journal {
    fn start(root: PathBuf, run: RunId, retain: bool, live: bool, fail: bool) -> Arc<Self> {
        let shared = Arc::new(Shared::default());
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let writer = shared.clone();
        let spawned = thread::Builder::new()
            .name("run-evidence".into())
            .spawn(move || {
                if fail {
                    writer.failed.store(true, Ordering::Release);
                    if let Ok(store) = PactrunPersistence::open(&root) {
                        let _ = store.save_diagnostics(
                            run,
                            &DiagnosticWindow::default(),
                            retain,
                            false,
                            true,
                        );
                    }
                } else {
                    collect(root, run, retain, &writer);
                }
                writer.writer_done.store(true, Ordering::Release);
                let _ = done_tx.send(());
            });
        if spawned.is_err() {
            shared.writer_done.store(true, Ordering::Release);
            shared.failed.store(true, Ordering::Release);
        }
        let presentation_done = if live {
            let output = shared.clone();
            let (tx, rx) = mpsc::sync_channel(1);
            let _ = thread::Builder::new()
                .name("hook-diagnostic-display".into())
                .spawn(move || {
                    present(run, &output);
                    let _ = tx.send(());
                });
            Some(rx)
        } else {
            None
        };
        Arc::new(Self {
            shared,
            done: Mutex::new(done_rx),
            presentation_done: Mutex::new(presentation_done),
            next_hook: AtomicU64::new(0),
        })
    }
    fn wait(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let _ = self
            .done
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .recv_timeout(timeout);
        if let Some(done) = self
            .presentation_done
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            let _ = done.recv_timeout(deadline.saturating_duration_since(Instant::now()));
        }
    }
}
#[derive(Debug)]
pub(super) struct DiagnosticScope {
    journal: Arc<Journal>,
    stage: String,
    pub(super) delivery: Option<super::delivery::Scope>,
}
impl DiagnosticScope {
    pub(super) fn record(&self, text: HookText) {
        if let Some(delivery) = &self.delivery {
            delivery.diagnostic(&text);
        }
        let text = text.bounded();
        let terminal = text.kind != DiagnosticKind::Diagnostic;
        // No I/O is performed while this short, bounded data lock is held.
        let mut window = self
            .journal
            .shared
            .window
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let sequence = window.observed.saturating_add(1);
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_millis().min(i64::MAX as u128) as u64);
        window.push(HookEvidence {
            sequence,
            received_at_unix_ms: time,
            stage: self.stage.clone(),
            text,
        });
        drop(window);
        if terminal {
            self.journal
                .shared
                .terminal_generation
                .fetch_add(1, Ordering::Release);
        }
    }
}
impl Drop for DiagnosticScope {
    fn drop(&mut self) {
        self.journal
            .shared
            .interactive
            .store(false, Ordering::Release);
    }
}
fn collect(root: PathBuf, run: RunId, retain: bool, shared: &Shared) {
    let result = (|| {
        let store = PactrunPersistence::open(&root)?;
        store.bound_diagnostic_wait()?;
        let mut last = Instant::now() - Duration::from_millis(250);
        let mut terminal = 0;
        let mut saved_observed = None;
        loop {
            let closing = shared.close.load(Ordering::Acquire);
            let current_terminal = shared.terminal_generation.load(Ordering::Acquire);
            if closing
                || last.elapsed() >= Duration::from_millis(250)
                || current_terminal != terminal
            {
                let window = shared
                    .window
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                if !closing && saved_observed == Some(window.observed) {
                    last = Instant::now();
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                let deadline = Instant::now() + Duration::from_secs(1);
                let saved = loop {
                    match store.save_diagnostics(run, &window, retain, closing, false) {
                        Err(error)
                            if error.diagnostic_contention() && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(20));
                        }
                        result => break result,
                    }
                };
                if let Err(error) = saved {
                    let _ = store.save_diagnostics(run, &window, retain, false, true);
                    return Err(error);
                }
                saved_observed = Some(window.observed);
                terminal = current_terminal;
                last = Instant::now();
            }
            if closing {
                return Ok::<_, crate::persistence::PersistenceError>(());
            }
            thread::sleep(Duration::from_millis(20));
        }
    })();
    if result.is_err() {
        shared.failed.store(true, Ordering::Release);
    }
}
fn present(run: RunId, shared: &Shared) {
    let mut output = std::io::stderr();
    let mut seen = 0;
    let mut warned = false;
    loop {
        if !warned
            && !shared.interactive.load(Ordering::Acquire)
            && shared.failed.load(Ordering::Acquire)
        {
            if writeln!(
                output,
                "Hook diagnostic persistence failed; evidence may be incomplete."
            )
            .is_err()
            {
                return;
            }
            warned = true;
        }
        if !shared.interactive.load(Ordering::Acquire) {
            let window = shared
                .window
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            for event in window
                .events()
                .into_iter()
                .filter(|e| e.sequence > seen)
                .collect::<Vec<_>>()
            {
                if event.sequence > seen + 1
                    && writeln!(
                        output,
                        "hook diagnostics: events {}..{} omitted from display",
                        seen + 1,
                        event.sequence - 1
                    )
                    .is_err()
                {
                    return;
                }
                if write_event(&mut output, run, event).is_err() {
                    return;
                }
                seen = event.sequence;
            }
        }
        if shared.close.load(Ordering::Acquire) && shared.writer_done.load(Ordering::Acquire) {
            if shared.failed.load(Ordering::Acquire) {
                let _ = writeln!(
                    output,
                    "Hook diagnostic persistence failed; evidence may be incomplete."
                );
            }
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
}
pub(crate) fn write_event(
    output: &mut dyn Write,
    run: RunId,
    event: &HookEvidence,
) -> std::io::Result<()> {
    let kind = match event.text.kind {
        DiagnosticKind::Diagnostic => "diagnostic",
        DiagnosticKind::Completion => "completion",
        DiagnosticKind::ProtocolError => "protocol_error",
    };
    let severity = match event.text.severity {
        Some(crate::domain::DiagnosticSeverity::Info) => "info",
        Some(crate::domain::DiagnosticSeverity::Warning) => "warning",
        Some(crate::domain::DiagnosticSeverity::Error) => "error",
        None => "not_applicable",
    };
    let code = event
        .text
        .code
        .as_ref()
        .map(|value| format!("{value:?}"))
        .unwrap_or_else(|| "absent".into());
    let received = event
        .received_at_unix_ms
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".into());
    writeln!(
        output,
        "hook event: run={} sequence={} received_at_unix_ms={} stage={:?} kind={} severity={} code={}",
        run, event.sequence, received, event.stage, kind, severity, code
    )?;
    if let Some(status) = event.text.completion_status {
        let status = match status {
            crate::domain::HookCompletionStatus::Success => "success",
            crate::domain::HookCompletionStatus::Failure => "failure",
        };
        writeln!(output, "hook completion: {status}")?;
    }
    if let Some(message) = &event.text.message {
        if event.text.truncated {
            let split = event.text.truncated_prefix_bytes;
            if split > message.len() || !message.is_char_boundary(split) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "invalid diagnostic truncation boundary",
                ));
            }
            writeln!(
                output,
                "hook message: {:?} [middle truncated] {:?}",
                &message[..split],
                &message[split..]
            )
        } else {
            writeln!(output, "hook message: {message:?}")
        }
    } else {
        writeln!(output, "hook message: absent")
    }
}
