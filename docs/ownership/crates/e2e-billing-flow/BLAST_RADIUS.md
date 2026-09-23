---
schema: corelink-ownership/1.1
document: blast_radius
package: e2e-billing-flow
manifest: tests/e2e-billing-flow/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: e2e-billing-flow-pilot-source-20260920
---

# e2e-billing-flow — blast radius

[Escopo](#b01) · [Método](#b02) · [Diretas](#b03) · [Propagação](#b04) · [Mudança](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Escopo

Relações do harness de teste e dos contratos que ele compõe. Relações são de teste/contrato até que uma seleção e execução específica demonstrem algo além disso.

<a id="b02"></a>
## B02 — Método e populações

| Camada | Método/evidência | Limite |
|---|---|---|
| Inventário | manifesto, alvo e dependências | não prova comportamento |
| Resolução registrada | `cargo tree --locked --offline -p e2e-billing-flow --edges normal` | seleção histórica; saída não anexada; não é runtime |
| Semântica | `harness.rs`, cenários e APIs upstream | não prova deploy |
| Orquestração | `nightly.yml` chama `ci-bounded-workspace-tests.sh` | seleção ampla de workspace; execução não observada |
| Externa | Stripe/DSR real | fora de escopo sem autorização |

<a id="b03"></a>
## B03 — Relações diretas

### REL-E2E-001 — signup
**Identity/endpoints:** `repo:1232040291:dependency:e2e-billing-flow-signup-001`; harness → `corelink-signup`.
**Tipo/endpoints:** dependência/runtime fake; harness → `corelink-signup`.
**Superfície:** provisionamento e `InMemoryAtomicSignupStore`.
**Ativação/falha:** `signup_and_accept_dpa`; rejeição upstream retorna erro e não cria estado local válido.
**Contrato/estado:** `SignupOutcome::Provisioned` produz tenant local; sem persistência externa. **Limite:** fake/in-memory.
**Validação/coordenação:** cenário happy/replay; owner signup não verificado; SRC-001/003/004.

### REL-E2E-002 — tier e checkout
**Identity/endpoints:** `repo:1232040291:dependency:e2e-billing-flow-tier-002`; harness → `corelink-tier-selection`.
**Tipo/endpoints:** dependência/runtime fake; harness → `corelink-tier-selection`.
**Superfície:** Starter, fake Stripe, receipt e ativação.
**Ativação/falha:** após signup; DPA/ledger pode rejeitar; `event_id` repetido deve deduplicar.
**Contrato/estado:** Starter cria sessão pendente e evento aceito ativa; estado é local ao harness/ledger fake. **Limite:** sem checkout real.
**Validação/coordenação:** cenários 1–3; owner tier não verificado; SRC-001/003/004.

### REL-E2E-003 — assinatura de webhook
**Identity/endpoints:** `repo:1232040291:dependency:e2e-billing-flow-stripe-003`; harness → `corelink-billing-stripe`.
**Tipo/endpoints:** dependência/contrato; harness → `corelink-billing-stripe`.
**Superfície:** `compute_signature`, handler in-memory, tipos de evento/audit.
**Ativação/falha:** webhook construído com fixture; payload adulterado é rejeitado antes da decisão aceita.

**Contrato/estado:** HMAC verifica bytes do fixture e insere evento no log fake; cenário 5 chama diretamente `deliver_signed_webhook`, sem passar por `process_refund`. **Limite:** não demonstra endpoint Stripe nem a falha de `process_refund`.
**Validação/coordenação:** cenário 5 valida somente rejeição de HMAC no handler direto e preservação do estado cancel-pending; `process_refund` muta antes do handler e essa falha continua sem teste. Owner billing-stripe não verificado; SRC-003/004.

### REL-E2E-004 — DSR
**Identity/endpoints:** `repo:1232040291:dependency:e2e-billing-flow-dsr-004`; harness → `corelink-dsr`.
**Tipo/endpoints:** dependência/runtime fake; harness → `corelink-dsr`.
**Superfície:** `DsrRequest`, erasure, MFA e decision.
**Ativação/falha:** somente PreCheckout/Refunded passam do gate local; erro do endpoint é propagado.
**Contrato/estado:** Active/CancelScheduled bloqueiam antes de `dsr.submit`; Refunded/PreCheckout alcançam fake DSR. **Limite:** sem DSR real.
**Validação/coordenação:** cenário 4; owner DSR não verificado; SRC-003/004.

### REL-E2E-005 — estado/orquestração local
**Identity/endpoints:** `repo:1232040291:boundary:e2e-billing-flow-lifecycle-005`; métodos públicos → `TenantState`/mutex/log.
**Tipo/endpoints:** dados; métodos públicos → `TenantState`/mutex/log.
**Superfície:** lifecycle e `HarnessGateEvent`.
**Ativação/falha:** cada jornada modifica estado em memória; mutex envenenado retorna `Invariant` em mutação.
**Contrato/estado:** lifecycle, `period_end_ms` e gate log vivem em memória; mutex é a fronteira de concorrência. **Limite:** não é persistência/telemetry de produto.
**Validação/coordenação:** testes de cenário; SRC-003/004; rollback de refund rejeitado é desconhecido/faltante.

### REL-E2E-006 — contratos de auditoria
**Identity/endpoints:** `repo:1232040291:test:e2e-billing-flow-audit-006`; cenários → sinks in-memory.
**Tipo/endpoints:** teste; harness → sinks in-memory dos quatro crates.
**Superfície:** snapshots e ordem de eventos.
**Ativação/falha:** cenários afirmam received/verified, duplicate e gate local; ausência/ordem errada falha no teste.
**Contrato/estado:** sinks registram received/verified/duplicate/gate na ordem observável do fake. **Limite:** não é export audit real.
**Validação/coordenação:** alvo `end_to_end` ainda não executado; SRC-003/004.

### REL-E2E-007 — dados de fixture
**Identity/endpoints:** `repo:1232040291:data:e2e-billing-flow-fixture-007`; slug → UUIDs/email/HMAC fixture.
**Tipo/endpoints:** dados/segurança; slug → UUIDs, email de exemplo, HMAC fixture.
**Superfície:** `make_test_tenant`, `derive_*`, `TEST_WEBHOOK_SECRET`.
**Ativação/falha:** qualquer mudança altera vetores/replay; segredo é não operacional mas deve permanecer limitado a teste.
**Contrato/estado:** derivação é determinística por slug/salt; segredo só é fixture compilada. **Limite:** não usar fora deste package.
**Validação/coordenação:** unit tests e revisão; SRC-003/004; nenhum dado real.

<a id="rel-e2e-008"></a>
### REL-E2E-008 — piloto de onboarding
**Identity/endpoints:** ID compartilhado `repo:1232040291:relation:e2e-pilot-onboarding-to-e2e-billing-flow-coverage-008`; `tests/e2e-pilot-onboarding/src/lib.rs` → este registro de cobertura.

**Tipo/endpoints:** contrato de teste/documentação; `e2e-pilot-onboarding` → referência a este harness.

**Superfície/ativação:** o crate root do onboarding declara que `e2e-billing-flow` fixa contratos internos, enquanto o onboarding fixa somente a forma da jornada cross-subsystem.

**Efeito/falha:** mudança neste harness pode reduzir a cobertura complementar sem criar dependência Cargo ou execução transitiva.

**Contrato/estado:** relação documental, sem dependência Cargo/runtime. **Limite:** não transfere ownership nem prova execução.

**Validação/coordenação:** ID e fatos compartilhados reconciliados com `e2e-pilot-onboarding` REL-033. É relação documental; nenhum dos registros demonstra execução. Fonte `tests/e2e-pilot-onboarding/src/lib.rs`, SRC-004.

### REL-E2E-009 — smoke noturno do workspace
**Identity/endpoints:** `repo:1232040291:build:e2e-billing-flow-workspace-smoke-009`; workflow `nightly` → seleção workspace.

**Tipo/endpoints:** test/build; workflow `nightly` → `e2e-billing-flow`.

**Superfície/ativação:** cron ou dispatch chama `ci-bounded-workspace-tests.sh`, que seleciona `--workspace --all-targets` por nextest ou cargo test.

**Efeito/falha:** alteração do harness pode falhar a suíte ampla; limite de 1.500 s encerra o grupo de processos completo.

**Contrato/estado:** `--workspace --all-targets` seleciona o package quando o workspace está íntegro; timeout é 1.500 s. **Limite:** seleção estática não prova run, resultado ou ambiente.

**Validação/coordenação:** revisar workflow/script e observar CI autorizada; `.github/workflows/nightly.yml`, `scripts/ci-bounded-workspace-tests.sh`; execution não observada.

<a id="b04"></a>
## B04 — Propagação transitiva

| Destino | REL/caminho | Condição | Efeito/limite | Contenção/validação |
|---|---|---|---|---|
| harness → ledgers fake | REL-E2E-001–004 | API/erro upstream muda | compile ou significado de cenário muda; não altera produção por si | selecionar cenário e coordenar owner; sem runtime |
| lifecycle local | REL-E2E-005 + REL-E2E-003 | `process_refund` aceita somente cancel-pending, muda para `Refunded` e depois o handler rejeita | falha de HMAC pode deixar `Refunded` prematuro; cenário 5 não alcança esse caminho | adicionar teste que chama `process_refund` com tamper e verifica a rejeição e o estado; corrigir ordem/rollback antes de aceitar |
| audit sinks | REL-E2E-006 | evento/ordem muda | cenário falha ou perde detecção | afirmar sequência e estado final no target |
| workspace/artefatos | REL-E2E-009 + `Cargo.lock`/SBOM | seleção ou geração muda | package pode sair da suíte ou do inventário | revisar script, lock e SBOM; execução permanece desconhecida |

O fluxo de dados é fixture → fake/ledger → receipt/audit; a direção de impacto de produção não deve ser inferida a partir desse fluxo isolado.

<a id="b05"></a>
## B05 — Mudança → impacto → validação

| Mudança | Impacto provável | Validação mínima |
|---|---|---|
| estado/gate local | REL-E2E-005 | cenário deixa de desafiar erro | `e2e-billing-flow` / `end_to_end` / features default; caso feliz + negação |
| HMAC no handler direto | REL-E2E-003 | falha de assinatura ou audit ordering incorreta | `end_to_end` / default; cenário 5 chama `deliver_signed_webhook` com payload adulterado e exige `SignatureRejected`, audit `WebhookReceived` antes de `SignatureRejected` e lifecycle `CancelScheduledAtPeriodEnd` |
| falha HMAC via `process_refund` | REL-E2E-003/005 | o método muta lifecycle para `Refunded` antes do handler; uma rejeição pode deixar mutação prematura | sem cenário atual; adicionar caso em `end_to_end` que chama `process_refund` com payload adulterado e exige erro + estado preservado. Hoje esse predicado falha por falta de rollback; validar a correção antes de declarar PASS |
| API upstream | REL-E2E-001–004 | compile/semântica do receipt | package + target `end_to_end`; censo e cenário selecionado |
| fixture/tempo | REL-E2E-007 | vetores não reproduzíveis | library + unit tests; entradas determinísticas |
| lock/SBOM/CI | REL-E2E-009 | seleção/inventário divergente | workspace script e artefatos; revisão estática, sem alegar execução |

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

| População | Descobertos | Documentados | Excluídos | Desconhecidos |
|---|---:|---:|---:|---:|
| Dependências normais | 8 | 8 | 0 | 0 |
| Consumidores Cargo normais | 0 encontrados na inversa registrada | 0 | 0 | 0 estruturais; execução dinâmica não observada |
| Fronteiras semânticas | 9 | 9 | 0 | 0 no escopo declarado |
| Build/artefatos | 2 (`Cargo.lock`, SBOM) | 2 | 0 | deployment não aplicável |

A resolução normal foi registrada em `CAMPAIGN_STATUS.md:173`, mas a saída do comando não está anexada a este package; a contagem de dependências é reconciliada diretamente com `Cargo.toml`/`Cargo.lock`. Não há job nominal do package, mas o smoke noturno seleciona todo o workspace e todos os targets.

`e2e-pilot-onboarding` é relação de contrato/documentação, não dependência Cargo; o fingerprint compartilhado de REL-E2E-008 está reconciliado com `e2e-pilot-onboarding` REL-033. Os fatos compartilhados limitam-se à referência documental e à ausência de dependência Cargo/runtime. Ainda faltam resultado/ambiente da CI e confirmação de owners/rota de escalação.

A rotina Stripe de produção é uma superfície separada; não foi ligada a este harness por nome ou dependência. Não existem relações de telemetry/Stripe real/dados de cliente; qualquer uma exige evidência própria e autorização.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01).
