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

## Addendum (2026-08-26) — BYOK: the scrubber cannot re-hash raw R2 bytes

Appended rather than folded into the sections above so the existing line
citations stay valid.

Decision 2 said the scrubber enumerates R2 and re-hashes what it finds. That is
correct **only for plaintext-plan tenants**, and the exception is not an edge
case — it is a whole tenant class the design would silently mishandle.

For a BYOK-`active` tenant the stored object is **ciphertext**. The read path
decrypts to plaintext BEFORE the content-hash re-verify, and the code says why
in as many words: *"the integrity re-verify MUST run on the PLAINTEXT, never on
ciphertext"* (`crates/corelink-container/src/storage/r2_s3.rs:1307-1332`).
`resolve_byok` also returns a `physical_digest` distinct from the logical one
(`crates/corelink-container/src/storage/r2_s3.rs:711-740`), so for those tenants
the R2 key is not necessarily the plaintext digest either. A scrubber that
re-hashes raw bytes and compares against the key would therefore be wrong twice
over, and its output would be a stream of false `CorrectnessViolation`s against
tenants whose data is perfectly intact.

BYOK is **GATED-INERT** today — `resolve_byok` returns `Plaintext` unless BOTH
the `byok_config_cache` and the `tcs_resolver` collaborators are present
(`crates/corelink-container/src/storage/r2_s3.rs:722-727`), and `_public` stays
plaintext by design to preserve cross-tenant dedup. So a scrubber written today
would be correct today and become wrong the day BYOK is switched on, with no
compile error and no test failure to announce it. That is the worst shape a
latent defect can take in this codebase, and it is why this is recorded now
rather than when BYOK ships.

**Decision 4: the scrubber resolves each object's BYOK plan and, for anything
that is not `Plaintext`, SKIPS the object and counts it separately as
`skipped_encrypted` — never as verified, and never as a violation.**

The counter split is the load-bearing part. Decision 2 already requires
reporting objects EXAMINED rather than only failures found; this extends it:
`examined`, `skipped_encrypted` and `failed` must be three distinct numbers. A
scrubber that folds skips into examined would report full coverage over a store
it never checked — the same silent-success failure that decision 2 exists to
prevent, arriving through a different door.

Decrypting inside the scrubber was considered and rejected for now: it would
put tenant key material on a background sweep's path for a coverage gain that
is currently zero (no active BYOK tenants), and it deserves its own decision
with its own threat model rather than being smuggled in as an implementation
detail of a scrubber.

## Addendum 2 (2026-08-26) — correcting decision 4's seam, and a second coverage trap

Appended for the same reason as addendum 1: the line citations above stay valid.

### Decision 4 named the wrong seam

Decision 4 says the scrubber "resolves each object's BYOK plan". It cannot, and
it should not want to.

`resolve_byok` is a **private** `async fn` on `impl R2CasHandler`
(`crates/corelink-container/src/storage/r2_s3.rs:711`) and, separately, on
`impl R2AcHandler` (`:2446`). It is not a method on `R2S3Client`, which is the
type that carries `list_objects_page` and is therefore what a sweep holds. So
the call decision 4 describes does not compile from where the scrubber stands.

It is also the wrong question. `resolve_byok` maps a **logical digest to a
physical one**; the scrubber starts from a physical R2 key and has no logical
digest to feed it. Asking it per object inverts the direction of the mapping.

**Decision 4 (revised): the scrubber asks the BYOK question ONCE PER TENANT,
through the public config API, and skips an encrypting tenant WHOLE.**

`ByokConfigCache::get` (`crates/corelink-container/src/storage/byok_cas.rs:203`)
does at most one D1 read per tenant and caches the not-configured answer too;
`engagement_for` (`:1138`) is the single source of truth that the read and write
paths already share, and both it and `ByokEngagement` are `pub`. The mapping:

- `ByokEngagement::Plaintext` (or no config row) — scrub the tenant.
- `ByokEngagement::Encrypt(_)` — skip the whole tenant, add its object count to
  `skipped_encrypted`. Never `failed`: the data is intact, it is merely opaque
  to a re-hash.
- `ByokEngagement::FailClosed(_)`, or a config read error — count `failed`.
  An encrypting tenant we cannot classify must be visible, not silently skipped,
  and must never be assumed plaintext.

This is strictly cheaper than the per-object form (one config read per tenant,
not per object) and it preserves everything decision 4 was protecting: no
re-hash of ciphertext, and the three-way counter split intact.

### The tenant list is a second coverage trap

Enumeration must be per tenant, because an R2 key is
`<region>/<tenant_prefix>/<digest>` where `tenant_prefix` is a secret-keyed HMAC
of the tenant UUID — a key cannot be mapped back to a tenant. That makes the
choice of tenant list load-bearing. Measured against `corelink-config-prod`
(`d64742ea-e102-40b2-a844-ff02e3f94562`) on 2026-08-26:

| table                  | rows |
|------------------------|------|
| `tenant`               | 262  |
| `tenant_storage_state` |  74  |
| `blob_meta`            |   0  |

`tenant_storage_state` only carries tenants that have accrued byte accounting.
Driving the sweep off it omits 188 of 262 tenants and reports a clean run — the
same silent-success shape as `blob_meta`, one table over. The scrubber
enumerates `tenant`.

### `verify_content_hash` stays the single enforcement point

This ADR rests on `verify_content_hash` being *"the single enforcement point for
content-addressing on the durable path"*. It is currently private to the
`storage::r2_s3` module, so a scrubber in `routes::cas_scrub` cannot call it.
It is to be widened to `pub(crate)` and called — not reimplemented. A second,
parallel hash check would quietly falsify the claim this ADR is built on.
