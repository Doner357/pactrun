//! Explicit helper CLI; file copying never passes payload bytes through IPC.
use super::*;
use crate::domain::{HelperCommand, HelperFailure, HelperReason, HelperStage};

#[derive(Debug)]
struct LocalProblem(HelperStage, HelperReason);
impl std::fmt::Display for LocalProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.0.text(), self.1.text())
    }
}
impl std::error::Error for LocalProblem {}
fn problem(stage: HelperStage, reason: HelperReason) -> io::Error {
    io::Error::other(LocalProblem(stage, reason))
}
fn io_reason(error: &io::Error) -> HelperReason {
    match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => HelperReason::NotFound,
        io::ErrorKind::PermissionDenied => HelperReason::PermissionDenied,
        io::ErrorKind::AlreadyExists => HelperReason::AlreadyExists,
        _ => HelperReason::Io,
    }
}
fn at(stage: HelperStage, error: io::Error) -> io::Error {
    problem(stage, io_reason(&error))
}
enum HelperError {
    Failure(HelperFailure),
    TerminalProtocolError,
}
fn classified(
    command: HelperCommand,
    error: io::Error,
    stage: HelperStage,
    reason: HelperReason,
) -> HelperError {
    let (stage, reason) = error
        .get_ref()
        .and_then(|e| e.downcast_ref::<LocalProblem>())
        .map_or((stage, reason), |e| (e.0, e.1));
    HelperError::Failure(HelperFailure {
        command,
        stage,
        reason,
    })
}

fn json_file(path: &Path) -> io::Result<Value> {
    let mut bytes = vec![];
    fs::File::open(path)
        .map_err(|e| at(HelperStage::ReadJson, e))?
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| at(HelperStage::ReadJson, e))?;
    if bytes.len() > LIMIT {
        return Err(problem(HelperStage::ParseJson, HelperReason::TooLarge));
    }
    strict_json::parse_json(&bytes, LIMIT)
        .map_err(|_| problem(HelperStage::ParseJson, HelperReason::InvalidJson))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| problem(HelperStage::ParseJson, HelperReason::InvalidJson))
}

struct Call {
    request: Request,
    output: Option<OsString>,
    copy_from: Option<OsString>,
    copy_to: Option<OsString>,
    text: bool,
}

fn parse(args: &[OsString]) -> io::Result<Call> {
    let verb = args.first().and_then(|s| s.to_str()).ok_or_else(failure)?;
    let mut position = vec![];
    let mut options = std::collections::BTreeMap::new();
    let mut index = 1;
    while index < args.len() {
        let argument = args[index].to_str().ok_or_else(failure)?;
        if argument.starts_with("--") {
            if !matches!(
                argument,
                "--output" | "--file" | "--view" | "--role" | "--copy-to"
            ) {
                return Err(failure());
            }
            index += 1;
            let value = args.get(index).ok_or_else(failure)?.clone();
            if options.insert(argument, value).is_some() {
                return Err(failure());
            }
        } else {
            position.push(argument.to_owned());
        }
        index += 1;
    }
    let mut output = options.remove("--output");
    let mut copy_from = None;
    let mut copy_to = None;
    let mut text = false;
    let id = |position: &[String]| -> io::Result<String> {
        if position.len() == 1 {
            Ok(position[0].clone())
        } else {
            Err(failure())
        }
    };
    let request = match verb {
        "session" if position.is_empty() => Request::Session,
        "parameter" => Request::Parameter { id: id(&position)? },
        "workspace" if position.is_empty() => {
            text = true;
            Request::Workspace
        }
        "input" => {
            let view = options
                .remove("--view")
                .unwrap_or_else(|| "current".into())
                .into_string()
                .map_err(|_| failure())?;
            let role = options
                .remove("--role")
                .unwrap_or_else(|| "active".into())
                .into_string()
                .map_err(|_| failure())?;
            copy_to = options.remove("--copy-to");
            text = true;
            Request::Input {
                id: id(&position)?,
                view,
                role,
            }
        }
        "resource" => {
            text = true;
            Request::Resource {
                handle: id(&position)?,
            }
        }
        "output" => {
            text = true;
            copy_from = options.remove("--file");
            Request::Output { id: id(&position)? }
        }
        "output-register" => Request::OutputRegister { id: id(&position)? },
        "candidate" if position.is_empty() => {
            text = true;
            Request::Candidate
        }
        "snapshot-content" if position.is_empty() => Request::SnapshotContent,
        "risk" => {
            let action = id(&position)?;
            if !matches!(action.as_str(), "enter" | "resolve") {
                return Err(failure());
            }
            text = true;
            Request::Risk {
                enter: action == "enter",
            }
        }
        "target-ready" if position.is_empty() => Request::TargetReady,
        "diagnostic" | "protocol-error" | "capture-register" | "completion"
            if position.is_empty() =>
        {
            let file = options.remove("--file").ok_or_else(failure)?;
            let value = json_file(Path::new(&file))?;
            match verb {
                "diagnostic" => Request::Diagnostic { value },
                "protocol-error" => Request::ProtocolError { value },
                "capture-register" => Request::CaptureRegister { value },
                _ => Request::Completion { value },
            }
        }
        _ => return Err(failure()),
    };
    if !options.is_empty() {
        return Err(failure());
    }
    if copy_to.is_some() || copy_from.is_some() {
        if output.is_some() {
            return Err(failure());
        }
        output = None;
    }
    Ok(Call {
        request,
        output,
        copy_from,
        copy_to,
        text,
    })
}

fn connection() -> io::Result<ProtocolStream> {
    let endpoint = env::var(HELPER_ENDPOINT).map_err(|_| failure())?;
    loop {
        match connect(&endpoint) {
            Ok(stream) => return Ok(stream),
            // Busy connection admission is mechanical, not a request replay.
            // The execution owner controls deadlines and the helper process
            // tree; do not impose a second deadline on a healthy busy Session.
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
}
fn execute(args: &[OsString]) -> Result<(), HelperError> {
    let command = HelperCommand::parse(args.first().and_then(|v| v.to_str()).unwrap_or_default());
    let call = parse(args).map_err(|e| {
        classified(
            command,
            e,
            HelperStage::Arguments,
            HelperReason::InvalidArguments,
        )
    })?;
    let mut stream = connection()
        .map_err(|e| classified(command, e, HelperStage::Connect, HelperReason::Unavailable))?;
    write_helper_frame(
        &mut stream,
        &serde_json::to_value(call.request).expect("typed helper request"),
    )
    .map_err(|e| classified(command, e, HelperStage::Submit, HelperReason::Io))?;
    let reply = read_helper_frame(&mut stream).map_err(|e| {
        if command == HelperCommand::ProtocolError
            && matches!(
                e.kind(),
                io::ErrorKind::UnexpectedEof
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::NotConnected
            )
        {
            return HelperError::TerminalProtocolError;
        }
        classified(command, e, HelperStage::Reply, HelperReason::InvalidReply)
    })?;
    let ok = reply["ok"].as_bool().ok_or_else(|| {
        classified(
            command,
            failure(),
            HelperStage::Reply,
            HelperReason::InvalidReply,
        )
    })?;
    let delivery = (|| -> io::Result<()> {
        if ok {
            let value = reply
                .get("value")
                .ok_or_else(|| problem(HelperStage::Reply, HelperReason::InvalidReply))?;
            if let Some(source) = call.copy_from {
                fs::copy(source, value.as_str().ok_or_else(failure)?)
                    .map_err(|e| at(HelperStage::CopyData, e))?;
            } else if let Some(destination) = call.copy_to {
                let source = fs::File::open(value.as_str().ok_or_else(failure)?)
                    .map_err(|e| at(HelperStage::ReadInput, e))?;
                let mut out = create_output(Path::new(&destination))?;
                io::copy(&mut io::BufReader::new(source), &mut out)
                    .map_err(|e| at(HelperStage::CopyData, e))?;
                out.flush()?;
            } else if !value.is_null() {
                let bytes = if call.text {
                    value.as_str().ok_or_else(failure)?.as_bytes().to_vec()
                } else {
                    serde_json::to_vec(value).map_err(|_| failure())?
                };
                if let Some(output) = call.output {
                    let mut file = create_output(Path::new(&output))?;
                    file.write_all(&bytes)?;
                    file.flush()?;
                } else {
                    let mut out = io::stdout().lock();
                    out.write_all(&bytes)?;
                    out.write_all(b"\n")?;
                    out.flush()?;
                }
            }
        }
        Ok(())
    })();
    write_helper_frame(&mut stream, &json!({"received":true}))
        .map_err(|e| classified(command, e, HelperStage::Submit, HelperReason::Io))?;
    delivery.map_err(|e| classified(command, e, HelperStage::WriteOutput, HelperReason::Io))?;
    if !ok {
        return Err(classified(
            command,
            failure(),
            HelperStage::Request,
            reply
                .get("reason")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or(HelperReason::InvalidRequest),
        ));
    }
    Ok(())
}

fn create_output(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|e| at(HelperStage::WriteOutput, e))
}

pub(super) fn run(args: &[OsString]) -> i32 {
    match execute(args) {
        Ok(()) => 0,
        Err(HelperError::TerminalProtocolError) => 1,
        Err(HelperError::Failure(failure)) => {
            if !crate::hook::core_diagnostics::publish(&failure) {
                eprintln!("error: Pactrun {failure}");
            }
            1
        }
    }
}
