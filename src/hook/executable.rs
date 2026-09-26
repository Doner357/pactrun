//! Execution-scoped access to the image that is actually running.
//! Never canonicalize the Linux locator back to the removable install path.
use std::{io, path::PathBuf};

pub(super) fn current() -> io::Result<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        // The caller remains alive while its child/Hook uses this locator.
        // This is not durable discovery; do not cache it across process/Session lifetimes.
        let executable = PathBuf::from(format!("/proc/{}/exe", std::process::id()));
        std::fs::metadata(&executable)?;
        Ok(executable)
    }
    #[cfg(not(target_os = "linux"))]
    {
        std::env::current_exe()
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::{env, fs, os::unix::fs::PermissionsExt, process::Command};

    // Test-ID: PR-TEST-0616
    // Verifies: PR-REQ-0349
    #[test]
    fn running_image_survives_unlink_and_installed_path_replacement() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("image-probe");
        fs::create_dir_all(&root).unwrap();
        let temp = tempfile::tempdir_in(root).unwrap();
        let copy = temp.path().join("copied-test-executable");
        fs::copy(env::current_exe().unwrap(), &copy).unwrap();
        let output = Command::new(&copy)
            .args([
                "--exact",
                "hook::executable::tests::running_image_worker",
                "--nocapture",
            ])
            .env("PACTRUN_IMAGE_PROBE_STAGE", "owner")
            .env("PACTRUN_IMAGE_PROBE_COPY", &copy)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("same running image"));
    }

    // Subprocess worker; the normal test runner intentionally has no stage.
    #[test]
    fn running_image_worker() {
        match env::var("PACTRUN_IMAGE_PROBE_STAGE").as_deref() {
            Ok("child") => println!("same running image"),
            Ok("owner") => {
                let copy =
                    std::path::PathBuf::from(env::var_os("PACTRUN_IMAGE_PROBE_COPY").unwrap());
                assert_eq!(
                    fs::canonicalize(&copy).unwrap(),
                    env::current_exe().unwrap()
                );
                assert!(copy.starts_with(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/image-probe")
                ));
                fs::remove_file(&copy).unwrap();
                fs::write(&copy, b"#!/bin/sh\necho wrong-replacement-image\nexit 93\n").unwrap();
                fs::set_permissions(&copy, fs::Permissions::from_mode(0o700)).unwrap();
                assert_eq!(
                    Command::new(&copy).output().unwrap().status.code(),
                    Some(93)
                );
                let output = Command::new(super::current().unwrap())
                    .args([
                        "--exact",
                        "hook::executable::tests::running_image_worker",
                        "--nocapture",
                    ])
                    .env("PACTRUN_IMAGE_PROBE_STAGE", "child")
                    .output()
                    .unwrap();
                assert!(output.status.success());
                let text = String::from_utf8_lossy(&output.stdout);
                assert!(text.contains("same running image"));
                assert!(!text.contains("wrong-replacement-image"));
                println!("same running image");
            }
            _ => {}
        }
    }
}
