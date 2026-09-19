#!/usr/bin/env python3
"""Collect allowlisted GitHub metadata with GET only, no credential values.

A snapshot is an observation, not readiness approval. Output contains only
explicitly selected metadata fields. Non-successes remain UNVERIFIED.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from typing import Any
from urllib.parse import quote

from repository_org_migration import SERVER_ID, valid_owner


def pick(data: dict[str, Any], keys: tuple[str, ...]) -> dict[str, Any]:
    return {key: data[key] for key in keys if key in data}


def clock() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat()


def query(endpoint: str, transform: Any) -> dict[str, Any]:
    """Only callers in collect construct paths; no custom endpoints or writes."""
    result: dict[str, Any] = {'endpoint': endpoint, 'observed_at_utc': clock()}
    try:
        proc = subprocess.run(['gh', 'api', '--hostname', 'github.com', '--method', 'GET',
                               endpoint, '--paginate', '--slurp'],
                              capture_output=True, text=True, timeout=45)
        if proc.returncode:
            code = re.search(r'HTTP (\d{3})', proc.stderr)
            result.update(status='UNVERIFIED', http_status=int(code[1]) if code else None,
                          error='GitHub read failed; no empty-state inference allowed')
        else:
            pages = json.loads(proc.stdout)
            if not isinstance(pages, list) or not pages:
                raise ValueError('invalid paginated response')
            result.update(status='OBSERVED', pages=[transform(page) for page in pages])
    except (OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError):
        result.update(status='UNVERIFIED', error='Read, timeout or response-schema failure')
    return result


def items(key: str, fields: tuple[str, ...]) -> Any:
    def project(page: dict[str, Any]) -> dict[str, Any]:
        rows = page[key]
        if not isinstance(rows, list):
            raise ValueError('expected list')
        return {'total_count': page.get('total_count'), key: [pick(row, fields) for row in rows]}
    return project


def list_rows(fields: tuple[str, ...]) -> Any:
    return lambda page: [pick(row, fields) for row in page]


def repo_metadata(page: dict[str, Any]) -> dict[str, Any]:
    data = pick(page, ('id', 'node_id', 'full_name', 'private', 'visibility', 'default_branch',
                      'has_pages', 'has_wiki', 'has_discussions', 'archived', 'permissions',
                      'delete_branch_on_merge', 'allow_squash_merge', 'allow_merge_commit',
                      'allow_rebase_merge', 'allow_auto_merge', 'fork'))
    data['owner'] = pick(page['owner'], ('login', 'id', 'type'))
    return data


def refs(page: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [{'ref': x['ref'], 'object': pick(x['object'], ('sha', 'type'))} for x in page]


def releases(page: list[dict[str, Any]]) -> list[dict[str, Any]]:
    rows = []
    for item in page:
        row = pick(item, ('id', 'tag_name', 'draft', 'prerelease', 'published_at'))
        row['assets'] = [pick(a, ('id', 'name', 'size', 'digest', 'state')) for a in item.get('assets', [])]
        rows.append(row)
    return rows


def environments(page: dict[str, Any]) -> dict[str, Any]:
    rows = []
    for item in page['environments']:
        row = pick(item, ('id', 'name', 'can_admins_bypass', 'deployment_branch_policy'))
        rules = []
        for rule in item.get('protection_rules', []):
            safe = pick(rule, ('id', 'type', 'wait_timer', 'prevent_self_review'))
            safe['reviewers'] = [{'type': r.get('type'),
                                  'reviewer': pick(r.get('reviewer', {}), ('id', 'login', 'slug'))}
                                 for r in rule.get('reviewers', [])]
            rules.append(safe)
        row['protection_rules'] = rules
        rows.append(row)
    return {'total_count': page.get('total_count'), 'environments': rows}


def collect(owner: str) -> dict[str, Any]:
    if not valid_owner(owner):
        raise ValueError('valid organization login required')
    started = clock()
    root = f'repos/{owner}/corelink-server'
    source = query(root, repo_metadata)
    result: dict[str, Any] = {'schema_version': 1, 'started_at_utc': started,
                              'status': 'OBSERVATION_NOT_READINESS', 'requested_repository': f'{owner}/corelink-server',
                              'method': 'GET only; paginated; allowlisted metadata; no secret, variable or webhook values',
                              'surfaces': {'metadata': source}}
    observed = source.get('pages', [{}])[0]
    if (source['status'] != 'OBSERVED' or observed.get('id') != SERVER_ID
            or observed.get('full_name', '').casefold() != f'{owner}/corelink-server'.casefold()
            or observed.get('owner', {}).get('type') != 'Organization'):
        result['status'] = 'STOP_IDENTITY_UNVERIFIED_OR_ALIAS'
        result['finished_at_utc'] = clock()
        return result
    endpoints: dict[str, tuple[str, Any]] = {
        'source_org': (f'orgs/{owner}', lambda p: {**pick(p, ('login', 'id', 'default_repository_permission', 'members_can_create_repositories', 'two_factor_requirement_enabled')), 'plan': pick(p.get('plan', {}), ('name',))}),
        'heads': (root + '/git/matching-refs/heads/?per_page=100', refs),
        'tags': (root + '/git/matching-refs/tags/?per_page=100', refs),
        'teams': (root + '/teams?per_page=100', list_rows(('id', 'slug', 'permission'))),
        'collaborators': (root + '/collaborators?affiliation=all&per_page=100', list_rows(('login', 'role_name', 'permissions'))),
        'webhooks': (root + '/hooks?per_page=100', list_rows(('id', 'name', 'active', 'events'))),
        'deploy_keys': (root + '/keys?per_page=100', list_rows(('id', 'title', 'read_only'))),
        'actions_permissions': (root + '/actions/permissions', lambda p: pick(p, ('enabled', 'allowed_actions', 'sha_pinning_required'))),
        'workflow_permissions': (root + '/actions/permissions/workflow', lambda p: pick(p, ('default_workflow_permissions', 'can_approve_pull_request_reviews'))),
        'repo_secrets': (root + '/actions/secrets?per_page=100', items('secrets', ('name', 'created_at', 'updated_at'))),
        'repo_variables': (root + '/actions/variables?per_page=100', items('variables', ('name', 'created_at', 'updated_at'))),
        'org_secrets': (root + '/actions/organization-secrets?per_page=100', items('secrets', ('name', 'visibility', 'created_at', 'updated_at'))),
        'org_variables': (root + '/actions/organization-variables?per_page=100', items('variables', ('name', 'visibility', 'created_at', 'updated_at'))),
        'environments': (root + '/environments?per_page=100', environments),
        'runners': (root + '/actions/runners?per_page=100', items('runners', ('id', 'name', 'os', 'status', 'busy', 'labels'))),
        'org_runners': (f'orgs/{owner}/actions/runners?per_page=100', items('runners', ('id', 'name', 'os', 'status', 'busy', 'labels'))),
        'runner_groups': (f'orgs/{owner}/actions/runner-groups?per_page=100', items('runner_groups', ('id', 'name', 'visibility', 'allows_public_repositories', 'default', 'inherited', 'restricted_to_workflows', 'selected_workflows'))),
        'org_installations': (f'orgs/{owner}/installations?per_page=100', items('installations', ('id', 'app_id', 'app_slug', 'repository_selection', 'permissions', 'events', 'target_type', 'suspended_at'))),
        'workflows': (root + '/actions/workflows?per_page=100', items('workflows', ('id', 'name', 'path', 'state'))),
        'oidc': (root + '/actions/oidc/customization/sub', lambda p: pick(p, ('use_default', 'include_claim_keys', 'use_immutable_subject', 'sub_claim_prefix'))),
        'dependabot_secrets': (root + '/dependabot/secrets?per_page=100', items('secrets', ('name', 'created_at', 'updated_at'))),
        'codespaces_secrets': (root + '/codespaces/secrets?per_page=100', items('secrets', ('name', 'created_at', 'updated_at'))),
        'releases': (root + '/releases?per_page=100', releases),
        'open_prs': (root + '/pulls?state=open&per_page=100', lambda p: [{'number': x['number'], 'draft': x['draft'], 'head_sha': x['head']['sha'], 'head_ref': x['head']['ref'], 'base_ref': x['base']['ref']} for x in p]),
        'open_issues': (root + '/issues?state=open&per_page=100', list_rows(('number', 'state'))),
        'rulesets': (root + '/rulesets?includes_parents=true&per_page=100', list_rows(('id', 'name', 'target', 'source_type', 'source', 'enforcement'))),
        'protection': (root + '/branches/main/protection', lambda p: pick(p, ('required_status_checks', 'enforce_admins', 'required_pull_request_reviews', 'restrictions', 'required_signatures', 'required_linear_history', 'allow_force_pushes', 'allow_deletions', 'required_conversation_resolution'))),
    }
    def fetch(item: tuple[str, tuple[str, Any]]) -> tuple[str, dict[str, Any]]:
        name, (path, transform) = item
        return name, query(path, transform)
    with ThreadPoolExecutor(max_workers=4) as pool:
        for key, value in pool.map(fetch, endpoints.items()):
            result['surfaces'][key] = value
    env_result = result['surfaces']['environments']
    for page in env_result.get('pages', []):
        for env in page['environments']:
            for kind in ('secrets', 'variables'):
                path = root + '/environments/' + quote(env['name'], safe='') + '/' + kind + '?per_page=100'
                result['surfaces'][f"env:{env['name']}:{kind}"] = query(path, items(kind, ('name', 'created_at', 'updated_at')))
    result['finished_at_utc'] = clock()
    result['unverified_surfaces'] = [key for key, val in result['surfaces'].items() if val['status'] != 'OBSERVED']
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--owner', required=True)
    args = parser.parse_args(argv)
    try:
        result = collect(args.owner)
        print(json.dumps(result, sort_keys=True, indent=2))
        return 2 if result['status'].startswith('STOP') or result.get('unverified_surfaces') else 0
    except (ValueError, OSError, TypeError, KeyError) as exc:
        print(f'ERROR: {exc}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
