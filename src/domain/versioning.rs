//! Version identifiers and explicit support policy. Ordering is not codec support.
use serde::{Serialize, Serializer};
use std::{cmp::Ordering, fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum Stage {
    Alpha,
    Beta,
    Rc,
}
impl Stage {
    fn name(self) -> &'static str {
        match self {
            Self::Alpha => "alpha",
            Self::Beta => "beta",
            Self::Rc => "rc",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Prerelease {
    stage: Stage,
    number: u64,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FormatVersion {
    major: u64,
    minor: u64,
    prerelease: Option<Prerelease>,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ProductVersion {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Option<Prerelease>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VersionError(&'static str);
impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for VersionError {}

fn number(text: &str) -> Result<u64, VersionError> {
    if text.is_empty()
        || !text.bytes().all(|b| b.is_ascii_digit())
        || (text.len() > 1 && text.starts_with('0'))
    {
        return Err(VersionError(
            "version numbers require canonical unsigned decimal integers",
        ));
    }
    text.parse()
        .map_err(|_| VersionError("version component exceeds u64"))
}
fn parts(text: &str, count: usize) -> Result<(Vec<u64>, Option<Prerelease>), VersionError> {
    if text.len() > 128 || !text.is_ascii() {
        return Err(VersionError("version must be bounded ASCII"));
    }
    let (base, suffix) = text
        .split_once('-')
        .map_or((text, None), |(b, p)| (b, Some(p)));
    let components = base.split('.').map(number).collect::<Result<Vec<_>, _>>()?;
    if components.len() != count {
        return Err(VersionError("wrong version component count"));
    }
    let prerelease = suffix
        .map(|s| {
            let (stage, n) = s
                .split_once('.')
                .ok_or(VersionError("prerelease must be alpha.N, beta.N or rc.N"))?;
            let stage = match stage {
                "alpha" => Stage::Alpha,
                "beta" => Stage::Beta,
                "rc" => Stage::Rc,
                _ => return Err(VersionError("unsupported prerelease stage spelling")),
            };
            let number = number(n)?;
            if number == 0 {
                return Err(VersionError("prerelease sequence starts at one"));
            }
            Ok(Prerelease { stage, number })
        })
        .transpose()?;
    Ok((components, prerelease))
}
fn precedence(a: Option<Prerelease>, b: Option<Prerelease>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => a.cmp(&b),
    }
}
fn suffix(f: &mut fmt::Formatter<'_>, p: Option<Prerelease>) -> fmt::Result {
    if let Some(p) = p {
        write!(f, "-{}.{}", p.stage.name(), p.number)?;
    }
    Ok(())
}
impl FormatVersion {
    pub(crate) const BASELINE: Self = Self {
        major: 1,
        minor: 0,
        prerelease: Some(Prerelease {
            stage: Stage::Alpha,
            number: 1,
        }),
    };
    pub(crate) const fn major(self) -> u64 {
        self.major
    }
    pub(crate) const fn is_formal(self) -> bool {
        self.prerelease.is_none()
    }
}
impl FromStr for FormatVersion {
    type Err = VersionError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (p, prerelease) = parts(s, 2)?;
        Ok(Self {
            major: p[0],
            minor: p[1],
            prerelease,
        })
    }
}
impl ProductVersion {
    pub(crate) const fn major(self) -> u64 {
        self.major
    }
    pub(crate) const fn is_formal(self) -> bool {
        self.prerelease.is_none()
    }
    pub(crate) const fn requires_formal_defaults(self) -> bool {
        self.prerelease.is_none()
            || matches!(
                self.prerelease,
                Some(Prerelease {
                    stage: Stage::Rc,
                    ..
                })
            )
    }
}
impl FromStr for ProductVersion {
    type Err = VersionError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (p, prerelease) = parts(s, 3)?;
        Ok(Self {
            major: p[0],
            minor: p[1],
            patch: p[2],
            prerelease,
        })
    }
}
impl fmt::Display for FormatVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)?;
        suffix(f, self.prerelease)
    }
}
impl fmt::Display for ProductVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        suffix(f, self.prerelease)
    }
}
impl Serialize for FormatVersion {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
impl Serialize for ProductVersion {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
impl Ord for FormatVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor)
            .cmp(&(other.major, other.minor))
            .then_with(|| precedence(self.prerelease, other.prerelease))
    }
}
impl PartialOrd for FormatVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ProductVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| precedence(self.prerelease, other.prerelease))
    }
}
impl PartialOrd for ProductVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VersionDomain {
    PackSource,
    Revision,
    Hook,
    Snapshot,
    SnapshotBundle,
    PackDistribution,
    Machine,
    Persistence,
}
impl VersionDomain {
    pub(crate) const ALL: [Self; 8] = [
        Self::PackSource,
        Self::Revision,
        Self::Hook,
        Self::Snapshot,
        Self::SnapshotBundle,
        Self::PackDistribution,
        Self::Machine,
        Self::Persistence,
    ];
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::PackSource => "pack_source",
            Self::Revision => "revision_canonical",
            Self::Hook => "hook_protocol",
            Self::Snapshot => "snapshot_integrity",
            Self::SnapshotBundle => "snapshot_bundle",
            Self::PackDistribution => "pack_distribution",
            Self::Machine => "cli_machine_interface",
            Self::Persistence => "persistence",
        }
    }
    pub(crate) const fn current(self) -> FormatVersion {
        // Each domain owns its entry; shared initial values do not couple evolution.
        match self {
            Self::Revision | Self::Hook | Self::Snapshot | Self::SnapshotBundle => {
                FormatVersion::BASELINE
            }
            Self::PackSource | Self::PackDistribution | Self::Machine | Self::Persistence => {
                FormatVersion {
                    major: 1,
                    minor: 0,
                    prerelease: Some(Prerelease {
                        stage: Stage::Alpha,
                        number: if matches!(self, Self::Persistence) {
                            3
                        } else {
                            2
                        },
                    }),
                }
            }
        }
    }
    pub(crate) const fn current_text(self) -> &'static str {
        match self {
            Self::Persistence => "1.0-alpha.3",
            Self::PackSource | Self::PackDistribution | Self::Machine => "1.0-alpha.2",
            _ => "1.0-alpha.1",
        }
    }
    pub(crate) fn supports(self, version: FormatVersion) -> bool {
        version == self.current()
    }
    pub(crate) fn unsupported_message(self, required: FormatVersion) -> String {
        format!(
            "unsupported {} version {required}; this executable supports {}; use an explicitly supporting release without converting or deleting data",
            self.name(),
            self.current()
        )
    }
    pub(crate) fn require(self, text: &str) -> Result<FormatVersion, String> {
        let version: FormatVersion = text
            .parse()
            .map_err(|e: VersionError| format!("invalid {} version: {e}", self.name()))?;
        if !self.supports(version) {
            return Err(format!(
                "unsupported {} version {version}; this executable supports {}; use an explicitly supporting release, without converting or deleting data",
                self.name(),
                self.current()
            ));
        }
        Ok(version)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReleaseSelection {
    Major { major: u64, allow_prerelease: bool },
    Exact(ProductVersion),
}
impl ReleaseSelection {
    pub(crate) fn select(
        self,
        candidates: impl IntoIterator<Item = ProductVersion>,
    ) -> Option<ProductVersion> {
        candidates
            .into_iter()
            .filter(|v| match self {
                Self::Major {
                    major,
                    allow_prerelease,
                } => v.major == major && (allow_prerelease || v.prerelease.is_none()),
                Self::Exact(exact) => *v == exact,
            })
            .max()
    }
    pub(crate) fn update(
        self,
        current: ProductVersion,
        candidates: impl IntoIterator<Item = ProductVersion>,
    ) -> Option<ProductVersion> {
        self.select(candidates)
            .filter(|v| v.major == current.major && *v >= current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0618
    // Verifies: PR-REQ-0077, PR-REQ-0331
    #[test]
    fn version_syntax_is_typed_bounded_and_round_trips_as_strings() {
        for text in ["1.0-alpha.1", "1.0-beta.10", "1.0-rc.2", "1.0", "2.10"] {
            let version: FormatVersion = text.parse().unwrap();
            assert_eq!(version.to_string(), text);
            assert_eq!(
                serde_json::to_string(&version).unwrap(),
                format!("\"{text}\"")
            );
        }
        for text in [
            "",
            "1",
            "1.0.0",
            "01.0",
            "1.00",
            "1.0-alpha.0",
            "1.0-alpha.01",
            "1.0-nightly.1",
            "1.0-rc.1.2",
            "1.0+meta",
            " 1.0",
            "1.0\n",
            "1.0-β.1",
            "18446744073709551616.0",
        ] {
            assert!(text.parse::<FormatVersion>().is_err(), "{text:?}");
        }
        assert!("1.0-alpha.1".parse::<ProductVersion>().is_err());
        assert_eq!(
            "1.0.0-alpha.1"
                .parse::<ProductVersion>()
                .unwrap()
                .to_string(),
            "1.0.0-alpha.1"
        );
        assert!(
            format!("1.0-alpha.{}", "1".repeat(128))
                .parse::<FormatVersion>()
                .is_err()
        );
    }
    // Test-ID: PR-TEST-0619
    // Verifies: PR-REQ-0077, PR-REQ-0331
    #[test]
    fn eight_explicit_domains_do_not_infer_support_from_order_or_major() {
        let names: std::collections::BTreeSet<_> = VersionDomain::ALL
            .into_iter()
            .map(VersionDomain::name)
            .collect();
        assert_eq!(names.len(), 8);
        for domain in VersionDomain::ALL {
            assert_eq!(
                domain.require(domain.current_text()).unwrap(),
                domain.current()
            );
            for text in ["1.0-alpha.2", "1.0", "1.1", "2.0", "0.1"] {
                if text == domain.current_text() {
                    continue;
                }
                assert!(domain.require(text).unwrap_err().contains(domain.name()));
            }
            if domain.current_text() == "1.0-alpha.2" {
                assert!(domain.require("1.0-alpha.1").is_err());
            }
            assert!(domain.require("1").is_err());
        }
    }
    // Test-ID: PR-TEST-0620
    // Verifies: PR-REQ-0333
    #[test]
    fn release_selection_uses_precedence_and_never_implicitly_downgrades() {
        let versions = [
            "2.0.0",
            "1.1.0-alpha.1",
            "1.0.0-rc.1",
            "1.0.0-alpha.10",
            "1.0.0-beta.1",
            "1.0.0",
            "1.0.0-alpha.9",
        ]
        .map(|s| s.parse::<ProductVersion>().unwrap());
        let stable = ReleaseSelection::Major {
            major: 1,
            allow_prerelease: false,
        };
        let preview = ReleaseSelection::Major {
            major: 1,
            allow_prerelease: true,
        };
        assert_eq!(stable.select(versions).unwrap().to_string(), "1.0.0");
        assert_eq!(
            preview.select(versions).unwrap().to_string(),
            "1.1.0-alpha.1"
        );
        assert!(
            stable
                .update("1.1.0-alpha.1".parse().unwrap(), versions)
                .is_none()
        );
        assert!(
            ReleaseSelection::Major {
                major: 2,
                allow_prerelease: true
            }
            .update("1.0.0".parse().unwrap(), versions)
            .is_none()
        );
        assert!(
            stable
                .select(versions.into_iter().filter(|v| v.prerelease.is_some()))
                .is_none()
        );
        let exact = "1.0.0-alpha.9".parse().unwrap();
        assert_eq!(ReleaseSelection::Exact(exact).select(versions), Some(exact));
        assert!(
            ReleaseSelection::Exact("1.0.1".parse().unwrap())
                .select(versions)
                .is_none()
        );
        let mut ordered = versions;
        ordered.sort();
        assert_eq!(
            ordered.map(|v| v.to_string()),
            [
                "1.0.0-alpha.9",
                "1.0.0-alpha.10",
                "1.0.0-beta.1",
                "1.0.0-rc.1",
                "1.0.0",
                "1.1.0-alpha.1",
                "2.0.0"
            ]
        );
    }
}
