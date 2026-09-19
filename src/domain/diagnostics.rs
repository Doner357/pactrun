//! Bounded, attributed Hook evidence; never execution-outcome authority.
use std::{collections::VecDeque, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticKind {
    Diagnostic,
    Completion,
    ProtocolError,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct HookText {
    pub(crate) kind: DiagnosticKind,
    pub(crate) severity: Option<DiagnosticSeverity>,
    pub(crate) completion_status: Option<super::HookCompletionStatus>,
    pub(crate) code: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) truncated: bool,
    pub(crate) truncated_prefix_bytes: usize,
}
impl std::fmt::Debug for HookText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookText")
            .field("kind", &self.kind)
            .field("severity", &self.severity)
            .field("truncated", &self.truncated)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DiagnosticInspection {
    pub(crate) retain_text: bool,
    pub(crate) started: bool,
    pub(crate) closed: bool,
    pub(crate) observed: u64,
    pub(crate) failed: bool,
    pub(crate) events: Vec<HookEvidence>,
}
impl HookText {
    pub(crate) fn bounded(mut self) -> Self {
        if let Some(message) = &mut self.message
            && message.len() > 64 * 1024
        {
            let mut first = 32 * 1024;
            while !message.is_char_boundary(first) {
                first -= 1;
            }
            let mut last = message.len() - 32 * 1024;
            while !message.is_char_boundary(last) {
                last += 1;
            }
            *message = format!("{}{}", &message[..first], &message[last..]);
            self.truncated = true;
            self.truncated_prefix_bytes = first;
        }
        self
    }
    pub(crate) fn bytes(&self) -> usize {
        self.code.as_ref().map_or(0, String::len) + self.message.as_ref().map_or(0, String::len)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HookEvidence {
    pub(crate) sequence: u64,
    pub(crate) received_at_unix_ms: Option<u64>,
    pub(crate) stage: String,
    pub(crate) text: HookText,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct DiagnosticWindow {
    pub(crate) observed: u64,
    pub(crate) prefix_sealed: bool,
    pub(crate) prefix: VecDeque<Arc<HookEvidence>>,
    pub(crate) suffix: VecDeque<Arc<HookEvidence>>,
    pub(crate) terminal: VecDeque<Arc<HookEvidence>>,
    prefix_bytes: usize,
    suffix_bytes: usize,
    terminal_bytes: usize,
}
impl DiagnosticWindow {
    pub(crate) fn push(&mut self, mut event: HookEvidence) {
        self.observed = self.observed.max(event.sequence);
        event.text = event.text.bounded();
        let bytes = event.text.bytes();
        if event.text.kind != DiagnosticKind::Diagnostic {
            Self::push_tail(
                &mut self.terminal,
                &mut self.terminal_bytes,
                event,
                1024 * 1024,
                256,
            );
        } else if !self.prefix_sealed
            && self.prefix.len() < 1024
            && self.prefix_bytes + bytes <= 1024 * 1024
        {
            self.prefix_bytes += bytes;
            self.prefix.push_back(Arc::new(event));
        } else {
            self.prefix_sealed = true;
            Self::push_tail(
                &mut self.suffix,
                &mut self.suffix_bytes,
                event,
                3 * 1024 * 1024,
                3072,
            );
        }
    }
    fn push_tail(
        queue: &mut VecDeque<Arc<HookEvidence>>,
        bytes: &mut usize,
        event: HookEvidence,
        limit: usize,
        count: usize,
    ) {
        if event.text.bytes() > limit {
            return;
        }
        *bytes += event.text.bytes();
        queue.push_back(Arc::new(event));
        while *bytes > limit || queue.len() > count {
            *bytes -= queue
                .pop_front()
                .expect("nonempty over-budget window")
                .text
                .bytes();
        }
    }
    pub(crate) fn events(&self) -> Vec<&HookEvidence> {
        let mut events: Vec<_> = self
            .prefix
            .iter()
            .chain(&self.suffix)
            .chain(&self.terminal)
            .map(AsRef::as_ref)
            .collect();
        events.sort_by_key(|event| event.sequence);
        events
    }
    pub(crate) fn omitted(&self) -> u64 {
        self.observed.saturating_sub(self.events().len() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(sequence: u64, kind: DiagnosticKind) -> HookEvidence {
        HookEvidence {
            sequence,
            received_at_unix_ms: Some(0),
            stage: "action".into(),
            text: HookText {
                kind,
                severity: Some(DiagnosticSeverity::Info),
                code: Some("trace".into()),
                message: Some(sequence.to_string()),
                completion_status: None,
                truncated: false,
                truncated_prefix_bytes: 0,
            },
        }
    }
    // Test-ID: PR-TEST-0518
    // Verifies: PR-REQ-0351
    #[test]
    fn flood_preserves_start_recent_tail_and_terminal_explanation() {
        let mut window = DiagnosticWindow::default();
        for sequence in 1..=10000 {
            window.push(event(sequence, DiagnosticKind::Diagnostic));
        }
        window.push(event(10001, DiagnosticKind::Completion));
        assert_eq!(window.prefix.front().unwrap().sequence, 1);
        assert_eq!(window.prefix.back().unwrap().sequence, 1024);
        assert_eq!(window.suffix.front().unwrap().sequence, 6929);
        assert_eq!(window.suffix.back().unwrap().sequence, 10000);
        assert_eq!(window.terminal.back().unwrap().sequence, 10001);
        assert_eq!(window.omitted(), 5904);
    }
    // Test-ID: PR-TEST-0519
    // Verifies: PR-REQ-0351
    #[test]
    fn truncation_keeps_both_ends_without_breaking_utf8_or_empty_presence() {
        let mut text = event(1, DiagnosticKind::Diagnostic).text;
        text.message = Some(format!("start{}end", "\u{754c}".repeat(30000)));
        let bounded = text.bounded();
        assert!(bounded.truncated);
        let message = bounded.message.unwrap();
        assert!(message.starts_with("start") && message.ends_with("end"));
        assert!(message.len() <= 65536);
        let mut empty = event(1, DiagnosticKind::Completion).text;
        empty.message = Some(String::new());
        assert_eq!(empty.bounded().message, Some(String::new()));
    }
}
