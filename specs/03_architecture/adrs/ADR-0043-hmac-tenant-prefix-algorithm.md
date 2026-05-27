---
id: "ADR-0043"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-29"
updated: "2026-04-29"
title: "HMAC-SHA256 + b64url truncation for tenant path prefix derivation"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
deciders: ["Gustavo Schneiter (Owner)", "Security Reviewer (TBD per WI-S01-001 §30)"]
status_history: [{"date": "2026-04-29", "status": "FROZEN", "by": "Gustavo Schneiter (S-01 implementation phase, tenant-path crate)"}]
context_links:
  - "specs/04_sprints/_sealed/S01/work_items/WI-S01-001-tenant-path-hmac.md"
  - "specs/03_architecture/remote_cache_product_profile.md"
  - "specs/03_architecture/storage_semantics_matrix.md"
  - "specs/03_architecture/auth_model.md"
  - "specs/03_architecture/key_management.md"
  - "crates/tenant-path"
tags: ["adr", "s01", "tenant-isolation", "hmac", "crypto-load-bearing"]
---

# ADR-0043 — HMAC-SHA256 + base64-URL-no-pad truncation for tenant path prefix

## Context

Layer 5 of `INV-TENANT-ISOLATION` (`auth_model.md §8.1`) demands that every R2 / KV / D1 key carry a tenant-bound prefix that an attacker who controls a tenant's PAT cannot forge for another tenant. The prefix has three competing requirements:

1. **Cryptographically tenant-bound.** The only practical way to forge it must be to break the underlying primitive.
2. **Path-safe.** R2 keys appear in S3-style URLs; `?`, `#`, `%`, and `/` are reserved or special.
3. **Compact.** R2 / D1 indexing performance and human-readable error logs both benefit from short prefixes (16 chars vs the 43 chars of full HMAC b64).

Three primitive choices were considered: HMAC-SHA256 (FIPS 140-2 approved, ubiquitous), HMAC-BLAKE3 (faster but non-FIPS), and HKDF-Expand over an HMAC-SHA256 PRK (separation-of-purposes from other TDK derivations).

`remote_cache_product_profile.md §7.1` already pinned the canonical formula to `HMAC16 = b64url_no_pad(HMAC_SHA256(TDK, tenant_id_bytes))[..16]` in Lote 10.1bis cycle 1. This ADR records *why* and resolves the tension with `WI-S01-001 §9.2` (which mentioned an HKDF-Extract+Expand pipeline and 128-bit raw entropy claims).

## Decision

**HMAC-SHA256 + URL-safe base64 (no padding) + truncate to 16 ASCII chars** is canonical for the tenant path prefix.

```rust
// Implemented in crates/tenant-path/src/prefix.rs
let mac    = hmac_sha256(tdk, tenant_id.as_bytes());      // 32 raw bytes
let b64    = URL_SAFE_NO_PAD.encode(mac);                  // 43 ASCII chars
let prefix = b64[..16];                                    // 16 ASCII chars
```

The internal newtype `TenantPrefix([u8; 16])` stores the 16 ASCII bytes verbatim; `Display` renders them as a `&str`. D1 schemas using `BLOB(16)` and KV / R2 keys using textual concatenation both consume the same 16-byte buffer without re-encoding.

`tenant_id.as_bytes()` is the canonical 16-byte big-endian UUID representation, stable across `uuid` crate versions.

## Decision matrix

| Criterion | HMAC-SHA256 + b64url-trunc-16 (chosen) | HMAC-BLAKE3 | HKDF(salt=tid, info="path") + b64url-trunc-16 |
|---|---|---|---|
| FIPS 140-2 compliant primitive | ✅ Yes (SOC 2 enterprise gate per `compliance_matrix.md`) | ❌ BLAKE3 not approved | ✅ Yes (HMAC-SHA256 is the PRK construction) |
| Performance (per derive, native) | ~1 µs (single block of HMAC-SHA256 + 43-char b64 encode) | ~0.5 µs | ~2 µs (Extract + Expand = 2 HMAC passes) |
| Performance (WASM CF Workers) | ~5-10 µs measured analogues | ~3-5 µs | ~10-20 µs |
| Domain separation from other TDK uses (AC sig key, manifest sig, envelope encryption) | Implicit — the message space is `Uuid::as_bytes()`; no information leak from same-key, different-message HMAC outputs (PRF property of HMAC-SHA256) | Same as HMAC-SHA256 | Explicit via `info` label, but at a 2× cost for an isolation property already provided by HMAC's PRF guarantee |
| Audit cost (small library surface) | Minimal — `hmac` + `sha2` + `base64` are all RustCrypto crates with constant-time implementations | Same | Larger — `hkdf` crate adds one more dep with its own audit surface |
| Operational entropy of prefix (96 bits over the URL-safe base64 alphabet) | Sufficient for namespacing. Per-pair collision probability `≈ 2^-96`; birthday bound ≈ `2^48` independent tenants for 50% chance of *any* collision in the system; never relied upon for authorization (which is enforced by layers 1-4 of `auth_model.md §8.1`) | Same | Same |
| Reversibility under TDK leak | None: HMAC-SHA256 is one-way under PRF assumption | Same | Same |
| Required by canonical sources | `remote_cache_product_profile.md §7.1` + `storage_semantics_matrix.md §3` | — | — |

**Decision: HMAC-SHA256 + b64url-trunc-16.** FIPS compliance is the hard gate; the marginal speedup of BLAKE3 does not justify failing SOC 2 enterprise audits. HKDF adds a domain-separation layer for a property HMAC already provides via its PRF guarantee; the extra Extract+Expand round is pure cost.

## Consequences

### Positive

- Deterministic: same `(tdk, tenant_id)` ⇒ same prefix forever, enabling content-addressable namespacing and cron-time path reconstruction without TDK access (per `ADR-0035 H-3` materialized prefix pattern).
- Zero allocations in steady state (`encode_slice` into a `[u8; 43]` stack buffer; truncate to `[u8; 16]`). Aligns with `WI-S01-001 §14.9` sustainability quality bar.
- Cross-language reproducible: a 4-line Python reference implementation matches the Rust impl byte-for-byte. Canonical regression vectors in `crates/tenant-path/tests/prop_tenant_path.rs::canonical_vectors` are computed independently.
- ASCII output is `&str`-safe without `unsafe`: the URL-safe base64 alphabet is a strict subset of ASCII, and the `as_str` accessor's `from_utf8` is provably total.

### Negative / accepted trade-offs

- **96 bits of entropy in the displayed prefix**, not 128. `WI-S01-001 §9.3` previously claimed 128-bit raw entropy and `2^-64` collisions; both are **stale** and patched in this same Lote in `WI-S01-001 §9.3`. The correct math: per-pair collision probability `≈ 2^-96`; birthday bound `≈ 2^48` independent tenants for 50% chance of any collision; at realistic scale (`10^9 ≈ 2^30` tenants) the expected number of pairwise collisions is `≈ 2^{2·30 - 96 - 1} = 2^{-37}` — negligible. Collisions never grant authorization (which is enforced by layers 1-4 of `auth_model.md §8.1`), only namespace co-residence.
- **Algorithm migration is breaking.** Rotating to a different prefix scheme would re-key every R2 / D1 key and invalidate every materialized `tenant_prefix` BLOB(16) column. Mitigation: ADR-0036 §5 already mandates the materialized pattern, so a future swap is a one-shot batch migration rather than a runtime re-derivation.
- **No FIPS-mode hard requirement enforced at the dependency level.** The `sha2` crate provides software SHA-256 (not a FIPS-validated module). Enforcement of a hardware-backed FIPS module would require a Cloudflare-side primitive (e.g. WebCrypto in a non-WASM Worker variant) or a future vendor swap; tracked as an open item for the SOC 2 audit pass in S-20.

### Neutral

- Display format collides intentionally with `remote_cache_product_profile.md §7.1` `HMAC16` notation, so any documentation referring to "the HMAC16 prefix" maps 1:1 to `TenantPrefix::as_str()`.

## Rejected alternatives

- **Raw HMAC[:16] persisted as bytes**, displayed as hex. Rejected because (a) hex doubles the textual length (32 chars vs 16) and worsens R2 key indexing without entropy gain; (b) `remote_cache_product_profile.md §7.1` is canonical and pins the b64 form; (c) `ADR-0021 §1.1` mentions `[:16]` raw bytes for S-04 AC contexts but is tangential to S-01 — its phrasing will be tightened in a follow-up doc patch.
- **HKDF(TDK, salt=tenant_id, info="corelink/v1/path", L=16) → 16 raw bytes → b64url(no_pad)**. Rejected because the `info` separation duplicates a property HMAC-SHA256 already gives us via its PRF security; the Extract step is ~2× the cost of plain HMAC for no isolation gain; the domain-separation labels `b"path"`, `b"ac-sig"`, `b"manifest-sig"`, `b"envelope"` enumerated in `key_management.md §2.1` and `security_model.md §6.4` apply *between distinct TDK consumers*, not between two messages under the same key inside one consumer. The path-prefix derivation is a single-message-space consumer.
- **HMAC-BLAKE3-keyed**. Rejected because BLAKE3 is not FIPS 140-2 approved and SOC 2 enterprise customers have explicit FIPS requirements (`compliance_matrix.md §4`).
- **Wider truncation (e.g. 22 chars = full b64 of 16 raw bytes)**. Rejected because the canonical R2 key format already pins 16 chars; widening would invalidate every example in `remote_cache_product_profile.md §7.1` and `storage_semantics_matrix.md §3`.

## Spec follow-ups

Closed under this same Lote (codex P1 remediation):

1. ✅ `WI-S01-001 §9.3` — entropy claim corrected to 96 bits per-pair `2^-96`, birthday-bound `2^48`. Spec-contract change log bumped to v1.5.0.
2. ✅ `ADR-0021 §1.1` line 77 — tightened to "raw HMAC bytes inside `corelink-tenant-path`, b64url-truncated to 16 ASCII chars at serialization boundary; see ADR-0043".

Deferred to a downstream WI (P2, non-blocking):

3. `auth_model.md §8.1`: link Layer 5 box to ADR-0043 + `crates/tenant-path/src/lib.rs` rustdoc anchor (cosmetic forward-link, low impact).

## Compliance

- **PRINC-007** (no plaintext tenant_id in storage paths): satisfied by HMAC over `Uuid::as_bytes()`; the prefix reveals no tenant identity.
- **CTRL-AUTH-004** (HMAC tenant prefix): implemented as the single derivation function `derive_prefix` in `crates/tenant-path/src/prefix.rs`.
- **CTRL-CRED-001** (no secret leak in logs): TDK has custom `Debug = "TenantDerivationKey(REDACTED)"` impl, `ZeroizeOnDrop`, and no public `PartialEq`/`Eq`. `derive_prefix` is a pure function: it never touches the global allocator and produces no observable side effects beyond its return value.
- **INV-CONF-AT-REST**: TDK is loaded from a Cloudflare Secrets binding (KMS-backed) at Worker init; never serialized.

## Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo Schneiter | ADR criado durante implementação S-01 (WI-S01-001 ST-009 / ST-010). Resolve ambiguidade `§1` vs `§9.2` no WI-S01-001, reafirma canonical `§7.1` do `remote_cache_product_profile.md`, e enumera as três correções de spec follow-up. |
| 1.1.0 | 2026-04-29 | Gustavo Schneiter (codex P1 remediation) | Entropy math corrected (per-pair `2^-96`, birthday-bound `2^48`, NOT `2^-48`). WI-S01-001 §9.3 + ADR-0021 §1.1 L77 closed in same Lote (no longer P2 deferred). |
