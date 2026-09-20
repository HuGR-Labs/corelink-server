"""Focal fail-closed tests for the schema-2 migration ledger checker."""
from __future__ import annotations

import copy
import json
import subprocess
import sys
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import org_migration_gate_check as checker

TEMPLATE_PATH = ROOT / 'docs/internal/org-migration/gate-ledger.template.json'
TEMPLATE = json.loads(TEMPLATE_PATH.read_text(encoding='utf-8'))


def stamp(value: datetime) -> str:
    return value.astimezone(timezone.utc).isoformat(timespec='seconds').replace('+00:00', 'Z')


def complete_ledger(now: datetime) -> dict:
    data = copy.deepcopy(TEMPLATE)
    data['scope']['destination'] = {'owner': 'Example-Target', 'owner_id': 987654321}
    data['scope_sha256'] = checker._scope_hash(data['scope'])
    scope_hash = data['scope_sha256']
    digest = 'b' * 64
    valid_until = stamp(now + timedelta(hours=4))
    for index, gate in enumerate(data['gates']):
        observed = now - timedelta(minutes=40 - index * 2)
        gate.update({
            'status': 'PASS', 'owner': f'owner-{index}', 'reviewer': f'reviewer-{index}',
            'observed_at': stamp(observed), 'valid_until': valid_until,
            'reviewed_at': stamp(observed + timedelta(seconds=10)), 'reason': None,
            'evidence': [{'uri': f'private://evidence/{gate["id"]}', 'sha256': digest,
                          'scope_sha256': scope_hash, 'observed_at': stamp(observed),
                          'valid_until': valid_until}],
        })
    # G11 was observed 18 minutes ago. Issue GO afterward, then request once.
    issued = now - timedelta(minutes=17, seconds=30)
    data['go'] = {
        'status': 'GO', 'issued_at': stamp(issued), 'valid_until': valid_until,
        'scope_sha256': scope_hash, 'evidence_sha256': digest,
        'window_start': stamp(issued), 'window_end': stamp(now + timedelta(minutes=30)),
        'authority_reference': 'private://approval/change-123',
        'candidate_sha': 'a' * 40, 'profile_sha256': 'd' * 64,
        'inventory_sha256': digest, 'refs_manifest_sha256': 'e' * 64,
        'controls_manifest_sha256': 'f' * 64, 'peers_manifest_sha256': '9' * 64,
        'freeze_sha': 'a' * 40, 'operator': 'owner-12', 'reviewer': 'reviewer-12',
        'impact_budget_reference': 'private://budget/change-123',
    }
    data['transfer_requested_at'] = stamp(now - timedelta(minutes=15))
    data['inventory'] = {'schema_version': 2,
                         'source_sha': 'a' * 40, 'report_sha256': digest,
                         'profile_sha256': 'd' * 64,
                         'unresolved_findings': 0, 'opaque_dispositions_complete': True,
                         'coverage_limit_acknowledged': True}
    data['readback'] = {'status': 'OBSERVED', 'repository_id': 1232040291,
                        'owner': 'Example-Target', 'owner_id': 987654321,
                        'name': 'corelink-server', 'visibility': 'private',
                        'canonical_path': 'Example-Target/corelink-server',
                        'observed_at': stamp(now - timedelta(minutes=14, seconds=30))}
    data['write_fences'] = {
        'source': 'FROZEN_READ_ONLY', 'destination': 'PREPARATION_ONLY',
        'writers_drained': True, 'remaining_writers': [], 'freeze_sha': 'a' * 40,
        'reconciliation': {'equal': True},
    }
    data['backup_restore'] = {
        'freeze_sha': 'a' * 40, 'backup_sha': 'a' * 40, 'restored_sha': 'a' * 40,
        'refs_manifest_sha256': digest, 'assets_manifest_sha256': digest,
        'lfs_manifest_sha256': digest, 'parity_verified': True, 'restore_tested': True,
        'evidence_reference': 'private://backup-restore/evidence-reference',
    }
    data['privacy'] = {'classification': 'PRIVATE', 'raw_credentials_exported': False,
                       'tenant_data_exported': False}
    data['rollback'] = {'forward_recovery_plan': 'restore forward from verified freeze pack',
                        'independent_recovery_access': True, 'transfer_back_assumed': False,
                        'evidence_reference': 'private://recovery/drill'}
    data['credential_rotation'] = {
        'planned': True, 'old_credentials_revoked': True, 'new_consumers_verified': True,
        'values_exported': False, 'evidence_reference': 'private://credentials/consumer-proof',
    }
    data['split_brain'] = {'source_writers_disabled': True, 'destination_single_writer': True,
                           'active_writer_count': 1, 'reconciliation_after_cutover': True}
    data['access_cleanup'] = {
        'inventory_complete': True, 'temporary_grants_reconciled': True,
        'inventory_evidence_reference': 'private://access-cleanup/inventory',
        'grants': [],
        'source_bridge': {
            'removed': True, 'readback_status': 'OBSERVED_ABSENT',
            'repository_id': 1232040291, 'source_owner_id': 311862110,
            'destination_owner_id': 987654321,
            'observed_at': stamp(now - timedelta(minutes=5)),
            'evidence_reference': 'private://access-cleanup/source-bridge-readback',
        },
    }
    return data


class TemplateTests(unittest.TestCase):
    def test_template_has_twenty_unknown_gates_and_fails_closed(self):
        result = checker.check_ledger(TEMPLATE)
        self.assertEqual([gate['id'] for gate in TEMPLATE['gates']], list(checker.GATES))
        self.assertTrue(all(gate['status'] == 'UNKNOWN' for gate in TEMPLATE['gates']))
        self.assertFalse(TEMPLATE['migration_ready'])
        self.assertFalse(TEMPLATE['authorization_verified'])
        self.assertIsNone(TEMPLATE['scope']['destination'])
        self.assertFalse(result['record_consistent'])
        self.assertEqual(len(result['blockers']), 20)
        self.assertEqual(result['errors'], [])
        self.assertFalse(result['migration_ready'])
        self.assertFalse(result['authorization_verified'])

    def test_schema_and_allowlists_are_fixed(self):
        self.assertEqual(TEMPLATE['schema_version'], 2)
        self.assertEqual(TEMPLATE['mutation_policy']['preparation_allowlist'],
                         list(checker.PREPARATION_ALLOWLIST))
        self.assertEqual(TEMPLATE['mutation_policy']['forbidden'],
                         list(checker.FORBIDDEN_MUTATIONS))

    def test_default_cli_exits_blocked_and_never_reports_authorized(self):
        result = subprocess.run([sys.executable, str(ROOT / 'scripts/org_migration_gate_check.py'),
                                 str(TEMPLATE_PATH)], capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 2)
        report = json.loads(result.stdout)
        self.assertFalse(report['migration_ready'])
        self.assertFalse(report['authorization_verified'])
        self.assertFalse(report['record_consistent'])


class LedgerValidationTests(unittest.TestCase):
    def setUp(self):
        self.now = datetime(2026, 9, 19, 15, 0, tzinfo=timezone.utc)
        self.ledger = complete_ledger(self.now)

    def test_consistency_never_means_readiness_or_authorization(self):
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(result['record_consistent'], result['errors'])
        self.assertEqual(result['blockers'], [])
        self.assertFalse(result['migration_ready'])
        self.assertFalse(result['authorization_verified'])

    def test_unknown_blocks_even_when_other_gates_pass(self):
        self.ledger['gates'][4]['status'] = 'UNKNOWN'
        result = checker.check_ledger(self.ledger, self.now)
        self.assertFalse(result['record_consistent'])
        self.assertIn('G04 status is UNKNOWN', result['blockers'])

    def test_inventory_schema_and_unresolved_findings_block_g01(self):
        self.ledger['inventory']['schema_version'] = 1
        self.ledger['inventory']['unresolved_findings'] = 1
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('inventory schema_version must be 2' in error for error in result['errors']))
        self.assertTrue(any('unresolved operational findings' in error for error in result['errors']))

    def test_g13_requires_matching_canonical_identity_readback(self):
        self.ledger['readback']['repository_id'] = 987654321
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('readback identity must match' in error for error in result['errors']))

    def test_wrong_gate_order_and_duplicate_gate_fail(self):
        self.ledger['gates'][0], self.ledger['gates'][1] = self.ledger['gates'][1], self.ledger['gates'][0]
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('exactly G00 through G19 in order' in error for error in result['errors']))

    def test_missing_owner_same_reviewer_and_invalid_evidence_fail(self):
        self.ledger['gates'][2]['owner'] = None
        self.ledger['gates'][3]['reviewer'] = self.ledger['gates'][3]['owner']
        self.ledger['gates'][4]['evidence'][0]['scope_sha256'] = '0' * 64
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('G02: accountable owner required' in error for error in result['errors']))
        self.assertTrue(any('G03: distinct reviewer required' in error for error in result['errors']))
        self.assertTrue(any('G04.evidence[0]: evidence scope hash mismatch' in error
                            for error in result['errors']))

    def test_casefold_same_reviewer_is_rejected_once(self):
        gate = self.ledger['gates'][3]
        gate['reviewer'] = gate['owner'].upper()
        result = checker.check_ledger(self.ledger, self.now)
        self.assertEqual(result['errors'].count('G03: distinct reviewer required'), 1)

    def test_malformed_non_string_gate_status_returns_structured_invalid(self):
        for status in ([], {}, None, 7):
            case = copy.deepcopy(self.ledger)
            case['gates'][1]['status'] = status
            result = checker.check_ledger(case, self.now)
            with self.subTest(status=status):
                self.assertFalse(result['record_consistent'])
                self.assertIn('G01: invalid status', result['errors'])

    def test_validity_equal_to_now_is_expired(self):
        self.ledger['gates'][1]['valid_until'] = stamp(self.now)
        self.ledger['gates'][1]['evidence'][0]['valid_until'] = stamp(self.now)
        result = checker.check_ledger(self.ledger, self.now)
        self.assertIn('G01: evidence validity expired', result['errors'])
        self.assertIn('G01.evidence[0]: evidence is expired', result['errors'])

    def test_expired_and_future_evidence_fail(self):
        self.ledger['gates'][1]['evidence'][0]['valid_until'] = stamp(self.now - timedelta(seconds=1))
        self.ledger['gates'][2]['evidence'][0]['observed_at'] = stamp(self.now + timedelta(minutes=1))
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('G01.evidence[0]: evidence is expired' in error for error in result['errors']))
        self.assertTrue(any('G02.evidence[0]: evidence timestamp is in the future' in error
                            for error in result['errors']))

    def test_future_review_timestamp_fails(self):
        self.ledger['gates'][0]['reviewed_at'] = stamp(self.now + timedelta(minutes=1))
        result = checker.check_ledger(self.ledger, self.now)
        self.assertIn('G00: reviewer timestamp is in the future', result['errors'])

    def test_migration_ready_true_is_rejected(self):
        self.ledger['migration_ready'] = True
        result = checker.check_ledger(self.ledger, self.now)
        self.assertFalse(result['record_consistent'])
        self.assertIn('migration_ready must remain false; this tool never grants readiness', result['errors'])
        self.assertFalse(result['migration_ready'])

    def test_future_or_pre_freeze_go_is_rejected(self):
        future = copy.deepcopy(self.ledger)
        future['go']['window_start'] = stamp(self.now + timedelta(minutes=1))
        early = copy.deepcopy(self.ledger)
        early['go']['issued_at'] = stamp(self.now - timedelta(minutes=20))
        for case in (future, early):
            result = checker.check_ledger(case, self.now)
            self.assertFalse(result['record_consistent'])
        self.assertTrue(any('future cutover window cannot be pre-approved' in error
                            for error in checker.check_ledger(future, self.now)['errors']))
        self.assertTrue(any('GO must be issued strictly after final freeze/drain gate G11' in error
                            for error in checker.check_ledger(early, self.now)['errors']))

    def test_go_must_be_strictly_after_g11_and_identity_checks_are_casefolded(self):
        equal_boundary = copy.deepcopy(self.ledger)
        equal_boundary['go']['issued_at'] = equal_boundary['gates'][11]['observed_at']
        equal_boundary['go']['window_start'] = equal_boundary['go']['issued_at']
        result = checker.check_ledger(equal_boundary, self.now)
        self.assertTrue(any('GO must be issued strictly after' in error for error in result['errors']))

        case_variant = copy.deepcopy(self.ledger)
        case_variant['go']['operator'] = case_variant['go']['operator'].upper()
        case_variant['go']['reviewer'] = case_variant['go']['reviewer'].upper()
        self.assertTrue(checker.check_ledger(case_variant, self.now)['record_consistent'])

    def test_post_transfer_gates_cannot_pass_without_request(self):
        self.ledger['transfer_requested_at'] = None
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('G13–G19 cannot be approved before a transfer request/readback exists'
                            in error for error in result['errors']))

    def test_na_is_limited_to_proven_app_gates(self):
        gate = self.ledger['gates'][0]
        gate['status'] = 'N/A'
        gate['reason'] = 'Claimed absence'
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('G00: N/A is permitted only' in error for error in result['errors']))

    def test_mutation_allowlist_is_enforced(self):
        self.ledger['preparation_mutations'] = ['transfer_repository']
        self.ledger['forbidden_mutations_observed'] = ['transfer_repository']
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('outside the allowlist' in error for error in result['errors']))
        self.assertTrue(any('forbidden_mutations_observed must be empty' in error
                            for error in result['errors']))

    def test_freeze_drain_and_reconciliation_are_required(self):
        self.ledger['write_fences']['source'] = 'WRITABLE'
        self.ledger['write_fences']['remaining_writers'] = ['release job']
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('source write fence must be FROZEN_READ_ONLY' in error
                            for error in result['errors']))
        self.assertTrue(any('all writers must be drained' in error for error in result['errors']))

    def test_backup_parity_privacy_and_restore_are_required(self):
        self.ledger['backup_restore']['restored_sha'] = 'c' * 40
        self.ledger['privacy']['raw_credentials_exported'] = True
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('freeze, backup, and restored commit identities must match' in error
                            for error in result['errors']))
        self.assertTrue(any('private evidence with no credential values or tenant data required'
                            in error for error in result['errors']))

    def test_forward_recovery_and_scoped_credential_rotation_are_required(self):
        self.ledger['rollback']['transfer_back_assumed'] = True
        self.ledger['credential_rotation']['values_exported'] = True
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('transfer-back is never assumed' in error for error in result['errors']))
        self.assertTrue(any('values must not be exported' in error for error in result['errors']))

    def test_split_brain_blocks_acceptance(self):
        self.ledger['split_brain']['active_writer_count'] = 2
        self.ledger['split_brain']['source_writers_disabled'] = False
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('split-brain prevention' in error for error in result['errors']))

    def test_g19_requires_access_grant_reconciliation_and_bridge_readback(self):
        self.ledger['access_cleanup']['source_bridge']['removed'] = False
        self.ledger['access_cleanup']['grants'] = [{
            'principal_kind': 'token', 'principal_ref': 'release-token-name-only',
            'side': 'source', 'access': 'read', 'disposition': 'REVOKED',
            'owner': 'credential-owner', 'reason': 'temporary migration access',
            'expires_at': None, 'reviewer': 'credential-reviewer',
            'negative_probe': {'status': 'FAIL', 'observed_at': stamp(self.now),
                               'evidence_reference': 'private://probe/failed'},
            'evidence_reference': 'private://access/revoke',
        }]
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('revoked grant requires a passing negative access probe' in error
                            for error in result['errors']))
        self.assertTrue(any('source bridge must be removed and read back absent' in error
                            for error in result['errors']))

    def test_g19_accepts_reconciled_revoked_and_time_bounded_retained_readers(self):
        self.ledger['access_cleanup']['grants'] = [
            {
                'principal_kind': 'collaborator', 'principal_ref': 'user-id-456',
                'side': 'source', 'access': 'read', 'disposition': 'REVOKED',
                'owner': 'access-owner', 'reason': 'temporary migration access ended',
                'expires_at': None, 'reviewer': 'independent-reviewer',
                'negative_probe': {'status': 'PASS',
                                   'observed_at': stamp(self.now - timedelta(minutes=3)),
                                   'evidence_reference': 'private://access/negative-probe'},
                'evidence_reference': 'private://access/revocation',
            },
            {
                'principal_kind': 'team', 'principal_ref': 'closeout-readers',
                'side': 'destination', 'access': 'read', 'disposition': 'RETAINED',
                'owner': 'access-owner', 'reason': 'bounded closeout observation',
                'expires_at': stamp(self.now + timedelta(days=1)),
                'reviewer': 'independent-reviewer', 'negative_probe': None,
                'evidence_reference': 'private://access/retention',
            },
        ]
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(result['record_consistent'], result['errors'])

    def test_retained_readers_need_named_owner_reason_expiry_reviewer(self):
        self.ledger['access_cleanup']['grants'] = [{
            'principal_kind': 'team', 'principal_ref': 'migration-auditors',
            'side': 'destination', 'access': 'read', 'disposition': 'RETAINED',
            'owner': 'access-owner', 'reason': 'time-bounded closeout review',
            'expires_at': stamp(self.now + timedelta(days=2)), 'reviewer': 'ACCESS-OWNER',
            'negative_probe': None,
            'evidence_reference': 'private://access/retention',
        }]
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('distinct grant owner and reviewer required' in error
                            for error in result['errors']))

    def test_access_grant_schema_rejects_secret_values_and_expired_retention(self):
        grant = {
            'principal_kind': 'human', 'principal_ref': 'person-123',
            'side': 'source', 'access': 'read', 'disposition': 'RETAINED',
            'owner': 'access-owner', 'reason': 'approved closeout',
            'expires_at': stamp(self.now), 'reviewer': 'independent-reviewer',
            'negative_probe': None, 'evidence_reference': 'private://access/retention',
        }
        self.ledger['access_cleanup']['grants'] = [grant]
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('retained reader expiry must be in the future' in error
                            for error in result['errors']))

        grant['token_value'] = 'must-never-be-stored'
        result = checker.check_ledger(self.ledger, self.now)
        self.assertTrue(any('grant record has invalid shape; do not store secrets' in error
                            for error in result['errors']))

    def test_scope_and_visibility_are_immutable(self):
        self.ledger['scope']['visibility'] = 'public'
        self.ledger['scope_sha256'] = checker._scope_hash(self.ledger['scope'])
        for gate in self.ledger['gates']:
            for item in gate['evidence']:
                item['scope_sha256'] = self.ledger['scope_sha256']
        self.ledger['go']['scope_sha256'] = self.ledger['scope_sha256']
        result = checker.check_ledger(self.ledger, self.now)
        self.assertFalse(result['record_consistent'])
        self.assertIn('scope visibility must remain private', result['errors'])


if __name__ == '__main__':
    unittest.main()
