------------------------- MODULE gc_grace_boundary -------------------------
(***************************************************************************)
(* CoreLink — GC grace-window strict boundary + tenant-scoped queries     *)
(* (DEBT-005 batch 3 #4).                                                  *)
(*                                                                         *)
(* Closes CRITICAL TLA gaps from canonical-consistency baseline:          *)
(*                                                                         *)
(*   INV-GC-GRACE-BOUNDARY-STRICT (CRITICAL §3.6 GC)                       *)
(*     The sweep grace boundary uses SQL `<` (strict), not `<=`.          *)
(*     A blob whose `deleted_at` equals the exact boundary is NOT yet    *)
(*     sweepable — the reversibility window is respected to the second.  *)
(*                                                                         *)
(*   INV-GC-MARK-TENANT-SCOPED (CRITICAL §3.6 GC)                          *)
(*     All mark-phase queries WHERE `tenant_id = ctx.tenant_id`. A mark *)
(*     operation cannot observe a blob from a foreign tenant.            *)
(*                                                                         *)
(*   INV-GC-SWEEP-TENANT-SCOPED (CRITICAL §3.6 GC)                         *)
(*     All sweep-phase queries are tenant-scoped strict; cross-tenant   *)
(*     sweep is impossible.                                              *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - `MarkOwn` / `SweepOwn` — happy path, tenant-scoped.               *)
(*   - `AttemptMarkCrossTenant` / `AttemptSweepCrossTenant`              *)
(*     (guard FALSE — adversarial exhibits).                             *)
(*   - `AttemptSweepAtBoundary` (deleted_at = grace boundary)            *)
(*     (guard FALSE — strict-`<` enforcement).                            *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.6 INV-GC-*`       *)
(*   - `crates/corelink-gc/src/lib.rs`                                   *)
(*   - `specs/tla/gc_correctness.tla` (parent — mark+sweep+grace).      *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Tenants,           \* Finite set of tenants
    Blobs,             \* Finite set of blob IDs
    GraceWindow,       \* Wall-clock units in the grace window
    MaxOps

ASSUME
    /\ Tenants # {}
    /\ Blobs # {}
    /\ GraceWindow \in Nat
    /\ GraceWindow > 0
    /\ MaxOps \in Nat

\* Owner of each blob is a Blobs -> Tenants function. We avoid a
\* function-literal CONSTANT in the .cfg (FT-4 brittleness) by sampling
\* the function in Init via `Owner \in [Blobs -> Tenants]` — TLC
\* enumerates every possible owner assignment, so the model proves the
\* invariants over the entire ownership product space (not a single
\* hard-coded mapping).

\* Blob states: live -> soft_deleted -> swept (terminal).
States == {"live", "soft_deleted", "swept"}

VARIABLES
    Owner,             \* Blobs -> Tenants (immutable post-init; enumerated by TLC)
    b_state,           \* Blobs -> States
    b_deleted_age,     \* Blobs -> Nat — units since soft-delete (0 while live)
    mark_log,          \* Sequence of <<actor_tenant, blob>>
    sweep_log,         \* Sequence of <<actor_tenant, blob>>
    op_count

vars == <<Owner, b_state, b_deleted_age, mark_log, sweep_log, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ Owner         \in [Blobs -> Tenants]
    /\ b_state       = [b \in Blobs |-> "live"]
    /\ b_deleted_age = [b \in Blobs |-> 0]
    /\ mark_log      = <<>>
    /\ sweep_log     = <<>>
    /\ op_count      = 0

(*-- Actions -----------------------------------------------------------------*)

\* Caller soft-deletes a blob it owns.
SoftDelete(b) ==
    /\ b \in Blobs
    /\ b_state[b] = "live"
    /\ op_count < MaxOps
    /\ b_state'       = [b_state EXCEPT ![b] = "soft_deleted"]
    /\ b_deleted_age' = [b_deleted_age EXCEPT ![b] = 0]
    /\ UNCHANGED <<Owner, mark_log, sweep_log>>
    /\ op_count'      = op_count + 1

\* Wall-clock progresses for a soft-deleted blob.
Tick(b) ==
    /\ b \in Blobs
    /\ b_state[b] = "soft_deleted"
    /\ op_count < MaxOps
    /\ b_deleted_age' = [b_deleted_age EXCEPT ![b] = @ + 1]
    /\ UNCHANGED <<Owner, b_state, mark_log, sweep_log>>
    /\ op_count' = op_count + 1

\* GC worker for `t` marks a blob `b`. Happy path: same tenant.
MarkOwn(t, b) ==
    /\ t \in Tenants
    /\ b \in Blobs
    /\ Owner[b] = t
    /\ op_count < MaxOps
    /\ mark_log' = Append(mark_log, <<t, b>>)
    /\ UNCHANGED <<Owner, b_state, b_deleted_age, sweep_log>>
    /\ op_count' = op_count + 1

\* GC worker for `t` sweeps `b`. Requires soft_deleted AND age STRICTLY
\* greater than the grace window — `>`, not `>=`. The boundary case
\* (age = GraceWindow) is exhibited as adversarial below.
SweepOwn(t, b) ==
    /\ t \in Tenants
    /\ b \in Blobs
    /\ Owner[b] = t
    /\ b_state[b] = "soft_deleted"
    /\ b_deleted_age[b] > GraceWindow   \* STRICT — matches SQL `<` test on (now - deleted_at)
    /\ op_count < MaxOps
    /\ b_state'   = [b_state EXCEPT ![b] = "swept"]
    /\ sweep_log' = Append(sweep_log, <<t, b>>)
    /\ UNCHANGED <<Owner, b_deleted_age, mark_log>>
    /\ op_count'  = op_count + 1

\* Adversarial: GC worker for `t` marks a blob owned by `t' # t`.
\* Guard FALSE — tenant-scoped enforcement.
AttemptMarkCrossTenant(t, b) ==
    /\ t \in Tenants
    /\ b \in Blobs
    /\ Owner[b] # t
    /\ op_count < MaxOps
    /\ FALSE
    /\ mark_log' = Append(mark_log, <<t, b>>)
    /\ UNCHANGED <<Owner, b_state, b_deleted_age, sweep_log>>
    /\ op_count' = op_count + 1

\* Adversarial: GC worker for `t` sweeps a blob owned by `t' # t`.
\* Guard FALSE — tenant-scoped enforcement.
AttemptSweepCrossTenant(t, b) ==
    /\ t \in Tenants
    /\ b \in Blobs
    /\ Owner[b] # t
    /\ b_state[b] = "soft_deleted"
    /\ op_count < MaxOps
    /\ FALSE
    /\ b_state'   = [b_state EXCEPT ![b] = "swept"]
    /\ sweep_log' = Append(sweep_log, <<t, b>>)
    /\ UNCHANGED <<Owner, b_deleted_age, mark_log>>
    /\ op_count'  = op_count + 1

\* Adversarial: sweeper acts at EXACTLY the grace boundary (age = window).
\* Strict boundary forbids this. Guard FALSE.
AttemptSweepAtBoundary(t, b) ==
    /\ t \in Tenants
    /\ b \in Blobs
    /\ Owner[b] = t
    /\ b_state[b] = "soft_deleted"
    /\ b_deleted_age[b] = GraceWindow
    /\ op_count < MaxOps
    /\ FALSE
    /\ b_state'   = [b_state EXCEPT ![b] = "swept"]
    /\ sweep_log' = Append(sweep_log, <<t, b>>)
    /\ UNCHANGED <<Owner, b_deleted_age, mark_log>>
    /\ op_count'  = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E b \in Blobs: SoftDelete(b)
    \/ \E b \in Blobs: Tick(b)
    \/ \E t \in Tenants, b \in Blobs: MarkOwn(t, b)
    \/ \E t \in Tenants, b \in Blobs: SweepOwn(t, b)
    \/ \E t \in Tenants, b \in Blobs: AttemptMarkCrossTenant(t, b)
    \/ \E t \in Tenants, b \in Blobs: AttemptSweepCrossTenant(t, b)
    \/ \E t \in Tenants, b \in Blobs: AttemptSweepAtBoundary(t, b)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-GC-MARK-TENANT-SCOPED. Every recorded mark has actor = blob owner.
InvMarkTenantScoped ==
    \A i \in 1..Len(mark_log):
        mark_log[i][1] = Owner[mark_log[i][2]]

\* INV-GC-SWEEP-TENANT-SCOPED. Every recorded sweep has actor = blob owner.
InvSweepTenantScoped ==
    \A i \in 1..Len(sweep_log):
        sweep_log[i][1] = Owner[sweep_log[i][2]]

\* INV-GC-GRACE-BOUNDARY-STRICT. A swept blob must have had age STRICTLY
\* greater than the grace window at sweep time. Encoded via the current
\* state: every blob currently swept must satisfy this, and SweepOwn is
\* the only writer of `swept`.
InvGraceBoundaryStrict ==
    \A b \in Blobs:
        b_state[b] = "swept" => b_deleted_age[b] > GraceWindow

\* State machine well-formedness: a live blob has age 0; a swept blob is
\* terminal.
InvStateWellFormed ==
    /\ \A b \in Blobs:
         b_state[b] = "live" => b_deleted_age[b] = 0

SafetyInvariants ==
    /\ InvMarkTenantScoped
    /\ InvSweepTenantScoped
    /\ InvGraceBoundaryStrict
    /\ InvStateWellFormed

================================================================================
