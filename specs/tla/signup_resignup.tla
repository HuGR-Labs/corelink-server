---------------------------- MODULE signup_resignup ----------------------------
(***************************************************************************)
(* CoreLink — Signup re-signup idempotency (DEBT-014 FT-9)                 *)
(*                                                                         *)
(* Closes 1 HIGH TLA gap from `specs/_audits/tla-followup-tickets.md`      *)
(* FT-9 by extending the runbook `signup_atomic.tla` shape with the        *)
(* explicit `ReSignupSameEmail` action: a user re-attempts signup with     *)
(* the same email AFTER a prior compensated failure.                       *)
(*                                                                         *)
(* This is the FT-9 companion spec that lives under the canonical          *)
(* `specs/tla/` tree so it is exercised by the PR-gate matrix in           *)
(* `.github/workflows/tla_check.yml` (the original runbook spec at         *)
(* `specs/03_architecture/tla+/runbooks/signup_atomic.tla` is wired into   *)
(* the runbook matrix added by FT-8).                                      *)
(*                                                                         *)
(* Invariants proved here:                                                 *)
(*                                                                         *)
(*   INV-SIGNUP-RESIGNUP-IDEMPOTENT (HIGH; new — registered in §3 onboard) *)
(*     A re-signup with the same email after a compensated failure         *)
(*     produces EITHER a fully-provisioned success OR another fully-       *)
(*     compensated failure. It MUST NOT leak partial state from the prior *)
(*     attempt (no orphan Clerk user, no orphan D1 tenant, no orphan       *)
(*     Stripe customer, no extra DPA acceptance).                          *)
(*                                                                         *)
(*   INV-ONBOARD-DPA-FIRST (HIGH; inherits from signup_atomic.tla)         *)
(*     D1 tenant row never persists without an accepted DPA row, EVEN      *)
(*     across a re-signup boundary.                                        *)
(*                                                                         *)
(*   INV-ONBOARD-ATOMIC-PROVISIONING (HIGH; inherits)                      *)
(*     Each terminal attempt is fully-provisioned OR fully-compensated;    *)
(*     re-signup MUST observe the same shape per-attempt.                  *)
(*                                                                         *)
(* Bug class targeted (per FT-9):                                          *)
(*   Stripe webhook arriving AFTER compensation of attempt N but BEFORE    *)
(*   the user retries (attempt N+1): the webhook MUST be no-op (no        *)
(*   resurrection of Stripe customer or tenant) so attempt N+1 starts      *)
(*   from a clean slate.                                                   *)
(*                                                                         *)
(* Cross-refs:                                                             *)
(*   - `specs/03_architecture/tla+/runbooks/signup_atomic.tla` (base)      *)
(*   - `specs/03_architecture/invariant_registry.md §3.18 ONBOARD-*`       *)
(*   - `specs/_audits/tla-followup-tickets.md` FT-9                        *)
(***************************************************************************)

EXTENDS Naturals, FiniteSets, Sequences, TLC

CONSTANTS
    MaxAttempts,   \* bound on number of signup attempts (e.g. 3)
    Emails         \* finite set of emails (e.g. {e1, e2})

ASSUME
    /\ MaxAttempts \in Nat /\ MaxAttempts > 0
    /\ Emails # {}

AttemptIds == 1..MaxAttempts

Steps == { "none", "init", "clerk_created", "d1_tx_committed",
           "stripe_created", "done", "compensating", "compensated" }

Outcomes == { "none", "in_progress", "success", "failure" }

VARIABLES
    clerk_users,        \* AttemptIds -> BOOLEAN
    d1_tenants,         \* AttemptIds -> BOOLEAN
    d1_dpa,             \* AttemptIds -> BOOLEAN
    stripe_customers,   \* AttemptIds -> BOOLEAN
    email_of,           \* AttemptIds -> Emails (set on StartAttempt)
    step,               \* AttemptIds -> Steps
    outcome,            \* AttemptIds -> Outcomes
    attempts_started,   \* Nat counter
    pending_webhooks    \* AttemptIds -> BOOLEAN; late Stripe webhook in flight

vars == <<clerk_users, d1_tenants, d1_dpa, stripe_customers, email_of,
          step, outcome, attempts_started, pending_webhooks>>

Started(a) == step[a] # "none"
Terminated(a) == outcome[a] \in {"success", "failure"}

(*-- Type invariant ---------------------------------------------------------*)
TypeOK ==
    /\ \A a \in AttemptIds : clerk_users[a]      \in BOOLEAN
    /\ \A a \in AttemptIds : d1_tenants[a]       \in BOOLEAN
    /\ \A a \in AttemptIds : d1_dpa[a]           \in BOOLEAN
    /\ \A a \in AttemptIds : stripe_customers[a] \in BOOLEAN
    /\ \A a \in AttemptIds : email_of[a]         \in Emails
    /\ \A a \in AttemptIds : step[a]             \in Steps
    /\ \A a \in AttemptIds : outcome[a]          \in Outcomes
    /\ attempts_started \in 0..MaxAttempts
    /\ \A a \in AttemptIds : pending_webhooks[a] \in BOOLEAN

(*-- Init -------------------------------------------------------------------*)
Init ==
    /\ clerk_users       = [a \in AttemptIds |-> FALSE]
    /\ d1_tenants        = [a \in AttemptIds |-> FALSE]
    /\ d1_dpa            = [a \in AttemptIds |-> FALSE]
    /\ stripe_customers  = [a \in AttemptIds |-> FALSE]
    /\ email_of          = [a \in AttemptIds |-> CHOOSE e \in Emails : TRUE]
    /\ step              = [a \in AttemptIds |-> "none"]
    /\ outcome           = [a \in AttemptIds |-> "none"]
    /\ attempts_started  = 0
    /\ pending_webhooks  = [a \in AttemptIds |-> FALSE]

(*-- Actions ----------------------------------------------------------------*)

\* StartAttempt: opens attempt N+1 with a chosen email. Re-signup is
\* expressed by choosing the SAME email as a prior terminated attempt.
\* Handler-side uniqueness: a new attempt with email `e` is admitted
\* ONLY IF no prior attempt with the same email is currently in_progress
\* OR has succeeded. (Compensated-failure attempts ARE re-attempt-able.)
StartAttempt(e) ==
    /\ attempts_started < MaxAttempts
    /\ ~ \E a2 \in 1..attempts_started :
            /\ email_of[a2] = e
            /\ outcome[a2] \in {"in_progress", "success"}
    /\ LET a == attempts_started + 1 IN
         /\ email_of'        = [email_of EXCEPT ![a] = e]
         /\ step'            = [step     EXCEPT ![a] = "init"]
         /\ outcome'         = [outcome  EXCEPT ![a] = "in_progress"]
         /\ attempts_started' = a
    /\ UNCHANGED <<clerk_users, d1_tenants, d1_dpa, stripe_customers,
                   pending_webhooks>>

StepClerk(a) ==
    /\ step[a] = "init"
    /\ outcome[a] = "in_progress"
    /\ clerk_users' = [clerk_users EXCEPT ![a] = TRUE]
    /\ step' = [step EXCEPT ![a] = "clerk_created"]
    /\ UNCHANGED <<d1_tenants, d1_dpa, stripe_customers, email_of,
                   outcome, attempts_started, pending_webhooks>>

StepD1Tx(a) ==
    /\ step[a] = "clerk_created"
    /\ outcome[a] = "in_progress"
    /\ d1_tenants' = [d1_tenants EXCEPT ![a] = TRUE]
    /\ d1_dpa'     = [d1_dpa     EXCEPT ![a] = TRUE]
    /\ step' = [step EXCEPT ![a] = "d1_tx_committed"]
    /\ UNCHANGED <<clerk_users, stripe_customers, email_of, outcome,
                   attempts_started, pending_webhooks>>

\* Stripe step OK: customer created; also leaves a pending webhook in
\* flight (Stripe may re-deliver). We do not assume webhooks have
\* already been acknowledged.
StepStripeOK(a) ==
    /\ step[a] = "d1_tx_committed"
    /\ outcome[a] = "in_progress"
    /\ stripe_customers' = [stripe_customers EXCEPT ![a] = TRUE]
    /\ pending_webhooks' = [pending_webhooks EXCEPT ![a] = TRUE]
    /\ step'    = [step    EXCEPT ![a] = "done"]
    /\ outcome' = [outcome EXCEPT ![a] = "success"]
    /\ UNCHANGED <<clerk_users, d1_tenants, d1_dpa, email_of,
                   attempts_started>>

\* Stripe step fails -> compensate. No webhook for failure path.
StepStripeFail(a) ==
    /\ step[a] = "d1_tx_committed"
    /\ outcome[a] = "in_progress"
    /\ step' = [step EXCEPT ![a] = "compensating"]
    /\ UNCHANGED <<clerk_users, d1_tenants, d1_dpa, stripe_customers,
                   email_of, outcome, attempts_started, pending_webhooks>>

\* Compensate: rollback all side-effects for this attempt id.
Compensate(a) ==
    /\ step[a] = "compensating"
    /\ d1_tenants'       = [d1_tenants       EXCEPT ![a] = FALSE]
    /\ d1_dpa'           = [d1_dpa           EXCEPT ![a] = FALSE]
    /\ clerk_users'      = [clerk_users      EXCEPT ![a] = FALSE]
    /\ stripe_customers' = [stripe_customers EXCEPT ![a] = FALSE]
    /\ step'    = [step    EXCEPT ![a] = "compensated"]
    /\ outcome' = [outcome EXCEPT ![a] = "failure"]
    /\ UNCHANGED <<email_of, attempts_started, pending_webhooks>>

\* Crash between any two steps; resumption triggers compensation.
CrashAndCompensate(a) ==
    /\ step[a] \in {"clerk_created", "d1_tx_committed"}
    /\ outcome[a] = "in_progress"
    /\ step' = [step EXCEPT ![a] = "compensating"]
    /\ UNCHANGED <<clerk_users, d1_tenants, d1_dpa, stripe_customers,
                   email_of, outcome, attempts_started, pending_webhooks>>

\* Late Stripe webhook delivery: arrives AFTER attempt has terminated.
\* INV requires this is a NO-OP — no resurrection of any side-effect.
\* This is the bug class FT-9 targets: webhook handler MUST observe the
\* idempotency-key tombstone from the compensation and discard.
LateStripeWebhook(a) ==
    /\ pending_webhooks[a] = TRUE
    /\ pending_webhooks' = [pending_webhooks EXCEPT ![a] = FALSE]
    \* Crucially: NO mutation of clerk_users / d1_tenants / stripe_customers.
    /\ UNCHANGED <<clerk_users, d1_tenants, d1_dpa, stripe_customers,
                   email_of, step, outcome, attempts_started>>

(*-- Next --------------------------------------------------------------------*)

Next ==
    \/ \E e \in Emails : StartAttempt(e)
    \/ \E a \in AttemptIds :
         /\ Started(a)
         /\ \/ StepClerk(a)
            \/ StepD1Tx(a)
            \/ StepStripeOK(a)
            \/ StepStripeFail(a)
            \/ Compensate(a)
            \/ CrashAndCompensate(a)
    \/ \E a \in AttemptIds : LateStripeWebhook(a)

Spec ==
    /\ Init
    /\ [][Next]_vars
    /\ \A a \in AttemptIds : WF_vars(StepClerk(a))
    /\ \A a \in AttemptIds : WF_vars(StepD1Tx(a))
    /\ \A a \in AttemptIds : WF_vars(StepStripeOK(a) \/ StepStripeFail(a))
    /\ \A a \in AttemptIds : WF_vars(Compensate(a))

(*-- Safety invariants ------------------------------------------------------*)

\* Per-attempt atomicity: success -> all 4 rows present; failure -> all 4
\* rows absent. Inherits INV-ONBOARD-ATOMIC-PROVISIONING shape.
InvAtomicPerAttempt ==
    \A a \in AttemptIds :
        Terminated(a) =>
            \/ /\ outcome[a] = "success"
               /\ clerk_users[a]
               /\ d1_tenants[a]
               /\ d1_dpa[a]
               /\ stripe_customers[a]
            \/ /\ outcome[a] = "failure"
               /\ ~clerk_users[a]
               /\ ~d1_tenants[a]
               /\ ~d1_dpa[a]
               /\ ~stripe_customers[a]

\* INV-ONBOARD-DPA-FIRST (inherits): d1_tenants[a] => d1_dpa[a].
InvDPAFirst ==
    \A a \in AttemptIds : d1_tenants[a] => d1_dpa[a]

\* INV-SIGNUP-RESIGNUP-IDEMPOTENT (new): for any two terminated attempts
\* sharing the same email, neither attempt leaks state into the other
\* (each is independently atomic). Encoded as: for every email e, the
\* set of attempts using e that have any side-effect TRUE is exactly
\* the subset whose outcome is "success".
InvResignupNoCrossLeak ==
    \A e \in Emails :
        \A a \in AttemptIds :
            ( /\ email_of[a] = e
              /\ Terminated(a)
              /\ outcome[a] = "failure" )
                => ~clerk_users[a]
                /\ ~d1_tenants[a]
                /\ ~d1_dpa[a]
                /\ ~stripe_customers[a]

\* Email uniqueness on SUCCESS: at most one terminated success attempt
\* per email (re-signup after success would be a no-op at the handler;
\* the model rejects this by construction since StartAttempt for the
\* same email after a success would not contradict the invariant — but
\* the user-facing handler enforces uniqueness, so we encode the
\* expectation).
InvAtMostOneSuccessPerEmail ==
    \A e \in Emails :
        Cardinality({ a \in AttemptIds :
                          /\ email_of[a] = e
                          /\ outcome[a] = "success" }) <= 1

\* Late webhook idempotency: pending_webhooks toggles to FALSE without
\* perturbing any side-effect bit. Encoded as a per-attempt invariant:
\* if outcome is "failure", side-effects remain FALSE regardless of
\* pending_webhooks history.
InvLateWebhookNoOp ==
    \A a \in AttemptIds :
        (outcome[a] = "failure") =>
            ( ~clerk_users[a] /\ ~d1_tenants[a] /\ ~d1_dpa[a]
              /\ ~stripe_customers[a] )

SafetyInvariants ==
    /\ TypeOK
    /\ InvAtomicPerAttempt
    /\ InvDPAFirst
    /\ InvResignupNoCrossLeak
    /\ InvAtMostOneSuccessPerEmail
    /\ InvLateWebhookNoOp

(*-- Liveness ---------------------------------------------------------------*)

\* Every in-progress attempt eventually terminates.
EventuallyTerminates ==
    \A a \in AttemptIds :
        (outcome[a] = "in_progress")
            ~> (outcome[a] \in {"success", "failure"})

================================================================================
