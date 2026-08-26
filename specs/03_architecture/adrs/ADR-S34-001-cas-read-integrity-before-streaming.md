---
id: "ADR-S34-001"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-26"
updated: "2026-08-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "cas", "integrity", "streaming", "scrubber", "availability", "read-path"]
references:
  - "crates/corelink-container/src/storage/r2_s3.rs"
  - "crates/corelink-container/src/routes/cas.rs"
  - "crates/corelink-container/src/routes/turbo_v8.rs"
---

# ADR-S34-001 — CAS read integrity: the scrubber precedes streaming, and the read-path memory bound is a separate concern

- **Status:** Proposed (2026-08-26)
- **Deciders:** CoreLink tech lead (architecture delegated by the owner/stakeholder)
- **Context tags:** cas, integrity, availability, read-path

## Context

Two changes were proposed against the CAS read path and were being treated as
independent:

- **S2 — stream CAS reads** instead of buffering the whole object, to cut
  container memory and time-to-first-byte.
- **S1 — a background integrity scrubber** that verifies objects at rest.

They are not independent, and the reason only becomes visible one layer below
the route.

### The read path re-verifies the content hash today

`R2CasHandler::read` re-hashes the bytes R2 returned and compares them against
the requested digest before serving
(`crates/corelink-container/src/storage/r2_s3.rs:1333`, calling
`verify_content_hash` at `:1103`). A mismatch emits an
`AuditEventKind::CorrectnessViolation` and is never served as a hit.

The comment on the check states its purpose precisely: it catches **R2 bitrot,
storage-tier tampering, or a historically mis-keyed blob** — failure modes
below our own API, which no amount of write-path validation can catch, because
they occur after the write succeeded.

`verify_content_hash`'s doc-comment calls itself *"the single enforcement point
for content-addressing on the durable path"* and names the surfaces that funnel
through it: the native `PUT/GET /v1/cas/...` route, the Bazel REAPI v2 bridge,
and sccache.

This is easy to miss, and was missed during the first pass of this analysis:
the CAS **route** (`crates/corelink-container/src/routes/cas.rs`) contains only
`is_canonical_digest`, which validates the *shape* of the digest (64 hex
characters) and says nothing about content. Grepping the route layer produces
the confident and wrong conclusion that reads are unverified.

### There is no at-rest integrity coverage

No background job, cron, or sweep reads stored objects to check them. A search
for `scrub`, `verify`, `integrity`, `fsck`, and sweep-shaped work across
`crates/corelink-container/src/routes/` finds nothing that reads object bytes to
validate them. The `audit_export` surface verifies inclusion proofs over the
audit chain, not stored CAS content.

So the *entire* integrity guarantee for stored CAS content is the read-path
check — and it is a sampling function driven by traffic: an object is verified
exactly when, and only when, a client asks for it. Cold objects are never
checked at all.

### What that means for streaming

Streaming cannot preserve this check. Verification requires the whole object;
by the time the digest can be computed, the bytes have already been written to
the client. Streaming does not weaken the check — it **removes** it, and with
it the only bitrot detection that exists anywhere in the system.

## Decision

**1. S1 (scrubber) is a hard prerequisite for S2 (streaming). They ship in that
order, and S2 does not merge until at-rest coverage is live and observable.**

Doing S2 first would take the system from "every served object is verified" to
"no object is ever verified", with no interval of overlap. That is not an
optimisation with a documented trade-off; it is the silent removal of a control.

**2. The scrubber enumerates R2 directly. It must not depend on `blob_meta`.**

`blob_meta` is **empty in production** and no code inserts into it — the table
exists with a correct schema that nothing maintains. A scrubber keyed on it
would enumerate zero objects, verify nothing, and report success. That is the
repo's characteristic failure mode (silent success), and here it would produce
a green integrity dashboard over an unverified store.

The enumeration primitive already exists:
`R2S3Client::list_objects_page(prefix, max_keys, cursor)`
(`crates/corelink-container/src/storage/r2_s3.rs:467`) returns
`(key, size, etag)` triples with a continuation cursor and a `max_keys` clamp of
1000 — cursor-driven, resumable, and bounded per call, which is what a
long-running sweep needs.

The scrubber must report **objects examined**, not just failures found. A run
that examined zero objects and found zero failures must be distinguishable from
a healthy run, or the first defect reappears in a different shape.

**3. The read-path memory bound is a SEPARATE concern from integrity, and it is
not an argument for streaming.**

The motivation originally attached to S2 — bounding heap on the read path — has
an existing answer in this codebase that costs no integrity at all: reserve a
concurrency permit *before* the body is buffered.

| Surface | Per-tenant cap | Process-wide cap |
|---|---|---|
| Turbo `GET /v8/artifacts/:hash` | `TURBO_GET_CONCURRENCY_LIMIT` = 4 | `GLOBAL_TURBO_GET_BUDGET` |
| Turbo `PUT /v8/artifacts/:hash` | `TURBO_PUT_CONCURRENCY_LIMIT` = 4 | `GLOBAL_TURBO_PUT_BUDGET` |
| CAS batch read | `CAS_READ_CONCURRENCY_LIMIT` = 8 | — |
| **CAS single `GET /v1/cas/:tenant/:hash`** | **none** | **none** |

`handle_read` (`crates/corelink-container/src/routes/cas.rs:773`) takes
`State`, `Path`, `auth`, `scope`, `headers` — and no guard. Its sibling
`handle_batch_read` declares `CasReadConcurrencyGuard` **ahead of** `body`
precisely so axum reserves the slot before buffering. The single-object read was
left out.

Turbo's own comment describes this exact shape as a bug that was already fixed
once there: *"the read path was left asymmetrically open while the PUT path was
hardened"* (`crates/corelink-container/src/routes/turbo_v8.rs:119`).

Closing that gap is a small, local change that follows an established pattern in
this repo. It should be evaluated on its own, and if it removes the memory
pressure that motivated S2, then S2 must justify itself on time-to-first-byte
alone — a much narrower claim.

## Consequences

- S2 is blocked on S1. This is a real schedule cost and it is deliberate.
- The scrubber's coverage metric is load-bearing, not decorative: without an
  "objects examined" counter, a scrubber that silently enumerates nothing is
  indistinguishable from a healthy one.
- Streaming, when it does land, must state in its own ADR what replaced the
  read-path check for the objects it serves, and accept that per-object
  verification latency moves from "on every read" to "whenever the scrubber last
  reached this key".
- The missing guard on `handle_read` is tracked separately. Its severity depends
  on container sharding: routing is `env.CORELINK_SERVER.idFromName(tenantId)`
  (`worker/src/index.ts:88`), i.e. one Durable Object — and therefore one
  container — per tenant, which makes an unbounded concurrent-read burst a
  self-inflicted denial of service rather than a cross-tenant one. That
  reading is not yet proven and does not change the fix, only its priority.

## Alternatives considered

**Ship S2 first, add the scrubber later.** Rejected. It creates an interval
with zero integrity coverage, and "later" is exactly when a deferred control
does not arrive.

**Verify on read while streaming, and fail after the fact.** Rejected. The
bytes have already reached the client; a trailer or a post-hoc audit event
cannot un-serve corrupted content, and a client that has already written those
bytes into its own cache has propagated the corruption.

**Drive the scrubber from `blob_meta`.** Rejected — the table is empty in
production and unmaintained by any code path. See decision 2.

**Treat the missing `handle_read` guard as part of S2.** Rejected. Bundling a
small, well-patterned availability fix behind a blocked integrity redesign
delays the fix for no benefit. They are separate changes with separate risk.
