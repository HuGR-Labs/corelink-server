#!/usr/bin/env python3
"""Static and adversarial guard for the B-074 money-path auth contract.

This guard deliberately does not build Rust or run the Worker.  It checks the
split-module source wiring and runs in-memory mutations that remove the
fail-closed branches or replace code with comment/string bait.
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def _read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        raise AssertionError(f"missing or non-regular source: {relative}")
    return path.read_text(encoding="utf-8")


def _without_comments(text: str) -> str:
    """Remove comments while retaining strings, so comments cannot satisfy gates."""
    text = re.sub(r"(?s)/\*.*?\*/", "", text)
    return re.sub(r"//[^\n]*", "", text)


def _require(text: str, pattern: str, label: str) -> None:
    if re.search(pattern, _without_comments(text), re.MULTILINE | re.DOTALL) is None:
        raise AssertionError(f"missing B-074 contract: {label}")


def _require_not(text: str, pattern: str, label: str) -> None:
    if re.search(pattern, _without_comments(text), re.MULTILINE | re.DOTALL) is not None:
        raise AssertionError(f"forbidden B-074 regression: {label}")


def _check_money_checklist(text: str) -> None:
    """Bind the canonical matrix rows to the money-path auth contract."""
    matches = re.findall(r"(?m)^\|\s*(233|234)\s*\|([^\n]*)$", text)
    if len(matches) != 2 or {row_id for row_id, _ in matches} != {"233", "234"}:
        raise AssertionError("B-074 checklist must contain exactly rows 233 and 234")
    rows = dict(matches)
    expected = {
        "233": "CORELINK_TIER_SELECT_AUTH_KEY",
        "234": "CORELINK_DPA_ACCEPT_AUTH_KEY",
    }
    for row_id, env_name in expected.items():
        row = rows[row_id]
        if f"`{env_name}`" not in row:
            raise AssertionError(f"B-074 checklist row {row_id} has the wrong environment name")
        if "**Only UNSET falls back**" not in row:
            raise AssertionError(
                f"B-074 checklist row {row_id} must state that only an absent dedicated binding falls back"
            )
        if "bound blank, whitespace-only, non-UTF8, or <32-character value fails CLOSED" not in row:
            raise AssertionError(
                f"B-074 checklist row {row_id} must document malformed dedicated values as fail-closed"
            )
        if re.search(r"UNSET/blank/<32 chars\s+falls back", row, re.IGNORECASE):
            raise AssertionError(
                f"B-074 checklist row {row_id} widens malformed dedicated values to the shared authority"
            )


def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> dict[str, int]:
    overrides = {} if overrides is None else overrides

    def read(relative: str) -> str:
        return overrides.get(relative, _read(root, relative))

    admin = read("crates/corelink-container/src/routes/admin/part-00.rs")
    dpa = read("crates/corelink-container/src/routes/dpa_accept.rs")
    tier = read("crates/corelink-container/src/routes/tier_select/part-00.rs")
    worker_auth = read("worker/src/index_auth.ts")
    worker_special = read("worker/src/index_special_routes.ts")
    do_start = read("worker/src/durable_object_start.ts")
    rust_test = read("crates/corelink-container/tests/money_path_auth_wiring.rs")
    worker_test = read("worker/tests/onboarding.test.ts")
    checklist = read("docs/internal/secrets-checklist.md")
    backlog = read("BACKLOG.md")

    # The dedicated branch must own invalid values.  The only shared fallback
    # is the branch selected by a genuinely absent dedicated variable.
    _require(admin, r"match\s+std::env::var_os\(specific_env\)", "Rust var_os presence check")
    _require(admin, r"Some\(raw\).*?raw\.into_string\(\)", "Rust non-Unicode rejection")
    _require(
        admin,
        r"if\s+key\.len\(\)\s*<\s*INTERNAL_AUTH_KEY_MIN_LEN\s*\|\|\s*key\.trim\(\)\.is_empty\(\)\s*\{.*?return\s+None;",
        "Rust invalid dedicated fail-closed branch",
    )
    _require(admin, r"None\s*=>\s*\{\s*\}", "Rust absent-dedicated fallback arm")
    _require(
        admin,
        r"let\s+shared\s*=\s*std::env::var_os\(\"CORELINK_INTERNAL_AUTH_KEY\"\)",
        "Rust shared fallback source",
    )
    _require_not(
        dpa,
        r"std::env::var\(\"CORELINK_INTERNAL_AUTH_KEY\"\)",
        "DPA raw shared read",
    )
    _require_not(
        tier,
        r"std::env::var\(\"CORELINK_INTERNAL_AUTH_KEY\"\)",
        "tier-select raw shared read",
    )

    for source, env_name, label in (
        (dpa, "CORELINK_DPA_ACCEPT_AUTH_KEY", "DPA"),
        (tier, "CORELINK_TIER_SELECT_AUTH_KEY", "tier-select"),
    ):
        _require(source, rf'const\s+\w+_AUTH_KEY_ENV:\s*&str\s*=\s*"{env_name}";', f"{label} dedicated name")
        _require(source, rf"resolve_internal_auth_key\(\w+_AUTH_KEY_ENV\)", f"{label} common resolver")
        _require_not(
            source,
            r'"[^"\n]*resolve_internal_auth_key\(\w+_AUTH_KEY_ENV\)[^"\n]*"',
            f"{label} resolver string bait",
        )

    _require(tier, r"let\s+auth_key\s*=\s*tier_select_auth_key_from_env\(\)\?;", "tier-select mount gate")
    _require(dpa, r"let\s+auth_key\s*=\s*dpa_accept_auth_key_from_env\(\)\?;", "DPA mount gate")

    _require(
        worker_auth,
        r"if\s*\(dedicatedKey\s*!==\s*undefined\)\s*\{.*?dedicatedKey\.length\s*>=\s*ONBOARDING_AUTH_KEY_MIN_LENGTH\s*&&\s*dedicatedKey\.trim\(\)\.length\s*>\s*0.*?:\s*null",
        "Worker invalid dedicated fail-closed branch",
    )
    _require_not(
        worker_auth,
        r"dedicatedKey\.length\s*>=\s*ONBOARDING_AUTH_KEY_MIN_LENGTH.*?\?\s*dedicatedKey\s*:\s*dedicatedKey",
        "Worker dedicated no-op fallback",
    )
    _require(
        worker_auth,
        r"const\s+shared\s*=\s*env\.CORELINK_INTERNAL_AUTH_KEY;.*?shared\.length\s*>=\s*ONBOARDING_AUTH_KEY_MIN_LENGTH\s*&&\s*shared\.trim\(\)\.length\s*>\s*0",
        "Worker absent-dedicated shared fallback",
    )
    _require(worker_special, r"resolveOnboardingAuthKey\(route\.pathSuffix,\s*env\)", "Worker route wiring")
    _require(do_start, r"CORELINK_TIER_SELECT_AUTH_KEY:\s*ctx\.env\.CORELINK_TIER_SELECT_AUTH_KEY", "DO tier key forwarding")
    _require(do_start, r"CORELINK_DPA_ACCEPT_AUTH_KEY:\s*ctx\.env\.CORELINK_DPA_ACCEPT_AUTH_KEY", "DO DPA key forwarding")
    _require(do_start, r"ctx\.env\.CORELINK_TIER_SELECT_AUTH_KEY\s*===\s*undefined", "DO preserves absent tier key")
    _require(do_start, r"ctx\.env\.CORELINK_DPA_ACCEPT_AUTH_KEY\s*===\s*undefined", "DO preserves absent DPA key")
    _require_not(
        do_start,
        r"CORELINK_(?:TIER_SELECT|DPA_ACCEPT)_AUTH_KEY:\s*ctx\.env\.CORELINK_(?:TIER_SELECT|DPA_ACCEPT)_AUTH_KEY\s*\?\?\s*\"\"",
        "DO turns absent money key into present empty key",
    )

    # Behavioral tests must pin controls and the two dangerous dedicated cases.
    _require(rust_test, r"valid shared key must mount", "Rust shared fallback control")
    _require(rust_test, r"dedicated key alone must mount", "Rust dedicated control")
    _require(rust_test, r"short dedicated key must fail closed", "Rust short dedicated behavior")
    _require(rust_test, r"whitespace-only dedicated key must fail closed", "Rust invalid dedicated behavior")
    _require(worker_test, r"fails CLOSED when a money dedicated key is sub-floor", "Worker short dedicated behavior")
    _require(worker_test, r"fails CLOSED when a money dedicated key is whitespace-only", "Worker invalid dedicated behavior")
    _check_money_checklist(checklist)

    b074 = re.search(r"### B-074 .*?(?=\n### B-075 )", backlog, re.DOTALL)
    if b074 is None or re.search(r"\nstatus:\s*done\s*\n", b074.group(0)) is None:
        raise AssertionError("B-074 must be done only after the executable proof runs")
    if re.search(r"verify_b074_money_path_auth\.py --self-test", backlog) is None:
        raise AssertionError("missing B-074 contract: B-074 verifier invocation")

    return {"rust_routes": 2, "worker_wiring": 3, "behavioral_tests": 2}


def self_test(root: Path = ROOT) -> None:
    """Reject implementation removal, comment/string bait, and no-op mutations."""
    admin_path = "crates/corelink-container/src/routes/admin/part-00.rs"
    admin = _read(root, admin_path)
    strict = "if key.len() < INTERNAL_AUTH_KEY_MIN_LEN || key.trim().is_empty() {"
    mutated = admin.replace(strict, "if false {", 1)
    try:
        verify(root, overrides={admin_path: mutated})
    except AssertionError:
        pass
    else:
        raise AssertionError("Rust invalid-key mutation survived")

    worker_path = "worker/src/index_auth.ts"
    worker = _read(root, worker_path)
    worker_mutated, replacements = re.subn(
        r"(dedicatedKey\.length\s*>=\s*ONBOARDING_AUTH_KEY_MIN_LENGTH\s*&&\s*"
        r"dedicatedKey\.trim\(\)\.length\s*>\s*0)\s*\?\s*dedicatedKey\s*:\s*null",
        r"\1 ? dedicatedKey : dedicatedKey",
        worker,
        count=1,
    )
    if replacements != 1:
        raise AssertionError("Worker mutation fixture did not match")
    try:
        verify(root, overrides={worker_path: worker_mutated})
    except AssertionError:
        pass
    else:
        raise AssertionError("Worker no-op fallback mutation survived")

    # A marker moved into a comment/string must not count as executable wiring.
    dpa_path = "crates/corelink-container/src/routes/dpa_accept.rs"
    dpa = _read(root, dpa_path)
    dpa_mutated = dpa.replace(
        "super::admin::resolve_internal_auth_key(DPA_ACCEPT_AUTH_KEY_ENV)",
        "/* super::admin::resolve_internal_auth_key(DPA_ACCEPT_AUTH_KEY_ENV) */ None",
        1,
    )
    try:
        verify(root, overrides={dpa_path: dpa_mutated})
    except AssertionError:
        pass
    else:
        raise AssertionError("comment/string bait mutation survived")

    dpa_string_mutated = dpa.replace(
        "super::admin::resolve_internal_auth_key(DPA_ACCEPT_AUTH_KEY_ENV)",
        '"super::admin::resolve_internal_auth_key(DPA_ACCEPT_AUTH_KEY_ENV)"; None',
        1,
    )
    try:
        verify(root, overrides={dpa_path: dpa_string_mutated})
    except AssertionError:
        pass
    else:
        raise AssertionError("resolver string bait mutation survived")

    checklist_path = "docs/internal/secrets-checklist.md"
    checklist = _read(root, checklist_path)
    for row_id in ("233", "234"):
        row_match = re.search(rf"(?m)^\|\s*{row_id}\s*\|[^\n]*$", checklist)
        if row_match is None or "**Only UNSET falls back**" not in row_match.group(0):
            raise AssertionError(f"B-074 checklist mutation fixture missing row {row_id} contract")
        mutated_row = row_match.group(0).replace(
            "**Only UNSET falls back**", "UNSET/blank/<32 chars falls back", 1
        )
        mutated_checklist = checklist[: row_match.start()] + mutated_row + checklist[row_match.end() :]
        try:
            verify(root, overrides={checklist_path: mutated_checklist})
        except AssertionError:
            pass
        else:
            raise AssertionError(f"checklist inverse fallback mutation survived for row {row_id}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    result = verify(args.root)
    if args.self_test:
        self_test(args.root)
        print(f"B-074 done: {result}; adversarial mutations red")
    else:
        print(f"B-074 done: {result}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
