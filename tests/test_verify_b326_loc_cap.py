from __future__ import annotations

from pathlib import Path

import pytest

import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b326_loc_cap as verify


def test_current_manifest_and_population_pass() -> None:
    verify.verify()
    verify.self_test()


def test_new_source_over_500_lines_is_rejected() -> None:
    baseline_paths, _ = verify._parse_manifest(verify.MANIFEST.read_text(encoding="utf-8"))
    target = sorted(verify._tracked_source_paths(verify.ROOT) - baseline_paths)[0]
    source = (verify.ROOT / target).read_text(encoding="utf-8")
    mutated = source + "\n" + "\n".join("// mutation" for _ in range(verify.MAX_LOC + 1))
    with pytest.raises(verify.VerificationError, match="HARD-CAP"):
        verify.verify(overrides={target: mutated})


def test_manifest_digest_and_baseline_identity_are_required() -> None:
    manifest = verify.MANIFEST.read_text(encoding="utf-8")
    with pytest.raises(verify.VerificationError):
        verify._parse_manifest(manifest.replace(verify.BASELINE_SHA, "0" * 40, 1))
    with pytest.raises(verify.VerificationError):
        verify._parse_manifest(manifest.replace("source-path-count: 2443", "source-path-count: 2442", 1))
