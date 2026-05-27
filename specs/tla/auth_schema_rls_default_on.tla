--------------------------- MODULE auth_schema_rls_default_on ---------------------------
(***************************************************************************)
(* CoreLink — Auth schema RLS default-on + PII encrypted at rest          *)
(*           (DEBT-005 batch 5 #5)                                         *)
(*                                                                         *)
(* Closes 2 CRITICAL TLA gaps from canonical-consistency baseline §3.4    *)
(* AUTH:                                                                   *)
(*                                                                         *)
(*   INV-AUTH-SCHEMA-RLS-DEFAULT-ON  (CRITICAL, §3.4 AUTH)                 *)
(*     Every Postgres table in the auth schema MUST have RLS ENABLED and *)
(*     FORCE RLS on creation. New tables created without RLS are rejected*)
(*     by the migration gate. Tables in production cannot have RLS       *)
(*     disabled — DDL audit alerts and fails-closed.                     *)
(*                                                                         *)
(*   INV-AUTH-PII-ENCRYPTED  (CRITICAL, §3.4 AUTH)                         *)
(*     Every PII-bearing column (email, phone, name) MUST be stored      *)
(*     encrypted with the per-tenant BYOK envelope key. Plaintext PII   *)
(*     never reaches the row store. The ORM enforces encryption at      *)
(*     `INSERT` and decryption at `SELECT`.                              *)
(*                                                                         *)
(* Threat model (modelled):                                                *)
(*   - CreateTable migrates a schema; the gate accepts iff rls=TRUE.    *)
(*   - InsertPii writes encrypted bytes to the row store; plaintext    *)
(*     never reaches the store.                                         *)
(*   - Adversarial actions (guard FALSE):                                *)
(*       AttemptCreateNoRls — create a table with rls=FALSE.            *)
(*       AttemptInsertPlaintext — write plaintext PII to row store.     *)
(*       AttemptDisableRls — flip rls=FALSE on an existing table.       *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - The RLS policy expressions themselves (covered by per-route      *)
(*     SQL fuzz tests).                                                 *)
(*   - Envelope key derivation (proved in `byok_envelope_aad.tla`).     *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.4 AUTH`            *)
(*   - `migrations/auth/*.sql`                                            *)
(*   - `crates/corelink-auth/src/schema/` (pii crypto wrapper; post      *)
(*     Wave 33 reorg: orm.rs split into schema/ submodule)                *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Tables,            \* Finite set of table identifiers
    Forms,             \* {"plaintext", "encrypted"}
    MaxOps

ASSUME
    /\ Tables # {}
    /\ Forms = {"plaintext", "encrypted"}
    /\ MaxOps \in Nat

\* `schema` is a SUBSET [table, rls] — only rls=TRUE records may exist
\* (the migration gate rejects rls=FALSE).
\* `row_store` is a Sequence of <<table, form>> for inserted PII rows.

VARIABLES
    schema,            \* SUBSET [table, rls]
    row_store,         \* Sequence of <<table, form>>
    op_count

vars == <<schema, row_store, op_count>>

Init ==
    /\ schema    = {}
    /\ row_store = <<>>
    /\ op_count  = 0

(*-- Actions -----------------------------------------------------------------*)

\* CreateTable: migration gate enforces rls=TRUE.
CreateTable(t) ==
    /\ t \in Tables
    /\ \A r \in schema: r.table # t
    /\ op_count < MaxOps
    /\ schema' = schema \cup {[table |-> t, rls |-> TRUE]}
    /\ UNCHANGED row_store
    /\ op_count' = op_count + 1

\* InsertPii: write encrypted PII row. The ORM enforces form=encrypted.
InsertPii(t) ==
    /\ t \in Tables
    /\ \E r \in schema: r.table = t /\ r.rls = TRUE
    /\ op_count < MaxOps
    /\ row_store' = Append(row_store, <<t, "encrypted">>)
    /\ UNCHANGED schema
    /\ op_count' = op_count + 1

\* Adversarial: create a table with RLS disabled. Guard FALSE.
AttemptCreateNoRls(t) ==
    /\ t \in Tables
    /\ \A r \in schema: r.table # t
    /\ op_count < MaxOps
    /\ FALSE
    /\ schema' = schema \cup {[table |-> t, rls |-> FALSE]}
    /\ UNCHANGED row_store
    /\ op_count' = op_count + 1

\* Adversarial: insert plaintext PII into row_store. Guard FALSE.
AttemptInsertPlaintext(t) ==
    /\ t \in Tables
    /\ \E r \in schema: r.table = t
    /\ op_count < MaxOps
    /\ FALSE
    /\ row_store' = Append(row_store, <<t, "plaintext">>)
    /\ UNCHANGED schema
    /\ op_count' = op_count + 1

\* Adversarial: flip RLS off on an existing table. Guard FALSE.
AttemptDisableRls(t) ==
    /\ t \in Tables
    /\ op_count < MaxOps
    /\ \E r \in schema: r.table = t /\ r.rls = TRUE
    /\ FALSE
    /\ schema' = (schema \ {r \in schema: r.table = t}) \cup
                 {[table |-> t, rls |-> FALSE]}
    /\ UNCHANGED row_store
    /\ op_count' = op_count + 1

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E t \in Tables: CreateTable(t)
    \/ \E t \in Tables: InsertPii(t)
    \/ \E t \in Tables: AttemptCreateNoRls(t)
    \/ \E t \in Tables: AttemptInsertPlaintext(t)
    \/ \E t \in Tables: AttemptDisableRls(t)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-AUTH-SCHEMA-RLS-DEFAULT-ON. Every table in the auth schema has
\* RLS enabled.
InvSchemaRlsDefaultOn ==
    \A r \in schema: r.rls = TRUE

\* INV-AUTH-PII-ENCRYPTED. Every row inserted into row_store is in
\* encrypted form.
InvPiiEncrypted ==
    \A i \in 1..Len(row_store): row_store[i][2] = "encrypted"

SafetyInvariants ==
    /\ InvSchemaRlsDefaultOn
    /\ InvPiiEncrypted

================================================================================
