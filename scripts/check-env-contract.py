#!/usr/bin/env python3
"""
check-env-contract.py — Container env-contract drift detector (CAA-360 F8).

Greps every env::var("X"), non_empty_env("X"), and env_or("X", ...) call
across crates/corelink-container/src (the `corelink-server` binary + its
storage sub-crates) and asserts that each name appears in the
worker/src/durable_object.ts container.start({ env: { ... } }) forward-list.

The class of bug this closes: an operator sets a Cloudflare Worker secret
(e.g. ERASURE_SALT_KEY, FABRIC_INTROSPECT_AUTH_KEY), the deploy succeeds,
but the container never sees the value because the DO's container.start()
call does not forward it.  The container then silently falls back to a
weak/absent default with NO error — the exact bug the 2026-06-13 CAA-360
audit (F8) found.

References:
  - docs/security/2026-06-13-CAA-360-followups.md  §CI mechanization (F8)
  - docs/security/2026-06-13-CAA-360-audit-report.md  [MEDIUM] F8
  - worker/src/durable_object.ts  container.start({ env: { ... } })
  - crates/corelink-container/src/storage.rs  env_or / non_empty_env

Style idiom: matches scripts/check_migrations_additive.py (regex-based,
self-contained, no external dependencies beyond stdlib, non-zero exit +
explicit message on failure).

Usage:
    python3 scripts/check-env-contract.py

Exit codes:
    0 — every container-read env var is present in the DO forward-list.
    1 — at least one container-read env var is NOT forwarded — the
        set-but-not-delivered class will hide secret overrides silently.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------
REPO_ROOT = Path(__file__).resolve().parent.parent

# The corelink-server binary (Cargo package name) lives here.
CONTAINER_SRC = REPO_ROOT / "crates" / "corelink-container" / "src"

# The Durable Object TypeScript that boots the container.
DO_TS = REPO_ROOT / "worker" / "src" / "durable_object.ts"

# ---------------------------------------------------------------------------
# Env vars the container reads but that are intentionally excluded from the
# forward-list contract check.  Three categories:
#
#   1. Provided by the container runtime itself (not via the DO env block).
#      PORT is injected as String(CONTAINER_PORT) — a JS literal, not
#      forwarded from Worker env.  RUST_LOG is also a literal in the block
#      (value "info") rather than this.env.RUST_LOG.
#
#   2. Test-only vars (only appear inside #[cfg(test)] / #[tokio::test] etc.;
#      they must never reach production and are excluded from the forward-list
#      by design).
#
#   3. Vars resolved through a const alias rather than a literal string
#      (CORELINK_OCI_TOKEN_KEY is referenced as the OCI_TOKEN_KEY_ENV const;
#      the grep below still catches the *const definition* and maps it).
#      If the script adds a const-resolution pass later this list can shrink.
#
# Add new exclusions here with a comment explaining why.
EXCLUSIONS: set[str] = {
    # Injected as a JS literal (String(CONTAINER_PORT)), not from Worker env.
    "PORT",
    # Injected as a string literal "info" in the env block, not forwarded.
    "RUST_LOG",
    # Test-only: only in #[ignore]-annotated integration tests (r2_s3.rs).
    "R2_TEST_BUCKET",
    # Test-only: proptest configuration knob (routes/audit_export/tests_proptest.rs).
    "PROPTEST_CASES",
}

# ---------------------------------------------------------------------------
# Patterns for extracting env var names from Rust source.
#
# The container uses three call shapes:
#   std::env::var("SOME_VAR")          → direct read
#   crate::storage::non_empty_env("X") → wraps std::env::var, returns Option
#   crate::storage::env_or("X", "def") → absent-or-empty → default
#
# We also catch the const-defined alias pattern:
#   pub const OCI_TOKEN_KEY_ENV: &str = "CORELINK_OCI_TOKEN_KEY";
# …passed as  non_empty_env(oci::OCI_TOKEN_KEY_ENV)
# The const definition itself embeds the literal, which our regex picks up
# from the file where the const is declared (routes/oci.rs).
# ---------------------------------------------------------------------------

# std::env::var("X") / env::var("X")
RUST_STD_ENV_RE = re.compile(
    r"\benv::var\(\s*\"([A-Z][A-Z0-9_]+)\""
)

# non_empty_env("X") — literal string argument
RUST_NON_EMPTY_ENV_LITERAL_RE = re.compile(
    r"\bnon_empty_env\(\s*\"([A-Z][A-Z0-9_]+)\""
)

# env_or("X", "default") — literal string argument
RUST_ENV_OR_LITERAL_RE = re.compile(
    r"\benv_or\(\s*\"([A-Z][A-Z0-9_]+)\""
)

# non_empty_env(some::CONST_NAME) / env_or(CONST_NAME, "default") — const arg.
# Captures the identifier (without the module path prefix) so we can resolve
# it via const_aliases built in the first pass.
RUST_ENV_CONST_ARG_RE = re.compile(
    r"\b(?:non_empty_env|env_or)\(\s*(?:\w+::)*([A-Z][A-Z0-9_]+)\s*(?:[,)])"
)

# pub const NAME: &str = "VALUE";  — captures both NAME and VALUE.
# Used to resolve the const-alias pattern (OCI_TOKEN_KEY_ENV → CORELINK_OCI_TOKEN_KEY).
RUST_CONST_STR_RE = re.compile(
    r'pub\s+const\s+([A-Z][A-Z0-9_]+)\s*:\s*&\'?\s*str\s*=\s*"([A-Z][A-Z0-9_]+)"'
)

# ---------------------------------------------------------------------------
# Pattern for extracting forwarded keys from the durable_object.ts env block.
#
# We look for lines of the form:
#   SOME_VAR: this.env.SOME_VAR ?? "",
#   SOME_VAR: "literal",          ← also forwarded (e.g. RUST_LOG, PORT)
#
# The forward-list starts at  container.start({  and ends at the matching  });
# We extract it as a slice and then scan for the key shape.
# ---------------------------------------------------------------------------

# Keys inside the env: { ... } block: word characters at the start of a line
# followed by a colon.  We scan the whole block and capture only
# UPPER_CASE names to avoid false positives from JS object method names.
DO_ENV_KEY_RE = re.compile(r"^\s+([A-Z][A-Z0-9_]+)\s*:", re.MULTILINE)

# ---------------------------------------------------------------------------
# Skip test files: anything under a `tests/` directory or named `*_test.rs`
# or containing `#[cfg(test)]`.  We use a simple heuristic: exclude paths
# whose last parent component is `tests` — sufficient for this codebase.
# ---------------------------------------------------------------------------
def _is_test_file(path: Path) -> bool:
    return "tests" in path.parts or path.stem.endswith("_test")


def _is_test_block_line(line: str) -> bool:
    """Heuristic: a line inside a #[cfg(test)] or #[tokio::test] block."""
    # We don't do full block parsing; we rely on _is_test_file for test
    # files and on the explicit EXCLUSIONS set for the few test-only vars
    # that live in non-test files (R2_TEST_BUCKET in r2_s3.rs).
    return False  # kept as a hook for future refinement


# ---------------------------------------------------------------------------
# Step 1 — collect env var names from Rust container source.
# ---------------------------------------------------------------------------
def collect_container_env_vars() -> dict[str, list[str]]:
    """
    Returns { env_var_name: [list of "relpath:lineno" occurrences] }
    for every env var name found in non-test Rust source under CONTAINER_SRC.
    """
    if not CONTAINER_SRC.is_dir():
        print(
            f"ERROR: container source directory not found: {CONTAINER_SRC}",
            file=sys.stderr,
        )
        sys.exit(1)

    # First pass: build a map of const aliases across all files.
    # { const_name: resolved_env_var_value }
    const_aliases: dict[str, str] = {}
    for rs_file in sorted(CONTAINER_SRC.rglob("*.rs")):
        if _is_test_file(rs_file):
            continue
        text = rs_file.read_text(encoding="utf-8", errors="replace")
        for m in RUST_CONST_STR_RE.finditer(text):
            const_name, env_value = m.group(1), m.group(2)
            # Accept only if the VALUE looks like a real env var (UPPER_CASE).
            if re.match(r"^[A-Z][A-Z0-9_]+$", env_value):
                const_aliases[const_name] = env_value

    # Second pass: collect literal and const-alias env var reads.
    found: dict[str, list[str]] = {}

    def _record(name: str, label: str) -> None:
        if name not in EXCLUSIONS:
            found.setdefault(name, []).append(label)

    # Literal-string patterns — match the env var name directly.
    literal_patterns = [
        RUST_STD_ENV_RE,
        RUST_NON_EMPTY_ENV_LITERAL_RE,
        RUST_ENV_OR_LITERAL_RE,
    ]

    for rs_file in sorted(CONTAINER_SRC.rglob("*.rs")):
        if _is_test_file(rs_file):
            continue
        text = rs_file.read_text(encoding="utf-8", errors="replace")
        rel = rs_file.relative_to(REPO_ROOT)
        for lineno, raw_line in enumerate(text.splitlines(), start=1):
            # Skip full-line comments.
            stripped = raw_line.lstrip()
            if stripped.startswith("//"):
                continue
            # Strip inline comments before matching.
            scan_line = re.sub(r"//[^\n]*$", "", raw_line)
            # Match literal string arguments.
            for pat in literal_patterns:
                for m in pat.finditer(scan_line):
                    _record(m.group(1), f"{rel}:{lineno}")
            # Match const-alias arguments (e.g. non_empty_env(oci::OCI_TOKEN_KEY_ENV)).
            for m in RUST_ENV_CONST_ARG_RE.finditer(scan_line):
                const_name = m.group(1)
                resolved = const_aliases.get(const_name)
                if resolved and resolved not in EXCLUSIONS:
                    _record(resolved, f"{rel}:{lineno} (via const {const_name})")

    return found


# ---------------------------------------------------------------------------
# Step 2 — collect forwarded keys from durable_object.ts.
# ---------------------------------------------------------------------------
def collect_do_forwarded_keys() -> set[str]:
    """
    Parse the  container.start({ env: { ... } })  call in durable_object.ts
    and return the set of env var keys forwarded to the container.
    """
    if not DO_TS.is_file():
        print(f"ERROR: durable_object.ts not found: {DO_TS}", file=sys.stderr)
        sys.exit(1)

    text = DO_TS.read_text(encoding="utf-8")

    # Locate the container.start({ block.  We scan for the `env: {` opening
    # and track brace depth to find the closing `}` of the env object.
    env_block_start = text.find("container.start({")
    if env_block_start == -1:
        print(
            "ERROR: could not find 'container.start({' in durable_object.ts",
            file=sys.stderr,
        )
        sys.exit(1)

    # Find `env: {` inside the container.start block.
    env_key_start = text.find("env: {", env_block_start)
    if env_key_start == -1:
        print(
            "ERROR: could not find 'env: {' inside container.start block",
            file=sys.stderr,
        )
        sys.exit(1)

    # Walk forward from the `{` after `env: ` to find the matching `}`.
    brace_open = text.index("{", env_key_start + len("env: "))
    depth = 0
    env_block_end = brace_open
    for i, ch in enumerate(text[brace_open:], start=brace_open):
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                env_block_end = i
                break

    env_block = text[brace_open : env_block_end + 1]

    # Extract all UPPER_CASE keys from the env block.
    forwarded: set[str] = set()
    for m in DO_ENV_KEY_RE.finditer(env_block):
        key = m.group(1)
        forwarded.add(key)

    return forwarded


# ---------------------------------------------------------------------------
# Step 3 — diff and report.
# ---------------------------------------------------------------------------
def main() -> int:
    container_vars = collect_container_env_vars()
    forwarded_keys = collect_do_forwarded_keys()

    missing: dict[str, list[str]] = {
        name: occurrences
        for name, occurrences in container_vars.items()
        if name not in forwarded_keys
    }

    total_scanned = len(container_vars)
    total_forwarded_checked = len([n for n in container_vars if n in forwarded_keys])

    if not missing:
        print(
            f"OK: {total_scanned} container-read env var(s) scanned; "
            f"all {total_forwarded_checked} present in the DO forward-list."
        )
        return 0

    # Build the report — fail CLOSED and LOUD (CAA-360 SOTA bar).
    print(
        "FAIL: container-env-contract drift detected — "
        f"{len(missing)} var(s) read by the container but NOT in the "
        "durable_object.ts container.start({env: {...}}) forward-list.\n"
    )
    print(
        "  Impact: setting these as Cloudflare Worker secrets will silently\n"
        "  have NO effect — the container process never sees the value and\n"
        "  falls back to its default (often a weak/absent fallback).\n"
        "  This is the ERASURE_SALT_KEY / FABRIC class of bug (CAA-360 F8).\n"
    )
    print("  Missing from forward-list:")
    for name in sorted(missing):
        occurrences = missing[name]
        # Show the first 3 call-sites to keep output readable.
        sites = ", ".join(occurrences[:3])
        if len(occurrences) > 3:
            sites += f", … (+{len(occurrences) - 3} more)"
        print(f"    {name}  (read at: {sites})")

    print(
        "\n  Fix: add each missing key to the env: { ... } block inside\n"
        "  container.start() in worker/src/durable_object.ts, e.g.:\n"
        "    SOME_VAR: this.env.SOME_VAR ?? \"\","
    )
    print(
        "\n  Gate: wire this script into CI so any new env::var(\"X\") call\n"
        "  that is not forwarded fails the build immediately."
    )

    return 1


if __name__ == "__main__":
    sys.exit(main())
