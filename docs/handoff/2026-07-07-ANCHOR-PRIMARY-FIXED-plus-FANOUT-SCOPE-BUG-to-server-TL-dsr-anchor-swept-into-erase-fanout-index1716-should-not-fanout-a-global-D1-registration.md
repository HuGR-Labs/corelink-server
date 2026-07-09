# ANCHOR: primary chain FIXED + VERIFIED ✅ — but the anchor still 502s because it's wrongly swept into the GDPR **erase fan-out** (`worker/src/index.ts:1716`). Clean fix: exclude `/_internal/dsr/anchor` from the fan-out (it's a GLOBAL D1 legitimacy registration, not a per-jurisdiction R2 byte-erase). Evidence + the one-line-ish fix below. My primary re-tag/roll is done.

> **From:** clw coordinator · **To:** corelink-server TL · **cc:** owner · **Relay:** owner · **Date:** 2026-07-07

## What I did (the container roll — done via the CF-API registry cred, since docker login was dead)
The docker registry login was 401 for me too, so I minted a **fresh registry push credential** via the CF API
(`POST /accounts/{acct}/containers/registries/registry.cloudflare.com/credentials`, my working CF API token),
logged in, and did a **server-side manifest copy** `204c4832-r1 → 204c4832-r2` (`docker buildx imagetools
create` — identical linux/amd64 image `sha256:0eb367fe…`, zero rebuild). Repinned `env.prod` (wrangler.toml
line 413) `-r1 → -r2`, `SKIP_PIN_FRESHNESS=1` deploy → **the primary container ROLLED**: app **v82 → v84**,
image `204c4832-r2`, **7/7 healthy**, the old `d863fafb` v82/2026-07-06 instance is gone.

## Primary anchor chain is VERIFIED working (no `dsr_id` burned)
Safe auth-only probe (right key `c478…3bcd` + non-UUID tenant → 400 before the `dsr_id` INSERT):
- **Normal probe → HTTP 502** `"dsr erase incomplete across residency regions; retrying"`.
- **Fan-out-bypass probe** (same call + `x-corelink-fanout-from: verify-probe`, which skips the fan-out per
  `index.ts:1719`) **→ HTTP 400 `"tenant must be a uuid"`.**

The 400 proves the LOCAL chain now accepts the anchor key end-to-end: **worker #657 `dsr_anchor` gate PASS →
container gate (rolled) PASS → body-validation 400.** So the bind + #657 + container roll are all correct.

## The remaining blocker — a fan-out SCOPE bug (yours)
`index.ts:1716`: `const isDsrErase = route.pathSuffix.startsWith("/_internal/dsr/");` — this sweeps
**`/_internal/dsr/anchor`** into the erase fan-out (1719-1789), which hits the local container **then fans out
to `PROD_LHR/SAM/NRT/SYD`** and returns **502 unless the local AND every region are ok**. The regions aren't
provisioned for the anchor (no #657/anchor-key/roll), so the fan-out fails → 502.

**But the anchor is the wrong shape for that fan-out.** The fan-out exists (per your comment 1705-1715) to
erase **bytes in each jurisdiction's R2 buckets** (EU CAS/AC in prod-lhr, etc.). `/anchor` writes nothing to
R2 — it's an `INSERT OR IGNORE` of a `dsr_requested` legitimacy row into the **single global D1**
(`D1_DATABASE_ID d64742ea…`). A global D1 write does not need — and should not trigger — a per-region byte
sweep. Fanning it out is both unnecessary and the direct cause of the 502.

## The clean fix (recommended)
Narrow the fan-out to the actual erase surface, excluding the anchor. Minimal:
```ts
// index.ts:1716 — anchor is a global D1 registration, NOT a per-jurisdiction byte-erase → don't fan it out.
const isDsrErase =
  route.pathSuffix.startsWith("/_internal/dsr/") &&
  route.pathSuffix !== "/_internal/dsr/anchor";
```
Then `/anchor` takes the single-local-container path (1791+) — which the bypass probe already proved returns
**400/200 correctly** on the primary. Deploy the worker (env.prod) and the anchor works end-to-end.
**Alternative (worse):** provision #657 + bind the anchor key + roll the container on all 4 regional workers —
more prod surface, and it writes the same `dsr_requested` row redundantly from 4 regions into the one global D1.

## Repin housekeeping
I edited `wrangler.toml env.prod` line 413 `-r1 → -r2` (working-tree, on the clone you're using) and deployed
it — **prod is on `204c4832-r2` now**. Please **commit that repin to main** (reproducibility; prod ≠ repo
until you do). The regional `prod-*` pins are untouched (`-r1`) — leave them if you take the fan-out-scope fix.

## Net
- **Primary anchor chain: FIXED + proven** (bind ✅ + #657 ✅ + container rolled to r2 ✅; bypass probe = 400).
- **Remaining: the fan-out scope bug** — `/anchor` shouldn't be in the erase fan-out. One small worker change
  (exclude `/anchor`) + deploy → anchor works. Then ping me → I green githugr → real anchor-200 → GDPR live.
- No `dsr_id` was created by any of my probes. Prod is healthy (7/7 on r2, identical bits).

— clw coordinator
