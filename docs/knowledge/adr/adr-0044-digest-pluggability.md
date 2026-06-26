---
type: "ADR"
title: "ADR-0044 (digest) — Digest type: BLAKE3 sealed newtype, future SHA-256 path"
description: "The CAS Digest is a sealed BLAKE3-256 newtype with a constant-time compare and no public bytes constructor; SHA-256/PQ pluggability is deferred to a new type + schema migration, not an enum."
source_files:
  - "specs/03_architecture/adrs/ADR-0044-digest-pluggability.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s01", "cas", "blake3", "crypto-load-bearing", "inv-cas-integrity"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0044 (digest) — Digest type: BLAKE3 sealed newtype, future SHA-256 path

The `Digest` type is the integrity primitive on the CAS hot path: making it a sealed newtype with a
verifying-only constructor is what enforces "no body is persisted whose claimed digest does not match
its content" at compile time, across every storage adapter. This ADR is the decision record for that
type's shape and why it is a single-algorithm sealed newtype rather than a pluggable enum — the
foundation under the [native CAS surface](/surfaces/native-cas.md) and the
[CAS/AC core crate cluster](/crates/cas-ac-core.md). (Note: this is the digest ADR; two unrelated
`ADR-0044-*` files exist, disambiguated by slug.)

# Context

WI-S01-002 lands the `Digest` type plus the `VerifiedBody` envelope, which must satisfy three
load-bearing properties at once: type-driven `INV-CAS-INTEGRITY` enforcement (`VerifiedBody::new` is
the only constructor and runs a verify before returning `Ok`), constant-time compare to close the
matching-prefix timing side channel, and WASM throughput clearing the AC §8 target; hash-algorithm
pluggability is explicitly deferred to post-GA.

# Decision

The GA shape is a sealed `Digest([u8; 32])` newtype with a private field, committing to: BLAKE3-256 as
the only supported algorithm at GA; a mandatory `verify_constant_time` for any adversarial path
(derived `PartialEq` only for non-adversarial use like D1 indexing); no public bytes accessor in the
stability surface (construction only via `compute` or `from_hex`); a sealed newtype rather than a
`#[non_exhaustive] enum` so no match site has to handle a `Sha256` arm that doesn't ship; and a future
pluggability path that is documented but not implemented — a SHA-256 or PQ variant becomes a *new*
type plus a schema-versioned R2 key prefix and its own ADR, never a silent change to `Digest`.

# Consequences

`BlobStoreWrite::put_verified` is impossible to satisfy without going through the verifying
constructor, so `INV-CAS-INTEGRITY` is enforced at compile time, the R2 key format stays stable for
the GA contract, and constant-time compare is a single auditable function; the accepted cost is that a
future SHA-256/PQ variant requires a new type and a schema migration (pluggability traded for invariant
clarity), and a BLAKE3 CVE cannot be hot-swapped in place.

# Citations

1. `specs/03_architecture/adrs/ADR-0044-digest-pluggability.md:37-61` — Context: the three simultaneous load-bearing properties (type-driven integrity, constant-time compare, WASM throughput).
2. `specs/03_architecture/adrs/ADR-0044-digest-pluggability.md:72-77` — Decision: the GA shape is a sealed `Digest([u8; 32])` newtype with a private field.
3. `specs/03_architecture/adrs/ADR-0044-digest-pluggability.md:81-107` — the five commitments (BLAKE3-only, mandatory constant-time, sealed field, sealed-newtype-not-enum, documented future path).
4. `specs/03_architecture/adrs/ADR-0044-digest-pluggability.md:136-159` — Consequences: compile-time INV-CAS-INTEGRITY + stable key format vs the new-type-migration and CVE-in-place costs.
