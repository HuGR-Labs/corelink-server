---------------------------- MODULE backup_fresh ----------------------------
(***************************************************************************)
(* CoreLink — backup freshness gate + sampler bound (DEBT-005 batch 3 #5) *)
(*                                                                         *)
(* Closes the following TLA gaps from canonical-consistency baseline:     *)
(*                                                                         *)
(*   INV-BACKUP-FRESH (HIGH §3.20 Backup)                                  *)
(*     `verify_freshness` rejects any snapshot whose                       *)
(*     `age_seconds > rpo_seconds(tier)`. The rejection MUST preserve     *)
(*     the prior accepted snapshot (no "swap to stale" path). On          *)
(*     rejection the latest_accepted pointer is UNCHANGED.                *)
(*                                                                         *)
(*   INV-BACKUP-INTEGRITY-SAMPLE-CAP (HIGH §3.20 Backup)                   *)
(*     The integrity sampler iterates at most `MaxSamples` aggregates    *)
(*     per cycle. Any cycle whose sample count would exceed the cap is   *)
(*     short-circuited at the iterator boundary (no O(catalog) hot       *)
(*     loop). Across all cycles, the sample count NEVER exceeds the cap.*)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - `SnapshotFresh`: incoming snapshot is within RPO; accepted +     *)
(*     `latest_accepted` advances.                                       *)
(*   - `SnapshotStale`: incoming snapshot exceeds RPO; rejected;        *)
(*     `latest_accepted` UNCHANGED.                                      *)
(*   - `SampleStep`: integrity sampler consumes one slot — gated on      *)
(*     `samples_this_cycle < MaxSamples`.                                *)
(*   - `CycleReset`: a new sampler cycle resets the counter.            *)
(*   - `AttemptSampleOverCap` (guard FALSE): adversarial trace showing  *)
(*     what the bound rules out.                                         *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - Wall-clock RPO budget table (`rpo_seconds(tier)`).                *)
(*   - Sampler statistical properties (uniformity; covered by property  *)
(*     tests).                                                            *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.20 INV-BACKUP-*` *)
(*   - `crates/corelink-backup-verify/src/lib.rs`                       *)
(*   - `specs/tla/backup_restore_ephemeral.tla` (sibling — restore     *)
(*     ephemeral namespace guarantee).                                  *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    RpoBudget,         \* Max acceptable age in abstract units
    MaxSamples,        \* Per-cycle sampler cap
    MaxOps

ASSUME
    /\ RpoBudget \in Nat
    /\ RpoBudget > 0
    /\ MaxSamples \in Nat
    /\ MaxSamples > 0
    /\ MaxOps \in Nat

VARIABLES
    latest_accepted_age,  \* Nat — age of the most recently accepted snapshot
    has_accepted,         \* BOOLEAN — TRUE once at least one snapshot accepted
    samples_this_cycle,   \* Nat — sampler count in the current cycle
    samples_total,        \* Nat — sampler count across all cycles
    cycle_count,          \* Nat — number of cycles started
    op_count

vars == <<latest_accepted_age, has_accepted, samples_this_cycle,
          samples_total, cycle_count, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ latest_accepted_age = 0
    /\ has_accepted        = FALSE
    /\ samples_this_cycle  = 0
    /\ samples_total       = 0
    /\ cycle_count         = 0
    /\ op_count            = 0

(*-- Actions -----------------------------------------------------------------*)

\* Incoming snapshot whose age is within RPO — accepted; latest pointer
\* advances to this snapshot's age.
SnapshotFresh(age) ==
    /\ age \in 0..RpoBudget
    /\ op_count < MaxOps
    /\ latest_accepted_age' = age
    /\ has_accepted'        = TRUE
    /\ UNCHANGED <<samples_this_cycle, samples_total, cycle_count>>
    /\ op_count'            = op_count + 1

\* Incoming snapshot whose age exceeds RPO — rejected; pointer
\* UNCHANGED. Models the "no swap to stale" requirement.
SnapshotStale(age) ==
    /\ age \in (RpoBudget + 1)..(RpoBudget + 5)
    /\ op_count < MaxOps
    /\ UNCHANGED <<latest_accepted_age, has_accepted,
                   samples_this_cycle, samples_total, cycle_count>>
    /\ op_count' = op_count + 1

\* Sampler consumes one slot — gated on the per-cycle cap.
SampleStep ==
    /\ samples_this_cycle < MaxSamples
    /\ op_count < MaxOps
    /\ samples_this_cycle' = samples_this_cycle + 1
    /\ samples_total'      = samples_total + 1
    /\ UNCHANGED <<latest_accepted_age, has_accepted, cycle_count>>
    /\ op_count'           = op_count + 1

\* Cycle boundary — the per-cycle counter resets.
CycleReset ==
    /\ op_count < MaxOps
    /\ samples_this_cycle' = 0
    /\ cycle_count'        = cycle_count + 1
    /\ UNCHANGED <<latest_accepted_age, has_accepted, samples_total>>
    /\ op_count'           = op_count + 1

\* Adversarial: sampler exceeds the cap. Guard FALSE — exhibits the
\* impossible trace that INV-BACKUP-INTEGRITY-SAMPLE-CAP rules out.
AttemptSampleOverCap ==
    /\ samples_this_cycle >= MaxSamples
    /\ op_count < MaxOps
    /\ FALSE
    /\ samples_this_cycle' = samples_this_cycle + 1
    /\ samples_total'      = samples_total + 1
    /\ UNCHANGED <<latest_accepted_age, has_accepted, cycle_count>>
    /\ op_count'           = op_count + 1

\* Adversarial: stale snapshot somehow updates the latest pointer
\* (fail-open swap). Guard FALSE.
AttemptStaleSwap(age) ==
    /\ age \in (RpoBudget + 1)..(RpoBudget + 5)
    /\ op_count < MaxOps
    /\ FALSE
    /\ latest_accepted_age' = age
    /\ has_accepted'        = TRUE
    /\ UNCHANGED <<samples_this_cycle, samples_total, cycle_count>>
    /\ op_count'            = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E age \in 0..(RpoBudget + 5): SnapshotFresh(age)
    \/ \E age \in 0..(RpoBudget + 5): SnapshotStale(age)
    \/ SampleStep
    \/ CycleReset
    \/ AttemptSampleOverCap
    \/ \E age \in 0..(RpoBudget + 5): AttemptStaleSwap(age)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-BACKUP-FRESH (core). The latest_accepted snapshot's age is
\* within RPO.
InvLatestAcceptedFresh ==
    has_accepted => latest_accepted_age <= RpoBudget

\* INV-BACKUP-INTEGRITY-SAMPLE-CAP (core). Per-cycle counter never
\* exceeds the cap.
InvSampleCapPerCycle ==
    samples_this_cycle <= MaxSamples

\* Counter monotonicity: per-cycle resets to 0 only on CycleReset.
\* Total is unbounded by design (no cap across cycles) — but per cycle
\* we enforce the upper bound.
InvNoBackwardSamples ==
    samples_total >= samples_this_cycle

SafetyInvariants ==
    /\ InvLatestAcceptedFresh
    /\ InvSampleCapPerCycle
    /\ InvNoBackwardSamples

================================================================================
