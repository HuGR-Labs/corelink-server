import copy
import hashlib
import subprocess
import sys
import tempfile
from pathlib import Path
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'tools'))
import publication_gate as pub

class PublicationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.contract_root = Path(cls.temp.name)
        standard = cls.contract_root / pub.STANDARD_PATH
        standard.parent.mkdir(parents=True)
        standard.write_text('# Fixture standard\n\n**Versão:** fixture · **Estado:** frozen.\n')
        subprocess.run(['git', 'init', '-q'], cwd=cls.contract_root, check=True)
        subprocess.run(['git', 'add', pub.STANDARD_PATH], cwd=cls.contract_root, check=True)
        subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                        'commit', '-qm', 'frozen standard'], cwd=cls.contract_root, check=True)
        cls.contract_commit = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=cls.contract_root,
                                             check=True, capture_output=True, text=True).stdout.strip()
        remote = cls.contract_root / 'origin.git'
        subprocess.run(['git', 'init', '--bare', '-q', str(remote)], check=True)
        subprocess.run(['git', 'remote', 'add', 'origin', str(remote)], cwd=cls.contract_root, check=True)
        subprocess.run(['git', 'push', '-q', 'origin', 'HEAD:main'], cwd=cls.contract_root, check=True)
        subprocess.run(['git', 'fetch', '-q', 'origin', 'main'], cwd=cls.contract_root, check=True)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def setUp(self):
        self.body=pub.marker('crates/x/Cargo.toml')+'\nIssue fixture.'
        self.item={'repository_id':1,'repository':'owner/repo','manifest':'crates/x/Cargo.toml','source_commit':'a'*40,
          'state':'PENDING','body_sha256':hashlib.sha256(self.body.encode()).hexdigest(),
          'contract':{'version':'fixture','frozen':True,'url':f'https://github.com/owner/repo/blob/{self.contract_commit}/{pub.STANDARD_PATH}'},
          'gates':{k:{'state':'PASS','evidence':'fixture-evidence'} for k in ('census','context','capacity','shared-contract','deduplication','backlog')}}
        self.snapshot={'repository_id':1,'scope':'open-and-closed','complete':True,'issues':[]}
    def decide(self):return pub.publication_decision(self.item,self.snapshot,expected_repo_id=1,expected_repository='owner/repo',contract_root=self.contract_root)
    def issue(self):return {'repository_id':1,'body':self.body,'number':1,'url':'https://github.com/owner/repo/issues/1','state':'open'}
    def test_control(self):self.assertEqual(self.decide()['action'],'ELIGIBLE_FOR_SERIAL_CREATE')
    def test_missing_precondition(self):
        self.item['gates']['census']['state']='BLOCKED';self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_partial_snapshot(self):
        self.snapshot['complete']=False;self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_closed_duplicate_reused_not_recreated(self):
        issue=self.issue();issue['state']='closed';self.snapshot['issues']=[issue]
        self.assertEqual(self.decide()['action'],'REUSE_OR_RECONCILE')
    def test_matched_candidate_number_must_match_exact_issue_url(self):
        for number, url in ((7,'https://github.com/owner/repo/issues/8'),
                            (7,'https://github.com/owner/repo/pull/7'),
                            (True,'https://github.com/owner/repo/issues/True')):
            with self.subTest(number=number,url=url):
                self.snapshot['issues']=[{**self.issue(),'number':number,'url':url}]
                self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_timeout_no_blind_retry(self):
        self.item['state']='UNCERTAIN';self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_timeout_existing_found(self):
        self.item['state']='UNCERTAIN';self.snapshot['issues']=[self.issue()]
        self.assertEqual(self.decide()['action'],'REUSE_OR_RECONCILE')
    def test_duplicate_matches_block(self):
        a=self.issue();b=self.issue();b['number']=2;self.snapshot['issues']=[a,b]
        self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_mutable_contract_link_blocked(self):
        self.item['contract']['url']='https://github.com/owner/repo/blob/main/contract.md'
        self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_nonexistent_contract_commit_blocked(self):
        self.item['contract']['url']=self.item['contract']['url'].replace(self.contract_commit, 'f'*40)
        self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_local_unpublished_contract_commit_blocked(self):
        tree=subprocess.run(['git', 'rev-parse', 'HEAD^{tree}'], cwd=self.contract_root,
                            check=True, capture_output=True, text=True).stdout.strip()
        commit=subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                               'commit-tree', tree, '-p', self.contract_commit], cwd=self.contract_root,
                              input='unpublished standard\n', check=True, capture_output=True,
                              text=True).stdout.strip()
        self.item['contract']['url']=self.item['contract']['url'].replace(self.contract_commit, commit)
        self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_wrong_contract_path_blocked(self):
        self.item['contract']['url']=self.item['contract']['url'].replace(pub.STANDARD_PATH, 'DOES-NOT-EXIST.md')
        self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_wrong_contract_version_blocked(self):
        self.item['contract']['version']='not-the-candidate'
        self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_uncommitted_standard_bytes_blocked(self):
        standard=self.contract_root/pub.STANDARD_PATH
        try:
            standard.write_text('Changed after freeze.\n')
            self.assertEqual(self.decide()['action'],'BLOCKED')
        finally:
            standard.write_text('# Fixture standard\n\n**Versão:** fixture · **Estado:** frozen.\n')
    def test_wrong_repository(self):
        self.snapshot['repository_id']=2;self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_missing_canonical_repository_is_blocked(self):
        self.item.pop('repository')
        self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_marker_from_other_repository_is_blocked(self):
        issue=self.issue(); issue['repository_id']=2; self.snapshot['issues']=[issue]
        self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_contract_from_other_repository_is_blocked(self):
        self.item['contract']['url']=self.item['contract']['url'].replace('owner/repo','attacker/other')
        self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_exact_readback(self):
        self.assertEqual(pub.record_readback(self.item,self.issue())['state'],'CONFIRMED')
    def test_readback_number_must_match_url(self):
        with self.assertRaises(ValueError):
            pub.record_readback(self.item,{**self.issue(),'number':7,
                'url':'https://github.com/owner/repo/issues/8'})
    def test_altered_readback_refused(self):
        issue=self.issue();issue['body']+='Changed'
        with self.assertRaises(ValueError):pub.record_readback(self.item,issue)
    def test_readback_url_from_other_repository_refused(self):
        with self.assertRaises(ValueError):
            pub.record_readback(self.item,{**self.issue(),'url':'https://attacker/other/issues/1'})
    def test_pull_request_url_readback_refused(self):
        with self.assertRaises(ValueError):
            pub.record_readback(self.item,{**self.issue(),'url':'https://github.com/owner/repo/pulls/1'})
    def test_pull_request_cannot_be_reused_as_issue(self):
        issue=self.issue();issue['pull_request']={'url':'fixture'};self.snapshot['issues']=[issue]
        self.assertEqual(self.decide()['action'],'BLOCKED')
    def test_upgrade_preserves_marker(self):self.assertIn('ownership:v1:',pub.marker(self.item['manifest']))
    def test_renamed_manifest_old_marker_reused(self):
        self.item['manifest']='crates/y/Cargo.toml';self.item['former_manifests']=['crates/x/Cargo.toml']
        self.snapshot['issues']=[self.issue()];self.assertEqual(self.decide()['action'],'REUSE_OR_RECONCILE')

if __name__=='__main__':unittest.main()
