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
B122_CONTAINER_SOURCE_SHA = "a5d56cb4516a2c11f5234eb83a738baa97a41c3d"
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


def _active_version(document: Any) -> tuple[str, str, str, str]:
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
    annotations = deployment.get("annotations")
    message = annotations.get("workers/message") if isinstance(annotations, dict) else None
    if not isinstance(message, str):
        raise BindingError("active Cloudflare deployment has no source-SHA annotation")
    return deployment_id, version_id, created_on, message


def verify_binding(
    *,
    deployments: Any,
    version: Any,
    github_deployments: Any,
    github_statuses: Any,
    container_application: Any,
    container_image_commit: Any,
    container_build_runs: Any,
    build_is_ancestor: Any,
    b122_is_ancestor: Any,
    worker_name: str,
    worker_serving_sha: str,
) -> dict[str, Any]:
    if not re.fullmatch(r"[a-z0-9_-]{1,63}", worker_name):
        raise BindingError("Worker name has unsupported characters")
    if not _SHA.fullmatch(worker_serving_sha):
        raise BindingError("Worker serving SHA must be 40 lowercase hexadecimal characters")

    app = _provider_result(container_application, "container application")
    if not isinstance(app, dict):
        raise BindingError("Cloudflare container application is malformed")
    app_id, app_version = app.get("id"), app.get("version")
    if not isinstance(app_id, str) or not _UUID.fullmatch(app_id):
        raise BindingError("Cloudflare container application ID is not a UUID")
    if isinstance(app_version, bool) or not isinstance(app_version, (int, str)) or str(app_version) in ("", "0"):
        raise BindingError("Cloudflare container application has no active version")
    image = (app.get("configuration") or {}).get("image") if isinstance(app.get("configuration"), dict) else None
    expected_image_prefix = f"registry.cloudflare.com/"
    if not isinstance(image, str) or not image.startswith(expected_image_prefix):
        raise BindingError("Cloudflare container application has no registry image reference")
    match = re.fullmatch(r"registry\.cloudflare\.com/([0-9a-f]{32})/([a-z0-9-]+):([0-9a-f]{7,40})-r1", image)
    if not match or match.group(2) != f"{worker_name}-corelinkserver-prod":
        raise BindingError("active container image does not belong to the requested production Worker")
    image_source_prefix = match.group(3)
    resolved_image_commit = container_image_commit.get("sha") if isinstance(container_image_commit, dict) else None
    if (not isinstance(resolved_image_commit, str) or not _SHA.fullmatch(resolved_image_commit)
            or not resolved_image_commit.startswith(image_source_prefix)):
        raise BindingError("container image tag does not resolve to its full build SHA")
    health = app.get("health")
    instances = health.get("instances") if isinstance(health, dict) else None
    if (not isinstance(instances, dict) or type(instances.get("healthy")) is not int
            or instances["healthy"] < 1 or type(instances.get("failed")) is not int
            or instances["failed"] != 0):
        raise BindingError("active container application is not reporting healthy instances")
    builds = container_build_runs.get("workflow_runs") if isinstance(container_build_runs, dict) else None
    successful_builds = [run for run in builds if isinstance(run, dict)
        and run.get("head_sha") == resolved_image_commit
        and run.get("conclusion") == "success" and run.get("event") == "workflow_dispatch"
        and isinstance(run.get("id"), int) and run["id"] > 0] if isinstance(builds, list) else []
    if not successful_builds:
        raise BindingError("no successful container image build is bound to the tag's build SHA")
    build_id = successful_builds[0]["id"]

    def require_ancestor(comparison: Any, base: str, head: str, label: str) -> None:
        base_commit = comparison.get("base_commit") if isinstance(comparison, dict) else None
        merge_base = comparison.get("merge_base_commit") if isinstance(comparison, dict) else None
        status = comparison.get("status") if isinstance(comparison, dict) else None
        commits = comparison.get("commits") if isinstance(comparison, dict) else None
        head_matches = (base == head if status == "identical" else
                        isinstance(commits, list) and bool(commits)
                        and isinstance(commits[-1], dict) and commits[-1].get("sha") == head)
        if (status not in ("ahead", "identical") or not isinstance(base_commit, dict)
                or base_commit.get("sha") != base or not isinstance(merge_base, dict)
                or merge_base.get("sha") != base or not head_matches):
            raise BindingError(f"GitHub does not prove {label} is an ancestor")

    require_ancestor(b122_is_ancestor, B122_CONTAINER_SOURCE_SHA, resolved_image_commit, "B-122 container source")
    require_ancestor(build_is_ancestor, resolved_image_commit, worker_serving_sha, "container build SHA")

    provider_deployment_id, version_id, deployment_created_on, message = _active_version(deployments)
    version_result = _provider_result(version, "version")
    if not isinstance(version_result, dict) or version_result.get("id") != version_id:
        raise BindingError("Worker version read does not match the active deployment")
    if message != f"corelink-source-sha={worker_serving_sha}":
        raise BindingError("active Worker deployment source annotation does not match source SHA")

    if not isinstance(github_deployments, list):
        raise BindingError("GitHub deployment response is malformed")
    matches = [
        deployment
        for deployment in github_deployments
        if isinstance(deployment, dict)
        and deployment.get("environment") == "production"
        and deployment.get("sha") == worker_serving_sha
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

    metadata = version_result.get("metadata")
    version_created_on = metadata.get("created_on") if isinstance(metadata, dict) else None
    if not isinstance(version_created_on, str) or not version_created_on:
        raise BindingError("Cloudflare version has no creation timestamp")
    return {
        "schema": "corelink.b122.provider-binding.v1",
        "worker_name": worker_name,
        "container_application_id": app_id,
        "container_application_version": str(app_version),
        "container_image": image,
        "container_build_sha": resolved_image_commit,
        "container_build_run_id": build_id,
        "worker_serving_sha": worker_serving_sha,
        "container_b122_source_sha": B122_CONTAINER_SOURCE_SHA,
        "container_healthy_instances": instances["healthy"],
        "worker_version_id": version_id,
        "worker_version_created_on": version_created_on,
        "provider_deployment_id": provider_deployment_id,
        "provider_deployment_created_on": deployment_created_on,
        "github_deployment_id": github_deployment["id"],
        "github_deployment_created_at": github_created_at,
        "github_deployment_sha": github_deployment["sha"],
        "github_deployment_status": status["state"],
        "binding_verified_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--worker-name", required=True)
    parser.add_argument("--deployments", type=Path, required=True)
    parser.add_argument("--version", type=Path, required=True)
    parser.add_argument("--github-deployments", type=Path, required=True)
    parser.add_argument("--github-statuses", type=Path, required=True)
    parser.add_argument("--container-application", type=Path, required=True)
    parser.add_argument("--container-image-commit", type=Path, required=True)
    parser.add_argument("--container-build-runs", type=Path, required=True)
    parser.add_argument("--build-is-ancestor", type=Path, required=True)
    parser.add_argument("--b122-is-ancestor", type=Path, required=True)
    parser.add_argument("--worker-serving-sha", required=True)
    args = parser.parse_args()
    try:
        receipt = verify_binding(
            deployments=_load(args.deployments),
            version=_load(args.version),
            github_deployments=_load(args.github_deployments),
            github_statuses=_load(args.github_statuses),
            container_application=_load(args.container_application),
            container_image_commit=_load(args.container_image_commit),
            container_build_runs=_load(args.container_build_runs),
            build_is_ancestor=_load(args.build_is_ancestor),
            b122_is_ancestor=_load(args.b122_is_ancestor),
            worker_name=args.worker_name,
            worker_serving_sha=args.worker_serving_sha,
        )
    except BindingError as exc:
        parser.error(str(exc))
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
