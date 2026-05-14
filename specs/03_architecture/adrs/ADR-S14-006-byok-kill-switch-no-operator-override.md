---
id: "ADR-S14-006"
type: "adr"
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
wi: "WI-S14-006"
owner: "Gustavo Schneiter"
tags: ["adr", "byok", "kill-switch", "cmk-revocation", "inv-byok-crypto-sovereignty", "no-operator-override", "s14"]
---

# ADR-S14-001 — BYOK CMK Revocation Kill Switch: Hard-Fail ≤ 5 min + INV-BYOK-CRYPTO-SOVEREIGNTY + NO Operator Override

## Status

**ACTIVE** — ratified 2026-05-14.

## Context

CoreLink BYOK enterprise tier grants customers **crypto sovereignty**: the
ability to revoke their Customer-Managed Key (CMK) at any time and have
CoreLink immediately cease all access to their data. This is the
**enterprise BYOK feature definition**; without it, BYOK = theater.

Three design questions required explicit architecture decisions:

1. **Kill switch SLA**: how fast must revocation propagate? ≤ 5 min global
   p99 per INV-BYOK-CRYPTO-SOVEREIGNTY.
2. **Operator override**: should an operator be able to defer or bypass the
   kill switch in an emergency? Answer: **NO**.
3. **Network partition behavior**: if CoreLink cannot reach the KMS provider,
   should it assume CMK is revoked (conservative) or still accessible
   (optimistic)?

## Decision

### D-1: Kill switch SLA ≤ 5 min p99 global

Implemented via two composed hard limits:

- **KMS access check cadence**: every 60 seconds per active BYOK tenant
  (`RevocationDetector::run_loop`).
- **DEK cache TTL hard ceiling**: 300 seconds (`DekCache::new` rejects
  `ttl_seconds > 300`; `BYOKError::CacheTtlExceedsHardLimit`).

Total p99 worst case: 60s (detection) + 300s (cache TTL hard expiry) = **360s
(6 min)**. Individual SLOs:
- `SLO-BYOK-CMK-DETECT`: detection ≤ 60s.
- `SLO-BYOK-DEK-EVICT`: cache eviction ≤ 5 min hard.
- `SLO-BYOK-KILL-SWITCH-TOTAL`: customer-perceived ≤ 6 min p99.

### D-2: NO operator override — compile-time absence

There is **no function, flag, configuration key, or code path** that allows
an operator to bypass, defer, or soften the kill switch.

Enforcement layers:
1. `RevocationDetector` has no `bypass`, `disable`, `advisory_mode`, or
   `operator_override` API surface (compile-time).
2. `RevocationConfig` has no `skip_kill_switch`, `advisory_only`, or
   `soft_fail` fields.
3. Code review MUST hard-reject any PR that adds such a path.
4. This ADR documents the hardness of the decision per spec contract §19
   waiver policy: this invariant is NOT waivable.

**Why**: an operator override path would be discovered by auditors (SOC 2
CC6.1, GDPR Art. 32) and result in permanent customer trust loss. The kill
switch is the entire value proposition of BYOK enterprise tier.

### D-3: Network partition — conservative degrade (NOT optimistic ignore)

When CoreLink cannot confirm CMK accessibility (sustained `Throttled` /
`ApiError` for ≥ 3 consecutive cycles = 3 minutes):

- **Conservative**: degrade tenant to read-only. Worst case = false-positive
  customer notification (customer verifies CMK and unblocks).
- **Optimistic** (rejected): assume CMK is still accessible. Risk = silent
  revocation miss if network partition coincides with actual CMK revocation.

The conservative approach is preferred: a false-positive notification is
recoverable (customer action + 60s detection); a silent revocation miss is
not recoverable (data access post-revocation = INV-BYOK-CRYPTO-SOVEREIGNTY
violation).

### D-4: NO 5-minute countdown UI before kill switch

No "cancel window" before kill switch fires. Rationale:
- Customer explicit CMK revoke = intent.
- Countdown adds complexity + customer confusion.
- Recovery is fast: re-enable CMK → 60s detection → access restored.

### D-5: Multi-channel customer alert — redundancy over simplicity

Alert channels: dashboard + email + in-app notification + optional Slack
webhook. Return `Ok` if ≥ 1 channel succeeds (not all-or-nothing). Rationale:
email bounce + in-app notification missed = customer surprised = trust loss.

## Alternatives Rejected

| Alternative | Rejection Reason |
|---|---|
| 30s check interval | 2× KMS API cost; marginal SLA improvement (6min→5.5min) |
| 10 min check interval | Total p99 > 10 min; INV-BYOK-CRYPTO-SOVEREIGNTY violation |
| Advisory mode (soft fail) | BYOK = theater; auditors would reject (SOC 2 CC6.1) |
| Single-channel alert (email only) | Email bounce → customer unaware → trust loss |
| 5-min countdown UI | Complexity; customer confusion; recovery is fast without it |

## Consequences

- **Positive**: customer crypto sovereignty enforced; SOC 2 CC6.1 + GDPR
  Art. 17/32 + LGPD Art. 38 satisfied; enterprise trust baseline met.
- **Positive**: chaos drill weekly per-provider verifies SLA in staging
  continuously.
- **Neutral**: KMS check_access calls add ~$10/month (bounded; acceptable).
- **Negative (acceptable)**: false-positive conservative degrade on network
  partition may cause brief customer disruption; recoverable in ≤ 60s.

## Invariants Enforced

- **INV-BYOK-CRYPTO-SOVEREIGNTY** (CRITICAL): customer revokes CMK →
  cache inaccessible ≤ 5 min global; no bypass via cached DEK > 5 min.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER**: revocation audit emit in D1
  atomic batch with tenant status update.

## References

- `WI-S14-006` spec §9 (Design Decisions).
- `specs/_spec_contract.md §19` (waiver policy).
- `crates/corelink-byok/src/cache.rs` (`DekCache::new` hard TTL rejection).
- `crates/corelink-byok-revocation/src/detector.rs` (`handle_revocation`
  — no override path).
- `specs/05_runbooks/RB-BYOK-REVOKE.md`.

## Change Log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | Ratified ADR-S14-001 (WI-S14-006). |
