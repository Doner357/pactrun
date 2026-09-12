//! Shared private staging -> same-directory atomic no-clobber publication.
use crate::managed_data::StagedFile;
use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputPublicationPoint {
    BeforeFinalNamePublication,
    AfterFinalNamePublication,
}

pub(crate) fn publish(source: &StagedFile, destination: &Path) -> io::Result<()> {
    publish_with_fault(source, destination, |_| Ok(()))
}
pub(crate) fn publish_with_fault(
    source: &StagedFile,
    destination: &Path,
    mut fault: impl FnMut(OutputPublicationPoint) -> io::Result<()>,
) -> io::Result<()> {
    if destination.file_name().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "output path has no final file name",
        ));
    };
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "output destination already exists",
            ));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let (path, mut file) = create_temporary(parent)?;
    let result = (|| {
        let mut reader = source.try_clone_reader().map_err(io::Error::other)?;
        io::copy(&mut reader, &mut file)?;
        file.sync_all()?;
        fault(OutputPublicationPoint::BeforeFinalNamePublication)?;
        publish_no_replace(&file, &path, destination)?;
        fault(OutputPublicationPoint::AfterFinalNamePublication)?;
        sync_parent(parent)
    })();
    drop(file);
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    result
}
fn create_temporary(parent: &Path) -> io::Result<(PathBuf, File)> {
    for _ in 0..32 {
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(io::Error::other)?;
        let path = parent.join(format!(".pactrun-export-{}.tmp", hex::encode(random)));
        #[cfg(windows)]
        let result = pactrun_windows_ntfs::create_staging(&path);
        #[cfg(unix)]
        let result = {
            use std::os::unix::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
        };
        match result {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("could not allocate output temporary file"))
}
#[cfg(windows)]
fn publish_no_replace(file: &File, _path: &Path, destination: &Path) -> io::Result<()> {
    pactrun_windows_ntfs::rename_no_replace(file, destination)
}
#[cfg(target_os = "linux")]
fn publish_no_replace(_file: &File, path: &Path, destination: &Path) -> io::Result<()> {
    fs::hard_link(path, destination)?;
    fs::remove_file(path)
}
#[cfg(windows)]
fn sync_parent(_parent: &Path) -> io::Result<()> {
    Ok(())
}
#[cfg(target_os = "linux")]
fn sync_parent(parent: &Path) -> io::Result<()> {
    File::open(parent)?.sync_all()
}
