#!/usr/bin/env python3
"""Fail-closed structural check of a schema-2 migration gate ledger.

This checker reads local JSON only. A consistent record is not live evidence,
human authentication, transfer readiness, or authorization to mutate anything.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

GATES = tuple(f'G{number:02d}' for number in range(20))
STATUSES = {'UNKNOWN', 'PASS', 'FAIL', 'BLOCKED', 'N/A'}
HASH256 = re.compile(r'[0-9a-f]{64}\Z')
SHA1 = re.compile(r'[0-9a-f]{40}\Z')
OWNER = re.compile(r'[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?\Z')
PREPARATION_ALLOWLIST = (
    'create_documentation',
    'create_offline_audit_report',
    'create_isolated_test_fixture',
    'update_profile_on_reviewed_branch',
    'open_preparation_pull_request',
)
FORBIDDEN_MUTATIONS = (
    'transfer_repository', 'push_mirror', 'rewrite_history_or_signatures',
    'deploy_or_publish', 'dispatch_production_workflow', 'restart_runner_fleet',
    'change_cloud_resources_or_customer_data', 'change_visibility_or_repository_name',
    'transfer_peer_repository', 'broad_credential_rotation', 'git_clean_or_reset_hard',
)


def _parse_time(value: Any, field: str, errors: list[str]) -> datetime | None:
    if not isinstance(value, str) or not value:
        errors.append(f'{field}: UTC timestamp required')
        return None
    try:
        parsed = datetime.fromisoformat(value.replace('Z', '+00:00'))
    except ValueError:
        errors.append(f'{field}: invalid ISO-8601 timestamp')
        return None
    if parsed.tzinfo is None or parsed.utcoffset() is None:
        errors.append(f'{field}: timezone required')
        return None
    if parsed.utcoffset().total_seconds() != 0:
        errors.append(f'{field}: timestamp must be UTC')
        return None
    return parsed.astimezone(timezone.utc)


def _scope_hash(scope: Any) -> str:
    encoded = json.dumps(scope, sort_keys=True, separators=(',', ':'), ensure_ascii=True).encode()
    return hashlib.sha256(encoded).hexdigest()


def _identity(value: Any) -> str | None:
    return value.strip().casefold() if isinstance(value, str) and value.strip() else None


def _valid_evidence(entries: Any, gate_id: str, scope_hash: str,
                    now: datetime, errors: list[str]) -> tuple[datetime | None, datetime | None]:
    if not isinstance(entries, list) or not entries:
        errors.append(f'{gate_id}: PASS requires at least one evidence reference')
        return None, None
    newest: datetime | None = None
    earliest_expiry: datetime | None = None
    for index, item in enumerate(entries):
        label = f'{gate_id}.evidence[{index}]'
        if not isinstance(item, dict) or set(item) != {
                'uri', 'sha256', 'scope_sha256', 'observed_at', 'valid_until'}:
            errors.append(f'{label}: evidence reference has invalid shape')
            continue
        if not isinstance(item['uri'], str) or not item['uri'].strip():
            errors.append(f'{label}: evidence URI/reference required')
        if not isinstance(item['sha256'], str) or not HASH256.fullmatch(item['sha256']):
            errors.append(f'{label}: SHA-256 required')
        if item['scope_sha256'] != scope_hash:
            errors.append(f'{label}: evidence scope hash mismatch')
        observed = _parse_time(item['observed_at'], f'{label}.observed_at', errors)
        valid_until = _parse_time(item['valid_until'], f'{label}.valid_until', errors)
        if observed is not None and observed > now:
            errors.append(f'{label}: evidence timestamp is in the future')
        if observed is not None and valid_until is not None and valid_until <= observed:
            errors.append(f'{label}: validity must end after observation')
        if valid_until is not None and valid_until <= now:
            errors.append(f'{label}: evidence is expired')
        if observed is not None and (newest is None or observed > newest):
            newest = observed
        if valid_until is not None and (earliest_expiry is None or valid_until < earliest_expiry):
            earliest_expiry = valid_until
    return newest, earliest_expiry


def _validate_access_cleanup(cleanup: Any, scope: Any, now: datetime,
                             request_time: datetime | None, closeout_time: datetime | None,
                             errors: list[str]) -> None:
    required = {'inventory_complete', 'temporary_grants_reconciled',
                'inventory_evidence_reference', 'grants', 'source_bridge'}
    if not isinstance(cleanup, dict) or set(cleanup) != required:
        errors.append('G19: complete access_cleanup record required')
        return
    if cleanup.get('inventory_complete') is not True \
            or cleanup.get('temporary_grants_reconciled') is not True:
        errors.append('G19: temporary human/collaborator/team/app/token read grants must be reconciled')
    if not isinstance(cleanup.get('inventory_evidence_reference'), str) \
            or not cleanup.get('inventory_evidence_reference', '').strip():
        errors.append('G19: access inventory evidence reference required; secret values are forbidden')

    grants = cleanup.get('grants')
    if not isinstance(grants, list):
        errors.append('G19: access grants must be an array')
    else:
        for index, grant in enumerate(grants):
            label = f'G19.access_cleanup.grants[{index}]'
            fields = {'principal_kind', 'principal_ref', 'side', 'access', 'disposition',
                      'owner', 'reason', 'expires_at', 'reviewer', 'negative_probe',
                      'evidence_reference'}
            if not isinstance(grant, dict) or set(grant) != fields:
                errors.append(f'{label}: grant record has invalid shape; do not store secrets')
                continue
            if not isinstance(grant.get('principal_kind'), str) or grant['principal_kind'] not in (
                    'human', 'collaborator', 'team', 'app', 'token'):
                errors.append(f'{label}: principal_kind must identify a reader class')
            if not isinstance(grant.get('principal_ref'), str) or not grant.get('principal_ref', '').strip():
                errors.append(f'{label}: non-secret principal reference required')
            if grant.get('side') not in ('source', 'destination', 'both') or grant.get('access') != 'read':
                errors.append(f'{label}: source/destination read grant scope required')
            disposition = grant.get('disposition')
            if not isinstance(disposition, str) or disposition not in ('REVOKED', 'RETAINED'):
                errors.append(f'{label}: grant disposition must be REVOKED or RETAINED')
            owner = grant.get('owner')
            reviewer = grant.get('reviewer')
            if not isinstance(owner, str) or not owner.strip() or not isinstance(reviewer, str) \
                    or not reviewer.strip() or _identity(owner) == _identity(reviewer):
                errors.append(f'{label}: distinct grant owner and reviewer required')
            if not isinstance(grant.get('reason'), str) or not grant.get('reason', '').strip():
                errors.append(f'{label}: grant disposition reason required')
            if not isinstance(grant.get('evidence_reference'), str) \
                    or not grant.get('evidence_reference', '').strip():
                errors.append(f'{label}: grant evidence reference required; secret values are forbidden')
            if disposition == 'REVOKED':
                if grant.get('expires_at') is not None:
                    errors.append(f'{label}: revoked grant must not have a retained expiry')
                probe = grant.get('negative_probe')
                if not isinstance(probe, dict) or set(probe) != {'status', 'observed_at', 'evidence_reference'} \
                        or probe.get('status') != 'PASS' \
                        or not isinstance(probe.get('evidence_reference'), str) \
                        or not probe.get('evidence_reference', '').strip():
                    errors.append(f'{label}: revoked grant requires a passing negative access probe')
                else:
                    observed = _parse_time(probe.get('observed_at'), f'{label}.negative_probe.observed_at', errors)
                    if observed is not None and observed > now:
                        errors.append(f'{label}: negative probe timestamp is in the future')
                    if observed is not None and request_time is not None and observed <= request_time:
                        errors.append(f'{label}: negative probe must follow the transfer request')
                    if observed is not None and closeout_time is not None and observed > closeout_time:
                        errors.append(f'{label}: negative probe postdates G19 closeout observation')
            elif disposition == 'RETAINED':
                expires = _parse_time(grant.get('expires_at'), f'{label}.expires_at', errors)
                if expires is not None and expires <= now:
                    errors.append(f'{label}: retained reader expiry must be in the future')
                if grant.get('negative_probe') is not None:
                    errors.append(f'{label}: retained reader must not claim a revoked-access probe')

    bridge = cleanup.get('source_bridge')
    bridge_fields = {'removed', 'readback_status', 'repository_id', 'source_owner_id',
                     'destination_owner_id', 'observed_at', 'evidence_reference'}
    if not isinstance(bridge, dict) or set(bridge) != bridge_fields:
        errors.append('G19: source bridge removal/readback record required')
        return
    source = scope.get('source') if isinstance(scope, dict) else None
    destination = scope.get('destination') if isinstance(scope, dict) else None
    if bridge.get('removed') is not True or bridge.get('readback_status') != 'OBSERVED_ABSENT' \
            or bridge.get('repository_id') != 1232040291 \
            or not isinstance(source, dict) or bridge.get('source_owner_id') != source.get('owner_id') \
            or not isinstance(destination, dict) or bridge.get('destination_owner_id') != destination.get('owner_id'):
        errors.append('G19: source bridge must be removed and read back absent for the same repository')
    bridge_at = _parse_time(bridge.get('observed_at'), 'G19.source_bridge.observed_at', errors)
    if bridge_at is not None and bridge_at > now:
        errors.append('G19: source bridge readback timestamp is in the future')
    if bridge_at is not None and request_time is not None and bridge_at <= request_time:
        errors.append('G19: source bridge readback must follow the transfer request')
    if bridge_at is not None and closeout_time is not None and bridge_at > closeout_time:
        errors.append('G19: source bridge readback postdates G19 closeout observation')
    if not isinstance(bridge.get('evidence_reference'), str) or not bridge.get('evidence_reference', '').strip():
        errors.append('G19: source bridge readback evidence reference required')


def _gate_label(gate_id: str) -> str:
    return gate_id


def check_ledger(data: Any, now: datetime | None = None) -> dict[str, Any]:
    now = now or datetime.now(timezone.utc)
    errors: list[str] = []
    blockers: list[str] = []
    if not isinstance(data, dict):
        return {'schema_version': 2, 'record_consistent': False,
                'migration_ready': False, 'authorization_verified': False,
                'blockers': ['ledger must be a JSON object'], 'errors': []}
    allowed_top = {
        'schema_version', 'campaign_id', 'scope', 'scope_sha256',
        'migration_ready', 'authorization_verified', 'gates', 'go',
        'transfer_requested_at', 'mutation_policy', 'preparation_mutations',
        'forbidden_mutations_observed', 'write_fences', 'backup_restore',
        'privacy', 'rollback', 'credential_rotation', 'split_brain', 'inventory', 'readback',
        'access_cleanup',
    }
    extra = set(data) - allowed_top
    if extra:
        errors.append('unknown top-level fields: ' + ', '.join(sorted(extra)))
    if data.get('schema_version') != 2:
        errors.append('schema_version must be 2')
    if not isinstance(data.get('campaign_id'), str) or not data.get('campaign_id', '').strip():
        errors.append('campaign_id is required')
    if data.get('migration_ready') is not False:
        errors.append('migration_ready must remain false; this tool never grants readiness')
    if data.get('authorization_verified') is not False:
        errors.append('authorization_verified must remain false; this tool cannot authenticate GO')

    scope = data.get('scope')
    if not isinstance(scope, dict):
        errors.append('scope must be an object')
        scope_hash = ''
    else:
        required_scope = {'repository_id', 'repository_name', 'source', 'destination',
                          'visibility', 'transfer_scope'}
        if set(scope) != required_scope:
            errors.append('scope fields must exactly identify repo, source, destination, visibility, and transfer_scope')
        if type(scope.get('repository_id')) is not int or scope.get('repository_id') != 1232040291:
            errors.append('scope repository_id must be 1232040291')
        if scope.get('repository_name') != 'corelink-server':
            errors.append('scope repository_name must be corelink-server')
        if scope.get('visibility') != 'private':
            errors.append('scope visibility must remain private')
        if scope.get('transfer_scope') != ['server']:
            errors.append('transfer_scope must contain server only')
        for role in ('source', 'destination'):
            value = scope.get(role)
            if value is not None and (not isinstance(value, dict)
                                      or not isinstance(value.get('owner'), str)
                                      or not OWNER.fullmatch(value.get('owner', ''))
                                      or type(value.get('owner_id')) is not int
                                      or value.get('owner_id', 0) <= 0):
                errors.append(f'scope.{role} identity is malformed')
        scope_hash = _scope_hash(scope)
    if data.get('scope_sha256') != scope_hash or not HASH256.fullmatch(str(data.get('scope_sha256', ''))):
        errors.append('scope_sha256 does not match canonical scope')

    policy = data.get('mutation_policy')
    if not isinstance(policy, dict) or set(policy) != {'preparation_allowlist', 'forbidden'}:
        errors.append('mutation_policy must name the fixed preparation allowlist and forbidden set')
    else:
        if policy.get('preparation_allowlist') != list(PREPARATION_ALLOWLIST):
            errors.append('preparation allowlist differs from the fixed contract')
        if policy.get('forbidden') != list(FORBIDDEN_MUTATIONS):
            errors.append('forbidden mutation set differs from the fixed contract')
    observed_actions = data.get('preparation_mutations')
    if not isinstance(observed_actions, list):
        errors.append('preparation_mutations must be an array')
    elif any(action not in PREPARATION_ALLOWLIST for action in observed_actions):
        errors.append('preparation_mutations includes an action outside the allowlist')
    forbidden_seen = data.get('forbidden_mutations_observed')
    if forbidden_seen != []:
        errors.append('forbidden_mutations_observed must be empty')

    gates = data.get('gates')
    if not isinstance(gates, list) or [g.get('id') if isinstance(g, dict) else None for g in gates] != list(GATES):
        errors.append('gates must contain exactly G00 through G19 in order')
        gates = []
    gate_times: dict[str, datetime] = {}
    statuses: dict[str, str] = {}
    for gate in gates:
        gate_id = gate['id']
        fields = {'id', 'status', 'owner', 'evidence', 'observed_at', 'valid_until',
                  'reviewer', 'reviewed_at', 'reason'}
        if set(gate) != fields:
            errors.append(f'{gate_id}: gate fields do not match schema 2')
        status = gate.get('status')
        statuses[gate_id] = status if isinstance(status, str) else ''
        if not isinstance(status, str) or status not in STATUSES:
            errors.append(f'{gate_id}: invalid status')
            continue
        if status in {'UNKNOWN', 'FAIL', 'BLOCKED'}:
            blockers.append(f'{gate_id} status is {status}')
            continue
        if gate_id == 'G00' and status == 'PASS' and isinstance(scope, dict):
            source = scope.get('source')
            destination = scope.get('destination')
            if not isinstance(source, dict) or not isinstance(destination, dict):
                errors.append('G00: verified source and destination identities are required')
            elif source.get('owner_id') == destination.get('owner_id') \
                    or _identity(source.get('owner')) == _identity(destination.get('owner')):
                errors.append('G00: source and destination must be distinct organizations')
        if status == 'N/A' and gate_id not in {'G07', 'G17'}:
            errors.append(f'{gate_id}: N/A is permitted only for proven App gates G07/G17')
        owner = gate.get('owner')
        reviewer = gate.get('reviewer')
        if not isinstance(owner, str) or not owner.strip():
            errors.append(f'{gate_id}: accountable owner required')
        if not isinstance(reviewer, str) or not reviewer.strip() \
                or _identity(reviewer) == _identity(owner):
            errors.append(f'{gate_id}: distinct reviewer required')
        observed = _parse_time(gate.get('observed_at'), f'{gate_id}.observed_at', errors)
        valid_until = _parse_time(gate.get('valid_until'), f'{gate_id}.valid_until', errors)
        reviewed_at = _parse_time(gate.get('reviewed_at'), f'{gate_id}.reviewed_at', errors)
        if observed is not None:
            gate_times[gate_id] = observed
            if observed > now:
                errors.append(f'{gate_id}: observation is in the future')
        if valid_until is not None and valid_until <= now:
            errors.append(f'{gate_id}: evidence validity expired')
        if observed is not None and valid_until is not None and valid_until <= observed:
            errors.append(f'{gate_id}: validity must end after observation')
        if reviewed_at is not None and observed is not None and reviewed_at < observed:
            errors.append(f'{gate_id}: reviewer timestamp predates evidence')
        if reviewed_at is not None and reviewed_at > now:
            errors.append(f'{gate_id}: reviewer timestamp is in the future')
        evidence_observed, evidence_expiry = _valid_evidence(
            gate.get('evidence'), gate_id, scope_hash, now, errors)
        if observed is not None and evidence_observed is not None and observed < evidence_observed:
            errors.append(f'{gate_id}: gate observation predates its evidence')
        if valid_until is not None and evidence_expiry is not None and valid_until > evidence_expiry:
            errors.append(f'{gate_id}: gate validity extends beyond its evidence')
        if status == 'N/A' and (not isinstance(gate.get('reason'), str) or not gate.get('reason', '').strip()):
            errors.append(f'{gate_id}: N/A requires an applicability reason')
        if status in {'PASS', 'N/A'} and (observed is None or valid_until is None or reviewed_at is None):
            errors.append(f'{gate_id}: accepted gate requires observed_at, valid_until, and reviewed_at')

    accepted = {'PASS', 'N/A'}
    if statuses.get('G01') == 'PASS':
        inventory = data.get('inventory')
        required_inventory = {'schema_version', 'source_sha', 'report_sha256', 'profile_sha256',
                              'unresolved_findings', 'opaque_dispositions_complete',
                              'coverage_limit_acknowledged'}
        if not isinstance(inventory, dict) or set(inventory) != required_inventory:
            errors.append('G01: schema-2 candidate inventory record required')
        else:
            if inventory.get('schema_version') != 2:
                errors.append('G01: inventory schema_version must be 2')
            if not isinstance(inventory.get('source_sha'), str) or not SHA1.fullmatch(inventory.get('source_sha', '')):
                errors.append('G01: candidate source SHA required')
            if not isinstance(inventory.get('report_sha256'), str) or not HASH256.fullmatch(inventory.get('report_sha256', '')):
                errors.append('G01: inventory report SHA-256 required')
            if type(inventory.get('unresolved_findings')) is not int or inventory.get('unresolved_findings') != 0:
                errors.append('G01: unresolved operational findings block the inventory gate')
            if inventory.get('opaque_dispositions_complete') is not True \
                    or inventory.get('coverage_limit_acknowledged') is not True:
                errors.append('G01: opaque objects and coverage limits need reviewed dispositions')
    for index, gate_id in enumerate(GATES):
        if gate_id in statuses and statuses[gate_id] in accepted:
            for earlier in GATES[:index]:
                if statuses.get(earlier) not in accepted:
                    errors.append(f'{gate_id}: cannot pass before {earlier}')
                    break
            if index and gate_id in gate_times and GATES[index - 1] in gate_times \
                    and gate_times[gate_id] < gate_times[GATES[index - 1]]:
                errors.append(f'{gate_id}: observation predates {GATES[index - 1]}')

    go = data.get('go')
    if not isinstance(go, dict) or set(go) != {
            'status', 'issued_at', 'valid_until', 'scope_sha256', 'evidence_sha256',
            'window_start', 'window_end', 'authority_reference', 'candidate_sha',
            'profile_sha256', 'inventory_sha256', 'refs_manifest_sha256',
            'controls_manifest_sha256', 'peers_manifest_sha256', 'freeze_sha',
            'operator', 'reviewer', 'impact_budget_reference'}:
        errors.append('go object has invalid schema')
    elif statuses.get('G12') in accepted:
        if go.get('status') != 'GO' or go.get('scope_sha256') != scope_hash:
            errors.append('G12: current GO must bind the exact scope hash')
        if not isinstance(go.get('evidence_sha256'), str) or not HASH256.fullmatch(go.get('evidence_sha256', '')):
            errors.append('G12: GO evidence SHA-256 required')
        if not isinstance(go.get('authority_reference'), str) or not go.get('authority_reference', '').strip():
            errors.append('G12: authority reference required')
        inventory = data.get('inventory')
        write_fences = data.get('write_fences')
        if not isinstance(go.get('candidate_sha'), str) or not SHA1.fullmatch(go.get('candidate_sha', '')) \
                or not isinstance(inventory, dict) or go.get('candidate_sha') != inventory.get('source_sha'):
            errors.append('G12: GO must bind the exact inventoried candidate SHA')
        if not isinstance(go.get('profile_sha256'), str) or not HASH256.fullmatch(go.get('profile_sha256', '')) \
                or not isinstance(inventory, dict) or go.get('profile_sha256') != inventory.get('profile_sha256'):
            errors.append('G12: GO must bind the audited profile hash')
        if not isinstance(go.get('inventory_sha256'), str) or not HASH256.fullmatch(go.get('inventory_sha256', '')) \
                or not isinstance(inventory, dict) or go.get('inventory_sha256') != inventory.get('report_sha256'):
            errors.append('G12: GO must bind the exact inventory report hash')
        for name in ('refs_manifest_sha256', 'controls_manifest_sha256', 'peers_manifest_sha256'):
            if not isinstance(go.get(name), str) or not HASH256.fullmatch(go.get(name, '')):
                errors.append(f'G12: {name} required')
        if not isinstance(go.get('freeze_sha'), str) or not SHA1.fullmatch(go.get('freeze_sha', '')) \
                or not isinstance(write_fences, dict) or go.get('freeze_sha') != write_fences.get('freeze_sha'):
            errors.append('G12: GO must bind the G11 FREEZE_SHA')
        gate12 = gates[12] if len(gates) == len(GATES) else {}
        if not isinstance(go.get('operator'), str) or not go.get('operator', '').strip() \
                or not isinstance(go.get('reviewer'), str) or not go.get('reviewer', '').strip() \
                or _identity(go.get('operator')) == _identity(go.get('reviewer')):
            errors.append('G12: distinct change operator and reviewer required')
        elif _identity(go.get('operator')) != _identity(gate12.get('owner')) \
                or _identity(go.get('reviewer')) != _identity(gate12.get('reviewer')):
            errors.append('G12: GO operator/reviewer must match the gate record')
        if not isinstance(go.get('impact_budget_reference'), str) or not go.get('impact_budget_reference', '').strip():
            errors.append('G12: approved impact budget reference required')
        issued = _parse_time(go.get('issued_at'), 'go.issued_at', errors)
        valid = _parse_time(go.get('valid_until'), 'go.valid_until', errors)
        window_start = _parse_time(go.get('window_start'), 'go.window_start', errors)
        window_end = _parse_time(go.get('window_end'), 'go.window_end', errors)
        if issued is not None and issued > now:
            errors.append('G12: GO was issued in the future')
        if issued is not None and valid is not None and valid <= now:
            errors.append('G12: GO is not current')
        if issued is not None and gate_times.get('G12') is not None \
                and gate_times['G12'] < issued:
            errors.append('G12: gate evidence predates GO issuance')
        if valid is not None and gate_times.get('G12') is not None:
            gate = gates[12]
            gate_valid = _parse_time(gate.get('valid_until'), 'G12.valid_until', errors)
            if gate_valid is not None and gate_valid > valid:
                errors.append('G12: gate validity extends beyond current GO')
        if issued is not None and window_start is not None and window_start > now:
            errors.append('G12: future cutover window cannot be pre-approved')
        if issued is not None and window_start is not None and window_start < issued:
            errors.append('G12: change window cannot predate GO issuance')
        if window_start is not None and window_end is not None and window_end <= window_start:
            errors.append('G12: invalid change window')
        if valid is not None and window_end is not None and valid < window_end:
            errors.append('G12: GO validity must cover the full change window')
        if window_end is not None and window_end < now:
            errors.append('G12: change window has ended')
        last_pre_cut = gate_times.get('G11')
        if issued is not None and last_pre_cut is not None and issued <= last_pre_cut:
            errors.append('G12: GO must be issued strictly after final freeze/drain gate G11')
    elif go and go.get('status') != 'UNKNOWN':
        errors.append('GO must remain UNKNOWN until G00–G11 have current proof')

    requested = data.get('transfer_requested_at')
    request_time = None
    if requested is not None:
        request_time = _parse_time(requested, 'transfer_requested_at', errors)
        if statuses.get('G12') != 'PASS':
            errors.append('transfer request requires a current G12 GO')
        if request_time is not None and gate_times.get('G12') is not None \
                and request_time < gate_times['G12']:
            errors.append('transfer request predates the recorded G12 GO')
        issued_value = go.get('issued_at') if isinstance(go, dict) else None
        issued = _parse_time(issued_value, 'go.issued_at', errors) if issued_value else None
        if request_time is not None and issued is not None and request_time < issued:
            errors.append('transfer request predates current GO')
        if request_time is not None and isinstance(go, dict):
            valid = _parse_time(go.get('valid_until'), 'go.valid_until', errors) if go.get('valid_until') else None
            window_start = _parse_time(go.get('window_start'), 'go.window_start', errors) if go.get('window_start') else None
            window_end = _parse_time(go.get('window_end'), 'go.window_end', errors) if go.get('window_end') else None
            if valid is not None and request_time > valid:
                errors.append('transfer request is outside GO validity')
            if window_start is not None and request_time < window_start:
                errors.append('transfer request predates authorized window')
            if window_end is not None and request_time > window_end:
                errors.append('transfer request is after authorized window')
        if request_time is not None and request_time > now:
            errors.append('transfer request timestamp is in the future')
    if any(statuses.get(g) in accepted for g in GATES[13:]) and request_time is None:
        errors.append('G13–G19 cannot be approved before a transfer request/readback exists')
    if request_time is not None and statuses.get('G13') in accepted \
            and gate_times.get('G13', request_time) <= request_time:
        errors.append('G13 observation must follow the transfer request')
    if statuses.get('G13') == 'PASS':
        readback = data.get('readback')
        expected_readback = {'status', 'repository_id', 'owner', 'owner_id', 'name',
                             'visibility', 'canonical_path', 'observed_at'}
        if not isinstance(readback, dict) or set(readback) != expected_readback:
            errors.append('G13: canonical post-transfer identity readback required')
        else:
            destination = scope.get('destination') if isinstance(scope, dict) else None
            expected_owner = destination.get('owner') if isinstance(destination, dict) else None
            expected_owner_id = destination.get('owner_id') if isinstance(destination, dict) else None
            expected_name = scope.get('repository_name') if isinstance(scope, dict) else None
            if readback.get('status') != 'OBSERVED' \
                    or readback.get('repository_id') != 1232040291 \
                    or readback.get('owner_id') != expected_owner_id \
                    or _identity(readback.get('owner')) != _identity(expected_owner) \
                    or readback.get('name') != expected_name \
                    or readback.get('visibility') != 'private' \
                    or str(readback.get('canonical_path', '')).casefold() != f'{expected_owner}/{expected_name}'.casefold():
                errors.append('G13: readback identity must match the same private repository at the approved destination')
            readback_at = _parse_time(readback.get('observed_at'), 'readback.observed_at', errors)
            if readback_at is not None and request_time is not None and readback_at <= request_time:
                errors.append('G13: readback must follow the transfer request')
            if readback_at is not None and readback_at > now:
                errors.append('G13: readback timestamp is in the future')

    write_fences = data.get('write_fences')
    backup = data.get('backup_restore')
    privacy = data.get('privacy')
    rollback = data.get('rollback')
    rotation = data.get('credential_rotation')
    split_brain = data.get('split_brain')
    if statuses.get('G11') == 'PASS':
        if not isinstance(write_fences, dict) or set(write_fences) != {
                'source', 'destination', 'writers_drained', 'remaining_writers', 'freeze_sha',
                'reconciliation'}:
            errors.append('G11: final freeze/write-fence record required')
        else:
            if write_fences.get('source') != 'FROZEN_READ_ONLY':
                errors.append('G11: source write fence must be FROZEN_READ_ONLY')
            if write_fences.get('destination') != 'PREPARATION_ONLY':
                errors.append('G11: destination write fence must be PREPARATION_ONLY')
            if write_fences.get('writers_drained') is not True or write_fences.get('remaining_writers') != []:
                errors.append('G11: all writers must be drained and reconciled')
            if not isinstance(write_fences.get('freeze_sha'), str) or not SHA1.fullmatch(write_fences.get('freeze_sha', '')):
                errors.append('G11: full FREEZE_SHA required')
            if not isinstance(write_fences.get('reconciliation'), dict) or write_fences['reconciliation'].get('equal') is not True:
                errors.append('G11: source/backup/restore reconciliation must be equal')
        if not isinstance(backup, dict) or backup.get('parity_verified') is not True or backup.get('restore_tested') is not True:
            errors.append('G11: backup/restore parity and restore test required')
        elif isinstance(write_fences, dict) and backup.get('freeze_sha') != write_fences.get('freeze_sha'):
            errors.append('G11: backup and write-fence FREEZE_SHA differ')

    if statuses.get('G08') == 'PASS':
        if not isinstance(backup, dict) or set(backup) != {
                'freeze_sha', 'backup_sha', 'restored_sha', 'refs_manifest_sha256',
                'assets_manifest_sha256', 'lfs_manifest_sha256', 'parity_verified',
                'restore_tested', 'evidence_reference'}:
            errors.append('G08: complete backup/restore parity record required')
        else:
            shas = [backup.get(k) for k in ('freeze_sha', 'backup_sha', 'restored_sha')]
            if not all(isinstance(x, str) and SHA1.fullmatch(x) for x in shas) or len(set(shas)) != 1:
                errors.append('G08: freeze, backup, and restored commit identities must match')
            for name in ('refs_manifest_sha256', 'assets_manifest_sha256', 'lfs_manifest_sha256'):
                if not isinstance(backup.get(name), str) or not HASH256.fullmatch(backup.get(name, '')):
                    errors.append(f'G08: {name} required for parity')
            if backup.get('parity_verified') is not True or backup.get('restore_tested') is not True:
                errors.append('G08: parity and tested restoration are required')
            if not isinstance(backup.get('evidence_reference'), str) or not backup.get('evidence_reference', '').strip():
                errors.append('G08: backup/restore evidence reference required')
    if statuses.get('G08') == 'PASS':
        if not isinstance(privacy, dict) or privacy != {
                'classification': 'PRIVATE', 'raw_credentials_exported': False,
                'tenant_data_exported': False}:
            errors.append('G08: private evidence with no credential values or tenant data required')

    if statuses.get('G09') == 'PASS':
        if not isinstance(rollback, dict) or rollback != {
                'forward_recovery_plan': rollback.get('forward_recovery_plan') if isinstance(rollback, dict) else None,
                'independent_recovery_access': True, 'transfer_back_assumed': False,
                'evidence_reference': rollback.get('evidence_reference') if isinstance(rollback, dict) else None}:
            errors.append('G09: tested forward recovery must be independent; transfer-back is never assumed')
        elif not isinstance(rollback.get('forward_recovery_plan'), str) or not rollback['forward_recovery_plan'].strip() \
                or not isinstance(rollback.get('evidence_reference'), str) or not rollback['evidence_reference'].strip():
            errors.append('G09: recovery plan and evidence reference required')
    if statuses.get('G16') == 'PASS':
        required_rotation = {'planned', 'old_credentials_revoked', 'new_consumers_verified',
                             'values_exported', 'evidence_reference'}
        if not isinstance(rotation, dict) or set(rotation) != required_rotation \
                or rotation.get('planned') is not True or rotation.get('old_credentials_revoked') is not True \
                or rotation.get('new_consumers_verified') is not True or rotation.get('values_exported') is not False \
                or not isinstance(rotation.get('evidence_reference'), str) or not rotation.get('evidence_reference', '').strip():
            errors.append('G16: scoped credential rotation and consumer proof required; values must not be exported')
    if statuses.get('G18') == 'PASS':
        if not isinstance(split_brain, dict) or split_brain != {
                'source_writers_disabled': True, 'destination_single_writer': True,
                'active_writer_count': 1, 'reconciliation_after_cutover': True}:
            errors.append('G18: split-brain prevention and single-writer reconciliation required')
    if statuses.get('G19') == 'PASS':
        _validate_access_cleanup(data.get('access_cleanup'), scope, now, request_time,
                                 gate_times.get('G19'), errors)

    consistent = not errors and not blockers
    return {'schema_version': 2, 'record_consistent': consistent,
            'migration_ready': False, 'authorization_verified': False,
            'blockers': blockers, 'errors': errors,
            'interpretation': 'Record consistency is not authenticated evidence, migration readiness, or authorization.'}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('ledger', type=Path)
    args = parser.parse_args(argv)
    try:
        result = check_ledger(json.loads(args.ledger.read_text(encoding='utf-8')))
    except (OSError, json.JSONDecodeError) as exc:
        print(json.dumps({'schema_version': 2, 'record_consistent': False,
                          'migration_ready': False, 'authorization_verified': False,
                          'blockers': [], 'errors': [str(exc)]}, sort_keys=True))
        return 2
    print(json.dumps(result, sort_keys=True, separators=(',', ':')))
    return 0 if result['record_consistent'] else 2


if __name__ == '__main__':
    raise SystemExit(main())
