//! Capture acquisition and terminal ownership. Candidate locators remain
//! operation-local; only verified bytes enter the immutable Snapshot store.
use super::*;
use crate::{
    domain::*,
    managed_data::{StagedFile, StagingError},
    snapshot_integrity::{VerifiedSnapshotManifest, encode_snapshot_manifest},
};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read, Write},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug)]
pub(super) enum CaptureError {
    Capability(CapabilityRefusal),
    Invalid(&'static str),
    Host(io::ErrorKind),
    Persistence(PersistenceError),
}
impl From<io::Error> for CaptureError {
    fn from(e: io::Error) -> Self {
        Self::Host(e.kind())
    }
}
impl From<StagingError> for CaptureError {
    fn from(error: StagingError) -> Self {
        match error {
            StagingError::Io { source, .. } => Self::Host(source.kind()),
            StagingError::Unsupported(_) => Self::Host(io::ErrorKind::Unsupported),
            StagingError::ManagedInputTooLarge | StagingError::LengthOverflow => {
                Self::Invalid("private Capture staging violated its length contract")
            }
        }
    }
}
impl From<PersistenceError> for CaptureError {
    fn from(e: PersistenceError) -> Self {
        Self::Persistence(e)
    }
}
impl From<CapabilityRefusal> for CaptureError {
    fn from(e: CapabilityRefusal) -> Self {
        Self::Capability(e)
    }
}
impl From<AccountingError> for CaptureError {
    fn from(e: AccountingError) -> Self {
        match e {
            AccountingError::Capability(e) => Self::Capability(e),
            AccountingError::InconsistentSourceLength => {
                Self::Invalid("Capture source length changed")
            }
        }
    }
}
impl CaptureError {
    pub(super) fn message(&self) -> &'static str {
        match self {
            Self::Capability(_) => {
                "Snapshot Capture exceeds a structural or representation capability"
            }
            Self::Host(_) => "Snapshot Capture acquisition encountered a host I/O failure",
            Self::Persistence(_) => "Snapshot Capture could not read its authoritative pinned data",
            Self::Invalid(reason) => reason,
        }
    }
}

pub(super) struct CapturePreparation {
    id: SnapshotId,
    instance: InstanceId,
    revision: RevisionIdentity,
    registry: Vec<PinnedInputBinding>,
    inputs: Vec<InputDeclarationV1>,
    managed: BTreeMap<ManagedInputPayloadId, Sha256Digest>,
    blobs: BTreeMap<Sha256Digest, StagedFile>,
    acquisition: CaptureAcquisitionBudget,
    storage: SnapshotBlobBudget,
    candidate_root: File,
}
impl fmt::Debug for CapturePreparation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CapturePreparation(<owner-only>)")
    }
}
pub(super) struct PreparedCapture {
    pub(super) manifest: VerifiedSnapshotManifest,
    blobs: BTreeMap<Sha256Digest, StagedFile>,
    #[cfg(test)]
    acquired_bytes: u64,
}
impl fmt::Debug for PreparedCapture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PreparedCapture(<owner-only, not committable by reconciliation>)")
    }
}

struct HashWriter<'a> {
    file: &'a mut File,
    hash: Sha256,
    written: u64,
    expected: u64,
}
impl Write for HashWriter<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if b.len() as u64 > self.expected - self.written {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Capture source grew while copying",
            ));
        }
        self.file.write_all(b)?;
        self.hash.update(b);
        self.written += b.len() as u64;
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
fn stage_with(
    staging: &StagingSession,
    length: u64,
    copy: impl FnOnce(&mut HashWriter<'_>) -> Result<(), CaptureError>,
) -> Result<(Sha256Digest, StagedFile), CaptureError> {
    let mut bytes = staging.create_snapshot_stage()?;
    let digest = {
        let mut out = HashWriter {
            file: bytes.writer(),
            hash: Sha256::new(),
            written: 0,
            expected: length,
        };
        copy(&mut out)?;
        if out.written != length {
            return Err(CaptureError::Invalid("Capture source length changed"));
        }
        Sha256Digest::from_bytes(out.hash.finalize().into())
    };
    if bytes.finish_operation_file()? != length {
        return Err(CaptureError::Invalid("Capture staging length changed"));
    }
    Ok((digest, bytes))
}

fn open_candidate(root: &File, path: &RuntimePath) -> Result<File, CaptureError> {
    crate::opened_files::open_file(root,&path.as_str().split('/').collect::<Vec<_>>()).map_err(|error|{
        let unsafe_path=matches!(error.kind(),io::ErrorKind::InvalidData|io::ErrorKind::InvalidInput|io::ErrorKind::NotFound|io::ErrorKind::NotADirectory|io::ErrorKind::IsADirectory|io::ErrorKind::Unsupported);
        #[cfg(target_os="linux")]
        let unsafe_path=unsafe_path || matches!(error.raw_os_error(),Some(code) if code==rustix::io::Errno::LOOP.raw_os_error() || code==rustix::io::Errno::XDEV.raw_os_error());
        if unsafe_path {CaptureError::Invalid("Capture candidate is missing or is not a safe regular contained file")}else{CaptureError::Host(error.kind())}
    })
}

impl CapturePreparation {
    pub(super) fn new(
        p: &PactrunPersistence,
        staging: &StagingSession,
        run: RunId,
        plan: &SnapshotExecutionPlan,
        candidate: &std::path::Path,
        cancellation: &ActionCancellation,
    ) -> Result<Self, CaptureError> {
        let ManagedRunIdentity::Capture { revision } = plan.operation() else {
            return Err(CaptureError::Invalid("Run is not Snapshot Capture"));
        };
        let registry = p.admitted_capture_registry(run)?;
        SnapshotCapability::Descriptors.check(registry.len() as u64)?;
        let inputs = p
            .load_revision(revision)?
            .ok_or(CaptureError::Invalid("Capture Revision is unavailable"))?
            .content
            .core
            .inputs()
            .to_vec();
        let mut prepared = Self {
            id: SnapshotId::generate()
                .map_err(|_| CaptureError::Invalid("Capture identity allocation failed"))?,
            instance: plan.instance(),
            revision: revision.clone(),
            registry,
            inputs,
            managed: BTreeMap::new(),
            blobs: BTreeMap::new(),
            acquisition: CaptureAcquisitionBudget::default(),
            storage: SnapshotBlobBudget::new(BlobAccountingProfile::SnapshotStorage),
            candidate_root: crate::opened_files::open_root(candidate)?,
        };
        for binding in &prepared.registry {
            let Some(payload) = binding.payload else {
                continue;
            };
            if prepared.managed.contains_key(&payload) {
                continue;
            }
            let length = p.admitted_payload_length(run, &binding.input)?;
            prepared.acquisition.record(
                CaptureAcquisitionSource::Managed {
                    instance: prepared.instance,
                    payload,
                },
                length,
            )?;
            let (digest, bytes) = stage_with(staging, length, |out| {
                p.stream_admitted_payload(
                    run,
                    &binding.input,
                    &mut super::materialize::CancellableWriter {
                        destination: out,
                        cancellation,
                    },
                )?;
                Ok(())
            })?;
            prepared.storage.record(digest.clone(), length)?;
            prepared.managed.insert(payload, digest.clone());
            prepared.blobs.entry(digest).or_insert(bytes);
        }
        Ok(prepared)
    }
    pub(super) fn active(&self) -> impl Iterator<Item = &PinnedInputBinding> {
        self.registry
            .iter()
            .filter(|b| matches!(b.role, ManagedInputRole::Active { .. }) && b.payload.is_some())
    }
    pub(super) fn copy_binding(
        &self,
        binding: &PinnedInputBinding,
        destination: &mut File,
        cancellation: &ActionCancellation,
    ) -> Result<(), CaptureError> {
        let mut reader = self.blobs[&self.managed[&binding.payload.expect("bound active pin")]]
            .try_clone_reader()?;
        io::copy(
            &mut reader,
            &mut super::materialize::CancellableWriter {
                destination,
                cancellation,
            },
        )?;
        Ok(())
    }
    fn complete(
        mut self,
        staging: &StagingSession,
        at: SnapshotTimestamp,
        submitted: &[CaptureServiceContentSubmission],
    ) -> Result<PreparedCapture, CaptureError> {
        SnapshotCapability::Descriptors.check(
            (self.registry.len() as u64)
                .checked_add(submitted.len() as u64)
                .ok_or(CaptureError::Invalid("Capture descriptor count overflow"))?,
        )?;
        let mut sources: BTreeMap<RuntimePath, Sha256Digest> = BTreeMap::new();
        let mut services = Vec::new();
        let mut service_budget =
            SnapshotBlobBudget::new(BlobAccountingProfile::CapturedServiceContent);
        for item in submitted {
            let digest = if let Some(digest) = sources.get(&item.candidate_path) {
                digest.clone()
            } else {
                let mut reader = open_candidate(&self.candidate_root, &item.candidate_path)?;
                let length = reader.metadata()?.len();
                SnapshotCapability::CaptureServiceBlob.check(length)?;
                self.acquisition.record(
                    CaptureAcquisitionSource::Candidate(CaptureSourceId::from_operation_index(
                        sources.len() as u64,
                    )),
                    length,
                )?;
                let (digest, bytes) = stage_with(staging, length, |out| {
                    io::copy(&mut Read::by_ref(&mut reader).take(length + 1), out)?;
                    Ok(())
                })?;
                self.storage.record(digest.clone(), length)?;
                service_budget.record(digest.clone(), length)?;
                sources.insert(item.candidate_path.clone(), digest.clone());
                self.blobs.entry(digest.clone()).or_insert(bytes);
                digest
            };
            services.push(SnapshotServiceContent {
                role: item.role.clone(),
                path: item.path.clone(),
                blob_digest: digest,
            });
        }
        let bindings = self
            .registry
            .iter()
            .map(|b| SnapshotBinding {
                input_id: b.input.clone(),
                role: match b.role {
                    ManagedInputRole::Active { .. } => SnapshotBindingRole::Active,
                    ManagedInputRole::Retained => SnapshotBindingRole::Retained,
                },
                state: b
                    .payload
                    .map(|p| SnapshotBindingState::Bound(self.managed[&p].clone()))
                    .unwrap_or(SnapshotBindingState::Absent),
                protection: b.protection,
            })
            .collect();
        let manifest = SnapshotManifest::new(SnapshotManifestParts {
            version: SnapshotIntegrityVersion::BASELINE,
            snapshot_id: self.id,
            producer: self.revision.clone(),
            origin_instance_id: self.instance,
            captured_at: at,
            managed_bindings: bindings,
            service_content: services,
        })
        .map_err(|_| CaptureError::Invalid("Capture candidate has invalid Snapshot semantics"))?;
        let manifest = encode_snapshot_manifest(
            manifest,
            Some(&SnapshotProducerContext {
                revision: &self.revision,
                inputs: &self.inputs,
            }),
        )
        .map_err(|e| match e {
            crate::snapshot_integrity::SnapshotCodecError::Capability(e) => {
                CaptureError::Capability(e)
            }
            _ => CaptureError::Invalid("Capture candidate failed V2 validation"),
        })?;
        Ok(PreparedCapture {
            manifest,
            blobs: self.blobs,
            #[cfg(test)]
            acquired_bytes: self.acquisition.total(),
        })
    }
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct TestCaptureClock {
    time: std::sync::Mutex<Option<SystemTime>>,
    reads: std::sync::atomic::AtomicUsize,
}
#[cfg(test)]
thread_local! {static CLOCK:std::cell::RefCell<std::sync::Arc<TestCaptureClock>>=std::cell::RefCell::new(std::sync::Arc::new(TestCaptureClock::default()));}
#[cfg(test)]
pub(crate) fn capture_clock_for_test() -> std::sync::Arc<TestCaptureClock> {
    CLOCK.with(|c| c.borrow().clone())
}
#[cfg(test)]
pub(crate) fn install_capture_clock_for_test(clock: std::sync::Arc<TestCaptureClock>) {
    CLOCK.with(|c| *c.borrow_mut() = clock);
}
#[cfg(test)]
pub(super) fn test_clock(time: Option<SystemTime>) -> usize {
    let clock = capture_clock_for_test();
    *clock.time.lock().unwrap() = time;
    clock.reads.load(std::sync::atomic::Ordering::SeqCst)
}
pub(super) fn completion_time() -> Result<SnapshotTimestamp, CaptureError> {
    let now = SystemTime::now();
    #[cfg(test)]
    let now = {
        let clock = capture_clock_for_test();
        clock
            .reads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        clock.time.lock().unwrap().unwrap_or(now)
    };
    timestamp(now)
}
fn timestamp(now: SystemTime) -> Result<SnapshotTimestamp, CaptureError> {
    let invalid = || CaptureError::Invalid("Capture wall clock is out of range");
    let (seconds, nanos) = match now.duration_since(UNIX_EPOCH) {
        Ok(d) => (
            i64::try_from(d.as_secs()).map_err(|_| invalid())?,
            i64::from(d.subsec_nanos()),
        ),
        Err(e) => {
            let d = e.duration();
            let seconds = i64::try_from(d.as_secs())
                .map_err(|_| invalid())?
                .checked_neg()
                .ok_or_else(invalid)?;
            if d.subsec_nanos() == 0 {
                (seconds, 0)
            } else {
                (
                    seconds.checked_sub(1).ok_or_else(invalid)?,
                    1_000_000_000 - i64::from(d.subsec_nanos()),
                )
            }
        }
    };
    SnapshotTimestamp::new(seconds, nanos).map_err(|_| invalid())
}

// Test-ID: PR-TEST-0254
// Verifies: PR-REQ-0290
#[test]
fn capture_wall_clock_normalizes_pre_epoch_instants() {
    use std::time::Duration;
    for (delta, seconds, nanos) in [
        (Duration::ZERO, 0, 0),
        (Duration::from_secs(1), -1, 0),
        (Duration::from_millis(250), -1, 750_000_000),
        (Duration::from_millis(1250), -2, 750_000_000),
    ] {
        assert_eq!(
            timestamp(UNIX_EPOCH - delta).unwrap(),
            SnapshotTimestamp::new(seconds, nanos).unwrap()
        );
    }
}

#[derive(Debug)]
pub(crate) struct CaptureFinalization {
    pub(super) facts: RuntimeTerminalFacts,
    pub(super) acquisition: Option<CapturePreparation>,
    pub(super) captured_at: Option<SnapshotTimestamp>,
    pub(super) submitted: Vec<CaptureServiceContentSubmission>,
    prepared: Option<PreparedCapture>,
    failure: Option<&'static str>,
    cleanup_attempted: bool,
    cleanup_failed: bool,
}
#[cfg(test)]
impl CaptureFinalization {
    pub(super) fn corrupt_staging_for_test(&mut self) {
        use std::io::{Seek, SeekFrom};
        let bytes = self
            .prepared
            .as_mut()
            .unwrap()
            .blobs
            .values_mut()
            .next()
            .unwrap();
        bytes.writer().seek(SeekFrom::Start(0)).unwrap();
        bytes.writer().write_all(b"!").unwrap();
        bytes.writer().sync_all().unwrap();
    }

    pub(super) fn prepared_metrics(&self) -> Option<(SnapshotId, SnapshotTimestamp, u64, usize)> {
        self.prepared.as_ref().map(|p| {
            (
                p.manifest.manifest().snapshot_id(),
                p.manifest.manifest().captured_at(),
                p.acquired_bytes,
                p.blobs.len(),
            )
        })
    }
}
pub(super) fn terminal(
    mut facts: RuntimeTerminalFacts,
    acquisition: Option<CapturePreparation>,
    captured_at: Option<SnapshotTimestamp>,
    submitted: Vec<CaptureServiceContentSubmission>,
) -> OwnerContinuation {
    if let Some(primary) = &mut facts.primary_failure {
        primary.step = RunFailedStep::for_operation(
            primary.step.rank(),
            ManagedExecutionKind::SnapshotCapture,
        )
        .expect("known failure step");
    }
    OwnerContinuation::CaptureFinalization(Box::new(CaptureFinalization {
        facts,
        acquisition,
        captured_at,
        submitted,
        prepared: None,
        failure: None,
        cleanup_attempted: false,
        cleanup_failed: false,
    }))
}

pub(super) fn advance(
    p: &PactrunPersistence,
    staging: &StagingSession,
    mut guard: ContinuationGuard<'_>,
) -> Result<bool, PersistenceError> {
    let Some(OwnerContinuation::CaptureFinalization(state)) = guard.continuation.as_mut() else {
        unreachable!()
    };
    if matches!(
        p.load_managed_run(state.facts.run)?.map(|v| v.state),
        Some(RunState::Finished(_))
    ) {
        guard.complete();
        return Ok(true);
    }
    if state.facts.process_started && !state.facts.process_terminated {
        return Err(PersistenceError::InvalidRunTransition(
            "Capture process is still live".to_owned(),
        ));
    }
    if state.facts.outcome == RunOutcome::Succeeded
        && state.failure.is_none()
        && state.prepared.is_none()
    {
        let result = match (state.acquisition.take(), state.captured_at) {
            (Some(acquisition), Some(at)) => acquisition.complete(staging, at, &state.submitted),
            _ => Err(CaptureError::Invalid(
                "Capture completion has no authoritative candidate",
            )),
        };
        match result {
            Ok(prepared) => state.prepared = Some(prepared),
            Err(error) => state.failure = Some(error.message()),
        }
    }
    if state.facts.outcome != RunOutcome::Succeeded || state.failure.is_some() {
        state.acquisition = None;
        state.prepared = None;
    }
    if !state.cleanup_attempted {
        state.cleanup_attempted = true;
        if let Some(execution) = &state.facts.execution {
            state.cleanup_failed = execution.cleanup().is_err();
        }
    }
    let mut finish = RunFinish {
        outcome: state.facts.outcome,
        primary_failure: state.facts.primary_failure.clone(),
        secondary_failures: Vec::new(),
        hook_completion: state
            .facts
            .hook_completion
            .as_ref()
            .map(structural_hook_completion),
    };
    if let Some(message) = state.failure
        && finish.outcome == RunOutcome::Succeeded
    {
        finish.outcome = RunOutcome::Failed;
        finish.primary_failure = Some(RunPrimaryFailure {
            cause: None,
            failure: safe_execution_failure("managed_output_publication_failed", message),
            step: RunFailedStep::SnapshotPlan(SnapshotPlanStep::PublishManagedResult),
        });
    }
    if state.cleanup_failed {
        finish.secondary_failures.push(safe_execution_failure(
            "workspace_cleanup_failed",
            "Capture execution workspace cleanup failed",
        ));
    }
    let result = if let Some(prepared) = &state.prepared {
        let readers = prepared
            .blobs
            .values()
            .map(|b| b.try_clone_reader())
            .collect::<Result<Vec<_>, _>>();
        let mut readers = match readers {
            Ok(readers) => readers,
            Err(_) => {
                state.failure = Some("Capture private staging encountered a host I/O failure");
                state.prepared = None;
                return Ok(false);
            }
        };
        let mut writes = prepared
            .blobs
            .iter()
            .zip(readers.iter_mut())
            .map(
                |((digest, bytes), reader)| crate::persistence::SnapshotBlobWrite {
                    digest: digest.clone(),
                    byte_len: bytes.byte_len(),
                    reader,
                },
            )
            .collect::<Vec<_>>();
        p.publish_capture(
            &staging.owner(),
            state.facts.run,
            &finish,
            &prepared.manifest,
            &mut writes,
        )
    } else {
        p.finish_run_owned(&staging.owner(), state.facts.run, &finish, &[], &mut [])
    };
    match result {
        Ok(_) => {
            guard.complete();
            Ok(true)
        }
        Err(PersistenceError::CorruptSnapshot(_)) => {
            state.failure = Some("Capture staged content failed integrity verification");
            state.prepared = None;
            Ok(false)
        }
        Err(PersistenceError::SnapshotCodec(
            crate::snapshot_integrity::SnapshotCodecError::Capability(_),
        )) => {
            state.failure =
                Some("Snapshot Capture exceeds a structural or representation capability");
            state.prepared = None;
            Ok(false)
        }
        Err(error) => Err(error),
    }
}
