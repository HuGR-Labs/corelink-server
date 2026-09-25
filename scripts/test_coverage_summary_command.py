"""Regression coverage for the cargo-llvm-cov summary command syntax."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[1]


class CoverageSummaryCommandTests(unittest.TestCase):
    def test_script_generates_reports_with_supported_report_arguments(self):
        with tempfile.TemporaryDirectory(prefix="coverage-summary-test-") as tmp:
            root = Path(tmp)
            bin_dir = root / "bin"
            bin_dir.mkdir()
            log_path = root / "commands.jsonl"
            fake_cov = bin_dir / "cargo-llvm-cov"
            fake_cov.write_text("#!/bin/sh\nexit 0\n")
            fake_cov.chmod(0o755)
            fake_cargo = bin_dir / "cargo"
            fake_cargo.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, pathlib, sys\n"
                "args = sys.argv[1:]\n"
                "with open(os.environ['COV_COMMAND_LOG'], 'a') as f:\n"
                "    f.write(json.dumps(args) + '\\n')\n"
                "if args[:2] == ['llvm-cov', 'clean']:\n"
                "    raise SystemExit(0)\n"
                "if args[:2] == ['llvm-cov', 'report']:\n"
                "    print('crate-a  80.0%')\n"
                "    raise SystemExit(0)\n"
                "if '--html' in args:\n"
                "    out = pathlib.Path(args[args.index('--output-dir') + 1])\n"
                "    out.mkdir(parents=True, exist_ok=True)\n"
                "    (out / 'index.html').write_text('<html>report</html>')\n"
                "    raise SystemExit(0)\n"
                "raise SystemExit('unexpected cargo invocation: ' + repr(args))\n"
            )
            fake_cargo.chmod(0o755)
            env = os.environ.copy()
            env.update(
                PATH=f"{bin_dir}{os.pathsep}{env['PATH']}",
                COV_OUT=str(root / "coverage"),
                COV_COMMAND_LOG=str(log_path),
            )

            subprocess.run(
                ["bash", str(REPO / "scripts/coverage.sh")],
                cwd=REPO,
                env=env,
                check=True,
                capture_output=True,
                text=True,
            )

            calls = [json.loads(line) for line in log_path.read_text().splitlines()]
            self.assertIn(["llvm-cov", "report", "--summary-only"], calls)
            self.assertFalse(
                any(call[:2] == ["llvm-cov", "report"] and "--workspace" in call for call in calls)
            )
            self.assertTrue((root / "coverage" / "SUMMARY.txt").is_file())
            self.assertIn("crate-a", (root / "coverage" / "SUMMARY.txt").read_text())
            self.assertTrue((root / "coverage" / "html" / "index.html").is_file())


if __name__ == "__main__":
    unittest.main()
