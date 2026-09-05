use crate::{
    domain::RevisionCoreV1Error,
    strict_json::{self, StrictJsonErrorKind},
};

const MAX_JSON_BYTES: usize = 16 * 1024 * 1024;
pub(super) use crate::strict_json::RawJsonValue;

pub(super) fn parse_json(bytes: &[u8]) -> Result<RawJsonValue, RevisionCoreV1Error> {
    strict_json::parse_json(bytes, MAX_JSON_BYTES).map_err(|error| match error.kind() {
        StrictJsonErrorKind::InvalidUtf8 | StrictJsonErrorKind::InvalidUnicodeScalar => {
            RevisionCoreV1Error::stable("invalid_unicode_scalar", error.message())
        }
        StrictJsonErrorKind::DuplicateProperty => {
            RevisionCoreV1Error::stable("duplicate_property", error.message())
        }
        StrictJsonErrorKind::InvalidJson => {
            RevisionCoreV1Error::internal("invalid_json_syntax", error.message())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_parser_rejects_duplicate_decoded_keys_and_invalid_surrogates() {
        let duplicate = parse_json(br#"{"a":1,"\u0061":2}"#).unwrap_err();
        assert_eq!(duplicate.internal_code(), "duplicate_property");
        assert_eq!(duplicate.stable_ref().unwrap().code(), "duplicate_property");

        let high = parse_json(br#"{"value":"\uD800"}"#).unwrap_err();
        assert_eq!(high.internal_code(), "invalid_unicode_scalar");
        let low = parse_json(br#"{"value":"\uDC00"}"#).unwrap_err();
        assert_eq!(low.internal_code(), "invalid_unicode_scalar");
        assert!(parse_json(br#"{"value":"\uD83D\uDE80"}"#).is_ok());
    }
}
