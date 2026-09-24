from __future__ import annotations
import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tools'))
import check_docs as checker
import ownership_gate as gate
from prepare_census import skill_slug


def doc(kind, package='example', manifest='crates/example/Cargo.toml',record_id='example-set'):
    p,n=checker.SECTIONS[kind]
    if kind=='skill':
        fm=f'''---
name: {skill_slug(package)}
description: Ownership de {package} para mudanças na sua API; não autoriza produção.
metadata:
  schema: "corelink-ownership/1.1"
  package: "{package}"
  manifest: "{manifest}"
  source-commit: "{'a'*40}"
  evidence-set: "{record_id}"
---
'''
    else:
        fm=f'''---
schema: corelink-ownership/1.1
document: {kind}
package: {package}
manifest: {manifest}
source_commit: {'a'*40}
profile: S
state: draft
evidence_set: {record_id}
---
'''
    return fm+'\n# Example\n\n'+' · '.join(f'[S{i}](#{p}{i:02d})' for i in range(1,n+1))+'\n\n'+''.join(
        f'<a id="{p}{i:02d}"></a>\n## {p.upper()}{i:02d} — Example\n\nExample section.\n\n' for i in range(1,n+1))


class ParserRegression(unittest.TestCase):
    def errors(self,text,kind='reference'):
        return checker.validate(text,kind,'S')['errors']
    def test_valid_control(self): self.assertEqual(self.errors(doc('reference')),[])
    def test_legacy_schema_requires_explicit_fixture_mode(self):
        text=doc('reference').replace('schema: corelink-ownership/1.1','schema: corelink-ownership/1')
        self.assertIn('legacy schema 1.0 requires explicit historical-fixture mode',self.errors(text))
    def test_heading_inside_backtick_fence(self):
        t=doc('reference').replace('<a id="r08"></a>\n## R08 — Example','```md\n<a id="r08"></a>\n## R08 — Example\n```')
        self.assertIn('missing section heading: R08',self.errors(t))
    def test_heading_inside_tilde_fence(self):
        t=doc('reference').replace('<a id="r08"></a>\n## R08 — Example','~~~md\n<a id="r08"></a>\n## R08 — Example\n~~~')
        self.assertIn('missing section heading: R08',self.errors(t))
    def test_heading_inside_comment(self):
        t=doc('reference').replace('<a id="r08"></a>\n## R08 — Example','<!--\n<a id="r08"></a>\n## R08 — Example\n-->')
        self.assertIn('missing section heading: R08',self.errors(t))
    def test_inline_code_cannot_supply_anchor(self):
        t=doc('reference').replace('<a id="r08"></a>','`<a id="r08"></a>`')
        self.assertIn('missing section anchor: r08',self.errors(t))
    def test_longer_closing_fence_is_valid_commonmark(self):
        self.assertEqual(self.errors(doc('reference')+'\n```txt\nx\n````\n'),[])
    def test_heading_as_blockquote_not_top_level(self):
        t=doc('reference').replace('## R08 — Example','> ## R08 — Example')
        self.assertIn('missing section heading: R08',self.errors(t))
    def test_extra_h2_requires_canonical_index(self):
        t=doc('reference').replace('<a id="r08"></a>\n## R08 — Example','<a id="r08"></a>\n## R08 — Example\n\n## Appendix — unindexed')
        self.assertIn('unindexed H2 heading: Appendix — unindexed',self.errors(t))
    def test_duplicate_yaml_key(self):
        self.assertTrue(any('invalid frontmatter' in e for e in self.errors(doc('reference').replace('document: reference','document: reference\ndocument: maintenance'))))
    def test_wrong_kind(self):
        self.assertIn('document kind differs from --kind',self.errors(doc('reference').replace('document: reference','document: maintenance')))
    def test_empty_evidence(self):
        self.assertIn('empty/non-string frontmatter key: evidence_set',self.errors(doc('reference').replace('evidence_set: example-set','evidence_set:')))
    def test_split_summary(self):
        self.assertIn('entry summary exceeds 120 words',self.errors(doc('reference').replace('Example section.','word '*70+'\n\n'+'word '*70,1)))
    def test_description_601_rejected(self):
        t=doc('skill');t=t.replace('description: Ownership de example para mudanças na sua API; não autoriza produção.','description: '+'x'*601)
        self.assertIn('skill description must have 1..600 characters',self.errors(t,'skill'))
    def test_inner_anchor_does_not_cut_budget(self):
        t=doc('blast_radius').replace('<a id="b04"></a>','<a id="rel-001"></a>\n### REL-001 — Example\n<a id="inside"></a>\n'+'\n'.join('- '+'detail '*9 for _ in range(18))+'\n[Return](#b03)\n\n<a id="b04"></a>')
        self.assertIn('REL-001 exceeds 120 words',self.errors(t,'blast_radius'))
    def test_relation_index_required(self):
        t=doc('blast_radius').replace('<a id="b04"></a>','<a id="rel-001"></a>\n### REL-001 — Example\nContract.\n[Return](#b03)\n\n<a id="b04"></a>')
        self.assertIn('record missing from preceding index: REL-001',self.errors(t,'blast_radius'))
    def test_template_uses_one_slug(self):
        for file in ('SKILL.md.tmpl','ISSUE.md.tmpl'):
            t=(ROOT/'templates'/file).read_text()
            self.assertIn('{{SKILL_SLUG}}',t)
            self.assertNotIn('own-{{PACKAGE}}',t)
    def test_long_name_deterministic(self):
        p='Ab_c'*30
        self.assertEqual(skill_slug(p),skill_slug(p));self.assertLessEqual(len(skill_slug(p)),64)


class EvidenceGate(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name)
        self.population=['crates/example/Cargo.toml','crates/example/src/lib.rs']
        self.write('crates/example/Cargo.toml','[package]\nname="example"\nversion="0.1.0"\n')
        self.write('crates/example/src/lib.rs','pub fn example() {}\n')
        self.write('docs/ownership/STANDARD.md',
                   '# Synthetic policy\n\n**Versão:** fixture · **Estado:** test-only.\n')
        self.write('evidence/fixture.txt','Synthetic test-only review and local execution evidence.\n')
        def sha(p):return gate.digest((self.root/p).read_bytes())
        ev={'id':'fixture','path':'evidence/fixture.txt','sha256':sha('evidence/fixture.txt'),'class':'REVIEW'}
        rules=[{'id':'consumers','globs':['crates/*.rs','crates/*/Cargo.toml'],'needles':['example']}]
        self.r={'schema':'corelink-ownership-record/1.1','record_id':'example-set','record_path':'docs/ownership/records/example.json',
            'repository_id':1,'package_uid':'repo:1:crates/example/Cargo.toml','package':'example','manifest':'crates/example/Cargo.toml',
            'skill_slug':'own-example','source_commit':'a'*40,'profile':'S',
            'standard':{'version':'fixture','path':'docs/ownership/STANDARD.md','sha256':sha('docs/ownership/STANDARD.md')},
            'author':{'identity':'fixture-author','session_id':'fixture-a'},
            'sources':[{'path':'crates/example/Cargo.toml','sha256':sha('crates/example/Cargo.toml'),'role':'manifest'},
                       {'path':'crates/example/src/lib.rs','sha256':sha('crates/example/src/lib.rs'),'role':'implementation'}],
            'evidence':[ev], 'discovery':{'rules':rules,'snapshot_sha256':gate.canonical_digest(gate.discovery_snapshot(self.root,rules,self.population))},
            'capacity':{'atomic_relations':0,'semantic_modules':1,'procedures':0,'evidence':['fixture']},
            'build_selections':[{'id':'fixture-host','package':'example','target':'x86_64-unknown-linux-gnu','features':[],'basis_evidence':['fixture']}],
            'review':{'reviewer':'fixture-reviewer','session_id':'fixture-b','fresh_context_confirmed':True,'independence_evidence':['fixture'],'reviewed_at':'2026-09-19T22:00:00Z'},
            'artifacts':[], 'relations':[], 'procedures':[]}
        for kind,path in gate.expected_paths('example','own-example').items():
            self.write(path,doc(kind))
            self.r['artifacts'].append({'kind':kind,'path':path,'sha256':sha(path),'verdict':'APPROVE',
                'checks':[{'requirement':req,'status':'PASS','evidence':['fixture']} for req in gate.CHECKS[kind]],
                'findings':[],'limitations':['Synthetic fixture, not actual independent review.']})
    def write(self,p,txt):
        file=self.root/p;file.parent.mkdir(parents=True,exist_ok=True);file.write_text(txt)
    def errors(self):return gate.record_errors(self.r,self.root,population=self.population)
    def test_consistency_control(self):self.assertEqual(self.errors(),[])
    def test_document_edit_stales_review(self):
        (self.root/self.r['artifacts'][0]['path']).write_text(doc('skill')+'\nChanged.\n')
        self.assertTrue(any('artifact bytes differ' in e for e in self.errors()))
    def metadata_readback(self):
        art=self.r['artifacts'][0]
        reviewed=doc('skill').replace('source-commit: "'+'a'*40+'"',
                                       'source-commit: "'+'b'*40+'"')
        self.write('evidence/reviewed-skill.md',reviewed)
        old_sha=gate.digest(reviewed.encode())
        self.r['evidence'].append({'id':'reviewed-skill','path':'evidence/reviewed-skill.md',
                                   'sha256':old_sha,'class':'REVIEW'})
        art['semantic_review']={'sha256':old_sha,'snapshot_evidence':'reviewed-skill'}
        art['metadata_readback']={'sha256':art['sha256'],'reader':'fixture-reader',
                                  'session_id':'fixture-c','read_at':'2026-09-23T12:00:00Z',
                                  'evidence':['fixture'],'changed_fields':['source-commit']}
        return art
    def test_metadata_only_readback_preserves_semantic_review(self):
        self.metadata_readback()
        self.assertEqual(self.errors(),[])
    def test_metadata_only_delta_rejects_other_top_level_mapping(self):
        reviewed=doc('skill').replace('source-commit: "'+'a'*40+'"',
                                      'source-commit: "'+'b'*40+'"')
        reviewed=reviewed.replace('---\n\n# Example', 'extra:\n  package: old\n---\n\n# Example')
        current=reviewed.replace('source-commit: "'+'b'*40+'"',
                                 'source-commit: "'+'a'*40+'"').replace('  package: old', '  package: new')
        self.assertEqual(checker.validate(reviewed,'skill','S')['errors'],[])
        self.assertEqual(checker.validate(current,'skill','S')['errors'],[])
        self.assertIsNone(gate.metadata_only_delta(reviewed.encode(),current.encode(),'skill'))
    def test_metadata_only_delta_rejects_nested_mapping_change(self):
        reviewed=doc('skill').replace('source-commit: "'+'a'*40+'"',
                                      'source-commit: "'+'b'*40+'"')
        reviewed=reviewed.replace('---\n\n# Example', 'extra:\n  nested:\n    package: old\n---\n\n# Example')
        current=reviewed.replace('source-commit: "'+'b'*40+'"',
                                 'source-commit: "'+'a'*40+'"').replace('    package: old', '    package: new')
        self.assertIsNone(gate.metadata_only_delta(reviewed.encode(),current.encode(),'skill'))
    def test_metadata_readback_rejects_other_mapping_change(self):
        art=self.metadata_readback()
        snapshot=self.root/'evidence/reviewed-skill.md'
        reviewed=snapshot.read_text().replace('---\n\n# Example',
                                              'extra:\n  package: old\n---\n\n# Example')
        snapshot.write_text(reviewed)
        reviewed_sha=gate.digest(reviewed.encode())
        self.r['evidence'][-1]['sha256']=reviewed_sha
        art['semantic_review']['sha256']=reviewed_sha
        current=(self.root/art['path']).read_text().replace('---\n\n# Example',
                                                          'extra:\n  package: new\n---\n\n# Example')
        self.write(art['path'],current)
        art['sha256']=art['metadata_readback']['sha256']=gate.digest(current.encode())
        self.assertIn('skill: changed bytes exceed declared metadata-only fields',self.errors())
    def test_metadata_readback_rejects_semantic_edit(self):
        art=self.metadata_readback()
        current=doc('skill')+'\nChanged body.\n'
        self.write(art['path'],current)
        art['sha256']=art['metadata_readback']['sha256']=gate.digest(current.encode())
        self.assertIn('skill: changed bytes exceed declared metadata-only fields',self.errors())
    def test_metadata_readback_requires_snapshot_and_evidence(self):
        art=self.metadata_readback()
        art['semantic_review']['snapshot_evidence']='unknown'
        art['metadata_readback']['evidence']=['unknown']
        errors=self.errors()
        self.assertTrue(any('semantic review snapshot needs REVIEW evidence' in error for error in errors))
        self.assertTrue(any('missing evidence references' in error for error in errors))
    def test_source_removed(self):
        (self.root/'crates/example/src/lib.rs').unlink()
        self.assertTrue(self.errors())
    def test_source_change(self):
        self.write('crates/example/src/lib.rs','pub fn example_changed() {}')
        self.assertTrue(any('missing or changed' in e for e in self.errors()))
    def test_manifest_feature_change(self):
        with (self.root/'crates/example/Cargo.toml').open('a') as f:f.write('[features]\nnew=[]\n')
        self.assertTrue(self.errors())
    def test_new_matching_consumer(self):
        self.write('crates/new/src/lib.rs','use example::Thing;');self.population.append('crates/new/src/lib.rs')
        self.assertIn('consumer/source discovery changed: new, removed or modified match',self.errors())
    def test_unrelated_file_does_not_invalidate(self):
        self.write('crates/unrelated/src/lib.rs','pub fn unrelated() {}');self.population.append('crates/unrelated/src/lib.rs')
        self.assertEqual(self.errors(),[])
    def test_stale_standard(self):
        self.write('docs/ownership/STANDARD.md','Different standard')
        self.assertTrue(self.errors())
    def test_review_standard_path_must_be_canonical_even_with_identical_bytes(self):
        self.write('docs/ownership/ALTERNATE.md',
                   (self.root/'docs/ownership/STANDARD.md').read_text())
        self.r['standard']['path']='docs/ownership/ALTERNATE.md'
        self.assertIn('review standard path differs from canonical candidate',self.errors())
    def test_review_standard_version_must_match_candidate(self):
        self.r['standard']['version']='not-the-candidate'
        self.assertIn('review standard version differs from canonical candidate',self.errors())
    def test_same_session_not_cold(self):
        self.r['review']['session_id']='fixture-a';self.assertIn('review shares author session',self.errors())
    def test_same_identity_not_independent(self):
        self.r['review']['reviewer']='fixture-author';self.assertIn('reviewer is the author',self.errors())
    def test_empty_check_arrays_refused(self):
        self.r['artifacts'][0]['checks']=[];self.assertTrue(self.errors())
    def test_nonexistent_evidence_id(self):
        self.r['review']['independence_evidence']=['unknown'];self.assertTrue(self.errors())
    def test_duplicate_artifact_refused(self):
        self.r['artifacts'][3]=copy.deepcopy(self.r['artifacts'][0]);self.assertTrue(self.errors())
    def test_required_finding_open(self):
        self.r['artifacts'][0]['findings']=[{'id':'F1','required':True,'state':'OPEN','resolution_evidence':[]}]
        self.assertTrue(self.errors())
    def test_wrong_slug(self):
        self.r['skill_slug']='own-wrong';self.assertTrue(self.errors())
    def test_noncanonical_paths(self):
        self.r['artifacts'][0]['path']='elsewhere/SKILL.md';self.assertTrue(self.errors())
    def test_symlink_path_escape(self):
        self.root.joinpath('outside').symlink_to(self.root.parent)
        with self.assertRaises(ValueError):gate.safe_path(self.root,'outside/x')
    def add_procedure(self, mode='LOCAL_ISOLATED', executed=True, required=True):
        art=next(a for a in self.r['artifacts'] if a['kind']=='maintenance')
        text=(self.root/art['path']).read_text().replace('## M02 — Example','## M02 — Example\n\n[PROC-001](#proc-001)')
        text=text.replace('<a id="m04"></a>','<a id="proc-001"></a>\n### PROC-001 — Fixture\nRead an isolated fixture.\n[Return](#m02)\n\n<a id="m04"></a>')
        self.write(art['path'],text);art['sha256']=gate.digest(text.encode())
        self.write('evidence/local-test.txt','Local synthetic test evidence, not a live operation.')
        self.r['evidence'].append({'id':'local','path':'evidence/local-test.txt','sha256':gate.digest((self.root/'evidence/local-test.txt').read_bytes()),'class':'EXECUTED_LOCAL'})
        self.r['procedures']=[{'id':'PROC-001','mode':mode,'review_status':'REVIEWED',
          'execution_status':'EXECUTED_LOCAL' if executed else 'BLOCKED_FOR_OPERATION',
          'required_for_acceptance':required,'environment':'isolated synthetic fixture',
          'result':'PASS' if executed else 'NOT_EXECUTED','review_evidence':['fixture'],
          'execution_evidence':['local'] if executed else [],
          'limitations':[] if executed else ['No authorized production operation executed.']}]
    def test_local_procedure_evidence_control(self):
        self.add_procedure();self.assertEqual(self.errors(),[])
    def test_required_procedure_not_executed_refused(self):
        self.add_procedure(executed=False);self.assertTrue(self.errors())
    def test_authorized_unexecuted_honestly_bounded(self):
        self.add_procedure(mode='AUTHORIZED_OPERATION',executed=False,required=False)
        self.assertEqual(self.errors(),[])
    def test_local_result_cannot_certify_production_operation(self):
        self.add_procedure(mode='AUTHORIZED_OPERATION');self.assertTrue(self.errors())
    def test_review_text_is_not_execution_evidence(self):
        self.add_procedure();self.r['procedures'][0]['execution_evidence']=['fixture']
        self.assertTrue(self.errors())
    def test_undocumented_procedure_refused(self):
        self.add_procedure();self.r['procedures'][0]['id']='PROC-002';self.assertTrue(self.errors())
    def test_malformed_review_time(self):
        self.r['review']['reviewed_at']='not-a-date';self.assertTrue(self.errors())
    def test_relation_missing_from_document_rejected(self):
        self.r['relations']=[self.relation()];self.assertTrue(self.errors())
    def relation(self):
        return {'key':'repo:1:boundary:example-import','kind':'dependency',
          'consumer_uid':self.r['package_uid'],'provider_uid':'repo:1:crates/provider/Cargo.toml',
          'contract_owner_uid':'repo:1:crates/provider/Cargo.toml','contract_id':'API-001',
          'contract_sha256':self.r['sources'][0]['sha256'],'surface':'provider::Value',
          'activation':'declared source only','data_direction':'not-applicable',
          'impact_direction':'provider-to-consumer','local_effect':'API changes require caller validation.',
          'local_anchor':'rel-001','external_boundary':None}
    def test_mirror_disagreement_detected(self):
        # Cross-view validation tested independently of the individual document checks.
        other=copy.deepcopy(self.r);other['package_uid']='repo:1:crates/provider/Cargo.toml'
        a=self.relation();b=copy.deepcopy(a);b['activation']='different activation'
        self.r['relations']=[a];other['relations']=[b]
        report=gate.integrate([self.r,other],self.root,population=self.population)
        self.assertTrue(any('views disagree' in e for e in report['cross_package_errors']))
    def test_missing_mirror_detected(self):
        other=copy.deepcopy(self.r);other['package_uid']='repo:1:crates/provider/Cargo.toml'
        self.r['relations']=[self.relation()]
        report=gate.integrate([self.r,other],self.root,population=self.population)
        self.assertTrue(any('missing or duplicate endpoint view' in e for e in report['cross_package_errors']))
    def test_dependency_and_impact_have_explicit_directions(self):
        edge=self.relation();self.assertEqual(edge['consumer_uid'],self.r['package_uid'])
        self.assertEqual(edge['impact_direction'],'provider-to-consumer')
        self.assertNotEqual(edge['data_direction'],edge['impact_direction'])
    def test_integration_order_independent_and_keeps_both(self):
        # Two independent records; change identities, source and doc paths, evidence-set.
        other=copy.deepcopy(self.r);other.update({'package':'other','package_uid':'repo:1:crates/other/Cargo.toml',
            'manifest':'crates/other/Cargo.toml','skill_slug':'own-other','record_id':'other-set','record_path':'docs/ownership/records/other.json'})
        self.write(other['manifest'],'[package]\nname="other"\nversion="0.1.0"\n')
        other['sources']=[{'path':other['manifest'],'sha256':gate.digest((self.root/other['manifest']).read_bytes()),'role':'manifest'}]
        other['discovery']['rules'][0]['needles']=['other']
        pop=self.population+[other['manifest']]
        for rec in (self.r,other):
            rec['discovery']['snapshot_sha256']=gate.canonical_digest(gate.discovery_snapshot(self.root,rec['discovery']['rules'],pop))
        for art in other['artifacts']:
            art['path']=gate.expected_paths('other','own-other')[art['kind']]
            self.write(art['path'],doc(art['kind'],'other',other['manifest'],'other-set'))
            art['sha256']=gate.digest((self.root/art['path']).read_bytes())
        a=gate.integrate([self.r,other],self.root,population=pop)
        b=gate.integrate([other,self.r],self.root,population=pop)
        self.assertEqual(a,b);self.assertTrue(a['consistent'],a);self.assertEqual(len(a['packages']),2)
        self.assertEqual(gate.render_index(a),gate.render_index(b))
        self.assertIn('| example |',gate.render_index(a));self.assertIn('| other |',gate.render_index(a))
    def test_duplicate_records_refused(self):
        self.assertFalse(gate.integrate([self.r,self.r],self.root,population=self.population)['consistent'])

if __name__=='__main__':unittest.main()
