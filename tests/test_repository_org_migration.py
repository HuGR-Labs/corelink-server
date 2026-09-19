"""Offline positive/negative controls for the server migration discovery kit."""
from __future__ import annotations

import contextlib
import copy
import importlib.util
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import repository_org_migration as migration
import repository_org_snapshot as snapshot

MANIFEST = json.loads(migration.DEFAULT_MANIFEST.read_text())


class ManifestTests(unittest.TestCase):
    def test_real_manifest_is_valid(self):
        migration.validate_manifest(MANIFEST)

    def test_target_plan_is_not_authorization(self):
        result = migration.plan(MANIFEST, 'Example-Destination', 987654321)
        self.assertEqual(result['status'], 'PLANNING_ONLY_NOT_AUTHORIZED')
        self.assertTrue(all(g['status'] == 'UNVERIFIED' for g in result['gates']))
        self.assertEqual(result['unchanged_dependencies'], MANIFEST['dependencies'])
        self.assertIn('@1232040291', result['predicted_immutable_sub_prefix'])
        self.assertNotIn('transfer_command', result)

    def test_bad_target_names_rejected(self):
        for value in ['', '-org', 'org-', 'two--parts', 'a/b', '$(cmd)', 'a;echo x', 'a\norg', 'á', 'x' * 40, None]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                migration.plan(MANIFEST, value, 987654321)

    def test_bad_target_ids_rejected(self):
        for value in [0, -1, True, '123', 1.5, None, 311862110]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                migration.plan(MANIFEST, 'Another-Org', value)

    def test_case_only_move_rejected(self):
        with self.assertRaises(ValueError):
            migration.plan(MANIFEST, 'hugr-labs', 987654321)

    def test_scope_and_visibility_mutations_rejected(self):
        mutations = [('transfer_scope', ['server', 'runners']), ('preserve_visibility', 'public'),
                     ('allow_repository_rename', True), ('schema_version', 9)]
        for key, value in mutations:
            data = copy.deepcopy(MANIFEST)
            data[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                migration.validate_manifest(data)

    def test_wrong_server_identity_rejected(self):
        for key, value in [('repository_id', 1266754321), ('name', 'corelink-runners'), ('baseline_sha', 'main')]:
            data = copy.deepcopy(MANIFEST)
            data['source'][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                migration.validate_manifest(data)

    def test_dependencies_cannot_silently_join_transfer(self):
        for role in MANIFEST['dependencies']:
            data = copy.deepcopy(MANIFEST)
            data['dependencies'][role]['transfer_in_this_campaign'] = True
            with self.subTest(role=role), self.assertRaises(ValueError):
                migration.validate_manifest(data)

    def test_dependency_owners_are_independent(self):
        data = copy.deepcopy(MANIFEST)
        data['dependencies']['runners']['owner'] = 'Already-Moved-Runners'
        result = migration.plan(data, 'Only-Server-Target', 987654321)
        self.assertEqual(result['unchanged_dependencies']['runners']['owner'], 'Already-Moved-Runners')
        self.assertEqual(result['unchanged_dependencies']['cli_distribution']['owner'], 'HuGR-Labs')

    def test_no_execution_subcommand(self):
        for command in ['apply', 'transfer', 'dispatch', 'rewrite']:
            with self.subTest(command=command), contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                migration.main([command])

    def test_plan_does_not_run_processes(self):
        with patch.object(subprocess, 'run', side_effect=AssertionError('network/process not permitted')):
            migration.plan(MANIFEST, 'Target-Org', 987654321)


class GitInventoryTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.cmd('init', '-q')
        self.cmd('config', 'user.name', 'Migration Fixture')
        self.cmd('config', 'user.email', 'fixture@example.invalid')
        self.cmd('config', 'core.hooksPath', '/dev/null')
        self.cmd('config', 'commit.gpgsign', 'false')
        (self.root / 'config.txt').write_text('HuGR-Labs/corelink-server\nHumanGuardrail\n')
        (self.root / 'binary.dat').write_bytes(b'\x00HuGR-Labs')
        (self.root / 'historic.json').write_text('{"signer":"HumanGuardrail/corelink-server"}\n')
        (self.root / 'outside-link').symlink_to('/does/not/exist')
        self.cmd('add', '.')
        self.cmd('commit', '-qm', 'fixture')
        self.sha = self.cmd('rev-parse', 'HEAD').strip()

    def tearDown(self):
        self.tmp.cleanup()

    def cmd(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args], text=True)

    def test_committed_tree_not_dirty_or_untracked(self):
        (self.root / 'config.txt').write_text('dirty-only-value\n')
        (self.root / 'untracked-secret').write_text('HuGR-Labs\n')
        before = self.cmd('status', '--porcelain')
        result = migration.scan(self.root, self.sha, MANIFEST)
        self.assertEqual(result['coverage']['tree_entries'], 4)
        self.assertEqual(result['coverage']['symlink_blobs_not_followed'], 1)
        self.assertEqual(result['source_sha'], self.sha)
        self.assertTrue(any(x['path'] == 'config.txt' for x in result['matches']))
        self.assertFalse(any(x['path'] == 'untracked-secret' for x in result['files']))
        self.assertEqual(before, self.cmd('status', '--porcelain'))

    def test_binary_coverage_is_explicit(self):
        result = migration.scan(self.root, self.sha, MANIFEST)
        self.assertTrue(any(x['path'] == 'binary.dat' and x['reason'] == 'binary-or-non-UTF8' for x in result['skipped_or_partial']))
        self.assertFalse(any(x['path'] == 'binary.dat' for x in result['matches']))

    def test_matches_never_export_raw_line_values(self):
        result = migration.scan(self.root, self.sha, MANIFEST)
        self.assertTrue(result['matches'])
        self.assertEqual(set(result['matches'][0]), {'path', 'line', 'terms', 'class', 'line_sha256'})
        self.assertNotIn('{"signer"', json.dumps(result))

    def test_reproducible_inventory(self):
        self.assertEqual(migration.scan(self.root, self.sha, MANIFEST), migration.scan(self.root, self.sha, MANIFEST))

    def test_large_blob_skip_explicit(self):
        with patch.object(migration, 'MAX_BLOB_BYTES', 4):
            result = migration.scan(self.root, self.sha, MANIFEST)
        self.assertEqual(len(result['skipped_or_partial']), 4)
        self.assertTrue(all(x['reason'] == 'over-16MiB' for x in result['skipped_or_partial']))

    def test_lfs_pointer_is_not_content_coverage(self):
        (self.root / 'large.bin').write_text('version https://git-lfs.github.com/spec/v1\noid sha256:' + '0' * 64 + '\nsize 2000\n')
        self.cmd('add', 'large.bin')
        self.cmd('commit', '-qm', 'LFS fixture')
        result = migration.scan(self.root, 'HEAD', MANIFEST)
        self.assertTrue(any(x['reason'] == 'LFS-pointer-only' for x in result['skipped_or_partial']))

    def test_source_changes_are_reported_not_approved(self):
        before = migration.scan(self.root, self.sha, MANIFEST)
        (self.root / 'config.txt').write_text('Another-Org/corelink-server\n')
        self.cmd('add', 'config.txt')
        self.cmd('commit', '-qm', 'change')
        after = migration.scan(self.root, 'HEAD', MANIFEST)
        delta = migration.compare(before, after)
        self.assertEqual(delta['changed'], ['config.txt'])
        self.assertEqual(delta['status'], 'REVIEW_REQUIRED')

    def test_invalid_revision_fails(self):
        with self.assertRaises(ValueError):
            migration.scan(self.root, '--all', MANIFEST)


class SnapshotTests(unittest.TestCase):
    def test_read_is_get_and_fixed_host(self):
        response = subprocess.CompletedProcess([], 0, '[{"total_count":1,"variables":[{"name":"CF_ID","value":"never-export-this"}]}]', '')
        with patch.object(subprocess, 'run', return_value=response) as run:
            out = snapshot.query('repos/Org/corelink-server/actions/variables', snapshot.items('variables', ('name',)))
        command = run.call_args.args[0]
        self.assertEqual(command[:7], ['gh', 'api', '--hostname', 'github.com', '--method', 'GET', 'repos/Org/corelink-server/actions/variables'])
        self.assertNotIn('never-export-this', json.dumps(out))

    def test_403_404_not_empty_success(self):
        for code in [403, 404, 409, 500]:
            response = subprocess.CompletedProcess([], 1, '{}', f'error HTTP {code}; sensitive-error-content')
            with self.subTest(code=code), patch.object(subprocess, 'run', return_value=response):
                out = snapshot.query('endpoint', lambda x: x)
            self.assertEqual(out['status'], 'UNVERIFIED')
            self.assertEqual(out['http_status'], code)
            self.assertNotIn('pages', out)
            self.assertNotIn('sensitive-error-content', json.dumps(out))

    def test_timeout_remains_unverified(self):
        with patch.object(subprocess, 'run', side_effect=subprocess.TimeoutExpired('gh', 45)):
            self.assertEqual(snapshot.query('endpoint', lambda x: x)['status'], 'UNVERIFIED')

    def test_malformed_page_not_success(self):
        response = subprocess.CompletedProcess([], 0, '[{}]', '')
        with patch.object(subprocess, 'run', return_value=response):
            self.assertEqual(snapshot.query('endpoint', snapshot.items('secrets', ('name',)))['status'], 'UNVERIFIED')

    def test_alias_or_wrong_id_stops_before_org_queries(self):
        for repo, repo_id in [('Other/corelink-server', 1232040291), ('HuGR-Labs/corelink-server', 777)]:
            result = {'status': 'OBSERVED', 'pages': [{'full_name': repo, 'id': repo_id, 'owner': {'type': 'Organization'}}]}
            with self.subTest(repo=repo), patch.object(snapshot, 'query', return_value=result) as query:
                out = snapshot.collect('HuGR-Labs')
            self.assertEqual(query.call_count, 1)
            self.assertEqual(out['status'], 'STOP_IDENTITY_UNVERIFIED_OR_ALIAS')

    def test_invalid_owner_rejected_before_network(self):
        with patch.object(snapshot, 'query', side_effect=AssertionError('must not read')):
            with self.assertRaises(ValueError):
                snapshot.collect('org/../../other')

    def test_webhook_and_key_fields_not_exported(self):
        projector = snapshot.list_rows(('id', 'title', 'read_only'))
        result = projector([{'id': 1, 'title': 'key', 'read_only': True, 'key': 'private-fixture'}])
        self.assertNotIn('private-fixture', json.dumps(result))

    def test_pagination_pages_are_retained(self):
        response = subprocess.CompletedProcess([], 0, '[{"total_count":2,"secrets":[{"name":"A"}]},{"total_count":2,"secrets":[{"name":"B"}]}]', '')
        with patch.object(subprocess, 'run', return_value=response):
            out = snapshot.query('endpoint', snapshot.items('secrets', ('name',)))
        self.assertEqual(len(out['pages']), 2)


if __name__ == '__main__':
    unittest.main()
