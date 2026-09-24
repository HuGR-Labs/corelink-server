---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-billing
manifest: crates/corelink-billing/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: H
state: draft
evidence_set: billing-pilot-source-20260919
---

# corelink-billing — blast radius

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) · [Propagação](#b04) ·
[Mudanças](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Leitura rápida e escopo

O risco principal é confundir a fachada com seus implementadores. Mudança em um caminho canônico
pode quebrar o servidor e consumidores que adotaram o alias; mudança nos módulos internos pode
alterar decisões de quota, abuso, replay, schema embutido ou audit. Stripe, D1, R2, Worker e
operações financeiras ficam fora da prova local. Há resultados de testes históricos da baseline, mas nenhum runtime
remoto foi observado.

**Builds avaliados:** library e nove targets de teste em `x86_64-apple-darwin`, sem features.
**Ambientes não observados:** release, wasm, Worker/DO, D1, R2, Stripe e servidor implantado.
Não confundir dependência declarada, resolução, chamada traçada e runtime observado.

<a id="b02"></a>
## B02 — Inventário e método

| População | Fontes / comando | Seleção / revisão | Limite do levantamento |
|---|---|---|---|
| Declarações Cargo | manifesto e `cargo metadata --no-deps` | baseline, package billing | declaração não demonstra uso |
| Resolução e inversas | `cargo tree --locked --offline --workspace --invert corelink-billing --target x86_64-apple-darwin --edges normal,build` | target nativo normal/build | não demonstra artefato entregue |
| Código e dados | `src`, migrations citadas e consumer servidor | fonte baseline | não revalida serviços externos |
| Testes e operação | `cargo test -p corelink-billing`; docs/scripts encontrados | execução local | suites live e ignoradas não executadas |

<a id="b03"></a>
## B03 — Registro de relações diretas

| ID | Tipo / direção | Superfície | Ativação | Owner do contrato |
|---|---|---|---|---|
| [REL-001](#rel-001) | reverse consumer server→billing | dependency Cargo | build servidor | servidor/billing |
| [REL-002](#rel-002) | reexport billing→aggregator | `aggregator::*` | import | crate aggregator |
| [REL-003](#rel-003) | reexport billing→emit | `emit::*` | import | crate emit |
| [REL-004](#rel-004) | reexport billing→reconcile | `reconcile::*` | import | crate reconcile |
| [REL-005](#rel-005) | reexport billing→Stripe schema | `stripe::schema::*` | import | crate Stripe |
| [REL-006](#rel-006) | reexport billing→materializer | `stripe_materializer::*` | import | materializer |
| [REL-007](#rel-007) | reexport billing→tier | `tier::*` | import | tier-selection |
| [REL-008](#rel-008) | reexport billing→headers | `rate_headers::*` | import | rate-headers |
| [REL-009](#rel-009) | reexport billing→ratelimit | `ratelimit::*` | import | ratelimit |
| [REL-010](#rel-010) | internal billing→eviction | `Tier`, região | abuse/quota | eviction |
| [REL-011](#rel-011) | runtime call billing→ratelimit | `update_plan` | Suspicious | ratelimit |
| [REL-012](#rel-012) | schema billing→D1 | migration 0009 | consumer aplica | operador D1 |
| [REL-013](#rel-013) | storage replay→archive | `ReplayArchive` | replay autorizado | adapters/operador |
| [REL-014](#rel-014) | external Stripe client | client real | binding autorizado | Stripe adapter |
| [REL-015](#rel-015) | telemetry/audit | audit sink | decisão/transição | observabilidade |
| [REL-016](#rel-016) | external Stripe real | `stripe::real::*` | import do client | crate Stripe |
| [REL-017](#rel-017) | external Stripe traits | `stripe::traits::*` | import do trait | crate Stripe |
| [REL-018](#rel-018) | schema billing→D1 | migration 0012 | consumer aplica | operador D1 |
| [REL-019](#rel-019) | schema billing→D1 | migration 0013 | consumer aplica | operador D1 |
| [REL-020](#rel-020) | replay→archive | `ReplayArchive` | replay autorizado | adapters/operador |
| [REL-021](#rel-021) | consumer→Stripe adapter | client real | binding autorizado | Stripe adapter |
| [REL-022](#rel-022) | abuse→métrica | abuse metrics | decisão | observabilidade |
| [REL-023](#rel-023) | billing→eviction | `EvictionRegion` | checker/config | eviction |
| [REL-024](#rel-024) | quota→audit sink | quota audit trait | reserva/CAS/FSM | observabilidade |
| [REL-025](#rel-025) | replay→audit sink | replay audit trait | replay autorizado | observabilidade |
| [REL-026](#rel-026) | quota→métrica | quota metrics | decisão/transição | observabilidade |
| [REL-027](#rel-027) | replay→métrica | replay metrics | replay autorizado | observabilidade |

<a id="rel-001"></a>
### REL-001 — Consumidor resolvido: servidor
**Identidade:** `repo:1232040291:boundary:billing-server-cargo-001`.
**Dependência / fluxo / impacto:** server→billing; API→container; billing→build servidor.
**Superfície:** inversa Cargo normal/build para `corelink-server`.
**Ativação:** target nativo resolvido.
**Contrato:** package público deve compilar com consumer selecionado.
**Estado / efeitos:** build, sem prova de rota ou runtime.
**Falha / propagação:** API removida pode quebrar server.
**Contenção:** inversa não diz qual símbolo é chamado.
**Validação:** compilar/testar consumer selecionado; pendente.
**Coordenação / fontes:** owner server; `cargo tree` da B02. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Reexport de agregação
**Identidade:** `repo:1232040291:boundary:billing-aggregator-export-001`.
**Dependência / fluxo / impacto:** billing→aggregator; símbolos→alias; mudança origem→callers.
**Superfície:** `corelink_billing::aggregator::*`.
**Ativação:** import do caminho canônico.
**Contrato:** identidade de tipo vem de `corelink-billing-aggregator`.
**Estado / efeitos:** fachada não cria estado.
**Falha / propagação:** API de origem muda ambos os caminhos.
**Contenção:** não transfere implementação à billing.
**Validação:** smoke path e testes da origem; origem não revisada aqui.
**Coordenação / fontes:** owner aggregator; `src/aggregator.rs`. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Reexport de emissão
**Identidade:** `repo:1232040291:boundary:billing-emit-export-001`.
**Dependência / fluxo / impacto:** billing→emit; API→alias; origem→callers.
**Superfície:** `corelink_billing::emit::*`.
**Ativação:** import.
**Contrato:** tipos e efeitos pertencem a `corelink-billing-emit`.
**Estado / efeitos:** sem duplicação local.
**Falha / propagação:** mudança de API quebra caminho canônico.
**Contenção:** não prova emissão ou audit remoto.
**Validação:** path smoke; testes da origem pendentes.
**Coordenação / fontes:** owner emit; `src/emit.rs`. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Reexport de reconciliação
**Identidade:** `repo:1232040291:boundary:billing-reconcile-export-001`.
**Dependência / fluxo / impacto:** billing→reconcile; alias→callers; origem→contrato.
**Superfície:** `corelink_billing::reconcile::*`.
**Ativação:** import.
**Contrato:** implementação permanece em `corelink-billing-reconcile`.
**Estado / efeitos:** nenhum no wrapper.
**Falha / propagação:** breaking change propaga aos dois imports.
**Contenção:** não executa reconciliação Stripe/D1.
**Validação:** compile path e suite origem; pendente.
**Coordenação / fontes:** owner reconcile; `src/reconcile.rs`. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Fachada Stripe schema
**Identidade:** `repo:1232040291:boundary:billing-stripe-schema-001`.
**Dependência / fluxo / impacto:** billing→Stripe schema; import→tipos; origem→callers.
**Superfície:** `stripe::schema::*`.
**Ativação:** import do submódulo schema.
**Contrato:** tipos de schema permanecem os da origem.
**Estado / efeitos:** billing não possui credencial nem HTTP.
**Falha / propagação:** mudança de tipo quebra o consumer do alias.
**Contenção:** não prova chamada Stripe ou wallet broker.
**Validação:** smoke path; `SOURCE`, suite não executada.
**Coordenação / fontes:** owner Stripe; `src/stripe.rs`. [Relation index](#b03)

<a id="rel-016"></a>
### REL-016 — Fachada Stripe real
**Identidade:** `repo:1232040291:boundary:billing-stripe-real-001`.
**Dependência / fluxo / impacto:** billing→Stripe real; import→client; origem→consumer.
**Superfície:** `stripe::real::*`.
**Ativação:** consumer importa o client real.
**Contrato:** `StripeAuthMode` e tipos pertencem à origem.
**Estado / efeitos:** billing não possui transporte ou credencial.
**Falha / propagação:** API incompatível quebra import/build do consumer.
**Contenção:** sem operação Stripe observada.
**Validação:** `SOURCE`; suite/wiring não executados.
**Coordenação / fontes:** owner Stripe; `src/stripe.rs`. [Relation index](#b03)

<a id="rel-017"></a>
### REL-017 — Fachada Stripe traits
**Identidade:** `repo:1232040291:boundary:billing-stripe-traits-001`.
**Dependência / fluxo / impacto:** billing→Stripe traits; import→implementador.
**Superfície:** `stripe::traits::*`.
**Ativação:** implementador importa o trait.
**Contrato:** trait exposto não implementa transporte.
**Estado / efeitos:** sem estado local na fachada.
**Falha / propagação:** mudança de assinatura quebra implementadores.
**Contenção:** não prova chamada remota.
**Validação:** `SOURCE`; testes de origem não executados.
**Coordenação / fontes:** owner Stripe; `src/stripe.rs`. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Reexport de materializer
**Identidade:** `repo:1232040291:boundary:billing-materializer-export-001`.
**Dependência / fluxo / impacto:** billing→materializer; API→alias; origem→D1 binding.
**Superfície:** `stripe_materializer::*`.
**Ativação:** import e consumer binding.
**Contrato:** writer D1 pertence ao materializer.
**Estado / efeitos:** billing não escreve D1 por este módulo.
**Falha / propagação:** path ou trait quebrado afeta webhook consumer.
**Contenção:** sem prova de route ou D1.
**Validação:** consumer/materializer em revisão própria.
**Coordenação / fontes:** owner materializer; `src/stripe_materializer.rs`. [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Reexport de seleção de tier
**Identidade:** `repo:1232040291:boundary:billing-tier-export-001`.
**Dependência / fluxo / impacto:** billing→tier; import→orquestrador; origem→callers.
**Superfície:** `tier::*`.
**Ativação:** import.
**Contrato:** lógica de tier e checkout é da origem.
**Estado / efeitos:** wrapper puro.
**Falha / propagação:** mudança de tipo quebra consumidores.
**Contenção:** não prova checkout Stripe.
**Validação:** origin suite pendente.
**Coordenação / fontes:** owner tier; `src/tier.rs`. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Reexport de headers
**Identidade:** `repo:1232040291:boundary:billing-rate-headers-export-001`.
**Dependência / fluxo / impacto:** billing→rate-headers; API→HTTP consumer; origem→wire.
**Superfície:** `rate_headers::*`.
**Ativação:** import e resposta HTTP do consumer.
**Contrato:** cabeçalhos pertencem à crate de origem.
**Estado / efeitos:** sem estado na fachada.
**Falha / propagação:** wire HTTP pode divergir.
**Contenção:** não houve trace de rota.
**Validação:** testes da origem e handler pendentes.
**Coordenação / fontes:** owner rate-headers; `src/rate_headers.rs`. [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — Reexport de rate limit
**Identidade:** `repo:1232040291:boundary:billing-ratelimit-export-001`.
**Dependência / fluxo / impacto:** billing→ratelimit; alias→limiter; origem→callers.
**Superfície:** `ratelimit::*`.
**Ativação:** import.
**Contrato:** bucket e circuit breaker são de ratelimit.
**Estado / efeitos:** wrapper não possui bucket.
**Falha / propagação:** API muda plano/limiter dos consumers.
**Contenção:** não prova tráfego ou configuração ativa.
**Validação:** suite origem pendente.
**Coordenação / fontes:** owner ratelimit; `src/ratelimit.rs`. [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Tier para decisão
**Identidade:** `repo:1232040291:boundary:billing-eviction-tier-001`.
**Dependência / fluxo / impacto:** billing→eviction; tier→decisão; provider→billing.
**Superfície:** `Tier` em abuse.
**Ativação:** construção de config interna.
**Contrato:** enumeração de tier deve permanecer compatível.
**Estado / efeitos:** decisão local/fake.
**Falha / propagação:** tier novo pode alterar limites.
**Contenção:** eviction real está fora do package.
**Validação:** `SOURCE`; propriedade não executada.
**Coordenação / fontes:** owners billing/eviction; `src/abuse/score.rs`. [Relation index](#b03)

<a id="rel-023"></a>
### REL-023 — Região de eviction
**Identidade:** `repo:1232040291:boundary:billing-eviction-region-001`.
**Dependência / fluxo / impacto:** billing→eviction; região→checker; provider→billing.
**Superfície:** `EvictionRegion` em quota/CAS.
**Ativação:** construção de checker com região.
**Contrato:** enumeração de região deve permanecer compatível.
**Estado / efeitos:** decisão local/fake; eviction real fora do package.
**Falha / propagação:** região nova pode alterar limites ou seleção.
**Contenção:** não prova operação de eviction.
**Validação:** `SOURCE`; propriedade não executada.
**Coordenação / fontes:** owners billing/eviction; `quota/cas/cas.rs`. [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Downgrade de abuse para limiter
**Identidade:** `repo:1232040291:boundary:billing-abuse-ratelimit-001`.
**Fluxo:** abuse→ratelimit; decisão Suspicious→`update_plan`.
**Superfície / ativação:** `InMemoryAbuseScorer`; decisão Suspicious no fake.
**Contrato:** audits iniciais precedem o update; `DowngradeApplied` vem depois.
**Estado:** limiter fake/consumer.
**Falha:** audit inicial falho aborta o update. Falha em `DowngradeApplied` retorna erro após o update; o fake não faz rollback.
**Contenção:** wiring de produção não verificado.
**Validação:** testes históricos sem pin; não executados no pin atual.
**Coordenação / fontes:** billing/ratelimit; `src/abuse/scorer.rs`. [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Migration D1 0009
**Identidade:** `repo:1232040291:boundary:billing-d1-migration-0009-001`.
**Dependência / fluxo / impacto:** billing→SQL; 0009→aplicador D1→dados.
**Superfície:** migration 0009 embutida em `quota/core.rs`.
**Ativação:** caller fornece DDL ao aplicador autorizado.
**Contrato:** testes pinam versão, constraints e ausência de tokens destrutivos.
**Estado / efeitos:** package contém bytes; não os aplica.
**Falha / propagação:** schema incompatível pode bloquear dados/roll-forward.
**Contenção:** sem D1 real observado.
**Validação:** `SOURCE`; migration suite não executada.
**Coordenação / fontes:** operador D1; `quota/core.rs`. [Relation index](#b03)

<a id="rel-018"></a>
### REL-018 — Migration D1 0012
**Identidade:** `repo:1232040291:boundary:billing-d1-migration-0012-001`.
**Dependência / fluxo / impacto:** billing→SQL; 0012→aplicador D1→dados.
**Superfície:** migration 0012 embutida em `quota/cas.rs`.
**Ativação:** caller autorizado fornece DDL ao aplicador.
**Contrato:** versão e constraints devem permanecer compatíveis.
**Estado / efeitos:** package contém bytes; não aplica D1.
**Falha / propagação:** incompatibilidade bloqueia schema/roll-forward.
**Contenção:** sem D1 real observado.
**Validação:** `SOURCE`; migration suite não executada.
**Coordenação / fontes:** operador D1; `quota/cas.rs`. [Relation index](#b03)

<a id="rel-019"></a>
### REL-019 — Migration D1 0013
**Identidade:** `repo:1232040291:boundary:billing-d1-migration-0013-001`.
**Dependência / fluxo / impacto:** billing→SQL; 0013→aplicador D1→dados.
**Superfície:** migration 0013 embutida em `abuse.rs`.
**Ativação:** caller autorizado fornece DDL ao aplicador.
**Contrato:** versão e constraints devem permanecer compatíveis.
**Estado / efeitos:** package contém bytes; não aplica D1.
**Falha / propagação:** incompatibilidade bloqueia schema/roll-forward.
**Contenção:** sem D1 real observado.
**Validação:** `SOURCE`; migration suite não executada.
**Coordenação / fontes:** operador D1; `abuse.rs`. [Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — Replay para archive
**Identidade:** `repo:1232040291:boundary:billing-replay-archive-001`.
**Dependência / fluxo / impacto:** replay→archive; backend→replay.
**Superfície:** `ReplayArchive`.
**Ativação:** request de replay autorizado.
**Contrato:** archive fornece o evento/resultado pedido ao engine.
**Estado / efeitos:** fake em memória; produção R2 não observada.
**Falha / propagação:** archive falho aborta o braço correspondente.
**Contenção:** sem endpoint ou backend real observado.
**Validação:** `SOURCE`; propriedade replay não executada.
**Coordenação / fontes:** billing, adapters, operador; `src/replay/archive.rs`. [Relation index](#b03)

<a id="rel-020"></a>
### REL-020 — Replay para ledger
**Identidade:** `repo:1232040291:boundary:billing-replay-ledger-001`.
**Dependência / fluxo / impacto:** replay→ledger; request→outcome; ledger→idempotência.
**Superfície:** `ReplayIdempotencyLedger`.
**Ativação:** replay autorizado alcança gravação não-dry-run.
**Contrato:** payload divergente no mesmo request id é erro.
**Estado / efeitos:** ledger fake; D1 de produção não observado.
**Falha / propagação:** ledger falho retorna erro sem declarar sucesso.
**Contenção:** sem endpoint/backend real observado.
**Validação:** `SOURCE`; propriedade replay não executada.
**Coordenação / fontes:** billing, adapters, operador; `src/replay/idempotency.rs`. [Relation index](#b03)

<a id="rel-014"></a>
### REL-014 — Cliente Stripe real
**Identidade:** `repo:1232040291:boundary:billing-stripe-adapter-001`.
**Dependência / fluxo / impacto:** consumer→Stripe adapter; adapter→Stripe.
**Superfície:** client real selecionado pelo consumer.
**Ativação:** binding autorizado escolhe o client.
**Contrato:** API remota é do adapter; billing preserva import.
**Estado / efeitos:** credencial, dinheiro e idempotência pertencem ao adapter.
**Falha / propagação:** indisponibilidade ou wire incompatível alcança o consumer.
**Contenção:** não executar nem certificar operação Stripe.
**Validação:** testes do adapter e ambiente autorizado; pendente.
**Coordenação / fontes:** owner Stripe/operator; `src/stripe.rs`. [Relation index](#b03)

<a id="rel-021"></a>
### REL-021 — Webhook/checkout Stripe
**Identidade:** `repo:1232040291:boundary:billing-stripe-webhook-001`.
**Dependência / fluxo / impacto:** webhook/checkout consumer→adapter; adapter→dados externos.
**Superfície:** caminho de integração documentado pelo consumer, não o alias billing.
**Ativação:** rota externa e binding autorizado.
**Contrato:** wire, idempotência e credencial pertencem ao adapter.
**Estado / efeitos:** dinheiro e dados remotos ficam fora do package.
**Falha / propagação:** erro externo alcança a rota do consumer.
**Contenção:** nenhum endpoint ou runtime Stripe observado.
**Validação:** `SOURCE`; teste do adapter pendente.
**Coordenação / fontes:** owner Stripe/operator; consumer e adapter. [Relation index](#b03)

<a id="rel-015"></a>
### REL-015 — Auditoria de abuso
**Identidade:** `repo:1232040291:boundary:billing-abuse-audit-001`.
**Dependência / fluxo / impacto:** abuse→audit sink; record→resultado.
**Superfície:** audit trait no scorer abuse.
**Ativação:** decisão abuse que exige efeito protegido.
**Contrato:** braços mutáveis auditam antes de escrever no fake.
**Estado / efeitos:** sinks in-memory acumulam registros locais.
**Falha / propagação:** erro de audit aborta ações protegidas.
**Contenção:** não prova exportação ou retenção.
**Validação:** `SOURCE`; negativos não executados.
**Coordenação / fontes:** owner consumer/observabilidade; `abuse/audit.rs`. [Relation index](#b03)

<a id="rel-024"></a>
### REL-024 — Auditoria de quota
**Identidade:** `repo:1232040291:boundary:billing-quota-audit-001`.
**Dependência / fluxo / impacto:** quota→audit sink; transition→record.
**Superfície:** audit traits de quota core/CAS/FSM.
**Ativação:** reserva, CAS ou transição FSM mutável.
**Contrato:** audit precede a escrita protegida no fake.
**Estado / efeitos:** sink in-memory recebe registro local.
**Falha / propagação:** audit falho aborta a mutação correspondente.
**Contenção:** não prova exportação ou retenção.
**Validação:** `SOURCE`; negativos não executados.
**Coordenação / fontes:** owner observabilidade; `quota/*.rs`. [Relation index](#b03)

<a id="rel-025"></a>
### REL-025 — Auditoria de replay
**Identidade:** `repo:1232040291:boundary:billing-replay-audit-001`.
**Dependência / fluxo / impacto:** replay→audit sink; arm→record.
**Superfície:** `ReplayAuditSink`.
**Ativação:** cada arm de replay autorizado/negado.
**Contrato:** audit precede ledger no arm executado.
**Estado / efeitos:** sink fake; cadeia remota não observada.
**Falha / propagação:** audit falho aborta o arm protegido.
**Contenção:** não prova exportação, retenção ou R2.
**Validação:** `SOURCE`; negativos não executados.
**Coordenação / fontes:** owner observabilidade; `replay/audit.rs`. [Relation index](#b03)

<a id="rel-022"></a>
### REL-022 — Métrica de abuso
**Identidade:** `repo:1232040291:boundary:billing-abuse-metrics-001`.
**Dependência / fluxo / impacto:** abuse→métrica sink; decision→observabilidade.
**Superfície:** métricas de abuse.
**Ativação:** decisão abuse emite registro local.
**Contrato:** métrica não autoriza nem substitui audit.
**Estado / efeitos:** sink in-memory acumula registros locais.
**Falha / propagação:** falha afeta diagnóstico; efeito protegido não é inferido.
**Contenção:** não prova cardinalidade, exportação ou alertas.
**Validação:** `SOURCE`; métrica não executada.
**Coordenação / fontes:** owner observabilidade; `abuse/metrics.rs`. [Relation index](#b03)

<a id="rel-026"></a>
### REL-026 — Métrica de quota
**Identidade:** `repo:1232040291:boundary:billing-quota-metrics-001`.
**Dependência / fluxo / impacto:** quota→métrica sink; transition→observabilidade.
**Superfície:** métricas de quota core/CAS/FSM.
**Ativação:** decisão ou transição local emite registro.
**Contrato:** métrica informa observabilidade; não substitui audit.
**Estado / efeitos:** sink in-memory acumula registros.
**Falha / propagação:** falha afeta diagnóstico, não autoriza mutação.
**Contenção:** não prova cardinalidade ou exportação.
**Validação:** `SOURCE`; métrica não executada.
**Coordenação / fontes:** owner observabilidade; `quota/*.rs`. [Relation index](#b03)

<a id="rel-027"></a>
### REL-027 — Métrica de replay
**Identidade:** `repo:1232040291:boundary:billing-replay-metrics-001`.
**Dependência / fluxo / impacto:** replay→métrica sink; arm→observabilidade.
**Superfície:** métricas de replay.
**Ativação:** arm de replay emite registro.
**Contrato:** métrica não autoriza nem substitui audit.
**Estado / efeitos:** sink in-memory; exportação não observada.
**Falha / propagação:** falha afeta diagnóstico do arm.
**Contenção:** não prova cardinalidade, retenção ou alertas.
**Validação:** `SOURCE`; métrica não executada.
**Coordenação / fontes:** owner observabilidade; `replay/*.rs`. [Relation index](#b03)

<a id="b04"></a>
## B04 — Propagação transitiva

| Destino | Caminho por RELs | Condição | Efeito causal | Contenção / validação |
|---|---|---|---|---|
| servidor | REL-001 + aliases | consumer importa billing | quebra de API no build | compilar consumer; rota não traçada |
| Stripe schema | REL-005 | import do schema | tipo incompatível pode quebrar build | origem revisada; runtime não observado |
| Stripe client/rota | REL-014/016/017/021 | binding externo | client, webhook ou checkout pode falhar | adapter e ambiente autorizado |
| D1 schema | REL-012/018/019 | migration aplicada | schema incompatível | backup e procedure de dados |
| replay archive | REL-013 | request autorizado | archive ausente/falho aborta replay | R2 não observado |
| replay ledger | REL-020/025 | request não-dry-run | idempotência/audit pode falhar | D1/RBAC não observados |
| quota tenant | REL-010/011/023/024/026 | decisão local ligada | deny/downgrade/audit | fake descrito, wiring não |

**Cobertura:** as onze dependências diretas de primeira parte e o consumidor Cargo resolvido têm
relação registrada; as relações internas materiais foram separadas por superfície e efeito.
**Caminhos alternativos materiais:** import original e canônico convivem; cada um pode falhar sem
alterar o outro no source, mas compartilham o mesmo tipo de origem.
**Não alcance comprovado:** nenhum target de produção foi provado ausente.

<a id="b05"></a>
## B05 — Mudança → impacto → validação

| Mudança | API / INV / REL afetados | Consumidores / estado | Validação necessária | Coordenação / recuperação |
|---|---|---|---|---|
| remover caminho reexportado | API-001; REL-002–009 | imports server/origens | compilar ambos os caminhos | owner origem; reintroduzir alias compatível |
| mudar score/limiar abuse | API-011/012; INV-001–003; REL-011/015/022 | decisões e limiter | propriedades e negativos | owner ratelimit; não auto-suspender |
| mudar quota core/CAS | API-013/014; INV-004–007; REL-023/024/026 | reserva, denial, Retry-After | boundary/race/calendar tests | owner D1/Worker; rollout compatível |
| mudar FSM | API-015; INV-008/009 | estado/audit | ladder e failure-path tests | owner webhook/ops; reconciliar estado |
| mudar replay | API-016; INV-010/011; REL-013/020/025/027 | audit/ledger/archive | role, dry-run, collision tests | owner RBAC/R2/D1; preservar ledger |
| mudar migration | REL-012/018/019 | schema e dados existentes | migration lint + D1 autorizado | backup/roll-forward, não apenas revert |

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

| População | Descobertos | Documentados | Excluídos com motivo | Desconhecidos |
|---|---:|---:|---:|---:|
| consumer Cargo normal/build | 1 | 1 | 0 | targets entregues |
| deps first-party diretas | 11 | 11 | 0 | features transitivas |
| módulos internos principais | 6 áreas | 6 | 0 | relações submódulo a submódulo |
| módulos/arquivos Rust de implementação | 47 arquivos não-test em `src/` | 47 | 0 | proxy de módulo semântico; confirmar decomposição na revisão |
| fronteiras remotas | 4 | 4 | 0 | runtime, credenciais e deploy |
| suites locais | 10 targets | 10 | 0 | release/wasm/live/ignored |

**Exclusões enumeradas:** não foram excluídos consumers por suposição; apenas nenhuma relação
externa foi promovida a runtime observado.
**Diferenças para o censo independente:** a inversa normal/build traz somente o servidor; buscas
semânticas encontraram referências em docs, scripts e packages de origem, que exigem revisão própria.
**O que não foi observado:** produção, dados de cliente, billing Stripe, D1, R2, Worker e mounts.
A igualdade de contagens não prova a completude da descoberta.

**Reanchor:** entre `cca798ff` e `fb611330`, o teste `quota_cas_prop_quota_cas.rs`
alterou o limite do probe de p99 estritamente menor que 5 ms para menor ou igual
a 5 ms. O teste não foi executado; qualquer claim de latência deve usar o novo
predicado e o pin atual.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01).
