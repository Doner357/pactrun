import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

from tools import check_docs_browser as docs


class BrowserDriverTests(unittest.TestCase):
    def setUp(self):
        (docs.ROOT/'target').mkdir(exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(prefix='browser-driver-test-',dir=docs.ROOT/'target')
        self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name)

    def test_reader_checks_use_development_edition_and_no_browser_build(self):
        with patch.object(docs,'ROOT',self.root),patch.object(docs.subprocess,'run') as run:
            docs.checks('https://example.invalid/pactrun//',self.root/'out')
        commands=[call.args[0] for call in run.call_args_list]
        self.assertEqual(len(commands),3)
        self.assertEqual(commands[0][2],'https://example.invalid/pactrun/')
        self.assertEqual(commands[2][2],'https://example.invalid/pactrun/next/')
        self.assertTrue(all(command[0]=='node' for command in commands))

    def test_owned_preview_is_stopped_when_a_check_fails(self):
        process=Mock(); process.poll.return_value=None
        response=Mock(); response.__enter__=Mock(return_value=Mock(status=200)); response.__exit__=Mock(return_value=False)
        output=self.root/'evidence'
        with patch.dict(os.environ,PLAYWRIGHT_PACKAGE='fixture',DOCUSAURUS_BASE_URL='/pactrun/'), \
             patch('sys.argv',['check_docs_browser','--output',str(output)]), \
             patch.object(docs.subprocess,'Popen',return_value=process) as popen, \
             patch.object(docs.urllib.request,'urlopen',return_value=response), \
             patch.object(docs,'checks',side_effect=RuntimeError('fixture failure')):
            with self.assertRaisesRegex(RuntimeError,'fixture failure'): docs.main()
        process.terminate.assert_called_once()
        process.wait.assert_called_once_with(timeout=15)
        self.assertNotIn('--build',popen.call_args.args[0])
        self.assertIn('127.0.0.1',popen.call_args.args[0])

    def test_public_checks_do_not_start_a_local_preview(self):
        with patch.dict(os.environ,PLAYWRIGHT_PACKAGE='fixture'), \
             patch('sys.argv',['check_docs_browser','--output',str(self.root/'public'),'--url','https://example.invalid/']), \
             patch.object(docs,'checks') as checks,patch.object(docs.subprocess,'Popen') as popen:
            docs.main()
        checks.assert_called_once()
        popen.assert_not_called()


if __name__=='__main__': unittest.main()
