"""Isolated standard-library checks for local artifact/source assembly."""
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile
from tools import release_artifacts as release

class ReleaseArtifacts(unittest.TestCase):
    def test_source_selection_keeps_runner_policy_without_arbitrary_local_configuration(self):
        self.assertTrue(release.selected('.config/nextest.toml'))
        self.assertFalse(release.selected('.config/private.toml'))
        self.assertFalse(release.selected('.configure-agent-workspace/profile.md'))
        self.assertFalse(release.selected('.env/ssh-connections/private.env'))

    def setUp(self):
        self.parent=Path(__file__).resolve().parents[1]/'target'/'release-artifact-tests'
        self.parent.mkdir(parents=True,exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(dir=self.parent)
        self.root=Path(self.temp.name)
        self.environment=patch.dict(os.environ, {'GIT_CONFIG_GLOBAL':os.devnull,'GIT_CONFIG_NOSYSTEM':'1'})
        self.environment.start()
    def tearDown(self):
        self.environment.stop()
        self.temp.cleanup()
    def git(self,*args):
        return subprocess.check_output(['git','-C',str(self.root/'source'),*args],stderr=subprocess.PIPE)
    def source_fixture(self):
        source=self.root/'source';source.mkdir()
        self.git('init')
        self.git('config','user.name','Pactrun artifact fixture')
        self.git('config','user.email','test@example.invalid')
        self.git('config','core.autocrlf','false')
        for name in ['Cargo.toml','Cargo.lock','build.rs','LICENSE','src/main.rs','src/cli/storage_config.rs','src/bin/pactrun-source.rs']:
            path=source/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(b'public fixture\n')
        self.git('add','.')
        self.git('commit','-m','isolated source fixture')
        return source

    def candidate_fixture(self):
        root = self.root/'candidate'
        (root/'output/assets').mkdir(parents=True)
        (root/'snapshot').mkdir()
        manifest = root/'snapshot/source.sha256'
        manifest.write_bytes(b'fixture manifest\n')
        version, platform = '1.0.0-alpha.9', 'windows-x86_64'
        part = dict(product_version=version, source_commit='a'*40,
                    source_manifest_sha256=release.sha(manifest), rustc='rustc fixture',
                    supported_formats={'cli_machine_interface':['1.0-alpha.4']},
                    default_formats={'cli_machine_interface':'1.0-alpha.4'}, artifacts=[])
        checks = {}
        for flavor in ['normal','test','standalone']:
            stem = 'pactrun-test' if flavor=='test' else 'pactrun'
            suffix = '-standalone' if flavor=='standalone' else ''
            name = f'{stem}-{version}-{platform}{suffix}.zip'
            archive = root/'output/assets'/name
            archive.write_bytes(('fixture '+flavor).encode())
            checks[name] = {'sha256':release.sha(archive),'bytes':archive.stat().st_size}
            if flavor != 'standalone':
                part['artifacts'].append(dict(platform=platform,flavor=flavor,
                    url='https://example.invalid/'+name,**checks[name]))
        (root/'output/release-part.json').write_text(json.dumps(part))
        (root/'output/artifact-checksums.json').write_text(json.dumps(checks))
        (root/'snapshot/source.json').write_text(json.dumps({k:part[k] for k in ['source_commit','source_manifest_sha256']}))
        (root/'output/binary-probe.json').write_text(json.dumps(dict(
            format='pactrun.cli',format_version='1.0-alpha.4',command='version',status='success',error=None,
            result=dict({k:v for k,v in part.items() if k!='artifacts'},build_target=release.TARGETS[platform]))))
        return root, part

    def test_candidate_verifier_binds_inventory_probe_and_requested_source(self):
        root, part = self.candidate_fixture()
        self.assertEqual(release.verify_candidate(root,'windows-x86_64','a'*40,'1.0.0-alpha.9'),part)
        for source,version in [('b'*40,'1.0.0-alpha.9'),('a'*40,'1.0.0-alpha.8')]:
            with self.assertRaises(ValueError):
                release.verify_candidate(root,'windows-x86_64',source,version)
        for path in ['snapshot/source.json','output/artifact-checksums.json','output/binary-probe.json']:
            file=root/path; original=file.read_bytes()
            file.write_text('{}')
            with self.assertRaises(ValueError): release.verify_candidate(root,'windows-x86_64')
            file.write_bytes(original)

    def test_candidate_verifier_rejects_tampering_and_wrong_native_metadata(self):
        root, part = self.candidate_fixture()
        archive=next((root/'output/assets').glob('*.zip')); original=archive.read_bytes()
        archive.write_bytes(b'changed')
        with self.assertRaises(ValueError): release.verify_candidate(root,'windows-x86_64')
        archive.write_bytes(original)
        part['artifacts'][0]['sha256']='0'*64
        (root/'output/release-part.json').write_text(json.dumps(part))
        with self.assertRaises(ValueError): release.verify_candidate(root,'windows-x86_64')

    def test_source_archive_requires_exact_complete_manifest_membership(self):
        source = self.source_fixture()
        snapshot = self.root/'snapshot'
        release.source(source,snapshot)
        expected = release.sha(snapshot/'source.sha256')
        release.verify_source_archive(snapshot,expected)
        original = (snapshot/'source.tar.gz').read_bytes()
        for extra, omit in [('extra.txt',False), (None,True)]:
            with tarfile.open(fileobj=io.BytesIO(original),mode='r:gz') as archive:
                members = [(member,archive.extractfile(member).read()) for member in archive]
            with tarfile.open(snapshot/'source.tar.gz','w:gz') as archive:
                for index,(member,data) in enumerate(members):
                    if omit and index==0: continue
                    archive.addfile(member,io.BytesIO(data))
                if extra:
                    member=tarfile.TarInfo(extra); member.size=1
                    archive.addfile(member,io.BytesIO(b'x'))
            with self.assertRaises(ValueError): release.verify_source_archive(snapshot,expected)

    def test_archives_are_reproducible_and_contain_only_the_explicit_program_layout(self):
        first=self.root/'program'
        first.write_bytes(b'program bytes\x00')
        files={'bin/pactrun':first}
        for windows,extension in [(True,'.zip'),(False,'.tar.gz')]:
            a=self.root/('first'+extension);b=self.root/('second'+extension)
            release.archive_files(a,files,windows)
            os.utime(first,(123456789,123456789))
            release.archive_files(b,files,windows)
            self.assertEqual(release.sha(a),release.sha(b))
            if windows:
                with zipfile.ZipFile(a) as archive:self.assertEqual(set(archive.namelist()),set(files))
            else:
                with tarfile.open(a) as archive:self.assertEqual(set(archive.getnames()),set(files))
            with self.assertRaises(FileExistsError):release.archive_files(a,files,windows)

    def test_every_delivery_contains_one_identical_product_image_and_no_launcher(self):
        product=self.root/'product';product.write_bytes(b'qualified product')
        license=self.root/'LICENSE';license.write_text('license')
        for windows in [False,True]:
            for flavor in ['normal','test','standalone']:
                with self.subTest(windows=windows,flavor=flavor):
                    name=('' if flavor=='standalone' else 'bin/')+('pactrun-test' if flavor=='test' else 'pactrun')+('.exe' if windows else '')
                    files=release.payload_files(product,{'LICENSE':license},flavor,windows)
                    self.assertEqual(files,{name:product,'LICENSE':license})
                    self.assertEqual(files[name].read_bytes(),b'qualified product')
        with self.assertRaises(ValueError): release.payload_files(product,{},'unknown',False)

    def test_candidate_source_requires_main_ancestry_or_exact_release_head(self):
        root=self.source_fixture(); old=self.git('rev-parse','HEAD').decode().strip()
        self.git('update-ref','refs/remotes/origin/main',old)
        release.qualify_candidate_source(root,old,'1.0.0-alpha.3','refs/heads/main',old)
        (root/'src/main.rs').write_text('new reviewed candidate\n',encoding='utf8')
        self.git('add','src/main.rs');self.git('commit','-m','candidate')
        current=self.git('rev-parse','HEAD').decode().strip()
        with self.assertRaises(RuntimeError):
            release.qualify_candidate_source(root,current,'1.0.0-alpha.3','refs/heads/main',old)
        release.qualify_candidate_source(root,current,'1.0.0-alpha.3','refs/heads/release/1.0.0-alpha.3',current)
        for ref,sha,commit in [('refs/heads/develop',current,current),
                               ('refs/heads/release/1.0.0-alpha.2',current,current),
                               ('refs/heads/release/1.0.0-alpha.3',old,current),
                               ('refs/heads/release/1.0.0-alpha.3',old,old)]:
            with self.subTest(ref=ref,sha=sha,commit=commit),self.assertRaises(ValueError):
                release.qualify_candidate_source(root,commit,'1.0.0-alpha.3',ref,sha)

    def test_candidate_workflow_remains_manual_read_only_and_qualifies_source(self):
        workflow=(Path(__file__).resolve().parents[1]/'.github/workflows/release-candidate.yml').read_text()
        self.assertIn('workflow_dispatch:',workflow)
        self.assertIn('contents: read',workflow)
        self.assertIn("qualify_candidate_source(root, sha, version, os.environ['GITHUB_REF'], os.environ['GITHUB_SHA'])",workflow)
        self.assertNotIn('contents: write',workflow)
        self.assertNotIn('gh release',workflow)
    def test_source_capture_is_committed_exact_and_preserves_unrelated_work(self):
        source=self.source_fixture();unrelated=source/'unrelated.zip';unrelated.write_bytes(b'leave alone')
        output=self.root/'snapshot';release.source(source,output)
        provenance=json.loads((output/'source.json').read_text(encoding='utf8'))
        self.assertEqual(provenance['source_commit'],self.git('rev-parse','HEAD').decode().strip())
        release.verify(source,output/'source.sha256',provenance['source_manifest_sha256'])
        with tarfile.open(output/'source.tar.gz') as archive:
            self.assertNotIn('unrelated.zip',archive.getnames())
            self.assertNotIn('.git',archive.getnames())
        self.assertEqual(unrelated.read_bytes(),b'leave alone')
        (source/'src/main.rs').write_bytes(b'uncommitted change')
        with self.assertRaises(ValueError):release.source(source,self.root/'refused')
        self.assertFalse((self.root/'refused').exists())
        with self.assertRaises(ValueError):release.verify(source,output/'source.sha256',provenance['source_manifest_sha256'])
    def test_source_capture_does_not_inherit_host_checkout_line_endings(self):
        source=self.source_fixture()
        first=self.root/'lf';release.source(source,first)
        self.git('config','core.autocrlf','true')
        self.git('config','core.eol','crlf')
        second=self.root/'crlf';release.source(source,second)
        for name in ['source.sha256','source.json','source.tar.gz']:
            self.assertEqual((first/name).read_bytes(),(second/name).read_bytes())
        with tarfile.open(second/'source.tar.gz') as archive:
            self.assertEqual(archive.extractfile('src/main.rs').read(),b'public fixture\n')

    def test_manifest_traversal_duplicate_and_checksum_mutation_are_refused(self):
        source=self.source_fixture();manifest=self.root/'manifest'
        digest=release.sha(source/'src/main.rs')
        for text in [digest+'  ../outside\n',digest+'  src/main.rs\n'+digest+'  src/main.rs\n','0'*64+'  src/main.rs\n']:
            manifest.write_text(text,encoding='utf8')
            with self.assertRaises(ValueError):release.verify(source,manifest,release.sha(manifest))
        self.assertFalse(release.selected('tools/__pycache__/generated.pyc'))
        self.assertFalse(release.safe_name('.env/ssh-connections/secret'))

    def test_dependency_notices_include_runtime_license_text_and_refuse_missing_notices(self):
        package=self.root/'dependency';package.mkdir()
        (package/'MIT-LICENSE').write_text('Copyright fixture\nPermission fixture\n',encoding='utf-8')
        metadata={'packages':[
            {'id':'root','name':'pactrun','version':'1','source':None},
            {'id':'dep','name':'example','version':'1','source':'registry','manifest_path':str(package/'Cargo.toml'),'license':'MIT'},
            {'id':'dev','name':'development-only','version':'1','source':'registry'}],
            'resolve':{'root':'root','nodes':[
                {'id':'root','deps':[{'pkg':'dep','dep_kinds':[{'kind':None}]},{'pkg':'dev','dep_kinds':[{'kind':'dev'}]}]},
                {'id':'dep','deps':[]},{'id':'dev','deps':[]}]}}
        notice=release.dependency_notices(metadata)
        self.assertIn('Copyright fixture',notice)
        self.assertNotIn('development-only',notice)
        self.assertNotIn(str(self.root),notice)
        (package/'MIT-LICENSE').unlink()
        with self.assertRaises(ValueError):release.dependency_notices(metadata)
        outside=self.root/'outside-license';outside.write_text('private outside fixture',encoding='utf8')
        metadata['packages'][1]['license_file']='../outside-license'
        with self.assertRaisesRegex(ValueError,'unsafe'):
            release.dependency_notices(metadata)

    def git_dependency_fixture(self, target):
        source=self.source_fixture()
        package=source/'sys';package.mkdir()
        (package/'Cargo.toml').write_text('[package]\nname="example-sys"\nversion="1.0.0"\n',encoding='utf8')
        # A regular working-tree placeholder reproduces Windows core.symlinks=false;
        # the committed entry still carries Git's symlink mode on every platform.
        (package/'LICENSE').write_text(target,encoding='utf8')
        self.git('add','sys/Cargo.toml')
        oid=self.git('hash-object','-w',str(package/'LICENSE')).decode().strip()
        self.git('update-index','--add','--cacheinfo',f'120000,{oid},sys/LICENSE')
        self.git('commit','-m','dependency with repository-relative license')
        revision=self.git('rev-parse','HEAD').decode().strip()
        dependency={'id':'dep','name':'example-sys','version':'1.0.0',
                    'source':'git+https://example.invalid/source#'+revision,
                    'manifest_path':str(package/'Cargo.toml'),'license':'MIT'}
        return source,dependency

    def notice_metadata(self, dependency):
        return {'packages':[{'id':'root','name':'pactrun','version':'1','source':None},dependency],
                'resolve':{'root':'root','nodes':[
                    {'id':'root','deps':[{'pkg':'dep','dep_kinds':[{'kind':None}]}]},
                    {'id':'dep','deps':[]}]}}

    def test_git_dependency_license_uses_pinned_blob_not_windows_link_placeholder(self):
        source,dependency=self.git_dependency_fixture('../LICENSE')
        notice=release.dependency_notices(self.notice_metadata(dependency))
        self.assertIn('public fixture',notice)
        self.assertNotIn('\n../LICENSE\n',notice)
        self.assertNotIn(str(self.root),notice)
        dependency['license_file']='../LICENSE'
        self.assertIn('public fixture',release.dependency_notices(self.notice_metadata(dependency)))
        dependency['license_file']='../../outside-license'
        with self.assertRaisesRegex(ValueError,'escapes'):
            release.dependency_notices(self.notice_metadata(dependency))
        dependency.pop('license_file')
        (source/'LICENSE').write_text('uncommitted-private-text',encoding='utf8')
        self.assertEqual(release.dependency_notices(self.notice_metadata(dependency)),notice)
        self.git('commit','--allow-empty','-m','different checkout identity')
        with self.assertRaisesRegex(ValueError,'locked commit'):
            release.dependency_notices(self.notice_metadata(dependency))

    def test_git_dependency_license_rejects_escape_absolute_and_cycles(self):
        _,dependency=self.git_dependency_fixture('../../outside')
        (self.root/'outside').write_text('private outside fixture',encoding='utf8')
        with self.assertRaisesRegex(ValueError,'escapes'):
            release.dependency_notices(self.notice_metadata(dependency))
        for target in ['/etc/passwd','C:/private','..\\private','LICENSE']:
            with self.subTest(target=target):
                file=self.root/'source'/'sys'/'LICENSE'
                file.write_text(target,encoding='utf8')
                oid=self.git('hash-object','-w',str(file)).decode().strip()
                self.git('update-index','--cacheinfo',f'120000,{oid},sys/LICENSE')
                self.git('commit','-m','unsafe license fixture')
                dependency['source']='git+https://example.invalid/source#'+self.git('rev-parse','HEAD').decode().strip()
                with self.assertRaises(ValueError):
                    release.dependency_notices(self.notice_metadata(dependency))

    def test_git_dependency_license_refuses_oversize_and_missing_committed_target(self):
        source,dependency=self.git_dependency_fixture('../LICENSE')
        (source/'LICENSE').write_bytes(b'x'*(release.LICENSE_LIMIT+1))
        self.git('add','LICENSE');self.git('commit','-m','oversized notice')
        dependency['source']='git+https://example.invalid/source#'+self.git('rev-parse','HEAD').decode().strip()
        with self.assertRaisesRegex(ValueError,'oversized'):
            release.dependency_notices(self.notice_metadata(dependency))
        self.git('rm','LICENSE');self.git('commit','-m','missing notice')
        dependency['source']='git+https://example.invalid/source#'+self.git('rev-parse','HEAD').decode().strip()
        with self.assertRaisesRegex(ValueError,'committed file'):
            release.dependency_notices(self.notice_metadata(dependency))

if __name__=='__main__':unittest.main()
