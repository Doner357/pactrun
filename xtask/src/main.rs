use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

mod error_catalog;
mod hook_protocol;
mod lexical_v1;
mod release_sources;
mod revision_reference;
mod snapshot_production;
mod snapshot_reference;

fn main() -> ExitCode {
    let workspace_root = workspace_root();
    let result = match env::args().nth(1).as_deref() {
        Some("release-catalog") => {
            let args = env::args_os().skip(2).collect::<Vec<_>>();
            if !(2..=3).contains(&args.len()) {
                Err("usage: cargo xtask release-catalog RELEASES.json NEW_OUTPUT_DIRECTORY [PREVIOUS_RELEASES.json]".into())
            } else {
                release_sources::generate_catalog(
                    Path::new(&args[0]),
                    Path::new(&args[1]),
                    args.get(2).map(Path::new),
                )
            }
        }
        Some("release-publish-local") => {
            let args = env::args_os().skip(2).collect::<Vec<_>>();
            if args.len() != 3 {
                Err("usage: cargo xtask release-publish-local INPUT.json LOCAL_BARE_REPO DEFAULT_REF".into())
            } else if let Some(default) = args[2].to_str() {
                release_sources::publish_local(Path::new(&args[0]), Path::new(&args[1]), default)
            } else {
                Err("default ref must be ASCII".into())
            }
        }
        Some("release-sources") => {
            let args = env::args_os().skip(2).collect::<Vec<_>>();
            if args.len() != 2 {
                Err("usage: cargo xtask release-sources INPUT.json NEW_OUTPUT_DIRECTORY".into())
            } else {
                release_sources::generate(Path::new(&args[0]), Path::new(&args[1]))
            }
        }
        Some("ci") => run_ci(&workspace_root),
        Some("rust-ci") => run_rust_ci(&workspace_root),
        Some("conformance") => run_conformance(&workspace_root),
        Some("system-test") => run_system_tests(&workspace_root),
        Some("docs-build") => build_docs(&workspace_root),
        Some("revision-reference-verify") => revision_reference::verify(&workspace_root),
        Some("revision-canonical-verify") => verify_revision_canonical(&workspace_root),
        Some("revision-reference-calculate") => revision_reference::calculate(&workspace_root),
        Some("snapshot-reference-verify") => snapshot_reference::verify(&workspace_root),
        Some("snapshot-integrity-verify") => snapshot_production::verify(&workspace_root),
        Some("snapshot-reference-calculate") => snapshot_reference::calculate(&workspace_root),
        Some("hook-protocol-verify") => hook_protocol::verify(&workspace_root),
        Some("error-catalog-verify") => error_catalog::verify(&workspace_root),
        Some(command) => Err(format!("unknown xtask command: {command}")),
        None => Err("missing xtask command".to_owned()),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!(
                "usage: cargo xtask <ci|rust-ci|conformance|system-test|docs-build|revision-reference-verify|revision-canonical-verify|revision-reference-calculate|snapshot-reference-verify|snapshot-reference-calculate|snapshot-integrity-verify|hook-protocol-verify|error-catalog-verify>"
            );
            ExitCode::FAILURE
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be a direct child of the workspace root")
        .to_path_buf()
}

fn run_ci(workspace_root: &Path) -> Result<(), String> {
    run_conformance(workspace_root)?;
    run_rust_ci(workspace_root)?;
    run(
        workspace_root,
        "pnpm",
        &["--dir", "website", "run", "typecheck"],
    )?;
    build_docs(workspace_root)
}

fn run_conformance(workspace_root: &Path) -> Result<(), String> {
    revision_reference::verify(workspace_root)?;
    verify_revision_oracles(workspace_root)?;
    snapshot_reference::verify(workspace_root)?;
    hook_protocol::verify(workspace_root)?;
    // Production canonical/snapshot tests run once in the workspace test suite.
    // Standalone focused commands retain their production tests and traceability.
    error_catalog::verify(workspace_root)
}

fn verify_revision_oracles(workspace_root: &Path) -> Result<(), String> {
    run(
        workspace_root,
        "node",
        &[
            "tests/oracles/revision_canonical_operations.mjs",
            "tests/vectors/revision_canonical/operations.json",
        ],
    )?;
    run(
        workspace_root,
        "node",
        &[
            "tests/oracles/revision_canonical_services.mjs",
            "tests/vectors/revision_canonical/services.json",
        ],
    )
}

fn verify_revision_canonical(workspace_root: &Path) -> Result<(), String> {
    verify_revision_oracles(workspace_root)?;
    run(
        workspace_root,
        "cargo",
        &[
            "test",
            "-p",
            "pactrun",
            "--lib",
            "--all-features",
            "revision_canonical::tests",
        ],
    )?;
    run(
        workspace_root,
        "cargo",
        &[
            "test",
            "-p",
            "pactrun",
            "--lib",
            "--all-features",
            "authoring::",
        ],
    )
}

fn run_rust_ci(workspace_root: &Path) -> Result<(), String> {
    run(workspace_root, "cargo", &["fmt", "--all", "--check"])?;
    run(
        workspace_root,
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    let runner = env::var("PACTRUN_TEST_RUNNER").unwrap_or_else(|_| "nextest".into());
    for arguments in test_commands(&runner)? {
        run(workspace_root, "cargo", &arguments)?;
    }
    Ok(())
}

fn test_commands(runner: &str) -> Result<Vec<Vec<&'static str>>, String> {
    match runner {
        "nextest" => Ok(vec![
            vec![
                "nextest",
                "run",
                "--locked",
                "--workspace",
                "--all-features",
                "--profile",
                "ci",
                "--no-tests",
                "fail",
            ],
            vec!["test", "--locked", "--workspace", "--all-features", "--doc"],
        ]),
        "libtest" => Ok(vec![vec![
            "test",
            "--locked",
            "--workspace",
            "--all-features",
        ]]),
        _ => Err("PACTRUN_TEST_RUNNER must be nextest or libtest".into()),
    }
}

fn run_system_tests(workspace_root: &Path) -> Result<(), String> {
    run(
        workspace_root,
        "cargo",
        &[
            "test",
            "-p",
            "pactrun",
            "--test",
            "system",
            "--all-features",
        ],
    )?;
    run(
        workspace_root,
        "cargo",
        &[
            "test",
            "-p",
            "pactrun",
            "--test",
            "invoke_cli",
            "--all-features",
        ],
    )
}

fn build_docs(workspace_root: &Path) -> Result<(), String> {
    run(
        workspace_root,
        "pnpm",
        &["--dir", "website", "run", "build"],
    )
}

fn run(workspace_root: &Path, program: &str, arguments: &[&str]) -> Result<(), String> {
    eprintln!("> {program} {}", arguments.join(" "));

    #[cfg(windows)]
    let executable = if program == "pnpm" {
        "pnpm.cmd"
    } else {
        program
    };
    #[cfg(not(windows))]
    let executable = program;

    let status = Command::new(executable)
        .args(arguments)
        .current_dir(workspace_root)
        .status()
        .map_err(|error| format!("could not start {program}: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}

#[cfg(test)]
mod verification_tests {
    use super::test_commands;

    #[test]
    fn nextest_runs_production_tests_once_and_keeps_doctests() {
        let commands = test_commands("nextest").unwrap();
        assert_eq!(commands.len(), 2);
        assert_eq!(&commands[0][..2], &["nextest", "run"]);
        assert!(commands[0].ends_with(&["--no-tests", "fail"]));
        assert!(commands[1].contains(&"--doc"));
        assert!(
            commands
                .iter()
                .all(|command| command.contains(&"--workspace")
                    && command.contains(&"--all-features"))
        );
    }

    #[test]
    fn explicit_libtest_baseline_runs_once_and_unknown_runner_fails_closed() {
        assert_eq!(test_commands("libtest").unwrap().len(), 1);
        assert!(test_commands("").is_err());
        assert!(test_commands("nextest --skip tests").is_err());
    }
}
