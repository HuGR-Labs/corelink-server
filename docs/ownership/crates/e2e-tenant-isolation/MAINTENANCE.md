---
schema: corelink-ownership/1.1
document: maintenance
package: e2e-tenant-isolation
manifest: tests/e2e-tenant-isolation/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: e2e-tenant-isolation-static-1177dad2c
---

# e2e-tenant-isolation — maintenance guide

This package record is static/documentary. The current authoring task permits only these four ownership artifacts and their structural checks. It does not run Cargo, Rust, tests, fuzzing, network, GitHub, providers, deploys, production, database, or storage operations.

[Preparation](#m01) · [Choose](#m02) · [Procedures](#m03) · [Test matrix](#m04) ·
[Recovery](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Safe preparation

| Mode | Preconditions | Stop |
|---|---|---|
| READ_ONLY | Use the pinned source commit in the task packet; inspect only the named manifest/source/config paths | Pin or assigned files differ, source requires Cargo resolution, or a runtime claim is required |
| LOCAL_ISOLATED | Isolated author worktree; edit only the four assigned ownership paths; use the supplied CO-1 v1.3 candidate checker | Scope expands, an external operation is needed, or checker identity/hash is not supplied and verified |

The present source pin is 1177dad2ca2a9f21c29b5a118aa7944b77147798. The assigned integration baseline was ab7137cd178f0e6cb282f3e944f9f7b58d0f5540; the root Cargo.toml and package tree have no content diff between them. Recheck only those scoped paths when resuming this assignment. The candidate standard is not frozen or repository-integrated; follow docs/ownership/WAVE_015_PLAN.md for its supplied archive identity. Do not hardcode an extraction path as package policy.

For the documentary checker, obtain CHECKER from the lead's controlled extraction, confirm it names `tools/check_docs.py` in the supplied candidate, and verify SHA-256 `84d093bc9e95b79644fac56ca35e34b4e0ced9cd2b24ec1704485343236585b8`. Do not substitute an older embedded copy. The checker is an external, unintegrated candidate; its identity and output are evidence, not approval.

<a id="m02"></a>
## M02 — Procedure selection

| Situation | Procedure | Mode | Effect |
|---|---|---|---|
| Recheck source pin, package identity, target declarations | [PROC-001](#proc-001) | READ_ONLY | Read Git objects only |
| Trace fake, scenario, and relation predicates | [PROC-002](#proc-002) | READ_ONLY | Read pinned source only |
| Revise docs after a separately authorized package/API source diff | [PROC-003](#proc-003) | LOCAL_ISOLATED | Change only the four ownership files |
| Validate this documentary handoff | [PROC-004](#proc-004) | LOCAL_ISOLATED | Python structural checks and scoped whitespace check |
| Run package code, a provider, or production operation | No procedure here | — | Stop; the wave does not authorize it |

<a id="m03"></a>
## M03 — Bounded procedures

**Procedure record states:** all review_status values are BLOCKED pending independent cold review. PROC-001/002 and PROC-004 were executed locally for this authoring snapshot; PROC-003 was not applicable and was not executed. These are author execution states, not approval of the artifacts. **Index:** [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004).

<a id="proc-001"></a>
### PROC-001 — Reconcile pinned identity and source drift

**Objective/trigger:** confirm the assigned identity and that scoped source files match the pin. **Preconditions:** exact source and integration baseline from the task. **Mode/environment/permissions:** READ_ONLY; isolated Git worktree; no Cargo or network. **Inputs:** root Cargo.toml and tests/e2e-tenant-isolation at both commits.

1. Read package name, autotests, library, direct dependency, dev dependency, and explicit target declarations from the pinned manifest.
2. Confirm root workspace membership at the pinned source.
3. Compare the root manifest and package tree between source pin and integration baseline; require an empty scoped diff.

**Expected predicate:** package/target facts match and scoped diff is empty. **Failure/stop:** any mismatch blocks authoring and requires lead reconciliation. **Recovery:** update only evidence after the source pin is formally corrected. **Evidence:** exact SHAs, scoped paths, command exit/result. **Review:** BLOCKED. **Execution:** EXECUTED_LOCAL; result PASS. **required_for_acceptance:** true.

[Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Trace a scenario and its fake boundary

**Objective/trigger:** support an API, relation, or invariant statement. **Preconditions:** pinned package sources are available. **Mode/environment/permissions:** READ_ONLY; local Git object/source view; no code execution. **Inputs:** one named scenario, fake implementation, imported API, and any verifier literal.

1. Trace the test from setup through the fake call to the asserted result.
2. Trace any record_deny call separately from the operation returning an error.
3. Record what source proves, what remains unknown, and whether independent text contradicts the source.

**Expected predicate:** one source-falsifiable claim with producer, consumer, boundary, and exact source anchors. **Failure/stop:** missing consumer, fake/provider conflation, or an unresolved verifier mismatch. **Recovery:** narrow the claim and retain UNKNOWN; route unresolved verifier ownership to the lead. **Evidence:** file/line anchors, scenario ID, affected REL/INV. **Review:** BLOCKED. **Execution:** EXECUTED_LOCAL; result PASS for this static trace, with the S04/S05 audit boundary retained. **required_for_acceptance:** true.

[Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Update ownership docs after a source contract change

**Objective/trigger:** a separate approved source task changes this package manifest, target, or imported API. **Preconditions:** reviewed source diff and named source owner. **Mode/environment/permissions:** LOCAL_ISOLATED; documentation worktree; no source edit or Cargo command here. **Inputs:** new source SHA, exact diff, affected scenarios and dependency owners.

1. Reconcile manifest declarations and target path.
2. Re-trace changed call sites, dependent fakes, and external literal consumers.
3. Update only the four ownership artifacts and preserve unresolved runtime/CI questions.
4. Run PROC-004 and return changed hashes for cold review.

**Expected predicate:** all changed claims cite the new source and no unsupported compatibility claim remains. **Failure/stop:** no reviewed diff, unknown target, or owner disagreement. **Recovery:** wait for the source owner or leave the docs unchanged. **Evidence:** source SHA/diff, exact four paths, checks. **Review:** BLOCKED. **Execution:** REVIEWED_NOT_EXECUTED; result NOT_EXECUTED. **required_for_acceptance:** false for this snapshot; no source contract change was assigned.

[Procedure index](#m02)

<a id="proc-004"></a>

### PROC-004 — Validate documentary artifacts

**Objective/trigger:** four ownership files are ready for local structural validation. **Preconditions:** CHECKER is lead-supplied candidate tools/check_docs.py; four-path scope is verified. **Mode/environment/permissions:** LOCAL_ISOLATED; Python with candidate dependencies present; no Cargo/Rust/network.

1. Run check_docs.py once per artifact with its exact kind, profile S, and --root . as listed in M04.
2. Run the scoped git diff --check command in M04.
3. Record all four metrics/verdicts, diff exit, and paths; a checker PASS is not semantic review.

**Expected predicate:** each checker verdict is IMPLEMENTED_CHECKS_PASS and diff check exits 0. **Failure/stop:** missing dependency, checker mismatch, overflow, broken local link, or any out-of-scope path. **Recovery:** fix only assigned documentation and rerun all affected checks. **Evidence:** candidate identity, exact commands/output, source pin, worktree SHA, exact path list. **Review:** BLOCKED pending cold review. **Execution:** EXECUTED_LOCAL; result PASS after the four commands below. **required_for_acceptance:** true.

[Procedure index](#m02)


<a id="m04"></a>
## M04 — Exact test and validation matrix

Package selection for all scenario rows: e2e-tenant-isolation; target adversarial; package feature arguments none (the manifest has no [features] table); environment is host-local in-memory fixtures. Exact future code command: cargo test --locked --offline -p e2e-tenant-isolation --test adversarial. It is documented, not run. The manifest target is explicit; resolved dependency features and prior CI results are unknown.

| Scenario | Static predicate expected if run | Exact source |
|---|---|---|
| S01 | CAS read across tenant prefix is denied and a matching audit attempt is asserted | adversarial.rs::s01_cas_read_other_tenant_denied_and_audited |
| S02 | CAS write to another tenant prefix is denied; matching prefix path is not a denial | adversarial.rs::s02_cas_write_to_other_tenant_prefix_denied |
| S03 | LIST scoped to either prefix exposes only that tenant's entries | adversarial.rs::s03_list_enumeration_does_not_leak_other_tenant |
| S04 | BYOK decrypt under wrong tenant AAD returns AadMismatch; no audit assertion in this scenario | adversarial.rs::s04_byok_dek_wrap_with_wrong_aad_rejected |
| S05 | Tampered/cross-tenant envelope returns AadMismatch, then the caller explicitly records a deny | adversarial.rs::s05_byok_envelope_tamper_rejected_with_audit |
| S06 | Cross-tenant audit query without valid dual approval is rejected | adversarial.rs::s06_audit_cross_tenant_query_requires_dual_approval |
| S07 | Conflicting D1 body tenant is rejected; fake row key uses JWT tenant | adversarial.rs::s07_d1_row_spoofing_rejected_jwt_wins |
| S08 | Same idempotency key is independent across tenants; same-tenant fingerprint conflict denies | adversarial.rs::s08_idempotency_key_collision_is_independent_per_tenant |
| S09 | Exhausting Tenant A quota does not consume Tenant B's independent quota | adversarial.rs::s09_quota_crosstalk_isolated |
| S10 | Tenant A rate limit does not deny Tenant B | adversarial.rs::s10_rate_limit_crosstalk_isolated |
| S11 | Reused Stripe event across tenants is rejected; same-tenant replay follows its asserted predicate | adversarial.rs::s11_stripe_webhook_replay_cross_tenant_rejected |
| S12 | PAT for A cannot authorize access to B's resource; denial attempts are checked | adversarial.rs::s12_pat_cross_tenant_use_rejected |
| S13 | Existing and missing tenant probes use the fake's constant-time comparison/latency-floor result shape | adversarial.rs::s13_timing_oracle_constant_time_auth_probe |
| S14 | Cache entry lookup remains prefix-scoped across tenant attempts | adversarial.rs::s14_cas_cache_poisoning_cross_tenant_isolated |
| S15 | Rotation reads accept committed/in-flight versions defined by the fake and reject the half-state probe | adversarial.rs::s15_cmk_rotation_race_no_half_state |
| S16 | Authorization at/after revoke time is denied by the logical-time fake | adversarial.rs::s16_pat_revoke_toctou_no_window |
| S17 | Mixed-case idempotency keys remain byte-exact and tenant-scoped under asserted cases | adversarial.rs::s17_idempotency_collision_mixed_case_cross_tenant |
| S18 | A forged audit-chain leaf is rejected under the other tenant's chain | adversarial_tail.rs::s18_audit_chain_leaf_forge_rejected |
| S19 | Routing to a region outside a tenant's pin is rejected | adversarial_tail.rs::s19_cross_region_replay_residency_enforced |
| S20 | DSR request tenant/principal mismatch is rejected | adversarial_tail.rs::s20_dsr_cross_tenant_submission_rejected |
| S21 | Exhausting one child quota does not affect its sibling | adversarial_tail.rs::s21_quota_inheritance_siblings_isolated |
| S22 | Forged cross-tenant multipart upload access is rejected | adversarial_tail.rs::s22_multipart_upload_cross_tenant_forge_rejected |
| S23 | Cross-account Stripe webhook spoof/replay is rejected | adversarial_tail.rs::s23_stripe_webhook_cross_account_spoof_rejected |
| S24 | A stale region fails closed while PAT revocation has not replicated | adversarial_tail.rs::s24_kv_partition_pat_revoke_fail_closed |
| S25 | Caller-supplied audit tenant filter cannot override requester scope | adversarial_tail.rs::s25_audit_query_injection_rejected |

| Change type | Package / target / features | Exact command | Expected predicate / execution state |
|---|---|---|---|
| Scenario/harness source change | e2e-tenant-isolation / adversarial / no package feature arguments | cargo test --locked --offline -p e2e-tenant-isolation --test adversarial | The affected scenario assertions pass; NOT EXECUTED in this task |
| Target/API lint review | e2e-tenant-isolation / all declared targets / no package feature arguments | cargo clippy --locked --offline -p e2e-tenant-isolation --all-targets -- -D warnings | No warnings for selected targets; NOT EXECUTED in this task |
| Ownership documentation | Four assigned files / profile S | See the four python3 CHECKER commands below | All return IMPLEMENTED_CHECKS_PASS; executed locally |
| Whitespace/scope | Four assigned files only | git diff --check 1177dad2ca2a9f21c29b5a118aa7944b77147798 -- .claude/skills/own-e2e-tenant-isolation/SKILL.md docs/ownership/crates/e2e-tenant-isolation/REFERENCE.md docs/ownership/crates/e2e-tenant-isolation/BLAST_RADIUS.md docs/ownership/crates/e2e-tenant-isolation/MAINTENANCE.md | Exit 0; executed locally |

The ownership checks use CHECKER from the controlled candidate extraction. The immutable checker identity is the SHA-256 above; record the exact path, hash, four exit statuses, and output metrics in the handoff:

```sh
python3 "$CHECKER" .claude/skills/own-e2e-tenant-isolation/SKILL.md --kind skill --profile S --root .
python3 "$CHECKER" docs/ownership/crates/e2e-tenant-isolation/REFERENCE.md --kind reference --profile S --root .
python3 "$CHECKER" docs/ownership/crates/e2e-tenant-isolation/BLAST_RADIUS.md --kind blast_radius --profile S --root .
python3 "$CHECKER" docs/ownership/crates/e2e-tenant-isolation/MAINTENANCE.md --kind maintenance --profile S --root .
```

Expected output for each command is `IMPLEMENTED_CHECKS_PASS`; this is a structural result only. The code commands are future package gates only. Do not run them for this documentation task. No Rust/test result, CI result, or runtime evidence is recorded here.

Author-side structural result with checker SHA `84d093bc9e95b79644fac56ca35e34b4e0ced9cd2b24ec1704485343236585b8`: skill `IMPLEMENTED_CHECKS_PASS` (93 lines / 915 words / 6648 bytes), reference `IMPLEMENTED_CHECKS_PASS` (156 / 1732 / 14002), blast `IMPLEMENTED_CHECKS_PASS` (222 / 2323 / 18622), and maintenance `IMPLEMENTED_CHECKS_PASS` (193 / 2603 / 20599). The pinned-baseline `git diff --check` exited 0. These outputs are immutable evidence for this candidate only; they are not semantic approval or cold review.

### Verifier matrix (source-only)

| Verifier | Endpoint/source | State at pin | Required handoff |
|---|---|---|---|
| `verify_b126_t3_refactor.py` | package `adversarial.rs` + included `adversarial_tail.rs` | NOT_EXECUTED; source population is documented in REL-005 | Run only with its own owner and record output; no Cargo inference |
| `verify_b270_b281_bundle_repairs.py` | package fake source files | NOT_EXECUTED; static marker contract in REL-006 | Reconcile required/forbidden imports before relying on it |
| `verify_b291_b293_bundle_residuals.py` | `adversarial_tail.rs` import marker | NOT_EXECUTED; source contradicts required `use e2e_tenant_isolation::*` | Preserve UNKNOWN and route to verifier owner |
| `verify_b294_b296_bundle_residuals.py` | `adversarial_tail.rs` import marker | NOT_EXECUTED; source contradicts required `use e2e_tenant_isolation::*` | Preserve UNKNOWN and route to verifier owner |
| `verify_b297_b312_bundle_residuals.py` | `adversarial.rs` import endpoint plus 15 unrelated package endpoints | EXECUTED_LOCAL; `BROKEN` at B297 because expected 12 imports but source has 19 | Preserve exact output; do not call it a Rust/test failure |

The B297 output is part of the evidence set: `B-297..B-312 BROKEN: B-297 imports: expected exact 12-symbol set, got ['AuditCapture', 'AuditChain', 'AuditQueryEngine', 'CasStore', 'CmkRotationLedger', 'ConstantTimeAuthProbe', 'DenyKind', 'DsrIntake', 'HierarchicalQuotaStore', 'IdempotencyStore', 'KvReplicatedPatStore', 'MultipartBroker', 'PatRevokeLedger', 'PatStore', 'QuotaStore', 'RateLimiter', 'RegionRouter', 'StripeWebhookLedger', 'TenantCtx']`. The verifier's other endpoints are outside this package's ownership and are not silently certified by its population count.

<a id="m05"></a>
## M05 — Recovery and compatibility

The harness fakes own no documented persistent state, wire format, database schema, or deployed service. Their maps, mutexes, captures, and ledgers are process-local source fixtures. Resetting a newly constructed fake is not recovery of production state. The three upstream first-party dependencies may have their own durable or wire contracts; compatibility must be handled by those owners.

| Surface | Reversibility / compatibility | Safe recovery boundary | Evidence before change |
|---|---|---|---|
| Four documentation files | Reversible by a corrective patch or revert of this task's own commit; does not affect harness/runtime | Restore only this task's files; do not reset the shared branch | Exact paths, diff, checker output, new cold review for changed bytes |
| Fixture or fake source | Local source change can be reverted; assertion coverage may change immediately in later selected runs | Separate authorized code task; compare affected fake state/predicates and the target | Source diff, API/INV/REL IDs, separate owner-approved code validation |
| Manifest, target, or dependency | Reversible as source text, but may change compilation/selection and workspace gates; resolved impact unknown here | Separate package-owner source change, followed by docs update through PROC-003 | Reviewed manifest diff, target/import census, affected workspace gates, compatibility owner |
| tenant-path, audit, or BYOK API | Compatibility belongs to provider package and consumers; this package can expose a compile/assertion break only | Coordinate with the source package owner; do not infer downstream production rollback from this harness | Provider API/version evidence and explicit consumer/source diff |

The test target's in-memory state has no production rollback path. A passing future target would not reverse a database, storage, key, or provider action; no such operation is represented or authorized. If a source change needs live-state recovery, stop and route to the actual composition owner, which is unknown in this record.

<a id="m06"></a>
## M06 — Escalation and handoff

| Condition | Route | Minimum evidence | Stop |
|---|---|---|---|
| Tenant-path, audit, or BYOK API question | Corresponding source package owner; audit and BYOK ownership docs exist; tenant-path owner route is not in this package record | Exact dependency surface and source commit | Do not assign ownership to this harness |
| Two residual verifier literals conflict with adversarial_tail source | Ask repository lead to identify verifier owner | Both expected literals and pinned source line 15; no execution claim | Do not run or edit either side under this package task |
| Production composition or data/provider recovery requested | Composition/system owner not identified here; ask repository lead to route | Affected real resource and owner-supplied recovery plan | No production, DB, storage, key, deploy, or provider action |
| Documentary handoff ready | Independent cold reviewer, then lead integration | Four final paths/hashes, source pin, checks/metrics, scope diff, unknowns | Author does not approve own artifacts |

Record every PROC with separate review status, execution status, result, evidence, limitation, and required_for_acceptance. Keep the five WAVE_015_PLAN acceptance axioms: success, completeness, quality, definition of done, and invariants. Reconcile only the affected package artifacts and backlog through the lead; do not edit global index/registry/rollout files in this author scope.

Current handoff: source pin 1177dad2ca2a9f21c29b5a118aa7944b77147798; integration baseline ab7137cd178f0e6cb282f3e944f9f7b58d0f5540; scope exactly the four paths listed in this guide. Structural checks do not replace independent cold review.

[Reference](REFERENCE.md#r01) · [Impact map](BLAST_RADIUS.md#b01) · [Start](#m01).
