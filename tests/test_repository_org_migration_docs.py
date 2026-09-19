"""Checks the migration kit, not live readiness or authorization to transfer."""
from __future__ import annotations

import json
import re
import subprocess
import sys
import unittest
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / 'docs/operations/repository-org-migration'
SKILL = ROOT / '.claude/skills/migrate-server-organization/SKILL.md'
sys.path.insert(0, str(ROOT / 'scripts'))
import repository_org_migration as migration

DOCS = ('README.md', 'MAP.md', 'PORTABILITY.md', 'RUNBOOK.md',
        'WORK-PACKAGES.md', 'ACCEPTANCE.md', 'SOURCE-ANCHORS.md', 'EVIDENCE.md')
AXIOMS = ('Completeness criteria', 'Success criteria', 'Quality standards',
          'Definition of Done', 'Invariants')


def read_json(name: str):
    return json.loads((DOC / name).read_text())


def headings(path: Path) -> set[str]:
    # Our links use unique, simple headings. Fenced examples are not headings.
    result = set()
    fenced = False
    for line in path.read_text().splitlines():
        if line.startswith('```'):
            fenced = not fenced
        if not fenced and re.match(r'^#{1,6} ', line):
            title = re.sub(r'^#+\s+', '', line).strip().lower()
            result.add(re.sub(r'[^\w\- ]', '', title).replace(' ', '-'))
    return result


class MigrationDocumentationTests(unittest.TestCase):
    def test_required_deliverables_exist(self):
        for name in DOCS:
            with self.subTest(name=name):
                self.assertTrue((DOC / name).is_file())
        self.assertTrue(SKILL.is_file())
        for name in ('migration.json', 'evidence/source-inventory.json',
                     'evidence/github-snapshot.json', 'evidence/workflow-surface.json',
                     'evidence/workflow-api-delta.json', 'evidence/dependency-identities.json',
                     'evidence/validation.json'):
            with self.subTest(name=name):
                self.assertTrue((DOC / name).is_file())

    def test_local_links_resolve_inside_repository(self):
        for source in [*(DOC / name for name in DOCS), SKILL]:
            for raw in re.findall(r'\[[^\]\n]*\]\(([^)\s]+)\)', source.read_text()):
                link = urlsplit(raw)
                if link.scheme or link.netloc:
                    continue
                target = (source.parent / unquote(link.path)).resolve() if link.path else source
                with self.subTest(source=source.name, link=raw):
                    self.assertTrue(target.is_relative_to(ROOT))
                    self.assertTrue(target.is_file(), f'missing {target}')
                    if link.fragment and target.suffix == '.md':
                        self.assertIn(unquote(link.fragment), headings(target))

    def test_eight_work_packages_have_all_five_axioms(self):
        text = (DOC / 'WORK-PACKAGES.md').read_text()
        sections = re.split(r'(?m)^## (WP-\d{2})[^\n]*\n', text)
        pairs = dict(zip(sections[1::2], sections[2::2]))
        self.assertEqual(set(pairs), {f'WP-{n:02d}' for n in range(8)})
        for name, body in pairs.items():
            for axiom in AXIOMS:
                with self.subTest(package=name, axiom=axiom):
                    self.assertIn('| ' + axiom + ' |', body)

    def test_ten_gates_are_defined_exactly_once(self):
        text = (DOC / 'ACCEPTANCE.md').read_text()
        gates = re.findall(r'(?m)^\| (G\d{2}) —', text)
        self.assertEqual(gates, [f'G{n:02d}' for n in range(1, 11)])
        self.assertEqual([x.split('-')[0] for x in migration.GATES], gates)

    def test_skill_metadata_and_discovery_default(self):
        text = SKILL.read_text()
        self.assertTrue(text.startswith('---\nname: migrate-server-organization\n'))
        self.assertIn('version: 1.0.0\n', text)
        self.assertIn('Padrão somente descoberta e planejamento', text)
        self.assertIn('G01–G08', text)
        self.assertIn('G09/G10', text)

    def test_source_inventory_reproduces_exact_baseline(self):
        manifest = read_json('migration.json')
        actual = migration.scan(ROOT, manifest['source']['baseline_sha'], manifest)
        self.assertEqual(actual, read_json('evidence/source-inventory.json'))

    def test_snapshot_repository_and_ref_anchor_match(self):
        manifest = read_json('migration.json')
        snapshot = read_json('evidence/github-snapshot.json')
        metadata = snapshot['surfaces']['metadata']['pages'][0]
        self.assertEqual(metadata['id'], manifest['source']['repository_id'])
        self.assertEqual(metadata['owner']['id'], manifest['source']['owner_id'])
        self.assertEqual(metadata['full_name'], manifest['source']['owner'] + '/corelink-server')
        self.assertTrue(metadata['private'])
        refs = [x for page in snapshot['surfaces']['heads']['pages'] for x in page]
        main = [x for x in refs if x['ref'] == 'refs/heads/main']
        self.assertEqual(len(main), 1)
        self.assertEqual(main[0]['object']['sha'], manifest['source']['baseline_sha'])

    def test_snapshot_credential_surfaces_never_export_values(self):
        surfaces = read_json('evidence/github-snapshot.json')['surfaces']
        for name, surface in surfaces.items():
            for page in surface.get('pages', []):
                if isinstance(page, dict):
                    for kind in ('variables', 'secrets'):
                        for item in page.get(kind, []):
                            with self.subTest(surface=name, kind=kind):
                                self.assertNotIn('value', item)
                                self.assertNotIn('encrypted_value', item)
                if name in ('webhooks', 'deploy_keys'):
                    for item in page:
                        self.assertNotIn('config', item)
                        self.assertNotIn('key', item)

    def test_snapshot_unknowns_are_not_erased(self):
        snapshot = read_json('evidence/github-snapshot.json')
        expected = {k for k, v in snapshot['surfaces'].items() if v['status'] != 'OBSERVED'}
        self.assertEqual(set(snapshot['unverified_surfaces']), expected)
        self.assertEqual(expected, {'codespaces_secrets', 'rulesets', 'protection'})
        for name in expected:
            self.assertNotIn('pages', snapshot['surfaces'][name])

    def test_workflow_inventory_paths_match_baseline_tree(self):
        manifest = read_json('migration.json')
        report = read_json('evidence/workflow-surface.json')
        base = manifest['source']['baseline_sha']
        paths = subprocess.check_output(['git', '-C', str(ROOT), 'ls-tree', '-r', '--name-only', base, '.github/workflows'], text=True).splitlines()
        names = {Path(p).name for p in paths if p.endswith(('.yml', '.yaml'))}
        self.assertEqual(report['source_sha'], base)
        self.assertEqual(set(report['workflow_files']), names)
        for mapping in ('secret_references', 'variable_references'):
            for consumers in report[mapping].values():
                for consumer in consumers:
                    self.assertIn(consumer, paths)

    def test_api_workflow_delta_is_exact_not_an_execution_claim(self):
        delta = read_json('evidence/workflow-api-delta.json')
        report = read_json('evidence/workflow-surface.json')
        snapshot = read_json('evidence/github-snapshot.json')
        live = [x for p in snapshot['surfaces']['workflows']['pages'] for x in p['workflows']]
        tree = {'.github/workflows/' + p for p in report['workflow_files']}
        absent = {(x['id'], x['path']) for x in live if x['path'] not in tree}
        declared = {(x['id'], x['path']) for x in delta['platform_dynamic_records'] + delta['file_paths_not_in_base_tree']}
        self.assertEqual(absent, declared)
        self.assertEqual(delta['api_count'], len(live))
        self.assertEqual(delta['tree_file_count'], len(tree))
        self.assertEqual(set(delta['tree_paths_not_in_api']), tree - {x['path'] for x in live})

    def test_independent_dependency_metadata_matches_roles(self):
        manifest = read_json('migration.json')
        metadata = {x['id']: x for x in read_json('evidence/dependency-identities.json')['dependencies']}
        for entry in manifest['dependencies'].values():
            self.assertEqual(metadata[entry['repository_id']]['full_name'], entry['owner'] + '/' + entry['name'])
            self.assertFalse(entry['transfer_in_this_campaign'])

    def test_ci_covers_doc_only_and_skill_only_changes(self):
        workflow = (ROOT / '.github/workflows/python-tests.yml').read_text()
        for event, pattern in (
            ('pull_request', r'(?ms)^  pull_request:\n(.*?)^  push:'),
            ('push', r'(?ms)^  push:\n(.*?)^  workflow_dispatch:'),
        ):
            match = re.search(pattern, workflow)
            self.assertIsNotNone(match)
            for path in ('docs/operations/repository-org-migration/**', '.claude/skills/migrate-server-organization/**'):
                with self.subTest(event=event, path=path):
                    self.assertIn("      - '" + path + "'", match[1])

    def test_backlog_only_claims_kit_not_transfer(self):
        text = (ROOT / 'BACKLOG.md').read_text()
        self.assertEqual(len(re.findall(r'(?m)^### B-374\b', text)), 1)
        section = text.split('### B-374', 1)[1].split('\n### ', 1)[0]
        self.assertIn("test_repository_org_migration*.py", section)
        self.assertIn('Não significa execução de transferência', section)


if __name__ == '__main__':
    unittest.main()
