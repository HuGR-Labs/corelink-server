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

Specs in `specs/tla/` that **do not currently verify**, with the measured reason.

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
**40 of 47 specs pass, and together they take about 6 minutes.** The suite was
never far from working; it was structured so that it could not.

One of those 40 only passes because this change also fixed it: `auth_pat_hybrid`
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

---

**Bottom line for a reader in a hurry:** 40 specs went from *never verified* to
*verified on every run*. Seven are honestly marked unverified. Nothing here
weakens an invariant to buy a green check.
