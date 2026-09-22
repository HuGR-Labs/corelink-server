# Ownership preparation: current-main Cargo census

**State:** source preparation only. No owner, signoff, semantic approval, or issue readiness is assigned.
**Source:** `HuGR-dev/corelink-server@37f96dba42cc159cd8ab8e9c4ab9d35b8e5634b5`.
**Related:** #1699.

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
- [Provenance verifier](verify_current_provenance.py)
- [Hosted verifier workflow](../../../../.github/workflows/issue-1699-ownership-preparation.yml)

The packets are bounded investigation inputs. They do not claim runtime behavior, complete semantic relations, an owner, a signoff boundary, or approval to publish a package issue.

## Verification boundary

No local tests, builds, or lint were run for this refresh. The hosted verifier requires the checked-out HEAD to equal this source revision, checks every package manifest and source evidence blob, requires exactly 105 aligned package/source/seed records, and rejects fabricated owner/signoff or approval fields. DCO, rustfmt, focused hosted checks, and claims checks remain repository gates on the pull request.
