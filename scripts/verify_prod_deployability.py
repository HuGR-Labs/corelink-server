#!/usr/bin/env python3
"""Fail closed when a production deploy surface cannot be reproduced from a PR.

The manifest is a deliberately small closed-world inventory. It names every
committed Wrangler production configuration, its source entrypoint/artifacts,
and the deploy workflow or package script that owns it. The verifier checks
the inventory in the checked-out tree and then checks the same source files at
both PR base and head commits. It performs no Cloudflare calls and consumes no
secrets, which makes it safe for fork pull requests.
"""

from __future__ import annotations

import argparse
import json
import re
import stat
import subprocess
import sys
import tomllib
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
MANIFEST = Path("reports/production-deploy-surfaces.v1.json")
WRANGLER_NAME = "wrangler.toml"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
EXCLUDED_CONFIG_ALLOWLIST = [
    {
        "path": "crates/corelink-clerk-cf/wrangler.toml",
        "reason": "proof-of-concept worker explicitly documents that it is not a production deployment",
        "identity": {
            "name": "corelink-clerk-cf-poc",
            "main": "build/worker/shim.mjs",
            "production_name": "corelink-clerk-cf-poc-prod",
            "environment": "dev",
        },
    }
]


class DeployabilityError(ValueError):
    """An inventory or production deploy input is absent, stale, or invalid."""


def _safe_relative(value: object, label: str) -> Path:
    if not isinstance(value, str) or not value or "\0" in value or "\\" in value:
        raise DeployabilityError(f"{label} must be a non-empty POSIX-relative path")
    path = Path(value)
    if path.is_absolute() or ".." in path.parts or "." in path.parts:
        raise DeployabilityError(f"{label} must stay beneath the repository root")
    if value != path.as_posix():
        raise DeployabilityError(f"{label} must use canonical POSIX spelling")
    return path


def _regular_current(repo_root: Path, relative: str, label: str) -> Path:
    path = repo_root / _safe_relative(relative, label)
    try:
        mode = path.stat(follow_symlinks=False).st_mode
    except OSError as exc:
        raise DeployabilityError(f"{label} is missing: {relative}") from exc
    if path.is_symlink() or not (stat.S_ISREG(mode) or stat.S_ISDIR(mode)):
        raise DeployabilityError(f"{label} must be a regular file or directory: {relative}")
    return path


def _git(repo_root: Path, *args: str) -> str:
    try:
        return subprocess.check_output(
            ("git", *args), cwd=repo_root, text=True, stderr=subprocess.PIPE
        ).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        raise DeployabilityError(
            f"git cannot validate base/head commit ({' '.join(args)})"
        ) from exc


def _commit(repo_root: Path, value: str, label: str) -> str:
    if not isinstance(value, str) or not value or value.startswith("-"):
        raise DeployabilityError(f"{label} must be a commit SHA")
    try:
        resolved = _git(repo_root, "rev-parse", "--verify", f"{value}^{{commit}}")
    except DeployabilityError as exc:
        raise DeployabilityError(f"{label} is not a resolvable commit SHA") from exc
    if not SHA_RE.fullmatch(resolved):
        raise DeployabilityError(f"{label} did not resolve to a full commit SHA")
    return resolved


def _git_file(repo_root: Path, commit: str, relative: str, label: str) -> bytes:
    path = _safe_relative(relative, label)
    try:
        mode = _git(repo_root, "cat-file", "-t", f"{commit}:{path.as_posix()}")
    except DeployabilityError as exc:
        raise DeployabilityError(f"{label} is absent at {commit[:12]}: {relative}") from exc
    if mode != "blob":
        raise DeployabilityError(
            f"{label} must be a committed regular file at {commit[:12]}: {relative}"
        )
    try:
        return subprocess.check_output(
            ("git", "show", f"{commit}:{path.as_posix()}"),
            cwd=repo_root,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        raise DeployabilityError(f"{label} cannot be read at {commit[:12]}: {relative}") from exc


def _discover_wrangler_configs(repo_root: Path) -> set[str]:
    found: set[str] = set()
    for path in repo_root.rglob(WRANGLER_NAME):
        if any(part in {".git", "node_modules", "target"} for part in path.parts):
            continue
        if path.is_symlink() or not path.is_file():
            raise DeployabilityError(
                f"Wrangler config is not a regular file: {path.relative_to(repo_root)}"
            )
        found.add(path.relative_to(repo_root).as_posix())
    return found


def _load_manifest(repo_root: Path, manifest_path: Path = MANIFEST) -> dict:
    path = (
        manifest_path
        if manifest_path.is_absolute()
        else _regular_current(repo_root, manifest_path.as_posix(), "deploy-surface manifest")
    )
    if not path.is_file() or path.is_symlink():
        raise DeployabilityError(f"deploy-surface manifest is missing: {path}")
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise DeployabilityError("deploy-surface manifest is not valid UTF-8 JSON") from exc
    if not isinstance(manifest, dict) or set(manifest) != {
        "version", "surfaces", "excluded_configs"
    }:
        raise DeployabilityError("deploy-surface manifest has ambiguous top-level fields")
    if manifest.get("version") != 1 or not isinstance(manifest["surfaces"], list) or not manifest["surfaces"]:
        raise DeployabilityError("deploy-surface manifest must declare version 1 and non-empty surfaces")
    if manifest["excluded_configs"] != EXCLUDED_CONFIG_ALLOWLIST:
        raise DeployabilityError(
            "deploy-surface excluded_configs must equal the immutable POC allowlist"
        )
    return manifest


def _parse_toml(raw: bytes, relative: str, commit: str) -> dict:
    try:
        value = tomllib.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as exc:
        raise DeployabilityError(f"invalid Wrangler TOML at {commit[:12]}: {relative}") from exc
    if not isinstance(value, dict):
        raise DeployabilityError(f"Wrangler config is not an object at {commit[:12]}: {relative}")
    return value


def _validate_surface(repo_root: Path, surface: dict, base: str, head: str) -> dict:
    required = {
        "id", "config", "production_environment", "entrypoint", "artifacts",
        "workflow", "workflow_markers"
    }
    optional = {"deploy_manifest", "deploy_marker"}
    if (
        not isinstance(surface, dict)
        or set(surface) - required - optional
        or not required <= set(surface)
    ):
        raise DeployabilityError("each deploy surface must have exactly the documented inventory fields")
    sid = surface.get("id")
    if not isinstance(sid, str) or not sid or not re.fullmatch(r"[a-z0-9-]+", sid):
        raise DeployabilityError("deploy surface id is invalid")
    config = _safe_relative(surface["config"], f"{sid}.config").as_posix()
    if not config.endswith("/wrangler.toml") and config != WRANGLER_NAME:
        raise DeployabilityError(f"{sid}: config must be a wrangler.toml")
    environment = surface["production_environment"]
    if environment not in {"prod", "default"}:
        raise DeployabilityError(f"{sid}: production_environment must be prod or default")
    entrypoint = _safe_relative(surface["entrypoint"], f"{sid}.entrypoint").as_posix()
    artifacts = surface["artifacts"]
    if not isinstance(artifacts, list) or not artifacts:
        raise DeployabilityError(f"{sid}: artifacts must be a non-empty list")
    config_parent = Path(config).parent
    current_toml = _parse_toml(_git_file(repo_root, head, config, f"{sid} config"), config, head)
    base_toml = _parse_toml(_git_file(repo_root, base, config, f"{sid} config"), config, base)
    for commit, toml in ((base, base_toml), (head, current_toml)):
        if not isinstance(toml.get("name"), str) or not toml["name"].strip():
            raise DeployabilityError(f"{sid}: Wrangler name missing at {commit[:12]}")
        envs = toml.get("env")
        prod = envs.get("prod") if isinstance(envs, dict) else None
        if environment == "prod" and not isinstance(prod, dict):
            raise DeployabilityError(f"{sid}: production env.prod block missing at {commit[:12]}")
        if environment == "default" and isinstance(prod, dict):
            raise DeployabilityError(
                f"{sid}: default production surface unexpectedly requires env.prod at {commit[:12]}"
            )
        effective = prod if isinstance(prod, dict) else toml
        configured_main = effective.get("main", toml.get("main"))
        if not isinstance(configured_main, str) or not configured_main:
            raise DeployabilityError(f"{sid}: Wrangler main missing at {commit[:12]}")
        configured_entrypoint = (config_parent / configured_main).as_posix()
        if configured_entrypoint != entrypoint:
            raise DeployabilityError(
                f"{sid}: manifest entrypoint {entrypoint} disagrees with Wrangler main "
                f"{configured_entrypoint} at {commit[:12]}"
            )
        if environment == "prod" and not isinstance(effective.get("name"), str):
            raise DeployabilityError(f"{sid}: env.prod name missing at {commit[:12]}")
    generated_paths: set[str] = set()
    for artifact in artifacts:
        if (
            not isinstance(artifact, dict)
            or set(artifact) - {"path", "generated", "build_marker"}
            or not {"path", "generated"} <= set(artifact)
        ):
            raise DeployabilityError(f"{sid}: artifact record is malformed")
        path = _safe_relative(artifact["path"], f"{sid}.artifact").as_posix()
        if not isinstance(artifact["generated"], bool):
            raise DeployabilityError(f"{sid}: artifact generated flag must be boolean")
        if artifact["generated"]:
            generated_paths.add(path)
            if not isinstance(artifact.get("build_marker"), str) or not artifact["build_marker"].strip():
                raise DeployabilityError(f"{sid}: generated artifact needs a build marker")
        else:
            for commit in (base, head):
                _git_file(repo_root, commit, path, f"{sid} source artifact")
    if entrypoint not in generated_paths:
        for commit in (base, head):
            _git_file(repo_root, commit, entrypoint, f"{sid} entrypoint")
    workflow = surface["workflow"]
    markers = surface["workflow_markers"]
    if not isinstance(markers, list) or any(not isinstance(marker, str) or not marker for marker in markers):
        raise DeployabilityError(f"{sid}: workflow_markers must be non-empty strings")
    if workflow is not None:
        workflow = _safe_relative(workflow, f"{sid}.workflow").as_posix()
        if not markers:
            raise DeployabilityError(f"{sid}: deploy workflow needs at least one marker")
        for commit in (base, head):
            try:
                text = _git_file(
                    repo_root, commit, workflow, f"{sid} deploy workflow"
                ).decode("utf-8")
            except UnicodeDecodeError as exc:
                raise DeployabilityError(
                    f"{sid}: deploy workflow is not UTF-8 at {commit[:12]}"
                ) from exc
            for marker in markers:
                if marker not in text:
                    raise DeployabilityError(
                        f"{sid}: deploy workflow is stale or missing marker {marker!r}"
                        f" at {commit[:12]}"
                    )
            if "pull_request:" in text:
                raise DeployabilityError(
                    f"{sid}: production deploy workflow must not run with credentials on pull_request"
                )
    else:
        if markers:
            raise DeployabilityError(f"{sid}: manual surface cannot declare workflow markers")
        if "deploy_manifest" not in surface or "deploy_marker" not in surface:
            raise DeployabilityError(f"{sid}: manually deployed surface needs its package deploy manifest")
        package = _safe_relative(surface["deploy_manifest"], f"{sid}.deploy_manifest").as_posix()
        marker = surface["deploy_marker"]
        if not isinstance(marker, str) or not marker:
            raise DeployabilityError(f"{sid}: production deploy script marker is invalid")
        for commit in (base, head):
            raw = _git_file(repo_root, commit, package, f"{sid} deploy manifest")
            try:
                package_json = json.loads(raw.decode("utf-8"))
            except (UnicodeDecodeError, json.JSONDecodeError) as exc:
                raise DeployabilityError(
                    f"{sid}: deploy manifest is invalid JSON at {commit[:12]}"
                ) from exc
            scripts = package_json.get("scripts") if isinstance(package_json, dict) else None
            if not isinstance(scripts, dict) or not isinstance(scripts.get(marker), str) or not scripts[marker].strip():
                raise DeployabilityError(
                    f"{sid}: production deploy script {marker!r} is absent"
                    f" at {commit[:12]}"
                )
    changed = _git(repo_root, "diff", "--name-only", base, head, "--", config, entrypoint) != ""
    return {"id": sid, "config": config, "changed": changed}


def _validate_excluded_config(repo_root: Path, record: dict, base: str, head: str) -> None:
    if record != EXCLUDED_CONFIG_ALLOWLIST[0]:
        raise DeployabilityError(
            "excluded Wrangler config does not match the immutable POC identity allowlist"
        )
    identity = record["identity"]
    path = record["path"]
    for commit in (base, head):
        toml = _parse_toml(_git_file(repo_root, commit, path, "excluded Wrangler config"), path, commit)
        if toml.get("name") != identity["name"] or toml.get("main") != identity["main"]:
            raise DeployabilityError(
                f"excluded Wrangler config identity changed at {commit[:12]}: {path}"
            )
        variables = toml.get("vars")
        if not isinstance(variables, dict) or variables.get("ENVIRONMENT") != identity["environment"]:
            raise DeployabilityError(
                f"excluded Wrangler config environment is no longer the POC at {commit[:12]}"
            )
        environments = toml.get("env")
        production = environments.get("prod") if isinstance(environments, dict) else None
        if not isinstance(production, dict) or production.get("name") != identity["production_name"]:
            raise DeployabilityError(
                f"excluded Wrangler config production identity changed at {commit[:12]}"
            )


def verify(
    repo_root: Path = REPO_ROOT,
    *,
    base_sha: str | None = None,
    head_sha: str | None = None,
    manifest_path: Path = MANIFEST,
) -> dict:
    manifest = _load_manifest(repo_root, manifest_path)
    base = _commit(repo_root, base_sha or "HEAD^", "base_sha")
    head = _commit(repo_root, head_sha or "HEAD", "head_sha")
    if base == head:
        raise DeployabilityError("base_sha and head_sha must identify different commits")
    surfaces = manifest["surfaces"]
    ids: set[str] = set()
    configs: set[str] = set()
    details = []
    for surface in surfaces:
        sid = surface.get("id") if isinstance(surface, dict) else None
        if sid in ids:
            raise DeployabilityError(f"duplicate deploy surface id: {sid}")
        ids.add(sid)
        config = surface.get("config") if isinstance(surface, dict) else None
        if config in configs:
            raise DeployabilityError(f"duplicate deploy surface config: {config}")
        configs.add(config)
        details.append(_validate_surface(repo_root, surface, base, head))
    excluded: set[str] = set()
    for record in manifest["excluded_configs"]:
        if not isinstance(record, dict) or set(record) != {"path", "reason", "identity"}:
            raise DeployabilityError("excluded config records must contain the canonical identity")
        path = _safe_relative(record["path"], "excluded config").as_posix()
        if path in excluded or path in configs:
            raise DeployabilityError(f"invalid or duplicate excluded config: {path}")
        excluded.add(path)
        _validate_excluded_config(repo_root, record, base, head)
    discovered = _discover_wrangler_configs(repo_root)
    if discovered != configs | excluded:
        missing = sorted((configs | excluded) - discovered)
        extra = sorted(discovered - configs - excluded)
        raise DeployabilityError(f"Wrangler production config census drifted (missing={missing}, extra={extra})")
    return {
        "base_sha": base,
        "head_sha": head,
        "surfaces": len(details),
        "changed_surfaces": sum(item["changed"] for item in details),
        "configs": sorted(configs),
        "excluded_configs": sorted(excluded),
        "status": "production_deployability_verified",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    parser.add_argument("--base-sha")
    parser.add_argument("--head-sha")
    args = parser.parse_args()
    try:
        report = verify(args.repo_root.resolve(), base_sha=args.base_sha, head_sha=args.head_sha)
    except DeployabilityError as exc:
        print(f"PRODUCTION-DEPLOYABILITY: RED: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(report, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
