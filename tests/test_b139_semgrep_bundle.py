"""Focused mutation tests for the B-139 SARIF suppression boundary."""
import importlib.util
import json
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("b139", ROOT / "scripts/verify_b139_semgrep.py")
b139 = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(b139)

SITES = {(f".github/workflows/w{i}.yml", i, b139.SUPPRESSION_RULE) for i in range(1, 10)}


def _result(site, *, suppressed=True, rule=None, uri=None, line=None):
    path, number, expected_rule = site
    result = {
        "ruleId": f"%B139_ROOT_2%.{rule or expected_rule}",
        "locations": [{"physicalLocation": {"artifactLocation": {"uri": uri or f"%B139_ROOT_3%/{path}"}, "region": {"startLine": line or number}}}],
    }
    if suppressed:
        result["suppressions"] = [{"kind": "inSource"}]
    return result


def _sarif(results):
    return {"version": "2.1.0", "runs": [{"results": results}]}


def _write(tmp_path, results, *, custom=None, report=None, bundled_rc=0, custom_rc=0):
    bundled = tmp_path / "bundled.sarif"
    custom_path = tmp_path / "custom.sarif"
    bundled.write_text(json.dumps(_sarif(results)), encoding="utf-8")
    custom_path.write_text(json.dumps(_sarif(custom or [])), encoding="utf-8")
    out = tmp_path / "report.json"
    with pytest.MonkeyPatch.context() as mp:
        mp.setattr(b139, "ROOT", tmp_path)
        # The real guard is supplied by its sibling WP; this test-local stub keeps this suite independent.
        guard = type("Guard", (), {"approved_suppression_sites": staticmethod(lambda _: SITES)})()
        mp.setitem(sys.modules, "verify_b139_prtarget_data_boundary", guard)
        rc = b139.evaluate(bundled, custom_path, out, bundled_rc, custom_rc)
    return rc, json.loads(out.read_text(encoding="utf-8"))


def test_nine_approved_in_source_errors_are_retained_but_nonblocking(tmp_path):
    rc, report = _write(tmp_path, [_result(site) for site in sorted(SITES)])
    assert rc == 0
    assert report["bundled"]["levels"]["error"] == 9
    assert report["bundled"]["approved_suppressed"] == 9
    assert report["blocking"]["explicit_error_findings"] == 9


@pytest.mark.parametrize("mutation", ["wrong-rule", "wrong-site", "duplicate", "missing", "custom", "arbitrary"])
def test_suppression_mutations_fail_closed(tmp_path, mutation):
    sites = sorted(SITES)
    results = [_result(site) for site in sites]
    if mutation == "wrong-rule":
        results[0]["ruleId"] = "%B139_ROOT_2%.other-rule"
    elif mutation == "wrong-site":
        results[0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"] = "%B139_ROOT_3%/.github/workflows/nope.yml"
    elif mutation == "duplicate":
        results[-1] = results[0]
    elif mutation == "missing":
        results.pop()
    elif mutation == "custom":
        results[0]["ruleId"] = "%B139_ROOT_2%.corelink.custom"
    else:
        results[0]["suppressions"] = [{"kind": "external"}]
    with pytest.raises(b139.VerificationError):
        _write(tmp_path, results)


def test_unsuppressed_error_blocks(tmp_path):
    rc, report = _write(tmp_path, [_result(site) for site in sorted(SITES)] + [_result(sorted(SITES)[0], suppressed=False)])
    assert rc == 1
    assert report["blocking"]["verdict"] == "FAIL"


def test_scanner_error_blocks(tmp_path):
    rc, report = _write(tmp_path, [_result(site) for site in sorted(SITES)], bundled_rc=2)
    assert rc == 1
    assert report["blocking"]["scanner_error"] is True


def test_missing_guard_fails_closed(tmp_path, monkeypatch):
    monkeypatch.setattr(b139, "ROOT", tmp_path)
    monkeypatch.setitem(sys.modules, "verify_b139_prtarget_data_boundary", None)
    bundled = tmp_path / "b.sarif"; custom = tmp_path / "c.sarif"
    bundled.write_text(json.dumps(_sarif([]))); custom.write_text(json.dumps(_sarif([])))
    with pytest.raises(b139.VerificationError):
        b139.evaluate(bundled, custom, tmp_path / "r.json", 0, 0)
