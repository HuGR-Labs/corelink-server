---
id: "ASVS-S05-V5-V6-V8-V10-V14"
type: "compliance_matrix"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-05-01"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["compliance", "asvs", "owasp", "s05", "multipart", "checklist"]
---

# OWASP ASVS v4.0.3 — V5 / V6 / V8 / V10 / V14 self-checklist (S-05)

> **Sprint:** S-05 (Multipart CAS + Chunking + Merkle dual-side) · **WI:** WI-S05-006 §6.1.5
> **Date:** 2026-05-01 · **Mode:** internal self-checklist (external audit deferred to S-20 GA gate)

ASVS chapter scope per WI-S05-006 §6.1.5 + S-04 ASVS precedent
(`specs/04_sprints/S04/asvs-v5-v6-v8-v10-v14-checklist.md`):
**V5 / V6 / V8 / V10 / V14**. Same chapters as S-04 — the multipart
CAS surface inherits the AC surface's threat model and adds chunker
+ R2 multipart adapter + sweeper sub-surfaces. Standard reference:
<https://owasp.org/www-project-application-security-verification-standard/>.

This checklist tracks PASS / WAIVED / N/A per requirement. WAIVED
entries carry a revalidation trigger (typically S-08 rate limit /
S-13 admin plane / S-19 onboarding / S-20 GA gate).

## V5 — Validation, Sanitization, and Encoding

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V5.1.1 | Application has defenses against HTTP parameter pollution | PASS | gRPC + REST single-source proto; tonic / axum reject duplicate fields; `BlobDigest` / `ChunkDigest` / `ManifestDigest` are typed newtypes (no string-bag input) |
| V5.1.2 | Frameworks protect against mass parameter assignment attacks | PASS | `BlobAuditRecord` `#[non_exhaustive]` typed; `OrphanCandidate` / `SessionInit` / `SessionFinalize` typed by-value structs; no JSON merge-patch path |
| V5.1.3 | All input is validated using positive validation (allow-lists) | PASS | `BlobDigest::new` + `ChunkDigest::compute` enforce digest length + algorithm allow-list (BLAKE3-256); `PartNumber` newtype rejects out-of-range 1..=10_000 (WI-S05-003); Region enum is typed allow-list |
| V5.1.4 | Structured data is strongly typed and validated against a schema | PASS | `MultipartError` 9-variant `#[non_exhaustive]` taxonomy (`corelink-r2-multipart`); `ManifestError` 9-variant `#[non_exhaustive]` (`corelink-manifest`); `SweeperError` 3-variant `#[non_exhaustive]` (`corelink-worker::reapi::cas::sweeper`); `serde_json::from_slice` runs AFTER `MAX_PAYLOAD_BYTES` pre-allocation rejection (WI-S05-005) |
| V5.1.5 | URL redirects and forwards only allow whitelisted destinations | N/A | CoreLink multipart has no URL redirect surface — pure RPC handler |
| V5.2.1 | All untrusted HTML input is sanitized | N/A | Multipart handler does not render HTML — pure proto/JSON/binary wire |
| V5.2.2 | Unstructured data is sanitized to enforce safety measures | PASS | `request_id` strings clamped to length + ASCII-only via tracing span attribute extraction; chunk bytes are content-addressed (BLAKE3 over the bytes IS the validation) |
| V5.2.3 | Application sanitizes user input before email/SMS | N/A | No email/SMS surface in S-05 |
| V5.2.5 | Application protects against template injection | N/A | No template engine in multipart handler |
| V5.2.6 | SSRF protection at every network call | PASS | All network calls go through typed adapters (R2 multipart / D1 / KV) — no user-controlled URL surface |
| V5.2.7 | SVG/script payloads sanitized | N/A | No SVG/script handling |
| V5.2.8 | Path traversal protection | PASS | R2 multipart path is HMAC-derived from tenant_id via `object_key::compose` (`corelink-r2-multipart`); tenant_prefix segment is structurally between bucket family and digest; client-supplied path components unreachable; user-controlled `blob_digest` is hex-encoded BLAKE3 — no ../ escape |
| V5.3.1 | Output encoding for shell command, SQL, etc. is safe | PASS | D1 query path uses prepared statements (`MultipartSchema` simulator pins typed envelope; no string concat); no shell command surface |
| V5.3.2 | Special characters escaped for OS, NoSQL, SQL, LDAP | PASS | Same as V5.3.1; `tenant_id` is UUID-typed, `blob_digest` / `chunk_digest` are hex BLAKE3 — no special char surface |
| V5.3.3 | XSS protection at output encoding | N/A | No HTML output |
| V5.3.4 | Application uses parameterized SQL queries | PASS | D1 trait surface uses parameterized binding; no SQL string concat in `corelink-multipart-schema` simulator or worker code |
| V5.3.5 | Type-safe ORM / DAL prevents injection | PASS | `MultipartSchema` simulator pins typed envelope; trait surface enforces typed inputs |
| V5.3.6 | Untrusted data not used in dynamic code interpreter | N/A | No dynamic interpreter |
| V5.3.7 | LDAP injection avoided | N/A | No LDAP surface |
| V5.3.8 | Operating system shell parsing avoided | PASS | No shell call surface |
| V5.3.9 | Local file inclusion avoided | PASS | No file inclusion surface |
| V5.3.10 | XPath injection avoided | N/A | No XPath |
| V5.5.1 | Serialization uses safe library + integrity check | PASS | `serde_json::from_slice` AFTER size pre-allocation reject (WI-S05-005); HKDF sig over canonical manifest preimage bound to `(tenant_id, blob_digest, manifest_digest)` provides integrity (WI-S05-005 sig sub-module) |
| V5.5.2 | XML parser disables DTD + entity resolution | N/A | No XML parser |
| V5.5.3 | Deserialization of untrusted data is restricted | PASS | Bounded parser limits (MAX_CHUNKS_PER_BLOB = 81920 cross-crate aligned across corelink-chunker + corelink-manifest + corelink-worker::reapi::cas::types) per WI-S05-002 + WI-S05-005 |

**V5 totals:** 14 PASS / 0 WAIVED / 9 N/A.

## V6 — Stored Cryptography (S-05 delta)

V6 was covered at PASS in S-03 (auth) + S-04 (AC sig). S-05 adds
multipart-side cripto requirements per CTRL-MULTIPART-002 + ADR-0041.

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V6.1.1 | Sensitive secrets at rest cryptographically protected | PASS | TDK derived per-tenant via path-HMAC (S-01); manifest signer uses HKDF-SHA256 of TDK with `info = b"manifest-sig"` (sibling-domain to `b"ac-sig"`); HKDF-derived signing keys never persisted |
| V6.1.2 | Approved algorithms (SHA-256+, AES-256) | PASS | BLAKE3-256 (chunker + manifest Merkle + canonical preimage) + HKDF-SHA256 (manifest sig key derive) per ADR-0041 + RFC 6962-style `\x00`-leaf / `\x01`-inner domain separation in `corelink-manifest::merkle` (mirrors corelink-ac::merkle byte-for-byte) |
| V6.1.3 | Encryption key strength matches algorithm | PASS | TDK 32 bytes (256-bit); HKDF derives manifest sig key of 32 bytes; HKDF info `b"manifest-sig"` byte-equal asserted at lib + integration |
| V6.2.1 | Random number generators are CSPRNG | PASS | No nonce surface in manifest sig — HKDF deterministic per `(tenant, manifest_digest)`; `subtle::ConstantTimeEq` for verify |
| V6.2.2 | All initialization vectors random | N/A | HKDF-SHA256 + BLAKE3-keyed are deterministic — no IV surface |
| V6.2.3 | Sufficient strength of cryptographic algorithms | PASS | BLAKE3-256 + HKDF-SHA256 (NIST SP 800-108 + 800-185 compliant) |
| V6.2.4 | Cryptographic operations are fail-safe | PASS | Manifest verify rejects bit-flipped envelopes 100 % of 10k iter (`corelink-manifest::tests::tampering`); INV-MULTIPART-MANIFEST-VALID + INV-MULTIPART-DUAL-SIDE-VERIFY enforced |
| V6.2.5 | Cryptographic functions resist downgrade attacks | PASS | Single algorithm pinned (BLAKE3 + HKDF-SHA256); no algorithm-negotiation surface — every wire artefact has a hard-pinned `alg` field rejected outside the allow-list |
| V6.2.6 | Side channel resistance | PASS | Constant-time verify on manifest sig (`subtle::ConstantTimeEq`); inherits S-04 Mann-Whitney 3-prong CT gate via shared HKDF infra (`keyed_mac_with_info`) |

**V6 totals:** 8 PASS / 0 WAIVED / 1 N/A.

## V8 — Data Protection

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V8.1.1 | Sensitive data is identified | PASS | Threat-modelled per WI §26 + spec contract §26 LINDDUN; multipart CAS data classes: chunk bytes (per-tenant content) + manifest envelope (tenant-scoped index) + audit chain (pseudonymized tenant_id) |
| V8.1.2 | Sensitive data is not logged | PASS | `tracing` spans redact tenant_prefix bytes; no chunk byte payload appears in log records; audit envelope canonical bytes never include raw user-typed input |
| V8.1.3 | Sensitive data is not cached client-side | PASS | Server-side only — no client cache headers set on multipart REST surface (WI-S05-001 §1) |
| V8.1.4 | Application minimizes parameters in URLs | PASS | gRPC + REST surface uses POST body for digests; no query-string disclosure |
| V8.1.5 | Backups encrypted | PASS (inherited) | R2 + D1 server-side encryption (Cloudflare default); BYOK envelope encryption is S-14 |
| V8.1.6 | Memory containing sensitive data clears upon use | PASS | TDK-derived signing keys live in `Zeroizing<Vec<u8>>` (zero-on-drop); manifest builder + verifier streaming avoids holding full chunk lists in memory (`INV-MULTIPART-STREAMING-MEMORY` O(1)) |
| V8.2.1 | Application sets cache-control header | N/A | No HTML/asset surface |
| V8.2.2 | Sensitive data sent in body, not URL | PASS | gRPC + REST POST body |
| V8.2.3 | Authenticated data cleared from client storage on logout | N/A | Server-side stack |
| V8.3.1 | Sensitive data sent over POST body, not URL | PASS | Same as V8.2.2 |
| V8.3.2 | Cache busting for sensitive resources | N/A | No browser cache surface |
| V8.3.3 | DPA disclosures | WAIVED — S-19 onboarding | Customer-facing DPA scoping is S-19 onboarding + S-20 GA per spec contract S-05 §11 outbound deps |
| V8.3.4 | Authenticated data deleted on revoke | PASS | Inherits S-03 DSR cascade; multipart-schema simulator preserves cascade structurally; production wiring at S-13 admin plane forward |
| V8.3.5 | Auditable trail of sensitive data access | PASS | 5-variant audit taxonomy + sweeper extends `SplitAborted` with `reason = "orphan_swept"` distinguisher; tenant-scoped per-record |
| V8.3.6 | Personal data minimization at processor | PASS | `tenant_id` is pseudonymous UUID v7; `blob_digest` / `chunk_digest` are content hashes; no raw PII |
| V8.3.7 | Privacy notices reviewed | WAIVED — S-19 onboarding | Customer-facing privacy notice copy is S-19; internal LINDDUN delta zero per spec contract §26 |
| V8.3.8 | Data subjects can erase + export | PASS | Inherits S-03 DSR PAT export integration; multipart-schema cascade tested in `corelink-multipart-schema::tests::idempotency_canonical` |

**V8 totals:** 13 PASS / 2 WAIVED (V8.3.3 + V8.3.7 — S-19 onboarding scope) / 2 N/A.

## V10 — Malicious Code

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V10.1.1 | Code analysis tool used | PASS | `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean across all S-05 crates |
| V10.2.1 | Application source integrity | PASS (inherited) | git commit signing on the canonical commits; SBOM CycloneDX 1.5+ via S-01 WI-S01-007 ship gate; cargo-deny on every PR |
| V10.2.2 | Pinned dependencies | PASS | `Cargo.lock` committed; workspace deps centralized; `cargo-audit` nightly via S-01 WI-S01-007 |
| V10.2.3 | Dependency provenance verified | PASS (inherited) | cosign keyless via S-01 WI-S01-007 ship gate |
| V10.3.1 | No backdoor / time-bomb / Easter egg | PASS | Code review surface — no time-based branch in S-05 source; sweeper alarm interval is a const; no `if tenant_id == X { … }` magic-special-case branches |
| V10.3.2 | Application checks for tampered binaries | PASS (inherited) | SBOM diff CI job; reproducible build smoke per S-01 |
| V10.3.3 | Application can detect anti-tampering | PASS | Manifest dual-side verify (server pre-persist + client post-download); INV-MULTIPART-MANIFEST-VALID + INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST |

**V10 totals:** 7 PASS / 0 WAIVED / 0 N/A.

## V14 — Configuration

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V14.1.1 | Build pipeline reproducible | PASS (inherited) | `cargo build --workspace --release` reproducible; SBOM diff CI |
| V14.1.2 | Application runs at minimum privilege | PASS | Worker runtime has only the bindings it needs (R2 + D1 + KV scoped per region); no admin / shell privileges |
| V14.1.3 | All dependency updates have been reviewed | PASS | Workspace deps reviewed at sprint contract version bump; cargo-deny enforces policy on every PR |
| V14.1.4 | Application security mechanisms cannot be disabled | PASS | 5-Layer Defense structurally enforced — `AuthCtx` is required to construct the handler call site; `SCOPE_CACHE_W` / `SCOPE_CACHE_R` checks at handler entry; no admin "disable security" toggle |
| V14.1.5 | Application is hardened against framework-level vulnerabilities | PASS | wasm32-clean (no tokio in src/ for `corelink-chunker` / `corelink-manifest` / `corelink-multipart-schema`); minimal feature surface (e.g. corelink-r2-multipart only takes `tokio::sync::Semaphore`, no reactor) |
| V14.2.1 | All components up-to-date | PASS | Cargo workspace deps; cargo-audit nightly |
| V14.2.2 | All unneeded features and frameworks removed | PASS | Default features minimal; `tower-middleware` feature opt-in for the auth layer; pure-logic crates default no-features |
| V14.2.3 | All third-party libraries CSPRNG-safe | PASS (inherited) | `rand::OsRng` via WI-S03-006 vendor pinning |
| V14.2.4 | Storage of secrets is approved | PASS | TDK in Cloudflare Secrets (forward) — handler relies on the boundary trait; in tests + CI a fixed-seed `TenantDerivationKey::from_bytes` is used |
| V14.2.5 | Public-facing components are isolated | PASS | Only the gRPC + REST surface is public; D1 / R2 / KV bindings are private to the worker |
| V14.2.6 | Application does not expose framework version | PASS | No version disclosure in REST headers |
| V14.3.1 | Web/app server hardened | PASS (inherited) | Cloudflare Workers runtime; no plaintext HTTP path |
| V14.3.2 | Reverse proxy hardened | N/A | Cloudflare itself |
| V14.3.3 | TLS 1.2+ required | PASS (inherited) | Cloudflare TLS 1.3 at the frontier; server-side stack |
| V14.4.1 | HTTP headers — Content-Security-Policy | N/A | No HTML surface |
| V14.4.2 | HTTP headers — X-Content-Type-Options | PASS | gRPC content-type pinned by tonic; REST surface emits `application/json` strict |
| V14.4.3 | HTTP headers — X-Frame-Options | N/A | No HTML surface |
| V14.4.4 | HTTP headers — referrer policy | N/A | No HTML surface |
| V14.4.5 | HTTP headers — HSTS | PASS (inherited) | Cloudflare HSTS at the frontier |

**V14 totals:** 14 PASS / 0 WAIVED / 4 N/A.

## Aggregate

| Chapter | PASS | WAIVED | N/A |
|---|---|---|---|
| V5 | 14 | 0 | 9 |
| V6 | 8 | 0 | 1 |
| V8 | 13 | 2 | 2 |
| V10 | 7 | 0 | 0 |
| V14 | 14 | 0 | 4 |
| **Total** | **56** | **2** | **16** |

**Verdict:** S-05 multipart CAS surface meets ASVS V5/V6/V8/V10/V14
at PASS or WAIVED-with-revalidation-trigger across 74 applicable
requirements. The 2 WAIVED items (V8.3.3 customer-facing DPA + V8.3.7
customer-facing privacy notice copy) revalidate at S-19 onboarding;
neither blocks S-05 SEAL.

**Authored:** 2026-05-01 by Gustavo Schneiter (via Claude Opus 4.7 1M;
orchestrator-finalized as part of WI-S05-006 SEAL Lote).
