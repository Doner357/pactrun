pub(crate) fn is_semantic_identifier(value: &str) -> bool {
    if value.is_empty() || value.len() > 128 || !value.is_ascii() {
        return false;
    }
    let bytes = value.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut previous_separator = false;
    for &byte in &bytes[1..] {
        let separator = matches!(byte, b'.' | b'_' | b'-');
        if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || separator)
            || (separator && previous_separator)
        {
            return false;
        }
        previous_separator = separator;
    }
    !previous_separator
}

pub(crate) fn is_runtime_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && value.is_ascii()
        && value.split('/').all(|segment| {
            is_semantic_identifier(segment) && !is_windows_reserved_device_name(segment)
        })
}

pub(crate) fn is_windows_reserved_device_name(value: &str) -> bool {
    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_runtime_path_profile_is_exact() {
        for valid in ["a", "bin/hook.exe", "data/cache_1"] {
            assert!(is_runtime_path(valid), "{valid}");
        }
        for invalid in [
            "",
            "/data",
            "data/",
            "data//file",
            "data/./file",
            "data/../file",
            "data\\file",
            "C:/data",
            "data/CON.txt",
        ] {
            assert!(!is_runtime_path(invalid), "{invalid}");
        }
    }
}
