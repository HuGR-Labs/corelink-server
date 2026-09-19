#!/usr/bin/env python3
"""Read-only, commit-bound inventory and plan for a corelink-server org transfer.

No apply/transfer/dispatch command exists. Output is JSON on stdout; diagnostics
are on stderr. Local Git reads never follow working-tree symlinks or run hooks.
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / 'docs/operations/repository-org-migration/migration.json'
SERVER_ID = 1232040291
MAX_BLOB_BYTES = 16 * 1024 * 1024
OWNER = re.compile(r'[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?\Z')
SHA = re.compile(r'[0-9a-f]{40}\Z')
GATES = (
    'G01-destination-authority', 'G02-private-access-and-governance',
    'G03-portability-patches', 'G04-apps-and-runner-handoff',
    'G05-credentials-and-integrations', 'G06-signing-and-oidc',
    'G07-freeze-and-recovery', 'G08-explicit-transfer-authorization',
    'G09-post-transfer-identity', 'G10-behavior-and-observation',
)


def valid_owner(value: Any) -> bool:
    return isinstance(value, str) and OWNER.fullmatch(value) is not None and '--' not in value


def positive_id(value: Any) -> bool:
    return type(value) is int and value > 0


def validate_manifest(data: dict[str, Any]) -> None:
    if data.get('schema_version') != 1:
        raise ValueError('unsupported manifest schema')
    source = data.get('source', {})
    if source.get('repository_id') != SERVER_ID or source.get('name') != 'corelink-server':
        raise ValueError('scope is exclusively repository 1232040291/corelink-server')
    if not valid_owner(source.get('owner')) or not positive_id(source.get('owner_id')):
        raise ValueError('source owner and numeric ID required')
    if not SHA.fullmatch(source.get('baseline_sha', '')):
        raise ValueError('baseline must be a full commit SHA')
    if data.get('transfer_scope') != ['server']:
        raise ValueError('only server may be transferred')
    if data.get('preserve_visibility') != 'private' or data.get('allow_repository_rename') is not False:
        raise ValueError('visibility and repository name are invariants')
    deps = data.get('dependencies', {})
    expected = {'runners': (1266754321, 'corelink-runners'),
                'workspaces': (1259579816, 'corelink-workspaces'),
                'cli_distribution': (1251605593, 'corelink-cli')}
    if set(deps) != set(expected):
        raise ValueError('independent dependency roles must be explicit')
    for role, (repo_id, name) in expected.items():
        entry = deps[role]
        if (entry.get('repository_id'), entry.get('name')) != (repo_id, name):
            raise ValueError(f'wrong dependency identity: {role}')
        if not valid_owner(entry.get('owner')) or entry.get('transfer_in_this_campaign') is not False:
            raise ValueError(f'dependency cannot join this transfer: {role}')
    if not all(valid_owner(x) for x in data.get('legacy_owners', [])):
        raise ValueError('invalid legacy owner')


def git(root: Path, *args: str) -> bytes:
    result = subprocess.run(['git', '-C', str(root), *args], capture_output=True, timeout=120)
    if result.returncode:
        raise ValueError(f'git read failed: {result.stderr.decode(errors="replace").strip()}')
    return result.stdout


def resolve(root: Path, revision: str) -> str:
    value = git(root, 'rev-parse', '--verify', '--end-of-options', revision + '^{commit}').decode().strip()
    if not SHA.fullmatch(value):
        raise ValueError('expected a SHA-1 Git commit')
    return value


def file_class(path: str) -> str:
    """Triage hint, NOT a claim that a match executes or is safe to rewrite."""
    if path.startswith(('evidence/', 'artifacts/', 'reports/', '_archive/', 'changelog.d/')) or path == 'CHANGELOG.md':
        return 'historical-review-do-not-rewrite'
    if path.startswith('.github/workflows/'):
        return 'workflow-review'
    if re.search(r'(^|/)(tests?|examples|fixtures|goldens)(/|_)|(^|/)test_|\.(test|spec)\.', path):
        return 'test-or-example-review'
    if path.startswith(('docs/', 'specs/', 'marketing/', 'legal/', 'compliance/')) or path.endswith(('.md', '.mdx')):
        return 'documentation-review'
    return 'code-or-config-review'


def search_terms(manifest: dict[str, Any]) -> tuple[str, ...]:
    owners = [manifest['source']['owner'], *manifest.get('legacy_owners', [])]
    owners.extend(v['owner'] for v in manifest['dependencies'].values())
    return tuple(sorted(set(owners + ['corelink-server', 'corelink-runners',
                                    'corelink-workspaces', 'corelink-cli', 'ghcr.io',
                                    'registry.cloudflare.com', 'GITHUB_APP_ORG']), key=str.casefold))


def scan(root: Path, revision: str, manifest: dict[str, Any]) -> dict[str, Any]:
    validate_manifest(manifest)
    commit = resolve(root, revision)
    entries = git(root, 'ls-tree', '-rz', '--full-tree', commit).split(b'\0')
    terms = search_terms(manifest)
    matches, skipped, files = [], [], []
    counts: collections.Counter[str] = collections.Counter()
    proc = subprocess.Popen(['git', '-C', str(root), 'cat-file', '--batch'],
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert proc.stdin is not None and proc.stdout is not None
    try:
        for entry in entries:
            if not entry:
                continue
            meta, raw_path = entry.split(b'\t', 1)
            mode, kind, oid = meta.decode().split()
            path = raw_path.decode('utf-8', errors='surrogateescape')
            counts['tree_entries'] += 1
            if kind != 'blob':
                skipped.append({'path': path, 'reason': 'gitlink-not-expanded', 'object': oid})
                continue
            proc.stdin.write(oid.encode() + b'\n')
            proc.stdin.flush()
            header = proc.stdout.readline().decode().split()
            if len(header) != 3 or header[0] != oid or header[1] != 'blob':
                raise ValueError(f'invalid cat-file response for {path}')
            size = int(header[2])
            if size > MAX_BLOB_BYTES:
                remaining = size
                while remaining:
                    chunk = proc.stdout.read(min(remaining, 65536))
                    if not chunk:
                        raise ValueError('truncated Git blob')
                    remaining -= len(chunk)
                if proc.stdout.read(1) != b'\n':
                    raise ValueError('invalid Git blob terminator')
                skipped.append({'path': path, 'reason': 'over-16MiB', 'object': oid, 'bytes': size})
                continue
            content = proc.stdout.read(size)
            if len(content) != size or proc.stdout.read(1) != b'\n':
                raise ValueError('truncated Git blob')
            record = {'path': path, 'mode': mode, 'blob': oid, 'bytes': size,
                      'sha256': hashlib.sha256(content).hexdigest()}
            files.append(record)
            counts['blob_bytes_read'] += size
            if mode == '120000':
                counts['symlink_blobs_not_followed'] += 1
            try:
                if b'\0' in content:
                    raise UnicodeError('binary NUL')
                text = content.decode('utf-8')
            except UnicodeError:
                skipped.append({'path': path, 'reason': 'binary-or-non-UTF8', 'object': oid})
                continue
            counts['text_blobs_scanned'] += 1
            if text.startswith('version https://git-lfs.github.com/spec/v1\n'):
                skipped.append({'path': path, 'reason': 'LFS-pointer-only', 'object': oid})
            for line_number, line in enumerate(text.splitlines(), 1):
                found = [term for term in terms if term.casefold() in line.casefold()]
                if found:
                    matches.append({'path': path, 'line': line_number, 'terms': found,
                                    'class': file_class(path), 'line_sha256': hashlib.sha256(line.encode()).hexdigest()})
    finally:
        proc.stdin.close()
        proc.stdout.close()
        try:
            code = proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)
            raise ValueError('Git batch did not exit')
        if proc.stderr:
            proc.stderr.close()
    if code:
        raise ValueError('Git batch failed')
    counts['matched_lines'] = len(matches)
    counts['matched_files'] = len({x['path'] for x in matches})
    return {'schema_version': 1, 'source_sha': commit, 'terms': list(terms),
            'coverage': dict(counts), 'skipped_or_partial': skipped,
            'method': 'Git tree only; case-insensitive literal triage; hashes and locations, never raw matching lines. No semantic or live coverage claim.',
            'files': files, 'matches': matches}


def plan(manifest: dict[str, Any], owner: str, owner_id: int) -> dict[str, Any]:
    validate_manifest(manifest)
    if not valid_owner(owner) or not positive_id(owner_id):
        raise ValueError('target owner slug and positive numeric ID required; verify with GitHub first')
    source = manifest['source']
    if owner.casefold() == source['owner'].casefold() or owner_id == source['owner_id']:
        raise ValueError('target must be a different organization, not a case-only rename')
    digest = hashlib.sha256(json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    return {'schema_version': 1, 'status': 'PLANNING_ONLY_NOT_AUTHORIZED',
            'manifest_sha256': digest, 'source': source,
            'target': {'owner': owner, 'owner_id': owner_id, 'name': source['name'],
                       'repository_id': source['repository_id'], 'visibility': 'private'},
            'predicted_immutable_sub_prefix': f"repo:{owner}@{owner_id}/{source['name']}@{source['repository_id']}",
            'warning': 'Prediction is not a token observation or trust policy. No transfer, patch, dispatch or readiness approval performed.',
            'unchanged_dependencies': manifest['dependencies'],
            'gates': [{'id': gate, 'status': 'UNVERIFIED', 'evidence': None} for gate in GATES]}


def compare(before: dict[str, Any], after: dict[str, Any]) -> dict[str, Any]:
    """Pure diff: no approval. Additions/removals and changed blob bytes are explicit."""
    for doc in (before, after):
        if doc.get('schema_version') != 1 or not isinstance(doc.get('files'), list):
            raise ValueError('two scan reports required')
    a = {x['path']: x for x in before['files']}
    b = {x['path']: x for x in after['files']}
    return {'status': 'REVIEW_REQUIRED', 'before': before['source_sha'], 'after': after['source_sha'],
            'added': sorted(b.keys() - a.keys()), 'removed': sorted(a.keys() - b.keys()),
            'changed': sorted(k for k in a.keys() & b.keys() if a[k] != b[k]),
            'before_partial': before.get('skipped_or_partial', []),
            'after_partial': after.get('skipped_or_partial', [])}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, default=DEFAULT_MANIFEST)
    commands = parser.add_subparsers(dest='command', required=True)
    inventory = commands.add_parser('scan', help='read committed Git blobs, not the working tree')
    inventory.add_argument('--revision', required=True)
    inventory.add_argument('--root', type=Path, default=ROOT)
    planner = commands.add_parser('plan', help='emit an unapproved parameterized plan')
    planner.add_argument('--target-owner', required=True)
    planner.add_argument('--target-owner-id', required=True, type=int)
    diff = commands.add_parser('compare', help='diff two inventories without approval')
    diff.add_argument('before', type=Path)
    diff.add_argument('after', type=Path)
    args = parser.parse_args(argv)
    try:
        manifest = json.loads(args.manifest.read_text())
        if args.command == 'scan':
            result = scan(args.root, args.revision, manifest)
        elif args.command == 'plan':
            result = plan(manifest, args.target_owner, args.target_owner_id)
        else:
            result = compare(json.loads(args.before.read_text()), json.loads(args.after.read_text()))
        print(json.dumps(result, ensure_ascii=True, sort_keys=True, separators=(',', ':')))
        return 0
    except (OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError) as exc:
        print(f'ERROR: {exc}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
