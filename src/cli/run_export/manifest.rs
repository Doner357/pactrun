//! Closed report projection. Internal payload identities and consent flags are absent.
use super::{
    RunExportOptions, RunId,
    execution_presentation::{ArtifactEdge, Inspection},
};
use crate::domain::TraceDocument;
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct InputEntry {
    pub input_id: String,
    pub scope: &'static str,
    pub edge: Option<ArtifactEdge>,
    pub snapshot_id: Option<String>,
    pub role: &'static str,
    pub protection_at_run: &'static str,
    pub export_protection: Option<&'static str>,
    pub state: &'static str,
    pub member: Option<String>,
}
#[derive(Serialize)]
pub(super) struct ArtifactEntry {
    pub output_id: String,
    pub edge: Option<ArtifactEdge>,
    pub state: &'static str,
    pub member: String,
}
#[derive(Serialize)]
struct Selection {
    hook_text: bool,
    artifacts: bool,
    inputs: bool,
    secrets: bool,
}
#[derive(Serialize)]
struct Consistency {
    run_data: &'static str,
    core_trace: &'static str,
    artifact_inventory: &'static str,
}
#[derive(Serialize)]
struct CoreTrace {
    state: &'static str,
    record: Option<TraceDocument>,
}
#[derive(Serialize)]
pub(super) struct Report {
    format: &'static str,
    format_version: &'static str,
    run_id: String,
    exporter_version: &'static str,
    warning: &'static str,
    selection: Selection,
    consistency: Consistency,
    inspection: Inspection,
    core_trace: CoreTrace,
    inputs: Vec<InputEntry>,
    artifacts: Vec<ArtifactEntry>,
}
impl Report {
    pub(super) fn new(
        run: RunId,
        options: RunExportOptions,
        trace: Option<TraceDocument>,
        inspection: Inspection,
        inputs: Vec<InputEntry>,
        artifacts: Vec<ArtifactEntry>,
    ) -> Self {
        Self {
            format: "pactrun.run-report",
            format_version: "1.0-alpha.1",
            run_id: run.to_string(),
            exporter_version: env!("CARGO_PKG_VERSION"),
            warning: "Review before sharing. Selected content is not redacted and may contain secrets. Metadata may also be sensitive. This report is not a backup or replay plan.",
            selection: Selection {
                hook_text: options.hook_text,
                artifacts: options.artifacts,
                inputs: options.inputs,
                secrets: options.secrets,
            },
            consistency: Consistency {
                run_data: "one_store_read_snapshot",
                core_trace: "independent_diagnostic_observation",
                artifact_inventory: "currently_retained_at_inspection",
            },
            inspection,
            core_trace: CoreTrace {
                state: if trace.is_some() {
                    "available"
                } else {
                    "unavailable"
                },
                record: trace,
            },
            inputs,
            artifacts,
        }
    }
}
