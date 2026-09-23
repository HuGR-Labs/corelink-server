"""Adversarial v3+ BASE-derived ledger successor and catalog tests."""

from __future__ import annotations

import json
import datetime as dt
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_backlog_wp_ledger as ledger
import backlog_ledger_successor as successor
from verify_backlog_wp_ledger import LedgerError, parse_ledger_state


def _git(repo: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", *args], cwd=repo, check=True, capture_output=True, text=True,
    )
    return result.stdout.strip()


def _successor_fixture(tmp_path, monkeypatch, *, first_status="open", anchor_open=False):
    """Tiny delivered-main fixture; no candidate code is executed."""
    base = tmp_path / "base"
    base.mkdir()
    catalog = Path("docs/campaigns/remediation/work-packages/B001-B045.md")
    genesis = ledger.GENESIS_SNAPSHOT_RELATIVE
    counts = {"open": 0, "parked": 0, "done": 1}
    counts[first_status] += 1
    anchor_open = anchor_open or first_status != "open"
    if anchor_open:
        counts["open"] += 1
    anchor_section = (
        "### B-003 — anchor\n```backlog\nid: B-003\nrepo: corelink-server\n"
        "owner: tl\nstatus: open\nverify: manual\nverify-means: anchor\n"
        "last-verified: 2026-09-01\n```\n"
    ) if anchor_open else ""
    for relative, body in {
        Path("BACKLOG.md"): (
            "### B-001 — fixture\n```backlog\nid: B-001\nrepo: corelink-server\n"
            f"owner: tl\nstatus: {first_status}\nverify: manual\nverify-means: first\n"
            "last-verified: 2026-09-12\n```\n"
            "### B-002 — closed fixture\n```backlog\nid: B-002\nrepo: corelink-server\n"
            "owner: tl\nstatus: done\nverify: manual\nverify-means: closed\n"
            "last-verified: 2026-09-01\n```\n"
            f"{anchor_section}"
        ),
        ledger.LEDGER_RELATIVE: (
            f"```ledger-state\nbase-ref: {ledger.POSTMERGE_BASE_SHA}\n"
            f"base-sha: {ledger.POSTMERGE_BASE_SHA}\nobserved-at: 2026-09-12\n"
            f"item-count: {sum(counts.values())}\nopen-count: {counts['open']}\n"
            f"done-count: {counts['done']}\nparked-count: {counts['parked']}\n"
            "catalog-counts: B001-B045=1\n```\n"
        ),
        catalog: "## WP-A — fixture\n```wp-coverage\nB-001 WP-A\n```\n",
        genesis: "genesis fixture\n",
        ledger.SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot.json": "original fixture\n",
        Path("scripts/backlog_verify.py"): "# trusted control fixture\n",
        Path("scripts/verify_owner_action_packets.py"): "# trusted control fixture\n",
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
    old_date, new_date = ("2026-09-01", "2026-09-02") if sequence == 3 else ("2026-09-02", "2026-09-03")
    first, second = backlog_path.read_text().split("### B-002", 1)
    backlog_path.write_text(first + "### B-002" + second.replace(
        f"last-verified: {old_date}", f"last-verified: {new_date}", 1,
    ))
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
        "item_count": sum(ledger.backlog_status_counts(current["BACKLOG.md"].decode()).values()),
        "status_counts": {
            key: ledger.backlog_status_counts(current["BACKLOG.md"].decode()).get(key, 0)
            for key in ("done", "open", "parked")
        },
        "open_ids": sorted(ledger.open_backlog_ids(current["BACKLOG.md"].decode())),
        "changed_ids": ["B-002"],
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


def test_exact_rewrite_authorization_binds_both_sources_ids_and_fields(tmp_path, monkeypatch):
    base, candidate = _successor_fixture(tmp_path, monkeypatch)
    prior, current = ledger._state_bytes(base), ledger._state_bytes(candidate)
    monkeypatch.setattr(successor, "SPRINT3_BACKLOG_SHA256", (
        ledger._sha256(prior["BACKLOG.md"]), ledger._sha256(current["BACKLOG.md"]),
    ))
    monkeypatch.setattr(successor, "SPRINT3_CHANGED_IDS", ["B-001"])
    first = ledger._sha256(b"first")
    manual = ledger._sha256(b"manual")
    monkeypatch.setattr(successor, "SPRINT3_FIELDS", {
        "B-001": ("open", manual, manual, first, first),
    })
    authorized = ledger._successor_policy()._sprint3_rewrite_authorized
    receipt = {"changed_ids": ["B-001"]}
    assert authorized(prior, current, receipt, 3)
    assert not authorized(prior, current, receipt, 4)
    assert not authorized(prior, current, {"changed_ids": []}, 3)
    assert not authorized({**prior, "BACKLOG.md": prior["BACKLOG.md"] + b"\n"}, current, receipt, 3)
    assert not authorized(prior, {**current, "BACKLOG.md": current["BACKLOG.md"] + b"\n"}, receipt, 3)
    monkeypatch.setattr(successor, "SPRINT3_FIELDS", {
        "B-001": ("open", manual, manual, first, ledger._sha256(b"no owner approval needed")),
    })
    assert not authorized(prior, current, receipt, 3)


def test_v0004_reconciliation_is_byte_pinned_before_any_data_successor(monkeypatch):
    policy = ledger._successor_policy()
    catalogs = {path.as_posix(): b"catalog" for path in policy._catalog_relatives()}
    prior = {
        "BACKLOG.md": (
            b"### B-098 \xe2\x80\x94 census before\n```backlog\n"
            b"id: B-098\nrepo: corelink-server\nowner: tl\nstatus: parked\n"
            b"verify: manual\nverify-means: census\nlast-verified: 2026-09-22\n```\n"
            b"### B-154 \xe2\x80\x94 legacy\n```backlog\n"
            b"id: B-154\nrepo: corelink-server\nowner: owner\nstatus: open\n"
            b"verify: legacy\nverify-means: legacy proof\nlast-verified: 2026-09-05\n```\n"
        ),
        ledger.LEDGER_RELATIVE.as_posix(): b"ledger-before",
        **catalogs,
    }
    current = {
        "BACKLOG.md": (
            b"### B-098 \xe2\x80\x94 census after\n```backlog\n"
            b"id: B-098\nrepo: corelink-server\nowner: tl\nstatus: parked\n"
            b"verify: manual\nverify-means: census\nlast-verified: 2026-09-22\n```\n"
            b"### B-154 \xe2\x80\x94 canonical\n```backlog\n"
            b"id: B-154\nrepo: corelink-server\nowner: owner\nstatus: open\n"
            b"verify: canonical\nverify-means: canonical proof\nlast-verified: 2026-09-22\n```\n"
        ),
        ledger.LEDGER_RELATIVE.as_posix(): b"ledger-after",
        **catalogs,
    }
    fields = (
        "open",
        ledger._sha256(b"legacy"), ledger._sha256(b"canonical"),
        ledger._sha256(b"legacy proof"), ledger._sha256(b"canonical proof"),
    )
    pinned = {
        "sequence": 4,
        "previous_sequence": 3,
        "previous_source_sha256": "previous-receipt-source",
        "prior_source_sha256": ledger._sha256(prior["BACKLOG.md"]),
        "source_sha256": ledger._sha256(current["BACKLOG.md"]),
        "prior_ledger_sha256": ledger._sha256(prior[ledger.LEDGER_RELATIVE.as_posix()]),
        "ledger_sha256": ledger._sha256(current[ledger.LEDGER_RELATIVE.as_posix()]),
        "changed_ids": ["B-098", "B-154"],
        "catalog_sha256": {key: ledger._sha256(value) for key, value in catalogs.items()},
        "fields": fields,
    }
    monkeypatch.setattr(successor, "V0004_RECONCILIATION", pinned)
    previous = {"sequence": 3, "source_sha256": "previous-receipt-source"}
    receipt = {
        "base_commit": "a" * 40,
        "prior_source_sha256": pinned["prior_source_sha256"],
        "source_sha256": pinned["source_sha256"],
        "prior_ledger_sha256": pinned["prior_ledger_sha256"],
        "ledger_sha256": pinned["ledger_sha256"],
        "prior_catalog_sha256": pinned["catalog_sha256"],
        "catalog_sha256": pinned["catalog_sha256"],
        "changed_ids": pinned["changed_ids"],
    }
    authorized = policy._v0004_reconciliation_authorized
    assert authorized(previous, prior, current, receipt, 4)
    assert not authorized(previous, prior, current, {**receipt, "changed_ids": ["B-154"]}, 4)
    assert not authorized(
        previous, prior, {**current, "BACKLOG.md": current["BACKLOG.md"] + b"\n"}, receipt, 4,
    )


def test_v0004_policy_in_base_accepts_only_the_exact_dynamic_base_successor(
    tmp_path, monkeypatch,
):
    repo_root = Path(__file__).resolve().parents[1]
    real_b154 = next(
        item for item in ledger.backlog_verify.parse((repo_root / "BACKLOG.md").read_text())
        if item.id == "B-154"
    ).raw
    legacy_verify = real_b154["verify"]
    assert ledger._sha256(legacy_verify.encode()) == ledger.backlog_verify.B154_LEGACY_VERIFY_SHA256
    canonical_verify = (
        "python3 -S scripts/verify_owner_action_packets.py --id B-154 &&\n"
        "python3 -S scripts/verify_b154_instrument_claims.py --self-test\n"
    )

    def exact_case(name: str):
        case_root = tmp_path / name
        case_root.mkdir()
        base, v3_candidate = _successor_fixture(case_root, monkeypatch)
        assert ledger.validate_candidate_successor(base, v3_candidate)["sequence"] == 3
        for relative in (
            Path("BACKLOG.md"), ledger.LEDGER_RELATIVE,
            ledger.SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot-v0003.json",
        ):
            (base / relative).write_bytes((v3_candidate / relative).read_bytes())
        _git(base, "add", ".")
        _git(base, "commit", "--quiet", "-m", "delivered v3")

        legacy_command = legacy_verify.rstrip().replace("\n", "\n  ")
        canonical_command = canonical_verify.rstrip().replace("\n", "\n  ")
        legacy_block = (
            "### B-154 — legacy\n```backlog\nid: B-154\nrepo: corelink-server\n"
            "owner: owner\nstatus: open\nverify: |\n  "
            + legacy_command
            + "\nverify-means: |\n  legacy proof\nlast-verified: 2026-09-05\n```\n"
        )
        prestate = (base / "BACKLOG.md").read_text() + (
            "### B-098 — census before\n```backlog\nid: B-098\nrepo: corelink-server\n"
            "owner: tl\nstatus: parked\nverify: manual\nverify-means: census\n"
            "last-verified: 2026-09-22\n```\n" + legacy_block
        )
        (base / "BACKLOG.md").write_text(prestate)
        base_ledger = base / ledger.LEDGER_RELATIVE
        base_ledger.write_text(base_ledger.read_text().replace(
            "item-count: 2", "item-count: 4"
        ).replace("open-count: 1", "open-count: 2").replace(
            "parked-count: 0", "parked-count: 1"
        ))
        _git(base, "add", ".")
        _git(base, "commit", "--quiet", "-m", "trusted policy base")
        base_sha = _git(base, "rev-parse", "HEAD")

        candidate = case_root / "v4-candidate"
        shutil.copytree(base, candidate, ignore=shutil.ignore_patterns(".git"))
        candidate_backlog = candidate / "BACKLOG.md"
        target_block = legacy_block.replace("### B-154 — legacy", "### B-154 — canonical").replace(
            legacy_command, canonical_command
        ).replace("legacy proof", "canonical proof").replace(
            "last-verified: 2026-09-05", "last-verified: 2026-09-22"
        )
        candidate_backlog.write_text(candidate_backlog.read_text().replace(
            "### B-098 — census before", "### B-098 — census after"
        ).replace(legacy_block, target_block))
        candidate_ledger = candidate / ledger.LEDGER_RELATIVE
        old_base = parse_ledger_state(candidate_ledger.read_text(), "candidate-ledger")["base-ref"]
        candidate_ledger.write_text(candidate_ledger.read_text().replace(
            f"base-ref: {old_base}", f"base-ref: {base_sha}"
        ).replace(f"base-sha: {old_base}", f"base-sha: {base_sha}"))
        prior, current = ledger._state_bytes(base), ledger._state_bytes(candidate)
        catalog_hashes = {
            path.as_posix(): ledger._sha256(prior[path.as_posix()])
            for path in ledger._successor_policy()._catalog_relatives()
        }
        old_b154 = next(item for item in ledger.backlog_verify.parse(prior["BACKLOG.md"].decode()) if item.id == "B-154").raw
        new_b154 = next(item for item in ledger.backlog_verify.parse(current["BACKLOG.md"].decode()) if item.id == "B-154").raw
        v3_receipt = json.loads((base / ledger.SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot-v0003.json").read_text())
        pinned = {
            "sequence": 4, "previous_sequence": 3,
            "previous_source_sha256": v3_receipt["source_sha256"],
            "prior_source_sha256": ledger._sha256(prior["BACKLOG.md"]),
            "source_sha256": ledger._sha256(current["BACKLOG.md"]),
            "prior_ledger_sha256": ledger._sha256(prior[ledger.LEDGER_RELATIVE.as_posix()]),
            "ledger_sha256": ledger._sha256(current[ledger.LEDGER_RELATIVE.as_posix()]),
            "changed_ids": ["B-098", "B-154"], "catalog_sha256": catalog_hashes,
            "fields": (
                old_b154["status"], ledger._sha256(old_b154["verify"].encode()),
                ledger._sha256(new_b154["verify"].encode()),
                ledger._sha256(old_b154["verify-means"].encode()),
                ledger._sha256(new_b154["verify-means"].encode()),
            ),
        }
        monkeypatch.setattr(successor, "V0004_RECONCILIATION", pinned)
        receipt = {
            "schema_version": 3, "sequence": 4, "transition": "base-derived-data",
            "base_commit": base_sha,
            "prior_snapshot_sha256": ledger._sha256((base / ledger.SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot-v0003.json").read_bytes()),
            "prior_source_sha256": pinned["prior_source_sha256"], "source_sha256": pinned["source_sha256"],
            "prior_ledger_sha256": pinned["prior_ledger_sha256"], "ledger_sha256": pinned["ledger_sha256"],
            "prior_catalog_sha256": catalog_hashes, "catalog_sha256": catalog_hashes,
            "item_count": 4, "status_counts": {"done": 1, "open": 2, "parked": 1},
            "open_ids": ["B-001", "B-154"], "changed_ids": ["B-098", "B-154"],
        }
        receipt_path = candidate / ledger.SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot-v0004.json"
        receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
        policy = ledger._successor_policy()
        assert successor.V0004_RECONCILIATION == pinned
        assert policy._v0004_reconciliation_authorized(v3_receipt, prior, current, receipt, 4)
        return base, candidate, receipt_path

    base, candidate, receipt_path = exact_case("exact")
    assert ledger.validate_candidate_successor(base, candidate, today=dt.date(2026, 9, 22))["sequence"] == 4

    base, candidate, receipt_path = exact_case("base-commit")
    receipt = json.loads(receipt_path.read_text())
    receipt["base_commit"] = "0" * 40
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
    with pytest.raises(LedgerError, match="stale/replayed"):
        ledger.validate_candidate_successor(base, candidate, today=dt.date(2026, 9, 22))

    base, candidate, receipt_path = exact_case("base-state")
    (base / "BACKLOG.md").write_text((base / "BACKLOG.md").read_text() + "\n")
    # Accept either correct earlier or later fail-closed guard.
    with pytest.raises(
        LedgerError,
        match="delivered state drifted after last successor|trusted BASE data differs",
    ):
        ledger.validate_candidate_successor(base, candidate, today=dt.date(2026, 9, 22))

    base, candidate, receipt_path = exact_case("candidate-data")
    candidate_backlog = candidate / "BACKLOG.md"
    candidate_backlog.write_text(candidate_backlog.read_text().replace("census after", "census mutated"))
    receipt = json.loads(receipt_path.read_text())
    receipt["source_sha256"] = ledger._sha256(candidate_backlog.read_bytes())
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
    with pytest.raises(LedgerError, match="normative BACKLOG section changed"):
        ledger.validate_candidate_successor(base, candidate, today=dt.date(2026, 9, 22))


@pytest.mark.parametrize("old_status,new_status,mutation", [
    ("open", "parked", "verify-means"),
    ("open", "done", "verify-means"),
    ("parked", "open", "prose"),
    ("done", "done", "prose"),
])
def test_status_transition_cannot_waive_existing_normative_text(
    tmp_path, monkeypatch, old_status, new_status, mutation,
):
    base, candidate = _successor_fixture(
        tmp_path, monkeypatch, first_status=old_status,
    )
    backlog = candidate / "BACKLOG.md"
    source = backlog.read_text().replace(
        f"status: {old_status}", f"status: {new_status}", 1,
    )
    if mutation == "verify-means":
        source = source.replace(
            "verify-means: first", "verify-means: no owner approval needed", 1,
        )
    else:
        source = source.replace(
            "### B-001 — fixture", "### B-001 — no owner approval needed", 1,
        )
    backlog.write_text(source)
    path = candidate / ledger.SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot-v0003.json"
    receipt = json.loads(path.read_text())
    receipt["source_sha256"] = ledger._sha256(backlog.read_bytes())
    receipt["changed_ids"] = ["B-001", "B-002"]
    path.write_text(json.dumps(receipt, indent=2) + "\n")
    with pytest.raises(LedgerError, match="normative BACKLOG section changed"):
        ledger.validate_candidate_successor(base, candidate)


def test_open_to_parked_status_only_is_allowed(tmp_path, monkeypatch):
    base, candidate = _successor_fixture(tmp_path, monkeypatch, anchor_open=True)
    backlog = candidate / "BACKLOG.md"
    backlog.write_text(backlog.read_text().replace("status: open", "status: parked", 1))
    ledger_path = candidate / ledger.LEDGER_RELATIVE
    ledger_path.write_text(ledger_path.read_text().replace(
        "open-count: 2", "open-count: 1", 1,
    ).replace("parked-count: 0", "parked-count: 1", 1))
    path = candidate / ledger.SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot-v0003.json"
    receipt = json.loads(path.read_text())
    receipt["source_sha256"] = ledger._sha256(backlog.read_bytes())
    receipt["ledger_sha256"] = ledger._sha256(ledger_path.read_bytes())
    receipt["changed_ids"] = ["B-001", "B-002"]
    receipt["status_counts"] = {"done": 1, "open": 1, "parked": 1}
    receipt["open_ids"] = ["B-003"]
    path.write_text(json.dumps(receipt, indent=2) + "\n")
    assert ledger.validate_candidate_successor(base, candidate)["sequence"] == 3


def test_b065_parked_owner_waiver_is_normative_not_status_data():
    source = (ledger.REPO_ROOT / "BACKLOG.md").read_bytes()
    start = source.index(b"### B-065")
    end = source.index(b"### B-066", start)
    old_section = source[start:end]
    new_section = old_section.replace(b"status: open", b"status: parked", 1).replace(
        b"verify-means: |\n",
        b"verify-means: |\n  No owner approval or Stripe dashboard action required.\n",
        1,
    )
    old_item = ledger.backlog_verify.parse(old_section.decode())[0]
    new_item = ledger.backlog_verify.parse(new_section.decode())[0]
    assert ledger.backlog_verify.validate_candidate_transitions(
        [new_item], [old_item], dt.date(2026, 9, 13), successor_mode=True,
    ) == []  # Field-only validation alone does not protect prose.
    normalize = ledger._successor_policy()._normative_section
    assert normalize(old_section, "B-065") != normalize(new_section, "B-065")


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
    ("candidate-verifier", "normative BACKLOG section changed"),
    ("approval-waiver", "normative BACKLOG section changed"),
    ("open-prose-waiver", "normative BACKLOG section changed"),
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
        first, second = backlog.read_text().split("### B-002", 1)
        backlog.write_text(first + "### B-002" + second.replace(
            "verify: manual", "verify: python3 scripts/evil.py", 1,
        ))
        receipt["source_sha256"] = ledger._sha256(backlog.read_bytes())
    elif mutation == "approval-waiver":
        backlog = candidate / "BACKLOG.md"
        backlog.write_text(backlog.read_text().replace(
            "verify-means: first", "verify-means: no owner approval needed", 1,
        ))
        receipt["source_sha256"] = ledger._sha256(backlog.read_bytes())
        receipt["changed_ids"] = ["B-001", "B-002"]
    elif mutation == "open-prose-waiver":
        backlog = candidate / "BACKLOG.md"
        backlog.write_text(backlog.read_text().replace(
            "### B-001 — fixture", "### B-001 — no owner approval needed", 1,
        ))
        receipt["source_sha256"] = ledger._sha256(backlog.read_bytes())
        receipt["changed_ids"] = ["B-001", "B-002"]
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
        receipt["changed_ids"] = ["B-001", "B-002"]
    elif mutation in {
        "preamble-edit", "preamble-only", "unlisted-section",
        "unlisted-section-only", "reordered-sections",
        "duplicate-heading", "heading-without-item", "malformed-heading",
    }:
        backlog = candidate / "BACKLOG.md"
        source = backlog.read_text()
        if mutation in {"preamble-only", "unlisted-section-only"}:
            source = source.replace("last-verified: 2026-09-02", "last-verified: 2026-09-01", 1)
            receipt["changed_ids"] = []
        if mutation in {"preamble-edit", "preamble-only"}:
            source = "attacker-owned preamble\n" + source
        elif mutation in {"unlisted-section", "unlisted-section-only"}:
            source = source.replace("### B-001 — fixture", "### B-001 — attacker fixture", 1)
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
