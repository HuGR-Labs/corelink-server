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
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/semgrep.yml"
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
    "corelink.rust.no-unwrap-in-src": ("WARNING", "advisory-duplicate-of-clippy"),
    "corelink.rust.no-tokio-in-lib-crates": ("ERROR", "explicit-error"),
    "corelink.rust.prop-assert-matches-struct-variant": ("ERROR", "explicit-error"),
    "corelink.rust.no-expect-in-byok-src": ("WARNING", "advisory-duplicate-of-clippy"),
}
SUPPRESSION_RULE = "yaml.github-actions.security.pull-request-target-code-checkout.pull-request-target-code-checkout"
_RULE_ROOT = "%B139_ROOT_2%"
_URI_ROOT = "%B139_ROOT_3%"
LEGACY_LEVELS = ("error", "warning", "note", "none")
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
        blocks[rule_id] = config[match.start():end]
    return blocks


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
    if population.get("custom_config") != "./semgrep.yml" or population.get("custom_rule_count") != len(CUSTOM):
        raise VerificationError("custom rule population changed")
    if population.get("custom_rule_prefix") != "corelink.":
        raise VerificationError("custom rule prefix changed")
    expected_severities = {rule_id: values[0] for rule_id, values in CUSTOM_POLICY.items()}
    if population.get("custom_rule_severities") != expected_severities:
        raise VerificationError("custom rule severity ledger changed")
    rule_policy = policy.get("policy")
    if not isinstance(rule_policy, dict):
        raise VerificationError("policy section must be an object")
    if rule_policy.get("fail_closed") is not True:
        raise VerificationError("policy must remain fail closed")
    if rule_policy.get("semgrep_error_flag") != "not used; the evaluator applies the explicit SARIF ERROR policy and preserves nonzero scanner failures":
        raise VerificationError("policy must name the evaluator rather than silently dropping --error")
    if "only when its SARIF level is error" not in str(rule_policy.get("blocking_condition", "")):
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
    if "unavailable_or_unverified" not in str(sarif.get("advanced_security_unavailable", "")):
        raise VerificationError("SARIF upload limitation is not explicit")
    suppression = rule_policy.get("suppression")
    if not isinstance(suppression, str) or "approved_suppression_sites" not in suppression:
        raise VerificationError("suppression policy does not require the guard")


def check_static(
    *,
    config: str | None = None,
    workflow: str | None = None,
    policy: dict[str, Any] | None = None,
) -> dict[str, int]:
    config = _read(RULEPACK) if config is None else config
    workflow = _read(WORKFLOW) if workflow is None else workflow
    policy = _load_json(POLICY) if policy is None else policy
    _check_policy(policy)

    blocks = _rule_blocks(config)
    if set(blocks) != set(CUSTOM):
        raise VerificationError("custom rule population is not exactly the four CoreLink rules")
    for rule_id in CUSTOM:
        block = blocks[rule_id]
        severity, policy = CUSTOM_POLICY[rule_id]
        _require(block, f"severity: {severity}", f"{rule_id} severity")
        _require(block, f"policy: {policy}", f"{rule_id} policy metadata")
        _require(block, "source: corelink-custom", f"{rule_id} source metadata")

    for ruleset in BUNDLED:
        _require(workflow, f"--config {ruleset}", "bundled scan")
    _require(workflow, "--config ./semgrep.yml", "custom scan")
    _require(workflow, "semgrep-bundled.sarif", "bundled SARIF")
    _require(workflow, "semgrep-custom.sarif", "custom SARIF")
    _require(workflow, "verify_b139_semgrep.py --evaluate", "explicit SARIF evaluator")
    _require(workflow, "--bundled-rc", "bundled scanner status")
    _require(workflow, "--custom-rc", "custom scanner status")
    _require(workflow, "continue-on-error: true", "GHAS upload boundary")
    _require(workflow, "unavailable_or_unverified", "truthful upload classification")
    _require(workflow, "actions/upload-artifact@", "retained SARIF artifact")
    if re.search(r"(?m)^\s+--error\s*$", workflow):
        raise VerificationError("workflow still delegates policy to Semgrep --error")
    if 'exit "${SEMGREP_RC}"' in workflow:
        raise VerificationError("scanner step exits before the explicit evaluator")
    return {"custom_rules": len(CUSTOM), "bundled_rulesets": len(BUNDLED)}


def _approved_sites() -> set[tuple[str, int, str]]:
    """Run the PR-target guard and return its tree-derived suppression allowlist."""
    try:
        from verify_b139_prtarget_data_boundary import approved_suppression_sites
    except (ImportError, ModuleNotFoundError) as exc:
        raise VerificationError("PR-target suppression guard is unavailable") from exc
    try:
        sites = approved_suppression_sites(ROOT)
    except Exception as exc:  # guard failures are fail-closed, regardless of guard exception type
        raise VerificationError(f"PR-target suppression guard failed: {exc}") from exc
    if not isinstance(sites, set) or len(sites) != 9 or any(
        not isinstance(site, tuple)
        or len(site) != 3
        or not isinstance(site[0], str)
        or not isinstance(site[1], int)
        or isinstance(site[1], bool)
        or site[1] < 1
        or site[2] != SUPPRESSION_RULE
        for site in sites
    ):
        raise VerificationError("PR-target suppression guard returned malformed sites")
    return sites


def _normalized_rule(rule_id: str) -> str:
    root_prefix = str(ROOT).replace("\\", "/") + "."
    if rule_id.startswith(_RULE_ROOT + "."):
        return rule_id[len(_RULE_ROOT) + 1 :]
    if rule_id.startswith(root_prefix):
        return rule_id[len(root_prefix) :]
    raise VerificationError(f"SARIF ruleId has unsupported prefix: {rule_id!r}")


def _suppression_site(result: dict[str, Any], path: Path) -> tuple[str, int, str]:
    if result.get("suppressions") != [{"kind": "inSource"}]:
        raise VerificationError(f"{path} has unsupported or malformed suppression")
    rule = _normalized_rule(result["ruleId"])
    if rule != SUPPRESSION_RULE:
        raise VerificationError(f"{path} has suppression for unsupported rule {rule!r}")
    locations = result.get("locations")
    if not isinstance(locations, list) or len(locations) != 1 or not isinstance(locations[0], dict):
        raise VerificationError(f"{path} has malformed suppressed location")
    physical = locations[0].get("physicalLocation")
    if not isinstance(physical, dict):
        raise VerificationError(f"{path} has malformed suppressed physical location")
    artifact = physical.get("artifactLocation")
    region = physical.get("region")
    if not isinstance(artifact, dict) or not isinstance(region, dict) or not isinstance(artifact.get("uri"), str):
        raise VerificationError(f"{path} has malformed suppressed location fields")
    uri = artifact["uri"].replace("\\", "/")
    root_prefix = str(ROOT).replace("\\", "/").rstrip("/") + "/"
    if uri.startswith(_URI_ROOT + "/"):
        uri = uri[len(_URI_ROOT) + 1 :]
    elif uri.startswith(root_prefix):
        uri = uri[len(root_prefix) :]
    elif uri.startswith("/") or uri.startswith("../") or "/../" in uri:
        raise VerificationError(f"{path} has non-repository suppression URI")
    line = region.get("startLine")
    if not isinstance(line, int) or isinstance(line, bool) or line < 1:
        raise VerificationError(f"{path} has malformed suppressed line")
    return (uri, line, rule)


def _sarif_counts(path: Path, *, custom: bool, approved: set[tuple[str, int, str]]) -> tuple[dict[str, int], int, int, set[tuple[str, int, str]]]:
    data = _load_json(path)
    runs = data.get("runs")
    if not isinstance(runs, list) or not runs:
        raise VerificationError(f"{path} has no SARIF runs")
    counts = {level: 0 for level in LEGACY_LEVELS}
    results_seen = 0
    suppressed_seen = 0
    suppressed_sites: set[tuple[str, int, str]] = set()
    for run in runs:
        if not isinstance(run, dict) or not isinstance(run.get("results"), list):
            raise VerificationError(f"{path} has malformed SARIF run")
        for result in run["results"]:
            if not isinstance(result, dict) or not isinstance(result.get("ruleId"), str):
                raise VerificationError(f"{path} has a result without ruleId")
            is_custom = result["ruleId"].startswith("corelink.")
            if is_custom != custom:
                expected = "custom CoreLink" if custom else "bundled"
                raise VerificationError(f"{path} mixes a non-{expected} rule result")
            level = result.get("level", "error")
            if level not in counts:
                raise VerificationError(f"{path} has unsupported SARIF level {level!r}")
            counts[level] += 1
            results_seen += 1
            if result.get("suppressions") is not None:
                site = _suppression_site(result, path)
                if site not in approved:
                    raise VerificationError(f"{path} contains unapproved suppression site {site!r}")
                suppressed_seen += 1
                suppressed_sites.add(site)
    return counts, results_seen, suppressed_seen, suppressed_sites


def evaluate(
    bundled_sarif: Path,
    custom_sarif: Path,
    report: Path,
    bundled_rc: int,
    custom_rc: int,
) -> int:
    approved = _approved_sites()
    bundled_counts, bundled_results, bundled_suppressed, bundled_sites = _sarif_counts(bundled_sarif, custom=False, approved=approved)
    custom_counts, custom_results, custom_suppressed, custom_sites = _sarif_counts(custom_sarif, custom=True, approved=approved)
    if bundled_suppressed + custom_suppressed != len(approved) or bundled_sites | custom_sites != approved:
        raise VerificationError("suppressed SARIF sites do not exactly match approved tree sites")
    error_findings = bundled_counts["error"] + custom_counts["error"]
    unsuppressed_errors = error_findings - bundled_suppressed - custom_suppressed
    scanner_error = bool(bundled_rc or custom_rc)
    report_data = {
        "schema_version": 1,
        "finding": "B-139",
        "status": "evaluated",
        "captured_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "scanner_exit_codes": {"bundled": bundled_rc, "custom": custom_rc},
        "bundled": {"results": bundled_results, "levels": bundled_counts, "approved_suppressed": bundled_suppressed},
        "custom": {"results": custom_results, "levels": custom_counts, "approved_suppressed": custom_suppressed},
        "blocking": {
            "explicit_error_findings": error_findings,
            "unsuppressed_error_findings": unsuppressed_errors,
            "scanner_error": scanner_error,
            "verdict": "FAIL" if unsuppressed_errors or scanner_error else "PASS",
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
    report.write_text(json.dumps(report_data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if bundled_rc or custom_rc:
        return 1
    return 1 if unsuppressed_errors else 0


def mutation_self_test() -> int:
    config = _read(RULEPACK)
    workflow = _read(WORKFLOW)
    policy = _load_json(POLICY)
    r4_start = config.index("corelink.rust.no-expect-in-byok-src")
    r4_severity = (
        config[:r4_start]
        + config[r4_start:].replace("severity: WARNING", "severity: ERROR", 1)
    )
    mutations = [
        ("custom-rule", config.replace("corelink.rust.no-unwrap-in-src", "", 1), workflow, policy),
        ("r1-severity", config.replace("severity: WARNING", "severity: ERROR", 1), workflow, policy),
        ("r4-severity", r4_severity, workflow, policy),
        ("explicit-evaluator", config, workflow.replace("verify_b139_semgrep.py --evaluate", "", 1), policy),
        ("blocking-policy", config, workflow, {**policy, "policy": {**policy["policy"], "blocking_condition": "all findings are informational"}}),
        ("upload-truth", config, workflow.replace("unavailable_or_unverified", ""), policy),
    ]
    passed = 0
    for name, mutated_config, mutated_workflow, mutated_policy in mutations:
        try:
            check_static(config=mutated_config, workflow=mutated_workflow, policy=mutated_policy)
        except VerificationError:
            passed += 1
        else:
            raise VerificationError(f"mutation was accepted: {name}")
    return passed


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--evaluate", action="store_true")
    parser.add_argument("--bundled-sarif", type=Path)
    parser.add_argument("--custom-sarif", type=Path)
    parser.add_argument("--report", type=Path, default=Path("semgrep-b139-report.json"))
    parser.add_argument("--bundled-rc", type=int, default=0)
    parser.add_argument("--custom-rc", type=int, default=0)
    args = parser.parse_args(argv)
    try:
        if args.evaluate:
            if not args.bundled_sarif or not args.custom_sarif:
                raise VerificationError("--evaluate requires both SARIF inputs")
            result = evaluate(args.bundled_sarif, args.custom_sarif, args.report, args.bundled_rc, args.custom_rc)
            print(f"B139 SARIF policy: {'PASS' if result == 0 else 'FAIL'}; report={args.report}")
            return result
        result = check_static()
        mutations = mutation_self_test() if args.self_test else 0
        suffix = f"; mutations={mutations}" if args.self_test else ""
        print(f"B139 static policy: PASS; custom={result['custom_rules']} bundled={result['bundled_rulesets']}{suffix}")
        return 0
    except (OSError, VerificationError) as exc:
        print(f"B139 FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
