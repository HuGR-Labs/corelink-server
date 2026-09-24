#!/usr/bin/env python3
"""Fail closed on the repository-owned B-122 phase-region contract."""

from __future__ import annotations

import argparse
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ADAPTER = "crates/corelink-container/src/adapter_cache.rs"
TIMING = "crates/corelink-container/src/origin_timing.rs"
RECORDING_TEST = "crates/corelink-container/src/origin_timing/tests_recording.rs"


def verify(root: Path) -> list[str]:
    """Return every missing repository-owned B-122 guardrail."""
    adapter = (root / ADAPTER).read_text(encoding="utf-8")
    timing = (root / TIMING).read_text(encoding="utf-8")
    recording_test = (root / RECORDING_TEST).read_text(encoding="utf-8")
    errors: list[str] = []

    if adapter.count("tokio::task::spawn_blocking") < 2:
        errors.append("adapter cache must retain both blocking CAS boundaries")
    if adapter.count("let ledger = crate::origin_timing::current_ledger();") < 2:
        errors.append("both blocking CAS boundaries must capture the current ledger")
    if adapter.count("PhaseScope::with_ledger(ledger)") < 2:
        errors.append("both blocking CAS boundaries must install the captured ledger")

    for required in (
        "depth: usize",
        "window.depth.checked_add(1)",
        "records: bool",
        "if self.records {",
        "if let Some(ledger) = &self.ledger {",
    ):
        if required not in timing:
            errors.append(f"PhaseScope depth guard is missing: {required}")
    if "nested_reentry_of_the_same_phase_records_once" not in recording_test:
        errors.append("same-phase re-entry adversarial test is missing")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    errors = verify(args.root.resolve())
    if errors:
        print("B-122 phase-region contract: FAIL", *errors, sep="\n- ")
        return 1
    print("B-122 phase-region contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
