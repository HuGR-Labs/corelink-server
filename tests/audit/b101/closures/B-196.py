#!/usr/bin/env python3
"""B-196 closure sentinel; implementation gate lives in the group verifier."""
from pathlib import Path
import runpy

ROOT = Path(__file__).resolve().parents[4]
runpy.run_path(str(ROOT / "scripts/verify_b193_b214_closures.py"))["verify"](ROOT, "B-196")
