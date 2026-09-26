//! Optional release provenance is supplied by the verified local build workflow.
//! This does not read Git credentials or infer a clean source commit.
use std::{env, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for (key, len) in [
        ("PACTRUN_BUILD_SOURCE_COMMIT", 40),
        ("PACTRUN_BUILD_SOURCE_MANIFEST", 64),
    ] {
        println!("cargo:rerun-if-env-changed={key}");
        if let Ok(value) = env::var(key) {
            assert!(
                value.len() == len
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "invalid supplied release provenance"
            );
            println!("cargo:rustc-env={key}={value}");
        }
    }
    let compiler = env::var_os("RUSTC").expect("Cargo supplies the compiler path");
    let output = Command::new(compiler)
        .arg("--version")
        .output()
        .expect("read compiler identity");
    assert!(output.status.success(), "read compiler identity");
    let version = String::from_utf8(output.stdout).expect("UTF-8 compiler identity");
    let version = version.trim();
    assert!(
        version.len() <= 256 && !version.chars().any(char::is_control),
        "invalid compiler identity"
    );
    println!("cargo:rustc-env=PACTRUN_BUILD_RUSTC={version}");
    println!(
        "cargo:rustc-env=PACTRUN_BUILD_TARGET={}",
        env::var("TARGET").expect("Cargo target")
    );
}
