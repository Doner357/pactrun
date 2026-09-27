use std::{fmt, path::PathBuf};

use super::{
    ActionIdentity, ActionV1, ContentId, FiniteF64, FormatVersion, HookServiceContractV2,
    HostExecutableName, InputIdentity, InstanceId, InstanceStateVersion, ManagedInputPayloadId,
    ManagedInputProtection, ManagedOutputIdentity, OperationAccessV1, ParameterDefaultV1,
    ParameterIdentity, ParameterTypeV1, RevisionIdentity, RuntimeFileV1, SafeIntegerV1,
    TerminalContractV1, ValidatedRevisionContent,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParameterTextSource {
    Ordinary,
    Protected,
}

#[derive(Clone)]
pub(crate) struct RawParameterInput {
    pub(crate) id: ParameterIdentity,
    pub(crate) text: String,
    pub(crate) source: ParameterTextSource,
}

impl fmt::Debug for RawParameterInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RawParameterInput")
            .field("id", &self.id)
            .field("text", &"<redacted>")
            .field("source", &self.source)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub(crate) enum InvocationParameterValue {
    Integer(SafeIntegerV1),
    Float(FiniteF64),
    Boolean(bool),
    String(String),
}

struct VisibleParameterValue<'a>(&'a InvocationParameterValue);

impl fmt::Debug for VisibleParameterValue<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            InvocationParameterValue::Integer(value) => {
                formatter.debug_tuple("Integer").field(value).finish()
            }
            InvocationParameterValue::Float(value) => {
                formatter.debug_tuple("Float").field(value).finish()
            }
            InvocationParameterValue::Boolean(value) => {
                formatter.debug_tuple("Boolean").field(value).finish()
            }
            InvocationParameterValue::String(value) => {
                formatter.debug_tuple("String").field(value).finish()
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct ParameterBinding {
    pub(crate) id: ParameterIdentity,
    value: InvocationParameterValue,
    pub(crate) effective_redaction: bool,
}

impl fmt::Debug for ParameterBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("ParameterBinding");
        debug.field("id", &self.id);
        if self.effective_redaction {
            debug.field("value", &"<redacted>");
        } else {
            debug.field("value", &VisibleParameterValue(&self.value));
        }
        debug
            .field("effective_redaction", &self.effective_redaction)
            .finish()
    }
}

impl ParameterBinding {
    pub(crate) fn value(&self) -> &InvocationParameterValue {
        &self.value
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InvokeAction {
    pub(crate) instance: InstanceId,
    pub(crate) expected_state_version: InstanceStateVersion,
    pub(crate) active_revision: RevisionIdentity,
    pub(crate) action: ActionIdentity,
    pub(crate) parameters: Vec<ParameterBinding>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResolvedIntent {
    InvokeAction(InvokeAction),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ActiveInstanceBindingReference {
    pub(crate) input: InputIdentity,
    pub(crate) payload: ManagedInputPayloadId,
    pub(crate) protection: ManagedInputProtection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Compile-time launcher observation. The path is exact; this type does not
/// define a stable launcher object identity or replacement-detection contract.
pub(crate) struct InterpreterLauncherObservation {
    pub(crate) command: HostExecutableName,
    pub(crate) search_directories: Vec<PathBuf>,
    pub(crate) resolved_absolute_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CompiledHookLaunch {
    ShellLoader {
        shell: super::ShellKind,
        launcher: InterpreterLauncherObservation,
        script: RuntimeFileV1,
    },
    Direct {
        executable: RuntimeFileV1,
    },
    Interpreter {
        launcher: InterpreterLauncherObservation,
        interpreter_args: Vec<String>,
        script: RuntimeFileV1,
    },
}

impl CompiledHookLaunch {
    pub(crate) fn launcher(&self) -> Option<&InterpreterLauncherObservation> {
        match self {
            Self::Direct { .. } => None,
            Self::Interpreter { launcher, .. } | Self::ShellLoader { launcher, .. } => {
                Some(launcher)
            }
        }
    }

    pub(crate) fn matches_shell(
        &self,
        hook: &super::HookLaunchV1,
        files: &[RuntimeFileV1],
    ) -> bool {
        match (hook, self) {
            (
                super::HookLaunchV1::ShellLoader {
                    shell,
                    command,
                    script: id,
                },
                Self::ShellLoader {
                    shell: actual,
                    launcher,
                    script,
                },
            ) => {
                shell == actual
                    && shell.supported_on_host()
                    && command == &launcher.command
                    && id == &script.id
                    && files.contains(script)
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstanceCompilationState {
    pub(crate) instance: InstanceId,
    pub(crate) state_version: InstanceStateVersion,
    pub(crate) active_revision: RevisionIdentity,
    pub(crate) active_bindings: Vec<ActiveInstanceBindingReference>,
    pub(crate) required_inputs_satisfied: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ActionCompilationObservation {
    pub(crate) instance: InstanceId,
    pub(crate) state_version: InstanceStateVersion,
    pub(crate) active_revision: RevisionIdentity,
    pub(crate) active_bindings: Vec<ActiveInstanceBindingReference>,
    pub(crate) required_inputs_satisfied: bool,
    pub(crate) revision_content: ValidatedRevisionContent,
    pub(crate) service_state: Option<super::InstanceServiceState>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompilationFacts {
    pub(crate) instance: InstanceId,
    pub(crate) observed_state_version: InstanceStateVersion,
    pub(crate) active_revision: RevisionIdentity,
    pub(crate) active_bindings: Vec<ActiveInstanceBindingReference>,
    pub(crate) required_inputs_satisfied: bool,
    pub(crate) action: ActionV1,
    pub(crate) parameters: Vec<ParameterBinding>,
    pub(crate) runtime_content: Vec<RuntimeFileV1>,
    pub(crate) launch: CompiledHookLaunch,
    pub(crate) service_hook: HookServiceContractV2,
    pub(crate) service_bindings: super::ServiceHookBindings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActionPlanStep {
    EstablishSession,
    LaunchHook,
    AcceptCompletion,
    PublishDeclaredOutputs,
    Finalize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ActionExecutionPlan {
    instance: InstanceId,
    expected_state_version: InstanceStateVersion,
    active_revision: RevisionIdentity,
    action: ActionIdentity,
    access: OperationAccessV1,
    parameters: Vec<ParameterBinding>,
    active_bindings: Vec<ActiveInstanceBindingReference>,
    required_inputs_satisfied: bool,
    runtime_content: Vec<RuntimeFileV1>,
    protocol_version: FormatVersion,
    terminal: TerminalContractV1,
    hook_args: Vec<String>,
    outputs: Vec<ManagedOutputIdentity>,
    launch: CompiledHookLaunch,
    steps: Vec<ActionPlanStep>,
    service_hook: HookServiceContractV2,
    service_bindings: super::ServiceHookBindings,
}

impl ActionExecutionPlan {
    pub(crate) fn service_bindings(&self) -> &super::ServiceHookBindings {
        &self.service_bindings
    }
    pub(crate) fn service_hook(&self) -> &HookServiceContractV2 {
        &self.service_hook
    }
    pub(crate) fn instance(&self) -> InstanceId {
        self.instance
    }

    pub(crate) fn expected_state_version(&self) -> InstanceStateVersion {
        self.expected_state_version
    }

    pub(crate) fn active_revision(&self) -> &RevisionIdentity {
        &self.active_revision
    }

    pub(crate) fn action(&self) -> &ActionIdentity {
        &self.action
    }

    pub(crate) fn access(&self) -> OperationAccessV1 {
        self.access
    }

    pub(crate) fn parameters(&self) -> &[ParameterBinding] {
        &self.parameters
    }

    pub(crate) fn active_bindings(&self) -> &[ActiveInstanceBindingReference] {
        &self.active_bindings
    }

    pub(crate) fn required_inputs_satisfied(&self) -> bool {
        self.required_inputs_satisfied
    }

    pub(crate) fn runtime_content(&self) -> &[RuntimeFileV1] {
        &self.runtime_content
    }

    pub(crate) fn launch(&self) -> &CompiledHookLaunch {
        &self.launch
    }

    pub(crate) fn protocol_version(&self) -> FormatVersion {
        self.protocol_version
    }

    pub(crate) fn terminal(&self) -> TerminalContractV1 {
        self.terminal
    }

    pub(crate) fn hook_args(&self) -> &[String] {
        &self.hook_args
    }

    pub(crate) fn outputs(&self) -> &[ManagedOutputIdentity] {
        &self.outputs
    }

    pub(crate) fn steps(&self) -> &[ActionPlanStep] {
        &self.steps
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ActionResolutionError {
    InstanceNotFound,
    ActionNotFound,
    UnknownParameter(ParameterIdentity),
    DuplicateParameter(ParameterIdentity),
    MissingParameter(ParameterIdentity),
    InvalidParameter {
        parameter: ParameterIdentity,
        expected: ParameterTypeV1,
    },
    RepositoryInvariant(String),
}

impl fmt::Display for ActionResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InstanceNotFound => formatter.write_str("Instance was not found"),
            Self::ActionNotFound => {
                formatter.write_str("Action was not found in the active Revision")
            }
            Self::UnknownParameter(id) => write!(formatter, "unknown parameter {}", id.as_str()),
            Self::DuplicateParameter(id) => {
                write!(formatter, "duplicate parameter {}", id.as_str())
            }
            Self::MissingParameter(id) => write!(formatter, "missing parameter {}", id.as_str()),
            Self::InvalidParameter {
                parameter,
                expected,
            } => write!(
                formatter,
                "parameter {} is not a valid {expected:?} value",
                parameter.as_str()
            ),
            Self::RepositoryInvariant(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ActionResolutionError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PlanCompilationError {
    UnsupportedShell,
    InconsistentFacts,
    UnsupportedHookProtocol(FormatVersion),
    InvalidLauncherSearchDirectory,
    LauncherNotFound,
    MissingRuntimeContent(ContentId),
}

impl fmt::Display for PlanCompilationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedShell => formatter.write_str("shell kind is unsupported on this host"),
            Self::InconsistentFacts => {
                formatter.write_str("compilation facts do not match the resolved intent")
            }
            Self::UnsupportedHookProtocol(version) => {
                formatter.write_str(&super::VersionDomain::Hook.unsupported_message(*version))
            }
            Self::InvalidLauncherSearchDirectory => {
                formatter.write_str("launcher search directories must be absolute")
            }
            Self::LauncherNotFound => formatter.write_str("no eligible host launcher was found"),
            Self::MissingRuntimeContent(id) => write!(
                formatter,
                "runtime content {} is unavailable to compilation",
                id.as_str()
            ),
        }
    }
}

impl std::error::Error for PlanCompilationError {}

pub(crate) fn bind_action_parameters(
    action: &ActionV1,
    inputs: Vec<RawParameterInput>,
) -> Result<Vec<ParameterBinding>, ActionResolutionError> {
    bind_operation_parameters(&action.parameters, inputs)
}

pub(crate) fn bind_operation_parameters(
    parameters: &[super::ParameterV1],
    inputs: Vec<RawParameterInput>,
) -> Result<Vec<ParameterBinding>, ActionResolutionError> {
    let mut supplied = std::collections::BTreeMap::new();
    for input in inputs {
        if !parameters.iter().any(|parameter| parameter.id == input.id) {
            return Err(ActionResolutionError::UnknownParameter(input.id));
        }
        let id = input.id.clone();
        if supplied.insert(id.clone(), input).is_some() {
            return Err(ActionResolutionError::DuplicateParameter(id));
        }
    }

    parameters
        .iter()
        .map(|declaration| {
            if let Some(input) = supplied.remove(&declaration.id) {
                let value = parse_parameter_text(declaration.parameter_type, &input.text)
                    .ok_or_else(|| ActionResolutionError::InvalidParameter {
                        parameter: declaration.id.clone(),
                        expected: declaration.parameter_type,
                    })?;
                Ok(ParameterBinding {
                    id: declaration.id.clone(),
                    value,
                    effective_redaction: declaration.sensitive
                        || input.source == ParameterTextSource::Protected,
                })
            } else if let Some(default) = &declaration.default {
                Ok(ParameterBinding {
                    id: declaration.id.clone(),
                    value: value_from_default(default),
                    effective_redaction: declaration.sensitive,
                })
            } else {
                Err(ActionResolutionError::MissingParameter(
                    declaration.id.clone(),
                ))
            }
        })
        .collect()
}

fn value_from_default(default: &ParameterDefaultV1) -> InvocationParameterValue {
    match default {
        ParameterDefaultV1::Integer(value) => InvocationParameterValue::Integer(*value),
        ParameterDefaultV1::Float(value) => InvocationParameterValue::Float(*value),
        ParameterDefaultV1::Boolean(value) => InvocationParameterValue::Boolean(*value),
        ParameterDefaultV1::String(value) => InvocationParameterValue::String(value.clone()),
    }
}

fn parse_parameter_text(
    parameter_type: ParameterTypeV1,
    text: &str,
) -> Option<InvocationParameterValue> {
    match parameter_type {
        ParameterTypeV1::Integer => {
            if !is_integer_text(text) {
                return None;
            }
            let value = text.parse::<i64>().ok()?;
            SafeIntegerV1::new(value)
                .ok()
                .map(InvocationParameterValue::Integer)
        }
        ParameterTypeV1::Float => {
            if !is_number_text(text) {
                return None;
            }
            let value = text.parse::<f64>().ok()?;
            FiniteF64::new(value)
                .ok()
                .map(InvocationParameterValue::Float)
        }
        ParameterTypeV1::Boolean => match text {
            "true" => Some(InvocationParameterValue::Boolean(true)),
            "false" => Some(InvocationParameterValue::Boolean(false)),
            _ => None,
        },
        ParameterTypeV1::String => Some(InvocationParameterValue::String(text.to_owned())),
    }
}

fn is_integer_text(value: &str) -> bool {
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    !unsigned.is_empty()
        && unsigned.bytes().all(|byte| byte.is_ascii_digit())
        && (unsigned == "0" || !unsigned.starts_with('0'))
}

fn is_number_text(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut index = usize::from(bytes[0] == b'-');
    if index == bytes.len() {
        return false;
    }
    if bytes[index] == b'0' {
        index += 1;
        if index < bytes.len() && bytes[index].is_ascii_digit() {
            return false;
        }
    } else if matches!(bytes[index], b'1'..=b'9') {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
    } else {
        return false;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == start {
            return false;
        }
    }
    if index < bytes.len() && matches!(bytes[index], b'e' | b'E') {
        index += 1;
        if index < bytes.len() && matches!(bytes[index], b'+' | b'-') {
            index += 1;
        }
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == start {
            return false;
        }
    }
    index == bytes.len()
}

pub(crate) fn build_action_plan(
    intent: &InvokeAction,
    facts: CompilationFacts,
) -> Result<ActionExecutionPlan, PlanCompilationError> {
    if intent.instance != facts.instance
        || intent.expected_state_version != facts.observed_state_version
        || intent.active_revision != facts.active_revision
        || intent.action != facts.action.id
        || intent.parameters != facts.parameters
    {
        return Err(PlanCompilationError::InconsistentFacts);
    }
    let action = facts.action;
    Ok(ActionExecutionPlan {
        instance: intent.instance,
        expected_state_version: intent.expected_state_version,
        active_revision: intent.active_revision.clone(),
        action: intent.action.clone(),
        access: action.access,
        parameters: intent.parameters.clone(),
        active_bindings: facts.active_bindings,
        required_inputs_satisfied: facts.required_inputs_satisfied,
        runtime_content: facts.runtime_content,
        protocol_version: action.hook.protocol_version,
        terminal: action.hook.io.terminal,
        hook_args: action.hook.args,
        outputs: action.outputs.into_iter().map(|output| output.id).collect(),
        launch: facts.launch,
        service_hook: facts.service_hook,
        service_bindings: facts.service_bindings,
        steps: vec![
            ActionPlanStep::EstablishSession,
            ActionPlanStep::LaunchHook,
            ActionPlanStep::AcceptCompletion,
            ActionPlanStep::PublishDeclaredOutputs,
            ActionPlanStep::Finalize,
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{HookLaunchV1, IOContractV1, ManagedOutputV1, ParameterV1};

    fn parameter(id: &str, parameter_type: ParameterTypeV1, sensitive: bool) -> ParameterV1 {
        ParameterV1 {
            id: ParameterIdentity::parse(id).unwrap(),
            parameter_type,
            sensitive,
            default: None,
        }
    }

    fn action(parameters: Vec<ParameterV1>) -> ActionV1 {
        ActionV1 {
            id: ActionIdentity::parse("inspect").unwrap(),
            access: OperationAccessV1::Observe,
            parameters,
            hook: super::super::HookV1 {
                protocol_version: crate::domain::FormatVersion::BASELINE,
                launch: HookLaunchV1::Direct {
                    executable: ContentId::parse("tool").unwrap(),
                },
                args: Vec::new(),
                io: IOContractV1 {
                    terminal: TerminalContractV1::None,
                },
            },
            outputs: vec![ManagedOutputV1 {
                id: ManagedOutputIdentity::parse("result").unwrap(),
            }],
        }
    }

    fn raw(id: &str, text: &str, source: ParameterTextSource) -> RawParameterInput {
        RawParameterInput {
            id: ParameterIdentity::parse(id).unwrap(),
            text: text.to_owned(),
            source,
        }
    }

    // Test-ID: PR-TEST-0080
    // Verifies: PR-REQ-0133, PR-REQ-0134, PR-REQ-0273
    #[test]
    fn primitive_invocation_text_profile_is_exact() {
        let integer_zero = InvocationParameterValue::Integer(SafeIntegerV1::new(0).unwrap());
        assert!(parse_parameter_text(ParameterTypeV1::Integer, "-0") == Some(integer_zero));
        for valid in ["0", "1", "-3", "9007199254740991", "-9007199254740991"] {
            assert!(
                parse_parameter_text(ParameterTypeV1::Integer, valid).is_some(),
                "{valid}"
            );
        }
        for invalid in [
            "",
            "+1",
            "01",
            "-01",
            "1.0",
            "1e0",
            " 1",
            "1 ",
            "9007199254740992",
            "-9007199254740992",
            "9223372036854775808",
            "-9223372036854775809",
            "１",
            "١",
            "−1",
        ] {
            assert!(
                parse_parameter_text(ParameterTypeV1::Integer, invalid).is_none(),
                "{invalid}"
            );
        }
        for valid in [
            "0", "-0", "-0.0", "1", "-3", "1.5", "1e3", "1E-3", "1e-9999", "-1e-9999",
        ] {
            assert!(
                parse_parameter_text(ParameterTypeV1::Float, valid).is_some(),
                "{valid}"
            );
        }
        for invalid in [
            "", "+1", "01", ".5", "1.", "NaN", "Infinity", " 1", "1 ", "1e9999", "１.0", "1٫0",
            "−0", "1e３",
        ] {
            assert!(
                parse_parameter_text(ParameterTypeV1::Float, invalid).is_none(),
                "{invalid}"
            );
        }
        assert!(matches!(
            parse_parameter_text(ParameterTypeV1::Boolean, "true"),
            Some(InvocationParameterValue::Boolean(true))
        ));
        assert!(matches!(
            parse_parameter_text(ParameterTypeV1::Boolean, "false"),
            Some(InvocationParameterValue::Boolean(false))
        ));
        for invalid in ["TRUE", " true", "false ", "1", "yes", "ｔｒｕｅ"] {
            assert!(parse_parameter_text(ParameterTypeV1::Boolean, invalid).is_none());
        }

        for exact in [
            "",
            "true 🦀",
            " 001 ",
            "line one\nline two",
            "é",
            "e\u{301}",
        ] {
            assert!(
                parse_parameter_text(ParameterTypeV1::String, exact)
                    == Some(InvocationParameterValue::String(exact.to_owned()))
            );
        }

        let rounded_down = parse_parameter_text(ParameterTypeV1::Float, "9007199254740993");
        assert!(
            rounded_down
                == Some(InvocationParameterValue::Float(
                    FiniteF64::new(9_007_199_254_740_992.0).unwrap()
                ))
        );
        let rounded_up = parse_parameter_text(ParameterTypeV1::Float, "9007199254740995");
        assert!(
            rounded_up
                == Some(InvocationParameterValue::Float(
                    FiniteF64::new(9_007_199_254_740_996.0).unwrap()
                ))
        );
        for normalized_zero in ["-0", "-0.0", "1e-9999", "-1e-9999"] {
            let Some(InvocationParameterValue::Float(value)) =
                parse_parameter_text(ParameterTypeV1::Float, normalized_zero)
            else {
                panic!("expected normalized float zero for {normalized_zero}");
            };
            assert_eq!(serde_json::to_string(&value).unwrap(), "0.0");
        }
    }

    // Test-ID: PR-TEST-0597
    // Verifies: PR-REQ-0133, PR-REQ-0134
    #[test]
    fn binding_is_typed_ordered_and_redaction_only_becomes_stricter() {
        let declarations = action(vec![
            parameter("count", ParameterTypeV1::Integer, false),
            parameter("secret", ParameterTypeV1::String, true),
            parameter("source_protected", ParameterTypeV1::String, false),
        ]);
        let bindings = bind_action_parameters(
            &declarations,
            vec![
                raw(
                    "source_protected",
                    "protected-value",
                    ParameterTextSource::Protected,
                ),
                raw("secret", "declared-value", ParameterTextSource::Ordinary),
                raw("count", "3", ParameterTextSource::Ordinary),
            ],
        )
        .unwrap();
        assert_eq!(
            bindings
                .iter()
                .map(|binding| binding.id.as_str())
                .collect::<Vec<_>>(),
            vec!["count", "secret", "source_protected"]
        );
        assert!(!bindings[0].effective_redaction);
        assert!(bindings[1].effective_redaction);
        assert!(bindings[2].effective_redaction);
        let debug = format!("{bindings:?}");
        assert!(!debug.contains("declared-value"));
        assert!(!debug.contains("protected-value"));
    }

    #[test]
    fn sensitive_invalid_parameter_diagnostics_do_not_reveal_source_text() {
        let marker = "TOP-SECRET-INVALID-VALUE";
        for (sensitive, source) in [
            (true, ParameterTextSource::Ordinary),
            (false, ParameterTextSource::Protected),
        ] {
            let declarations = action(vec![parameter(
                "secret_count",
                ParameterTypeV1::Integer,
                sensitive,
            )]);
            let input = raw("secret_count", marker, source);
            assert!(!format!("{input:?}").contains(marker));
            let error = bind_action_parameters(&declarations, vec![input]).unwrap_err();
            assert!(!error.to_string().contains(marker));
            assert!(!format!("{error:?}").contains(marker));
        }
    }

    // Test-ID: PR-TEST-0598
    // Verifies: PR-REQ-0038, PR-REQ-0133, PR-REQ-0134
    #[test]
    fn binding_applies_defaults_and_rejects_unknown_duplicate_missing_and_invalid_values() {
        let mut defaulted = parameter("defaulted", ParameterTypeV1::Integer, false);
        defaulted.default = Some(ParameterDefaultV1::Integer(SafeIntegerV1::new(7).unwrap()));
        let declarations = action(vec![
            defaulted,
            parameter("required", ParameterTypeV1::Boolean, false),
        ]);
        let bindings = bind_action_parameters(
            &declarations,
            vec![raw("required", "true", ParameterTextSource::Ordinary)],
        )
        .unwrap();
        assert!(
            bindings[0].value == InvocationParameterValue::Integer(SafeIntegerV1::new(7).unwrap())
        );
        assert!(bindings[1].value == InvocationParameterValue::Boolean(true));

        assert!(matches!(
            bind_action_parameters(
                &declarations,
                vec![raw("unknown", "x", ParameterTextSource::Ordinary)]
            ),
            Err(ActionResolutionError::UnknownParameter(_))
        ));
        assert!(matches!(
            bind_action_parameters(
                &declarations,
                vec![
                    raw("required", "true", ParameterTextSource::Ordinary),
                    raw("required", "false", ParameterTextSource::Ordinary),
                ]
            ),
            Err(ActionResolutionError::DuplicateParameter(_))
        ));
        assert!(matches!(
            bind_action_parameters(&declarations, Vec::new()),
            Err(ActionResolutionError::MissingParameter(_))
        ));
        assert!(matches!(
            bind_action_parameters(
                &declarations,
                vec![raw("required", "TRUE", ParameterTextSource::Ordinary)]
            ),
            Err(ActionResolutionError::InvalidParameter { .. })
        ));
    }
}
