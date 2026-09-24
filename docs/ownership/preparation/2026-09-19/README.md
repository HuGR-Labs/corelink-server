# Ownership preparation: historical Cargo census and current manifest observation

**State:** source preparation only. No owner, signoff, semantic approval, or issue readiness is assigned.
**Source:** `HuGR-dev/corelink-server@37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
**Related:** #1699.

The census and its packet bytes remain pinned to that historical source commit. The
separate [current manifest observation](current-manifest-observations.json) records
the `corelink-audit-chain` manifest at main `a577032ab816c1ae5debb2f6ddf29494162f1336`
after #2347. It reconciles the opt-in `aws-s3-object-lock` feature and its optional
AWS dependencies without claiming that the feature was selected, built, or used.

## Population

| Population | Count |
|---|---:|
| Main workspace packages | 95 |
| Independent fuzz packages | 10 |
| Eligible packages | **105** |
| Cargo targets | 631 |
| Tracked manifests | 107 |
| Declared internal edges | 235 |

The historical archive manifest is classified outside the eligible population. The 105 package identities and manifest hashes are captured from this revision.

## Contents

- [Package index](index.md)
- [Census](census.json)
- [Source packets](source-packets.json)
- [Seeds](seeds.json)
- [Summary](summary.json)
- [Manifest classifications](tracked-manifests.json)
- [Current manifest observations](current-manifest-observations.json)
- [Provenance verifier](verify_current_provenance.py)
- [Hosted verifier workflow](../../../../.github/workflows/issue-1699-ownership-preparation.yml)

The packets are bounded investigation inputs. They do not claim runtime behavior, complete semantic relations, an owner, a signoff boundary, or approval to publish a package issue.

## Verification boundary

The hosted verifier checks each package manifest in the historical source tree, checks
seed evidence against its recorded blob and hash in either that source tree or the
explicitly recorded current-main tree, validates the separate current-main observation
and its recorded manifest delta, and requires exactly
105 aligned package/source/seed records. Some seed evidence paths were updated after
the census source commit; their packet pins remain unchanged and are not retroactively
attributed to the historical tree.
It also exercises stale-blob rejection and checks that historical packets, the registry,
and publication ledger remain byte-identical, with 105 `BLOCKED`, 105 `UNVERIFIED`,
105 `NOT_PUBLISHED`, and zero publications. Manifest evidence does not prove runtime
behavior, feature activation, provider operation, approval, or deployment. DCO,
rustfmt, focused hosted checks, and claims checks remain repository gates on the PR.
