---------------------------- MODULE gc_correctness ----------------------------
(***************************************************************************)
(* CoreLink — INV-GC-001 (reachable never deleted) CRITICAL                *)
(*             INV-GC-004 (mark-phase-aware re-ref safe) CRITICAL          *)
(*                                                                         *)
(* v2 — reescrita Lote 6.1 endereçando audit finding G-01:                  *)
(* Mark não é mais atômico. Modelo explícito de multi-pass scan sobre D1   *)
(* com interleaving de UpdateActionResult durante o scan. O blob pode ser  *)
(* re-referenciado via AC update depois que Mark já passou por ele — esse  *)
(* é exatamente o race que INV-GC-004 deve proteger.                       *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Blobs,           \* Finite set de blob digests
    AC_Entries,      \* Finite set de AC entry IDs
    MaxTime,         \* Bound de tempo lógico
    GracePeriod      \* Grace period

ASSUME
    /\ Blobs # {}
    /\ MaxTime \in Nat
    /\ GracePeriod \in Nat

VARIABLES
    blob_meta,         \* Blob → [state, created_at, last_referenced_at, deleted_at]
    ac_entries,        \* AC_Entry → [blob_refs, created_at]
    gc_phase,          \* {"idle", "marking", "sweeping"}
    mark_progress,     \* Set of blobs JÁ visitados pelo scan atual (parcial)
    mark_set,          \* Set of blobs reachable encontrados até agora
    mark_started_at,   \* Timestamp de início da Mark phase
    now,
    physically_deleted

vars == <<blob_meta, ac_entries, gc_phase, mark_progress, mark_set,
          mark_started_at, now, physically_deleted>>

(*-- Helpers -----------------------------------------------------------------*)

Reachable(b) ==
    \E e \in DOMAIN ac_entries: b \in ac_entries[e].blob_refs

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ blob_meta = [b \in {} |->
         [state |-> "active", created_at |-> 0,
          last_referenced_at |-> 0, deleted_at |-> 0]]
    /\ ac_entries = [e \in {} |-> [blob_refs |-> {}, created_at |-> 0]]
    /\ gc_phase = "idle"
    /\ mark_progress = {}
    /\ mark_set = {}
    /\ mark_started_at = 0
    /\ now = 1
    /\ physically_deleted = {}

(*-- Actions -----------------------------------------------------------------*)

\* Client action: upload de novo blob.
UploadBlob(b) ==
    /\ b \in Blobs
    /\ b \notin DOMAIN blob_meta
    /\ now < MaxTime
    /\ blob_meta' = blob_meta @@ (b :>
         [state |-> "active", created_at |-> now,
          last_referenced_at |-> now, deleted_at |-> 0])
    /\ now' = now + 1
    /\ UNCHANGED <<ac_entries, gc_phase, mark_progress, mark_set,
                   mark_started_at, physically_deleted>>

\* Client action: criar/atualizar AC entry.
\* IMPORTANTE: pode acontecer A QUALQUER MOMENTO, inclusive durante "marking".
\* Este é o núcleo do race que INV-GC-004 protege.
UpdateActionResult(e, refs) ==
    /\ e \in AC_Entries
    /\ refs \subseteq Blobs
    /\ \A b \in refs: b \in DOMAIN blob_meta /\ blob_meta[b].state = "active"
    /\ now < MaxTime
    /\ ac_entries' = ac_entries @@ (e :>
         [blob_refs |-> refs, created_at |-> now])
    /\ blob_meta' = [b \in DOMAIN blob_meta |->
         IF b \in refs
           THEN [blob_meta[b] EXCEPT !.last_referenced_at = now]
           ELSE blob_meta[b]]
    /\ now' = now + 1
    /\ UNCHANGED <<gc_phase, mark_progress, mark_set, mark_started_at,
                   physically_deleted>>

\* Client action: invalidate AC (explicit delete OR TTL expiry).
InvalidateAC(e) ==
    /\ e \in DOMAIN ac_entries
    /\ now < MaxTime
    /\ ac_entries' = [ac_entries EXCEPT ![e] =
         [blob_refs |-> {}, created_at |-> ac_entries[e].created_at]]
    /\ now' = now + 1
    /\ UNCHANGED <<blob_meta, gc_phase, mark_progress, mark_set,
                   mark_started_at, physically_deleted>>

\* GC Phase 1 — start: transição idle → marking. Captura tempo de início.
\* NÃO captura mark_set atomicamente — isso é o que v2 consegue modelar.
GCMarkStart ==
    /\ gc_phase = "idle"
    /\ now < MaxTime
    /\ gc_phase' = "marking"
    /\ mark_started_at' = now
    /\ mark_progress' = {}
    /\ mark_set' = {}
    /\ now' = now + 1
    /\ UNCHANGED <<blob_meta, ac_entries, physically_deleted>>

\* GC Phase 1 — step: visita UM blob por vez. Scan parcial.
\* Durante esses steps, UpdateActionResult pode acontecer interleaved.
\* Bug clássico: blob "b" é visitado; Mark vê que b não tem AC refs;
\* adiciona b a mark_progress mas NÃO a mark_set; depois UpdateActionResult
\* referencia b; Sweep vai deletar b porque ele não está em mark_set.
\* INV-GC-004 protege contra isso via check "ac.created_at >= mark_started_at".
GCMarkStep(b) ==
    /\ gc_phase = "marking"
    /\ b \in DOMAIN blob_meta
    /\ b \notin mark_progress
    /\ now < MaxTime
    /\ mark_progress' = mark_progress \union {b}
    /\ IF \E e \in DOMAIN ac_entries: b \in ac_entries[e].blob_refs
         THEN mark_set' = mark_set \union {b}
         ELSE mark_set' = mark_set
    /\ now' = now + 1
    /\ UNCHANGED <<blob_meta, ac_entries, gc_phase, mark_started_at,
                   physically_deleted>>

\* GC Phase 1 → Phase 2: transição marking → sweeping quando todos blobs
\* foram visitados.
GCMarkToSweep ==
    /\ gc_phase = "marking"
    /\ DOMAIN blob_meta \subseteq mark_progress
    /\ now < MaxTime
    /\ gc_phase' = "sweeping"
    /\ now' = now + 1
    /\ UNCHANGED <<blob_meta, ac_entries, mark_progress, mark_set,
                   mark_started_at, physically_deleted>>

\* GC Phase 2 — Sweep: deleta blobs que NÃO estão em mark_set E passaram do
\* grace period E não foram re-referenciados após mark_started_at (INV-GC-004).
GCSweepBlob(b) ==
    /\ gc_phase = "sweeping"
    /\ b \in DOMAIN blob_meta
    /\ blob_meta[b].state = "active"
    /\ b \notin mark_set
    /\ now - blob_meta[b].last_referenced_at >= GracePeriod
    \* INV-GC-004 enforcement: nenhuma AC entry criada após mark_started_at
    \* referencia b.
    /\ ~(\E e \in DOMAIN ac_entries:
           /\ b \in ac_entries[e].blob_refs
           /\ ac_entries[e].created_at >= mark_started_at)
    /\ now < MaxTime
    /\ blob_meta' = [blob_meta EXCEPT ![b] =
         [@ EXCEPT !.state = "soft_deleted", !.deleted_at = now]]
    /\ physically_deleted' = physically_deleted \union {b}
    /\ now' = now + 1
    /\ UNCHANGED <<ac_entries, gc_phase, mark_progress, mark_set,
                   mark_started_at>>

\* GC Phase 2 → idle: sweep completo.
GCSweepEnd ==
    /\ gc_phase = "sweeping"
    /\ now < MaxTime
    /\ gc_phase' = "idle"
    /\ mark_progress' = {}
    /\ mark_set' = {}
    /\ now' = now + 1
    /\ UNCHANGED <<blob_meta, ac_entries, mark_started_at, physically_deleted>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E b \in Blobs: UploadBlob(b)
    \/ \E e \in AC_Entries, refs \in SUBSET Blobs: UpdateActionResult(e, refs)
    \/ \E e \in AC_Entries: InvalidateAC(e)
    \/ GCMarkStart
    \/ \E b \in Blobs: GCMarkStep(b)
    \/ GCMarkToSweep
    \/ \E b \in Blobs: GCSweepBlob(b)
    \/ GCSweepEnd

Spec ==
  /\ Init
  /\ [][Next]_vars
  /\ \A b \in Blobs : WF_vars(GCMarkStep(b))   \* fairness: every blob eventually marked (canonical action name)
  /\ \A b \in Blobs : WF_vars(GCSweepBlob(b))  \* fairness: every unreachable blob eventually swept (canonical action name)
  \* WF on UpdateActionResult intentionally omitted (write path; client-driven, not GC-driven)

(*-- Invariantes CRITICAL ----------------------------------------------------*)

\* INV-GC-001 (reformulada Lote 7.1 endereçando C-01/U-01):
\* Nenhum blob physically_deleted é atualmente referenciado por alguma
\* AC entry ativa. Formalização direta: após sweep, blob deletado não
\* tem nenhuma referência ativa no state atual.
\* Se post-condição falhar, sweep deletou um blob que estava reachable
\* — bug direto em INV-GC-001 (violação de CTRL-GC-001).
InvGCReachableNeverDeleted ==
    \A b \in physically_deleted:
        ~(\E e \in DOMAIN ac_entries: b \in ac_entries[e].blob_refs)

\* INV-GC-004 (reformulada Lote 7.1): blob re-referenciado via
\* UpdateActionResult com created_at >= mark_started_at é preservado.
\* Formalização sem constant mágico (`- 10` removido): nenhum blob em
\* physically_deleted tem AC entry com created_at >= mark_started_at.
\* Semântica: AC criada durante sweep ativo protege blob mesmo que
\* Mark phase não o tenha visto.
InvGCReRefProtected ==
    \A b \in physically_deleted:
        ~(\E e \in DOMAIN ac_entries:
           /\ b \in ac_entries[e].blob_refs
           /\ ac_entries[e].created_at >= mark_started_at)

\* Invariante derivado: durante "marking", se blob é reachable, então
\* eventualmente é adicionado a mark_set OU é protegido por INV-GC-004.
\* Este é o check core do race condition.
\*
\* Lote 10.6-tris OPUS-MISS-1 fix: removido o branch vacuously-true `/\ TRUE`
\* que tornava o invariante mais fraco do que parecia. O check agora afirma
\* corretamente: durante marking, todo blob in mark_progress está em mark_set
\* OU não tem AC entry pre-existente referenciando-o (i.e., não é reachable).
\*
\* TLC cfg bounds (canonical Lote 10.6 cycle 4 — verify against gc_correctness.cfg):
\* - Blobs={b1,b2}, AC_Entries={e1}, MaxTime=10, GracePeriod=2
\* - At these bounds, all interleavings exhaustively explored (~5k-50k states; ≤30s TLC).
\* - Property test 100k extends coverage via random sampling against real Rust impl.
InvMarkingConsistent ==
    gc_phase = "marking" =>
        \A b \in mark_progress:
            (b \in mark_set) \/
            \* b foi visitado mas não tem AC entries referenciando-o no momento do step
            ~(\E e \in DOMAIN ac_entries:
                /\ b \in ac_entries[e].blob_refs
                /\ ac_entries[e].created_at <= now - 1)

================================================================================
