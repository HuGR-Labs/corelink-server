"""Snapshot consistency tests for the source-grounded 2026-09-19 preparation.
These do not execute Cargo or certify the semantics of the original crates.
"""
import hashlib
import json
import posixpath
from pathlib import Path
import re
import sys
import tomllib
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tools'))
from prepare_census import skill_slug

def load(path): return json.loads((ROOT/path).read_text())

class ManifestPreparation(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.census=load('inventory/declared-census.json')
        cls.proofs=load('evidence/revision-1.2/manifest-identities.json')
        cls.by={p['manifest']:p for p in cls.proofs}
        cls.units=cls.census['units']
        cls.contexts=load('preparation/manifest-context.json')['contexts']
        cls.ledger=load('plans/publication-ledger.json')['items']

    def test_identity_set_has_105_unique_manifests(self):
        self.assertEqual(len(self.proofs),105)
        self.assertEqual(len(self.by),105)
        self.assertEqual(set(self.by),{u['manifest'] for u in self.units})

    def test_all_95_declared_workspace_names_are_confirmed(self):
        units=[u for u in self.units if u['scope']=='workspace_declared']
        self.assertEqual(len(units),95)
        self.assertTrue(all(u['package_name'] and u['name_evidence']=='manifest_read_at_pinned_main' for u in units))

    def test_names_parse_from_observed_source_excerpts(self):
        for p in self.proofs:
            with self.subTest(manifest=p['manifest']):
                self.assertEqual(tomllib.loads(p['observed_identity_excerpt'])['package']['name'],p['package_name'])

    def test_source_revision_and_blob_identifiers(self):
        baseline=self.census['source_commit']
        self.assertEqual(baseline,'cca798ff5bc2df660ecf2570ed243eb9775ff3d0')
        for p in self.proofs:
            self.assertEqual(p['source_commit'],baseline)
            self.assertRegex(p['provider_reported_git_blob'],r'^[a-f0-9]{40}$')
            self.assertIn('/blob/'+baseline+'/'+p['manifest']+'#L',p['source_url'])

    def test_package_names_and_skill_slugs_are_unique(self):
        names=[u['package_name'] for u in self.units]
        slugs=[u['skill_slug'] for u in self.units]
        self.assertEqual(len(set(names)),105)
        self.assertEqual(len(set(slugs)),105)
        for n,s in zip(names,slugs): self.assertEqual(s,skill_slug(n))

    def test_nontrivial_aliases_are_preserved(self):
        expected={'crates/corelink-container/Cargo.toml':'corelink-server',
                  'crates/tenant-path/Cargo.toml':'corelink-tenant-path',
                  'tools/cli/Cargo.toml':'corelink-cli','tools/sdks/python/Cargo.toml':'corelink-py',
                  'tools/sdks/go/Cargo.toml':'corelink-go','tests/chaos/Cargo.toml':'chaos-campaign',
                  'tools/openapi/Cargo.toml':'corelink-openapi','tools/dt-cli/Cargo.toml':'corelink-dt-cli'}
        for path,name in expected.items(): self.assertEqual(self.by[path]['package_name'],name)
        cli=self.contexts['tools/cli/Cargo.toml']['explicit_targets']
        self.assertEqual([t['name'] for t in cli if t['kind']=='bin'],['corelink'])

    def test_one_draft_marker_per_unit(self):
        seen=set()
        for u in self.units:
            text=(ROOT/u['body_path']).read_text()
            markers=re.findall(r'<!-- corelink-ownership:v1:manifest=([^ ]+) -->',text)
            self.assertEqual(markers,[u['manifest']]);seen.add(u['manifest'])
        self.assertEqual(len(seen),105)

    def test_drafts_have_no_identity_placeholders(self):
        for u in self.units:
            text=(ROOT/u['body_path']).read_text()
            self.assertNotIn('{{PACKAGE',text)
            self.assertNotIn('{{SKILL_SLUG',text)
            self.assertNotIn('package a confirmar',text)

    def test_four_destinations_match_identity(self):
        for u in self.units:
            text=(ROOT/u['body_path']).read_text()
            self.assertIn(f".claude/skills/{u['skill_slug']}/SKILL.md",text)
            for name in ('REFERENCE.md','BLAST_RADIUS.md','MAINTENANCE.md'):
                self.assertIn(f"docs/ownership/crates/{u['package_name']}/{name}",text)

    def test_drafts_respect_issue_budgets(self):
        for u in self.units:
            raw=(ROOT/u['body_path']).read_bytes(); text=raw.decode()
            with self.subTest(manifest=u['manifest']):
                self.assertLessEqual(len(raw),24000)
                self.assertLessEqual(len(text.split()),3000)
                self.assertLessEqual(len(text.splitlines()),240)

    def test_all_publication_rows_remain_blocked(self):
        self.assertEqual(len(self.ledger),105)
        self.assertEqual({i['manifest'] for i in self.ledger},set(self.by))
        for item in self.ledger:
            self.assertEqual(item['state'],'BLOCKED')
            self.assertIsNone(item['issue_number']); self.assertIsNone(item['issue_url'])
            self.assertFalse(item['contract']['frozen']); self.assertIsNone(item['contract']['url'])
            self.assertTrue(all(g['state']=='PENDING' for g in item['gates'].values()))

    def test_publication_body_hashes_match_updated_drafts(self):
        for item in self.ledger:
            self.assertEqual(item['body_sha256'],hashlib.sha256((ROOT/item['body_path']).read_bytes()).hexdigest())

    def test_source_confirmation_does_not_claim_cargo_execution(self):
        self.assertFalse(self.census['scope_complete_for_tracked_first_party_manifests'])
        self.assertTrue(all(not u['cargo_metadata_verified'] for u in self.units))
        self.assertTrue(all(not p['cargo_metadata_verified'] for p in self.proofs))
        self.assertEqual(self.census['github_publications'],0)
        self.assertEqual(self.census['full_semantic_packets'],0)

    def test_ten_independent_fuzz_manifests_and_new_cli_entry(self):
        fuzz=[u for u in self.units if u['scope']=='fuzz_manifest_confirmed']
        self.assertEqual(len(fuzz),10)
        self.assertIn('tools/cli/fuzz/Cargo.toml',{u['manifest'] for u in fuzz})
        for u in fuzz: self.assertTrue(self.contexts[u['manifest']]['workspace_is_independent'])

    def test_fuzz_declared_targets_total_23(self):
        fuzz=[c for c in self.contexts.values() if c.get('workspace_is_independent')]
        self.assertEqual(sum(len(c['explicit_targets']) for c in fuzz),23)
        self.assertEqual(len(self.contexts['tools/cli/fuzz/Cargo.toml']['explicit_targets']),5)

    def test_ac_fuzz_does_not_gain_an_invented_parent_dependency(self):
        ac=self.contexts['crates/corelink-ac/fuzz/Cargo.toml']
        self.assertEqual(ac['local_dependencies'],[])
        self.assertNotIn('corelink-ac',ac['declared_dependency_keys'])

    def test_client_verify_fuzz_preserves_ffi_specific_policy(self):
        c=self.contexts['crates/corelink-client-verify/fuzz/Cargo.toml']
        self.assertEqual(c['unsafe_code_declared'],'allow')
        self.assertEqual(c['local_dependencies'][0]['features'],['stream','ffi'])

    def test_reapi_unset_bench_is_not_fabricated_false(self):
        c=self.contexts['crates/corelink-reapi/fuzz/Cargo.toml']
        self.assertEqual(c['version'],'0.1.0')
        self.assertTrue(all(t['bench_declared'] is None for t in c['explicit_targets']))

    def test_fuzz_local_paths_resolve_to_verified_packages(self):
        for c in self.contexts.values():
            for dep in c.get('local_dependencies',[]):
                path=posixpath.normpath(posixpath.join(posixpath.dirname(c['manifest']),dep['path'],'Cargo.toml'))
                self.assertIn(path,self.by)
                self.assertEqual(self.by[path]['package_name'],dep['key'])

    def test_24_context_records_keep_partial_scope(self):
        self.assertEqual(len(self.contexts),24)
        for path,c in self.contexts.items():
            self.assertEqual(c['provider_reported_git_blob'],self.by[path]['provider_reported_git_blob'])
            self.assertFalse(c['semantic_context_complete'])
            self.assertFalse(c['runtime_verified'])
            self.assertFalse(c['resolved_graph_verified'])
            self.assertFalse(c['all_targets_enumerated'])

    def test_contract_tools_templates_and_original_tests_preserved(self):
        changed={row['path'] for row in load('evidence/revision-1.4/changed-framework-files.json')['files']}
        changed|={p.removeprefix('docs/ownership/') for p in changed}
        for row in load('evidence/revision-1.2/preserved-files.json'):
            if row['path'] in changed:
                continue
            self.assertEqual(hashlib.sha256((ROOT/row['path']).read_bytes()).hexdigest(),row['sha256'])

if __name__=='__main__': unittest.main()
