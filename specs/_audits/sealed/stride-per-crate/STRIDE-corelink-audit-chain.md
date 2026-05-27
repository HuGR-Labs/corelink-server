# STRIDE deep dive — `corelink-audit-chain` (Merkle / hash-chain audit log)

- **Crate:** `crates/corelink-audit-chain` (+ `corelink-audit`, `corelink-client-verify`)
- **Date:** 2026-05-15
- **Owner:** Security Lead + Compliance
- **Pentest scope:** Yes — engagement 2026-06-15 (P0 surface — tamper-evidence is foundational for SOC 2 / LGPD repudiation defenses)
- **Coarse references:** matrix-stride-ctrl.csv THR-T-004 (AST-AUDIT, TB-5), THR-R-001..003 (Repudiation)
- **Pentest doc cross-ref:** §3.4 Audit chain (tamper-evident integrity)
- **SOC 2 cross-ref:** CC3.2 (risk ID — tamper-evidence), CC7.2 (system monitoring), CC4.1 (control monitoring)

## 1. Trust boundaries

| Boundary | Caller | Callee | Auth/authz | Output |
|---|---|---|---|---|
| **TB-audit-1** | All write-path crates (worker, byok, dsr, dual-approval, stripe, …) emitting `AuditEvent` | `corelink-audit-chain::append(event)` | Internal call — caller-side already authenticated; tenant_id explicitly passed (CTRL-AUTHZ-002) | Event signed (Ed25519) + chained (BLAKE3 over JCS canonicalization, RFC 8785) into per-tenant Merkle tree; persisted D1 + R2 Object Lock |
| **TB-audit-2** | Storage layer (D1 + R2) | Storage drivers | R2 Object Lock COMPLIANCE mode (7y); D1 CHECK constraint deny UPDATE/DELETE | Immutable record |
| **TB-audit-3** | Daily verifier cron + on-demand `client-verify` | Audit-chain read API | Background-cron identity (scoped IAM); `client-verify` exposes proof to customer with `audit:read` scope | Chain-integrity result + Merkle inclusion proof |
| **TB-audit-4** | External verifier (customer SDK or auditor) | Public verify endpoint | PAT scope `audit:verify`; tenant-scoped | Inclusion proof (RFC 6962 §2.1 leaf/inner discrimination); chain-head signature |

## 2. STRIDE per boundary

### 2.1 TB-audit-1 (emit)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Worker emits event under wrong tenant_id (spoof) | CTRL-AUTHZ-002 explicit tenant_id + storage-side assertion; INV-TENANT-ISOLATION 5-layer | `crates/corelink-audit-chain/tests/prop_audit_chain.rs` |
| **T** | Event signature forged at emit | Ed25519 signing key per region in HSM-backed CF Secrets; INV-AUDIT-CHAIN-HASH-DETERMINISTIC (JCS) | `crates/corelink-audit-chain/tests/` forge regression — FM-AUDIT-002 |
| **R** | Caller never emits (silent action) | INV-AUTH-AUDIT-PRE-POST-ORDERING — middleware emits pre+post; missing post = SEV-2 alert via heartbeat | `crates/corelink-clerk/tests/adversarial.rs` (pre/post ordering) |
| **I** | Audit event contains PII / secret accidentally | CTRL-PRIV-001 redact + schema allowlist; no-secret-in-log lint | LINDDUN audit + `tools/pat_plaintext_lint/` |
| **D** | Emit storm fills D1 → blocks legitimate writes | Per-tenant emit rate cap; spill to R2 NDJSON with backpressure (PAT-BULKHEAD-001) | `specs/_audits/sealed/2026-05-14-property-test-summary-s19.md` |
| **E** | Emit API used to inject crafted event with attacker tenant_id | tenant_id derived from caller context (Worker request `tid`), never from event payload | property test |

### 2.2 TB-audit-2 (storage)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Insider replaces R2 bucket with controlled bucket | R2 bucket ARN pinned in Terraform + signed deploy verify (CTRL-SUPPLY-002); INV-SUPPLY-SIGNED-DEPLOY | terraform drift detect (FM-206) |
| **T** | Insider rewrites D1 row directly via Cloudflare API | INV-AUDIT-APPEND-ONLY enforced at D1 CHECK constraint (`UPDATE` and `DELETE` rejected on `audit_log` table) + R2 Object Lock COMPLIANCE 7y | `specs/tla/audit_immutability.tla` (Lote 6.2) — FM-AUDIT-003 |
| **R** | Operator claims chain was always broken (cover-up by silent truncation) | Daily `PAT-AUDIT-VERIFY-001` cron + Merkle root anchored across regions; heartbeat dead-man switch | `RB-FM-AUDIT-BREAK` dry-run + INV-OBS-AUDIT-CHAIN-INTEGRITY |
| **I** | Cross-tenant audit read via D1 query | Neon RLS + tenant-scoped views; storage layer filters at driver | `specs/tla/tenant_isolation.tla` |
| **D** | R2 Object Lock storms (legal-hold pin storms) block writes | Pre-flight cap on legal-hold add rate; runbook FM-061 | runbook tests |
| **E** | Privilege escalation via storage driver (raw SQL passthrough) | INV-INPUT-002 parameterized queries; `sqlx::query!` macros only | clippy + cargo-deny |

### 2.3 TB-audit-3 (verifier + client-verify)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Forged verifier identity emits "all-good" signal | Verifier cron runs in CF Workers with scoped IAM; result is signed by independent key; cross-region verifier diff | RB-FM-AUDIT-BREAK |
| **T** | Verifier code tampered to skip checks | INV-SUPPLY-SIGNED-DEPLOY + reproducible build (CTRL-SUPPLY-008); cargo-fuzz coverage | `specs/_audits/sealed/2026-05-14-cargo-fuzz-summary-s15.md` |
| **R** | Customer claims they were never alerted on break | Alerts go to multiple channels + SEV-1 PagerDuty + customer status page entry; immutable alert log | oncall readiness `specs/_audits/sealed/2026-05-14-s20-oncall-24-7-readiness.md` |
| **I** | Inclusion proof leaks unrelated event existence | RFC 6962 §2.1 leaf/inner discrimination (`BLAKE3(0x00 ‖ data)` vs `BLAKE3(0x01 ‖ left ‖ right)`); proof contains only sibling hashes | `crates/corelink-client-verify/fuzz` — FM-AUDIT-007 |
| **D** | Verify endpoint flooded; legitimate auditor blocked | Per-tenant rate limit + priority queue for audit:verify scope | k6 audit-flood scenario |
| **E** | Verifier path used to read other tenants' chain | PAT `audit:verify` scope is tenant-scoped; INV-TENANT-ISOLATION | property test |

### 2.4 TB-audit-4 (external verifier)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | External party impersonates customer auditor | PAT scope `audit:verify`; HMAC sig (CTRL-AUTH-001/004) | `crates/corelink-pat/tests/adversarial.rs` |
| **T** | Returned proof tampered in transit | TLS 1.3 + proof itself self-verifying (Merkle path) | INV-CONF-IN-FLIGHT |
| **R** | Customer accepts forged proof | Chain head signed by Ed25519 key whose pubkey is published + rotated via key_management.md; external auditor verifies signature offline | `specs/03_architecture/key_management.md §3` |
| **I** | Side-channel reveals event count of other tenants | Tenant-scoped query; size of proof depends only on own tree depth | timing benches |
| **D** | Sweep of verify calls (mass enumeration) | Per-PAT rate limit + cost-per-call metering | rate-limit tests |
| **E** | Bug in client-verify allows forging proof on customer side | `corelink-client-verify` exposes only verify (no construct); fuzz-tested | `crates/corelink-client-verify/fuzz` |

## 3. Residual risks

| RR-id | Description | Severity | Mitigation status |
|---|---|---|---|
| RR-AUDIT-01 | R2 Object Lock COMPLIANCE mode is irreversible — bug emitting bad-but-valid events forces compensation entries forever | LOW (by design) | Documented in FM-061 + RB-GDPR-ERASURE-HOLD legal path |
| RR-AUDIT-02 | Chain-head Ed25519 key rotation overlap period (CTRL-KEY-005) allows brief window of two valid signers | LOW | INV-AUDIT-CHAIN-HASH-DETERMINISTIC + rotation playbook in key_management.md §3 |
| RR-AUDIT-03 | Cross-region verifier divergence not yet exercised in adversarial pentest | MEDIUM | Scheduled for 2026-06-15 (annex D in PENTEST-EVIDENCE-PACKAGE) |

## 4. Adversarial test pointers

- `crates/corelink-audit-chain/tests/prop_audit_chain.rs` — emit / chain integrity
- `crates/corelink-audit-chain/tests/mutation_kills.rs` — mutation baseline
- `crates/corelink-client-verify/fuzz/` — RFC 6962 leaf/inner discrimination fuzz (FM-AUDIT-007)
- `specs/tla/audit_immutability.tla` — append-only formal model
- `specs/_audits/2026-05-14-rb-fm-303-dry-run.md` / RB-FM-AUDIT-BREAK dry-runs
- Daily cron `PAT-AUDIT-VERIFY-001` (INV-OBS-AUDIT-CHAIN-INTEGRITY)

## 5. Cross-references

- Invariants: INV-AUDIT-APPEND-ONLY, INV-AUDIT-RETENTION, INV-AUDIT-CHAIN-HASH-DETERMINISTIC, INV-OBS-AUDIT-CHAIN-INTEGRITY, INV-AUTH-AUDIT-PRE-POST-ORDERING, INV-TENANT-ISOLATION
- Controls: CTRL-AUDIT-001..004, CTRL-PRIV-001, CTRL-CRYPTO-001, CTRL-INPUT-002, CTRL-SUPPLY-002, CTRL-KEY-005/006
- Failure modes: FM-061 (Object Lock blocks redaction), FM-304 (chain corruption), FM-AUDIT-001..007 (pentest §3.4)
- SOC 2: CC3.2, CC4.1, CC7.2
