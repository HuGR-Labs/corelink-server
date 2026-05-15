#!/usr/bin/env python3
"""
gen-grafana-provision.py — Generate Grafana JSON dashboards from CoreLink DASH-*.md specs.

Reads:
  specs/_dashboards/DASH-*.md            — one spec per dashboard (Markdown + YAML front matter)
  specs/_dashboards/templates/
    grafana-dashboard.json.template     — Grafana 11 JSON skeleton with {{PLACEHOLDERS}}
  specs/03_architecture/observability_model.md  — canonical metric-name registry (validation)

Writes:
  infra/grafana/dashboards/<dash-id>.json     — one JSON per spec, ready for Grafana provisioning

Modes:
  python3 scripts/gen-grafana-provision.py             # write JSON
  python3 scripts/gen-grafana-provision.py --validate  # dry-run: parse + cross-check metrics; exit 1 on issues
  python3 scripts/gen-grafana-provision.py --check     # exit 1 if generated JSON differs from on-disk (CI gate)

Spec format expected per DASH-*.md (see specs/_dashboards/INDEX.md §2):
  - YAML front matter with `id` matching DASH-*
  - `## Metadata` section with Purpose / Audience / Refresh / Variables / Time range
  - `## Panels` table — columns: # | Title | Type | Query | Threshold

Conventions:
  - Idempotent: re-running produces byte-identical output.
  - Stable panel IDs (1..N from the table).
  - Default grid: 2 columns × N rows, panel height 8.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SPECS_DIR = REPO_ROOT / "specs"
DASH_DIR = SPECS_DIR / "_dashboards"
TEMPLATE_PATH = DASH_DIR / "templates" / "grafana-dashboard.json.template"
OBSERVABILITY_MD = SPECS_DIR / "03_architecture" / "observability_model.md"
OUT_DIR = REPO_ROOT / "infra" / "grafana" / "dashboards"

FRONT_MATTER_RE = re.compile(r"^---\n(.*?)\n---\n", re.DOTALL)
SECTION_RE = re.compile(r"^##\s+(.+?)\s*$", re.MULTILINE)
METRIC_RE = re.compile(r"\bcorelink_[a-z0-9_]+\b")

# Panel-type mapping: spec "Type" column → Grafana panel `type`
PANEL_TYPE_MAP = {
    "timeseries": "timeseries",
    "stat": "stat",
    "stat-grid": "stat",
    "bar": "barchart",
    "bar-gauge": "bargauge",
    "gauge": "gauge",
    "table": "table",
    "heatmap": "heatmap",
    "histogram": "histogram",
    "geomap": "geomap",
    "annotations": "timeseries",
    "exemplar": "timeseries",
    "text": "text",
    "sankey": "table",
}


def load_metric_registry() -> set[str]:
    """Extract all corelink_* metric names referenced in observability_model.md."""
    if not OBSERVABILITY_MD.exists():
        return set()
    text = OBSERVABILITY_MD.read_text(encoding="utf-8")
    return set(METRIC_RE.findall(text))


def parse_front_matter(text: str) -> dict:
    m = FRONT_MATTER_RE.match(text)
    if not m:
        raise ValueError("no YAML front matter")
    # naive key:value parse (avoids pyyaml dep for CI portability)
    fm = {}
    for line in m.group(1).splitlines():
        if ":" not in line or line.lstrip().startswith("#"):
            continue
        k, _, v = line.partition(":")
        fm[k.strip()] = v.strip().strip('"')
    return fm


def parse_metadata(text: str) -> dict:
    """Pull Refresh + Time range from the ## Metadata section."""
    out = {"refresh": "1m", "time_from": "now-24h"}
    md_section = re.search(r"^## Metadata\s*\n(.*?)(?=\n##\s|\Z)", text, re.MULTILINE | re.DOTALL)
    if not md_section:
        return out
    body = md_section.group(1)
    refresh_m = re.search(r"\*\*Refresh:\*\*\s*([^\s.(]+)", body)
    if refresh_m:
        out["refresh"] = refresh_m.group(1).strip().rstrip(".")
    # Look for "default `now-XXX`" pattern in Time range line
    time_m = re.search(r"\*\*Time range:\*\*[^\n]*?`(now-[^`]+)`", body)
    if time_m:
        out["time_from"] = time_m.group(1)
    return out


def parse_panels(text: str) -> list[dict]:
    """Parse the ## Panels table into a list of {id, title, type, query, threshold} dicts."""
    panels_section = re.search(r"^## Panels\s*\n(.*?)(?=\n##\s|\Z)", text, re.MULTILINE | re.DOTALL)
    if not panels_section:
        return []
    body = panels_section.group(1)
    panels = []
    for line in body.splitlines():
        line = line.strip()
        if not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip("|").split("|")]
        if len(cells) < 5:
            continue
        first = cells[0]
        # skip header + separator rows
        if not first or first in {"#"} or set(first) <= {"-", ":", " "}:
            continue
        try:
            panel_id = int(first)
        except ValueError:
            continue
        title, ptype, query, threshold = cells[1], cells[2], cells[3], cells[4]
        # strip backtick fences from query
        query = query.strip().strip("`")
        # normalise panel type (drop trailing " (N)" annotations like "timeseries (4)")
        ptype_norm = re.split(r"\s*\(", ptype)[0].strip()
        panels.append({
            "id": panel_id,
            "title": title,
            "type_spec": ptype_norm,
            "type_grafana": PANEL_TYPE_MAP.get(ptype_norm, "timeseries"),
            "query": query,
            "threshold": threshold,
        })
    return panels


def grid_pos(idx: int) -> dict:
    """2-column grid, height 8, width 12."""
    row = idx // 2
    col = idx % 2
    return {"h": 8, "w": 12, "x": col * 12, "y": row * 8}


def panel_to_grafana(panel: dict, idx: int) -> dict:
    """Map our spec panel to a Grafana JSON panel."""
    return {
        "id": panel["id"],
        "type": panel["type_grafana"],
        "title": panel["title"],
        "description": f"Threshold/alert: {panel['threshold']}" if panel["threshold"] else "",
        "datasource": {"type": "prometheus", "uid": "${DS_PROMETHEUS}"},
        "gridPos": grid_pos(idx),
        "targets": [
            {
                "refId": "A",
                "datasource": {"type": "prometheus", "uid": "${DS_PROMETHEUS}"},
                "expr": panel["query"],
                "legendFormat": "__auto",
            }
        ],
        "fieldConfig": {
            "defaults": {
                "color": {"mode": "palette-classic"},
                "thresholds": {"mode": "absolute", "steps": [{"color": "green", "value": None}]},
            },
            "overrides": [],
        },
        "options": {},
    }


def extract_metrics(panels: list[dict]) -> set[str]:
    found = set()
    for p in panels:
        found.update(METRIC_RE.findall(p["query"]))
    return found


def render_dashboard(spec_path: Path, registry: set[str], strict: bool) -> tuple[str, list[str]]:
    text = spec_path.read_text(encoding="utf-8")
    fm = parse_front_matter(text)
    dash_id = fm["id"]
    if not dash_id.startswith("DASH-"):
        raise ValueError(f"{spec_path}: id must start with DASH-")
    meta = parse_metadata(text)
    panels = parse_panels(text)
    if not panels:
        raise ValueError(f"{spec_path}: no panels parsed")

    issues: list[str] = []
    metrics = extract_metrics(panels)
    # Whitelist patterns: ALERTS, predict_linear refs already covered.
    for m in sorted(metrics):
        if m not in registry:
            issues.append(f"{dash_id}: metric `{m}` not declared in observability_model.md")

    title_m = re.search(r"^#\s+DASH-[^\s]+\s+—\s+(.+?)\s*$", text, re.MULTILINE)
    title = f"CoreLink — {title_m.group(1)}" if title_m else f"CoreLink — {dash_id}"

    # Audience tag for Grafana tags
    audience_m = re.search(r"\*\*Audience:\*\*\s*([^\n.]+)", text)
    audience_tag = "sre"
    if audience_m:
        a = audience_m.group(1).lower()
        if "security" in a:
            audience_tag = "security"
        elif "privacy" in a or "dpo" in a:
            audience_tag = "privacy"
        elif "finance" in a:
            audience_tag = "finance"
        elif "exec" in a or "leadership" in a:
            audience_tag = "exec"
        elif "compliance" in a:
            audience_tag = "compliance"
        elif "product" in a:
            audience_tag = "product"

    grafana_panels = [panel_to_grafana(p, i) for i, p in enumerate(panels)]

    template = TEMPLATE_PATH.read_text(encoding="utf-8")
    spec_rel = spec_path.relative_to(REPO_ROOT).as_posix()
    spec_url = f"https://github.com/humangr-labs/corelink-server/blob/main/{spec_rel}"

    rendered = (
        template
        .replace("{{DASH_ID_LOWER}}", dash_id.lower())
        .replace("{{TITLE}}", title)
        .replace("{{DESCRIPTION}}", f"Auto-generated from {spec_rel}. DO NOT EDIT — modify the spec instead.")
        .replace("{{REFRESH}}", meta["refresh"])
        .replace("{{TIME_FROM}}", meta["time_from"])
        .replace("{{SPEC_URL}}", spec_url)
        .replace("{{AUDIENCE_TAG}}", audience_tag)
        .replace("{{EXTRA_VARIABLES_JSON}}", "")
        .replace("{{PANELS_JSON}}", json.dumps(grafana_panels, indent=2))
    )

    # Final JSON sanity-check
    try:
        parsed = json.loads(rendered)
    except json.JSONDecodeError as e:
        raise ValueError(f"{dash_id}: rendered template is not valid JSON: {e}")

    # Re-serialise canonically for stable diffs
    canonical = json.dumps(parsed, indent=2, sort_keys=False) + "\n"

    if strict and issues:
        for i in issues:
            print(f"  ⚠️  {i}", file=sys.stderr)
    return canonical, issues


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--validate", action="store_true", help="dry-run + metric cross-check; exit 1 on issues")
    ap.add_argument("--check", action="store_true", help="exit 1 if on-disk JSON differs from regenerated")
    args = ap.parse_args()

    if not TEMPLATE_PATH.exists():
        print(f"ERROR: template missing: {TEMPLATE_PATH}", file=sys.stderr)
        return 1

    registry = load_metric_registry()
    spec_files = sorted(DASH_DIR.glob("DASH-*.md"))
    spec_files = [p for p in spec_files if p.name != "INDEX.md"]
    if not spec_files:
        print(f"ERROR: no DASH-*.md specs found in {DASH_DIR}", file=sys.stderr)
        return 1

    OUT_DIR.mkdir(parents=True, exist_ok=True)

    all_issues: list[str] = []
    drift = False
    for spec in spec_files:
        try:
            rendered, issues = render_dashboard(spec, registry, strict=args.validate)
        except Exception as e:  # noqa: BLE001
            print(f"❌ {spec.name}: {e}", file=sys.stderr)
            return 1
        all_issues.extend(issues)

        out_path = OUT_DIR / f"{spec.stem.lower()}.json"
        if args.check:
            if not out_path.exists() or out_path.read_text(encoding="utf-8") != rendered:
                drift = True
                print(f"⚠️  DRIFT: {out_path}", file=sys.stderr)
        elif args.validate:
            print(f"✓ parsed {spec.name} ({rendered.count(chr(10))} lines)")
        else:
            out_path.write_text(rendered, encoding="utf-8")
            print(f"✓ wrote {out_path.relative_to(REPO_ROOT)}")

    if args.check and drift:
        print("CHECK FAILED: regenerate with `python3 scripts/gen-grafana-provision.py`", file=sys.stderr)
        return 1
    if args.validate:
        if all_issues:
            print(f"\n❌ {len(all_issues)} metric-registry issue(s)", file=sys.stderr)
            return 1
        print(f"\n✓ all {len(spec_files)} dashboards parse + metrics registered")
    return 0


if __name__ == "__main__":
    sys.exit(main())
