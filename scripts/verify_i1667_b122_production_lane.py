#!/usr/bin/env python3
"""Credentialless static contract for the B-122 production evidence lane."""

from pathlib import Path


WORKFLOW = Path(".github/workflows/issue-1667-b122-production-evidence.yml")
BINDING_VERIFIER = Path("scripts/verify_i1667_b122_provider_binding.py")
DEPLOY_WORKFLOW = Path(".github/workflows/cf-deploy-prod.yml")
DEPLOY_SCRIPT = Path("scripts/deploy-container-prod.sh")


def main() -> None:
    text = WORKFLOW.read_text(encoding="utf-8")
    binding_text = BINDING_VERIFIER.read_text(encoding="utf-8")
    deploy_workflow_text = DEPLOY_WORKFLOW.read_text(encoding="utf-8")
    deploy_script_text = DEPLOY_SCRIPT.read_text(encoding="utf-8")
    required = (
        "workflow_dispatch:",
        "approval:",
        "production_base:",
        "dogfood_tenant:",
        "serving_sha:",
        "region:",
        "worker_name:",
        "environment: production",
        "deployments: read",
        "persist-credentials: false",
        "secrets.CORELINK_DOGFOOD_PAT",
        "secrets.CF_API_TOKEN",
        "secrets.CF_ACCOUNT_ID",
        "Bind the active Worker version to the GitHub source SHA",
        "Confirm the same Worker version remained active during sampling",
        "workers/scripts/${WORKER_NAME}/deployments?per_page=100",
        "worker_version_id",
        "provider_deployment_id",
        "container_build_sha",
        "worker_serving_sha",
        "container_application_version",
        "container_image",
        "container-build-push-prod.yml/runs?head_sha=${container_build_sha}",
        "compare/${container_build_sha}...${source_sha}",
        "a5d56cb4516a2c11f5234eb83a738baa97a41c3d...${container_build_sha}",
        '--worker-serving-sha "${source_sha}"',
        "provider_binding",
        "timeout 25s curl",
        "request_id=%header{x-request-id}",
        "cf_ray=%header{cf-ray}",
        "--connect-timeout 10",
        "--max-time 20",
        "--data-binary @<(head -c 1024 /dev/zero)",
        "-X PUT",
        "b102-warm",
        "b107-separated",
        "X-Server-Timing-Wdb-Detail: on",
        "verify_d03_timing_artifact.py --writes --require-identities artifacts/b122/b102-warm.txt",
        "verify_d03_timing_artifact.py --writes --require-identities artifacts/b122/b107-separated.txt",
        "sample-started-utc.txt",
        "sample_started_utc",
        "shape-check-b102.txt",
        "shape-check-b107.txt",
        "shape_checks",
        "upload-artifact",
        "retention-days: 30",
    )
    missing = [needle for needle in required if needle not in text]
    missing.extend(
        f"provider verifier: {needle}"
        for needle in (
            "workers/message",
            "percentage != 100",
            "worker_version_id",
            "provider_deployment_id",
            "github_deployment_sha",
            "container_build_sha",
            "worker_serving_sha",
            "B122_CONTAINER_SOURCE_SHA",
            "build_is_ancestor",
            "b122_is_ancestor",
        )
        if needle not in binding_text
    )
    if 'parser.add_argument("--source-sha"' in binding_text:
        missing.append("provider verifier: obsolete required --source-sha argument")
    missing.extend(
        f"deploy provenance: {needle}"
        for needle in (
            "DEPLOY_SOURCE_SHA: ${{ github.sha }}",
            "corelink-source-sha=",
            '--message "$DEPLOY_MESSAGE"',
        )
        if needle not in deploy_workflow_text + deploy_script_text
    )
    forbidden = [needle for needle in ("wrangler deploy", "kubectl apply", "terraform apply", "gh issue close") if needle in text]
    if missing:
        raise SystemExit("missing lane contract: " + ", ".join(missing))
    if forbidden:
        raise SystemExit("forbidden mutation command: " + ", ".join(forbidden))
    preflight = text.find("Bind the active Worker version to the GitHub source SHA")
    sampling = text.find("Capture B-102 warm and B-107 separated 1 KiB samples")
    postflight = text.find("Confirm the same Worker version remained active during sampling")
    if min(preflight, sampling, postflight) < 0 or not preflight < sampling < postflight:
        raise SystemExit("provider readback must bracket all B-122 sampling")
    print("B-122 production evidence lane contract: PASS")


if __name__ == "__main__":
    main()
