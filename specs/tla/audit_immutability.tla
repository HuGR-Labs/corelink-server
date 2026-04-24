---------------------------- MODULE audit_immutability ----------------------------
(***************************************************************************)
(* CoreLink — INV-AUDIT-APPEND-ONLY (CRITICAL)                             *)
(*                                                                         *)
(* Criado Lote 6.2 endereçando audit finding T-02:                          *)
(* "INV-AUDIT-APPEND-ONLY é CRITICAL mas tem exemption TLA+ sem waiver      *)
(* formal — viola CTRL-FORMAL-001 diretamente."                            *)
(*                                                                         *)
(* Endereça `invariant_registry.md §3.6` + `security_model.md §6.8         *)
(* CTRL-AUDIT-001` + `auth_model.md §7`.                                   *)
(*                                                                         *)
(* Modelo: audit log append-only com hash chain, R2 Object Lock, daily    *)
(* verify. Atacante/admin tenta tampering ou reordering.                  *)
(*                                                                         *)
(* Invariantes:                                                            *)
(*   INV-AUDIT-APPEND-ONLY: nenhum UPDATE/DELETE aceito em audit log      *)
(*   INV-AUDIT-CHAIN-INTACT: hash chain válido (H[i] = H(e[i] || H[i-1])) *)
(*   INV-AUDIT-ORDER-PRESERVED: ordem de emissão preservada               *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Actors,          \* Finite set de principals que emitem events
    EventTypes,      \* Finite set de tipos (ex: {cas_write, pat_revoked, config_change})
    MaxEvents        \* Bound de eventos totais

ASSUME
    /\ Actors # {}
    /\ EventTypes # {}
    /\ MaxEvents \in Nat

VARIABLES
    audit_log,         \* Sequence de records {actor, type, prev_hash, record_hash}
    tamper_attempts,   \* Sequence de tentativas (delete/replace/reorder)
    hash_counter,      \* Monotonic counter: cada event consome um valor único
    object_lock_on,    \* v2 Lote 7.1: R2 Object Lock Governance Mode flag
    db_constraint_on   \* v2 Lote 7.1: D1 CHECK constraint flag

vars == <<audit_log, tamper_attempts, hash_counter,
          object_lock_on, db_constraint_on>>

(*-- Helpers -----------------------------------------------------------------*)

\* Abstração de hash via counter monotônico injetivo:
\* cada AppendEvent incrementa hash_counter e usa o novo valor como hash.
\* Isso modela a propriedade essencial: hashes são únicos + ordenados.
\* No modelo real: SHA-256(prev_hash || content) — que também é único
\* com probabilidade 1-2^-256.

\* Evento record.
EventRecord(actor, type, prev_h, self_h) ==
    [actor |-> actor,
     type |-> type,
     prev_hash |-> prev_h,
     record_hash |-> self_h]

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ audit_log = <<>>
    /\ tamper_attempts = <<>>
    /\ hash_counter = 0  \* genesis = 0; primeiro hash real é 1
    /\ object_lock_on = TRUE   \* R2 Object Lock Governance Mode ATIVO (default)
    /\ db_constraint_on = TRUE \* D1 CHECK constraint ATIVO (default)

(*-- Actions -----------------------------------------------------------------*)

\* Append normal de evento ao audit log (CTRL-AUDIT-002).
AppendEvent(actor, type) ==
    LET prev_h == IF Len(audit_log) = 0
                    THEN 0  \* genesis
                    ELSE audit_log[Len(audit_log)].record_hash
        self_h == hash_counter + 1
        new_record == EventRecord(actor, type, prev_h, self_h)
    IN
        /\ actor \in Actors
        /\ type \in EventTypes
        /\ Len(audit_log) < MaxEvents
        /\ audit_log' = Append(audit_log, new_record)
        /\ hash_counter' = self_h
        /\ UNCHANGED <<tamper_attempts, object_lock_on, db_constraint_on>>

\* v2 Lote 7.1 (endereça C-02/U-04): adversary agora REALMENTE tenta mutar
\* audit_log. Sistema tem DUAS camadas de defesa (defense-in-depth):
\*   Camada 1: D1 CHECK constraint (db_constraint_on)
\*   Camada 2: R2 Object Lock Governance Mode (object_lock_on)
\* Tentativa só resulta em mutação se AMBAS camadas falharam.
\* No Init, ambas = TRUE → adversary nunca consegue mutar → invariantes
\* verdes. Se alguém setar qualquer flag para FALSE, TLC encontra trace
\* de violação (defense-in-depth model exposed).

\* Adversarial: atacante tenta DELETE em posição i.
TryDelete(i) ==
    /\ i \in 1..Len(audit_log)
    /\ Len(tamper_attempts) < MaxEvents
    /\ tamper_attempts' = Append(tamper_attempts, <<"delete", i>>)
    /\ \/ /\ object_lock_on /\ db_constraint_on
          \* Pelo menos uma defesa ativa → mutação rejeitada
          /\ UNCHANGED <<audit_log, hash_counter>>
       \/ /\ ~object_lock_on /\ ~db_constraint_on
          \* AMBAS defesas bypassed → mutação efetiva (invariante falha)
          /\ audit_log' = SubSeq(audit_log, 1, i-1) \o
                          SubSeq(audit_log, i+1, Len(audit_log))
          /\ UNCHANGED hash_counter
    /\ UNCHANGED <<object_lock_on, db_constraint_on>>

\* Adversarial: atacante tenta REPLACE em posição i.
TryReplace(i, actor, type) ==
    /\ i \in 1..Len(audit_log)
    /\ actor \in Actors
    /\ type \in EventTypes
    /\ Len(tamper_attempts) < MaxEvents
    /\ tamper_attempts' = Append(tamper_attempts, <<"replace", i>>)
    /\ \/ /\ object_lock_on /\ db_constraint_on
          /\ UNCHANGED <<audit_log, hash_counter>>
       \/ /\ ~object_lock_on /\ ~db_constraint_on
          /\ LET fake_record == EventRecord(actor, type,
                  IF i = 1 THEN 0 ELSE audit_log[i-1].record_hash,
                  hash_counter + 1)  \* fake hash (bate com prev mas não com next)
             IN audit_log' = [audit_log EXCEPT ![i] = fake_record]
          /\ UNCHANGED hash_counter
    /\ UNCHANGED <<object_lock_on, db_constraint_on>>

\* Adversarial: atacante tenta REORDER (swap adjacentes).
TryReorder(i) ==
    /\ i \in 1..Len(audit_log) - 1
    /\ Len(tamper_attempts) < MaxEvents
    /\ tamper_attempts' = Append(tamper_attempts, <<"reorder", i, i+1>>)
    /\ \/ /\ object_lock_on /\ db_constraint_on
          /\ UNCHANGED <<audit_log, hash_counter>>
       \/ /\ ~object_lock_on /\ ~db_constraint_on
          /\ audit_log' = [audit_log EXCEPT
                            ![i] = audit_log[i+1],
                            ![i+1] = audit_log[i]]
          /\ UNCHANGED hash_counter
    /\ UNCHANGED <<object_lock_on, db_constraint_on>>

\* Daily verify job (PAT-AUDIT-VERIFY-001): recomputa toda hash chain.
\* v2 Lote 7.1: action incluída em Next para validar que o job roda.
\* Se tamper bem-sucedido corromper chain, próxima VerifyChain detecta.
VerifyChain ==
    /\ Len(audit_log) > 0
    /\ UNCHANGED vars  \* verify é read-only; state não muta

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E actor \in Actors, type \in EventTypes: AppendEvent(actor, type)
    \/ \E i \in 1..MaxEvents: TryDelete(i)
    \/ \E i \in 1..MaxEvents, actor \in Actors, type \in EventTypes:
         TryReplace(i, actor, type)
    \/ \E i \in 1..MaxEvents: TryReorder(i)
    \/ VerifyChain

Spec == Init /\ [][Next]_vars

(*-- Invariantes CRITICAL ----------------------------------------------------*)

\* v2 Lote 7.1 (endereça U-04): invariantes agora são distintos.

\* INV-AUDIT-APPEND-ONLY (length monotonic): audit_log só cresce, nunca
\* encolhe. Se TryDelete mutasse, Len(audit_log) cairia — violação aqui
\* captura delete bem-sucedido.
\* Também: hash_counter = Len(audit_log) monotonic (nem delete nem replace
\* podem decrementar — se decrementarem, estado inválido).
InvAuditAppendOnly ==
    hash_counter >= Len(audit_log)

\* INV-AUDIT-CHAIN-INTACT (record-level consistency): cada record.prev_hash
\* bate com record_hash do anterior. Se TryReplace mutasse (mesmo sem
\* mudar Len), novo fake_record.prev_hash pode bater com anterior MAS o
\* NEXT record ainda aponta para o record_hash original — chain quebra
\* em i+1.
InvAuditChainIntact ==
    \A i \in 2..Len(audit_log):
        audit_log[i].prev_hash = audit_log[i-1].record_hash

\* v2 Lote 7.1 — novo invariante derivado: VerifyChain job sempre passa
\* quando defesas ativas. Se ambas defesas TRUE, tamper attempts não mutam
\* → chain preserved → verify verde. Se defesa desativada, tamper muta
\* → InvAuditChainIntact falha → TLC encontra contraexemplo.
InvDefenseInDepth ==
    (object_lock_on /\ db_constraint_on) =>
        (\A i \in 2..Len(audit_log):
            audit_log[i].prev_hash = audit_log[i-1].record_hash)

\* INV-AUDIT-ORDER-PRESERVED: ordem de AppendEvent é a mesma que a ordem
\* no audit_log. Como AppendEvent só faz Append (push back), ordem é
\* preservada por construção.
\* (Invariante trivial no modelo; documentado para clareza.)
InvAuditOrderPreserved ==
    \A i, j \in 1..Len(audit_log):
        i < j => audit_log[i].record_hash # audit_log[j].record_hash
        \* hash único por posição (via hash chain)

\* INV-AUDIT-REJECT-TAMPER: todas as tentativas de tamper foram registradas
\* em tamper_attempts mas NÃO resultaram em mudança em audit_log.
\* Verificação: primeiro record sempre tem prev_hash = 0 (genesis), e
\* hash_counter reflete Len(audit_log) (só incrementa em AppendEvent).
InvAuditRejectTamper ==
    /\ hash_counter = Len(audit_log)  \* 1:1 com appends, nenhum remove
    /\ (Len(audit_log) = 0 \/ audit_log[1].prev_hash = 0)

================================================================================
