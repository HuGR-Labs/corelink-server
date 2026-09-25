#!/usr/bin/env python3
"""
validate_no_shared_rustup_mutation — CI gate against provisioning a Rust
toolchain on the shared-$HOME self-hosted fleet.

WHY THIS GATE EXISTS
--------------------
Sibling of scripts/validate_shared_home_cache_guard.py. Same fleet, same shared
`$HOME`, one level up: that gate protects `~/.cargo`; this one protects
`~/.rustup`.

Five GitHub Actions runners share ONE `$HOME`, so ONE `~/.rustup`.
`dtolnay/rust-toolchain` is built for GitHub-hosted runners, where `$HOME` is
job-private and ephemeral. Per its own action.yml it runs:

    rustup toolchain install <tc> --profile minimal --no-self-update
    rustup default <tc>

Both write into that SHARED tree while other jobs execute binaries out of it.
Two failure modes, and the second is the dangerous one:

  1. CRASH. Every `~/.cargo/bin/*` is a symlink to the single `rustup` shim, so a
     concurrent install can make `rustc` unexecutable mid-build:

         Caused by:
           could not execute process `rustc …` (never executed)
         Caused by:
           No such file or directory (os error 2)

     This took the whole host down for a day on 2026-06-15 — see the "ROOT FIX"
     note in .github/workflows/fuzz-nightly.yml.

  2. SILENT REPRODUCIBILITY DRIFT. `rustup default` rewrites the MACHINE-GLOBAL
     default toolchain. rust-toolchain.toml pins the workspace to a specific
     channel precisely because "rustc minor upgrades can introduce LLVM
     non-determinism that breaks reproducibility" (ADR-0015). A job that defaults
     the host to `stable` therefore moves every OTHER concurrent job off that pin
     with no error anywhere. Measured 2026-08-03: the host default had already
     drifted to `stable-x86_64-apple-darwin` while the workspace pinned 1.91.1.

THE RULE
--------
A self-hosted job must NOT provision a toolchain. Use the pre-installed host
toolchain via `bash scripts/ci-use-host-toolchain.sh [targets…]`, which reads the
channel from rust-toolchain.toml (so it cannot drift from ADR-0015) and fails
loudly if the toolchain or a requested target is absent.

Also banned: hardcoding a versioned toolchain path such as
`$HOME/.rustup/toolchains/1.91.1-x86_64-apple-darwin/bin`. It works, but it is a
SECOND copy of the pin that no gate keeps in sync with rust-toolchain.toml — the
next ADR-0015 bump would silently leave CI on the old compiler. (Unversioned
`nightly` paths are allowed: cargo-fuzz needs nightly, and there is no pin to
drift from.)

FAIL-CLOSED
-----------
If this script finds ZERO self-hosted jobs it EXITS NON-ZERO rather than
reporting success. A parser that silently matches nothing is the "green gate that
proves nothing" failure mode this repo has been bitten by repeatedly.

PULL-REQUEST BASELINE MODE
--------------------------
`--baseline-workflows` compares a candidate tree to its immutable PR base. It
permits only findings already present in that base and fails on an added finding;
push and manual runs stay strict. Both trees must parse and inspect a nonzero
self-hosted population.
"""
from __future__ import annotations

import argparse
from collections import Counter
import pathlib
import re
import sys
from dataclasses import dataclass
from typing import Any, Sequence

try:
    import yaml
except ImportError:  # pragma: no cover - exercised on the minimal fleet image
    yaml = None

WORKFLOWS = pathlib.Path(".github/workflows")

PROVISIONING_ACTIONS = re.compile(
    r"^(?:dtolnay/rust-toolchain|actions-rs/toolchain|actions-rust-lang/setup-rust-toolchain)(?:@|\s|$)"
)
# A rustup toolchain path carrying an explicit VERSION (1.91.1, 1.90, …).
# `nightly` / `stable` bare channels are not version pins and are not flagged.
HARDCODED_CHANNEL = re.compile(r"rustup/toolchains/\d[\w.\-]*")
JOB_START = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
RUNS_ON = re.compile(r"^(?P<indent>\s*)runs-on:\s*(?P<value>.*?)\s*$")


class WorkflowParseError(ValueError):
    """A workflow could not be parsed safely enough for this fail-closed gate."""


@dataclass(frozen=True)
class Job:
    name: str
    runs_on: Any
    start: int
    end: int
    definition: Any = None


@dataclass(frozen=True)
class Finding:
    """One rustup-safety violation with a stable baseline-comparison key."""

    workflow: str
    job: str
    rule: str
    subject: str
    lineno: int
    source: str

    @property
    def identity(self) -> tuple[str, str, str, str]:
        """Exclude line numbers so harmless movement cannot create a new finding."""
        return (self.workflow, self.job, self.rule, self.subject)

    def render(self) -> str:
        if self.rule == "provisioning-action":
            return (
                f"{self.workflow}:{self.lineno}: job '{self.job}' provisions a toolchain on the shared-$HOME fleet.\n"
                f"    uses: {self.subject}\n"
                "    FIX: replace with `run: bash scripts/ci-use-host-toolchain.sh [targets…]`."
            )
        return (
            f"{self.workflow}:{self.lineno}: job '{self.job}' hardcodes a versioned toolchain path — a second\n"
            "    copy of the ADR-0015 pin that nothing keeps in sync with rust-toolchain.toml.\n"
            f"    {self.source}\n"
            "    FIX: `run: bash scripts/ci-use-host-toolchain.sh` (it reads the channel)."
        )


def _strip_trailing_comment(runs_on: str) -> str:
    """Drop a trailing `# …` from a scalar `runs-on:` value.

    The repo convention is to justify a runner choice inline. The structural
    workflow parser removes this comment before classifying the value:

        runs-on: corelink  # zero-hosted: python3 baked into the image

    Parsing `jobs[*].runs-on` as a YAML value covers scalar, quoted scalar,
    inline sequence, block sequence, and expression forms. PyYAML is used when
    present; the gate also carries a stdlib-only fallback for the minimal fleet
    image. Both paths fail closed on malformed input rather than silently
    shrinking the population being inspected.
    """
    value = runs_on.strip()
    quote: str | None = None
    escaped = False
    for idx, char in enumerate(value):
        if quote == '"' and escaped:
            escaped = False
            continue
        if quote == '"' and char == "\\":
            escaped = True
            continue
        if char in {"'", '"'}:
            if quote is None:
                quote = char
            elif quote == char:
                # YAML single quoted strings escape a quote by doubling it.
                if char == "'" and value[idx : idx + 2] == "''":
                    continue
                quote = None
            continue
        if char == "#" and quote is None and (idx == 0 or value[idx - 1].isspace()):
            return value[:idx].rstrip()
    return value


def is_self_hosted(runs_on: Any) -> bool:
    """Classify every legal GitHub Actions ``runs-on`` shape conservatively."""
    if isinstance(runs_on, dict):
        return any(is_self_hosted(value) for value in runs_on.values())
    if isinstance(runs_on, (list, tuple, set)):
        return any(is_self_hosted(value) for value in runs_on)
    if runs_on is None:
        return False
    value = _strip_trailing_comment(str(runs_on))
    # PyYAML has already removed YAML quotes.  The stdlib fallback intentionally
    # leaves them in place, so accept only the exact quoted scalar too; this does
    # not widen hosted labels or arbitrary runner names.
    unquoted = value
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
        unquoted = value[1:-1]
    return "self-hosted" in unquoted or unquoted == "corelink"


def _resolved_matrix_runs_on(runs_on: Any, definition: Any) -> Any | None:
    """Resolve a simple matrix runner; return None when it cannot be proven."""
    if not isinstance(runs_on, str):
        return runs_on
    expression = re.fullmatch(r"\s*\${{\s*matrix\.([A-Za-z0-9_-]+)\s*}}\s*", runs_on)
    if not expression:
        if "${{" not in runs_on:
            return runs_on
        # Expressions containing a literal self-hosted label are already
        # positively classified.  Any other expression is unresolved and must
        # be inspected conservatively rather than treated as hosted.
        return runs_on if is_self_hosted(runs_on) else None
    if not isinstance(definition, dict):
        return None
    strategy = definition.get("strategy")
    matrix = strategy.get("matrix") if isinstance(strategy, dict) else None
    if not isinstance(matrix, dict) or expression.group(1) not in matrix:
        return None
    return matrix[expression.group(1)]


def job_is_self_hosted(job: Job) -> bool:
    """Never drop an unresolved dynamic runner from the inspected population."""
    if job.runs_on is None:
        return False
    resolved = _resolved_matrix_runs_on(job.runs_on, job.definition)
    if resolved is not None:
        return is_self_hosted(resolved)
    # `${{ ... }}` may resolve to a hosted or self-hosted label at dispatch
    # time.  Without a complete expression evaluator, inspect it conservatively.
    return True


def _indent(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def _non_comment(line: str) -> bool:
    return bool(line.strip()) and not line.lstrip().startswith("#")


def _job_spans(lines: list[str]) -> list[tuple[str, int, int]]:
    """Find entries under the root ``jobs`` mapping without grepping nested text."""
    jobs_line = None
    for idx, line in enumerate(lines):
        if _non_comment(line) and _indent(line) == 0 and re.match(r"jobs:\s*(?:#.*)?$", line):
            jobs_line = idx
            break
    if jobs_line is None:
        return []

    entries: list[tuple[str, int]] = []
    for idx in range(jobs_line + 1, len(lines)):
        line = lines[idx]
        if not _non_comment(line):
            continue
        ind = _indent(line)
        if ind <= 0:
            break
        if ind == 2:
            match = re.fullmatch(r"  ([A-Za-z0-9_-]+):(?:\s*#.*)?", line)
            if match:
                entries.append((match.group(1), idx))
            elif ":" not in line:
                raise WorkflowParseError(f"line {idx + 1}: malformed jobs entry")

    return [
        (name, start, entries[pos + 1][1] if pos + 1 < len(entries) else len(lines))
        for pos, (name, start) in enumerate(entries)
    ]


def _split_flow_items(value: str, *, line: int) -> list[str]:
    """Split a YAML flow sequence while respecting quotes and nested brackets."""
    body = value.strip()[1:-1].strip()
    if not body:
        return []
    items: list[str] = []
    start = 0
    depth = 0
    quote: str | None = None
    escaped = False
    for idx, char in enumerate(body):
        if quote == '"' and escaped:
            escaped = False
            continue
        if quote == '"' and char == "\\":
            escaped = True
            continue
        if char in {"'", '"'}:
            if quote is None:
                quote = char
            elif quote == char:
                quote = None
        elif quote is None and char in "[{":
            depth += 1
        elif quote is None and char in "]}":
            depth -= 1
            if depth < 0:
                raise WorkflowParseError(f"line {line}: unbalanced flow sequence")
        elif quote is None and char == "," and depth == 0:
            item = body[start:idx].strip()
            if not item:
                raise WorkflowParseError(f"line {line}: empty runs-on list item")
            items.append(item)
            start = idx + 1
    if quote is not None or depth != 0:
        raise WorkflowParseError(f"line {line}: unterminated runs-on scalar/list")
    item = body[start:].strip()
    if not item:
        raise WorkflowParseError(f"line {line}: empty runs-on list item")
    items.append(item)
    return items


_YAML_DOUBLE_QUOTE_ESCAPES = {
    "0": "\0",
    "a": "\a",
    "b": "\b",
    "t": "\t",
    "n": "\n",
    "v": "\v",
    "f": "\f",
    "r": "\r",
    "e": "\x1b",
    " ": " ",
    '"': '"',
    "/": "/",
    "\\": "\\",
    "N": "\N{NO-BREAK SPACE}",
    "_": "\N{NARROW NO-BREAK SPACE}",
    "L": "\N{LINE SEPARATOR}",
    "P": "\N{PARAGRAPH SEPARATOR}",
}


def _decode_yaml_double_quoted(value: str, *, line: int) -> str:
    """Decode the YAML escapes supported by the stdlib parser, or fail closed."""
    if len(value) < 2 or value[0] != '"' or value[-1] != '"':
        raise WorkflowParseError(f"line {line}: unterminated quoted YAML scalar")
    result: list[str] = []
    index = 1
    end = len(value) - 1
    while index < end:
        char = value[index]
        if char != "\\":
            result.append(char)
            index += 1
            continue
        index += 1
        if index >= end:
            raise WorkflowParseError(f"line {line}: unterminated YAML escape")
        escape = value[index]
        if escape in _YAML_DOUBLE_QUOTE_ESCAPES:
            result.append(_YAML_DOUBLE_QUOTE_ESCAPES[escape])
            index += 1
            continue
        width = {"x": 2, "u": 4, "U": 8}.get(escape)
        if width is None:
            raise WorkflowParseError(f"line {line}: unsupported YAML double-quote escape \\{escape}")
        digits = value[index + 1 : index + 1 + width]
        if len(digits) != width or not re.fullmatch(r"[0-9A-Fa-f]+", digits):
            raise WorkflowParseError(f"line {line}: malformed YAML escape \\{escape}{digits}")
        codepoint = int(digits, 16)
        if codepoint > 0x10FFFF or 0xD800 <= codepoint <= 0xDFFF:
            raise WorkflowParseError(f"line {line}: invalid YAML Unicode escape \\{escape}{digits}")
        result.append(chr(codepoint))
        index += width + 1
    return "".join(result)


def _fallback_scalar(value: str, *, line: int) -> str:
    """Decode scalar quoting the fallback relies on; reject unknown escapes."""
    scalar = value.strip()
    if not scalar:
        raise WorkflowParseError(f"line {line}: empty YAML scalar")
    if scalar[0] == '"':
        return _decode_yaml_double_quoted(scalar, line=line)
    if scalar[0] == "'":
        if len(scalar) < 2 or scalar[-1] != "'":
            raise WorkflowParseError(f"line {line}: unterminated quoted YAML scalar")
        return scalar[1:-1].replace("''", "'")
    return scalar


def _flow_mapping_separator(item: str) -> int | None:
    """Find a YAML flow-map key/value colon outside quotes and nesting."""
    depth = 0
    quote: str | None = None
    escaped = False
    index = 0
    while index < len(item):
        char = item[index]
        if quote == '"' and escaped:
            escaped = False
        elif quote == '"' and char == "\\":
            escaped = True
        elif quote == "'" and char == "'" and index + 1 < len(item) and item[index + 1] == "'":
            index += 1
        elif char in {"'", '"'}:
            if quote is None:
                quote = char
            elif quote == char:
                quote = None
        elif quote is None and char in "[{":
            depth += 1
        elif quote is None and char in "]}":
            depth -= 1
        elif quote is None and depth == 0 and char == ":":
            next_char = item[index + 1] if index + 1 < len(item) else None
            if next_char is None or next_char.isspace() or next_char in ",]}[{":
                return index
        index += 1
    return None


def _fallback_flow_mapping_pairs(value: str, *, line: int) -> list[tuple[str, str | None]]:
    """Read a small flow-map slice used by a flow-style steps sequence."""
    mapping = value.strip()
    if not mapping.startswith("{") or not mapping.endswith("}"):
        raise WorkflowParseError(f"line {line}: unsupported non-map flow step")
    pairs: list[tuple[str, str | None]] = []
    for item in _split_flow_items(mapping, line=line):
        separator = _flow_mapping_separator(item)
        if separator is None:
            pairs.append((_fallback_scalar(item, line=line), None))
            continue
        key = _fallback_scalar(item[:separator], line=line)
        raw_value = item[separator + 1 :].strip()
        pairs.append((key, _fallback_scalar(raw_value, line=line) if raw_value else None))
    return pairs


def _fallback_runs_on(lines: list[str], start: int, end: int) -> Any:
    """Read runs-on structurally when the fleet's stdlib-only Python lacks PyYAML."""
    for idx in range(start, end):
        line = lines[idx]
        if not _non_comment(line) or _indent(line) != 4:
            continue
        match = RUNS_ON.fullmatch(line)
        if not match:
            continue
        raw = _strip_trailing_comment(match.group("value"))
        if raw:
            if re.search(r"(?:^|[\s,\[])&[A-Za-z0-9_-]+|(?:^|[\s,\[])\*[A-Za-z0-9_-]+", raw):
                raise WorkflowParseError(
                    f"line {idx + 1}: runs-on uses an anchor/alias unavailable to the stdlib parser"
                )
            if raw.startswith("["):
                if not raw.endswith("]"):
                    raise WorkflowParseError(f"line {idx + 1}: unterminated runs-on list")
                return [_fallback_scalar(item, line=idx + 1) for item in _split_flow_items(raw, line=idx + 1)]
            return _fallback_scalar(raw, line=idx + 1)

        values: list[str] = []
        for child in range(idx + 1, end):
            nested = lines[child]
            if not _non_comment(nested):
                continue
            if _indent(nested) <= 4:
                break
            if _indent(nested) != 6 or not nested.lstrip().startswith("-"):
                raise WorkflowParseError(f"line {child + 1}: malformed block runs-on sequence")
            item = nested.lstrip()[1:].strip()
            if not item:
                raise WorkflowParseError(f"line {child + 1}: empty runs-on list item")
            if re.match(r"[&*][A-Za-z0-9_-]+(?:\s|$)", item):
                raise WorkflowParseError(
                    f"line {child + 1}: runs-on uses an anchor/alias unavailable to the stdlib parser"
                )
            values.append(_fallback_scalar(_strip_trailing_comment(item), line=child + 1))
        return values
    return None


def _fallback_validate_document(lines: list[str]) -> None:
    """Reject malformed YAML outside the small structural slice we extract.

    There is no YAML parser in the fleet image.  The fallback therefore refuses
    the constructs it cannot validate instead of claiming that a malformed
    workflow is clean: flow collections, quoted scalars, mapping-looking plain
    values, and non-list/non-mapping lines.  Block scalar bodies are skipped;
    they are shell/JSON text, not YAML structure.
    """
    flow_stack: list[str] = []
    # Track whether the current flow item contains a token. Balanced
    # delimiters alone are insufficient: PyYAML rejects `{, key: value}` and
    # `{key: value,,}`, while a comma immediately before `}`/`]` is valid.
    flow_item_seen: list[bool] = []
    # For mapping frames, False means a key is being read and True means its
    # value is being read. Sequence frames use None. A flow collection used as
    # a mapping key is tracked separately because PyYAML cannot construct such
    # an unhashable key.
    flow_map_value: list[bool | None] = []
    flow_map_key_flow: list[bool] = []
    flow_collection_closed = False
    block_indent: int | None = None

    def scan_flow(value: str, lineno: int) -> None:
        nonlocal flow_collection_closed
        quote: str | None = None
        escaped = False
        quote_ended = False
        quote_role: str | None = None
        last_sig: str | None = None
        skip_single_quote = False
        for index, char in enumerate(value):
            if flow_collection_closed:
                if char.isspace():
                    continue
                parent_allows = bool(flow_stack) and (
                    char in ",]}"
                    or (
                        char == ":"
                        and flow_stack[-1] == "{"
                        and flow_map_value[-1] is False
                    )
                )
                if not parent_allows:
                    raise WorkflowParseError(
                        f"line {lineno}: trailing YAML text after flow collection"
                    )
                flow_collection_closed = False
            if skip_single_quote:
                skip_single_quote = False
                continue
            if quote == '"' and escaped:
                escaped = False
                continue
            if quote == '"' and char == "\\":
                escaped = True
                continue
            if quote == "'" and char == "'" and index + 1 < len(value) and value[index + 1] == "'":
                # YAML escapes a single quote by doubling it.
                skip_single_quote = True
                continue
            if char in {"'", '"'}:
                if quote is None:
                    if quote_ended:
                        raise WorkflowParseError(f"line {lineno}: malformed YAML quoted value")
                    quote_role = (
                        "key"
                        if flow_stack and flow_stack[-1] == "{" and last_sig in {"{", ","}
                        else "value"
                    )
                    quote = char
                    if flow_stack:
                        flow_item_seen[-1] = True
                elif quote == char:
                    quote = None
                    quote_ended = True
                continue
            if quote is not None:
                continue
            if quote_ended:
                if char.isspace():
                    continue
                if char not in ",]}" and not (char == ":" and quote_role == "key"):
                    raise WorkflowParseError(f"line {lineno}: trailing YAML text after quoted value")
                quote_ended = False
            if char in "[{":
                if flow_stack:
                    flow_item_seen[-1] = True
                    if flow_stack[-1] == "{" and flow_map_value[-1] is False:
                        flow_map_key_flow[-1] = True
                flow_stack.append(char)
                flow_item_seen.append(False)
                flow_map_value.append(False if char == "{" else None)
                flow_map_key_flow.append(False)
            elif char == ":":
                # A colon is a flow-map separator only at a YAML boundary.
                # This keeps `http://...` in a plain scalar instead of treating
                # its second colon as a repeated map separator.
                if (
                    flow_stack
                    and flow_stack[-1] == "{"
                    and flow_map_value[-1] is False
                    and not flow_item_seen[-1]
                ):
                    raise WorkflowParseError(f"line {lineno}: empty YAML flow-map key")
                next_char = value[index + 1] if index + 1 < len(value) else None
                is_separator = next_char is None or next_char.isspace() or next_char in ",]}[{"
                if flow_stack and flow_stack[-1] == "{" and is_separator:
                    if flow_map_value[-1] is False:
                        if not flow_item_seen[-1]:
                            raise WorkflowParseError(f"line {lineno}: empty YAML flow-map key")
                        if flow_map_key_flow[-1]:
                            raise WorkflowParseError(
                                f"line {lineno}: flow collection cannot be a YAML map key"
                            )
                        flow_map_value[-1] = True
                        flow_item_seen[-1] = False
                    elif flow_map_value[-1] is True:
                        if not flow_item_seen[-1]:
                            raise WorkflowParseError(f"line {lineno}: empty YAML flow-map value")
                        raise WorkflowParseError(f"line {lineno}: repeated YAML flow-map separator")
                elif flow_stack:
                    flow_item_seen[-1] = True
            elif char == ",":
                if flow_stack:
                    # A mapping value may be implicit null (`{key:,}`), while
                    # an empty key or sequence item around a comma is invalid.
                    if not flow_item_seen[-1] and not (
                        flow_stack[-1] == "{" and flow_map_value[-1] is True
                    ):
                        raise WorkflowParseError(f"line {lineno}: empty YAML flow collection item")
                    flow_item_seen[-1] = False
                    if flow_stack[-1] == "{":
                        flow_map_value[-1] = False
                        flow_map_key_flow[-1] = False
            elif char in "]}":
                expected = "]" if char == "]" else "}"
                if not flow_stack or (flow_stack[-1] == "[" and expected != "]") or (
                    flow_stack[-1] == "{" and expected != "}"
                ):
                    raise WorkflowParseError(f"line {lineno}: unbalanced YAML flow collection")
                flow_stack.pop()
                flow_item_seen.pop()
                flow_map_value.pop()
                flow_map_key_flow.pop()
                if flow_stack:
                    flow_item_seen[-1] = True
                flow_collection_closed = True
            elif flow_stack and not char.isspace():
                flow_item_seen[-1] = True
            if not char.isspace():
                last_sig = char
        if quote is not None:
            raise WorkflowParseError(f"line {lineno}: unterminated YAML quote")
        if not flow_stack:
            # A complete top-level flow value may end the line. A pending
            # suffix is only meaningful while a parent flow collection remains
            # open, where the next token must be a structural boundary.
            flow_collection_closed = False

    for lineno, original in enumerate(lines, start=1):
        if block_indent is not None:
            if _non_comment(original) and _indent(original) <= block_indent:
                block_indent = None
            else:
                continue
        if not _non_comment(original):
            continue
        if flow_collection_closed and not flow_stack:
            flow_collection_closed = False
        indent = _indent(original)
        value = _strip_trailing_comment(original).strip()
        if re.search(r":\s*[|>][+\-]?\d*\s*$", value):
            block_indent = indent
            continue
        if flow_stack:
            scan_flow(value, lineno)
            continue
        body = value[2:].lstrip() if value.startswith("- ") else value
        colon = body.find(":")
        if colon < 0:
            if not value.startswith(("-", "---", "...")):
                raise WorkflowParseError(f"line {lineno}: YAML mapping entry is missing ':'")
            continue
        scalar = body[colon + 1 :].strip()
        if not scalar:
            continue
        if scalar.startswith(("[", "{")):
            scan_flow(scalar, lineno)
            continue
        if scalar.startswith(('"', "'")):
            quote = scalar[0]
            escaped = False
            closed = False
            index = 1
            while index < len(scalar):
                char = scalar[index]
                if quote == '"' and escaped:
                    escaped = False
                elif quote == '"' and char == "\\":
                    escaped = True
                elif quote == "'" and char == "'" and index + 1 < len(scalar) and scalar[index + 1] == "'":
                    index += 1
                elif char == quote:
                    closed = True
                    if scalar[index + 1 :].strip():
                        raise WorkflowParseError(f"line {lineno}: trailing YAML text after quoted value")
                    break
                index += 1
            if not closed:
                raise WorkflowParseError(f"line {lineno}: unterminated YAML quote")
            continue
        if scalar.startswith(("]", "}")):
            raise WorkflowParseError(f"line {lineno}: unmatched YAML flow delimiter")
        # A second mapping colon in an unquoted scalar is invalid YAML.  This
        # catches `bad: key: value` without rejecting ordinary URLs/comments.
        if re.match(r"^[A-Za-z0-9_.-]+:\s", scalar):
            raise WorkflowParseError(f"line {lineno}: malformed YAML mapping value")
    if flow_stack:
        raise WorkflowParseError("end of file: unterminated YAML flow collection")


def _load_jobs(path: pathlib.Path) -> list[Job]:
    lines = path.read_text(encoding="utf-8").splitlines()
    spans = _job_spans(lines)
    if yaml is not None:
        try:
            document = yaml.safe_load("\n".join(lines))
        except yaml.YAMLError as exc:
            raise WorkflowParseError(f"{path}: invalid YAML: {exc}") from exc
        if document is None:
            return []
        if not isinstance(document, dict):
            raise WorkflowParseError(f"{path}: workflow root must be a mapping")
        jobs = document.get("jobs")
        if jobs is None:
            return []
        if not isinstance(jobs, dict):
            raise WorkflowParseError(f"{path}: jobs must be a mapping")
        by_name = {str(name): value for name, value in jobs.items()}
        result: list[Job] = []
        for name, start, end in spans:
            definition = by_name.get(name)
            if definition is not None and not isinstance(definition, dict):
                raise WorkflowParseError(f"{path}: job '{name}' must be a mapping")
            result.append(
                Job(name, definition.get("runs-on") if definition else None, start, end, definition)
            )
        # A structurally valid YAML document whose jobs use unusual key spelling
        # must not silently shrink the inspected population.
        if len(result) != len(by_name):
            raise WorkflowParseError(f"{path}: could not locate every jobs entry structurally")
        return result

    _fallback_validate_document(lines)
    result = [Job(name, _fallback_runs_on(lines, start, end), start, end) for name, start, end in spans]
    return result


def _source_line_for_uses(lines: list[str], start: int, end: int, action: str) -> int:
    """Keep a useful source location after loading a step structurally."""
    for idx in range(start, end):
        line = lines[idx]
        inline = re.match(r"^ {6}-\s+uses:\s*(.*?)\s*$", line)
        sibling = re.match(r"^ {8}uses:\s*(.*?)\s*$", line)
        match = inline or sibling
        if match and _strip_trailing_comment(match.group(1)).strip(" '\"") == action:
            return idx + 1
    for idx in range(start, end):
        if re.search(r"(?:^|[,{]\s*)['\"]?uses['\"]?\s*:", lines[idx]):
            return idx + 1
    return start + 1


def _structured_step_actions(definition: Any, *, job_name: str) -> list[str]:
    """Extract only top-level step uses values from PyYAML's structure."""
    if not isinstance(definition, dict):
        raise WorkflowParseError(f"job '{job_name}' has no mapping definition")
    steps = definition.get("steps")
    if steps is None:
        return []
    if not isinstance(steps, list):
        raise WorkflowParseError(f"job '{job_name}' has a non-sequence steps value")
    actions: list[str] = []
    for position, step in enumerate(steps, start=1):
        if not isinstance(step, dict):
            raise WorkflowParseError(f"job '{job_name}' step {position} is not a mapping")
        if "uses" not in step:
            continue
        action = step["uses"]
        if not isinstance(action, str) or not action:
            raise WorkflowParseError(f"job '{job_name}' step {position} has a non-scalar uses value")
        actions.append(action)
    return actions


def _fallback_uses_action(value: str | None, *, line: int) -> str:
    """Return a fallback uses scalar, refusing references it cannot resolve."""
    if value is None or not value:
        raise WorkflowParseError(f"line {line}: empty step uses value")
    if value.startswith(("[", "{")):
        raise WorkflowParseError(f"line {line}: non-scalar step uses value")
    if re.match(r"^[&*][A-Za-z0-9_-]+(?:\s|$)", value):
        raise WorkflowParseError(f"line {line}: step uses an anchor/alias unavailable to the stdlib parser")
    return value


def _fallback_step_uses(lines: list[str], start: int, end: int) -> list[tuple[int, str]]:
    """Read step mappings structurally in the stdlib-only parser."""
    for idx in range(start, end):
        source = lines[idx]
        indentation = source[: len(source) - len(source.lstrip())]
        raw_line = indentation + _strip_trailing_comment(source).lstrip()
        match = re.fullmatch(r" {4}steps:\s*(.*?)\s*", raw_line)
        if not match:
            continue
        raw_steps = match.group(1)
        if raw_steps:
            if not raw_steps.startswith("[") or not raw_steps.endswith("]"):
                raise WorkflowParseError(f"line {idx + 1}: unsupported non-sequence steps value")
            found: list[tuple[int, str]] = []
            for step in _split_flow_items(raw_steps, line=idx + 1):
                for key, value in _fallback_flow_mapping_pairs(step, line=idx + 1):
                    if key == "uses":
                        found.append((idx + 1, _fallback_uses_action(value, line=idx + 1)))
            return found

        found = []
        in_step = False
        for child in range(idx + 1, end):
            source = lines[child]
            indentation_prefix = source[: len(source) - len(source.lstrip())]
            nested = indentation_prefix + _strip_trailing_comment(source).lstrip()
            if not _non_comment(nested):
                continue
            indentation = _indent(nested)
            if indentation <= 4:
                break
            if indentation == 6:
                item = re.fullmatch(r" {6}-\s*(.*?)\s*", nested)
                if not item:
                    raise WorkflowParseError(f"line {child + 1}: malformed block step")
                in_step = True
                inline = item.group(1)
                if inline.startswith("{"):
                    for key, value in _fallback_flow_mapping_pairs(inline, line=child + 1):
                        if key == "uses":
                            found.append((child + 1, _fallback_uses_action(value, line=child + 1)))
                    continue
                uses = re.fullmatch(r"uses:\s*(.*?)\s*", inline)
                if uses:
                    found.append(
                        (
                            child + 1,
                            _fallback_uses_action(
                                _fallback_scalar(uses.group(1), line=child + 1), line=child + 1
                            ),
                        )
                    )
                continue
            if indentation == 8 and in_step:
                sibling = re.fullmatch(r" {8}uses:\s*(.*?)\s*", nested)
                if sibling:
                    found.append(
                        (
                            child + 1,
                            _fallback_uses_action(
                                _fallback_scalar(sibling.group(1), line=child + 1), line=child + 1
                            ),
                        )
                    )
        return found
    return []


def _step_uses(lines: list[str], job: Job) -> list[tuple[int, str]]:
    """Return actual step-level uses keys in both parser implementations."""
    if job.definition is None:
        return _fallback_step_uses(lines, job.start + 1, job.end)
    return [
        (_source_line_for_uses(lines, job.start + 1, job.end, action), action)
        for action in _structured_step_actions(job.definition, job_name=job.name)
    ]


def _scan(workflows: pathlib.Path) -> tuple[int, list[Finding]]:
    """Scan one workflow tree, retaining source locations for diagnostics."""
    if not workflows.is_dir():
        raise WorkflowParseError(f"{workflows}: workflow directory is missing")

    findings: list[Finding] = []
    self_hosted_jobs = 0
    seen_jobs: set[tuple[str, str]] = set()

    for path in sorted(workflows.glob("*.yml")):
        jobs = _load_jobs(path)
        lines = path.read_text(encoding="utf-8").splitlines()
        for job in jobs:
            if not job_is_self_hosted(job):
                continue
            if (path.name, job.name) not in seen_jobs:
                seen_jobs.add((path.name, job.name))
                self_hosted_jobs += 1
            step_uses = _step_uses(lines, job)
            for lineno, action in step_uses:
                if PROVISIONING_ACTIONS.search(action):
                    findings.append(
                        Finding(
                            workflow=f".github/workflows/{path.name}",
                            job=job.name,
                            rule="provisioning-action",
                            subject=action,
                            lineno=lineno,
                            source=f"uses: {action}",
                        )
                    )
            for lineno in range(job.start + 1, job.end + 1):
                line = lines[lineno - 1]
                if not line.strip() or line.lstrip().startswith("#"):
                    continue
                match = HARDCODED_CHANNEL.search(line)
                if match:
                    findings.append(
                        Finding(
                            workflow=f".github/workflows/{path.name}",
                            job=job.name,
                            rule="hardcoded-versioned-toolchain-path",
                            subject=match.group(0),
                            lineno=lineno,
                            source=line.strip(),
                        )
                    )

    if self_hosted_jobs == 0:
        raise WorkflowParseError(
            f"{workflows}: found ZERO self-hosted jobs. Either the fleet is gone or this script's "
            "parser broke — refusing to report success on a check that inspected nothing."
        )
    return self_hosted_jobs, findings


def _new_findings(head: list[Finding], baseline: list[Finding]) -> list[Finding]:
    """Return the multiset difference, so duplicate violations cannot hide."""
    inherited = Counter(finding.identity for finding in baseline)
    result: list[Finding] = []
    for finding in head:
        if inherited[finding.identity]:
            inherited[finding.identity] -= 1
        else:
            result.append(finding)
    return result


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--baseline-workflows",
        type=pathlib.Path,
        help="compare the candidate workflow tree to this baseline instead of requiring zero findings",
    )
    args = parser.parse_args(argv)

    try:
        self_hosted_jobs, findings = _scan(WORKFLOWS)
        baseline_jobs = None
        if args.baseline_workflows is not None:
            baseline_jobs, baseline_findings = _scan(args.baseline_workflows)
            findings = _new_findings(findings, baseline_findings)
    except WorkflowParseError as exc:
        print(f"::error::validate_no_shared_rustup_mutation: parser failure: {exc}", file=sys.stderr)
        return 2

    if findings:
        comparison = "new " if baseline_jobs is not None else ""
        baseline_note = f" (baseline: {baseline_jobs})" if baseline_jobs is not None else ""
        print(
            f"::error::validate_no_shared_rustup_mutation: {len(findings)} {comparison}violation(s) "
            f"across {self_hosted_jobs} self-hosted job(s){baseline_note}.",
            file=sys.stderr,
        )
        for finding in findings:
            print(f"  {finding.render()}", file=sys.stderr)
        return 1

    if baseline_jobs is not None:
        print(
            f"OK: {self_hosted_jobs} candidate and {baseline_jobs} baseline self-hosted job(s) "
            "inspected; no new toolchain provisioning or versioned toolchain paths."
        )
        return 0

    print(
        f"OK: {self_hosted_jobs} self-hosted job(s) inspected; none provisions a toolchain "
        f"or hardcodes a versioned toolchain path."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
