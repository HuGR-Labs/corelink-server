#!/usr/bin/env python3
"""Join owner-lane outputs into the sole v2 packet consumed by the verifier.

The individual collectors are deliberately not publishable artifacts.  This
join is the boundary that attaches every observation to the same signed
deployment identity, adds the immutable B-108 source binding, and writes only
the canonical packet that is subsequently verified fail-closed.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import subprocess
from pathlib import Path
from typing import Any


SCHEMA = "corelink.performance-evidence.v2"
SOURCE = "worker/src/lib/quota.ts"


def mint_binding_sha256(attestation: dict[str, Any]) -> str:
    material = json.dumps(
        {
            "expires_ms": attestation.get("expires_ms"),
            "pat_id": attestation.get("pat_id"),
            "tenant_id": attestation.get("tenant_id"),
            "token_fingerprint": attestation.get("token_fingerprint"),
            "token_id": attestation.get("token_id"),
            "mint_operation_id": attestation.get("mint_operation_id"),
            "mint_request_id": attestation.get("mint_request_id"),
            "mint_response_sha256": attestation.get("mint_response_sha256"),
            "minted_at_epoch": attestation.get("minted_at_epoch"),
            "unused_since_epoch": attestation.get("unused_since_epoch"),
            "observed_at_epoch": attestation.get("observed_at_epoch"),
            "attestation_source": attestation.get("attestation_source"),
        },
        sort_keys=True,
        separators=(",", ":"),
    ).encode()
    return "sha256:" + hashlib.sha256(material).hexdigest()


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, indent=2) + "\n").encode()


def load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain an object")
    return value


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--context", type=Path, required=True)
    parser.add_argument("--measurements", type=Path, required=True)
    parser.add_argument("--b105", type=Path, required=True)
    parser.add_argument("--b106-attestation-subject", type=Path, required=True)
    parser.add_argument("--b106-attestation-bundle", type=Path, required=True)
    parser.add_argument("--b106-attestation-verification", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    context = load(args.context)
    measurements = load(args.measurements)
    lane = load(args.b105)
    subject_bytes = args.b106_attestation_subject.read_bytes()
    subject = load(args.b106_attestation_subject)
    bundle = load(args.b106_attestation_bundle)
    verification = json.loads(args.b106_attestation_verification.read_text(encoding="utf-8"))
    tenant = str(context.get("tenant_id", measurements.get("tenant_id", "")))
    if not tenant or measurements.get("tenant_id") != tenant:
        raise ValueError("context and measurements tenant_id do not match")
    source_head = str(context["source_head"])
    provider_id = str(context["deployment_id"])
    provider = context.get("provider_record")
    if not isinstance(provider, dict) or provider.get("provider") != str(context["provider"]) or provider.get("environment") != "production" or provider.get("commit") != str(context["provider_commit"]) or provider.get("deployment_id") != provider_id:
        raise ValueError("context provider_record is not a single bound production identity")

    def deployment(item: str) -> dict[str, Any]:
        return {
            "repository": context["repository"],
            "environment": context["environment"],
            "deployed_commit": str(context["provider_commit"]),
            "source_head": source_head,
            "github_sha": source_head,
            "github_repository": context["repository"],
            "github_event": context["github_event"],
            "github_run_id": context["github_run_id"],
            "github_run_attempt": context["github_run_attempt"],
            "github_run_started_at": context["github_run_started_at"],
            "github_deployment_id": context["github_deployment_id"],
            "provider_record": provider,
            "provider_blob_sha256": context["provider_blob_sha256"],
            "deployment_id": provider_id,
            "version": str(context["provider_commit"]),
            "tenant_id": tenant,
            "operation_id": f"deploy-{item.lower()}-{context['github_run_id']}",
        }

    items: dict[str, Any] = {}
    for item in ("B-102", "B-103", "B-104", "B-106", "B-107"):
        value = measurements.get("items", {}).get(item)
        if not isinstance(value, dict):
            raise ValueError(f"measurements missing {item}")
        value = dict(value)
        value["deployment"] = deployment(item)
        value["tenant_id"] = tenant
        if item == "B-106":
            attestation = subject.get("cold_attestation")
            if not isinstance(attestation, dict):
                raise ValueError("B-106 attestation subject has no cold attestation")
            value["cold_attestation"] = attestation
            value["github_attestation"] = {
                "subject_sha256": "sha256:" + hashlib.sha256(subject_bytes).hexdigest(),
                "subject_name": args.b106_attestation_subject.name,
                "bundle_sha256": "sha256:" + hashlib.sha256(canonical_json(bundle)).hexdigest(),
                "bundle": bundle,
                "verification": verification,
                "verification_policy": {
                    "repository": context["repository"],
                    "signer_workflow": f"{context['repository']}/.github/workflows/perf-production-evidence.yml",
                    "signer_digest": source_head,
                    "predicate_type": "https://slsa.dev/provenance/v1",
                },
            }
        items[item] = value

    b105 = {
        "tenant_id": tenant,
        "deployment": deployment("B-105"),
        "pairs": lane.get("pairs"),
    }
    items["B-105"] = b105

    source = subprocess.run(
        ("git", "show", f"{source_head}:{SOURCE}"),
        cwd=args.root,
        capture_output=True,
        check=True,
        timeout=10,
    ).stdout
    items["B-108"] = {
        "tenant_id": tenant,
        "deployment": deployment("B-108"),
        "source_binding": {
            "path": SOURCE,
            "commit": source_head,
            "blob_sha256": "sha256:" + hashlib.sha256(source).hexdigest(),
        },
        "counter_statement": (
            "INSERT INTO monthly_request_counts (tenant_id, year_month, request_count, updated_at_ms) "
            "VALUES (?1, ?2, 1, ?3) "
            "ON CONFLICT(tenant_id, year_month) "
            "DO UPDATE SET request_count = request_count + 1, updated_at_ms = ?3 "
            "RETURNING request_count"
        ),
    }
    items["B-108"]["counter_statement_sha256"] = "sha256:" + hashlib.sha256(items["B-108"]["counter_statement"].encode()).hexdigest()
    now = dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")
    packet = {"schema": SCHEMA, "environment": "production", "captured_at": now, "tenant_id": tenant, "items": items}
    args.output.write_text(json.dumps(packet, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
