use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

mod error_taxonomy_v1;
mod hook_protocol_v1;
mod lexical_v1;
mod revision_core_v1;
mod snapshot_integrity_v1;

fn main() -> ExitCode {
    let workspace_root = workspace_root();
    let result = match env::args().nth(1).as_deref() {
        Some("ci") => run_ci(&workspace_root),
        Some("docs-build") => build_docs(&workspace_root),
        Some("revision-core-v1-verify") => revision_core_v1::verify(&workspace_root),
        Some("revision-core-v1-calculate") => revision_core_v1::calculate(&workspace_root),
        Some("snapshot-integrity-v1-verify") => snapshot_integrity_v1::verify(&workspace_root),
        Some("snapshot-integrity-v1-calculate") => {
            snapshot_integrity_v1::calculate(&workspace_root)
        }
        Some("hook-protocol-v1-verify") => hook_protocol_v1::verify(&workspace_root),
        Some("error-taxonomy-v1-verify") => error_taxonomy_v1::verify(&workspace_root),
        Some(command) => Err(format!("unknown xtask command: {command}")),
        None => Err("missing xtask command".to_owned()),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!(
                "usage: cargo xtask <ci|docs-build|revision-core-v1-verify|revision-core-v1-calculate|snapshot-integrity-v1-verify|snapshot-integrity-v1-calculate|hook-protocol-v1-verify|error-taxonomy-v1-verify>"
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
    revision_core_v1::verify(workspace_root)?;
    snapshot_integrity_v1::verify(workspace_root)?;
    hook_protocol_v1::verify(workspace_root)?;
    error_taxonomy_v1::verify(workspace_root)?;
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
    run(
        workspace_root,
        "pnpm",
        &["--dir", "website", "run", "typecheck"],
    )?;
    build_docs(workspace_root)
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

    let status = Command::new(program)
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
