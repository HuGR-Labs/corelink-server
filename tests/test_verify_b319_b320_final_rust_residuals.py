from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b319_b320_final_rust_residuals as verify


def source(path: str) -> str:
    return (verify.ROOT / path).read_text(encoding="utf-8")


def rejects(path: str, text: str) -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={path: text})


def test_current_tree_and_embedded_mutations_pass():
    verify.verify()
    verify.self_test()


def test_b319_output_deletion_fails_closed():
    current = source(verify.BILLING)
    rejects(verify.BILLING, current.replace("std::io::stderr().lock(),", "std::io::sink(),", 1))


def test_b319_suppression_fails_closed():
    rejects(verify.BILLING, "#[allow(clippy::print_stderr)]\n" + source(verify.BILLING))


def test_b320_alias_deletion_fails_closed():
    current = source(verify.REGION)
    rejects(verify.REGION, current.replace('assert!(matches!(Region::from_str("afr"), Ok(Region::Afr)));', "", 1))


def test_b320_negative_case_deletion_fails_closed():
    current = source(verify.REGION)
    rejects(verify.REGION, current.replace('assert!(Region::from_str("").is_err());', "", 1))
