const ERROR_NAME_MAX_BYTES: usize = 128;

/// A stable Pactrun-owned machine error identity.
///
/// Presentation text and implementation error variants are deliberately not
/// part of this value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PactrunErrorRef {
    owner: String,
    code: String,
}

impl PactrunErrorRef {
    pub(crate) fn new(owner: impl Into<String>, code: impl Into<String>) -> Result<Self, String> {
        let owner = owner.into();
        let code = code.into();
        if !is_error_name(&owner) {
            return Err(format!("invalid error owner {owner:?}"));
        }
        if !is_error_name(&code) {
            return Err(format!("invalid error code {code:?}"));
        }
        Ok(Self { owner, code })
    }

    pub(crate) fn owner(&self) -> &str {
        &self.owner
    }

    pub(crate) fn code(&self) -> &str {
        &self.code
    }
}

fn is_error_name(value: &str) -> bool {
    if value.is_empty() || value.len() > ERROR_NAME_MAX_BYTES || !value.is_ascii() {
        return false;
    }
    let bytes = value.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut previous_separator = false;
    for &byte in &bytes[1..] {
        let separator = byte == b'_';
        if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || separator)
            || (separator && previous_separator)
        {
            return false;
        }
        previous_separator = separator;
    }
    !previous_separator
}
