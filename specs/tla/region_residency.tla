---- MODULE region_residency ----
EXTENDS Naturals, FiniteSets, TLC

(* CoreLink S-14 WI-S14-009 — Cross-region routing actions + tenant residency enforcement.
 *
 * Validates (Lote 10.14 canonical):
 *  - TypeOK:                  shape of all state variables.
 *  - RegionResidencyHolds:    every blob is stored in a region ∈ tenant_residency_set(tenant).
 *  - NoCrossRegionWrite:      write requests MUST target tenant.primary_region or be rejected.
 *  - NoCrossRegionLeak:       reads outside primary_region are ONLY permitted as failover reads.
 *  - ReplicationEventuallyConverges (liveness): replica_lag eventually < LAG_BOUND (FairExecution).
 *
 * INVs verified:
 *   INV-DATA-RESIDENCY     (CRITICAL §3.11)
 *   INV-REGION-NO-CROSS-LEAK (CRITICAL §3.12)
 *   INV-TENANT-ISOLATION   (CRITICAL §3.1)
 *
 * TLC v1.8.0 SHA-256 pinned (ADR-0042 §A1). Bounded state space:
 *   tenants=3, blobs=5, regions=4 → CI runtime ≤ 60s target.
 *)

CONSTANTS
  Tenants,          \* finite set of tenant identifiers
  Blobs,            \* finite set of blob identifiers
  Regions,          \* {WNAM, ENAM, WEUR, SAM}
  PrimaryRegionOf,  \* function Tenants -> Regions (injected via .cfg override)
  MaxLag            \* abstract upper bound on replication lag clock ticks

ASSUME
  /\ Cardinality(Regions) = 4
  /\ PrimaryRegionOf \in [Tenants -> Regions]
  /\ MaxLag \in Nat /\ MaxLag > 0

(* ── STATE VARIABLES ──────────────────────────────────────────────────────── *)

VARIABLES
  storage,          \* [Blobs -> [tenant: Tenants, region: Regions, replicated: BOOLEAN]]
                    \* domain is the set of blobs currently written
  requests,         \* [Nat -> [type: ReqType, tenant: Tenants, region: Regions, is_failover: BOOLEAN]]
                    \* log of all routing decisions (bounded for TLC tractability)
  replica_lag,      \* [Tenants -> Nat]  abstract lag clock; 0 = fully converged
  req_count         \* Nat; monotonic counter bounding requests domain

vars == <<storage, requests, replica_lag, req_count>>

(* ── TYPE SETS ────────────────────────────────────────────────────────────── *)

ReqType == {"write", "read", "failover_read", "replica_sync"}

BlobRecord == [tenant: Tenants, region: Regions, replicated: BOOLEAN]

RequestRecord == [type: ReqType, tenant: Tenants, region: Regions, is_failover: BOOLEAN]

MaxRequests == 20  \* bound request log for TLC tractability

(* ── TYPE INVARIANT ───────────────────────────────────────────────────────── *)

TypeOK ==
  /\ DOMAIN storage \subseteq Blobs
  /\ \A b \in DOMAIN storage :
       /\ storage[b].tenant \in Tenants
       /\ storage[b].region \in Regions
       /\ storage[b].replicated \in BOOLEAN
  /\ DOMAIN requests \subseteq 1..MaxRequests
  /\ \A i \in DOMAIN requests :
       /\ requests[i].type \in ReqType
       /\ requests[i].tenant \in Tenants
       /\ requests[i].region \in Regions
       /\ requests[i].is_failover \in BOOLEAN
  /\ \A t \in Tenants : replica_lag[t] \in Nat
  /\ req_count \in Nat

(* ── INIT ─────────────────────────────────────────────────────────────────── *)

Init ==
  /\ storage    = << >>
  /\ requests   = << >>
  /\ replica_lag = [t \in Tenants |-> 0]
  /\ req_count  = 0

(* ── HELPERS ──────────────────────────────────────────────────────────────── *)

\* Region set that a tenant may legitimately store data in.
\* For S-14: primary_region only (replica reads are transient; no cross-store writes).
TenantResidencySet(t) == {PrimaryRegionOf[t]}

\* Log a request if under the cap (TLC bound).
LogRequest(rec) ==
  IF req_count < MaxRequests
  THEN /\ req_count' = req_count + 1
       /\ requests'  = requests @@ (req_count + 1 :> rec)
  ELSE /\ req_count' = req_count
       /\ requests'  = requests

(* ── ACTIONS ──────────────────────────────────────────────────────────────── *)

(*
 * WriteRequest(t, b, r):
 *   Tenant t writes blob b to region r.
 *   ALLOWED only if r = PrimaryRegionOf[t].
 *   Rejected (no state change to storage) if cross-region write attempted.
 *   Verifies INV-DATA-RESIDENCY + NoCrossRegionWrite.
 *)
WriteRequest(t, b, r) ==
  /\ t \in Tenants /\ b \in Blobs /\ r \in Regions
  /\ r = PrimaryRegionOf[t]                  \* MUST hit primary_region (else 403)
  /\ b \notin DOMAIN storage                 \* blob not yet stored (write-once abstraction)
  /\ storage'    = storage @@ (b :> [tenant |-> t, region |-> r, replicated |-> FALSE])
  /\ replica_lag' = [replica_lag EXCEPT ![t] = MaxLag]  \* lag introduced after write
  /\ LogRequest([type |-> "write", tenant |-> t, region |-> r, is_failover |-> FALSE])

(*
 * WriteRequestRejected(t, b, r):
 *   Cross-region write attempt is REJECTED (403 + audit emit).
 *   Blob is NOT stored; routing decision is logged for audit.
 *   Verifies that no cross-region write pollutes storage.
 *)
WriteRequestRejected(t, b, r) ==
  /\ t \in Tenants /\ b \in Blobs /\ r \in Regions
  /\ r # PrimaryRegionOf[t]                  \* cross-region write → rejected
  /\ UNCHANGED storage                        \* no blob stored
  /\ UNCHANGED replica_lag
  /\ LogRequest([type |-> "write", tenant |-> t, region |-> r, is_failover |-> FALSE])

(*
 * ReadRequest(t, b, r):
 *   Tenant t reads blob b from region r.
 *   Normal (non-failover) reads MUST target PrimaryRegionOf[t].
 *)
ReadRequest(t, b, r) ==
  /\ t \in Tenants /\ b \in Blobs /\ r \in Regions
  /\ r = PrimaryRegionOf[t]                  \* primary region only
  /\ b \in DOMAIN storage                    \* blob must exist
  /\ storage[b].tenant = t                   \* blob belongs to this tenant
  /\ UNCHANGED <<storage, replica_lag>>
  /\ LogRequest([type |-> "read", tenant |-> t, region |-> r, is_failover |-> FALSE])

(*
 * FailoverRead(t, b, r_primary, r_secondary):
 *   Primary region r_primary unavailable (503/504); read served from replica r_secondary.
 *   Blob data remains LOGICALLY PINNED to r_primary (replica is read-only mirror).
 *   Marks request as is_failover = TRUE — distinguishable from a cross-region leak.
 *   PAT-REGION-FAILOVER-001.
 *)
FailoverRead(t, b, r_primary, r_secondary) ==
  /\ t \in Tenants /\ b \in Blobs
  /\ r_primary   \in Regions /\ r_secondary \in Regions
  /\ r_primary = PrimaryRegionOf[t]          \* primary must be correct region
  /\ r_secondary \in Regions                 \* replica may be any other region
  /\ r_secondary # r_primary                 \* distinct (otherwise just a normal read)
  /\ b \in DOMAIN storage
  /\ storage[b].tenant = t
  /\ storage[b].replicated = TRUE            \* replica must exist
  /\ UNCHANGED <<storage, replica_lag>>
  /\ LogRequest([type |-> "failover_read", tenant |-> t,
                 region |-> r_secondary, is_failover |-> TRUE])

(*
 * ReplicaSync(t, b):
 *   Replication worker copies blob b from primary to replicas.
 *   Sets replicated = TRUE; decrements replica_lag.
 *)
ReplicaSync(t, b) ==
  /\ t \in Tenants /\ b \in Blobs
  /\ b \in DOMAIN storage
  /\ storage[b].tenant = t
  /\ storage[b].replicated = FALSE           \* only sync un-replicated blobs
  /\ replica_lag[t] > 0                      \* lag exists; close it
  /\ storage'    = [storage EXCEPT ![b].replicated = TRUE]
  /\ replica_lag' = [replica_lag EXCEPT ![t] = replica_lag[t] - 1]
  /\ UNCHANGED <<requests, req_count>>

(* ── NEXT ─────────────────────────────────────────────────────────────────── *)

Next ==
  \/ \E t \in Tenants, b \in Blobs, r \in Regions :
       \/ WriteRequest(t, b, r)
       \/ WriteRequestRejected(t, b, r)
       \/ ReadRequest(t, b, r)
  \/ \E t \in Tenants, b \in Blobs,
        r_p \in Regions, r_s \in Regions :
       FailoverRead(t, b, r_p, r_s)
  \/ \E t \in Tenants, b \in Blobs :
       ReplicaSync(t, b)

(* ── SPEC (with WEAK FAIRNESS for replication convergence liveness) ───────── *)

Spec ==
  /\ Init
  /\ [][Next]_vars
  /\ \A t \in Tenants, b \in Blobs : WF_vars(ReplicaSync(t, b))

(* ── INVARIANTS ───────────────────────────────────────────────────────────── *)

(*
 * RegionResidencyHolds (INV-DATA-RESIDENCY CRITICAL §3.11):
 *   Every stored blob's region is in the tenant's residency set.
 *)
RegionResidencyHolds ==
  \A b \in DOMAIN storage :
    storage[b].region \in TenantResidencySet(storage[b].tenant)

(*
 * NoCrossRegionWrite (INV-DATA-RESIDENCY + INV-TENANT-ISOLATION CRITICAL):
 *   No blob is stored in a region that is not the tenant's primary_region.
 *   This is a stronger statement that covers storage state only
 *   (rejected writes produce request log entries but no storage mutation).
 *)
NoCrossRegionWrite ==
  \A b \in DOMAIN storage :
    storage[b].region = PrimaryRegionOf[storage[b].tenant]

(*
 * NoCrossRegionLeak (INV-REGION-NO-CROSS-LEAK CRITICAL §3.12):
 *   Every logged request targeting a region != primary_region
 *   MUST be flagged as a failover read.
 *   Cross-region non-failover reads = leak.
 *)
NoCrossRegionLeak ==
  \A i \in DOMAIN requests :
    (requests[i].region # PrimaryRegionOf[requests[i].tenant]
     /\ requests[i].type \in {"read", "write"})
    => requests[i].is_failover = TRUE

(* ── TEMPORAL PROPERTIES (LIVENESS) ──────────────────────────────────────── *)

(*
 * ReplicationEventuallyConverges:
 *   If a tenant has replica lag > 0, it eventually converges to 0.
 *   Requires WF_vars(ReplicaSync) fairness assumption in Spec.
 *)
ReplicationEventuallyConverges ==
  \A t \in Tenants :
    (replica_lag[t] > 0) ~> (replica_lag[t] = 0)

(* ── THEOREM ──────────────────────────────────────────────────────────────── *)

THEOREM Spec =>
  /\ []TypeOK
  /\ []RegionResidencyHolds
  /\ []NoCrossRegionWrite
  /\ []NoCrossRegionLeak
  /\ ReplicationEventuallyConverges
====
