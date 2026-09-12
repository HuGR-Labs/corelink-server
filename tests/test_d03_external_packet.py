"""Focused executable checks for the D03 external/production packet."""

from pathlib import Path
import sys

import pytest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import verify_d03_external_packet as packet  # noqa: E402


def test_external_packet_is_structurally_valid_and_reports_owner_blockers() -> None:
    result = packet.verify(ROOT)
    assert set(result) == {"B-216", "B-229"}
    assert {item["status"] for item in result.values()} == {"OWNER_BLOCKED"}
    assert result["B-216"]["blocker_receipt"] == {
        "path": packet.B216_BLOCKER_RECEIPT,
        "status": "captured",
    }


@pytest.mark.parametrize("lane", tuple(packet.CHECKS))
def test_each_external_lane_is_independently_callable(lane: str) -> None:
    result = packet.verify(ROOT, lane)
    assert result[lane]["status"] in {"READY_FOR_BUNDLE", "OWNER_BLOCKED"}


def test_b216_rejects_removed_dlq_dispatch(tmp_path: Path) -> None:
    import shutil

    for relative in (
        "apps/signup-worker/src/webhooks/dsr_consumer.ts",
        "apps/signup-worker/src/index.ts",
        "apps/signup-worker/wrangler.toml",
        ".github/workflows/signup-worker-deploy.yml",
        "docs/internal/b215-b230-runtime-owner-actions.md",
    ):
        destination = tmp_path / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(ROOT / relative, destination)
    source = tmp_path / "apps/signup-worker/src/index.ts"
    source.write_text(
        source.read_text(encoding="utf-8").replace("await handleErasureDlqBatch(", "await removedHandler(", 1),
        encoding="utf-8",
    )
    with pytest.raises(packet.PacketError, match="handleErasureDlqBatch"):
        packet.check_b216(tmp_path)


def test_b216_rejects_renamed_dlq_binding(tmp_path: Path) -> None:
    import shutil

    for relative in (
        "apps/signup-worker/src/webhooks/dsr_consumer.ts",
        "apps/signup-worker/src/index.ts",
        "apps/signup-worker/wrangler.toml",
        "docs/internal/b215-b230-runtime-owner-actions.md",
    ):
        destination = tmp_path / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(ROOT / relative, destination)
    config = tmp_path / "apps/signup-worker/wrangler.toml"
    config.write_text(
        config.read_text(encoding="utf-8").replace(
            'queue = "corelink-dsr-erasure-dlq"\nmax_batch_size',
            'queue = "renamed-dlq"\nmax_batch_size',
            1,
        ),
        encoding="utf-8",
    )
    with pytest.raises(packet.PacketError, match="DLQ consumer binding"):
        packet.check_b216(tmp_path)


def test_b216_ignores_comment_decoys(tmp_path: Path) -> None:
    import shutil

    for relative in (
        "apps/signup-worker/src/webhooks/dsr_consumer.ts",
        "apps/signup-worker/src/index.ts",
        "apps/signup-worker/wrangler.toml",
        ".github/workflows/signup-worker-deploy.yml",
        "docs/internal/b215-b230-runtime-owner-actions.md",
    ):
        destination = tmp_path / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(ROOT / relative, destination)
    source = tmp_path / "apps/signup-worker/src/index.ts"
    source.write_text(
        "// await handleErasureDlqBatch(\n" + source.read_text(encoding="utf-8"),
        encoding="utf-8",
    )
    assert packet.check_b216(tmp_path)["status"] == "OWNER_BLOCKED"


@pytest.mark.parametrize(
    ("old", "new", "message"),
    (
        (
            "printf '%s' \"$PAGERDUTY_ROUTING_KEY\" | wrangler secret put",
            "wrangler secret put",
            "PagerDuty secret sync",
        ),
        (
            "Synchronize PagerDuty routing key",
            "Synchronize disabled routing key",
            "unique PagerDuty sync",
        ),
        (
            "PAGERDUTY_ROUTING_KEY: ${{ secrets.PAGERDUTY_ROUTING_KEY }}",
            "PAGERDUTY_ROUTING_KEY: ${{ secrets.UNRELATED_SECRET }}",
            "identically named repository secret",
        ),
        (
            "set -euo pipefail",
            'set -euo pipefail\n          echo "$PAGERDUTY_ROUTING_KEY"',
            "may be exposed",
        ),
    ),
)
def test_b216_rejects_weakened_pagerduty_secret_sync(
    tmp_path: Path, old: str, new: str, message: str
) -> None:
    import shutil

    for relative in (
        "apps/signup-worker/src/index.ts",
        "apps/signup-worker/wrangler.toml",
        ".github/workflows/signup-worker-deploy.yml",
        "docs/internal/b215-b230-runtime-owner-actions.md",
    ):
        destination = tmp_path / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(ROOT / relative, destination)
    workflow = tmp_path / ".github/workflows/signup-worker-deploy.yml"
    original = workflow.read_text(encoding="utf-8")
    assert old in original
    workflow.write_text(original.replace(old, new, 1), encoding="utf-8")
    with pytest.raises(packet.PacketError, match=message):
        packet.check_b216(tmp_path)


def test_b229_rejects_removed_secret_and_reordered_gate(tmp_path: Path) -> None:
    import shutil

    for relative in (
        "scripts/verify-signup-worker-secrets.sh",
        ".github/workflows/signup-worker-deploy.yml",
        "docs/internal/b215-b230-runtime-owner-actions.md",
    ):
        destination = tmp_path / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(ROOT / relative, destination)
    script = tmp_path / "scripts/verify-signup-worker-secrets.sh"
    script.write_text(
        script.read_text(encoding="utf-8").replace("  CLERK_WEBHOOK_SECRET\n", "", 1),
        encoding="utf-8",
    )
    with pytest.raises(packet.PacketError, match="CLERK_WEBHOOK_SECRET"):
        packet.check_b229(tmp_path)

    original = (ROOT / ".github/workflows/signup-worker-deploy.yml").read_text(encoding="utf-8")
    verify = original.index("      - name: Verify required runtime secrets")
    deploy = original.index("      - name: Deploy Worker")
    reordered = original[:verify] + original[deploy:] + original[verify:deploy]
    (tmp_path / ".github/workflows/signup-worker-deploy.yml").write_text(reordered, encoding="utf-8")
    # Restore the secret so this assertion reaches the ordering contract.
    script.write_text((ROOT / "scripts/verify-signup-worker-secrets.sh").read_text(encoding="utf-8"), encoding="utf-8")
    with pytest.raises(packet.PacketError, match="must precede"):
        packet.check_b229(tmp_path)


def test_b229_ignores_comment_and_unrelated_step_bait(tmp_path: Path) -> None:
    import shutil

    for relative in (
        "scripts/verify-signup-worker-secrets.sh",
        ".github/workflows/signup-worker-deploy.yml",
        "docs/internal/b215-b230-runtime-owner-actions.md",
    ):
        destination = tmp_path / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(ROOT / relative, destination)
    workflow = tmp_path / ".github/workflows/signup-worker-deploy.yml"
    workflow.write_text(
        "# - name: Verify required runtime secrets\n" + workflow.read_text(encoding="utf-8")
        + "\n# run: bash ../../scripts/verify-signup-worker-secrets.sh\n",
        encoding="utf-8",
    )
    assert packet.check_b229(tmp_path)["status"] == "OWNER_BLOCKED"


def test_b170_truth_and_b226_exclusion_remain_explicit() -> None:
    assert "B-170" not in packet.CHECKS
    assert "B-226" not in packet.CHECKS
    b170 = (ROOT / "docs/internal/b087-questionnaire-owner-actions.md").read_text(encoding="utf-8")
    b226 = (ROOT / "docs/internal/b215-b230-runtime-owner-actions.md").read_text(encoding="utf-8")
    assert "no notification is claimed here" in b170
    assert "current truthful `Failed` outcomes" in b226
