---
name: own-corelink-eviction
description: >-
  Own source-only changes to corelink-eviction's eviction decisions, LRU and TTL
  policy, quota trigger logic, state traits, and in-memory fakes. Use to route
  static contract work; do not use it to assert runtime or Cloudflare behavior.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-eviction"
  manifest: "crates/corelink-eviction/Cargo.toml"
  source-commit: "6ed297f5b2b64cf97447985111a2ecbbaa9536bb"
  evidence-set: "eviction-source-static-20260920"
---

# Ownership — corelink-eviction

Candidate source-only ownership for `corelink-eviction`. This skill records static Rust and test-source evidence; it neither executes nor certifies a backend, scheduler, D1, Cloudflare, or production eviction path.

[Use](#s01) · [Authority](#s02) · [Read](#s03) · [Decide](#s04) · [Flow](#s05) · [Stop](#s06) · [Deliver](#s07).

<a id="s01"></a>
## S01 — When to use

Use this skill for `EvictionPhase`, candidate ordering and decisions, tier or reservation TTL calculations, quota-trigger boundary logic, `TenantStorageStateStore`, `BlobMetaSoftDeleteStore`, `AcReferenceProbe`, and their in-memory implementations. Do not use it as the owner of physical deletion, chunk lifecycle, quota-accounting authority, deploys, or an actual persistence/runtime adapter.

<a id="s02"></a>
## S02 — Authority and boundary

The package identity and manifest are `corelink-eviction` and `crates/corelink-eviction/Cargo.toml`. It owns the static contracts exported from `src/lib.rs` and source-local fakes. Canonical verified OKF material remains a routed reference at [OKF profile](../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md); do not copy, revalidate, or redefine it here. The ownership boundary ends at traits and source-visible algorithms.

<a id="s03"></a>
## S03 — Reading route

| Question | Read |
|---|---|
| What is in scope and what is unknown? | [R01](../../../docs/ownership/crates/corelink-eviction/REFERENCE.md#r01) |
| Which invariant is falsifiable? | [R05](../../../docs/ownership/crates/corelink-eviction/REFERENCE.md#r05) |
| What changes together? | [B01–B06](../../../docs/ownership/crates/corelink-eviction/BLAST_RADIUS.md#b01) |
| What evidence is permitted now? | [M01](../../../docs/ownership/crates/corelink-eviction/MAINTENANCE.md#m01) |
| What is incomplete? | [B06](../../../docs/ownership/crates/corelink-eviction/BLAST_RADIUS.md#b06) |

<a id="s04"></a>
## S04 — Decision form

| If the change concerns | Decide and record | Stop when |
|---|---|---|
| LRU or TTL | cutoff comparison, tier/default/override, and boundary examples | the changed comparison has no falsifiable source assertion |
| quota trigger | 95% inclusive arm, 90% reclaim target, and zero-quota outcome | an external counter is being represented as observed runtime fact |
| reachability | tenant key, strict timestamp boundary, and soft-delete decision | a request would alter chunk or physical-delete ownership |
| state mutation | key, monotonic watermark, reclaim arithmetic, and error outcome | backend atomicity is assumed beyond the trait/fake |
| fake/test | which implementation is exercised and which real adapter remains unknown | fake evidence is relabeled runtime evidence |
| public surface | affected export, error compatibility, and B01–B06 relation | a consumer or integration is untraced |

**Invariants:** retain tenant-plus-region scope for state, tenant scope for blob/reference operations, strict `<` in the reachable probe, soft-delete idempotence, and bounded TTL calculations. These are source claims only; R05 names their evidence and limits.

<a id="s05"></a>
## S05 — Work flow

1. Confirm the manifest and source baseline named in this skill.
2. Route to the applicable R, B, and M sections before changing a contract.
3. State the source evidence and the execution/runtime evidence state separately.
4. Preserve or update the falsifiable invariant and its test-source location.
5. Record unknown adapters, callers, and operational behavior rather than filling gaps by inference.
6. Request independent review for cross-package, storage, or lifecycle ownership changes.

<a id="s06"></a>
## S06 — Stop conditions

Stop and escalate when a task requires a real scheduler, D1/Cloudflare behavior, physical object deletion, production quota authority, an uninspected consumer, or verified OKF interpretation. A trait, embedded migration text, comment, or in-memory fake is not execution evidence. Do not use a static source result to claim runtime success.

<a id="s07"></a>
## S07 — Delivery standard

**Success criteria:** the change has a scoped source rationale, a falsifiable invariant, and explicit unknowns. **Completeness criteria:** every touched export and B01–B06 relation is reconciled or marked unknown. **Quality standards:** source links resolve, claims distinguish SOURCE from execution/runtime, and no runtime behavior is asserted without execution evidence. **Definition of Done:** manifest, source symbols, affected invariant, evidence state, review route, and recovery implication are recorded; no external behavior is certified by this skill.

**Candidate documentary checker procedure:** from the repository root, run the four invocations below after an ownership-doc change and retain their JSON verdicts.
```sh
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-eviction/SKILL.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-eviction/REFERENCE.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-eviction/BLAST_RADIUS.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-eviction/MAINTENANCE.md
```
This is an external, unintegrated candidate checker. A passing result is structural only: it is not semantic-completeness validation, cold approval, or execution/runtime proof.

[Return to use](#s01)
