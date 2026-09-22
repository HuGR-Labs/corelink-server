#!/usr/bin/env python3
"""Capture a redacted, read-only Cloudflare Containers capacity receipt."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import tomllib
from datetime import datetime, timezone
from pathlib import Path


APP_NAMES = (
    "corelink-prod-corelinkserver-prod",
    "corelink-prod-sam-corelinkserver-prod",
    "corelink-prod-lhr-corelinkserver-prod",
    "corelink-prod-nrt-corelinkserver-prod",
    "corelink-prod-syd-corelinkserver-prod",
)
ENVIRONMENTS = ("prod", "prod-sam", "prod-lhr", "prod-nrt", "prod-syd")
INSTANCE_STATES = frozenset(
    {"provisioning", "running", "failed", "stopping", "stopped", "unhealthy", "inactive", "unknown"}
)
ACCOUNT_ID_RE = re.compile(r"^[0-9a-f]{32}$")
APP_ID_RE = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
)
MAX_OUTPUT_BYTES = 1_048_576
ACCOUNT_LIMITS = {"vcpu": 1500, "memory_tib": 6, "disk_tb": 30}
INSTANCE_SHAPE = {"type": "basic", "vcpu": 0.25, "memory_mib": 1024, "disk_gb": 4}
MAX_INSTANCES_PER_APP = 200
ROOT = Path(__file__).resolve().parents[1]


class ProbeError(RuntimeError):
    pass


def _digest(value: object) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def _parse_json(raw: bytes) -> object:
    if len(raw) > MAX_OUTPUT_BYTES:
        raise ProbeError("provider response exceeded the bounded size")
    try:
        return json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ProbeError("provider response was not valid JSON") from exc


def _run_wrangler(args: list[str]) -> bytes:
    command = ["pnpm", "exec", "wrangler", *args]
    try:
        result = subprocess.run(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            check=False,
            timeout=90,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise ProbeError("read-only provider command unavailable") from exc
    if result.returncode != 0:
        raise ProbeError("read-only provider command failed")
    return result.stdout


def _application_ids(payload: object) -> dict[str, str]:
    if not isinstance(payload, list):
        raise ProbeError("application list shape is invalid")
    found: dict[str, str] = {}
    for row in payload:
        if not isinstance(row, dict):
            raise ProbeError("application row shape is invalid")
        name, app_id = row.get("name"), row.get("id")
        if not isinstance(name, str) or not isinstance(app_id, str) or not APP_ID_RE.fullmatch(app_id):
            raise ProbeError("application identity shape is invalid")
        if name in APP_NAMES:
            if name in found:
                raise ProbeError("duplicate production application")
            found[name] = app_id
    if set(found) != set(APP_NAMES):
        raise ProbeError("production application allowlist did not resolve exactly")
    return found


def _state_counts(payload: object) -> dict[str, int]:
    if not isinstance(payload, list):
        raise ProbeError("instance list shape is invalid")
    counts = {state: 0 for state in sorted(INSTANCE_STATES)}
    for row in payload:
        if not isinstance(row, dict):
            raise ProbeError("instance row shape is invalid")
        state = row.get("state")
        if not isinstance(state, str) or state not in INSTANCE_STATES:
            raise ProbeError("instance state is absent or unknown")
        counts[state] += 1
    return counts


def _verify_declared_capacity() -> None:
    try:
        config = tomllib.loads((ROOT / "wrangler.toml").read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as exc:
        raise ProbeError("repository capacity declaration is unavailable") from exc
    envs = config.get("env")
    if not isinstance(envs, dict):
        raise ProbeError("repository production environments are unavailable")
    for environment, expected_name in zip(ENVIRONMENTS, APP_NAMES, strict=True):
        section = envs.get(environment)
        containers = section.get("containers") if isinstance(section, dict) else None
        if not isinstance(containers, list) or len(containers) != 1 or not isinstance(containers[0], dict):
            raise ProbeError("repository production container declaration drifted")
        container = containers[0]
        image = container.get("image")
        image_name = image.rsplit("/", 1)[-1].split(":", 1)[0] if isinstance(image, str) else None
        if (
            container.get("class_name") != "CoreLinkServer"
            or container.get("instance_type") != INSTANCE_SHAPE["type"]
            or container.get("max_instances") != MAX_INSTANCES_PER_APP
            or image_name != expected_name
        ):
            raise ProbeError("repository production capacity declaration drifted")


def _write_receipt(path: Path, receipt: dict[str, object]) -> None:
    if path.is_symlink() or (path.exists() and not path.is_file()):
        raise ProbeError("receipt output must be a regular non-symlink file")
    path.parent.mkdir(parents=True, exist_ok=True)
    receipt["receipt_sha256"] = _digest(receipt)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump(receipt, handle, indent=2, sort_keys=True)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def capture(account_id: str, source_sha: str, output: Path) -> dict[str, object]:
    if not ACCOUNT_ID_RE.fullmatch(account_id):
        raise ProbeError("provider account scope is absent or malformed")
    if not re.fullmatch(r"[0-9a-f]{40}", source_sha):
        raise ProbeError("source commit identity is absent or malformed")
    _verify_declared_capacity()

    apps = _application_ids(_parse_json(_run_wrangler(["containers", "list", "--json"])))
    per_app: list[dict[str, int]] = []
    for name in APP_NAMES:
        raw = _run_wrangler(["containers", "instances", apps[name], "--json"])
        per_app.append(_state_counts(_parse_json(raw)))

    running = sum(row["running"] for row in per_app)
    receipt: dict[str, object] = {
        "schema": "corelink.issue-2075.capacity-read-only.v1",
        "schema_version": 1,
        "issue": 2075,
        "read_only": True,
        "captured_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_sha": source_sha,
        "account_id_redacted": f"{account_id[:4]}...{account_id[-4:]}",
        "source": "Cloudflare Wrangler containers list/instances --json",
        "account_limits": {
            "scope": "published per-account concurrent platform limits",
            "source_updated": "2026-08-28",
            **ACCOUNT_LIMITS,
            "account_specific_entitlement_verified": False,
        },
        "configured_production_ceiling": {
            "application_count": len(APP_NAMES),
            "max_instances_per_application": MAX_INSTANCES_PER_APP,
            "instance_shape": INSTANCE_SHAPE,
            "max_instances_total": len(APP_NAMES) * MAX_INSTANCES_PER_APP,
            "reserved_vcpu": len(APP_NAMES) * MAX_INSTANCES_PER_APP * INSTANCE_SHAPE["vcpu"],
            "reserved_memory_gib": len(APP_NAMES) * MAX_INSTANCES_PER_APP,
            "reserved_disk_gb": len(APP_NAMES) * MAX_INSTANCES_PER_APP * INSTANCE_SHAPE["disk_gb"],
            "source": "wrangler.toml production container declarations",
        },
        "observed_production_instances": {
            "scope": "five allowlisted production CoreLink applications; not the full Cloudflare account",
            "instance_states_by_application_in_allowlist_order": per_app,
            "running_instances": running,
            "running_instance_resource_allocation_estimate": {
                "vcpu": running * INSTANCE_SHAPE["vcpu"],
                "memory_mib": running * INSTANCE_SHAPE["memory_mib"],
                "disk_gb": running * INSTANCE_SHAPE["disk_gb"],
                "basis": "running instance count multiplied by declared basic instance shape; not provider account usage or billable consumption",
            },
        },
        "concurrency": {
            "active_tenant_region_assignments": None,
            "deduplicated_tenant_count": None,
            "status": "unavailable",
            "reason": "provider instance state and registered tenant state do not establish active tenant workload or tenant-to-instance assignments",
        },
    }
    _write_receipt(output, receipt)
    return receipt


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        token = os.environ.get("CLOUDFLARE_CAPACITY_READ_TOKEN", "")
        if not token:
            raise ProbeError("protected provider credential is absent")
        os.environ["CLOUDFLARE_API_TOKEN"] = token
        receipt = capture(
            os.environ.get("CLOUDFLARE_ACCOUNT_ID", ""),
            os.environ.get("GITHUB_SHA", ""),
            args.output,
        )
        instances = receipt["observed_production_instances"]
        print(f"issue-2075 capacity receipt: PASS; running_instances={instances['running_instances']}")
        return 0
    except ProbeError as exc:
        print(f"issue-2075 capacity receipt: FAIL-CLOSED: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
