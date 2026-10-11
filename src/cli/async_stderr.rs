//! Production stderr cannot hold execution ownership hostage to a blocked pipe.
use std::{
    io::{self, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
pub(super) struct AsyncStderr {
    sender: Option<mpsc::SyncSender<Vec<u8>>>,
    failed: Arc<AtomicBool>,
    done: mpsc::Receiver<()>,
    pending: Vec<u8>,
    finished: bool,
}
impl AsyncStderr {
    pub(super) fn new() -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(64);
        let (done_tx, done) = mpsc::sync_channel(1);
        let failed = Arc::new(AtomicBool::new(false));
        let flag = failed.clone();
        let started = thread::Builder::new()
            .name("cli-stderr".into())
            .spawn(move || {
                let mut destination = io::stderr();
                for bytes in receiver {
                    if destination.write_all(&bytes).is_err() {
                        flag.store(true, Ordering::Release);
                        break;
                    }
                }
                let _ = done_tx.send(());
            });
        if started.is_err() {
            failed.store(true, Ordering::Release);
        }
        Self {
            sender: Some(sender),
            failed,
            done,
            pending: Vec::with_capacity(4096),
            finished: false,
        }
    }
    pub(super) fn finish(&mut self) -> io::Result<()> {
        if !self.finished {
            if self.flush().is_err() {
                self.failed.store(true, Ordering::Release);
            }
            self.sender.take();
            if self.done.recv_timeout(Duration::from_secs(2)).is_err() {
                self.failed.store(true, Ordering::Release);
            }
            self.finished = true;
        }
        if self.failed.load(Ordering::Acquire) {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "stderr delivery incomplete",
            ))
        } else {
            Ok(())
        }
    }
}
impl Write for AsyncStderr {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.finished || self.failed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "stderr unavailable",
            ));
        }
        if self.pending.len() == 4096 {
            self.flush()?;
        }
        let accepted = bytes.len().min(4096 - self.pending.len());
        self.pending.extend_from_slice(&bytes[..accepted]);
        Ok(accepted)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let bytes = std::mem::take(&mut self.pending);
        match self.sender.as_ref().expect("writer active").try_send(bytes) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(bytes)) => {
                self.pending = bytes;
                Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "stderr backlog exhausted",
                ))
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.failed.store(true, Ordering::Release);
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "stderr unavailable",
                ))
            }
        }
    }
}
impl Drop for AsyncStderr {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0681
    // Verifies: PR-REQ-0375
    #[test]
    fn small_fragments_share_the_byte_budget_and_final_delivery_failure_is_observable() {
        let (sender, receiver) = mpsc::sync_channel(64);
        let (done_tx, done) = mpsc::sync_channel(1);
        let mut output = AsyncStderr {
            sender: Some(sender),
            failed: Arc::new(AtomicBool::new(false)),
            done,
            pending: Vec::new(),
            finished: false,
        };
        let expected = "small result field\n".repeat(150);
        for byte in expected.as_bytes() {
            output.write_all(&[*byte]).unwrap();
        }
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        output.flush().unwrap();
        assert_eq!(receiver.try_recv().unwrap(), expected.as_bytes());
        // The consumer can fail after queue admission but before finalization.
        output.failed.store(true, Ordering::Release);
        done_tx.send(()).unwrap();
        assert!(output.finish().is_err());
        assert!(output.finish().is_err());
    }
}
