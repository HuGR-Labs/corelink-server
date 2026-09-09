#!/usr/bin/env python3
"""Static and mutation checks for the B-068 real-harness executor.

This verifier is intentionally dependency-free.  The executor is an allow-list
of exact test targets, so a successful grep for a phrase in a comment is not
evidence that a harness is wired.  The checks below ignore comment-only lines,
then run negative mutations for the failure modes that previously made this
backlog item look closed: partial selection, wrong tests, secret exposure,
comment bait, and missing workflow triggers.
"""

from __future__ import annotations

import hashlib
import re
import os
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW_PATH = ROOT / ".github/workflows/real-ignored-harnesses.yml"
RUNNER_PATH = ROOT / "scripts/run-real-ignored-harnesses.sh"
SEED_PATH = ROOT / "crates/corelink-pat/tests/emit_e2e_seed.rs"

REQUIRED_D1 = (
    "d1_acquire_lock_then_held_then_release",
    "d1_dpa_and_active_subscription_reads",
    "d1_persist_free_active_does_not_count_as_a_subscription",
    "d1_http_cas_meta_round_trip",
    "d1_http_tenant_admin_lookup_round_trip",
    "d1_audit_write_blocking_records_oaudit_phase",
)
REQUIRED_R2 = (
    "r2_cas_list_durable_audit_failure_precedes_storage",
    "storage_r2_round_trip",
    "cas_idempotent_rewrite_reports_durable_false",
    "delete_if_present_credits_size_once_then_none",
    "r2_cas_exists_batch_fails_closed_on_bad_audit_creds",
)

REQUIRED_TARGET_SOURCES = {
    **{target: "crates/corelink-container/src/routes/tier_select_store.rs" for target in REQUIRED_D1[:3]},
    "d1_http_cas_meta_round_trip": "crates/corelink-container/src/storage/d1_http.rs",
    "d1_http_tenant_admin_lookup_round_trip": "crates/corelink-container/src/storage/d1_http.rs",
    "d1_audit_write_blocking_records_oaudit_phase": "crates/corelink-container/src/storage/d1_audit_sink/tests_phase_attribution.rs",
    "r2_cas_list_durable_audit_failure_precedes_storage": "crates/corelink-container/src/storage/r2_s3_parts/tests_1_network.rs",
    "storage_r2_round_trip": "crates/corelink-container/src/storage/r2_s3_parts/tests_1_network.rs",
    "cas_idempotent_rewrite_reports_durable_false": "crates/corelink-container/src/storage/r2_s3_parts/tests_1_network.rs",
    "delete_if_present_credits_size_once_then_none": "crates/corelink-container/src/storage/r2_s3_parts/tests_1_network.rs",
    "r2_cas_exists_batch_fails_closed_on_bad_audit_creds": "crates/corelink-container/src/storage/r2_s3_parts/tests_2.rs",
}

# Byte-locked source manifest for the exact files containing the 11 selected
# harnesses. This is intentionally reviewed data, not a generated claim: any
# legitimate source edit (including cfg_attr/raw/unicode/macro changes) must
# update this manifest in the same reviewed change before semantic checks can
# run. The self-hosted runner's PATH/toolchain remains the infrastructure trust
# boundary; this manifest binds the repository-owned selector/source inputs.
SOURCE_SHA256 = {
    "crates/corelink-container/src/routes/tier_select_store.rs": "ea65f1134e2055468226b8e62e8b319244c34e48fe08834b42b2a1df2581f712",
    "crates/corelink-container/src/storage/d1_http.rs": "6c1932b0be0b578469c6710c06cbe16ca1b437b9248b085f4671e22822c3e81a",
    "crates/corelink-container/src/storage/d1_audit_sink/tests_phase_attribution.rs": "474d45a030f333bfb73d7152bc2a802d9d29b8af2d559c5310f9a683bc74e717",
    "crates/corelink-container/src/storage/r2_s3_parts/tests_1_network.rs": "9a946473e1f76d1af26958cdbefca8ac641dbfaeb2066af7e7a8a5d302df0f8d",
    "crates/corelink-container/src/storage/r2_s3_parts/tests_2.rs": "3c46817aa4a3f758768297ad13baa7033088c27a25848531bcc3cda280451779",
}


def code_lines(text: str) -> list[str]:
    """Return non-blank, non-comment lines.

    We deliberately do not use a broad regex over the raw file: an attacker
    (or a stale handoff) must not be able to satisfy a gate by leaving the
    required command only in a comment.
    """

    return [line for line in text.splitlines() if line.strip() and not line.lstrip().startswith("#")]


def code_text(text: str) -> str:
    return "\n".join(code_lines(text))


def workflow_run_lines(text: str) -> list[str]:
    """Extract active YAML run scalars, excluding inline comment bait."""
    runs: list[str] = []
    in_block = False
    indent = 0
    block_lines = 0
    for raw in text.splitlines():
        stripped = raw.lstrip()
        if not stripped or stripped.startswith("#"):
            continue
        if in_block:
            current = len(raw) - len(stripped)
            if current <= indent:
                in_block = False
            else:
                runs.append(stripped.split(" #", 1)[0].rstrip())
                block_lines += 1
                continue
        match = re.match(r"^(\s*)run:\s*([|>])?\s*(.*)$", raw)
        if not match:
            continue
        if match.group(2):
            in_block = True
            indent = len(match.group(1))
            block_lines = 0
            continue
        value = match.group(3).split(" #", 1)[0].rstrip()
        if value:
            runs.append(value)
    if in_block and block_lines == 0:
        raise AssertionError("workflow run block is unterminated")
    return runs


def case_branch(text: str, label: str) -> str:
    """Return one executable branch from the runner's allow-list case."""

    lines = code_lines(text)
    marker = f"{label})"
    try:
        start = next(i for i, line in enumerate(lines) if line.strip().startswith(marker))
    except StopIteration as exc:
        raise AssertionError(f"runner case branch is missing: {label}") from exc
    first_remainder = lines[start].strip()[len(marker) :].strip()
    if ";;" in first_remainder:
        return first_remainder.split(";;", 1)[0].strip()
    body: list[str] = [first_remainder] if first_remainder else []
    for line in lines[start + 1 :]:
        if line.strip() == ";;":
            return "\n".join(body)
        body.append(line)
    raise AssertionError(f"runner case branch is unterminated: {label}")


def function_body(text: str, name: str) -> str:
    """Extract a simple shell function body for side-effect checks."""

    marker = f"{name}() {{"
    start = text.find(marker)
    if start < 0:
        fail(f"runner function is missing: {name}")
    end = text.find("\n}", start)
    if end < 0:
        fail(f"runner function is unterminated: {name}")
    return text[start:end]


def fail(message: str) -> None:
    raise AssertionError(message)


def verify_source_digests(root: Path = ROOT, overrides: dict[str, bytes] | None = None) -> None:
    """Fail before semantic parsing if any bound source byte changed."""
    for relative, expected in SOURCE_SHA256.items():
        target = root / relative
        body = overrides[relative] if overrides and relative in overrides else target.read_bytes()
        actual = hashlib.sha256(body).hexdigest()
        if actual != expected:
            fail(f"source digest mismatch (reviewed manifest required): {relative}")


def rust_code_without_comments_and_strings(body: str) -> str:
    """Blank Rust comments/strings (including raw strings) but keep newlines."""
    out: list[str] = []
    state = "code"
    block_depth = 0
    raw_hashes = 0
    escaped = False
    i = 0
    while i < len(body):
        char = body[i]
        nxt = body[i + 1] if i + 1 < len(body) else ""
        if state == "code":
            if char == "/" and nxt == "/":
                out.extend("  "); i += 2; state = "line"; continue
            if char == "/" and nxt == "*":
                out.extend("  "); i += 2; block_depth = 1; state = "block"; continue
            if char == "r":
                marker = re.match(r'r(#{0,255})"', body[i:])
                if marker:
                    raw_hashes = len(marker.group(1)); out.extend(" " * len(marker.group(0)))
                    i += len(marker.group(0)); state = "raw"; continue
            if char in ('"', "'"):
                out.append(" "); i += 1; state = char; escaped = False; continue
            out.append(char); i += 1; continue
        if state == "line":
            out.append("\n" if char == "\n" else " "); i += 1
            if char == "\n": state = "code"
            continue
        if state == "block":
            if char == "/" and nxt == "*": out.extend("  "); i += 2; block_depth += 1; continue
            if char == "*" and nxt == "/":
                out.extend("  "); i += 2; block_depth -= 1
                if block_depth == 0: state = "code"
                continue
            out.append("\n" if char == "\n" else " "); i += 1; continue
        if state == "raw":
            closing = '"' + ('#' * raw_hashes)
            if body.startswith(closing, i):
                out.extend(" " * len(closing)); i += len(closing); state = "code"; continue
            out.append("\n" if char == "\n" else " "); i += 1; continue
        out.append("\n" if char == "\n" else " "); i += 1
        if escaped: escaped = False
        elif char == "\\": escaped = True
        elif char == state: state = "code"
    return "".join(out)


def exact_ignored_source(body: str, target: str) -> bool:
    code = rust_code_without_comments_and_strings(body)
    pattern = rf"(?m)^[ \t]*(?:(?:pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?fn\s+{re.escape(target)}\s*\()"
    declarations = list(re.finditer(pattern, code))
    if len(declarations) != 1:
        return False
    declaration = declarations[0]

    def balanced_end(opening: int) -> int:
        matching = {"{": "}", "(": ")", "[": "]"}
        stack = [code[opening]]
        cursor = opening + 1
        while cursor < len(code) and stack:
            char = code[cursor]
            if char in matching:
                stack.append(char)
            elif char == matching[stack[-1]]:
                stack.pop()
            cursor += 1
        return cursor

    # A macro_rules! body is not an executable test declaration: Cargo can
    # compile it successfully while never expanding/invoking it. Rust macro
    # bodies may use any of {}, (), or [] as their outer delimiter.
    for macro in re.finditer(r"macro_rules!\s*[A-Za-z_][A-Za-z0-9_]*\s*([\{\(\[])", code):
        end = balanced_end(macro.start(1))
        if macro.end() <= declaration.start() < end:
            return False

    # cfg on the crate/module containing this harness can silently remove the
    # test from the compiled target. These real harnesses must always compile;
    # reject every cfg attribute rather than trying to evaluate expressions.
    if re.search(r"(?m)^\s*#!\[\s*cfg(?:\s*\(|\s*\])", code[:declaration.start()]):
        return False

    line_start = code.rfind("\n", 0, declaration.start()) + 1
    prior = code[:line_start].splitlines()
    attrs: list[str] = []
    while prior and re.fullmatch(r"\s*#\[[^\n]*\]\s*", prior[-1]):
        attrs.insert(0, prior.pop().strip())
    if not attrs or not re.fullmatch(r"#\[ignore(?:\s*=\s*[^]]+)?\]", attrs[-1]):
        return False
    if not any(re.search(r"#\[\s*(?:tokio::)?test(?:\s*\(|\s*\])", attr, re.IGNORECASE) for attr in attrs):
        return False
    if any(re.match(r"#\[\s*cfg(?:\s*\(|\s*\])", attr, re.IGNORECASE) for attr in attrs):
        return False

    # Find enclosing module items and inspect only their contiguous attributes;
    # unrelated cfg modules elsewhere in the source do not taint this target.
    for module in re.finditer(r"(?m)^[ \t]*(?:(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+[A-Za-z_][A-Za-z0-9_]*\s*\{)", code):
        end = balanced_end(code.find("{", module.start(), module.end()))
        if module.end() <= declaration.start() < end:
            module_line = code.rfind("\n", 0, module.start()) + 1
            module_prior = code[:module_line].splitlines()
            module_attrs: list[str] = []
            while module_prior and re.fullmatch(r"\s*#\[[^\n]*\]\s*", module_prior[-1]):
                module_attrs.insert(0, module_prior.pop().strip())
            if any(re.match(r"#\[\s*cfg(?:\s*\(|\s*\])", attr, re.IGNORECASE) for attr in module_attrs):
                return False
    return True


def assert_contract(workflow: str, runner: str) -> None:
    # Digest binding is the first source gate; parser/attribute checks are
    # defense in depth and must never silently bless a changed source file.
    verify_source_digests()
    wf = code_text(workflow)
    sh = code_text(runner)
    runs = workflow_run_lines(workflow)

    # The only event that may inject credentials is a deliberate operator
    # dispatch.  Anchoring to YAML keys keeps prose/comment bait irrelevant.
    if not re.search(r"(?m)^on:\s*$", wf):
        fail("workflow has no top-level on block")
    if not re.search(r"(?m)^\s{2}workflow_dispatch:\s*(?:\{\})?\s*$", wf):
        fail("workflow_dispatch trigger is missing")
    for event in ("pull_request", "pull_request_target", "push", "schedule", "workflow_call"):
        if re.search(rf"(?m)^\s{{2}}{re.escape(event)}:\s*", wf):
            fail(f"untrusted/automatic trigger is present: {event}")
    if 'test "$GITHUB_REF" = "refs/heads/main"' not in wf:
        fail("executor is not pinned to the protected main ref")
    if 'test "$GITHUB_EVENT_NAME" = "workflow_dispatch"' not in wf:
        fail("executor does not fail closed on event type")
    if "HuGR-Labs/corelink-server" not in wf:
        fail("executor is not repository-scoped")

    # The workflow must have a selectable, bounded profile set.
    for profile in ("d1", "r2", "stripe", "neon", "all"):
        if not re.search(rf"(?m)^\s+-\s+{profile}\s*$", wf):
            fail(f"workflow profile option is missing: {profile}")
    if "environment: real-integration" not in wf:
        fail("real integration environment approval boundary is missing")
    if "persist-credentials: false" not in wf:
        fail("checkout credential persistence is not disabled")
    if "python3 scripts/verify_real_ignored_harnesses.py" not in wf:
        fail("workflow does not run the semantic executor guard")
    if 'REAL_HARNESS_PROFILE: ${{ inputs.profile }}' not in wf:
        fail("workflow does not pass the profile through an environment variable")
    if 'bash scripts/run-real-ignored-harnesses.sh "$REAL_HARNESS_PROFILE"' not in wf:
        fail("workflow interpolates the dispatch input into shell source")
    if 'bash scripts/run-real-ignored-harnesses.sh "${{ inputs.profile }}"' in wf:
        fail("dispatch input is interpolated directly into shell source")
    if runs.count("python3 scripts/verify_real_ignored_harnesses.py") != 1:
        fail("semantic executor guard is missing or only comment bait")
    if runs.count('bash scripts/run-real-ignored-harnesses.sh "$REAL_HARNESS_PROFILE"') != 1:
        fail("real executor wiring is missing or only comment bait")

    # No path in this executor may receive the PAT signing key or invoke the
    # side-effecting seed.  Check executable content, not explanatory comments.
    forbidden = ("CORELINK_PAT_SIGNING_KEY_HEX", "emit_e2e_seed", "PAT_PLAINTEXT", "SEED_SQL")
    for token in forbidden:
        if token in wf or token in sh:
            fail(f"PAT seed/secret reached the executor: {token}")
    if "CORELINK_PAT_SIGNING_KEY_HEX" in workflow or "emit_e2e_seed" in workflow:
        fail("workflow comments must not create a seed/secret-shaped selector")

    # Exact target coverage: every real ignored class has an executor, and the
    # executor cannot quietly replace one with an unrelated test.
    for target in REQUIRED_D1 + REQUIRED_R2:
        if target not in sh:
            fail(f"required real target is not selected: {target}")
    required_fragments = (
        "run_cargo --package corelink-stripe-real --features live-integration --test live_integration",
        "run_cargo --package corelink-audit-chain --features neon-real --test neon_shadow_real",
        'case "$PROFILE" in',
        "d1) preflight_d1; run_d1",
        "r2) preflight_r2; run_r2",
        "stripe) preflight_stripe; run_stripe",
        "neon) preflight_neon; run_neon",
        "all)",
        'die "unknown harness profile:',
        "--ignored --nocapture",
    )
    for fragment in required_fragments:
        if fragment not in sh:
            fail(f"executor allow-list fragment is missing: {fragment}")

    # Preflights must be pure checks.  In particular, `all` must finish every
    # check before its first cargo invocation, otherwise a missing late secret
    # could leave earlier real tests with external side effects.
    for name in ("preflight_d1", "preflight_r2", "preflight_stripe", "preflight_neon"):
        body = code_text(function_body(runner, name))
        if "run_cargo" in body or re.search(r"\bcargo\b", body):
            fail(f"preflight has a side effect: {name}")
    all_branch = case_branch(runner, "all")
    expected_preflights = ("preflight_d1", "preflight_r2", "preflight_stripe", "preflight_neon")
    positions = [all_branch.find(name) for name in expected_preflights]
    if any(position < 0 for position in positions) or positions != sorted(positions):
        fail("all profile does not run every preflight in order")
    first_run = all_branch.find("run_d1")
    if first_run < 0 or first_run < positions[-1]:
        fail("all profile invokes a harness before the final preflight")
    for profile, preflight, runner_fn in (
        ("d1", "preflight_d1", "run_d1"),
        ("r2", "preflight_r2", "run_r2"),
        ("stripe", "preflight_stripe", "run_stripe"),
        ("neon", "preflight_neon", "run_neon"),
    ):
        branch = case_branch(runner, profile)
        if branch.find(preflight) < 0 or branch.find(runner_fn) < branch.find(preflight):
            fail(f"{profile} profile does not preflight before execution")

    # Environment preconditions are part of correctness: absent credentials
    # must fail before a test can fall back to an in-memory adapter.
    for name in (
        "CLOUDFLARE_ACCOUNT_ID",
        "CF_API_TOKEN",
        "D1_DATABASE_ID",
        "R2_S3_ENDPOINT",
        "R2_S3_ACCESS_KEY_ID",
        "R2_S3_SECRET_ACCESS_KEY",
        "R2_TEST_BUCKET",
        "HUGR_WALLET_BASE",
        "HUGR_WALLET_TOKEN",
        "HUGR_STRIPE_REF",
        "STRIPE_AUTH_MODE",
        "STRIPE_PRICE_ID_STARTER",
        "NEON_TEST_DSN",
    ):
        if name not in sh:
            fail(f"credential/config precondition is missing: {name}")
    if '[[ "$HUGR_STRIPE_REF" == stripe-prod-test ]]' not in runner:
        fail("Stripe profile is not pinned to the test wallet reference")
    if '[[ "$HUGR_WALLET_TOKEN" == hugrw_* ]]' not in runner:
        fail("Stripe profile does not validate wallet-token shape")
    if '[[ "$STRIPE_AUTH_MODE" == wallet-broker ]]' not in runner:
        fail("Stripe profile does not force wallet-broker auth")
    if '[[ "$STRIPE_PRICE_ID_STARTER" == price_* ]]' not in runner:
        fail("Stripe profile does not validate Starter price id")

    # A generic caller must not be able to select an arbitrary cargo target.
    if re.search(r"(?m)^\s*cargo\s+test\s+.*--ignored", wf):
        fail("workflow invokes cargo ignored tests directly instead of the allow-list")
    if re.search(r"(?m)^\s*cargo\s+test\s+--ignored", sh):
        fail("runner contains an unscoped cargo --ignored invocation")
    if "CARGO_BIN" in sh or "CARGO_BIN" in wf:
        fail("executor must not honor a caller-controlled CARGO_BIN override")
    if 'cargo test --locked "$@" -- --ignored --nocapture' not in sh:
        fail("runner does not execute the trusted image Cargo through PATH")

    for target, source_path in REQUIRED_TARGET_SOURCES.items():
        source = ROOT / source_path
        if not source.is_file():
            fail(f"source for real target is missing: {target}: {source_path}")
        body = source.read_text(encoding="utf-8")
        if not exact_ignored_source(body, target):
            fail(f"real target source declaration/ignore association is not exact: {target}")
        if not re.search(rf"(?m)^\s*run_cargo\b[^\n]*\b{re.escape(target)}\b", sh):
            fail(f"runner does not execute exact source target: {target}")

    # The PAT seed itself remains deliberately ignored and secret-shaped.  This
    # assertion prevents a future cleanup from deleting the safety boundary.
    seed = SEED_PATH.read_text(encoding="utf-8")
    if "#[ignore" not in seed or "CORELINK_PAT_SIGNING_KEY_HEX" not in seed:
        fail("PAT seed safety boundary changed: it must remain ignored and key-gated")
    if "PAT_PLAINTEXT" not in seed or "SEED_SQL" not in seed:
        fail("PAT seed output markers changed; review before changing executor policy")


def expect_rejected(label: str, workflow: str, runner: str) -> None:
    try:
        assert_contract(workflow, runner)
    except AssertionError:
        return
    fail(f"negative mutation was accepted: {label}")


def mutation_checks(workflow: str, runner: str) -> None:
    # Missing trigger: a manually documented lane is not an executor.
    expect_rejected(
        "missing workflow_dispatch",
        workflow.replace("  workflow_dispatch:\n", "  # workflow_dispatch:\n", 1),
        runner,
    )
    expect_rejected(
        "workflow command replaced with inline comment bait",
        workflow.replace(
            '          bash scripts/run-real-ignored-harnesses.sh "$REAL_HARNESS_PROFILE"',
            '          echo "# bash scripts/run-real-ignored-harnesses.sh \\"$REAL_HARNESS_PROFILE\\""',
            1,
        ),
        runner,
    )
    # Automatic PR execution would expose network credentials to untrusted code.
    expect_rejected("pull_request trigger", workflow.replace("  workflow_dispatch:\n", "  pull_request:\n  workflow_dispatch:\n", 1), runner)
    # Comment bait: a commented-out command is not executable coverage.
    target = REQUIRED_D1[0]
    expect_rejected("commented target", workflow, runner.replace(f"run_cargo --package corelink-server --lib {target}", f"# run_cargo --package corelink-server --lib {target}", 1))
    # Partial executor: dropping any exact real target must be detected.
    expect_rejected("partial D1 executor", workflow, runner.replace(REQUIRED_D1[-1], "d1_target_removed", 1))
    expect_rejected("partial R2 executor", workflow, runner.replace(REQUIRED_R2[-1], "r2_target_removed", 1))
    # Wrong-test substitution must not look like proof of the intended path.
    expect_rejected("wrong test target", workflow, runner.replace("storage_r2_round_trip", "unrelated_unit_test", 1))
    expect_rejected("caller Cargo override", workflow, runner.replace('cargo test --locked "$@" -- --ignored --nocapture', '"$CARGO_BIN" test --locked "$@" -- --ignored --nocapture', 1))
    expect_rejected("missing R2 source target", workflow, runner.replace("r2_cas_list_durable_audit_failure_precedes_storage", "r2_cas_list_target_removed", 1))
    expect_rejected("R2 wrong source target", workflow, runner.replace("r2_cas_list_durable_audit_failure_precedes_storage", "r2_cas_list_serial_fallback_attributes_the_r2_call_to_ostore", 1))

    source_target = REQUIRED_R2[0]
    source_path = ROOT / REQUIRED_TARGET_SOURCES[source_target]
    source_bytes = source_path.read_bytes()
    source_body = source_bytes.decode("utf-8")
    # Every unique source file gets a byte mutation, proving the manifest is
    # fail-closed independently of which harness file was edited.
    for relative in SOURCE_SHA256:
        original = (ROOT / relative).read_bytes()
        try:
            verify_source_digests(overrides={relative: original + b"\n// B-068 digest mutation\n"})
        except AssertionError:
            pass
        else:
            fail(f"byte mutation was accepted for source digest: {relative}")
    cfg_attr_mutation = source_bytes.replace(b"#[ignore", b"#[cfg_attr(any(), ignore)]\n#[ignore", 1)
    try:
        verify_source_digests(overrides={REQUIRED_TARGET_SOURCES[source_target]: cfg_attr_mutation})
    except AssertionError:
        pass
    else:
        fail("cfg_attr source mutation was accepted by digest")
    for label, mutation in (
        ("raw", source_bytes + b'\nr##"#[cfg(any())]"##\n'),
        ("unicode", source_bytes + "\n// B-068 \u2603\n".encode("utf-8")),
    ):
        try:
            verify_source_digests(overrides={REQUIRED_TARGET_SOURCES[source_target]: mutation})
        except AssertionError:
            pass
        else:
            fail(f"{label} source mutation was accepted by digest")
    declaration = re.search(rf"(?m)^(\s*)(async\s+fn\s+{re.escape(source_target)}\s*\()", source_body)
    if declaration is None:
        fail("source mutation setup could not find declaration")
    declaration_line_start = source_body.rfind("\n", 0, declaration.start()) + 1
    ignore_line_start = source_body.rfind("\n", 0, declaration_line_start - 1) + 1
    source_without_ignore = source_body[:ignore_line_start] + "// #[ignore]\n" + source_body[declaration_line_start:]
    if exact_ignored_source(source_without_ignore, source_target):
        fail("source comment bait mutation was accepted")
    source_without_declaration = source_body[:declaration.start()] + "// " + source_body[declaration.start():]
    if exact_ignored_source(source_without_declaration, source_target):
        fail("commented source declaration mutation was accepted")
    valid_attrs = "#[tokio::test]\n#[ignore]\nasync fn target() {}"
    if exact_ignored_source(valid_attrs.replace("#[tokio::test]", "#[ignore]"), "target"):
        fail("missing active test attribute mutation was accepted")
    if exact_ignored_source("#[cfg(any())]\n" + valid_attrs, "target"):
        fail("disabled cfg mutation was accepted")
    if exact_ignored_source("#[cfg(feature = \"never\")]\nmod disabled {\n" + valid_attrs + "\n}", "target"):
        fail("cfg module mutation was accepted")
    for opening, closing in (("{", "}"), ("(", ")"), ("[", "]")):
        if exact_ignored_source(f"macro_rules! unused {opening}\n" + valid_attrs + f"\n{closing}", "target"):
            fail(f"uninvoked macro declaration mutation was accepted: {opening}")
    # Secret exposure: any PAT key-shaped input is forbidden, even if no seed
    # command is present.
    expect_rejected("PAT signing secret", workflow, runner + "\nexport CORELINK_PAT_SIGNING_KEY_HEX=unsafe\n")
    # A direct shell interpolation reintroduces command/injection ambiguity.
    expect_rejected("direct profile interpolation", workflow.replace('bash scripts/run-real-ignored-harnesses.sh "$REAL_HARNESS_PROFILE"', 'bash scripts/run-real-ignored-harnesses.sh "${{ inputs.profile }}"', 1), runner)
    # The Stripe mode and price are both load-bearing; accepting either missing
    # value would silently exercise another auth/price configuration.
    expect_rejected("missing wallet-broker mode", workflow, runner.replace('[[ "$STRIPE_AUTH_MODE" == wallet-broker ]]', '[[ "$STRIPE_AUTH_MODE" == any-mode ]]', 1))
    expect_rejected("missing Starter price", workflow, runner.replace('[[ "$STRIPE_PRICE_ID_STARTER" == price_* ]]', '[[ "$STRIPE_PRICE_ID_STARTER" == any_* ]]', 1))
    # A preflight that contains a cargo call can mutate the external system
    # before a later profile is checked.
    expect_rejected("cargo in D1 preflight", workflow, runner.replace("  require_https R2_S3_ENDPOINT\n}\n\npreflight_r2", "  require_https R2_S3_ENDPOINT\n  run_cargo --package corelink-server --lib d1_target\n}\n\npreflight_r2", 1))
    expect_rejected("late Neon check omitted from all", workflow, runner.replace("    preflight_neon\n    run_d1", "    run_d1", 1))


def preflight_runtime_checks() -> None:
    """Prove late missing prerequisites result in zero cargo invocations."""

    baseline = {
        "CLOUDFLARE_ACCOUNT_ID": "account",
        "CF_API_TOKEN": "cf-token",
        "D1_DATABASE_ID": "database",
        "R2_S3_ENDPOINT": "https://r2.example.test",
        "R2_S3_ACCESS_KEY_ID": "access",
        "R2_S3_SECRET_ACCESS_KEY": "secret",
        "R2_TEST_BUCKET": "bucket",
        "HUGR_WALLET_BASE": "https://wallet.example.test",
        "HUGR_WALLET_TOKEN": "hugrw_test",
        "HUGR_STRIPE_REF": "stripe-prod-test",
        "STRIPE_AUTH_MODE": "wallet-broker",
        "STRIPE_PRICE_ID_STARTER": "price_test",
        "NEON_TEST_DSN": "postgresql://user:pass@db.example.test/shadow?sslmode=require",
    }
    with tempfile.TemporaryDirectory(prefix="b068-preflight-") as temp:
        root = Path(temp)
        for missing in ("HUGR_WALLET_TOKEN", "STRIPE_PRICE_ID_STARTER", "NEON_TEST_DSN"):
            marker = root / f"cargo-{missing}"
            fake_cargo = root / f"fake-cargo-{missing}"
            fake_cargo.write_text(
                "#!/bin/sh\n"
                f"printf invoked > {marker}\n"
                "exit 99\n",
                encoding="utf-8",
            )
            fake_cargo.chmod(0o700)
            env = os.environ.copy()
            env.update(baseline)
            env.pop(missing, None)
            env["CARGO_BIN"] = str(fake_cargo)
            result = subprocess.run(
                ["bash", str(RUNNER_PATH), "all"],
                env=env,
                cwd=ROOT,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
            if result.returncode == 0:
                fail(f"missing {missing} unexpectedly allowed all profile")
            if marker.exists():
                fail(f"missing {missing} invoked cargo before failing preflight")


def main() -> int:
    workflow = WORKFLOW_PATH.read_text(encoding="utf-8")
    runner = RUNNER_PATH.read_text(encoding="utf-8")
    assert_contract(workflow, runner)
    mutation_checks(workflow, runner)
    preflight_runtime_checks()
    print("B-068 executor contract: PASS (full allow-list + negative mutations)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError) as exc:
        print(f"B-068 executor contract: FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1)
