#!/usr/bin/env python3
"""Comment-safe semantic guard for the B-064 audit-drain caller."""

from __future__ import annotations

from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
CRON = ROOT / "apps/signup-worker/src/webhooks/audit_drain_cron.ts"


class VerificationError(RuntimeError):
    pass


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        raise VerificationError(f"B-064 required artifact unreadable: {path}") from error


def strip_ts_comments(source: str) -> str:
    """Blank TS comments while preserving quoted and template string contents."""
    out: list[str] = []
    index = 0
    block = False
    quote = ""
    while index < len(source):
        if block:
            if source.startswith("*/", index):
                block = False
                out.extend("  ")
                index += 2
            else:
                out.append("\n" if source[index] == "\n" else " ")
                index += 1
            continue
        if quote:
            out.append(source[index])
            if source[index] == "\\" and index + 1 < len(source):
                index += 1
                out.append(source[index])
            elif source[index] == quote:
                quote = ""
            index += 1
            continue
        if source.startswith("//", index):
            out.extend("  ")
            index += 2
            while index < len(source) and source[index] != "\n":
                out.append(" ")
                index += 1
            continue
        if source.startswith("/*", index):
            block = True
            out.extend("  ")
            index += 2
            continue
        if source[index] in ('"', "'", "`"):
            quote = source[index]
        out.append(source[index])
        index += 1
    if block:
        raise VerificationError("B-064 unterminated TS block comment")
    if quote:
        raise VerificationError("B-064 unterminated TS string")
    return "".join(out)


def strip_ts_noncode(source: str) -> str:
    """Blank comments and quoted literals while retaining executable syntax."""
    out: list[str] = []
    index = 0
    block = False
    quote = ""
    while index < len(source):
        if block:
            if source.startswith("*/", index):
                block = False
                out.extend("  ")
                index += 2
            else:
                out.append("\n" if source[index] == "\n" else " ")
                index += 1
            continue
        if quote:
            out.append("\n" if source[index] == "\n" else " ")
            if source[index] == "\\" and index + 1 < len(source):
                index += 1
                out.append(" " if source[index] != "\n" else "\n")
            elif source[index] == quote:
                quote = ""
            index += 1
            continue
        if source.startswith("//", index):
            out.extend("  ")
            index += 2
            while index < len(source) and source[index] != "\n":
                out.append(" ")
                index += 1
            continue
        if source.startswith("/*", index):
            block = True
            out.extend("  ")
            index += 2
            continue
        if source[index] in ('"', "'", "`"):
            quote = source[index]
            out.append(" ")
            index += 1
            continue
        out.append(source[index])
        index += 1
    if block or quote:
        raise VerificationError("B-064 unterminated TS comment or literal")
    return "".join(out)


def _matching_brace(source: str, opening: int) -> int:
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return index
    return -1


def _reachable_failure_predicate(source: str) -> None:
    """Require the failure predicate in the live drain sweep, not bait/dead code."""
    code = strip_ts_noncode(source)
    marker = "if (!body.ok || body.partitions_failed > 0)"
    position = code.find(marker)
    if position < 0:
        raise VerificationError("B-064 boolean-contract predicate missing from executable code")
    function = code.rfind("runAuditDrainSweep(", 0, position)
    if function < 0:
        raise VerificationError("B-064 predicate is not in runAuditDrainSweep")
    opening = code.find("{", function, position)
    closing = _matching_brace(code, opening) if opening >= 0 else -1
    if opening < 0 or closing < position:
        raise VerificationError("B-064 predicate is outside the live drain function")
    for dead in re.finditer(r"\bif\s*\(\s*false\s*\)\s*\{", code[:position]):
        dead_end = _matching_brace(code, code.find("{", dead.start(), dead.end()))
        if dead_end >= position:
            raise VerificationError("B-064 predicate is inside unreachable TypeScript code")
    prefix = code[max(function, position - 600) : position]
    if "const body = parsed.value" not in prefix or "partitionsFailed += body.partitions_failed" not in prefix:
        raise VerificationError("B-064 predicate lacks live parsed-body structural context")


def verify_text(source: str) -> None:
    code = strip_ts_comments(source)
    required = (
        "resolveDedicatedEraseAuthKey",
        "parseAuditDrainResponse",
        "if (!madeDrainProgress(body))",
        "CORELINK_ERASE_AUTH_KEY",
        "incomplete = body.incomplete",
        "if (!incomplete)",
    )
    missing = [marker for marker in required if marker not in code]
    if missing:
        raise VerificationError(f"B-064 semantic contract missing: {', '.join(missing)}")
    _reachable_failure_predicate(source)


def mutation_self_test(source: str) -> None:
    old = "if (!body.ok || body.partitions_failed > 0)"
    weakened = source.replace(old, "if (body.partitions_failed > 0)", 1)
    if weakened == source:
        raise VerificationError("B-064 boolean-contract mutation fixture missing")
    try:
        verify_text(weakened)
    except VerificationError:
        pass
    else:
        raise VerificationError("B-064 boolean-contract mutation survived")

    string_mutant = source.replace(old, 'const bait = "if (!body.ok || body.partitions_failed > 0)";', 1)
    string_mutant += '\nconst B064_BAIT = "if (!body.ok || body.partitions_failed > 0)";\n'
    try:
        verify_text(string_mutant)
    except VerificationError:
        pass
    else:
        raise VerificationError("B-064 string-only predicate mutation survived")

    dead_mutant = source.replace(old, "if (false) {\n" + old + "\n}", 1)
    try:
        verify_text(dead_mutant)
    except VerificationError:
        pass
    else:
        raise VerificationError("B-064 dead-code predicate mutation survived")

    bait = source.replace(old, f"// {old}", 1) + f"\n// {old}\n"
    try:
        verify_text(bait)
    except VerificationError as error:
        if "boolean-contract" not in str(error) and "semantic contract" not in str(error):
            raise VerificationError(f"B-064 comment mutation failed for wrong reason: {error}") from error
    else:
        raise VerificationError("B-064 comment-only predicate mutation survived")


def main() -> int:
    source = read(CRON)
    verify_text(source)
    mutation_self_test(source)
    print("B-064 comment-safe semantic/mutation guard PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
