---
schema: corelink-ownership/1.1
document: maintenance
package: e2e-billing-flow
manifest: tests/e2e-billing-flow/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: e2e-billing-flow-pilot-source-20260920
---

# e2e-billing-flow — manual de manutenção

[Preparação](#m01) · [Seleção](#m02) · [Procedimentos](#m03) · [Matriz](#m04) · [Recuperação](#m05) · [Escalação](#m06).

<a id="m01"></a>
## M01 — Preparação segura

Trabalhe em baseline explícita e checkout limpo. Confirme que o alvo é o harness `publish=false`, não um endpoint. Não carregue `.env`, `STRIPE_*`, credenciais, dados de cliente ou webhook real. Leia [Referência](REFERENCE.md#r01) e [Relações](BLAST_RADIUS.md#b03).

<a id="m02"></a>
## M02 — Seleção de procedimento

| Sinal | Procedimento | Limite |
|---|---|---|
| transição/replay | [PROC-001](#proc-001) | estado in-memory |
| HMAC/reembolso | [PROC-002](#proc-002) | sem Stripe real |
| DSR ativo/cancelado | [PROC-003](#proc-003) | sem dado pessoal |
| mudança upstream | [PROC-004](#proc-004) | coordenar owner |
| execução local | [PROC-005](#proc-005) | não é produção |

<a id="m03"></a>
## M03 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — Diagnosticar transição ou replay
**Objetivo/gatilho:** explicar estado/receipt inesperado. **Pré-condições:** checkout no pin, tenant slug sintético e nenhum dado real. **Ambiente/permissões:** checkout isolado, somente leitura; sem acesso externo. **Entradas:** estado inicial, `event_id`, cenário e receipt. **Modo:** `READ_ONLY`.

1. Registre tenant slug, estado inicial e `event_id` sem dados reais.
2. Trace `select_starter_tier` e `deliver/replay_checkout_completed`.
3. Compare receipt com a pré-condição do ledger upstream.

**Esperado/predicado:** uma transição concreta explica o resultado e o replay não duplica ativação. **Pare:** contrato do tier for incerto. **Recuperação:** não alterar fonte; registrar owner e evidência. **Evidência:** função, estado, cenário e saída. **Registro:** `id=PROC-001`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=false`; `environment=checkout isolado`; `result=NOT_EXECUTED`; `review_evidence=SRC-001/003/004`; `execution_evidence=[]`; `limitations=sem execução Cargo nesta tarefa`. [Índice](#m02)

<a id="proc-002"></a>

### PROC-002 — Alterar ou investigar webhook/reembolso
**Objetivo/gatilho:** alterar ou investigar assinatura, evento ou refund. **Pré-condições:** fonte pinada, fixture sintética e estado `CancelScheduledAtPeriodEnd`; não usar segredo real. **Ambiente/permissões:** checkout descartável, permissões locais; sem Stripe/rede. **Entradas:** `SignedWebhook`, payload adulterado e estado inicial. **Modo:** `LOCAL_ISOLATED`.

1. Use apenas `build_signed_webhook` e o segredo de fixture.
2. O cenário 5 chama `deliver_signed_webhook` diretamente; não o trate como teste de `process_refund`.
3. O caso adulterado de `process_refund` não existe. O método grava `Refunded` antes do handler; rejeição pode deixar esse estado.
4. Confirme que refund sem cancelamento falha no gate antes da mutação.

**Esperado/predicado:** o handler direto rejeita e mantém cancel-pending. Um `process_refund` rejeitado deveria preservar estado, mas hoje deixa `Refunded`; refund válido chega a `Refunded`; refund sem cancelamento falha antes de mutar.

**Pare:** exija chave/endpoint Stripe ou se a rejeição deixa lifecycle mutado. **Recuperação:** capture lifecycle, erro e audit snapshots; descarte o harness e siga [M05](#m05). Não há setter/rollback público. **Evidência:** estado antes/depois, erro, audit snapshots, comando e SHA.

**Registro:** `id=PROC-002`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=true`; `environment=checkout isolado`; `result=SOURCE_CONFLICT/NOT_EXECUTED`; `review_evidence=SRC-003/004`; `execution_evidence=[]`; `limitations=process_refund muta antes do handler; caminho adulterado sem teste`. [Índice](#m02)

<a id="proc-003"></a>

### PROC-003 — Alterar gate DSR
**Objetivo/gatilho:** alterar ou investigar estado permitido/bloqueado ou MFA. **Pré-condições:** tenant sintético em estado conhecido e DSR fake; sem dado pessoal. **Ambiente/permissões:** checkout isolado, sem endpoint externo. **Entradas:** lifecycle, request tag e MFA sintética. **Modo:** `LOCAL_ISOLATED`.

1. Trace lifecycle antes de criar `DsrRequest`.
2. Teste Active e CancelScheduled como bloqueios antes do endpoint.
3. Teste Refunded com e sem MFA no fake.

**Esperado/predicado:** endpoint/audit DSR fica vazio no bloqueio e decisões pós-refund respeitam MFA. **Pare:** política/retention real mudar. **Recuperação:** reverter somente bytes/estado local; não compensar dado real. **Evidência:** estado, decisão, audit snapshot e cenário. **Registro:** `id=PROC-003`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=false`; `environment=checkout isolado`; `result=NOT_EXECUTED`; `review_evidence=SRC-003/004`; `execution_evidence=[]`; `limitations=sem execução Cargo nesta tarefa`. [Índice](#m02)

<a id="proc-004"></a>

### PROC-004 — Coordenar mudança de contrato upstream
**Objetivo/gatilho:** coordenar mudança de tipos, receipts ou audit upstream. **Pré-condições:** mudança identificada e owner/rota verificados. **Ambiente/permissões:** leitura do checkout e processo de revisão autorizado; sem alteração externa. **Entradas:** símbolos, versão, RELs, compatibilidade e cenário afetado. **Modo:** `READ_ONLY` até plano aceito.

1. Identifique qual dos quatro crates é dono da semântica.
2. Atualize relações, API e cenários afetados.
3. Registre compatibilidade e escolha teste seletivo.

**Esperado/predicado:** owner, impacto, compatibilidade e teste estão explícitos. **Pare:** incompatibilidade ou rota de escalação sem decisão. **Recuperação:** não atualizar docs parcialmente; manter estado stale e reabrir revisão. **Evidência:** símbolos, owner verificado, matriz e decisão. **Registro:** `id=PROC-004`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=false`; `environment=checkout pinado`; `result=NOT_EXECUTED`; `review_evidence=SRC-001/003/005`; `execution_evidence=[]`; `limitations=owners/rota upstream não verificados`. [Índice](#m02)

<a id="proc-005"></a>

### PROC-005 — Executar o alvo isolado
**Objetivo/gatilho:** validar mudança no harness/cenários. **Pré-condições:** checkout descartável, lockfile disponível, toolchain registrada e destino isolado; nenhuma rede/credencial. **Ambiente/permissões:** Rust local com permissões de escrever apenas no target temporário. **Entradas:** SHA, target `end_to_end`, features default e comando exato. **Modo:** `LOCAL_ISOLATED`.

1. Registre `git rev-parse HEAD`, `rustc -Vv` e `cargo -V`.
2. Execute `target_dir="$(mktemp -d "${TMPDIR:-/tmp}/corelink-e2e-target.XXXXXX")"`,
   registre o caminho e não use o `target/` padrão.
3. Execute `cargo test --target-dir "$target_dir" --locked --offline -p
   e2e-billing-flow --test end_to_end` em target isolado.
4. Preserve status e saída; confira checkout depois.

**Esperado/predicado:** os seis cenários do target passam e o estado final de cada cenário satisfaz as assertivas; isso confirma somente fakes selecionados.

**Pare:** lock/rede/segredo/serviço for requerido, `target_dir` não puder ser criado, ou o processo alterar fora do target.

**Recuperação:** remover somente o diretório temporário registrado e preservar saída; não relaxar assertivas.

**Evidência:** comando completo com `--target-dir`, caminho temporário, ambiente, status, stdout/stderr, SHA e diff pós-run.

**Registro:** `id=PROC-005`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=true`; `environment=target isolado não usado nesta tarefa`; `result=NOT_EXECUTED`; `review_evidence=SRC-001/004`; `execution_evidence=[]`; `limitations=Cargo proibido neste review; sem certificação local`. [Índice](#m02)


<a id="m04"></a>
## M04 — Matriz de testes/validação

| Mudança | Package/target/features e comando | Predicado/ambiente | Não prova |
|---|---|---|---|
| estado/replay | `e2e-billing-flow` / `end_to_end` / default; `cargo test --target-dir "$target_dir" --locked --offline -p e2e-billing-flow --test end_to_end` | cenários 1–3, target isolado | cobrança ativa |
| tamper no handler direto | `e2e-billing-flow` / `end_to_end` / default; `cargo test --target-dir "$target_dir" --locked --offline -p e2e-billing-flow --test end_to_end` | cenário 5 chama `deliver_signed_webhook`; espera `SignatureRejected`, `WebhookReceived` antes da rejeição e lifecycle `CancelScheduledAtPeriodEnd` | caminho de falha em `process_refund`; assinatura/endpoint Stripe reais |
| tamper via `process_refund` | mesmo package/target/features; `cargo test --target-dir "$target_dir" --locked --offline -p e2e-billing-flow --test end_to_end`, depois de adicionar o caso ausente | sem cobertura atual. O futuro caso deve chamar `process_refund` com tamper e exigir erro + lifecycle preservado; com a ordem atual, o lifecycle fica `Refunded`, então o predicado falha | correção/rollback ainda não implementado; Stripe real |
| refund sem cancelamento | mesmo package/target/features; `cargo test --target-dir "$target_dir" --locked --offline -p e2e-billing-flow --test end_to_end` | cenário 6; gate rejeita antes de mutar e estado permanece `Active` | reembolso Stripe real |
| gate DSR | mesmo package/target/features; `cargo test --target-dir "$target_dir" --locked --offline -p e2e-billing-flow --test end_to_end` | cenário 4; DSR fake vazio nos bloqueios | processamento legal real |
| fixture/tempo | library + unit tests / default; `cargo test --target-dir "$target_dir" --locked --offline -p e2e-billing-flow` | IDs/tempo reproduzíveis em target isolado | isolamento de produção |
| lock/SBOM/CI | workspace/script/artefatos; revisão estática, sem comando de runtime | membro, lock, SBOM e seleção nightly conciliados | execução CI ou deployment |

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

Ao falhar, capture lifecycle inicial/final, evento, audit snapshots e erro antes de editar. O cenário 5 prova apenas a chamada direta a `deliver_signed_webhook`. `process_refund` altera lifecycle antes do handler e não oferece rollback.

Para recuperar, descarte o harness inteiro. Crie outro com `BillingHarness::setup()`, refaça signup, tier, checkout e cancelamento usando a fixture determinística, e confirme `CancelScheduledAtPeriodEnd`. Não reutilize nem altere manualmente a instância que falhou; ela não tem API segura de restauração. Mantenha o procedimento bloqueado até um teste de tamper via `process_refund` confirmar rejeição e estado preservado. `RefundProcessed` não é emitido no erro, mas isso não desfaz a mutação.

Reverter um commit de harness restaura apenas a detecção em teste; não desfaz cobranças, webhooks, dados ou decisões externas. Para mudança de recibo/tipo, coordene com o owner e preserve cenário de compatibilidade até a migração estar decidida.

<a id="m06"></a>
## M06 — Escalação e evidências

Escale a `corelink-signup`, `corelink-tier-selection`, `corelink-billing-stripe` ou `corelink-dsr` conforme o contrato, mas registre a rota verificável antes de fechar; `.github/CODEOWNERS` (`@gmhelmold`) apenas solicita revisão e não prova independência. Escale operação para qualquer Stripe/DSR real. Capture baseline, cenário, estado, comando, saída e lacunas, com logs redigidos e referências `SRC/REL/PROC`. Aprovação documental exige cold review separado e não executa pagamento, reembolso ou erasure.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01).
