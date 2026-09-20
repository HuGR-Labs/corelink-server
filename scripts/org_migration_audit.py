#!/usr/bin/env python3
"""Offline, read-only source audit and planning aid for the server org move.

This tool reads committed Git objects only. It cannot establish live identity,
approve a gate, authenticate a GO, or perform a migration mutation.
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import os
import json
import platform
import re
import selectors
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / 'docs/internal/org-migration/topology.example.json'
SERVER_ID = 1232040291
MAX_BLOB_BYTES = 16 * 1024 * 1024
MAX_TREE_ENTRIES = 100_000
MAX_TREE_LISTING_BYTES = 64 * 1024 * 1024
MAX_TOTAL_BLOB_BYTES = 512 * 1024 * 1024
MAX_FINDINGS = 100_000
MAX_SCAN_SECONDS = 120
TOOL_VERSION = '2.0.0'
OWNER = re.compile(r'[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?\Z')
SHA = re.compile(r'[0-9a-f]{40}\Z')
GATES = tuple(f'G{number:02d}' for number in range(20))
OWNER_PATH = re.compile(r'([A-Za-z0-9][A-Za-z0-9-]{0,38})/corelink-server', re.I | re.ASCII)
ASCII_LOWER_TABLE = bytes.maketrans(b'ABCDEFGHIJKLMNOPQRSTUVWXYZ', b'abcdefghijklmnopqrstuvwxyz')


def valid_owner(value: Any) -> bool:
    return isinstance(value, str) and OWNER.fullmatch(value) is not None and '--' not in value


def positive_id(value: Any) -> bool:
    return type(value) is int and value > 0


def validate_manifest(data: dict[str, Any]) -> None:
    if not isinstance(data, dict):
        raise ValueError('manifest must be an object')
    if data.get('schema_version') != 1:
        raise ValueError('unsupported manifest schema')
    source = data.get('source', {})
    if not isinstance(source, dict):
        raise ValueError('source must be an object')
    if not positive_id(source.get('repository_id')):
        raise ValueError('repository ID must be an integer')
    if source.get('repository_id') != SERVER_ID or source.get('name') != 'corelink-server':
        raise ValueError('scope is exclusively repository 1232040291/corelink-server')
    if not valid_owner(source.get('owner')) or not positive_id(source.get('owner_id')):
        raise ValueError('source owner and numeric ID required')
    if not isinstance(source.get('baseline_sha'), str) or not SHA.fullmatch(source['baseline_sha']):
        raise ValueError('baseline must be a full commit SHA')
    if data.get('transfer_scope') != ['server']:
        raise ValueError('only server may be transferred')
    if data.get('preserve_visibility') != 'private' or data.get('allow_repository_rename') is not False:
        raise ValueError('visibility and repository name are invariants')
    deps = data.get('dependencies', {})
    if not isinstance(deps, dict):
        raise ValueError('dependencies must be an object')
    expected = {'runners': (1266754321, 'corelink-runners'),
                'workspaces': (1259579816, 'corelink-workspaces'),
                'cli_distribution': (1251605593, 'corelink-cli')}
    if set(deps) != set(expected):
        raise ValueError('independent dependency roles must be explicit')
    for role, (repo_id, name) in expected.items():
        entry = deps[role]
        if not isinstance(entry, dict) or not positive_id(entry.get('repository_id')):
            raise ValueError(f'invalid dependency: {role}')
        if (entry.get('repository_id'), entry.get('name')) != (repo_id, name):
            raise ValueError(f'wrong dependency identity: {role}')
        if not valid_owner(entry.get('owner')) or entry.get('transfer_in_this_campaign') is not False:
            raise ValueError(f'dependency cannot join this transfer: {role}')
    legacy = data.get('legacy_owners', [])
    if not isinstance(legacy, list) or not all(valid_owner(x) for x in legacy):
        raise ValueError('invalid legacy owner')
    destination = data.get('destination')
    if destination is not None:
        if not isinstance(destination, dict):
            raise ValueError('destination must be null or an object')
        if not valid_owner(destination.get('owner')) or not positive_id(destination.get('owner_id')):
            raise ValueError('destination owner and numeric ID required')


def git(root: Path, *args: str, deadline: float) -> bytes:
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise ValueError('scan time limit exceeded; no complete report produced')
    try:
        result = subprocess.run(['git', '-C', str(root), *args], capture_output=True, timeout=remaining)
    except subprocess.TimeoutExpired as exc:
        raise ValueError('scan time limit exceeded; no complete report produced') from exc
    if result.returncode:
        raise ValueError(f'git read failed: {result.stderr.decode(errors="replace").strip()}')
    return result.stdout


def resolve(root: Path, revision: str, deadline: float) -> str:
    value = git(root, 'rev-parse', '--verify', '--end-of-options', revision + '^{commit}',
                deadline=deadline).decode().strip()
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
    if isinstance(manifest.get('destination'), dict):
        owners.append(manifest['destination']['owner'])
    return tuple(sorted(set(owners + ['corelink-server', 'corelink-runners',
                                    'corelink-workspaces', 'corelink-cli', 'ghcr.io',
                                    'registry.cloudflare.com', 'GITHUB_APP_ORG']), key=str.casefold))


def _ascii_lower(value: str) -> str:
    """Lower ASCII letters only so Unicode code-point and UTF-8 offsets stay stable."""
    raw = value.encode('utf-8', errors='surrogateescape')
    return raw.translate(ASCII_LOWER_TABLE).decode('utf-8', errors='surrogateescape')


def _finding(source_sha: str, path: str, blob: str, rule: str, term: str,
             location: str, line: int | None, start: int | None, end: int | None,
             absolute_start: int | None,
             line_sha256: str | None, classification: str, owner_known: bool | None) -> dict[str, Any]:
    identity = '\0'.join((source_sha, path, blob, rule, location,
                          term, '' if line is None else str(line),
                          '' if start is None else str(start), '' if end is None else str(end),
                          '' if absolute_start is None else str(absolute_start)))
    result = {'id': hashlib.sha256(identity.encode('utf-8', errors='surrogateescape')).hexdigest()[:24],
              'rule': rule, 'path': path, 'location': location, 'line': line,
              'byte_start': start, 'byte_end_exclusive': end,
              'absolute_byte_start': absolute_start, 'term': term,
              'classification_hint': classification, 'blob': blob}
    if line_sha256 is not None:
        result['line_sha256'] = line_sha256
    if owner_known is not None:
        result['configured_owner'] = owner_known
        result['disposition'] = 'UNRESOLVED'
    return result


def _tree_entries(root: Path, commit: str, deadline: float):
    """Stream NUL-delimited ls-tree records with bounded buffering and entry count."""
    proc = subprocess.Popen(['git', '-C', str(root), 'ls-tree', '-rz', '--full-tree', commit],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert proc.stdout is not None
    fd = proc.stdout.fileno()
    os.set_blocking(fd, False)
    selector = selectors.DefaultSelector()
    selector.register(fd, selectors.EVENT_READ)
    pending = bytearray()
    entries = 0
    listing_bytes = 0
    max_record_bytes = 1024 * 1024
    try:
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not selector.select(remaining):
                raise ValueError('scan time limit exceeded during tree listing; no complete report produced')
            chunk = os.read(fd, 65536)
            if not chunk:
                break
            pending.extend(chunk)
            while True:
                delimiter = pending.find(b'\0')
                if delimiter < 0:
                    if len(pending) > max_record_bytes:
                        raise ValueError('tree entry byte limit exceeded; no complete report produced')
                    break
                if delimiter > max_record_bytes:
                    raise ValueError('tree entry byte limit exceeded; no complete report produced')
                record = bytes(pending[:delimiter])
                del pending[:delimiter + 1]
                if not record:
                    continue
                listing_bytes += len(record) + 1
                if listing_bytes > MAX_TREE_LISTING_BYTES:
                    raise ValueError('tree listing byte limit exceeded; no complete report produced')
                entries += 1
                if entries > MAX_TREE_ENTRIES:
                    raise ValueError('tree entry limit exceeded; no complete report produced')
                yield record
        if pending:
            raise ValueError('unterminated tree entry; no complete report produced')
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise ValueError('scan time limit exceeded during tree listing; no complete report produced')
        if proc.wait(timeout=remaining) != 0:
            stderr = proc.stderr.read(4096).decode(errors='replace') if proc.stderr else ''
            raise ValueError(f'git tree listing failed: {stderr.strip()}')
    except (subprocess.TimeoutExpired, OSError) as exc:
        raise ValueError('scan time limit exceeded during tree listing; no complete report produced') from exc
    finally:
        selector.close()
        proc.stdout.close()
        if proc.stderr:
            proc.stderr.close()
        if proc.poll() is None:
            proc.kill()
            proc.wait()


def scan(root: Path, revision: str, manifest: dict[str, Any]) -> dict[str, Any]:
    deadline = time.monotonic() + MAX_SCAN_SECONDS
    validate_manifest(manifest)
    commit = resolve(root, revision, deadline)
    terms = search_terms(manifest)
    findings, skipped, files = [], [], []
    known_owners = {manifest['source']['owner'].casefold(), *[x.casefold() for x in manifest.get('legacy_owners', [])]}
    known_owners.update(v['owner'].casefold() for v in manifest['dependencies'].values())
    if isinstance(manifest.get('destination'), dict):
        known_owners.add(manifest['destination']['owner'].casefold())
    counts: collections.Counter[str] = collections.Counter()
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise ValueError('scan time limit exceeded; no complete report produced')
    proc = subprocess.Popen(['git', '-C', str(root), 'cat-file', '--batch'],
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        proc.kill()
        proc.wait()
        raise ValueError('scan time limit exceeded; no complete report produced')
    killer = threading.Timer(remaining, proc.kill)
    killer.daemon = True
    killer.start()
    assert proc.stdin is not None and proc.stdout is not None
    try:
        for entry in _tree_entries(root, commit, deadline):
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
            if counts['blob_bytes_read'] + size > MAX_TOTAL_BLOB_BYTES:
                raise ValueError('total blob byte limit exceeded; no complete report produced')
            if size > MAX_BLOB_BYTES:
                remaining = size
                while remaining:
                    chunk = proc.stdout.read(min(remaining, 65536))
                    if not chunk:
                        raise ValueError('truncated Git blob')
                    remaining -= len(chunk)
                if proc.stdout.read(1) != b'\n':
                    raise ValueError('invalid Git blob terminator')
                skipped.append({'path': path, 'reason': 'over-blob-limit', 'object': oid,
                                'bytes': size, 'limit_bytes': MAX_BLOB_BYTES})
                counts['blob_bytes_read'] += size
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
            path_lower = _ascii_lower(path)
            for match in OWNER_PATH.finditer(path):
                owner = match.group(1)
                start = len(path[:match.start(1)].encode('utf-8', errors='surrogateescape'))
                end = len(path[:match.end(1)].encode('utf-8', errors='surrogateescape'))
                findings.append(_finding(commit, path, oid, 'SERVER_OWNER_REFERENCE', owner,
                                          'path', None, start, end, start, None,
                                          file_class(path), owner.casefold() in known_owners))
                if len(findings) > MAX_FINDINGS:
                    raise ValueError('finding limit exceeded; no complete report produced')
            for term in terms:
                cursor = 0
                folded = path_lower
                needle = _ascii_lower(term)
                while (index := folded.find(needle, cursor)) >= 0:
                    start = len(path[:index].encode('utf-8', errors='surrogateescape'))
                    end = start + len(term.encode('utf-8'))
                    findings.append(_finding(commit, path, oid, 'ORG_LITERAL', term, 'path', None,
                                              start, end, start, None, file_class(path), None))
                    if len(findings) > MAX_FINDINGS:
                        raise ValueError('finding limit exceeded; no complete report produced')
                    cursor = index + max(len(needle), 1)
            # Git text lines are delimited only by LF; str.splitlines() also
            # treats Unicode separators as boundaries and shifts Git locations.
            absolute_line_start = 0
            for line_number, line in enumerate(text.split('\n'), 1):
                content_line = line[:-1] if line.endswith('\r') else line
                line_bytes = content_line.encode('utf-8')
                line_hash = hashlib.sha256(line_bytes).hexdigest()
                folded = _ascii_lower(content_line)
                for term in terms:
                    needle = _ascii_lower(term)
                    cursor = 0
                    while (index := folded.find(needle, cursor)) >= 0:
                        start = len(content_line[:index].encode('utf-8'))
                        end = start + len(term.encode('utf-8'))
                        findings.append(_finding(commit, path, oid, 'ORG_LITERAL', term, 'content',
                                                  line_number, start, end, absolute_line_start + start, line_hash,
                                                  file_class(path), None))
                        if len(findings) > MAX_FINDINGS:
                            raise ValueError('finding limit exceeded; no complete report produced')
                        cursor = index + max(len(needle), 1)
                for match in OWNER_PATH.finditer(content_line):
                    owner = match.group(1)
                    start = len(content_line[:match.start(1)].encode('utf-8'))
                    end = len(content_line[:match.end(1)].encode('utf-8'))
                    findings.append(_finding(commit, path, oid, 'SERVER_OWNER_REFERENCE', owner,
                                              'content', line_number, start, end, absolute_line_start + start, line_hash,
                                              file_class(path), owner.casefold() in known_owners))
                    if len(findings) > MAX_FINDINGS:
                        raise ValueError('finding limit exceeded; no complete report produced')
                absolute_line_start += len(line.encode('utf-8')) + 1
    finally:
        killer.cancel()
        try:
            proc.stdin.close()
        except OSError:
            pass
        proc.stdout.close()
        try:
            code = proc.wait(timeout=max(0.1, min(5, deadline - time.monotonic())))
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)
            raise ValueError('Git batch did not exit')
        if proc.stderr:
            proc.stderr.close()
    if code:
        raise ValueError('Git batch failed')
    counts['findings'] = len(findings)
    counts['finding_paths'] = len({x['path'] for x in findings})
    source = json.dumps(manifest, sort_keys=True, separators=(',', ':'), ensure_ascii=True).encode()
    rules = json.dumps({'terms': terms, 'unknown_owner_rule': OWNER_PATH.pattern},
                       sort_keys=True, separators=(',', ':')).encode()
    tool_sha = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise ValueError('scan time limit exceeded; no complete report produced')
    try:
        git_version = subprocess.run(['git', '--version'], capture_output=True, text=True,
                                      timeout=min(10, remaining), check=True).stdout.strip()
    except subprocess.TimeoutExpired as exc:
        raise ValueError('scan time limit exceeded; no complete report produced') from exc
    return {'schema_version': 2, 'source_sha': commit,
            'profile_sha256': hashlib.sha256(source).hexdigest(),
            'rules_sha256': hashlib.sha256(rules).hexdigest(), 'tool_sha256': tool_sha,
            'captured_at': datetime.now(timezone.utc).isoformat(timespec='seconds'),
            'versions': {'tool': TOOL_VERSION, 'python': platform.python_version(), 'git': git_version},
            'limits': {'max_blob_bytes': MAX_BLOB_BYTES, 'max_total_blob_bytes': MAX_TOTAL_BLOB_BYTES,
                       'max_tree_entries': MAX_TREE_ENTRIES,
                       'max_tree_listing_bytes': MAX_TREE_LISTING_BYTES,
                       'max_tree_entry_bytes': 1024 * 1024,
                       'max_findings': MAX_FINDINGS,
                       'max_scan_seconds': MAX_SCAN_SECONDS},
            'coverage_complete': not skipped, 'terms': list(terms),
            'coverage': dict(counts), 'skipped_or_partial': skipped,
            'method': 'Committed Git tree only; literal triage with byte spans and hashes; no raw matching lines, working-tree reads, network, or semantic/live coverage claim.',
            'files': files, 'findings': findings}


def plan(manifest: dict[str, Any], owner: str | None = None,
         owner_id: int | None = None) -> dict[str, Any]:
    validate_manifest(manifest)
    source = manifest['source']
    if owner is None and owner_id is None:
        destination = manifest.get('destination')
        owner = destination.get('owner') if isinstance(destination, dict) else None
        owner_id = destination.get('owner_id') if isinstance(destination, dict) else None
    elif not valid_owner(owner) or not positive_id(owner_id):
        raise ValueError('target owner and positive numeric ID must be supplied together')
    target = None
    if owner is not None or owner_id is not None:
        if not valid_owner(owner) or not positive_id(owner_id):
            raise ValueError('target owner slug and positive numeric ID required; verify with GitHub first')
        if owner.casefold() == source['owner'].casefold() or owner_id == source['owner_id']:
            raise ValueError('target must be a different organization, not a case-only rename')
        target = {'owner': owner, 'owner_id': owner_id, 'name': source['name'],
                  'repository_id': source['repository_id'], 'visibility': 'private',
                  'identity_status': 'UNVERIFIED'}
    digest = hashlib.sha256(json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    return {'schema_version': 2, 'status': 'PLANNING_ONLY_NOT_AUTHORIZED',
            'manifest_sha256': digest, 'source': source,
            'target': target,
            'predicted_immutable_sub_prefix': (f"repo:{owner}@{owner_id}/{source['name']}@{source['repository_id']}"
                                               if target else None),
            'migration_ready': False, 'authorization_verified': False,
            'warning': 'Identity and subject are unverified predictions. No transfer, patch, dispatch, credential change, or readiness approval was performed.',
            'unchanged_dependencies': manifest['dependencies'],
            'gates': [{'id': gate, 'status': 'UNKNOWN', 'owner': None, 'evidence': None,
                       'valid_until': None} for gate in GATES]}


def compare(before: dict[str, Any], after: dict[str, Any]) -> dict[str, Any]:
    """Pure diff: no approval. Additions/removals and changed blob bytes are explicit."""
    for doc in (before, after):
        if doc.get('schema_version') != 2 or not isinstance(doc.get('files'), list):
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
    planner.add_argument('--target-owner')
    planner.add_argument('--target-owner-id', type=int)
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
