# ADR: the container's D1 transport is the hot path's largest remaining cost

- Status: **Proposed** — the decision this ADR asks for is WHICH transport, and it
  should not be taken before the one unmeasured number below is measured.
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

1. **Measure the missing term first.** `container → Worker edge` RTT from inside the
   fabric decides between A and B. If that hop costs ~40 ms — the same order as the
   `ohop` we measure in the other direction — then A saves roughly nothing (40 + 40 vs
   100 per write) and only B is worth building.
2. **Prefer B if the measurement allows it**, because it adds NO hop, batches the two
   events into one statement, and keeps the fail-closed boundary intact — it just moves
   it one layer out, to the component that already gates the response.
3. **Never A-as-SQL-proxy.** If A is chosen it is a typed event endpoint.
4. C only on an explicit owner decision.

## What this ADR does not claim

- It does not claim the container's REST transport is slow because of D1. The
  measurement isolates the transport, not the query: the same database answers the
  Worker's binding in half the time for twice the statements.
- It does not have the `container → Worker` number. That is stated as unknown here
  rather than estimated, because estimating it is precisely what would make the wrong
  option look right.
