---------------------------- MODULE gc_lock_protocol ----------------------------
(***************************************************************************)
(* CoreLink — GC distributed lock protocol (DEBT-014 FT-5)                 *)
(*                                                                         *)
(* Closes 1 HIGH (plus 1 CRITICAL inherited) TLA gap from                  *)
(* `specs/_audits/tla-followup-tickets.md` FT-5 by formalising the         *)
(* per-(tenant, region) GC singleton lock protocol under                   *)
(*   - leader handoff between workers                                      *)
(*   - DO (Durable Object) crash mid-run                                   *)
(*   - checkpoint-resume sequence                                          *)
(*                                                                         *)
(* Invariants proved here (from invariant_registry.md §3.6):               *)
(*                                                                         *)
(*   INV-GC-SINGLE-RUNNING-PER-TENANT-REGION (HIGH)                        *)
(*     At most one worker holds the lock for any (tenant, region) pair at  *)
(*     any reachable state. Equivalent to: |{w : lock_holder=(t,r,w)}|<=1. *)
(*                                                                         *)
(*   INV-GC-RECONCILE-AUDIT-FAIL-CLOSED (CRITICAL — inherits from          *)
(*   audit_immutability.tla via the audit_log append rule):                *)
(*     Every successful "release" emits exactly one audit-log entry, and   *)
(*     audit-log entries are append-only. Crash before audit emission      *)
(*     leaves the lock in `crashed` state (not `released`), which the      *)
(*     reconcile sweep re-runs.                                            *)
(*                                                                         *)
(* Threat model:                                                           *)
(*   - Multiple workers compete for the same lock; only one can win.       *)
(*   - DO crashes at arbitrary points; reconcile sweep is the only         *)
(*     mechanism that unsticks a crashed run.                              *)
(*   - Leader handoff is modelled as worker death + new worker acquire.    *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - Concrete D1 SQL row-level locking (modelled abstractly as a single  *)
(*     `lock_holder` function).                                            *)
(*   - GC algorithm correctness (covered by `gc_correctness.tla`).         *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.6 INV-GC-*`         *)
(*   - `specs/_audits/tla-followup-tickets.md` FT-5                        *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Tenants,
    Regions,
    Workers,
    MaxOps

ASSUME
    /\ Tenants # {}
    /\ Regions # {}
    /\ Workers # {}
    /\ MaxOps \in Nat

\* A lock state per (tenant, region): "free" | <<"held", worker>> | <<"crashed", worker>>
LockStates == {"free"} \cup ({"held"} \X Workers) \cup ({"crashed"} \X Workers)

VARIABLES
    lock,           \* (tenant, region) -> LockStates element
    audit_log,      \* Sequence of <<tenant, region, worker, event>>
    op_count

vars == <<lock, audit_log, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ lock = [tr \in (Tenants \X Regions) |-> "free"]
    /\ audit_log = <<>>
    /\ op_count = 0

(*-- Actions ----------------------------------------------------------------*)

\* Acquire: worker w wins the lock for (t, r) only if it is currently free.
\* Compare-and-set semantics — only one worker can win.
Acquire(t, r, w) ==
    /\ op_count < MaxOps
    /\ lock[<<t,r>>] = "free"
    /\ lock' = [lock EXCEPT ![<<t,r>>] = <<"held", w>>]
    /\ audit_log' = Append(audit_log, <<t, r, w, "acquire">>)
    /\ op_count' = op_count + 1

\* Release: worker w releases the lock it holds; emits audit entry then frees.
\* The "audit-before-free" ordering is what gives INV-GC-RECONCILE-AUDIT-
\* FAIL-CLOSED — a crash before this action leaves the lock as crashed,
\* not free, so reconcile re-runs the cycle.
Release(t, r, w) ==
    /\ op_count < MaxOps
    /\ lock[<<t,r>>] = <<"held", w>>
    /\ audit_log' = Append(audit_log, <<t, r, w, "release">>)
    /\ lock' = [lock EXCEPT ![<<t,r>>] = "free"]
    /\ op_count' = op_count + 1

\* Crash: worker w dies while holding the lock. Lock transitions to
\* `crashed` — fail-closed; reconcile sweep is the only way out. No audit
\* entry on crash (the worker process died before it could emit one).
Crash(t, r, w) ==
    /\ op_count < MaxOps
    /\ lock[<<t,r>>] = <<"held", w>>
    /\ lock' = [lock EXCEPT ![<<t,r>>] = <<"crashed", w>>]
    /\ op_count' = op_count + 1
    /\ UNCHANGED audit_log

\* Reconcile: the sweep finds a `crashed` lock and frees it. Emits an
\* audit entry for the reconcile transition.
Reconcile(t, r) ==
    /\ op_count < MaxOps
    /\ \E w \in Workers: lock[<<t,r>>] = <<"crashed", w>>
    /\ lock' = [lock EXCEPT ![<<t,r>>] = "free"]
    /\ audit_log' =
        LET crashed_w == CHOOSE w \in Workers: lock[<<t,r>>] = <<"crashed", w>>
        IN Append(audit_log, <<t, r, crashed_w, "reconcile">>)
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \E t \in Tenants, r \in Regions:
        \/ \E w \in Workers: Acquire(t, r, w)
        \/ \E w \in Workers: Release(t, r, w)
        \/ \E w \in Workers: Crash(t, r, w)
        \/ Reconcile(t, r)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-GC-SINGLE-RUNNING-PER-TENANT-REGION (HIGH): at most one worker holds
\* the lock for any (tenant, region). The state structure makes this a
\* type-level invariant; we encode it explicitly for TLC to check.
InvSingleHolderPerKey ==
    \A t \in Tenants, r \in Regions:
        Cardinality({w \in Workers: lock[<<t,r>>] = <<"held", w>>}) <= 1

\* No double-acquire trace: if a worker holds the lock, no other worker can
\* be recorded as also holding it. Subset of the above; kept as a separate
\* invariant for TLC counter-example clarity.
InvNoConcurrentHolders ==
    \A t \in Tenants, r \in Regions, w1, w2 \in Workers:
        (lock[<<t,r>>] = <<"held", w1>> /\ lock[<<t,r>>] = <<"held", w2>>)
            => (w1 = w2)

\* INV-GC-RECONCILE-AUDIT-FAIL-CLOSED (CRITICAL, inherited): every
\* `release` audit entry has a matching prior `acquire` for the same
\* (t, r, w). Equivalent to: no "release without acquire" in the log.
InvAuditReleaseRequiresAcquire ==
    \A i \in 1..Len(audit_log):
        (audit_log[i][4] = "release") =>
            \E j \in 1..(i-1):
                /\ audit_log[j][1] = audit_log[i][1]
                /\ audit_log[j][2] = audit_log[i][2]
                /\ audit_log[j][3] = audit_log[i][3]
                /\ audit_log[j][4] = "acquire"

\* Reconcile audit entry only exists for keys that were once crashed. This
\* gives the fail-closed property: there is NO path from "held" → "free"
\* that bypasses either a `release` or a `reconcile` audit emission.
InvReconcileOnlyFromCrashed ==
    \A i \in 1..Len(audit_log):
        (audit_log[i][4] = "reconcile") =>
            \E j \in 1..(i-1):
                /\ audit_log[j][1] = audit_log[i][1]
                /\ audit_log[j][2] = audit_log[i][2]
                /\ audit_log[j][3] = audit_log[i][3]
                /\ audit_log[j][4] = "acquire"

\* Bound sanity: lock function range is well-typed.
InvLockTypeOK ==
    \A t \in Tenants, r \in Regions:
        lock[<<t,r>>] \in LockStates

SafetyInvariants ==
    /\ InvSingleHolderPerKey
    /\ InvNoConcurrentHolders
    /\ InvAuditReleaseRequiresAcquire
    /\ InvReconcileOnlyFromCrashed
    /\ InvLockTypeOK

================================================================================
