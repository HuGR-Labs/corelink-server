---
id: "TLA-QUARANTINE"
type: "framework"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-08-02"
updated: "2026-08-02"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["tla", "formal-verification", "evidence", "ci"]
---

# TLA+ suite quarantine

Specs that **do not currently verify**, with the measured reason. Covers every
directory the suite scans (`specs/tla/` and `specs/03_architecture/tla+/runbooks/`
— see `SPEC_DIRS` in the runner); rows are matched by spec name.

This file is machine-read by `scripts/run_tla_suite.sh`. It is not documentation
*about* the gate — it *is* part of the gate. Two rules keep it from rotting into
permanent debt:

1. A spec listed here that **starts passing** fails the build. Quarantine is a
   statement of fact; a stale entry is a lie about what is verified.
2. A spec **not** listed here that fails also fails the build. Nothing gets
   quietly skipped.

So the only way to make the suite green is to either fix a spec (and delete its
row) or to consciously add a row with a reason. Both are visible in review.

## Why this file exists at all

`tla_check` had **0 successes in 195 runs**, back to the earliest record
(2026-05-07) — it never passed once. Two causes stacked:

- the TLC pin was stale, so the install step died before any spec was checked
  (fixed separately — ADR-0042 §A1 re-pin);
- the job ran all 47 specs **serially in one 30-minute job**, and the first spec
  in the list is one that does not terminate — so it consumed the entire budget
  and the other 46 were skipped, every single run.

Measured 2026-08-02 on a 12-core host with the CI flags (`-workers 2 -fp 32`):
**44 of 51 specs pass, and the whole suite finishes in about 5 minutes** (299 s
wall clock, including the bounded budget the quarantined specs are allowed to
burn). The suite was never far from working; it was structured so that it could
not.

The count is 51, not 47, because the specs live in TWO directories: `specs/tla/`
and `specs/03_architecture/tla+/runbooks/`. The second was missed by the first
version of the runner, whose discovery scanned only `specs/tla/`; those 4 specs
were reachable solely through the separate weekly `tla_runbooks_check` workflow.
They all pass, in about 3 seconds combined.

One of those 44 only passes because this change also fixed it: `auth_pat_hybrid`
kept its submission log as an ordered sequence, so its state space was the number
of ordered logs — sum(16^i, i=0..6) = 17,895,697 states, 4 min 12 s at 8 workers,
over any sane per-spec budget. No invariant read a log position, so the ordering
was paid for and never used; as a set the same bounds check in 25,141 states and
~7 s. Three mutants (admit without Argon2id, DB probe before HMAC verify, admit
without step 3c) are still killed, so the model kept its discriminating power.

## Quarantined — state-space explosion (2)

These do not terminate at their current `.cfg` bounds. Not a capacity problem:
`dsr_erasure_atomicity` reached only **depth 4** after 6 minutes with 5.1 M states
generated, 5.1 M distinct, and 5.1 M still queued — every state is new and the
frontier grows faster than it is drained. More cores explore an exponential
frontier faster; they do not make it finite.

| spec | measured |
|---|---|
| `dsr_erasure_atomicity` | 1.5 M states in 90 s, no convergence; depth 4 after 6 min |
| `auth_jwt_validation` | 34.2 M states in 90 s, no convergence |

> **Correction, 2026-08-02 — do not act on the "shrink the bounds" advice below
> for `dsr_erasure_atomicity`.** It was written from the timing evidence alone and
> is wrong about the cause. Investigating it produced a much more serious finding;
> see the dedicated section further down. Shrinking bounds there would buy a green
> check on a model that proves nothing. The advice still stands for
> `auth_jwt_validation`, which has not been investigated at this depth.

**Fix direction (NOT applied here — it changes what is proven):** shrink the
`.cfg` bounds until the model checks, and record in the spec header exactly which
coverage was traded away. `dsr_erasure_atomicity` carries 12 backends × 2 subjects
× 2 tenants × 2 tickets × 6 regions × 3 locales with `MaxAuditChainLen = 30`; the
audit-chain bound alone is combinatorially dominant. Reducing bounds is a
modelling decision with real consequences for assurance, so it needs its own
change with the trade stated — not a quiet edit here.

## Quarantined — spec defects (5)

These fail for reasons in the spec itself, not in the system it models. The first
four fail **at or near the initial state**, which is only possible if the spec has
never been executed — consistent with the gate having never run.

| spec | defect |
|---|---|
| `billing_chain_integrity` | 3 semantic errors — **does not parse** |
| `gc_lock_protocol` | type error in the initial state: compares string `"free"` with `<<"held", w1>>` |
| `replica_failover` | type error: checks whether `"NONE"` is an element of `Nat` |
| `gc_reachable_set_complete` | `InvGcReachableSetComplete` violated — needs a counterexample review to tell a model bug from a real finding |
| `byok_envelope_aad` | see below — three separate defects, one of them serious |

### `byok_envelope_aad` — the spec does not model the attack it claims to refute

This one deserves its own note because the header claims to prove
`INV-BYOK-CRYPTO-SOVEREIGNTY` (CRITICAL), and it does not.

1. **`PutEnvelope`'s guard is dead code.** It reads
   `envelope_tenant[e] # t \/ envelope_aad[e] # t \/ TRUE` — the trailing
   `\/ TRUE` makes the whole disjunction unconditionally true, so the guard
   constrains nothing. Ownership of an envelope can be re-bound to another tenant
   at will, which BYOK does not permit: the AAD binds the envelope to its tenant,
   and changing owner means re-wrapping into a *new* envelope (the header itself
   says "one per put").
2. **The adversary cannot act.** `AttemptCrossTenantUnwrap` yields `"ok"` only
   when `envelope_aad[e] = t2` for an envelope owned by `t1 # t2` — i.e. a forged
   AAD. But **no action in the spec ever makes `envelope_aad` differ from
   `envelope_tenant`**: `Init` sets them equal and `PutEnvelope` writes both to
   the same value. The comment on `envelope_aad` says it is "separated from
   envelope_tenant so the model can express tampering" — that tampering action was
   never written. The cross-tenant attack is therefore unreachable, and the
   safety property about it is vacuously true.
3. **`InvNoCrossTenantOk` is time-naive.** It asserts that for every *past*
   successful decrypt, the envelope's *current* owner equals the decrypting
   tenant. A legitimate re-binding retroactively falsifies a decrypt that was
   correct when it happened. The observed violation is this defect firing on
   defect 1 — not a production flaw.

**Fix direction (NOT applied here):** restore the ownership guard, add the missing
`TamperAAD` action so the modelled adversary can actually attempt a forgery, and
restate the invariant against the owner *at decrypt time*. That is a
formal-methods change to a CRITICAL invariant's model and must be reviewed as
such — doing it in the same change that merely makes the suite runnable would bury
it.

### `dsr_erasure_atomicity` — `INV-DATA-ERASURE-COMPLETE` cannot fail

Investigated 2026-08-02 while attempting the "shrink the bounds" fix above. The
bounds are not the problem. This spec is quarantined for the same reason as
`byok_envelope_aad`, on the invariant that carries the **GDPR** erasure claim.

`InvErasureComplete` asserts: a ticket in state `completed` has every backend in
`{erased, pseudonymized, not_applicable}`. It is true by construction, twice over:

1. **The only writer cannot write a violating value.** `backend_state` is assigned
   in exactly one place (`EraseBackend`), as
   `IF b \in EffectiveBackends THEN "erased" ELSE "pseudonymized"` — a total
   function of the backend with **no failure branch and no nondeterminism**. Of
   the five values `BackendOutcome` declares, **three (`pending`,
   `not_applicable`, `failed`) are never written by any action.** The model cannot
   express an incomplete erasure, which is the only thing the invariant forbids.
2. **The invariant restates the guard.** `CompleteErasure` is the only action that
   can set `completed`, and its guard is character-for-character the invariant's
   consequent. So the check reduces to *an `if` implies its own condition*.

This also explains the state explosion, which is a **symptom**: `attempt_count`
carries a 0..`MaxAttempts` counter per (ticket, backend) — 24 slots — to model
**retries of an operation that never fails**, and `FailDsr` can only fire after 5
redundant, identical re-erasures of a backend that already succeeded. Together
`backend_state` × `attempt_count` is on the order of 10^37 before any other
variable is considered. Two further findings from the same pass, recorded so the
next attempt does not rediscover them: the consent timestamp KEY ranged over
`1..MaxAuditChainLen`, coupling two unrelated quantities so the audit-chain bound
multiplied the consent key space; and `ProofRecord` was the 216-record cross
product of six fields (~10^561 partial functions on `consent_ledger`) of which
**every one was valid**, so `ValidProof` never rejected anything reachable.

**Fix direction (NOT applied — this is a model redesign, not a bounds tweak):**
give `EraseBackend` a genuine failure branch so `pending`/`failed` become
reachable — which is also what makes `attempt_count` and `FailDsr` do real work —
and restate `InvErasureComplete` against an independent record of what erasure was
*requested*, so it is no longer the guard of the action it constrains. Only then
is it worth tuning bounds. A bounds-only change here would produce a green check
on a model that proves nothing, which is strictly worse than the honest red.

**Consequence to state plainly:** `INV-DATA-ERASURE-COMPLETE` is currently **not**
formally verified, and was not verified at any point in this gate's history. The
dedicated `tla_dsr_erasure_check` workflow (0 successes in 100 runs) does not
change that — it never got past the TLC install step.

---

**Bottom line for a reader in a hurry:** 44 specs went from *never verified* to
*verified on every run*. Seven are honestly marked unverified. Nothing here
weakens an invariant to buy a green check.
