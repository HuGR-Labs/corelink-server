---
schema: corelink-ownership/1.1
document: reference
package: corelink-audit
manifest: crates/corelink-audit/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: S
state: draft
evidence_set: audit-static-graph-20260920
---

# corelink-audit — referência de ownership

Referência de fonte estática na baseline fixada. Descreve contratos declarados; não
certifica produção, persistência, entrega externa ou a cobertura total de consumers.

[Identidade](#r01) · [Fronteiras](#r02) · [Mapa](#r03) · [Envelope](#r04) ·
[Privacidade](#r05) · [Portas](#r06) · [Verificação](#r07) · [Lacunas](#r08).

<a id="r01"></a>
## R01 — Identidade

| Campo | Fonte estática |
|---|---|
| Package | `corelink-audit`; `crates/corelink-audit/Cargo.toml` |
| Papel | Taxonomia `auth.*`, envelope CloudEvents 1.0 e primitivas de auditoria |
| Dependências CoreLink | `corelink-audit-chain` e `corelink-analytics` |
| Limite | Dependência/reexport não transfere implementation ownership |

<a id="r02"></a>
## R02 — Fronteiras

| Área | Nesta crate | Fora desta crate |
|---|---|---|
| Eventos, JCS e redaction | Tipos, funções e macros | Política de produtor/consumer |
| Emissão | `Emitter`, memória e abstrações outbox | Wiring/outbox produtivo e Direct-SIEM |
| Chain e analytics | Caminhos de reexport | Implementação em crates dependentes |

<a id="r03"></a>
## R03 — Mapa da implementação

| Arquivo | Responsabilidade observada |
|---|---|
| `events.rs` | `AuthEvent`, `AuthEventType`, dados e constantes CloudEvents |
| `link_hash.rs` | Hash JCS de conteúdo e ligação de chain |
| `redact.rs`, `retention.rs` | Newtypes/macros PII e hints por tier |
| `emitter.rs`, `outbox.rs`, `ports.rs`, `metrics.rs` | Traits, sinks em memória, outbox abstraído e métricas |
| `analytics.rs`, `chain.rs` | Reexports completos; marcador de caminho |

<a id="r04"></a>
## R04 — Envelope e taxonomia

`AuthEvent` declara `specversion`, id UUIDv7, `source`, `type`, `time_unix_ms`, conteúdo
JSON e extensões CoreLink. `subject()` é método, não campo serializado; a conformidade do
shape serializado com os campos CloudEvents normais `time` e `subject` permanece desconhecida.
`AuthEventType::canonical` e `AuthEventData` associam a taxonomia `auth.*`; testes locais
declaram 33 variantes. `compute_content_hash` serializa por JCS e SHA-256; `link_chain_hash`
liga hashes já calculados. Alterações exigem avaliar vetores canônicos e consumers estáticos.

<a id="r05"></a>
## R05 — Privacidade e retenção

`PrincipalIdHash`, `PatIdHash` e `EmailHash` derivam prefixo hexadecimal SHA-256 e
rejeitam entrada vazia. `redact_pat!` e `redact_pat_str` expõem o placeholder, não o
segredo. `RetentionHint::for_tier` mapeia `TenantTier` para 30, 90, 365 ou 2555 dias.
Isso é contrato de tipos/fonte, não prova que todo produtor evita PII ou que um worker
aplica a retenção.

<a id="r06"></a>
## R06 — Portas e efeitos

`Emitter` e `InMemoryEmitter` formam a superfície simples de emissão em memória.
`AuditOutboxWriter`/`OutboxEmitter` são abstrações locais. `MetricsObserver` possui
sinks noop/em memória. Essas presenças não demonstram commit durável, batch atômico,
webhook ou SIEM: confirmar o adapter e entrypoint real em cada integração.

<a id="r07"></a>
## R07 — Evidência estática disponível

| Evidência | Escopo |
|---|---|
| Manifesto | Dependências, examples e três integration tests declarados |
| Fonte | Módulos, exports, traits, funções, macros e comentários |
| Examples/tests | Token validado, redaction, chain/anomaly; properties, vetores e redaction |
| Busca reversa | Imports/referências em consumers conhecidos; não é execução |

<a id="r08"></a>
## R08 — Lacunas e encaminhamento

Desconhecidos explícitos: owner nominal, policy de compatibilidade de eventos, lista
completa de consumers, destino/outbox produtivo, SIEM, retenção real, replay e runtime.
Um import ou docstring não prova sink durável. Para cada lacuna, abrir decisão no fluxo
do repositório com trecho fonte, SHA, consumer e adapter/entrypoint a investigar.

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#r01)
