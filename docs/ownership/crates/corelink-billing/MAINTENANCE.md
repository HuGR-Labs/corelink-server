---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-billing
manifest: crates/corelink-billing/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: H
state: draft
evidence_set: billing-pilot-source-20260919
---

# corelink-billing — manual de manutenção

[Preparação](#m01) · [Escolher procedimento](#m02) · [Procedimentos](#m03) ·
[Testes](#m04) · [Recuperação](#m05) · [Escalonamento](#m06).

<a id="m01"></a>
## M01 — Preparação segura

Confirme SHA, manifesto e target antes de agir. A evidência inicial usou Rust/Cargo 1.91.1 e
`x86_64-apple-darwin` no baseline. Use `--locked --offline` e `CARGO_TARGET_DIR` temporário.
Não rode suite live, ignored, deploy, migration D1, Stripe, R2 ou Worker por aparecer aqui.
O primeiro build inclui dependências transitivas de Stripe e pode demorar.

<a id="m02"></a>
## M02 — Seleção de procedimentos

| Situação | Procedimento | Modo | Efeito permitido | Autorização extra? |
|---|---|---|---|---|
| confirmar package/grafo | [PROC-001](#proc-001) | READ_ONLY | leitura | não |
| validar alteração local | [PROC-002](#proc-002) | LOCAL_ISOLATED | build/teste temporário | não |
| abuse inesperado | [PROC-003](#proc-003) | LOCAL_ISOLATED | fake/testes | não |
| quota, CAS ou migration | [PROC-004](#proc-004) | LOCAL_ISOLATED | fake/schema local | D1 real |
| FSM de quota | [PROC-005](#proc-005) | LOCAL_ISOLATED | fake/testes | Worker/webhook |
| replay forense | [PROC-006](#proc-006) | LOCAL_ISOLATED | fake/testes | R2/D1/RBAC |
| alias/reexport | [PROC-007](#proc-007) | LOCAL_ISOLATED | compilação | owner da origem |

<a id="m03"></a>
## M03 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — Confirmar package e fronteiras

`review_status: REVIEWED` · `execution_status: REVIEWED_NOT_EXECUTED` · `mode: READ_ONLY` ·
`environment: checkout local` · `result: NOT_RUN` · `evidence: SOURCE` ·
`limitation: não certifica runtime` · `required_for_acceptance: false`

**Gatilho / resultado esperado:** confirmar que o alvo é billing e se o símbolo é interno ou alias.
**Modo / ambiente / permissão:** READ_ONLY no checkout local.
**Entradas e validação:** baseline e target explícitos.
**Pré-condições:** árvore de trabalho identificada.

1. Rode `git rev-parse HEAD` e `git status --short`.
2. Leia `crates/corelink-billing/Cargo.toml` e `src/lib.rs`.
3. Rode `cargo tree --locked --offline --workspace --invert corelink-billing --target x86_64-apple-darwin --edges normal,build`.
4. Classifique o símbolo pela referência R02/R03.

**Falhas e parada:** SHA, package ou target divergente; não inferir runtime do grafo.
**Recuperação:** nenhuma escrita ocorreu; corrigir seleção e repetir.
**Estado:** conteúdo revisado; nenhum comando executado nesta passagem.
**Evidência:** SHA, manifesto, comando e output.

[Índice de procedimentos](#m02)

<a id="proc-002"></a>

### PROC-002 — Validar pacote em isolamento

`review_status: REVIEWED` · `execution_status: BLOCKED_NOT_EXECUTED` · `mode: LOCAL_ISOLATED` ·
`environment: target temporário, offline` · `result: NOT_RUN` · `evidence: SOURCE` ·
`source_pin: cca798ff5bc2df660ecf2570ed243eb9775ff3d0` · `limitation: aceitação local pendente` · `required_for_acceptance: true`

**Gatilho / resultado esperado:** mudança local mantém as suites selecionadas verdes.
**Modo / ambiente / permissão:** LOCAL_ISOLATED; sem rede, deploy ou dados reais.
**Entradas e validação:** Rust/Cargo e target existentes.
**Pré-condições:** diff revisado e target directory fora do repositório.

1. Crie target temporário da tarefa.
2. Rode `cargo test --locked --offline -p corelink-billing --target x86_64-apple-darwin`.
3. Preserve targets, passed/failed/ignored e toolchain.
4. Rode clippy se a mudança afetar Rust e o target estiver disponível.

**Falhas e parada:** teste falho, seleção vazia ou lock alterado; não force ignore.
**Recuperação:** reverta somente mudança local autorizada ou faça roll-forward com teste negativo.
**Estado:** bloqueado e não executado nesta passagem; não há output nem contagem independente para o source pin declarado.
**Evidência requerida:** baseline, comando e saída completa com classe `EXECUTED_LOCAL`.

[Índice de procedimentos](#m02)

<a id="proc-003"></a>

### PROC-003 — Diagnosticar decisão de abuse

`review_status: REVIEWED` · `execution_status: BLOCKED_NOT_EXECUTED` · `mode: LOCAL_ISOLATED` ·
`environment: fakes/dados sintéticos` · `result: NOT_RUN` · `evidence: SOURCE` ·
`source_pin: cca798ff5bc2df660ecf2570ed243eb9775ff3d0` · `limitation: sem prova de limiter remoto` · `required_for_acceptance: true`

**Gatilho / resultado esperado:** score, downgrade ou suspensão inesperados.
**Modo / ambiente / permissão:** LOCAL_ISOLATED; fakes e dados sintéticos.
**Entradas e validação:** features, tier, config e tenant sem PII.
**Pré-condições:** selecionar API-011/012 e INV-001–003.

1. Reproduza com `AbuseFeatures` e `AbuseConfig` sintéticos.
2. Verifique score `[0,1]` e limiares 0.5/0.8.
3. Separe falha nos audits iniciais (sem update) de falha no audit `DowngradeApplied` (update já aplicado; efeito parcial).
4. Confirme `AutoSuspendForbidden`.
5. Rode `cargo test --locked --offline -p corelink-billing --test abuse_prop_abuse`.

**Falhas e parada:** dado real, pedido de suspensão ou limiter ativo.
**Recuperação:** descarte fixture; encaminhe decisão humana ao owner operacional.
**Estado:** bloqueado e não executado nesta passagem; nenhum output está registrado para o source pin declarado. A falha no primeiro audit não prova rollback para a falha pós-update.
**Evidência requerida:** pin, inputs sintéticos, audit que falhou, estado do limiter antes/depois, estado da janela/métricas, comando, exit status e saída completa com classe `EXECUTED_LOCAL`.

[Índice de procedimentos](#m02)

<a id="proc-004"></a>

### PROC-004 — Diagnosticar quota e migração

`review_status: REVIEWED` · `execution_status: BLOCKED_NOT_EXECUTED` · `mode: LOCAL_ISOLATED` ·
`environment: fakes/offline; D1 bloqueado` · `result: NOT_RUN` · `evidence: SOURCE` ·
`source_pin: cca798ff5bc2df660ecf2570ed243eb9775ff3d0` · `limitation: sem aplicação D1` · `required_for_acceptance: true`

**Gatilho / resultado esperado:** denial 429, corrida, Retry-After ou drift de schema.
**Modo / ambiente / permissão:** LOCAL_ISOLATED; D1 real é AUTHORIZED_OPERATION.
**Entradas e validação:** bytes, tempo e tenant sintéticos; migration versionada.
**Pré-condições:** identificar core, CAS ou FSM; não aplicar DDL apenas para certificar.

1. Reproduza o limiar, corrida ou calendário com o fake correto.
2. Rode `cargo test --locked --offline -p corelink-billing --test quota_core_prop_quota`.
3. Rode `cargo test --locked --offline -p corelink-billing --test quota_cas_prop_quota_cas`.
4. Leia migration e teste canônico; não execute contra D1.
5. Para schema, prepare backup e roll-forward com owner D1.

**Falhas e parada:** dados existentes, D1, rollback incerto ou regra contraditória.
**Recuperação:** parar antes de DDL; restaurar compatibilidade local e rerodar testes.
**Estado:** bloqueado e não executado nesta passagem; sem output no source pin; D1 `BLOCKED_FOR_OPERATION`.
**Evidência requerida:** versão, predicado, boundary e saída `EXECUTED_LOCAL`.

[Índice de procedimentos](#m02)

<a id="proc-005"></a>

### PROC-005 — Diagnosticar FSM de quota

`review_status: REVIEWED` · `execution_status: BLOCKED_NOT_EXECUTED` · `mode: LOCAL_ISOLATED` ·
`environment: fake em memória` · `result: NOT_RUN` · `evidence: SOURCE` ·
`source_pin: cca798ff5bc2df660ecf2570ed243eb9775ff3d0` · `limitation: Worker/webhook não observado` · `required_for_acceptance: true`

**Gatilho / resultado esperado:** transição 80/95/100, suspensão ou reinstatement incorreto.
**Modo / ambiente / permissão:** LOCAL_ISOLATED; fake em memória.
**Entradas e validação:** tenant sintético, utilização e contador de falhas.
**Pré-condições:** selecionar INV-008 ou INV-009.

1. Reproduza entrada no limiar exato.
2. Verifique transição, no-op idempotente e audit.
3. Induza falha de audit e confirme ausência de UPSERT.
4. Rode `cargo test --locked --offline -p corelink-billing --test quota_fsm_prop_quota_fsm`.

**Falhas e parada:** pedido para atuar em webhook, Worker ou estado real.
**Recuperação:** descarte fake e escale estado remoto.
**Estado:** bloqueado e não executado nesta passagem; sem output no source pin; binding Worker/webhook não observado.
**Evidência requerida:** estado anterior/posterior, entrada, audit e saída `EXECUTED_LOCAL`.

[Índice de procedimentos](#m02)

<a id="proc-006"></a>

### PROC-006 — Diagnosticar replay forense

`review_status: REVIEWED` · `execution_status: BLOCKED_NOT_EXECUTED` · `mode: LOCAL_ISOLATED` ·
`environment: archive/ledger fake` · `result: NOT_RUN` · `evidence: SOURCE` ·
`source_pin: cca798ff5bc2df660ecf2570ed243eb9775ff3d0` · `limitation: R2/D1/RBAC não observado` · `required_for_acceptance: true`

**Gatilho / resultado esperado:** role negada, dry-run mutando, ledger divergente ou drift.
**Modo / ambiente / permissão:** LOCAL_ISOLATED; archive e ledger fake.
**Entradas e validação:** UUID/tier/tenant sintéticos; sem dados de cliente.
**Pré-condições:** selecionar API-016 e INV-010/011.

1. Teste role diferente de `billing_forensics_admin`.
2. Teste dry-run sem escrita no ledger.
3. Reenvie request id e depois payload divergente.
4. Rode `cargo test --locked --offline -p corelink-billing --test replay_prop_billing_replay`.

**Falhas e parada:** R2, D1, RBAC real ou recuperação de dado de cliente.
**Recuperação:** descarte fake; não apague/reexecute ledger real sem owner.
**Estado:** bloqueado e não executado nesta passagem; sem output no source pin; operação real `BLOCKED_FOR_OPERATION`.
**Evidência requerida:** request sintético, braço, audit e outcome `EXECUTED_LOCAL`.

[Índice de procedimentos](#m02)

<a id="proc-007"></a>

### PROC-007 — Alterar caminho reexportado

`review_status: REVIEWED` · `execution_status: REVIEWED_NOT_EXECUTED` · `mode: LOCAL_ISOLATED` ·
`environment: checkout e consumer selecionado` · `result: NOT_RUN` · `evidence: SOURCE` ·
`limitation: suite de origem não observada` · `required_for_acceptance: false`

**Gatilho / resultado esperado:** criar, mover, remover ou renomear alias canônico.
**Modo / ambiente / permissão:** LOCAL_ISOLATED; coordenação com origem.
**Entradas e validação:** caminho antigo/novo, crate de origem e consumers conhecidos.
**Pré-condições:** ler REL-002–009 e API-001–010.

1. Confirme que o símbolo não é implementação billing.
2. Compile caminho antigo e canônico em consumer selecionado.
3. Preserve alias ou publique migração versionada.
4. Rode PROC-002 e suite da origem quando autorizada.

**Falhas e parada:** incompatibilidade, consumer não enumerado ou owner indisponível.
**Recuperação:** reintroduzir alias; revert não repara wire/dado externo.
**Certificação:** reviewed-not-executed; cold review dos bytes finais é obrigatória.
**Evidência:** API diff, grafo, comandos, consumers e decisão do owner.

[Índice de procedimentos](#m02)


<a id="m04"></a>
## M04 — Matriz de testes e validação

| Tipo de mudança | Package / target | Comando ou PROC | Predicado | Ambiente / evidência |
|---|---|---|---|---|
| Rust interno | billing / nativo | PROC-002 | pacote selecionado | `BLOCKED_NOT_EXECUTED`; source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`; sem output |
| abuse | `abuse_prop_abuse` | PROC-003 | score/audit; inclui falha de audit pré e pós-update | `BLOCKED_NOT_EXECUTED`; source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`; sem output |
| quota core | `quota_core_prop_quota` | PROC-004 | limite/isolamento | `BLOCKED_NOT_EXECUTED`; source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`; sem output |
| quota CAS | `quota_cas_prop_quota_cas` | PROC-004 | race/calendário | `BLOCKED_NOT_EXECUTED`; source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`; sem output |
| FSM | `quota_fsm_prop_quota_fsm` | PROC-005 | ladder/audit | `BLOCKED_NOT_EXECUTED`; source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`; sem output |
| replay | `replay_prop_billing_replay` | PROC-006 | role/dry-run/idempotência | `BLOCKED_NOT_EXECUTED`; source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`; sem output |
| alias | billing + origem/consumer | PROC-007 | paths preservados | `REVIEWED_NOT_EXECUTED` |

**Negativos obrigatórios:** audit falho, limite exato, payload divergente, role negada e auto-suspend.
**Gates globais compartilhados:** CI billing não foi inventariado nesta edição. Não execute suites live,
ignored ou deploy indiscriminadamente.

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

| Superfície | Reversível? | Condição / dados existentes | Ação segura | Prova |
|---|---|---|---|---|
| código/fake | sim | sem efeito externo | restaurar API e rerodar PROC | suite verde |
| reexport | condicional | consumer pode usar alias | reintroduzir compatibilidade | compilar paths |
| migration D1 | não só por revert | schema/dados podem existir | backup e roll-forward | leitura/plano owner |
| Stripe/R2/ledger | não só por revert | pagamento/dado pode existir | parar e escalar | operador |

A fachada não cria estado; módulos internos possuem fakes e descrevem mirrors remotos não observados.
`git revert` não recupera schema aplicado, pagamento, evento remoto, objeto ou ledger já escrito.

<a id="m06"></a>
## M06 — Escalonamento e registro de manutenção

| Condição | Responsável / rota | Evidência mínima | Ação vedada |
|---|---|---|---|
| D1/schema/dado real | `owner UNKNOWN`; rota D1 autorizada | versão, backup, query redigida | aplicar DDL sem plano |
| Stripe/cobrança | `owner UNKNOWN`; rota Stripe/financeiro | correlação sem segredo | retry/cancelamento cego |
| R2/replay/ledger | `owner UNKNOWN`; rota dados/RBAC | request redigido e tenant scope | apagar/reexecutar dado |
| alias origem | owner da crate reexportada | API e consumer | mover implementação unilateralmente |
| prova insuficiente | `@gmhelmold` somente revisão; owner operacional UNKNOWN | baseline e lacuna | declarar runtime/approval |

Após a tarefa: registrar baseline e resultado; limpar só temporários próprios; reconciliar documentos
e backlog; encaminhar hashes alterados para cold review.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01).
