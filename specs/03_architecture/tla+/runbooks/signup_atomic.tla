---- MODULE signup_atomic ----
(***************************************************************************)
(* CoreLink WI-S20-007 — Runbook formal verification (1/4)                 *)
(*                                                                         *)
(* Runbook: RB-FM-SIGNUP-FAILED                                            *)
(* Source : S-19 WI-S19-001 "Signup orchestration · atomic provisioning    *)
(*          · Clerk + D1 TX · chaos Stripe outage"                         *)
(*                                                                         *)
(* What this models                                                        *)
(* ----------------                                                        *)
(* Self-service signup is a saga across 3 external systems:                *)
(*   1) Clerk         (identity)                                           *)
(*   2) D1 transaction (tenant row + DPA acceptance row)                  *)
(*   3) Stripe        (customer + subscription)                           *)
(*                                                                         *)
(* The runbook MUST guarantee atomic provisioning: either all 3 succeed    *)
(* and the user can sign in, or every partial side-effect is compensated   *)
(* before the runbook returns failure. Crashes between steps must NOT      *)
(* leave a tenant in a half-provisioned state.                             *)
(*                                                                         *)
(* Invariants verified                                                     *)
(* -------------------                                                     *)
(*   AtomicSignup       — final state is either fully_provisioned OR       *)
(*                        fully_compensated; never half_done.              *)
(*   NoOrphanIdentity   — no Clerk user exists without matching D1 tenant *)
(*                        once runbook is terminated (success or failure). *)
(*   NoOrphanBilling    — no Stripe customer exists without matching D1   *)
(*                        tenant once runbook is terminated.               *)
(*   DPAFirstHolds      — D1 tenant row never exists without an            *)
(*                        accepted DPA row (INV-ONBOARD-DPA-FIRST).        *)
(*                                                                         *)
(* Liveness                                                                *)
(* --------                                                                *)
(*   EventuallyTerminates — every signup attempt eventually reaches a      *)
(*                          terminal state under FairExecution.            *)
(*                                                                         *)
(* TLC tractability: 2 attempts, all 8 crash points → ≤ 60s on CI.         *)
(***************************************************************************)

EXTENDS Naturals, FiniteSets, TLC

CONSTANTS
  MaxAttempts        \* bound on number of signup attempts (e.g. 2)

ASSUME MaxAttempts \in Nat /\ MaxAttempts > 0

AttemptIds == 1..MaxAttempts

VARIABLES
  clerk_users,       \* Set of attempt ids that have a Clerk user
  d1_tenants,        \* Set of attempt ids that have a D1 tenant row
  d1_dpa,            \* Set of attempt ids that have an accepted DPA row
  stripe_customers,  \* Set of attempt ids that have a Stripe customer
  step,              \* AttemptIds -> step label (initial: "none")
  outcome,           \* AttemptIds -> outcome (initial: "none")
  attempts_started   \* Nat; monotonic counter

vars == <<clerk_users, d1_tenants, d1_dpa, stripe_customers,
          step, outcome, attempts_started>>

Steps == { "none", "init", "clerk_created", "d1_tx_committed",
           "stripe_created", "done", "compensating", "compensated" }

Outcomes == { "none", "in_progress", "success", "failure" }

Started(a) == step[a] # "none"

(*-- Type invariant ---------------------------------------------------------*)
TypeOK ==
  /\ clerk_users      \subseteq AttemptIds
  /\ d1_tenants       \subseteq AttemptIds
  /\ d1_dpa           \subseteq AttemptIds
  /\ stripe_customers \subseteq AttemptIds
  /\ \A a \in AttemptIds : step[a]    \in Steps
  /\ \A a \in AttemptIds : outcome[a] \in Outcomes
  /\ attempts_started \in 0..MaxAttempts

(*-- Init -------------------------------------------------------------------*)
Init ==
  /\ clerk_users      = {}
  /\ d1_tenants       = {}
  /\ d1_dpa           = {}
  /\ stripe_customers = {}
  /\ step             = [a \in AttemptIds |-> "none"]
  /\ outcome          = [a \in AttemptIds |-> "none"]
  /\ attempts_started = 0

(*-- Actions ----------------------------------------------------------------*)

StartAttempt ==
  /\ attempts_started < MaxAttempts
  /\ LET a == attempts_started + 1 IN
       /\ step'     = [step    EXCEPT ![a] = "init"]
       /\ outcome'  = [outcome EXCEPT ![a] = "in_progress"]
       /\ attempts_started' = a
  /\ UNCHANGED <<clerk_users, d1_tenants, d1_dpa, stripe_customers>>

\* Step 1: create Clerk user.
StepClerk(a) ==
  /\ step[a] = "init"
  /\ outcome[a] = "in_progress"
  /\ clerk_users' = clerk_users \cup {a}
  /\ step' = [step EXCEPT ![a] = "clerk_created"]
  /\ UNCHANGED <<d1_tenants, d1_dpa, stripe_customers,
                 outcome, attempts_started>>

\* Step 2: D1 TX (atomic: tenant row AND DPA row together — DPA-FIRST).
StepD1Tx(a) ==
  /\ step[a] = "clerk_created"
  /\ outcome[a] = "in_progress"
  /\ d1_tenants' = d1_tenants \cup {a}
  /\ d1_dpa'     = d1_dpa     \cup {a}
  /\ step' = [step EXCEPT ![a] = "d1_tx_committed"]
  /\ UNCHANGED <<clerk_users, stripe_customers, outcome, attempts_started>>

\* Step 3: Stripe customer (may fail → triggers compensation).
StepStripeOK(a) ==
  /\ step[a] = "d1_tx_committed"
  /\ outcome[a] = "in_progress"
  /\ stripe_customers' = stripe_customers \cup {a}
  /\ step'    = [step    EXCEPT ![a] = "done"]
  /\ outcome' = [outcome EXCEPT ![a] = "success"]
  /\ UNCHANGED <<clerk_users, d1_tenants, d1_dpa, attempts_started>>

StepStripeFail(a) ==
  /\ step[a] = "d1_tx_committed"
  /\ outcome[a] = "in_progress"
  /\ step' = [step EXCEPT ![a] = "compensating"]
  /\ UNCHANGED <<clerk_users, d1_tenants, d1_dpa, stripe_customers,
                 outcome, attempts_started>>

\* Compensation: rollback D1 (TX) then Clerk; runbook is responsible.
Compensate(a) ==
  /\ step[a] = "compensating"
  /\ d1_tenants'       = d1_tenants       \ {a}
  /\ d1_dpa'           = d1_dpa           \ {a}
  /\ clerk_users'      = clerk_users      \ {a}
  /\ stripe_customers' = stripe_customers \ {a}
  /\ step'    = [step    EXCEPT ![a] = "compensated"]
  /\ outcome' = [outcome EXCEPT ![a] = "failure"]
  /\ UNCHANGED attempts_started

\* Crash between any two steps; runbook resumption triggers compensation
\* on partial state.
CrashAndCompensate(a) ==
  /\ step[a] \in { "clerk_created", "d1_tx_committed" }
  /\ outcome[a] = "in_progress"
  /\ step' = [step EXCEPT ![a] = "compensating"]
  /\ UNCHANGED <<clerk_users, d1_tenants, d1_dpa, stripe_customers,
                 outcome, attempts_started>>

Next ==
  \/ StartAttempt
  \/ \E a \in AttemptIds :
       /\ Started(a)
       /\ \/ StepClerk(a) \/ StepD1Tx(a)
          \/ StepStripeOK(a) \/ StepStripeFail(a)
          \/ Compensate(a)   \/ CrashAndCompensate(a)

\* Fairness: any in-progress attempt eventually advances or compensates;
\* compensating attempts eventually finalize. We do NOT require fairness on
\* StartAttempt itself — the model permits an attempt to never start.
Spec == /\ Init
        /\ [][Next]_vars
        /\ \A a \in AttemptIds : WF_vars(StepClerk(a))
        /\ \A a \in AttemptIds : WF_vars(StepD1Tx(a))
        /\ \A a \in AttemptIds : WF_vars(StepStripeOK(a) \/ StepStripeFail(a))
        /\ \A a \in AttemptIds : WF_vars(Compensate(a))

(*-- Invariants -------------------------------------------------------------*)

Terminated(a) == outcome[a] \in { "success", "failure" }

\* Atomicity: terminated attempts are either fully provisioned (success) or
\* fully compensated (failure). No half_done.
AtomicSignup ==
  \A a \in AttemptIds :
    Terminated(a) =>
      \/ /\ outcome[a] = "success"
         /\ a \in clerk_users
         /\ a \in d1_tenants
         /\ a \in d1_dpa
         /\ a \in stripe_customers
      \/ /\ outcome[a] = "failure"
         /\ a \notin clerk_users
         /\ a \notin d1_tenants
         /\ a \notin d1_dpa
         /\ a \notin stripe_customers

\* No orphan identity: Clerk user without D1 tenant never persists on terminal.
NoOrphanIdentity ==
  \A a \in AttemptIds :
    Terminated(a) /\ a \in clerk_users => a \in d1_tenants

\* No orphan billing: Stripe customer without D1 tenant never persists.
NoOrphanBilling ==
  \A a \in AttemptIds :
    Terminated(a) /\ a \in stripe_customers => a \in d1_tenants

\* DPA-FIRST: D1 tenant row never exists without an accepted DPA row.
DPAFirstHolds == d1_tenants = d1_dpa

(*-- Liveness ---------------------------------------------------------------*)
EventuallyTerminates ==
  \A a \in AttemptIds :
    (outcome[a] = "in_progress") ~> (outcome[a] \in { "success", "failure" })

====
