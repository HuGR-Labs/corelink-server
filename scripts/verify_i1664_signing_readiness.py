#!/usr/bin/env python3
"""Validate the credentialless release signing readiness contract.

This verifier is deliberately offline. It reads only the owner packet and
never contacts a signing service, imports a key, reads an Actions secret, or
produces a signature/notarization receipt. ``--emit-preflight-ready`` checks
identity, bounded access, and expiry before any release mutation. ``ready``
remains strict and requires an independent verification receipt for every
lane.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PACKET = ROOT / "docs/handoff/2026-09-22-i1664-signing-readiness.json"
LANES = ("gpg", "windows", "apple")
SECRET_NAMES = {
    "gpg": (
        "GPG_PRIVATE_KEY",
        "GPG_PRIVATE_KEY_PASS",
        "GPG_KEY_ID",
        "GPG_KEY_FINGERPRINT",
    ),
    "windows": (
        "WINDOWS_CODE_SIGNING_CERT",
        "WINDOWS_CODE_SIGNING_PASSWORD",
        "WINDOWS_CODE_SIGNING_FINGERPRINT",
        "WINDOWS_CODE_SIGNING_SUBJECT",
    ),
    "apple": (
        "APPLE_DEVELOPER_ID",
        "APPLE_DEVELOPER_ID_PASSWORD",
        "APPLE_TEAM_ID",
        "APPLE_NOTARIZATION_API_KEY",
        "APPLE_NOTARIZATION_KEY_ID",
        "APPLE_NOTARIZATION_ISSUER",
        "APPLE_DEVELOPER_ID_FINGERPRINT",
    ),
}
FORBIDDEN_SECRET_MARKERS = (
    "-----BEGIN ",
    "PRIVATE KEY-----",
    "github_pat_",
    "ghp_",
    "gho_",
    "sk_live_",
    "whsec_",
)
SHA256 = re.compile(r"^[0-9A-Fa-f]{64}$")
AUTHORIZED_SCOPES = {
    "windows": "release-cli Windows signer workflow only",
    "apple": "release-cli macOS notarization workflow only",
}
PLACEHOLDER_FINGERPRINTS = {"0123456789abcdef" * 4}


class ContractError(ValueError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ContractError(message)


def nonempty(value: Any, path: str) -> None:
    require(isinstance(value, str) and bool(value.strip()), f"{path} must be a non-empty string")


def validate_certificate_fingerprint(value: Any, path: str) -> None:
    nonempty(value, path)
    require(SHA256.fullmatch(value) is not None, f"{path} must be a 64-digit SHA-256 fingerprint")
    normalized = value.lower()
    require(len(set(normalized)) > 1, f"{path} must not be a placeholder fingerprint")
    require(normalized not in PLACEHOLDER_FINGERPRINTS, f"{path} must not be a placeholder fingerprint")


def walk_strings(value: Any, path: str = "packet") -> None:
    if isinstance(value, str):
        upper = value.upper()
        for marker in FORBIDDEN_SECRET_MARKERS:
            require(marker.upper() not in upper, f"{path} contains private/credential material")
    elif isinstance(value, dict):
        for key, child in value.items():
            walk_strings(child, f"{path}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            walk_strings(child, f"{path}[{index}]")


def parse_date(value: Any, path: str) -> dt.datetime:
    nonempty(value, path)
    try:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ContractError(f"{path} must be ISO-8601") from exc
    require(parsed.tzinfo is not None, f"{path} must include a timezone")
    return parsed.astimezone(dt.timezone.utc)


def validate_lane(name: str, lane: dict[str, Any]) -> tuple[bool, bool]:
    required = {"status", "identity", "access", "expiry", "verification", "blockers"}
    require(set(lane) == required, f"{name} lane fields drifted: expected {sorted(required)}")
    require(lane["status"] in {"blocked", "preflight_ready", "ready"}, f"{name}.status is invalid")
    require(isinstance(lane["blockers"], list), f"{name}.blockers must be a list")
    require(all(isinstance(item, str) and item.strip() for item in lane["blockers"]), f"{name}.blockers contains an empty item")

    identity = lane["identity"]
    require(isinstance(identity, dict), f"{name}.identity must be an object")
    for key in ("fingerprint", "issuer", "subject_or_team", "chain_or_profile"):
        require(key in identity, f"{name}.identity.{key} is missing")
    if name in AUTHORIZED_SCOPES and identity["fingerprint"] is not None:
        validate_certificate_fingerprint(identity["fingerprint"], f"{name}.identity.fingerprint")

    access = lane["access"]
    require(isinstance(access, dict), f"{name}.access must be an object")
    require(access.get("secret_names") == list(SECRET_NAMES[name]), f"{name}.access.secret_names drifted")
    for key in ("scope", "rotation_owner", "last_rotated_at", "access_status"):
        require(key in access, f"{name}.access.{key} is missing")
    nonempty(access["scope"], f"{name}.access.scope")
    if name in AUTHORIZED_SCOPES:
        require(access["scope"] == AUTHORIZED_SCOPES[name], f"{name}.access.scope is not an authorized bounded scope")
    nonempty(access["rotation_owner"], f"{name}.access.rotation_owner")
    require(access["access_status"] in {"missing", "verified"}, f"{name}.access.access_status is invalid")
    if access["last_rotated_at"] is not None:
        parse_date(access["last_rotated_at"], f"{name}.access.last_rotated_at")

    expiry = lane["expiry"]
    require(isinstance(expiry, dict), f"{name}.expiry must be an object")
    require(set(expiry) == {"expires_at", "status"}, f"{name}.expiry fields drifted")
    require(expiry["status"] in {"missing", "valid"}, f"{name}.expiry.status is invalid")
    expiry_date = None if expiry["expires_at"] is None else parse_date(expiry["expires_at"], f"{name}.expiry.expires_at")
    if expiry["status"] == "valid":
        require(expiry_date is not None and expiry_date > dt.datetime.now(dt.timezone.utc), f"{name} expiry is not in the future")

    verification = lane["verification"]
    require(isinstance(verification, dict), f"{name}.verification must be an object")
    require(set(verification) == {"status", "receipt_reference", "verified_at", "method"}, f"{name}.verification fields drifted")
    require(verification["status"] in {"missing", "verified"}, f"{name}.verification.status is invalid")
    if verification["status"] == "verified":
        nonempty(verification["receipt_reference"], f"{name}.verification.receipt_reference")
        parse_date(verification["verified_at"], f"{name}.verification.verified_at")
        nonempty(verification["method"], f"{name}.verification.method")
    else:
        require(verification["receipt_reference"] is None and verification["verified_at"] is None, f"{name} missing verification must not claim a receipt")

    preflight_ready = lane["status"] in {"preflight_ready", "ready"}
    if preflight_ready:
        for key in ("fingerprint", "issuer", "subject_or_team", "chain_or_profile"):
            nonempty(identity[key], f"{name}.identity.{key}")
        require(access["access_status"] == "verified", f"{name} access is not verified")
        require(expiry["status"] == "valid", f"{name} expiry is not valid")

    ready = lane["status"] == "ready"
    if ready:
        require(verification["status"] == "verified", f"{name} verification receipt is missing")
        require(not lane["blockers"], f"{name} is ready but has blockers")
    return preflight_ready, ready


def validate(packet: Any) -> bool:
    require(isinstance(packet, dict), "packet must be a JSON object")
    required = {"schema_version", "status", "captured_at", "owner", "credentialless", "lanes", "redaction"}
    require(set(packet) == required, f"packet fields drifted: expected {sorted(required)}")
    require(packet["schema_version"] == "i1664.signing-readiness.v1", "unsupported schema_version")
    require(packet["status"] in {"blocked", "preflight_ready", "ready"}, "packet.status is invalid")
    parse_date(packet["captured_at"], "captured_at")
    nonempty(packet["owner"], "owner")
    require(packet["credentialless"] is True, "packet must be credentialless")
    require(packet["redaction"] == {"private_material_present": False, "secret_values_recorded": False}, "redaction contract drifted")
    require(isinstance(packet["lanes"], dict) and set(packet["lanes"]) == set(LANES), "lane set drifted")
    walk_strings(packet)
    states = [validate_lane(name, packet["lanes"][name]) for name in LANES]
    preflight_ready = packet["status"] in {"preflight_ready", "ready"} and all(state[0] for state in states)
    require(packet["status"] not in {"preflight_ready", "ready"} or preflight_ready, "preflight-ready packet has incomplete identity, access, or expiry data")
    ready = packet["status"] == "ready" and all(state[1] for state in states)
    require(packet["status"] != "ready" or ready, "ready packet has incomplete signing receipts")
    return ready


def validate_preflight(packet: Any) -> bool:
    """Return true only when every lane is authorized for a signing attempt."""
    validate(packet)
    if packet["status"] not in {"preflight_ready", "ready"}:
        return False
    return all(validate_lane(name, packet["lanes"][name])[0] for name in LANES)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--packet", type=Path, default=DEFAULT_PACKET)
    output_mode = parser.add_mutually_exclusive_group()
    output_mode.add_argument("--emit-ready", action="store_true", help="print true/false when all post-run verification receipts exist")
    output_mode.add_argument("--emit-preflight-ready", action="store_true", help="print true/false when identity, access, and expiry authorize signing")
    args = parser.parse_args()
    try:
        packet = json.loads(args.packet.read_text(encoding="utf-8"))
        ready = validate_preflight(packet) if args.emit_preflight_ready else validate(packet)
    except (OSError, json.JSONDecodeError, ContractError) as exc:
        print(f"signing readiness contract failed: {exc}", file=sys.stderr)
        return 1
    if args.emit_ready:
        print("true" if ready else "false")
    else:
        if args.emit_preflight_ready:
            print("READY: all platform identity, access, and expiry inputs authorize signing" if ready else "BLOCKED: external signing identity, access, or expiry inputs are missing")
        else:
            print("READY: all platform identities, expiry, access, and verification receipts are present" if ready else "BLOCKED: external signing identities or verification receipts are missing")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
