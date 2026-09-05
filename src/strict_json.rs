use std::{collections::BTreeSet, fmt};

use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, SeqAccess, Visitor},
};
use serde_json::value::RawValue;

const MAX_JSON_DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RawJsonValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<RawJsonValue>),
    Object(Vec<(String, RawJsonValue)>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StrictJsonErrorKind {
    InvalidUtf8,
    InvalidJson,
    DuplicateProperty,
    InvalidUnicodeScalar,
}

#[derive(Debug)]
pub(crate) struct StrictJsonError {
    kind: StrictJsonErrorKind,
    message: String,
}

impl StrictJsonError {
    fn new(kind: StrictJsonErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub(crate) fn kind(&self) -> StrictJsonErrorKind {
        self.kind
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for StrictJsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for StrictJsonError {}

pub(crate) fn parse_json(
    bytes: &[u8],
    maximum_bytes: usize,
) -> Result<RawJsonValue, StrictJsonError> {
    if bytes.len() > maximum_bytes {
        return Err(StrictJsonError::new(
            StrictJsonErrorKind::InvalidJson,
            "JSON input exceeds the production codec size limit",
        ));
    }
    let input = std::str::from_utf8(bytes).map_err(|_| {
        StrictJsonError::new(
            StrictJsonErrorKind::InvalidUtf8,
            "JSON input is not valid UTF-8",
        )
    })?;
    validate_unicode_escapes(input.as_bytes())?;
    parse_raw_value(input, 0)
}

fn parse_raw_value(input: &str, depth: usize) -> Result<RawJsonValue, StrictJsonError> {
    if depth > MAX_JSON_DEPTH {
        return Err(StrictJsonError::new(
            StrictJsonErrorKind::InvalidJson,
            "JSON nesting exceeds the production codec depth limit",
        ));
    }
    let raw: Box<RawValue> = serde_json::from_str(input).map_err(json_syntax_error)?;
    let exact = raw.get();
    let first = exact.as_bytes().first().copied().ok_or_else(|| {
        StrictJsonError::new(StrictJsonErrorKind::InvalidJson, "empty JSON value")
    })?;
    match first {
        b'{' => {
            let RawObject(entries) = serde_json::from_str(exact).map_err(json_syntax_error)?;
            let mut keys = BTreeSet::new();
            let mut values = Vec::with_capacity(entries.len());
            for (key, raw) in entries {
                if !keys.insert(key.clone()) {
                    return Err(StrictJsonError::new(
                        StrictJsonErrorKind::DuplicateProperty,
                        format!("duplicate decoded property {key:?}"),
                    ));
                }
                values.push((key, parse_raw_value(raw.get(), depth + 1)?));
            }
            Ok(RawJsonValue::Object(values))
        }
        b'[' => {
            let RawArray(entries) = serde_json::from_str(exact).map_err(json_syntax_error)?;
            entries
                .into_iter()
                .map(|raw| parse_raw_value(raw.get(), depth + 1))
                .collect::<Result<Vec<_>, _>>()
                .map(RawJsonValue::Array)
        }
        b'"' => serde_json::from_str(exact)
            .map(RawJsonValue::String)
            .map_err(json_syntax_error),
        b't' => Ok(RawJsonValue::Bool(true)),
        b'f' => Ok(RawJsonValue::Bool(false)),
        b'n' => Ok(RawJsonValue::Null),
        b'-' | b'0'..=b'9' => Ok(RawJsonValue::Number(exact.to_owned())),
        _ => Err(StrictJsonError::new(
            StrictJsonErrorKind::InvalidJson,
            "unsupported JSON token",
        )),
    }
}

struct RawObject(Vec<(String, Box<RawValue>)>);

impl<'de> Deserialize<'de> for RawObject {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(RawObjectVisitor)
    }
}

struct RawObjectVisitor;

impl<'de> Visitor<'de> for RawObjectVisitor {
    type Value = RawObject;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0));
        while let Some(key) = map.next_key::<String>()? {
            entries.push((key, map.next_value::<Box<RawValue>>()?));
        }
        Ok(RawObject(entries))
    }
}

struct RawArray(Vec<Box<RawValue>>);

impl<'de> Deserialize<'de> for RawArray {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(RawArrayVisitor)
    }
}

struct RawArrayVisitor;

impl<'de> Visitor<'de> for RawArrayVisitor {
    type Value = RawArray;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON array")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut entries = Vec::with_capacity(sequence.size_hint().unwrap_or(0));
        while let Some(value) = sequence.next_element::<Box<RawValue>>()? {
            entries.push(value);
        }
        Ok(RawArray(entries))
    }
}

fn validate_unicode_escapes(bytes: &[u8]) -> Result<(), StrictJsonError> {
    let mut position = 0;
    while position < bytes.len() {
        if bytes[position] != b'"' {
            position += 1;
            continue;
        }
        position += 1;
        while position < bytes.len() {
            match bytes[position] {
                b'"' => {
                    position += 1;
                    break;
                }
                b'\\' => {
                    position += 1;
                    if position >= bytes.len() {
                        break;
                    }
                    if bytes[position] != b'u' {
                        position += 1;
                        continue;
                    }
                    let (first, next) = hex_quad(bytes, position + 1)?;
                    position = next;
                    if (0xd800..=0xdbff).contains(&first) {
                        if bytes.get(position..position + 2) != Some(br"\u") {
                            return Err(invalid_unicode("lone high surrogate"));
                        }
                        let (second, after_second) = hex_quad(bytes, position + 2)?;
                        if !(0xdc00..=0xdfff).contains(&second) {
                            return Err(invalid_unicode(
                                "high surrogate is not followed by low surrogate",
                            ));
                        }
                        position = after_second;
                    } else if (0xdc00..=0xdfff).contains(&first) {
                        return Err(invalid_unicode("lone low surrogate"));
                    }
                }
                _ => position += 1,
            }
        }
    }
    Ok(())
}

fn hex_quad(bytes: &[u8], start: usize) -> Result<(u16, usize), StrictJsonError> {
    let Some(slice) = bytes.get(start..start + 4) else {
        return Err(StrictJsonError::new(
            StrictJsonErrorKind::InvalidJson,
            "incomplete Unicode escape",
        ));
    };
    let mut value = 0_u16;
    for byte in slice {
        let digit = match byte {
            b'0'..=b'9' => u16::from(byte - b'0'),
            b'a'..=b'f' => u16::from(byte - b'a' + 10),
            b'A'..=b'F' => u16::from(byte - b'A' + 10),
            _ => {
                return Err(StrictJsonError::new(
                    StrictJsonErrorKind::InvalidJson,
                    "invalid Unicode escape",
                ));
            }
        };
        value = value * 16 + digit;
    }
    Ok((value, start + 4))
}

fn invalid_unicode(message: &str) -> StrictJsonError {
    StrictJsonError::new(StrictJsonErrorKind::InvalidUnicodeScalar, message)
}

fn json_syntax_error(error: serde_json::Error) -> StrictJsonError {
    StrictJsonError::new(StrictJsonErrorKind::InvalidJson, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_decoded_keys_and_invalid_surrogates() {
        assert_eq!(
            parse_json(br#"{"a":1,"\u0061":2}"#, usize::MAX)
                .unwrap_err()
                .kind(),
            StrictJsonErrorKind::DuplicateProperty
        );
        assert_eq!(
            parse_json(br#"{"value":"\uD800"}"#, usize::MAX)
                .unwrap_err()
                .kind(),
            StrictJsonErrorKind::InvalidUnicodeScalar
        );
        assert!(parse_json(br#"{"value":"\uD83D\uDE80"}"#, usize::MAX).is_ok());
    }
}
