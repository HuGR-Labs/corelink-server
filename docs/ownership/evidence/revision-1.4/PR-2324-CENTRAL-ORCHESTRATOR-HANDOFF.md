# PR #2324 — handoff para o orquestrador central

## Identidade

- PR: https://github.com/HuGR-dev/corelink-server/pull/2324
- issue de follow-up: https://github.com/HuGR-dev/corelink-server/issues/2310
- repositório: `HuGR-dev/corelink-server`
- base: `main`
- head: `gustavomhss:codex/corelink-ownership-pr`
- baseline de reancoragem: `0ec05c34db282122579d7e2ab6541183e20aeaf5`
- commits publicados da PR: consultar o head atual antes do merge; todos devem ter assinatura SSH válida e trailer DCO
- snapshot de origem: `427ff4bd6`
- estado da PR: draft; merge ainda não realizado

## Conteúdo entregue

A PR adiciona o snapshot de ownership do pacote: standard, schemas, templates,
gates, registry/index, evidências, issue drafts, 105 skills `own-*` e os três
documentos de ownership por pacote. Os arquivos de preparação já existentes no
`main` foram preservados; não há alteração de código Rust nesta entrega. A
reancoragem atualiza somente os hashes que o verifier lê como estado corrente, o
registry/index e o ponteiro atual em `STANDARD.md`; packets e snapshots históricos
permanecem históricos.

## Validação reproduzida no snapshot

- `python3 -m unittest discover -s docs/ownership/tests -q`: 157 testes;
- `python3 docs/ownership/tests/adversarial_probe.py`: 13/13 casos esperados;
- geração do registry: 105 pacotes, calibração `PASS`, zero publicações;
- checkers estruturais dos artefatos revisados: `IMPLEMENTED_CHECKS_PASS`;
- nenhum Cargo, runtime, deploy ou operação de produção foi executado.

## Gates que permanecem explícitos

- standard ainda não está congelado em um commit publicado no `main`;
- registry permanece `UNVERIFIED`/`NOT_PUBLISHED` para os 105 pacotes;
- cold review final classificou quatro áreas `FIX-FIRST`/stale-marked;
- nenhum registro formal de review de pacote foi fabricado;
- nenhuma issue de ownership em massa foi publicada.

Esses gates são residuais intencionais. Não convertê-los em aprovação apenas para
obter um merge verde.

## Procedimento de merge

1. Confirmar que a PR ainda aponta para `main` e que o head publicado corresponde
   ao handoff.
2. Fazer `git fetch origin main` e ler novamente `git rev-parse origin/main` e
   `git ls-remote origin refs/heads/main` imediatamente antes da validação.
3. Rodar os gates hospedados no head exato, incluindo DCO e current-main
   provenance; conferir que registry/index foram gerados com o SHA recém-observado.
   Se o main mudar antes da integração, reancorar e repetir os gates afetados;
   não enfraquecer o stale-main gate.
4. Fazer cold review independente do diff da PR e resolver ou registrar os
   `FIX-FIRST`/`BLOCKED` findings sem inventar evidência.
5. Confirmar que #2310 continua sendo o owner do refresh pós-WIP de READMEs e
   ponteiros locais.
6. Somente então usar o método normal de merge da organização e registrar o SHA
   final. A correção DCO já reconstituiu os dois commits sobre o main atual com
   `--force-with-lease`; não reescrever novamente durante a revisão. Não publicar
   as 105 issues a partir desta PR.

## Resultado esperado do handoff

O orquestrador deve deixar um comentário/registro com: SHA final de `main`,
comandos e resultados reproduzidos, decisão sobre os gates residuais, método de
merge e URL/ID do merge. Até esse registro existir, esta PR deve permanecer como
`DRAFT`/não mergeada.
