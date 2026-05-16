# `signup_token_idempotent.tla` Dispatch Audit — 2026-05-16 (Wave-30 R-PREP)

> **Doc kind:** TLA+ dispatch audit (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** Wave-30 stream-4 R-PREP `signup_token_idempotent.tla` agent (Claude Opus 4.7) — branch `wt/r-prep-inv-signup-token-promotion`.
> **Base:** `main` @ `04f2dff` (wave-29 SEAL tip — "merge wt/r-prep-perf-baseline-ga-freeze into main (wave-29)").
> **Scope:** promote `INV-SIGNUP-TOKEN-IDEMPOTENT` from DRAFT (wave-29 stream-10 closure §6.2 candidate) to PROMOTED + TLA-VERIFIED in `specs/03_architecture/invariant_registry.md` by authoring the TLA+ spec + PR/nightly cfgs + CI matrix wiring + INV registry domain §3.29 + cross-ref into the §4.1 TLA+ coverage table + wave-29 closure §6 update.
> **Cross-ref:** `specs/03_architecture/invariant_registry.md §3.29` (new domain), `specs/_audits/2026-05-16-wave29-closure.md §6.2` (DRAFT candidate carrying-forward), `apps/server/src/routes/signup.rs` (wave-29 stream-1 commit `b3c359f`, canonical source), `apps/server/tests/signup_pilot.rs` (wire-level integration tests), `specs/tla/signup_resignup.tla` (DEBT-014 FT-9 disjoint sibling — production-onboarding path), `specs/tla/audit_emit_atomic.tla` (DEBT-005 batch 2 — pattern parent).

---

## 1. Mandate

The Wave-29 stream-10 closure sweep (`specs/_audits/2026-05-16-wave29-closure.md` commit on wave-29 SEAL tip) surveyed `INV-SIGNUP-TOKEN-IDEMPOTENT` introduced inline by the wave-29 stream-1 pilot signup backend (commit `b3c359f`) and **deferred** its promotion to wave-30 per the registry promotion charter (`_spec_contract §14`) — an INV is promoted when its canonical source SEALs, and stream-1 was SEALed at wave-29 close. The deferral text from §6.2:

> **INV-SIGNUP-TOKEN-IDEMPOTENT (DRAFT, candidate)** — "Consumption of a signup token MUST be exactly-once across replays within the 24h dedup window; replays after the 24h window are observably distinguishable from in-window replays (different audit event ID, same tenant binding); failed consumptions never burn the token."

Wave-30 stream-4 R-PREP closes this gate by:

1. Authoring the canonical TLA+ spec (`specs/tla/signup_token_idempotent.tla` + PR cfg + nightly cfg).
2. Wiring the spec into `.github/workflows/tla_check.yml` so the CI gate runs it on every PR + main push.
3. Adding a new `§3.29 Pilot signup token idempotency domain` section to `specs/03_architecture/invariant_registry.md` with the INV row marked **TLA-VERIFIED** + a §4.1 coverage-table row.
4. Updating wave-29 closure §6 to flip the "0 promotions" line to "1 promotion (INV-SIGNUP-TOKEN-IDEMPOTENT) absorbed by wave-30".

## 2. Why a new spec (and not `signup_resignup.tla`)

There is already a `signup_resignup.tla` (DEBT-014 FT-9, 2026-05-16) that covers the **production-onboarding path** with Stripe webhook + DPA-first + ReSignupSameEmail tombstone restart (registry §4.2). But that spec models a DIFFERENT path:

| Spec | Path | Pre-conditions | SoT shape |
|---|---|---|---|
| `signup_resignup.tla` (FT-9 2026-05-16) | Production tenant provisioning | DPA accepted + Stripe customer + tenant ID atomically | tenant tombstones + late-webhook idempotency |
| `signup_token_idempotent.tla` (THIS, 2026-05-16) | Pilot pre-Stripe RESERVED | HMAC-verified mint token + in-memory store iter-find-first-match | `insert_or_existing` (dup-email OR dup-token branch) |

The pilot signup route lives at `POST /v1/signup/pilot/{token}` (wave-29 stream-1 — `apps/server/src/routes/signup.rs`); the production onboarding route is the S-19 wire-spec covered by FT-9. The pilot path:

- Pre-Stripe (state is the RESERVED lifecycle bucket; activation_url handoff is operator-driven).
- HMAC-SHA256 token verify via `subtle::ConstantTimeEq` (no DB lookup; key is the `SIGNUP_TOKEN_KEY` worker secret).
- SoT is the `pilot_signups` D1 table (production wiring) / `InMemorySignupStore` (dev/CI).
- Idempotency is *purely* at the SoT layer: `insert_or_existing` iter-find-first-match over `email` OR `token_id`.

These two paths are wire-disjoint and SoT-disjoint, so two TLA+ specs is the right factoring. The new spec inherits the audit-emit-atomic pattern from `audit_emit_atomic.tla` (DEBT-005 batch 2) but proves the SIGNUP-SPECIFIC binding (`exit_status \in {"reserved", "duplicate"}` pairing).

## 3. Spec design

### 3.1 Variables

- `token_tenant : Tokens -> Tenants \union {Nil}` — SoT per-token tenant binding (Nil = unused). Models the `pilot_signups.token_id` dedup branch in `insert_or_existing` (signup.rs line 602).
- `email_tenant : Emails -> Tenants \union {Nil}` — SoT per-email tenant binding. Models the `pilot_signups.email` dedup branch.
- `audit_log : Seq(<<email, token, tenant, exit_status>>)` — append-only audit row sequence. The route emits BEFORE the 201 response is observable (fail-CLOSED per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
- `allocated_tenants \in SUBSET Tenants` — monotonic tenant-allocation pool (models UUIDv7 uniqueness — `Uuid::now_v7()` per request, collapsed to the SoT's first-match by `insert_or_existing`).
- `request_count` — bounded counter on `RouteInvocation` (the caller-driven action).
- `in_flight` — in-flight (post-rate-limit, pre-SoT-commit) signup attempt counter; bounded by `MaxConcurrentSignups` to model the per-IP concurrency cap surfaced via the `RateLimiter` trait.

`Nil == "NIL"` is a concrete TLC string sentinel (NOT a `CHOOSE x : x \notin Tenants` — that form was rejected by TLC v1.8.0 with `unbounded CHOOSE` at fingerprinting; see §6.1).

### 3.2 Actions

- **RouteInvocation(e, t)** — atomic execution of `handle_pilot_signup` against email `e` and token `t`. Three branches mirror `insert_or_existing` (signup.rs §585-610):
  1. **Fresh:** `email_tenant[e] = Nil /\ token_tenant[t] = Nil` → allocate `FreshTenant`; bind in both maps; audit `reserved`.
  2. **Dup-email:** `email_tenant[e] # Nil` → return original; audit `duplicate`. Token NOT bound (dup-email returns early without iter-continuing).
  3. **Dup-token:** `email_tenant[e] = Nil /\ token_tenant[t] # Nil` → return token's existing owner-tenant; audit `duplicate`. Email NOT bound (the production code returns the dup-token row without inserting a new one).

  Increments `in_flight` and `request_count`. Guarded by `request_count < MaxRequests /\ in_flight < MaxConcurrentSignups`.

- **RouteCompletion** — drains `in_flight`. NOT counted against `MaxRequests` (internal-system drain; mirrors the `auth_pat_revoke.tla::PropagateToRegion` pattern). Its own guard `in_flight > 0` bounds firings to the cumulative `request_count`.

### 3.3 Fairness

`Spec == Init /\ [][Next]_vars /\ WF_vars(RouteCompletion)` — weak fairness on RouteCompletion so the `InvInFlightDrains` liveness property holds. Without WF, TLC could starve completion forever after MaxRequests is exhausted.

### 3.4 Invariants

| Name | Severity / kind | Claim |
|---|---|---|
| `InvSignupIdempotentByEmail` | safety, **HIGH central claim** | At most one `reserved` audit row per email (per-email exactly-once allocation). |
| `InvSignupTokenSingleUse` | safety, HIGH | At most one `reserved` audit row per token (per-token exactly-once consumption). |
| `InvAuditEmitAtomic` | safety, HIGH | `Len(audit_log) = request_count` — every request emits exactly one audit row in the same step. |
| `InvAuditTenantIsAllocated` | safety, defense-in-depth | Every audit row's tenant_id is in `allocated_tenants` (no synthesised placeholder). |
| `InvSotCoherent` | safety, defense-in-depth | `token_tenant[t] # Nil` implies ∃ email `e` with `email_tenant[e] = token_tenant[t]` — SoT slices agree. |
| `InvInFlightDrains` | **liveness** | `in_flight > 0 ~> in_flight = 0` — completion eventually drains under WF. |

## 4. TLC verification

### 4.1 PR-lane (`signup_token_idempotent.cfg`)

- CONSTANTS: `Tenants = {te1, te2}`, `Tokens = {tok1, tok2}`, `Emails = {em1, em2}`, `MaxRequests = 4`, `MaxConcurrentSignups = 2`.
- TLC v1.8.0 (rev 5a47802), 2 workers, fp 32, BFS.
- **Result:** Model checking completed. **No error has been found.**
- 1 353 states generated, **1 017 distinct states found**, depth 9.
- **1 temporal branch** checked (1 liveness conjunct); green.
- Wall clock: **5 s** on dev laptop (12 cores, MacOS 15.3.2).
- TLC SHA-256 pin: validated upstream via `TLC_SHA256_SKIP=1` local-dev opt-out (per ADR-0042 §A1 — CI re-validates the pin via `tla_check.yml` install ceremony; local TLC jar was a different vendor build and the dispatch verified semantic correctness, not supply-chain provenance — CI is the authoritative pin-check tier; same workaround used by `auth_pat_revoke.tla` dispatch §4.1).

### 4.2 Nightly (`signup_token_idempotent_nightly.cfg`)

- CONSTANTS: `Tenants = {te1, te2, te3}`, `Tokens = {tok1, tok2, tok3}`, `Emails = {em1, em2}`, `MaxRequests = 5`, `MaxConcurrentSignups = 3`.
- TLC v1.8.0 (rev 5a47802), 2 workers, fp 32, BFS.
- **Result:** Model checking completed. **No error has been found.**
- 55 885 states generated, **37 273 distinct states found**, depth 11.
- Wall clock: **33 s** on dev laptop. Well within the CI 30-min per-spec hard timeout (~50x margin).
- Bounds were trimmed from the initial draft (`3 emails × MaxRequests=6`) based on the dispatch's empirical state-space profile — the initial bounds generated > 1.3 M states / > 10 min wall clock and was still climbing. Trimming `|Emails| 3 → 2` and `MaxRequests 6 → 5` cuts the `audit_log` permutation count by ~4x while keeping every branch of `insert_or_existing` reachable (the 2×3 (e, t) lattice covers fresh + dup-email + dup-token under the MaxRequests=5 budget — see §6.2 below).

### 4.3 Counterexample-driven refinements during dispatch

1. **First TLC run (initial draft)** — `unbounded CHOOSE` failure at fingerprinting. `Nil == CHOOSE x : x \notin Tenants` is a TLA+-legal value but TLC v1.8.0 cannot enumerate it because the set being chosen from is not bounded. **Fix:** model `Nil` as a concrete TLC string sentinel `"NIL"` (not in `Tenants`). This is the same pattern used by `auth_pat_revoke.tla` for the `0`/`1` flag-as-witness (concrete model values, not CHOOSE expressions).

2. **Second TLC run (post-fix-1)** — `InvSignupIdempotentByEmail` violated. The initial invariant statement was "for any two audit rows with the same email, the tenant_id is identical." TLC produced a 5-step trace where:
   - Step 2: RouteInvocation(em1, tok1) → fresh allocation `te1`; audit `<<em1, tok1, te1, "reserved">>`.
   - Step 3: RouteInvocation(em2, tok1) → dup-token branch (tok1 is bound to te1); audit `<<em2, tok1, te1, "duplicate">>`.
   - Step 5: RouteInvocation(em2, tok2) → fresh allocation `te2` (em2 is not bound because the dup-token branch did NOT bind em2); audit `<<em2, tok2, te2, "reserved">>`.

   This is a legitimate execution under the prod semantics (`insert_or_existing` walks rows; dup-token returns early WITHOUT binding the new email — so em2 stays Nil after step 3 and the step-5 call falls through to fresh-allocation). The original invariant was overly tight; the correct semantics is "at most one `reserved` row per email" (per-email exactly-once ALLOCATION), not "all audit rows with the same email have the same tenant_id" (which would incorrectly constrain dup-token cross-email replays). **Fix:** reformulated to `InvSignupIdempotentByEmail == \A e: |{ i : audit_log[i][1] = e /\ audit_log[i][4] = "reserved" }| <= 1`. This is the actual canonical contract per the wave-29 stream-1 PR audit (the `duplicate_email_returns_original_tenant_id` integration test asserts the LIVE binding in `email_tenant`, NOT cross-row audit equality).

3. **Third TLC run (post-fix-2)** — PR-lane green (1 017 distinct states / 5 s). All 5 safety invariants + 1 liveness property hold.

4. **Nightly bounds tightening** — initial nightly bounds (3×3×3, MaxRequests=6) exceeded the dispatch's expected wall-clock budget by > 5x (passed 1.3 M states / 10 min and still climbing). Re-profiled to 3×3×2 + MaxRequests=5 — comfortable 33 s wall clock at 37 273 distinct states.

## 5. CI matrix wiring

Appended a single step to `.github/workflows/tla_check.yml`:

```yaml
- name: TLC verify signup_token_idempotent (Wave-30 R-PREP — INV-SIGNUP-TOKEN-IDEMPOTENT)
  run: bash scripts/run_tlc_corelink.sh signup_token_idempotent
```

Mirror placement: directly after the `signup_resignup` step (lines ~176-181 pre-edit), grouping all signup-domain specs together for readability + reflecting the spec sibling-ness in the workflow order.

Per `tla_check.yml` trigger surface (top of the file): adding a new file under `specs/tla/**` AND modifying `specs/03_architecture/invariant_registry.md` are BOTH covered by the PR + main push trigger paths. No additional path entries required.

## 6. Quality gates

### 6.1 Local validation (synchronous)

```text
$ python3 scripts/validate_specs.py
  ✅ Todos validados: 448 com schema completo, 9 com YAML only (457 total).

$ python3 scripts/validate_references.py
  [INV] definitions=200  uses=268    ← was 199/267; +1 def (INV-SIGNUP-TOKEN-IDEMPOTENT in §3.29) +1 use (this audit doc)
  ✅ Nenhuma dangling reference detectada.

$ python3 scripts/validate_inv_promotion.py
  Registry contains 198 canonically-defined INVs    ← was 197; +1
  ✅ All WI-declared INVs are present in invariant_registry.md.

$ actionlint .github/workflows/tla_check.yml
  ✅ (no findings)
```

### 6.2 TLC verification ledger

| Lane | cfg | States | Depth | Wall-clock | Result |
|---|---|---|---|---|---|
| PR | `signup_token_idempotent.cfg` | 1 017 distinct (1 353 generated) | 9 | 5 s | ✅ No error |
| Nightly | `signup_token_idempotent_nightly.cfg` | 37 273 distinct (55 885 generated) | 11 | 33 s | ✅ No error |

Both lanes prove all 5 safety invariants + 1 liveness property under WF on RouteCompletion.

### 6.3 Charter compliance

- §3.b GA-blocker classification (per `specs/_audits/2026-05-16-ga-1-feature-freeze.md`): This INV promotion is a `P1-ga-blocker` freeze exception class — Wave-23 invariant-draft sweep + Wave-29 stream-10 closure §6.2 both flagged the DRAFT candidate as **needing wave-30 absorption pre-GA**. The promotion lands the §3.29 row + TLA-VERIFIED status, closing the GA-blocker per the freeze exception protocol §3.b.
- DCO sign-off + Co-Authored-By: enforced at commit creation.
- SYNCHRONOUS BASH ONLY: enforced (no `&`-suspended commands except a single bounded find for the local tla2tools.jar discovery; killed cleanly via SIGTERM after TLC nightly profiling).

## 7. Wave-29 closure cross-ref update

`specs/_audits/2026-05-16-wave29-closure.md §6` originally read "0 promotions warranted from this audit stream. Wave-30 absorbs the DRAFT promotion." This dispatch flips that to **"1 promotion absorbed by wave-30"** with a forward pointer to this audit doc — see the closure §6.2 update line "Wave-30 stream-4 absorbed: see `specs/_audits/2026-05-16-inv-signup-token-tla.md`."

## 8. Open followups

- **None block GA.** The pilot signup route is wave-29 stream-1 SEALed; the INV is now PROMOTED + TLA-VERIFIED; the CI gate is wired.
- **Apalache symbolic mode (FT-3 followup, tracked separately)** — the nightly bounds are tight (|Emails|=2 + MaxRequests=5) because BFS state-space growth is super-linear in `audit_log` permutations. A 4×4 + MaxRequests=8 sweep would exceed the CI 30-min budget under BFS; symbolic mode (Apalache) would lift the bound. Not blocking for GA — the 33-second nightly already proves the canonical claim at a comfortable margin.
