//! Explicit support-report selection never grants sensitive export authority.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RunExportOptions {
    pub(crate) hook_text: bool,
    pub(crate) artifacts: bool,
    pub(crate) inputs: bool,
    pub(crate) secrets: bool,
    pub(crate) authorize_sensitive: bool,
    pub(crate) authorize_secret: bool,
}
impl RunExportOptions {
    pub(crate) fn validate(self) -> Result<(), &'static str> {
        if (self.hook_text || self.artifacts) && !self.authorize_sensitive {
            return Err("Hook text and Artifact export require --authorize-sensitive-export");
        }
        if self.secrets && !self.authorize_secret {
            return Err("Secret export requires --authorize-secret-export");
        }
        Ok(())
    }
    pub(crate) fn includes(self, protection: super::ManagedInputProtection) -> bool {
        match protection {
            super::ManagedInputProtection::Normal => self.inputs,
            super::ManagedInputProtection::Secret => self.secrets,
        }
    }
}

// Test-ID: PR-TEST-0779
// Verifies: PR-REQ-0404
#[test]
fn run_export_selection_and_authorization_are_independent() {
    use super::ManagedInputProtection::{Normal, Secret};
    for bits in 0..64 {
        let options = RunExportOptions {
            hook_text: bits & 1 != 0,
            artifacts: bits & 2 != 0,
            inputs: bits & 4 != 0,
            secrets: bits & 8 != 0,
            authorize_sensitive: bits & 16 != 0,
            authorize_secret: bits & 32 != 0,
        };
        assert_eq!(
            options.validate().is_ok(),
            (!(options.hook_text || options.artifacts) || options.authorize_sensitive)
                && (!options.secrets || options.authorize_secret)
        );
        assert_eq!(options.includes(Normal), options.inputs);
        assert_eq!(options.includes(Secret), options.secrets);
    }
}
