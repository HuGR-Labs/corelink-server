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
| `checkpoint_sha` | string | 40-hex. The commit this concept was last reconciled against. Still REQUIRED: it is the provenance record and the input C-AGE / C-REV / C5b read (they need a `commit_date`, which a blob has not got). SHOULD be a reachable commit; a SQUASH-ORPHANED checkpoint (a rewritten pre-merge tip, unreachable in a `fetch-depth: 0` clone) is TOLERATED — C4 warns and C5 re-anchors freshness to the base ref (see §4 C4) **for files with no `source_blobs` anchor**. **Extension key.** |

### 2.2 OPTIONAL
| Key | Type | Use |
|-----|------|-----|
| `source_blobs` | list[string] | Per-FILE content anchor: `<repo-relative path>@<40-hex git BLOB id>`, one entry per anchored path, each path also in `source_files`. **This is what C5 compares against when present** — a blob id is a content hash and is immutable under rebase, squash and cherry-pick, unlike the commit id in `checkpoint_sha`, which they all destroy. A blob-addressed file gets **no** squash-orphan tolerance and **no** base-ref fallback (§4 C4b), and once a path is anchored the anchor cannot be dropped while the path is still a declared source (§4 C4c). Additive and per-file, so the corpus migrates incrementally. Shape is a FLAT list of strings on purpose: this frontmatter is read by five hand-rolled parsers plus PyYAML, and a nested mapping is the one shape they disagree on. **Extension key.** |
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
| **C4** Checkpoint valid (squash-merge resilient) | `checkpoint_sha` is 40-hex. Its commit SHOULD be resolvable; a 40-hex whose commit is UNREACHABLE (squash/rebase orphaned its pre-merge tip — the recurring #688/#690 trap) is TOLERATED as a non-blocking warning and C5 re-anchors freshness to the base ref (the fork point), whose cited content equals the dead checkpoint's. | **Malformed** (not 40-hex) SHA. An unreachable-but-well-formed SHA does NOT fail C4 — it is a squash-orphan, not drift; freshness is still gated by C5 against the base ref, so a drifted cite still fails. |
| **C4b** Blob anchor valid (fail-closed) | Every `source_blobs` entry is `path@<40-hex>`, its path is declared in `source_files`, appears at most once, and the id names a **blob object present in this clone**. There is NO tolerance and NO fallback on this path: a commit id can legitimately be unreachable (rebase/squash rewrote it) but a blob id cannot — blobs are content hashes and survive history rewriting, so a blob that was ever pushed is in the `fetch-depth: 0` clone. | Malformed entry, an anchor for an undeclared path, a duplicate anchor, a **commit** id where a blob id belongs (right shape, wrong object type), or an id that resolves to nothing. This is what closes the forged-unreachable-SHA laundering path C4 accepts: an absent blob REDs instead of re-anchoring to a base ref that already contains the landed drift. |
| **C4c** Blob addressing is a ratchet | A path that carried a `source_blobs` anchor in the concept's PREVIOUS version (read at the merge-base, exactly as C5b reads it) must still carry one, while it remains a declared source. | The anchor was deleted while the path is still in `source_files` — i.e. an un-migration back onto the C4 carve-out. Without this, closing the laundering path would only MOVE it. Dropping the source itself is fine (that is a review-visible `source_files` change C3/C6/C10b all react to). |
| **C5** Freshness (anti-drift, declared+line-scoped) | Baseline is chosen PER FILE: the file's `source_blobs` BLOB anchor when it has one (§2.2 — immutable under rebase/squash/cherry-pick, and NOT subject to the C4 orphan tolerance or the base-ref fallback), else the legacy commit anchor below. For each `source_files` path: **two-tree `git diff <checkpoint_sha> HEAD -- <path>`** over the WHOLE file (merge-safe), **then post-filter to the diff hunks that intersect the concept's cited line ranges** for that file. **Implementation MUST NOT use `git log -L`/blame** (line-history reintroduces the merge-simplification B2 removed). No intersecting hunk ⇒ fresh. | A cited line-range's content changed since the checkpoint → concept STALE → FAIL (the bound). Edits OUTSIDE all cited ranges do NOT trigger (anti-churn). **EVERY drifted range is reported** — the check does not stop at the first hit per (concept, file), so the offender list is the complete worklist (a range cited more than once is reported once). |
| **C5b** No phantom reconcile | If a PR advances a concept's `checkpoint_sha`, that concept's body MUST also change in the same PR. | SHA bumped without a body edit (reconcile-without-reconciling). |
| **C6** Citation resolves | Every inline `path:line[-line]` cite: file exists at HEAD, line(s) in bounds; every `source_files` path appears ≥1× under `# Citations`. | Dangling cite, out-of-range line, or an ungrounded declared source. |
| **C6b** Cited ⊆ declared | Every file appearing in any inline `path:line` cite is in `source_files`. | A cited file not declared (would escape C5 freshness). |
| **C6c** Per-claim grounding | Every bullet/line under `# How it works` and `# Invariants` contains ≥1 `path:line` token **AND ≥1 of those cites is a NON-test path** (a bullet whose only cites are test paths is a FAILURE — a test can be neutered later, so an invariant must be grounded on the enforcer; tests allowed only as ADDITIONAL cites). **A test path = `tests/…`, `…/tests/…`, `…/__tests__/…`, the bare `test.rs`/`tests.rs` module files, a SEPARATOR-bounded `*_test.rs`/`*_tests.rs`, a `test_*.rs`/`tests_*.rs` prefix, or `*.test.ts`/`*.spec.ts`. The boundary is required (fix #4): `attest.rs`/`latest.rs`/`contest.rs`/`proptests.rs` are NOT tests — the old bare `…test.rs`-suffix match misclassified them as test-only.** | An ungrounded claim bullet, OR a bullet grounded SOLELY on a test path. |
| **C7** Link integrity | Every bundle-relative link to a **declared** concept resolves. (Links to not-yet-written/`deferred` concepts tolerated per OKF.) | A link to a declared concept path with no file. |
| **C8** Orphan | Every concept reachable from `index.md`. | An unreachable concept. |
| **C9** Reserved names | `index.md`/`log.md` never concept docs; no concept uses a reserved path. | Violation. |
| **C10** Manifest completeness | Every candidate in `concept-manifest.yaml` with **`status: active`** has a concept doc OR a `deferred:` marker. Candidates `status: planned` (not-yet-dispatched backlog) do NOT block — they let the foundation be green before the fill waves run; a concept-authoring WP flips its candidates to `active` and lands the docs in the same PR. | An `active` candidate with neither. |
| **C10b** Manifest ⊇ repo surface | The manifest itself is cross-checked against the mechanically-enumerable surface: `specs/03_architecture/adrs/*.md` (76 ADRs), **`crates/corelink-container/src/routes/**/*.rs` (RECURSIVE — route handlers in `routes/dsr/`, `routes/audit_export/`, `routes/audit_analytics/` subdirs are enumerated, not just top-level)**, **`crates/corelink-container/src/*.rs` (the crate ROOT, file-granular — a handler dropped beside `webhook.rs`/`native_pat_gate.rs` is gated)**, each `crates/*` dir, plus the Worker edge plane + apps/ (`worker/src/**` **RECURSIVE** — any `worker/src/<subdir>/` file, not just top-level + `lib/`; gate v6 fix #3 — and `apps/*/src/**`), enumerated over the FULL **executable-extension set** `{.ts,.mts,.cts,.tsx,.js,.mjs,.cjs,.jsx}` (+`.rs` for Rust) via ONE shared `_is_exec_source` predicate so the surface walk and the `_is_file_granular_strict` classifier AGREE — a `.mts`/`.mjs` edge handler is gated, not just `.ts` (gate v9) — **plus every wrangler `main` declared by ANY `wrangler*.{toml,jsonc,json}` config in the repo — enumerated over a RECURSIVE `rglob` of the surface root (gate v14: excluding build-output/vendor dirs `.wrangler`/`.open-next`/`node_modules`/`target`/`.git`/`dist`/`build`), so config LOCATION no longer hides a main — wherever each config lives (`apps/*`, `crates/*`, the repo ROOT, a crate-NESTED `crates/<x>/cf/`, a sibling top-level `services/edge/`, the `worker/` dir, any dir), not just the three canonical names, taking the UNION of all mains (NO first-file-win): the top-level main AND every per-environment `[env.<name>]` (TOML) / `"env": {…}` (JSONC) override, from EVERY config file; each distinct entrypoint that resolves to a real file OUTSIDE `*/src/**` is enumerated STRICT so an env-override main outside `src/` masked by a top-level decoy is gated, not just the first `main` (gate v11), a main declared only in an alt-named config (`wrangler.prod.toml`) or hidden by dual-config precedence (a `wrangler.toml` decoy over the real `wrangler.jsonc` main) is gated, not just the canonical-name first-file (gate v12), a CRATE-hosted or ROOT wrangler `main` outside `src/` (e.g. `crates/<x>/build/worker/shim.mjs` build output) is enumerated, not silently inherited via whole-crate cluster adoption (gate v13), AND a config whose LOCATION is outside the old fixed root+apps/*+crates/* set (a crate-NESTED `crates/<x>/cf/`, a sibling top-level `services/edge/`, a `worker/wrangler.staging.toml`) can no longer hide its main behind coarse dir-adoption (gate v14)** → each maps to a manifest entry or an explicit `excludes:` reason. | A repo-surface item with no manifest entry and no exclude (silent gap one level up). |

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

**gate v7 — two HIGH strict-tree backdoor holes CLOSED + one MED hardened (PoC-proven).** A brutal
refute proved three ways to slip a backdoor handler past the file-granular strict trees
(`crates/corelink-container/src/**`, `worker/src/**`, `apps/*/src/**`) with the gate staying GREEN.
All are closed at the root (no rigor loosened), each with a self-running fixture:

- **#1 — strict-tree exclude-glob escape (HIGH, closed).** The broad manifest `excludes:` PATTERN globs
  (`**/tests/**`, `**/e2e/**`, `**/__tests__/**`, `**/*.test.ts`, `**/*.config.ts`, `**/*.d.ts`) ran
  AFTER the `_is_file_granular_strict` guard, so the strict guard suppressed directory/cluster ADOPTION
  but NOT the bulk excludes. A REAL request-reachable handler dropped under any `tests/` dir inside a
  strict tree (`routes/tests/poison.rs`) was swallowed by `**/tests/**` with ZERO edits and stayed
  GREEN. v7: inside a strict tree, a BROAD pattern exclude may exclude a file ONLY when that file is
  GENUINELY a test/config by its OWN BASENAME (`_basename_is_test_or_config` — `test.rs`/`*_tests.rs`/
  `test_*.rs`/`*.test.ts`/`*.spec.ts`/`*.d.ts`/`*.config.ts`/`wrangler.toml`/`package.json`/
  `tsconfig.json`). A real handler name (`poison.rs`) under a `tests/` dir now requires an EXPLICIT
  per-file `excludes:` entry (an exact, review-visible surface) or grounded coverage — else it REDs.
  Genuine inline tests under `tests/` stay excluded.
- **#2 — a strict-tree COVERING cite must be a CODE line (HIGH, closed).** A strict file counted as
  covered when a concept listed it in `source_files` + cited it under `# Citations` — but the cite
  could point at a NON-grounding line (a `//`/`//!`/`///` doc-comment, e.g. the `:1-2` file header)
  while the executed handler body stayed uncited. A backdoor `routes/poisonx/poison.rs` cited at `:1-2`
  stayed GREEN. v7 adds `_cite_is_code_line`: for a strict-tree file that IS cited, at least one of its
  cite ranges must contain a real CODE line (non-blank, not a `//`/`//!`/`///`/`#`/`*`/`/*` comment).
  A strict file whose only citations are comment/blank lines is NOT covered (REDs). (CrateCluster
  ADOPTION — a `seed_from` entry under a `type: CrateCluster` concept with NO per-file cite — is a
  different, already review-gated path and is unaffected; the code-line rule binds only the per-file
  CITE relation, the exact hole the PoC used.)
- **#3 — unvalidated strict-tree exclude reason (MED, hardened).** A one-line `excludes:` entry with a
  fabricated reason can hide a backdoor. v7 (a) HARD-FAILS [C10b] any strict-tree exclude whose reason
  is trivial/placeholder (too short, or a known placeholder like `TODO`/`TBD`/`N/A`), and (b) surfaces
  a review WARN for every strict-tree-hitting exclude entry, so a human reviewer sees exactly which
  strict surfaces were waived (the sanctioned alternative to a per-file cite stays review-visible).

**gate v8 — two MATERIAL backdoor-coverage bypasses CLOSED (PoC-proven; a real backdoor shipped GREEN).**
A brutal refute proved two more ways a request-reachable backdoor passed the file-granular strict trees:

- **#1 — app enumeration was a HARDCODED 3-app allowlist (MEDIUM, closed).** `_is_file_granular_strict`
  classifies EVERY `apps/*/src/**` as strict, but the C10b surface WALK enumerated only a hardcoded
  `("signup-worker","cas-worker","analytics-worker")` — MISALIGNED. A brand-new app's handler was
  classified strict yet NEVER enumerated, so it could never surface as a [C10b] gap: `apps/runner-worker/
  src/poison.ts` shipped GREEN with zero coverage. v8 enumerates ALL `apps/*/src/**/*.{ts,rs}` DYNAMICALLY
  (glob the `apps/` dir for any app with a `src/`), so strict-classification and the surface walk AGREE.
  A genuinely-non-app/presentation dir under `apps/` (e.g. `apps/admin-ui`, `apps/docs`, `apps/get-corelink-
  worker`) stays out of the surface via its existing exact-prefix `excludes:` entry — the same honest,
  review-visible waiver every wholesale-exempt app carries. (Verified: `apps/runner-worker/src/poison.ts`
  now REDs; a wholesale-excluded UI app's file stays covered.)
- **#2 — a strict-tree covering cite must be a SUBSTANTIVE line, not boilerplate (HIGH, RAISES the bar).**
  v7 #2 required the covering cite to be a CODE line (non-comment), but `_line_is_code` accepted ANY
  non-comment line — including pure BOILERPLATE that SUBSTANTIATES NOTHING: a `use crate::auth;` /
  `mod x;` / `pub use …` / `extern crate …` / an `import …`/`from …` line, a line that is ONLY a brace/
  paren/bracket (`{`,`}`,`);`,`});`,`,`), or a bare TYPE/BLOCK declaration-header with no body on the
  line (`impl X {`, `struct X {`, `enum X {`, `trait X {`, `match … {`). A backdoor handler cited at its
  `use` line therefore shipped GREEN. v8 adds `_line_is_substantive` (used by `_cite_is_code_line`): a
  strict-tree covering cite must land on a line carrying real statement/expression content. A NAMED
  function-signature opener (`fn NAME(…)` / `async fn NAME(…)`, body may open on the next line) IS
  substantive — it names the actual callable enforcer (its identity + interface), the legitimate anchor
  a per-file cite uses — so this is a RAISED bar, not a behavioural break (real existing concepts that
  cite a handler at its signature line stay covered). (Verified: the `use`-line PoC and a bare-brace-line
  PoC now RED; the same file cited at a genuine enforcer statement stays covered.)
  **⚠️ This only RAISES the bar.** A backdoor cited at a REAL-but-misdescribed enforcer line — e.g. the
  handler's own `pub async fn poison(…)` signature, a real code line that names a real function — is STILL
  the C5 **freshness ≠ authoring-correctness** structural residual below (the gate attests the cited line
  is a substantive code line, never that it is the RIGHT line that enforces the prose claim). That residual
  is owned by the periodic human/panel deep-audit layer, NOT by this fix; v8 #2 only stops the trivial
  boilerplate-line bypass (the cite that names nothing at all).

**gate v9 — one MATERIAL backdoor-coverage bypass CLOSED (PoC-proven; a real backdoor shipped GREEN).**
A brutal refute proved that the file-granular strict CLASSIFIER and the C10b surface WALK disagreed on
the EXTENSION set:

- **#1 — strict-classifier vs surface-walk EXTENSION misalignment (HIGH, closed).** `_is_file_granular_strict`
  classifies EVERY file under `worker/src/**` + `apps/*/src/**` as strict (file-granular-required) REGARDLESS
  of extension, but the C10b surface WALK enumerated only `*.ts` (worker) and `*.ts`/`*.rs` (apps). So a real
  request-reachable Cloudflare-Worker backdoor handler dropped in those trees with ANY OTHER executable
  extension — `.mts`/`.mjs`/`.cts`/`.cjs`/`.tsx`/`.jsx`/`.js`, ALL of which wrangler accepts as `main` — was
  classified strict yet NEVER enumerated, so it could never surface as a [C10b] gap and shipped GREEN with
  zero coverage (PoCs: `worker/src/poison.mts` + `apps/runner-worker/src/poison.mts`). v9 factors ONE shared
  executable-source predicate (`_is_exec_source` over `_EXEC_JS_TS_EXTS = {.ts,.mts,.cts,.tsx,.js,.mjs,.cjs,
  .jsx}` + `.rs`), and both the surface walk (`_iter_exec_sources`) and the strict classification now use the
  IDENTICAL extension set — so they AGREE. A genuinely-non-executable file the recursive walk reaches (`.json`/
  `.md`/`.css` under `src/`) is NOT executable source and so is not enumerated; nothing data/config-shaped is
  newly surfaced. No NEW real handler surfaced after the tightening: the existing `.tsx` files all live under
  the wholesale-excluded presentation apps (`apps/docs`, `apps/admin-ui`), and `worker/src` carries no non-`.ts`
  executable file — so the gate stays GREEN at 152. (Verified: `worker/src/poison.mts` + `worker/src/widget.tsx`
  + the new app's `apps/runner-worker/src/poison.mjs` now RED [C10b]; a genuine concept-cited `.ts` entrypoint
  stays covered.)

**gate v11 — one MATERIAL coverage hole CLOSED (PoC-proven; a real prod-deploy entrypoint shipped GREEN).**
gate v10 #2 parses each app's wrangler `main` and enumerates it wherever it lives — but it parsed only the
**FIRST** `main` declaration (`_WRANGLER_MAIN_RE.search()`):

- **env-OVERRIDE main masked by a top-level decoy (HIGH, closed).** A wrangler config can declare a top-level
  `main` AND a **per-environment override** — TOML `[env.prod]\nmain = "build/worker-prod.mjs"`, or JSONC
  `"env": {"prod": {"main": …}}`. `wrangler deploy --env prod` deploys the **ENV** main, not the top-level one.
  Because the old parser took only the FIRST match, a decoy top-level `main = "src/index.ts"` (concept-cited)
  over an `[env.prod]` override pointing at `build/worker-prod.mjs` (uncited, outside `src/`) made the REAL
  prod entrypoint ship **GREEN with zero coverage** (PoC: `apps/poc-envmain` — its `[env.prod]` main shipped a
  green backdoor). v11 replaces `search()` with `findall()` and enumerates **EVERY** `main` declaration in the
  file — the top-level main AND every `[env.<name>]` (TOML) / `"env": {"<name>": {…}}` (JSONC) override — as a
  distinct required surface, each treated STRICT. So an env-override main outside `src/` now surfaces as a
  [C10b] gap unless a concept grounds it or an explicit `excludes:` entry waives it; a top-level decoy can no
  longer mask it. The matcher also tolerates single quotes and a TOML/JSONC ARRAY `main` form (`main = ["a",
  "b"]`) if the wrangler schema ever allows it, enumerating every element. No real app regressed: every shipped
  wrangler config declares only a top-level `src/index.ts` main with NO `[env.*]` main override (the `[env.prod]`
  blocks in `analytics-worker`/`get-corelink-worker` carry routes/D1/observability, not a `main`), and the
  wholesale-excluded `apps/admin-ui` / `apps/get-corelink-worker` exact-prefix excludes still cover their mains
  — so the gate stays GREEN at 152. (Verified: the `apps/poc-envmain` `[env.prod]` main now REDs [C10b]; a
  concept-cited top-level `src/index.ts` main stays covered; single-quote + array forms parse.)

**gate v12 — two LOW wrangler config-RESOLUTION holes CLOSED (same root cause: the gate's config selection
diverged from wrangler's actual selection; neither in-tree today, but the gate's stated contract is "every
wrangler main").** `_wrangler_mains` resolved configs by checking only the three CANONICAL names
(`wrangler.toml`/`.jsonc`/`.json`) and FIRST-FILE-WINS (it `return`ed on the first that existed):

- **ALT-NAMED / per-env config (LOW, closed).** `wrangler deploy --config wrangler.prod.toml` (and the
  convention `wrangler.<env>.toml`) deploys a `main` declared in a NON-canonical-named config. Because the
  resolver looked only at the three canonical names, a main declared SOLELY in `wrangler.prod.toml` was never
  read → it could ship GREEN with zero coverage. v12 GLOBS every `wrangler*.{toml,jsonc,json}` file in the app
  dir and parses mains from EACH.
- **DUAL-CONFIG precedence (LOW, closed).** When an app has BOTH `wrangler.toml` AND `wrangler.jsonc`, the old
  resolver returned the FIRST (`.toml`) and short-circuited — but modern wrangler PREFERS `.jsonc`/`.json` over
  `.toml`. So a decoy `wrangler.toml` (cited `src/index.ts`) over a `wrangler.jsonc` (real, uncited
  `build/*.mjs`) hid the real entrypoint. v12 does NOT first-file-win: it UNIONS the mains across ALL
  `wrangler*.{toml,jsonc,json}` configs (every declared main from every file), so neither a precedence decoy nor
  an alt-named config can hide the real entrypoint. Over-enumerating a decoy main is conservative/safe — it just
  requires that main to be concept-covered-or-excluded; NEVER hiding a real one is the goal. No real app
  regressed: every shipped app declares exactly one canonical `wrangler.toml` with a top-level `src/index.ts`
  main, so the union is identical to the old first-file result for them — the gate stays GREEN at 152.
  (Verified: an `apps/altcfg-worker/wrangler.prod.toml` main `dist/prod.mjs` outside `src/` REDs [C10b]; an
  `apps/dualcfg-worker` with a `wrangler.toml` decoy + a `wrangler.jsonc` real `build/real.mjs` main has the
  `.jsonc` main RED [C10b] despite the decoy; the concept-cited top-level `src/index.ts` mains stay covered.)

**gate v13 — one MATERIAL coverage hole CLOSED (PoC-proven): the wrangler-`main` enumeration was scoped to
`apps/` ONLY.** v10/v11/v12 each claimed to close the wrangler-main class ("every wrangler main, wherever it
lives, strict"), but the enumeration (both the strict-classifier set AND the surface walk) globbed only
`apps/*/wrangler*`. A wrangler config that lives OUTSIDE `apps/` — in a `crates/<x>` dir, or the repo ROOT —
declaring a `main` pointing OUTSIDE `*/src/**` was therefore NEVER enumerated as a strict surface:

- **CRATE-hosted / ROOT wrangler main outside `src/` (HIGH, closed).** A crate is dir-covered by its
  `CrateCluster` `seed_from`, so a crate-hosted wrangler `main` pointing at build output (e.g.
  `crates/corelink-clerk-cf/wrangler.toml` → `main = "build/worker/shim.mjs"`) rode coarse crate-dir/cluster
  ADOPTION and shipped GREEN with a **zero-coverage edge entrypoint**. PoC-proven: creating
  `crates/corelink-clerk-cf/build/worker/shim.mjs` kept the gate GREEN under the apps-only fix. v13 runs the
  SAME `_wrangler_mains` enumeration over the **UNION of ALL `wrangler*.{toml,jsonc,json}` configs under
  `apps/`, `crates/*`, AND the repo ROOT** (`_wrangler_config_dirs`) — every declared main (top-level +
  `[env.*]` + array forms) that resolves to a real file outside `*/src/**` is enumerated as a STRICT required
  surface, exactly as `apps/` mains are. A main INSIDE the owning crate's `src/**` is already covered by the
  file-granular-strict tree and is untouched; the hole was specifically a main outside `src/`. The default is
  ENUMERATION, not silent inheritance: a crate/root build-output main now needs a concept cite or an explicit
  `excludes:` reason. No real surface regressed — the root `wrangler.toml` main is `worker/src/index.ts`
  (inside the strict tree → already covered); the only crate-hosted wrangler is `corelink-clerk-cf` whose
  build-output main is a DORMANT POC (the shim file is not built/present), so its main does not resolve and the
  gate stays GREEN. (Verified: with the `corelink-clerk-cf/build/worker/shim.mjs` shim present the gate REDs
  [C10b] naming it; absent, it is GREEN. A hermetic fixture proves a crate-hosted `build/worker/shim.mjs` main
  under whole-crate adoption REDs [C10b] while a crate whose main IS its concept-cited `src/index.ts` stays
  covered.)

**gate v14 — three MATERIAL coverage holes CLOSED (PoC-proven; SAME root cause: config LOCATION could still
hide a deploy entrypoint).** v13's `_wrangler_config_dirs` enumerated a FIXED 3-class LOCATION set — the repo
ROOT + ONE level into `apps/*` + ONE level into `crates/*` — yet its docstring claimed "EVERY directory that
hosts a wrangler config." So a wrangler config whose LOCATION fell OUTSIDE that set shipped GREEN with only
coarse crate-dir/cluster ADOPTION, and a real edge entrypoint (a `main` resolving outside `*/src/**`) declared
there rode that adoption uncovered. Three PoC-proven location bypasses (all reverted):

- **crate-NESTED config (MATERIAL, closed).** `crates/corelink-clerk-cf/cf/wrangler.toml` — the one-level-only
  walk into `crates/*` never descended into `crates/<x>/cf/`, so a `main` declared there was invisible.
- **sibling top-level dir (MATERIAL, closed).** `services/edge/wrangler.toml` — only `root` + `apps/` +
  `crates/` were ever considered, so a config in a brand-new top-level dir was never enumerated.
- **`worker/` alt config (MATERIAL, closed).** `worker/wrangler.staging.toml` — the `worker/` dir itself was
  not in the set, so an alt-named staging config there was never read.

v14 replaces the hardcoded 3-class walk with a RECURSIVE `rglob("wrangler*.{toml,jsonc,json}")` over the
**surface root** (excluding build-output / vendor dirs: `.wrangler`, `.open-next`, `node_modules`, `target`,
`.git`, `dist`, `build` — we never scan generated bundles, the same exclusion the secrets gates apply), so
config LOCATION can no longer hide a deploy entrypoint — matching what v10–v13 already do for config NAME
(`wrangler*` glob) and env-override SYNTAX (`findall` over `[env.*]`). Every config found, in ANY dir,
contributes its mains (top-level + `[env.*]` + array, via the existing `_wrangler_mains`) as STRICT required
surfaces (concept cite or explicit exclude), exactly as before. The `corelink-clerk-cf` dormant POC stays
GREEN: its `main = "build/worker/shim.mjs"` is a build artifact that does not exist on disk, so the
`_mp.is_file()` guard drops the non-existent main (the exclude, if it ever materialises, must be PRECISE).
No real apps/main config regressed — they live under `apps/*` and `crates/*` exactly as before; the rglob is a
SUPERSET of the v13 set. (Verified: a crate-NESTED `crates/x/cf/wrangler.toml`, a sibling-top-level
`services/edge/wrangler.toml`, and a `worker/wrangler.staging.toml` — each with a main outside `src/` — now RED
[C10b]; a config UNDER a pruned `build/` dir is NOT walked; a crate-nested config whose main IS its
concept-cited `src/index.ts` stays covered. A hermetic fixture proves the crate-NESTED + sibling-top-level
positives, the build-output prune, and the selectivity negative.)

**ACCEPTED STRUCTURAL RESIDUAL — C5 freshness ≠ authoring-correctness (gate v7 #4: documented, NOT
"fixed").** C5 proves FRESHNESS — that the CONTENT of each cited `path:Lx-Ly` has not drifted since the
concept's `checkpoint_sha` (an in-range edit or a pure position-shift makes it STALE). It does NOT — and
structurally CANNOT — prove authoring-CORRECTNESS: that the cited line actually *substantiates the prose
claim* it anchors. A cite repointed to a WRONG but in-bounds, byte-unchanged line (e.g. an author moves
an invariant's anchor onto a neighbouring line that happens to be stable) passes C5 TRIVIALLY — the gate
attests the cited content is unchanged, never that it is the RIGHT content. gate v7 #2 + v8 #2 narrow this
for the strict trees (the cite must be a SUBSTANTIVE code line — not a doc-comment, and not pure
import/module-wiring/brace boilerplate), but "this code line is the one that enforces this claim" remains
a semantic judgement no mechanical freshness check can make. This
is an **accepted structural residual**, in the same family as the C5b cosmetic-edit / rename-reset
residuals: the gate owns FRESHNESS; **authoring-CORRECTNESS is owned by the periodic human/panel
deep-audit layer** (the N-lens `okf-truth-panel` + the standing human re-attestation), which is a
**PERMANENT, non-optional part of the OKF system, not a stopgap.** The gate makes drift mechanical and
cheap to catch; it deliberately does not pretend to mechanize correctness, and the deep-audit layer is
the load-bearing guarantor of it.

**ACCEPTED STRUCTURAL RESIDUAL — C10b COVERAGE-BY-CONVENTION ≠ BUILD-REACHABLE SURFACE (gate v10,
round 5: documented, NOT "fixed").** The C10b enumeration walks the conventional source trees — the
wrangler-`main` entrypoints, every app's `*/src/**`, the container `src/**` (root + recursive
`routes/`), and `worker/src/**` — so a load-bearing file placed in those conventional locations is gated.
gate v10 closes two FILESYSTEM-CONVENTION holes within that model: the Rust enumeration is now
case-INSENSITIVE (it shares the lowercasing `_is_exec_source` predicate, so a `#[path]`-pulled
`src/Poison.RS` is enumerated, not skipped by a case-sensitive `*.rs` glob — fix #1), and each app's
wrangler `main` is parsed and enumerated WHEREVER it lives (an entrypoint at app-root or any non-`src/`
dir is now a required surface, not just `apps/*/src/**` — fix #2). But the gate still enumerates by
CONVENTION (those source trees), **NOT the full build/import graph** — it is not an esbuild bundler nor a
cargo module-tree analyzer. A load-bearing handler placed OUTSIDE the conventional trees and pulled in
ONLY via an import or a `#[path]` is NOT enumerated: e.g. `worker/handlers/poison.ts` imported by
`worker/src/index.ts` ships in the esbuild bundle the gate does not follow; a `crates/.../src/x.rs`
declared with `#[path = "../../../elsewhere/poison.rs"]` outside the walked tree is in the cargo build
but off the convention. Fully closing this needs a RECURSIVE import-graph traversal from each wrangler
`main` (and the cargo `#[path]`/`mod` tree) — out of scope for a content-anchor completeness gate, and a
large analyzer dependency. This is an **accepted structural residual**, in the same family as the C5
freshness ≠ authoring-correctness residual above: such a non-conventional deploy layout is
**DIFF-VISIBLE** (a handler file outside the conventional trees, an import/`#[path]` that reaches across
to it) and is owned by **code-review + the human/panel deep-audit layer** — the SAME permanent
correctness backstop that owns the C5 residual. The gate RAISES the bar (every conventional layout is
gated file-granular; a non-conventional deploy layout is itself a review red-flag), it deliberately does
**not** pretend to be a build-graph analyzer.

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
