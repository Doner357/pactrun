//! Native package launch integration only. Package managers own program changes.
//! No managed data, program installation, source selection or update occurs here.
use std::{
    env,
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Flavor {
    Normal,
    Test,
}
fn flavor(path: &Path) -> io::Result<Flavor> {
    match path.file_stem().and_then(OsStr::to_str) {
        Some("pactrun") => Ok(Flavor::Normal),
        Some("pactrun-test") => Ok(Flavor::Test),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unrecognized package entrypoint",
        )),
    }
}
fn data_root(kind: Flavor, lookup: impl Fn(&str) -> Option<OsString>) -> io::Result<OsString> {
    // An explicitly supplied empty value is still the operator's choice: the
    // product rejects it, rather than this launcher silently substituting data.
    if let Some(root) = lookup("PACTRUN_STORAGE_ROOT") {
        return Ok(root);
    }
    #[cfg(windows)]
    let base = lookup("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(unix)]
    let base = lookup("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| lookup("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let base = base.filter(|p| p.is_absolute()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "set PACTRUN_STORAGE_ROOT or an absolute platform data directory",
        )
    })?;
    Ok(base
        .join(match kind {
            Flavor::Normal => "pactrun",
            Flavor::Test => "pactrun-test",
        })
        .into_os_string())
}
fn command(
    executable: &Path,
    args: impl IntoIterator<Item = OsString>,
    lookup: impl Fn(&str) -> Option<OsString>,
) -> io::Result<Command> {
    let kind = flavor(executable)?;
    let prefix = executable
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid package layout"))?;
    let program = prefix.join("libexec").join(if cfg!(windows) {
        "pactrun.exe"
    } else {
        "pactrun"
    });
    let mut command = Command::new(program);
    command
        .args(args)
        .env("PACTRUN_STORAGE_ROOT", data_root(kind, lookup)?);
    Ok(command)
}
fn launch() -> io::Result<ExitCode> {
    let executable = env::current_exe()?;
    let mut child = command(&executable, env::args_os().skip(1), |key| env::var_os(key))?;
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Preserve native argv, stdio, PID, signal delivery and exit status.
        Err(child.exec())
    }
    #[cfg(windows)]
    {
        // Both processes share the console. The real product handles Ctrl-C;
        // keep this forwarding parent alive until the product has cleaned up.
        ctrlc::set_handler(|| {}).map_err(io::Error::other)?;
        let status = child.status()?;
        // Windows process exit status is a native u32, not a portable u8.
        std::process::exit(status.code().unwrap_or(1));
    }
}
fn main() -> ExitCode {
    match launch() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("pactrun package launcher: {}", error.kind());
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/package-launcher-fixtures")
    }
    fn lookup(key: &str) -> Option<OsString> {
        matches!(key, "LOCALAPPDATA" | "XDG_DATA_HOME" | "HOME").then(|| base().into_os_string())
    }
    // Test-ID: PR-TEST-0626
    // Verifies: PR-REQ-0333
    #[test]
    fn normal_and_test_default_roots_are_distinct_and_explicit_roots_are_never_replaced() {
        let normal = data_root(Flavor::Normal, lookup).unwrap();
        let test = data_root(Flavor::Test, lookup).unwrap();
        assert_ne!(normal, test);
        assert!(Path::new(&normal).is_absolute());
        assert!(Path::new(&test).is_absolute());
        for explicit in [OsString::new(), base().join("explicit").into_os_string()] {
            assert_eq!(
                data_root(Flavor::Test, |key| if key == "PACTRUN_STORAGE_ROOT" {
                    Some(explicit.clone())
                } else {
                    None
                })
                .unwrap(),
                explicit
            );
        }
        assert!(
            data_root(Flavor::Normal, |_| Some(OsString::from("relative"))).is_ok(),
            "explicit root is forwarded, not interpreted by the launcher"
        );
        assert!(
            data_root(Flavor::Normal, |key| if key == "PACTRUN_STORAGE_ROOT" {
                None
            } else {
                Some("relative".into())
            })
            .is_err()
        );
    }
    // Test-ID: PR-TEST-0627
    // Verifies: PR-REQ-0333
    #[test]
    fn launcher_selects_only_its_adjacent_payload_and_forwards_argument_vectors_without_shell_evaluation()
     {
        let executable = base().join("bin").join(if cfg!(windows) {
            "pactrun-test.exe"
        } else {
            "pactrun-test"
        });
        let args: Vec<OsString> = [
            "--file",
            "space & dollar $HOME ; quote \\\"",
            "",
            "--literal=é",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        let command = command(&executable, args.clone(), lookup).unwrap();
        assert_eq!(
            command.get_args().map(OsStr::to_owned).collect::<Vec<_>>(),
            args
        );
        assert_eq!(
            Path::new(command.get_program()).parent(),
            Some(base().join("libexec").as_path())
        );
        assert_eq!(
            command
                .get_envs()
                .find(|(key, _)| *key == OsStr::new("PACTRUN_STORAGE_ROOT"))
                .unwrap()
                .1,
            Some(data_root(Flavor::Test, lookup).unwrap().as_os_str())
        );
        assert!(flavor(&base().join("other.exe")).is_err());
    }
}
