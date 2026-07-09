# Handoff — OKF Wiki merge (7 PRs)

**For:** the CoreLink TL owning CI / merge · **From:** the OKF-wiki program (session 2026-06-26)

## TL;DR
The OKF code-grounded architecture wiki is **complete: 146/146 concepts, adversarially
truth-certified** (0 BLOCKER, 11 MAJOR + ~45 MINOR all found by adversarial review and remediated).
It ships as **6 STACKED PRs (#506→#514)** plus **1 independent corrections PR (#516)**. All are
content-green locally (`validate_okf.py` + the `okf_wiki` gate). Merge the stack in order; #516 is
independent. One gate-enforced coupling between #516 and the stack (below).

## Merge order — STACKED (each PR's base is the previous; GitHub auto-retargets to `main` as each merges)
| # | PR | base | content |
|---|----|------|---------|
| 1 | **#506** `feat/okf-wiki-foundation` | main | format + validator + `okf_wiki` gate + scaffold + manifest (0 concepts) |
| 2 | **#508** `feat/okf-wiki-concepts-arch` | #506 | wave-1: 19 architecture concepts |
| 3 | **#509** `feat/okf-wiki-concepts-arch2` | #508 | wave-2: 19 (storage/tenancy/crates) |
| 4 | **#511** `feat/okf-wiki-audit-remediation` | #509 | truth-audit + remediation of the 38 |
| 5 | **#512** `feat/okf-wiki-concepts-adr` | #511 | wave-3: 76 ADR concepts |
| 6 | **#514** `feat/okf-wiki-concepts-docx` | #512 | wave-4: 32 doc-extraction → **146 total** |

**Independent:** **#516** `fix/okf-audit-corrections` (base `main`) — `INV-BAZEL-NO-GROPC`→`GRPC` typo
+ 3 ADR H1 id typos. Not stacked; merge any time.

## The one coupling (gate-enforced — impossible to miss)
#516 edits `crates/corelink-bazel-bridge/src/lib.rs:49`. The OKF `adapter-hosts` concept (in #509)
declares + cites that file. So whichever merges **second**:
- the `okf_wiki` **freshness gate (C5) fires** on `docs/knowledge/crates/adapter-hosts.md` (a cited line
  changed since its checkpoint) → RED.
- **Reconcile (~2 min):** in `adapter-hosts.md`, drop the "the code currently spells it `GROPC`" note
  (it's `GRPC` now) and advance its `checkpoint_sha` to the new HEAD; re-run `python3 scripts/validate_okf.py` → green.

The gate makes this self-announcing — it cannot silently desync.

## CI note
The `okf_wiki` gate runs on **ubuntu-latest** (`fetch-depth: 0`), zero self-hosted/Mac load. Each PR's
`validate_okf` was confirmed green locally at integration. **Earlier this session the PR CI showed
uniform ~5s no-log failures across ALL jobs** (dco/gitleaks/spec/okf) — that was infra/cancellation
(concurrent pushes + a transient), **NOT content**; identical jobs passed 20 min prior. Re-run if seen.

## Product flag (NOT a merge blocker — for the owner)
The wave-1 + wave-4 audits surfaced that the cross-tenant `_public` dedup **moat is NOT built for npm**
(only pip/brew/oci). npm tarball bytes are per-tenant; cross-tenant npm dedup is a tracked, unbuilt
enhancement. If the sales/moat narrative assumes npm, there's a **pitch↔code gap** — decision (build vs
reword) is the owner's.

## Artifacts
- Bundle: `docs/knowledge/` (146 concepts). Contract: `docs/internal/okf-wiki/01-okf-corelink-profile.contract.md`.
- Validator: `scripts/validate_okf.py` + self-running suite `tests/okf/run_fixtures.sh`. Gate: `.github/workflows/okf_wiki.yml`.
- Audit report + 3 remediation worklists: `docs/internal/okf-wiki/audits/`.
- The anti-drift gate is empirically proven (mutate a cited line → C5 STALE → red); two-tree `git diff`, never `git log -L`.
