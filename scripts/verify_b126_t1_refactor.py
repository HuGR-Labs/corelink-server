#!/usr/bin/env python3
"""Independent structural gate for B-126-T1's bounded Rust source units.

The gate does not import or execute the container crate's ``main_tests``
module. It reads source files directly, so removing that ``#[path] mod``
cannot disable the gate or make it circular.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from typing import Mapping


ROOT_UNITS = (
    "crates/corelink-container/src/routes/dpa_accept.rs",
    "crates/corelink-container/src/routes/public_revoke.rs",
    "crates/corelink-container/src/routes/failover.rs",
    "crates/corelink-container/src/routes/public_mirror.rs",
    "crates/corelink-container/src/routes/signup.rs",
    "crates/corelink-container/src/routes/ratelimit_layer.rs",
    "crates/corelink-container/src/routes/dsr/adapter_d1.rs",
    "crates/corelink-container/src/main.rs",
)

SUPPORT_UNITS = (
    "crates/corelink-container/src/main_boot.rs",
    "crates/corelink-container/src/main_tests.rs",
    "crates/corelink-container/src/routes/signup_support.rs",
    "crates/corelink-container/src/routes/dpa_accept_tests.rs",
    "crates/corelink-container/src/routes/public_revoke_tests.rs",
    "crates/corelink-container/src/routes/failover_tests.rs",
    "crates/corelink-container/src/routes/public_mirror_tests.rs",
    "crates/corelink-container/src/routes/signup_tests.rs",
    "crates/corelink-container/src/routes/ratelimit_layer_tests.rs",
    "crates/corelink-container/src/routes/dsr/adapter_d1_registry.rs",
    "crates/corelink-container/src/routes/dsr/adapter_d1_tests.rs",
)

ALL_UNITS = ROOT_UNITS + SUPPORT_UNITS

TEST_WIRING = {
    "crates/corelink-container/src/routes/dpa_accept.rs": "dpa_accept_tests.rs",
    "crates/corelink-container/src/routes/public_revoke.rs": "public_revoke_tests.rs",
    "crates/corelink-container/src/routes/failover.rs": "failover_tests.rs",
    "crates/corelink-container/src/routes/public_mirror.rs": "public_mirror_tests.rs",
    "crates/corelink-container/src/routes/signup.rs": "signup_tests.rs",
    "crates/corelink-container/src/routes/ratelimit_layer.rs": "ratelimit_layer_tests.rs",
    "crates/corelink-container/src/routes/dsr/adapter_d1.rs": "adapter_d1_tests.rs",
}


def mask_rust(source: str) -> str:
    """Blank comments and literals, preserving newlines and code positions."""

    out: list[str] = []
    i = 0
    n = len(source)

    def blank(start: int, end: int) -> None:
        out.extend("\n" if c == "\n" else " " for c in source[start:end])

    def quoted_end(start: int, quote: str) -> int:
        j = start + 1
        while j < n:
            if source[j] == "\\":
                j += 2
            elif source[j] == quote:
                return j + 1
            else:
                j += 1
        return n

    while i < n:
        if source.startswith("//", i):
            end = source.find("\n", i + 2)
            end = n if end < 0 else end
            blank(i, end)
            i = end
            continue
        if source.startswith("/*", i):
            depth = 1
            end = i + 2
            while end < n and depth:
                if source.startswith("/*", end):
                    depth += 1
                    end += 2
                elif source.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            blank(i, end)
            i = end
            continue
        if source[i] in {"r", "b"}:
            raw_start = i + 1
            if source[i] == "b" and raw_start < n and source[raw_start] == "r":
                raw_start += 1
            marker = raw_start
            while marker < n and source[marker] == "#":
                marker += 1
            if marker < n and source[marker] == '"':
                hashes = source[raw_start:marker]
                terminator = '"' + hashes
                closing = source.find(terminator, marker + 1)
                end = n if closing < 0 else closing + len(terminator)
                blank(i, end)
                i = end
                continue
        if source[i] == '"':
            end = quoted_end(i, '"')
            blank(i, end)
            i = end
            continue
        # Lifetimes (`'tenant`) are code. A short Rust character literal is
        # masked when its closing quote is unambiguously adjacent.
        if source[i] == "'" and i + 2 < n and source[i + 2] == "'":
            end = quoted_end(i, "'")
            blank(i, end)
            i = end
            continue
        out.append(source[i])
        i += 1
    return "".join(out)


def _rust_string(source: str, start: int) -> tuple[str, int] | None:
    if start >= len(source) or source[start] != '"':
        return None
    i = start + 1
    value: list[str] = []
    while i < len(source):
        if source[i] == "\\" and i + 1 < len(source):
            value.append(source[i + 1])
            i += 2
        elif source[i] == '"':
            return "".join(value), i + 1
        else:
            value.append(source[i])
            i += 1
    return None


def parse_path_modules(source: str) -> list[tuple[str, str]]:
    """Extract real ``#[path = "..."] mod name;`` declarations."""

    code = mask_rust(source)
    found: list[tuple[str, str]] = []
    for match in re.finditer(r"#\s*\[\s*path\s*=", code):
        i = match.end()
        while i < len(source) and source[i].isspace():
            i += 1
        parsed = _rust_string(source, i)
        if parsed is None:
            continue
        value, end = parsed
        suffix = code[end:]
        mod = re.match(r"\s*\]\s*mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;", suffix)
        if mod:
            found.append((value, mod.group(1)))
    return found


def read_sources(root: Path, overrides: Mapping[str, str] | None = None) -> dict[str, str]:
    overrides = overrides or {}
    return {
        relative: overrides.get(relative, (root / relative).read_text(encoding="utf-8"))
        for relative in ALL_UNITS
    }


def verify(root: Path, overrides: Mapping[str, str] | None = None) -> list[str]:
    sources = read_sources(root, overrides)
    errors: list[str] = []
    for relative, source in sources.items():
        if len(source.splitlines()) > 1_000:
            errors.append(f"file-size regression: {relative} exceeds 1000 lines")

    for relative, expected in TEST_WIRING.items():
        paths = {module: path for path, module in parse_path_modules(sources[relative])}
        if paths.get("tests") != expected:
            errors.append(f"missing test wiring in {relative}: expected tests -> {expected}")

    signup = {
        module: path
        for path, module in parse_path_modules(
            sources["crates/corelink-container/src/routes/signup.rs"]
        )
    }
    if signup.get("support") != "signup_support.rs":
        errors.append("missing signup support wiring: expected support -> signup_support.rs")

    adapter_d1 = {
        module: path
        for path, module in parse_path_modules(
            sources["crates/corelink-container/src/routes/dsr/adapter_d1.rs"]
        )
    }
    if adapter_d1.get("registry") != "adapter_d1_registry.rs":
        errors.append(
            "missing D1 registry wiring: expected registry -> adapter_d1_registry.rs"
        )

    main = {
        module: path
        for path, module in parse_path_modules(sources["crates/corelink-container/src/main.rs"])
    }
    if main.get("boot") != "main_boot.rs":
        errors.append("missing boot wiring in main.rs: expected boot -> main_boot.rs")
    return errors


def self_test(root: Path) -> None:
    assert verify(root) == []
    fixture = (
        '/* nested /* #[path = "fake.rs"] mod fake; */ still fake */\n'
        'let raw = r###"#[path = "fake.rs"] mod fake;"###;\n'
        'let raw_bytes = br##"#[path = "fake.rs"] mod fake;"##;\n'
        '// #[path = "fake.rs"] mod fake;\n'
        '/* #[path = "fake.rs"] mod fake; */\n'
        'let text = "#[path = \\\"fake.rs\\\"] mod fake;";\n'
        '#[path = "real.rs"] mod real;\n'
    )
    assert parse_path_modules(fixture) == [("real.rs", "real")]

    dpa_path = "crates/corelink-container/src/routes/dpa_accept.rs"
    dpa = (root / dpa_path).read_text(encoding="utf-8")
    removed = dpa.replace('#[path = "dpa_accept_tests.rs"]\nmod tests;\n', "", 1)
    diagnostics = verify(root, {dpa_path: removed})
    assert any("missing test wiring" in error and "dpa_accept_tests.rs" in error for error in diagnostics)

    signup_path = "crates/corelink-container/src/routes/signup.rs"
    signup = (root / signup_path).read_text(encoding="utf-8")
    renamed = signup.replace('#[path = "signup_support.rs"]', '#[path = "signup_support_renamed.rs"]', 1)
    diagnostics = verify(root, {signup_path: renamed})
    assert any("missing signup support wiring" in error for error in diagnostics)

    adapter_path = "crates/corelink-container/src/routes/dsr/adapter_d1.rs"
    adapter = (root / adapter_path).read_text(encoding="utf-8")
    renamed = adapter.replace(
        '#[path = "adapter_d1_registry.rs"]',
        '#[path = "adapter_d1_registry_renamed.rs"]',
        1,
    )
    diagnostics = verify(root, {adapter_path: renamed})
    assert any("missing D1 registry wiring" in error for error in diagnostics)

    # The external gate remains executable and green if the Rust test module
    # is removed; it does not consume the circular main_tests guard.
    main_path = "crates/corelink-container/src/main.rs"
    main = (root / main_path).read_text(encoding="utf-8")
    without_main_tests = main.replace('#[path = "main_tests.rs"]\nmod tests;\n', "", 1)
    assert verify(root, {main_path: without_main_tests}) == []
    print("PASS: B126-T1 independent verifier, parser, and wiring mutations")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test(args.root)
        return 0
    errors = verify(args.root)
    if errors:
        for error in errors:
            print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print("PASS: B126-T1 source units, wiring, and size cap")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
