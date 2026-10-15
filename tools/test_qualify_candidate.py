"""Offline qualification glue tests. No package manager or network is started."""
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from tools import qualify_candidate as qualify
from tools.public_repository import release_repository


class QualificationTests(unittest.TestCase):
    def setUp(self):
        (qualify.ROOT/'target').mkdir(exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(prefix='qualification-test-',dir=qualify.ROOT/'target')
        self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)
        self.baseline=self.record('1.0.0-alpha.8')
        self.candidate=self.record('1.0.0-alpha.9')
        self.record_file=self.root/'baseline.json'
        self.record_file.write_text(json.dumps(self.baseline))

    def record(self,version):
        formats={'pack_source':'1.0-alpha.2','cli_machine_interface':'1.0-alpha.4',
                 'persistence':'1.0-alpha.6','hook_protocol':'1.0-alpha.1'}
        artifacts=[]
        for platform in qualify.PLATFORMS:
            for flavor in ['normal','test']:
                stem='pactrun' if flavor=='normal' else 'pactrun-test'
                extension='.zip' if platform=='windows-x86_64' else '.tar.gz'
                name=f'{stem}-{version}-{platform}{extension}'
                artifacts.append(dict(platform=platform,flavor=flavor,
                    url=f'https://github.com/{release_repository(version)}/releases/download/v{version}/{name}',
                    bytes=3,sha256=hashlib.sha256(b'abc').hexdigest()))
        return dict(product_version=version,source_commit='a'*40,source_manifest_sha256='b'*64,
                    supported_formats={k:[v] for k,v in formats.items()},default_formats=formats,artifacts=artifacts)

    def test_transfer_accepts_historical_and_new_assets_but_rejects_wrong_publishers(self):
        for version, owner, foreign in [('1.0.0-alpha.9', 'Doner357', 'Pipapoen'),
                                         ('1.0.0-alpha.10', 'Pipapoen', 'Doner357')]:
            artifact = self.record(version)['artifacts'][0]
            self.assertIn('/'+owner+'/pactrun/', artifact['url'])
            self.assertTrue(qualify.public_name(artifact, version))
            for replacement in [foreign, 'untrusted']:
                with self.assertRaises(ValueError):
                    qualify.public_name(dict(artifact, url=artifact['url'].replace('/'+owner+'/', '/'+replacement+'/')), version)

    def test_preparation_uses_explicit_versions_and_never_installs_packages(self):
        downloaded=[]
        def download(artifact,version,path):
            downloaded.append((version,path.name)); path.write_bytes(b'abc')
        with patch.object(qualify.release,'verify_candidate',return_value=self.candidate) as verify, \
             patch.object(qualify.release,'verify_source_archive') as source:
            result=qualify.prepare(self.root/'candidates',self.record_file,'a'*40,
                                   '1.0.0-alpha.9',self.root/'prepared',download)
            self.assertEqual(verify.call_count,2)
            self.assertEqual(source.call_count,2)
        self.assertEqual(result['versions'],['1.0.0-alpha.8','1.0.0-alpha.9'])
        self.assertFalse(result['store_upgrade'])
        self.assertEqual(len(result['parts']),4)
        self.assertTrue(all(Path(p).is_absolute() for p in result['parts']))
        self.assertEqual(len(downloaded),4)

    def test_invalid_baseline_fails_before_output_or_download(self):
        for mutate in [lambda r:r.update(product_version='1.0.0-alpha.9'),
                       lambda r:r['artifacts'].pop(),
                       lambda r:r['artifacts'][0].update(url='https://foreign.invalid/payload')]:
            record=self.record('1.0.0-alpha.8'); mutate(record)
            self.record_file.write_text(json.dumps(record))
            with patch.object(qualify.release,'verify_candidate',return_value=self.candidate), \
                 patch.object(qualify.release,'verify_source_archive'), \
                 patch.object(qualify,'download') as download:
                with self.assertRaises(ValueError):
                    qualify.prepare(self.root/'candidates',self.record_file,'a'*40,
                                    '1.0.0-alpha.9',self.root/'prepared',download)
                download.assert_not_called()
            self.assertFalse((self.root/'prepared').exists())

    def test_public_download_checks_size_and_digest_and_preserves_existing_files(self):
        artifact=self.baseline['artifacts'][0]
        target=self.root/'asset'
        with patch.object(qualify.urllib.request,'urlopen',return_value=io.BytesIO(b'abc')):
            qualify.download(artifact,'1.0.0-alpha.8',target)
        self.assertEqual(target.read_bytes(),b'abc')
        with patch.object(qualify.urllib.request,'urlopen',return_value=io.BytesIO(b'abc')):
            with self.assertRaises(FileExistsError): qualify.download(artifact,'1.0.0-alpha.8',target)
        for index,payload in enumerate([b'ab',b'abcd',b'bad']):
            with patch.object(qualify.urllib.request,'urlopen',return_value=io.BytesIO(payload)):
                with self.assertRaises(ValueError):
                    qualify.download(artifact,'1.0.0-alpha.8',self.root/f'bad-{index}')


if __name__=='__main__':
    unittest.main()
