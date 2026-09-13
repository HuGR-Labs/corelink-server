"""Regression contracts for B-139 shell-injection fixes."""

from pathlib import Path


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
    assert "weur|sam|asia" in dr
    assert "aws|gcp|azure|vault" in dr
    assert "[0-9]{4}-[0-9]{2}-[0-9]{2}" in dr
    e2e = _workflow("e2e-browser-prod.yml")
    assert "direct Playwright spec path" in e2e
    release = _workflow("release-notes.yml")
    assert "git check-ref-format" in release
    assert "invalid version" in release
    sbom = _workflow("sbom.yml")
    assert "invalid release version" in sbom
