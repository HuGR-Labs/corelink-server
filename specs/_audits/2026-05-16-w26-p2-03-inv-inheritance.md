---
id: "AUDIT-2026-05-16-W26-P2-03-INV-INHERITANCE"
type: "audit"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "w26", "p2-03", "inv-inheritance", "validator", "registry"]
---

# W26-P2-03 — Structured INV inheritance field + validator

> **Wave / dispatch:** W26-P2-03 (DEFER-POST-GA absorption, wave-30 stream-08 → wave-26 closure).
> **Branch:** `wt/r-prep-w26-p2-03-inv-inheritance`.
> **Parent worktree main:** commit `0f77f48`.
> **Owner:** Gustavo Schneiter.

## §1. Scope + provenance

The wave-26 adversarial review enumerated W26-P2-03 (`specs/_audits/2026-05-16-p2-absorption-sweep-w25-28.md` L49):

> *INV inheritance chains (`INV-CAS-IDEMPOTENCY → cas_integrity.tla`, `INV-GC-004 → InvGCReRefProtected`) are asserted by prose, not machine-readable. A future regression dropping an inherited property would not be caught mechanically. Verdict: DEFER-POST-GA. Requires a structured-field addition to the invariant registry schema + a new validator check. Architectural — not a cosmetic absorption candidate. Recorded in wave-30 residual queue.*

The wave-30 stream-08 P2 absorption sweep classified this as DEFER-POST-GA because it requires a schema addition + validator implementation, not a prose patch. This audit closes the deferral by landing both.

## §2. Pre-fix state (prose-only inheritance)

Pre-W26-P2-03, inheritance was documented in three prose forms scattered across `specs/03_architecture/invariant_registry.md`:

| Pattern | Example | Line |
|---|---|---|
| `"Coberto por <spec>.tla (<Property>)"` | `INV-CAS-IMMUTABILITY` | §4.1 L634 |
| `"Deriva de INV-<…>"` | `INV-AC-TENANT-SCOPED` | §3.3 L93 |
| `"Parent: <spec>.tla"` | `INV-AUTH-AUDIT-PSEUDONYMIZATION` | §4.1 L660 |

The risk model: a rename of `cas_integrity.tla` or deletion of `InvGCReRefProtected` from `gc_correctness.tla` would silently break the inheritance claim. No CI gate would surface the regression. The post-GA fix horizon raised this as architectural debt; W26-P2-03 closes it.

## §3. Schema change

Added a new subsection **§3.30 INV inheritance index (machine-readable structured field)** to `specs/03_architecture/invariant_registry.md`. The section contains a fenced YAML code block headed by the marker comment `# inv-inheritance-index v1` and structured as:

```yaml
chains:
  - inv: INV-CAS-IDEMPOTENCY            # child INV-ID (must be registered in §3.X)
    inherits_from:
      - specs/tla/cas_integrity.tla     # TLA file path
    source: "§4.1 L633 — '…' "          # free-form prose provenance pointer

parents:
  - target: specs/tla/cas_integrity.tla # same value forms as inherits_from
    inherited_by:
      - INV-CAS-IDEMPOTENCY             # reciprocal pointer
```

### Target value forms

- **TLA file path** — `specs/tla/<name>.tla` (validator checks the file exists on disk).
- **TLA property name** — `specs/tla/<name>.tla:<PropertyName>` (`:` separator; validator checks the file exists AND `<PropertyName>` appears as a word-boundary token in the spec source).
- **INV ID** — `INV-<NAME>` (validator checks the INV is registered as a §3.X table row).

### Additivity contract

The schema is OPTIONAL. INVs without an inheritance entry validate exactly as before — no §3.1–§3.29 table row was mutated. The validator only enforces integrity of declared chains; missing chains are not flagged.

### Registry version bump

`version: 0.2.2 → 0.2.3` and the `Versão:` banner moved accordingly. No `doc_status` change (remains `DRAFT`).

## §4. Validator design

New script `scripts/validate_inv_inheritance.py` (267 lines, pure stdlib + pyyaml).

### Where the check lives

A sibling to `validate_specs.py` / `validate_inv_promotion.py`, runnable standalone:

```
python3 scripts/validate_inv_inheritance.py [--registry PATH] [--repo-root PATH] [--verbose]
```

Sibling rationale (not folded into `validate_specs.py`): the existing script validates YAML front matter against `front_matter.schema.json` for **every** spec file. The inheritance index is a single fenced block inside one specific file. Keeping it separate keeps each validator's responsibility cohesive and the failure messages targeted.

### Integrity rules

The validator enforces three rules and exits 1 on any failure:

1. **Target existence.** Every `inherits_from` entry must resolve:
   - file path → file exists on disk
   - file:property → file exists AND property appears as a word-boundary token
   - INV ID → INV is defined in a §3.X table row
2. **Bidirectional integrity (forward).** For every `(child, parent)` in `chains[*].inherits_from`, `parents[parent].inherited_by` must list `child`.
3. **Bidirectional integrity (inverse).** For every `(parent, child)` in `parents[*].inherited_by`, `chains[child].inherits_from` must list `parent`.

### Sample error messages

| Failure | Message form |
|---|---|
| Missing TLA file | `[INV-CAS-IDEMPOTENCY] inherits_from TLA spec file 'specs/tla/cas_integrity.tla' does not exist on disk` |
| Missing property | `[INV-GC-004] inherits_from property 'InvGCReRefProtected' not found in specs/tla/gc_correctness.tla` |
| Unknown INV parent | `[INV-AC-TENANT-SCOPED] inherits_from INV target 'INV-NOT-REGISTERED' is not registered in any §3.X table row` |
| Asymmetric (forward) | `[INV-X] inherits_from cites parent 'P' but parents[P].inherited_by does not list 'INV-X' (asymmetric link)` |
| Asymmetric (inverse) | `[parents[P]] inherited_by lists 'INV-X' but 'INV-X' has no chain entry citing 'P' (asymmetric link)` |
| Missing inheritance index | Soft-pass (additivity) with a note printed; rc=0 |

### CI wiring guidance

`validate_inv_inheritance.py` should run in the same CI step as `validate_specs.py` + `validate_inv_promotion.py`. Exit code propagates as the gate signal. The current change does **not** modify CI workflows — that wiring will land in the next infra dispatch alongside the rest of the validator chain ordering review (out of scope for W26-P2-03 charter).

## §5. Chains converted

Sixteen child chains were converted from prose to structured fields, spanning **8 distinct parent targets**. Every chain entry has a `source:` prose-pointer back to the exact registry line that already documented the relationship — i.e. no chain was speculatively invented (per W26-P2-03 charter "do NOT speculatively add chains").

| # | Child INV | Parent target | Provenance |
|---|---|---|---|
| 1 | `INV-CAS-IDEMPOTENCY` | `specs/tla/cas_integrity.tla` | §4.1 L633 |
| 2 | `INV-CAS-IMMUTABILITY` | `specs/tla/cas_integrity.tla:InvCASImmutability` | §4.1 L634 |
| 3 | `INV-DIGEST-VERIFICATION` | `specs/tla/cas_integrity.tla:InvPoisoningRejected` | §4.1 L639 |
| 4 | `INV-AC-TENANT-SCOPED` | `INV-TENANT-ISOLATION` | §3.3 L93 + §4.1 L635 |
| 5 | `INV-GC-004` | `specs/tla/gc_correctness.tla:InvGCReRefProtected` | §3.4 L102 + §4.1 L637 |
| 6 | `INV-GC-MARK-STARTED-AT-IMMUTABLE` | `specs/tla/gc_correctness.tla` | §3.X L349 |
| 7 | `INV-GC-MARK-STARTED-AT-ATOMIC` | `specs/tla/gc_correctness.tla` | §3.X L351 |
| 8 | `INV-GC-REACHABLE-SET-COMPLETE` | `specs/tla/gc_correctness.tla` | §4.1 L663 (Parent annotation) |
| 9 | `INV-KEY-AUDIT` | `specs/tla/audit_immutability.tla` | §3.13 L196 |
| 10 | `INV-AUTH-AUDIT-PSEUDONYMIZATION` | `specs/tla/audit_immutability.tla` | §4.1 L660 (Parent annotation) |
| 11 | `INV-SUB-PROCESSOR-AUDIT-FAIL-CLOSED` | `specs/tla/audit_immutability.tla` | §4.1 L664 + §3.X L413 + §4.3 L721 |
| 12 | `INV-OFFBOARDING-AUDIT-COMPLETE` | `specs/tla/audit_immutability.tla` | §3.X L534 |
| 13 | `INV-BILLING-PORTAL-AUDIT-FAIL-CLOSED` | `specs/tla/audit_immutability.tla` | §3.X L455 |
| 14 | `INV-AUTH-REVOCATION-SLO-60S` | `specs/tla/auth_revocation.tla` | §4.1 L641 |
| 15 | `INV-AUTH-MASS-REVOKE-ATOMIC` | `specs/tla/auth_revocation.tla` | §4.1 L642 |
| 16 | `INV-AUTH-PROPAGATION-AT-LEAST-ONCE` | `specs/tla/auth_revocation.tla` | §4.1 L643 |

### Not converted (intentional)

The following prose forms describe weaker semantic relationships than inheritance and were left out of the structured index pending a separate field design:

- `"subsumed by"` / `"subsumido por"` (e.g. `INV-DATA-BILLING-RECONCILE` subsumed by `INV-BILLING-RECONCILE-3-LAYER`) — substitution, not inheritance.
- `"sibling of"` (e.g. `auth_pat_revoke.tla` sibling of `auth_pat_hybrid.tla`) — peer relationship, not inheritance.
- `"indirectly covered by"` (e.g. `INV-AVAIL-ISOLATION` indirectly via 5-layer TLA defense) — too loose for the strict existence semantics of `inherits_from`.

These are real relationships but deserve their own structured field if/when post-GA refactors call for it. The W26-P2-03 charter is explicit: only convert chains "clearly documented" — and these don't pass that bar.

## §6. Test net

New file `tests/validate_inv_inheritance_test.py` with **11 pytest cases** covering:

| # | Test | What it pins |
|---|---|---|
| 1 | `test_happy_path_inv_to_inv` | Pure INV→INV chain validates rc=0 |
| 2 | `test_happy_path_inv_to_tla_property` | INV→`file:Property` chain validates when both exist |
| 3 | `test_missing_tla_file` | `inherits_from` cites missing file → error message contains path + "does not exist" |
| 4 | `test_missing_property_in_tla` | `inherits_from` cites absent property in real file → error |
| 5 | `test_unknown_parent_inv` | `inherits_from: INV-UNREGISTERED` → error |
| 6 | `test_asymmetric_child_missing_in_parent` | Child cites parent; parent omits child → "asymmetric link" error |
| 7 | `test_asymmetric_parent_lists_unknown_child` | Parent lists a child with no chain → "asymmetric link" error |
| 8 | `test_absent_index_passes_vacuously` | Registry with no index block validates rc=0 (additivity) |
| 9 | `test_empty_inherits_from_rejected` | Schema-form rejection: empty `inherits_from` list |
| 10 | `test_invalid_target_form` | Garbage target form rejected |
| 11 | `test_live_registry_validates_green` | The real `invariant_registry.md` post-W26-P2-03 validates rc=0 |

The synthetic-registry fixtures use a minimal template with three INV table rows (`INV-TEST-PARENT` / `INV-TEST-CHILD` / `INV-TEST-OTHER`) and an inline yaml-fenced index. Each test patches one specific rule and asserts the validator surfaces the expected error.

## §7. Gates + results

| Gate | Result |
|---|---|
| `python3 scripts/validate_inv_inheritance.py --verbose` (live registry) | rc=0 — 16 child chain(s) across 8 parent target(s) |
| `python3 scripts/validate_specs.py` (additivity smoke) | rc=0 — 449 schema OK + 9 YAML-only OK (458 total) |
| `python3 scripts/validate_inv_promotion.py` (additivity smoke) | rc=0 — 143/143 WI-declared INVs registered |
| `python3 -m pytest tests/validate_inv_inheritance_test.py -v` | 11/11 passed in 0.29s |

No regression in the existing validator chain. The new index is additive — every prior validator that walked the registry continues to pass.

## §8. DCO sign-off

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
