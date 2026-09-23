"""Closed-world mutation tests for the 73 canonical B-101 proposal contracts."""

from __future__ import annotations

import copy
import json
import shutil
import sys
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import verify_b101_proposals as guard  # noqa: E402
import verify_b101_open_state as open_guard  # noqa: E402


def fixture_tree(tmp_path: Path) -> Path:
    for relative in (guard.BACKLOG, guard.REGISTRY, guard.MANIFEST):
        destination = tmp_path / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, destination)
    for relative in (
        Path("docs/security/2026-06-15-launch-due-diligence-audit.md"),
        Path("reports/audits/2026-08-26-go-live-readiness.md"),
        Path("docs/security/2026-07-02-pilot-identity-brutal-audit.md"),
    ):
        destination = tmp_path / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, destination)
    return tmp_path


def read_registry(root: Path) -> dict:
    return json.loads((root / guard.REGISTRY).read_text(encoding="utf-8"))


def write_registry(root: Path, registry: dict) -> None:
    (root / guard.REGISTRY).write_text(json.dumps(registry), encoding="utf-8")


def test_live_registry_covers_all_73_specific_contracts() -> None:
    assert guard.verify(ROOT) == {"records": 73, "unfinished": 0, "done": 73}


def test_global_guard_accepts_later_shared_done_verifier() -> None:
    assert guard.verify(ROOT, "B-231") == {"records": 1, "unfinished": 0, "done": 1}


def test_b210_retirement_contract_rejects_stale_proposal_next_action(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    verifier = root / "scripts/verify_b210_retirement.py"
    verifier.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / "scripts/verify_b210_retirement.py", verifier)
    assert guard.verify(root, "B-210") == {"records": 1, "unfinished": 0, "done": 1}

    backlog_path = root / "BACKLOG.md"
    backlog = backlog_path.read_text(encoding="utf-8")
    old_next_action = 'next-action: "Keep B-210 done while B-119 remains done and /admin/ops* remains absent; if a durable, securely bound approval surface is restored, re-open B-119 and B-210 together and re-audit SSR guard ordering before publishing any page."'
    canonical_next_action = next(
        proposal["next_action"]
        for proposal in read_registry(root)["proposals"]
        if proposal["id"] == "B-210"
    )
    assert backlog.count(old_next_action) == 1
    backlog_path.write_text(
        backlog.replace(old_next_action, f"next-action: {json.dumps(canonical_next_action)}", 1),
        encoding="utf-8",
    )
    with pytest.raises(guard.ProposalVerificationError, match="missing or mismatched next_action"):
        guard.verify(root, "B-210")

    backlog_path.write_text(backlog, encoding="utf-8")
    assert guard.verify(root, "B-210") == {"records": 1, "unfinished": 0, "done": 1}


def test_b229_production_done_contract_rejects_stale_proposal_next_action(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    verifier = root / "scripts/verify_b229_clerk_webhook.py"
    verifier.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / "scripts/verify_b229_clerk_webhook.py", verifier)
    assert guard.verify(root, "B-229") == {"records": 1, "unfinished": 0, "done": 1}

    backlog_path = root / "BACKLOG.md"
    backlog = backlog_path.read_text(encoding="utf-8")
    completed_next_action = 'next-action: "Completed: confirm the deployed secret-name gate, deploy the exact source SHA, and verify one signed production webhook without retaining credential or payload values."'
    canonical_next_action = next(
        proposal["next_action"]
        for proposal in read_registry(root)["proposals"]
        if proposal["id"] == "B-229"
    )
    assert backlog.count(completed_next_action) == 1
    backlog_path.write_text(
        backlog.replace(completed_next_action, f"next-action: {json.dumps(canonical_next_action)}", 1),
        encoding="utf-8",
    )
    with pytest.raises(guard.ProposalVerificationError, match="missing or mismatched next_action"):
        guard.verify(root, "B-229")

    backlog_path.write_text(backlog, encoding="utf-8")
    assert guard.verify(root, "B-229") == {"records": 1, "unfinished": 0, "done": 1}


def test_open_state_rule_set_is_dense_and_executable() -> None:
    assert set(open_guard.RULES) == {f"B-{number}" for number in range(171, 244)}
    assert open_guard.verify(ROOT) == {"records": 0, "unfinished": 0}


def test_open_state_missing_artifact_is_red(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    with pytest.raises(open_guard.OpenStateError, match="artifact escapes or is missing"):
        open_guard.verify(root, "B-171")


def test_open_state_symlink_escape_is_red(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    relative = Path(open_guard.RULES["B-171"].artifacts[0])
    destination = root / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    outside = tmp_path / "outside-source.rs"
    outside.write_text("fn r2_key() {}\n", encoding="utf-8")
    destination.symlink_to(outside)
    with pytest.raises(open_guard.OpenStateError, match="artifact is not a regular non-symlink file"):
        open_guard.verify(root, "B-171")


def test_open_state_comment_bait_cannot_replace_code_anchor(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    relative = Path(open_guard.RULES["B-171"].artifacts[0])
    destination = root / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    source = ROOT / relative
    destination.write_text(
        source.read_text(encoding="utf-8").replace("fn r2_key", "// fn r2_key"),
        encoding="utf-8",
    )
    with pytest.raises(open_guard.OpenStateError, match="anchor missing"):
        open_guard.verify(root, "B-171")


def test_open_state_string_bait_cannot_replace_semantic_body(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    relative = Path(open_guard.RULES["B-171"].artifacts[0])
    destination = root / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    source = ROOT / relative
    text = source.read_text(encoding="utf-8")
    text = text.replace(
        "fn r2_key(&self, tenant: &str, digest: &str, algo: DigestAlgo) -> Result<String, String> {",
        'fn r2_key(&self, tenant: &str, digest: &str, algo: DigestAlgo) -> Result<String, String> {\n    let bait = "tenant_prefix(self.tdk.as_ref(), tenant)? R2S3Client::blob_key";\n    let _ = bait;',
        1,
    )
    # Remove the executable key derivation while retaining a declaration and
    # a string that contains both required semantic tokens.
    start = text.index("fn r2_key(")
    body_start = text.index("    let prefix =", start)
    body_end = text.index("    Ok(R2S3Client::blob_key(", body_start)
    text = text[:body_start] + text[text.index("    }", body_end) + 6 :]
    destination.write_text(text, encoding="utf-8")
    with pytest.raises(open_guard.OpenStateError, match="semantic contract clause"):
        open_guard.verify(root, "B-171")


def test_open_state_closure_witness_turns_green_open_guard_red(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    relative = Path(open_guard.RULES["B-171"].artifacts[0])
    destination = root / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / relative, destination)
    witness = root / open_guard.RULES["B-171"].closure_witness
    witness.parent.mkdir(parents=True, exist_ok=True)
    witness.write_text("executable closure evidence\n", encoding="utf-8")
    with pytest.raises(open_guard.OpenStateError, match="closure witness exists"):
        open_guard.verify(root, "B-171")


def test_open_state_oci_header_toy_is_red(tmp_path: Path) -> None:
    """A copied h.set/string bait must not stand in for the OCI forwarding path."""
    root = fixture_tree(tmp_path)
    relative = Path(open_guard.RULES["B-179"].artifacts[0])
    destination = root / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    source = ROOT / relative
    text = source.read_text(encoding="utf-8")
    text = text.replace(
        'h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");',
        'function bait() { const h = new Headers(); h.set("x-corelink-client-ip", "bait"); }',
        1,
    )
    destination.write_text(text, encoding="utf-8")
    with pytest.raises(open_guard.OpenStateError, match="OCI client-ip setter"):
        open_guard.verify(root, "B-179")


def test_open_state_oci_header_outside_branch_is_red(tmp_path: Path) -> None:
    """A setter outside both OCI forwarding Requests cannot replace either one."""
    root = fixture_tree(tmp_path)
    relative = Path(open_guard.RULES["B-179"].artifacts[0])
    destination = root / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    source = ROOT / relative
    text = source.read_text(encoding="utf-8")
    exact = 'h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");'
    route_start = text.index('if (route.routeKind === "oci_v2"')
    route_end = text.index("return applyCors(ociResp, request);\n    }", route_start)
    branch = text[route_start:route_end]
    assert branch.count(exact) == 2
    branch = branch.replace(exact, "// removed forwarding setter", 2)
    outside = f"\nfunction outsideOciBranch(h: Headers, request: Request) {{ {exact} }}\n"
    branch_end = route_end + len("return applyCors(ociResp, request);\n    }")
    destination.write_text(
        text[:route_start] + branch + text[route_end:branch_end] + outside + text[branch_end:],
        encoding="utf-8",
    )
    with pytest.raises(open_guard.OpenStateError, match="OCI client-ip setter"):
        open_guard.verify(root, "B-179")


def test_open_state_auth_tenant_attacker_constant_is_red(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    relative = Path(open_guard.RULES["B-192"].artifacts[0])
    destination = root / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    source = ROOT / relative
    text = source.read_text(encoding="utf-8").replace(
        "Ok(AuthTenant(raw.to_owned()))",
        'Ok(AuthTenant("attacker".to_owned()))',
        1,
    )
    destination.write_text(text, encoding="utf-8")
    kv = Path("crates/corelink-container/src/storage/r2_kv.rs")
    (root / kv).parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / kv, root / kv)
    with pytest.raises(open_guard.OpenStateError, match="AuthTenant semantic clause"):
        open_guard.verify(root, "B-192")


def test_open_state_tenant_org_map_insert_mutation_is_red(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    relative = Path(open_guard.RULES["B-243"].artifacts[0])
    destination = root / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    source = ROOT / relative
    text = source.read_text(encoding="utf-8").replace(
        '"INSERT OR IGNORE INTO tenant_org_map "',
        '"INSERT INTO tenant_org_map "',
        1,
    )
    destination.write_text(text, encoding="utf-8")
    with pytest.raises(open_guard.OpenStateError, match="semantic contract clause"):
        open_guard.verify(root, "B-243")


def test_missing_record_is_red(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    registry = read_registry(root)
    registry["proposals"].pop()
    write_registry(root, registry)
    with pytest.raises(guard.ProposalVerificationError, match="exactly 73"):
        guard.verify(root)


def test_duplicate_record_is_red(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    registry = read_registry(root)
    registry["proposals"][-1] = copy.deepcopy(registry["proposals"][0])
    write_registry(root, registry)
    with pytest.raises(guard.ProposalVerificationError, match="unique"):
        guard.verify(root)


def test_mismatched_title_is_red(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    registry = read_registry(root)
    registry["proposals"][0]["title"] = "different specific title"
    write_registry(root, registry)
    with pytest.raises(guard.ProposalVerificationError, match="manifest proposal"):
        guard.verify(root, "B-171")


def test_stale_id_is_red(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    registry = read_registry(root)
    registry["proposals"][0]["id"] = "B-999"
    write_registry(root, registry)
    with pytest.raises(guard.ProposalVerificationError, match="stale or unreserved"):
        guard.verify(root)


def test_placeholder_or_generic_evidence_is_red(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    registry = read_registry(root)
    registry["proposals"][0]["evidence"] = "generic evidence TODO"
    write_registry(root, registry)
    with pytest.raises(guard.ProposalVerificationError, match="placeholder or generic"):
        guard.verify(root, "B-171")


@pytest.mark.parametrize(
    ("field", "value", "message"),
    (
        ("owner", "owner", "owner must be tl"),
        ("status", "done", "status must be open"),
        ("dependencies", ["B-999"], "dependencies"),
        ("next_action", "address issue", "backlog contract"),
    ),
)
def test_bad_contract_fields_are_red(tmp_path: Path, field: str, value: object, message: str) -> None:
    root = fixture_tree(tmp_path)
    registry = read_registry(root)
    registry["proposals"][0][field] = value
    write_registry(root, registry)
    with pytest.raises(guard.ProposalVerificationError, match=message):
        guard.verify(root, "B-171")


def test_missing_source_is_red(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    (root / "docs/security/2026-06-15-launch-due-diligence-audit.md").unlink()
    with pytest.raises(guard.ProposalVerificationError, match="admitted source is missing"):
        guard.verify(root, "B-171")


def test_source_document_symlink_escape_is_red(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    source = root / "docs/security/2026-06-15-launch-due-diligence-audit.md"
    outside = tmp_path / "outside-audit.md"
    outside.write_text(source.read_text(encoding="utf-8"), encoding="utf-8")
    source.unlink()
    source.symlink_to(outside)
    with pytest.raises(guard.ProposalVerificationError, match="resolve inside|non-symlink"):
        guard.verify(root, "B-171")


def test_missing_backlog_record_is_red(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    path = root / guard.BACKLOG
    text = path.read_text(encoding="utf-8")
    start = text.index("### B-171 — ")
    end = text.index("### B-172 — ", start)
    path.write_text(text[:start] + text[end:], encoding="utf-8")
    with pytest.raises(guard.ProposalVerificationError, match="heading is missing"):
        guard.verify(root, "B-171")


def test_backlog_owner_mutation_is_red(tmp_path: Path) -> None:
    root = fixture_tree(tmp_path)
    path = root / guard.BACKLOG
    text = path.read_text(encoding="utf-8")
    start = text.index("### B-171 — ")
    end = text.index("### B-172 — ", start)
    section = text[start:end].replace("owner: tl", "owner: owner", 1)
    path.write_text(text[:start] + section + text[end:], encoding="utf-8")
    with pytest.raises(guard.ProposalVerificationError, match="owner"):
        guard.verify(root, "B-171")
