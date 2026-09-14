#!/usr/bin/env python3
"""Run the content-locked B-139 Semgrep bundle and emit canonical evidence."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import hmac
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

from verify_b139_semgrep import BUNDLED, VerificationError, evaluate


ROOT = Path(__file__).resolve().parents[1]
LOCKFILE = ROOT / "semgrep-bundled-lock.json"
OUTPUT_NAMES = {
    "bundled": "semgrep-bundled.sarif",
    "custom": "semgrep-custom.sarif",
    "report": "semgrep-b139-report.json",
}
MAX_RULESET_BYTES = 32 * 1024 * 1024
VOLATILE_KEYS = {
    "account",
    "arguments",
    "baselineGuid",
    "commandLine",
    "correlationGuid",
    "endTimeUtc",
    "environmentVariables",
    "guid",
    "machine",
    "processId",
    "startTimeUtc",
    "workingDirectory",
}
# These SARIF arrays are sets for this scanner's evidence semantics. Arrays
# carrying ordered execution/location semantics (locations, codeFlows, stacks,
# fixes, artifacts) are deliberately preserved.
UNORDERED_ARRAY_KEYS = {
    "extensions",
    "notifications",
    "relatedLocations",
    "results",
    "rules",
    "runs",
    "tags",
    "taxonomies",
}


def _json_key(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def _normalize_value(
    value: Any,
    *,
    key: str | None,
    replacements: tuple[tuple[str, str], ...],
    dotted_rule_prefix: tuple[str, str] | None = None,
    path: tuple[str, ...] = (),
) -> Any:
    if isinstance(value, dict):
        return {
            item_key: _normalize_value(
                item,
                key=item_key,
                replacements=replacements,
                dotted_rule_prefix=dotted_rule_prefix,
                path=path + (item_key,),
            )
            for item_key, item in value.items()
            if item_key not in VOLATILE_KEYS
        }
    if isinstance(value, list):
        normalized = [
            _normalize_value(
                item,
                key=None,
                replacements=replacements,
                dotted_rule_prefix=dotted_rule_prefix,
                path=path,
            )
            for item in value
        ]
        if key in UNORDERED_ARRAY_KEYS:
            normalized.sort(key=_json_key)
        return normalized
    if isinstance(value, str):
        for original, replacement in replacements:
            value = value.replace(original, replacement)
        # Semgrep may encode a temporary absolute config path as a dotted
        # rule ID (``<absolute-dotted-rules-dir>.<config>.<rule>``), so the slash
        # replacement above cannot see it. Restrict this substitution to rule
        # identifiers and a prefix match: ordinary finding text containing a
        # similar name must remain untouched.
        metadata_path = (
            ("tool", "driver", "rules", "name") == path[-4:]
            or ("tool", "driver", "rules", "shortDescription", "text") == path[-5:]
            or (
                "invocations",
                "toolExecutionNotifications",
                "message",
                "text",
            )
            == path[-4:]
        )
        if dotted_rule_prefix is not None and (
            key in {"id", "ruleId"} or metadata_path
        ):
            original, replacement = dotted_rule_prefix
            notification_path = (
                "invocations",
                "toolExecutionNotifications",
                "message",
                "text",
            ) == path[-4:]
            if notification_path:
                # Semgrep embeds this path in diagnostic sentences (with
                # several different lead-ins); scope replacement to this
                # notification field and the exact invocation prefix.
                value = value.replace(original, replacement)
            elif value.startswith(original):
                value = replacement + value[len(original) :]
        return value
    return value


def _canonical_json(
    path: Path, *volatile_roots: Path, dotted_rule_root: Path | None = None
) -> None:
    """Normalize Semgrep/SARIF volatility while preserving ordered semantics."""

    if not path.is_file() or path.is_symlink():
        raise VerificationError(f"Semgrep did not emit regular SARIF: {path}")
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise VerificationError(f"invalid SARIF {path}: {exc}") from exc
    if (
        not isinstance(data, dict)
        or not isinstance(data.get("runs"), list)
        or not data["runs"]
    ):
        raise VerificationError(f"SARIF has no runs: {path}")
    for run in data["runs"]:
        if not isinstance(run, dict) or not isinstance(run.get("results"), list):
            raise VerificationError(f"SARIF has malformed run: {path}")
    root_labels: dict[str, str] = {}
    for index, root in enumerate(volatile_roots):
        root_labels.setdefault(str(root.resolve()), f"%B139_ROOT_{index}%")
    replacements = tuple(
        sorted(root_labels.items(), key=lambda item: (-len(item[0]), item[1]))
    )
    dotted_rule_prefix = None
    if dotted_rule_root is not None:
        # Keep the exact path spelling supplied to Semgrep. On macOS,
        # ``/var`` commonly resolves to ``/private/var`` but Semgrep's dotted
        # rule IDs retain the former spelling.
        rules_dir = str(dotted_rule_root)
        rules_label = root_labels[str(dotted_rule_root.resolve())]
        dotted_rule_prefix = (
            f"{rules_dir.lstrip('/').replace('/', '.')}.",
            f"{rules_label}.",
        )
    normalized = _normalize_value(
        data,
        key=None,
        replacements=replacements,
        dotted_rule_prefix=dotted_rule_prefix,
    )
    path.write_text(
        json.dumps(normalized, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def _load_lock(path: Path = LOCKFILE) -> dict[str, Any]:
    if not path.is_file() or path.is_symlink():
        raise VerificationError(f"missing/non-regular Semgrep lock: {path}")
    try:
        lock = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise VerificationError(f"invalid Semgrep lock {path}: {exc}") from exc
    if not isinstance(lock, dict) or set(lock) != {
        "schema_version",
        "semgrep_version",
        "captured_at",
        "provenance",
        "snapshot_dir",
        "rulesets",
    }:
        raise VerificationError("Semgrep lock fields are not closed")
    if lock["schema_version"] != 1 or lock["semgrep_version"] != "1.164.0":
        raise VerificationError("Semgrep lock schema/version changed")
    if not isinstance(lock["captured_at"], str) or not re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", lock["captured_at"]):
        raise VerificationError("Semgrep lock provenance is malformed")
    if not isinstance(lock["provenance"], str) or not lock["provenance"].strip():
        raise VerificationError("Semgrep lock provenance is malformed")
    if lock["snapshot_dir"] != "semgrep-rulesets":
        raise VerificationError("Semgrep snapshot directory changed")
    rulesets = lock["rulesets"]
    names = (
        [item.get("name") for item in rulesets if isinstance(item, dict)]
        if isinstance(rulesets, list)
        else []
    )
    if names != list(BUNDLED):
        raise VerificationError("Semgrep lock population/order changed")
    for index, item in enumerate(rulesets):
        if not isinstance(item, dict) or set(item) != {
            "name", "sha256", "url", "snapshot", "size_bytes"
        }:
            raise VerificationError("Semgrep ruleset lock fields are not closed")
        expected_url = f"https://semgrep.dev/c/{item['name']}"
        if item["url"] != expected_url:
            raise VerificationError(
                f"Semgrep ruleset URL is not canonical: {item['name']}"
            )
        if not isinstance(item["sha256"], str) or not re.fullmatch(
            r"[0-9a-f]{64}", item["sha256"]
        ):
            raise VerificationError(
                f"Semgrep ruleset digest is invalid: {item['name']}"
            )
        if not isinstance(item["snapshot"], str) or item["snapshot"] != f"{index:02d}-{item['name'].removeprefix('p/')}.yml.gz":
            raise VerificationError(f"Semgrep snapshot name is invalid: {item['name']}")
        if isinstance(item["size_bytes"], bool) or not isinstance(item["size_bytes"], int) or not 0 < item["size_bytes"] <= MAX_RULESET_BYTES:
            raise VerificationError(f"Semgrep snapshot size is invalid: {item['name']}")
    return lock


def _materialize_rulesets(
    lock: dict[str, Any],
    directory: Path,
) -> list[str]:
    configs: list[str] = []
    snapshot_root = ROOT / lock["snapshot_dir"]
    if snapshot_root.is_symlink() or not snapshot_root.is_dir():
        raise VerificationError("Semgrep snapshot directory is missing/non-regular")
    snapshot_root = snapshot_root.resolve()
    for index, item in enumerate(lock["rulesets"]):
        snapshot = snapshot_root / item["snapshot"]
        if snapshot.parent != snapshot_root or snapshot.is_symlink() or not snapshot.is_file():
            raise VerificationError(f"Semgrep snapshot is missing/non-regular: {item['name']}")
        if snapshot.stat().st_size > MAX_RULESET_BYTES:
            raise VerificationError(f"Semgrep compressed snapshot is too large: {item['name']}")
        try:
            with gzip.open(snapshot, "rb") as stream:
                content = stream.read(MAX_RULESET_BYTES + 1)
        except (OSError, EOFError) as exc:
            raise VerificationError(f"Semgrep snapshot cannot be decompressed: {item['name']}") from exc
        if len(content) != item["size_bytes"] or len(content) > MAX_RULESET_BYTES:
            raise VerificationError(f"Semgrep snapshot size mismatch: {item['name']}")
        actual = hashlib.sha256(content).hexdigest()
        if not hmac.compare_digest(actual, item["sha256"]):
            raise VerificationError(f"Semgrep ruleset digest mismatch: {item['name']}")
        destination = directory / f"{index:02d}-{item['name'].removeprefix('p/')}.yml"
        destination.write_bytes(content)
        configs.append(str(destination))
    return configs


def _verify_semgrep_version(semgrep: Path, expected: str) -> None:
    completed = subprocess.run(
        [str(semgrep), "--version"],
        cwd=ROOT,
        check=False,
        text=True,
        capture_output=True,
    )
    versions = re.findall(r"(?m)^([0-9]+\.[0-9]+\.[0-9]+)\s*$", completed.stdout)
    if completed.returncode != 0 or versions != [expected]:
        raise VerificationError(f"Semgrep executable is not locked version {expected}")


def _scan_command(
    semgrep: Path, configs: list[str], output: Path, target: Path
) -> list[str]:
    command = [str(semgrep), "scan"]
    for config in configs:
        command.extend(("--config", config))
    command.extend(
        (
            "--sarif",
            "--output",
            str(output),
            "--metrics",
            "off",
            "--jobs",
            "1",
            "--disable-version-check",
            str(target),
        )
    )
    return command


def run_bundle(
    semgrep: Path,
    output_dir: Path,
    target: Path,
    *,
    lock_path: Path | None = None,
) -> int:
    if not semgrep.is_file() or not os.access(semgrep, os.X_OK):
        raise VerificationError(
            f"Semgrep executable is missing or not executable: {semgrep}"
        )
    lock = _load_lock(LOCKFILE if lock_path is None else lock_path)
    _verify_semgrep_version(semgrep, lock["semgrep_version"])
    output_dir.mkdir(parents=True, exist_ok=True)
    if output_dir.is_symlink():
        raise VerificationError(f"output directory must not be a symlink: {output_dir}")

    bundled = output_dir / OUTPUT_NAMES["bundled"]
    custom = output_dir / OUTPUT_NAMES["custom"]
    report = output_dir / OUTPUT_NAMES["report"]
    for path in (bundled, custom, report):
        if path.is_symlink():
            raise VerificationError(f"refusing symlink output path: {path}")
        if path.exists():
            if not path.is_file():
                raise VerificationError(f"refusing non-regular output path: {path}")
            path.unlink()

    with tempfile.TemporaryDirectory(prefix="corelink-b139-rules-") as temporary:
        rules_dir = Path(temporary)
        configs = _materialize_rulesets(lock, rules_dir)
        bundled_run = subprocess.run(
            _scan_command(semgrep, configs, bundled, target), cwd=ROOT, check=False
        )
        custom_run = subprocess.run(
            _scan_command(semgrep, ["./semgrep.yml"], custom, target),
            cwd=ROOT,
            check=False,
        )
        _canonical_json(
            bundled, ROOT, output_dir, rules_dir, target, dotted_rule_root=rules_dir
        )
        _canonical_json(
            custom, ROOT, output_dir, rules_dir, target, dotted_rule_root=rules_dir
        )

    return evaluate(
        bundled, custom, report, bundled_run.returncode, custom_run.returncode
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--semgrep-bin", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, default=Path("."))
    parser.add_argument("--target", type=Path, default=Path("."))
    parser.add_argument("--lock", type=Path, default=LOCKFILE)
    args = parser.parse_args(argv)
    try:
        result = run_bundle(
            args.semgrep_bin.resolve(),
            args.output_dir.absolute(),
            args.target.resolve(),
            lock_path=args.lock.absolute(),
        )
        print(
            "B139 content-locked bundle: "
            f"{'PASS' if result == 0 else 'FAIL'}; "
            f"outputs={','.join(OUTPUT_NAMES.values())}"
        )
        return result
    except (OSError, VerificationError) as exc:
        print(f"B139 bundle FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
