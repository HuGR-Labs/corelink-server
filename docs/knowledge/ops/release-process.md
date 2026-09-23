---
type: "Runbook"
title: "Release / GA tag process"
description: "Fail-closed process for the signed v1.0.0-GA server release tag and its exact evidence bindings."
source_files:
  - "docs/release/v1.0.0-GA-tag-draft-final.txt"
  - "scripts/cut-v1-0-0-ga-tag.sh"
source_blobs:
  - "docs/release/v1.0.0-GA-tag-draft-final.txt@1769b9949f98cb0434f751e12203caee130191d2"
  - "scripts/cut-v1-0-0-ga-tag.sh@0cf12f932dcffd108e6d8d5e824bf51212301207"
checkpoint_sha: "64e57a2ccb218f475b44a260b64af95e0bc7df2c"
provenance: "AUTHORED"
tags: ["ops", "release", "ga", "sign-off", "provenance"]
timestamp: "2026-09-09T00:00:00-03:00"

---
# Release / GA tag process

`v1.0.0-GA` is the canonical first server GA release. It is a cryptographically
signed Git tag bound to an exact source commit, production OCI digest,
deployment readback, cutover attestation, framework promotion object, bundled
CI result, and two independent signed approvals. It is not a capability census
and does not turn planned or partially wired features into shipped features.

# Required gates

1. `origin/main` equals the full expected release SHA.
2. `framework-v1-0-0-ga` is a trusted signed tag, targets an ancestor of the
   release, and that release tree contains `specs/00_framework.md` as FROZEN
   v1.0.0.
3. Spec and reference validators pass on the exact release tree.
4. The typed cutover JSON names the exact SHA and records GREEN, 6/6
   greenlights, and 11/11 steps. A rehearsal is not accepted.
5. Evidence lives in an immutable descendant commit, outside the already
   frozen release tree, and is addressed as `<commit>:<path>` plus SHA-256 in
   the tag. This avoids an impossible self-referential commit hash. Deployment
   evidence names the exact SHA and deployed OCI digest.
6. A machine-readable signoff manifest names exactly one Owner and one Ops
   authority, distinct principals, distinct signed commits, and distinct
   policy-pinned key fingerprints. Each commit binds the release SHA and all
   evidence digests. The Ops principal/fingerprint remain null until an
   independent authority is actually nominated; the cut fails closed meanwhile.
   The evidence and both signoff commits are published atomically under the
   dedicated `release-evidence-*` / `release-signoff-{owner,ops}-*` tag refs,
   so a fresh clone can resolve every object named by the signed release tag.
7. The tag message has no placeholders and makes explicit non-claims for
   unevidenced capabilities.
8. `scripts/cut-v1-0-0-ga-tag.sh` proves the configured release key can sign,
   creates the tag with `git tag -s`, verifies it against
   `.github/release-allowed-signers`, pushes only to canonical `origin`, and
   verifies the remote peeled target equals the exact release SHA.

# Provenance boundary

The server release provenance is the signed tag plus its content-addressed
source, OCI, deployment, cutover, framework, CI, and signoff bindings. The
`release-slsa3.yml` path belongs to separate `cli-v*` binary releases; this
server tag does not claim CLI SLSA provenance.

# Failure semantics

Dry-run is non-mutating but applies the same gates as the real cut. Missing or
lightweight framework tags, unresolved placeholders, wrong remote state,
untrusted/reused signoff keys, stale evidence, or an unusable release key all
fail closed. Release notes remain DRAFT for the tag-triggered editorial PR;
the cut script does not leave an uncommitted local thaw.

# Current state

Until every gate above passes, B-098 remains open and neither the local nor
remote `v1.0.0-GA` ref should exist.

# Citations

- `docs/release/v1.0.0-GA-tag-draft-final.txt:1-18` — release identity fields and explicit placeholder requirements.
- `scripts/cut-v1-0-0-ga-tag.sh:5-23` — cut prerequisites, two-key sign-off, evidence, signed tag, and remote readback.
- `scripts/cut-v1-0-0-ga-tag.sh:45-60` — accepted evidence arguments and exit outcomes.
