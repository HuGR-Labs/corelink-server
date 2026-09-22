# B-054 nonproduction key custody and rotation

## Protected environment

The GitHub Actions environment `b054-key-custody-drill` is reserved for
synthetic nonproduction material. Keep its existing two-person protections:
self-approval is disabled, administrator bypass is disabled, and only `main`
is an allowed branch. A named independent reviewer approves each protected
run. The workflow requires a manual dispatch from protected `main` in the
canonical repository; pull request jobs compile the ignored Rust exercise
without access to any environment value.

The environment contains four write-only, versioned names:

- `B054_E1_SIGNING_SEED_HEX` and `B054_E1_LINK_KEY_HEX` are the historic E1
  signing and link material.
- `B054_E2_SIGNING_SEED_HEX` and `B054_E2_LINK_KEY_HEX` are the forward E2
  material. E2 uses a distinct signing key, link key, and link-key id; E1
  remains available with its original commitment.

Provision and update values only through the GitHub environment secret UI.
Never put a key value in source, D1, R2, workflow output, a log, a test
fixture, or a receipt. The workflow passes values only to the ignored test
process. It holds the process transcript in runner temporary storage and
publishes neither that transcript nor the receipt until the redaction scan
confirms that source, transcript, and receipt contain none of the protected
bytes. The uploaded receipt contains epoch ids, public link-key commitments,
run metadata, and status fields only.

The drill exercises the shipped epoch, keyring, commitment, and archive
verification primitives in memory. It does not connect to D1, R2, a Worker,
or a production credential. A successful run produces a redacted receipt with
90-day artifact retention. Inspect that artifact before closing issue #1794.

## Rotation, retention, and revocation

Register each successor under new epoch-specific secret names and the next
unused epoch and link-key ids. Never overwrite historical material with
successor bytes. Keep each historical name through the linked audit-data
retention horizon. The drill demonstrates retention of E1 while E2 is active;
it does not simulate elapsed time across that full horizon.

Missing, malformed, or unavailable historical material must produce
`INDETERMINATE`. It must not fall back to the active key or the legacy E0
formula. Revoke historical material only after every linked audit record has
left its required retention window. The drill simulates early revocation with
an ephemeral in-memory keyring and confirms fail-closed behavior; it does not
delete or modify E1.

GitHub retains each environment secret until an authorized administrator
deletes it and does not provide per-secret immutable retention. At each
rotation, a provisioning operator records the names-only inventory and
confirms that historical names remain present. The drill validates one
forward rotation and the retention procedure; it makes no claim that the
multi-year retention period has elapsed.
