---
id: "TECHLEAD-CHECKLIST"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["techlead", "checklist", "orchestrator", "verification"]
---

# Tech Lead / Orchestrator — Per-Agent-Deliverable Checklist

**Mandate (user-stated):** *"voce e o techlead e o teamleader aqui. Verificar os commits, PRs, organizar os merges, documentar tudo."*

Every Sonnet agent SEAL gets walked through this checklist BEFORE merge to `main`. Trust the agent's summary; verify with cold tools.

**Order matters** — fail-fast cheap checks first; expensive ones last.

---

## L0 — Sanity (60 seconds, every deliverable)

- [ ] **L0.1** Worktree exists at expected path; branch matches WI name
- [ ] **L0.2** `git status --short` shows clean tree post-SEAL (no uncommitted bits left behind)
- [ ] **L0.3** `git log --oneline -3` shows the SEAL commit at HEAD with the canonical commit message format
- [ ] **L0.4** Agent's reported test count matches actual `cargo test --no-run` or `pnpm test` output (random spot-check the number; agents have lied about counts before)
- [ ] **L0.5** Files claimed in the report actually exist (`ls` the absolute paths)

If ANY L0 fails → STOP. Dispatch fixer or investigate before merging.

---

## L1 — Build + Lint Gates (2-5 min)

- [ ] **L1.1** `cargo build --workspace` exit 0 inside the worktree
- [ ] **L1.2** `cargo clippy --workspace --tests -- -D warnings` exit 0 (rust 1.91 strict lints; uninlined_format_args, default_constructed_unit_structs, etc.)
- [ ] **L1.3** `cargo test --workspace --no-run` exit 0 (compiles all test binaries; doesn't run them — fast)
- [ ] **L1.4** No new `#[allow(clippy::*)]` at function-level (only crate-level `#![allow]` in test modules is acceptable; charter constraint)
- [ ] **L1.5** No `--no-verify` in any commit (`git log --pretty=format:"%H %s" | grep -i no.verify` empty)
- [ ] **L1.6** Workspace `Cargo.toml` has the new crate listed in `[workspace] members`

If pure-docs deliverable: skip L1.1-1.5; verify `python3 scripts/validate_specs.py` 0 new failures instead.

---

## L2 — Charter Constraints (5 min)

This is the SOTA invariant set. Failures here = regression.

- [ ] **L2.1** All public enums and structs in new code have `#[non_exhaustive]` (or are explicitly internal). Grep `pub enum\|pub struct` in new files.
- [ ] **L2.2** Zero `unsafe` outside FFI boundary modules (`corelink-py`, `corelink-go`, `corelink-wasm`, `corelink-clerk-cf`'s `worker::*` FFI). Run `grep -rn "unsafe " <new-crate>/src/`.
- [ ] **L2.3** No `tokio` import in `src/` (tests + `main.rs` only). `grep -rn "use tokio" <new-crate>/src/` should be empty unless inside `#[cfg(test)]`.
- [ ] **L2.4** No `prop_assert!(matches!(..., Variant { .. }))` anti-pattern (the S-08 P1-1 lesson). `grep -rn "prop_assert!(matches" <new-crate>/` should be empty.
- [ ] **L2.5** `PROPTEST_CASES` reads from env via runtime fn, NOT const literal. `grep -rn "ProptestConfig::with_cases\|cases:" <new-crate>/tests/` — every numeric arg should be `proptest_cases(N)` helper, not bare integer literal.
- [ ] **L2.6** Audit fail-CLOSED ordering: `lookup → emit_audit → mutate_state`. Find every `audit.emit(` call in new code; verify the preceding lines did the lookup and the following lines do the mutate. **NEVER** mutate before audit emit.
- [ ] **L2.7** Zero `unwrap()` / `expect()` / `panic!` / `unimplemented!()` / `todo!()` outside `#[cfg(test)]`. Crate-level `[lints.clippy]` should already deny these; verify.
- [ ] **L2.8** `#![forbid(unsafe_code)]` at the crate root of every new non-FFI crate.
- [ ] **L2.9** Secrets (API keys, tokens, PATs, JWT material) NEVER logged. Grep `tracing::*!\|log::*!\|println!\|eprintln!\|dbg!` near auth code; values must be redacted (newtype with custom Debug, or `***` literals).

---

## L3 — Security & Privacy (10 min)

- [ ] **L3.1** Every new HTTP endpoint validates auth before processing (Bearer PAT or Clerk session)
- [ ] **L3.2** Every state mutation emits an audit event (no silent state changes)
- [ ] **L3.3** No PII in audit payloads beyond hashes/IDs (CTRL-PRIV-001). Search audit emit calls for `email`, `phone`, `name`, `tenant_name` strings; should be hash-derived or absent.
- [ ] **L3.4** Webhook/callback endpoints verify signature before parsing body (Stripe HMAC-SHA256, GitHub HMAC, etc.) with constant-time compare (`subtle::ConstantTimeEq`).
- [ ] **L3.5** Timestamp-bound signatures reject replay (5-min default tolerance).
- [ ] **L3.6** Idempotency keys hashed before storage (don't leak structure to attacker who reads D1).
- [ ] **L3.7** Cross-tenant AAD binding present on every crypto operation that should be tenant-isolated. INV-BYOK-CRYPTO-SOVEREIGNTY = AAD must include `tenant_id` (or HMAC thereof).
- [ ] **L3.8** GDPR/LGPD compliance: 7y retention bound respected; DSR-erasable data marked appropriately.
- [ ] **L3.9** New GitHub Actions workflows use SHA-pinned `uses:` references (40-char hex + `# vX.Y.Z` comment). Floating tags = P0.

---

## L4 — Test Quality (10 min, spot-sample)

- [ ] **L4.1** ≥ 5 unit tests per new module of substance
- [ ] **L4.2** ≥ 1 property test per non-trivial invariant (10k PR cases / 100k nightly via PROPTEST_CASES)
- [ ] **L4.3** ≥ 1 adversarial / negative test per public happy-path function (what happens with malformed input, exhausted budget, race, replay, etc.)
- [ ] **L4.4** Test names follow `prop_*` / `adversarial_*` / `prop_assert_*` convention (greppable)
- [ ] **L4.5** No test imports `tokio::*` directly when synchronous test would work (charter)
- [ ] **L4.6** If new crate exposes async API: at least 1 test exercises it via runtime
- [ ] **L4.7** Tests use deterministic seeds; randomness gated through `proptest` framework, not `rand::thread_rng()` ad-hoc
- [ ] **L4.8** Real-network tests gated `#[ignore]` + `#[cfg(feature = "live-integration")]`; default `cargo test` doesn't touch the wire

---

## L5 — Documentation (3 min)

- [ ] **L5.1** Every new crate has crate-level rustdoc (`//!` at top of `lib.rs`) covering: purpose, charter constraints honored, API surface summary, examples
- [ ] **L5.2** Every public type has `///` docstring (clippy `missing_docs = "deny"` already enforces; verify it's enabled)
- [ ] **L5.3** README.md updated if new top-level concept
- [ ] **L5.4** ADR filed for any architectural decision (`specs/03_architecture/adrs/ADR-S20-*.md`)
- [ ] **L5.5** Spec contract changelog row added if scope changed
- [ ] **L5.6** Frontmatter on new spec docs (`type`, `doc_status`, `audit_status`, `version`, `created`, `updated`, `owner`, `final_approver`, `reviewers`)
- [ ] **L5.7** Deferred items documented explicitly with spec § reference (no silent skips)

---

## L6 — Spec Hygiene (2 min)

- [ ] **L6.1** `python3 scripts/validate_specs.py` exits 0 (or only pre-existing failures unchanged)
- [ ] **L6.2** `python3 scripts/check_migrations_additive.py` exits 0 (no DROP / ALTER … DROP statements)
- [ ] **L6.3** `python3 scripts/validate_references.py` (if exists) — no new dangling references
- [ ] **L6.4** Migration numbering: next-free; no collisions with sibling worktrees
- [ ] **L6.5** Cross-reference INVs / CTRLs / FMs / EVTs / PATs to canonical registries
- [ ] **L6.6** `python3 scripts/validate_canonical_consistency.py` — no new orphan `INV-*` refs in code and no regression vs baseline in `specs/_audits/sealed/2026-05-15-canonical-consistency-baseline.md` (triage flow: `specs/_runbooks/RB-CANONICAL-DRIFT.md`)

---

## L7 — Merge Hygiene (5 min)

- [ ] **L7.1** Branch is up-to-date with `main` OR plan stated for rebase
- [ ] **L7.2** Cargo.lock conflicts resolved via `--ours` + `cargo update -w` (not blind theirs)
- [ ] **L7.3** Workspace `Cargo.toml` `members` list unioned (no entries lost)
- [ ] **L7.4** No conflict markers remain (`grep -rn "<<<<<<<\|>>>>>>>" --include=*.toml --include=*.rs --include=*.md`)
- [ ] **L7.5** Test binary count after merge ≥ baseline count (no regression)
- [ ] **L7.6** Final commit signed-off / co-authored properly
- [ ] **L7.7** Pre-commit hooks pass without `--no-verify`

---

## L8 — Documentation of Decisions (always)

Tech lead MUST do, agent CANNOT:

- [ ] **L8.1** Log every merge in commit message with `merge wt/<branch> (one-line summary) into main`
- [ ] **L8.2** For any P0 surfaced post-merge: file a follow-up commit with `fix(<wave>): P0 remediation` + link to audit doc
- [ ] **L8.3** Update `ROADMAP-TO-GA.md` change log if wave completes (e.g. R-2 fully shipped)
- [ ] **L8.4** Tag major milestones (`<wave>-sealed`, `<phase>-complete`)
- [ ] **L8.5** Push regularly (every 3-5 merges) so origin is not far behind local

---

## L9 — Risk-Assessment (5 min, before SEAL tag)

Before tagging a phase complete, the tech lead asks aloud:

- [ ] **L9.1** What's still mocked / trait-abstraction-deferred? Is the trait API stable for prod swap?
- [ ] **L9.2** What's still human-action-bound? Is it tracked in `ROADMAP-TO-GA.md` §9 Human Track?
- [ ] **L9.3** What CHANGED in this wave's contract vs the spec contract? Did we silently scope-shift?
- [ ] **L9.4** Is there a customer-visible API or schema change? Migration story documented?
- [ ] **L9.5** Did we INTRODUCE any new security primitive (cipher / KDF / proof) without an ADR?
- [ ] **L9.6** Are there 24/7 implications? Did we add a new SLO that needs Grafana / PagerDuty / runbook?
- [ ] **L9.7** What's the worst-case if THIS specific code shipped to prod tomorrow with a bug?

---

## L10 — Rolling Hygiene (every ~5 merges)

- [ ] **L10.1** `git worktree list` — any dead worktrees? Clean up
- [ ] **L10.2** `df -h /` — disk space ok (target ≥ 40GB free)
- [ ] **L10.3** `validate_specs.py` baseline failures count unchanged (or DOWN, never up)
- [ ] **L10.4** Test binary total count compared to last checkpoint (trend tracked)
- [ ] **L10.5** Push origin if local is > 10 commits ahead

---

## Per-deliverable cycle time budget

- **L0**: 1 min
- **L1**: 3 min (background `cargo build`)
- **L2**: 5 min (greps + read 1-2 critical files)
- **L3**: 8 min (read security-adjacent code)
- **L4**: 5 min (spot-check test naming + 1-2 actual test files)
- **L5**: 3 min
- **L6**: 2 min
- **L7**: 4 min during merge
- **L8**: 2 min after merge
- **L9**: 5 min before tag

**Total per agent SEAL: ~38 min worst case, ~10 min if mostly trusted prior agent quality.**

For batched merges (5+ branches at once): one L0-L7 pass per branch + one L9 pass at the end.

---

## Anti-patterns I (orchestrator) have committed before (acknowledge + repent)

1. **Trusted agent's reported test count without sampling** (R2-1 Stripe initially showed "0 tests" then I dug and found 27 + 4 prop). LESSON: cold-spot-check counts always.
2. **Allowed agents to leave uncommitted work** (R2-1, R2-10, R6-prep) → had to commit myself or send resume agent. LESSON: every dispatch instruction must include "DO NOT report success without committing".
3. **Tagged `s20-impl-sealed` before round-2 review** (caught by R1-1 audit; lucky no regressions). LESSON: round-2 verification BEFORE tag, not after.
4. **Skipped Opus eyeball pass** during the 21-sprint sprint waves — only ran spot-checks. LESSON: add explicit L2 step at every wave-close.
5. **Let validate_specs failures sit "out-of-scope" for 9 sprints** before R-1 fixer closed them. LESSON: 0-tolerance for "pre-existing" failures lingering wave-over-wave.

---

## Cross-links

- `docs/internal/CODE-REVIEW-CHECKLIST.md` — reviewer-side L0-L7 (human reviewers walk this on every PR).
- `docs/internal/AUTHOR-PRE-PR-CHECKLIST.md` — author-side 30-row self-check BEFORE opening PR.
- `.github/PULL_REQUEST_TEMPLATE.md` — embeds the author + reviewer checklists into every PR.
- `.github/CODEOWNERS` — review-routing for "every PR reviewed" GA-Gate.
- `.claude/skills/techlead/SKILL.md` — orchestrator skill that automates this.
- `docs/internal/ENGINEERING-ONBOARDING.md` — Day 4-5 first-PR flow points here.
- `ROADMAP-TO-GA.md` — GA-Gate criteria reference this doc.

---

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Claude Opus 4.7) | Initial checklist post-user-mandate "voce e o techlead". 11 L-levels + cycle time budgets + anti-pattern acknowledgements. |
| 1.1.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7) | Added cross-link section pointing to new CODE-REVIEW-CHECKLIST.md, AUTHOR-PRE-PR-CHECKLIST.md, PR template, CODEOWNERS. GA-Gate "every PR reviewed" prerequisite. |
