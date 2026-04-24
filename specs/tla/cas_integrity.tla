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
\* requested_digest. v2: server retorna body (possivelmente corrompido);
\* client verifica hash REAL contra digest requested.
Read(c, requested_digest) ==
    /\ c \in Clients
    /\ requested_digest \in Digests
    /\ op_count < MaxOps
    /\ \/ /\ requested_digest \in DOMAIN r2_storage
          /\ LET body == r2_storage[requested_digest]
                 actual_hash == Hash(body)
             IN
                IF actual_hash = requested_digest
                  THEN read_log' = Append(read_log, <<c, requested_digest, body, "verify_ok">>)
                  ELSE read_log' = Append(read_log, <<c, requested_digest, body, "verify_mismatch">>)
       \/ /\ requested_digest \notin DOMAIN r2_storage
          /\ read_log' = Append(read_log, <<c, requested_digest, "NONE", "not_found">>)
    /\ op_count' = op_count + 1
    /\ UNCHANGED <<hash_fn, r2_storage, write_log, corruption_flags>>

\* Adversarial v2 (Lote 6.1, endereça G-02): simular bit rot MUTANDO o body,
\* não apenas setando flag. Atacante/hardware consegue trocar body[d] por
\* body alternativo b' com hash(b') != d. Isso TENSIONA genuinamente
\* InvCASIntegrity porque hash_fn[body_atual] != d após BitRot.
BitRot(d, new_body) ==
    /\ d \in DOMAIN r2_storage
    /\ new_body \in Bodies
    /\ new_body # r2_storage[d]        \* realmente muta
    /\ d \notin corruption_flags       \* uma vez por digest (bound state)
    /\ op_count < MaxOps
    /\ r2_storage' = [r2_storage EXCEPT ![d] = new_body]
    /\ corruption_flags' = corruption_flags \union {d}
    /\ op_count' = op_count + 1
    /\ UNCHANGED <<hash_fn, write_log, read_log>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E c \in Clients, body \in Bodies, d \in Digests: Write(c, body, d)
    \/ \E c \in Clients, d \in Digests: Read(c, d)
    \/ \E d \in Digests, new_body \in Bodies: BitRot(d, new_body)

Spec == Init /\ [][Next]_vars

(*-- Invariantes CRITICAL ----------------------------------------------------*)

\* v2 (Lote 6.1, endereça G-02): invariantes reformulados porque BitRot
\* agora MUTA r2_storage. InvCASIntegrity sobre raw storage pode falhar
\* (por design — é o que atacante/hardware faz). O que importa é:
\*   1. Write path SEMPRE rejeita poisoning (InvPoisoningRejected)
\*   2. Client verify SEMPRE detecta corrupção (InvClientVerifyDetectsRot)
\*   3. Uncorrupted storage é consistente (InvCASIntegrityUncorrupted)

\* INV-CAS-INTEGRITY (refinado): para digest d SEM corruption_flag,
\* hash(body_atual) = d. Para digest d COM corruption_flag, sem garantia
\* (atacante mutou), MAS o client detecta via verify.
InvCASIntegrityUncorrupted ==
    \A d \in DOMAIN r2_storage:
        d \notin corruption_flags => Hash(r2_storage[d]) = d

\* Propriedade core: o sistema é safe sob atacante desde que
\* client verifique. Ou seja: se read retornou "verify_ok", então
\* body retornado realmente bate com digest (mesmo sob BitRot, client
\* recusou o body corrompido).
InvClientVerifyIsSound ==
    \A i \in 1..Len(read_log):
        LET entry == read_log[i]
            d == entry[2]
            body == entry[3]
            verify_outcome == entry[4]
        IN verify_outcome = "verify_ok" =>
            /\ d \in Digests  \* body não é "NONE"
            /\ body \in Bodies
            /\ Hash(body) = d

\* Nota: removido "InvClientVerifyDetectsRot" que era mal-formulado
\* (cobria corrupção temporal — client lia antes, BitRot depois).
\* InvClientVerifyIsSound é suficiente: prova que client NUNCA aceita
\* body com hash errado.

\* InvCASImmutability: body originalmente aceito em write permanece
\* inalterado (fora de BitRot adversarial). Modelo: toda entry "accepted"
\* em write_log com digest d tal que d \notin corruption_flags mantém
\* r2_storage[d] = body original.
InvCASImmutability ==
    \A i \in 1..Len(write_log):
        LET entry == write_log[i]
        IN (entry[4] = "accepted" /\ entry[3] \notin corruption_flags) =>
            r2_storage[entry[3]] = entry[2]

\* InvPoisoningRejected: nenhum write com hash mismatch escreveu body.
InvPoisoningRejected ==
    \A i \in 1..Len(write_log):
        LET entry == write_log[i]
        IN entry[4] = "rejected" =>
            \/ entry[3] \notin DOMAIN r2_storage
            \/ r2_storage[entry[3]] # entry[2]
            \/ entry[3] \in corruption_flags  \* BitRot depois pode ter coincidência

================================================================================
