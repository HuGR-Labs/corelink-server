"""BASE-derived append-only backlog ledger successor policy."""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
from pathlib import Path
from types import SimpleNamespace


def install(api):
    REPO_ROOT = api.REPO_ROOT
    CATALOGS = api.CATALOGS
    SNAPSHOT_DIRECTORY = api.SNAPSHOT_DIRECTORY
    LEDGER_RELATIVE = api.LEDGER_RELATIVE
    GENESIS_SNAPSHOT_RELATIVE = api.GENESIS_SNAPSHOT_RELATIVE
    SUCCESSOR_NAME_RE = api.SUCCESSOR_NAME_RE
    RECEIPT_FIELDS = api.RECEIPT_FIELDS
    LedgerError = api.LedgerError
    load_postmerge_snapshot_manifest = api.load_postmerge_snapshot_manifest
    backlog_verify = api.backlog_verify
    backlog_status_counts = api.backlog_status_counts
    open_backlog_ids = api.open_backlog_ids
    parse_ledger_state = api.parse_ledger_state
    canonical_id = api.canonical_id
    validate_complete_catalog_state = api.validate_complete_catalog_state

    def _sha256(raw: bytes) -> str:
        return hashlib.sha256(raw).hexdigest()

    def _catalog_relatives() -> tuple[Path, ...]:
        return tuple(path.relative_to(REPO_ROOT) for path in CATALOGS)

    def _regular_bytes(root: Path, relative: Path) -> bytes:
        """Read candidate data without following a symlink out of its checkout."""
        if relative.is_absolute() or ".." in relative.parts:
            raise LedgerError(f"unsafe data path: {relative}")
        current = root
        for part in relative.parts:
            current = current / part
            if current.is_symlink():
                raise LedgerError(f"symlink data path: {relative}")
        if not current.is_file():
            raise LedgerError(f"missing regular data file: {relative}")
        if current.stat().st_size > 2_000_000:
            raise LedgerError(f"oversized data file: {relative}")
        return current.read_bytes()

    def _git_bytes(repo_root: Path, commit: str, relative: Path) -> bytes:
        if not re.fullmatch(r"[0-9a-f]{40}", commit):
            raise LedgerError(f"invalid immutable base SHA: {commit!r}")
        result = subprocess.run(
            ["git", "show", f"{commit}:{relative.as_posix()}"],
            cwd=repo_root,
            check=False,
            capture_output=True,
        )
        if result.returncode:
            raise LedgerError(f"immutable base lacks {relative}: {commit}")
        return result.stdout

    def _successor_paths(root: Path) -> list[Path]:
        directory = root / SNAPSHOT_DIRECTORY
        if directory.is_symlink() or not directory.is_dir():
            raise LedgerError("snapshot directory is missing or symlinked")
        paths: list[Path] = []
        for path in directory.iterdir():
            if not path.name.startswith("backlog-ledger-snapshot-v"):
                continue
            match = SUCCESSOR_NAME_RE.fullmatch(path.name)
            if match is None:
                raise LedgerError(f"malformed successor snapshot name: {path.name}")
            paths.append(SNAPSHOT_DIRECTORY / path.name)
        paths.sort()
        numbers = [
            int(SUCCESSOR_NAME_RE.fullmatch(path.name).group(1)) for path in paths
        ]
        if numbers != list(range(3, len(paths) + 3)):
            raise LedgerError(
                "successor snapshot sequence is not contiguous from v0003"
            )
        return paths

    def _receipt(raw: bytes, source: str) -> dict[str, object]:
        def unique_pairs(pairs: list[tuple[str, object]]) -> dict[str, object]:
            result: dict[str, object] = {}
            for key, value in pairs:
                if key in result:
                    raise LedgerError(f"{source}: duplicate snapshot key {key}")
                result[key] = value
            return result

        try:
            value = json.loads(raw, object_pairs_hook=unique_pairs)
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise LedgerError(f"{source}: malformed successor snapshot: {exc}") from exc
        if not isinstance(value, dict) or set(value) != RECEIPT_FIELDS:
            raise LedgerError(f"{source}: successor snapshot schema drifted")
        if type(value["schema_version"]) is not int or value["schema_version"] != 3:
            raise LedgerError(f"{source}: successor snapshot schema version drifted")
        if type(value["sequence"]) is not int or value["sequence"] < 3:
            raise LedgerError(f"{source}: invalid successor sequence")
        if value["transition"] != "base-derived-data":
            raise LedgerError(
                f"{source}: successor transition is not base-derived data"
            )
        for key in (
            "base_commit",
            "prior_snapshot_sha256",
            "prior_source_sha256",
            "source_sha256",
            "prior_ledger_sha256",
            "ledger_sha256",
        ):
            length = 40 if key == "base_commit" else 64
            if not isinstance(value[key], str) or not re.fullmatch(
                rf"[0-9a-f]{{{length}}}", value[key]
            ):
                raise LedgerError(f"{source}: malformed {key}")
        for key in ("prior_catalog_sha256", "catalog_sha256"):
            hashes = value[key]
            if (
                not isinstance(hashes, dict)
                or set(hashes) != {path.as_posix() for path in _catalog_relatives()}
                or any(
                    not isinstance(digest, str)
                    or not re.fullmatch(r"[0-9a-f]{64}", digest)
                    for digest in hashes.values()
                )
            ):
                raise LedgerError(f"{source}: malformed {key}")
        if type(value["item_count"]) is not int or value["item_count"] < 1:
            raise LedgerError(f"{source}: malformed item count")
        if (
            not isinstance(value["status_counts"], dict)
            or set(value["status_counts"]) != {"done", "open", "parked"}
            or any(
                type(count) is not int or count < 0
                for count in value["status_counts"].values()
            )
        ):
            raise LedgerError(f"{source}: malformed status counts")
        for key in ("open_ids", "changed_ids"):
            ids = value[key]
            if (
                not isinstance(ids, list)
                or any(
                    not isinstance(item, str) or not re.fullmatch(r"B-[0-9]{3,}", item)
                    for item in ids
                )
                or ids != sorted(set(ids))
            ):
                raise LedgerError(f"{source}: malformed {key}")
        return value

    def _state_bytes(root: Path) -> dict[str, bytes]:
        relatives = (Path("BACKLOG.md"), LEDGER_RELATIVE, *_catalog_relatives())
        return {path.as_posix(): _regular_bytes(root, path) for path in relatives}

    def _git_state_bytes(repo_root: Path, commit: str) -> dict[str, bytes]:
        relatives = (Path("BACKLOG.md"), LEDGER_RELATIVE, *_catalog_relatives())
        return {
            path.as_posix(): _git_bytes(repo_root, commit, path) for path in relatives
        }

    def _backlog_sections(raw: bytes) -> tuple[bytes, list[str], dict[str, bytes]]:
        """Preserve each complete item section and all bytes before the first item."""
        headings = list(re.finditer(rb"(?m)^### B-[^\r\n]*", raw))
        if not headings:
            raise LedgerError("BACKLOG.md has no canonical B-ID sections")
        order: list[str] = []
        sections: dict[str, bytes] = {}
        for index, heading in enumerate(headings):
            match = re.match(rb"### (B-[0-9]+)(?=\b|\W)", heading.group())
            if match is None:
                raise LedgerError("BACKLOG.md has malformed B-ID heading")
            item_id = canonical_id(match.group(1).decode("ascii"))[0]
            if item_id in sections:
                raise LedgerError(f"BACKLOG.md has duplicate heading {item_id}")
            end = headings[index + 1].start() if index + 1 < len(headings) else len(raw)
            section = raw[heading.start() : end]
            try:
                items = backlog_verify.parse(section.decode("utf-8"))
            except UnicodeDecodeError as exc:
                raise LedgerError(f"BACKLOG.md section {item_id} is not UTF-8") from exc
            if len(items) != 1 or items[0].id != item_id:
                raise LedgerError(
                    f"BACKLOG.md heading {item_id} lacks exactly one matching item"
                )
            order.append(item_id)
            sections[item_id] = section
        return raw[: headings[0].start()], order, sections

    def _validate_receipt_transition(
        receipt: dict[str, object],
        prior_manifest_raw: bytes,
        prior: dict[str, bytes],
        current: dict[str, bytes],
        *,
        base_sha: str,
        sequence: int,
        workflow_root: Path,
    ) -> None:
        """Derive every receipt assertion from immutable BASE and candidate bytes."""
        expected_hashes = {
            "prior_snapshot_sha256": _sha256(prior_manifest_raw),
            "prior_source_sha256": _sha256(prior["BACKLOG.md"]),
            "source_sha256": _sha256(current["BACKLOG.md"]),
            "prior_ledger_sha256": _sha256(prior[LEDGER_RELATIVE.as_posix()]),
            "ledger_sha256": _sha256(current[LEDGER_RELATIVE.as_posix()]),
        }
        if receipt["base_commit"] != base_sha or receipt["sequence"] != sequence:
            raise LedgerError(
                "successor snapshot has stale/replayed immutable base or sequence"
            )
        for key, expected in expected_hashes.items():
            if receipt[key] != expected:
                raise LedgerError(
                    f"successor snapshot {key} differs from BASE-derived bytes"
                )
        for name, state in (
            ("prior_catalog_sha256", prior),
            ("catalog_sha256", current),
        ):
            expected = {
                path.as_posix(): _sha256(state[path.as_posix()])
                for path in _catalog_relatives()
            }
            if receipt[name] != expected:
                raise LedgerError(
                    f"successor snapshot {name} differs from BASE-derived bytes"
                )
        new_text = current["BACKLOG.md"].decode("utf-8")
        old_preamble, old_order, old_sections = _backlog_sections(prior["BACKLOG.md"])
        new_preamble, new_order, new_sections = _backlog_sections(current["BACKLOG.md"])
        if old_preamble != new_preamble:
            raise LedgerError("successor changed immutable BACKLOG preamble")
        if new_order[: len(old_order)] != old_order:
            raise LedgerError("successor reordered or deleted BASE backlog sections")
        changed = sorted(
            item_id
            for item_id in new_order
            if old_sections.get(item_id) != new_sections[item_id]
        )
        if receipt["changed_ids"] != changed:
            raise LedgerError(
                "successor changed IDs differ from BACKLOG section byte delta"
            )
        counts = backlog_status_counts(new_text)
        if receipt["status_counts"] != {
            key: counts.get(key, 0) for key in ("done", "open", "parked")
        } or receipt["item_count"] != sum(counts.values()):
            raise LedgerError("successor status counts differ from BACKLOG")
        open_ids = sorted(open_backlog_ids(new_text))
        if receipt["open_ids"] != open_ids:
            raise LedgerError("successor open IDs differ from BACKLOG")
        ledger_text = current[LEDGER_RELATIVE.as_posix()].decode("utf-8")
        state = parse_ledger_state(ledger_text, str(LEDGER_RELATIVE))
        if state["base-ref"] != base_sha:
            raise LedgerError("successor ledger base-ref differs from immutable BASE")
        validate_complete_catalog_state(
            new_text,
            ledger_text,
            {
                path.as_posix(): current[path.as_posix()]
                for path in _catalog_relatives()
            },
            workflow_root,
            base_sha,
        )

    def load_successor_chain(root: Path = REPO_ROOT) -> dict[str, object]:
        """Replay append-only receipts against immutable delivered-main preimages."""
        previous = load_postmerge_snapshot_manifest()
        previous_path = GENESIS_SNAPSHOT_RELATIVE
        previous_raw = _regular_bytes(root, previous_path)
        paths = _successor_paths(root)
        for index, path in enumerate(paths):
            raw = _regular_bytes(root, path)
            receipt = _receipt(raw, path.as_posix())
            base_sha = receipt["base_commit"]
            introduced = subprocess.run(
                [
                    "git",
                    "log",
                    "--first-parent",
                    "--full-history",
                    "--format=%H",
                    "--",
                    path.as_posix(),
                ],
                cwd=root,
                check=False,
                capture_output=True,
                text=True,
            )
            commits = introduced.stdout.splitlines()
            if introduced.returncode or len(commits) != 1:
                raise LedgerError(
                    f"{path}: successor introduction is not unique on main history"
                )
            first_parent = subprocess.run(
                ["git", "rev-parse", f"{commits[0]}^1"],
                cwd=root,
                check=False,
                capture_output=True,
                text=True,
            ).stdout.strip()
            if first_parent != base_sha or _git_bytes(root, commits[0], path) != raw:
                raise LedgerError(
                    f"{path}: successor was rewritten or names the wrong main parent"
                )
            if (
                subprocess.run(
                    ["git", "merge-base", "--is-ancestor", base_sha, "HEAD"],
                    cwd=root,
                    check=False,
                ).returncode
                != 0
            ):
                raise LedgerError(
                    f"{path}: immutable successor base is not an ancestor of HEAD"
                )
            if _git_bytes(root, base_sha, previous_path) != previous_raw:
                raise LedgerError(f"{path}: prior snapshot differs from immutable BASE")
            prior = _git_state_bytes(root, base_sha)
            if _sha256(prior["BACKLOG.md"]) != previous["source_sha256"]:
                raise LedgerError(f"{path}: prior BACKLOG differs from prior snapshot")
            current = _git_state_bytes(root, commits[0])
            if index + 1 < len(paths):
                next_raw = _regular_bytes(root, paths[index + 1])
                next_receipt = _receipt(next_raw, paths[index + 1].as_posix())
                if current != _git_state_bytes(root, next_receipt["base_commit"]):
                    raise LedgerError(
                        f"{path}: delivered state drifted before next successor"
                    )
            elif current != _state_bytes(root):
                raise LedgerError(
                    f"{path}: delivered state drifted after last successor"
                )
            _validate_receipt_transition(
                receipt,
                previous_raw,
                prior,
                current,
                base_sha=base_sha,
                sequence=index + 3,
                workflow_root=root,
            )
            previous, previous_raw, previous_path = receipt, raw, path
        return previous

    def validate_candidate_successor(
        base_root: Path,
        candidate_root: Path,
        *,
        base_sha: str | None = None,
        today: backlog_verify.dt.date | None = None,
    ) -> dict[str, object]:
        """BASE-owned PR check: the receipt records, but never authorizes, a delta."""
        resolved = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=base_root,
            check=False,
            capture_output=True,
            text=True,
        ).stdout.strip()
        if not re.fullmatch(r"[0-9a-f]{40}", resolved) or (
            base_sha is not None and base_sha != resolved
        ):
            raise LedgerError("trusted BASE is not the immutable event SHA")
        base_sha = resolved
        previous = load_successor_chain(base_root)
        base_paths = _successor_paths(base_root)
        candidate_paths = _successor_paths(candidate_root)
        next_path = (
            SNAPSHOT_DIRECTORY
            / f"backlog-ledger-snapshot-v{len(base_paths) + 3:04d}.json"
        )
        if candidate_paths != [*base_paths, next_path]:
            raise LedgerError(
                "candidate must append exactly one next successor snapshot"
            )
        old_paths = (
            SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot.json",
            GENESIS_SNAPSHOT_RELATIVE,
            *base_paths,
        )
        for path in old_paths:
            old = _regular_bytes(base_root, path)
            if old != _git_bytes(base_root, base_sha, path):
                raise LedgerError(f"trusted BASE snapshot drifted: {path}")
            if _regular_bytes(candidate_root, path) != old:
                raise LedgerError(f"candidate rewrote prior snapshot: {path}")
        prior = _state_bytes(base_root)
        if prior != _git_state_bytes(base_root, base_sha):
            raise LedgerError("trusted BASE data differs from immutable event SHA")
        current = _state_bytes(candidate_root)
        previous_path = base_paths[-1] if base_paths else GENESIS_SNAPSHOT_RELATIVE
        raw = _regular_bytes(candidate_root, next_path)
        receipt = _receipt(raw, next_path.as_posix())
        _validate_receipt_transition(
            receipt,
            _regular_bytes(base_root, previous_path),
            prior,
            current,
            base_sha=base_sha,
            sequence=len(base_paths) + 3,
            workflow_root=candidate_root,
        )
        if receipt["prior_source_sha256"] != previous["source_sha256"]:
            raise LedgerError("candidate predecessor is not the trusted BASE snapshot")
        trusted_items = backlog_verify.parse(prior["BACKLOG.md"].decode("utf-8"))
        candidate_items = backlog_verify.parse(current["BACKLOG.md"].decode("utf-8"))
        backlog_verify.check_candidate_controls(
            candidate_root, base_root, trusted_items
        )
        errors = backlog_verify.validate_candidate_transitions(
            candidate_items,
            trusted_items,
            today or backlog_verify.dt.date.today(),
            allow_open_verify_means=True,
        )
        if errors:
            raise LedgerError(
                "candidate BACKLOG transition rejected: " + "; ".join(errors)
            )
        return receipt

    def successor_required(base_root: Path, candidate_root: Path) -> bool:
        """A changed ledger source, catalog, or snapshot requires a new receipt."""
        if _state_bytes(base_root) != _state_bytes(candidate_root):
            return True
        if _successor_paths(base_root) != _successor_paths(candidate_root):
            return True
        for path in (
            SNAPSHOT_DIRECTORY / "backlog-ledger-snapshot.json",
            GENESIS_SNAPSHOT_RELATIVE,
            *_successor_paths(base_root),
        ):
            if _regular_bytes(base_root, path) != _regular_bytes(candidate_root, path):
                return True
        return False

    return SimpleNamespace(
        _sha256=_sha256,
        _catalog_relatives=_catalog_relatives,
        _state_bytes=_state_bytes,
        _successor_paths=_successor_paths,
        load_successor_chain=load_successor_chain,
        validate_candidate_successor=validate_candidate_successor,
        successor_required=successor_required,
    )
