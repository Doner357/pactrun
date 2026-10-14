"""Prepare verified native rehearsal inputs and probe exact standalone payloads.

The existing native_package_acceptance runner owns the actual upgrade scenarios.
This glue neither builds product images nor publishes a release.
"""
import argparse
import json
import os
from pathlib import Path, PureWindowsPath
import re
import shutil
import subprocess
import urllib.parse
import urllib.request
import zipfile

from tools import release_artifacts as release
from tools import native_package_acceptance as native
from tools.linux_compatibility import unpack

ROOT = Path(__file__).resolve().parents[1]
PLATFORMS = ('windows-x86_64', 'linux-x86_64')


def public_name(artifact, version):
    url = urllib.parse.urlsplit(artifact['url'])
    prefix = f'/Doner357/pactrun/releases/download/v{version}/'
    name = url.path.removeprefix(prefix)
    platform, flavor = artifact['platform'], artifact['flavor']
    if platform not in PLATFORMS or flavor not in ('normal','test'):
        raise ValueError('Unknown native asset role')
    stem = 'pactrun' if flavor == 'normal' else 'pactrun-test'
    extension = '.zip' if platform == 'windows-x86_64' else '.tar.gz'
    expected_name = f'{stem}-{version}-{platform}{extension}'
    if (url.scheme != 'https' or url.netloc != 'github.com' or not url.path.startswith(prefix)
            or url.query or url.fragment or name != expected_name
            or type(artifact['bytes']) is not int or not 0 < artifact['bytes'] <= 1024 ** 3
            or not re.fullmatch('[0-9a-f]{64}', artifact['sha256'])):
        raise ValueError('Expected an exact public Pactrun release asset')
    return name


def download(artifact, version, destination):
    public_name(artifact, version)
    remaining = artifact['bytes'] + 1
    with destination.open('xb') as output, urllib.request.urlopen(artifact['url'], timeout=120) as response:
        while remaining:
            block = response.read(min(remaining, 1024 * 1024))
            if not block:
                break
            output.write(block)
            remaining -= len(block)
    if destination.stat().st_size != artifact['bytes'] or release.sha(destination) != artifact['sha256']:
        raise ValueError('Public baseline bytes do not match the immutable record')


def candidate_records(artifacts, source, version):
    candidates = [release.verify_candidate(artifacts / ('candidate-' + platform), platform, source, version)
                  for platform in PLATFORMS]
    metadata = lambda record: {k:v for k,v in record.items() if k != 'artifacts'}
    if metadata(candidates[0]) != metadata(candidates[1]):
        raise ValueError('Platform candidates do not share exact source and formats')
    for candidate in candidates:
        for artifact in candidate['artifacts']:
            public_name(artifact, version)
    for platform in PLATFORMS:
        release.verify_source_archive(artifacts / ('candidate-' + platform) / 'snapshot',
                                      candidates[0]['source_manifest_sha256'])
    return candidates


def combined_record(artifacts, source, version):
    parts = candidate_records(artifacts, source, version)
    return dict(parts[0], artifacts=[artifact for part in parts for artifact in part['artifacts']])


def stage_catalog(artifacts, source, version, output, xtask, previous=None):
    candidate = combined_record(artifacts, source, version)
    previous = previous or ROOT/'releases/catalog.json'
    records = json.loads(previous.read_text(encoding='utf-8'))
    existing = [record for record in records if record['product_version'] == version]
    if existing and (len(existing) != 1 or existing[0] != candidate):
        raise ValueError('Existing release identity must not be replaced')
    if not existing:
        records.append(candidate)
    if output.exists() or output.is_symlink():
        raise FileExistsError(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    input_file = output.with_name(output.name+'-input.json')
    with input_file.open('x',encoding='utf-8') as stream:
        json.dump(records,stream,indent=2)
    subprocess.run([str(xtask.resolve()),'release-catalog',str(input_file.resolve()),
                    str(output.absolute()),str(previous.resolve())],cwd=ROOT,check=True)
    return candidate


def prepare(artifacts, baseline_record, source, version, output, downloader=download):
    candidates = candidate_records(artifacts, source, version)
    baseline = json.loads(baseline_record.read_text(encoding='utf-8'))
    previous = baseline['product_version']
    if not isinstance(previous, str) or not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:-(?:alpha|beta|rc)\.[1-9][0-9]*)?', previous):
        raise ValueError('Invalid baseline version')
    if previous == version:
        raise ValueError('Baseline and candidate must be different versions')
    expected = {(p,f) for p in PLATFORMS for f in ('normal','test')}
    records = baseline['artifacts']
    if len(records) != 4 or {(r['platform'],r['flavor']) for r in records} != expected:
        raise ValueError('Baseline must contain the four native artifacts')
    for artifact in records:
        public_name(artifact, previous)
    upgrade = baseline['default_formats']['persistence'] != candidates[0]['default_formats']['persistence']
    if upgrade:
        native.upgrade_fixture_source_format([baseline, candidates[0]], [previous, version])
    else:
        native.fixture_source_format([baseline, candidates[0]], [previous, version])
    if output.exists() or output.is_symlink():
        raise FileExistsError(output)
    output.mkdir(parents=True)
    parts = []
    for platform in PLATFORMS:
        part_root = output / 'baseline' / platform
        assets = part_root / 'assets'
        assets.mkdir(parents=True)
        selected = [r for r in records if r['platform'] == platform]
        for artifact in selected:
            downloader(artifact, previous, assets / public_name(artifact, previous))
        part = part_root / 'release-part.json'
        part.write_text(json.dumps(dict(baseline, artifacts=selected), indent=2) + '\n', encoding='utf-8')
        parts.append(str(part.resolve()))
    parts.extend(str((artifacts / ('candidate-' + p) / 'output/release-part.json').resolve()) for p in PLATFORMS)
    result = {'format':'pactrun.native-inputs.v1', 'versions':[previous,version],
              'source_commit':source, 'source_manifest_sha256':candidates[0]['source_manifest_sha256'],
              'store_upgrade':upgrade, 'parts':parts}
    (output / 'prepared.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    return result


def standalone(artifacts, platform, source, version, output):
    part = release.verify_candidate(artifacts, platform, source, version)
    windows = platform == 'windows-x86_64'
    if windows != (os.name == 'nt'):
        raise ValueError('Standalone probe requires the native platform')
    extension = '.zip' if windows else '.tar.gz'
    archive = artifacts / 'output/assets' / f'pactrun-{version}-{platform}-standalone{extension}'
    if windows:
        with zipfile.ZipFile(archive) as data:
            entries = data.infolist()
            names = [entry.filename for entry in entries]
            if (len(names) != len(set(names)) or sum(e.file_size for e in entries) > 256 * 1024 * 1024
                or any(not release.safe_name(n) or PureWindowsPath(n).drive for n in names)
                or any((e.external_attr >> 16) & 0o170000 not in (0,0o100000,0o040000) for e in entries)):
                raise ValueError('Unsafe standalone archive')
            output.mkdir(parents=True, exist_ok=False)
            for entry in entries:
                target = output / entry.filename
                if entry.is_dir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    target.parent.mkdir(parents=True, exist_ok=True)
                    with data.open(entry) as src, target.open('xb') as dst:
                        shutil.copyfileobj(src,dst)
    else:
        unpack(archive, output)
    program = output / ('pactrun.exe' if windows else 'pactrun')
    state = output / 'probe-state'
    env = dict(os.environ, **native.isolated_product_environment(state))
    env.pop('PACTRUN_STORAGE_ROOT', None)
    result = subprocess.run([str(program), '--format','json','--version'], env=env,
                            capture_output=True, check=True, timeout=30)
    _, info = native.cli_result(result.stdout, 'version')
    for key in ('product_version','source_commit','source_manifest_sha256','supported_formats','default_formats'):
        if info[key] != part[key]:
            raise ValueError('Executed standalone differs from candidate provenance')
    help_result = subprocess.run([str(program),'--help'], env=env,capture_output=True,check=True,timeout=30)
    if not help_result.stdout or state.exists():
        raise ValueError('Standalone startup probes failed or created default state')
    receipt = {'source_commit':source, 'version':version, 'platform':platform,
               'archive_sha256':release.sha(archive), 'binary_sha256':release.sha(program), 'passed':True}
    (output/'probe.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
    return receipt


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode',choices=['prepare','standalone','catalog'])
    parser.add_argument('--artifacts',type=Path,required=True)
    parser.add_argument('--source',required=True)
    parser.add_argument('--version',required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--baseline-record',type=Path)
    parser.add_argument('--platform',choices=PLATFORMS)
    parser.add_argument('--xtask',type=Path)
    args=parser.parse_args()
    if args.mode=='prepare':
        if args.baseline_record is None: parser.error('prepare requires --baseline-record')
        result=prepare(args.artifacts.resolve(),args.baseline_record,args.source,args.version,args.output.absolute())
    elif args.mode=='standalone':
        if args.platform is None: parser.error('standalone requires --platform')
        result=standalone(args.artifacts.resolve(),args.platform,args.source,args.version,args.output.absolute())
    else:
        if args.xtask is None: parser.error('catalog requires --xtask')
        result=stage_catalog(args.artifacts.resolve(),args.source,args.version,args.output.absolute(),args.xtask)
    print(json.dumps(result))


if __name__=='__main__':
    main()
