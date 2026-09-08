#!/usr/bin/env python3
"""Verify the B-077 shared deployed-container capacity contract."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CAPACITY = ROOT / "crates/corelink-container/src/container_capacity.rs"
LIB = ROOT / "crates/corelink-container/src/lib.rs"
CAS = ROOT / "crates/corelink-container/src/routes/cas.rs"
CAS_ERASE = ROOT / "crates/corelink-container/src/routes/cas_erase.rs"
R2_S3 = ROOT / "crates/corelink-container/src/storage/r2_s3.rs"
TURBO = ROOT / "crates/corelink-container/src/routes/turbo_v8.rs"
PAT = ROOT / "crates/corelink-container/src/adapter_pat.rs"
MAIN = ROOT / "crates/corelink-container/src/main.rs"
AUTH = ROOT / "crates/corelink-container/src/auth_tenant.rs"

# The executable CAS route is assembled with include!() from these source
# units. Keep this list aligned with cas.rs so the verifier reads the live
# implementation rather than historical pre-split modules.
CAS_ROUTE_PARTS = (
    "crates/corelink-container/src/routes/cas.rs",
    "crates/corelink-container/src/routes/cas/foundation_core.rs",
    "crates/corelink-container/src/routes/cas/foundation_state.rs",
    "crates/corelink-container/src/routes/cas/single_setup.rs",
    "crates/corelink-container/src/routes/cas/single_handlers.rs",
    "crates/corelink-container/src/routes/cas/batch_write.rs",
    "crates/corelink-container/src/routes/cas/batch_read.rs",
    "crates/corelink-container/src/routes/cas/list_delete.rs",
)


def const_int(source: str, name: str, env: dict[str, int] | None = None) -> int:
    match = re.search(rf"pub const {name}: u64 =\s*([^;]+);", source)
    if not match:
        raise AssertionError(f"missing u64 constant {name}")
    expr = re.sub(r"\s+", "", match.group(1))
    names = {**(env or {})}
    for dependency in re.findall(r"[A-Z][A-Z0-9_]+", expr):
        names.setdefault(dependency, const_int(source, dependency, names))
    if not re.fullmatch(r"[0-9*+/ ()A-Z_]+", expr):
        raise AssertionError(f"{name} contains unsupported expression syntax")
    return eval(expr, {"__builtins__": {}}, names)


def main() -> int:
    r2_parts = (
        "crates/corelink-container/src/storage/r2_s3.rs",
        "crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs",
        "crates/corelink-container/src/storage/r2_s3_parts/cas_ops.rs",
    )
    turbo_parts = (
        "crates/corelink-container/src/routes/turbo_v8.rs",
        "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs",
        "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs",
    )
    erase_parts = (
        "crates/corelink-container/src/routes/cas_erase.rs",
        "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_01.rs",
        "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02.rs",
    )
    pat_parts = (
        "crates/corelink-container/src/adapter_pat.rs",
        "crates/corelink-container/src/adapter_pat_crypto.rs",
        "crates/corelink-container/src/adapter_pat_gate.rs",
        "crates/corelink-container/src/adapter_pat_lookup.rs",
        "crates/corelink-container/src/adapter_pat_verifier.rs",
    )
    sources = {
        p: p.read_text(encoding="utf-8")
        for p in (CAPACITY, LIB, CAS_ERASE, TURBO, PAT, MAIN, AUTH)
    }
    sources[CAS] = "\n".join(
        (ROOT / p).read_text(encoding="utf-8") for p in CAS_ROUTE_PARTS
    )
    sources[R2_S3] = "\n".join((ROOT / p).read_text(encoding="utf-8") for p in r2_parts)
    sources[TURBO] = "\n".join((ROOT / p).read_text(encoding="utf-8") for p in turbo_parts)
    sources[CAS_ERASE] = "\n".join((ROOT / p).read_text(encoding="utf-8") for p in erase_parts)
    sources[PAT] = "\n".join((ROOT / p).read_text(encoding="utf-8") for p in pat_parts)
    cap = sources[CAPACITY]
    assert "pub mod container_capacity;" in sources[LIB]
    assert "validate_runtime_budget()" in sources[MAIN]
    assert "container_capacity::CONTAINER_MEMORY_BYTES" in sources[CAS]
    assert "container_capacity::TURBO_PUT_GLOBAL_PERMITS" in sources[TURBO]
    assert "container_capacity::TURBO_GET_GLOBAL_PERMITS" in sources[TURBO]
    assert "container_capacity::ARGON2_VERIFY_PERMITS" in sources[PAT]
    assert "CAS_READ_COPY_MULTIPLIER" in sources[CAS]
    assert "CAS_READ_BATCH_BODY_LIMIT_BYTES" in sources[CAPACITY]
    assert "CAS_READ_BATCH_PEAK_BYTES" in sources[CAPACITY]
    assert "CAS_BATCH_PARSE_METADATA_BYTES" in sources[CAPACITY]
    assert "BATCH_MAX_LINE_BYTES" in sources[CAS]
    assert "BATCH_MAX_HASH_BYTES" in sources[CAS]
    assert "BATCH_METADATA_WORST_CASE_BYTES" in sources[CAS]
    assert "let _read_slot = _read_concurrency;" in sources[CAS]
    assert ".checked_add(" in sources[CAS]
    assert sources[CAS].count("DefaultBodyLimit::max") >= 3
    assert "MAX_TENANT_ID_BYTES" in sources[AUTH]
    assert "GlobalCasWriteBudgetGuard" in sources[CAS]
    assert "GlobalCasBatchWriteBudgetGuard" in sources[CAS]
    assert sources[CAS].count("_global_read_budget: GlobalCasBatchReadBudgetGuard") >= 2
    assert "CAS_WRITE_GLOBAL_PERMITS" in sources[CAS]
    assert "StreamBody::new(body_stream)" in sources[CAS]
    assert "Frame::data(axum::body::Bytes::from(resp.bytes))" in sources[CAS]
    assert "BLOOM_CACHE_BIT_ARRAY_BYTES" in sources[CAS_ERASE]
    assert "bits.min(DEFAULT_BLOOM_BITS)" in sources[CAS_ERASE]
    assert "max_tenants.clamp(1, DEFAULT_MAX_TENANT_BLOOMS)" in sources[CAS_ERASE]
    assert "byok_read_peak_is_bounded" in cap
    assert_capped_read_paths(sources[R2_S3])
    mutation_self_test(sources[R2_S3])

    memory = const_int(cap, "CONTAINER_MEMORY_BYTES")
    assert memory == 1024 * 1024 * 1024
    assert const_int(cap, "CONTAINER_VCPU_MILLICORES") == 250
    reserve = const_int(cap, "RUNTIME_MEMORY_RESERVE_BYTES")
    declared = const_int(cap, "DECLARED_MEMORY_BUDGET_BYTES")
    assert reserve > 0 and declared + reserve <= memory
    expected = sum(
        const_int(cap, name)
        for name in (
            "CAS_READ_GLOBAL_BUDGET_BYTES",
            "CAS_WRITE_GLOBAL_BUDGET_BYTES",
            "ARGON2_MEMORY_BUDGET_BYTES",
            "TURBO_PUT_MEMORY_BUDGET_BYTES",
            "TURBO_GET_MEMORY_BUDGET_BYTES",
            "TURBO_EVENTS_MEMORY_BUDGET_BYTES",
            "BLOOM_CACHE_MEMORY_BUDGET_BYTES",
        )
    )
    assert declared == expected
    read_global = const_int(cap, "CAS_READ_GLOBAL_BUDGET_BYTES")
    read_single_peak = const_int(cap, "CAS_READ_SINGLE_PEAK_BYTES")
    read_batch_peak = const_int(cap, "CAS_READ_BATCH_PEAK_BYTES")
    read_batch_body = const_int(cap, "CAS_READ_BATCH_BODY_LIMIT_BYTES")
    parse_metadata = const_int(cap, "CAS_BATCH_PARSE_METADATA_BYTES")
    response_cap = const_int(cap, "CAS_WRITE_BATCH_PAYLOAD_LIMIT_BYTES")
    assert read_batch_peak == read_batch_body + parse_metadata + response_cap
    assert read_single_peak == (
        const_int(cap, "CAS_READ_COPY_MULTIPLIER") * 64 * 1024 * 1024
        + parse_metadata
    )
    assert read_single_peak + read_batch_peak <= read_global
    write_global = const_int(cap, "CAS_WRITE_GLOBAL_BUDGET_BYTES")
    write_single_peak = const_int(cap, "CAS_WRITE_SINGLE_PEAK_BYTES")
    write_batch_peak = const_int(cap, "CAS_WRITE_BATCH_PEAK_BYTES")
    assert write_global == 38 * 1024 * 1024
    assert write_single_peak <= write_global
    assert write_batch_peak <= write_global
    assert write_batch_peak == read_batch_body + response_cap + parse_metadata
    assert const_int(cap, "BLOOM_CACHE_BIT_ARRAY_BYTES") == 2048 * 128 * 1024
    assert "budget_fits(" in cap and "over_budget_mutation_is_rejected" in cap

    stale = re.compile(r"standard-1|~4\s*GiB|0\.5\s*vCPU", re.IGNORECASE)
    for path in (CAS, CAS_ERASE, TURBO, PAT):
        assert not stale.search(sources[path]), f"stale capacity claim in {path}"

    print(
        f"B-077 verified: memory={memory} vCPU_millicores=250 "
        f"declared={declared} reserve={reserve}"
    )
    return 0


def concurrent_read_block(source: str) -> str:
    marker = "// CONCURRENT PATH (production: durable D1 audit sink wired)."
    start = source.find(marker)
    if start < 0:
        raise AssertionError("production concurrent CAS read path is missing")
    end = source.find("// ONE `Phase::Store` scope", start)
    if end < 0:
        raise AssertionError("concurrent CAS read path boundary is missing")
    return source[start:end]


def assert_capped_read_paths(source: str) -> None:
    """Require both CAS read branches to share the pre-materialisation cap."""
    concurrent = concurrent_read_block(source)
    if "get_capped(key, max_bytes)" not in source:
        raise AssertionError("CAS read helper does not propagate the request byte ceiling")
    if "self.get_capped_for_read(&key, max_bytes)" not in concurrent:
        raise AssertionError("concurrent CAS read path does not use the capped helper")
    if "self.client.get(&key)" in concurrent:
        raise AssertionError("concurrent CAS read path bypasses the capped helper")
    if source.count("self.get_capped_for_read(&key, max_bytes)") != 2:
        raise AssertionError("CAS concurrent and serial paths must each call the capped helper")


def mutation_self_test(source: str) -> None:
    """Prove reintroducing an uncapped concurrent GET turns the check red."""
    assert_capped_read_paths(source)
    mutant = source.replace(
        "self.get_capped_for_read(&key, max_bytes)",
        "self.client.get(&key)",
        1,
    )
    if mutant == source:
        raise AssertionError("could not construct the uncapped concurrent-read mutation")
    try:
        assert_capped_read_paths(mutant)
    except AssertionError:
        return
    raise AssertionError("uncapped concurrent-read mutation was not detected")


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError) as exc:
        print(f"B-077 FAILED: {exc}", file=sys.stderr)
        raise SystemExit(1)
