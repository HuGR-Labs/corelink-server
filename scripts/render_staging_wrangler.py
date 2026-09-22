#!/usr/bin/env python3
"""Render isolated Wrangler configs from the B-029 staging topology.

The generated configs are direct per-Worker configs. They do not use Wrangler
environments, so no development or production bindings can be inherited.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from typing import Any
from urllib.parse import urlparse


ROOT = Path(__file__).resolve().parents[1]
TOPOLOGY = ROOT / "infra/staging/topology.json"
PRODUCTION_NAME = re.compile(r"(^|[-_.])(prod|production)([-_.]|$)", re.IGNORECASE)
STAGING_SUFFIX = re.compile(r"-staging$", re.IGNORECASE)
UUID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$", re.I)
HEX_ID = re.compile(r"^[0-9a-f]{32}$", re.I)


class ContractError(ValueError):
    pass


def toml_string(value: str) -> str:
    return json.dumps(value, ensure_ascii=True)


def toml_value(value: Any) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int) or isinstance(value, float):
        return str(value)
    if isinstance(value, str):
        return toml_string(value)
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    raise ContractError(f"unsupported TOML value type: {type(value).__name__}")


def key_value(key: str, value: Any) -> str:
    return f"{key} = {toml_value(value)}"


def table(name: str, values: dict[str, Any]) -> list[str]:
    lines = [f"[{name}]"]
    lines.extend(key_value(key, value) for key, value in values.items())
    return lines


def array_table(name: str, values: dict[str, Any]) -> list[str]:
    lines = [f"[[{name}]]"]
    lines.extend(key_value(key, value) for key, value in values.items())
    return lines


def require_staging_name(value: str, label: str) -> None:
    if not isinstance(value, str) or not value or PRODUCTION_NAME.search(value):
        raise ContractError(f"{label} is empty or production-scoped")
    if not STAGING_SUFFIX.search(value):
        raise ContractError(f"{label} must end in -staging")


def validate_topology(data: dict[str, Any], r2_endpoint: str) -> dict[str, Any]:
    cloudflare = data["cloudflare"]
    outputs = data["outputs"]
    target = outputs["target_host"]
    origin = cloudflare["canonical_origin"]
    parsed_target = urlparse(target)
    if (
        target != origin
        or parsed_target.scheme != "https"
        or parsed_target.hostname != "staging.corelink.humangr.com"
        or parsed_target.path not in ("", "/")
        or parsed_target.query
        or parsed_target.fragment
        or parsed_target.username
        or parsed_target.password
        or PRODUCTION_NAME.search(target)
    ):
        raise ContractError("canonical staging target is invalid")

    endpoint = urlparse(r2_endpoint)
    if (
        endpoint.scheme != "https"
        or not endpoint.hostname
        or not endpoint.hostname.endswith(".r2.cloudflarestorage.com")
        or endpoint.path not in ("", "/")
        or endpoint.query
        or endpoint.fragment
        or endpoint.username
        or endpoint.password
        or PRODUCTION_NAME.search(r2_endpoint)
    ):
        raise ContractError("R2 provider readback endpoint is invalid")

    workers = outputs["worker_names"]
    expected_workers = [
        cloudflare["root_worker"],
        cloudflare["signup_worker"],
        cloudflare["synthetic_receiver_worker"],
    ]
    if len(workers) != 3 or len(set(workers)) != 3 or set(workers) != set(expected_workers):
        raise ContractError("topology must identify exactly the three staging Workers")
    for name in workers:
        require_staging_name(name, "Worker name")

    for name in outputs["resource_names"]:
        require_staging_name(name, "resource name")

    for route in cloudflare["routes"]:
        if route["worker"] not in workers or route["zone_name"] != cloudflare["zone_name"]:
            raise ContractError("route references an unexpected Worker or zone")
        pattern_host = route["pattern"].split("/", 1)[0]
        if pattern_host != parsed_target.hostname or PRODUCTION_NAME.search(route["pattern"]):
            raise ContractError("route escapes the canonical staging host")

    secret_names = {
        name
        for worker, names in data["required_secret_names"].items()
        if worker != "github_environment_staging"
        for name in names
    }
    for settings_key in (
        "root_worker_settings",
        "signup_worker_settings",
        "synthetic_receiver_worker_settings",
    ):
        worker_vars = cloudflare[settings_key]["vars"]
        if worker_vars.get("ENVIRONMENT") != "staging":
            raise ContractError("Worker environment var must remain staging")
        for name, value in worker_vars.items():
            if name in secret_names:
                raise ContractError("a secret name was placed in non-secret vars")
            if isinstance(value, str) and PRODUCTION_NAME.search(value):
                raise ContractError("non-secret Worker vars contain a production target")
    if cloudflare["signup_worker_settings"]["vars"].get("CORELINK_API_BASE") != target:
        raise ContractError("signup API base must match the canonical staging target")

    for database in cloudflare["d1"]:
        require_staging_name(database["database_name"], "D1 database name")
        if not UUID.fullmatch(database["database_id"]):
            raise ContractError("D1 database ID is malformed")
        for binding in database["bindings"]:
            if binding["worker"] not in workers:
                raise ContractError("D1 binding references an unexpected Worker")
    for bucket in cloudflare["r2"]:
        require_staging_name(bucket["bucket_name"], "R2 bucket name")
        if bucket["worker"] not in workers:
            raise ContractError("R2 binding references an unexpected Worker")
    for namespace in cloudflare["kv"]:
        require_staging_name(namespace["namespace_title"], "KV namespace name")
        if not HEX_ID.fullmatch(namespace["namespace_id"]):
            raise ContractError("KV namespace ID is malformed")
        if namespace["worker"] not in workers:
            raise ContractError("KV binding references an unexpected Worker")
    for binding in cloudflare["durable_objects"]:
        if binding["worker"] not in workers:
            raise ContractError("Durable Object binding references an unexpected Worker")
    for queue in cloudflare["queues"]:
        require_staging_name(queue["queue_name"], "queue name")
        require_staging_name(queue["dead_letter_queue"], "dead-letter queue name")
        if not HEX_ID.fullmatch(queue["queue_id"]) or not HEX_ID.fullmatch(queue["dead_letter_queue_id"]):
            raise ContractError("queue ID is malformed")
        if queue["consumer_worker"] not in workers:
            raise ContractError("queue references an unexpected Worker")
    for binding in cloudflare["service_bindings"]:
        if binding["worker"] not in workers or binding["service"] not in workers:
            raise ContractError("service binding references an unexpected Worker")
    return data


def settings_for(data: dict[str, Any]) -> dict[str, dict[str, Any]]:
    cf = data["cloudflare"]
    return {
        cf["root_worker"]: cf["root_worker_settings"],
        cf["signup_worker"]: cf["signup_worker_settings"],
        cf["synthetic_receiver_worker"]: cf["synthetic_receiver_worker_settings"],
    }


def render_worker(
    data: dict[str, Any],
    worker: str,
    r2_endpoint: str,
    phase: str,
    output_path: Path,
) -> str:
    cf = data["cloudflare"]
    settings = settings_for(data)[worker]
    def repository_path(path: str) -> str:
        return Path(os.path.relpath(ROOT / path, output_path.parent)).as_posix()

    lines: list[str] = [
        "# Generated by scripts/render_staging_wrangler.py from topology.json.",
        "# Do not hand edit. This isolated config contains no inherited env tables.",
        key_value("name", worker),
        key_value("main", repository_path(settings["main"])),
        key_value("workers_dev", False),
        key_value("compatibility_date", settings["compatibility_date"]),
    ]
    if settings.get("compatibility_flags"):
        lines.append(key_value("compatibility_flags", settings["compatibility_flags"]))

    vars_map = dict(settings["vars"])
    if worker == cf["root_worker"]:
        for key, value in vars_map.items():
            if isinstance(value, dict) and value.get("source") == "provider_output":
                if value.get("name") != "r2_s3_endpoint":
                    raise ContractError("topology contains an unsupported provider output")
                vars_map[key] = r2_endpoint
    lines.extend(["", *table("vars", vars_map)])

    if phase == "final":
        routes = [route for route in cf["routes"] if route["worker"] == worker]
    elif phase == "bootstrap":
        routes = []
    else:
        raise ContractError("phase must be bootstrap or final")
    for route in routes:
        if route["worker"] == worker:
            lines.extend(["", *array_table("routes", {
                "pattern": route["pattern"],
                "zone_name": route["zone_name"],
            })])

    for database in cf["d1"]:
        for binding in database["bindings"]:
            if binding["worker"] == worker:
                lines.extend(["", *array_table("d1_databases", {
                    "binding": binding["binding"],
                    "database_name": database["database_name"],
                    "database_id": database["database_id"],
                    "migrations_dir": repository_path(database["migrations_dir"]),
                })])

    for bucket in cf["r2"]:
        if bucket["worker"] == worker:
            lines.extend(["", *array_table("r2_buckets", {
                "binding": bucket["binding"],
                "bucket_name": bucket["bucket_name"],
            })])
    for namespace in cf["kv"]:
        if namespace["worker"] == worker:
            lines.extend(["", *array_table("kv_namespaces", {
                "binding": namespace["binding"],
                "id": namespace["namespace_id"],
            })])
    for binding in cf["durable_objects"]:
        if binding["worker"] == worker:
            lines.extend(["", *array_table("durable_objects.bindings", {
                "name": binding["binding"],
                "class_name": binding["class_name"],
            })])
    do_classes = list(dict.fromkeys(
        binding["class_name"]
        for binding in cf["durable_objects"]
        if binding["worker"] == worker
    ))
    if do_classes:
        lines.extend(["", *array_table("migrations", {
            "tag": "staging-v1",
            "new_sqlite_classes": do_classes,
        })])

    for binding in cf["service_bindings"]:
        if binding["worker"] == worker:
            lines.extend(["", *array_table("services", {
                "binding": binding["binding"],
                "service": binding["service"],
            })])

    for queue in cf["queues"]:
        if queue["consumer_worker"] != worker:
            continue
        lines.extend(["", *array_table("queues.producers", {
            "binding": queue["producer_binding"],
            "queue": queue["queue_name"],
        })])
        lines.extend(["", *array_table("queues.consumers", {
            "queue": queue["queue_name"],
            "max_batch_size": 10,
            "max_batch_timeout": 30,
            "max_retries": 10,
            "dead_letter_queue": queue["dead_letter_queue"],
        })])
        lines.extend(["", *array_table("queues.consumers", {
            "queue": queue["dead_letter_queue"],
            "max_batch_size": 10,
            "max_batch_timeout": 30,
            "max_retries": 3,
        })])

    if worker == cf["root_worker"]:
        container = cf["container"]
        if container["worker"] != worker:
            raise ContractError("container is assigned to an unexpected Worker")
        lines.extend(["", *array_table("containers", {
            "class_name": container["class_name"],
            "image": repository_path(container["source"].removeprefix("./")),
            "instance_type": container["instance_type"],
            "max_instances": container["max_instances"],
        })])
        lines.extend(["", *table("observability", settings["observability"])])
    elif "observability" in settings:
        lines.extend(["", *table("observability", settings["observability"])])

    if settings.get("crons") is not None:
        lines.extend(["", *table("triggers", {"crons": settings["crons"]})])
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase", choices=("bootstrap", "final"), required=True)
    parser.add_argument("--worker", choices=("root", "signup", "synthetic"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)

    try:
        data = json.loads(TOPOLOGY.read_text(encoding="utf-8"))
        r2_endpoint = os.environ.get("STAGING_R2_S3_ENDPOINT", "")
        if not r2_endpoint:
            raise ContractError("STAGING_R2_S3_ENDPOINT provider readback is required")
        validate_topology(data, r2_endpoint)
        cf = data["cloudflare"]
        worker_names = {
            "root": cf["root_worker"],
            "signup": cf["signup_worker"],
            "synthetic": cf["synthetic_receiver_worker"],
        }
        worker = worker_names[args.worker]
        output_path = args.output if args.output.is_absolute() else ROOT / args.output
        output_path = output_path.resolve()
        try:
            output_path.relative_to(ROOT)
        except ValueError:
            pass
        else:
            raise ContractError("rendered configs must be written outside the repository")
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(
            render_worker(data, worker, r2_endpoint, args.phase, output_path),
            encoding="utf-8",
        )
        print(f"rendered {args.phase} config for {worker} to {output_path}")
    except (OSError, KeyError, TypeError, json.JSONDecodeError, ContractError, ValueError) as error:
        print(f"staging config render rejected: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
