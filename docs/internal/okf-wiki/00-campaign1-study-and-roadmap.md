---
title: "OKF Architecture Wiki — Campaign 1: Study, Mapping & Roadmap"
type: "Roadmap"
description: "Lead's study/mapping synthesis and the phased roadmap for a self-maintaining, code-grounded, OKF-format architecture wiki for CoreLink."
status: "RATIFIED — maximal bundle, build now (stakeholder, 2026-06-26). Campaign 2 in progress."
owner: "tech-lead (orchestrator)"
baseline_sha: "2f82a2a3"
tags: ["okf", "knowledge-wiki", "roadmap", "documentation", "maintainability"]
---

# OKF Architecture Wiki — Campaign 1: Study, Mapping & Roadmap

> **Process contract (set by stakeholder):** The lead plans, studies, and documents every step
> (what / how / why) BEFORE any code. This document is the roadmap; the roadmap becomes the WPs;
> the WPs are executed by the agent team against **SOTA frozen contracts**. Agents do not decide —
> the lead decides; agents transcribe. Every WP carries a DoD, invariants, completeness criteria,
> and severe SOTA quality standards. The lead's context window is protected: discovery/build is
> delegated; judgment is not.

---

## 1. The goal (stakeholder intent — the lead is its guardian)

A **self-maintaining, in-repo, OKF-format architecture knowledge wiki** for CoreLink that:

1. **Cuts maintenance / onboarding / cross-TL handoff friction** across ~71 crates and 3 planes.
2. Is **grounded in code** — every factual claim cites a resolvable code anchor (`path:line`), so it
   is *assertive*, not *plausible*.
3. **Cannot silently drift on declared dependencies** — a mechanical freshness gate marks any concept
   stale the moment a cited source line-range changes; CI refuses the merge until reconciled. We gate
   the **bound (staleness)**, never mere existence. (Honest limit: the gate cannot prove `source_files`
   is *complete* — see the contract §4 residual-risk note; mitigated by C6b/C-REV/C-AGE, not eliminated.)
4. Is **vendor-neutral and self-contained** — adopts the open OKF v0.1 spec; nothing leaves the repo
   (kills the third-party-SaaS exposure that DeepWiki-style tools imply for a crown-jewel private repo).
5. **Dogfoods the pattern** the owner's auto-memory already proves works (directory of markdown +
   YAML frontmatter + `[[links]]` + an index).

**Non-goals (explicit, to prevent scope creep / treadmill):** this is the *narrative / architecture*
layer **above** rustdoc. We do **not** re-document every function or type — `cargo doc` owns the API
surface. We document the cross-crate "why", the plane interactions, the request flows, and the gotchas
— the knowledge rustdoc structurally cannot hold.

---

## 2. Campaign 1 findings (delegated discovery — 5 read-only agents, compact returns)

### 2.1 OKF v0.1 is real, precise, and permissive enough
- Source of truth: `GoogleCloudPlatform/knowledge-catalog/okf/SPEC.md` (Apache-2.0). Authors: Sam
  McVeety & Amir Hormati. v0.1 = Draft, versioned `<major>.<minor>` (minor = backward-compatible
  additions). Blog: 2026-06-12.
- **Bundle** = self-contained directory tree of markdown; structure independent of domain;
  distributable as git repo. Reserved filenames `index.md` (listing) and `log.md` (history) — MUST
  NOT be concept docs.
- **Concept identity** = one concept per `.md`; concept ID = bundle-relative path minus `.md`
  (`auth/pat-gauntlet.md` → `auth/pat-gauntlet`). Path is the cross-link identifier.
- **Frontmatter schema:** `type` **REQUIRED, non-empty** (free-form, not centrally registered).
  Optional: `title`, `description`, `resource` (URI), `tags` (list), `timestamp` (ISO-8601),
  `okf_version` (root `index.md` only). **Producers MAY add any extra keys; consumers MUST preserve
  unknown keys and MUST NOT reject docs with unrecognized fields.** ← this legitimizes our extension.
- **Body/links/citations:** standard + structural markdown; cross-links as bundle-relative markdown
  links with a leading `/` (RECOMMENDED); citations under a `# Citations` heading, numbered. A broken
  link = "not yet written", and consumers MUST tolerate it.
- **Conformance (the entire hard requirement):** every non-reserved `.md` has parseable frontmatter
  with a non-empty `type`. Everything else is soft guidance. **No official validator exists** → we
  author our own (and it becomes our gate).
- Reference impls (non-normative POC): an enrichment agent + a single-file static HTML graph
  visualizer (no backend, no data leaves the page).

### 2.2 The self-maintaining loop — SHA-checkpoint incremental, never whole-rebuild
- Dominant proven pattern (CodeWiki, git-wiki, llmwiki): store the last-reconciled commit SHA; on a
  new commit, diff SHA→HEAD; cross-reference changed files against each concept's `source_files`
  frontmatter; mark **only dependent concepts** stale; regenerate just those; advance the checkpoint.
- **Deterministic shell/python owns intake, scaffolding, routing, linting, freshness; the LLM is
  reserved for synthesis.** Leanest proven architecture ≈ a ~400-line Rust CLI + a Claude skill.
- **Grounding:** code wikis cite `path:line` + carry `source_files:` frontmatter (reverse-traceable;
  a freshness check verifies them). Implementations that skip line-level cites are the weak ones.
- **Lint:** orphans (no inbound links), broken `[[links]]`, missing pages, stale claims,
  contradictions, frontmatter/type-path consistency. Mechanical subset → pre-push/CI (zero LLM cost).
- **Documented failure mode:** past ~day-60, an un-gated wiki becomes "confident-but-stale memory that
  makes the agent worse." This is the exact risk our freshness gate exists to kill.
- **What to copy:** SHA-checkpoint incremental diff; `source_files` reverse-map; deterministic-CLI vs
  LLM-synthesis split; mechanical pre-merge linter; `path:line` cites verifiable by the freshness
  check. **What to avoid:** whole-rebuild scans; untyped 1-bit wikilinks; trusting LLM-discovered
  links without approval; heavy DB/MCP/extension stacks; skipping line-level grounding.

### 2.3 Existing knowledge assets — the repo is ~70% of the way to an OKF bundle
- **Auto-memory (73 `.md`)** — already a near-perfect OKF bundle (one fact/file, YAML frontmatter,
  `[[links]]`, `MEMORY.md` index). Only diff: flat vs nested. *Lives outside the repo* (`~/.claude`).
- **ADRs (76)** in `/specs/03_architecture/adrs/*.md` — rich frontmatter (id, type=adr, status_history,
  deciders, context_links). Very high seed value; live in a separate tree.
- **README.md + ARCHITECTURE.md** — canonical positioning + 5 invariant commitments (BLAKE3 integrity,
  TLA+ tenant-isolation, BYOK ×4 KMS, RFC-6962 audit chain, residency-honest storage). High seed value.
- **docs/ (262 files / 26 folders)** — large but mostly narrative, sparse frontmatter; selective
  concept-extraction needed. Seed-worthy clusters: security/, compliance/, testing/e2e,
  launch/readiness, internal/onboarding.
- **CHANGELOG.md** — `[Unreleased]`-gated; low architectural seed value.
- **Verdict:** auto-memory usable as-is; ADRs re-indexable; README/ARCHITECTURE decompose into ~8–12
  seed concepts; docs/ feed selective extraction.

### 2.4 Architecture concept candidates — 35 mapped, with code anchors
Clustered (anchors abbreviated & ILLUSTRATIVE; the frozen, repo-verified anchors live in
`concept-manifest.yaml`). **NB:** all container/route anchors are crate-prefixed —
`crates/corelink-container/src/...` and `crates/corelink-container/src/routes/*.rs` (the bare
`routes/*.rs` / `corelink-container/src/...` forms below do NOT resolve; verified against HEAD).
- **Planes (4):** Worker (`worker/src/index.ts:Env`), DO lifecycle
  (`worker/src/durable_object.ts:CoreLinkServer`), Container
  (`corelink-container/src/main.rs`, `routes.rs`), Request flow Worker→DO→Container.
- **Cache surfaces (6):** native CAS (`routes/cas.rs`), AC (`routes/ac.rs`), Bazel REAPI v2
  (`routes/bazel_v2.rs`), Turborepo v8 (`routes/turbo_v8.rs`), sccache/cargo (`routes/cargo.rs`),
  package-manager `_public` (`routes/{npm,pip,brew,oci}.rs`).
- **Auth/identity (5):** 2-level PAT moat, HMAC fast-reject, D1 PAT store, Argon2id verify, introspect
  endpoint (`corelink-container/src/adapter_pat.rs:PatVerifier`, `routes/auth_introspect.rs`).
- **Storage/data (6):** R2 CAS bucket, R2 AC ×5 regional, chunk/manifest buckets, D1 CONFIG_DB, CAS
  hot-path latency (the D1-over-HTTP cost).
- **Multi-tenancy/governance (5):** tenant isolation (`idFromName(tenant_id)`), monthly $-ceiling
  (`tenant_quota.rs`), request-quota enforcement, storage-quota header.
- **Crate clusters (8):** CAS/AC core, Auth/PAT, Adapter hosts, Billing/commerce, Privacy/compliance,
  Audit/analytics, Operations, Container/platform.
- **Key flows (4):** CAS write, PAT verification gauntlet, billing quota check, introspection (runners
  fabric).

### 2.5 CI/gate machinery — a freshness gate slots in cleanly
- Gate authoring pattern: a `scripts/validate_*.{py,sh}` that **exits non-zero + prints a human
  verdict** (`"463/0"`, `"OK (no drift)"`), called by a workflow step, surfaced in `gh pr checks`,
  consumed by `pre-merge-gate-check.sh`.
- Fast PR gates run on `ubuntu-latest` (GitHub-hosted). Heavy gates moved OFF per-PR (2026-06-02) to
  nightly+main+on-demand on the self-hosted Mac. **Our gate is light → `ubuntu-latest`, zero Mac load.**
- Best host: a new `.github/workflows/okf_wiki.yml` (`on: pull_request`, paths
  `docs/knowledge/**`, `crates/**`, `scripts/validate_okf.py`) → auto-wires into the pre-merge gate.

---

## 3. Architectural decisions (the lead decides; rationale logged)

| # | Decision | Rationale |
|---|----------|-----------|
| **D1** | Adopt **OKF v0.1 verbatim** as the format. | Open/vendor-neutral (the whole point), structure-for-free, interop, and the auto-memory already proves the pattern for this owner. |
| **D2** | Bundle root = a **new `docs/knowledge/` tree**; existing ADRs/README/memory are SOURCES that seed it, not the bundle. | Clean bundle (reserved `index.md`/`log.md` honored); does not pollute the 262 narrative docs; clear audience separation. |
| **D3** | **Grounding is mandatory:** every concept carries `source_files:` (≥1 repo path) + inline `path:line` cites under `# Citations`. | The single highest-leverage choice — converts plausible prose into a code-anchored, checkable claim, AND enables the mechanical freshness gate. OKF permits the extra key. |
| **D4** | **Anti-drift = SHA-checkpoint freshness gate**, not time-based. Each concept records a `checkpoint_sha`. | Matches the proven pattern + the existing `validate_*` gate culture; runs on `ubuntu-latest`; gates the BOUND (staleness), never flatness/existence. |
| **D5** | **Deterministic CLI/python owns intake+scaffold+lint+freshness; the LLM agent does only synthesis.** | Leanest proven architecture (CodeWiki); avoids the heavy DB/MCP stack the research flags as the anti-pattern. |
| **D6** | **Incremental update, never whole-rebuild.** Commit-diff → stale-flag dependents → regen only those. | Cost + the dominant proven loop. |
| **D7** | **Scope = the narrative/architecture layer above rustdoc** (the 35 candidates), seeded from README/ARCHITECTURE/ADRs/memory. | Fills the real gap; avoids the function-by-function treadmill that has no moat and infinite maintenance. |
| **D8** | **Keep the in-repo wiki SEPARATE from the owner's auto-memory.** The wiki may *cite* memory facts as seed, but the two are distinct bundles. | Memory is owner/session-scoped and partly sensitive (secret hints, live state); the wiki is team/durable architecture. Two audiences → two bundles. |

**The OKF-CoreLink profile (to be frozen in Campaign 2):** OKF v0.1 + two required extension keys —
`source_files: [<repo-relative path>, ...]` and `checkpoint_sha: <40-hex>` — plus a concept-path
taxonomy aligned to §2.4 (`planes/`, `surfaces/`, `auth/`, `storage/`, `tenancy/`, `flows/`,
`crates/`). This profile is the frozen contract the concept-authoring agents transcribe against.

---

## 4. Stakeholder ratification (logged 2026-06-26)

The two escalated calls were decided by the stakeholder, **overriding the lead's conservative
recommendations** — the maximal, build-now path is ratified:

- **SC1 — Scope ceiling → MAXIMAL BUNDLE.** Architecture-core (35 candidates) **+** concept extraction
  from the seed-worthy docs/ clusters (security/, compliance/, testing/e2e, launch/, internal/onboarding,
  dev/, build/, release/, operator/, perf/, cli/, sdk/, handoff/) **+** re-index of the 80+ ADRs into the
  bundle. Target corpus ≈ **architecture (35) + ADRs (80+) + extracted doc-concepts (~60–100)**.
- **SC2 — Priority vs launch → BUILD COMPLETE NOW.** Foundation **and** full fill built now, not deferred.

**Lead's execution consequence (the judgment that stays with the lead):** maximal scope does NOT
loosen rigor — it raises the stakes on the contract freeze and on mechanical verification. The plan
therefore **hardens** the Campaign-2 contract and runs the fill as **disjoint, file-partitioned,
rolling concept-authoring waves** (0-conflict), each WP mechanically verified by the validator on
landing and cold-checked by the lead against its cited anchors. We scale agent count to the corpus,
never the WP size (AP-4). No fan-out begins until the profile + template + validator contract are
frozen and a cold critic confirms the acceptance suite covers the maximal demand.

(SC3 — unify with auto-memory? — decided by the lead as D8: keep separate; cite as seed only.)

---

## 5. The roadmap (campaigns → WPs)

- **Campaign 1 — Study & Mapping (THIS doc).** ✅ Done. Delegated discovery; findings + decisions logged.
- **Campaign 2 — Decompose + Freeze Contracts.** Externalize the demand as a failing acceptance suite
  (a fixture bundle the validator must pass/fail correctly); cold-critic confirms coverage; THEN slice
  the foundation into disjoint WPs and **freeze the OKF-CoreLink profile + the concept template/stub +
  the validator contract**. Agents transcribe a frozen interface.
- **Campaign 3 — Foundation build wave** (rolling-scheduled, SEAL per WP, change-scoped integration).
- **Campaign 4 — Rolling fill + the incremental updater** (post-launch).

### 5.1 Provisional WP table for the Foundation (Campaign 3) — frozen in Campaign 2

| WP | Owner-files (disjoint) | Depends on | Deliverable |
|----|------------------------|-----------|-------------|
| **W-A** *(contract)* | `docs/knowledge/PROFILE.md`, `concept.template.md` | — | The frozen OKF-CoreLink profile + concept stub. **Campaign 2 artifact.** |
| **W-B** | `scripts/validate_okf.py`, `tests/okf/fixtures/**` | W-A | Validator: OKF-conformance + freshness (`source_files` exist, `path:line` cites resolve, `checkpoint_sha` vs git-log of source_files) + a red/green fixture corpus. |
| **W-C** | `.github/workflows/okf_wiki.yml` | W-B | Fast PR gate on `ubuntu-latest`; wires into `pre-merge-gate-check.sh`; reporting idiom matches existing gates. |
| **W-D** | `docs/knowledge/index.md`, `log.md`, `scripts/okf_scaffold.py` | W-A | Bundle scaffold + deterministic `index.md` generator + `log.md` + intake CLI skeleton (lean). |
| **W-E1..En** | `docs/knowledge/<domain>/<concept>.md` (disjoint per file) | W-A | Seed-concept authoring, ~1–3 concepts/WP from a specific source cluster. Parallelizable 0-conflict wave. |
| **W-F** *(Campaign 4)* | the updater skill/CLI | W-B,W-D | Commit-diff → stale-flag → regen-dependents loop. |

### 5.2 DoD / invariants / completeness / quality — inherited by EVERY WP (SOTA, severe)

**Invariants (inviolable — a breach is a RED gate, never waived without explicit human authorization):**
1. Every non-reserved concept `.md` has parseable frontmatter with a **non-empty `type`** (OKF conformance).
2. Every concept has **≥1 `source_files` path that resolves** to an existing repo file.
3. Every concept has a **`checkpoint_sha`** and is **not stale** (no cited source changed after the checkpoint).
4. Every inline `path:line` citation **resolves** to an existing file (line within bounds).
5. **No orphan concepts** — every concept reachable from `index.md`; **no broken bundle-relative links** to declared concepts.
6. Reserved filenames (`index.md`, `log.md`) are **never concept docs**.
7. The generated layer is **marked `GENERATED` and never hand-edited**; the source of truth is the code + the cited sources.

**DoD (a WP merges only if ALL hold):**
- Change-scoped validator (`validate_okf.py`) passes; OKF-conformance + freshness green.
- The lead **cold-checks every concept's factual claims against its cited anchors** — zero hallucinated facts (AP-5: never trust the agent's self-report).
- `CHANGELOG.md [Unreleased]` entry present; **DCO `Signed-off-by`** trailer on every commit.
- Branch is on-baseline (no off-baseline drift; post-flight sweep clean).
- The compact return-card matches the dispatched return-shape.

**Completeness criteria (anchored on a red→green suite, NOT on the slice — a bad cut is slower, never half-built):**
- The fixture corpus (W-B) has a known-GOOD bundle that MUST pass and ≥1 known-BAD fixture per invariant
  that MUST fail. The validator is complete only when every invariant has a failing fixture it catches.
- Every concept candidate in §2.4 either has a concept doc **or** an explicit `deferred:` marker with a
  reason (no silent omission — silent truncation reads as "covered everything").

**Quality standards (SOTA, severe):**
- **No unverified claim ships.** Every factual statement cites a resolvable anchor or is removed.
- **No prose-padding** — structural markdown (headings/lists/tables), OKF-conventional headings.
- **Root-cause only** — a failing gate is fixed at the root, never `#[allow]`/`--no-verify`/"later".
- **Zero-drift tolerance** — gate the bound; a stale concept blocks merge.

---

## 6. Next step

Awaiting stakeholder ratification on **SC1 (scope ceiling)** and **SC2 (priority vs launch)**. On
ratification → **Campaign 2**: author the failing acceptance suite, run the cold suite-critic, and
**freeze the OKF-CoreLink profile + concept template + validator contract** before any build WP is
dispatched. No code is written until the contract is frozen.
