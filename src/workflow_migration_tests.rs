use super::*;
use crate::domain::*;
use std::cell::Cell;

struct Repository(MigrationCompilationObservation);
impl MigrationCompilationRepository for Repository {
    fn verify_migration_runtime(&self, _: &RevisionIdentity) -> Result<(), PersistenceError> {
        Ok(())
    }
    fn observe_migration_compilation(
        &self,
        _: InstanceId,
    ) -> Result<MigrationCompilationObservation, PersistenceError> {
        Ok(self.0.clone())
    }
}
struct Launcher {
    called: Cell<usize>,
    unavailable: bool,
}
impl HostLauncherLookup for Launcher {
    fn resolve(
        &self,
        _: &[PathBuf],
        _: &HostExecutableName,
    ) -> Result<PathBuf, PlanCompilationError> {
        self.called.set(self.called.get() + 1);
        if self.unavailable {
            Err(PlanCompilationError::LauncherNotFound)
        } else {
            Ok(std::env::current_dir().unwrap().join("test-interpreter"))
        }
    }
}
fn revision(number: u8, incoming: Option<u8>, with_hook: bool) -> MigrationRevision {
    let file = RuntimeFileV1 {
        id: ContentId::parse("tool").unwrap(),
        path: RuntimePath::parse("tool").unwrap(),
        kind: RuntimeFileKindV1::RegularFile,
        blob_digest: Sha256Digest::from_bytes([10; 32]),
        executable: false,
    };
    let hook = with_hook.then(|| HookV1 {
        protocol_version: PositiveVersion::new(1).unwrap(),
        launch: HookLaunchV1::Interpreter {
            command: HostExecutableName::parse("interpreter").unwrap(),
            interpreter_args: vec![],
            script: file.id.clone(),
        },
        args: vec![],
        io: IOContractV1 {
            terminal: TerminalContractV1::None,
        },
    });
    let core = project_revision_core_v1(RevisionCoreProjectionInputV1 {
        inputs: vec![],
        actions: vec![],
        snapshot: None,
        migrations: incoming
            .map(|source| MigrationV1 {
                source_revision_digest: Sha256Digest::from_bytes([source; 32]),
                transitions: vec![],
                requires_source: vec![],
                requires_target: vec![],
                produces_target: vec![],
                hook,
            })
            .into_iter()
            .collect(),
        cleanup: None,
    })
    .unwrap();
    let closure =
        project_runtime_content_closure_v1(RuntimeContentProjectionInputV1 { files: vec![file] })
            .unwrap();
    MigrationRevision {
        identity: RevisionIdentity::new(
            PackageId::from_bytes([1; 16]),
            RevisionContentDigest::from_bytes([number; 32]),
        ),
        content: validate_revision_content_v1(core, closure).unwrap().into(),
    }
}
fn setup(with_hook: bool) -> (TransitionRevision, Repository) {
    let a = revision(1, None, false);
    let b = revision(2, Some(1), false);
    let c = revision(3, Some(2), with_hook);
    let intent = TransitionRevision {
        instance: InstanceId::from_bytes([1; 16]),
        expected_state_version: InstanceStateVersion::from_bytes([2; 16]),
        source: a.identity.clone(),
        target: c.identity.clone(),
        path: MigrationPathSelection::Automatic,
        operator_inputs: vec![],
        authorize_declassification: false,
    };
    let repo = Repository(MigrationCompilationObservation {
        service_state: None,
        instance: intent.instance,
        state_version: intent.expected_state_version,
        active_revision: intent.source.clone(),
        revisions: vec![a, b, c],
        bindings: vec![],
    });
    (intent, repo)
}

// Test-ID: PR-TEST-0284
// Verifies: PR-REQ-0305, PR-REQ-0156
#[test]
fn later_hook_is_qualified_before_a_plan_is_returned() {
    let (intent, repo) = setup(true);
    let launcher = Launcher {
        called: Cell::new(0),
        unavailable: true,
    };
    assert!(matches!(
        compile_migration(&repo, &launcher, &intent, &[]),
        Err(crate::application::ApplicationError::PlanCompilation(
            PlanCompilationError::LauncherNotFound
        ))
    ));
    assert_eq!(launcher.called.get(), 1);
    let launcher = Launcher {
        called: Cell::new(0),
        unavailable: false,
    };
    let plan = compile_migration(&repo, &launcher, &intent, &[]).unwrap();
    assert_eq!(plan.instance(), intent.instance);
    assert_eq!(plan.expected_state_version(), intent.expected_state_version);
    assert_eq!(plan.edges().len(), 2);
    assert!(plan.edges()[0].launch.is_none());
    assert!(plan.edges()[1].launch.is_some());
}

#[test]
fn declarative_compilation_needs_no_launcher_and_rejects_changed_observation() {
    let (mut intent, repo) = setup(false);
    let launcher = Launcher {
        called: Cell::new(0),
        unavailable: true,
    };
    assert!(compile_migration(&repo, &launcher, &intent, &[]).is_ok());
    assert_eq!(launcher.called.get(), 0);
    intent.expected_state_version = InstanceStateVersion::from_bytes([9; 16]);
    assert!(matches!(
        compile_migration(&repo, &launcher, &intent, &[]),
        Err(crate::application::ApplicationError::MigrationCompilation(
            MigrationError::InvalidObservation
        ))
    ));
    assert_eq!(launcher.called.get(), 0);
}
