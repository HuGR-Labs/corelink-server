from __future__ import annotations

import importlib.util
import json
import subprocess
from pathlib import Path

import pytest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/run_b251_latency_probe.py"
SPEC = importlib.util.spec_from_file_location("run_b251_latency_probe", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def identity(source: str, **overrides: str) -> dict[str, str]:
    payload = {"source": source, "seed": "seed-a", "failure": "failure-a", "blob": "blob-a"}
    payload.update(overrides)
    return payload


def write(path: Path, payload: dict[str, str]) -> None:
    path.write_text(json.dumps(payload) + "\n", encoding="utf-8")


def marker(p99_us: int = 17, **overrides: object) -> str:
    payload: dict[str, object] = {
        "sample_count": 1000,
        "p99_us": p99_us,
        "limit_us": 5000,
        "fixture": "InMemoryAtomicQuotaChecker",
        "production_latency_measured": False,
    }
    payload.update(overrides)
    return MODULE.PROBE_MARKER + json.dumps(payload)


def test_parse_probe_output_accepts_strictly_under_limit() -> None:
    assert MODULE.parse_probe_output(marker())["p99_us"] == 17


@pytest.mark.parametrize(
    "output,match",
    [
        ("", "exactly one"),
        (marker() + "\n" + marker(), "exactly one"),
        (marker(5000), "not below 5ms"),
        (marker(sample_count=999), "exactly 1,000"),
        (marker(production_latency_measured=True), "falsely claimed production"),
    ],
)
def test_parse_probe_output_fails_closed(output: str, match: str) -> None:
    with pytest.raises(MODULE.ProbeError, match=match):
        MODULE.parse_probe_output(output)


def test_identity_mismatch_names_field_without_values() -> None:
    with pytest.raises(MODULE.ProbeError, match="D02 identity mismatch: blob") as raised:
        MODULE.compare_identity(identity("D02"), identity("D03-observed", blob="different"))
    assert "blob-a" not in str(raised.value)
    assert "different" not in str(raised.value)


def test_run_requires_identity_before_spawning_cargo(tmp_path: Path) -> None:
    baseline = tmp_path / "d02.json"
    observed = tmp_path / "observed.json"
    output = tmp_path / "evidence.json"
    write(baseline, identity("D02"))
    write(observed, identity("D03-observed", seed="wrong"))
    output.write_text("stale evidence\n", encoding="utf-8")
    called = False

    def runner(*args: object, **kwargs: object) -> subprocess.CompletedProcess[str]:
        nonlocal called
        called = True
        return subprocess.CompletedProcess([], 0, marker(), "")

    with pytest.raises(MODULE.ProbeError, match="D02 identity mismatch: seed"):
        MODULE.run_probe(baseline, observed, output, runner)
    assert called is False
    assert not output.exists()


def test_failed_cargo_does_not_write_evidence(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    baseline = tmp_path / "d02.json"
    observed = tmp_path / "observed.json"
    output = tmp_path / "evidence.json"
    write(baseline, identity("D02"))
    write(observed, identity("D03-observed"))
    output.write_text("stale evidence\n", encoding="utf-8")
    values = iter(("a" * 40, "b" * 40, "b" * 40))
    monkeypatch.setattr(MODULE, "_git_value", lambda *args: next(values))

    def runner(*args: object, **kwargs: object) -> subprocess.CompletedProcess[str]:
        return subprocess.CompletedProcess([], 101, "", "compile failed")

    with pytest.raises(MODULE.ProbeError, match="exit 101"):
        MODULE.run_probe(baseline, observed, output, runner)
    assert not output.exists()


def test_dirty_probe_source_prevents_cargo(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    baseline = tmp_path / "d02.json"
    observed = tmp_path / "observed.json"
    output = tmp_path / "evidence.json"
    write(baseline, identity("D02"))
    write(observed, identity("D03-observed"))
    values = iter(("a" * 40, "committed-blob", "working-blob"))
    monkeypatch.setattr(MODULE, "_git_value", lambda *args: next(values))
    called = False

    def runner(*args: object, **kwargs: object) -> subprocess.CompletedProcess[str]:
        nonlocal called
        called = True
        return subprocess.CompletedProcess([], 0, marker(), "")

    with pytest.raises(MODULE.ProbeError, match="source differs"):
        MODULE.run_probe(baseline, observed, output, runner)
    assert called is False
    assert not output.exists()


def test_success_writes_bound_evidence_atomically(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    baseline = tmp_path / "d02.json"
    observed = tmp_path / "observed.json"
    output = tmp_path / "evidence.json"
    write(baseline, identity("D02"))
    write(observed, identity("D03-observed"))

    def runner(*args: object, **kwargs: object) -> subprocess.CompletedProcess[str]:
        return subprocess.CompletedProcess([], 0, "", marker(23))

    values = iter(("a" * 40, "b" * 40, "b" * 40))
    monkeypatch.setattr(MODULE, "_git_value", lambda *args: next(values))
    evidence = MODULE.run_probe(baseline, observed, output, runner)
    persisted = json.loads(output.read_text(encoding="utf-8"))
    assert persisted == evidence
    assert persisted["revision"] == "a" * 40
    assert persisted["test_blob"] == "b" * 40
    assert persisted["identity"] == {
        "baseline_source": "D02",
        "observed_source": "D03-observed",
        "fields_compared": ["seed", "failure", "blob"],
        "match": True,
    }
    assert persisted["measurement"]["p99_us"] == 23
    assert persisted["measurement"]["production_latency_measured"] is False
