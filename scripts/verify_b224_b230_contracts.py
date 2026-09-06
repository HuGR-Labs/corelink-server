#!/usr/bin/env python3
"""Fail-closed closure contracts for the seven D03 B-224..B-230 lanes.

Each lane has an independent executable check over its canonical source.  The
checks deliberately assert ordering and paired controls where applicable; a
single generic grep must not be able to hide a distinct failure.  ``--self-test``
applies an adversarial mutation to each required marker in memory and proves
that the corresponding lane turns red.
"""

from __future__ import annotations

import argparse
import sys
import tempfile
from pathlib import Path

import verify_b226_alert_wiring


ROOT = Path(__file__).resolve().parents[1]


class ContractError(ValueError):
    """A source-specific B-224..B-230 contract is absent or weakened."""


def _read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        raise ContractError(f"missing or non-regular canonical artifact: {relative}")
    return path.read_text(encoding="utf-8")


def _require(text: str, marker: str, lane: str) -> None:
    if marker not in text:
        raise ContractError(f"{lane}: required executable marker missing: {marker}")


def _rust_tokens(source: str, lane: str) -> list[tuple[str, str]]:
    """Tokenize active Rust structure, discarding comments and literals.

    B225 spans an ``include!`` manifest and two implementation fragments.
    Raw ``find`` calls can therefore be satisfied by a stale marker in a
    comment/string, or by a marker copied into the wrong fragment.  This
    small fail-closed lexer handles nested block comments, line comments,
    regular/raw strings, and character literals while retaining punctuation
    needed for structural checks.
    """

    tokens: list[tuple[str, str]] = []
    index = 0
    length = len(source)

    def quoted_end(start: int, quote: str) -> int:
        cursor = start + 1
        escaped = False
        while cursor < length:
            char = source[cursor]
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                return cursor + 1
            cursor += 1
        raise ContractError(f"{lane}: unterminated Rust {quote}-literal")

    while index < length:
        if source.startswith("//", index):
            newline = source.find("\n", index + 2)
            index = length if newline < 0 else newline + 1
            continue
        if source.startswith("/*", index):
            depth = 1
            index += 2
            while index < length and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            if depth:
                raise ContractError(f"{lane}: unterminated Rust block comment")
            continue

        # Raw strings must be recognized before `r`/`b` identifiers.
        raw_prefix = None
        if source.startswith("br", index):
            raw_prefix = index + 2
        elif source.startswith("r", index):
            raw_prefix = index + 1
        if raw_prefix is not None:
            hashes_end = raw_prefix
            while hashes_end < length and source[hashes_end] == "#":
                hashes_end += 1
            if hashes_end < length and source[hashes_end] == '"':
                hashes = source[raw_prefix:hashes_end]
                closing = '"' + hashes
                end = source.find(closing, hashes_end + 1)
                if end < 0:
                    raise ContractError(f"{lane}: unterminated Rust raw string")
                tokens.append(("string", source[hashes_end + 1 : end]))
                index = end + len(closing)
                continue

        if source.startswith("b\"", index):
            end = quoted_end(index + 1, '"')
            tokens.append(("string", source[index + 2 : end - 1]))
            index = end
            continue
        if source[index] == '"':
            end = quoted_end(index, '"')
            tokens.append(("string", source[index + 1 : end - 1]))
            index = end
            continue
        if source.startswith("b'", index):
            end = quoted_end(index + 1, "'")
            tokens.append(("char", source[index + 2 : end - 1]))
            index = end
            continue
        if source[index] == "'" and index + 1 < length:
            # A short character literal is not code structure.  Lifetimes
            # (e.g. `'a`) remain ordinary punctuation/ident tokens.
            end = quoted_end(index, "'") if index + 2 < length and "'" in source[index + 1 : index + 4] else None
            if end is not None:
                tokens.append(("char", source[index + 1 : end - 1]))
                index = end
                continue

        char = source[index]
        if char.isspace():
            index += 1
            continue
        if char.isalpha() or char == "_":
            end = index + 1
            while end < length and (source[end].isalnum() or source[end] == "_"):
                end += 1
            tokens.append(("ident", source[index:end]))
            index = end
            continue
        if char.isdigit():
            end = index + 1
            while end < length and (source[end].isalnum() or source[end] in "._"):
                end += 1
            tokens.append(("number", source[index:end]))
            index = end
            continue
        operator = next(
            (candidate for candidate in ("::", "&&", "||", ">=", "<=", "==", "!=", "=>") if source.startswith(candidate, index)),
            char,
        )
        tokens.append(("punct", operator))
        index += len(operator)
    return tokens


def _token_values(tokens: list[tuple[str, str]], *, include_strings: bool = False) -> list[str]:
    if include_strings:
        return [value for _kind, value in tokens]
    return [value for kind, value in tokens if kind not in {"string", "char"}]


def _has_sequence(tokens: list[tuple[str, str]], expected: tuple[str, ...]) -> bool:
    values = _token_values(tokens)
    width = len(expected)
    return any(values[index : index + width] == list(expected) for index in range(len(values) - width + 1))


def _sequence_index(tokens: list[tuple[str, str]], expected: tuple[str, ...]) -> int:
    values = _token_values(tokens)
    width = len(expected)
    return next(
        (index for index in range(len(values) - width + 1) if values[index : index + width] == list(expected)),
        -1,
    )


def _block_after(tokens: list[tuple[str, str]], opening: int, lane: str) -> list[tuple[str, str]]:
    """Return the active token body beginning at an opening brace."""

    values = _token_values(tokens)
    if opening >= len(values) or values[opening] != "{":
        raise ContractError(f"{lane}: expected an active Rust block")
    depth = 0
    for index in range(opening, len(values)):
        if values[index] == "{":
            depth += 1
        elif values[index] == "}":
            depth -= 1
            if depth == 0:
                return [("ident", value) for value in values[opening + 1 : index]]
    raise ContractError(f"{lane}: active Rust block is unterminated")


def _require_sequence(tokens: list[tuple[str, str]], expected: tuple[str, ...], lane: str, label: str) -> None:
    if not _has_sequence(tokens, expected):
        raise ContractError(f"{lane}: active Rust sequence missing: {label}")


def _rust_function(tokens: list[tuple[str, str]], name: str, lane: str) -> list[tuple[str, str]]:
    """Return one active, brace-balanced Rust function body."""

    values = _token_values(tokens)
    start = next(
        (index for index in range(len(values) - 2) if values[index : index + 3] == ["fn", name, "("]),
        None,
    )
    if start is None:
        raise ContractError(f"{lane}: active function signature missing: {name}")
    opening = next((index for index in range(start + 3, len(values)) if values[index] == "{"), None)
    if opening is None:
        raise ContractError(f"{lane}: active function has no body: {name}")
    depth = 0
    for index in range(opening, len(values)):
        if values[index] == "{":
            depth += 1
        elif values[index] == "}":
            depth -= 1
            if depth == 0:
                return [("ident", value) for value in values[opening + 1 : index]]
    raise ContractError(f"{lane}: active function is unterminated: {name}")


def _active_include(tokens: list[tuple[str, str]], path: str, lane: str) -> None:
    values = _token_values(tokens, include_strings=True)
    for index in range(len(values) - 4):
        if values[index : index + 5] == ["include", "!", "(", path, ")"]:
            return
    raise ContractError(f"{lane}: active include missing: {path}")


def _function(text: str, signature: str, lane: str) -> str:
    start = text.find(signature)
    if start < 0:
        raise ContractError(f"{lane}: function signature missing: {signature}")
    brace = text.find("{", start)
    if brace < 0:
        raise ContractError(f"{lane}: function has no body: {signature}")
    depth = 0
    for index in range(brace, len(text)):
        if text[index] == "{":
            depth += 1
        elif text[index] == "}":
            depth -= 1
            if depth == 0:
                return text[start : index + 1]
    raise ContractError(f"{lane}: unterminated function: {signature}")


def check_b224(root: Path) -> None:
    lane = "B-224"
    text = _read(root, "crates/corelink-adapter-host/src/npm/upstream.rs")
    _require(text, "pub const NPM_METADATA_MAX_RESPONSE_BYTES: usize", lane)
    _require(text, "async fn read_bounded_json_response(", lane)
    _require(text, "resp.bytes_stream()", lane)
    _require(text, "NpmAdapterError::MetadataOversized(total as u64)", lane)
    for signature in ("pub async fn fetch_metadata(", "pub async fn fetch_search("):
        body = _function(text, signature, lane)
        if ".bytes().await" in body:
            raise ContractError(f"{lane}: {signature} still buffers without the bounded reader")
        _require(body, "read_bounded_json_response(resp).await", lane)


def check_b225(root: Path) -> None:
    lane = "B-225"
    manifest_tokens = _rust_tokens(_read(root, "crates/corelink-container/src/routes/oci.rs"), lane)
    _active_include(manifest_tokens, "oci/b126_m2_impl_01.rs", lane)
    _active_include(manifest_tokens, "oci/b126_m2_impl_02.rs", lane)
    impl_01_tokens = _rust_tokens(
        _read(root, "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs"), lane
    )
    impl_02_tokens = _rust_tokens(
        _read(root, "crates/corelink-container/src/routes/oci/b126_m2_impl_02.rs"), lane
    )
    _require_sequence(impl_01_tokens, ("max_blob_size_bytes", ":", "u64"), lane, "max_blob_size_bytes: u64")
    _require_sequence(impl_01_tokens, ("fn", "with_allowlist_and_blob_limit", "("), lane, "blob-limit constructor")
    _require_sequence(
        impl_02_tokens,
        ("let", "blob_size_limit_bytes", "=", "defaults", "::", "BLOB_SIZE_LIMIT_BYTES", ";"),
        lane,
        "configured blob-size limit",
    )
    _require_sequence(
        impl_02_tokens,
        ("public_allowlist", ",", "blob_size_limit_bytes", ","),
        lane,
        "constructor allowlist/blob limit",
    )
    _require_sequence(impl_02_tokens, ("blob_size_limit_bytes", ","), lane, "blob limit propagation")
    _require_sequence(
        impl_01_tokens,
        (
            "const",
            "OCI_MAX_INFLIGHT_BYTES_PER_TENANT",
            ":",
            "u64",
            "=",
            "128",
            "*",
            "1024",
            "*",
            "1024",
            ";",
        ),
        lane,
        "per-tenant in-flight ceiling",
    )
    if _has_sequence(
        impl_01_tokens,
        ("OCI_MAX_INFLIGHT_BYTES_PER_TENANT", ":", "u64", "=", "OCI_MAX_INFLIGHT_BYTES"),
    ):
        raise ContractError(f"{lane}: tenant in-flight cap must remain below the global cap")
    if _has_sequence(impl_01_tokens, ("OCI_MAX_UPLOAD_BYTES_PER_SESSION",)):
        raise ContractError(f"{lane}: fixed 64 MiB session cap must not remain")
    body = _rust_function(impl_01_tokens, "append_chunk", lane)
    for sequence, label in (
        (("current_len",), "current session length"),
        (("checked_add", "(", "chunk_len", ")"), "checked length addition"),
        (("drop", "(", "g", ")"), "mutex release"),
        (("fetch_sub", "(", "chunk_len"), "global reservation release"),
        (("release_tenant_bytes", "(", "&", "tenant_text", ",", "chunk_len", ")"), "tenant reservation release"),
    ):
        _require_sequence(body, sequence, lane, label)
    # The exact active condition is intentional: allowing an arbitrary suffix
    # would let `&& false` preserve the marker while disabling the ceiling.
    guard = ("if", "next_len", ">", "self", ".", "max_blob_size_bytes", "{")
    _require_sequence(body, guard, lane, "active pre-append session ceiling")
    append = ("session", ".", "buf", ".", "extend_from_slice", "(", "&", "chunk", ")")
    check_index = _sequence_index(body, guard)
    append_index = _sequence_index(body, append)
    if check_index < 0 or append_index < 0 or check_index > append_index:
        raise ContractError(f"{lane}: session ceiling must be checked before buffer append")
    guard_body = _block_after(body, check_index + len(guard) - 1, lane)
    _require_sequence(guard_body, ("return", "Err", "("), lane, "guard must reject the oversized append")
    upload_tokens = _rust_tokens(_read(root, "crates/corelink-adapter-host/src/oci/push/upload.rs"), lane)
    patch_body = _rust_function(upload_tokens, "patch", lane)
    _require_sequence(
        patch_body,
        ("let", "chunk_len", "=", "u64", "::", "try_from", "(", "chunk", ".", "len", "(", ")", ")"),
        lane,
        "adapter chunk length",
    )
    patch_guard = ("if", "chunk_len", ">", "blob_size_limit_bytes", "{")
    _require_sequence(patch_body, patch_guard, lane, "adapter pre-append chunk ceiling")
    patch_check = _sequence_index(patch_body, patch_guard)
    patch_append = _sequence_index(
        patch_body,
        (".", "append_chunk", "(", "tenant", ",", "upload_uuid", ",", "chunk", ")"),
    )
    if patch_check < 0 or patch_append < 0 or patch_check > patch_append:
        raise ContractError(f"{lane}: configured blob cap must precede adapter append")
    put_body = _rust_function(upload_tokens, "put", lane)
    _require_sequence(put_body, ("if", "tail_len", ">", "blob_size_limit_bytes", "{"), lane, "PUT tail ceiling")


def check_b226(root: Path) -> None:
    lane = "B-226"
    try:
        verify_b226_alert_wiring.verify(root)
    except verify_b226_alert_wiring.ContractError as error:
        raise ContractError(f"{lane}: {error}") from error


def check_b227(root: Path) -> None:
    lane = "B-227"
    text = _read(root, "crates/corelink-container/src/main.rs")
    _require(text, "EnvFilter::try_from_default_env()", lane)
    _require(text, '"info,corelink_server=info"', lane)
    if '"info,corelink_server=debug"' in text:
        raise ContractError(f"{lane}: production fallback still enables debug logging")


def check_b228(root: Path) -> None:
    lane = "B-228"
    text = _read(root, "crates/corelink-container/src/routes/users.rs")
    _require(text, "fn sanitize_log_value(value: &str) -> String", lane)
    _require(text, "if c.is_control() { '�' }", lane)
    _require(text, "principal = %sanitize_log_value(&token_prefix)", lane)
    if "principal = %token_prefix" in text:
        raise ContractError(f"{lane}: raw principal is still used in a log field")


def check_b229(root: Path) -> None:
    lane = "B-229"
    script = _read(root, "scripts/verify-signup-worker-secrets.sh")
    workflow = _read(root, ".github/workflows/signup-worker-deploy.yml")
    _require(script, "REQUIRED=(", lane)
    _require(script, "  CLERK_WEBHOOK_SECRET", lane)
    _require(workflow, "bash ../../scripts/verify-signup-worker-secrets.sh", lane)
    _require(workflow, "name: Verify required runtime secrets", lane)


def check_b230(root: Path) -> None:
    lane = "B-230"
    text = _read(root, "wrangler.toml")
    start = text.find("[env.prod]\n")
    if start < 0:
        raise ContractError(f"{lane}: [env.prod] section missing")
    end = text.find("\n[env.prod.triggers]", start)
    section = text[start:] if end < 0 else text[start:end]
    _require(section, 'R2_CHUNK_BUCKET = "corelink-chunk-iad"', lane)
    _require(section, 'R2_CHUNK_REGION = "iad"', lane)


CHECKS = {
    "B-224": check_b224,
    "B-225": check_b225,
    "B-226": check_b226,
    "B-227": check_b227,
    "B-228": check_b228,
    "B-229": check_b229,
    "B-230": check_b230,
}


def verify(root: Path = ROOT, lane: str | None = None) -> dict[str, str]:
    selected = (lane,) if lane else tuple(CHECKS)
    for item in selected:
        try:
            CHECKS[item](root)
        except KeyError as exc:
            raise ContractError(f"unknown lane: {item}") from exc
    return {item: "pass" for item in selected}


def self_test(root: Path = ROOT) -> None:
    """Kill independent contract mutations without editing the source tree."""
    mutations = {
        "B-224": (("marker-removal", "crates/corelink-adapter-host/src/npm/upstream.rs", "pub const NPM_METADATA_MAX_RESPONSE_BYTES: usize", "pub const REMOVED_METADATA_CAP: usize", None),),
        "B-225": (
            ("removal", "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs", "if next_len > self.max_blob_size_bytes", "if next_len > u64::MAX", None),
            ("comment-bait", "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs", "if next_len > self.max_blob_size_bytes {", "if false { // if next_len > self.max_blob_size_bytes", None),
            ("string-bait", "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs", "if next_len > self.max_blob_size_bytes {", "if false { let b225_bait = \"if next_len > self.max_blob_size_bytes\";", None),
            ("no-op-guard", "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs", "if next_len > self.max_blob_size_bytes {", "if next_len > self.max_blob_size_bytes && false {", None),
            ("wrong-fragment", "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs", "if next_len > self.max_blob_size_bytes {", "if next_len > u64::MAX {", ("crates/corelink-container/src/routes/oci/b126_m2_impl_02.rs", "\nstruct B225WrongFragmentDecoy { max_blob_size_bytes: u64 }\nimpl B225WrongFragmentDecoy {\n    fn check(&self, next_len: u64) {\n        if next_len > self.max_blob_size_bytes { }\n    }\n}\n")),
        ),
        "B-226": ((
            "channel-removal",
            "crates/corelink-ops/src/alerts/alerter.rs",
            "self\n            .dispatch_channel(AlertChannel::Dashboard, envelope)\n            .await",
            "self.dispatch_channel(AlertChannel::Email, envelope).await",
            None,
        ),),
        "B-227": (("debug-fallback", "crates/corelink-container/src/main.rs", '"info,corelink_server=info"', '"info,corelink_server=debug"', None),),
        "B-228": (("sanitizer-removal", "crates/corelink-container/src/routes/users.rs", "fn sanitize_log_value(value: &str) -> String", "fn removed_log_value(value: &str) -> String", None),),
        "B-229": (("secret-removal", "scripts/verify-signup-worker-secrets.sh", "  CLERK_WEBHOOK_SECRET\n", "  REMOVED_SECRET\n", None),),
        "B-230": (("region-removal", "wrangler.toml", 'R2_CHUNK_REGION = "iad"', 'R2_CHUNK_REGION = "removed"', None),),
    }
    for lane, cases in mutations.items():
        for label, relative, needle, replacement, extra in cases:
            with tempfile.TemporaryDirectory(prefix=f"{lane.lower()}-{label}-") as temporary:
                temp_root = Path(temporary)
                mutated = _read(root, relative).replace(needle, replacement, 1)
                destination = temp_root / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_text(mutated, encoding="utf-8")
                # Copy every artifact consumed by a multi-file check.  The
                # B225 mutant must never be checked against an incomplete tree.
                if lane == "B-229":
                    other = temp_root / ".github/workflows/signup-worker-deploy.yml"
                    other.parent.mkdir(parents=True, exist_ok=True)
                    other.write_text(_read(root, ".github/workflows/signup-worker-deploy.yml"), encoding="utf-8")
                elif lane == "B-225":
                    for dependency in (
                        "crates/corelink-container/src/routes/oci.rs",
                        "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs",
                        "crates/corelink-container/src/routes/oci/b126_m2_impl_02.rs",
                        "crates/corelink-adapter-host/src/oci/push/upload.rs",
                    ):
                        if dependency == relative:
                            continue
                        other = temp_root / dependency
                        other.parent.mkdir(parents=True, exist_ok=True)
                        other.write_text(_read(root, dependency), encoding="utf-8")
                if extra is not None:
                    extra_relative, extra_text = extra
                    extra_path = temp_root / extra_relative
                    extra_path.parent.mkdir(parents=True, exist_ok=True)
                    extra_path.write_text(extra_path.read_text(encoding="utf-8") + extra_text, encoding="utf-8")
                try:
                    CHECKS[lane](temp_root)
                except ContractError:
                    pass
                else:
                    raise ContractError(f"{lane}: adversarial mutation survived: {label}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--id", choices=tuple(CHECKS))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    try:
        result = verify(ROOT, args.id)
        if args.self_test:
            self_test(ROOT)
    except (ContractError, OSError, UnicodeError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1
    print("PASS: " + ", ".join(result))
    if args.self_test:
        print("PASS: adversarial mutations rejected for every lane")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
