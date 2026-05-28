# CoreLink Grafana Dashboards

All dashboards are **provider-agnostic Grafana JSON** (schemaVersion 39).
Datasource: Prometheus-compatible (VictoriaMetrics / Grafana Cloud Mimir
tenant both work). Every panel query uses canonical metric names verified
against `crates/corelink-analytics/src/canonical.rs` `RedMetricKind::as_str()`.

---

## Dashboard inventory

| File | UID | Purpose |
|------|-----|---------|
| `DASH-AC.json` | `DASH-AC` | ActionCache hot path |
| `DASH-CAS.json` | `DASH-CAS` | Content-Addressable Storage hot path |
| `DASH-COST.json` | `DASH-COST` | Cost attribution (R2 + D1 + KV + DO) |
| `DASH-DEDUP.json` | `DASH-DEDUP` | Deduplication ratio + savings |
| `DASH-EXEC.json` | `DASH-EXEC` | REAPI v2 ExecuteAction slots / queue / latency |
| `DASH-GC.json` | `DASH-GC` | Garbage collection runs + correctness |
| `DASH-GLOBAL-HEALTH.json` | `DASH-GLOBAL-HEALTH` | Global health (entry point for on-call) |
| `DASH-GLOBAL-PRODUCT.json` | `DASH-GLOBAL-PRODUCT` | Product-level metrics (WAU, cache hit ratio) |
| `DASH-MULTIPART.json` | `DASH-MULTIPART` | Multipart upload state machine |
| `DASH-ONCALL-24-7.json` | `DASH-ONCALL-24-7` | On-call 24/7 primary responder view |
| `DASH-ONCALL-FATIGUE.json` | `DASH-ONCALL-FATIGUE` | Alert fatigue tracker + MTTA p99 |
| `DASH-PRIVACY.json` | `DASH-PRIVACY` | Privacy DSR lifecycle + GDPR obligations |
| `DASH-RATE.json` | `DASH-RATE` | Rate-limit reject rate per tier |
| `DASH-SECURITY.json` | `DASH-SECURITY` | Security events + BYOK boundary |
| `DASH-SLO-API.json` | `DASH-SLO-API` | **API request SLO — p50/p95/p99 latency, error rate, per-tenant top-10** |
| `DASH-SLO-AUDIT.json` | `DASH-SLO-AUDIT` | **Audit-chain SLO — emit latency, chain integrity cadence, storage growth** |
| `DASH-SLO-CATALOG.json` | `DASH-SLO-CATALOG` | SLO catalog — multi-burn-rate alerts + error budget |
| `DASH-SUPPLY-CHAIN.json` | `DASH-SUPPLY-CHAIN` | Supply-chain / SBOM freshness |
| `DASH-TENANT.json` | `DASH-TENANT` | Per-tenant usage + quota |

---

## Import instructions

### Grafana UI (single dashboard)

1. Open your Grafana instance (`https://<grafana-host>/`).
2. Navigate to **Dashboards → Import**.
3. Click **Upload JSON file** and select the file from this directory.
4. On the import screen, map the `Prometheus` input to your Prometheus /
   VictoriaMetrics / Mimir datasource.
5. Click **Import**.

### Grafana UI — import all dashboards at once

```bash
# Requires grafana-cli or the HTTP API (Grafana 10+)
GRAFANA_URL=https://your-grafana-host
GRAFANA_TOKEN=<service-account-token>

for f in dashboards/grafana/DASH-*.json; do
  echo "Importing $f …"
  curl -sS -X POST \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${GRAFANA_TOKEN}" \
    "${GRAFANA_URL}/api/dashboards/import" \
    --data-binary "{\"dashboard\": $(cat "$f"), \"overwrite\": true, \"folderId\": 0}" \
    | jq -r '.status + " → " + .slug'
done
```

### Grafana provisioning (recommended for prod)

Place the JSON files in your Grafana provisioning directory and add a
datasource + dashboard provisioning YAML:

```yaml
# /etc/grafana/provisioning/dashboards/corelink.yaml
apiVersion: 1
providers:
  - name: corelink
    orgId: 1
    type: file
    disableDeletion: false
    updateIntervalSeconds: 60
    allowUiUpdates: false
    options:
      path: /var/lib/grafana/dashboards/corelink
      foldersFromFilesStructure: false
```

Then copy the JSON files to `/var/lib/grafana/dashboards/corelink/` and
restart Grafana (or wait for the 60 s poll).

---

## DASH-SLO-API — import notes

**File:** `dashboards/grafana/DASH-SLO-API.json`
**UID:** `DASH-SLO-API`
**Purpose:** API request SLO for prod: p50/p95/p99 latency (CAS PUT),
error rate by endpoint, AC hit ratio per tier, rate-limit reject rate,
per-tenant top-10 talkers (R15 AdminCtx-gated), CAS GET egress bytes,
CF Worker CPU time headroom.

**Metric names used (all verified against `RedMetricKind::as_str()`):**

| Panel | Metric | `RedMetricKind` variant |
|-------|--------|-------------------------|
| 1 — CAS PUT latency p50/p95/p99 | `corelink_cas_put_duration_seconds_bucket` | `CasPutDurationSeconds` |
| 2 — CAS PUT error rate | `corelink_cas_put_requests_total` | `CasPutRequestsTotal` |
| 3 — AC hit ratio | `corelink_ac_lookup_requests_total` | `AcLookupRequestsTotal` |
| 4 — Rate-limit reject rate | `corelink_rate_limit_rejects_total` | `RateLimitRejectsTotal` |
| 5 — Top-10 tenants by CAS PUT | `corelink_cas_put_requests_total` | `CasPutRequestsTotal` |
| 6 — Top-10 tenants AC lookup | `corelink_ac_lookup_requests_total` | `AcLookupRequestsTotal` |
| 7 — CAS GET egress bytes | `corelink_cas_get_bytes_total` | `CasGetBytesTotal` |
| 8 — CF CPU time | `corelink_cf_cpu_time_us` | `CfCpuTimeUs` |

**Template variables:** `tenant_tier`, `tenant` (query-type, AdminCtx),
`region` (sam/iad/lhr/nrt/syd).

**SLO targets:**
- CAS PUT p99 ≤ 500 ms (red at 500 ms, yellow at 250 ms)
- CAS PUT error rate < 0.1% (yellow at 0.1%, red at 1%)
- AC hit ratio ≥ 80% for Business/Enterprise (green at 80%)
- Enterprise rate-limit rejects < 0.01%

---

## DASH-SLO-AUDIT — import notes

**File:** `dashboards/grafana/DASH-SLO-AUDIT.json`
**UID:** `DASH-SLO-AUDIT`
**Purpose:** Audit-chain SLO for prod: chain break SEV-0 counter, emit
failure rate, daily verifier cadence (ok vs break), R2 storage ops growth
rate, DO audit storage size, per-tenant top-10 audit volume (R15), 7d
rolling chain clean gauge, Privacy DSR active obligations.

**Metric names used:**

| Panel | Metric | Source |
|-------|--------|--------|
| 1 — Chain break SEV-0 | `corelink_audit_chain_break_detected_total` | WI §6.1.10 + `error.rs:66` |
| 2 — Emit failure rate | `corelink_audit_emit_failures_total{reason}` | WI §6.1.10 + `error.rs:34` + `sink.rs:245` |
| 3 — Verifier cadence | `corelink_audit_chain_verify_runs_total{result}` | WI §6.1 SLA + `verifier.rs:184` |
| 4 — R2 ops per bucket | `corelink_r2_ops_total` | `RedMetricKind::R2OpsTotal` (`canonical.rs:111`) |
| 5 — DO storage size | `corelink_do_storage_size_bytes` | `RedMetricKind::DoStorageSizeBytes` |
| 6 — Top-10 tenants | `corelink_cas_put_requests_total` | `RedMetricKind::CasPutRequestsTotal` |
| 7 — 7d rolling clean | `corelink_audit_chain_verify_pass_7d` | DASH-SLO-CATALOG panel 15 cross-link |
| 8 — Privacy DSR active | `corelink_privacy_dsr_active_total` | `RedMetricKind::PrivacyDsrActiveTotal` |

**Template variables:** `tenant_tier`, `tenant` (query-type, AdminCtx),
`region`.

**SLO targets / alert thresholds:**
- Chain breaks: MUST = 0 at all times (SEV-0, RB-AUDIT-CHAIN-001)
- Emit failure rate: < 0.01% sustained (SEV-1 at > 0.01%)
- Verifier cadence: 1 ok run/day/tenant; any `result=break` = SEV-0
- DO storage: green < 512 MiB, yellow < 900 MiB, red ≥ 900 MiB per class

---

## Cardinality budget

The cardinality budget counter is tracked per-dashboard in the
`annotations.cardinality_budget_used` field. Total across all dashboards
must remain below the 100 000-series limit per Grafana Cloud Mimir tenant
(Lote 10.9bis P0-I discipline).

| Dashboard | Budget used |
|-----------|-------------|
| DASH-SLO-API | 8 200 / 100 000 |
| DASH-SLO-AUDIT | 6 800 / 100 000 |

---

## Pilot-tenant provisioning

See `dash-pilot-tenants.yml` for the pilot-tenant variable override
provisioning (used for isolated per-tenant Grafana org slices).

---

## Metric name provenance

All metric names in these dashboards trace to `RedMetricKind::as_str()`
in `crates/corelink-analytics/src/canonical.rs` (15 canonical RED+USE
variants). Audit-chain-specific metric names
(`corelink_audit_chain_break_detected_total`,
`corelink_audit_emit_failures_total`,
`corelink_audit_chain_verify_runs_total`,
`corelink_audit_chain_verify_pass_7d`) are specified in
`crates/corelink-audit-chain/src/error.rs` + `verifier.rs` WI §6.1.10
and will be emitted by the production R2 binding wiring (landing at
WI-S09-007 PRR ship gate). No metric names are invented; all are
code-referenced or spec-cited.
