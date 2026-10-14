//! Configuration inspection does not acquire a Store or writer admission.
use super::*;
use crate::configuration::{Effective, FileSelection, Level, Overrides, Verbosity};

#[derive(Default)]
pub(super) struct Globals {
    pub(super) user_file: Option<PathBuf>,
    pub(super) overrides: Overrides,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct HumanPreferences {
    pub(super) verbosity: Verbosity,
    pub(super) phase_messages: bool,
}
impl Default for HumanPreferences {
    fn default() -> Self {
        Self {
            verbosity: Verbosity::Normal,
            phase_messages: true,
        }
    }
}
impl From<&Effective> for HumanPreferences {
    fn from(settings: &Effective) -> Self {
        Self {
            verbosity: settings.presentation.verbosity.value,
            phase_messages: settings.presentation.phase_messages.value,
        }
    }
}

pub(super) fn global_width(key: &str) -> Option<usize> {
    match key {
        "--config" | "--verbosity" | "--diagnostic-level" => Some(2),
        "--phase-messages" | "--no-phase-messages" => Some(1),
        _ => None,
    }
}

/// Parse only known leading options. Command operands are never reinterpreted.
pub(super) fn select(args: Vec<OsString>) -> Result<(Globals, Vec<OsString>), CliError> {
    let mut globals = Globals::default();
    let mut remaining = Vec::new();
    let mut index = 0;
    while let Some(key) = args.get(index).and_then(|a| a.to_str()) {
        if matches!(key, "--format" | "--machine-version") {
            remaining.push(args[index].clone());
            index += 1;
            if let Some(value) = args.get(index) {
                remaining.push(value.clone());
                index += 1;
            }
            continue;
        }
        let Some(width) = global_width(key) else {
            break;
        };
        let value = if width == 2 {
            Some(
                args.get(index + 1)
                    .ok_or_else(|| CliError::usage("missing configuration option value"))?,
            )
        } else {
            None
        };
        match key {
            "--config" => {
                let path = value.expect("file option");
                if path.is_empty() || path == "-" {
                    return Err(CliError::usage("--config requires a file path"));
                }
                set_once(&mut globals.user_file, PathBuf::from(path), key)?;
            }
            "--verbosity" => {
                let value = match value.and_then(|v| v.to_str()) {
                    Some("quiet") => Verbosity::Quiet,
                    Some("normal") => Verbosity::Normal,
                    Some("detailed") => Verbosity::Detailed,
                    _ => {
                        return Err(CliError::usage(
                            "--verbosity requires quiet, normal or detailed",
                        ));
                    }
                };
                set_once(&mut globals.overrides.verbosity, value, key)?;
            }
            "--diagnostic-level" => {
                let value = match value.and_then(|v| v.to_str()) {
                    Some("normal") => Level::Normal,
                    Some("detailed") => Level::Detailed,
                    _ => {
                        return Err(CliError::usage(
                            "--diagnostic-level requires normal or detailed",
                        ));
                    }
                };
                set_once(&mut globals.overrides.level, value, key)?;
            }
            "--phase-messages" | "--no-phase-messages" => set_once(
                &mut globals.overrides.phase_messages,
                key == "--phase-messages",
                "phase message selection",
            )?,
            _ => unreachable!("known global width"),
        }
        index += width;
    }
    remaining.extend_from_slice(&args[index..]);
    Ok((globals, remaining))
}

pub(super) fn resolve(
    globals: &Globals,
    root: Option<&OsString>,
    candidate_user: Option<&Path>,
    candidate_store: Option<&Path>,
    discover_user: bool,
) -> Result<Effective, CliError> {
    if candidate_user.is_some() && globals.user_file.is_some() {
        return Err(CliError::usage(
            "--config and --user-file select the same user configuration; choose one",
        ));
    }
    let explicit_user = candidate_user.or(globals.user_file.as_deref());
    let user = match explicit_user {
        Some(path) => Some(FileSelection {
            path: path.to_owned(),
            required: true,
        }),
        None if discover_user => {
            crate::configuration::default_user_path(env::current_exe().ok().as_deref(), |key| {
                env::var_os(key)
            })
            .map_err(|e| CliError::usage(e.to_string()))?
            .map(|path| FileSelection {
                path,
                required: false,
            })
        }
        None => None,
    };
    let store = match candidate_store {
        Some(path) => Some(FileSelection {
            path: path.to_owned(),
            required: true,
        }),
        None => match root {
            Some(root) if root.is_empty() => {
                return Err(CliError::usage("selected Store root must not be empty"));
            }
            Some(root) => Some(FileSelection {
                path: PathBuf::from(root).join("pactrun.toml"),
                required: false,
            }),
            None => None,
        },
    };
    crate::configuration::load(user.as_ref(), store.as_ref(), &globals.overrides)
        .map_err(|e| CliError::usage(e.to_string()))
}

pub(super) struct Request {
    validate: bool,
    user_file: Option<PathBuf>,
    store_file: Option<PathBuf>,
    pub(super) globals: Globals,
    pub(super) discover_user: bool,
}
impl Request {
    pub(super) fn command_name(&self) -> &'static str {
        if self.validate {
            "config validate"
        } else {
            "config show"
        }
    }
}

pub(super) fn parse(parser: &mut Parser) -> Result<Request, CliError> {
    let validate = match required_value_string(parser, "config command")?.as_str() {
        "show" => false,
        "validate" => true,
        _ => {
            return Err(CliError::usage(
                "unknown config command; use show or validate",
            ));
        }
    };
    let mut request = Request {
        validate,
        user_file: None,
        store_file: None,
        globals: Globals::default(),
        discover_user: false,
    };
    while let Some(arg) = parser.next().map_err(lex_error)? {
        let (slot, label) = match arg {
            Arg::Long("user-file") => (&mut request.user_file, "--user-file"),
            Arg::Long("store-file") => (&mut request.store_file, "--store-file"),
            _ => return Err(CliError::usage("unsupported config option")),
        };
        let path = required_parser_value(parser, "configuration file")?;
        if path.is_empty() || path == "-" {
            return Err(CliError::usage("configuration requires a file path"));
        }
        set_once(slot, PathBuf::from(path), label)?;
    }
    Ok(request)
}

#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct ConfigurationInspection {
    pub(super) valid: bool,
    pub(super) settings: Effective,
}

pub(super) fn execute(
    request: Request,
    root: Option<OsString>,
    output: reply::OutputContext,
) -> Result<(), CliError> {
    let name = request.command_name();
    let settings = resolve(
        &request.globals,
        root.as_ref(),
        request.user_file.as_deref(),
        request.store_file.as_deref(),
        request.discover_user,
    )?;
    presentation::emit_result(
        output,
        name,
        &ConfigurationInspection {
            valid: true,
            settings,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test-ID: PR-TEST-0744
    // Verifies: PR-REQ-0393, PR-REQ-0395
    #[test]
    fn inspection_and_validation_are_read_only_and_machine_readable() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/alpha8-config-tests");
        std::fs::create_dir_all(&root).unwrap();
        let temp = tempfile::tempdir_in(root).unwrap();
        let user = temp.path().join("user.toml");
        let store = temp.path().join("store.toml");
        std::fs::write(&user, "[presentation]\nverbosity = 'quiet'").unwrap();
        std::fs::write(&store, "[diagnostics]\nretain_hook_text = false").unwrap();
        let nonexistent = temp.path().join("not-a-store");
        for command in ["show", "validate"] {
            let args = vec![
                "--format".into(),
                "json".into(),
                "config".into(),
                command.into(),
                "--user-file".into(),
                user.clone().into_os_string(),
                "--store-file".into(),
                store.clone().into_os_string(),
            ];
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            assert_eq!(
                super::super::run(
                    args,
                    Some(nonexistent.clone().into_os_string()),
                    &mut std::io::empty(),
                    &mut stdout,
                    &mut stderr
                ),
                0,
                "{}",
                String::from_utf8_lossy(&stderr)
            );
            let value: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
            assert_eq!(value["command"], format!("config {command}"));
            assert_eq!(value["result"]["valid"], true);
            assert_eq!(
                value["result"]["settings"]["presentation"]["verbosity"]["value"],
                "quiet"
            );
            assert_eq!(
                value["result"]["settings"]["diagnostics"]["retain_hook_text"]["source"],
                "store_file"
            );
            assert!(!nonexistent.exists());
        }
    }

    // Test-ID: PR-TEST-0745
    // Verifies: PR-REQ-0395
    #[test]
    fn legacy_machine_selection_refuses_configuration_before_acquisition() {
        let args = [
            "--format",
            "json",
            "--machine-version",
            "1.0-alpha.3",
            "config",
            "show",
            "--user-file",
            "missing-must-not-be-read.toml",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        let mut out = Vec::new();
        assert_ne!(
            super::super::run(args, None, &mut std::io::empty(), &mut out, &mut Vec::new()),
            0
        );
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["format_version"], "1.0-alpha.3");
        assert!(value["command"].is_null());
        assert!(
            value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("machine interface")
        );
        assert!(
            !String::from_utf8(out)
                .unwrap()
                .contains("missing-must-not-be-read")
        );
    }
}
