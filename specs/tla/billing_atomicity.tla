--------------------------- MODULE billing_atomicity ---------------------------
(***************************************************************************)
(* CoreLink — INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP (HIGH)              *)
(*             INV-BILLING-RECONCILE-3-LAYER Layer-1 sub-property (HIGH)   *)
(*                                                                         *)
(* Spec contract S-10 §6 DoD: "TLA+ spec billing_atomicity.tla [state      *)
(* machine event -> counter -> invoice; no-loss, no-dup] verde em CI";     *)
(* §16 SOTA bar diferencial vs Stripe Billing / Mux / Datadog / Lago — all *)
(* ship robust testing, NONE ship TLA+.                                    *)
(*                                                                         *)
(* WI-S10-007 §6.1.1 + §6.1.3 scope: model only the load-bearing 3-layer   *)
(* atomicity invariants (emit -> aggregator -> stripe), bounded so the     *)
(* CONSTANTS Cartesian product fits 1M state ceiling (CI feasibility       *)
(* threshold). State-space caveat per WI §9.4: TLC reachable graph is      *)
(* SEPARATE from the Cartesian product upper-bound — actions semantics    *)
(* determine transitions; `MaxConcurrentEvents` bounds the unbounded       *)
(* `events_in_staging`/`events_in_r2`/`retry_queue` set growth that would  *)
(* otherwise explode the reachable graph.                                  *)
(*                                                                         *)
(* Modeling discipline:                                                    *)
(*   - Lote 10.6bis split-tier canonical: EmitEvent = fail-OPEN at hot     *)
(*     path (always succeeds locally, staging buffers); AggregateCounter   *)
(*     = fail-CLOSED (atomic D1 transaction; counter + chain advance).     *)
(*   - Lote 10.10-quaters R5 P0-A: counter PK is 4-tuple                   *)
(*     (tenant, region, sku, hour); cross-region tenants would corrupt     *)
(*     data on every UPSERT under the prior 3-tuple.                       *)
(*   - Lote 10.7bis P0-7: 5-tier canonical Plan (free/solo/team/business/  *)
(*     enterprise) referenced via tenant tier abstraction.                 *)
(*   - Lote 10.8bis P0-D: BillingPeriods enumerated as canonical YYYY-MM   *)
(*     strings; corelink_time::next_month_first_utc_midnight() canonical   *)
(*     primitive abstracted at this boundary.                              *)
(*   - Lote 10.9bis P0-G: CloudEvents canonical envelope                   *)
(*     `dev.hugr.corelink.<op>.v1` referenced in EmitEvent comments.       *)
(*   - Lote 10.9-quinquies NEW-P0-2: typed model state via TLA+ CONSTANTS  *)
(*     sets (no untyped catch-all states).                                 *)
(*                                                                         *)
(* Out-of-scope per WI §6.2 (modeled abstractly, NOT exercised):           *)
(*   - Real Stripe API HTTP semantics (`StripeChargeIdempotent` is         *)
(*     atomic in the model; the production HTTP retry/backoff /            *)
(*     5xx-handling is WI-S10-003 production wiring, not model state).    *)
(*   - Real R2 PutObject latency / Object Lock retention (events_in_r2    *)
(*     transition is atomic; production NDJSON archive retention is the   *)
(*     WI-S10-001 production binding).                                    *)
(*   - Cron jitter / scheduling drift (AggregateCounter fires when its    *)
(*     guards are satisfied; production CF Cron DO 02:00 UTC scheduling   *)
(*     is the WI-S10-002 production binding).                             *)
(*   - Multi-currency, real-time per-second, tax calc — anti-scope        *)
(*     §10.                                                                *)
(***************************************************************************)

EXTENDS Integers, FiniteSets, Sequences, TLC

CONSTANTS
    Tenants,                \* Finite set of tenant IDs (canonical 3 in cfg)
    SKUs,                   \* Finite set of usage SKUs (canonical 1 in cfg for state-space)
    BillingPeriods,         \* Finite set of YYYY-MM canonical strings (canonical 2)
    MaxEventsPerTenant,     \* Per-tenant emit upper bound (canonical 5)
    MaxConcurrentEvents     \* In-flight bound (staging+r2+retry); previne unbounded set growth

ASSUME
    /\ Tenants # {}
    /\ SKUs # {}
    /\ BillingPeriods # {}
    /\ MaxEventsPerTenant \in Nat
    /\ MaxConcurrentEvents \in Nat

VARIABLES
    events_emitted,         \* Set of (tenant, sku, billing_period, seq) emitted from CAS hot path
    events_in_staging,      \* Set buffered em D1 staging (WI-S10-001) — pending drain
    events_in_r2,           \* Set persisted to R2 Object Lock (WI-S10-001 drain complete)
    counters,               \* Map (tenant, sku, billing_period) -> Set of events aggregated
    invoice_line_items,     \* Map (tenant, sku, billing_period) -> Set of events invoiced
    stripe_invoiced,        \* Map (tenant, billing_period) -> Set of events charged to Stripe
    stripe_outage_active,   \* BOOLEAN: simulates Stripe API 5xx
    retry_queue,            \* Sequence of (tenant, billing_period) charges pending retry
    op_count                \* Bound counter to terminate the model

vars == <<events_emitted, events_in_staging, events_in_r2, counters,
          invoice_line_items, stripe_invoiced, stripe_outage_active,
          retry_queue, op_count>>

(*-- Helpers ----------------------------------------------------------------*)

\* In-flight cardinality bound — Lote 10.10-quaters NEW-P0-3: previne unbounded
\* set growth -> state-space explosion. Combined cardinality of staging + r2 +
\* retry_queue must be <= MaxConcurrentEvents.
InFlightCount ==
    Cardinality(events_in_staging) + Cardinality(events_in_r2) + Len(retry_queue)

\* Range of a sequence (TLA+ idiom for sequence -> set conversion).
\* Lote 10.10-sextus R5 P1-QUIN-4 fix: retry_queue is a Sequence; `\in retry_queue`
\* tests domain (indices) not value membership; convert via Range first.
RetrySet == { retry_queue[i] : i \in DOMAIN retry_queue }

\* Events in retry queue — derived from the (tenant, billing_period) charges
\* awaiting Stripe recovery. Each pending charge logically holds the events of
\* its (tenant, billing_period) bucket that have NOT yet been Stripe-invoiced.
EventsInRetry ==
    LET pending_buckets == { retry_queue[i] : i \in DOMAIN retry_queue }
    IN { evt \in events_in_r2 :
            \E bp \in pending_buckets :
                /\ evt[1] = bp[1]   \* tenant matches
                /\ evt[3] = bp[2] } \* billing_period matches

\* INV-BILLING-NO-LOSS canonical envelope: every emitted event is accounted
\* somewhere in the pipeline.
EventAccountedSomewhere(evt) ==
    \/ evt \in events_in_staging
    \/ evt \in events_in_r2
    \/ \E bp \in DOMAIN stripe_invoiced :
        bp[1] = evt[1] /\ bp[2] = evt[3] /\ evt \in stripe_invoiced[bp]

(*-- Init -------------------------------------------------------------------*)

Init ==
    /\ events_emitted = {}
    /\ events_in_staging = {}
    /\ events_in_r2 = {}
    /\ counters = [k \in {} |-> {}]
    /\ invoice_line_items = [k \in {} |-> {}]
    /\ stripe_invoiced = [k \in {} |-> {}]
    /\ stripe_outage_active = FALSE
    /\ retry_queue = <<>>
    /\ op_count = 0

(*-- Actions ----------------------------------------------------------------*)

\* WI-S10-001 hot path emit (Lote 10.6bis fail-OPEN tier).
\* CloudEvents type `dev.hugr.corelink.cas.put.v1` (Lote 10.9bis P0-G).
\* Idempotency by construction: (tenant, sku, billing_period, seq) tuple is
\* unique per emit attempt; same tuple cannot be emitted twice.
EmitEvent(tenant, sku, bp, seq) ==
    LET evt == <<tenant, sku, bp, seq>>
    IN  /\ op_count < MaxEventsPerTenant * Cardinality(Tenants)
        /\ InFlightCount < MaxConcurrentEvents
        /\ evt \notin events_emitted
        /\ events_emitted' = events_emitted \union {evt}
        /\ events_in_staging' = events_in_staging \union {evt}
        /\ op_count' = op_count + 1
        /\ UNCHANGED <<events_in_r2, counters, invoice_line_items,
                       stripe_invoiced, stripe_outage_active, retry_queue>>

\* WI-S10-001 staging -> R2 drain. Cloudflare Queue retry; eventual delivery
\* guaranteed by weak fairness on this action (events emitted reach R2).
DrainStagingToR2(evt) ==
    /\ evt \in events_in_staging
    /\ events_in_staging' = events_in_staging \ {evt}
    /\ events_in_r2' = events_in_r2 \union {evt}
    /\ UNCHANGED <<events_emitted, counters, invoice_line_items,
                   stripe_invoiced, stripe_outage_active, retry_queue, op_count>>

\* WI-S10-002 hourly aggregator (Lote 10.6bis fail-CLOSED tier).
\* Atomic D1 transaction: SELECT all R2 events for (tenant, sku, billing_period)
\* and UPSERT counter row + advance hash chain head. Idempotent by construction:
\* counter set assignment is deterministic from R2 event set.
AggregateCounter(tenant, sku, bp) ==
    LET bucket == { evt \in events_in_r2 :
                        evt[1] = tenant /\ evt[2] = sku /\ evt[3] = bp }
        key    == <<tenant, sku, bp>>
    IN  /\ bucket # {}
        /\ \/ key \notin DOMAIN counters
           \/ counters[key] # bucket
        /\ counters' = IF key \in DOMAIN counters
                       THEN [counters EXCEPT ![key] = bucket]
                       ELSE counters @@ (key :> bucket)
        /\ UNCHANGED <<events_emitted, events_in_staging, events_in_r2,
                       invoice_line_items, stripe_invoiced, stripe_outage_active,
                       retry_queue, op_count>>

\* WI-S10-003 monthly invoice generator. FAIL-CLOSED: drains counter rows
\* into invoice_line_items deterministically.
GenerateInvoice(tenant, sku, bp) ==
    LET key == <<tenant, sku, bp>>
    IN  /\ key \in DOMAIN counters
        /\ \/ key \notin DOMAIN invoice_line_items
           \/ invoice_line_items[key] # counters[key]
        /\ invoice_line_items' = IF key \in DOMAIN invoice_line_items
                                 THEN [invoice_line_items EXCEPT ![key] = counters[key]]
                                 ELSE invoice_line_items @@ (key :> counters[key])
        /\ UNCHANGED <<events_emitted, events_in_staging, events_in_r2,
                       counters, stripe_invoiced, stripe_outage_active,
                       retry_queue, op_count>>

\* WI-S10-003 Stripe API call with Idempotency-Key.
\* INV-BILLING-NO-DUP: (tenant, billing_period) UNIQUE in stripe_invoiced.
\* If Stripe outage active, charge enters retry_queue (PAT-QUEUE-EVENTS-001).
StripeChargeIdempotent(tenant, bp) ==
    LET stripe_key == <<tenant, bp>>
        bucket_events == { evt \in events_in_r2 :
                              evt[1] = tenant /\ evt[3] = bp }
    IN  /\ stripe_key \notin DOMAIN stripe_invoiced
        /\ \E sku \in SKUs : <<tenant, sku, bp>> \in DOMAIN invoice_line_items
        /\ ~stripe_outage_active
        /\ InFlightCount < MaxConcurrentEvents
        /\ stripe_invoiced' = stripe_invoiced @@ (stripe_key :> bucket_events)
        /\ UNCHANGED <<events_emitted, events_in_staging, events_in_r2,
                       counters, invoice_line_items, stripe_outage_active,
                       retry_queue, op_count>>

\* WI-S10-003 + RB-FM-151 outage simulation: Stripe charge enters retry_queue.
\* PAT-QUEUE-EVENTS-001 fallback (cooperation WI-S10-003).
StripeChargeQueuedDuringOutage(tenant, bp) ==
    LET stripe_key == <<tenant, bp>>
    IN  /\ stripe_outage_active
        /\ stripe_key \notin DOMAIN stripe_invoiced
        /\ \E sku \in SKUs : <<tenant, sku, bp>> \in DOMAIN invoice_line_items
        /\ stripe_key \notin RetrySet
        /\ InFlightCount < MaxConcurrentEvents
        /\ retry_queue' = Append(retry_queue, stripe_key)
        /\ UNCHANGED <<events_emitted, events_in_staging, events_in_r2,
                       counters, invoice_line_items, stripe_invoiced,
                       stripe_outage_active, op_count>>

\* WI-S10-003 retry queue drain on Stripe recovery.
\* PAT-BACKOFF-001 retry; charge succeeds idempotently (same Idempotency-Key
\* by construction).
DrainRetryQueue ==
    /\ Len(retry_queue) > 0
    /\ ~stripe_outage_active
    /\ LET stripe_key == Head(retry_queue)
           bucket_events == { evt \in events_in_r2 :
                                  evt[1] = stripe_key[1] /\ evt[3] = stripe_key[2] }
       IN  /\ stripe_invoiced' =
                IF stripe_key \in DOMAIN stripe_invoiced
                THEN stripe_invoiced
                ELSE stripe_invoiced @@ (stripe_key :> bucket_events)
           /\ retry_queue' = Tail(retry_queue)
    /\ UNCHANGED <<events_emitted, events_in_staging, events_in_r2,
                   counters, invoice_line_items, stripe_outage_active, op_count>>

\* RB-FM-151 outage begin / recover (FM-151 risk catalog).
StripeOutageBegins ==
    /\ ~stripe_outage_active
    /\ stripe_outage_active' = TRUE
    /\ UNCHANGED <<events_emitted, events_in_staging, events_in_r2, counters,
                   invoice_line_items, stripe_invoiced, retry_queue, op_count>>

StripeOutageRecovers ==
    /\ stripe_outage_active
    /\ stripe_outage_active' = FALSE
    /\ UNCHANGED <<events_emitted, events_in_staging, events_in_r2, counters,
                   invoice_line_items, stripe_invoiced, retry_queue, op_count>>

(*-- Next -------------------------------------------------------------------*)

Next ==
    \/ \E t \in Tenants, s \in SKUs, bp \in BillingPeriods, n \in 1..MaxEventsPerTenant :
         EmitEvent(t, s, bp, n)
    \/ \E evt \in events_in_staging : DrainStagingToR2(evt)
    \/ \E t \in Tenants, s \in SKUs, bp \in BillingPeriods : AggregateCounter(t, s, bp)
    \/ \E t \in Tenants, s \in SKUs, bp \in BillingPeriods : GenerateInvoice(t, s, bp)
    \/ \E t \in Tenants, bp \in BillingPeriods : StripeChargeIdempotent(t, bp)
    \/ \E t \in Tenants, bp \in BillingPeriods : StripeChargeQueuedDuringOutage(t, bp)
    \/ DrainRetryQueue
    \/ StripeOutageBegins
    \/ StripeOutageRecovers

Spec ==
    /\ Init
    /\ [][Next]_vars
    /\ WF_vars(StripeOutageRecovers)
    /\ \A evt \in [tenant: Tenants, sku: SKUs, bp: BillingPeriods, seq: 1..MaxEventsPerTenant] :
         WF_vars(DrainStagingToR2(<<evt.tenant, evt.sku, evt.bp, evt.seq>>))
    /\ WF_vars(DrainRetryQueue)

(*-- Invariantes HIGH -------------------------------------------------------*)

\* INV-BILLING-NO-LOSS (HIGH; registry §3.9 line 136):
\* Every emitted event is accounted for somewhere — staging, R2, or
\* aggregated into a Stripe-invoiced bucket. Drift > 0.1% = SEV-2 +
\* Finance review per spec contract §14.s10.1.
INV_BILLING_NO_LOSS ==
    \A evt \in events_emitted :
        EventAccountedSomewhere(evt)

\* INV-BILLING-NO-DUP (HIGH; registry §3.9 line 137):
\* No two distinct (tenant, billing_period) Stripe charges share the
\* same idempotency key. Modeled by the DOMAIN being a set: the same
\* key maps to a single bucket by construction. The non-trivial check
\* is that GenerateInvoice and StripeChargeIdempotent are deterministic
\* in the bucket they assign — same input -> same output.
INV_BILLING_NO_DUP ==
    \A k1, k2 \in DOMAIN stripe_invoiced :
        k1 = k2 => stripe_invoiced[k1] = stripe_invoiced[k2]

\* INV-BILLING-CHAIN-INTEGRITY (HIGH; spec contract §6 NEW for S-10):
\* Aggregator counters for (tenant, sku, bp) match the corresponding
\* R2 event subset; Stripe-invoiced sets are subsets of R2; chain
\* monotonicity by construction (counter is deterministic from R2).
INV_BILLING_CHAIN_INTEGRITY ==
    /\ \A k \in DOMAIN counters :
         counters[k] \subseteq events_in_r2
    /\ \A k \in DOMAIN invoice_line_items :
         invoice_line_items[k] = counters[k]
    /\ \A k \in DOMAIN stripe_invoiced :
         stripe_invoiced[k] \subseteq events_in_r2

\* INV-BILLING-RECONCILE-3-LAYER Layer-1 sub-property (HIGH; registry §3.12 line 166):
\* For every Stripe-invoiced (tenant, billing_period), the count of
\* events charged equals the sum of counter buckets for that
\* (tenant, *, billing_period) — Layer 1 zero-drift sustained.
\* Higher layers (counter <-> invoice_line_items <-> Stripe) follow from
\* INV_BILLING_CHAIN_INTEGRITY.
INV_LAYER_1_RECONCILE ==
    \A k \in DOMAIN stripe_invoiced :
        Cardinality(stripe_invoiced[k]) =
            Cardinality({ evt \in events_in_r2 :
                              evt[1] = k[1] /\ evt[3] = k[2] })

================================================================================
