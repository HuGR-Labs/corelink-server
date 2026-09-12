#!/usr/bin/env python3
"""Fail-closed, source-only verifier for the B-126 T3 split.

This is deliberately separate from the docs-reality CLI and from the Rust/TS
test modules being refactored.  It parses real declarations (after masking
comments and literals where needed), verifies the expected module path, and
checks the complete symbol population of every fragment.  A marker in a
comment or string cannot satisfy the gate.
"""

from __future__ import annotations

import argparse
import ast
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Mapping


LIMIT = 1_000


class SourceSyntaxError(ValueError):
    """A split source, declaration, or population is not trustworthy."""


@dataclass(frozen=True)
class SplitSpec:
    parent: str
    fragment: str
    language: str
    module: str
    fragment_symbols: frozenset[str]
    parent_symbols: frozenset[str] = frozenset()
    parent_imports: frozenset[str] = frozenset()


def _fs(*items: str) -> frozenset[str]:
    return frozenset(items)


T3_SPECS = (
    SplitSpec(
        "apps/admin-ui/src/lib/e2e-mock-fixtures.ts",
        "apps/admin-ui/src/lib/e2e-mock-fixture-state.ts",
        "ts",
        "e2e-mock-fixture-state",
        _fs(
            "interface:MockState",
            "function:makeTenants",
            "function:makeAuditEvents",
            "function:makeAuditDetail",
            "function:makeOps",
            "function:makeCustomerAudit",
            "function:makeCustomerPats",
            "function:makeCustomerTeam",
            "function:makeCustomerBilling",
            "function:freshState",
        ),
        parent_symbols=_fs(
            "import:freshState",
            "import:makeAuditDetail",
            "import:type MockState",
        ),
    ),
    SplitSpec(
        "tests/e2e-user-journeys/src/journeys/adapters.rs",
        "tests/e2e-user-journeys/src/journeys/adapters_auth.rs",
        "rust",
        "adapters_auth",
        _fs("fn:oci_basic_header", "fn:base64_standard"),
        parent_imports=_fs("use:adapters_auth::oci_basic_header"),
    ),
    SplitSpec(
        "tests/e2e-tenant-isolation/tests/adversarial.rs",
        "tests/e2e-tenant-isolation/tests/adversarial_tail.rs",
        "rust-tests",
        "adversarial_tail",
        _fs(
            "test:s18_audit_chain_leaf_forge_rejected",
            "test:s19_cross_region_replay_residency_enforced",
            "test:s20_dsr_cross_tenant_submission_rejected",
            "test:s21_quota_inheritance_siblings_isolated",
            "test:s22_multipart_upload_cross_tenant_forge_rejected",
            "test:s23_stripe_webhook_cross_account_spoof_rejected",
            "test:s24_kv_partition_pat_revoke_fail_closed",
            "test:s25_audit_query_injection_rejected",
        ),
        parent_symbols=_fs(
            "test:s01_cas_read_other_tenant_denied_and_audited",
            "test:s02_cas_write_to_other_tenant_prefix_denied",
            "test:s03_list_enumeration_does_not_leak_other_tenant",
            "test:s04_byok_dek_wrap_with_wrong_aad_rejected",
            "test:s05_byok_envelope_tamper_rejected_with_audit",
            "test:s06_audit_cross_tenant_query_requires_dual_approval",
            "test:s07_d1_row_spoofing_rejected_jwt_wins",
            "test:s08_idempotency_key_collision_is_independent_per_tenant",
            "test:s09_quota_crosstalk_isolated",
            "test:s10_rate_limit_crosstalk_isolated",
            "test:s11_stripe_webhook_replay_cross_tenant_rejected",
            "test:s12_pat_cross_tenant_use_rejected",
            "test:s13_timing_oracle_constant_time_auth_probe",
            "test:s14_cas_cache_poisoning_cross_tenant_isolated",
            "test:s15_cmk_rotation_race_no_half_state",
            "test:s16_pat_revoke_toctou_no_window",
            "test:s17_idempotency_collision_mixed_case_cross_tenant",
        ),
    ),
    SplitSpec(
        "tests/e2e-pilot-onboarding/src/harness.rs",
        "tests/e2e-pilot-onboarding/src/harness_lifecycle.rs",
        "rust",
        "harness_lifecycle",
        _fs(
            "method:request_dsr_erasure",
            "method:finalise_dsr_erasure",
            "method:cancel_subscription",
            "method:complete_offboarding",
        ),
    ),
    SplitSpec(
        "scripts/validate_docs_reality.py",
        "scripts/validate_docs_reality_core.py",
        "python",
        "validate_docs_reality_core",
        _fs(
            "assign:REPO_ROOT",
            "assign:CLI_MAIN",
            "assign:OKF_ROOT",
            "assign:DEFAULT_ALLOWLIST",
            "assign:DOC_ROOTS",
            "assign:DOC_ROOTS_WITH_EXCLUDES",
            "assign:ROOT_DOC_GLOBS",
            "assign:ROOT_DOC_EXCLUDE",
            "assign:ROUTE_SOURCE_ROOTS",
            "assign:DOC_EXTS",
            "assign:WALK_EXCLUDE_DIRS",
            "assign:SHELL_FENCE_LANGS",
            "class:SourceSyntaxError",
            "function:_raw_string_start",
            "function:_blank_comment",
            "function:_strip_source_comments",
            "function:_camel_to_kebab",
            "assign:_ENUM_RE",
            "assign:_VARIANT_RE",
            "assign:_ACTION_FIELD_RE",
            "class:CliModel",
            "function:parse_cli_model",
            "class:DocFile",
            "function:_iter_doc_files",
            "assign:_FENCE_RE",
            "assign:_INLINE_CODE_RE",
            "assign:_HTML_CODE_RE",
            "assign:_PROMPT_RE",
            "assign:_CORELINK_CMD_RE",
            "class:CliRef",
            "function:_parse_corelink_line",
            "function:_strip_html",
            "function:extract_cli_refs",
            "assign:_ROUTE_DECL_RE",
            "assign:_WORKER_PATH_RE",
            "class:RouteRegistration",
            "class:RouteInventory",
            "function:_route_methods",
            "function:collect_route_inventory",
            "function:collect_routes",
            "function:_route_to_regex",
            "function:_is_generic_catchall",
            "function:endpoint_resolves",
            "function:_endpoint_method",
            "assign:_DOC_PATH_RE",
            "function:extract_doc_endpoints",
            "function:_normalise_endpoint",
        ),
        parent_imports=_fs(
            "from*:validate_docs_reality_core",
            "from:_endpoint_method",
            "from:_is_generic_catchall",
            "from:_iter_doc_files",
            "from:_normalise_endpoint",
            "from:_route_to_regex",
            "from:_strip_source_comments",
        ),
    ),
)


def _blank(value: str) -> str:
    return "".join("\n" if c == "\n" else " " for c in value)


def mask_rust(source: str) -> str:
    """Mask Rust comments and literals while retaining token positions."""
    out: list[str] = []
    i = 0
    n = len(source)
    while i < n:
        if source.startswith("//", i):
            end = source.find("\n", i + 2)
            end = n if end < 0 else end
            out.append(_blank(source[i:end]))
            i = end
            continue
        if source.startswith("/*", i):
            depth = 1
            end = i + 2
            while end < n and depth:
                if source.startswith("/*", end):
                    depth += 1
                    end += 2
                elif source.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            if depth:
                raise SourceSyntaxError("unterminated Rust block comment")
            out.append(_blank(source[i:end]))
            i = end
            continue
        raw = i
        if source.startswith("br", i):
            raw = i + 2
        elif source.startswith("r", i):
            raw = i + 1
        if raw != i:
            hashes = 0
            while raw + hashes < n and source[raw + hashes] == "#":
                hashes += 1
            if raw + hashes < n and source[raw + hashes] == '"':
                close = '"' + ("#" * hashes)
                end = source.find(close, raw + hashes + 1)
                if end < 0:
                    raise SourceSyntaxError("unterminated Rust raw string")
                end += len(close)
                out.append(_blank(source[i:end]))
                i = end
                continue
        # Apostrophes in lifetimes (`'static`, `'a`) are code, not literals.
        # Only mask the short, unambiguously closed character-literal form.
        if source[i] == "'" and not (i + 2 < n and source[i + 2] == "'"):
            out.append(source[i])
            i += 1
            continue
        if source[i] in {'"', "'"}:
            quote = source[i]
            end = i + 1
            while end < n:
                if source[end] == "\\":
                    end += 2
                elif source[end] == quote:
                    end += 1
                    break
                else:
                    end += 1
            if end > n or (end == n and source[n - 1] != quote):
                raise SourceSyntaxError("unterminated Rust literal")
            out.append(_blank(source[i:end]))
            i = end
            continue
        out.append(source[i])
        i += 1
    return "".join(out)


def mask_ts_code(source: str) -> str:
    """Mask TypeScript comments and literals while retaining code tokens."""
    out: list[str] = []
    i = 0
    n = len(source)
    while i < n:
        if source[i] in {'"', "'", "`"}:
            quote = source[i]
            end = i + 1
            while end < n:
                if source[end] == "\\":
                    end += 2
                elif source[end] == quote:
                    end += 1
                    break
                else:
                    end += 1
            if end > n or (end == n and source[n - 1] != quote):
                raise SourceSyntaxError("unterminated TypeScript literal")
            out.append(_blank(source[i:end]))
            i = end
            continue
        if source.startswith("//", i):
            end = source.find("\n", i + 2)
            end = n if end < 0 else end
            out.append(_blank(source[i:end]))
            i = end
        elif source.startswith("/*", i):
            end = source.find("*/", i + 2)
            if end < 0:
                raise SourceSyntaxError("unterminated TypeScript block comment")
            end += 2
            out.append(_blank(source[i:end]))
            i = end
        else:
            out.append(source[i])
            i += 1
    return "".join(out)


def _read(root: Path, relative: str, overrides: Mapping[str, str | None]) -> str:
    if relative in overrides:
        value = overrides[relative]
        if value is None:
            raise SourceSyntaxError(f"{relative}: source missing")
        return value
    path = root / relative
    if path.is_symlink() or not path.is_file():
        raise SourceSyntaxError(f"{relative}: source missing or not a regular file")
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise SourceSyntaxError(f"{relative}: source unreadable") from exc


def _rust_module_declarations(source: str) -> list[str]:
    code = mask_rust(source)
    return re.findall(r"\bmod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;", code)


def _rust_functions(source: str) -> list[str]:
    code = mask_rust(source)
    return re.findall(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(", code)


def _rust_test_functions(source: str) -> set[str]:
    return {name for name in _rust_functions(source) if re.match(r"s\d{2}_", name)}


def _rust_method_functions(source: str) -> set[str]:
    code = mask_rust(source)
    return set(re.findall(r"\bpub\s+fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(", code))


def _ts_fragment_symbols(source: str) -> set[str]:
    code = mask_ts_code(source)
    symbols = {f"interface:{x}" for x in re.findall(r"^\s*export\s+interface\s+(\w+)", code, re.M)}
    symbols |= {f"function:{x}" for x in re.findall(r"^\s*(?:export\s+)?function\s+(\w+)\s*\(", code, re.M)}
    return symbols


def _ts_imports(source: str) -> set[str]:
    code = mask_ts_code(source)
    found: set[str] = set()
    code_pattern = re.compile(r"\bimport\s*\{[^}]*\}\s*from\s*")
    raw_pattern = re.compile(
        r"\bimport\s*\{(?P<body>[^}]*)\}\s*from\s*[\"'](?P<path>[^\"']+)[\"']"
    )
    for candidate in code_pattern.finditer(code):
        match = raw_pattern.match(source, candidate.start())
        if match is None or match.group("path") != "./e2e-mock-fixture-state":
            continue
        for item in match.group("body").split(","):
            item = item.strip()
            if item:
                found.add(f"import:{item}")
    return found


def _python_population(source: str) -> set[str]:
    try:
        tree = ast.parse(source)
    except SyntaxError as exc:
        raise SourceSyntaxError(f"Python parse failed: {exc}") from exc
    result: set[str] = set()
    for node in tree.body:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            result.add(f"function:{node.name}")
        elif isinstance(node, ast.ClassDef):
            result.add(f"class:{node.name}")
        elif isinstance(node, (ast.Assign, ast.AnnAssign, ast.AugAssign)):
            targets = node.targets if isinstance(node, ast.Assign) else [node.target]
            for target in targets:
                if isinstance(target, ast.Name):
                    result.add(f"assign:{target.id}")
    return result


def _python_imports(source: str) -> set[str]:
    try:
        tree = ast.parse(source)
    except SyntaxError as exc:
        raise SourceSyntaxError(f"Python parse failed: {exc}") from exc
    found: set[str] = set()
    for node in tree.body:
        if isinstance(node, ast.ImportFrom) and node.module == "validate_docs_reality_core":
            for alias in node.names:
                found.add(
                    f"from*:{node.module}" if alias.name == "*" else f"from:{alias.name}"
                )
    return found


def _check_spec(spec: SplitSpec, parent: str, fragment: str) -> list[str]:
    errors: list[str] = []
    for label, text in (("parent", parent), ("fragment", fragment)):
        if len(text.splitlines()) > LIMIT:
            errors.append(f"{spec.parent if label == 'parent' else spec.fragment}: exceeds {LIMIT} lines")

    if spec.language == "ts":
        imports = _ts_imports(parent)
        if imports != spec.parent_symbols:
            errors.append(f"{spec.parent}: import population mismatch: {sorted(imports)}")
        symbols = _ts_fragment_symbols(fragment)
        if symbols != spec.fragment_symbols:
            errors.append(f"{spec.fragment}: symbol population mismatch: {sorted(symbols)}")
        if "import:MockState" in imports:
            errors.append(f"{spec.parent}: type import lost its explicit type qualifier")
        return errors

    if spec.language.startswith("rust"):
        modules = _rust_module_declarations(parent)
        if modules.count(spec.module) != 1:
            errors.append(f"{spec.parent}: real module declaration for {spec.module!r} is not unique")
        if spec.parent == "tests/e2e-user-journeys/src/journeys/adapters.rs":
            code = mask_rust(parent)
            if len(re.findall(r"\buse\s+adapters_auth\s*::\s*oci_basic_header\s*;", code)) != 1:
                errors.append(f"{spec.parent}: OCI auth helper import is not unique")
        else:
            wiring_count = len(re.findall(r"(?m)^\s*use\s+super\s*::\s*\*\s*;\s*$", mask_rust(fragment)))
            if wiring_count != 1:
                errors.append(
                    f"{spec.fragment}: expected exactly one real use-super wiring, found {wiring_count}"
                )
        if spec.language == "rust-tests":
            actual = {f"test:{name}" for name in _rust_test_functions(fragment)}
            parent_actual = {f"test:{name}" for name in _rust_test_functions(parent)}
            if actual != spec.fragment_symbols:
                errors.append(f"{spec.fragment}: test population mismatch: {sorted(actual)}")
            if parent_actual != spec.parent_symbols:
                errors.append(f"{spec.parent}: parent test population mismatch: {sorted(parent_actual)}")
            # The exact 25-name closure is checked below without relying on a
            # marker or on the number of functions alone.
            expected = spec.fragment_symbols | spec.parent_symbols
            if actual & parent_actual or (actual | parent_actual) != expected:
                errors.append(f"{spec.parent}: adversarial test population is not closed")
        elif spec.parent == "tests/e2e-pilot-onboarding/src/harness.rs":
            actual = {f"method:{name}" for name in _rust_method_functions(fragment)}
            if actual != spec.fragment_symbols:
                errors.append(f"{spec.fragment}: lifecycle method population mismatch: {sorted(actual)}")
            parent_methods = {f"method:{name}" for name in _rust_method_functions(parent)}
            if parent_methods & spec.fragment_symbols:
                errors.append(f"{spec.parent}: lifecycle methods duplicated in parent")
        else:
            actual = {f"fn:{name}" for name in _rust_functions(fragment)}
            if actual != spec.fragment_symbols:
                errors.append(f"{spec.fragment}: helper population mismatch: {sorted(actual)}")
        return errors

    if spec.language == "python":
        imports = _python_imports(parent)
        if imports != spec.parent_imports:
            errors.append(f"{spec.parent}: import population mismatch: {sorted(imports)}")
        symbols = _python_population(fragment)
        if symbols != spec.fragment_symbols:
            errors.append(f"{spec.fragment}: definition population mismatch: {sorted(symbols)}")
        parent_defs = _python_population(parent)
        duplicated = spec.fragment_symbols - {"function:collect_route_inventory"}
        if any(item in parent_defs for item in duplicated):
            errors.append(f"{spec.parent}: core definition duplicated in parent")
        return errors

    raise AssertionError(f"unknown verifier language: {spec.language}")


def verify(root: Path, overrides: Mapping[str, str | None] | None = None) -> list[str]:
    overrides = overrides or {}
    errors: list[str] = []
    for spec in T3_SPECS:
        try:
            parent = _read(root, spec.parent, overrides)
        except SourceSyntaxError as exc:
            errors.append(str(exc))
            continue
        try:
            fragment = _read(root, spec.fragment, overrides)
        except SourceSyntaxError as exc:
            errors.append(str(exc))
            continue
        try:
            errors.extend(_check_spec(spec, parent, fragment))
        except SourceSyntaxError as exc:
            errors.append(f"{spec.parent}: {exc}")
    return errors


def self_test(root: Path) -> None:
    assert verify(root) == []
    for spec in T3_SPECS:
        parent = _read(root, spec.parent, {})
        fragment = _read(root, spec.fragment, {})
        if spec.language == "python":
            with_marker_removed = parent.replace(
                "from validate_docs_reality_core import *",
                "from removed_split_module import *",
                1,
            )
        else:
            with_marker_removed = parent.replace(
                spec.module if spec.language == "ts" else f"mod {spec.module}",
                "removed_split_marker",
                1,
            )
        assert verify(root, {spec.parent: with_marker_removed}), spec.parent
        # A comment carrying the old marker is not a declaration/import.
        if spec.language == "ts":
            fake = parent.replace(
                'import { freshState, makeAuditDetail, type MockState } from "./e2e-mock-fixture-state";',
                '// import { freshState, makeAuditDetail, type MockState } from "./e2e-mock-fixture-state";',
                1,
            )
        elif spec.language == "python":
            fake = parent.replace(
                "from validate_docs_reality_core import *",
                "# from validate_docs_reality_core import *",
                1,
            )
        else:
            fake = parent.replace(f"mod {spec.module};", f"// mod {spec.module};", 1)
        assert verify(root, {spec.parent: fake}), spec.parent
        if spec.language == "ts":
            string_fake = parent.replace(
                'import { freshState, makeAuditDetail, type MockState } from "./e2e-mock-fixture-state";',
                'const bait = "import { freshState, makeAuditDetail, type MockState } from \'./e2e-mock-fixture-state\'";',
                1,
            )
        elif spec.language == "python":
            string_fake = parent.replace(
                "from validate_docs_reality_core import *",
                'BAIT = "from validate_docs_reality_core import *"',
                1,
            )
        else:
            string_fake = parent.replace(
                f"mod {spec.module};",
                f'const _BAIT: &str = "mod {spec.module};";',
                1,
            )
        assert verify(root, {spec.parent: string_fake}), spec.parent
        assert verify(root, {spec.fragment: None}), spec.fragment
        mutation_kind = {
            "ts": "function:makeAuditDetail",
            "python": "function:parse_cli_model",
            "rust-tests": "test:s18_audit_chain_leaf_forge_rejected",
        }.get(spec.language, "fn:oci_basic_header" if spec.parent.endswith("adapters.rs") else "method:request_dsr_erasure")
        missing_symbol = mutation_kind
        name = missing_symbol.split(":", 1)[1]
        if spec.language == "ts":
            mutated_fragment = re.sub(
                rf"^(\s*(?:export\s+)?(?:interface|function)\s+){re.escape(name)}\b",
                r"\1removed_symbol",
                fragment,
                count=1,
                flags=re.M,
            )
        elif spec.language == "python":
            mutated_fragment = re.sub(
                rf"^(\s*(?:async\s+)?def\s+){re.escape(name)}\b",
                r"\1removed_symbol",
                fragment,
                count=1,
                flags=re.M,
            )
        elif spec.language == "rust" and spec.parent.endswith("harness.rs"):
            mutated_fragment = re.sub(
                rf"(\bpub\s+fn\s+){re.escape(name)}\b",
                r"\1removed_symbol",
                fragment,
                count=1,
            )
        else:
            mutated_fragment = re.sub(
                rf"(\bfn\s+){re.escape(name)}\b",
                r"\1removed_symbol",
                fragment,
                count=1,
            )
        assert verify(root, {spec.fragment: mutated_fragment}), spec.fragment
    first = T3_SPECS[0]
    parent = _read(root, first.parent, {})
    oversized = parent + "\n" * (LIMIT + 1 - len(parent.splitlines()))
    assert verify(root, {first.parent: oversized}), first.parent
    print("PASS: B126-T3 semantic verifier and all wiring/population mutations")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.self_test:
            self_test(args.root)
            return 0
        errors = verify(args.root)
    except (OSError, UnicodeError, SourceSyntaxError) as exc:
        print(f"FAIL: B126-T3 verifier indeterminate: {exc}", file=sys.stderr)
        return 2
    if errors:
        for error in errors:
            print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print("PASS: B126-T3 source units, real wiring, and closed populations")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
