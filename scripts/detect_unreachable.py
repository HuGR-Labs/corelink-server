#!/usr/bin/env python3
"""detect_unreachable.py — find declared-but-unreached worker capabilities.

WHY THIS EXISTS
----------------
CoreLink has repeatedly shipped code that is built, deployed, and registered
in `wrangler.toml` but that no request path ever actually reaches — the
"built but unreachable" pattern. Two confirmed instances: the `EventLogDO`
Durable Object (bound in prod, called by nothing) and the gRPC REAPI service
implementations (compiled into the container binary, but no
`tonic::transport::Server` is ever bound). This script scans
`wrangler.toml` + `worker/src/` for bindings/env-vars/flags that are
declared but never referenced from TypeScript source, as a first-pass radar
for the same failure mode in the Worker layer.

THE FALSE-POSITIVE HAZARD (read before trusting a "dead" verdict)
-------------------------------------------------------------------
A naive `grep -c "env.FOO"` detector was tried here first and it produced a
CONFIRMED FALSE POSITIVE: `EDGE_DO_METER` looked dead by that method, but it
is actually read at worker/src/index.ts via a TYPE-CAST expression:

    (env as unknown as { EDGE_DO_METER?: string }).EDGE_DO_METER

A plain `env.EDGE_DO_METER` grep never matches that shape. Any detector that
only looks for `env.NAME` / `env["NAME"]` / `env['NAME']` will keep making
this exact mistake for every future flag that gets read through a cast,
an intermediate variable, a destructure, or a helper function that takes
`env` and does the lookup one level removed.

Because a wrong "this is dead" claim is strictly worse than a missed one
(it invites someone to delete live code), this tool is deliberately
CONSERVATIVE:

  * It matches a broad UNION of access patterns (see `BINDING_PATTERNS`
    below) — direct dot access, bracket access (both quote styles),
    destructuring (including renamed/nested destructuring of `env`), and
    cast/reassignment access via `as unknown as { NAME`.
  * It also matches a bare identifier occurrence of the binding name
    anywhere in worker/src (e.g. a helper that receives the value as a
    plain parameter after some upstream indirection, or a re-export). This
    is intentionally loose: a bare-name search will basically never produce
    a false "unreached" verdict, at the cost of occasionally missing a truly
    dead binding whose name happens to collide with prose. That tradeoff is
    the point of "prefer false negatives."
  * Every binding this tool DOES report as unreached prints the exact
    patterns it searched, so a human can grep the same patterns by hand and
    audit the negative before acting on it.
  * This script always exits 0. It is a report, not a CI gate, until the
    output has been trusted over time.

USAGE
-----
    python3 scripts/detect_unreachable.py [--json] [--repo-root PATH]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from dataclasses import dataclass, field
from typing import Optional


# ---------------------------------------------------------------------------
# TOML-lite parsing (stdlib only — no tomllib assumption pre-3.11, and we
# only need a narrow slice of wrangler.toml's shape anyway).
# ---------------------------------------------------------------------------

TABLE_HEADER_RE = re.compile(r"^\[\[?([^\]]+)\]\]?\s*(#.*)?$")
KV_RE = re.compile(r"""^([A-Za-z0-9_.-]+)\s*=\s*(.+?)\s*(#.*)?$""")


def strip_toml_string(raw: str) -> str:
    raw = raw.strip()
    if raw.startswith('"') and raw.endswith('"') and len(raw) >= 2:
        return raw[1:-1]
    if raw.startswith("'") and raw.endswith("'") and len(raw) >= 2:
        return raw[1:-1]
    return raw


def parse_inline_table(raw: str) -> dict:
    """Parse `{ A = "x", B = "y" }` inline tables (used by `vars = {...}`)."""
    raw = raw.strip()
    if not (raw.startswith("{") and raw.endswith("}")):
        return {}
    inner = raw[1:-1]
    out = {}
    # Split on commas that are not inside quotes.
    parts = []
    cur = ""
    in_quote = None
    for ch in inner:
        if in_quote:
            cur += ch
            if ch == in_quote:
                in_quote = None
        elif ch in ("'", '"'):
            in_quote = ch
            cur += ch
        elif ch == "," :
            parts.append(cur)
            cur = ""
        else:
            cur += ch
    if cur.strip():
        parts.append(cur)
    for part in parts:
        if "=" not in part:
            continue
        k, v = part.split("=", 1)
        out[k.strip()] = strip_toml_string(v.strip())
    return out


@dataclass
class WranglerModel:
    # list of (name, class_name, table_path) for every [[...durable_objects.bindings]]
    durable_objects: list = field(default_factory=list)
    # list of (binding, kind, table_path) for r2/d1/kv/service bindings
    resource_bindings: list = field(default_factory=list)
    # env-var name -> set of table_paths it was set in (top-level [vars] and
    # per-env vars = {...} inline tables, plus bare KEY = "val" at top level
    # inside [vars]/[env.X.vars] blocks)
    declared_vars: dict = field(default_factory=dict)


def parse_wrangler_toml(path: str) -> WranglerModel:
    model = WranglerModel()
    if not os.path.isfile(path):
        return model

    with open(path, "r", encoding="utf-8") as f:
        lines = f.readlines()

    current_table = ""
    for raw_line in lines:
        line = raw_line.rstrip("\n")
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue

        header_match = TABLE_HEADER_RE.match(stripped)
        if header_match:
            current_table = header_match.group(1)
            continue

        kv_match = KV_RE.match(stripped)
        if not kv_match:
            continue
        key, value, _comment = kv_match.groups()
        value = value.strip()

        # [[...durable_objects.bindings]] name = "..." / class_name = "..."
        if current_table.endswith("durable_objects.bindings"):
            if key == "name":
                model.durable_objects.append(
                    (strip_toml_string(value), current_table)
                )
            continue

        # [[...r2_buckets]] / [[...d1_databases]] / [[...kv_namespaces]] /
        # [[...services]] binding = "..."
        for kind, suffix in (
            ("r2_bucket", "r2_buckets"),
            ("d1_database", "d1_databases"),
            ("kv_namespace", "kv_namespaces"),
            ("service", "services"),
        ):
            if current_table.endswith(suffix) and key == "binding":
                model.resource_bindings.append(
                    (strip_toml_string(value), kind, current_table)
                )

        # [vars] / [env.X.vars] blocks: bare KEY = "value" assignments.
        if current_table == "vars" or current_table.endswith(".vars"):
            model.declared_vars.setdefault(key, set()).add(current_table)
            continue

        # inline `vars = { A = "x", B = "y" }` on an [env.X] table (e.g.
        # `[env.prod]` ... `vars = { ENVIRONMENT = "prod", ... }`).
        if key == "vars" and value.startswith("{"):
            inline = parse_inline_table(value)
            for k in inline:
                model.declared_vars.setdefault(k, set()).add(
                    current_table + ".vars(inline)"
                )

    return model


# ---------------------------------------------------------------------------
# Env interface parsing (worker/src/index.ts)
# ---------------------------------------------------------------------------

ENV_MEMBER_RE = re.compile(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\??\s*:\s*")


def parse_env_interface(index_ts_path: str) -> list:
    """Return the list of member names declared on `export interface Env {...}`."""
    if not os.path.isfile(index_ts_path):
        return []
    with open(index_ts_path, "r", encoding="utf-8") as f:
        text = f.read()

    match = re.search(r"export interface Env\s*\{", text)
    if not match:
        return []

    start = match.end()
    depth = 1
    i = start
    while i < len(text) and depth > 0:
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
        i += 1
    body = text[start : i - 1]

    members = []
    for line in body.splitlines():
        m = ENV_MEMBER_RE.match(line)
        if m:
            members.append(m.group(1))
    return members


# ---------------------------------------------------------------------------
# Reachability search over worker/src
# ---------------------------------------------------------------------------


def build_patterns(name: str) -> list:
    """Every access-pattern regex searched for `name`. Order = human-readable
    description of the pattern, for the audit printout."""
    esc = re.escape(name)
    return [
        (f"env.{name}", re.compile(r"\benv\." + esc + r"\b")),
        (f'env["{name}"]', re.compile(r"""\benv\[\s*["']""" + esc + r"""["']\s*\]""")),
        (
            f"destructure `{{ {name} }} = env` (any brace distance/rename-adjacent)",
            re.compile(
                r"\{[^{}]*\b" + esc + r"\b[^{}]*\}\s*(?::\s*[^=]+)?=\s*(?:this\.)?env\b"
            ),
        ),
        (
            f"type-cast access `as ... {{ {name}",
            re.compile(r"as\s+unknown\s+as\s*\{[^}]*\b" + esc + r"\b"),
        ),
        (
            f"bare identifier `{name}` anywhere in worker/src",
            re.compile(r"\b" + esc + r"\b"),
        ),
    ]


def strip_comments_preserve_lines(text: str) -> str:
    """Blank out `//...` and `/*...*/` comment content (both JSDoc blocks and
    line comments) while preserving line numbers and non-comment code, so
    prose that merely NAMES a binding (design notes, "Status: bound but
    unused" annotations, etc.) can never masquerade as a reaching reference.
    This is intentionally simple (no string-literal awareness for `//` /
    `/*` inside strings) — the rare case where that miscounts a real string
    literal as a comment only makes the tool MORE conservative (more
    REACHED, never less), which matches "prefer false negatives"."""
    out = []
    i = 0
    n = len(text)
    in_block = False
    while i < n:
        if in_block:
            end = text.find("*/", i)
            if end == -1:
                out.append("\n" * text.count("\n", i))
                i = n
                break
            out.append("\n" * text.count("\n", i, end))
            i = end + 2
            in_block = False
            continue
        two = text[i : i + 2]
        if two == "/*":
            in_block = True
            i += 2
            continue
        if two == "//":
            nl = text.find("\n", i)
            if nl == -1:
                i = n
                break
            i = nl  # keep the newline itself
            continue
        out.append(text[i])
        i += 1
    return "".join(out)


def scan_worker_src(worker_src_dir: str) -> dict:
    """Return {filepath: comment-stripped_text} for every .ts file under
    worker/src. Comments are stripped (see strip_comments_preserve_lines) so
    a binding merely being NAMED in a doc-comment doesn't count as a
    reaching reference — line numbers are preserved for accurate reporting."""
    files = {}
    for root, _dirs, filenames in os.walk(worker_src_dir):
        for fn in filenames:
            if fn.endswith(".ts") or fn.endswith(".tsx"):
                p = os.path.join(root, fn)
                try:
                    with open(p, "r", encoding="utf-8", errors="replace") as f:
                        raw = f.read()
                except OSError:
                    continue
                files[p] = strip_comments_preserve_lines(raw)
    return files


def find_reaching_refs(
    name: str,
    source_files: dict,
    index_ts_path: str,
    skip_declaration_line_in_index: bool = False,
) -> dict:
    """Search all patterns for `name`. Returns
    {pattern_desc: [ "path:lineno", ... ]} for every pattern that hit at
    least once, excluding the bare Env-interface declaration line itself
    (so declaring a member doesn't count as "reading" it)."""
    patterns = build_patterns(name)
    hits = {}
    for desc, regex in patterns:
        found_here = []
        for path, text in source_files.items():
            for lineno, line in enumerate(text.splitlines(), start=1):
                if not regex.search(line):
                    continue
                if (
                    skip_declaration_line_in_index
                    and path == index_ts_path
                    and re.match(r"^\s*" + re.escape(name) + r"\??\s*:", line)
                ):
                    continue
                # Also skip pure wrangler.toml doc-comment mirrors — N/A here
                # since we only scan .ts files.
                found_here.append(f"{path}:{lineno}")
        if found_here:
            hits[desc] = found_here
    return hits


# ---------------------------------------------------------------------------
# Report assembly
# ---------------------------------------------------------------------------


@dataclass
class Finding:
    category: str
    name: str
    status: str  # "REACHED" or "UNREACHED"
    detail: str
    patterns_searched: list
    evidence: dict


def run(repo_root: str) -> dict:
    wrangler_path = os.path.join(repo_root, "wrangler.toml")
    worker_src_dir = os.path.join(repo_root, "worker", "src")
    index_ts_path = os.path.join(worker_src_dir, "index.ts")

    model = parse_wrangler_toml(wrangler_path)
    env_members = parse_env_interface(index_ts_path)
    source_files = scan_worker_src(worker_src_dir)

    findings = []

    # 1. Durable Object bindings (dedup by name across per-env blocks).
    do_names = sorted({n for n, _table in model.durable_objects})
    for name in do_names:
        patterns_desc = [d for d, _r in build_patterns(name)]
        evidence = find_reaching_refs(
            name, source_files, index_ts_path, skip_declaration_line_in_index=True
        )
        status = "REACHED" if evidence else "UNREACHED"
        findings.append(
            Finding(
                category="durable_object_binding",
                name=name,
                status=status,
                detail="wrangler.toml [[durable_objects.bindings]] name",
                patterns_searched=patterns_desc,
                evidence=evidence,
            )
        )

    # 2. Env interface members (declared-in-Env-but-never-read).
    for name in env_members:
        patterns_desc = [d for d, _r in build_patterns(name)]
        evidence = find_reaching_refs(
            name, source_files, index_ts_path, skip_declaration_line_in_index=True
        )
        status = "REACHED" if evidence else "UNREACHED"
        findings.append(
            Finding(
                category="env_interface_member",
                name=name,
                status=status,
                detail="declared on `export interface Env` in worker/src/index.ts",
                patterns_searched=patterns_desc,
                evidence=evidence,
            )
        )

    # 3. wrangler.toml vars (set-in-wrangler-but-never-read) — separate label
    #    per the spec, even though the search method is identical.
    for name, tables in sorted(model.declared_vars.items()):
        patterns_desc = [d for d, _r in build_patterns(name)]
        evidence = find_reaching_refs(
            name, source_files, index_ts_path, skip_declaration_line_in_index=True
        )
        status = "REACHED" if evidence else "UNREACHED"
        findings.append(
            Finding(
                category="wrangler_var",
                name=name,
                status=status,
                detail=f"set via wrangler.toml vars in: {', '.join(sorted(tables))}",
                patterns_searched=patterns_desc,
                evidence=evidence,
            )
        )

    # 4. R2 / D1 / KV / service bindings.
    seen_resource = set()
    for binding, kind, _table in model.resource_bindings:
        key = (binding, kind)
        if key in seen_resource:
            continue
        seen_resource.add(key)
        patterns_desc = [d for d, _r in build_patterns(binding)]
        evidence = find_reaching_refs(
            binding, source_files, index_ts_path, skip_declaration_line_in_index=True
        )
        status = "REACHED" if evidence else "UNREACHED"
        findings.append(
            Finding(
                category=f"{kind}_binding",
                name=binding,
                status=status,
                detail=f"wrangler.toml [[{kind}s]] binding" if not kind.endswith("s") else f"wrangler.toml [[{kind}]] binding",
                patterns_searched=patterns_desc,
                evidence=evidence,
            )
        )

    return {
        "repo_root": repo_root,
        "wrangler_toml": wrangler_path,
        "worker_src_scanned_files": len(source_files),
        "findings": [f.__dict__ for f in findings],
    }


def print_human_report(result: dict) -> None:
    findings = result["findings"]
    unreached = [f for f in findings if f["status"] == "UNREACHED"]
    reached = [f for f in findings if f["status"] == "REACHED"]

    print("=" * 78)
    print("detect_unreachable.py — declared-but-unreached capability report")
    print("=" * 78)
    print(f"repo root:            {result['repo_root']}")
    print(f"wrangler.toml:        {result['wrangler_toml']}")
    print(f"worker/src files:     {result['worker_src_scanned_files']}")
    print(f"total items checked:  {len(findings)}")
    print(f"REACHED:              {len(reached)}")
    print(f"UNREACHED:            {len(unreached)}")
    print()

    if not unreached:
        print("No declared-but-unreached items found.")
    else:
        print("-" * 78)
        print("UNREACHED — audit these by hand before deleting anything")
        print("-" * 78)
        for f in unreached:
            print(f"\n[{f['category']}] {f['name']}  ->  UNREACHED")
            print(f"    source: {f['detail']}")
            print("    patterns searched (all came back empty):")
            for p in f["patterns_searched"]:
                print(f"      - {p}")

    print()
    print("-" * 78)
    print("REACHED (for reference)")
    print("-" * 78)
    for f in reached:
        first_pattern = next(iter(f["evidence"]), None)
        n_hits = sum(len(v) for v in f["evidence"].values())
        print(
            f"  [{f['category']}] {f['name']}  ->  REACHED "
            f"({n_hits} hit(s), e.g. via: {first_pattern})"
        )

    print()
    print("NOTE: this tool prefers false negatives to false positives. An")
    print("UNREACHED verdict here is a lead to investigate by hand, not proof")
    print("of dead code — see the module docstring for the EDGE_DO_METER")
    print("false-positive case this design was built to avoid.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo-root",
        default=os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        help="Repo root containing wrangler.toml and worker/src (default: parent of scripts/)",
    )
    parser.add_argument(
        "--json", action="store_true", help="Emit machine-readable JSON instead of the human report."
    )
    args = parser.parse_args()

    result = run(args.repo_root)

    if args.json:
        print(json.dumps(result, indent=2))
    else:
        print_human_report(result)

    return 0  # always 0 — report, not a gate.


if __name__ == "__main__":
    sys.exit(main())
