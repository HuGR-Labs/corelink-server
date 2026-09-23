#!/usr/bin/env python3
"""Prepare a fresh ownership census without publishing issues or compiling Rust.

Python >= 3.11; git and a configured Cargo installation required.
Runs only git reads and `cargo metadata --locked --offline --no-deps`.
Does not alter the lockfile, relax offline mode, run tests or contact GitHub.
Output must be a new directory OUTSIDE the checkout to avoid repo pollution.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib


def run(argv: list[str], root: Path) -> str:
    cp=subprocess.run(argv,cwd=root,text=True,capture_output=True,timeout=90,
                      env={**os.environ,'CARGO_NET_OFFLINE':'true'})
    if cp.returncode:
        raise RuntimeError(f'{argv[0]} failed ({cp.returncode}): {cp.stderr.strip()}')
    return cp.stdout


def skill_slug(package: str) -> str:
    if not re.fullmatch(r'[A-Za-z0-9_-]+',package):
        raise ValueError('package name cannot be mapped to this skill profile')
    slug=re.sub('-+','-','own-'+package.lower().replace('_','-')).strip('-')
    if len(slug)>64:
        slug=slug[:55].rstrip('-')+'-'+hashlib.sha256(package.encode()).hexdigest()[:8]
    return slug


def normalize_metadata(data: dict, root: Path, *, expected_workspace_root: Path | None = None) -> list[dict]:
    if data.get('version') != 1:
        raise ValueError('expected Cargo metadata format version 1')
    if Path(data.get('workspace_root','')).resolve()!=(expected_workspace_root or root).resolve():
        raise ValueError('workspace_root mismatch')
    ids=data.get('workspace_members')
    if not isinstance(ids,list) or not ids or len(set(ids))!=len(ids):
        raise ValueError('empty, malformed or duplicate workspace_members')
    all_packages=data.get('packages',[])
    packages={p['id']:p for p in all_packages}
    if len(packages)!=len(all_packages) or not set(ids)<=packages.keys():
        raise ValueError('duplicate or missing package IDs')
    out=[]; names=set(); paths=set(); slugs=set()
    for pid in ids:
        p=packages[pid]
        manifest=Path(p['manifest_path']).resolve()
        if not manifest.is_relative_to(root.resolve()):
            raise ValueError('workspace package outside authorized checkout')
        relative=manifest.relative_to(root.resolve()).as_posix()
        actual=tomllib.loads(manifest.read_text(encoding='utf-8'))
        if actual.get('package',{}).get('name') != p['name']:
            raise ValueError(f'package name disagrees with manifest: {relative}')
        slug=skill_slug(p['name'])
        if p['name'] in names or relative in paths or slug in slugs:
            raise ValueError('ambiguous package, manifest or skill identity')
        names.add(p['name']);paths.add(relative);slugs.add(slug)
        target_list=[]
        for t in p.get('targets',[]):
            src=Path(t['src_path']).resolve()
            if not src.is_relative_to(root.resolve()):
                raise ValueError(f'target outside checkout: {p["name"]}')
            target_list.append({'name':t['name'],'kind':t['kind'],
                                'src_path':src.relative_to(root.resolve()).as_posix(),
                                'required_features':t.get('required-features',[])})
        if not target_list:
            raise ValueError(f'empty targets: {p["name"]}')
        deps=[]
        for d in p.get('dependencies',[]):
            local=d.get('path')
            local_manifest=None
            if local:
                local_path=Path(local).resolve()/'Cargo.toml'
                if local_path.is_relative_to(root.resolve()):
                    local_manifest=local_path.relative_to(root.resolve()).as_posix()
            deps.append({'package':d['name'],'alias':d.get('rename'),
                         'kind':d.get('kind') or 'normal','target_cfg':d.get('target'),
                         'optional':d.get('optional',False),'features':d.get('features',[]),
                         'default_features':d.get('uses_default_features',True),
                         'requirement':d.get('req'),'source':d.get('source'),
                         'local_manifest':local_manifest})
        out.append({'package':p['name'],'manifest':relative,'skill_slug':slug,
                    'manifest_sha256':hashlib.sha256(manifest.read_bytes()).hexdigest(),
                    'targets':target_list,'features':p.get('features',{}),
                    'license':p.get('license'),'publish':p.get('publish'),
                    'declared_dependencies':deps,'declared_workspace_consumers':[],
                    'semantic_relations_verified':False,'runtime_verified':False})
    by_manifest={p['manifest']:p for p in out}
    for consumer in out:
        for dep in consumer['declared_dependencies']:
            producer=by_manifest.get(dep['local_manifest'])
            if producer is not None:
                producer['declared_workspace_consumers'].append({
                    'package':consumer['package'],'manifest':consumer['manifest'],
                    'kind':dep['kind'],'target_cfg':dep['target_cfg'],
                    'optional':dep['optional'],'alias':dep['alias'],
                    'note':'Declared edge only; activation/call not proven.'})
    return sorted(out,key=lambda p:p['manifest'])



def classify_manifest(manifest: str, parsed: dict, decision: dict | None) -> str:
    """Classification is explicit for vendor/fixture/independent packages."""
    if 'package' not in parsed:
        return 'workspace_only_no_package'
    if decision is None:
        return 'UNCLASSIFIED'
    # Historical manifests are tracked evidence, but are not eligible packages.
    # Keep this classification explicit instead of guessing from path names.
    allowed={'first_party_independent','third_party_vendor','test_fixture','archive'}
    if decision.get('classification') not in allowed or not decision.get('reason') or not decision.get('evidence'):
        raise ValueError(f'incomplete scope decision: {manifest}')
    return decision['classification']


def reconcile_packages(groups: list[list[dict]]) -> list[dict]:
    packages=[p for group in groups for p in group]
    for field in ('package','manifest','skill_slug'):
        values=[p[field] for p in packages]
        if len(values)!=len(set(values)):
            raise ValueError(f'ambiguous combined population: duplicate {field}')
    by_manifest={p['manifest']:p for p in packages}
    for p in packages:p['declared_workspace_consumers']=[]
    for consumer in packages:
        for dep in consumer['declared_dependencies']:
            producer=by_manifest.get(dep['local_manifest'])
            if producer is not None:
                producer['declared_workspace_consumers'].append({
                    'package':consumer['package'],'manifest':consumer['manifest'],
                    'kind':dep['kind'],'target_cfg':dep['target_cfg'],'optional':dep['optional'],
                    'alias':dep['alias'],'note':'Declared edge only; activation/call not proven.'})
    return sorted(packages,key=lambda p:p['manifest'])

def main() -> int:
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--repo-root',type=Path,required=True)
    ap.add_argument('--expected-commit',required=True)
    ap.add_argument('--output-dir',type=Path,required=True)
    ap.add_argument('--cargo-bin',default='cargo',help='Configured Cargo executable, not a shell command')
    ap.add_argument('--scope-decisions',type=Path,help='JSON decisions keyed by non-member manifest; never guessed from vendor/fuzz names')
    ns=ap.parse_args()
    try:
        root=ns.repo_root.resolve();dest=ns.output_dir.resolve()
        if dest.exists() or dest.is_relative_to(root) or root.is_relative_to(dest):
            raise ValueError('output must be a new directory outside and not above the checkout')
        if not re.fullmatch(r'[0-9a-f]{40}',ns.expected_commit):
            raise ValueError('expected-commit must be a full 40-hex Git commit SHA')
        gitroot=Path(run(['git','rev-parse','--show-toplevel'],root).strip()).resolve()
        if gitroot!=root:
            raise ValueError('repo-root must be the Git checkout root')
        head=run(['git','rev-parse','HEAD'],root).strip()
        if head!=ns.expected_commit:
            raise ValueError('baseline mismatch; no reset/checkout is performed')
        if run(['git','status','--porcelain=v1','--untracked-files=all'],root).strip():
            raise ValueError('checkout is dirty; use an isolated clean checkout')
        lock=root/'Cargo.lock'
        before=lock.read_bytes()
        command=[ns.cargo_bin,'metadata','--locked','--offline','--no-deps','--format-version=1']
        raw=run(command,root)
        data=json.loads(raw)
        packages=normalize_metadata(data,root)
        if lock.read_bytes()!=before:
            raise ValueError('Cargo.lock changed; census refused')
        if run(['git','rev-parse','HEAD'],root).strip()!=head or run(
            ['git','status','--porcelain=v1','--untracked-files=all'],root).strip():
            raise ValueError('checkout changed during census; output refused')
        tracked=run(['git','ls-files','-z'],root).split('\0')
        manifests=sorted(p for p in tracked if p=='Cargo.toml' or p.endswith('/Cargo.toml'))
        member_paths={p['manifest'] for p in packages}
        decisions=json.loads(ns.scope_decisions.read_text()) if ns.scope_decisions else {}
        if not isinstance(decisions,dict) or not set(decisions)<=set(manifests)-member_paths:
            raise ValueError('scope decisions contain unknown/member paths')
        other=[]; independent_raw={}; groups=[packages]
        for m in manifests:
            if m in member_paths:
                continue
            parsed=tomllib.loads((root/m).read_text(encoding='utf-8'))
            classification=classify_manifest(m,parsed,decisions.get(m))
            other.append({'manifest':m,'package':parsed.get('package',{}).get('name'),
                          'classification':classification,'decision':decisions.get(m)})
            if classification=='first_party_independent':
                cmd=command+['--manifest-path',m]
                extra_raw=run(cmd,root)
                extra=normalize_metadata(json.loads(extra_raw),root,expected_workspace_root=(root/m).parent)
                groups.append(extra);independent_raw[m]=extra_raw
        packages=reconcile_packages(groups)
        if (run(['git','rev-parse','HEAD'],root).strip()!=head or lock.read_bytes()!=before
                or run(['git','status','--porcelain=v1','--untracked-files=all'],root).strip()):
            raise ValueError('checkout/lock changed while classifying independent packages')
        report={'schema':'corelink-ownership-census/1','source_commit':head,
                'command':command,'cargo_lock_sha256':hashlib.sha256(before).hexdigest(),
                'packages':packages,'other_tracked_manifests':other,
                'resolved_graph_produced':False,'first_party_scope_fully_classified':all(x['classification']!='UNCLASSIFIED' for x in other),
                'workspace_member_count':len(member_paths),
                'github_publications':0,'cold_reviews':0}
        # Only write after all source checks pass. A failed write is reported,
        # never retried by overwriting a possibly populated destination.
        dest.mkdir(parents=True,exist_ok=False)
        (dest/'census.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
        (dest/'metadata.json').write_text(raw,encoding='utf-8')
        (dest/'independent-metadata.json').write_text(json.dumps(independent_raw,indent=2)+'\n',encoding='utf-8')
        print(json.dumps({'workspace_packages':len(member_paths),'eligible_packages':len(packages),'other_manifests':len(other),
                          'output':str(dest),'published_issues':0},ensure_ascii=False))
        return 0
    except (OSError,ValueError,KeyError,TypeError,RuntimeError,subprocess.TimeoutExpired) as exc:
        print(f'BLOCKED: {exc}',file=sys.stderr);return 2

if __name__=='__main__':
    raise SystemExit(main())
