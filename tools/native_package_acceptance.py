"""Source-qualified, isolated native-manager rehearsal; not an installer.
Inputs are real release-part.json files and their sibling assets directories.
Requires an explicitly authorized transient Scoop shim PATH change on Windows.
"""
import argparse, datetime, functools, hashlib, http.server, json, os
from pathlib import Path
import shutil, signal, sqlite3, subprocess, tempfile, threading, time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--root', type=Path, required=True)
parser.add_argument('--manager', type=Path, required=True)
parser.add_argument('--xtask', type=Path, required=True)
parser.add_argument('--parts', type=Path, nargs=4, required=True)
parser.add_argument('--allow-user-path-change', action='store_true')
parser.add_argument('--public-source', help='Verify actual published acquisition instead of the isolated upgrade fixture')
parser.add_argument('--versions', nargs=2, default=['1.0.0-alpha.1','1.0.0-alpha.2'],
                    metavar=('BASELINE', 'CANDIDATE'), help='Explicit baseline and candidate artifact versions')
parser.add_argument('--public-version', default='1.0.0-alpha.1',
                    help='Expected moving package version for public acquisition; exact test package stays on the baseline')
args = parser.parse_args()
if len(set(args.versions)) != 2 or (args.public_source and args.public_version not in args.versions):
    parser.error('Choose two distinct artifact versions and a matching public version')
WIN = os.name == 'nt'
if WIN and not args.allow_user_path_change:
    parser.error('Scoop needs explicit authorization for a temporary user PATH entry')
ROOT = args.root.resolve()
ROOT.mkdir(parents=True, exist_ok=False)
for name in ['assets','evidence','home','tmp','cache','logs','data']:
    (ROOT/name).mkdir()
ENV = dict(os.environ, TEMP=str(ROOT/'tmp'), TMP=str(ROOT/'tmp'), TMPDIR=str(ROOT/'tmp'),
    XDG_CONFIG_HOME=str(ROOT/'home/config'), XDG_CACHE_HOME=str(ROOT/'cache'),
    LOCALAPPDATA=str(ROOT/'data'), XDG_DATA_HOME=str(ROOT/'data'),
    GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM='1', NO_PROXY='127.0.0.1,localhost',
    GIT_AUTHOR_NAME='Pactrun qualification', GIT_AUTHOR_EMAIL='fixture@invalid.local',
    GIT_COMMITTER_NAME='Pactrun qualification', GIT_COMMITTER_EMAIL='fixture@invalid.local')
ENV.pop('PACTRUN_STORAGE_ROOT',None)
RESULTS, COMMANDS, CHILDREN = [], [], []
FLAGS = {'creationflags':subprocess.CREATE_NO_WINDOW} if WIN else {}

def save():
    for name, data in [('results',RESULTS),('commands',COMMANDS)]:
        (ROOT/'evidence'/f'{name}.json').write_text(json.dumps(data,indent=2),encoding='utf-8')
def check(name, passed, detail=''):
    RESULTS.append(dict(name=name,passed=bool(passed),detail=detail)); save()
    print(('PASS ' if passed else 'FAIL ')+name,flush=True)
    if not passed: raise AssertionError(name+': '+str(detail))
def run(cmd, label, env=None, required=True, timeout=240):
    p=subprocess.Popen([str(x) for x in cmd],cwd=ROOT,env=env or ENV,
        stdout=subprocess.PIPE,stderr=subprocess.STDOUT,start_new_session=not WIN,**FLAGS)
    try: out=p.communicate(timeout=timeout)[0].decode('utf-8',errors='replace')
    except subprocess.TimeoutExpired:
        if WIN: p.kill()
        else: os.killpg(p.pid,signal.SIGTERM)
        out=p.communicate(timeout=15)[0].decode('utf-8',errors='replace')
        raise RuntimeError(label+' timeout: '+out[-500:])
    log=f'{len(COMMANDS):03d}-{label}.log'
    (ROOT/'evidence'/log).write_text(out,encoding='utf-8')
    COMMANDS.append(dict(label=label,args=[str(x) for x in cmd],code=p.returncode,log=log)); save()
    if required and p.returncode: raise RuntimeError(label+': '+out[-2000:])
    return p.returncode,out

def sha(p):
    h=hashlib.sha256()
    with p.open('rb') as f:
        for b in iter(lambda:f.read(1024*1024),b''): h.update(b)
    return h.hexdigest()
class Handler(http.server.SimpleHTTPRequestHandler):
    def copyfile(self,source,output):
        try: super().copyfile(source,output)
        except (BrokenPipeError,ConnectionResetError): pass
    def log_message(self,fmt,*values):
        with (ROOT/'evidence/http.log').open('a') as f: f.write(fmt%values+'\n')
SERVER=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Handler,directory=str(ROOT/'assets')))
threading.Thread(target=SERVER.serve_forever,daemon=True).start()
URL=f'http://127.0.0.1:{SERVER.server_port}'
RELEASES={}
for part in args.parts:
    data=json.loads(part.read_text(encoding='utf-8'))
    version=data['product_version']
    release=RELEASES.setdefault(version,dict(data,artifacts=[]))
    for key in data:
        if key!='artifacts' and data[key]!=release[key]: raise ValueError('inconsistent '+key)
    for artifact in data['artifacts']:
        artifact=dict(artifact)
        name=artifact['url'].rsplit('/',1)[1]
        file=part.parent/'assets'/name
        if file.exists():
            check('artifact checksum '+name,sha(file)==artifact['sha256'] and file.stat().st_size==artifact['bytes'])
            shutil.copyfile(file,ROOT/'assets'/name)
        artifact['url']=URL+'/'+name
        release['artifacts'].append(artifact)
assert set(RELEASES)==set(args.versions)
VERSIONS=list(args.versions)
BASELINE_EXACT=VERSIONS[0].replace('.', '-').replace('+', '-')
PUBLISHER=ROOT/'source'
PACKAGES={'normal':'pactrun-preview','test':'pactrun-test-exact-'+BASELINE_EXACT}
def publish(versions):
    source=ROOT/'releases.json'
    source.write_text(json.dumps([RELEASES[v] for v in versions]),encoding='utf-8')
    output=ROOT/('catalog-'+str(len(COMMANDS)))
    command=[args.xtask,'release-catalog',source,output]
    if PUBLISHER.exists(): command.append(PUBLISHER/'releases/catalog.json')
    run(command,'generate-catalog')
    if not PUBLISHER.exists():
        run(['git','init','--initial-branch=main',PUBLISHER],'init-source')
        (PUBLISHER/'README.md').write_text('Ordinary project default branch fixture.\n',encoding='utf-8')
    shutil.copytree(output,PUBLISHER,dirs_exist_ok=True)
    run(['git','-C',PUBLISHER,'add','README.md','bucket','Formula','releases'],'stage-catalog')
    run(['git','-C',PUBLISHER,'commit','-m','Publish reviewed package definitions'],'commit-catalog')

ORIGINAL_PATH=None
MANAGER=ROOT/('scoop' if WIN else 'brew')
def manager_setup():
    global PM, ORIGINAL_PATH
    if WIN:
        import winreg
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER,'Environment') as k:
            try: ORIGINAL_PATH=winreg.QueryValueEx(k,'Path')
            except FileNotFoundError: pass
        run(['git','clone','--no-hardlinks',args.manager,MANAGER/'apps/scoop/current'],'scoop-clone')
        run(['git','-C',MANAGER/'apps/scoop/current','checkout','--detach','b588a06e41d920d2123ec70aee682bae14935939'],'scoop-pin')
        for d in ['buckets','shims','cache','persist']: (MANAGER/d).mkdir()
        ENV.update(SCOOP=str(MANAGER),SCOOP_GLOBAL=str(ROOT/'global'),SCOOP_CACHE=str(ROOT/'cache'))
        (MANAGER/'config.json').write_text(json.dumps(dict(last_update=datetime.datetime.now().isoformat(),hold_update_until='2100-01-01',aria2_enabled=False)),encoding='utf-8')
        shim=MANAGER/'shims/scoop.ps1'
        shim.write_text('$path = Join-Path $PSScriptRoot "../apps/scoop/current/bin/scoop.ps1"\n& $path @args\nexit $LASTEXITCODE\n',encoding='utf-8')
        PM=[shutil.which('pwsh'),'-NoLogo','-NoProfile','-NonInteractive','-File',shim]
    else:
        run(['git','clone','--no-hardlinks',args.manager,MANAGER],'brew-clone')
        run(['git','-C',MANAGER,'checkout','--detach','570982948a8a194f0f42f43f4a5bce2d1c9f64cb'],'brew-pin')
        ruby=Path('Library/Homebrew/vendor/portable-ruby')
        shutil.copytree(args.manager/ruby,MANAGER/ruby,symlinks=True,dirs_exist_ok=True)
        ENV.update(HOME=str(ROOT/'home'),HOMEBREW_CACHE=str(ROOT/'cache'),HOMEBREW_LOGS=str(ROOT/'logs'),
            HOMEBREW_TEMP=tempfile.mkdtemp(prefix='e-pm-',dir='/tmp'),HOMEBREW_NO_AUTO_UPDATE='1',
            HOMEBREW_NO_ANALYTICS='1',HOMEBREW_NO_GITHUB_API='1',HOMEBREW_NO_ENV_HINTS='1',
            HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK='1',HOMEBREW_FORCE_VENDOR_RUBY='1')
        for key in ['HOMEBREW_NO_INSTALL_CLEANUP','HOMEBREW_NO_CLEANUP_FORMULAE','HOMEBREW_NO_INSTALL_FROM_API']:
            ENV.pop(key,None)
        ENV['TMPDIR']=ENV['HOMEBREW_TEMP']
        PM=[MANAGER/'bin/brew']
        mirror=ROOT/'brew-source.git'
        run(['git','clone','--bare',args.manager,mirror],'brew-owned-source')
        run(['git','--git-dir',mirror,'update-ref','refs/heads/main','570982948a8a194f0f42f43f4a5bce2d1c9f64cb'],'brew-owned-main')
        run(['git','--git-dir',mirror,'symbolic-ref','HEAD','refs/heads/main'],'brew-owned-head')
        run(['git','-C',MANAGER,'remote','set-url','origin',mirror],'brew-local-origin')
        run(['git','-C',MANAGER,'checkout','-B','main','570982948a8a194f0f42f43f4a5bce2d1c9f64cb'],'brew-local-main')
        ENV['HOMEBREW_BREW_GIT_REMOTE']=str(mirror)
    if WIN:
        empty=ROOT/'empty-main'
        run(['git','init',empty],'empty-main-init')
        run(['git','-C',empty,'commit','--allow-empty','-m','isolated empty main bucket'],'empty-main-commit')
        run(['git','clone',empty,MANAGER/'buckets/main'],'empty-main-clone')
    run(PM+['--version'],'manager-version')
    source_url=args.public_source or PUBLISHER.as_uri()
    run(PM+(['bucket','add','pactrun',source_url] if WIN else ['tap','--custom-remote','doner357/pactrun',source_url]),'add-native-source')
    (ROOT/'evidence/manager-environment.json').write_text(json.dumps({k:v for k,v in ENV.items() if k.startswith(('SCOOP','HOMEBREW'))},indent=2),encoding='utf-8')
def source_path(flavor):
    return MANAGER/('buckets/pactrun' if WIN else 'Library/Taps/doner357/homebrew-pactrun')
def app(flavor): return 'pactrun' if flavor=='normal' else 'pactrun-test'
def qualified(flavor): return ('pactrun/' if WIN else 'doner357/pactrun/')+PACKAGES[flavor]
def pm(operation,flavor='normal',required=True,extra=()):
    name=PACKAGES[flavor] if WIN and operation in ['hold','unhold','cleanup','uninstall'] else qualified(flavor)
    return run(PM+[operation,*extra,name],operation+'-'+flavor,required=required)
def binary(flavor='normal'):
    return MANAGER/('shims/'+app(flavor)+'.exe' if WIN else 'bin/'+app(flavor))
def observed(flavor='normal'):
    _,out=run([binary(flavor),'--format','json','--version'],'version-'+flavor)
    return json.loads(out)['result']['product_version']
# Remaining scenario functions are appended below.

DATA=ROOT/'data'
BARRIER=DATA/'barrier'
def prepare_data():
    BARRIER.mkdir()
    source=DATA/'pack'; source.mkdir()
    for flavor in ['normal','test']:
        root=DATA/app(flavor)
        for d in ['database','runtime-content','staging']: (root/d).mkdir(parents=True)
    script = """param([string]$barrier)
[IO.File]::WriteAllText((Join-Path $barrier 'ready'), $env:PACTRUN_EXECUTABLE)
while (-not (Test-Path (Join-Path $barrier 'release'))) { Start-Sleep -Milliseconds 50 }
& $env:PACTRUN_EXECUTABLE hook session | Out-File (Join-Path $barrier 'session.json')
$code=$LASTEXITCODE
[IO.File]::WriteAllText((Join-Path $barrier 'helper-code'), [string]$code)
exit $code
""" if WIN else """barrier="$1"
printf '%s' "$PACTRUN_EXECUTABLE" > "$barrier/ready"
while ! test -f "$barrier/release"; do sleep 0.05; done
"$PACTRUN_EXECUTABLE" hook session > "$barrier/session.json"
code=$?
printf '%s' "$code" > "$barrier/helper-code"
exit "$code"
"""
    (source/'script.txt').write_text(script,encoding='utf-8')
    shell,command,suffix=('powershell_7','pwsh.exe','ps1') if WIN else ('sh','sh','sh')
    yaml=f"""source_format: '1.0-alpha.2'
package_id: '00000000000000000000000000000777'
revision:
  actions:
    - id: run
      access: observe
      parameters: []
      hook:
        protocol_version: '1.0-alpha.1'
        launch: {{kind: shell_loader, shell: {shell}, command: {command}, script: script}}
        args: {json.dumps([str(BARRIER)])}
        io: {{terminal: output}}
      outputs: []
runtime_content:
  files:
    - {{id: script, source: script.txt, path: scripts/script.{suffix}, executable: false}}
"""
    (source/'pactrun.yaml').write_text(yaml,encoding='utf-8')
    for flavor in ['normal','test']:
        _,out=run([binary(flavor),'pack','install',source],'pack-install-'+flavor)
        revision=out.splitlines()[0].strip()
        run([binary(flavor),'instance','create',flavor,'--revision',revision],'create-'+flavor)
    code,_=run([binary('test'),'instance','show','normal'],'test-root-isolation',required=False)
    check('normal and test use distinct default management roots',code!=0)
    run([binary(),'instance','show','normal'],'normal-root-control')
    e=dict(ENV,PACTRUN_STORAGE_ROOT=str(DATA/'pactrun'))
    run([binary('test'),'instance','show','normal'],'explicit-root-forwarding',env=e)
    (BARRIER/'release').write_text('control')
    run([binary(),'invoke','normal','run','--action-timeout-ms','120000'],'hook-control')
    check('current baseline Hook handshake succeeds',(BARRIER/'helper-code').read_text()=='0')

def live(operation,label):
    for name in ['release','ready','helper-code','session.json']:
        path=BARRIER/name
        if path.exists(): path.unlink()
    stream=(ROOT/'evidence'/f'live-{label}.log').open('wb')
    child=subprocess.Popen([str(binary()),'invoke','normal','run','--action-timeout-ms','120000'],
        cwd=ROOT,env=ENV,stdout=stream,stderr=stream,**FLAGS)
    CHILDREN.append((child,stream))
    end=time.monotonic()+20
    while not (BARRIER/'ready').exists():
        if child.poll() is not None or time.monotonic()>end: raise RuntimeError('live Hook not ready: '+label)
        time.sleep(.05)
    service=DATA/'service.bin'
    if not service.exists(): service.write_bytes(b'isolated-service-resource-not-managed-by-installer')
    expected=sha(service); stop=threading.Event(); errors=[]; reads=[]
    def watch():
        with service.open('rb') as f:
            while not stop.wait(.01):
                try:
                    f.seek(0)
                    if hashlib.sha256(f.read()).hexdigest()!=expected or sha(service)!=expected: errors.append('changed')
                    reads.append(time.monotonic())
                except OSError as exc: errors.append(str(exc))
    thread=threading.Thread(target=watch); thread.start()
    try: result=operation()
    finally:
        stop.set(); thread.join(timeout=5)
        (BARRIER/'release').write_text('continue')
        child.wait(timeout=30); stream.close()
    check('service access continues during '+label,bool(reads) and not errors,dict(samples=len(reads),errors=errors))
    helper=(BARRIER/'helper-code').read_text() if (BARRIER/'helper-code').exists() else 'missing'
    check('active Hook survives '+label,child.returncode==0 and helper=='0',dict(exit=child.returncode,helper=helper))
    return result

def immutable_state():
    with sqlite3.connect((DATA/'pactrun/database/pactrun.sqlite3').as_uri()+'?mode=ro',uri=True) as db:
        return [db.execute(query).fetchall() for query in [
            'SELECT instance_id,active_package_id,active_revision_content_digest,instance_state_version FROM instances ORDER BY instance_id',
            'SELECT package_id,revision_content_digest,core_jcs,runtime_content_jcs FROM revisions ORDER BY package_id,revision_content_digest',
            'SELECT * FROM pactrun_metadata ORDER BY singleton']]

def scenarios():
    initial = args.public_version if args.public_source else VERSIONS[0]
    if not args.public_source: publish([VERSIONS[0]])
    manager_setup()
    pm('install'); pm('install','test')
    check('moving and exact packages match selected published versions',observed()==initial and observed('test')==VERSIONS[0])
    check('installation did not provision management data',not (DATA/'pactrun').exists() and not (DATA/'pactrun-test').exists())
    prepare_data()
    before=immutable_state()
    stable=('pactrun/' if WIN else 'doner357/pactrun/')+'pactrun'
    code,_=run(PM+['install',stable],'stable-unavailable',required=False)
    check('stable cannot fall back to alpha',code!=0 and observed()==initial)
    conflict=('pactrun/' if WIN else 'doner357/pactrun/')+'pactrun-exact-'+BASELINE_EXACT
    code,_=run(PM+['install',conflict],'command-conflict',required=False)
    check('second package cannot take over the command',code!=0 and observed()==initial)
    if args.public_source:
        run(PM+['update'],'public-source-refresh')
        check('public source stays on ordinary main',run(['git','-C',source_path('normal'),'branch','--show-current'],'public-source-branch')[1].strip()=='main')
        live(lambda:pm('uninstall',required=False),'public-uninstall')
        if binary().exists(): pm('uninstall')
        pm('install')
        check('public reinstall retains data',immutable_state()==before and observed()==initial)
        pm('uninstall'); pm('uninstall','test')
        check('public uninstall retains data',immutable_state()==before)
        return
    pm('hold' if WIN else 'pin')
    if WIN:
        info=json.loads((MANAGER/'apps'/PACKAGES['normal']/'current/install.json').read_text(encoding='utf-8-sig'))
        check('Scoop installed record is actually held',info.get('hold') is True and info.get('bucket')=='pactrun')
    publish(VERSIONS)
    run(PM+['update'],'native-source-refresh')
    manager_repo=MANAGER/'apps/scoop/current' if WIN else MANAGER
    expected='b588a06e41d920d2123ec70aee682bae14935939' if WIN else '570982948a8a194f0f42f43f4a5bce2d1c9f64cb'
    check('native metadata refresh keeps manager source pinned',run(['git','-C',manager_repo,'rev-parse','HEAD'],'manager-source-after-refresh')[1].strip()==expected)
    check('native refresh follows ordinary main',run(['git','-C',source_path('normal'),'branch','--show-current'],'source-branch')[1].strip()=='main')
    check('selection changes metadata only',observed()==VERSIONS[0])
    check('exact package stays on its version',observed('test')==VERSIONS[0])
    pm('update' if WIN else 'upgrade',required=False)
    check('native hold or pin prevents upgrade',observed()==VERSIONS[0])
    pm('unhold' if WIN else 'unpin')
    live(lambda:pm('update' if WIN else 'upgrade',required=False),'upgrade')
    if observed()!=VERSIONS[1]: pm('update' if WIN else 'upgrade')
    check('real candidate upgrade completes',observed()==VERSIONS[1])
    run([binary(),'instance','show','normal'],'existing-instance-after-upgrade')
    check('upgrade preserves existing identities and canonical bytes',immutable_state()==before)
    run([binary(),'pack','install',DATA/'pack'],'pack-origin-independent-after-upgrade')
    check('newer software reuses exactly the same Pack identity',immutable_state()==before)
    check('test installation unaffected',observed('test')==VERSIONS[0])
    live(lambda:pm('cleanup',required=False),'cleanup')
    pm('cleanup')
    pm('update' if WIN else 'upgrade',required=False)
    check('ordinary update never downgrades',observed()==VERSIONS[1])
    live(lambda:pm('uninstall',required=False),'explicit-switch')
    if binary().exists(): pm('uninstall')
    PACKAGES['normal']='pactrun-exact-'+BASELINE_EXACT
    pm('install')
    check('explicit native reinstall can select older exact',observed()==VERSIONS[0])
    check('explicit switch does not rewrite object identity',immutable_state()==before)
    # Corrupt only this owned cloned source. Do not modify the immutable publisher.
    definition=source_path('test')/(('bucket/'+PACKAGES['test']+'.json') if WIN else ('Formula/'+PACKAGES['test']+'.rb'))
    original=definition.read_text(encoding='utf-8')
    release=RELEASES[VERSIONS[0]]
    artifact=next(x for x in release['artifacts'] if x['flavor']=='test' and x['platform']==('windows-x86_64' if WIN else 'linux-x86_64'))
    definition.write_text(original.replace(artifact['sha256'],'0'*64),encoding='utf-8')
    pm('uninstall','test')
    code,_=pm('install','test',required=False)
    check('wrong checksum refuses installation',code!=0 and not binary('test').exists())
    definition.write_text(original,encoding='utf-8')
    pm('install','test')
    check('failed installation can recover using the original package',observed('test')==VERSIONS[0] and immutable_state()==before)
    relocated=ROOT/'relocated-source'
    run(['git','clone',PUBLISHER,relocated],'relocate-source')
    run(['git','-C',source_path('normal'),'remote','set-url','origin',relocated.as_uri()],'change-source-location')
    PUBLISHER.rename(ROOT/'retired-source')
    run(PM+['update'],'refresh-after-relocation')
    check('source relocation needs neither reinstall nor data migration',observed()==VERSIONS[0] and immutable_state()==before)
    live(lambda:pm('uninstall',required=False),'uninstall')
    if binary().exists(): pm('uninstall')
    check('uninstall completes after active operation',not binary().exists())
    e=dict(ENV,PACTRUN_STORAGE_ROOT=str(DATA/'pactrun'))
    run([binary('test'),'instance','show','normal'],'data-preserved-after-uninstall',env=e)
    pm('uninstall','test')

try:
    scenarios()
except Exception as exc:
    RESULTS.append(dict(name='acceptance execution',passed=False,detail=str(exc))); save()
    raise
finally:
    for child,stream in CHILDREN:
        if child.poll() is None:
            (BARRIER/'release').write_text('finalizer')
            try: child.wait(timeout=20)
            except subprocess.TimeoutExpired: child.kill(); child.wait()
        if not stream.closed: stream.close()
    SERVER.shutdown(); SERVER.server_close()
    if WIN and MANAGER.exists():
        import winreg
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER,'Environment',0,winreg.KEY_READ|winreg.KEY_SET_VALUE) as k:
            try: current,kind=winreg.QueryValueEx(k,'Path')
            except FileNotFoundError: current,kind='',winreg.REG_EXPAND_SZ
            ours=str(MANAGER/'shims').rstrip('\\/').casefold()
            cleaned=';'.join(p for p in current.split(';') if p.rstrip('\\/').casefold()!=ours)
            if cleaned!=current:
                if ORIGINAL_PATH is not None and cleaned==ORIGINAL_PATH[0]: kind=ORIGINAL_PATH[1]
                if not cleaned and ORIGINAL_PATH is None: winreg.DeleteValue(k,'Path')
                else: winreg.SetValueEx(k,'Path',0,kind,cleaned)
            before_path=ORIGINAL_PATH[0] if ORIGINAL_PATH else ''
            (ROOT/'evidence/path-fingerprints.json').write_text(json.dumps(dict(before=hashlib.sha256(before_path.encode()).hexdigest(),after=hashlib.sha256(cleaned.encode()).hexdigest())),encoding='utf-8')
            check('only temporary shim PATH entry removed',cleaned==before_path)
        import ctypes
        from ctypes import wintypes
        notify=ctypes.WinDLL('user32',use_last_error=True).SendMessageTimeoutW
        notify.argtypes=[wintypes.HWND,wintypes.UINT,wintypes.WPARAM,wintypes.LPCWSTR,wintypes.UINT,wintypes.UINT,ctypes.POINTER(ctypes.c_size_t)]
        notify.restype=wintypes.LPARAM
        result=ctypes.c_size_t()
        notify(65535,26,0,'Environment',2,3000,ctypes.byref(result))
    save()
