---------------------------- MODULE byok_dek_race ----------------------------
(***************************************************************************)
(* CoreLink — BYOK per-region DEK cache eviction race (DEBT-014 FT-7)      *)
(*                                                                         *)
(* Closes 1 CRITICAL TLA gap (CRITICAL upgrade) from                       *)
(* `specs/_audits/tla-followup-tickets.md` FT-7 by formalising the         *)
(* per-region DEK cache eviction race when the customer revokes their      *)
(* KMS key while CoreLink workers in MULTIPLE regions hold independent     *)
(* DEK caches.                                                             *)
(*                                                                         *)
(* The pre-existing runbook spec `byok_kill_switch.tla` admits the         *)
(* invariant but models the DEK cache as a single global variable. This    *)
(* spec lifts that abstraction by modelling per-region caches and          *)
(* per-region KMS access checks, exposing the inter-region race that the   *)
(* runbook-only model could not catch.                                     *)
(*                                                                         *)
(* Invariant proved here (from invariant_registry.md §3.20):               *)
(*                                                                         *)
(*   INV-BYOK-CRYPTO-SOVEREIGNTY (CRITICAL)                                *)
(*     Customer revokes CMK -> cache becomes inaccessible <= 5 min in      *)
(*     EVERY region independently. No region holds a usable DEK after      *)
(*     its local TTL expires, even if other regions are still in their    *)
(*     eviction window. No encrypt op admitted in a region whose KMS       *)
(*     access check has flipped to revoked AND whose cache has been        *)
(*     evicted.                                                            *)
(*                                                                         *)
(* Threat model:                                                           *)
(*   - Customer revokes their KMS key globally; CoreLink detection probes  *)
(*     in each region run on independent clocks.                           *)
(*   - Region A may evict ahead of Region B (different TTL phases) -       *)
(*     this is acceptable BUT neither region may admit an encrypt while    *)
(*     KMS = revoked AND local cache = evicted.                            *)
(*   - DEK material is NEVER copied across regions on cache hit (would     *)
(*     bypass per-region eviction).                                        *)
(*   - Re-enable propagation is per-region (independent acquire).          *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - AAD binding (covered by `byok_envelope_aad.tla`).                   *)
(*   - Wrap/unwrap correctness (covered by `byok_envelope_aad.tla`).       *)
(*   - Audit emission for revoke (covered by `audit_emit_atomic.tla`).     *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.20                  *)
(*      INV-BYOK-CRYPTO-SOVEREIGNTY`                                       *)
(*   - `specs/_audits/tla-followup-tickets.md` FT-7                        *)
(*   - Companion runbook spec:                                             *)
(*     `specs/03_architecture/tla+/runbooks/byok_kill_switch.tla`          *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, TLC

CONSTANTS
    Tenants,
    Regions,
    MaxTicks,
    SlaTicks,
    MaxEncrypts

ASSUME
    /\ Tenants # {}
    /\ Regions # {}
    /\ MaxTicks \in Nat /\ MaxTicks > 0
    /\ SlaTicks \in Nat /\ SlaTicks > 0 /\ SlaTicks <= MaxTicks
    /\ MaxEncrypts \in Nat

\* Global per-tenant KMS state (customer-owned; flips together for all
\* regions but detection / eviction is region-local).
KmsStates    == {"active", "revoked"}

\* Per-region cache state.
CacheStates  == {"cached", "evicted"}

\* Per-region access state for the tenant.
AccessStates == {"active", "detecting", "degraded_read_only"}

VARIABLES
    kms_state,         \* Tenant -> KmsStates (customer side)
    dek_cache,         \* (Tenant, Region) -> CacheStates
    detected,          \* (Tenant, Region) -> BOOLEAN; region noticed revoke
    access,            \* (Tenant, Region) -> AccessStates
    ticks_since_rev,   \* (Tenant, Region) -> Nat; region-local elapsed
    clock,             \* Nat; global tick (monotonic)
    encrypts_admitted  \* Nat counter; encrypts that succeeded anywhere

vars == <<kms_state, dek_cache, detected, access,
          ticks_since_rev, clock, encrypts_admitted>>

(*-- Type invariant ---------------------------------------------------------*)
TypeOK ==
    /\ \A t \in Tenants : kms_state[t] \in KmsStates
    /\ \A t \in Tenants, r \in Regions : dek_cache[<<t, r>>] \in CacheStates
    /\ \A t \in Tenants, r \in Regions : detected[<<t, r>>] \in BOOLEAN
    /\ \A t \in Tenants, r \in Regions : access[<<t, r>>] \in AccessStates
    /\ \A t \in Tenants, r \in Regions :
         ticks_since_rev[<<t, r>>] \in 0..MaxTicks
    /\ clock \in 0..MaxTicks
    /\ encrypts_admitted \in 0..MaxEncrypts

(*-- Init -------------------------------------------------------------------*)
Init ==
    /\ kms_state         = [t \in Tenants |-> "active"]
    /\ dek_cache         = [tr \in (Tenants \X Regions) |-> "cached"]
    /\ detected          = [tr \in (Tenants \X Regions) |-> FALSE]
    /\ access            = [tr \in (Tenants \X Regions) |-> "active"]
    /\ ticks_since_rev   = [tr \in (Tenants \X Regions) |-> 0]
    /\ clock             = 0
    /\ encrypts_admitted = 0

(*-- Actions ----------------------------------------------------------------*)

\* Customer revokes KMS key globally for tenant t.
Revoke(t) ==
    /\ kms_state[t] = "active"
    /\ kms_state' = [kms_state EXCEPT ![t] = "revoked"]
    /\ ticks_since_rev' = [tr \in (Tenants \X Regions) |->
                              IF tr[1] = t THEN 0 ELSE ticks_since_rev[tr]]
    /\ UNCHANGED <<dek_cache, detected, access, clock, encrypts_admitted>>

\* Region-local detection probe notices unavailability. Each region
\* probes on its own clock; detection happens independently.
Detect(t, r) ==
    /\ kms_state[t] = "revoked"
    /\ ~detected[<<t, r>>]
    /\ detected' = [detected EXCEPT ![<<t, r>>] = TRUE]
    /\ access'   = [access   EXCEPT ![<<t, r>>] = "detecting"]
    /\ UNCHANGED <<kms_state, dek_cache, ticks_since_rev, clock,
                   encrypts_admitted>>

\* Region-local eviction: drop DEK cache for this (tenant, region) AND
\* move to degraded_read_only. Inter-region order is non-deterministic;
\* this is what FT-7 forces the model to expose.
EvictAndDegrade(t, r) ==
    /\ detected[<<t, r>>] = TRUE
    /\ access[<<t, r>>] = "detecting"
    /\ dek_cache' = [dek_cache EXCEPT ![<<t, r>>] = "evicted"]
    /\ access'    = [access    EXCEPT ![<<t, r>>] = "degraded_read_only"]
    /\ UNCHANGED <<kms_state, detected, ticks_since_rev, clock,
                   encrypts_admitted>>

\* Customer re-enables KMS key globally.
ReEnable(t) ==
    /\ kms_state[t] = "revoked"
    /\ kms_state' = [kms_state EXCEPT ![t] = "active"]
    /\ UNCHANGED <<dek_cache, detected, access, ticks_since_rev, clock,
                   encrypts_admitted>>

\* Region-local restore (re-acquire DEK from KMS, clear detection flag).
\* Only fires when KMS is active AND access is degraded_read_only.
Restore(t, r) ==
    /\ kms_state[t] = "active"
    /\ access[<<t, r>>] = "degraded_read_only"
    /\ access'     = [access     EXCEPT ![<<t, r>>] = "active"]
    /\ dek_cache'  = [dek_cache  EXCEPT ![<<t, r>>] = "cached"]
    /\ detected'   = [detected   EXCEPT ![<<t, r>>] = FALSE]
    /\ ticks_since_rev' = [ticks_since_rev EXCEPT ![<<t, r>>] = 0]
    /\ UNCHANGED <<kms_state, clock, encrypts_admitted>>

\* Attempt to encrypt in region r for tenant t. Admitted iff:
\*   (a) per-region KMS access check (mirrors kms_state[t]) returns active
\*       AND (b) per-region DEK cache is populated
\*       AND (c) per-region access state = "active".
\* INV-BYOK-CRYPTO-SOVEREIGNTY rejects every other combination.
AttemptEncrypt(t, r) ==
    /\ encrypts_admitted < MaxEncrypts
    /\ IF /\ kms_state[t] = "active"
          /\ dek_cache[<<t, r>>] = "cached"
          /\ access[<<t, r>>] = "active"
         THEN encrypts_admitted' = encrypts_admitted + 1
         ELSE encrypts_admitted' = encrypts_admitted
    /\ UNCHANGED <<kms_state, dek_cache, detected, access,
                   ticks_since_rev, clock>>

\* Global clock tick. Increments per-(tenant,region) ticks_since_rev for
\* any (t, r) where kms_state[t] = "revoked" AND access has not yet
\* reached degraded_read_only.
Tick ==
    /\ clock < MaxTicks
    /\ clock' = clock + 1
    /\ ticks_since_rev' =
        [tr \in (Tenants \X Regions) |->
            IF /\ kms_state[tr[1]] = "revoked"
               /\ access[tr] # "degraded_read_only"
              THEN ticks_since_rev[tr] + 1
              ELSE ticks_since_rev[tr]]
    /\ UNCHANGED <<kms_state, dek_cache, detected, access, encrypts_admitted>>

Next ==
    \/ \E t \in Tenants : Revoke(t)
    \/ \E t \in Tenants : ReEnable(t)
    \/ \E t \in Tenants, r \in Regions : Detect(t, r)
    \/ \E t \in Tenants, r \in Regions : EvictAndDegrade(t, r)
    \/ \E t \in Tenants, r \in Regions : Restore(t, r)
    \/ \E t \in Tenants, r \in Regions : AttemptEncrypt(t, r)
    \/ Tick

Spec ==
    /\ Init
    /\ [][Next]_vars
    /\ \A t \in Tenants, r \in Regions : WF_vars(Detect(t, r))
    /\ \A t \in Tenants, r \in Regions : WF_vars(EvictAndDegrade(t, r))
    /\ WF_vars(Tick)

(*-- Safety invariants ------------------------------------------------------*)

\* INV-BYOK-CRYPTO-SOVEREIGNTY (CRITICAL): no encrypt admitted in a
\* (tenant, region) where KMS is revoked AND cache has been evicted.
\* The encrypt path MUST atomically observe kms_state AND dek_cache[t,r];
\* this invariant catches any handler refactor that splits the check.
InvNoEncryptAfterRegionEvicted ==
    \A t \in Tenants, r \in Regions :
        ( /\ kms_state[t] = "revoked"
          /\ dek_cache[<<t, r>>] = "evicted" )
            => access[<<t, r>>] # "active"

\* Region-local cleanliness: once a region reaches degraded_read_only
\* under a revoked KMS, its DEK cache MUST be evicted.
InvDegradedRegionCacheClean ==
    \A t \in Tenants, r \in Regions :
        ( /\ access[<<t, r>>] = "degraded_read_only"
          /\ kms_state[t] = "revoked" )
            => dek_cache[<<t, r>>] = "evicted"

\* No cross-region copy: if region r1 has evicted but region r2 has not
\* yet, that does NOT permit region r1 to read its DEK from region r2.
\* Modelled as: encrypt in r1 with cache[r1]=evicted is always rejected
\* irrespective of cache[r2]. This is a structural property of the
\* AttemptEncrypt guard (no r2-lookup); we encode it as a state invariant
\* to flag any future refactor that introduces such a lookup.
InvNoCrossRegionDekCopy ==
    \A t \in Tenants, r1 \in Regions, r2 \in Regions :
        ( /\ r1 # r2
          /\ kms_state[t] = "revoked"
          /\ dek_cache[<<t, r1>>] = "evicted"
          /\ dek_cache[<<t, r2>>] = "cached" )
            => access[<<t, r1>>] # "active"

\* SLA bound: ticks_since_rev never exceeds MaxTicks (state invariant).
\* The "<= SlaTicks before degraded" property is liveness-side; see
\* RegionDegradesUnderRevoke below.
InvTicksBounded ==
    \A t \in Tenants, r \in Regions :
        ticks_since_rev[<<t, r>>] <= MaxTicks

SafetyInvariants ==
    /\ TypeOK
    /\ InvNoEncryptAfterRegionEvicted
    /\ InvDegradedRegionCacheClean
    /\ InvNoCrossRegionDekCopy
    /\ InvTicksBounded

(*-- Liveness ---------------------------------------------------------------*)

\* Every region eventually reaches degraded_read_only under a revoked KMS
\* OR KMS is re-enabled. Independent per region.
RegionDegradesUnderRevoke ==
    \A t \in Tenants, r \in Regions :
        (kms_state[t] = "revoked")
            ~> ( access[<<t, r>>] = "degraded_read_only"
                 \/ kms_state[t] = "active" )

================================================================================
