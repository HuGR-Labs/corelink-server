"""BASE-derived append-only backlog ledger successor policy."""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
from pathlib import Path
from types import SimpleNamespace

# A one-time trusted-control authorization for the reviewed Sprint3 correction.
# The candidate receipt is not an authority: both entire BACKLOG preimages and
# the seven rewritten sections' machine fields must match pinned SHA-256 bytes.
SPRINT3_BACKLOG_SHA256 = (
    "66e3eb81fc02f5c7eb65bad1faf0b793565234a9d2f0830e46623b25d4f28c97",
    "41726d6c8b2f4b1dc7ff35466a78c242e147b99eaca69a956064024e04212e23",
)
SPRINT3_CHANGED_IDS = ["B-012", "B-065", "B-087", "B-089", "B-097", "B-154", "B-170"]
SPRINT3_FIELDS = {
    "B-012": (
        "open",
        "07e9926943884c0fed543dc54d99eff7d0255c8fb5a92e4e16d5c23c64383e98",
        "07e9926943884c0fed543dc54d99eff7d0255c8fb5a92e4e16d5c23c64383e98",
        "e301ed123332e0b4533d487ab3688af2ddd593e7a234a0eb8830913dc08bcf57",
        "fa310cff3550ddcf700eccd78169a7377c9706351e4c040eba7374a8c5fb5d19",
    ),
    "B-065": (
        "open",
        "3e3dc027408184f216b8acc68320210b9bb3a549b7a9628b9141697e32d3a252",
        "3e3dc027408184f216b8acc68320210b9bb3a549b7a9628b9141697e32d3a252",
        "6255c16777eb66c330bbbce47af999642365ecd7a35ea93938c482c590f26f79",
        "a6ff51aa1bed4d8fa536fbe4da0b46a4889f4f1c44173bcae4d978313016124c",
    ),
    "B-087": (
        "done",
        "dd5808253fecba2d0dfc469062cfed897be9cfbb733bd5ebc8343dcaf02be3ec",
        "dd5808253fecba2d0dfc469062cfed897be9cfbb733bd5ebc8343dcaf02be3ec",
        "aa6705cddd3ae0e498f0df30109234d001666fc12b40fb659dbe2e94cb8203e3",
        "d8c3e802a44aa14b23ba4c5ac85aa457bf927830400034a47d14dd918e7e6cc4",
    ),
    "B-089": (
        "open",
        "5eda9fcb444055498b855f8a8f4861349a1c200950d638c4f12c489ee705d8eb",
        "be3ca49c4531f3c274213f35c785cdc037f48619a07308c350e1b4bfc673f60e",
        "87077367ef44d0050d30a178f5cc51854ec227afc12a28028a2dddc818e5c96c",
        "c40313446497a27c2ab46bc4838e880cc10d4f50af141ec6ebdedc585d9d8b16",
    ),
    "B-097": (
        "open",
        "c6ac2f9d6bf7f85d3f9b5a9787dec6649d32117744c0d05919e60c81205bd16b",
        "c6ac2f9d6bf7f85d3f9b5a9787dec6649d32117744c0d05919e60c81205bd16b",
        "4204daae06fa2b6ad8ee0b8e535551267b67532296ebbdd1d189c02a8a6e7099",
        "e16c7afa00b279ab513461f20286cb257c5eb706d46f109e2296bddde6484085",
    ),
    "B-154": (
        "open",
        "a6045afb801b3b6a61ff09d97b6e77ba5fa4f7e1c4f9ed0fde8554957484264e",
        "b86a04ff5d4ca729279b39631764e85c01c934e459c2db2ac23d0900587de590",
        "f3434222e0ed605f801a03a34f112e978c242a42f8c4470fda666e267939da35",
        "8d7fb8611da55cf7db8198fdf67b3cf3884eca09785b5f90871fb4792a3e59f0",
    ),
    "B-170": (
        "open",
        "91ead1ed21d307436564ac291480eb45ae690b9e3a8ed0b094f9be5026dc2afd",
        "91ead1ed21d307436564ac291480eb45ae690b9e3a8ed0b094f9be5026dc2afd",
        "fcfc27240f0995a8483ed10314af4c138886fd97b4ccaac7cf8d056a81cc18de",
        "fcfc27240f0995a8483ed10314af4c138886fd97b4ccaac7cf8d056a81cc18de",
    ),
}

# One reviewed bridge repairs a recorded v0003 successor that was followed by
# unrecorded, main-only BACKLOG drift. It does not rewrite v0003: it pins both
# sides of the gap, admits only the B-154 conjunction below, and returns the
# chain to ordinary append-only successor validation at v0004.
B154_RECONCILIATION = {
    "sequence": 4,
    "previous_sequence": 3,
    "base_commit": "80614c5e83e74099dee3c710ad126cb44a9e557f",
    "previous_source_sha256": "41726d6c8b2f4b1dc7ff35466a78c242e147b99eaca69a956064024e04212e23",
    "prior_source_sha256": "c272de9f9cc4e6ae36bddbff4c97012d8589150a22c0038e0875a6ddf855ac87",
    "source_sha256": "208c18517916ad39d262ff76fdf99765b678f9d1b768f4f107173beb21f663b4",
    "prior_ledger_sha256": "02d81ecf3ade17a5317b3f24e68a17a801837bde6112cac9288b6a7f6175b656",
    "ledger_sha256": "74299a6b9283e60958aa1ff98cb2efcd027f70dd783d4111dc68e9e51ade86c9",
    "changed_ids": ["B-154"],
    "catalog_sha256": {
        "docs/campaigns/remediation/work-packages/B001-B045.md": "2a1735804f43bb726789b99ee80c14cf876f41eb6c684e3ea54e68981413cd38",
        "docs/campaigns/remediation/work-packages/B046-B090.md": "1a164260a84f3adb406676e1505fd8379aad07b15a5c45365369dcef7b0586da",
        "docs/campaigns/remediation/work-packages/B091-B130.md": "2875d5374471382a5422ceac83e3259fcd5780e55fa3e07f80746f7ceb2995ce",
        "docs/campaigns/remediation/work-packages/B131-B167.md": "4be50ac320f2c8d238a21760bc24a5391a7a5dc1102532d9d034e05556ad089d",
    },
    "fields": (
        "open",
        "a6045afb801b3b6a61ff09d97b6e77ba5fa4f7e1c4f9ed0fde8554957484264e",
        "b86a04ff5d4ca729279b39631764e85c01c934e459c2db2ac23d0900587de590",
        "8d7fb8611da55cf7db8198fdf67b3cf3884eca09785b5f90871fb4792a3e59f0",
        "f3434222e0ed605f801a03a34f112e978c242a42f8c4470fda666e267939da35",
    ),
}


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

    def _sprint3_rewrite_authorized(
        prior: dict[str, bytes], current: dict[str, bytes],
        receipt: dict[str, object], sequence: int,
    ) -> bool:
        if (
            sequence != 3
            or (_sha256(prior["BACKLOG.md"]), _sha256(current["BACKLOG.md"]))
            != SPRINT3_BACKLOG_SHA256
            or receipt["changed_ids"] != SPRINT3_CHANGED_IDS
        ):
            return False
        old_items = {item.id: item.raw for item in backlog_verify.parse(prior["BACKLOG.md"].decode())}
        new_items = {item.id: item.raw for item in backlog_verify.parse(current["BACKLOG.md"].decode())}
        for item_id, (status, old_verify, new_verify, old_means, new_means) in SPRINT3_FIELDS.items():
            old, new = old_items.get(item_id, {}), new_items.get(item_id, {})
            if (
                old.get("status") != status or new.get("status") != status
                or _sha256(str(old.get("verify", "")).encode()) != old_verify
                or _sha256(str(new.get("verify", "")).encode()) != new_verify
                or _sha256(str(old.get("verify-means", "")).encode()) != old_means
                or _sha256(str(new.get("verify-means", "")).encode()) != new_means
            ):
                return False
        return True

    def _b154_reconciliation_authorized(
        previous: dict[str, object], prior: dict[str, bytes], current: dict[str, bytes],
        receipt: dict[str, object], sequence: int,
    ) -> bool:
        """Authorize the sole v0003-to-v0004 gap repair by complete byte binding."""
        pinned = B154_RECONCILIATION
        if (
            sequence != pinned["sequence"]
            or previous.get("sequence") != pinned["previous_sequence"]
            or previous.get("source_sha256") != pinned["previous_source_sha256"]
            or receipt.get("base_commit") != pinned["base_commit"]
            or receipt.get("changed_ids") != pinned["changed_ids"]
            or _sha256(prior["BACKLOG.md"]) != pinned["prior_source_sha256"]
            or _sha256(current["BACKLOG.md"]) != pinned["source_sha256"]
            or _sha256(prior[LEDGER_RELATIVE.as_posix()]) != pinned["prior_ledger_sha256"]
            or _sha256(current[LEDGER_RELATIVE.as_posix()]) != pinned["ledger_sha256"]
            or receipt.get("prior_source_sha256") != pinned["prior_source_sha256"]
            or receipt.get("source_sha256") != pinned["source_sha256"]
            or receipt.get("prior_ledger_sha256") != pinned["prior_ledger_sha256"]
            or receipt.get("ledger_sha256") != pinned["ledger_sha256"]
        ):
            return False
        catalog_hashes = {
            path.as_posix(): _sha256(prior[path.as_posix()])
            for path in _catalog_relatives()
        }
        if (
            catalog_hashes != pinned["catalog_sha256"]
            or {
                path.as_posix(): _sha256(current[path.as_posix()])
                for path in _catalog_relatives()
            } != catalog_hashes
            or receipt.get("prior_catalog_sha256") != catalog_hashes
            or receipt.get("catalog_sha256") != catalog_hashes
        ):
            return False
        old_items = {item.id: item.raw for item in backlog_verify.parse(prior["BACKLOG.md"].decode())}
        new_items = {item.id: item.raw for item in backlog_verify.parse(current["BACKLOG.md"].decode())}
        old, new = old_items.get("B-154", {}), new_items.get("B-154", {})
        status, old_verify, new_verify, old_means, new_means = pinned["fields"]
        return (
            old.get("status") == new.get("status") == status
            and _sha256(str(old.get("verify", "")).encode()) == old_verify
            and _sha256(str(new.get("verify", "")).encode()) == new_verify
            and _sha256(str(old.get("verify-means", "")).encode()) == old_means
            and _sha256(str(new.get("verify-means", "")).encode()) == new_means
        )

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

    def _normative_section(section: bytes, item_id: str) -> bytes:
        """Mask only canonical transition fields inside the one backlog fence.

        Every other byte, including headings, free prose, and verify-means,
        remains normative across every status transition. The parsed field
        transition is checked separately against BASE-owned rules.
        """
        fence = re.search(rb"(?ms)^```backlog\n(.*?)^```", section)
        if fence is None:
            raise LedgerError(f"{item_id}: missing canonical backlog fence")
        body = fence.group(1)
        for key in (b"status", b"owner", b"last-verified"):
            pattern = rb"(?m)^" + key + rb":[^\r\n]*$"
            body, count = re.subn(pattern, key + b": <transition>", body)
            if count != 1:
                raise LedgerError(
                    f"{item_id}: transition field {key.decode()} is not unique and canonical"
                )
        return section[: fence.start(1)] + body + section[fence.end(1) :]

    def _validate_receipt_transition(
        receipt: dict[str, object],
        prior_manifest_raw: bytes,
        prior: dict[str, bytes],
        current: dict[str, bytes],
        *,
        previous: dict[str, object] | None,
        base_sha: str,
        sequence: int,
        workflow_root: Path,
        today: backlog_verify.dt.date | None = None,
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
        sprint3_rewrite = _sprint3_rewrite_authorized(
            prior, current, receipt, sequence,
        )
        b154_reconciliation = previous is not None and _b154_reconciliation_authorized(
            previous, prior, current, receipt, sequence,
        )
        for item_id in changed:
            if item_id not in old_sections:
                continue
            if not (
                (sprint3_rewrite and item_id in SPRINT3_CHANGED_IDS)
                or (b154_reconciliation and item_id == "B-154")
            ) and (
                _normative_section(old_sections[item_id], item_id)
                != _normative_section(new_sections[item_id], item_id)
            ):
                raise LedgerError(
                    f"{item_id}: normative BACKLOG section changed without trusted byte-pinned authorization"
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
        transition_errors = backlog_verify.validate_candidate_transitions(
            backlog_verify.parse(new_text),
            backlog_verify.parse(prior["BACKLOG.md"].decode("utf-8")),
            today or backlog_verify.dt.date.today(),
            allow_sprint3_rewrite=sprint3_rewrite,
            allow_b154_reconciliation=b154_reconciliation,
            successor_mode=True,
        )
        if transition_errors:
            raise LedgerError("candidate BACKLOG transition rejected: " + "; ".join(transition_errors))

    def load_successor_chain(
        root: Path = REPO_ROOT,
        *,
        pending_receipt: dict[str, object] | None = None,
        pending_current: dict[str, bytes] | None = None,
    ) -> dict[str, object]:
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
            current = _git_state_bytes(root, commits[0])
            b154_reconciliation = _b154_reconciliation_authorized(
                previous, prior, current, receipt, index + 3,
            )
            if (
                _sha256(prior["BACKLOG.md"]) != previous["source_sha256"]
                and not b154_reconciliation
            ):
                raise LedgerError(f"{path}: prior BACKLOG differs from prior snapshot")
            if index + 1 < len(paths):
                next_raw = _regular_bytes(root, paths[index + 1])
                next_receipt = _receipt(next_raw, paths[index + 1].as_posix())
                next_current = _git_state_bytes(
                    root,
                    subprocess.run(
                        ["git", "log", "--first-parent", "--full-history", "--format=%H", "--", paths[index + 1].as_posix()],
                        cwd=root, check=False, capture_output=True, text=True,
                    ).stdout.strip(),
                )
                next_prior = _git_state_bytes(root, next_receipt["base_commit"])
                bridge = _b154_reconciliation_authorized(
                    receipt, next_prior, next_current, next_receipt, index + 4,
                )
                if current != next_prior and not bridge:
                    raise LedgerError(
                        f"{path}: delivered state drifted before next successor"
                    )
            elif current != _state_bytes(root):
                bridge = (
                    pending_receipt is not None
                    and pending_current is not None
                    and _b154_reconciliation_authorized(
                        receipt, _state_bytes(root), pending_current,
                        pending_receipt, index + 4,
                    )
                )
                if not bridge:
                    raise LedgerError(
                        f"{path}: delivered state drifted after last successor"
                    )
            _validate_receipt_transition(
                receipt,
                previous_raw,
                prior,
                current,
                previous=previous,
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
        raw = _regular_bytes(candidate_root, next_path)
        receipt = _receipt(raw, next_path.as_posix())
        previous = load_successor_chain(
            base_root,
            pending_receipt=receipt,
            pending_current=_state_bytes(candidate_root),
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
        _validate_receipt_transition(
            receipt,
            _regular_bytes(base_root, previous_path),
            prior,
            current,
            previous=previous,
            base_sha=base_sha,
            sequence=len(base_paths) + 3,
            workflow_root=candidate_root,
            today=today,
        )
        if (
            receipt["prior_source_sha256"] != previous["source_sha256"]
            and not _b154_reconciliation_authorized(
                previous, prior, current, receipt, len(base_paths) + 3,
            )
        ):
            raise LedgerError("candidate predecessor is not the trusted BASE snapshot")
        trusted_items = backlog_verify.parse(prior["BACKLOG.md"].decode("utf-8"))
        backlog_verify.check_candidate_controls(
            candidate_root, base_root, trusted_items
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
        _sprint3_rewrite_authorized=_sprint3_rewrite_authorized,
        _b154_reconciliation_authorized=_b154_reconciliation_authorized,
        _normative_section=_normative_section,
        _catalog_relatives=_catalog_relatives,
        _state_bytes=_state_bytes,
        _successor_paths=_successor_paths,
        load_successor_chain=load_successor_chain,
        validate_candidate_successor=validate_candidate_successor,
        successor_required=successor_required,
    )
