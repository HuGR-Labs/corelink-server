#!/usr/bin/env python3
"""Bounded mutation tests for the offline B-250 evidence verifier."""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import time

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import b250_deleted_workflow_startup_failure as b250


def fixture() -> dict:
    return json.loads(b250.SNAPSHOT_PATH.read_text(encoding="utf-8"))


def expect_reject(mutator, label: str) -> None:
    candidate = fixture()
    mutator(candidate)
    try:
        b250.verify(candidate)
    except b250.SnapshotError:
        return
    raise AssertionError(f"mutation was accepted: {label}")


def test_retained_population_is_hold_and_never_closure():
    report = b250.verify_retained_snapshot()
    assert report["finding"] == "B-250"
    assert report["status"] == "HOLD"
    assert report["closure_permitted"] is False
    assert report["population"]["run_count"] == 278
    assert report["population"]["job_count"] == 0
    assert report["workflow"] == {"id": 303501160, "path": "BuildFailed", "state": "deleted"}


@pytest.mark.parametrize(
    ("mutator", "label"),
    [
        (lambda value: value["population"].update(run_count=277), "count"),
        (lambda value: value["population"].update(conclusion="failure"), "conclusion"),
        (lambda value: value["population"].update(job_count=1), "jobs"),
        (lambda value: value["workflow"].update(id=9), "workflow id"),
        (lambda value: value["workflow"].update(state="active"), "workflow state"),
        (lambda value: value["owner_action"].update(closure_permitted=True), "closure"),
        (lambda value: value["population"]["run_ids"].update(count=277), "run identity count"),
        (lambda value: value["window"].update(end_exclusive="2026-09-07T00:00:00Z"), "window"),
    ],
)
def test_load_bearing_mutations_fail_closed(mutator, label):
    expect_reject(mutator, label)


def test_raw_values_or_provenance_mutation_fails_closed():
    def mutate(value):
        value["source"]["raw_api_payload_persisted"] = True

    expect_reject(mutate, "raw API payload persistence")


def test_duplicate_count_mutation_fails_closed():
    expect_reject(
        lambda value: value["population"].update(duplicate_count=1),
        "duplicate count",
    )


def test_redacted_raw_values_mutation_fails_closed():
    def mutate(value):
        value["population"]["run_ids"]["values"] = [1, 1]

    expect_reject(mutate, "raw duplicate run IDs")


@pytest.mark.parametrize("field", ["uniqueness_digest", "document_sha256"])
def test_missing_digest_mutation_fails_closed(field):
    def mutate(value):
        if field == "uniqueness_digest":
            del value["population"][field]
        else:
            del value["source"][field]

    expect_reject(mutate, f"missing {field}")


@pytest.mark.parametrize("field", ["uniqueness_digest", "document_sha256"])
def test_bad_digest_mutation_fails_closed(field):
    def mutate(value):
        target = value["population"] if field == "uniqueness_digest" else value["source"]
        target[field] = "sha256:" + "0" * 64

    expect_reject(mutate, f"bad {field}")


def test_stale_refresh_timestamp_mutation_fails_closed():
    expect_reject(
        lambda value: value["source"].update(last_refreshed="2026-08-01T00:00:00Z"),
        "stale source timestamp",
    )


def test_changed_audit_document_fails_hash_and_content_binding(tmp_path):
    changed = tmp_path / "b250-audit.md"
    changed.write_text(
        b250.AUDIT_DOCUMENT_PATH.read_text(encoding="utf-8") + "\nchanged\n",
        encoding="utf-8",
    )
    with pytest.raises(b250.SnapshotError, match="audit document hash"):
        b250.verify_retained_snapshot(audit_document=changed)


def test_unknown_snapshot_field_fails_closed():
    candidate = fixture()
    candidate["unexpected"] = True
    with pytest.raises(b250.SnapshotError, match="unknown field"):
        b250.verify(candidate)


@pytest.mark.parametrize(
    "claim",
    (
        "O verificador também consulta API em cada refresh.",
        "O verificador percorre paginação do endpoint para confirmar os runs.",
        "A consulta ao endpoint escopado permanece obrigatória.",
        "Consulta a API em cada refresh.",
        "É obrigatório percorrer a paginação do endpoint.",
        "A rede permanece necessária.",
    ),
)
def test_backlog_description_rejects_positive_or_mandatory_live_claims(tmp_path, claim):
    source = b250.BACKLOG_PATH.read_text(encoding="utf-8")
    start = source.index("### B-250 —")
    end = source.index("### B-251 —", start)
    path = tmp_path / "BACKLOG.md"
    path.write_text(source[:end] + claim + "\n" + source[end:], encoding="utf-8")
    with pytest.raises(b250.SnapshotError, match="positive or mandatory API/endpoint/pagination/network"):
        b250.verify_backlog_description(path)


@pytest.mark.parametrize(
    "claim",
    (
        "O verificador não consulta API.",
        "Não percorre a paginação do endpoint.",
        "A rede não é acessada pelo verificador.",
        "Sem API, endpoint ou rede no caminho offline.",
    ),
)
def test_backlog_description_allows_explicitly_negated_live_claims(tmp_path, claim):
    source = b250.BACKLOG_PATH.read_text(encoding="utf-8")
    start = source.index("### B-250 —")
    end = source.index("### B-251 —", start)
    path = tmp_path / "BACKLOG.md"
    path.write_text(source[:end] + claim + "\n" + source[end:], encoding="utf-8")
    b250.verify_backlog_description(path)


def test_backlog_description_allows_only_explicit_owner_external_refresh(tmp_path):
    source = b250.BACKLOG_PATH.read_text(encoding="utf-8")
    start = source.index("### B-250 —")
    end = source.index("### B-251 —", start)
    owner_refresh = (
        "O owner executa refresh externo para gerar ou atualizar o snapshot e consulta a API.\n"
    )
    path = tmp_path / "BACKLOG.md"
    path.write_text(source[:end] + owner_refresh + source[end:], encoding="utf-8")
    b250.verify_backlog_description(path)


def test_backlog_description_rejects_verifier_network_attribution_inside_owner_sentence(tmp_path):
    source = b250.BACKLOG_PATH.read_text(encoding="utf-8")
    start = source.index("### B-250 —")
    end = source.index("### B-251 —", start)
    claim = (
        "O owner executa refresh externo para gerar ou atualizar o snapshot e o verificador consulta a API.\n"
    )
    path = tmp_path / "BACKLOG.md"
    path.write_text(source[:end] + claim + source[end:], encoding="utf-8")
    with pytest.raises(b250.SnapshotError, match="claim attributed to the verifier"):
        b250.verify_backlog_description(path)


def test_backlog_description_requires_owner_refresh_provenance_controls(tmp_path):
    source = b250.BACKLOG_PATH.read_text(encoding="utf-8")
    start = source.index("### B-250 —")
    end = source.index("### B-251 —", start)
    mutated = source[:start] + source[start:end].replace(
        "O owner executa separadamente\no refresh externo autorizado para gerar ou atualizar o snapshot.",
        "O owner consulta a API.",
    ) + source[end:]
    path = tmp_path / "BACKLOG.md"
    path.write_text(mutated, encoding="utf-8")
    with pytest.raises(b250.SnapshotError, match="offline contract"):
        b250.verify_backlog_description(path)


def test_backlog_description_status_mutation_fails_closed(tmp_path):
    source = b250.BACKLOG_PATH.read_text(encoding="utf-8")
    start = source.index("### B-250 —")
    end = source.index("### B-251 —", start)
    section = source[start:end].replace("status: parked", "status: done", 1)
    path = tmp_path / "BACKLOG.md"
    path.write_text(source[:start] + section + source[end:], encoding="utf-8")
    with pytest.raises(b250.SnapshotError, match="offline contract"):
        b250.verify_backlog_description(path)


def test_offline_cli_is_bounded_and_does_not_require_gh():
    command = [sys.executable, str(pathlib.Path(__file__).with_name("b250_deleted_workflow_startup_failure.py"))]
    started = time.monotonic()
    result = subprocess.run(
        command,
        capture_output=True,
        text=True,
        check=False,
        timeout=5,
        env={"PATH": ""},
    )
    elapsed = time.monotonic() - started
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["status"] == "HOLD"
    assert elapsed < 5


def test_canonical_cli_cannot_become_a_live_api_command():
    script = str(pathlib.Path(__file__).with_name("b250_deleted_workflow_startup_failure.py"))
    for option in ("--repo", "--workflow-id", "--start", "--end", "--fetch-logs"):
        result = subprocess.run(
            [sys.executable, script, option, "mutated"],
            capture_output=True,
            text=True,
            check=False,
        )
        assert result.returncode == 2
        assert "unrecognized arguments" in result.stderr


def test_output_path_writes_hold_report(tmp_path):
    report_path = tmp_path / "report.json"
    assert b250.main(["--output", str(report_path)]) == 0
    assert json.loads(report_path.read_text(encoding="utf-8"))["closure_permitted"] is False


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-q"]))
