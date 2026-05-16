#!/usr/bin/env python3
"""
validate_inv_inheritance_test.py — pytest suite for
`scripts/validate_inv_inheritance.py` (W26-P2-03).

Coverage:
- Happy path: a registry with valid bidirectional inheritance passes.
- Broken target: `inherits_from` cites a TLA file that does not exist
  on disk → validator errors with the expected message.
- Broken property: `inherits_from` cites a TLA file:Property pair where
  the property is absent from the spec → validator errors.
- Asymmetric link: child cites a parent but `parents.inherited_by` does
  not list the child → validator errors.
- Asymmetric inverse: `parents.inherited_by` lists a child that has no
  chain entry citing the parent → validator errors.
- Absent index: registry with no inheritance index block validates as
  vacuously OK (additivity contract).
- Live registry: the real `specs/03_architecture/invariant_registry.md`
  validates green end-to-end (W26-P2-03 closure smoke test).

Run:
    python3 -m pytest tests/validate_inv_inheritance_test.py -v
"""

from __future__ import annotations

import importlib.util
import sys
import textwrap
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent
SCRIPT_PATH = REPO_ROOT / "scripts" / "validate_inv_inheritance.py"


def _load_module():
    spec = importlib.util.spec_from_file_location(
        "validate_inv_inheritance", SCRIPT_PATH
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules["validate_inv_inheritance"] = module
    spec.loader.exec_module(module)
    return module


VAL = _load_module()


def _write_registry(tmp_path: Path, yaml_body: str, *, extra_rows: str = "") -> Path:
    """Build a minimal synthetic registry with a couple of INV rows + an
    inheritance index. The index is fenced exactly as the real registry
    fences it: ```yaml\n# inv-inheritance-index v1\n…\n```.
    """
    base_rows = textwrap.dedent(
        """
        # Test Registry

        ## 3. Registry

        ### 3.1 Test domain

        | ID | Severity |
        |---|---|
        | **INV-TEST-PARENT** | CRITICAL |
        | **INV-TEST-CHILD** | HIGH |
        | **INV-TEST-OTHER** | HIGH |
        """
    )
    body = base_rows + extra_rows + "\n```yaml\n# inv-inheritance-index v1\n" + yaml_body + "\n```\n"
    p = tmp_path / "invariant_registry.md"
    p.write_text(body, encoding="utf-8")
    return p


def _make_tla(tmp_path: Path, rel_path: str, body: str) -> Path:
    p = tmp_path / rel_path
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(body, encoding="utf-8")
    return p


# -----------------------------------------------------------------
# Happy path
# -----------------------------------------------------------------

def test_happy_path_inv_to_inv(tmp_path):
    """Pure INV→INV chain validates."""
    registry = _write_registry(
        tmp_path,
        textwrap.dedent(
            """
            chains:
              - inv: INV-TEST-CHILD
                inherits_from:
                  - INV-TEST-PARENT
            parents:
              - target: INV-TEST-PARENT
                inherited_by:
                  - INV-TEST-CHILD
            """
        ).strip(),
    )
    rc = VAL.main(
        ["--registry", str(registry), "--repo-root", str(tmp_path)]
    )
    assert rc == 0


def test_happy_path_inv_to_tla_property(tmp_path):
    """INV → TLA-file:Property chain validates when both exist."""
    _make_tla(
        tmp_path,
        "specs/tla/example.tla",
        "---- MODULE example ----\nInvSomething == TRUE\n====",
    )
    registry = _write_registry(
        tmp_path,
        textwrap.dedent(
            """
            chains:
              - inv: INV-TEST-CHILD
                inherits_from:
                  - specs/tla/example.tla:InvSomething
            parents:
              - target: specs/tla/example.tla:InvSomething
                inherited_by:
                  - INV-TEST-CHILD
            """
        ).strip(),
    )
    rc = VAL.main(
        ["--registry", str(registry), "--repo-root", str(tmp_path)]
    )
    assert rc == 0


# -----------------------------------------------------------------
# Broken-target failure modes
# -----------------------------------------------------------------

def test_missing_tla_file(tmp_path, capsys):
    """`inherits_from` cites a TLA file that does not exist."""
    registry = _write_registry(
        tmp_path,
        textwrap.dedent(
            """
            chains:
              - inv: INV-TEST-CHILD
                inherits_from:
                  - specs/tla/missing.tla
            parents:
              - target: specs/tla/missing.tla
                inherited_by:
                  - INV-TEST-CHILD
            """
        ).strip(),
    )
    rc = VAL.main(
        ["--registry", str(registry), "--repo-root", str(tmp_path)]
    )
    out = capsys.readouterr().out
    assert rc == 1
    assert "specs/tla/missing.tla" in out
    assert "does not exist" in out


def test_missing_property_in_tla(tmp_path, capsys):
    """`inherits_from` cites a property absent from an existing spec."""
    _make_tla(
        tmp_path,
        "specs/tla/present.tla",
        "---- MODULE present ----\nInvFoo == TRUE\n====",
    )
    registry = _write_registry(
        tmp_path,
        textwrap.dedent(
            """
            chains:
              - inv: INV-TEST-CHILD
                inherits_from:
                  - specs/tla/present.tla:InvAbsentProperty
            parents:
              - target: specs/tla/present.tla:InvAbsentProperty
                inherited_by:
                  - INV-TEST-CHILD
            """
        ).strip(),
    )
    rc = VAL.main(
        ["--registry", str(registry), "--repo-root", str(tmp_path)]
    )
    out = capsys.readouterr().out
    assert rc == 1
    assert "InvAbsentProperty" in out
    assert "not found" in out


def test_unknown_parent_inv(tmp_path, capsys):
    """`inherits_from: INV-NOPE` where INV-NOPE is not registered."""
    registry = _write_registry(
        tmp_path,
        textwrap.dedent(
            """
            chains:
              - inv: INV-TEST-CHILD
                inherits_from:
                  - INV-NOT-REGISTERED
            parents:
              - target: INV-NOT-REGISTERED
                inherited_by:
                  - INV-TEST-CHILD
            """
        ).strip(),
    )
    rc = VAL.main(
        ["--registry", str(registry), "--repo-root", str(tmp_path)]
    )
    out = capsys.readouterr().out
    assert rc == 1
    assert "INV-NOT-REGISTERED" in out


# -----------------------------------------------------------------
# Asymmetric-link failure modes
# -----------------------------------------------------------------

def test_asymmetric_child_missing_in_parent(tmp_path, capsys):
    """Child cites parent; parent's inherited_by omits the child."""
    registry = _write_registry(
        tmp_path,
        textwrap.dedent(
            """
            chains:
              - inv: INV-TEST-CHILD
                inherits_from:
                  - INV-TEST-PARENT
            parents:
              - target: INV-TEST-PARENT
                inherited_by:
                  - INV-TEST-OTHER
            """
        ).strip(),
    )
    rc = VAL.main(
        ["--registry", str(registry), "--repo-root", str(tmp_path)]
    )
    out = capsys.readouterr().out
    assert rc == 1
    assert "asymmetric link" in out
    assert "INV-TEST-CHILD" in out
    assert "INV-TEST-PARENT" in out


def test_asymmetric_parent_lists_unknown_child(tmp_path, capsys):
    """Parent's inherited_by lists a child with no chain entry."""
    registry = _write_registry(
        tmp_path,
        textwrap.dedent(
            """
            chains:
              - inv: INV-TEST-CHILD
                inherits_from:
                  - INV-TEST-PARENT
            parents:
              - target: INV-TEST-PARENT
                inherited_by:
                  - INV-TEST-CHILD
                  - INV-TEST-OTHER
            """
        ).strip(),
    )
    rc = VAL.main(
        ["--registry", str(registry), "--repo-root", str(tmp_path)]
    )
    out = capsys.readouterr().out
    assert rc == 1
    assert "INV-TEST-OTHER" in out
    assert "asymmetric link" in out


# -----------------------------------------------------------------
# Additivity contract
# -----------------------------------------------------------------

def test_absent_index_passes_vacuously(tmp_path):
    """A registry without an inheritance index block validates OK."""
    p = tmp_path / "invariant_registry.md"
    p.write_text(
        "# Test Registry\n\n## 3. Registry\n\n| ID |\n|---|\n| **INV-X** |\n",
        encoding="utf-8",
    )
    rc = VAL.main(["--registry", str(p), "--repo-root", str(tmp_path)])
    assert rc == 0


# -----------------------------------------------------------------
# Schema / form validation
# -----------------------------------------------------------------

def test_empty_inherits_from_rejected(tmp_path, capsys):
    registry = _write_registry(
        tmp_path,
        textwrap.dedent(
            """
            chains:
              - inv: INV-TEST-CHILD
                inherits_from: []
            parents: []
            """
        ).strip(),
    )
    rc = VAL.main(
        ["--registry", str(registry), "--repo-root", str(tmp_path)]
    )
    out = capsys.readouterr().out
    assert rc == 1
    assert "non-empty list" in out


def test_invalid_target_form(tmp_path, capsys):
    """A garbage target like 'foo bar' is rejected."""
    registry = _write_registry(
        tmp_path,
        textwrap.dedent(
            """
            chains:
              - inv: INV-TEST-CHILD
                inherits_from:
                  - not-a-valid-target
            parents:
              - target: not-a-valid-target
                inherited_by:
                  - INV-TEST-CHILD
            """
        ).strip(),
    )
    rc = VAL.main(
        ["--registry", str(registry), "--repo-root", str(tmp_path)]
    )
    out = capsys.readouterr().out
    assert rc == 1
    assert "not a valid form" in out


# -----------------------------------------------------------------
# Live-registry smoke test
# -----------------------------------------------------------------

def test_live_registry_validates_green():
    """The real production registry post-W26-P2-03 conversion must pass."""
    rc = VAL.main([])
    assert rc == 0


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-v"]))
