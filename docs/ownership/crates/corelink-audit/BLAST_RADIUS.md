---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-audit
manifest: crates/corelink-audit/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: S
state: draft
evidence_set: audit-static-graph-20260920
---

# corelink-audit — blast radius

Relações atômicas derivadas de manifesto, exports e busca estática. Setas indicam
dependência ou fluxo declarado; não estabelecem execução, durabilidade ou cobertura completa.

[Base](#b01) · [Reexports](#b02) · [Hash](#b03) · [Dados](#b04) ·
[Portas](#b05) · [Consumers](#b06).

<a id="b01"></a>
## B01 — Taxonomia para envelope

`AuthEventType` + `AuthEventData` → `AuthEvent`: uma alteração de string/variante/payload
impacta serialização e todo leitor que presume a associação. Evidência: `events.rs` e
`tests/canonical_vectors.rs`; desconhecido: leitores externos completos.

<a id="b02"></a>
## B02 — Reexports para dependências

`corelink-audit` → `corelink-audit-chain` por `chain::*`; `corelink-audit` →
`corelink-analytics` por `analytics::*`. Mudar caminho/export impacta callers desses caminhos;
a reexportação não move a implementação nem o ownership das duas crates.

<a id="b03"></a>
## B03 — Canonicalização para cadeia

`AuthEvent` → JCS → `ContentHash` → `link_chain_hash`: alterar representação ou algoritmo
impacta vetores e links posteriores. A fonte declara que o link usa o hash de conteúdo; não
prova quais bytes são persistidos nem um processador produtivo.

<a id="b04"></a>
## B04 — PII e tier para payload

Raw identifier → hash newtype; PAT formatado → placeholder; `TenantTier` → `RetentionHint`
→ evento. Alterações afetam correlação, conteúdo serializado e a expectativa de retenção.
Não está demonstrado que cada producer usa esses caminhos ou que algum worker expira linhas.

<a id="b05"></a>
## B05 — Evento para abstração de saída

`AuthEvent` → `Emitter`/`InMemoryEmitter`; `AuditEvent` → `AuditEmitter`; outbox row →
`AuditOutboxWriter`/`OutboxEmitter`. Alterar trait ou erro rompe adapters estáticos. Nenhuma
relação, isoladamente, prova commit, retry, webhook, SIEM ou atomicidade transacional.

<a id="b06"></a>
## B06 — Consumers conhecidos e cobertura

Declarações diretas em manifestos: `corelink-container`, `corelink-cas`, `corelink-ac`,
`corelink-worker`, `corelink-adapter-host`, `corelink-auth` (dev-dependency), `corelink-ops`
e `tests/e2e-tenant-isolation`. `corelink-clerk-cf`, billing materializers e `tools/cli`
declaram `corelink-audit-chain` ou `corelink-analytics`, não `corelink-audit`; não inferir
uso semântico de seus reexports. `corelink-cf-bindings` não declara nenhuma das três nesta
varredura. Cobertura de imports condicionais, geração, features, deploy e consumers externos
permanece desconhecida.

A borda de harness `tests/e2e-tenant-isolation` →
`corelink-audit::{AuthEvent,Emitter,InMemoryEmitter}` compartilha a identidade
`repo:1232040291:boundary:e2e-tenant-isolation-audit-001` com
`e2e-tenant-isolation` BLAST `REL-002`. A fake encaminha a negação para
evento/emitter e mantém a captura local; `corelink-audit` mantém a propriedade da
API de evento/emitter, enquanto o harness mantém a fake e suas asserções. É uma
relação estática de fonte: sem sink durável,
execução observada ou alcance em runtime.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01)
