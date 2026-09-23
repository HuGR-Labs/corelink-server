#!/usr/bin/env python3
"""Read-only publication preflight and timeout reconciliation. NEVER creates issues.

Inputs are snapshots/evidence prepared through the authorized GitHub connection.
A complete boolean is an operator attestation, not a substitute for fetching all
pages. Plans with pending prerequisites stay blocked. See COMMON.md.
"""
from __future__ import annotations
import argparse
import copy
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

GITHUB_REPO_URL = re.compile(r'https://github\.com/([^/]+/[^/]+)/')
ROOT = Path(__file__).resolve().parents[3]
STANDARD_PATH = 'docs/ownership/STANDARD.md'
REQUIRED_GATES = frozenset(('census', 'context', 'capacity', 'shared-contract',
                            'deduplication', 'backlog'))


def frozen_standard_error(url: str, repository: str, root: Path = ROOT) -> str | None:
    prefix = f'https://github.com/{repository}/blob/'
    if not url.startswith(prefix):
        return 'frozen contract repository mismatch'
    match = re.fullmatch(r'([0-9a-f]{40})/' + re.escape(STANDARD_PATH), url[len(prefix):])
    if not match:
        return 'frozen contract must point to the standard at a commit SHA'
    commit = match.group(1)
    try:
        standard = (root / STANDARD_PATH).read_bytes()
        kind = subprocess.run(['git', 'cat-file', '-t', commit], cwd=root, check=True,
                              capture_output=True, text=True, timeout=30).stdout.strip()
        if kind != 'commit':
            return 'frozen contract SHA is not a commit'
        committed = subprocess.run(['git', 'show', f'{commit}:{STANDARD_PATH}'], cwd=root,
                                   check=True, capture_output=True, timeout=30).stdout
        fetched = subprocess.run(['git', 'rev-parse', '--verify', 'refs/remotes/origin/main^{commit}'],
                                 cwd=root, check=True, capture_output=True, text=True,
                                 timeout=30).stdout.strip()
        remote = subprocess.run(['git', 'ls-remote', 'origin', 'refs/heads/main'], cwd=root,
                                check=True, capture_output=True, text=True, timeout=30).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return 'frozen contract commit or standard path does not exist'
    if committed != standard:
        return 'frozen contract does not match current standard bytes'
    remote_match = re.fullmatch(r'([0-9a-f]{40})\s+refs/heads/main', remote)
    if not remote_match or fetched != remote_match.group(1):
        return 'frozen contract origin/main readback is stale'
    published = subprocess.run(['git', 'merge-base', '--is-ancestor', commit, fetched],
                               cwd=root, capture_output=True, timeout=30)
    if published.returncode != 0:
        return 'frozen contract commit is not on published origin/main'
    return None


def publication_prerequisite_errors(item: dict, repository: str | None,
                                    root: Path = ROOT) -> list[str]:
    """Use the same frozen-standard and six-gate contract for preflight and readback."""
    errors = []
    if not isinstance(repository, str) or not re.fullmatch(r'[^/\s]+/[^/\s]+', repository):
        errors.append('canonical repository identity is required')
    if type(item.get('repository_id')) is not int or item['repository_id'] < 1:
        errors.append('canonical repository ID is required')
    contract = item.get('contract')
    if not isinstance(contract, dict) or contract.get('frozen') is not True or not isinstance(contract.get('url'), str):
        errors.append('frozen contract with immutable GitHub URL required')
    elif repository:
        contract_error = frozen_standard_error(contract['url'], repository, root)
        if contract_error:
            errors.append(contract_error)
    try:
        standard_text = (root / STANDARD_PATH).read_text(encoding='utf-8')
        versions = re.findall(r'^\*\*Versão:\*\*\s*([A-Za-z0-9][A-Za-z0-9._-]*)',
                              standard_text, flags=re.MULTILINE)
        if (len(versions) != 1 or not isinstance(contract, dict)
                or contract.get('version') != versions[0]):
            errors.append('frozen contract version differs from current standard')
    except (OSError, UnicodeError):
        errors.append('current standard version unavailable')
    gates = item.get('gates')
    if (not isinstance(gates, dict) or set(gates) != REQUIRED_GATES
            or any(not isinstance(gates[k], dict) or gates[k].get('state') != 'PASS'
                   or not gates[k].get('evidence') for k in REQUIRED_GATES if k in gates)):
        errors.append('publication prerequisites missing or not passed')
    return errors

def _repo_from_url(url: object) -> str | None:
    match = GITHUB_REPO_URL.match(str(url))
    return match.group(1) if match else None


def marker(manifest: str) -> str:
    if not manifest.endswith('Cargo.toml') or '..' in Path(manifest).parts or Path(manifest).is_absolute():
        raise ValueError('invalid manifest')
    # Preserve the v1 marker across policy revisions: upgrading must not duplicate tickets.
    return f'<!-- corelink-ownership:v1:manifest={manifest} -->'


def publication_decision(item: dict, snapshot: dict, *, expected_repo_id: int,
                         expected_repository: str | None = None,
                         contract_root: Path = ROOT) -> dict:
    errors=[]
    manifest=item['manifest']
    expected_repository = expected_repository or item.get('repository')
    if not expected_repository or not re.fullmatch(r'[^/\s]+/[^/\s]+', str(expected_repository)):
        errors.append('canonical repository identity is required')
    if not item.get('repository'):
        errors.append('item repository identity missing')
    elif expected_repository and item.get('repository') != expected_repository:
        errors.append('item repository identity mismatch')
    if snapshot.get('repository_id')!=expected_repo_id or item.get('repository_id')!=expected_repo_id:
        errors.append('repository identity mismatch')
    if snapshot.get('scope')!='open-and-closed' or snapshot.get('complete') is not True:
        errors.append('complete open-and-closed snapshot required')
    if item.get('state') not in ('PENDING','UNCERTAIN','CONFIRMED','REUSED','BLOCKED'):
        errors.append('unknown publication state')
    if not re.fullmatch(r'[0-9a-f]{40}',str(item.get('source_commit',''))):
        errors.append('invalid source baseline')
    if not re.fullmatch(r'[0-9a-f]{64}',str(item.get('body_sha256',''))):
        errors.append('missing issue body fingerprint')
    errors.extend(publication_prerequisite_errors(item, expected_repository, contract_root))
    if item.get('state')=='BLOCKED': errors.append('item explicitly blocked')
    markers=[marker(manifest)]+[marker(m) for m in item.get('former_manifests',[])]
    candidates=[i for i in snapshot.get('issues',[]) if any(m in i.get('body','') for m in markers)]
    for candidate in candidates:
        if candidate.get('repository_id') != expected_repo_id:
            errors.append('issue marker matched a different repository')
        if expected_repository and _repo_from_url(candidate.get('url')) != expected_repository:
            errors.append('issue URL repository mismatch')
        number = candidate.get('number')
        if (type(number) is not int or number < 1 or not expected_repository
                or candidate.get('url') != f'https://github.com/{expected_repository}/issues/{number}'):
            errors.append('matched issue lacks exact canonical issue identity')
    if any('pull_request' in i or i.get('is_pull_request') for i in candidates):
        errors.append('marker on pull request; issue identity must be reconciled')
    matches=[i for i in candidates if 'pull_request' not in i and not i.get('is_pull_request')]
    if len(matches)>1:
        errors.append('duplicate marker matches; explicit reconciliation required')
    if errors:
        return {'action':'BLOCKED','errors':sorted(set(errors)),'manifest':manifest}
    if matches:
        found=matches[0]
        if item.get('issue_number') is not None and item['issue_number']!=found['number']:
            return {'action':'BLOCKED','errors':['recorded issue number differs from marker match'],'manifest':manifest}
        return {'action':'REUSE_OR_RECONCILE','manifest':manifest,'number':found['number'],
                'state':found['state'],'url':found['url'],
                'note':'Read complete issue; never reopen or rewrite a closed ticket automatically.'}
    if item['state'] in ('UNCERTAIN','CONFIRMED','REUSED'):
        return {'action':'BLOCKED','manifest':manifest,
                'errors':['previous write outcome not reconciled; absence does not authorize blind retry']}
    return {'action':'ELIGIBLE_FOR_SERIAL_CREATE','manifest':manifest,
            'note':'Not a publication. Recheck baseline/snapshot immediately before an authorized write.'}


def record_readback(item: dict, issue: dict) -> dict:
    """Promote only an exact readback, not an API request or an ambiguous timeout."""
    if issue.get('repository_id')!=item.get('repository_id'):
        raise ValueError('readback repository mismatch')
    body=issue.get('body','')
    if marker(item['manifest']) not in body:
        raise ValueError('readback marker missing')
    if hashlib.sha256(body.encode()).hexdigest()!=item['body_sha256']:
        raise ValueError('readback body differs from intended body')
    if type(issue.get('number')) is not int or issue['number']<1 or not issue.get('url'):
        raise ValueError('readback lacks canonical issue identity')
    expected_repository = item.get('repository')
    if not expected_repository:
        raise ValueError('readback canonical repository identity missing')
    if _repo_from_url(issue.get('url')) != expected_repository:
        raise ValueError('readback URL repository mismatch')
    if (issue.get('is_pull_request') or issue.get('pull_request')
            or issue['url'] != f'https://github.com/{expected_repository}/issues/{issue["number"]}'):
        raise ValueError('readback is not a canonical issue URL')
    result=copy.deepcopy(item)
    result.update({'state':'CONFIRMED','issue_number':issue['number'],'issue_url':issue['url']})
    return result


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--item',type=Path,required=True);p.add_argument('--snapshot',type=Path,required=True)
    p.add_argument('--repository-id',type=int,required=True)
    p.add_argument('--repository', required=True, help='canonical OWNER/REPO; strict URL identity')
    a=p.parse_args()
    try:
        result=publication_decision(json.loads(a.item.read_text()),json.loads(a.snapshot.read_text()),expected_repo_id=a.repository_id,expected_repository=a.repository)
        print(json.dumps(result,ensure_ascii=False,indent=2));return int(result['action']=='BLOCKED')
    except (OSError,KeyError,TypeError,ValueError) as e:
        print(f'BLOCKED: {e}',file=sys.stderr);return 2

if __name__=='__main__':raise SystemExit(main())
