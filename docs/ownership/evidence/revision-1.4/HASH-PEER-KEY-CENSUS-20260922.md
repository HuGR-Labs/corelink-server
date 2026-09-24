# `corelink-hash` peer-key census — 2026-09-22

A stable-key search was run across all package blast documents. It did not
infer approval or runtime use; it only checked whether the qualified boundary
was independently named by a peer artifact.

## Exact shared-key matches

| Hash relation | Peer artifact | Result |
|---|---|---|
| REL-025 `hash-signup-r2-integrity-001` | `e2e-signup-flow/BLAST_RADIUS.md` | key present; peer state still pending |
| REL-026 `hash-server-cas-read-limit-001` | `corelink-server/BLAST_RADIUS.md` | key present; peer state still pending |
| REL-027 `hash-server-cas-delete-limit-001` | `corelink-server/BLAST_RADIUS.md` | key present; peer state still pending |
| REL-029 `hash-fuzz-harness-001` | `corelink-hash-fuzz/BLAST_RADIUS.md` | key present; peer state still pending |
| REL-030 `hash-fuzz-digest-parse-001` | `corelink-hash-fuzz/BLAST_RADIUS.md` | key present; peer state still pending |
| REL-031 `hash-fuzz-verify-body-001` | `corelink-hash-fuzz/BLAST_RADIUS.md` | key present; peer state still pending |
| REL-047 `hash-container-envelope-001` | `corelink-server/BLAST_RADIUS.md` | key present; peer state still pending |
| REL-048 `hash-server-bazel-limit-001` | `corelink-server/BLAST_RADIUS.md` | key present; peer state still pending |
| REL-034..REL-036 | `corelink-hash-fuzz/BLAST_RADIUS.md` | keys present; peer state still pending |
| REL-033 `worker-fuzz-hash-001` | `corelink-worker-fuzz/BLAST_RADIUS.md` | global key `worker-fuzz-hash-001` present as peer REL-002; peer state still pending |

## No exact peer key found

REL-020..REL-024, REL-028, REL-032 and REL-037..REL-042 have no exact
stable-key match in another package document. REL-047 and REL-048 are exact
matches in `corelink-server`; REL-033 is matched by its global key in the
worker-fuzz peer despite the local peer ID being REL-002, so none are part of
the no-match population.
This is an explicit unknown, not
an exclusion: AC, meta, REAPI, Bazel bridge, CAS, worker fuzz, CI, server
internal paths and client FFI still require peer-owner reconciliation.

The census reduces the unresolved search surface but does not change any
ledger `peer_review` value, cold-review verdict or publication gate.
