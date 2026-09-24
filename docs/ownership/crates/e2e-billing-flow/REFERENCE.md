---
schema: corelink-ownership/1.1
document: reference
package: e2e-billing-flow
manifest: tests/e2e-billing-flow/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: e2e-billing-flow-pilot-source-20260920
---

# e2e-billing-flow — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) · [Contratos](#r04) · [Estado](#r05) · [Configuração](#r06) · [Falhas](#r07) · [Verificação](#r08).

<a id="r01"></a>
## R01 — Identidade e função

`e2e-billing-flow` é um package de teste, `publish = false`, que compõe fakes e ledgers in-memory para testar signup → Starter checkout → ativação → cancelamento → reembolso → gate DSR. Ele não é serviço, SDK ou adaptador Stripe e, pelo manifesto, não realiza I/O de rede no fluxo padrão.

| Campo | Valor verificado |
|---|---|
| Manifesto/target | `tests/e2e-billing-flow/Cargo.toml` / library + `end_to_end` |
| Dependências normais | `corelink-signup`, `corelink-tier-selection`, `corelink-billing-stripe`, `corelink-dsr`, `hex`, `sha2`, `uuid`, `thiserror` |
| Tempo e segredo de fixture | constantes fixas, `TEST_WEBHOOK_SECRET` de teste |
| Runtime | não observado |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Superfície | Implementação | Contrato | Composição/operação/revisão |
|---|---|---|---|
| jornada/estado local | este harness | APIs de `BillingHarness` | `BillingHarness::setup`; runner local/CI, operador não verificado |
| signup/tier/Stripe/DSR | crates upstream | seus owners | composição fake aqui; operadores upstream não verificados |
| HMAC/verificação | billing-stripe | primitive canônica | não há Stripe real neste package |
| cobrança/dado pessoal | não pertence aqui | contratos externos | operação autorizada; rota real não verificada |

O harness adiciona gates de orquestração para teste; isso não transfere a propriedade da política de faturamento ou DSR para este package.

<a id="r03"></a>
## R03 — Mapa da implementação

| Fonte | Papel | Evidência |
|---|---|---|
| `src/lib.rs` | fronteira pública, lints e reexports | crate root |
| `src/harness.rs` | fixture, estado, gates, webhook e DSR | `BillingHarness` |
| `tests/end_to_end.rs` | seis cenários de jornada/negação | integration target |
| `Cargo.toml` | composição, `publish=false` | manifesto |

<a id="r04"></a>
## R04 — Contratos públicos

[API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004).

<a id="api-001"></a>
### API-001 — Fixture e identidade determinística
**Símbolos:** `BillingHarness::setup`, `make_test_tenant`, `derive_*`, `FIXED_NOW_*`.
**Entradas/saída:** slug de teste → tenant/UUIDs determinísticos; setup → fakes e relógio fixo.
**Efeito:** só estado in-memory. **Compatibilidade:** mudar derivação muda reprodutibilidade e fixtures. **Prova:** INV-001; `harness.rs`.
[Índice de contratos](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Jornada e ciclo de assinatura
**Símbolos:** `signup_and_accept_dpa`, `select_starter_tier`, `deliver_checkout_completed`, `replay_checkout_completed`, `cancel_subscription`, `has_access_at`.
**Pré-condições:** signup antes de tier; sessão pendente antes de ativação; estado Active antes de cancelar.
**Saída/erros:** receipts upstream ou `BillingHarnessError`; replay deve retornar `DuplicateIgnored`. **Prova:** INV-002; cenários 1–3.
[Índice de contratos](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Webhook assinado e reembolso
**Símbolos:** `build_signed_webhook`, `deliver_signed_webhook`, `process_refund`, `SignedWebhook`.
**Pré-condição:** tenant em `CancelScheduledAtPeriodEnd`; payload/header e relógio de fixture. **Pós-condição desejada:** somente webhook aceito deixa `Refunded`; rejeição preserva o estado anterior.
**Efeito/erro:** hoje `process_refund` grava `Refunded` antes de chamar o handler; assinatura, skew ou erro de log posterior pode deixar estado prematuro. **Limite:** não chama Stripe. **INV/REL:** INV-003, REL-E2E-003/005; **prova:** SRC-003/004.
[Índice de contratos](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Gate de erasure
**Símbolos:** `request_dsr_erasure`, `SubscriptionLifecycleState`, `HarnessGateEvent`.
**Regra:** Active e CancelScheduled são rejeitados antes do endpoint; Refunded e PreCheckout seguem à DSR, sujeito a MFA/decisão upstream.
**Efeito:** log local de gate e, somente quando permitido, chamada ao fake DSR. **Prova:** INV-004; cenário 4.
[Índice de contratos](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

[INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004).

Fluxo nominal: `PreCheckout → PendingActivation → Active → CancelScheduledAtPeriodEnd → Refunded`. `has_access_at` é verdadeiro em Active e antes de `period_end_ms` no estado cancelado; não é um entitlement de produção. No fluxo de erro atual, a atribuição a `Refunded` precede a validação do webhook e não há rollback documentado.

<a id="inv-001"></a>
### INV-001 — Fixture é reprodutível
**Predicado:** mesmo slug produz mesmos IDs/tenant e relógio não varia. **Imposição:** funções de derivação e constantes. **Violação:** teste depende de hora/aleatoriedade externa. **Verificação:** unit tests. **Estado:** SOURCE.
[Índice de invariantes](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Ativação não duplica por replay
**Predicado:** um `event_id` ativado não produz segunda ativação. **Imposição:** ledger tier upstream, exposto por replay. **Violação:** duas ativações para evento. **Verificação:** cenário 2. **Estado:** SOURCE, execução desta campanha pendente.
[Índice de invariantes](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Reembolso exige cancelamento e assinatura válida
**Predicado alvo:** só estado cancel-pending com webhook aceito chega a `Refunded`, e qualquer rejeição mantém o estado anterior. **Imposição atual:** `harness.rs:813–835` valida o estado, atribui `Refunded`, depois chama o handler; não existe rollback se o handler rejeitar. **Violação:** assinatura/skew/log failure pode deixar `Refunded` sem webhook aceito. **Verificação:** cenário 5 cobre `deliver_signed_webhook`, não `process_refund`; adicionar caso de `process_refund` adulterado e definir correção (validar antes de mutar ou restaurar estado). **Estado:** `SOURCE_CONFLICT/FIX_FIRST`.
[Índice de invariantes](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Erasure não alcança DSR enquanto assinatura bloqueia
**Predicado:** Active/CancelScheduled retornam erro antes de `dsr.submit`. **Imposição:** gate em `request_dsr_erasure`. **Violação:** audit/endpoint DSR recebe a solicitação. **Verificação:** cenário 4. **Estado:** SOURCE.
[Índice de invariantes](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

Não declara features nem configuração externa de serviço. `TEST_WEBHOOK_SECRET` é material de fixture compilado, não credencial operacional; não reutilize nem promova-o. A variante Stripe live pertence a outro package/contexto e não é ativada por este harness. `Cargo.lock` e o SBOM registram o package como artefato de workspace, não como deployment.

<a id="r07"></a>
## R07 — Falhas e observabilidade

| Sinal | Significado | Ação |
|---|---|---|
| `BillingHarnessError::Invariant` | ordem de jornada inválida | corrigir cenário/contrato |
| `Stripe(SignatureRejected)` | payload/header incompatível | manter fail-closed; conferir se `process_refund` não deixou lifecycle mutado |
| `DsrBlockedSubscriptionActive` | gate local antes de DSR | não bypassar |
| logs de audit in-memory | evidência de teste | não é telemetry/runtime |
| lifecycle `Refunded` após erro Stripe | mutação anterior ao handler em `process_refund` | bloquear aceitação, preservar estado ou aplicar rollback explícito |

<a id="r08"></a>
## R08 — Verificação e evidências

| ID | Fonte | Blob/origem | Classe | Resultado/limite |
|---|---|---|---|---|
| SRC-001 | `tests/e2e-billing-flow/Cargo.toml:1–30` | `cca798ff5` / `1ae56c9eb087110cfe2ead4cf657d0f87cf8fcc4` | `SOURCE` | package, target e 8 dependências normais |
| SRC-002 | `tests/e2e-billing-flow/src/lib.rs:1–55` | `cca798ff5` / `63aef8d0a99b17ba39fb007b738c79d58027ffa3` | `SOURCE` | superfície pública e limites de harness |
| SRC-003 | `tests/e2e-billing-flow/src/harness.rs:727–878` | `cca798ff5` / `8dabd41d1b379f9aab5bb7d3ba411d010b82c2fe` | `SOURCE` | lifecycle, HMAC, DSR e conflito de ordem/rollback |
| SRC-004 | `tests/e2e-billing-flow/tests/end_to_end.rs:53–487` | `cca798ff5` / `110c493b42c6d8d756ce976de4a856163b6e6483` | `SOURCE` | seis cenários; tamper usa `deliver_signed_webhook` diretamente |
| SRC-005 | `Cargo.lock:3004–3014` | `cca798ff5` / `49daaed2e9b17578e8368ac173b183a1eca79345` | `SOURCE` | lockfile enumera as mesmas 8 dependências; não é execução |
| SRC-006 | `.sbom/cyclonedx-rust.json:4407–4422` | `cca798ff5` / `e5d5e52911cf7da05319fe09871ecd1e7e649c5e` | `SOURCE` | package aparece em SBOM; não prova deployment |
| SRC-007 | `Cargo.toml:300–302` | `cca798ff5` / `801ee986c93374461d468ed6e643345e8c2ed8f1` | `SOURCE` | membro do workspace; seleção ainda não é execução |

Foram lidos manifesto, library, harness e cenário. Os conceitos canônicos relevantes permanecem em [billing-commerce](../../../knowledge/crates/billing-commerce.md), [billing-pipeline](../../../knowledge/crates/billing-pipeline.md), [money path](../../../knowledge/launch/money-path.md) e [Stripe activation](../../../knowledge/launch/stripe-activation-webhook.md); este harness aplica suas fronteiras, sem redefinir política. Há seis cenários de integração declarados, incluindo replay, janela de acesso, bloqueio DSR, HMAC adulterado e refund sem cancelamento. Não se executou `cargo test` nesta etapa: o documento oferece `SOURCE`, não `EXECUTED_LOCAL`; não há `DEPLOYMENT` ou `OBSERVED_RUNTIME`.

**Estado de alcance:** `implemented=yes` (`SRC-002/003/004`), `wired=yes` para membro/target (`SRC-001/007`), `runtime_verified=unknown` (nenhuma execução). A aceitação do INV-003 permanece bloqueada pelo conflito `SRC-003`.

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#r01).
