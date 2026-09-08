#!/usr/bin/env python3
"""Fail-closed structural contract for D03 bundle-CI repairs B-264..B-267."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ALERTS = "crates/corelink-ops/src/alerts/channel.rs"
SEALED = "crates/corelink-audit-chain/src/sealed_archive.rs"
MIGRATIONS = "crates/corelink-ops/src/migrations.rs"
AUDIT_MODULES = {
    "crates/corelink-audit-chain/src/archive_producer.rs": "archive_producer_tests.rs",
    "crates/corelink-audit-chain/src/neon_shadow.rs": "neon_shadow_tests.rs",
    SEALED: "sealed_archive_tests.rs",
}


class VerificationError(RuntimeError):
    """A bundle repair or its load-bearing regression contract drifted."""


def _read(root: Path, path: str, overrides: dict[str, str]) -> str:
    if path in overrides:
        return overrides[path]
    try:
        return (root / path).read_text(encoding="utf-8")
    except OSError as exc:
        raise VerificationError(f"missing B-264..B-267 input: {path}") from exc


def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> None:
    overrides = overrides or {}
    alerts = _read(root, ALERTS, overrides)
    sealed = _read(root, SEALED, overrides)
    migrations = _read(root, MIGRATIONS, overrides)

    documented_field = re.compile(
        r"(?m)^    NotConfigured \{\n"
        r"        /// Channel missing endpoint/provider configuration\.\n"
        r"        channel: AlertChannel,\n"
        r"    \},$"
    )
    if len(documented_field.findall(alerts)) != 1:
        raise VerificationError("B-264 NotConfigured.channel must have its field documentation")

    if sealed.count("lines.get(..0).unwrap_or(&[])") != 1 or "&lines[..0]" in sealed:
        raise VerificationError("B-265 empty prefix must avoid indexing_slicing")
    if sealed.count("!= Ok(claimed)") != 1 or ".map_or(true, |computed| computed != claimed)" in sealed:
        raise VerificationError("B-265 link comparison must use the direct Result comparison")

    required_migration_markers = (
        "let mut case_depth: u32 = 0;",
        'else if upper == "CASE" && trigger_depth > 0',
        'else if upper == "END" && case_depth > 0',
        "case_depth = case_depth.saturating_sub(1);",
        "fn split_statements_respects_case_end_inside_trigger_body()",
        "assert_eq!(parts.len(), 4, \"got: {parts:#?}\");",
    )
    for marker in required_migration_markers:
        if migrations.count(marker) != 1:
            raise VerificationError(f"B-266 migration splitter marker must occur once: {marker}")

    for parent, child in AUDIT_MODULES.items():
        source = _read(root, parent, overrides)
        binding = re.compile(rf'(?m)^#\[path = "{re.escape(child)}"\]\nmod {re.escape(child[:-3])};$')
        if len(binding.findall(source)) != 1:
            raise VerificationError(f"B-267 {parent} must bind sibling {child} exactly once")
        child_path = str(Path(parent).with_name(child))
        _read(root, child_path, overrides)


if __name__ == "__main__":
    try:
        verify()
    except VerificationError as exc:
        raise SystemExit(f"B-264..B-267 BROKEN: {exc}")
    print("B-264..B-267 D03 bundle repairs: PASS")
