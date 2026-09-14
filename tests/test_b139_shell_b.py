"""Regression contracts for B-139 shell-injection fixes."""

from pathlib import Path
import subprocess


ROOT = Path(__file__).parents[1]


def _workflow(name: str) -> str:
    return (ROOT / ".github" / "workflows" / name).read_text(encoding="utf-8")


def test_dispatch_values_are_env_bound_before_shell_use() -> None:
    cases = {
        "dr-drill-monthly.yml": ("CYCLE_INPUT", "REGION_INPUT", "SNAPSHOT_INPUT", "PROVIDER_INPUT"),
        "e2e-browser-prod.yml": ("SPEC_INPUT",),
        "neon-shadow-reconcile-daily.yml": ("TARGET_DATE_INPUT",),
        "release-notes.yml": ("FROM_REF_INPUT", "TO_REF_INPUT", "VERSION_INPUT"),
        "sbom.yml": ("VERSION_INPUT",),
    }
    for name, env_names in cases.items():
        source = _workflow(name)
        assert "github.event.inputs." not in source
        for env_name in env_names:
            assert f'"${env_name}"' in source


def test_fail_closed_validators_cover_each_input_boundary() -> None:
    dr = _workflow("dr-drill-monthly.yml")
    assert "cycle must be 1, 2, or 3" in dr
    assert "weur|sam|asia" in dr
    assert "staging synthetic alias" in dr
    assert "aws|gcp|azure|vault" in dr
    assert "[0-9]{4}-[0-9]{2}-[0-9]{2}" in dr
    e2e = _workflow("e2e-browser-prod.yml")
    assert "direct Playwright spec path" in e2e
    release = _workflow("release-notes.yml")
    assert "git check-ref-format" in release
    assert "invalid version" in release
    sbom = _workflow("sbom.yml")
    assert "invalid release version" in sbom
    assert 'VERSION="${RELEASE_TAG#v}"' in sbom


def test_release_tag_v_prefix_is_normalized_for_sbom() -> None:
    source = _workflow("sbom.yml")
    assert "VERSION=\"${RELEASE_TAG#v}\"" in source
    assert "^drift-check-[0-9]{8}$" in source


def test_invalid_dispatch_cycle_is_rejected() -> None:
    result = subprocess.run(
        ["bash", "-c", '[[ "$1" =~ ^[123]$ ]]', "cycle-check", "4;id"],
        check=False,
    )
    assert result.returncode != 0


def test_snapshot_rejects_invalid_calendar_dates() -> None:
    source = _workflow("dr-drill-monthly.yml")
    assert "datetime.date.fromisoformat(sys.argv[1])" in source
    validator = "import datetime, sys; datetime.date.fromisoformat(sys.argv[1])"
    for value in ("2026-99-99", "2026-02-29"):
        result = subprocess.run(["python3", "-c", validator, value], check=False)
        assert result.returncode != 0


def test_release_args_keep_shell_metacharacters_as_data() -> None:
    payload = "a$(id)"
    assert subprocess.run(
        ["git", "check-ref-format", "--allow-onelevel", f"refs/{payload}"],
        check=False,
    ).returncode == 0
    script = 'VALUE="$1"; args=(--from "$VALUE"); printf "%s\\n" "${args[@]}"'
    result = subprocess.run(["bash", "-c", script, "shell", payload], check=True, text=True, capture_output=True)
    assert result.stdout.splitlines()[-1] == payload


def test_release_generate_step_has_no_direct_output_interpolation() -> None:
    source = _workflow("release-notes.yml")
    generate = source.split("          set -euo pipefail\n", 2)[-1].split("      - name: Upload notes", 1)[0]
    assert "${{ steps.refs.outputs." not in generate
    assert 'args=(--from "$FROM_REF" --to "$TO_REF")' in generate
