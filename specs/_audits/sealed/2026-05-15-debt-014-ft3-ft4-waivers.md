---
type: debt_waiver
title: DEBT-014 FT-3 + FT-4 — infrastructure waivers (deferred to owner WGs)
parent_register: 2026-05-15-debt-register.md
parent_tickets: tla-followup-tickets.md
created: 2026-05-15
owner: Orchestrator (waiver issuer); Security WG + Architect (FT-3 closure); S-14 owner (FT-4 closure)
status: ISSUED — waiver active until owner WG executes closure
---

# DEBT-014 FT-3 + FT-4 — infrastructure waivers

The DEBT-014 dispatch closes FT-1, FT-2, FT-5 via net-new TLA+ specs
(`auth_pat_hybrid`, `key_lifecycle`, `gc_lock_protocol`). FT-3 and FT-4
are **infrastructure / governance** tickets that the dispatch CANNOT
unilaterally close because:

- FT-3 (TLC jar SHA-256 pin drift) requires Security WG review +
  Architect sign-off + ADR-0042 §A1 amendment per the pin-rotation
  protocol declared in that ADR. The orchestrator MUST NOT update the
  SHA pin autonomously — doing so would bypass the supply-chain control
  the pin is designed to enforce.
- FT-4 (`.cfg` function-literal parser brittleness) requires the S-14
  owner to refactor `region_residency.tla` so that `PrimaryRegionOf`
  becomes a TLA-level operator instead of a `.cfg` constant. That spec
  is owned by S-14; cross-cutting refactor without their review risks
  breaking the model's CONSTANT contract.

This waiver documents (a) the FAIL-mode analysis for each ticket and
(b) the monitoring compensation that holds while the waiver is active.
Both compensations are already in place — this doc records them as
formal control evidence.

---

## FT-3 — TLC jar SHA-256 pin drift

### FAIL mode

The pin `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f`
in `.github/workflows/tla_check.yml`, `.github/workflows/nightly.yml`,
and `scripts/run_tlc_corelink.sh` does NOT match the current upstream
asset at `https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar`
(actual: `25780ac9578e1ec356ff423f7f1cf70b0d663f2af8b83ae99745701dcdf299b0`).

If the pin and the upstream remain divergent, the install step in every
TLA+ CI workflow fails the SHA check and exits 1 — which fails the PR
gate. This is the **fail-CLOSED** mode: no merge can land while the pin
mismatch persists, so no untrusted TLC binary can enter the CI lane.

The risk is therefore NOT a security risk — it is an **availability**
risk on the PR gate: every TLA+ PR will fail-closed until the pin is
re-validated by Security WG.

### Failure surface

- All TLA+ PR gates: BLOCKED.
- Nightly TLA+ extended matrix: BLOCKED.
- TLA+ runbook checks (FT-8 surface): not yet wired, no impact.

### Monitoring compensation (active during waiver)

- The CI install step already emits the SHA mismatch as a GitHub
  `::error::` annotation citing ADR-0042 §A1 (see `tla_check.yml`
  lines 80–86). This is the canonical alert channel.
- Any TLA+ PR opened during the waiver window will visibly fail on the
  install step. There is no risk of silent acceptance.
- `scripts/run_tlc_corelink.sh` enforces the same pin defensively
  (line 62) so local dev runs also fail-closed.

### Closure path (NOT executed by this dispatch)

Per ADR-0042 §A1 the closure ceremony is:

1. Security WG fetches and re-hashes the upstream tla2tools.jar v1.8.0.
2. Security WG signs off that the new SHA reflects an upstream re-build
   of the same release tag (no version drift), or escalates to
   "pin to a different version" if upstream tampered.
3. ADR-0042 §A1 is amended with the new pin + the date of validation
   + the Security WG approver.
4. The pin constant is bumped in:
   - `.github/workflows/tla_check.yml` (`TLC_SHA256_PINNED`)
   - `.github/workflows/nightly.yml` (`TLC_SHA256_PINNED`)
   - `scripts/run_tlc_corelink.sh` (line 24)
5. Architect approves the amendment PR.

### Owner

Security WG + Architect. Tracked in DEBT-014 register row.

---

## FT-4 — `.cfg` function-literal parser brittleness

### FAIL mode

TLC v1.8.0 (the canonical pinned version) and the 2026.05.12 nightly
both reject the `[k |-> v]` syntax for function-valued constants in
`.cfg` files. `specs/tla/region_residency.cfg` line declaring
`PrimaryRegionOf = [t1 |-> "WNAM", t2 |-> "ENAM", t3 |-> "WEUR"]`
DOES NOT PARSE on either tested TLC build — the local TLC reproduction
in the parent audit (`2026-05-15-tla-coverage-audit.md`) confirmed
this with a `ConfigFileException`.

If `region_residency` is invoked in CI as-is, TLC exits 1 at config
parse time — fail-CLOSED. **No reachable trace is hidden** by this
failure, because TLC literally never enters the model.

### Failure surface

- `region_residency` (PR + nightly): would fail-closed if added to the
  CI matrix as-is. The spec is currently NOT in `tla_check.yml`'s job
  list (only `dsr_erasure_atomicity`, `tenant_isolation`,
  `gc_correctness`, `cas_integrity`, `audit_immutability`,
  `auth_revocation` are wired). So this is a **future** failure
  surface, not an active one.

### Monitoring compensation (active during waiver)

- Until FT-4 is closed, `region_residency` MUST NOT be added to the
  PR-gate or nightly TLA+ CI matrix. This dispatch DOES NOT add it.
- The closure path (option (a) below) gives the spec a non-`.cfg`
  representation that is portable across TLC versions.
- Region residency invariants (`INV-DATA-RESIDENCY`, `NoCrossRegionLeak`)
  retain partial TLA+ coverage via `failover_no_split_brain.tla`
  (DEBT-005 dispatch, write-lease handoff side).

### Closure path (NOT executed by this dispatch)

Per FT-4 ticket the preferred option is **(a)**:

1. S-14 owner refactors `region_residency.tla` so that
   `PrimaryRegionOf` is a TLA-level operator derived from `Tenants`
   and `Regions`, e.g.:
   ```tla
   PrimaryRegionOf == [t \in Tenants |-> CHOOSE r \in Regions: TRUE]
   ```
   or a deterministic injection bootstrapped from the constants.
2. Remove `PrimaryRegionOf` from `.cfg`.
3. Re-run TLC locally to confirm parse + model-check pass.
4. Add `region_residency` to `tla_check.yml` matrix AFTER (1)–(3) pass.

### Owner

S-14 owner (region_residency is theirs).

---

## Net effect on DEBT-014

- FT-1, FT-2, FT-5 — CLOSED via new TLA+ specs in this dispatch.
- FT-3, FT-4 — **WAIVED** with fail-closed monitoring compensation
  (this doc). Tickets remain OPEN but the risk is bounded and
  auditable.
- FT-6, FT-7, FT-8, FT-9 — remain OPEN (separate future dispatches).

DEBT-014 status: **PARTIAL (5/9)** — 3 specs CLOSED, 2 infra
waivers issued.
