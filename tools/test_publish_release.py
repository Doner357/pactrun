"""Publication state tests use fake APIs; they never upload, tag or publish."""
import copy
import hashlib
import os
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path

from tools import publish_release as publish


class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.main='a'*40
        self.record={'product_version':'1.0.0-alpha.9','source_commit':'b'*40}
        self.checks={'payload.zip':{'sha256':hashlib.sha256(b'payload').hexdigest(),'bytes':7}}
        self.assets=[{'name':'payload.zip','size':7,'digest':'sha256:'+self.checks['payload.zip']['sha256'],'state':'uploaded'}]
        self.run={'path':'.github/workflows/release-candidate.yml','event':'workflow_dispatch',
                  'status':'completed','conclusion':'success','head_repository':{'full_name':publish.REPOSITORY},
                  'head_sha':'b'*40,'head_branch':'release/1.0.0-alpha.9'}

    def test_unqualified_foreign_and_wrong_source_candidates_are_refused(self):
        publish.require_candidate(self.run,self.record)
        for changes in [{'conclusion':'failure'},{'status':'in_progress'},{'head_sha':'c'*40},
                        {'head_repository':None},{'head_repository':{'full_name':'fork/repo'}},
                        {'path':'.github/workflows/ci.yml'},{'head_branch':'feature/unreviewed'}]:
            with self.assertRaises(ValueError): publish.require_candidate(dict(self.run,**changes),self.record)

    def test_assets_cannot_be_replaced_extended_or_silently_omitted(self):
        self.assertEqual(publish.verify_assets(self.assets,self.checks),[])
        self.assertEqual(publish.verify_assets([],self.checks,True),['payload.zip'])
        for assets in [[],self.assets*2,self.assets+[dict(self.assets[0],name='unexpected')],
                       [dict(self.assets[0],digest='sha256:'+'0'*64)],
                       [dict(self.assets[0],size=8)],[dict(self.assets[0],state='open')]]:
            with self.assertRaises(ValueError): publish.verify_assets(assets,self.checks)

    def test_wrong_event_or_repository_cannot_publish(self):
        with patch.dict(os.environ,GITHUB_ACTIONS='true',GITHUB_EVENT_NAME='pull_request',
                        GITHUB_REF='refs/heads/main',GITHUB_REPOSITORY=publish.REPOSITORY,GITHUB_SHA=self.main), \
             patch.object(publish,'api') as api:
            with self.assertRaises(ValueError): publish.require_main(self.main,self.record)
            api.assert_not_called()

    def test_same_published_release_is_verified_without_writes_or_uploads(self):
        release={'id':1,'tag_name':'v1.0.0-alpha.9','draft':False,'prerelease':True,'assets':self.assets}
        calls=[]
        def api(path,method='GET',payload=None,optional=False):
            calls.append((path,method))
            if path.startswith('actions/runs/'): return self.run
            if path.startswith('git/ref/'): return {'object':{'type':'commit','sha':self.main}}
            if path.startswith('releases/tags/'): return copy.deepcopy(release)
            raise AssertionError(path)
        with patch.object(publish,'require_main'),patch.object(publish,'wait_main_ci',return_value=99), \
             patch.object(publish,'api',side_effect=api),patch.object(publish,'command') as command, \
             patch.object(publish,'verify_public') as verify:
            result=publish.publish(self.record,self.checks,Path('unused'),'product notes',self.main,10)
        self.assertTrue(result['published'])
        command.assert_not_called()
        verify.assert_called_once_with('1.0.0-alpha.9',self.checks)
        self.assertTrue(all(method=='GET' for _,method in calls))

    def test_conflicting_tag_aborts_before_release_mutation(self):
        def api(path,method='GET',payload=None,optional=False):
            self.assertEqual(method,'GET')
            if path.startswith('actions/runs/'): return self.run
            if path.startswith('git/ref/'): return {'object':{'type':'commit','sha':'c'*40}}
            raise AssertionError(path)
        with patch.object(publish,'require_main'),patch.object(publish,'wait_main_ci',return_value=99), \
             patch.object(publish,'api',side_effect=api),patch.object(publish,'command') as command:
            with self.assertRaises(ValueError):
                publish.publish(self.record,self.checks,Path('unused'),'product notes',self.main,10)
        command.assert_not_called()

    def test_failed_upload_retains_draft_and_identical_retry_finishes_without_clobber(self):
        state={'release':None,'ref':None,'fail_upload':True}
        writes=[]
        def api(path,method='GET',payload=None,optional=False):
            if method!='GET': writes.append((path,method))
            if path.startswith('actions/runs/'): return self.run
            if path.startswith('git/ref/'): return copy.deepcopy(state['ref'])
            if path=='git/refs':
                state['ref']={'object':{'type':'commit','sha':payload['sha']}}
                return state['ref']
            if path=='releases' and method=='POST':
                state['release']=dict(payload,id=1,assets=[])
                return copy.deepcopy(state['release'])
            if path.startswith('releases/tags/') or path=='releases/1':
                if method=='PATCH': state['release'].update(payload)
                return copy.deepcopy(state['release'])
            raise AssertionError(path)
        def command(args,**kwargs):
            self.assertNotIn('--clobber',args)
            self.assertEqual(args[:3],['gh','release','upload'])
            if state['fail_upload']:
                state['fail_upload']=False
                raise publish.GitHubError(503)
            state['release']['assets']=copy.deepcopy(self.assets)
            return b''
        with patch.object(publish,'require_main'),patch.object(publish,'wait_main_ci',return_value=99), \
             patch.object(publish,'api',side_effect=api),patch.object(publish,'command',side_effect=command), \
             patch.object(publish,'verify_public'):
            with self.assertRaises(publish.GitHubError):
                publish.publish(self.record,self.checks,Path('unused'),'notes',self.main,10)
            self.assertTrue(state['release']['draft'])
            self.assertIsNone(state['ref'])
            result=publish.publish(self.record,self.checks,Path('unused'),'notes',self.main,10)
        self.assertTrue(result['published'])
        self.assertEqual(writes.count(('releases','POST')),1)
        self.assertEqual(writes.count(('git/refs','POST')),1)

    def test_main_guard_refuses_runtime_changes_even_if_catalog_claims_old_source(self):
        parent=publish.ROOT/'target'; parent.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as directory:
            root=Path(directory)
            (root/'Cargo.toml').write_text('[package]\nversion="1.0.0-alpha.9"\n')
            changed=[b'releases/catalog.json\0']
            def command(args,**kwargs):
                if args==['git','rev-parse','HEAD']: return self.main.encode()
                if args==['git','diff','--binary','HEAD']: return b''
                if args[:2]==['git','merge-base']: return b''
                if args[:3]==['git','diff','--name-only']: return changed[0]
                raise AssertionError(args)
            with patch.dict(os.environ,GITHUB_ACTIONS='true',GITHUB_EVENT_NAME='workflow_dispatch',
                            GITHUB_REF='refs/heads/main',GITHUB_REPOSITORY=publish.REPOSITORY,GITHUB_SHA=self.main), \
                 patch.object(publish,'ROOT',root),patch.object(publish,'command',side_effect=command), \
                 patch.object(publish,'api',return_value={'commit':{'sha':self.main}}):
                publish.require_main(self.main,self.record)
                changed[0]=b'src/main.rs\0'
                with self.assertRaises(ValueError): publish.require_main(self.main,self.record)


if __name__=='__main__':
    unittest.main()
