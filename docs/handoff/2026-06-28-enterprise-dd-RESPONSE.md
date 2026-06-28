# Response to the Enterprise Technical Due Diligence (2026-06-28)

**Responding to:** `docs/handoff/2026-06-28-enterprise-due-diligence.md` (audited at `c58cb208`).
**Author:** CoreLink tech lead. **Date:** 2026-06-28. **Posture:** we accept the audit as fair, accurate,
and well-calibrated. This response (a) states what's already been remediated *since* the audit commit,
(b) gives the crate-disposition strategy (the "35-40% scaffold" question), (c) lays out the build plan and
the explicit owner/external decisions, and (d) states the one thing we will do **immediately and for free**.

We agree with the headline: **a production-grade revenue core wrapped in a partly-aspirational enterprise
shell, and the risk is the gap between what the data plane does and what the enterprise/compliance/marketing
surface claims.** The remediation below closes that gap from both ends — building the load-bearing pieces,
and (the free, highest-leverage move) descoping the claims we will not back.

---

## 1. Already remediated SINCE the audit commit (`c58cb208`)

The audit was a snapshot; several Critical/High findings have been closed and shipped to prod since:

| DD finding | Status now | Evidence |
|---|---|---|
| **#4 Audit trail forgeable / unverified (HIGH, SOC2-CC7)** | **CLOSED** | The per-partition audit-chain head is now **Ed25519-signed** + verified-on-resume (tamper → fail-closed SEV-1); migration 0080. A D1-writer can no longer forge a self-consistent chain. (CF-6) |
| **#5 GDPR erasure / "signed erasure attestation" deferred (HIGH)** | **CLOSED (erasure + attestation)** | Erase-set is **complete (60/60 tenant-keyed tables) + a runtime drift-gate** so a future table can't silently escape (CF-1); and the **signed+served attestation is LIVE** — `sign_and_persist` signs the JCS payload, persists fail-closed (region-gated → R2 → D1), served at `GET /v1/public/attestation/{id}` + `/v1/public/keys/erasure/{region}.pub` (verified: enam pubkey serving in prod). (Artifact 1, mig 0079) |
| **OCI/Turbo/CAS DoS (HIGH/MED)** | **MOSTLY CLOSED** | Turbo-GET + CAS batch-read concurrency guards, OCI rate-limiter LRU bound, request-body 413 caps. **Residual:** the specific OCI blob-upload `to_bytes(usize::MAX)` (handlers.rs:491) is on the P0 list below — honest open item. |
| **#6 residency `sam` trap (HIGH)** | **GATED CLOSED** | `sam` is now non-provisionable (signup rejects it); a `sam`/LGPD tenant can't be created to land bytes in US R2. Physical per-region buckets remain a product decision (below). (CF-3) |
| **Quota default over-admit / billing-state (MED/HIGH)** | **CLOSED** | `QuotaStore::check_and_accrue` default is fail-closed; the subscription-state gate is enforced. (CF-4 + prior) |

Plus ~8 pre-existing latent compile/test breaks the flaky CI had hidden were fixed at root (handler-crate
breaks, mutation-survivors, stale prop-tests) — the workspace `--no-run` is clean.

---

## 2. The "35-40% scaffold" — disposition strategy (measured, not estimated)

We measured it: of 93 workspace crates, ~51 are reachable from the prod container + 17 from the worker.
The ~43 "not in a prod binary" are **NOT all dead** — categorized:

- **Tests (12)** — `e2e-*`, chaos-campaign. Legit; meant to be separate. Not ghost.
- **Tooling/CLIs (8)** — `corelink-cli/go/py/dt-cli/openapi/wasm`, sbom-publish, migrate. Legit deliverables.
- **Separately-deployed workers/DOs (8)** — config-do, statuspage/dt-webhook/reconcile, slack-real, etc. Own binaries.
- **Genuine scaffold (~15)** — split THREE ways, and this is the real answer to "what do we do":
  1. **True dead stubs (~300 LOC) → DELETE:** `corelink-adapters-cloud` (199), `corelink-adapters-vault` (95) — fake/XOR BYOK KMS adapters, used by nobody. Delete once BYOK is descoped.
  2. **Real features built-but-UNWIRED → WIRE (code exists, cheap to activate, closes DD gaps):** `corelink-gc` (11k LOC — physical-delete GC; activating it closes the DD's "erased bytes not reclaimed"), `corelink-telemetry` (9k LOC — observability; closes the "blind ops" deal-blocker once a metrics/PD sink is provided).
  3. **Sold enterprise features, skeleton → OWNER build-vs-descope:** transparency-log, dual-approval, dpa-acceptance, rotation-adapters, runbook-tracker, enterprise-inquiry. Build IF we sell that tier; otherwise descope + delete.
- **Genuinely used, NOT ghost:** `corelink-dsr` (6 dependents), `corelink-privacy` (7).

**On "shouldn't we wire everything?" — no.** Every wired feature is a liability surface (test/maintain/
secure/claim-accurately), and a **half-wired compliance feature is more dangerous than an unwired one** — it
gives false assurance (wiring the XOR-fake BYOK or single-bucket "multi-region" would *create* the
misrepresentation, not fix it). BYOK is additionally a **redesign**, not a hook-up (content-addressing stores
plaintext-digest keys; ciphertext breaks dedup). The principle: **wire what serves the customer we sell to
(the SMB cache), delete the dead, and for the enterprise shell, build only what we'll actually sell — and
until then, descope the claim.** Minimal surface + honest claims beats zero-dead-code vanity.

---

## 3. Build plan (engineering — we own, in priority order)

**P0 (before enterprise conversations; most before SMB launch):**
1. **Backups + restore** — wire a live scheduled D1 backup; make migration 0064 FK-safe; flip `backup-daily-verify` to non-dry-run. (DD-1)
2. **Wire `corelink-gc`** — give physical-delete GC an entrypoint so erased/over-cap bytes are reclaimed. (DD note; code exists)
3. **Wire observability** — a real PagerDuty dispatcher behind a scheduled SLO evaluator + ship logs/metrics to a backend (`corelink-telemetry` exists). **Needs from owner:** a PD routing key + a Logpush destination. (DD-2)
4. **Cap the OCI/cargo upload buffers** (`to_bytes(usize::MAX)` → bounded) + service-binding-only + per-tenant authz on `/_internal/pat/mint`. (DD notable)

**P1 (before enterprise GA):** billing-reconcile cron; salt the audit email hash; Sentry scrub; GDPR
access/portability/rectification routes + gather-export (erasure already done); wire R2 Object-Lock on the
audit objects (completes CF-6's at-rest immutability).

**P2 (hygiene):** delete the dead stubs (post-descope); prune the Wave-33/35 crate façades; reconcile the
SLSA label.

---

## 4. Decisions that are the owner's (not engineering's) — these gate the rest

1. **THE #1 action, free, biggest risk-reducer: descope the unbuilt enterprise/compliance claims** (BYOK /
   "your key" / crypto-shred, immutable-audit-as-SOC2, multi-region/residency, full DSAR, SOC2) from all
   **marketing + DPA + legal** collateral — or commit to building them. The audit calls an un-descoped claim
   the single most-likely enterprise deal-killer (material misrepresentation). Editing collateral costs zero.
2. **Product positioning:** SMB self-serve cache vs enterprise platform — this decides whether we build or
   descope the enterprise shell (BYOK redesign, physical multi-region buckets, dual-approval, …).
3. **Confirm the live Stripe webhook endpoint targets the downgrade authority** (signup-worker), not the
   grant-only container — else a lapsed tenant keeps paid access. (Stripe dashboard — we can't read it.)
4. **Bus-factor / secret-vault:** a second committer and moving the ~25 `.env.local` prod secrets into a
   vault. Org/people decision.

---

## 5. Requires an external firm (cannot be closed from code — we concur)

Live penetration test (`/_internal/*` reachability + blast radius, audit-chain forgery against the real
store, edge WAF/rate-limit, timing side-channels); SOC2 Type I/II (Schellman/Drata-class); legal counsel for
the DPA/SCCs/TIA/Schrems-II + the residency/BYOK marketing claims; real DR cold-restore + failover + load/soak
drills; and confirmation (CF secrets are write-only) that the dedicated per-consumer internal-auth keys +
Clerk Bot-Protection are provisioned in prod.

---

## 6. Net

The audit is right and we're acting on it. The compliance-integrity deal-blockers it flagged (forgeable
audit, deferred attestation, incomplete erasure) are **already closed and shipped**; the operational ones
(backups, observability) are the live P0 build; the enterprise shell is a **deliberate build-vs-descope
product decision**, and the cheapest, highest-leverage move — **descoping the claims we don't back** — is
the owner's to make today. We are not shipping a half-built compliance surface to look feature-complete; we
are making the running system and the claims about it converge.

— CoreLink tech lead
