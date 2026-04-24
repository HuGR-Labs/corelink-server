# Review Lote 1-bis — Second Verification Pass (GPT)

## 1. Veredicto global

`b06ab36` corrige a maior parte do que estava pendente no review anterior: os 6 docs agora têm front matter real no topo, `audit_status` entrou no schema e nos arquivos, o drift de count em §34 foi fechado, a regra de split de WI ficou semanticamente alinhada, o hook de PRR no WI foi completado e a ref quebrada de `§19.5` foi corrigida. Mas o commit **não** entrega a promessa de “todos os 8 itens resolvidos”: o item 2 continua **parcial** porque ainda sobraram refs inline com `Status` genérico e estados soltos (`DOING`, `FROZEN`, `ACTIVE`) no corpo dos templates. Pior: o próprio `1-bis` introduziu regressões novas de schema/CI. Score honesto: **7/8 resolvidos, 1/8 parcial, 0/8 não resolvidos, 0/8 pioraram**.

## 2. Status dos 8 itens pendentes

1. **YAML front matter REAL**
   Status: **RESOLVIDO**
   Evidence: `specs/00_framework.md:1-15`, `specs/_templates/adr.md:1-17`, `specs/_templates/production_readiness_review.md:1-34`, `specs/_templates/sprint_contract.md:1-29`, `specs/_templates/subtask.md:1-20`, `specs/_templates/work_item.md:1-28`.
   Nota: os 6 arquivos agora começam com `---` no topo absoluto, sem code fence; placeholders problemáticos foram trocados por strings válidas.

2. **Eliminar `Status` / `ACCEPTED` / `SEALED` no corpo**
   Status: **PARCIAL**
   Evidence: metadata tables foram corrigidas em `specs/_templates/adr.md:43-46`, `specs/_templates/production_readiness_review.md:112-119`, `specs/_templates/sprint_contract.md:84-99`, `specs/_templates/subtask.md:86-92`, `specs/_templates/work_item.md:132-141`.
   Evidence do gap remanescente: `specs/_templates/subtask.md:119-125` ainda usa coluna `Status` com valores nus `DOING`, `FROZEN`, `ACTIVE`; `specs/_templates/adr.md:508-512` ainda usa `| Dependência | Tipo | Status |` com `CAP-XXX | capability | FROZEN`; `specs/_templates/work_item.md:953-958` ainda usa `Status` genérico em dependências; `specs/_templates/sprint_contract.md:170-174` ainda usa `| Item | Status | Verificação |`.
   Nota: `ACCEPTED` ficou só em contexto histórico (`specs/_templates/adr.md:32,576`) e `SEALED` ficou restrito a valor legítimo de `work_status`, mas a limpeza do corpo **não** terminou.

3. **§34 count drift residual**
   Status: **RESOLVIDO**
   Evidence: `specs/00_framework.md:1930-1940` agora diz `33 seções totais (§0–§32)` para WI; `specs/00_framework.md:1979-1985` agora diz `19 seções totais (§0–§18)` para ST.

4. **WI split rule unificada**
   Status: **RESOLVIDO**
   Evidence: `specs/_templates/work_item.md:1036-1047` usa original `superseded_by` e sucessores `supersedes`, com distinção explícita entre `DEPRECATED` e `SUPERSEDED` e ref corrigida para `§31 Change Log`; `specs/00_framework.md:2505-2510` formaliza `PWI` no glossário.
   Nota: a semântica textual está correta; a inconsistência de tipo entre schema e regra ficou como regressão nova abaixo.

5. **`audit_status` schema field**
   Status: **RESOLVIDO**
   Evidence: schema em `specs/00_framework.md:880-885`; campo presente nos 6 docs em `specs/00_framework.md:5`, `specs/_templates/adr.md:5`, `specs/_templates/production_readiness_review.md:6`, `specs/_templates/sprint_contract.md:6`, `specs/_templates/subtask.md:6`, `specs/_templates/work_item.md:6`.

6. **PRR hook WI completo com `REJECTED`**
   Status: **RESOLVIDO**
   Evidence: `specs/_templates/work_item.md:887-894` lista `NOT_STARTED`, `IN_REVIEW`, `CONDITIONALLY_APPROVED`, `APPROVED`, `REJECTED` e bloqueia `work_status: DONE` enquanto o PRR estiver em `NOT_STARTED | IN_REVIEW | REJECTED`.

7. **Ref `§19.5` corrigida**
   Status: **RESOLVIDO**
   Evidence: `specs/_templates/work_item.md:1378-1381` agora aponta para `§19.5` só se houve re-estimate, `§20.4` se houve escalação, e `§31 Change Log` caso contrário.

8. **Validação YAML passa**
   Status: **RESOLVIDO**
   Evidence: validação manual com `python3` + `yaml.safe_load` retornou `OK` para os 6 arquivos auditados em 2026-04-24; o front matter parseado bate com os blocos em `specs/00_framework.md:1-15`, `specs/_templates/adr.md:1-17`, `specs/_templates/production_readiness_review.md:1-34`, `specs/_templates/sprint_contract.md:1-29`, `specs/_templates/subtask.md:1-20`, `specs/_templates/work_item.md:1-28`.

## 3. Regressões novas

- **`b06ab36` introduziu um CI snippet que falha no próprio repositório.** O script novo de `specs/00_framework.md:956-966` faz `Path('specs').rglob('*.md')` e exige front matter em todo `.md` de `specs/`. Executado como documentado, ele falha imediatamente em `specs/_audits/2026-04-24-gpt-audit-v1.md`, que não tem front matter. O problema não está nos 6 docs auditados; está no escopo errado da validação.

- **`b06ab36` criou contradição de tipo na regra de supersession.** O schema canônico declara `supersedes` e `superseded_by` como `string ou null` em `specs/00_framework.md:892-893`, mas a regra de split manda preencher `superseded_by` com “lista dos sucessores” e usar `supersedes: ["<este WI>"]` em `specs/_templates/work_item.md:1036-1047`. Ou o schema aceita lista, ou a regra não aceita split 1→N. Hoje os dois textos se desmentem.

- **`b06ab36` criou contradição entre schema de Nível 4 e template de PRR.** `specs/00_framework.md:898-904` exige `parent` para artefatos `type ∈ {sprint, work_item, sub_task, prr}` (exceto sprint), mas `specs/_templates/production_readiness_review.md:27-30` não tem `parent`; usa `feature_wi` no lugar. O schema ficou mais rígido que o próprio template.

## 4. Findings ainda abertos do audit v1 / review Lote 1

Excluindo explicitamente o que o review anterior já empurrou para **Lote 2** (backlog de machine-readable/evidence taxonomy) e **Lote 6** (contradição fundacional do framework ainda `DRAFT`), ainda restam abertos:

- **Drift residual de contagem dentro do próprio WI template.** `specs/_templates/work_item.md:917` ainda diz que cada sub-task é “contrato próprio com 14 seções”, enquanto o framework crava 19 em `specs/00_framework.md:1979-1985` e o template real tem `§0–§18` em `specs/_templates/subtask.md:55-75`. Isso já estava no audit v1.

- **O framework ainda carrega semântica antiga de `Status`/`ACCEPTED` fora dos 5 templates.** Exemplos objetivos: `specs/00_framework.md:645` (“Todo ADR `ACCEPTED`”), `specs/00_framework.md:693` (`PROPOSED -> ACCEPTED`), `specs/00_framework.md:1242`, `specs/00_framework.md:2039`, `specs/00_framework.md:2096`. O cleanup de lifecycle não fechou globalmente.

- **O finding de cardinalidade/privacy com `tenant_id` em métricas continua aberto.** `specs/_templates/sprint_contract.md:596-597` e `specs/_templates/work_item.md:1067-1068` ainda usam `tenant_id` como label de métrica; `specs/_templates/work_item.md:1357` ainda trata “limitar labels a `tenant_id` apenas” como mitigação aceitável. O audit v1 já tinha chamado isso de erro de cardinalidade e data minimization.

## 5. Recomendação: POSSO AVANÇAR PARA LOTE 2? (SIM / NÃO)

**NÃO.** O `1-bis` ficou perto, mas não zerou o backlog que declarou zerar: o item 2 segue parcial e o commit ainda introduziu regressões novas em schema/CI. O mínimo honesto antes de abrir Lote 2 é um micro-patch final fechando o resíduo de `Status` genérico nas refs inline e reconciliando as três contradições novas (`validator scope`, `supersedes/superseded_by` tipo escalar vs lista, `parent` obrigatório vs PRR sem `parent`).
