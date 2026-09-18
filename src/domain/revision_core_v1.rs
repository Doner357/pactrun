use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use serde::{Serialize, Serializer};

use super::{PactrunErrorRefV1, PresentationTargetV1};

pub(crate) const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

macro_rules! semantic_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub(crate) struct $name(String);

        impl $name {
            pub(crate) fn parse(value: impl Into<String>) -> Result<Self, RevisionCoreV1Error> {
                let value = value.into();
                validate_semantic_identifier(&value)?;
                Ok(Self(value))
            }

            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

semantic_id!(InputIdentity);
semantic_id!(ActionIdentity);
semantic_id!(ParameterIdentity);
semantic_id!(ManagedOutputIdentity);
semantic_id!(ContentId);
semantic_id!(HookCodeV1);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub(crate) struct RuntimePath(String);

impl RuntimePath {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, RevisionCoreV1Error> {
        let value = value.into();
        if !is_runtime_path(&value) {
            return Err(RevisionCoreV1Error::internal(
                "invalid_runtime_path",
                format!("invalid runtime path {value:?}"),
            ));
        }
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct HostExecutableName(String);

impl HostExecutableName {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, RevisionCoreV1Error> {
        let value = value.into();
        let valid_length = !value.is_empty() && value.len() <= 128;
        let valid_edges = value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
            && value
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric);
        let valid_characters = value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
        if !valid_length
            || !value.is_ascii()
            || !valid_edges
            || !valid_characters
            || value == "."
            || value == ".."
            || is_windows_reserved(&value)
        {
            return Err(RevisionCoreV1Error::stable(
                "invalid_host_executable_name",
                format!("invalid host executable name {value:?}"),
            ));
        }
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub(crate) struct Sha256Digest(String);

impl Sha256Digest {
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(format!("sha256:{}", hex::encode(bytes)))
    }

    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, RevisionCoreV1Error> {
        let value = value.into();
        let Some(hex) = value.strip_prefix("sha256:") else {
            return Err(RevisionCoreV1Error::internal(
                "invalid_digest",
                "digest must use sha256 prefix",
            ));
        };
        if hex.len() != 64
            || !hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(RevisionCoreV1Error::internal(
                "invalid_digest",
                "digest must contain 64 lowercase hexadecimal characters",
            ));
        }
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn to_bytes(&self) -> [u8; 32] {
        let bytes = hex::decode(
            self.0
                .strip_prefix("sha256:")
                .expect("typed digest always has the SHA-256 prefix"),
        )
        .expect("typed digest always has lowercase hexadecimal payload");
        bytes
            .try_into()
            .expect("typed SHA-256 digest always contains 32 bytes")
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct PositiveVersion(i64);

impl PositiveVersion {
    pub(crate) fn new(value: i64) -> Result<Self, RevisionCoreV1Error> {
        if !(1..=MAX_SAFE_INTEGER).contains(&value) {
            return Err(RevisionCoreV1Error::stable(
                "invalid_number",
                "Hook protocol_version must be a positive safe integer",
            ));
        }
        Ok(Self(value))
    }

    pub(crate) fn get(self) -> i64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct SafeIntegerV1(i64);

impl SafeIntegerV1 {
    pub(crate) fn new(value: i64) -> Result<Self, RevisionCoreV1Error> {
        if !(-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value) {
            return Err(RevisionCoreV1Error::stable(
                "invalid_number",
                "integer default must be within the safe-integer range",
            ));
        }
        Ok(Self(value))
    }

    pub(crate) fn get(self) -> i64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FiniteF64(f64);

impl FiniteF64 {
    pub(crate) fn new(value: f64) -> Result<Self, RevisionCoreV1Error> {
        if !value.is_finite() {
            return Err(RevisionCoreV1Error::stable(
                "invalid_number",
                "float default must be finite binary64",
            ));
        }
        Ok(Self(if value == 0.0 { 0.0 } else { value }))
    }

    pub(crate) fn get(self) -> f64 {
        self.0
    }
}

impl Serialize for FiniteF64 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct RevisionCoreV1 {
    format_version: u8,
    inputs: Vec<InputDeclarationV1>,
    actions: Vec<ActionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot: Option<SnapshotCapabilityV1>,
    migrations: Vec<MigrationV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cleanup: Option<CleanupV1>,
}

impl RevisionCoreV1 {
    pub(crate) fn contains_shell_loader(&self) -> bool {
        let mut found = false;
        visit_hooks(self, &mut |hook| {
            found |= matches!(hook.launch, HookLaunchV1::ShellLoader { .. });
            Ok(())
        })
        .expect("infallible Hook visit");
        found
    }

    pub(crate) fn inputs(&self) -> &[InputDeclarationV1] {
        &self.inputs
    }

    pub(crate) fn actions(&self) -> &[ActionV1] {
        &self.actions
    }

    pub(crate) fn snapshot(&self) -> Option<&SnapshotCapabilityV1> {
        self.snapshot.as_ref()
    }

    pub(crate) fn migrations(&self) -> &[MigrationV1] {
        &self.migrations
    }

    pub(crate) fn cleanup(&self) -> Option<&CleanupV1> {
        self.cleanup.as_ref()
    }

    pub(crate) fn contains_presentation_target(&self, target: &PresentationTargetV1) -> bool {
        match target {
            PresentationTargetV1::Revision => true,
            PresentationTargetV1::Input(input) => {
                self.inputs.iter().any(|declared| &declared.id == input)
            }
            PresentationTargetV1::Action(action) => {
                self.actions.iter().any(|declared| &declared.id == action)
            }
            PresentationTargetV1::ActionParameter { action, parameter } => self
                .actions
                .iter()
                .find(|declared| &declared.id == action)
                .is_some_and(|declared| {
                    declared
                        .parameters
                        .iter()
                        .any(|candidate| &candidate.id == parameter)
                }),
            PresentationTargetV1::ManagedOutput { action, output } => self
                .actions
                .iter()
                .find(|declared| &declared.id == action)
                .is_some_and(|declared| {
                    declared
                        .outputs
                        .iter()
                        .any(|candidate| &candidate.id == output)
                }),
            PresentationTargetV1::SnapshotCapture => self
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.capture.is_some()),
            PresentationTargetV1::SnapshotCaptureParameter(parameter) => self
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.capture.as_ref())
                .is_some_and(|capture| {
                    capture
                        .parameters
                        .iter()
                        .any(|candidate| &candidate.id == parameter)
                }),
            PresentationTargetV1::SnapshotRestore => self
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.restore.is_some()),
            PresentationTargetV1::SnapshotRestoreParameter(parameter) => self
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.restore.as_ref())
                .is_some_and(|restore| {
                    restore
                        .parameters
                        .iter()
                        .any(|candidate| &candidate.id == parameter)
                }),
            PresentationTargetV1::MigrationEdge(source_digest) => {
                self.migrations.iter().any(|migration| {
                    migration.source_revision_digest.to_bytes() == *source_digest.as_bytes()
                })
            }
            PresentationTargetV1::Cleanup => self.cleanup.is_some(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RevisionCoreProjectionInputV1 {
    pub(crate) inputs: Vec<InputDeclarationV1>,
    pub(crate) actions: Vec<ActionV1>,
    pub(crate) snapshot: Option<SnapshotCapabilityV1>,
    pub(crate) migrations: Vec<MigrationV1>,
    pub(crate) cleanup: Option<CleanupV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct InputDeclarationV1 {
    pub(crate) id: InputIdentity,
    pub(crate) required: bool,
    pub(crate) protection: InputProtectionV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InputProtectionV1 {
    Normal,
    Secret,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ActionV1 {
    pub(crate) id: ActionIdentity,
    pub(crate) access: OperationAccessV1,
    pub(crate) parameters: Vec<ParameterV1>,
    pub(crate) hook: HookV1,
    pub(crate) outputs: Vec<ManagedOutputV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OperationAccessV1 {
    Observe,
    Mutate,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ManagedOutputV1 {
    pub(crate) id: ManagedOutputIdentity,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ParameterV1 {
    pub(crate) id: ParameterIdentity,
    #[serde(rename = "type")]
    pub(crate) parameter_type: ParameterTypeV1,
    pub(crate) sensitive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default: Option<ParameterDefaultV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ParameterTypeV1 {
    Integer,
    Float,
    Boolean,
    String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum ParameterDefaultV1 {
    Integer(SafeIntegerV1),
    Float(FiniteF64),
    Boolean(bool),
    String(String),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct HookV1 {
    pub(crate) protocol_version: PositiveVersion,
    pub(crate) launch: HookLaunchV1,
    pub(crate) args: Vec<String>,
    pub(crate) io: IOContractV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum HookLaunchV1 {
    Direct {
        executable: ContentId,
    },
    Interpreter {
        command: HostExecutableName,
        interpreter_args: Vec<String>,
        script: ContentId,
    },
    // Shared runtime representation; only the Core V3 codec admits this variant.
    ShellLoader {
        shell: ShellKind,
        command: HostExecutableName,
        script: ContentId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShellKind {
    Sh,
    Bash,
    #[serde(rename = "powershell_7")]
    Powershell7,
    #[serde(rename = "windows_powershell_5_1")]
    WindowsPowershell51,
}

impl ShellKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Sh => "sh",
            Self::Bash => "bash",
            Self::Powershell7 => "powershell_7",
            Self::WindowsPowershell51 => "windows_powershell_5_1",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "sh" => Some(Self::Sh),
            "bash" => Some(Self::Bash),
            "powershell_7" => Some(Self::Powershell7),
            "windows_powershell_5_1" => Some(Self::WindowsPowershell51),
            _ => None,
        }
    }

    pub(crate) fn supported_on_host(self) -> bool {
        match self {
            Self::Sh | Self::Bash => cfg!(unix),
            Self::Powershell7 | Self::WindowsPowershell51 => cfg!(windows),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct IOContractV1 {
    pub(crate) terminal: TerminalContractV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TerminalContractV1 {
    None,
    Output,
    Interactive,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct SnapshotCapabilityV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) capture: Option<CaptureV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) restore: Option<RestoreV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct CaptureV1 {
    pub(crate) parameters: Vec<ParameterV1>,
    pub(crate) access: OperationAccessV1,
    pub(crate) hook: HookV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct RestoreV1 {
    pub(crate) parameters: Vec<ParameterV1>,
    pub(crate) hook: HookV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct MigrationV1 {
    pub(crate) source_revision_digest: Sha256Digest,
    pub(crate) transitions: Vec<MigrationTransitionV1>,
    pub(crate) requires_source: Vec<InputBindingRefV1>,
    pub(crate) requires_target: Vec<InputIdentity>,
    pub(crate) produces_target: Vec<InputIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) hook: Option<HookV1>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct InputBindingRefV1 {
    pub(crate) role: InputBindingRoleV1,
    pub(crate) input_id: InputIdentity,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InputBindingRoleV1 {
    Active,
    Retained,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum MigrationTransitionV1 {
    Carry {
        source: InputBindingRefV1,
        target_input_id: InputIdentity,
    },
    Declassify {
        source: InputBindingRefV1,
        target_input_id: InputIdentity,
    },
    Keep {
        source: InputBindingRefV1,
    },
    Discard {
        source: InputBindingRefV1,
    },
}

impl MigrationTransitionV1 {
    pub(crate) fn source(&self) -> &InputBindingRefV1 {
        match self {
            Self::Carry { source, .. }
            | Self::Declassify { source, .. }
            | Self::Keep { source }
            | Self::Discard { source } => source,
        }
    }

    pub(crate) fn target(&self) -> Option<&InputIdentity> {
        match self {
            Self::Carry {
                target_input_id, ..
            }
            | Self::Declassify {
                target_input_id, ..
            } => Some(target_input_id),
            Self::Keep { .. } | Self::Discard { .. } => None,
        }
    }

    fn kind_key(&self) -> &'static str {
        match self {
            Self::Carry { .. } => "carry",
            Self::Declassify { .. } => "declassify",
            Self::Keep { .. } => "keep",
            Self::Discard { .. } => "discard",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct CleanupV1 {
    pub(crate) requires: Vec<InputBindingRefV1>,
    pub(crate) hook: HookV1,
}

/// The second RevisionCoreFormatV1 semantic/hash component.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct RuntimeContentClosureIdentityV1 {
    files: Vec<RuntimeFileV1>,
}

impl RuntimeContentClosureIdentityV1 {
    /// Identity-preserving read-only access for physical availability checks.
    pub(crate) fn files(&self) -> &[RuntimeFileV1] {
        &self.files
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeContentProjectionInputV1 {
    pub(crate) files: Vec<RuntimeFileV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct RuntimeFileV1 {
    pub(crate) id: ContentId,
    pub(crate) path: RuntimePath,
    pub(crate) kind: RuntimeFileKindV1,
    pub(crate) blob_digest: Sha256Digest,
    pub(crate) executable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuntimeFileKindV1 {
    RegularFile,
}

/// Validated sibling components accepted by the digest API.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ValidatedRevisionContentV1 {
    pub(crate) core: RevisionCoreV1,
    pub(crate) runtime_content: RuntimeContentClosureIdentityV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RelationalValidationV1 {
    Valid,
    NotEvaluated,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RevisionCoreV1Error {
    internal_code: &'static str,
    stable_ref: Option<PactrunErrorRefV1>,
    message: String,
}

impl RevisionCoreV1Error {
    pub(crate) fn stable(code: &'static str, message: impl Into<String>) -> Self {
        assert!(
            matches!(
                code,
                "duplicate_property"
                    | "invalid_hook_launch"
                    | "invalid_host_executable_name"
                    | "invalid_number"
                    | "invalid_source_binding_role"
                    | "invalid_unicode_scalar"
                    | "unknown_field"
            ),
            "stable RevisionCoreFormatV1 code must be catalog-backed"
        );
        Self {
            internal_code: code,
            stable_ref: Some(
                PactrunErrorRefV1::new("revision_core_format_v1", code)
                    .expect("Frozen error owner and code are valid"),
            ),
            message: message.into(),
        }
    }

    pub(crate) fn internal(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            internal_code: code,
            stable_ref: None,
            message: message.into(),
        }
    }

    pub(crate) fn internal_code(&self) -> &str {
        self.internal_code
    }

    pub(crate) fn stable_ref(&self) -> Option<&PactrunErrorRefV1> {
        self.stable_ref.as_ref()
    }
}

impl fmt::Display for RevisionCoreV1Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.internal_code, self.message)
    }
}

impl std::error::Error for RevisionCoreV1Error {}

pub(crate) fn project_revision_core_v1(
    mut input: RevisionCoreProjectionInputV1,
) -> Result<RevisionCoreV1, RevisionCoreV1Error> {
    sort_unique(&mut input.inputs, |value| value.id.as_str(), "Input")?;
    let target_inputs: BTreeSet<_> = input.inputs.iter().map(|value| value.id.clone()).collect();

    for action in &mut input.actions {
        normalize_parameters(&mut action.parameters)?;
        sort_unique(
            &mut action.outputs,
            |value| value.id.as_str(),
            "ManagedOutput",
        )?;
    }
    sort_unique(&mut input.actions, |value| value.id.as_str(), "Action")?;

    if let Some(snapshot) = &mut input.snapshot {
        if snapshot.capture.is_none() && snapshot.restore.is_none() {
            return Err(RevisionCoreV1Error::internal(
                "invalid_snapshot",
                "SnapshotCapabilityV1 must contain capture or restore",
            ));
        }
        if let Some(capture) = &mut snapshot.capture {
            normalize_parameters(&mut capture.parameters)?;
        }
        if let Some(restore) = &mut snapshot.restore {
            normalize_parameters(&mut restore.parameters)?;
        }
    }

    for migration in &mut input.migrations {
        normalize_migration(migration, &target_inputs)?;
    }
    input.migrations.sort_by(|left, right| {
        left.source_revision_digest
            .cmp(&right.source_revision_digest)
    });
    reject_adjacent_duplicates(
        &input.migrations,
        |left, right| left.source_revision_digest == right.source_revision_digest,
        "Migration",
    )?;

    if let Some(cleanup) = &mut input.cleanup {
        normalize_binding_refs(&mut cleanup.requires)?;
        let mut roles = BTreeMap::new();
        for reference in &cleanup.requires {
            record_source_role(reference, &mut roles)?;
            let declared = target_inputs.contains(&reference.input_id);
            if (reference.role == InputBindingRoleV1::Active) != declared {
                return Err(RevisionCoreV1Error::stable(
                    "invalid_source_binding_role",
                    format!(
                        "Cleanup Input {} has role {:?} relative to current Revision",
                        reference.input_id.as_str(),
                        reference.role
                    ),
                ));
            }
        }
    }

    Ok(RevisionCoreV1 {
        format_version: 1,
        inputs: input.inputs,
        actions: input.actions,
        snapshot: input.snapshot,
        migrations: input.migrations,
        cleanup: input.cleanup,
    })
}

pub(crate) fn project_runtime_content_closure_v1(
    mut input: RuntimeContentProjectionInputV1,
) -> Result<RuntimeContentClosureIdentityV1, RevisionCoreV1Error> {
    sort_unique(&mut input.files, |value| value.id.as_str(), "RuntimeFile")?;
    let mut paths = BTreeSet::new();
    for file in &input.files {
        if !paths.insert(file.path.as_str()) {
            return Err(RevisionCoreV1Error::internal(
                "duplicate_semantic_key",
                format!("duplicate runtime path {}", file.path.as_str()),
            ));
        }
    }
    Ok(RuntimeContentClosureIdentityV1 { files: input.files })
}

pub(crate) fn validate_revision_content_v1(
    core: RevisionCoreV1,
    runtime_content: RuntimeContentClosureIdentityV1,
) -> Result<ValidatedRevisionContentV1, RevisionCoreV1Error> {
    let files: BTreeMap<_, _> = runtime_content
        .files
        .iter()
        .map(|file| (&file.id, file))
        .collect();
    visit_hooks(&core, &mut |hook| match &hook.launch {
        HookLaunchV1::Direct { executable } => {
            let file = files.get(executable).ok_or_else(|| {
                RevisionCoreV1Error::internal(
                    "invalid_reference",
                    format!("unknown executable ContentId {}", executable.as_str()),
                )
            })?;
            if !file.executable {
                return Err(RevisionCoreV1Error::stable(
                    "invalid_hook_launch",
                    format!("direct ContentId {} is not executable", executable.as_str()),
                ));
            }
            Ok(())
        }
        HookLaunchV1::Interpreter { script, .. } | HookLaunchV1::ShellLoader { script, .. } => {
            if !files.contains_key(script) {
                return Err(RevisionCoreV1Error::internal(
                    "invalid_reference",
                    format!("unknown script ContentId {}", script.as_str()),
                ));
            }
            Ok(())
        }
    })?;

    Ok(ValidatedRevisionContentV1 {
        core,
        runtime_content,
    })
}

pub(crate) fn validate_revision_sources_v1(
    core: &RevisionCoreV1,
    source_input_ids: &BTreeMap<Sha256Digest, BTreeSet<InputIdentity>>,
) -> Result<RelationalValidationV1, RevisionCoreV1Error> {
    if core.migrations.is_empty() {
        return Ok(RelationalValidationV1::Valid);
    }
    let mut evaluated_all = true;
    for migration in &core.migrations {
        let Some(source_ids) = source_input_ids.get(&migration.source_revision_digest) else {
            evaluated_all = false;
            continue;
        };
        for reference in migration.requires_source.iter().chain(
            migration
                .transitions
                .iter()
                .map(MigrationTransitionV1::source),
        ) {
            let declared = source_ids.contains(&reference.input_id);
            if (reference.role == InputBindingRoleV1::Active) != declared {
                return Err(RevisionCoreV1Error::stable(
                    "invalid_source_binding_role",
                    format!(
                        "source Input {} has role {:?} but declared={declared}",
                        reference.input_id.as_str(),
                        reference.role
                    ),
                ));
            }
        }
    }
    Ok(if evaluated_all {
        RelationalValidationV1::Valid
    } else {
        RelationalValidationV1::NotEvaluated
    })
}

fn normalize_parameters(parameters: &mut [ParameterV1]) -> Result<(), RevisionCoreV1Error> {
    for parameter in parameters.iter() {
        let matches = matches!(
            (parameter.parameter_type, parameter.default.as_ref()),
            (_, None)
                | (
                    ParameterTypeV1::Integer,
                    Some(ParameterDefaultV1::Integer(_))
                )
                | (ParameterTypeV1::Float, Some(ParameterDefaultV1::Float(_)))
                | (
                    ParameterTypeV1::Boolean,
                    Some(ParameterDefaultV1::Boolean(_))
                )
                | (ParameterTypeV1::String, Some(ParameterDefaultV1::String(_)))
        );
        if !matches {
            return Err(RevisionCoreV1Error::internal(
                "invalid_type",
                "parameter default does not match parameter type",
            ));
        }
    }
    sort_unique(parameters, |value| value.id.as_str(), "Parameter")
}

fn normalize_migration(
    migration: &mut MigrationV1,
    target_inputs: &BTreeSet<InputIdentity>,
) -> Result<(), RevisionCoreV1Error> {
    normalize_binding_refs(&mut migration.requires_source)?;
    normalize_target_ids(&mut migration.requires_target, target_inputs)?;
    normalize_target_ids(&mut migration.produces_target, target_inputs)?;
    if migration
        .requires_target
        .iter()
        .any(|id| migration.produces_target.contains(id))
    {
        return Err(RevisionCoreV1Error::internal(
            "invalid_transition",
            "requires_target and produces_target overlap",
        ));
    }

    let mut source_roles = BTreeMap::new();
    for reference in &migration.requires_source {
        record_source_role(reference, &mut source_roles)?;
    }
    let mut source_dispositions = BTreeSet::new();
    let mut target_writers: BTreeSet<_> = migration.produces_target.iter().cloned().collect();
    for transition in &migration.transitions {
        record_source_role(transition.source(), &mut source_roles)?;
        if !source_dispositions.insert(transition.source().clone()) {
            return Err(RevisionCoreV1Error::internal(
                "invalid_transition",
                "source binding has more than one disposition",
            ));
        }
        if let Some(target) = transition.target() {
            if !target_inputs.contains(target) {
                return Err(RevisionCoreV1Error::internal(
                    "invalid_reference",
                    format!("target Input {} is not declared", target.as_str()),
                ));
            }
            if !target_writers.insert(target.clone()) {
                return Err(RevisionCoreV1Error::internal(
                    "invalid_transition",
                    "target Input has multiple writers",
                ));
            }
        }
    }
    migration.transitions.sort_by(|left, right| {
        let left_source = left.source();
        let right_source = right.source();
        (
            left_source.role,
            &left_source.input_id,
            left.kind_key(),
            left.target().map(InputIdentity::as_str).unwrap_or(""),
        )
            .cmp(&(
                right_source.role,
                &right_source.input_id,
                right.kind_key(),
                right.target().map(InputIdentity::as_str).unwrap_or(""),
            ))
    });
    Ok(())
}

fn normalize_binding_refs(references: &mut [InputBindingRefV1]) -> Result<(), RevisionCoreV1Error> {
    references.sort();
    reject_adjacent_duplicates(
        references,
        |left, right| left == right,
        "source binding reference",
    )
}

fn normalize_target_ids(
    ids: &mut [InputIdentity],
    target_inputs: &BTreeSet<InputIdentity>,
) -> Result<(), RevisionCoreV1Error> {
    for id in ids.iter() {
        if !target_inputs.contains(id) {
            return Err(RevisionCoreV1Error::internal(
                "invalid_reference",
                format!("target Input {} is not declared", id.as_str()),
            ));
        }
    }
    ids.sort();
    reject_adjacent_duplicates(ids, |left, right| left == right, "target Input")
}

fn record_source_role(
    reference: &InputBindingRefV1,
    roles: &mut BTreeMap<InputIdentity, InputBindingRoleV1>,
) -> Result<(), RevisionCoreV1Error> {
    if let Some(previous) = roles.insert(reference.input_id.clone(), reference.role)
        && previous != reference.role
    {
        return Err(RevisionCoreV1Error::stable(
            "invalid_source_binding_role",
            format!(
                "source Input {} has conflicting roles",
                reference.input_id.as_str()
            ),
        ));
    }
    Ok(())
}

fn visit_hooks(
    core: &RevisionCoreV1,
    visitor: &mut impl FnMut(&HookV1) -> Result<(), RevisionCoreV1Error>,
) -> Result<(), RevisionCoreV1Error> {
    for action in &core.actions {
        visitor(&action.hook)?;
    }
    if let Some(snapshot) = &core.snapshot {
        if let Some(capture) = &snapshot.capture {
            visitor(&capture.hook)?;
        }
        if let Some(restore) = &snapshot.restore {
            visitor(&restore.hook)?;
        }
    }
    for migration in &core.migrations {
        if let Some(hook) = &migration.hook {
            visitor(hook)?;
        }
    }
    if let Some(cleanup) = &core.cleanup {
        visitor(&cleanup.hook)?;
    }
    Ok(())
}

fn sort_unique<T>(
    values: &mut [T],
    key: impl Fn(&T) -> &str,
    name: &str,
) -> Result<(), RevisionCoreV1Error> {
    values.sort_by(|left, right| key(left).cmp(key(right)));
    reject_adjacent_duplicates(values, |left, right| key(left) == key(right), name)
}

fn reject_adjacent_duplicates<T>(
    values: &[T],
    equal: impl Fn(&T, &T) -> bool,
    name: &str,
) -> Result<(), RevisionCoreV1Error> {
    if values.windows(2).any(|pair| equal(&pair[0], &pair[1])) {
        return Err(RevisionCoreV1Error::internal(
            "duplicate_semantic_key",
            format!("duplicate {name} key"),
        ));
    }
    Ok(())
}

fn validate_semantic_identifier(value: &str) -> Result<(), RevisionCoreV1Error> {
    if value.is_empty() || value.len() > 128 || !value.is_ascii() {
        return Err(RevisionCoreV1Error::internal(
            "invalid_identifier",
            format!("invalid semantic identifier {value:?}"),
        ));
    }
    let bytes = value.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return Err(RevisionCoreV1Error::internal(
            "invalid_identifier",
            format!("invalid semantic identifier {value:?}"),
        ));
    }
    let mut previous_separator = false;
    for &byte in &bytes[1..] {
        let separator = matches!(byte, b'.' | b'_' | b'-');
        if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || separator)
            || (separator && previous_separator)
        {
            return Err(RevisionCoreV1Error::internal(
                "invalid_identifier",
                format!("invalid semantic identifier {value:?}"),
            ));
        }
        previous_separator = separator;
    }
    if previous_separator {
        return Err(RevisionCoreV1Error::internal(
            "invalid_identifier",
            format!("invalid semantic identifier {value:?}"),
        ));
    }
    Ok(())
}

fn is_runtime_path(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 1024
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains('\\')
        || value.contains(':')
    {
        return false;
    }
    value.split('/').all(|segment| {
        segment != "."
            && segment != ".."
            && validate_semantic_identifier(segment).is_ok()
            && !is_windows_reserved(segment)
    })
}

pub(super) fn is_windows_reserved(value: &str) -> bool {
    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}
