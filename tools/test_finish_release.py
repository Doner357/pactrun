import os
import unittest
from unittest.mock import patch
from tools import finish_release as closeout


class CloseoutTests(unittest.TestCase):
    def test_actions_token_does_not_gain_pr_authority(self):
        with patch.dict(os.environ,GITHUB_ACTIONS='true'),patch.object(closeout,'api') as api:
            with self.assertRaises(ValueError): closeout.finish('1.0.0-alpha.9','a'*40,10,True)
        api.assert_not_called()

    def test_failed_public_verification_cannot_start_back_merge(self):
        with patch.dict(os.environ,GITHUB_ACTIONS='false'),patch.object(closeout,'api',return_value={}) as api:
            with self.assertRaises(ValueError): closeout.finish('1.0.0-alpha.9','a'*40,10,True)
        self.assertEqual(api.call_count,1)

    def test_already_integrated_release_is_read_only(self):
        main='a'*40
        def api(path,*args,**kwargs):
            self.assertFalse(args)
            if path.startswith('actions/runs/'):
                return dict(path='.github/workflows/publish-release.yml',head_sha=main,event='workflow_dispatch',status='completed',conclusion='success')
            if path.startswith('branches/'): return {'commit':{'sha':main}}
            if path.startswith('releases/tags/'): return {'draft':False}
            if path.startswith('git/ref/'): return {'object':{'type':'commit','sha':main}}
            if path.startswith('compare/'): return {'status':'identical'}
            raise AssertionError(path)
        with patch.dict(os.environ,GITHUB_ACTIONS='false'),patch.object(closeout,'api',side_effect=api), \
             patch.object(closeout,'command') as command:
            self.assertTrue(closeout.finish('1.0.0-alpha.9',main,10,True)['already_contained'])
        command.assert_not_called()


if __name__=='__main__': unittest.main()
