"""Pure validation of a candidate ledger and its complete WP catalog contracts."""

from __future__ import annotations

from pathlib import Path


def validate(
    api,
    backlog_text: str,
    ledger_text: str,
    catalog_bytes: dict[str, bytes],
    repo_root: Path,
    expected_base_sha: str,
) -> int:
    """Apply the same full contract invariants on main and before PR merge."""
    open_ids = api.open_backlog_ids(backlog_text)
    backlog_ids = api.all_backlog_ids(backlog_text)
    status_counts = api.backlog_status_counts(backlog_text)
    ledger_source = "docs/campaigns/remediation/BACKLOG-WP-LEDGER.md"
    ledger_state = api.parse_ledger_state(ledger_text, ledger_source)
    catalog_counts: dict[str, int] = {}
    assignments: list[tuple[str, str, str]] = []
    valid_wps: set[str] = set()
    sections: dict[str, str] = {}
    source_by_wp: dict[str, str] = {}
    ownership_rows: list[tuple[str, str, str]] = []
    workflow_rows: list[tuple[str, str, str]] = []
    ownership_fence_count = 0
    workflow_fence_count = 0
    for path, (lower, upper) in api.CATALOGS.items():
        source = path.relative_to(api.REPO_ROOT).as_posix()
        try:
            text = catalog_bytes[source].decode("utf-8")
        except (KeyError, UnicodeDecodeError) as exc:
            raise api.LedgerError(f"missing or non-UTF-8 catalog: {source}") from exc
        if api.strict_fence(text, "wp-editable-allowlist", source) is not None:
            ownership_fence_count += 1
        ownership_rows.extend(api.parse_structured_allowlist(text, source))
        if api.strict_fence(text, api.WORKFLOW_OWNERSHIP_FENCE, source) is not None:
            workflow_fence_count += 1
        workflow_rows.extend(api.parse_workflow_ownership(text, source))
        names = api.declared_wp_names(text, source)
        valid_wps.update(names)
        rows = api.parse_catalog(text, source, lower, upper)
        catalog_counts[f"B{lower:03d}-B{upper:03d}"] = len(rows)
        for item_id, wp in rows:
            section = api.contract_section(text, wp, source)
            api.validate_contract_section(section, wp, source)
            sections.setdefault(wp, section)
            source_by_wp.setdefault(wp, source)
            assignments.append((item_id, wp, source))
    api.compare(open_ids, assignments, valid_wps)
    api.validate_ledger_state(
        ledger_state,
        source=ledger_source,
        item_count=sum(status_counts.values()),
        status_counts=status_counts,
        catalog_counts=catalog_counts,
        expected_base_sha=expected_base_sha,
    )
    api.validate_wp_dependency_order(
        api.parse_wp_dependency_order(ledger_text, ledger_source),
        valid_wps,
        ledger_source,
        required={
            "WP-140": (),
            "WP-146": (),
            "WP-148": ("WP-140", "WP-146"),
            "WP-150": ("WP-148",),
        },
    )
    for wp, section in sections.items():
        api.validate_predecessors(section, wp, source_by_wp[wp], backlog_ids, valid_wps)
    if ownership_fence_count != 1:
        raise api.LedgerError(
            f"expected exactly one editable allowlist fence, found {ownership_fence_count}"
        )
    api.validate_structured_allowlist(ownership_rows, valid_wps)
    if workflow_fence_count != 1:
        raise api.LedgerError(
            f"expected exactly one workflow ownership fence, found {workflow_fence_count}"
        )
    api.validate_workflow_ownership(
        workflow_rows, valid_wps, api.validate_workflow_population(repo_root)
    )
    return len(assignments)
