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
                "usage: cargo xtask <ci|rust-ci|system-test|docs-build|revision-reference-verify|revision-canonical-verify|revision-reference-calculate|snapshot-reference-verify|snapshot-reference-calculate|snapshot-integrity-verify|hook-protocol-verify|error-catalog-verify>"
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
    revision_reference::verify(workspace_root)?;
    verify_revision_canonical(workspace_root)?;
    snapshot_reference::verify(workspace_root)?;
    snapshot_production::verify(workspace_root)?;
    hook_protocol::verify(workspace_root)?;
    error_catalog::verify(workspace_root)?;
    run_rust_ci(workspace_root)?;
    run(
        workspace_root,
        "pnpm",
        &["--dir", "website", "run", "typecheck"],
    )?;
    build_docs(workspace_root)
}

fn verify_revision_canonical(workspace_root: &Path) -> Result<(), String> {
    run(
        workspace_root,
        "node",
        &[
            "tests/oracles/revision_canonical_services.mjs",
            "tests/vectors/revision_canonical/services.json",
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
            "authoring::v2::tests",
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
    run(
        workspace_root,
        "cargo",
        &["test", "--workspace", "--all-features"],
    )?;
    Ok(())
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
            "m3_slice6_real_cli_e2e",
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
