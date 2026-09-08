#!/usr/bin/env python3
"""Run bounded adversarial mutations against the B-072 scheduled-drill contract.

Each mutation is applied to a fresh temporary Worker fixture and checked by the
dependency-free static semantic oracle.  A mutation is accepted only when the
oracle goes red for the expected invariant; an unrelated subprocess failure
(for example, an unavailable Vitest installation) cannot count as a kill.
"""

from __future__ import annotations

import shutil
import sys
import tempfile
from contextlib import redirect_stderr, redirect_stdout
from dataclasses import dataclass
from io import StringIO
from pathlib import Path

from verify_b072_scheduled_drills import verify


ROOT = Path(__file__).resolve().parents[1]
WORKER = ROOT / "worker"


@dataclass(frozen=True)
class Mutation:
    name: str
    needle: str
    replacement: str
    markers: tuple[str, ...]


BEHAVIORAL_MUTATIONS = (
    Mutation(
        "non-2xx-delivery",
        "if (!response.ok)",
        "if (false)",
        ("non-2xx response guard",),
    ),
    Mutation(
        "unknown-cron-no-retry",
        'console.error("[scheduled_drill] rejected reason=unknown_cron");\n      controller.noRetry();',
        'console.error("[scheduled_drill] rejected reason=unknown_cron");',
        ("controller.noRetry()",),
    ),
    Mutation(
        "boundary-handoff-timestamp",
        "emit_at_ms: syntheticEmitAtMs(controller.scheduledTime, week)",
        "emit_at_ms: controller.scheduledTime",
        ("syntheticEmitAtMs",),
    ),
)


@dataclass(frozen=True)
class VerifierMutation:
    name: str
    relative_path: str
    needle: str
    replacement: str
    markers: tuple[str, ...]


VERIFIER_MUTATIONS = (
    VerifierMutation(
        "missing-entry-import",
        "worker/src/index.ts",
        'import { baseHandler } from "./index_fetch.js";',
        '// import { baseHandler } from "./index_fetch.js";',
        ("worker entry is missing active static import of index_fetch",),
    ),
    VerifierMutation(
        "missing-entry-scheduled-wiring",
        "worker/src/index.ts",
        "  scheduled: baseHandler.scheduled!,",
        "  // scheduled: baseHandler.scheduled!,",
        ("worker entry is missing active scheduled -> baseHandler wiring",),
    ),
    VerifierMutation(
        "missing-fetch-import",
        "worker/src/index_fetch.ts",
        'import { runScheduled } from "./index_schedule.js";',
        '// import { runScheduled } from "./index_schedule.js";',
        ("index_fetch is missing active static import of index_schedule",),
    ),
    VerifierMutation(
        "missing-fetch-scheduled-wiring",
        "worker/src/index_fetch.ts",
        "  scheduled: runScheduled,",
        "  // scheduled: runScheduled,",
        ("index_fetch is missing active scheduled -> runScheduled wiring",),
    ),
    VerifierMutation(
        "missing-scheduled-handler",
        "worker/src/index_schedule.ts",
        "export async function runScheduled(",
        "export async function removedScheduled(",
        ("scheduled handler is missing from index_schedule.ts",),
    ),
    VerifierMutation(
        "comment-handler-bait",
        "worker/src/index_schedule.ts",
        "export async function runScheduled(",
        "// export async function runScheduled(",
        ("scheduled handler is missing from index_schedule.ts",),
    ),
    VerifierMutation(
        "string-handler-bait",
        "worker/src/index_schedule.ts",
        "export async function runScheduled(",
        'const bait = "export async function runScheduled(controller, env)";\n//',
        ("scheduled handler is missing from index_schedule.ts",),
    ),
)


def make_verifier_fixture(destination: Path) -> None:
    (destination / "worker").mkdir()
    shutil.copytree(WORKER / "src", destination / "worker/src")
    shutil.copy2(ROOT / "wrangler.toml", destination / "wrangler.toml")


def run_mutation(mutation: Mutation) -> None:
    with tempfile.TemporaryDirectory(prefix=f"b072-{mutation.name}-") as temporary:
        fixture = Path(temporary)
        make_verifier_fixture(fixture)

        source_path = fixture / "worker/src/index_schedule.ts"
        source = source_path.read_text(encoding="utf-8")
        if source.count(mutation.needle) != 1:
            raise RuntimeError(f"{mutation.name}: mutation target count is not one")
        source_path.write_text(source.replace(mutation.needle, mutation.replacement), encoding="utf-8")

        output = StringIO()
        with redirect_stdout(output), redirect_stderr(output):
            result = verify(fixture)
        if result == 0:
            raise RuntimeError(f"{mutation.name}: mutant unexpectedly passed")
        missing = [marker for marker in mutation.markers if marker not in output.getvalue()]
        if missing:
            raise RuntimeError(f"{mutation.name}: oracle lacked expected failure marker(s): {missing}")
        print(f"B-072 MUTATION PASS: {mutation.name} was rejected by static oracle ({output.getvalue().strip()})")


def run_verifier_mutation(mutation: VerifierMutation) -> None:
    with tempfile.TemporaryDirectory(prefix=f"b072-verifier-{mutation.name}-") as temporary:
        root = Path(temporary)
        make_verifier_fixture(root)
        source_path = root / mutation.relative_path
        source = source_path.read_text(encoding="utf-8")
        if source.count(mutation.needle) != 1:
            raise RuntimeError(f"{mutation.name}: mutation target count is not one")
        source_path.write_text(source.replace(mutation.needle, mutation.replacement), encoding="utf-8")

        output = StringIO()
        with redirect_stdout(output), redirect_stderr(output):
            result = verify(root)
        assert_semantic_failure(mutation.name, result, output.getvalue(), mutation.markers)
        print(f"B-072 VERIFIER MUTATION PASS: {mutation.name} was rejected ({output.getvalue().strip()})")


def assert_semantic_failure(
    name: str, result: int, output: str, markers: tuple[str, ...]
) -> None:
    """Require a non-zero result *and* the reason for that exact mutant."""

    if result == 0:
        raise RuntimeError(f"{name}: verifier unexpectedly passed")
    missing = [marker for marker in markers if marker not in output]
    if missing:
        raise RuntimeError(f"{name}: oracle lacked expected semantic marker(s): {missing}")


def run_empty_output_mutation() -> None:
    """Negative control: non-zero/no-output must never count as a kill."""

    name = "empty-output-nonsemantic-failure"
    original_verify = globals()["verify"]
    globals()["verify"] = lambda _root: 1
    output = StringIO()
    try:
        with redirect_stdout(output), redirect_stderr(output):
            result = verify(Path("/nonexistent/fixture"))
    finally:
        globals()["verify"] = original_verify
    try:
        assert_semantic_failure(name, result, output.getvalue(), ("B-072 FAIL:",))
    except RuntimeError as error:
        if "oracle lacked expected semantic marker" not in str(error):
            raise
        print(f"B-072 VERIFIER MUTATION PASS: {name} was rejected ({error})")
        return
    raise RuntimeError(f"{name}: empty nonsemantic failure escaped the oracle")


def run_dead_code_mutation() -> None:
    """Hide the complete handler body behind if(false), retaining all tokens."""

    name = "dead-code-no-op-handler"
    with tempfile.TemporaryDirectory(prefix=f"b072-verifier-{name}-") as temporary:
        root = Path(temporary)
        make_verifier_fixture(root)
        source_path = root / "worker/src/index_schedule.ts"
        source = source_path.read_text(encoding="utf-8")
        start = source.index("export async function runScheduled")
        opening = source.index("{", start) + 1
        closing = source.rstrip().rfind("}")
        if closing <= opening:
            raise RuntimeError(f"{name}: handler body boundaries are invalid")
        mutant = (
            source[:opening]
            + "\n    if (false) {"
            + source[opening:closing]
            + "\n    }\n}\n"
        )
        source_path.write_text(mutant, encoding="utf-8")

        output = StringIO()
        with redirect_stdout(output), redirect_stderr(output):
            result = verify(root)
        marker = "scheduled handler body is unreachable/no-op"
        if result == 0 or marker not in output.getvalue():
            raise RuntimeError(f"{name}: oracle did not identify the unreachable handler body")
        print(f"B-072 VERIFIER MUTATION PASS: {name} was rejected ({output.getvalue().strip()})")


def main() -> int:
    try:
        for mutation in VERIFIER_MUTATIONS:
            run_verifier_mutation(mutation)
        for mutation in BEHAVIORAL_MUTATIONS:
            run_mutation(mutation)
        run_dead_code_mutation()
        run_empty_output_mutation()
    except (OSError, RuntimeError) as error:
        print(f"B-072 MUTATION FAIL: {error}", file=sys.stderr)
        return 1
    total = len(VERIFIER_MUTATIONS) + len(BEHAVIORAL_MUTATIONS) + 2
    print(f"B-072 MUTATION PASS: {total}/{total} bounded regressions rejected")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
