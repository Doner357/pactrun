"""Isolated standard-library checks for local artifact/source assembly."""
import hashlib
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
        for name in ['Cargo.toml','Cargo.lock','build.rs','src/main.rs','src/bin/pactrun-launcher.rs','src/bin/pactrun-source.rs']:
            path=source/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(b'public fixture\n')
        self.git('add','.')
        self.git('commit','-m','isolated source fixture')
        return source
    def test_archives_are_reproducible_and_contain_only_the_explicit_program_layout(self):
        first=self.root/'program';second=self.root/'launcher'
        first.write_bytes(b'program bytes\x00');second.write_bytes(b'launcher bytes')
        files={'libexec/pactrun':first,'bin/pactrun':second}
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

if __name__=='__main__':unittest.main()
