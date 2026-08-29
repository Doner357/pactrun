use std::{fs::File, io, path::Path};

pub(super) fn open_root(path: &Path) -> io::Result<File> {
    pactrun_windows_ntfs::open_root(path)
}

pub(super) fn open_lock(root: &Path) -> io::Result<File> {
    pactrun_windows_ntfs::open_lock(&root.join(".publish.lock"))
}

pub(super) fn create_staging(root: &Path, name: &str, _root_file: &File) -> io::Result<File> {
    pactrun_windows_ntfs::create_staging(&root.join(name))
}

pub(super) fn open_existing_durable(
    root: &Path,
    name: &str,
    _root_file: &File,
) -> io::Result<File> {
    pactrun_windows_ntfs::open_existing_for_durability(&root.join(name))
}

pub(super) fn open_existing_read(root: &Path, name: &str, _root_file: &File) -> io::Result<File> {
    pactrun_windows_ntfs::open_existing_read(&root.join(name))
}

pub(super) fn sync_file(file: &File) -> io::Result<()> {
    pactrun_windows_ntfs::flush(file)
}

pub(super) fn publish_no_replace(
    root: &Path,
    _root_file: &File,
    _staging_name: &str,
    staging: &File,
    final_name: &str,
) -> io::Result<()> {
    pactrun_windows_ntfs::rename_no_replace(staging, &root.join(final_name))
}

pub(super) fn sync_namespace(_root_file: &File) -> io::Result<()> {
    // NTFS flushes rename metadata as part of the write-through handle request.
    Ok(())
}

pub(super) fn remove_staging(root: &Path, name: &str, _root_file: &File) -> io::Result<()> {
    std::fs::remove_file(root.join(name))
}
