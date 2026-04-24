---------------------------- MODULE gc_correctness ----------------------------
(***************************************************************************)
(* CoreLink — INV-GC-001 (reachable never deleted) CRITICAL                *)
(*             INV-GC-004 (mark-phase-aware re-ref safe) CRITICAL          *)
(*                                                                         *)
(* Endereça `invariant_registry.md §3.4` + `remote_cache_product_profile  *)
(* .md §9` + `failure_modes.md FM-300/FM-404` + audit findings S-12, S-15. *)
(*                                                                         *)
(* Modelo: GC mark-and-sweep com grace period + mark_started_at-aware      *)
(* check para evitar race de re-referência via AC update mid-GC.            *)
(*                                                                         *)
(* Invariantes:                                                            *)
(*   INV-GC-001: blob reachable no momento do sweep não é deletado.        *)
(*   INV-GC-004: blob re-referenciado após mark_started_at é preservado.   *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Blobs,           \* Finite set of blob digests (abstrato)
    AC_Entries,      \* Finite set of AC entry IDs
    MaxTime,         \* Bound pra tempo lógico
    GracePeriod      \* Grace period em unidades de tempo (ex: 72)

ASSUME
    /\ Blobs # {}
    /\ MaxTime \in Nat
    /\ GracePeriod \in Nat

VARIABLES
    blob_meta,        \* Blob → {created_at, last_referenced_at, deleted_at, state}
    ac_entries,       \* AC_Entry → {blob_refs: Set of Blobs, created_at}
    gc_phase,         \* {"idle", "mark", "sweep"}
    gc_mark_set,      \* Set of Blobs marcados como reachable (snapshot da mark phase)
    mark_started_at,  \* Timestamp de quando a mark phase iniciou (INV-GC-004 core)
    now,              \* Tempo lógico
    physically_deleted  \* Set of Blobs com physical delete (após grace + tombstone)

vars == <<blob_meta, ac_entries, gc_phase, gc_mark_set, mark_started_at, now, physically_deleted>>

(*-- Helpers -----------------------------------------------------------------*)

\* Blob está "alive" (não physical-deleted, pode estar soft-deleted).
Alive(b) == b \in DOMAIN blob_meta /\ blob_meta[b].state # "physical_deleted"

\* Blob é reachable: referenciado por pelo menos uma AC entry active.
Reachable(b) ==
    \E e \in DOMAIN ac_entries:
        b \in ac_entries[e].blob_refs

\* Blob está dentro do grace period (CTRL-GC-001; 72h para CAS).
InGracePeriod(b) ==
    /\ b \in DOMAIN blob_meta
    /\ now - blob_meta[b].last_referenced_at < GracePeriod

\* CTRL-GC-001 + INV-GC-004: sweep decision.
\* Blob só é soft-deleted se:
\*   (1) Está em gc_mark_set? NÃO (não reachable segundo mark)
\*   (2) age (now - last_referenced_at) > GracePeriod
\*   (3) CRITICAL: nenhuma AC entry nova foi criada após mark_started_at
\*       referenciando este blob (INV-GC-004)
CanSweep(b) ==
    /\ b \notin gc_mark_set
    /\ ~InGracePeriod(b)
    \* INV-GC-004 check: re-reference após mark_started_at protege blob
    /\ ~(\E e \in DOMAIN ac_entries:
           /\ b \in ac_entries[e].blob_refs
           /\ ac_entries[e].created_at >= mark_started_at)

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ blob_meta = [b \in {} |->
         [created_at |-> 0, last_referenced_at |-> 0,
          deleted_at |-> 0, state |-> "unknown"]]
    /\ ac_entries = [e \in {} |-> [blob_refs |-> {}, created_at |-> 0]]
    /\ gc_phase = "idle"
    /\ gc_mark_set = {}
    /\ mark_started_at = 0
    /\ now = 1
    /\ physically_deleted = {}

(*-- Actions -----------------------------------------------------------------*)

\* Client uploads a new blob.
UploadBlob(b) ==
    /\ b \in Blobs
    /\ b \notin DOMAIN blob_meta
    /\ now < MaxTime
    /\ blob_meta' = blob_meta @@ (b :>
         [created_at |-> now, last_referenced_at |-> now,
          deleted_at |-> 0, state |-> "active"])
    /\ now' = now + 1
    /\ UNCHANGED <<ac_entries, gc_phase, gc_mark_set, mark_started_at, physically_deleted>>

\* Client creates or updates an AC entry that references blobs.
UpdateActionResult(e, refs) ==
    /\ e \in AC_Entries
    /\ refs \subseteq Blobs
    /\ \A b \in refs: b \in DOMAIN blob_meta /\ blob_meta[b].state = "active"
    /\ now < MaxTime
    /\ ac_entries' = ac_entries @@ (e :>
         [blob_refs |-> refs, created_at |-> now])
    \* Update last_referenced_at nos blobs referenciados:
    /\ blob_meta' = [b \in DOMAIN blob_meta |->
         IF b \in refs
           THEN [blob_meta[b] EXCEPT !.last_referenced_at = now]
           ELSE blob_meta[b]]
    /\ now' = now + 1
    /\ UNCHANGED <<gc_phase, gc_mark_set, mark_started_at, physically_deleted>>

\* Client invalidates an AC entry (ex: via TTL ou explicit delete).
InvalidateAC(e) ==
    /\ e \in DOMAIN ac_entries
    /\ now < MaxTime
    /\ ac_entries' = [ac_entries EXCEPT ![e] = [blob_refs |-> {}, created_at |-> ac_entries[e].created_at]]
    /\ now' = now + 1
    /\ UNCHANGED <<blob_meta, gc_phase, gc_mark_set, mark_started_at, physically_deleted>>

\* GC Phase 1 (Mark): captura snapshot de reachable blobs.
GCMarkStart ==
    /\ gc_phase = "idle"
    /\ now < MaxTime
    /\ mark_started_at' = now
    /\ gc_mark_set' = {b \in DOMAIN blob_meta:
        \E e \in DOMAIN ac_entries: b \in ac_entries[e].blob_refs}
    /\ gc_phase' = "sweep"
    /\ now' = now + 1
    /\ UNCHANGED <<blob_meta, ac_entries, physically_deleted>>

\* GC Phase 2 (Sweep): soft-delete blobs que CanSweep.
\* Physical delete é depois do tombstone grace (não modelado aqui — foca em correctness do sweep decision).
GCSweepBlob(b) ==
    /\ gc_phase = "sweep"
    /\ b \in DOMAIN blob_meta
    /\ blob_meta[b].state = "active"
    /\ CanSweep(b)
    /\ now < MaxTime
    /\ blob_meta' = [blob_meta EXCEPT ![b] = [@ EXCEPT !.state = "soft_deleted", !.deleted_at = now]]
    /\ physically_deleted' = physically_deleted \union {b}  \* modelo abstrato: vamos
                                                               \* assumir que soft-delete
                                                               \* eventualmente vira physical
    /\ now' = now + 1
    /\ UNCHANGED <<ac_entries, gc_phase, gc_mark_set, mark_started_at>>

\* GC Sweep completa, volta a idle.
GCSweepEnd ==
    /\ gc_phase = "sweep"
    /\ now < MaxTime
    /\ gc_phase' = "idle"
    /\ gc_mark_set' = {}
    /\ now' = now + 1
    /\ UNCHANGED <<blob_meta, ac_entries, mark_started_at, physically_deleted>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E b \in Blobs: UploadBlob(b)
    \/ \E e \in AC_Entries, refs \in SUBSET Blobs: UpdateActionResult(e, refs)
    \/ \E e \in AC_Entries: InvalidateAC(e)
    \/ GCMarkStart
    \/ \E b \in Blobs: GCSweepBlob(b)
    \/ GCSweepEnd

Spec == Init /\ [][Next]_vars

(*-- Invariantes CRITICAL ----------------------------------------------------*)

\* INV-GC-001: Blob reachable (referenciado por alguma AC entry) nunca é
\* physically_deleted. Essa é a formalização literal.
\*
\* Nota: blob pode estar soft-deleted por curto período enquanto transiente,
\* MAS se ele ganha uma AC reference, o blob deve retornar a active (fluxo de
\* resurrection). Para simplicidade do modelo, consideramos physical_deleted
\* como terminal state e verificamos invariante sobre ele.
InvGCReachableNeverDeleted ==
    \A b \in physically_deleted:
        ~Reachable(b)

\* INV-GC-004: Blob re-referenciado via UpdateActionResult com created_at
\* >= mark_started_at é preservado mesmo durante sweep in-flight.
\* Formalização: no momento do sweep, CanSweep(b) é falso se existe uma
\* AC entry com created_at >= mark_started_at referenciando b.
\* (Esse check está embutido em CanSweep; o invariante garante que nenhum
\* blob com re-ref é deletado.)
InvGCReRefProtected ==
    \A b \in physically_deleted:
        ~(\E e \in DOMAIN ac_entries:
           /\ b \in ac_entries[e].blob_refs
           /\ ac_entries[e].created_at >= mark_started_at)

\* INV-GC-003 (refcount consistency — simplificado):
\* refcount "real" é o número de AC entries referenciando. blob_meta não
\* persiste refcount no modelo, mas podemos checar que soft_deleted ⇒
\* refcount = 0 no momento da decisão.
\* (Verificação de consistency em reconcile job — ver CTRL-GC-002.)

(*-- Propriedade de liveness (opcional) --------------------------------------*)

\* Eventualmente GC roda pelo menos uma vez (sanity).
Liveness_GC_Runs == <>(\E b \in Blobs: b \in physically_deleted \/ gc_phase = "sweep")

================================================================================
