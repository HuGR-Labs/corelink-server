# STRIDE deep dive — `corelink-dual-approval` (admin dual-approval workflow)

- **Crate:** `crates/corelink-dual-approval` (+ `corelink-admin-api`, `corelink-admin-dry-run`)
- **Date:** 2026-05-15
- **Owner:** Security Lead + SRE Lead
- **Pentest scope:** Yes — engagement 2026-06-15 (P0 surface — destructive op gating)
- **Coarse references:** matrix-stride-ctrl.csv THR-S-005 (admin spoof), THR-R-002 (insider denial), FM-205 (manual intervention apaga dado), FM-258 (insider exfil)
- **Pentest doc cross-ref:** §3.3 BYOK (E row — kill-switch bypass) and §3.1 Identity E row
- **SOC 2 cross-ref:** CC3.2, CC6.1 (logical access — privileged ops), CC6.3 (segregation of duties)

## 1. Trust boundaries

| Boundary | Caller | Callee | Auth/authz | Output |
|---|---|---|---|---|
| **TB-da-1** | First admin (caller) initiating destructive op via admin-api | `corelink-dual-approval::propose(op_payload)` | Clerk + WebAuthn (CTRL-AUTH-010) + INV-ADMIN-MFA-FRESHNESS 30 min | Proposal record signed by caller; pending approver |
| **TB-da-2** | Second admin (approver) reviewing proposal | `corelink-dual-approval::approve(proposal_id)` | Clerk + WebAuthn; **distinct identity** from caller; D1 hard-check `caller_id != approver_id` | Both signatures captured; op transitions to executable |
| **TB-da-3** | Executor running approved op | Backend driver (e.g., byok rotate, dsr legal-hold, gc force-sweep) | Internal — invoked after dual-sign verified | Effect + audit emit (pre+post) |
| **TB-da-4** | Dry-run / preview surface | `corelink-admin-dry-run` | Caller-only scope (no second signer needed) | Simulated effect; read-only |

## 2. STRIDE per boundary

### 2.1 TB-da-1 (propose)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Session-hijack → propose destructive op | CTRL-AUTH-010 (WebAuthn + session bound); INV-ADMIN-MFA-FRESHNESS | `crates/corelink-clerk/tests/adversarial.rs` |
| **T** | Payload mutated post-sign | Proposal canonicalized via JCS (RFC 8785) then signed; mutation invalidates sig | `crates/corelink-dual-approval/tests/prop_dual_approval.rs` |
| **R** | "I never proposed this" | Audit chain pre+post propose with WebAuthn attestation | INV-AUDIT-APPEND-ONLY |
| **I** | Proposal exposes other proposers' op details | Proposal queue scoped to platform admins; lower-privileged admins blind | RBAC review |
| **D** | Proposal flood blocks legitimate ops | Per-admin rate limit + queue depth cap | rate-limit tests |
| **E** | Caller scope-injects to propose op they shouldn't | Op kind enum; scope check at propose (CTRL-AUTHZ-001) | property test 10k attempts |

### 2.2 TB-da-2 (approve)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Approver impersonated (same human, two sessions) | WebAuthn credential bound to authenticator; second factor distinct device verified via Clerk metadata; D1 `caller_id != approver_id` hard-check + IP/UA divergence soft-check | `crates/corelink-dual-approval/tests/adversarial.rs` |
| **T** | Approver signs different payload than proposer | Approver sees and signs `BLAKE3(canonical proposal payload)`; mismatch = reject | property test |
| **R** | "I never approved this" | WebAuthn attestation + audit chain entry with both attestations | INV-AUDIT-APPEND-ONLY |
| **I** | Approver UI leaks unrelated proposals | Scoped queue + page-size cap; explicit "approve link" includes proposal_id | UI/handler review |
| **D** | Approval queue starvation (no second admin available) | On-call rotation guarantee + SEV-2 alert if queue age > SLO; emergency single-sign with break-glass audit | oncall readiness doc |
| **E** | Approver elevates their own scope while approving | Approver's scope set ≥ op's required scope; check at approve | property test 10k attempts (INV-ADMIN-DUAL-APPROVAL — 0 bypasses) |

### 2.3 TB-da-3 (execute)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Executor invoked without dual-sign | D1 hard-check at execute: row state = `APPROVED` + both sig verify; INV-ADMIN-DUAL-APPROVAL | `crates/corelink-dual-approval/tests/mutation_kills.rs` |
| **T** | Op payload mutated between approve and execute | Hash committed at approve = hash recomputed at execute | property test |
| **R** | "Op ran without anyone approving" | Pre+post audit emission with both signer identities | INV-AUDIT-APPEND-ONLY |
| **I** | Execution leaks side-effects to wrong tenant | tenant_id is part of proposal payload (signed); driver re-checks at storage call | INV-TENANT-ISOLATION |
| **D** | Long-running op blocks queue | Per-op-kind deadline + queue priority; concurrency limit per executor | benches |
| **E** | Executor escalates to perform additional unapproved op | Executor is per-op-kind function; no generic eval path | API surface review |

### 2.4 TB-da-4 (dry-run)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Dry-run identity spoofed to leak destructive op preview | Caller-only auth required; same auth profile as propose | adversarial test |
| **T** | Dry-run output tampered (false-negative on impact) | Output computed from same code path; output signed; comparable to actual execute output by hash | `crates/corelink-admin-dry-run/` |
| **R** | "Dry-run said it was safe" | Dry-run output emitted to audit + linked to subsequent propose | INV-AUDIT-APPEND-ONLY |
| **I** | Dry-run output exposes data | Output rows pseudonymized; row counts only for some ops | LINDDUN review |
| **D** | Dry-run is expensive | Same op-cost meter as actual execute; rate-limited | rate-limit tests |
| **E** | Dry-run actually mutates (bug) | Read-only contract enforced at driver level; mutation traps trigger panic + SEV-1 | RB-FM-205 |

## 3. Residual risks

| RR-id | Description | Severity | Mitigation status |
|---|---|---|---|
| RR-DA-01 | Break-glass single-sign path exists for outage scenarios (e.g., only one admin online) | MEDIUM | Audit emission + SEV-1 + post-incident review mandatory; documented in runbook |
| RR-DA-02 | Approver fatigue → rubber-stamp risk | MEDIUM | UI shows diff vs baseline + recent-similar-op count; quarterly review of approval times |
| RR-DA-03 | Insider with both proposer and approver credentials (shared device) | LOW (mitigated) | WebAuthn ties cred to authenticator; corporate policy + onboarding training; FM-258 mitigation |

## 4. Adversarial test pointers

- `crates/corelink-dual-approval/tests/adversarial.rs` — same-identity-bypass, sig forge
- `crates/corelink-dual-approval/tests/prop_dual_approval.rs` — 10k bypass attempts (INV-ADMIN-DUAL-APPROVAL = 0 bypasses)
- `crates/corelink-dual-approval/tests/mutation_kills.rs` — mutation baseline
- `specs/_audits/sealed/2026-05-14-rb-fm-205-dry-run.md` — admin-mistake runbook drill
- `specs/_audits/sealed/2026-05-14-s17-tabletop-byok-revoke.md` — dual-approval in BYOK revoke flow

## 5. Cross-references

- Invariants: INV-ADMIN-DUAL-APPROVAL, INV-ADMIN-MFA-FRESHNESS, INV-AUDIT-APPEND-ONLY, INV-TENANT-ISOLATION, INV-AUTH-AUDIT-PRE-POST-ORDERING
- Controls: CTRL-AUTH-010, CTRL-AUTHZ-001/002, CTRL-AUDIT-003 (MFA attestation + session recording), CTRL-CRED-001, CTRL-SUPPLY-002
- Failure modes: FM-205 (manual intervention), FM-258 (insider exfil), FM-201 (config rollback)
- SOC 2: CC3.2, CC6.1, CC6.3 (segregation of duties)
