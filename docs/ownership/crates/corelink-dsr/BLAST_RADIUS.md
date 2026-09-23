---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-dsr
manifest: crates/corelink-dsr/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-dsr-structural-normalization-20260921
---

# corelink-dsr — blast radius

[Escopo](#b01) · [Método](#b02) · [Diretas](#b03) · [Propagação](#b04) · [Mudança](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Escopo

O raio cobre contrato de tipos/traits/fakes DSR e seus consumidores estáticos. A direção de dependência é consumidor → `corelink-dsr`; a direção de impacto é API DSR → consumidor. Relações adjacentes de CF, clerk e replication são explicitamente distintas de import direto. Nada aqui prova rota, operação, dados, runtime ou cumprimento legal.

<a id="b02"></a>
## B02 — Método e populações

| Camada | Evidência SOURCE | Limite |
|---|---|---|
| Package | `crates/corelink-dsr/Cargo.toml`, `src/lib.rs` | declaração/export não é uso executado |
| Consumidor direto | manifestos e `use corelink_dsr` | não prova caminho alcançável |
| Reexport | `crates/corelink-privacy/src/dsr.rs` | não lista todos os importadores do umbrella |
| Adjacência | fontes/manifestos de CF, clerk e replication | não transforma comentário/cadeia em import direto |
| Arquivo | `_archive/wi-s11-002-partial/Cargo.toml` | excluído da população atual |

<a id="b03"></a>
## B03 — Relações atômicas

### REL-DSR-001 — Reexport de privacy
**Tipo/direção:** reexport; `corelink-privacy` → `corelink-dsr` (dependência), e API DSR → importadores de `corelink_privacy::dsr` (impacto).
**Superfície:** `pub use corelink_dsr::*` em `crates/corelink-privacy/src/dsr.rs`.
**Efeito/falha:** remoção ou mudança de símbolo pode quebrar ambos os caminhos de import.
**Coordenação/evidência:** owner privacy; `crates/corelink-privacy/{Cargo.toml,src/dsr.rs}`.

### REL-DSR-002 — Portal do container
**Tipo/direção:** consumidor direto; `corelink-container` → `corelink-dsr`.
**Superfície:** `DsrRequestKind`, `DsrJurisdiction` e `sla_for` nas rotas `routes/dsr/portal`.
**Efeito/falha:** mudança de enum/helper pode alterar o parsing/resultado do portal ou impedir sua compilação.
**Coordenação/evidência:** owner container; `crates/corelink-container/Cargo.toml`, `src/routes/dsr/portal/part-00.rs` e `part-01-01.rs`.

### REL-DSR-003 — Erasure worker de desenvolvimento
**Tipo/direção:** consumidor direto dev; `corelink-privacy-erasure-worker` → `corelink-dsr`.
**Superfície:** lifecycle test constrói o endpoint/fakes e envia `Erasure`.
**Efeito/falha:** mudança de trait/taxonomia pode quebrar o harness de integração; o próprio manifesto informa que a dependência é dev-only.
**Coordenação/evidência:** owner erasure; `crates/corelink-privacy-erasure-worker/Cargo.toml`, `tests/integration_erasure_lifecycle.rs`.

### REL-DSR-004 — Harnesses E2E
**Tipo/direção:** consumidores diretos de teste; `e2e-dsr` e `e2e-billing-flow` → `corelink-dsr`.
**Superfície:** `InMemoryDsrEndpoint`, fakes, decisões e direitos, inclusive Erasure.
**Efeito/falha:** alteração de API/invariante muda o harness e suas expectativas estáticas.
**Coordenação/evidência:** owners E2E; `tests/e2e-dsr/{Cargo.toml,src/helpers.rs}` e `tests/e2e-billing-flow/{Cargo.toml,src/harness.rs}`.

### REL-DSR-005 — Caminho CF/clerk adjacente
**Tipo/direção:** adjacência de wiring estático, não dependência direta comprovada; `corelink-clerk-cf` → scheduler/erasure worker/CF bindings, em torno do domínio DSR.
**Superfície:** cron de statuspage e dependências wasm condicionais; `corelink-dsr` não aparece como dependência direta no manifesto clerk-cf inspecionado.
**Efeito/falha:** uma mudança de contrato DSR pode exigir reconciliação do caminho, mas essa fonte não demonstra chamada ao endpoint DSR.
**Coordenação/evidência:** owners clerk-cf e scheduler; `crates/corelink-clerk-cf/{Cargo.toml,src/dsr_statuspage_cron.rs}`.

### REL-DSR-006 — CF bindings e replication adjacentes
**Tipo/direção:** adjacência estática, não import direto comprovado; `corelink-cf-bindings` e `corelink-replication` participam de configurações/infraestrutura citadas ao redor de DSR.
**Superfície:** o manifesto CF menciona scheduler/erasure em comentário de cadeia wasm; replication não declara `corelink-dsr` no manifesto inspecionado.
**Efeito/falha:** não é permitido converter esta relação em requisito de compatibilidade sem owner e import concreto.
**Coordenação/evidência:** owners CF/replication; `crates/corelink-cf-bindings/Cargo.toml`, `crates/corelink-replication/Cargo.toml`.

<a id="b04"></a>
## B04 — Propagação transitiva

Mudança em tipo/reexport pode seguir `corelink-dsr` → `corelink-privacy::dsr` → importador do umbrella. Mudança em taxonomia/SLA pode seguir `corelink-dsr` → container ou harnesses. Os caminhos CF/clerk/replication permanecem hipóteses de coordenação estática até uma importação e o wiring concreto serem reconciliados. Nenhuma seta equivale a execução.

<a id="b05"></a>
## B05 — Mudança → impacto → validação

| Mudança | Impacto provável | Validação permitida nesta campanha |
|---|---|---|
| enum, decisão ou `sla_for` | portal/reexport/harness divergentes | busca de símbolo e censo estático |
| `DsrEndpoint`/trait de backend | fakes e consumidor direto deixam de satisfazer contrato | mapear assinatura e implementadores fonte |
| ordem audit/store | quebra do predicado fail-closed do orquestrador | inspeção de ordem em `endpoint.rs` |
| recibo/MFA | expectativa E2E e fronteira de segurança mudam | comparar predicado/invariante, sem key/token real |
| relação CF/clerk | possível gap de wiring | escalar, não inferir runtime |

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

Cobertos: manifesto, superfície fonte, reexport privacy, container, erasure dev-only e dois harnesses E2E. Conhecidos mas não provados como consumidores diretos: clerk-cf, cf-bindings e replication; foram retidos como adjacências para evitar silêncio e impedir sobreafirmação. Desconhecidos: censo inverso completo, importadores do umbrella, compatibilidade de formatos persistidos, rota CF, store Neon, chave/RS256, WebAuthn, Email, dados de tenant, entrega e status legal. O manifesto arquivado foi excluído, não contado como consumidor atual.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01).
