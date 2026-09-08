from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b270_b281_bundle_repairs as verify


def _text(path: str) -> str:
    return (verify.ROOT / path).read_text(encoding="utf-8")


def _must_fail(path: str, mutated: str) -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={path: mutated})


def test_current_contract_passes() -> None:
    verify.verify()


@pytest.mark.parametrize(
    ("path", "marker"),
    [(path, marker) for path, required, _ in verify.CONTRACTS for marker in required],
)
def test_required_marker_mutations_fail(path: str, marker: str) -> None:
    source = _text(path)
    assert marker in source
    _must_fail(path, source.replace(marker, ""))


@pytest.mark.parametrize(
    ("path", "marker"),
    [(path, marker) for path, _, forbidden in verify.CONTRACTS for marker in forbidden],
)
def test_forbidden_marker_mutations_fail(path: str, marker: str) -> None:
    _must_fail(path, f"{marker}\n{_text(path)}")


@pytest.mark.parametrize(("parent", "module", "sibling"), verify.MODULE_PATHS)
def test_module_path_mutations_fail(parent: str, module: str, sibling: str) -> None:
    source = _text(parent)
    binding = f'#[path = "{sibling}"]\nmod {module};'
    assert binding in source
    _must_fail(parent, source.replace(binding, f"mod {module};", 1))


def test_rand_cannot_move_back_to_dev_dependencies() -> None:
    path = "crates/corelink-container/Cargo.toml"
    source = _text(path)
    line = 'rand = "0.8"'
    without = source.replace(f"{line}\n", "", 1)
    mutated = without.replace("[dev-dependencies]", f"[dev-dependencies]\n{line}", 1)
    _must_fail(path, mutated)


@pytest.mark.parametrize(
    ("parent", "child"),
    [(parent, child) for parent, children in verify.INCLUDE_CENSUSES for child in children],
)
def test_each_parent_include_is_load_bearing(parent: str, child: str) -> None:
    source = _text(parent)
    marker = f'include!("{child}");'
    assert marker in source
    _must_fail(parent, source.replace(marker, f'/* {marker} */', 1))


@pytest.mark.parametrize(
    ("parent", "child"),
    [(parent, child) for parent, children in verify.INCLUDE_CENSUSES for child in children],
)
def test_raw_include_decoys_do_not_rewire_a_parent(parent: str, child: str) -> None:
    source = _text(parent)
    marker = f'include!("{child}");'
    assert marker in source
    mutated = source.replace(marker, "")
    mutated = f'r###"{marker}"###\n' + mutated
    _must_fail(parent, mutated)


@pytest.mark.parametrize(
    ("parent", "child"),
    [(parent, child) for parent, children in verify.INCLUDE_CENSUSES for child in children],
)
def test_each_declared_child_must_be_read(
    parent: str, child: str, monkeypatch: pytest.MonkeyPatch
) -> None:
    child_path = str(Path(parent).parent / child)
    original = verify._read

    def read(root: Path, path: str, overrides: dict[str, str]) -> str:
        if path == child_path:
            raise verify.VerificationError(f"missing input: {path}")
        return original(root, path, overrides)

    monkeypatch.setattr(verify, "_read", read)
    with pytest.raises(verify.VerificationError):
        verify.verify()


@pytest.mark.parametrize(
    ("path", "marker"),
    [(path, marker) for path, required, _ in verify.CONTRACTS for marker in required],
)
def test_raw_string_decoys_do_not_satisfy_required_markers(path: str, marker: str) -> None:
    source = _text(path)
    mutated = source.replace(marker, "")
    mutated = f'r###"{marker}"###\n' + mutated
    _must_fail(path, mutated)
