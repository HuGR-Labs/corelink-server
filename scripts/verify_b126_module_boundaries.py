#!/usr/bin/env python3
"""Cheap B-126-M1 guard for bounded Rust route/storage modules.

The production files are intentionally thin include roots.  This guard checks
that every declared fragment is present, bounded, and still contains the
load-bearing public symbols from the original module.  Direct include order is
closed by a manifest, while the transitive ``part-*.rs`` census is checked
against disk.  It has no Cargo, CI, or network dependency; its mutation check
exercises removal, duplication, reordering, and include-shaped bait.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MAX_LINES = 1000

TARGETS: dict[str, tuple[str, ...]] = {
    "crates/corelink-container/src/routes/internal_pat.rs": (
        r"pub struct InternalPatRouteState",
        r"pub fn router\(",
        r"async fn handle_mint\(",
    ),
    "crates/corelink-container/src/routes/customer.rs": (
        r"pub struct CustomerRouteState",
        r"pub fn router\(",
        r"deterministic_dsr_id",
    ),
    "crates/corelink-container/src/routes/dsr/portal.rs": (
        r"pub struct PrivacyDsrRouteState",
        r"pub trait DsrTicketStore",
        r"pub fn router\(",
    ),
    "crates/corelink-container/src/routes/tier_select.rs": (
        r"pub struct TierSelectRouteState",
        r"pub async fn handle\(",
        r"pub fn router\(",
    ),
    "crates/corelink-container/src/routes/ac.rs": (
        r"pub struct AcRouteState",
        r"pub fn build_handlers\(",
        r"pub fn router\(",
    ),
    "crates/corelink-container/src/routes/auth_introspect.rs": (
        r"pub struct AuthIntrospectRouteState",
        r"pub async fn resolve_tenant_for_org\(",
        r"pub fn router\(",
    ),
    "crates/corelink-container/src/routes/admin.rs": (
        r"pub struct AdminRouteState",
        r"pub struct D1ApprovalLedger",
        r"pub fn router\(",
    ),
    "crates/corelink-container/src/routes/public_pullthrough.rs": (
        r"pub\(crate\) struct UpstreamManifestResolver",
        r"impl ManifestResolver for UpstreamManifestResolver",
        r"MAX_PULLTHROUGH_BLOB_BYTES",
    ),
    "crates/corelink-container/src/routes/cargo.rs": (
        r"pub fn router\(",
        r"struct CargoGateState",
        r"async fn cargo_gate\(",
    ),
    "crates/corelink-container/src/routes/admin_pilot.rs": (
        r"pub struct PilotAdminRouteState",
        r"pub trait PilotStore",
        r"pub fn router\(",
    ),
    "crates/corelink-container/src/routes/bazel_v2.rs": (
        r"pub struct BazelRouteState",
        r"pub fn build_handlers\(",
        r"pub fn router\(",
    ),
    "crates/corelink-container/src/storage/byok_cas.rs": (
        r"pub struct ByokConfigCache",
        r"pub fn engagement_for\(",
        r"pub fn encrypt_cas_blob\(",
    ),
}

# The direct roots are deliberately explicit.  Some modules use a thin
# ``part-00.rs`` aggregate which includes ``part-00-00.rs`` and friends; a
# directory glob cannot distinguish those nested children from direct roots.
# Keep this manifest in the same source order as the production wrappers so a
# swapped include cannot silently change the assembled module.
DIRECT_INCLUDES: dict[str, tuple[str, ...]] = {
    "crates/corelink-container/src/routes/internal_pat.rs": (
        "internal_pat/part-00.rs",
        "internal_pat/part-01.rs",
        "internal_pat/part-02.rs",
    ),
    "crates/corelink-container/src/routes/customer.rs": (
        "customer/part-00.rs",
        "customer/part-00-01.rs",
        "customer/part-01.rs",
        "customer/part-01-01.rs",
        "customer/part-02.rs",
    ),
    "crates/corelink-container/src/routes/dsr/portal.rs": (
        "portal/part-00.rs",
        "portal/part-00-01.rs",
        "portal/part-01.rs",
        "portal/part-01-01.rs",
        "portal/part-02.rs",
    ),
    "crates/corelink-container/src/routes/tier_select.rs": (
        "tier_select/part-00.rs",
        "tier_select/part-01.rs",
        "tier_select/part-02.rs",
    ),
    "crates/corelink-container/src/routes/ac.rs": (
        "ac/part-00.rs",
        "ac/part-00-01.rs",
        "ac/part-01.rs",
    ),
    "crates/corelink-container/src/routes/auth_introspect.rs": (
        "auth_introspect/part-00.rs",
        "auth_introspect/part-00-01.rs",
        "auth_introspect/part-01.rs",
    ),
    "crates/corelink-container/src/routes/admin.rs": (
        "admin/part-00.rs",
        "admin/part-01.rs",
        "admin/part-02.rs",
    ),
    "crates/corelink-container/src/routes/public_pullthrough.rs": (
        "public_pullthrough/part-00.rs",
        "public_pullthrough/part-00-01.rs",
        "public_pullthrough/part-01.rs",
    ),
    "crates/corelink-container/src/routes/cargo.rs": (
        "cargo/part-00.rs",
        "cargo/part-00-01.rs",
    ),
    "crates/corelink-container/src/routes/admin_pilot.rs": (
        "admin_pilot/part-00.rs",
        "admin_pilot/part-01.rs",
        "admin_pilot/part-02.rs",
        "admin_pilot/part-03.rs",
    ),
    "crates/corelink-container/src/routes/bazel_v2.rs": (
        "bazel_v2/part-00.rs",
        "bazel_v2/part-00-01.rs",
        "bazel_v2/part-01.rs",
    ),
    "crates/corelink-container/src/storage/byok_cas.rs": (
        "byok_cas/part-00.rs",
        "byok_cas/part-01.rs",
        "byok_cas/part-02.rs",
    ),
}

# Visibility is part of each module's API contract.  In particular,
# ``public_pullthrough`` is intentionally crate-visible; accepting either
# spelling would let a refactor silently widen or narrow the surface.
EXPECTED_REEXPORTS: dict[str, str] = {
    "crates/corelink-container/src/routes/internal_pat.rs": "pub use implementation::*;",
    "crates/corelink-container/src/routes/customer.rs": "pub use implementation::*;",
    "crates/corelink-container/src/routes/dsr/portal.rs": "pub use implementation::*;",
    "crates/corelink-container/src/routes/tier_select.rs": "pub use implementation::*;",
    "crates/corelink-container/src/routes/ac.rs": "pub use implementation::*;",
    "crates/corelink-container/src/routes/auth_introspect.rs": "pub use implementation::*;",
    "crates/corelink-container/src/routes/admin.rs": "pub use implementation::*;",
    "crates/corelink-container/src/routes/public_pullthrough.rs": "pub(crate) use implementation::*;",
    "crates/corelink-container/src/routes/cargo.rs": "pub use implementation::*;",
    "crates/corelink-container/src/routes/admin_pilot.rs": "pub use implementation::*;",
    "crates/corelink-container/src/routes/bazel_v2.rs": "pub use implementation::*;",
    "crates/corelink-container/src/storage/byok_cas.rs": "pub use implementation::*;",
}


class BoundaryError(ValueError):
    pass


INCLUDE_RE = re.compile(r'^\s*include!\("([^"\n]+)"\);\s*$', re.MULTILINE)
INCLUDE_TOKEN_RE = re.compile(r'\binclude!\s*\(')
REEXPORT_RE = re.compile(
    r'^[ \t]*(pub(?:\(crate\))?[ \t]+use[ \t]+implementation::\*[ \t]*;)[ \t]*$',
    re.MULTILINE,
)
INNER_DOC_RE = re.compile(r'^\s*//!', re.MULTILINE)
TRAILING_DOC_RE = re.compile(r'(?:^|\n)(?:\s*///[^\n]*(?:\n|$))+\s*\Z')
TRAILING_ATTRIBUTE_RE = re.compile(r'(?s)(?:^|\n)\s*#\[[^\]]*\]\s*\)?\s*\Z')
BARE_TEST_MODULE_RE = re.compile(r'^\s*mod\s+tests\s*;', re.MULTILINE)
TEST_MODULE_RE = re.compile(r'^\s*mod\s+tests\s*\{', re.MULTILINE)
PATH_MODULE_RE = re.compile(
    r'^\s*#\[path\s*=\s*"([^"]+)"\]\s*$\n\s*mod\s+([A-Za-z_]\w*)\s*;',
    re.MULTILINE,
)


def _read(path: Path) -> str:
    if path.is_symlink() or not path.is_file():
        raise BoundaryError(f"missing or non-regular module file: {path}")
    return path.read_text(encoding="utf-8")


def _validate_fragment_source(path: Path, text: str) -> None:
    """Reject source that is invalid when included in ``mod implementation``."""
    if INNER_DOC_RE.search(text):
        raise BoundaryError(f"{path}: inner module documentation (//! ) in included fragment")
    if TRAILING_DOC_RE.search(text):
        raise BoundaryError(f"{path}: doc-comment crosses an include fragment boundary")
    if TRAILING_ATTRIBUTE_RE.search(text):
        raise BoundaryError(f"{path}: attribute crosses an include fragment boundary")
    for match in BARE_TEST_MODULE_RE.finditer(text):
        prefix = text[: match.start()]
        path_match = list(re.finditer(r'^\s*#\[path\s*=\s*"([^"]+)"\]\s*$', prefix, re.MULTILINE))
        if not path_match or path_match[-1].group(1) != "tests.rs":
            raise BoundaryError(f"{path}: bare mod tests; has no local #[path = \"tests.rs\"] wiring")
        relative = Path(path_match[-1].group(1))
        target = path.parent / relative
        if relative.is_absolute() or ".." in relative.parts or target.parent != path.parent:
            raise BoundaryError(f"{path}: test module path escapes its fragment directory")
        if target.is_symlink() or not target.is_file():
            raise BoundaryError(f"{path}: test module path is missing or non-regular: {relative}")


def _validate_customer_test_tree(root: Path, override: str | None = None) -> None:
    """Validate the external customer test module tree used by part-02."""
    tests = root / "crates/corelink-container/src/routes/customer/tests.rs"
    text = _read(tests) if override is None else override
    declared: list[Path] = []
    for relative, _name in PATH_MODULE_RE.findall(text):
        relative_path = Path(relative)
        target = tests.parent / relative_path
        if relative_path.is_absolute() or ".." in relative_path.parts or target.parent != tests.parent:
            raise BoundaryError(f"{tests}: nested test module path escapes its directory: {relative}")
        if target.is_symlink() or not target.is_file():
            raise BoundaryError(f"{tests}: nested test module is missing or non-regular: {relative}")
        declared.append(target)
    expected = sorted(tests.parent.glob("tests_*.rs"))
    if sorted(declared) != expected:
        raise BoundaryError(f"{tests}: nested test modules do not equal tests_*.rs inventory")


def _expand_file(
    path: Path,
    *,
    overrides: dict[Path, str],
    active: tuple[Path, ...] = (),
) -> tuple[str, list[Path]]:
    """Expand every literal include and return text plus all included files."""
    if path in active:
        raise BoundaryError(f"include cycle: {' -> '.join(str(item) for item in (*active, path))}")
    text = overrides[path] if path in overrides else _read(path)
    _validate_fragment_source(path, text)
    token_count = len(INCLUDE_TOKEN_RE.findall(text))
    includes = list(INCLUDE_RE.finditer(text))
    if token_count != len(includes):
        raise BoundaryError(f"{path}: malformed include! directive")
    rendered: list[str] = []
    included_paths: list[Path] = []
    cursor = 0
    for match in includes:
        relative = match.group(1)
        relative_path = Path(relative)
        if relative_path.is_absolute() or ".." in relative_path.parts:
            raise BoundaryError(f"{path}: include escapes its source directory: {relative}")
        child = path.parent / relative_path
        if child.parent != path.parent:
            raise BoundaryError(f"{path}: include must stay in its source directory: {relative}")
        if relative_path.name.startswith("tests-") and TEST_MODULE_RE.search(text[: match.start()]) is None:
            raise BoundaryError(f"{path}: test fragment included outside mod tests {{: {relative}")
        child_text, descendants = _expand_file(child, overrides=overrides, active=(*active, path))
        rendered.extend((text[cursor : match.start()], child_text))
        included_paths.extend((child, *descendants))
        cursor = match.end()
    rendered.append(text[cursor:])
    return "".join(rendered), included_paths


def _assemble(
    root: Path,
    wrapper: Path,
    override: str | None = None,
    fragment_override: tuple[Path, str] | None = None,
) -> tuple[str, list[Path], list[Path]]:
    overrides = {} if fragment_override is None else {fragment_override[0]: fragment_override[1]}
    text = _read(wrapper) if override is None else override
    includes = list(INCLUDE_RE.finditer(text))
    if len(INCLUDE_TOKEN_RE.findall(text)) != len(includes):
        raise BoundaryError(f"{wrapper}: malformed include! directive")
    if len(re.findall(r'^\s*mod\s+implementation\s*\{', text, re.MULTILINE)) != 1:
        raise BoundaryError(f"{wrapper}: expected exactly one implementation module")
    relative_wrapper = wrapper.relative_to(root).as_posix()
    try:
        expected_reexport = EXPECTED_REEXPORTS[relative_wrapper]
    except KeyError as exc:
        raise BoundaryError(f"{wrapper}: re-export manifest missing") from exc
    reexports = REEXPORT_RE.findall(text)
    if len(reexports) != 1 or reexports[0] != expected_reexport:
        raise BoundaryError(f"{wrapper}: implementation re-export differs from manifest")
    if not includes:
        raise BoundaryError(f"{wrapper}: no bounded include fragments")
    expected_dir = wrapper.with_suffix("")
    try:
        expected_direct = DIRECT_INCLUDES[relative_wrapper]
    except KeyError as exc:
        raise BoundaryError(f"{wrapper}: direct include manifest missing") from exc
    declared: list[Path] = []
    pieces: list[str] = []
    all_included: list[Path] = []
    for match in includes:
        relative = match.group(1)
        if not relative.startswith(expected_dir.name + "/") or Path(relative).is_absolute() or ".." in Path(relative).parts:
            raise BoundaryError(f"{wrapper}: include escapes its fragment directory: {relative}")
        path = wrapper.parent / relative
        if path.parent != expected_dir or path.suffix != ".rs":
            raise BoundaryError(f"{wrapper}: include is not a direct fragment: {relative}")
        if path in declared:
            raise BoundaryError(f"{wrapper}: duplicate fragment include: {relative}")
        declared.append(path)
        expanded, nested = _expand_file(path, overrides=overrides, active=(wrapper,))
        pieces.append(expanded)
        all_included.extend((path, *nested))
    actual_direct = tuple(path.relative_to(wrapper.parent).as_posix() for path in declared)
    if actual_direct != expected_direct:
        raise BoundaryError(f"{wrapper}: direct include order/population differs from manifest")
    expected_parts = sorted(expected_dir.glob("part-*.rs"))
    actual_parts = sorted(
        path for path in all_included if path.parent == expected_dir and path.name.startswith("part-")
    )
    if actual_parts != expected_parts:
        raise BoundaryError(f"{wrapper}: transitive part-*.rs inventory differs from disk")
    if len(all_included) != len(set(all_included)):
        raise BoundaryError(f"{wrapper}: fragment is included more than once")
    expected_tests = sorted(expected_dir.glob("tests-*.rs"))
    declared_tests = sorted(path for path in all_included if path.name.startswith("tests-"))
    if declared_tests != expected_tests:
        raise BoundaryError(f"{wrapper}: test fragments do not equal tests-*.rs inventory")
    return "\n".join(pieces), declared, all_included


def verify(
    root: Path = ROOT,
    *,
    wrapper_override: tuple[Path, str] | None = None,
    fragment_override: tuple[Path, str] | None = None,
    path_override: tuple[Path, str] | None = None,
) -> dict[str, int]:
    if tuple(DIRECT_INCLUDES) != tuple(TARGETS) or tuple(EXPECTED_REEXPORTS) != tuple(TARGETS):
        raise BoundaryError("module boundary manifests do not match module targets")
    checked = 0
    fragments = 0
    for relative, patterns in TARGETS.items():
        wrapper = root / relative
        override = wrapper_override[1] if wrapper_override and wrapper_override[0] == wrapper else None
        assembled, paths, all_included = _assemble(root, wrapper, override, fragment_override)
        if len(wrapper.read_text(encoding="utf-8").splitlines() if override is None else override.splitlines()) > MAX_LINES:
            raise BoundaryError(f"{relative}: wrapper exceeds {MAX_LINES} lines")
        for path in all_included:
            source = fragment_override[1] if fragment_override and fragment_override[0] == path else _read(path)
            if len(source.splitlines()) > MAX_LINES:
                raise BoundaryError(f"{path.relative_to(root)} exceeds {MAX_LINES} lines")
        missing = [pattern for pattern in patterns if re.search(pattern, assembled, re.MULTILINE) is None]
        if missing:
            raise BoundaryError(f"{relative}: critical symbol missing: {missing[0]}")
        checked += 1
        fragments += len(paths)
    customer_tests = root / "crates/corelink-container/src/routes/customer/tests.rs"
    customer_override = path_override[1] if path_override and path_override[0] == customer_tests else None
    _validate_customer_test_tree(root, customer_override)
    return {"modules": checked, "fragments": fragments}


def _must_red(root: Path, wrapper: Path, mutated: str, label: str) -> None:
    try:
        verify(root, wrapper_override=(wrapper, mutated))
    except BoundaryError:
        return
    raise BoundaryError(f"mutation unexpectedly passed: {label}")


def self_test(root: Path = ROOT) -> None:
    wrapper = root / next(iter(TARGETS))
    text = _read(wrapper)
    mutated = re.sub(r'^\s*include!\([^\n]+\);\s*\n', "", text, count=1, flags=re.MULTILINE)
    _must_red(root, wrapper, mutated, "include removal")

    include_line_re = r'^\s*include!\("[^"\n]+"\);\s*\n'
    include_matches = list(re.finditer(include_line_re, text, re.MULTILINE))
    first_include = include_matches[0] if include_matches else None
    second_include = (
        include_matches[1]
        if len(include_matches) > 1
        else None
    )
    if first_include is None or second_include is None:
        raise BoundaryError("self-test wrapper has fewer than two direct includes")
    first = first_include.group(0)
    second = second_include.group(0)
    _must_red(root, wrapper, text.replace(first, first + first, 1), "duplicate direct include")
    _must_red(
        root,
        wrapper,
        text.replace(first + second, second + first, 1),
        "reordered direct includes",
    )
    include_path = re.search(r'include!\("([^"\n]+)"\)', first)
    if include_path is None:
        raise BoundaryError("self-test include path is missing")
    indent = first[: len(first) - len(first.lstrip())]
    bait = f'{indent}const INCLUDE_BAIT: &str = "include!(\\"{include_path.group(1)}\\");";\n'
    _must_red(root, wrapper, text.replace(first, bait, 1), "include-shaped string bait")

    for relative, expected in EXPECTED_REEXPORTS.items():
        wrapper = root / relative
        text = _read(wrapper)
        line_matches = list(
            re.finditer(rf'^[ \t]*{re.escape(expected)}[ \t]*\n', text, re.MULTILINE)
        )
        if len(line_matches) != 1:
            raise BoundaryError(
                f"self-test expected one re-export line in {relative}, found {len(line_matches)}"
            )
        match = line_matches[0]
        line = match.group(0)
        indent = line[: len(line) - len(line.lstrip())]
        opposite = (
            "pub(crate) use implementation::*;"
            if expected == "pub use implementation::*;"
            else "pub use implementation::*;"
        )
        opposite_line = f"{indent}{opposite}\n"
        mutated_opposite = text[: match.start()] + opposite_line + text[match.end() :]
        _must_red(root, wrapper, mutated_opposite, f"re-export visibility mutation: {relative}")
        mutated_removed = text[: match.start()] + text[match.end() :]
        _must_red(root, wrapper, mutated_removed, f"re-export removal: {relative}")
        mutated_duplicate = text[: match.start()] + line + line + text[match.end() :]
        _must_red(root, wrapper, mutated_duplicate, f"re-export duplication: {relative}")
        reexport_bait = f'{indent}const REEXPORT_BAIT: &str = "{expected}";\n'
        mutated_bait = text[: match.start()] + reexport_bait + text[match.end() :]
        _must_red(root, wrapper, mutated_bait, f"re-export string bait: {relative}")

    fragment = root / "crates/corelink-container/src/routes/ac/part-00.rs"
    source = _read(fragment)
    mutated_docs = source.replace("// `GET /v1/ac", "//! `GET /v1/ac", 1)
    try:
        verify(root, fragment_override=(fragment, mutated_docs))
    except BoundaryError:
        pass
    else:
        raise BoundaryError("inner-doc mutation was not detected")

    nested = root / "crates/corelink-container/src/routes/ac/part-01.rs"
    source = _read(nested)
    mutated_include = source.replace('include!("tests-00-00.rs");', 'include!("ac/tests-00-00.rs");', 1)
    try:
        verify(root, fragment_override=(nested, mutated_include))
    except BoundaryError:
        pass
    else:
        raise BoundaryError("nested-test-path mutation was not detected")

    customer_tests = root / "crates/corelink-container/src/routes/customer/tests.rs"
    source = _read(customer_tests)
    mutated_customer = source.replace('tests_account.rs', 'missing_account.rs', 1)
    try:
        verify(root, path_override=(customer_tests, mutated_customer))
    except BoundaryError:
        pass
    else:
        raise BoundaryError("customer nested-test missing-file mutation was not detected")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    try:
        report = verify(Path(args.root).resolve())
        if args.self_test:
            self_test(Path(args.root).resolve())
    except (BoundaryError, OSError, UnicodeDecodeError) as exc:
        print(f"B-126-M1 module boundary guard: FAIL: {exc}", file=sys.stderr)
        return 1
    print(f"B-126-M1 module boundary guard: PASS: {report['modules']} modules, {report['fragments']} fragments")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
