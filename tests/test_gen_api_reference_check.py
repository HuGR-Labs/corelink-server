from __future__ import annotations

import contextlib
import importlib.util
import io
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/gen-api-reference.py"
sys.path.insert(0, str(SCRIPT.parent))
spec = importlib.util.spec_from_file_location("gen_api_reference", SCRIPT)
assert spec and spec.loader
generator = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = generator
spec.loader.exec_module(generator)


class GeneratorCheckTests(unittest.TestCase):
    def test_check_is_fail_closed_for_changed_stale_and_missing_output(self) -> None:
        # Keep this test isolated from the checked-in translated trees.  The
        # generator's localized parity check is exercised by its own gate;
        # this test focuses on the output-check contract.
        original_localized_check = generator.validate_localized_api_indexes
        generator.validate_localized_api_indexes = lambda _expected: []
        try:
            with tempfile.TemporaryDirectory(dir=ROOT) as raw_dir:
                out_dir = Path(raw_dir) / "reference"

                self.assertEqual(self._run(out_dir)[0], 0)  # normal generation
                self.assertEqual(self._run(out_dir, "--check")[0], 0)

                changed = out_dir / "endpoints/post-v1-customer-keys.mdx"
                changed.write_text(
                    changed.read_text(encoding="utf-8") + "<!-- mutation -->\n",
                    encoding="utf-8",
                )
                missing = out_dir / "endpoints/post-v1-pats.mdx"
                missing.unlink()
                stale = out_dir / "endpoints/stale-generated-page.mdx"
                stale.write_text("stale\n", encoding="utf-8")

                code, output = self._run(out_dir, "--check")
                self.assertEqual(code, 1, output)
                self.assertIn("post-v1-customer-keys.mdx", output)
                self.assertIn("post-v1-pats.mdx", output)
                self.assertIn("stale-generated-page.mdx", output)

                self.assertEqual(self._run(out_dir)[0], 0)  # repair
                self.assertEqual(self._run(out_dir, "--check")[0], 0)
        finally:
            generator.validate_localized_api_indexes = original_localized_check

    @staticmethod
    def _run(out_dir: Path, *flags: str) -> tuple[int, str]:
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            code = generator.main(
                ["--spec", str(generator.DEFAULT_SPEC), "--out-dir", str(out_dir), *flags]
            )
        return code, output.getvalue()


if __name__ == "__main__":
    unittest.main()
