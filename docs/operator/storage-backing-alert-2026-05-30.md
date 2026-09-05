# Storage Backing Alert — historical record and current probe

This document records the 2026-05-30 storage monitor and the current operator
procedure. The old public monitor is retained as historical evidence only.

## Historical monitor (retired by B-082)

On 2026-05-30, BetterStack monitor `4467865` was created against the anonymous
URL `https://corelink-api.humangr.com/_health/container`, with the keyword
`"storage":"r2"`, 180-second checks, and status-page resource `8890128` under
status page `247652` / API section `328864`. The recorded state was **up**.

That monitor is no longer an actionable storage alert. The anonymous endpoint
now deliberately removes `storage` and topology fields, so a public keyword
monitor cannot distinguish durable R2 from the in-memory fallback. Do not
recreate, re-enable, attach alerting to, or expose this monitor as a storage
signal. The historical resource should be disabled or removed through the
BetterStack console by the service owner; this codebase performs no remote
monitor mutation.

The old provisioning helper is retired at
`scripts/provision-storage-monitor.sh`; it exits with an explanatory error and
must not be replaced with a public monitor carrying a secret.

## Current operator deep probe (B-082)

Use the authenticated variant from a protected operator shell or secret-bearing
internal probe:

```sh
curl -fsS \
  -H "X-Corelink-Internal-Auth: $CORELINK_ADMIN_AUTH_KEY" \
  https://corelink-api.humangr.com/_health/container/authenticated
```

The response must contain `"status":"ok"` and `"storage":"r2"`. The
`CORELINK_ADMIN_AUTH_KEY` is the dedicated admin key, not the shared internal
key. Never put it in a URL, query string, public monitor, status page, or log.

Authentication semantics are intentional: a missing or malformed configured
admin key returns `503`; a missing or incorrect header returns `401`. The
authenticated response is marked `Cache-Control: no-store`.

## Fallback runbook

If the authenticated probe reports `"storage":"inmemory"`:

1. Check the R2 bindings on the main Worker and signup Worker (names only).
2. Confirm `R2_S3_ACCESS_KEY_ID`, `R2_S3_SECRET_ACCESS_KEY`, and
   `R2_S3_ENDPOINT` are present and valid.
3. Re-inject rotated values with `wrangler secret put` and redeploy the affected
   Worker through the normal release process.
4. Repeat the authenticated probe and confirm `"storage":"r2"`.
5. Audit writes accepted during the in-memory interval; they are not durable.

The detailed remediation runbook is
`specs/_runbooks/rb-storage-fallback.md`, which must follow the same
authenticated probe procedure.
