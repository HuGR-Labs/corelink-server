# B-054 synthetic key custody and rotation

## Evidence status

This procedure exercises four synthetic values in the protected GitHub
environment `b054-key-custody-drill`. A successful run proves the repository
primitives, the separation between dispatcher and environment approver, and
the redacted receipt path. It does not provision or rotate production keys,
read D1 or R2, exercise a Worker, prove that a retention horizon elapsed, or
complete the real custody ceremony for #1794.

Witness receipt signing seeds are never supplied to this drill. Their custody
belongs to the separate Security-administered #1793 boundary; this receipt is
not evidence for witness deployment, witness-key rotation, or its retention.

The environment's existing reviewer list, self-review prevention, administrator
bypass prohibition, and `main` branch policy are prerequisites. The GitHub
environment approval gate requires an approval from a configured reviewer;
self-review prevention makes that person distinct from the workflow
dispatcher. The protected job reads the run's approval history and fails closed
unless it can record the dispatcher and at least one different approver. The
receipt contains those public account names so a reviewer can audit the two
person separation.

## Generate and provision synthetic material

Use an approved cryptographically secure random generator to create four
independent 32-byte values: E1 signing seed, E1 link key, E2 signing seed, and
E2 link key. A provisioning custodian enters the lower-case hexadecimal
values directly into the write-only secret fields in the existing GitHub
environment. Do not put values in source, D1, R2, local files, shell history,
workflow inputs, logs, or evidence. The exercise consumes only these names:

- `B054_E1_SIGNING_SEED_HEX` and `B054_E1_LINK_KEY_HEX` identify retained E1
  material.
- `B054_E2_SIGNING_SEED_HEX` and `B054_E2_LINK_KEY_HEX` identify the E2
  successor material.

The operator dispatches the workflow from protected `main`. A separate
configured environment reviewer approves it. Pull request jobs never access
the environment and only compile the ignored Rust exercise. The protected job
builds before it receives values, captures a minimal approval receipt, and
then passes values only to the ignored in-memory test. Output stays in runner
temporary storage until the scanner checks tracked text, the test transcript,
the approval receipt, and the redacted receipt for protected values and their
decoded bytes. A scan failure withholds the summary and artifact.

The technical receipt contains epoch and key ids, public signing keys, public
link-key commitments, run metadata, and check results. GitHub retains the
redacted artifact for 90 days. Inspect it and attach its reference to #1647
after the protected run succeeds. No real ceremony is complete before that
provider run and the separate production approvals and receipts exist.

## Forward rotation, failure, and retention

For each actual rotation, the key custodian generates fresh successor signing
and link keys, provisions them under new write-only names, and selects the
next unused epoch and link-key ids. A separate reviewer approves promotion.
Never overwrite E1 or change its registered commitment. Retain E1 material
until every linked audit record has passed its required retention horizon.

Promote only after the successor signature, forward boundary, distinct key
identity, historic archive verification, and unchanged E1 commitment all
pass. If candidate material is absent or malformed, do not commit the successor
epoch: keep the previous epoch head and its key active, retain its secret, and
retry only after the candidate is repaired and independently approved. A
missing or malformed historical key must return `INDETERMINATE`; never fall
back to E2 or legacy E0. Revoke a historic key only after retention has
expired and an independently reviewed readback proves the required records
remain verifiable.

The hosted drill simulates a failed E2 candidate and early E1 revocation using
temporary in-memory keyrings. It does not alter GitHub secrets or perform a
real rollback. The receipt explicitly records that historical revocation was
not performed and that the retention horizon has not elapsed.
