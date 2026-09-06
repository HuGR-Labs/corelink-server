#!/usr/bin/env python3
"""Fail-closed source proof for B-056's process-wide CAS read byte budget."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class VerificationError(RuntimeError):
    pass


def fail(message: str) -> None:
    raise VerificationError(message)


def read(path: Path) -> str:
    if not path.is_file():
        fail(f"missing B-056 proof input: {path.relative_to(ROOT)}")
    return path.read_text(encoding="utf-8")


def source() -> dict[str, str]:
    cas_parts = (
        "crates/corelink-container/src/routes/cas/foundation.rs",
        "crates/corelink-container/src/routes/cas/single.rs",
        "crates/corelink-container/src/routes/cas/batch.rs",
        "crates/corelink-container/src/routes/cas/list_delete.rs",
    )
    return {
        "cas": "\n".join(read(ROOT / part) for part in cas_parts),
        "backlog": read(ROOT / "BACKLOG.md"),
        "native": read(ROOT / "docs/knowledge/surfaces/native-cas.md"),
        "container": read(ROOT / "docs/knowledge/planes/container.md"),
        "capacity": read(ROOT / "crates/corelink-container/src/container_capacity.rs"),
        "okf": read(ROOT / "docs/okf-wiki-site/index.html"),
        "changelog": read(ROOT / "changelog.d/056-cas-process-wide-budget.md"),
        "workflow": read(ROOT / ".github/workflows/backlog-verify.yml"),
    }


def handler_span(cas: str, name: str, next_name: str | None = None) -> str:
    end = f"async fn {next_name}" if next_name else "\n#[cfg(test)]"
    match = re.search(rf"async fn {name}\(.*?(?={re.escape(end)})", cas, re.S)
    if not match:
        fail(f"B-056 handler missing: {name}")
    return match.group(0)


def assess(files: dict[str, str], expected_status: str = "done") -> None:
    cas = files["cas"]
    required = (
        (
            "budget declaration",
            (
                "pub const CAS_READ_GLOBAL_BUDGET_BYTES: u64 = CONTAINER_MEMORY_BYTES / 2;",
                "pub const CAS_READ_GLOBAL_BUDGET_BYTES: u64 =\n    crate::container_capacity::CAS_READ_GLOBAL_BUDGET_BYTES;",
            ),
        ),
        (
            "budget unit",
            (
                "pub const CAS_READ_BUDGET_UNIT_BYTES: u64 = 1024 * 1024;",
                "const CAS_READ_BUDGET_UNIT_BYTES: u64 = crate::container_capacity::MEMORY_BUDGET_UNIT_BYTES;",
            ),
        ),
        "CAS_READ_GLOBAL_BUDGET_BYTES / CAS_READ_BUDGET_UNIT_BYTES <= u32::MAX as u64",
        "static GLOBAL_CAS_READ_BUDGET: OnceLock<Arc<Semaphore>> = OnceLock::new();",
        (
            "weighted acquire",
            (
                "budget.acquire_many_owned(permits)",
                "global_cas_read_budget().acquire_many_owned(permits)",
            ),
        ),
        "CAS_READ_GLOBAL_PERMIT_WAIT",
        "GlobalCasReadBudgetGuard",
        "GlobalCasBatchReadBudgetGuard",
        "StatusCode::SERVICE_UNAVAILABLE",
        '"global CAS read budget saturated; returning 503 before buffering"',
    )
    for requirement in required:
        if isinstance(requirement, tuple):
            label, alternatives = requirement
            if not any(needle in cas for needle in alternatives):
                fail(f"B-056 CAS budget contract missing: {label}")
        elif requirement not in cas:
            fail(f"B-056 CAS budget contract missing: {requirement}")
    if "crate::container_capacity::CAS_READ_GLOBAL_BUDGET_BYTES" in cas and (
        "pub const CAS_READ_GLOBAL_BUDGET_BYTES: u64 =" not in files["capacity"]
    ):
        fail("B-056 shared budget does not resolve to the central capacity declaration")

    single = handler_span(cas, "handle_read", "handle_write")
    if "_read_concurrency: CasReadConcurrencyGuard" not in single:
        fail("single CAS GET lost its per-tenant guard")
    if "_global_read_budget: GlobalCasReadBudgetGuard" not in single:
        fail("single CAS GET lost its process-wide byte guard")
    if single.index("_global_read_budget") < single.index("_read_concurrency"):
        fail("single CAS GET process-wide guard is not after the per-tenant guard")

    batch = handler_span(cas, "handle_batch_read", "handle_batch_exists")
    if "_read_concurrency: CasReadConcurrencyGuard" not in batch:
        fail("CAS batch-read lost its per-tenant guard")
    if "_global_read_budget: GlobalCasBatchReadBudgetGuard" not in batch:
        fail("CAS batch-read lost its process-wide byte guard")
    if batch.index("body: axum::body::Bytes") < batch.index("_global_read_budget"):
        fail("CAS batch-read buffers its body before reserving the byte budget")
    if not any(
        name in batch for name in ("acquire_cas_read_budget_from", "acquire_global_cas_read_budget")
    ) or "CAS_READ_SINGLE_PERMITS" not in batch:
        fail("CAS batch-read object fan-out lost its per-object byte reservations")

    # Global saturation logs deliberately carry only a static route and weight;
    # tenant/hash/request identifiers would turn a bounded guard into a high-
    # cardinality observability sink.
    saturation = cas[cas.index('"global CAS read budget saturated'):cas.index('"global CAS read budget saturated') + 180]
    if "tenant_id" in saturation or "hash" in saturation or "request_id" in saturation:
        fail("B-056 saturation log contains high-cardinality identity")

    backlog_match = re.search(r"### B-056 .*?(?=\n### B-052 )", files["backlog"], re.S)
    if not backlog_match:
        fail("B-056 backlog section is missing")
    section = backlog_match.group(0)
    if f"status: {expected_status}" not in section:
        fail(f"B-056 backlog status is not {expected_status!r}")
    if "verify_b056_cas_budget.py --self-test --expect done" not in section:
        fail("B-056 backlog does not name its executable verifier")
    if "process-wide" not in files["native"] or "CAS_READ_GLOBAL_BUDGET_BYTES" not in files["container"]:
        fail("B-056 knowledge docs do not describe the process-wide byte budget")
    if "CAS_READ_GLOBAL_BUDGET_BYTES" not in files["okf"]:
        fail("B-056 OKF render is missing the process-wide byte budget")
    if "B-056" not in files["changelog"]:
        fail("B-056 changelog fragment is missing its finding ID")
    if "B-056 CAS read budget verifier and mutations" not in files["workflow"]:
        fail("workflow does not run the B-056 verifier")


def mutation_checks(files: dict[str, str]) -> None:
    mutants = (
        (
            "budget sizing",
            "cas",
            (
                "CAS_READ_GLOBAL_BUDGET_BYTES: u64 = CONTAINER_MEMORY_BYTES / 2",
                "CAS_READ_GLOBAL_BUDGET_BYTES: u64 =\n    crate::container_capacity::CAS_READ_GLOBAL_BUDGET_BYTES",
            ),
            (
                "CAS_READ_GLOBAL_BUDGET_BYTES: u64 = CONTAINER_MEMORY_BYTES / 4",
                "CAS_READ_GLOBAL_BUDGET_BYTES: u64 =\n    crate::container_capacity::MISSING_BUDGET",
            ),
        ),
        ("weighted acquire", "cas", "acquire_many_owned(permits)", "acquire_owned()"),
        (
            "single guard",
            "cas",
            "_global_read_budget: GlobalCasReadBudgetGuard",
            "_global_read_budget: MissingGuard",
        ),
        (
            "batch guard",
            "cas",
            "_global_read_budget: GlobalCasBatchReadBudgetGuard",
            "_global_read_budget: MissingGuard",
        ),
        ("backlog status", "backlog", "status: done", "status: open"),
        (
            "OKF render",
            "okf",
            "CAS_READ_GLOBAL_BUDGET_BYTES",
            "CAS_READ_GLOBAL_BUDGET_REMOVED",
        ),
    )
    for name, key, old, new in mutants:
        if isinstance(old, tuple):
            fixture_present = any(needle in files[key] for needle in old)
        else:
            fixture_present = old in files[key]
        if not fixture_present:
            fail(f"mutation fixture for {name} did not match the source")
        mutant = dict(files)
        if name == "backlog status":
            mutant[key] = re.sub(
                r"(### B-056 .*?\n```backlog\n.*?status: )done",
                rf"\g<1>{new.removeprefix('status: ') if new.startswith('status: ') else new}",
                files[key],
                count=1,
                flags=re.S,
            )
        else:
            old_needles = old if isinstance(old, tuple) else (old,)
            old_needle = next((needle for needle in old_needles if needle in files[key]), None)
            if old_needle is None:
                fail(f"mutation fixture for {name} did not match the source")
            new_needle = new[old_needles.index(old_needle)] if isinstance(new, tuple) else new
            count = -1 if name == "OKF render" else 1
            mutant[key] = files[key].replace(old_needle, new_needle, count)
        try:
            assess(mutant)
        except VerificationError:
            continue
        fail(f"mutation unexpectedly passed: {name}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--expect", default="done", choices=("open", "done"))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    files = source()
    assess(files, args.expect)
    if args.self_test:
        mutation_checks(files)
    print("B-056 done: process-wide weighted CAS read budget; mutations red")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except VerificationError as error:
        raise SystemExit(f"FAIL: {error}")
