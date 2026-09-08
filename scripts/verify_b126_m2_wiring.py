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
        "audit_drain/b126_m2_impl_01.rs",
        "audit_drain/b126_m2_impl_02.rs",
        "audit_drain/b126_m2_impl_03.rs",
    ),
    "crates/corelink-container/src/routes/cas_erase.rs": (
        "cas_erase/b126_m2_impl_01.rs",
        "cas_erase/b126_m2_impl_02.rs",
    ),
    "crates/corelink-container/src/routes/oci.rs": (
        "oci/b126_m2_impl_01.rs",
        "oci/b126_m2_impl_02.rs",
    ),
    "crates/corelink-container/src/routes/turbo_v8.rs": (
        "turbo_v8/b126_m2_impl_01.rs",
        "turbo_v8/b126_m2_impl_02.rs",
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
        "crates/corelink-container/src/byte_accounting/b126_m2_impl_02.rs",
        "crates/corelink-container/src/byte_accounting/b126_m2_test_1_1.rs",
        "crates/corelink-container/src/byte_accounting/b126_m2_test_2_1.rs",
        "crates/corelink-container/src/byte_accounting/b126_m2_test_3_1.rs",
        "crates/corelink-container/src/byte_accounting/b126_m2_test_4_1.rs",
    ),
    "crates/corelink-container/src/routes/audit_drain.rs": (
        "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs",
        "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02.rs",
        "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_03.rs",
        "crates/corelink-container/src/routes/audit_drain/b126_m2_test_1_1.rs",
        "crates/corelink-container/src/routes/audit_drain/b126_m2_test_1_2.rs",
    ),
    "crates/corelink-container/src/routes/cas_erase.rs": (
        "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_01.rs",
        "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02.rs",
        "crates/corelink-container/src/routes/cas_erase/b126_m2_test_1_1.rs",
        "crates/corelink-container/src/routes/cas_erase/b126_m2_test_1_2.rs",
    ),
    "crates/corelink-container/src/routes/oci.rs": (
        "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs",
        "crates/corelink-container/src/routes/oci/b126_m2_impl_02.rs",
        "crates/corelink-container/src/routes/oci/b126_m2_test_1_1.rs",
        "crates/corelink-container/src/routes/oci/b126_m2_test_1_2.rs",
        "crates/corelink-container/src/routes/oci/b126_m2_test_1_3.rs",
    ),
    "crates/corelink-container/src/routes/turbo_v8.rs": (
        "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs",
        "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs",
        "crates/corelink-container/src/routes/turbo_v8/b126_m2_test_1_1.rs",
        "crates/corelink-container/src/routes/turbo_v8/b126_m2_test_1_2.rs",
    ),
    "crates/corelink-container/src/tenant_quota.rs": (
        "crates/corelink-container/src/tenant_quota/b126_m2_impl_01.rs",
        "crates/corelink-container/src/tenant_quota/b126_m2_impl_02.rs",
        "crates/corelink-container/src/tenant_quota/b126_m2_test_1_1.rs",
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
        "build_state_from_env", "router", "now_ms", "AUDIT_DRAIN_LEASE_TTL_MS",
        "new_lease_holder", "should_fence", "FencedSeal", "run_fenced_seal_loop",
        "acquire_lease", "release_lease", "chain_hash_from_hex", "checkpoint_nullable_u64",
        "checkpoint_nullable_text", "parse_sealed_epoch_metadata", "HeadCheckpoint", "SealedRow",
        "PartitionOutcome", "seal_rows", "seal_rows_for_epoch", "resolve_resume", "HeadResumeCheck",
        "check_head_on_resume", "reject_unwired_epoch_checkpoint", "read_pending_partitions",
        "read_unsigned_head_partitions", "read_checkpoint", "read_sealed_tail", "read_pending_rows",
        "write_seal", "advance_head_cas", "resign_unsigned_head", "converge_unsigned_heads",
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


def _fragment_reanchor(path: Path, code: str, root: Path = ROOT) -> str:
    name = path.stem
    match = re.fullmatch(r"b126_m2_(impl|test)_(\d+)(?:_(\d+))?", name)
    if not match:
        raise BoundaryError(f"unexpected M2 fragment name: {_relative(path, root)}")
    if match.group(1) == "impl":
        expected = f"B126_M2_IMPL_{int(match.group(2))}_REANCHOR"
    else:
        expected = f"B126_M2_TEST_{match.group(2)}_{match.group(3)}_REANCHOR"
    if not re.search(rf"\bconst\s+{re.escape(expected)}\s*:\s*\(\)\s*=\s*\(\)\s*;", code):
        raise BoundaryError(f"{_relative(path, root)}: missing reanchor {expected}")
    return expected


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
        for path in included:
            _regular(path, root)
            fragment = _read(path, overrides)
            if len(fragment.splitlines()) > MAX_LINES:
                raise BoundaryError(f"{_relative(path, root)}: exceeds {MAX_LINES} lines")
            _fragment_reanchor(path, _mask_non_code(fragment), root)
        expected_global.extend(included)
        checked_roots += 1

    if len(expected_global) != 28 or len(set(expected_global)) != 28:
        raise BoundaryError("M2 fragment population is not exactly 28 unique files")
    discovered = sorted(
        (root / "crates/corelink-container/src").rglob("b126_m2_*.rs")
    )
    for path in discovered:
        _regular(path, root)
    actual_files = sorted(_relative(path, root) for path in discovered)
    if actual_files != sorted(_relative(path, root) for path in expected_global):
        raise BoundaryError("on-disk b126_m2_*.rs inventory differs from closed manifest")

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
