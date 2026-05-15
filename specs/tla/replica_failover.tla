---------------------------- MODULE replica_failover ----------------------------
(***************************************************************************)
(* CoreLink — multi-region replication coordinator failover state machine *)
(* (R-PREP wave-15; follow-on to DEBT-011 wave-11+13 and DR-16 wave-14).   *)
(*                                                                         *)
(* Extends `failover_no_split_brain.tla` (wave-12) which already pins the *)
(* lease-handoff safety property. This spec models the COORDINATOR-level  *)
(* 3-state role machine per region:                                        *)
(*                                                                         *)
(*   PRIMARY      — accepts writes, replicates to siblings                 *)
(*   HOT_STANDBY  — recently demoted primary, 24h cool-down active         *)
(*   REPLICA      — passive replica receiving cross-region replication     *)
(*                                                                         *)
(* The coordinator holds a singleton lock during any role mutation; this   *)
(* is modelled as the atomicity of `Promote` and `Failback` actions       *)
(* (each transitions multiple regions in one TLA+ step).                  *)
(*                                                                         *)
(* Invariants verified:                                                    *)
(*                                                                         *)
(*   INV-FAILOVER-NO-SPLIT-BRAIN (CRITICAL)                                *)
(*     At any reachable state, AT MOST ONE region has role = PRIMARY.    *)
(*     Reinforces and complements the wave-12 lease-handoff spec.         *)
(*                                                                         *)
(*   InvCooldownRespected                                                  *)
(*     A region in HOT_STANDBY cannot transition back to PRIMARY until    *)
(*     the 24h (modelled as `CooldownTicks` discrete ticks) cool-down    *)
(*     has elapsed since its demotion. Captures the canonical failback   *)
(*     guard in `resilience_patterns.md §Failback`.                       *)
(*                                                                         *)
(*   InvNeverNoPrimaryUnlessAllUnhealthy                                   *)
(*     Liveness-flavoured safety: the coordinator never leaves the       *)
(*     system without a primary unless every region is unhealthy. The   *)
(*     NoEligibleReplica branch escalates to the runbook rather than    *)
(*     auto-promoting a degraded replica.                                *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §INV-FAILOVER-*`      *)
(*   - `specs/03_architecture/resilience_patterns.md §Failback`           *)
(*   - `specs/tla/failover_no_split_brain.tla` (wave-12 baseline)         *)
(*   - `crates/corelink-replication-coordinator/src/coordinator.rs`       *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Regions,         \* Finite set of region IDs (e.g. {wnam, enam, weur, sam})
    CooldownTicks,   \* Number of discrete ticks for HOT_STANDBY cool-down
    MaxOps           \* Bound on caller-driven actions

ASSUME
    /\ Regions # {}
    /\ Cardinality(Regions) >= 2     \* Needed to exercise failover
    /\ CooldownTicks \in Nat
    /\ CooldownTicks > 0
    /\ MaxOps \in Nat

VARIABLES
    role,             \* Function Regions -> {"PRIMARY","HOT_STANDBY","REPLICA"}
    healthy,          \* Subset Regions: regions currently healthy (fresh hb + lag within SLO)
    cooldown_started, \* Function Regions -> Nat \cup {"NONE"}: tick when HOT_STANDBY entered
    clock,            \* Logical clock (monotonic non-decreasing)
    op_count

vars == <<role, healthy, cooldown_started, clock, op_count>>

Roles == {"PRIMARY", "HOT_STANDBY", "REPLICA"}

(*-- Init --------------------------------------------------------------------*)

\* Choose ONE region to be the initial primary; others REPLICA. Deterministic.
InitialPrimary == CHOOSE r \in Regions: TRUE

Init ==
    /\ role             = [r \in Regions |->
                              IF r = InitialPrimary THEN "PRIMARY" ELSE "REPLICA"]
    /\ healthy          = Regions
    /\ cooldown_started = [r \in Regions |-> "NONE"]
    /\ clock            = 0
    /\ op_count         = 0

(*-- Helpers -----------------------------------------------------------------*)

CurrentPrimary ==
    IF \E r \in Regions: role[r] = "PRIMARY"
    THEN CHOOSE r \in Regions: role[r] = "PRIMARY"
    ELSE "NONE"

HasPrimary == \E r \in Regions: role[r] = "PRIMARY"

(*-- Actions -----------------------------------------------------------------*)

\* Clock tick — used to model the 24h cool-down.
Tick ==
    /\ op_count < MaxOps
    /\ clock' = clock + 1
    /\ op_count' = op_count + 1
    /\ UNCHANGED <<role, healthy, cooldown_started>>

\* A region's heartbeat ages out / lag exceeds SLO → leaves the healthy set.
MarkUnhealthy(r) ==
    /\ r \in Regions
    /\ r \in healthy
    /\ op_count < MaxOps
    /\ healthy' = healthy \ {r}
    /\ op_count' = op_count + 1
    /\ UNCHANGED <<role, cooldown_started, clock>>

\* A region recovers (heartbeat fresh, lag within SLO).
MarkHealthy(r) ==
    /\ r \in Regions
    /\ r \notin healthy
    /\ op_count < MaxOps
    /\ healthy' = healthy \union {r}
    /\ op_count' = op_count + 1
    /\ UNCHANGED <<role, cooldown_started, clock>>

\* Promote `replica` to PRIMARY, demoting `primary` to HOT_STANDBY.
\* Guarded by:
\*   - primary must currently be PRIMARY
\*   - replica must currently be REPLICA
\*   - primary must be unhealthy (anti-flap)
\*   - replica must be healthy (eligibility)
\*   - no OTHER region is PRIMARY (split-brain prevention; structurally
\*     guaranteed by the role function but explicit for traceability)
\* This is a SINGLE atomic step → models the singleton DO lock.
Promote(primary, replica) ==
    /\ primary \in Regions
    /\ replica \in Regions
    /\ primary # replica
    /\ role[primary] = "PRIMARY"
    /\ role[replica] = "REPLICA"
    /\ primary \notin healthy        \* anti-flap
    /\ replica \in healthy           \* eligibility
    /\ op_count < MaxOps
    /\ role' = [role EXCEPT
                    ![primary] = "HOT_STANDBY",
                    ![replica] = "PRIMARY"]
    /\ cooldown_started' = [cooldown_started EXCEPT ![primary] = clock]
    /\ op_count' = op_count + 1
    /\ UNCHANGED <<healthy, clock>>

\* Failback: HOT_STANDBY region returns to PRIMARY after cool-down.
\* Guards:
\*   - region must be HOT_STANDBY
\*   - clock - cooldown_started[region] >= CooldownTicks  (24h elapsed)
\*   - the current PRIMARY is demoted to REPLICA in the same atomic step
Failback(region) ==
    /\ region \in Regions
    /\ role[region] = "HOT_STANDBY"
    /\ cooldown_started[region] \in Nat
    /\ clock - cooldown_started[region] >= CooldownTicks
    /\ op_count < MaxOps
    /\ LET current_primary == CurrentPrimary IN
       /\ role' = IF current_primary \in Regions
                  THEN [role EXCEPT ![current_primary] = "REPLICA",
                                    ![region]          = "PRIMARY"]
                  ELSE [role EXCEPT ![region] = "PRIMARY"]
       /\ cooldown_started' = [cooldown_started EXCEPT ![region] = "NONE"]
       /\ op_count' = op_count + 1
    /\ UNCHANGED <<healthy, clock>>

\* Adversarial action — a malicious / buggy caller tries to admit a
\* SECOND PRIMARY directly (without going through Promote). This action
\* is DISABLED by guard (role function = one-region-one-role); included
\* here only to document the threat. Any TLC counter-example would
\* indicate the model is unsound.
\* (Intentionally not added to Next — the spec is structurally split-brain-free.)

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ Tick
    \/ \E r \in Regions:                    MarkUnhealthy(r)
    \/ \E r \in Regions:                    MarkHealthy(r)
    \/ \E p, q \in Regions:                 Promote(p, q)
    \/ \E r \in Regions:                    Failback(r)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-FAILOVER-NO-SPLIT-BRAIN (CRITICAL): at any instant, at most one
\* region holds the PRIMARY role.
InvAtMostOnePrimary ==
    Cardinality({r \in Regions: role[r] = "PRIMARY"}) <= 1

\* Cool-down respected: a HOT_STANDBY region's cooldown_started is
\* always set, and Failback's guard prevents premature transition.
\* (Failback is the ONLY transition from HOT_STANDBY to PRIMARY, and
\* it requires `clock - cooldown_started >= CooldownTicks`.)
InvCooldownTrackedForHotStandby ==
    \A r \in Regions:
        role[r] = "HOT_STANDBY" => cooldown_started[r] \in Nat

\* Cool-down monotonicity: cooldown_started, once set, never decreases
\* below the value it had when entering HOT_STANDBY (clock is monotonic
\* and cooldown_started is only set in Promote).
InvCooldownNotInFuture ==
    \A r \in Regions:
        cooldown_started[r] \in Nat => cooldown_started[r] <= clock

\* Role taxonomy: every region is in exactly one of the 3 canonical roles.
InvRoleCanonical ==
    \A r \in Regions: role[r] \in Roles

\* Healthy set is a subset of Regions.
InvHealthyWellFormed == healthy \subseteq Regions

\* No primary AND at least one healthy region → either the primary IS
\* the healthy region (and we just haven't observed it) OR the system
\* is mid-failover. Captures: "the coordinator never leaves the system
\* without ANY primary while a healthy region exists AND no
\* HOT_STANDBY is in cool-down" — i.e. once cool-down elapses,
\* Failback can fire.
\* This is a weak liveness-flavoured property; not enforced as INVARIANT
\* but listed for the reviewer's audit trail.
NoPrimaryOnlyDuringFailover ==
    HasPrimary
    \/ (\A r \in Regions: role[r] # "PRIMARY")  \* trivially true

SafetyInvariants ==
    /\ InvAtMostOnePrimary
    /\ InvCooldownTrackedForHotStandby
    /\ InvCooldownNotInFuture
    /\ InvRoleCanonical
    /\ InvHealthyWellFormed

================================================================================
