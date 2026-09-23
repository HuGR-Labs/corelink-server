---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-r2-multipart
manifest: crates/corelink-r2-multipart/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: author_validated
evidence_set: r2-multipart-static-source-20260920
---

# corelink-r2-multipart — maintenance

Static-source maintenance guide for the multipart trait, fakes, canonical keys,
and concurrency source. It authorizes no Cargo/build/test, provider/network
access, R2 operation, D1 change, credential use, deployment, or review claim.

[Baseline](#m01) · [Classify](#m02) · [Trait/fake](#m03) · [Key/bounds](#m04) ·
[Relations](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Baseline and mode

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_LOCAL | Scoped checkout, expected revision, manifest, and target sources are identified | Assessed paths match the recorded package and baseline | Stop on divergent baseline; obtain the reconciled scope without reset | SHA, branch, path inventory; SOURCE |

<a id="m02"></a>
## M02 — Classify the requested change

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_LOCAL | Targeted symbol is known | It maps to trait/fake, key/type, bounds/concurrency, or failover source in R03 | Stop if provider, storage, worker, or deployment behavior is required; route to that owner | [R03](REFERENCE.md#r03), [R07](REFERENCE.md#r07); SOURCE |

<a id="m03"></a>
## M03 — Preserve trait and fake source contracts

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_TRAIT_FAKE | Method/type change is identified | Trait method signatures, handle fields, fake exports, and affected REL-004/005 are compared | Stop for provider conformance or consumer compatibility approval; obtain implementation/consumer evidence | `src/{adapter,types,in_memory,lib}.rs`, INV-001/002/007; SOURCE |

<a id="m04"></a>
## M04 — Preserve key, bound, and concurrency source predicates

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_KEY_BOUND | Composer, part, bucket, or permit change is identified | Typed composer shape, bound reference, bucket mapping, and tenant-keyed permit source remain explicitly compared | Stop for storage isolation, R2 acceptance, performance, or fairness claim; leave it unknown | `src/{object_key,types,bounds,concurrency}.rs`, INV-003–006; SOURCE |

<a id="m05"></a>
## M05 — Assess static relations and scope

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_RELATIONS | Changed symbol and bounded search scope are recorded | Manifest edges, CAS re-export, and cf-bindings adjacency are classified separately | Stop if a complete graph, selected features, or runtime route is required; request fresh bounded evidence | B01–B06 and cited paths; SOURCE |

<a id="m06"></a>
## M06 — Static handoff and explicit unknowns

| Mode | Prerequisite | Expected predicate | Stop / recovery | Evidence state |
|---|---|---|---|---|
| STATIC_HANDOFF | Only owned artifacts changed and required sections navigate correctly | Report baseline, paths, changed contracts/invariants/relations, checker results, diff result, and unknowns | Repair owned documentation only if a structural/scope check fails; do not change shared source or manifest | Four structural checker invocations below and `git diff --check`; SOURCE |

Run the four exact profile-S structural checks with
`docs/ownership/tools/check_docs.py`:

```sh
python3 docs/ownership/tools/check_docs.py .claude/skills/own-corelink-r2-multipart/SKILL.md --kind skill --profile S --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-r2-multipart/REFERENCE.md --kind reference --profile S --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-r2-multipart/BLAST_RADIUS.md --kind blast_radius --profile S --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-r2-multipart/MAINTENANCE.md --kind maintenance --profile S --root .
```

Those checks are structural only; a passing result does not certify semantic
completeness, runtime behavior, execution, profile eligibility, or cold review.

The handoff must distinguish SOURCE evidence from executed/runtime evidence.
Required unknowns are resolved features/targets, complete consumers, provider
implementation/conformance, R2 calls and persistence, worker mounting, D1,
object durability, sweeper operation, concurrency behavior, credentials,
deployment, and cold review. An author documentation pass is neither Cargo
execution, test success, runtime observation, nor approval. WAVE_007_PLAN and
canonical OKF remain route/reference only and are not redefined or revalidated.

[Reference](REFERENCE.md#r01) · [Relations](BLAST_RADIUS.md#b01) · [Start](#m01)
