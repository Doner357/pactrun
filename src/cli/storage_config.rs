//! Entry configuration only; Store validation/preparation remains in Application.
use std::{ffi::OsString, path::Path};

pub(super) fn selected_root(
    executable: Option<&Path>,
    lookup: impl Fn(&str) -> Option<OsString>,
) -> Option<OsString> {
    if let Some(explicit) = lookup(super::STORAGE_ROOT_ENV) {
        return Some(explicit);
    }
    let name = executable?.file_stem()?.to_str()?;
    let app = match name {
        "pactrun" => "pactrun",
        "pactrun-test" => "pactrun-test",
        _ => return None,
    };
    #[cfg(windows)]
    let base = lookup("LOCALAPPDATA").map(std::path::PathBuf::from);
    #[cfg(not(windows))]
    let base = lookup("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| lookup("HOME").map(|home| Path::new(&home).join(".local/share")));
    base.filter(|path| path.is_absolute())
        .map(|path| path.join(app).into_os_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test-ID: PR-TEST-0626
    // Verifies: PR-REQ-0271, PR-REQ-0333
    #[test]
    fn normal_and_test_roots_are_distinct_and_explicit_values_are_never_replaced() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/entry-config");
        let lookup = |key: &str| {
            matches!(key, "LOCALAPPDATA" | "XDG_DATA_HOME" | "HOME")
                .then(|| base.clone().into_os_string())
        };
        assert_eq!(
            selected_root(Some(Path::new("pactrun")), lookup),
            Some(base.join("pactrun").into_os_string())
        );
        assert_eq!(
            selected_root(Some(Path::new("pactrun-test.exe")), lookup),
            Some(base.join("pactrun-test").into_os_string())
        );
        assert_eq!(selected_root(Some(Path::new("renamed")), lookup), None);
        for explicit in [
            OsString::new(),
            OsString::from("relative"),
            base.join("explicit").into_os_string(),
        ] {
            assert_eq!(
                selected_root(None, |key| (key == super::super::STORAGE_ROOT_ENV)
                    .then(|| explicit.clone())),
                Some(explicit)
            );
        }
        assert_eq!(
            selected_root(Some(Path::new("pactrun-test")), |_| None),
            None
        );
        assert_eq!(
            selected_root(Some(Path::new("pactrun-test")), |key| (key
                != super::super::STORAGE_ROOT_ENV)
                .then(|| "relative".into())),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn xdg_home_fallback_and_native_path_bytes_are_preserved() {
        use std::os::unix::ffi::OsStringExt;
        let base = OsString::from_vec(b"/fixture/non-unicode-\xff".to_vec());
        assert_eq!(
            selected_root(Some(Path::new("pactrun")), |key| (key == "HOME")
                .then(|| base.clone())),
            Some(
                Path::new(&base)
                    .join(".local/share/pactrun")
                    .into_os_string()
            )
        );
        assert_eq!(
            selected_root(Some(Path::new("pactrun")), |key| match key {
                "XDG_DATA_HOME" => Some(OsString::new()),
                "HOME" => Some(base.clone()),
                _ => None,
            }),
            None
        );
    }
}
