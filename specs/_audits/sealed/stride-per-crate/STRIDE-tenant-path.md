# STRIDE deep dive — `tenant-path` (HMAC tenant-prefix primitive)

> **Scope distinction:** this deep dive covers the **primitive** in `crates/tenant-path` (`derive_prefix`, key handling, property tests, fuzz). The *integration surface* (worker call sites, storage paths, metadata layer, dedup) is covered in `STRIDE-corelink-tenant-path.md`.

- **Crate:** `crates/tenant-path` (lib + `fuzz/` + `benches/`)
- **Date:** 2026-05-15
- **Owner:** Security Lead + Crypto Engineering
- **Pentest scope:** Yes — engagement 2026-06-15 (P0 surface — primitive backing INV-TENANT-ISOLATION)
- **Coarse references:** matrix-stride-ctrl.csv THR-I-001 (cross-tenant via path guess), THR-S-001 (TOKEN/HMAC), FM-253 (cross-tenant read)
- **Pentest doc cross-ref:** §3.5 Multi-tenant isolation — T row (HMAC tenant prefix)
- **SOC 2 cross-ref:** CC3.2, CC6.1, CC6.6, CC6.7

## 1. Trust boundaries

| Boundary | Caller | Callee | Auth/authz | Output |
|---|---|---|---|---|
| **TB-tpp-1** | Any in-process caller (`corelink-worker`, byok, audit, …) | `tenant_path::derive_prefix(tenant_id)` | Library call — caller pre-validated tenant_id | 16-byte prefix = `b64(HMAC(tdk, tenant_id))[:16]` |
| **TB-tpp-2** | Library → TDK (Tenant Derivation Key) secret in CF Secrets | Key fetcher | Service-binding identity | Key bytes (zeroized on drop) |
| **TB-tpp-3** | Key rotation flow (semestral) | `corelink-byok` key-rotation drivers via `key_management.md §3` | Admin + dual-approval | New TDK; both old/new tried during overlap (CTRL-KEY-005) |
| **TB-tpp-4** | Fuzz harness + property suite (CI) | `tenant_path::derive_prefix` | n/a (test) | Coverage report |

## 2. STRIDE per boundary

### 2.1 TB-tpp-1 (derive call)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Caller passes empty / null / malformed tenant_id | Input validation: tenant_id is ULID (canonical 26-char base32-crockford); reject empty / non-canonical | `crates/tenant-path/tests/prop_tenant_path.rs` |
| **T** | Different tenant_ids derive same prefix (collision) | HMAC-SHA256 first 16 bytes; collision probability 2^-64; property test 100k cases ≥ 0 collisions | `crates/tenant-path/tests/prop_tenant_path.rs` + `crates/tenant-path/fuzz/` |
| **R** | "We were given a wrong prefix" | Deterministic function over (TDK, tenant_id); reproducible via `tenant_path::derive_prefix` lib — single SoT (CTRL-ISO-001) | unit test + golden vectors |
| **I** | Timing of derive leaks tenant_id length / shape | `RustCrypto/hmac` constant-time; 16-byte truncation constant-time slice | constant-time bench |
| **D** | Excessive cost per call | HMAC-SHA256 ≤ 10 µs; called once per request and cached | benches |
| **E** | Caller bypasses derive by constructing prefix manually | Prefix type is opaque newtype (`TenantPrefix([u8;16])`); constructor private; only `derive_prefix` produces; clippy lint denies raw construction | type-system + clippy lint |

### 2.2 TB-tpp-2 (key fetch)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Rogue caller fetches TDK | Key access via CF Secrets binding — restricted to deployed Worker; key never crosses TB-1 in plaintext beyond derivation closure | binding review |
| **T** | TDK tampered (key swap) | Key versioned; version_id stored alongside prefix in D1 for cross-check; mismatch alarms | `crates/tenant-path/tests/prop_tenant_path.rs` rotation property |
| **R** | "TDK was changed without notice" | Audit emit on rotation with both versions; dual-approval signature | INV-AUDIT-APPEND-ONLY |
| **I** | TDK leaks via panic / trace | `Zeroize` on Drop; no `Debug` derive on key type; no-secret-in-log lint (CTRL-CRED-001) | `tools/pat_plaintext_lint/` |
| **D** | Key fetch latency on cold path | Key cached for Worker lifetime; refresh on signal | benches |
| **E** | Compromised Worker reads TDK to forge prefixes for other tenants | Defense-in-depth: even with TDK, attacker still needs tenant_id list (low-entropy via ULID but enumeration creates audit trail); INV-TENANT-ISOLATION 5-layer (auth interceptor → tenant resolver → RBAC → ACL → storage prefix verify) | `specs/tla/tenant_isolation.tla` |

### 2.3 TB-tpp-3 (rotation)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Forged rotation event swaps to attacker-known TDK | INV-ADMIN-DUAL-APPROVAL + WebAuthn attestation; key gen in HSM-backed CF Secret | `crates/corelink-dual-approval/tests/prop_dual_approval.rs` |
| **T** | New TDK tampered before commit | Rotation request hash + dual-sign; both checked at apply | dual-approval test |
| **R** | "Tenant data lost after rotation" | Overlap period (CTRL-KEY-005) — old + new both tried; SLO chart on rotation success rate; FM-204 mitigation | FM-204 + `key_management.md §3` |
| **I** | Rotation reveals old TDK | TDKs are write-only into Secrets; rotation tool reads only metadata (version_id) | rotation tool review |
| **D** | Rotation in flight blocks derives | Overlap period absorbs; derives try new then old | property test |
| **E** | Rotation used to permanently lock customer out | Quarterly review of rotation cadence; INV-AUDIT-RETENTION ensures history reconstructable | quarterly review |

### 2.4 TB-tpp-4 (test/fuzz)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Test harness substitutes mock derive at runtime | `cfg(test)` only; release build has no mock path; cargo-deny + clippy gate | CI build matrix |
| **T** | Fuzz seed corpus tampered to hide regressions | Corpus checked into repo + signed commits (CTRL-SUPPLY-002) | INV-SUPPLY-SIGNED-DEPLOY |
| **R** | "Fuzz never caught X" disputes | Fuzz runs published to CI artifact; coverage report in `2026-05-14-cargo-fuzz-summary-s15.md` | `specs/_audits/sealed/2026-05-14-cargo-fuzz-summary-s15.md` |
| **I** | Fuzz outputs leak prod tenant_id shape | Fuzz uses synthetic ULIDs; no prod data | fuzz/corpus review |
| **D** | Fuzz job hogs CI | Time-boxed budget per job | CI config |
| **E** | Fuzz harness writes to prod | Fuzz hermetic; no network; no prod secrets | fuzz/ review |

## 3. Residual risks

| RR-id | Description | Severity | Mitigation status |
|---|---|---|---|
| RR-TPP-01 | Collision probability 2^-64 over 16-byte prefix is acceptable for current tenant scale (target ≤ 10⁶ tenants); revisit if scale grows 10⁹ | LOW | Tracked in `architecture/`; revisit gate at 10⁵ tenants |
| RR-TPP-02 | TDK rotation cadence (semestral) — bound on key-compromise blast radius | LOW | CTRL-CRYPTO-003 + `key_management.md §3` |
| RR-TPP-03 | Defense-in-depth assumes TDK plus tenant_id enumeration creates audit trail — assumption: attacker has bounded tenant_id list | MEDIUM | INV-AUDIT-APPEND-ONLY + INV-OBS-AUDIT-CHAIN-INTEGRITY + 5-layer defense |

## 4. Adversarial test pointers

- `crates/tenant-path/tests/prop_tenant_path.rs` — 100k property cases (no collision, deterministic, rotation overlap)
- `crates/tenant-path/fuzz/` — cargo-fuzz harness (FM-156 supply-chain analog covered)
- `crates/tenant-path/benches/` — constant-time + cost benches
- `specs/tla/tenant_isolation.tla` — 5-layer defense formal model
- `specs/_audits/sealed/2026-05-14-cargo-fuzz-summary-s15.md` — fuzz baseline

## 5. Cross-references

- Invariants: INV-TENANT-ISOLATION (CRITICAL), INV-CONF-AT-REST (TDK in Secrets), INV-ADMIN-DUAL-APPROVAL (rotation), INV-AUDIT-APPEND-ONLY
- Controls: CTRL-AUTH-004, CTRL-ISO-001 (HMAC tenant prefix), CTRL-CRYPTO-002/003, CTRL-CRED-001, CTRL-KEY-005/006, CTRL-SUPPLY-002
- Failure modes: FM-253 (cross-tenant read), FM-204 (secret rotation), FM-156 (supply chain)
- SOC 2: CC3.2, CC6.1, CC6.6, CC6.7
