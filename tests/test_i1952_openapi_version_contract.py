#!/usr/bin/env python3
"""Keep the OpenAPI document version aligned with Cargo's workspace version."""

import json
import re
from pathlib import Path


ROOT = Path(__file__).parents[1]
cargo = (ROOT / "Cargo.toml").read_text()
workspace = re.search(r"(?ms)^\[workspace\.package\]\s+.*?^version\s*=\s*\"([^\"]+)\"", cargo)
if workspace is None:
    raise SystemExit("missing workspace package version")
expected = workspace.group(1)

yaml = (ROOT / "openapi/corelink-v1.yaml").read_text()
match = re.search(r"(?m)^  version:\s*([^\s#]+)\s*$", yaml)
if match is None:
    raise SystemExit("missing OpenAPI YAML info.version")
if match.group(1) != expected:
    raise SystemExit(f"OpenAPI YAML version {match.group(1)} != workspace version {expected}")

document = json.loads((ROOT / "openapi/corelink-v1.json").read_text())
if document.get("info", {}).get("version") != expected:
    raise SystemExit("OpenAPI JSON info.version is not the workspace version")
if not str(document.get("openapi", "")).startswith("3.1"):
    raise SystemExit("OpenAPI major contract changed")
