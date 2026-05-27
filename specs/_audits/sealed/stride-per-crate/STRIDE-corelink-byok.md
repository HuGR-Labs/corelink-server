# STRIDE deep dive — `corelink-byok` (BYOK envelope operations)

- **Crate:** `crates/corelink-byok` + provider drivers (`corelink-byok-aws`, `-azure`, `-gcp`, `-vault`) + `corelink-byok-revocation`
- **Date:** 2026-05-15
- **Owner:** Security Lead + Crypto Engineering
- **Pentest scope:** Yes — engagement 2026-06-15 (P0 surface)
- **Coarse references:** `specs/_audits/matrix-stride-ctrl.csv` rows AST-BLOB×TB-2 (T-001/T-002) and AST-TOKEN×TB-2 (I-006)
- **Pentest doc cross-ref:** `specs/_audits/sealed/pentest/PENTEST-EVIDENCE-PACKAGE.md` §3.3 (BYOK envelope encryption)
- **SOC 2 cross-ref:** CC3.2 (risk identification — encryption sovereignty), CC6.1 (logical access — KMS), CC6.7 (data-at-rest crypto)

## 1. Trust boundaries

| Boundary | Caller (left side) | Callee (right side, this crate) | Auth/authz that precedes | Output / side effect |
|---|---|---|---|---|
| **TB-byok-1** | `corelink-worker` write path (after PAT scope check + tenant resolution) | `corelink-byok::wrap_dek(tenant_id, dek)` | PAT-AUTHZ scope `cas:write` + INV-TENANT-ISOLATION resolver | Wrapped DEK bytes (opaque); KMS provider-side audit entry |
| **TB-byok-2** | This crate's wrap/unwrap → external KMS endpoint (AWS / GCP / Azure / Vault) | Provider KMS API (cross-org TB-0/TB-1 hybrid) | Provider IAM role (workload identity / IRSA / WIF / Vault AppRole), TLS 1.3 + cert pin | Plaintext DEK (TTL ≤ 5 min in cache); provider CloudTrail / Cloud Audit / Activity Log entry |
| **TB-byok-3** | `corelink-byok-revocation` kill-switch checker | All read paths consulting cached DEK | INV-BYOK-CRYPTO-SOVEREIGNTY 5-min hard TTL; KMS access probe 60s | DEK cache eviction; subsequent reads fail-CLOSED |
| **TB-byok-4** | Admin operator → BYOK configuration API | `corelink-admin-api::byok_*` handlers (calls this crate) | Clerk session + WebAuthn (CTRL-AUTH-010) + dual-approval (INV-ADMIN-DUAL-APPROVAL) | CMK rotation; provider rebind; emit audit event with two signatures |

## 2. STRIDE per boundary

### 2.1 TB-byok-1 (worker → byok)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** Spoofing | Worker forges `tenant_id` to wrap a DEK under another tenant's CMK | CTRL-AUTHZ-002 (explicit tenant_id assertion at storage call); INV-TENANT-ISOLATION 5-layer defense | `crates/corelink-byok/tests/adversarial.rs` (tenant-spoof case) + `specs/tla/tenant_isolation.tla` |
| **T** Tampering | AAD (`tenant_id`, `blob_hash`) stripped/replaced before reaching KMS | KMS `EncryptionContext` enforced provider-side; AAD canonicalized via JCS (RFC 8785) before submit | `crates/corelink-byok/tests/prop_byok.rs` (30k cases, AAD-tamper rejects) — FM-BYOK-002 |
| **R** Repudiation | Worker emits wrap event but customer claims it never happened | Dual-audit: CoreLink EVT-028 audit-chain entry + provider CloudTrail / Cloud Audit; daily reconcile diff | `crates/corelink-audit-chain` chain-verify CI job + INV-AUDIT-APPEND-ONLY |
| **I** Info-disclosure | Plaintext DEK leaks via panic backtrace or trace log | `Zeroize` Drop on `Dek`; no-secret-in-log lint (CTRL-CRED-001); structured-log schema allowlist | `tools/pat_plaintext_lint/` CI gate + `crates/corelink-byok/tests/mutation_kills.rs` |
| **D** DoS | Storm of wrap calls saturates KMS rate limit, blocks legitimate writes | Per-tenant token bucket (CTRL-RATE-001) ahead of byok; jittered exp backoff (CTRL-BACKOFF-001); DEK cache absorbs hot-tenant load within TTL | k6 wrap-storm scenario + `crates/corelink-byok/benches/` |
| **E** Elevation | Worker uses byok crate to wrap arbitrary plaintext (key-leak primitive) | API surface restricted to `wrap_dek` / `unwrap_dek` over fixed `Dek` newtype (32 B); no `encrypt_arbitrary` exposed | API surface review in `crates/corelink-byok/src/lib.rs` public-export audit; cargo-public-api CI |

### 2.2 TB-byok-2 (byok → external KMS)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | MITM impersonates KMS endpoint (cert swap, DNS hijack) | TLS 1.3 + per-provider cert pinning; rustls-webpki ≥ R1-9 (F-003/F-004/F-005 fixed `77a7b3c`); CAA pin | `crates/corelink-byok-aws/tests/tls_pinning.rs` — FM-BYOK-001 |
| **T** | Provider returns tampered wrapped-DEK (downgrade attack on cipher suite) | KMS response signature (AWS SigV4 / GCP signed responses / Vault response wrapping); AAD checked client-side | adversarial regression in driver tests |
| **R** | Provider denies receiving wrap request (billing dispute on KMS spend) | CoreLink emits pre+post audit (INV-AUTH-AUDIT-PRE-POST-ORDERING analogue); cross-check vs provider invoice in monthly reconcile | INV-AUDIT-APPEND-ONLY + `crates/corelink-byok-matrix-test` |
| **I** | KMS audit log retains DEK in plaintext (provider misconfiguration) | Wrap/unwrap calls send ciphertext only; AAD is metadata (tenant_id + blob_hash hex), never plaintext; provider audit allowlist documented per ADR | LINDDUN review `specs/_audits/sealed/2026-05-14-linddun-cli-telemetry.md` |
| **D** | KMS provider regional outage | Kill-switch ≤ 5 min global hard-fail (INV-BYOK-CRYPTO-SOVEREIGNTY); DEK cache absorbs short outages within TTL; runbook `RB-BYOK-REVOKE` | `specs/03_architecture/tla+/runbooks/byok_kill_switch.tla` + `specs/_audits/sealed/2026-05-14-byok-kill-switch-drill-aws.md` — FM-BYOK-005 |
| **E** | Compromised workload identity escalates from `kms:Decrypt` to `kms:CreateGrant` | IAM policy least-privilege (`kms:Encrypt`+`kms:Decrypt` only); CMK policy denies `kms:*Grant*` to CoreLink workload role; quarterly IAM diff review | `specs/_audits/sealed/2026-05-14-pentest-s14-byok.md` |

### 2.3 TB-byok-3 (revocation propagation)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Forged revocation signal (admin pretends customer revoked) | Revocation source = KMS access-denied probe (60s) — not admin-asserted; signed kill-switch event when admin-initiated (dual-approval) | `crates/corelink-byok-revocation/src/` + `specs/_audits/sealed/2026-05-14-rb-byok-revoke-dry-run.md` |
| **T** | Cached DEK TTL extended past 5 min via clock skew | `MAX(now, prev_value)` monotonic timestamps (INV-DATA-MONOTONIC-TS); TTL bounded by both wallclock and monotonic | adversarial regression — clock-skew property test |
| **R** | Customer claims kill-switch fired but cache never evicted | Eviction emits audit event with affected tenant count; daily SLO chart (P99 propagation ≤ 5 min) | `specs/_audits/sealed/2026-05-14-byok-kill-switch-drill-aws.md` (P99 = 2m18s observed) |
| **I** | Reading kill-switch state leaks customer KMS topology | Boolean kill-state per tenant — no provider details exposed via API | API review |
| **D** | Probe storm against KMS triggers throttle, masking real revoke | Probes coalesced (single inflight per tenant); circuit breaker (PAT-CIRCUIT-001) | benches |
| **E** | Admin can bypass kill-switch via direct config flag | INV-ADMIN-DUAL-APPROVAL on kill-switch override; flag flip emits audit + alerts SEV-1 | `crates/corelink-dual-approval/tests/prop_dual_approval.rs` |

### 2.4 TB-byok-4 (admin → BYOK config)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Admin session hijack mounts CMK rotation | CTRL-AUTH-010 (WebAuthn + session bound to UA+IP+PKCE); MFA freshness ≤ 30 min (INV-ADMIN-MFA-FRESHNESS) | `crates/corelink-clerk/tests/adversarial.rs` |
| **T** | Rotation request mutated in-flight (new CMK swapped) | Dual-approval signs request hash; both signatures verified at apply time | `crates/corelink-dual-approval/tests/adversarial.rs` |
| **R** | Admin denies initiating rotation | Audit chain entry with both signer identities + WebAuthn attestation | INV-AUDIT-APPEND-ONLY |
| **I** | Config API leaks current CMK ARN to lower-privilege admin | RBAC: BYOK admin scope distinct from billing/support scopes | `specs/_audits/sealed/2026-05-14-pentest-s14-byok.md` |
| **D** | Spam rotation requests block legitimate ops | Per-admin rate limit + queueing | rate-limit tests |
| **E** | Lower-privilege admin escalates via BYOK config endpoint | Scope check on `byok:rotate` verb (CTRL-AUTHZ-001); INV-ADMIN-DUAL-APPROVAL | property test 10k attempts |

## 3. Residual risks

| RR-id | Description | Severity | Mitigation status |
|---|---|---|---|
| RR-BYOK-01 | rsa Marvin timing side-channel (RUSTSEC-2023-0071) on verify-only path | LOW (post-WAIVER F-002 — CVSS 3.7) | ADR `ADR-S20-RSA-MARVIN-MITIGATION.md` signed; migration to `aws-lc-rs` tracked R-6 |
| RR-BYOK-02 | DEK cache TTL hard at 5 min — if KMS outage exceeds and grace logic triggers, fail-CLOSED degrades read availability | MEDIUM | Documented; runbook `RB-BYOK-REVOKE`; customer SLA reflects sovereignty trade-off |
| RR-BYOK-03 | AAD canonicalization (JCS RFC 8785) relies on `serde_jcs` — supply-chain dep | LOW | INV-SUPPLY-PROVENANCE-IN-REKOR + Cargo.lock pin + cargo-deny |
| RR-BYOK-04 | Pentest-side test of cross-provider matrix (AWS+Azure+GCP+Vault hot-swap) is internal-only (`corelink-byok-matrix-test`); no external red-team verification yet | MEDIUM | Scheduled for 2026-06-15 engagement (P0 in §2 PENTEST-EVIDENCE-PACKAGE) |

## 4. Adversarial test pointers (file:line anchors)

- `crates/corelink-byok/tests/adversarial.rs` — spoof, AAD-tamper, plaintext-leak regressions
- `crates/corelink-byok/tests/prop_byok.rs` — 30k property cases AAD-tamper / cross-tenant / TTL
- `crates/corelink-byok/tests/mutation_kills.rs` — mutation-baseline kills (see `specs/_audits/sealed/2026-05-14-mutation-baseline.md`)
- `crates/corelink-byok-aws/tests/tls_pinning.rs` — TLS 1.3 + cert-pin coverage (FM-BYOK-001)
- `crates/corelink-byok-matrix-test/` — cross-provider matrix
- `crates/corelink-byok-revocation/tests/` — kill-switch propagation (FM-BYOK-005)
- `crates/corelink-byok/benches/constant_time.rs` — Mann-Whitney 3-prong timing (FM-BYOK-004)
- `specs/03_architecture/tla+/runbooks/byok_kill_switch.tla` — formal verification of revocation propagation

## 5. Cross-references

- Invariants: INV-BYOK-CRYPTO-SOVEREIGNTY, INV-ADMIN-DUAL-APPROVAL, INV-ADMIN-MFA-FRESHNESS, INV-TENANT-ISOLATION, INV-AUDIT-APPEND-ONLY, INV-DATA-MONOTONIC-TS
- Controls: CTRL-CRYPTO-002, CTRL-CRYPTO-003, CTRL-CRYPTO-005, CTRL-AUTH-010, CTRL-AUTHZ-001, CTRL-AUTHZ-002, CTRL-CRED-001, CTRL-RATE-001, CTRL-BACKOFF-001
- Failure modes: FM-BYOK-001..006 (pentest §3.3); structural overlap with FM-204 (secret rotation), FM-258 (insider exfil)
- Runbooks: `RB-BYOK-REVOKE` (`specs/_audits/sealed/2026-05-14-rb-byok-revoke-dry-run.md`); BYOK kill-switch drill `specs/_audits/sealed/2026-05-14-byok-kill-switch-drill-aws.md`
- SOC 2: CC3.2 (risk ID — encryption sovereignty), CC6.1 (logical access — KMS), CC6.7 (at-rest crypto)
