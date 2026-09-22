"""Offline B-063 readback controls; no Cloudflare or PagerDuty calls."""

import json

import blake3
import pytest

from scripts import read_b063_archive_partition as readback


def test_query_posts_select_only_to_canonical_d1(monkeypatch):
    seen = []

    class Response:
        status = 200

        def __enter__(self):
            return self

        def __exit__(self, *_):
            return False

        def read(self, *_):
            return (b'{"success":true,"result":[{"success":true,"results":[],"meta":'
                    b'{"changed_db":false,"rows_written":0,"served_by_primary":true}}]}')

    def fake_open(request, timeout):
        seen.append((request, timeout))
        return Response()

    monkeypatch.setattr(readback.urllib.request, "urlopen", fake_open)
    assert readback.query("SELECT 1", "account", "secret") == []
    request, timeout = seen[0]
    assert request.full_url.endswith(f"/d1/database/{readback.DATABASE_ID}/query")
    assert request.get_method() == "POST"
    assert json.loads(request.data)["sql"] == "SELECT 1"
    assert timeout == 60
    assert "pagerduty" not in request.full_url


@pytest.mark.parametrize("meta", [
    None,
    {},
    {"rows_written": 0, "served_by_primary": True},
    {"changed_db": False, "served_by_primary": True},
    {"changed_db": False, "rows_written": 0},
    {"changed_db": False, "rows_written": 0, "served_by_primary": False},
    {"changed_db": False, "rows_written": 0, "served_by_primary": None},
    {"changed_db": True, "rows_written": 0, "served_by_primary": True},
    {"changed_db": False, "rows_written": 1, "served_by_primary": True},
    {"changed_db": False, "rows_written": False, "served_by_primary": True},
])
def test_query_rejects_missing_or_unsafe_d1_meta(monkeypatch, meta):
    class Response:
        status = 200

        def __enter__(self):
            return self

        def __exit__(self, *_):
            return False

        def read(self, *_):
            return json.dumps({
                "success": True,
                "result": [{"success": True, "results": [], "meta": meta}],
            }).encode()

    monkeypatch.setattr(readback.urllib.request, "urlopen", lambda *_args, **_kwargs: Response())
    with pytest.raises(ValueError, match="provenance"):
        readback.query("SELECT 1", "account", "secret")


def test_indeterminate_read_exits_two_without_green_output(monkeypatch, capsys):
    monkeypatch.setenv("OWNER_APPROVED_READONLY", "1")
    monkeypatch.setenv("CF_ACCOUNT_ID", "account")
    monkeypatch.setenv("CF_API_TOKEN", "do-not-print-this")
    monkeypatch.setattr(readback, "sample", lambda *_: (_ for _ in ()).throw(ValueError("unsafe")))
    assert readback.main() == 2
    captured = capsys.readouterr()
    assert captured.out == ""
    assert "indeterminate" in captured.err
    assert "do-not-print-this" not in captured.err


def test_full_population_zero_with_exact_canary_control(monkeypatch):
    queries = []

    def fake_query(sql, account, token):
        queries.append(sql)
        if "AS target_rows" in sql:
            return [{
                "target_rows": 188,
                "target_active_rows": 0,
                "target_archived_rows": 180,
                "target_quarantined_rows": 8,
            }]
        return []

    monkeypatch.setattr(readback, "query", fake_query)
    result = readback.sample("account", "token", 1788638262347)
    assert result["failed_partitions"] == 0
    assert result["exact_target_failed"] is False
    assert "GROUP BY o.tenant_id, o.region" in queries[0]
    assert "o.quarantined_at IS NULL" in queries[0]
    assert readback.TARGET_TENANT not in queries[0]
    assert f"tenant_id='{readback.TARGET_TENANT}'" in queries[1]
    assert "substr(" not in queries[1]
    assert result["exact_target_archived_rows"] == 180
    assert result["exact_target_quarantined_rows"] == 8
    assert result["exact_target_replay_verdict"] == "empty_active_queue"


def test_sample_replays_the_exact_active_partition_without_returning_payload(monkeypatch):
    previous = "00" * 32
    active_rows = []
    for sequence in range(2):
        canonical = f'{{"n":{sequence}}}'
        digest = blake3.blake3(bytes.fromhex(previous) + canonical.encode()).hexdigest()
        active_rows.append({
            "id": f"row-{sequence}",
            "tenant_id": readback.TARGET_TENANT,
            "region": readback.TARGET_REGION,
            "sequence_number": sequence,
            "prev_hash": previous,
            "chain_hash": digest,
            "enqueued_at": 1_787_824_088_488,
            "canonical_jcs": canonical,
            "algorithm_id": None,
            "epoch_id": None,
            "link_key_id": None,
        })
        previous = digest

    def fake_query(sql, *_):
        if "AS target_rows" in sql:
            return [{
                "target_rows": 2,
                "target_active_rows": 2,
                "target_archived_rows": 0,
                "target_quarantined_rows": 0,
            }]
        if "SELECT id, tenant_id, region" in sql:
            return active_rows
        return []

    monkeypatch.setattr(readback, "query", fake_query)
    result = readback.sample("account", "token", 1_788_638_262_347)
    assert result["exact_target_replay_verdict"] == "drainable"
    assert result["exact_target_verifying_prefix_rows"] == 2
    assert result["exact_target_replay_writes"] == {"d1": 0, "r2": 0}
    assert "canonical_jcs" not in json.dumps(result)


@pytest.mark.parametrize("tenant,region,target", [
    (readback.TARGET_TENANT, "enam", True),
    ("93da3f7a-other-tenant", "enam", False),
    ("different-tenant", "weur", False),
])
def test_any_failing_partition_is_red(monkeypatch, tenant, region, target):
    def fake_query(sql, *_):
        if "AS target_rows" in sql:
            return [{
                "target_rows": 188,
                "target_active_rows": 0,
                "target_archived_rows": 180,
                "target_quarantined_rows": 8,
            }]
        if "SELECT id, tenant_id, region" in sql:
            return []
        return [{"tenant_id": tenant, "region": region, "pending_old": 188}]

    monkeypatch.setattr(readback, "query", fake_query)
    result = readback.sample("account", "token", 1788638262347)
    assert result["failed_partitions"] == 1
    assert result["exact_target_failed"] is target


@pytest.mark.parametrize("control", [
    [],
    [{
        "target_rows": 0,
        "target_active_rows": 0,
        "target_archived_rows": 0,
        "target_quarantined_rows": 0,
    }],
    [{
        "target_rows": "188",
        "target_active_rows": 0,
        "target_archived_rows": 0,
        "target_quarantined_rows": 0,
    }],
    [{
        "target_rows": 188,
        "target_active_rows": "0",
        "target_archived_rows": 188,
        "target_quarantined_rows": 0,
    }],
])
def test_empty_or_malformed_canary_control_is_indeterminate(monkeypatch, control):
    monkeypatch.setattr(
        readback, "query",
        lambda sql, *_: control if "AS target_rows" in sql else [],
    )
    with pytest.raises(ValueError, match="control|state"):
        readback.sample("account", "token", 1788638262347)


def test_three_samples_red_exit_and_no_secret_in_output(monkeypatch, capsys):
    monkeypatch.setenv("OWNER_APPROVED_READONLY", "1")
    monkeypatch.setenv("CF_ACCOUNT_ID", "account")
    monkeypatch.setenv("CF_API_TOKEN", "do-not-print-this")
    monkeypatch.setattr(readback.time, "sleep", lambda _: None)
    monkeypatch.setattr(readback.time, "time", lambda: 1788638262.347)
    calls = []

    def fake_sample(*_):
        calls.append(1)
        return {
            "failed_partitions": 1 if len(calls) == 2 else 0,
            "exact_target_replay_verdict": "empty_active_queue",
        }

    monkeypatch.setattr(readback, "sample", fake_sample)
    assert readback.main() == 1
    output = capsys.readouterr().out
    assert len(calls) == 3
    assert json.loads(output)["verdict"] == "RECOVERY_REQUIRED"
    assert "do-not-print-this" not in output


def test_zero_verdict_is_observation_not_recovery(monkeypatch, capsys):
    monkeypatch.setenv("OWNER_APPROVED_READONLY", "1")
    monkeypatch.setenv("CF_ACCOUNT_ID", "account")
    monkeypatch.setenv("CF_API_TOKEN", "secret")
    monkeypatch.setattr(readback.time, "sleep", lambda _: None)
    monkeypatch.setattr(
        readback,
        "sample",
        lambda *_: {
            "failed_partitions": 0,
            "exact_target_replay_verdict": "empty_active_queue",
        },
    )
    assert readback.main() == 0
    assert json.loads(capsys.readouterr().out)["verdict"] == "REPLAY_AND_PARTITION_CHECKS_OK"
