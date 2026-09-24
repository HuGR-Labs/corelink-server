import copy
from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'tools'))
from prepare_census import classify_manifest,reconcile_packages,skill_slug
from render_issue import render

class PreparationTests(unittest.TestCase):
    def setUp(self):
        self.p={'package':'alpha_beta','manifest':'crates/alpha/Cargo.toml','skill_slug':skill_slug('alpha_beta'),
          'targets':[{'name':'alpha_beta','kind':['lib'],'src_path':'crates/alpha/src/lib.rs','required_features':[]}],
          'features':{},'declared_dependencies':[],'declared_workspace_consumers':[]}
        self.seed={k:['Specific synthetic fixture.'] for k in ('verified_facts','okf_context','specific_risks','initial_commands','seed_evidence')}
        self.url='https://github.com/owner/repo/blob/'+'a'*40+'/docs/ownership/STANDARD.md'
    def test_renderer_consistent_slug(self):
        text=render(self.p,self.seed,baseline='a'*40,contract_url=self.url,expected_repository='owner/repo')
        self.assertIn('.claude/skills/own-alpha-beta/SKILL.md',text)
        self.assertIn('crates/alpha_beta/REFERENCE.md',text)
        self.assertNotIn('own-alpha_beta',text)
    def test_renderer_never_self_marks_ready(self):
        self.assertIn('DRAFT_VALIDATED_STRUCTURE',render(self.p,self.seed,baseline='a'*40,contract_url=self.url,expected_repository='owner/repo'))
    def test_missing_seed_rejected(self):
        self.seed['specific_risks']=[]
        with self.assertRaises(ValueError):render(self.p,self.seed,baseline='a'*40,contract_url=self.url,expected_repository='owner/repo')
    def test_mutable_contract_rejected(self):
        with self.assertRaises(ValueError):render(self.p,self.seed,baseline='a'*40,contract_url=self.url.replace('a'*40,'main'),expected_repository='owner/repo')
    def test_contract_repository_mismatch_rejected(self):
        with self.assertRaises(ValueError):render(self.p,self.seed,baseline='a'*40,contract_url=self.url,expected_repository='attacker/other')
    def test_missing_repository_rejected(self):
        with self.assertRaises(TypeError):render(self.p,self.seed,baseline='a'*40,contract_url=self.url)
    def test_placeholder_seed_rejected(self):
        self.seed['verified_facts']=['{{UNKNOWN}}']
        with self.assertRaises(ValueError):render(self.p,self.seed,baseline='a'*40,contract_url=self.url,expected_repository='owner/repo')
    def test_unknown_targets_rejected(self):
        self.p['targets']=[]
        with self.assertRaises(ValueError):render(self.p,self.seed,baseline='a'*40,contract_url=self.url,expected_repository='owner/repo')
    def test_issue_capacity_not_truncated(self):
        self.seed['verified_facts']=['long '*5000]
        with self.assertRaisesRegex(ValueError,'CAPACITY'):render(self.p,self.seed,baseline='a'*40,contract_url=self.url,expected_repository='owner/repo')
    def test_vendor_not_auto_excluded_by_path(self):
        self.assertEqual(classify_manifest('vendor/x/Cargo.toml',{'package':{'name':'x'}},None),'UNCLASSIFIED')
    def test_fuzz_not_auto_included_by_path(self):
        self.assertEqual(classify_manifest('crates/x/fuzz/Cargo.toml',{'package':{'name':'x-fuzz'}},None),'UNCLASSIFIED')
    def test_virtual_workspace_not_a_package(self):
        self.assertEqual(classify_manifest('Cargo.toml',{'workspace':{}},None),'workspace_only_no_package')
    def test_explicit_decision(self):
        self.assertEqual(classify_manifest('x/Cargo.toml',{'package':{'name':'x'}},{'classification':'first_party_independent','reason':'Inspected owner manifest','evidence':'source fixture'}),'first_party_independent')

    def test_explicit_archive_decision(self):
        self.assertEqual(classify_manifest('_archive/old/Cargo.toml',{'package':{'name':'old'}},{'classification':'archive','reason':'Historical tracked snapshot outside workspace','evidence':'git path and manifest inspection'}),'archive')
    def test_decision_without_evidence_rejected(self):
        with self.assertRaises(ValueError):classify_manifest('x/Cargo.toml',{'package':{}},{'classification':'third_party_vendor'})
    def test_duplicate_combined_population_rejected(self):
        with self.assertRaises(ValueError):reconcile_packages([[self.p],[copy.deepcopy(self.p)]])

if __name__=='__main__':unittest.main()
