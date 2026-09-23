---
name: own-corelink-pat
description: >-
  Use when changing CoreLink PAT primitive contracts: canonical token format,
  Argon2id PHC mint/verify, HMAC-SHA256 fast-fail, scope bits, or secret-bearing
  PAT types. This is source/static guidance, not key custody, token-store,
  authorization-wiring, timing, deployment, or runtime evidence.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-pat
  manifest: crates/corelink-pat/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-pat-structural-normalization-20260921
---

# Ownership — corelink-pat

Candidate ownership guide based on checked crate source and static manifests.
It does not establish configuration, secret custody, stored rows, request
latency, authorization, deployment, test execution, or cold review.

[Trigger](#s01) · [Boundary](#s02) · [Read](#s03) · [Format](#s04) · [Crypto](#s05) · [Scopes](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A PAT format, mint, verify, signing, type, or scope change is requested | Open its reference contract before editing | `src/lib.rs`; [R04](../../../docs/ownership/crates/corelink-pat/REFERENCE.md#r04) | Work is principally key custody, store persistence, or request authorization |

<a id="s02"></a>
## S02 — Boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Ownership is uncertain | Limit the work to the crate's primitive and exported type surface | `src/{argon,error,format,mint,scopes,sig,types,verify}.rs` | Treat a source comment, manifest edge, or token-looking fixture as runtime proof |

<a id="s03"></a>
## S03 — Read routing

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A contract, invariant, or static consumer is needed | Read [reference](../../../docs/ownership/crates/corelink-pat/REFERENCE.md#r04), then [relations](../../../docs/ownership/crates/corelink-pat/BLAST_RADIUS.md#b03) | `src/lib.rs`; manifest and import search | Infer a complete reverse graph or actual request path |

<a id="s04"></a>
## S04 — Wire format and plaintext boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing segments, lengths, environments, parsing, or plaintext representation | Preserve the canonical parser/mint agreement and the explicit plaintext exposure boundary | `format.rs`; `mint.rs`; `types.rs`; R04–R05 | Compatibility, token migration, or plaintext delivery behavior needs evidence beyond source |

<a id="s05"></a>
## S05 — Cryptographic primitive boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing Argon2id, PHC validation, HMAC, comparison, rotation-set, or dummy verification code | Trace the whole local parse → token-id compare → HMAC → Argon2id relation | `argon.rs`; `sig.rs`; `verify.rs`; B02/B04 | Claim a timing envelope, key rotation operation, or actual middleware invocation |

<a id="s06"></a>
## S06 — Scope and error boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing a scope bit, name, mask, error, or public re-export | Reconcile bit construction, masking, names, and the exported error/type surface | `scopes.rs`; `error.rs`; `lib.rs`; R04–R05 | Claim a scope grants access or that an error maps to a wire response |

<a id="s07"></a>
## S07 — Handoff

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Static ownership work is complete | Report baseline, changed records, static consumer evidence, and unknowns | [maintenance](../../../docs/ownership/crates/corelink-pat/MAINTENANCE.md#m06) | Present document checks as semantic approval, runtime evidence, or cold review |

[Back to trigger](#s01)
