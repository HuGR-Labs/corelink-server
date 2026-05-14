---
id: "ADR-S13-001"
title: "DO config-singleton single-instance per-region with CAS atomic update"
type: "adr"
doc_status: "SEALED"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers:
  - { role: "architect", name: "Architect (CAS semantics + audit chain integration)" }
  - { role: "security_lead", name: "Security Lead (admin role + MFA gate)" }
supersedes: null
superseded_by: null
tags: ["adr", "s13", "config-singleton", "durable-object", "cas", "admin-plane"]
---

# ADR-S13-001 — DO config-singleton single-instance per-region with CAS atomic update

## Status

ACCEPTED (WI-S13-001)

## Context

CoreLink needs a runtime config source-of-truth for:

1. Feature flags (dark launches, canary deploys).
2. Rate-limit tunables (per-layer, per-tier).
3. Retention policies (per-resource TTL).

The config must support:

- **Strong consistency**: concurrent admin updates must not lose writes
  (lost-write race condition catastrophic — blast radius global multi-tenant).
- **Low read latency**: Workers read config on hot paths (rate-limit middleware).
- **90d audit history** for SOC 2 CC8.1 compliance + rollback.
- **Edge propagation ≤ 5s p99** (stale config = blast radius global).

## Decision

Use a **Cloudflare Durable Object** (`ConfigSingletonDO`) as the
single source-of-truth per region:

1. **Single-instance per-region**: `idFromName("corelink-config-{region}")` —
   strong consistency within the Cloudflare DO model; blast radius is
   per-region (one region misconfigured does not affect other regions).

2. **CAS atomic update** via `storage.transaction()`:
   - Caller provides `expected_version: u64`.
   - DO checks `current_version == expected_version`; if mismatch → 409.
   - On match: increments `current_version`, writes new payload.
   - Makes concurrent lost-write races impossible.

3. **Propagation via Cloudflare Queue** `cfg-change-region-{region}`:
   - DO emits `{version, payload_hash}` on each successful update.
   - Per-Worker consumer refreshes in-memory snapshot.
   - Safety-net: 60s periodic poll to DO (idempotent, version-monotone).
   - SLO: ≤ 5s p99 edge global.

4. **D1 `config_change_log`**: 90d history; each row = `(version,
   payload_hash, payload_jcs, actor_user_id, actor_email_hash, mfa_ts_ms,
   created_at_ms, change_type, previous_version)`.

5. **Rollback API**: `POST /v1/admin/config/rollback?to_version=X` —
   fetches historical payload from D1, re-validates against runtime
   invariants, writes new CAS version. Recovery ≤ 5 min p99.

## Alternatives Rejected

### A. D1 single-row config

D1 is SQLite-based; row-level locking weaker than DO transactional storage
for high-concurrency CAS. Cross-region writes not natively handled. DO is
purpose-built for this access pattern (strong consistency, low latency reads).

### B. Cloudflare KV

KV is eventual-consistency by design — **unsafe** for CAS semantics (lost
writes possible). Rejected immediately for this use case.

### C. Cloudflare Pub/Sub (instead of Queue)

Newer, less mature in 2026. Queue has better delivery guarantees (at-least-once)
and simpler retry semantics. Pub/Sub deferred for future evaluation.

### D. Encrypted config payload at rest

Config payload is pseudo-public operational metadata (feature flags, rate
limits, retention policies) — not secrets. Secrets are in a separate KMS
path (WI-S13-003). DO + D1 storage is already encrypted at rest by
Cloudflare platform. Adding payload encryption adds complexity without
security benefit for this data class.

## Consequences

### Positive

- Lost-write race condition impossible (CAS + DO transaction).
- 90d audit history enables SOC 2 CC8.1 compliance.
- Rollback ≤ 5 min p99 provides recovery for blast-radius events.
- Edge propagation ≤ 5s p99 ensures fresh rate-limit + retention config.
- Schema drift prevented by `schema_version` + `serde(deny_unknown_fields)`.

### Negative / Mitigations

- DO single-instance = single point of failure per region (mitigated by
  Cloudflare DO HA and regional isolation).
- CAS retry storm possible under high concurrency (mitigated by client-side
  exponential backoff ≤ 3 retries + circuit breaker + metric alert).
- Rollback target may fail invariant check if schema evolved (mitigated by
  pre-rollback re-validation + monthly drill).

## Compliance

- SOC 2 CC8.1 (system change management) — D1 90d history.
- ISO 27001 A.5.15 (access control) — admin role + MFA freshness.
- LGPD Art. 38 — audit log pseudonymization (email_hash).
