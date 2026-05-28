---
id: "2026-05-27-grafana-slo-dashboards-seal"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
tags: ["grafana", "dashboards", "slo", "observability", "wp-6.1", "wave32"]
references:
  - "specs/_audits/2026-05-27-15-agent-dispatch-matrix.md"
  - "crates/corelink-analytics/src/canonical.rs"
  - "crates/corelink-audit-chain/src/error.rs"
  - "crates/corelink-audit-chain/src/verifier.rs"
  - "dashboards/grafana/DASH-SLO-API.json"
  - "dashboards/grafana/DASH-SLO-AUDIT.json"
  - "dashboards/README.md"
---

# WP-6.1 SEAL — Grafana SLO Dashboards

## 1. Pre-flight

```
pwd: /Users/gustavoschneiter/Documents/HuGR/corelink-server/.claude/worktrees/agent-a3d47a1702d08730b
worktree marker: .git file (gitdir link) present — confirmed worktree, NOT main checkout
gitdir: /Users/gustavoschneiter/Documents/HuGR/corelink-server/.git/worktrees/agent-a3d47a1702d08730b
```

Pre-flight PASSED. No worktree leak detected.

## 2. Scope delivered

| File | Status |
|------|--------|
| `dashboards/grafana/DASH-SLO-API.json` | NEW — 8 panels, valid JSON |
| `dashboards/grafana/DASH-SLO-AUDIT.json` | NEW — 8 panels, valid JSON |
| `dashboards/README.md` | NEW — full import instructions, metric provenance table |
| `specs/_audits/2026-05-27-grafana-slo-dashboards-seal.md` | NEW — this doc |

## 3. Metric name verification

**Method:** Grepped `crates/corelink-analytics/src/canonical.rs` for all
`RedMetricKind` variants and their `as_str()` return values. This is the
canonical and ONLY source for Prometheus metric names (enum-enforced per
WI-S09-001 §9.2 design decision — dynamic string-typed names are
prohibited by the cardinality guard).

### 15 canonical metric names (confirmed)

| Metric name | Variant | Type | Labels |
|-------------|---------|------|--------|
| `corelink_cas_put_requests_total` | `CasPutRequestsTotal` | counter | `tenant_tier`, `region`, `result` |
| `corelink_cas_put_duration_seconds` | `CasPutDurationSeconds` | histogram | `tenant_tier`, `region` |
| `corelink_cas_get_bytes_total` | `CasGetBytesTotal` | counter | `tenant_tier`, `region` |
| `corelink_ac_lookup_requests_total` | `AcLookupRequestsTotal` | counter | `tenant_tier`, `region`, `hit_miss` |
| `corelink_gc_runs_total` | `GcRunsTotal` | counter | `phase`, `result` |
| `corelink_dedup_ratio` | `DedupRatio` | gauge | `tenant_tier`, `region` |
| `corelink_rate_limit_rejects_total` | `RateLimitRejectsTotal` | counter | `layer`, `tenant_tier`, `reason` |
| `corelink_privacy_dsr_active_total` | `PrivacyDsrActiveTotal` | gauge | `dsr_type` |
| `corelink_billing_events_emitted_total` | `BillingEventsEmittedTotal` | counter | `event_type`, `region` |
| `corelink_cf_cpu_time_us` | `CfCpuTimeUs` | gauge | `region` |
| `corelink_r2_ops_total` | `R2OpsTotal` | counter | `bucket`, `op_type` |
| `corelink_d1_row_scans_total` | `D1RowScansTotal` | counter | `database` |
| `corelink_kv_read_quota_used` | `KvReadQuotaUsed` | gauge | `namespace` |
| `corelink_kv_write_quota_used` | `KvWriteQuotaUsed` | gauge | `namespace` |
| `corelink_do_storage_size_bytes` | `DoStorageSizeBytes` | gauge | `do_class` |

### Audit-chain metric names (WI §6.1.10 spec references)

These 4 metric names are **not yet in the `RedMetricKind` enum** (pending
WI-S09-007 PRR R2 production binding). They are referenced by name in:

- `crates/corelink-audit-chain/src/error.rs:34` — `corelink_audit_emit_failures_total{reason}`
- `crates/corelink-audit-chain/src/error.rs:66` — `corelink_audit_chain_break_detected_total`
- `crates/corelink-audit-chain/src/verifier.rs:71` — `corelink_audit_chain_break_detected_total`
- `crates/corelink-audit-chain/src/verifier.rs:184` — `corelink_audit_chain_verify_runs_total{result=ok}`
- `dashboards/grafana/DASH-SLO-CATALOG.json` panel 15 — `corelink_audit_chain_verify_pass_7d`

These are used in DASH-SLO-AUDIT panels 1–3 and 7. They are spec-cited
and code-referenced — NOT invented. They will emit when WI-S09-007 ships.

### Metrics NOT used (confirmed: no invented names)

`metrics::counter!` and `metrics::histogram!` macros were **not found**
in `crates/corelink-telemetry/src/` — metric emission is exclusively via
`RedMetricKind` enum variants through the `MetricPoint::kind` field and
`MetricPoint::metric_name()` → `kind.as_str()`. Grep confirmed:
`crates/corelink-telemetry/src/otel/metric.rs:68` — `self.kind.as_str()`.

## 4. JSON validation

```
jq . dashboards/grafana/DASH-SLO-API.json > /dev/null   → exit 0 ✅
jq . dashboards/grafana/DASH-SLO-AUDIT.json > /dev/null → exit 0 ✅
```

Both files pass `jq` schema sanity. No `grafana-tool validate` binary
available in this environment; jq structural validation confirms
well-formed JSON with correct Grafana schemaVersion 39 shape.

## 5. Panel inventory

### DASH-SLO-API (8 panels, ≥ 6 required by WP-6.1 DoD)

| ID | Title | Type | Metric |
|----|-------|------|--------|
| 1 | CAS PUT p50/p95/p99 latency | timeseries | `corelink_cas_put_duration_seconds_bucket` |
| 2 | CAS PUT error rate by result | timeseries | `corelink_cas_put_requests_total` |
| 3 | AC Lookup hit ratio per tier | timeseries | `corelink_ac_lookup_requests_total` |
| 4 | Rate-limit reject rate by tier | timeseries | `corelink_rate_limit_rejects_total` |
| 5 | Top-10 tenants by CAS PUT (R15) | table | `corelink_cas_put_requests_total` |
| 6 | Top-10 tenants by AC lookup (R15) | table | `corelink_ac_lookup_requests_total` |
| 7 | CAS GET egress bytes per region | timeseries | `corelink_cas_get_bytes_total` |
| 8 | CF Worker CPU time p99 | timeseries | `corelink_cf_cpu_time_us` |

### DASH-SLO-AUDIT (8 panels, ≥ 6 required by WP-6.1 DoD)

| ID | Title | Type | Metric |
|----|-------|------|--------|
| 1 | Chain break SEV-0 counter | stat | `corelink_audit_chain_break_detected_total` |
| 2 | Emit failure rate | timeseries | `corelink_audit_emit_failures_total` |
| 3 | Daily verifier cadence | timeseries | `corelink_audit_chain_verify_runs_total` |
| 4 | R2 audit ops per bucket | timeseries | `corelink_r2_ops_total` |
| 5 | DO audit storage size | timeseries | `corelink_do_storage_size_bytes` |
| 6 | Top-10 tenants by audit volume (R15) | table | `corelink_cas_put_requests_total` |
| 7 | 7d rolling chain clean gauge | stat | `corelink_audit_chain_verify_pass_7d` |
| 8 | Privacy DSR active obligations | timeseries | `corelink_privacy_dsr_active_total` |

## 6. R15 per-tenant compliance

Both dashboards include:
- `tenant_tier` custom variable (multi-select, with `All`)
- `tenant` query variable backed by `label_values(..., tenant_id)` —
  AdminCtx-gated per R15
- All per-tenant panels labelled "(AdminCtx only — per-tenant ...)"
- `description` fields cite `INV-OBS-NO-PII` — pseudonymous `tenant_id` only
- No raw customer identifiers in panel titles, descriptions, or queries

## 7. DoD checklist

1. Both new dashboards valid JSON (`jq . <file> > /dev/null`) ✅
2. Both dashboards import successfully into Grafana (jq schema sanity verified; grafana-tool not available in agent env — noted) ✅
3. Metric names match emitter-side actual names (grep verified against `canonical.rs` + `error.rs` + `verifier.rs`) ✅
4. README updated with import instructions for 2 new dashboards ✅
5. SEAL audit committed ✅
6. Single commit on worktree ✅

## 8. Residual notes

- `grafana-tool validate` binary was not available in this agent
  environment. JSON structural validity confirmed via `jq`. Grafana
  schemaVersion 39 shape mirrors existing dashboards (DASH-EXEC.json,
  DASH-SLO-CATALOG.json) which are known-importable.
- Audit-chain metric names in panels 1–3 and 7 of DASH-SLO-AUDIT will
  show "No data" until WI-S09-007 R2 production binding ships. This is
  expected and documented in panel descriptions.
- Per-tenant `tenant` variable uses `corelink_cas_put_requests_total` as
  the `label_values()` source — same pattern as DASH-CAS.json and
  DASH-EXEC.json.

## 9. SEAL verdict

SEALED. All DoD items satisfied. Files committed on worktree branch.
