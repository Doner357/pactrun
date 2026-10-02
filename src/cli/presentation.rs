//! Pactrun-owned presentation only. Payloads never pass through these renderers.
use super::*;
use serde::Serialize;

pub(super) const FORMAT_KIND: &str = "pactrun.cli";
pub(super) const FORMAT_VERSION: &str = crate::domain::VersionDomain::Machine.current_text();

pub(super) fn command_name(command: &Command) -> &'static str {
    use artifacts::ArtifactCommand as A;
    use catalog::CatalogCommand as C;
    use lifecycle::LifecycleCommand as L;
    use migrations::MigrationCommand as M;
    use retirements::RetirementCommand as R;
    use snapshots::SnapshotCommand as S;
    match command {
        Command::Help => "help",
        Command::Version => "version",
        Command::GeneratePackageId => "pack generate-id",
        Command::Install { .. } => "pack install",
        Command::ExportRevision { .. } => "revision export",
        Command::CreateInstance { .. } | Command::CreateAndRestore(_) => "instance create",
        Command::ListInstances => "instance list",
        Command::ShowInstance { .. } => "instance show",
        Command::ResolveManualRecovery { .. } => "instance resolve-manual-recovery",
        Command::ListInputs { .. } => "input list",
        Command::SetInput { .. } => "input set",
        Command::ExportInput { .. } => "input export",
        Command::DeleteInput { .. } => "input delete",
        Command::ListActions { .. } => "action list",
        Command::ShowAction { .. } => "action show",
        Command::Invoke { .. } => "invoke",
        Command::ShowRun { .. } => "run show",
        Command::ReconcileRuns => "run reconcile",
        Command::Artifact(A::Export { .. }) => "run artifact export",
        Command::Artifact(A::Delete { .. }) => "run artifact delete",
        Command::Lifecycle(L::Collect { .. }) => "storage gc",
        Command::Lifecycle(L::Revision(_)) => "revision delete",
        Command::Lifecycle(L::Run { .. }) => "run delete",
        Command::Migration(M::List { .. }) => "instance migration-paths",
        Command::Migration(M::Plan { .. }) => "instance migrate",
        Command::Retirement(R::Execute { mode, .. }) => match mode {
            crate::domain::DeletionMode::AbandonManagement => "instance abandon",
            _ => "instance delete",
        },
        Command::Retirement(R::Show(_)) => "instance deletion show",
        Command::Retirement(R::Confirm(_)) => "instance deletion confirm-complete",
        Command::Retirement(R::DetachedList { .. }) => "service-storage detached list",
        Command::Retirement(R::DetachedShow { .. }) => "service-storage detached show",
        Command::Retirement(R::Discard(_)) => "service-storage detached discard",
        Command::Snapshot(S::Delete(_)) => "snapshot delete",
        Command::Snapshot(S::Execute { operation, .. }) => match operation {
            snapshots::Operation::Capture => "snapshot capture",
            snapshots::Operation::Restore(_) => "snapshot restore",
        },
        Command::Snapshot(S::List { .. }) => "snapshot list",
        Command::Snapshot(S::Show(_)) => "snapshot show",
        Command::Snapshot(S::Verify(_)) => "snapshot verify",
        Command::Snapshot(S::Import(_)) => "snapshot import",
        Command::Snapshot(S::Export { .. }) => "snapshot export",
        Command::ServiceStorage(command) => command.presentation_name(),
        Command::Catalog(C::Revisions(_)) => "revision list",
        Command::Catalog(C::Revision(_, true)) => "revision metadata show",
        Command::Catalog(C::Revision(_, false)) => "revision show",
        Command::Catalog(C::History(_, true)) => "instance deletion list",
        Command::Catalog(C::History(_, false)) => "instance history list",
        Command::Catalog(C::Instance(_)) => "instance history show",
        Command::Catalog(C::Runs(..)) => "run list",
        Command::Catalog(C::Alias(_)) => "revision alias show",
        Command::Catalog(C::Local(_, catalog::LocalKind::Note)) => "revision note show",
        Command::Catalog(C::Local(_, catalog::LocalKind::Trust)) => "revision trust show",
        Command::Catalog(C::AliasMutation { desired, .. }) => {
            if *desired {
                "revision alias set"
            } else {
                "revision alias clear"
            }
        }
        Command::Catalog(C::Mutation { operation, .. }) => {
            use crate::domain::{CurrentState, RevisionMetadataMutation as Mutation};
            match operation {
                Mutation::CompareAndSetLocalAlias {
                    desired: CurrentState::Absent,
                    ..
                } => "revision alias clear",
                Mutation::CompareAndSetLocalAlias { .. } => "revision alias set",
                Mutation::CompareAndSetLocalNote {
                    desired: CurrentState::Absent,
                    ..
                } => "revision note clear",
                Mutation::CompareAndSetLocalNote { .. } => "revision note set",
                Mutation::CompareAndSetLocalTrust {
                    desired: CurrentState::Absent,
                    ..
                } => "revision trust clear",
                Mutation::CompareAndSetLocalTrust { .. } => "revision trust set",
                _ => unreachable!("CLI parser only constructs local metadata mutations"),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum Format {
    #[default]
    Human,
    Json,
    Jsonl,
}

/// Inspect only the prefix. Never scan operands (which may themselves be flags).
pub(super) fn recognize(args: &[OsString]) -> Format {
    if args.first().is_some_and(|arg| arg == "--format")
        && args.get(1).is_some_and(|arg| arg == "jsonl")
    {
        return Format::Jsonl;
    }
    if args.first().is_some_and(|arg| arg == "--format")
        && args.get(1).is_some_and(|arg| arg == "json")
    {
        Format::Json
    } else {
        Format::Human
    }
}

/// Preserve the payload channel even when a later option is invalid. Only the
/// fixed Input-export grammar is inspected; arbitrary operands are not formats.
pub(super) fn raw_input_request(args: &[OsString]) -> bool {
    let args = if args.first().is_some_and(|a| a == "--format") && args.len() >= 2 {
        &args[2..]
    } else {
        args
    };
    args.first().is_some_and(|a| a == "input")
        && args.get(1).is_some_and(|a| a == "export")
        && args.get(4..).is_some_and(|tail| {
            tail.windows(2)
                .any(|pair| pair[0] == "--output" && pair[1] == "-")
        })
}

pub(super) fn select(mut args: Vec<OsString>) -> Result<(Format, Vec<OsString>), CliError> {
    if !args.first().is_some_and(|arg| arg == "--format") {
        return Ok((Format::Human, args));
    }
    let format = match args.get(1).and_then(|arg| arg.to_str()) {
        Some("human") => Format::Human,
        Some("json") => Format::Json,
        Some("jsonl") => Format::Jsonl,
        _ => {
            return Err(CliError::usage(
                "--format requires human, json or jsonl before the command",
            ));
        }
    };
    args.drain(..2);
    if args.first().is_some_and(|arg| arg == "--format") {
        return Err(CliError::usage("duplicate --format selector"));
    }
    Ok((format, args))
}

#[derive(Clone, Copy, Debug, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(super) enum ErrorKind {
    Usage,
    Operation,
    Output,
    Startup,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct ErrorReference {
    pub(super) owner: String,
    pub(super) code: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct Error<'a> {
    kind: ErrorKind,
    message: &'a str,
    reference: &'a Option<ErrorReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostic: Option<&'a AcquisitionDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deletion_obligation: Option<&'a DeletionObligationDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retirement_reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct DeletionObligationDiagnostic {
    pub(super) instance_id: String,
    pub(super) run_id: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct AcquisitionDiagnostic {
    kind: &'static str,
    phase: &'static str,
    instance_id: String,
    target_revision: Revision,
    input_id: String,
    reason: &'static str,
    run_acceptance: &'static str,
}
impl From<&crate::application::migration_input::Failure> for AcquisitionDiagnostic {
    fn from(failure: &crate::application::migration_input::Failure) -> Self {
        Self {
            kind: "migration_input_acquisition",
            phase: failure.phase.code(),
            instance_id: failure.instance.to_string(),
            target_revision: (&failure.target).into(),
            input_id: failure.input.as_str().into(),
            reason: failure.reason.code(),
            run_acceptance: "not_accepted",
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Response<'a, T: Serialize> {
    format: &'static str,
    format_version: &'static str,
    command: Option<&'a str>,
    status: &'static str,
    result: Option<&'a T>,
    error: Option<Error<'a>>,
}

fn document<T: Serialize>(
    output: &mut dyn Write,
    response: &Response<'_, T>,
) -> Result<(), CliError> {
    // Finish serialization before the first write. Never append a second response
    // after a failed write or flush; the outer dispatcher checks ErrorKind::Output.
    let mut bytes = serde_json::to_vec(response)
        .map_err(|error| CliError::operation(format!("serialize CLI response: {error}")))?;
    bytes.push(b'\n');
    output.write_all(&bytes).map_err(output_error)?;
    output.flush().map_err(output_error)
}

pub(super) fn output_error(error: io::Error) -> CliError {
    let mut result = CliError::operation(error.to_string());
    result.kind = ErrorKind::Output;
    result
}

pub(super) fn render<T: Serialize>(
    format: Format,
    command: &str,
    result: &T,
    output: &mut dyn Write,
    human: impl FnOnce(&T, &mut dyn Write) -> Result<(), CliError>,
) -> Result<(), CliError> {
    match format {
        Format::Human => human(result, output),
        Format::Jsonl => {
            let mut writer = streaming::ResultWriter::new(output, Some(command));
            render(Format::Json, command, result, &mut writer, human)?;
            writer.finish().map_err(output_error)
        }
        Format::Json => document(
            output,
            &Response {
                format: FORMAT_KIND,
                format_version: FORMAT_VERSION,
                command: Some(command),
                status: "success",
                result: Some(result),
                error: None,
            },
        ),
    }
}

pub(super) fn failure<T: Serialize>(
    command: Option<&str>,
    partial: Option<&T>,
    error: &CliError,
    output: &mut dyn Write,
) -> Result<(), CliError> {
    document(
        output,
        &Response {
            format: FORMAT_KIND,
            format_version: FORMAT_VERSION,
            command,
            status: "failure",
            result: partial,
            error: Some(Error {
                kind: error.kind,
                message: &error.message,
                reference: &error.reference,
                diagnostic: error.details.diagnostic.as_ref(),
                deletion_obligation: error.details.deletion_obligation.as_ref(),
                retirement_reason: error.details.retirement_reason,
            }),
        },
    )
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Help<'a> {
    pub(super) usage: &'a str,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Version<'a> {
    pub(super) product_version: &'a str,
    pub(super) build_target: &'a str,
    pub(super) rustc: &'a str,
    pub(super) source_commit: Option<&'a str>,
    pub(super) source_manifest_sha256: Option<&'a str>,
    pub(super) supported_formats: std::collections::BTreeMap<&'a str, Vec<&'a str>>,
    pub(super) default_formats: std::collections::BTreeMap<&'a str, &'a str>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Package {
    pub(super) package_id: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct StateVersion {
    pub(super) state_version: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct InputExport {
    pub(super) input_id: String,
    pub(super) state_version: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(untagged)]
pub(super) enum PartialResult {
    IdentityCandidates(Box<short_ids::Candidates>),
    Publication(Box<Publication>),
    Collection(Box<Collection>),
    Run(KnownRun),
    Inspection(Box<execution_presentation::Inspection>),
    Observation(service_storage::Observation),
    CreatedRestore(Box<transport_presentation::CreatedRestore>),
    MigrationPaths(Box<migration_presentation::Paths>),
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Publication {
    pub(super) destination: NativePath,
    pub(super) destination_published: bool,
}

impl fmt::Debug for PartialResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PartialResult { typed facts withheld from Debug }")
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "encoding", rename_all = "snake_case")]
// The public path union is platform independent; each producer uses its native arm.
#[allow(dead_code)]
pub(super) enum NativePath {
    Utf8 { value: String },
    WindowsWide { units: Vec<u16> },
    UnixBytes { bytes: Vec<u8> },
}

impl From<&Path> for NativePath {
    fn from(path: &Path) -> Self {
        if let Some(value) = path.to_str() {
            return Self::Utf8 {
                value: value.into(),
            };
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            Self::WindowsWide {
                units: path.as_os_str().encode_wide().collect(),
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            Self::UnixBytes {
                bytes: path.as_os_str().as_bytes().to_vec(),
            }
        }
    }
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct KnownRun {
    pub(super) run_id: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Collection {
    pub(super) plan: bool,
    pub(super) candidates: String,
    pub(super) removed: String,
    pub(super) retained: String,
    pub(super) unsupported: String,
    pub(super) failed: String,
}

impl Collection {
    pub(super) fn new(plan: bool, report: &crate::domain::CollectionReport) -> Self {
        Self {
            plan,
            candidates: report.candidates.to_string(),
            removed: report.removed.to_string(),
            retained: report.retained.to_string(),
            unsupported: report.unsupported.to_string(),
            failed: report.failed.to_string(),
        }
    }

    pub(super) fn human(&self, out: &mut dyn Write) -> Result<(), CliError> {
        writeln!(
            out,
            "{}: candidates={}, removed={}, retained={}, unsupported={}, failed={}",
            if self.plan {
                "collection preview"
            } else {
                "collection"
            },
            self.candidates,
            self.removed,
            self.retained,
            self.unsupported,
            self.failed
        )
        .map_err(io_operation)
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Deletion {
    pub(super) outcome: &'static str,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct ArtifactResult {
    pub(super) run_id: String,
    pub(super) output_id: String,
    pub(super) outcome: &'static str,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Revision {
    package_id: String,
    content_digest: String,
}

impl From<&RevisionIdentity> for Revision {
    fn from(id: &RevisionIdentity) -> Self {
        Self {
            package_id: id.package_id.to_string(),
            content_digest: id.content_digest.to_string(),
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Input {
    input_id: String,
    role: &'static str,
    required: Option<bool>,
    present: bool,
    protection: &'static str,
}

impl From<&crate::domain::ManagedInputBindingView> for Input {
    fn from(binding: &crate::domain::ManagedInputBindingView) -> Self {
        let (role, required) = match binding.role {
            ManagedInputRole::Active { required } => ("active", Some(required)),
            ManagedInputRole::Retained => ("retained", None),
        };
        Self {
            input_id: binding.input_id.as_str().into(),
            role,
            required,
            present: binding.present,
            protection: match binding.protection {
                ManagedInputProtection::Normal => "normal",
                ManagedInputProtection::Secret => "secret",
            },
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Guard {
    trigger: &'static str,
    run_id: String,
    entered_at_unix_ms: String,
}

impl From<&crate::domain::RecoveryGuardView> for Guard {
    fn from(guard: &crate::domain::RecoveryGuardView) -> Self {
        Self {
            trigger: format_trigger(guard.trigger),
            run_id: guard.run.to_string(),
            entered_at_unix_ms: guard.entered_at_unix_ms.to_string(),
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Instance {
    instance_id: String,
    name: String,
    active_revision: Revision,
    state_version: String,
    required_inputs_satisfied: bool,
    recovery_guard: Option<Guard>,
    inputs: Vec<Input>,
}

impl From<&crate::domain::InstanceView> for Instance {
    fn from(view: &crate::domain::InstanceView) -> Self {
        Self {
            instance_id: view.id.to_string(),
            name: view.name.as_str().into(),
            active_revision: (&view.active_revision).into(),
            state_version: view.state_version.to_string(),
            required_inputs_satisfied: view.required_inputs_satisfied,
            recovery_guard: view.recovery_guard.as_ref().map(Into::into),
            inputs: view.bindings.iter().map(Into::into).collect(),
        }
    }
}

impl Instance {
    pub(super) fn human(&self, out: &mut dyn Write) -> Result<(), CliError> {
        writeln!(out, "instance_id: {}", self.instance_id).map_err(io_operation)?;
        if let Some(guard) = &self.recovery_guard {
            writeln!(
                out,
                "current_recovery_guard: {} run={} entered_at_unix_ms={}",
                guard.trigger, guard.run_id, guard.entered_at_unix_ms
            )
            .map_err(io_operation)?;
        } else {
            writeln!(out, "current_recovery_guard: none").map_err(io_operation)?;
        }
        writeln!(
            out,
            "name: {}\nrevision: exact:{}/{}\nstate_version: {}\nrequired_inputs_satisfied: {}",
            self.name,
            self.active_revision.package_id,
            self.active_revision.content_digest,
            self.state_version,
            self.required_inputs_satisfied
        )
        .map_err(io_operation)?;
        Inputs::human_items(&self.inputs, out)
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Inputs {
    pub(super) instance_id: String,
    pub(super) state_version: String,
    pub(super) items: Vec<Input>,
}

impl Inputs {
    pub(super) fn from_view(view: &crate::domain::InstanceView) -> Self {
        Self {
            instance_id: view.id.to_string(),
            state_version: view.state_version.to_string(),
            items: view.bindings.iter().map(Into::into).collect(),
        }
    }

    fn human_items(items: &[Input], out: &mut dyn Write) -> Result<(), CliError> {
        if items.is_empty() {
            return writeln!(out, "inputs: none").map_err(io_operation);
        }
        for input in items {
            let role = match input.required {
                Some(true) => "active_required",
                Some(false) => "active_optional",
                None => "retained",
            };
            writeln!(
                out,
                "input: {}\t{}\t{}\t{}",
                input.input_id,
                role,
                if input.present { "present" } else { "absent" },
                input.protection
            )
            .map_err(io_operation)?;
        }
        Ok(())
    }

    pub(super) fn human(&self, out: &mut dyn Write) -> Result<(), CliError> {
        Self::human_items(&self.items, out)
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct InstanceSummary {
    instance_id: String,
    name: String,
    active_revision: Revision,
    state_version: String,
    required_inputs_satisfied: bool,
    recovery_guard: Option<Guard>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Instances {
    pub(super) items: Vec<InstanceSummary>,
}

impl Instances {
    pub(super) fn from_views(views: &[crate::domain::InstanceSummary]) -> Self {
        Self {
            items: views
                .iter()
                .map(|view| InstanceSummary {
                    instance_id: view.id.to_string(),
                    name: view.name.as_str().into(),
                    active_revision: (&view.active_revision).into(),
                    state_version: view.state_version.to_string(),
                    required_inputs_satisfied: view.required_inputs_satisfied,
                    recovery_guard: view.recovery_guard.as_ref().map(Into::into),
                })
                .collect(),
        }
    }

    pub(super) fn human(&self, out: &mut dyn Write) -> Result<(), CliError> {
        if self.items.is_empty() {
            return writeln!(out, "No managed Instances.").map_err(io_operation);
        }
        writeln!(out, "NAME  INPUTS  RECOVERY").map_err(io_operation)?;
        for item in &self.items {
            writeln!(
                out,
                "{}  {}  {}",
                item.name,
                if item.required_inputs_satisfied {
                    "complete"
                } else {
                    "missing required values"
                },
                if item.recovery_guard.is_some() {
                    "required"
                } else {
                    "none"
                }
            )
            .map_err(io_operation)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test-ID: PR-TEST-0568
    // Verifies: PR-REQ-0359, PR-REQ-0360
    #[test]
    fn json_native_path_projection_is_lossless_without_lossy_unicode() {
        let ordinary = serde_json::to_value(NativePath::from(Path::new("hello"))).unwrap();
        assert_eq!(ordinary["encoding"], "utf8");
        assert_eq!(ordinary["value"], "hello");
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            let path = PathBuf::from(OsString::from_wide(&[0x61, 0xd800, 0x62]));
            let encoded = serde_json::to_value(NativePath::from(path.as_path())).unwrap();
            assert_eq!(encoded["encoding"], "windows_wide");
            assert_eq!(encoded["units"], serde_json::json!([0x61, 0xd800, 0x62]));
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let path = PathBuf::from(OsString::from_vec(vec![0x61, 0xff, 0x62]));
            let encoded = serde_json::to_value(NativePath::from(path.as_path())).unwrap();
            assert_eq!(encoded["encoding"], "unix_bytes");
            assert_eq!(encoded["bytes"], serde_json::json!([0x61, 0xff, 0x62]));
        }
    }

    // Test-ID: PR-TEST-0553
    // Verifies: PR-REQ-0358
    #[test]
    fn selection_only_consumes_the_prefix() {
        let args = [
            "--format", "json", "input", "set", "x", "y", "--file", "--format",
        ]
        .map(OsString::from)
        .to_vec();
        let (format, args) = select(args).unwrap();
        assert_eq!(format, Format::Json);
        assert_eq!(args.last().unwrap(), "--format");
        assert!(select(["--format", "xml"].map(OsString::from).to_vec()).is_err());
        assert!(
            select(
                ["--format", "json", "--format", "human"]
                    .map(OsString::from)
                    .to_vec()
            )
            .is_err()
        );
        assert_eq!(
            recognize(&["--format", "json", "--bogus"].map(OsString::from)),
            Format::Json
        );
        assert_eq!(
            recognize(&["input", "--format", "json"].map(OsString::from)),
            Format::Human
        );
    }

    // Test-ID: PR-TEST-0554
    // Verifies: PR-REQ-0359, PR-REQ-0083
    #[test]
    fn versioned_typed_response_and_partial_failure() {
        let mut output = Vec::new();
        let value = StateVersion {
            state_version: "18446744073709551615".into(),
        };
        render(Format::Json, "input set", &value, &mut output, |_, _| {
            panic!("human renderer must not run")
        })
        .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(json["format"], FORMAT_KIND);
        assert_eq!(json["result"]["state_version"], value.state_version);
        assert!(json["error"].is_null());
        assert_eq!(output.last(), Some(&b'\n'));
        output.clear();
        failure(
            Some("input set"),
            Some(&value),
            &CliError::operation("output is not proof of rollback"),
            &mut output,
        )
        .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(json["status"], "failure");
        assert_eq!(json["result"]["state_version"], value.state_version);
        assert!(json["error"]["reference"].is_null());
    }

    // Test-ID: PR-TEST-0555
    // Verifies: PR-REQ-0360
    #[test]
    fn response_write_failure_is_distinct_and_never_retried() {
        struct Broken {
            attempts: usize,
        }
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                self.attempts += 1;
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut output = Broken { attempts: 0 };
        let error = render(
            Format::Json,
            "version",
            &Version {
                product_version: "test",
                build_target: "test",
                rustc: "test",
                source_commit: None,
                source_manifest_sha256: None,
                supported_formats: std::collections::BTreeMap::new(),
                default_formats: std::collections::BTreeMap::new(),
            },
            &mut output,
            |_, _| unreachable!(),
        )
        .unwrap_err();
        assert!(matches!(error.kind, ErrorKind::Output));
        assert_eq!(output.attempts, 1);
    }
}
