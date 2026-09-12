from __future__ import annotations

import hashlib
import json
import stat
import sys
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

import run_b139_semgrep as bundle_runner  # noqa: E402
from run_b139_semgrep import OUTPUT_NAMES, run_bundle  # noqa: E402
from verify_b139_semgrep import (  # noqa: E402
    VerificationError,
    classify_uploads,
    enforce_report,
)


FAKE_SEMGREP = r"""#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

args = sys.argv[1:]
if args == ["--version"]:
    print("1.164.0")
    raise SystemExit(0)
output = Path(args[args.index("--output") + 1])
configs = [args[index + 1] for index, value in enumerate(args) if value == "--config"]
custom = configs == ["./semgrep.yml"]
population = "CUSTOM" if custom else "BUNDLED"
level = os.environ.get(f"FAKE_{population}_LEVEL", "warning")
count = int(os.environ.get(f"FAKE_{population}_COUNT", "1"))
prefix = "corelink.test." if custom else "python.test."
results = [
    {
        "ruleId": f"{prefix}{index}",
        "level": level,
        "message": {"text": "controlled finding"},
        "locations": [{"physicalLocation": {"artifactLocation": {"uri": "fixture.py"}}}],
    }
    for index in reversed(range(count))
]
sarif = {
    "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
    "version": "2.1.0",
    "runs": [{"tool": {"driver": {"name": "fake-semgrep", "rules": []}}, "results": results}],
}
output.write_text(json.dumps(sarif))
raise SystemExit(int(os.environ.get(f"FAKE_{population}_RC", "0")))
"""


@pytest.fixture()
def fake_semgrep(tmp_path: Path) -> Path:
    executable = tmp_path / "semgrep"
    executable.write_text(FAKE_SEMGREP, encoding="utf-8")
    executable.chmod(executable.stat().st_mode | stat.S_IXUSR)
    return executable


@pytest.fixture(autouse=True)
def locked_registry(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    content_by_url = {
        f"https://semgrep.dev/c/{name}": f"rules: [] # {name}\n".encode()
        for name in bundle_runner.BUNDLED
    }
    lock = {
        "schema_version": 1,
        "semgrep_version": "1.164.0",
        "rulesets": [
            {
                "name": name,
                "url": f"https://semgrep.dev/c/{name}",
                "sha256": hashlib.sha256(
                    content_by_url[f"https://semgrep.dev/c/{name}"]
                ).hexdigest(),
            }
            for name in bundle_runner.BUNDLED
        ],
    }
    lock_path = tmp_path / "semgrep-bundled-lock.json"
    lock_path.write_text(json.dumps(lock), encoding="utf-8")
    monkeypatch.setattr(bundle_runner, "LOCKFILE", lock_path)
    monkeypatch.setattr(bundle_runner, "_download", content_by_url.__getitem__)


def _load(path: Path) -> dict[str, object]:
    return json.loads(path.read_text(encoding="utf-8"))


def test_ci_semgrep_venv_is_excluded_from_scan() -> None:
    """The workflow creates this venv before scanning the repository root."""
    for ignore_file in (ROOT / ".gitignore", ROOT / ".semgrepignore"):
        assert ".semgrep-venv/" in ignore_file.read_text(encoding="utf-8").splitlines()


def test_complete_bundle_is_deterministic(fake_semgrep: Path, tmp_path: Path) -> None:
    output = tmp_path / "evidence"
    assert run_bundle(fake_semgrep, output, ROOT) == 0
    first = {
        name: (output / filename).read_bytes()
        for name, filename in OUTPUT_NAMES.items()
    }

    assert run_bundle(fake_semgrep, output, ROOT) == 0
    second = {
        name: (output / filename).read_bytes()
        for name, filename in OUTPUT_NAMES.items()
    }

    assert first == second
    assert set(path.name for path in output.iterdir()) == set(OUTPUT_NAMES.values())
    report = _load(output / OUTPUT_NAMES["report"])
    assert report["blocking"]["verdict"] == "PASS"  # type: ignore[index]


@pytest.mark.parametrize("population", ["BUNDLED", "CUSTOM"])
def test_every_error_population_blocks(
    fake_semgrep: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, population: str
) -> None:
    monkeypatch.setenv(f"FAKE_{population}_LEVEL", "error")
    output = tmp_path / population.lower()

    assert run_bundle(fake_semgrep, output, ROOT) == 1
    report = _load(output / OUTPUT_NAMES["report"])
    assert report["blocking"] == {  # type: ignore[index]
        "explicit_error_findings": 1,
        "scanner_error": False,
        "verdict": "FAIL",
    }


@pytest.mark.parametrize("population", ["BUNDLED", "CUSTOM"])
def test_every_scanner_error_blocks(
    fake_semgrep: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, population: str
) -> None:
    monkeypatch.setenv(f"FAKE_{population}_RC", "7")
    output = tmp_path / population.lower()

    assert run_bundle(fake_semgrep, output, ROOT) == 1
    report = _load(output / OUTPUT_NAMES["report"])
    assert report["blocking"]["scanner_error"] is True  # type: ignore[index]
    assert report["blocking"]["verdict"] == "FAIL"  # type: ignore[index]


@pytest.mark.parametrize(
    ("bundled", "custom", "expected_bundled", "expected_custom", "claim"),
    [
        ("success", "success", "uploaded", "uploaded", "verified"),
        (
            "failure",
            "success",
            "unavailable_or_unverified",
            "uploaded",
            "unverified-until-both-upload-steps-succeed",
        ),
        (
            "skipped",
            "cancelled",
            "unavailable_or_unverified",
            "unavailable_or_unverified",
            "unverified-until-both-upload-steps-succeed",
        ),
    ],
)
def test_ghas_upload_classification_is_evidence_bound(
    fake_semgrep: Path,
    tmp_path: Path,
    bundled: str,
    custom: str,
    expected_bundled: str,
    expected_custom: str,
    claim: str,
) -> None:
    output = tmp_path / "evidence"
    assert run_bundle(fake_semgrep, output, ROOT) == 0
    report_path = output / OUTPUT_NAMES["report"]

    classify_uploads(report_path, bundled, custom)
    upload = _load(report_path)["sarif_upload"]
    assert upload["bundled"] == expected_bundled  # type: ignore[index]
    assert upload["custom"] == expected_custom  # type: ignore[index]
    assert upload["advanced_security_claim"] == claim  # type: ignore[index]
    assert enforce_report(report_path) == 0


def test_unknown_upload_outcome_is_rejected(fake_semgrep: Path, tmp_path: Path) -> None:
    output = tmp_path / "evidence"
    assert run_bundle(fake_semgrep, output, ROOT) == 0
    with pytest.raises(VerificationError, match="unsupported bundled upload outcome"):
        classify_uploads(output / OUTPUT_NAMES["report"], "", "success")


def test_pending_upload_classification_cannot_pass(
    fake_semgrep: Path, tmp_path: Path
) -> None:
    output = tmp_path / "evidence"
    assert run_bundle(fake_semgrep, output, ROOT) == 0
    with pytest.raises(VerificationError, match="no closed upload step outcomes"):
        enforce_report(output / OUTPUT_NAMES["report"])


@pytest.mark.parametrize("tamper", ["error-count", "upload-status", "ghas-claim"])
def test_final_gate_rejects_tampered_evidence(
    fake_semgrep: Path, tmp_path: Path, tamper: str
) -> None:
    output = tmp_path / "evidence"
    assert run_bundle(fake_semgrep, output, ROOT) == 0
    report_path = output / OUTPUT_NAMES["report"]
    classify_uploads(report_path, "failure", "success")
    report = _load(report_path)
    if tamper == "error-count":
        report["bundled"]["levels"]["error"] = 1  # type: ignore[index]
        report["bundled"]["results"] += 1  # type: ignore[index,operator]
    elif tamper == "upload-status":
        report["sarif_upload"]["bundled"] = "uploaded"  # type: ignore[index]
    else:
        report["sarif_upload"]["advanced_security_claim"] = "verified"  # type: ignore[index]
    report_path.write_text(json.dumps(report), encoding="utf-8")

    with pytest.raises(VerificationError):
        enforce_report(report_path)


def test_output_symlink_is_rejected(fake_semgrep: Path, tmp_path: Path) -> None:
    output = tmp_path / "evidence"
    output.mkdir()
    (output / OUTPUT_NAMES["bundled"]).symlink_to(tmp_path / "outside.sarif")

    with pytest.raises(VerificationError, match="refusing symlink output path"):
        run_bundle(fake_semgrep, output, ROOT)


def test_ruleset_content_mutation_is_rejected(
    fake_semgrep: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    original_fetch = bundle_runner._download

    def changed_content(url: str) -> bytes:
        content = original_fetch(url)
        return (
            content + b"# mutable registry response\n"
            if url.endswith("security-audit")
            else content
        )

    monkeypatch.setattr(bundle_runner, "_download", changed_content)
    with pytest.raises(
        VerificationError, match="ruleset digest mismatch: p/security-audit"
    ):
        run_bundle(fake_semgrep, tmp_path / "evidence", ROOT)


def test_ruleset_order_mutation_is_rejected(tmp_path: Path) -> None:
    lock = json.loads(bundle_runner.LOCKFILE.read_text(encoding="utf-8"))
    lock["rulesets"] = list(reversed(lock["rulesets"]))
    changed = tmp_path / "reordered-lock.json"
    changed.write_text(json.dumps(lock), encoding="utf-8")

    with pytest.raises(VerificationError, match="population/order changed"):
        bundle_runner._load_lock(changed)


def _sarif_variant(
    root: str, *, reverse: bool, stamp: str, message: str = "same"
) -> dict:
    def run(name: str, offset: int) -> dict:
        rules = [
            {"id": f"rule-{offset + index}", "properties": {"tags": ["z", "a"]}}
            for index in range(2)
        ]
        results = [
            {
                "ruleId": rule["id"],
                "level": "warning",
                "message": {"text": message},
                "locations": [
                    {
                        "physicalLocation": {
                            "artifactLocation": {"uri": f"{root}/src/file.py"}
                        }
                    }
                ],
            }
            for rule in rules
        ]
        if reverse:
            rules.reverse()
            results.reverse()
        return {
            "automationDetails": {"id": name, "guid": f"volatile-{stamp}-{name}"},
            "invocations": [
                {
                    "executionSuccessful": True,
                    "startTimeUtc": stamp,
                    "endTimeUtc": stamp,
                    "commandLine": f"semgrep --output {root}/out.sarif",
                    "workingDirectory": {"uri": root},
                }
            ],
            "tool": {"driver": {"name": name, "rules": rules}},
            "results": results,
        }

    runs = [run("scanner-b", 2), run("scanner-a", 0)]
    if reverse:
        runs.reverse()
    return {"version": "2.1.0", "runs": runs}


def test_canonicalization_normalizes_order_and_volatile_fields(tmp_path: Path) -> None:
    first = tmp_path / "first.sarif"
    second = tmp_path / "second.sarif"
    first.write_text(
        json.dumps(
            _sarif_variant(
                "/private/tmp/build-one", reverse=False, stamp="2026-01-01T00:00:00Z"
            )
        ),
        encoding="utf-8",
    )
    second.write_text(
        json.dumps(
            _sarif_variant(
                "/private/tmp/build-two", reverse=True, stamp="2027-02-02T02:02:02Z"
            )
        ),
        encoding="utf-8",
    )

    bundle_runner._canonical_json(first, Path("/private/tmp/build-one"))
    bundle_runner._canonical_json(second, Path("/private/tmp/build-two"))
    assert first.read_bytes() == second.read_bytes()


def test_canonicalization_preserves_semantic_content_mutation(tmp_path: Path) -> None:
    first = tmp_path / "first.sarif"
    second = tmp_path / "second.sarif"
    first.write_text(
        json.dumps(_sarif_variant("/src", reverse=False, stamp="one")), encoding="utf-8"
    )
    second.write_text(
        json.dumps(
            _sarif_variant("/src", reverse=False, stamp="two", message="changed")
        ),
        encoding="utf-8",
    )

    bundle_runner._canonical_json(first, Path("/src"))
    bundle_runner._canonical_json(second, Path("/src"))
    assert first.read_bytes() != second.read_bytes()
