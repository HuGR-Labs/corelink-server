# B231–B251 canonical closure packet

This packet is authored from the exact D03 base
`38104f666dc5ad4f2bedfb5901011dc7be01f5ac` in the isolated worktree
`/tmp/corelink-b231-b251-author.XSnYoI`. It is limited to B231–B251; it does
not renumber, delete, or reinterpret any other backlog item.

The disposition rule is strict: `done` requires the repository-local verifier
to be green and to contain a real fail-closed negative/mutation guard. A green
static marker alone never closes an item. Runtime, external, and owner actions
remain `open` and are not converted into repository claims.

| ID | disposition | evidence / remaining boundary |
|---|---|---|
| B-231 | done | `verify_b231_b243_contracts.py`; pricing/tier source contract and shared fail-closed mutation guard. |
| B-232 | done | same verifier; physical `_public` and authenticated accounting tenant are separated, with anti-spoof/round-trip/quota mutation guards. |
| B-233 | done | same verifier; Brew/Pip resolve the tenant cap and write through the bounded tenant path. |
| B-234 | done | same verifier; refund dispatch and fully-refunded materialization contract are present. |
| B-235 | done | same verifier; production wiring excludes the volatile DLQ and preserves durable-D1 boundary. |
| B-236 | done | same verifier; secret matrix/workflow contract and code-only classification are guarded. |
| B-237 | done | same verifier; five-second PAT cache, token-row invalidation, and response-header stripping are guarded. |
| B-238 | done | same verifier; six-region enum and residency-read-before-delete ordering are guarded. |
| B-239 | done | same verifier; canonical DPA notice hash and mismatch rejection are guarded. |
| B-240 | done | same verifier; bounded negative `kid` cache and second-refresh behavior have adversarial guards. |
| B-241 | done | same verifier; resolver bounds and primary-fabric-only authorization are guarded. |
| B-242 | done | same verifier; all five salt destinations and signup-worker verification are guarded. |
| B-243 | done | same verifier; githugr-only writer and migration boundary are guarded. |
| B-244 | done | `verify_b244_oci_lock_scope.py`; focal tenant-scoped lock plus 4/4 mutations rejected. |
| B-245 | done | Matrix rows 242–243 classify `CORELINK_FRESH_SESSION` and `CORELINK_PERF_PAT` as test/perf-only GitHub Actions credentials with exact workflow/collector consumers; `validate_secrets_matrix.py` reports `code_only=0`, and `verify_b245_secrets_matrix.py` rejects four bounded mutations. |
| B-246 | done | `verify_b246_webauthn_otp.py`; 100-cycle test-only seam, production-cost smoke, and mutation guards pass. |
| B-247 | done | `verify_b247_clerk_proptest.py`; cached test key, real RSA-2048 smoke, 11/11 mutations rejected. |
| B-248 | done | `verify_dpa_acceptance_fixture.py`; shared `OnceLock` fixture and bounded 32-case contract pass. |
| B-249 | done | `verify_b249_dt_webhook_sunset.py`; injected clock, day-90/day-91 boundary, workflow and mutation guards pass. |
| B-250 | open / external + owner | the 278-run deleted-workflow population is an external control-plane observation; owner must identify/disable the stale emitter and retain authorized GitHub evidence. No local verifier may manufacture closure. |
| B-251 | open / external | deterministic quota-CAS budget/correctness replacement is locally verified, but the isolated real 1,000-sample p99 probe remains opt-in runtime evidence. The static gate does not claim that measurement. |

## Open-action packets

### B-245 — secret-matrix closure evidence

`CORELINK_FRESH_SESSION` and `CORELINK_PERF_PAT` are consumed only by
`.github/workflows/perf-production-evidence.yml` and the bounded collectors
`scripts/collect_b102_b107_measurements.py` (both) and
`scripts/collect_b105_same_lane.py` (`CORELINK_PERF_PAT`). Rows 242–243 record
that they are owner-provisioned test/perf credentials, never deployment or
production-runtime secrets, and neither name is globally allowlisted.

The local evidence is:

```sh
python3 scripts/verify_b245_secrets_matrix.py
python3 scripts/validate_secrets_matrix.py
```

The validator reports `matrix=239 code=202 in_both=202 matrix_only=37
code_only=0`. The focal verifier rejects missing, moved, and extra consumers
plus matrix-row removal; any future use outside the exact scope remains
fail-closed. B-250 and B-251 remain open for their external/runtime evidence.

### B-250 — deleted-workflow external action

Run `python3 scripts/b250_deleted_workflow_startup_failure.py` only with the
authorized GitHub/API context, preserve its redacted report and run IDs, then
identify and disable/correct the stale `BuildFailed` emitter through the
authorized administrative path. Re-run the bounded verifier after the change.
The closed-window population, zero jobs, rerun, or an unrelated green workflow
does not prove remediation. Do not conflate this item with B-152.

### B-251 — runtime measurement

The local gate proves deterministic allow/deny correctness, bounded attempts,
bounded audit events, and that weakening mutations turn red. The ignored
`real_latency_probe_under_5ms_p99` is the only place that measures wall time;
run it in the D03 bundle runtime lane and attach the run, environment, sample
count, and p99 result. Until that packet exists, B-251 remains open.

## Required local gate set

For this packet, the per-ID backlog schema gate is run with
`python3 scripts/backlog_verify.py --id B-NNN`. The focused gates are:

```sh
python3 scripts/verify_b231_b243_contracts.py
python3 scripts/verify_b244_oci_lock_scope.py
python3 scripts/validate_secrets_matrix.py --dry-run
python3 scripts/verify_b246_webauthn_otp.py
python3 scripts/verify_b247_clerk_proptest.py
python3 scripts/verify_dpa_acceptance_fixture.py
python3 scripts/verify_b249_dt_webhook_sunset.py
python3 scripts/verify_b251_quota_cas_budget.py
python3 scripts/backlog_verify.py --id B-NNN
git diff --check
```

No Cargo/build/target/global-CI action is part of this packet. DCO is checked
on the authored commit before handoff; the parent bundle remains responsible
for its bundled CI and merge decision.
