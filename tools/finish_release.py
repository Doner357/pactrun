"""Owner-authenticated Git Flow closeout; do not grant Actions PR approval rights.

This uses existing local gh authentication and GitHub's required CI checks.
It never force-pushes, deletes main/develop, or resolves divergent integration.
"""
import argparse
import json
import os
import re

from tools.publish_release import REPOSITORY, api, command


def finish(version,main,publication_run,merge=False):
    if os.environ.get('GITHUB_ACTIONS')=='true':
        raise ValueError('Use existing local owner authentication, not the Actions token')
    run=api(f'actions/runs/{publication_run}')
    if (run.get('path')!='.github/workflows/publish-release.yml' or run.get('head_sha')!=main
        or run.get('event')!='workflow_dispatch' or run.get('status')!='completed' or run.get('conclusion')!='success'):
        raise ValueError('Public verification has not completed for this main commit')
    if api('branches/main')['commit']['sha']!=main:
        raise ValueError('Main changed; review closeout again')
    release=api('releases/tags/v'+version)
    ref=api('git/ref/tags/v'+version)
    if release['draft'] or ref['object']['type']!='commit' or ref['object']['sha']!=main:
        raise ValueError('Release and tag do not match the approved main commit')
    develop=api('branches/develop')['commit']['sha']
    comparison=api(f'compare/{develop}...{main}')['status']
    if comparison in ('identical','behind'):
        return {'version':version,'back_merged':True,'already_contained':True}
    if comparison!='ahead': raise ValueError('Develop diverged; explicit integration review is required')
    if not merge: return {'version':version,'ready':True,'main':main,'develop':develop}
    protection=api('branches/develop/protection')
    checks=protection.get('required_status_checks') or {}
    if checks.get('strict') is not True or 'CI gate' not in checks.get('contexts',[]):
        raise ValueError('Up-to-date required CI must protect automatic closeout')
    title='release: back-merge '+version
    pulls=api('pulls?state=open&base=develop&head=Doner357:main')
    if pulls:
        if len(pulls)!=1 or pulls[0]['title']!=title or pulls[0]['head']['sha']!=main:
            raise ValueError('An unrelated back-merge PR needs review')
        pr=pulls[0]
    else:
        pr=api('pulls','POST',dict(base='develop',head='main',title=title,
            body=f'Back-merge the verified {version} release at {main}. Publication workflow {publication_run} completed. Required CI remains enforced; this does not close development measurement work.'))
    command(['gh','pr','checks',str(pr['number']),'--repo',REPOSITORY,'--watch','--fail-fast'],timeout=3600)
    if api('branches/main')['commit']['sha']!=main or api('branches/develop')['commit']['sha']!=develop:
        raise ValueError('A branch advanced while CI ran; do not merge unchecked inputs')
    command(['gh','pr','merge',str(pr['number']),'--repo',REPOSITORY,'--merge','--match-head-commit',main])
    after=api('branches/develop')['commit']['sha']
    if api(f'compare/{main}...{after}')['status'] not in ('ahead','identical'):
        raise ValueError('Develop does not contain the released commit')
    return {'version':version,'back_merged':True,'pr':pr['number'],'main':main,'develop':after}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version',required=True)
    parser.add_argument('--expected-main',required=True)
    parser.add_argument('--publication-run',type=int,required=True)
    parser.add_argument('--merge',action='store_true',help='Perform the reviewed back-merge through protected PR checks')
    args=parser.parse_args()
    if (not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:-(?:alpha|beta|rc)\.[1-9][0-9]*)?',args.version)
        or not re.fullmatch('[0-9a-f]{40}',args.expected_main) or args.publication_run<=0):
        parser.error('Invalid publication identity')
    print(json.dumps(finish(args.version,args.expected_main,args.publication_run,args.merge)))


if __name__=='__main__': main()
