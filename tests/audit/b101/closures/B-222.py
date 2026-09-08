#!/usr/bin/env python3
from pathlib import Path
import runpy

ROOT = Path(__file__).resolve().parents[4]
runpy.run_path(str(ROOT / "scripts/verify_b215_b223_contracts.py"))["verify"](ROOT, "B-222")
