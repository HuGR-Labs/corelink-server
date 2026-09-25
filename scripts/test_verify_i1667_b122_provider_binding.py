"""Mutation checks for the fail-closed B-122 provider binding verifier."""

from __future__ import annotations

from copy import deepcopy

import pytest

from scripts.verify_i1667_b122_provider_binding import BindingError, verify_binding


SHA = "af79ca2f296f39b8149931de16441f8e4582b53c"
VERSION_ID = "2d36f49d-06aa-4f4c-b4e2-25a6da2b9b10"
DEPLOYMENT_ID = "1d36f49d-06aa-4f4c-b4e2-25a6da2b9b10"
APP_ID = "4d36f49d-06aa-4f4c-b4e2-25a6da2b9b10"


def _evidence():
    deployments = {"success": True, "result": {"deployments": [{
        "id": DEPLOYMENT_ID, "created_on": "2026-09-25T08:00:00Z",
        "annotations": {"workers/message": f"corelink-source-sha={SHA}"},
        "versions": [{"version_id": VERSION_ID, "percentage": 100}],
    }]}}
    version = {"success": True, "result": {
        "id": VERSION_ID, "metadata": {"created_on": "2026-09-25T08:00:00Z"},
    }}
    app = {"success": True, "result": {
        "id": APP_ID, "version": 101,
        "configuration": {"image": f"registry.cloudflare.com/6a1fc1c626fc2628823e60b9db01f5cd/corelink-prod-corelinkserver-prod:{SHA[:9]}-r1"},
        "health": {"instances": {"healthy": 1, "failed": 0}},
    }}
    builds = {"workflow_runs": [{"head_sha": SHA, "conclusion": "success", "event": "workflow_dispatch"}]}
    image_commit = {"sha": SHA}
    return deployments, version, app, image_commit, builds, [
        {"id": 42, "environment": "production", "sha": SHA, "created_at": "2026-09-25T08:00:00Z"}
    ], [{"state": "success"}]


def test_provider_version_and_successful_github_deployment_bind_source_sha() -> None:
    deployments, version, app, image_commit, builds, github_deployments, statuses = _evidence()
    receipt = verify_binding(
        deployments=deployments,
        version=version,
        github_deployments=github_deployments,
        github_statuses=statuses,
        container_application=app,
        container_image_commit=image_commit,
        container_build_runs=builds,
        worker_name="corelink-prod",
        source_sha=SHA,
    )
    assert receipt["worker_version_id"] == VERSION_ID
    assert receipt["container_application_id"] == APP_ID
    assert receipt["container_application_version"] == "101"
    assert receipt["container_image"].endswith(f"{SHA[:9]}-r1")
    assert receipt["provider_deployment_id"] == DEPLOYMENT_ID
    assert receipt["source_sha"] == SHA == receipt["github_deployment_sha"]
    assert receipt["github_deployment_status"] == "success"


@pytest.mark.parametrize("mutation", ["partial", "annotation", "metadata", "sha", "failure"])
def test_provider_or_github_mismatch_is_rejected(mutation: str) -> None:
    deployments, version, app, image_commit, builds, github_deployments, statuses = deepcopy(_evidence())
    if mutation == "partial":
        deployments["result"]["deployments"][0]["versions"][0]["percentage"] = 99
    elif mutation == "annotation":
        deployments["result"]["deployments"][0]["annotations"]["workers/message"] = "corelink-source-sha=" + "0" * 40
    elif mutation == "metadata":
        version["result"].pop("metadata")
    elif mutation == "sha":
        github_deployments[0]["sha"] = "0" * 40
    else:
        statuses[0]["state"] = "failure"
    with pytest.raises(BindingError):
        verify_binding(
            deployments=deployments,
            version=version,
            github_deployments=github_deployments,
            github_statuses=statuses,
            container_application=app,
            container_image_commit=image_commit,
            container_build_runs=builds,
            worker_name="corelink-prod",
            source_sha=SHA,
        )


def test_active_deployment_with_multiple_versions_is_rejected() -> None:
    deployments, version, app, image_commit, builds, github_deployments, statuses = _evidence()
    deployments["result"]["deployments"][0]["versions"].append(
        {"version_id": "3d36f49d-06aa-4f4c-b4e2-25a6da2b9b10", "percentage": 0}
    )
    with pytest.raises(BindingError):
        verify_binding(
            deployments=deployments,
            version=version,
            github_deployments=github_deployments,
            github_statuses=statuses,
            container_application=app,
            container_image_commit=image_commit,
            container_build_runs=builds,
            worker_name="corelink-prod",
            source_sha=SHA,
        )


@pytest.mark.parametrize("mutation", ["image_sha", "application_version", "health", "missing_build"])
def test_container_runtime_without_bound_build_is_rejected(mutation: str) -> None:
    deployments, version, app, image_commit, builds, github_deployments, statuses = deepcopy(_evidence())
    if mutation == "image_sha":
        app["result"]["configuration"]["image"] = app["result"]["configuration"]["image"].replace(SHA[:9], "0" * 9)
    elif mutation == "application_version":
        app["result"]["version"] = 0
    elif mutation == "health":
        app["result"]["health"]["instances"]["failed"] = 1
    else:
        builds["workflow_runs"] = []
    with pytest.raises(BindingError):
        verify_binding(
            deployments=deployments,
            version=version,
            github_deployments=github_deployments,
            github_statuses=statuses,
            container_application=app,
            container_image_commit=image_commit,
            container_build_runs=builds,
            worker_name="corelink-prod",
            source_sha=SHA,
        )
