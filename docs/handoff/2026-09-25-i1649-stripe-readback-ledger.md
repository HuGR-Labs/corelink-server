# Issue #1649 Stripe readback and owner action ledger

Observed 2026-09-25 from the isolated `origin/main` checkout at
`6a22fe1987d547c001758d19e6c3b84e969f6fd1`. This is a read-only evidence
ledger. It records no Stripe mutation, webhook replay, customer creation, charge,
refund, secret access, or workflow dispatch.

## Five identity and delivery axioms

1. **Event identity is the payload's Stripe Event `id` (`evt_…`).** Use that
   value to compare deliveries of the same Event across destinations and
   retries. A Stripe API response `Request-Id` identifies an API request; it is
   not an Event ID or a webhook deduplication key. See [Stripe Request IDs](https://docs.stripe.com/api/request_ids).
2. **Destination IDs name distinct API resources.** Snapshot destinations use
   the v1 `/v1/webhook_endpoints` collection (`we_…`); event destinations use
   the v2 `/v2/core/event_destinations` collection (`ed_…`). They require
   separate, complete pagination. A v1-only read cannot establish v2 absence.
3. **A signature authenticates the exact delivery bytes, not its identity.**
   Verify `Stripe-Signature` against the raw request body and that destination's
   signing secret before side effects. The repository verifier accepts any
   matching `v1` candidate for rotation overlap; this does not establish which
   secret is currently installed at either production endpoint.
4. **Retries and duplicate effects are different questions.** The worker's
   durable claim uses `event.id`, while the container's recorded key has
   historically been a derived 64-character digest. Those keys cannot conflict
   with each other. A type-level count or two key schemes therefore does not
   prove that two rows are one Event, nor that two destinations are active now.
5. **API request idempotency does not replace webhook-event deduplication.**
   Stripe's `Idempotency-Key` makes a retried API request repeat its stored
   result; webhook delivery replay must still be checked using the Event ID and
   the receiving handler's durable idempotency record. Negative evidence must
   keep these identifiers separate.

## Readback captured

### Stripe

- A live read-only `stripe webhook_endpoints list --limit 100 --live` was
  attempted using the installed Stripe CLI. It failed before producing an
  authenticated page; provider/CLI error output was suppressed. No Stripe
  credential variable was present in the environment. No v1 or v2 page was
  read, so **live and test destination state, API versions, event lists, and
  latest delivery times remain unknown**.
- The latest historical dashboard readback referenced by issue #1649 is
  2026-09-22 23:05 UTC. It reports the signup-worker v1 destination
  `we_1ToligLh0hhAZjwoI8PERw8x` and container v1 destination
  `we_1Tfh8PLh0hhAZjwoCnqvruqC` active; thin v2 destination
  `ed_61Up55mmoh5aeEa5U16UMYdU7v9Px7f35nqL2wHU8N6W` and wallet v1
  destination `we_1TJ1YZLh0hhAZjwoTXnh9mTD` disabled. That readback found no
  billing deliveries to correlate. It is historical and does not prove status
  on 2026-09-25. Preserve the two legitimate CoreLink destinations unless the
  owner explicitly authorizes a specific change.
- No delivery was generated or replayed. No signing secret was displayed,
  retrieved, rotated, or changed.

### GitHub billing-health evidence

The workflow is still `disabled_manually` (workflow ID `340275338`). Read-only
run inspection shows two consecutive successful checks after the preceding
failure. Both success logs report `BILLING HEALTH: OK` and “no event type
ingested under two webhook id schemes.” This supersedes the issue body's older
claim that no later successful run existed, but does not satisfy the required
three consecutive successes.

| Run | Created (UTC) | Result | Exact head SHA |
| --- | --- | --- | --- |
| [35659074271](https://github.com/HuGR-dev/corelink-server/actions/runs/35659074271) | 2026-09-21 21:46:09 | failure | `c363612a5db81906b632f64c9ef449a55902777a` |
| [35681550304](https://github.com/HuGR-dev/corelink-server/actions/runs/35681550304) | 2026-09-22 03:00:31 | success, health OK | `683df028a56a07b32384013c48ab38b5b8b9d981` |
| [35803767730](https://github.com/HuGR-dev/corelink-server/actions/runs/35803767730) | 2026-09-23 00:50:15 | success, health OK | `a5c34e6e46e8571b141590e124336620fd04e2b6` |

The third consecutive successful run is absent. Do not enable or dispatch this
manually disabled workflow as part of this ledger.

## Exact owner action to unblock

1. Provide an authorized operator with read-only access to the correct Stripe
   account in **both live and test mode**, including v1 and v2 destination
   inventory and Workbench delivery history. Confirm account identity in the
   private operator session; do not put account credentials or signing secrets
   in the receipt.
2. Exhaustively paginate `/v1/webhook_endpoints` and
   `/v2/core/event_destinations` separately in each mode. Record for every
   destination: API/resource version, full destination ID, URL host and path,
   status, complete event list, payload type, destination API version, observed
   timestamp, and most recent delivery time (or `null` plus “no deliveries”).
   Exclude signing-secret fields, payloads, customer data, and query strings.
3. In Workbench, inspect the relevant delivery history and correlate by the
   same `evt_…` Event ID when available. Record the delivery identifier, time,
   destination, attempt/result, and correlation outcome in redacted form. Do
   not infer identity from event type, count, or API `Request-Id`. If history
   cannot correlate an Event, record that limitation.
4. Confirm which destination signing secret is configured at each receiver and
   whether a rotation overlap is in progress, without reading the secret value.
   Any secret rotation or destination change remains a separate owner-approved
   action.
5. Preserve signup-worker `we_1ToligLh0hhAZjwoI8PERw8x` and container
   `we_1Tfh8PLh0hhAZjwoCnqvruqC`. If a redundant destination is currently
   active, record its ID and prior state and obtain explicit owner authorization
   for that exact ID before disabling it. Re-read and record the post-state.
   If already disabled, record `resolution.mode=observed_disabled` and
   `mutation_performed=false`.
6. After the operator determines the workflow owner-approved path, obtain a
   third consecutive successful `billing-health-daily` run and record its run
   ID, UTC time, head SHA, and `BILLING HEALTH: OK` result. The workflow is
   currently disabled manually; this ledger does not authorize enabling it.
7. Write the redacted result to
   `evidence/owner-actions/B-065/stripe-endpoint-retirement.json` using the
   contract in issue #1649 and run
   `python3 -S scripts/verify_owner_action_packets.py --id B-065`. The
   verifier's MANUAL result is not provider evidence. Keep B-065 open until the
   provider readback, correlation outcome, and all three run receipts satisfy
   the issue's acceptance criteria.

## Repository evidence pointers

- `scripts/check_billing_health.py`: explicitly treats mixed ID schemes as a
  historical signal, not current destination status or event correlation.
- `apps/signup-worker/src/webhooks/stripe.ts` and
  `apps/signup-worker/src/webhooks/stripe_persistence_billing.ts`: raw-body
  signature check and durable `event.id` claim after idempotent writes.
- `crates/corelink-billing-stripe/src/signature.rs`: timestamped HMAC
  verification and multiple `v1` signature candidates for rotation overlap.
- `scripts/ops/stripe_webhook_inventory.py` and
  `scripts/ops/stripe-reconcile-webhook-events.sh`: separate fail-closed,
  paginated v1/v2 inventory; inventory is read-only unless separately opted
  into event reconciliation.
- `docs/operator/stripe-webhook-events-reconcile.md`: canonical event coverage
  and preservation context for the signup-worker and container destinations.
