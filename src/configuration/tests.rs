use super::*;

fn scratch() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/alpha8-config-tests");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}

// Test-ID: PR-TEST-0738
// Verifies: PR-REQ-0393, PR-REQ-0394, PR-REQ-0395
#[test]
fn defaults_are_typed_and_ownership_is_closed() {
    let defaults = parse("", "", &Overrides::default()).unwrap();
    assert_eq!(defaults.presentation.verbosity.value, Verbosity::Normal);
    assert_eq!(defaults.presentation.verbosity.source, Source::Default);
    assert_eq!(defaults.diagnostics.max_bytes.value, 256 * MIB);
    assert_eq!(defaults.startup_diagnostics.max_bytes.value, 64 * MIB);
    assert!(defaults.diagnostics.retain_hook_text.value);
    assert_eq!(defaults.diagnostics.retention_days.value, 30);
    for bad in [
        "version = 2",
        "version = -1",
        "version = '1'",
        "version = 1\nversion = 1",
        "[diagnostics]\nlevel = 'detailed'",
        "[presentation]\nverbosity = 'verbose'",
        "[presentation]\nphase_messages = 'false'",
        "unknown = 'sensitive-sentinel'",
    ] {
        let e = parse(bad, "", &Overrides::default()).unwrap_err();
        assert_eq!(e.scope, "user_file");
        assert!(!e.to_string().contains("sensitive-sentinel"));
    }
    assert!(
        parse(
            "",
            "[presentation]\nverbosity = 'quiet'",
            &Overrides::default()
        )
        .is_err()
    );
    assert!(parse("", "[diagnostics]\nunknown = true", &Overrides::default()).is_err());
}

// Test-ID: PR-TEST-0739
// Verifies: PR-REQ-0394, PR-REQ-0395
#[test]
fn cli_overrides_do_not_invent_a_policy_lock_or_file_sources() {
    let user = "[presentation]\nverbosity = 'quiet'\n[startup_diagnostics]\nretention_days = 7";
    let store = "[diagnostics]\nretain_hook_text = false\nlevel = 'detailed'";
    let overrides = Overrides {
        verbosity: Some(Verbosity::Detailed),
        retain_hook_text: Some(true),
        ..Overrides::default()
    };
    let resolved = parse(user, store, &overrides).unwrap();
    assert_eq!(resolved.presentation.verbosity.value, Verbosity::Detailed);
    assert_eq!(resolved.presentation.verbosity.source, Source::CommandLine);
    assert_eq!(resolved.presentation.phase_messages.source, Source::Default);
    assert_eq!(
        resolved.startup_diagnostics.retention_days.source,
        Source::UserFile
    );
    assert_eq!(resolved.diagnostics.level.source, Source::StoreFile);
    assert!(resolved.diagnostics.retain_hook_text.value);
    assert_eq!(
        resolved.diagnostics.retain_hook_text.source,
        Source::CommandLine
    );
    let serialized = serde_json::to_value(resolved).unwrap();
    assert_eq!(serialized["diagnostics"]["level"]["source"], "store_file");
}

// Test-ID: PR-TEST-0740
// Verifies: PR-REQ-0394
#[test]
fn retention_bounds_are_inclusive_without_unlimited_sentinels() {
    for days in [1, 3650] {
        for bytes in [MIB, 1024 * MIB] {
            let fields = format!("retention_days = {days}\nmax_bytes = {bytes}");
            parse(
                &format!("[startup_diagnostics]\n{fields}"),
                &format!("[diagnostics]\n{fields}"),
                &Overrides::default(),
            )
            .unwrap();
        }
    }
    for fields in [
        "retention_days = 0",
        "retention_days = 3651",
        "retention_days = -1",
        "max_bytes = 1048575",
        "max_bytes = 1073741825",
        "max_bytes = '64MiB'",
        "max_bytes = 1.5",
    ] {
        assert!(
            parse(
                "",
                &format!("[diagnostics]\n{fields}"),
                &Overrides::default()
            )
            .is_err()
        );
        assert!(
            parse(
                &format!("[startup_diagnostics]\n{fields}"),
                "",
                &Overrides::default()
            )
            .is_err()
        );
    }
}

// Test-ID: PR-TEST-0741
// Verifies: PR-REQ-0393, PR-REQ-0395
#[test]
fn reads_are_bounded_and_never_provision_a_store() {
    let temp = scratch();
    let missing = FileSelection {
        path: temp.path().join("missing/store/pactrun.toml"),
        required: false,
    };
    load(None, Some(&missing), &Overrides::default()).unwrap();
    assert!(!temp.path().join("missing").exists());
    let required = FileSelection {
        path: missing.path,
        required: true,
    };
    assert!(load(None, Some(&required), &Overrides::default()).is_err());
    let file = FileSelection {
        path: temp.path().join("config.toml"),
        required: true,
    };
    std::fs::write(&file.path, " ".repeat(DOCUMENT_LIMIT as usize)).unwrap();
    load(Some(&file), None, &Overrides::default()).unwrap();
    std::fs::write(&file.path, " ".repeat(DOCUMENT_LIMIT as usize + 1)).unwrap();
    assert!(load(Some(&file), None, &Overrides::default()).is_err());
    std::fs::write(&file.path, [0xff]).unwrap();
    assert!(load(Some(&file), None, &Overrides::default()).is_err());
    assert!(
        load(
            Some(&FileSelection {
                path: temp.path().to_owned(),
                required: true
            }),
            None,
            &Overrides::default()
        )
        .is_err()
    );
}

// Test-ID: PR-TEST-0742
// Verifies: PR-REQ-0393
#[test]
fn discovery_separates_normal_test_and_absent_environment() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/config-base");
    let normal = default_user_path(Some(Path::new("pactrun.exe")), |_| {
        Some(root.clone().into())
    })
    .unwrap()
    .unwrap();
    let test = default_user_path(Some(Path::new("pactrun-test.exe")), |_| {
        Some(root.clone().into())
    })
    .unwrap()
    .unwrap();
    assert!(normal.ends_with("pactrun/config.toml"));
    assert!(test.ends_with("pactrun-test/config.toml"));
    assert_ne!(normal, test);
    assert!(default_user_path(None, |_| None).unwrap().is_none());
    assert!(default_user_path(None, |_| Some(OsString::new())).is_err());
    assert!(default_user_path(None, |_| Some("relative".into())).is_err());
}

// Test-ID: PR-TEST-0743
// Verifies: PR-REQ-0393
#[cfg(unix)]
#[test]
fn symlinks_and_fifos_are_refused_without_reading_or_blocking() {
    let temp = scratch();
    let target = temp.path().join("real.toml");
    std::fs::write(&target, "version = 1").unwrap();
    let link = temp.path().join("link.toml");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let file = FileSelection {
        path: link,
        required: true,
    };
    assert!(load(Some(&file), None, &Overrides::default()).is_err());
    let fifo = temp.path().join("fifo.toml");
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    assert!(
        load(
            Some(&FileSelection {
                path: fifo,
                required: true
            }),
            None,
            &Overrides::default()
        )
        .is_err()
    );
}
