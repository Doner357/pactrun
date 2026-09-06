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

pub(super) struct MaterializedAction {
    directory: ExecutionDirectory,
    outputs: Vec<MaterializedOutputSlot>,
    program: PathBuf,
    arguments: Vec<String>,
    session_id: String,
    session: Value,
}

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
        Ok(Self {
            directory,
            outputs,
            program,
            arguments,
            session_id,
            session,
        })
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
            .collect()
    }

    pub(super) fn into_output_slots(self) -> Vec<LiveOutputSlot> {
        self.outputs
            .into_iter()
            .map(|slot| LiveOutputSlot {
                output: slot.output,
                path: slot.path,
            })
            .collect()
    }
}

#[derive(Debug)]
pub(super) enum MaterializationError {
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
