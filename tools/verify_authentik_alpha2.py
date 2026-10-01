"""Opt-in real-service regression. Requires local Docker access, never runs in CI.

Use an absolute alpha.2 executable and a new persistent workspace. This harness
creates disposable loopback services; failures preserve evidence/resources for
explicit inspection, never delete guessed storage paths or escalate Pactrun.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import time
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--workspace', type=Path, required=True)
    parser.add_argument('--port', type=int, default=29000)
    args = parser.parse_args()
    if os.getuid() == 0 or not args.cli.is_absolute() or not args.workspace.is_absolute():
        parser.error('require a non-root user and absolute executable/workspace paths')
    if not 1024 <= args.port <= 65530:
        parser.error('port must leave room for four adjacent unprivileged ports')
    version = subprocess.check_output([str(args.cli), '--version'], text=True).strip()
    if '1.0.0-alpha.2' not in version:
        parser.error('require the alpha.2 candidate')
    args.workspace.mkdir(mode=0o700, parents=False, exist_ok=False)
    root = args.workspace
    evidence = root / 'evidence'
    evidence.mkdir()
    private = root / 'private'
    private.mkdir(mode=0o700)
    pack = root / 'author-source'
    shutil.copytree(Path(__file__).parent / 'fixtures/authentik-alpha2', pack)
    for store in ('store-a', 'store-b'):
        for part in ('database', 'runtime-content', 'staging'):
            (root / store / part).mkdir(parents=True)
    receipts = []

    def cli(label, *operands, store='store-a', success=True):
        result = subprocess.run([str(args.cli), '--format', 'json', *map(str, operands)],
                                env=dict(os.environ, PACTRUN_STORAGE_ROOT=str(root / store)),
                                capture_output=True, text=True, timeout=600)
        (evidence / (label + '.json')).write_text(result.stdout)
        (evidence / (label + '.stderr')).write_text(result.stderr)
        doc = json.loads(result.stdout)
        good = result.returncode == 0 and doc['status'] == 'success'
        receipts.append({'label': label, 'exit': result.returncode, 'status': doc['status']})
        (evidence / 'commands.json').write_text(json.dumps(receipts, indent=2))
        if good != success:
            raise RuntimeError('unexpected CLI outcome: ' + label + '; inspect private evidence')
        print(label, 'passed', flush=True)
        return doc

    def write_private(name, value):
        target = private / name
        target.write_text(json.dumps(value))
        target.chmod(0o600)
        return target

    config = {
        'authentik_image': 'ghcr.io/goauthentik/server@sha256:a10398480e7f8292dbcc27b64fe572f6abed6220bd40f4b6d28e9c12d4b78dca',
        'postgres_image': 'postgres@sha256:721873c34ceb9f8d8fc265984940dc982404c105f19ad51be9fdc5970a6080ea',
        'redis_image': 'redis@sha256:858f009f9709ce576febc734aa78b8f6d624b82571f9ddb6bda4377c833b3499',
        'bind_address': '127.0.0.1', 'http_port': args.port, 'https_port': args.port + 1,
    }
    # Do not replace immutable images with whatever tag happens to be current.
    for image in (config[k] for k in ('authentik_image', 'postgres_image', 'redis_image')):
        subprocess.run(['docker', 'image', 'inspect', image], check=True, stdout=subprocess.DEVNULL)
    credentials = {key: secrets.token_hex(32) for key in
                   ('postgres_password', 'secret_key', 'bootstrap_password', 'bootstrap_token')}
    credentials_path = write_private('credentials.json', credentials)
    config_a = write_private('config-a.json', config)
    config_b = write_private('config-b.json', dict(config, http_port=args.port + 2, https_port=args.port + 3))
    installed = cli('install', 'pack', 'install', pack)['result']['revision']
    reference = 'exact:' + installed['package_id'] + '/' + installed['content_digest']
    cli('export', 'revision', 'export', reference, '--output', root / 'portable')
    imported = cli('import', 'pack', 'install', root / 'portable.pack', store='store-b')['result']['revision']
    assert imported == installed
    pack.rename(root / 'author-source-held')
    for name, store, cfg in [('normal', 'store-a', config_a), ('imported', 'store-b', config_b)]:
        cli(name + '-create', 'instance', 'create', name, '--revision', reference,
            '--input-file', 'config=' + str(cfg), '--input-file', 'credentials=' + str(credentials_path), store=store)
        cli(name + '-start', 'invoke', name, 'start', store=store)

    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def api(port, suffix='', data=None):
        url = f'http://127.0.0.1:{port}/api/v3/core/users/' + suffix
        request = urllib.request.Request(url, data=None if data is None else json.dumps(data).encode(),
                                        headers={'Authorization': 'Bearer ' + credentials['bootstrap_token'],
                                                 'Content-Type': 'application/json'})
        with opener.open(request, timeout=15) as response:
            return json.load(response)

    for port in (args.port, args.port + 2):
        for attempt in range(100):
            try:
                if any(u['username'] == 'akadmin' for u in api(port, '?username=akadmin')['results']):
                    break
            except (urllib.error.URLError, TimeoutError):
                pass
            time.sleep(3)
        else:
            raise RuntimeError('authenticated API did not become ready')
    username = 'alpha2-' + secrets.token_hex(8)
    created = api(args.port, data={'username': username, 'name': 'alpha2 persistence probe',
                                 'is_active': True, 'path': 'users', 'attributes': {'probe': 'alpha2'}})
    cli('normal-stop', 'invoke', 'normal', 'stop')
    cli('normal-restart', 'invoke', 'normal', 'start')
    found = api(args.port, '?username=' + username)['results']
    assert any(u['pk'] == created['pk'] and u['attributes'].get('probe') == 'alpha2' for u in found)
    cli('conflict-create', 'instance', 'create', 'conflict', '--revision', reference,
        '--input-file', 'config=' + str(config_a), '--input-file', 'credentials=' + str(credentials_path))
    failed = cli('conflict-start', 'invoke', 'conflict', 'start', success=False)
    assert failed['result']['current_recovery_guard'] is not None
    for name, store, extra in [('conflict', 'store-a', ['--authorize-recovery-override']),
                               ('normal', 'store-a', []), ('imported', 'store-b', [])]:
        result = cli(name + '-delete', 'instance', 'delete', name, *extra, store=store)
        state = result['result']['run']['state']
        assert state['outcome'] == 'succeeded' and state['hook_completion_status'] == 'success'
    for store in ('store-a', 'store-b'):
        assert not cli(store + '-instances', 'instance', 'list', store=store)['result']['items']
        assert not cli(store + '-detached', 'service-storage', 'detached', 'list', store=store)['result']['items']
    for secret in credentials.values():
        for file in evidence.glob('*'):
            assert secret not in file.read_text(), 'credential in captured evidence'
    summary = {'version': version, 'executable_sha256': hashlib.sha256(args.cli.read_bytes()).hexdigest(),
               'uid': os.getuid(), 'gid': os.getgid(), 'revision': installed,
               'api_and_persistence': 'passed', 'three_retirements': 'passed',
               'scope': 'Linux same-host corrected Pack; not TLS, SSO, upgrade or old-data permission repair',
               'commands': len(receipts)}
    (evidence / 'summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary), flush=True)


if __name__ == '__main__':
    main()
