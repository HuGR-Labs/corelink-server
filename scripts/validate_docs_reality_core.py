#!/usr/bin/env python3
"""
Validate that CUSTOMER-FACING claims (product docs + marketing) are backed by
REAL, shipped behaviour — the anti-drift gate for the surfaces the OKF wiki does
NOT cover.

WHY THIS EXISTS
---------------
`docs/knowledge/` (OKF) is anti-drift-gated against code (validate_okf.py, C5 =
citation stability). But OKF has ZERO coverage of `apps/docs/` (the product docs
site), `marketing/`, or `tools/cli/` — so customer-facing claims silently drift
from what actually ships. Real examples this gate is built to make impossible:

  * `corelink bazel-init` is documented (tutorial) + referenced in e2e, but is NOT
    in the CLI `enum Commands` — it does not exist.
  * The quickstart tells users to compute a SHA-256 digest (`sha256sum`) and upload
    it, but the CAS plane addresses + verifies blobs by BLAKE3 (CTRL-CAS-002).
  * Marketing sells "BYOK across 4 KMS providers" as an available capability while
    the OKF byok concept records the prod KmsProvider wiring as DEFERRED / inert.
  * The gate checked documented PATHS against wired routes but had no hostname or
    DNS dimension at all — so it stayed green on main while the deployed docs site
    served NXDOMAIN hostnames on 9+ live pages, `README.md` sent new users to a
    dead host, `openapi/corelink-v1.yaml` pointed termsOfService/contact/license at
    a dead host, and 6 regulator-facing breach-notification templates promised
    post-mortems on a dead host. A resolvable path behind an unresolvable host is
    a 404 for the customer either way. See [hostname-liveness].

This is a CURATED, LOW-FALSE-POSITIVE gate. It reads claims ONLY from CODE
CONTEXTS (shell fenced blocks + inline-code spans + HTML <code>/<pre>) so prose
("CoreLink is a cache", "a corelink mirror caches metadata") is never mistaken for
a command, and it skips non-shell fenced blocks so a Python `from corelink import
CoreLinkClient` is never mistaken for a `corelink import` subcommand. A maintained
allowlist (`scripts/docs_reality_allowlist.json`) carries two buckets so the gate
ships GREEN on the current tree yet stays MEANINGFUL:

  * roadmap_allow  — intentional forward-looking / other-product references
                     (e.g. `corelink workspace` = the separate Workspaces product,
                     `corelink ci` = the post-launch build-acceleration campaign).
                     Never a failure.
  * tracked_drift  — KNOWN drift being removed/fixed by in-flight PRs. Non-fatal in
                     the normal gate (so a green tree stays green while the fix PRs
                     land) but FAILS under `--strict` — this is the punch-list, and
                     it is exactly what "would this gate have caught bazel-init?"
                     means: run `--strict` and it does.

Any CLI reference that is neither valid, nor roadmap-allowed, nor tracked -> FAIL.
That is the load-bearing property: NEW drift (a freshly-documented command that
does not exist) fails the gate the moment it is introduced.

CHECKS
------
  [cli-existence]            Every `corelink <subcommand>` referenced in a code
                            context under the doc roots MUST exist in the CLI
                            `enum Commands` (parsed live from tools/cli/src/main.rs),
                            including the two-level group actions (`audit export`,
                            `ac put`, `config set`, ...). FAIL on an unknown command.
  [okf-deferred-coherence]  Curated rules (grounded in an OKF concept or the CLI
                            source) that flag customer-facing text asserting a
                            DEFERRED/unbuilt capability as if it were live, or
                            contradicting a canonical code fact (e.g. the CAS digest
                            algorithm). Each rule RE-VERIFIES its OKF grounding, so a
                            rule auto-retires (WARN: stale) once the capability ships
                            and the OKF concept drops the DEFERRED marker.
  [suppression-hygiene]     WARN (non-fatal) for every allowlist key — in either
                            CLI bucket — that suppressed NOTHING on this run. A
                            suppression with no expiry is a permanent mute; this
                            makes a dead one visible so it is deleted rather than
                            left armed to re-silence the same defect on recurrence.
  [endpoint-existence]      Best-effort: HTTP paths named in onboarding recipes
                            (`/v1/cas/...`, `/bazel/v2/...`, `/turbo/...`,
                            `grpcs://...`) should resolve to a wired route (the
                            container `.route(`/`.nest(` table + the worker route
                            literals). Unresolved -> WARN (or FAIL for a flagship
                            quickstart listed in the allowlist).
  [hostname-liveness]       Every `*.humangr.com` hostname printed on a SHIPPED
                            surface (docs site incl. i18n, README, marketing,
                            openapi, sdks, examples, templates, legal, corelink-go,
                            tools, and the live `src/` trees) MUST be on the
                            checked-in live-host allowlist. A path check resolving
                            to a wired route proves nothing if the HOST in front of
                            it is NXDOMAIN — this dimension closes that gap. Static
                            comparison only: NO DNS/HTTP at CI time (see
                            `--verify-dns`).

Usage:
    python3 scripts/validate_docs_reality.py                 # normal gate (CI)
    python3 scripts/validate_docs_reality.py --strict        # also fail tracked_drift
    python3 scripts/validate_docs_reality.py --list-refs      # dump every CLI ref found
    python3 scripts/validate_docs_reality.py --list-hosts     # dump every hostname found
    python3 scripts/validate_docs_reality.py --verify-dns     # MANUAL: re-derive the
                                                              # live-host allowlist
    python3 scripts/validate_docs_reality.py --allowlist <path>

Exit contract (mirrors validate_okf.py / validate_specs.py):
    0        -> "OK docs-reality: <N> CLI refs, <T> tracked, <W> warnings, 0 drift"
    non-zero -> per-check offender list, then
                "DOCS-REALITY INVALID: <k> failures"

Dependencies: Python stdlib only (json + re + argparse). No PyYAML required.
"""

from __future__ import annotations

import argparse
import datetime
import json
import re
import sys
from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path

# ---------------------------------------------------------------------------
# Repo layout
# ---------------------------------------------------------------------------
REPO_ROOT = Path(__file__).resolve().parent.parent
CLI_MAIN = REPO_ROOT / "tools" / "cli" / "src" / "main.rs"
OKF_ROOT = REPO_ROOT / "docs" / "knowledge"
DEFAULT_ALLOWLIST = REPO_ROOT / "scripts" / "docs_reality_allowlist.json"

# Customer-facing documentation + marketing roots scanned for claims.
# `docs/knowledge` (the internal, separately-gated OKF wiki) and the generated
# `docs/okf-wiki-site` are EXCLUDED — they are the source of truth, not a claim.
DOC_ROOTS = [
    REPO_ROOT / "apps" / "docs" / "docs",
    REPO_ROOT / "apps" / "docs" / "blog",
    REPO_ROOT / "apps" / "docs" / "src",
    REPO_ROOT / "marketing",
    # `legal/` was scanned for HOSTNAMES but not for claims, and that split cost
    # us: the TLS-floor claim survived a full sweep inside a DPA, an SCC annex,
    # three privacy notices and two regulator breach-notification templates,
    # because no coherence rule could see them. A document filed with a data
    # protection authority is the last place a stale control claim should live.
    REPO_ROOT / "legal",
]
DOC_ROOTS_WITH_EXCLUDES = [
    # (root, [excluded subdirs relative to repo root]) — `docs/**` minus the
    # internal wiki + its generated site + internal design notes.
    (REPO_ROOT / "docs", ["docs/knowledge", "docs/okf-wiki-site", "docs/internal"]),
]

# Repo-root markdown, scanned NON-recursively (the recursive roots above already
# cover every subtree we care about; rglob from REPO_ROOT would re-walk the whole
# repo). These files are outside every root above and yet include the two most-read
# documents in the project — `README.md` and the roadmaps — so a phantom command
# here reaches more readers than one buried in a tutorial. `ROADMAP-TO-LAUNCH.md`
# was carrying `corelink ping` (a subcommand that has never existed) and a
# `get.corelink.io` one-liner (a domain we do not own) precisely because nothing
# looked here.
#
# `CHANGELOG.md` is EXCLUDED, and the exclusion is load-bearing rather than
# convenient: it is a historical ledger whose entries DESCRIBE defects, so it
# legitimately quotes commands that do not exist — 9 mentions of `corelink ping`
# today, every one of them narrating the fix that removed it. Scanning it would
# make the gate red for recording history accurately, which is the fastest way to
# get a gate allowlisted into uselessness (see `scripts/docs_reality_allowlist.json`).
ROOT_DOC_GLOBS = ["*.md"]
ROOT_DOC_EXCLUDE = {"CHANGELOG.md"}
# Route sources for the best-effort endpoint check.
# Only the composed container router and the edge dispatcher can prove a
# customer-facing endpoint.  Adapter-local routers are intentionally not
# scanned as global roots: their paths are meaningful only after a mount prefix
# (for example npm's `/{pkg}` lives under `/npm`).
ROUTE_SOURCE_ROOTS = [
    REPO_ROOT / "crates" / "corelink-container" / "src" / "routes",
    REPO_ROOT / "crates" / "corelink-container" / "src" / "main.rs",
    REPO_ROOT / "worker" / "src",
]

DOC_EXTS = {".md", ".mdx", ".mdc", ".html", ".htm", ".txt"}
WALK_EXCLUDE_DIRS = {"node_modules", ".git", "build", "dist", ".open-next",
                     ".wrangler", "target", ".docusaurus"}

# Fenced-code info-strings that are SHELL (a `corelink ...` line is a CLI
# invocation). Any OTHER language (python, ts, rust, json, ...) is skipped for CLI
# extraction so an SDK snippet — `from corelink import CoreLinkClient` — is never
# mistaken for a `corelink import` subcommand.
SHELL_FENCE_LANGS = {"", "bash", "sh", "shell", "shell-session", "console",
                     "zsh", "text", "sh-session", "terminal"}


class SourceSyntaxError(ValueError):
    """A source comment or literal is incomplete for static inspection."""


def _raw_string_start(source: str, index: int) -> tuple[int, int] | None:
    """Return (opening quote, hash count) for a Rust raw string."""
    if source.startswith("br", index):
        prefix_end = index + 2
    elif source.startswith("r", index):
        prefix_end = index + 1
    else:
        return None
    hashes = 0
    while prefix_end + hashes < len(source) and source[prefix_end + hashes] == "#":
        hashes += 1
    quote = prefix_end + hashes
    if quote >= len(source) or source[quote] != '"':
        return None
    return quote, hashes


def _blank_comment(value: str) -> str:
    """Replace comment text without changing line positions."""
    return "".join("\n" if character == "\n" else " " for character in value)


def _strip_source_comments(source: str, *, nested: bool = True) -> str:
    """Remove Rust/Worker comments while preserving all string literals.

    Rust block comments nest; Worker comments do not, but accepting nesting is
    harmless and keeps a malformed fixture from becoming evidence.  An
    unterminated comment or literal raises instead of returning a permissive
    partial source surface.
    """
    output: list[str] = []
    index = 0
    length = len(source)
    while index < length:
        if source.startswith("//", index):
            output.extend((" ", " "))
            index += 2
            while index < length and source[index] != "\n":
                output.append(" ")
                index += 1
            continue
        if source.startswith("/*", index):
            output.extend((" ", " "))
            index += 2
            depth = 1
            while index < length and depth:
                if nested and source.startswith("/*", index):
                    output.extend((" ", " "))
                    index += 2
                    depth += 1
                elif source.startswith("*/", index):
                    output.extend((" ", " "))
                    index += 2
                    depth -= 1
                elif source[index] == "\n":
                    output.append("\n")
                    index += 1
                else:
                    end = index
                    while end < length and source[end] not in "/*\n":
                        end += 1
                    if end == index:
                        output.append(" ")
                        index += 1
                    else:
                        output.append(_blank_comment(source[index:end]))
                        index = end
            if depth:
                raise SourceSyntaxError("unterminated block comment")
            continue

        if source[index] in {"r", "b"}:
            raw = _raw_string_start(source, index)
            if raw is not None:
                quote, hashes = raw
                closing = '"' + ("#" * hashes)
                end = source.find(closing, quote + 1)
                if end < 0:
                    raise SourceSyntaxError("unterminated raw string")
                output.append(source[index : end + len(closing)])
                index = end + len(closing)
                continue

        quote = source[index]
        if quote in {'"', "`"} or (quote == "b" and index + 1 < length and source[index + 1] in {'"', "'"}):
            if quote == "b":
                output.append(quote)
                index += 1
                quote = source[index]
            output.append(quote)
            index += 1
            terminated = False
            while index < length:
                character = source[index]
                output.append(character)
                index += 1
                if character == "\\" and index < length:
                    output.append(source[index])
                    index += 1
                elif character == quote:
                    terminated = True
                    break
            if not terminated:
                raise SourceSyntaxError("unterminated string literal")
            continue

        if quote == "'":
            # Rust lifetimes (`'a`) are not character literals.  A closing
            # quote on this line proves the latter; otherwise copy the tick.
            end = index + 1
            escaped = False
            while end < length and source[end] != "\n":
                character = source[end]
                if character == "'" and not escaped:
                    break
                if character == "\\" and not escaped:
                    escaped = True
                else:
                    escaped = False
                end += 1
            if end < length and source[end] == "'":
                output.append(source[index : end + 1])
                index = end + 1
                continue

        output.append(source[index])
        index += 1
    return "".join(output)


# ---------------------------------------------------------------------------
# CLI command model — parsed LIVE from tools/cli/src/main.rs
# ---------------------------------------------------------------------------
def _camel_to_kebab(name: str) -> str:
    """`RunbookDrill` -> `runbook-drill`, `VerifyNdjson` -> `verify-ndjson`,
    `Ls` -> `ls`. Clap's default rename_all for subcommands is kebab-case."""
    out = []
    for i, ch in enumerate(name):
        if ch.isupper() and i > 0:
            out.append("-")
        out.append(ch.lower())
    return "".join(out)


_ENUM_RE = re.compile(r"enum\s+(\w+)\s*\{")
# A top-level variant line: 4-space-indented `Ident` optionally followed by `{` or
# `,` (skips `//`, attributes `#[...]`, and non-variant lines).
_VARIANT_RE = re.compile(r"^\s{4}([A-Z]\w*)\s*(\{|,|$)")
_ACTION_FIELD_RE = re.compile(r"action:\s*(\w+)")


@dataclass
class CliModel:
    top: set[str] = field(default_factory=set)          # kebab top-level commands
    groups: dict[str, set[str]] = field(default_factory=dict)  # group -> {actions}

    def is_valid_top(self, cmd: str) -> bool:
        return cmd in self.top

    def is_group(self, cmd: str) -> bool:
        return cmd in self.groups

    def is_valid_action(self, group: str, action: str) -> bool:
        return action in self.groups.get(group, set())


def parse_cli_model(main_rs: Path) -> CliModel:
    """Parse `enum Commands` (top-level) + every `*Action` sub-enum, and the
    `action: <Enum>` field wiring that links a group command to its action enum.
    Robust to new commands: everything is derived from the source, nothing hard-
    coded."""
    text = _strip_source_comments(main_rs.read_text(encoding="utf-8"))
    lines = text.splitlines()

    # 1. Split into brace-balanced enum bodies keyed by enum name.
    enum_bodies: dict[str, str] = {}
    i = 0
    while i < len(lines):
        m = _ENUM_RE.search(lines[i])
        if not m:
            i += 1
            continue
        name = m.group(1)
        depth = lines[i].count("{") - lines[i].count("}")
        body: list[str] = []
        i += 1
        while i < len(lines) and depth > 0:
            depth += lines[i].count("{") - lines[i].count("}")
            if depth > 0:
                body.append(lines[i])
            i += 1
        enum_bodies[name] = "\n".join(body)

    # 2. Variant identifiers per enum (only top-indented variant lines).
    def variants(body: str) -> list[str]:
        out = []
        for ln in body.splitlines():
            vm = _VARIANT_RE.match(ln)
            if vm and vm.group(1) not in ("Self",):
                out.append(vm.group(1))
        return out

    model = CliModel()
    commands_body = enum_bodies.get("Commands", "")

    top_variants = variants(commands_body)
    for v in top_variants:
        model.top.add(_camel_to_kebab(v))

    # Map top-level variant -> its action enum by scanning each variant body for
    # `action: <Enum>` and associating with the nearest preceding variant.
    cur_variant = None
    for ln in commands_body.splitlines():
        vm = _VARIANT_RE.match(ln)
        if vm:
            cur_variant = vm.group(1)
        am = _ACTION_FIELD_RE.search(ln)
        if am and cur_variant:
            action_enum = am.group(1)
            group_kebab = _camel_to_kebab(cur_variant)
            acts = {_camel_to_kebab(a) for a in variants(enum_bodies.get(action_enum, ""))}
            if acts:
                model.groups[group_kebab] = acts
    return model


# ---------------------------------------------------------------------------
# Doc corpus
# ---------------------------------------------------------------------------
@dataclass
class DocFile:
    path: Path
    rel: str
    text: str


def _iter_doc_files() -> list[DocFile]:
    seen: set[Path] = set()
    out: list[DocFile] = []

    def _walk(root: Path, excludes: list[str]):
        if not root.exists():
            return
        for p in sorted(root.rglob("*")):
            if not p.is_file() or p.suffix.lower() not in DOC_EXTS:
                continue
            if any(part in WALK_EXCLUDE_DIRS for part in p.parts):
                continue
            rel = p.relative_to(REPO_ROOT).as_posix()
            if any(rel.startswith(ex + "/") or rel == ex for ex in excludes):
                continue
            if p in seen:
                continue
            seen.add(p)
            try:
                out.append(DocFile(p, rel, p.read_text(encoding="utf-8")))
            except (OSError, UnicodeDecodeError):
                continue

    for root in DOC_ROOTS:
        _walk(root, [])
    for root, excludes in DOC_ROOTS_WITH_EXCLUDES:
        _walk(root, excludes)

    # Repo-root markdown, non-recursively (see ROOT_DOC_GLOBS above).
    for pattern in ROOT_DOC_GLOBS:
        for p in sorted(REPO_ROOT.glob(pattern)):
            if not p.is_file() or p.suffix.lower() not in DOC_EXTS:
                continue
            if p.name in ROOT_DOC_EXCLUDE or p in seen:
                continue
            seen.add(p)
            try:
                out.append(DocFile(p, p.relative_to(REPO_ROOT).as_posix(),
                                   p.read_text(encoding="utf-8")))
            except (OSError, UnicodeDecodeError):
                continue
    return out


# ---------------------------------------------------------------------------
# CLI-reference extraction (code contexts only — the low-false-positive core)
# ---------------------------------------------------------------------------
_FENCE_RE = re.compile(r"^(\s*)(`{3,}|~{3,})\s*([A-Za-z0-9_+-]*)")
_INLINE_CODE_RE = re.compile(r"`([^`\n]+)`")
_HTML_CODE_RE = re.compile(r"<(?:code|pre)[^>]*>(.*?)</(?:code|pre)>",
                           re.IGNORECASE | re.DOTALL)
_PROMPT_RE = re.compile(r"^\s*(?:\$|>|#\s|PS>|C:\\[^>]*>)\s*")
_CORELINK_CMD_RE = re.compile(r"^corelink\s+([a-z][a-z0-9-]*)(?:\s+([a-z][a-z0-9-]*))?")


@dataclass
class CliRef:
    file: str
    line: int
    raw: str
    cmd: str
    action: str | None


def _parse_corelink_line(s: str) -> tuple[str, str | None] | None:
    """From a shell-ish string, return (subcommand, action_or_None) if it is a
    `corelink <subcommand> ...` invocation, else None. Strips a shell prompt and
    tolerates a leading `sudo`/`env VAR=..` prefix."""
    s = s.strip()
    s = _PROMPT_RE.sub("", s).strip()
    if s.startswith("sudo "):
        s = s[5:].strip()
    m = _CORELINK_CMD_RE.match(s)
    if not m:
        return None
    return m.group(1), m.group(2)


def _strip_html(fragment: str) -> str:
    return re.sub(r"<[^>]+>", "", fragment)


def extract_cli_refs(doc: DocFile) -> list[CliRef]:
    """Collect `corelink <cmd>` references from CODE CONTEXTS only:
      * shell fenced blocks (``` / ~~~ with a shell/blank info-string),
      * inline `code` spans whose content starts with `corelink `,
      * HTML <code>/<pre> blocks.
    Prose is ignored (never in a code context); non-shell fences are skipped so an
    SDK import is not mistaken for a CLI command."""
    refs: list[CliRef] = []
    lines = doc.text.splitlines()
    is_html = doc.path.suffix.lower() in (".html", ".htm")

    # --- fenced code blocks (markdown) ---
    in_fence = False
    fence_marker = ""
    fence_is_shell = False
    for idx, ln in enumerate(lines, start=1):
        fm = _FENCE_RE.match(ln)
        if fm and (not in_fence or ln.strip().startswith(fence_marker)):
            marker = fm.group(2)
            if not in_fence:
                in_fence = True
                fence_marker = marker[0] * 3
                lang = (fm.group(3) or "").lower()
                fence_is_shell = lang in SHELL_FENCE_LANGS
                continue
            else:
                in_fence = False
                fence_is_shell = False
                continue
        if in_fence and fence_is_shell:
            for piece in re.split(r"&&|\|\||;|\|", ln):
                parsed = _parse_corelink_line(piece)
                if parsed:
                    refs.append(CliRef(doc.rel, idx, ln.strip(), parsed[0], parsed[1]))

    # --- inline code spans (markdown + any file) ---
    for idx, ln in enumerate(lines, start=1):
        for span in _INLINE_CODE_RE.findall(ln):
            span = span.strip()
            if span.startswith("corelink "):
                parsed = _parse_corelink_line(span)
                if parsed:
                    refs.append(CliRef(doc.rel, idx, span, parsed[0], parsed[1]))

    # --- HTML <code>/<pre> blocks ---
    if is_html:
        for m in _HTML_CODE_RE.finditer(doc.text):
            inner = _strip_html(m.group(1))
            line_no = doc.text[: m.start()].count("\n") + 1
            for piece in re.split(r"&&|\|\||;|\n", inner):
                parsed = _parse_corelink_line(piece)
                if parsed:
                    refs.append(CliRef(doc.rel, line_no, piece.strip()[:80],
                                       parsed[0], parsed[1]))

    uniq: dict[tuple, CliRef] = {}
    for r in refs:
        uniq[(r.file, r.line, r.cmd, r.action)] = r
    return list(uniq.values())


# ---------------------------------------------------------------------------
# Route inventory (best-effort endpoint check)
# ---------------------------------------------------------------------------
_ROUTE_DECL_RE = re.compile(
    r"\.(?P<kind>route|nest|nest_service)\(\s*\"(?P<path>[^\"]+)\"")
# Worker route literals + bare "/path" table entries. Best-effort: any string
# literal that looks like an absolute product route.
_WORKER_PATH_RE = re.compile(r"[\"'](/(?:v1|v2|turbo|bazel|npm|nix|cache|cas|ac|"
                             r"sccache|pip|cargo|brew|pkg|simple|token|internal|"
                             r"_internal|healthz|_health)[A-Za-z0-9/_{}:.*-]*)[\"']")


@dataclass(frozen=True)
class RouteRegistration:
    path: str
    kind: str = "route"
    methods: frozenset[str] = frozenset()


@dataclass(frozen=True)
class RouteInventory:
    registrations: tuple[RouteRegistration, ...]

    @property
    def all(self) -> frozenset[str]:
        return frozenset(r.path for r in self.registrations)

    def resolves(self, path: str, method: str | None = None) -> bool:
        for registration in self.registrations:
            if method and registration.methods and method.upper() not in registration.methods:
                continue
            if _route_to_regex(registration.path,
                               nested=registration.kind in {"nest", "nest_service"}).fullmatch(path):
                return True
        return False


def _route_methods(text: str, end: int) -> frozenset[str]:
    """Read axum method combinators near a route declaration.

    An empty set means the method could not be proven statically, so path-only
    checks remain conservative.  `nest*` registrations never have methods.
    """
    tail = text[end:text.find(".route(", end) if text.find(".route(", end) >= 0 else end + 500]
    methods = re.findall(r"\b(get|post|put|delete|patch|head|options|connect|trace|any)\s*\(", tail, re.I)
    return frozenset(m.upper() for m in methods)


def collect_route_inventory() -> RouteInventory:
    registrations: list[RouteRegistration] = []
    for root in ROUTE_SOURCE_ROOTS:
        if not root.exists():
            continue
        for p in root.rglob("*"):
            if not p.is_file() or p.suffix not in (".rs", ".ts", ".js", ".mts"):
                continue
            if any(part in WALK_EXCLUDE_DIRS for part in p.parts):
                continue
            try:
                t = p.read_text(encoding="utf-8")
            except (OSError, UnicodeDecodeError):
                continue
            t = _strip_source_comments(t, nested=p.suffix == ".rs")
            for m in _ROUTE_DECL_RE.finditer(t):
                kind, path = m.group("kind"), m.group("path")
                methods = frozenset() if kind != "route" else _route_methods(t, m.end())
                registrations.append(RouteRegistration(path, kind, methods))
            for m in _WORKER_PATH_RE.findall(t):
                registrations.append(RouteRegistration(m, "route"))
    # Adapter-local parameter roots (notably `/{pkg}` in the npm router) are
    # matched only after a mount prefix is stripped.  Without a full router
    # composition graph they are unsafe evidence for a public path.
    registrations = [r for r in registrations if not _is_generic_catchall(r.path)]
    unique = {(r.path, r.kind, r.methods): r for r in registrations}
    return RouteInventory(tuple(unique.values()))


def collect_routes() -> set[str]:
    """Compatibility seam used by the backlog probe and older callers."""
    return set(collect_route_inventory().all)


@lru_cache(maxsize=None)
def _route_to_regex(route: str, *, nested: bool = False) -> re.Pattern:
    """Compile an axum/itty route; only an explicit nest gets a deep suffix."""
    tmp = re.sub(r"\{\*[^}]+\}", "\x00", route)            # {*rest} -> catch-all
    tmp = re.sub(r"\{[^}]+\}", "\x01", tmp)                # {param} -> one segment
    tmp = re.sub(r":[A-Za-z_][A-Za-z0-9_]*", "\x01", tmp)  # :param -> one segment
    out = ["^"]
    for ch in tmp:
        if ch == "\x00":
            out.append(".*")
        elif ch == "\x01":
            out.append("[^/]+")
        elif ch == "*":
            out.append(".*")
        else:
            out.append(re.escape(ch))
    out.append(r"(?:/.*)?$" if nested else r"$")
    return re.compile("".join(out))


def _is_generic_catchall(route: str) -> bool:
    """Catch-all dispatchers are inventory evidence, not public endpoints."""
    if route == "/":
        return True
    parts = [part for part in route.strip("/").split("/") if part]
    return bool("{*" in route or route.rstrip().endswith("/*") or
                (parts and (re.fullmatch(r"\{[^}]+\}|:[A-Za-z_]\w*", parts[0]) is not None)))


def endpoint_resolves(path: str, route_regexes: list[re.Pattern] | RouteInventory,
                      route_prefixes: set[str] | None = None,
                      method: str | None = None) -> bool:
    if isinstance(route_regexes, RouteInventory):
        return route_regexes.resolves(path, method)
    for rx in route_regexes:
        if rx.fullmatch(path):
            return True
    return False


def _endpoint_method(line: str, raw: str) -> str | None:
    """Recover an explicitly documented HTTP verb when one is present."""
    before = line[:line.find(raw)] if raw in line else line
    explicit = re.findall(r"(?:-X\s+|\b)(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|CONNECT|TRACE)\b",
                          before, re.IGNORECASE)
    return explicit[-1].upper() if explicit else None


_DOC_PATH_RE = re.compile(r"(grpcs?://[A-Za-z0-9./_{}:-]+|/(?:v1|v2|turbo|bazel|"
                          r"npm|nix|cache|cas|ac|sccache|pip|cargo|brew)"
                          r"[A-Za-z0-9/_{}:.-]*)")


def extract_doc_endpoints(doc: DocFile) -> list[tuple[int, str]]:
    """Candidate HTTP paths from code contexts (curl/fenced/inline) only."""
    found: list[tuple[int, str]] = []
    lines = doc.text.splitlines()
    in_fence = False
    fence_marker = ""
    for idx, ln in enumerate(lines, start=1):
        fm = _FENCE_RE.match(ln)
        if fm and (not in_fence or ln.strip().startswith(fence_marker)):
            if not in_fence:
                in_fence = True
                fence_marker = fm.group(2)[0] * 3
            else:
                in_fence = False
            continue
        candidates = []
        if in_fence:
            candidates = _DOC_PATH_RE.findall(ln)
        else:
            for span in _INLINE_CODE_RE.findall(ln):
                candidates += _DOC_PATH_RE.findall(span)
        for c in candidates:
            found.append((idx, c))
    return found


def _normalise_endpoint(raw: str) -> str | None:
    raw = raw.strip().rstrip(".,);:'\"")
    if raw.startswith("grpc"):
        m = re.match(r"grpcs?://[^/]+(/.*)?", raw)
        if m and m.group(1):
            return m.group(1).split("?")[0]
        return None  # bare grpc host — resolution out of scope
    path = raw.split("?")[0].split("#")[0]
    if not path.startswith("/"):
        return None
    return path


# ---------------------------------------------------------------------------
# [hostname-liveness] — shipped-surface hostname corpus
#
# WHAT IS SCANNED: only surfaces a CUSTOMER (or a regulator, or a machine reading
# our live config) can reach. A dead hostname in a sealed audit or a test fixture
# harms nobody; a dead hostname in `README.md`, the docs site, the OpenAPI
# document or a breach-notification template is shipped copy.
#
# WHAT IS NOT SCANNED, and why the exclusions are load-bearing rather than
# convenient (same reasoning as ROOT_DOC_EXCLUDE above):
#   * tests/ conformance/ *_test* *.test.* *.spec.* — fixtures deliberately name
#     hosts that must NOT exist (`evil.corelink.humangr.com` is the POINT of a
#     tenant-isolation test).
#   * specs/ — SEALED audits. Rewriting history is forbidden in this repo, so a
#     gate that reddened on a sealed document could only be satisfied by an
#     illegal edit or by an allowlist entry, i.e. by weakening itself.
#   * CHANGELOG.md, reports/, docs/findings/, dated handoff/relay documents —
#     historical ledgers that DESCRIBE the defect ("repointed away from
#     status.corelink.humangr.com"); scanning them makes the gate red for
#     recording history accurately.
#   * build output + _archive/ + releases/ + mutants.out*/ — not authored.
