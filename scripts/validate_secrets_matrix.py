#!/usr/bin/env python3
"""
validate_secrets_matrix.py — Secrets-matrix drift validator (Python).

Cross-references `docs/internal/secrets-checklist.md` (the canonical 89-row
production-secrets matrix) against the codebase. Emits a structured JSON
report (matrix-only / code-only / in-both) and fails on real drift.

Complements `scripts/secrets-checklist-verify.sh` (deploy-gate, bash):
  * The bash verifier is wired into `cf-deploy-prod.yml` (fail-closed on every
    production deploy).
  * This Python validator is wired into a daily cron + PR gate
    (`.github/workflows/secrets-drift.yml`) and produces a machine-readable
    JSON artifact suitable for dashboards, runbook triage, and SOC 2 CC6.1
    evidence sampling.

Exit codes:
  0 — no `code-only` drift (matrix is sound; `matrix-only` rows soft-warn).
  1 — at least one `code-only` env var found (real drift; PR must add a row).
  2 — invocation error (matrix file missing, unparsable, etc.).

Usage:
  python3 scripts/validate_secrets_matrix.py
  python3 scripts/validate_secrets_matrix.py --json-out report.json
  python3 scripts/validate_secrets_matrix.py --dry-run        # never fail
  python3 scripts/validate_secrets_matrix.py --quiet          # JSON-only

Outputs JSON shape::

  {
    "schema_version": "1",
    "matrix_file": "docs/internal/secrets-checklist.md",
    "summary": {
      "matrix_total": <int>,
      "code_total":   <int>,
      "in_both":      <int>,
      "matrix_only":  <int>,
      "code_only":    <int>,
      "drift":        <int>          // == code_only after allowlist
    },
    "matrix_only": [...],            // dead/forward-looking rows (warn)
    "code_only":   [...],            // real drift (fail)
    "in_both":     [...],            // clean intersection
    "allowlist_skipped": [...]       // env vars filtered as not-a-secret
  }

Cross-references:
  * docs/internal/secrets-checklist.md (canonical matrix)
  * ROADMAP-TO-GA.md §9 (Human Track — credentials)
  * specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md (CC6.1)
  * specs/_runbooks/RB-SECRETS-DRIFT.md (triage)
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from typing import Iterable

REPO_ROOT = Path(__file__).resolve().parent.parent
MATRIX_FILE_REL = "docs/internal/secrets-checklist.md"

# ---------------------------------------------------------------------------
# Allowlist — env vars intentionally not in the matrix.
# Kept in sync with scripts/secrets-checklist-verify.sh (single source of
# truth is the bash regex; we mirror it here for the Python pipeline).
# ---------------------------------------------------------------------------
ALLOWLIST_REGEX = re.compile(
    r"^("
    r"PROPTEST_"
    r"|HOME$|USERPROFILE$"
    r"|CARGO_"
    r"|GITHUB_(TOKEN|OUTPUT|ENV|PATH|STEP_SUMMARY|ACTIONS|REPOSITORY|SHA|REF"
    r"|WORKFLOW|RUN_ID|RUN_NUMBER|ACTOR|EVENT_NAME|EVENT_PATH|JOB|API_URL"
    r"|SERVER_URL|GRAPHQL_URL|WORKSPACE)$"
    r"|RUNNER_"
    r"|GH_TOKEN$"
    r"|GNUPGHOME$"
    r"|SOURCE_DATE_EPOCH$"
    r"|PATH$|PWD$|USER$|SHELL$|TERM$|CI$|TZ$|LANG$|LC_"
    r"|NODE_ENV$|ENVIRONMENT$"
    r"|RUST_"
    r"|TODO_"
    r"|DOCS_BASE_URL$|DOCS_BASE_PATH$"
    r"|LH_BASE_URL$|LH_START_COMMAND$"
    r"|E2E_BASE_URL$|NEXT_PUBLIC_E2E_TEST_MODE$"
    r"|PROJECTS$|SKIP_WEBSERVER$"
    r"|GCP_TEST_KEY_RESOURCE$|GCP_TEST_REGION$"
    r"|DT_API_KEY_TEST_|DT_MOCK_INJECTION_ENABLED$"
    # Wave-20 — `NEON_TEST_DSN` is the `#[ignore]`-by-default Neon-staging
    # integration test DSN consumed by
    # `crates/corelink-audit-chain/tests/neon_shadow_real.rs`. Not a
    # production secret — the production per-region DSN lives at
    # `NEON_DB_URL_<REGION>` (matrix rows 120–124).
    r"|NEON_TEST_DSN$"
    # Wave-33 secrets-matrix drift triage (2026-05-27) — 17 non-secret env
    # vars consumed by wrangler.toml [vars] bindings, build metadata
    # injection, e2e test configuration, or Sentry build-time metadata.
    # None of these carry credential material.
    #
    # CF Pages / git build metadata — injected by Cloudflare Pages or CI;
    # no secrets, pure build provenance.
    #   Consumer: apps/docs/docusaurus.config.ts (SENTRY_DOCS_RELEASE build label)
    r"|CF_PAGES_COMMIT_SHA$"
    r"|GIT_SHA$"
    # wrangler.toml [vars] public config — committed in plaintext in
    # wrangler.toml; no credential material.
    #   Consumer: apps/analytics-worker/src/ingest.ts (CORS allow-list)
    r"|ALLOWED_ORIGINS$"
    #   Consumer: apps/analytics-worker/src/types.ts (Plausible domain label)
    r"|PLAUSIBLE_DOMAIN$"
    #   Consumer: apps/analytics-worker/src/cron/weekly-email.ts (From address)
    r"|DIGEST_FROM$"
    #   Consumer: apps/analytics-worker/src/cron/weekly-email.ts (recipient)
    r"|DIGEST_RECIPIENT$"
    #   Consumer: apps/get-corelink-worker/src/index.ts (install script origin)
    r"|RELEASE_ORIGIN$"
    #   Consumer: apps/get-corelink-worker/src/index.ts (default API endpoint)
    r"|DEFAULT_API_ENDPOINT$"
    # E2E test configuration — CI-only overrides; no secrets.
    #   Consumer: apps/admin-ui/e2e/signup-welcome.spec.ts (auth storage path)
    r"|E2E_AUTH_STORAGE_STATE$"
    #   Consumer: apps/admin-ui/e2e/signup-welcome.spec.ts (docs URL)
    r"|E2E_DOCS_URL$"
    #   Consumer: apps/admin-ui/e2e/get-install-worker.spec.ts (install URL)
    r"|E2E_INSTALL_URL$"
    # Sentry build metadata — non-credential config injected at build/deploy
    # time; environment label, release tag, org/project names.
    #   Consumer: apps/admin-ui/sentry.client.config.ts / sentry.server.config.ts
    r"|NEXT_PUBLIC_SENTRY_ENVIRONMENT$"
    r"|NEXT_PUBLIC_SENTRY_RELEASE$"
    r"|SENTRY_ENVIRONMENT$"
    r"|SENTRY_RELEASE$"
    #   Consumer: apps/admin-ui/next.config.ts (withSentryConfig org/project)
    r"|SENTRY_ORG$"
    r"|SENTRY_PROJECT$"
    # Next.js FRAMEWORK-provided runtime discriminant ("nodejs" | "edge"), set by
    # Next itself — NOT a secret/credential. Surfaced by the Sentry v10 migration:
    # instrumentation.ts branches register() on `process.env.NEXT_RUNTIME`.
    #   Consumer: apps/admin-ui/instrumentation.ts
    r"|NEXT_RUNTIME$"
    # Admin-ui analytics endpoint override — public URL; default is hardcoded
    # in source. No credential material.
    #   Consumer: apps/admin-ui/src/lib/analytics.ts
    r"|NEXT_PUBLIC_ANALYTICS_ENDPOINT$"
    # WP-C1 (2026-05-28) — R2 S3 endpoint URL template for the native container.
    # Declared in [[env.prod.containers]] [vars] (non-secret; the URL itself is
    # public knowledge: https://<account_id>.r2.cloudflarestorage.com).
    # The actual account ID is supplied at runtime via the CLOUDFLARE_ACCOUNT_ID
    # secret (matrix row #49-equivalent). No credential material here.
    r"|R2_S3_ENDPOINT$"
    # 2026-06-02 secrets-matrix reconciliation — non-secret env vars surfaced
    # after excluding .open-next/.wrangler build output from the scan. None carry
    # credential material (test config, R2 bucket names/regions, CF resource IDs).
    #   Test-only alt PAT key — crates/corelink-pat/tests/emit_e2e_seed.rs
    r"|CORELINK_PAT_SIGNING_KEY_HEX$"
    #   E2E user-journey harness config + test-minted PATs (tests/e2e-user-journeys).
    #   The ENTIRE `CORELINK_E2E_*` namespace is black-box test configuration —
    #   endpoint, per-persona PATs, tenant ids, run flags (RUN_SLOW), DSR/introspect
    #   fixtures. NONE are deployed prod secrets; they're supplied OOB only when the
    #   suite is run live. Prefix-allowlisted so the whole namespace stays covered as
    #   the suite grows (was 5 individually-listed vars; the 45-journey rebuild added
    #   RUN_SLOW/TENANT(_B)/PAT_*/TOMBSTONED_HASH/INTROSPECT_KEY).
    r"|CORELINK_E2E_"
    #   E2E Clerk-session minter (scripts/e2e-real-client/clerk-session-minter.mjs):
    #   a LOCAL/dev test harness (Playwright + Clerk testing-token) that reads
    #   these from .env.local to mint a real session bearer for the Clerk-session
    #   -gated journeys. NONE are deployed container secrets — CLERK_FAPI/
    #   CORELINK_API_ENDPOINT/CORELINK_APP_URL are public hosts/URLs, and
    #   CLERK_LIVE_SECRET_KEY is the operator's local .env.local Clerk key (the
    #   DEPLOYED Clerk secret is the matrix's canonical CLERK_SECRET_KEY row).
    r"|CLERK_FAPI$|CLERK_LIVE_SECRET_KEY$|CORELINK_API_ENDPOINT$|CORELINK_APP_URL$|CLERK_JS_VERSION$"
    #   Cloudflare D1 database UUID — committed in wrangler.toml [[d1_databases]];
    #   a resource identifier, not a credential (CF_API_TOKEN gates access).
    r"|D1_DATABASE_ID$"
    #   R2 bucket names + regions — wrangler.toml [vars] deploy config; the R2
    #   access keys (matrix rows #139/#140) are the actual credentials.
    r"|R2_AC_BUCKET$"
    r"|R2_AC_REGION$"
    r"|R2_CAS_BUCKET$"
    r"|R2_CAS_REGION$"
    r"|R2_CHUNK_BUCKET$"
    r"|R2_CHUNK_REGION$"
    r"|R2_TEST_BUCKET$"
    # WP-3 dashboard revival (2026-06-10) — Stripe billing-portal return_url
    # override (public dashboard URL; default hardcoded in source). No
    # credential material — STRIPE_SECRET_KEY (matrix row) is the actual
    # credential.
    #   Consumer: crates/corelink-container/src/customer_d1.rs (from_env)
    r"|CORELINK_PORTAL_RETURN_URL$"
    # 2026-06-10 — public-flip smoke harness (scripts/smoke/). Dedicated
    # low-privilege smoke-user credentials + base-URL override consumed by
    # scripts/smoke/authenticated-smoke.spec.ts. Operator-local test-account
    # creds (same precedent as CORELINK_E2E_TOKEN / NEON_TEST_DSN) — never a
    # deployed production secret, so not a matrix row.
    r"|SMOKE_USER_EMAIL$"
    r"|SMOKE_USER_PASSWORD$"
    r"|SMOKE_BASE_URL$"
    # 2026-06-14 — admin-ui browser render smoke
    # (scripts/e2e-admin-ui-render-smoke.mjs + .github/workflows/e2e-admin-ui-render.yml).
    #   BASE_URL            — the (public) URL to smoke; defaulted to prod in-script.
    #   PW_EXECUTABLE_PATH  — optional local Chromium path (CI uses the default).
    # CI/test config, never a deployed production secret — same precedent as the
    # SMOKE_* smoke-harness vars above.
    r"|BASE_URL$"
    r"|PW_EXECUTABLE_PATH$"
    # 2026-07-09 — client SDK log-level toggle (`CORELINK_LOG=debug` enables
    # verbose logging), NOT a credential. Same class as RUST_LOG / NODE_ENV.
    #   Consumer: sdks/js/src/client.ts (process.env.CORELINK_LOG === "debug")
    r"|CORELINK_LOG$"
    r")"
)

EXCLUDE_DIR_PARTS = {
    "target",
    "node_modules",
    ".next",
    # OpenNext + Wrangler build output: gitignored, generated bundles that embed
    # third-party SDK code (Sentry CI-provider detection → Vercel/Zeit/Azure/CI
    # env-var references). Scanning these surfaced ~130 false-positive vendor env
    # vars that are not CoreLink secrets. Source dirs only — see 2026-06-02 audit.
    ".open-next",
    ".wrangler",
    "dist",
    "build",
    ".git",
    "_archive",
    ".turbo",
    ".pnpm-store",
}

# Matrix row regex: `| 1 | <secret> | `ENV_VAR` | ...`
MATRIX_ROW_RE = re.compile(r"^\|\s*\d+\s*\|")
MATRIX_ENV_VAR_RE = re.compile(r"`([A-Z][A-Z0-9_]*)`")

# Rust env::var("X") / std::env::var("X") / env::var_os("X") / env::set_var("X"
RUST_ENV_RE = re.compile(
    r"(?:std::)?env::(?:var|var_os|set_var)\(\s*\"([A-Z][A-Z0-9_]*)\""
)

# Cloudflare Worker / workers-rs env-binding access:
#   env.secret("STATUSPAGE_API_KEY")
#   env.var("STATUSPAGE_PAGE_ID")
#   env.secret(bindings::STATUSPAGE_API_KEY)
#   env.var(bindings::STATUSPAGE_TENANT_ID)
#
# The literal-string form is captured by group 1. The `path::IDENT` form is
# captured by group 2 and must be resolved against the file-local map of
# `pub const IDENT: &str = "VALUE"` declarations (RUST_BINDING_CONST_RE
# below) to recover the env-var name.
RUST_WORKER_ENV_RE = re.compile(
    r"\.(?:secret|var)\(\s*"
    r"(?:\"([A-Z][A-Z0-9_]*)\"|(?:\w+::)?([A-Z][A-Z0-9_]+))\s*\)"
)

# File-local map: `pub const NAME: &str = "VALUE";`
# Used only to resolve the path::IDENT form of RUST_WORKER_ENV_RE — we
# accept a binding only if the resolved VALUE itself matches the env-var
# shape `[A-Z][A-Z0-9_]*` (filters out SQL constants, error codes, etc.).
RUST_BINDING_CONST_RE = re.compile(
    r'pub\s+const\s+([A-Z][A-Z0-9_]*)\s*:\s*&\'?\s*str\s*=\s*"([^"]+)"'
)

# TS/JS process.env.X and process.env["X"]
TS_ENV_DOT_RE = re.compile(r"process\.env\.([A-Z][A-Z0-9_]*)")
TS_ENV_BRACKET_RE = re.compile(r"process\.env\[\s*\"([A-Z][A-Z0-9_]*)\"\s*\]")

# GHA `${{ secrets.X }}` and `${{ env.X }}` (env. only if uppercase-style)
GHA_SECRETS_RE = re.compile(r"\$\{\{\s*secrets\.([A-Z][A-Z0-9_]*)\s*\}\}")

# Wrangler bindings under [vars] or [env.X.vars] — `KEY = "value"` shape.
WRANGLER_VAR_RE = re.compile(r"^([A-Z][A-Z0-9_]*)\s*=")


# ---------------------------------------------------------------------------
# Matrix parsing
# ---------------------------------------------------------------------------
def parse_matrix(path: Path) -> set[str]:
    """Extract env-var names from the matrix's 4th column (backtick-quoted)."""
    if not path.is_file():
        print(f"ERROR: matrix file not found: {path}", file=sys.stderr)
        sys.exit(2)

    names: set[str] = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        if not MATRIX_ROW_RE.match(line):
            continue
        cols = line.split("|")
        # cols indices: 0 (leading empty), 1 (#), 2 (name), 3 (env var), 4..
        if len(cols) < 4:
            continue
        env_col = cols[3]
        for m in MATRIX_ENV_VAR_RE.finditer(env_col):
            names.add(m.group(1))
    return names


# ---------------------------------------------------------------------------
# Code scanning
# ---------------------------------------------------------------------------
def _iter_files(root: Path, suffixes: Iterable[str]) -> Iterable[Path]:
    suffixes = tuple(suffixes)
    for dirpath, dirnames, filenames in os.walk(root):
        # prune excluded dirs in-place
        dirnames[:] = [d for d in dirnames if d not in EXCLUDE_DIR_PARTS]
        for fn in filenames:
            if fn.endswith(suffixes):
                yield Path(dirpath) / fn


def scan_rust(root: Path) -> set[str]:
    hits: set[str] = set()
    env_var_shape = re.compile(r"^[A-Z][A-Z0-9_]*$")
    for f in _iter_files(root, (".rs",)):
        try:
            txt = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        # std::env::var("X") — direct env access
        for m in RUST_ENV_RE.finditer(txt):
            hits.add(m.group(1))
        # workers-rs `env.secret(...)` / `env.var(...)` — Cloudflare Worker
        # binding access. Pre-build the file-local const map once; only
        # accept values that themselves look like env-var names.
        bindings = {
            m.group(1): m.group(2)
            for m in RUST_BINDING_CONST_RE.finditer(txt)
        }
        for m in RUST_WORKER_ENV_RE.finditer(txt):
            literal, ident = m.group(1), m.group(2)
            if literal:
                hits.add(literal)
                continue
            if ident is None:
                continue
            value = bindings.get(ident)
            if value and env_var_shape.match(value):
                hits.add(value)
    return hits


def scan_ts(root: Path) -> set[str]:
    hits: set[str] = set()
    for f in _iter_files(root, (".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs")):
        try:
            txt = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for m in TS_ENV_DOT_RE.finditer(txt):
            hits.add(m.group(1))
        for m in TS_ENV_BRACKET_RE.finditer(txt):
            hits.add(m.group(1))
    return hits


def scan_workflows(root: Path) -> set[str]:
    hits: set[str] = set()
    wf_dir = root / ".github" / "workflows"
    if not wf_dir.is_dir():
        return hits
    for f in wf_dir.rglob("*.yml"):
        try:
            txt = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for m in GHA_SECRETS_RE.finditer(txt):
            hits.add(m.group(1))
    for f in wf_dir.rglob("*.yaml"):
        try:
            txt = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for m in GHA_SECRETS_RE.finditer(txt):
            hits.add(m.group(1))
    return hits


def scan_wrangler(root: Path) -> set[str]:
    """Extract var/binding names from wrangler.toml files (uppercase tokens)."""
    hits: set[str] = set()
    in_vars_section = False
    for f in root.rglob("wrangler.toml"):
        # Skip excluded paths
        if any(part in EXCLUDE_DIR_PARTS for part in f.parts):
            continue
        try:
            txt = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        in_vars_section = False
        for raw in txt.splitlines():
            line = raw.strip()
            if line.startswith("[") and line.endswith("]"):
                # Heuristic: only collect under [vars], [env.*.vars], or
                # binding name = "..." patterns. Skip everything else
                # to avoid collecting bucket names / class names.
                in_vars_section = (
                    line == "[vars]"
                    or line.endswith(".vars]")
                )
                continue
            if not in_vars_section or not line or line.startswith("#"):
                continue
            m = WRANGLER_VAR_RE.match(line)
            if m:
                hits.add(m.group(1))
    return hits


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[1])
    ap.add_argument(
        "--json-out",
        type=Path,
        default=None,
        help="Write the JSON report to this path (in addition to stdout summary).",
    )
    ap.add_argument(
        "--dry-run",
        action="store_true",
        help="Compute and print the report but always exit 0.",
    )
    ap.add_argument(
        "--quiet",
        action="store_true",
        help="Suppress human summary; print only JSON to stdout.",
    )
    ap.add_argument(
        "--repo-root",
        type=Path,
        default=REPO_ROOT,
        help="Repository root (default: parent of scripts/).",
    )
    args = ap.parse_args()

    root: Path = args.repo_root.resolve()
    matrix_path = root / MATRIX_FILE_REL

    matrix = parse_matrix(matrix_path)
    code = scan_rust(root) | scan_ts(root) | scan_workflows(root) | scan_wrangler(root)

    # Partition with allowlist applied to `code` side.
    allowlist_skipped = sorted(v for v in code if ALLOWLIST_REGEX.match(v))
    code_filtered = {v for v in code if not ALLOWLIST_REGEX.match(v)}

    in_both = sorted(matrix & code_filtered)
    matrix_only = sorted(matrix - code_filtered)
    code_only = sorted(code_filtered - matrix)

    report = {
        "schema_version": "1",
        "matrix_file": MATRIX_FILE_REL,
        "summary": {
            "matrix_total": len(matrix),
            "code_total": len(code_filtered),
            "in_both": len(in_both),
            "matrix_only": len(matrix_only),
            "code_only": len(code_only),
            "drift": len(code_only),
        },
        "matrix_only": matrix_only,
        "code_only": code_only,
        "in_both": in_both,
        "allowlist_skipped": allowlist_skipped,
    }

    if args.json_out:
        args.json_out.write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )

    if args.quiet:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        s = report["summary"]
        print(
            "validate_secrets_matrix: "
            f"matrix={s['matrix_total']} code={s['code_total']} "
            f"in_both={s['in_both']} matrix_only={s['matrix_only']} "
            f"code_only={s['code_only']}"
        )
        if matrix_only:
            print("WARN matrix_only (forward-looking or stale rows; soft-warn):")
            for v in matrix_only:
                print(f"  - {v}")
        if code_only:
            print("ERROR code_only (real drift; add to matrix or allowlist):")
            for v in code_only:
                print(f"  - {v}")
        if not code_only and not matrix_only:
            print("validate_secrets_matrix: OK (no drift).")

    if args.dry_run:
        return 0
    return 1 if code_only else 0


if __name__ == "__main__":
    sys.exit(main())
