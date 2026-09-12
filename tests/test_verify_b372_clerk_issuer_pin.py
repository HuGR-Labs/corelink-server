from __future__ import annotations

from pathlib import Path
import sys

import pytest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import verify_b372_clerk_issuer_pin as verify  # noqa: E402


def test_live_tree_contract_passes() -> None:
    assert verify.main(["--self-test"]) == 0


@pytest.mark.parametrize(
    ("label", "replacement"),
    [
        ("missing", None),
        ("wrong", "https://clerk.example.invalid"),
        ("retired app host", verify.RETIRED_APP_HOST),
        ("obsolete Clerk host", verify.OBSOLETE_CLERK_HOST),
    ],
)
def test_prod_pin_mutations_fail_closed(label: str, replacement: str | None) -> None:
    del label
    source = (ROOT / "wrangler.toml").read_text(encoding="utf-8")
    needle = f'CLERK_ISSUER_URL = "{verify.CANONICAL_ISSUER}"'
    assert source.count(needle) == 1
    mutation = source.replace(needle, "" if replacement is None else f'CLERK_ISSUER_URL = "{replacement}"', 1)
    with pytest.raises(verify.VerificationError):
        verify.verify_wrangler(mutation)


def test_region_cannot_inherit_clerk_surface() -> None:
    source = (ROOT / "wrangler.toml").read_text(encoding="utf-8")
    marker = '[env.prod-sam.vars]\n'
    assert source.count(marker) == 1
    mutation = source.replace(
        marker,
        marker + f'CLERK_ISSUER_URL = "{verify.CANONICAL_ISSUER}"\n',
        1,
    )
    with pytest.raises(verify.VerificationError, match="IAD-only"):
        verify.verify_wrangler(mutation)


def test_strict_equality_mutations_fail_closed() -> None:
    source = (ROOT / "worker/src/lib/clerk_auth.ts").read_text(encoding="utf-8")
    assert source.count("iss !== clerkIssuerUrl") == 1
    verify.verify_strict_equality(source)
    for replacement in (
        "iss === clerkIssuerUrl",
        "!iss.startsWith(clerkIssuerUrl)",
        "!iss.includes(clerkIssuerUrl)",
    ):
        with pytest.raises(verify.VerificationError):
            verify.verify_strict_equality(
                source.replace("iss !== clerkIssuerUrl", replacement, 1)
            )


def test_checklist_secret_regression_is_rejected() -> None:
    source = (ROOT / "docs/internal/secrets-checklist.md").read_text(encoding="utf-8")
    verify.verify_checklist(source)
    mutation = source.replace(
        "versioned `wrangler.toml` `[env.prod].vars` (non-secret)",
        "cf-wrangler",
        1,
    )
    with pytest.raises(verify.VerificationError, match="Worker secret"):
        verify.verify_checklist(mutation)
