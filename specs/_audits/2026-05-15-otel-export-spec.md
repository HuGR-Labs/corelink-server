---
id: AUDIT-2026-05-15-OTEL-EXPORT
type: audit
doc_status: REVIEW
audit_status: ACTIVE
version: 0.1.0
created: 2026-05-15
updated: 2026-05-15
owner: Gustavo Schneiter
final_approver: Gustavo Schneiter
reviewers: []
supersedes: null
superseded_by: null
tags: [audit, observability, otel-export, enterprise, r-prep, datadog, otel, grafana]
---

# `corelink-otel-export` — Spec Audit (2026-05-15)

> **Scope.** This audit fixes the data contract for the customer-facing
> observability forwarder shipped in `crates/corelink-otel-export/` and
> documented in `apps/docs/docs/how-to/observability/*.mdx`. It pins
> three things per `observability_model.md §10`: **(1)** which metrics
> we forward, **(2)** which traces we forward, **(3)** the per-metric
> data classification — proving PII-free by construction. The audit is
> the falsifiability target the property test
> `prop_exported_label_set_pii_clean` runs against; any drift between
> this doc and the trait surface is a P0 spec violation.

---

## 1. Threat model

The enterprise customer's threat model for "let CoreLink talk to my
Datadog / OTel Collector / Grafana Cloud" is:

1. **Data exfiltration via observability.** A bug in our forwarder
   leaks raw `tenant_id`, customer IPs, PAT prefixes, S3 paths, or
   action digest contents into the customer's third-party SaaS. The
   customer's compliance team gets paged because **CoreLink** wrote
   PII into Datadog without a DPA covering Datadog as a sub-processor
   of CoreLink's data.

2. **Observability stack outage cascading into CoreLink unavailability.**
   Datadog has a regional outage; CoreLink's request path retries the
   metric submission until it times out; CoreLink's p99 latency blows
   the SLO. The customer's incident is "CoreLink is down because their
   Datadog integration is down."

3. **API-key timing oracle.** A naive `==` comparison on the API key
   leaks the matching prefix length over a sufficiently large sample,
   eroding a 32-char hex key down to ~16 chars of effective entropy.

4. **Cardinality blow-up.** A bad PR adds a new metric label that
   embeds a high-cardinality value (e.g., raw `tenant_id`), and the
   customer's Datadog bill explodes overnight.

This audit closes (1), (2), (3), and (4) by construction. The trait
surface + this doc are the falsifiability targets.

## 2. What we forward — metrics

| `RedMetricKind` variant | Prom slug | Default labels | Class | Forward by default |
|---|---|---|---|---|
| `CasPutRequestsTotal` | `corelink_cas_put_requests_total` | `{tenant_tier, region, result}` | PII-free | **YES** |
| `CasPutDurationSeconds` | `corelink_cas_put_duration_seconds` | `{tenant_tier, region}` | PII-free | **YES** |
| `CasGetBytesTotal` | `corelink_cas_get_bytes_total` | `{tenant_tier, region}` | PII-free | **YES** |
| `AcLookupRequestsTotal` | `corelink_ac_lookup_requests_total` | `{tenant_tier, region, hit_miss}` | PII-free | **YES** |
| `GcRunsTotal` | `corelink_gc_runs_total` | `{phase, result}` | PII-free | **YES** |
| `DedupRatio` | `corelink_dedup_ratio` | `{tenant_tier, region}` | PII-free | **YES** |
| `RateLimitRejectsTotal` | `corelink_rate_limit_rejects_total` | `{layer, tenant_tier, reason}` | PII-free | **YES** |
| `PrivacyDsrActiveTotal` | `corelink_privacy_dsr_active_total` | `{dsr_type}` | PII-free | **YES** |
| `BillingEventsEmittedTotal` | `corelink_billing_events_emitted_total` | `{event_type, region}` | PII-free | **YES** |
| `CfCpuTimeUs` | `corelink_cf_cpu_time_us` | `{region}` | PII-free | YES (low-value opt-out) |
| `R2OpsTotal` | `corelink_r2_ops_total` | `{bucket, op_type}` | PII-free | YES (low-value opt-out) |
| `D1RowScansTotal` | `corelink_d1_row_scans_total` | `{database}` | PII-free | YES (low-value opt-out) |
| `KvReadQuotaUsed` | `corelink_kv_read_quota_used` | `{namespace}` | PII-free | YES (low-value opt-out) |
| `KvWriteQuotaUsed` | `corelink_kv_write_quota_used` | `{namespace}` | PII-free | YES (low-value opt-out) |
| `DoStorageSizeBytes` | `corelink_do_storage_size_bytes` | `{do_class}` | PII-free | YES (low-value opt-out) |

**Forwarding policy.** All 9 top-level RED + 6 USE metrics are
**per-tenant by default**. The 6 USE metrics are classified
"low-value high-cost" (per-DO storage and per-DB row-scan have
moderate cardinality on multi-region deployments); a single
`opt_out_low_value_metrics = true` toggle drops them. The
SLO-bound metrics (the 9 RED) are NEVER opt-outable — they are the
SLO falsifiability target.

**Verification.** The `prop_exported_label_set_pii_clean` property
test enumerates every canonical label key allowed and rejects any
denylisted PII key (`email`, `tenant_id_raw`, `ip`, `token`,
`password`, `api_key`, `phone`, `name`, `address`, `cpf`, `cnpj`,
`ssn`). The denylist is intentionally generous — drift between this
audit and the test denylist is a P0 spec violation.

## 3. What we forward — traces

| Span source | Sample rate (prod) | Sample rate (staging) | Per-tenant | Notes |
|---|---|---|---|---|
| Request-scoped server spans (CAS GET / PUT, AC Lookup, REAPI methods) | 1% | 100% | YES | Per CF Worker sampling config; canonical `RateBasedSampler` in `corelink-tracing` |
| Errors (any span with `SpanStatus::Error`) | 100% | 100% | YES | Tail-based sampling — errors always sampled |
| p99 SLO-breach paths | 100% | 100% | YES | Tail-based; canonical Google SRE Workbook Ch 6 |
| Internal infra spans (GC, eviction, billing aggregation) | 0% | 100% | NO | NEVER forwarded — these are CoreLink-internal |

**W3C Trace Context.** Every forwarded span carries a canonical W3C
`traceparent` (16-byte `trace_id` + 8-byte `span_id`) per
`corelink-tracing::parse_traceparent` / `format_traceparent` strict
ABNF (W3C Trace Context Recommendation 2020). No `tracestate` vendor
extensions are propagated in this WI — deferred to a follow-up.

**Span name shape.** `service.operation` (e.g., `cas.put`, `ac.lookup`,
`reapi.findMissingBlobs`). These match the canonical span catalog in
`observability_model.md §6.2`.

## 4. Data classification — proof of PII-free

| Label key | Class | Example values | Notes |
|---|---|---|---|
| `tenant_tier` | Pseudonymous | `free`, `pro`, `enterprise` | Closed enum; no per-tenant cardinality |
| `region` | Public | `us-east-1`, `eu-central-1` | CF Workers region; ~12 canonical values |
| `result` | Closed enum | `ok`, `error`, `rate_limited` | ~5 canonical values |
| `hit_miss` | Closed enum | `hit`, `miss` | 2 values |
| `phase` | Closed enum | `mark`, `sweep`, `soft_delete`, `phys_delete` | GC phase taxonomy |
| `layer` | Closed enum | `edge`, `worker`, `do`, `r2` | Rate-limit enforcement layer |
| `reason` | Closed enum | `quota_exceeded`, `abuse_signal`, ... | ~10 canonical values |
| `bucket` | Closed enum | R2 bucket name (operator-controlled) | ≤ 20 buckets per CF account |
| `op_type` | Closed enum | `get`, `put`, `delete`, `head`, `list` | R2 op taxonomy |
| `database` | Closed enum | D1 database name (operator-controlled) | ≤ 50 DBs per CF account |
| `namespace` | Closed enum | KV namespace name (operator-controlled) | ≤ 50 namespaces per CF account |
| `do_class` | Closed enum | DO class name (CoreLink-internal) | ≤ 30 classes |
| `event_type` | Closed enum | Billing event type taxonomy | ~10 canonical values |
| `dsr_type` | Closed enum | `export`, `delete`, `rectify`, `restrict` | LGPD/GDPR DSR types |

**Why this is PII-free by construction.**

1. The metric kind is a **closed enum** (`RedMetricKind` 15 variants) —
   no string-typed metric names accepted in the hot path. Adding a new
   metric requires a PR that amends `corelink-analytics::canonical` +
   `observability_model.md §4.2`. CODEOWNERS routes this to the
   observability-discipline reviewer.

2. The label set is a **closed enum** of pseudonymous-only keys (14
   canonical keys above). The free-form `tenant_id` is **pseudonymized
   at the audit boundary** (per `corelink-logpush::redaction` + the
   `corelink-privacy-pseudonymize` crate) BEFORE export; the customer
   sees `tenant_tier` (closed enum) but never the raw tenant ID.

3. The `RedactorMiddleware` re-runs the canonical 5-pattern PII
   redaction (`Email` / `Ip` / `Token` / `Pan` / `CpfCnpj`) on every
   span attribute and every metric label value at the export boundary.
   Defense in depth — even if a future bug introduces a PII-leaking
   label, the redactor catches it before bytes leave CoreLink.

4. The property test
   `prop_exported_label_set_pii_clean` enumerates 32 random canonical
   metric points and asserts NO denylisted PII key ever appears. The
   denylist is intentionally generous (12 keys; add more as the
   compliance taxonomy evolves).

## 5. Fail-OPEN proof

| Failure path | Customer observable | CoreLink request path | Audit event |
|---|---|---|---|
| Vendor 401/403 (wrong API key) | Dashboard goes dark | UNAFFECTED | `corelink.observability.export_failed` |
| Vendor 429 (rate-limit) | Dashboard sparse | UNAFFECTED | `corelink.observability.export_failed` |
| Vendor 5xx (vendor outage) | Dashboard goes dark | UNAFFECTED | `corelink.observability.export_failed` |
| Network unreachable (DNS / TLS) | Dashboard goes dark | UNAFFECTED | `corelink.observability.export_failed` |
| Vendor 400 (payload rejected) | Dashboard sparse | UNAFFECTED | `corelink.observability.export_failed` |
| Internal serialization failure | Dashboard sparse | UNAFFECTED | `corelink.observability.export_failed` |
| Audit chain itself failed | **PROPAGATES** (P0 incident) | UNAFFECTED | (audit chain unavailable; SRE paged) |

**Pinned by** `prop_fail_open_under_any_transport_error` — over any
sequence of injected transport errors, the orchestrator returns
`Ok(())` to the caller and emits exactly one audit event per failure.

The ONLY error path that propagates is an audit-sink internal failure
(audit chain unavailable). That is itself a CRITICAL P0 — silently
dropping audit evidence is worse than degrading availability per
`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`.

## 6. Constant-time API-key compare

API-key + bearer-token + Grafana password equality goes through
`subtle::ConstantTimeEq` via `secret::constant_time_secret_eq`.
**Pinned by** `prop_constant_time_secret_eq_total_function`:

- Reflexive over arbitrary byte sequences.
- Symmetric over arbitrary byte sequences.
- Total (never panics) on length mismatch.
- Matches stdlib `==` on equality (no observable behavior difference;
  only timing differs).

Length-mismatch short-circuit is acceptable in our threat model
because canonical lengths are published (Datadog API key: 32-char hex
canonical, conservative band `[16, 128]`; Grafana Cloud token:
opaque ASCII band `[16, 256]`; OTel bearer: opaque ASCII band
`[8, 4096]`).

## 7. Sub-processor / DPA implications

When a customer enables export to Datadog / Grafana Cloud / a vendor
fronted by their OTel Collector, that vendor becomes a sub-processor
of THEIR data — not of CoreLink's data. CoreLink ships only the
forwarder; the customer holds the DPA with Datadog/Grafana on their
own paper.

For the OTel Collector route this is especially clean: CoreLink streams
to the **customer's own infrastructure**; the customer routes onward.
CoreLink has no contractual relationship with whatever the Collector
fans out to.

Cross-link: [Trust → Sub-processors](../../apps/docs/docs/trust/subprocessors.mdx)
documents CoreLink's own sub-processor list (Cloudflare, Stripe,
PagerDuty). Datadog / Grafana / Honeycomb / etc. are NOT on that
list because CoreLink does not contract them — the customer does.

## 8. Future work (out of scope for this WI)

- **Real HTTP/gRPC wiring per vendor.** Per the
  `trait-abstraction-defer` charter pattern, the real `reqwest` /
  `tonic` client + Prom remote-write protobuf + Snappy encoder lands
  in a follow-up WI alongside its own chaos test (induced 5xx, induced
  network partition, induced auth reject, induced over-quota throttle).
- **`tracestate` vendor extension propagation** (W3C §3.3) — deferred.
- **Multiplex exporter** — a single CoreLink tenant fan-out to N
  vendors simultaneously without a Collector. Customer demand pending.
- **Customer-tunable label allow-list** — today the closed set is
  baked in; future iterations may allow customers to opt OUT of
  specific labels for additional privacy posture (e.g., drop
  `region`).

## 9. Cross-references

- `crates/corelink-otel-export/` (canonical implementation).
- `apps/docs/docs/how-to/observability/forward-to-{datadog,otel-collector,grafana-cloud}.mdx`
  (customer setup guides).
- `specs/03_architecture/observability_model.md` (the canonical
  metric + label + trace taxonomy this audit constrains).
- `specs/03_architecture/privacy_model.md` (PII / pseudonymization
  contract).
- `specs/03_architecture/security_model.md §10` (constant-time
  secret compare canonical pattern).
- Trust center: [Data handling](../../apps/docs/docs/trust/data-handling.mdx),
  [Sub-processors](../../apps/docs/docs/trust/subprocessors.mdx).
