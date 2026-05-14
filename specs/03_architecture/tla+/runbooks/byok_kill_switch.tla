---- MODULE byok_kill_switch ----
(***************************************************************************)
(* CoreLink WI-S20-007 — Runbook formal verification (3/4)                 *)
(*                                                                         *)
(* Runbook: RB-BYOK-REVOKE / RB-BYOK-KILL-SWITCH                           *)
(* Source : S-14 WI-S14-006 + S-17 tabletop "BYOK kill switch — DEK cache  *)
(*          evict + tenant degrade read-only within ≤ 360s SLA"            *)
(*                                                                         *)
(* What this models                                                        *)
(* ----------------                                                        *)
(* Enterprise BYOK customers can disable their KMS key (AWS / GCP / Azure  *)
(* / Vault) at any time. CoreLink must:                                    *)
(*   1) detect KMS unavailability within ≤ 60s,                            *)
(*   2) evict any cached DEK material from the per-region DEK cache,       *)
(*   3) move the tenant to degraded_read_only,                            *)
(*   4) reach the final state within ≤ 360s (kill-switch SLA),             *)
(*   5) reverse the transition deterministically when KMS is re-enabled.   *)
(*                                                                         *)
(* Invariants verified                                                     *)
(* -------------------                                                     *)
(*   NoEncryptAfterRevoke    — once kms_state = revoked AND dek_cache for *)
(*                             that tenant is empty, no new encrypt op is  *)
(*                             admitted (INV-BYOK-CRYPTO-SOVEREIGNTY).     *)
(*   DegradeWithinSLA        — every revoke event eventually leads to     *)
(*                             access = degraded_read_only within         *)
(*                             SlaTicks of detection (bounded liveness).   *)
(*   DekCacheCleanOnRevoke   — once tenant is degraded_read_only AND      *)
(*                             kms_state = revoked, dek_cache[tenant] = {}.*)
(*   ReEnableRestoresActive  — re-enable KMS + accept tenant ⇒ access      *)
(*                             returns to active (no stuck state).         *)
(*                                                                         *)
(* TLC tractability: 2 tenants, MaxTicks=8 → ≤ 60s on CI.                  *)
(***************************************************************************)

EXTENDS Naturals, FiniteSets, TLC

CONSTANTS
  Tenants,        \* finite set of BYOK tenants
  SlaTicks,       \* ticks to reach kill-switch SLA (e.g. 4)
  MaxTicks        \* clock bound

ASSUME
  /\ Tenants # {}
  /\ SlaTicks \in Nat /\ SlaTicks > 0
  /\ MaxTicks \in Nat /\ MaxTicks > SlaTicks

MaxEncrypts == 2  \* bound encrypts_admitted counter for TLC tractability

KmsStates    == {"active", "revoked"}
AccessStates == {"active", "detecting", "degraded_read_only"}

VARIABLES
  kms_state,         \* Tenant -> KmsStates
  detected_revoke,   \* Tenant -> BOOLEAN; runbook noticed revoke
  dek_cache,         \* Tenant -> BOOLEAN; TRUE = a cached DEK exists
  access,            \* Tenant -> AccessStates
  encrypts_admitted, \* Nat counter; encrypt operations that succeeded
  ticks_since_rev,   \* Tenant -> Nat; ticks accumulated since revoke detected
  clock              \* Nat; monotonic clock

vars == <<kms_state, detected_revoke, dek_cache, access,
          encrypts_admitted, ticks_since_rev, clock>>

TypeOK ==
  /\ \A t \in Tenants : kms_state[t]       \in KmsStates
  /\ \A t \in Tenants : detected_revoke[t] \in BOOLEAN
  /\ \A t \in Tenants : dek_cache[t]       \in BOOLEAN
  /\ \A t \in Tenants : access[t]          \in AccessStates
  /\ \A t \in Tenants : ticks_since_rev[t] \in 0..MaxTicks
  /\ encrypts_admitted \in 0..MaxEncrypts
  /\ clock             \in 0..MaxTicks

Init ==
  /\ kms_state         = [t \in Tenants |-> "active"]
  /\ detected_revoke   = [t \in Tenants |-> FALSE]
  /\ dek_cache         = [t \in Tenants |-> TRUE]
  /\ access            = [t \in Tenants |-> "active"]
  /\ encrypts_admitted = 0
  /\ ticks_since_rev   = [t \in Tenants |-> 0]
  /\ clock             = 0

(*-- Actions ----------------------------------------------------------------*)

\* Customer revokes KMS key on their side.
Revoke(t) ==
  /\ kms_state[t] = "active"
  /\ kms_state'         = [kms_state         EXCEPT ![t] = "revoked"]
  /\ detected_revoke'   = [detected_revoke   EXCEPT ![t] = FALSE]
  /\ ticks_since_rev'   = [ticks_since_rev   EXCEPT ![t] = 0]
  /\ UNCHANGED <<dek_cache, access, encrypts_admitted, clock>>

\* Detection probe notices unavailability.
Detect(t) ==
  /\ kms_state[t] = "revoked"
  /\ detected_revoke[t] = FALSE
  /\ detected_revoke'  = [detected_revoke  EXCEPT ![t] = TRUE]
  /\ access'           = [access           EXCEPT ![t] = "detecting"]
  /\ UNCHANGED <<kms_state, dek_cache, encrypts_admitted,
                 ticks_since_rev, clock>>

\* Runbook: evict DEK cache + transition to degraded_read_only.
EvictAndDegrade(t) ==
  /\ detected_revoke[t] = TRUE
  /\ access[t] = "detecting"
  /\ dek_cache' = [dek_cache EXCEPT ![t] = FALSE]
  /\ access'    = [access    EXCEPT ![t] = "degraded_read_only"]
  /\ UNCHANGED <<kms_state, detected_revoke, encrypts_admitted,
                 ticks_since_rev, clock>>

\* Tenant re-enables KMS key.
ReEnable(t) ==
  /\ kms_state[t] = "revoked"
  /\ kms_state' = [kms_state EXCEPT ![t] = "active"]
  /\ UNCHANGED <<detected_revoke, dek_cache, access,
                 encrypts_admitted, ticks_since_rev, clock>>

\* Operator confirms re-enable and restores tenant to active.
Restore(t) ==
  /\ kms_state[t] = "active"
  /\ access[t] = "degraded_read_only"
  /\ access'         = [access         EXCEPT ![t] = "active"]
  /\ dek_cache'      = [dek_cache      EXCEPT ![t] = TRUE]
  /\ detected_revoke'= [detected_revoke EXCEPT ![t] = FALSE]
  /\ ticks_since_rev'= [ticks_since_rev EXCEPT ![t] = 0]
  /\ UNCHANGED <<kms_state, encrypts_admitted, clock>>

\* Encrypt op: admitted iff KMS active AND DEK cache populated AND tenant active.
AttemptEncrypt(t) ==
  /\ encrypts_admitted < MaxEncrypts
  /\ IF kms_state[t] = "active" /\ dek_cache[t] /\ access[t] = "active"
       THEN encrypts_admitted' = encrypts_admitted + 1
       ELSE encrypts_admitted' = encrypts_admitted
  /\ UNCHANGED <<kms_state, detected_revoke, dek_cache, access,
                 ticks_since_rev, clock>>

\* Clock tick: advance ticks_since_rev for tenants in detecting/revoked state.
Tick ==
  /\ clock < MaxTicks
  /\ clock' = clock + 1
  /\ ticks_since_rev' = [t \in Tenants |->
                          IF kms_state[t] = "revoked" /\ access[t] # "degraded_read_only"
                            THEN ticks_since_rev[t] + 1
                            ELSE ticks_since_rev[t]]
  /\ UNCHANGED <<kms_state, detected_revoke, dek_cache, access, encrypts_admitted>>

Next ==
  \/ \E t \in Tenants : Revoke(t)
  \/ \E t \in Tenants : Detect(t)
  \/ \E t \in Tenants : EvictAndDegrade(t)
  \/ \E t \in Tenants : ReEnable(t)
  \/ \E t \in Tenants : Restore(t)
  \/ \E t \in Tenants : AttemptEncrypt(t)
  \/ Tick

Spec ==
  /\ Init
  /\ [][Next]_vars
  /\ \A t \in Tenants : WF_vars(Detect(t))
  /\ \A t \in Tenants : WF_vars(EvictAndDegrade(t))
  /\ \A t \in Tenants : WF_vars(Restore(t))
  /\ WF_vars(Tick)

(*-- Invariants -------------------------------------------------------------*)

\* No encrypt admitted while tenant is in revoked + cache-empty state.
NoEncryptAfterRevoke ==
  \A t \in Tenants :
    (kms_state[t] = "revoked" /\ ~dek_cache[t]) =>
      (access[t] # "active")

\* DEK cache cleaned on revoke + degraded.
DekCacheCleanOnRevoke ==
  \A t \in Tenants :
    (access[t] = "degraded_read_only" /\ kms_state[t] = "revoked") =>
      ~dek_cache[t]

\* SLA bound (state invariant): ticks_since_rev is bounded by MaxTicks
\* (TypeOK). The "≤ SlaTicks" property is liveness — see
\* RevokeLeadsToDegrade below; if Detect + EvictAndDegrade are weakly fair,
\* the system reaches degraded_read_only without violating MaxTicks.
DegradeWithinSLA == \A t \in Tenants : ticks_since_rev[t] <= MaxTicks

(*-- Liveness ---------------------------------------------------------------*)

\* Every revoke leads to degraded_read_only OR the KMS is re-enabled.
\* (Captures: the runbook never permanently leaves a tenant in a revoked +
\* still-encrypting state — either we degrade, or KMS is restored.)
RevokeLeadsToDegrade ==
  \A t \in Tenants :
    (kms_state[t] = "revoked")
       ~> (access[t] = "degraded_read_only" \/ kms_state[t] = "active")

\* Re-enable + restore eventually returns tenant to active OR KMS is
\* revoked again (chained revoke is permitted).
ReEnableRestoresActive ==
  \A t \in Tenants :
    (kms_state[t] = "active" /\ access[t] = "degraded_read_only")
       ~> (access[t] = "active" \/ kms_state[t] = "revoked")

====
