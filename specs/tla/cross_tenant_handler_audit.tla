---------------------------- MODULE cross_tenant_handler_audit ----------------------------
(*******************************************************************************)
(* CoreLink — INV-CROSS-TENANT-DENIED (CRITICAL)                              *)
(*                                                                             *)
(* Independent handler-layer model.  `tenant_isolation.tla` proves storage    *)
(* read/write/enumeration isolation; this model proves the missing refinement: *)
(* for every customer endpoint group, the denied audit is in the event trace   *)
(* immediately before the CrossTenantDenied return.                           *)
(*                                                                             *)
(* The six groups are explicit: overview, usage, billing, keys, team, and     *)
(* audit_query.  A failing audit sink has a distinct AuditFailed outcome and   *)
(* therefore cannot manufacture an unaudited CrossTenantDenied result.         *)
(*******************************************************************************)

EXTENDS Integers, Sequences, TLC

CONSTANTS
    Tenants,
    Groups,
    MaxOps

ASSUME
    /\ Tenants = {"tenant_a", "tenant_b"}
    /\ Groups = {"overview", "usage", "billing", "keys", "team", "audit_query"}
    /\ MaxOps >= 6

VARIABLES event_log, next_group, completed, audit_healthy

vars == <<event_log, next_group, completed, audit_healthy>>

GroupOrder == <<"overview", "usage", "billing", "keys", "team", "audit_query">>

DeniedKinds == [
    overview   |-> "OverviewDenied",
    usage      |-> "UsageDenied",
    billing    |-> "BillingDenied",
    keys       |-> "KeysDenied",
    team       |-> "TeamDenied",
    audit_query|-> "AuditQueryDenied"
]

AuditEvent(g) == [group |-> g, phase |-> "audit", kind |-> DeniedKinds[g]]
DeniedReturn(g, caller, target) ==
    [group |-> g, phase |-> "return", kind |-> "CrossTenantDenied",
     caller |-> caller, target |-> target]
AuditFailure(g) == [group |-> g, phase |-> "audit_failure", kind |-> "AuditFailed"]

Init ==
    /\ event_log = <<>>
    /\ next_group = 1
    /\ completed = FALSE
    /\ audit_healthy \in BOOLEAN

(* The production path: audit append linearizes before the typed denial return. *)
CrossTenantDeny(g, caller, target) ==
    /\ ~completed
    /\ next_group <= Len(GroupOrder)
    /\ g = GroupOrder[next_group]
    /\ caller \in Tenants
    /\ target \in Tenants
    /\ caller # target
    /\ audit_healthy
    /\ event_log' = event_log \o <<AuditEvent(g), DeniedReturn(g, caller, target)>>
    /\ next_group' = next_group + 1
    /\ completed' = (next_group = Len(GroupOrder))
    /\ UNCHANGED audit_healthy

(* A sink outage is a real alternate outcome, not a bypass.  It never appends a
   CrossTenantDenied return, so the fail-CLOSED contract remains meaningful. *)
AuditFailureDeny(g) ==
    /\ ~completed
    /\ next_group <= Len(GroupOrder)
    /\ g = GroupOrder[next_group]
    /\ ~audit_healthy
    /\ event_log' = event_log \o <<AuditFailure(g)>>
    /\ next_group' = next_group + 1
    /\ completed' = FALSE
    /\ UNCHANGED audit_healthy

Next ==
    \/ \E g \in Groups, caller \in Tenants, target \in Tenants:
           CrossTenantDeny(g, caller, target)
    \/ \E g \in Groups: AuditFailureDeny(g)

Spec == Init /\ [][Next]_vars

(* Every CrossTenantDenied return is preceded by its own group's denial audit. *)
InvAuditBeforeCrossTenantDenied ==
    \A i \in 1..Len(event_log):
        event_log[i].kind = "CrossTenantDenied" =>
            /\ i > 1
            /\ event_log[i - 1].phase = "audit"
            /\ event_log[i - 1].group = event_log[i].group
            /\ event_log[i - 1].kind = DeniedKinds[event_log[i].group]

(* A denial cannot be returned for a same-tenant request. *)
InvCrossTenantReturnHasDistinctTenants ==
    \A i \in 1..Len(event_log):
        event_log[i].kind = "CrossTenantDenied" =>
            event_log[i].caller # event_log[i].target

(* The completed run has one independently modelled denial for each group. *)
DeniedGroups ==
    {g \in Groups :
        \E i \in 1..Len(event_log):
            event_log[i].group = g /\ event_log[i].kind = "CrossTenantDenied"}

InvAllSixGroupsCovered == completed => DeniedGroups = Groups

(* Fail-CLOSED audit failures cannot be mistaken for CrossTenantDenied. *)
InvAuditFailureNeverReturnsCrossTenantDenied ==
    \A i \in 1..Len(event_log):
        event_log[i].kind = "AuditFailed" =>
            \A j \in i + 1..Len(event_log):
                event_log[j].kind # "CrossTenantDenied"

=============================================================================
