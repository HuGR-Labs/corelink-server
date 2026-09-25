#!/usr/bin/env python3
"""Bind a B-122 receipt to the active Cloudflare Worker and GitHub deployment.

The inputs are read-only API responses captured by the protected evidence lane.
Only the minimal verified binding is emitted; raw provider responses are never
uploaded with the timing receipt.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


class BindingError(ValueError):
    pass


_SHA = re.compile(r"^[0-9a-f]{40}$")
_UUID = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
    re.IGNORECASE,
)


def _load(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise BindingError(f"invalid JSON evidence: {path.name}") from exc


def _provider_result(document: Any, field: str) -> Any:
    if not isinstance(document, dict) or document.get("success") is not True:
        raise BindingError(f"Cloudflare {field} read did not succeed")
    return document.get("result")


def _active_version(document: Any) -> tuple[str, str, str]:
    result = _provider_result(document, "deployment")
    deployments = result.get("deployments") if isinstance(result, dict) else result
    if not isinstance(deployments, list) or not deployments:
        raise BindingError("Cloudflare returned no active Worker deployment")
    deployment = deployments[0]
    if not isinstance(deployment, dict):
        raise BindingError("Cloudflare active deployment is malformed")
    deployment_id = deployment.get("id")
    versions = deployment.get("versions")
    if not isinstance(deployment_id, str) or not _UUID.fullmatch(deployment_id):
        raise BindingError("Cloudflare deployment ID is not a UUID")
    if not isinstance(versions, list) or len(versions) != 1:
        raise BindingError("active Worker deployment must contain exactly one version")
    version = versions[0]
    if not isinstance(version, dict):
        raise BindingError("Cloudflare active version entry is malformed")
    version_id = version.get("version_id")
    percentage = version.get("percentage")
    if not isinstance(version_id, str) or not _UUID.fullmatch(version_id):
        raise BindingError("Cloudflare active version ID is not a UUID")
    if not isinstance(percentage, (int, float)) or percentage != 100:
        raise BindingError("Cloudflare active Worker version is not serving 100% of traffic")
    created_on = deployment.get("created_on")
    if not isinstance(created_on, str) or not created_on:
        raise BindingError("Cloudflare deployment has no creation timestamp")
    return deployment_id, version_id, created_on


def verify_binding(
    *,
    deployments: Any,
    version: Any,
    github_deployments: Any,
    github_statuses: Any,
    worker_name: str,
    source_sha: str,
) -> dict[str, Any]:
    if not re.fullmatch(r"[a-z0-9_-]{1,63}", worker_name):
        raise BindingError("Worker name has unsupported characters")
    if not _SHA.fullmatch(source_sha):
        raise BindingError("source SHA must be 40 lowercase hexadecimal characters")

    provider_deployment_id, version_id, deployment_created_on = _active_version(deployments)
    version_result = _provider_result(version, "version")
    if not isinstance(version_result, dict) or version_result.get("id") != version_id:
        raise BindingError("Worker version read does not match the active deployment")
    annotations = version_result.get("annotations")
    message = annotations.get("workers/message") if isinstance(annotations, dict) else None
    if message != f"corelink-source-sha={source_sha}":
        raise BindingError("active Worker version source annotation does not match source SHA")

    if not isinstance(github_deployments, list):
        raise BindingError("GitHub deployment response is malformed")
    matches = [
        deployment
        for deployment in github_deployments
        if isinstance(deployment, dict)
        and deployment.get("environment") == "production"
        and deployment.get("sha") == source_sha
        and isinstance(deployment.get("id"), int)
    ]
    if not matches:
        raise BindingError("no production GitHub deployment matches the source SHA")
    github_deployment = matches[0]
    github_created_at = github_deployment.get("created_at")
    if not isinstance(github_created_at, str) or not github_created_at:
        raise BindingError("matching GitHub deployment has no creation timestamp")
    if not isinstance(github_statuses, list) or not github_statuses:
        raise BindingError("matching GitHub deployment has no status receipt")
    status = github_statuses[0]
    if not isinstance(status, dict) or status.get("state") != "success":
        raise BindingError("latest matching GitHub deployment status is not successful")

    version_created_on = version_result.get("created_on")
    if not isinstance(version_created_on, str) or not version_created_on:
        raise BindingError("Cloudflare version has no creation timestamp")
    return {
        "schema": "corelink.b122.provider-binding.v1",
        "worker_name": worker_name,
        "worker_version_id": version_id,
        "worker_version_created_on": version_created_on,
        "provider_deployment_id": provider_deployment_id,
        "provider_deployment_created_on": deployment_created_on,
        "source_sha": source_sha,
        "github_deployment_id": github_deployment["id"],
        "github_deployment_created_at": github_created_at,
        "github_deployment_sha": github_deployment["sha"],
        "github_deployment_status": status["state"],
        "binding_verified_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--worker-name", required=True)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--deployments", type=Path, required=True)
    parser.add_argument("--version", type=Path, required=True)
    parser.add_argument("--github-deployments", type=Path, required=True)
    parser.add_argument("--github-statuses", type=Path, required=True)
    args = parser.parse_args()
    try:
        receipt = verify_binding(
            deployments=_load(args.deployments),
            version=_load(args.version),
            github_deployments=_load(args.github_deployments),
            github_statuses=_load(args.github_statuses),
            worker_name=args.worker_name,
            source_sha=args.source_sha,
        )
    except BindingError as exc:
        parser.error(str(exc))
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
