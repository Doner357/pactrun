"""Build local, source-qualified Pactrun archives; never publish or install them.

Developer tooling uses Python's standard library. Installed packages do not.
`source` captures only committed build/document/test inputs. `build` verifies that
snapshot before and after Cargo, embeds its provenance, probes the resulting
program, and creates reproducible normal/test/standalone archives.
"""
from __future__ import annotations
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import tarfile
import zipfile

PREFIXES = ("src/", "crates/", "xtask/", "tests/", "docs/", "website/", "tools/", ".cargo/", ".github/", "bucket/", "Formula/", "releases/")
ROOT_FILES = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "build.rs", "README.md", "CONTRIBUTING.md", ".gitattributes", "LICENSE"}
TARGETS = {"windows-x86_64": "x86_64-pc-windows-msvc", "linux-x86_64": "x86_64-unknown-linux-gnu"}
FORBIDDEN = {".git", ".env", ".ssh", ".remote-sync", "node_modules", "__pycache__", "target", "dist", ".pactrun"}

def selected(name: str) -> bool:
    return (name in ROOT_FILES or name.startswith(PREFIXES)) and not FORBIDDEN.intersection(PurePosixPath(name).parts)

def safe_name(name: str) -> bool:
    p = PurePosixPath(name)
    return bool(name) and not p.is_absolute() and ".." not in p.parts and not FORBIDDEN.intersection(p.parts) and "\\" not in name

def sha(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()

def execute(args, cwd=None, env=None) -> bytes:
    result = subprocess.run([str(a) for a in args], cwd=cwd, env=env,
                            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    if result.returncode:
        # Build inputs are trusted. Keep credentials and machine-specific origins
        # out of publication records; detailed local Cargo logs remain local.
        raise RuntimeError(f"{Path(str(args[0])).name} failed with exit {result.returncode}")
    return result.stdout

def source(root: Path, output: Path):
    root = root.resolve()
    changes = execute(["git", "status", "--porcelain", "--untracked-files=all"], root).decode("utf-8")
    for line in changes.splitlines():
        name = line[3:].replace('"', '')
        if selected(name) or " -> " in name and any(selected(p) for p in name.split(" -> ")):
            raise ValueError("commit all selected release inputs before making a source snapshot")
    commit = execute(["git", "rev-parse", "HEAD"], root).decode("ascii").strip()
    # Archive uses Git conversion attributes; do not inherit host checkout line endings.
    raw = execute(["git", "-c", "core.autocrlf=false", "-c", "core.eol=lf",
                   "archive", "--format=tar", "HEAD"], root)
    files = {}
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        for member in archive:
            if not selected(member.name) or member.isdir():
                continue
            if not safe_name(member.name) or not member.isfile():
                raise ValueError("release input must be a safe regular source file")
            files[member.name] = (archive.extractfile(member).read(), bool(member.mode & 0o111))
    required = {"Cargo.toml", "Cargo.lock", "build.rs", "LICENSE", "src/main.rs", "src/bin/pactrun-launcher.rs"}
    if not required.issubset(files):
        raise ValueError("source snapshot lacks required committed build inputs")
    manifest = "".join(hashlib.sha256(data).hexdigest() + "  " + name + "\n" for name, (data, _) in sorted(files.items())).encode()
    provenance = dict(source_commit=commit, source_manifest_sha256=hashlib.sha256(manifest).hexdigest())
    output.mkdir(parents=False, exist_ok=False)
    with (output / "source.tar.gz").open("wb") as raw_output, gzip.GzipFile(fileobj=raw_output, mode="wb", filename="", mtime=0) as compressed, tarfile.open(fileobj=compressed, mode="w|") as archive:
        for name, (data, executable) in sorted(files.items()):
            member = tarfile.TarInfo(name)
            member.size, member.mode, member.mtime = len(data), 0o755 if executable else 0o644, 0
            archive.addfile(member, io.BytesIO(data))
    (output / "source.sha256").write_bytes(manifest)
    (output / "source.json").write_text(json.dumps(provenance, indent=2) + "\n", encoding="utf-8")

def verify(root: Path, manifest: Path, expected: str):
    if sha(manifest) != expected:
        raise ValueError("source manifest identity mismatch")
    if manifest.stat().st_size > 16 * 1024 * 1024:
        raise ValueError("source manifest exceeds its bound")
    seen = set()
    for line in manifest.read_text(encoding="utf-8").splitlines():
        digest, name = line.split("  ", 1)
        file = root / name
        if (not safe_name(name) or not selected(name) or name in seen or file.is_symlink()
            or not file.resolve().is_relative_to(root.resolve()) or sha(file) != digest):
            raise ValueError("source snapshot content mismatch")
        seen.add(name)

def archive_files(destination: Path, files: dict[str, Path], windows: bool):
    if windows:
        with zipfile.ZipFile(destination, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for name, path in sorted(files.items()):
                entry = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
                entry.compress_type = zipfile.ZIP_DEFLATED
                entry.external_attr = (0o100755 << 16)
                with path.open("rb") as source, archive.open(entry, "w", force_zip64=True) as target:
                    shutil.copyfileobj(source, target, 1024 * 1024)
    else:
        with destination.open("xb") as raw, gzip.GzipFile(fileobj=raw, mode="wb", filename="", mtime=0) as zipped, tarfile.open(fileobj=zipped, mode="w|") as archive:
            for name, path in sorted(files.items()):
                member = tarfile.TarInfo(name)
                member.mode, member.size, member.mtime = 0o755, path.stat().st_size, 0
                with path.open("rb") as content:
                    archive.addfile(member, content)

def build(root: Path, snapshot: Path, output: Path, platform: str, url_base: str):
    root, snapshot, output = root.resolve(), snapshot.resolve(), output.resolve()
    info_path = snapshot / "source.json"
    if info_path.stat().st_size > 4096:
        raise ValueError("source provenance exceeds its bound")
    provenance = json.loads(info_path.read_text(encoding="utf-8"))
    if set(provenance) != {"source_commit", "source_manifest_sha256"}:
        raise ValueError("invalid closed source provenance")
    for key, length in [("source_commit", 40), ("source_manifest_sha256", 64)]:
        value = provenance[key]
        if not isinstance(value, str) or len(value) != length or any(c not in "0123456789abcdef" for c in value):
            raise ValueError("invalid source provenance identity")
    verify(root, snapshot / "source.sha256", provenance["source_manifest_sha256"])
    output.mkdir(parents=False, exist_ok=False)
    env = dict(os.environ)
    env.update(CARGO_TARGET_DIR=str(output / "cargo-target"),
               PACTRUN_BUILD_SOURCE_COMMIT=provenance["source_commit"],
               PACTRUN_BUILD_SOURCE_MANIFEST=provenance["source_manifest_sha256"],
               CARGO_PROFILE_RELEASE_STRIP="symbols")
    windows = platform == "windows-x86_64"
    if windows:
        env.pop("CARGO_ENCODED_RUSTFLAGS", None)
        env["RUSTFLAGS"] = "-C target-feature=+crt-static"
    target = TARGETS[platform]
    # No cross-environment execution claim: run this on the actual target host.
    with (output / "cargo-build.log").open("wb") as log:
        result = subprocess.run(["cargo", "build", "--offline", "--locked", "--release", "--target", target,
                                 "--bin", "pactrun", "--bin", "pactrun-launcher"],
                                cwd=root, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)
    if result.returncode:
        raise RuntimeError("release build failed; inspect the local cargo-build.log")
    verify(root, snapshot / "source.sha256", provenance["source_manifest_sha256"])
    directory = output / "cargo-target" / target / "release"
    suffix = ".exe" if windows else ""
    product, launcher = [directory / (name + suffix) for name in ("pactrun", "pactrun-launcher")]
    metadata_graph = json.loads(execute(["cargo", "metadata", "--offline", "--locked", "--format-version", "1", "--filter-platform", target], root))
    notices = output / "THIRD_PARTY_NOTICES.txt"
    notices.write_text(dependency_notices(metadata_graph), encoding="utf-8")
    legal = {"LICENSE": root / "LICENSE", "THIRD_PARTY_NOTICES.txt": notices}
    sysroot = Path(execute(["rustc", "--print", "sysroot"], root).decode().strip())
    rust_legal = sysroot / "share/doc/rust"
    copyright_file = rust_legal / "COPYRIGHT-library.html"
    if not copyright_file.is_file():
        raise ValueError("Rust standard-library copyright material is required for distribution")
    legal["rust-licenses/COPYRIGHT-library.html"] = copyright_file
    for file in sorted((rust_legal / "licenses").rglob("*")):
        if file.is_file() and not file.is_symlink():
            legal["rust-licenses/" + file.relative_to(rust_legal).as_posix()] = file
    response = json.loads(execute([product, "--format", "json", "--version"], root, env))
    info = response["result"]
    if (response["status"] != "success" or info["build_target"] != target
        or any(info.get(key) != value for key, value in provenance.items())):
        raise ValueError("built executable does not match supplied source/build provenance")
    version = info["product_version"]
    # Cargo owns product syntax; the shared Rust publication validator owns policy.
    if not version or any(c not in "0123456789abcdefghijklmnopqrstuvwxyz.-" for c in version):
        raise ValueError("unsafe product artifact name")
    metadata = dict(product_version=version, **provenance, rustc=info["rustc"],
                    supported_formats=info["supported_formats"], default_formats=info["default_formats"], artifacts=[])
    assets = output / "assets"
    assets.mkdir()
    extension = ".zip" if windows else ".tar.gz"
    for flavor in ("normal", "test"):
        app = "pactrun" if flavor == "normal" else "pactrun-test"
        path = assets / f"{app}-{version}-{platform}{extension}"
        archive_files(path, {f"bin/{app}{suffix}": launcher,
                             f"libexec/pactrun{suffix}": product, **legal}, windows)
        metadata["artifacts"].append(dict(platform=platform, flavor=flavor, url=url_base.rstrip('/') + '/' + path.name,
                                          sha256=sha(path), bytes=path.stat().st_size))
    standalone = assets / f"pactrun-{version}-{platform}-standalone{extension}"
    archive_files(standalone, {"pactrun" + suffix: product, **legal}, windows)
    (output / "release-part.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    (output / "binary-probe.json").write_text(json.dumps(response, indent=2) + "\n", encoding="utf-8")
    (output / "artifact-checksums.json").write_text(json.dumps({p.name: dict(sha256=sha(p), bytes=p.stat().st_size) for p in sorted(assets.iterdir())}, indent=2)+"\n", encoding="utf-8")

def dependency_notices(metadata):
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    pending = [metadata["resolve"]["root"]]
    seen = set()
    while pending:
        node = pending.pop()
        if node in seen:
            continue
        seen.add(node)
        pending.extend(d["pkg"] for d in nodes[node]["deps"] if any(k["kind"] != "dev" for k in d["dep_kinds"]))
    text = ["Pactrun third-party notices\n\nDependency licenses remain their respective owners' licenses.\n"]
    for key in sorted(seen, key=lambda k: (packages[k]["name"], packages[k]["version"])):
        package = packages[key]
        if package["source"] is None:
            continue
        base = Path(package["manifest_path"]).parent
        candidates = set()
        for pattern in ["*LICENSE*", "*LICENCE*", "COPYING*", "NOTICE*", "UNLICENSE*", "license*", "licenses/**/*"]:
            candidates.update(p for p in base.glob(pattern) if p.is_file())
        if package.get("license_file"):
            candidates.add(base / package["license_file"])
        if not candidates or not package.get("license"):
            raise ValueError("review missing dependency license: " + package["name"])
        text.append(f"\n=== {package['name']} {package['version']} ===\nLicense: {package['license']}\n")
        for file in sorted(candidates):
            if not file.resolve().is_relative_to(base.resolve()) or file.is_symlink() or file.stat().st_size > 2 * 1024 * 1024:
                raise ValueError("unsafe dependency license file")
            text.append(f"\n--- {file.relative_to(base).as_posix()} ---\n" + file.read_text(encoding="utf-8", errors="strict") + "\n")
    return "".join(text)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    capture = commands.add_parser("source")
    capture.add_argument("--root", type=Path, required=True)
    capture.add_argument("--output", type=Path, required=True)
    package = commands.add_parser("build")
    for name in ("root", "snapshot", "output"):
        package.add_argument("--" + name, type=Path, required=True)
    package.add_argument("--platform", choices=TARGETS, required=True)
    package.add_argument("--url-base", required=True)
    args = parser.parse_args()
    if args.operation == "source": source(args.root, args.output)
    else: build(args.root, args.snapshot, args.output, args.platform, args.url_base)

if __name__ == "__main__":
    main()
