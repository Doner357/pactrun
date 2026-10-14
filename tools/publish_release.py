"""Promote exact qualified bytes from reviewed main; never rebuild or clobber.

Only the manually dispatched main publication workflow may call publish().
GitHub remains the state store. Repeating the same request verifies existing
assets; changed inputs, unexpected assets and moved refs fail closed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import tomllib
import urllib.request

from tools import release_artifacts as release
from tools.ci_scope import delivery_metadata
from tools.qualify_candidate import combined_record, PLATFORMS

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = 'Doner357/pactrun'


def command(args, *, data=None, timeout=120):
    result = subprocess.run(args,cwd=ROOT,input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=timeout)
    if result.returncode:
        status = re.search(rb'HTTP ([0-9]{3})', result.stderr)
        raise GitHubError(int(status[1]) if status else None)
    return result.stdout


class GitHubError(RuntimeError):
    def __init__(self,status):
        self.status=status
        super().__init__(f'GitHub command failed (status {status}); no response body is exposed')


def api(path, method='GET', payload=None, optional=False):
    args=['gh','api','--hostname','github.com',f'repos/{REPOSITORY}/{path}','--method',method]
    data=None
    if payload is not None:
        args.extend(['--input','-']); data=json.dumps(payload).encode()
    try:
        return json.loads(command(args,data=data))
    except GitHubError as error:
        if optional and error.status==404: return None
        raise


def equivalent_record(record):
    return dict(record,artifacts=sorted(record['artifacts'],key=lambda a:(a['platform'],a['flavor'])))


def write_exact(path,data):
    if path.is_symlink(): raise ValueError('Staged output must not be a link')
    if path.exists():
        if path.read_bytes()!=data: raise ValueError('Existing staging content differs')
    else:
        with path.open('xb') as stream: stream.write(data)


def stage(artifacts,record_path,output):
    record=json.loads(record_path.read_text(encoding='utf-8'))
    version, source=record['product_version'],record['source_commit']
    candidate=combined_record(artifacts,source,version)
    if equivalent_record(record)!=equivalent_record(candidate):
        raise ValueError('Reviewed release record differs from candidate')
    sources={}
    for platform in PLATFORMS:
        root=artifacts/('candidate-'+platform)
        checks=json.loads((root/'output/artifact-checksums.json').read_text())
        sources.update({name:root/'output/assets'/name for name in checks})
    snapshot=artifacts/'candidate-linux-x86_64/snapshot'
    sources.update({f'pactrun-{version}-build-source.tar.gz':snapshot/'source.tar.gz',
                    'source.json':snapshot/'source.json','source.sha256':snapshot/'source.sha256',
                    'release.json':record_path})
    if output.is_symlink(): raise ValueError('Staging directory must not be a link')
    output.mkdir(parents=True,exist_ok=True)
    if any(p.name not in {*sources,'SHA256SUMS'} for p in output.iterdir()):
        raise ValueError('Unexpected staged release file')
    checks={}
    for name,source_file in sources.items():
        target=output/name
        digest,size=release.sha(source_file),source_file.stat().st_size
        if target.is_symlink(): raise ValueError('Staged asset must not be a link')
        if target.exists():
            if target.stat().st_size!=size or release.sha(target)!=digest:
                raise ValueError('Existing staged asset differs')
        else:
            with source_file.open('rb') as src,target.open('xb') as dst: shutil.copyfileobj(src,dst)
        checks[name]={'sha256':digest,'bytes':size}
    sums=''.join(f"{value['sha256']}  {name}\n" for name,value in sorted(checks.items())).encode()
    write_exact(output/'SHA256SUMS',sums)
    checks['SHA256SUMS']={'sha256':hashlib.sha256(sums).hexdigest(),'bytes':len(sums)}
    return record, checks


def verify_assets(assets,expected,allow_missing=False):
    names=[asset['name'] for asset in assets]
    if len(names)!=len(set(names)) or set(names)-set(expected):
        raise ValueError('Unexpected or duplicate uploaded asset')
    for asset in assets:
        entry=expected[asset['name']]
        if (asset.get('state')!='uploaded' or asset.get('size')!=entry['bytes']
            or asset.get('digest')!='sha256:'+entry['sha256']):
            raise ValueError('Uploaded bytes differ; existing assets are never replaced')
    missing=set(expected)-set(names)
    if missing and not allow_missing: raise ValueError('Release asset inventory is incomplete')
    return sorted(missing)


def require_main(main,record):
    if (os.environ.get('GITHUB_ACTIONS')!='true' or os.environ.get('GITHUB_EVENT_NAME')!='workflow_dispatch'
        or os.environ.get('GITHUB_REF')!='refs/heads/main'
        or os.environ.get('GITHUB_REPOSITORY')!=REPOSITORY or os.environ.get('GITHUB_SHA')!=main):
        raise ValueError('Publication requires an explicit current-main dispatch')
    if api('branches/main')['commit']['sha']!=main:
        raise ValueError('Main advanced; re-evaluate publication instead of publishing stale inputs')
    if command(['git','rev-parse','HEAD']).decode().strip()!=main:
        raise ValueError('Checkout is not the reviewed main commit')
    if command(['git','diff','--binary','HEAD']): raise ValueError('Publication checkout is dirty')
    version=tomllib.loads((ROOT/'Cargo.toml').read_text())['package']['version']
    if version!=record['product_version']: raise ValueError('Version differs from reviewed main')
    command(['git','merge-base','--is-ancestor',record['source_commit'],main])
    changes=command(['git','diff','--name-only','-z',record['source_commit'],main]).decode().split('\0')
    if any(path and not delivery_metadata(path) for path in changes):
        raise ValueError('Non-delivery inputs changed since candidate qualification')


def require_candidate(run,record):
    if (not isinstance(run,dict) or not isinstance(run.get('head_repository'),dict)
        or run.get('path')!='.github/workflows/release-candidate.yml' or run.get('event')!='workflow_dispatch'
        or run.get('status')!='completed' or run.get('conclusion')!='success'
        or run.get('head_repository',{}).get('full_name')!=REPOSITORY
        or run.get('head_branch') not in ('main','release/'+record['product_version'])):
        raise ValueError('Candidate run is not qualified')
    if run['head_sha']!=record['source_commit']:
        if run['head_branch']!='main': raise ValueError('Candidate source mismatch')
        command(['git','merge-base','--is-ancestor',record['source_commit'],run['head_sha']])


def wait_main_ci(main):
    run=None
    for _ in range(12):
        runs=api(f'actions/workflows/ci.yml/runs?head_sha={main}&event=push&per_page=20')['workflow_runs']
        run=next((r for r in runs if r['head_sha']==main and r['event']=='push' and r['head_branch']=='main'),None)
        if run: break
        time.sleep(5)
    if run is None: raise ValueError('No main CI run exists for the selected commit')
    print(f"Waiting for main CI run {run['id']} before publication",flush=True)
    if run['status']!='completed':
        command(['gh','run','watch',str(run['id']),'--repo',REPOSITORY,'--exit-status','--interval','10'],timeout=3600)
    current=api(f"actions/runs/{run['id']}")
    if current['status']!='completed' or current['conclusion']!='success':
        raise ValueError('Main CI has not qualified publication')
    return current['id']


def verify_public(version,checks):
    for name,expected in checks.items():
        url=f'https://github.com/{REPOSITORY}/releases/download/v{version}/{name}'
        digest,size=hashlib.sha256(),0
        # Intentionally anonymous: an authenticated API response is not public acquisition.
        with urllib.request.urlopen(url,timeout=120) as stream:
            for block in iter(lambda:stream.read(1024*1024),b''):
                size+=len(block)
                if size>expected['bytes']: raise ValueError('Public download exceeds expected size')
                digest.update(block)
        if size!=expected['bytes'] or digest.hexdigest()!=expected['sha256']:
            raise ValueError('Public download differs from qualified bytes')


def publish(record,checks,directory,notes,main,candidate_run):
    require_main(main,record)
    require_candidate(api(f'actions/runs/{candidate_run}'),record)
    ci_run=wait_main_ci(main)
    require_main(main,record)
    version=record['product_version']; tag='v'+version
    ref=api('git/ref/tags/'+tag,optional=True)
    if ref and (ref['object']['type']!='commit' or ref['object']['sha']!=main):
        raise ValueError('Existing tag must never move')
    release_info=api('releases/tags/'+tag,optional=True)
    if release_info is None:
        release_info=api('releases','POST',dict(tag_name=tag,target_commitish=main,name='Pactrun '+version,
            body=notes,draft=True,prerelease='-' in version,make_latest='false'))
    if release_info['tag_name']!=tag or release_info['prerelease']!=('-' in version):
        raise ValueError('Release identity or channel differs')
    if not release_info['draft'] and ref is None:
        raise ValueError('Published release has no verified tag')
    missing=verify_assets(release_info['assets'],checks,allow_missing=release_info['draft'])
    if not release_info['draft'] and missing: raise ValueError('Published release is incomplete')
    if release_info['draft']:
        for name in missing:
            print('Uploading verified asset '+name,flush=True)
            command(['gh','release','upload',tag,str(directory/name),'--repo',REPOSITORY],timeout=600)
        release_info=api(f"releases/{release_info['id']}")
        if release_info['tag_name']!=tag or not release_info['draft']:
            raise ValueError('Draft changed during preparation')
        verify_assets(release_info['assets'],checks)
        require_main(main,record)
        ref=api('git/ref/tags/'+tag,optional=True)
        if ref is None:
            api('git/refs','POST',dict(ref='refs/tags/'+tag,sha=main))
        elif ref['object']['type']!='commit' or ref['object']['sha']!=main:
            raise ValueError('Tag changed during publication')
        release_info=api(f"releases/{release_info['id']}",'PATCH',
                         dict(draft=False,body=notes,prerelease='-' in version,make_latest='false'))
    verify_assets(release_info['assets'],checks)
    if release_info['draft'] or release_info['tag_name']!=tag:
        raise ValueError('Release was not published as requested')
    ref=api('git/ref/tags/'+tag)
    if ref['object']['type']!='commit' or ref['object']['sha']!=main:
        raise ValueError('Published tag differs from reviewed main')
    print('Verifying every asset by anonymous public download',flush=True)
    verify_public(version,checks)
    return {'version':version,'source_commit':record['source_commit'],'tag_commit':main,
            'release_id':release_info['id'],'candidate_run':candidate_run,'main_ci_run':ci_run,
            'public_assets_verified':len(checks),'published':True}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version',required=True)
    parser.add_argument('--candidate-run',type=int,required=True)
    parser.add_argument('--artifacts',type=Path,required=True)
    parser.add_argument('--notes',type=Path,required=True)
    parser.add_argument('--expected-main',required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if (not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:-(?:alpha|beta|rc)\.[1-9][0-9]*)?',args.version)
        or not re.fullmatch('[0-9a-f]{40}',args.expected_main) or args.candidate_run<=0):
        parser.error('Invalid release identity')
    notes=args.notes.read_text(encoding='utf-8')
    if not notes.strip() or len(notes.encode())>32768: parser.error('Provide bounded product release notes')
    record,checks=stage(args.artifacts,ROOT/'releases'/f'{args.version}.json',args.output/'assets')
    result=publish(record,checks,args.output/'assets',notes,args.expected_main,args.candidate_run)
    (args.output/'publication.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(result))


if __name__=='__main__':
    main()
