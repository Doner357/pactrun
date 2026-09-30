//! CLI-only naming for Pactrun envelopes. Raw Input/Artifact export is unchanged.
use super::{CliError, OsString, Path, PathBuf};

pub(super) enum Format {
    Pack,
    Snapshot,
}

pub(super) fn destination(mut base: OsString, format: Format) -> Result<PathBuf, CliError> {
    let bytes = base.as_encoded_bytes();
    let directory_spelling = bytes.ends_with(b"/")
        || bytes.ends_with(b"/.")
        || cfg!(windows) && (bytes.ends_with(b"\\") || bytes.ends_with(b"\\."));
    if base.is_empty()
        || base == "-"
        || Path::new(&base).file_name().is_none()
        || directory_spelling
    {
        return Err(CliError::usage(
            "--output requires a base filename, not a directory or stdout; the format extension is always appended",
        ));
    }
    // Do not use set_extension, detect an existing suffix, or inspect the base
    // as a destination. Only the appended final path participates in publication.
    base.push(match format {
        Format::Pack => ".pack",
        Format::Snapshot => ".snapshot",
    });
    Ok(PathBuf::from(base))
}

#[cfg(test)]
#[path = "export_paths_tests.rs"]
mod tests;
