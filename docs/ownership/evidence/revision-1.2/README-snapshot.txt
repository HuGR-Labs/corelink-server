# CoreLink — preparação de ownership, revisão 1.2

**Estado:** identidades e rascunhos atualizados; publicação bloqueada pelos gates restantes.  
**Data:** 2026-09-19. **Fonte fixada:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.  
**Contrato:** `1.1-candidate`, preservado sem alteração; não congelado por cold review.

Esta entrega elimina os 90 nomes pendentes nos rascunhos do workspace por leitura dos
manifestos na revisão fixada. Confirma dez packages fuzz, incluindo o fuzzer da CLI
omitido no levantamento inicial, e entrega 105 rascunhos com identidades e destinos
preenchidos. Vinte e quatro unidades receberam contexto adicional de manifesto.
Isso não equivale a 105 pacotes semânticos completos nem a 105 issues publicadas.

## Entradas principais

| Necessidade | Arquivo |
|---|---|
| Resultado desta rodada e limites | [PROGRESS.md](PROGRESS.md) |
| 105 identidades, manifestos e fontes | [Inventário](inventory/index.md) |
| 105 rascunhos individuais | [Índice de rascunhos](issue-drafts/index.md) |
| Exemplo novo: fuzzer da CLI | [corelink-cli-fuzz](issue-drafts/fuzz-candidates/tools__cli__fuzz.md) |
| Exemplo de preparação específica | [billing-stripe-traits](issue-drafts/workspace/crates__corelink-billing-stripe-traits.md) |
| Evidência das identidades | [manifest-identities.json](evidence/revision-1.2/manifest-identities.json) |
| Contextos parciais extraídos de manifestos | [manifest-context.json](preparation/manifest-context.json) |
| Registro de emissão — nenhum ID publicado | [publication-ledger.json](plans/publication-ledger.json) |
| Contrato das quatro entregas e limites | [STANDARD.md](STANDARD.md) |
| Schema, ownership comum, relações e aprovação | [COMMON.md](COMMON.md) |
| Verificação automática versus revisão | [VALIDATION-MATRIX.md](VALIDATION-MATRIX.md) |
| Preparação e responsabilidades | [plans/PREPARATION.md](plans/PREPARATION.md) |
| Ferramentas e comandos | [tools/README.md](tools/README.md) |
| Skill | [SKILL.md.tmpl](templates/SKILL.md.tmpl) |
| Referência | [REFERENCE.md.tmpl](templates/REFERENCE.md.tmpl) |
| Blast radius | [BLAST_RADIUS.md.tmpl](templates/BLAST_RADIUS.md.tmpl) |
| Manutenção | [MAINTENANCE.md.tmpl](templates/MAINTENANCE.md.tmpl) |
| Revisão independente | [COLD_REVIEW.md](templates/COLD_REVIEW.md) |
| Histórico das correções 1.1 | [CORRECTIONS.md](CORRECTIONS.md) |

## Evidência e fronteira da entrega

Os resultados atuais estão em `evidence/revision-1.2/`. `CORRECTIONS.md`, `RESEARCH.md`
e evidências anteriores permanecem históricos: seus números de preparação não
representam o estado atual. A referência de estado é este README com PROGRESS.md.

O censo é **estático e source-grounded**, não resultado de `cargo metadata`. Todos os
105 `package.name` foram lidos; a reconciliação de todos os manifestos rastreados,
os targets automáticos e o grafo resolvido não foram executados. As dez declarações
fuzz somam 23 bins explícitos; isso não certifica corpus, cobertura ou execução.

As identidades estão preenchidas, mas os rascunhos continuam bloqueados por preparação
semântica, piloto/capacidade, cold review/freeze, censo Cargo e duplicatas/backlog.
Nenhuma issue, branch, PR, commit, workflow ou arquivo do checkout no Mac foi alterado.

## Incorporação

A integração ao repositório continua pertencendo à frente CO-COMMON por PR. Este ZIP
não instala o gate no repo. Os validadores, schemas, templates, contrato e testes
originais foram preservados; a revisão 1.2 adiciona dados e testes de preparação.
A pasta histórica `issue-drafts/fuzz-candidates/` foi conservada para não romper links.
Seus dez manifestos estão agora confirmados como packages; emissão ainda pendente.
