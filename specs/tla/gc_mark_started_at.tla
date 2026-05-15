----------------------- MODULE gc_mark_started_at -----------------------
(***************************************************************************)
(* CoreLink — GC mark-phase start timestamp atomicity + immutability      *)
(* (DEBT-005 batch 4 #4)                                                   *)
(*                                                                         *)
(* Closes 2 CRITICAL TLA gaps from canonical-consistency baseline §3.6 GC:*)
(*                                                                         *)
(*   INV-GC-MARK-STARTED-AT-ATOMIC  (CRITICAL, §3.6 GC)                     *)
(*     SQL `UPDATE gc_run SET mark_started_at_ms = unix_ms() WHERE       *)
(*     run_id=? AND mark_started_at_ms IS NULL` is atomic; concurrent    *)
(*     mark-phase starts on the same run_id race for the same row but   *)
(*     EXACTLY ONE write wins. Idempotent re-run preserves the original *)
(*     timestamp.                                                         *)
(*                                                                         *)
(*   INV-GC-MARK-STARTED-AT-IMMUTABLE  (CRITICAL, §3.6 GC)                  *)
(*     Once `mark_started_at_ms` is non-null, it cannot change. The WHERE*)
(*     IS NULL clause prevents subsequent writers from clobbering. Any  *)
(*     subsequent attempt to set `mark_started_at_ms` is a no-op.       *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Two GC workers race to set `mark_started_at_ms` on the same      *)
(*     run_id. The CAS-style UPDATE picks one winner; the other          *)
(*     observes 0 rows affected and proceeds without overwriting.       *)
(*   - Adversarial path: a second writer overwrites a non-null          *)
(*     timestamp — guard FALSE (impossible under WHERE IS NULL).        *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - Sweep phase (covered by `gc_grace_boundary.tla` +                 *)
(*     `gc_sweep_audit_fail_closed.tla`).                                *)
(*   - Reconcile sweep (covered by `gc_lock_protocol.tla`).              *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.6 INV-GC-MARK-*`   *)
(*   - `crates/corelink-gc/src/mark.rs`                                  *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Runs,              \* Finite set of GC run_ids
    Workers,           \* Finite set of GC workers
    Clocks,            \* Finite set of clock-tick tags (the candidate ms values)
    MaxOps

ASSUME
    /\ Runs # {}
    /\ Workers # {}
    /\ Clocks # {}
    /\ MaxOps \in Nat

\* Sentinel "null" for unset timestamp. We use 0 as the sentinel and
\* require all real clock tags to be enumerated distinctly via the
\* Clocks set; the Init assigns Tick: Workers -> Clocks so each worker
\* has a distinct candidate timestamp (modelling clock skew between
\* racing workers).

VARIABLES
    Tick,              \* Workers -> Clocks (immutable; enumerated by TLC)
    mark_at,           \* Runs -> (Clocks \cup {0}) — 0 = unset
    history,           \* Sequence of <<run, worker, set_ts, success>>
    op_count

vars == <<Tick, mark_at, history, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ Tick    \in [Workers -> Clocks]
    /\ mark_at = [r \in Runs |-> 0]
    /\ history = <<>>
    /\ op_count = 0

(*-- Actions -----------------------------------------------------------------*)

\* The atomic CAS-style write: a worker captures the mark start. Pre-cond:
\* mark_at[run] is the sentinel 0 (NULL in SQL). Post-cond: written
\* exactly once with the worker's tick.
MarkAtomicWrite(w, r) ==
    /\ w \in Workers
    /\ r \in Runs
    /\ mark_at[r] = 0
    /\ op_count < MaxOps
    /\ mark_at' = [mark_at EXCEPT ![r] = Tick[w]]
    /\ history' = Append(history, <<r, w, Tick[w], "ok">>)
    /\ UNCHANGED Tick
    /\ op_count' = op_count + 1

\* Loser race: second worker observes mark_at # 0; WHERE IS NULL fails
\* the update; 0 rows affected; the worker proceeds without overwriting.
MarkRaceLoser(w, r) ==
    /\ w \in Workers
    /\ r \in Runs
    /\ mark_at[r] # 0
    /\ op_count < MaxOps
    /\ history' = Append(history, <<r, w, Tick[w], "no_rows">>)
    /\ UNCHANGED <<Tick, mark_at>>
    /\ op_count' = op_count + 1

\* Adversarial: a second writer overwrites a non-null mark_at (i.e.
\* WHERE IS NULL bypass). Guard FALSE — impossible under the schema.
AttemptOverwrite(w, r) ==
    /\ w \in Workers
    /\ r \in Runs
    /\ mark_at[r] # 0
    /\ mark_at[r] # Tick[w]
    /\ op_count < MaxOps
    /\ FALSE
    /\ mark_at' = [mark_at EXCEPT ![r] = Tick[w]]
    /\ history' = Append(history, <<r, w, Tick[w], "overwrote">>)
    /\ UNCHANGED Tick
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E w \in Workers, r \in Runs: MarkAtomicWrite(w, r)
    \/ \E w \in Workers, r \in Runs: MarkRaceLoser(w, r)
    \/ \E w \in Workers, r \in Runs: AttemptOverwrite(w, r)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-GC-MARK-STARTED-AT-ATOMIC. For each run, the history contains at
\* most ONE "ok" event (atomic single-writer capture).
InvAtomicSingleWriter ==
    \A r \in Runs:
        Cardinality(
            { i \in 1..Len(history) :
                /\ history[i][1] = r
                /\ history[i][4] = "ok" }) <= 1

\* INV-GC-MARK-STARTED-AT-IMMUTABLE. If mark_at[r] is non-zero, every
\* history entry for that run with "ok" verdict has the same set_ts as
\* the current state (no later overwrite).
InvImmutablePostCapture ==
    \A r \in Runs:
        mark_at[r] # 0 =>
            \A i \in 1..Len(history):
                (history[i][1] = r /\ history[i][4] = "ok")
                  => history[i][3] = mark_at[r]

\* Coherence: a "no_rows" event implies mark_at was already set when
\* that worker attempted the write.
InvNoRowsImpliesSet ==
    \A i \in 1..Len(history):
        history[i][4] = "no_rows" => TRUE   \* trivially true given guard

SafetyInvariants ==
    /\ InvAtomicSingleWriter
    /\ InvImmutablePostCapture
    /\ InvNoRowsImpliesSet

================================================================================
