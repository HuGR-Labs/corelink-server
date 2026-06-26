---
title: "OKF-CoreLink Profile v0.1 — FROZEN CONTRACT"
type: "Contract"
description: "The frozen interface every concept doc and the validator transcribe against. OKF v0.1 + two required grounding extensions. Agents do not deviate from this."
status: "FROZEN — cold-critic FREEZE-OK (2026-06-26). Campaign 2 complete; profile_version 0.1 locked."
okf_base: "0.1"
profile_version: "0.1"
baseline_sha: "6d670131b94ffd6c4ff197a05747c5d42b3a77f2"
tags: ["okf", "contract", "frozen", "knowledge-wiki"]
---

# OKF-CoreLink Profile v0.1 — FROZEN CONTRACT

> **This is a frozen contract (techlead Pillar D).** It is the código-âncora the concept-authoring
> agents and the validator agent **transcribe**, not design. Any change to this file is a contract
> revision and requires the lead's explicit re-freeze + a bump of `profile_version`. An agent that
> deviates has failed the WP. Ambiguity here is the lead's bug to fix, never the agent's to resolve.

Profile = **OKF v0.1 (verbatim base)** + **two required grounding extensions** (`source_files`,
`checkpoint_sha`) + a **frozen path taxonomy** + a **frozen validator acceptance suite**. The OKF base
explicitly permits extra frontmatter keys and requires consumers to preserve them, so this profile is
OKF-conformant by construction.

---

## 1. Bundle layout (FROZEN)

- Bundle root: **`docs/knowledge/`** (created by W-D). This contract + the roadmap live OUTSIDE the
  bundle, in `docs/internal/okf-wiki/`, so the bundle stays clean and conformant.
- Reserved files (root only): **`index.md`** (machine-generated listing), **`log.md`** (machine
  appended change history). These are NEVER concept docs.
- Concept ID = bundle-relative path minus `.md`. Cross-links use bundle-relative paths with a leading
  `/` (e.g. `[PAT gauntlet](/auth/pat-gauntlet.md)`).
- A `references/` subdir MAY hold immutable cited artifacts (diagrams, captured external docs).

### 1.1 Path taxonomy (FROZEN — concepts MUST live under one of these)

| Dir | Holds | `type` values |
|-----|-------|---------------|
| `planes/` | Worker / DO / Container planes + the request flow | `Plane` |
| `surfaces/` | The 6 cache surfaces (CAS, AC, Bazel, Turbo, sccache, _public) | `CacheSurface` |
| `auth/` | PAT moat, HMAC reject, Argon2id, D1 store, introspect | `AuthMechanism` |
| `storage/` | R2 bucket topology, D1 CONFIG_DB, hot-path latency | `StorageComponent` |
| `tenancy/` | Isolation, quota, $-ceiling, governance | `TenancyControl` |
| `flows/` | CAS write, PAT gauntlet, billing check, introspection | `RequestFlow` |
| `crates/` | The 8 crate clusters | `CrateCluster` |
| `adr/` | Re-indexed ADRs (one concept per ADR) | `ADR` |
| `ops/` | Deploy, secrets lifecycle, runners, GC/eviction | `Runbook` |
| `security/` | Posture, pentest learnings, attack-surface | `SecurityControl` |
| `compliance/` | DSR/erasure, residency, sub-processors, audit chain | `ComplianceControl` |
| `testing/` | e2e strategy, real-user suites, gates | `TestStrategy` |
| `launch/` | Go-live readiness, money-path, tier model | `LaunchControl` |

New `type` values are allowed (OKF requires consumers tolerate unknown types) but SHOULD be added to
this table via a contract revision so the taxonomy stays curated.

---

## 2. Frontmatter schema (FROZEN)

### 2.1 REQUIRED on every concept doc
| Key | Type | Rule |
|-----|------|------|
| `type` | string | Non-empty. From the taxonomy table §1.1 (or a contract-revised addition). |
| `title` | string | Non-empty. (We tighten OKF's optional → required.) |
| `source_files` | list[string] | **≥1.** Each = a repo-relative path that MUST resolve to an existing file. The code/docs this concept is grounded in. **Extension key.** |
| `checkpoint_sha` | string | 40-hex. The commit this concept was last reconciled against; MUST exist in git history. **Extension key.** |

### 2.2 OPTIONAL
| Key | Type | Use |
|-----|------|-----|
| `description` | string | One sentence. |
| `tags` | list[string] | Free. |
| `timestamp` | string | ISO-8601, last meaningful change. |
| `provenance` | enum | `AUTHORED` (hand-written seed) \| `GENERATED` (LLM-synthesized). Default `AUTHORED`. GENERATED concepts MUST carry the marker. |
| `supersedes` / `superseded_by` | string | Concept path; for ADR-style evolution. |
| `deferred` | string | A reason. Marks a concept candidate intentionally not-yet-written. A doc with `deferred` is exempt from body/grounding requirements but MUST still have `type` + `title`. **This is the only sanctioned way to leave a candidate uncovered — no silent omission.** |

### 2.3 Reserved `index.md` frontmatter
`okf_version: '0.1'`, `profile_version: '0.1'`, plus `type: Index`. Generated; never hand-edited.

---

## 3. Body conventions (FROZEN)

1. **Lead paragraph** = the *why* / the role of the concept in the system (the knowledge rustdoc can't hold).
2. **Structural markdown** — headings/lists/tables/fenced code. No prose-padding.
3. Conventional headings when applicable: `# Role`, `# How it works`, `# Invariants`, `# Gotchas`, `# Citations`.
4. **`# Citations` is REQUIRED** for every non-`deferred` concept. Numbered list. **Every path in
   `source_files` MUST appear at least once** as a `path:line` or `path:line-line` citation. Citations
   MAY also point to ADRs (`/adr/adr-0033.md`), other concepts, or `references/` artifacts.
5. Inline code-anchor format: `crates/foo/src/bar.rs:42` or `:42-58`. The cited file MUST exist and the
   line(s) MUST be within the file's bounds at `checkpoint_sha`.
6. **GENERATED concepts** carry a top-of-body marker: `> GENERATED — do not hand-edit. Reconcile via the updater.`

---

## 4. Validator acceptance suite (FROZEN — this IS the completeness oracle)

> **Honest guarantee (corrected after cold-critic, do not overclaim):** the gate guarantees freedom
> from **declared-dependency drift** — a concept cannot go stale relative to the files it *declares*
> in `source_files` and still pass. It does **not** prove `source_files` is *complete* (behavior could
> move to an undeclared file). Three mechanisms bound that residual risk: C6b (cited ⊆ declared),
> C-REV (nightly reverse-coverage WARN), and C-AGE (a secondary time backstop). The trust root that
> remains — "the author declared the right dependencies" — is mitigated, not eliminated; it is the
> single biggest residual risk and is logged as such.

The validator `scripts/validate_okf.py` (WP W-B) MUST enforce the checks below. **Validator
completeness = each check has ≥1 known-BAD fixture it catches AND the known-GOOD golden bundle passes
all checks.** The validator — not the slice — is the done-oracle. All structural checks validate the
**working tree at HEAD** (the thing being gated); `checkpoint_sha` is used ONLY as the diff base for
freshness.

| ID | Check | FAIL condition |
|----|-------|----------------|
| **C1** Conformance | Every non-reserved `.md` has parseable YAML frontmatter with non-empty `type`. | Missing/empty `type`, unparseable frontmatter. |
| **C2** Required fields | `title`, `source_files` (≥1), `checkpoint_sha` present (unless `deferred`). | Any required field absent on a non-deferred concept. |
| **C3** Source resolves | Every `source_files` path exists **verbatim** at HEAD. Renames do NOT auto-follow — a renamed-away declared path FAILS, forcing the author to correct `source_files` (accuracy of the dependency set is the point). | A declared path does not exist verbatim at HEAD. |
| **C4** Checkpoint valid | `checkpoint_sha` is 40-hex and exists in git history. | Malformed or unknown SHA. |
| **C5** Freshness (anti-drift, declared+line-scoped) | For each `source_files` path: **two-tree `git diff <checkpoint_sha> HEAD -- <path>`** over the WHOLE file (merge-safe), **then post-filter to the diff hunks that intersect the concept's cited line ranges** for that file. **Implementation MUST NOT use `git log -L`/blame** (line-history reintroduces the merge-simplification B2 removed). No intersecting hunk ⇒ fresh. | A cited line-range's content changed since the checkpoint → concept STALE → FAIL (the bound). Edits OUTSIDE all cited ranges do NOT trigger (anti-churn). |
| **C5b** No phantom reconcile | If a PR advances a concept's `checkpoint_sha`, that concept's body MUST also change in the same PR. | SHA bumped without a body edit (reconcile-without-reconciling). |
| **C6** Citation resolves | Every inline `path:line[-line]` cite: file exists at HEAD, line(s) in bounds; every `source_files` path appears ≥1× under `# Citations`. | Dangling cite, out-of-range line, or an ungrounded declared source. |
| **C6b** Cited ⊆ declared | Every file appearing in any inline `path:line` cite is in `source_files`. | A cited file not declared (would escape C5 freshness). |
| **C6c** Per-claim grounding | Every bullet/line under `# How it works` and `# Invariants` contains ≥1 `path:line` token. | An ungrounded claim bullet (mechanical proxy against hallucinated prose). |
| **C7** Link integrity | Every bundle-relative link to a **declared** concept resolves. (Links to not-yet-written/`deferred` concepts tolerated per OKF.) | A link to a declared concept path with no file. |
| **C8** Orphan | Every concept reachable from `index.md`. | An unreachable concept. |
| **C9** Reserved names | `index.md`/`log.md` never concept docs; no concept uses a reserved path. | Violation. |
| **C10** Manifest completeness | Every candidate in `concept-manifest.yaml` with **`status: active`** has a concept doc OR a `deferred:` marker. Candidates `status: planned` (not-yet-dispatched backlog) do NOT block — they let the foundation be green before the fill waves run; a concept-authoring WP flips its candidates to `active` and lands the docs in the same PR. | An `active` candidate with neither. |
| **C10b** Manifest ⊇ repo surface | The manifest itself is cross-checked against the mechanically-enumerable surface: `specs/03_architecture/adrs/*.md` (76 ADRs), `crates/corelink-container/src/routes/*.rs`, and each `crates/*` dir → each maps to a manifest entry or an explicit `excludes:` list with a reason. | A repo-surface item with no manifest entry and no exclude (silent gap one level up). |

**Secondary (nightly, WARN — not a per-PR blocker; catches undeclared-dependency rot):**
- **C-AGE** — a non-`deferred` concept whose `checkpoint_sha` is older than **90 days** is flagged for
  human re-attestation (the documented day-60 "confident-but-stale" failure mode; time as a *backstop*,
  never the primary gate — see D4).
- **C-REV** (reverse-coverage) — for each crate cluster, if files changed in the cluster since the
  oldest concept checkpoint referencing it and no concept's `source_files` covers them, WARN (pressure
  toward `source_files` completeness; cannot be proven, only surfaced).

**Exit contract (mirrors `validate_specs.py`):**
- `0` → stdout: `"✅ OKF-CoreLink profile valid: <N> concepts, <D> deferred, 0 stale, 0 drift"`.
- non-zero → stdout: per-offender list grouped by check ID, then `"⛔ OKF-CoreLink profile INVALID: <k> failures"`.
- (String says *profile*-valid, not OKF-conformance — C2-strict/C6c/C8 are stricter-than-OKF profile rules; the bundle stays OKF-portable because we only ADD requirements and never reject on tolerated conditions.)

**Gate wiring (W-C):** `.github/workflows/okf_wiki.yml`, `on: pull_request`, paths
`['docs/knowledge/**','crates/**','specs/**','scripts/validate_okf.py']`, `runs-on: ubuntu-latest`
(zero Mac load), **`actions/checkout` with `fetch-depth: 0`** (C4/C5 need full history; shallow
checkout errors). Step `python3 scripts/validate_okf.py`. Auto-surfaces in `gh pr checks` →
`pre-merge-gate-check.sh`. C5 firing on a PR that touched a cited line-range is the anti-drift
mechanism: the author reconciles the concept and advances `checkpoint_sha` in the same PR (and C5b
forces a real body edit, not just a SHA bump).

### 4.1 ADR & doc-extraction sub-profile (FROZEN — closes the maximal-scope gap)

The 35 architecture concepts are code-grounded. The ratified ADR re-index and docs/ extraction need
their own grounding/freshness semantics, else 76+ agents guess:

- **ADR concepts (`adr/`, `type: ADR`):** `source_files` = the ADR file itself **plus** any code paths
  the ADR governs (if cited). Freshness for an **accepted/immutable** ADR is NOT code-drift — an
  accepted decision does not go stale when code evolves. C5 applies only to the ADR file's own content;
  a superseded ADR MUST carry `superseded_by`. C6c is relaxed for ADRs (decision prose, not mechanics).
- **Doc-extracted concepts (`security/`,`compliance/`,`testing/`,`launch/`,`ops/`):** `source_files` =
  the source doc(s) under `docs/**` **plus** any cited code. C5/C6 apply to both. If the concept makes
  operational claims about code, those claims fall under C6c like any other.
- Both sub-types are enumerated in `concept-manifest.yaml` and gated by C10/C10b like architecture
  concepts.

---

## 5. Invariants (INVIOLABLE — a breach is a RED gate, never waived without explicit human authorization)

1. Non-empty `type` on every non-reserved concept (C1).
2. ≥1 resolving `source_files` path on every non-deferred concept (C2/C3).
3. A valid `checkpoint_sha`, and the concept is **not stale** on its declared+cited line ranges (C4/C5); a SHA bump implies a real body reconcile (C5b).
4. Every inline `path:line` cite resolves; every `source_files` path is cited; every cited file is declared (C6/C6b); every `# How it works`/`# Invariants` claim carries a `path:line` (C6c).
5. No orphan concepts; no broken links to declared concepts (C7/C8).
6. Reserved filenames never used as concepts (C9).
7. GENERATED layer marked and never hand-edited (§3.6).
8. No concept candidate silently omitted — `deferred:` or a doc, never nothing (C10); the manifest itself covers the enumerable repo surface (C10b).

---

## 6. What the lead still freezes in Campaign 2 (before any fan-out)

- [ ] `02-concept-template.md` — the per-concept stub agents fill (this contract's companion).
- [ ] `concept-manifest.yaml` — the full enumerated candidate list (35 arch + 80+ ADRs + the
      doc-extraction candidates) with target path + type + source cluster per candidate. The C10 oracle.
- [ ] The validator's red/green **fixture corpus spec** (the acceptance suite W-B builds to).
- [ ] Cold suite-critic sign-off that the above covers the maximal ratified demand.

Only then does Campaign 3 dispatch W-B/W-C/W-D, followed by the rolling concept-authoring waves.
