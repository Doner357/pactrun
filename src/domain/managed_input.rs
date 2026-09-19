use std::{cmp::Ordering, fmt};

use super::{InputIdentity, InstanceId, InstanceStateVersion, RevisionIdentity};

pub(crate) const MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1: u64 = 536_870_912;
pub(crate) const MANAGED_INPUT_CHUNK_BYTES_V1: usize = 1_048_576;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InstanceName(String);

impl InstanceName {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, ManagedInputError> {
        let value = value.into();
        let valid_length = !value.is_empty() && value.len() <= 128;
        let valid_scalars = !value.chars().any(|scalar| {
            matches!(scalar, '\u{0000}'..='\u{001f}' | '\u{007f}'..='\u{009f}' | '\u{2028}' | '\u{2029}')
        });
        if !valid_length || !valid_scalars {
            return Err(ManagedInputError::new(
                "InstanceName must contain 1 through 128 UTF-8 bytes without control or line-separator scalars",
            ));
        }
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl Ord for InstanceName {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}

impl PartialOrd for InstanceName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum ManagedInputProtection {
    Normal,
    Secret,
}

impl ManagedInputProtection {
    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Secret => 1,
        }
    }

    pub(crate) fn from_rank(rank: i64) -> Result<Self, ManagedInputError> {
        match rank {
            0 => Ok(Self::Normal),
            1 => Ok(Self::Secret),
            _ => Err(ManagedInputError::new(format!(
                "invalid Managed Input protection rank {rank}"
            ))),
        }
    }

    pub(crate) fn sticky(self, declared: Self) -> Self {
        self.max(declared)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManagedInputRole {
    Active { required: bool },
    Retained,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManagedInputBindingView {
    pub(crate) input_id: InputIdentity,
    pub(crate) role: ManagedInputRole,
    pub(crate) present: bool,
    pub(crate) protection: ManagedInputProtection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstanceView {
    pub(crate) id: InstanceId,
    pub(crate) name: InstanceName,
    pub(crate) active_revision: RevisionIdentity,
    pub(crate) state_version: InstanceStateVersion,
    pub(crate) required_inputs_satisfied: bool,
    pub(crate) recovery_guard: Option<super::RecoveryGuardView>,
    pub(crate) bindings: Vec<ManagedInputBindingView>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstanceSummary {
    pub(crate) id: InstanceId,
    pub(crate) name: InstanceName,
    pub(crate) active_revision: RevisionIdentity,
    pub(crate) state_version: InstanceStateVersion,
    pub(crate) required_inputs_satisfied: bool,
    pub(crate) recovery_guard: Option<super::RecoveryGuardView>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManagedInputError {
    message: String,
}

impl ManagedInputError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ManagedInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ManagedInputError {}

#[cfg(test)]
mod tests {
    use super::*;

    // Supporting domain coverage for PR-TEST-0074.
    // Verifies: PR-REQ-0263, PR-REQ-0265
    #[test]
    fn instance_names_and_payload_limits_are_exact() {
        assert!(InstanceName::parse("service-a").is_ok());
        assert!(InstanceName::parse("").is_err());
        assert!(InstanceName::parse("a".repeat(129)).is_err());
        assert!(InstanceName::parse("line\nfeed").is_err());
        assert_eq!(MANAGED_INPUT_PAYLOAD_MAX_BYTES_V1, 512 * 1024 * 1024);
        assert_eq!(MANAGED_INPUT_CHUNK_BYTES_V1, 1024 * 1024);
    }

    // Supporting domain coverage for PR-TEST-0076.
    // Verifies: PR-REQ-0034, PR-REQ-0264, PR-REQ-0266, PR-REQ-0268
    #[test]
    fn secret_protection_is_a_sticky_floor() {
        assert_eq!(
            ManagedInputProtection::Normal.sticky(ManagedInputProtection::Secret),
            ManagedInputProtection::Secret
        );
        assert_eq!(
            ManagedInputProtection::Secret.sticky(ManagedInputProtection::Normal),
            ManagedInputProtection::Secret
        );
    }
}
