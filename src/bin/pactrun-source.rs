//! Narrow native package-source selection. Never installs or updates a program.
#[allow(dead_code)]
#[path = "../domain/versioning.rs"]
mod versioning;
use std::{
    env,
    ffi::OsStr,
    fs,
    path::Path,
    process::{Command, ExitCode},
};
use versioning::ProductVersion;

fn source_ref(selection: &[String]) -> Result<String, String> {
    match selection {
        [flag, version] if flag == "--exact" => {
            let version: ProductVersion = version
                .parse()
                .map_err(|e| format!("invalid exact product version: {e}"))?;
            Ok(format!("version-{version}"))
        }
        [flag, major, eligibility]
            if flag == "--major" && matches!(eligibility.as_str(), "--stable" | "--preview") =>
        {
            let major: u64 = major.parse().map_err(|_| "invalid Major")?;
            if selection[1] != major.to_string() {
                return Err("Major must be canonical decimal".into());
            }
            Ok(format!(
                "major-{major}-{}",
                if eligibility == "--stable" {
                    "stable"
                } else {
                    "preview"
                }
            ))
        }
        _ => Err("use --exact VERSION or --major N --stable|--preview".into()),
    }
}
fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|_| "Git is required for native package source selection")?;
    if !output.status.success() {
        return Err(
            "native Git source operation failed; inspect the trusted source with Git".into(),
        );
    }
    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|_| "invalid Git output".into())
}
fn read_small(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let metadata = fs::symlink_metadata(path).map_err(|_| "source metadata is missing")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("source metadata must be regular files".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| "cannot read source metadata")?
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read source metadata")?;
    if bytes.len() as u64 > maximum {
        return Err("source metadata exceeds its bound".into());
    }
    Ok(bytes)
}
fn select(root: &Path, branch: &str) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|_| "package source does not exist")?;
    let git_dir = root
        .join(".git")
        .canonicalize()
        .map_err(|_| "source must be a native standalone Git clone")?;
    if !git_dir.starts_with(&root) || !git_dir.is_dir() {
        return Err(
            "external Git directories and worktrees are not supported source clones".into(),
        );
    }
    let marker = root.join("pactrun-package-source.txt");
    let marker_bytes = read_small(&marker, 128)?;
    // This is a text marker in a native Git worktree, not canonical product data.
    // Git's Windows autocrlf checkout may legitimately use CRLF.
    if marker_bytes != b"pactrun-native-package-source\n"
        && marker_bytes != b"pactrun-native-package-source\r\n"
    {
        return Err("not a Pactrun native package source".into());
    }
    if !git(
        &root,
        &[
            "status",
            "--porcelain",
            "--ignored",
            "--untracked-files=all",
        ],
    )?
    .is_empty()
    {
        return Err("source has local changes; selection did not change".into());
    }
    if git(
        &root,
        &["rev-list", "--count", "HEAD", "--not", "--remotes"],
    )? != "0"
    {
        return Err("source has unpublished local commits; selection did not change".into());
    }
    if !git(
        &root,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads/pactrun-selected",
        ],
    )?
    .is_empty()
        && git(
            &root,
            &[
                "rev-list",
                "--count",
                "pactrun-selected",
                "--not",
                "--remotes",
            ],
        )? != "0"
    {
        return Err(
            "selection branch has unpublished local commits; selection did not change".into(),
        );
    }
    let major_file = root.join("pactrun-source-major.txt");
    let scope = String::from_utf8(read_small(&major_file, 24)?)
        .map_err(|_| "invalid source Major scope")?;
    let scope = scope.trim_end_matches(['\r', '\n']);
    let major = scope
        .parse::<u64>()
        .map_err(|_| "invalid source Major scope")?;
    if scope != major.to_string() {
        return Err("invalid source Major scope".into());
    }
    let selected_major = if let Some(version) = branch.strip_prefix("version-") {
        version
            .parse::<ProductVersion>()
            .map_err(|_| "invalid exact source")?
            .major()
    } else {
        branch
            .strip_prefix("major-")
            .and_then(|value| value.split_once('-'))
            .and_then(|(value, _)| value.parse::<u64>().ok())
            .ok_or("invalid Major source")?
    };
    if selected_major != major {
        return Err("ordinary source updates cannot cross Major; use a separate Major-scoped source with an explicit native install/switch".into());
    }
    let remote = format!("refs/remotes/origin/{branch}");
    let refspec = format!("refs/heads/{branch}:{remote}");
    // Only fetch source metadata. The native manager alone acquires/activates binaries.
    git(&root, &["fetch", "origin", &refspec])?;
    if git(
        &root,
        &["show", &format!("{remote}:pactrun-source-major.txt")],
    )? != scope
    {
        return Err("published source Major does not match the selected scope".into());
    }
    let target = git(&root, &["rev-parse", &remote])?;
    let tracking = format!("origin/{branch}");
    git(
        &root,
        &[
            "switch",
            "--force-create",
            "pactrun-selected",
            "--track",
            &tracking,
        ],
    )?;
    // Homebrew update follows origin/HEAD; checkout alone is insufficient.
    git(
        &root,
        &["symbolic-ref", "refs/remotes/origin/HEAD", &remote],
    )?;
    if git(&root, &["rev-parse", "HEAD"])? != target
        || git(
            &root,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
        )? != tracking
        || git(&root, &["symbolic-ref", "refs/remotes/origin/HEAD"])? != remote
    {
        return Err("native source tracking verification failed; no program was installed".into());
    }
    Ok(())
}
fn run() -> Result<(), String> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|s| s == OsStr::new("--help")) {
        println!(
            "Usage: pactrun-source --source-root PATH (--major N --stable|--preview | --exact VERSION)\nChanges native Git source metadata only. Run Scoop/Homebrew separately to install, update or explicitly switch programs."
        );
        return Ok(());
    }
    if args.len() < 4 || args[0] != OsStr::new("--source-root") {
        return Err(
            "expected --source-root PATH and explicit release selection; use --help".into(),
        );
    }
    let selection = args[2..]
        .iter()
        .map(|s| {
            s.to_str()
                .map(str::to_owned)
                .ok_or_else(|| "selection must be ASCII".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let branch = source_ref(&selection)?;
    select(Path::new(&args[1]), &branch)?;
    println!(
        "Selected native source {branch}; installed programs and management data are unchanged."
    );
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pactrun source selection: {error}");
            ExitCode::FAILURE
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0628
    // Verifies: PR-REQ-0333
    #[test]
    fn source_selection_accepts_only_exact_versions_or_explicit_major_eligibility() {
        let args = |values: &[&str]| values.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            source_ref(&args(&["--major", "1", "--preview"])).unwrap(),
            "major-1-preview"
        );
        assert_eq!(
            source_ref(&args(&["--major", "1", "--stable"])).unwrap(),
            "major-1-stable"
        );
        assert_eq!(
            source_ref(&args(&["--exact", "1.0.0-alpha.10"])).unwrap(),
            "version-1.0.0-alpha.10"
        );
        for values in [
            vec!["--major", "01", "--preview"],
            vec!["--major", "1", "--anything"],
            vec!["--major", "1"],
            vec!["--exact", "--upload-pack=evil"],
            vec!["--exact", "1.0"],
            vec!["--exact", "1.0.0; command"],
        ] {
            assert!(source_ref(&args(&values)).is_err());
        }
    }
    // Test-ID: PR-TEST-0631
    // Verifies: PR-REQ-0333
    #[test]
    fn source_selection_really_updates_native_tracking_without_changing_installed_programs() {
        use std::fs;
        let parent =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target/native-source-selection-tests");
        fs::create_dir_all(&parent).unwrap();
        let temp = tempfile::tempdir_in(&parent).unwrap();
        let publisher = temp.path().join("publisher");
        fs::create_dir(&publisher).unwrap();
        git(&publisher, &["init"]).unwrap();
        git(
            &publisher,
            &["config", "user.name", "Pactrun isolated source test"],
        )
        .unwrap();
        git(
            &publisher,
            &["config", "user.email", "test@example.invalid"],
        )
        .unwrap();
        fs::write(
            publisher.join("pactrun-package-source.txt"),
            b"pactrun-native-package-source\n",
        )
        .unwrap();
        fs::write(publisher.join("pactrun-source-major.txt"), b"1\n").unwrap();
        git(
            &publisher,
            &[
                "add",
                "pactrun-package-source.txt",
                "pactrun-source-major.txt",
            ],
        )
        .unwrap();
        git(&publisher, &["commit", "-m", "fixture source marker"]).unwrap();
        for (branch, value) in [
            ("version-1.0.0-alpha.1", "alpha"),
            ("major-1-preview", "preview"),
            ("major-1-stable", "stable"),
        ] {
            git(&publisher, &["switch", "--create", branch]).unwrap();
            fs::write(publisher.join("selection.txt"), value).unwrap();
            git(&publisher, &["add", "selection.txt"]).unwrap();
            git(&publisher, &["commit", "-m", "fixture selection"]).unwrap();
        }
        let source = temp.path().join("native-clone");
        let result = Command::new("git")
            .args(["clone", "--no-local"])
            .arg(&publisher)
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "isolated Git clone failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let installed = temp.path().join("installed-program");
        let managed = temp.path().join("managed-data");
        fs::write(&installed, b"current executable").unwrap();
        fs::write(&managed, b"service owned sentinel").unwrap();
        for (branch, value) in [
            ("version-1.0.0-alpha.1", "alpha"),
            ("major-1-preview", "preview"),
            ("major-1-stable", "stable"),
        ] {
            select(&source, branch).unwrap();
            assert_eq!(
                fs::read_to_string(source.join("selection.txt")).unwrap(),
                value
            );
            assert_eq!(
                git(&source, &["symbolic-ref", "refs/remotes/origin/HEAD"]).unwrap(),
                format!("refs/remotes/origin/{branch}")
            );
            assert_eq!(
                git(
                    &source,
                    &[
                        "rev-parse",
                        "--abbrev-ref",
                        "--symbolic-full-name",
                        "@{upstream}"
                    ]
                )
                .unwrap(),
                format!("origin/{branch}")
            );
            assert_eq!(fs::read(&installed).unwrap(), b"current executable");
            assert_eq!(fs::read(&managed).unwrap(), b"service owned sentinel");
        }
        assert!(
            select(&source, "major-2-preview")
                .unwrap_err()
                .contains("cannot cross Major")
        );
        assert!(
            select(&source, "version-2.0.0-alpha.1")
                .unwrap_err()
                .contains("cannot cross Major")
        );
        let before = git(&source, &["rev-parse", "HEAD"]).unwrap();
        assert!(select(&source, "version-1.0.0-alpha.99").is_err());
        assert_eq!(git(&source, &["rev-parse", "HEAD"]).unwrap(), before);
        fs::write(source.join("selection.txt"), b"operator edits").unwrap();
        assert!(select(&source, "major-1-preview").is_err());
        assert_eq!(
            fs::read(source.join("selection.txt")).unwrap(),
            b"operator edits"
        );
        assert_eq!(git(&source, &["rev-parse", "HEAD"]).unwrap(), before);
    }
}
