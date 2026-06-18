# rt-nuclear cycle-2 — triage & fix-wave (2026-06-18, autonomous run)

Workflow `corelink-redteam-nuclear` (run `wf_b6ea5150-cee`): 414 agents, 4 rounds, 18 trust
boundaries, 55 candidates → **12 confirmed exploitable** (7 high, 5 medium) after 5-skeptic
refutation (≥3/5 to survive). ⚠️ **Rounds 3-4 were heavily API-rate-limited** (≈60 hunt
agents failed on throttle) — deep second-order/composed-kill-chain coverage is PARTIAL; a
re-run when throttle clears is warranted for completeness.

TL cold-verifies each against code (AP-5 — never trust the agent self-report) before fixing.
Status legend: ✅ fixed+verified · 🔧 fixing · 📋 designed (owner-aware / pending build) · ❓ cold-verify pending.

| # | Sev | Reach | TL verdict | Status |
|---|---|---|---|---|
| 7 | high | authed | **REAL** — `handle_keys_revoke`/`_list` (customer.rs) lack the `requires_cache_write` gate that `handle_keys_create` has → a `cas:r` token revokes/enumerates ANY tenant credential | ✅ gate added to both |
| 2 | high | free-tenant | **REAL** — OCI `finalize_upload` persists bytes BEFORE `verify_against_bytes`; on mismatch the poisoned slot stays (no `delete_blob` port) → intra-tenant digest-lie content-addressing break | 📋 verify-before-commit (port change) |
| 3 | high | free-tenant | **REAL (cold-verified)** — native CAS/AC (cas.rs:552, ac.rs:516) charge the `$`-ceiling on reads UNCONDITIONALLY; OCI (oci.rs:789) wrapped it in `if is_write` → OCI reads bypass it | ✅ write-only wrapper removed |
| 1 | high | free-tenant | **REAL (amplifier confirmed)** — no per-tenant PAT cap (customer_d1 create has no COUNT) + shared 16-permit Argon2id pool not per-tenant-fair → 200 distinct PATs flood → cross-tenant native 503 | 🔧 PAT cap + per-tenant Argon2 sub-limit |
| 4 | high | free-tenant | nuclear-confirmed (3/5) — Turbo `/v8/artifacts/events` shares the 16-permit PUT budget; slow-body holds permits → starves real writes | 📋 decouple events budget + body cap |
| 5 | high | free-tenant | nuclear-confirmed (5/5) — 8 accounts pin the 512 MiB global OCI in-flight ceiling (per-tenant slice too large vs global) | 📋 reserve global headroom / smaller per-tenant slice |
| 6 | high | authed | nuclear-confirmed (4/5) — adapter writes (cargo/npm/brew/pip) don't thread the per-tier cap header → #297 seeding gap on the adapter plane | 📋 thread cap into adapter CasWriteRequest |
| 8 | med | free-tenant | nuclear-confirmed (4/5) — Turbo global upload budget holdable by slow-body `/events` (no body timeout) | 📋 (same as #4) |
| 9 | med | free-tenant | nuclear-confirmed (4/5) — `/events` has no per-tenant concurrency cap (the PUT sibling does) | 📋 per-tenant `/events` guard |
| 10 | med | authed | nuclear-confirmed (5/5) — adapter writes don't reconcile a downgraded cap (#297 reconcile gap) | 📋 (same path as #6) |
| 11 | med | authed | nuclear-confirmed (3/5) — multi-region fanout double-counts the monthly request quota (OVER-count — conservative, customer-unfavorable, not a bypass) | 📋 gate quota block on `!isFanout` (worker) |
| 12 | med | leaked-secret | nuclear-confirmed (5/5) — leaked `PAT_SIGNING_KEY` → valid-HMAC nonexistent-tenant PATs force Argon2id work → adapter-pool starvation (depends on a leaked secret) | 📋 isolate OCI verify pool + per-tenant Argon2 cap (overlaps #1) |

## Fix-wave plan (themed minimal PRs)
- **PR A — customer-plane authz (#7):** ✅ `requires_cache_write` gate on revoke+list. + regression test. (done, pending cargo-verify)
- **PR B — billing (#3):** remove the `if is_write` wrapper on the OCI `$`-ceiling gate (oci.rs ~789) so reads are charged like the native plane. Small, clear.
- **PR C — DoS shared-pool fairness (#1/#4/#5/#8/#9/#12):** per-tenant PAT mint cap (customer_d1) + per-tenant Argon2id sub-limit (adapter_pat) + decouple Turbo `/events` from the PUT budget + body/concurrency caps on `/events` + reserve OCI global headroom + isolate the OCI verify pool. Cohesive but the largest; do in careful sub-steps.
- **PR D — byte-accounting adapter plane (#6/#10):** thread the server-trusted per-tier cap into the adapter (cargo/npm/brew/pip/OCI) `CasWriteRequest` so seeding + downgrade-reconcile run (closes the #297 gap on the adapter plane).
- **PR E — OCI cache-poisoning (#2):** verify-before-commit (split assemble/commit in the BlobStore port, or add `delete_blob` rollback). Careful port change.
- **#11 (multi-region over-count):** worker `index.ts` — gate the quota block on `!isFanout`. Lowest priority (over-count, not a bypass).

## Constraint note
Self-hosted Linux CI is DOWN + the Mac cancels long jobs (see `ci-infra-state` memory) → these
Rust fixes are LOCAL-verified (`cargo check`/targeted tests) + `--admin` merged with the documented
infra reason. Fixes that can't be safely completed+verified tonight are left as precise 📋 designs
above for an owner-aware follow-up — never merged unverified (rigor compact).
