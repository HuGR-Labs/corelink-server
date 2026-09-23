#!/usr/bin/env python3
"""B-210 closure sentinel; retirement gate owns this superseded finding."""
from pathlib import Path
import runpy

ROOT = Path(__file__).resolve().parents[4]
runpy.run_path(str(ROOT / "scripts/verify_b210_retirement.py"))["verify"](ROOT)
