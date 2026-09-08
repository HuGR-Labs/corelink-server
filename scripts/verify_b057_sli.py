#!/usr/bin/env python3
"""Fail-closed focal verifier for the B-057 CAS/AC SLI path."""

from __future__ import annotations

import argparse
from pathlib import Path


class ContractError(RuntimeError):
    pass


def _section(source: str, start: str, end: str) -> str:
    begin = source.find(start)
    if begin < 0:
        raise ContractError(f"missing {start}")
    finish = source.find(end, begin + len(start))
    if finish < 0:
        raise ContractError(f"missing section terminator {end}")
    return source[begin:finish]


def assess_source(storage: str, aggregate: str, docs: str) -> list[str]:
    gaps: list[str] = []
    try:
        cas_builder = _section(
            storage,
            "pub async fn build_r2_cas_handler_from_env",
            "/// A sync `AcLookupHandler`",
        )
        ac_builder = _section(
            storage,
            "pub async fn build_r2_ac_handler_from_env",
            "/// Load the tenant derivation key",
        )
    except ContractError as error:
        return [str(error)]

    if "crate::sli_aggregate::shared()" not in cas_builder:
        gaps.append("cas-production-sli-consumer")
    if "crate::sli_aggregate::shared()" not in ac_builder:
        gaps.append("ac-production-sli-consumer")
    if "InMemorySliObserver::new()" in cas_builder or "InMemorySliObserver::new()" in ac_builder:
        gaps.append("unbounded-production-sli-sink")

    if "BTreeMap<Sli, SliCounters>" not in aggregate:
        gaps.append("bounded-sli-storage")
    if "BurnRateCalculator::new().decide" not in aggregate:
        gaps.append("burn-rate-calculator-consumer")
    if "BurnRateWindow::Fast1h" not in aggregate:
        gaps.append("burn-rate-window")
    for token in ("VecDeque", "BUCKET_MS", "MAX_WINDOW_MS", "window_counters_at", "canonical_burn_rate_windows"):
        if token not in aggregate:
            gaps.append(f"temporal-{token.lower().replace('_', '-')}")
    if "window_duration_seconds()" not in aggregate:
        gaps.append("temporal-canonical-range-duration")
    if "decision = decision.slug()" not in aggregate:
        gaps.append("structured-burn-decision")

    for token in (
        "fn emit_lookup_sli(&self, is_error: bool, latency_us: u64)",
        "fn emit_update_sli(&self, is_error: bool, latency_us: u64)",
        "fn emit_list_sli(&self, is_error: bool, latency_us: u64)",
        "elapsed_us(started)",
    ):
        if token not in storage:
            gaps.append(f"latency-{token}")
    if "emit_lookup_sli(true, 0)" in storage or "emit_lookup_sli(false, 0)" in storage:
        gaps.append("ac-lookup-zero-latency")
    if "emit_update_sli(true, 0)" in storage or "emit_update_sli(false, 0)" in storage:
        gaps.append("ac-update-zero-latency")

    operations = (
        ("lookup", "impl corelink_handler_ac::AcLookupHandler", "impl corelink_handler_ac::AcUpdateHandler", "emit_lookup_sli"),
        ("update", "impl corelink_handler_ac::AcUpdateHandler", "impl corelink_handler_ac::AcDeleteHandler", "emit_update_sli"),
        ("delete", "impl corelink_handler_ac::AcDeleteHandler", "impl corelink_handler_ac::AcListHandler", "emit_update_sli"),
        ("list", "impl corelink_handler_ac::AcListHandler", "/// Build an `R2AcHandler`", "emit_list_sli"),
    )
    for operation, start, end, emit in operations:
        try:
            section = _section(storage, start, end)
        except ContractError as error:
            gaps.append(f"audit-failure-sli-{operation}-section")
            continue
        if "map_err(AcHandlerError::AuditFailed)" in section:
            gaps.append(f"audit-failure-sli-{operation}")
        closures = section.split("map_err(|e| {")[1:]
        audit_closures = []
        for chunk in closures:
            closure = chunk.split("})?", 1)[0]
            if "AcHandlerError::AuditFailed(e)" in closure:
                audit_closures.append(closure)
        if not audit_closures or any(
            f"self.{emit}(true, elapsed_us(started));" not in closure for closure in audit_closures
        ):
            gaps.append(f"audit-failure-sli-{operation}")
        if operation == "list" and "self.emit_lookup_sli(" in section:
            gaps.append("latency-catalog-list-is-hit")

    try:
        list_helper = _section(storage, "fn emit_list_sli", "    fn emit_update_sli")
        if "LatencyAcHitP99" in list_helper:
            gaps.append("latency-catalog-list-is-hit")
    except ContractError:
        gaps.append("latency-catalog-list-helper")

    for token in ("BurnRateCalculator", "burn-rate", "latency", "bounded", "list", "LatencyAcHitP99"):
        if token not in docs:
            gaps.append(f"okf-{token}")
    return gaps


def assess(root: Path) -> list[str]:
    storage = "\n".join(
        (root / path).read_text(encoding="utf-8")
        for path in (
            "crates/corelink-container/src/storage/r2_s3.rs",
            "crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs",
            "crates/corelink-container/src/storage/r2_s3_parts/ac_ops.rs",
        )
    )
    aggregate = (root / "crates/corelink-container/src/sli_aggregate.rs").read_text(encoding="utf-8")
    docs = (root / "docs/knowledge/storage/r2-cas-bucket.md").read_text(encoding="utf-8")
    return assess_source(storage, aggregate, docs)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    try:
        gaps = assess(args.root)
    except (OSError, UnicodeError) as error:
        print(f"B-057 verifier could not read inputs: {error}")
        return 2
    if gaps:
        print("B-057 OPEN: " + ", ".join(gaps))
        return 1
    print("B-057 DONE: bounded CAS/AC SLI consumer and measured latency are present")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
