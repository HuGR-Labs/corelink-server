#!/usr/bin/env python3
"""Validate CO-1 records and derive an order-independent registry.

This is an evidence-consistency gate, NOT a signature service, semantic oracle,
identity provider or cold reviewer. Reads Git-tracked/worktree files; no network,
production access, or execution of document commands. See COMMON.md.
"""
from __future__ import annotations
import argparse
import fnmatch
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

from jsonschema import Draft202012Validator, FormatChecker

TOOLS = Path(__file__).resolve().parent
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))
from prepare_census import skill_slug
from check_docs import validate as check_document, explicit_ids, frontmatter

KINDS = ('skill', 'reference', 'blast_radius', 'maintenance')
CHECKS = {
    'skill': ('triggers', 'authority', 'navigation'),
    'reference': ('contracts', 'invariants', 'source-trace', 'navigation'),
    'blast_radius': ('independent-census', 'non-cargo-edges', 'exclusions', 'navigation'),
    'maintenance': ('procedure-scope', 'local-validation', 'failure-recovery', 'navigation'),
}
DEFAULT_SCHEMA = TOOLS.parent/'schemas/package-record.schema.json'
STANDARD_PATH = 'docs/ownership/STANDARD.md'


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def canonical_digest(value) -> str:
    return digest(json.dumps(value, ensure_ascii=False, sort_keys=True,
                             separators=(',', ':')).encode())


def safe_path(root: Path, relative: str) -> Path:
    if (not relative or Path(relative).is_absolute() or '..' in Path(relative).parts
            or '\\' in relative):
        raise ValueError(f'unsafe relative path: {relative!r}')
    resolved = (root/relative).resolve()
    if not resolved.is_relative_to(root.resolve()):
        raise ValueError(f'path leaves root: {relative}')
    return resolved


def tracked_files(root: Path) -> list[str]:
    result = subprocess.run(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'],
                            cwd=root, check=True, capture_output=True, timeout=30)
    return sorted(set(x for x in result.stdout.decode('utf-8').split('\0') if x))


def discovery_snapshot(root: Path, rules: list[dict], population: list[str] | None = None) -> dict:
    """Literal-search fingerprints. New matching consumers invalidate the record.

    A search miss is not proof of semantic absence. Rules themselves are reviewed
    and frozen. Generated/dynamic edges and external repos require manual coverage.
    """
    files = tracked_files(root) if population is None else sorted(set(population))
    found = []
    for rule in sorted(rules, key=lambda r:r['id']):
        matches = []
        for path in files:
            if not any(fnmatch.fnmatchcase(path, glob) for glob in rule['globs']):
                continue
            file = safe_path(root, path)
            if not file.is_file():
                raise ValueError(f'listed discovery source missing: {path}')
            # An unreadable/non-UTF8 source is BLOCKED, never silently excluded.
            raw = file.read_bytes()
            text = raw.decode('utf-8')
            needles = sorted(n for n in rule['needles'] if n in text)
            if needles:
                matches.append({'path':path, 'needles':needles, 'sha256':digest(raw)})
        found.append({'id':rule['id'], 'matches':matches})
    return {'rules_sha256':canonical_digest(sorted(rules,key=lambda r:r['id'])), 'results':found}


def expected_paths(package: str, slug: str) -> dict[str, str]:
    base = f'docs/ownership/crates/{package}'
    return {'skill':f'.claude/skills/{slug}/SKILL.md',
            'reference':f'{base}/REFERENCE.md', 'blast_radius':f'{base}/BLAST_RADIUS.md',
            'maintenance':f'{base}/MAINTENANCE.md'}


def metadata_only_delta(reviewed: bytes, current: bytes, kind: str) -> set[str] | None:
    """Return changed allowed fields only when every other byte is unchanged."""
    try:
        before, after = reviewed.decode('utf-8'), current.decode('utf-8')
        old_meta, old_end, old_errors = frontmatter(before)
        new_meta, new_end, new_errors = frontmatter(after)
        if old_errors or new_errors:
            return None
        keys = {'package', 'manifest', 'source-commit', 'evidence-set'} if kind == 'skill' else {
            'package', 'manifest', 'source_commit', 'evidence_set'}
        old_fields = old_meta.get('metadata', {}) if kind == 'skill' else old_meta
        new_fields = new_meta.get('metadata', {}) if kind == 'skill' else new_meta
        if not isinstance(old_fields, dict) or not isinstance(new_fields, dict):
            return None
        changed = {key for key in keys if old_fields.get(key) != new_fields.get(key)}
        if not changed:
            return None
        def without_allowed(text: str, end: int) -> str:
            lines = text.splitlines(keepends=True)
            result = []
            in_skill_metadata = False
            for n, line in enumerate(lines):
                if kind == 'skill' and 0 < n < end and line and line[0] not in ' \t#\r\n':
                    # Only the direct children of the top-level metadata mapping
                    # may be removed before comparing every remaining byte.
                    in_skill_metadata = line.rstrip('\r\n') == 'metadata:'
                allowed = (in_skill_metadata and re.match(
                    r'  (?:package|manifest|source-commit|evidence-set):', line)
                    if kind == 'skill' else re.match(
                        r'(?:package|manifest|source_commit|evidence_set):', line))
                if n > end or not allowed:
                    result.append(line)
            return ''.join(result)
        if without_allowed(before, old_end) != without_allowed(after, new_end):
            return None
        return changed
    except UnicodeError:
        return None


def record_errors(record: dict, root: Path, *, population: list[str] | None = None,
                  schema_path: Path = DEFAULT_SCHEMA) -> list[str]:
    schema = json.loads(schema_path.read_text())
    errors = [f'schema:{"/".join(map(str,e.absolute_path))}: {e.message}'
              for e in Draft202012Validator(schema, format_checker=FormatChecker()).iter_errors(record)]
    if errors:
        return sorted(errors)
    package = record['package']
    slug = skill_slug(package)
    if record['skill_slug'] != slug:
        errors.append('skill slug differs from canonical normalizer')
    expected_uid = f"repo:{record['repository_id']}:{record['manifest']}"
    if record['package_uid'] != expected_uid:
        errors.append('package_uid differs from repository/manifest identity')
    sources = record['sources']
    if len({s['path'] for s in sources}) != len(sources):
        errors.append('duplicate source path')
    if record['manifest'] not in {s['path'] for s in sources}:
        errors.append('package manifest missing from source set')
    if record['standard']['path'] != STANDARD_PATH:
        errors.append('review standard path differs from canonical candidate')
    try:
        standard_text = (root/STANDARD_PATH).read_text(encoding='utf-8')
        versions = re.findall(r'^\*\*Versão:\*\*\s*([A-Za-z0-9][A-Za-z0-9._-]*)',
                              standard_text, flags=re.MULTILINE)
        if len(versions) != 1:
            errors.append('candidate standard version missing or ambiguous')
        elif record['standard']['version'] != versions[0]:
            errors.append('review standard version differs from canonical candidate')
    except (OSError, UnicodeError) as exc:
        errors.append(f'candidate standard unavailable: {exc}')
    evidence_by_id = {e['id']:e for e in record['evidence']}
    if len(evidence_by_id) != len(record['evidence']):
        errors.append('duplicate evidence ID')
    for group in (sources, record['evidence'], [record['standard']]):
        for entry in group:
            try:
                file = safe_path(root, entry['path'])
                if not file.is_file() or digest(file.read_bytes()) != entry['sha256']:
                    errors.append(f'missing or changed evidence/source: {entry["path"]}')
            except (ValueError, OSError) as exc:
                errors.append(str(exc))
    def evidence_refs(ids, context):
        if not ids or not set(ids) <= evidence_by_id.keys():
            errors.append(f'{context}: missing evidence references')
    review = record['review']
    if not review['fresh_context_confirmed']:
        errors.append('review has no fresh-context attestation')
    if review['reviewer'] == record['author']['identity']:
        errors.append('reviewer is the author')
    if review['session_id'] == record['author']['session_id']:
        errors.append('review shares author session')
    evidence_refs(review['independence_evidence'], 'independence')
    artifact_map = {a['kind']:a for a in record['artifacts']}
    if len(artifact_map) != 4 or set(artifact_map) != set(KINDS):
        errors.append('exactly four unique artifact kinds required')
    paths = expected_paths(package, slug)
    for kind, artifact in artifact_map.items():
        if artifact['path'] != paths[kind]:
            errors.append(f'{kind}: noncanonical path')
        if artifact['verdict'] != 'APPROVE':
            errors.append(f'{kind}: not approved by reviewer')
        checks = {c['requirement']:c for c in artifact['checks']}
        if len(checks) != len(artifact['checks']) or set(checks) != set(CHECKS[kind]):
            errors.append(f'{kind}: required review checks missing/duplicated/unknown')
        for check in artifact['checks']:
            if check['status'] != 'PASS':
                errors.append(f'{kind}: review requirement not passed: {check["requirement"]}')
            evidence_refs(check['evidence'], kind+'/'+check['requirement'])
        for finding in artifact['findings']:
            if finding['required'] and finding['state'] != 'RESOLVED':
                errors.append(f'{kind}: required finding open: {finding["id"]}')
            if finding['state'] == 'RESOLVED':
                evidence_refs(finding['resolution_evidence'], kind+'/'+finding['id'])
        try:
            file = safe_path(root, artifact['path'])
            raw = file.read_bytes()
            readback = artifact.get('metadata_readback')
            semantic_review = artifact.get('semantic_review')
            if readback is None:
                if digest(raw) != artifact['sha256']:
                    errors.append(f'{kind}: artifact bytes differ from reviewed bytes')
                if semantic_review is not None:
                    errors.append(f'{kind}: semantic_review snapshot needs metadata_readback')
            else:
                if digest(raw) != artifact['sha256'] or readback['sha256'] != artifact['sha256']:
                    errors.append(f'{kind}: metadata readback does not match current bytes')
                evidence_refs(readback['evidence'], kind+'/metadata-readback')
                if any(evidence_by_id.get(ref, {}).get('class') != 'REVIEW'
                       for ref in readback['evidence']):
                    errors.append(f'{kind}: metadata readback needs REVIEW evidence')
                if not semantic_review:
                    errors.append(f'{kind}: metadata readback lacks semantic review snapshot')
                else:
                    snapshot = evidence_by_id.get(semantic_review['snapshot_evidence'])
                    if not snapshot or snapshot['class'] != 'REVIEW':
                        errors.append(f'{kind}: semantic review snapshot needs REVIEW evidence')
                    else:
                        previous = safe_path(root, snapshot['path']).read_bytes()
                        if digest(previous) != semantic_review['sha256'] or digest(previous) != snapshot['sha256']:
                            errors.append(f'{kind}: semantic review snapshot hash differs')
                        actual_fields = metadata_only_delta(previous, raw, kind)
                        if actual_fields is None or actual_fields != set(readback['changed_fields']):
                            errors.append(f'{kind}: changed bytes exceed declared metadata-only fields')
            text = raw.decode('utf-8')
            result = check_document(text,kind,record['profile'],path=file,root=root)
            errors.extend(f'{kind}: {e}' for e in result['errors'])
            fm, _, _ = frontmatter(text)
            meta = fm.get('metadata', {}) if kind == 'skill' else fm
            for key, expected in [('package',package),('manifest',record['manifest'])]:
                if meta.get(key) != expected:
                    errors.append(f'{kind}: metadata {key} differs from record')
            ekey = 'evidence-set' if kind == 'skill' else 'evidence_set'
            if meta.get(ekey) != record['record_id']:
                errors.append(f'{kind}: evidence_set differs from record')
            skey = 'source-commit' if kind == 'skill' else 'source_commit'
            if meta.get(skey) != record['source_commit']:
                errors.append(f'{kind}: source_commit differs from record')
            if kind == 'maintenance':
                procedures = {x for x in explicit_ids(text) if re.fullmatch(r'proc-[0-9]{3,}',x)}
                declared = {p['id'].lower() for p in record['procedures']}
                if declared != procedures or len(declared) != len(record['procedures']):
                    errors.append('procedure population differs from maintenance document')
        except (OSError, UnicodeError, ValueError) as exc:
            errors.append(f'{kind}: {exc}')
    for proc in record['procedures']:
        evidence_refs(proc['review_evidence'], proc['id']+'/review')
        if proc['review_status'] != 'REVIEWED':
            errors.append(f'{proc["id"]}: procedure review incomplete')
        if proc['execution_status'] == 'EXECUTED_LOCAL':
            evidence_refs(proc['execution_evidence'], proc['id']+'/execution')
            if any(evidence_by_id.get(e,{}).get('class') != 'EXECUTED_LOCAL' for e in proc['execution_evidence']):
                errors.append(f'{proc["id"]}: execution evidence is not EXECUTED_LOCAL')
            if proc['mode'] == 'AUTHORIZED_OPERATION':
                errors.append(f'{proc["id"]}: local evidence cannot certify authorized operation')
            if proc['result'] != 'PASS':
                errors.append(f'{proc["id"]}: local execution failed')
        else:
            if not proc['limitations']:
                errors.append(f'{proc["id"]}: unexecuted procedure needs explicit limitations')
            if proc['required_for_acceptance']:
                errors.append(f'{proc["id"]}: required procedure has not been executed')
            if proc['execution_evidence'] or proc['result'] != 'NOT_EXECUTED':
                errors.append(f'{proc["id"]}: unexecuted state contradicts execution claim')
    rules = record['discovery']['rules']
    if len({r['id'] for r in rules}) != len(rules):
        errors.append('duplicate discovery rule ID')
    # Self-watching docs/evidence would recursively change their own fingerprint.
    protected = [a['path'] for a in record['artifacts']] + [record['record_path']]
    protected += [e['path'] for e in record['evidence']]
    for rule in rules:
        if any(fnmatch.fnmatchcase(p,g) for p in protected for g in rule['globs']):
            errors.append(f'{rule["id"]}: discovery watches its own docs/evidence')
    try:
        fresh = canonical_digest(discovery_snapshot(root,rules,population))
        if fresh != record['discovery']['snapshot_sha256']:
            errors.append('consumer/source discovery changed: new, removed or modified match')
    except (OSError,ValueError,UnicodeError,subprocess.SubprocessError) as exc:
        errors.append(f'discovery blocked: {exc}')
    if record['profile'] == 'H' and not (
        record['capacity']['atomic_relations'] > 40 or record['capacity']['semantic_modules'] > 20
        or record['capacity']['procedures'] > 8
    ):
        errors.append('H profile not justified by recorded counts')
    evidence_refs(record['capacity']['evidence'], 'capacity')
    for selection in record['build_selections']:
        evidence_refs(selection['basis_evidence'], selection['id']+'/build-selection')
    for relation in record['relations']:
        if record['package_uid'] not in (relation['consumer_uid'],relation['provider_uid']):
            errors.append(f'{relation["key"]}: record is neither consumer nor provider')
        if relation['contract_owner_uid'] not in (relation['consumer_uid'],relation['provider_uid']):
            # A contract may be held by a verified third party (external API,
            # schema registry, or composition platform). The boundary must
            # name that owner explicitly so a typo cannot masquerade as authority.
            owner=relation['contract_owner_uid']
            boundary=relation.get('external_boundary')
            if not (isinstance(boundary,str) and boundary.strip()
                    and (owner.startswith('external:') or owner.startswith('system:')
                         or owner.startswith('repo:'))
                    and 'surface=' in boundary and 'evidence=' in boundary):
                errors.append(f'{relation["key"]}: contract owner is neither endpoint nor verified external owner')
        if relation['contract_sha256'] not in {s['sha256'] for s in sources}:
            errors.append(f'{relation["key"]}: contract fingerprint is not anchored in source set')
        blast_path = safe_path(root, paths['blast_radius'])
        if blast_path.is_file() and relation['local_anchor'] not in explicit_ids(blast_path.read_text()):
            errors.append(f'{relation["key"]}: relation anchor absent from local blast document')
        if not relation['local_effect'].strip():
            errors.append(f'{relation["key"]}: missing local impact explanation')
    return sorted(set(errors))


def integrate(records: list[dict], root: Path, *, population: list[str] | None = None) -> dict:
    """Pure deterministic aggregation. Authors never merge a hand-edited registry."""
    entries, relations, errors = [], {}, []
    ids = [r.get('package_uid') for r in records]
    if not records or len(set(ids)) != len(ids):
        errors.append('empty population or duplicate package_uid')
    slugs = [r.get('skill_slug') for r in records]
    if len(set(slugs)) != len(slugs):
        errors.append('skill slug collision across records')
    for record in sorted(records,key=lambda r:str(r.get('package_uid',''))):
        local = record_errors(record,root,population=population)
        uid = record.get('package_uid')
        entries.append({'package_uid':uid,'package':record.get('package'),
                        'record_path':record.get('record_path'),
                        'record_sha256':canonical_digest(record),
                        'state':'BLOCKED' if local else 'EVIDENCE_CONSISTENT', 'errors':local})
        if any(e.startswith('schema:') for e in local):
            continue
        for relation in record.get('relations',[]):
            if not isinstance(relation,dict) or 'key' not in relation:
                continue
            signature = {k:v for k,v in relation.items() if k not in ('local_effect','local_anchor')}
            relations.setdefault(relation['key'],[]).append((uid,signature))
    for key, appearances in sorted(relations.items()):
        canonical = appearances[0][1]
        if any(sig != canonical for _,sig in appearances[1:]):
            errors.append(f'{key}: provider/consumer contract views disagree')
        endpoints = {canonical['consumer_uid'],canonical['provider_uid']}
        required = endpoints & set(ids)
        seen = [uid for uid,_ in appearances]
        if len(seen) != len(set(seen)) or set(seen) != required:
            errors.append(f'{key}: missing or duplicate endpoint view')
        # An external/non-onboarded endpoint must be visible, never silently fresh.
        if endpoints - set(ids) and not canonical.get('external_boundary'):
            errors.append(f'{key}: absent peer needs explicit external/non-onboarded boundary')
    return {'schema':'corelink-ownership-registry/1.1','packages':entries,
            'cross_package_errors':sorted(set(errors)),
            'consistent':not errors and all(e['state']=='EVIDENCE_CONSISTENT' for e in entries),
            'not_proven':['human identity','actual cold-context independence','semantic completeness',
                          'runtime behavior','repository merge/CI status']}



def render_index(registry: dict) -> str:
    """Markdown view derived from the same registry, never a manually merged index."""
    lines=['# Ownership — índice gerado', '',
           'Gerado dos registros. EVIDENCE_CONSISTENT não prova revisão independente ou runtime.', '',
           '| Package | Estado do gate | Registro |', '|---|---|---|']
    for row in sorted(registry['packages'],key=lambda r:str(r['package_uid'])):
        path=row['record_path']
        prefix='docs/ownership/'
        if not isinstance(path,str) or not path.startswith(prefix):
            raise ValueError('index record must live below docs/ownership')
        lines.append(f"| {row['package']} | {row['state']} | [Evidência]({path[len(prefix):]}) |")
    if registry['cross_package_errors']:
        lines += ['', '## Bloqueios entre packages', '']
        lines += ['- '+error for error in registry['cross_package_errors']]
    return '\n'.join(lines)+'\n'

def main() -> int:
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repo-root',type=Path,required=True)
    p.add_argument('records',type=Path,nargs='+')
    p.add_argument('--markdown-index',action='store_true',help='Render index to stdout instead of registry JSON')
    args=p.parse_args()
    try:
        records=[json.loads(f.read_text()) for f in args.records]
        for file,record in zip(args.records, records):
            if file.resolve() != safe_path(args.repo_root, record['record_path']):
                raise ValueError('record file path differs from record_path')
            subprocess.run(['git','merge-base','--is-ancestor',record['source_commit'],'HEAD'],
                           cwd=args.repo_root,check=True,capture_output=True,timeout=30)
            for source in record['sources']:
                safe_path(args.repo_root, source['path'])
                archived = subprocess.run(['git','show',record['source_commit']+':'+source['path']],
                    cwd=args.repo_root,check=True,capture_output=True,timeout=30).stdout
                if digest(archived) != source['sha256']:
                    raise ValueError('source not present at declared Git baseline: '+source['path'])
        report=integrate(records,args.repo_root.resolve())
        print(render_index(report) if args.markdown_index else json.dumps(report,ensure_ascii=False,sort_keys=True,indent=2),end='\n')
        return 0 if report['consistent'] else 1
    except (OSError,ValueError,TypeError,KeyError,subprocess.SubprocessError) as exc:
        print(f'BLOCKED: {exc}',file=sys.stderr);return 2

if __name__=='__main__':
    raise SystemExit(main())
