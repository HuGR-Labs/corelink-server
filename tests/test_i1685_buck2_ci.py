"""Contract and mutation checks for the focused Buck2 starter lane (#1685)."""
from __future__ import annotations

import re
from pathlib import Path

import pytest
import yaml


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW_PATH = ROOT / ".github/workflows/buck2-starter-ci.yml"
BUCKCONFIG_PATH = ROOT / "examples/buck2-starter/.buckconfig"
TOOLCHAINS_PATH = ROOT / "examples/buck2-starter/toolchains/BUCK"
METRO_PLATFORM_SHIM_PATH = ROOT / "examples/buck2-starter/tools/build_defs/js/constraints/metro_js_platform_override/BUCK"
ASSET_RESOLVER_SHIM_PATH = ROOT / "examples/buck2-starter/tools/build_defs/js/constraints/asset_dest_path_resolver/BUCK"
EXPECTED_VERSION = "2026-08-01"
EXPECTED_BUILD_VERSION = "2026-07-31"
EXPECTED_SHA256 = "aa304d471a79f69233b09767d4ba9add769049b7a37f78a3a71a72983372f511"
SUITE_COMMAND = "python -m pytest -q tests/test_i1685_buck2_ci.py"


def _workflow(text: str) -> dict:
    parsed = yaml.safe_load(text)
    assert isinstance(parsed, dict)
    return parsed


def _events(parsed: dict) -> dict:
    # PyYAML 1.1 parses the YAML key `on` as boolean True.
    events = parsed.get("on", parsed.get(True, {}))
    assert isinstance(events, dict)
    return events


def _run(step: dict) -> str:
    value = step.get("run", "")
    assert isinstance(value, str)
    return value


def _step(job: dict, name: str) -> dict:
    steps = job.get("steps")
    assert isinstance(steps, list)
    matches = [item for item in steps if isinstance(item, dict) and item.get("name") == name]
    assert len(matches) == 1, f"expected one {name!r} step"
    return matches[0]


def assert_contract(text: str) -> None:
    parsed = _workflow(text)
    events = _events(parsed)
    assert set(events) == {"workflow_dispatch"}
    assert parsed.get("permissions") == {"contents": "read"}

    jobs = parsed.get("jobs")
    assert isinstance(jobs, dict)
    assert set(jobs) == {"build", "negative-scenarios", "benchmark"}
    for name, expected_timeout in (("build", 20), ("negative-scenarios", 10), ("benchmark", 30)):
        job = jobs[name]
        assert job.get("runs-on") == "ubuntu-latest"
        assert job.get("timeout-minutes") == expected_timeout

    env = parsed.get("env")
    assert isinstance(env, dict)
    assert env.get("BUCK2_VERSION") == EXPECTED_VERSION
    assert env.get("BUCK2_BUILD_VERSION") == EXPECTED_BUILD_VERSION
    assert env.get("BUCK2_SHA256") == EXPECTED_SHA256
    assert re.fullmatch(r"[0-9a-f]{64}", env["BUCK2_SHA256"])
    assert env.get("BUCK2_INSTALL_DIR") == "${{ github.workspace }}/.buck2-bin"

    build = jobs["build"]
    install = _run(_step(build, "Install Buck2 latest stable"))
    assert "set -euo pipefail" in install
    assert "ARCH=\"x86_64-unknown-linux-gnu\"" in install
    assert "releases/download/${BUCK2_VERSION}/buck2-${ARCH}.zst" in install
    assert "--proto '=https' --tlsv1.2" in install
    assert "--connect-timeout 10 --max-time 120" in install
    assert "sha256sum" in install or "openssl dgst -sha256" in install
    assert '"${ACTUAL_SHA256}" != "${BUCK2_SHA256}"' in install
    assert "sudo apt-get install -y --no-install-recommends zstd jq" in install
    assert "zstd -d \"${ARCHIVE}\"" in install
    assert 'VERSION_OUTPUT="$(${BUCK2_INSTALL_DIR}/buck2 --version)"' not in install
    assert 'VERSION_OUTPUT="$("${BUCK2_INSTALL_DIR}/buck2" --version)"' in install
    assert 'grep -F -- "${BUCK2_BUILD_VERSION}" <<<"${VERSION_OUTPUT}"' in install
    assert "|| true" not in install

    cold = _run(_step(build, "Cold build — buck2 build :hello (populate remote cache)"))
    warm = _run(_step(build, "Warm build — assert ≥ 80 % remote cache hits"))
    smoke = _run(_step(build, "Smoke-run binary"))
    assert "buck2 build :hello \\" in cold
    assert "test -s /tmp/cold-report.json" in cold
    assert ".cache_hits" in warm and ".total_actions" in warm and "RATIO < 80" in warm
    assert "buck2 run :hello -- CoreLink" in smoke
    assert "Hello, CoreLink" in smoke

    for job_name, install_name in (
        ("negative-scenarios", "Install Buck2"),
        ("benchmark", "Install Buck2"),
    ):
        job_install = _run(_step(jobs[job_name], install_name))
        assert "set -euo pipefail" in job_install
        assert "${BUCK2_VERSION}" in job_install
        assert '"${ACTUAL_SHA256}" == "${BUCK2_SHA256}"' in job_install
        assert "--max-time 120" in job_install
        assert "|| true" not in job_install

    benchmark = jobs["benchmark"]
    benchmark_text = "\n".join(_run(step) for step in benchmark["steps"] if isinstance(step, dict))
    assert not re.search(r"(?m)^\s*git\s+(?:add|commit|push)\b", benchmark_text)
    assert any(
        isinstance(step, dict)
        and step.get("uses") == "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a"
        for step in benchmark["steps"]
    )

    uses = re.findall(r"^\s*uses:\s*([^\s#]+)", text, re.MULTILINE)
    assert uses
    assert all(re.search(r"@[0-9a-f]{40}$", action) for action in uses)


def test_buck2_contract_is_complete() -> None:
    assert_contract(WORKFLOW_PATH.read_text(encoding="utf-8"))


def test_starter_declares_root_cell_and_bundled_execution_platform() -> None:
    config = BUCKCONFIG_PATH.read_text(encoding="utf-8")
    toolchains = TOOLCHAINS_PATH.read_text(encoding="utf-8")
    metro_platform_shim = METRO_PLATFORM_SHIM_PATH.read_text(encoding="utf-8")
    asset_resolver_shim = ASSET_RESOLVER_SHIM_PATH.read_text(encoding="utf-8")
    assert "[cells]\n    root = .\n    prelude = prelude\n    toolchains = toolchains\n" in config
    assert "[cell_aliases]\n    config = prelude\n    fbsource = root\n" in config
    assert "[external_cells]\n    prelude = bundled\n" in config
    assert config.count("execution_platforms = prelude//platforms:default") == 2
    assert "execution_platforms = //:platforms" not in config
    assert 'load("@prelude//toolchains:cxx.bzl", "system_cxx_toolchain")' in toolchains
    assert 'name = "cxx"' in toolchains
    assert 'visibility = ["PUBLIC"]' in toolchains
    assert 'name = "metro_js_platform_override"' in metro_platform_shim
    for value in ("android", "ios", "macos", "vr", "windows"):
        assert (
            f'    name = "{value}",\n'
            '    constraint_setting = ":metro_js_platform_override",\n'
        ) in metro_platform_shim
    assert 'name = "asset_dest_path_resolver"' in asset_resolver_shim
    for value in ("android", "generic"):
        assert (
            f'    name = "{value}",\n'
            '    constraint_setting = ":asset_dest_path_resolver",\n'
        ) in asset_resolver_shim


@pytest.mark.parametrize(
    ("mutation", "expected"),
    (
        (lambda text: text.replace("    prelude = prelude\n", "", 1), "prelude cell"),
        (lambda text: text.replace("    config = prelude\n", "", 1), "config cell alias"),
        (lambda text: text.replace("    config = prelude\n", "    config = root\n", 1), "config cell alias target"),
        (lambda text: text.replace("    fbsource = root\n", "", 1), "fbsource cell alias"),
        (lambda text: text.replace("    fbsource = root\n", "    fbsource = prelude\n", 1), "fbsource cell alias target"),
        (lambda text: text.replace("    prelude = bundled\n", "", 1), "bundled prelude origin"),
    ),
)
def test_starter_config_rejects_each_prelude_mapping_mutation(mutation, expected: str) -> None:
    config = BUCKCONFIG_PATH.read_text(encoding="utf-8")
    with pytest.raises(AssertionError):
        mutated = mutation(config)
        assert "[cells]\n    root = .\n    prelude = prelude\n    toolchains = toolchains\n" in mutated
        assert "[cell_aliases]\n    config = prelude\n    fbsource = root\n" in mutated
        assert "[external_cells]\n    prelude = bundled\n" in mutated


@pytest.mark.parametrize(
    ("mutation", "expected"),
    (
        (lambda text: text.replace(f'BUCK2_VERSION: "{EXPECTED_VERSION}"', 'BUCK2_VERSION: "2026-09-01"'), "version"),
        (lambda text: text.replace(f'BUCK2_BUILD_VERSION: "{EXPECTED_BUILD_VERSION}"', 'BUCK2_BUILD_VERSION: "2026-09-01"'), "build version"),
        (lambda text: text.replace(EXPECTED_SHA256, "0" * 64), "checksum"),
        (lambda text: text.replace("runs-on: ubuntu-latest", "runs-on: corelink"), "runner"),
        (lambda text: text.replace("timeout-minutes: 20", "timeout-minutes: 0"), "timeout"),
        (lambda text: text.replace(".cache_hits", "cache_hits_removed"), "cache"),
        (lambda text: text.replace('grep -F -- "${BUCK2_BUILD_VERSION}" <<<"${VERSION_OUTPUT}"', "# version check removed"), "version smoke"),
        (lambda text: text.replace("sudo apt-get install -y --no-install-recommends zstd jq", "sudo apt-get install -y --no-install-recommends zstd jq || true", 1), "install failure"),
    ),
)


def test_contract_rejects_each_mutation(mutation, expected: str) -> None:
    with pytest.raises(AssertionError):
        assert_contract(mutation(WORKFLOW_PATH.read_text(encoding="utf-8")))


@pytest.mark.parametrize(
    "mutation",
    (
        lambda text: text.replace("    toolchains = toolchains\n", "", 1),
        lambda text: text.replace("    toolchains = toolchains\n", "    toolchains = root\n", 1),
    ),
)
def test_starter_config_rejects_each_toolchains_mapping_mutation(mutation) -> None:
    config = BUCKCONFIG_PATH.read_text(encoding="utf-8")
    with pytest.raises(AssertionError):
        mutated = mutation(config)
        assert "    toolchains = toolchains\n" in mutated


@pytest.mark.parametrize(
    "mutation",
    (
        lambda text: text.replace(
            'load("@prelude//toolchains:cxx.bzl", "system_cxx_toolchain")\n',
            "",
            1,
        ),
        lambda text: text.replace('name = "cxx"', 'name = "wrong"', 1),
    ),
)
def test_starter_toolchain_rejects_each_cxx_structure_mutation(mutation) -> None:
    toolchains = TOOLCHAINS_PATH.read_text(encoding="utf-8")
    with pytest.raises(AssertionError):
        mutated = mutation(toolchains)
        assert 'load("@prelude//toolchains:cxx.bzl", "system_cxx_toolchain")' in mutated
        assert 'name = "cxx"' in mutated


@pytest.mark.parametrize(
    ("path", "mutation", "expected"),
    (
        (
            METRO_PLATFORM_SHIM_PATH,
            lambda text: text.replace('name = "android"', "", 1),
            '    name = "android",\n    constraint_setting = ":metro_js_platform_override",',
        ),
        (
            METRO_PLATFORM_SHIM_PATH,
            lambda text: text.replace('name = "android"', 'name = "wrong"', 1),
            '    name = "android",\n    constraint_setting = ":metro_js_platform_override",',
        ),
        (
            ASSET_RESOLVER_SHIM_PATH,
            lambda text: text.replace('name = "generic"', "", 1),
            '    name = "generic",\n    constraint_setting = ":asset_dest_path_resolver",',
        ),
        (
            ASSET_RESOLVER_SHIM_PATH,
            lambda text: text.replace('name = "generic"', 'name = "wrong"', 1),
            '    name = "generic",\n    constraint_setting = ":asset_dest_path_resolver",',
        ),
    ),
)
def test_starter_fbsource_shims_reject_each_target_mutation(path, mutation, expected) -> None:
    text = path.read_text(encoding="utf-8")
    with pytest.raises(AssertionError):
        mutated = mutation(text)
        assert expected in mutated
