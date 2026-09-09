"""Mutation regression for the B083 common-purge quarantine contract."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INPUTS = (
    Path("scripts/verify_b083_common_purge.py"),
    Path("crates/corelink-container/src/storage/byok_generation_catalog.rs"),
    Path("crates/corelink-container/src/storage/byok_purge_io.rs"),
    Path("crates/corelink-container/src/main.rs"),
    Path("crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs"),
    Path("crates/corelink-container/src/storage/r2_s3_parts/ac_update.rs"),
    Path("crates/corelink-container/src/storage/byok_backfill_d1.rs"),
    Path("crates/corelink-container/src/storage.rs"),
    Path("migrations/d1/0121_byok_activation_pipeline.sql"),
    Path("migrations/d1/0125_b083_common_purge_quarantine.sql"),
    Path("migrations/d1/0126_b083_common_purge_r2_absent_quarantine.sql"),
)


def _copy_inputs(destination: Path) -> None:
    for relative in INPUTS:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


def _run(tree: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, "scripts/verify_b083_common_purge.py"],
        cwd=tree,
        text=True,
        capture_output=True,
        check=False,
    )


def test_b083_common_purge_bound_and_r2_absent_mutations_are_red() -> None:
    with tempfile.TemporaryDirectory(prefix="b083-common-purge-") as raw:
        tree = Path(raw)
        _copy_inputs(tree)

        green = _run(tree)
        assert green.returncode == 0, green.stdout + green.stderr

        catalog = tree / "crates/corelink-container/src/storage/byok_generation_catalog.rs"
        source = catalog.read_text(encoding="utf-8")
        mutated = source.replace("attempts>=?6", "attempts>?6", 1)
        assert mutated != source
        catalog.write_text(mutated, encoding="utf-8")

        red = _run(tree)
        assert red.returncode != 0, red.stdout + red.stderr

        # Reverting the unowned NULL-claim scheduler branch would strand every
        # r2_absent invalid identity after its first completion checkpoint.
        catalog.write_text(source, encoding="utf-8")
        scheduler_mutation = source.replace(
            "p.state='r2_absent' AND p.claim_owner IS NULL",
            "p.state='r2_absent' AND p.claim_owner IS NOT NULL",
            1,
        )
        assert scheduler_mutation != source
        catalog.write_text(scheduler_mutation, encoding="utf-8")

        red_scheduler = _run(tree)
        assert red_scheduler.returncode != 0, red_scheduler.stdout + red_scheduler.stderr

        # Quarantine is terminal and must not be reported as retryable; doing
        # so would trigger the supervisor's all-retryable rebuild/backoff.
        catalog.write_text(source, encoding="utf-8")
        driver = tree / "crates/corelink-container/src/storage/byok_purge_io.rs"
        driver_source = driver.read_text(encoding="utf-8")
        report_mutation = driver_source.replace(
            "ClaimOutcome::Quarantined => report.quarantined += 1",
            "ClaimOutcome::Quarantined => report.retryable += 1",
            1,
        )
        assert report_mutation != driver_source
        driver.write_text(report_mutation, encoding="utf-8")

        red_report = _run(tree)
        assert red_report.returncode != 0, red_report.stdout + red_report.stderr


if __name__ == "__main__":
    test_b083_common_purge_bound_and_r2_absent_mutations_are_red()
    print("B083 common-purge mutation: green baseline and named red mutant")
