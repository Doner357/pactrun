use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    let workspace_root = workspace_root();
    let result = match env::args().nth(1).as_deref() {
        Some("ci") => run_ci(&workspace_root),
        Some("docs-build") => build_docs(&workspace_root),
        Some(command) => Err(format!("unknown xtask command: {command}")),
        None => Err("missing xtask command".to_owned()),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!("usage: cargo xtask <ci|docs-build>");
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
