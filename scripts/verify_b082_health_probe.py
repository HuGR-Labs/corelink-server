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


ROOT = Path(__file__).resolve().parents[1]
INDEX = ROOT / "worker/src/index.ts"
TEST = "tests/index.test.ts"


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


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-tests", action="store_true", help="skip the focal Vitest run")
    args = parser.parse_args()

    source = INDEX.read_text(encoding="utf-8")
    if not secure_shape(source):
        raise SystemExit("B082 verifier: health probe security shape is incomplete")
    mutation_self_test(source)

    if not args.skip_tests:
        subprocess.run(
            ["pnpm", "--dir", "worker", "test:file", TEST, "--run"],
            cwd=ROOT,
            check=True,
        )
    print("B082 verifier: authenticated admin-only storage signal + anonymous redaction PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
