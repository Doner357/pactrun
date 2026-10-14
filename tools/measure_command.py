"""Measure one explicit command; receipts are measurements, not qualification.

Do not pass credentials in command arguments. No scheduler, cache or retry policy
is implemented here. Earlier receipts and logs are never overwritten.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
BUILD_ENV = ('RUSTUP_TOOLCHAIN', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS',
             'CARGO_PROFILE_TEST_OPT_LEVEL', 'CARGO_PROFILE_TEST_DEBUG',
             'CARGO_PROFILE_TEST_DEBUG_ASSERTIONS', 'CARGO_PROFILE_TEST_OVERFLOW_CHECKS',
             'CARGO_BUILD_JOBS', 'CARGO_INCREMENTAL', 'CARGO_BUILD_TARGET',
             'RUST_TEST_THREADS', 'PACTRUN_TEST_RUNNER')


def source_identity():
    def git(*args):
        return subprocess.check_output(['git', *args], cwd=ROOT, stderr=subprocess.PIPE)
    try:
        # An archive extracted inside another checkout must not claim its parent's identity.
        if Path(git('rev-parse', '--show-toplevel').decode().strip()).resolve() != ROOT:
            return {'kind': 'unavailable', 'reason': 'not-this-source-root'}
        return {'kind': 'git', 'commit': git('rev-parse', 'HEAD').decode().strip(),
                'tree': git('rev-parse', 'HEAD^{tree}').decode().strip(),
                'tracked_diff_sha256': hashlib.sha256(git('diff', '--binary', 'HEAD')).hexdigest(),
                'untracked_paths': [p for p in git('ls-files', '--others', '--exclude-standard', '-z')
                                    .decode('utf-8').split('\0') if p]}
    except (OSError, subprocess.CalledProcessError, UnicodeError):
        return {'kind': 'unavailable', 'reason': 'git-metadata-unavailable'}


def measure(command, output, label, cwd=ROOT):
    if not command:
        raise ValueError('An explicit command is required')
    output = Path(output)
    log_path = output.with_suffix('.log')
    if output == log_path:
        raise ValueError('Receipt must not use the .log suffix')
    source = source_identity()
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open('x', encoding='utf-8', newline='\n') as receipt:
        record = {
            'format': 'pactrun.command-measurement.v1', 'label': label,
            'command': list(command), 'source': source,
            'build_environment': {k: os.environ[k] for k in BUILD_ENV if k in os.environ},
            'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'completed': False, 'exit_code': None, 'elapsed_seconds': None, 'log': str(log_path),
        }
        receipt.write(json.dumps(record, indent=2) + '\n')
        receipt.flush()
        started = time.perf_counter()
        try:
            with log_path.open('xb') as log:
                with subprocess.Popen(command, cwd=cwd, stdout=subprocess.PIPE,
                                      stderr=subprocess.STDOUT) as child:
                    console = getattr(sys.stdout, 'buffer', None)
                    try:
                        while chunk := child.stdout.read1(65536):
                            log.write(chunk)
                            log.flush()
                            if console is not None:
                                try:
                                    console.write(chunk)
                                    console.flush()
                                except (BrokenPipeError, OSError):
                                    console = None
                        code = child.wait()
                    except BaseException:
                        child.kill()
                        child.wait()
                        raise
            record.update(completed=True, exit_code=code)
        except OSError as error:
            record.update(error_type=type(error).__name__, exit_code=127)
        finally:
            record['elapsed_seconds'] = time.perf_counter() - started
            record['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
            receipt.seek(0)
            receipt.write(json.dumps(record, indent=2) + '\n')
            receipt.truncate()
    return record


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--label', required=True)
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    result = measure(command, args.output, args.label)
    print(json.dumps({k: result[k] for k in ('label', 'completed', 'exit_code', 'elapsed_seconds', 'log')}))
    return result['exit_code']


if __name__ == '__main__':
    sys.exit(main())
