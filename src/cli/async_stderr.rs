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
        }
    }
}
impl Write for AsyncStderr {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "stderr unavailable",
            ));
        }
        for chunk in bytes.chunks(4096) {
            match self
                .sender
                .as_ref()
                .expect("writer active")
                .try_send(chunk.to_vec())
            {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(_)) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "stderr backlog exhausted",
                    ));
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "stderr unavailable",
                    ));
                }
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Drop for AsyncStderr {
    fn drop(&mut self) {
        self.sender.take();
        let _ = self.done.recv_timeout(Duration::from_secs(2));
    }
}
