# Follow-up: Bazel SHA-256 CAS digest (concern D) — surface-gated + ADR-0044

> 2026-06-15 · Deferred from the `feat/followups-runners-deferred` bundle. This is the
> turnkey plan; the code was implemented + clippy-clean but EXCLUDED because it relaxes a
> CRITICAL invariant on a shared gate and needs the keyspace/ADR decision below.

## The problem (why it was excluded)

Real `bazel --remote_cache` clients content-address with **SHA-256** (REAPI v2 default), but the
CAS write path only verified BLAKE3 → every genuine Bazel upload 422'd. The branch fixed it two
ways:

1. **REAPI boundary** (`routes/bazel_v2.rs::handle_cas_write`) — verifies the client SHA-256 digest
   against the bytes before delegating to the shared handler. **This is correct + well-placed.**
2. **Shared durable gate** (`storage/r2_s3.rs::verify_content_hash`) — changed to accept a match
   under **BLAKE3 OR SHA-256**. **This is the problem:** the function is the SINGLE content-addressing
   gate for ALL surfaces (native CAS, sccache, Bazel) and it is **blanket, not surface-gated**. So a
   native-CAS/sccache client could now store a blob keyed by its SHA-256 → admitted under a digest
   that is NOT its BLAKE3, into a BLAKE3-keyed store (unreadable/un-dedupable by the BLAKE3-only read
   + dedup paths). This contradicts **ADR-0044** ("BLAKE3-only at GA; SHA-256 is a future new type
   with a schema-versioned key prefix + its own ADR").

Critically, `verify_content_hash` runs on BOTH **write** AND **read** (read re-verifies for bitrot),
so a SHA-256-keyed Bazel blob must be re-verifiable as SHA-256 on read too — which is exactly why a
clean fix needs **keyspace tagging**, not a per-call flag (the read path can't otherwise know which
function a stored blob used).

## The fix (pick one)

**Option A (cleanest — recommended):** keep the durable gate **BLAKE3-only**; store Bazel blobs in a
**surface-tagged keyspace** (e.g. `bazel/sha256/<digest>`) whose durable invariant is
`key == SHA-256(body)`. The native BLAKE3 keyspace stays pure. Larger change (keyspace + read/write
routing for REAPI).

**Option B (smaller — needs a superseding ADR):** surface-gate `verify_content_hash` so SHA-256 is
accepted ONLY for the REAPI write/read path (thread the surface through `CasReadRequest`/
`CasWriteRequest` + the `CasWriteHandler` trait), native/sccache stay strictly BLAKE3. Requires a
short **superseding ADR** amending ADR-0044 §5 to allow REAPI-scoped SHA-256 on the durable gate,
recording the keyspace-purity risk.

Do NOT ship the blanket form.

## Turnkey doc edits (from the D-review agent — apply with whichever option ships)

**`specs/03_architecture/invariant_registry.md`** (INV-CAS-INTEGRITY, ~line 84) — change the
description to: `hash_fn(body) == path.digest` where `hash_fn` is surface-determined (**BLAKE3** for
native CAS + sccache; **SHA-256** for Bazel REAPI v2, verified at the REAPI boundary
`corelink-bazel-bridge::digest::verify_sha256`); both collision-resistant; a blob is never admitted
under a digest its bytes don't produce; the function choice is NOT silent or cross-surface (native
stays BLAKE3-only per ADR-0044 §1). Also bind INV-CAS-IDEMPOTENCY (line 85) to the surface.

**TLA** (`cas_integrity.tla`, `digest_verification.tla`) — no model-logic change (the abstract
`hash_fn`/`TrueDigest` is function-agnostic; proofs hold under either collision-resistant function).
Add a comment noting `hash_fn` is the surface's canonical digest (BLAKE3 native / SHA-256 REAPI) and
the model does not mix functions within one keyspace. TLC re-checks green.

**ADR-0044** — the genuine contradiction. Either add a one-line note (Option A: "REAPI verifies
SHA-256 at its boundary; durable BLAKE3 keyspace unchanged") or write a superseding ADR (Option B).
Cite the `data_model.md:464` BLAKE3→BLAKE3-2 dual-read precedent.

**Canonical-consistency gate:** PASSES with the registry edit (the script parses IDs/severity, not
prose; no new INV ids, severity stays CRITICAL).

## Code already written (reuse from branch history)

The clean, clippy-passing implementation of `digest.rs` (SHA-256 verify), `bazel_v2.rs` (boundary
check), and the `r2_s3.rs` dual-accept lived on `feat/followups-runners-deferred` at the commit
BEFORE the concern-D revert. The boundary check + digest.rs are reusable as-is; only the r2_s3.rs
durable-gate change must be reworked per Option A/B.

## CHANGELOG (when it ships)
Bazel REAPI v2 CAS uploads no longer 422; REAPI boundary verifies SHA-256; native CAS + sccache
remain BLAKE3-only; INV-CAS-INTEGRITY updated to record the surface-scoped digest function.
