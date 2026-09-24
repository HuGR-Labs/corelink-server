---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-privacy-erasure-worker
manifest: crates/corelink-privacy-erasure-worker/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-privacy-erasure-worker-structural-normalization-20260921
---

# corelink-privacy-erasure-worker — blast radius

[B01](#b01) · [B02](#b02) · [B03](#b03) · [B04](#b04) · [B05](#b05) · [B06](#b06)

O raio cobre consumidores e adjacências encontrados estaticamente. Dependência aponta consumidor → pacote; impacto aponta contrato → consumidor. Nenhuma seta prova execução.

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) · [Mudança](#b04) · [Lacunas](#b05).

<a id="b01"></a>
## B01 — Escopo

Uma alteração de enum, trait, formato de relatório, regra de pseudônimo ou métrica pode afetar consumidores Rust, schema/documentação e bridges encontrados. Provider/backing service é separado: uma dependência em `corelink-container` ou texto sobre R2 não é prova de operação R2.

<a id="b02"></a>
## B02 — Método

Busca estática por nome de package/crate em manifestos e fontes não arquivadas; classificação de import/reexport/bridge/documentação. Comentários, lockfile e specs foram usados apenas para contexto, não como consumer executado. Não houve Cargo, teste, rede ou inspeção de provider.

<a id="b03"></a>
## B03 — Relações atômicas

### REL-ERASURE-001 — Container e adapters
**Tipo/direção:** consumidor direto; `corelink-container` → worker. **Superfície:** rotas DSR, legitimidade, ledger, audit, attestation e adapters CAS/D1/Stripe/not-applicable. **Impacto:** mudanças nas traits, enum, key/idempotência ou request podem quebrar o wiring fonte. **Limite:** estes arquivos não provam Queue, D1/R2/Stripe ou qualquer operação executada.

### REL-ERASURE-002 — Scheduler/statuspage
**Tipo/direção:** consumidores diretos/bridge; `corelink-dsr-statuspage-scheduler`, `corelink-clerk-cf`, `corelink-statuspage-real` → worker. **Superfície:** `VerificationOutcome`, aggregate, `outcome_json`, métricas e publish bridge. **Impacto:** mudanças em decisão/report/snapshot podem romper parsing ou publicação. **Limite:** cron, Statuspage e alertas não foram observados.

### REL-ERASURE-003 — Privacy umbrella e pseudonymize
**Tipo/direção:** reexport/fornecedor; `corelink-privacy` → worker e worker → `corelink-privacy-pseudonymize`. **Superfície:** caminho `erasure` do umbrella e helpers/hash/marker. **Impacto:** alteração de reexport ou formato do pseudônimo propaga a importadores do umbrella. **Limite:** não identifica todos os importadores transitivos nem gerencia salts.

### REL-ERASURE-004 — DSR e E2E
**Tipo/direção:** DSR é dev-dependency do worker; `tests/e2e-dsr` é consumidor direto. **Superfície:** lifecycle, request/decision, fakes e falha parcial/SLA. **Impacto:** alterações no contrato quebram seus harnesses estáticos. **Limite:** teste declarado/encontrado não foi executado.

### REL-ERASURE-005 — SLO, ops e CF bindings
**Tipo/direção:** `corelink-slo` é dev-only do worker; `corelink-ops` e `corelink-cf-bindings` são adjacências estáticas. **Superfície:** nome da métrica e referências de offboarding/wasm. **Impacto:** mudança de SLI ou integração pode requerer coordenação. **Limite:** não há import direto comprovado de worker em cada adjacência citada; não assumir runtime CF.

### REL-ERASURE-006 — Attestation especializada
**Tipo/direção:** adjacência de domínio, não dependência direta; `corelink-erasure-attestation` e container compartilham o domínio de prova de erasure. **Superfície:** report/JCS versus `ErasureAttestation` especializado. **Impacto:** uma mudança de payload pode exigir reconciliação. **Limite:** o worker não importa essa crate; não alegar attestation assinada ou armazenada.

<a id="b04"></a>
## B04 — Mudança → impacto → checagem permitida

| Mudança | Impacto | Checagem estática |
|---|---|---|
| `BackendKind`/outcome/CloudEvent | container, scheduler, E2E, schemas | censo de símbolo e match/reexport |
| adapter/legitimidade/ledger | container/wiring e fakes | assinatura, implementadores e direção |
| report/MAC/key | scheduler/statuspage/attestation adjacente | comparar payload, JCS e paths |
| `VerificationOutcome`/snapshot/métrica | scheduler/statuspage/SLO | buscar campos/constantes |
| pseudônimo/marker | privacy/container/adapters | buscar helper e formato |

<a id="b05"></a>
## B05 — Cobertura e lacunas

Cobertos: manifesto/fonte do worker, container, scheduler, clerk CF, statuspage real, privacy, pseudonymize, SLO, ops, CF bindings e E2E encontrados por busca. Desconhecidos: consumidores fora do workspace, todos os importadores via umbrella, schema em produção, compatibilidade persistida, adapters de cada provider, Queue/cron, operações/retention, alertas, assinatura KMS/Ed25519 e conclusão jurídica. Escalar ao owner do consumer/provider em vez de inferir a lacuna.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01)

<a id="b06"></a>
## B06 — Coverage and unknowns

The static census covered the worker manifest/source and named container, scheduler, Clerk bridge, statuspage, privacy umbrella, pseudonymize, SLO, ops, CF-binding, and E2E paths in B03. It found direct consumer/bridge arrows in REL-ERASURE-001–004 and development-only dependencies or adjacent references in REL-ERASURE-005–006.

This does not prove those paths execute or exhaust consumers outside this checkout. Still unknown: applied schemas/migrations, provider wiring/effects, durable audit/ledger, cron/Queue delivery, report upload/signing keys, alerts, retention, and legal outcome. Route contract changes to the named consumer owner; route provider/runtime questions to the provider or scheduler owner with missing evidence named.
