# STRIDE deep dive — `corelink-rate-limit` (rate-limit + abuse detection)

- **Crate:** `crates/corelink-rate-limit` (+ `corelink-abuse`, `corelink-customer-alerts`)
- **Date:** 2026-05-15
- **Owner:** SRE Lead + Security Lead
- **Pentest scope:** Yes — engagement 2026-06-15 (P1 surface — availability for tenants + cost-of-attack)
- **Coarse references:** matrix-stride-ctrl.csv THR-D-001..005 (DoS), THR-D-001 (per-tenant), FM-250 (DDoS volumetric), FM-255 (cryptominer abuse)
- **Pentest doc cross-ref:** §3.5 Multi-tenant isolation — D row (noisy-neighbor)
- **SOC 2 cross-ref:** CC3.2 (availability risk), A1.1/A1.2/A1.3 (availability commitments)

## 1. Trust boundaries

| Boundary | Caller | Callee | Auth/authz | Output |
|---|---|---|---|---|
| **TB-rl-1** | CF edge → Worker (any tenant request) | `corelink-rate-limit::check(tenant_id, verb)` | After PAT verify; tenant_id explicit | Allow / throttle (429) / shed (529) |
| **TB-rl-2** | DO actor holding per-tenant token-bucket state | `corelink-rate-limit` config sync | Internal binding | Atomic counter update; refill calculation |
| **TB-rl-3** | Abuse detector reading patterns | `corelink-abuse::evaluate(tenant_id, signal)` | Read-only on rate metrics + write to action queue | Action (warn / throttle / suspend / page) |
| **TB-rl-4** | Admin → rate-limit config update | `corelink-config-api` writing limits | Clerk + dual-approval for permanent changes | New limit applied within ≤ 5 min (INV-RATE-LIMIT-PROPORTIONALITY) |

## 2. STRIDE per boundary

### 2.1 TB-rl-1 (per-request check)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Spoofed tenant_id triggers bucket of victim tenant | tenant_id derived from validated PAT/JWT, not client-supplied; CTRL-AUTHZ-002 | property test + INV-TENANT-ISOLATION |
| **T** | Header tamper to bypass limit ("X-RateLimit-Override") | No client-trusted headers; limits sourced from DO state | adversarial regression |
| **R** | "We were rate-limited but never told" | 429 response includes `Retry-After`, `RateLimit-Remaining`; audit emit for sustained throttle | `crates/corelink-customer-alerts/tests/` |
| **I** | Limit leak reveals other tenants' usage (timing) | Limit check constant-time over presence; bucket state per-tenant isolated | timing benches |
| **D** | Noisy-neighbor saturates DO | INV-AVAIL-ISOLATION + PAT-BULKHEAD-001; per-tenant bucket; sharded DO per tenant cohort | k6 noisy-neighbor scenario + `crates/corelink-abuse/tests/calibration_abuse.rs` — FM-TENANT-005 |
| **E** | Bypass limit by spreading across IPs | Tenant-scoped (not IP-scoped) for authenticated traffic; IP-scoped extra for unauth surface | property test |

### 2.2 TB-rl-2 (DO state)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Rogue DO actor returns spoofed state | DO accessed via Service Binding (CTRL-NET-003); deploy-signed | INV-SUPPLY-SIGNED-DEPLOY |
| **T** | DO storage tampered post-hoc to inflate quota | DO storage quota 32 MiB (FM-059); state hashed + checked; reconcile vs D1 daily | FM-059 runbook |
| **R** | "I never exceeded the limit" | DO atomic counter exposed via diagnostic endpoint (admin-only); audit emit on suspend | INV-AUDIT-APPEND-ONLY |
| **I** | DO state read reveals usage of other tenants in shard | Per-tenant key with HMAC; DO admin API requires platform scope | RBAC review |
| **D** | DO rebalance causes latency spike | FM-005 — PAT-DEGRADE-001; client retry with backoff | FM-005 |
| **E** | DO write-path used to set arbitrary counters | Counter update via fixed verbs only (consume/refill); no setter exposed | API surface review |

### 2.3 TB-rl-3 (abuse detector)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Attacker forges abuse signal to suspend victim tenant | Abuse signals only from in-trust components (rate metrics, error rates); admin action requires dual-approval | `crates/corelink-abuse/tests/prop_abuse.rs` |
| **T** | Pattern table mutated to whitelist abuser | Calibration table in code (CTRL-SUPPLY-002); changes require signed deploy + audit | `crates/corelink-abuse/tests/calibration_abuse.rs` |
| **R** | "Tenant suspended without notice" | Pre-suspension notification (`corelink-customer-alerts`); audit chain entry; appeal path | `crates/corelink-customer-alerts/` |
| **I** | Abuse signal leaks competitive usage data to ops | Signals aggregated; raw queries off-limits; SOC 2 access review | quarterly review |
| **D** | Abuse-eval loop becomes self-DoS (eval cost > bucket value) | Eval sampled (not every request); bounded cost | benches |
| **E** | Abuse detector escalates to admin scope to "auto-mitigate" | Detector queues actions; humans/dual-approval enforce; auto-throttle only (never auto-delete) | property test |

### 2.4 TB-rl-4 (admin config)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Compromised admin session lowers limits to deny customer | CTRL-AUTH-010 + INV-ADMIN-MFA-FRESHNESS + INV-ADMIN-DUAL-APPROVAL for production change | `crates/corelink-dual-approval/tests/adversarial.rs` |
| **T** | Config payload mutated in transit | Dual-approval signs config hash; both signatures verified at apply | dual-approval test |
| **R** | "I never changed the limits" | Audit chain entry with both signer identities | INV-AUDIT-APPEND-ONLY |
| **I** | Config reveals customer plan tier | Config API tenant-scoped; cross-tenant requires platform scope | RBAC review |
| **D** | Bad config triggers cascade (limits too tight) | INV-RATE-LIMIT-PROPORTIONALITY + auto-rollback (PAT-DUAL-APPROVAL-001 + FM-201) | FM-201 |
| **E** | Admin escalates to disable rate-limiting wholesale | Disable requires platform scope + dual-approval + SEV-1 audit | property test |

## 3. Residual risks

| RR-id | Description | Severity | Mitigation status |
|---|---|---|---|
| RR-RL-01 | Edge rate-limit at CF can absorb most volumetric DDoS (FM-250) but L7 application-layer attacks reach Worker | MEDIUM | CF WAF + abuse detector; ongoing tuning |
| RR-RL-02 | Per-tenant bucket sharding granularity — extremely active tenants get dedicated shard; cohort tenants share | LOW | Documented; promotion path on noisy-neighbor signal |
| RR-RL-03 | Abuse detector false-positive risk (legitimate spike) | MEDIUM | Soft-action ladder (warn → throttle → suspend); appeal path; auto-throttle never auto-delete |

## 4. Adversarial test pointers

- `crates/corelink-rate-limit/tests/` — limit accuracy, DO state
- `crates/corelink-abuse/tests/prop_abuse.rs` — abuse signal property tests
- `crates/corelink-abuse/tests/calibration_abuse.rs` — calibration regression
- `crates/corelink-customer-alerts/tests/` — alert + appeal
- k6 noisy-neighbor + edge-flood scenarios
- `specs/_audits/2026-05-14-rb-fm-201-dry-run.md` (config rollback drill)

## 5. Cross-references

- Invariants: INV-AVAIL-ISOLATION, INV-RATE-LIMIT-PROPORTIONALITY, INV-QUOTA-ENFORCEMENT, INV-TENANT-ISOLATION, INV-ADMIN-DUAL-APPROVAL, INV-ADMIN-MFA-FRESHNESS, INV-AUDIT-APPEND-ONLY
- Controls: CTRL-RATE-001, CTRL-QUOTA-001, CTRL-BACKOFF-001, CTRL-AUTHZ-001/002, CTRL-AUTH-010, CTRL-NET-003, CTRL-SUPPLY-002
- Failure modes: FM-250 (volumetric DDoS), FM-255 (cryptominer abuse), FM-005 (DO rebalance), FM-059 (DO quota), FM-201 (config rollback)
- SOC 2: CC3.2, A1.1, A1.2, A1.3
