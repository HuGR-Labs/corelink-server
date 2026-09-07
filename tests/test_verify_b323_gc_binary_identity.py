from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b323_gc_binary_identity as verify


def text(path: str) -> str:
    return (verify.ROOT / path).read_text(encoding="utf-8")


def rejects(path: str, mutated: str) -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={path: mutated})


def test_current_tree_and_embedded_mutations_pass():
    verify.verify()
    verify.self_test()


def test_auto_discovery_regression_fails_closed():
    source = text(verify.SERVER_MANIFEST)
    rejects(verify.SERVER_MANIFEST, source.replace("autobins = false", "autobins = true", 1))


def test_binary_name_collision_fails_closed():
    source = text(verify.SERVER_MANIFEST)
    rejects(verify.SERVER_MANIFEST, source.replace("corelink-gc-sweep-production", "gc_sweep", 1))


def test_subprocess_stderr_deletion_fails_closed():
    source = text(verify.FIXTURE_TEST)
    rejects(verify.FIXTURE_TEST, source.replace("stderr: String", "diagnostic: String", 1))
