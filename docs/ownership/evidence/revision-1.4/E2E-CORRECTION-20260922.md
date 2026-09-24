# e2e-billing-flow coverage and recovery correction — 2026-09-22

Baseline inspected: `11898804e5c8cfb518e3f48fe84e096917879152`.

## Corrected coverage claim

`tests/e2e-billing-flow/tests/end_to_end.rs`, scenario 5, creates a signed
fixture, tampers with its payload, then calls `BillingHarness::deliver_signed_webhook`
directly. Its assertions expect `SignatureRejected`, audit order
`WebhookReceived` before `SignatureRejected`, and lifecycle to remain
`CancelScheduledAtPeriodEnd`. This is evidence for the handler path only.

`BillingHarness::process_refund` in `tests/e2e-billing-flow/src/harness.rs`
first changes lifecycle to `Refunded`, then calls `deliver_signed_webhook`.
If signature validation or a later handler operation returns an error, the
method returns that error without restoring lifecycle. The existing tamper
scenario does not call this method. A separate tampered `process_refund` test
is absent, and the hazardous failure path remains untested. The existing
refund-without-cancel scenario exercises the pre-mutation invariant gate only.

## Recovery guidance

The harness exposes no safe state restoration method. After a failed
`process_refund` on a disposable local harness, capture the lifecycle, error,
and audit snapshots, then discard that whole `BillingHarness`. To retry from
the cancel-pending precondition, create `BillingHarness::setup()`, rebuild the
deterministic tenant with `make_test_tenant`, then call signup, tier selection,
checkout completion, and cancellation in order. Verify the new instance is
`CancelScheduledAtPeriodEnd` before continuing. Do not reuse the failed
instance or manually mutate its state. This recipe resets harness memory only;
it does not establish or restore any external refund or Stripe state.

## Evidence state

| Claim | Class | Result |
|---|---|---|
| Scenario 5 calls the handler directly and checks rejection, audit order, and unchanged lifecycle | `SOURCE` | Confirmed by static source inspection |
| `process_refund` mutates before handler validation and has no rollback on error | `SOURCE` | Confirmed by static source inspection |
| Tampered `process_refund` behavior | `REVIEWED_NOT_EXECUTED` | No test currently exercises this path; no execution evidence |
| Package runtime behavior | `UNKNOWN` | No Cargo/test/runtime execution was performed for this correction |

This file and the package docs describe a source-level correction only. They do
not certify test execution, deployment, Stripe behavior, or observed runtime.
