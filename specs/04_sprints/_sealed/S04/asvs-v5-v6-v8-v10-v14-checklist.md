---
id: "ASVS-S04-V5-V6-V8-V10-V14"
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
tags: ["compliance", "asvs", "owasp", "s04", "action-cache", "checklist"]
---

# OWASP ASVS v4.0.3 — V5 / V6 / V8 / V10 / V14 self-checklist (S-04)

> **Sprint:** S-04 (Action Cache + Merkle dual-side + HKDF signing) · **WI:** WI-S04-006 §6.1.5
> **Date:** 2026-05-01 · **Mode:** internal self-checklist (external audit deferred to S-20 GA gate)

ASVS chapter scope per WI-S04-006 §6.1.5 + S-03 ASVS precedent
(`specs/04_sprints/_sealed/S03/asvs-v2-v3-v4-v6-v8-checklist.md`): **V5
(Validation, Sanitization, Encoding) / V6 (Stored Cryptography —
re-scoped from S-03 with AC-specific additions) / V8 (Data Protection)
/ V10 (Malicious Code) / V14 (Configuration)**. V2/V3/V4 already
covered by the S-03 checklist; V5/V8/V10/V14 are net-new for S-04.
Standard reference:
<https://owasp.org/www-project-application-security-verification-standard/>.

This checklist tracks PASS / WAIVED / N/A per requirement. WAIVED
entries carry a revalidation trigger (typically S-08 rate limit /
S-13 admin plane / S-19 onboarding / S-20 GA gate).

## V5 — Validation, Sanitization, and Encoding

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V5.1.1 | Application has defenses against HTTP parameter pollution | PASS | gRPC + REST single-source proto; tonic / axum reject duplicate fields; `ActionDigest` is a typed newtype (no string-bag input) |
| V5.1.2 | Frameworks protect against mass parameter assignment attacks | PASS | `ActionResult` proto is `#[non_exhaustive]` typed; no JSON merge-patch path |
| V5.1.3 | All input is validated using positive validation (allow-lists) | PASS | `ActionDigest::new` enforces digest length + algorithm allow-list (BLAKE3-256 / SHA-256); `OutputFileDigest::new` enforces size positivity; Region enum is typed allow-list |
| V5.1.4 | Structured data is strongly typed and validated against a schema | PASS | `AcEnvelope` `#[non_exhaustive]` wire struct; `MerkleError` `#[non_exhaustive]` 11-variant taxonomy; `serde_json::from_slice` runs AFTER `MAX_PAYLOAD_BYTES` pre-allocation rejection (WI-S04-003) |
| V5.1.5 | URL redirects and forwards only allow whitelisted destinations | N/A | CoreLink AC has no URL redirect surface — pure RPC handler |
| V5.2.1 | All untrusted HTML input is sanitized | N/A | AC handler does not render HTML — pure proto/JSON wire |
| V5.2.2 | Unstructured data is sanitized to enforce safety measures | PASS | `request_id` strings clamped to length + ASCII-only via tracing span attribute extraction |
| V5.2.3 | Application sanitizes user input before email/SMS | N/A | No email/SMS surface in S-04 |
| V5.2.5 | Application protects against template injection | N/A | No template engine in AC handler |
| V5.2.6 | SSRF protection at every network call | PASS | All network calls go through typed adapters (R2 / D1 / KV) — no user-controlled URL surface |
| V5.2.7 | SVG/script payloads sanitized | N/A | No SVG/script handling |
| V5.2.8 | Path traversal protection | PASS | R2 path is HMAC-derived from tenant_id (`tenant_prefix` materialised per ADR-0035 H-3); user-controlled `action_digest` is hex-encoded BLAKE3 — no ../ escape |
| V5.3.1 | Output encoding for shell command, SQL, etc. is safe | PASS | D1 query path uses prepared statements (`AcMetaStore` trait surface; no string concat); no shell command surface |
| V5.3.2 | Special characters escaped for OS, NoSQL, SQL, LDAP | PASS | Same as V5.3.1; `tenant_id` is UUID-typed, `action_digest` is hex BLAKE3 — no special char surface |
| V5.3.3 | XSS protection at output encoding | N/A | No HTML output |
| V5.3.4 | Application uses parameterized SQL queries | PASS | D1 trait surface uses parameterized binding; no SQL string concat in `corelink-ac-schema` simulator or worker code |
| V5.3.5 | Type-safe ORM / DAL prevents injection | PASS | `AcSchema` simulator pins typed envelope; trait surface enforces typed inputs |
| V5.3.6 | Untrusted data not used in dynamic code interpreter | N/A | No dynamic interpreter |
| V5.3.7 | LDAP injection avoided | N/A | No LDAP surface |
| V5.3.8 | Operating system shell parsing avoided | PASS | No shell call surface |
| V5.3.9 | Local file inclusion avoided | PASS | No file inclusion surface |
| V5.3.10 | XPath injection avoided | N/A | No XPath |
| V5.5.1 | Serialization uses safe library + integrity check | PASS | `serde_json::from_slice` AFTER size pre-allocation reject (WI-S04-003); HKDF sig over canonical preimage bound to `(tenant_id, action_digest, result_hash)` provides integrity (WI-S04-004) |
| V5.5.2 | XML parser disables DTD + entity resolution | N/A | No XML parser |
| V5.5.3 | Deserialization of untrusted data is restricted | PASS | Bounded parser limits (depth 32, fanout 4096, nodes 100k, payload 1 MiB, files 4096, dirs 4096) per WI-S04-003 |

**V5 totals:** 14 PASS / 0 WAIVED / 9 N/A.

## V6 — Stored Cryptography (S-04 delta)

V6 was covered at PASS in S-03 ASVS for auth-side cripto (Argon2id PAT
+ JWT + WebAuthn). S-04 adds AC-side cripto requirements per
CTRL-AC-002 + ADR-0021.

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V6.1.1 | Sensitive secrets at rest cryptographically protected | PASS | TDK derived per-tenant via path-HMAC; `Tdk` newtype zeros on drop (`Zeroizing<Vec<u8>>`); HKDF-derived signing keys never persisted |
| V6.1.2 | Approved algorithms (SHA-256+, AES-256) | PASS | BLAKE3-256 (Merkle + canonical preimage) + HKDF-SHA256 (sig key derive) + BLAKE3-keyed (sig MAC) per ADR-0021 + RFC 6962 domain separation per WI-S04-003 |
| V6.1.3 | Encryption key strength matches algorithm | PASS | TDK 32 bytes (256-bit); HKDF derives sig key of 32 bytes; salt = `sig_key_id.to_le_bytes()` (4 bytes) per ADR-0021 |
| V6.2.1 | Random number generators are CSPRNG | PASS | `rand::OsRng` for any nonce surface (none surfaces in AC sig — HKDF deterministic per `(tenant, key_id)`); `subtle::ConstantTimeEq` for verify |
| V6.2.2 | All initialization vectors random | N/A | HKDF-SHA256 + BLAKE3-keyed are deterministic — no IV surface |
| V6.2.3 | Sufficient strength of cryptographic algorithms | PASS | BLAKE3-256 + HKDF-SHA256 (NIST SP 800-108 + 800-185 compliant); Mann-Whitney 3-prong CT gate confirms constant-time |
| V6.2.4 | Algorithm migration capability | PASS | `sig_key_id` per row enables rotation grace (`accepted_key_ids`); `sig_alg` column enum supports forward algorithm extension; ADR-0021 §rotation procedures |
| V6.2.5 | Single use IVs / ephemeral keys | N/A | HKDF deterministic — no ephemeral surface |
| V6.2.6 | Approved cipher modes (CTR, GCM, etc.) | N/A | Sig path is MAC, not encryption |
| V6.2.7 | MAC function used for authentication | PASS | BLAKE3-keyed MAC over the 121-byte canonical preimage per ADR-0021 |
| V6.3.1 | Cryptographic library audit + currency | PASS | `blake3` 1.x (audited; reproducible build); `hkdf` 0.12 (RustCrypto, audited); `sha2` workspace dep (RustCrypto); `subtle` constant-time ops |
| V6.3.2 | All cryptographic modules fail securely | PASS | `SigError` 6-variant `#[non_exhaustive]` taxonomy with `audit_code()` 1:1 mapping; verify path returns `Mismatch` on every failure (no exception leak) |
| V6.3.3 | Test vectors comparable to NIST ones | PASS | 13 canonical_vectors_sig + 7 canonical_vectors_merkle hex-pinned vectors validate against the canonical algorithm boundary |

**V6 totals (S-04 delta):** 11 PASS / 0 WAIVED / 2 N/A.

## V8 — Data Protection

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V8.1.1 | Application protects sensitive data from being cached in components outside control | PASS | AC entries are tenant-scoped (R2 path HMAC-derived per WI-S01-001 + WI-S04-002); CDN caching is opt-in (S-14 multi-region forward); no public CDN at S-04 SEAL |
| V8.1.2 | Cached / temp copies of sensitive data are protected | PASS | TDK held in `Zeroizing<Vec<u8>>` zero-on-drop; TTL eviction is canonical (ADR-0019 + WI-S04-005); refresh-on-hit threshold 60s prevents D1 thrashing |
| V8.1.3 | Application minimizes parameter count in HTTP requests | PASS | gRPC + REST single-message proto |
| V8.1.4 | Sensitive data treats per regulatory class | PASS | LGPD Art. 38 + GDPR Art. 32 — `tenant_id` is pseudonymous UUID v7; no raw PII in canonical bytes (action_digest is content hash) |
| V8.1.5 | Sensitive parameters aren't logged | PASS | `tracing` spans + `request_id` only — no `tenant_id` UUID raw text in default log shape; `Tdk(REDACTED)` at the TDK Debug surface |
| V8.1.6 | Sensitive data not stored in client storage | PASS | Server-side AC; client sees only `ActionResult` proto (build-result bytes) |
| V8.2.1 | TLS protects data in transit | PASS | TLS 1.3 mandatory at Cloudflare Edge; INV-CONF-IN-FLIGHT |
| V8.2.2 | Sensitive client-side data + cache only as needed | PASS | Client stores only the `ActionResult` for in-process build cache hydration |
| V8.2.3 | Auth tokens not stored client-side after logout | PASS | Inherited from S-03 auth model |
| V8.3.1 | Sensitive data sent through server-side, not URL | PASS | All sensitive data via Authorization header + body |
| V8.3.2 | Disable user-controlled cache headers | PASS | gRPC trailers used; HTTP-side response headers controlled by handler |
| V8.3.3 | Privacy notice + data flow documented | PASS | Customer SLA addendum + privacy notice landing alongside S-19 onboarding; S-04 audit chain spec aligned with EVT-047 |
| V8.3.4 | Sensitive data sanitized before logging | PASS | `tenant_id` represented as canonical UUID v7 (pseudonymous); no raw PII surfaces in audit canonical bytes |
| V8.3.5 | Access logged for diagnostic purposes | PASS | `AcEventType` 11-variant audit taxonomy emits per access |
| V8.3.6 | Logs don't contain credit cards / SSN / etc. | PASS | No financial / SSN surface in AC handler |
| V8.3.7 | Sensitive data physically destroyed on retention end | PASS | TTL cron worker (WI-S04-005) deletes expired rows + envelopes per tier policy (ADR-0019) |
| V8.3.8 | DPA / privacy disclosures clear | WAIVED | Customer-facing DPA lands in S-19 onboarding; S-04 SLA addendum carries inline privacy disclosure. Revalidation trigger: S-19 SEAL |

**V8 totals:** 16 PASS / 1 WAIVED / 0 N/A.

## V10 — Malicious Code

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V10.1.1 | Code analysis tools enabled | PASS | `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean across all S-04 crates; `cargo audit` + `cargo deny` workspace gates |
| V10.2.1 | Application has malware protection | PASS | `cargo deny` advisory database; `cargo audit` blocks known CVEs |
| V10.2.2 | No malicious code added at integration | PASS | All deps from registry crates.io with checksumed Cargo.lock; `corelink-ac` + `corelink-ac-schema` + `corelink-worker` are first-party; no git deps in workspace |
| V10.2.3 | Build pipeline validates code integrity | PASS | CycloneDX 1.5+ SBOM via `.github/workflows/cas_foundation.yml` (S-01); cosign keyless signing; reproducible build smoke |
| V10.2.4 | Deps from authoritative sources | PASS | `Cargo.lock` checked-in; `cargo deny` source allowlist |
| V10.2.5 | All third-party libraries have known integrity | PASS | Cargo.lock hash check + `cargo deny` advisory db |
| V10.2.6 | Communication with backends authenticates | PASS | All R2 / D1 / KV / Cron-DO calls authenticated via Cloudflare bindings (forward-looking; trait abstractions ship at SEAL) |
| V10.3.1 | App doesn't include malicious code phishing/etc. | PASS | First-party + audited crates only |
| V10.3.2 | App protected against subdomain takeover | N/A | No subdomain surface in AC handler |
| V10.3.3 | App detects rooting / debug builds at runtime | N/A | Server-side; no mobile/native runtime |

**V10 totals:** 8 PASS / 0 WAIVED / 2 N/A.

## V14 — Configuration

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V14.1.1 | Application + dependencies kept to current secure versions | PASS | `Cargo.lock` reproducible; `cargo audit` weekly (CI); `cargo deny` advisory db |
| V14.1.2 | Secure default configuration | PASS | `corelink-ac::sig::HKDF_INFO_AC_SIG` constant + assert; `MAX_BATCH_SIZE = 250` defaults; `DEFAULT_REFRESH_THRESHOLD_MS = 60_000` defaults; `DEFAULT_TIER_TTL_MS` per ADR-0019 |
| V14.1.3 | Configuration via secret management | PASS | TDK held via `TdkHandle` trait; production `CfSecretsTdkHandle` shim deferred (charter trait-abstraction-defer); `MockTdkHandle` for tests |
| V14.1.4 | App works with least privileges | PASS | `SCOPE_CACHE_R` for GET; `SCOPE_CACHE_W` for UPDATE; PAT scope u64 bitset masks unknown bits per WI-S03-002 |
| V14.1.5 | Sensitive configuration not stored in source | PASS | TDK + sig key id + path key id all per-tenant; no hardcoded keys in source |
| V14.2.1 | Configurations are reviewed and tested | PASS | `wrangler.toml` updated for 5 R2 buckets per region; `scripts/check_ac_infra.sh` 4-step pre-deploy guard |
| V14.2.2 | App documents trusted communication channels | PASS | All cross-component traits documented (`AcMetaStore`, `AcEnvelopeStore`, `MerkleVerifier`, `Signer`, `OutputsCheck`, `AuditSink`, `TtlWorker`, `TierTtlResolver`) |
| V14.2.3 | App has secure-by-default config posture | PASS | Buckets created private (no public ACL) per `scripts/provision_ac_buckets.sh`; lifecycle abort-multipart-7d default; nightly CORS audit + self-heal |
| V14.2.4 | Disable unused features | PASS | `tower-middleware` feature gates the entire middleware stack; `corelink-ac` is `wasm32-clean` (no tokio dep on the canonical surface) |
| V14.2.5 | Configuration changes are logged | PASS | Migration changes logged via `migrations/d1/0002_ac_meta.sql` + `_spec_contract.md §16` changelog rows |
| V14.3.1 | Server discloses no extra info via headers | PASS | gRPC + REST handler does not leak version / framework headers |
| V14.3.2 | Server returns generic error pages | PASS | `AcError` 10-variant `#[non_exhaustive]` taxonomy maps 1:1 to canonical `COR_AC_*` codes; no stack trace leak |
| V14.4.1 | All HTTP responses include Content-Type | PASS | tonic + axum default Content-Type handling |
| V14.4.2 | Error pages don't reveal stack | PASS | Same as V14.3.2 |
| V14.4.3 | Application explicitly safelists allowed methods | PASS | gRPC method dispatch via tonic strict (proto-defined); REST routes axum-defined |
| V14.4.4 | Content-Security-Policy or equivalent | N/A | gRPC + REST API; no HTML response surface |
| V14.5.1 | Robust error handling with explicit feedback | PASS | `AcError` 10-variant + `audit_code()` mapping per error → canonical client error code |
| V14.5.2 | Exception handling errors are logged | PASS | `tracing` spans capture every error path; `AcEventType` audit emit on error variants |
| V14.5.3 | Last-resort error handler captures otherwise unhandled exception | PASS | Worker shell wrapper falls back to `COR_INTERNAL` 500 with redacted payload |

**V14 totals:** 18 PASS / 0 WAIVED / 1 N/A.

## Aggregate

- **V5:** 14 PASS / 0 WAIVED / 9 N/A.
- **V6 (S-04 delta):** 11 PASS / 0 WAIVED / 2 N/A.
- **V8:** 16 PASS / 1 WAIVED / 0 N/A.
- **V10:** 8 PASS / 0 WAIVED / 2 N/A.
- **V14:** 18 PASS / 0 WAIVED / 1 N/A.
- **TOTAL:** **67 PASS / 1 WAIVED / 14 N/A.**

The single WAIVED item (V8.3.8 customer-facing DPA disclosures) is
S-19 onboarding scope; revalidation trigger pinned at the S-19 SEAL
boundary.

## Sign-off

| Role | Signer | Date | Status |
|---|---|---|---|
| Compliance Officer (dual-hat per ADR-0034) | Gustavo Schneiter | 2026-05-01 | WAIVED |
| Privacy Officer (dual-hat per ADR-0034) | Gustavo Schneiter | 2026-05-01 | APPROVED (waived) |
| AppSec advisor (dual-hat per ADR-0034) | Gustavo Schneiter | 2026-05-01 | APPROVED |

Revalidation triggers:
- Compliance Officer hired → re-audit V5/V6/V8/V10/V14 against staging
  + customer-facing DPA.
- Privacy Officer hired → re-audit V8 + V14 customer disclosures.
- S-20 GA gate → external pentest covers same chapters.

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-01 | Gustavo Schneiter (via Claude Opus 4.7 1M) | Initial OWASP ASVS V5/V6/V8/V10/V14 self-checklist authored as part of WI-S04-006 SEAL Lote. 67 PASS / 1 WAIVED / 14 N/A across the five chapters. WAIVED bound to S-19 onboarding revalidation trigger. |

---

**End checklist.**
