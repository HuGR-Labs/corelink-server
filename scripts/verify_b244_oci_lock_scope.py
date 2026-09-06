#!/usr/bin/env python3
"""Fail-closed source and mutation guard for B-244's OCI lock scope."""
from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/corelink-container/src/routes/oci.rs"
SPLIT_SOURCES = (
    SOURCE,
    ROOT / "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs",
    ROOT / "crates/corelink-container/src/routes/oci/b126_m2_impl_02.rs",
    ROOT / "crates/corelink-container/src/routes/oci/b126_m2_test_1_1.rs",
    ROOT / "crates/corelink-container/src/routes/oci/b126_m2_test_1_2.rs",
    ROOT / "crates/corelink-container/src/routes/oci/b126_m2_test_1_3.rs",
)
MAX_SOURCE_BYTES = 2_000_000
TEST_NAME = "inc6_manifest_put_index_stays_tenant_scoped"
TEST_DECLARATION = re.compile(
    r"(?m)^[ \t]*#\[tokio::test\]\r?\n[ \t]*async fn (?P<name>[A-Za-z_][A-Za-z0-9_]*)\(\)"
)


class VerificationError(RuntimeError):
    pass


def _raw_string_start(text: str, i: int) -> tuple[int, str] | None:
    """Return (prefix length, terminator) for a raw-string opener."""
    if i and (text[i - 1].isalnum() or text[i - 1] == "_"):
        return None
    prefix_len = 2 if text.startswith("br", i) else 1 if text.startswith("r", i) else 0
    if not prefix_len:
        return None
    j = i + prefix_len
    while j < len(text) and text[j] == "#":
        j += 1
    if j >= len(text) or text[j] != '"':
        return None
    hashes = text[i + prefix_len : j]
    return j - i + 1, '"' + hashes


def _mask_comments(text: str) -> str:
    """Mask Rust comments while preserving offsets and line boundaries."""
    out = list(text)
    i = 0
    in_string = False
    escaped = False
    raw_terminator: str | None = None
    while i < len(text):
        if raw_terminator is not None:
            if text.startswith(raw_terminator, i):
                i += len(raw_terminator)
                raw_terminator = None
            else:
                i += 1
            continue
        if in_string:
            if escaped:
                escaped = False
            elif text[i] == "\\":
                escaped = True
            elif text[i] == '"':
                in_string = False
            i += 1
            continue
        raw = _raw_string_start(text, i)
        if raw is not None:
            opener_len, raw_terminator = raw
            i += opener_len
            continue
        if text.startswith("//", i):
            end = text.find("\n", i)
            end = len(text) if end < 0 else end
            for j in range(i, end):
                out[j] = " "
            i = end
        elif text.startswith("/*", i):
            end = text.find("*/", i + 2)
            end = len(text) - 2 if end < 0 else end
            for j in range(i, min(end + 2, len(text))):
                if out[j] != "\n":
                    out[j] = " "
            i = min(end + 2, len(text))
        else:
            if text[i] == '"':
                in_string = True
            i += 1
    return "".join(out)


def _contains_code_marker(text: str, needle: str) -> bool:
    """Find a marker outside comments and quoted string literals."""
    return _contains_code_marker_masked(_mask_comments(text), needle)


def _contains_code_marker_masked(masked: str, needle: str) -> bool:
    """Find a marker in text whose comments have already been masked."""
    in_string = False
    escaped = False
    i = 0
    while i < len(masked):
        char = masked[i]
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            i += 1
            continue
        # Rust raw strings (including byte raw strings) do not use backslash
        # escaping; skip them as one literal so an embedded quote cannot turn
        # the scanner's state inside-out before a later code marker.
        raw = _raw_string_start(masked, i)
        if raw is not None:
            opener_len, terminator = raw
            end = masked.find(terminator, i + opener_len)
            i = len(masked) if end < 0 else end + len(terminator)
            continue
        if char == '"':
            in_string = True
            i += 1
            continue
        if masked.startswith(needle, i):
            return True
        i += 1
    return False


def read_source() -> str:
    parts = []
    total_bytes = 0
    for path in SPLIT_SOURCES:
        if not path.is_file():
            raise VerificationError(f"missing B-244 source fragment: {path}")
        text = path.read_text(encoding="utf-8")
        total_bytes += len(text.encode("utf-8"))
        parts.append(f"// B-244 source fragment: {path.relative_to(ROOT)}\n{text}")
    if total_bytes > MAX_SOURCE_BYTES:
        raise VerificationError("B-244 source exceeds bounded size")
    # The production Rust module is include!-composed. Concatenating the
    # bounded fragments gives the verifier the semantic namespace while the
    # composition markers below keep each include edge load-bearing.
    return "\n".join(parts)


def test_body(source: str, masked: str | None = None) -> str:
    scanned = _mask_comments(source) if masked is None else masked
    declarations = list(TEST_DECLARATION.finditer(scanned))
    focal = [match for match in declarations if match.group("name") == TEST_NAME]
    if len(focal) != 1:
        raise VerificationError(
            f"B-244 requires exactly one #[tokio::test] focal declaration, found {len(focal)}"
        )
    position = declarations.index(focal[0])
    if position + 1 >= len(declarations):
        raise VerificationError("B-244 focal test has no unambiguous following test boundary")
    start = focal[0].end()
    end = declarations[position + 1].start()
    if end - start > 100_000:
        raise VerificationError("B-244 focal test boundary is missing or too large")
    return scanned[start:end]


def validate_source(source: str) -> None:
    masked = _mask_comments(source)
    for marker in (
        'include!("oci/b126_m2_impl_01.rs")',
        'include!("oci/b126_m2_impl_02.rs")',
        'include!("b126_m2_test_1_2.rs")',
    ):
        if not _contains_code_marker_masked(masked, marker):
            raise VerificationError(f"B-244 split composition marker missing: {marker!r}")
    body = test_body(source, masked)
    required = (
        "let rows = kv.0.lock().unwrap();",
        "assert!(!rows.is_empty()",
        "assert!(rows.keys().all(|(t, _)| t == &tenant.to_canonical_text()));",
        "assert!(kv",
        '.get(&other, "oci_manifest:alpine:latest")',
        ".is_none());",
    )
    for needle in required:
        if not _contains_code_marker_masked(body, needle):
            raise VerificationError(f"B-244 focal behavior marker missing: {needle!r}")

    # The lock and its reads must be inside a lexical block which closes before
    # the async read. An explicit drop is not accepted as a substitute: clippy
    # still considers the guard live in this pattern.
    scoped = re.search(
        r"\n[ \t]+\{\n[ \t]+let rows = kv\.0\.lock\(\)\.unwrap\(\);"
        r"(?P<body>.*?)\n[ \t]+\}\n[ \t]+assert!\(kv\s*\.get\(&other,",
        _mask_comments(body),
        re.DOTALL,
    )
    if scoped is None:
        raise VerificationError("B-244 rows guard is not lexically scoped before await")
    if ".await" in scoped.group("body"):
        raise VerificationError("B-244 rows scope still crosses an await")
    if _contains_code_marker_masked(body, "drop(rows)"):
        raise VerificationError("B-244 must use lexical scope, not drop(rows)")


def mutation_checks(source: str) -> None:
    """Ensure removing either proof tooth or boundary is rejected."""
    validate_source(source)
    assertion = '        assert!(rows.keys().all(|(t, _)| t == &tenant.to_canonical_text()));\n'
    mutant = source.replace(assertion, "", 1)
    if mutant == source:
        raise VerificationError("B-244 assertion mutation fixture did not change source")
    try:
        validate_source(mutant)
    except VerificationError:
        pass
    else:
        raise VerificationError("B-244 tenant-scope assertion mutation was accepted")

    commented = source.replace(
        assertion,
        "        // " + assertion.lstrip(),
        1,
    )
    if commented == source:
        raise VerificationError("B-244 comment mutation fixture did not change source")
    try:
        validate_source(commented)
    except VerificationError:
        pass
    else:
        raise VerificationError("B-244 comment-only assertion mutation was accepted")

    string_bait = source.replace(
        assertion,
        '        const _BAIT: &str = "assert!(rows.keys().all(|(t, _)| t == &tenant.to_canonical_text()));";\n',
        1,
    )
    if string_bait == source:
        raise VerificationError("B-244 string mutation fixture did not change source")
    try:
        validate_source(string_bait)
    except VerificationError:
        pass
    else:
        raise VerificationError("B-244 string-only assertion mutation was accepted")

    opening = "    {\n        let rows = kv.0.lock().unwrap();"
    mutant = source.replace(opening, "    let rows = kv.0.lock().unwrap();", 1)
    if mutant == source:
        raise VerificationError("B-244 lexical-scope mutation fixture did not change source")
    try:
        validate_source(mutant)
    except VerificationError:
        pass
    else:
        raise VerificationError("B-244 lexical-scope mutation was accepted")

    declaration = "#[tokio::test]\n" f"async fn {TEST_NAME}()"
    renamed = source.replace(TEST_NAME, f"{TEST_NAME}_renamed", 1)
    if renamed == source:
        raise VerificationError("B-244 renamed-boundary mutation fixture did not change source")
    try:
        validate_source(renamed)
    except VerificationError:
        pass
    else:
        raise VerificationError("B-244 renamed focal boundary was accepted")
    validate_source(source)

    duplicate = source.replace(
        declaration,
        declaration + "\n#[tokio::test]\nasync fn " + TEST_NAME + "()",
        1,
    )
    if duplicate == source:
        raise VerificationError("B-244 duplicate-boundary mutation fixture did not change source")
    try:
        validate_source(duplicate)
    except VerificationError:
        pass
    else:
        raise VerificationError("B-244 duplicate focal boundary was accepted")
    validate_source(source)

    for marker in (
        'include!("oci/b126_m2_impl_02.rs")',
        'include!("b126_m2_test_1_2.rs")',
    ):
        mutant = source.replace(marker, "", 1)
        if mutant == source:
            raise VerificationError("B-244 split-composition mutation fixture did not change source")
        try:
            validate_source(mutant)
        except VerificationError:
            pass
        else:
            raise VerificationError(f"B-244 split-composition mutation was accepted: {marker}")

    noop = source.replace("B244_NONEXISTENT_MARKER", "")
    if noop != source:
        raise VerificationError("B-244 no-op fixture unexpectedly changed source")


def main() -> int:
    try:
        source = read_source()
        mutation_checks(source)
    except (OSError, VerificationError) as error:
        print(f"B-244 DRIFTED: {error}", file=sys.stderr)
        return 1
    print("B-244 confirmed: rows lock is lexically closed before await; 8/8 mutations rejected; no-op fixture unchanged")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
