---------------------------- MODULE cas_integrity ----------------------------
(***************************************************************************)
(* CoreLink — INV-CAS-INTEGRITY (CRITICAL)                                 *)
(*                                                                         *)
(* Endereça `invariant_registry.md §3.2` + `security_model.md §6.2         *)
(* CTRL-CAS-001/002` + `failure_modes.md FM-051/FM-254`.                   *)
(*                                                                         *)
(* Modelo: CAS com content-addressable naming — write rejeita se           *)
(* hash(body) != digest; client-side verify on read detecta corruption.    *)
(*                                                                         *)
(* Invariante core:                                                        *)
(*   Para todo blob B armazenado no R2, hash(body(B)) = digest(path(B)).   *)
(*   Violação = cache poisoning OU bit rot — ambos catastróficos.          *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Bodies,          \* Finite set de byte-sequences possíveis (abstrato)
    Digests,         \* Finite set de digests possíveis
    Clients,         \* Finite set de client IDs
    MaxOps

ASSUME
    /\ Bodies # {}
    /\ Digests # {}
    /\ Cardinality(Bodies) <= Cardinality(Digests)

VARIABLES
    hash_fn,          \* Function Bodies -> Digests, imutável durante execução
    r2_storage,       \* digest -> body (content-addressable)
    write_log,        \* Sequence de (client, body, claimed_digest, outcome)
    read_log,         \* Sequence de (client, digest, body_received, verify_outcome)
    corruption_flags, \* Set of digests com bit rot simulado
    op_count

vars == <<hash_fn, r2_storage, write_log, read_log, corruption_flags, op_count>>

\* Hash concreto: usa variável hash_fn (fixa no Init, UNCHANGED depois).
Hash(b) == hash_fn[b]

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ hash_fn \in [Bodies -> Digests]
    /\ \A b1, b2 \in Bodies: b1 # b2 => hash_fn[b1] # hash_fn[b2]  \* injetiva
    /\ r2_storage = [d \in {} |-> CHOOSE x \in Bodies: TRUE]
    /\ write_log = <<>>
    /\ read_log = <<>>
    /\ corruption_flags = {}
    /\ op_count = 0

(*-- Actions -----------------------------------------------------------------*)

\* CTRL-CAS-001: write path REJEITA se hash(body) != claimed_digest.
\* Modelo: cliente envia (body, claimed_digest); server valida.
Write(c, body, claimed_digest) ==
    /\ c \in Clients
    /\ body \in Bodies
    /\ claimed_digest \in Digests
    /\ op_count < MaxOps
    /\ \/ /\ Hash(body) = claimed_digest  \* integridade OK
          /\ r2_storage' = r2_storage @@ (claimed_digest :> body)
          /\ write_log' = Append(write_log, <<c, body, claimed_digest, "accepted">>)
       \/ /\ Hash(body) # claimed_digest  \* poisoning attempt
          /\ write_log' = Append(write_log, <<c, body, claimed_digest, "rejected">>)
          /\ UNCHANGED r2_storage
    /\ op_count' = op_count + 1
    /\ UNCHANGED <<hash_fn, read_log, corruption_flags>>

\* Client read + verify. CTRL-CAS-002 manda client verificar hash(body) =
\* requested_digest. Server pode retornar body corrompido (bit rot), e
\* client detecta.
Read(c, requested_digest) ==
    /\ c \in Clients
    /\ requested_digest \in Digests
    /\ op_count < MaxOps
    /\ \/ /\ requested_digest \in DOMAIN r2_storage
          /\ LET body == r2_storage[requested_digest]
                 actual_hash == Hash(body)
             IN
                \/ /\ requested_digest \notin corruption_flags
                   /\ read_log' = Append(read_log, <<c, requested_digest, body, "verify_ok">>)
                \/ /\ requested_digest \in corruption_flags
                   \* Bit rot: server retorna body com hash divergente
                   /\ read_log' = Append(read_log, <<c, requested_digest, body, "verify_mismatch">>)
       \/ /\ requested_digest \notin DOMAIN r2_storage
          /\ read_log' = Append(read_log, <<c, requested_digest, "NONE", "not_found">>)
    /\ op_count' = op_count + 1
    /\ UNCHANGED <<hash_fn, r2_storage, write_log, corruption_flags>>

\* Adversarial: simular bit rot em um blob armazenado.
BitRot(d) ==
    /\ d \in DOMAIN r2_storage
    /\ d \notin corruption_flags
    /\ op_count < MaxOps
    /\ corruption_flags' = corruption_flags \union {d}
    /\ op_count' = op_count + 1
    /\ UNCHANGED <<hash_fn, r2_storage, write_log, read_log>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E c \in Clients, body \in Bodies, d \in Digests: Write(c, body, d)
    \/ \E c \in Clients, d \in Digests: Read(c, d)
    \/ \E d \in Digests: BitRot(d)

Spec == Init /\ [][Next]_vars

(*-- Invariantes CRITICAL ----------------------------------------------------*)

\* INV-CAS-INTEGRITY: para todo blob armazenado, hash(body) = digest do path.
\* Garantido por CTRL-CAS-001 (write-time check).
InvCASIntegrity ==
    \A d \in DOMAIN r2_storage:
        Hash(r2_storage[d]) = d

\* INV-CAS-IDEMPOTENCY: mesmo body sempre mapeia para mesmo digest.
\* Garantido pela definição Hash \in [Bodies -> Digests] (função determinística).
\* (Check vacuous no modelo, mas documentado.)

\* INV-CAS-IMMUTABILITY: uma vez escrito, body nunca muda para um digest.
\* Modelo: r2_storage é @@ union, não substitui. Implícito no Write action.
\* Para ser explícito:
InvCASImmutability ==
    \A d \in DOMAIN r2_storage:
        \A i \in 1..Len(write_log):
            LET entry == write_log[i]
            IN (entry[3] = d /\ entry[4] = "accepted") =>
                r2_storage[d] = entry[2]

\* Propriedade de cache poisoning resistance: nenhum write "rejected" resultou
\* em blob no storage.
InvPoisoningRejected ==
    \A i \in 1..Len(write_log):
        LET entry == write_log[i]
        IN entry[4] = "rejected" =>
            \/ entry[3] \notin DOMAIN r2_storage
            \/ r2_storage[entry[3]] # entry[2]  \* body rejeitado não ficou

\* Bit rot é detectado por client verify (INV-CAS-CLIENT-VERIFY).
\* Se read retornou "verify_mismatch", então há corruption_flag.
\* (Essa é uma consequência do modelo, útil para sanity.)
InvClientVerifyDetectsRot ==
    \A i \in 1..Len(read_log):
        LET entry == read_log[i]
            d == entry[2]
            verify_outcome == entry[4]
        IN verify_outcome = "verify_mismatch" => d \in corruption_flags

================================================================================
