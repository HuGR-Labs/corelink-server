"""Mutation tests for the B-153 zero-reader gate."""

from __future__ import annotations

import re
import subprocess
import textwrap
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
BACKLOG = REPO_ROOT / "BACKLOG.md"


def _b153_verify() -> str:
    match = re.search(
        r"(?ms)^id: B-153\nrepo: corelink-server\n.*?^verify: \|\n(?P<body>.*?)(?=^verify-means:)",
        BACKLOG.read_text(encoding="utf-8"),
    )
    assert match, "B-153 verify block is missing"
    return textwrap.dedent(match.group("body"))


def _fixture(tmp_path: Path) -> None:
    matrix = tmp_path / "apps/docs/docs/explanation/rbac/permission-matrix.mdx"
    matrix.parent.mkdir(parents=True)
    matrix.write_text("| Role | Read |\n| --- | --- |\n| Admin | yes |\n| Developer | yes |\n", encoding="utf-8")
    cas = tmp_path / "crates/corelink-container/src/routes/cas.rs"
    cas.parent.mkdir(parents=True)
    cas.write_text("if scope.can_write() {\n    delete_blob();\n}\n", encoding="utf-8")
    (tmp_path / "scripts").mkdir()
    (tmp_path / ".github/workflows").mkdir(parents=True)
    # The canonical B-094 data manifest contains permission-matrix paths as
    # inventory data, so it remains excluded by its exact path.
    (tmp_path / "scripts" / "b094_published_inventory.json").write_text(
        '{"path": "apps/docs/docs/explanation/rbac/permission-matrix.mdx"}\n',
        encoding="utf-8",
    )


def _run(tmp_path: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["bash", "-c", _b153_verify()], cwd=tmp_path, capture_output=True, text=True, check=False)


def test_b153_baseline_has_no_reader(tmp_path: Path):
    _fixture(tmp_path)
    result = _run(tmp_path)
    assert result.returncode == 0, result.stdout + result.stderr


def test_b153_same_basename_in_nested_scripts_is_not_excluded(tmp_path: Path):
    _fixture(tmp_path)
    nested = tmp_path / "scripts" / "nested" / "b094_published_inventory.json"
    nested.parent.mkdir()
    nested.write_text(
        "grep permission-matrix.mdx apps/docs/docs/explanation/rbac/permission-matrix.mdx\n",
        encoding="utf-8",
    )
    result = _run(tmp_path)
    assert result.returncode == 1, result.stdout + result.stderr
    assert "1 instrumento(s)" in result.stdout + result.stderr
