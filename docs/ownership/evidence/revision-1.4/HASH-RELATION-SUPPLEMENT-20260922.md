# `corelink-hash` relation supplement — 2026-09-22

This supplement originally serialized the 23 relations that the pilot blast
document described but that were absent from the historical 19-record
candidate ledger. The records are now promoted into the candidate ledger by
`HASH-LEDGER-RECONCILIATION-20260922.md`; this table remains a read-only audit
view. Every record still has `peer_review=not_reconciled`, and several
source/consumer pins require independent peer confirmation.

| ID | Stable relation key | Qualified boundary | Source anchor(s) | Peer state |
|---|---|---|---|---|
| REL-020 | `hash-ac-digest-serde-001` | `corelink-ac→corelink-hash` | S20 | not reconciled |
| REL-021 | `hash-meta-blob-key-001` | `corelink-meta→corelink-hash` | S21 | not reconciled |
| REL-022 | `hash-meta-migration-fingerprint-001` | `corelink-meta→corelink-hash` | S22 | not reconciled |
| REL-023 | `hash-reapi-verified-write-001` | `corelink-reapi→corelink-hash` | S23 | not reconciled |
| REL-024 | `hash-bazel-bridge-size-limit-001` | `corelink-bazel-bridge→corelink-hash` | S24, S25 | not reconciled |
| REL-025 | `hash-signup-r2-integrity-001` | `e2e-signup-flow→corelink-hash` | S26 | not reconciled |
| REL-026 | `hash-server-cas-read-limit-001` | `corelink-server→corelink-hash` | S27 | not reconciled |
| REL-027 | `hash-server-cas-delete-limit-001` | `corelink-server→corelink-hash` | S28 | not reconciled |
| REL-028 | `hash-cas-manifest-001` | `corelink-cas→corelink-hash` | S29 | not reconciled |
| REL-029 | `hash-fuzz-harness-001` | `corelink-hash-fuzz→corelink-hash` | F01, F03, F04, F09, F10, F14 | not reconciled |
| REL-030 | `hash-fuzz-digest-parse-001` | fuzz `digest_parse`→`corelink-hash` | F03 | not reconciled |
| REL-031 | `hash-fuzz-verify-body-001` | fuzz `verify_body`→`corelink-hash` | F04 | not reconciled |
| REL-032 | `meta-fuzz-hash-001` | `corelink-meta-fuzz→corelink-hash` | M01, M02, M03 | not reconciled |
| REL-033 | `worker-fuzz-hash-001` | `corelink-worker-fuzz→corelink-hash` | W01, W02, W03 | not reconciled |
| REL-034 | `hash-ci-fuzz-pr-001` | workflow `fuzz-smoke`→hash fuzz | S12 | not reconciled |
| REL-035 | `hash-ci-wasm-001` | workflow `wasm-build`→`corelink-hash` | S12 | not reconciled |
| REL-036 | `hash-fuzz-cache-001` | workflow cache→hash fuzz workspace | S12 | not reconciled |
| REL-037 | `hash-container-adapter-cache-001` | `corelink-server` adapter cache→hash | server source anchor | not reconciled |
| REL-038 | `hash-container-byok-tcs-001` | `corelink-server` BYOK TCS→hash | server source anchor | not reconciled |
| REL-039 | `hash-container-byok-transition-001` | `corelink-server` BYOK transition→hash | server source anchor | not reconciled |
| REL-040 | `hash-container-billing-payload-001` (renamed from the pre-recapture `...billing-image-001`) | `corelink-server` canonical billing-payload fingerprint→hash | current-main source recaptured at `b9b3ee8`; see peer census | not reconciled |
| REL-041 | `hash-container-r2-parts-001` | `corelink-server` multipart helper→hash | server source anchor | not reconciled |
| REL-042 | `hash-client-ffi-001` | `corelink-client-verify` FFI alias→hash | client-verify source/header anchors | not reconciled |

The canonical artifact remains the authority for relation semantics and source
citations. This table proves only that the omitted IDs have stable names,
qualified endpoints and explicit unresolved state; it does not prove Cargo
resolution, runtime reachability, peer approval or production behavior.
