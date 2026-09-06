from pathlib import Path

from scripts import verify_b155_backlog_grep_population as census


ROOT = Path(__file__).resolve().parents[1]


def _record_block(backlog: str, record_id: str):
    matches = [
        match
        for match in census.FENCE.finditer(backlog)
        if f"id: {record_id}\n" in match.group(1)
    ]
    assert len(matches) == 1
    return matches[0]


def _mutate_record(backlog: str, record_id: str, guarded: str, unguarded: str) -> str:
    block = _record_block(backlog, record_id)
    body = block.group(1)
    assert body.count(guarded) == 1
    mutated = body.replace(guarded, unguarded, 1)
    return backlog[: block.start(1)] + mutated + backlog[block.end(1) :]


def test_b071_b073_are_absent_from_b155_comment_sensitive_census() -> None:
    backlog = (ROOT / "BACKLOG.md").read_text(encoding="utf-8")
    result = census.census(backlog)
    assert not {
        check.record_id for check in result.unsafe
    }.intersection({"B-071", "B-073"})


def test_b071_b073_comment_guard_mutations_reopen_census() -> None:
    backlog = (ROOT / "BACKLOG.md").read_text(encoding="utf-8")
    mutations = (
        (
            "B-071",
            'grep -Eq "^[^#]*GC_LIVE_DELETE=false"',
            'grep -Eq "GC_LIVE_DELETE=false"',
        ),
        (
            "B-073",
            'grep -q "^[^#/*-]*invitation_token_hash"',
            'grep -q "invitation_token_hash"',
        ),
        (
            "B-073",
            'grep -q "^[^#/*-]*invitation_token_hash"',
            'grep -q "^[^/*-]*invitation_token_hash"',
        ),
    )
    for record_id, guarded, unguarded in mutations:
        mutated = _mutate_record(backlog, record_id, guarded, unguarded)
        result = census.census(mutated)
        assert any(check.record_id == record_id for check in result.unsafe)
