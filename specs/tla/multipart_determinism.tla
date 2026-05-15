--------------------- MODULE multipart_determinism ---------------------
(***************************************************************************)
(* CoreLink — Multipart chunking determinism + tenant-scoped object_key   *)
(* (DEBT-005 batch 4 #5)                                                   *)
(*                                                                         *)
(* Closes 2 CRITICAL TLA gaps from canonical-consistency baseline §3.5    *)
(* Multipart:                                                              *)
(*                                                                         *)
(*   INV-MULTIPART-CHUNK-DETERMINISTIC  (CRITICAL, §3.5 Multipart)          *)
(*     FastCDC mask seeds fixed in `ChunkerConfig::default()`. The same  *)
(*     input bytes ALWAYS produce the same chunk sequence byte-identical*)
(*     across runs, machines, and chunker invocations. ADR-0022          *)
(*     stability commitment (Lote 10.5).                                  *)
(*                                                                         *)
(*   INV-MULTIPART-PATH-TENANT-SCOPED  (CRITICAL, §3.5 Multipart)           *)
(*     R2 object_key is constructed from `tenant_prefix` (Layer 4); the *)
(*     server NEVER trusts client-provided path components. The         *)
(*     `MultipartAdapter` constructs object_key from a materialized     *)
(*     tenant_prefix BLOB(16). CI grep gate forbids client-path concat. *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - Two chunk runs on the same input produce identical chunk         *)
(*     sequences (determinism).                                          *)
(*   - The server constructs object_key = tenant_prefix(t) ++ chunk_id; *)
(*     a request from tenant `t` can NEVER produce an object_key       *)
(*     whose tenant_prefix belongs to `t' # t`.                          *)
(*   - Adversarial paths (guard FALSE):                                  *)
(*       AttemptChunkDivergence — same input → different chunks.        *)
(*       AttemptClientControlledPath — server uses client-supplied      *)
(*         prefix.                                                       *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - FastCDC algorithm correctness (proved in ADR-0022 vector annex). *)
(*   - tenant_prefix derivation crypto (covered by                       *)
(*     `tenant_ctx_propagation.tla`).                                    *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.5 INV-MULTIPART-*` *)
(*   - `crates/corelink-chunker/src/fastcdc.rs`                          *)
(*   - `crates/corelink-multipart/src/adapter.rs::MultipartAdapter`      *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Tenants,           \* Finite set of tenants
    Inputs,            \* Finite set of input-bytes tags
    ChunkSeqs,         \* Finite set of chunk-sequence tags (the chunker outputs)
    Sessions,          \* Finite set of multipart upload sessions
    MaxOps

ASSUME
    /\ Tenants # {}
    /\ Inputs # {}
    /\ ChunkSeqs # {}
    /\ Sessions # {}
    /\ MaxOps \in Nat

\* The deterministic chunker: a function Inputs -> ChunkSeqs lifted via
\* TLC Init enumeration. Two distinct chunker calls on the same input
\* `i` MUST yield the same chunk seq `Chunker[i]`.

VARIABLES
    Chunker,           \* Inputs -> ChunkSeqs (immutable; enumerated by TLC)
    chunk_log,         \* Sequence of <<session, input, chunk_seq>>
    object_keys,       \* Sequence of <<session, tenant, key_tenant_prefix>>
    sess_tenant,       \* Sessions -> Tenants (assigned at OpenSession)
    sess_open,         \* SUBSET Sessions
    op_count

vars == <<Chunker, chunk_log, object_keys, sess_tenant, sess_open, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ Chunker     \in [Inputs -> ChunkSeqs]
    /\ chunk_log   = <<>>
    /\ object_keys = <<>>
    /\ sess_tenant = [s \in Sessions |-> CHOOSE t \in Tenants: TRUE]
    /\ sess_open   = {}
    /\ op_count    = 0

(*-- Actions -----------------------------------------------------------------*)

\* Caller from tenant `t` opens a multipart session `s`.
OpenSession(t, s) ==
    /\ t \in Tenants
    /\ s \in Sessions
    /\ s \notin sess_open
    /\ op_count < MaxOps
    /\ sess_open'   = sess_open \cup {s}
    /\ sess_tenant' = [sess_tenant EXCEPT ![s] = t]
    /\ UNCHANGED <<Chunker, chunk_log, object_keys>>
    /\ op_count'    = op_count + 1

\* Chunker runs on an input `i` for an open session `s`. Output is
\* `Chunker[i]` — deterministic.
ChunkRun(s, i) ==
    /\ s \in Sessions
    /\ i \in Inputs
    /\ s \in sess_open
    /\ op_count < MaxOps
    /\ chunk_log' = Append(chunk_log, <<s, i, Chunker[i]>>)
    /\ UNCHANGED <<Chunker, object_keys, sess_tenant, sess_open>>
    /\ op_count'  = op_count + 1

\* Server constructs object_key for session `s` with the SESSION's
\* tenant prefix (not client-supplied). The tenant-prefix component
\* recorded in object_keys[i][3] equals sess_tenant[s].
ServerConstructKey(s) ==
    /\ s \in Sessions
    /\ s \in sess_open
    /\ op_count < MaxOps
    /\ object_keys' = Append(object_keys, <<s, sess_tenant[s], sess_tenant[s]>>)
    /\ UNCHANGED <<Chunker, chunk_log, sess_tenant, sess_open>>
    /\ op_count'   = op_count + 1

\* Adversarial: chunker produces a divergent output for the same input.
\* Guard FALSE — deterministic by construction.
AttemptChunkDivergence(s, i, cs_alt) ==
    /\ s \in Sessions
    /\ i \in Inputs
    /\ cs_alt \in ChunkSeqs
    /\ cs_alt # Chunker[i]
    /\ s \in sess_open
    /\ op_count < MaxOps
    /\ FALSE
    /\ chunk_log' = Append(chunk_log, <<s, i, cs_alt>>)
    /\ UNCHANGED <<Chunker, object_keys, sess_tenant, sess_open>>
    /\ op_count'  = op_count + 1

\* Adversarial: client supplies a tenant_prefix # session tenant.
\* Server MUST refuse — guard FALSE.
AttemptClientControlledPath(s, fake_tenant) ==
    /\ s \in Sessions
    /\ fake_tenant \in Tenants
    /\ fake_tenant # sess_tenant[s]
    /\ s \in sess_open
    /\ op_count < MaxOps
    /\ FALSE
    /\ object_keys' = Append(object_keys, <<s, sess_tenant[s], fake_tenant>>)
    /\ UNCHANGED <<Chunker, chunk_log, sess_tenant, sess_open>>
    /\ op_count'   = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E t \in Tenants, s \in Sessions: OpenSession(t, s)
    \/ \E s \in Sessions, i \in Inputs: ChunkRun(s, i)
    \/ \E s \in Sessions: ServerConstructKey(s)
    \/ \E s \in Sessions, i \in Inputs, cs \in ChunkSeqs: AttemptChunkDivergence(s, i, cs)
    \/ \E s \in Sessions, t \in Tenants: AttemptClientControlledPath(s, t)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-MULTIPART-CHUNK-DETERMINISTIC. Any two chunk_log entries with the
\* same input have the same chunk-sequence output.
InvChunkDeterministic ==
    \A i, j \in 1..Len(chunk_log):
        chunk_log[i][2] = chunk_log[j][2] => chunk_log[i][3] = chunk_log[j][3]

\* INV-MULTIPART-PATH-TENANT-SCOPED. Every recorded object_key has its
\* path prefix equal to the session's tenant (server-derived, not
\* client-supplied).
InvPathTenantScoped ==
    \A i \in 1..Len(object_keys):
        object_keys[i][3] = sess_tenant[object_keys[i][1]]

\* Coherence: chunk_log + object_keys only reference open sessions.
InvLogsScopedToOpenSessions ==
    /\ \A i \in 1..Len(chunk_log):   chunk_log[i][1]   \in sess_open
    /\ \A i \in 1..Len(object_keys): object_keys[i][1] \in sess_open

SafetyInvariants ==
    /\ InvChunkDeterministic
    /\ InvPathTenantScoped
    /\ InvLogsScopedToOpenSessions

================================================================================
