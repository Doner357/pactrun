//! Shared private staging -> same-directory atomic no-clobber publication.
use crate::managed_data::StagedFile;
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputPublicationPoint {
    CopyChunk,
    BeforeFinalNamePublication,
    AfterFinalNamePublication,
}

pub(crate) fn publish(source: &StagedFile, destination: &Path) -> io::Result<()> {
    publish_with_fault(source, destination, |_| Ok(()))
}

/// Carries publication truth even when the subsequent durability barrier fails.
#[derive(Debug)]
pub(crate) struct PublicationFailure {
    pub(crate) destination_published: bool,
    pub(crate) source: io::Error,
}

pub(crate) fn publish_reported(
    source: &StagedFile,
    destination: &Path,
) -> Result<(), PublicationFailure> {
    publish_reported_with_fault(source, destination, |_| Ok(()))
}

pub(crate) fn publish_pack(
    source: &StagedFile,
    destination: &Path,
    cancellation: &crate::hook::ActionCancellation,
) -> Result<(), PublicationFailure> {
    let mut gate = None;
    publish_reported_with_fault(source, destination, |point| match point {
        OutputPublicationPoint::CopyChunk => crate::pack_transport::check_cancel(cancellation),
        OutputPublicationPoint::BeforeFinalNamePublication => {
            gate = Some(cancellation.lock_acceptance_gate());
            crate::pack_transport::check_cancel(cancellation)
        }
        OutputPublicationPoint::AfterFinalNamePublication => {
            gate.take();
            Ok(())
        }
    })
}

fn publish_reported_with_fault(
    source: &StagedFile,
    destination: &Path,
    mut fault: impl FnMut(OutputPublicationPoint) -> io::Result<()>,
) -> Result<(), PublicationFailure> {
    let mut destination_published = false;
    publish_with_fault(source, destination, |point| {
        if point == OutputPublicationPoint::AfterFinalNamePublication {
            destination_published = true;
        }
        fault(point)
    })
    .map_err(|source| PublicationFailure {
        destination_published,
        source,
    })
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
        let mut buffer = [0u8; 64 * 1024];
        loop {
            fault(OutputPublicationPoint::CopyChunk)?;
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            file.write_all(&buffer[..n])?;
        }
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
    // Cleanup of the staging name belongs after the publication truth boundary.
    // Failure to unlink that name must not hide an already-published destination.
    fs::hard_link(path, destination)
}

#[cfg(windows)]
fn sync_parent(_parent: &Path) -> io::Result<()> {
    Ok(())
}
#[cfg(target_os = "linux")]
fn sync_parent(parent: &Path) -> io::Result<()> {
    File::open(parent)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // Test-ID: PR-TEST-0448
    // Verifies: PR-REQ-0342
    #[test]
    fn export_publication_reports_commit_truth_and_never_clobbers() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/artifact-publication-tests");
        fs::create_dir_all(&base).unwrap();
        let temp = tempfile::tempdir_in(base).unwrap();
        fs::create_dir(temp.path().join("staging")).unwrap();
        let session = crate::managed_data::StagingSession::prepare(temp.path()).unwrap();
        let mut staged = session.create_managed_output_stage().unwrap();
        staged.writer().write_all(b"private-output").unwrap();
        staged.finish_operation_file().unwrap();
        for point in [
            OutputPublicationPoint::BeforeFinalNamePublication,
            OutputPublicationPoint::AfterFinalNamePublication,
        ] {
            let path = temp.path().join(format!("{point:?}"));
            let failure = publish_reported_with_fault(&staged, &path, |seen| {
                if seen == point {
                    Err(io::Error::other("injected"))
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
            let published = point == OutputPublicationPoint::AfterFinalNamePublication;
            assert_eq!(failure.destination_published, published);
            assert_eq!(path.exists(), published);
            if published {
                assert_eq!(fs::read(&path).unwrap(), b"private-output");
                let conflict = publish_reported(&staged, &path).unwrap_err();
                assert!(!conflict.destination_published);
                assert_eq!(conflict.source.kind(), io::ErrorKind::AlreadyExists);
                assert_eq!(fs::read(&path).unwrap(), b"private-output");
            }
        }
        assert!(fs::read_dir(temp.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".pactrun-export-")
        }));
    }

    // Test-ID: PR-TEST-0454
    // Verifies: PR-REQ-0342
    #[test]
    fn export_publication_process_loss_preserves_the_final_name_boundary() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/artifact-crash-tests");
        fs::create_dir_all(&base).unwrap();
        let temp = tempfile::tempdir_in(base).unwrap();
        for (phase, published) in [("before", false), ("after", true)] {
            let root = temp.path().join(phase);
            fs::create_dir_all(root.join("staging")).unwrap();
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "output_publication::tests::export_publication_crash_worker",
                    "--nocapture",
                ])
                .env("PACTRUN_EXPORT_CRASH_ROOT", &root)
                .env("PACTRUN_EXPORT_CRASH_PHASE", phase)
                .output()
                .unwrap();
            assert_eq!(
                child.status.code(),
                Some(73),
                "{}",
                String::from_utf8_lossy(&child.stderr)
            );
            let output = root.join("delivered");
            assert_eq!(output.exists(), published);
            if published {
                assert_eq!(fs::read(&output).unwrap(), b"complete-private-export");
            }
        }
    }

    #[test]
    fn export_publication_crash_worker() {
        let Some(root) = std::env::var_os("PACTRUN_EXPORT_CRASH_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let phase = std::env::var("PACTRUN_EXPORT_CRASH_PHASE").unwrap();
        let session = crate::managed_data::StagingSession::prepare(&root).unwrap();
        let mut staged = session.create_managed_output_stage().unwrap();
        staged
            .writer()
            .write_all(b"complete-private-export")
            .unwrap();
        staged.finish_operation_file().unwrap();
        let point = match phase.as_str() {
            "before" => OutputPublicationPoint::BeforeFinalNamePublication,
            "after" => OutputPublicationPoint::AfterFinalNamePublication,
            _ => panic!("unknown crash phase"),
        };
        let _ = publish_reported_with_fault(&staged, &root.join("delivered"), |seen| {
            if seen == point {
                // Exit immediately: no Rust destructor may clean up the staged state.
                std::process::exit(73);
            }
            Ok(())
        });
        panic!("publication did not reach the required crash boundary");
    }
}
