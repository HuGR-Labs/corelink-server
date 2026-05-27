---
id: "AUDIT-2026-05-16-invariant-draft-SWEEP"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "invariants", "wave-23", "promotion-sweep"]
---

# Wave-23 Invariant-Draft Promotion Sweep — 2026-05-16

> **Scope**: Surface INV references introduced across Wave-19/20/21/22 code + spec
> changes that were not formally promoted to the canonical
> `specs/03_architecture/invariant_registry.md`. Triage each to one of three
> dispositions: **PROMOTE** (new registry entry), **ALIAS** (map to existing
> canonical), or **ORPHAN-FIX** (fix the stray reference in source).
>
> **Base**: `main` at `043428a` (Wave-22 merge). Registry shows 143/143 stable WI
> coverage since Wave-19, but new INV references may have been added in
> non-WI surfaces (spec_contracts, PRRs, crate doc-comments, public API
> docs) without canonical promotion.

---

## 1. Discovery method

```bash
# All INV-* references across spec sprints + crates + apps
grep -rhoE 'INV-[A-Z][A-Z0-9]+(-[A-Z0-9]+)*' specs/04_sprints/ crates/ apps/ | sort -u > /tmp/all_inv_full.txt

# All INV-* mentioned anywhere in registry (canonical or alias)
grep -oE 'INV-[A-Z][A-Z0-9]+(-[A-Z0-9]+)*' specs/03_architecture/invariant_registry.md | sort -u > /tmp/registry_full.txt

# Diff
comm -23 /tmp/all_inv_full.txt /tmp/registry_full.txt > /tmp/missing.txt
```

26 candidate IDs surfaced. Per-candidate refinement: count **standalone** references (INV-X NOT followed by additional `-WORD`, i.e., not just a prefix of a longer registered ID). The standalone count separates true gaps from regex artifacts.

```bash
for inv in $(cat /tmp/missing.txt); do
  count=$(grep -rohE "${inv}[^A-Z0-9-]" specs/04_sprints/ crates/ apps/ | wc -l)
  count_eol=$(grep -rohE "${inv}$" specs/04_sprints/ crates/ apps/ | wc -l)
  echo "$inv standalone=$((count + count_eol))"
done
```

12 of 26 had standalone count = 0 (pure prefix artifacts of registered longer IDs); 14 had > 0 standalone references requiring triage.

---

## 2. Per-reference triage (14 candidates)

| # | INV ID | Standalone refs | Surface | Disposition | Action |
|---|---|---|---|---|---|
| 1 | `INV-S17-OPS-EXCLUSIVITY` | 1 | `specs/04_sprints/_sealed/S17/_spec_contract.md §8` | **PROMOTE** | Added to registry §3.27 (new OPS domain) |
| 2 | `INV-S17-SEV1-DRILL-PAUSE` | 1 | `specs/04_sprints/_sealed/S17/_spec_contract.md §8` | **PROMOTE** | Added to registry §3.27 (new OPS domain) |
| 3 | `INV-S17-CHAOS-STAGING-ONLY` | 1 | `specs/04_sprints/_sealed/S17/_spec_contract.md §8` | **PROMOTE** | Added to registry §3.27 (new OPS domain) |
| 4 | `INV-S17-ONCALL-FATIGUE-AUTOROTATE` | 1 | `specs/04_sprints/_sealed/S17/_spec_contract.md §8` | **PROMOTE** | Added to registry §3.27 (new OPS domain) |
| 5 | `INV-PAT-REVOKE-PROPAGATION` | 5 | `apps/docs/static/openapi-corelink-v1.yaml` + 4 i18n MDX (`apps/docs/.../delete-v1-pats-by-pat_id.mdx`) | **PROMOTE** | Added to registry §3.28 (new AUTH-PAT revocation domain) |
| 6 | `INV-AUDIT-CHAIN` | 2 | `specs/04_sprints/_sealed/S03/_spec_contract.md v1.3.2` + `specs/04_sprints/_sealed/S03/PRR-S03.md R-S03-007` | **ALIAS** | Added to §5 aliases → `INV-AUDIT-APPEND-ONLY` (shortened form of chain-integrity claim) |
| 7 | `INV-AUDIT-EMIT-ATOMIC` | 16 | 5 Wave-20+ crates: `corelink-statuspage-real`, `corelink-slack-real` (incl. property test), `corelink-region`, `corelink-rotation-adapters`, `corelink-drata-sync` | **ALIAS** | Added to §5 aliases → `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (shortened form widely used in source) |
| 8 | `INV-AUTH-WEBAUTHN` | 2 | `crates/corelink-webauthn/README.md` (explicitly described as "family invariant") | **ALIAS** | Added to §5 aliases → §3.14 AUTH-WEBAUTHN family-collective shorthand (5 canonical INVs: UV-REQUIRED-ADMIN / ATTESTATION-VERIFIED / SIGN-COUNT-MONOTONIC / ORIGIN-EXACT / RP-ID-CANONICAL) |
| 9 | `INV-BLAKE3-256-LOWER-HEX-64` | 1 | `specs/04_sprints/_sealed/S06/_review_R4_opus_part1.md §P3-002-1` (informational review recommendation, never canonicalized) | **ALIAS** | Added to §5 aliases → `INV-CAS-INTEGRITY` (digest canonical-form constraint subsumed by write-time hash check + BLAKE3 deterministic). Audit doc is immutable; alias entry documents the subsumption explicitly. |
| 10 | `INV-AUDIT-CHAIN-001` | 4 | 4 SDK quickstart examples: `crates/corelink-{cli,wasm,py,go}/examples/quickstart_audit.{rs,ts,py,go}` | **ORPHAN-FIX** | Replaced with canonical `INV-AUDIT-APPEND-ONLY` in all 4 examples (Wave-21 SDK docs used fabricated `-001` suffix not present in registry). |
| 11 | `INV-AUDIT-EMIT` | 1 | `crates/corelink-handler-cas/src/handler.rs:462` (test comment) | **ORPHAN-FIX** | Replaced with canonical `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` in test comment. |
| 12 | `INV-CRITICAL` | 1 | `specs/04_sprints/_sealed/S06/_review_R4_opus_part1.md:160` ("INV-CRITICAL crypto-load-bearing WIs") | **NO-ACTION** | Prose artifact (semantic = "CRITICAL-severity INVs"), not a real INV identifier. Audit doc is immutable. No structural fix required; documented here as a known prose artifact for grep-based scans. |
| 13 | `INV-DATA-CLASSIFICATION` | 2 | `specs/04_sprints/S00/_spec_contract.md §88 + §97` | **NO-ACTION** | Already explicitly deprecated in S-00 spec_contract: "meta-rules de processo, não invariantes técnicas formais — promovidas a quality standards (§9)". Already in `scripts/validate_inv_promotion.py` EXCLUDE_INV set. Historical reference preserved intentionally. |
| 14 | `INV-SCOPE-DISCIPLINE` | 2 | `specs/04_sprints/S00/_spec_contract.md §88 + §98` | **NO-ACTION** | Same as #13 (sister meta-rule deprecated to §9 quality standards in S-00 spec_contract Lote 9.4 fix). Already in validator EXCLUDE_INV set. |

---

## 3. Summary counts

- **Total candidates surveyed:** 26 (12 pure-prefix artifacts excluded after standalone-count refinement → 14 real candidates).
- **PROMOTE:** 5 (4 × OPS/S-17 + 1 × AUTH-PAT revoke).
- **ALIAS:** 4 (INV-AUDIT-CHAIN, INV-AUDIT-EMIT-ATOMIC, INV-AUTH-WEBAUTHN, INV-BLAKE3-256-LOWER-HEX-64).
- **ORPHAN-FIX:** 5 source-file references rewritten (4 SDK quickstart examples + 1 handler-cas test comment).
- **NO-ACTION (documented):** 3 (1 prose artifact in immutable audit doc; 2 deprecated meta-rules already in EXCLUDE_INV).

---

## 4. Quality gates (re-validated post-sweep)

```
scripts/validate_inv_promotion.py    → ✅ 143/143 WI coverage (stable; sweep targets non-WI surfaces)
                                        Registry now contains 197 canonical INVs (was 192; +5 PROMOTE)
scripts/validate_references.py       → ✅ 0 dangling references
scripts/validate_specs.py            → ✅ 452 docs validated (443 full schema + 9 YAML-only)
```

---

## 5. Files changed in this sweep

### Registry
- `specs/03_architecture/invariant_registry.md` — version 0.2.1 → 0.2.2; +§3.27 (4 INVs); +§3.28 (1 INV); +5 alias rows in §5; header changelog entry.

### Source-code orphan fixes (5 files)
- `crates/corelink-cli/examples/quickstart_audit.rs` — `INV-AUDIT-CHAIN-001` → `INV-AUDIT-APPEND-ONLY`.
- `crates/corelink-wasm/examples/quickstart_audit.ts` — same.
- `crates/corelink-py/examples/quickstart_audit.py` — same.
- `crates/corelink-go/examples/quickstart_audit.go` — same.
- `crates/corelink-handler-cas/src/handler.rs` — `INV-AUDIT-EMIT` → `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (test comment).

### Audit doc (new)
- `specs/_audits/sealed/2026-05-16-inv-draft-sweep.md` (this doc).

### Not modified
- 5 Wave-20+ crates referencing `INV-AUDIT-EMIT-ATOMIC` (shortened) — covered by new alias entry; no rewrite required (alias is canonical resolution path until 2026-10-24 deprecation window).
- `crates/corelink-webauthn/README.md` — `INV-AUTH-WEBAUTHN` is intentional family-shorthand; covered by alias entry.
- `specs/04_sprints/_sealed/S06/_review_R4_opus_part1.md` — immutable AUDIT doc; references to `INV-BLAKE3-256-LOWER-HEX-64` and `INV-CRITICAL` preserved as historical record.
- `specs/04_sprints/S00/_spec_contract.md` — explicit deprecation prose for `INV-DATA-CLASSIFICATION` / `INV-SCOPE-DISCIPLINE` preserved; no orphan to fix.
- `specs/04_sprints/_sealed/S03/_spec_contract.md` + `specs/04_sprints/_sealed/S03/PRR-S03.md` — `INV-AUDIT-CHAIN` short form covered by alias; no rewrite required.

---

## 6. Followups (NOT in scope for Wave-23)

1. **TLA+ for INV-S17-OPS-EXCLUSIVITY / INV-S17-SEV1-DRILL-PAUSE / INV-S17-CHAOS-STAGING-ONLY**: HIGH severity → property test obligation already satisfied via S-17 integration tests. TLA+ not mandatory per §2 severity matrix (TLA+ is CRITICAL-only mandatory). No model-check obligation created.
2. **TLA+ for INV-PAT-REVOKE-PROPAGATION (CRITICAL)**: marked as PLANNED `auth_pat_revoke.tla` sibling of existing `auth_pat_hybrid.tla` (INV-AUTH-PAT-VERIFY-CONSTANT-TIME). Pre-GA gate per §2 severity matrix; tracking obligation added.
3. **Optional rewrites** of Wave-20+ crates from shortened `INV-AUDIT-EMIT-ATOMIC` to canonical `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`: deferred to Wave-24+ refactor — alias entry provides 6-month deprecation window (2026-10-24); rewrites are mechanical and can run as separate cleanup PR.
4. **Optional rewrite** of `crates/corelink-webauthn/README.md` to enumerate the 5 family INVs explicitly instead of using `INV-AUTH-WEBAUTHN` shorthand: deferred; alias entry is sufficient.

---

**Auditor**: Wave-23 invariant-draft promotion sweep (automated discovery + manual triage).
**Verdict**: ✅ ALL GATES GREEN. Registry now reflects all Wave-19/20/21/22 standalone INV references either as canonical entries (5 new), aliases (4 added), or documented orphan fixes (5 source-file rewrites).
