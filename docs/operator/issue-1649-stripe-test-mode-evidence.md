# Issue #1649 Stripe test-mode evidence lane

This lane is a bounded GitHub hosted probe. It talks only to Stripe's API with a disposable test-mode Customer. It does not call a CoreLink, Cloudflare, webhook, or production endpoint.

## Dispatch prerequisites

The repository must have a protected Actions environment named `stripe-test` with this secret:

| Name | Required value | Failure behavior |
| --- | --- | --- |
| `STRIPE_SECRET_KEY` | A Stripe test secret beginning `sk_test_` | Missing: the executor is skipped with a notice. Any other prefix, including `sk_live_`: the probe fails before a network request. |

Dispatch must target protected `main` and use the exact input `run-i1649-stripe-test-mode`. The workflow runs on `ubuntu-latest`; it has no self-hosted runner dependency.

## Evidence contract

The probe first reads `/v1/account` and requires `livemode=false`. It creates one Customer with a deterministic run-scoped idempotency key, repeats the identical request, requires the same Customer id, retrieves it after creation, and then deletes it. The receipt records the ordered step names, `livemode=false`, replay success, and cleanup success. Customer and idempotency identifiers are shortened; no key, response body, or payment data is written to logs or the receipt.

Cleanup runs from `finally`. A missing or unsuccessful deletion makes the run fail. The receipt is uploaded only when the secret gate opened and is retained for 14 days. The credentialless contract job runs on pull requests and checks the hosted runner, test-key guard, livemode assertion, idempotency, cleanup, and credential persistence markers.

This lane is runtime evidence for Stripe test-mode API behavior. It does not claim webhook delivery or production billing behavior.
