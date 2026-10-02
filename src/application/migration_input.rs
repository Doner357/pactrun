//! Bounded, path-free acquisition facts. These are not retry or recovery authority.
use crate::{domain::*, managed_data::StagingError};
use std::{fmt, io};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Phase {
    OpenSource,
    StageInput,
}
impl Phase {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::OpenSource => "open_source",
            Self::StageInput => "stage_input",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Reason {
    NotFound,
    PermissionDenied,
    TooLarge,
    IoError,
    Unavailable,
}
impl Reason {
    pub(crate) fn from_io(error: &io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::NotFound => Self::NotFound,
            io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            _ => Self::IoError,
        }
    }
    pub(crate) fn from_staging(error: &StagingError) -> Self {
        match error {
            StagingError::Io { source, .. } => Self::from_io(source),
            StagingError::ManagedInputTooLarge => Self::TooLarge,
            StagingError::Unsupported(_) | StagingError::LengthOverflow => Self::Unavailable,
        }
    }
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::PermissionDenied => "permission_denied",
            Self::TooLarge => "too_large",
            Self::IoError => "io_error",
            Self::Unavailable => "unavailable",
        }
    }
    fn summary(self) -> &'static str {
        match self {
            Self::NotFound => "a required file or directory was not found",
            Self::PermissionDenied => "filesystem access was denied",
            Self::TooLarge => "Input exceeds the managed payload limit",
            Self::IoError => "an I/O operation failed",
            Self::Unavailable => "Input staging is unavailable",
        }
    }
}

#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) instance: InstanceId,
    pub(crate) target: RevisionIdentity,
    pub(crate) input: InputIdentity,
    pub(crate) phase: Phase,
    pub(crate) reason: Reason,
}
impl Failure {
    pub(crate) fn new(
        plan: &MigrationExecutionPlan,
        key: &MigrationTargetInput,
        phase: Phase,
        reason: Reason,
    ) -> Self {
        let target = plan
            .edges()
            .iter()
            .find(|edge| edge.bindings.target().content_digest == key.revision)
            .expect("acquisition key belongs to the compiled path")
            .bindings
            .target()
            .clone();
        Self {
            instance: plan.instance(),
            target,
            input: key.input.clone(),
            phase,
            reason,
        }
    }
}
impl fmt::Display for Failure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "Migration Input acquisition failed.\ninstance: {}\ntarget: exact:{}/{}\ninput: {}\nphase: {}\nreason: {} ({})\nNo Run was accepted. Check the supplied Input and staging access or size limit before deciding to retry; no automatic retry is authorized.",
            self.instance,
            self.target.package_id,
            self.target.content_digest,
            self.input.as_str(),
            self.phase.code(),
            self.reason.code(),
            self.reason.summary()
        )
    }
}

impl std::error::Error for Failure {}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0650
    // Verifies: PR-REQ-0315, PR-REQ-0359
    #[test]
    fn acquisition_reason_classification_does_not_disclose_native_error_text() {
        for (kind, expected) in [
            (io::ErrorKind::NotFound, Reason::NotFound),
            (io::ErrorKind::PermissionDenied, Reason::PermissionDenied),
            (io::ErrorKind::Other, Reason::IoError),
        ] {
            let error = StagingError::Io {
                operation: "private operation sentinel",
                source: io::Error::new(kind, "private-host-path secret-value"),
            };
            let reason = Reason::from_staging(&error);
            assert_eq!(reason, expected);
            assert!(!reason.code().contains("private"));
        }
        assert_eq!(
            Reason::from_staging(&StagingError::ManagedInputTooLarge),
            Reason::TooLarge
        );
        assert_eq!(
            Reason::from_staging(&StagingError::Unsupported("private secret".into())),
            Reason::Unavailable
        );
        assert_eq!(
            Reason::from_staging(&StagingError::LengthOverflow),
            Reason::Unavailable
        );
    }
}
