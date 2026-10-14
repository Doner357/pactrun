"""Conservative reuse of completed same-repository runtime verification.

GitHub artifacts hold the receipts; there is no service or mutable result DB.
Generated delivery metadata is excluded only because its consumers run afresh.
Any unavailable, mismatched, dirty, foreign or expired evidence means rerun.
Manual dispatch always runs afresh so measurements cannot be cache hits.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import zipfile

try:
    from tools.ci_scope import delivery_metadata
    from tools.measure_command import BUILD_ENV
except ModuleNotFoundError:
    from ci_scope import delivery_metadata
    from measure_command import BUILD_ENV

ROOT = Path(__file__).resolve().parents[1]
FORMAT = 'pactrun.ci-runtime-receipt.v1'
MAX_ZIP = 262144
MAX_RECEIPT = 65536


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, stderr=subprocess.PIPE, timeout=30)


def input_digest(tree):
    selected = []
    for entry in tree.split(b'\0'):
        if not entry:
            continue
        metadata, name = entry.split(b'\t', 1)
        if not delivery_metadata(name.decode('utf-8')):
            selected.append(metadata + b'\t' + name + b'\0')
    return hashlib.sha256(b''.join(selected)).hexdigest()


def context(platform):
    repository = os.environ.get('GITHUB_REPOSITORY', '')
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository):
        raise ValueError('not a GitHub repository run')
    if os.environ.get('GITHUB_EVENT_NAME') == 'pull_request':
        event = json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text(encoding='utf-8'))
        if event['pull_request']['head']['repo']['full_name'] != repository:
            raise ValueError('fork verification is not reusable')
    if os.environ.get('PACTRUN_RUNNER_ENVIRONMENT') != 'github-hosted':
        raise ValueError('only the qualified hosted runner model is reusable')
    expected_os = {'linux': 'Linux', 'windows': 'Windows'}[platform]
    if os.environ.get('RUNNER_OS') != expected_os or not os.environ.get('ImageVersion'):
        raise ValueError('unknown runner image')
    if command('git', 'diff', '--binary', 'HEAD') or command('git', 'ls-files', '--others', '--exclude-standard'):
        raise ValueError('dirty source is not reusable')
    runner = os.environ.get('PACTRUN_TEST_RUNNER', 'nextest')
    if runner not in ('libtest', 'nextest'):
        raise ValueError('unknown runner')
    inputs = {
        'repository': repository, 'platform': platform,
        'runtime_inputs': input_digest(command('git', 'ls-tree', '-r', '-z', 'HEAD')),
        'workspace': str(ROOT), 'image': os.environ['ImageVersion'],
        'image_os': os.environ.get('ImageOS'), 'architecture': os.environ.get('RUNNER_ARCH'),
        'rustc': command('rustc', '--version', '--verbose').decode().strip(),
        'node': command('node', '--version').decode().strip(),
        'runner': runner,
        'nextest': command('cargo', 'nextest', '--version').decode().strip() if runner == 'nextest' else None,
        'build_environment': {key: os.environ.get(key) for key in BUILD_ENV},
    }
    return {'inputs': inputs, 'key': hashlib.sha256(json.dumps(inputs, sort_keys=True).encode()).hexdigest(),
            'commit': command('git', 'rev-parse', 'HEAD').decode().strip(),
            'tree': command('git', 'rev-parse', 'HEAD^{tree}').decode().strip(),
            'run_id': int(os.environ['GITHUB_RUN_ID']), 'attempt': int(os.environ['GITHUB_RUN_ATTEMPT'])}


def receipt_from_zip(data):
    if len(data) > MAX_ZIP:
        raise ValueError('oversized evidence archive')
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        if (len(entries) != 1 or entries[0].filename != 'runtime.json'
                or entries[0].file_size > MAX_RECEIPT or entries[0].flag_bits & 1):
            raise ValueError('unexpected evidence members')
        result = json.loads(archive.read(entries[0]))
        if not isinstance(result, dict):
            raise ValueError('invalid receipt object')
        return result


def acceptable_run(run, repository, current_id):
    return (isinstance(run, dict) and isinstance(run.get('head_repository'), dict)
            and type(run.get('id')) is int and run['id'] != current_id
            and run.get('status') == 'completed' and run.get('conclusion') == 'success'
            and run.get('path') == '.github/workflows/ci.yml'
            and run.get('event') in ('pull_request', 'push', 'workflow_dispatch')
            and run.get('head_repository', {}).get('full_name') == repository)


def matches(receipt, current, run):
    return (isinstance(receipt, dict) and receipt.get('format') == FORMAT and receipt.get('verified') is True
            and receipt.get('key') == current['key'] and receipt.get('inputs') == current['inputs']
            and type(receipt.get('run_id')) is int and type(receipt.get('attempt')) is int
            and receipt['run_id'] == run['id'] and receipt['attempt'] == run.get('run_attempt')
            and isinstance(receipt.get('commit'), str)
            and re.fullmatch('[0-9a-f]{40}', receipt['commit']) is not None)


def api(path):
    args = ['gh', 'api', '--hostname', 'github.com', path]
    if path.endswith('/zip'):
        # Binary bytes are captured, bounded and parsed, never sent to a terminal.
        args.append('--allow-escape-sequences')
    return command(*args)


def lookup(current):
    repository = current['inputs']['repository']
    runs = json.loads(api(f'repos/{repository}/actions/workflows/ci.yml/runs?status=completed&per_page=12'))
    for run in runs.get('workflow_runs', []):
        if not acceptable_run(run, repository, current['run_id']):
            continue
        name = f"runtime-proof-{current['inputs']['platform']}-{run['run_attempt']}"
        artifacts = json.loads(api(f"repos/{repository}/actions/runs/{run['id']}/artifacts?per_page=100"))
        for artifact in artifacts.get('artifacts', []):
            if (artifact.get('name') != name or artifact.get('expired') is not False
                    or type(artifact.get('id')) is not int
                    or type(artifact.get('size_in_bytes')) is not int
                    or not 0 < artifact.get('size_in_bytes', MAX_ZIP + 1) <= MAX_ZIP):
                continue
            receipt = receipt_from_zip(api(f"repos/{repository}/actions/artifacts/{artifact['id']}/zip"))
            if matches(receipt, current, run):
                return receipt
    return None


def record(current, measurement):
    data = json.loads(Path(measurement).read_text(encoding='utf-8'))
    if not isinstance(data, dict) or not isinstance(data.get('source'), dict):
        raise ValueError('invalid measurement')
    if (data.get('completed') is not True or type(data.get('exit_code')) is not int
            or data['exit_code'] != 0 or data.get('command') != ['cargo', 'xtask', 'rust-ci']
            or data.get('source', {}).get('kind') != 'git'
            or data['source'].get('commit') != current['commit']
            or data['source'].get('tree') != current['tree']
            or data['source'].get('tracked_diff_sha256') != hashlib.sha256(b'').hexdigest()
            or data['source'].get('untracked_paths') != []):
        raise ValueError('runtime command did not qualify this source')
    return dict(current, format=FORMAT, verified=True, elapsed_seconds=data['elapsed_seconds'])


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['check', 'record'])
    parser.add_argument('--platform', choices=['linux', 'windows'], required=True)
    parser.add_argument('--measurement', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args(argv)
    receipt = None
    try:
        current = context(args.platform)
        if args.mode == 'check':
            if os.environ.get('GITHUB_EVENT_NAME') != 'workflow_dispatch':
                receipt = lookup(current)
        else:
            if args.measurement is None or args.output is None:
                raise ValueError('record requires measurement and output')
            receipt = record(current, args.measurement)
            args.output.parent.mkdir(parents=True, exist_ok=True)
            with args.output.open('x', encoding='utf-8') as output:
                json.dump(receipt, output, indent=2)
    except (OSError, subprocess.SubprocessError, ValueError, KeyError, TypeError, zipfile.BadZipFile):
        # Failure to reuse must never turn into an unverified success or block a fresh run.
        print('No reusable runtime evidence; fresh verification is required.')
    if args.mode == 'check':
        with open(os.environ['GITHUB_OUTPUT'], 'a', encoding='utf-8') as output:
            output.write(f"reused={str(receipt is not None).lower()}\n")
        if receipt is not None:
            print(f"Reusing completed runtime verification from run {receipt['run_id']} attempt {receipt['attempt']}; delivery consumers still run afresh.")
        if os.environ.get('GITHUB_STEP_SUMMARY'):
            with open(os.environ['GITHUB_STEP_SUMMARY'], 'a', encoding='utf-8') as summary:
                result = (f"Reused successful run {receipt['run_id']}, attempt {receipt['attempt']}."
                          if receipt is not None else 'Fresh runtime verification required.')
                summary.write(f"### {args.platform} verification\n{result}\n\nDelivery consumers and current documentation are checked separately.\n")
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
