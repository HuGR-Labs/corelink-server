"""Mutation checks for the fail-closed B-122 provider binding verifier."""

from __future__ import annotations

from copy import deepcopy
import sys

import pytest

from scripts.verify_i1667_b122_provider_binding import BindingError, main, verify_binding


SHA = "af79ca2f296f39b8149931de16441f8e4582b53c"
BUILD_SHA = "123456789abcdef0123456789abcdef012345678"
VERSION_ID = "2d36f49d-06aa-4f4c-b4e2-25a6da2b9b10"
DEPLOYMENT_ID = "1d36f49d-06aa-4f4c-b4e2-25a6da2b9b10"
APP_ID = "4d36f49d-06aa-4f4c-b4e2-25a6da2b9b10"


def test_cli_requires_worker_serving_sha(monkeypatch) -> None:
    monkeypatch.setattr(sys, "argv", [
        "verify", "--worker-name", "corelink-prod", "--deployments", "d",
        "--version", "v", "--github-deployments", "gd", "--github-statuses", "gs",
        "--container-application", "ca", "--container-image-commit", "ci",
        "--container-build-runs", "cb", "--build-is-ancestor", "ba",
        "--b122-is-ancestor", "b2",
    ])
    with pytest.raises(SystemExit) as error:
        main()
    assert error.value.code == 2


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
        "configuration": {"image": f"registry.cloudflare.com/6a1fc1c626fc2628823e60b9db01f5cd/corelink-prod-corelinkserver-prod:{BUILD_SHA[:9]}-r1"},
        "health": {"instances": {"healthy": 1, "failed": 0}},
    }}
    builds = {"workflow_runs": [{"id": 43, "head_sha": BUILD_SHA, "conclusion": "success", "event": "workflow_dispatch"}]}
    image_commit = {"sha": BUILD_SHA}
    build_ancestor = {"status": "ahead", "base_commit": {"sha": BUILD_SHA}, "merge_base_commit": {"sha": BUILD_SHA}, "commits": [{"sha": SHA}]}
    b122_ancestor = {"status": "ahead", "base_commit": {"sha": "a5d56cb4516a2c11f5234eb83a738baa97a41c3d"}, "merge_base_commit": {"sha": "a5d56cb4516a2c11f5234eb83a738baa97a41c3d"}, "commits": [{"sha": BUILD_SHA}]}
    return deployments, version, app, image_commit, builds, build_ancestor, b122_ancestor, [
        {"id": 42, "environment": "production", "sha": SHA, "created_at": "2026-09-25T08:00:00Z"}
    ], [{"state": "success"}]


def test_provider_version_and_successful_github_deployment_bind_source_sha() -> None:
    deployments, version, app, image_commit, builds, build_ancestor, b122_ancestor, github_deployments, statuses = _evidence()
    receipt = verify_binding(
        deployments=deployments,
        version=version,
        github_deployments=github_deployments,
        github_statuses=statuses,
        container_application=app,
        container_image_commit=image_commit,
        container_build_runs=builds,
        build_is_ancestor=build_ancestor,
        b122_is_ancestor=b122_ancestor,
        worker_name="corelink-prod",
        worker_serving_sha=SHA,
    )
    assert receipt["worker_version_id"] == VERSION_ID
    assert receipt["container_application_id"] == APP_ID
    assert receipt["container_application_version"] == "101"
    assert receipt["container_image"].endswith(f"{BUILD_SHA[:9]}-r1")
    assert receipt["container_build_sha"] == BUILD_SHA
    assert receipt["worker_serving_sha"] == SHA
    assert receipt["container_build_run_id"] == 43
    assert receipt["provider_deployment_id"] == DEPLOYMENT_ID
    assert receipt["worker_serving_sha"] == SHA == receipt["github_deployment_sha"]
    assert receipt["github_deployment_status"] == "success"


@pytest.mark.parametrize("mutation", ["partial", "annotation", "metadata", "sha", "failure"])
def test_provider_or_github_mismatch_is_rejected(mutation: str) -> None:
    deployments, version, app, image_commit, builds, build_ancestor, b122_ancestor, github_deployments, statuses = deepcopy(_evidence())
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
            build_is_ancestor=build_ancestor,
            b122_is_ancestor=b122_ancestor,
            worker_name="corelink-prod",
            worker_serving_sha=SHA,
        )


def test_active_deployment_with_multiple_versions_is_rejected() -> None:
    deployments, version, app, image_commit, builds, build_ancestor, b122_ancestor, github_deployments, statuses = _evidence()
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
            build_is_ancestor=build_ancestor,
            b122_is_ancestor=b122_ancestor,
            worker_name="corelink-prod",
            worker_serving_sha=SHA,
        )


@pytest.mark.parametrize("mutation", ["image_sha", "application_version", "health", "missing_build", "failed_build", "wrong_build_sha", "not_ancestor", "wrong_serving_head", "missing_b122"])
def test_container_runtime_without_bound_build_is_rejected(mutation: str) -> None:
    deployments, version, app, image_commit, builds, build_ancestor, b122_ancestor, github_deployments, statuses = deepcopy(_evidence())
    if mutation == "image_sha":
        app["result"]["configuration"]["image"] = app["result"]["configuration"]["image"].replace(BUILD_SHA[:9], "0" * 9)
    elif mutation == "application_version":
        app["result"]["version"] = 0
    elif mutation == "health":
        app["result"]["health"]["instances"]["failed"] = 1
    elif mutation == "missing_build":
        builds["workflow_runs"] = []
    elif mutation == "failed_build":
        builds["workflow_runs"][0]["conclusion"] = "failure"
    elif mutation == "wrong_build_sha":
        image_commit["sha"] = "0" * 40
    elif mutation == "not_ancestor":
        build_ancestor["merge_base_commit"]["sha"] = "0" * 40
    elif mutation == "wrong_serving_head":
        build_ancestor["commits"][-1]["sha"] = "0" * 40
    else:
        b122_ancestor["status"] = "diverged"
    with pytest.raises(BindingError):
        verify_binding(
            deployments=deployments,
            version=version,
            github_deployments=github_deployments,
            github_statuses=statuses,
            container_application=app,
            container_image_commit=image_commit,
            container_build_runs=builds,
            build_is_ancestor=build_ancestor,
            b122_is_ancestor=b122_ancestor,
            worker_name="corelink-prod",
            worker_serving_sha=SHA,
        )
