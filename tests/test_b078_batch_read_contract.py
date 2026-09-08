from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b078_batch_read.py"
spec = importlib.util.spec_from_file_location("b078_verifier", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)


class B078ContractTests(unittest.TestCase):
    def _sources(self):
        route = (ROOT / "crates/corelink-container/src/routes/cas.rs").read_text()
        route += "\n" + "\n".join(
            (ROOT / "crates/corelink-container/src/routes/cas" / name).read_text()
            for name in verifier.CAS_ROUTE_PARTS
        )
        handler = (ROOT / "crates/corelink-handler-cas/src/request.rs").read_text()
        handler += (ROOT / "crates/corelink-handler-cas/src/handler.rs").read_text()
        storage = (ROOT / "crates/corelink-container/src/storage/r2_s3.rs").read_text()
        storage += "\n" + "\n".join(
            (ROOT / "crates/corelink-container/src/storage/r2_s3_parts" / name).read_text()
            for name in ("client.rs", "client_impl.rs", "cas_core.rs", "cas_ops.rs", "ac_core.rs", "ac_ops.rs")
        )
        return route, handler, storage

    def test_repaired_contract_is_closed(self) -> None:
        self.assertEqual(verifier.assess(ROOT), [])

    def test_unbounded_handle_fanout_mutation_reopens_memory_gate(self) -> None:
        route, handler, storage = self._sources()
        openapi = (ROOT / "openapi/corelink-v1.yaml").read_text()
        docs = (ROOT / "docs/knowledge/surfaces/native-cas.md").read_text()
        mutated = route.replace(
            "Semaphore::new(BATCH_READ_FANOUT)",
            "Semaphore::new(hashes.len())",
            1,
        )
        gaps = verifier.assess_source(mutated, handler, storage, openapi, docs)
        self.assertIn("bounded-buffer", gaps)

    def test_unbounded_task_guard_mutation_is_rejected(self) -> None:
        route, handler, storage = self._sources()
        openapi = (ROOT / "openapi/corelink-v1.yaml").read_text()
        docs = (ROOT / "docs/knowledge/surfaces/native-cas.md").read_text()
        mutated = route.replace(
            "BatchReadTaskGuard::with_capacity(BATCH_READ_FANOUT)",
            "BatchReadTaskGuard::with_capacity(hashes.len())",
            1,
        )
        gaps = verifier.assess_source(mutated, handler, storage, openapi, docs)
        self.assertIn("materialized-task-collection", gaps)

    def test_drop_abort_mutation_is_rejected(self) -> None:
        route, handler, storage = self._sources()
        openapi = (ROOT / "openapi/corelink-v1.yaml").read_text()
        docs = (ROOT / "docs/knowledge/surfaces/native-cas.md").read_text()
        start = route.index("impl Drop for BatchReadTaskGuard")
        end = route.index("async fn handle_batch_read", start)
        drop_guard = route[start:end].replace("pending.abort();", "pending.cancel();", 1)
        mutated = route[:start] + drop_guard + route[end:]
        gaps = verifier.assess_source(mutated, handler, storage, openapi, docs)
        self.assertIn("drop-abort", gaps)

    def test_batch_route_removal_mutations_reopen_completeness_gate(self) -> None:
        route, handler, storage = self._sources()
        openapi = (ROOT / "openapi/corelink-v1.yaml").read_text()
        docs = (ROOT / "docs/knowledge/surfaces/native-cas.md").read_text()

        for path, marker in (
            ("/v1/cas/{tenant}/batch", "openapi-batch-path"),
            ("/v1/cas/{tenant}/batch-exists", "openapi-batch-exists-path"),
        ):
            start = openapi.index(f"  {path}:")
            end = openapi.find("\n  /", start + 1)
            mutated = openapi[:start] + (openapi[end:] if end >= 0 else "")
            gaps = verifier.assess_source(route, handler, storage, mutated, docs)
            self.assertIn(marker, gaps)

        for constant, handler_name in (
            ("CAS_BATCH_ROUTE", "handle_batch_write"),
            ("CAS_BATCH_EXISTS_ROUTE", "handle_batch_exists"),
        ):
            registration = f"{constant},\n            post({handler_name})"
            mutated = route.replace(registration, f"{constant},\n            get({handler_name})", 1)
            gaps = verifier.assess_source(mutated, handler, storage, openapi, docs)
            self.assertIn(f"source-registration-{constant}", gaps)

    def test_cancellation_lease_mutation_is_rejected(self) -> None:
        route, handler, storage = self._sources()
        openapi = (ROOT / "openapi/corelink-v1.yaml").read_text()
        docs = (ROOT / "docs/knowledge/surfaces/native-cas.md").read_text()
        mutated = route.replace(
            "let response_lease = Arc::clone(&lease);",
            "let response_lease = lease;",
            1,
        )
        gaps = verifier.assess_source(mutated, handler, storage, openapi, docs)
        self.assertIn("cancellation-lease", gaps)

    def test_unbounded_get_capped_collection_is_rejected(self) -> None:
        route, handler, storage = self._sources()
        openapi = (ROOT / "openapi/corelink-v1.yaml").read_text()
        docs = (ROOT / "docs/knowledge/surfaces/native-cas.md").read_text()
        start = storage.index("pub async fn get_capped")
        end = storage.index("pub async fn head_size", start)
        capped = storage[start:end].replace(
            "while let Some(chunk) = body.next().await",
            "let bytes = body.collect().await?;",
            1,
        )
        mutated_storage = storage[:start] + capped + storage[end:]
        gaps = verifier.assess_source(route, handler, mutated_storage, openapi, docs)
        self.assertIn("bounded-r2-body-loop", gaps)

    def test_413_wire_contract_mutations_reopen_the_gate(self) -> None:
        route, handler, storage = self._sources()
        openapi = (ROOT / "openapi/corelink-v1.yaml").read_text()
        docs = (ROOT / "docs/knowledge/surfaces/native-cas.md").read_text()

        for mutated_openapi, expected in (
            (openapi.replace(
                '        application/json:\n          schema: { $ref: "#/components/schemas/BatchTooLargeResponse" }',
                "        text/plain:\n          schema: { type: string }",
                1,
            ),
             "openapi-413-application/json"),
            (openapi.replace(
                "      required: [error, limit_objects, limit_bytes]",
                "      required: [error]",
                1,
            ),
             "openapi-413-required: [error, limit_objects, limit_bytes]"),
            (openapi.replace(
                "        limit_bytes:\n          type: integer\n          format: int64\n          const: 8388608",
                "        bytes: { type: integer }",
                1,
            ),
             "openapi-413-limit_bytes:"),
        ):
            with self.subTest(expected=expected):
                gaps = verifier.assess_source(
                    route, handler, storage, mutated_openapi, docs
                )
                self.assertIn(expected, gaps)


if __name__ == "__main__":
    unittest.main()
