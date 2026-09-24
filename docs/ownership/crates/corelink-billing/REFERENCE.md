---
schema: corelink-ownership/1.1
document: reference
package: corelink-billing
manifest: crates/corelink-billing/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: H
state: draft
evidence_set: billing-pilot-source-20260919
---

# corelink-billing — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) ·
[Contratos](#r04) · [Estado e invariantes](#r05) · [Configuração](#r06) ·
[Falhas](#r07) · [Verificação](#r08).

<a id="r01"></a>
## R01 — Identidade e função

`corelink-billing` é a superfície canônica de cobrança. Ela combina cinco implementações
movidas para o package (`abuse`, `replay`, e três módulos de quota) com dez crates
reexportadas. Não cria um serviço nem executa I/O remoto por si. O manifesto, não o nome
do diretório, identifica o package; seu único consumidor Cargo direto resolvido é
`corelink-server`. A inclusão em um artefato e o comportamento em produção não foram observados.

| Campo | Valor verificado |
|---|---|
| Package / manifesto | `corelink-billing` / `crates/corelink-billing/Cargo.toml` |
| Targets | library `corelink_billing`; nove integration-test targets; sem bin, feature ou build-dep declarada |
| Papel | Híbrido: import surface, módulo interno e composição de contratos |
| Licença / publish | Herdados do workspace; valores efetivos não resolvidos nesta passagem |
| Implementado | `yes` — módulos próprios e fachadas presentes no source; classe `SOURCE` |
| Wired | `unknown` — consumidor Cargo direto resolvido; classe `RESOLVED`, sem prova de chamada |
| Runtime verified | `unknown` — não há evidência `OBSERVED_RUNTIME` nem observação de implantação anexada; `SOURCE` não prova nem nega alcance em runtime |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Superfície | Dono da implementação | Dono do contrato | Operação / escalonamento |
|---|---|---|---|
| `abuse`, `replay`, `quota::*` | `corelink-billing` | package billing | servidor/adapters para wiring remoto |
| `aggregator`, `emit`, `reconcile` | packages reexportados | crate de origem e billing no caminho canônico | owner de cada origem |
| `stripe::{schema,real,traits}` | Stripe packages reexportados | origem; billing para path | Stripe/adapters; operação autorizada |
| materializer, tier, rate headers, ratelimit | packages reexportados | origem; billing para path | owner da origem e servidor |

**Não faz:** não chama Stripe, D1, R2 ou Cloudflare diretamente no package root; não torna fake
uma prova de binding remoto; não transfere ownership da implementação por `pub use`.
**Conceitos canônicos:** [billing-commerce](../../../knowledge/crates/billing-commerce.md),
[billing-pipeline](../../../knowledge/crates/billing-pipeline.md) e contratos/ADRs citados no código.
**Absorções e aliases:** cinco áreas foram movidas para este package; dez crates reexportadas preservam
o caminho original e acrescentam `corelink_billing::<módulo>`.

<a id="r03"></a>
## R03 — Mapa da implementação

| Módulo / entradas | Papel e dado sob ownership | Natureza | Contratos / evidências |
|---|---|---|---|
| `lib.rs` e módulos fachada | rotas públicas e reexports | composição/reexport | API-001–010; classe `SOURCE` |
| `abuse/{features,score,scorer}` | features, score e decisão por tenant | implementado/fake | API-011; INV-001/002 |
| `abuse/{audit,metrics,config,error}` | auditoria, métricas e configuração | implementado/fake | API-012; INV-003 |
| `quota/core/*` | reservas e decisão provisória | implementado/fake | API-013; INV-004/005 |
| `quota/cas/*` | limite estrito, CAS e calendário | implementado/fake | API-014; INV-006/007 |
| `quota/fsm/*` | estado de quota e transições | implementado/fake | API-015; INV-008/009 |
| `replay/*` | replay, archive, ledger e auditoria | implementado/fake | API-016; INV-010/011 |
| oito arquivos de fachada | exposição de crates externas | reexport | API-001–010; REL-002 a REL-010 |

**Inventário:** 49 arquivos Rust em `src` foram enumerados. A classificação é por módulo e não
substitui o censo de todos os símbolos exportados pelas dez dependências reexportadas.

<a id="r04"></a>
## R04 — Contratos públicos

**Índice:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) ·
[API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [API-007](#api-007) ·
[API-008](#api-008) · [API-009](#api-009) · [API-010](#api-010) · [API-011](#api-011) ·
[API-012](#api-012) · [API-013](#api-013) · [API-014](#api-014) · [API-015](#api-015) ·
[API-016](#api-016).

<a id="r04-export-catalog"></a>
### Catálogo exato de símbolos das fachadas

Listas extraídas dos `pub mod`/`pub use` no `lib.rs` de cada origem, no pin deste documento. Cada item enumera exports do root; módulos descendentes estão explicitados.

| Contrato | Caminho e exports públicos exatos da origem |
|---|---|
| [API-001](#api-001) · `corelink_billing::aggregator` | módulo `aggregator`; `AggregationRequest`, `AggregatedCounter`, `AggregatedCounterData`, `AggregatedCounterStore`, `AggregatedCounterStoreError`, `AggregationDecision`, `AggregatorAuditEmitError`, `AggregatorAuditEventType`, `AggregatorAuditRecord`, `AggregatorAuditSink`, `AggregatorAuditSinkError`, `AggregatorError`, `ChainHash`, `ChainHeadRecord`, `CounterAggregator`, `CounterGroupKey`, `FailingAggregatedCounterStore`, `FailingAggregatorAuditSink`, `HashChainBuilder`, `InMemoryAggregatedCounterStore`, `InMemoryAggregatorAuditSink`, `InMemoryCounterAggregator`, `PeriodWindow`, `UpsertOutcome`, `CLOUDEVENTS_DATACONTENTTYPE`, `CLOUDEVENTS_SPECVERSION`, `COUNTER_AGGREGATED_EVENT_TYPE`, `GENESIS_PREV_HASH`, `GENESIS_SEQUENCE_NUMBER`, `aggregator_schema_version`, `canonical_aggregator_audit_event_strings`, `compute_canonical_bytes`, `deterministic_event_order`, `link_chain_hash`, `link_chain_hash_from_canonical`, `verify_chain_link`. O módulo upstream `aggregator` também permanece acessível como `corelink_billing::aggregator::aggregator`; os demais módulos públicos upstream são `audit`, `chain`, `error`, `event` e `store`. |
| [API-002](#api-002) · `corelink_billing::emit` | módulo `emit`; `BillingAuditEmitError`, `BillingAuditEventType`, `BillingAuditRecord`, `BillingAuditSink`, `BillingAuditSinkError`, `BillingEmitError`, `EmitOutcome`, `FailingBillingAuditSink`, `FailingR2UsageSink`, `IdemKey`, `IdempotencyDecision`, `IdempotencyTracker`, `InMemoryBillingAuditSink`, `InMemoryIdempotencyTracker`, `InMemoryR2UsageSink`, `InMemoryUsageEventEmitter`, `InvalidBillingPeriod`, `PersistedUsageLine`, `R2UsageSink`, `R2UsageSinkError`, `UsageEvent`, `UsageEventData`, `UsageEventEmitter`, `UsageEventKind`, `UsageUnit`, `CLOUDEVENTS_DATACONTENTTYPE`, `CLOUDEVENTS_SPECVERSION`, `GENESIS_IDEM_KEY`, `USAGE_EVENT_TYPE`, `billing_emit_schema_version`, `canonical_billing_audit_event_strings`, `canonical_r2_key`, `canonical_usage_event_kinds`, `compute_canonical_bytes_for_idem`, `derive_idem_key`, `derive_idem_key_from_canonical`, `validate_billing_period`. Upstream public modules: `audit`, `emitter`, `error`, `event`, `idempotency`, `sink`. |
| [API-003](#api-003) · `corelink_billing::reconcile` | módulo `reconcile`; `BillingReconciler`, `DriftHistoryInsertOutcome`, `DriftHistoryLedger`, `DriftHistoryRow`, `FailingDriftHistoryLedger`, `FailingReconcileAuditSink`, `FailingStripeSubmissionControl`, `InMemoryBillingReconciler`, `InMemoryDriftHistoryLedger`, `InMemoryReconcileAuditSink`, `InMemoryStripeSubmissionControl`, `LayerTotals`, `ReconcileAuditEmitError`, `ReconcileAuditEntry`, `ReconcileAuditEventType`, `ReconcileAuditRecord`, `ReconcileAuditSink`, `ReconcileAuditSinkError`, `ReconcileConfig`, `ReconcileDecision`, `ReconcileDriftHistoryError`, `ReconcileError`, `ReconcileLayerKind`, `ReconcileReport`, `ReconcileRunInput`, `ReconcileSeverity`, `ReconcileSnapshot`, `ReconcileStripePauseError`, `StripePauseOutcome`, `StripeSubmissionControl`, `TenantReconcileOutcome`, `TenantSnapshotInput`, `AUTO_FIX_MAX_PERCENT`, `AUTO_FIX_MAX_RECORDS`, `DEFAULT_RUN_STARTED_AT_MS`, `MARKER_CLEAN`, `MARKER_DRIFT`, `MARKER_ERROR`, `QUIET_THRESHOLD`, `SEV2_TO_SEV1_THRESHOLD`, `SEV3_TO_SEV2_THRESHOLD`, `auto_fix_gate_fires`, `audit_event_for_decision`, `canonical_reconcile_audit_event_strings`, `canonical_reconcile_layer_kinds`, `compute_drift_record_count`, `compute_max_drift`, `compute_pairwise_drift_pct`, `empty_input`, `parse_input`, `reconcile_schema_version`, `reconcile_snapshots`, `run_reconcile_pass`. Upstream public modules: `audit`, `drift`, `error`, `event`, `history`, `reconciler`, `run`, `stripe_pause`. |
| [API-004](#api-004) · `corelink_billing::stripe::schema` | módulo `stripe::schema`; `FailingStripeAuditSink`, `FailingStripeUsageLedger`, `FailingStripeWebhookLog`, `IdempotencyKey`, `InMemoryStripeAuditSink`, `InMemoryStripeBillingAdapter`, `InMemoryStripeUsageLedger`, `InMemoryStripeWebhookHandler`, `InMemoryStripeWebhookLog`, `RecordOutcome`, `StripeAdapterDecision`, `StripeAuditEmitError`, `StripeAuditEventType`, `StripeAuditRecord`, `StripeAuditSink`, `StripeAuditSinkError`, `StripeBillingAdapter`, `StripeError`, `StripeSignatureHeader`, `StripeUsageLedger`, `StripeUsageLedgerError`, `StripeWebhookLog`, `StripeWebhookLogError`, `StripeWebhookHandler`, `SubscriptionItemId`, `UsageRecordRequest`, `WebhookEvent`, `WebhookEventKind`, `WebhookHandleRequest`, `WebhookInsertOutcome`, `REPLAY_WINDOW_MS`, `compute_canonical_aggregate_bytes`, `compute_signature`, `derive_idempotency_key`, `derive_idempotency_key_from_canonical`, `stripe_schema_version`, `verify_stripe_signature`. Upstream public modules: `adapter`, `audit`, `error`, `event`, `idempotency`, `ledger`, `signature`, `webhook`, `webhook_log`. |
| [API-005](#api-005) · `corelink_billing::stripe::real` | módulos `stripe::real::{client,clock,dlq,error,portal,retry,webhook,webhook_dispatch}`; `AuditEmitter`, `AuditOutcome`, `AuditRecord`, `BillingPortalSessionCreator`, `CanonicalWebhookEventType`, `Clock`, `DispatchResponse`, `DlqError`, `DlqQuarantineOutcome`, `DlqReplayOutcome`, `FixedClock`, `IdempotencyOutcome`, `IdempotencyStore`, `IdempotencyToken`, `InMemoryFakeClock`, `InMemoryIdempotencyStore`, `InMemoryPortalAuditSink`, `InMemoryPortalSessionCreator`, `InMemoryWebhookDlqStore`, `MaterializerError`, `PortalAuditEvent`, `PortalAuditSink`, `PortalSessionError`, `PortalSessionUrl`, `RecordedPortalEvent`, `RecordingAuditEmitter`, `RecordingSliRecorder`, `RecordingStateMaterializer`, `RetryPolicy`, `SliObservation`, `SliRecorder`, `StateMaterializer`, `StripeError`, `StripeWebhookEnvelope`, `WebhookDispatcher`, `WebhookDlqRow`, `WebhookDlqStore`, `WebhookVerifyError`, `DEFAULT_DLQ_TTL_MS`, `DEFAULT_HUGR_STRIPE_REF`, `DEFAULT_HUGR_WALLET_BASE`, `DEFAULT_MAX_RETRIES`, `DEFAULT_STRIPE_API_BASE`, `DEFAULT_TOLERANCE_SECONDS`, `DLQ_DEPTH_GAUGE`, `DLQ_OLDEST_AGE_SECONDS_GAUGE`, `DLQ_PAGE_OLDEST_AGE_SECONDS`, `DLQ_PRUNED_TOTAL`, `DLQ_QUARANTINED_TOTAL`, `DLQ_REPLAYED_TOTAL`, `DLQ_WARN_DEPTH`, `SLI_BILLING_STRIPE_EVENT_SECONDS`. Target-conditional exports: native (`cfg(not(target_arch="wasm32")`) adds `StripeAuthMode`, `StripeClientConfig`, `StripeRealClient`, `StripeRealClientBuilder`, `SystemClock`; wasm32 adds `WasmWorkerClock`. |
| [API-006](#api-006) · `corelink_billing::stripe::traits` | módulo `stripe::traits`; `AuditEmitter`, `AuditOutcome`, `AuditRecord`, `CanonicalWebhookEventType`, `DispatchResponse`, `IdempotencyOutcome`, `IdempotencyStore`, `IdempotencyToken`, `MaterializerError`, `SLI_BILLING_STRIPE_EVENT_SECONDS`, `SliObservation`, `SliRecorder`, `StateMaterializer`, `StripeWebhookEnvelope`. |
| [API-007](#api-007) · `corelink_billing::stripe_materializer` | módulo `stripe_materializer`; `AuditSeverity`, `BillingAuditEmitter`, `BillingAuditError`, `BillingAuditRecord`, `BillingD1Error`, `BillingD1Writer`, `D1IdempotencyStore`, `D1SubscriptionStateHandler`, `EVENT_MATERIALIZATION_MATRIX`, `InMemoryBillingAuditEmitter`, `InMemoryBillingD1`, `InMemoryFakeMatClock`, `InMemoryRunnersEntitlementResolver`, `InMemoryTierSelector`, `MaterializedRow`, `MatClock`, `ProductionArchiveSink`, `ProductionAuditLine`, `RealStripeAuditEmitter`, `RunnersEntitlement`, `RunnersEntitlementResolver`, `SystemMatClock`, `TierSelectError`, `TierSelector`, `WasmWorkerMatClock`, `WebhookOutcome`, `default_mat_clock`, `SQL_DELETE_RUNNERS_ENTITLEMENT`, `SQL_DOWNGRADE_TIER`, `SQL_INSERT_DISPUTE`, `SQL_INSERT_REFUND`, `SQL_INSERT_WEBHOOK_EVENT_PROCESSED`, `SQL_MARK_SUBSCRIPTION_CANCELED`, `SQL_READ_TIER`, `SQL_UPSERT_CUSTOMER`, `SQL_UPSERT_INVOICE`, `SQL_UPSERT_RUNNERS_ENTITLEMENT`, `SQL_UPSERT_SUBSCRIPTION`, `SQL_UPSERT_TIER`. `CfD1BillingWriter` and `ArchiveProducerBillingEmitter` are additionally exported only under upstream feature `cf-billing-real`; target-specific clock exports are native `SystemMatClock` vs wasm32 `WasmWorkerMatClock`. |
| [API-008](#api-008) · `corelink_billing::tier` | módulo `tier`; `AlwaysDenyDpaGate`, `CheckoutSessionRow`, `CheckoutSessionRequest`, `CheckoutSessionResponse`, `DpaAcceptanceGate`, `FailingTierSelectionAuditSink`, `InMemoryDpaGate`, `InMemoryStripeClient`, `InMemoryTierSelectionAuditSink`, `StripeCheckoutSessionCompletedEvent`, `StripeClient`, `StripeCustomerId`, `SubscriptionActivationReceipt`, `SubscriptionState`, `TenantCtx`, `TenantId`, `TierError`, `TierKind`, `TierSelectionAuditEmitError`, `TierSelectionAuditEventType`, `TierSelectionAuditRecord`, `TierSelectionAuditSink`, `TierSelectionLedger`, `TierSelectionReceipt`, `TierSelectionRow`, `STRIPE_REPLAY_WINDOW_MS`, `TIER_SELECTION_LOCK_WINDOW_MS`, `canonical_runner_tiers`, `canonical_tier_selection_audit_event_strings`, `canonical_tiers`, `compute_stripe_signature`, `parse_stripe_signature_header`, `tier_selection_schema_version`, `verify_stripe_signature`. Upstream public modules: `audit`, `dpa`, `error`, `ledger`, `stripe`, `tenant`, `tier`. |
| [API-009](#api-009) · `corelink_billing::rate_headers` | módulo `rate_headers`; `CircuitAuditRecord`, `CircuitAuditSink`, `CircuitAuditSinkError`, `CircuitDecision`, `CircuitError`, `CircuitEventType`, `CircuitMetricKind`, `CircuitMetricsObserver`, `CircuitMetricsObserverError`, `CircuitState`, `CircuitStateSnapshot`, `CircuitThresholds`, `FailingCircuitAuditSink`, `FailingCircuitMetrics`, `GlobalCircuitBreaker`, `HealthObservation`, `InMemoryCircuitAuditSink`, `InMemoryCircuitMetrics`, `InMemoryGlobalCircuitBreaker`, `ManualOverrideTarget`, `ObservationStatus`, `RateLimitErrorBody`, `RateLimitHeaderBuilder`, `RateLimitHeaders`, `RateLimitPolicy`, `SignalEvaluation`, `TripReason`, `XRateLimitTypeKind`, `MIGRATION_0014_GLOBAL_CIRCUIT_STATE`, `DOCS_URL`, `ERROR_CODE_RATE_LIMIT_EXCEEDED`, `GLOBAL_CIRCUIT_RETRY_AFTER_SECS`, `HALFOPEN_DWELL_MS`, `HALFOPEN_SAMPLE_PCT`, `OBSERVATION_BUFFER_CAP`, `PER_IP_RETRY_AFTER_SECS`, `RECOVERY_RATIO_HALFOPEN_TO_CLOSED`, `RECOVERY_RATIO_OPEN_TO_HALFOPEN`, `RECOVERY_SAMPLE_SUCCESS_FLOOR`, `RETRY_AFTER_HARD_CEILING_SECS`, `ROLLING_WINDOW_MS`, `TIER_UPGRADE_URL`, `allow_halfopen_request`, `canonical_audit_event_strings`, `canonical_kind_list`, `canonical_metric_names`, `evaluate_signals`, `rate_headers_schema_version`. Upstream public modules: `audit`, `circuit`, `error`, `headers`, `metrics`. |
| [API-010](#api-010) · `corelink_billing::ratelimit` | módulo `ratelimit`; `BucketDecision`, `BucketKey`, `FailingRateLimitAuditSink`, `FailingRateLimitMetrics`, `InMemoryRateLimitAuditSink`, `InMemoryRateLimitMetrics`, `InMemoryTokenBucketRateLimiter`, `KeyDimension`, `NoOpRateLimitAuditSink`, `NoOpRateLimitMetrics`, `RateLimitAuditRecord`, `RateLimitAuditSink`, `RateLimitAuditSinkError`, `RateLimitConfig`, `RateLimitDecision`, `RateLimitError`, `RateLimitMetricKind`, `RateLimitMetricsObserver`, `RateLimitMetricsObserverError`, `RateLimitOutcome`, `RateLimitResultLabel`, `RateLimiter`, `RateLimitEventType`, `TokenBucketState`, `MIGRATION_0010_RATELIMIT_BUCKETS`, `BUSINESS_BURST`, `BUSINESS_REFILL_RPS`, `CANONICAL_CUSTOMER_BILLING_LABELS`, `DEFAULT_BURST_CAPACITY`, `DEFAULT_REFILL_RATE_PER_SEC`, `DEFAULT_RETRY_AFTER_FLOOR_SECS`, `ENTERPRISE_BURST`, `ENTERPRISE_REFILL_RPS`, `FREE_BURST`, `FREE_REFILL_RPS`, `KEY_DIMENSION_LIST`, `RETRY_AFTER_CANCELED_TENANT_SECS`, `RETRY_AFTER_HARD_CEILING_SECS`, `SOLO_BURST`, `SOLO_REFILL_RPS`, `TEAM_BURST`, `TEAM_REFILL_RPS`, `TIER_RATE_LADDER`, `bucket_try_acquire`, `canonical_audit_event_strings`, `canonical_metric_names`, `ratelimit_schema_version`, `refill_rate_for_tier`, `tier_for_billing_label`. Upstream public modules: `audit`, `bucket`, `config`, `error`, `key`, `limiter`, `metrics`, `tier`. |

<a id="api-001"></a>
[↩](#r01)
### API-001 — Fachada aggregator

**Símbolos exatos:** ver [catálogo API-001](#r04-export-catalog); lista enumerada, com cfg/feature indicados quando aplicável.
**Entrada / pré-condição:** uso das assinaturas Rust upstream; request e ports store/audit precisam estar válidos.
**Saída / efeitos:** agrega e encadeia hashes por ports; a facade não persiste nem cria estado.
**Erros / compatibilidade:** propaga erros tipados upstream; API incompatível quebra o alias. Implementação: `corelink-billing-aggregator`.
**Vínculos / evidência:** INV-012; REL-002; upstream `src/lib.rs` e `src/aggregator.rs`; `SOURCE`, não executado.

[Índice de contratos](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Fachada emit

**Símbolos exatos:** ver [catálogo API-002](#r04-export-catalog); lista enumerada, com cfg/feature indicados quando aplicável.
**Entrada / pré-condição:** período `YYYY-MM` válido e ports de audit, idempotência e sink implementados.
**Saída / efeitos:** emitter calcula idempotência e retorna `EmitOutcome`; efeitos dependem dos ports.
**Erros / compatibilidade:** erros tipados de período, emit, audit ou R2; breaking change afeta o alias. Implementação: `corelink-billing-emit`.
**Vínculos / evidência:** INV-012; REL-003; upstream `src/lib.rs` e facade `src/emit.rs`; `SOURCE`, não executado.

[Índice de contratos](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Fachada reconcile

**Símbolos exatos:** ver [catálogo API-003](#r04-export-catalog); lista enumerada, com cfg/feature indicados quando aplicável.
**Entrada / pré-condição:** snapshots/run input válidos e ports explícitos de audit, histórico e controle Stripe.
**Saída / efeitos:** calcula drift/decisão e relatório por tenant; efeitos dependem dos ports invocados.
**Erros / compatibilidade:** `ReconcileError` e erros tipados de parse/audit/history/pause; breaking change afeta o alias. Implementação: `corelink-billing-reconcile`.
**Vínculos / evidência:** INV-012; REL-004; upstream `src/lib.rs` e facade `src/reconcile.rs`; `SOURCE`, não executado.

[Índice de contratos](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Fachada Stripe schema

**Símbolos exatos:** ver [catálogo API-004](#r04-export-catalog); lista enumerada, com cfg/feature indicados quando aplicável.
**Entrada / pré-condição:** inputs de assinatura seguem Stripe; ledger/audit/webhook requerem ports configurados.
**Saída / efeitos:** expõe tipos wire e resultados via interfaces; `schema` não cria credenciais nem HTTP.
**Erros / compatibilidade:** erros tipados de Stripe/audit/ledger/log; mudança de wire/API afeta consumers. Implementação: `corelink-billing-stripe`.
**Vínculos / evidência:** INV-012; REL-005; upstream `src/lib.rs` e facade `src/stripe.rs`; `SOURCE`, não executado.

[Índice de contratos](#r04)

<a id="api-005"></a>
[↩](#r01)
### API-005 — Fachada Stripe real

**Símbolos exatos:** ver [catálogo API-005](#r04-export-catalog); lista enumerada, com cfg/feature indicados quando aplicável.
**Entrada / pré-condição:** client nativo exige config autorizada; dispatcher exige envelope, clock e ports audit/idempotency/materializer/SLI.
**Saída / efeitos:** client/dispatcher podem chamar transporte/ports upstream; a facade não certifica operação Stripe.
**Erros / compatibilidade:** erros tipados de client/webhook/DLQ/portal/materializer; cfg/API incompatível quebra consumers do target. Implementação: `corelink-stripe-real`.
**Vínculos / evidência:** INV-012; REL-016 e REL-021; upstream `src/lib.rs` e facade `src/stripe.rs`; `SOURCE`, runtime não observado.

[Índice de contratos](#r04)

<a id="api-006"></a>
[↩](#r01)
### API-006 — Fachada Stripe traits

**Símbolos exatos:** ver [catálogo API-006](#r04-export-catalog); lista enumerada, com cfg/feature indicados quando aplicável.
**Entrada / pré-condição:** implementadores satisfazem traits sync `Debug + Send + Sync` e inputs wire da origem.
**Saída / efeitos:** expõe quatro ports e tipos wire; invocação depende de dispatcher implementador.
**Erros / compatibilidade:** mudança de assinatura/outcome afeta implementadores; a facade não implementa transporte.
**Vínculos / evidência:** INV-012; REL-017; origem `corelink-billing-stripe-traits/src/lib.rs` e facade `src/stripe.rs`; `SOURCE`, runtime não observado.

[Índice de contratos](#r04)

<a id="api-007"></a>
[↩](#r01)
### API-007 — Fachada stripe materializer

**Símbolos exatos:** ver [catálogo API-007](#r04-export-catalog); lista enumerada, com cfg/feature indicados quando aplicável.
**Entrada / pré-condição:** handler recebe writer, clock, audit, idempotency e resolver; binder de produção depende da feature upstream.
**Saída / efeitos:** materialização usa ports; SQL constants identificam statements, não os executam.
**Erros / compatibilidade:** erros tipados D1/audit/tier; mudança SQL/API afeta callers. Implementação: `corelink-billing-stripe-materializer`.
**Vínculos / evidência:** INV-012; REL-006; upstream `src/lib.rs` e facade `src/stripe_materializer.rs`; `SOURCE`, D1 não observado.

[Índice de contratos](#r04)

<a id="api-008"></a>
[↩](#r01)
### API-008 — Fachada tier

**Símbolos exatos:** ver [catálogo API-008](#r04-export-catalog); lista enumerada, com cfg/feature indicados quando aplicável.
**Entrada / pré-condição:** contexto tenant/customer e ports DPA/audit/Stripe válidos.
**Saída / efeitos:** expõe tier/receipts/ledger upstream; wrapper não introduz efeitos.
**Erros / compatibilidade:** erros tipados de seleção/assinatura/ledger; mudanças de enum/trait afetam callers. Implementação: `corelink-tier-selection`.
**Vínculos / evidência:** INV-012; REL-007; upstream `src/lib.rs` e facade `src/tier.rs`; `SOURCE`, runtime não observado.

[Índice de contratos](#r04)

<a id="api-009"></a>
[↩](#r01)
### API-009 — Fachada rate headers

**Símbolos exatos:** ver [catálogo API-009](#r04-export-catalog); lista enumerada, com cfg/feature indicados quando aplicável.
**Entrada / pré-condição:** policy/kind/observações seguem a API; audit/metrics usam ports configurados.
**Saída / efeitos:** expõe headers ou decisão/snapshot circuit; migration é SQL, não executada pela facade.
**Erros / compatibilidade:** erros tipados circuit/audit/metrics; mudanças de wire/schema afetam callers. Implementação: `corelink-rate-headers`.
**Vínculos / evidência:** INV-012; REL-008; upstream `src/lib.rs` e facade `src/rate_headers.rs`; `SOURCE`, rota não traçada.

[Índice de contratos](#r04)

<a id="api-010"></a>
[↩](#r01)
### API-010 — Fachada ratelimit

**Símbolos exatos:** ver [catálogo API-010](#r04-export-catalog); lista enumerada, com cfg/feature indicados quando aplicável.
**Entrada / pré-condição:** limiter recebe key/config/quantidade conforme API; audit/metrics dependem de ports.
**Saída / efeitos:** expõe decisão e atualização do bucket upstream; migration não é aplicada pela facade.
**Erros / compatibilidade:** erros tipados limiter/audit/metrics; mudanças de tier/key/default/schema afetam callers. Implementação: `corelink-ratelimit`.
**Vínculos / evidência:** INV-012; REL-009; upstream `src/lib.rs` e facade `src/ratelimit.rs`; `SOURCE`, tráfego não observado.

[Índice de contratos](#r04)

<a id="api-011"></a>
[↩](#r01)
### API-011 — Decisão de abuso por tenant

**Símbolos exatos:** `AbuseFeatures`, `AbuseScore`, `AbuseDecision`, `AbuseConfig`, `AbuseScorer`, `compute_score`, `decide`.
**Entrada / pré-condição:** quatro features normalizadas e configuração válida.
**Saída / efeitos:** score `[0,1]` e decisão Benign/Suspicious/Malicious.
**Erros / compatibilidade:** `AbuseError`; auto-suspend retorna `AutoSuspendForbidden`.
**Vínculos / evidência:** INV-001/002; `src/abuse/{features,score,scorer}.rs`; `SOURCE`, teste não executado.

[Índice de contratos](#r04)

<a id="api-012"></a>
[↩](#r01)
### API-012 — Auditoria e configuração de abuso

**Símbolos exatos:** `InMemoryAbuseScorer`, sinks de audit/métrica e `AbuseConfig`.
**Entrada / pré-condição:** decisão local e sink in-memory configurado.
**Saída / efeitos:** audit/métrica precedem downgrade protegido no fake.
**Erros / compatibilidade:** falhas nos audits iniciais abortam antes do update; falha em `DowngradeApplied`, após `update_plan`, retorna erro, mas o limiter permanece atualizado; não há rollback no fake.
**Vínculos / evidência:** INV-003; `src/abuse/*`; source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`; teste atual falha no primeiro audit, não cobre falha pós-update e está `NOT_RUN` neste pin.

[Índice de contratos](#r04)

<a id="api-013"></a>
[↩](#r01)
### API-013 — Quota core por reserva

**Símbolos exatos:** `QuotaCheck`, `InMemoryQuotaCheck`, `QuotaDecision`, `QuotaConfig`, `ReservationTracker`, `provisional_retry_after_secs`.
**Entrada / pré-condição:** tenant, região, bytes e quota válidos.
**Saída / efeitos:** Allow/Reserve/Deny429; reserva isolada por tenant e TTL.
**Erros / compatibilidade:** `QuotaError`; Retry-After é provisório.
**Vínculos / evidência:** INV-004/005; `src/quota/core.rs`; `SOURCE`, teste não executado.

[Índice de contratos](#r04)

<a id="api-014"></a>
[↩](#r01)
### API-014 — Quota CAS

**Símbolos exatos:** `AtomicQuotaChecker`, `InMemoryAtomicQuotaChecker`, `AtomicCasState`, `QuotaCasDecision`, `days_until_month_reset_secs`.
**Entrada / pré-condição:** tenant, bytes e versão CAS.
**Saída / efeitos:** predicado estrito, versão monotônica e retry limitado.
**Erros / compatibilidade:** `QuotaCasError`; denial calcula Retry-After mensal.
**Vínculos / evidência:** INV-006/007; `src/quota/cas.rs`; `SOURCE`, teste não executado.

[Índice de contratos](#r04)

<a id="api-015"></a>
[↩](#r01)
### API-015 — Máquina de estados de quota

**Símbolos exatos:** `QuotaStateMachine`, `InMemoryQuotaStateMachine`, `QuotaState`, `QuotaTransition`, `QuotaFsmConfig`.
**Entrada / pré-condição:** tenant, utilização `[0,200]` e falhas.
**Saída / efeitos:** ladder de estados; audit precede UPSERT no fake.
**Erros / compatibilidade:** `QuotaFsmError`; no-op idempotente não muda estado.
**Vínculos / evidência:** INV-008/009; `src/quota/fsm.rs`; `SOURCE`, teste não executado.

[Índice de contratos](#r04)

<a id="api-016"></a>
[↩](#r01)
### API-016 — Replay forense

**Símbolos exatos:** `ReplayEngine`, `InMemoryReplayEngine`, `ReplayRequest`, `ReplayOutcome`, `ReplayDecision`.
**Entrada / pré-condição:** UUIDv7, tenant, período, motivo e role exata.
**Saída / efeitos:** decisão e auditoria; dry-run não grava ledger.
**Erros / compatibilidade:** `ReplayError`; payload divergente no mesmo request id é erro.
**Vínculos / evidência:** INV-010/011; `src/replay/{engine,archive,idempotency}.rs`; `SOURCE`, teste não executado.

[Índice de contratos](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

| Estado / recurso | Chave e owner | Persistência / vida útil | Escrita / leitura / durabilidade |
|---|---|---|---|
| score abuse | tenant no scorer | `HashMap` em memória no fake | scorer lê/escreve; produção não observada |
| reserva quota | tenant + reservation id | fake; migração 0009 embutida | tracker; D1 real não observado |
| CAS quota | tenant + versão | fake; migração 0012 embutida | CAS atômico no fake |
| FSM quota | tenant | fake; owner prevê mirror D1 | state machine e store fake |
| replay | tenant + request id | archive/ledger fake; migração 0021 externa | engine; R2/D1 reais não observados |

**Índice:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) ·
[INV-004](#inv-004) · [INV-005](#inv-005) · [INV-006](#inv-006) · [INV-007](#inv-007) ·
[INV-008](#inv-008) · [INV-009](#inv-009) · [INV-010](#inv-010) · [INV-011](#inv-011) · [INV-012](#inv-012) · [FLOW-001](#flow-001).

<a id="inv-012"></a>
[↩](#r01)
### INV-012 — Fachadas preservam identidade dos exports

**Regra:** cada nome enumerado no catálogo API-001–010 resolve ao símbolo upstream homônimo; a façade não substitui sua implementação.
**Imposição:** `pub use <crate>::*` nos módulos de fachada e lista upstream em cada API.
**Violação / verificação:** símbolo omitido, sombreado ou com identidade local; comparar imports canônicos e upstream por compilação/`TypeId`. Smoke existente cobre aggregator e Stripe real; demais exports não têm execução anexada neste pin (`NOT_RUN`).

[Índice de estado](#r05)

<a id="inv-001"></a>
### INV-001 — Score de abuso permanece no intervalo fechado

**Regra:** `0.0 ≤ AbuseScore ≤ 1.0`; NaN e valores não finitos não escapam ao intervalo.
**Imposição:** construtores em `abuse/{features,score}.rs` normalizam/clampam entradas.
**Violação / teste definido:** score negativo, NaN ou acima de um; `prop_score_in_range_0_1` existe no source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`; `execution_status: BLOCKED_NOT_EXECUTED`, sem saída de execução neste pin.

[Índice de estado](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Suspensão não é aplicada automaticamente

**Regra:** chamada programática de auto-suspend retorna `AbuseError::AutoSuspendForbidden`.
**Imposição:** `AbuseScorer::auto_suspend` e fluxo do scorer.
**Violação / teste definido:** score alto levando a suspensão automática; `auto_suspend_at_any_score_returns_forbidden` consta no source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`; `execution_status: BLOCKED_NOT_EXECUTED`, sem saída de execução neste pin.

[Índice de estado](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Auditoria prévia não impede efeito parcial pós-update

**Regra:** audits iniciais falhos abortam antes do update. Se `DowngradeApplied` falha após `update_plan`, há erro com efeito parcial e sem rollback no fake.
**Imposição:** `score_and_apply` emite dois audits, chama `update_plan` e depois emite `DowngradeApplied`; janela e métricas vêm depois.
**Violação / teste definido:** presumir ausência de efeito ou rollback após erro tardio. `audit_failure_aborts_decision_and_no_downgrade_applied` usa sink sempre falho e cobre apenas a primeira emissão. No pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`: `BLOCKED_NOT_EXECUTED`; sem output ou teste da falha tardia.

[Índice de estado](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Reserva de quota é isolada por tenant e expira

**Regra:** tenant A não lê/remove reserva de B; reserva expirada não conta bytes ativos.
**Imposição:** chave composta e sweep no `ReservationTracker` fake.
**Violação / verificação:** cross-tenant read ou leak após TTL; `prop_tenant_isolation` e o teste de expiry constam do source. Não há saída de execução anexada para a baseline histórica nem para este pin; status de execução `UNKNOWN`.

[Índice de estado](#r05)

<a id="inv-005"></a>
[↩](#r01)
### INV-005 — Quota core nega acima do limite

**Regra:** `used + reservations + request > quota` produz `Deny429`.
**Imposição:** `QuotaCheck::check_and_reserve` e `QuotaConfig`.
**Violação / verificação:** bytes acima do teto permitidos; `prop_quota_check_at_100pct_denies_429` consta do source, mas não há saída de execução anexada; status `NOT_VERIFIED`.

[Índice de estado](#r05)

<a id="inv-006"></a>
[↩](#r01)
### INV-006 — CAS usa predicado estrito e versão monotônica

**Regra:** `used + request < quota`; mutação bem-sucedida aumenta `cas_version`.
**Imposição:** `AtomicCasState::try_acquire` e checker com retry limitado.
**Violação / verificação:** dupla aprovação no limite ou versão regressiva; as propriedades CAS estão presentes no source, mas não há saída de execução anexada para a baseline histórica nem para este pin; execução `NOT_VERIFIED`.

[Índice de estado](#r05)

<a id="inv-007"></a>
[↩](#r01)
### INV-007 — Retry-After CAS termina no próximo mês UTC

**Regra:** denial usa segundos até próxima meia-noite UTC do dia um, com piso canônico.
**Imposição:** `days_until_month_reset_secs` em `quota/cas/retry_after.rs`.
**Violação / verificação:** fevereiro/dezembro ou último segundo errados; as propriedades de calendário estão presentes no source, mas não há saída de execução anexada para a baseline histórica nem para este pin; execução `NOT_VERIFIED`.

[Índice de estado](#r05)

<a id="inv-008"></a>
[↩](#r01)
### INV-008 — FSM respeita ladder e idempotência

**Regra:** 80/95/100 e três falhas dirigem transições; mesma entrada sem mudança resulta NoChange.
**Imposição:** `utilization_bucket` e `InMemoryQuotaStateMachine`.
**Violação / verificação:** transição duplicada ou limiar errado; as propriedades FSM estão presentes no source, mas não há saída de execução anexada para a baseline histórica nem para este pin; execução `NOT_VERIFIED`.

[Índice de estado](#r05)

<a id="inv-009"></a>
[↩](#r01)
### INV-009 — Audit de FSM precede UPSERT

**Regra:** em transição mutável, erro do audit impede escrita de estado.
**Imposição:** ordem no `InMemoryQuotaStateMachine`.
**Violação / verificação:** store alterado após audit falho; `audit_failure_aborts_no_state_mutation` consta do source, mas não há saída de execução anexada; status `NOT_VERIFIED`.

[Índice de estado](#r05)

<a id="inv-010"></a>
[↩](#r01)
### INV-010 — Replay requer role forense exata

**Regra:** apenas `billing_forensics_admin` alcança braço executado.
**Imposição:** verificação inicial do `InMemoryReplayEngine`.
**Violação / verificação:** role regular executando replay; teste de role exata consta do source histórico, mas não há saída de execução anexada para a baseline nem para este pin; status `NOT_VERIFIED`.

[Índice de estado](#r05)

<a id="inv-011"></a>
[↩](#r01)
### INV-011 — Replay idempotente detecta payload divergente

**Regra:** mesmo request id retorna resultado anterior somente para mesma requisição; divergência é erro.
**Imposição:** `ReplayIdempotencyLedger` e `RecordOutcome`.
**Violação / verificação:** colisão silenciosa; `divergent_payload_collision_surfaces_error` consta do source, mas não há saída de execução anexada; status `NOT_VERIFIED`.

[Índice de estado](#r05)

<a id="flow-001"></a>
[↩](#r01)
### FLOW-001 — Replay autorizado

Role é validada; audit de autorização ocorre; archive é lido; divergência é classificada; audit
de execução e eventual divergência antecedem escrita no ledger. Falha de audit, archive ou ledger
retorna erro. Esta sequência foi exercitada no fake, não em rota Worker/R2/D1.

[Índice de estado](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

| Nome / origem / classe | Default efetivo | Leitura | Target / condição | Efeito | Validação / falha |
|---|---|---|---|---|---|
| features Cargo / `Cargo.toml` / `SOURCE` | nenhuma declarada | resolução | library/test normal | sem matriz de features | ausência não falha; `SOURCE`, não executado |
| `AbuseConfig` / `abuse/config.rs` / `SOURCE` | pesos/limiares canônicos | construção scorer | lógica local | muda score/decisão | API normaliza/rejeita inválido; não executado |
| `QuotaConfig` / `quota/core.rs` / `SOURCE` | limites canônicos | construção checker | lógica local | muda reserva/Retry-After | `QuotaError`; não executado |
| `QuotaCasConfig` / `quota/cas.rs` / `SOURCE` | limite/piso canônicos | construção checker | lógica local | muda CAS/Retry-After | `QuotaCasError`; não executado |
| `QuotaFsmConfig` / `quota/fsm.rs` / `SOURCE` | ladder e defaults | construção | lógica local | muda transição | `QuotaFsmError`; não executado |
| `ReplayConfig` / `replay/*` / `SOURCE` | role/políticas canônicas | construção engine | lógica local | muda replay | `ReplayError`; não executado |
| migrations 0009/0012/0013 / source embutido / `SOURCE` | strings versionadas | build/runtime consumer | caller autorizado | bytes podem ser aplicados | não aplica D1; não executado |

**Matriz suportada:** o comando abaixo é o procedimento selecionado, não resultado desta revisão:
`cargo test --locked --offline -p corelink-billing --target x86_64-apple-darwin`.
Classe `SOURCE`; wasm, release, clippy e binding remoto não foram executados nesta revisão.
**Combinações recusadas / stubs:** fakes não são parsers D1, Workers, R2 nem Stripe real.

<a id="r07"></a>
## R07 — Erros e observabilidade

| Sinal / erro | Causa no contrato | Estado após falha | Diagnóstico / procedimento |
|---|---|---|---|
| `AutoSuspendForbidden` | tentativa programática de suspensão | não aplica suspensão | PROC-003 |
| `QuotaError` / `QuotaCasError` | limite, estado ausente, overflow ou corrida | decisão/escrita é abortada no fake | PROC-004 |
| `QuotaFsmError::Audit` | sink falhou antes de UPSERT | estado não é alterado no fake | PROC-005 |
| `ReplayError` | role, archive, audit ou ledger inválido | depende do braço; ledger não deve ser adiantado | PROC-006 |
| taxonomias de audit/métrica | decisão/transição | registros em sinks fake | PROC-002 |

**Limites:** métricas in-memory não provam cardinalidade nem exportação; erros podem carregar ids de
tenant e não devem ser copiados para logs sem política de redaction do consumer.

<a id="r08"></a>
## R08 — Verificação e evidências

| API / INV / FLOW | Fonte / classe | Teste / método | Resultado e limite |
|---|---|---|---|
| API-001–010 | fachada `src/*.rs` / `SOURCE` | smoke de paths | `REVIEWED_NOT_EXECUTED`; origem/runtime não observados |
| API-011–012 / INV-001–003 | `src/abuse/*` / `SOURCE`, pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0` | `abuse_*` | `BLOCKED_NOT_EXECUTED`; nenhum output neste pin; teste atual só cobre falha de audit pré-update; sem binding real |
| API-013 / INV-004–005 | `src/quota/core/*` / `SOURCE` | `quota_core_*` | `REVIEWED_NOT_EXECUTED`; D1 não observado |
| API-014 / INV-006–007 | `src/quota/cas/*` / `SOURCE` | `quota_cas_*` | `REVIEWED_NOT_EXECUTED`; latência real desconhecida |
| API-015 / INV-008–009 | `src/quota/fsm/*` / `SOURCE` | `quota_fsm_*` | `REVIEWED_NOT_EXECUTED`; Worker não observado |
| API-016 / INV-010–011 | `src/replay/*` / `SOURCE` | `replay_prop_billing_replay` | `REVIEWED_NOT_EXECUTED`; R2/D1 não observado |
| package | Cargo/source / `SOURCE` | comando de R06 | não executado nesta revisão; sem contagem alegada |
| teste quota CAS | `crates/corelink-billing/tests/quota_cas_prop_quota_cas.rs` / `SOURCE` | comparação `cca798ff` → `fb611330` | predicado de p99 mudou de `<5 ms` para `≤5 ms`; teste não executado |

**Desconhecidos e contradições:** reexports externos exigem revisão dos crates de origem; o código
descreve integração futura/remota em alguns comentários, mas esta referência não a trata como prova.
**Continuar:** [Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01).

[Voltar ao início](#r01)
