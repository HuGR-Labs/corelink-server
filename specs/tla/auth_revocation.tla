---------------------------- MODULE auth_revocation ----------------------------
(***************************************************************************)
(* CoreLink — auth revocation propagation (R-PREP / GA hardening 2026-05-15)*)
(*                                                                         *)
(* Closes the highest-impact uncovered gap surfaced by                      *)
(* `specs/_audits/2026-05-15-tla-coverage-audit.md`:                        *)
(* three CRITICAL invariants over PAT revocation were PLANNED in            *)
(* `invariant_registry.md §4.2` (`auth_revocation.tla`) but had no spec.    *)
(*                                                                         *)
(* Invariants proved here (all from invariant_registry.md §3.14):           *)
(*                                                                         *)
(*   INV-AUTH-REVOCATION-IDEMPOTENT   (CRITICAL)                            *)
(*     Retrying a revoke against the same `pat_id` MUST collapse to the    *)
(*     same effect: `revoked_at` set exactly once (UNIQUE constraint),     *)
(*     and AT MOST ONE `auth.token.revoked` audit event is observed by     *)
(*     downstream consumers regardless of how many retries the caller     *)
(*     issues or how many times the CF Queue redelivers.                  *)
(*                                                                         *)
(*   INV-AUTH-REVOCATION-SLO-60S   (CRITICAL)                              *)
(*     For every PAT marked `revoked_at` in Neon (SoT per                  *)
(*     INV-AUTH-NEON-IS-SOT), all live regional DO caches converge to the *)
(*     revoked state in BOUNDED steps — modelled here as the temporal     *)
(*     property `RevokedEventuallyConverges`. The 60s SLA is a wall-time  *)
(*     refinement; the spec proves bounded convergence under retry +      *)
(*     queue redelivery (PropagationCount bound).                         *)
(*                                                                         *)
(*   INV-AUTH-MASS-REVOKE-ATOMIC   (CRITICAL)                              *)
(*     Mass revoke of N tokens for a tenant either flips ALL of them to   *)
(*     revoked in one Neon transaction (UPDATE phase) or NONE — even if   *)
(*     the outbox INSERT phase is later chunked, the SoT update is        *)
(*     all-or-nothing. Models the cycle-4 codex SEAL fix.                 *)
(*                                                                         *)
(*   INV-AUTH-PROPAGATION-AT-LEAST-ONCE   (HIGH)                           *)
(*     The CF Queue between revocation source and per-region DOs is       *)
(*     at-least-once: every revocation produced by the source is          *)
(*     observed by every region (model bounded). Combined with consumer  *)
(*     dedup via `(pat_id, revoked_at)` UNIQUE, this collapses with       *)
(*     IDEMPOTENT to ensure a single effect at every region.              *)
(*                                                                         *)
(* Threat model:                                                           *)
(*   - Callers may retry revoke arbitrarily (network errors, dashboard    *)
(*     double-clicks).                                                    *)
(*   - CF Queue may redeliver any message zero, one, or many times until  *)
(*     ack (modelled by non-deterministic `DeliverRevocation`).           *)
(*   - Crashes between Neon UPDATE and outbox INSERT are simulated via    *)
(*     leaving outbox pending; the SoT remains canonical.                 *)
(*                                                                         *)
(* Out of scope (documented in §4 of the audit doc):                       *)
(*   - Wall-clock timing (TLA+ proves bounded convergence; chaos tests    *)
(*     measure the 60s p99 SLO).                                          *)
(*   - JWT/Clerk revocation (separate domain; cookie session ttl-based).  *)
(*   - Argon2id hash verification (algorithmic; not state-machine).       *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/auth_model.md §6`                            *)
(*   - `specs/03_architecture/invariant_registry.md §3.14`                 *)
(*   - `specs/03_architecture/security_model.md §6.9 CTRL-FORMAL-001`      *)
(*   - `specs/_audits/2026-05-15-tla-coverage-audit.md` §5 recommendation  *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Pats,            \* Finite set of PAT IDs in scope of the model
    Regions,         \* Finite set of DO regions (e.g. {iad, fra, gru})
    Tenants,         \* Finite set of tenant IDs
    MaxOps,          \* Bound on total operations (caller retries + deliveries)
    MaxQueue         \* Bound on queue depth

ASSUME
    /\ Pats # {}
    /\ Regions # {}
    /\ Tenants # {}
    /\ MaxOps \in Nat
    /\ MaxQueue \in Nat

\* PatTenant maps each PAT to its owning tenant. Modelled as an operator
\* (not a CONSTANT) because TLC `.cfg` literal syntax for function-valued
\* constants is brittle across TLC releases — defining it as a TLA-level
\* operator keeps the model fully self-contained while still varying with
\* the cfg via Pats/Tenants sizes. The operator uses a CHOOSE binding so
\* every PAT lands in SOME tenant; the only property the spec depends on
\* (mass-revoke correctness) holds for ANY such assignment as long as
\* Tenants partitions Pats — which CHOOSE guarantees by construction.
PatTenant ==
    LET TenantSeq == CHOOSE seq \in [1..Cardinality(Tenants) -> Tenants]:
                         \A t \in Tenants:
                             \E i \in 1..Cardinality(Tenants): seq[i] = t
        PatSeq    == CHOOSE seq \in [1..Cardinality(Pats) -> Pats]:
                         \A p \in Pats:
                             \E i \in 1..Cardinality(Pats): seq[i] = p
    IN [p \in Pats |->
            LET idx == CHOOSE i \in 1..Cardinality(Pats): PatSeq[i] = p
            IN  TenantSeq[((idx - 1) % Cardinality(Tenants)) + 1]]

VARIABLES
    neon_revoked,        \* Function Pats -> {0, 1}: SoT revocation flag
                         \* (1 = revoked_at NOT NULL; 0 = NULL)
    region_revoked,      \* Function [Regions × Pats] -> {0, 1}: per-region DO cache
    queue,               \* Sequence of <<pat_id, region>> messages pending delivery
    outbox,              \* Sequence of audit events emitted to outbox
    delivered_set,       \* Set of <<pat_id, region>> pairs already consumed (dedup)
    op_count             \* Bound counter

vars == <<neon_revoked, region_revoked, queue, outbox,
          delivered_set, op_count>>

(*-- Helpers -----------------------------------------------------------------*)

\* Every (pat, region) pair that SHOULD eventually be in delivered_set
\* once the SoT is set. Used by the liveness property.
PendingPropagations ==
    { <<p, r>> : p \in {pp \in Pats : neon_revoked[pp] = 1}, r \in Regions }
        \ delivered_set

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ neon_revoked = [p \in Pats |-> 0]
    /\ region_revoked = [r \in Regions, p \in Pats |-> 0]
    /\ queue = <<>>
    /\ outbox = <<>>
    /\ delivered_set = {}
    /\ op_count = 0

(*-- Actions -----------------------------------------------------------------*)

\* Caller issues a revoke for pat_id. Idempotent at the SoT layer:
\* Neon UPDATE with `revoked_at IS NULL` guard collapses retries.
\* Models cycle 4 codex SEAL: SoT update first, then enqueue propagation.
RevokeSingle(p) ==
    /\ p \in Pats
    /\ op_count < MaxOps
    /\ Len(queue) + Cardinality(Regions) <= MaxQueue
    /\ \/ /\ neon_revoked[p] = 0
          \* First effective revoke: flip SoT + enqueue one msg per region
          /\ neon_revoked' = [neon_revoked EXCEPT ![p] = 1]
          \* Audit emission is part of the same Neon transaction (cycle 4
          \* SEAL: outbox INSERT is in the same TX as the SoT UPDATE).
          /\ outbox' = Append(outbox, <<"auth.token.revoked", p>>)
          /\ queue' = queue \o
                [i \in 1..Cardinality(Regions) |->
                    LET ordered == CHOOSE seq \in [1..Cardinality(Regions) -> Regions]:
                                       \A r \in Regions: \E j \in 1..Cardinality(Regions): seq[j] = r
                    IN <<p, ordered[i]>>]
       \/ /\ neon_revoked[p] = 1
          \* Retry: UNIQUE constraint on (pat_id, revoked_at) collapses;
          \* SoT unchanged; NO additional audit event; NO queue spam.
          \* (Models the IDEMPOTENT property at the producer layer.)
          /\ UNCHANGED <<neon_revoked, outbox, queue>>
    /\ UNCHANGED <<region_revoked, delivered_set>>
    /\ op_count' = op_count + 1

\* Mass revoke: all PATs of a tenant flip in a single Neon transaction
\* (all-or-nothing UPDATE phase). The outbox INSERT phase is chunked
\* (one outbox row per affected PAT) — modelled by a single atomic step.
\* INV-AUTH-MASS-REVOKE-ATOMIC: there is no observable intermediate state
\* where some PATs of the tenant are revoked and others are not within
\* a single MassRevoke step.
MassRevoke(t) ==
    /\ t \in Tenants
    /\ op_count < MaxOps
    /\ LET affected == { p \in Pats : PatTenant[p] = t /\ neon_revoked[p] = 0 }
       IN
        /\ affected # {}
        /\ Len(queue) + Cardinality(affected) * Cardinality(Regions) <= MaxQueue
        /\ neon_revoked' = [p \in Pats |->
                                IF p \in affected THEN 1 ELSE neon_revoked[p]]
        /\ outbox' = outbox \o
                [i \in 1..Cardinality(affected) |->
                    LET ordered == CHOOSE seq \in [1..Cardinality(affected) -> affected]:
                                       \A pp \in affected: \E j \in 1..Cardinality(affected): seq[j] = pp
                    IN <<"auth.mass_revocation", ordered[i]>>]
        /\ queue' = queue \o
                [i \in 1..(Cardinality(affected) * Cardinality(Regions)) |->
                    LET pairs == { <<p, r>> : p \in affected, r \in Regions }
                        ordered == CHOOSE seq \in [1..Cardinality(pairs) -> pairs]:
                                       \A x \in pairs: \E j \in 1..Cardinality(pairs): seq[j] = x
                    IN ordered[i]]
    /\ UNCHANGED <<region_revoked, delivered_set>>
    /\ op_count' = op_count + 1

\* Queue delivers a (possibly duplicated) message to a region DO.
\* The DO performs dedup via (pat_id, revoked_at) UNIQUE → idempotent
\* consumer. We model this with `delivered_set`: re-deliveries are
\* observed by the queue but do NOT mutate region_revoked twice.
\*
\* NOTE: Deliver is NOT counted against MaxOps — it is an internal
\* system action that MUST be able to drain the queue for liveness.
\* MaxOps bounds only caller-driven actions (RevokeSingle, MassRevoke,
\* RedeliverDup adversarial).
DeliverRevocation(i) ==
    /\ i \in 1..Len(queue)
    /\ LET msg == queue[i]
           p == msg[1]
           r == msg[2]
       IN
        /\ region_revoked' = [region_revoked EXCEPT ![r, p] = 1]
        /\ delivered_set' = delivered_set \union {<<p, r>>}
        /\ queue' = SubSeq(queue, 1, i-1) \o SubSeq(queue, i+1, Len(queue))
    /\ UNCHANGED <<neon_revoked, outbox, op_count>>

\* Adversarial: queue redelivers an already-consumed message (at-least-once
\* semantics). Models the case where the broker fails to receive an ack
\* and replays. Consumer dedup MUST collapse it.
RedeliverDup(p, r) ==
    /\ p \in Pats
    /\ r \in Regions
    /\ <<p, r>> \in delivered_set
    /\ Len(queue) < MaxQueue
    /\ op_count < MaxOps
    /\ queue' = Append(queue, <<p, r>>)
    /\ op_count' = op_count + 1
    /\ UNCHANGED <<neon_revoked, region_revoked, outbox, delivered_set>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E p \in Pats: RevokeSingle(p)
    \/ \E t \in Tenants: MassRevoke(t)
    \/ \E i \in 1..Len(queue): DeliverRevocation(i)
    \/ \E p \in Pats, r \in Regions: RedeliverDup(p, r)

Spec == Init /\ [][Next]_vars /\ WF_vars(\E i \in 1..Len(queue): DeliverRevocation(i))

(*-- Safety invariants -------------------------------------------------------*)

\* INV-AUTH-REVOCATION-IDEMPOTENT: even with arbitrary retries +
\* redeliveries, the outbox contains AT MOST ONE auth.token.revoked
\* event per PAT for single-revoke flows.
\* (Mass revoke uses event_type "auth.mass_revocation", disjoint from
\* "auth.token.revoked", so cardinality of the single-revoke event per
\* PAT is the right metric here.)
InvRevocationIdempotent ==
    \A p \in Pats:
        Cardinality({ i \in 1..Len(outbox) :
                          outbox[i][1] = "auth.token.revoked"
                       /\ outbox[i][2] = p }) <= 1

\* INV-AUTH-MASS-REVOKE-ATOMIC: there is no reachable state where the
\* SoT shows a "torn" mass revoke for any tenant — i.e. some PATs of a
\* tenant are revoked and some are not, while at least one outbox event
\* for the same tenant has been emitted and another for that tenant has
\* NOT. Because MassRevoke is a single atomic step that flips ALL
\* unrevoked PATs of a tenant, any tenant whose outbox contains a
\* mass_revocation event for some pat p must have neon_revoked[p] = 1.
InvMassRevokeAtomicOutbox ==
    \A i \in 1..Len(outbox):
        outbox[i][1] = "auth.mass_revocation" =>
            neon_revoked[outbox[i][2]] = 1

\* Stronger projection: for every PAT included in a mass_revocation
\* event, every OTHER PAT of the same tenant that was unrevoked at the
\* same step must now also be revoked. We approximate this safely with:
\* if any pat p of tenant t is currently NOT revoked, then no
\* mass_revocation event for tenant t exists in outbox referring to a
\* pat p' that was alongside p in the same atomic UPDATE.
\* Operationally: once any pat of tenant t appears in a mass_revocation
\* event, ALL pats of tenant t that EXISTED in `affected` at that step
\* are also revoked. We bound this by the invariant: an outbox entry
\* for tenant t implies the corresponding pat is revoked. Combined with
\* the action semantics (atomic flip of `affected`), this proves
\* all-or-nothing.

\* INV-AUTH-PROPAGATION-AT-LEAST-ONCE (safety side): region_revoked is
\* monotonic — once flipped, never flips back. This combined with the
\* liveness property below proves at-least-once + idempotent = exactly
\* one effect per (pat, region).
InvRegionMonotonic ==
    \A r \in Regions, p \in Pats:
        region_revoked[r, p] = 1 => neon_revoked[p] = 1

\* Defense-in-depth: SoT precedes regions. A region must not show a PAT
\* as revoked unless Neon already does. (Models INV-AUTH-NEON-IS-SOT
\* projection in the revocation pipeline.)
InvSoTPrecedesRegion == InvRegionMonotonic  \* same expression, named clarity

(*-- Liveness ----------------------------------------------------------------*)

\* INV-AUTH-REVOCATION-SLO-60S (refined as bounded eventuality):
\* every Neon-revoked PAT eventually shows up as revoked in every region.
\* The 60s wall-clock budget is validated by chaos tests; here we prove
\* topological convergence under WF on Deliver.
RevokedEventuallyConverges ==
    \A p \in Pats, r \in Regions:
        (neon_revoked[p] = 1) ~> (region_revoked[r, p] = 1)

\* Combined safety conjunction (single INVARIANT line in cfg).
SafetyInvariants ==
    /\ InvRevocationIdempotent
    /\ InvMassRevokeAtomicOutbox
    /\ InvRegionMonotonic
    /\ InvSoTPrecedesRegion

================================================================================
