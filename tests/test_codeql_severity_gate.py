from __future__ import annotations

import sys
from pathlib import Path
from typing import Any

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from codeql_severity_gate import SarifSeverityError, high_findings  # noqa: E402


def _rule(rule_id: str, score: str | float | None, *, security: bool = True) -> dict[str, Any]:
    tags = ["security"] if security else ["quality", "maintainability"]
    properties: dict[str, Any] = {"tags": tags}
    if score is not None:
        properties["security-severity"] = score
    return {"id": rule_id, "properties": properties}


def _sarif(
    rule: dict[str, Any],
    *,
    component_index: int | None,
    result_rule_id: str | None = None,
    component: str = "extension",
) -> dict[str, Any]:
    rule_id = rule["id"]
    driver_rules = [rule] if component == "driver" else []
    extensions = [] if component == "driver" else [{"name": "codeql/language-queries", "rules": [rule]}]
    result_id = result_rule_id or rule_id
    rule_ref: dict[str, Any] = {"id": result_id, "index": 0}
    if component_index is not None:
        rule_ref["toolComponent"] = {"index": component_index}
    return {
        "version": "2.1.0",
        "runs": [
            {
                "tool": {"driver": {"name": "CodeQL", "rules": driver_rules}, "extensions": extensions},
                "results": [{"ruleId": result_id, "rule": rule_ref, "message": {"text": "synthetic finding"}, "locations": []}],
            }
        ],
    }


@pytest.mark.parametrize(
    ("language", "rule_id", "severity"),
    [
        ("rust", "rust/cleartext-logging", "9.8"),
        ("javascript-typescript", "js/file-system-race", "7.7"),
        ("python", "py/command-line-injection", "7.5"),
    ],
)
def test_extension_rules_fail_gate_for_each_codeql_language(
    language: str, rule_id: str, severity: str
) -> None:
    # CodeQL query metadata is in extension[0], as in the retained SARIF.
    fixture = _sarif(_rule(rule_id, severity), component_index=0)

    findings = high_findings(fixture)

    assert len(findings) == 1, language
    assert findings[0]["rule"] == rule_id
    assert findings[0]["severity"] == float(severity)


def test_driver_rule_metadata_is_supported() -> None:
    fixture = _sarif(_rule("custom/high", "8.1"), component_index=None, component="driver")

    findings = high_findings(fixture)

    assert [finding["rule"] for finding in findings] == ["custom/high"]


def test_non_security_rules_without_security_score_are_classified_as_below_threshold() -> None:
    fixture = _sarif(
        _rule("py/unused-import", None, security=False),
        component_index=0,
    )

    assert high_findings(fixture) == []


def test_threshold_boundary_is_inclusive_at_high() -> None:
    for score, expected in [("6.9", 0), (6.99, 0), ("7.0", 1), (9.0, 1)]:
        fixture = _sarif(_rule("codeql/threshold", score), component_index=0)

        assert len(high_findings(fixture)) == expected, score


def test_unresolved_result_rule_fails_closed() -> None:
    fixture = _sarif(_rule("js/known", "8.0"), component_index=0, result_rule_id="js/missing")

    with pytest.raises(SarifSeverityError, match="unresolved SARIF metadata"):
        high_findings(fixture)


def test_unqualified_duplicate_rule_id_fails_as_ambiguous() -> None:
    fixture = _sarif(_rule("rust/high", "8.0"), component_index=None)
    run = fixture["runs"][0]
    run["tool"]["driver"]["rules"].append(_rule("rust/high", "8.0"))
    run["results"][0]["rule"].pop("index")

    with pytest.raises(SarifSeverityError, match="ambiguous SARIF metadata"):
        high_findings(fixture)


def test_missing_extension_reference_fails_closed() -> None:
    fixture = _sarif(_rule("js/high", "8.0"), component_index=1)

    with pytest.raises(SarifSeverityError, match="references missing extension"):
        high_findings(fixture)


def test_missing_security_severity_fails_closed_for_security_rules() -> None:
    fixture = _sarif(_rule("py/security-rule", None), component_index=0)

    with pytest.raises(SarifSeverityError, match="no security-severity"):
        high_findings(fixture)


def test_invalid_security_severity_fails_closed() -> None:
    for score in ["NaN", "inf", "10.1", "unknown"]:
        fixture = _sarif(_rule("rust/bad-score", score), component_index=0)

        with pytest.raises(SarifSeverityError, match="invalid|out-of-range"):
            high_findings(fixture)
