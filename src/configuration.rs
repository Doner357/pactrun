//! Read-only, bounded configuration acquisition and typed invocation defaults.
//! Configuration is not authorization and never opens the management database.
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

const DOCUMENT_LIMIT: u64 = 65_536;
const MIB: u64 = 1_048_576;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum Level {
    #[default]
    Normal,
    Detailed,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum Verbosity {
    Quiet,
    #[default]
    Normal,
    Detailed,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum Source {
    Default,
    UserFile,
    StoreFile,
    CommandLine,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) struct Setting<T> {
    pub(crate) value: T,
    pub(crate) source: Source,
}
impl<T> Setting<T> {
    fn resolve(value: Option<T>, default: T, source: Source) -> Self {
        match value {
            Some(value) => Self { value, source },
            None => Self {
                value: default,
                source: Source::Default,
            },
        }
    }
    fn override_with(&mut self, value: Option<T>) {
        if let Some(value) = value {
            *self = Self {
                value,
                source: Source::CommandLine,
            };
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) struct Presentation {
    pub(crate) verbosity: Setting<Verbosity>,
    pub(crate) phase_messages: Setting<bool>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) struct StartupDiagnostics {
    pub(crate) level: Setting<Level>,
    pub(crate) retention_days: Setting<u32>,
    pub(crate) max_bytes: Setting<u64>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) struct StoreDiagnostics {
    pub(crate) level: Setting<Level>,
    pub(crate) retain_hook_text: Setting<bool>,
    pub(crate) retention_days: Setting<u32>,
    pub(crate) max_bytes: Setting<u64>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) struct Effective {
    pub(crate) presentation: Presentation,
    pub(crate) startup_diagnostics: StartupDiagnostics,
    pub(crate) diagnostics: StoreDiagnostics,
}
#[derive(Default)]
pub(crate) struct Overrides {
    pub(crate) verbosity: Option<Verbosity>,
    pub(crate) phase_messages: Option<bool>,
    pub(crate) level: Option<Level>,
    pub(crate) retain_hook_text: Option<bool>,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UserDocument {
    version: Option<u32>,
    presentation: PresentationDocument,
    startup_diagnostics: StartupDocument,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PresentationDocument {
    verbosity: Option<Verbosity>,
    phase_messages: Option<bool>,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct StartupDocument {
    level: Option<Level>,
    retention_days: Option<u32>,
    max_bytes: Option<u64>,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct StoreDocument {
    version: Option<u32>,
    diagnostics: StoreDocumentFields,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct StoreDocumentFields {
    level: Option<Level>,
    retain_hook_text: Option<bool>,
    retention_days: Option<u32>,
    max_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Error {
    pub(crate) scope: &'static str,
    pub(crate) reason: &'static str,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} configuration: {}", self.scope, self.reason)
    }
}
impl std::error::Error for Error {}
fn error(scope: &'static str, reason: &'static str) -> Error {
    Error { scope, reason }
}

pub(crate) struct FileSelection {
    pub(crate) path: PathBuf,
    pub(crate) required: bool,
}

pub(crate) fn default_user_path(
    executable: Option<&Path>,
    lookup: impl Fn(&str) -> Option<OsString>,
) -> Result<Option<PathBuf>, Error> {
    let entry = if executable
        .and_then(Path::file_stem)
        .is_some_and(|v| v == "pactrun-test")
    {
        "pactrun-test"
    } else {
        "pactrun"
    };
    #[cfg(windows)]
    let base = lookup("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = lookup("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| lookup("HOME").map(|h| PathBuf::from(h).join(".config")));
    base.map(|base| {
        if !base.is_absolute() {
            return Err(error("user_file", "configuration base must be absolute"));
        }
        Ok(base.join(entry).join("config.toml"))
    })
    .transpose()
}

pub(crate) fn load(
    user: Option<&FileSelection>,
    store: Option<&FileSelection>,
    overrides: &Overrides,
) -> Result<Effective, Error> {
    let user = read_document(user, "user_file")?;
    let store = read_document(store, "store_file")?;
    parse(&user, &store, overrides)
}

fn parse(user: &str, store: &str, overrides: &Overrides) -> Result<Effective, Error> {
    let user: UserDocument = toml::from_str(user).map_err(|_| {
        error(
            "user_file",
            "invalid TOML, unknown setting or invalid setting type",
        )
    })?;
    let store: StoreDocument = toml::from_str(store).map_err(|_| {
        error(
            "store_file",
            "invalid TOML, unknown setting or invalid setting type",
        )
    })?;
    for (version, scope) in [(user.version, "user_file"), (store.version, "store_file")] {
        if version.is_some_and(|v| v != 1) {
            return Err(error(scope, "unsupported configuration version"));
        }
    }
    validate_retention(
        user.startup_diagnostics.retention_days,
        user.startup_diagnostics.max_bytes,
        "user_file",
    )?;
    validate_retention(
        store.diagnostics.retention_days,
        store.diagnostics.max_bytes,
        "store_file",
    )?;
    let mut result = Effective {
        presentation: Presentation {
            verbosity: Setting::resolve(
                user.presentation.verbosity,
                Verbosity::Normal,
                Source::UserFile,
            ),
            phase_messages: Setting::resolve(
                user.presentation.phase_messages,
                true,
                Source::UserFile,
            ),
        },
        startup_diagnostics: StartupDiagnostics {
            level: Setting::resolve(
                user.startup_diagnostics.level,
                Level::Normal,
                Source::UserFile,
            ),
            retention_days: Setting::resolve(
                user.startup_diagnostics.retention_days,
                30,
                Source::UserFile,
            ),
            max_bytes: Setting::resolve(
                user.startup_diagnostics.max_bytes,
                64 * MIB,
                Source::UserFile,
            ),
        },
        diagnostics: StoreDiagnostics {
            level: Setting::resolve(store.diagnostics.level, Level::Normal, Source::StoreFile),
            retain_hook_text: Setting::resolve(
                store.diagnostics.retain_hook_text,
                true,
                Source::StoreFile,
            ),
            retention_days: Setting::resolve(
                store.diagnostics.retention_days,
                30,
                Source::StoreFile,
            ),
            max_bytes: Setting::resolve(store.diagnostics.max_bytes, 256 * MIB, Source::StoreFile),
        },
    };
    result
        .presentation
        .verbosity
        .override_with(overrides.verbosity);
    result
        .presentation
        .phase_messages
        .override_with(overrides.phase_messages);
    result
        .startup_diagnostics
        .level
        .override_with(overrides.level);
    result.diagnostics.level.override_with(overrides.level);
    result
        .diagnostics
        .retain_hook_text
        .override_with(overrides.retain_hook_text);
    Ok(result)
}

fn validate_retention(
    days: Option<u32>,
    bytes: Option<u64>,
    scope: &'static str,
) -> Result<(), Error> {
    if days.is_some_and(|v| !(1..=3650).contains(&v)) {
        return Err(error(scope, "retention_days must be between 1 and 3650"));
    }
    if bytes.is_some_and(|v| !(MIB..=1024 * MIB).contains(&v)) {
        return Err(error(
            scope,
            "max_bytes must be between 1048576 and 1073741824",
        ));
    }
    Ok(())
}

fn read_document(selection: Option<&FileSelection>, scope: &'static str) -> Result<String, Error> {
    let Some(selection) = selection else {
        return Ok(String::new());
    };
    let file = match open_document(&selection.path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !selection.required => {
            return Ok(String::new());
        }
        Err(e) => {
            return Err(error(
                scope,
                match e.kind() {
                    std::io::ErrorKind::NotFound => "selected configuration file does not exist",
                    std::io::ErrorKind::PermissionDenied => "configuration file access denied",
                    _ => "configuration file cannot be opened safely",
                },
            ));
        }
    };
    let metadata = file
        .metadata()
        .map_err(|_| error(scope, "configuration metadata unavailable"))?;
    if !metadata.is_file() {
        return Err(error(scope, "configuration must be a regular file"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(error(scope, "configuration reparse points are unsupported"));
        }
    }
    if metadata.len() > DOCUMENT_LIMIT {
        return Err(error(scope, "configuration exceeds 65536 bytes"));
    }
    let mut bytes = Vec::new();
    file.take(DOCUMENT_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error(scope, "configuration read failed"))?;
    if bytes.len() as u64 > DOCUMENT_LIMIT {
        return Err(error(scope, "configuration exceeds 65536 bytes"));
    }
    String::from_utf8(bytes).map_err(|_| error(scope, "configuration must be UTF-8"))
}

fn open_document(path: &Path) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Open the final object itself, then reject all reparse-point kinds.
        options.custom_flags(0x0020_0000 | 0x0200_0000);
    }
    options.open(path)
}

#[cfg(test)]
mod tests;
