//! Publish a fully initialized private Store without adopting an existing path.
use super::{PactrunPersistence, PersistenceError, validate_supported_storage_root};
use std::{
    ffi::OsStr,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

fn io_error(source: io::Error) -> PersistenceError {
    PersistenceError::Io {
        operation: "prepare new Store",
        source,
    }
}

pub(crate) fn prepare_new_store(requested: &Path) -> Result<(), PersistenceError> {
    if !requested.is_absolute() {
        return Err(PersistenceError::InvalidMetadata(
            "Store path must be absolute".into(),
        ));
    }
    match fs::symlink_metadata(requested) {
        Ok(_) => return Ok(()), // Existing paths are validated, never repaired here.
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(io_error(e)),
    }
    let parent = requested
        .parent()
        .ok_or_else(|| io_error(io::Error::other("Store path has no parent")))?;
    let final_name = requested
        .file_name()
        .ok_or_else(|| io_error(io::Error::other("Store path has no name")))?;
    // Qualify the existing filesystem before creating missing parent directories.
    let mut ancestor = parent;
    loop {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => {
                validate_supported_storage_root(ancestor)?;
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                ancestor = ancestor.parent().ok_or_else(|| io_error(e))?;
            }
            Err(e) => return Err(io_error(e)),
        }
    }
    fs::create_dir_all(parent).map_err(io_error)?;
    let parent = validate_supported_storage_root(parent)?;
    let parent_handle = open_parent(&parent).map_err(io_error)?;
    let (temporary, temporary_name, temporary_handle) = create_temporary(&parent, &parent_handle)?;
    let destination = parent.join(final_name);
    let result = (|| {
        let mut children = Vec::new();
        for child in ["database", "runtime-content", "staging"] {
            children.push(create_child(&temporary_handle, child).map_err(io_error)?);
        }
        // Close all leases and SQLite connections before publishing the directory.
        drop(PactrunPersistence::open(&temporary)?);
        for child in children {
            sync_directory(&child).map_err(io_error)?;
        }
        sync_directory(&temporary_handle).map_err(io_error)?;
        match publish(
            &parent_handle,
            &temporary_name,
            &temporary_handle,
            final_name,
            &destination,
        ) {
            Ok(()) => sync_parent(&parent_handle).map_err(io_error),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                // A concurrent initializer may have won. The caller performs full
                // Store validation, including refusal of an unrelated destination.
                Ok(())
            }
            Err(e) => Err(io_error(e)),
        }
    })();
    drop(temporary_handle);
    // Only our unpredictable, private sibling is eligible for this cleanup.
    // Publication never replaces the destination, including an empty directory.
    if temporary.parent() == Some(parent.as_path())
        && temporary.file_name() == Some(OsStr::new(&temporary_name))
    {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

fn create_temporary(
    parent: &Path,
    handle: &File,
) -> Result<(PathBuf, String, File), PersistenceError> {
    for _ in 0..32 {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).map_err(|e| io_error(io::Error::other(e)))?;
        let name = format!(".pactrun-init-{}", hex::encode(random));
        match create_child(handle, &name) {
            Ok(file) => return Ok((parent.join(&name), name, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_error(e)),
        }
    }
    Err(io_error(io::Error::other(
        "unable to allocate a private Store preparation directory",
    )))
}

#[cfg(windows)]
fn open_parent(path: &Path) -> io::Result<File> {
    pactrun_windows_ntfs::open_root(path)
}
#[cfg(windows)]
fn create_child(parent: &File, name: &str) -> io::Result<File> {
    pactrun_windows_ntfs::create_service_directory(parent, name)
}
#[cfg(windows)]
fn publish(
    _parent: &File,
    _name: &str,
    file: &File,
    _final_name: &OsStr,
    destination: &Path,
) -> io::Result<()> {
    pactrun_windows_ntfs::rename_no_replace(file, destination)
}
#[cfg(windows)]
fn sync_parent(_parent: &File) -> io::Result<()> {
    Ok(())
}
#[cfg(windows)]
fn sync_directory(directory: &File) -> io::Result<()> {
    pactrun_windows_ntfs::flush(directory)
}

#[cfg(target_os = "linux")]
fn open_parent(path: &Path) -> io::Result<File> {
    use rustix::fs::{Mode, OFlags};
    Ok(rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?
    .into())
}
#[cfg(target_os = "linux")]
fn create_child(parent: &File, name: &str) -> io::Result<File> {
    use rustix::fs::{Mode, OFlags};
    rustix::fs::mkdirat(parent, name, Mode::RWXU)?;
    let child: File = rustix::fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?
    .into();
    child.sync_all()?;
    parent.sync_all()?;
    Ok(child)
}
#[cfg(target_os = "linux")]
fn publish(
    parent: &File,
    name: &str,
    _file: &File,
    final_name: &OsStr,
    _destination: &Path,
) -> io::Result<()> {
    rustix::fs::renameat_with(
        parent,
        name,
        parent,
        final_name,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    Ok(())
}
#[cfg(target_os = "linux")]
fn sync_parent(parent: &File) -> io::Result<()> {
    parent.sync_all()
}
#[cfg(target_os = "linux")]
fn sync_directory(directory: &File) -> io::Result<()> {
    directory.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::PactrunApplication;

    fn temporary() -> tempfile::TempDir {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/store-bootstrap-tests");
        fs::create_dir_all(&base).unwrap();
        tempfile::tempdir_in(base).unwrap()
    }

    // Test-ID: PR-TEST-0671
    // Verifies: PR-REQ-0374
    #[test]
    fn fresh_store_is_published_ready_without_manual_directories() {
        let temp = temporary();
        let root = temp.path().join("nested/store");
        drop(PactrunApplication::open(&root).unwrap());
        assert!(
            PactrunApplication::open_read_only(&root)
                .unwrap()
                .list_instances()
                .unwrap()
                .is_empty()
        );
        assert!(root.join("database/pactrun.sqlite3").is_file());
        assert_eq!(fs::read_dir(root.parent().unwrap()).unwrap().count(), 1);
    }

    // Test-ID: PR-TEST-0672
    // Verifies: PR-REQ-0374
    #[test]
    fn existing_incomplete_store_and_files_are_not_repaired_or_replaced() {
        let temp = temporary();
        let root = temp.path().join("existing");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("sentinel"), b"keep").unwrap();
        assert!(PactrunApplication::open(&root).is_err());
        assert_eq!(fs::read(root.join("sentinel")).unwrap(), b"keep");
        assert!(!root.join("database").exists());
        let file = temp.path().join("file");
        fs::write(&file, b"keep").unwrap();
        assert!(PactrunApplication::open(&file).is_err());
        assert_eq!(fs::read(file).unwrap(), b"keep");
    }

    // Test-ID: PR-TEST-0673
    // Verifies: PR-REQ-0374
    #[test]
    fn concurrent_bootstrap_does_not_replace_a_published_store() {
        let temp = temporary();
        let root = temp.path().join("store");
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..3)
                .map(|_| scope.spawn(|| prepare_new_store(&root)))
                .collect();
            for handle in handles {
                handle.join().unwrap().unwrap();
            }
        });
        assert!(PactrunApplication::open_read_only(&root).is_ok());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }
}
