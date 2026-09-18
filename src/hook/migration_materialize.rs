//! Frozen MigrationSessionV1, with separate pinned source and staged target views.
use super::*;
use crate::domain::{MigrationCompiledEdge, MigrationTargetInput, MigrationValueOrigin, RunId};
use crate::managed_data::StagedFile;
use std::collections::BTreeMap;

impl MaterializedExecution {
    pub(in crate::hook) fn create_migration(
        p: &PactrunPersistence,
        staging: &StagingSession,
        run: RunId,
        index: usize,
        compiled: &MigrationCompiledEdge,
        inputs: &BTreeMap<MigrationTargetInput, StagedFile>,
        service: Option<&crate::domain::ServiceMigrationEdge>,
    ) -> Result<Self, MaterializationError> {
        let operators = inputs
            .keys()
            .filter(|key| key.revision == compiled.bindings.target().content_digest)
            .cloned()
            .collect();
        let (edge, source_inputs, target_inputs) =
            p.prepare_migration_edge(run, &staging.owner(), index, &operators)?;
        let directory = staging.create_migration_execution_directory(run, index)?;
        let result = (|| {
            let hook = edge
                .declaration()
                .hook
                .as_ref()
                .ok_or(MaterializationError::InvalidParameter)?;
            let launch = compiled
                .launch
                .as_ref()
                .ok_or(MaterializationError::InvalidParameter)?;
            let runtime = directory.create_directory(Path::new("runtime"))?;
            let workspace = directory.create_directory(Path::new("workspace"))?;
            for file in &compiled.runtime {
                let relative = Path::new("runtime").join(runtime_relative_path(file));
                let (path, mut destination) = directory.create_file(&relative)?;
                p.copy_migration_runtime(run, edge.target(), file, &mut destination)?;
                destination.flush()?;
                drop(destination);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(
                        &path,
                        fs::Permissions::from_mode(if file.executable { 0o700 } else { 0o600 }),
                    )?;
                }
                let _ = path;
            }
            let mut source_bindings = Vec::new();
            for binding in edge.source_bindings() {
                let role = if source_inputs.contains(&binding.input) {
                    "active"
                } else {
                    "retained"
                };
                let path = materialize_binding(p, run, &directory, "source", binding, inputs)?;
                source_bindings.push(json!({"handle":random_handle()?, "input_id":binding.input.as_str(), "role":role, "readonly_path":host_path(&path)?}));
            }
            source_bindings.sort_by(|a, b| {
                (a["role"].as_str(), a["input_id"].as_str())
                    .cmp(&(b["role"].as_str(), b["input_id"].as_str()))
            });
            let mut target_bindings = Vec::new();
            for binding in edge
                .staged_bindings()
                .iter()
                .filter(|b| target_inputs.contains(&b.input))
            {
                let path = materialize_binding(p, run, &directory, "target", binding, inputs)?;
                target_bindings.push(json!({"handle":random_handle()?, "input_id":binding.input.as_str(), "role":"active", "readonly_path":host_path(&path)?}));
            }
            let mut outputs = Vec::new();
            let mut target_outputs = Vec::new();
            for input in &edge.declaration().produces_target {
                let (path, file) =
                    directory.create_file(&Path::new("outputs").join(input.as_str()))?;
                drop(file);
                let handle = random_handle()?;
                target_outputs.push(json!({"handle":handle, "input_id":input.as_str(), "staged_path":host_path(&path)?}));
                outputs.push(super::super::MigrationOutputSlot {
                    input: input.clone(),
                    handle,
                    path,
                });
            }
            let (program, mut arguments) =
                launch_command(launch, &runtime, hook.protocol_version.get())?;
            arguments.extend(hook.args.iter().cloned());
            let session_id = random_handle()?;
            let revision = |r: &crate::domain::RevisionIdentity| json!({"package_id":r.package_id.to_string(),"revision_content_digest":r.content_digest.to_string()});
            let session = json!({
                "type":"session_start", "protocol_version":hook.protocol_version.get(), "session_id":session_id,
                "run_id":run.to_string(), "revision":revision(edge.target()), "parameters":[],
                "workspace":{"handle":random_handle()?, "root_path":host_path(&workspace)?},
                "io":{"terminal":terminal_name(hook.io.terminal)},
                "operation":{"kind":"migration", "source_revision":revision(edge.source()),
                    "source_bindings":source_bindings, "target_bindings":target_bindings, "target_outputs":target_outputs}
            });
            super::super::protocol::validate_session_frame(&session)?;
            Ok((
                program,
                arguments,
                session_id,
                session,
                outputs,
                hook.io.terminal,
            ))
        })();
        match result {
            Ok((program, arguments, session_id, session, migration_outputs, terminal)) => {
                let materialized = Self {
                    directory,
                    program,
                    arguments,
                    session_id,
                    session,
                    migration_outputs,
                    terminal,
                    outputs: Vec::new(),
                    capture: None,
                    operation: super::super::protocol::SessionOperation::Migration,
                    v2_state: None,
                    v2_transport: None,
                    _service_access: None,
                };
                let bindings = service
                    .map(crate::domain::ServiceMigrationEdge::hook_bindings)
                    .transpose()
                    .map_err(|_| MaterializationError::InvalidParameter)?
                    .unwrap_or_default();
                materialized.with_service_target(
                    p,
                    staging,
                    run,
                    edge.declaration()
                        .hook
                        .as_ref()
                        .expect("Hook edge")
                        .protocol_version
                        .get(),
                    &bindings,
                    service.is_some_and(|e| e.transform),
                )
            }
            Err(error) => {
                let _ = directory.cleanup();
                Err(error)
            }
        }
    }
}

fn materialize_binding(
    p: &PactrunPersistence,
    run: RunId,
    directory: &ExecutionDirectory,
    context: &str,
    binding: &crate::domain::MigrationBinding,
    inputs: &BTreeMap<MigrationTargetInput, StagedFile>,
) -> Result<PathBuf, MaterializationError> {
    let (path, mut destination) =
        directory.create_file(&Path::new(context).join(binding.input.as_str()))?;
    match &binding.origin {
        MigrationValueOrigin::Existing(payload) => {
            p.copy_migration_payload(run, *payload, &mut destination)?
        }
        MigrationValueOrigin::Operator(input) => {
            let file = inputs
                .get(input)
                .ok_or(MaterializationError::InvalidParameter)?;
            io::copy(&mut file.try_clone_reader()?, &mut destination)?;
        }
        MigrationValueOrigin::Hook(_) => return Err(MaterializationError::InvalidParameter),
    }
    destination.flush()?;
    drop(destination);
    let mut permissions = fs::metadata(&path)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions)?;
    Ok(path)
}
