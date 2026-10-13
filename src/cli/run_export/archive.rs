//! ZIP is presentation transport, never authoritative Run state or an import format.
use super::manifest::{ArtifactEntry, InputEntry, Report};
use super::*;
use crate::{
    domain::*,
    persistence::{PersistenceError, RunExportSink},
    zip_output::DropSafeWriter,
};
use std::{cell::Cell, rc::Rc};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

type Archive<'a> = ZipWriter<DropSafeWriter<&'a mut File>>;
pub(super) struct Writer<'a> {
    zip: Archive<'a>,
    failed: Rc<Cell<Option<io::ErrorKind>>>,
    run: RunId,
    options: RunExportOptions,
    trace: Option<TraceDocument>,
    inspection: Option<execution_presentation::Inspection>,
    edges: Vec<MigrationEdgeIdentity>,
    inputs: Vec<InputEntry>,
    artifacts: Vec<ArtifactEntry>,
    copied_inputs: usize,
}
fn failure(error: io::Error) -> PersistenceError {
    PersistenceError::Io {
        operation: "write Run report staging",
        source: io::Error::from(error.kind()),
    }
}
fn zip_io(error: zip::result::ZipError) -> io::Error {
    match error {
        zip::result::ZipError::Io(error) => error,
        _ => io::ErrorKind::InvalidData.into(),
    }
}
fn options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o600)
        .large_file(true)
}
impl<'a> Writer<'a> {
    pub(super) fn new(
        stage: &'a mut StagedFile,
        run: RunId,
        options: RunExportOptions,
        trace: Option<TraceDocument>,
    ) -> Self {
        let failed = Rc::new(Cell::new(None));
        Self {
            zip: ZipWriter::new(DropSafeWriter {
                inner: stage.writer(),
                failed: failed.clone(),
                position: 0,
                length: 0,
            }),
            failed,
            run,
            options,
            trace,
            inspection: None,
            edges: vec![],
            inputs: vec![],
            artifacts: vec![],
            copied_inputs: 0,
        }
    }
    pub(super) fn finish(mut self) -> Result<Exported, PersistenceError> {
        let inspection = self
            .inspection
            .take()
            .ok_or_else(|| PersistenceError::CorruptRun("report has no Run observation".into()))?;
        let result = Exported {
            run_id: self.run.to_string(),
            outcome: "exported",
            copied_inputs: self.copied_inputs,
            unavailable_inputs: self.inputs.len() - self.copied_inputs,
            copied_artifacts: self.artifacts.len(),
            review_before_sharing: true,
        };
        let report = Report::new(
            self.run,
            self.options,
            self.trace,
            inspection,
            self.inputs,
            self.artifacts,
        );
        self.zip
            .start_file("report.json", options())
            .map_err(zip_io)
            .map_err(failure)?;
        serde_json::to_writer_pretty(&mut self.zip, &report)
            .map_err(|_| failure(io::ErrorKind::Other.into()))?;
        self.zip.write_all(b"\n").map_err(failure)?;
        let finished = self.zip.finish();
        if let Some(kind) = self.failed.get() {
            return Err(failure(kind.into()));
        }
        finished.map_err(zip_io).map_err(failure)?;
        Ok(result)
    }
}

/// Unavailable values have no member; an available empty payload still has one.
struct LazyMember<'a, 'b> {
    zip: &'a mut Archive<'b>,
    name: &'a str,
    started: bool,
}
impl LazyMember<'_, '_> {
    fn start(&mut self) -> io::Result<()> {
        if !self.started {
            self.zip.start_file(self.name, options()).map_err(zip_io)?;
            self.started = true;
        }
        Ok(())
    }
}
impl Write for LazyMember<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        self.start()?;
        self.zip.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.zip.flush()
    }
}
impl RunExportSink for Writer<'_> {
    fn inspection(&mut self, view: &ManagedRunInspectionData) -> Result<(), PersistenceError> {
        if view.run.id != self.run || self.inspection.is_some() {
            return Err(PersistenceError::CorruptRun(
                "report Run attribution changed".into(),
            ));
        }
        if let ManagedRunIdentity::Migration(invocation) = &view.run.operation {
            self.edges = invocation
                .path()
                .windows(2)
                .map(|pair| MigrationEdgeIdentity {
                    source: pair[0].content_digest,
                    target: pair[1].content_digest,
                })
                .collect();
        }
        let projected =
            execution_presentation::Inspection::for_version(view, machine::Version::Operations)
                .map_err(|_| {
                    PersistenceError::CorruptRun("Run report cannot project history".into())
                })?;
        self.inspection = Some(projected);
        Ok(())
    }
    fn input(
        &mut self,
        context: &HistoricalInputContext,
        input: &HistoricalInput,
        copy: impl FnOnce(&mut dyn Write) -> Result<HistoricalInputExport, PersistenceError>,
    ) -> Result<(), PersistenceError> {
        let name = format!("inputs/{:06}.bin", self.inputs.len());
        let mut member = LazyMember {
            zip: &mut self.zip,
            name: &name,
            started: false,
        };
        let outcome = copy(&mut member)?;
        let (state, exported_protection) = match outcome {
            HistoricalInputExport::Copied { protection } => {
                member.start().map_err(failure)?;
                self.copied_inputs += 1;
                ("copied", Some(protection))
            }
            HistoricalInputExport::NotRecorded => ("not_recorded", None),
            HistoricalInputExport::Absent => ("absent", None),
            HistoricalInputExport::Transient => ("transient", None),
            HistoricalInputExport::PendingOutput => ("pending_output", None),
            HistoricalInputExport::Unavailable => ("unavailable", None),
        };
        if exported_protection.is_none() && member.started {
            return Err(PersistenceError::CorruptRun(
                "unavailable Input wrote report bytes".into(),
            ));
        }
        let edge = context
            .edge_index
            .map(|index| {
                self.edges.get(index).ok_or_else(|| {
                    PersistenceError::CorruptRun("report edge is unavailable".into())
                })
            })
            .transpose()?;
        self.inputs.push(InputEntry {
            input_id: input.input.as_str().into(),
            scope: match context.scope {
                InputHistoryScope::Current => "current",
                InputHistoryScope::Snapshot => "snapshot",
                InputHistoryScope::Source => "source",
                InputHistoryScope::TargetPrepared => "target_prepared",
                InputHistoryScope::TargetCommitted => "target_committed",
            },
            edge: edge.map(execution_presentation::ArtifactEdge::from),
            snapshot_id: context.snapshot.map(|id| id.to_string()),
            role: match input.role {
                InputHistoryRole::Active => "active",
                InputHistoryRole::Retained => "retained",
            },
            protection_at_run: protection(input.protection),
            export_protection: exported_protection.map(protection),
            state,
            member: exported_protection.map(|_| name),
        });
        Ok(())
    }
    fn artifact(
        &mut self,
        artifact: &RunArtifactSummary,
        copy: impl FnOnce(&mut dyn Write) -> Result<(), PersistenceError>,
    ) -> Result<(), PersistenceError> {
        let name = format!("artifacts/{:06}.bin", self.artifacts.len());
        self.zip
            .start_file(&name, options())
            .map_err(zip_io)
            .map_err(failure)?;
        copy(&mut self.zip)?;
        self.artifacts.push(ArtifactEntry {
            output_id: artifact.output.as_str().into(),
            edge: artifact
                .edge
                .as_ref()
                .map(execution_presentation::ArtifactEdge::from),
            state: "copied",
            member: name,
        });
        Ok(())
    }
}
fn protection(value: ManagedInputProtection) -> &'static str {
    match value {
        ManagedInputProtection::Normal => "normal",
        ManagedInputProtection::Secret => "secret",
    }
}
