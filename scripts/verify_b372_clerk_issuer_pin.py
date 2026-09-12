#!/usr/bin/env python3
"""Verify B-372's production Clerk issuer pin and its trust-boundary wiring."""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CANONICAL_ISSUER = "https://clerk.corelink-app.humangr.com"
RETIRED_APP_HOST = "https://corelink-app.humangr.com"
OBSOLETE_CLERK_HOST = "https://clerk.corelink.humangr.com"
REGIONAL_ENVS = ("prod-sam", "prod-lhr", "prod-nrt", "prod-syd")


class VerificationError(ValueError):
    """The checked tree does not preserve the B-372 contract."""


def _parse_toml(text: str) -> dict[str, object]:
    try:
        value = tomllib.loads(text)
    except tomllib.TOMLDecodeError as exc:
        raise VerificationError(f"wrangler.toml is invalid TOML: {exc}") from exc
    if not isinstance(value, dict):
        raise VerificationError("wrangler.toml root is not a table")
    return value


def verify_wrangler(text: str) -> None:
    config = _parse_toml(text)
    envs = config.get("env")
    if not isinstance(envs, dict):
        raise VerificationError("wrangler.toml has no [env] table")
    prod = envs.get("prod")
    if not isinstance(prod, dict):
        raise VerificationError("wrangler.toml has no [env.prod] table")
    prod_vars = prod.get("vars")
    if not isinstance(prod_vars, dict):
        raise VerificationError("wrangler.toml has no [env.prod].vars")
    actual = prod_vars.get("CLERK_ISSUER_URL")
    if actual != CANONICAL_ISSUER:
        if actual is None:
            detail = "missing"
        elif actual in {RETIRED_APP_HOST, OBSOLETE_CLERK_HOST}:
            detail = f"retired/obsolete host {actual!r}"
        else:
            detail = f"wrong value {actual!r}"
        raise VerificationError(
            f"[env.prod].vars CLERK_ISSUER_URL is {detail}; "
            f"expected {CANONICAL_ISSUER!r}"
        )
    for env_name in REGIONAL_ENVS:
        region = envs.get(env_name)
        if not isinstance(region, dict):
            raise VerificationError(f"wrangler.toml has no [env.{env_name}] table")
        region_vars = region.get("vars", {})
        if not isinstance(region_vars, dict):
            raise VerificationError(f"wrangler.toml [env.{env_name}].vars is malformed")
        if "CLERK_ISSUER_URL" in region_vars:
            raise VerificationError(
                f"[env.{env_name}].vars must not gain the IAD-only Clerk issuer pin"
            )


def verify_strict_equality(source: str) -> None:
    # Bind the decision to executable source, not comments or documentation.
    without_comments = re.sub(r"/\*.*?\*/", "", source, flags=re.DOTALL)
    without_comments = re.sub(r"//[^\n]*", "", without_comments)
    exact = re.compile(r"\biss\s*!==\s*clerkIssuerUrl\b")
    if exact.search(without_comments) is None:
        raise VerificationError("Clerk auth no longer rejects iss !== clerkIssuerUrl")
    weakened = (
        r"iss\.(?:includes|startsWith|endsWith)\(\s*clerkIssuerUrl\s*\)",
        r"clerkIssuerUrl\.(?:includes|startsWith|endsWith)\(\s*iss\s*\)",
    )
    if any(re.search(pattern, without_comments) for pattern in weakened):
        raise VerificationError("Clerk issuer boundary contains a shape/substring comparison")


def _matrix_row(text: str, env_name: str) -> list[str]:
    rows: list[list[str]] = []
    for line in text.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) >= 3 and cells[2] == f"`{env_name}`":
            rows.append(cells)
    if len(rows) != 1:
        raise VerificationError(
            f"secrets checklist must contain exactly one {env_name} row; found {len(rows)}"
        )
    return rows[0]


def verify_checklist(text: str) -> None:
    row = _matrix_row(text, "CLERK_ISSUER_URL")
    if len(row) != 11:
        raise VerificationError("CLERK_ISSUER_URL checklist row is malformed")
    rendered = " | ".join(row)
    for required in (
        "non-secret",
        "[env.prod].vars",
        CANONICAL_ISSUER,
        RETIRED_APP_HOST,
        "Regional Workers intentionally have no Clerk surface",
    ):
        if required not in rendered:
            raise VerificationError(f"CLERK_ISSUER_URL checklist row lacks {required!r}")
    stored_at = row[10]
    if "wrangler secret put" in rendered or "cf-wrangler" in stored_at:
        raise VerificationError("public issuer pin is still classified as a Worker secret")


def verify_runbook(text: str) -> None:
    required = (
        CANONICAL_ISSUER,
        "https://humangr.com/corelink",
        RETIRED_APP_HOST,
        "strict equality",
        "regional",
    )
    for marker in required:
        if marker not in text:
            raise VerificationError(f"deploy runbook lacks issuer distinction {marker!r}")


def _valid_fixture() -> str:
    regions = "\n".join(
        f'[env.{name}]\nname = "corelink-{name}"\n[env.{name}.vars]\nENVIRONMENT = "{name}"'
        for name in REGIONAL_ENVS
    )
    return (
        '[env.prod]\nname = "corelink-prod"\n'
        f'[env.prod.vars]\nCLERK_ISSUER_URL = "{CANONICAL_ISSUER}"\n'
        + regions
        + "\n"
    )


def self_test() -> None:
    baseline = _valid_fixture()
    verify_wrangler(baseline)
    mutations = {
        "missing": baseline.replace(
            f'CLERK_ISSUER_URL = "{CANONICAL_ISSUER}"\n', "", 1
        ),
        "wrong": baseline.replace(CANONICAL_ISSUER, "https://clerk.example.invalid", 1),
        "retired-app-host": baseline.replace(CANONICAL_ISSUER, RETIRED_APP_HOST, 1),
        "obsolete-clerk-host": baseline.replace(CANONICAL_ISSUER, OBSOLETE_CLERK_HOST, 1),
        "regional-copy": baseline.replace(
            '[env.prod-sam.vars]\n',
            f'[env.prod-sam.vars]\nCLERK_ISSUER_URL = "{CANONICAL_ISSUER}"\n',
            1,
        ),
    }
    for name, mutation in mutations.items():
        try:
            verify_wrangler(mutation)
        except VerificationError:
            continue
        raise AssertionError(f"mutation survived: {name}")

    strict = "if (iss !== clerkIssuerUrl) { return unauthorized; }"
    verify_strict_equality(strict)
    for name, mutation in {
        "missing equality": "return authorized;",
        "substring": "if (!iss.startsWith(clerkIssuerUrl)) { return unauthorized; }",
        "wrong operator": "if (iss === clerkIssuerUrl) { return unauthorized; }",
    }.items():
        try:
            verify_strict_equality(mutation)
        except VerificationError:
            continue
        raise AssertionError(f"strict-equality mutation survived: {name}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.self_test:
            self_test()
        verify_wrangler((ROOT / "wrangler.toml").read_text(encoding="utf-8"))
        verify_strict_equality(
            (ROOT / "worker/src/lib/clerk_auth.ts").read_text(encoding="utf-8")
        )
        verify_checklist(
            (ROOT / "docs/internal/secrets-checklist.md").read_text(encoding="utf-8")
        )
        verify_runbook(
            (ROOT / "docs/knowledge/ops/deploy-runbook.md").read_text(encoding="utf-8")
        )
    except (OSError, VerificationError, AssertionError) as exc:
        print(f"B-372 issuer pin: FAIL: {exc}", file=sys.stderr)
        return 1
    print("B-372 issuer pin: PASS: canonical IAD-only versioned pin; strict issuer equality")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
