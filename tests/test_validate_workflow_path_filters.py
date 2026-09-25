from __future__ import annotations

import importlib.util
import pathlib
import sys

import pytest


ROOT = pathlib.Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "validate_workflow_path_filters.py"


def _load():
    spec = importlib.util.spec_from_file_location("path_filters", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules["path_filters"] = module
    spec.loader.exec_module(module)
    return module


guard = _load()
FILES = ["live/file.rs"]


def _scan(tmp_path: pathlib.Path, content: str):
    workflows = tmp_path / ".github" / "workflows"
    workflows.mkdir(parents=True)
    (workflows / "fixture.yml").write_text(content, encoding="utf-8")
    return guard.scan(str(workflows), FILES)


DEAD = """name: fixture
on:
  push:
    paths:
      - 'dead/**'
"""


def test_baseline_allows_unchanged_dead_pattern(tmp_path: pathlib.Path) -> None:
    assert guard.new_findings(_scan(tmp_path / "head", DEAD).findings, _scan(tmp_path / "base", DEAD).findings) == []


def test_baseline_rejects_added_dead_pattern(tmp_path: pathlib.Path) -> None:
    head = _scan(tmp_path / "head", DEAD.replace("'dead/**'", "'dead/**'\n      - 'also-dead/**'"))
    base = _scan(tmp_path / "base", DEAD)
    assert [finding.pattern for finding in guard.new_findings(head.findings, base.findings)] == ["also-dead/**"]


def test_baseline_allows_removed_dead_pattern(tmp_path: pathlib.Path) -> None:
    clean = DEAD.replace("'dead/**'", "'live/**'")
    assert guard.new_findings(_scan(tmp_path / "head", clean).findings, _scan(tmp_path / "base", DEAD).findings) == []


def test_baseline_counts_duplicate_dead_pattern(tmp_path: pathlib.Path) -> None:
    head = _scan(tmp_path / "head", DEAD.replace("'dead/**'", "'dead/**'\n      - 'dead/**'"))
    base = _scan(tmp_path / "base", DEAD)
    assert len(guard.new_findings(head.findings, base.findings)) == 1


def test_baseline_ignores_line_shift(tmp_path: pathlib.Path) -> None:
    shifted = DEAD.replace("paths:\n", "paths:\n      # movement only\n")
    assert guard.new_findings(_scan(tmp_path / "head", shifted).findings, _scan(tmp_path / "base", DEAD).findings) == []


def test_empty_or_malformed_path_filter_scan_fails_closed(tmp_path: pathlib.Path) -> None:
    with pytest.raises(guard.PathFilterError, match="inspected 0"):
        _scan(tmp_path, "name: malformed\non:\n  push:\n    paths: [unterminated\n")


def test_missing_workflow_tree_fails_closed(tmp_path: pathlib.Path) -> None:
    with pytest.raises(guard.PathFilterError, match="not found"):
        guard.scan(str(tmp_path / "missing"), FILES)


def test_actionlint_uses_the_immutable_baseline_for_path_filters() -> None:
    workflow = (ROOT / ".github" / "workflows" / "actionlint.yml").read_text(encoding="utf-8")
    assert (
        'if [[ "${{ github.event_name }}" == "pull_request" || "${{ github.event_name }}" == "workflow_dispatch" ]]; then\n'
        "            python3 scripts/validate_workflow_path_filters.py \\\n"
        "              --baseline-workflows baseline/.github/workflows\n"
        "          else\n"
        "            python3 scripts/validate_workflow_path_filters.py\n"
        "          fi"
    ) in workflow
