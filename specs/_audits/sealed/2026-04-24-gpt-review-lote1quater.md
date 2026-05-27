# Review Lote 1-quater — Fourth Verification Pass (GPT)

## 1. Veredicto global

Base deste passe: `git diff 9985170..c187262 -- specs/` + releitura dos 6 documentos canônicos (`00_framework.md` + 5 templates).

`c187262` fecha **2 dos 3 fixes** declarados e limpa de fato os 3 resíduos textuais locais. Mas **não** entrega o que escreveu no changelog: o ponto de PRR em `§7.11` continua **parcial**, então o backlog do Lote 1 **não** está zerado. Score honesto: **2/3 reais, 1/3 parcial, 1 regressão nova de governança/auditabilidade**.

## 2. Status dos 3 fixes

1. **`§7.11.3` agora mostra `supersedes` / `superseded_by` como escalar ou lista**

   **Status: RESOLVIDO.**

   Evidência: a tabela canônica aceita `string, lista de strings, ou null` em `specs/00_framework.md:898-899`, e o snippet de `§7.11.3` finalmente espelha isso em `specs/00_framework.md:929-930`.

   Observação brutal: o changelog `0.3.3` diz “match tabela §7.11.2”, mas esses campos moram em `§7.11.1`, não em `§7.11.2`.

2. **`§7.11.4` agora inclui `feature_wi` / `capabilities` / `prod_target_date` para PRR**

   **Status: PARCIAL.**

   O que entrou de verdade: `specs/00_framework.md:948-952` agora lista os 3 campos no template adicional de Nível 4, e o template real de PRR já continua coerente com isso em `specs/_templates/production_readiness_review.md:27-30`.

   O que continua quebrado: a tabela normativa imediatamente acima, em `specs/00_framework.md:906-912`, ainda só declara `work_status`, `parent`, `feature_wi` e `assignee`. **`capabilities` e `prod_target_date` seguem fora do schema canônico.** Então a frase “match schema e template real de PRR” no changelog `0.3.3` é falsa. O patch melhorou a situação, mas **não fechou** o drift.

3. **3 resíduos finais de `Status` fora das 5 tabelas principais**

   **Status: RESOLVIDO.**

   Evidência:

   - Sprint `§13.1`: `specs/_templates/sprint_contract.md:567-571` agora usa `Atendida?`.
   - ST `§11.1`: `specs/_templates/subtask.md:428-431` agora usa `Atendida?`.
   - WI `§4` JTBD: `specs/_templates/work_item.md:249-258` agora usa ``doc_status: FROZEN`` em vez de `FROZEN` cru.

## 3. Regressões novas

- **`c187262` introduziu um changelog autoritativo factualmente incorreto.** Em `specs/00_framework.md:2595`, a entrada `0.3.3` registra: `(a)` “match tabela §7.11.2” para um campo que está em `§7.11.1`; `(b)` “match schema” para um caso em que `§7.11.2` ainda não lista `capabilities` nem `prod_target_date`; e conclui “Backlog de Lote 1 totalmente zerado” mesmo com esse drift ainda aberto. Não é regressão funcional de template, mas é regressão de **auditabilidade**: o registro oficial da mudança passou a mentir sobre o estado real do repo.

## 4. Backlog Lote 1 finalmente zerado?

**NÃO.**

Mesmo excluindo explicitamente os 2 findings reservados para Lotes 5 e 6, ainda sobra **1 finding do próprio Lote 1**:

- **Schema `§7.11.2` ainda não alcançou o template PRR.** `specs/00_framework.md:906-912` não declara `capabilities` nem `prod_target_date`, embora `specs/00_framework.md:948-952` e `specs/_templates/production_readiness_review.md:27-30` já tratem esses campos como parte do contrato de PRR.

Os 2 reservados continuam abertos como esperado:

- **Lote 5:** `tenant_id` ainda aparece em labels de métricas e até como “mitigação” aceitável em `specs/_templates/sprint_contract.md:596-597` e `specs/_templates/work_item.md:1067-1068,1357`.
- **Lote 6:** a contradição fundacional segue viva em `specs/00_framework.md:4,19,28-30`: o framework exige estar *frozen* para que outros docs existam, mas ele próprio continua `doc_status: DRAFT`.

Conclusão seca: **o backlog do Lote 1 não zerou nem no recorte “descontando Lote 5 e 6”.**

## 5. Recomendação final: AVANÇAR PARA LOTE 2? (SIM / NÃO / SIM COM CAVEATS)

**NÃO.**

`c187262` foi um micro-patch útil, mas ainda não pode encerrar o Lote 1 honestamente. O mínimo técnico antes de abrir Lote 2 é:

- alinhar `specs/00_framework.md:906-912` com `specs/00_framework.md:948-952` e com o template real de PRR;
- corrigir a entrada `0.3.3` em `specs/00_framework.md:2595`, que hoje superdeclara o que foi resolvido.

Enquanto isso não for feito, “Lote 1 zerado” continua sendo **autoelogio prematuro**, não fato.
