# Parallel WP Dispatch Plan — remaining launch work (2026-06-26)

> **TechLead PREP artifact.** Frozen-contract work packages for parallel dispatch
> across ≥10 agents. Disjoint by owner-file (conflict-map below). Judgment
> (slicing, contracts, merge order) is pre-decided here so execution is
> *verifiable, not judged*. Baseline: `main` after #501 (tooth-audit closed).

Survey: 9 read-only explorers mapped every remaining workstream. Result: **15
agent-buildable WPs**, a clean set of **owner-blockers**, and **3 out-of-scope**
buckets (don't dispatch).

---

## Out of scope — DO NOT dispatch

- **clerk-session journeys** (tier-select, DSR-request, DSR-self-drive): the Clerk
  session bearer is NOT server-mintable (anti-fraud `needs_client_trust`). All
  three are **architecturally gated-by-design** and already covered by Worker-edge
  unit tests (onboarding/customer-bridge) + container orchestration tests. Honest
  verdict from the survey: stay gated, not coverage holes.
- **runners admit/reject**: the server already exposes `max_concurrency`/`max_vcpu_h`
  via `/internal/v1/auth/introspect` (hardened, fail-closed). Enforcement lives in
  the **corelink-runners** repo (`CoreLinkPlanStore`). Cross-repo conformance test
  belongs there — handoff already sent. Nothing to build here.
- **bazel CLI round-trip**: the `corelink bazel-init` command lives in a SEPARATE
  paused repo (`HumanGuardrail/corelink-cli`) awaiting an owner GH-org action, and
  two cold Bazel builds per run load the founder's Mac. Owner-gated + Mac-heavy.

---

## Owner-blockers (human-only — track, don't dispatch)

| # | Item | Why it needs you |
|---|------|------------------|
| OB-1 | Clerk **live** invitations | `POST /invitations` in prod needs the live Clerk secret + sends a real email to `CORELINK_E2E_TEAM_INVITE_EMAIL` |
| OB-2 | Stripe **live** whsec + live price/sub IDs | webhook→tier against prod billing; write-only secret on the Stripe dashboard |
| OB-3 | Alerting channel wiring | SendGrid/Slack creds (or ratify dashboard-only) for BYOK-revocation alerts |
| OB-4 | DPA residency decision | docs sell WEUR/SAM but signup serves {wnam,enam} — legal/Phase-2 call |
| OB-5 | `HumanGuardrail/corelink-cli` repo | GH-org creation to unpause the CLI (bazel) |

---

## Frozen contracts (the seams agents code against — DO NOT renegotiate)

```ts
// C-INVITE  (apps/signup-worker/src/lib/clerk-invitations.ts)
export async function inviteViaClerk(email: string, redirectUrl: string): Promise<{ id: string }>;

// C-ACCEPT  (apps/signup-worker/src/lib/d1.ts — consumed by webhooks/clerk.ts)
export async function acceptTeamInvitation(db: D1, clerkUserId: string, emailHash: string): Promise<boolean>; // true iff an invited row flipped to active
```

```sql
-- C-LEGALHOLD  (migrations/d1/0076_tenant_legal_hold.sql)
CREATE TABLE IF NOT EXISTS tenant_legal_hold (
    tenant_id   TEXT NOT NULL PRIMARY KEY,
    reason      TEXT NOT NULL,
    held_at_ms  BIGINT NOT NULL
);
```

```rust
// C-MOAT  (crates/corelink-adapter-host/src/brew/ports.rs)
// BottleService::fetch returns the hit signal alongside bytes (wrapper, error path unchanged):
pub struct CacheFetch { pub bytes: Vec<u8>, pub is_hit: bool }
// Brew server sets response header:  X-Cache: HIT | MISS

// C-RESOLVE  (worker/src/lib/clerk_auth.ts) — additive OR-branch, never widen the owner query:
//   owner lookup (tenant.clerk_user_id) FAILS  →  SELECT tenant_id FROM team_member
//     WHERE user_id=?1 AND status='active' LIMIT 1   (else 403 — UNCHANGED)

// C-ACCTDEL  (crates/corelink-container/src/routes/customer.rs)
//   POST /v1/customer/account/delete  →  dsr_queue.send(msg) + dsr_requested INSERT-OR-IGNORE  →  202
```

---

## Work-package table (15 — all disjoint by owner-file)

| WP | Title | Owner-files (disjoint) | Contract | Dep | Size | Model | Verified by |
|----|-------|------------------------|----------|-----|------|-------|-------------|
| **T1** | Clerk invitations wrapper | `apps/signup-worker/src/lib/clerk-invitations.ts` (NEW) | exports C-INVITE | — | S | sonnet | unit: success + 401/422 |
| **T2** | `invite()` impl (501→real) | `crates/corelink-container/src/customer_d1.rs` | consumes C-INVITE; writes `team_member status='invited'` | C-INVITE | M | opus | unit: mock Clerk + D1 row; err paths |
| **T3** | signup-worker clerk.ts webhooks | `apps/signup-worker/src/webhooks/clerk.ts`, `apps/signup-worker/src/lib/d1.ts` | impl C-ACCEPT (user.created flip) **+** legal-hold read on user.deleted | C-ACCEPT, C-LEGALHOLD | M | opus | webhook-e2e: invited→active flip; held tenant → legal_hold=true |
| **T4** | member session resolver | `worker/src/lib/clerk_auth.ts` | impl C-RESOLVE (additive fallback) | — (reads 0074) | S | opus | unit: member resolves; **owner unchanged** |
| **T5** | resolver tests | `worker/tests/customer_clerk_bridge.test.ts` | — | T4 | S | sonnet | cross-tenant denied; removed-member denied |
| **T6** | un-gate team e2e | `tests/e2e-user-journeys/src/journeys/team.rs` | 200/201 + member.status=invited | T1-T3 | S | sonnet | team journey 1 PASS (not gated) |
| **M1** | moat hit-signal | `crates/corelink-adapter-host/src/brew/{bottle.rs,ports.rs}` | C-MOAT struct | — | M | opus | unit: hit flag propagates |
| **M2** | X-Cache header | `crates/corelink-adapter-host/src/brew/server.rs` | sets `X-Cache` | C-MOAT | S | sonnet | unit: header on hit+miss |
| **M3** | moat e2e positive assert | `tests/e2e-user-journeys/src/journeys/shared_cache.rs` | asserts `X-Cache: HIT` | M2 | S | sonnet | cross-tenant HIT decisively proven |
| **D1** | account-delete route | `crates/corelink-container/src/routes/customer.rs` | impl C-ACCTDEL | — | S | opus | e2e dsr_full_flow (DSR_TEST) on throwaway tenant |
| **D2** | legal-hold migration | `migrations/d1/0076_tenant_legal_hold.sql` (NEW) | C-LEGALHOLD DDL | — | S | sonnet | migration applies; additive gate green |
| **D3** | DSR attestation signer | `crates/corelink-container/src/routes/dsr/attestation.rs` (NEW) | Ed25519 sign on VerifiedComplete | — | S | opus | unit: sign+persist; fail-open |
| **D4** | 24h DSR SLA sweep | `apps/signup-worker/src/webhooks/dsr_verify_cron.ts` | cron: requested→verified past deadline | — | M | sonnet | unit + manual internal-key drive |
| **S1** | Stripe harness 4-tier | `scripts/e2e-stripe-webhook-local.sh` | +solo/team/pro scenarios | — | S | sonnet | all 4 tiers activate; CHECK holds |
| **S2** | price→tier reverse-map test | `apps/signup-worker/tests/stripe.test.ts` | — | — | S | sonnet | mismatch detection PASS |

Plus **L1** (docs) `docs/customer/dpa-onboarding.md` — reconcile to {wnam,enam} / Phase-2 mark — haiku — *(owner-ratify OB-4)*.

---

## Conflict-map

The ONE shared-file risk was `apps/signup-worker/src/webhooks/clerk.ts` (the
invite-accept flip **and** the legal-hold read both live there). **Resolved** by
giving **T3 sole ownership of clerk.ts** (it does both edits — distinct handlers:
`user.created` flip + `user.deleted` legal-hold read). The legal-hold *migration*
is split out to D2 (its own file). Every other WP owns a unique file/dir →
**conflict-free**. Verdict: **CONFLICT-FREE** under this slicing.

## Dispatch waves (DAG; worktree-isolated; merge in this order)

- **Wave A (parallel, 10):** T1, T4, M1, D1, D2, D3, D4, S1, S2, L1 — no deps.
  Contracts C-INVITE/C-ACCEPT/C-LEGALHOLD/C-MOAT are frozen above, so the
  dependents author against the stub immediately.
- **Wave B (parallel, 4):** T2 (←C-INVITE), T3 (←C-ACCEPT + D2 DDL), M2 (←C-MOAT), T5 (←T4).
- **Wave C (parallel, 2):** T6 (←T1-T3), M3 (←M2).
- **Merge order:** migrations (D2) → container (T2,D1,D3,M1) → adapter (M2) →
  worker (T4) → signup-worker (T1,T3,D4) → e2e (T5,T6,M3,S1,S2) → docs (L1).
  Each branch SEALs (local-verify on the quiet Mac) before merge; CI green per PR.

## Risk register

- **T4 (C-RESOLVE) is the highest-risk** — auth-path, blast radius = every Clerk
  session → customer call. MUST be additive (owner query unchanged), `status='active'`
  filter mandatory, code-reviewed before merge (ADR-S33-001 calls it gated-review).
- **M1** changes `BottleService::fetch` return type → thread via the C-MOAT
  wrapper so the error path is untouched (no caller breakage).
- **T2/T3** the email→`email_hash` is SHA-256 (CTRL-PRIV-001); invite id namespacing
  verified non-cross-tenant in review.
