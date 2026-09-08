#!/usr/bin/env python3
"""Fail-closed structural contract for the Worker tenant-suspend wiring (B-260)."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
AUTH = "worker/src/index_auth.ts"
SUSPEND = "worker/src/lib/tenant_suspend_gate.ts"
TESTS = "worker/tests/tenant_suspend_gate.test.ts"
IMPORT = 'import { isTenantSuspended } from "./lib/tenant_suspend_gate.js";'


class VerificationError(RuntimeError):
    """The production suspend gate is not reachable from Worker auth."""


def _source(root: Path, path: str, overrides: dict[str, str]) -> str:
    if path in overrides:
        return overrides[path]
    try:
        return (root / path).read_text(encoding="utf-8")
    except OSError as exc:
        raise VerificationError(f"missing B-260 source: {path}") from exc


def _active_imports(text: str) -> list[str]:
    masked_lines = _mask_non_code(text).splitlines()
    return [
        line.strip()
        for line, masked in zip(text.splitlines(), masked_lines)
        if "import" in masked
        and re.fullmatch(
            r'import \{ isTenantSuspended \} from "\./lib/tenant_suspend_gate\.js";',
            line.strip(),
        )
    ]


def _mask_non_code(text: str) -> str:
    """Mask comments and literals while preserving source positions/newlines."""
    out = list(text)
    i = 0
    while i < len(text):
        if text.startswith("//", i):
            end = text.find("\n", i + 2)
            end = len(text) if end < 0 else end
            for pos in range(i, end):
                out[pos] = " "
            i = end
            continue
        if text.startswith("/*", i):
            end = text.find("*/", i + 2)
            if end < 0:
                end = len(text) - 2
            for pos in range(i, min(len(text), end + 2)):
                if text[pos] != "\n":
                    out[pos] = " "
            i = min(len(text), end + 2)
            continue
        if text[i] in {'"', "'", "`"}:
            quote = text[i]
            j = i + 1
            while j < len(text):
                if text[j] == "\\":
                    j += 2
                elif text[j] == quote:
                    j += 1
                    break
                else:
                    j += 1
            for pos in range(i, min(j, len(text))):
                if text[pos] != "\n":
                    out[pos] = " "
            i = j
            continue
        i += 1
    return "".join(out)


def _matching_brace(masked: str, opening: int) -> int:
    depth = 0
    for pos in range(opening, len(masked)):
        if masked[pos] == "{":
            depth += 1
        elif masked[pos] == "}":
            depth -= 1
            if depth == 0:
                return pos
    return -1


def _inside_dead_if(masked: str, position: int) -> bool:
    for dead in re.finditer(r"\bif\s*\(\s*false\s*\)\s*\{", masked):
        closing = _matching_brace(masked, masked.find("{", dead.start(), dead.end()))
        if dead.end() <= position < closing:
            return True
    return False


def _active_suspend_tests(text: str) -> dict[str, str]:
    """Extract target ``it`` bodies from executable (non-comment) code."""
    masked = _mask_non_code(text)
    if re.search(r"\bdescribe\.skip\s*\(", masked):
        raise VerificationError("B-260 suspend tests are disabled")
    found: dict[str, str] = {}
    for match in re.finditer(r"(?<![\w.])it\s*\(", masked):
        original = text[match.start() :]
        name_match = re.match(r'it\s*\(\s*(["\'])(.*?)\1\s*,', original, re.DOTALL)
        if not name_match:
            continue
        name = name_match.group(2)
        arrow = masked.find("=>", match.end())
        opening = masked.find("{", arrow if arrow >= 0 else match.end())
        if arrow < 0 or opening < 0:
            continue
        closing = _matching_brace(masked, opening)
        if closing < 0:
            raise VerificationError(f"B-260 test body is unbalanced: {name}")
        if _inside_dead_if(masked, match.start()):
            raise VerificationError(f"B-260 test is under a dead if(false): {name}")
        found[name] = text[opening + 1 : closing]
    return found


def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> None:
    overrides = overrides or {}
    auth = _source(root, AUTH, overrides)
    suspend = _source(root, SUSPEND, overrides)
    tests = _source(root, TESTS, overrides)

    if len(_active_imports(auth)) != 1:
        raise VerificationError("index_auth.ts must carry exactly one active isTenantSuspended import")
    if "export async function isTenantSuspended" not in suspend:
        raise VerificationError("tenant_suspend_gate.ts no longer exports isTenantSuspended")
    masked_auth = _mask_non_code(auth)
    calls = list(re.finditer(r"(?m)^\s*await\s+isTenantSuspended\s*\(", masked_auth))
    if len(calls) != 1 or _inside_dead_if(masked_auth, calls[0].start()):
        raise VerificationError("index_auth.ts must call isTenantSuspended exactly once")
    bodies = _active_suspend_tests(tests)
    expected = {
        "403s a valid PAT whose tenant is SUSPENDED": "expect(resp.status).toBe(403)",
        "403s a valid PAT whose tenant is ERASED": "expect(resp.status).toBe(403)",
        "does NOT 403 an ACTIVE tenant (no offboarding row) — gate passes to the DO": "expect(resp.status).not.toBe(403)",
    }
    for name, assertion in expected.items():
        body = bodies.get(name)
        if body is None:
            raise VerificationError(f"B-260 executable behavioral test missing: {name}")
        if assertion not in _mask_non_code(body):
            raise VerificationError(f"B-260 executable behavioral assertion missing: {assertion}")


if __name__ == "__main__":
    try:
        verify()
    except VerificationError as exc:
        raise SystemExit(f"B-260 BROKEN: {exc}")
    print("B-260 worker suspend wiring: PASS")
