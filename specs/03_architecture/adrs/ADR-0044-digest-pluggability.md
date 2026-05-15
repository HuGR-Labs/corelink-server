---
id: "ADR-0044-digest"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
title: "Digest type: BLAKE3 default, sealed-newtype today, future SHA-256 path"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
deciders: ["Gustavo Schneiter (Owner)", "Crypto SME (TBD per WI-S01-002 §30)"]
status_history: [{"date": "2026-05-15", "status": "FROZEN", "by": "Gustavo Schneiter (S-01 implementation phase, corelink-hash crate)"}]
context_links:
  - "specs/04_sprints/S01/work_items/WI-S01-002-blake3-verify-at-write.md"
  - "specs/04_sprints/S01/work_items/WI-S01-001-tenant-path-hmac.md"
  - "specs/03_architecture/security_model.md"
  - "specs/03_architecture/invariant_registry.md"
  - "crates/corelink-hash"
  - "ROADMAP-TO-GA.md"
tags: ["adr", "s01", "cas", "blake3", "crypto-load-bearing", "ctrl-cas-001", "inv-cas-integrity"]
---

# ADR-0044 (digest) — `corelink-hash::Digest`: BLAKE3 default, sealed newtype at GA, documented future SHA-256 path

> **Note on numbering.** Two prior `ADR-0044-*` files exist in the corpus
> (`-deploy-gate-hard-cosign-keyless.md`, `-sbom-cyclonedx-toolchain.md`). This
> file's logical identifier is `ADR-0044 (digest)`; the slug
> `digest-pluggability` disambiguates the filename. Front-matter `id` is set
> to `ADR-0044-digest` to keep validator-level uniqueness.

## Context

WI-S01-002 lands the foundational integrity primitive for the CAS hot path:
the `Digest` type plus the `VerifiedBody` envelope (`crates/corelink-hash/`).
Three load-bearing properties must hold simultaneously:

1. **Type-driven CTRL-CAS-001 enforcement.** `INV-CAS-INTEGRITY` (CRITICAL,
   TLA+) says *no body is ever persisted whose claimed digest does not match
   its content.* That invariant is enforced architecturally by making
   `VerifiedBody` the *only* type that `BlobStoreWrite::put_verified` accepts,
   and by making `VerifiedBody::new(body, claimed)` the *only* constructor —
   one that runs a constant-time verify before returning `Ok`.
2. **Constant-time compare.** Standard byte-by-byte `==` short-circuits on the
   first mismatch and leaks the matching-prefix length through latency. The
   `subtle::ConstantTimeEq` path closes that side channel for adversarial
   verification paths.
3. **Throughput at WASM.** Cloudflare Workers cap CPU at 50 ms per request;
   the AC §8 throughput target is p99 ≤ 3.5 ms for a 5 MiB blob (≥ 1.4 GB/s in
   WASM). The native release benchmark must clear this comfortably to leave
   headroom for the WASM JIT.

WI-S01-002 §6.2 (Out-of-scope) explicitly defers *hash-algorithm
pluggability* — "BLAKE3-only at GA; SHA-256 backward-compat opcional pós-GA".
This ADR records *why* the GA shape is a single-algorithm sealed newtype
rather than a `#[non_exhaustive] enum Digest { Blake3(..), Sha256(..) }`, and
documents the migration path if a future control (BYOK CMK, FIPS-only deploy
profile, post-quantum migration) ever forces a second variant.

WI-S01-002 §9.6 ("ADR potencial?") said no ADR was needed at the time the WI
was drafted because no disruptive architectural decision was visible. The
codex/orchestrator review at SEAL time flagged that the same decision *does*
deserve a written record now that the type's shape is committed across the
crate, the storage adapter trait (`BlobStoreWrite`), the multipart Merkle WI
(S-05), and the client-side verify WI (S-02). This ADR fills that gap.

## Decision

The GA shape of `corelink-hash::Digest` is:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Digest([u8; 32]);   // sealed newtype, private field
```

with the following commitments:

1. **BLAKE3-256 is the only supported algorithm at GA.** Both constructors
   (`Digest::compute` and `Digest::from_hex`) speak BLAKE3 only; any future
   pluggability is a *new* type or constructor, never a silent semantic shift
   on the existing API.
2. **Constant-time equality is mandatory for adversarial paths.**
   `Digest::verify_constant_time(&self, other: &Self) -> bool` is the
   canonical comparison for any path where an attacker may observe latency
   (write-time verify, AC verify, client-side verify). Derived `PartialEq` is
   available for non-adversarial use (set membership, D1 indexing); rustdoc
   on each call site must declare which mode it is using.
3. **Private field, no public bytes accessor in the stability surface.**
   `Digest` cannot be constructed from arbitrary bytes outside the crate. The
   `#[doc(hidden)] pub fn as_bytes()` is a testing-only escape hatch and is
   not part of the semver-locked public API; downstream code must round-trip
   through `Digest::compute` (server-side hash) or `Digest::from_hex` (parsed
   untrusted client input).
4. **Sealed newtype rather than `#[non_exhaustive] enum`.** A single
   `[u8; 32]` field is the smallest object that satisfies INV-CAS-INTEGRITY.
   An enum form (`Blake3([u8; 32])` / `Sha256([u8; 32])`) would force every
   match site to handle a `Sha256` arm that does not exist at GA, and would
   either weaken or duplicate the BLAKE3-default invariants used by R2 keys,
   D1 indexes, and CAS dedup.
5. **Future pluggability path is documented, not implemented.** If GA+N adds
   SHA-256 (FIPS deploy profile) or a post-quantum hash, the migration is a
   *new* type `DigestV2` (or `Digest<H: HashAlgo>`) accompanied by a
   schema-versioned R2 key prefix and a separate ADR. Mixing algorithms
   inside a single `Digest` value is explicitly out of scope.

## Decision matrix

| Criterion | Sealed newtype `Digest([u8; 32])` (chosen) | `#[non_exhaustive] enum { Blake3, Sha256, … }` | Generic `Digest<H: HashAlgo>` |
|---|---|---|---|
| GA surface area | ✅ Minimal: one type, one length, one algo | ❌ Adds a `Sha256` arm we don't ship; every match site grows | ❌ Generic noise on every storage signature |
| INV-CAS-INTEGRITY enforcement | ✅ One verify path; constant-time compare is the only equality used in adversarial code | ⚠ Each variant needs its own verify; cross-variant equality is meaningless | ⚠ Type parameter leaks into REAPI / R2 adapter signatures |
| WASM perf | ✅ No vtable, no match dispatch on hot path | ❌ Match on every put_verified call | ✅ Monomorphised; equivalent to chosen |
| Future SHA-256 migration | ✅ New type + ADR + schema-versioned prefix | ⚠ Looks pluggable; in practice every consumer must reflow | ⚠ Requires bumping every downstream generic |
| Constant-time discipline | ✅ Single `verify_constant_time` impl; auditable | ⚠ Easy to forget on a new variant; per-variant timing audit |  ⚠ Per-instantiation timing audit |
| Type-system seal on construction | ✅ Private field + 2 constructors = inviolable | ⚠ Each new variant is a new constructor surface | ⚠ Generic parameter encourages "carrier" patterns |

## Constant-time requirement (security-load-bearing)

`Digest::verify_constant_time` wraps `subtle::ConstantTimeEq` and returns a
`bool` via `subtle::Choice::into()`. Three invariants apply:

1. **Branch-free.** No `if`/`?:`/`&&` short-circuit on byte values.
2. **`black_box` on benchmark probes.** The release-only test
   `constant_time_variance` (1000 partial-match attempts; zero-prefix vs
   last-byte-differs) gates relative-delta < 5%; failure blocks the merge.
3. **No leak through error type.** `HashMismatch` is a unit struct; it
   carries no digest payload that could be logged and correlated. The audit
   event `corelink.cas.poisoning_attempt` (emitted by REAPI in
   `WI-S01-005`) is the canonical sink for forensics.

## Consequences

**Positive.**

- `BlobStoreWrite::put_verified(&VerifiedBody)` is impossible to satisfy
  without going through `VerifiedBody::new` — INV-CAS-INTEGRITY enforced at
  compile time across every storage adapter.
- The sealed shape keeps the R2 key format (`<tenant_prefix>/<digest_hex>`)
  stable for the lifetime of the GA contract; backup, GC, dedup, and
  multipart Merkle (WI-S05-002) all assume a fixed 32-byte payload.
- Constant-time compare is a single auditable function rather than a
  per-variant burden.

**Negative / accepted.**

- A future SHA-256 (or PQ) variant requires a *new* type and a schema
  migration; it cannot be a transparent change to `Digest`. This is the
  intentional trade — pluggability deferred in exchange for invariant clarity.
- BLAKE3 crate compromise (CVE) cannot be patched by swapping algorithms in
  place. Mitigation: WI-S01-002 §28 R-WI-001 captures the dupla-hash
  fallback as a documented residual; ADR-supersede this ADR if/when invoked.

**Neutral.**

- Generic over `HashAlgo` is rejected for now but is the natural successor if
  the dupla-hash fallback ever ships. The path remains open.

## Compliance & invariants

- `INV-CAS-INTEGRITY` (CRITICAL, TLA+) — directly enforced by `VerifiedBody`'s
  private fields + verifying constructor.
- `INV-DIGEST-VERIFICATION` (CRITICAL, TLA+) — every adversarial digest
  comparison goes through `Digest::verify_constant_time`.
- `INV-CAS-IDEMPOTENCY` (CRITICAL) — BLAKE3 is deterministic; same body →
  same digest → same R2 key.
- FIPS 140-2 — BLAKE3 is *not* FIPS-approved. Acceptable at GA because the
  CAS path is content-addressing, not authentication; SOC 2 / ISO 27001 do
  not require FIPS for content integrity (they require it for
  authentication/encryption keys, covered separately by
  `key_management.md`). If a future enterprise deploy profile mandates FIPS,
  the SHA-256 migration path documented in §"Future pluggability" applies.

## Cross-links

- WI-S01-001 (sister; sealed) — `tenant-path` crate using the same
  type-driven private-field discipline for `TenantPrefix`. Together these
  two crates form the foundational integrity + isolation layer for S-01.
- WI-S01-003 (R2 adapter, single-blob) — first concrete implementer of
  `BlobStoreWrite::put_verified`.
- WI-S01-005 (REAPI handler) — call site of `VerifiedBody::new`; emission
  point for `corelink.cas.poisoning_attempt` audit event + RED metrics.
- WI-S02-003 (client verify, CTRL-CAS-002) — reuses `Digest::compute` +
  `verify_constant_time` for defense-in-depth.
- WI-S05-002 (multipart Merkle) — extends the single-blob shape to chunked
  blobs while preserving the sealed `Digest` newtype at the leaf level.
- ROADMAP-TO-GA.md §1 R-1 — S-01 foundation wave; this ADR is one of the R-1
  artifacts gating "Spec hygiene + abandoned review backlog + Opus eyeball
  pass".
- S-01 sprint (`specs/04_sprints/S01/sprint.md`) — parent.
- ADR-0043 — `tenant-path` sister ADR documenting the parallel HMAC-SHA256
  decision; this ADR mirrors that one for the digest side of the same
  type-driven security pattern.

## Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7) | Initial — records WI-S01-002 SEAL-time decision: BLAKE3-only sealed-newtype `Digest`, constant-time compare mandatory for adversarial paths, future SHA-256 path is a new type + schema migration. Sister to ADR-0043 for `tenant-path`. |
