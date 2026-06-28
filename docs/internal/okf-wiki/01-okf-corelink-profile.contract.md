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
6. **Cite the in-repo LEAF enforcer, never a line that DELEGATES to an uncited callee
   (AUTHORING RULE — closes the C5 "callee-swap" bypass).** C5 freshness is mechanical: it re-reads the
   cited *lines*, it cannot follow a function call. So if a concept anchors an invariant on a line that
   merely *calls* a helper, and the real enforcement lives in that helper in an **uncited** file, an
   author can rewrite the helper (changing the behaviour) while the cited call line stays byte-identical
   — C5 still reports "fresh." Therefore: **an invariant's inline anchor cite MUST point at the in-repo
   LEAF enforcer — the line that PERFORMS the check**, where "leaf" = the deepest line in *our own* code
   on the call chain; an external-crate call (e.g. `subtle::ct_eq`, `hmac`) at that line IS the leaf (we
   do not chase a cite into a third-party dependency we do not version-pin in `source_files`). Every file
   on the full verify chain SHOULD appear in `source_files` (and so be C5-gated); the delegating /
   intermediate-wrapper call sites MAY be cited *in addition*, never *instead* of the leaf.
   (Worked example: the PAT moat's HMAC fast-reject delegates `adapter_pat.rs` → `verify_hmac_only_multi`
   in `crates/corelink-pat/src/verify.rs` → `verify_hmac_sig_multi` in `crates/corelink-pat/src/sig.rs`.
   `verify_hmac_only_multi` is ITSELF a wrapper (it just calls onward), so the leaf is one hop deeper: the
   `ct_eq` constant-time fold in `sig.rs`. The concept's invariant anchors on the `sig.rs` fold; verify.rs
   and the adapter call site are cited only as call-chain context — audit #2 corrected an earlier draft
   that stopped one hop shallow at the verify.rs wrapper.)

   **ACCEPTED STRUCTURAL RESIDUAL of content-anchoring (callee-swap, audit #2).** Content-anchoring is a
   line-content oracle; it fundamentally **cannot follow a call graph**. So a cite can ALWAYS be authored
   one call shallower than the real enforcer (anchor on a wrapper that just delegates), and C5 cannot
   detect it — the wrapper line is byte-stable while the leaf it calls is rewritten. This is not a bug to
   "fix" mechanically (it is undecidable without whole-program call-graph following, which C3's
   no-rename-follow / declared-dependency stance deliberately rejects); it is an **accepted structural
   residual**. The mitigations are (a) the hard authoring rule above (anchor on the in-repo leaf), (b)
   `source_files` SHOULD carry every file on the verify chain so C5 gates each, and (c) review catches a
   wrapper-anchored invariant. The residual is review-caught, in the same family as the C5b
   cosmetic-edit / rename-reset residuals (§4).
7. **GENERATED concepts** carry a top-of-body marker: `> GENERATED — do not hand-edit. Reconcile via the updater.`

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
| **C6c** Per-claim grounding | Every bullet/line under `# How it works` and `# Invariants` contains ≥1 `path:line` token **AND ≥1 of those cites is a NON-test path** (a bullet whose only cites are test paths is a FAILURE — a test can be neutered later, so an invariant must be grounded on the enforcer; tests allowed only as ADDITIONAL cites). **A test path = `tests/…`, `…/tests/…`, `…/__tests__/…`, the bare `test.rs`/`tests.rs` module files, a SEPARATOR-bounded `*_test.rs`/`*_tests.rs`, a `test_*.rs`/`tests_*.rs` prefix, or `*.test.ts`/`*.spec.ts`. The boundary is required (fix #4): `attest.rs`/`latest.rs`/`contest.rs`/`proptests.rs` are NOT tests — the old bare `…test.rs`-suffix match misclassified them as test-only.** | An ungrounded claim bullet, OR a bullet grounded SOLELY on a test path. |
| **C7** Link integrity | Every bundle-relative link to a **declared** concept resolves. (Links to not-yet-written/`deferred` concepts tolerated per OKF.) | A link to a declared concept path with no file. |
| **C8** Orphan | Every concept reachable from `index.md`. | An unreachable concept. |
| **C9** Reserved names | `index.md`/`log.md` never concept docs; no concept uses a reserved path. | Violation. |
| **C10** Manifest completeness | Every candidate in `concept-manifest.yaml` with **`status: active`** has a concept doc OR a `deferred:` marker. Candidates `status: planned` (not-yet-dispatched backlog) do NOT block — they let the foundation be green before the fill waves run; a concept-authoring WP flips its candidates to `active` and lands the docs in the same PR. | An `active` candidate with neither. |
| **C10b** Manifest ⊇ repo surface | The manifest itself is cross-checked against the mechanically-enumerable surface: `specs/03_architecture/adrs/*.md` (76 ADRs), **`crates/corelink-container/src/routes/**/*.rs` (RECURSIVE — route handlers in `routes/dsr/`, `routes/audit_export/`, `routes/audit_analytics/` subdirs are enumerated, not just top-level)**, **`crates/corelink-container/src/*.rs` (the crate ROOT, file-granular — a handler dropped beside `webhook.rs`/`native_pat_gate.rs` is gated)**, each `crates/*` dir, plus the Worker edge plane + apps/ (`worker/src/**` **RECURSIVE** — any `worker/src/<subdir>/*.ts`, not just top-level + `lib/`; gate v6 fix #3 — and `apps/*/src/**`) → each maps to a manifest entry or an explicit `excludes:` reason. | A repo-surface item with no manifest entry and no exclude (silent gap one level up). |

**C10b — ACCEPTED RESIDUAL: coarse crate-DIR coverage for the non-container library crates (audit #3 MED #3, documented honest, NOT closed).** The C10b enumeration is FILE-granular for the deployed compute plane and the edge plane — `crates/corelink-container/src/**/*.rs` (root + recursive `routes/`), `worker/src/**` (recursive — gate v6 fix #3), and `apps/*/src/**` are walked file-by-file, so a new load-bearing file there REDs the gate. **gate v6 fix #1 (audit #1 HIGH): in these SECURITY-CRITICAL file-granular trees, directory / crate-cluster ADOPTION no longer auto-covers an individual file.** Before v6 the `crates/container-platform` `CrateCluster` seeded the whole `crates/corelink-container` DIRECTORY, which `_concept_grounds` clause (b) promoted to a grounded-directory seed; the FILE branch of `is_covered` then auto-covered EVERY `.rs` under the crate via the `rel.startswith(grounded_dir + '/')` relation — so dropping a new `routes/poison.rs` / `src/poison_top.rs` stayed GREEN and the recursive anti-shadow enumeration was DEAD. v6 suppresses the directory-grounded-seed coverage relation for `crates/corelink-container/src/**`, `worker/src/**`, and `apps/*/src/**` (`_is_file_granular_strict`): a file there is covered ONLY by an exact grounded seed (a per-file `source_files`/`# Citations` cite) or an explicit `excludes:` entry. (Cluster/dir adoption still covers files OUTSIDE these trees — the non-file-granular library crates below.) But every OTHER `crates/*` library crate is enumerated as a single **DIRECTORY** surface, covered by reviewed cluster MEMBERSHIP (a `seed_from` path that is the crate dir or lives under it — `is_covered`'s `is_dir` arm). Consequence: a NEW file added inside an already-covered library crate dir is **invisible to C10b** — the directory is already "covered," so the new file does not surface as a silent gap. This is ~92% of the Rust surface by file count (the non-container crates). It is an **accepted residual, not a closed hole**: (a) the deployed/reachable plane — `corelink-container` — IS file-granular, so the code an attacker actually hits is fully gated; (b) library crates are covered at the cluster-narrative level by design (taxonomy §1.1 assigns `crates/` to "the 8 crate clusters", whose job is breadth-of-narrative, not per-file citation); (c) the nightly **C-REV** reverse-coverage WARN is the backstop that pressures `source_files` completeness WITHIN a member crate when it changes. Making all of `crates/*` file-granular is a large authoring change deferred deliberately; it is logged here as a KNOWN LIMITATION so it is never mistaken for a silent gap that the gate proves absent.

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
`['docs/knowledge/**','crates/**','specs/**','worker/**','apps/**','scripts/validate_okf.py']`
(`worker/**`+`apps/**` are REQUIRED — C5/C10b cite+gate the TypeScript edge plane and the apps/
workers, so a PR touching only `worker/src` or `apps/*/src` must still run the gate; omitting them
was a silent bypass), `runs-on: ubuntu-latest`
(zero Mac load), **`actions/checkout` with `fetch-depth: 0`** (C4/C5 need full history; shallow
checkout errors). Step `python3 scripts/validate_okf.py`. Auto-surfaces in `gh pr checks` →
`pre-merge-gate-check.sh`. C5 firing on a PR that touched a cited line-range is the anti-drift
mechanism: the author reconciles the concept and advances `checkpoint_sha` in the same PR (and C5b
forces a real body edit, not just a SHA bump).

**C5b — accepted residual limitations (documented, not over-engineered).** C5b proves a SHA bump is
accompanied by *some* body change; it does NOT prove the body change is *meaningful*. Two residuals
remain, both requiring a **malicious author** (not an honest drift) and both caught by ordinary review:
- **Cosmetic-edit defeat:** a one-word/whitespace body edit alongside the SHA bump satisfies C5b
  without a real reconcile. Mechanizing "meaningful" is undecidable; the guard is the human reviewer,
  who sees a checkpoint advance paired with a trivial diff and rejects it.
- **Rename-reset:** renaming a concept file makes it look NEW to C5b (no `base_ref` predecessor at the
  new path), so its `checkpoint_sha` can jump straight to HEAD with no body edit and no phantom-reconcile
  finding. The guard is review: a rename **plus** a checkpoint jump in the same PR is the exact pattern a
  reviewer is told to scrutinize. (A code-level fix would need rename-following — deliberately avoided
  here per C3's "renames do NOT auto-follow" stance, which keeps the dependency set author-attested.)
These are accepted residuals: the cost of closing them mechanically exceeds the benefit given a
review-gated, malicious-author-only threat model.

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
- **C6c test-only-grounding relaxation for a test-harness concept (gate v6 fix #2: `testing/` dir AND
  `type: TestStrategy`, BOTH required).** The C6c rule that an invariant must cite ≥1 NON-test path does
  NOT apply to a concept whose SUBJECT is the test harness: there a test file (`tests/…/*.rs`) is the
  legitimate enforcer it grounds on, not a neuterable proxy. The exemption is keyed on BOTH axes agreeing
  — the doc lives under `docs/knowledge/testing/` AND its declared `type` is a testing type
  (`TestStrategy`). Keying on EITHER alone was a bypass: placement-only let an author file a security/auth/
  compliance invariant at `docs/knowledge/testing/sneaky.md` grounded solely on a test and inherit the
  exemption (audit #2 MED); type-only let any concept under `auth/` self-declare `type: TestStrategy` and
  shed the requirement. Requiring both binds the exemption to the concept genuinely being ABOUT the
  harness. The ban stands for every other taxonomy (where a sole test cite is the bypass it closes).

---

## 5. Invariants (INVIOLABLE — a breach is a RED gate, never waived without explicit human authorization)

1. Non-empty `type` on every non-reserved concept (C1).
2. ≥1 resolving `source_files` path on every non-deferred concept (C2/C3).
3. A valid `checkpoint_sha`, and the concept is **not stale** on its declared+cited line ranges (C4/C5); a SHA bump implies a real body reconcile (C5b).
4. Every inline `path:line` cite resolves; every `source_files` path is cited; every cited file is declared (C6/C6b); every `# How it works`/`# Invariants` claim carries a `path:line` (C6c).
5. No orphan concepts; no broken links to declared concepts (C7/C8).
6. Reserved filenames never used as concepts (C9).
7. GENERATED layer marked and never hand-edited (§3.7).
8. No concept candidate silently omitted — `deferred:` or a doc, never nothing (C10); the manifest itself covers the enumerable repo surface (C10b).

---

## 6. What the lead still freezes in Campaign 2 (before any fan-out)

- [ ] `02-concept-template.md` — the per-concept stub agents fill (this contract's companion).
- [ ] `concept-manifest.yaml` — the full enumerated candidate list (35 arch + 80+ ADRs + the
      doc-extraction candidates) with target path + type + source cluster per candidate. The C10 oracle.
- [ ] The validator's red/green **fixture corpus spec** (the acceptance suite W-B builds to).
- [ ] Cold suite-critic sign-off that the above covers the maximal ratified demand.

Only then does Campaign 3 dispatch W-B/W-C/W-D, followed by the rolling concept-authoring waves.
