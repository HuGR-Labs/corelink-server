#!/usr/bin/env python3
"""Fail a CodeQL run when SARIF contains HIGH-or-higher security findings."""

from __future__ import annotations

import json
import math
import os
import sys
from pathlib import Path
from typing import Any


THRESHOLD = 7.0


class SarifSeverityError(ValueError):
    """SARIF evidence cannot be classified safely."""


def _as_rule_list(component: dict[str, Any], label: str) -> list[dict[str, Any]]:
    rules = component.get("rules", [])
    if not isinstance(rules, list):
        raise SarifSeverityError(f"{label}.rules must be an array")
    if any(not isinstance(rule, dict) for rule in rules):
        raise SarifSeverityError(f"{label}.rules contains a non-object entry")
    seen: set[str] = set()
    for rule in rules:
        rule_id = rule.get("id")
        if not isinstance(rule_id, str) or not rule_id:
            raise SarifSeverityError(f"{label}.rules contains a missing rule id")
        if rule_id in seen:
            raise SarifSeverityError(f"{label}.rules contains duplicate rule id {rule_id!r}")
        seen.add(rule_id)
    return rules


def _rule_for_result(
    result: dict[str, Any],
    driver_name: str | None,
    driver_guid: str | None,
    driver_rules: list[dict[str, Any]],
    extensions: list[tuple[str | None, str | None, list[dict[str, Any]]]],
) -> dict[str, Any]:
    rule_id = result.get("ruleId")
    if not isinstance(rule_id, str) or not rule_id:
        raise SarifSeverityError("result has no ruleId")

    rule_ref = result.get("rule", {})
    if not isinstance(rule_ref, dict):
        raise SarifSeverityError(f"result {rule_id!r} has a malformed rule reference")
    ref_id = rule_ref.get("id")
    if ref_id is not None and ref_id != rule_id:
        raise SarifSeverityError(f"result ruleId {rule_id!r} conflicts with its rule reference {ref_id!r}")

    component_ref = rule_ref.get("toolComponent", result.get("toolComponent", {}))
    if not isinstance(component_ref, dict):
        raise SarifSeverityError(f"result {rule_id!r} has a malformed toolComponent reference")
    component_index = component_ref.get("index")
    component_name = component_ref.get("name")
    if component_name is not None and (not isinstance(component_name, str) or not component_name):
        raise SarifSeverityError(f"result {rule_id!r} has an invalid toolComponent name")
    component_guid = component_ref.get("guid")
    if component_guid is not None and (not isinstance(component_guid, str) or not component_guid):
        raise SarifSeverityError(f"result {rule_id!r} has an invalid toolComponent guid")
    raw_rule_index = result.get("ruleIndex", rule_ref.get("index"))
    rule_index: int | None = None
    if raw_rule_index is not None:
        if isinstance(raw_rule_index, bool) or not isinstance(raw_rule_index, int) or raw_rule_index < 0:
            raise SarifSeverityError(f"result {rule_id!r} has an invalid rule index")
        rule_index = raw_rule_index

    components: list[tuple[str, str | None, str | None, list[dict[str, Any]]]] = [
        ("driver", driver_name, driver_guid, driver_rules)
    ]
    components.extend(
        (f"extension[{index}]", name, guid, rules)
        for index, (name, guid, rules) in enumerate(extensions)
    )

    if component_index is not None:
        if isinstance(component_index, bool) or not isinstance(component_index, int) or component_index < 0:
            raise SarifSeverityError(f"result {rule_id!r} has an invalid toolComponent index")
        if component_index >= len(extensions):
            raise SarifSeverityError(f"result {rule_id!r} references missing extension[{component_index}]")
        candidates = [components[component_index + 1]]
    elif component_guid is not None:
        candidates = [component for component in components if component[2] == component_guid]
        if not candidates:
            raise SarifSeverityError(
                f"result {rule_id!r} references unknown toolComponent guid {component_guid!r}"
            )
        if len(candidates) > 1:
            raise SarifSeverityError(
                f"result {rule_id!r} references ambiguous toolComponent guid {component_guid!r}"
            )
    else:
        # SARIF 2.1 defaults an unqualified toolComponent reference to the driver.
        # The name is a consistency check, never an alternate lookup key.
        candidates = [components[0]]

    resolved = candidates[0]
    if component_name is not None and component_name != resolved[1]:
        raise SarifSeverityError(
            f"result {rule_id!r} toolComponent name {component_name!r} "
            f"does not match {resolved[0]} name {resolved[1]!r}"
        )
    if component_guid is not None and component_guid != resolved[2]:
        raise SarifSeverityError(
            f"result {rule_id!r} toolComponent guid {component_guid!r} "
            f"does not match {resolved[0]} guid {resolved[2]!r}"
        )

    matches: list[dict[str, Any]] = []
    rules = resolved[3]
    if rule_index is not None:
        if rule_index < len(rules) and rules[rule_index].get("id") == rule_id:
            matches.append(rules[rule_index])
    else:
        matches.extend(rule for rule in rules if rule.get("id") == rule_id)

    if len(matches) != 1:
        reason = "unresolved" if not matches else "ambiguous"
        raise SarifSeverityError(f"{reason} SARIF metadata for result rule {rule_id!r}")
    return matches[0]


def _security_score(rule: dict[str, Any], rule_id: str) -> float:
    properties = rule.get("properties", {})
    if not isinstance(properties, dict):
        raise SarifSeverityError(f"rule {rule_id!r} has malformed properties")

    raw_score = properties.get("security-severity")
    tags = properties.get("tags")
    if not isinstance(tags, list) or any(not isinstance(tag, str) for tag in tags):
        raise SarifSeverityError(f"rule {rule_id!r} has no valid tags for severity classification")
    if raw_score is None:
        if "security" in tags:
            raise SarifSeverityError(f"security rule {rule_id!r} has no security-severity")
        return 0.0

    if isinstance(raw_score, bool):
        raise SarifSeverityError(f"rule {rule_id!r} has invalid security-severity {raw_score!r}")
    try:
        score = float(raw_score)
    except (TypeError, ValueError) as exc:
        raise SarifSeverityError(f"rule {rule_id!r} has invalid security-severity {raw_score!r}") from exc
    if not math.isfinite(score) or not 0.0 <= score <= 10.0:
        raise SarifSeverityError(f"rule {rule_id!r} has out-of-range security-severity {raw_score!r}")
    return score


def high_findings(sarif: dict[str, Any]) -> list[dict[str, Any]]:
    """Return all findings at or above the HIGH threshold or reject unclear data."""
    runs = sarif.get("runs")
    if not isinstance(runs, list) or not runs:
        raise SarifSeverityError("SARIF must contain at least one run")

    findings: list[dict[str, Any]] = []
    for run_number, run in enumerate(runs):
        if not isinstance(run, dict):
            raise SarifSeverityError(f"run {run_number} is not an object")
        tool = run.get("tool")
        if not isinstance(tool, dict) or not isinstance(tool.get("driver"), dict):
            raise SarifSeverityError(f"run {run_number} has no tool driver")
        driver_rules = _as_rule_list(tool["driver"], f"run {run_number} driver")
        driver_name = tool["driver"].get("name")
        if driver_name is not None and (not isinstance(driver_name, str) or not driver_name):
            raise SarifSeverityError(f"run {run_number} driver has an invalid name")
        driver_guid = tool["driver"].get("guid")
        if driver_guid is not None and (not isinstance(driver_guid, str) or not driver_guid):
            raise SarifSeverityError(f"run {run_number} driver has an invalid guid")
        raw_extensions = tool.get("extensions", [])
        if not isinstance(raw_extensions, list) or any(not isinstance(ext, dict) for ext in raw_extensions):
            raise SarifSeverityError(f"run {run_number} tool.extensions must be an array of objects")
        extensions: list[tuple[str | None, list[dict[str, Any]]]] = []
        for index, extension in enumerate(raw_extensions):
            extension_name = extension.get("name")
            if extension_name is not None and (
                not isinstance(extension_name, str) or not extension_name
            ):
                raise SarifSeverityError(
                    f"run {run_number} extension[{index}] has an invalid name"
                )
            extension_guid = extension.get("guid")
            if extension_guid is not None and (
                not isinstance(extension_guid, str) or not extension_guid
            ):
                raise SarifSeverityError(
                    f"run {run_number} extension[{index}] has an invalid guid"
                )
            rules = _as_rule_list(extension, f"run {run_number} extension[{index}]")
            extensions.append((extension_name, extension_guid, rules))
        results = run.get("results", [])
        if not isinstance(results, list) or any(not isinstance(result, dict) for result in results):
            raise SarifSeverityError(f"run {run_number} results must be an array of objects")

        tool_name = tool["driver"].get("name", "CodeQL")
        for result_number, result in enumerate(results):
            rule_id = result.get("ruleId")
            try:
                rule = _rule_for_result(result, driver_name, driver_guid, driver_rules, extensions)
                score = _security_score(rule, rule_id if isinstance(rule_id, str) else "unknown")
            except SarifSeverityError as exc:
                raise SarifSeverityError(f"run {run_number}, result {result_number}: {exc}") from exc
            if score < THRESHOLD:
                continue

            location = "unknown"
            locations = result.get("locations", [])
            if locations:
                physical = locations[0].get("physicalLocation", {})
                uri = physical.get("artifactLocation", {}).get("uri", "?")
                line = physical.get("region", {}).get("startLine", "?")
                location = f"{uri}:{line}"
            message = result.get("message", {}).get("text", "")
            findings.append({
                "tool": tool_name,
                "rule": rule_id,
                "severity": score,
                "location": location,
                "message": message,
            })
    return findings


def main(paths: list[str]) -> int:
    if not paths:
        print("::error::No SARIF files supplied to CodeQL severity gate.")
        return 2

    findings: list[dict[str, Any]] = []
    try:
        for path in paths:
            sarif = json.loads(Path(path).read_text(encoding="utf-8"))
            if not isinstance(sarif, dict):
                raise SarifSeverityError(f"{path}: SARIF document must be an object")
            findings.extend(high_findings(sarif))
    except (OSError, json.JSONDecodeError, SarifSeverityError) as exc:
        print(f"::error::CodeQL severity gate cannot classify SARIF: {exc}")
        return 2

    summary = os.environ.get("GITHUB_STEP_SUMMARY", os.devnull)
    with open(summary, "a", encoding="utf-8") as output:
        output.write("## CodeQL severity gate\n\n")
        output.write(f"- HIGH+CRITICAL findings: **{len(findings)}**\n\n")
        for finding in findings:
            output.write(
                f"- `{finding['rule']}` (sev={finding['severity']}) at `"
                f"{finding['location']}` — {finding['message']}\n"
            )

    if findings:
        print(f"::error::CodeQL gate: {len(findings)} HIGH/CRITICAL findings")
        for finding in findings:
            print(
                f"::error file={finding['location']}::{finding['rule']} "
                f"(sev={finding['severity']}): {finding['message']}"
            )
        return 1
    print("CodeQL gate: clean (no HIGH/CRITICAL findings).")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
