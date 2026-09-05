//! Workflow compilation and execution-plan ownership.

// Slice 1 establishes crate-private compilation contracts before the human CLI
// and Executor become production callers in later M3 slices.
#![allow(dead_code)]

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    domain::{
        ActionExecutionPlan, CompilationFacts, CompiledHookLaunch, HookLaunchV1,
        HostExecutableName, InvokeAction, PlanCompilationError, RuntimeFileV1, build_action_plan,
    },
    persistence::PersistenceError,
};

pub(crate) trait ActionCompilationRepository {
    fn observe_action_compilation(
        &self,
        intent: &InvokeAction,
    ) -> Result<crate::domain::ActionCompilationObservation, PersistenceError>;
}

impl ActionCompilationRepository for crate::application::PactrunApplication {
    fn observe_action_compilation(
        &self,
        intent: &InvokeAction,
    ) -> Result<crate::domain::ActionCompilationObservation, PersistenceError> {
        crate::application::PactrunApplication::observe_action_compilation(self, intent)
    }
}
pub(crate) trait HostLauncherLookup {
    fn resolve(
        &self,
        directories: &[PathBuf],
        command: &HostExecutableName,
    ) -> Result<PathBuf, PlanCompilationError>;
}

pub(crate) struct PlatformHostLauncherLookup;

impl HostLauncherLookup for PlatformHostLauncherLookup {
    fn resolve(
        &self,
        directories: &[PathBuf],
        command: &HostExecutableName,
    ) -> Result<PathBuf, PlanCompilationError> {
        if directories.iter().any(|directory| !directory.is_absolute()) {
            return Err(PlanCompilationError::InvalidLauncherSearchDirectory);
        }
        directories
            .iter()
            .map(|directory| directory.join(command.as_str()))
            .find(|candidate| is_eligible_host_launcher(candidate))
            .ok_or(PlanCompilationError::LauncherNotFound)
    }
}

#[cfg(windows)]
fn is_eligible_host_launcher(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| !metadata.file_type().is_dir())
}

#[cfg(unix)]
fn is_eligible_host_launcher(path: &Path) -> bool {
    use rustix::fs::{Access, AtFlags, CWD, accessat};

    fs::metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
        && accessat(CWD, path, Access::EXEC_OK, AtFlags::EACCESS).is_ok()
}

pub(crate) fn compile_action<R: ActionCompilationRepository, L: HostLauncherLookup>(
    repository: &R,
    launcher_lookup: &L,
    intent: &InvokeAction,
    launcher_search_directories: &[PathBuf],
) -> Result<ActionExecutionPlan, crate::application::ApplicationError> {
    let observation = repository.observe_action_compilation(intent)?;
    let action = observation
        .revision_content
        .core
        .actions()
        .iter()
        .find(|action| action.id == intent.action)
        .cloned()
        .ok_or(crate::domain::ActionResolutionError::ActionNotFound)?;
    let runtime_content: Vec<RuntimeFileV1> = observation
        .revision_content
        .runtime_content
        .files()
        .to_vec();
    let launch = match &action.hook.launch {
        HookLaunchV1::Direct { executable } => {
            let executable = runtime_file(&runtime_content, executable)?;
            CompiledHookLaunch::Direct { executable }
        }
        HookLaunchV1::Interpreter {
            command,
            interpreter_args,
            script,
        } => {
            let resolved_absolute_path =
                launcher_lookup.resolve(launcher_search_directories, command)?;
            CompiledHookLaunch::Interpreter {
                launcher: crate::domain::InterpreterLauncherObservation {
                    command: command.clone(),
                    search_directories: launcher_search_directories.to_vec(),
                    resolved_absolute_path,
                },
                interpreter_args: interpreter_args.clone(),
                script: runtime_file(&runtime_content, script)?,
            }
        }
    };
    let facts = CompilationFacts {
        instance: observation.instance,
        observed_state_version: observation.state_version,
        active_revision: observation.active_revision,
        active_bindings: observation.active_bindings,
        required_inputs_satisfied: observation.required_inputs_satisfied,
        action,
        parameters: intent.parameters.clone(),
        runtime_content,
        launch,
    };
    Ok(build_action_plan(intent, facts)?)
}

fn runtime_file(
    runtime_content: &[RuntimeFileV1],
    content: &crate::domain::ContentId,
) -> Result<RuntimeFileV1, PlanCompilationError> {
    runtime_content
        .iter()
        .find(|file| file.id == *content)
        .cloned()
        .ok_or_else(|| PlanCompilationError::MissingRuntimeContent(content.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::*;

    #[derive(Clone)]
    struct FactsRepository(ActionCompilationObservation);

    impl ActionCompilationRepository for FactsRepository {
        fn observe_action_compilation(
            &self,
            _intent: &InvokeAction,
        ) -> Result<ActionCompilationObservation, PersistenceError> {
            Ok(self.0.clone())
        }
    }

    struct FakeLauncherLookup(PathBuf);

    impl HostLauncherLookup for FakeLauncherLookup {
        fn resolve(
            &self,
            _directories: &[PathBuf],
            _command: &HostExecutableName,
        ) -> Result<PathBuf, PlanCompilationError> {
            Ok(self.0.clone())
        }
    }

    fn identities() -> (InstanceId, InstanceStateVersion, RevisionIdentity) {
        (
            InstanceId::from_bytes([1; 16]),
            InstanceStateVersion::from_bytes([2; 16]),
            RevisionIdentity::new(
                PackageId::from_bytes([3; 16]),
                RevisionContentDigest::from_bytes([4; 32]),
            ),
        )
    }

    fn revision(interpreter: bool) -> ValidatedRevisionContentV1 {
        let file = RuntimeFileV1 {
            id: ContentId::parse("tool").unwrap(),
            path: RuntimePath::parse("bin/tool").unwrap(),
            kind: RuntimeFileKindV1::RegularFile,
            blob_digest: Sha256Digest::from_bytes([5; 32]),
            executable: !interpreter,
        };
        let launch = if interpreter {
            HookLaunchV1::Interpreter {
                command: HostExecutableName::parse("runtime").unwrap(),
                interpreter_args: vec!["--strict".to_owned()],
                script: file.id.clone(),
            }
        } else {
            HookLaunchV1::Direct {
                executable: file.id.clone(),
            }
        };
        let core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
            inputs: vec![InputDeclarationV1 {
                id: InputIdentity::parse("config").unwrap(),
                required: true,
                protection: InputProtectionV1::Secret,
            }],
            actions: vec![ActionV1 {
                id: ActionIdentity::parse("inspect").unwrap(),
                access: OperationAccessV1::Observe,
                parameters: Vec::new(),
                hook: HookV1 {
                    protocol_version: PositiveVersion::new(1).unwrap(),
                    launch,
                    args: vec!["tail".to_owned()],
                    io: IOContractV1 {
                        terminal: TerminalContractV1::Output,
                    },
                },
                outputs: vec![ManagedOutputV1 {
                    id: ManagedOutputIdentity::parse("report").unwrap(),
                }],
            }],
            snapshot: None,
            migrations: Vec::new(),
            cleanup: None,
        })
        .unwrap();
        let runtime_content = project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 {
            files: vec![file],
        })
        .unwrap();
        validate_revision_content_v1(core, runtime_content).unwrap()
    }

    fn setup(interpreter: bool) -> (InvokeAction, FactsRepository) {
        let (instance, state, revision_id) = identities();
        let content = revision(interpreter);
        let binding = ActiveInstanceBindingReference {
            input: InputIdentity::parse("config").unwrap(),
            payload: ManagedInputPayloadId::from_bytes([6; 16]),
            protection: ManagedInputProtection::Secret,
        };
        let intent = InvokeAction {
            instance,
            expected_state_version: state,
            active_revision: revision_id.clone(),
            action: ActionIdentity::parse("inspect").unwrap(),
            parameters: Vec::new(),
        };
        let repository = FactsRepository(ActionCompilationObservation {
            instance,
            state_version: state,
            active_revision: revision_id,
            active_bindings: vec![binding],
            required_inputs_satisfied: true,
            revision_content: content,
        });
        (intent, repository)
    }

    #[test]
    fn explicit_facts_produce_a_deterministic_plan_with_exact_references() {
        let (intent, repository) = setup(false);
        let launcher = FakeLauncherLookup(PathBuf::from("unused"));
        let left = compile_action(&repository, &launcher, &intent, &[]).unwrap();
        let right = compile_action(&repository, &launcher, &intent, &[]).unwrap();
        assert_eq!(left, right);
        assert_eq!(left.instance(), intent.instance);
        assert_eq!(left.expected_state_version(), intent.expected_state_version);
        assert_eq!(left.active_revision(), &intent.active_revision);
        assert_eq!(left.active_bindings(), repository.0.active_bindings);
        assert!(matches!(left.launch(), CompiledHookLaunch::Direct { .. }));
        assert_eq!(
            left.steps(),
            vec![
                ActionPlanStep::EstablishSession,
                ActionPlanStep::LaunchHook,
                ActionPlanStep::AcceptCompletion,
                ActionPlanStep::PublishDeclaredOutputs,
                ActionPlanStep::Finalize
            ]
        );
    }

    #[test]
    fn pure_plan_builder_rejects_facts_that_do_not_match_the_intent() {
        let (intent, repository) = setup(false);
        let observation = repository.0;
        let action = observation
            .revision_content
            .core
            .actions()
            .first()
            .unwrap()
            .clone();
        let runtime_content = observation
            .revision_content
            .runtime_content
            .files()
            .to_vec();
        let launch = CompiledHookLaunch::Direct {
            executable: runtime_content.first().unwrap().clone(),
        };
        let facts = CompilationFacts {
            instance: observation.instance,
            observed_state_version: observation.state_version,
            active_revision: observation.active_revision,
            active_bindings: observation.active_bindings,
            required_inputs_satisfied: observation.required_inputs_satisfied,
            action,
            parameters: intent.parameters.clone(),
            runtime_content,
            launch,
        };

        let mut wrong_instance = facts.clone();
        wrong_instance.instance = InstanceId::from_bytes([8; 16]);
        let mut wrong_state = facts.clone();
        wrong_state.observed_state_version = InstanceStateVersion::from_bytes([9; 16]);
        let mut wrong_revision = facts.clone();
        wrong_revision.active_revision = RevisionIdentity::new(
            PackageId::from_bytes([10; 16]),
            RevisionContentDigest::from_bytes([11; 32]),
        );
        let mut wrong_action = facts;
        wrong_action.action.id = ActionIdentity::parse("other").unwrap();

        for inconsistent in [wrong_instance, wrong_state, wrong_revision, wrong_action] {
            assert_eq!(
                build_action_plan(&intent, inconsistent),
                Err(PlanCompilationError::InconsistentFacts)
            );
        }
    }

    #[test]
    fn compiled_plan_is_detached_from_later_fact_changes() {
        let (intent, mut repository) = setup(false);
        let plan = compile_action(
            &repository,
            &FakeLauncherLookup(PathBuf::from("unused")),
            &intent,
            &[],
        )
        .unwrap();
        let original = plan.clone();

        repository.0.state_version = InstanceStateVersion::from_bytes([12; 16]);
        repository.0.active_bindings.clear();
        repository.0.required_inputs_satisfied = false;

        assert_eq!(plan, original);
        assert_eq!(plan.expected_state_version(), intent.expected_state_version);
        assert_eq!(plan.active_bindings().len(), 1);
    }

    // Test-ID: PR-TEST-0081
    // Verifies: PR-REQ-0194, PR-REQ-0274
    #[test]
    fn interpreter_launcher_selection_and_plan_facts_are_exact() {
        let (intent, repository) = setup(true);
        let exact = if cfg!(windows) {
            PathBuf::from(r"C:\host\runtime.exe")
        } else {
            PathBuf::from("/host/runtime")
        };
        let search_directories = if cfg!(windows) {
            vec![PathBuf::from(r"C:\first"), PathBuf::from(r"D:\second")]
        } else {
            vec![PathBuf::from("/first"), PathBuf::from("/second")]
        };
        let plan = compile_action(
            &repository,
            &FakeLauncherLookup(exact.clone()),
            &intent,
            &search_directories,
        )
        .unwrap();
        match plan.launch() {
            CompiledHookLaunch::Interpreter {
                launcher,
                interpreter_args,
                ..
            } => {
                assert_eq!(launcher.resolved_absolute_path, exact);
                assert_eq!(launcher.command.as_str(), "runtime");
                assert_eq!(launcher.search_directories, search_directories);
                assert_eq!(interpreter_args.as_slice(), ["--strict"]);
            }
            _ => panic!("expected interpreter launch"),
        }
        assert_platform_launcher_lookup_semantics();
    }

    #[cfg(unix)]
    fn write_eligible_launcher(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(path, b"not a prevalidated executable image").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[cfg(windows)]
    fn write_eligible_launcher(path: &Path) {
        fs::write(path, b"not a prevalidated executable image").unwrap();
    }

    #[cfg(unix)]
    fn create_file_symlink(target: &Path, link: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).unwrap();
        true
    }

    #[cfg(windows)]
    fn create_file_symlink(target: &Path, link: &Path) -> bool {
        match std::os::windows::fs::symlink_file(target, link) {
            Ok(()) => true,
            Err(error) if error.raw_os_error() == Some(1314) => false,
            Err(error) => panic!("could not create launcher file symlink: {error}"),
        }
    }

    #[cfg(unix)]
    fn create_directory_symlink(target: &Path, link: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).unwrap();
        true
    }

    #[cfg(windows)]
    fn create_directory_symlink(target: &Path, link: &Path) -> bool {
        match std::os::windows::fs::symlink_dir(target, link) {
            Ok(()) => true,
            Err(error) if error.raw_os_error() == Some(1314) => false,
            Err(error) => panic!("could not create launcher directory symlink: {error}"),
        }
    }

    fn assert_platform_launcher_lookup_semantics() {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m3-launcher-tests");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("launcher-")
            .tempdir_in(parent)
            .unwrap();
        let first = temporary.path().join("first");
        let second = temporary.path().join("second");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let command = HostExecutableName::parse(if cfg!(windows) {
            "runtime.exe"
        } else {
            "runtime"
        })
        .unwrap();
        let first_candidate = first.join(command.as_str());
        let second_candidate = second.join(command.as_str());
        write_eligible_launcher(&first_candidate);
        write_eligible_launcher(&second_candidate);
        let observation = PlatformHostLauncherLookup
            .resolve(&[first.clone(), second.clone()], &command)
            .unwrap();
        assert_eq!(observation, first_candidate);
        assert_eq!(
            PlatformHostLauncherLookup.resolve(&[PathBuf::from("relative")], &command),
            Err(PlanCompilationError::InvalidLauncherSearchDirectory)
        );
        fs::remove_file(first.join(command.as_str())).unwrap();
        fs::create_dir(first.join(command.as_str())).unwrap();
        let observation = PlatformHostLauncherLookup
            .resolve(&[first.clone(), second.clone()], &command)
            .unwrap();
        assert_eq!(observation, second_candidate);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&second_candidate, fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(
                PlatformHostLauncherLookup.resolve(std::slice::from_ref(&second), &command),
                Err(PlanCompilationError::LauncherNotFound)
            );
            fs::set_permissions(&second_candidate, fs::Permissions::from_mode(0o700)).unwrap();
        }

        let target_directory = temporary.path().join("target-directory");
        fs::create_dir(&target_directory).unwrap();
        let target_candidate = target_directory.join(command.as_str());
        write_eligible_launcher(&target_candidate);

        let linked_directory = temporary.path().join("linked-directory");
        if create_directory_symlink(&target_directory, &linked_directory) {
            let exact_linked_candidate = linked_directory.join(command.as_str());
            assert_eq!(
                PlatformHostLauncherLookup
                    .resolve(std::slice::from_ref(&linked_directory), &command)
                    .unwrap(),
                exact_linked_candidate
            );
        }

        let final_link_directory = temporary.path().join("final-link-directory");
        fs::create_dir(&final_link_directory).unwrap();
        let final_link_candidate = final_link_directory.join(command.as_str());
        if create_file_symlink(&target_candidate, &final_link_candidate) {
            assert_eq!(
                PlatformHostLauncherLookup
                    .resolve(std::slice::from_ref(&final_link_directory), &command)
                    .unwrap(),
                final_link_candidate
            );
        }

        let extensionless = HostExecutableName::parse("extensionless").unwrap();
        fs::write(second.join("extensionless.exe"), b"must not be inferred").unwrap();
        assert_eq!(
            PlatformHostLauncherLookup.resolve(std::slice::from_ref(&second), &extensionless),
            Err(PlanCompilationError::LauncherNotFound)
        );
        assert_eq!(
            PlatformHostLauncherLookup.resolve(&[], &command),
            Err(PlanCompilationError::LauncherNotFound)
        );
        assert_eq!(
            PlatformHostLauncherLookup
                .resolve(&[first], &HostExecutableName::parse("absent").unwrap()),
            Err(PlanCompilationError::LauncherNotFound)
        );
    }
}
