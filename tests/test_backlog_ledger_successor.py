"""Adversarial v3+ BASE-derived ledger successor and catalog tests."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_backlog_wp_ledger as ledger
from verify_backlog_wp_ledger import LedgerError, parse_ledger_state


def _git(repo: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", *args], cwd=repo, check=True, capture_output=True, text=True,
    )
    return result.stdout.strip()


def _successor_fixture(tmp_path, monkeypatch):
    """Tiny delivered-main fixture; no candidate code is executed."""
    base = tmp_path / "base"
    base.mkdir()
    catalog = Path("docs/campaigns/remediation/work-packages/B001-B045.md")
    genesis = ledger.GENESIS_SNAPSHOT_RELATIVE
    for relative, body in {
        Path("BACKLOG.md"): (
            "### B-001 — fixture\n```backlog\nid: B-001\nrepo: corelink-server\n"
            "owner: tl\nstatus: open\nverify: manual\nverify-means: first\n"
            "last-verified: 2026-09-12\n```\n"
            "### B-002 — closed fixture\n```backlog\nid: B-002\nrepo: corelink-server\n"
            "owner: tl\nstatus: done\nverify: manual\nverify-means: closed\n"
            "last-verified: 2026-09-12\n```\n"
        ),
        ledger.LEDGER_RELATIVE: (
            f"```ledger-state\nbase-ref: {ledger.POSTMERGE_BASE_SHA}\n"
            f"base-sha: {ledger.POSTMERGE_BASE_SHA}\nobserved-at: 2026-09-12\n"
            "item-count: 2\nopen-count: 1\ndone-count: 1\nparked-count: 0\n"
            "catalog-counts: B001-B045=1\n```\n"
        ),
        catalog: "## WP-A — fixture\n```wp-coverage\nB-001 WP-A\n```\n",
        genesis: "genesis fixture\n",
        ledger.SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot.json": "original fixture\n",
        Path("scripts/backlog_verify.py"): "# trusted control fixture\n",
        Path("scripts/verify_backlog_wp_ledger.py"): "# trusted ledger fixture\n",
        Path("scripts/backlog_ledger_successor.py"): "# trusted successor fixture\n",
        Path("scripts/backlog_ledger_contracts.py"): "# trusted contracts fixture\n",
    }.items():
        path = base / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body)
    _git(base, "init", "--quiet")
    _git(base, "config", "user.email", "tests@example.invalid")
    _git(base, "config", "user.name", "ledger tests")
    _git(base, "add", ".")
    _git(base, "commit", "--quiet", "-m", "delivered base")
    _git(base, "branch", "-M", "main")
    genesis_source_sha = ledger._sha256((base / "BACKLOG.md").read_bytes())
    monkeypatch.setattr(ledger, "REPO_ROOT", base)
    monkeypatch.setattr(ledger, "CATALOGS", {base / catalog: (1, 45)})
    monkeypatch.setattr(
        ledger, "load_postmerge_snapshot_manifest",
        lambda: {"source_sha256": genesis_source_sha},
    )
    monkeypatch.setattr(ledger, "validate_complete_catalog_state", lambda *_: 1)
    candidate = tmp_path / "candidate"
    shutil.copytree(base, candidate, ignore=shutil.ignore_patterns(".git"))
    _advance_successor(base, candidate, 3)
    return base, candidate


def _advance_successor(base: Path, candidate: Path, sequence: int) -> Path:
    base_sha = _git(base, "rev-parse", "HEAD")
    prior = ledger._state_bytes(base)
    backlog_path = candidate / "BACKLOG.md"
    old_word, new_word = ("first", "second") if sequence == 3 else ("second", "third")
    backlog_path.write_text(backlog_path.read_text().replace(f"verify-means: {old_word}", f"verify-means: {new_word}"))
    ledger_path = candidate / ledger.LEDGER_RELATIVE
    text = ledger_path.read_text()
    previous_base = ledger.parse_ledger_state(text, "candidate-ledger")["base-ref"]
    text = text.replace(f"base-ref: {previous_base}", f"base-ref: {base_sha}")
    text = text.replace(f"base-sha: {previous_base}", f"base-sha: {base_sha}")
    ledger_path.write_text(text)
    current = ledger._state_bytes(candidate)
    previous_path = (
        ledger.GENESIS_SNAPSHOT_RELATIVE if sequence == 3 else
        ledger.SNAPSHOT_DIRECTORY / f"backlog-ledger-snapshot-v{sequence - 1:04d}.json"
    )
    receipt = {
        "schema_version": 3, "sequence": sequence, "transition": "base-derived-data",
        "base_commit": base_sha,
        "prior_snapshot_sha256": ledger._sha256((base / previous_path).read_bytes()),
        "prior_source_sha256": ledger._sha256(prior["BACKLOG.md"]),
        "source_sha256": ledger._sha256(current["BACKLOG.md"]),
        "prior_ledger_sha256": ledger._sha256(prior[ledger.LEDGER_RELATIVE.as_posix()]),
        "ledger_sha256": ledger._sha256(current[ledger.LEDGER_RELATIVE.as_posix()]),
        "prior_catalog_sha256": {path.as_posix(): ledger._sha256(prior[path.as_posix()]) for path in ledger._catalog_relatives()},
        "catalog_sha256": {path.as_posix(): ledger._sha256(current[path.as_posix()]) for path in ledger._catalog_relatives()},
        "item_count": 2, "status_counts": {"done": 1, "open": 1, "parked": 0},
        "open_ids": ["B-001"], "changed_ids": ["B-001"],
    }
    path = candidate / ledger.SNAPSHOT_DIRECTORY / f"backlog-ledger-snapshot-v{sequence:04d}.json"
    path.write_text(json.dumps(receipt, indent=2) + "\n")
    return path


def test_base_derived_successor_accepts_v3_then_v4(tmp_path, monkeypatch):
    base, candidate = _successor_fixture(tmp_path, monkeypatch)
    assert ledger.validate_candidate_successor(base, candidate)["sequence"] == 3
    for relative in (
        Path("BACKLOG.md"), ledger.LEDGER_RELATIVE,
        ledger.SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot-v0003.json",
    ):
        target = base / relative
        target.write_bytes((candidate / relative).read_bytes())
    _git(base, "add", ".")
    _git(base, "commit", "--quiet", "-m", "accepted v3")
    next_candidate = tmp_path / "next-candidate"
    shutil.copytree(base, next_candidate, ignore=shutil.ignore_patterns(".git"))
    _advance_successor(base, next_candidate, 4)
    assert ledger.validate_candidate_successor(base, next_candidate)["sequence"] == 4


def test_delivered_successor_rejects_rewritten_receipt(tmp_path, monkeypatch):
    base, candidate = _successor_fixture(tmp_path, monkeypatch)
    assert ledger.validate_candidate_successor(base, candidate)["sequence"] == 3
    for relative in (
        Path("BACKLOG.md"), ledger.LEDGER_RELATIVE,
        ledger.SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot-v0003.json",
    ):
        (base / relative).write_bytes((candidate / relative).read_bytes())
    _git(base, "add", ".")
    _git(base, "commit", "--quiet", "-m", "accepted v3")
    receipt_path = base / ledger.SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot-v0003.json"
    receipt_path.write_text(receipt_path.read_text() + "\n")
    _git(base, "add", ".")
    _git(base, "commit", "--quiet", "-m", "rewrite v3")
    with pytest.raises(LedgerError, match="introduction is not unique"):
        ledger.load_successor_chain(base)


def test_b315_style_two_parent_merge_replays_successor(tmp_path, monkeypatch):
    base, candidate = _successor_fixture(tmp_path, monkeypatch)
    assert ledger.validate_candidate_successor(base, candidate)["sequence"] == 3
    _git(base, "branch", "candidate")
    _git(base, "checkout", "--quiet", "candidate")
    for relative in (
        Path("BACKLOG.md"), ledger.LEDGER_RELATIVE,
        ledger.SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot-v0003.json",
    ):
        (base / relative).write_bytes((candidate / relative).read_bytes())
    _git(base, "add", ".")
    _git(base, "commit", "--quiet", "-m", "candidate data")
    _git(base, "checkout", "--quiet", "main")
    _git(base, "merge", "--no-ff", "--quiet", "candidate", "-m", "B315-style merge")
    assert len(_git(base, "rev-list", "--parents", "-n", "1", "HEAD").split()) == 3
    assert ledger.load_successor_chain(base)["sequence"] == 3


@pytest.mark.parametrize("mutation,match", [
    ("wrong-base", "stale/replayed"),
    ("wrong-sequence", "stale/replayed"),
    ("forged-digest", "BASE-derived bytes"),
    ("rewritten-prior", "rewrote prior snapshot"),
    ("candidate-verifier", "immutable field 'verify'"),
    ("candidate-code", "mutated trusted backlog control"),
    ("candidate-successor-module", "mutated trusted backlog control"),
    ("candidate-contracts-module", "mutated trusted backlog control"),
    ("missing-receipt", "append exactly one"),
    ("malformed-receipt", "malformed successor snapshot"),
    ("illegal-status", "status counts differ"),
    ("preamble-edit", "immutable BACKLOG preamble"),
    ("preamble-only", "immutable BACKLOG preamble"),
    ("unlisted-section", "changed IDs differ"),
    ("unlisted-section-only", "changed IDs differ"),
    ("reordered-sections", "reordered or deleted"),
    ("duplicate-heading", "duplicate heading"),
    ("heading-without-item", "lacks exactly one matching item"),
    ("malformed-heading", "malformed B-ID heading"),
])
def test_base_derived_successor_rejects_mutations(tmp_path, monkeypatch, mutation, match):
    base, candidate = _successor_fixture(tmp_path, monkeypatch)
    path = candidate / ledger.SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot-v0003.json"
    receipt = json.loads(path.read_text())
    if mutation == "wrong-base":
        receipt["base_commit"] = "0" * 40
    elif mutation == "wrong-sequence":
        receipt["sequence"] = 4
    elif mutation == "forged-digest":
        receipt["source_sha256"] = "0" * 64
    elif mutation == "rewritten-prior":
        (candidate / ledger.GENESIS_SNAPSHOT_RELATIVE).write_text("rewritten\n")
    elif mutation == "candidate-verifier":
        backlog = candidate / "BACKLOG.md"
        backlog.write_text(backlog.read_text().replace("verify: manual", "verify: python3 scripts/evil.py", 1))
        receipt["source_sha256"] = ledger._sha256(backlog.read_bytes())
        receipt["changed_ids"] = ["B-001"]
    elif mutation == "candidate-code":
        (candidate / "scripts/backlog_verify.py").write_text("# candidate bypass\n")
    elif mutation == "candidate-successor-module":
        (candidate / "scripts/backlog_ledger_successor.py").write_text("# candidate bypass\n")
    elif mutation == "candidate-contracts-module":
        (candidate / "scripts/backlog_ledger_contracts.py").write_text("# candidate bypass\n")
    elif mutation == "missing-receipt":
        path.unlink()
    elif mutation == "malformed-receipt":
        path.write_text("{\n")
    elif mutation == "illegal-status":
        backlog = candidate / "BACKLOG.md"
        backlog.write_text(backlog.read_text().replace("status: open", "status: done"))
        receipt["source_sha256"] = ledger._sha256(backlog.read_bytes())
    elif mutation in {
        "preamble-edit", "preamble-only", "unlisted-section",
        "unlisted-section-only", "reordered-sections",
        "duplicate-heading", "heading-without-item", "malformed-heading",
    }:
        backlog = candidate / "BACKLOG.md"
        source = backlog.read_text()
        if mutation in {"preamble-only", "unlisted-section-only"}:
            source = source.replace("verify-means: second", "verify-means: first", 1)
            receipt["changed_ids"] = []
        if mutation in {"preamble-edit", "preamble-only"}:
            source = "attacker-owned preamble\n" + source
        elif mutation in {"unlisted-section", "unlisted-section-only"}:
            source = source.replace("closed fixture", "attacker fixture", 1)
        elif mutation == "reordered-sections":
            first, second = source.split("### B-002", 1)
            source = "### B-002" + second + first
        elif mutation == "duplicate-heading":
            source += "### B-001 — duplicated\n```backlog\nid: B-001\n```\n"
        elif mutation == "heading-without-item":
            source = source[:source.index("### B-002")] + "### B-002 — no item\n"
        elif mutation == "malformed-heading":
            source = source.replace("### B-002", "### B-foo", 1)
        backlog.write_text(source)
        receipt["source_sha256"] = ledger._sha256(backlog.read_bytes())
    if mutation not in {"missing-receipt", "malformed-receipt"}:
        path.write_text(json.dumps(receipt, indent=2) + "\n")
    error_type = RuntimeError if mutation in {
        "candidate-code", "candidate-successor-module", "candidate-contracts-module",
    } else LedgerError
    with pytest.raises(error_type, match=match):
        ledger.validate_candidate_successor(base, candidate)


@pytest.mark.parametrize("mutation,match", [
    ("missing-dod", "missing contract fields"),
    ("missing-read-first", "missing contract fields"),
    ("missing-allowlist", "editable allowlist fence"),
    ("missing-workflow-ownership", "workflow ownership fence"),
    ("broken-dependency-order", "dependency order does not match"),
])
def test_candidate_catalog_contracts_are_validated_before_merge(mutation, match):
    root = ledger.REPO_ROOT
    backlog = (root / "BACKLOG.md").read_text()
    ledger_text = ledger.LEDGER_PATH.read_text()
    catalog_data = {
        path.relative_to(root).as_posix(): path.read_bytes() for path in ledger.CATALOGS
    }
    base_sha = parse_ledger_state(ledger_text, "ledger")["base-ref"]
    if mutation in {"missing-dod", "missing-read-first"}:
        key = "docs/campaigns/remediation/work-packages/B001-B045.md"
        text = catalog_data[key].decode()
        begin, end = text.index("## WP-B008"), text.index("## WP-B012")
        section = text[begin:end]
        marker = "**Definition of Done.**" if mutation == "missing-dod" else "**Read first.**"
        assert marker in section
        catalog_data[key] = (text[:begin] + section.replace(marker, "**Notes.**", 1) + text[end:]).encode()
    elif mutation in {"missing-allowlist", "missing-workflow-ownership"}:
        key = "docs/campaigns/remediation/work-packages/B131-B167.md"
        fence = "wp-editable-allowlist" if mutation == "missing-allowlist" else "wp-workflow-ownership"
        catalog_data[key] = catalog_data[key].replace(f"```{fence}".encode(), b"```untrusted", 1)
    else:
        ledger_text = ledger_text.replace("WP-148 | WP-140,WP-146", "WP-148 | none", 1)
    with pytest.raises(LedgerError, match=match):
        ledger.validate_complete_catalog_state(backlog, ledger_text, catalog_data, root, base_sha)
