---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-handler-cas-erase
manifest: crates/corelink-handler-cas-erase/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-handler-cas-erase-structural-normalization-20260921
---

# corelink-handler-cas-erase — blast radius

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) · [Propagação](#b04) · [Mudança](#b05) · [Desconhecidos](#b06).

<a id="b01"></a>
## B01 — Escopo

O raio cobre a API pura de erase/tombstone e consumidores estáticos. A direção de dependência é consumidor → `corelink-handler-cas-erase`; a direção de impacto é contrato desta crate → consumidor. Delete R2 e persistência/consulta D1 são relações de composição externa, não relações de implementação desta crate. Nenhuma seta prova reachability, operação ou runtime.

<a id="b02"></a>
## B02 — Método e populações

| Camada | Evidência SOURCE | Limite |
|---|---|---|
| Package | `Cargo.toml`, `src/lib.rs` | export não é uso executado |
| Consumidor | `corelink-container/Cargo.toml` e imports de `routes/cas_erase` | import não prova rota montada |
| Teste local | test target declarado | não executado; não é consumer de produção |
| Dependência externa | `thiserror` / `proptest` no manifesto | não transfere ownership do contrato |

<a id="b03"></a>
## B03 — Relações atômicas

<a id="rel-erase-001"></a>
### REL-ERASE-001 — Reexport canônico local
**Tipo/direção:** export; `corelink-handler-cas-erase::lib` → símbolos definidos em `handler`/`error`.
**Superfície:** `CasEraseError`, request, marker, enums e quatro funções são reexportados na raiz.
**Impacto:** remover/renomear símbolo quebra importadores da raiz.
**Evidência/coordenação:** `crates/corelink-handler-cas-erase/src/lib.rs`; owner deste package.

<a id="rel-erase-002"></a>
### REL-ERASE-002 — Consumidor estático do container
**Tipo/direção:** dependência/import direto; `corelink-container` → `corelink-handler-cas-erase`.
**Superfície:** manifesto do container declara a dependência; `routes/cas_erase.rs` importa `prepare_erase`, `erase_outcome` e `EraseOutcome`, e seus módulos usam request/erro.
**Impacto:** mudança em assinatura, enum ou semântica pura pode exigir ajuste no container.
**Limite/coordenação:** owner container; import não certifica montagem da rota, auth ou tráfego. Evidência: `crates/corelink-container/{Cargo.toml,src/routes/cas_erase.rs,src/routes/cas_erase/b126_m2_impl_01.rs}`.

<a id="rel-erase-003"></a>
### REL-ERASE-003 — Composição externa R2/D1
**Tipo/direção:** relação de composição, não dependência de implementação; `corelink-container::routes::cas_erase` recebe decisão/marker desta crate e é responsável por qualquer delete R2, consulta/upsert D1 e tradução de transporte.
**Superfície:** comentários de `src/{lib,handler,error}.rs` identificam esse boundary; a fonte do container é o local separado a reconciliar.
**Impacto:** alterar gramática, marker, gate ou outcome pode alterar o dado/decisão entregue à composição; não implica que esta crate toque R2/D1.
**Limite/coordenação:** owners container e storage/persistence; nenhum efeito externo é observado. Evidência: `crates/corelink-handler-cas-erase/src/{lib,handler,error}.rs`, `crates/corelink-container/src/routes/cas_erase.rs`.

<a id="rel-erase-004"></a>
### REL-ERASE-004 — Teste de propriedades declarado
**Tipo/direção:** alvo dev local; test target → API pública da crate.
**Superfície:** propriedades de gramática, cross-tenant, gate e outcome em `tests/prop_handler_cas_erase.rs`.
**Impacto:** mudança de contrato requer rever expectativas estáticas do target.
**Limite/coordenação:** não executado e não prova composição R2/D1/HTTP. Evidência: `Cargo.toml`, `tests/prop_handler_cas_erase.rs`.

<a id="b04"></a>
## B04 — Propagação transitiva

Uma mudança pode propagar API pura → import no container → composição de rota. Esse último elo é uma obrigação de reconciliação, não prova de delete, tombstone D1 ou resposta HTTP. Não foi estabelecido censo reverso completo de consumidores nem importadores indiretos; não há caminho transitivo certificado para R2, D1, auth ou runtime.

<a id="b05"></a>
## B05 — Mudança → impacto → validação

| Mudança | Impacto provável | Validação permitida nesta campanha |
|---|---|---|
| gramática/limite do digest | container pode aceitar/rejeitar outro conjunto de chaves | comparar API-001 e import estático |
| ordem de `prepare_erase` | pode mudar qual erro a composição recebe | inspecionar INV-001 e call sites estáticos |
| `ReadGate`/`EraseOutcome` | consumer pode mapear resultado diferente | busca de enum/símbolo, sem HTTP real |
| marker/campos públicos | contrato do dado entregue ao container muda | censo de uso estático e coordenação |
| R2/D1/rota | fora do package | escalar para owner do container; não implementar/alegar aqui |

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

Cobertos: manifesto, módulos públicos, test target declarado e dependência/import do container. Desconhecidos: todos os consumidores inversos, concrete R2 delete, D1 schema/lookup/upsert, transação/ordem/retry, autorização e identidade reais, rota/mount, mapeamento HTTP, observabilidade, dados de tenant, deploy e runtime. A relação R2/D1 foi mantida separada para impedir que esse kernel puro herde ownership ou comportamento não demonstrado.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01).
