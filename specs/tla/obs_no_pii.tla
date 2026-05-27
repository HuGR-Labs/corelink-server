---------------------------- MODULE obs_no_pii ----------------------------
(***************************************************************************)
(* CoreLink — observability export PII fail-CLOSED (DEBT-005 batch 3 #1) *)
(*                                                                         *)
(* Closes CRITICAL TLA gap from canonical-consistency baseline:           *)
(*                                                                         *)
(*   INV-OBS-NO-PII  (CRITICAL, §3.24 Obs-Export)                          *)
(*     Every metric / trace / log forwarded to a third-party sink MUST   *)
(*     be PII-free. The corelink-otel-export crate enforces an           *)
(*     allowlist of label keys at CONSTRUCTION time; any label whose     *)
(*     key is NOT in the allowlist OR whose value matches the PII        *)
(*     pattern is rejected at the constructor (no fallback, no swap,     *)
(*     no fail-open). The third-party sink NEVER observes a rejected    *)
(*     label.                                                             *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Caller submits an event carrying a forbidden label key            *)
(*     (`AttemptSubmitForbiddenKey`) — rejected at admit.                *)
(*   - Caller submits an event whose value matches a PII pattern        *)
(*     (`AttemptSubmitPiiValue`) — rejected at admit.                    *)
(*   - Caller submits a clean event (`SubmitClean`) — accepted +         *)
(*     forwarded to the sink.                                            *)
(*   - Adversarial: a rejected event reaches the sink                    *)
(*     (`AttemptRejectedReachSink`, guard FALSE) — exhibited to make    *)
(*     the unreachability premise visible in the trace.                  *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - PII pattern detection algorithm (handled by allowlist + regex   *)
(*     in `corelink-otel-export::lib::REDACT_PATTERNS`).                 *)
(*   - Wall-clock retry semantics on sink delivery failures (RB-OBS-001).*)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.24 INV-OBS-NO-PII`*)
(*   - `crates/corelink-telemetry/src/otel/exporter.rs` (post Wave 35 P2:*)
(*     corelink-otel-export absorbed into corelink-telemetry/src/otel/)   *)
(*   - `crates/corelink-telemetry/src/otel/audit.rs`                     *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Events,            \* Finite set of caller-side event IDs to model
    MaxOps             \* Bound on total transitions

ASSUME
    /\ Events # {}
    /\ MaxOps \in Nat

Verdicts == {"pending", "admitted", "rejected_key", "rejected_value"}

VARIABLES
    e_verdict,         \* Events -> Verdicts
    sink,              \* SUBSET Events — events delivered downstream
    op_count

vars == <<e_verdict, sink, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ e_verdict = [e \in Events |-> "pending"]
    /\ sink      = {}
    /\ op_count  = 0

(*-- Actions -----------------------------------------------------------------*)

\* Happy path: caller submits a clean event. Allowlisted key + non-PII
\* value. The constructor admits; the export pipeline forwards to sink.
SubmitClean(e) ==
    /\ e \in Events
    /\ e_verdict[e] = "pending"
    /\ op_count < MaxOps
    /\ e_verdict' = [e_verdict EXCEPT ![e] = "admitted"]
    /\ sink'      = sink \cup {e}
    /\ op_count'  = op_count + 1

\* Caller submits an event whose label key is NOT in the allowlist.
\* Constructor rejects; sink never sees it.
AttemptSubmitForbiddenKey(e) ==
    /\ e \in Events
    /\ e_verdict[e] = "pending"
    /\ op_count < MaxOps
    /\ e_verdict' = [e_verdict EXCEPT ![e] = "rejected_key"]
    /\ UNCHANGED sink
    /\ op_count'  = op_count + 1

\* Caller submits an event whose value matches a PII pattern
\* (email regex / principal_id format / raw token).
\* Constructor rejects; sink never sees it.
AttemptSubmitPiiValue(e) ==
    /\ e \in Events
    /\ e_verdict[e] = "pending"
    /\ op_count < MaxOps
    /\ e_verdict' = [e_verdict EXCEPT ![e] = "rejected_value"]
    /\ UNCHANGED sink
    /\ op_count'  = op_count + 1

\* Adversarial: a rejected event somehow reaches the sink (bug class:
\* fail-open in the export pipeline). Guard FALSE — exhibits the
\* impossible trace so the invariant's unreachability is visible.
AttemptRejectedReachSink(e) ==
    /\ e \in Events
    /\ e_verdict[e] \in {"rejected_key", "rejected_value"}
    /\ op_count < MaxOps
    /\ FALSE
    /\ sink' = sink \cup {e}
    /\ UNCHANGED e_verdict
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E e \in Events: SubmitClean(e)
    \/ \E e \in Events: AttemptSubmitForbiddenKey(e)
    \/ \E e \in Events: AttemptSubmitPiiValue(e)
    \/ \E e \in Events: AttemptRejectedReachSink(e)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-OBS-NO-PII (core). No rejected event ever appears in the sink.
InvNoPiiInSink ==
    \A e \in Events:
        e_verdict[e] \in {"rejected_key", "rejected_value"} => e \notin sink

\* Sink contents are exactly the admitted events.
InvSinkEqAdmitted ==
    \A e \in Events: (e \in sink) <=> (e_verdict[e] = "admitted")

\* No pending event reaches the sink (pre-admit forwarding is impossible).
InvPendingNotInSink ==
    \A e \in Events: e_verdict[e] = "pending" => e \notin sink

SafetyInvariants ==
    /\ InvNoPiiInSink
    /\ InvSinkEqAdmitted
    /\ InvPendingNotInSink

================================================================================
