#!/usr/bin/env python3
"""Fail-closed verifier for the redacted B-229 production evidence packet."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_EVIDENCE = ROOT / "evidence/production/B-229-clerk-webhook-2026-09-08.md"


class VerificationError(RuntimeError):
    pass


REQUIRED = {
    "item": "B-229",
    "worker": "corelink-signup-worker",
    "route": "https://corelink-signup.humangr.com/webhooks/clerk",
    "source_sha": "cba0e59655131c09bd5255ac783cac977a206da8",
    "deployed_version_id": "64282952-57bf-49d5-832c-b4cc4723b8b3",
    "deployed_version_number": "103",
    "traffic": "100%",
    "deployed_at": "2026-09-08T18:58:57.510834Z",
    "signed_probe_http_status": "200",
    "signed_probe_result": "ignored",
    "health_http_status": "200",
    "secret_rotation": "not_required_existing_binding_verified",
    "payload_and_secret_values": "omitted",
}


def read_evidence(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        raise VerificationError(f"evidence packet unavailable: {path}: {error}") from error


def verify_text(text: str) -> None:
    if "```yaml" not in text or "```" not in text.split("```yaml", 1)[1]:
        raise VerificationError("evidence packet has no bounded YAML evidence block")
    for key, value in REQUIRED.items():
        if not re.search(rf"(?m)^{re.escape(key)}: {re.escape(value)}$", text):
            raise VerificationError(f"missing or changed evidence field: {key}")
    forbidden = (
        r"whsec_[A-Za-z0-9_./:+=-]+",
        r"sk_(?:live|test)_[A-Za-z0-9_./:+=-]+",
        r"svix-signature:\s*\S+",
        r"v1,[A-Za-z0-9+/=_-]+",
        r'\{"type"\s*:\s*[^}]+\}',
    )
    for pattern in forbidden:
        if re.search(pattern, text, flags=re.IGNORECASE):
            raise VerificationError("credential, signature, or payload value leaked")
    if "no webhook payload" not in text.lower() or "credential value" not in text.lower():
        raise VerificationError("redaction statement missing")


def verify_repo(path: Path = DEFAULT_EVIDENCE) -> None:
    verify_text(read_evidence(path))


MUTATIONS = (
    ("sha", "cba0e59655131c09bd5255ac783cac977a206da8", "0000000000000000000000000000000000000000000000000000000000000000"),
    ("version", "64282952-57bf-49d5-832c-b4cc4723b8b3", "00000000-0000-0000-0000-000000000000"),
    ("traffic", "traffic: 100%", "traffic: 50%"),
    ("probe", "signed_probe_result: ignored", "signed_probe_result: accepted"),
    ("health", "health_http_status: 200", "health_http_status: 500"),
    ("redaction", "payload_and_secret_values: omitted", "payload_and_secret_values: present"),
)


def verify_mutations(path: Path = DEFAULT_EVIDENCE) -> int:
    original = read_evidence(path)
    passed = 0
    for name, old, new in MUTATIONS:
        mutated = original.replace(old, new, 1)
        if mutated == original:
            raise VerificationError(f"mutation {name} did not change evidence")
        try:
            verify_text(mutated)
        except VerificationError:
            passed += 1
        else:
            raise VerificationError(f"mutation {name} was accepted")
    return passed


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence", type=Path, default=DEFAULT_EVIDENCE)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    try:
        verify_repo(args.evidence)
        mutations = verify_mutations(args.evidence) if args.self_test else 0
    except VerificationError as error:
        print(f"B229 FAIL: {error}", file=sys.stderr)
        return 1
    suffix = f"; mutations={mutations}/{len(MUTATIONS)}" if args.self_test else ""
    print(f"B229 PASS: redacted production evidence is complete{suffix}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
