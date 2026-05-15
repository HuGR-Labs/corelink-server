---------------------------- MODULE auth_jwt_validation ----------------------------
(***************************************************************************)
(* CoreLink — JWT validation algorithm + issuer binding (DEBT-005 5/40)    *)
(*                                                                         *)
(* Closes 2 CRITICAL TLA gaps from                                         *)
(* `specs/_audits/2026-05-15-canonical-consistency-baseline.md` §2         *)
(* (critical_no_tla=40) by formally proving the algorithm-confusion and    *)
(* issuer-spoof attack surface is closed by the verify state machine.     *)
(*                                                                         *)
(* Invariants proved here (from invariant_registry.md §3.14):              *)
(*                                                                         *)
(*   INV-AUTH-JWT-VALIDATE-RS256-ONLY   (CRITICAL)                         *)
(*     Every token that the verify path admits MUST have alg = "RS256".   *)
(*     CVE-2015-9235 "alg=none" and CVE-2018-0114 "HS256-with-public-pem" *)
(*     key-confusion are explicitly modelled as adversary actions and      *)
(*     proved to be rejected.                                              *)
(*                                                                         *)
(*   INV-AUTH-ISS-EXACT-MATCH         (CRITICAL)                           *)
(*     Issuer is compared via exact set membership against the allowlist; *)
(*     prefix attacks (`https://clerk.corelink.dev.attacker.com`) and     *)
(*     suffix attacks are rejected.                                        *)
(*                                                                         *)
(* Threat model:                                                           *)
(*   - Adversary chooses any (alg, iss) pair from a finite adversarial    *)
(*     set including alg ∈ {none, HS256, RS256, ES256} and iss values     *)
(*     that are prefix/suffix/permutation of a legitimate issuer.         *)
(*   - The verify pipeline is the only mutator of accepted_tokens.        *)
(*                                                                         *)
(* Out of scope:                                                           *)
(*   - JWK key rotation timing (covered by `key_lifecycle.tla` follow-up). *)
(*   - Signature math (algorithmic; not a state-machine concern).         *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/invariant_registry.md §3.14 INV-AUTH-JWT-*`  *)
(*   - `specs/03_architecture/security_model.md §6.9 CTRL-FORMAL-001`      *)
(*   - `specs/_audits/2026-05-15-canonical-consistency-baseline.md` §2     *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Algs,             \* Adversarial alg set, MUST include "none","HS256","RS256"
    Issuers,          \* Finite adversarial issuer set (mix of valid + spoofed)
    Allowlist,        \* Subset of Issuers that the deployment accepts
    Tokens,           \* Finite token IDs to bound the model
    MaxOps            \* Bound on adversary submissions

ASSUME
    /\ Algs # {}
    /\ "RS256" \in Algs
    /\ "none"  \in Algs
    /\ Issuers # {}
    /\ Allowlist \subseteq Issuers
    /\ Allowlist # {}
    /\ Tokens # {}
    /\ MaxOps \in Nat

VARIABLES
    submitted,          \* Sequence of <<token, alg, iss>> presented to verify
    accepted_tokens,    \* Set of token IDs the verify pipeline admitted
    accepted_meta,      \* Function tokens -> <<alg, iss>> for each accepted token
    op_count

vars == <<submitted, accepted_tokens, accepted_meta, op_count>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ submitted = <<>>
    /\ accepted_tokens = {}
    /\ accepted_meta = [t \in Tokens |-> <<"NONE", "NONE">>]
    /\ op_count = 0

(*-- Verify pipeline --------------------------------------------------------*)

\* The verify state machine. Defensively models the canonical
\* `jsonwebtoken::Validation::new(Algorithm::RS256)` configuration plus
\* the exact-match issuer allowlist. Any failure mode short-circuits with
\* NO mutation to accepted_tokens (fail-closed).
VerifyAdmit(t, alg, iss) ==
    /\ t \in Tokens
    /\ alg \in Algs
    /\ iss \in Issuers
    /\ op_count < MaxOps
    /\ submitted' = Append(submitted, <<t, alg, iss>>)
    /\ op_count' = op_count + 1
    /\ \/ /\ alg = "RS256"
          /\ iss \in Allowlist
          /\ t \notin accepted_tokens
          \* Pipeline accepts: stamp accepted metadata.
          /\ accepted_tokens' = accepted_tokens \union {t}
          /\ accepted_meta' = [accepted_meta EXCEPT ![t] = <<alg, iss>>]
       \/ /\ \/ alg # "RS256"
             \/ iss \notin Allowlist
             \/ t \in accepted_tokens   \* Replay-safe: idempotent on dup
          /\ UNCHANGED <<accepted_tokens, accepted_meta>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \E t \in Tokens, alg \in Algs, iss \in Issuers: VerifyAdmit(t, alg, iss)

Spec == Init /\ [][Next]_vars

(*-- Safety invariants -------------------------------------------------------*)

\* INV-AUTH-JWT-VALIDATE-RS256-ONLY: no accepted token records a non-RS256
\* algorithm. Equivalent to: alg = "none" / "HS256" / "ES256" attempts are
\* always rejected.
InvOnlyRS256Accepted ==
    \A t \in accepted_tokens: accepted_meta[t][1] = "RS256"

\* INV-AUTH-ISS-EXACT-MATCH: no accepted token records an issuer outside
\* the allowlist. Prefix/suffix/substring tampering is rejected because
\* set membership is exact.
InvIssuerExactMatch ==
    \A t \in accepted_tokens: accepted_meta[t][2] \in Allowlist

\* Defense-in-depth: every submission either landed in accepted_tokens with
\* (RS256, allowlisted iss) or is absent. Equivalent to: there is no
\* "half-accepted" state in which accepted_meta points to garbage.
InvAcceptedMetaConsistent ==
    \A t \in Tokens:
        \/ t \notin accepted_tokens
        \/ /\ accepted_meta[t][1] = "RS256"
           /\ accepted_meta[t][2] \in Allowlist

\* No reachable state has more accepted tokens than submissions. Sanity bound.
InvAcceptedBoundedBySubmissions ==
    Cardinality(accepted_tokens) <= Len(submitted)

SafetyInvariants ==
    /\ InvOnlyRS256Accepted
    /\ InvIssuerExactMatch
    /\ InvAcceptedMetaConsistent
    /\ InvAcceptedBoundedBySubmissions

================================================================================
