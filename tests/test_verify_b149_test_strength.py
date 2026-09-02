from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b149_test_strength.py"
APPROVED = "627ec21afefb3873e11010800db2ba216b86b87a"
spec = importlib.util.spec_from_file_location("b149_verifier", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)


class B149VerifierTests(unittest.TestCase):
    def approved_root(self) -> tuple[tempfile.TemporaryDirectory[str], Path]:
        temp = tempfile.TemporaryDirectory()
        root = Path(temp.name)
        for relative in verifier.CHECKPOINTS:
            content = subprocess.check_output(
                ["git", "show", f"{APPROVED}:{relative}"], cwd=ROOT
            )
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
        return temp, root

    def mutate(self, relative: str, suffix: bytes = b"\n// review mutation\n") -> list[str]:
        temp, root = self.approved_root()
        with temp:
            target = root / relative
            target.write_bytes(target.read_bytes() + suffix)
            return verifier.assess(root)

    def rewrite(self, relative: str, old: bytes, new: bytes) -> list[str]:
        temp, root = self.approved_root()
        with temp:
            target = root / relative
            content = target.read_bytes()
            self.assertIn(old, content)
            target.write_bytes(content.replace(old, new, 1))
            return verifier.assess(root)

    def test_hygiene_baseline_is_open_with_the_four_named_gaps(self) -> None:
        self.assertEqual(list(verifier.GAPS), verifier.assess(ROOT))

    def test_exact_approved_candidate_is_done_and_cli_polarity_reverses(self) -> None:
        temp, root = self.approved_root()
        with temp:
            self.assertEqual(verifier.assess(root), [])
            done = subprocess.run(
                [sys.executable, str(SCRIPT), "--root", str(root), "--expect", "done"],
                text=True, capture_output=True, check=False,
            )
            opened = subprocess.run(
                [sys.executable, str(SCRIPT), "--root", str(root), "--expect", "open"],
                text=True, capture_output=True, check=False,
            )
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertEqual(opened.returncode, 1)

    def test_a_byte_mutation_in_each_protected_file_reopens_its_gap(self) -> None:
        expected = {
            verifier.AUDIT: verifier.GAPS[0],
            verifier.AUTH: verifier.GAPS[1],
            verifier.VALIDATE: verifier.GAPS[2],
            verifier.INGEST: verifier.GAPS[2],
            verifier.SKIP: verifier.GAPS[3],
        }
        for relative, gap in expected.items():
            with self.subTest(relative=relative):
                self.assertEqual(self.mutate(relative), [gap])

    def test_known_syntax_bypasses_all_reopen_their_protected_checkpoint(self) -> None:
        loop = b"for (wire, expected, expected_code) in cases {"
        audit_assert = b'''    assert!(
        statements.is_empty(),
        "an empty batch emits zero statements"
    );'''
        audit_dead = b'''    if false {
        assert!(
        statements.is_empty(),
        "an empty batch emits zero statements"
    ); }'''
        auth_assert = b"    assert!(configured_ingest_auth_key(None).is_none());"
        skip_assert = b'    assert_eq!(RecordError::BadTenantId.code(), "bad_tenant_id");'
        cases = (
            ("audit tautology", verifier.AUDIT, b"statements.is_empty(),", b"statements.is_empty() || true,", verifier.GAPS[0]),
            ("audit dead branch", verifier.AUDIT, audit_assert, audit_dead, verifier.GAPS[0]),
            ("auth tautology", verifier.AUTH, b".is_none());", b".is_none() || true);", verifier.GAPS[1]),
            ("auth uncalled closure", verifier.AUTH, auth_assert, b"    let uncalled = || { assert!(configured_ingest_auth_key(None).is_none()); };", verifier.GAPS[1]),
            ("fixture take zero", verifier.VALIDATE, loop, b"for (wire, expected, expected_code) in cases.into_iter().take(0) {", verifier.GAPS[2]),
            ("fixture first row", verifier.VALIDATE, loop, b"for (wire, expected, expected_code) in [cases.into_iter().next().unwrap()] {", verifier.GAPS[2]),
            ("fixture early return", verifier.VALIDATE, b"    " + loop, b"    return;\n    " + loop, verifier.GAPS[2]),
            ("wrong live mapping", verifier.INGEST, b'Self::BadTenantId => "bad_tenant_id"', b'Self::BadTenantId => "bad_region"', verifier.GAPS[2]),
            ("skip dead branch", verifier.SKIP, skip_assert, b'    if false { assert_eq!(RecordError::BadTenantId.code(), "bad_tenant_id"); }', verifier.GAPS[3]),
            ("skip disconnected tokens", verifier.SKIP, b"    assert_eq!(RecordError::BadTenantId.code(),", b"    let disconnected = RecordError::BadTenantId.code(); //", verifier.GAPS[3]),
        )
        for label, relative, old, new, gap in cases:
            with self.subTest(label=label):
                self.assertEqual(self.rewrite(relative, old, new), [gap])

    def test_missing_checkpoint_is_an_instrument_error(self) -> None:
        temp, root = self.approved_root()
        with temp:
            (root / verifier.AUTH).unlink()
            with self.assertRaises(verifier.InstrumentError):
                verifier.assess(root)

    def test_symlink_to_approved_tree_and_path_swap_fail_closed(self) -> None:
        approved_temp, approved = self.approved_root()
        target_temp, root = self.approved_root()
        with approved_temp, target_temp:
            target = root / verifier.AUDIT
            target.unlink()
            target.symlink_to(approved / verifier.AUDIT)
            with self.assertRaises(verifier.InstrumentError):
                verifier.assess(root)
        temp, root = self.approved_root()
        with temp:
            target = root / verifier.AUDIT
            alias = target.with_name("approved-copy.rs")
            alias.write_bytes(target.read_bytes())
            target.unlink()
            target.symlink_to(alias.name)
            with self.assertRaises(verifier.InstrumentError):
                verifier.assess(root)

    def test_escaping_checkpoint_and_non_regular_swap_are_instrument_errors(self) -> None:
        temp, root = self.approved_root()
        checkpoints = verifier.CHECKPOINTS
        try:
            verifier.CHECKPOINTS = {"../outside.rs": "0" * 64}
            with temp, self.assertRaises(verifier.InstrumentError):
                verifier.assess(root)
        finally:
            verifier.CHECKPOINTS = checkpoints
        temp, root = self.approved_root()
        with temp:
            target = root / verifier.AUTH
            target.unlink()
            target.mkdir()
            with self.assertRaises(verifier.InstrumentError):
                verifier.assess(root)


if __name__ == "__main__":
    unittest.main()
