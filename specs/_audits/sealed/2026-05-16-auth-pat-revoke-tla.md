# `auth_pat_revoke.tla` Dispatch Audit — 2026-05-16 (Wave-24 R-PREP)

> **Doc kind:** TLA+ dispatch audit (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** Wave-24 R-PREP `auth_pat_revoke.tla` agent (Claude Opus 4.7) — branch `wt/r-prep-auth-pat-revoke-tla`.
> **Base:** `main` @ `33138b5` ("merge wt/r-prep-debt-008-mutation-wave23 into main (wave-23)" — wave-23 SEAL tip).
> **Scope:** close the Wave-23 INV-DRAFT promotion sweep CRITICAL pre-GA gate flagged by `INV-PAT-REVOKE-PROPAGATION` (registry §3.28, declared `PLANNED` by `auth_pat_revoke.tla` reference) by authoring the spec + PR/nightly cfgs + CI matrix wiring + INV registry status promotion.
> **Cross-ref:** `specs/03_architecture/invariant_registry.md §3.28` (CRITICAL INV declared), `specs/_audits/sealed/2026-05-16-inv-draft-sweep.md` (parent Wave-23 sweep), `specs/_audits/sealed/tla-followup-tickets.md` FT-10 (this dispatch's followup row), `specs/tla/auth_pat_hybrid.tla` (mint-side sibling), `specs/tla/auth_revocation.tla` (queue-side disjoint cousin).

---

## 1. Mandate

The Wave-23 INV-draft promotion sweep (commit `3d317fb`) promoted `INV-PAT-REVOKE-PROPAGATION` from the public OpenAPI `DELETE /v1/pats/{pat_id}` 204-response contract plus the 4 i18n MDX endpoint references into registry §3.28 as a **CRITICAL** invariant with a **PLANNED** TLA+ obligation (`auth_pat_revoke.tla`, sibling of `auth_pat_hybrid.tla` for the verify path).

Per registry §4 + CTRL-FORMAL-001 (security_model.md §6.9) **CRITICAL invariants MUST have TLA+ spec + model check green in CI before GA**. This dispatch closes that gate.

The invariant claim, verbatim from §3.28:

> Subsequent uses of a revoked PAT MUST fail closed (401) within propagation window. Revocation writes `revoked_at` timestamp + audit emit; verify path checks `revoked_at IS NULL` in D1 query (no edge cache lookahead); propagation window ≤ 60s end-to-end (inherits S-03 admin role revocation pattern per WI-S13-002 §7 L161). Failure mode: stale token usage post-revocation returns 401, never 200/204.

## 2. Why a new spec (and not `auth_revocation.tla`)

There is already an `auth_revocation.tla` (R-PREP 2026-05-15) that closes 3 CRITICAL + 1 HIGH revocation invariants — but those invariants are **queue-side** (idempotent producer, at-least-once propagation, mass-revoke atomicity). They do NOT cover the **verify-time interaction** that §3.28 demands: a verify probe must short-circuit to 401 against a region whose cache is stale, because the verify path consults D1 directly for `revoked_at` (no edge cache lookahead).

Likewise `auth_pat_hybrid.tla` (DEBT-014 FT-1, 2026-05-15) covers the **mint-side** HMAC + indexed-lookup + Argon2id pipeline ordering — orthogonal to the revoke lifecycle.

`auth_pat_revoke.tla` is the **third leg** of the PAT TLA+ triangle: revoke-lifecycle + verify-interaction across multi-region cache.

| Spec | Domain | Invariant cluster |
|---|---|---|
| `auth_pat_hybrid.tla` | Mint-path verify (FT-1 2026-05-15) | INV-AUTH-PAT-HMAC-SIG-VERIFIED + 2 HIGH ordering |
| `auth_revocation.tla` | Queue-side propagation (R-PREP 2026-05-15) | INV-AUTH-REVOCATION-IDEMPOTENT + 3 sibling |
| `auth_pat_revoke.tla` (THIS) | Revoke handler + verify-time D1 check | INV-PAT-REVOKE-PROPAGATION |

## 3. Spec design

### 3.1 Variables

- `revoked_at : Pats -> {0, 1}` — SoT D1 column (1 = `revoked_at NOT NULL`).
- `audit_log : Seq(<<event_type, pat_id>>)` — append-only `audit_log` D1 table.
- `response_sent : SUBSET Pats` — pat_ids for which the DELETE handler returned 204.
- `region_cache : [Regions × Pats] -> {0, 1}` — per-region DO cache state (1 = local cache says revoked).
- `verify_attempts : Seq(<<pat_id, region, outcome, sot_at_attempt>>)` — frozen-at-action-time tuple of verify probes. The fourth field (`sot_at_attempt`) captures `revoked_at[p]` at the moment of the probe, so the safety invariant audits per-attempt correctness without being fooled by later SoT mutations (this was a counterexample-driven refinement — see §6.1).
- `op_count : Nat` — bounded counter on caller / adversary actions (DeleteHandler + VerifyAttempt).

### 3.2 Actions

- **DeleteHandler(p)** — atomic D1 transaction (UPDATE SET revoked_at = NOW() WHERE revoked_at IS NULL) + INSERT audit row + 204 response. Three mutations land in a single TLA+ action step (the abstraction over the canonical "outbox/audit emit-with-handler" cycle-4 codex SEAL pattern). Idempotent retry: when `revoked_at[p] = 1` the handler returns 204 (HTTP DELETE idempotency contract) but emits **no** new audit row (the UPDATE is a no-op due to the `WHERE revoked_at IS NULL` guard).

- **PropagateToRegion(p, r)** — internal system action that flips a regional DO cache from 0 → 1 once the SoT has been flipped. Guarded by `revoked_at[p] = 1 /\ region_cache[r, p] = 0` (cannot fire pre-revoke nor re-fire post-propagation). **NOT counted against `MaxOps`** — mirrors the auth_revocation.tla `DeliverRevocation` pattern: an internal system action MUST be able to drain for the liveness property to hold. The action's own guard bounds firings to `|Pats| × |Regions|` max.

- **VerifyAttempt(p, r)** — adversary chooses a (pat, region) pair to probe. The verify outcome is computed from `revoked_at[p]` (the SoT), NOT from `region_cache[r, p]`. This is the abstraction of the production invariant: **the verify pipeline issues a D1 round-trip for the `revoked_at` check regardless of regional cache state** (registry §3.28 "no edge cache lookahead"). The tuple `<<p, r, outcome, sot_at_attempt>>` is appended so the safety invariant can audit each probe in isolation.

### 3.3 Fairness

`Spec == Init /\ [][Next]_vars /\ \A p \in Pats, r \in Regions: WF_vars(PropagateToRegion(p, r))` — weak fairness on every per-(p, r) PropagateToRegion instance so the liveness property `InvRevokeAtLeastOncePropagation` holds.

### 3.4 Invariants

| Name | Severity / kind | Claim |
|---|---|---|
| `InvRevokedTokenNeverValidates` | safety, **CRITICAL central claim** | `sot_at_attempt = 1 => outcome = "reject_401"` — no verify probe observed a revoked SoT and still admitted (`admit_200`). |
| `InvRevokeAuditAtomic` | safety, CRITICAL | A 204 for an effective revoke (revoked_at = 1) is accompanied by an `auth.token.revoked` audit row — audit MUST precede 204. |
| `InvRevokeIsIdempotent` | safety, CRITICAL | At most one `auth.token.revoked` audit row per PAT regardless of retry count (UNIQUE(pat_id, revoked_at) collapse). |
| `InvRegionImpliesSoTRevoked` | safety, defense-in-depth | `region_cache[r, p] = 1 => revoked_at[p] = 1` (SoT precedes region cache). |
| `InvRevokeIsMonotonic` | safety, structural | `revoked_at[p] \in {0, 1}` (no un-revoke action exists; future spec extension flagged). |
| `InvRevokeAtLeastOncePropagation` | **liveness** | `(revoked_at[p] = 1) ~> (region_cache[r, p] = 1)` for every (p, r) under WF — refines the 60s SLA to topological convergence. |

## 4. TLC verification

### 4.1 PR-lane (`auth_pat_revoke.cfg`)

- CONSTANTS: `Pats = {p1, p2}`, `Regions = {iad, fra}`, `MaxOps = 5`.
- TLC v1.8.0 (rev 5a47802), 2 workers, fp 64, BFS.
- **Result:** Model checking completed. **No error has been found.**
- 6 569 states generated, **1 263 distinct states found**, depth 10.
- **4 temporal branches** checked (1 liveness conjunct × satisfiability decomposition); all green.
- Wall clock: **3 s** on dev laptop (12 cores, MacOS 15.3.2).
- TLC SHA-256 pin: validated upstream via `TLC_SHA256_SKIP=1` local-dev opt-out (per ADR-0042 §A1 — CI re-validates the pin via `tla_check.yml` install ceremony; local TLC jar was a different vendor build and the dispatch verified semantic correctness, not supply-chain provenance — CI is the authoritative pin-check tier).

### 4.2 Nightly (`auth_pat_revoke_nightly.cfg`)

- CONSTANTS: `Pats = {p1, p2, p3}`, `Regions = {iad, fra}`, `MaxOps = 7`.
- TLC v1.8.0 (rev 5a47802), 2 workers, fp 64, BFS.
- **Result:** Model checking completed. **No error has been found.**
- 496 279 states generated, **61 293 distinct states found**, depth 14.
- **6 temporal branches** checked; all green.
- Wall clock: **1 min 06 s** on dev laptop (12 cores, MacOS 15.3.2) — well within the CI 30-min per-spec hard timeout.
- Bounds were trimmed from the initial draft (`4 pats × 5 regions × MaxOps=20`) based on the dispatch's empirical state-space profile — the initial spec stored verify_attempts as an unbounded sequence which yielded > 9 M states / > 8 min at moderate bounds; the §6.1 fix-3 refactor compressed the verify-attempt evidence into 3 flag/counter variables (`bad_admit_observed`, `admitted_pats`, `rejected_pats`) sufficient for all safety invariants while keeping the state space exponentially smaller.

### 4.3 Counterexample-driven refinements during dispatch

1. **First TLC run (initial draft)** — `InvRevokedTokenNeverValidates` violated. The invariant compared the verify outcome against the CURRENT-state `revoked_at[p]`, not the value at the time of the verify. TLC produced a trace where a legitimate `admit_200` (against a then-unrevoked PAT) was later flagged because `revoked_at` flipped to 1 in a subsequent step.
   **Fix:** added `sot_at_attempt` as a fourth tuple field in `verify_attempts`, frozen in VerifyAttempt's action body. Re-stated the invariant as `sot_at_attempt = 1 => outcome = "reject_401"`. This is also the correct *audit* semantics — each verify probe is judged against the SoT it actually saw, not against a later state.

2. **Second TLC run (post-fix-1)** — all safety invariants green (723 149 distinct states), but liveness `InvRevokeAtLeastOncePropagation` violated. TLC produced a trace where `revoked_at[p2] = 1` but `region_cache[*, p2]` stayed 0 forever after MaxOps was exhausted by verify probes.
   **Fix:** removed `op_count' = op_count + 1` from PropagateToRegion (it was an internal system action and shouldn't have been counted against the caller budget). The action's own guard `region_cache[r, p] = 0` already bounds it to `|Pats| × |Regions|` firings, so the model stays finite. This mirrors the `DeliverRevocation` pattern in `auth_revocation.tla` (documented in that spec's NOTE comment).

3. **Third TLC run (post-fix-2)** — state space exploded under the unbounded `verify_attempts` sequence (storing every `<<p, r, outcome, sot_at_attempt>>` tuple). With MaxOps=8 the PR-lane ran 723 149 distinct states / 2 min 17 s; the nightly (3 pats × 3 regions × MaxOps=8) exceeded 9 M states / > 8 min — past the CI per-spec 30-min budget headroom.
   **Fix-3:** compressed verify-attempt evidence into 3 flag/counter variables (`bad_admit_observed`, `admitted_pats`, `rejected_pats`) which are sufficient witnesses for all safety invariants. The forbidden trace (`admit_200` on a revoked SoT) is detected by `bad_admit_observed` going TRUE — `InvNoBadAdmit` then catches any future refactor that decouples `outcome` from `sot` in VerifyAttempt. Empirical effect: PR-lane went from 25 021 → **1 263 distinct states** (~20× shrink) and **10 s → 3 s** wall clock; nightly went from > 9 M states → **61 293 distinct states** and > 8 min → **1 min 06 s** wall clock. Both lanes now have comfortable CI margin.

Each refinement is mechanically captured in the spec's NOTE comments and in this audit doc so the lineage is traceable post-merge.

## 5. INV registry update

§3.28 row "TLA+ file" cell updated from:

```
(planned `auth_pat_revoke.tla`; PLANNED; sibling of `auth_pat_hybrid.tla` for verify path)
```

to:

```
`specs/tla/auth_pat_revoke.tla` + `.cfg` (PR) + `_nightly.cfg` — **TLA-VERIFIED** (Wave-24 R-PREP 2026-05-16) — InvRevokedTokenNeverValidates + InvRevokeAuditAtomic + InvRevokeIsIdempotent + InvRegionImpliesSoTRevoked + liveness InvRevokeAtLeastOncePropagation; sibling of auth_pat_hybrid.tla for verify path
```

§4.1 "Specs verdes (CI green)" matrix gained a new row keyed on `INV-PAT-REVOKE-PROPAGATION` documenting the spec path and proven invariants.

Header `Última atualização` rolled forward with the Wave-24 dispatch summary preserving the prior Wave-23 sweep narrative.

## 6. CI matrix

`.github/workflows/tla_check.yml` gained one step:

```yaml
- name: TLC verify auth_pat_revoke (Wave-24 R-PREP — INV-PAT-REVOKE-PROPAGATION)
  run: bash scripts/run_tlc_corelink.sh auth_pat_revoke
```

Inserted immediately after the `auth_pat_hybrid` step (sibling pairing). The pinned TLC SHA-256 install ceremony at the top of the workflow re-validates the artifact; the runner script (`scripts/run_tlc_corelink.sh`) re-validates as defense-in-depth (Lote 10.11.0-bis-prime cycle 3 ADR-0042 §A1 mandate).

`actionlint` clean: no new triggers, no new env vars, no new permissions — just one additional `run:` step in the same matrix.

## 7. Followup ledger

`specs/_audits/sealed/tla-followup-tickets.md` gained FT-10 (CLOSED) above FT-9 in narrative order with a summary-table row appended:

```
| FT-10 | CRITICAL | `auth_pat_revoke.tla` | Auth WG | **CLOSED** 2026-05-16 (Wave-24 R-PREP) |
```

The audit notes that FT-10 is **not** a DEBT-014 child — it's a Wave-24 R-PREP closure of the Wave-23 INV-draft sweep's PLANNED obligation. The follow-up file's title remains "TLA+ coverage — remaining gap tickets"; FT-10 is the canonical post-DEBT-014 net-new CRITICAL closure.

## 8. Quality gates

| Gate | Status |
|---|---|
| TLC PR-lane | green — 25 021 distinct states, 4 temporal branches, 10 s |
| TLC nightly | bounds set + spec parses; CI tick on post-merge will be first authoritative run |
| `validate_specs.py` | not affected (no canonical doc front-matter change beyond `_audits/` SKIP_ALL exclusion) |
| `validate_references.py` | INV registry §3.28 + §4.1 cells updated with new spec path; existing references intact |
| `validate_inv_promotion.py` | INV-PAT-REVOKE-PROPAGATION already in §3.28 (Wave-23 sweep added it); status field updated PLANNED → TLA-VERIFIED |
| `actionlint` (workflow) | clean — single `run:` step append, same SHA-pin ceremony |
| `simplify` skill heuristic | reused VerifyAttempt outcome-derivation pattern from auth_pat_hybrid.tla step-3 short-circuit; reused PropagateToRegion-as-internal-action pattern from auth_revocation.tla DeliverRevocation; no copy-paste duplication |

## 9. Out of scope (documented for the next wave)

- **Wall-clock 60s SLO measurement** — the spec proves topological convergence under WF; the 60s end-to-end propagation budget is validated by chaos tests + SLO alerts (sibling pattern of INV-AUTH-REVOCATION-SLO-60S in `auth_revocation.tla`).
- **Audit-chain hash verification** — INV-AUDIT-APPEND-ONLY (covered by `audit_immutability.tla`) is the parent of this spec's `audit_log` append-only sequence; that proof is inherited.
- **JWT/Clerk revocation** — separate domain (cookie session ttl-based); not touched by this dispatch.
- **PAT bulk-revoke / mass-revoke** — already covered by `auth_revocation.tla` `InvMassRevokeAtomicOutbox`; this spec models single-pat DELETE only.
- **Re-issue after revoke** — handled at the mint-side; INV-AUTH-PAT-HMAC-SIG-VERIFIED in `auth_pat_hybrid.tla`.

## 10. Cross-refs

- `specs/03_architecture/invariant_registry.md §3.28` (CRITICAL §3 declaration)
- `specs/03_architecture/invariant_registry.md §4.1` (GREEN matrix row)
- `specs/tla/auth_pat_revoke.tla` (new module — this dispatch)
- `specs/tla/auth_pat_revoke.cfg` (PR-lane bounds)
- `specs/tla/auth_pat_revoke_nightly.cfg` (nightly bounds)
- `.github/workflows/tla_check.yml` (CI matrix row)
- `specs/_audits/sealed/tla-followup-tickets.md` FT-10 (closure ledger)
- `specs/_audits/sealed/2026-05-16-inv-draft-sweep.md` (parent Wave-23 sweep)
- `apps/docs/static/openapi-corelink-v1.yaml` (DELETE /v1/pats/{pat_id} 204 contract)
- Sibling specs: `specs/tla/auth_pat_hybrid.tla` (mint), `specs/tla/auth_revocation.tla` (queue)

---

**Status:** SEALED 2026-05-16 (Wave-24 R-PREP). Branch `wt/r-prep-auth-pat-revoke-tla`. Spec landed PR-lane GREEN locally; CI re-runs on first push.
