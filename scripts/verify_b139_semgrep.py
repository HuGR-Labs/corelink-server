#!/usr/bin/env python3
"""Fail-closed policy and SARIF evaluator for B-139.

The static mode checks the rulepack, workflow, and checked-in policy decision.
The evaluate mode consumes two local SARIF files and applies only the explicit
ERROR policy. It never dispatches a workflow, contacts GitHub, or runs Semgrep.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/semgrep.yml"
RUNNER = ROOT / "scripts/run_b139_semgrep.py"
BUNDLED_LOCK = ROOT / "semgrep-bundled-lock.json"
RULEPACK = ROOT / "semgrep.yml"
POLICY = ROOT / "docs/handoff/2026-09-06-b139-semgrep-policy-decision.json"

BUNDLED = (
    "p/security-audit",
    "p/rust",
    "p/typescript",
    "p/python",
    "p/owasp-top-ten",
    "p/cwe-top-25",
)
CUSTOM = (
    "corelink.rust.no-unwrap-in-src",
    "corelink.rust.no-tokio-in-lib-crates",
    "corelink.rust.prop-assert-matches-struct-variant",
    "corelink.rust.no-expect-in-byok-src",
)
CUSTOM_POLICY = {
    "corelink.rust.no-unwrap-in-src": (
        "WARNING",
        "advisory-duplicate-of-clippy",
        "pattern: $X.unwrap()",
    ),
    "corelink.rust.no-tokio-in-lib-crates": (
        "ERROR",
        "explicit-error",
        "pattern: tokio::$X",
    ),
    "corelink.rust.prop-assert-matches-struct-variant": (
        "ERROR",
        "explicit-error",
        "pattern-inside: prop_assert!(matches!(...))",
    ),
    "corelink.rust.no-expect-in-byok-src": (
        "WARNING",
        "advisory-duplicate-of-clippy",
        "pattern: $X.expect($MSG)",
    ),
}
CUSTOM_PATHS = {
    "corelink.rust.no-unwrap-in-src": {
        "include": ["/crates/*/src/**/*.rs", "/apps/**/src/**/*.rs"],
        "exclude": [
            "**/tests/**",
            "**/tests.rs",
            "**/*test*.rs",
            "**/test_*.rs",
            "**/benches/**",
            "**/examples/**",
            "**/build.rs",
            "**/bin/**",
        ],
    },
    "corelink.rust.no-tokio-in-lib-crates": {
        "include": [
            "/crates/corelink-lighthouse-tracker/src/**/*.rs",
            "/crates/corelink-synthetic-pager/src/**/*.rs",
        ],
        "exclude": [
            "**/tests/**",
            "**/tests.rs",
            "**/*_test.rs",
            "**/test_*.rs",
            "**/benches/**",
            "**/examples/**",
            "**/bin/**",
            "**/main.rs",
        ],
    },
    "corelink.rust.prop-assert-matches-struct-variant": {
        "include": ["/crates/**/*.rs", "/apps/**/*.rs", "/tests/**/*.rs"],
        "exclude": [],
    },
    "corelink.rust.no-expect-in-byok-src": {
        "include": [
            "/crates/corelink-byok/src/**/*.rs",
            "/crates/corelink-byok-aws/src/**/*.rs",
            "/crates/corelink-byok-azure/src/**/*.rs",
            "/crates/corelink-byok-gcp/src/**/*.rs",
            "/crates/corelink-byok-vault/src/**/*.rs",
            "/crates/corelink-byok-revocation/src/**/*.rs",
            "/crates/corelink-byok-matrix-test/src/**/*.rs",
        ],
        "exclude": ["**/tests/**", "**/tests.rs", "**/*test*.rs", "**/test_*.rs"],
    },
}
RULE_FIXTURE = ROOT / "tests/semgrep/b139/rust.rs"
LEGACY_LEVELS = ("error", "warning", "note", "none")
UPLOAD_OUTCOMES = ("success", "failure", "cancelled", "skipped")
REQUIRED_POLICY_KEYS = {
    "schema_version",
    "finding",
    "status",
    "non_claim",
    "scan_population",
    "policy",
    "sarif",
    "historical_baseline",
    "closure",
}


class VerificationError(ValueError):
    pass


def _read(path: Path) -> str:
    if not path.is_file() or path.is_symlink():
        raise VerificationError(f"missing/non-regular file: {path}")
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise VerificationError(f"cannot read {path}: {exc}") from exc


def _load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(_read(path))
    except json.JSONDecodeError as exc:
        raise VerificationError(f"invalid JSON {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise VerificationError(f"JSON root is not an object: {path}")
    return value


def _require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise VerificationError(f"{label} is missing {needle!r}")


def _rule_blocks(config: str) -> dict[str, str]:
    matches = list(re.finditer(r"(?m)^  - id:\s*([^\s]+)\s*$", config))
    blocks: dict[str, str] = {}
    for index, match in enumerate(matches):
        rule_id = match.group(1)
        end = matches[index + 1].start() if index + 1 < len(matches) else len(config)
        if rule_id in blocks:
            raise VerificationError(f"duplicate custom rule id: {rule_id}")
        blocks[rule_id] = config[match.start() : end]
    return blocks


def _path_list(block: str, name: str) -> list[str]:
    match = re.search(
        rf"(?m)^      {re.escape(name)}:\s*$\n((?:        - \"[^\"]+\"\s*$\n?)*)",
        block,
    )
    if match is None:
        return []
    return [
        json.loads(item)
        for item in re.findall(r'^        - ("[^"]+")\s*$', match.group(1), re.M)
    ]


def _check_policy(policy: dict[str, Any]) -> None:
    if set(policy) != REQUIRED_POLICY_KEYS:
        raise VerificationError("policy artifact fields are not closed")
    if policy.get("schema_version") != 1 or policy.get("finding") != "B-139":
        raise VerificationError("policy artifact identity changed")
    population = policy.get("scan_population")
    if not isinstance(population, dict):
        raise VerificationError("scan_population must be an object")
    if population.get("bundled_rulesets") != list(BUNDLED):
        raise VerificationError("bundled ruleset population changed")
    if population.get("bundled_lock") != "./semgrep-bundled-lock.json":
        raise VerificationError("bundled ruleset lock path changed")
    if "SHA-256" not in str(population.get("bundled_lock_policy", "")):
        raise VerificationError("bundled ruleset lock policy is not content-addressed")
    if population.get("custom_config") != "./semgrep.yml" or population.get(
        "custom_rule_count"
    ) != len(CUSTOM):
        raise VerificationError("custom rule population changed")
    if population.get("custom_rule_prefix") != "corelink.":
        raise VerificationError("custom rule prefix changed")
    expected_severities = {
        rule_id: values[0] for rule_id, values in CUSTOM_POLICY.items()
    }
    if population.get("custom_rule_severities") != expected_severities:
        raise VerificationError("custom rule severity ledger changed")
    rule_policy = policy.get("policy")
    if not isinstance(rule_policy, dict):
        raise VerificationError("policy section must be an object")
    if rule_policy.get("fail_closed") is not True:
        raise VerificationError("policy must remain fail closed")
    if (
        rule_policy.get("semgrep_error_flag")
        != "not used; the evaluator applies the explicit SARIF ERROR policy and preserves nonzero scanner failures"
    ):
        raise VerificationError(
            "policy must name the evaluator rather than silently dropping --error"
        )
    if "only when its SARIF level is error" not in str(
        rule_policy.get("blocking_condition", "")
    ):
        raise VerificationError("blocking policy is not severity-explicit")
    sarif = policy.get("sarif")
    if not isinstance(sarif, dict) or sarif.get("required_files") != [
        "semgrep-bundled.sarif",
        "semgrep-custom.sarif",
        "semgrep-b139-report.json",
    ]:
        raise VerificationError("SARIF evidence population changed")
    if sarif.get("upload_categories") != ["semgrep-bundled", "semgrep-custom"]:
        raise VerificationError("SARIF upload categories changed")
    if "unavailable_or_unverified" not in str(
        sarif.get("advanced_security_unavailable", "")
    ):
        raise VerificationError("SARIF upload limitation is not explicit")


def _check_bundled_lock(lock: dict[str, Any]) -> None:
    if set(lock) != {
        "schema_version",
        "semgrep_version",
        "captured_at",
        "provenance",
        "snapshot_dir",
        "rulesets",
    }:
        raise VerificationError("bundled lock fields are not closed")
    if lock.get("schema_version") != 1 or lock.get("semgrep_version") != "1.164.0":
        raise VerificationError("bundled lock schema/version changed")
    captured_at = lock.get("captured_at")
    if not isinstance(captured_at, str) or not re.fullmatch(
        r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", captured_at
    ):
        raise VerificationError("bundled lock capture timestamp is invalid")
    try:
        dt.datetime.strptime(captured_at, "%Y-%m-%dT%H:%M:%SZ")
    except ValueError as exc:
        raise VerificationError("bundled lock capture timestamp is invalid") from exc
    if not isinstance(lock.get("provenance"), str) or not lock["provenance"].strip():
        raise VerificationError("bundled lock provenance is missing")
    if lock.get("snapshot_dir") != "semgrep-rulesets":
        raise VerificationError("bundled lock snapshot directory changed")
    rulesets = lock.get("rulesets")
    if not isinstance(rulesets, list) or len(rulesets) != len(BUNDLED):
        raise VerificationError("bundled lock population changed")
    for index, (expected_name, item) in enumerate(
        zip(BUNDLED, rulesets, strict=True)
    ):
        if not isinstance(item, dict) or set(item) != {
            "name",
            "sha256",
            "url",
            "snapshot",
            "size_bytes",
        }:
            raise VerificationError("bundled ruleset fields are not closed")
        if item.get("name") != expected_name:
            raise VerificationError("bundled lock order changed")
        if item.get("url") != f"https://semgrep.dev/c/{expected_name}":
            raise VerificationError(f"bundled ruleset URL changed: {expected_name}")
        digest = item.get("sha256")
        if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise VerificationError(f"bundled ruleset digest invalid: {expected_name}")
        snapshot = f"{index:02d}-{expected_name.removeprefix('p/')}.yml.gz"
        if item.get("snapshot") != snapshot:
            raise VerificationError(f"bundled ruleset snapshot changed: {expected_name}")
        size_bytes = item.get("size_bytes")
        if (
            isinstance(size_bytes, bool)
            or not isinstance(size_bytes, int)
            or not 0 < size_bytes <= 32 * 1024 * 1024
        ):
            raise VerificationError(
                f"bundled ruleset snapshot size invalid: {expected_name}"
            )


def check_static(
    *,
    config: str | None = None,
    workflow: str | None = None,
    policy: dict[str, Any] | None = None,
    runner: str | None = None,
    bundled_lock: dict[str, Any] | None = None,
) -> dict[str, int]:
    config = _read(RULEPACK) if config is None else config
    workflow = _read(WORKFLOW) if workflow is None else workflow
    policy = _load_json(POLICY) if policy is None else policy
    runner = _read(RUNNER) if runner is None else runner
    bundled_lock = _load_json(BUNDLED_LOCK) if bundled_lock is None else bundled_lock
    _check_policy(policy)
    _check_bundled_lock(bundled_lock)

    blocks = _rule_blocks(config)
    if set(blocks) != set(CUSTOM):
        raise VerificationError(
            "custom rule population is not exactly the four CoreLink rules"
        )
    for rule_id in CUSTOM:
        block = blocks[rule_id]
        severity, policy_name, required_pattern = CUSTOM_POLICY[rule_id]
        _require(block, f"severity: {severity}", f"{rule_id} severity")
        _require(block, f"policy: {policy_name}", f"{rule_id} policy metadata")
        _require(block, "source: corelink-custom", f"{rule_id} source metadata")
        _require(block, required_pattern, f"{rule_id} executable pattern")
        for path_kind in ("include", "exclude"):
            if _path_list(block, path_kind) != CUSTOM_PATHS[rule_id][path_kind]:
                raise VerificationError(f"{rule_id} {path_kind} paths changed")

    _require(workflow, "semgrep-bundled.sarif", "bundled SARIF")
    _require(workflow, "semgrep-custom.sarif", "custom SARIF")
    _require(workflow, "run_b139_semgrep.py", "canonical local bundle runner")
    _require(workflow, "--lock semgrep-bundled-lock.json", "bundled ruleset lock")
    _require(workflow, "--classify-uploads", "upload classifier")
    _require(
        workflow, "verify_b139_semgrep.py --enforce-report", "final policy enforcement"
    )
    _require(workflow, "continue-on-error: true", "GHAS upload boundary")
    _require(workflow, "unavailable_or_unverified", "truthful upload classification")
    _require(workflow, "actions/upload-artifact@", "retained SARIF artifact")
    if re.search(r"(?m)^\s+--error\s*$", workflow):
        raise VerificationError("workflow still delegates policy to Semgrep --error")
    if 'exit "${SEMGREP_RC}"' in workflow:
        raise VerificationError("scanner step exits before the explicit evaluator")
    _require(
        runner, "from verify_b139_semgrep import BUNDLED", "shared bundled population"
    )
    _require(runner, "_materialize_rulesets", "content-locked bundled scan")
    _require(runner, "hmac.compare_digest", "ruleset digest verification")
    _require(runner, "VOLATILE_KEYS", "volatile SARIF normalization")
    _require(runner, "UNORDERED_ARRAY_KEYS", "unordered SARIF normalization")
    if "--config p/" in runner:
        raise VerificationError(
            "runner still passes mutable registry aliases to Semgrep"
        )
    _require(runner, '"./semgrep.yml"', "local custom scan")
    for output in (
        "semgrep-bundled.sarif",
        "semgrep-custom.sarif",
        "semgrep-b139-report.json",
    ):
        _require(runner, output, "deterministic evidence name")
    _require(runner, '"--jobs",\n            "1"', "deterministic single-job scan")
    if "datetime.now" in runner:
        raise VerificationError("B139 report contains a wall-clock-dependent field")
    return {"custom_rules": len(CUSTOM), "bundled_rulesets": len(BUNDLED)}


def _sarif_result_level(
    path: Path, result: dict[str, Any], rules: list[Any], *, custom: bool
) -> tuple[str, str]:
    """Resolve a result against its run's driver rules before applying defaults."""

    rule_id = result.get("ruleId")
    rule_index = result.get("ruleIndex")
    rule_ref = result.get("rule")
    if "provenance" in result:
        raise VerificationError(f"{path} has unsupported SARIF result provenance")
    if any(
        key in result and result[key] is None
        for key in ("ruleId", "ruleIndex", "rule")
    ):
        raise VerificationError(f"{path} has a null SARIF rule reference")
    if rule_ref is not None:
        if not isinstance(rule_ref, dict) or any(
            key in rule_ref for key in ("toolComponent", "guid")
        ):
            raise VerificationError(f"{path} has an unsupported SARIF rule reference")
        if "id" in rule_ref:
            if rule_id is not None and rule_id != rule_ref["id"]:
                raise VerificationError(f"{path} has conflicting SARIF rule ids")
            rule_id = rule_ref["id"]
        if "index" in rule_ref:
            if rule_index is not None and rule_index != rule_ref["index"]:
                raise VerificationError(f"{path} has conflicting SARIF rule indices")
            rule_index = rule_ref["index"]
        if any(
            key in rule_ref and rule_ref[key] is None for key in ("id", "index")
        ):
            raise VerificationError(f"{path} has a null SARIF rule reference")
    if rule_id is not None and (not isinstance(rule_id, str) or not rule_id):
        raise VerificationError(f"{path} has an invalid SARIF rule id")
    if rule_index is not None and (type(rule_index) is not int or rule_index < 0):
        raise VerificationError(f"{path} has an invalid SARIF rule index")
    if rule_id is None and rule_index is None:
        raise VerificationError(f"{path} has a result without a rule reference")

    descriptor: dict[str, Any] | None = None
    if rule_index is not None:
        if rule_index >= len(rules):
            raise VerificationError(f"{path} has an out-of-range SARIF rule index")
        descriptor = rules[rule_index]
        if rule_id is not None and rule_id != descriptor["id"]:
            raise VerificationError(f"{path} has conflicting SARIF rule id/index")
    elif rules:
        # Semgrep 1.164.0 emits ruleId without ruleIndex. Match its driver
        # descriptor only when the ID is unique within this run.
        matches = [rule for rule in rules if rule["id"] == rule_id]
        if len(matches) != 1:
            raise VerificationError(
                f"{path} has an unresolved or ambiguous SARIF rule id"
            )
        descriptor = matches[0]
    effective_id = descriptor["id"] if descriptor is not None else rule_id
    assert isinstance(effective_id, str)
    configuration = descriptor.get("defaultConfiguration") if descriptor else None
    if (
        descriptor is not None
        and "defaultConfiguration" in descriptor
        and configuration is None
    ):
        raise VerificationError(f"{path} has malformed SARIF rule configuration")
    if configuration is not None:
        if not isinstance(configuration, dict):
            raise VerificationError(f"{path} has malformed SARIF rule configuration")
        if "level" in configuration and configuration["level"] not in LEGACY_LEVELS:
            raise VerificationError(
                f"{path} has unsupported SARIF level {configuration['level']!r}"
            )

    # A SARIF file from another rulepack revision can disagree with M3's
    # calibrated severities. Reject that evidence instead of reclassifying it.
    custom_expected: str | None = None
    if custom:
        if effective_id not in CUSTOM_POLICY:
            raise VerificationError(f"{path} has an unknown custom rule {effective_id}")
        if descriptor is None:
            raise VerificationError(f"{path} has no descriptor for {effective_id}")
        if not isinstance(configuration, dict):
            raise VerificationError(f"{path} has no severity for {effective_id}")
        custom_expected = CUSTOM_POLICY[effective_id][0].lower()
        if configuration.get("level") != custom_expected:
            raise VerificationError(
                f"{path} SARIF severity for {effective_id} conflicts with custom policy"
            )

    kind = result.get("kind", "fail")
    if not isinstance(kind, str) or kind not in {
        "fail", "pass", "review", "open", "informational", "notApplicable"
    }:
        raise VerificationError(f"{path} has unsupported SARIF kind {kind!r}")
    if kind != "fail":
        if "level" in result and result["level"] != "none":
            raise VerificationError(f"{path} has inconsistent SARIF kind/level")
        return effective_id, "none"

    if "level" in result:
        level = result["level"]
    elif configuration is not None:
        level = configuration.get("level", "warning")
    else:
        level = "warning"  # SARIF's implicit level for a failing result.
    if level not in LEGACY_LEVELS:
        raise VerificationError(f"{path} has unsupported SARIF level {level!r}")
    if custom_expected is not None and "level" in result and level != custom_expected:
        raise VerificationError(
            f"{path} SARIF result severity for {effective_id} conflicts with custom policy"
        )
    return effective_id, level


def _sarif_counts(path: Path, *, custom: bool) -> tuple[dict[str, int], int]:
    data = _load_json(path)
    runs = data.get("runs")
    if not isinstance(runs, list) or not runs:
        raise VerificationError(f"{path} has no SARIF runs")
    counts = {level: 0 for level in LEGACY_LEVELS}
    results_seen = 0
    for run in runs:
        if not isinstance(run, dict) or not isinstance(run.get("results"), list):
            raise VerificationError(f"{path} has malformed SARIF run")
        tool = run.get("tool")
        driver = tool.get("driver") if isinstance(tool, dict) else None
        if not isinstance(driver, dict):
            raise VerificationError(f"{path} has no SARIF driver")
        invocations = run.get("invocations", [])
        if not isinstance(invocations, list) or any(
            not isinstance(invocation, dict) for invocation in invocations
        ):
            raise VerificationError(f"{path} has malformed SARIF invocations")
        for invocation in invocations:
            if "ruleConfigurationOverrides" in invocation:
                overrides = invocation["ruleConfigurationOverrides"]
                if not isinstance(overrides, list) or overrides:
                    raise VerificationError(
                        f"{path} has unsupported SARIF rule configuration overrides"
                    )
        rules = driver.get("rules", [])
        if not isinstance(rules, list) or any(
            not isinstance(rule, dict)
            or not isinstance(rule.get("id"), str)
            or not rule["id"]
            for rule in rules
        ):
            raise VerificationError(f"{path} has malformed SARIF driver rules")
        if custom:
            if len({rule["id"] for rule in rules}) != len(rules):
                raise VerificationError(f"{path} has duplicate custom rule descriptors")
            for rule in rules:
                rule_id = rule["id"]
                if rule_id not in CUSTOM_POLICY:
                    raise VerificationError(f"{path} has an unknown custom rule {rule_id}")
                configuration = rule.get("defaultConfiguration")
                if not isinstance(configuration, dict) or configuration.get(
                    "level"
                ) != CUSTOM_POLICY[rule_id][0].lower():
                    raise VerificationError(
                        f"{path} SARIF severity for {rule_id} conflicts with custom policy"
                    )
            if {rule["id"] for rule in rules} != set(CUSTOM_POLICY):
                raise VerificationError(
                    f"{path} has an incomplete or extra custom rule descriptor set"
                )
        for result in run["results"]:
            if not isinstance(result, dict):
                raise VerificationError(f"{path} has a malformed SARIF result")
            rule_id, level = _sarif_result_level(path, result, rules, custom=custom)
            is_custom = rule_id.startswith("corelink.")
            if is_custom != custom:
                expected = "custom CoreLink" if custom else "bundled"
                raise VerificationError(f"{path} mixes a non-{expected} rule result")
            counts[level] += 1
            results_seen += 1
    return counts, results_seen


def evaluate(
    bundled_sarif: Path,
    custom_sarif: Path,
    report: Path,
    bundled_rc: int,
    custom_rc: int,
) -> int:
    bundled_counts, bundled_results = _sarif_counts(bundled_sarif, custom=False)
    custom_counts, custom_results = _sarif_counts(custom_sarif, custom=True)
    error_findings = bundled_counts["error"] + custom_counts["error"]
    scanner_error = bool(bundled_rc or custom_rc)
    report_data = {
        "schema_version": 1,
        "finding": "B-139",
        "status": "evaluated",
        "scanner_exit_codes": {"bundled": bundled_rc, "custom": custom_rc},
        "bundled": {"results": bundled_results, "levels": bundled_counts},
        "custom": {"results": custom_results, "levels": custom_counts},
        "blocking": {
            "explicit_error_findings": error_findings,
            "scanner_error": scanner_error,
            "verdict": "FAIL" if error_findings or scanner_error else "PASS",
        },
        "sarif_upload": {
            "bundled": "pending-upload-step",
            "custom": "pending-upload-step",
            "advanced_security_claim": "unverified-until-upload-step-succeeds",
        },
        "evidence_gap": {
            "full_scan_executed": True,
            "missing_or_invalid_sarif": False,
            "nonzero_scanner_exit": bool(bundled_rc or custom_rc),
        },
    }
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(
        json.dumps(report_data, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    if bundled_rc or custom_rc:
        return 1
    return 1 if error_findings else 0


def classify_uploads(report: Path, bundled_outcome: str, custom_outcome: str) -> None:
    """Record only what the upload steps proved, preserving scan verdict."""

    report_data = _load_json(report)
    if report_data.get("status") != "evaluated":
        raise VerificationError(
            "cannot classify uploads without an evaluated scan report"
        )
    outcomes = {
        "bundled": bundled_outcome.strip().lower(),
        "custom": custom_outcome.strip().lower(),
    }
    for population, outcome in outcomes.items():
        if outcome not in UPLOAD_OUTCOMES:
            raise VerificationError(
                f"unsupported {population} upload outcome {outcome!r}"
            )
    statuses = {
        population: "uploaded" if outcome == "success" else "unavailable_or_unverified"
        for population, outcome in outcomes.items()
    }
    report_data["sarif_upload"] = {
        "bundled": statuses["bundled"],
        "custom": statuses["custom"],
        "step_outcomes": outcomes,
        "advanced_security_claim": (
            "verified"
            if all(status == "uploaded" for status in statuses.values())
            else "unverified-until-both-upload-steps-succeed"
        ),
    }
    report.write_text(
        json.dumps(report_data, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def enforce_report(report: Path) -> int:
    """Fail closed on scan errors, every ERROR result, and incomplete evidence."""

    report_data = _load_json(report)
    if report_data.get("status") != "evaluated":
        raise VerificationError("scan report is not evaluated")
    exit_codes = report_data.get("scanner_exit_codes")
    if not isinstance(exit_codes, dict) or set(exit_codes) != {"bundled", "custom"}:
        raise VerificationError("scan report has malformed scanner exit codes")
    if any(type(code) is not int or code < 0 for code in exit_codes.values()):
        raise VerificationError("scan report has invalid scanner exit code")
    error_findings = 0
    for population in ("bundled", "custom"):
        scan = report_data.get(population)
        if not isinstance(scan, dict) or type(scan.get("results")) is not int:
            raise VerificationError(
                f"scan report has malformed {population} population"
            )
        levels = scan.get("levels")
        if not isinstance(levels, dict) or set(levels) != set(LEGACY_LEVELS):
            raise VerificationError(f"scan report has malformed {population} levels")
        if any(type(count) is not int or count < 0 for count in levels.values()):
            raise VerificationError(f"scan report has invalid {population} count")
        if scan["results"] != sum(levels.values()):
            raise VerificationError(
                f"scan report {population} total does not match levels"
            )
        error_findings += levels["error"]
    blocking = report_data.get("blocking")
    if not isinstance(blocking, dict) or blocking.get("verdict") not in {
        "PASS",
        "FAIL",
    }:
        raise VerificationError("scan report has no closed blocking verdict")
    scanner_error = any(exit_codes.values())
    expected_verdict = "FAIL" if error_findings or scanner_error else "PASS"
    if blocking.get("explicit_error_findings") != error_findings:
        raise VerificationError("blocking ERROR count does not match SARIF populations")
    if blocking.get("scanner_error") is not scanner_error:
        raise VerificationError("blocking scanner status does not match exit codes")
    if blocking.get("verdict") != expected_verdict:
        raise VerificationError(
            "blocking verdict does not match ERROR/scanner evidence"
        )
    upload = report_data.get("sarif_upload")
    if not isinstance(upload, dict):
        raise VerificationError("scan report has no SARIF upload classification")
    outcomes = upload.get("step_outcomes")
    if not isinstance(outcomes, dict) or set(outcomes) != {"bundled", "custom"}:
        raise VerificationError("scan report has no closed upload step outcomes")
    for population in ("bundled", "custom"):
        outcome = outcomes.get(population)
        if outcome not in UPLOAD_OUTCOMES:
            raise VerificationError(f"{population} SARIF upload outcome is invalid")
        expected = "uploaded" if outcome == "success" else "unavailable_or_unverified"
        if upload.get(population) != expected:
            raise VerificationError(
                f"{population} SARIF upload is not honestly classified"
            )
    expected_claim = (
        "verified"
        if outcomes == {"bundled": "success", "custom": "success"}
        else "unverified-until-both-upload-steps-succeed"
    )
    if upload.get("advanced_security_claim") != expected_claim:
        raise VerificationError(
            "Advanced Security claim does not match upload outcomes"
        )
    return 0 if expected_verdict == "PASS" else 1


def mutation_self_test() -> int:
    config = _read(RULEPACK)
    workflow = _read(WORKFLOW)
    policy = _load_json(POLICY)
    bundled_lock = _load_json(BUNDLED_LOCK)
    mutations = [
        (
            "custom-rule",
            config.replace("corelink.rust.no-unwrap-in-src", "", 1),
            workflow,
            policy,
        ),
        (
            "bundle-runner",
            config,
            workflow.replace("run_b139_semgrep.py", "", 1),
            policy,
        ),
        (
            "blocking-policy",
            config,
            workflow,
            {
                **policy,
                "policy": {
                    **policy["policy"],
                    "blocking_condition": "all findings are informational",
                },
            },
        ),
        (
            "upload-truth",
            config,
            workflow.replace("unavailable_or_unverified", ""),
            policy,
        ),
        (
            "r1-severity",
            config.replace("severity: WARNING", "severity: ERROR", 1),
            workflow,
            policy,
        ),
        (
            "r3-pattern",
            config.replace(
                "pattern-inside: prop_assert!(matches!(...))",
                "pattern-inside: assert!(matches!(...))",
                1,
            ),
            workflow,
            policy,
        ),
        (
            "severity-ledger",
            config,
            workflow,
            {
                **policy,
                "scan_population": {
                    **policy["scan_population"],
                    "custom_rule_severities": {},
                },
            },
        ),
        (
            "r2-path-scope",
            config.replace(
                '        - "/crates/corelink-synthetic-pager/src/**/*.rs"\n', "", 1
            ),
            workflow,
            policy,
        ),
    ]
    passed = 0
    for name, mutated_config, mutated_workflow, mutated_policy in mutations:
        try:
            check_static(
                config=mutated_config, workflow=mutated_workflow, policy=mutated_policy
            )
        except VerificationError:
            passed += 1
        else:
            raise VerificationError(f"mutation was accepted: {name}")
    reordered_lock = {
        **bundled_lock,
        "rulesets": list(reversed(bundled_lock["rulesets"])),
    }
    try:
        check_static(bundled_lock=reordered_lock)
    except VerificationError:
        passed += 1
    else:
        raise VerificationError("mutation was accepted: bundled-lock-order")
    invalid_snapshot = {
        **bundled_lock,
        "rulesets": [
            {**bundled_lock["rulesets"][0], "snapshot": "../outside.yml.gz"},
            *bundled_lock["rulesets"][1:],
        ],
    }
    try:
        check_static(bundled_lock=invalid_snapshot)
    except VerificationError:
        passed += 1
    else:
        raise VerificationError("mutation was accepted: bundled-lock-snapshot")
    missing_provenance = {
        key: value for key, value in bundled_lock.items() if key != "provenance"
    }
    try:
        check_static(bundled_lock=missing_provenance)
    except VerificationError:
        passed += 1
    else:
        raise VerificationError("mutation was accepted: bundled-lock-provenance")
    invalid_timestamp = {**bundled_lock, "captured_at": "2026-02-30T21:20:43Z"}
    try:
        check_static(bundled_lock=invalid_timestamp)
    except VerificationError:
        passed += 1
    else:
        raise VerificationError("mutation was accepted: bundled-lock-timestamp")
    invalid_size = {
        **bundled_lock,
        "rulesets": [
            {**bundled_lock["rulesets"][0], "size_bytes": True},
            *bundled_lock["rulesets"][1:],
        ],
    }
    try:
        check_static(bundled_lock=invalid_size)
    except VerificationError:
        passed += 1
    else:
        raise VerificationError("mutation was accepted: bundled-lock-size")
    return passed


def semgrep_rule_test(semgrep_bin: Path) -> None:
    if not semgrep_bin.is_file():
        raise VerificationError(f"Semgrep executable is missing: {semgrep_bin}")
    command = [
        str(semgrep_bin),
        "--test",
        "--config",
        str(RULEPACK),
        str(RULE_FIXTURE),
        "--no-git-ignore",
    ]
    completed = subprocess.run(
        command, cwd=ROOT, check=False, text=True, capture_output=True
    )
    if completed.returncode != 0:
        detail = (completed.stdout + completed.stderr).strip()
        raise VerificationError(f"Semgrep rule controls failed: {detail}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--evaluate", action="store_true")
    parser.add_argument("--bundled-sarif", type=Path)
    parser.add_argument("--custom-sarif", type=Path)
    parser.add_argument("--report", type=Path, default=Path("semgrep-b139-report.json"))
    parser.add_argument("--bundled-rc", type=int, default=0)
    parser.add_argument("--custom-rc", type=int, default=0)
    parser.add_argument("--classify-uploads", action="store_true")
    parser.add_argument("--bundled-upload-outcome")
    parser.add_argument("--custom-upload-outcome")
    parser.add_argument("--enforce-report", action="store_true")
    parser.add_argument("--semgrep-bin", type=Path)
    args = parser.parse_args(argv)
    try:
        selected_modes = sum(
            (args.evaluate, args.classify_uploads, args.enforce_report)
        )
        if selected_modes > 1:
            raise VerificationError("select only one report operation")
        if args.classify_uploads:
            if (
                args.bundled_upload_outcome is None
                or args.custom_upload_outcome is None
            ):
                raise VerificationError(
                    "--classify-uploads requires both upload outcomes"
                )
            classify_uploads(
                args.report, args.bundled_upload_outcome, args.custom_upload_outcome
            )
            print(f"B139 SARIF upload classification: PASS; report={args.report}")
            return 0
        if args.enforce_report:
            result = enforce_report(args.report)
            print(
                f"B139 final policy: {'PASS' if result == 0 else 'FAIL'}; report={args.report}"
            )
            return result
        if args.evaluate:
            if not args.bundled_sarif or not args.custom_sarif:
                raise VerificationError("--evaluate requires both SARIF inputs")
            result = evaluate(
                args.bundled_sarif,
                args.custom_sarif,
                args.report,
                args.bundled_rc,
                args.custom_rc,
            )
            print(
                f"B139 SARIF policy: {'PASS' if result == 0 else 'FAIL'}; report={args.report}"
            )
            return result
        result = check_static()
        mutations = mutation_self_test() if args.self_test else 0
        if args.semgrep_bin:
            semgrep_rule_test(args.semgrep_bin)
        suffix = f"; mutations={mutations}/13" if args.self_test else ""
        if args.semgrep_bin:
            suffix += "; semgrep-controls=4/4"
        print(
            f"B139 static policy: PASS; custom={result['custom_rules']} bundled={result['bundled_rulesets']}{suffix}"
        )
        return 0
    except (OSError, VerificationError) as exc:
        print(f"B139 FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
