# B-215–B-230 runtime and external closure packet

This packet is intentionally limited to the two lanes whose source
implementation is not sufficient to claim a deployed/runtime closure. B-226
is retained below only as optional deployment evidence: its implementation
contract is closed, but this document never claims that production endpoints
are provisioned or that a deployed delivery was observed.

## B-216 — DSR dead-letter operational evidence

Owner: `tl` / operations. Keep B-216 `open` until all of the following are
attached to the sprint evidence bundle:

1. the deployed signup-worker revision and queue binding showing the
   `corelink-dsr-erasure-dlq` consumer is active;
2. the on-call destination receiving the structured critical alert (not only a
   console log), with a redacted delivery receipt; and
3. one controlled exhausted-message observation showing alert, bounded
   requeue, and final operator disposition.

The source consumer and its tests are implementation evidence; they do not
prove that an external alert was delivered.

Attach the three redacted records below to the evidence bundle:

- `reports/owner-actions/b216-deployed-signup-worker.json` — revision,
  `corelink-dsr-erasure-dlq` binding, and deployment timestamp;
- `reports/owner-actions/b216-alert-delivery.json` — on-call destination and
  redacted alert delivery receipt; and
- `reports/owner-actions/b216-exhausted-observation.md` — controlled exhausted
  message, alert, bounded requeue, and final operator disposition.

## B-226 — implementation contract closed; optional runtime evidence

Owner: `tl` / operations. B-226 is `done` for the repository implementation:
dashboard, in-app, email, and Slack all dispatch through the repo-owned
`AlertTransport`, and only a provider `DeliveryReceipt` can produce success;
disabled, unconfigured, non-2xx, and transport failures remain visible. The
current truthful `Failed` outcomes remain visible on provider failure and must
not be replaced with fabricated success.

This section is not evidence that production endpoints are provisioned or
that a deployed delivery was observed. If operations separately records that
runtime state, attach one authenticated, redacted receipt per channel and one
visible failure per channel to the evidence bundle below.

Attach:
`reports/owner-actions/b226-channel-delivery.json` (one authenticated,
redacted receipt per channel) and
`reports/owner-actions/b226-failure-observations.md` (one visible failure per
channel). Do not include endpoint secrets or customer payloads.

## B-229 — Clerk webhook secret binding

Owner: `tl` / release operations. Keep B-229 `open` until a production deploy
shows `CLERK_WEBHOOK_SECRET` present in the deploy gate, the gate output is
retained, and one real verified Clerk webhook reaches the signature check on
that revision. The repository script/workflow wiring alone does not prove the
runtime secret is bound.

Attach `reports/owner-actions/b229-production-deploy-gate.json` (redacted gate
output with revision) and `reports/owner-actions/b229-verified-webhook.json`
(redacted event id, revision, and signature-verification result). Never record
the secret value or a raw webhook payload.
