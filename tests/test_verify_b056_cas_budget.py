from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b056_cas_budget as verify


def test_current_contract_and_mutation_suite_pass() -> None:
    files = verify.source()
    verify.assess(files)
    verify.mutation_checks(files)


@pytest.mark.parametrize("path", verify.CAS_PARTS)
def test_every_compiled_cas_fragment_is_required(
    path: str, monkeypatch: pytest.MonkeyPatch
) -> None:
    original = verify.read

    def read(target: Path) -> str:
        if target == verify.ROOT / path:
            raise verify.VerificationError(f"missing B-056 proof input: {path}")
        return original(target)

    monkeypatch.setattr(verify, "read", read)
    with pytest.raises(verify.VerificationError):
        verify.source()


@pytest.mark.parametrize(
    ("needle", "replacement"),
    (
        (
            "static GLOBAL_CAS_READ_BUDGET: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();",
            "static GLOBAL_CAS_READ_BUDGET: OnceLock<Arc<tokio::sync::MISSING>> = OnceLock::new();",
        ),
        (
            "\n                CAS_READ_BATCH_OBJECT_PERMITS,",
            "CAS_READ_SINGLE_PERMITS",
        ),
        (
            "_global_read_budget: GlobalCasBatchReadBudgetGuard",
            "_global_read_budget: MissingGuard",
        ),
    ),
)
def test_wrong_or_missing_current_cas_markers_fail(
    needle: str, replacement: str
) -> None:
    files = verify.source()
    assert needle in files["cas"]
    mutant = dict(files)
    mutant["cas"] = files["cas"].replace(needle, replacement, 1)
    with pytest.raises(verify.VerificationError):
        verify.assess(mutant)


def test_comment_markers_cannot_prove_real_guards() -> None:
    files = verify.source()
    mutant = dict(files)
    mutant["cas"] = files["cas"].replace(
        "_global_read_budget: GlobalCasReadBudgetGuard",
        "/* _global_read_budget: GlobalCasReadBudgetGuard */ MissingGuard",
        1,
    )
    with pytest.raises(verify.VerificationError):
        verify.assess(mutant)


def test_comment_markers_cannot_prove_weighted_acquire() -> None:
    files = verify.source()
    mutant = dict(files)
    mutant["cas"] = files["cas"].replace(
        "global_cas_read_budget().acquire_many_owned(permits)",
        "/* global_cas_read_budget().acquire_many_owned(permits) */ acquire_owned()",
        1,
    )
    with pytest.raises(verify.VerificationError):
        verify.assess(mutant)


def test_parent_include_removal_fails_closed() -> None:
    files = verify.source()
    mutant = dict(files)
    mutant["cas_module"] = files["cas_module"].replace(
        'include!("cas/batch_read.rs");',
        '/* include!("cas/batch_read.rs"); */',
        1,
    )
    with pytest.raises(verify.VerificationError):
        verify.assess(mutant)
