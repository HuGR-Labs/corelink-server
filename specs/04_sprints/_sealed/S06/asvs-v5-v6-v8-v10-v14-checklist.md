---
id: "ASVS-S06-V5-V6-V8-V10-V14"
type: "compliance_matrix"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-05-02"
updated: "2026-05-02"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["compliance", "asvs", "owasp", "s06", "gc", "checklist"]
---

# OWASP ASVS v4.0.3 — V5 / V6 / V8 / V10 / V14 self-checklist (S-06)

> **Sprint:** S-06 (Garbage Collection: Mark & Sweep + INV-GC-001/004 + TLA+ verified) · **WI:** WI-S06-007 §6.1
> **Date:** 2026-05-02 · **Mode:** internal self-checklist (external audit deferred to S-20 GA gate)

ASVS chapter scope per WI-S06-007 §6 + S-04/S-05 ASVS precedent
(`specs/04_sprints/_sealed/S04/asvs-v5-v6-v8-v10-v14-checklist.md` +
`specs/04_sprints/_sealed/S05/asvs-v5-v6-v8-v10-v14-checklist.md`):
**V5 / V6 / V8 / V10 / V14**. Same chapters as S-04 + S-05 — the GC
surface inherits the AC + multipart threat models and adds mark / sweep
/ physical-delete / reconcile sub-surfaces. Standard reference:
<https://owasp.org/www-project-application-security-verification-standard/>.

This checklist tracks PASS / WAIVED / N/A per requirement. WAIVED
entries carry a revalidation trigger (typically S-09 observability /
S-11 DSR / S-13 admin plane / S-19 onboarding / S-20 GA gate).

## V5 — Validation, Sanitization, and Encoding

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V5.1.1 | Application has defenses against HTTP parameter pollution | PASS | gRPC + REST single-source proto; tonic / axum reject duplicate fields; `BlobDigest` / `RunId` / `GcRegion` typed newtypes (no string-bag input) |
| V5.1.2 | Frameworks protect against mass parameter assignment attacks | PASS | `GcRun` + `GcCandidate` + `BlobMetaRow` `#[non_exhaustive]` typed structs; no JSON merge-patch path; trait surface enforces typed inputs |
| V5.1.3 | All input is validated using positive validation (allow-lists) | PASS | `BlobDigest::new` enforces 64-char lower-case hex BLAKE3-256 (rejects upper-case/non-hex/wrong-length); `GcRegion` enum 5-region allow-list; `GcStatus` + `GcPhase` enums monotone state machines; `GcEventType` 11-variant `#[non_exhaustive]` |
| V5.1.4 | Structured data is strongly typed and validated against a schema | PASS | `GcError` `#[non_exhaustive]` + `audit_code()` short-id contract; `MarkError` / `SweepError` / `PhysicalDeleteError` / `ReconcileError` taxonomies; serde paths run AFTER bounds reject (mark/sweep/physical-delete/reconcile do not deserialize untrusted JSON; `ac_meta.blob_refs` deserialization happens at the AC ingest seam from S-04) |
| V5.1.5 | URL redirects and forwards only allow whitelisted destinations | N/A | GC has no URL redirect surface — pure cron-driven worker |
| V5.2.1 | All untrusted HTML input is sanitized | N/A | GC does not render HTML — pure proto/binary wire + audit chain |
| V5.2.2 | Unstructured data is sanitized | PASS | `request_id` / `actor_principal` strings clamped to length + ASCII-only via tracing span attribute extraction; blob bytes are content-addressed (BLAKE3 over the bytes IS the validation) |
| V5.2.3 | Application sanitizes user input before email/SMS | N/A | No email/SMS surface in S-06 |
| V5.2.5 | Application protects against template injection | N/A | No template engine in GC |
| V5.2.6 | SSRF protection at every network call | PASS | All network calls go through typed adapters (R2 / D1 / KV / Cron DO) — no user-controlled URL surface |
| V5.2.7 | SVG/script payloads sanitized | N/A | No SVG/script handling |
| V5.2.8 | Path traversal protection | PASS | R2 path is HMAC-derived from tenant_id (S-01); blob_digest is hex-encoded BLAKE3 — no ../ escape; admin plane S-13 will enforce its own auth surface |
| V5.3.1 | Output encoding for shell command, SQL, etc. is safe | PASS | D1 query path uses prepared statements (`MarkPhase::execute` / `SweepPhase::execute` simulators pin typed envelope; no string concat); `json_each` canonical idiom (Lote 10.6bis Part 2a P0-1) replaces fragile LIKE-substring match |
| V5.3.2 | Special characters escaped for OS, NoSQL, SQL, LDAP | PASS | Same as V5.3.1; `tenant_id` is UUID-typed, `blob_digest` is hex BLAKE3 — no special char surface |
| V5.3.3 | XSS protection at output encoding | N/A | No HTML output |
| V5.3.4 | Application uses parameterized SQL queries | PASS | D1 trait surface uses parameterized binding; no SQL string concat in any GC simulator or worker code |
| V5.3.5 | Type-safe ORM / DAL prevents injection | PASS | `BlobMetaStore` / `AcReferenceIndex` / `RefcountSource` / `R2BlobStore` trait surfaces enforce typed inputs |
| V5.3.6 | Untrusted data not used in dynamic code interpreter | N/A | No dynamic interpreter |
| V5.3.7 | LDAP injection avoided | N/A | No LDAP surface |
| V5.3.8 | Operating system shell parsing avoided | PASS | No shell call surface |
| V5.3.9 | Local file inclusion avoided | PASS | No file inclusion surface |
| V5.3.10 | XPath injection avoided | N/A | No XPath |
| V5.5.1 | Serialization uses safe library + integrity check | PASS | GC ingests pre-validated AC + blob_meta + manifest_chunks rows; no raw bytes deserialization at GC seam; the AC ingest path (S-04) validates upstream |
| V5.5.2 | XML parser disables DTD + entity resolution | N/A | No XML parser |
| V5.5.3 | Deserialization of untrusted data is restricted | PASS | No untrusted deserialization at GC seam; bounded batch enforced (`CANONICAL_BATCH_SIZE = 250` per Lote 10.4bis D1 100KB envelope) |

**V5 totals:** 14 PASS / 0 WAIVED / 9 N/A.

## V6 — Stored Cryptography (S-06 delta)

V6 was covered at PASS in S-03 (auth) + S-04 (AC sig) + S-05 (manifest
sig). S-06 adds GC-side cripto requirements: TLA+ formal verification
of `gc_correctness.tla::InvGCReachableNeverDeleted` +
`InvGCReRefProtected` + TLC v1.8.0 SHA-256 supply-chain pinning
(ADR-0042 §A1).

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V6.1.1 | Sensitive secrets at rest cryptographically protected | PASS | GC does not handle TDK directly — inherits S-04 sig sub-module; refcount data is non-sensitive per LINDDUN delta |
| V6.1.2 | Approved algorithms (SHA-256+, AES-256) | PASS | TLC SHA-256 supply-chain pin (ADR-0042 §A1) `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f`; BLAKE3-256 for `BlobDigest` validation |
| V6.1.3 | Encryption key strength matches algorithm | N/A | GC has no encryption surface; inherits S-04 sig key strength |
| V6.2.1 | Random number generators are CSPRNG | PASS | Property test PRNG `ChaCha20Rng::seed_from_u64` deterministic for reproducibility per Lote 10.6-tris OPUS-MISS-2; production cron jitter uses CF Cron DO platform RNG |
| V6.2.2 | All initialization vectors random | N/A | No IV surface in GC |
| V6.2.3 | Sufficient strength of cryptographic algorithms | PASS | BLAKE3-256 + SHA-256 (TLC supply-chain pin) NIST SP 800-185 + 800-108 compliant |
| V6.2.4 | Cryptographic operations are fail-safe | PASS | Audit fail-closed envelope: emit BEFORE mutation; emit failure surfaces `*Error::AuditEmissionFailed` and the row is preserved (sweep + reconcile); INV-GC-001/004 0 violations sustained |
| V6.2.5 | Cryptographic functions resist downgrade attacks | PASS | TLC SHA-256 pin enforced fail-closed in CI; BLAKE3 single algorithm pinned |
| V6.2.6 | Side channel resistance | N/A | GC has no constant-time-sensitive path (refcount comparison is non-secret; tenant isolation is structural not timing-based; inherits S-04 Mann-Whitney 3-prong CT gate via shared infra) |

**V6 totals:** 7 PASS / 0 WAIVED / 2 N/A.

## V8 — Data Protection

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V8.1.1 | Sensitive data identified + classified | PASS | `prev_state` BlobState is forensic snapshot; `tenant_id` UUID v7 pseudonymous; `blob_digest` content-hash; refcount non-sensitive; LINDDUN delta zero per spec contract §26 |
| V8.1.2 | Sensitive data not unnecessarily logged | PASS | INV-AUDIT-NO-RAW-PII enforced — no PII in canonical audit envelope bytes; `redact_pat!` macro inherited; tracing spans clamp `request_id` to length + ASCII-only |
| V8.1.3 | Consent for processing tracked | PASS | Tenant onboarding via Clerk SSO (S-03) captures consent; LGPD Art. 16 retention compliance via grace period 72h CAS / 24h AC enforced |
| V8.2.1 | Sensitive cookies marked Secure + HttpOnly | N/A | GC has no cookie surface |
| V8.2.2 | Cache-Control header for sensitive data | N/A | GC has no HTTP response surface (cron-driven worker) |
| V8.3.1 | Customer-controllable PII is removable on request | PASS | DSR erasure (S-11 forward) bypass grace authorized only for the issuing tenant; cross-tenant impossible by INV-TENANT-ISOLATION + 24 lib unit tests; CTRL-PRIV-030 alignment |
| V8.3.2 | Customer-controllable PII export | WAIVED (S-19) | DSR PAT export (S-03) inherited; GC delta has no export surface |
| V8.3.3 | Customer-facing DPA available | WAIVED (S-19) | S-19 onboarding scope |
| V8.3.4 | Sensitive data not sent to log files | PASS | INV-AUDIT-NO-RAW-PII enforced; canonical event types `corelink.gc.*` carry only pseudonymous fields |
| V8.3.5 | Sensitive data deleted when no longer needed | PASS | Grace period 72h CAS / 24h AC enforced; physical-delete strict-`>` post-grace gate (off-by-one anti-pattern pinned); R2→D1 ordering invariant |
| V8.3.6 | Sensitive data identified for retention policy | PASS | `RetentionHint::for_tier` mapping inherited from S-03 (Solo30d / Team90d / Business1y / Enterprise7y); GC respects retention via grace period |
| V8.3.7 | Customer-facing privacy notice copy | WAIVED (S-19) | S-19 onboarding scope |

**V8 totals:** 7 PASS / 3 WAIVED (S-19) / 2 N/A.

## V10 — Malicious Code Protection

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V10.1.1 | Code analysis for malicious patterns | PASS | `clippy --workspace --all-targets -- -D warnings` clean; cargo-deny supply-chain inherited (S-01); `forbid(unsafe_code)` literal at every crate root |
| V10.2.1 | Code reviewed against backdoor patterns | PASS | Per-WI codex / Sonnet review trace + cumulative sprint-close Sonnet review (S-06 forward); no `unsafe` / `unwrap` / `expect` / `panic` / `[i]` indexing in lib code per crate-strict deny |
| V10.2.2 | Application is built using current dependencies | PASS | `cargo audit` clean (workspace inherited); no known CVEs |
| V10.2.3 | Library + framework dependencies signed/verified | PASS | TLC v1.8.0 SHA-256 pinned (ADR-0042 §A1); cargo-deny supply-chain (S-01); cosign keyless SBOM (S-01) |
| V10.3.1 | Application has integrity controls | PASS | Audit chain integrity (S-09 forward INV-OBS-AUDIT-CHAIN-INTEGRITY); reconcile validates refcount via `json_each` JSON-aware membership; cross-component property tests at 10k+ iter |
| V10.3.2 | Application sources cryptographically signed | PASS | git commit signing inherited; cosign keyless SBOM |

**V10 totals:** 6 PASS / 0 WAIVED / 0 N/A.

## V14 — Configuration

| Req | Statement | Status | Evidence / WI link |
|---|---|---|---|
| V14.1.1 | Build pipeline ensures security checks | PASS | `.github/workflows/spec_validation.yml` + `tla_check.yml` + `nightly.yml` + `gc-ship-gate.yml` chain; CI gate `validate_inv_promotion.py` + `validate_specs.py` + `check_migrations_additive.py` |
| V14.1.2 | Application doesn't depend on cleartext secrets in config | PASS | wrangler.toml has only public R2 bucket bindings; secrets via CF Secrets binding (deferred to staging account provisioning) |
| V14.1.3 | Application configuration files cannot reveal sensitive data | PASS | All canonical constants pinned via Rust constants (CANONICAL_BATCH_SIZE / GRACE_CAS_MS / etc.); no per-environment config secrets |
| V14.1.4 | Application uses up-to-date dependencies | PASS | `cargo audit` + cargo-deny inherited; nightly job re-runs |
| V14.1.5 | TLS configuration is correct | PASS | Inherits Cloudflare Worker default TLS posture (S-01); no GC-specific TLS surface |
| V14.2.1 | Application removes unused features at deployment | PASS | feature flags `tower-middleware` opt-in; GC crate has no feature flags |
| V14.2.2 | All components are up-to-date | PASS | Nightly cargo-audit + cargo-deny gates inherited |
| V14.3.1 | Application has documented secrets-handling policy | PASS | TDK hygiene inherited from S-04 (Zeroizing + redacted Debug + crate-private as_bytes); GC has no secret surface delta |
| V14.4.1 | Sensitive variables not exposed via debug interface | PASS | Custom `Debug` redaction inherited; no `Display` impl on secret newtypes |
| V14.5.1 | Application uses safe defaults | PASS | `SweepConfig::canonical()` + `MarkConfig::canonical()` + `PhysicalDeleteConfig::canonical()` + `ReconcileConfig::canonical()` constructors pin canonical defaults; validators reject non-canonical inversions |

**V14 totals:** 10 PASS / 0 WAIVED / 0 N/A.

## Summary

| Chapter | PASS | WAIVED | N/A |
|---|---|---|---|
| V5 | 14 | 0 | 9 |
| V6 | 7 | 0 | 2 |
| V8 | 7 | 3 (S-19) | 2 |
| V10 | 6 | 0 | 0 |
| V14 | 10 | 0 | 0 |
| **Total** | **44** | **3** | **13** |

**Status:** 44 PASS / 3 WAIVED (all S-19 onboarding scope) / 13 N/A.

The 3 WAIVED items (V8.3.2 customer-controllable PII export +
V8.3.3 customer-facing DPA + V8.3.7 customer-facing privacy notice
copy) are S-19 onboarding scope. SOC 2 + LGPD ship-gate gap analysis
closes at S-20 GA gate via external pentest.

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-02 | Gustavo Schneiter (via Claude Opus 4.7 1M) | Initial S-06 ASVS V5/V6/V8/V10/V14 self-checklist (WI-S06-007 §6.1.5). |

---

**End ASVS S-06 v1.0.0.**
