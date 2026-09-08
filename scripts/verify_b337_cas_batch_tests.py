#!/usr/bin/env python3
"""Verify the registered CAS batch-test population after the B-337 split.

``cas.rs`` is an ``include!`` composition point.  A file that merely exists
under ``routes/cas`` is not an executable test, and a path mentioned in a
comment or string is not a registration.  This guard therefore extracts the
actual ``mod tests`` include sequence, requires the reviewed split exactly,
and checks the named batch tests from the registered files.  The census is
the 28-test focused batch population used by the D03 proof.
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CAS_ROUTE = "crates/corelink-container/src/routes/cas.rs"
CAS_PARTS = "crates/corelink-container/src/routes/cas"

EXPECTED_TEST_INCLUDES = (
    "cas/tests_core_part1.rs",
    "cas/tests_core_part2.rs",
    "cas/tests_batch_part1.rs",
    "cas/tests_batch_part2.rs",
    "cas/tests_batch_write_part2.rs",
    "cas/tests_edges.rs",
)
EXPECTED_READ_CEILING_INCLUDES = ("cas/tests_read_ceiling.rs",)

# The names are the semantic identity of the B-337 population.  A plain
# count would let a test disappear while an unrelated test kept the total
# green; the file mapping makes that substitution visible.
EXPECTED_BATCH_TESTS = {
    "tests_core_part1.rs": (
        "batch_parser_rejects_line_and_hash_clone_amplification",
        "batch_parser_rejects_more_than_the_frozen_object_cap_before_allocating_all_lines",
    ),
    "tests_batch_part1.rs": (
        "batch_upload_all_created",
        "batch_write_at_concurrency_limit_returns_429_before_body",
        "batch_write_below_limit_releases_slot",
        "batch_read_at_concurrency_limit_returns_429_before_body",
        "batch_exists_at_concurrency_limit_returns_429_before_body",
    ),
    "tests_batch_part2.rs": (
        "batch_read_below_limit_releases_slot",
        "batch_read_admission_is_released_after_response_consumed",
        "batch_read_window_is_bounded_and_passes_object_ceiling",
        "batch_read_failure_drains_all_active_tasks_before_return",
        "batch_read_overflow_drains_all_active_tasks_before_return",
        "batch_read_cancellation_aborts_tasks_and_waits_for_unwind",
    ),
    "tests_batch_write_part2.rs": (
        "batch_reupload_returns_exists",
        "batch_upload_one_bad_object_others_commit",
        "batch_upload_over_object_cap_returns_413",
        "batch_upload_over_byte_cap_returns_413",
        "batch_upload_oversized_hash_is_rejected_before_storage",
    ),
    "tests_edges.rs": (
        "batch_upload_wrong_content_type_returns_415",
        "batch_upload_framing_mismatch_returns_400",
        "batch_upload_cross_tenant_returns_403",
        "batch_upload_read_only_scope_returns_403",
        "batch_read_round_trip_with_absent",
        "batch_read_preserves_order_and_slicing",
        "batch_read_tombstoned_is_gone",
        "batch_exists_present_and_absent",
        "batch_exists_over_cap_returns_413",
        "batch_upload_charges_quota_once_for_n",
    ),
}
EXPECTED_BATCH_TEST_COUNT = sum(len(names) for names in EXPECTED_BATCH_TESTS.values())


class ContractError(RuntimeError):
    """The executable registration or test census cannot be trusted."""


def _raw_string_end(source: str, start: int) -> int | None:
    """Return the end of a Rust raw string beginning at ``start``."""
    if source[start : start + 1] != "r":
        return None
    cursor = start + 1
    hashes = 0
    while cursor < len(source) and source[cursor] == "#":
        hashes += 1
        cursor += 1
    if cursor >= len(source) or source[cursor] != '"':
        return None
    terminator = '"' + ("#" * hashes)
    end = source.find(terminator, cursor + 1)
    if end < 0:
        raise ContractError("unterminated raw string")
    return end + len(terminator)


def _code_mask(source: str) -> str:
    """Blank comments and literals while preserving source offsets/newlines."""
    chars = list(source)
    i = 0
    block_depth = 0
    while i < len(source):
        if source[i : i + 2] == "//":
            end = source.find("\n", i + 2)
            end = len(source) if end < 0 else end
            for index in range(i, end):
                chars[index] = " "
            i = end
            continue
        if source[i : i + 2] == "/*":
            block_depth = 1
            start = i
            i += 2
            while i < len(source) and block_depth:
                if source[i : i + 2] == "/*":
                    block_depth += 1
                    i += 2
                elif source[i : i + 2] == "*/":
                    block_depth -= 1
                    i += 2
                else:
                    i += 1
            for index in range(start, i):
                if source[index] != "\n":
                    chars[index] = " "
            continue
        raw_end = _raw_string_end(source, i)
        if raw_end is not None:
            for index in range(i, raw_end):
                if source[index] != "\n":
                    chars[index] = " "
            i = raw_end
            continue
        if source[i] == '"':
            start = i
            i += 1
            while i < len(source):
                if source[i] == "\\":
                    i += 2
                elif source[i] == '"':
                    i += 1
                    break
                else:
                    i += 1
            for index in range(start, i):
                if source[index] != "\n":
                    chars[index] = " "
            continue
        i += 1
    return "".join(chars)


def _module_body(source: str, module: str) -> str:
    masked = _code_mask(source)
    match = re.search(rf"^\s*mod\s+{re.escape(module)}\s*\{{", masked, re.MULTILINE)
    if match is None:
        raise ContractError(f"missing mod {module} {{ registration")
    opening = source.find("{", match.start(), match.end())
    depth = 0
    state = "code"
    block_depth = 0
    i = opening
    while i < len(source):
        char = source[i]
        next_char = source[i + 1] if i + 1 < len(source) else ""
        if state == "line":
            if char == "\n":
                state = "code"
        elif state == "block":
            if char == "/" and next_char == "*":
                block_depth += 1
                i += 1
            elif char == "*" and next_char == "/":
                block_depth -= 1
                i += 1
                if block_depth == 0:
                    state = "code"
        elif state == "string":
            if char == "\\":
                i += 1
            elif char == '"':
                state = "code"
        else:
            if char == "/" and next_char == "/":
                state = "line"
                i += 1
            elif char == "/" and next_char == "*":
                state = "block"
                block_depth = 1
                i += 1
            elif char == "r":
                raw_end = _raw_string_end(source, i)
                if raw_end is not None:
                    i = raw_end - 1
            elif char == '"':
                state = "string"
            elif char == "{":
                depth += 1
            elif char == "}":
                depth -= 1
                if depth == 0:
                    return source[opening + 1 : i]
        i += 1
    raise ContractError(f"unterminated mod {module} {{ registration")


def _include_paths(source: str) -> tuple[str, ...]:
    """Return only literal include! calls in code, ignoring comments/strings."""
    paths: list[str] = []
    i = 0
    while i < len(source):
        char = source[i]
        next_char = source[i + 1] if i + 1 < len(source) else ""
        if char == "/" and next_char == "/":
            i = source.find("\n", i + 2)
            if i < 0:
                break
            continue
        if char == "/" and next_char == "*":
            end = source.find("*/", i + 2)
            if end < 0:
                raise ContractError("unterminated block comment")
            i = end + 2
            continue
        raw_end = _raw_string_end(source, i)
        if raw_end is not None:
            i = raw_end
            continue
        if char == '"':
            i += 1
            while i < len(source):
                if source[i] == "\\":
                    i += 2
                elif source[i] == '"':
                    i += 1
                    break
                else:
                    i += 1
            continue
        match = re.match(r"include\b", source[i:])
        if match is None:
            i += 1
            continue
        cursor = i + len(match.group(0))
        while cursor < len(source) and source[cursor].isspace():
            cursor += 1
        if source[cursor : cursor + 2] != "!(":
            i = cursor
            continue
        cursor += 2
        while cursor < len(source) and source[cursor].isspace():
            cursor += 1
        if cursor >= len(source) or source[cursor] != '"':
            raise ContractError("include! path must be a literal string")
        cursor += 1
        start = cursor
        while cursor < len(source) and source[cursor] != '"':
            if source[cursor] == "\\":
                raise ContractError("include! path contains an escape")
            cursor += 1
        if cursor >= len(source):
            raise ContractError("unterminated include! path")
        path = source[start:cursor]
        cursor += 1
        while cursor < len(source) and source[cursor].isspace():
            cursor += 1
        if source[cursor : cursor + 2] != ");":
            raise ContractError("malformed include! invocation")
        paths.append(path)
        i = cursor + 2
    return tuple(paths)


_TEST_RE = re.compile(
    r"#\[(?:tokio::)?test(?:\s*\([^]]*\))?\]\s*"
    r"(?:async\s+)?fn\s+([A-Za-z_]\w*)\s*\(",
    re.MULTILINE,
)


def _test_names(source: str) -> tuple[str, ...]:
    return tuple(_TEST_RE.findall(source))


def assess_source(route: str, parts: dict[str, str]) -> list[str]:
    gaps: list[str] = []
    try:
        test_module = _module_body(route, "tests")
        read_module = _module_body(route, "read_size_ceiling_tests")
        found_tests = _include_paths(test_module)
        found_read = _include_paths(read_module)
    except ContractError as error:
        return [str(error)]
    if found_tests != EXPECTED_TEST_INCLUDES:
        gaps.append(f"test-include-registration (expected {EXPECTED_TEST_INCLUDES!r}, found {found_tests!r})")
    if found_read != EXPECTED_READ_CEILING_INCLUDES:
        gaps.append(f"read-ceiling-include-registration (expected {EXPECTED_READ_CEILING_INCLUDES!r}, found {found_read!r})")

    if tuple(sorted(parts)) != tuple(sorted(name.removeprefix("cas/") for name in EXPECTED_TEST_INCLUDES)) + ("tests_read_ceiling.rs",):
        gaps.append("CAS test source inventory")
    observed: dict[str, tuple[str, ...]] = {}
    for name, expected in EXPECTED_BATCH_TESTS.items():
        text = parts.get(name)
        if text is None:
            continue
        # The registered files also carry a few non-batch CAS tests (for
        # example the single-read slot test).  The B-337 population is the
        # exact ``batch_*`` identity, not every test in those source units.
        observed[name] = tuple(test for test in _test_names(text) if test.startswith("batch_"))
        if observed[name] != expected:
            gaps.append(f"batch-test-identity:{name}")
    if sum(len(names) for names in observed.values()) != EXPECTED_BATCH_TEST_COUNT:
        gaps.append(f"batch-test-census (expected {EXPECTED_BATCH_TEST_COUNT}, found {sum(len(names) for names in observed.values())})")
    return gaps


def assess(root: Path = ROOT) -> list[str]:
    route = (root / CAS_ROUTE).read_text(encoding="utf-8")
    expected = tuple(name.removeprefix("cas/") for name in EXPECTED_TEST_INCLUDES) + ("tests_read_ceiling.rs",)
    parts: dict[str, str] = {}
    for name in expected:
        path = root / CAS_PARTS / name
        if path.is_symlink() or not path.is_file():
            return [f"missing or non-regular CAS test source: {name}"]
        parts[name] = path.read_text(encoding="utf-8")
    return assess_source(route, parts)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    try:
        gaps = assess(args.root)
    except (OSError, UnicodeError, ContractError) as error:
        print(f"B-337 CAS batch-test census: FAIL: {error}")
        return 2
    if gaps:
        print("B-337 CAS batch-test census: FAIL: " + "; ".join(gaps))
        return 1
    print(f"B-337 CAS batch-test census: PASS ({EXPECTED_BATCH_TEST_COUNT} registered batch tests)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
