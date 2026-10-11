"""Offline checks for the native-manager rehearsal; no manager or PATH mutation."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sqlite3
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = '0'*29 + '777'
DIGEST = 'sha256:' + 'b'*64


def response(command, result, version='1.0-alpha.1'):
    return json.dumps(dict(format='pactrun.cli', format_version=version, command=command,
                           status='success', result=result, error=None))


def release(version, source='1.0-alpha.1', machine='1.0-alpha.1', persistence='1.0-alpha.1'):
    defaults = dict(pack_source=source, cli_machine_interface=machine,
                    hook_protocol='1.0-alpha.1', persistence=persistence)
    return dict(product_version=version, default_formats=defaults,
                supported_formats={key:[value] for key,value in defaults.items()}, artifacts=[])


def load_tool():
    spec = importlib.util.spec_from_file_location('native_acceptance_under_test', ROOT/'tools/native_package_acceptance.py')
    module = importlib.util.module_from_spec(spec)
    with patch.object(argparse.ArgumentParser, 'parse_args', side_effect=AssertionError('import parsed CLI arguments')), \
         patch.object(subprocess, 'Popen', side_effect=AssertionError('import launched process')):
        spec.loader.exec_module(module)
    return module


class NativePackageAcceptance(unittest.TestCase):
    def setUp(self):
        self.tool = load_tool()
        parent = ROOT/'target/native-acceptance-unit'
        parent.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=parent)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root/'evidence').mkdir()

    def runtime(self, **extra):
        values = dict(ROOT=self.root, ENV=dict(os.environ, TEMP=str(self.root), TMP=str(self.root)),
                      COMMANDS=[], RESULTS=[], WIN=os.name=='nt',
                      FLAGS={'creationflags':subprocess.CREATE_NO_WINDOW} if os.name=='nt' else {})
        values.update(extra)
        return patch.multiple(self.tool, create=True, **values)

    def test_import_has_no_execution_side_effects(self):
        module = load_tool()
        self.assertTrue(callable(module.main))
        self.assertNotIn('ROOT', vars(module))
        self.assertNotIn('SERVER', vars(module))

    def test_install_reference_uses_machine_format_not_human_text_or_product_version(self):
        for fmt, expected in [('1.0-alpha.1', 'exact:'+PACKAGE+'/'+DIGEST),
                              ('1.0-alpha.2', PACKAGE+':'+DIGEST[7:]),
                              ('1.0-alpha.3', PACKAGE+':'+DIGEST[7:])]:
            with self.subTest(format=fmt):
                result = dict(revision=dict(package_id=PACKAGE, content_digest=DIGEST),
                              reference='a-name:another-name', newly_installed=True)
                self.assertEqual(self.tool.installed_reference(response('pack install',result,fmt)),expected)

    def test_install_reference_refuses_invalid_envelopes_and_identities(self):
        valid = json.loads(response('pack install',dict(revision=dict(package_id=PACKAGE,content_digest=DIGEST))))
        cases = ['Pack installed.\n'+json.dumps(valid), '{}', '[]', 'not-json']
        for key,value in [('format','other'),('format_version','1.0-alpha.99'),('command','revision show'),
                          ('status','error'),('error',{'message':'failed'}),('result',None)]:
            cases.append(json.dumps(dict(valid, **{key:value})))
        for revision in [None, {}, {'package_id':PACKAGE}, {'package_id':PACKAGE.upper()+'x','content_digest':DIGEST},
                         {'package_id':PACKAGE,'content_digest':DIGEST[7:]},
                         {'package_id':PACKAGE,'content_digest':'sha256:'+'b'*63},
                         {'package_id':PACKAGE,'content_digest':'sha256:'+'B'*64}]:
            cases.append(json.dumps(dict(valid,result={'revision':revision})))
        for value in cases:
            with self.subTest(value=value), self.assertRaises(ValueError):
                self.tool.installed_reference(value)

    def test_fixture_format_follows_explicit_common_support(self):
        old = [release('baseline'), release('candidate')]
        self.assertEqual(self.tool.fixture_source_format(old,['baseline','candidate']),'1.0-alpha.1')
        current = [release('baseline','1.0-alpha.2','1.0-alpha.2','1.0-alpha.4'),
                   release('candidate','1.0-alpha.2','1.0-alpha.3','1.0-alpha.4')]
        self.assertEqual(self.tool.fixture_source_format(current,['baseline','candidate']),'1.0-alpha.2')
        current[1]['supported_formats']['pack_source'].append('1.0-alpha.1')
        current[0]['supported_formats']['pack_source'].append('1.0-alpha.1')
        self.assertEqual(self.tool.fixture_source_format(current,['baseline','candidate']),'1.0-alpha.2')
        self.assertEqual(self.tool.fixture_source_format([old[0],current[1]],['baseline']),'1.0-alpha.1')

    def test_release_matrix_rejects_no_common_source_changed_store_and_unknown_cli(self):
        for candidate, message in [
            (release('candidate','1.0-alpha.2'), 'common Pack source'),
            (release('candidate',persistence='1.0-alpha.4'), 'Persistence'),
            (release('candidate',machine='1.0-alpha.99'), 'machine interface'),
        ]:
            with self.subTest(message=message), self.assertRaisesRegex(ValueError,message):
                self.tool.fixture_source_format([release('baseline'),candidate],['baseline','candidate'])

    def test_published_default_matrix_and_fixture_hook_support(self):
        records=json.loads((ROOT/'releases/catalog.json').read_text(encoding='utf-8'))
        self.assertEqual(self.tool.fixture_source_format(records,['1.0.0-alpha.1','1.0.0-alpha.2']),'1.0-alpha.1')
        record=release('baseline')
        record['default_formats']['hook_protocol']='1.0-alpha.99'
        record['supported_formats']['hook_protocol']=['1.0-alpha.99']
        with self.assertRaisesRegex(ValueError,'Hook protocol'):
            self.tool.fixture_source_format([record],['baseline'])

    def test_store_upgrade_is_explicit_and_only_accepts_the_implemented_transition(self):
        records=[release('baseline'),release('candidate','1.0-alpha.2','1.0-alpha.3','1.0-alpha.4')]
        self.assertEqual(self.tool.upgrade_fixture_source_format(records,['baseline','candidate']),'1.0-alpha.1')
        with self.assertRaisesRegex(ValueError,'common Pack source'):
            self.tool.fixture_source_format(records,['baseline','candidate'])
        with self.assertRaisesRegex(ValueError,'Persistence'):
            self.tool.upgrade_fixture_source_format(records,['candidate','baseline'])
        with self.assertRaisesRegex(ValueError,'Persistence'):
            self.tool.upgrade_fixture_source_format([release('baseline'),release('candidate')],['baseline','candidate'])

    def test_upgrade_checks_reject_identity_changes_and_old_reader_success(self):
        data=self.root/'data'
        (data/'pactrun/database').mkdir(parents=True)
        (data/'pack').mkdir()
        with sqlite3.connect(data/'pactrun/database/pactrun.sqlite3') as db:
            db.execute('CREATE TABLE pactrun_metadata (format_version TEXT)')
            db.execute("INSERT INTO pactrun_metadata VALUES ('1.0-alpha.4')")
        db.close()
        before=[['instance'],['revision'],['old-header']]
        after=[['instance'],['revision'],['new-header']]
        records={'baseline':release('baseline'),'candidate':release('candidate','1.0-alpha.2','1.0-alpha.3','1.0-alpha.4')}
        for defect in ['identity','content','old-reader-success','refusal-mutation']:
            (data/'pack/pactrun.yaml').write_text("source_format: '1.0-alpha.1'\n")
            changed=[['changed'],after[1],after[2]] if defect=='identity' else after
            state=Mock(side_effect=[before,changed,changed])
            rejected=False
            def run(cmd,label,**kwargs):
                nonlocal rejected
                if label=='old-binary-refuses-upgraded-store':
                    rejected=True
                    return (0 if defect=='old-reader-success' else 1),json.dumps({'status':'failure','error':{'kind':'operation'}})
                return 0,''
            def dump(flavor='normal'):
                return 'test' if flavor=='test' else ('changed' if rejected and defect=='refusal-mutation' else 'unchanged')
            with self.subTest(defect=defect), self.runtime(DATA=data,RELEASES=records,VERSIONS=['baseline','candidate'],SOURCE_FORMAT='1.0-alpha.1'), \
                 patch.object(self.tool,'immutable_state',state), \
                 patch.object(self.tool,'runtime_bytes',return_value={'blob':'changed' if defect=='content' else 'hash'}), \
                 patch.object(self.tool,'store_dump',side_effect=dump), \
                 patch.object(self.tool,'run',side_effect=run), \
                 patch.object(self.tool,'binary',return_value=self.root/'fake'), \
                 self.assertRaises(AssertionError):
                self.tool.upgraded_store_scenarios(before,{'blob':'hash'},'test')

    def test_observed_uses_separate_json_output(self):
        with patch.object(self.tool,'binary',return_value='pactrun'), \
             patch.object(self.tool,'run',return_value=(0,response('version',{'product_version':'1.0.0-alpha.1'}))) as run:
            self.assertEqual(self.tool.observed(),'1.0.0-alpha.1')
            run.assert_called_once_with(['pactrun','--format','json','--version'],'version-normal',json_output=True)

    def test_matrix_requires_consistent_records_and_advertised_defaults(self):
        a=release('baseline'); b=release('candidate')
        conflict=release('baseline','1.0-alpha.2')
        with self.assertRaisesRegex(ValueError,'inconsistent'):
            self.tool.fixture_source_format([a,conflict,b],['baseline','candidate'])
        b['supported_formats']['pack_source']=[]
        with self.assertRaises(ValueError):
            self.tool.fixture_source_format([a,b],['baseline','candidate'])
        with self.assertRaises(ValueError):
            self.tool.fixture_source_format([a],['baseline','absent'])

    def test_json_command_separates_stderr_and_preserves_exit_status(self):
        value=response('pack install',dict(revision=dict(package_id=PACKAGE,content_digest=DIGEST)))
        script='import sys; sys.stdout.write('+repr(value)+'); sys.stderr.write("manager warning\\n")'
        with self.runtime():
            code, output=self.tool.run([sys.executable,'-c',script],'json-install',json_output=True)
            self.assertEqual(code,0)
            self.assertEqual(self.tool.installed_reference(output),'exact:'+PACKAGE+'/'+DIGEST)
            self.assertIn('manager warning',(self.root/'evidence'/'000-json-install.stderr.log').read_text())
            with self.assertRaises(RuntimeError):
                self.tool.run([sys.executable,'-c',script+'; sys.exit(7)'],'failed-json',json_output=True)
            code,output=self.tool.run([sys.executable,'-c',script+'; sys.exit(7)'],'optional-json',json_output=True,required=False)
            self.assertEqual(code,7)
            self.assertEqual(output,value)

    def test_timeout_preserves_both_output_streams(self):
        process=Mock(returncode=-1)
        process.communicate.side_effect=[subprocess.TimeoutExpired('fixture',1),(b'partial',b'diagnostic')]
        with self.runtime(WIN=True), patch.object(self.tool.subprocess,'Popen',return_value=process), \
             self.assertRaisesRegex(RuntimeError,'timeout'):
            self.tool.run(['fixture'],'timeout',timeout=1,json_output=True)
        process.kill.assert_called_once()
        self.assertEqual((self.root/'evidence/000-timeout.log').read_text(),'partial')
        self.assertEqual((self.root/'evidence/000-timeout.stderr.log').read_text(),'diagnostic')

    def test_prepare_data_uses_selected_source_and_each_binarys_install_envelope(self):
        data=self.root/'data'; data.mkdir(); barrier=data/'barrier'
        formats={'normal':'1.0-alpha.3','test':'1.0-alpha.1'}
        created=[]
        def fake_run(cmd,label,**kwargs):
            args=list(map(str,cmd)); flavor=Path(args[0]).name
            if args[1:5]==['--format','json','pack','install']:
                self.assertTrue(kwargs['json_output'])
                self.assertIn("source_format: '1.0-alpha.1'",(data/'pack/pactrun.yaml').read_text())
                return 0,response('pack install',dict(revision=dict(package_id=PACKAGE,content_digest=DIGEST)),formats[flavor])
            if args[1:3]==['pack','install']:
                self.fail('prepare_data parsed human installation output')
            if args[1:3]==['instance','create']:
                expected=PACKAGE+':'+DIGEST[7:] if flavor=='normal' else 'exact:'+PACKAGE+'/'+DIGEST
                self.assertEqual(args[-1],expected); created.append(flavor)
            if label=='test-root-isolation': return 1,'missing'
            if label=='hook-control': (barrier/'helper-code').write_text('0')
            return 0,''
        with self.runtime(DATA=data,BARRIER=barrier,SOURCE_FORMAT='1.0-alpha.1'), \
             patch.object(self.tool,'run',side_effect=fake_run), \
             patch.object(self.tool,'binary',side_effect=lambda flavor='normal':self.root/flavor):
            self.tool.prepare_data(['test'])
        self.assertEqual(created,['normal','test'])
        self.assertFalse((data/'pactrun').exists(), 'current binaries must prepare their own complete Store')
        self.assertTrue((data/'pactrun-test/database').is_dir())

    def test_incompatible_matrix_is_rejected_before_creating_root_or_server(self):
        records=[release('1.0.0-alpha.4'), release('1.0.0-alpha.4'),
                 release('1.0.0-alpha.5','1.0-alpha.2','1.0-alpha.3','1.0-alpha.4'),
                 release('1.0.0-alpha.5','1.0-alpha.2','1.0-alpha.3','1.0-alpha.4')]
        paths=[]
        for i,record in enumerate(records):
            p=self.root/(str(i)+'.json'); p.write_text(json.dumps(record)); paths.append(str(p))
        target=self.root/'not-created'
        argv=['--root',str(target),'--manager',str(self.root/'manager'),'--xtask',str(self.root/'xtask'),
              '--parts',*paths,'--versions','1.0.0-alpha.4','1.0.0-alpha.5','--allow-user-path-change']
        with patch.object(self.tool.http.server,'ThreadingHTTPServer',side_effect=AssertionError('server started')), \
             self.assertRaisesRegex(ValueError,'common Pack source'):
            self.tool.main(argv)
        self.assertFalse(target.exists())


if __name__ == '__main__':
    unittest.main()
