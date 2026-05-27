# Review Lote 1-ter — Third Verification Pass (GPT)

## 1. Veredicto global

`0f972af` limpa a maior parte do que prometeu: o snippet de CI finalmente deixa de se autossabotar, o schema de supersession agora aceita lista, as 5 tabelas marcadas perderam a coluna genérica `Status`, o drift de "14 seções" morreu, e o cleanup do framework removeu os resíduos de `ACCEPTED`/`PROPOSED` que estavam pendurados no `00_framework.md`. Mas o commit **não** zera o backlog como anunciado: a reconciliação `parent` vs `feature_wi` ficou **parcial**, porque `§7.11.2` foi corrigida e o template real de PRR usa `feature_wi`, porém o template canônico de Nível 4 em `§7.11.4` continua sem esse campo. Pior: isso introduz um drift novo dentro da própria seção normativa de schema. Score honesto: **5/6 resolvidos, 1/6 parcial; 1 regressão nova**.

## 2. Status dos 6 itens pendentes

1. **CI script scope corrigido**  
   Status: **RESOLVIDO**  
   Evidence: `specs/00_framework.md:970-977` define `SKIP_DIRS = {'_audits', '_archive'}` e pula paths desses diretórios antes de exigir front matter; `specs/00_framework.md:978-985` mantém o parse via `yaml.safe_load`. Execução literal do snippet no estado atual: `OK 6 files`.

2. **Schema `supersedes` / `superseded_by` aceita lista**  
   Status: **RESOLVIDO**  
   Evidence: `specs/00_framework.md:898-899` agora declara `string, lista de strings, ou null` para ambos os campos; a regra operacional continua alinhada em `specs/_templates/work_item.md:1037,1046-1047`, que usa `superseded_by` no original e `supersedes: [<ID original>]` nos sucessores.

3. **PRR `parent` vs `feature_wi` reconciliado**  
   Status: **PARCIAL**  
   Evidence do fix: `specs/00_framework.md:909-910` torna `parent` N/A para `prr` e adiciona `feature_wi` como obrigatório apenas para PRR; `specs/_templates/production_readiness_review.md:1-33` usa `feature_wi:` no front matter e não exige `parent`.  
   Evidence do gap: `specs/00_framework.md:935-942` ainda mostra o "Template adicional para Nível 4" só com `work_status`, `parent` e `assignee`; `feature_wi` ficou fora do snippet canônico.

4. **Coluna `Status` renomeada nas 5 tabelas flagged**  
   Status: **RESOLVIDO**  
   Evidence: `specs/_templates/subtask.md:119`, `specs/_templates/adr.md:508`, `specs/_templates/work_item.md:249`, `specs/_templates/work_item.md:953` e `specs/_templates/sprint_contract.md:170` não usam mais a coluna `Status`; agora aparecem como `Estado requerido`, `Atendida?` ou `Atendido?`.

5. **`work_item.md:917` drift `14 seções` → `19 seções §0–§18`**  
   Status: **RESOLVIDO**  
   Evidence: `specs/_templates/work_item.md:917` agora diz `cada sub-task é contrato próprio com 19 seções totais (§0–§18)`; bate com `specs/00_framework.md:2000,2043`.

6. **Framework cleanup ACCEPTED/PROPOSED residuais + duplicata 34.6/34.7**  
   Status: **RESOLVIDO**  
   Evidence: `specs/00_framework.md:645` troca `ADR ACCEPTED` por `doc_status: FROZEN`; `specs/00_framework.md:692-700` reescreve o ciclo em termos de `DRAFT/REVIEW/FROZEN`; `specs/00_framework.md:1259-1263`, `2058-2059` e `2116-2117` substituem `Status` antigo por `doc_status` + `audit_status`; a duplicata sumiu, porque `specs/00_framework.md:2053-2109` vai de `34.6` direto para `34.8`.

## 3. Regressões novas

- **`§7.11` ficou semanticamente desalinhada por propagação incompleta do próprio patch.** O schema canônico foi ampliado em `specs/00_framework.md:898-910`, mas os snippets normativos logo abaixo não acompanharam: `specs/00_framework.md:929-930` ainda exemplifica `supersedes` / `superseded_by` só como caso escalar, e `specs/00_framework.md:935-942` omite o `feature_wi` que `specs/00_framework.md:910` acabou de tornar obrigatório para PRR. Não quebra o repo hoje, mas volta a criar documentação que se contradiz dentro da mesma seção.

## 4. Findings ainda abertos

- **O finding de cardinalidade/privacy com `tenant_id` em métricas continua aberto.** `specs/_templates/sprint_contract.md:596-597` e `specs/_templates/work_item.md:1067-1068` ainda usam `tenant_id` como label de métrica; `specs/_templates/work_item.md:1357` ainda trata "limitar labels a `tenant_id` apenas" como mitigação aceitável.

- **A limpeza de nomenclatura ainda não fechou 100% fora das 5 tabelas atacadas.** `specs/_templates/subtask.md:428-431` ainda usa `| Dependência | Status | Hard blocker? |`; `specs/_templates/sprint_contract.md:567-572` ainda usa `| Dependência | Tipo | Status | Bloqueia? |`; e `specs/_templates/work_item.md:258` ainda deixa um `JTBD-ZZZ | FROZEN | enables | §3` cru dentro de uma tabela já migrada para `Estado requerido`.

## 5. Recomendação: POSSO AVANÇAR PARA LOTE 2? (SIM / NÃO / SIM COM CAVEATS)

**NÃO.** O `1-ter` ficou muito perto, mas ainda não entregou o que declarou: 1 dos 6 itens continua parcial e o patch criou um drift novo justamente na seção canônica de schema. O mínimo honesto antes de abrir Lote 2 é um micro-patch final para: `§7.11.3`/`§7.11.4`, o resíduo de `Status` fora das tabelas atacadas, e o problema recorrente de `tenant_id` em labels de métrica.
