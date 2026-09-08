#!/usr/bin/env python3
"""Verify the executable contract of the remediation roadmap (B-061).

The roadmap is a control document, not a second source of truth for measured
state.  This guard derives the repository-local counts from the same committed
objects the OKF and billing code use, then checks the explicitly marked
canonical projection in the roadmap.  The Actions census is an external,
dated snapshot; it is checked as a complete record rather than silently
re-derived from a local workflow-file count.

Any missing input, malformed marker, unavailable git object, or workflow path
drift is an error.  A partial answer is not a green answer.
"""

from __future__ import annotations

import argparse
import ast
from functools import lru_cache
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
ROADMAP = Path("docs/campaigns/remediation/ROADMAP.md")
METRICS = Path("reports/b061-remediation-roadmap.v1.json")
ACTIONS_EXPORT = Path("reports/b061-actions-census.v1.json")
BACKLOG = Path("BACKLOG.md")
CHANGELOG = Path("changelog.d/b061-remediation-roadmap.md")
WORKFLOW = Path(".github/workflows/backlog-verify.yml")
OKF_WORKFLOW = Path(".github/workflows/okf_wiki.yml")
SCRIPT = Path("scripts/verify_b061_remediation_roadmap.py")
TEST = Path("tests/test_verify_b061_remediation_roadmap.py")
OKF_VALIDATOR = Path("scripts/validate_okf.py")
OKF_RUNTIME = Path("scripts/validate_okf_runtime.py")
OKF_MANIFEST = Path("docs/internal/okf-wiki/concept-manifest.yaml")
OKF_INDEX = Path("docs/knowledge/index.md")
OKF_INDEX_SCRIPT = Path("scripts/okf_index.py")
OKF_RENDER_SCRIPT = Path("scripts/okf_render.py")
OKF_RENDERED_SITE = Path("docs/okf-wiki-site/index.html")
OKF_SPLIT_MODULES = (
    OKF_RUNTIME,
    Path("scripts/validate_okf_core1.py"),
    Path("scripts/validate_okf_core2.py"),
    Path("scripts/validate_okf_checks.py"),
    Path("scripts/okf_anchor_reverify.py"),
)

CANONICAL_KEYS = {
    "okf_documents",
    "okf_checkpoint_claims",
    "okf_orphan_anchors",
    "tier_kind_variants",
    "rate_ladder_variants",
    "actions_zero_success_lanes",
    "actions_ran_never_success_lanes",
    "actions_never_run_lanes",
    "actions_runs_without_success",
}
CANONICAL_MARKERS = {
    "ROADMAP.md": re.compile(
        r"<!-- B061-CANONICAL-METRICS\n(?P<body>.*?)\nB061-CANONICAL-METRICS -->",
        re.DOTALL,
    ),
    "metric": re.compile(
        r"^(?P<key>[a-z0-9_]+)=(?P<value>[0-9]+|unavailable)$", re.MULTILINE
    ),
}
CHECKPOINT_RE = re.compile(r"^checkpoint_sha:\s*\"?([0-9a-f]{8,40})", re.MULTILINE)
DEFERRED_RE = re.compile(r"^deferred:\s*", re.MULTILINE)
MANIFEST_ID_RE = re.compile(r"^[A-Za-z0-9._-]+(?:/[A-Za-z0-9._-]+)+$")


def _rust_tokens(source: str) -> list[str]:
    """Tokenize Rust enough for declarations, ignoring comments and literals."""
    tokens: list[str] = []
    i = 0
    while i < len(source):
        if source.startswith("//", i):
            end = source.find("\n", i + 2)
            i = len(source) if end < 0 else end + 1
            continue
        if source.startswith("/*", i):
            depth = 1
            i += 2
            while i < len(source) and depth:
                if source.startswith("/*", i):
                    depth += 1
                    i += 2
                elif source.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            continue
        if source.startswith("r#", i) or source.startswith("r\"", i):
            # Raw strings are literals too; skip through their matching quote.
            hashes = 0
            j = i + 1
            while j < len(source) and source[j] == "#":
                hashes += 1
                j += 1
            if j < len(source) and source[j] == '"':
                close = '"' + ('#' * hashes)
                end = source.find(close, j + 1)
                i = len(source) if end < 0 else end + len(close)
                continue
        if source[i] in {'"', "'"}:
            quote = source[i]
            i += 1
            while i < len(source):
                escaped = source[i] == "\\"
                i += 2 if escaped else 1
                if not escaped and source[i - 1] == quote:
                    break
            continue
        if source[i].isspace():
            i += 1
            continue
        if source[i].isalpha() or source[i] == "_":
            j = i + 1
            while j < len(source) and (source[j].isalnum() or source[j] == "_"):
                j += 1
            tokens.append(source[i:j])
            i = j
            continue
        if source[i].isdigit():
            j = i + 1
            while j < len(source) and (source[j].isalnum() or source[j] == "_"):
                j += 1
            tokens.append(source[i:j])
            i = j
            continue
        tokens.append(source[i])
        i += 1
    return tokens


def _matching_delimiter(tokens: list[str], opening: int, left: str = "[", right: str = "]") -> int | None:
    depth = 0
    pairs = {"[": "]", "(": ")", "{": "}"}
    expected = pairs.get(left, right)
    for index in range(opening, len(tokens)):
        if tokens[index] == left:
            depth += 1
        elif tokens[index] == expected:
            depth -= 1
            if depth == 0:
                return index
    return None


def _tier_kind_variants(source: str) -> list[str]:
    tokens = _rust_tokens(source)
    try:
        declaration = next(i for i in range(len(tokens) - 2) if tokens[i:i + 3] == ["pub", "enum", "TierKind"])
        opening = tokens.index("{", declaration + 3)
    except ValueError:
        raise RoadmapVerificationError("TierKind enum disappeared or changed shape") from None
    closing = _matching_delimiter(tokens, opening, "{", "}")
    if closing is None:
        raise RoadmapVerificationError("TierKind enum has unbalanced delimiters")
    variants: list[str] = []
    depth = 0
    for token in tokens[opening + 1:closing]:
        if token in {"{", "(", "["}:
            depth += 1
        elif token in {"}", ")", "]"}:
            depth -= 1
        elif depth == 0 and (token[0].isupper() or token[0] == "_"):
            variants.append(token)
    if not variants:
        raise RoadmapVerificationError("TierKind enum has no variants")
    return variants


def _rate_ladder_length(source: str) -> int:
    tokens = _rust_tokens(source)
    try:
        name = next(
            i for i in range(len(tokens) - 3)
            if tokens[i:i + 4] == ["pub", "const", "TIER_RATE_LADDER", ":"]
        ) + 2
        equals = tokens.index("=", name)
    except ValueError:
        raise RoadmapVerificationError("canonical rate ladder declaration disappeared") from None
    type_open = next((i for i in range(name, equals) if tokens[i] == "["), None)
    if type_open is None:
        raise RoadmapVerificationError("canonical rate ladder declaration changed shape")
    type_close = _matching_delimiter(tokens, type_open)
    if type_close is None or ";" not in tokens[type_open:type_close]:
        raise RoadmapVerificationError("canonical rate ladder declaration changed shape")
    semicolon = tokens.index(";", type_open, type_close)
    raw_length = tokens[semicolon + 1]
    if not raw_length.isdigit() or tokens[type_close + 1:equals + 1] != ["="] or tokens[equals + 1] != "[":
        raise RoadmapVerificationError("canonical rate ladder declaration changed shape")
    return int(raw_length)


class RoadmapVerificationError(ValueError):
    """The roadmap contract cannot be established from committed inputs."""


def _read(repo_root: Path, relative: Path) -> str:
    path = repo_root / relative
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise RoadmapVerificationError(f"required input is unreadable: {relative}") from exc


def _git(repo_root: Path, *args: str) -> str:
    try:
        return subprocess.check_output(
            ("git", *args), cwd=repo_root, text=True, stderr=subprocess.PIPE
        ).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        raise RoadmapVerificationError(f"git input unavailable: {' '.join(args)}") from exc


def _git_blob(repo_root: Path, relative: str, revision: str = "HEAD") -> str:
    try:
        return subprocess.check_output(
            ("git", "show", f"{revision}:{relative}"),
            cwd=repo_root,
            text=True,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        raise RoadmapVerificationError(f"committed OKF input unavailable: {relative}") from exc


def _canonical_metrics(roadmap: str) -> dict[str, int | str]:
    match = CANONICAL_MARKERS["ROADMAP.md"].search(roadmap)
    if not match:
        raise RoadmapVerificationError("ROADMAP.md has no B061 canonical metrics block")
    values: dict[str, int | str] = {}
    for item in CANONICAL_MARKERS["metric"].finditer(match.group("body")):
        key = item.group("key")
        if key in values:
            raise RoadmapVerificationError(f"duplicate B061 canonical metric: {key}")
        raw_value = item.group("value")
        values[key] = raw_value if raw_value == "unavailable" else int(raw_value)
    if set(values) != CANONICAL_KEYS:
        missing = sorted(CANONICAL_KEYS - set(values))
        extra = sorted(set(values) - CANONICAL_KEYS)
        raise RoadmapVerificationError(
            f"B061 canonical metric set drifted (missing={missing}, extra={extra})"
        )
    return values


def _derived_okf(repo_root: Path) -> tuple[int, int, int]:
    head = _git(repo_root, "rev-parse", "HEAD")
    return _derived_okf_cached(str(repo_root), head)


@lru_cache(maxsize=4)
def _derived_okf_cached(repo_root_string: str, head: str) -> tuple[int, int, int]:
    repo_root = Path(repo_root_string)
    files = [
        path
        for path in _git(repo_root, "ls-tree", "-r", "--name-only", head, "--", "docs/knowledge/").splitlines()
        if path.endswith(".md") and not re.search(r"/(index|log)\.md$", path)
    ]
    if not files:
        raise RoadmapVerificationError("OKF corpus enumeration is empty")
    reachable = set(_git(repo_root, "rev-list", head).splitlines())
    checkpoints: list[str] = []
    for relative in files:
        blob = _git_blob(repo_root, relative, head)
        # A deferred concept is intentionally outside the claimed population:
        # it carries provenance for future authoring but is not an active
        # checkpoint claim and must not manufacture an orphan in this census.
        if DEFERRED_RE.search(blob):
            continue
        match = CHECKPOINT_RE.search(blob)
        if match:
            checkpoints.append(match.group(1))
    if not checkpoints:
        raise RoadmapVerificationError("OKF corpus has no checkpoint claims")
    orphaned = 0
    for checkpoint in checkpoints:
        resolved = _git(repo_root, "rev-parse", f"{checkpoint}^{{commit}}")
        if resolved not in reachable:
            orphaned += 1
    return len(files), len(checkpoints), orphaned


def _derived_tiers(repo_root: Path) -> tuple[int, int]:
    tier_source = _read(repo_root, Path("crates/corelink-tier-selection/src/tier.rs"))
    variants = _tier_kind_variants(tier_source)
    rate_source = _read(repo_root, Path("crates/corelink-ratelimit/src/tier.rs"))
    return len(variants), _rate_ladder_length(rate_source)


def _actions_metrics(
    repo_root: Path,
    metrics_path: Path,
    actions_export_path: Path | None = None,
) -> dict[str, int | str]:
    """Derive lane counts from the committed inventory, never from constants."""
    sidecar_path = repo_root / metrics_path
    try:
        sidecar = json.loads(sidecar_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise RoadmapVerificationError("B061 metrics sidecar is invalid or unreadable") from exc
    if not isinstance(sidecar, dict) or sidecar.get("version") != 1:
        raise RoadmapVerificationError("B061 metrics sidecar has an unsupported version")
    if set(sidecar) != {"version", "canonical_metrics", "actions_evidence"}:
        raise RoadmapVerificationError("B061 metrics sidecar has ambiguous fields")
    evidence = sidecar.get("actions_evidence")
    if not isinstance(evidence, dict):
        raise RoadmapVerificationError("B061 Actions evidence metadata is missing")
    required = {"as_of", "status", "export", "sha256", "source_query"}
    if set(evidence) != required or not isinstance(evidence["as_of"], str):
        raise RoadmapVerificationError("B061 Actions evidence metadata is incomplete")
    if evidence["status"] not in {"available", "unavailable"}:
        raise RoadmapVerificationError("B061 Actions evidence status is invalid")
    if evidence["export"] != ACTIONS_EXPORT.as_posix():
        raise RoadmapVerificationError("B061 Actions evidence export path drifted")
    export = actions_export_path or Path(evidence["export"])
    if not isinstance(export, Path) or (
        actions_export_path is None and (export.is_absolute() or ".." in export.parts)
    ):
        raise RoadmapVerificationError("B061 Actions evidence export path is unsafe")
    export_path = export if export.is_absolute() else repo_root / export
    try:
        raw_export = export_path.read_bytes()
    except OSError as exc:
        raise RoadmapVerificationError("B061 Actions evidence export is missing") from exc
    actual_digest = hashlib.sha256(raw_export).hexdigest()
    if actual_digest != evidence["sha256"]:
        raise RoadmapVerificationError("B061 Actions evidence export digest mismatches sidecar")
    try:
        export_data = json.loads(raw_export.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise RoadmapVerificationError("B061 Actions evidence export is invalid JSON") from exc
    if not isinstance(export_data, dict) or set(export_data) != {"version", "status", "as_of", "source", "lanes"}:
        raise RoadmapVerificationError("B061 Actions evidence export has ambiguous fields")
    if export_data["version"] != 1 or export_data["status"] != evidence["status"] or export_data["as_of"] != evidence["as_of"]:
        raise RoadmapVerificationError("B061 Actions evidence metadata disagrees with export")
    source = export_data["source"]
    if not isinstance(source, dict) or source.get("query") != evidence["source_query"]:
        raise RoadmapVerificationError("B061 Actions source query disagrees with export")
    lanes = export_data["lanes"]
    if not isinstance(lanes, list) or not lanes:
        raise RoadmapVerificationError("B061 Actions lane inventory is empty")
    seen: set[str] = set()
    for lane in lanes:
        if not isinstance(lane, dict) or set(lane) != {"workflow", "result"}:
            raise RoadmapVerificationError("B061 Actions lane record is malformed")
        workflow = lane["workflow"]
        if not isinstance(workflow, str) or workflow in seen or not workflow.startswith(".github/workflows/"):
            raise RoadmapVerificationError("B061 Actions lane identity is invalid or duplicated")
        if lane["result"] not in {"ran_no_success", "never_ran"}:
            raise RoadmapVerificationError("B061 Actions lane result is invalid")
        if not (repo_root / workflow).is_file():
            raise RoadmapVerificationError(f"B061 Actions workflow is missing: {workflow}")
        seen.add(workflow)
    ran = sum(lane["result"] == "ran_no_success" for lane in lanes)
    never = sum(lane["result"] == "never_ran" for lane in lanes)
    if evidence["status"] == "available":
        raise RoadmapVerificationError(
            "B061 Actions export claims available but has no per-lane run counts"
        )
    return {
        "actions_zero_success_lanes": len(lanes),
        "actions_ran_never_success_lanes": ran,
        "actions_never_run_lanes": never,
        "actions_runs_without_success": "unavailable",
    }


def _validate_projection(
    repo_root: Path,
    metrics: dict[str, int | str],
    *,
    roadmap_path: Path = ROADMAP,
    metrics_path: Path = METRICS,
) -> None:
    roadmap = _read(repo_root, roadmap_path)
    canonical = _canonical_metrics(roadmap)
    if canonical != metrics:
        mismatches = {
            key: (canonical[key], metrics[key])
            for key in sorted(CANONICAL_KEYS)
            if canonical[key] != metrics[key]
        }
        raise RoadmapVerificationError(f"ROADMAP canonical metrics drifted: {mismatches}")
    sidecar = _read(repo_root, metrics_path)
    try:
        data = json.loads(sidecar)
    except json.JSONDecodeError as exc:
        raise RoadmapVerificationError("B061 metrics sidecar is invalid JSON") from exc
    if set(data) != {"version", "canonical_metrics", "actions_evidence"} or data.get("version") != 1:
        raise RoadmapVerificationError("B061 metrics sidecar has ambiguous fields")
    if data.get("canonical_metrics") != metrics:
        raise RoadmapVerificationError("B061 metrics sidecar disagrees with derived metrics")


def _validate_projections(
    repo_root: Path,
    *,
    backlog_path: Path = BACKLOG,
    changelog_path: Path = CHANGELOG,
    workflow_path: Path = WORKFLOW,
) -> None:
    backlog = _read(repo_root, backlog_path)
    if not re.search(
        r"(?ms)^id: B-061\s+.*?^verify:\s*\|\s*\n\s+python3 scripts/verify_b061_remediation_roadmap\.py\s*$",
        backlog,
    ):
        raise RoadmapVerificationError("BACKLOG B-061 does not invoke its canonical verifier")
    if "B-061" not in _read(repo_root, changelog_path):
        raise RoadmapVerificationError("B061 changelog projection is missing its id")
    workflow = _read(repo_root, workflow_path)
    required_paths = {
        str(BACKLOG),
        str(ROADMAP),
        str(METRICS),
        str(ACTIONS_EXPORT),
        str(CHANGELOG),
        str(SCRIPT),
        str(TEST),
        str(OKF_VALIDATOR),
        str(OKF_MANIFEST),
        str(OKF_INDEX),
        str(OKF_INDEX_SCRIPT),
        str(OKF_RENDER_SCRIPT),
        str(OKF_RENDERED_SITE),
        "docs/knowledge/**",
        "crates/corelink-tier-selection/src/tier.rs",
        "crates/corelink-ratelimit/src/tier.rs",
    }
    required_paths.update(str(path) for path in OKF_SPLIT_MODULES)
    try:
        import yaml  # type: ignore
    except ImportError as exc:
        raise RoadmapVerificationError("PyYAML is required to parse workflow filters") from exc
    try:
        parsed = yaml.safe_load(workflow)
    except yaml.YAMLError as exc:
        raise RoadmapVerificationError("backlog workflow is invalid YAML") from exc
    trigger = parsed.get("on") if isinstance(parsed, dict) else None
    if trigger is None and isinstance(parsed, dict):
        trigger = parsed.get(True)  # PyYAML 1.1 parses the YAML key `on` as bool.
    if not isinstance(trigger, dict):
        raise RoadmapVerificationError("backlog workflow has no structured on/push trigger")
    for event in ("pull_request", "push"):
        event_data = trigger.get(event)
        paths = event_data.get("paths") if isinstance(event_data, dict) else None
        if not isinstance(paths, list) or any(not isinstance(path, str) for path in paths):
            raise RoadmapVerificationError(f"backlog workflow has no structured {event}.paths filter")
        missing_paths = sorted(path for path in required_paths if paths.count(path) != 1)
        if missing_paths:
            raise RoadmapVerificationError(f"backlog workflow paths omit or duplicate B061 inputs in {event}: {missing_paths}")
    if "python3 scripts/verify_b061_remediation_roadmap.py" not in workflow:
        raise RoadmapVerificationError("backlog workflow does not execute B061 verifier")
    okf_workflow = _read(repo_root, OKF_WORKFLOW)
    try:
        okf_parsed = yaml.safe_load(okf_workflow)
    except yaml.YAMLError as exc:
        raise RoadmapVerificationError("OKF workflow is invalid YAML") from exc
    okf_trigger = okf_parsed.get("on") if isinstance(okf_parsed, dict) else None
    if okf_trigger is None and isinstance(okf_parsed, dict):
        okf_trigger = okf_parsed.get(True)
    if not isinstance(okf_trigger, dict):
        raise RoadmapVerificationError("OKF workflow has no structured on/push trigger")
    for event in ("pull_request", "push"):
        event_data = okf_trigger.get(event)
        paths = event_data.get("paths") if isinstance(event_data, dict) else None
        if not isinstance(paths, list) or any(not isinstance(path, str) for path in paths):
            raise RoadmapVerificationError(f"OKF workflow has no structured {event}.paths filter")
        missing_paths = sorted(path for path in (str(item) for item in OKF_SPLIT_MODULES) if paths.count(path) != 1)
        if missing_paths:
            raise RoadmapVerificationError(f"OKF workflow split-module paths omit or duplicate in {event}: {missing_paths}")


def _validate_okf_surfaces(
    repo_root: Path,
    *,
    manifest_path: Path = OKF_MANIFEST,
    index_path: Path = OKF_INDEX,
    site_path: Path = OKF_RENDERED_SITE,
) -> None:
    """Fail closed when the split validator and its generated OKF surfaces drift."""
    validator_path = repo_root / OKF_VALIDATOR
    runtime_path = repo_root / OKF_RUNTIME
    try:
        validator_text = validator_path.read_text(encoding="utf-8")
        ast.parse(validator_text, filename=str(validator_path))
        split_sources = {
            relative: (repo_root / relative).read_text(encoding="utf-8")
            for relative in OKF_SPLIT_MODULES
        }
        split_trees = {
            relative: ast.parse(source, filename=str(repo_root / relative))
            for relative, source in split_sources.items()
        }
    except (OSError, UnicodeDecodeError, SyntaxError) as exc:
        raise RoadmapVerificationError("OKF split validator module is missing, unreadable, or invalid Python") from exc
    runtime_text = split_sources[OKF_RUNTIME]
    runtime_tree = split_trees[OKF_RUNTIME]
    def imported_modules(tree: ast.AST) -> set[str]:
        modules: set[str] = set()
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                modules.update(alias.name for alias in node.names)
            elif isinstance(node, ast.ImportFrom) and node.module:
                modules.add(node.module)
        return modules

    wrapper_imports = imported_modules(ast.parse(validator_text, filename=str(validator_path)))
    required_wrapper_imports = {
        "validate_okf_runtime",
        "validate_okf_core1",
        "validate_okf_core2",
        "validate_okf_checks",
    }
    imported = wrapper_imports | imported_modules(runtime_tree)
    if not required_wrapper_imports <= imported or "okf_anchor_reverify" not in imported:
        raise RoadmapVerificationError("OKF split validator does not import every required module")
    run_checks = next(
        (node for node in ast.walk(runtime_tree) if isinstance(node, ast.FunctionDef) and node.name == "run_checks"),
        None,
    )
    if run_checks is None or "validate_okf_runtime" not in validator_text:
        raise RoadmapVerificationError("OKF wrapper/runtime split lost executable run_checks")
    calls = {node.func.attr for node in ast.walk(run_checks) if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)}
    literal_checks = {
        node.args[0].value for node in ast.walk(run_checks)
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
        and node.func.attr == "add" and node.args and isinstance(node.args[0], ast.Constant)
        and isinstance(node.args[0].value, str)
    }
    if "is_ancestor" not in calls or not {"C4", "C5"} <= literal_checks:
        raise RoadmapVerificationError("OKF split validator lost fail-closed checkpoint/content handling")

    try:
        import yaml  # type: ignore
    except ImportError as exc:
        raise RoadmapVerificationError("PyYAML is required to validate the OKF manifest") from exc
    manifest_file = manifest_path if manifest_path.is_absolute() else repo_root / manifest_path
    try:
        manifest = yaml.safe_load(manifest_file.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, yaml.YAMLError) as exc:
        raise RoadmapVerificationError("OKF manifest is invalid or unreadable") from exc
    if not isinstance(manifest, dict) or not isinstance(manifest.get("candidates"), list):
        raise RoadmapVerificationError("OKF manifest has no closed candidates population")
    candidates: set[str] = set()
    active: set[str] = set()
    for candidate in manifest["candidates"]:
        if not isinstance(candidate, dict) or not isinstance(candidate.get("id"), str):
            raise RoadmapVerificationError("OKF manifest contains a malformed candidate")
        cid = candidate["id"].strip().lstrip("/")
        if not cid or not MANIFEST_ID_RE.fullmatch(cid) or cid in candidates:
            raise RoadmapVerificationError(f"OKF manifest candidate population is not unique: {cid!r}")
        candidates.add(cid)
        if candidate.get("status") == "active":
            active.add(cid)

    scripts_dir = repo_root / "scripts"
    if str(scripts_dir) not in sys.path:
        sys.path.insert(0, str(scripts_dir))
    import okf_index  # type: ignore
    import okf_render  # type: ignore
    index_concepts = okf_index.find_concepts()
    index_ids = [cid for _, cid, _ in index_concepts]
    if len(index_ids) != len(set(index_ids)) or active != set(index_ids):
        raise RoadmapVerificationError("OKF manifest active population is not the exact generated index population")
    index_file = index_path if index_path.is_absolute() else repo_root / index_path
    try:
        index_text = index_file.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise RoadmapVerificationError("OKF generated index is missing or unreadable") from exc
    if index_text != okf_index.render(index_concepts):
        raise RoadmapVerificationError("OKF index is stale relative to the closed concept population")

    render_concepts = okf_render.find_concepts()
    render_ids = [concept["id"] for concept in render_concepts]
    if len(render_ids) != len(index_ids) or set(render_ids) != set(index_ids):
        raise RoadmapVerificationError("OKF renderer and index disagree on the closed concept population")
    site_file = site_path if site_path.is_absolute() else repo_root / site_path
    try:
        site_text = site_file.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise RoadmapVerificationError("OKF rendered site is missing or unreadable") from exc
    if site_text != okf_render.render_html(render_concepts):
        raise RoadmapVerificationError("OKF rendered site is stale relative to the closed concept population")
    payload_match = re.search(r'<script type="application/json" id="okf-data">(.*?)</script>', site_text, re.DOTALL)
    if not payload_match:
        raise RoadmapVerificationError("OKF rendered site has no machine-readable concept payload")
    try:
        payload = json.loads(payload_match.group(1))
    except json.JSONDecodeError as exc:
        raise RoadmapVerificationError("OKF rendered site concept payload is invalid JSON") from exc
    payload_concepts = payload.get("concepts") if isinstance(payload, dict) else None
    if not isinstance(payload_concepts, list) or {c.get("id") for c in payload_concepts if isinstance(c, dict)} != set(index_ids):
        raise RoadmapVerificationError("OKF rendered payload does not exactly enumerate the index concepts")


def verify(
    repo_root: Path = REPO_ROOT,
    *,
    roadmap_path: Path = ROADMAP,
    metrics_path: Path = METRICS,
    backlog_path: Path = BACKLOG,
    changelog_path: Path = CHANGELOG,
    workflow_path: Path = WORKFLOW,
    actions_export_path: Path | None = None,
) -> dict[str, int | str]:
    if _git(repo_root, "rev-parse", "--is-shallow-repository") != "false":
        raise RoadmapVerificationError("shallow clone: ancestry cannot be verified")
    documents, checkpoints, orphaned = _derived_okf(repo_root)
    tier_variants, rate_ladder = _derived_tiers(repo_root)
    actions = _actions_metrics(repo_root, metrics_path, actions_export_path)
    metrics = {
        "okf_documents": documents,
        "okf_checkpoint_claims": checkpoints,
        "okf_orphan_anchors": orphaned,
        "tier_kind_variants": tier_variants,
        "rate_ladder_variants": rate_ladder,
        **actions,
    }
    _validate_projection(repo_root, metrics, roadmap_path=roadmap_path, metrics_path=metrics_path)
    _validate_projections(
        repo_root,
        backlog_path=backlog_path,
        changelog_path=changelog_path,
        workflow_path=workflow_path,
    )
    _validate_okf_surfaces(repo_root)
    return {**metrics, "status": "b061_remediation_roadmap_verified"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    args = parser.parse_args()
    try:
        report = verify(args.repo_root.resolve())
    except RoadmapVerificationError as exc:
        print(f"B061-ROADMAP: RED: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(report, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
