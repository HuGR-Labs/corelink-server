"""Fail-closed tests for path-scoped synthetic raw-curl environment data."""
from __future__ import annotations

import importlib.util
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/validate_secrets_matrix.py"
spec = importlib.util.spec_from_file_location("validate_secrets_matrix", SCRIPT)
assert spec and spec.loader
gate = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = gate
spec.loader.exec_module(gate)


SYNTHETIC_NAMES = {
    "CORELINK_HTTP_PORT_FILE",
    "CORELINK_HTTP_REQUEST_FILE",
    "CORELINK_HTTP_STATUS",
}

GC_SAFETY_FLAGS = {
    "GC_LIVE_DELETE",
    "GC_OBSERVATION_ONLY",
}

GC_UNCLASSIFIED_NAMES = {
    "GC_LIVE_DELETE_CONFIRM",
    "GC_R2_BUCKET",
    "GC_RUN_ID",
    "GC_VALIDATE_ONLY",
    "GC_ADMIN_TOKEN",
    "GC_OBSERVATION_ONLY_TOKEN",
}

NON_SECRET_CONFIG_NAMES = {
    "D1_DATABASE_ID",
    "R2_S3_ENDPOINT",
}

def _source(names: set[str]) -> str:
    return "\n".join(f"const value = process.env.{name};" for name in sorted(names))


def test_only_exact_raw_curl_paths_skip_the_three_synthetic_names(tmp_path: Path) -> None:
    for relative in gate.SYNTHETIC_ENV_MANIFEST:
        path = tmp_path / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(_source(SYNTHETIC_NAMES), encoding="utf-8")

    assert gate.scan_ts(tmp_path) == set()


def test_moving_the_fixture_fails_closed(tmp_path: Path) -> None:
    path = tmp_path / "apps/docs/tests/fixtures/raw-curl-http-server-renamed.mjs"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(_source(SYNTHETIC_NAMES), encoding="utf-8")

    assert gate.scan_ts(tmp_path) == SYNTHETIC_NAMES


def test_a_fourth_name_in_an_exact_fixture_path_fails_closed(tmp_path: Path) -> None:
    path = tmp_path / "apps/docs/tests/fixtures/raw-curl-http-server.mjs"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(_source(SYNTHETIC_NAMES | {"CORELINK_HTTP_SECRET_FILE"}), encoding="utf-8")

    assert gate.scan_ts(tmp_path) == {"CORELINK_HTTP_SECRET_FILE"}


def test_synthetic_names_in_production_paths_fail_closed(tmp_path: Path) -> None:
    path = tmp_path / "worker/src/secrets.ts"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(_source(SYNTHETIC_NAMES), encoding="utf-8")

    assert gate.scan_ts(tmp_path) == SYNTHETIC_NAMES

    rust_path = tmp_path / "crates/corelink-container/src/secrets.rs"
    rust_path.parent.mkdir(parents=True, exist_ok=True)
    rust_path.write_text(
        "\n".join(f'std::env::var("{name}");' for name in sorted(SYNTHETIC_NAMES)),
        encoding="utf-8",
    )

    assert gate.scan_rust(tmp_path) == SYNTHETIC_NAMES


def test_gc_safety_flags_are_exact_non_secret_allowlist_entries() -> None:
    assert all(gate.ALLOWLIST_REGEX.match(name) for name in GC_SAFETY_FLAGS)
    assert all(not gate.ALLOWLIST_REGEX.match(name) for name in GC_UNCLASSIFIED_NAMES)

    shell_gate = (ROOT / "scripts/secrets-checklist-verify.sh").read_text(
        encoding="utf-8"
    )
    regex = re.search(r"^ALLOWLIST_REGEX='([^']+)'$", shell_gate, re.MULTILINE)
    assert regex, "Bash allowlist must have one canonical assignment"
    for name in GC_SAFETY_FLAGS:
        assert subprocess.run(
            ["grep", "-E", regex.group(1)],
            input=f"{name}\n",
            text=True,
            capture_output=True,
            check=False,
        ).returncode == 0
    for name in GC_UNCLASSIFIED_NAMES:
        assert subprocess.run(
            ["grep", "-E", regex.group(1)],
            input=f"{name}\n",
            text=True,
            capture_output=True,
            check=False,
        ).returncode != 0


def test_bash_repo_root_override_requires_canonical_sentinels(tmp_path: Path) -> None:
    """A matrix-shaped arbitrary directory cannot become a trusted root."""
    matrix = tmp_path / "docs/internal/secrets-checklist.md"
    matrix.parent.mkdir(parents=True)
    matrix.write_text(
        "\n".join(
            f"| {i} | Fixture {i} | `FIXTURE_{i}` |" for i in range(1, 21)
        )
        + "\n",
        encoding="utf-8",
    )
    result = subprocess.run(
        ["bash", str(ROOT / "scripts/secrets-checklist-verify.sh"), "--repo-root", str(tmp_path)],
        text=True,
        capture_output=True,
        check=False,
    )
    assert result.returncode == 2
    assert "canonical sentinels" in result.stderr


def test_bash_repo_root_override_accepts_isolated_canonical_fixture(tmp_path: Path) -> None:
    """The verifier remains testable against an isolated, complete fixture root."""
    for relative in (
        "Cargo.toml",
        "scripts/validate_secrets_matrix.py",
    ):
        path = tmp_path / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("", encoding="utf-8")
    (tmp_path / ".github/workflows").mkdir(parents=True)
    matrix = tmp_path / "docs/internal/secrets-checklist.md"
    matrix.parent.mkdir(parents=True, exist_ok=True)
    matrix.write_text(
        "\n".join(
            f"| {i} | Fixture {i} | `FIXTURE_{i}` |" for i in range(1, 21)
        )
        + "\n",
        encoding="utf-8",
    )
    source = tmp_path / "crates/fixture/src/lib.rs"
    source.parent.mkdir(parents=True)
    source.write_text(
        'std::env::var("GC_LIVE_DELETE");\n'
        'std::env::var("GC_OBSERVATION_ONLY");\n',
        encoding="utf-8",
    )
    result = subprocess.run(
        ["bash", str(ROOT / "scripts/secrets-checklist-verify.sh"), "--repo-root", str(tmp_path)],
        text=True,
        capture_output=True,
        check=False,
    )
    assert result.returncode == 0, result.stderr


def test_non_secret_config_names_are_allowlisted_by_both_validators() -> None:
    """Keep the Bash deploy gate aligned with Python's narrow classifications."""
    assert all(gate.ALLOWLIST_REGEX.match(name) for name in NON_SECRET_CONFIG_NAMES)

    shell_gate = (ROOT / "scripts/secrets-checklist-verify.sh").read_text(encoding="utf-8")
    assert all(f"|{name}$" in shell_gate for name in NON_SECRET_CONFIG_NAMES)

    assert not gate.ALLOWLIST_REGEX.match("D1_DATABASE_TOKEN")
    assert not gate.ALLOWLIST_REGEX.match("R2_S3_SECRET_ACCESS_KEY")
    assert not gate.ALLOWLIST_REGEX.match("CORELINK_HTTP_SECRET_FILE")


def test_synthetic_names_are_not_global_allowlist_entries() -> None:
    """Synthetic raw-curl names stay visible outside their exact fixture paths."""
    shell_gate = (ROOT / "scripts/secrets-checklist-verify.sh").read_text(encoding="utf-8")
    for name in SYNTHETIC_NAMES:
        assert not gate.ALLOWLIST_REGEX.match(name)
        assert f"|{name}$" not in shell_gate
