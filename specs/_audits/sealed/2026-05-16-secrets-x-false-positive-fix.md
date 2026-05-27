---
id: "AUDIT-2026-05-16-SECRETS-X-FALSE-POSITIVE-FIX"
type: "audit_report"
doc_status: "FROZEN"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "secrets", "r-prep", "wave-21", "small-closure", "validator-tighten", "deploy-gate"]
---

# Secrets-Checklist Bash Validator — Single-Letter `X` False-Positive Fix (Wave-21)

> **Audit Date:** 2026-05-16 · **Branch:** `wt/r-prep-secrets-x-false-positive` · **Lane:** R-PREP · **Wave:** 21 small-closure
> **Reviewer:** Gustavo Schneiter
> **Files touched:** `scripts/secrets-checklist-verify.sh`
> **Base commit:** `30e5f66` (main, post-wave-20 merge)
> **Cross-ref:** `specs/_audits/sealed/2026-05-16-secrets-matrix-tighten.md` (wave-20 sibling — Python validator tighten)
> **Disposition:** Tightens the env-var name regex in the bash deploy-gate from `[A-Z][A-Z0-9_]*` (≥1 char) to `[A-Z][A-Z0-9_]+` (≥2 chars) to eliminate the single-letter `X` false-positive injected by the `${{ secrets.X }}` documentation placeholder in `.github/workflows/_TEMPLATE.yml.md`. Adds a `--self-test` mode that exercises the new shape against canonical accept/reject fixtures. No production code touched.

---

## 1. Context

Wave-20's secrets-matrix-tighten audit (`specs/_audits/sealed/2026-05-16-secrets-matrix-tighten.md`, commit `c590a67`) flagged a pre-existing inconsistency between the two parallel secret validators:

- **`scripts/validate_secrets_matrix.py`** — the daily-cron Python validator. Uses AST/string-literal extraction; never flags `X`.
- **`scripts/secrets-checklist-verify.sh`** — the GitHub Actions deploy-gate bash validator wired into `.github/workflows/cf-deploy-prod.yml`. Uses `grep -oE` over five patterns; was flagging `X` as a `code_only` drift.

Bash-script output on base `30e5f66`:

```
ERROR: the following env vars are referenced in code but NOT in docs/internal/secrets-checklist.md:
  - NEON_TEST_DSN
  - X
```

`NEON_TEST_DSN` is a pre-existing legitimate-or-missing-allowlist drift unrelated to this WI. `X` is the false-positive this audit closes.

### 1.1 Root cause

The bash validator's GHA-secrets extractor was:

```
grep -rEh '\$\{\{\s*secrets\.[A-Z][A-Z0-9_]*\s*\}\}' .github/workflows/
```

The `[A-Z0-9_]*` quantifier admits zero-length tails, so the canonical pattern `[A-Z][A-Z0-9_]*` matches any leading-uppercase token of **≥ 1 character**. The `.github/workflows/_TEMPLATE.yml.md` template documents the step-scoped secrets-binding idiom with a literal placeholder:

```
10. **Steps that read secrets without scoping** — declare
    `env: SECRET: ${{ secrets.X }}` at the step level, not workflow level.
```

That `secrets.X` is a documentation placeholder — "any secret X" — not a real env-var reference. The `*` quantifier accepted it as a legitimate name, drove the bash extractor's output to include `X`, and the comparison against the matrix (where no row exists for a one-letter name) raised it as `MISSING_FROM_MATRIX`.

### 1.2 Why the Python validator did not have this bug

`scripts/validate_secrets_matrix.py` excludes `.md` files from its workflow scan, so the `_TEMPLATE.yml.md` template never entered its corpus. The bash script's extractor uses `grep -rEh ... .github/workflows/` which, by default, walks every regular file under that directory regardless of extension. Rather than excluding `.md` files (which would silently mask future similar leakage from real `.md` files we care about), this audit tightens the regex itself — the proper fix is to require env-var names to actually look like env-var names.

---

## 2. Fix

### 2.1 Regex change

Across all five extractor patterns in `scripts/secrets-checklist-verify.sh` (matrix extractor + 4 code extractors: Rust `env::var`, Rust `env::set_var`, TS `process.env.X`, TS `process.env["X"]`, GHA `${{ secrets.X }}`), the name shape:

```
[A-Z][A-Z0-9_]*   →   [A-Z][A-Z0-9_]+
```

This raises the minimum length from 1 character to 2 characters. The shortest real env var in `docs/internal/secrets-checklist.md` is `PORT` (4 chars); the empirical minimum across all five extractor surfaces in the workspace is also ≥ 4 chars. The new 2-char floor is well below the empirical minimum and strictly tighter than the documentation placeholder shape — no real env var is at risk of being missed.

### 2.2 Self-test mode

Added a `--self-test` mode (~50 lines, runs before any repo I/O) that:

1. Creates a temp dir with one `.rs` fixture, one `.ts` fixture, and one `.yml` workflow fixture.
2. Each fixture contains both canonical valid names (`PORT`, `DT_API_KEY`, `STATUSPAGE_API_KEY`) and a single-letter `X` (the rejection case).
3. Runs the same extractors against the temp dir.
4. Asserts the extracted set is exactly `{DT_API_KEY, PORT, STATUSPAGE_API_KEY}` — no `X`.

Self-test invocation:

```
$ bash scripts/secrets-checklist-verify.sh --self-test
self-test: OK (regex accepts ≥2-char canonical names; rejects single-letter 'X')
$ echo $?
0
```

The self-test is hermetic (uses `mktemp -d` + `trap rm`), runs entirely outside the repo, and short-circuits before any production extraction. CI may invoke `secrets-checklist-verify.sh --self-test` separately from the deploy gate; this audit does not wire the self-test into CI (single-purpose change), but the entry point is in place for a follow-on if desired.

### 2.3 Doc-comment hardening

Added a top-of-file comment block documenting the canonical shape, why single-letter tokens are rejected, and a pointer to this audit. Future readers do not need to spelunk git history to understand the `+` (not `*`) quantifier.

---

## 3. Verification

### 3.1 Before (base `30e5f66`)

```
$ bash scripts/secrets-checklist-verify.sh ; echo exit=$?
secrets-checklist-verify: matrix has 125 env vars; code references 103 unique non-allowlisted env vars.

ERROR: the following env vars are referenced in code but NOT in docs/internal/secrets-checklist.md:
  - NEON_TEST_DSN
  - X
  ...
exit=1
```

False-positive count (this WI's scope): **1** (`X`).

### 3.2 After (this audit)

```
$ bash scripts/secrets-checklist-verify.sh ; echo exit=$?
secrets-checklist-verify: matrix has 125 env vars; code references 102 unique non-allowlisted env vars.

ERROR: the following env vars are referenced in code but NOT in docs/internal/secrets-checklist.md:
  - NEON_TEST_DSN
  ...
exit=1
```

False-positive count (this WI's scope): **0**. `NEON_TEST_DSN` is pre-existing on base `30e5f66`, is a real env-var name (not a regex false-positive), and is out of scope for this WI (a separate triage decides whether it lands in `ALLOWLIST_REGEX` or as a new matrix row).

### 3.3 Self-test

```
$ bash scripts/secrets-checklist-verify.sh --self-test ; echo exit=$?
self-test: OK (regex accepts ≥2-char canonical names; rejects single-letter 'X')
exit=0
```

### 3.4 Counter-check: code-vars count delta

Before: `103 unique non-allowlisted env vars`. After: `102`. Delta: `-1`, exactly the eliminated `X`. No legitimate name was dropped (the would-have-been-shortest-real-name probe found nothing ≤ 1 char in any extractor surface across the workspace).

---

## 4. Compliance impact

- **SOC 2 CC6.1 (logical access — credentials).** The deploy-gate validator now refuses to deploy on the canonical drift surface (code/matrix mismatch) but no longer trips on a documentation placeholder. Reduces false-friction in the deploy path; does not weaken any real drift detection.
- **CTRL-PRIV-001 (no credential plaintext).** Unchanged. The validator continues to scan for *names*, never values.
- **Drift policy.** Unchanged. Adding `env::var("FOO")`, `process.env.FOO`, or `${{ secrets.FOO }}` for any name `FOO` of length ≥ 2 still requires a matrix row in the same PR.
- **Audit observability.** The `--self-test` mode gives future maintainers a one-command regression check for the regex shape, so accidental relaxation back to `*` (or accidental over-tightening) is caught locally before reaching CI.

---

## 5. Decision log

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-16 | Tighten regex from `*` → `+` (≥ 2-char floor) | Strictly tighter than the documentation-placeholder shape (`X`, 1 char) and well below the empirical-minimum real env var (`PORT`, 4 chars). Workspace-wide probe confirmed no real ≤ 1-char env var consumer. |
| 2026-05-16 | Tighten the regex (not exclude `.md` from the walk) | Excluding extensions silently masks future similar leakage; tightening the name shape encodes the actual invariant ("env var names are ≥ 2 chars") at the extraction site. |
| 2026-05-16 | Add `--self-test` mode in-script (not a sibling test file) | Sibling file would double the touched-surface and require a separate CI hook. In-script `--self-test` is hermetic, zero-dep, and discoverable to anyone who reads the script. |
| 2026-05-16 | Do not address `NEON_TEST_DSN` drift in this WI | It pre-exists on base `30e5f66` and is a legitimate-or-allowlist-gap drift, not a regex false-positive. Out of scope for the single-purpose wave-21 small-closure. |
| 2026-05-16 | Do not wire `--self-test` into CI in this WI | Single-purpose change. The entry point is in place; CI wiring (e.g., a one-line addition to a lint workflow) is a trivial follow-on if/when desired. |

---

## 6. Files changed

- `scripts/secrets-checklist-verify.sh` — tightened env-var name regex from `[A-Z][A-Z0-9_]*` → `[A-Z][A-Z0-9_]+` across all five extractor patterns; added `--self-test` mode (~50 lines, hermetic); added top-of-file doc comment documenting the canonical shape and pointing here.
- `specs/_audits/sealed/2026-05-16-secrets-x-false-positive-fix.md` — this audit.

No production code touched. No spec touched. No runbook touched. Closure is deploy-gate-validator-only.
