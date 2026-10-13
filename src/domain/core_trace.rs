//! Non-authoritative, bounded evidence. No filesystem, CLI or persistence policy.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    str::FromStr,
};

pub(crate) const TRACE_DOCUMENT_LIMIT: usize = 524_288;
pub(crate) const TRACE_EVENT_LIMIT: usize = 1024;
const PREFIX: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(try_from = "String", into = "String")]
pub(crate) struct TraceId([u8; 16]);
impl TraceId {
    pub(crate) fn generate() -> Result<Self, getrandom::Error> {
        let mut id = [0; 16];
        getrandom::fill(&mut id)?;
        Ok(Self(id))
    }
    pub(crate) fn from_bytes(id: [u8; 16]) -> Self {
        Self(id)
    }
    pub(crate) fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}
impl fmt::Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.0))
    }
}
impl From<TraceId> for String {
    fn from(id: TraceId) -> Self {
        id.to_string()
    }
}
impl TryFrom<String> for TraceId {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl FromStr for TraceId {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 32
            || !s
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid diagnostic identity");
        }
        let mut bytes = [0; 16];
        hex::decode_to_slice(s, &mut bytes).map_err(|_| "invalid diagnostic identity")?;
        Ok(Self(bytes))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum TraceOperation {
    #[default]
    Unknown,
    Catalog,
    PackageInstall,
    RevisionExport,
    InstanceCreate,
    InputSet,
    InputExport,
    InputDelete,
    ActionInvoke,
    SnapshotCapture,
    SnapshotRestore,
    SnapshotTransport,
    Migration,
    Retirement,
    Recovery,
    Collection,
    RunExport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum TracePhase {
    Command,
    Configuration,
    Resolution,
    StoreOpen,
    Compilation,
    Admission,
    Materialization,
    SessionSetup,
    HookLaunch,
    HookExecution,
    OutputPublication,
    StorageFinalization,
    ResultCommit,
    OutputDelivery,
    InputRead,
    OutputValidation,
    DurableRecordWrite,
    DirectorySync,
}
impl TracePhase {
    pub(crate) fn detailed(self) -> bool {
        matches!(
            self,
            Self::InputRead
                | Self::OutputValidation
                | Self::DurableRecordWrite
                | Self::DirectorySync
        )
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum PhaseResult {
    Started,
    Succeeded,
    Failed,
    Unfinished,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct PhaseEvent {
    #[serde(with = "decimal")]
    #[cfg_attr(test, schemars(with = "String"))]
    pub(crate) sequence: u64,
    #[serde(with = "decimal")]
    #[cfg_attr(test, schemars(with = "String"))]
    pub(crate) span: u64,
    pub(crate) phase: TracePhase,
    pub(crate) result: PhaseResult,
    #[serde(with = "decimal")]
    #[cfg_attr(test, schemars(with = "String"))]
    pub(crate) at_us: u64,
    #[serde(with = "decimal_option")]
    #[cfg_attr(test, schemars(with = "Option<String>"))]
    pub(crate) elapsed_us: Option<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct PhaseTotals {
    #[serde(with = "decimal")]
    #[cfg_attr(test, schemars(with = "String"))]
    pub(crate) started: u64,
    #[serde(with = "decimal")]
    #[cfg_attr(test, schemars(with = "String"))]
    pub(crate) finished: u64,
    #[serde(with = "decimal")]
    #[cfg_attr(test, schemars(with = "String"))]
    pub(crate) failed: u64,
    #[serde(with = "decimal")]
    #[cfg_attr(test, schemars(with = "String"))]
    pub(crate) unfinished: u64,
    #[serde(with = "decimal")]
    #[cfg_attr(test, schemars(with = "String"))]
    pub(crate) elapsed_us: u64,
    #[serde(with = "decimal")]
    #[cfg_attr(test, schemars(with = "String"))]
    pub(crate) max_elapsed_us: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct TracePolicy {
    pub(crate) detailed: bool,
    pub(crate) retention_days: u32,
    pub(crate) max_bytes: u64,
}
impl TracePolicy {
    pub(crate) fn startup() -> Self {
        Self {
            detailed: false,
            retention_days: 30,
            max_bytes: 64 * 1024 * 1024,
        }
    }
    pub(crate) fn valid(self) -> bool {
        (1..=3650).contains(&self.retention_days)
            && (1_048_576..=1_073_741_824).contains(&self.max_bytes)
    }
}
impl Default for TracePolicy {
    fn default() -> Self {
        Self {
            max_bytes: 256 * 1024 * 1024,
            ..Self::startup()
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) enum TraceFormat {
    #[serde(rename = "pactrun.core-trace")]
    Core,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) enum TraceVersion {
    #[serde(rename = "1.0-alpha.1")]
    V1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct TraceProducer {
    product_version: String,
    target: String,
    source_commit: Option<String>,
    source_manifest_sha256: Option<String>,
}
impl TraceProducer {
    fn current() -> Self {
        Self {
            product_version: env!("CARGO_PKG_VERSION").into(),
            target: env!("PACTRUN_BUILD_TARGET").into(),
            source_commit: option_env!("PACTRUN_BUILD_SOURCE_COMMIT").map(str::to_owned),
            source_manifest_sha256: option_env!("PACTRUN_BUILD_SOURCE_MANIFEST").map(str::to_owned),
        }
    }
    fn valid(&self) -> bool {
        fn digest(value: &Option<String>, size: usize) -> bool {
            value.as_ref().is_none_or(|v| {
                v.len() == size
                    && v.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
        }
        self.product_version
            .parse::<super::ProductVersion>()
            .is_ok()
            && !self.target.is_empty()
            && self.target.len() <= 128
            && self
                .target
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            && digest(&self.source_commit, 40)
            && digest(&self.source_manifest_sha256, 64)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct TraceDocument {
    pub(crate) format: TraceFormat,
    pub(crate) format_version: TraceVersion,
    pub(crate) id: TraceId,
    pub(crate) producer: TraceProducer,
    #[serde(with = "run_ref")]
    #[cfg_attr(test, schemars(with = "Option<String>"))]
    pub(crate) run_id: Option<super::RunId>,
    pub(crate) operation: TraceOperation,
    #[serde(with = "decimal_option")]
    #[cfg_attr(test, schemars(with = "Option<String>"))]
    pub(crate) started_at_unix_ms: Option<u64>,
    pub(crate) closed: bool,
    pub(crate) exit_code: Option<i32>,
    pub(crate) saturated: bool,
    pub(crate) association_conflict: bool,
    #[serde(with = "decimal")]
    #[cfg_attr(test, schemars(with = "String"))]
    pub(crate) observed: u64,
    #[serde(with = "decimal")]
    #[cfg_attr(test, schemars(with = "String"))]
    pub(crate) omitted: u64,
    pub(crate) policy: TracePolicy,
    pub(crate) totals: BTreeMap<TracePhase, PhaseTotals>,
    pub(crate) events: Vec<PhaseEvent>,
}
impl TraceDocument {
    pub(crate) fn valid(&self) -> bool {
        if !self.producer.valid()
            || !self.policy.valid()
            || self.events.len() > TRACE_EVENT_LIMIT
            || self.omitted != self.observed.saturating_sub(self.events.len() as u64)
            || self.events.len() as u64 > self.observed
            || self.exit_code.is_some_and(|v| !(0..=255).contains(&v))
            || self.closed != self.exit_code.is_some()
        {
            return false;
        }
        let mut previous = 0;
        for event in &self.events {
            if event.sequence <= previous
                || event.sequence > self.observed
                || event.span == 0
                || (event.result == PhaseResult::Started) != event.elapsed_us.is_none()
                || event
                    .elapsed_us
                    .is_some_and(|elapsed| elapsed > event.at_us)
            {
                return false;
            }
            previous = event.sequence;
        }
        self.totals.values().all(|t| {
            t.finished <= t.started
                && t.failed
                    .checked_add(t.unfinished)
                    .is_some_and(|n| n <= t.finished)
                && t.max_elapsed_us <= t.elapsed_us
        })
    }
}

#[derive(Debug)]
pub(crate) struct TraceWindow {
    document: TraceDocument,
    suffix: VecDeque<PhaseEvent>,
    next_span: u64,
}
impl TraceWindow {
    pub(crate) fn id(&self) -> TraceId {
        self.document.id
    }
    pub(crate) fn new(
        id: TraceId,
        operation: TraceOperation,
        at: Option<u64>,
        policy: TracePolicy,
    ) -> Self {
        Self {
            document: TraceDocument {
                format: TraceFormat::Core,
                format_version: TraceVersion::V1,
                id,
                producer: TraceProducer::current(),
                run_id: None,
                operation,
                started_at_unix_ms: at,
                closed: false,
                exit_code: None,
                saturated: false,
                association_conflict: false,
                observed: 0,
                omitted: 0,
                policy,
                totals: BTreeMap::new(),
                events: Vec::new(),
            },
            suffix: VecDeque::new(),
            next_span: 0,
        }
    }
    pub(crate) fn set_operation(&mut self, operation: TraceOperation) {
        self.document.operation = operation;
    }
    pub(crate) fn set_policy(&mut self, policy: TracePolicy) {
        self.document.policy = policy;
    }
    pub(crate) fn associate(&mut self, run: super::RunId) {
        if self.document.run_id.is_some_and(|previous| previous != run) {
            self.document.association_conflict = true;
        } else {
            self.document.run_id = Some(run);
        }
    }
    pub(crate) fn begin(&mut self, phase: TracePhase, at_us: u64) -> Option<u64> {
        if self.document.closed {
            return None;
        }
        let Some(span) = self.next_span.checked_add(1) else {
            self.document.saturated = true;
            return None;
        };
        self.next_span = span;
        let totals = self.document.totals.entry(phase).or_default();
        totals.started = totals.started.saturating_add(1);
        self.event(PhaseEvent {
            sequence: 0,
            span,
            phase,
            result: PhaseResult::Started,
            at_us,
            elapsed_us: None,
        });
        Some(span)
    }
    pub(crate) fn end(
        &mut self,
        span: u64,
        phase: TracePhase,
        at_us: u64,
        elapsed_us: u64,
        result: PhaseResult,
    ) {
        if self.document.closed || result == PhaseResult::Started {
            return;
        }
        let totals = self.document.totals.entry(phase).or_default();
        totals.finished = totals.finished.saturating_add(1);
        totals.failed = totals
            .failed
            .saturating_add(u64::from(result == PhaseResult::Failed));
        totals.unfinished = totals
            .unfinished
            .saturating_add(u64::from(result == PhaseResult::Unfinished));
        if let Some(sum) = totals.elapsed_us.checked_add(elapsed_us) {
            totals.elapsed_us = sum;
        } else {
            totals.elapsed_us = u64::MAX;
            self.document.saturated = true;
        }
        totals.max_elapsed_us = totals.max_elapsed_us.max(elapsed_us);
        self.event(PhaseEvent {
            sequence: 0,
            span,
            phase,
            result,
            at_us,
            elapsed_us: Some(elapsed_us),
        });
    }
    fn event(&mut self, mut event: PhaseEvent) {
        if self.document.saturated || (!self.document.policy.detailed && event.phase.detailed()) {
            return;
        }
        let Some(sequence) = self.document.observed.checked_add(1) else {
            self.document.saturated = true;
            return;
        };
        self.document.observed = sequence;
        event.sequence = sequence;
        if self.document.events.len() < PREFIX {
            self.document.events.push(event);
        } else {
            self.suffix.push_back(event);
            if self.suffix.len() > TRACE_EVENT_LIMIT - PREFIX {
                self.suffix.pop_front();
            }
        }
    }
    pub(crate) fn close(&mut self, exit_code: i32) {
        if !self.document.closed {
            self.document.closed = true;
            self.document.exit_code = Some(exit_code);
        }
    }
    pub(crate) fn snapshot(&self) -> TraceDocument {
        let mut result = self.document.clone();
        result.events.extend(self.suffix.iter().cloned());
        result.omitted = result.observed.saturating_sub(result.events.len() as u64);
        result
    }
}

mod decimal {
    use serde::{Deserialize, Deserializer, Serializer};
    pub(super) fn serialize<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        let text = String::deserialize(d)?;
        parse(&text).ok_or_else(|| serde::de::Error::custom("invalid decimal counter"))
    }
    pub(super) fn parse(s: &str) -> Option<u64> {
        if s.is_empty()
            || !s.bytes().all(|b| b.is_ascii_digit())
            || (s.len() > 1 && s.starts_with('0'))
        {
            None
        } else {
            s.parse().ok()
        }
    }
}
mod decimal_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub(super) fn serialize<S: Serializer>(v: &Option<u64>, s: S) -> Result<S::Ok, S::Error> {
        v.map(|v| v.to_string()).serialize(s)
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
        Option::<String>::deserialize(d)?
            .map(|s| {
                super::decimal::parse(&s)
                    .ok_or_else(|| serde::de::Error::custom("invalid decimal counter"))
            })
            .transpose()
    }
}
mod run_ref {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub(super) fn serialize<S: Serializer>(
        v: &Option<super::super::RunId>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        v.map(|id| id.to_string()).serialize(s)
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<super::super::RunId>, D::Error> {
        Option::<String>::deserialize(d)?
            .map(|s| {
                s.parse()
                    .map_err(|_| serde::de::Error::custom("invalid Run identity"))
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests;
