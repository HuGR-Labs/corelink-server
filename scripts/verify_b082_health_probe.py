#!/usr/bin/env python3
"""Decision-bearing verifier for the B-082 authenticated health probe.

The anonymous redaction and authenticated preservation arms are intentionally
checked separately.  The mutation self-test prevents a future verifier from
passing after the dedicated-auth gate or anonymous ``storage`` deletion is
removed.
"""

from __future__ import annotations

import argparse
import subprocess
from pathlib import Path
from typing import Mapping


ROOT = Path(__file__).resolve().parents[1]
INDEX = ROOT / "worker/src/index_public_routes.ts"
ROUTES = ROOT / "worker/src/route_match.ts"
TEST = "tests/index.test.ts"
DOCS = (
    ROOT / "docs/operator/storage-backing-alert-2026-05-30.md",
    ROOT / "specs/_runbooks/rb-storage-fallback.md",
)

# These artifacts are retired and must not remain in operator-facing docs. Keep
# the exact body marker here so the mutation test catches a copy/paste revival;
# the docs themselves must describe the semantic probe instead.
OBSOLETE_DOC_MARKERS = (
    "4467865",
    "https://corelink-api.humangr.com/_health/container",
    '"storage":"r2"',
)
AUTH_DOC_MARKERS = (
    "CORELINK_ADMIN_AUTH_KEY",
    "X-Corelink-Internal-Auth:",
    "/_health/container/authenticated",
)


def secure_shape(source: str) -> bool:
    """Return whether both B-082 response arms are visibly present."""
    required = (
        'path === "/_health/container/authenticated"',
        'routeKind: "health_container_authed"',
        "requireDedicatedAdminAuth(request, env, requestId)",
        'delete raw["storage"]',
        'const includeStorage = route.routeKind === "health_container_authed"',
    )
    if any(marker not in source for marker in required):
        return False

    # The authenticated return must precede the anonymous redaction arm. This
    # keeps a future refactor from accidentally deleting storage for operators.
    authenticated_return = source.find("if (includeStorage)")
    redaction = source.find('delete raw["storage"]')
    return authenticated_return >= 0 and redaction > authenticated_return


def mutation_self_test(source: str) -> None:
    """Prove the guard goes red for the two security-regressing mutations."""
    if secure_shape(source.replace('delete raw["storage"]', "void raw[\"storage\"]", 1)):
        raise SystemExit("B082 verifier accepted anonymous storage-leak mutation")
    if secure_shape(source.replace("requireDedicatedAdminAuth(request, env, requestId)", "null", 1)):
        raise SystemExit("B082 verifier accepted authenticated-auth bypass mutation")


def docs_secure_shape(documents: Mapping[Path, str]) -> bool:
    """Require identifier-free history and an intact authenticated probe."""
    for path in DOCS:
        text = documents.get(path)
        if text is None:
            return False
        if any(marker in text for marker in OBSOLETE_DOC_MARKERS):
            return False
        if any(marker not in text for marker in AUTH_DOC_MARKERS):
            return False
    return True


def docs_mutation_self_test(documents: Mapping[Path, str]) -> None:
    """Prove reintroducing each retired artifact makes the verifier fail."""
    for marker in OBSOLETE_DOC_MARKERS:
        mutated = dict(documents)
        path = DOCS[0]
        mutated[path] = mutated[path] + "\n" + marker + "\n"
        if docs_secure_shape(mutated):
            raise SystemExit(f"B082 verifier accepted obsolete docs marker: {marker}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--run-tests", action="store_true", help="run the optional focal Vitest test")
    args = parser.parse_args()

    source = INDEX.read_text(encoding="utf-8") + "\n" + ROUTES.read_text(encoding="utf-8")
    if not secure_shape(source):
        raise SystemExit("B082 verifier: health probe security shape is incomplete")
    mutation_self_test(source)
    documents = {path: path.read_text(encoding="utf-8") for path in DOCS}
    if not docs_secure_shape(documents):
        raise SystemExit("B082 verifier: operator docs contain a retired artifact or lost auth probe")
    docs_mutation_self_test(documents)

    if args.run_tests:
        subprocess.run(
            ["pnpm", "--dir", "worker", "test:file", TEST, "--run"],
            cwd=ROOT,
            check=True,
        )
    print("B082 verifier: authenticated admin-only storage signal + anonymous redaction PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
