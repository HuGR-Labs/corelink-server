#!/usr/bin/env python3
"""Render bounded source-preparation packets, not approved crate manuals."""
import argparse
import json
from pathlib import Path


def render(pkg, seed, source, baseline):
    name = pkg['package']
    url = 'https://github.com/HuGR-Labs/corelink-server/blob/' + baseline + '/'
    text = [f'# Preparação: {name}', '',
        '**Estado:** SOURCE_PREPARED_NOT_SEMANTICALLY_APPROVED. **Publicação de issue:** bloqueada pelo contrato/piloto/revisões pendentes.', '',
        f'**Manifesto:** `{pkg["manifest"]}`. **Baseline:** `{baseline}`.', '',
        '[Fatos](#fatos) · [Targets](#targets) · [Relações](#relacoes) · [OKF](#okf) · [Riscos](#riscos) · [Comandos](#comandos) · [Fontes](#fontes)', '',
        '## Fatos', '']
    text += ['- ' + x for x in seed['verified_facts']]
    text += ['', '## Targets', '',
        f'{len(pkg["targets"])} targets enumerados no [censo completo](../census.json), registro cujo `manifest` é `{pkg["manifest"]}`. Abaixo estão apenas as entradas de implementação, não uma substituição do inventário completo.', '']
    text += ['- [`' + x + '`](' + url + x + ')' for x in source['entrypoints']]
    consumers = ', '.join('`' + x + '`' for x in sorted({a['package'] for a in pkg['declared_workspace_consumers']}))
    text += ['', '## Relações', '',
        f'{len(pkg["declared_dependencies"])} declarações de dependência e {len(pkg["declared_workspace_consumers"])} registros inversos. O censo preserva kind, aliases, optional, cfg e features. A união inclui workspaces independentes e **não é um grafo resolvido de um build**.', '',
        'Consumidores declarados: ' + (consumers or 'nenhum na população Cargo examinada') + '.', '', '## OKF', '']
    text += ['- ' + x for x in seed['okf_context']]
    text += ['', '## Riscos', ''] + ['- ' + x for x in seed['specific_risks']]
    text += ['', '## Comandos', '']
    for x in seed['initial_commands']:
        text += [f'- **{x["status"]} / {x["effect"]}:** `{x["command"]}`. {x["result"]}']
    text += ['', '## Fontes', '']
    for x in seed['seed_evidence']:
        text += [f'- [{x["path"]}]({url + x["path"]}); blob `{x["blob"]}`.']
    text += ['', 'O inventário semântico completo, os contratos de runtime, os procedimentos e os quatro cold reviews continuam no escopo da futura issue. Nenhuma mudança funcional é autorizada por este pacote.', '', '[Voltar ao índice](../index.md)']
    result = '\n'.join(text) + '\n'
    for title, anchor in [('Fatos', 'fatos'), ('Targets', 'targets'), ('Relações', 'relacoes'), ('OKF', 'okf'), ('Riscos', 'riscos'), ('Comandos', 'comandos'), ('Fontes', 'fontes')]:
        result = result.replace('\n## ' + title + '\n', '\n<a id="' + anchor + '"></a>\n## ' + title + '\n')
    if len(result.splitlines()) > 150 or len(result.encode()) > 24000 or len(result.split()) > 2400:
        raise ValueError('Packet capacity exceeded: ' + name)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    root = args.root
    c = json.loads((root / 'census.json').read_text())
    seeds = json.loads((root / 'seeds.json').read_text())
    sources = json.loads((root / 'source-packets.json').read_text())
    rows = [f'# Preparação por package — {len(c["packages"])} unidades', '',
        'Dados de entrada para autoria; nenhum pacote abaixo constitui os quatro documentos finais ou aprovação para publicar uma issue.', '',
        '| Package | Targets Cargo | Relações Cargo de entrada | Fontes Rust próprias |',
        '|---|---:|---:|---:|']
    rendered = {}
    for p in c['packages']:
        name = p['package']
        if Path(name).name != name or name in ('.', '..'):
            raise ValueError('Unsafe package name')
        rendered['packets/' + name + '.md'] = render(p, seeds[name], sources[name], c['source_commit'])
        rows.append(f'| [{name}](packets/{name}.md) | {len(p["targets"])} | {len(p["declared_workspace_consumers"])} | {sources[name]["rust_files"]} |')
    rendered['index.md'] = '\n'.join(rows) + '\n'
    existing = {x.relative_to(root).as_posix() for x in (root / 'packets').glob('*.md')}
    expected = {x for x in rendered if x.startswith('packets/')}
    if existing - expected:
        raise ValueError('Unowned packet files: ' + str(sorted(existing - expected)))
    for path, content in rendered.items():
        destination = root / path
        if args.check:
            if not destination.exists() or destination.read_text() != content:
                raise ValueError('Rendered drift: ' + path)
        else:
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(content)
    print(json.dumps({'packets': len(c['packages']), 'mode': 'check' if args.check else 'render', 'publication_approved': False}))
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
