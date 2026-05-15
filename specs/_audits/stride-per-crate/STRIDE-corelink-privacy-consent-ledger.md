# STRIDE deep dive — `corelink-privacy-consent-ledger` (consent grant/revoke ledger)

- **Crate:** `crates/corelink-privacy-consent-ledger`
- **Date:** 2026-05-15
- **Owner:** Privacy Officer + Security Lead
- **Pentest scope:** Yes — engagement 2026-06-15 (P1 surface — GDPR Art. 7 / LGPD Art. 8 verifiability)
- **Coarse references:** matrix-stride-ctrl.csv THR-R-001..003 (Repudiation), THR-T-004 (audit tamper), FM-450 (DSR pipeline)
- **Pentest doc cross-ref:** §3.5 Multi-tenant isolation + Annex N privacy
- **SOC 2 cross-ref:** CC3.2, Privacy P1.1/P2.1/P3.1 (notice, choice, collection)

## 1. Trust boundaries

| Boundary | Caller | Callee | Auth/authz | Output |
|---|---|---|---|---|
| **TB-cl-1** | Data subject (end-user) via tenant dashboard | Consent submit endpoint → `consent_ledger::record_grant_or_revoke` | Subject authn (Clerk / tenant SSO) + locale + notice_version | Ledger entry (grant or revoke) with HMAC + signed receipt |
| **TB-cl-2** | Verifier (data subject, regulator, internal auditor) | `consent_ledger::verify(record_id)` | PAT scope `consent:verify` or subject-bound | JWS-signed proof of grant + notice_text_hash |
| **TB-cl-3** | DSR orchestrator reading consent state | `consent_ledger::query(subject_id, purpose)` | Internal — service identity | Current grant set per purpose (12 canonical purposes per `privacy_model.md §5.6.1`) |
| **TB-cl-4** | Notice publisher (legal team) | Notice version registration | Compliance scope + dual-approval | New notice_version + canonical text hash |

## 2. STRIDE per boundary

### 2.1 TB-cl-1 (record)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Forged consent on behalf of another subject | Subject authn + tenant_id scope; subject_id derived from session, not payload | `crates/corelink-privacy-consent-ledger/tests/integration_consent_lifecycle.rs` |
| **T** | Notice text mutated after grant ("we showed them X but logged Y") | INV-CONSENT-PROOF-VERIFIABLE — SHA-256 of notice + HMAC-SHA256 with HKDF info=`corelink/v1/consent-hmac`; tampering invalidates sig | `crates/corelink-privacy-consent-ledger/tests/prop_hmac_roundtrip.rs` + `crates/corelink-privacy-consent-ledger/tests/regression_notice_version.rs` |
| **R** | Subject denies granting consent | Receipt JWS-signed; chain-anchored timestamp; grant/revoke ledger symmetric (Lote 9.4 H-05) | `crates/corelink-privacy-consent-ledger/tests/regression_symmetric_schema.rs` |
| **I** | Consent ledger reveals other subjects' purposes | Tenant + subject-scoped queries; pseudonymized subject_id in non-essential paths | LINDDUN review |
| **D** | Flood of revoke storms blocks legitimate grants | Idempotency-replay protection; per-subject rate limit | `crates/corelink-privacy-consent-ledger/tests/prop_idempotency_replay.rs` |
| **E** | Grant for one purpose used to imply consent on another | Per-purpose record; 12 canonical purposes enum; legal_basis fixed per purpose (no fail-open swap) | `crates/corelink-privacy-consent-ledger/tests/regression_locale_enforce.rs` |

### 2.2 TB-cl-2 (verify)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | External party impersonates subject during verify | PAT scope `consent:verify` or subject-session bound | adversarial test |
| **T** | Verify returns mutated proof | Proof is JWS-signed (Ed25519); receiver verifies offline | `prop_hmac_roundtrip.rs` |
| **R** | "Verify said it was granted but I never granted it" | Ledger entry includes WebAuthn / session attestation at grant time; audit chain entry | INV-AUDIT-APPEND-ONLY + INV-CONSENT-PROOF-VERIFIABLE (CRITICAL) |
| **I** | Verify leaks consent history of unrelated subjects | Tenant + subject-scoped; pagination | tenant_isolation test |
| **D** | Verify storms | Per-PAT + per-subject rate limit | rate-limit |
| **E** | Verify endpoint used to grant retroactively | Read-only contract; no mutation surface | API surface review |

### 2.3 TB-cl-3 (DSR query)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | DSR caller spoofs subject | DSR orchestrator authenticated via service identity; subject_id passed from validated DSR ticket | `crates/corelink-dsr/tests/prop_dsr.rs` |
| **T** | Query result tampered | Result includes chain-head sig; deterministic over ledger entries; INV-CONSENT-PROOF-VERIFIABLE + TLA+ InvConsentSymmetry (S-11 WI-S11-008) | `specs/tla/dsr_erasure_atomicity.tla` |
| **R** | "DSR claimed consent state X but ledger says Y" | TLA+ proof of symmetry between grant/revoke ledger states; daily reconcile | `specs/tla/dsr_erasure_atomicity.tla` (InvConsentSymmetry) |
| **I** | Cross-tenant consent state leak via DSR | DSR pipeline tenant-scoped; INV-TENANT-ISOLATION | INV-TENANT-ISOLATION |
| **D** | Query path becomes hot during mass DSR | Cached per-subject snapshot; invalidated on grant/revoke | benches |
| **E** | DSR query used to flip consent | Read-only contract; flip-paths separate (TB-cl-1) | API surface review |

### 2.4 TB-cl-4 (notice publisher)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Compromised legal session pushes attacker notice | Compliance scope + INV-ADMIN-DUAL-APPROVAL; WebAuthn (CTRL-AUTH-010) | dual-approval test |
| **T** | Notice text mutated post-publish | Notice hash committed at publish; any change requires new notice_version | `regression_notice_version.rs` |
| **R** | "We never published that notice" | Audit chain entry on publish with dual signatures + locale + text hash | INV-AUDIT-APPEND-ONLY |
| **I** | Draft notice leaks before publish | Drafts in separate table with restricted scope; published version only goes to ledger | RBAC review |
| **D** | Notice publish storms invalidate cached grants | Notice rate limited; canary rollout to subjects | observability |
| **E** | Notice publish used to backdate grants | Notice version strictly forward-only; grants pin to version at time of grant | `regression_locale_enforce.rs` |

## 3. Residual risks

| RR-id | Description | Severity | Mitigation status |
|---|---|---|---|
| RR-CL-01 | Audit-emit failure during grant could leave ledger entry unanchored | LOW | Chaos test `chaos_audit_emit_failure.rs` covers retry + DLQ |
| RR-CL-02 | Locale enforcement requires notice translated per locale at publish | LOW | `regression_locale_enforce.rs` covers; legal team owns process |
| RR-CL-03 | 12 canonical purposes enum is frozen — adding purpose requires ADR + dual-approval + customer notice | LOW (by design) | Documented in `privacy_model.md §5.6.1` |

## 4. Adversarial test pointers

- `crates/corelink-privacy-consent-ledger/tests/chaos_audit_emit_failure.rs` — audit-emit chaos
- `crates/corelink-privacy-consent-ledger/tests/integration_consent_lifecycle.rs` — full lifecycle
- `crates/corelink-privacy-consent-ledger/tests/prop_hmac_roundtrip.rs` — HMAC tamper detection
- `crates/corelink-privacy-consent-ledger/tests/prop_idempotency_replay.rs` — replay
- `crates/corelink-privacy-consent-ledger/tests/regression_locale_enforce.rs` — locale enforcement
- `crates/corelink-privacy-consent-ledger/tests/regression_notice_version.rs` — notice version pinning
- `crates/corelink-privacy-consent-ledger/tests/regression_symmetric_schema.rs` — grant/revoke symmetry (H-05 Lote 9.4)
- `specs/tla/dsr_erasure_atomicity.tla` (S-11 WI-S11-008) — InvConsentSymmetry

## 5. Cross-references

- Invariants: INV-CONSENT-PROOF-VERIFIABLE (CRITICAL), INV-AUDIT-APPEND-ONLY, INV-DATA-ERASURE-COMPLETE, INV-ADMIN-DUAL-APPROVAL, INV-TENANT-ISOLATION
- Controls: CTRL-PRIV-001, CTRL-AUTH-010, CTRL-AUTHZ-001/002, CTRL-CRED-001, CTRL-SUPPLY-002
- Failure modes: FM-450 (DSR pipeline), FM-61 (Object Lock blocks redaction); pentest §3.4 audit row applies
- SOC 2 + Privacy: CC3.2, Privacy P1.1 (notice), P2.1 (choice), P3.1 (collection); GDPR Art. 7; LGPD Art. 8
