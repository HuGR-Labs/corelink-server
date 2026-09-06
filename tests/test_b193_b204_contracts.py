"""Behavioral and inverted-mutation tests for the B193..B204 verifier."""

from pathlib import Path
import shutil
import tempfile
import unittest

from scripts.verify_b193_b204_contracts import verify


class B193B204VerifierTests(unittest.TestCase):
    def copy_evidence(self, source_root: Path, root: Path) -> None:
        """Copy only the verifier's small evidence surface, not build trees."""
        files = (
            "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs",
            "crates/corelink-container/src/routes/oci/b126_m2_test_1_1.rs",
            "crates/corelink-adapter-host/src/oci/server/core.rs",
            "crates/corelink-container/src/routes/residency.rs",
            "crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs",
            "crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs",
            "crates/corelink-container/src/storage/region_map.rs",
            "apps/signup-worker/src/webhooks/dsr_verify_cron.ts",
            "apps/signup-worker/src/webhooks/clerk_identity.ts",
            "worker/src/region-map.ts",
            "migrations/d1/0044_drata_evidence_sent.sql",
            "migrations/d1/0044_stripe_webhook_events_processed.sql",
            "scripts/check_migration_prefixes.py",
            "docs/compliance/byok-fips-evidence.md",
            "legal/dpa-residency-amendment.md",
            "docs/design/2026-08-17-wp4-apac-physical-plan.md",
            "docs/knowledge/adr/adr-s14-001-multi-region-terraform-module.md",
            "docs/knowledge/adr/adr-s14-002-region-pinning-enforcement.md",
            "specs/03_architecture/adrs/ADR-S14-008-dpa-amendment-schrems-ii-tia-legal-externo.md",
            "legal/tia-template.md",
            "legal/dpa/SUB-PROCESSOR-COMMITMENTS.md",
            "docs/knowledge/adr/adr-s14-008-dpa-amendment-schrems-ii-tia-legal-externo.md",
            "docs/okf-wiki-site/index.html",
        )
        for rel in files:
            destination = root / rel
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source_root / rel, destination)

    def test_clean_checkout_passes_and_mutated_limit_fails_closed(self) -> None:
        source_root = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory(prefix="b193-b204-verify-") as tmp:
            root = Path(tmp)
            self.copy_evidence(source_root, root)
            self.assertEqual(verify(root), [])
            cron = root / "apps/signup-worker/src/webhooks/dsr_verify_cron.ts"
            text = cron.read_text(encoding="utf-8")
            cron.write_text(text.replace("LIMIT ?3", "LIMIT ?9"), encoding="utf-8")
            errors = verify(root)
            self.assertTrue(any("B-196" in error for error in errors), errors)

    def test_region_set_mutation_fails_closed(self) -> None:
        source_root = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory(prefix="b193-b204-region-verify-") as tmp:
            root = Path(tmp)
            self.copy_evidence(source_root, root)
            worker = root / "worker/src/region-map.ts"
            text = worker.read_text(encoding="utf-8")
            worker.write_text(text.replace('  "apac",\n]);', "]);", 1), encoding="utf-8")
            errors = verify(root)
            self.assertTrue(any("B-198" in error for error in errors), errors)

    def test_dpa_reactivating_byok_claim_fails_closed(self) -> None:
        source_root = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory(prefix="b193-b204-dpa-verify-") as tmp:
            root = Path(tmp)
            self.copy_evidence(source_root, root)
            dpa = root / "legal/dpa-residency-amendment.md"
            text = dpa.read_text(encoding="utf-8")
            dpa.write_text(
                text.replace("BYOK is not enabled or provisioned", "BYOK is enabled and provisioned", 1),
                encoding="utf-8",
            )
            errors = verify(root)
            self.assertTrue(any("B-204" in error for error in errors), errors)

    def test_b198_stale_clerk_and_design_claims_fail_closed(self) -> None:
        source_root = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory(prefix="b198-stale-verify-") as tmp:
            root = Path(tmp)
            self.copy_evidence(source_root, root)
            clerk = root / "apps/signup-worker/src/webhooks/clerk_identity.ts"
            text = clerk.read_text(encoding="utf-8")
            clerk.write_text(
                text.replace(
                    "provisioned macro (wnam/enam/weur/apac)",
                    "provisioned macro (wnam/enam/weur/sam)",
                    1,
                ).replace(
                    "REJECT unprovisioned macro regions (sam/afr today)",
                    "REJECT unprovisioned macro regions (apac/afr today)",
                    1,
                ),
                encoding="utf-8",
            )
            wp4 = root / "docs/design/2026-08-17-wp4-apac-physical-plan.md"
            wp4_text = wp4.read_text(encoding="utf-8")
            wp4.write_text(
                wp4_text.replace(
                    "current provisioned contract is `{wnam, enam, weur, apac}`",
                    "PROVISIONED_MACROS = {wnam, enam, weur}",
                    1,
                ),
                encoding="utf-8",
            )
            okf = root / "docs/knowledge/adr/adr-s14-002-region-pinning-enforcement.md"
            okf_text = okf.read_text(encoding="utf-8")
            okf.write_text(
                okf_text.replace(
                    "APAC uses the APAC-located `nrt` bucket",
                    "At launch CAS is a **single US bucket**",
                    1,
                ),
                encoding="utf-8",
            )
            errors = verify(root)
            self.assertGreaterEqual(sum("B-198" in error for error in errors), 3, errors)

    def test_b198_legal_and_generated_corpus_mutations_fail_closed(self) -> None:
        source_root = Path(__file__).resolve().parent.parent
        mutations = (
            (
                "specs/03_architecture/adrs/ADR-S14-008-dpa-amendment-schrems-ii-tia-legal-externo.md",
                "4 provisioned regions `{wnam, enam, weur, apac}`",
                "4 enumerated regions (WNAM/ENAM/WEUR/SAM)",
            ),
            (
                "legal/tia-template.md",
                "4 provisioned regions (WNAM/ENAM/WEUR/APAC)",
                "4 enumerated regions (WNAM/ENAM/WEUR/SAM)",
            ),
            (
                "legal/dpa/SUB-PROCESSOR-COMMITMENTS.md",
                "(WNAM / ENAM / WEUR / APAC)",
                "(WNAM / ENAM / WEUR / SAM)",
            ),
            (
                "docs/knowledge/adr/adr-s14-008-dpa-amendment-schrems-ii-tia-legal-externo.md",
                "`apac → nrt`; `sam` and `afr` are not provisioned",
                "`sam → gru`; `sam` and `afr` are provisioned",
            ),
            (
                "docs/okf-wiki-site/index.html",
                "`apac → nrt`; `sam` and `afr` are not provisioned",
                "`sam → gru`; `sam` and `afr` are provisioned",
            ),
            (
                "docs/knowledge/adr/adr-s14-001-multi-region-terraform-module.md",
                "with `apac → nrt`",
                "with `sam → gru`",
            ),
        )
        for rel, needle, replacement in mutations:
            with self.subTest(rel=rel):
                with tempfile.TemporaryDirectory(prefix="b198-legal-mutation-") as tmp:
                    root = Path(tmp)
                    self.copy_evidence(source_root, root)
                    path = root / rel
                    text = path.read_text(encoding="utf-8")
                    self.assertIn(needle, text, rel)
                    path.write_text(text.replace(needle, replacement, 1), encoding="utf-8")
                    errors = verify(root)
                    self.assertTrue(any("B-198" in error for error in errors), errors)


if __name__ == "__main__":
    unittest.main()
