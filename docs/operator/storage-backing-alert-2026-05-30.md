# Storage Backing Alert — historical record and current probe

This document records the storage alert provisioned on 2026-05-30 and the
current operator procedure. The original public alert is retained only as
historical context; its remote identifiers are intentionally omitted so this
page cannot be used to recreate or mutate retired alerting.

## Historical alert (retired by B-082)

On 2026-05-30, a BetterStack check watched the anonymous container-health
response for a storage body marker. It was later retired because the anonymous
endpoint deliberately redacts storage and topology details. A public check can
therefore no longer distinguish durable R2 from the in-memory fallback.

Do not recreate, re-enable, attach alerting to, or expose the retired check as
a storage signal. Any historical status-page entry should remain disabled or
be removed by the service owner; this codebase performs no remote alerting
mutation. The provisioning helper at
`scripts/provision-storage-monitor.sh` is retired and must not be replaced with
a public monitor carrying a secret.

## Current operator deep probe (B-082)

Set `CORELINK_API_ORIGIN` to the current API origin in a protected operator
shell or secret-bearing internal probe, then use the authenticated variant:

```sh
AUTH_HEALTH_URL="${CORELINK_API_ORIGIN:?set the current API origin}/_health/container/authenticated"
curl -fsS \
  -H "X-Corelink-Internal-Auth: $CORELINK_ADMIN_AUTH_KEY" \
  "$AUTH_HEALTH_URL"
```

Validate the response semantically rather than configuring a public body-match
alert:

```sh
curl -fsS \
  -H "X-Corelink-Internal-Auth: $CORELINK_ADMIN_AUTH_KEY" \
  "$AUTH_HEALTH_URL" | jq -e '.status == "ok" and .storage == "r2"'
```

The `CORELINK_ADMIN_AUTH_KEY` is the dedicated admin key, not the shared
internal key. Never put it in a URL, query string, public monitor, status page,
or log.

Authentication semantics are intentional: a missing or malformed configured
admin key returns `503`; a missing or incorrect header returns `401`. The
authenticated response is marked `Cache-Control: no-store`.

## Fallback runbook

If the authenticated probe reports that storage is `inmemory`:

1. Check the R2 bindings on the main Worker and signup Worker (names only).
2. Confirm `R2_S3_ACCESS_KEY_ID`, `R2_S3_SECRET_ACCESS_KEY`, and
   `R2_S3_ENDPOINT` are present and valid.
3. Re-inject rotated values with `wrangler secret put` and redeploy the affected
   Worker through the normal release process.
4. Repeat the authenticated probe and confirm that storage reports `r2`.
5. Audit writes accepted during the in-memory interval; they are not durable.

The detailed remediation runbook is
`specs/_runbooks/rb-storage-fallback.md`, which must follow the same
authenticated probe procedure.
