#!/usr/bin/env python3
"""Static B126-H2 guard for the adapter PAT extraction.

This is intentionally a source-level guard, not a substitute for the focal
Rust test. It proves that the stable adapter_pat module still wires every
production submodule, that the security pipeline remains in the verifier
module, and that each extracted file stays bounded. The mutation self-test
proves removal of a module declaration or public reexport is rejected.
"""

from __future__ import annotations

from pathlib import Path
import re
import sys


MAX_LINES = 1000
PRODUCTION = (
    "adapter_pat.rs",
    "adapter_pat_lookup.rs",
    "adapter_pat_gate.rs",
    "adapter_pat_crypto.rs",
    "adapter_pat_verifier.rs",
)
TEST_FRAGMENTS = (
    "adapter_pat_tests_1.rs",
    "adapter_pat_tests_2.rs",
    "adapter_pat_tests_3.rs",
)
VERIFIER_FRAGMENTS = (
    "adapter_pat_verifier/part-01.rs",
    "adapter_pat_verifier/part-02.rs",
    "adapter_pat_verifier/part-03.rs",
)
TEST_FRAGMENT_PATHS = {
    "adapter_pat_tests_1.rs": ("adapter_pat_tests_1/part-01.rs",),
    "adapter_pat_tests_2.rs": ("adapter_pat_tests_2/part-01.rs",),
    "adapter_pat_tests_3.rs": (
        "adapter_pat_tests_3/part-01.rs",
        "adapter_pat_tests_3/part-02.rs",
    ),
}
INCLUDED_FRAGMENTS = {
    "adapter_pat_verifier.rs": VERIFIER_FRAGMENTS,
    **TEST_FRAGMENT_PATHS,
}
PARENT_INCLUDES = (
    "adapter_pat_tests_1.rs",
    "adapter_pat_tests_2.rs",
    "adapter_pat_tests_3.rs",
)
SOURCE_MANIFEST = (
    PRODUCTION
    + TEST_FRAGMENTS
    + tuple(
        fragment for fragments in INCLUDED_FRAGMENTS.values() for fragment in fragments
    )
)
EXPECTED = {
    "adapter_pat_lookup.rs": (
        r"pub struct PatRow\b",
        r"pub trait PatRowLookup\b",
        r"pub struct SingleFlightPatLookup\b",
        r"impl PatRowLookup for D1HttpClient",
    ),
    "adapter_pat_gate.rs": (
        r"pub\(super\) struct PerTenantGate\b",
        r"pub\(super\) const ARGON2_PER_TENANT_PERMITS",
        r"pub\(super\) const UNKNOWN_TOKEN_BUCKET",
    ),
    "adapter_pat_crypto.rs": (
        r"pub\(super\) struct SecretMatchMemo\b",
        r"pub\(super\) struct FlightGroup",
        r"pub\(super\) fn secret_match_fingerprint",
        r"pub\(super\) enum VerifyFlight",
        r"pub\(super\) enum BurnFlight",
    ),
    "adapter_pat_verifier.rs": (
        r"pub struct PatVerifier\b",
        r"verify_hmac_only_multi",
        r"self\.lookup\.lookup",
        r"verify_with_hash_multi",
        r"requires_cache_read",
        r"requires_cache_write",
        r"Arc::new\(SingleFlightPatLookup::new\(inner\)\)",
    ),
}


class GuardError(RuntimeError):
    pass


def _blank(chars: list[str], start: int, end: int) -> None:
    """Mask a source range while preserving line positions."""
    for index in range(start, end):
        if chars[index] != "\n":
            chars[index] = " "


def _raw_string_end(source: str, start: int) -> int | None:
    """Return the end of a Rust raw string/byte string beginning at ``start``."""
    index = start
    if source.startswith("br", index):
        index += 2
    elif source.startswith("r", index):
        index += 1
    else:
        return None
    hashes = 0
    while index < len(source) and source[index] == "#":
        hashes += 1
        index += 1
    if index >= len(source) or source[index] != '"':
        return None
    terminator = '"' + ("#" * hashes)
    end = source.find(terminator, index + 1)
    return len(source) if end < 0 else end + len(terminator)


def _quoted_end(source: str, start: int) -> int:
    """Return the end of a normal Rust string or char literal."""
    quote = source[start]
    index = start + 1
    while index < len(source):
        if source[index] == "\\":
            index += 2
            continue
        if source[index] == quote:
            return index + 1
        if source[index] == "\n":
            return index
        index += 1
    return len(source)


def _char_end(source: str, start: int) -> int | None:
    """Return the end of a Rust char literal, or None for a lifetime."""
    index = start + 1
    if index >= len(source) or source[index] in ("\n", "'"):
        return None
    if source[index] == "\\":
        index += 2
        if index < len(source) and source[index - 1] == "u" and source[index] == "{":
            closing = source.find("}", index + 1)
            if closing < 0:
                return None
            index = closing + 1
    else:
        index += 1
    return index + 1 if index < len(source) and source[index] == "'" else None


def mask_rust(source: str, *, strings: bool) -> str:
    """Mask comments, and optionally strings/chars, without shifting offsets.

    Rust permits nested block comments and raw strings with arbitrary hash
    delimiters. Handling those here keeps source-level markers tied to code,
    rather than to documentation, string literals, or a comment-shaped bait.
    """
    chars = list(source)
    index = 0
    while index < len(source):
        if source.startswith("//", index):
            end = source.find("\n", index)
            end = len(source) if end < 0 else end
            _blank(chars, index, end)
            index = end
            continue
        if source.startswith("/*", index):
            depth = 1
            end = index + 2
            while end < len(source) and depth:
                if source.startswith("/*", end):
                    depth += 1
                    end += 2
                elif source.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            _blank(chars, index, end)
            index = end
            continue

        raw_end = _raw_string_end(source, index)
        if raw_end is not None:
            if strings:
                _blank(chars, index, raw_end)
            index = raw_end
            continue

        if source[index] == '"' or (
            source[index] == "b"
            and index + 1 < len(source)
            and source[index + 1] == '"'
        ):
            start = index if source[index] == '"' else index + 1
            end = _quoted_end(source, start)
            if strings:
                _blank(chars, index, end)
            index = end
            continue

        if source[index] == "'":
            end = _char_end(source, index)
            if end is not None:
                if strings:
                    _blank(chars, index, end)
                index = end
                continue

        index += 1
    return "".join(chars)


def code_source(source: str) -> str:
    return mask_rust(source, strings=True)


def comment_masked_source(source: str) -> str:
    return mask_rust(source, strings=False)


def include_paths(source: str) -> tuple[str, ...]:
    """Extract active include! paths, ignoring comments and string bait."""
    comments_masked = comment_masked_source(source)
    code_masked = code_source(source)
    paths: list[str] = []
    pattern = re.compile(r'include!\s*\(\s*"((?:\\.|[^"\\])*)"\s*\)\s*;')
    for match in pattern.finditer(comments_masked):
        start = match.start()
        if code_masked[start : start + len("include!")] != "include!":
            continue
        paths.append(match.group(1))
    return tuple(paths)


def files(root: Path) -> dict[str, str]:
    src = root / "crates" / "corelink-container" / "src"
    on_disk = {path.relative_to(src).as_posix() for path in src.glob("adapter_pat*.rs")}
    on_disk.update(
        path.relative_to(src).as_posix() for path in src.glob("adapter_pat_*/**/*.rs")
    )
    if on_disk != set(SOURCE_MANIFEST):
        raise GuardError(
            "adapter PAT source population drift: "
            f"on disk={sorted(on_disk)!r}, manifest={sorted(SOURCE_MANIFEST)!r}"
        )
    names = PRODUCTION + TEST_FRAGMENTS
    data = {name: (src / name).read_text(encoding="utf-8") for name in names}
    for fragment in (
        fragment for fragments in INCLUDED_FRAGMENTS.values() for fragment in fragments
    ):
        data[fragment] = (src / fragment).read_text(encoding="utf-8")
    return data


def expanded(data: dict[str, str], name: str) -> str:
    """Return one logical source unit, preserving include order."""
    return "\n".join(
        (data[name], *(data[fragment] for fragment in INCLUDED_FRAGMENTS.get(name, ())))
    )


def require_exact_markers(source: str, markers: tuple[str, ...], label: str) -> None:
    for marker in markers:
        count = len(re.findall(marker, source, flags=re.MULTILINE))
        if count != 1:
            raise GuardError(
                f"{label} marker count for {marker!r} is {count}, expected one"
            )


def validate(data: dict[str, str]) -> None:
    required = tuple(PRODUCTION + TEST_FRAGMENTS) + tuple(
        fragment for fragments in INCLUDED_FRAGMENTS.values() for fragment in fragments
    )
    missing = sorted(set(required) - set(data))
    if missing:
        raise GuardError(f"missing adapter PAT source(s): {', '.join(missing)}")

    for name in data:
        line_count = len(data[name].splitlines())
        if line_count > MAX_LINES:
            raise GuardError(f"{name} has {line_count} lines (limit {MAX_LINES})")

    for name in PRODUCTION + TEST_FRAGMENTS:
        line_count = len(expanded(data, name).splitlines())
        if line_count > MAX_LINES:
            raise GuardError(
                f"expanded {name} has {line_count} lines (limit {MAX_LINES})"
            )

    parent = data["adapter_pat.rs"]
    parent_code = code_source(parent)
    required_modules = (
        "mod adapter_pat_crypto;",
        "mod adapter_pat_gate;",
        "mod adapter_pat_lookup;",
        "mod adapter_pat_verifier;",
    )
    require_exact_markers(
        parent_code,
        tuple(re.escape(marker) for marker in required_modules),
        "adapter_pat.rs module wiring",
    )

    required_exports = (
        "pub use adapter_pat_lookup::{PatRow, PatRowLookup, SingleFlightPatLookup};",
        "pub use adapter_pat_verifier::{PatVerifier, VerifyError};",
    )
    require_exact_markers(
        parent_code,
        tuple(re.escape(marker) for marker in required_exports),
        "stable public reanchor",
    )
    actual_parent_includes = include_paths(parent)
    if actual_parent_includes != PARENT_INCLUDES:
        raise GuardError(
            "adapter_pat.rs direct include wiring must match the closed manifest: "
            f"{actual_parent_includes!r}"
        )

    for name, fragments in INCLUDED_FRAGMENTS.items():
        actual = include_paths(data[name])
        if actual != fragments:
            raise GuardError(
                f"{name} fragment wiring must contain each include exactly once "
                f"and in order: {actual!r}"
            )

    for name, markers in EXPECTED.items():
        source = code_source(expanded(data, name))
        for marker in markers:
            if not re.search(marker, source):
                raise GuardError(
                    f"{name} missing required symbol/pipeline marker {marker!r}"
                )

    verifier = code_source(expanded(data, "adapter_pat_verifier.rs"))
    pipeline_markers = (
        r"^\s*let \(_env, token_id\) = verify_hmac_only_multi",
        r"^\s*self\.lookup\.lookup\(",
        r"^\s*let r = verify_with_hash_multi",
        r"^\s*if !requires_cache_read\(",
    )
    require_exact_markers(verifier, pipeline_markers, "adapter PAT pipeline")
    ordered = tuple(
        re.search(marker, verifier, flags=re.MULTILINE).start()
        for marker in pipeline_markers
    )
    if ordered != tuple(sorted(ordered)):
        raise GuardError(
            "verifier pipeline markers are no longer HMAC -> D1 -> Argon2id -> scope"
        )

    # The large implementation must not be silently recomposed into the
    # stable parent. Definitions are allowed only in their dedicated files.
    forbidden_parent_defs = (
        r"pub struct PatVerifier\b",
        r"pub struct PatRow\b",
        r"pub trait PatRowLookup\b",
        r"pub struct SingleFlightPatLookup\b",
    )
    for marker in forbidden_parent_defs:
        if re.search(marker, parent_code):
            raise GuardError(f"implementation recomposed in adapter_pat.rs: {marker}")


def mutation_self_test(data: dict[str, str]) -> None:
    # A guard that only checks the pristine tree is easy to weaken. Both
    # mutations must turn the same validator red without touching the worktree.
    for marker in (
        "mod adapter_pat_verifier;",
        "pub use adapter_pat_verifier::{PatVerifier, VerifyError};",
        'include!("adapter_pat_tests_2.rs");',
    ):
        mutant = dict(data)
        mutant["adapter_pat.rs"] = mutant["adapter_pat.rs"].replace(marker, "", 1)
        try:
            validate(mutant)
        except GuardError:
            continue
        raise GuardError(f"wiring mutation unexpectedly passed: {marker}")

    # A live module declaration replaced by a comment must not satisfy the
    # wiring census. This is the exact bypass that raw str.count accepted.
    commented_mod = dict(data)
    commented_mod["adapter_pat.rs"] = data["adapter_pat.rs"].replace(
        "mod adapter_pat_verifier;",
        "// mod adapter_pat_verifier;",
        1,
    )
    try:
        validate(commented_mod)
    except GuardError:
        pass
    else:
        raise GuardError("commented module mutation unexpectedly passed")

    # The same declaration-looking text in a string literal is not an active
    # module. This pins the string masking path independently of comments.
    string_mod = dict(data)
    string_mod["adapter_pat.rs"] = data["adapter_pat.rs"].replace(
        "mod adapter_pat_verifier;",
        'const _B126_MODULE_BAIT: &str = "mod adapter_pat_verifier;";',
        1,
    )
    try:
        validate(string_mod)
    except GuardError:
        pass
    else:
        raise GuardError("string module bait mutation unexpectedly passed")

    # The parent has a closed direct-include manifest. An unlisted include is
    # a wiring change even if every expected include remains present.
    unlisted_include = dict(data)
    unlisted_include["adapter_pat.rs"] = (
        data["adapter_pat.rs"] + '\ninclude!("adapter_pat_unlisted.rs");\n'
    )
    try:
        validate(unlisted_include)
    except GuardError:
        pass
    else:
        raise GuardError("unlisted parent include mutation unexpectedly passed")

    # The implementation itself is included in bounded fragments. Removing or
    # duplicating one must invalidate the same census; otherwise a stale guard
    # could silently stop checking the expensive half of the pipeline.
    verifier_parent = data["adapter_pat_verifier.rs"]
    for marker in (
        'include!("adapter_pat_verifier/part-02.rs");',
        'include!("adapter_pat_verifier/part-03.rs");',
    ):
        mutant = dict(data)
        mutant["adapter_pat_verifier.rs"] = verifier_parent.replace(marker, "", 1)
        try:
            validate(mutant)
        except GuardError:
            continue
        raise GuardError(f"verifier fragment removal unexpectedly passed: {marker}")

    duplicate = dict(data)
    duplicate["adapter_pat_verifier.rs"] = verifier_parent.replace(
        'include!("adapter_pat_verifier/part-02.rs");',
        'include!("adapter_pat_verifier/part-02.rs");\n'
        'include!("adapter_pat_verifier/part-02.rs");',
        1,
    )
    try:
        validate(duplicate)
    except GuardError:
        pass
    else:
        raise GuardError("verifier fragment duplication unexpectedly passed")

    # A marker-looking comment is not evidence that the production stage is
    # wired. Anchor the pipeline census to executable-looking Rust lines and
    # prove the guard rejects a comment bait in place of the D1 call.
    bait = dict(data)
    bait["adapter_pat_verifier/part-02.rs"] = data[
        "adapter_pat_verifier/part-02.rs"
    ].replace(
        "self.lookup.lookup(token_id.as_str()),",
        "/* self.lookup.lookup(token_id.as_str()), */",
        1,
    )
    try:
        validate(bait)
    except GuardError:
        pass
    else:
        raise GuardError("pipeline bait mutation unexpectedly passed")


def main() -> int:
    root = (
        Path(sys.argv[1]).resolve()
        if len(sys.argv) > 1
        else Path(__file__).resolve().parents[1]
    )
    try:
        data = files(root)
        validate(data)
        mutation_self_test(data)
    except (OSError, GuardError) as exc:
        print(f"B126-H2 FAIL: {exc}", file=sys.stderr)
        return 1
    print(
        "B126-H2 PASS: bounded extraction, stable reanchors, pipeline wiring, and mutations"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
