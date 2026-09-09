#!/usr/bin/env python3
"""Probe and guard the B-046 Object-Lock capability boundary.

The repository cannot turn R2's ``NotImplemented`` response into a compliance
guarantee.  This module therefore has two deliberately separate modes:

* the default mode checks the repository contract and remains green while
  B-046 is truthfully open/blocked;
* ``--probe`` performs the two destructive S3 capability probes, but only after
  an operator explicitly opts into creating a probe bucket.  A successful
  command is evidence only for that provider/account, never for R2 generally.

Unknown errors are ``INDETERMINATE``.  Only an explicit provider
``NotImplemented`` response is classified as ``NOT_SUPPORTED``.  This is a
truth boundary, not a fake Object-Lock implementation.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Mapping, Sequence


ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = ROOT / "scripts/verify_b046_object_lock_probe.py"
BACKLOG_PATH = ROOT / "BACKLOG.md"
OKF_PATH = ROOT / "docs/knowledge/ops/r2-object-lock-probe.md"
ADR_PATH = ROOT / "specs/03_architecture/adrs/ADR-0100-r2-object-lock-capability-gate.md"
CHANGELOG_PATH = ROOT / "changelog.d/b046-r2-object-lock-reprobe.md"
WORKFLOW_PATH = ROOT / ".github/workflows/backlog-verify.yml"
ADAPTER_PATH = ROOT / "crates/corelink-container/src/routes/dsr/adapter_r2_cas_legalhold.rs"
MIGRATION_PATH = ROOT / "migrations/d1/0102_cas_retention.sql"

NOT_SUPPORTED_RE = re.compile(r"\bNotImplemented\b|\bNot[ \t]+Implemented\b", re.IGNORECASE)
SAFE_BUCKET_RE = re.compile(r"^corelink-b046-probe-[a-z0-9-]{3,50}$")
BACKLOG_BLOCK_RE = re.compile(r"^```backlog\n(.*?)^```", re.MULTILINE | re.DOTALL)
B046_VERIFY_COMMAND = "python3 scripts/verify_owner_action_packets.py --id B-046"


class ProbeError(RuntimeError):
    """A malformed local probe contract, not a provider result."""


@dataclass(frozen=True)
class OperationResult:
    name: str
    status: str
    returncode: int
    detail: str


def classify_operation(returncode: int, stdout: str = "", stderr: str = "") -> str:
    """Classify one provider call without turning permission errors into proof."""
    if returncode == 0:
        return "PASS"
    if NOT_SUPPORTED_RE.search(f"{stdout}\n{stderr}"):
        return "NOT_SUPPORTED"
    return "INDETERMINATE"


def _b046_record(backlog: str) -> str:
    """Return the one fenced B-046 record, rejecting ambiguous population."""
    matches = [
        match.group(1)
        for match in BACKLOG_BLOCK_RE.finditer(backlog)
        if re.search(r"^id:\s*B-046\s*$", match.group(1), re.MULTILINE)
    ]
    if len(matches) != 1:
        raise ProbeError(f"expected exactly one fenced B-046 backlog record, found {len(matches)}")
    return matches[0]


def _record_field(record: str, field: str) -> str:
    match = re.search(rf"^{re.escape(field)}:\s*(.*?)\s*$", record, re.MULTILINE)
    if not match:
        raise ProbeError(f"B-046 backlog record is missing exact field {field!r}")
    return match.group(1)


def evaluate_operations(create: OperationResult, put: OperationResult) -> str:
    """Return the conservative aggregate capability verdict."""
    statuses = {create.status, put.status}
    if "INDETERMINATE" in statuses:
        return "INDETERMINATE"
    if "NOT_SUPPORTED" in statuses:
        return "BLOCKED"
    if "SKIPPED" in statuses:
        return "INDETERMINATE"
    if statuses == {"PASS"}:
        return "SUPPORTED"
    raise ProbeError(f"unknown operation statuses: {sorted(statuses)}")


def redact(value: str, secrets: Sequence[str]) -> str:
    result = value
    for secret in secrets:
        if secret:
            result = result.replace(secret, "<redacted>")
    return result[-600:]


def validate_no_secrets_in_command(command: Sequence[str], secrets: Sequence[str]) -> None:
    """Reject credential-shaped AWS argv before spawning the child process."""
    if any(secret and any(secret in argument for argument in command) for secret in secrets):
        raise ProbeError("probe credentials must be supplied via the child environment, never argv")


def _aws_child_env(access_key: str, secret_key: str, session_token: str) -> dict[str, str]:
    """Build the AWS CLI environment without exposing credentials in argv/logs."""
    child_env = os.environ.copy()
    child_env["AWS_ACCESS_KEY_ID"] = access_key
    child_env["AWS_SECRET_ACCESS_KEY"] = secret_key
    # Remove an ambient token when this probe did not receive one, so the
    # explicitly supplied key pair cannot be silently combined with stale auth.
    if session_token:
        child_env["AWS_SESSION_TOKEN"] = session_token
    else:
        child_env.pop("AWS_SESSION_TOKEN", None)
    for source_name in ("R2_S3_ACCESS_KEY_ID", "R2_S3_SECRET_ACCESS_KEY", "R2_S3_SESSION_TOKEN"):
        child_env.pop(source_name, None)
    return child_env


def _run(
    command: Sequence[str], secrets: Sequence[str], child_env: Mapping[str, str]
) -> OperationResult:
    validate_no_secrets_in_command(command, secrets)
    name = command[3] if len(command) > 3 else "aws"
    try:
        completed = subprocess.run(
            command,
            capture_output=True,
            text=True,
            check=False,
            env=dict(child_env),
        )
    except OSError as error:
        return OperationResult(name, "INDETERMINATE", 127, redact(str(error), secrets))
    status = classify_operation(completed.returncode, completed.stdout, completed.stderr)
    detail = redact((completed.stdout + "\n" + completed.stderr).strip(), secrets)
    return OperationResult(name, status, completed.returncode, detail)


def _probe_config() -> tuple[str, str, str, str, str, str]:
    endpoint = os.environ.get("R2_S3_ENDPOINT", "").strip()
    access_key = os.environ.get("R2_S3_ACCESS_KEY_ID", "").strip()
    secret_key = os.environ.get("R2_S3_SECRET_ACCESS_KEY", "").strip()
    session_token = os.environ.get("R2_S3_SESSION_TOKEN", "").strip()
    bucket = os.environ.get("B046_PROBE_BUCKET", "").strip()
    region = os.environ.get("B046_PROBE_REGION", "auto").strip() or "auto"
    if not endpoint or not access_key or not secret_key or not bucket:
        raise ProbeError(
            "B046_PROBE_BUCKET, R2_S3_ENDPOINT, R2_S3_ACCESS_KEY_ID, and "
            "R2_S3_SECRET_ACCESS_KEY are required; no provider claim was made"
        )
    if not endpoint.startswith("https://"):
        raise ProbeError("R2_S3_ENDPOINT must use https")
    if not SAFE_BUCKET_RE.fullmatch(bucket):
        raise ProbeError("B046_PROBE_BUCKET must use the corelink-b046-probe-* safety prefix")
    return endpoint, access_key, secret_key, session_token, bucket, region


def run_external_probe() -> dict[str, object]:
    """Run both Object-Lock probes after explicit operator opt-in."""
    if os.environ.get("B046_PROBE_ALLOW_MUTATION") != "1":
        return {
            "verdict": "INDETERMINATE",
            "reason": "set B046_PROBE_ALLOW_MUTATION=1 for the explicit bucket-creating probe",
        }
    endpoint, access_key, secret_key, session_token, bucket, region = _probe_config()
    retain_until = (dt.datetime.now(dt.timezone.utc) + dt.timedelta(days=2)).replace(microsecond=0)
    retain_until_text = retain_until.isoformat().replace("+00:00", "Z")
    key = "b046-capability-probe/object-lock.txt"
    secrets = (access_key, secret_key, session_token)
    child_env = _aws_child_env(access_key, secret_key, session_token)
    common = (
        "aws",
        "--endpoint-url",
        endpoint,
        "--region",
        region,
        "--no-cli-pager",
        "--output",
        "json",
    )
    create = _run(
        (*common, "s3api", "create-bucket", "--bucket", bucket, "--object-lock-enabled-for-bucket"),
        secrets,
        child_env,
    )
    if create.status != "PASS":
        put = OperationResult(
            "put-object",
            "SKIPPED",
            125,
            "not attempted because CreateBucket did not prove Object-Lock support",
        )
    else:
        with tempfile.NamedTemporaryFile("w", encoding="utf-8", delete=False) as body:
            body.write("corelink B-046 capability probe\n")
            body_path = body.name
        try:
            put = _run(
                (
                    *common,
                    "s3api",
                    "put-object",
                    "--bucket",
                    bucket,
                    "--key",
                    key,
                    "--body",
                    body_path,
                    "--object-lock-mode",
                    "COMPLIANCE",
                    "--object-lock-retain-until-date",
                    retain_until_text,
                ),
                secrets,
                child_env,
            )
        finally:
            Path(body_path).unlink(missing_ok=True)
    verdict = evaluate_operations(create, put)
    return {
        "verdict": verdict,
        "evidence_external": verdict == "SUPPORTED",
        "provider": "the configured S3-compatible endpoint only",
        "bucket": bucket,
        "retain_until": retain_until_text,
        "operations": [create.__dict__, put.__dict__],
        "warning": "SUPPORTED is not a product claim until an owner provisions and reviews this backend",
    }


def _required_markers() -> Mapping[str, tuple[str, ...]]:
    return {
        BACKLOG_PATH.as_posix(): (
            "id: B-046",
            "NotImplemented",
            "INDETERMINATE",
            "does not claim Compliance mode",
        ),
        OKF_PATH.as_posix(): (
            "B-046",
            "INDETERMINATE",
            "NotImplemented",
            "legal hold",
            "Governance",
            "Bucket Lock",
            "administrator-removable",
        ),
        ADR_PATH.as_posix(): (
            "ADR-0100",
            "DEFERRED / BLOCKED",
            "NotImplemented",
            "fail-closed",
            "legal-hold",
            "administrator-removable",
        ),
        CHANGELOG_PATH.as_posix(): (
            "B-046",
            "blocked",
            "NotImplemented",
            "does not claim a Compliance guarantee",
        ),
        WORKFLOW_PATH.as_posix(): (
            "scripts/verify_b046_object_lock_probe.py",
            "tests/test_verify_b046_object_lock_probe.py",
            "crates/corelink-container/src/routes/dsr/adapter_r2_cas_legalhold.rs",
            "migrations/d1/0102_cas_retention.sql",
        ),
        ADAPTER_PATH.as_posix(): (
            "legal_hold == true",
            "NOT storage immutability",
            "subject_id",
        ),
        MIGRATION_PATH.as_posix(): (
            "CHECK (mode IN ('governance'))",
            "NOT storage-level immutability",
            "Compliance / Object-Lock mode",
        ),
    }


def validate_repository_contract(files: Mapping[str, str] | None = None) -> None:
    """Validate the honesty/coverage contract without contacting a provider."""
    texts: dict[str, str] = {}
    for path in (
        BACKLOG_PATH,
        OKF_PATH,
        ADR_PATH,
        CHANGELOG_PATH,
        WORKFLOW_PATH,
        ADAPTER_PATH,
        MIGRATION_PATH,
    ):
        key = path.as_posix()
        texts[key] = files[key] if files is not None and key in files else path.read_text(encoding="utf-8")
    for path, markers in _required_markers().items():
        text = texts[path]
        missing = [marker for marker in markers if marker not in text]
        if missing:
            raise ProbeError(f"{path} missing contract markers: {', '.join(missing)}")
    backlog = texts[BACKLOG_PATH.as_posix()]
    b046 = _b046_record(backlog)
    if _record_field(b046, "status") != "parked":
        raise ProbeError("B-046 status must remain parked without external provider evidence")
    if _record_field(b046, "verify") != B046_VERIFY_COMMAND:
        raise ProbeError(f"B-046 verify must remain {B046_VERIFY_COMMAND!r}")
    compact_b046 = " ".join(b046.split())
    if "R2 does not implement S3 Object Lock" not in compact_b046:
        raise ProbeError("B-046 must preserve the current R2 platform blocker")
    if "compliance" in compact_b046.lower() and "does not claim Compliance mode" not in compact_b046:
        raise ProbeError("B-046 backlog contract lost its no-claim marker")


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", action="store_true", help="run the explicitly opt-in external S3 probes")
    args = parser.parse_args(argv)
    try:
        validate_repository_contract()
        if not args.probe:
            print("B-046 contract confirmed: R2 Object-Lock remains open/blocked; no compliance guarantee")
            return 0
        print(json.dumps(run_external_probe(), sort_keys=True))
        return 0
    except (OSError, ProbeError) as error:
        print(f"B-046 INDETERMINATE: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
