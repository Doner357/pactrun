//! One Store policy across phase documents and table-backed Hook/helper evidence.
//! Every writer holds the same narrow lock; primary Run transactions do not.
use crate::{
    domain::{
        CoreDiagnosticWindow, DiagnosticWindow, RunId, TraceDocument, TraceId, TracePolicy,
        TraceRetention,
    },
    persistence::{
        DiagnosticRetentionCandidate, DiagnosticRetentionInventory, PactrunPersistence,
        PersistenceError,
    },
    trace_journal::{Error, Journal, Location, RetentionGuard, TraceRetentionCandidate},
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) struct StoreWriter {
    root: PathBuf,
    store: Option<PactrunPersistence>,
    journal: Option<Journal>,
}
pub(crate) struct EvidenceUpdate<'a> {
    pub(crate) run: RunId,
    pub(crate) hook: &'a DiagnosticWindow,
    pub(crate) core: Option<&'a CoreDiagnosticWindow>,
    pub(crate) retain_text: bool,
    pub(crate) closed: bool,
    pub(crate) failed: bool,
}
impl StoreWriter {
    pub(crate) fn open(root: &Path) -> Result<Self, Error> {
        let _guard = RetentionGuard::acquire(root)?;
        Ok(Self {
            root: root.to_owned(),
            store: None,
            journal: None,
        })
    }
    pub(crate) fn save_trace(&mut self, document: &TraceDocument, now: u64) -> Result<(), Error> {
        let mut pending = document.clone();
        pending.retention = TraceRetention::NotChecked;
        {
            let _guard = RetentionGuard::acquire(&self.root)?;
            self.ensure_journal()?;
            self.journal
                .as_mut()
                .expect("opened phase journal")
                .save_pending_retention(&pending, now)?;
        }
        // Persist phase evidence before attempting to inspect the management
        // database. Damage or contention there must not erase the useful trace.
        let retention = self.ensure_store().and_then(|()| {
            let _guard = RetentionGuard::acquire(&self.root)?;
            self.retain(
                document.policy,
                now,
                Some(document.id),
                None,
                Scope::Combined,
            )
        });
        let status = match retention {
            Ok(()) => TraceRetention::Complete,
            Err(Error::Busy | Error::RetentionIncomplete) => TraceRetention::Incomplete,
            Err(_) => TraceRetention::Unavailable,
        };
        if let Ok(_guard) = RetentionGuard::acquire(&self.root) {
            let journal = self.journal.as_mut().expect("opened phase journal");
            if retention.is_err() {
                // Keep the phase file bounded even when combined retention
                // cannot be completed. The marker never claims combined success.
                let _ = journal.save(&pending, now);
            }
            let _ = journal.mark_retention(document.id, status);
        }
        Ok(())
    }
    fn ensure_store(&mut self) -> Result<(), Error> {
        if self.store.is_none() {
            self.store = Some(
                PactrunPersistence::open_for_diagnostics(&self.root).map_err(persistence_error)?,
            );
        }
        Ok(())
    }
    fn ensure_journal(&mut self) -> Result<(), Error> {
        if self.journal.is_none() {
            self.journal = Some(Journal::open(&Location::store(&self.root))?);
        }
        Ok(())
    }
    /// Errors mean recording failed. A successful write separately reports
    /// whether combined maintenance completed; unavailable peers do not stop it.
    pub(crate) fn save_evidence(
        &mut self,
        update: EvidenceUpdate<'_>,
        policy: TracePolicy,
        now: u64,
    ) -> Result<TraceRetention, Error> {
        if !policy.valid() {
            return Err(Error::InvalidRecord);
        }
        self.ensure_store()?;
        let _guard = RetentionGuard::acquire(&self.root)?;
        let store = self.store.as_ref().expect("qualified diagnostic Store");
        store
            .save_diagnostics(
                update.run,
                update.hook,
                update.retain_text,
                update.closed,
                update.failed,
            )
            .map_err(persistence_error)?;
        if let Some(core) = update.core {
            store
                .save_core_diagnostics(update.run, core, update.closed, update.failed)
                .map_err(persistence_error)?;
        }
        let result = self.retain(policy, now, None, Some(update.run), Scope::Combined);
        if result.is_err() {
            // A phase-file failure must not prevent storing bounded Hook/helper
            // evidence. Preserve the combined failure, not a false success.
            let _ = self.retain(policy, now, None, Some(update.run), Scope::TablesOnly);
        }
        Ok(match result {
            Ok(()) => TraceRetention::Complete,
            Err(Error::Busy | Error::RetentionIncomplete) => TraceRetention::Incomplete,
            Err(_) => TraceRetention::Unavailable,
        })
    }
    fn retain(
        &mut self,
        policy: TracePolicy,
        now: u64,
        trace: Option<TraceId>,
        run: Option<RunId>,
        scope: Scope,
    ) -> Result<(), Error> {
        if matches!(scope, Scope::Combined) {
            self.ensure_journal()?;
        }
        let mut journal = self
            .journal
            .as_mut()
            .filter(|_| matches!(scope, Scope::Combined));
        let store = self.store.as_ref().ok_or(Error::Missing)?;
        let deadline = Instant::now() + Duration::from_secs(1);
        let cutoff = now.saturating_sub(u64::from(policy.retention_days) * 86_400_000);
        // Expiry precedes capacity. Status/counter rows survive so inspection
        // can distinguish reclaimed events from an originally empty collection.
        loop {
            let sql = store
                .diagnostic_retention_inventory(Some(cutoff), None)
                .map_err(persistence_error)?;
            let (_, traces) = match journal.as_mut() {
                Some(journal) => journal.retention_inventory(Some(cutoff), None)?,
                None => (0, Vec::new()),
            };
            if sql.candidates.is_empty() && traces.is_empty() {
                break;
            }
            store
                .delete_diagnostic_retention_candidates(&sql.candidates)
                .map_err(persistence_error)?;
            if let Some(journal) = journal.as_mut() {
                journal.delete_retention_candidates(&traces)?;
            }
            if Instant::now() >= deadline {
                return Err(Error::RetentionIncomplete);
            }
        }
        loop {
            let DiagnosticRetentionInventory { bytes, candidates } = store
                .diagnostic_retention_inventory(None, run)
                .map_err(persistence_error)?;
            let (trace_bytes, traces) = match journal.as_mut() {
                Some(journal) => journal.retention_inventory(None, trace)?,
                None => (0, Vec::new()),
            };
            let total = bytes.checked_add(trace_bytes).ok_or(Error::InvalidRecord)?;
            let Some(mut excess) = total.checked_sub(policy.max_bytes).filter(|n| *n > 0) else {
                return Ok(());
            };
            let mut candidates: Vec<_> = candidates
                .into_iter()
                .map(Candidate::Table)
                .chain(traces.into_iter().map(Candidate::Trace))
                .collect();
            candidates.sort_by_key(Candidate::order);
            let mut table_victims = Vec::new();
            let mut trace_victims = Vec::new();
            for candidate in candidates.into_iter().take(64) {
                excess = excess.saturating_sub(candidate.bytes());
                match candidate {
                    Candidate::Table(value) => table_victims.push(value),
                    Candidate::Trace(value) => trace_victims.push(value),
                }
                if excess == 0 {
                    break;
                }
            }
            if table_victims.is_empty() && trace_victims.is_empty() {
                return Err(Error::InvalidRecord);
            }
            store
                .delete_diagnostic_retention_candidates(&table_victims)
                .map_err(persistence_error)?;
            if let Some(journal) = journal.as_mut() {
                journal.delete_retention_candidates(&trace_victims)?;
            }
            if Instant::now() >= deadline {
                return Err(Error::RetentionIncomplete);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Scope {
    Combined,
    TablesOnly,
}

enum Candidate {
    Table(DiagnosticRetentionCandidate),
    Trace(TraceRetentionCandidate),
}
impl Candidate {
    fn bytes(&self) -> u64 {
        match self {
            Self::Table(row) => row.bytes,
            Self::Trace(row) => row.bytes,
        }
    }
    fn order(&self) -> (bool, Option<u64>, [u8; 16], u64, u8) {
        match self {
            Self::Table(row) => {
                let (id, sequence, channel) = row.key();
                (row.preferred, row.received_at_ms, id, sequence, channel)
            }
            Self::Trace(row) => (row.preferred, Some(row.updated_at_ms), row.key(), 0, 2),
        }
    }
}
fn persistence_error(error: PersistenceError) -> Error {
    match error {
        PersistenceError::Io { source, .. } => match source.kind() {
            std::io::ErrorKind::NotFound => Error::Missing,
            std::io::ErrorKind::PermissionDenied => Error::AccessDenied,
            std::io::ErrorKind::StorageFull => Error::Full,
            _ => Error::Io,
        },
        PersistenceError::Sqlite { source, .. } => match source.sqlite_error_code() {
            Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
                Error::Busy
            }
            Some(rusqlite::ErrorCode::DiskFull) => Error::Full,
            Some(rusqlite::ErrorCode::PermissionDenied | rusqlite::ErrorCode::ReadOnly) => {
                Error::AccessDenied
            }
            _ => Error::Io,
        },
        PersistenceError::CorruptRun(_)
        | PersistenceError::SchemaMismatch(_)
        | PersistenceError::DatabaseOwnership(_) => Error::InvalidRecord,
        _ => Error::Io,
    }
}
