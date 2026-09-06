#!/usr/bin/env python3
"""Executable contract and mutation gate for B-053 failover safety.

This is intentionally source-level: the bundle workflow runs the focal Rust
test separately, while this guard proves that the test cannot be detached from
the any-5xx event bucket, router fallback, hysteresis, authenticated internal
heartbeat, or production route wiring. Every named mutation must turn the guard
red.
"""

from __future__ import annotations

from pathlib import Path
import re


ROOT = Path(__file__).resolve().parent.parent
FAILOVER = ROOT / "crates/corelink-container/src/routes/failover.rs"
FAILOVER_TESTS = ROOT / "crates/corelink-container/src/routes/failover_tests.rs"
BUILD = ROOT / "crates/corelink-container/src/routes/build.rs"
MAIN = ROOT / "crates/corelink-container/src/main.rs"
WORKFLOW = ROOT / ".github/workflows/backlog-verify.yml"


class B053ContractError(ValueError):
    pass


def _read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise B053ContractError(f"required B-053 input unreadable: {path}") from exc


def strip_rust_comments(source: str) -> str:
    """Blank Rust comments while preserving strings, chars, and line layout."""
    out: list[str] = []
    index = 0
    block_depth = 0
    quote = ""
    while index < len(source):
        if block_depth:
            if source.startswith("/*", index):
                block_depth += 1
                out.extend("  ")
                index += 2
            elif source.startswith("*/", index):
                block_depth -= 1
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
        # Rust raw strings may contain comment delimiters as data.
        if source[index] == "r":
            cursor = index + 1
            while cursor < len(source) and source[cursor] == "#":
                cursor += 1
            if cursor < len(source) and source[cursor] == '"':
                hashes = source[index + 1 : cursor]
                terminator = '"' + hashes
                end = source.find(terminator, cursor + 1)
                if end < 0:
                    raise B053ContractError("unterminated Rust raw string literal")
                out.append(source[index : end + len(terminator)])
                index = end + len(terminator)
                continue
        if source.startswith("//", index):
            out.extend("  ")
            index += 2
            while index < len(source) and source[index] != "\n":
                out.append(" ")
                index += 1
            continue
        if source.startswith("/*", index):
            block_depth = 1
            out.extend("  ")
            index += 2
            continue
        if source[index] == '"':
            quote = source[index]
        elif source[index] == "'":
            # A lifetime such as `'static` is syntax, not a character literal.
            if (index + 2 < len(source) and source[index + 2] == "'") or (
                index + 3 < len(source)
                and source[index + 1] == "\\"
                and source[index + 3] == "'"
            ):
                quote = source[index]
        out.append(source[index])
        index += 1
    if block_depth:
        raise B053ContractError("unterminated Rust block comment")
    if quote:
        raise B053ContractError("unterminated Rust string or character literal")
    return "".join(out)


def strip_rust_noncode(source: str) -> str:
    """Blank Rust comments and literals, retaining only executable structure."""
    out: list[str] = []
    index = 0
    block_depth = 0
    quote = ""
    while index < len(source):
        if block_depth:
            if source.startswith("/*", index):
                block_depth += 1
                out.extend("  ")
                index += 2
            elif source.startswith("*/", index):
                block_depth -= 1
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
        if source[index] == "r":
            cursor = index + 1
            while cursor < len(source) and source[cursor] == "#":
                cursor += 1
            if cursor < len(source) and source[cursor] == '"':
                hashes = source[index + 1 : cursor]
                terminator = '"' + hashes
                end = source.find(terminator, cursor + 1)
                if end < 0:
                    raise B053ContractError("unterminated Rust raw string literal")
                literal = source[index : end + len(terminator)]
                out.extend("\n" if char == "\n" else " " for char in literal)
                index = end + len(terminator)
                continue
        if source.startswith("//", index):
            out.extend("  ")
            index += 2
            while index < len(source) and source[index] != "\n":
                out.append(" ")
                index += 1
            continue
        if source.startswith("/*", index):
            block_depth = 1
            out.extend("  ")
            index += 2
            continue
        if source[index] == '"':
            quote = source[index]
            out.append(" ")
            index += 1
            continue
        if source[index] == "'" and (
            (index + 2 < len(source) and source[index + 2] == "'")
            or (index + 3 < len(source) and source[index + 1] == "\\" and source[index + 3] == "'")
        ):
            quote = source[index]
            out.append(" ")
            index += 1
            continue
        out.append(source[index])
        index += 1
    if block_depth or quote:
        raise B053ContractError("unterminated Rust comment or literal")
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


def _reachable_event_bucket(source: str) -> None:
    """Require the event bucket in the live ``HealthProbe::probe`` body."""
    code = strip_rust_noncode(source)
    marker = "filter_map(|sample| sample.error_event)"
    position = code.find(marker)
    if position < 0:
        raise B053ContractError("B-053 any-5xx event bucket missing from executable code")
    function = code.rfind("fn probe(&self, region: Region, timestamp_ms: u64)", 0, position)
    impl = code.rfind("impl HealthProbe for RollingMetricsHealthProbe", 0, position)
    if function < 0 or impl < 0 or function < impl:
        raise B053ContractError("B-053 event bucket is not in the production HealthProbe::probe")
    opening = code.find("{", function, position)
    closing = _matching_brace(code, opening) if opening >= 0 else -1
    if opening < 0 or closing < position:
        raise B053ContractError("B-053 event bucket is outside the live probe body")
    if re.search(r"\bif\s+(?:false|cfg!\s*\(\s*false\s*\))\s*\{", code[:position]):
        for dead in re.finditer(r"\bif\s+(?:false|cfg!\s*\(\s*false\s*\))\s*\{", code[:position]):
            dead_end = _matching_brace(code, code.find("{", dead.start(), dead.end()))
            if dead_end >= position:
                raise B053ContractError("B-053 event bucket is inside unreachable Rust code")
    prefix = code[max(function, position - 260) : position]
    suffix = code[position : min(len(code), position + 180)]
    if "let newest_error_event = guard" not in prefix or ".iter()" not in prefix or ".max()" not in suffix:
        raise B053ContractError("B-053 event bucket lacks the live guard/max structural context")


def strip_hash_comments(source: str) -> str:
    """Blank YAML/shell comments without treating a quoted ``#`` as syntax."""
    out: list[str] = []
    for line in source.splitlines(keepends=True):
        quote = ""
        escaped = False
        chars: list[str] = []
        for position, char in enumerate(line):
            if char == "\n":
                chars.append(char)
                continue
            if escaped:
                chars.append(char)
                escaped = False
                continue
            if char == "\\" and quote == '"':
                chars.append(char)
                escaped = True
                continue
            if char in ('"', "'"):
                if not quote:
                    quote = char
                elif quote == char:
                    quote = ""
                chars.append(char)
                continue
            if char == "#" and not quote:
                chars.extend(" " for _ in line[position:].rstrip("\n"))
                if line.endswith("\n"):
                    chars.append("\n")
                break
            chars.append(char)
        out.append("".join(chars))
    return "".join(out)


def _require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise B053ContractError(f"B-053 {label} missing: {needle}")


def verify_texts(files: dict[str, str]) -> None:
    failover = strip_rust_comments(files["failover"])
    build = strip_rust_comments(files["build"])
    main = strip_rust_comments(files["main"])
    workflow = strip_hash_comments(files["workflow"])

    # Distinct 5xx events are consumed once; stale samples cannot repeatedly
    # pump the same degraded observation into hysteresis.
    _require(failover, "error_event: Option<u64>", "sample event identity")
    _require(failover, "last_reported_error_event: AtomicU64", "event reset state")
    _reachable_event_bucket(files["failover"])
    _require(failover, "newest_error_event > last_reported_error_event", "event consumption")
    _require(
        failover,
        "one_transient_5xx_then_clean_probe_is_not_counted_repeatedly",
        "integrated transient/clean behavior test",
    )
    _require(
        failover,
        "integrated_transient_error_then_clean_requests_do_not_trip_hysteresis",
        "middleware transient/clean sequence test",
    )

    # Router errors and heartbeat staleness both flow through the existing
    # hysteresis gate; a clean data-plane request is not a heartbeat.
    _require(failover, "heartbeat_stale || snap.health.requires_failover()", "heartbeat/router OR")
    _require(failover, ".hysteresis\n        .observe(", "hysteresis wiring")
    _require(failover, ">= FAILOVER_TRIP_PROBES", "hysteresis threshold")
    _require(failover, "stale_external_heartbeat_blocks_without_error_traffic", "heartbeat behavior test")
    _require(failover, "anonymous_caller_cannot_refresh_heartbeat", "heartbeat authenticity test")
    _require(failover, "internal_heartbeat_router", "authenticated heartbeat route")
    _require(failover, "apply_authenticated_heartbeat", "heartbeat auth boundary")
    _require(failover, "if !internal_auth_ok(expected, headers)", "fail-closed heartbeat auth")
    _require(failover, "INTERNAL_AUTH_HEADER", "heartbeat auth header")
    _require(failover, "post(internal_heartbeat_handler)", "internal heartbeat method")
    _require(failover, "\"/_internal/failover/heartbeat\"", "internal heartbeat path")

    # The external signal is the authenticated internal route, not the public
    # readiness endpoint or run_and_record's data-plane outcome path.
    _require(main, "routes::failover::internal_heartbeat_router()", "internal heartbeat route mount")
    if "record_external_heartbeat" in main:
        raise B053ContractError("public health handler must not refresh failover heartbeat")
    _require(build, "let failover_state = failover::FailoverLayerState::from_env();", "state construction")
    _require(build, "failover::failover_guard", "guard route wiring")
    _require(build, "from_fn_with_state", "stateful middleware wiring")
    _require(workflow, "scripts/verify_b053_failover.py", "workflow verifier path")
    _require(workflow, "cargo test -p corelink-server --lib routes::failover::tests", "workflow focal Rust test")


def mutation_self_test(files: dict[str, str]) -> None:
    mutations = {
        "any-5xx": ("filter_map(|sample| sample.error_event)", "filter_map(|_| None)"),
        "router": (
            "heartbeat_stale || snap.health.requires_failover()",
            "heartbeat_stale && snap.health.requires_failover()",
        ),
        "hysteresis": (">= FAILOVER_TRIP_PROBES", "> FAILOVER_TRIP_PROBES"),
        "heartbeat": (
            "if !internal_auth_ok(expected, headers)",
            "if internal_auth_ok(expected, headers)",
        ),
    }
    for label, (old, new) in mutations.items():
        mutated = dict(files)
        target = "failover"
        if old not in mutated[target]:
            raise B053ContractError(f"B-053 mutation fixture missing: {label}")
        mutated[target] = mutated[target].replace(old, new, 1)
        try:
            verify_texts(mutated)
        except B053ContractError:
            continue
        raise B053ContractError(f"B-053 mutation survived: {label}")

    # A marker moved into a Rust comment must not satisfy the source contract,
    # even when the bait text remains in the fixture.
    marker = "error_event: Option<u64>"
    comment_mutant = dict(files)
    if marker not in comment_mutant["failover"]:
        raise B053ContractError("B-053 comment mutation fixture missing")
    comment_mutant["failover"] = comment_mutant["failover"].replace(
        marker, f"// {marker}", 1
    ) + f"\n// {marker}\n"
    try:
        verify_texts(comment_mutant)
    except B053ContractError as error:
        if "sample event identity" not in str(error):
            raise B053ContractError(
                f"B-053 comment-only mutation failed for the wrong contract: {error}"
            ) from error
    else:
        raise B053ContractError("B-053 comment-only marker mutation survived")

    # Neither a string literal nor an unreachable block is executable proof.
    old = "filter_map(|sample| sample.error_event)"
    string_mutant = files["failover"].replace(old, '"filter_map(|sample| sample.error_event)"', 1)
    string_mutant += '\nconst _BAIT: &str = "filter_map(|sample| sample.error_event)";\n'
    try:
        _reachable_event_bucket(string_mutant)
    except B053ContractError:
        pass
    else:
        raise B053ContractError("B-053 string-only event-bucket mutation survived")
    dead_mutant = files["failover"].replace(old, "if false {\n" + old + "\n}", 1)
    try:
        _reachable_event_bucket(dead_mutant)
    except B053ContractError:
        pass
    else:
        raise B053ContractError("B-053 dead-code event-bucket mutation survived")


def main() -> int:
    files = {
        # The facade owns production wiring; the split test module owns the
        # executable sequence/authentication regressions.  Verify the composed
        # source so a stale facade path cannot make the gate blind.
        "failover": _read(FAILOVER) + "\n" + _read(FAILOVER_TESTS),
        "build": _read(BUILD),
        "main": _read(MAIN),
        "workflow": _read(WORKFLOW),
    }
    verify_texts(files)
    mutation_self_test(files)
    print("B053-FAILOVER: PASS (event bucket, router, hysteresis, heartbeat, build wiring mutations red)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
