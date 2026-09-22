# B-054 key custody and rotation

## Nonproduction exercise

The GitHub `b054-key-custody-drill` environment is a nonproduction vault for
the synthetic rotation exercise. It is restricted to the exact `main` branch,
requires an independent reviewer, prevents self-review, and has administrator
bypass disabled. Its four write-only values are versioned by epoch and purpose:

- `B054_E1_SIGNING_SEED_HEX` and `B054_E1_LINK_KEY_HEX` are historical E1
  material retained through the later E2 rotation.
- `B054_E2_SIGNING_SEED_HEX` and `B054_E2_LINK_KEY_HEX` are the forward E2
  material. E2 uses a new signing key, a new link key, and link-key id 102;
  E1 keeps id 101 and its original commitment.

The protected workflow is manually dispatched on `main` after an independent
reviewer approves the environment job. It exercises the production Rust
epoch, keyring, commitment, and archive-verification primitives using only
synthetic bytes. It touches no D1, R2, Worker, or production credential. Its
receipt contains epoch ids, public link-key commitments, statuses, workflow id,
and commit SHA. It contains no key bytes or tenant data. Test output is held in
a runner temporary file and scanned against all four secret values before any
summary is published; on failure the transcript is withheld.

## Custody and lifecycle policy

Provisioning, rotation, and revocation use separate GitHub environment secret
names. Never overwrite a historic epoch's secret with newer bytes. Register
each successor with the next unused epoch and link-key id, and retain the
historic secret names while any linked audit record remains within its
retention window. Missing, malformed, or unavailable historic key material
must produce `INDETERMINATE`; it must never fall back to the active key or
legacy E0 formula. Revoke a historic key only after the linked audit records
and their required retention window have expired. The drill deliberately
simulates missing and malformed historic material but does not revoke E1.

The authorized provisioning operator is the GitHub repository administrator
who creates or updates environment secrets. A different named reviewer must
approve the protected job; self-approval and administrator bypass are disabled.
The environment is restricted to `main`. This is the bounded two-person
authorization path for the exercise; it does not claim separation inside an
external HSM or a production key ceremony. The public receipt and names-only
secret metadata are the only durable campaign evidence. Do not copy key bytes
to source, D1, R2, workflow output, or artifacts.

GitHub retains the versioned secret entries until an authorized operator
deletes them; the platform does not supply per-secret WORM retention. Therefore
the retention obligation is enforced by preserving each epoch-specific name
through the linked audit-data retention horizon and auditing names-only secret
inventory at each rotation. The synthetic drill proves preservation across
one forward rotation, not the passage of a multi-year retention interval.
