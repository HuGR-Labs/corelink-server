#!/usr/bin/env python3
"""Verify the closed B-255 customer cross-tenant audit contract.

The gate checks the machine-readable registry/TLA+ wiring and the executable
handler/test surfaces.  TLC itself is run by ``run_tla_suite.sh``; keeping this
script independent of a Java toolchain lets the backlog verifier run in the
small Python-only contract lane as well.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "specs/03_architecture/invariant_registry.md"
TLA = ROOT / "specs/tla/tenant_isolation.tla"
HANDLER = ROOT / "crates/corelink-handler-customer/src/handler.rs"
HANDLER_LIB = ROOT / "crates/corelink-handler-customer/src/lib.rs"
D1_HANDLER = ROOT / "crates/corelink-container/src/customer_d1_handler_state.rs"
D1_MODULE = ROOT / "crates/corelink-container/src/customer_d1.rs"
HANDLER_TESTS = ROOT / "crates/corelink-handler-customer/tests/handler_customer.rs"
MUTATION_TESTS = ROOT / "crates/corelink-handler-customer/tests/mutation_kills.rs"
D1_TESTS = ROOT / "crates/corelink-container/src/customer_d1_tests_cross_tenant.rs"
CONSISTENCY = ROOT / "scripts/validate_canonical_consistency.py"
INV_ID = "INV-CROSS-TENANT-DENIED"
SPEC = ROOT / "specs/tla/cross_tenant_handler_audit.tla"
CFG = ROOT / "specs/tla/cross_tenant_handler_audit.cfg"
EVIDENCE = ROOT / "specs/_audits/sealed/2026-09-06-b255-cross-tenant-tlc.md"
ADR = ROOT / "specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md"
PINNED_TLC_SHA256 = "eabd140a70f49eb9305a3bd3f3df944eddf87e5a90d329789085f8953a80533a"
EXPECTED_INVARIANTS = [
    "InvAuditBeforeCrossTenantDenied",
    "InvCrossTenantReturnHasDistinctTenants",
    "InvAllSixGroupsCovered",
    "InvAuditFailureNeverReturnsCrossTenantDenied",
]


def _fail(message: str, code: int = 2) -> int:
    print(f"INDETERMINATE: {message}", file=sys.stderr)
    return code


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _evidence_json(text: str) -> dict[str, object] | None:
    match = re.search(r"```json\s*(\{.*?\})\s*```", text, re.DOTALL)
    if match is None:
        return None
    try:
        parsed = json.loads(match.group(1))
    except json.JSONDecodeError:
        return None
    return parsed if isinstance(parsed, dict) else None


def _validate_tlc_evidence(evidence_text: str) -> str | None:
    evidence = _evidence_json(evidence_text)
    if evidence is None:
        return "TLC evidence has no parseable JSON record"
    if evidence.get("schema") != "corelink.tla-verification-evidence.v1":
        return "TLC evidence schema is not the bounded v1 record"
    if evidence.get("invariant") != INV_ID:
        return "TLC evidence is bound to the wrong invariant"

    model = evidence.get("model")
    config = evidence.get("config")
    tool = evidence.get("tool")
    bounds = evidence.get("bounds")
    result = evidence.get("result")
    command = evidence.get("command")
    if not all(isinstance(value, dict) for value in (model, config, tool, bounds, result)):
        return "TLC evidence has incomplete model/config/tool/bounds/result objects"
    if not isinstance(command, str):
        return "TLC evidence has no canonical runner command"

    expected_command = (
        "TLC_JAR=/path/to/tla2tools.jar bash scripts/run_tla_suite.sh "
        "--only cross_tenant_handler_audit --timeout 60 --workers 1"
    )
    if command != expected_command:
        return "TLC evidence command does not match the canonical bounded runner"
    for obj, path, label in ((model, SPEC, "model"), (config, CFG, "config")):
        if obj.get("path") != str(path.relative_to(ROOT)):
            return f"TLC evidence {label} path drifted"
        if obj.get("sha256") != _sha256(path):
            return f"TLC evidence {label} SHA-256 does not match the checked-in file"

    current_pin = re.search(
        r"\*\*Current pinned values\*\*.*?- SHA-256:\s*`([0-9a-f]{64})`",
        ADR.read_text(encoding="utf-8"),
        re.DOTALL,
    )
    if current_pin is None or current_pin.group(1) != PINNED_TLC_SHA256:
        return "ADR-0042 current TLC pin is absent or drifted"
    if tool.get("name") != "tla2tools.jar" or tool.get("version") != "1.8.0":
        return "TLC evidence tool identity is not the canonical v1.8.0 jar"
    if tool.get("sha256") != PINNED_TLC_SHA256 or tool.get("size_bytes") != 4487757:
        return "TLC evidence tool digest/size does not match ADR-0042 §A1"
    if tool.get("pin_source") != "specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md §A1":
        return "TLC evidence does not identify ADR-0042 §A1 as its pin source"

    if bounds != {
        "Tenants": ["tenant_a", "tenant_b"],
        "Groups": ["overview", "usage", "billing", "keys", "team", "audit_query"],
        "MaxOps": 6,
    }:
        return "TLC evidence bounds do not match the checked-in config"
    if result.get("status") != "PASS":
        return "TLC evidence does not record a PASS result"
    if result.get("states_generated") != 134 or result.get("distinct_states") != 134 or result.get("depth") != 7:
        return "TLC evidence state/depth result is not the reviewed 134/134/depth-7 run"
    if result.get("invariants") != EXPECTED_INVARIANTS:
        return "TLC evidence invariant set does not match the cfg gate"

    # The artifact is useful without a JVM in the backlog lane, but whenever a
    # caller supplies the canonical jar, rerun the actual repository runner and
    # validate a PASS instead of accepting a historical claim.
    jar = os.environ.get("TLC_JAR")
    if jar:
        jar_path = Path(jar)
        if not jar_path.is_file() or _sha256(jar_path) != PINNED_TLC_SHA256:
            return "TLC_JAR is present but does not match the canonical SHA-256 pin"
        run = subprocess.run(
            [
                "bash",
                "scripts/run_tla_suite.sh",
                "--only",
                "cross_tenant_handler_audit",
                "--timeout",
                "60",
                "--workers",
                "1",
            ],
            cwd=ROOT,
            env={**os.environ, "TLC_JAR": str(jar_path)},
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        if run.returncode != 0 or "PASS              cross_tenant_handler_audit" not in run.stdout:
            return "canonical TLC runner did not reproduce a PASS for B-255"
        return None
    return None


def main() -> int:
    required = (
        REGISTRY,
        TLA,
        HANDLER,
        HANDLER_LIB,
        D1_HANDLER,
        D1_MODULE,
        HANDLER_TESTS,
        MUTATION_TESTS,
        D1_TESTS,
        CONSISTENCY,
        SPEC,
        CFG,
        EVIDENCE,
        ADR,
    )
    missing = [str(path.relative_to(ROOT)) for path in required if not path.is_file()]
    if missing:
        return _fail("missing required evidence: " + ", ".join(missing))

    registry = REGISTRY.read_text(encoding="utf-8")
    tla = TLA.read_text(encoding="utf-8")
    handler = HANDLER.read_text(encoding="utf-8")
    handler_lib = HANDLER_LIB.read_text(encoding="utf-8")
    d1_handler = D1_HANDLER.read_text(encoding="utf-8")
    d1_module = D1_MODULE.read_text(encoding="utf-8")
    handler_tests = HANDLER_TESTS.read_text(encoding="utf-8")
    mutation_tests = MUTATION_TESTS.read_text(encoding="utf-8")
    d1_tests = D1_TESTS.read_text(encoding="utf-8")
    spec = SPEC.read_text(encoding="utf-8")
    cfg = CFG.read_text(encoding="utf-8")
    evidence_text = EVIDENCE.read_text(encoding="utf-8")

    evidence_status = _validate_tlc_evidence(evidence_text)
    if evidence_status is not None:
        return _fail(evidence_status)

    row = next(
        (line for line in registry.splitlines() if INV_ID in line and line.startswith("|")),
        None,
    )
    if row is None:
        return _fail(f"registry row for {INV_ID} is absent")
    if "CRITICAL" not in row or "cross_tenant_handler_audit.tla" not in row:
        return _fail("registry row does not record the resolved CRITICAL handler proof")
    if "cross_tenant_handler_audit.tla" not in registry:
        return _fail("registry does not point to the independent handler proof")
    if "techlead-review" in registry:
        return _fail("stale techlead adjudication comment remains in registry")

    if not all(token in tla for token in ("InvTenantIsolationRead", "InvTenantIsolationWrite", "InvTenantIsolationEnum")):
        return _fail("tenant_isolation.tla evidence is incomplete")
    if not all(token in handler_lib for token in (INV_ID, "CrossTenantDenied", "Denied")):
        return _fail("customer-handler invariant/audit/error references are incomplete")
    if not all(token in handler for token in ("CrossTenantDenied", "reject_cross_tenant", "requested_tenant")):
        return _fail("customer-handler cross-tenant guard wiring is incomplete")
    if not all(token in d1_handler for token in ("reject_cross_tenant", "CrossTenantDenied", "AuditFailed")):
        return _fail("D1 customer handler cross-tenant guard wiring is incomplete")
    if not all(token in handler_tests for token in ("OverviewDenied", "UsageDenied", "BillingDenied", "KeysDenied", "TeamDenied", "AuditQueryDenied", "for_tenant")):
        return _fail("the six customer handler groups are not represented in behavioral tests")
    if not all(token in mutation_tests for token in ("mutation_kills", "CrossTenantDenied", "Denied", "for_tenant")):
        return _fail("mutation-backed customer denial tests are missing")
    if not all(
        token in d1_tests
        for token in (
            "d1_cross_tenant_denials_cover_all_six_groups",
            "d1_cross_tenant_mutations_and_portal",
            "d1_cross_tenant_audit_failure_is_fail_closed",
            "CustomerHandlerError::CrossTenantDenied",
            "calls().len()",
            "inject_failure",
            "OverviewDenied",
            "UsageDenied",
            "BillingDenied",
            "KeysDenied",
            "TeamDenied",
            "AuditQueryDenied",
        )
    ):
        return _fail("D1 executable cross-tenant behavioral coverage is incomplete")
    if 'include!("customer_d1_tests_cross_tenant.rs")' not in d1_module:
        return _fail("D1 cross-tenant behavioral test module is not wired into customer_d1")
    # The customer route is a bounded include facade in the current tree.  Read
    # the facade and its exact local include targets so the guard follows the
    # executable route rather than mistaking a module split for missing wiring.
    route_path = ROOT / "crates/corelink-container/src/routes/customer.rs"
    route = route_path.read_text(encoding="utf-8")
    for included in re.findall(r'include!\("customer/([^"].*?)"\)', route):
        include_path = route_path.parent / "customer" / included
        if not include_path.is_file():
            return _fail(f"customer route include is missing: {include_path.relative_to(ROOT)}")
        route += "\n" + include_path.read_text(encoding="utf-8")
    if "CustomerHandlerError::CrossTenantDenied" not in route or "StatusCode::FORBIDDEN" not in route:
        return _fail("customer route does not preserve CrossTenantDenied -> 403 mapping")
    if not all(token in spec for token in (INV_ID, "CrossTenantDeny", "AuditFailureDeny", "InvAuditBeforeCrossTenantDenied", "InvAllSixGroupsCovered")):
        return _fail("independent TLA+ handler proof is incomplete")
    if not all(token in cfg for token in ("SPECIFICATION Spec", "InvAuditBeforeCrossTenantDenied", "InvAllSixGroupsCovered")):
        return _fail("TLA+ configuration does not gate the handler proof")

    result = subprocess.run(
        [sys.executable, str(CONSISTENCY), "--json"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    try:
        report = json.loads(result.stdout)
    except json.JSONDecodeError:
        return _fail("canonical-consistency JSON output is not parseable")
    critical_no_tla = report.get("critical_no_tla")
    if not isinstance(critical_no_tla, list):
        return _fail("canonical-consistency JSON has no critical_no_tla list")

    if critical_no_tla != []:
        return _fail(f"canonical consistency still reports critical_no_tla={critical_no_tla}", 1)

    backlog = (ROOT / "BACKLOG.md").read_text(encoding="utf-8")
    backlog_match = re.search(r"### B-255 .*?(?=\n### B-|\Z)", backlog, re.DOTALL)
    if backlog_match is None:
        return _fail("B-255 backlog block is absent")
    backlog_block = backlog_match.group(0)
    if "status: done" not in backlog_block or "proof absent" in backlog_block or "status: open" in backlog_block:
        return _fail("B-255 backlog narrative contradicts the closed proof")

    print(
        "CLOSED: independent handler TLA+ proof, six-group behavioral/mutation "
        f"coverage, validated TLC evidence, and registry consistency verified for {INV_ID}",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
