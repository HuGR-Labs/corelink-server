from __future__ import annotations
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
def load(name: str):
    spec=importlib.util.spec_from_file_location(name,ROOT/'tools'/f'{name}.py')
    mod=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod
check=load('check_docs'); census=load('prepare_census')

class DocumentChecks(unittest.TestCase):
    def text(self,kind='skill'):
        filename={'skill':'SKILL','reference':'REFERENCE','blast_radius':'BLAST_RADIUS','maintenance':'MAINTENANCE'}[kind]
        return (ROOT/'templates'/f'{filename}.md.tmpl').read_text()
    def result(self,text,kind='skill'):
        return check.validate(text,kind,'S',template=True)
    def test_all_templates_fit_and_navigate(self):
        for kind in check.BUDGETS:
            with self.subTest(kind=kind):
                self.assertEqual(self.result(self.text(kind),kind)['errors'],[])
    def test_words_include_metadata(self):
        self.assertEqual(check.metrics('a b\nc')['words'],3)
    def test_utf8_bytes_not_character_count(self):
        self.assertEqual(check.metrics('á')['bytes'],2)
    def test_line_budget_rejected(self):
        self.assertTrue(any('lines:' in e for e in self.result(self.text()+'\n'*200)['errors']))
    def test_word_budget_rejected(self):
        self.assertTrue(any('words:' in e for e in self.result(self.text()+'\n'+'x '*1300)['errors']))
    def test_byte_budget_rejected(self):
        self.assertTrue(any('bytes:' in e for e in self.result(self.text()+'\n'+'á'*7000)['errors']))
    def test_unresolved_template_not_approved_as_document(self):
        self.assertIn('unresolved placeholder/TODO/TBD',check.validate(self.text(),'skill','S')['errors'])
    def test_duplicate_anchor_rejected(self):
        self.assertIn('duplicate explicit anchor',self.result(self.text()+'\n<a id="s01"></a>')['errors'])
    def test_missing_required_section(self):
        self.assertIn('missing section anchor: s01',self.result(self.text().replace('<a id="s01"></a>',''))['errors'])
    def test_broken_anchor_rejected(self):
        self.assertIn('broken internal anchor: #not-here',self.result(self.text()+'\n[x](#not-here)')['errors'])
    def test_deep_heading_rejected(self):
        self.assertIn('heading deeper than H3',self.result(self.text()+'\n#### deeper')['errors'])
    def test_long_code_block_rejected(self):
        self.assertTrue(any('code block exceeds' in e for e in self.result(self.text()+'\n```\n'+'a\n'*26+'```\n')['errors']))
    def test_unclosed_code_fence_rejected(self):
        self.assertIn('unclosed code fence',self.result(self.text()+'\n```sh\nx\n')['errors'])
    def test_wide_table_rejected(self):
        self.assertTrue(any('table exceeds' in e for e in self.result(self.text()+'\n|a|b|c|d|e|f|g|\n')['errors']))
    def test_long_prose_rejected(self):
        self.assertIn('prose paragraph exceeds 80 words',self.result(self.text()+'\n'+'word '*81+'\n')['errors'])
    def test_relation_card_limit(self):
        t=self.text('blast_radius').replace('**Contrato:**','**Contrato:** '+'word '*130)
        self.assertTrue(any('REL-001 exceeds 120 words'==e for e in self.result(t,'blast_radius')['errors']))
    def test_procedure_steps_limit(self):
        t=self.text('maintenance').replace('**Falhas e parada:**','\n'.join(f'{i}. passo' for i in range(3,10))+'\n\n**Falhas e parada:**')
        self.assertIn('PROC-001 exceeds eight numbered steps',self.result(t,'maintenance')['errors'])
    def test_external_links_not_claimed_verified(self):
        r=self.result(self.text()+'\n[x](https://example.invalid/not-fetched)')
        self.assertIn('external links',r['not_certified'])
    def test_no_cold_review_claim(self):
        self.assertIn('cold review',self.result(self.text())['not_certified'])
    def test_missing_frontmatter(self):
        self.assertIn('missing frontmatter',self.result(self.text().replace('---\n','',1))['errors'])

class CensusChecks(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(); self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name).resolve()
        self.data={'version':1,'workspace_root':str(self.root),'workspace_members':['id-a','id-b'],'packages':[]}
        for ident,name,folder in [('id-a','corelink-server','crates/corelink-container'),('id-b','corelink-x','crates/x')]:
            p=self.root/folder;p.mkdir(parents=True)
            (p/'Cargo.toml').write_text(f'[package]\nname = "{name}"\nversion = "0.1.0"\n')
            self.data['packages'].append({'id':ident,'name':name,'manifest_path':str(p/'Cargo.toml'),
             'targets':[{'name':name,'kind':['lib'],'src_path':str(p/'src/lib.rs')}],'dependencies':[],
             'features':{},'license':None,'publish':[]})
    def test_package_identity_not_folder(self):
        result=census.normalize_metadata(self.data,self.root)
        self.assertEqual(result[0]['package'],'corelink-server')
        self.assertEqual(result[0]['manifest'],'crates/corelink-container/Cargo.toml')
    def test_empty_members_rejected(self):
        self.data['workspace_members']=[]
        with self.assertRaises(ValueError):census.normalize_metadata(self.data,self.root)
    def test_missing_package_rejected(self):
        self.data['packages'].pop()
        with self.assertRaises(ValueError):census.normalize_metadata(self.data,self.root)
    def test_duplicate_member_rejected(self):
        self.data['workspace_members'].append('id-a')
        with self.assertRaises(ValueError):census.normalize_metadata(self.data,self.root)
    def test_wrong_root_rejected(self):
        self.data['workspace_root']=str(self.root/'wrong')
        with self.assertRaises(ValueError):census.normalize_metadata(self.data,self.root)
    def test_wrong_name_rejected(self):
        self.data['packages'][0]['name']='folder-is-not-name'
        with self.assertRaises(ValueError):census.normalize_metadata(self.data,self.root)
    def test_missing_targets_rejected(self):
        self.data['packages'][0]['targets']=[]
        with self.assertRaises(ValueError):census.normalize_metadata(self.data,self.root)
    def test_path_escape_rejected(self):
        self.data['packages'][0]['manifest_path']=str(self.root.parent/'other'/'Cargo.toml')
        with self.assertRaises(ValueError):census.normalize_metadata(self.data,self.root)
    def test_reverse_edges_preserve_kinds_conditions_and_alias(self):
        self.data['packages'][0]['dependencies']=[{'name':'corelink-x','path':str(self.root/'crates/x'),
         'rename':'x_alias','kind':'dev','target':'cfg(unix)','optional':True,'features':['x'] }]
        result=census.normalize_metadata(self.data,self.root)
        dep=result[1]['declared_workspace_consumers'][0]
        self.assertEqual(dep['kind'],'dev');self.assertEqual(dep['alias'],'x_alias')
        self.assertEqual(dep['target_cfg'],'cfg(unix)');self.assertTrue(dep['optional'])
        self.assertFalse(result[1]['runtime_verified'])
    def test_slug_bounded(self):
        self.assertLessEqual(len(census.skill_slug('verylong-'*12)),64)
    def test_slug_collision_rejected(self):
        self.data['packages'][0]['name']='a_b'; self.data['packages'][1]['name']='a-b'
        for p in self.data['packages']:
            Path(p['manifest_path']).write_text(f'[package]\nname="{p["name"]}"\n')
        with self.assertRaises(ValueError):census.normalize_metadata(self.data,self.root)
    def test_opaque_package_ids_not_parsed(self):
        self.data['workspace_members'][0]='opaque-id:with:future:syntax'
        self.data['packages'][0]['id']=self.data['workspace_members'][0]
        self.assertEqual(len(census.normalize_metadata(self.data,self.root)),2)

if __name__=='__main__':unittest.main()
