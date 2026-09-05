#!/usr/bin/env python3
"""Fail-closed source and mutation guard for B-244's OCI lock scope."""
from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/corelink-container/src/routes/oci.rs"
MAX_SOURCE_BYTES = 2_000_000
TEST_NAME = "inc6_manifest_put_index_stays_tenant_scoped"
TEST_DECLARATION = re.compile(
    r"(?m)^    #\[tokio::test\]\n    async fn (?P<name>[A-Za-z_][A-Za-z0-9_]*)\(\)"
)


class VerificationError(RuntimeError):
    pass


def read_source() -> str:
    if not SOURCE.is_file():
        raise VerificationError(f"missing B-244 source: {SOURCE}")
    source = SOURCE.read_text(encoding="utf-8")
    if len(source.encode("utf-8")) > MAX_SOURCE_BYTES:
        raise VerificationError("B-244 source exceeds bounded size")
    return source


def test_body(source: str) -> str:
    declarations = list(TEST_DECLARATION.finditer(source))
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
    return source[start:end]


def validate_source(source: str) -> None:
    body = test_body(source)
    required = (
        "let rows = kv.0.lock().unwrap();",
        "assert!(!rows.is_empty(), \"manifest PUT must persist tenant KV rows\");",
        "assert!(rows.keys().all(|(t, _)| t == &tenant.to_canonical_text()));",
        'kv\n            .get(&other, "oci_manifest:alpine:latest")\n            .await',
        ".is_none());",
    )
    for needle in required:
        if needle not in body:
            raise VerificationError(f"B-244 focal behavior marker missing: {needle!r}")

    # The lock and its reads must be inside a lexical block which closes before
    # the async read. An explicit drop is not accepted as a substitute: clippy
    # still considers the guard live in this pattern.
    scoped = re.search(
        r"\n        \{\n            let rows = kv\.0\.lock\(\)\.unwrap\(\);"
        r"(?P<body>.*?)\n        \}\n        assert!\(kv\s*\.get\(&other,",
        body,
        re.DOTALL,
    )
    if scoped is None:
        raise VerificationError("B-244 rows guard is not lexically scoped before await")
    if ".await" in scoped.group("body"):
        raise VerificationError("B-244 rows scope still crosses an await")
    if "drop(rows)" in body:
        raise VerificationError("B-244 must use lexical scope, not drop(rows)")


def mutation_checks(source: str) -> None:
    """Ensure removing either proof tooth or boundary is rejected."""
    validate_source(source)
    assertion = '            assert!(rows.keys().all(|(t, _)| t == &tenant.to_canonical_text()));\n'
    mutant = source.replace(assertion, "", 1)
    if mutant == source:
        raise VerificationError("B-244 assertion mutation fixture did not change source")
    try:
        validate_source(mutant)
    except VerificationError:
        pass
    else:
        raise VerificationError("B-244 tenant-scope assertion mutation was accepted")

    opening = "        {\n            let rows = kv.0.lock().unwrap();"
    mutant = source.replace(opening, "        let rows = kv.0.lock().unwrap();", 1)
    if mutant == source:
        raise VerificationError("B-244 lexical-scope mutation fixture did not change source")
    try:
        validate_source(mutant)
    except VerificationError:
        pass
    else:
        raise VerificationError("B-244 lexical-scope mutation was accepted")

    declaration = "    #[tokio::test]\n" f"    async fn {TEST_NAME}()"
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
        declaration + "\n    #[tokio::test]\n    async fn " + TEST_NAME + "()",
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


def main() -> int:
    try:
        source = read_source()
        mutation_checks(source)
    except (OSError, VerificationError) as error:
        print(f"B-244 DRIFTED: {error}", file=sys.stderr)
        return 1
    print("B-244 confirmed: rows lock is lexically closed before await; 4/4 mutations rejected")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
