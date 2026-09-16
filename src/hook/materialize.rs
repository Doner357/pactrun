use std::{
    collections::BTreeSet,
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde_json::{Number, Value, json};

use crate::{
    domain::{
        CompiledHookLaunch, InvocationParameterValue, ManagedOutputIdentity, OperationAccessV1,
        RuntimeFileV1, TerminalContractV1,
    },
    executor::AdmittedExecution,
    managed_data::{ExecutionDirectory, StagingError, StagingSession},
    persistence::{PactrunPersistence, PersistenceError},
};

use super::LiveOutputSlot;

#[path = "migration_materialize.rs"]
mod migration;

#[path = "cleanup_materialize.rs"]
mod cleanup;

pub(super) struct MaterializedExecution {
    directory: ExecutionDirectory,
    outputs: Vec<MaterializedOutputSlot>,
    program: PathBuf,
    arguments: Vec<String>,
    session_id: String,
    session: Value,
    operation: super::protocol::SessionOperation,
    terminal: TerminalContractV1,
    capture: Option<super::capture::CapturePreparation>,
    migration_outputs: Vec<super::MigrationOutputSlot>,
    v2_state: Option<super::protocol::v2::State>,
    v2_transport: Option<super::protocol::v2::PreparedTransport>,
    _service_access: Option<super::service_storage::PreparedServiceAccess>,
}

pub(super) type MaterializedAction = MaterializedExecution;

impl fmt::Debug for MaterializedAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MaterializedAction")
            .field("execution_root", &"<owner-ephemeral>")
            .field("outputs", &self.outputs.len())
            .field("session", &"<redacted>")
            .finish()
    }
}

#[derive(Debug)]
struct MaterializedOutputSlot {
    output: ManagedOutputIdentity,
    handle: String,
    path: PathBuf,
}

impl MaterializedAction {
    pub(super) fn create_restore(
        p: &PactrunPersistence,
        staging: &StagingSession,
        run: crate::domain::RunId,
        plan: &crate::domain::SnapshotExecutionPlan,
    ) -> Result<Self, MaterializationError> {
        use crate::domain::{ManagedRunIdentity, SnapshotBindingRole, SnapshotBindingState};
        let directory = staging.create_execution_directory(run)?;
        let result = (|| {
            let manifest = p.admitted_restore_manifest(run)?;
            if !matches!(plan.operation(), ManagedRunIdentity::Restore {snapshot,revision} if *snapshot == manifest.snapshot_id() && revision == manifest.producer())
            {
                return Err(MaterializationError::Persistence(
                    PersistenceError::CorruptSnapshot(
                        "Restore Plan differs from admitted Snapshot",
                    ),
                ));
            }
            let runtime = directory.create_directory(Path::new("runtime"))?;
            let workspace = directory.create_directory(Path::new("workspace"))?;
            directory.create_directory(Path::new("bindings"))?;
            let content = directory.create_directory(Path::new("snapshot-content"))?;
            for file in plan.runtime_content() {
                materialize_runtime_file(p, run, &directory, file)?;
            }
            let mut bindings = Vec::new();
            for binding in manifest.managed_bindings() {
                if binding.role != SnapshotBindingRole::Active {
                    continue;
                }
                let SnapshotBindingState::Bound(digest) = &binding.state else {
                    continue;
                };
                let path = materialize_restore_blob(
                    p,
                    run,
                    &directory,
                    &Path::new("bindings").join(binding.input_id.as_str()),
                    digest,
                )?;
                bindings.push(json!({"handle":random_handle()?,"input_id":binding.input_id.as_str(),"role":"active","readonly_path":host_path(&path)?}));
            }
            let mut descriptors = Vec::new();
            for (index, descriptor) in manifest.service_content().iter().enumerate() {
                let relative = format!("objects/item-{index}");
                materialize_restore_blob(
                    p,
                    run,
                    &directory,
                    &Path::new("snapshot-content").join(&relative),
                    &descriptor.blob_digest,
                )?;
                descriptors.push(json!({"role":descriptor.role.as_str(),"path":descriptor.path.as_str(),"blob_digest":descriptor.blob_digest.as_str(),"materialized_path":relative}));
            }
            let (program, mut arguments) = launch_command(plan.launch(), &runtime);
            arguments.extend(plan.hook().args.iter().cloned());
            let session_id = random_handle()?;
            let parameters = plan
                .parameters()
                .iter()
                .map(|p| {
                    Ok(json!({"parameter_id":p.id.as_str(),"value":parameter_value(p.value())?}))
                })
                .collect::<Result<Vec<_>, MaterializationError>>()?;
            let session = json!({"type":"session_start","protocol_version":1,"session_id":session_id,"run_id":run.to_string(),"revision":{"package_id":manifest.producer().package_id.to_string(),"revision_content_digest":manifest.producer().content_digest.to_string()},"parameters":parameters,"workspace":{"handle":random_handle()?,"root_path":host_path(&workspace)?},"io":{"terminal":terminal_name(plan.hook().io.terminal)},"operation":{"kind":"snapshot_restore","snapshot_id":manifest.snapshot_id().to_string(),"bindings":bindings,"snapshot_content":{"handle":random_handle()?,"readonly_root_path":host_path(&content)?,"logical_descriptors":descriptors}}});
            let mut session = session;
            session["protocol_version"] = json!(plan.hook().protocol_version.get());
            super::protocol::validate_session_frame(&session)?;
            Ok((program, arguments, session_id, session))
        })();
        match result {
            Ok((program, arguments, session_id, session)) => Self {
                directory,
                outputs: Vec::new(),
                program,
                arguments,
                session_id,
                session,
                operation: super::protocol::SessionOperation::Restore,
                v2_state: None,
                v2_transport: None,
                _service_access: None,
                terminal: plan.hook().io.terminal,
                capture: None,
                migration_outputs: Vec::new(),
            }
            .with_service(
                p,
                staging,
                run,
                plan.hook().protocol_version.get(),
                plan.service_bindings(),
            ),
            Err(error) => {
                let _ = directory.cleanup();
                Err(error)
            }
        }
    }
    pub(super) fn create(
        persistence: &PactrunPersistence,
        staging: &StagingSession,
        admitted: &AdmittedExecution,
    ) -> Result<Self, MaterializationError> {
        let directory = staging.create_execution_directory(admitted.run())?;
        let runtime = directory.create_directory(Path::new("runtime"))?;
        let workspace = directory.create_directory(Path::new("workspace"))?;
        directory.create_directory(Path::new("bindings"))?;
        directory.create_directory(Path::new("outputs"))?;
        for file in admitted.plan().runtime_content() {
            materialize_runtime_file(persistence, admitted.run(), &directory, file)?;
        }

        let mut binding_authorities = Vec::new();
        let mut active_bindings = admitted.plan().active_bindings().to_vec();
        active_bindings.sort_by(|left, right| left.input.cmp(&right.input));
        for binding in active_bindings {
            let relative = Path::new("bindings").join(binding.input.as_str());
            let (path, mut destination) = directory.create_file(&relative)?;
            persistence.stream_admitted_payload(
                admitted.run(),
                &binding.input,
                &mut destination,
            )?;
            destination.flush()?;
            drop(destination);
            let mut permissions = fs::metadata(&path)?.permissions();
            permissions.set_readonly(true);
            fs::set_permissions(&path, permissions)?;
            binding_authorities.push(json!({
                "handle": random_handle()?,
                "input_id": binding.input.as_str(),
                "role": "active",
                "readonly_path": host_path(&path)?,
            }));
        }

        let mut output_ids = admitted.plan().outputs().to_vec();
        output_ids.sort();
        let mut outputs = Vec::new();
        let mut output_authorities = Vec::new();
        for output in output_ids {
            let relative = Path::new("outputs").join(output.as_str());
            let (path, file) = directory.create_file(&relative)?;
            drop(file);
            let handle = random_handle()?;
            output_authorities.push(json!({
                "handle": handle,
                "output_id": output.as_str(),
                "staged_path": host_path(&path)?,
            }));
            outputs.push(MaterializedOutputSlot {
                output,
                handle,
                path,
            });
        }

        let (program, mut arguments) = launch_command(admitted.plan().launch(), &runtime);
        arguments.extend(admitted.plan().hook_args().iter().cloned());
        let session_id = random_handle()?;
        let mut parameters = admitted
            .plan()
            .parameters()
            .iter()
            .map(|parameter| {
                Ok(json!({
                    "parameter_id": parameter.id.as_str(),
                    "value": parameter_value(parameter.value())?,
                }))
            })
            .collect::<Result<Vec<_>, MaterializationError>>()?;
        parameters.sort_by(|left, right| {
            left["parameter_id"]
                .as_str()
                .cmp(&right["parameter_id"].as_str())
        });
        let session = json!({
            "type": "session_start",
            "protocol_version": admitted.plan().protocol_version().get(),
            "session_id": session_id,
            "run_id": admitted.run().to_string(),
            "revision": {
                "package_id": admitted.plan().active_revision().package_id.to_string(),
                "revision_content_digest": admitted.plan().active_revision().content_digest.to_string(),
            },
            "parameters": parameters,
            "workspace": {
                "handle": random_handle()?,
                "root_path": host_path(&workspace)?,
            },
            "io": {
                "terminal": terminal_name(admitted.plan().terminal()),
            },
            "operation": {
                "kind": "action",
                "action_id": admitted.plan().action().as_str(),
                "access": access_name(admitted.plan().access()),
                "bindings": binding_authorities,
                "outputs": output_authorities,
            },
        });
        Self {
            directory,
            outputs,
            program,
            arguments,
            session_id,
            session,
            operation: super::protocol::SessionOperation::Action,
            v2_state: None,
            v2_transport: None,
            _service_access: None,
            terminal: admitted.plan().terminal(),
            capture: None,
            migration_outputs: Vec::new(),
        }
        .with_service(
            persistence,
            staging,
            admitted.run(),
            admitted.plan().protocol_version().get(),
            admitted.plan().service_bindings(),
        )
    }

    pub(super) fn create_capture(
        p: &PactrunPersistence,
        staging: &StagingSession,
        run: crate::domain::RunId,
        plan: &crate::domain::SnapshotExecutionPlan,
    ) -> Result<Self, MaterializationError> {
        let directory = staging.create_execution_directory(run)?;
        let result = (|| {
            let runtime = directory.create_directory(Path::new("runtime"))?;
            let workspace = directory.create_directory(Path::new("workspace"))?;
            directory.create_directory(Path::new("bindings"))?;
            let candidate = directory.create_directory(Path::new("candidate"))?;
            let preparation =
                super::capture::CapturePreparation::new(p, staging, run, plan, &candidate)
                    .map_err(MaterializationError::Capture)?;
            for file in plan.runtime_content() {
                materialize_runtime_file(p, run, &directory, file)?;
            }
            let mut bindings = Vec::new();
            for binding in preparation.active() {
                let (path, mut file) =
                    directory.create_file(&Path::new("bindings").join(binding.input.as_str()))?;
                preparation
                    .copy_binding(binding, &mut file)
                    .map_err(MaterializationError::Capture)?;
                file.flush()?;
                drop(file);
                let mut permissions = fs::metadata(&path)?.permissions();
                permissions.set_readonly(true);
                fs::set_permissions(&path, permissions)?;
                bindings.push(json!({"handle":random_handle()?,"input_id":binding.input.as_str(),"role":"active","readonly_path":host_path(&path)?}));
            }
            let (program, mut arguments) = launch_command(plan.launch(), &runtime);
            arguments.extend(plan.hook().args.iter().cloned());
            let session_id = random_handle()?;
            let parameters = plan
                .parameters()
                .iter()
                .map(|p| {
                    Ok(json!({"parameter_id":p.id.as_str(),"value":parameter_value(p.value())?}))
                })
                .collect::<Result<Vec<_>, MaterializationError>>()?;
            let session = json!({"type":"session_start","protocol_version":1,"session_id":session_id,"run_id":run.to_string(),
                "revision":{"package_id":plan.operation().revision().package_id.to_string(),"revision_content_digest":plan.operation().revision().content_digest.to_string()},
                "parameters":parameters,"workspace":{"handle":random_handle()?,"root_path":host_path(&workspace)?},"io":{"terminal":terminal_name(plan.hook().io.terminal)},
                "operation":{"kind":"snapshot_capture","access":access_name(plan.access()),"bindings":bindings,"candidate":{"handle":random_handle()?,"root_path":host_path(&candidate)?}}});
            let mut session = session;
            session["protocol_version"] = json!(plan.hook().protocol_version.get());
            super::protocol::validate_session_frame(&session)?;
            Ok((preparation, program, arguments, session_id, session))
        })();
        match result {
            Ok((capture, program, arguments, session_id, session)) => Self {
                directory,
                outputs: Vec::new(),
                program,
                arguments,
                session_id,
                session,
                operation: super::protocol::SessionOperation::Capture,
                v2_state: None,
                v2_transport: None,
                _service_access: None,
                terminal: plan.hook().io.terminal,
                capture: Some(capture),
                migration_outputs: Vec::new(),
            }
            .with_service(
                p,
                staging,
                run,
                plan.hook().protocol_version.get(),
                plan.service_bindings(),
            ),
            Err(error) => {
                let _ = directory.cleanup();
                Err(error)
            }
        }
    }
    pub(super) fn operation(&self) -> super::protocol::SessionOperation {
        self.operation
    }
    pub(super) fn take_v2_state(&mut self) -> Option<super::protocol::v2::State> {
        self.v2_state.take()
    }
    pub(super) fn take_v2_transport(&mut self) -> Option<super::protocol::v2::PreparedTransport> {
        self.v2_transport.take()
    }
    fn with_service(
        self,
        p: &PactrunPersistence,
        staging: &StagingSession,
        run: crate::domain::RunId,
        version: i64,
        bindings: &crate::domain::ServiceHookBindings,
    ) -> Result<Self, MaterializationError> {
        self.with_service_target(p, staging, run, version, bindings, false)
    }
    fn with_service_target(
        mut self,
        p: &PactrunPersistence,
        staging: &StagingSession,
        run: crate::domain::RunId,
        version: i64,
        bindings: &crate::domain::ServiceHookBindings,
        transform: bool,
    ) -> Result<Self, MaterializationError> {
        let result = (|| {
            if version == 1 {
                if !bindings.grants.is_empty() || transform {
                    return Err(MaterializationError::InvalidParameter);
                }
                // Core V2 create predicates are evaluated by Pactrun, even when
                // the edge's Hook uses an unchanged V1 Session with no grants.
                self._service_access = Some(
                    super::service_storage::prepare(p, run, &staging.owner(), bindings)
                        .map_err(MaterializationError::Service)?,
                );
                return Ok(());
            }
            if version != 2 {
                return Err(MaterializationError::InvalidParameter);
            }
            let mut service = super::service_storage::prepare(p, run, &staging.owner(), bindings)
                .map_err(MaterializationError::Service)?;
            let prepared = super::protocol::v2::PreparedSession::new(
                self.session.clone(),
                self.operation,
                &service.declarations(),
                service.take_authorities(),
                transform,
                if transform {
                    Some(random_handle()?)
                } else {
                    None
                },
            )
            .map_err(|_| MaterializationError::InvalidParameter)?;
            let (transport, state) = prepared.into_parts();
            self.v2_state = Some(state);
            self.v2_transport = Some(transport);
            self._service_access = Some(service);
            Ok(())
        })();
        if let Err(error) = result {
            let _ = self.directory.cleanup();
            return Err(error);
        }
        Ok(self)
    }
    pub(super) fn cleanup_unlaunched(&self) {
        let _ = self.directory.cleanup();
    }
    pub(super) fn terminal(&self) -> TerminalContractV1 {
        self.terminal
    }
    pub(super) fn take_capture(&mut self) -> Option<super::capture::CapturePreparation> {
        self.capture.take()
    }
    pub(super) fn take_migration_outputs(&mut self) -> Vec<super::MigrationOutputSlot> {
        std::mem::take(&mut self.migration_outputs)
    }

    pub(super) fn program(&self) -> &Path {
        &self.program
    }

    pub(super) fn arguments(&self) -> &[String] {
        &self.arguments
    }

    pub(super) fn session(&self) -> &Value {
        &self.session
    }

    pub(super) fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(super) fn execution_root(&self) -> &Path {
        self.directory.root()
    }

    pub(super) fn output_handle_set(&self) -> BTreeSet<String> {
        self.outputs
            .iter()
            .map(|slot| slot.handle.clone())
            .chain(
                self.migration_outputs
                    .iter()
                    .map(|slot| slot.handle.clone()),
            )
            .collect()
    }

    pub(super) fn submitted_outputs(&self, handles: &[String]) -> Vec<ManagedOutputIdentity> {
        let selected = handles.iter().collect::<BTreeSet<_>>();
        let mut outputs = self
            .outputs
            .iter()
            .filter(|slot| selected.contains(&slot.handle))
            .map(|slot| slot.output.clone())
            .collect::<Vec<_>>();
        outputs.sort();
        outputs
    }

    pub(super) fn into_execution(self) -> (ExecutionDirectory, Vec<LiveOutputSlot>) {
        let outputs = self
            .outputs
            .into_iter()
            .map(|slot| LiveOutputSlot {
                output: slot.output,
                handle: slot.handle,
                path: slot.path,
            })
            .collect();
        (self.directory, outputs)
    }

    pub(super) fn into_output_slots(self) -> Vec<LiveOutputSlot> {
        self.outputs
            .into_iter()
            .map(|slot| LiveOutputSlot {
                output: slot.output,
                handle: slot.handle,
                path: slot.path,
            })
            .collect()
    }
}

fn materialize_restore_blob(
    p: &PactrunPersistence,
    run: crate::domain::RunId,
    directory: &ExecutionDirectory,
    relative: &Path,
    digest: &crate::domain::Sha256Digest,
) -> Result<PathBuf, MaterializationError> {
    let (path, mut file) = directory.create_file(relative)?;
    p.copy_admitted_restore_blob(run, digest, &mut file)?;
    file.flush()?;
    drop(file);
    let mut permissions = fs::metadata(&path)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions)?;
    Ok(path)
}

#[derive(Debug)]
pub(super) enum MaterializationError {
    Service(super::service_storage::NativeServiceError),
    Capture(super::capture::CaptureError),
    Io(io::Error),
    Staging(StagingError),
    Persistence(PersistenceError),
    InvalidParameter,
    InvalidHostPath,
    Random(getrandom::Error),
}

impl From<io::Error> for MaterializationError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<StagingError> for MaterializationError {
    fn from(error: StagingError) -> Self {
        Self::Staging(error)
    }
}

impl From<PersistenceError> for MaterializationError {
    fn from(error: PersistenceError) -> Self {
        Self::Persistence(error)
    }
}

fn materialize_runtime_file(
    persistence: &PactrunPersistence,
    run: crate::domain::RunId,
    directory: &ExecutionDirectory,
    file: &RuntimeFileV1,
) -> Result<(), MaterializationError> {
    let relative = Path::new("runtime").join(runtime_relative_path(file));
    let (_path, mut destination) = directory.create_file(&relative)?;
    let mut source = persistence.open_admitted_runtime_blob(run, file)?;
    io::copy(&mut source, &mut destination)?;
    destination.flush()?;
    drop(destination);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            &_path,
            fs::Permissions::from_mode(if file.executable { 0o700 } else { 0o600 }),
        )?;
    }
    Ok(())
}

fn runtime_relative_path(file: &RuntimeFileV1) -> PathBuf {
    file.path.as_str().split('/').collect()
}

fn launch_command(launch: &CompiledHookLaunch, runtime: &Path) -> (PathBuf, Vec<String>) {
    match launch {
        CompiledHookLaunch::Direct { executable } => {
            (runtime.join(runtime_relative_path(executable)), Vec::new())
        }
        CompiledHookLaunch::Interpreter {
            launcher,
            interpreter_args,
            script,
        } => {
            let mut arguments = interpreter_args.clone();
            arguments.push(
                runtime
                    .join(runtime_relative_path(script))
                    .to_string_lossy()
                    .into_owned(),
            );
            (launcher.resolved_absolute_path.clone(), arguments)
        }
    }
}

fn parameter_value(value: &InvocationParameterValue) -> Result<Value, MaterializationError> {
    match value {
        InvocationParameterValue::Integer(value) => Ok(Value::Number(Number::from(value.get()))),
        InvocationParameterValue::Float(value) => Number::from_f64(value.get())
            .map(Value::Number)
            .ok_or(MaterializationError::InvalidParameter),
        InvocationParameterValue::Boolean(value) => Ok(Value::Bool(*value)),
        InvocationParameterValue::String(value) => Ok(Value::String(value.clone())),
    }
}

/// `HostNativeAbsolutePath` must be valid Unicode; a staging root that is not
/// representable cannot be granted as Session authority.
fn host_path(path: &Path) -> Result<&str, MaterializationError> {
    path.to_str().ok_or(MaterializationError::InvalidHostPath)
}

fn random_handle() -> Result<String, MaterializationError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(MaterializationError::Random)?;
    Ok(hex::encode(bytes))
}

fn terminal_name(terminal: TerminalContractV1) -> &'static str {
    match terminal {
        TerminalContractV1::None => "none",
        TerminalContractV1::Output => "output",
        TerminalContractV1::Interactive => "interactive",
    }
}

fn access_name(access: OperationAccessV1) -> &'static str {
    match access {
        OperationAccessV1::Observe => "observe",
        OperationAccessV1::Mutate => "mutate",
    }
}
