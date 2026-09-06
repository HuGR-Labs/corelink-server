#!/usr/bin/env python3
"""Fail-closed verifier for the bounded, owner-only B-102..B-108 lane.

The packet is evidence, not a declaration.  Every observation carries the
tenant and an operation id; deployment identity is bound to the GitHub run and
to the provider response digest; all derived values are recomputed here.  No
credential is accepted in the packet.  The verifier never contacts production.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import math
import re
import subprocess
import sys
from pathlib import Path
from statistics import quantiles
from typing import Any

SCHEMA = "corelink.performance-evidence.v2"
ITEMS = tuple(f"B-{n:03d}" for n in range(102, 109))
REPO = "HuGR/corelink-server"
SOURCE = "worker/src/lib/quota.ts"
UUID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
SHA1 = re.compile(r"^[0-9a-f]{40}$")
OP = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._:-]{7,127}$")
SECRET = re.compile(r"(?i)(bearer\s+|pat[_-]?token|api[_-]?key|password|secret|private[_-]?key|authorization)")
FINGERPRINT = re.compile(r"^sha256:[0-9a-f]{64}$")
MAX_AGE = 15 * 60
CLOCK_TOLERANCE = 2
COUNTER_SQL = (
    "INSERT INTO monthly_request_counts (tenant_id, year_month, request_count, updated_at_ms) "
    "VALUES (?1, ?2, 1, ?3) "
    "ON CONFLICT(tenant_id, year_month) "
    "DO UPDATE SET request_count = request_count + 1, updated_at_ms = ?3 "
    "RETURNING request_count"
)


class EvidenceError(ValueError):
    pass


def obj(v: Any, label: str) -> dict[str, Any]:
    if not isinstance(v, dict):
        raise EvidenceError(f"{label} must be an object")
    return v


def text(v: Any, label: str) -> str:
    if not isinstance(v, str) or not v.strip():
        raise EvidenceError(f"{label} must be a non-empty string")
    return v.strip()


def num(v: Any, label: str, minimum: float = 0.0) -> float:
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        raise EvidenceError(f"{label} must be numeric")
    x = float(v)
    if not math.isfinite(x) or x < minimum:
        raise EvidenceError(f"{label} must be finite and >= {minimum}")
    return x


def entries(v: Any, label: str, minimum: int) -> list[dict[str, Any]]:
    if not isinstance(v, list) or len(v) < minimum:
        raise EvidenceError(f"{label} requires at least {minimum} entries")
    return [obj(x, f"{label}[{i}]") for i, x in enumerate(v)]


def sha(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def raw_hash(v: Any, label: str) -> str:
    value = text(v, label)
    if not FINGERPRINT.fullmatch(value):
        raise EvidenceError(f"{label} must be sha256:<64 lowercase hex>")
    return value


def mint_binding_sha256(att: dict[str, Any]) -> str:
    material = json.dumps(
        {
            "expires_ms": att.get("expires_ms"),
            "pat_id": att.get("pat_id"),
            "tenant_id": att.get("tenant_id"),
            "token_fingerprint": att.get("token_fingerprint"),
            "token_id": att.get("token_id"),
            "mint_operation_id": att.get("mint_operation_id"),
            "mint_request_id": att.get("mint_request_id"),
            "mint_response_sha256": att.get("mint_response_sha256"),
        },
        sort_keys=True,
        separators=(",", ":"),
    ).encode()
    return sha(material)


def reject_secrets(v: Any, path: str = "packet") -> None:
    if isinstance(v, str) and SECRET.search(v):
        raise EvidenceError(f"{path} contains credential-like material")
    if isinstance(v, dict):
        for k, child in v.items():
            # Credential-shaped names are forbidden even when their value is
            # redacted.  Fingerprints are the sole intentional token exception.
            if SECRET.search(k) and not (k.endswith("_fingerprint") or k.endswith("_raw_output_sha256")):
                raise EvidenceError(f"{path}.{k} is a credential field")
            reject_secrets(child, f"{path}.{k}")
    elif isinstance(v, list):
        for i, child in enumerate(v):
            reject_secrets(child, f"{path}[{i}]")


def iso_epoch(value: Any, label: str) -> float:
    raw = text(value, label)
    try:
        parsed = dt.datetime.fromisoformat(raw.replace("Z", "+00:00"))
    except ValueError as exc:
        raise EvidenceError(f"{label} must be ISO-8601") from exc
    if parsed.tzinfo is None:
        raise EvidenceError(f"{label} must include a timezone")
    return parsed.timestamp()


def common(row: dict[str, Any], label: str, tenant: str) -> None:
    if row.get("tenant_id") != tenant:
        raise EvidenceError(f"{label}.tenant_id is not the packet tenant")
    operation = text(row.get("operation_id"), f"{label}.operation_id")
    if not OP.fullmatch(operation):
        raise EvidenceError(f"{label}.operation_id is malformed")


def deployment(item: dict[str, Any], label: str, root: Path, tenant: str) -> str:
    d = obj(item.get("deployment"), f"{label}.deployment")
    if d.get("repository") != REPO or d.get("environment") != "production":
        raise EvidenceError(f"{label} deployment is not the production repository")
    commit = text(d.get("deployed_commit"), f"{label}.deployment.deployed_commit")
    source_head = text(d.get("source_head"), f"{label}.deployment.source_head")
    if not SHA1.fullmatch(commit) or not SHA1.fullmatch(source_head):
        raise EvidenceError(f"{label} deployment SHAs must be full lowercase commits")
    if d.get("github_sha") != source_head or d.get("github_repository") != REPO:
        raise EvidenceError(f"{label} deployment is not bound to GitHub context")
    if d.get("github_event") not in {"workflow_dispatch", "workflow_run"}:
        raise EvidenceError(f"{label} deployment lane is not owner-triggered")
    run_id = text(d.get("github_run_id"), f"{label}.deployment.github_run_id")
    if not run_id.isdigit():
        raise EvidenceError(f"{label}.deployment.github_run_id is malformed")
    github_deployment_id = text(d.get("github_deployment_id"), f"{label}.deployment.github_deployment_id")
    if not github_deployment_id.isdigit():
        raise EvidenceError(f"{label}.deployment.github_deployment_id is malformed")
    provider = obj(d.get("provider_record"), f"{label}.deployment.provider_record")
    if provider.get("provider") != "cloudflare" or provider.get("environment") != "production":
        raise EvidenceError(f"{label} provider record is not Cloudflare production")
    provider_blob = text(d.get("provider_blob_sha256"), f"{label}.deployment.provider_blob_sha256")
    if not FINGERPRINT.fullmatch(provider_blob):
        raise EvidenceError(f"{label} provider response must be content-addressed")
    if provider.get("commit") != commit or provider.get("deployment_id") != d.get("deployment_id"):
        raise EvidenceError(f"{label} provider record is not bound to the deployed commit/id")
    deployment_id = text(d.get("deployment_id"), f"{label}.deployment.deployment_id")
    if not OP.fullmatch(deployment_id):
        raise EvidenceError(f"{label}.deployment_id is malformed")
    if text(d.get("version"), f"{label}.deployment.version") != commit:
        raise EvidenceError(f"{label}.deployment.version is not the deployed commit")
    if text(d.get("tenant_id"), f"{label}.deployment.tenant_id") != tenant:
        raise EvidenceError(f"{label}.deployment is not tenant-scoped")
    op = text(d.get("operation_id"), f"{label}.deployment.operation_id")
    if not OP.fullmatch(op):
        raise EvidenceError(f"{label}.deployment.operation_id is malformed")
    try:
        present = subprocess.run(("git", "cat-file", "-e", f"{commit}^{{commit}}"), cwd=root, capture_output=True, timeout=5)
        ancestor = subprocess.run(("git", "merge-base", "--is-ancestor", commit, source_head), cwd=root, capture_output=True, timeout=5)
    except (OSError, subprocess.SubprocessError) as exc:
        raise EvidenceError(f"{label} git provenance unavailable: {exc}") from exc
    if present.returncode or ancestor.returncode:
        raise EvidenceError(f"{label} deployed commit is absent or not ancestral to GitHub SHA")
    return commit


def b102(item: dict[str, Any], root: Path, tenant: str) -> str:
    deployment(item, "B-102", root, tenant)
    rows = entries(item.get("requests"), "B-102.requests", 3)[:3]
    for i, row in enumerate(rows):
        common(row, f"B-102.requests[{i}]", tenant)
        if row.get("method") != "PUT" or row.get("status") != 200 or num(row.get("payload_bytes"), "B-102.payload_bytes") != 1024:
            raise EvidenceError("B-102 requires three successful 1 KiB PUTs")
        raw_hash(row.get("raw_output_sha256"), f"B-102.requests[{i}].raw_output_sha256")
        text(row.get("server_timing"), "B-102.server_timing")
    if num(item.get("sequence_window_seconds"), "B-102.sequence_window_seconds") > 60:
        raise EvidenceError("B-102 PUTs are not a bounded warm sequence")
    if not FINGERPRINT.fullmatch(text(item.get("token_fingerprint"), "B-102.token_fingerprint")):
        raise EvidenceError("B-102 token fingerprint is not SHA-256")
    return "open"


def b103(item: dict[str, Any], root: Path, tenant: str) -> str:
    deployment(item, "B-103", root, tenant)
    runs = entries(item.get("runs"), "B-103.runs", 3)
    if len({num(x.get("concurrency"), "B-103.concurrency", 1) for x in runs}) < 3:
        raise EvidenceError("B-103 requires three concurrency levels")
    measured = []
    for i, row in enumerate(runs):
        common(row, f"B-103.runs[{i}]", tenant)
        c = num(row.get("concurrency"), "B-103.concurrency", 1)
        requests = num(row.get("requests"), "B-103.requests", 1)
        successes = num(row.get("successes"), "B-103.successes")
        failures = obj(row.get("failures"), "B-103.failures")
        failed = sum(num(v, "B-103.failure_count") for v in failures.values())
        if requests != successes + failed:
            raise EvidenceError(f"B-103 counts do not reconcile at concurrency {c:g}")
        if failed != 0:
            raise EvidenceError("B-103 requires zero failures at every concurrency level")
        throughput = successes / (num(row.get("wall_ms"), "B-103.wall_ms", .001) / 1000)
        raw = num(row.get("throughput_rps"), "B-103.throughput_rps")
        if abs(raw - throughput) > max(.001, throughput * .001):
            raise EvidenceError("B-103 throughput is not derived from raw counts")
        raw_hash(row.get("raw_output_sha256"), "B-103.raw_output_sha256")
        measured.append((c, throughput, failed))
    measured.sort()
    low, high = measured[0], measured[-1]
    # Significant scaling is explicit and mathematical, not a self-attested bool.
    return "open"


def percentile(values: list[float], p: float) -> float:
    return values[0] if len(values) == 1 else quantiles(values, n=100, method="inclusive")[int(p * 100) - 1]


def b104(item: dict[str, Any], root: Path, tenant: str) -> str:
    deployment(item, "B-104", root, tenant)
    rows = entries(item.get("samples"), "B-104.samples", 10)
    times = []
    for i, row in enumerate(rows):
        common(row, f"B-104.samples[{i}]", tenant)
        if row.get("method") != "GET" or row.get("status") != 404 or row.get("authenticated") is not True:
            raise EvidenceError("B-104 requires authenticated 404 GETs")
        if row.get("auth_source") not in {"l1", "kv", "d1"}:
            raise EvidenceError("B-104 requires a wire-derived auth source")
        if not text(row.get("colo"), f"B-104.samples[{i}].colo"):
            raise EvidenceError("B-104 requires a wire-derived colo")
        if not text(row.get("response_request_id"), f"B-104.samples[{i}].response_request_id"):
            raise EvidenceError("B-104 requires a wire-derived response request id")
        raw_hash(row.get("raw_output_sha256"), f"B-104.samples[{i}].raw_output_sha256")
        times.append(num(row.get("elapsed_ms"), "B-104.elapsed_ms"))
    ordered = sorted(times)
    median = (ordered[(len(ordered)-1)//2] + ordered[len(ordered)//2]) / 2
    p90 = percentile(times, .90)
    computed = obj(item.get("computed"), "B-104.computed")
    if abs(num(computed.get("median_ms"), "B-104.median_ms") - median) > .001 or abs(num(computed.get("p90_ms"), "B-104.p90_ms") - p90) > .001:
        raise EvidenceError("B-104 percentiles are not derived from raw samples")
    return "open"


def b105(item: dict[str, Any], root: Path, tenant: str) -> str:
    deployment(item, "B-105", root, tenant)
    pairs = entries(item.get("pairs"), "B-105.pairs", 6)
    if len(pairs) != 6:
        raise EvidenceError("B-105 requires exactly six paired runs")
    first = []
    for i, pair in enumerate(pairs):
        common(pair, f"B-105.pairs[{i}]", tenant)
        control, treatment = obj(pair.get("control"), "B-105.control"), obj(pair.get("treatment"), "B-105.treatment")
        for arm, row, mode in (("control", control, "disabled"), ("treatment", treatment, "enabled")):
            common(row, f"B-105.{arm}", tenant)
            if row.get("cache_mode") != mode or row.get("status") != "complete":
                raise EvidenceError("B-105 cache mode/status is not real lane evidence")
            raw_hash(row.get("raw_output_sha256"), f"B-105.{arm}.raw_output_sha256")
            raw_hash(row.get("sccache_stats_raw_sha256"), f"B-105.{arm}.sccache_stats_raw_sha256")
            num(row.get("duration_seconds"), f"B-105.{arm}.duration_seconds", .001)
            stats = obj(row.get("sccache"), f"B-105.{arm}.sccache")
            num(stats.get("hits"), f"B-105.{arm}.sccache.hits")
            num(stats.get("misses"), f"B-105.{arm}.sccache.misses")
            if any(num(stats.get(k), f"B-105.{arm}.sccache.{k}") != 0 for k in ("read_errors", "write_errors")):
                raise EvidenceError("B-105 cache lane contains an sccache error")
        for key in ("revision", "runner", "machine", "toolchain", "command"):
            if control.get(key) != treatment.get(key):
                raise EvidenceError(f"B-105 arms differ in {key}")
        first.append(pair.get("control_first"))
    if any(x not in {True, False} for x in first) or any(a == b for a, b in zip(first, first[1:])):
        raise EvidenceError("B-105 must alternate control-first and treatment-first")
    return "open"


def b106(item: dict[str, Any], root: Path, tenant: str, now: float) -> str:
    deployment(item, "B-106", root, tenant)
    if item.get("kv_ttl_seconds") != 60:
        raise EvidenceError("B-106 must preserve the 60-second revocation floor")
    att = obj(item.get("cold_attestation"), "B-106.cold_attestation")
    common(att, "B-106.cold_attestation", tenant)
    raw_hash(att.get("mint_raw_output_sha256"), "B-106.mint_raw_output_sha256")
    raw_hash(att.get("mint_response_sha256"), "B-106.mint_response_sha256")
    if att.get("mint_response_sha256") != att.get("mint_raw_output_sha256"):
        raise EvidenceError("B-106 mint response digests do not agree")
    raw_hash(att.get("mint_response_binding_sha256"), "B-106.mint_response_binding_sha256")
    for key in ("pat_id", "token_id"):
        text(att.get(key), f"B-106.{key}")
    num(att.get("expires_ms"), "B-106.expires_ms", 1)
    mint_operation_id = text(att.get("mint_operation_id"), "B-106.mint_operation_id")
    if mint_operation_id != text(att.get("operation_id"), "B-106.cold_attestation.operation_id"):
        raise EvidenceError("B-106 mint operation is not the attested operation")
    text(att.get("mint_request_id"), "B-106.mint_request_id")
    fingerprint = text(att.get("token_fingerprint"), "B-106.token_fingerprint")
    if not FINGERPRINT.fullmatch(fingerprint):
        raise EvidenceError("B-106 token fingerprint is not SHA-256")
    if att.get("mint_response_binding_sha256") != mint_binding_sha256(att):
        raise EvidenceError("B-106 mint response binding is not reproducible")
    for key in ("minted_at_epoch", "unused_since_epoch", "observed_at_epoch"):
        num(att.get(key), f"B-106.{key}", 1)
    minted, unused, observed = (float(att[k]) for k in ("minted_at_epoch", "unused_since_epoch", "observed_at_epoch"))
    if not minted <= unused <= observed - 60 or observed > now + CLOCK_TOLERANCE:
        raise EvidenceError("B-106 does not prove a newly minted token idle for 60 seconds")
    cold, warm = obj(item.get("cold"), "B-106.cold"), obj(item.get("warm_control"), "B-106.warm_control")
    for row, label in ((cold, "B-106.cold"), (warm, "B-106.warm_control")):
        common(row, label, tenant)
        raw_hash(row.get("raw_output_sha256"), f"{label}.raw_output_sha256")
        if not row.get("response_request_id") or not row.get("colo"):
            raise EvidenceError(f"{label} is missing wire request/colo identity")
        if row.get("token_fingerprint") != fingerprint:
            raise EvidenceError("B-106 mint and request are not cryptographically linked")
    if cold.get("status") != 404 or cold.get("authenticated") is not True or cold.get("auth_source") != "d1" or warm.get("status") != 404 or warm.get("authenticated") is not True or warm.get("auth_source") not in {"l1", "kv"} or cold.get("colo") != warm.get("colo") or cold.get("response_request_id") == warm.get("response_request_id"):
        raise EvidenceError("B-106 cold and same-colo warm control are not proven")
    return "open"


def b107(item: dict[str, Any], root: Path, tenant: str) -> str:
    deployment(item, "B-107", root, tenant)
    rows = entries(item.get("samples"), "B-107.samples", 10)
    totals = []
    for i, row in enumerate(rows):
        common(row, f"B-107.samples[{i}]", tenant)
        if row.get("method") != "PUT" or row.get("status") != 200 or num(row.get("payload_bytes"), f"B-107.samples[{i}].payload_bytes") != 1024:
            raise EvidenceError("B-107 requires successful 1 KiB PUT responses")
        r2, accounting, phase_store, total = (
            num(row.get(k), f"B-107.{k}")
            for k in ("r2_ms", "accounting_ms", "ostore_ms", "storage_total_ms")
        )
        if abs(phase_store - r2) > .001 or abs(total - r2 - accounting) > .001:
            raise EvidenceError("B-107 R2/accounting phases do not reconcile exactly")
        raw_hash(row.get("r2_raw_output_sha256"), "B-107.r2_raw_output_sha256")
        raw_hash(row.get("accounting_raw_output_sha256"), "B-107.accounting_raw_output_sha256")
        totals.append(total)
    computed = obj(item.get("computed"), "B-107.computed")
    values = sorted(totals)
    expected = ((values[(len(values)-1)//2] + values[len(values)//2]) / 2, percentile(values, .90), percentile(values, .99))
    for key, value in zip(("p50_ms", "p90_ms", "p99_ms"), expected):
        if abs(num(computed.get(key), f"B-107.{key}") - value) > .001:
            raise EvidenceError("B-107 percentile is not derived from raw phase samples")
    return "open"


def b108(item: dict[str, Any], root: Path, tenant: str) -> str:
    commit = deployment(item, "B-108", root, tenant)
    binding = obj(item.get("source_binding"), "B-108.source_binding")
    if binding.get("path") != SOURCE or binding.get("commit") != commit:
        raise EvidenceError("B-108 source binding is not the deployed blob")
    try:
        source = subprocess.run(("git", "show", f"{commit}:{SOURCE}"), cwd=root, capture_output=True, text=True, check=True, timeout=5).stdout
    except (OSError, subprocess.SubprocessError) as exc:
        raise EvidenceError(f"B-108 deployed source blob unavailable: {exc}") from exc
    if binding.get("blob_sha256") != sha(source.encode()) or not FINGERPRINT.fullmatch(str(binding.get("blob_sha256", ""))):
        raise EvidenceError("B-108 source hash is not the deployed blob")
    # The source stores the SQL as adjacent TypeScript string literals, so
    # compare every immutable clause rather than looking for a non-existent
    # contiguous source-text rendering of the runtime statement.
    clauses = (
        "INSERT INTO monthly_request_counts (tenant_id, year_month, request_count, updated_at_ms)",
        "VALUES (?1, ?2, 1, ?3)",
        "ON CONFLICT(tenant_id, year_month)",
        "DO UPDATE SET request_count = request_count + 1, updated_at_ms = ?3",
        "RETURNING request_count",
    )
    if any(clause not in source for clause in clauses) or "runQuotaBatch" not in source or "db.batch" not in source:
        raise EvidenceError("B-108 atomic fresh counter guard is absent from deployed source")
    statement = text(item.get("counter_statement"), "B-108.counter_statement")
    if statement != COUNTER_SQL or item.get("counter_statement_sha256") != sha(COUNTER_SQL.encode()):
        raise EvidenceError("B-108 counter statement is not the canonical atomic SQL")
    # Guard is intentionally inverted: the exact fresh atomic path closes this
    # item as accepted-by-correctness; no owner-supplied decision is consulted.
    return "closed"


CHECKERS = {"B-102": b102, "B-103": b103, "B-104": b104, "B-105": b105, "B-107": b107, "B-108": b108}


def assess(packet: Any, repo_root: Path = Path("."), now_epoch: float | None = None) -> dict[str, str]:
    root = obj(packet, "packet")
    reject_secrets(root)
    if root.get("schema") != SCHEMA or root.get("environment") != "production":
        raise EvidenceError("packet schema/environment is invalid")
    captured = iso_epoch(root.get("captured_at"), "packet.captured_at")
    now = float(now_epoch if now_epoch is not None else dt.datetime.now(dt.timezone.utc).timestamp())
    if captured > now + CLOCK_TOLERANCE or now - captured > MAX_AGE:
        raise EvidenceError("packet is stale or captured in the future")
    tenant = text(root.get("tenant_id"), "packet.tenant_id")
    if not UUID.fullmatch(tenant):
        raise EvidenceError("packet.tenant_id must be a v4 UUID")
    items = obj(root.get("items"), "packet.items")
    if set(items) != set(ITEMS):
        raise EvidenceError("packet.items must contain exactly B-102 through B-108")
    result: dict[str, str] = {}
    operation_ids: list[str] = []
    deployment_keys: set[tuple[Any, ...]] = set()
    for item_id in ITEMS:
        item = obj(items[item_id], item_id)
        if item.get("tenant_id") != tenant:
            raise EvidenceError(f"{item_id}.tenant_id is not the packet tenant")
        deployment_record = obj(item.get("deployment"), f"{item_id}.deployment")
        deployment_keys.add(tuple(deployment_record.get(k) if k != "provider_record" else json.dumps(deployment_record.get(k), sort_keys=True) for k in ("deployed_commit", "source_head", "github_sha", "github_repository", "github_run_id", "github_deployment_id", "deployment_id", "version", "provider_blob_sha256", "provider_record")))
        def collect_ops(value: Any) -> None:
            if isinstance(value, dict):
                if "operation_id" in value:
                    operation_ids.append(text(value["operation_id"], f"{item_id}.operation_id"))
                for child in value.values():
                    collect_ops(child)
            elif isinstance(value, list):
                for child in value:
                    collect_ops(child)
        collect_ops(item)
        result[item_id] = (b106(item, repo_root, tenant, now) if item_id == "B-106" else CHECKERS[item_id](item, repo_root, tenant))
    if len(operation_ids) != len(set(operation_ids)):
        raise EvidenceError("operation_id values must be unique across the packet")
    if len(deployment_keys) != 1:
        raise EvidenceError("all items must use one consistent deployed commit/provider identity")
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--packet", type=Path, required=True)
    parser.add_argument("--expect", choices=("open", "closed"), default="closed")
    args = parser.parse_args(argv)
    try:
        result = assess(json.loads(args.packet.read_text(encoding="utf-8")))
    except (OSError, json.JSONDecodeError, EvidenceError) as exc:
        print(f"instrument error: {exc}", file=sys.stderr)
        return 2
    for item, state in result.items():
        print(f"{item}: {state}")
    closed = all(state == "closed" for state in result.values())
    print(f"B-102..B-108 {'closed' if closed else 'open'}")
    return 0 if (closed == (args.expect == "closed")) else 1


if __name__ == "__main__":
    raise SystemExit(main())
