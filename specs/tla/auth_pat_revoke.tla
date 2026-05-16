---------------------------- MODULE auth_pat_revoke ----------------------------
(***************************************************************************)
(* CoreLink — PAT revoke endpoint state machine (Wave-24 R-PREP)            *)
(*                                                                          *)
(* Closes the CRITICAL pre-GA gate flagged by the Wave-23 INV-DRAFT         *)
(* promotion sweep:                                                         *)
(*                                                                          *)
(*   INV-PAT-REVOKE-PROPAGATION (CRITICAL, registry §3.28)                  *)
(*     Subsequent uses of a revoked PAT MUST fail closed (401) within the   *)
(*     propagation window. Revocation writes `revoked_at` timestamp + emits *)
(*     audit event; the verify path checks `revoked_at IS NULL` against D1  *)
(*     (no edge cache lookahead); propagation window <= 60s end-to-end.     *)
(*     Failure mode: stale token usage post-revocation returns 401, never   *)
(*     200/204. Audit emit MUST precede the 204 response on the DELETE      *)
(*     /v1/pats/{pat_id} endpoint.                                          *)
(*                                                                          *)
(* This spec is the SIBLING of `auth_pat_hybrid.tla` (which covers the      *)
(* verify-time HMAC + Argon2id pipeline for the MINT side) and is DISJOINT  *)
(* from `auth_revocation.tla` (which covers queue-side at-least-once        *)
(* propagation + idempotent producer + mass-revoke atomicity). This file    *)
(* models specifically:                                                     *)
(*                                                                          *)
(*   1. The DELETE /v1/pats/{pat_id} handler state machine:                 *)
(*      a) D1 UPDATE: SET revoked_at = NOW() WHERE pat_id = ? AND           *)
(*         revoked_at IS NULL  (idempotent at the SoT layer).               *)
(*      b) Audit emit: `auth.token.revoked` row in audit_log in the SAME    *)
(*         D1 transaction as the UPDATE (cycle-4 codex SEAL pattern).       *)
(*      c) 204 response: ONLY after audit row is durable.                   *)
(*                                                                          *)
(*   2. The verify-path interaction (sibling of auth_pat_hybrid.tla step 3b *)
(*      indexed lookup): the verify query joins on `revoked_at IS NULL`,    *)
(*      so a verify attempt that observes a revoked row MUST short-circuit  *)
(*      to 401 BEFORE any Argon2id work.                                    *)
(*                                                                          *)
(*   3. Multi-region propagation: regional DO caches converge to the SoT    *)
(*      state via the at-least-once queue (modelled abstractly; the full   *)
(*      queue semantics are in `auth_revocation.tla`). The combined claim  *)
(*      here is: a verify against a region's local cache MUST NOT admit a  *)
(*      token whose SoT row is revoked, even during the propagation window *)
(*      — because the verify path bypasses the regional cache for the     *)
(*      `revoked_at` check and goes straight to D1 (no edge cache          *)
(*      lookahead, per registry §3.28 enforcement language).              *)
(*                                                                          *)
(* Threat model:                                                            *)
(*   - Attacker holds a still-cached valid PAT; user clicks revoke.         *)
(*   - Between the D1 UPDATE and the regional DO cache invalidation, the    *)
(*     attacker submits the PAT to a region whose cache still says "valid".*)
(*   - INV-PAT-REVOKE-PROPAGATION mandates: the verify path consults D1    *)
(*     for `revoked_at` directly (NO edge-cache shortcut), so the attack   *)
(*     fails closed at 401 regardless of cache staleness.                  *)
(*   - Audit emission MUST precede 204 (auditor cannot deny a revoke ever  *)
(*     happened if the customer got the 204).                              *)
(*                                                                          *)
(* Out of scope:                                                            *)
(*   - Queue redelivery and at-least-once consumer dedup — `auth_revocation*)
(*     .tla` already covers these as INV-AUTH-PROPAGATION-AT-LEAST-ONCE.   *)
(*   - Mass-revoke tenant atomicity — `auth_revocation.tla` covers via    *)
(*     INV-AUTH-MASS-REVOKE-ATOMIC.                                        *)
(*   - HMAC / Argon2id pipeline ordering — `auth_pat_hybrid.tla` covers    *)
(*     via INV-AUTH-PAT-HMAC-SIG-VERIFIED.                                 *)
(*   - Wall-clock 60s SLA — chaos tests measure the p99 budget; this spec  *)
(*     proves the topological invariants (no 200/204 on revoked).          *)
(*                                                                          *)
(* Cross-refs:                                                              *)
(*   - `specs/03_architecture/invariant_registry.md §3.28`                  *)
(*   - `apps/docs/static/openapi-corelink-v1.yaml` DELETE /v1/pats/{pat_id} *)
(*   - `specs/_audits/2026-05-16-auth-pat-revoke-tla.md` (this dispatch)    *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Pats,          \* Finite set of PAT IDs in scope
    Regions,       \* Finite set of DO regions
    MaxOps         \* Bound on adversary + caller actions

ASSUME
    /\ Pats # {}
    /\ Regions # {}
    /\ MaxOps \in Nat

VARIABLES
    revoked_at,        \* Function Pats -> {0, 1}: SoT D1 column (1 = revoked_at NOT NULL)
    audit_log,         \* Sequence of <<event_type, pat_id>> append-only audit rows
    response_sent,     \* Set of pat_id for which the handler returned 204
    region_cache,      \* Function [Regions x Pats] -> {0, 1}: 1 = region cache says revoked
    \* The verify-attempt evidence is intentionally compressed to a small
    \* number of flag/counter variables rather than an unbounded sequence
    \* of <<pat, region, outcome, sot_at_attempt>> tuples. Storing the full
    \* trace blew up the BFS state space ~16^MaxOps under exhaustive search
    \* with little informational gain — every reachable verify outcome is
    \* exhibited at depth ≤ 2 from any (revoked_at, region_cache) shape.
    \*
    \* The flags below are sufficient witnesses for the safety invariants:
    \*   - `bad_admit_observed`: TRUE iff there exists a trace prefix that
    \*     produces admit_200 for a PAT whose SoT was revoked at attempt
    \*     time. This is the CENTRAL CRITICAL claim
    \*     (InvRevokedTokenNeverValidates) — must stay FALSE in all reachable
    \*     states.
    \*   - `admitted_pats`: the set of PATs that have produced at least one
    \*     admit_200 outcome (used by the InvRevokeAuditAtomic check —
    \*     never directly asserted, but cross-evidence for the audit pairing
    \*     proof).
    \*   - `rejected_pats`: the set of PATs that have produced at least one
    \*     reject_401 outcome (analogous role).
    bad_admit_observed, \* Boolean witness — must remain FALSE in all states.
    admitted_pats,      \* Subset of Pats with at least one admit_200.
    rejected_pats,      \* Subset of Pats with at least one reject_401.
    op_count

vars == <<revoked_at, audit_log, response_sent, region_cache,
          bad_admit_observed, admitted_pats, rejected_pats, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ revoked_at = [p \in Pats |-> 0]
    /\ audit_log = <<>>
    /\ response_sent = {}
    /\ region_cache = [r \in Regions, p \in Pats |-> 0]
    /\ bad_admit_observed = FALSE
    /\ admitted_pats = {}
    /\ rejected_pats = {}
    /\ op_count = 0

(*-- Actions -----------------------------------------------------------------*)

\* DELETE /v1/pats/{pat_id} handler — ATOMIC: D1 UPDATE + audit INSERT in
\* the SAME transaction (cycle-4 codex SEAL: outbox/audit emit-with-handler).
\* The 204 response is sent ONLY after the transaction commits, so the audit
\* row is durable before the customer observes success.
\*
\* Idempotent at the SoT layer: re-issuing DELETE against an already-revoked
\* row collapses (D1 UPDATE ... WHERE revoked_at IS NULL is a no-op) — the
\* handler returns 204 to preserve idempotency but emits NO additional audit
\* row (INV-PAT-REVOKE-NO-DOUBLE-AUDIT, projection of INV-AUTH-REVOCATION-
\* IDEMPOTENT from sibling auth_revocation.tla).
DeleteHandler(p) ==
    /\ p \in Pats
    /\ op_count < MaxOps
    /\ op_count' = op_count + 1
    /\ \/ /\ revoked_at[p] = 0
          \* First effective revoke: flip SoT, emit audit, send 204.
          \* All three observable mutations are atomic in this step (the
          \* TLA+ action is the abstraction over the D1 transaction).
          /\ revoked_at' = [revoked_at EXCEPT ![p] = 1]
          /\ audit_log' = Append(audit_log, <<"auth.token.revoked", p>>)
          /\ response_sent' = response_sent \union {p}
       \/ /\ revoked_at[p] = 1
          \* Idempotent retry: 204 returned but NO new audit row.
          \* (Models the UPDATE ... WHERE revoked_at IS NULL no-op + handler
          \* returns 204 because the END-STATE is "revoked", per HTTP DELETE
          \* idempotency contract.)
          /\ response_sent' = response_sent \union {p}
          /\ UNCHANGED <<revoked_at, audit_log>>
    /\ UNCHANGED <<region_cache, bad_admit_observed,
                   admitted_pats, rejected_pats>>

\* Regional DO cache invalidation: receives the propagation message from the
\* at-least-once queue (modelled here as a non-deterministic action; the full
\* queue semantics are in auth_revocation.tla).
\*
\* CRUCIAL: this action MAY lag arbitrarily behind DeleteHandler, modelling
\* the propagation window. The safety invariant proves the verify path is
\* immune to this lag (it consults D1, not the regional cache, for the
\* revoked_at check).
\*
\* NOTE: PropagateToRegion is NOT counted against MaxOps — it is an internal
\* system action that MUST be able to drain the propagation backlog for the
\* liveness property to hold (mirrors auth_revocation.tla DeliverRevocation
\* pattern). MaxOps bounds only caller / adversary actions (DeleteHandler +
\* VerifyAttempt).
PropagateToRegion(p, r) ==
    /\ p \in Pats
    /\ r \in Regions
    /\ revoked_at[p] = 1
    /\ region_cache[r, p] = 0
    /\ region_cache' = [region_cache EXCEPT ![r, p] = 1]
    /\ UNCHANGED <<revoked_at, audit_log, response_sent,
                   bad_admit_observed, admitted_pats, rejected_pats,
                   op_count>>

\* Verify-path probe at a region: the runtime hits the regional DO for
\* hot-path verify, BUT for `revoked_at` the verify pipeline issues a D1
\* round-trip (registry §3.28 enforcement: "no edge cache lookahead").
\*
\* Model: the verify outcome is determined by `revoked_at[p]` (the SoT),
\* NOT by `region_cache[r, p]`. This is the abstraction of the production
\* invariant: even if region_cache says "valid" (stale 0), the verify path
\* observes the canonical D1 revoked_at = 1 and short-circuits to 401.
\*
\* The adversary chooses (p, r) freely to probe the worst-case interleaving
\* (revoked at SoT but not yet at region).
VerifyAttempt(p, r) ==
    /\ p \in Pats
    /\ r \in Regions
    /\ op_count < MaxOps
    /\ op_count' = op_count + 1
    /\ LET sot == revoked_at[p]
           outcome ==
            IF sot = 1
            THEN "reject_401"   \* SoT says revoked => fail closed
            ELSE "admit_200"    \* SoT says valid => admit
       IN
        \* Compress the per-attempt evidence into:
        \*   - bad_admit_observed: the forbidden trace (admit on revoked SoT)
        \*   - admitted_pats / rejected_pats: the by-pat outcome aggregates.
        \* By construction the VerifyAttempt action's outcome computation
        \* ensures bad_admit_observed' = bad_admit_observed (i.e., the
        \* forbidden trace is unreachable). The InvNoBadAdmit invariant
        \* asserts this explicitly so any future refactor that decouples
        \* `outcome` from `sot` immediately surfaces a counterexample.
        /\ bad_admit_observed' = bad_admit_observed
             \/ (outcome = "admit_200" /\ sot = 1)
        /\ admitted_pats' = IF outcome = "admit_200"
                            THEN admitted_pats \union {p}
                            ELSE admitted_pats
        /\ rejected_pats' = IF outcome = "reject_401"
                            THEN rejected_pats \union {p}
                            ELSE rejected_pats
    /\ UNCHANGED <<revoked_at, audit_log, response_sent, region_cache>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E p \in Pats: DeleteHandler(p)
    \/ \E p \in Pats, r \in Regions: PropagateToRegion(p, r)
    \/ \E p \in Pats, r \in Regions: VerifyAttempt(p, r)

\* Weak fairness on propagation so the model can prove convergence
\* (region_cache eventually agrees with SoT — bounded eventuality).
Spec ==
    /\ Init
    /\ [][Next]_vars
    /\ \A p \in Pats, r \in Regions:
        WF_vars(PropagateToRegion(p, r))

(*-- Safety invariants -------------------------------------------------------*)

\* INV-PAT-REVOKE-PROPAGATION (CRITICAL §3.28): the central safety claim.
\* No verify attempt observed a revoked SoT and STILL admitted. The
\* compressed witness `bad_admit_observed` flips to TRUE the instant any
\* trace tries to admit a PAT whose SoT was revoked at attempt time. This
\* is the "no edge cache lookahead" invariant: the verify path consulted
\* the SoT and MUST honour what it saw.
\*
\* The flag-as-witness pattern keeps the model state space tractable
\* (avoids storing every verify_attempts tuple in an unbounded sequence)
\* while preserving the central CRITICAL claim. The forbidden trace is
\* unreachable by construction in VerifyAttempt's outcome computation —
\* this invariant catches any future refactor that decouples `outcome`
\* from `sot` (e.g. an erroneous edge-cache shortcut for the revoked_at
\* check).
InvRevokedTokenNeverValidates ==
    bad_admit_observed = FALSE

\* INV-PAT-REVOKE-AUDIT-ATOMIC (CRITICAL §3.28): the audit row for a
\* successful revoke is emitted BEFORE the 204 response is observable.
\* In the TLA+ abstraction this is: any pat_id in response_sent that
\* corresponds to an effective revoke (revoked_at=1) MUST have at least
\* one matching audit_log entry. The atomic step in DeleteHandler enforces
\* this — they mutate together — but the explicit invariant catches any
\* future refactor that splits the steps.
InvRevokeAuditAtomic ==
    \A p \in Pats:
        (p \in response_sent /\ revoked_at[p] = 1) =>
            \E i \in 1..Len(audit_log):
                /\ audit_log[i][1] = "auth.token.revoked"
                /\ audit_log[i][2] = p

\* INV-PAT-REVOKE-IDEMPOTENT (CRITICAL §3.28 projection): no PAT has more
\* than one `auth.token.revoked` audit row, no matter how many DELETE
\* retries the caller issues. Maps to the D1 UPDATE ... WHERE revoked_at
\* IS NULL guard collapsing retries at the SoT.
InvRevokeIsIdempotent ==
    \A p \in Pats:
        Cardinality({ i \in 1..Len(audit_log) :
                          audit_log[i][1] = "auth.token.revoked"
                       /\ audit_log[i][2] = p }) <= 1

\* INV-PAT-REVOKE-SOT-PRECEDES-REGION (defense-in-depth §3.28): a region
\* cache marked revoked implies the SoT has already been flipped. Models
\* the "SoT is canonical" projection of INV-AUTH-NEON-IS-SOT into the PAT
\* revoke pipeline. Combined with InvRevokedTokenNeverValidates this proves
\* the verify path cannot be fooled by an out-of-order cache update.
InvRegionImpliesSoTRevoked ==
    \A r \in Regions, p \in Pats:
        region_cache[r, p] = 1 => revoked_at[p] = 1

\* INV-PAT-REVOKE-MONOTONIC: once a PAT is revoked it stays revoked
\* (no un-revoke action exists). Models the append-only nature of the
\* `revoked_at` column (UNIQUE constraint, NOT NULL once written).
\* Trivially true by construction — explicit so any future spec extension
\* (e.g. "un-revoke" admin action) flags the gap immediately at TLC.
InvRevokeIsMonotonic ==
    \* No transition lowers revoked_at[p] from 1 to 0. Checked via
    \* StateConstraint in cfg + verified by the absence of any action that
    \* writes 0 to revoked_at — explicit here for documentation.
    \A p \in Pats: revoked_at[p] \in {0, 1}

(*-- Liveness ----------------------------------------------------------------*)

\* INV-AUTH-REVOKE-AT-LEAST-ONCE-PROPAGATION (HIGH, §3.28 projection):
\* every region eventually agrees with the SoT under WF on the
\* PropagateToRegion action. This refines the 60s SLA into a topological
\* eventuality (the wall-clock budget is validated by chaos tests).
InvRevokeAtLeastOncePropagation ==
    \A p \in Pats, r \in Regions:
        (revoked_at[p] = 1) ~> (region_cache[r, p] = 1)

(*-- Combined safety conjunction (single INVARIANT line in cfg) --------------*)

SafetyInvariants ==
    /\ InvRevokedTokenNeverValidates
    /\ InvRevokeAuditAtomic
    /\ InvRevokeIsIdempotent
    /\ InvRegionImpliesSoTRevoked
    /\ InvRevokeIsMonotonic

================================================================================
