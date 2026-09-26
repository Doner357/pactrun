//! Explicit execution projections. Legacy free-form failure/completion text is
//! deliberately excluded; retained Hook evidence follows its own disclosure policy.
use super::*;
use crate::domain::{DiagnosticKind, DiagnosticSeverity, ManagedRunIdentity};
use serde::Serialize;

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0572
    // Verifies: PR-REQ-0361, PR-REQ-0362
    #[test]
    fn initialization_details_are_safe_core_messages_only() {
        let safe = "IPC initialization: permission denied; check execution-owner access to the temporary location.";
        let mut record = crate::domain::RunFailureRecord {
            error: crate::domain::PactrunErrorRef::new("execution", "ipc_initialization_failed")
                .unwrap(),
            message: safe.into(),
        };
        assert_eq!(Failure::new(&record, None).detail.as_deref(), Some(safe));
        record.message = "private path or password".into();
        assert!(Failure::new(&record, None).detail.is_none());
        record.message = safe.into();
        record.error =
            crate::domain::PactrunErrorRef::new("execution", "protocol_transport_failed").unwrap();
        assert!(Failure::new(&record, None).detail.is_none());
    }
    // Test-ID: PR-TEST-0567
    // Verifies: PR-REQ-0359, PR-REQ-0360
    #[test]
    fn json_run_projection_withholds_legacy_text_and_obeys_retention() {
        use crate::domain::*;
        let state = RunState::Finished(RunOutcomeView {
            outcome: RunOutcome::Failed,
            boundary: ActionRunBoundary::Admitted,
            terminal_risk: RecoveryRiskState::Clear,
            finished_at_unix_ms: u64::MAX,
            primary_failure: None,
            secondary_failures: vec![RunFailureRecord {
                error: PactrunErrorRef::new("execution", "launch_failed").unwrap(),
                message: "legacy-secret-sentinel".into(),
            }],
            hook_completion: None,
            artifacts: Vec::new(),
        });
        let value = serde_json::to_value(State::from(&state)).unwrap();
        assert!(!value.to_string().contains("legacy-secret-sentinel"));
        assert_eq!(value["finished_at_unix_ms"], u64::MAX.to_string());
        assert_eq!(
            value["secondary_failures"][0]["reference"]["code"],
            "launch_failed"
        );
        let mut view = DiagnosticInspection {
            retain_text: false,
            started: true,
            closed: false,
            observed: 4,
            failed: false,
            events: vec![HookEvidence {
                sequence: 4,
                received_at_unix_ms: None,
                stage: "action".into(),
                text: HookText {
                    kind: DiagnosticKind::Diagnostic,
                    severity: Some(DiagnosticSeverity::Warning),
                    completion_status: None,
                    code: Some("hook_code".into()),
                    message: Some("retained-hook-text".into()),
                    truncated: false,
                    truncated_prefix_bytes: 0,
                },
            }],
        };
        let hidden = serde_json::to_value(Diagnostics::from(&view)).unwrap();
        assert!(hidden["events"].as_array().unwrap().is_empty());
        assert_eq!(hidden["collection_closed"], false);
        view.retain_text = true;
        let shown = serde_json::to_value(Diagnostics::from(&view)).unwrap();
        assert_eq!(shown["events"][0]["source"], "hook");
        assert_eq!(shown["events"][0]["sequence"], "4");
        assert_eq!(shown["events"][0]["message"], "retained-hook-text");
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Failure {
    reference: presentation::ErrorReference,
    explanation: &'static str,
    detail: Option<String>,
    step: Option<String>,
}

impl Failure {
    fn new(
        record: &crate::domain::RunFailureRecord,
        step: Option<crate::domain::RunFailedStep>,
    ) -> Self {
        Self {
            reference: presentation::ErrorReference {
                owner: record.error.owner().into(),
                code: record.error.code().into(),
            },
            explanation: failure_explanation(&record.error),
            detail: failure_detail(record).map(str::to_owned),
            step: step.map(format_failed_step),
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Artifact {
    output_id: String,
    byte_length: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "phase", rename_all = "snake_case")]
pub(super) enum State {
    Running {
        boundary: &'static str,
        recovery_risk: &'static str,
    },
    Finished {
        boundary: &'static str,
        recovery_risk: &'static str,
        outcome: &'static str,
        finished_at_unix_ms: String,
        primary_failure: Option<Box<Failure>>,
        secondary_failures: Vec<Failure>,
        hook_completion_status: Option<&'static str>,
        legacy_hook_completion_text: &'static str,
        artifacts: Vec<Artifact>,
    },
}

impl From<&RunState> for State {
    fn from(state: &RunState) -> Self {
        match state {
            RunState::Running(v) => Self::Running {
                boundary: format_boundary(v.boundary),
                recovery_risk: format_risk(v.risk_state),
            },
            RunState::Finished(v) => Self::Finished {
                boundary: format_boundary(v.boundary),
                recovery_risk: format_risk(v.terminal_risk),
                outcome: format_outcome(v.outcome),
                finished_at_unix_ms: v.finished_at_unix_ms.to_string(),
                primary_failure: v
                    .primary_failure
                    .as_ref()
                    .map(|f| Box::new(Failure::new(&f.failure, Some(f.step)))),
                secondary_failures: v
                    .secondary_failures
                    .iter()
                    .map(|f| Failure::new(f, None))
                    .collect(),
                hook_completion_status: v
                    .hook_completion
                    .as_ref()
                    .map(|c| format_hook_status(c.status)),
                legacy_hook_completion_text: "withheld",
                artifacts: v
                    .artifacts
                    .iter()
                    .map(|a| Artifact {
                        output_id: a.output.as_str().into(),
                        byte_length: a.byte_length.to_string(),
                    })
                    .collect(),
            },
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Operation {
    Action {
        action_id: String,
    },
    SnapshotCapture,
    SnapshotRestore {
        snapshot_id: String,
    },
    Migration {
        target_revision: presentation::Revision,
        edge_count: usize,
    },
    InstanceDelete,
    InstanceAbandon,
}

impl From<&ManagedRunIdentity> for Operation {
    fn from(operation: &ManagedRunIdentity) -> Self {
        match operation {
            ManagedRunIdentity::Action(a) => Self::Action {
                action_id: a.action.as_str().into(),
            },
            ManagedRunIdentity::Capture { .. } => Self::SnapshotCapture,
            ManagedRunIdentity::Restore { snapshot, .. } => Self::SnapshotRestore {
                snapshot_id: snapshot.to_string(),
            },
            ManagedRunIdentity::Migration(m) => Self::Migration {
                target_revision: m.target().into(),
                edge_count: m.edge_count(),
            },
            ManagedRunIdentity::Deletion {
                mode: crate::domain::DeletionMode::ManagedCleanup,
                ..
            } => Self::InstanceDelete,
            ManagedRunIdentity::Deletion {
                mode: crate::domain::DeletionMode::AbandonManagement,
                ..
            } => Self::InstanceAbandon,
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Run {
    run_id: String,
    instance_id: String,
    revision: presentation::Revision,
    accepted_state_version: String,
    accepted_at_unix_ms: String,
    operation: Operation,
    state: State,
}

impl From<&crate::domain::ManagedRunView> for Run {
    fn from(run: &crate::domain::ManagedRunView) -> Self {
        Self {
            run_id: run.id.to_string(),
            instance_id: run.instance.to_string(),
            revision: run.operation.revision().into(),
            accepted_state_version: run.accepted_state_version.to_string(),
            accepted_at_unix_ms: run.accepted_at_unix_ms.to_string(),
            operation: (&run.operation).into(),
            state: (&run.state).into(),
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct HookEvent {
    source: &'static str,
    sequence: String,
    received_at_unix_ms: Option<String>,
    stage: String,
    kind: &'static str,
    severity: Option<&'static str>,
    completion_status: Option<&'static str>,
    code: Option<String>,
    message: Option<String>,
    truncated: bool,
    truncated_prefix_bytes: usize,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Diagnostics {
    retain_text: bool,
    started: bool,
    collection_closed: bool,
    persistence_failed: bool,
    observed: String,
    events: Vec<HookEvent>,
}

impl From<&crate::domain::DiagnosticInspection> for Diagnostics {
    fn from(view: &crate::domain::DiagnosticInspection) -> Self {
        Self {
            retain_text: view.retain_text,
            started: view.started,
            collection_closed: view.closed,
            persistence_failed: view.failed,
            observed: view.observed.to_string(),
            events: if view.retain_text {
                view.events
                    .iter()
                    .map(|e| HookEvent {
                        source: "hook",
                        sequence: e.sequence.to_string(),
                        received_at_unix_ms: e.received_at_unix_ms.map(|n| n.to_string()),
                        stage: e.stage.clone(),
                        kind: match e.text.kind {
                            DiagnosticKind::Diagnostic => "diagnostic",
                            DiagnosticKind::Completion => "completion",
                            DiagnosticKind::ProtocolError => "protocol_error",
                        },
                        severity: e.text.severity.map(|s| match s {
                            DiagnosticSeverity::Info => "info",
                            DiagnosticSeverity::Warning => "warning",
                            DiagnosticSeverity::Error => "error",
                        }),
                        completion_status: e.text.completion_status.map(format_hook_status),
                        code: e.text.code.clone(),
                        message: e.text.message.clone(),
                        truncated: e.text.truncated,
                        truncated_prefix_bytes: e.text.truncated_prefix_bytes,
                    })
                    .collect()
            } else {
                Vec::new()
            },
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct MigrationProgress {
    committed_edges: usize,
    boundary_revision: presentation::Revision,
    boundary_state_version: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Inspection {
    run: Run,
    current_recovery_guard: Option<presentation::Guard>,
    capture_result: Option<String>,
    migration_progress: Option<MigrationProgress>,
    diagnostics: Option<Diagnostics>,
}

impl From<&crate::domain::ManagedRunInspectionData> for Inspection {
    fn from(view: &crate::domain::ManagedRunInspectionData) -> Self {
        Self {
            run: (&view.run).into(),
            current_recovery_guard: view.current_recovery_guard.as_ref().map(Into::into),
            capture_result: view.capture_result.map(|id| id.to_string()),
            migration_progress: view.migration_progress.as_ref().map(|p| MigrationProgress {
                committed_edges: p.committed_edges,
                boundary_revision: (&p.boundary_revision).into(),
                boundary_state_version: p.boundary_state_version.to_string(),
            }),
            diagnostics: view.diagnostics.as_ref().map(Into::into),
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Reconciled {
    pub(super) run_ids: Vec<String>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Action {
    action_id: String,
    access: &'static str,
    terminal: &'static str,
    protocol_version: String,
    hook_args_count: usize,
    launch: Launch,
    parameters: Vec<Parameter>,
    output_ids: Vec<String>,
    metadata: definitions::Text,
    outputs: Vec<definitions::Output>,
    hook: Option<definitions::Hook>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Launch {
    Direct {
        content_id: String,
    },
    Interpreter {
        command: String,
        interpreter_args_count: usize,
        script: String,
    },
    ShellLoader {
        shell: String,
        command: String,
        script: String,
    },
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Parameter {
    parameter_id: String,
    parameter_type: &'static str,
    sensitive: bool,
    default_present: bool,
    default_redacted: bool,
    default_value: Option<definitions::DefaultValue>,
    metadata: definitions::Text,
}

impl From<&ActionV1> for Action {
    fn from(action: &ActionV1) -> Self {
        Self {
            metadata: definitions::Text::default(),
            outputs: action
                .outputs
                .iter()
                .map(|o| definitions::Output {
                    output_id: o.id.as_str().into(),
                    metadata: definitions::Text::default(),
                })
                .collect(),
            hook: None,
            action_id: action.id.as_str().into(),
            access: access_name(action.access),
            terminal: terminal_name(action.hook.io.terminal),
            protocol_version: action.hook.protocol_version.to_string(),
            hook_args_count: action.hook.args.len(),
            launch: match &action.hook.launch {
                HookLaunchV1::Direct { executable } => Launch::Direct {
                    content_id: executable.as_str().into(),
                },
                HookLaunchV1::Interpreter {
                    command,
                    interpreter_args,
                    script,
                } => Launch::Interpreter {
                    command: command.as_str().into(),
                    interpreter_args_count: interpreter_args.len(),
                    script: script.as_str().into(),
                },
                HookLaunchV1::ShellLoader {
                    shell,
                    command,
                    script,
                } => Launch::ShellLoader {
                    shell: shell.as_str().into(),
                    command: command.as_str().into(),
                    script: script.as_str().into(),
                },
            },
            parameters: action
                .parameters
                .iter()
                .map(|p| Parameter {
                    parameter_id: p.id.as_str().into(),
                    parameter_type: parameter_type_name(p.parameter_type),
                    sensitive: p.sensitive,
                    default_present: p.default.is_some(),
                    default_redacted: p.sensitive && p.default.is_some(),
                    default_value: if p.sensitive {
                        None
                    } else {
                        p.default.as_ref().map(Into::into)
                    },
                    metadata: definitions::Text::default(),
                })
                .collect(),
            output_ids: action
                .outputs
                .iter()
                .map(|o| o.id.as_str().into())
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Actions {
    pub(super) items: Vec<Action>,
}

impl Action {
    pub(super) fn defined(action: &ActionV1, row: &crate::domain::RevisionCatalogEntry) -> Self {
        use crate::domain::{PresentationTargetV1 as T, ServiceHookSite};
        let mut result = Self::from(action);
        result.metadata = definitions::Text::new(&row.metadata, T::Action(action.id.clone()));
        for (parameter, source) in result.parameters.iter_mut().zip(&action.parameters) {
            parameter.metadata = definitions::Text::new(
                &row.metadata,
                T::ActionParameter {
                    action: action.id.clone(),
                    parameter: source.id.clone(),
                },
            );
        }
        for (output, source) in result.outputs.iter_mut().zip(&action.outputs) {
            output.metadata = definitions::Text::new(
                &row.metadata,
                T::ManagedOutput {
                    action: action.id.clone(),
                    output: source.id.clone(),
                },
            );
        }
        result.hook = Some(definitions::Hook::new(
            &action.hook,
            &row.core,
            ServiceHookSite::Action(action.id.clone()),
        ));
        result
    }
}
impl From<&HookLaunchV1> for Launch {
    fn from(h: &HookLaunchV1) -> Self {
        match h {
            HookLaunchV1::Direct { executable } => Self::Direct {
                content_id: executable.as_str().into(),
            },
            HookLaunchV1::Interpreter {
                command,
                interpreter_args,
                script,
            } => Self::Interpreter {
                command: command.as_str().into(),
                interpreter_args_count: interpreter_args.len(),
                script: script.as_str().into(),
            },
            HookLaunchV1::ShellLoader {
                shell,
                command,
                script,
            } => Self::ShellLoader {
                shell: shell.as_str().into(),
                command: command.as_str().into(),
                script: script.as_str().into(),
            },
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct PlanParameter {
    pub(super) parameter_id: String,
    pub(super) effective_redaction: bool,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CompiledLaunch {
    Direct {
        runtime_path: String,
        executable: bool,
    },
    Interpreter {
        command: String,
        resolved_path: presentation::NativePath,
        search_directory_count: usize,
        interpreter_args_count: usize,
        script: String,
    },
    ShellLoader {
        shell: String,
        command: String,
        runtime_path: String,
    },
}

impl From<&CompiledHookLaunch> for CompiledLaunch {
    fn from(launch: &CompiledHookLaunch) -> Self {
        match launch {
            CompiledHookLaunch::Direct { executable } => Self::Direct {
                runtime_path: executable.path.as_str().into(),
                executable: executable.executable,
            },
            CompiledHookLaunch::Interpreter {
                launcher,
                interpreter_args,
                script,
            } => Self::Interpreter {
                command: launcher.command.as_str().into(),
                resolved_path: launcher.resolved_absolute_path.as_path().into(),
                search_directory_count: launcher.search_directories.len(),
                interpreter_args_count: interpreter_args.len(),
                script: script.path.as_str().into(),
            },
            CompiledHookLaunch::ShellLoader {
                shell,
                launcher,
                script,
            } => Self::ShellLoader {
                shell: shell.as_str().into(),
                command: launcher.command.as_str().into(),
                runtime_path: script.path.as_str().into(),
            },
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Plan {
    action_id: String,
    instance_id: String,
    revision: presentation::Revision,
    expected_state_version: String,
    access: &'static str,
    required_inputs_satisfied: bool,
    terminal: &'static str,
    protocol_version: String,
    parameters: Vec<PlanParameter>,
    launch: CompiledLaunch,
    hook_args_count: usize,
    steps: Vec<&'static str>,
    output_ids: Vec<String>,
    startup_timeout_ms: Option<String>,
    execution_timeout_ms: Option<String>,
    termination_grace_ms: String,
    recovery_override: bool,
    preview: &'static str,
}

impl Plan {
    pub(super) fn new(
        plan: &ActionExecutionPlan,
        recovery_override: bool,
        startup: Option<u64>,
        execution: Option<u64>,
        grace: Option<u64>,
    ) -> Self {
        Self {
            action_id: plan.action().as_str().into(),
            instance_id: plan.instance().to_string(),
            revision: plan.active_revision().into(),
            expected_state_version: plan.expected_state_version().to_string(),
            access: access_name(plan.access()),
            required_inputs_satisfied: plan.required_inputs_satisfied(),
            terminal: terminal_name(plan.terminal()),
            protocol_version: plan.protocol_version().to_string(),
            parameters: plan
                .parameters()
                .iter()
                .map(|p| PlanParameter {
                    parameter_id: p.id.as_str().into(),
                    effective_redaction: p.effective_redaction,
                })
                .collect(),
            launch: plan.launch().into(),
            hook_args_count: plan.hook_args().len(),
            steps: plan.steps().iter().map(|s| plan_step_name(*s)).collect(),
            output_ids: plan.outputs().iter().map(|id| id.as_str().into()).collect(),
            startup_timeout_ms: startup.map(|n| n.to_string()),
            execution_timeout_ms: execution.map(|n| n.to_string()),
            termination_grace_ms: grace.unwrap_or(5_000).to_string(),
            recovery_override,
            preview: "not_admitted",
        }
    }
}
