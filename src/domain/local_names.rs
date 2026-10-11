//! Local names and two-part selectors. Names never replace immutable identity.
use super::{PackageId, RevisionIdentity};
use std::{collections::BTreeSet, fmt};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct LocalName(String);

#[derive(Clone, Debug, Default)]
pub(crate) struct InstallNames {
    pub(crate) package: Option<LocalName>,
    pub(crate) revision: Option<LocalName>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LocalRevisionFacts {
    pub(crate) package_name: Option<LocalName>,
    pub(crate) revision_name: Option<LocalName>,
    pub(crate) installed_at_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RevisionInstallReceipt {
    pub(crate) identity: RevisionIdentity,
    pub(crate) local: LocalRevisionFacts,
    pub(crate) reference: String,
    pub(crate) newly_installed: bool,
}

/// Self-contained catalog position: deletion of the last row cannot lose its time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RevisionCursor {
    pub(crate) identity: RevisionIdentity,
    pub(crate) installed_at_unix_ms: Option<i64>,
}
impl fmt::Display for RevisionCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "r1:{}:{}:{}",
            self.installed_at_unix_ms
                .map(|v| v.to_string())
                .unwrap_or_else(|| "u".into()),
            self.identity.package_id,
            hex::encode(self.identity.content_digest.as_bytes())
        )
    }
}
impl std::str::FromStr for RevisionCursor {
    type Err = NameError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() > 120 {
            return Err(NameError::InvalidReference);
        }
        let parts: Vec<_> = value.split(':').collect();
        if parts.len() != 4 || parts[0] != "r1" {
            return Err(NameError::InvalidReference);
        }
        let installed_at_unix_ms = if parts[1] == "u" {
            None
        } else {
            let time = parts[1]
                .parse::<i64>()
                .map_err(|_| NameError::InvalidReference)?;
            if time < 0 || time.to_string() != parts[1] {
                return Err(NameError::InvalidReference);
            }
            Some(time)
        };
        Ok(Self {
            identity: RevisionIdentity::new(
                parts[2].parse().map_err(|_| NameError::InvalidReference)?,
                format!("sha256:{}", parts[3])
                    .parse()
                    .map_err(|_| NameError::InvalidReference)?,
            ),
            installed_at_unix_ms,
        })
    }
}

impl LocalName {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, NameError> {
        let value = value.into();
        if value.len() > 31
            || !value
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            return Err(NameError::InvalidName);
        }
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NameError {
    InvalidName,
    InvalidReference,
    NotFound,
    Ambiguous,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidName => "names must be 1..31 ASCII characters, start with a letter or digit, and contain only letters, digits, '-', '_' or '.'",
            Self::InvalidReference => "Revision reference must be <package>:<revision>, with exactly one ':'",
            Self::NotFound => "reference does not identify an installed object",
            Self::Ambiguous => "reference matches multiple identities; use a longer or complete ID reference",
        })
    }
}
impl std::error::Error for NameError {}

/// Text can match a local name, an ID prefix, or both. Neither wins implicitly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LocalSelector(String);
impl LocalSelector {
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub(crate) fn id_prefix(&self, full_length: usize) -> Option<&str> {
        let text = self.as_str();
        (text.len() >= 8
            && text.len() <= full_length
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f')))
        .then_some(text)
    }
    pub(crate) fn parse(text: impl Into<String>) -> Result<Self, NameError> {
        Self::parse_component(text.into(), 32)
    }

    fn parse_component(text: String, id_length: usize) -> Result<Self, NameError> {
        let is_id = (8..=id_length).contains(&text.len())
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'));
        if is_id || LocalName::parse(text.clone()).is_ok() {
            Ok(Self(text))
        } else {
            Err(NameError::InvalidReference)
        }
    }

    pub(crate) fn matches(&self, id_hex: &str, name: Option<&LocalName>) -> bool {
        let text = self.0.as_str();
        name.is_some_and(|name| name.as_str() == text)
            || (text.len() >= 8
                && text.len() <= id_hex.len()
                && text
                    .bytes()
                    .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
                && id_hex.starts_with(text))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LocalRevisionReference {
    package: LocalSelector,
    revision: LocalSelector,
}
impl LocalRevisionReference {
    pub(crate) fn components(&self) -> (&LocalSelector, &LocalSelector) {
        (&self.package, &self.revision)
    }
    pub(crate) fn parse(text: &str) -> Result<Self, NameError> {
        let (package, revision) = text.split_once(':').ok_or(NameError::InvalidReference)?;
        if package.is_empty() || revision.is_empty() || revision.contains(':') {
            return Err(NameError::InvalidReference);
        }
        Ok(Self {
            package: LocalSelector::parse(package)?,
            revision: LocalSelector::parse_component(revision.to_owned(), 64)?,
        })
    }

    pub(crate) fn matches(
        &self,
        identity: &RevisionIdentity,
        package_name: Option<&LocalName>,
        revision_name: Option<&LocalName>,
    ) -> bool {
        self.package
            .matches(&identity.package_id.to_string(), package_name)
            && self.revision.matches(
                &hex::encode(identity.content_digest.as_bytes()),
                revision_name,
            )
    }

    pub(crate) fn resolve<'a>(
        &self,
        candidates: impl IntoIterator<
            Item = (
                &'a RevisionIdentity,
                Option<&'a LocalName>,
                Option<&'a LocalName>,
            ),
        >,
    ) -> Result<RevisionIdentity, NameError> {
        unique(
            candidates
                .into_iter()
                .filter(|(identity, package, revision)| self.matches(identity, *package, *revision))
                .map(|(identity, _, _)| identity.clone()),
        )
    }
}

pub(crate) fn resolve_package<'a>(
    selector: &LocalSelector,
    candidates: impl IntoIterator<Item = (PackageId, Option<&'a LocalName>)>,
) -> Result<PackageId, NameError> {
    unique(
        candidates
            .into_iter()
            .filter_map(|(id, name)| selector.matches(&id.to_string(), name).then_some(id)),
    )
}

fn unique<T: Ord>(values: impl IntoIterator<Item = T>) -> Result<T, NameError> {
    let mut matches = BTreeSet::new();
    for value in values {
        matches.insert(value);
        if matches.len() > 1 {
            return Err(NameError::Ambiguous);
        }
    }
    matches.pop_first().ok_or(NameError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RevisionContentDigest;

    fn identity(package: u8, revision: u8) -> RevisionIdentity {
        RevisionIdentity::new(
            PackageId::from_bytes([package; 16]),
            RevisionContentDigest::from_bytes([revision; 32]),
        )
    }
    fn name(text: &str) -> LocalName {
        LocalName::parse(text).unwrap()
    }

    // Test-ID: PR-TEST-0656
    // Verifies: PR-REQ-0371
    #[test]
    fn names_are_exact_ascii_at_the_public_boundary_not_normalized() {
        for text in ["web", "Web", "v1.2", "test_2", "123", "deadbeef"] {
            assert_eq!(name(text).as_str(), text);
        }
        for text in [
            "",
            "-web",
            "_web",
            ".web",
            " web",
            "web ",
            "web:stable",
            "a/b",
            "a\\b",
            "a\n",
            "\u{4e2d}\u{6587}",
            "caf\u{e9}",
            "\u{ff21}",
        ] {
            assert_eq!(
                LocalName::parse(text),
                Err(NameError::InvalidName),
                "{text:?}"
            );
        }
        assert_ne!(name("web"), name("Web"));
    }

    // Test-ID: PR-TEST-0657
    // Verifies: PR-REQ-0371
    #[test]
    fn all_four_reference_combinations_resolve_to_the_same_identity() {
        let id = identity(0x12, 0x34);
        let package = name("web");
        let revision = name("stable");
        for left in [
            package.as_str().to_owned(),
            id.package_id.to_string(),
            "12121212".into(),
        ] {
            for right in [
                revision.as_str().to_owned(),
                hex::encode(id.content_digest.as_bytes()),
                "34343434".into(),
            ] {
                let reference = LocalRevisionReference::parse(&format!("{left}:{right}")).unwrap();
                assert_eq!(
                    reference.resolve([(&id, Some(&package), Some(&revision))]),
                    Ok(id.clone())
                );
            }
        }
        assert_eq!(
            LocalRevisionReference::parse("web:missing")
                .unwrap()
                .resolve([(&id, Some(&package), Some(&revision))]),
            Err(NameError::NotFound)
        );
    }

    // Test-ID: PR-TEST-0658
    // Verifies: PR-REQ-0371
    #[test]
    fn whole_reference_disambiguates_packages_and_deduplicates_identity() {
        let a = identity(0x12, 0x34);
        let b = identity(0x56, 0x78);
        let alias = name("12121212");
        let stable = name("stable");
        let other = name("other");
        let reference = LocalRevisionReference::parse("12121212:stable").unwrap();
        assert_eq!(
            reference.resolve([(&a, None, Some(&stable)), (&b, Some(&alias), Some(&other))]),
            Ok(a.clone())
        );
        assert_eq!(
            reference.resolve([(&a, None, Some(&stable)), (&b, Some(&alias), Some(&stable))]),
            Err(NameError::Ambiguous)
        );
        assert_eq!(
            reference.resolve([
                (&a, Some(&alias), Some(&stable)),
                (&a, Some(&alias), Some(&stable))
            ]),
            Ok(a)
        );
    }

    // Test-ID: PR-TEST-0659
    // Verifies: PR-REQ-0371
    #[test]
    fn revision_name_and_digest_matches_have_no_priority() {
        let a = identity(0x12, 0x34);
        let b = identity(0x12, 0x56);
        let name = name("34343434");
        let reference = LocalRevisionReference::parse("12121212:34343434").unwrap();
        assert_eq!(
            reference.resolve([(&a, None, None), (&b, None, Some(&name))]),
            Err(NameError::Ambiguous)
        );
        assert_eq!(reference.resolve([(&a, None, Some(&name))]), Ok(a));
    }

    // Test-ID: PR-TEST-0660
    // Verifies: PR-REQ-0371
    #[test]
    fn malformed_references_and_nonmatching_short_prefixes_are_not_guessed() {
        for text in [
            "web",
            ":stable",
            "web:",
            "web:sha256:abc",
            "alias:web",
            "label:web",
        ] {
            // Old-looking two-part spellings are names, not reserved prefixes.
            if matches!(text, "alias:web" | "label:web") {
                assert!(LocalRevisionReference::parse(text).is_ok());
            } else {
                assert!(LocalRevisionReference::parse(text).is_err());
            }
        }
        let id = identity(0x12, 0x34);
        assert_eq!(
            LocalRevisionReference::parse("1212121:34343434")
                .unwrap()
                .resolve([(&id, None, None)]),
            Err(NameError::NotFound)
        );
    }

    // Test-ID: PR-TEST-0669
    // Verifies: PR-REQ-0371
    #[test]
    fn name_length_limit_preserves_full_ids_without_limiting_references() {
        let mut left = [0_u8; 16];
        let mut right = left;
        left[15] = 0x21;
        right[15] = 0x22;
        let a = RevisionIdentity::new(
            PackageId::from_bytes(left),
            RevisionContentDigest::from_bytes([0x34; 32]),
        );
        let b = RevisionIdentity::new(PackageId::from_bytes(right), a.content_digest);
        assert!(LocalName::parse("a".repeat(31)).is_ok());
        for value in [
            "a".repeat(32),
            b.package_id.to_string(),
            hex::encode(a.content_digest.as_bytes()),
        ] {
            assert_eq!(LocalName::parse(value), Err(NameError::InvalidName));
        }
        let prefix_name = name(&a.package_id.to_string()[..31]);
        for id in [&a, &b] {
            let package = id.package_id.to_string();
            for width in 8..=32 {
                let reference = LocalRevisionReference::parse(&format!(
                    "{}:{}",
                    &package[..width],
                    hex::encode(id.content_digest.as_bytes())
                ))
                .unwrap();
                let actual = reference.resolve([(&a, Some(&prefix_name), None), (&b, None, None)]);
                if width == 32 {
                    assert_eq!(actual, Ok(id.clone()));
                } else {
                    assert_eq!(actual, Err(NameError::Ambiguous));
                }
            }
        }
        for text in [
            format!("{}:stable", "f".repeat(33)),
            format!("web:{}", "f".repeat(65)),
            format!("web:{}", "z".repeat(32)),
        ] {
            assert_eq!(
                LocalRevisionReference::parse(&text),
                Err(NameError::InvalidReference)
            );
        }
    }

    // Test-ID: PR-TEST-0661
    // Verifies: PR-REQ-0371
    #[test]
    fn package_commands_require_a_unique_package_without_a_revision_hint() {
        let a = identity(0x12, 0x34).package_id;
        let b = identity(0x56, 0x78).package_id;
        let name = name("12121212");
        let selector = LocalSelector::parse("12121212").unwrap();
        assert_eq!(
            resolve_package(&selector, [(a, None), (b, Some(&name))]),
            Err(NameError::Ambiguous)
        );
        assert_eq!(
            resolve_package(&selector, [(a, Some(&name)), (a, None)]),
            Ok(a)
        );
    }
}
