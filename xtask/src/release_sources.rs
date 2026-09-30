//! Local native-source definitions. No network publication or program installation.
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
#[allow(dead_code)]
#[path = "../../src/domain/versioning.rs"]
mod versions;
use versions::{FormatVersion, ProductVersion, ReleaseSelection, VersionDomain};
#[allow(dead_code)]
#[path = "../../src/strict_json.rs"]
mod strict_json;
const MAX_INPUT: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Artifact {
    pub platform: String,
    pub flavor: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Release {
    pub product_version: String,
    pub source_commit: String,
    pub source_manifest_sha256: String,
    pub rustc: String,
    pub supported_formats: BTreeMap<String, Vec<String>>,
    pub default_formats: BTreeMap<String, String>,
    pub artifacts: Vec<Artifact>,
}
fn digest(text: &str, size: usize) -> bool {
    text.len() == size
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_url(url: &str) -> bool {
    if fluent_uri::Uri::parse(url).is_err()
        || url.len() > 4096
        || !url.is_ascii()
        || url.bytes().any(|b| b.is_ascii_control() || b == b' ')
    {
        return false;
    }
    let (secure, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        (false, rest)
    } else {
        return false;
    };
    let Some((authority, path)) = rest.split_once('/') else {
        return false;
    };
    !authority.is_empty()
        && !authority.contains('@')
        && !path.is_empty()
        && (secure
            || authority == "127.0.0.1"
            || authority == "localhost"
            || authority.starts_with("127.0.0.1:")
            || authority.starts_with("localhost:"))
}
fn validate(releases: &[Release]) -> Result<BTreeMap<ProductVersion, &Release>, String> {
    let mut result = BTreeMap::new();
    let names = VersionDomain::ALL
        .into_iter()
        .map(|d| d.name().to_owned())
        .collect::<BTreeSet<_>>();
    for release in releases {
        let version: ProductVersion = release
            .product_version
            .parse()
            .map_err(|e| format!("invalid product version: {e}"))?;
        if !digest(&release.source_commit, 40)
            || !digest(&release.source_manifest_sha256, 64)
            || release.rustc.is_empty()
            || release.rustc.len() > 256
            || release.rustc.chars().any(char::is_control)
        {
            return Err("invalid source/build provenance".into());
        }
        if release
            .supported_formats
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
            != names
        {
            return Err("release must declare exactly eight format domains".into());
        }
        if release
            .default_formats
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
            != names
        {
            return Err("release must declare an explicit default for each domain".into());
        }
        for supported in release.supported_formats.values() {
            if supported.is_empty() {
                return Err("a required format domain has no supported contract".into());
            }
            let mut seen = BTreeSet::new();
            for value in supported {
                let format: FormatVersion = value
                    .parse()
                    .map_err(|e| format!("invalid format version: {e}"))?;
                if !seen.insert(format) {
                    return Err("duplicate support entry".into());
                }
            }
        }
        for (domain, value) in &release.default_formats {
            let default: FormatVersion = value
                .parse()
                .map_err(|e| format!("invalid default format: {e}"))?;
            if !release.supported_formats[domain].contains(value)
                || default.major() != version.major()
                || (version.requires_formal_defaults() && !default.is_formal())
            {
                return Err("defaults must be explicitly supported; RC/formal products require eight formal defaults in their baseline Major".into());
            }
        }
        let mut matrix = BTreeSet::new();
        for artifact in &release.artifacts {
            if !matches!(
                artifact.platform.as_str(),
                "windows-x86_64" | "linux-x86_64"
            ) || !matches!(artifact.flavor.as_str(), "normal" | "test")
                || !matrix.insert((artifact.platform.as_str(), artifact.flavor.as_str()))
                || !valid_url(&artifact.url)
                || !digest(&artifact.sha256, 64)
                || artifact.bytes == 0
            {
                return Err("invalid or duplicate native artifact".into());
            }
        }
        if matrix.len() != 4 {
            return Err("both platforms and both installation flavors are required".into());
        }
        if result.insert(version, release).is_some() {
            return Err("an exact product version is immutable and must appear once".into());
        }
    }
    Ok(result)
}
fn ruby(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}
fn definitions(release: &Release) -> BTreeMap<String, String> {
    let mut normalized = release.clone();
    normalized
        .artifacts
        .sort_by(|a, b| (&a.platform, &a.flavor).cmp(&(&b.platform, &b.flavor)));
    for versions in normalized.supported_formats.values_mut() {
        versions.sort_by_key(|value| value.parse::<FormatVersion>().expect("validated format"));
    }
    let release = &normalized;
    let mut files = BTreeMap::from([
        (
            "pactrun-package-source.txt".into(),
            "pactrun-native-package-source\n".into(),
        ),
        (
            "release.json".into(),
            serde_json::to_string_pretty(release).unwrap() + "\n",
        ),
    ]);
    files.insert(
        "pactrun-source-major.txt".into(),
        format!(
            "{}\n",
            release
                .product_version
                .parse::<ProductVersion>()
                .unwrap()
                .major()
        ),
    );
    for artifact in &release.artifacts {
        let (app, class) = if artifact.flavor == "normal" {
            ("pactrun", "Pactrun")
        } else {
            ("pactrun-test", "PactrunTest")
        };
        if artifact.platform == "windows-x86_64" {
            let manifest = json!({"version":release.product_version,"description":"Pactrun immutable revisions and managed instances",
                "architecture":{"64bit":{"url":artifact.url,"hash":artifact.sha256}},
                "bin":[[format!("bin/{app}.exe"),app.to_owned()],[format!("bin/{app}-source.exe"),format!("{app}-source")]]});
            files.insert(
                format!("bucket/{app}.json"),
                serde_json::to_string_pretty(&manifest).unwrap() + "\n",
            );
        } else {
            // Official Homebrew Formula DSL; only immutable program files are installed.
            files.insert(format!("Formula/{app}.rb"),format!(
                "class {class} < Formula\n  desc 'Pactrun immutable revisions and managed instances'\n  url {}\n  version {}\n  sha256 {}\n  depends_on :linux\n  depends_on arch: :x86_64\n  def install\n    prefix.install 'bin', 'libexec'\n  end\n  test do\n    assert_match version.to_s, shell_output(\"#{{bin}}/{app} --version\")\n  end\nend\n",ruby(&artifact.url),ruby(&release.product_version),ruby(&artifact.sha256)));
        }
    }
    files
}
fn plans(releases: &[Release]) -> Result<BTreeMap<String, BTreeMap<String, String>>, String> {
    let versions = validate(releases)?;
    let mut result = BTreeMap::new();
    for (version, release) in &versions {
        result.insert(format!("version-{version}"), definitions(release));
    }
    for major in versions.keys().map(|v| v.major()).collect::<BTreeSet<_>>() {
        for (name, allow_prerelease) in [("stable", false), ("preview", true)] {
            let choice = ReleaseSelection::Major {
                major,
                allow_prerelease,
            }
            .select(versions.keys().copied());
            let files = choice
                .map(|v| definitions(versions[&v]))
                .unwrap_or_else(|| {
                    BTreeMap::from([
                        (
                            "pactrun-package-source.txt".into(),
                            "pactrun-native-package-source\n".into(),
                        ),
                        (
                            "NO-ELIGIBLE-RELEASE.txt".into(),
                            "No eligible release; there is no implicit prerelease fallback.\n"
                                .into(),
                        ),
                    ])
                });
            let mut files = files;
            files.insert("pactrun-source-major.txt".into(), format!("{major}\n"));
            result.insert(format!("major-{major}-{name}"), files);
        }
    }
    Ok(result)
}
fn load(input: &Path) -> Result<Vec<Release>, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(input)
        .map_err(|e| e.to_string())?
        .take(MAX_INPUT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_INPUT {
        return Err("release metadata exceeds the bounded input profile".into());
    }
    strict_json::parse_json(&bytes, MAX_INPUT as usize).map_err(|e| e.to_string())?;
    let releases: Vec<Release> = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(releases)
}

fn package_base(major: u64, test: bool) -> String {
    let flavor = if test { "pactrun-test" } else { "pactrun" };
    if major == 1 {
        flavor.to_owned()
    } else {
        format!("{flavor}-v{major}")
    }
}

fn catalog_definition(release: &Release, package: &str, test: bool) -> BTreeMap<String, String> {
    let app = if test { "pactrun-test" } else { "pactrun" };
    let flavor = if test { "test" } else { "normal" };
    let mut files = BTreeMap::new();
    let class = package
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().unwrap().to_uppercase().collect::<String>() + chars.as_str()
        })
        .collect::<String>();
    for artifact in release.artifacts.iter().filter(|a| a.flavor == flavor) {
        if artifact.platform == "windows-x86_64" {
            // Detect any already installed package owning this entrypoint, including
            // exact versions not known when this immutable definition was published.
            let guard = format!(
                "$root = if ($global) {{ $globaldir }} else {{ $scoopdir }}; Get-ChildItem (Join-Path $root 'apps') -Directory | Where-Object {{ $_.Name -ne $app }} | ForEach-Object {{ $manifest = Join-Path $_.FullName 'current/manifest.json'; $installed = Join-Path $_.FullName 'current/install.json'; if ((Test-Path -LiteralPath $manifest) -and (Test-Path -LiteralPath $installed)) {{ $bins = (Get-Content -Raw -LiteralPath $manifest | ConvertFrom-Json).bin; foreach ($entry in $bins) {{ if ($entry -is [array] -and $entry.Count -ge 2 -and $entry[1] -eq '{app}') {{ throw \"Uninstall $($_.Name) before installing $app; managed data is retained.\" }} }} }} }}"
            );
            let manifest = json!({"version": release.product_version,
                "description": "Pactrun immutable revisions and managed instances",
                "homepage": "https://github.com/Doner357/pactrun", "license": "MIT",
                "architecture": {"64bit": {"url": artifact.url, "hash": artifact.sha256}},
                "bin": [[format!("bin/{app}.exe"), app]], "pre_install": guard});
            files.insert(
                format!("bucket/{package}.json"),
                serde_json::to_string_pretty(&manifest).unwrap() + "\n",
            );
        } else {
            files.insert(format!("Formula/{package}.rb"), format!(
                "class {class} < Formula\n  desc 'Pactrun immutable revisions and managed instances'\n  homepage 'https://github.com/Doner357/pactrun'\n  license 'MIT'\n  url {}\n  version {}\n  sha256 {}\n  depends_on :linux\n  depends_on arch: :x86_64\n  def install\n    Formula.installed.each do |other|\n      if other.name != name && (other.opt_bin/\"{app}\").exist?\n        raise \"Uninstall #{{other.full_name}} before installing #{{full_name}}; managed data is retained.\"\n      end\n    end\n    prefix.install 'bin', 'libexec', 'LICENSE', 'THIRD_PARTY_NOTICES.txt', 'rust-licenses'\n  end\n  test do\n    assert_match version.to_s, shell_output(\"#{{bin}}/{app} --version\")\n  end\nend\n",
                ruby(&artifact.url), ruby(&release.product_version), ruby(&artifact.sha256)));
        }
    }
    files
}

fn catalog(releases: &[Release], previous: &[Release]) -> Result<BTreeMap<String, String>, String> {
    let versions = validate(releases)?;
    for (version, old) in validate(previous)? {
        let new = versions
            .get(&version)
            .ok_or("published release cannot be removed")?;
        if definitions(old) != definitions(new) {
            return Err("published release identity cannot change; use a new version".into());
        }
    }
    let mut files = BTreeMap::new();
    for (version, release) in &versions {
        files.insert(
            format!("releases/{version}.json"),
            serde_json::to_string_pretty(release).unwrap() + "\n",
        );
        for test in [false, true] {
            let base = package_base(version.major(), test);
            let exact = format!("{base}-exact-{}", version.to_string().replace('.', "-"));
            files.extend(catalog_definition(release, &exact, test));
        }
    }
    for major in versions.keys().map(|v| v.major()).collect::<BTreeSet<_>>() {
        for (suffix, allow_prerelease) in [("", false), ("-preview", true)] {
            if let Some(version) = (ReleaseSelection::Major {
                major,
                allow_prerelease,
            })
            .select(versions.keys().copied())
            {
                for test in [false, true] {
                    let name = format!("{}{suffix}", package_base(major, test));
                    files.extend(catalog_definition(versions[&version], &name, test));
                }
            }
        }
    }
    let ordered = versions.into_values().collect::<Vec<_>>();
    files.insert(
        "releases/catalog.json".into(),
        serde_json::to_string_pretty(&ordered).unwrap() + "\n",
    );
    Ok(files)
}

/// Produce ordinary project files, never branch/channel refs or installation state.
pub(crate) fn generate_catalog(
    input: &Path,
    output: &Path,
    previous: Option<&Path>,
) -> Result<(), String> {
    let previous = previous.map(load).transpose()?.unwrap_or_default();
    let files = catalog(&load(input)?, &previous)?;
    fs::create_dir(output).map_err(|e| e.to_string())?;
    for (name, contents) in files {
        let path = output.join(name);
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(contents.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn generate(input: &Path, output: &Path) -> Result<(), String> {
    let releases = load(input)?;
    let plans = plans(&releases)?;
    // Validate the complete plan before creating any output. Existing roots,
    // exact definitions and native-manager state are never overwritten here.
    fs::create_dir(output).map_err(|e| e.to_string())?;
    for (branch, files) in plans {
        for (name, contents) in files {
            let path = output.join(&branch).join(name);
            fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
            use std::io::Write;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|e| e.to_string())?;
            file.write_all(contents.as_bytes())
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn local_git(repo: &Path, args: &[&str], input: Option<&[u8]>) -> Result<String, String> {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input)
            .map_err(|e| e.to_string())?;
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(
            "local source Git operation failed; no remote publication was attempted".into(),
        );
    }
    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|e| e.to_string())
}
fn git_tree(repo: &Path, files: &BTreeMap<String, String>) -> Result<String, String> {
    let mut root = String::new();
    let mut directories = BTreeMap::<&str, String>::new();
    for (name, bytes) in files {
        let blob = local_git(
            repo,
            &["hash-object", "-w", "--stdin"],
            Some(bytes.as_bytes()),
        )?;
        if let Some((directory, file)) = name.split_once('/') {
            directories
                .entry(directory)
                .or_default()
                .push_str(&format!("100644 blob {blob}\t{file}\n"));
        } else {
            root.push_str(&format!("100644 blob {blob}\t{name}\n"));
        }
    }
    for (directory, entries) in directories {
        let tree = local_git(repo, &["mktree"], Some(entries.as_bytes()))?;
        root.push_str(&format!("040000 tree {tree}\t{directory}\n"));
    }
    local_git(repo, &["mktree"], Some(root.as_bytes()))
}
/// Updates only an explicitly owned local bare source repository. Exact release
/// refs never move; channel refs advance through a single Git ref transaction.
/// Neither pushing nor activation/removal of installed programs exists here.
pub(crate) fn publish_local(input: &Path, repo: &Path, default_ref: &str) -> Result<(), String> {
    let incoming = load(input)?;
    validate(&incoming)?;
    let existed = repo.exists();
    let mut releases = BTreeMap::<ProductVersion, Release>::new();
    let mut references = BTreeMap::<String, String>::new();
    if existed {
        if local_git(repo, &["rev-parse", "--is-bare-repository"], None)? != "true"
            || local_git(
                repo,
                &["config", "--get", "pactrun.nativeSourcePublisher"],
                None,
            )? != "1"
        {
            return Err("refusing an unowned or non-bare repository".into());
        }
        for line in local_git(
            repo,
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                "refs/heads/",
            ],
            None,
        )?
        .lines()
        {
            let (name, sha) = line.split_once(' ').ok_or("invalid local ref")?;
            references.insert(name.to_owned(), sha.to_owned());
            if let Some(version) = name.strip_prefix("refs/heads/version-") {
                let version: ProductVersion = version
                    .parse()
                    .map_err(|_| "invalid immutable source ref")?;
                let blob = format!("{sha}:release.json");
                let size = local_git(repo, &["cat-file", "-s", &blob], None)?
                    .parse::<u64>()
                    .map_err(|_| "invalid release metadata size")?;
                if size > MAX_INPUT {
                    return Err("stored release metadata exceeds its bound".into());
                }
                let raw = local_git(repo, &["show", &blob], None)?;
                strict_json::parse_json(raw.as_bytes(), MAX_INPUT as usize)
                    .map_err(|e| e.to_string())?;
                let release: Release = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
                validate(std::slice::from_ref(&release))?;
                if release.product_version != version.to_string() {
                    return Err("immutable ref/version mismatch".into());
                }
                releases.insert(version, release);
            }
        }
    }
    for release in incoming {
        let version = release.product_version.parse::<ProductVersion>().unwrap();
        if let Some(existing) = releases.get(&version)
            && definitions(existing) != definitions(&release)
        {
            return Err("an exact release source is immutable; use a distinct version".into());
        }
        releases.insert(version, release);
    }
    let records = releases.into_values().collect::<Vec<_>>();
    let planned = plans(&records)?;
    if !planned.contains_key(default_ref) {
        return Err("explicit default source ref is not present in the validated plan".into());
    }
    if !existed {
        fs::create_dir(repo).map_err(|e| e.to_string())?;
        local_git(repo, &["init", "--bare"], None)?;
        local_git(
            repo,
            &["config", "pactrun.nativeSourcePublisher", "1"],
            None,
        )?;
    }
    let mut transaction = String::from("start\n");
    for (branch, files) in planned {
        let name = format!("refs/heads/{branch}");
        let tree = git_tree(repo, &files)?;
        let old = references.get(&name);
        if let Some(old) = old {
            let old_tree = local_git(repo, &["rev-parse", &format!("{old}^{{tree}}")], None)?;
            if tree == old_tree {
                continue;
            }
            if branch.starts_with("version-") {
                return Err("immutable source tree changed".into());
            }
        }
        let mut args = vec![
            "-c",
            "user.name=Pactrun local source publisher",
            "-c",
            "user.email=source@example.invalid",
            "commit-tree",
            &tree,
        ];
        if let Some(parent) = old {
            args.extend(["-p", parent]);
        }
        let message = format!("Native source {branch}\n");
        let commit = local_git(repo, &args, Some(message.as_bytes()))?;
        match old {
            Some(old) => transaction.push_str(&format!("update {name} {commit} {old}\n")),
            None => transaction.push_str(&format!("create {name} {commit}\n")),
        }
    }
    transaction.push_str("prepare\ncommit\n");
    local_git(
        repo,
        &["update-ref", "--stdin"],
        Some(transaction.as_bytes()),
    )?;
    if !existed {
        local_git(
            repo,
            &["symbolic-ref", "HEAD", &format!("refs/heads/{default_ref}")],
            None,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-ID: PR-TEST-0640
    // Verifies: PR-REQ-0333
    #[test]
    fn ordinary_catalog_separates_channels_major_and_immutable_exact_entries() {
        let first = release("1.0.0-alpha.1");
        let second = release("1.0.0-alpha.2");
        let old = catalog(std::slice::from_ref(&first), &[]).unwrap();
        let new = catalog(&[first.clone(), second], std::slice::from_ref(&first)).unwrap();
        assert!(!new.contains_key("bucket/pactrun.json"));
        assert!(new["bucket/pactrun-preview.json"].contains("1.0.0-alpha.2"));
        assert_eq!(
            old["bucket/pactrun-exact-1-0-0-alpha-1.json"],
            new["bucket/pactrun-exact-1-0-0-alpha-1.json"]
        );
        assert_eq!(
            old["Formula/pactrun-exact-1-0-0-alpha-1.rb"],
            new["Formula/pactrun-exact-1-0-0-alpha-1.rb"]
        );
        assert!(new["Formula/pactrun-preview.rb"].contains("class PactrunPreview < Formula"));
        assert!(new.values().all(|value| !value.contains("pactrun-source")));
        assert!(catalog(&[], std::slice::from_ref(&first)).is_err());
        let mut rewritten = first.clone();
        rewritten.artifacts[0].sha256 = "d".repeat(64);
        assert!(catalog(&[rewritten], &[first]).is_err());
        assert_eq!(package_base(1, false), "pactrun");
        assert_eq!(package_base(2, false), "pactrun-v2");
        assert_eq!(package_base(2, true), "pactrun-test-v2");
    }

    // Test-ID: PR-TEST-0641
    // Verifies: PR-REQ-0333
    #[test]
    fn native_catalog_uses_formal_precedence_and_guards_command_ownership() {
        let files = catalog(
            &[
                release("1.0.0-alpha.2"),
                release("1.0.0"),
                release("1.1.0-alpha.1"),
            ],
            &[],
        )
        .unwrap();
        let stable: serde_json::Value =
            serde_json::from_str(&files["bucket/pactrun.json"]).unwrap();
        let preview: serde_json::Value =
            serde_json::from_str(&files["bucket/pactrun-preview.json"]).unwrap();
        assert_eq!(stable["version"], "1.0.0");
        assert_eq!(preview["version"], "1.1.0-alpha.1");
        assert_eq!(preview["bin"][0][1], "pactrun");
        assert!(
            preview["pre_install"]
                .as_str()
                .unwrap()
                .contains("Uninstall")
        );
        assert!(files["Formula/pactrun-preview.rb"].contains("Formula.installed.each"));
        assert!(!preview.to_string().contains("persist"));
    }
    fn release(version: &str) -> Release {
        let formal = version.parse::<ProductVersion>().unwrap().is_formal();
        Release {
            product_version: version.into(),
            source_commit: "a".repeat(40),
            source_manifest_sha256: "b".repeat(64),
            rustc: "rustc fixture".into(),
            supported_formats: VersionDomain::ALL
                .into_iter()
                .map(|d| {
                    (
                        d.name().into(),
                        vec![if formal { "1.0" } else { "1.0-alpha.1" }.into()],
                    )
                })
                .collect(),
            default_formats: VersionDomain::ALL
                .into_iter()
                .map(|d| {
                    (
                        d.name().into(),
                        if formal { "1.0" } else { "1.0-alpha.1" }.into(),
                    )
                })
                .collect(),
            artifacts: ["windows-x86_64", "linux-x86_64"]
                .into_iter()
                .flat_map(|platform| {
                    ["normal", "test"].map(|flavor| Artifact {
                        platform: platform.into(),
                        flavor: flavor.into(),
                        url: format!("https://example.invalid/{version}/{platform}-{flavor}.zip"),
                        sha256: "c".repeat(64),
                        bytes: 42,
                    })
                })
                .collect(),
        }
    }
    // Test-ID: PR-TEST-0629
    // Verifies: PR-REQ-0333
    #[test]
    fn native_sources_share_precedence_exact_selection_and_no_prerelease_fallback() {
        let releases = [release("1.0.0-alpha.9"), release("1.0.0-alpha.10")];
        let plan = plans(&releases).unwrap();
        assert!(plan["major-1-stable"].contains_key("NO-ELIGIBLE-RELEASE.txt"));
        let manifest: serde_json::Value =
            serde_json::from_str(&plan["major-1-preview"]["bucket/pactrun.json"]).unwrap();
        assert_eq!(manifest["version"], "1.0.0-alpha.10");
        assert!(plan["major-1-preview"]["Formula/pactrun.rb"].contains("version '1.0.0-alpha.10'"));
        assert!(
            plan["version-1.0.0-alpha.9"]["Formula/pactrun-test.rb"]
                .contains("version '1.0.0-alpha.9'")
        );
        for files in plan.values() {
            for value in files.values() {
                assert!(!value.contains("post_install"));
                assert!(!value.contains("uninstaller"));
            }
        }
    }
    // Test-ID: PR-TEST-0630
    // Verifies: PR-REQ-0333
    #[test]
    fn formal_publication_requires_formal_defaults_and_a_complete_immutable_artifact_matrix() {
        let mut valid = release("1.0.0");
        assert!(plans(&[valid.clone()]).is_ok());
        valid
            .default_formats
            .insert("persistence".into(), "1.0-alpha.1".into());
        assert!(plans(&[valid]).is_err());
        let rc = release("1.0.0-rc.1");
        assert!(plans(&[rc]).is_err());
        let mut explicit_older_reader = release("1.0.0");
        explicit_older_reader.product_version = "2.0.0".into();
        for values in explicit_older_reader.supported_formats.values_mut() {
            values.push("2.0".into());
        }
        for value in explicit_older_reader.default_formats.values_mut() {
            *value = "2.0".into();
        }
        assert!(
            plans(&[explicit_older_reader]).is_ok(),
            "explicit earlier-Major readers are not prohibited or inferred by ordering"
        );
        let valid = release("1.0.0-alpha.1");
        assert!(plans(&[valid.clone(), valid.clone()]).is_err());
        let mut missing = valid.clone();
        missing.artifacts.pop();
        assert!(plans(&[missing]).is_err());
        let mut corrupt = valid;
        corrupt.artifacts[0].sha256 = "not-a-digest".into();
        assert!(plans(&[corrupt]).is_err());
        assert!(!valid_url("http://untrusted.example/artifact"));
        assert!(!valid_url("https://user:password@example.invalid/artifact"));
        assert_eq!(ruby("x'#{not_evaluated}"), "'x\\'#{not_evaluated}'");
    }
    // Test-ID: PR-TEST-0635
    // Verifies: PR-REQ-0333
    #[test]
    fn artifact_assembly_is_reproducible_and_refuses_dirty_or_escaping_inputs() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap();
        let python = if cfg!(windows) { "python" } else { "python3" };
        let output = std::process::Command::new(python)
            .args(["-B", "-m", "unittest", "tools.test_release_artifacts"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "artifact tooling tests failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    // Test-ID: PR-TEST-0636
    // Verifies: PR-REQ-0333
    #[test]
    fn local_publication_preserves_exact_refs_and_never_rolls_channels_back_when_inputs_omit_history()
     {
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("target/local-source-publisher-tests");
        fs::create_dir_all(&parent).unwrap();
        let name = format!(
            "run-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = parent.join(name);
        fs::create_dir(&root).unwrap();
        let input = root.join("records.json");
        let repo = root.join("source.git");
        let first = release("1.0.0-alpha.1");
        fs::write(&input, serde_json::to_vec(&vec![first.clone()]).unwrap()).unwrap();
        publish_local(&input, &repo, "major-1-preview").unwrap();
        let exact = local_git(
            &repo,
            &["rev-parse", "refs/heads/version-1.0.0-alpha.1"],
            None,
        )
        .unwrap();
        fs::write(
            &input,
            serde_json::to_vec(&vec![release("1.0.0-alpha.2")]).unwrap(),
        )
        .unwrap();
        publish_local(&input, &repo, "major-1-preview").unwrap();
        let current = local_git(&repo, &["rev-parse", "refs/heads/major-1-preview"], None).unwrap();
        assert_eq!(
            local_git(
                &repo,
                &["rev-parse", "refs/heads/version-1.0.0-alpha.1"],
                None
            )
            .unwrap(),
            exact
        );
        fs::write(&input, serde_json::to_vec(&vec![first.clone()]).unwrap()).unwrap();
        publish_local(&input, &repo, "major-1-preview").unwrap();
        assert_eq!(
            local_git(&repo, &["rev-parse", "refs/heads/major-1-preview"], None).unwrap(),
            current
        );
        let mut changed = first;
        changed.artifacts[0].sha256 = "d".repeat(64);
        fs::write(&input, serde_json::to_vec(&vec![changed]).unwrap()).unwrap();
        assert!(publish_local(&input, &repo, "major-1-preview").is_err());
        assert_eq!(
            local_git(&repo, &["rev-parse", "refs/heads/major-1-preview"], None).unwrap(),
            current
        );
        assert_eq!(
            local_git(&repo, &["symbolic-ref", "HEAD"], None).unwrap(),
            "refs/heads/major-1-preview"
        );
    }
}
