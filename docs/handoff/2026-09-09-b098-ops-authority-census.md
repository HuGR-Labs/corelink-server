---
type: "Evidence"
title: "B-098 Ops authority and key census"
description: "Metadata-only census proving that the independent Ops release authority is not yet provisioned."
checkpoint_sha: "1126e25d294ae16e73efa70004642e34f223285d"
provenance: "AUTHORED"
capture_scope: "current-tree"
remote_status: "ABSENT"
tags: ["b-098", "release", "ops", "fail-closed", "key-census"]
timestamp: "2026-09-22T05:05:26Z"

---
# B-098 Ops authority and key census

## Result

No independently controlled Ops principal and release-signing key were found.
The release policy therefore remains intentionally fail-closed:

- `.github/release-signing-policy.json` (`git-object 08fa5a6906ce4bd7895e0a75badac671651b2bb9`)
  has `roles.ops.principal: null` and `roles.ops.fingerprint: null`.
- `.github/release-allowed-signers` (`git-object
  dfd72d7f31980aa452bfd67628e635a769b3207f`) contains only the Owner trust
  entry (`gustavo@humangr.com`, `SHA256:grBP7UAeYUlzeyv9oe6TDk1OMADK0QrMOVfaIm9Zbwo`).
- `docs/knowledge/ops/release-process.md` requires the Ops fields to remain
  null until an independent authority is actually nominated.

No policy, allowlist, release tag, signoff ref, or remote evidence ref was
changed or created by this census.

## Metadata inspected

The inspection was performed from the current repository checkout at checkpoint `1126e25d294ae16e73efa70004642e34f223285d` (`capture_scope: current-tree`).
Only public metadata and fingerprints were read; private key and token material
was never printed.

- SSH agent: one loaded identity, `SHA256:grBP7UAeYUlzeyv9oe6TDk1OMADK0QrMOVfaIm9Zbwo`,
  matching the Owner entry.
- Local SSH private-key fingerprints: `id_ed25519` → `SHA256:grBP7UAeYUlzeyv9oe6TDk1OMADK0QrMOVfaIm9Zbwo`,
  `hetzner_ci` → `SHA256:561tBRG7n4hAAn4HR7UvkFEt7VC8c2T3WZCH3ddSZ38`
  (`corelink-ci-runner`), and `hugit-runner-01` →
  `SHA256:tOJ88OMgtssM7OmAA5yJWpIJlSpmG9MPMn5J1crdo1c`.
  The latter two have no documented Ops principal or release-policy mapping,
  so they cannot be treated as independent authority keys.
- GPG secret-key metadata: fingerprint `795253CEBD6D54C862CFC4A3EC0AD89A75EC6756`,
  UID `CoreLink Release Signing (corelink-cli release signing key)
  <releases@humangr.com>`. Repository documentation identifies this as the
  CLI release key, not an independent server Ops key.
- GitHub CLI sessions: `gmhelmold` (active) and `gustavomhss` (inactive).
  Both are owner-controlled local accounts; neither establishes an independent
  Ops principal.
- Keychain metadata includes account `corelink-ops` for service
  `CoreLink/METRICS_OBSERVABILITY_KEY`; this is an observability credential and
  has no release public key or fingerprint, so it is not an Ops signer.
- Recent repository signature metadata contains only the Owner fingerprint;
  no distinct Ops signer appears.

## Remote/ref check

`git ls-remote --tags origin` returned no matching refs for:

- `refs/tags/v1.0.0-GA*`
- `refs/tags/release-signoff-owner-v1.0.0-GA`
- `refs/tags/release-signoff-ops-v1.0.0-GA`
- `refs/tags/release-evidence-v1.0.0-GA`

This is an absence record, not a release authorization. The next valid state
requires an actually independent Ops principal, its public signing key and
fingerprint, policy/allowlist updates, two distinct signed signoffs, and
atomic remote evidence anchors. Until then, release cutting remains blocked.
