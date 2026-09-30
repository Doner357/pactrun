use super::*;
use crate::domain::{CompiledCleanup, ManagedInputRole, RevisionIdentity, RunId};

impl MaterializedExecution {
    pub(in crate::hook) fn create_cleanup(
        p: &PactrunPersistence,
        staging: &StagingSession,
        run: RunId,
        revision: &RevisionIdentity,
        plan: &CompiledCleanup,
    ) -> Result<Self, MaterializationError> {
        let directory = staging.create_execution_directory(run)?;
        let result = (|| {
            let runtime = directory.create_directory(Path::new("runtime"))?;
            let workspace = directory.create_directory(Path::new("workspace"))?;
            directory.create_directory(Path::new("bindings"))?;
            for file in &plan.runtime_content {
                materialize_runtime_file(p, run, &directory, file)?;
            }
            let mut registry = p.admitted_cleanup_registry(run)?;
            registry.sort_by(|a, b| {
                let role = |b: &crate::domain::PinnedInputBinding| match b.role {
                    ManagedInputRole::Active { .. } => 0,
                    ManagedInputRole::Retained => 1,
                };
                (role(a), &a.input).cmp(&(role(b), &b.input))
            });
            let mut bindings = Vec::new();
            for binding in registry
                .into_iter()
                .filter(|binding| binding.payload.is_some())
            {
                let (path, mut file) =
                    directory.create_file(&Path::new("bindings").join(binding.input.as_str()))?;
                p.stream_admitted_payload(run, &binding.input, &mut file)?;
                file.flush()?;
                drop(file);
                let mut permissions = fs::metadata(&path)?.permissions();
                permissions.set_readonly(true);
                fs::set_permissions(&path, permissions)?;
                bindings.push(json!({"handle":random_handle()?,"input_id":binding.input.as_str(),
                    "role":match binding.role { ManagedInputRole::Active { .. }=>"active",ManagedInputRole::Retained=>"retained" },
                    "readonly_path":host_path(&path)?}));
            }
            let (program, mut arguments) =
                launch_command(&plan.launch, &runtime, plan.hook.protocol_version)?;
            arguments.extend(plan.hook.args.iter().cloned());
            let session_id = random_handle()?;
            let session = json!({"type":"session_start","protocol_version":plan.hook.protocol_version,
                "session_id":session_id,"run_id":run.to_string(),
                "revision":{"package_id":revision.package_id.to_string(),"revision_content_digest":revision.content_digest.to_string()},
                "parameters":[],"workspace":{"handle":random_handle()?,"root_path":host_path(&workspace)?},
                "io":{"terminal":terminal_name(plan.hook.io.terminal)},"operation":{"kind":"cleanup","bindings":bindings}});
            super::super::protocol::validate_session_frame(&session)?;
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
                operation: super::super::protocol::SessionOperation::Cleanup,
                terminal: plan.hook.io.terminal,
                capture: None,
                migration_outputs: Vec::new(),
                protocol_state: None,
                protocol_transport: None,
                _service_access: None,
            }
            .with_service(
                p,
                staging,
                run,
                plan.hook.protocol_version,
                &plan.service_bindings,
            ),
            Err(error) => {
                let _ = directory.cleanup();
                Err(error)
            }
        }
    }
}
