import hashlib
import io
from pathlib import Path
import tempfile
import tarfile
import unittest
from unittest.mock import patch
import zipfile

from tools import pages_artifact as pages
from tools.publish_release import ROOT, REPOSITORY


class PagesArtifactTests(unittest.TestCase):
    def setUp(self):
        (ROOT/'target').mkdir(exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(prefix='pages-artifact-test-',dir=ROOT/'target')
        self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)
        self.sha='a'*40
        self.run=dict(head_sha=self.sha,head_branch='main',event='push',path='.github/workflows/ci.yml',
                      status='completed',conclusion='success',head_repository={'full_name':REPOSITORY})

    def tar(self,names):
        output=io.BytesIO()
        with tarfile.open(fileobj=output,mode='w') as archive:
            for name in names:
                entry=tarfile.TarInfo(name); entry.size=1
                archive.addfile(entry,io.BytesIO(b'x'))
        return output.getvalue()

    def test_only_exact_successful_main_producer_is_eligible(self):
        pages.validate_producer(self.run,self.sha)
        for changes in [{'head_sha':'b'*40},{'event':'pull_request'},{'conclusion':'failure'},
                        {'head_branch':'develop'},{'head_repository':None},
                        {'head_repository':{'full_name':'fork/repo'}}]:
            with self.assertRaises(ValueError): pages.validate_producer(dict(self.run,**changes),self.sha)

    def test_site_members_cannot_escape_or_fake_a_root_entrypoint(self):
        path=self.root/'site.tar'
        path.write_bytes(self.tar(['./index.html','assets/main.js']))
        pages.inspect_site(path)
        for names in [['../index.html'],['/index.html'],['C:/index.html'],['.../index.html'],
                      ['index.html','./index.html'],['index.html','.env/secret']]:
            path.write_bytes(self.tar(names))
            with self.assertRaises(ValueError): pages.inspect_site(path)

    def test_import_checks_server_digest_before_exposing_the_tar(self):
        zipped=io.BytesIO()
        with zipfile.ZipFile(zipped,'w') as archive:
            archive.writestr('artifact.tar',self.tar(['index.html']))
        payload=zipped.getvalue()
        artifact=dict(id=1,name='github-pages',expired=False,size_in_bytes=len(payload),
                      digest='sha256:'+hashlib.sha256(payload).hexdigest())
        def api(path):
            if path=='actions/runs/10': return self.run
            if path=='branches/main': return {'commit':{'sha':self.sha}}
            if path.endswith('artifacts?per_page=100'): return {'artifacts':[artifact]}
            raise AssertionError(path)
        def download(args,stdout,**kwargs):
            self.assertIn('--allow-escape-sequences',args)
            stdout.write(payload)
        with patch.object(pages,'api',side_effect=api),patch.object(pages.subprocess,'run',side_effect=download):
            self.assertTrue(pages.import_site(10,self.sha,self.root/'good')['verified'])
            artifact['digest']='sha256:'+'0'*64
            with self.assertRaises(ValueError): pages.import_site(10,self.sha,self.root/'bad')
        self.assertFalse((self.root/'bad/artifact.tar').exists())


if __name__=='__main__': unittest.main()
