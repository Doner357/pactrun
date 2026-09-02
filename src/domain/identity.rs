use std::{fmt, str::FromStr};

const OPAQUE_ID_BYTES: usize = 16;
const OPAQUE_ID_HEX_LENGTH: usize = OPAQUE_ID_BYTES * 2;
const SHA256_BYTES: usize = 32;
const SHA256_HEX_LENGTH: usize = SHA256_BYTES * 2;

macro_rules! opaque_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub(crate) struct $name([u8; OPAQUE_ID_BYTES]);

        impl $name {
            pub(crate) fn generate() -> Result<Self, getrandom::Error> {
                let mut bytes = [0_u8; OPAQUE_ID_BYTES];
                getrandom::fill(&mut bytes)?;
                Ok(Self(bytes))
            }

            pub(crate) fn from_bytes(bytes: [u8; OPAQUE_ID_BYTES]) -> Self {
                Self(bytes)
            }

            pub(crate) fn as_bytes(&self) -> &[u8; OPAQUE_ID_BYTES] {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&hex::encode(self.0))
            }
        }

        impl FromStr for $name {
            type Err = String;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                parse_lower_hex::<OPAQUE_ID_BYTES>(value, OPAQUE_ID_HEX_LENGTH).map(Self)
            }
        }
    };
}

opaque_id!(PackageId);
opaque_id!(InstanceId);
opaque_id!(InstanceStateVersion);
opaque_id!(ManagedInputPayloadId);

/// The SHA-256 content identity owned by RevisionCoreFormatV1.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct RevisionContentDigest([u8; SHA256_BYTES]);

impl RevisionContentDigest {
    pub(crate) fn from_bytes(bytes: [u8; SHA256_BYTES]) -> Self {
        Self(bytes)
    }

    pub(crate) fn as_bytes(&self) -> &[u8; SHA256_BYTES] {
        &self.0
    }
}

impl fmt::Display for RevisionContentDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "sha256:{}", hex::encode(self.0))
    }
}

impl FromStr for RevisionContentDigest {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let hex = value
            .strip_prefix("sha256:")
            .ok_or_else(|| "RevisionContentDigest must use sha256 prefix".to_owned())?;
        parse_lower_hex::<SHA256_BYTES>(hex, SHA256_HEX_LENGTH).map(Self)
    }
}

/// Exact Revision identity; no flattened spelling is a second identity form.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct RevisionIdentity {
    pub(crate) package_id: PackageId,
    pub(crate) content_digest: RevisionContentDigest,
}

impl RevisionIdentity {
    pub(crate) fn new(package_id: PackageId, content_digest: RevisionContentDigest) -> Self {
        Self {
            package_id,
            content_digest,
        }
    }
}

fn parse_lower_hex<const N: usize>(value: &str, expected_length: usize) -> Result<[u8; N], String> {
    if value.len() != expected_length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(format!(
            "expected {expected_length} lowercase hexadecimal characters"
        ));
    }
    let decoded = hex::decode(value).map_err(|error| error.to_string())?;
    decoded
        .try_into()
        .map_err(|_| format!("expected {N} decoded bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test-ID: PR-TEST-0039
    // Verifies: PR-REQ-0225
    #[test]
    fn opaque_identity_spelling_and_structure_are_exact() {
        let package = PackageId::from_bytes([0xab; 16]);
        let instance = InstanceId::from_bytes([0xcd; 16]);
        let state = InstanceStateVersion::from_bytes([0xef; 16]);
        assert_eq!(package.to_string(), "ab".repeat(16));
        assert_eq!(instance.to_string(), "cd".repeat(16));
        assert_eq!(state.to_string(), "ef".repeat(16));
        assert_eq!(package.to_string().parse::<PackageId>(), Ok(package));
        assert!(
            "AB000000000000000000000000000000"
                .parse::<PackageId>()
                .is_err()
        );
        assert!("00".parse::<InstanceId>().is_err());

        let digest = RevisionContentDigest::from_bytes([0x12; 32]);
        assert_eq!(digest.to_string(), format!("sha256:{}", "12".repeat(32)));
        let exact = RevisionIdentity::new(package, digest);
        assert_eq!(exact.package_id, package);
        assert_eq!(exact.content_digest, digest);
        assert_ne!(
            PackageId::generate().unwrap(),
            PackageId::generate().unwrap()
        );
        assert_ne!(
            InstanceId::generate().unwrap(),
            InstanceId::generate().unwrap()
        );
        assert_ne!(
            InstanceStateVersion::generate().unwrap(),
            InstanceStateVersion::generate().unwrap()
        );
    }
}
