# B-229 production evidence — Clerk webhook binding and deploy

This packet records redacted, bounded evidence captured on 2026-09-08. It
contains no webhook payload, signature header, or credential value.

```yaml
item: B-229
worker: corelink-signup-worker
route: https://corelink-signup.humangr.com/webhooks/clerk
source_sha: cba0e59655131c09bd5255ac783cac977a206da8
deployed_version_id: 64282952-57bf-49d5-832c-b4cc4723b8b3
deployed_version_number: 103
traffic: 100%
deployed_at: 2026-09-08T18:58:57.510834Z
signed_probe_http_status: 200
signed_probe_result: ignored
health_http_status: 200
secret_rotation: not_required_existing_binding_verified
payload_and_secret_values: omitted
```

The signed probe used a synthetic ignored event and was bounded to signature
verification plus the handler's `ignored` response; it did not create a Clerk
user or mutate D1. The health request returned HTTP 200. The deployment was
performed from the exact source SHA above, with no source change in this
operational action.
