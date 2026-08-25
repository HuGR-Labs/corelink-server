# ADR: the container's D1 transport is the hot path's largest remaining cost

- Status: **Proposed — the blocking number is now MEASURED** (see §"The missing term").
  The measurement decides for option A; the decision left to the owner is whether to
  take it now or fold it into the larger option B.
- Date: 2026-08-25
- Extends: `docs/design/2026-08-17-adr-edge-async-metering.md` (and its two 2026-08-25
  amendments), `docs/design/2026-08-19-adr-edge-local-do-request-metering.md`.
- Concepts: `docs/knowledge/planes/request-flow.md`, `docs/knowledge/compliance/audit-chain.md`.

## Context — what is actually left, measured from where customers are

Every number below was taken from an ephemeral box on Cloudflare Containers at
`colo=IAD` — the same fabric a paying CI job lands on — against production, tenant
`d863fafb`, via `latency-probe.yml` in corelink-runners. Measuring from São Paulo, as
every earlier record did, measures the GRU→ENAM distance instead of the product.

Tonight's sequence, same endpoint, same vantage point:

| after | total | what left the critical path |
|---|---:|---|
| baseline | ~336 ms | — |
| #1299 | ~291 ms | the D1 storage SUM the serve path was forcing on every warm read |
| #1311 | ~204 ms | the DO meter hop, now concurrent with the origin fetch |
| #1316 | **~126 ms** | the audit write, now concurrent with the R2 list |

And then the endpoint a build tool ACTUALLY hits, measured for the first time
(run 32811306666):

| path | total | `ohop` | `ostore` | unattributed |
|---|---:|---:|---:|---:|
| `GET /v1/ac/{tenant}?limit=1` (list) | ~150 ms | 41-45 | 102-119 | 0 |
| **`GET /v1/ac/{tenant}/{ref}` (lookup)** | **~290 ms** | 41-45 | **25-30** | **207-222** |

On the lookup path the R2 GET costs 25-30 ms. The ~210 ms is container work, and
`R2AcHandler::lookup` says what it is: **two serial blocking D1 audit writes** — the
`LookupAttempted` row before the storage read and the outcome row after — with
`resolve_byok` between them.

## The observation this ADR exists for

A single D1 write from the container costs **~100 ms**, and the container is in the
same region as the D1 primary. The Worker's own D1 round trip, from the same colo, in
the same minutes, measures **35-51 ms for a BATCH OF TWO statements** (`qbatch`,
before #1299 removed it from the read path).

The difference is the transport, not the database:

- the container reaches D1 through the **public REST API** —
  `https://api.cloudflare.com/client/v4/accounts/{acct}/d1/database/{db}/query`
  (`crates/corelink-container/src/storage/d1_http.rs:91`), i.e. a full public-internet
  TLS round trip to the API frontend, which then reaches D1;
- the Worker reaches the SAME database through a **binding** (`env.CONFIG_DB`), which
  is an in-network call.

So the container pays roughly **2× the Worker's cost for HALF the work**. Every D1 call
the container makes is on that transport: audit writes, the `$`-ceiling accrue, the
per-request PAT row read. Audit is simply where it hurts most, because the lookup path
does it twice, serially.

## The missing term — measured 2026-08-25, run 32813248443

From the same ephemeral box at `colo=IAD`, eight samples each, `curl -w`:

| target | wall total | TLS handshake | **after TLS** |
|---|---:|---:|---:|
| our own Worker edge (`/health` — no DO, no container, no D1) | 44-49 ms | 33-36 ms | **~9 ms** |
| the public CF API frontend the container uses for D1 today | 124-133 ms | 29-37 ms | **~95 ms** |

The TLS handshake is an artefact of `curl` opening a fresh connection per sample; the
container's `reqwest` client keeps its connections, so the number that matters is the
post-handshake round trip. **The box reaches our own edge in ~9 ms and the CF API
frontend in ~95 ms.** The ~100 ms a container D1 write costs is therefore almost
entirely the API frontend, not distance and not D1.

This is the opposite of what I would have estimated — I expected the fabric→edge hop to
cost the ~40 ms that `ohop` costs in the other direction, which would have made option A
worthless. It does not, and that is exactly why the ADR refused to guess it.

## Options

### A. Narrow, typed internal Worker endpoint (NOT an SQL proxy)
The container POSTs the audit EVENT (tenant, digest, principal, event type, timestamp)
to an internal-auth-gated Worker route; the Worker writes the row with the D1 binding.

- **Never expose SQL.** A `/_internal/d1/exec` that accepts a statement from the
  container would hand the whole control-plane database to whatever compromises a
  container — the exact blast radius the tenant-scoped design exists to prevent. The
  endpoint takes the event's FIELDS and owns the statement itself.
- Cost: `container → Worker edge` + `Worker → D1 binding`. The second term is measured
  (~35-51 ms for two statements). **The first term is the unmeasured number.**

### B. The DO writes the row (it already holds the binding)
`worker/src/durable_object.ts` already has `CONFIG_DB` (`:1314`), and the DO is already
on every request's path. The container returns its audit events as response metadata;
the DO commits them via the binding before returning the response upstream.

- Fail-closed is PRESERVED at a different boundary: bytes reach the client only after
  the DO has committed the row. "Never serve without the audit row" still holds.
- It also collapses the lookup path's TWO writes into ONE batched statement — the two
  events are both known by the time the container answers.
- Cost: one binding write on a hop that is already being paid. No new hop at all.
- Risk: the DO must not drop the row if it crashes between the container's answer and
  the commit — the same durability question the drain already answers for the
  pre-seal window, and it must be answered explicitly, not assumed.

### C. Emit one audit row per lookup instead of two
Halves the cost without touching transport, but it changes the audit EVENT TAXONOMY —
`LookupAttempted` exists so an attempt that dies before completing still leaves
evidence. That is compliance surface (`docs/knowledge/compliance/audit-chain.md`), so
it is an owner decision, not a perf refactor, and it is listed here to be REJECTED
unless the owner explicitly wants it.

### D. Do nothing
Defensible only if the ~200 ms is judged acceptable against the competitive claim the
product makes ("faster AND cheaper"). It is not, at 290 ms for a cache lookup whose
storage read costs 27.

## Recommendation

**Option A, now.** The measurement settles it: `~9 ms` to our edge plus the Worker's own
D1 binding round trip (35-51 ms for a batch of TWO statements, so a single write is at
or under that) replaces a `~100 ms` REST write. That is roughly a **50-60 ms saving per
D1 call the container makes**, and the lookup path makes two of them.

Constraints on A, non-negotiable:

- the endpoint takes the audit event's **FIELDS**, never SQL. A `/_internal/d1/exec`
  would hand the control-plane database to whatever compromises a container;
- it is gated by the existing internal-auth key, and the Worker route must match the
  handler-key convention (`/_internal/audit/*` → the audit handler) or it 401s at the
  edge and the endpoint is dead on arrival;
- it is behind a flag, with the REST transport as the fallback, and it must be proven by
  measurement in prod before the fallback is removed.

**Option B stays on the table as the next step, not the first one.** It removes even the
9 ms and batches the lookup path's two events into one statement, but it changes where
the fail-closed boundary sits, which needs the durability question answered explicitly.
A gets most of the win without touching the response path.

## What this ADR does not claim

- It does not claim the container's REST transport is slow because of D1. The
  measurement isolates the transport, not the query: the same database answers the
  Worker's binding in half the time for twice the statements.
- It does not claim the Worker's D1 binding will cost exactly what `qbatch` measured.
  35-51 ms was a batch of two statements on a warm path; a single audit INSERT should be
  at or under that, but the flagged rollout must measure it rather than assume it.
- It no longer withholds the `container → Worker` number: measured at ~9 ms after TLS
  (run 32813248443). The estimate I would have written instead — ~40 ms, by symmetry
  with `ohop` — would have been wrong by 4x and would have killed the right option.
