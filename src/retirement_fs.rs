//! Identity-qualified retirement. Linux uses a private, allocation-scoped
//! physical-progress journal; V8 remains the source of deletion authority.
use crate::{domain::ServiceAllocationId, service_storage};
use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(crate) struct HandoffLocation {
    pub(crate) path: PathBuf,
    pub(crate) original_relative_path: PathBuf,
}

// Tag 3 has tag 2's native Linux incarnation fields, but requires the isolated
// journal backend. Old readers reject it rather than infer deletion from the
// missing public pathname. Tag 4 similarly requires Windows namespace pinning.
// Upgrade the capability marker durably before physical retirement begins.
pub(crate) fn same_incarnation(a: &[u8; 40], b: &[u8; 40]) -> bool {
    a == b
        || (matches!((a[0], b[0]), (2, 3) | (3, 2) | (1, 4) | (4, 1))
            && a[1..8] == [0; 7]
            && b[1..8] == [0; 7]
            && a[8..] == b[8..])
}

pub(crate) struct AllocationRoot {
    parent: File,
    name: String,
    #[cfg(not(target_os = "linux"))]
    directory: File,
    identity: [u8; 40],
}
impl AllocationRoot {
    pub(crate) fn identity(&self) -> &[u8; 40] {
        &self.identity
    }
    pub(crate) fn remove(self) -> io::Result<()> {
        platform::remove(&self.parent, &self.name, &self.identity)?;
        #[cfg(not(target_os = "linux"))]
        {
            drop(self.directory);
            match service_storage::open_directory(&self.parent, &self.name) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "retired allocation path still exists",
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
fn open(root: &Path, allocation: ServiceAllocationId) -> io::Result<Option<AllocationRoot>> {
    open_qualified(root, allocation, None)
}

pub(crate) fn open_qualified(
    root: &Path,
    allocation: ServiceAllocationId,
    expected: Option<&[u8; 40]>,
) -> io::Result<Option<AllocationRoot>> {
    #[cfg(target_os = "linux")]
    {
        platform::open(root, allocation, expected)
    }
    #[cfg(not(target_os = "linux"))]
    {
        require_native_key(expected)?;
        let root = service_storage::open_root(root)?;
        reject_foreign_progress(&root, allocation)?;
        let parent = match platform::open_parent(&root) {
            Ok(parent) => parent,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let name = format!("alloc-{allocation}");
        let directory = match service_storage::open_directory(&parent, &name) {
            Ok(directory) => directory,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let identity = platform::identity(&directory)?;
        if expected.is_some_and(|key| !same_incarnation(key, &identity)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "allocation root was replaced",
            ));
        }
        Ok(Some(AllocationRoot {
            parent,
            name,
            directory,
            identity,
        }))
    }
}

pub(crate) fn handoff(
    root: &Path,
    allocation: ServiceAllocationId,
    expected: Option<&[u8; 40]>,
) -> io::Result<Vec<HandoffLocation>> {
    #[cfg(target_os = "linux")]
    {
        platform::handoff(root, allocation, expected)
    }
    #[cfg(not(target_os = "linux"))]
    {
        require_native_key(expected)?;
        let root_file = service_storage::open_root(root)?;
        reject_foreign_progress(&root_file, allocation)?;
        let file = (|| {
            let parent = service_storage::open_directory(&root_file, "service-storage")?;
            service_storage::open_directory(&parent, &format!("alloc-{allocation}"))
        })();
        match file {
            Ok(file) => {
                let key = platform::identity(&file)?;
                if expected.is_some_and(|expected| !same_incarnation(expected, &key)) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "allocation root was replaced",
                    ));
                }
                Ok(vec![HandoffLocation {
                    path: root
                        .join("service-storage")
                        .join(format!("alloc-{allocation}")),
                    original_relative_path: PathBuf::from("."),
                }])
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(e),
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn require_native_key(expected: Option<&[u8; 40]>) -> io::Result<()> {
    if let Some(key) = expected
        && (!matches!(
            u64::from_le_bytes(key[..8].try_into().expect("fixed tag")),
            1 | 4
        ) || key[32..] != [0; 8])
    {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "physical retirement evidence belongs to another platform",
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn reject_foreign_progress(root: &File, allocation: ServiceAllocationId) -> io::Result<()> {
    let area = match service_storage::open_directory(root, "retirement-v1") {
        Ok(area) => area,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    match service_storage::open_directory(&area, &format!("alloc-{allocation}")) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "allocation has platform-qualified retirement progress",
        )),
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    pub(super) fn open_parent(root: &File) -> io::Result<File> {
        pactrun_windows_ntfs::open_retirement_parent(root, "service-storage")
    }
    pub(super) fn identity(file: &File) -> io::Result<[u8; 40]> {
        pactrun_windows_ntfs::retirement_identity(file)
    }
    pub(super) fn remove(parent: &File, name: &str, expected: &[u8; 40]) -> io::Result<()> {
        pactrun_windows_ntfs::remove_retirement_tree(parent, name, expected)
    }
}
#[cfg(target_os = "linux")]
#[path = "retirement_fs_linux.rs"]
mod platform;

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::*;
    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "retirement filesystem unsupported",
        )
    }
    pub(super) fn open_parent(_: &File) -> io::Result<File> {
        Err(unsupported())
    }
    pub(super) fn identity(_: &File) -> io::Result<[u8; 40]> {
        Err(unsupported())
    }
    pub(super) fn remove(_: &File, _: &str, _: &[u8; 40]) -> io::Result<()> {
        Err(unsupported())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, std::path::PathBuf, ServiceAllocationId) {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m7-retirement-fs");
        std::fs::create_dir_all(&base).unwrap();
        let temp = tempfile::tempdir_in(base).unwrap();
        let id = ServiceAllocationId::generate().unwrap();
        let path = temp
            .path()
            .join("service-storage")
            .join(format!("alloc-{id}"));
        std::fs::create_dir_all(path.join("nested")).unwrap();
        std::fs::write(path.join("nested/data"), b"service bytes").unwrap();
        (temp, path, id)
    }

    #[cfg(target_os = "linux")]
    fn crash_at(
        root: &Path,
        id: ServiceAllocationId,
        key: &[u8; 40],
        point: &str,
        node: Option<&str>,
    ) {
        let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            "retirement_fs::tests::linux_retirement_crash_worker",
            "--nocapture",
        ])
        .env("PACTRUN_RETIREMENT_TEST_ROOT", root)
        .env("PACTRUN_RETIREMENT_TEST_ID", id.to_string())
        .env("PACTRUN_RETIREMENT_TEST_KEY", hex::encode(key))
        .env("PACTRUN_RETIREMENT_FAULT", point);
        if let Some(node) = node {
            cmd.env("PACTRUN_RETIREMENT_FAULT_NODE", node);
        }
        let output = cmd.output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(87),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Test-ID: PR-TEST-0439
    // Verifies: PR-REQ-0336, PR-REQ-0337
    #[cfg(target_os = "linux")]
    #[test]
    fn repeated_claim_interruption_keeps_handoff_paths_attached_to_actual_parent() {
        let (temp, _, id) = fixture();
        let key = *open(temp.path(), id).unwrap().unwrap().identity();
        crash_at(
            temp.path(),
            id,
            &key,
            "retirement_before_claim",
            Some("data"),
        );
        crash_at(
            temp.path(),
            id,
            &key,
            "retirement_after_claim",
            Some("nested"),
        );
        let locations = handoff(temp.path(), id, Some(&key)).unwrap();
        let leaf = locations
            .iter()
            .find(|l| l.original_relative_path == Path::new("nested/data"))
            .unwrap();
        assert_eq!(std::fs::read(&leaf.path).unwrap(), b"service bytes");
        open_qualified(temp.path(), id, Some(&key))
            .unwrap()
            .unwrap()
            .remove()
            .unwrap();
        assert!(
            open_qualified(temp.path(), id, Some(&key))
                .unwrap()
                .is_none()
        );
    }

    // Test-ID: PR-TEST-0444
    // Verifies: PR-REQ-0336, PR-REQ-0337
    #[cfg(target_os = "linux")]
    #[test]
    fn durable_move_receipt_never_revisits_a_handed_off_source_binding() {
        let (temp, _, id) = fixture();
        let key = *open(temp.path(), id).unwrap().unwrap().identity();
        crash_at(
            temp.path(),
            id,
            &key,
            "retirement_before_claim",
            Some("data"),
        );
        crash_at(
            temp.path(),
            id,
            &key,
            "retirement_source_sealed",
            Some("nested"),
        );
        let locations = handoff(temp.path(), id, Some(&key)).unwrap();
        let directory = locations
            .iter()
            .find(|l| l.original_relative_path == Path::new("nested"))
            .unwrap();
        let taken = temp.path().join("operator-taken-data");
        std::fs::rename(&directory.path, &taken).unwrap();
        open_qualified(temp.path(), id, Some(&key))
            .unwrap()
            .unwrap()
            .remove()
            .unwrap();
        assert_eq!(std::fs::read(taken.join("data")).unwrap(), b"service bytes");
        assert!(
            open_qualified(temp.path(), id, Some(&key))
                .unwrap()
                .is_none()
        );
    }

    // Test-ID: PR-TEST-0440
    // Verifies: PR-REQ-0336, PR-REQ-0337
    #[cfg(target_os = "linux")]
    #[test]
    fn previously_handed_off_frame_is_requalified_and_never_used_as_removal_parent() {
        let (temp, _, id) = fixture();
        let key = *open(temp.path(), id).unwrap().unwrap().identity();
        crash_at(temp.path(), id, &key, "retirement_claim_committed", None);
        let old = handoff(temp.path(), id, Some(&key)).unwrap().remove(0).path;
        let preserved = temp.path().join("preserved-handoff");
        let root = open_qualified(temp.path(), id, Some(&key))
            .unwrap()
            .unwrap();
        let mut changed = false;
        let result = platform::remove_observed(&root.parent, &root.name, &key, |_, _| {
            if !changed {
                std::fs::rename(&old, &preserved).unwrap();
                std::fs::create_dir(&old).unwrap();
                std::fs::write(old.join("replacement"), b"new entry").unwrap();
                changed = true;
            }
        });
        assert!(changed && result.is_err());
        assert_eq!(
            std::fs::read(preserved.join("nested/data")).unwrap(),
            b"service bytes"
        );
        assert_eq!(
            std::fs::read(old.join("replacement")).unwrap(),
            b"new entry"
        );
    }

    // Test-ID: PR-TEST-0441
    // Verifies: PR-REQ-0336, PR-REQ-0337, PR-REQ-0338
    #[cfg(target_os = "linux")]
    #[test]
    fn malformed_physical_progress_never_falls_back_to_the_public_name() {
        for invalid in ["syntax", "anchor", "claim", "unknown"] {
            let (temp, path, id) = fixture();
            let key = *open(temp.path(), id).unwrap().unwrap().identity();
            crash_at(temp.path(), id, &key, "retirement_after_claim", None);
            let kept = handoff(temp.path(), id, Some(&key)).unwrap().remove(0).path;
            let record = temp
                .path()
                .join("retirement-v1")
                .join(format!("alloc-{id}"))
                .join("records/root.json");
            let mut value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
            match invalid {
                "anchor" => value["anchor"] = serde_json::Value::String("0".repeat(80)),
                "claim" => value["claim"] = serde_json::Value::String("../outside".to_owned()),
                "unknown" => value["unexpected"] = serde_json::Value::Bool(true),
                _ => {}
            }
            std::fs::write(
                &record,
                if invalid == "syntax" {
                    b"{".to_vec()
                } else {
                    serde_json::to_vec(&value).unwrap()
                },
            )
            .unwrap();
            std::fs::create_dir(&path).unwrap();
            std::fs::write(path.join("replacement"), b"leave alone").unwrap();
            assert!(
                open_qualified(temp.path(), id, Some(&key)).is_err(),
                "{invalid}"
            );
            assert!(handoff(temp.path(), id, Some(&key)).is_err());
            assert_eq!(
                std::fs::read(kept.join("nested/data")).unwrap(),
                b"service bytes"
            );
            assert_eq!(
                std::fs::read(path.join("replacement")).unwrap(),
                b"leave alone"
            );
        }
    }

    // Test-ID: PR-TEST-0442
    // Verifies: PR-REQ-0336, PR-REQ-0337
    #[cfg(windows)]
    #[test]
    fn windows_namespace_guards_prevent_root_and_child_reparenting() {
        let (temp, path, id) = fixture();
        let root = open(temp.path(), id).unwrap().unwrap();
        let pinned =
            pactrun_windows_ntfs::open_retirement_directory(&root.parent, &root.name).unwrap();
        assert!(std::fs::rename(&path, temp.path().join("moved-root")).is_err());
        let child = pactrun_windows_ntfs::open_retirement_directory(&pinned, "nested").unwrap();
        assert!(
            open(temp.path(), id).unwrap().unwrap().remove().is_err(),
            "a conflicting holder must prevent traversal"
        );
        assert!(std::fs::rename(path.join("nested"), temp.path().join("moved-child")).is_err());
        assert_eq!(
            std::fs::read(path.join("nested/data")).unwrap(),
            b"service bytes"
        );
        drop(child);
        drop(pinned);
        root.remove().unwrap();
        assert!(!path.exists());
    }

    // Test-ID: PR-TEST-0443
    // Verifies: PR-REQ-0336, PR-REQ-0337
    #[test]
    fn retirement_unlinks_only_owned_hard_links_and_handles_readonly_data() {
        let (temp, path, id) = fixture();
        let outside = temp.path().join("outside-file");
        std::fs::write(&outside, b"shared bytes").unwrap();
        std::fs::hard_link(&outside, path.join("link")).unwrap();
        let readonly = path.join("read-only");
        std::fs::write(&readonly, b"owned readonly").unwrap();
        let mut permissions = std::fs::metadata(&readonly).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&readonly, permissions).unwrap();
        open(temp.path(), id).unwrap().unwrap().remove().unwrap();
        assert!(!path.exists());
        assert_eq!(std::fs::read(outside).unwrap(), b"shared bytes");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_retirement_crash_worker() {
        let Some(root) = std::env::var_os("PACTRUN_RETIREMENT_TEST_ROOT") else {
            return;
        };
        let root = std::fs::canonicalize(root).unwrap();
        let workspace = std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
        let base = std::fs::canonicalize(workspace.join("target/m7-retirement-fs")).unwrap();
        assert!(base.starts_with(&workspace) && root.starts_with(&base) && root != base);
        let allocation = std::env::var("PACTRUN_RETIREMENT_TEST_ID")
            .unwrap()
            .parse()
            .unwrap();
        let key: [u8; 40] = hex::decode(std::env::var("PACTRUN_RETIREMENT_TEST_KEY").unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        open_qualified(&root, allocation, Some(&key))
            .unwrap()
            .unwrap()
            .remove()
            .unwrap();
    }

    // Test-ID: PR-TEST-0433
    // Verifies: PR-REQ-0247, PR-REQ-0336, PR-REQ-0337
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_claim_crashes_resume_only_recorded_objects_and_preserve_replacements() {
        for (point, node) in [
            ("retirement_before_journal_publish", None),
            ("retirement_after_journal_publish", None),
            ("retirement_before_claim", None),
            ("retirement_after_claim", None),
            ("retirement_claim_committed", None),
            ("retirement_before_claim", Some("data")),
            ("retirement_after_claim", Some("nested")),
            ("retirement_after_claim", Some("data")),
            ("retirement_claim_committed", Some("data")),
            ("retirement_before_unlink", None),
            ("retirement_after_unlink", None),
        ] {
            let (temp, path, id) = fixture();
            let key = *open(temp.path(), id).unwrap().unwrap().identity();
            let mut worker = std::process::Command::new(std::env::current_exe().unwrap());
            worker
                .args([
                    "--exact",
                    "retirement_fs::tests::linux_retirement_crash_worker",
                    "--nocapture",
                ])
                .env("PACTRUN_RETIREMENT_TEST_ROOT", temp.path())
                .env("PACTRUN_RETIREMENT_TEST_ID", id.to_string())
                .env("PACTRUN_RETIREMENT_TEST_KEY", hex::encode(key))
                .env("PACTRUN_RETIREMENT_FAULT", point);
            if let Some(node) = node {
                worker.env("PACTRUN_RETIREMENT_FAULT_NODE", node);
            }
            let output = worker.output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(87),
                "{point}/{node:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let was_moved = !path.exists();
            if was_moved {
                std::fs::create_dir(&path).unwrap();
                std::fs::write(path.join("replacement"), b"unrelated bytes").unwrap();
            }
            // Read-only handoff covers every remaining fragment with native
            // names; it must not mistake a new public directory for the root.
            if point != "retirement_before_journal_publish" {
                let locations = handoff(temp.path(), id, Some(&key)).unwrap();
                assert!(!locations.is_empty(), "{point}");
                if point != "retirement_after_unlink" {
                    assert!(
                        locations.iter().any(|location| {
                            let suffix = if location.original_relative_path == Path::new(".") {
                                Path::new("nested/data")
                            } else {
                                let Ok(suffix) = Path::new("nested/data")
                                    .strip_prefix(&location.original_relative_path)
                                else {
                                    return false;
                                };
                                suffix
                            };
                            std::fs::read(if suffix.as_os_str().is_empty() {
                                location.path.clone()
                            } else {
                                location.path.join(suffix)
                            })
                            .is_ok_and(|bytes| bytes == b"service bytes")
                        }),
                        "remaining data was not handed off at {point}/{node:?}"
                    );
                }
            }
            open_qualified(temp.path(), id, Some(&key))
                .unwrap()
                .unwrap()
                .remove()
                .unwrap();
            assert!(
                open_qualified(temp.path(), id, Some(&key))
                    .unwrap()
                    .is_none()
            );
            if was_moved {
                assert_eq!(
                    std::fs::read(path.join("replacement")).unwrap(),
                    b"unrelated bytes"
                );
            } else {
                assert!(!path.exists());
            }
        }
    }

    // Test-ID: PR-TEST-0434
    // Verifies: PR-REQ-0336, PR-REQ-0337
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_claim_owner_excludes_handoff_and_handles_non_unicode_native_names() {
        use std::os::unix::ffi::OsStringExt;
        let (temp, path, id) = fixture();
        std::fs::write(
            path.join(std::ffi::OsString::from_vec(vec![b'x', 0xff])),
            b"native name",
        )
        .unwrap();
        let root = open(temp.path(), id).unwrap().unwrap();
        let key = *root.identity();
        let mut observed = false;
        platform::remove_observed(&root.parent, &root.name, &key, |_, _| {
            assert_eq!(
                handoff(temp.path(), id, Some(&key)).unwrap_err().kind(),
                io::ErrorKind::WouldBlock
            );
            observed = true;
        })
        .unwrap();
        assert!(observed);
        assert!(!path.exists());
    }

    // Test-ID: PR-TEST-0435
    // Verifies: PR-REQ-0336, PR-REQ-0337
    #[cfg(windows)]
    #[test]
    fn foreign_retirement_progress_is_not_mistaken_for_an_absent_allocation() {
        let (temp, path, id) = fixture();
        let root = open(temp.path(), id).unwrap().unwrap();
        let key = *root.identity();
        let progress = temp
            .path()
            .join("retirement-v1")
            .join(format!("alloc-{id}"));
        std::fs::create_dir_all(&progress).unwrap();
        std::fs::write(progress.join("preserved"), b"foreign custody").unwrap();
        assert!(open_qualified(temp.path(), id, Some(&key)).is_err());
        assert!(handoff(temp.path(), id, Some(&key)).is_err());
        assert_eq!(
            std::fs::read(path.join("nested/data")).unwrap(),
            b"service bytes"
        );
        assert_eq!(
            std::fs::read(progress.join("preserved")).unwrap(),
            b"foreign custody"
        );
    }
    #[test]
    fn owned_tree_removal_is_relative_and_replacement_is_rejected() {
        let (temp, path, id) = fixture();
        let root = open(temp.path(), id).unwrap().unwrap();
        let original = *root.identity();
        std::fs::rename(&path, temp.path().join("preserved")).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert_ne!(
            *open(temp.path(), id).unwrap().unwrap().identity(),
            original
        );
        assert!(root.remove().is_err());
        assert!(temp.path().join("preserved/nested/data").exists());
        assert!(path.exists());
        assert!(open_qualified(temp.path(), id, Some(&original)).is_err());
    }
    #[test]
    fn nested_service_bytes_are_removed_without_touching_a_sibling() {
        let (temp, path, id) = fixture();
        std::fs::write(temp.path().join("outside"), b"keep").unwrap();
        let root = open(temp.path(), id).unwrap().unwrap();
        let key = *root.identity();
        root.remove().unwrap();
        assert!(!path.exists());
        assert_eq!(std::fs::read(temp.path().join("outside")).unwrap(), b"keep");
        assert!(
            open_qualified(temp.path(), id, Some(&key))
                .unwrap()
                .is_none()
        );
    }

    // Test-ID: PR-TEST-0429
    // Verifies: PR-REQ-0336, PR-REQ-0337
    #[cfg(target_os = "linux")]
    #[test]
    fn namespace_swap_after_root_check_must_not_unlink_the_replacement() {
        let (temp, path, id) = fixture();
        let root = open(temp.path(), id).unwrap().unwrap();
        let preserved = temp.path().join("original-root");
        let mut swapped = false;
        let result = platform::remove_observed(
            &root.parent,
            &root.name,
            root.identity(),
            |name, directory| {
                if directory && name.to_bytes() == root.name.as_bytes() {
                    std::fs::rename(&path, &preserved).unwrap();
                    std::fs::create_dir(&path).unwrap();
                    swapped = true;
                }
            },
        );
        assert!(swapped, "the root unlink boundary was not exercised");
        assert!(preserved.is_dir());
        assert!(
            path.is_dir(),
            "a different directory was removed after qualification"
        );
        assert!(result.is_err(), "replacement must fail closed");
    }

    // Test-ID: PR-TEST-0430
    // Verifies: PR-REQ-0336, PR-REQ-0337
    #[cfg(target_os = "linux")]
    #[test]
    fn namespace_reparent_after_child_check_must_not_delete_outside_bytes() {
        let (temp, path, id) = fixture();
        let root = open(temp.path(), id).unwrap().unwrap();
        // A service can retain a handle to the original allocation even after
        // its public name is isolated. Exercise its ability to reparent a child.
        let old_root = File::open(&path).unwrap();
        let preserved = temp.path().join("outside-allocation");
        let mut moved = false;
        let result = platform::remove_observed(
            &root.parent,
            &root.name,
            root.identity(),
            |name, directory| {
                if directory && name.to_bytes() == b"nested" && !moved {
                    rustix::fs::renameat(&old_root, "nested", rustix::fs::CWD, &preserved).unwrap();
                    rustix::fs::mkdirat(&old_root, "nested", rustix::fs::Mode::RWXU).unwrap();
                    moved = true;
                }
            },
        );
        assert!(moved, "the child qualification boundary was not exercised");
        assert_eq!(
            std::fs::read(preserved.join("data")).ok().as_deref(),
            Some(b"service bytes".as_slice()),
            "an opened directory handle followed data outside the authorized allocation"
        );
        assert!(result.is_err(), "relocation must fail closed");
    }

    // Test-ID: PR-TEST-0431
    // Verifies: PR-REQ-0336, PR-REQ-0337
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_removal_ends_only_the_qualified_lifetime_and_keeps_completed_evidence() {
        // This proof needs an uncontended lock. Other parallel libtest cases can
        // fork while removal owns its open-file-description lock; CLOEXEC only
        // closes inherited descriptors at exec, not at fork. Isolate this proof
        // instead of weakening production contention checks or retrying errors.
        const CHILD: &str = "PACTRUN_TEST_RETIREMENT_LIFETIME_CHILD";
        if std::env::var_os(CHILD).as_deref() != Some(std::ffi::OsStr::new("1")) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "retirement_fs::tests::linux_removal_ends_only_the_qualified_lifetime_and_keeps_completed_evidence",
                    "--nocapture",
                    "--test-threads=1",
                    "--color=never",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"),
                "the isolated proof must actually execute"
            );
            return;
        }
        let (temp, path, id) = fixture();
        let outside = temp.path().join("outside");
        std::fs::write(&outside, b"outside bytes").unwrap();
        let root = open(temp.path(), id).unwrap().unwrap();
        let key = *root.identity();
        root.remove().unwrap();
        assert!(!path.exists());
        assert_eq!(std::fs::read(outside).unwrap(), b"outside bytes");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("replacement"), b"not authorized").unwrap();
        assert!(
            open_qualified(temp.path(), id, Some(&key))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            std::fs::read(path.join("replacement")).unwrap(),
            b"not authorized"
        );
    }

    // Test-ID: PR-TEST-0414
    // Verifies: PR-REQ-0247, PR-REQ-0336, PR-REQ-0337
    #[test]
    fn retirement_refuses_linked_subtrees_and_preserves_outside_bytes() {
        let (temp, path, id) = fixture();
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("sentinel"), b"outside custody").unwrap();
        let link = path.join("alias");
        #[cfg(target_os = "linux")]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let script = temp.path().join("junction.ps1");
            std::fs::write(&script, b"param([string]$Link,[string]$Target)\n$ErrorActionPreference='Stop'\nNew-Item -ItemType Junction -Path $Link -Target $Target | Out-Null\n").unwrap();
            assert!(
                std::process::Command::new("powershell.exe")
                    .args([
                        "-NoProfile",
                        "-NonInteractive",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-File"
                    ])
                    .arg(script)
                    .arg(&link)
                    .arg(&outside)
                    .creation_flags(0x0800_0000)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let root = open(temp.path(), id).unwrap().unwrap();
        let key = *root.identity();
        assert!(root.remove().is_err());
        let locations = handoff(temp.path(), id, Some(&key)).unwrap();
        assert!(!locations.is_empty());
        assert_eq!(
            std::fs::read(outside.join("sentinel")).unwrap(),
            b"outside custody"
        );
        // Remove only the test-created link, never its target, before TempDir
        // housekeeping so the test does not depend on recursive link behavior.
        #[cfg(windows)]
        std::fs::remove_dir(link).unwrap();
        #[cfg(target_os = "linux")]
        for location in locations {
            if location.path.join("alias").symlink_metadata().is_ok() {
                std::fs::remove_file(location.path.join("alias")).unwrap();
            }
        }
    }
}
