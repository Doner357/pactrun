"""Run existing version/reader browser checks; always stop the owned preview."""
import argparse
import os
from pathlib import Path
import socket
import subprocess
import time
import urllib.error
import urllib.request

ROOT=Path(__file__).resolve().parents[1]


def checks(base,output):
    base=base.rstrip('/')+'/'
    for script,url,name in [('verify-publication.mjs',base,'publication'),
                            ('version-browser.mjs',base,'versions'),
                            ('role-browser.mjs',base.rstrip('/')+'/next/','roles')]:
        subprocess.run(['node',str(ROOT/'website/scripts'/script),url,str(output/name)],cwd=ROOT,check=True)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--url',help='Public verification; otherwise start a loopback-only preview')
    args=parser.parse_args()
    if not os.environ.get('PLAYWRIGHT_PACKAGE'): parser.error('PLAYWRIGHT_PACKAGE is required')
    args.output.mkdir(parents=True,exist_ok=False)
    if args.url:
        checks(args.url,args.output)
        return
    with socket.socket() as probe:
        probe.bind(('127.0.0.1',0)); port=probe.getsockname()[1]
    base=f"http://127.0.0.1:{port}"+os.environ.get('DOCUSAURUS_BASE_URL','/')
    command=['node','node_modules/@docusaurus/core/bin/docusaurus.mjs','serve',
             '--host','127.0.0.1','--port',str(port),'--no-open']
    with (args.output/'preview.log').open('wb') as log:
        process=subprocess.Popen(command,cwd=ROOT/'website',stdout=log,stderr=subprocess.STDOUT,
                                 env=dict(os.environ,NO_UPDATE_NOTIFIER='1'))
        try:
            for _ in range(120):
                if process.poll() is not None: raise RuntimeError('Preview exited before readiness')
                try:
                    with urllib.request.urlopen(base,timeout=2) as response:
                        if response.status==200: break
                except (OSError,urllib.error.URLError): pass
                time.sleep(.5)
            else: raise TimeoutError('Preview did not become ready')
            checks(base,args.output)
        finally:
            if process.poll() is None:
                process.terminate()
                try: process.wait(timeout=15)
                except subprocess.TimeoutExpired: process.kill(); process.wait()


if __name__=='__main__': main()
