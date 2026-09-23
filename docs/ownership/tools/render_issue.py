#!/usr/bin/env python3
"""Render one non-published ownership issue from real census + reviewed seed.

No guessed package names, substituted directory basenames, or automatic READY.
The rendered draft still needs publication_gate and an immutable frozen contract.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import re
import sys

if str(Path(__file__).resolve().parent) not in sys.path:
    sys.path.insert(0,str(Path(__file__).resolve().parent))
from prepare_census import skill_slug
from publication_gate import marker


def render(package: dict, seed: dict, *, baseline: str, contract_url: str,
           expected_repository: str) -> str:
    required=('verified_facts','okf_context','specific_risks','initial_commands','seed_evidence')
    if any(not isinstance(seed.get(k),list) or not seed[k] for k in required):
        raise ValueError('crate-specific seed incomplete')
    if not package.get('targets') or not re.fullmatch(r'[0-9a-f]{40}',baseline):
        raise ValueError('verified targets/baseline missing')
    name=package['package'];slug=skill_slug(name)
    if package.get('skill_slug')!=slug:raise ValueError('noncanonical skill_slug')
    if not re.fullmatch(r'https://github\.com/[^/]+/[^/]+/blob/[0-9a-f]{40}/.+',contract_url):
        raise ValueError('immutable contract URL required')
    if not re.fullmatch(r'[^/\s]+/[^/\s]+', expected_repository):
        raise ValueError('canonical repository identity required')
    match=re.match(r'https://github\.com/([^/]+/[^/]+)/',contract_url)
    if not match or match.group(1)!=expected_repository:
        raise ValueError('contract repository mismatch')
    def lines(items):return '\n'.join('- '+str(i) for i in items)
    targets=lines(f"{t['name']} ({','.join(t['kind'])}): `{t['src_path']}`; required features={t.get('required_features',[])}" for t in package['targets'])
    deps=lines(f"{d['package']} | {d['kind']} | alias={d.get('alias')} | cfg={d.get('target_cfg')} | optional={d.get('optional')} | features={d.get('features',[])}" for d in package['declared_dependencies']) or 'Nenhuma declaração nessa população; verificar relações fora de Cargo.'
    consumers=lines(f"{d['package']} | {d['kind']} | cfg={d.get('target_cfg')} | alias={d.get('alias')}" for d in package['declared_workspace_consumers']) or 'Nenhum consumidor Cargo declarado no censo; isso não prova ausência de consumidores externos.'
    text=f'''{marker(package['manifest'])}
# [ownership] {name}: skill, referência, blast radius e manutenção

## Identidade e contrato

Package: `{name}`. Manifesto: `{package['manifest']}`. Baseline: `{baseline}`.
Contrato: {contract_url}
Estado: DRAFT_VALIDATED_STRUCTURE; publicação depende do gate e da revisão do seed.
Capacidade: aplicar perfil S; H somente com a medição exigida pelo contrato.

## Preparação específica

### Fatos e fontes
{lines(seed['verified_facts'])}

### Targets e features
{targets}
Features declaradas: `{json.dumps(package.get('features',{}),ensure_ascii=False,sort_keys=True)}`.

### Dependências declaradas — não é prova de execução
{deps}

### Consumidores declarados — não é grafo semântico completo
{consumers}

### Contexto OKF
{lines(seed['okf_context'])}

### Riscos concretos
{lines(seed['specific_risks'])}

### Comandos iniciais e limites
{lines(seed['initial_commands'])}

### Evidência de preparação
{lines(seed['seed_evidence'])}

## Entregas

- [ ] `.claude/skills/{slug}/SKILL.md`.
- [ ] `docs/ownership/crates/{name}/REFERENCE.md`.
- [ ] `docs/ownership/crates/{name}/BLAST_RADIUS.md`.
- [ ] `docs/ownership/crates/{name}/MAINTENANCE.md`.

## Success criteria

Outro owner encontra contrato, impacto e procedimento em até três cliques, sem a
conversa do autor. Distingue implementação, wiring e comportamento observado.

## Completeness criteria

Conciliar targets, módulos, APIs, invariantes e relações diretas/inversas, incluindo
dados, configuração, TypeScript, SQL, build e contratos externos aplicáveis.
A descoberta semântica aprofundada é trabalho desta issue, não uma alegação deste seed.

## Quality standards

Cumprir seções e tetos simultâneos de linhas/palavras/bytes do contrato congelado.
Preservar uma única referência, um blast radius e um manual; não omitir para caber.
Usar relações com identidade compartilhada e fontes; não copiar políticas do OKF.

## Cold review

- [ ] Skill: APPROVE nos bytes finais, gatilhos e autoridade verificados.
- [ ] Referência: APPROVE, contratos/invariantes confrontados com o código.
- [ ] Blast radius: APPROVE, recenso independente e visões dos pares conciliadas.
- [ ] Manual: APPROVE no escopo declarado, certificação individual dos procedimentos.

Revisor diferente do autor e contexto novo; registrar evidência e reavaliar alterações.

## Definition of done

Quatro documentos, fontes e quatro revisões finais coerentes; requisitos materiais
sem pendências; gates aplicáveis verdes; PR integrado e índices/backlog conciliados.
A integração compartilhada é responsabilidade da frente CO-COMMON; não editar índices
concorrentemente por crate. Não fechar antes de integrar essa dependência.

## Invariants

Sem mudanças funcionais disfarçadas, autoridade inventada, execução fictícia,
omissão de relações, autoaprovação ou alteração de contratos compartilhados sem revisão.
'''
    if re.search(r'\{\{[^{}]+\}\}|\bTODO\b|\bTBD\b',text):
        raise ValueError('unresolved placeholder in rendered issue')
    if len(text.encode())>24000 or len(text.splitlines())>240 or len(text.split())>3000:
        raise ValueError('BLOCKED_ISSUE_CAPACITY: 240 lines / 3000 words / 24000 UTF-8 bytes')
    return text


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--census',type=Path,required=True);ap.add_argument('--manifest',required=True)
    ap.add_argument('--seed',type=Path,required=True);ap.add_argument('--contract-url',required=True)
    ap.add_argument('--repository',required=True,help='canonical OWNER/REPO')
    args=ap.parse_args()
    try:
        c=json.loads(args.census.read_text())
        if c.get('first_party_scope_fully_classified') is not True:raise ValueError('scope census not reconciled')
        matches=[p for p in c['packages'] if p['manifest']==args.manifest]
        if len(matches)!=1:raise ValueError('manifest must identify exactly one census package')
        print(render(matches[0],json.loads(args.seed.read_text()),baseline=c['source_commit'],contract_url=args.contract_url,expected_repository=args.repository),end='')
        return 0
    except (OSError,ValueError,KeyError,TypeError) as e:
        print(f'BLOCKED: {e}',file=sys.stderr);return 2

if __name__=='__main__':raise SystemExit(main())
