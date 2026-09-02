"""Contract tests for the abbreviated-citation resolver (B-059)."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT = REPO_ROOT / "scripts" / "okf_resolve_abbrev_cites.py"
SPEC = importlib.util.spec_from_file_location("okf_resolve_abbrev_cites", SCRIPT)
assert SPEC and SPEC.loader
RESOLVER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = RESOLVER
SPEC.loader.exec_module(RESOLVER)


def _scan(tmp_path: Path, concept_body: str, source_files: dict[str, str]):
    RESOLVER.REPO_ROOT = tmp_path
    concept = tmp_path / "docs" / "knowledge" / "concept.md"
    concept.parent.mkdir(parents=True)
    concept.write_text(concept_body, encoding="utf-8")
    for rel, body in source_files.items():
        source = tmp_path / rel
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text(body, encoding="utf-8")

    full_re, bare_re = RESOLVER._compile()
    return RESOLVER.scan_concept(
        concept,
        RESOLVER.SourceCache(tmp_path),
        full_re,
        bare_re,
    )


def test_inherits_across_lines_and_accepts_non_code_extensions(tmp_path: Path) -> None:
    seen, findings = _scan(
        tmp_path,
        "# Citations\n1. `migrations/d1/0074_team_member.sql:1`\n2. `:2`\n",
        {"migrations/d1/0074_team_member.sql": "CREATE TABLE x;\nCREATE INDEX x_i;\n"},
    )

    assert seen == 1
    assert findings == []


def test_repo_relative_path_cannot_escape_source_root(tmp_path: Path) -> None:
    seen, findings = _scan(
        tmp_path,
        "# Citations\n1. `../outside.rs:1`\n2. `:1`\n",
        {"outside.rs": "fn outside() {}\n"},
    )

    assert seen == 1
    assert len(findings) == 2
    assert "full-file-missing" in findings[0]
    assert "file-missing" in findings[1]


def test_rejects_zero_and_past_eof_ranges_and_malformed_bare_range(tmp_path: Path) -> None:
    seen, findings = _scan(
        tmp_path,
        "# Citations\n"
        "1. `src/live.rs:0`\n"
        "2. `src/live.rs:1-4`\n"
        "3. `src/live.rs:1` then `:999-`\n",
        {"src/live.rs": "fn live() {}\n"},
    )

    assert seen == 1
    assert any("full-line-before-start" in finding for finding in findings)
    assert any("full-past-eof" in finding for finding in findings)
    assert any("malformed-range" in finding for finding in findings)


def test_path_only_anchor_resolves_following_abbreviated_citations(tmp_path: Path) -> None:
    seen, findings = _scan(
        tmp_path,
        "# Citations\n"
        "The implementation is `crates/example/src/customer_d1.rs`; see `:1` and `:2`.\n",
        {"crates/example/src/customer_d1.rs": "impl Customer {\nfn method() {}\n}\n"},
    )

    assert seen == 2
    assert findings == []


def test_missing_named_concept_is_a_fatal_scan_failure() -> None:
    result = subprocess.run(
        [sys.executable, str(SCRIPT), "/private/tmp/okf-concept-that-does-not-exist.md"],
        cwd=REPO_ROOT,
        text=True,
        capture_output=True,
        check=False,
    )

    assert result.returncode == 2
    assert "FATAL" in result.stderr
