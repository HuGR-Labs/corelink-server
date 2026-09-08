"""Small Rust source masker for fail-closed static verifiers.

The verifier contracts only need a lexical view, not a parser.  This scanner
keeps source offsets and line shape while masking nested comments, ordinary
strings/chars, and raw strings, so markers in reviewer bait cannot satisfy a
contract.  ``strip_comments`` is the companion view used for macro arguments
such as ``include!("fragment.rs")`` where string contents are meaningful.
"""

from __future__ import annotations

import re


def strip_comments(source: str) -> str:
    """Blank nested Rust comments while retaining literal contents."""
    out: list[str] = []
    i = 0
    depth = 0
    while i < len(source):
        ch = source[i]
        nxt = source[i + 1] if i + 1 < len(source) else ""
        if ch == "/" and nxt == "/":
            out.extend((" ", " "))
            i += 2
            while i < len(source) and source[i] != "\n":
                out.append(" ")
                i += 1
            continue
        if ch == "/" and nxt == "*":
            out.extend((" ", " "))
            i += 2
            depth = 1
            while i < len(source) and depth:
                if source[i : i + 2] == "/*":
                    out.extend((" ", " "))
                    depth += 1
                    i += 2
                elif source[i : i + 2] == "*/":
                    out.extend((" ", " "))
                    depth -= 1
                    i += 2
                else:
                    out.append("\n" if source[i] == "\n" else " ")
                    i += 1
            continue
        if ch == '"':
            out.append(ch)
            i += 1
            while i < len(source):
                out.append(source[i])
                if source[i] == "\\" and i + 1 < len(source):
                    i += 1
                    out.append(source[i])
                elif source[i] == '"':
                    i += 1
                    break
                i += 1
            continue
        raw_end = _raw_string_end(source, i)
        if raw_end is not None:
            out.extend(source[i:raw_end])
            i = raw_end
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def mask(source: str) -> str:
    """Blank comments and literals, retaining line/offset shape."""
    out: list[str] = []
    i = 0
    depth = 0
    state = "code"
    quote = ""
    while i < len(source):
        ch = source[i]
        nxt = source[i + 1] if i + 1 < len(source) else ""
        if state == "line":
            if ch == "\n":
                out.append(ch)
                state = "code"
            else:
                out.append(" ")
            i += 1
            continue
        if state == "block":
            if ch == "/" and nxt == "*":
                out.extend((" ", " "))
                depth += 1
                i += 2
            elif ch == "*" and nxt == "/":
                out.extend((" ", " "))
                depth -= 1
                i += 2
                if depth == 0:
                    state = "code"
            else:
                out.append("\n" if ch == "\n" else " ")
                i += 1
            continue
        if state == "literal":
            if ch == "\\":
                out.append(" ")
                if i + 1 < len(source):
                    out.append("\n" if source[i + 1] == "\n" else " ")
                    i += 2
                else:
                    i += 1
            elif ch == quote:
                out.append(" ")
                state = "code"
                i += 1
            else:
                out.append("\n" if ch == "\n" else " ")
                i += 1
            continue
        raw_end = _raw_string_end(source, i)
        if raw_end is not None:
            out.extend("\n" if c == "\n" else " " for c in source[i:raw_end])
            i = raw_end
            continue
        if ch == "/" and nxt == "/":
            out.extend((" ", " "))
            state = "line"
            i += 2
        elif ch == "/" and nxt == "*":
            out.extend((" ", " "))
            state = "block"
            depth = 1
            i += 2
        elif ch == '"' or (ch == "'" and _looks_like_char(source, i)):
            out.append(" ")
            quote = ch
            state = "literal"
            i += 1
        else:
            out.append(ch)
            i += 1
    return "".join(out)


def include_paths(source: str) -> tuple[str, ...]:
    """Return real ``include!("...")`` paths, excluding lexical decoys."""
    code = mask(source)
    paths: list[str] = []
    for match in re.finditer(r'include!\(\s*"([^"]+)"\s*\)\s*;', source):
        if code[match.start() : match.start() + len("include!")] == "include!":
            paths.append(match.group(1))
    return tuple(paths)


def marker_present(source: str, marker: str) -> bool:
    """Whether a marker starts in code, rather than a comment or literal."""
    return marker_count(source, marker) > 0


def marker_count(source: str, marker: str) -> int:
    """Count marker occurrences whose starting token is real code."""
    code = mask(source)
    prefix = marker.split('"', 1)[0]
    if not prefix:
        return 0
    count = 0
    for start in (match.start() for match in re.finditer(re.escape(marker), source)):
        if code[start : start + len(prefix)] == prefix:
            count += 1
    return count


def normal_string_marker_count(source: str, marker: str) -> int:
    """Count marker occurrences inside real normal strings (not raw strings)."""
    spans = _normal_string_spans(source)
    return sum(
        1
        for match in re.finditer(re.escape(marker), source)
        if any(start <= match.start() and match.end() <= end for start, end in spans)
    )


def _normal_string_spans(source: str) -> list[tuple[int, int]]:
    spans: list[tuple[int, int]] = []
    i = 0
    depth = 0
    while i < len(source):
        if source[i : i + 2] == "//":
            i += 2
            while i < len(source) and source[i] != "\n":
                i += 1
            continue
        if source[i : i + 2] == "/*":
            i += 2
            depth = 1
            while i < len(source) and depth:
                if source[i : i + 2] == "/*":
                    depth += 1
                    i += 2
                elif source[i : i + 2] == "*/":
                    depth -= 1
                    i += 2
                else:
                    i += 1
            continue
        raw_end = _raw_string_end(source, i)
        if raw_end is not None:
            i = raw_end
            continue
        if source[i] == '"':
            start = i + 1
            i += 1
            while i < len(source):
                if source[i] == "\\":
                    i += 2
                elif source[i] == '"':
                    spans.append((start, i))
                    i += 1
                    break
                else:
                    i += 1
            continue
        i += 1
    return spans


def _looks_like_char(source: str, start: int) -> bool:
    """Distinguish a Rust char literal from a lifetime marker."""
    end = start + 1
    escaped = False
    while end < len(source) and end - start <= 8:
        ch = source[end]
        if escaped:
            escaped = False
        elif ch == "\\":
            escaped = True
        elif ch == "'":
            return end > start + 1
        elif ch == "\n":
            return False
        end += 1
    return False


def _raw_string_end(source: str, start: int) -> int | None:
    if source[start] == "r":
        quote_index = start + 1
    elif source[start : start + 2] == "br":
        quote_index = start + 2
    else:
        return None
    hash_index = quote_index
    while hash_index < len(source) and source[hash_index] == "#":
        hash_index += 1
    if hash_index >= len(source) or source[hash_index] != '"':
        return None
    hashes = source[quote_index:hash_index]
    terminator = '"' + hashes
    end = source.find(terminator, hash_index + 1)
    return len(source) if end < 0 else end + len(terminator)
