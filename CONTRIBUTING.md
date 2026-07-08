# Contributing to CoreLink

> ## 0. GA-1 FEATURE FREEZE IS ACTIVE (effective 2026-05-16)
>
> CoreLink is in a **GA-1 feature-freeze window** that runs from 2026-05-16
> through the GA cutover and the post-GA T+7d clean-state observation
> period. **New features will not be accepted to `main` during the
> freeze.** Only three exception classes can land:
>
> 1. **P0 security fixes** (CVSS ≥ 7.0 on a reachable surface) — 2-key
>    approval (Owner + Security Lead) required.
> 2. **P1 GA-blocker fixes** (defects that block `RB-GA-CUTOVER.md` §0
>    greenlight) — 2-key approval (Owner + on-call SRE) required.
> 3. **Cosmetic doc fixes** (typo / broken link / formatting; no semantic
>    change) — single CODEOWNER approval OK.
>
> Every commit that touches a frozen surface MUST carry a
> `FREEZE-EXCEPTION: <class>` trailer in its body (one of
> `P0-security`, `P1-ga-blocker`, `cosmetic-doc`, or `implicit-allow`).
> The `scripts/check-ga-freeze-allowed.py` gate enforces this.
>
> **Frozen surfaces:** spec corpus, invariant registry, ADR set, public
> API surface (`crates/corelink-api/`, `apps/server/src/routes/`),
> OpenAPI envelope (`openapi/`), runbooks (`specs/_runbooks/`),
> dashboards (`dashboards/`), migrations, schemas.
>
> **Implicitly allowed (no exception trailer required):** anything under
> `specs/_audits/`, `specs/_compliance/`, `reports/`, plus
> `CHANGELOG.md`, `TODO.md`, `ROADMAP-TO-GA.md`.
>
> Full allowlist + decision protocol + thaw conditions live in
> [`specs/_audits/sealed/2026-05-16-ga-1-feature-freeze.md`](./specs/_audits/sealed/2026-05-16-ga-1-feature-freeze.md)
> §3 / §4 / §6. Read it before opening a PR that touches a frozen path.
>
> If your contribution does not fit one of the three exception classes,
> please hold the PR and re-open it after the Owner publishes the
> companion **thaw declaration** post-GA T+7d clean.

---

Thanks for thinking about contributing to CoreLink! This file is the
**community-facing** entry point. Maintainers and full-time contributors
should also read `docs/internal/AUTHOR-PRE-PR-CHECKLIST.md` and
`docs/internal/CODE-REVIEW-CHECKLIST.md`.

This is a lightweight guide — not a legal contract. CoreLink does not
require a Contributor License Agreement (CLA). We use the
[Developer Certificate of Origin (DCO)](https://developercertificate.org/)
instead.

---

## 1. Where to start

- **Bugs** → use [`.github/ISSUE_TEMPLATE/bug_report.yml`](./.github/ISSUE_TEMPLATE/bug_report.yml).
- **Feature ideas** → use [`.github/ISSUE_TEMPLATE/feature_request.yml`](./.github/ISSUE_TEMPLATE/feature_request.yml).
- **Security findings** → do NOT file a public issue. See
  [`SECURITY.md`](./SECURITY.md) or email `security@humangr.com`.
- **Open-ended questions / design discussions** →
  [GitHub Discussions](https://github.com/HumanGuardrail/corelink-server/discussions).
- **Looking for something small to do?** Issues tagged
  `good first issue` are scoped to roughly half a day of work.

---

## 2. Local build, lint, test

CoreLink targets stable Rust per `rust-toolchain.toml`. Before opening a
PR, the following commands must pass locally:

```bash
cargo build --workspace
cargo clippy --workspace --tests -- -D warnings
cargo test --workspace
python3 scripts/validate_specs.py        # spec hygiene (only if you touched specs/)
```

CI re-runs all four on every PR (plus deny / advisory checks).

The apps under `apps/` (admin-ui, docs, server) have their own per-app
build steps documented in their respective `README.md` files.

---

## 3. Branch + commit conventions

- **Branch name**: `wt/<wave>-<wi>-<slug>` for maintainers, or
  `<your-handle>/<short-desc>` for community PRs. Avoid spaces and
  uppercase letters.
- **Commit subject**: `<type>(<scope>): <subject>` — for example,
  `fix(byok-aws): retry envelope decrypt on transient KMS 5xx`. Common
  types are `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `ci`.
- **Commit body**: explain **why**, not what. The diff already shows the
  what.

### DCO sign-off (required)

Every commit must end with a `Signed-off-by: <name> <email>` trailer.
The easy way:

```bash
git commit -s -m "fix(byok-aws): ..."
```

If you forgot, fix the last commit with `git commit --amend -s`, or use
`git rebase --signoff <base>..HEAD` for a batch.

The DCO CI check will block merge if any commit is missing the trailer.

---

## 4. Pull request lifecycle

1. Fork the repo and push your branch to your fork.
2. Open the PR against `main`. The PR template
   ([`.github/PULL_REQUEST_TEMPLATE.md`](./.github/PULL_REQUEST_TEMPLATE.md))
   asks for a summary, test plan, and Author Pre-PR Checklist — fill it
   out before requesting review.
3. CI runs build, clippy, tests, spec validation, license check, and
   cargo-deny. Automation also adds `area:*` and `size:*` labels.
4. A `CODEOWNERS` reviewer is auto-assigned. Maintainer response targets:
   - First triage: 3 business days.
   - Substantive review: 7 business days for `size:XS`-`size:M`; longer
     for larger or security-sensitive surfaces.
5. Address comments by pushing **new commits** (do not force-push during
   review — it disrupts comment threading). Squash on merge happens
   maintainer-side.
6. Once approvals + required checks are green, a maintainer merges.

### `size:XL` PRs

PRs labelled `size:XL` (>= 500 lines changed, lockfiles excluded) will
likely be sent back with a request to split. See the bot comment for
suggested partitioning strategies.

---

## 4.1 Property test density gate

Every crate with `INV-*` references in `src/` or `tests/` MUST carry at
least 1 proptest `#[test]` per distinct `INV-*` reference (ratio ≥ 1.0).
The PR-gate workflow `.github/workflows/proptest-density-gate.yml`
enforces this.

How to run locally:

```bash
bash scripts/audit_proptest_density.sh           # full density report
bash scripts/check_proptest_density_gate.sh      # PR gate (exit 1 on fail)
```

If a crate must temporarily land below 1.0 (e.g. an INV is referenced
but the matching proptest is in a follow-up WI), add the crate to
`scripts/proptest-density-allowlist.txt` with a closing WI ID. The
allowlist is review-gated by an `@code-owner` so silent regressions
don't accumulate; the list shrinks as follow-up WIs land.

Audit baseline: `specs/_audits/sealed/2026-05-15-proptest-density.md`.
Followup tickets: `specs/_audits/sealed/proptest-followup-tickets.md`.

---

## 5. What we are unlikely to accept

Saying "no" is part of maintaining a stable product. Common rejection
patterns:

- New crates without a clear consumer in `apps/`.
- BYOK / audit-chain changes without proptest invariants.
- New `unsafe` blocks outside FFI crates.
- Use of `unwrap` / `expect` / `panic!` / `todo!()` / `unimplemented!()`
  in `src/` (outside `#[cfg(test)]`).
- Direct `tokio::` imports in non-`main.rs` `src/` files — runtime
  selection is centralised.
- Changes that regress `python3 scripts/validate_specs.py` failure count.
- PRs without a Test plan in the description.

Most of these are spelled out in the PR template's Author Pre-PR
Checklist.

---

## 6. Code of Conduct

Be respectful, assume good faith, focus on the work. We do not yet
maintain a separate `CODE_OF_CONDUCT.md`; in the meantime, the
[Contributor Covenant v2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct/)
applies. Report Code-of-Conduct concerns to `conduct@humangr.com`.

---

## 7. License

By contributing, you agree your contribution is licensed under the same
terms as the rest of the repository (see [`LICENSE`](./LICENSE) if
present, or the per-crate license headers).

Thanks again — your contribution makes CoreLink better.
