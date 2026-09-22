#!/usr/bin/env python3
"""Check captured preparation consistency, not semantic completeness or approval."""
from __future__ import annotations
import argparse
import copy
import json
from pathlib import Path, PurePosixPath
import shlex

FILES = ('census.json', 'source-packets.json', 'seeds.json', 'summary.json', 'metadata-commands.json')

def require(ok, message):
    if not ok:
        raise ValueError(message)

def validate(c, sources, seeds, summary, commands):
    packages = c['packages']
    names = {p['package']: p for p in packages}
    manifests = {p['manifest']: p for p in packages}
    require(packages and len(names) == len(manifests) == len(packages), 'population identity')
    require(set(names) == set(sources) == set(seeds), 'source/seed population')
    require(len(packages) == summary['eligible_packages'], 'eligible count')
    require(c['source_commit'] == summary['source_commit'] == summary['head_after'], 'baseline')
    require(summary['checkout_status'] == '' and summary['lock_unchanged'] is True, 'dirty capture')
    require(c['first_party_scope_fully_classified'] is True, 'scope unclassified')
    require(c['resolved_graph_produced'] is False and summary['runtime_verified'] is False, 'overclaim')
    executed = {tuple(x['command']) for x in commands if x['exit_code'] == 0}
    require(len(executed) == summary['metadata_commands_succeeded'], 'command count')
    inverse = {name: [] for name in names}
    owned = set()
    for name, p in names.items():
        s, seed = sources[name], seeds[name]
        require(s['manifest'] == seed['manifest'] == p['manifest'], 'manifest disagreement')
        require(s['source_commit'] == c['source_commit'], 'source baseline')
        require(p['skill_slug'] == 'own-' + name.replace('_', '-').lower(), 'slug')
        require(p['targets'] and p['runtime_verified'] is False, 'targets/runtime')
        require(p['semantic_relations_verified'] is False, 'unproved semantics')
        require(seed['issue_ready'] is False and seed['deep_semantic_relations_complete'] is False, 'unproved readiness')
        for key in ('verified_facts', 'okf_context', 'specific_risks', 'initial_commands', 'seed_evidence'):
            require(isinstance(seed[key], list) and bool(seed[key]), 'seed field: ' + key)
        target_keys = [(t['name'], tuple(t['kind']), t['src_path']) for t in p['targets']]
        require(len(target_keys) == len(set(target_keys)), 'duplicate target')
        require(s['rust_files'] == len(s['tracked_rust_paths']), 'source count')
        for path in s['tracked_rust_paths']:
            parsed = PurePosixPath(path)
            require(not parsed.is_absolute() and '..' not in parsed.parts, 'unsafe path')
            require(path.startswith(str(PurePosixPath(p['manifest']).parent) + '/'), 'source outside package')
            require(path not in owned, 'duplicate source ownership')
            owned.add(path)
        for d in p['declared_dependencies']:
            producer = manifests.get(d['local_manifest'])
            if producer:
                inverse[producer['package']].append((name, d['kind'], d['target_cfg'], d['optional'], d['alias']))
        for command in seed['initial_commands']:
            if command['status'] == 'EXECUTED':
                require(tuple(shlex.split(command['command'])) in executed, 'unexecuted command credited')
            else:
                require(command['status'] == 'NOT_EXECUTED', 'command status')
    for name, p in names.items():
        actual = [(x['package'], x['kind'], x['target_cfg'], x['optional'], x['alias']) for x in p['declared_workspace_consumers']]
        require(sorted(map(repr, actual)) == sorted(map(repr, inverse[name])), 'inverse mismatch: ' + name)
    targets = sum(len(p['targets']) for p in packages)
    edges = sum(len(x) for x in inverse.values())
    require(targets == summary['targets'], 'target count')
    require(edges == summary['declared_internal_edges'], 'edge count')
    require(len(owned) == summary['own_rust_files'], 'owned source count')
    other = c['other_tracked_manifests']
    require(len(other) + summary['workspace_packages'] == summary['tracked_manifests'], 'manifest count')
    allowed = {'first_party_independent', 'workspace_only_no_package', 'historical_archive'}
    require(all(x['classification'] in allowed and x['reason'] and x['evidence'] for x in other), 'scope decision')
    return {'result': 'PREPARATION_STRUCTURE_CONSISTENT', 'packages': len(packages), 'targets': targets, 'declared_edges': edges, 'publication_approved': False}

def self_test(data):
    results = [validate(*data)]
    mutations = [
        lambda d: d[0]['packages'].append(copy.deepcopy(d[0]['packages'][0])),
        lambda d: d[0]['packages'][0].update(runtime_verified=True),
        lambda d: d[0]['packages'][0]['targets'].clear(),
        lambda d: d[2].pop(next(iter(d[2]))),
        lambda d: d[2][next(iter(d[2]))].update(issue_ready=True),
        lambda d: d[3].update(lock_unchanged=False),
        lambda d: d[3].update(targets=d[3]['targets'] + 1),
        lambda d: d[3].update(declared_internal_edges=d[3]['declared_internal_edges'] + 1),
        lambda d: d[0].update(first_party_scope_fully_classified=False),
        lambda d: next(p for p in d[0]['packages'] if p['declared_workspace_consumers'])['declared_workspace_consumers'][0].update(package='not-a-real-package'),
        lambda d: d[2][next(iter(d[2]))]['initial_commands'][0].update(command='cargo test --workspace'),
    ]
    for number, mutate in enumerate(mutations, 1):
        changed = copy.deepcopy(data)
        mutate(changed)
        try:
            validate(*changed)
        except (ValueError, KeyError) as error:
            results.append({'case': number, 'rejected': True, 'reason': str(error)})
        else:
            raise AssertionError('mutation accepted: ' + str(number))
    return {'positive_controls': 1, 'rejected_negative_cases': len(mutations), 'cases': results}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    try:
        data = tuple(json.loads((args.root / name).read_text()) for name in FILES)
        result = self_test(data) if args.self_test else validate(*data)
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0
    except (OSError, ValueError, KeyError, TypeError, AssertionError) as error:
        print('PREPARATION_INVALID: ' + str(error))
        return 1

if __name__ == '__main__':
    raise SystemExit(main())
