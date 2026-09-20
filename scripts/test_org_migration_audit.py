"""Focal tests for the offline organization migration auditor."""
from __future__ import annotations

import contextlib
import copy
import hashlib
import io
import json
import re
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from urllib.parse import unquote, urlsplit
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import org_migration_audit as audit

PROFILE = json.loads(audit.DEFAULT_MANIFEST.read_text(encoding='utf-8'))


class ProfileTests(unittest.TestCase):
    def test_example_profile_is_valid_and_server_only(self):
        audit.validate_manifest(PROFILE)
        self.assertEqual(PROFILE['source']['repository_id'], 1232040291)
        self.assertEqual(PROFILE['transfer_scope'], ['server'])
        self.assertTrue(all(not item['transfer_in_this_campaign']
                            for item in PROFILE['dependencies'].values()))

    def test_plan_is_always_unapproved_and_has_twenty_unknown_gates(self):
        result = audit.plan(PROFILE)
        self.assertEqual(result['status'], 'PLANNING_ONLY_NOT_AUTHORIZED')
        self.assertFalse(result['migration_ready'])
        self.assertFalse(result['authorization_verified'])
        self.assertEqual([gate['id'] for gate in result['gates']], list(audit.GATES))
        self.assertTrue(all(gate['status'] == 'UNKNOWN' for gate in result['gates']))
        self.assertIsNone(result.get('transfer_command'))

    def test_plan_supports_missing_destination(self):
        profile = copy.deepcopy(PROFILE)
        profile['destination'] = None
        result = audit.plan(profile)
        self.assertIsNone(result['target'])
        self.assertIsNone(result['predicted_immutable_sub_prefix'])

    def test_invalid_target_is_rejected(self):
        for owner, owner_id in [('', 10), ('org-', 10), ('two--parts', 10),
                                ('a/b', 10), ('$(cmd)', 10), ('x' * 40, 10),
                                ('Target', None), (None, 987654321)]:
            with self.subTest(owner=owner, owner_id=owner_id), self.assertRaises(ValueError):
                audit.plan(PROFILE, owner, owner_id)
        with self.assertRaises(ValueError):
            audit.plan(PROFILE, 'hUgR-lAbs', 987654321)
        with self.assertRaises(ValueError):
            audit.plan(PROFILE, 'Another-Org', 311862110)

    def test_wrong_scope_and_peer_mutations_are_rejected(self):
        for path, value in [
            (('transfer_scope',), ['server', 'runners']),
            (('preserve_visibility',), 'public'),
            (('allow_repository_rename',), True),
            (('source', 'repository_id'), 1266754321),
            (('schema_version',), 9),
            (('dependencies', 'runners', 'transfer_in_this_campaign'), True),
        ]:
            profile = copy.deepcopy(PROFILE)
            target = profile
            for part in path[:-1]:
                target = target[part]
            target[path[-1]] = value
            with self.subTest(path=path), self.assertRaises(ValueError):
                audit.validate_manifest(profile)

    def test_planning_does_not_spawn_processes(self):
        with patch.object(subprocess, 'run', side_effect=AssertionError('offline plan spawned a process')):
            audit.plan(PROFILE)

    def test_no_mutation_subcommands_exist(self):
        for command in ('apply', 'transfer', 'dispatch', 'rewrite', 'deploy', 'publish'):
            with self.subTest(command=command), contextlib.redirect_stderr(io.StringIO()), \
                    self.assertRaises(SystemExit):
                audit.main([command])


class GitTreeAuditTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.git('init', '-q')
        self.git('config', 'user.name', 'Audit Fixture')
        self.git('config', 'user.email', 'audit@example.invalid')
        self.git('config', 'core.hooksPath', '/dev/null')
        self.git('config', 'commit.gpgsign', 'false')
        (self.root / 'config.txt').write_text(
            'café HuGR-Labs/corelink-server; HuGR-Labs/corelink-server\n'
            'café Surprise-org/corelink-server contains-sensitive-raw-text\u2028 '
            'HuGR-Labs/corelink-server\n', encoding='utf-8')
        (self.root / 'same-lines.txt').write_text(
            'HuGR-Labs/corelink-server\nHuGR-Labs/corelink-server\n', encoding='utf-8')
        (self.root / 'unicode-prefix.txt').write_text('ßcorelink-server\n', encoding='utf-8')
        (self.root / 'binary.dat').write_bytes(b'\x00HuGR-Labs')
        (self.root / 'historic.json').write_text('{"signer":"HumanGuardrail/corelink-server"}\n')
        (self.root / 'outside-link').symlink_to('/does/not/exist')
        path_only = self.root / 'HuGR-Labs' / 'corelink-server-path-fixture.txt'
        path_only.parent.mkdir()
        path_only.write_text('path-only owner fixture\n')
        unicode_path = self.root / 'ßcorelink-server-name.txt'
        unicode_path.write_text('unicode path fixture\n')
        self.git('add', '.')
        self.git('commit', '-qm', 'audit fixture')
        self.sha = self.git('rev-parse', 'HEAD').strip()

    def tearDown(self):
        self.temp.cleanup()

    def git(self, *args: str) -> str:
        return subprocess.check_output(['git', '-C', str(self.root), *args], text=True)

    def test_reads_exact_commit_not_dirty_or_untracked_files(self):
        (self.root / 'config.txt').write_text('dirty-only-secret\n', encoding='utf-8')
        (self.root / 'untracked-secret').write_text('HuGR-Labs\n', encoding='utf-8')
        before = self.git('status', '--porcelain')
        result = audit.scan(self.root, self.sha, PROFILE)
        self.assertEqual(result['source_sha'], self.sha)
        self.assertEqual(result['coverage']['tree_entries'], 8)
        self.assertEqual(result['coverage']['symlink_blobs_not_followed'], 1)
        self.assertNotIn('untracked-secret', [row['path'] for row in result['files']])
        self.assertNotIn('dirty-only-secret', json.dumps(result))
        self.assertEqual(before, self.git('status', '--porcelain'))

    def test_finding_ids_preserve_repeated_occurrences_and_no_raw_values(self):
        result = audit.scan(self.root, self.sha, PROFILE)
        repeated = [finding for finding in result['findings']
                    if finding['rule'] == 'SERVER_OWNER_REFERENCE'
                    and finding['path'] == 'config.txt' and finding['line'] == 1]
        self.assertEqual(len(repeated), 2)
        self.assertEqual(len({item['id'] for item in repeated}), 2)
        self.assertNotIn('contains-sensitive-raw-text', json.dumps(result))
        self.assertTrue(all(len(item['blob']) == 40 and len(item['line_sha256']) == 64
                            for item in repeated))

    def test_finding_ids_include_absolute_occurrence_across_identical_lines(self):
        result = audit.scan(self.root, self.sha, PROFILE)
        repeated = [item for item in result['findings']
                    if item['rule'] == 'SERVER_OWNER_REFERENCE'
                    and item['path'] == 'same-lines.txt']
        self.assertEqual([item['line'] for item in repeated], [1, 2])
        self.assertEqual([item['absolute_byte_start'] for item in repeated],
                         [0, len(b'HuGR-Labs/corelink-server\n')])
        self.assertEqual(len({item['id'] for item in result['findings']}), len(result['findings']))

    def test_ascii_case_matching_preserves_utf8_offsets_after_sharp_s(self):
        result = audit.scan(self.root, self.sha, PROFILE)
        content = next(item for item in result['findings']
                       if item['rule'] == 'ORG_LITERAL' and item['path'] == 'unicode-prefix.txt'
                       and item['term'].casefold() == 'corelink-server')
        path = next(item for item in result['findings']
                    if item['rule'] == 'ORG_LITERAL' and item['path'] == 'ßcorelink-server-name.txt'
                    and item['term'].casefold() == 'corelink-server')
        self.assertEqual(content['byte_start'], 2)
        self.assertEqual(content['absolute_byte_start'], 2)
        self.assertEqual(path['byte_start'], 2)

    def test_unknown_owner_and_utf8_byte_spans_are_explicit(self):
        result = audit.scan(self.root, self.sha, PROFILE)
        unknown = [item for item in result['findings']
                   if item['rule'] == 'SERVER_OWNER_REFERENCE'
                   and item['path'] == 'config.txt' and item['line'] == 2
                   and not item['configured_owner']]
        self.assertEqual(len(unknown), 1)
        self.assertFalse(unknown[0]['configured_owner'])
        self.assertEqual(unknown[0]['disposition'], 'UNRESOLVED')
        self.assertEqual(unknown[0]['byte_start'], len('café '.encode('utf-8')))
        same_git_line = [item for item in result['findings']
                         if item['rule'] == 'SERVER_OWNER_REFERENCE'
                         and item['path'] == 'config.txt' and item['term'] == 'HuGR-Labs'
                         and item['line'] == 2]
        self.assertEqual(len(same_git_line), 1)

    def test_path_only_reference_is_reported_without_a_line_number(self):
        result = audit.scan(self.root, self.sha, PROFILE)
        path_finding = next(item for item in result['findings']
                            if item['rule'] == 'SERVER_OWNER_REFERENCE'
                            and item['path'] == 'HuGR-Labs/corelink-server-path-fixture.txt')
        self.assertEqual(path_finding['location'], 'path')
        self.assertIsNone(path_finding['line'])
        self.assertEqual(path_finding['byte_start'], 0)

    def test_binary_symlink_and_coverage_limits_are_declared(self):
        result = audit.scan(self.root, self.sha, PROFILE)
        self.assertFalse(result['coverage_complete'])
        self.assertTrue(any(row['path'] == 'binary.dat' and row['reason'] == 'binary-or-non-UTF8'
                            for row in result['skipped_or_partial']))
        with patch.object(audit, 'MAX_BLOB_BYTES', 4):
            limited = audit.scan(self.root, self.sha, PROFILE)
        self.assertTrue(any(row['reason'] == 'over-blob-limit'
                            for row in limited['skipped_or_partial']))

    def test_report_has_provenance_versions_and_limits(self):
        result = audit.scan(self.root, self.sha, PROFILE)
        for key in ('profile_sha256', 'rules_sha256', 'tool_sha256', 'captured_at', 'versions', 'limits'):
            self.assertIn(key, result)
        self.assertEqual(result['schema_version'], 2)
        self.assertIn('max_scan_seconds', result['limits'])
        self.assertIn('findings', result)

    def test_comparison_is_review_only(self):
        before = audit.scan(self.root, self.sha, PROFILE)
        (self.root / 'config.txt').write_text('Another-Org/corelink-server\n', encoding='utf-8')
        self.git('add', 'config.txt')
        self.git('commit', '-qm', 'changed source')
        after = audit.scan(self.root, 'HEAD', PROFILE)
        delta = audit.compare(before, after)
        self.assertEqual(delta['status'], 'REVIEW_REQUIRED')
        self.assertEqual(delta['changed'], ['config.txt'])

    def test_bad_revision_fails_closed(self):
        with self.assertRaises(ValueError):
            audit.scan(self.root, '--all', PROFILE)

    def test_tree_entry_limit_fails_closed_without_complete_listing(self):
        with patch.object(audit, 'MAX_TREE_ENTRIES', 1), self.assertRaisesRegex(ValueError, 'tree entry limit'):
            audit.scan(self.root, self.sha, PROFILE)

    def test_tree_listing_byte_limit_fails_closed_while_streaming(self):
        with patch.object(audit, 'MAX_TREE_LISTING_BYTES', 1), \
                self.assertRaisesRegex(ValueError, 'tree listing byte limit'):
            audit.scan(self.root, self.sha, PROFILE)

    def test_scan_deadline_includes_resolution_and_tree_listing(self):
        def delayed_resolve(*_args):
            time.sleep(0.02)
            return self.sha

        with patch.object(audit, 'MAX_SCAN_SECONDS', 0.01), \
                patch.object(audit, 'resolve', side_effect=delayed_resolve), \
                self.assertRaisesRegex(ValueError, 'scan time limit exceeded'):
            audit.scan(self.root, self.sha, PROFILE)
        with self.assertRaisesRegex(ValueError, 'scan time limit exceeded during tree listing'):
            list(audit._tree_entries(self.root, self.sha, time.monotonic() - 1))


class CanonicalKitContractTests(unittest.TestCase):
    DOC = ROOT / 'docs/internal/org-migration'
    SKILL = ROOT / '.claude/skills/corelink-org-migration/SKILL.md'
    REQUIRED_DOCS = (
        'README.md', 'MAP.md', 'DESIGN.md', 'RUNBOOK.md', 'OPERATION-CONTRACT.md',
        'WORKPACKAGES.md', 'SOURCES.md', 'ADVERSARIAL-REVIEW.md', 'REVIEW.md',
        'INTEGRATION.md', 'topology.example.json', 'gate-ledger.template.json',
        'source-inventory.json',
    )

    def test_exact_canonical_deliverables_exist_and_legacy_paths_are_removed(self):
        for name in self.REQUIRED_DOCS:
            with self.subTest(path=name):
                self.assertTrue((self.DOC / name).is_file())
        self.assertTrue(self.SKILL.is_file())
        self.assertTrue((ROOT / 'scripts/org_migration_audit.py').is_file())
        self.assertTrue((ROOT / 'scripts/test_org_migration_audit.py').is_file())
        self.assertTrue((ROOT / 'scripts/org_migration_gate_check.py').is_file())
        self.assertTrue((ROOT / 'scripts/test_org_migration_gate_check.py').is_file())
        for stale in (
            ROOT / '.claude/skills/migrate-server-organization',
            ROOT / 'docs/operations/repository-org-migration',
            ROOT / 'scripts/repository_org_migration.py',
            ROOT / 'scripts/repository_org_snapshot.py',
        ):
            with self.subTest(stale=stale):
                self.assertFalse(stale.exists())

    def test_local_links_resolve_inside_repository(self):
        docs = [self.DOC / name for name in self.REQUIRED_DOCS if name.endswith('.md')]
        for source in [*docs, self.SKILL]:
            for raw in re.findall(r'\[[^\]\n]*\]\(([^)\s]+)\)', source.read_text(encoding='utf-8')):
                link = urlsplit(raw)
                if link.scheme or link.netloc:
                    continue
                target = (source.parent / unquote(link.path)).resolve() if link.path else source
                with self.subTest(source=source.name, link=raw):
                    self.assertTrue(target.is_relative_to(ROOT))
                    self.assertTrue(target.is_file(), f'missing {target}')

    def test_document_size_and_gate_work_package_contracts(self):
        readme = (self.DOC / 'README.md').read_text(encoding='utf-8')
        runbook = (self.DOC / 'RUNBOOK.md').read_text(encoding='utf-8')
        contract = (self.DOC / 'OPERATION-CONTRACT.md').read_text(encoding='utf-8')
        workpackages = (self.DOC / 'WORKPACKAGES.md').read_text(encoding='utf-8')
        adversarial = (self.DOC / 'ADVERSARIAL-REVIEW.md').read_text(encoding='utf-8')
        self.assertLessEqual(len(readme.splitlines()), 140)
        self.assertLessEqual(len(runbook.splitlines()), 420)
        self.assertLessEqual(len(contract.splitlines()), 450)
        self.assertLessEqual(len(self.SKILL.read_text(encoding='utf-8').splitlines()), 180)
        self.assertLessEqual(len(adversarial.split()), 2500)
        self.assertEqual(re.findall(r'(?m)^\| (G\d{2}) \|', contract), list(audit.GATES))
        for number in range(8):
            self.assertIn(f'WP-{number:02d}', workpackages)
        for criterion in ('Completude', 'Sucesso', 'Padrão de qualidade',
                          'Definition of Done', 'Invariantes'):
            self.assertIn(criterion, workpackages)

    def test_backlog_remains_open_and_ci_triggers_cover_canonical_docs(self):
        backlog = (ROOT / 'BACKLOG.md').read_text(encoding='utf-8')
        self.assertEqual(len(re.findall(r'(?m)^### B-374\b', backlog)), 1)
        section = backlog.split('### B-374', 1)[1].split('\n### ', 1)[0]
        self.assertIn('https://github.com/HuGR-Labs/corelink-server/issues/1702', section)
        self.assertIn('status: open', section)
        self.assertIn('test_org_migration_audit.py', section)
        self.assertIn('test_org_migration_gate_check.py', section)
        self.assertIn('não conclui nem fecha a migração', section)
        workflow = (ROOT / '.github/workflows/python-tests.yml').read_text(encoding='utf-8')
        self.assertEqual(workflow.count("- 'docs/internal/org-migration/**'"), 2)
        self.assertEqual(workflow.count("- '.claude/skills/corelink-org-migration/**'"), 2)
        for suite in ('scripts/test_org_migration_audit.py',
                      'scripts/test_org_migration_gate_check.py'):
            with self.subTest(suite=suite):
                self.assertEqual(workflow.count(f'            {suite}'), 1)
        self.assertIn('python3 -m pytest "${FILES[@]}" "${REQUIRED_SUITES[@]}" -q', workflow)

    def test_live_control_surface_matrix_is_explicit_and_unknown_by_default(self):
        for name in ('MAP.md', 'RUNBOOK.md'):
            doc = (self.DOC / name).read_text(encoding='utf-8')
            for surface in ('CODEOWNERS', 'base permission', 'SSO', 'Pages', 'cron',
                            'launchd', 'Cloudflare', 'Clerk', 'Stripe', 'OIDC', 'SLSA'):
                with self.subTest(doc=name, surface=surface):
                    self.assertIn(surface.casefold(), doc.casefold())
            self.assertIn('`UNKNOWN`', doc)
            self.assertIn('`TBD`', doc)
            self.assertIn('`null`', doc)
            self.assertTrue('git tree' in doc.casefold() or 'git-tree-only' in doc.casefold())

    def test_g19_closeout_documents_access_cleanup_and_bridge_readback(self):
        contract = (self.DOC / 'OPERATION-CONTRACT.md').read_text(encoding='utf-8')
        runbook = (self.DOC / 'RUNBOOK.md').read_text(encoding='utf-8')
        for doc in (contract, runbook):
            self.assertIn('source bridge', doc.casefold())
            self.assertIn('OBSERVED_ABSENT', doc)
            self.assertIn('negat', doc.casefold())
            self.assertIn('expiry', doc.casefold())
            self.assertIn('secret', doc.casefold())

    def test_source_inventory_is_versioned_audit_report_not_live_certification(self):
        report = json.loads((self.DOC / 'source-inventory.json').read_text(encoding='utf-8'))
        self.assertEqual(report['schema_version'], 2)
        self.assertEqual(report['source_sha'], PROFILE['source']['baseline_sha'])
        self.assertEqual(report['tool_sha256'], hashlib.sha256(
            (ROOT / 'scripts/org_migration_audit.py').read_bytes()).hexdigest())
        self.assertIn('skipped_or_partial', report)
        self.assertFalse(report.get('migration_ready', False))


if __name__ == '__main__':
    unittest.main()
