#!/usr/bin/env python3
"""Closed census for the retired admin-operation HTTP surface (B-119).

The dual-approval policy crates remain useful as internal, unbound policy
logic.  Until durable persistence and a safe Worker binding exist, no live or
published UI, mock, SDK example, documentation, or crate may claim the old
HTTP surface.  This verifier intentionally scans those surfaces together so a
green API comparator cannot hide a frontend or example-only phantom.

Historical changelogs and audit notes are not product surfaces and are not
roots of this census.  The self-test mutates an isolated temporary tree and
proves both the positive detector and the clean verdict, without touching the
checkout.
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
ROOTS = (
    REPO / "apps" / "admin-ui",
    REPO / "apps" / "docs",
    REPO / "crates",
    REPO / "docs",
    REPO / "examples",
    REPO / "marketing",
    REPO / "openapi",
    REPO / "tools",
)
SKIP_DIRS = {".git", "node_modules", "target", ".next", "dist", "build", "changelog.d", "specs"}
SKIP_NAMES = {"CHANGELOG.md"}
TEXT_SUFFIXES = {
    ".go",
    ".html",
    ".json",
    ".md",
    ".mdx",
    ".py",
    ".rs",
    ".ts",
    ".tsx",
    ".yaml",
    ".yml",
}

# Matches both the API spelling and UI links such as /en/admin/ops/id.  The
# boundary excludes harmless names like /admin/ops-monitor while retaining
# query, fragment, quote, and punctuation boundaries in source text.
PHANTOM = re.compile(
    r"(?<![A-Za-z0-9_-])/(?:v1/)?admin/ops"
    r"(?=$|[/\\?`'\"#),;\s])"
)


def files_under(root: Path):
    if root.is_file():
        yield root
        return
    if not root.exists():
        return
    for path in root.rglob("*"):
        if not path.is_file() or path.name in SKIP_NAMES:
            continue
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        if path.suffix.lower() in TEXT_SUFFIXES:
            yield path


def census(roots: tuple[Path, ...]) -> list[tuple[Path, int, str]]:
    hits: list[tuple[Path, int, str]] = []
    for root in roots:
        for path in files_under(root):
            try:
                text = path.read_text(encoding="utf-8")
            except UnicodeDecodeError:
                continue
            for line_no, line in enumerate(text.splitlines(), 1):
                if PHANTOM.search(line):
                    hits.append((path, line_no, line.strip()))
    return hits


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="b119-census-") as raw:
        root = Path(raw)
        clean = root / "clean.ts"
        clean.write_text("const route = '/admin/ops-monitor';\n", encoding="utf-8")
        if census((root,)):
            print("self-test failed: clean tree was rejected", file=sys.stderr)
            return 1
        probes = (
            root / "ui" / "page.tsx",
            root / "sdk" / "example.rs",
            root / "mock" / "fixture.ts",
            root / "docs" / "readme.md",
            root / "crate" / "lib.rs",
        )
        for probe in probes:
            probe.parent.mkdir(parents=True, exist_ok=True)
            probe.write_text("fetch('/v1/admin/ops');\n", encoding="utf-8")
        legacy = root / "changelog.d" / "historical.md"
        legacy.parent.mkdir(parents=True, exist_ok=True)
        legacy.write_text("historical: /v1/admin/ops\n", encoding="utf-8")
        hits = census((root,))
        if {path for path, _, _ in hits} != set(probes):
            print("self-test failed: one or more product-surface mutations escaped", file=sys.stderr)
            return 1
        print("B-119 census self-test: positive and negative mutations passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()

    hits = census(ROOTS)
    if hits:
        print("B-119 phantom admin-operation claims found:", file=sys.stderr)
        for path, line_no, line in hits:
            print(f"  {path.relative_to(REPO)}:{line_no}: {line}", file=sys.stderr)
        return 1
    print("B-119 census: 0 phantom admin-operation claims across live/published UI, mocks, SDKs, docs, OpenAPI, and crates")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
