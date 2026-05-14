---
id: "ADR-S13-005"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s13", "admin-plane", "progressive-rollout", "auto-rollback", "error-budget", "high-risk"]
---

# ADR-S13-005 — Progressive Rollout Controller 4-Stage + Auto-Rollback + Monthly Budget Cap 30%

## Status

DRAFT — pending Architect + SRE Lead + Security Lead ratification.

## Context

WI-S13-005 implements CAP-ADMIN-005 (progressive rollout orchestrator). FM-200 (bad deploy = 100% blast radius simultaneously) is a P1 failure mode for CoreLink. Without progressive rollout, every deployment touches all customers at once; a crypto-touching change (auth, audit chain, secret rotation) that regresses would be catastrophic.

Reference: Google SRE Workbook Ch 16 (canarying releases) + AWS Cell-based Architecture progressive rollout.

## Decision

### 1. 4-Stage Progression: 1% → 10% → 50% → 100%

**Why 4 stages over 3 (skip 50%):** The jump from 10% → 100% is a 10× ratio step, too coarse for detecting capacity-class bugs (e.g., DO memory exhaustion at majority load). 4-stage is validated in Google SRE Workbook + AWS Cell-based Architecture.

**Why these percentages:** 1% = minimum blast radius; 10% = ratio-scale bugs; 50% = capacity-scale bugs; 100% = terminal. Standard canary progression.

**Minimum dwell times:** 15 min / 30 min / 60 min / 0 min. Lower traffic stages have less data per minute — longer dwell needed for hi-fidelity metric collection. Total minimum: 105 min = ~1.75h. Balances safety vs deploy velocity.

### 2. Auto-Rollback: 3 Independent Triggers + 5-Min Sustained Threshold

**Trigger 1 — Error rate > baseline + 3σ:** Rolling 7-day baseline. 3σ threshold covers genuine degradation while filtering variance (99.7% of normal distribution falls within ±3σ).

**Trigger 2 — SLO burn-rate > 14.4 (1h window):** Per Google SRE Workbook Ch 16: 14.4× burn rate over 1h = depleting 1 month error budget in ~70 min. Industry canonical; not invented.

**Trigger 3 — p99 latency > baseline + 50%:** Catches latency degradation that doesn't manifest as errors (e.g., timeout retry loops, non-erroring slowdowns). Orthogonal to triggers 1 + 2.

**Why 3 independent triggers (any fires):** Single-trigger designs have blind spots. Error rate alone misses pure latency regressions. SLO burn alone misses sub-1h spikes. p99 alone misses reliability. 3-trigger coverage is orthogonal.

**5-min sustained threshold:** Eliminates false positives from transient variance (e.g., a single slow Cloudflare PoP, a brief GC pause). Production engineering standard: alerts fire on trends, not spikes.

### 3. Monthly Rollback Budget Cap 30% (Measured Burn)

**Why 30% cap:** Each auto-rollback consumes ~1–10% of monthly error budget (measured). Allowing unlimited auto-rollbacks would drain the error budget via false positives, eventually freezing legitimate deployments indirectly. 30% cap allows ~3–6 rollbacks/month.

**Why measured burn (not estimated 5%):** Lote 10.13 codex P1 fix. Fixed 5% estimate is wrong when actual error counts differ. Measured burn = `error_count_consumed / monthly_error_budget_target × 10000` basis points — accurate to actual SLO consumption.

**Rolling 30d window (not calendar month):** Calendar month reset gaming: on the 1st of each month, the slate clears and 3 immediately back-to-back rollbacks on the 1st would not be penalized. Rolling 30d window prevents this exploit.

**Manual override:** Architect + Security lead approval + ADR waiver required. Prevents operational bypass while allowing legitimate emergency overrides.

### 4. DO Singleton per Environment

**Why per-env (not global):** Concurrent rollouts to the same environment = race condition on traffic shifting (both controllers try to adjust the same CF gradual deploy percentage). Per-env allows staging + prod to run independently.

**How:** DO `idFromName("rollout-controller-{env}")` provides a singleton reference per environment. D1 `UNIQUE WHERE status='active'` provides an additional storage-layer enforcement.

### 5. Cloudflare Gradual Deploy API (Not Custom Traffic Shifting)

**Why CF native:** Cloudflare's gradual deploy is atomic per-cell at the edge — no custom load balancer, no routing layer. Integrated with Workers + Versions API. Free within Workers plan.

**Tradeoff:** Dependency on CF API availability. Mitigated by: SEV-3 alert on stage stuck > 1h; manual override via `wrangler rollback`; runbook RB-ROLLOUT-STUCK.

### 6. Cosign Signature Gate at start()

Inherited from S-12 INV-SUPPLY-SIGNED-DEPLOY. Unsigned deploy → `RolloutError::UnsignedDeploy` rejection. Prevents bad deploys that bypass the S-12 supply chain verification from entering progressive rollout at all.

## Consequences

**Positive:**
- FM-200 (bad deploy blast radius) mitigated from 100% to 1% initial exposure.
- Auto-rollback detection p99 ≤ 10 min (target: ≤ 360s at 60s probe interval + 5 × 60s sustained threshold).
- Rollback p99 ≤ 5 min (via `wrangler rollback` execution).
- False-positive flood protection via 30% monthly budget cap.
- Audit-append-only via D1 + CloudEvent emission per transition.
- SOC 2 CC8.1 (system change management) evidence produced per transition.

**Negative:**
- Minimum rollout duration: ~105 min (sum of dwell minimums). Cannot hot-fix faster without waiver (ADR waiver available per spec contract §19).
- Dependency on Cloudflare gradual deploy API availability (mitigated by RB-ROLLOUT-STUCK).
- Budget cap can freeze legitimate rollouts if 30% is consumed by genuine regressions in same month (manual override path available).

## Rejected Alternatives

| Alternative | Why rejected |
|---|---|
| Single-stage deploy (100% direct) | FM-200 blast radius unacceptable |
| 3-stage (skip 50%) | 10%→100% ratio jump too coarse for capacity bugs |
| Fixed 5% error budget per rollback | Inaccurate; real consumption varies |
| Calendar-month budget reset | Exploitable on 1st of month |
| Global singleton (not per-env) | Prevents staging + prod parallel rollouts |
| Custom traffic shifting (nginx/HAproxy) | Adds infrastructure complexity; CF native is simpler |
| Single trigger (error rate only) | Blind to latency regressions |

## References

- Google SRE Workbook Ch 16 — Canarying releases.
- AWS Cell-based Architecture — progressive rollout pattern.
- WI-S13-005 §9 (Design Decisions).
- PAT-PROGRESSIVE-ROLLOUT-001 (resilience_patterns.md §3.7).
- FM-200 (failure_modes.md).
- INV-SUPPLY-SIGNED-DEPLOY (invariant_registry.md §3.12).
