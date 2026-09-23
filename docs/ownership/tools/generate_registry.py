#!/usr/bin/env python3
"""Generate the ownership population registry and Markdown index.

This is an inventory/readback tool. It never upgrades a package to approved,
never invents a reviewer, and never publishes an issue.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

TOOLS = Path(__file__).resolve().parent
ROOT = TOOLS.parents[2]
sys.path.insert(0, str(TOOLS))
from check_docs import validate, frontmatter, metrics  # noqa: E402
from prepare_census import skill_slug  # noqa: E402
from ownership_gate import record_errors, safe_path  # noqa: E402
from publication_gate import marker, publication_prerequisite_errors, record_readback  # noqa: E402

KINDS = {
    'SKILL.md': 'skill',
    'REFERENCE.md': 'reference',
    'BLAST_RADIUS.md': 'blast_radius',
    'MAINTENANCE.md': 'maintenance',
}
PILOTS = ('corelink-hash', 'corelink-billing', 'corelink-server',
          'corelink-cf-bindings', 'e2e-billing-flow')
CALIBRATION = 'docs/ownership/evidence/revision-1.4/PILOT-CALIBRATION-READBACK-20260923-CURRENT.md'


def verify_observed_main(root: Path, observed_main: str) -> None:
    """Refuse a stale pin or an origin/main ref that has not caught up to origin."""
    try:
        top = subprocess.run(['git', 'rev-parse', '--show-toplevel'], cwd=root, check=True,
                             capture_output=True, text=True, timeout=30).stdout.strip()
        if Path(top).resolve() != root.resolve():
            raise ValueError('registry root is not the repository root')
        fetched = subprocess.run(['git', 'rev-parse', '--verify', 'refs/remotes/origin/main^{commit}'],
                                 cwd=root, check=True, capture_output=True, text=True,
                                 timeout=30).stdout.strip()
        remote = subprocess.run(['git', 'ls-remote', 'origin', 'refs/heads/main'], cwd=root,
                                check=True, capture_output=True, text=True, timeout=30).stdout.strip()
    except (OSError, subprocess.SubprocessError) as exc:
        raise ValueError(f'cannot verify current origin/main: {exc}') from exc
    match = re.fullmatch(r'([0-9a-f]{40})\s+refs/heads/main', remote)
    if not match:
        raise ValueError('origin main did not return one exact commit')
    if fetched != match.group(1):
        raise ValueError(f'fetched origin/main is stale: {fetched} != {match.group(1)}')
    if observed_main != fetched:
        raise ValueError(f'observed-main is stale: {observed_main} != {fetched}')


def calibration_errors(root: Path, path: str = CALIBRATION) -> list[str]:
    table = root / path
    if not table.is_file():
        return [f'missing calibration readback: {path}']
    rows = {}
    capacities = {}
    for line in table.read_text(encoding='utf-8').splitlines():
        cells = [cell.strip() for cell in line.strip().strip('|').split('|')]
        if len(cells) not in (4, 6) or not cells[0].startswith('`'):
            continue
        package = cells[0].strip('`')
        if package not in PILOTS:
            continue
        target = rows if len(cells) == 6 else capacities
        if package in target:
            return [f'duplicate calibration row: {package}']
        target[package] = cells[1:]
    errors = []
    for package in PILOTS:
        if package not in rows:
            errors.append(f'missing calibration row: {package}')
            continue
        if package not in capacities:
            errors.append(f'{package}: missing material population/capacity decision')
        else:
            material, overflow, decision = capacities[package]
            if not material or overflow not in ('NO', 'YES') or not decision:
                errors.append(f'{package}: incomplete material population/capacity decision')
        profile, *reported = rows[package]
        paths = [root / f'.claude/skills/own-{package}/SKILL.md'] + [
            root / f'docs/ownership/crates/{package}/{name}.md'
            for name in ('REFERENCE', 'BLAST_RADIUS', 'MAINTENANCE')]
        if any(not artifact.is_file() for artifact in paths):
            errors.append(f'{package}: missing pilot artifact')
            continue
        reference_meta, _, front_errors = frontmatter(paths[1].read_text(encoding='utf-8'))
        if front_errors or profile != reference_meta.get('profile'):
            errors.append(f'{package}: calibration profile differs from reference')
        for artifact, cell in zip(paths, reported):
            match = re.fullmatch(r'([0-9,]+) / ([0-9,]+) / ([0-9,]+)', cell)
            if not match:
                errors.append(f'{package}: malformed calibration metric: {artifact.name}')
                continue
            observed = metrics(artifact.read_text(encoding='utf-8'))
            expected = tuple(int(value.replace(',', '')) for value in match.groups())
            if expected != (observed['lines'], observed['words'], observed['bytes']):
                errors.append(f'{package}: stale calibration metric: {artifact.name}')
    return errors


def review_state(root: Path, package: str, manifest: str) -> tuple[str, dict]:
    path = root / 'docs/ownership/records' / f'{package}.json'
    if not path.is_file():
        return 'UNVERIFIED', {}
    try:
        record = json.loads(path.read_text(encoding='utf-8'))
        if (record.get('package') != package or record.get('manifest') != manifest
                or record.get('record_path') != str(path.relative_to(root))):
            return 'INVALID_REVIEW_RECORD', {}
        if record_errors(record, root):
            return 'INVALID_REVIEW_RECORD', {}
        subprocess.run(['git', 'merge-base', '--is-ancestor', record['source_commit'], 'HEAD'],
                       cwd=root, check=True, capture_output=True, timeout=30)
        for source in record['sources']:
            safe_path(root, source['path'])
            archived = subprocess.run(['git', 'show',
                                       record['source_commit'] + ':' + source['path']],
                                      cwd=root, check=True, capture_output=True, timeout=30).stdout
            if hashlib.sha256(archived).hexdigest() != source['sha256']:
                return 'INVALID_REVIEW_RECORD', {}
        return 'REVIEW_EVIDENCE_CONSISTENT', {
            item['kind']: item['verdict'] for item in record['artifacts']}
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
        return 'INVALID_REVIEW_RECORD', {}


def publication_states(root: Path) -> dict[str, str]:
    path = root / 'docs/ownership/plans/publication-ledger.json'
    if not path.is_file():
        return {}
    ledger = json.loads(path.read_text(encoding='utf-8'))
    states = {}
    for item in ledger['items']:
        manifest = item['manifest']
        if manifest in states:
            raise ValueError(f'duplicate manifest in publication ledger: {manifest}')
        state = item['state']
        if state in ('CONFIRMED', 'REUSED'):
            readback_path = item.get('readback_path')
            if not readback_path:
                states[manifest] = 'READBACK_REQUIRED'
                continue
            if publication_prerequisite_errors(item, item.get('repository'), root):
                states[manifest] = 'INVALID_PREREQUISITES'
                continue
            snapshot_file = (root / readback_path).resolve()
            if not snapshot_file.is_relative_to(root.resolve()) or not snapshot_file.is_file():
                states[manifest] = 'INVALID_READBACK'
                continue
            try:
                snapshot_bytes = snapshot_file.read_bytes()
                if (not re.fullmatch(r'[0-9a-f]{64}', str(item.get('readback_sha256', '')))
                        or hashlib.sha256(snapshot_bytes).hexdigest() != item['readback_sha256']):
                    states[manifest] = 'INVALID_READBACK'
                    continue
                issue = json.loads(snapshot_bytes.decode('utf-8'))
                confirmed = record_readback(item, issue)
                body_path = (root / 'docs/ownership' / item['body_path']).resolve()
                if (not body_path.is_relative_to(root.resolve()) or not body_path.is_file()
                        or hashlib.sha256(body_path.read_bytes()).hexdigest() != item['body_sha256']
                        or marker(manifest) not in body_path.read_text(encoding='utf-8')
                        or confirmed['issue_number'] != item['issue_number']
                        or confirmed['issue_url'] != item['issue_url']):
                    states[manifest] = 'INVALID_READBACK'
                else:
                    states[manifest] = 'PUBLISHED' if state == 'CONFIRMED' else 'REUSED'
            except (OSError, ValueError, KeyError, TypeError, UnicodeError):
                states[manifest] = 'INVALID_READBACK'
        elif state == 'UNCERTAIN':
            states[manifest] = 'UNCERTAIN'
        elif state in ('PENDING', 'BLOCKED'):
            states[manifest] = 'NOT_PUBLISHED'
        else:
            states[manifest] = 'INVALID_LEDGER_STATE'
    return states


def build(root: Path, observed_main: str) -> dict:
    packages = []
    publications = publication_states(root)
    for skill_path in sorted((root / '.claude/skills').glob('own-*/SKILL.md')):
        fm, _, errors = frontmatter(skill_path.read_text(encoding='utf-8'))
        if errors or not isinstance(fm, dict):
            raise ValueError(f'invalid skill frontmatter: {skill_path}')
        skill_meta = fm.get('metadata', {}) if isinstance(fm.get('metadata'), dict) else {}
        skill_package = skill_meta.get('package')
        reference_path = root / f"docs/ownership/crates/{skill_package or skill_path.parent.name.removeprefix('own-')}/REFERENCE.md"
        rfm, _, rerrors = frontmatter(reference_path.read_text(encoding='utf-8')) if reference_path.is_file() else ({}, -1, ['missing reference'])
        package = rfm.get('package')
        manifest = rfm.get('manifest')
        if not isinstance(package, str) or not isinstance(manifest, str):
            raise ValueError(f'skill identity missing: {skill_path}')
        if skill_package and skill_package != package:
            raise ValueError(f'skill/reference package mismatch: {skill_path}')
        expected = {
            'skill': str(skill_path.relative_to(root)),
            'reference': f'docs/ownership/crates/{package}/REFERENCE.md',
            'blast_radius': f'docs/ownership/crates/{package}/BLAST_RADIUS.md',
            'maintenance': f'docs/ownership/crates/{package}/MAINTENANCE.md',
        }
        artifacts = {}
        errors_by_kind = {}
        source_commit = None
        profile = None
        metadata_by_kind = {}
        for filename, kind in KINDS.items():
            path = root / expected[kind]
            if not path.is_file():
                errors_by_kind[kind] = ['missing artifact']
                continue
            text = path.read_text(encoding='utf-8')
            afm, _, ferr = frontmatter(text)
            metadata_by_kind[kind] = afm.get('metadata', {}) if kind == 'skill' else afm
            if kind != 'skill':
                source_commit = source_commit or afm.get('source_commit')
                profile = profile or afm.get('profile')
            report = validate(text, kind, profile or 'S', path=path.resolve(), root=root.resolve())
            errors_by_kind[kind] = ferr + report['errors']
            artifacts[kind] = {'path': expected[kind], 'structural': not errors_by_kind[kind]}
        expected_meta = {
            'package': package,
            'manifest': manifest,
            'source_commit': metadata_by_kind.get('reference', {}).get('source_commit'),
            'profile': metadata_by_kind.get('reference', {}).get('profile'),
            'evidence_set': metadata_by_kind.get('reference', {}).get('evidence_set'),
        }
        integrity_errors = []
        for kind, meta in metadata_by_kind.items():
            # The skill template intentionally carries package/manifest,
            # source-commit and evidence-set in its nested metadata block, but
            # not the document profile (S/H is an artifact-document concern).
            # Do not turn that deliberate schema difference into a false
            # cross-artifact integrity failure.
            required_keys = expected_meta.keys() if kind != 'skill' else (
                'package', 'manifest', 'source_commit', 'evidence_set')
            key_map = {'source-commit': 'source_commit', 'evidence-set': 'evidence_set'} if kind == 'skill' else {}
            for key in required_keys:
                expected_value = expected_meta[key]
                actual_key = next((k for k, v in key_map.items() if v == key), key)
                if not meta.get(actual_key):
                    integrity_errors.append(f'{kind}: missing {actual_key}')
                elif meta.get(actual_key) != expected_value:
                    integrity_errors.append(f'{kind}: {actual_key} differs from reference')
        if integrity_errors:
            errors_by_kind.setdefault('integrity', []).extend(integrity_errors)
        structural = not any(errors_by_kind.values())
        cold_review, review_verdicts = review_state(root, package, manifest)
        packages.append({
            'package': package,
            'manifest': manifest,
            'skill_slug': skill_slug(package),
            'source_commit': source_commit,
            'profile': profile,
            'artifacts': artifacts,
            'structural': 'PASS' if structural else 'FAIL',
            'artifact_integrity': 'PASS' if not integrity_errors else 'FAIL',
            'cold_review': cold_review,
            'review_verdicts': review_verdicts,
            'publication': publications.get(manifest, 'NOT_PUBLISHED'),
            'errors': errors_by_kind if not structural else {},
        })
    if len(packages) != len({p['package'] for p in packages}):
        raise ValueError('duplicate package in skill population')
    census_path = root / 'docs/ownership/evidence/revision-1.4/census.json'
    if not census_path.is_file():
        raise ValueError(f'missing population census: {census_path}')
    census = json.loads(census_path.read_text(encoding='utf-8'))
    expected_population = {(p['package'], p['manifest']) for p in census['packages']}
    actual_population = {(p['package'], p['manifest']) for p in packages}
    if (not expected_population or len(expected_population) != len(census['packages'])
            or actual_population != expected_population):
        raise ValueError('generated population differs from census')
    calibration = calibration_errors(root)
    return {
        'schema': 'corelink-ownership-population-registry/1',
        'observed_main': observed_main,
        'population_count': len(packages),
        'publication_count': sum(p['publication'] in ('PUBLISHED', 'REUSED') for p in packages),
        'calibration': 'PASS' if not calibration else 'STALE',
        'calibration_errors': calibration,
        'packages': sorted(packages, key=lambda p: (p['package'], p['manifest'])),
        'not_proven': ['semantic completeness', 'independent cold review', 'Cargo/runtime',
                       'GitHub deduplication', 'issue publication', 'repository merge status'],
    }


def markdown(registry: dict) -> str:
    lines = [
        '# Ownership — generated population registry', '',
        f"Observed `main`: `{registry['observed_main']}`. Population: **{registry['population_count']}**.",
        'This index is generated from the four artifact paths. `PASS` is structural only; '
        'it is not cold approval, runtime evidence, or publication.', '',
        '| Package | Manifest | Structural | Cold review | Publication |',
        '|---|---|---|---|---|',
    ]
    for item in registry['packages']:
        package = item['package']
        lines.append(
            f"| `{package}` | `{item['manifest']}` | {item['structural']} | "
            f"{item['cold_review']} | {item['publication']} |"
        )
    return '\n'.join(lines) + '\n'


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--root', type=Path, default=ROOT)
    ap.add_argument('--observed-main', required=True)
    ap.add_argument('--json-output', type=Path, required=True)
    ap.add_argument('--markdown-output', type=Path, required=True)
    args = ap.parse_args()
    if not re.fullmatch(r'[0-9a-f]{40}', args.observed_main):
        ap.error('observed-main must be a 40-character commit SHA')
    try:
        root = args.root.resolve()
        verify_observed_main(root, args.observed_main)
        registry = build(root, args.observed_main)
        if registry['calibration'] != 'PASS':
            raise ValueError('calibration readback is stale: ' + '; '.join(registry['calibration_errors']))
        verify_observed_main(root, args.observed_main)
    except (OSError, ValueError, KeyError, TypeError) as exc:
        print(f'BLOCKED: {exc}', file=sys.stderr)
        return 1
    args.json_output.write_text(json.dumps(registry, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    args.markdown_output.write_text(markdown(registry), encoding='utf-8')
    print(json.dumps({'population_count': registry['population_count'], 'publication_count': registry['publication_count'],
                      'calibration': registry['calibration']}))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
