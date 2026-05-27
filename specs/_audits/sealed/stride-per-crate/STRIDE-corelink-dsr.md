# STRIDE deep dive — `corelink-dsr` (Data-Subject Request handling)

- **Crate:** `crates/corelink-dsr` (+ `corelink-erasure-attestation`)
- **Date:** 2026-05-15
- **Owner:** Privacy Officer + Security Lead
- **Pentest scope:** Yes — engagement 2026-06-15 (P1 surface — privacy compliance; GDPR Art. 17 / LGPD Art. 18)
- **Coarse references:** matrix-stride-ctrl.csv THR-I-003 (PII in log), THR-R-001 (repudiation of write), and FM-450 (DSR pipeline failure)
- **Pentest doc cross-ref:** §3.5 Multi-tenant isolation (DSR tangent) + Annex N privacy
- **SOC 2 cross-ref:** CC3.2 (privacy risk), CC6.5 (data classification), Privacy criteria P4.1/P4.2 (use/retention)

## 1. Trust boundaries

| Boundary | Caller | Callee | Auth/authz | Output |
|---|---|---|---|---|
| **TB-dsr-1** | Data subject (end user) via tenant's dashboard or direct portal | DSR submission endpoint (admin-api) → `corelink-dsr::submit_request` | Tenant-side authn (Clerk or tenant SSO) + verification challenge | DSR ticket id + receipt (signed) |
| **TB-dsr-2** | DSR orchestrator | All 12 canonical backends (Neon multi-table / Neon billing fiscal exception / R2 CAS refcount-aware / R2 AC / D1 / KV / Stripe `Customer.update` / Loki + 4 pseudonymized: R2 audit Object Lock 7y / Neon PITR backup 30d / R2 CAS legal_hold partition / R2 evidence-* buckets 7y) | Service-account identity per backend; tenant_id explicit (CTRL-AUTHZ-002) | Per-backend ack within 24h SLO; aggregated receipt |
| **TB-dsr-3** | Erasure attestation issuance | `corelink-erasure-attestation` | Internal — invoked after all backends ack | Signed attestation (Ed25519) returned to data subject |
| **TB-dsr-4** | Legal-hold check | DSR → legal-hold partition | Compliance scope; dual-approval for hold add/remove | Hold flag honored — erasure deferred |

## 2. STRIDE per boundary

### 2.1 TB-dsr-1 (submit)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Submit DSR on behalf of another data subject | Verification challenge (email magic-link / Clerk session) + tenant-side proof; tenant_id derived from authenticated session | `crates/corelink-dsr/tests/prop_dsr.rs` |
| **T** | Submitted request payload mutated (change scope from "delete me" to "delete tenant") | Request hash signed at receipt; scope enum restricted server-side (Art. 15/17/18/20 mapped to fixed verbs) | property test |
| **R** | "I never submitted that erasure" | Receipt signed with chain-anchored timestamp; audit emit pre+post | INV-AUDIT-APPEND-ONLY |
| **I** | Submit endpoint leaks other DSR tickets | Tenant + subject-scoped query; tickets paginated, server-filtered | tenant_isolation test |
| **D** | Spam DSR submissions exhaust orchestrator | Per-subject + per-tenant rate limit; queue with bounded depth + DLQ; SEV-2 on backlog | `crates/corelink-dsr/tests/prop_dsr.rs` (replay coverage) |
| **E** | DSR submission triggers cross-tenant deletion | Verb-level scope check (CTRL-AUTHZ-001); subject_id × tenant_id pair validated; INV-TENANT-ISOLATION | property test + TLA+ |

### 2.2 TB-dsr-2 (orchestrator → 12 backends)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Backend driver impersonated (rogue worker calls real backends) | Service-account identity scoped per backend; deploy signature (CTRL-SUPPLY-002) | INV-SUPPLY-SIGNED-DEPLOY |
| **T** | Backend ack tampered to skip a backend | Per-backend ack stored with hash + signed; aggregator requires all-12 acks before attestation; missing ack = SEV-1 page ≤ 5 min | `crates/corelink-dsr/tests/prop_dsr.rs` + RB-DSR-ERASURE-INCOMPLETE |
| **R** | "Backend X never received the request" | INV-AUTH-AUDIT-PRE-POST-ORDERING per backend call; PAT-RETRY-IDEMPOTENT-001 idempotency keys for replay | INV-AUDIT-APPEND-ONLY + `specs/tla/dsr_erasure_atomicity.tla` (S-11 WI-S11-008) |
| **I** | Subject-id leaked in backend audit logs (PII in log) | CTRL-PRIV-001 redact + schema allowlist; subject pseudonymized in non-essential backend logs | LINDDUN review `specs/_audits/sealed/2026-05-14-linddun-cli-telemetry.md` |
| **D** | One backend stuck → 24h SLO breach | Per-backend timeout + circuit breaker; DLQ quarantine; on-call page ≤ 5 min; runbook RB-DSR-ERASURE-INCOMPLETE | `specs/_audits/sealed/2026-05-13-rb-fm-156-dry-run.md` analogue, FM-450 |
| **E** | One backend driver gains scope on unrelated tenant | tenant_id explicit per call; backend driver least-privilege IAM | CTRL-AUTHZ-002 |

### 2.3 TB-dsr-3 (attestation)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Forged attestation served to subject | Ed25519 signed; public key in `key_management.md §3`; subject can verify offline | `crates/corelink-erasure-attestation/tests/` |
| **T** | Attestation backdated | Timestamp from chain-head (Merkle anchor); diverges from wallclock → invalid | INV-AUDIT-CHAIN-HASH-DETERMINISTIC |
| **R** | "Attestation was issued but the data still exists" | INV-DATA-ERASURE-COMPLETE TLA+ proof in `specs/tla/dsr_erasure_atomicity.tla`; E2E test EVT-042 | `specs/tla/dsr_erasure_atomicity.tla` |
| **I** | Attestation reveals scope of other subjects | Attestation contains only own subject_id (hashed) + ticket_id | unit test |
| **D** | Attestation issuance bottleneck | Async — does not block subject UX | n/a (design) |
| **E** | Attestation reuse to claim erasure that didn't happen | Attestation tied to ticket_id one-shot; revocable | unit test |

### 2.4 TB-dsr-4 (legal hold)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Anyone adds legal hold to block erasure indefinitely | Compliance scope + INV-ADMIN-DUAL-APPROVAL on hold add/remove | `crates/corelink-dual-approval/tests/prop_dual_approval.rs` |
| **T** | Hold flag flipped post-hoc to hide non-compliance | Hold ledger append-only (INV-AUDIT-APPEND-ONLY); hold added emits audit event | INV-AUDIT-APPEND-ONLY |
| **R** | "We never told the regulator about this hold" | Audit emission + retention 7y (INV-AUDIT-RETENTION) | INV-AUDIT-RETENTION |
| **I** | Hold reveals subject existence to operator (privacy) | Hold metadata pseudonymized; only compliance role sees subject mapping | RBAC review |
| **D** | Mass hold to block compliance backlog | Hold rate-limited; SEV-2 on bulk-hold pattern | observability |
| **E** | Hold bypass to erase locked record | INV-DATA-ERASURE-COMPLETE — legal hold exception is part of the model; erasure path checks hold first | TLA+ + E2E |

## 3. Residual risks

| RR-id | Description | Severity | Mitigation status |
|---|---|---|---|
| RR-DSR-01 | Stripe `Customer.update` pseudonymization not full delete (vendor constraint) | MEDIUM | Documented as fiscal/legal exception; subject informed in DPA |
| RR-DSR-02 | R2 PITR backup 30-day pseudonymized window — erasure delayed until backup roll-off | LOW | Customer-visible SLO; matches GDPR Art. 17(3) lawful retention |
| RR-DSR-03 | DSR portal Webform CSRF surface | LOW | CF Turnstile + origin check; covered in pentest §3.1 |

## 4. Adversarial test pointers

- `crates/corelink-dsr/tests/prop_dsr.rs` — submission / replay / scope-confusion
- `crates/corelink-erasure-attestation/tests/` — attestation forge / backdate
- `specs/tla/dsr_erasure_atomicity.tla` (S-11 WI-S11-008) — formal model
- RB-DSR-ERASURE-INCOMPLETE runbook dry-run

## 5. Cross-references

- Invariants: INV-DATA-ERASURE-COMPLETE (CRITICAL), INV-AUDIT-APPEND-ONLY, INV-AUDIT-RETENTION, INV-ADMIN-DUAL-APPROVAL, INV-TENANT-ISOLATION, INV-CONSENT-PROOF-VERIFIABLE
- Controls: CTRL-PRIV-001, CTRL-PRIV-DSR-RECEIPT, CTRL-AUTHZ-001/002, CTRL-AUTH-010, CTRL-SUPPLY-002
- Failure modes: FM-450 (DSR pipeline failure), FM-61 (Object Lock blocks redaction)
- SOC 2: CC3.2 + Privacy P4.1/P4.2; GDPR Art. 17; LGPD Art. 18
