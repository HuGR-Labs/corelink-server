#!/usr/bin/env python3
"""Fail-closed structural gate for the npm storage-cap flow (F-008/B-259).

The gate intentionally parses a small, deterministic subset of Rust instead
of searching source text. It masks comments and every Rust string/character
literal, balances delimiters, removes only statically false branches, and
then checks the one ``NpmMoatStore::put`` body. A source fragment elsewhere
cannot satisfy the contract.
"""

from __future__ import annotations

from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parents[1]
MAX_SOURCE_BYTES = 2_000_000
MAX_TOKENS = 500_000


def _tokens(source: str) -> list[str]:
    """Lex the Rust syntax needed by this contract, masking opaque literals."""
    if len(source.encode("utf-8")) > MAX_SOURCE_BYTES:
        raise AssertionError("source exceeds bounded verifier input")
    out: list[str] = []
    i = 0
    n = len(source)
    multi = ("<<=", ">>=", "=>", "==", "!=", "&&", "||", "::", "->", ">=", "<=", "..")
    while i < n:
        char = source[i]
        if char.isspace():
            i += 1
            continue
        if source.startswith("//", i):
            end = source.find("\n", i + 2)
            if end < 0:
                break
            i = end + 1
            continue
        if source.startswith("/*", i):
            depth = 1
            i += 2
            while i < n and depth:
                if source.startswith("/*", i):
                    depth += 1
                    i += 2
                elif source.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            if depth:
                raise AssertionError("unterminated block comment")
            continue

        raw_prefix = None
        if source.startswith("br", i) and i + 2 < n and source[i + 2] in "#\"":
            raw_prefix = "br"
        elif char == "r" and i + 1 < n and source[i + 1] in "#\"":
            raw_prefix = "r"
        if raw_prefix is not None:
            j = i + len(raw_prefix)
            hashes = 0
            while j < n and source[j] == "#":
                hashes += 1
                j += 1
            if j >= n or source[j] != '"':
                raise AssertionError("malformed raw string")
            close = "]" + ("#" * hashes) + '"'
            end = source.find(close, j + 1)
            if end < 0:
                raise AssertionError("unterminated raw string")
            out.append("<string>")
            i = end + len(close)
            continue

        if char == '"' or (
            char in {"b", "c"} and i + 1 < n and source[i + 1] == '"'
        ):
            i += 2 if char in {"b", "c"} else 1
            while i < n:
                if source[i] == "\\":
                    i += 2
                elif source[i] == '"':
                    i += 1
                    break
                else:
                    i += 1
            else:
                raise AssertionError("unterminated string literal")
            out.append("<string>")
            continue

        if char == "'":
            if i + 1 < n and (source[i + 1].isalpha() or source[i + 1] == "_"):
                # `'a'` is a character literal; `'a`/`'static` is a lifetime.
                if i + 2 < n and source[i + 2] == "'":
                    i += 1
                else:
                    i += 1
                    continue
            i += 1
            while i < n:
                if source[i] == "\\":
                    i += 2
                elif source[i] == "'":
                    i += 1
                    break
                else:
                    i += 1
            else:
                raise AssertionError("unterminated character literal")
            out.append("<char>")
            continue

        if char.isalpha() or char == "_":
            j = i + 1
            while j < n and (source[j].isalnum() or source[j] == "_"):
                j += 1
            out.append(source[i:j])
            i = j
        elif char.isdigit():
            j = i + 1
            while j < n and (source[j].isalnum() or source[j] == "_"):
                j += 1
            out.append(source[i:j])
            i = j
        else:
            token = next((op for op in multi if source.startswith(op, i)), None)
            if token is None:
                token = char
            out.append(token)
            i += len(token)
        if len(out) > MAX_TOKENS:
            raise AssertionError("token stream exceeds bounded verifier input")
    return out


_OPEN_TO_CLOSE = {"{": "}", "(": ")", "[": "]"}
_CLOSE_TO_OPEN = {value: key for key, value in _OPEN_TO_CLOSE.items()}


def _matching_close(tokens: list[str], opening: int) -> int:
    expected = _OPEN_TO_CLOSE.get(tokens[opening])
    if expected is None:
        raise AssertionError("expected an opening delimiter")
    stack = [expected]
    for pos in range(opening + 1, len(tokens)):
        token = tokens[pos]
        if token in _OPEN_TO_CLOSE:
            stack.append(_OPEN_TO_CLOSE[token])
        elif token in _CLOSE_TO_OPEN:
            if not stack or token != stack.pop():
                raise AssertionError("mismatched Rust delimiters")
            if not stack:
                return pos
    raise AssertionError("unterminated Rust delimiter")


def _validate_balanced(tokens: list[str]) -> None:
    stack: list[str] = []
    for token in tokens:
        if token in _OPEN_TO_CLOSE:
            stack.append(_OPEN_TO_CLOSE[token])
        elif token in _CLOSE_TO_OPEN:
            if not stack or token != stack.pop():
                raise AssertionError("mismatched Rust delimiters")
    if stack:
        raise AssertionError("unterminated Rust delimiter")


def _body_after(tokens: list[str], marker: list[str]) -> list[str]:
    """Extract the sole balanced brace body following a declaration marker."""
    matches = [pos for pos in range(len(tokens) - len(marker) + 1) if tokens[pos : pos + len(marker)] == marker]
    if len(matches) != 1:
        raise AssertionError(f"expected one {' '.join(marker)}, found {len(matches)}")
    start = matches[0] + len(marker)
    brace = next((pos for pos in range(start, len(tokens)) if tokens[pos] == "{"), None)
    if brace is None:
        raise AssertionError(f"missing body after {' '.join(marker)}")
    end = _matching_close(tokens, brace)
    return tokens[brace + 1 : end]


def _condition_is_false(condition: list[str]) -> bool:
    while len(condition) >= 2 and condition[0] == "(" and condition[-1] == ")":
        try:
            if _matching_close(condition, 0) != len(condition) - 1:
                break
        except AssertionError:
            return False
        condition = condition[1:-1]
    if condition == ["false"]:
        return True
    return (
        len(condition) == 3
        and condition[1] == "=="
        and condition[0].isdigit()
        and condition[2].isdigit()
        and condition[0] != condition[2]
    )


def _if_open(tokens: list[str], if_pos: int) -> int:
    paren = bracket = 0
    for pos in range(if_pos + 1, len(tokens)):
        token = tokens[pos]
        if token == "(":
            paren += 1
        elif token == ")":
            paren -= 1
            if paren < 0:
                raise AssertionError("mismatched if condition")
        elif token == "[":
            bracket += 1
        elif token == "]":
            bracket -= 1
            if bracket < 0:
                raise AssertionError("mismatched if condition")
        elif token == "{" and paren == 0 and bracket == 0:
            return pos
    raise AssertionError("if has no body")


def _reachable(tokens: list[str]) -> list[str]:
    """Remove only complete branches proven unreachable by literal syntax."""
    _validate_balanced(tokens)
    out: list[str] = []
    pos = 0
    while pos < len(tokens):
        if tokens[pos] != "if":
            out.append(tokens[pos])
            pos += 1
            continue
        brace = _if_open(tokens, pos)
        if not _condition_is_false(tokens[pos + 1 : brace]):
            out.append(tokens[pos])
            pos += 1
            continue
        end = _matching_close(tokens, brace)
        pos = end + 1
    _validate_balanced(out)
    return out


def _indices(tokens: list[str], needle: list[str]) -> list[int]:
    return [pos for pos in range(len(tokens) - len(needle) + 1) if tokens[pos : pos + len(needle)] == needle]


def _top_level_positions(tokens: list[str], needle: list[str]) -> list[int]:
    found: list[int] = []
    brace_depth = 0
    for pos, token in enumerate(tokens):
        if token == "{":
            brace_depth += 1
        elif token == "}":
            brace_depth -= 1
            if brace_depth < 0:
                raise AssertionError("mismatched body braces")
        if brace_depth == 0 and tokens[pos : pos + len(needle)] == needle:
            found.append(pos)
    if brace_depth:
        raise AssertionError("unterminated body braces")
    return found


def _read(root: Path, relative: str) -> str:
    path = root / relative
    try:
        if path.stat().st_size > MAX_SOURCE_BYTES:
            raise AssertionError(f"{relative} exceeds bounded verifier input")
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise AssertionError(f"cannot read required source {relative}: {exc}") from exc


def verify(root: Path = ROOT) -> None:
    npm = _tokens(_read(root, "crates/corelink-container/src/routes/npm.rs"))
    build = _tokens(_read(root, "crates/corelink-container/src/routes/build.rs"))
    struct = _body_after(npm, ["struct", "NpmMoatStore"])
    if len(_indices(struct, ["cap_resolver", ":", "Arc", "<", "dyn", "TenantCapResolver", ">"])) != 1:
        raise AssertionError("NpmMoatStore must carry exactly one TenantCapResolver field")

    impl = _body_after(npm, ["impl", "CasStore", "for", "NpmMoatStore"])
    put = _reachable(_body_after(impl, ["async", "fn", "put"]))
    _validate_balanced(put)
    tenant_binding = ["let", "tenant_text", "=", "tenant", ".", "to_string", "(", ")"]
    tenant_positions = _top_level_positions(put, tenant_binding)
    if len(tenant_positions) != 1:
        raise AssertionError("npm put must bind the PAT-derived tenant exactly once")
    tenant_declarations = _indices(put, ["let", "tenant_text"]) + _indices(put, ["let", "mut", "tenant_text"])
    if len(tenant_declarations) != 1 or tenant_declarations[0] != tenant_positions[0]:
        raise AssertionError("npm put must not rebind tenant_text to an alternate namespace")
    tenant_assignments = _indices(put, ["tenant_text", "="])
    if len(tenant_assignments) != 1:
        raise AssertionError("npm put must not assign an alternate tenant namespace")

    write = ["self", ".", "moat", ".", "put", "("]
    writes = _indices(put, write)
    if len(writes) != 1:
        raise AssertionError(f"npm put must have exactly one reachable self.moat.put (found {len(writes)})")
    write_pos = writes[0]
    if _top_level_positions(put, write) != [write_pos]:
        raise AssertionError("npm moat write must be a top-level statement in NpmMoatStore::put")

    forbidden_before_write = {"return", "?", "if", "match", "while", "for", "loop", "break", "continue"}
    if any(token in forbidden_before_write for token in put[:write_pos]):
        raise AssertionError("npm moat write has a preceding return/short-circuit or control-flow guard")

    resolver = [
        "let", "storage_cap_bytes", "=", "self", ".", "cap_resolver", ".",
        "resolve_storage_cap", "(", "&", "tenant_text", ")", ".", "await",
    ]
    resolver_positions = _top_level_positions(put, resolver)
    if len(resolver_positions) != 1 or resolver_positions[0] >= write_pos:
        raise AssertionError("npm put must resolve the current tenant cap before its sole write")
    if tenant_positions[0] >= resolver_positions[0]:
        raise AssertionError("npm put must derive tenant_text before resolving its cap")
    if len(_indices(put, ["resolve_storage_cap"])) != 1:
        raise AssertionError("npm put must not resolve a cap for an alternate tenant")

    open_pos = write_pos + len(write) - 1
    close_pos = _matching_close(put, open_pos)
    arguments = put[open_pos + 1 : close_pos]
    comma_positions = [pos for pos, token in enumerate(arguments) if token == ","]
    if len(comma_positions) != 3:
        raise AssertionError("npm moat write must have exactly four arguments")
    cap_argument = arguments[comma_positions[-1] + 1 :]
    if cap_argument != ["storage_cap_bytes"]:
        raise AssertionError("npm moat write cap argument must be the resolved non-None value")

    _contains_npm_contract(npm)
    _contains_build_contract(build)


def _contains(tokens: list[str], needle: list[str], label: str) -> None:
    if not _indices(tokens, needle):
        raise AssertionError(f"{label}: missing active token sequence {' '.join(needle)}")


def _contains_npm_contract(npm: list[str]) -> None:
    _contains(npm, ["pub", "fn", "router_with_cap_resolver"], "npm router")
    _contains(npm, ["NpmMoatStore", "{", "moat", ",", "cap_resolver"], "npm router state")


def _contains_build_contract(build: list[str]) -> None:
    _contains(build, ["npm_cap_resolver", ":", "Arc", "<", "dyn", "crate", "::", "oci_cap", "::", "TenantCapResolver", ">"], "build npm resolver")
    _contains(build, ["Arc", "::", "new", "(", "crate", "::", "oci_cap", "::", "D1TenantCapResolver", "::", "new", "(", "d1", ".", "clone", "(", ")", ")", ")"], "build D1 resolver")
    _contains(build, ["npm", "::", "router_with_cap_resolver", "("], "build npm router")
    _contains(build, ["npm", "::", "router_with_cap_resolver", "(", "npm_cas_read", ",", "npm_cas_write"], "build npm arguments")
    _contains(build, ["npm_cap_resolver", ",", ")"], "build cap argument")


def _mutated(root: Path, replacements: dict[str, str]) -> Path:
    tmp = Path(tempfile.mkdtemp(prefix="f008-npm-mutation-"))
    changed_any = False
    for relative in ("crates/corelink-container/src/routes/npm.rs", "crates/corelink-container/src/routes/build.rs"):
        target = tmp / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        text = _read(root, relative)
        changed = False
        for old, new in replacements.items():
            if old in text:
                text = text.replace(old, new, 1)
                changed = True
        changed_any |= changed
        target.write_text(text, encoding="utf-8")
    if not changed_any:
        raise AssertionError(f"mutation anchor absent: {replacements!r}")
    return tmp


def self_test(root: Path = ROOT) -> None:
    verify(root)
    original_flow = (
        "let storage_cap_bytes = self.cap_resolver.resolve_storage_cap(&tenant_text).await;\n"
        "        self.moat\n"
        "            .put(&tenant_text, &digest.to_hex(), bytes, storage_cap_bytes)"
    )
    mutations = [
        {"resolve_storage_cap(&tenant_text).await": "None"},
        {"let storage_cap_bytes = self.cap_resolver.resolve_storage_cap(&tenant_text).await;": "// resolver moved\n        let storage_cap_bytes = None;"},
        {"let storage_cap_bytes = self.cap_resolver.resolve_storage_cap(&tenant_text).await;": 'let storage_cap_bytes = "resolve_storage_cap(&tenant_text).await";'},
        {"resolve_storage_cap(&tenant_text)": "resolve_storage_cap(&PUBLIC_NAMESPACE)"},
        {".put(&tenant_text, &digest.to_hex(), bytes, storage_cap_bytes)": ".put(&tenant_text, &digest.to_hex(), bytes, None)"},
        {
            original_flow:
            "if false {\n            " + original_flow.replace("\n        ", "\n            ") + "\n        };\n"
            "        let storage_cap_bytes = None;\n"
            "        self.moat\n"
            "            .put(&tenant_text, &digest.to_hex(), bytes, None)",
        },
        {
            original_flow:
            "if 0 == 1 {\n            " + original_flow.replace("\n        ", "\n            ") + "\n        };\n"
            "        let storage_cap_bytes = None;\n"
            "        self.moat\n"
            "            .put(&tenant_text, &digest.to_hex(), bytes, None)",
        },
        {
            "        let storage_cap_bytes = self.cap_resolver.resolve_storage_cap(&tenant_text).await;":
            "        return Ok(());\n"
            "        let storage_cap_bytes = self.cap_resolver.resolve_storage_cap(&tenant_text).await;",
        },
        {
            original_flow:
            original_flow + "\n        self.moat\n            .put(&tenant_text, &digest.to_hex(), bytes, None)",
        },
        {
            original_flow:
            original_flow + "\n        self.moat\n            .put(&other_tenant, &digest.to_hex(), bytes, None)",
        },
    ]
    for mutation in mutations:
        candidate = _mutated(root, mutation)
        try:
            verify(candidate)
        except AssertionError:
            continue
        raise AssertionError(f"semantic mutation survived: {mutation!r}")


if __name__ == "__main__":
    self_test()
    print("F008 npm storage-cap contract: PASS (structural flow + 10 semantic mutations)")
