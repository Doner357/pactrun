//! Non-authoritative, execution-scoped helper evidence, separate from Hook RPC.
use super::{
    diagnostics::CoreSink,
    platform::{ProtocolListener, ProtocolStream},
};
use crate::domain::{CoreCollectionIssue, HelperFailure};
use std::{
    io::{self, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

pub(super) const ENDPOINT_ENV: &str = "PACTRUN_INTERNAL_CORE_DIAGNOSTICS";
const LIMIT: usize = 1024;
const CLIENT_LIMIT: usize = 64;
const REPORT_BUDGET: Duration = Duration::from_millis(250);

fn nonblocking(stream: &ProtocolStream) -> io::Result<()> {
    #[cfg(unix)]
    {
        stream.set_nonblocking(true)
    }
    #[cfg(windows)]
    {
        let _ = stream;
        Ok(())
    }
}
fn available(stream: &mut ProtocolStream, bytes: &mut [u8]) -> io::Result<usize> {
    #[cfg(unix)]
    {
        std::io::Read::read(stream, bytes)
    }
    #[cfg(windows)]
    {
        stream.try_read(bytes)
    }
}
fn packet(failure: &HelperFailure) -> io::Result<Vec<u8>> {
    let data = serde_json::to_vec(failure).map_err(io::Error::other)?;
    if data.len() > LIMIT {
        return Err(io::Error::other("diagnostic frame exceeds limit"));
    }
    let mut bytes = (data.len() as u32).to_be_bytes().to_vec();
    bytes.extend(data);
    Ok(bytes)
}
fn decode(bytes: &[u8]) -> io::Result<Option<HelperFailure>> {
    if bytes.len() < 4 {
        return Ok(None);
    }
    let length = u32::from_be_bytes(bytes[..4].try_into().expect("four-byte header")) as usize;
    if length > LIMIT {
        return Err(io::Error::other("diagnostic frame exceeds limit"));
    }
    if bytes.len() < length + 4 {
        return Ok(None);
    }
    if bytes.len() != length + 4 {
        return Err(io::Error::other("extra diagnostic frame bytes"));
    }
    serde_json::from_slice(&bytes[4..4 + length])
        .map(Some)
        .map_err(io::Error::other)
}

/// Reporting is bounded and cannot issue or retry an authoritative helper request.
pub(super) fn publish(failure: &HelperFailure) -> bool {
    let failure = *failure;
    let (send, receive) = mpsc::sync_channel(1);
    if thread::Builder::new()
        .name("helper-diagnostic".into())
        .spawn(move || {
            let _ = send.send(transmit(&failure));
        })
        .is_err()
    {
        return false;
    }
    receive.recv_timeout(REPORT_BUDGET).unwrap_or(false)
}
fn transmit(failure: &HelperFailure) -> bool {
    let attempt = (|| {
        let endpoint = std::env::var(ENDPOINT_ENV).map_err(io::Error::other)?;
        if endpoint.is_empty()
            || std::env::var("PACTRUN_SHELL_HELPER_ENDPOINT")
                .ok()
                .as_deref()
                == Some(&endpoint)
        {
            return Err(io::Error::other("diagnostic endpoint unavailable"));
        }
        let bytes = packet(failure)?;
        let deadline = Instant::now() + REPORT_BUDGET;
        let mut stream = loop {
            match super::shell_loader::connect(&endpoint) {
                Ok(stream) => break stream,
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(1))
                }
                Err(error) => return Err(error),
            }
        };
        nonblocking(&stream)?;
        let mut written = 0;
        while written < bytes.len() {
            match stream.write(&bytes[written..]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => written += n,
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(1))
                }
                Err(error) => return Err(error),
            }
        }
        let mut receipt = [0];
        loop {
            match available(&mut stream, &mut receipt) {
                Ok(1) if receipt[0] == 1 => return Ok(()),
                Ok(_) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(1))
                }
                Err(error) => return Err(error),
            }
        }
    })();
    attempt.is_ok()
}

struct Client {
    stream: ProtocolStream,
    bytes: Vec<u8>,
    recorded: bool,
}
impl Client {
    fn new(stream: ProtocolStream) -> io::Result<Self> {
        nonblocking(&stream)?;
        Ok(Self {
            stream,
            bytes: Vec::with_capacity(LIMIT + 4),
            recorded: false,
        })
    }
    fn poll(&mut self, sink: &CoreSink) -> io::Result<bool> {
        if self.recorded {
            return match available(&mut self.stream, &mut [0]) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(false),
                Ok(n) if n > 0 => Err(io::Error::other("extra diagnostic frame")),
                _ => Ok(true),
            };
        }
        let mut buffer = [0; LIMIT + 4];
        let remaining = buffer.len() - self.bytes.len();
        match available(&mut self.stream, &mut buffer[..remaining]) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => self.bytes.extend_from_slice(&buffer[..n]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) => return Err(error),
        }
        if let Some(failure) = decode(&self.bytes)? {
            sink.record(failure);
            // Receipt confirms observation, not diagnostic persistence or Run success.
            self.recorded = true;
            return Ok(self.stream.write_all(&[1]).is_err());
        }
        Ok(false)
    }
}

pub(super) struct Bridge {
    launched: bool,
    endpoint: String,
    stop: Arc<AtomicBool>,
    done: mpsc::Receiver<()>,
    sink: CoreSink,
}
impl Bridge {
    pub(super) fn start(sink: CoreSink) -> io::Result<Self> {
        let mut listener = ProtocolListener::bind_helpers()?;
        let endpoint = listener.endpoint().to_owned();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let receiver = sink.clone();
        let (sent, done) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("core-diagnostic-receiver".into())
            .spawn(move || {
                let result = (|| {
                    let mut clients: Vec<Client> = Vec::new();
                    let mut closing = None;
                    loop {
                        if stopped.load(Ordering::Acquire) {
                            closing.get_or_insert_with(Instant::now);
                        }
                        let mut accepted = false;
                        while let Some(stream) = listener.try_accept()? {
                            accepted = true;
                            if clients.len() == CLIENT_LIMIT {
                                receiver.incomplete_for(CoreCollectionIssue::Capacity);
                                drop(stream);
                                break;
                            }
                            clients.push(Client::new(stream)?);
                        }
                        clients.retain_mut(|client| {
                            if closing.is_some() && client.recorded {
                                false
                            } else {
                                match client.poll(&receiver) {
                                    Ok(done) => !done,
                                    Err(_) => {
                                        receiver.incomplete();
                                        false
                                    }
                                }
                            }
                        });
                        if let Some(since) = closing {
                            if clients.is_empty() && !accepted {
                                return Ok::<_, io::Error>(());
                            }
                            if since.elapsed() >= REPORT_BUDGET {
                                receiver.incomplete();
                                return Ok(());
                            }
                        }
                        thread::sleep(Duration::from_millis(1));
                    }
                })();
                if result.is_err() {
                    receiver.incomplete();
                }
                let _ = sent.send(());
            })?;
        Ok(Self {
            launched: false,
            endpoint,
            stop,
            done,
            sink,
        })
    }
    pub(super) fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub(super) fn mark_launched(&mut self) {
        self.launched = true;
        self.sink.start();
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if self
            .done
            .recv_timeout(REPORT_BUDGET + Duration::from_millis(100))
            .is_err()
            && self.launched
        {
            self.sink.incomplete();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0693
    // Verifies: PR-REQ-0378
    #[test]
    fn private_core_channel_rejects_unbounded_and_untyped_payloads() {
        assert!(decode(&((LIMIT + 1) as u32).to_be_bytes()).is_err());
        for bytes in [br#"{"command":"diagnostic","stage":"parse_json","reason":"invalid_json","message":"private"}"#.as_slice(), b"not-json"] {
            let mut data = (bytes.len() as u32).to_be_bytes().to_vec(); data.extend_from_slice(bytes);
            assert!(decode(&data).is_err());
        }
        let failure = HelperFailure {
            command: crate::domain::HelperCommand::Diagnostic,
            stage: crate::domain::HelperStage::ParseJson,
            reason: crate::domain::HelperReason::InvalidJson,
        };
        let data = packet(&failure).unwrap();
        for count in 0..data.len() {
            assert_eq!(decode(&data[..count]).unwrap(), None);
        }
        assert_eq!(decode(&data).unwrap(), Some(failure));
        let mut trailing = data;
        trailing.push(0);
        assert!(decode(&trailing).is_err());
    }
}
