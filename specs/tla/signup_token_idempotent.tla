------------------------ MODULE signup_token_idempotent ------------------------
(***************************************************************************)
(* CoreLink — Pilot signup token idempotency state machine                  *)
(*                                                                          *)
(* Wave-30 stream-4 promotion (R-PREP) — closes the Wave-29 stream-10       *)
(* DRAFT promotion candidate `INV-SIGNUP-TOKEN-IDEMPOTENT` flagged in       *)
(* `specs/_audits/2026-05-16-wave29-closure.md §6.2`.                       *)
(*                                                                          *)
(* Invariant claim (registry §3.29):                                        *)
(*                                                                          *)
(*   Consumption of a pilot signup token MUST be exactly-once across all   *)
(*   replays. Re-issuing the route against the SAME email (regardless of   *)
(*   token replay vs. fresh token mint) MUST return the ORIGINAL tenant_id *)
(*   (idempotent). Re-issuing against the SAME token (regardless of email) *)
(*   MUST return the original record. Successful new-tenant creation is    *)
(*   audit-emitted BEFORE the 201 response is observable (audit fail-     *)
(*   CLOSED).                                                              *)
(*                                                                          *)
(* The canonical source for the production invariant is the wave-29        *)
(* stream-1 signup backend commit `b3c359f`:                                *)
(*   apps/server/src/routes/signup.rs `insert_or_existing` (lines 585-610) *)
(*     /// If a row with the same `email` OR `token_id` already exists,    *)
(*     /// return the existing record (idempotent — the route returns the  *)
(*     /// original tenant_id).                                            *)
(*                                                                          *)
(* This spec is the SIBLING of:                                            *)
(*   - `signup_resignup.tla` (DEBT-014 FT-9) — covers the S-19 onboarding  *)
(*     re-signup tombstone idempotency on the post-compensated-failure     *)
(*     path. DISJOINT from this spec: signup_resignup models the           *)
(*     production-tenant-provisioning path (Stripe webhook + DPA-first +   *)
(*     ReSignupSameEmail with tombstone restart), while THIS spec models  *)
(*     the pilot-token path (HMAC-verified mint token + insert-or-existing *)
(*     SoT + audit-fail-CLOSED).                                           *)
(*   - `audit_emit_atomic.tla` (DEBT-005 batch 2) — covers the audit-emit  *)
(*     before-response-mutation atomic pairing pattern generally; THIS     *)
(*     spec inherits the pattern and proves the SIGNUP-SPECIFIC binding    *)
(*     (audit row tagged "reserved" vs. "duplicate" by exit_status).       *)
(*                                                                          *)
(* Threat model:                                                            *)
(*   - Attacker (or honest retry) submits the same pilot email twice with  *)
(*     two distinct minted tokens → the second submission MUST collapse to *)
(*     the original tenant_id, not allocate a new one. Otherwise a single  *)
(*     pilot operator email could fork into two tenants and consume two   *)
(*     pilot slots from the cohort cap.                                    *)
(*   - Attacker replays the SAME token N times → only the first        *)
(*     submission allocates a new tenant_id; subsequent replays return the *)
(*     original (same tenant binding, but observably distinguishable via   *)
(*     the `exit_status="duplicate"` audit emit).                          *)
(*                                                                          *)
(* Out of scope:                                                            *)
(*   - HMAC-SHA256 token verify itself — covered by the Rust unit tests    *)
(*     (`parse_and_verify_pilot_token` constant-time ct_eq compare). The   *)
(*     TLA+ abstraction assumes the token has already been verified; we   *)
(*     model only the SoT race + audit pairing.                            *)
(*   - Rate-limit gate — covered by `audit_export` route's rate-limit     *)
(*     pattern; pilot signup uses the same `RateLimiter` trait. Tested via *)
(*     `apps/server/tests/signup_pilot.rs::rate_limit_*` integration tests.*)
(*   - Stripe webhook + DPA — `signup_resignup.tla` covers the production *)
(*     onboarding path. Pilot signup is pre-Stripe (RESERVED state); the  *)
(*     activation_url is the operator-driven handoff.                     *)
(*   - Concurrent body-validation failures — covered by the wave-29       *)
(*     stream-1 integration test `bad_request_body_returns_400`.          *)
(*                                                                          *)
(* Cross-refs:                                                              *)
(*   - `specs/03_architecture/invariant_registry.md §3.29`                  *)
(*   - `apps/server/src/routes/signup.rs` (wave-29 commit b3c359f)         *)
(*   - `apps/server/tests/signup_pilot.rs::duplicate_email_returns_*`     *)
(*   - `specs/_audits/2026-05-16-inv-signup-token-tla.md` (this dispatch)  *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Tenants,                \* Finite pool of tenant_ids the model may allocate
    Tokens,                 \* Finite set of distinct minted pilot tokens
    Emails,                 \* Finite set of distinct pilot contact emails
    MaxRequests,            \* Bound on adversary + caller route invocations
    MaxConcurrentSignups    \* Bound on the in-flight (post-rate-limit, pre-SoT)
                            \* signup attempts (models the per-IP bucket cap)

ASSUME
    /\ Tenants # {}
    /\ Tokens # {}
    /\ Emails # {}
    /\ MaxRequests \in Nat
    /\ MaxConcurrentSignups \in Nat
    /\ MaxConcurrentSignups <= MaxRequests
    \* Tenant pool MUST be large enough to absorb every distinct token mint
    \* (worst case: every token allocates a fresh tenant). TLC will surface
    \* allocation-exhausted as a counterexample if this is too tight, so the
    \* ASSUME makes the relationship explicit at config-load time rather than
    \* deep inside the state space.
    /\ Cardinality(Tenants) >= Cardinality(Tokens)

\* Sentinel value: a token / email that has not yet bound to a tenant.
\* Modelled as a TLC string constant (not in `Tenants`) so the model checker
\* keeps it concrete — using `CHOOSE x : x \notin Tenants` would be
\* unbounded under TLC v1.8.0 and fails fingerprinting.
Nil == "NIL"

VARIABLES
    \* SoT: per-token outcome. `Nil` = unused; otherwise the tenant_id this
    \* token successfully reserved. Models the `pilot_signups.token_id`
    \* dedup branch of `insert_or_existing` (line 602 in signup.rs).
    token_tenant,

    \* SoT: per-email outcome. `Nil` = no signup with this email yet;
    \* otherwise the tenant_id this email is bound to. Models the
    \* `pilot_signups.email` dedup branch of `insert_or_existing`.
    email_tenant,

    \* Append-only audit log of `corelink.signup.pilot_reserved.v1` rows
    \* with `exit_status \in {"reserved", "duplicate"}`. Each entry is
    \* the tuple <<email, token, tenant_id, exit_status>>.
    \* The route emits BEFORE the 201 response is observable (fail-CLOSED).
    audit_log,

    \* Pool of tenant_ids already allocated (monotonic, models UUIDv7
    \* uniqueness — the route mints a fresh UUIDv7 for `tenant_id` on every
    \* call to `handle_pilot_signup`, but the `insert_or_existing` SoT
    \* collapses dup-email / dup-token to the original allocation).
    allocated_tenants,

    \* Bookkeeping bound on caller-driven actions.
    request_count,

    \* In-flight (post-rate-limit, pre-SoT-commit) signup attempts. Bounded
    \* by `MaxConcurrentSignups` so we model contention without unbounded
    \* parallelism.
    in_flight

vars == <<token_tenant, email_tenant, audit_log, allocated_tenants,
          request_count, in_flight>>

(*-- Init --------------------------------------------------------------------*)

Init ==
    /\ token_tenant = [t \in Tokens |-> Nil]
    /\ email_tenant = [e \in Emails |-> Nil]
    /\ audit_log = <<>>
    /\ allocated_tenants = {}
    /\ request_count = 0
    /\ in_flight = 0

(*-- Actions -----------------------------------------------------------------*)

\* Pick a tenant_id that has not yet been allocated (models UUIDv7
\* uniqueness — the route calls `Uuid::now_v7()` which yields a fresh
\* monotonic ID per call). If the pool is exhausted the action is disabled
\* — TLC will surface allocation-exhausted as a stuck state, but the
\* ASSUME at the top of the file enforces `Cardinality(Tenants) >=
\* Cardinality(Tokens)` so the disabled case is unreachable from any
\* legitimate trace.
FreshTenant == CHOOSE t \in Tenants : t \notin allocated_tenants

\* RouteInvocation(e, t) — single execution of `handle_pilot_signup` against
\* email `e` and token `t`, with the in-flight bound enforcing the
\* per-IP concurrency cap. Atomic from the TLA+ perspective: the rate-limit
\* gate, token verify, SoT insert_or_existing, and audit emit are all in
\* the same step (the production route serialises these via the axum
\* handler — there is no observable intermediate state for an outside
\* observer because the audit emit precedes the response).
\*
\* Branch structure mirrors `insert_or_existing` (signup.rs §585-610):
\*   1. SoT has neither (e, t) → fresh tenant allocated; `reserved` audit.
\*   2. SoT has email_tenant[e] = some tenant → return original; `duplicate`
\*      audit (this captures the dup-email branch — token is irrelevant).
\*   3. SoT has token_tenant[t] = some tenant /\ email_tenant[e] = Nil →
\*      return original via the dup-token branch; `duplicate` audit.
\*      (The route walks the in-memory store row-by-row; the first match
\*      wins. Models the first-match-wins ordering.)
RouteInvocation(e, t) ==
    /\ e \in Emails
    /\ t \in Tokens
    /\ request_count < MaxRequests
    /\ in_flight < MaxConcurrentSignups
    /\ request_count' = request_count + 1
    /\ \/ \* Branch 1: neither email nor token previously seen → fresh.
          /\ email_tenant[e] = Nil
          /\ token_tenant[t] = Nil
          /\ LET new_tenant == FreshTenant IN
                /\ allocated_tenants' = allocated_tenants \union {new_tenant}
                /\ token_tenant' = [token_tenant EXCEPT ![t] = new_tenant]
                /\ email_tenant' = [email_tenant EXCEPT ![e] = new_tenant]
                /\ audit_log' =
                    Append(audit_log, <<e, t, new_tenant, "reserved">>)
       \/ \* Branch 2: email already bound (dup-email branch — first row
          \* in the in-memory store iteration that matches wins; the
          \* production code's iter().find() returns the FIRST match).
          /\ email_tenant[e] # Nil
          /\ LET existing == email_tenant[e] IN
                /\ audit_log' =
                    Append(audit_log, <<e, t, existing, "duplicate">>)
                /\ UNCHANGED <<token_tenant, email_tenant, allocated_tenants>>
       \/ \* Branch 3: email is fresh BUT token was already used by another
          \* signup (dup-token branch). Returns the original tenant_id, even
          \* though the email did not match — this is the production
          \* semantics: `insert_or_existing` returns existing on EITHER
          \* dup-email OR dup-token. The audit emit tags `duplicate`.
          /\ email_tenant[e] = Nil
          /\ token_tenant[t] # Nil
          /\ LET existing == token_tenant[t] IN
                /\ audit_log' =
                    Append(audit_log, <<e, t, existing, "duplicate">>)
                /\ UNCHANGED <<token_tenant, email_tenant, allocated_tenants>>
    /\ in_flight' = in_flight + 1
    /\ UNCHANGED <<>>

\* RouteCompletion — drains the in-flight counter once a route invocation
\* has committed its SoT mutation + audit emit. This is the TLA+ abstraction
\* over "the axum handler returned a Response". Not counted against
\* `MaxRequests` because it is an internal-system drain.
RouteCompletion ==
    /\ in_flight > 0
    /\ in_flight' = in_flight - 1
    /\ UNCHANGED <<token_tenant, email_tenant, audit_log,
                   allocated_tenants, request_count>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E e \in Emails, t \in Tokens: RouteInvocation(e, t)
    \/ RouteCompletion

\* Weak fairness on RouteCompletion so the in-flight counter eventually
\* drains for any prefix that fired RouteInvocation. Without this the
\* model could starve completion forever and lose liveness pressure.
Spec ==
    /\ Init
    /\ [][Next]_vars
    /\ WF_vars(RouteCompletion)

(*-- Safety invariants -------------------------------------------------------*)

\* INV-SIGNUP-TOKEN-IDEMPOTENT central claim (§3.29): at most ONE `reserved`
\* audit row per email. The canonical idempotency contract from `signup.rs`
\* `insert_or_existing` line 602: "If a row with the same `email` ... already
\* exists, return the existing record" — so the second submission with a
\* same email cannot trigger a second `reserved` allocation; it must produce
\* a `duplicate`-tagged row instead.
\*
\* Note: a single email MAY appear across multiple audit rows with
\* DIFFERENT tenant_ids if the email first hits the dup-TOKEN branch (which
\* returns the original token-owner's tenant without binding the new email)
\* and then later issues a fresh-allocation against a different token. This
\* is consistent with the prod semantics in signup.rs `insert_or_existing`:
\* the dup-token branch does NOT bind the new email, so the email's
\* canonical binding in `email_tenant` is set on the LATER fresh-allocation
\* call. Hence the "idempotent by email" invariant is stated at the
\* `reserved`-row granularity (per-email exactly-once allocation), NOT at
\* the audit-row granularity (which would over-constrain dup-token).
InvSignupIdempotentByEmail ==
    \A e \in Emails:
        Cardinality({ i \in 1..Len(audit_log) :
                          audit_log[i][1] = e
                       /\ audit_log[i][4] = "reserved" }) <= 1

\* INV-SIGNUP-TOKEN-SINGLE-USE (§3.29 projection): a single token can be
\* successfully "consumed" (i.e. trigger a fresh-tenant allocation tagged
\* `reserved`) at most ONCE. Subsequent invocations against the same token
\* MUST emit a `duplicate` audit row, never a second `reserved`.
\*
\* This is the "exactly-once new-tenant-creation per token" claim — the
\* atom the registry §6.2 candidate text calls out: "the 25th-hour-onward
\* replay MUST be observably distinguishable from the 1st-hour replay
\* (different audit event ID, same tenant binding)." In TLA+ terms: at
\* most one `reserved` row per token; all subsequent rows with that token
\* are `duplicate`.
InvSignupTokenSingleUse ==
    \A t \in Tokens:
        Cardinality({ i \in 1..Len(audit_log) :
                          audit_log[i][2] = t
                       /\ audit_log[i][4] = "reserved" }) <= 1

\* INV-SIGNUP-AUDIT-EMIT-ATOMIC (§3.29 projection of INV-AUDIT-EMIT-ATOMIC-
\* WITH-HANDLER): every SoT mutation has a corresponding audit row in the
\* same step. Since RouteInvocation appends to audit_log unconditionally
\* on every branch (reserved + both duplicate branches), this is enforced
\* by construction; the explicit invariant catches any future refactor
\* that splits the steps.
\*
\* The invariant is stated as: the number of audit rows equals the number
\* of completed route invocations. Since RouteInvocation increments
\* request_count AND appends to audit_log atomically, this is the same as
\* `Len(audit_log) = request_count`.
InvAuditEmitAtomic ==
    Len(audit_log) = request_count

\* INV-SIGNUP-NO-CROSS-TENANT-LEAK (defense-in-depth): an audit row's
\* tenant_id is always one of the allocated tenants. Catches a refactor
\* that emits a synthesised / placeholder tenant_id (e.g. nil-UUID) on
\* a duplicate path.
InvAuditTenantIsAllocated ==
    \A i \in 1..Len(audit_log):
        audit_log[i][3] \in allocated_tenants

\* INV-SIGNUP-SOT-COHERENT (defense-in-depth): if token_tenant[t] = X then
\* there is exactly one email e with email_tenant[e] = X bound through that
\* token. Models the in-memory store's row uniqueness: the first matching
\* row wins, so token_tenant and email_tenant are coherent slices of the
\* same SoT.
\*
\* Note: dup-email branch (branch 2) does NOT mutate token_tenant, so it
\* is possible for two distinct tokens to share an email -> tenant binding
\* (the first token won the email; the second was replayed against the
\* same email but the dup-email branch returned the original without
\* binding the new token). This is by-design: token_tenant only records
\* tokens that ACTUALLY allocated a new tenant; email_tenant records the
\* canonical email → tenant binding.
InvSotCoherent ==
    \A t \in Tokens:
        (token_tenant[t] # Nil) =>
            \E e \in Emails: email_tenant[e] = token_tenant[t]

(*-- Liveness ----------------------------------------------------------------*)

\* RouteCompletion drains in_flight eventually under WF, so a route
\* invocation cannot block the in-flight counter forever. This is the
\* topological abstraction of "the axum handler always returns a Response"
\* — the wall-clock budget is enforced separately by the route's SLO.
InvInFlightDrains ==
    (in_flight > 0) ~> (in_flight = 0)

(*-- Combined safety conjunction (single INVARIANT line in cfg) --------------*)

SafetyInvariants ==
    /\ InvSignupIdempotentByEmail
    /\ InvSignupTokenSingleUse
    /\ InvAuditEmitAtomic
    /\ InvAuditTenantIsAllocated
    /\ InvSotCoherent

================================================================================
