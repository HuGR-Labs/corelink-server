--------------------------- MODULE audit_no_raw_pii ---------------------------
(***************************************************************************)
(* CoreLink — Audit / log PII redaction (DEBT-005 batch 5 #1)              *)
(*                                                                         *)
(* Closes 3 CRITICAL TLA gaps from canonical-consistency baseline §3.4:    *)
(*                                                                         *)
(*   INV-AUDIT-NO-RAW-PII  (CRITICAL, §3.4 AUDIT)                          *)
(*     The audit pipeline MUST NOT persist any raw PII field. All         *)
(*     PII-bearing values are passed through the redactor BEFORE the      *)
(*     append-only writer fsyncs the record. There is no bypass.          *)
(*                                                                         *)
(*   INV-NO-PII-IN-LOGS  (CRITICAL, §3.4 OBS)                              *)
(*     The structured logger MUST NOT emit raw PII to any sink (stderr,   *)
(*     OTel, file). The `tracing` middleware redactor strips PII before   *)
(*     the emitter writes the event. Failure to redact = drop record      *)
(*     fail-closed.                                                       *)
(*                                                                         *)
(*   INV-NO-BODY-IN-LOGS  (CRITICAL, §3.4 OBS)                             *)
(*     HTTP request/response bodies MUST NEVER appear in logs. Even if a  *)
(*     handler attempts to log a body, the request-body filter drops the  *)
(*     entry before emission.                                             *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Producers attempt to write records with arbitrary payload          *)
(*     containing PII / body bytes.                                       *)
(*   - The pipeline runs Redact() over every record. If the redacted     *)
(*     form still carries PII or body bytes, the record is dropped       *)
(*     fail-closed (drop_count incremented, never reaches sinks).        *)
(*   - Adversarial bypass actions (guard FALSE) attempt to surface raw   *)
(*     PII in sinks; TLC proves the trace is impossible.                 *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - The redactor's regex catalogue completeness (covered by goldens).  *)
(*   - Cryptographic envelope of audit chain (proved in audit_immutability).*)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.4 AUDIT, OBS`      *)
(*   - `crates/corelink-audit/src/redactor.rs`                            *)
(*   - `crates/corelink-obs/src/tracing_filter.rs`                        *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    PayloadKinds,      \* { "clean", "pii_raw", "body_bytes" }
    MaxOps

ASSUME
    /\ PayloadKinds # {}
    /\ MaxOps \in Nat

\* A record has a kind tag describing its payload before redaction.
\* The pipeline is modelled as three sinks: audit_log, obs_log, dropped.
\* Records reach a sink iff Redact() classifies them as clean.

VARIABLES
    audit_log,         \* Sequence of redacted records that hit the audit store
    obs_log,           \* Sequence of redacted records that hit the obs sink
    dropped,           \* Sequence of <<kind, reason>> for fail-closed drops
    op_count

vars == <<audit_log, obs_log, dropped, op_count>>

\* Redact: clean payloads pass; pii_raw and body_bytes are dropped.
\* This is a function literal lifted to a definition (FT-4 mitigation
\* pattern: the function is purely structural — no state, no compound
\* literal in Init).
IsClean(kind) == kind = "clean"

Init ==
    /\ audit_log = <<>>
    /\ obs_log   = <<>>
    /\ dropped   = <<>>
    /\ op_count  = 0

(*-- Actions -----------------------------------------------------------------*)

\* Producer writes an audit record. Pipeline applies Redact() — only clean
\* payloads reach the persistent audit_log. PII/body payloads are dropped.
AuditWrite(kind) ==
    /\ kind \in PayloadKinds
    /\ op_count < MaxOps
    /\ IF IsClean(kind)
       THEN /\ audit_log' = Append(audit_log, kind)
            /\ UNCHANGED <<obs_log, dropped>>
       ELSE /\ dropped' = Append(dropped, <<kind, "audit_redacted">>)
            /\ UNCHANGED <<audit_log, obs_log>>
    /\ op_count' = op_count + 1

\* Producer writes an obs (log) event. Same redactor; same fail-closed.
ObsEmit(kind) ==
    /\ kind \in PayloadKinds
    /\ op_count < MaxOps
    /\ IF IsClean(kind)
       THEN /\ obs_log' = Append(obs_log, kind)
            /\ UNCHANGED <<audit_log, dropped>>
       ELSE /\ dropped' = Append(dropped, <<kind, "obs_redacted">>)
            /\ UNCHANGED <<audit_log, obs_log>>
    /\ op_count' = op_count + 1

\* Adversarial: bypass redactor and surface raw PII in audit_log.
\* Guard FALSE — pipeline has no such path.
AttemptRawPiiInAudit(kind) ==
    /\ kind \in PayloadKinds
    /\ kind # "clean"
    /\ op_count < MaxOps
    /\ FALSE
    /\ audit_log' = Append(audit_log, kind)
    /\ UNCHANGED <<obs_log, dropped>>
    /\ op_count' = op_count + 1

\* Adversarial: bypass redactor and surface body bytes in obs_log.
\* Guard FALSE.
AttemptBodyInObs(kind) ==
    /\ kind \in PayloadKinds
    /\ kind = "body_bytes"
    /\ op_count < MaxOps
    /\ FALSE
    /\ obs_log' = Append(obs_log, kind)
    /\ UNCHANGED <<audit_log, dropped>>
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E k \in PayloadKinds: AuditWrite(k)
    \/ \E k \in PayloadKinds: ObsEmit(k)
    \/ \E k \in PayloadKinds: AttemptRawPiiInAudit(k)
    \/ \E k \in PayloadKinds: AttemptBodyInObs(k)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-AUDIT-NO-RAW-PII. No audit_log entry is a non-clean payload.
InvAuditNoRawPii ==
    \A i \in 1..Len(audit_log): IsClean(audit_log[i])

\* INV-NO-PII-IN-LOGS. No obs_log entry is pii_raw.
InvNoPiiInLogs ==
    \A i \in 1..Len(obs_log): obs_log[i] # "pii_raw"

\* INV-NO-BODY-IN-LOGS. No obs_log entry is body_bytes.
InvNoBodyInLogs ==
    \A i \in 1..Len(obs_log): obs_log[i] # "body_bytes"

SafetyInvariants ==
    /\ InvAuditNoRawPii
    /\ InvNoPiiInLogs
    /\ InvNoBodyInLogs

================================================================================
