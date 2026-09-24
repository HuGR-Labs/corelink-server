---
schema: corelink-ownership/1.1
document: reference
package: e2e-signup-flow
manifest: tests/e2e-signup-flow/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w015-signup-flow-source-1177dad2
---

# e2e-signup-flow — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) ·
[Contratos](#r04) · [Estado](#r05) · [Configuração](#r06) ·
[Falhas](#r07) · [Verificação](#r08).

<a id="r01"></a>
## R01 — Identidade e função

`e2e-signup-flow` é um harness Rust `publish = false` para compor fakes de signup, DPA e seleção de tier com um CAS em memória. A jornada exercita provisionamento, aceite separado de DPA, tier Free/Starter e, no cenário Free, PUT/GET e verificação BLAKE3. Fonte estática: manifesto e pacote no commit `1177dad2ca2a9f21c29b5a118aa7944b77147798`; nenhuma execução é afirmada.

| Campo | Valor verificado |
|---|---|
| Package / manifesto | `e2e-signup-flow` / `tests/e2e-signup-flow/Cargo.toml` |
| Targets declarados | uma library (`src/lib.rs`) e seis alvos `[[test]]` |
| Módulos próprios | `helpers.rs`, `r2.rs`; módulo de teste unitário em `r2.rs` |
| Papel | harness de integração de teste; não é serviço, route ou adapter de produção |
| Publicação / runtime | `publish = false`; runtime não observado |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Superfície | Dono da implementação | Dono do contrato | Operação / aprovação |
|---|---|---|---|
| Fixtures, tipos auxiliares e cenários | este package | API de teste deste package | teste local; aprovador `UNKNOWN` |
| Orquestração de signup | `corelink-signup` | `corelink-signup` | operador/rota `UNKNOWN` |
| Aceite DPA e recibo | `corelink-dpa-acceptance` | `corelink-dpa-acceptance` | operador/rota `UNKNOWN` |
| Gate e seleção de tier | `corelink-tier-selection` | `corelink-tier-selection` | operador/rota `UNKNOWN` |
| Cliente Stripe declarado | `corelink-stripe-real` | `corelink-stripe-real` | provider/config fora do escopo; `UNKNOWN` |
| Digest e corpo verificado | `corelink-hash` | `corelink-hash` | armazenamento real fora do escopo; `UNKNOWN` |
| CAS simulado e contadores | este package (`r2.rs`) | API do fake deste package | estado efêmero de teste |

**Não faz:** não monta endpoint, não prova o signup live descrito em documentação de operação, não fornece persistência real e não concede acesso a provider. Não foi identificada absorção de um package antigo nem reexport de implementação upstream. Os conceitos de domínio permanecem com os owners upstream; não foi estabelecido um link OKF que cubra precisamente este harness.

<a id="r03"></a>
## R03 — Mapa da implementação

| Fonte | Papel e dados sob ownership | Natureza | Evidência |
|---|---|---|---|
| `src/lib.rs` | lint gates, módulos, reexports e constantes de fixture | facade local | linhas 36–58 |
| `src/helpers.rs` | ledgers fake, tenant determinístico, contextos, DPA e audit assertions | harness próprio | `setup_test_ledgers`, `make_test_tenant`, `verify_audit_chain` |
| `src/helpers.rs` | relógio, contador JTI e chaves RSA compartilhadas | fixtures públicas | `FixedClock`, `CountingJtiMinter`, `Keys`, `gen_keys` |
| `src/r2.rs` | mapa de PAT, objetos e contadores sob `Mutex` | fake próprio | `InMemoryR2Client` |
| `tests/happy_path_free.rs` | fluxo Free, blob round-trip e PAT não vinculado | alvo de teste | cenário positivo/negativo |
| `tests/happy_path_starter_stripe_test_mode.rs` | alvo de Starter; inclui variante `#[ignore]` declarada | alvo; limite estático | detalhes provider/config não inspecionados |
| `tests/adversarial_dpa_not_accepted.rs` | tier bloqueado antes de aceite DPA | alvo de teste | Free e Starter |
| `tests/adversarial_webhook_signature_invalid.rs` | assinatura inválida e estado de tier | alvo de teste | source only nesta revisão |
| `tests/idempotency_replay.rs` | replay de chave de idempotência | alvo de teste | contagem de commit e evento |
| `tests/prop_atomic_invariants.rs` | payloads proptest, atomicidade e replay | alvo property | default fonte: 1.000 casos |

**Inventário:** três módulos-fonte e seis arquivos de alvo declarados, conferidos no manifesto e na árvore Git do source pin. `r2.rs` também declara testes internos. O diretório não contém binário ou bench declarado.

<a id="r04"></a>
## R04 — Contratos públicos

[API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005).

<a id="api-001"></a>
### API-001 — Fixtures e conversões

**Símbolos exatos:** `setup_test_ledgers`, `make_test_tenant`, `ProvisionedTenant::from_response`, `dpa_ctx_for`, `dpa_request_for`, `tier_ctx_for`, `gen_keys`, `Keys::shared`. Entrada de tenant `&str` gera bundle: somente `email_hash` usa SHA-256 sobre o nome e o sufixo fixo; `idempotency key`, `clerk event` e `correlation id` usam strings derivadas diretamente do nome. `setup_test_ledgers` devolve `TestEnv` com stores e sinks in-memory. `gen_keys` cria/chama cache RSA por processo. **Efeito:** somente objetos locais segundo a fonte. **Compatibilidade:** alterar qualquer derivação afeta fixtures e outros harnesses que as espelham. **Vínculos/prova:** INV-001, REL-015–017; `src/helpers.rs:144–250,370–483`.

[Índice de contratos](#r04)

<a id="api-005"></a>
[↩](#r01)
### API-005 — Relógio, JTI e chaves RSA

**Símbolos exatos:** `FixedClock`, `impl Clock for FixedClock`, `CountingJtiMinter`, `impl JtiMinter for CountingJtiMinter`, `Keys::{private,public}`, `gen_keys`, `Keys::shared`. `FixedClock` retorna `now_ms`; o minter guarda `Mutex<u64>`, recupera lock poisoned, incrementa com saturação e emite `jti-{:08}`. `Keys` expõe `private: RsaPrivateKeyPem` e `public: RsaPublicKeyPem`; `gen_keys` chama o cache `OnceLock` e gera RSA 2048 quando ausente. **Efeito:** fixtures locais de tempo, JTI e chave por processo. **Prova:** INV-005; `src/helpers.rs:115–142,424–483`.

[Índice de contratos](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Resultado e audit assertions

**Símbolos exatos:** `ExpectedAuditEvent`, `TestEnv`, `verify_audit_chain`, `TierAuditSnapshotExt::snapshot_event_types`. A função recebe lista esperada e exige subsequência da sequência construída; retorna `Ok(())` ou `String` no primeiro item ausente. Os registros são coletados em blocos por sink — signup, DPA e tier — e não têm timestamp comum ordenado pelo helper. **Compatibilidade:** nomes/variantes e ordem dos blocos alteram assertions. **Vínculos/prova:** INV-003, REL-018; `src/helpers.rs:46–112,295–362,491–502`.

[Índice de contratos](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Fake CAS autenticado por PAT

**Símbolos exatos:** `InMemoryR2Client::{new,bind_pat,put,get,stat}`, `R2Error::{Unauthorized,Forbidden,NotFound,DigestMismatch}`, `R2StatReport`. `put` exige digest em `VerifiedBody` e PAT previamente ligado; `get` limita chave pelo tenant derivado do PAT e recalcula o digest. Estado e contadores ficam no mapa protegido por mutex. O tipo `Forbidden` está declarado, mas não há ramo que o produza no código revisto. **Vínculos/prova:** INV-004, REL-019 e chave compartilhada `hash-signup-r2-integrity-001`; `src/r2.rs:23–165`.

[Índice de contratos](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Constantes de fixture e exports

**Símbolos exatos:** módulos públicos `helpers`, `r2`; reexports `make_test_tenant`, `setup_test_ledgers`, `verify_audit_chain`, `ExpectedAuditEvent`, `ProvisionedTenant`, `TenantBundle`, `TestEnv`, `InMemoryR2Client`, `R2Error`, `R2StatReport`; constantes `TEST_DPA_VERSION`, `TEST_DPA_NOTICE_EN`, `TEST_WEBHOOK_SECRET`. Valores de notice e secret são fixtures no source; não são conteúdo legal ou credencial operacionais. **Compatibilidade:** modificar exports ou constantes requer checar todos os call sites de teste enumerados. **Prova:** `src/lib.rs:40–58`; REL-020.

[Índice de contratos](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Estado, fluxo e invariantes

| Estado / recurso | Chave e owner | Persistência / vida útil | Escrita / leitura |
|---|---|---|---|
| Stores e audit sinks | por `TestEnv` / fakes upstream | memória do teste | `setup_test_ledgers`; snapshots nos cenários |
| Gate DPA | `TenantId` + versão / fake de tier | memória do teste | cenário chama `accept` explicitamente |
| CAS | `tenant_id:digest_hex` / `InMemoryR2Client` | memória do objeto | PAT bind, PUT/GET, contadores |
| RSA fixture | cache `OnceLock` / process-wide | vida do processo | geração aleatória única por processo |
| Clock | `now_ms` fixo em `TestEnv` | vida da fixture | `1_700_000_000_000` |
| JTI counter | `Mutex<u64>` / `CountingJtiMinter` | vida da instância `TestEnv`; reinicia em novo `Default` | `mint`: `1` → `jti-00000001`, saturando; lock poisoned recupera |

**Índice:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005) · [FLOW-001](#flow-001).

<a id="inv-001"></a>
### INV-001 — Derivação do tenant é estável para o mesmo nome

**Predicado:** o mesmo `TenantBundle.name` (o input armazenado, sem slugificação adicional) produz o mesmo `email_hash` (SHA-256 com sufixo fixo), `idempotency key`, `clerk event` e `correlation id` conforme `make_test_tenant`; o valor não promete igualdade da chave RSA entre processos. **Imposição:** `make_test_tenant`. **Violação:** mudar o hash/sufixo do email ou os prefixos/separadores das strings quebra o vetor/peer. **Prova:** cenários e source; não executados nesta revisão. **Estado:** SOURCE.

[Índice de estado](#r05)

<a id="inv-005"></a>
[↩](#r01)
### INV-005 — JTI e relógio permanecem locais e reproduzíveis

**Predicado:** cada `CountingJtiMinter` inicia em zero, emite sequência `jti-00000001` em diante com saturação, e cada `FixedClock` retorna seu `now_ms`; novo fixture reinicia apenas o contador, enquanto `Keys::shared` mantém a chave no processo. **Imposição:** `Mutex<u64>`, `mint`, `FixedClock::now_ms`, `OnceLock`. **Violação:** sequência/reset ou processo divergirem. **Prova:** source; não executado. **Estado:** SOURCE.

[Índice de estado](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Replay do signup preserva um único provisionamento

**Predicado testado:** replay do mesmo request retorna `Duplicate` com mesmo `signup_id`, um tenant committed e sem segundo evento `Started`; property source exige um commit e `Started`/`Completed` na primeira chamada. **Imposição:** crate upstream e asserts do harness. **Violação:** segundo commit/ID divergente. **Prova:** `idempotency_replay.rs`, `prop_atomic_invariants.rs`; não executados. **Estado:** SOURCE.

[Índice de estado](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Seleção de tier requer o gate DPA aceito

**Predicado testado:** sem `accept`, tentativa Free e Starter resulta em `DpaRequired` e nenhuma sessão do fake Stripe. O fluxo feliz aceita DPA e altera o gate compartilhado explicitamente. **Imposição:** ledger e seus fakes upstream; esta fonte prova apenas configuração/assertions do cenário. **Violação:** sucesso sem gate ou sessão antes da aceitação. **Prova:** adversarial e Free; não executados. **Estado:** SOURCE.

[Índice de estado](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Fake CAS limita acesso e confere o conteúdo

**Predicado:** PUT com PAT desconhecido retorna `Unauthorized`; GET resolve somente a chave `tenant:digest` do PAT e compara digest recalculado antes de retornar bytes. **Imposição:** `InMemoryR2Client::put/get`. **Violação:** ler tenant diferente ou devolver corpo com digest divergente. **Prova:** fonte do fake e casos no `r2.rs`/`happy_path_free.rs`; não executados. **Estado:** SOURCE. `Forbidden` não é alcançado pelo código atual.

[Índice de estado](#r05)

<a id="flow-001"></a>
[↩](#r01)
### FLOW-001 — Jornada Free composta

1. Criar `TestEnv` e `TenantBundle` determinísticos.
2. Provisionar signup no store fake e extrair `ProvisionedTenant`.
3. Aceitar DPA no serviço fake, verificar receipt e aceitar manualmente o gate de tier.
4. Selecionar Free e afirmar ausência de sessões Stripe no fake.
5. Ligar hash PAT ao CAS, PUT e GET sob `tenant:digest`, comparar bytes/digest e contadores.
6. Conferir a subsequência de audit pelo helper.

A sequência é fixture/código fonte, não evidência de execução. O passo 3 é costura manual; não prova que o serviço DPA de produção alimenta o gate no sistema implantado. `verify_audit_chain` concatena snapshots por família e não mede uma ordem temporal global.

[Índice de estado](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração e targets

O manifesto não declara features próprias. Declara library mais seis `[[test]]`, dependências first-party para signup, DPA, tier, Stripe adapter e hash, dependências de workspace (`bytes`, `blake3`, `hex`, `sha2`, `thiserror`, `tokio`), `rsa` com feature `pem`, `rand` e `proptest` dev. Os defaults efetivos dessas dependências não foram resolvidos com Cargo.

| Seletor | Fonte/default observado | Efeito / limite |
|---|---|---|
| `PROPTEST_CASES` | leitura no alvo property; fallback `1_000` | quantidade de casos no teste declarado |
| `#[ignore]` no alvo Starter | presença declarada no manifesto/source superficial | não implica que o target seja seguro para executar; credenciais, provider e configuração são `UNKNOWN` |

Não inspecionar provider config nem valores de segredo; não rodar nem habilitar a variante ignorada. `--ignored` e seleção ampla de workspace não são gates autorizados por esta referência.

<a id="r07"></a>
## R07 — Falhas e observabilidade

| Sinal | Causa representada | Efeito delimitado |
|---|---|---|
| `SignupOutcome::Duplicate` | replay de chave | resultado de fixture; sem conclusão sobre durabilidade real |
| `TierError::DpaRequired` | gate fake sem aceite/version | cenário espera rejeição antes de criar sessão no fake |
| `R2Error::Unauthorized` | hash PAT não ligado | erro do CAS em memória |
| `R2Error::NotFound` | chave ausente para tenant e digest | incrementa miss local |
| `R2Error::DigestMismatch` | corpo recuperado não confere digest | ramo de defesa local; não é backend R2 |
| audit snapshot mismatch | subsequência esperada ausente | helper retorna erro textual |

Os sinks e contadores descritos são fakes. Logs, métricas, durabilidade, auditoria exportada, tráfego, tenant e falha de provider não foram observados.

<a id="r08"></a>
## R08 — Verificação e desconhecidos

Fonte revisada no pin: `tests/e2e-signup-flow/Cargo.toml`, `src/{lib,helpers,r2}.rs` e cinco arquivos de alvo que não são o caminho Stripe ignorado; o sexto alvo foi tratado somente pelo limite estático declarado, sem inspecionar seu provider/setup. Comparação do manifesto e da membership do workspace entre pin e baseline não mostrou drift. Evidência é `SOURCE`; não se executou Cargo/Rust, teste, build, fuzz, rede, provider ou CI; não há `RESOLVED`, `EXECUTED_LOCAL`, `DEPLOYMENT` ou `OBSERVED_RUNTIME`.

Desconhecidos: grafo Cargo resolvido/inverso completo; consumidores dinâmicos; execução de qualquer cenário; configuração e credenciais do alvo ignorado; wiring de produção; persistência, runtime e deploy; rota real de escalação e aprovação. Não use o nome parecido ou conceitos do fluxo live como prova deste harness.

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Skill](../../../../.claude/skills/own-e2e-signup-flow/SKILL.md#s01).
