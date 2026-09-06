# D-1 — Audit trail: does the compliance artifact exist, and can an outsider verify it?

**Method.** Operated against production D1 (`d64742ea`) and the Cloudflare API, not
against the code. Every query that could return zero carries a **control query
alongside it** that is known to return rows, so "found nothing" is distinguishable
from "my instrument broke".

## Repair boundary (2026-09-05)

The historical 200-row path is now identified in source: the container's
`build_state_from_env` defaults `AUDIT_DRAIN_BATCH_LIMIT` to 200, the production
environment vars previously omitted that forwarded setting, and the hourly
signup-worker `runAuditDrainSweep` is the caller. The code/config repair
declares a bounded 512-row budget in all five production environment blocks and
writes seals in ordered 32-row JSON1 chunks; the caller remains bounded at ten
calls and ten minutes.
Lease skips and head drift are treated as backpressure, not retry progress. The
signed head CAS, sealed-tail resume, and fail-closed verifier are unchanged.

This section records repository state only. No production query was run or
retained for the repair, so B-125 remains **open** pending an authorized
read-only rerun with retained evidence.

---

## 1. Is the chain head actually signed in production? — YES

| query | control | result |
|---|---|---|
| `audit_chain_head` rows with a non-empty `head_signature` | `SELECT COUNT(*) FROM audit_outbox` → **77 935** | **365 heads, 365 signed** |

CF-6 is live. Every per-tenant chain head carries an Ed25519 signature.

## 2. Where does the signing seed live? — a write-only Cloudflare secret

I previously asserted, **from memory and without measuring**, that
`AUDIT_CHAIN_SIGNING_SEED_HEX` sat in plaintext in `wrangler.toml`. That is
**false** and is retired here.

| query | control | result |
|---|---|---|
| name in `wrangler.toml` on `origin/main` | `EDGE_FIND_MISSING` → **5** | **0** |
| name in `corelink-prod` CF secrets listing | `CORELINK_AUDIT_ATTEMPTED_AUTH_KEY` → **1**, 53 names total | **0** |
| `ERASURE_ATTESTATION_SEED_HEX` in that listing | same, 53 names | **PRESENT** |

The dedicated slot is simply unset, and the drain falls back to the
erasure-attestation seed by design (`routes/audit_drain.rs::resolve_seed`, which
treats the DO's `?? ""` forward as ABSENT precisely so the empty dedicated slot
cannot mask the reused one). Cloudflare secrets are write-only — the API lists
names and never values — so "deployed as a secret" is proven by the name's
presence in the secrets listing, and "deployed as a var" by its presence in
`wrangler.toml`. Two different queries; both were run, both with controls.

**Verdict: key handling is correct.** No plaintext seed in the repo.

## 3. How long after an event does its evidence become verifiable? — HOURS

This is the finding.

| measure | value |
|---|---|
| rows sealed per hour, six consecutive hours | **200, 200, 200, 200, 200, 200** |
| seal latency over the last 24 h (n=2692) | min **12 s**, mean **1 h 28 min**, max **5 h 13 min** |
| unsealed backlog | **4 243** of 77 935 |
| age of the oldest unsealed row | **6.8 h** |

Exactly 200 per hour for six hours running was a **hard cap**, not a load curve —
a load-shaped series does not land on the same integer six times.

Arrivals over the same window were **86, 300, 2, 30, 88, 92** (mean ≈ 100/h), so
the drain currently outpaces the mean and the backlog shrinks slowly. **That is
the whole safety margin: a factor of two against a bursty arrival process whose
observed peak (300/h) already exceeds the ceiling.** Any sustained burst above
200/h accumulates, and the backlog only clears at 200/h afterwards.

**Historical customer impact:** an event was not tamper-evident when it happened. It
becomes so on average **~1.5 hours later**, and in the observed tail **over 5
hours** later. Anything read from `audit_outbox` inside that window is unsealed —
present, but outside the chain.

## 4. What this dossier does NOT decide

- **Whether the repair closes the production latency/throughput threshold.** The
  source path and configuration default are identified now, but this dossier
  contains no post-repair production measurement. The historical values above
  remain evidence of the pre-repair state only.
- **Whether an outsider can verify the chain end-to-end offline.** Signature
  presence is proven; the customer-facing verification path is not exercised here.
- **Whether the sealed segment is internally consistent.** Not recomputed.

## 5. Bearing on the promise

The product promises tamper-evidence. In the historical pre-repair window,
that promise held **for rows older than a few hours** and did not hold for
recent activity. A compliance artifact that lags the event by 1.5 h on average
is not the same product as one that lags by seconds; whether the repair closes
that gap remains unmeasured.

Filed as **B-125**.
