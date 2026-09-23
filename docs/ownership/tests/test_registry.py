from pathlib import Path
import importlib.util
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[3]


def load():
    path = ROOT / 'docs/ownership/tools/generate_registry.py'
    spec = importlib.util.spec_from_file_location('generate_registry', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class RegistryTests(unittest.TestCase):
    def test_population_is_deterministic_and_non_approving(self):
        registry = load().build(ROOT, 'a' * 40)
        self.assertEqual(registry['population_count'], 105)
        self.assertEqual(registry['publication_count'], 0)
        self.assertEqual({p['structural'] for p in registry['packages']}, {'PASS'})
        self.assertEqual({p['artifact_integrity'] for p in registry['packages']}, {'PASS'})
        self.assertFalse(any(
            'skill: missing profile' in error
            for p in registry['packages']
            for error in p.get('errors', {}).get('integrity', [])
        ))
        self.assertEqual({p['cold_review'] for p in registry['packages']}, {'UNVERIFIED'})
        self.assertEqual({p['publication'] for p in registry['packages']}, {'NOT_PUBLISHED'})
        self.assertEqual(registry['calibration'], 'PASS')

    def test_valid_review_record_is_reflected_without_claiming_independence(self):
        module=load()
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            subprocess.run(['git','init','-q'],cwd=root,check=True)
            source=root/'source.txt'
            source.write_text('baseline\n')
            subprocess.run(['git','add','source.txt'],cwd=root,check=True)
            subprocess.run(['git','-c','user.name=Test','-c','user.email=test@example.invalid',
                            'commit','-qm','baseline'],cwd=root,check=True)
            baseline=subprocess.run(['git','rev-parse','HEAD'],cwd=root,check=True,
                                    capture_output=True,text=True).stdout.strip()
            source.write_text('current\n')
            subprocess.run(['git','add','source.txt'],cwd=root,check=True)
            subprocess.run(['git','-c','user.name=Test','-c','user.email=test@example.invalid',
                            'commit','-qm','current'],cwd=root,check=True)
            current=subprocess.run(['git','rev-parse','HEAD'],cwd=root,check=True,
                                   capture_output=True,text=True).stdout.strip()
            path=root/'docs/ownership/records/example.json'
            path.parent.mkdir(parents=True)
            record={'package':'example','manifest':'crates/example/Cargo.toml',
                'record_path':'docs/ownership/records/example.json',
                'source_commit':baseline,
                'sources':[{'path':'source.txt','sha256':hashlib.sha256(source.read_bytes()).hexdigest()}],
                'artifacts':[{'kind':kind,'verdict':'APPROVE'} for kind in module.KINDS.values()]}
            path.write_text(json.dumps(record))
            with mock.patch.object(module,'record_errors',return_value=[]):
                self.assertEqual(module.review_state(root,'example','crates/example/Cargo.toml')[0],
                                 'INVALID_REVIEW_RECORD')
                record['source_commit']=current
                path.write_text(json.dumps(record))
                state, verdicts=module.review_state(root,'example','crates/example/Cargo.toml')
            self.assertEqual(state,'REVIEW_EVIDENCE_CONSISTENT')
            self.assertEqual(set(verdicts),set(module.KINDS.values()))
            with mock.patch.object(module,'record_errors',return_value=['stale bytes']):
                self.assertEqual(module.review_state(root,'example','crates/example/Cargo.toml')[0],
                                 'INVALID_REVIEW_RECORD')

    def test_cli_default_root_and_invalid_output_refusal(self):
        module=load()
        observed=subprocess.run(['git','rev-parse','origin/main'],cwd=ROOT,check=True,
                                capture_output=True,text=True).stdout.strip()
        self.assertEqual(module.ROOT,ROOT)
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder)
            json_out=folder/'registry.json'
            md_out=folder/'index.md'
            command=[sys.executable,str(ROOT/'docs/ownership/tools/generate_registry.py'),
                     '--observed-main',observed,'--json-output',str(json_out),
                     '--markdown-output',str(md_out)]
            result=subprocess.run(command,cwd=ROOT,capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(json.loads(json_out.read_text())['population_count'],105)
            self.assertEqual(json.loads(json_out.read_text())['calibration'],'PASS')
            json_out.write_text('untouched json')
            md_out.write_text('untouched markdown')
            stale=command.copy()
            stale[stale.index('--observed-main')+1]='a'*40
            result=subprocess.run(stale,cwd=ROOT,capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            self.assertIn('observed-main is stale',result.stderr)
            self.assertEqual(json_out.read_text(),'untouched json')
            self.assertEqual(md_out.read_text(),'untouched markdown')
            empty=folder/'empty-root'
            empty.mkdir()
            result=subprocess.run(command+['--root',str(empty)],cwd=ROOT,
                                  capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            self.assertEqual(json_out.read_text(),'untouched json')
            self.assertEqual(md_out.read_text(),'untouched markdown')
            with mock.patch.object(module,'build',return_value={
                    'calibration':'STALE','calibration_errors':['stale fixture']}), \
                 mock.patch.object(sys,'argv',command[1:]):
                self.assertEqual(module.main(),1)
            self.assertEqual(json_out.read_text(),'untouched json')
            self.assertEqual(md_out.read_text(),'untouched markdown')

    def test_fetched_origin_ref_must_match_remote_main(self):
        module=load()
        outputs=(str(ROOT)+'\n', 'a'*40+'\n', 'b'*40+'\trefs/heads/main\n')
        results=[subprocess.CompletedProcess([],0,output,'') for output in outputs]
        with mock.patch.object(module.subprocess,'run',side_effect=results):
            with self.assertRaisesRegex(ValueError,'fetched origin/main is stale'):
                module.verify_observed_main(ROOT,'a'*40)

    def test_publication_requires_exact_readback(self):
        module=load()
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            (root/'docs/ownership/plans').mkdir(parents=True)
            (root/'docs/ownership/issue-drafts').mkdir(parents=True)
            (root/'evidence').mkdir()
            manifest='crates/example/Cargo.toml'
            body=f'Issue\n{module.marker(manifest)}\n'
            (root/'docs/ownership/issue-drafts/example.md').write_text(body)
            issue={'repository_id':1,'number':7,'url':'https://github.com/o/r/issues/7','body':body}
            (root/'evidence/issue.json').write_text(json.dumps(issue))
            item={'manifest':manifest,'state':'CONFIRMED','repository_id':1,'repository':'o/r',
                  'body_path':'issue-drafts/example.md','body_sha256':hashlib.sha256(body.encode()).hexdigest(),
                  'issue_number':7,'issue_url':issue['url']}
            ledger=root/'docs/ownership/plans/publication-ledger.json'
            ledger.write_text(json.dumps({'items':[item]}))
            self.assertEqual(module.publication_states(root)[manifest],'READBACK_REQUIRED')
            item['readback_path']='evidence/issue.json'
            item['readback_sha256']=hashlib.sha256((root/'evidence/issue.json').read_bytes()).hexdigest()
            ledger.write_text(json.dumps({'items':[item]}))
            self.assertEqual(module.publication_states(root)[manifest],'INVALID_PREREQUISITES')
            standard=root/'docs/ownership/STANDARD.md'
            standard.write_text('# Fixture standard\n\n**Versão:** fixture · **Estado:** frozen.\n')
            subprocess.run(['git','init','-q'],cwd=root,check=True)
            subprocess.run(['git','add','docs/ownership/STANDARD.md'],cwd=root,check=True)
            subprocess.run(['git','-c','user.name=Test','-c','user.email=test@example.invalid',
                            'commit','-qm','frozen standard'],cwd=root,check=True)
            commit=subprocess.run(['git','rev-parse','HEAD'],cwd=root,check=True,
                                  capture_output=True,text=True).stdout.strip()
            remote=root/'origin.git'
            subprocess.run(['git','init','--bare','-q',str(remote)],check=True)
            subprocess.run(['git','remote','add','origin',str(remote)],cwd=root,check=True)
            subprocess.run(['git','push','-q','origin','HEAD:main'],cwd=root,check=True)
            subprocess.run(['git','fetch','-q','origin','main'],cwd=root,check=True)
            item['contract']={'version':'fixture','frozen':True,
                              'url':f'https://github.com/o/r/blob/{commit}/docs/ownership/STANDARD.md'}
            item['gates']={gate:{'state':'PASS','evidence':['fixture']} for gate in (
                'census','context','capacity','shared-contract','deduplication','backlog')}
            item['gates']['capacity']['state']='PENDING'
            ledger.write_text(json.dumps({'items':[item]}))
            self.assertEqual(module.publication_states(root)[manifest],'INVALID_PREREQUISITES')
            item['gates']['capacity']['state']='PASS'
            item['contract']['version']='wrong'
            ledger.write_text(json.dumps({'items':[item]}))
            self.assertEqual(module.publication_states(root)[manifest],'INVALID_PREREQUISITES')
            item['contract']['version']='fixture'
            ledger.write_text(json.dumps({'items':[item]}))
            self.assertEqual(module.publication_states(root)[manifest],'PUBLISHED')
            issue['url']='https://github.com/o/r/issues/8'
            item['issue_url']=issue['url']
            (root/'evidence/issue.json').write_text(json.dumps(issue))
            item['readback_sha256']=hashlib.sha256((root/'evidence/issue.json').read_bytes()).hexdigest()
            ledger.write_text(json.dumps({'items':[item]}))
            self.assertEqual(module.publication_states(root)[manifest],'INVALID_READBACK')
            issue['body']='different'
            (root/'evidence/issue.json').write_text(json.dumps(issue))
            self.assertEqual(module.publication_states(root)[manifest],'INVALID_READBACK')

    def test_calibration_rejects_stale_current_bytes(self):
        module=load()
        self.assertEqual(module.calibration_errors(ROOT),[])
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            readback=root/module.CALIBRATION
            readback.parent.mkdir(parents=True)
            readback.write_bytes((ROOT/module.CALIBRATION).read_bytes())
            for package in module.PILOTS:
                source=ROOT/f'.claude/skills/own-{package}/SKILL.md'
                target=root/source.relative_to(ROOT)
                target.parent.mkdir(parents=True,exist_ok=True)
                shutil.copyfile(source,target)
                for name in ('REFERENCE','BLAST_RADIUS','MAINTENANCE'):
                    source=ROOT/f'docs/ownership/crates/{package}/{name}.md'
                    target=root/source.relative_to(ROOT)
                    target.parent.mkdir(parents=True,exist_ok=True)
                    shutil.copyfile(source,target)
            with (root/'docs/ownership/crates/corelink-hash/REFERENCE.md').open('a') as file:
                file.write('\nchanged\n')
            self.assertIn('corelink-hash: stale calibration metric: REFERENCE.md',
                          module.calibration_errors(root))


if __name__ == '__main__':
    unittest.main()
