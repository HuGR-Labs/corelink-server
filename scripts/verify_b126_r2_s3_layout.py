#!/usr/bin/env python3
"""Fail-closed, source-level guard for the B126 R2-S3 split.

This verifier is deliberately outside the Rust module it verifies.  It lexes
Rust enough to recognize real ``include!("...")`` invocations while ignoring
comments, normal strings, raw strings, and character literals.  Thus a
comment/string bait cannot replace a missing part or make a renamed part look
present.  Its built-in mutations exercise the same predicate used by the
normal check.
"""

from __future__ import annotations

import argparse
import re
import stat
import sys
from pathlib import Path


PARTS = (
    "client.rs",
    "client_impl.rs",
    "client_types.rs",
    "cas_core.rs",
    "cas_byok_body.rs",
    "cas_helpers.rs",
    "cas_ops.rs",
    "cas_batch.rs",
    "cas_write.rs",
    "ac_core.rs",
    "cas_builder.rs",
    "ac_handler.rs",
    "ac_ops.rs",
    "ac_update.rs",
    "ac_delete.rs",
    "ac_list.rs",
    "ac_builder.rs",
    "tests_1.rs",
    "tests_1_network.rs",
    "tests_2.rs",
    "tests_2_byok.rs",
    "tests_3.rs",
    "tests_4.rs",
)
EXPECTED_SYMBOLS = {
    "client.rs": ("R2S3Client", "CappedGet"),
    "cas_core.rs": ("R2CasHandler",),
    "cas_helpers.rs": ("public_namespace_prefix", "verify_content_hash"),
    "ac_core.rs": ("CasDeleteHandler", "CasListHandler"),
    "cas_builder.rs": (
        "build_r2_cas_handler_from_env",
        "validate_cas_bucket_for_region",
    ),
    "ac_handler.rs": ("R2AcHandler",),
    "ac_builder.rs": ("build_r2_ac_handler_from_env",),
}
INCLUDE_PREFIX = "r2_s3_parts/"


class LayoutError(RuntimeError):
    """The R2-S3 source layout cannot be trusted."""


def _quoted(text: str, start: int) -> int:
    """Return the end of a normal Rust string, or raise on unterminated input."""
    index = start + 1
    escaped = False
    while index < len(text):
        char = text[index]
        if escaped:
            escaped = False
        elif char == "\\":
            escaped = True
        elif char == '"':
            return index + 1
        index += 1
    raise LayoutError("unterminated Rust string")


def _raw_string(text: str, start: int) -> int | None:
    if text[start] != "r":
        return None
    cursor = start + 1
    while cursor < len(text) and text[cursor] == "#":
        cursor += 1
    if cursor >= len(text) or text[cursor] != '"':
        return None
    hashes = text[start + 1 : cursor]
    # A raw string closes with quote followed by exactly the opening hashes.
    terminator = '"' + hashes
    end = text.find(terminator, cursor + 1)
    if end < 0:
        raise LayoutError("unterminated Rust raw string")
    return end + len(terminator)


def tokens(text: str) -> list[tuple[str, str]]:
    """Lex identifiers, punctuation, and string tokens; discard bait syntax."""
    result: list[tuple[str, str]] = []
    index = 0
    while index < len(text):
        char = text[index]
        if char.isspace():
            index += 1
            continue
        if text.startswith("//", index):
            newline = text.find("\n", index + 2)
            index = len(text) if newline < 0 else newline + 1
            continue
        if text.startswith("/*", index):
            depth = 1
            index += 2
            while index < len(text) and depth:
                if text.startswith("/*", index):
                    depth += 1
                    index += 2
                elif text.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            if depth:
                raise LayoutError("unterminated Rust block comment")
            continue
        raw_end = _raw_string(text, index)
        if raw_end is not None:
            result.append(("string", text[index:raw_end]))
            index = raw_end
            continue
        if char == '"':
            end = _quoted(text, index)
            result.append(("string", text[index:end]))
            index = end
            continue
        if char == "'":
            # Keep lifetimes (`'a`, `'static`) lexable as punctuation plus an
            # identifier; discard only an actual character literal.
            if index + 2 < len(text) and text[index + 2] == "'":
                index += 3
            elif index + 1 < len(text) and text[index + 1] == "\\":
                index += 2
                if index < len(text) and text[index] == "'":
                    index += 1
                else:
                    while index < len(text) and text[index] != "'":
                        index += 1
                    if index < len(text):
                        index += 1
            else:
                result.append(("punct", "'"))
                index += 1
            continue
        match = re.match(r"[A-Za-z_][A-Za-z0-9_]*", text[index:])
        if match:
            value = match.group(0)
            result.append(("ident", value))
            index += len(value)
            continue
        result.append(("punct", char))
        index += 1
    return result


def active_identifiers(text: str) -> set[str]:
    return {value for kind, value in tokens(text) if kind == "ident"}


def include_paths(facade: str) -> tuple[str, ...]:
    lexed = tokens(facade)
    paths: list[str] = []
    for index, (kind, value) in enumerate(lexed):
        if kind != "ident" or value != "include":
            continue
        tail = lexed[index + 1 : index + 4]
        if len(tail) < 3 or tail[0] != ("punct", "!") or tail[1] != ("punct", "("):
            continue
        if tail[2][0] != "string":
            raise LayoutError("include! must have a literal path")
        if len(lexed) <= index + 4 or lexed[index + 4] != ("punct", ")"):
            raise LayoutError("include! path invocation is malformed")
        raw = tail[2][1]
        if not raw.startswith('"') or not raw.endswith('"'):
            raise LayoutError("raw include paths are not accepted")
        paths.append(raw[1:-1])
    return tuple(paths)


def verify_texts(facade: str, part_texts: dict[str, str]) -> list[str]:
    errors: list[str] = []
    found = include_paths(facade)
    expected = tuple(INCLUDE_PREFIX + name for name in PARTS)
    if found != expected:
        errors.append(f"include map mismatch: expected {expected!r}, found {found!r}")
    if set(part_texts) != set(PARTS):
        errors.append(f"part population mismatch: expected {PARTS!r}, found {tuple(sorted(part_texts))!r}")
    for name, symbols in EXPECTED_SYMBOLS.items():
        text = part_texts.get(name)
        if text is None:
            continue
        identifiers = active_identifiers(text)
        for symbol in symbols:
            if symbol not in identifiers:
                errors.append(f"{name} lost active symbol {symbol}")
    for name, text in part_texts.items():
        if not text.strip():
            errors.append(f"{name} is empty")
        if len(text.splitlines()) > 1000:
            errors.append(f"{name} exceeds 1000 lines")
    return errors


def mutation_self_test(facade: str, part_texts: dict[str, str]) -> None:
    baseline = verify_texts(facade, part_texts)
    if baseline:
        raise LayoutError("baseline is not green: " + "; ".join(baseline))
    for name in PARTS:
        mutated = facade.replace(f'include!("{INCLUDE_PREFIX}{name}");', "", 1)
        if not verify_texts(mutated, part_texts):
            raise LayoutError(f"removing include was accepted: {name}")
        renamed = facade.replace(f'include!("{INCLUDE_PREFIX}{name}");',
                                 f'include!("{INCLUDE_PREFIX}{name}.renamed");', 1)
        if not verify_texts(renamed, part_texts):
            raise LayoutError(f"renaming include was accepted: {name}")
    bait = facade + '\n// include!("r2_s3_parts/client.rs");\nconst BAIT: &str = "include!(\\"r2_s3_parts/cas_core.rs\\")";\n'
    if verify_texts(bait, part_texts):
        raise LayoutError("comment/string bait changed a valid baseline")
    escaped_char = facade.replace(
        "mod implementation {",
        "mod implementation { const B126_ESCAPED_CHAR: char = '\\\\';",
        1,
    )
    if verify_texts(escaped_char, part_texts):
        raise LayoutError("escaped backslash char hid the following include")
    escaped_removed = escaped_char.replace(
        'include!("r2_s3_parts/client.rs");', "", 1
    )
    if not verify_texts(escaped_removed, part_texts):
        raise LayoutError("include after escaped backslash was not load-bearing")


def read_regular(path: Path, label: str) -> str:
    try:
        mode = path.lstat().st_mode
    except OSError as exc:
        raise LayoutError(f"{label} unreadable: {path}: {exc}") from exc
    if not stat.S_ISREG(mode):
        raise LayoutError(f"{label} is not a regular file: {path}")
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise LayoutError(f"{label} unreadable: {path}: {exc}") from exc
    if not text.strip():
        raise LayoutError(f"{label} is empty: {path}")
    return text


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args(argv)
    root = args.root.resolve()
    try:
        facade = read_regular(root / "crates/corelink-container/src/storage/r2_s3.rs", "facade")
        directory = root / "crates/corelink-container/src/storage/r2_s3_parts"
        # Enumerate every Rust-looking entry, including directories and
        # symlinks, so a non-regular replacement cannot disappear from the
        # population before ``read_regular`` rejects it.
        names = tuple(path.name for path in directory.iterdir() if path.name.endswith(".rs"))
        part_texts = {
            name: read_regular(directory / name, f"part {name}")
            for name in names
            if name.endswith(".rs")
        }
        mutation_self_test(facade, part_texts)
    except (LayoutError, OSError) as exc:
        print(f"B126 RED: {exc}", file=sys.stderr)
        return 1
    print("B126 R2-S3 layout guard: PASS (include map, symbols, mutations, size)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
