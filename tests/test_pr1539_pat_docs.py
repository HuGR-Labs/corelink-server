"""Focal tests for the PR #1539 PAT documentation verifier."""

from __future__ import annotations

import importlib.util
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_pr1539_pat_docs", ROOT / "scripts/verify_pr1539_pat_docs.py"
)
assert SPEC and SPEC.loader
VERIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFIER)


def test_current_four_docs_are_aligned() -> None:
    assert VERIFIER.verify_sources(ROOT) == []
    assert VERIFIER.verify_executable_sources(ROOT) == []
    assert VERIFIER.verify_texts(VERIFIER.load(ROOT)) == []


def test_every_positive_contract_marker_is_adversarially_load_bearing() -> None:
    cases = VERIFIER.mutation_cases(VERIFIER.load(ROOT))
    assert cases
    undetected = [(name, label) for name, label, diagnostics in cases if not diagnostics]
    assert not undetected


def test_comment_preserving_implementation_mutations_are_rejected() -> None:
    cases = VERIFIER.executable_mutation_cases(ROOT)
    assert cases
    undetected = [label for label, diagnostics in cases if not diagnostics]
    assert not undetected


def test_comment_bait_does_not_satisfy_signup_executable_anchors() -> None:
    baited_sources = {}
    for relative, old in (
        (
            "crates/corelink-container/src/routes/signup.rs",
            "    let tenant_id = Uuid::now_v7();\n",
        ),
        (
            "apps/signup-worker/src/webhooks/clerk.ts",
            "  const tenantId = crypto.randomUUID();\n",
        ),
    ):
        original = (ROOT / relative).read_text(encoding="utf-8")
        baited_sources[relative] = original.replace(old, "    // " + old.strip() + "\n", 1)

    diagnostics = VERIFIER.verify_executable_sources_text(baited_sources)
    assert any("pilot signup UUIDv7" in error for error in diagnostics)
    assert any("customer Clerk signup UUIDv4" in error for error in diagnostics)


def test_block_template_and_rust_raw_literals_do_not_satisfy_anchors() -> None:
    fixtures = (
        (
            "worker/src/index.ts",
            """/*
if (path.startsWith(\"/bazel/v2/\")) {
  const rest = path.slice(\"/bazel/v2/\".length);
  const tenant = extractFirstSegment(\"/\" + rest) ?? \"_anonymous\";
  return { tenantId: tenant, pathSuffix: path, routeKind: \"bazel_v2\" };
}
*/""",
            "Worker Bazel tenant extraction",
        ),
        (
            "worker/src/index.ts",
            """`if (path.startsWith(\"/bazel/v2/\")) {
  const rest = path.slice(\"/bazel/v2/\".length);
  const tenant = extractFirstSegment(\"/\" + rest) ?? \"_anonymous\";
  return { tenantId: tenant, pathSuffix: path, routeKind: \"bazel_v2\" };
}`""",
            "Worker Bazel tenant extraction",
        ),
        (
            "crates/corelink-bazel-bridge/src/adapter.rs",
            r'''r###"fn check_tenant(instance: &str, caller_tenant: &str) -> Result<(), BazelBridgeError> {
    if instance != caller_tenant {
        return Err(BazelBridgeError::TenantMismatch);
    }
}"###''',
            "Bazel adapter instance comparison",
        ),
    )
    for relative, fixture, label in fixtures:
         diagnostics = VERIFIER.verify_executable_sources_text({relative: fixture})
         assert any(label in error for error in diagnostics), (relative, diagnostics)
