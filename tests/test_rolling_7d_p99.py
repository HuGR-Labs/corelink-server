"""Fail-closed tests for the rolling endurance p99 artifact gate."""
from __future__ import annotations

import importlib.util
import json
import subprocess
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "rolling_7d_p99", ROOT / "scripts" / "rolling-7d-p99.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def history_file(tmp_path: Path, count: int = 7) -> Path:
    rows = [
        {
            "databaseId": str(1000 + index),
            "createdAt": f"2026-09-{index + 1:02d}T03:00:00Z",
        }
        for index in range(count)
    ]
    path = tmp_path / "runs.json"
    path.write_text(json.dumps(rows), encoding="utf-8")
    return path


def summary(path: Path, value: object = 10) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    metrics = {
        f"endurance_op_latency_ms{{op:{op}}}": {"p(99)": value}
        for op in MODULE.EXPECTED_OPS
    }
    path.write_text(json.dumps({"metrics": metrics}), encoding="utf-8")


def current_summary(path: Path, value: object = 11) -> None:
    summary(path / "endurance-2h-summary-current.json", value)


def fake_gh(
    monkeypatch: pytest.MonkeyPatch,
    bad_run: str | None = None,
    failed_job_run: str | None = None,
) -> None:
    def run(command: list[str], **_: object) -> subprocess.CompletedProcess[str]:
        if command[1:2] == ["api"]:
            run_id = command[2].rsplit("/", 2)[1]
            if command[2].endswith("/jobs?per_page=100"):
                run_id = command[2].split("/runs/", 1)[1].split("/", 1)[0]
                body = {"jobs": [{
                    "name": MODULE.MEASUREMENT_JOB_NAME,
                    "status": "completed",
                    "conclusion": "failure" if run_id == failed_job_run else "success",
                }]}
                return subprocess.CompletedProcess(command, 0, json.dumps(body), "")
            body = {"artifacts": [{"name": f"endurance-2h-results-{run_id}", "expired": False, "size_in_bytes": 1}]}
            return subprocess.CompletedProcess(command, 0, json.dumps(body), "")
        if command[1:3] == ["run", "download"]:
            run_id = command[3]
            if run_id == bad_run:
                raise subprocess.TimeoutExpired(command, MODULE.COMMAND_TIMEOUT_SECONDS)
            target = Path(command[command.index("--dir") + 1])
            summary(target / f"endurance-2h-summary-{run_id}.json")
            return subprocess.CompletedProcess(command, 0, "", "")
        raise AssertionError(command)

    monkeypatch.setattr(MODULE.subprocess, "run", run)


def invoke(
    tmp_path: Path,
    history: Path,
    downloads: Path,
    output: Path,
    current_value: object = 11,
) -> int:
    current = tmp_path / "current"
    current_summary(current, current_value)
    return MODULE.main(
        [
            "--history",
            str(history),
            "--current-dir",
            str(current),
            "--downloads-dir",
            str(downloads),
            "--output",
            str(output),
            "--repo",
            "example/corelink",
            "--current-run-id",
            "9999",
        ]
    )


def test_complete_history_downloads_and_writes_deterministic_baseline(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    fake_gh(monkeypatch)
    output = tmp_path / "rolling-7d-p99.json"
    assert invoke(tmp_path, history_file(tmp_path), tmp_path / "downloads", output) == 0
    first = output.read_text(encoding="utf-8")
    assert json.loads(first)["window_runs"] == 7
    assert json.loads(first)["p99_ms"]["cas_read"] == 10.0

    # Same inputs, including the sorted history, produce byte-identical output.
    assert invoke(tmp_path, history_file(tmp_path), tmp_path / "downloads-2", output) == 0
    assert output.read_text(encoding="utf-8") == first


def test_missing_history_fails_closed_before_download(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    called = False

    def unexpected(_: list[str], **__: object) -> None:
        nonlocal called
        called = True
        raise AssertionError("download must not start with incomplete history")

    monkeypatch.setattr(MODULE.subprocess, "run", unexpected)
    output = tmp_path / "rolling.json"
    assert invoke(tmp_path, history_file(tmp_path, count=6), tmp_path / "downloads", output) == 1
    assert not called
    assert not output.exists()


def test_download_timeout_fails_closed(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    fake_gh(monkeypatch, bad_run="1003")
    output = tmp_path / "rolling.json"
    assert invoke(tmp_path, history_file(tmp_path), tmp_path / "downloads", output) == 1
    assert not output.exists()


def test_failed_measurement_job_is_never_admitted_to_baseline(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    fake_gh(monkeypatch, failed_job_run="1003")
    output = tmp_path / "rolling.json"
    assert invoke(tmp_path, history_file(tmp_path), tmp_path / "downloads", output) == 1
    assert not output.exists()


def test_missing_artifact_fails_closed(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    def missing(_: str, __: str) -> str:
        raise MODULE.InputError("no usable artifact")

    monkeypatch.setattr(MODULE, "find_artifact", missing)
    output = tmp_path / "rolling.json"
    assert invoke(tmp_path, history_file(tmp_path), tmp_path / "downloads", output) == 1
    assert not output.exists()


def test_malformed_downloaded_summary_fails_closed(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    fake_gh(monkeypatch)
    original = MODULE.read_summary

    def malformed(path: Path, label: str) -> dict[str, float]:
        if label == "run 1004":
            raise MODULE.InputError("malformed summary")
        return original(path, label)

    monkeypatch.setattr(MODULE, "read_summary", malformed)
    output = tmp_path / "rolling.json"
    assert invoke(tmp_path, history_file(tmp_path), tmp_path / "downloads", output) == 1
    assert not output.exists()


def test_current_regression_fails_after_generating_evidence(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    fake_gh(monkeypatch)
    output = tmp_path / "rolling.json"
    assert invoke(
        tmp_path, history_file(tmp_path), tmp_path / "downloads", output, current_value=13
    ) == 1
    assert json.loads(output.read_text(encoding="utf-8"))["window_runs"] == 7


def test_duplicate_history_fails_closed(tmp_path: Path) -> None:
    path = history_file(tmp_path)
    rows = json.loads(path.read_text(encoding="utf-8"))
    rows[-1]["databaseId"] = rows[0]["databaseId"]
    path.write_text(json.dumps(rows), encoding="utf-8")
    with pytest.raises(MODULE.InputError, match="duplicate"):
        MODULE.load_history(path)
