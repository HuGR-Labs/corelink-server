#!/usr/bin/env python3
"""Collect a redacted, read-only Wrangler deployment/version binding receipt."""

from __future__ import annotations

import argparse
import base64
import contextlib
import hashlib
import io
import json
import sys
import subprocess
import tarfile
import tempfile
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.verify_b006_provider_binding import (
    EXPECTED_ACCOUNT_ID,
    EXPECTED_AUTH_EMAIL,
    EXPECTED_AUTH_TYPE,
    EXPECTED_DEPLOYMENT_ID,
    EXPECTED_SCRIPT_ETAG,
    EXPECTED_VERSION_ID,
    EXPECTED_VERSION_NUMBER,
    EXPECTED_ROLLOUT_PERCENTAGE,
    EXPECTED_WRANGLER_VERSION,
    LOCK_SOURCE,
    PROVIDER,
    SOURCE,
    WORKER,
    read_lock_integrity,
    validate_provider_binding,
)

WRANGLER_TARBALL_URL = f"https://registry.npmjs.org/wrangler/-/wrangler-{EXPECTED_WRANGLER_VERSION}.tgz"
MAX_TARBALL_BYTES = 100 * 1024 * 1024


class VerifiedWrangler:
    def __init__(self, entrypoint: Path, integrity: str) -> None:
        self.entrypoint = entrypoint
        self.integrity = integrity

    def json(self, *args: str) -> tuple[Any, bool]:
        result = subprocess.run(
            ["node", str(self.entrypoint), *args, "--json"],
            check=False,
            capture_output=True,
            text=True,
            timeout=60,
        )
        payload = json.loads(result.stdout)
        provider_errors = isinstance(payload, dict) and (payload.get("success") is False or bool(payload.get("errors")))
        return payload, result.returncode == 0 and payload is not None and not provider_errors


def _safe_extract(blob: bytes, destination: Path) -> Path:
    with tarfile.open(fileobj=io.BytesIO(blob), mode="r:gz") as archive:
        members = archive.getmembers()
        for member in members:
            path = Path(member.name)
            if path.is_absolute() or ".." in path.parts or member.issym() or member.islnk():
                raise ValueError("Wrangler package contains an unsafe archive member")
        archive.extractall(destination)
    package = destination / "package"
    if not package.is_dir():
        raise ValueError("Wrangler package archive has no package root")
    manifest = json.loads((package / "package.json").read_text(encoding="utf-8"))
    if manifest.get("version") != EXPECTED_WRANGLER_VERSION:
        raise ValueError("Wrangler package version drifted")
    entrypoint = package / "bin" / "wrangler.js"
    if not entrypoint.is_file():
        raise ValueError("verified Wrangler package entrypoint is missing")
    return entrypoint


@contextlib.contextmanager
def _verified_wrangler() -> Any:
    lock_integrity = read_lock_integrity()
    request = urllib.request.Request(WRANGLER_TARBALL_URL, headers={"Accept": "application/octet-stream"}, method="GET")
    with urllib.request.urlopen(request, timeout=60) as response:
        blob = response.read(MAX_TARBALL_BYTES + 1)
    if len(blob) > MAX_TARBALL_BYTES:
        raise ValueError("Wrangler package exceeded the bounded archive limit")
    computed_integrity = "sha512-" + base64.b64encode(hashlib.sha512(blob).digest()).decode("ascii")
    if computed_integrity != lock_integrity:
        raise ValueError("downloaded Wrangler package integrity does not match pnpm-lock.yaml")
    with tempfile.TemporaryDirectory(prefix="corelink-b006-wrangler-") as private_dir:
        private_root = Path(private_dir)
        _safe_extract(blob, private_root / "inspection")
        tarball = private_root / "wrangler.tgz"
        tarball.write_bytes(blob)
        runtime = private_root / "runtime"
        subprocess.run(
            [
                "npm", "install", "--prefix", str(runtime), "--ignore-scripts", "--no-package-lock",
                "--no-save", "--no-audit", "--no-fund", str(tarball),
            ],
            check=True,
            capture_output=True,
            text=True,
            timeout=120,
        )
        entrypoint = runtime / "node_modules" / "wrangler" / "bin" / "wrangler.js"
        if not entrypoint.is_file():
            raise ValueError("verified Wrangler package was not installed at its expected entrypoint")
        yield VerifiedWrangler(entrypoint, computed_integrity)


def _wrangler_json(*args: str, runner: VerifiedWrangler | None = None) -> tuple[Any, bool]:
    if runner is None:
        raise ValueError("provider calls must use a verified Wrangler package")
    return runner.json(*args)


def _validate_whoami(identity: Any) -> dict[str, Any]:
    if not isinstance(identity, dict) or identity.get("loggedIn") is not True or identity.get("authType") != EXPECTED_AUTH_TYPE:
        raise ValueError("Wrangler whoami is not an authenticated expected identity")
    if identity.get("email") != EXPECTED_AUTH_EMAIL:
        raise ValueError("Wrangler whoami account email drifted")
    accounts = identity.get("accounts")
    if not isinstance(accounts, list) or len(accounts) != 1 or not isinstance(accounts[0], dict) or accounts[0].get("id") != EXPECTED_ACCOUNT_ID:
        raise ValueError("Wrangler whoami account binding drifted")
    return {
        "account_id": EXPECTED_ACCOUNT_ID,
        "email": EXPECTED_AUTH_EMAIL,
        "auth_type": EXPECTED_AUTH_TYPE,
        "logged_in": True,
    }


def _collect(json_reader: Callable[..., tuple[Any, bool]]) -> dict[str, Any]:
    identity_response, identity_authenticated = json_reader("whoami")
    identity = _validate_whoami(identity_response)
    if not identity_authenticated:
        raise ValueError("Wrangler whoami authentication evidence is unavailable")
    deployments, deployments_authenticated = json_reader("deployments", "list", "--name", WORKER)
    if not deployments_authenticated:
        raise ValueError("Wrangler deployment API result was not authenticated/successful")
    deployment = next((item for item in deployments if item.get("id") == EXPECTED_DEPLOYMENT_ID), None)
    if not isinstance(deployment, dict):
        raise ValueError("pinned deployment was not returned by Wrangler")
    versions = deployment.get("versions")
    if not isinstance(versions, list) or not any(
        item.get("version_id") == EXPECTED_VERSION_ID and item.get("percentage") == EXPECTED_ROLLOUT_PERCENTAGE
        for item in versions
        if isinstance(item, dict)
    ):
        raise ValueError("pinned deployment does not serve the pinned version at 100%")
    version, version_authenticated = json_reader("versions", "view", EXPECTED_VERSION_ID, "--name", WORKER)
    if not version_authenticated:
        raise ValueError("Wrangler version API result was not authenticated/successful")
    if not isinstance(version, dict) or version.get("id") != EXPECTED_VERSION_ID or version.get("number") != EXPECTED_VERSION_NUMBER:
        raise ValueError("pinned version metadata drifted")
    script = version.get("resources", {}).get("script", {})
    script_etag = script.get("etag") if isinstance(script, dict) else None
    if script_etag != EXPECTED_SCRIPT_ETAG:
        raise ValueError("provider response lacks the pinned script content digest")
    authenticated = identity_authenticated and deployments_authenticated and version_authenticated
    if not authenticated:
        raise ValueError("Wrangler/API authentication evidence is unavailable")
    receipt = {
        "schema": "corelink-b006-provider-binding-v3",
        "provider": PROVIDER,
        "evidence_source": "Cloudflare Wrangler deployments/versions API",
        "authenticated": authenticated,
        "credential_values_printed": False,
        "auth_identity": identity,
        "wrangler_version": EXPECTED_WRANGLER_VERSION,
        "wrangler_lock_source": LOCK_SOURCE,
        "wrangler_package_integrity": "",
        "deployment_api_success": deployments_authenticated,
        "version_api_success": version_authenticated,
        "worker": WORKER,
        "metrics_source": SOURCE,
        "deployment_id": EXPECTED_DEPLOYMENT_ID,
        "version_id": EXPECTED_VERSION_ID,
        "version_number": EXPECTED_VERSION_NUMBER,
        "script_etag": script_etag,
        "rollout_percentage": EXPECTED_ROLLOUT_PERCENTAGE,
        "captured_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
    }
    return receipt


def collect(json_reader: Callable[..., tuple[Any, bool]] | None = None) -> dict[str, Any]:
    if json_reader is not None:
        receipt = _collect(json_reader)
        receipt["wrangler_package_integrity"] = read_lock_integrity()
        validate_provider_binding(receipt)
        return receipt
    with _verified_wrangler() as runner:
        receipt = _collect(lambda *args: _wrangler_json(*args, runner=runner))
        receipt["wrangler_package_integrity"] = runner.integrity
        validate_provider_binding(receipt)
        return receipt


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        receipt = collect()
    except (OSError, subprocess.SubprocessError, ValueError, json.JSONDecodeError) as exc:
        print(f"B-006 provider collection FAIL: {exc}")
        return 2
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"provider": receipt["provider"], "version_id": receipt["version_id"], "rollout_percentage": receipt["rollout_percentage"]}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
