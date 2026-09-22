from __future__ import annotations

import hashlib
import json
import gzip
import shutil
import stat
import subprocess
import sys
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

import run_b139_semgrep as bundle_runner  # noqa: E402
from run_b139_semgrep import OUTPUT_NAMES, _canonical_json, run_bundle  # noqa: E402
from verify_b139_semgrep import (  # noqa: E402
    VerificationError,
    _sarif_counts,
    classify_uploads,
    CUSTOM_POLICY,
    check_static,
    enforce_report,
    evaluate,
)
import verify_b139_semgrep as b139_policy  # noqa: E402


def test_workflow_locks_semgrep_to_hosted_runner() -> None:
    workflow = (ROOT / ".github/workflows/semgrep.yml").read_text(encoding="utf-8")
    check_static()
    with pytest.raises(VerificationError, match="hosted ubuntu-latest"):
        check_static(workflow=workflow.replace("runs-on: ubuntu-latest", "runs-on: corelink", 1))


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
custom_rule = "corelink.rust.no-tokio-in-lib-crates" if level == "error" else "corelink.rust.no-unwrap-in-src"
if custom:
    bundled_rules = []
else:
    # Semgrep uses a dotted config path when it cannot resolve a stable rule ID.
    # The temporary parent is intentionally different on every invocation.
    rules_prefix = str(Path(configs[0]).parent).lstrip("/").replace("/", ".")
    bundled_rules = [
        {"id": (f"{rules_prefix}.{Path(config).name}.fixture-rule-{index}"
                 if index == 0 else f"fixture-rule-{index}"),
         "name": f"{rules_prefix}.{Path(config).name}.fixture-rule-{index}",
         "shortDescription": {"text": f"{rules_prefix}.{Path(config).name}.fixture-rule-{index}"},
         "defaultConfiguration": {"level": "warning"}}
        for index, config in enumerate(configs)
    ]
results = [
    {
        "ruleId": custom_rule if custom else bundled_rules[0]["id"],
        "level": level,
        "message": {"text": "controlled finding"},
        "locations": [{"physicalLocation": {"artifactLocation": {"uri": "fixture.py"}}}],
        "benign": "similar-corelink-b139-rules-text",
    }
    for index in reversed(range(count))
]
notification_texts = (["similar-corelink-b139-rules-text"] if custom else [
    f"Syntax error at line 1. When parsing expression in rule '{rules_prefix}.{Path(configs[0]).name}.notification'",
    f"rule {rules_prefix}.{Path(configs[0]).name}.notification could not be loaded",
    f"when running {rules_prefix}.{Path(configs[0]).name}.notification then rule {rules_prefix}.{Path(configs[0]).name}.again",
    "unrelated context corelink-b139-rules-sentinel",
    "similar-corelink-b139-rules-text",
])
sarif = {
    "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
    "version": "2.1.0",
    "runs": [{
        "tool": {"driver": {"name": "fake-semgrep", "rules": bundled_rules if not custom else [
            {"id": "corelink.rust.no-unwrap-in-src", "defaultConfiguration": {"level": "warning"}},
            {"id": "corelink.rust.no-tokio-in-lib-crates", "defaultConfiguration": {"level": "error"}},
            {"id": "corelink.rust.prop-assert-matches-struct-variant", "defaultConfiguration": {"level": "error"}},
            {"id": "corelink.rust.no-expect-in-byok-src", "defaultConfiguration": {"level": "warning"}},
        ] if custom else []}},
        "results": results,
        "invocations": [{"toolExecutionNotifications": [
            {"message": {"text": text}} for text in notification_texts
        ]}],
    }],
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
    snapshot_dir = tmp_path / "semgrep-rulesets"
    snapshot_dir.mkdir()
    source_dir = ROOT / "semgrep-rulesets"
    lock = {
        "schema_version": 1,
        "semgrep_version": "1.164.0",
        "captured_at": "2026-09-13T21:20:43Z",
        "provenance": "fixture diagnostic capture",
        "snapshot_dir": "semgrep-rulesets",
        "rulesets": [
            {
                "name": name,
                "url": f"https://semgrep.dev/c/{name}",
                "snapshot": f"{index:02d}-{name.removeprefix('p/')}.yml.gz",
                "size_bytes": len(gzip.open(source_dir / f"{index:02d}-{name.removeprefix('p/')}.yml.gz", "rb").read()),
                "sha256": hashlib.sha256(gzip.open(source_dir / f"{index:02d}-{name.removeprefix('p/')}.yml.gz", "rb").read()).hexdigest(),
            }
            for index, name in enumerate(bundle_runner.BUNDLED)
        ],
    }
    for source in source_dir.glob("*.yml.gz"):
        shutil.copy2(source, snapshot_dir / source.name)
    lock_path = tmp_path / "semgrep-bundled-lock.json"
    lock_path.write_text(json.dumps(lock), encoding="utf-8")
    monkeypatch.setattr(bundle_runner, "LOCKFILE", lock_path)
    monkeypatch.setattr(bundle_runner, "ROOT", tmp_path)


def _load(path: Path) -> dict[str, object]:
    return json.loads(path.read_text(encoding="utf-8"))


def test_ci_semgrep_venv_is_excluded_from_scan() -> None:
    """The workflow creates this venv before scanning the repository root."""
    for ignore_file in (ROOT / ".gitignore", ROOT / ".semgrepignore"):
        lines = ignore_file.read_text(encoding="utf-8").splitlines()
        assert "/.semgrep-venv/" in lines
        assert ".semgrep-venv/" not in lines

    root_venv = "./.semgrep-venv/bin/pysemgrep"
    nested_source = "crates/corelink-server/src/.semgrep-venv/example.py"
    for path, expected in ((root_venv, 0), (nested_source, 1)):
        ignored = subprocess.run(
            ["git", "check-ignore", "--no-index", "-q", "--", path],
            cwd=ROOT,
            check=False,
            capture_output=True,
        )
        assert ignored.returncode == expected, path


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
    bundled_sarif = _load(output / OUTPUT_NAMES["bundled"])
    serialized = json.dumps(bundled_sarif)
    notifications = bundled_sarif["runs"][0]["invocations"][0]["toolExecutionNotifications"]  # type: ignore[index]
    benign = notifications[3]["message"]["text"]  # type: ignore[index]
    assert benign == "unrelated context corelink-b139-rules-sentinel"
    assert "%B139_ROOT_2%" not in benign
    assert "corelink-b139-rules-" not in serialized.replace(benign, "").replace(
        "similar-corelink-b139-rules-text", ""
    )
    assert "%B139_ROOT_2%.00-security-audit.yml.fixture-rule-" in serialized
    assert "%B139_ROOT_2%.00-security-audit.yml.notification" in serialized
    assert "similar-corelink-b139-rules-text" in serialized


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
        "approved_suppressed_error_findings": 0,
        "unsuppressed_error_findings": 1,
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
    snapshot = bundle_runner.ROOT / "semgrep-rulesets/00-security-audit.yml.gz"
    snapshot.write_bytes(snapshot.read_bytes() + b"tamper")
    with pytest.raises(VerificationError):
        run_bundle(fake_semgrep, tmp_path / "evidence", ROOT)


def test_network_fetcher_is_not_called(fake_semgrep: Path, tmp_path: Path) -> None:
    source = (ROOT / "scripts/run_b139_semgrep.py").read_text(encoding="utf-8")
    assert "urllib" not in source
    assert "urlopen" not in source
    assert "_download" not in source
    assert run_bundle(fake_semgrep, tmp_path / "evidence", ROOT) == 0


@pytest.mark.parametrize("mutation", ["missing", "symlink", "size", "corrupt", "root-symlink"])
def test_snapshot_files_fail_closed(fake_semgrep: Path, tmp_path: Path, mutation: str) -> None:
    snapshot_dir = bundle_runner.ROOT / "semgrep-rulesets"
    snapshot = snapshot_dir / "00-security-audit.yml.gz"
    original = snapshot.read_bytes()
    if mutation == "missing":
        snapshot.unlink()
    elif mutation == "symlink":
        outside = tmp_path / "outside.yml.gz"
        outside.write_bytes(original)
        snapshot.unlink()
        snapshot.symlink_to(outside)
    elif mutation == "corrupt":
        snapshot.write_bytes(b"not-gzip")
    elif mutation == "size":
        snapshot.write_bytes(original + b"x" * (bundle_runner.MAX_RULESET_BYTES + 1))
    else:
        snapshot_dir.rename(tmp_path / "snapshot-backup")
        snapshot_dir.symlink_to(tmp_path / "snapshot-backup")
    expected = "snapshot directory" if mutation == "root-symlink" else None
    with pytest.raises(VerificationError, match=expected):
        run_bundle(fake_semgrep, tmp_path / "evidence", ROOT)


@pytest.mark.parametrize("field,value", [("semgrep_version", "1.163.0"), ("captured_at", "yesterday"), ("size_bytes", True)])
def test_lock_metadata_fails_closed(tmp_path: Path, field: str, value: object) -> None:
    lock = json.loads(bundle_runner.LOCKFILE.read_text(encoding="utf-8"))
    if field == "size_bytes":
        lock["rulesets"][0][field] = value
    else:
        lock[field] = value
    path = tmp_path / "bad-lock.json"
    path.write_text(json.dumps(lock), encoding="utf-8")
    with pytest.raises(VerificationError):
        bundle_runner._load_lock(path)


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


def _artifact_sarif(uri: str) -> dict:
    return {
        "version": "2.1.0",
        "runs": [{
            "tool": {"driver": {"name": "fixture", "rules": [{
                "id": "fixture.rule",
                "defaultConfiguration": {"level": "warning"},
            }]}},
            "results": [{
                "ruleId": "fixture.rule",
                "level": "warning",
                "locations": [{"physicalLocation": {
                    "artifactLocation": {"uri": uri},
                    "region": {"startLine": 7, "endLine": 7},
                }}],
            }],
        }],
    }


def _canonical_artifact(tmp_path: Path, uri: str) -> dict:
    repository = tmp_path / "repository"
    repository.mkdir()
    path = repository / "evidence.sarif"
    path.write_text(json.dumps(_artifact_sarif(uri)), encoding="utf-8")
    _canonical_json(path, repository, dotted_rule_root=repository)
    return _load(path)


def test_artifact_locations_are_repository_relative_and_semantics_remain(
    tmp_path: Path,
) -> None:
    """Code Scanning receives paths, while rule/level/region fields survive."""
    # This uses an explicit root in the fixture because the real scanner's
    # absolute path is canonicalized to the corresponding B139 root label.
    repository = tmp_path / "fixture-repository"
    repository.mkdir()
    data = _artifact_sarif("%B139_ROOT_0%/src/main.rs")
    path = tmp_path / "fixture-artifact.sarif"
    path.write_text(json.dumps(data), encoding="utf-8")
    _canonical_json(path, repository, dotted_rule_root=repository)
    normalized = _load(path)
    result = normalized["runs"][0]["results"][0]  # type: ignore[index]
    artifact = result["locations"][0]["physicalLocation"]["artifactLocation"]  # type: ignore[index]
    assert artifact == {"uri": "src/main.rs"}
    assert result["ruleId"] == "fixture.rule"  # type: ignore[index]
    assert result["level"] == "warning"  # type: ignore[index]
    assert result["locations"][0]["physicalLocation"]["region"] == {  # type: ignore[index]
        "startLine": 7,
        "endLine": 7,
    }


@pytest.mark.parametrize(
    "uri",
    [
        "%B139_ROOT_99%/src/main.rs",
        "%B139_ROOT_%/src/main.rs",
        "%B139_ROOT_0%/../outside.rs",
        "/private/etc/passwd",
        "file:///private/etc/passwd",
    ],
)
def test_artifact_location_unresolvable_roots_fail_closed(tmp_path: Path, uri: str) -> None:
    with pytest.raises(VerificationError, match="artifactLocation"):
        _canonical_artifact(tmp_path, uri)


def test_srcroot_base_id_is_removed_after_uri_normalization(tmp_path: Path) -> None:
    repository = tmp_path / "repository"
    repository.mkdir()
    path = repository / "evidence.sarif"
    data = _artifact_sarif("%B139_ROOT_0%/src/main.rs")
    data["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]["uriBaseId"] = "%SRCROOT%"  # type: ignore[index]
    path.write_text(json.dumps(data), encoding="utf-8")
    _canonical_json(path, repository)
    artifact = _load(path)["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]  # type: ignore[index]
    assert artifact == {"uri": "src/main.rs"}


def test_unresolved_uri_base_fails_closed(tmp_path: Path) -> None:
    repository = tmp_path / "repository"
    repository.mkdir()
    path = repository / "evidence.sarif"
    data = _artifact_sarif("%B139_ROOT_0%/src/main.rs")
    data["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]["uriBaseId"] = "%B139_ROOT_99%"  # type: ignore[index]
    path.write_text(json.dumps(data), encoding="utf-8")
    with pytest.raises(VerificationError, match="URI base"):
        _canonical_json(path, repository)


def _write_sarif(
    path: Path,
    rules: list[dict],
    results: list[dict],
    *,
    invocations: list[dict] | None = None,
) -> Path:
    path.write_text(
        json.dumps(
            {
                "version": "2.1.0",
                "runs": [
                    {
                        "tool": {"driver": {"name": "Semgrep", "rules": rules}},
                        "results": results,
                        "invocations": [] if invocations is None else invocations,
                    }
                ],
            }
        ),
        encoding="utf-8",
    )
    return path


def test_omitted_result_level_uses_rule_error_and_blocks(tmp_path: Path) -> None:
    bundled = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": "python.example", "defaultConfiguration": {"level": "error"}}],
        [{"ruleId": "python.example"}],
    )
    custom = _write_sarif(
        tmp_path / "custom.sarif",
        [
            {"id": rule_id, "defaultConfiguration": {"level": severity[0].lower()}}
            for rule_id, severity in CUSTOM_POLICY.items()
        ],
        [{"ruleId": "corelink.rust.no-tokio-in-lib-crates"}],
    )
    report = tmp_path / "report.json"
    assert evaluate(bundled, custom, report, 0, 0) == 1
    data = _load(report)
    assert data["blocking"]["explicit_error_findings"] == 2  # type: ignore[index]
    assert data["blocking"]["verdict"] == "FAIL"  # type: ignore[index]


def test_explicit_result_level_overrides_rule_default(tmp_path: Path) -> None:
    path = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": "python.example", "defaultConfiguration": {"level": "error"}}],
        [{"ruleId": "python.example", "level": "warning"}],
    )
    counts, total = _sarif_counts(path, custom=False)
    assert total == 1
    assert counts["warning"] == 1
    assert counts["error"] == 0


def test_implicit_warning_only_without_explicit_or_rule_level(tmp_path: Path) -> None:
    path = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": "python.example", "defaultConfiguration": {}}],
        [{"ruleId": "python.example"}],
    )
    assert _sarif_counts(path, custom=False)[0]["warning"] == 1


def test_index_only_rule_reference_and_suppressed_result_count(tmp_path: Path) -> None:
    path = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": "python.example", "defaultConfiguration": {"level": "error"}}],
        [{"ruleIndex": 0, "suppressions": [{"kind": "inSource"}]}],
    )
    assert _sarif_counts(path, custom=False)[0]["error"] == 1


def test_approved_bundled_suppression_is_retained_but_nonblocking(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    site = (
        ".github/workflows/example.yml",
        12,
        b139_policy.SUPPRESSION_RULE,
    )
    monkeypatch.setattr(b139_policy, "_approved_suppression_sites", lambda: {site})
    bundled = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": b139_policy.SUPPRESSION_RULE, "defaultConfiguration": {"level": "error"}}],
        [{
            "ruleId": b139_policy.SUPPRESSION_RULE,
            "suppressions": [{"kind": "inSource"}],
            "locations": [{"physicalLocation": {
                "artifactLocation": {"uri": site[0]},
                "region": {"startLine": site[1]},
            }}],
        }],
    )
    custom = _write_sarif(
        tmp_path / "custom.sarif",
        [
            {"id": rule_id, "defaultConfiguration": {"level": severity[0].lower()}}
            for rule_id, severity in CUSTOM_POLICY.items()
        ],
        [],
    )
    report = tmp_path / "report.json"
    assert evaluate(bundled, custom, report, 0, 0) == 0
    data = _load(report)
    assert data["bundled"]["approved_suppressed"] == 1  # type: ignore[index]
    assert data["blocking"]["explicit_error_findings"] == 1  # type: ignore[index]
    assert data["blocking"]["unsuppressed_error_findings"] == 0  # type: ignore[index]
    assert data["blocking"]["verdict"] == "PASS"  # type: ignore[index]


def test_unapproved_bundled_suppression_fails_closed(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    site = (
        ".github/workflows/example.yml",
        12,
        b139_policy.SUPPRESSION_RULE,
    )
    monkeypatch.setattr(b139_policy, "_approved_suppression_sites", lambda: set())
    bundled = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": b139_policy.SUPPRESSION_RULE, "defaultConfiguration": {"level": "error"}}],
        [{
            "ruleId": b139_policy.SUPPRESSION_RULE,
            "suppressions": [{"kind": "inSource"}],
            "locations": [{"physicalLocation": {
                "artifactLocation": {"uri": site[0]},
                "region": {"startLine": site[1]},
            }}],
        }],
    )
    custom = _write_sarif(
        tmp_path / "custom.sarif",
        [
            {"id": rule_id, "defaultConfiguration": {"level": severity[0].lower()}}
            for rule_id, severity in CUSTOM_POLICY.items()
        ],
        [],
    )
    with pytest.raises(VerificationError, match="unapproved suppression"):
        evaluate(bundled, custom, tmp_path / "report.json", 0, 0)


@pytest.mark.parametrize(
    "result",
    [
        {"ruleId": "python.example", "ruleIndex": -1},
        {"ruleId": "python.example", "ruleIndex": True},
        {"ruleId": "python.example", "ruleIndex": 1},
        {"ruleId": "python.other", "ruleIndex": 0},
        {"ruleId": "python.example", "rule": {"id": "python.other"}},
        {"ruleId": "python.example", "ruleIndex": 0, "rule": {"index": 1}},
    ],
)
def test_malformed_or_conflicting_rule_reference_fails_closed(
    tmp_path: Path, result: dict
) -> None:
    path = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": "python.example", "defaultConfiguration": {"level": "error"}}],
        [result],
    )
    with pytest.raises(VerificationError):
        _sarif_counts(path, custom=False)


def test_ambiguous_rule_id_fails_closed(tmp_path: Path) -> None:
    path = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": "python.example"}, {"id": "python.example"}],
        [{"ruleId": "python.example"}],
    )
    with pytest.raises(VerificationError, match="ambiguous"):
        _sarif_counts(path, custom=False)


def test_malformed_rule_severity_fails_closed(tmp_path: Path) -> None:
    path = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": "python.example", "defaultConfiguration": {"level": "ERROR"}}],
        [{"ruleId": "python.example"}],
    )
    with pytest.raises(VerificationError, match="unsupported SARIF level"):
        _sarif_counts(path, custom=False)


def test_custom_sarif_severity_must_match_calibrated_policy(tmp_path: Path) -> None:
    path = _write_sarif(
        tmp_path / "custom.sarif",
        [
            {
                "id": "corelink.rust.no-unwrap-in-src",
                "defaultConfiguration": {"level": "error"},
            }
        ],
        [{"ruleId": "corelink.rust.no-unwrap-in-src"}],
    )
    with pytest.raises(VerificationError, match="conflicts with custom policy"):
        _sarif_counts(path, custom=True)


def test_m3_custom_omitted_levels_keep_advisory_findings_nonblocking(
    tmp_path: Path,
) -> None:
    rules = [
        {
            "id": "corelink.rust.no-unwrap-in-src",
            "defaultConfiguration": {"level": "warning"},
        },
        {
            "id": "corelink.rust.no-tokio-in-lib-crates",
            "defaultConfiguration": {"level": "error"},
        },
        {
            "id": "corelink.rust.prop-assert-matches-struct-variant",
            "defaultConfiguration": {"level": "error"},
        },
        {
            "id": "corelink.rust.no-expect-in-byok-src",
            "defaultConfiguration": {"level": "warning"},
        },
    ]
    path = _write_sarif(
        tmp_path / "custom.sarif",
        rules,
        [
            {"ruleId": "corelink.rust.no-unwrap-in-src"},
            {
                "ruleId": "corelink.rust.no-expect-in-byok-src",
                "suppressions": [{"kind": "inSource"}],
            },
        ],
    )
    counts, total = _sarif_counts(path, custom=True)
    assert total == 2
    assert counts == {"error": 0, "warning": 2, "note": 0, "none": 0}


def test_invocation_override_cannot_turn_error_into_implicit_warning(
    tmp_path: Path,
) -> None:
    path = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": "python.example", "defaultConfiguration": {"level": "warning"}}],
        [{"ruleId": "python.example", "provenance": {"invocationIndex": 0}}],
        invocations=[{"ruleConfigurationOverrides": [{
            "descriptor": {"id": "python.example", "index": 0},
            "configuration": {"level": "error"},
        }]}],
    )
    with pytest.raises(VerificationError, match="overrides"):
        _sarif_counts(path, custom=False)


def test_result_provenance_without_override_is_rejected(tmp_path: Path) -> None:
    path = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": "python.example", "defaultConfiguration": {"level": "warning"}}],
        [{"ruleId": "python.example", "provenance": {"invocationIndex": 0}}],
        invocations=[{}],
    )
    with pytest.raises(VerificationError, match="provenance"):
        _sarif_counts(path, custom=False)


def test_nonfailing_kind_defaults_to_none_not_rule_error(tmp_path: Path) -> None:
    path = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": "python.example", "defaultConfiguration": {"level": "error"}}],
        [{"ruleId": "python.example", "kind": "pass"}],
    )
    counts, _ = _sarif_counts(path, custom=False)
    assert counts["none"] == 1
    assert counts["error"] == 0


def test_nonfailing_kind_with_error_level_is_rejected(tmp_path: Path) -> None:
    path = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": "python.example", "defaultConfiguration": {"level": "error"}}],
        [{"ruleId": "python.example", "kind": "pass", "level": "error"}],
    )
    with pytest.raises(VerificationError, match="kind/level"):
        _sarif_counts(path, custom=False)


def test_guid_rule_reference_is_rejected_before_level_fallback(tmp_path: Path) -> None:
    path = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": "python.example", "defaultConfiguration": {"level": "error"}}],
        [{"ruleId": "python.example", "ruleIndex": 0, "rule": {"guid": "mismatch"}}],
    )
    with pytest.raises(VerificationError, match="unsupported SARIF rule reference"):
        _sarif_counts(path, custom=False)


@pytest.mark.parametrize("result", [
    {"ruleId": None, "ruleIndex": 0},
    {"ruleId": "python.example", "ruleIndex": None},
    {"ruleId": "python.example", "rule": None},
])
def test_null_rule_references_are_rejected(tmp_path: Path, result: dict) -> None:
    path = _write_sarif(
        tmp_path / "bundled.sarif",
        [{"id": "python.example", "defaultConfiguration": {"level": "error"}}],
        [result],
    )
    with pytest.raises(VerificationError, match="null SARIF rule reference"):
        _sarif_counts(path, custom=False)


def test_unknown_custom_rule_result_is_rejected(tmp_path: Path) -> None:
    path = _write_sarif(
        tmp_path / "custom.sarif",
        [{"id": "corelink.rust.new-rule", "defaultConfiguration": {"level": "warning"}}],
        [{"ruleId": "corelink.rust.new-rule"}],
    )
    with pytest.raises(VerificationError, match="unknown custom rule"):
        _sarif_counts(path, custom=True)


def _custom_result_sarif(tmp_path: Path, rule_id: str, result: dict) -> Path:
    rules = [
        {"id": configured_id, "defaultConfiguration": {"level": severity[0].lower()}}
        for configured_id, severity in CUSTOM_POLICY.items()
    ]
    return _write_sarif(
        tmp_path / "custom.sarif",
        rules,
        [{"ruleId": rule_id, **result}],
    )


def test_custom_error_result_cannot_be_downgraded(tmp_path: Path) -> None:
    path = _custom_result_sarif(
        tmp_path,
        "corelink.rust.no-tokio-in-lib-crates",
        {"level": "warning"},
    )
    with pytest.raises(VerificationError, match="conflicts with custom policy"):
        _sarif_counts(path, custom=True)


def test_custom_warning_result_cannot_be_escalated(tmp_path: Path) -> None:
    path = _custom_result_sarif(
        tmp_path,
        "corelink.rust.no-unwrap-in-src",
        {"level": "error"},
    )
    with pytest.raises(VerificationError, match="conflicts with custom policy"):
        _sarif_counts(path, custom=True)


def test_custom_result_matching_explicit_level_is_accepted(tmp_path: Path) -> None:
    path = _custom_result_sarif(
        tmp_path,
        "corelink.rust.no-tokio-in-lib-crates",
        {"level": "error"},
    )
    counts, total = _sarif_counts(path, custom=True)
    assert total == 1
    assert counts["error"] == 1


def test_custom_nonfailing_kind_ignores_calibrated_result_level(tmp_path: Path) -> None:
    path = _custom_result_sarif(
        tmp_path,
        "corelink.rust.no-tokio-in-lib-crates",
        {"kind": "pass"},
    )
    counts, total = _sarif_counts(path, custom=True)
    assert total == 1
    assert counts["none"] == 1


@pytest.mark.parametrize("descriptor_mutation", ["missing", "extra"])
def test_custom_sarif_requires_exact_rule_descriptor_set(
    tmp_path: Path, descriptor_mutation: str
) -> None:
    rules = [
        {"id": rule_id, "defaultConfiguration": {"level": severity[0].lower()}}
        for rule_id, severity in CUSTOM_POLICY.items()
    ]
    if descriptor_mutation == "missing":
        rules.pop()
    else:
        rules.append({"id": "corelink.rust.unconfigured"})
    path = _write_sarif(tmp_path / "custom.sarif", rules, [])
    with pytest.raises(VerificationError, match="descriptor set|unknown custom rule"):
        _sarif_counts(path, custom=True)


def test_custom_sarif_exact_rule_descriptor_set_allows_zero_findings(
    tmp_path: Path,
) -> None:
    rules = [
        {"id": rule_id, "defaultConfiguration": {"level": severity[0].lower()}}
        for rule_id, severity in CUSTOM_POLICY.items()
    ]
    path = _write_sarif(tmp_path / "custom.sarif", rules, [])
    counts, total = _sarif_counts(path, custom=True)
    assert total == 0
    assert counts == {"error": 0, "warning": 0, "note": 0, "none": 0}


def _valid_evaluated_report() -> dict:
    levels = {"error": 0, "warning": 0, "note": 0, "none": 0}
    return {
        "status": "evaluated",
        "scanner_exit_codes": {"bundled": 0, "custom": 0},
        "bundled": {"results": 0, "levels": levels.copy(), "approved_suppressed": 0},
        "custom": {"results": 0, "levels": levels.copy(), "approved_suppressed": 0},
        "blocking": {
            "explicit_error_findings": 0,
            "approved_suppressed_error_findings": 0,
            "unsuppressed_error_findings": 0,
            "scanner_error": False,
            "verdict": "PASS",
        },
        "sarif_upload": {
            "bundled": "uploaded",
            "custom": "uploaded",
            "step_outcomes": {"bundled": "success", "custom": "success"},
            "advanced_security_claim": "verified",
        },
    }


@pytest.mark.parametrize("tamper", [
    "invalid-exit-code",
    "mismatched-total",
    "missing-total",
    "missing-blocking-field",
])
def test_enforce_report_rejects_tampered_core_evidence(
    tmp_path: Path, tamper: str
) -> None:
    report = _valid_evaluated_report()
    if tamper == "invalid-exit-code":
        report["scanner_exit_codes"]["custom"] = -1  # type: ignore[index]
    elif tamper == "mismatched-total":
        report["bundled"]["results"] = 1  # type: ignore[index]
    elif tamper == "missing-total":
        del report["custom"]["results"]  # type: ignore[index]
    else:
        del report["blocking"]["verdict"]  # type: ignore[index]
    report_path = tmp_path / "report.json"
    report_path.write_text(json.dumps(report), encoding="utf-8")
    with pytest.raises(VerificationError):
        enforce_report(report_path)
