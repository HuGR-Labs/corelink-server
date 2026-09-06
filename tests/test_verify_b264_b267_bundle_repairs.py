from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))

import verify_b264_b267_bundle_repairs as verify


def _text(path: str) -> str:
    return (verify.ROOT / path).read_text(encoding="utf-8")


def _reject(path: str, old: str, new: str) -> None:
    source = _text(path)
    mutated = source.replace(old, new, 1)
    assert mutated != source
    try:
        verify.verify(overrides={path: mutated})
    except verify.VerificationError:
        return
    raise AssertionError(f"B-264..B-267 accepted mutation in {path}: {old!r}")


def test_current_contract_passes() -> None:
    verify.verify()


def test_b264_missing_field_doc_is_rejected() -> None:
    _reject(verify.ALERTS, "        /// Channel missing endpoint/provider configuration.\n", "")


def test_b264_comment_bait_is_rejected() -> None:
    _reject(verify.ALERTS, "        channel: AlertChannel,", "        // channel: AlertChannel,")


def test_b265_indexing_regression_is_rejected() -> None:
    _reject(verify.SEALED, "lines.get(..0).unwrap_or(&[])", "&lines[..0]")


def test_b265_map_or_regression_is_rejected() -> None:
    _reject(
        verify.SEALED,
        "!= Ok(claimed)",
        ".map_or(true, |computed| computed != claimed)",
    )


def test_b266_missing_case_depth_is_rejected() -> None:
    _reject(verify.MIGRATIONS, "    let mut case_depth: u32 = 0;\n", "")


def test_b266_missing_case_end_branch_is_rejected() -> None:
    _reject(
        verify.MIGRATIONS,
        '            } else if upper == "END" && case_depth > 0 {\n',
        '            } else if upper == "NEVER" && case_depth > 0 {\n',
    )


def test_b266_regression_test_is_rejected_when_disabled() -> None:
    _reject(
        verify.MIGRATIONS,
        "    fn split_statements_respects_case_end_inside_trigger_body() {",
        "    fn disabled_split_statements_respects_case_end_inside_trigger_body() {",
    )


def test_each_b267_path_binding_is_load_bearing() -> None:
    for parent, child in verify.AUDIT_MODULES.items():
        _reject(parent, f'#[path = "{child}"]\n', "")


def test_b267_commented_binding_is_rejected() -> None:
    parent, child = next(iter(verify.AUDIT_MODULES.items()))
    _reject(
        parent,
        f'#[path = "{child}"]\nmod {child[:-3]};',
        f'// #[path = "{child}"]\n// mod {child[:-3]};',
    )
