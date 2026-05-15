#!/usr/bin/env python3
"""GAP-15 cold-restore verification gate.

Runs AFTER `scripts/cold-restore-drill.sh` completes. Validates that the
restored region is functionally + cryptographically + evidentially
equivalent to the pre-drill snapshot.

Checks (all must pass for exit 0):
  C1. Synthetic drill tenant can read pre-drill CAS fixtures.
      (fixture file lists 3 sha256s; we re-fetch and compare.)
  C2. BYOK envelope unwrap returns identical DEK SHA-256 to the fixture.
      (proves the BYOK envelope-encryption chain survived the restore.)
  C3. Audit-chain Merkle root for the pre-drill epoch matches the fixture's
      pre-drill checkpoint root (bit-for-bit).
  C4. SLO Prometheus histograms have non-zero buckets for the canonical
      three series (gw_request, audit_chain_append, byok_envelope_unwrap)
      within the last 15 minutes.

On any failure, prints a structured diagnostic identifying which check
failed and why. Exits with a check-specific exit code (1..4) so the
orchestrator can route the failure to the right escalation path.

This script is intentionally side-effect-free: it only reads. It does
not mutate any restored state. Safe to re-run any number of times.

Usage:
    scripts/verify-cold-restore.py \
        --env staging \
        --fixture specs/_compliance/drill-evidence/fixtures/cold-restore-tenant-pre-drill.json \
        [--merkle-root-expected <hex>] \
        [--output drill-verification-<ts>.json] \
        [--skip-network]   # offline mode: validate fixture format only

Environment variables (optional, override defaults):
    CORELINK_API_BASE      Base URL for synthetic CAS reads (default: from fixture).
    DRILL_TENANT_PAT       PAT for synthetic drill tenant (default: from fixture).
    BYOK_VERIFY_ENDPOINT   Internal byok unwrap-test endpoint (default: from fixture).
    PROMETHEUS_URL         Prometheus URL for SLO check (default: from fixture).

Exit codes:
    0 = all checks pass
    1 = C1 CAS fixture read failed
    2 = C2 BYOK envelope unwrap failed
    3 = C3 audit-chain Merkle root mismatch
    4 = C4 SLO histograms not recovered
    64 = usage error
    65 = fixture file invalid
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any

try:
    import urllib.error
    import urllib.request
except ImportError as exc:  # pragma: no cover - stdlib should always import
    print(f"ERROR: failed to import urllib: {exc}", file=sys.stderr)
    sys.exit(64)


# ---------------------------------------------------------------------------
# Dataclasses for structured output
# ---------------------------------------------------------------------------


@dataclass
class CheckResult:
    """Result of one verification check."""

    check_id: str
    name: str
    status: str  # "pass" | "fail" | "skipped"
    message: str
    duration_ms: int = 0
    extra: dict = field(default_factory=dict)


@dataclass
class VerificationReport:
    """Full verification output."""

    env: str
    fixture_path: str
    started_at_utc: str
    completed_at_utc: str = ""
    overall: str = "pending"
    checks: list = field(default_factory=list)

    def as_dict(self) -> dict:
        return {
            "env": self.env,
            "fixture_path": self.fixture_path,
            "started_at_utc": self.started_at_utc,
            "completed_at_utc": self.completed_at_utc,
            "overall": self.overall,
            "checks": [asdict(c) for c in self.checks],
        }


# ---------------------------------------------------------------------------
# Fixture loading
# ---------------------------------------------------------------------------


def load_fixture(path: Path) -> dict:
    """Load + minimally validate the pre-drill fixture file.

    Required top-level keys: tenant_id, cas_blobs, byok_envelope,
    audit_chain_checkpoint, slo_endpoints.

    Returns the parsed fixture dict on success; calls sys.exit(65) on any
    structural issue (we treat fixture corruption as a setup error, not a
    drill failure).
    """
    if not path.is_file():
        print(f"ERROR: fixture not found: {path}", file=sys.stderr)
        sys.exit(65)

    try:
        with path.open("r", encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, json.JSONDecodeError) as exc:
        print(f"ERROR: failed to parse fixture {path}: {exc}", file=sys.stderr)
        sys.exit(65)

    required = (
        "tenant_id",
        "cas_blobs",
        "byok_envelope",
        "audit_chain_checkpoint",
        "slo_endpoints",
    )
    missing = [k for k in required if k not in data]
    if missing:
        print(
            f"ERROR: fixture missing required keys: {missing}",
            file=sys.stderr,
        )
        sys.exit(65)

    if not isinstance(data["cas_blobs"], list) or not data["cas_blobs"]:
        print("ERROR: fixture.cas_blobs must be a non-empty list", file=sys.stderr)
        sys.exit(65)

    return data


# ---------------------------------------------------------------------------
# Network helpers — bounded timeouts; never raise to caller.
# ---------------------------------------------------------------------------


def safe_http_get(
    url: str,
    headers: dict | None = None,
    timeout_seconds: float = 10.0,
) -> tuple[int, bytes]:
    """HTTP GET with bounded timeout. Returns (status, body).

    Returns (0, b"") on transport error (caller treats as failure).
    Never raises.
    """
    req = urllib.request.Request(url, headers=headers or {})
    try:
        with urllib.request.urlopen(req, timeout=timeout_seconds) as resp:
            return resp.status, resp.read()
    except urllib.error.HTTPError as exc:
        return exc.code, exc.read() if exc.fp else b""
    except (urllib.error.URLError, TimeoutError, ConnectionError, OSError):
        return 0, b""


def safe_http_post(
    url: str,
    payload: dict,
    headers: dict | None = None,
    timeout_seconds: float = 10.0,
) -> tuple[int, bytes]:
    """HTTP POST with JSON body."""
    body = json.dumps(payload).encode("utf-8")
    hdrs = {"Content-Type": "application/json"}
    if headers:
        hdrs.update(headers)
    req = urllib.request.Request(url, data=body, headers=hdrs, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=timeout_seconds) as resp:
            return resp.status, resp.read()
    except urllib.error.HTTPError as exc:
        return exc.code, exc.read() if exc.fp else b""
    except (urllib.error.URLError, TimeoutError, ConnectionError, OSError):
        return 0, b""


# ---------------------------------------------------------------------------
# Checks
# ---------------------------------------------------------------------------


def check_c1_cas_fixtures(fixture: dict, skip_network: bool) -> CheckResult:
    """C1: synthetic tenant can read pre-drill CAS fixtures."""
    t0 = time.time()
    api_base = os.environ.get(
        "CORELINK_API_BASE", fixture.get("api_base", "https://api.corelink.io")
    )
    pat = os.environ.get("DRILL_TENANT_PAT", fixture.get("drill_tenant_pat", ""))
    tenant_id = fixture["tenant_id"]

    if skip_network:
        return CheckResult(
            check_id="C1",
            name="CAS fixture reads",
            status="skipped",
            message="--skip-network",
            duration_ms=int((time.time() - t0) * 1000),
        )

    if not pat:
        return CheckResult(
            check_id="C1",
            name="CAS fixture reads",
            status="fail",
            message="no DRILL_TENANT_PAT available (env or fixture)",
            duration_ms=int((time.time() - t0) * 1000),
        )

    expected = [b["sha256"] for b in fixture["cas_blobs"]]
    mismatches: list[str] = []
    for sha in expected:
        url = f"{api_base}/v1/tenants/{tenant_id}/cas/{sha}"
        status, body = safe_http_get(url, headers={"Authorization": f"Bearer {pat}"})
        if status != 200:
            mismatches.append(f"sha={sha[:12]}.. http={status}")
            continue
        # Independently verify content hash.
        import hashlib

        actual = hashlib.sha256(body).hexdigest()
        if actual != sha:
            mismatches.append(f"sha={sha[:12]}.. body_hash_mismatch")

    dur = int((time.time() - t0) * 1000)
    if mismatches:
        return CheckResult(
            check_id="C1",
            name="CAS fixture reads",
            status="fail",
            message=f"{len(mismatches)}/{len(expected)} CAS reads failed",
            duration_ms=dur,
            extra={"mismatches": mismatches},
        )
    return CheckResult(
        check_id="C1",
        name="CAS fixture reads",
        status="pass",
        message=f"all {len(expected)} CAS blobs read + hash-verified",
        duration_ms=dur,
    )


def check_c2_byok_envelope(fixture: dict, skip_network: bool) -> CheckResult:
    """C2: BYOK envelope unwrap returns identical DEK SHA-256."""
    t0 = time.time()
    if skip_network:
        return CheckResult(
            check_id="C2",
            name="BYOK envelope unwrap",
            status="skipped",
            message="--skip-network",
            duration_ms=int((time.time() - t0) * 1000),
        )

    endpoint = os.environ.get(
        "BYOK_VERIFY_ENDPOINT",
        fixture.get(
            "slo_endpoints", {}
        ).get(
            "byok_unwrap_test",
            "https://api.corelink.io/internal/byok/unwrap-test",
        ),
    )
    pat = os.environ.get(
        "BYOK_OPERATOR_PAT", fixture.get("byok_operator_pat", "")
    )
    expected_dek = fixture["byok_envelope"].get("dek_sha256_for_verification", "")
    tenant_id = fixture["tenant_id"]

    if not pat:
        return CheckResult(
            check_id="C2",
            name="BYOK envelope unwrap",
            status="fail",
            message="no BYOK_OPERATOR_PAT available",
            duration_ms=int((time.time() - t0) * 1000),
        )
    if not expected_dek:
        return CheckResult(
            check_id="C2",
            name="BYOK envelope unwrap",
            status="fail",
            message="fixture missing dek_sha256_for_verification",
            duration_ms=int((time.time() - t0) * 1000),
        )

    status, body = safe_http_post(
        endpoint,
        payload={"tenant_id": tenant_id},
        headers={"Authorization": f"Bearer {pat}"},
    )
    dur = int((time.time() - t0) * 1000)
    if status != 200:
        return CheckResult(
            check_id="C2",
            name="BYOK envelope unwrap",
            status="fail",
            message=f"unwrap endpoint http={status}",
            duration_ms=dur,
        )
    try:
        parsed = json.loads(body)
        actual_dek = parsed.get("dek_sha256", "")
    except (json.JSONDecodeError, AttributeError):
        return CheckResult(
            check_id="C2",
            name="BYOK envelope unwrap",
            status="fail",
            message="unwrap response not JSON or missing dek_sha256",
            duration_ms=dur,
        )

    if actual_dek != expected_dek:
        return CheckResult(
            check_id="C2",
            name="BYOK envelope unwrap",
            status="fail",
            message="DEK SHA-256 mismatch (envelope did NOT survive restore)",
            duration_ms=dur,
            extra={
                "expected_prefix": expected_dek[:12],
                "actual_prefix": actual_dek[:12],
            },
        )
    return CheckResult(
        check_id="C2",
        name="BYOK envelope unwrap",
        status="pass",
        message="DEK SHA-256 matches pre-drill fixture",
        duration_ms=dur,
    )


def check_c3_merkle_root(
    fixture: dict,
    expected_root_override: str | None,
    skip_network: bool,
) -> CheckResult:
    """C3: audit-chain Merkle root matches pre-drill checkpoint."""
    t0 = time.time()
    expected_root = (
        expected_root_override
        or fixture["audit_chain_checkpoint"].get("merkle_root", "")
    )

    if not expected_root:
        return CheckResult(
            check_id="C3",
            name="Audit-chain Merkle root",
            status="fail",
            message="no expected Merkle root (fixture nor override)",
            duration_ms=int((time.time() - t0) * 1000),
        )

    if skip_network:
        return CheckResult(
            check_id="C3",
            name="Audit-chain Merkle root",
            status="skipped",
            message="--skip-network",
            duration_ms=int((time.time() - t0) * 1000),
        )

    endpoint = fixture["slo_endpoints"].get(
        "audit_chain_root",
        "https://api.corelink.io/internal/audit-chain/root",
    )
    epoch = fixture["audit_chain_checkpoint"].get("epoch", "")
    url = f"{endpoint}?epoch={epoch}" if epoch else endpoint
    status, body = safe_http_get(url)
    dur = int((time.time() - t0) * 1000)

    if status != 200:
        return CheckResult(
            check_id="C3",
            name="Audit-chain Merkle root",
            status="fail",
            message=f"audit-chain root endpoint http={status}",
            duration_ms=dur,
        )

    try:
        parsed = json.loads(body)
        actual_root = parsed.get("merkle_root", "")
    except (json.JSONDecodeError, AttributeError):
        return CheckResult(
            check_id="C3",
            name="Audit-chain Merkle root",
            status="fail",
            message="audit-chain root response not JSON",
            duration_ms=dur,
        )

    if actual_root != expected_root:
        return CheckResult(
            check_id="C3",
            name="Audit-chain Merkle root",
            status="fail",
            message="Merkle root mismatch (audit chain integrity violated)",
            duration_ms=dur,
            extra={
                "expected": expected_root,
                "actual": actual_root,
            },
        )
    return CheckResult(
        check_id="C3",
        name="Audit-chain Merkle root",
        status="pass",
        message="Merkle root matches pre-drill checkpoint",
        duration_ms=dur,
    )


def check_c4_slo_histograms(fixture: dict, skip_network: bool) -> CheckResult:
    """C4: SLO Prometheus histograms have non-zero buckets in last 15min."""
    t0 = time.time()
    if skip_network:
        return CheckResult(
            check_id="C4",
            name="SLO histograms recovered",
            status="skipped",
            message="--skip-network",
            duration_ms=int((time.time() - t0) * 1000),
        )

    prom_url = os.environ.get(
        "PROMETHEUS_URL",
        fixture.get("slo_endpoints", {}).get(
            "prometheus_query",
            "https://prometheus.corelink.io/api/v1/query",
        ),
    )
    series = [
        "gw_request_latency_seconds_bucket",
        "audit_chain_append_latency_seconds_bucket",
        "byok_envelope_unwrap_latency_seconds_bucket",
    ]
    missing: list[str] = []
    for s in series:
        query = f'sum(increase({s}[15m])) > 0'
        url = f"{prom_url}?query={urllib.request.quote(query)}"
        status, body = safe_http_get(url)
        if status != 200:
            missing.append(f"{s}: http={status}")
            continue
        try:
            parsed = json.loads(body)
            result = parsed.get("data", {}).get("result", [])
            if not result:
                missing.append(f"{s}: empty result")
        except (json.JSONDecodeError, AttributeError):
            missing.append(f"{s}: response not JSON")

    dur = int((time.time() - t0) * 1000)
    if missing:
        return CheckResult(
            check_id="C4",
            name="SLO histograms recovered",
            status="fail",
            message=f"{len(missing)}/{len(series)} series missing",
            duration_ms=dur,
            extra={"missing": missing},
        )
    return CheckResult(
        check_id="C4",
        name="SLO histograms recovered",
        status="pass",
        message=f"all {len(series)} SLO series active in last 15min",
        duration_ms=dur,
    )


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


CHECK_EXIT_CODES = {"C1": 1, "C2": 2, "C3": 3, "C4": 4}


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(
        prog="verify-cold-restore.py",
        description="GAP-15 cold-restore verification gate",
    )
    p.add_argument("--env", required=True, help="Target env (staging|production)")
    p.add_argument(
        "--fixture",
        required=True,
        help="Path to pre-drill fixture JSON",
    )
    p.add_argument(
        "--merkle-root-expected",
        default=None,
        help="Override Merkle root from fixture (use captured pre-drill value)",
    )
    p.add_argument(
        "--output",
        default=None,
        help="Path to write JSON report (default: stdout only)",
    )
    p.add_argument(
        "--skip-network",
        action="store_true",
        help="Offline mode: only validate fixture format (returns pass on all checks)",
    )

    args = p.parse_args(argv)

    fixture_path = Path(args.fixture)
    fixture = load_fixture(fixture_path)

    report = VerificationReport(
        env=args.env,
        fixture_path=str(fixture_path),
        started_at_utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    )

    print(f"verify-cold-restore: env={args.env} fixture={fixture_path.name}")
    print(f"  tenant_id={fixture['tenant_id']}")
    print(f"  cas_blobs={len(fixture['cas_blobs'])}")
    print()

    checks = [
        check_c1_cas_fixtures(fixture, args.skip_network),
        check_c2_byok_envelope(fixture, args.skip_network),
        check_c3_merkle_root(fixture, args.merkle_root_expected, args.skip_network),
        check_c4_slo_histograms(fixture, args.skip_network),
    ]
    report.checks = checks

    first_fail = None
    for c in checks:
        marker = {"pass": "PASS", "fail": "FAIL", "skipped": "SKIP"}.get(
            c.status, "????"
        )
        print(f"  [{marker}] {c.check_id} {c.name} — {c.message} ({c.duration_ms}ms)")
        if c.extra:
            for k, v in c.extra.items():
                print(f"          {k}: {v}")
        if c.status == "fail" and first_fail is None:
            first_fail = c

    report.completed_at_utc = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())

    if first_fail is None:
        report.overall = "pass"
        print()
        print("verify-cold-restore: ALL CHECKS PASS")
        exit_code = 0
    else:
        report.overall = "fail"
        print()
        print(f"verify-cold-restore: FAIL on {first_fail.check_id}")
        exit_code = CHECK_EXIT_CODES.get(first_fail.check_id, 1)

    if args.output:
        try:
            out_path = Path(args.output)
            out_path.write_text(
                json.dumps(report.as_dict(), indent=2, sort_keys=True),
                encoding="utf-8",
            )
            print(f"verify-cold-restore: report written to {out_path}")
        except OSError as exc:
            print(f"WARN: could not write report to {args.output}: {exc}")
            # Do not flip exit code; report failure is non-fatal.

    return exit_code


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
