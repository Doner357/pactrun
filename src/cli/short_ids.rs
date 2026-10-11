//! CLI selectors are resolved once, before effectful command dispatch.
use super::*;
use crate::domain::{
    CatalogIdentityKind as Kind, IdentityMatches, InstanceId, ServiceAllocationId, SnapshotId,
};

pub(super) trait Key: Clone + fmt::Display + FromStr {
    const NAME: &'static str;
    const TAG: &'static str = "";
    const WIDTH: usize = 32;
    const KIND: Option<Kind>;
    const SELECTOR_KIND: SelectorKind;
}
macro_rules! key {
    ($t:ty,$name:literal,$kind:ident) => {
        impl Key for $t {
            const NAME: &'static str = $name;
            const KIND: Option<Kind> = Some(Kind::$kind);
            const SELECTOR_KIND: SelectorKind = SelectorKind::$kind;
        }
    };
}
key!(RunId, "Run", Run);
key!(SnapshotId, "Snapshot", Snapshot);
key!(InstanceId, "Instance", Instance);
key!(ServiceAllocationId, "Allocation", Allocation);
impl Key for crate::domain::MigrationPathId {
    const NAME: &'static str = "Migration path";
    const TAG: &'static str = "mp1-";
    const WIDTH: usize = 64;
    const KIND: Option<Kind> = None;
    const SELECTOR_KIND: SelectorKind = SelectorKind::MigrationPath;
}
impl Key for crate::domain::RevisionContentDigest {
    const NAME: &'static str = "Revision digest";
    const TAG: &'static str = "sha256:";
    const WIDTH: usize = 64;
    const KIND: Option<Kind> = None;
    const SELECTOR_KIND: SelectorKind = SelectorKind::RevisionDigest;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Selector<T> {
    Full(T),
    Prefix(String),
}
impl<T: Key> FromStr for Selector<T> {
    type Err = CliError;
    fn from_str(text: &str) -> Result<Self, CliError> {
        if let Ok(id) = text.parse::<T>() {
            return Ok(Self::Full(id));
        }
        let hex = text
            .strip_prefix(T::TAG)
            .ok_or_else(|| CliError::usage(format!("{} ID requires {}", T::NAME, T::TAG)))?;
        if valid_hex(hex, T::WIDTH - 1) {
            Ok(Self::Prefix(hex.into()))
        } else {
            Err(CliError::usage(format!(
                "{} ID requires a complete ID or at least 8 lowercase hexadecimal digits",
                T::NAME
            )))
        }
    }
}
impl<T> From<T> for Selector<T> {
    fn from(value: T) -> Self {
        Self::Full(value)
    }
}
impl<T: Key> fmt::Display for Selector<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full(id) => id.fmt(f),
            Self::Prefix(p) => write!(f, "{}{p}", T::TAG),
        }
    }
}
impl<T: Key> Selector<T> {
    pub(super) fn full(self) -> T {
        match self {
            Self::Full(id) => id,
            Self::Prefix(_) => unreachable!("selector resolution precedes execution"),
        }
    }
    pub(super) fn resolve(&mut self, r: &mut Resolver<'_>) -> Result<(), CliError> {
        if let Self::Prefix(prefix) = self {
            let matches = r
                .app()?
                .identity_prefix(
                    T::KIND.expect("scoped selectors use their own observation"),
                    prefix,
                )
                .map_err(app_error)?;
            let id = unique(T::SELECTOR_KIND, &format!("{}{prefix}", T::TAG), matches)?;
            *self = Self::Full(
                id.parse()
                    .map_err(|_| CliError::operation("invalid catalog identity"))?,
            );
        }
        Ok(())
    }
}
pub(super) fn valid_hex(text: &str, max: usize) -> bool {
    (8..=max).contains(&text.len())
        && text
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

#[derive(Clone, Debug, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(super) enum SelectorKind {
    Run,
    Snapshot,
    Instance,
    Allocation,
    Revision,
    RevisionDigest,
    MigrationPath,
}
impl fmt::Display for SelectorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Run => "Run",
            Self::Snapshot => "Snapshot",
            Self::Instance => "Instance",
            Self::Allocation => "Allocation",
            Self::Revision => "Revision",
            Self::RevisionDigest => "Revision digest",
            Self::MigrationPath => "Migration path",
        })
    }
}
#[derive(Clone, Debug, serde::Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct Candidates {
    selector_kind: SelectorKind,
    prefix: String,
    candidates: Vec<String>,
    has_more: bool,
}
pub(super) fn unique(
    kind: SelectorKind,
    prefix: &str,
    matches: IdentityMatches,
) -> Result<String, CliError> {
    if matches.candidates.len() == 1 && !matches.has_more {
        return Ok(matches.candidates[0].clone());
    }
    let mut message = if matches.candidates.is_empty() {
        format!("No {kind} matches ID \"{prefix}\"")
    } else {
        format!("{kind} ID \"{prefix}\" matches multiple objects:\n")
    };
    for id in &matches.candidates {
        message.push_str(&format!("  {id}\n"));
    }
    if matches.has_more {
        message.push_str("  More matches are available.\n");
    }
    if !matches.candidates.is_empty() {
        message.push_str("Enter a longer ID prefix.");
    }
    let mut error = CliError::operation(message);
    error.partial = Some(presentation::PartialResult::IdentityCandidates(Box::new(
        Candidates {
            selector_kind: kind,
            prefix: prefix.into(),
            candidates: matches.candidates,
            has_more: matches.has_more,
        },
    )));
    Err(error)
}

pub(super) struct Resolver<'a> {
    pub(super) cancellation: ActionCancellation,
    root: Option<&'a OsString>,
    application: Option<PactrunApplication>,
}
impl<'a> Resolver<'a> {
    pub(super) fn new(root: Option<&'a OsString>, cancellation: &ActionCancellation) -> Self {
        Self {
            cancellation: cancellation.clone(),
            root,
            application: None,
        }
    }
    pub(super) fn app(&mut self) -> Result<&PactrunApplication, CliError> {
        if self.cancellation.is_requested() {
            return Err(CliError::operation("ID lookup cancelled"));
        }
        if self.application.is_none() {
            let root = self
                .root
                .filter(|r| !r.is_empty())
                .ok_or_else(|| CliError::operation(format!("{STORAGE_ROOT_ENV} is required")))?;
            self.application =
                Some(PactrunApplication::open_read_only(Path::new(root)).map_err(app_error)?);
        }
        Ok(self.application.as_ref().expect("opened read-only"))
    }
    pub(super) fn revision(&mut self, reference: &mut RevisionReference) -> Result<(), CliError> {
        if let RevisionReference::Named(named) = reference {
            *reference = RevisionReference::Exact(
                self.app()?
                    .resolve_named_revision(named)
                    .map_err(app_error)?,
            );
            return Ok(());
        }
        if let RevisionReference::Prefix { package, digest } = reference {
            let text = format!("exact:{package}/sha256:{digest}");
            let matches = self
                .app()?
                .revision_prefix(package, digest)
                .map_err(app_error)?;
            *reference = parse_revision_reference(unique(SelectorKind::Revision, &text, matches)?)?;
        }
        Ok(())
    }
}

pub(super) fn resolve(
    command: &mut Command,
    root: Option<&OsString>,
    cancellation: &ActionCancellation,
) -> Result<(), CliError> {
    let mut r = Resolver::new(root, cancellation);
    match command {
        Command::ShowRun { run } => run.resolve(&mut r)?,
        Command::ExportRevision { revision, .. } | Command::CreateInstance { revision, .. } => {
            r.revision(revision)?
        }
        Command::Catalog(c) => c.resolve_ids(&mut r)?,
        Command::Lifecycle(c) => c.resolve_ids(&mut r)?,
        Command::Artifact(c) => c.resolve_ids(&mut r)?,
        Command::Retirement(c) => c.resolve_ids(&mut r)?,
        Command::Snapshot(c) => c.resolve_ids(&mut r)?,
        Command::CreateAndRestore(c) => {
            r.revision(&mut c.revision)?;
            c.snapshot.resolve(&mut r)?;
        }
        Command::Migration(c) => c.resolve_ids(&mut r)?,
        _ => {}
    }
    Ok(())
}

pub(super) fn exact(reference: RevisionReference) -> crate::domain::RevisionIdentity {
    match reference {
        RevisionReference::Exact(id) => id,
        _ => unreachable!("exact selector resolved before dispatch"),
    }
}
