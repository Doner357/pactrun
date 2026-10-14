"""Import one exact successful main-CI Pages artifact without rebuilding it."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath, PureWindowsPath
import re
import shutil
import subprocess
import tarfile
import zipfile

from tools.publish_release import REPOSITORY, api

MAX_ARCHIVE = 512 * 1024 * 1024
MAX_SITE = 1024 * 1024 * 1024


def validate_producer(run,sha):
    if (run.get('head_sha')!=sha or run.get('head_branch')!='main' or run.get('event')!='push'
        or run.get('path')!='.github/workflows/ci.yml' or run.get('status')!='completed'
        or run.get('conclusion')!='success'
        or not isinstance(run.get('head_repository'),dict)
        or run['head_repository'].get('full_name')!=REPOSITORY):
        raise ValueError('Pages producer is not the successful selected main CI')


def inspect_site(path):
    names=set(); total=0; has_index=False
    with tarfile.open(path,'r|*') as archive:
        for entry in archive:
            name=PurePosixPath(entry.name)
            if (name.is_absolute() or '..' in name.parts or '\\' in entry.name or PureWindowsPath(entry.name).drive
                or any(part in {'.git','.env','.ssh','.aws'} for part in name.parts)
                or any(ord(c)<32 for c in entry.name)
                or not (entry.isfile() or entry.isdir()) or name.as_posix() in names
                or name.as_posix()=='.' and not entry.isdir()):
                raise ValueError('Unsafe site archive entry')
            names.add(name.as_posix()); total+=entry.size
            has_index = has_index or entry.isfile() and name.as_posix()=='index.html'
            if entry.size<0 or total>MAX_SITE or len(names)>100000: raise ValueError('Site archive exceeds its bound')
    if not has_index:
        raise ValueError('Site archive has no root entrypoint')


def import_site(run_id,sha,output):
    run=api(f'actions/runs/{run_id}')
    validate_producer(run,sha)
    if api('branches/main')['commit']['sha']!=sha:
        raise ValueError('Refusing superseded site publication')
    artifacts=api(f'actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
    selected=[artifact for artifact in artifacts if artifact['name']=='github-pages' and not artifact['expired']]
    if len(selected)!=1: raise ValueError('Missing or ambiguous Pages artifact')
    artifact=selected[0]
    if (type(artifact['size_in_bytes']) is not int or not 0<artifact['size_in_bytes']<=MAX_ARCHIVE
        or not re.fullmatch(r'sha256:[0-9a-f]{64}',artifact.get('digest') or '')):
        raise ValueError('Pages artifact lacks a bounded authenticated digest')
    if output.exists() or output.is_symlink(): raise FileExistsError(output)
    output.mkdir(parents=True)
    archive=output/'download.zip'
    with archive.open('xb') as stream:
        subprocess.run(['gh','api','--hostname','github.com',
            f"repos/{REPOSITORY}/actions/artifacts/{artifact['id']}/zip",'--allow-escape-sequences'],
            stdout=stream,check=True,timeout=300)
    if archive.stat().st_size>MAX_ARCHIVE:
        raise ValueError('Oversized Pages download')
    with archive.open('rb') as stream: digest=hashlib.file_digest(stream,'sha256').hexdigest()
    if 'sha256:'+digest!=artifact['digest']: raise ValueError('Pages download digest differs')
    site=output/'artifact.tar'
    with zipfile.ZipFile(archive) as data:
        entries=data.infolist()
        if len(entries)!=1 or entries[0].filename!='artifact.tar' or entries[0].file_size>MAX_SITE:
            raise ValueError('Unexpected Pages artifact members')
        with data.open(entries[0]) as src,site.open('xb') as dst: shutil.copyfileobj(src,dst)
    inspect_site(site)
    result={'main_sha':sha,'ci_run':run_id,'artifact_id':artifact['id'],'archive_sha256':digest,'verified':True}
    (output/'receipt.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-id',type=int,required=True)
    parser.add_argument('--sha',required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if args.run_id<=0 or not re.fullmatch('[0-9a-f]{40}',args.sha): parser.error('Invalid source identity')
    print(json.dumps(import_site(args.run_id,args.sha,args.output.absolute())))


if __name__=='__main__': main()
