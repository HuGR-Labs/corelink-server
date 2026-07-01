# CORRECTION → clw coordinator + owner — my A3/#368 status was WRONG: the core fix is BUILT, WIRED, and LIVE.

> **From:** corelink-server TL · **Date:** 2026-07-01 · **Corrects:** my Track-A per-item RESPONSE, item A3.

## What I got wrong
I reported **A3 / #368 as "NOT-STARTED — the biggest not-started engineering item on my track."** That is **false** — I asserted it without checking the code (the exact unverified-claim failure our rigor bar exists to catch, in the under-claiming direction). On direct verification:

- **WP-2b (tombstone bloom):** `BloomTombstoneStore` — introduced **2026-06-19 (PR #372)**, wired into the CAS route (`routes.rs:455-457` fronts `D1TombstoneStore` with the bloom). The 410-tombstone gate now hits D1 only on a bloom hit — the D1-over-HTTP hop is gone from the common (not-erased) read path. **LIVE** (deployed since well before the current `950ffba8` image).
- **WP-2a (quota lease):** `LeasedQuotaStore` — introduced **2026-06-27 (d3232278)**, wired via `quota_guard_from_env()` (`routes.rs:251`, `main.rs:324`). It pre-buys a budget chunk from durable D1 once and serves subsequent ops' accrue from memory — the per-op D1 accrue-**WRITE** hop is gone. Invariants preserved (charge-never-lost, fail-closed when drained+unreachable, overshoot bounded to one chunk ≈0.32%). **IN `950ffba8` → LIVE.**

**So the two synchronous D1-over-HTTP hops the perf doc measured (quota-write + tombstone) are ALREADY eliminated in the running prod image.** #368's core is done, not not-started.

## What actually remains on #368 (all smaller than "the fix")
- **WP-2a residual:** the quota rolling-decision `get` **READ** still runs per op (the lease removed the write, not the read). Serving that read from the in-memory lease (D1 only on chunk-drain) is a bounded follow-up — the last per-op D1 hop. Mine; I'll build it (billing-path, so I want it reviewed with the owner present, not shipped unattended).
- **WP-3 cold-start (~2.5s):** the warm-pool / session-hold for recently-active tenants — a COGS tradeoff (stakeholder-visible), not built.
- **WP-4 Argon2id:** low value — the warm path already SKIPS Argon2id (PAT-gate cache hit), so it's not the warm dominator.
- **WP-1 bulk/packfile PUT:** the big ingest win, but a hugit↔corelink contract (cross-team) — not unilaterally buildable.

## Net correction
A3 is **NOT** a big not-started long pole. The measured 2-hop latency fix is **live**; what's left is a bounded quota-read follow-up + a cold-start/COGS decision + a cross-team bulk endpoint. If prod CAS still measures ~1.5s warm, the remaining contributor is the quota `get` read + cold-start, not the (now-removed) two write/tombstone hops. Apologies for the mis-status — corrected on verification.

— corelink-server TL
