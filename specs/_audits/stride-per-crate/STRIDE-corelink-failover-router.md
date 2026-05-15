# STRIDE deep dive — `corelink-failover-router` (region failover routing)

- **Crate:** `crates/corelink-failover-router`
- **Date:** 2026-05-15
- **Owner:** SRE Lead + Security Lead
- **Pentest scope:** Yes — engagement 2026-06-15 (P1 surface — availability + residency)
- **Coarse references:** matrix-stride-ctrl.csv THR-D-001..005 (DoS), FM-050 (R2 region down), FM-057 (Neon failover), FM-451 (residency leak)
- **Pentest doc cross-ref:** §3.5 Multi-tenant isolation (D row) — and residency overlay (Annex N)
- **SOC 2 cross-ref:** CC3.2, A1.1/A1.2 (availability commitments), Privacy P5.1 (cross-border)

## 1. Trust boundaries

| Boundary | Caller | Callee | Auth/authz | Output |
|---|---|---|---|---|
| **TB-fr-1** | Worker handling request after tenant resolve | `failover_router::route(tenant_id, op_kind)` | tenant_id validated; `tenant.primary_region` lookup | Region binding for storage call; fail-CLOSED 451 on residency mismatch |
| **TB-fr-2** | Router → region-health probes | Probe targets per region (R2, Neon, D1 endpoints) | Service-binding identity | Per-region health state (HEALTHY / DEGRADED / FAILED) |
| **TB-fr-3** | Failover decision actuator | Storage driver per region | INV-DATA-RESIDENCY check on route output; fail-CLOSED if cross-region | Routed request executes in pinned region only |
| **TB-fr-4** | Admin → manual failover override | Config API | Clerk + dual-approval (INV-ADMIN-DUAL-APPROVAL) | Override active for bounded TTL; audit emit |

## 2. STRIDE per boundary

### 2.1 TB-fr-1 (per-request route)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | tenant_id forged to route to attacker-controlled region | tenant_id from validated PAT/JWT; `tenant.primary_region` read from D1 SoT | `crates/corelink-failover-router/tests/prop_failover.rs` |
| **T** | Region binding mutated post-decision | Route result returned by value (immutable); driver re-checks region tag matches `tenant.primary_region` | property test — INV-REGION-NO-CROSS-LEAK 30k cases ≥ 0 leaks |
| **R** | Repudiation of cross-region read | INV-DATA-RESIDENCY fail-CLOSED 451 emits CloudEvents `dev.hugr.corelink.residency.write_rejected_cross_region.v1` audit; pre+post audit on every route | `crates/corelink-failover-router/tests/prop_failover.rs` + INV-AUDIT-APPEND-ONLY |
| **I** | Region of tenant leaks via routing latency | Routing decision constant-time over presence; SLO chart aggregates regions | timing benches |
| **D** | One region's outage cascades (all traffic re-pins) | PAT-ROUTING-PINNED-001 fail-CLOSED 451 in mismatch (NUNCA passthrough silencioso); per-region degraded mode preserves tenancy; tenant chooses degraded-read vs hard-fail in plan | `specs/_audits/2026-05-14-region-outage-chaos-s14.md` — FM-050 |
| **E** | Re-routing escalates tenant to read another region's data | INV-REGION-NO-CROSS-LEAK + INV-DATA-RESIDENCY (CRITICAL — Schrems II + LGPD Art. 33 §1º) | 20k property test cases — FM-451 |

### 2.2 TB-fr-2 (health probes)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Probe target impersonated (poisoned health-state) | Probes use service binding (CTRL-NET-003) + deploy-signed targets | INV-SUPPLY-SIGNED-DEPLOY |
| **T** | Health-state table tampered to mask region down | Probe state read from DO + replicated; reconcile across two independent probers | `specs/_audits/2026-05-15-replication-audit.md` |
| **R** | "Router never knew region was down" | Probe heartbeat + dead-man switch; missed probe = SEV-2 | RB-FM-050 (region-outage) |
| **I** | Probe results leak tenant counts per region | Aggregated state only; cardinality bound (INV-OBS-CARDINALITY-BUDGET) | observability_model.md |
| **D** | Probe storm self-DoS targets | Probe cadence bounded; jitter | benches |
| **E** | Probe path used to gain read on regional resource | Probe makes only HEAD on synthetic objects; no general read | API surface review |

### 2.3 TB-fr-3 (actuator)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Storage driver spoofed to accept cross-region call | Driver compares request region tag to `tenant.primary_region` from D1; mismatch = fail-CLOSED 451 + audit | `crates/corelink-failover-router/tests/prop_failover.rs` |
| **T** | Storage tag tampered (write to wrong region) | D1 trigger `trg_blob_meta_region_match` + Worker pre-flight | INV-DATA-RESIDENCY |
| **R** | "Storage was written to wrong region" | Audit chain entry per write with region tag; cross-region attempt emits SEV-1 | INV-AUDIT-APPEND-ONLY |
| **I** | Storage error leaks region info | Sanitized error envelope (CTRL-NET-004); 451 is generic residency-block | error envelope test |
| **D** | Storage write storms in healthy region | Per-tenant + per-region rate limit | rate-limit tests |
| **E** | Driver bug causes silent cross-region write | INV-REGION-NO-CROSS-LEAK + INV-DATA-RESIDENCY CRITICAL + 20k property test + chaos drill | `specs/_audits/2026-05-14-region-outage-chaos-s14.md` |

### 2.4 TB-fr-4 (admin override)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Admin session hijacked → manual failover triggers cross-region routing | CTRL-AUTH-010 + INV-ADMIN-DUAL-APPROVAL + INV-ADMIN-MFA-FRESHNESS | dual-approval test |
| **T** | Override payload mutated (target region) | Dual-sign payload hash; both verified at apply | dual-approval test |
| **R** | "Operator triggered cross-region read" | Audit entry with both signer identities + reason | INV-AUDIT-APPEND-ONLY |
| **I** | Override exposes other tenants' region pins | Override is per-tenant or per-region; cross-tenant aggregate scope is platform-only | RBAC review |
| **D** | Bad override drops region | Auto-rollback on SLO breach (PAT-DRIFT-DETECTION-001) | FM-201 |
| **E** | Override used to disable residency check | Disable requires platform scope + dual-approval + ANPD/EDPB pre-notification path | RB-DATA-RESIDENCY-LEAK |

## 3. Residual risks

| RR-id | Description | Severity | Mitigation status |
|---|---|---|---|
| RR-FR-01 | Manual override is the most-privileged surface; mistake = residency leak | HIGH (well-controlled) | Dual-approval + bounded TTL + audit + SEV-1 alert |
| RR-FR-02 | Probe replication relies on two independent regions both healthy | LOW | Documented in resilience patterns |
| RR-FR-03 | Custom domain routing (PAT-ROUTING-PINNED-001) ties tenant to region at DNS — change requires DNS sync | LOW | Documented in onboarding |

## 4. Adversarial test pointers

- `crates/corelink-failover-router/tests/prop_failover.rs` — routing properties + residency (20k cases)
- `specs/_audits/2026-05-14-region-outage-chaos-s14.md` — region-outage chaos drill
- `specs/_audits/2026-05-15-replication-audit.md` — replication audit
- RB-DATA-RESIDENCY-LEAK + RB-FM-050 dry-runs
- `specs/03_architecture/tla+/...` (custom domain routing TLA+ S-11 WI-S11-007 SEALED)

## 5. Cross-references

- Invariants: INV-DATA-RESIDENCY (CRITICAL), INV-REGION-NO-CROSS-LEAK (CRITICAL), INV-AVAIL-ISOLATION, INV-ADMIN-DUAL-APPROVAL, INV-ADMIN-MFA-FRESHNESS, INV-AUDIT-APPEND-ONLY, INV-OBS-CARDINALITY-BUDGET
- Controls: CTRL-PRIV-031 (residency), CTRL-AUTH-010, CTRL-AUTHZ-001/002, CTRL-NET-003, CTRL-NET-004, CTRL-RATE-001, CTRL-SUPPLY-002
- Failure modes: FM-050 (R2 region down), FM-057 (Neon failover), FM-100/101 (DNS/edge), FM-451 (residency leak — P0)
- SOC 2: CC3.2, A1.1/A1.2, Privacy P5.1
