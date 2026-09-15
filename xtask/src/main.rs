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
mod snapshot_integrity_v2;

fn main() -> ExitCode {
    let workspace_root = workspace_root();
    let result = match env::args().nth(1).as_deref() {
        Some("ci") => run_ci(&workspace_root),
        Some("rust-ci") => run_rust_ci(&workspace_root),
        Some("system-test") => run_system_tests(&workspace_root),
        Some("docs-build") => build_docs(&workspace_root),
        Some("revision-core-v1-verify") => revision_core_v1::verify(&workspace_root),
        Some("revision-core-v2-verify") => verify_revision_core_v2(&workspace_root),
        Some("revision-core-v1-calculate") => revision_core_v1::calculate(&workspace_root),
        Some("snapshot-integrity-v1-verify") => snapshot_integrity_v1::verify(&workspace_root),
        Some("snapshot-integrity-v2-verify") => snapshot_integrity_v2::verify(&workspace_root),
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
                "usage: cargo xtask <ci|rust-ci|system-test|docs-build|revision-core-v1-verify|revision-core-v2-verify|revision-core-v1-calculate|snapshot-integrity-v1-verify|snapshot-integrity-v1-calculate|snapshot-integrity-v2-verify|hook-protocol-v1-verify|error-taxonomy-v1-verify>"
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
    verify_revision_core_v2(workspace_root)?;
    snapshot_integrity_v1::verify(workspace_root)?;
    snapshot_integrity_v2::verify(workspace_root)?;
    hook_protocol_v1::verify(workspace_root)?;
    error_taxonomy_v1::verify(workspace_root)?;
    run_rust_ci(workspace_root)?;
    run(
        workspace_root,
        "pnpm",
        &["--dir", "website", "run", "typecheck"],
    )?;
    build_docs(workspace_root)
}

fn verify_revision_core_v2(workspace_root: &Path) -> Result<(), String> {
    run(
        workspace_root,
        "node",
        &[
            "tests/oracles/revision_core_format_v2.mjs",
            "tests/vectors/revision_core_format_v2/vectors.json",
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
            "revision_core_v2::tests",
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
