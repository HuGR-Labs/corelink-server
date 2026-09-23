from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b057_sli.py"
spec = importlib.util.spec_from_file_location("b057_verifier", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)


class B057ContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.storage = verifier.storage_source(ROOT)
        cls.aggregate = (ROOT / "crates/corelink-container/src/sli_aggregate.rs").read_text()
        cls.docs = (ROOT / "docs/knowledge/storage/r2-cas-bucket.md").read_text()

    def test_repaired_contract_is_closed(self) -> None:
        self.assertEqual(verifier.assess(ROOT), [])

    def test_consumer_removal_reopens_gate(self) -> None:
        mutated = self.aggregate.replace("BurnRateCalculator::new().decide", "CalculatorRemoved.decide", 1)
        gaps = verifier.assess_source(self.storage, mutated, self.docs)
        self.assertIn("burn-rate-calculator-consumer", gaps)

    def test_ac_zero_latency_mutation_reopens_gate(self) -> None:
        mutated = self.storage.replace(
            "self.emit_lookup_sli(true, elapsed_us(started));",
            "self.emit_lookup_sli(true, 0);",
            1,
        )
        gaps = verifier.assess_source(mutated, self.aggregate, self.docs)
        self.assertIn("ac-lookup-zero-latency", gaps)

    def test_unbounded_production_sink_mutation_reopens_gate(self) -> None:
        mutated = self.storage.replace(
            "let sli = crate::sli_aggregate::shared();",
            "let sli = Arc::new(InMemorySliObserver::new());",
            1,
        )
        gaps = verifier.assess_source(mutated, self.aggregate, self.docs)
        self.assertIn("unbounded-production-sli-sink", gaps)

    def test_temporal_window_mutation_reopens_gate(self) -> None:
        mutated = self.aggregate.replace("window_counters_at", "temporal_window_sample")
        gaps = verifier.assess_source(self.storage, mutated, self.docs)
        self.assertIn("temporal-window-counters-at", gaps)

    def test_each_audit_failure_path_emits_an_error_sli(self) -> None:
        for operation, start, end, emit in verifier.AUDIT_OPERATION_SECTIONS:
            begin = self.storage.index(start)
            finish = self.storage.index(end, begin + len(start))
            section = self.storage[begin:finish]
            token = f"self.{emit}(true, elapsed_us(started));"
            self.assertIn(token, section, operation)
            mutated_section = section.replace(token, "// audit SLI removed", 1)
            mutated = self.storage[:begin] + mutated_section + self.storage[finish:]
            gaps = verifier.assess_source(mutated, self.aggregate, self.docs)
            self.assertIn(f"audit-failure-sli-{operation}", gaps)

    def test_list_does_not_pollute_lookup_hit_latency_catalog(self) -> None:
        begin = self.storage.index("impl corelink_handler_ac::AcListHandler")
        finish = self.storage.index("/// Build an `R2AcHandler`", begin)
        mutated_section = self.storage[begin:finish].replace(
            "self.emit_list_sli(false, elapsed_us(started));",
            "self.emit_lookup_sli(false, elapsed_us(started));",
            1,
        )
        mutated = self.storage[:begin] + mutated_section + self.storage[finish:]
        gaps = verifier.assess_source(mutated, self.aggregate, self.docs)
        self.assertIn("latency-catalog-list-is-hit", gaps)


if __name__ == "__main__":
    unittest.main()
