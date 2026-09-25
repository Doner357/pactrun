//! Bounded source guard, complementary to the recorded composition review.
//! This checks direct named dependencies, not arbitrary Rust macro expansion.
use std::{fs, path::Path};

fn forbidden_dependency(source: &str) -> bool {
    let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();
    [
        "crate::cli",
        "crate::persistence",
        "crate::application",
        "crate::hook",
        "crate::executor",
        "crate::authoring",
        "crate::workflow",
        "rusqlite::",
        "lexopt::",
        "yaml_rust2::",
        "staticmut",
    ]
    .iter()
    .any(|name| compact.contains(name))
}

// Test-ID: PR-TEST-0605
// Verifies: PR-REQ-0009, PR-REQ-0010
#[test]
fn domain_has_no_direct_adapter_dependencies() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut paths = vec![root.join("domain.rs")];
    paths.extend(
        fs::read_dir(root.join("domain"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension().is_some_and(|e| e == "rs")
                    && !path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .ends_with("_tests.rs")
            }),
    );
    assert!(
        paths.len() > 20,
        "the production Domain scan must not be empty"
    );
    for path in paths {
        let source = fs::read_to_string(&path).unwrap();
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(
            !forbidden_dependency(production),
            "forbidden direct dependency: {}",
            path.display()
        );
    }
    for forbidden in [
        "use crate :: persistence::Repository;",
        "type Db = rusqlite::Connection;",
        "use crate::cli as adapter;",
        "static mut STATE: u8 = 0;",
    ] {
        assert!(forbidden_dependency(forbidden), "guard failed: {forbidden}");
    }
    assert!(!forbidden_dependency(
        "use std::collections::BTreeMap; use serde::Serialize;"
    ));
}
