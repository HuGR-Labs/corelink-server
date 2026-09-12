#!/usr/bin/env python3
"""Fail-closed B-126-M2 split and G4b fixture wiring guard.

The M2 roots are deliberately thin ``include!`` composition points.  This
guard keeps their population closed and checks the source which is actually
included, rather than trusting a marker or a Rust test that may have stopped
being wired.  It is intentionally stdlib-only and never invokes Cargo, CI,
the network, or a baseline checkout.
"""

from __future__ import annotations

import argparse
import functools
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MAX_LINES = 1000

# The six M2 roots and their direct include order are a closed contract.  A
# derived glob is checked separately below, so a new or renamed fragment cannot
# silently become part of the measured population.
ROOT_INCLUDES: dict[str, tuple[str, ...]] = {
    "crates/corelink-container/src/byte_accounting.rs": (
        "byte_accounting/b126_m2_impl_01.rs",
        "byte_accounting/b126_m2_impl_02.rs",
    ),
    "crates/corelink-container/src/routes/audit_drain.rs": (
        "audit_drain/b054_witness.rs",
        "audit_drain/b054_epoch_admin.rs",
        "audit_drain/b126_m2_impl_01.rs",
        "audit_drain/b126_m2_impl_01_part2.rs",
        "audit_drain/b126_m2_impl_02.rs",
        "audit_drain/b126_m2_impl_02_part2.rs",
        "audit_drain/b126_m2_impl_03.rs",
    ),
    "crates/corelink-container/src/routes/cas_erase.rs": (
        "cas_erase/b126_m2_impl_01.rs",
        "cas_erase/b126_m2_impl_01_part2.rs",
        "cas_erase/b126_m2_impl_02.rs",
        "cas_erase/b126_m2_impl_02_part2.rs",
    ),
    "crates/corelink-container/src/routes/oci.rs": (
        "oci/b126_m2_impl_01.rs",
        "oci/b126_m2_impl_01_part2.rs",
        "oci/b126_m2_impl_02.rs",
    ),
    "crates/corelink-container/src/routes/turbo_v8.rs": (
        "turbo_v8/b126_m2_impl_01.rs",
        "turbo_v8/b126_m2_impl_01_part2.rs",
        "turbo_v8/b126_m2_impl_02.rs",
        "turbo_v8/b126_m2_impl_02_part2.rs",
    ),
    "crates/corelink-container/src/tenant_quota.rs": (
        "tenant_quota/b126_m2_impl_01.rs",
        "tenant_quota/b126_m2_impl_02.rs",
    ),
}

# Complete transitive include order for each root.  Keeping this list explicit
# catches a removed nested test include even when the file remains on disk.
ROOT_FRAGMENTS: dict[str, tuple[str, ...]] = {
    "crates/corelink-container/src/byte_accounting.rs": (
        "crates/corelink-container/src/byte_accounting/b126_m2_impl_01.rs",
        "crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs",
        "crates/corelink-container/src/byte_accounting/b126_m2_impl_02.rs",
        "crates/corelink-container/src/byte_accounting/b126_m2_test_1_1.rs",
        "crates/corelink-container/src/byte_accounting/b126_m2_test_2_1.rs",
        "crates/corelink-container/src/byte_accounting/b126_m2_test_3_1.rs",
        "crates/corelink-container/src/byte_accounting/b126_m2_test_3_1_part_02.rs",
        "crates/corelink-container/src/byte_accounting/b126_m2_test_4_1.rs",
    ),
    "crates/corelink-container/src/routes/audit_drain.rs": (
        "crates/corelink-container/src/routes/audit_drain/b054_witness.rs",
        "crates/corelink-container/src/routes/audit_drain/b054_witness_runtime.rs",
        "crates/corelink-container/src/routes/audit_drain/b054_witness_tests.rs",
        "crates/corelink-container/src/routes/audit_drain/b054_epoch_admin.rs",
        "crates/corelink-container/src/routes/audit_drain/b054_epoch_admin_authorities.rs",
        "crates/corelink-container/src/routes/audit_drain/b054_epoch_admin_part2.rs",
        "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs",
        "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01_part2.rs",
        "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02.rs",
        "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part3.rs",
        "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part2.rs",
        "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_03.rs",
        "crates/corelink-container/src/routes/audit_drain/b126_m2_test_1_1.rs",
        "crates/corelink-container/src/routes/audit_drain/b126_m2_test_1_1_part2.rs",
        "crates/corelink-container/src/routes/audit_drain/b126_m2_test_1_2.rs",
    ),
    "crates/corelink-container/src/routes/cas_erase.rs": (
        "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_01.rs",
        "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_01_part2.rs",
        "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02.rs",
        "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02_part2.rs",
        "crates/corelink-container/src/routes/cas_erase/b126_m2_test_1_1.rs",
        "crates/corelink-container/src/routes/cas_erase/b126_m2_test_1_1_part2.rs",
        "crates/corelink-container/src/routes/cas_erase/b126_m2_test_1_2.rs",
    ),
    "crates/corelink-container/src/routes/oci.rs": (
        "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs",
        "crates/corelink-container/src/routes/oci/b126_m2_impl_01_part2.rs",
        "crates/corelink-container/src/routes/oci/b126_m2_impl_02.rs",
        "crates/corelink-container/src/routes/oci/b126_m2_test_1_1.rs",
        "crates/corelink-container/src/routes/oci/b126_m2_test_1_1_part2.rs",
        "crates/corelink-container/src/routes/oci/b126_m2_test_1_2.rs",
        "crates/corelink-container/src/routes/oci/b126_m2_test_1_2_part2.rs",
        "crates/corelink-container/src/routes/oci/b126_m2_test_1_3.rs",
    ),
    "crates/corelink-container/src/routes/turbo_v8.rs": (
        "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs",
        "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01_part2.rs",
        "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs",
        "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02_part2.rs",
        "crates/corelink-container/src/routes/turbo_v8/b126_m2_test_1_1.rs",
        "crates/corelink-container/src/routes/turbo_v8/b126_m2_test_1_1_part2.rs",
        "crates/corelink-container/src/routes/turbo_v8/b126_m2_test_1_2.rs",
        "crates/corelink-container/src/routes/turbo_v8/b126_m2_test_1_2_part2.rs",
    ),
    "crates/corelink-container/src/tenant_quota.rs": (
        "crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs",
        "crates/corelink-container/src/tenant_quota/b126_m2_impl_01_part_02.rs",
        "crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs",
        "crates/corelink-container/src/tenant_quota/b126_m2_impl_02_part_02.rs",
        "crates/corelink-container/src/tenant_quota/b126_m2_test_1_1.rs",
        "crates/corelink-container/src/tenant_quota/b126_m2_test_1_1_part_02.rs",
        "crates/corelink-container/src/tenant_quota/b126_m2_test_1_2.rs",
    ),
}

# Symbols named by the six root manifests.  These are checked in the masked,
# expanded source so a name in a comment/string cannot satisfy API parity.
API_SYMBOLS: dict[str, tuple[str, ...]] = {
    "crates/corelink-container/src/byte_accounting.rs": (
        "region_from_env", "STORAGE_QUOTA_HEADER", "storage_quota_from_headers",
        "AccrueOutcome", "ByteStore", "ByteAccountant", "current_unix_ms",
        "byte_accountant_from_env", "D1ByteStore", "OVER_CAP_SENTINEL",
        "ACCT_UNAVAILABLE_SENTINEL", "block_on_accrue", "byok_committed_len",
        "block_on_release", "CAS_LOCK_SHARDS", "AccountingCasHandler",
        "AccountingAcHandler",
    ),
    "crates/corelink-container/src/routes/audit_drain.rs": (
        "INTERNAL_AUTH_HEADER", "AuditDrainState", "load_signing_seed", "resolve_seed",
        "signing_key_id_from_env", "resolve_key_id", "region_for_key", "CanonicalAuditHead",
        "CanonicalAuditHeadV2", "canonical_head_bytes", "canonical_head_v2_bytes", "sign_head",
        "sign_head_v2", "verify_head", "verify_head_v2", "internal_auth_ok",
        "B054EpochAdminRequest", "handle_epoch_admin", "build_state_from_env", "router",
        "now_ms", "AUDIT_DRAIN_LEASE_TTL_MS",
        "new_lease_holder", "should_fence", "FencedSeal", "run_chunked_fenced_seal_loop",
        "acquire_lease", "release_lease", "chain_hash_from_hex", "checkpoint_nullable_u64",
        "checkpoint_nullable_text", "parse_sealed_epoch_metadata", "HeadCheckpoint", "SealedRow",
        "PartitionOutcome", "seal_rows", "seal_rows_for_epoch", "resolve_resume", "HeadResumeCheck",
        "check_head_on_resume", "reject_unwired_epoch_checkpoint", "read_pending_partitions",
        "read_unsigned_head_partitions", "read_checkpoint", "read_sealed_tail", "read_pending_rows",
        "write_seal_chunk", "advance_head_cas", "resign_unsigned_head", "converge_unsigned_heads",
        "drain_partition", "drain_partition_inner", "handle_drain", "DrainOutcome",
        "build_drain_response_body",
    ),
    "crates/corelink-container/src/routes/cas_erase.rs": (
        "INTERNAL_AUTH_HEADER", "CAS_ERASE_ROUTE", "MAX_REASON_LEN", "TombstoneStore",
        "CasBlobEraser", "CasEraseRouteState", "router", "EraseBody", "handle_erase",
        "internal_auth_ok", "now_unix_ms", "D1TombstoneStore", "DEFAULT_BLOOM_BITS",
        "DEFAULT_BLOOM_HASHES", "DEFAULT_BLOOM_REFRESH", "DEFAULT_MAX_TENANT_BLOOMS",
        "MAX_BLOOM_TENANT_ID_BYTES", "MAX_BLOOM_ENTRY_METADATA_BYTES", "Bloom", "TenantBloom",
        "TenantBloomEntry", "BloomTombstoneStore", "TOMBSTONE_GONE_SENTINEL",
        "TOMBSTONE_UNAVAILABLE_SENTINEL", "TombstoneGatedCasHandler", "InMemoryTombstoneStore",
        "lock_or_recover", "InMemoryBlobEraser", "DEFAULT_CAS_BUCKET", "TENANT_PREFIX_LEN",
        "R2CasBlobEraser", "build_state_from_env", "load_tdk_from_env",
    ),
    "crates/corelink-container/src/routes/oci.rs": (
        "OCI_SERVICE_PRINCIPAL", "OCI_MAX_OPEN_SESSIONS_PER_TENANT", "OCI_MAX_INFLIGHT_BYTES",
        "OCI_MAX_INFLIGHT_BYTES_PER_TENANT", "OCI_SESSION_IDLE_TIMEOUT_MS", "OCI_BEARER_REALM",
        "OCI_TOKEN_KEY_ENV", "OCI_TOKEN_KEY_ENV_LEGACY", "UploadSession", "OciMoatStore",
        "now_unix_ms", "upload_uuid_belongs_to", "OciPatResolver", "router", "OciCostGate",
        "oci_bearer_tenant", "oci_quota_gate",
    ),
    "crates/corelink-container/src/routes/turbo_v8.rs": (
        "TURBO_GET_ROUTE", "TURBO_PUT_ROUTE", "TURBO_EVENTS_ROUTE", "TURBO_STATUS_ROUTE",
        "ARTIFACT_TAG_HEADER", "TURBO_BODY_LIMIT_BYTES", "TURBO_PUT_CONCURRENCY_LIMIT",
        "TURBO_GET_CONCURRENCY_LIMIT", "EVENTS_BODY_LIMIT_BYTES", "GLOBAL_TURBO_PUT_PERMITS",
        "GLOBAL_TURBO_GET_PERMITS", "GLOBAL_TURBO_EVENTS_PERMITS", "EVENTS_CONCURRENCY_LIMIT",
        "EVENTS_BODY_READ_TIMEOUT", "GLOBAL_PUT_PERMIT_WAIT", "TURBO_WRITE_LOCK_SHARDS",
        "ArtifactQuery", "PutArtifactResponse", "TurboRouteState", "global_turbo_put_budget",
        "global_turbo_get_budget", "global_turbo_events_budget", "new_write_locks",
        "write_lock_shard", "PutSlot", "PutConcurrencyGuard", "GlobalPutBudgetGuard", "GetSlot",
        "GetConcurrencyGuard", "GlobalGetBudgetGuard", "EventsBudgetGuard", "EventsSlot",
        "EventsConcurrencyGuard", "build_handlers", "router", "handle_get", "handle_put",
        "handle_events", "handle_status", "TURBO_STORAGE_UNAVAILABLE_SENTINEL",
        "UnavailableTurboHandler", "map_err",
    ),
    "crates/corelink-container/src/tenant_quota.rs": (
        "NEAR_CEILING_RUNBOOK_URL", "NEAR_CEILING_ALERT_SINK_ENV", "DEFAULT_MONTHLY_BUDGET_USD_MICROS",
        "CYCLE_LENGTH_MS", "COST_PER_OP_MICROS_ENV", "DEFAULT_COST_PER_OP_MICROS",
        "cost_per_op_micros", "quota_guard_from_env", "QuotaState", "QuotaStore", "DEFAULT_LEASE_OPS",
        "LeasedQuotaStore", "Lease", "build_near_ceiling_event", "QuotaGuard", "InMemoryQuotaStore",
        "D1QuotaStore",
    ),
}

G4B_FRAGMENT = "crates/corelink-container/src/routes/oci/b126_m2_test_1_3.rs"
G4B_FIXTURE = "crates/corelink-container/src/routes/oci/tests/oci_g4b_tests.rs"
AUDIT_DRAIN_ROOT = "crates/corelink-container/src/routes/audit_drain.rs"
AUDIT_DUPLICATE_TAIL_DEFINITION = "fn reject_duplicate_sealed_tail("
AUDIT_DUPLICATE_TAIL_CALL = "reject_duplicate_sealed_tail(&rows)?;"
AUDIT_SEALED_TAIL_READER = "async fn read_any_sealed_tail("
G4B_TESTS = (
    "g4b_suspended_tenant_token_leg_denies_no_bearer",
    "g4b_suspended_tenant_v2_leg_is_403",
    "g4b_active_tenant_passes_token_and_v2",
    "g4b_mid_session_suspend_v2_leg_is_403_within_bearer_ttl",
    "g4b_fail_closed_known_suspended_read_error_still_denies_v2",
)


class BoundaryError(ValueError):
    """The source population or its wiring cannot be trusted."""


INCLUDE_RE = re.compile(r'^\s*include!\s*\("([^"\n]+)"\);\s*$', re.MULTILINE)
INCLUDE_TOKEN_RE = re.compile(r'\binclude!\s*\(')
PATH_RE = re.compile(
    r'^\s*#\[path\s*=\s*"([^"]+)"\]\s*\n\s*mod\s+([A-Za-z_]\w*)\s*;\s*$',
    re.MULTILINE,
)
PATH_TOKEN_RE = re.compile(r'^\s*#\[path\b', re.MULTILINE)
TEST_RE = re.compile(
    r'^\s*#\[tokio::test(?:\s*=\s*[^\]]+)?\]\s*\n\s*async\s+fn\s+([A-Za-z_]\w*)\s*\([^)]*\)\s*\{',
    re.MULTILINE,
)
TEST_TOKEN_RE = re.compile(r'^\s*#\[tokio::test\b', re.MULTILINE)
RAW_START_RE = re.compile(r'r(#+)?"')


def _read(path: Path, overrides: dict[Path, str] | None = None) -> str:
    if overrides and path in overrides:
        return overrides[path]
    if path.is_symlink() or not path.is_file():
        raise BoundaryError(f"missing or non-regular source: {path}")
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise BoundaryError(f"cannot read source: {path}: {exc}") from exc


@functools.lru_cache(maxsize=256)
def _mask_non_code(text: str) -> str:
    """Blank comments and literals while preserving offsets and newlines."""
    out = list(text)
    i = 0
    state = "normal"
    block_depth = 0
    raw_hashes = 0
    while i < len(text):
        if state == "normal":
            if text.startswith("//", i):
                out[i] = out[i + 1] = " "
                i += 2
                state = "line"
                continue
            if text.startswith("/*", i):
                out[i] = out[i + 1] = " "
                i += 2
                block_depth = 1
                state = "block"
                continue
            raw = RAW_START_RE.match(text, i)
            if raw:
                raw_hashes = len(raw.group(1) or "")
                for j in range(i, i + len(raw.group(0))):
                    if text[j] != "\n":
                        out[j] = " "
                i += len(raw.group(0))
                state = "raw"
                continue
            if text[i] == '"':
                out[i] = " "
                i += 1
                state = "string"
                continue
            i += 1
            continue
        if state == "line":
            if text[i] == "\n":
                state = "normal"
            else:
                out[i] = " "
            i += 1
            continue
        if state == "block":
            if text.startswith("/*", i):
                out[i] = out[i + 1] = " "
                block_depth += 1
                i += 2
            elif text.startswith("*/", i):
                out[i] = out[i + 1] = " "
                block_depth -= 1
                i += 2
                if block_depth == 0:
                    state = "normal"
            else:
                if text[i] != "\n":
                    out[i] = " "
                i += 1
            continue
        if state == "string":
            if text[i] == "\\":
                out[i] = " "
                if i + 1 < len(text):
                    if text[i + 1] != "\n":
                        out[i + 1] = " "
                    i += 2
                else:
                    i += 1
            elif text[i] == '"':
                out[i] = " "
                i += 1
                state = "normal"
            else:
                if text[i] != "\n":
                    out[i] = " "
                i += 1
            continue
        # raw string
        closing = '"' + ("#" * raw_hashes)
        if text.startswith(closing, i):
            for j in range(i, i + len(closing)):
                out[j] = " "
            i += len(closing)
            state = "normal"
        else:
            if text[i] != "\n":
                out[i] = " "
            i += 1
    return "".join(out)


@functools.lru_cache(maxsize=256)
def _strip_comments(text: str) -> str:
    """Blank comments but retain string literals for include/path matching."""
    # Recovering strings is unnecessary for the current source and would make
    # the parser larger; include/path regexes are line-anchored, so use the
    # original text with comment-only lines removed by a second small scanner.
    out = list(text)
    i = 0
    state = "normal"
    depth = 0
    while i < len(text):
        if state == "normal" and text.startswith("//", i):
            out[i] = out[i + 1] = " "
            i += 2
            state = "line"
        elif state == "normal" and text.startswith("/*", i):
            out[i] = out[i + 1] = " "
            i += 2
            depth = 1
            state = "block"
        elif state == "line":
            if text[i] == "\n":
                state = "normal"
            else:
                out[i] = " "
            i += 1
        elif state == "block":
            if text.startswith("/*", i):
                out[i] = out[i + 1] = " "
                depth += 1
                i += 2
            elif text.startswith("*/", i):
                out[i] = out[i + 1] = " "
                depth -= 1
                i += 2
                if depth == 0:
                    state = "normal"
            else:
                if text[i] != "\n":
                    out[i] = " "
                i += 1
        else:
            # Strings are intentionally opaque to comment recognition.
            if text[i] == "\\" and i + 1 < len(text):
                i += 2
            elif text[i] == '"':
                i += 1
                while i < len(text):
                    if text[i] == "\\":
                        i += 2
                    elif text[i] == '"':
                        i += 1
                        break
                    else:
                        i += 1
            else:
                i += 1
    return "".join(out)


def _relative(path: Path, root: Path = ROOT) -> str:
    return path.relative_to(root).as_posix()


def _regular(path: Path, root: Path = ROOT) -> None:
    if path.is_symlink() or not path.is_file():
        raise BoundaryError(f"missing or non-regular source: {_relative(path, root)}")


def _include_paths(path: Path, text: str, root: Path = ROOT) -> list[str]:
    visible = _strip_comments(text)
    tokens = len(INCLUDE_TOKEN_RE.findall(_mask_non_code(text)))
    matches = list(INCLUDE_RE.finditer(visible))
    if tokens != len(matches):
        raise BoundaryError(f"{_relative(path, root)}: malformed or non-line include! directive")
    return [match.group(1) for match in matches]


def _expand(
    path: Path,
    overrides: dict[Path, str],
    active: tuple[Path, ...] = (),
    root_paths: frozenset[Path] = frozenset(),
    display_root: Path = ROOT,
) -> tuple[str, list[Path]]:
    if path in active:
        raise BoundaryError(f"include cycle at {_relative(path, display_root)}")
    _regular(path, display_root)
    text = _read(path, overrides)
    relative_includes = _include_paths(path, text, display_root)
    pieces: list[str] = []
    descendants: list[Path] = []
    cursor = 0
    visible = _strip_comments(text)
    matches = list(INCLUDE_RE.finditer(visible))
    for match, relative in zip(matches, relative_includes, strict=True):
        rel = Path(relative)
        if rel.is_absolute() or ".." in rel.parts or rel.suffix != ".rs":
            raise BoundaryError(f"{_relative(path, display_root)}: include escapes source: {relative}")
        child = path.parent / rel
        expected_parent = path.with_suffix("") if path in root_paths else path.parent
        if child.parent != expected_parent:
            raise BoundaryError(f"{_relative(path, display_root)}: include must stay beside source: {relative}")
        child_text, child_descendants = _expand(
            child, overrides, (*active, path), root_paths, display_root
        )
        pieces.extend((text[cursor : match.start()], child_text))
        descendants.extend((child, *child_descendants))
        cursor = match.end()
    pieces.append(text[cursor:])
    return "".join(pieces), descendants


def _fragment_reanchor(
    path: Path,
    root: Path = ROOT,
) -> str:
    name = path.stem
    impl = re.fullmatch(r"b126_m2_impl_(\d+)(?:_part_?\d+)?", name)
    test = re.fullmatch(r"b126_m2_test_(\d+)_(\d+)(?:_part_?\d+)?", name)
    if impl:
        expected = f"B126_M2_IMPL_{int(impl.group(1))}_REANCHOR"
    elif test:
        expected = f"B126_M2_TEST_{test.group(1)}_{test.group(2)}_REANCHOR"
    else:
        raise BoundaryError(f"unexpected M2 fragment name: {_relative(path, root)}")
    return expected


def _rust_function_body_span(code: str, marker: str) -> tuple[int, int]:
    """Return the code span for one simple Rust function in masked source."""
    start = code.find(marker)
    if start < 0:
        raise BoundaryError(f"Rust function marker missing: {marker}")
    opening = code.find("{", start + len(marker))
    if opening < 0:
        raise BoundaryError(f"Rust function body missing: {marker}")
    depth = 0
    for position in range(opening, len(code)):
        if code[position] == "{":
            depth += 1
        elif code[position] == "}":
            depth -= 1
            if depth == 0:
                return start, position + 1
    raise BoundaryError(f"Rust function body is unterminated: {marker}")


def audit_drain_duplicate_tail_guard_report(
    root: Path = ROOT, overrides: dict[Path, str] | None = None
) -> dict[str, tuple[str, ...]]:
    """Check the duplicate-tail guard against the declared include chain.

    The guard is allowed to move between declared B126 fragments as the
    implementation is split, but it cannot disappear into a comment/string,
    move into the root/non-B126 source, or be duplicated.  The call must remain
    inside the actual ``read_any_sealed_tail`` body in the expanded composition.
    """
    expected = ROOT_FRAGMENTS[AUDIT_DRAIN_ROOT]
    if len(expected) != len(set(expected)):
        raise BoundaryError("audit-drain fragment manifest contains duplicates")
    root_path = root / AUDIT_DRAIN_ROOT
    root_paths = frozenset(root / item for item in ROOT_INCLUDES)
    expanded, included = _expand(
        root_path,
        overrides or {},
        root_paths=root_paths,
        display_root=root,
    )
    actual = tuple(_relative(path, root) for path in included)
    if actual != expected:
        raise BoundaryError("audit-drain duplicate-tail guard composition changed")

    definition_fragments: list[str] = []
    for relative in expected:
        path = root / relative
        fragment = _mask_non_code(_read(path, overrides))
        count = fragment.count(AUDIT_DUPLICATE_TAIL_DEFINITION)
        definition_fragments.extend([relative] * count)
    expanded_code = _mask_non_code(expanded)
    if expanded_code.count(AUDIT_DUPLICATE_TAIL_DEFINITION) != 1:
        raise BoundaryError("audit-drain duplicate-tail guard definition is not unique")
    if len(definition_fragments) != 1 or not definition_fragments[0].startswith(
        "crates/corelink-container/src/routes/audit_drain/b126_m2_"
    ):
        raise BoundaryError("audit-drain duplicate-tail guard is outside the B126 manifest")

    call_count = expanded_code.count(AUDIT_DUPLICATE_TAIL_CALL)
    if call_count != 1:
        raise BoundaryError("audit-drain duplicate-tail guard call is not unique")
    reader_count = expanded_code.count(AUDIT_SEALED_TAIL_READER)
    if reader_count != 1:
        raise BoundaryError(
            "audit-drain v2 sealed-tail reader is not unique"
        )
    reader_start = expanded_code.find(AUDIT_SEALED_TAIL_READER)
    if reader_start < 0:
        raise BoundaryError("audit-drain v2 sealed-tail reader is missing")
    _, reader_end = _rust_function_body_span(
        expanded_code, AUDIT_SEALED_TAIL_READER
    )
    call_position = expanded_code.find(AUDIT_DUPLICATE_TAIL_CALL)
    if not reader_start < call_position < reader_end:
        raise BoundaryError("audit-drain duplicate-tail guard is not wired in v2 reader")
    return {
        "definition_fragments": tuple(definition_fragments),
        "call_fragments": tuple(
            relative
            for relative in expected
            if AUDIT_DUPLICATE_TAIL_CALL
            in _mask_non_code(_read(root / relative, overrides))
        ),
    }


def _validate_g4b(fixture: str, wiring_fragment: str) -> None:
    wiring_visible = _strip_comments(wiring_fragment)
    paths = list(PATH_RE.finditer(wiring_visible))
    if len(list(PATH_TOKEN_RE.finditer(wiring_visible))) != 1 or len(paths) != 1:
        raise BoundaryError("G4b fixture wiring must have exactly one path-loaded module")
    if paths[0].groups() != ("tests/oci_g4b_tests.rs", "g4b_tests"):
        raise BoundaryError("G4b fixture path/module identity changed")

    code = _mask_non_code(fixture)
    tests = list(TEST_RE.finditer(code))
    if len(list(TEST_TOKEN_RE.finditer(code))) != len(tests):
        raise BoundaryError("G4b test attribute is malformed")
    names = [match.group(1) for match in tests]
    if len(names) != len(set(names)) or tuple(names) != G4B_TESTS:
        raise BoundaryError(f"G4b test population changed: {names!r}")


def verify(root: Path = ROOT, overrides: dict[Path, str] | None = None) -> dict[str, int]:
    if tuple(ROOT_INCLUDES) != tuple(ROOT_FRAGMENTS) or tuple(ROOT_INCLUDES) != tuple(API_SYMBOLS):
        raise BoundaryError("internal manifest roots are not identical")

    expected_global: list[Path] = []
    checked_roots = 0
    for relative, direct in ROOT_INCLUDES.items():
        root_path = root / relative
        _regular(root_path, root)
        source = _read(root_path, overrides)
        if "B126-M2 REANCHOR MANIFEST" not in source:
            raise BoundaryError(f"{relative}: M2 manifest marker missing")
        if len(source.splitlines()) > MAX_LINES:
            raise BoundaryError(f"{relative}: exceeds {MAX_LINES} lines")
        actual_direct = _include_paths(root_path, source, root)
        if tuple(actual_direct) != direct:
            raise BoundaryError(f"{relative}: direct include order/population changed")
        root_paths = frozenset(root / item for item in ROOT_INCLUDES)
        expanded, included = _expand(
            root_path, overrides or {}, root_paths=root_paths, display_root=root
        )
        actual_paths = [_relative(path, root) for path in included]
        if tuple(actual_paths) != ROOT_FRAGMENTS[relative]:
            raise BoundaryError(f"{relative}: transitive fragment population/order changed")
        code = _mask_non_code(expanded)
        for symbol in API_SYMBOLS[relative]:
            if re.search(rf"(?<![A-Za-z0-9_]){re.escape(symbol)}(?![A-Za-z0-9_])", code) is None:
                raise BoundaryError(f"{relative}: API symbol missing: {symbol}")
        expected_reanchors: set[str] = set()
        for path in included:
            _regular(path, root)
            fragment = _read(path, overrides)
            if len(fragment.splitlines()) > MAX_LINES:
                raise BoundaryError(f"{_relative(path, root)}: exceeds {MAX_LINES} lines")
            if path.name.startswith("b126_m2_"):
                expected_reanchors.add(_fragment_reanchor(path, root))
        for expected in sorted(expected_reanchors):
            declarations = re.findall(
                rf"\bconst\s+{re.escape(expected)}\s*:\s*\(\)\s*=\s*\(\)\s*;",
                code,
            )
            if len(declarations) != 1:
                raise BoundaryError(
                    f"{relative}: expected exactly one reanchor {expected}, "
                    f"found {len(declarations)}"
                )
        expected_global.extend(included)
        checked_roots += 1

    if len(expected_global) != 53 or len(set(expected_global)) != 53:
        raise BoundaryError("B126 source population is not exactly 53 unique files")
    discovered = sorted(
        (root / "crates/corelink-container/src").rglob("b126_m2_*.rs")
    )
    for path in discovered:
        _regular(path, root)
    actual_files = sorted(_relative(path, root) for path in discovered)
    expected_m2_files = sorted(
        _relative(path, root) for path in expected_global if path.name.startswith("b126_m2_")
    )
    if actual_files != expected_m2_files:
        raise BoundaryError("on-disk b126_m2_*.rs inventory differs from closed manifest")

    audit_drain_duplicate_tail_guard_report(root, overrides)

    fixture_path = root / G4B_FIXTURE
    wiring_path = root / G4B_FRAGMENT
    fixture = _read(fixture_path, overrides)
    wiring = _read(wiring_path, overrides)
    _validate_g4b(fixture, wiring)
    return {"roots": checked_roots, "fragments": len(expected_global), "g4b_tests": len(G4B_TESTS)}


def _must_red(root: Path, overrides: dict[Path, str], label: str) -> None:
    try:
        verify(root, overrides)
    except BoundaryError:
        return
    raise BoundaryError(f"mutation unexpectedly passed: {label}")


def _remove_function(source: str, name: str) -> str:
    code = _mask_non_code(source)
    marker = re.search(rf"async\s+fn\s+{re.escape(name)}\s*\(", code)
    if marker is None:
        raise BoundaryError(f"self-test function missing: {name}")
    start = code.rfind("#[tokio::test]", 0, marker.start())
    if start < 0:
        raise BoundaryError(f"self-test attribute missing: {name}")
    opening = code.find("{", marker.end())
    if opening < 0:
        raise BoundaryError(f"self-test body missing: {name}")
    depth = 0
    end = opening
    while end < len(code):
        if code[end] == "{":
            depth += 1
        elif code[end] == "}":
            depth -= 1
            if depth == 0:
                end += 1
                break
        end += 1
    if depth:
        raise BoundaryError(f"self-test function unterminated: {name}")
    return source[:start] + source[end:]


def self_test(root: Path = ROOT) -> None:
    fixture_path = root / G4B_FIXTURE
    wiring_path = root / G4B_FRAGMENT
    fixture = _read(fixture_path)
    wiring = _read(wiring_path)
    for name in G4B_TESTS:
        renamed = fixture.replace(f"async fn {name}(", f"async fn {name}_renamed(", 1)
        _must_red(root, {fixture_path: renamed}, f"rename test {name}")
        _must_red(root, {fixture_path: _remove_function(fixture, name)}, f"remove test {name}")

    _must_red(
        root,
        {wiring_path: wiring.replace('#[path = "tests/oci_g4b_tests.rs"]', "", 1)},
        "remove G4b path wiring",
    )
    _must_red(
        root,
        {wiring_path: wiring.replace('#[path = "tests/oci_g4b_tests.rs"]', '#[path = "tests/renamed.rs"]', 1)},
        "rename G4b path wiring",
    )
    _must_red(
        root,
        {wiring_path: wiring.replace("mod g4b_tests;", "mod renamed_g4b_tests;", 1)},
        "rename G4b module wiring",
    )

    marker_only = (
        'pub(super) const B126_M2_OCI_G4B_WIRING_SENTINEL: &str = '
        '"oci-g4b-tests-wired-v1";\n'
        "// async fn " + " / async fn ".join(G4B_TESTS) + "\n"
        'const bait: &str = "' + " ".join(G4B_TESTS) + '";\n'
    )
    _must_red(root, {fixture_path: marker_only}, "sentinel-only fixture")

    no_tests = fixture
    for name in G4B_TESTS:
        no_tests = _remove_function(no_tests, name)
    bait = no_tests + "\n/* " + " ".join(G4B_TESTS) + " */\n" + 'const B: &str = "' + " ".join(G4B_TESTS) + '";\n'
    _must_red(root, {fixture_path: bait}, "comment/string bait")

    impl2_path = root / "crates/corelink-container/src/routes/oci/b126_m2_impl_02.rs"
    impl2 = _read(impl2_path)
    include_removed = impl2.replace('include!("b126_m2_test_1_3.rs");\n', "", 1)
    _must_red(root, {impl2_path: include_removed}, "remove fixture include")

    audit_impl2_path = root / "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02.rs"
    audit_impl2 = _read(audit_impl2_path)
    part3 = 'include!("b126_m2_impl_02_part3.rs");\n'
    _must_red(
        root,
        {audit_impl2_path: audit_impl2.replace(part3, "", 1)},
        "remove audit implementation continuation",
    )

    audit_report = audit_drain_duplicate_tail_guard_report(root)
    owner_relative = audit_report["definition_fragments"][0]
    owner_path = root / owner_relative
    owner_source = _read(owner_path)
    owner_code = _mask_non_code(owner_source)
    function_start, function_end = _rust_function_body_span(
        owner_code, AUDIT_DUPLICATE_TAIL_DEFINITION
    )
    function_source = owner_source[function_start:function_end]
    owner_without_function = owner_source[:function_start] + owner_source[function_end:]
    _must_red(
        root,
        {owner_path: owner_without_function},
        "remove duplicate-tail guard definition",
    )

    audit_root_path = root / AUDIT_DRAIN_ROOT
    audit_root_source = _read(audit_root_path)
    _must_red(
        root,
        {
            owner_path: owner_without_function,
            audit_root_path: audit_root_source + "\n" + function_source + "\n",
        },
        "relocate duplicate-tail guard outside B126 fragment chain",
    )

    duplicate_target_relative = next(
        relative
        for relative in ROOT_FRAGMENTS[AUDIT_DRAIN_ROOT]
        if relative.startswith(
            "crates/corelink-container/src/routes/audit_drain/b126_m2_"
        )
        and relative != owner_relative
    )
    duplicate_target_path = root / duplicate_target_relative
    duplicate_target_source = _read(duplicate_target_path)
    _must_red(
        root,
        {
            duplicate_target_path: duplicate_target_source + "\n" + function_source + "\n"
        },
        "duplicate duplicate-tail guard definition",
    )

    _must_red(
        root,
        {
            owner_path: owner_without_function
            + "\n// "
            + AUDIT_DUPLICATE_TAIL_DEFINITION
            + "\n"
        },
        "comment bait for duplicate-tail guard definition",
    )

    reader_relative = next(
        relative
        for relative in ROOT_FRAGMENTS[AUDIT_DRAIN_ROOT]
        if AUDIT_SEALED_TAIL_READER
        in _mask_non_code(_read(root / relative))
    )
    reader_path = root / reader_relative
    reader_source = _read(reader_path)
    reader_code = _mask_non_code(reader_source)
    reader_start, reader_end = _rust_function_body_span(
        reader_code, AUDIT_SEALED_TAIL_READER
    )
    reader_without_function = reader_source[:reader_start] + reader_source[reader_end:]
    real_call_removed = reader_source.replace(AUDIT_DUPLICATE_TAIL_CALL, "", 1)
    decoy_reader = (
        "\nasync fn read_any_sealed_tail() {\n    "
        + AUDIT_DUPLICATE_TAIL_CALL
        + "\n}\n"
    )
    _must_red(
        root,
        {
            reader_path: real_call_removed,
            audit_root_path: audit_root_source + decoy_reader,
        },
        "decoy reader before real reader with real call removed",
    )

    _must_red(
        root,
        {audit_root_path: audit_root_source + "\nasync fn read_any_sealed_tail() {}\n"},
        "duplicate sealed-tail reader",
    )

    _must_red(
        root,
        {
            reader_path: reader_without_function
            + "\n// "
            + AUDIT_SEALED_TAIL_READER
            + "\n"
        },
        "sealed-tail reader name bait",
    )

    audit_impl2_tail_path = (
        root / "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02_part2.rs"
    )
    audit_impl2_tail = _read(audit_impl2_tail_path)
    duplicate_reanchor = audit_impl2_tail + "\nconst B126_M2_IMPL_2_REANCHOR: () = ();\n"
    _must_red(
        root,
        {audit_impl2_tail_path: duplicate_reanchor},
        "duplicate audit implementation reanchor",
    )

    audit_root = root / "crates/corelink-container/src/routes/audit_drain.rs"
    audit_source = _read(audit_root)
    witness = 'include!("audit_drain/b054_witness.rs");\n'
    epoch_admin = 'include!("audit_drain/b054_epoch_admin.rs");\n'
    _must_red(
        root,
        {audit_root: audit_source.replace(witness, "", 1)},
        "remove B054 witness include",
    )
    _must_red(
        root,
        {audit_root: audit_source.replace(witness, witness + witness, 1)},
        "duplicate B054 witness include",
    )
    _must_red(
        root,
        {audit_root: audit_source.replace(witness + epoch_admin, epoch_admin + witness, 1)},
        "reorder B054 witness include",
    )
    _must_red(
        root,
        {audit_root: audit_source.replace(epoch_admin, "", 1)},
        "remove B054 epoch admin include",
    )
    _must_red(
        root,
        {audit_root: audit_source.replace(epoch_admin, epoch_admin + epoch_admin, 1)},
        "duplicate B054 epoch admin include",
    )

    epoch_admin_path = root / "crates/corelink-container/src/routes/audit_drain/b054_epoch_admin.rs"
    epoch_admin_source = _read(epoch_admin_path)
    epoch_admin_part = '    include!("b054_epoch_admin_part2.rs");\n'
    _must_red(
        root,
        {epoch_admin_path: epoch_admin_source.replace(epoch_admin_part, "", 1)},
        "remove B054 epoch admin continuation",
    )
    _must_red(
        root,
        {
            epoch_admin_path: epoch_admin_source.replace(
                epoch_admin_part, epoch_admin_part + epoch_admin_part, 1
            )
        },
        "duplicate B054 epoch admin continuation",
    )

    witness_path = root / "crates/corelink-container/src/routes/audit_drain/b054_witness.rs"
    witness_source = _read(witness_path)
    added_lines = MAX_LINES - len(witness_source.splitlines()) + 1
    over_cap = witness_source + ("// B054 over-cap mutation\n" * added_lines)
    _must_red(root, {witness_path: over_cap}, "B054 witness exceeds fragment cap")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    root = Path(args.root).resolve()
    try:
        report = verify(root)
        if args.self_test:
            self_test(root)
    except (BoundaryError, OSError, UnicodeDecodeError) as exc:
        print(f"B-126-M2 wiring guard: FAIL: {exc}", file=sys.stderr)
        return 1
    print(
        "B-126-M2 wiring guard: PASS: "
        f"{report['roots']} roots, {report['fragments']} fragments, {report['g4b_tests']} G4b tests"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
