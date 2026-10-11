//! Safe Pactrun-owned diagnostic facts, separate from Hook-authored evidence.
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

macro_rules! vocabulary {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[cfg_attr(test, derive(schemars::JsonSchema))]
        #[serde(rename_all = "snake_case")]
        pub(crate) enum $name { $($variant),+ }
        impl $name {
            pub(crate) fn text(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }
    };
}
vocabulary!(HelperCommand {
    Unknown => "command", Session => "session", Parameter => "parameter",
    Workspace => "workspace", Input => "input", Resource => "resource",
    Output => "output", OutputRegister => "output-register", Candidate => "candidate",
    SnapshotContent => "snapshot-content", Diagnostic => "diagnostic",
    ProtocolError => "protocol-error", Risk => "risk", CaptureRegister => "capture-register",
    Completion => "completion", TargetReady => "target-ready"
});
impl HelperCommand {
    pub(crate) fn parse(value: &str) -> Self {
        [
            Self::Session,
            Self::Parameter,
            Self::Workspace,
            Self::Input,
            Self::Resource,
            Self::Output,
            Self::OutputRegister,
            Self::Candidate,
            Self::SnapshotContent,
            Self::Diagnostic,
            Self::ProtocolError,
            Self::Risk,
            Self::CaptureRegister,
            Self::Completion,
            Self::TargetReady,
        ]
        .into_iter()
        .find(|c| c.text() == value)
        .unwrap_or(Self::Unknown)
    }
}
vocabulary!(HelperStage {
    Arguments => "arguments", ReadJson => "read JSON file", ParseJson => "parse JSON file",
    Connect => "connect to script session", Submit => "send request", Reply => "receive reply",
    ReadInput => "read source", CopyData => "copy data", WriteOutput => "write output", Request => "validate request"
});
vocabulary!(HelperReason {
    InvalidArguments => "invalid command arguments", NotFound => "file or location not found",
    PermissionDenied => "permission denied", AlreadyExists => "destination already exists",
    InvalidJson => "invalid JSON", TooLarge => "JSON exceeds the size limit",
    Unavailable => "session unavailable", InvalidRequest => "request is not valid for this session",
    InvalidReply => "invalid session reply", ParameterUnavailable => "parameter is not available in this session",
    InvalidMessage => "message does not match the protocol schema", Io => "I/O operation failed"
});
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct HelperFailure {
    pub(crate) command: HelperCommand,
    pub(crate) stage: HelperStage,
    pub(crate) reason: HelperReason,
}
impl std::fmt::Display for HelperFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "helper {}: {}: {}",
            self.command.text(),
            self.stage.text(),
            self.reason.text()
        )
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CoreEvidence {
    pub(crate) sequence: u64,
    pub(crate) received_at_unix_ms: Option<u64>,
    pub(crate) stage: String,
    pub(crate) failure: HelperFailure,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CoreDiagnosticInspection {
    pub(crate) started: bool,
    pub(crate) observed: u64,
    pub(crate) collection_closed: bool,
    pub(crate) persistence_failed: bool,
    pub(crate) events: Vec<CoreEvidence>,
}
vocabulary!(CoreCollectionIssue {
    Transport => "diagnostic channel did not finish",
    Capacity => "diagnostic channel capacity was exceeded",
    Storage => "diagnostic storage failed"
});
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum CoreDiagnostic {
    HelperFailure { failure: HelperFailure },
    CollectionIncomplete { reason: CoreCollectionIssue },
}
#[derive(Clone, Debug, Default)]
pub(crate) struct CoreDiagnosticWindow {
    pub(crate) observed: u64,
    pub(crate) incomplete: bool,
    prefix: Vec<CoreEvidence>,
    suffix: VecDeque<CoreEvidence>,
}
impl CoreDiagnosticWindow {
    pub(crate) fn push(&mut self, mut event: CoreEvidence) {
        self.observed = self.observed.saturating_add(1);
        event.sequence = self.observed;
        if self.prefix.len() < 1024 {
            self.prefix.push(event);
        } else {
            self.suffix.push_back(event);
            if self.suffix.len() > 3072 {
                self.suffix.pop_front();
            }
        }
    }
    pub(crate) fn events(&self) -> impl Iterator<Item = &CoreEvidence> {
        self.prefix.iter().chain(self.suffix.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0689
    // Verifies: PR-REQ-0378, PR-REQ-0352
    #[test]
    fn core_diagnostics_keep_bounded_prefix_and_recent_safe_facts() {
        let failure = HelperFailure {
            command: HelperCommand::Diagnostic,
            stage: HelperStage::ParseJson,
            reason: HelperReason::InvalidJson,
        };
        let mut window = CoreDiagnosticWindow::default();
        for _ in 0..6000 {
            window.push(CoreEvidence {
                sequence: 0,
                received_at_unix_ms: None,
                stage: "Action".into(),
                failure,
            });
        }
        assert_eq!(window.observed, 6000);
        let events: Vec<_> = window.events().collect();
        assert_eq!(events.len(), 4096);
        assert_eq!(events[0].sequence, 1);
        assert_eq!(events[1023].sequence, 1024);
        assert_eq!(events[1024].sequence, 2929);
        assert_eq!(events.last().unwrap().sequence, 6000);
        assert!(!window.incomplete);
        let mut value = serde_json::to_value(failure).unwrap();
        value["path"] = "private-path".into();
        assert!(serde_json::from_value::<HelperFailure>(value).is_err());
    }
}
