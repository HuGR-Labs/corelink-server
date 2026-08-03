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

Usage:
    python3 scripts/validate_docs_reality.py                 # normal gate (CI)
    python3 scripts/validate_docs_reality.py --strict        # also fail tracked_drift
    python3 scripts/validate_docs_reality.py --list-refs      # dump every CLI ref found
    python3 scripts/validate_docs_reality.py --allowlist <path>

Exit contract (mirrors validate_okf.py / validate_specs.py):
    0        -> "OK docs-reality: <N> CLI refs, <T> tracked, <W> warnings, 0 drift"
    non-zero -> per-check offender list, then
                "DOCS-REALITY INVALID: <k> failures"

Dependencies: Python stdlib only (json + re + argparse). No PyYAML required.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
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
ROUTE_SOURCE_ROOTS = [REPO_ROOT / "crates", REPO_ROOT / "worker" / "src"]

DOC_EXTS = {".md", ".mdx", ".mdc", ".html", ".htm", ".txt"}
WALK_EXCLUDE_DIRS = {"node_modules", ".git", "build", "dist", ".open-next",
                     ".wrangler", "target", ".docusaurus"}

# Fenced-code info-strings that are SHELL (a `corelink ...` line is a CLI
# invocation). Any OTHER language (python, ts, rust, json, ...) is skipped for CLI
# extraction so an SDK snippet — `from corelink import CoreLinkClient` — is never
# mistaken for a `corelink import` subcommand.
SHELL_FENCE_LANGS = {"", "bash", "sh", "shell", "shell-session", "console",
                     "zsh", "text", "sh-session", "terminal"}


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
    text = main_rs.read_text(encoding="utf-8")
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
_ROUTE_DECL_RE = re.compile(r"\.(?:route|nest|nest_service)\(\s*\"([^\"]+)\"")
# Worker route literals + bare "/path" table entries. Best-effort: any string
# literal that looks like an absolute product route.
_WORKER_PATH_RE = re.compile(r"[\"'](/(?:v1|v2|turbo|bazel|npm|nix|cache|cas|ac|"
                             r"sccache|pip|cargo|brew|pkg|simple|token|internal|"
                             r"_internal|healthz|_health)[A-Za-z0-9/_{}:.*-]*)[\"']")


def collect_routes() -> set[str]:
    routes: set[str] = set()
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
            for m in _ROUTE_DECL_RE.findall(t):
                routes.add(m)
            for m in _WORKER_PATH_RE.findall(t):
                routes.add(m)
    return routes


def _route_to_regex(route: str) -> re.Pattern:
    """axum/itty path -> regex. `{param}` / `:param` / `{*rest}` / `*` are
    wildcards; a trailing wildcard also matches deeper (nested) paths."""
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
    out.append(r"(?:/.*)?$")  # exact OR a deeper path under this route (nesting)
    return re.compile("".join(out))


def endpoint_resolves(path: str, route_regexes: list[re.Pattern],
                      route_prefixes: set[str]) -> bool:
    for rx in route_regexes:
        if rx.match(path):
            return True
    for pref in route_prefixes:
        base = pref.rstrip("/")
        if base and (path == base or path.startswith(base + "/")):
            return True
    return False


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
# Allowlist
# ---------------------------------------------------------------------------
def load_allowlist(path: Path) -> dict:
    if not path.exists():
        return {}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        print(f"warning: could not parse allowlist {path}: {exc}", file=sys.stderr)
        return {}


# ---------------------------------------------------------------------------
# okf-deferred-coherence rules
# ---------------------------------------------------------------------------
@dataclass
class DeferredFinding:
    rule_id: str
    file: str
    line: int
    excerpt: str
    reason: str


def _file_has_marker(rel: str, marker: str) -> bool:
    p = REPO_ROOT / rel
    if not p.exists():
        return False
    try:
        return marker.lower() in p.read_text(encoding="utf-8").lower()
    except (OSError, UnicodeDecodeError):
        return False


def run_deferred_coherence(rules: list[dict], docs: list[DocFile],
                           warnings: list[str]) -> list[DeferredFinding]:
    """Each rule:
        id, grounding: {okf_concept|cli_source, marker},
        forbidden: [regex,...], allow: [regex,...] (skip a line matching any),
        scope_globs: [path-prefix,...] (optional), reason.
    A rule that no longer grounds (marker gone / capability shipped) is reported as
    WARN:stale and SKIPPED — so building the capability + updating OKF auto-retires
    the rule with a nudge to delete it."""
    findings: list[DeferredFinding] = []
    for rule in rules:
        rid = rule.get("id", "?")
        grounding = rule.get("grounding", {})
        src = grounding.get("okf_concept") or grounding.get("cli_source")
        marker = grounding.get("marker", "deferred")
        if src is not None and not _file_has_marker(src, marker):
            warnings.append(
                f"[okf-deferred-coherence] rule '{rid}' STALE: grounding marker "
                f"'{marker}' no longer present in {src} — capability may have "
                f"shipped; delete or update this rule.")
            continue

        forbidden = [re.compile(p, re.IGNORECASE) for p in rule.get("forbidden", [])]
        allow = [re.compile(p, re.IGNORECASE) for p in rule.get("allow", [])]
        scope = rule.get("scope_globs", [])
        for doc in docs:
            if scope and not any(doc.rel.startswith(s) for s in scope):
                continue
            for idx, ln in enumerate(doc.text.splitlines(), start=1):
                if not any(fx.search(ln) for fx in forbidden):
                    continue
                if any(ax.search(ln) for ax in allow):
                    continue
                findings.append(DeferredFinding(
                    rid, doc.rel, idx, ln.strip()[:160], rule.get("reason", "")))
    return findings


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def main() -> int:
    ap = argparse.ArgumentParser(description="Validate customer-facing claims "
                                             "against shipped reality.")
    ap.add_argument("--strict", action="store_true",
                    help="also FAIL on tracked_drift (the known-drift punch-list).")
    ap.add_argument("--allowlist", default=str(DEFAULT_ALLOWLIST))
    ap.add_argument("--list-refs", action="store_true",
                    help="print every CLI reference discovered, then exit 0.")
    args = ap.parse_args()

    allow = load_allowlist(Path(args.allowlist))
    cli_allow = allow.get("cli", {})
    roadmap_allow = {k.strip() for k in cli_allow.get("roadmap_allow", {})}
    tracked_drift = {k.strip() for k in cli_allow.get("tracked_drift", {})}

    model = parse_cli_model(CLI_MAIN)
    docs = _iter_doc_files()

    all_refs: list[CliRef] = []
    for d in docs:
        all_refs.extend(extract_cli_refs(d))

    if args.list_refs:
        for r in sorted(all_refs, key=lambda x: (x.cmd, x.file, x.line)):
            tag = "OK" if model.is_valid_top(r.cmd) else "??"
            act = f" {r.action}" if r.action else ""
            print(f"[{tag}] corelink {r.cmd}{act}  ({r.file}:{r.line})")
        print(f"\nvalid top-level commands: {sorted(model.top)}")
        print(f"groups: { {k: sorted(v) for k, v in model.groups.items()} }")
        return 0

    # --- [cli-existence] ---
    cli_failures: list[tuple[str, CliRef, str]] = []
    cli_tracked: list[tuple[str, CliRef]] = []
    # Every allowlist key that actually suppressed something on this run. A key
    # that suppresses NOTHING is a DEAD suppression: the defect it was written
    # for is gone (or was never reachable), and all it does now is stand ready to
    # re-silence the same defect the day it recurs. See [suppression-hygiene].
    used_suppressions: set[str] = set()
    for r in all_refs:
        if model.is_valid_top(r.cmd):
            if model.is_group(r.cmd) and r.action and not r.action.startswith("-"):
                key2 = f"{r.cmd} {r.action}"
                if not model.is_valid_action(r.cmd, r.action):
                    if key2 in roadmap_allow:
                        used_suppressions.add(key2)
                        continue
                    if key2 in tracked_drift:
                        used_suppressions.add(key2)
                        cli_tracked.append((key2, r))
                        continue
                    cli_failures.append(
                        (key2, r,
                         f"`corelink {r.cmd} {r.action}` — '{r.action}' is not a "
                         f"valid `{r.cmd}` action (valid: "
                         f"{sorted(model.groups[r.cmd])})"))
            continue
        if r.cmd in roadmap_allow:
            used_suppressions.add(r.cmd)
            continue
        if r.cmd in tracked_drift:
            used_suppressions.add(r.cmd)
            cli_tracked.append((r.cmd, r))
            continue
        cli_failures.append(
            (r.cmd, r,
             f"`corelink {r.cmd}` — not in CLI `enum Commands` "
             f"(valid: {sorted(model.top)})"))

    strict_tracked_fail: list[tuple[str, CliRef]] = []
    if args.strict:
        strict_tracked_fail = cli_tracked
        cli_tracked = []

    # --- [suppression-hygiene] ---
    # An allowlist with no expiry is a permanent mute button. Report every
    # suppression key that matched NOTHING on this run so a dead entry is visible
    # instead of dormant — the difference between "known and accepted" and "known
    # and forgotten". Non-fatal by design (the corpus legitimately shrinks); the
    # remedy is to DELETE the entry, recording why, not to leave it armed.
    warnings: list[str] = []
    for key in sorted(roadmap_allow | tracked_drift):
        if key in used_suppressions:
            continue
        bucket = "roadmap_allow" if key in roadmap_allow else "tracked_drift"
        warnings.append(
            f"[suppression-hygiene] allowlist {bucket} entry `corelink {key}` "
            f"never matched — dead suppression; delete it (record the removal in "
            f"cli._retired) or state why it must stay armed.")

    # --- [okf-deferred-coherence] ---
    deferred_rules = allow.get("deferred_coherence", [])
    deferred_findings = run_deferred_coherence(deferred_rules, docs, warnings)
    deferred_tracked_ids = {r["id"] for r in deferred_rules if r.get("tracked")}
    deferred_fatal = [f for f in deferred_findings
                      if f.rule_id not in deferred_tracked_ids or args.strict]
    deferred_softtracked = [f for f in deferred_findings
                            if f.rule_id in deferred_tracked_ids and not args.strict]

    # --- [endpoint-existence] (best-effort, WARN unless flagship) ---
    routes = collect_routes()
    route_regexes = [_route_to_regex(r) for r in routes]
    route_prefixes = {r for r in routes if "{" not in r and ":" not in r}
    endpoint_cfg = allow.get("endpoint", {})
    flagship_files = set(endpoint_cfg.get("flagship_files", []))
    ignore_prefixes = tuple(endpoint_cfg.get("ignore_path_prefixes", []))
    endpoint_warns: list[str] = []
    endpoint_fatal: list[str] = []
    seen_ep: set[tuple] = set()
    for d in docs:
        for ln, raw in extract_doc_endpoints(d):
            norm = _normalise_endpoint(raw)
            if norm is None:
                continue
            if ignore_prefixes and norm.startswith(ignore_prefixes):
                continue
            if endpoint_resolves(norm, route_regexes, route_prefixes):
                continue
            key = (d.rel, norm)
            if key in seen_ep:
                continue
            seen_ep.add(key)
            msg = (f"[endpoint-existence] {d.rel}:{ln} — path `{norm}` "
                   f"resolves to no wired route")
            if d.rel in flagship_files:
                endpoint_fatal.append(msg)
            else:
                endpoint_warns.append(msg)

    # -----------------------------------------------------------------------
    # Report
    # -----------------------------------------------------------------------
    total_fail = (len(cli_failures) + len(strict_tracked_fail) +
                  len(deferred_fatal) + len(endpoint_fatal))

    if cli_failures:
        print("== [cli-existence] FAIL — documented command does not exist ==")
        for _k, r, msg in sorted(cli_failures, key=lambda x: (x[0], x[1].file)):
            print(f"  {r.file}:{r.line}: {msg}")
        print()

    if strict_tracked_fail:
        print("== [cli-existence] --strict — tracked drift (would fail the gate) ==")
        for key, r in sorted(strict_tracked_fail, key=lambda x: x[0]):
            note = cli_allow.get("tracked_drift", {}).get(key, {})
            reason = note.get("reason", "") if isinstance(note, dict) else str(note)
            print(f"  {r.file}:{r.line}: `corelink {key}` — {reason}")
        print()

    if deferred_fatal:
        print("== [okf-deferred-coherence] FAIL — claim contradicts OKF/code ==")
        for f in deferred_fatal:
            print(f"  {f.file}:{f.line}: [{f.rule_id}] {f.reason}")
            print(f"      > {f.excerpt}")
        print()

    if endpoint_fatal:
        print("== [endpoint-existence] FAIL — flagship recipe path is not wired ==")
        for m in endpoint_fatal:
            print(f"  {m}")
        print()

    if cli_tracked:
        print(f"-- tracked CLI drift (non-fatal; --strict to fail): "
              f"{len(cli_tracked)} --")
        for key, rel in sorted({(k, rr.file) for k, rr in cli_tracked}):
            print(f"  tracked: `corelink {key}`  ({rel})")
    if deferred_softtracked:
        print(f"-- tracked claim drift (non-fatal; --strict to fail): "
              f"{len(deferred_softtracked)} --")
        for f in deferred_softtracked:
            print(f"  tracked: {f.file}:{f.line} [{f.rule_id}]")
    if warnings:
        print(f"-- warnings: {len(warnings)} --")
        for w in warnings:
            print(f"  {w}")
    if endpoint_warns:
        print(f"-- [endpoint-existence] unresolved (best-effort WARN): "
              f"{len(endpoint_warns)} --")
        for m in endpoint_warns[:40]:
            print(f"  {m}")
        if len(endpoint_warns) > 40:
            print(f"  ... and {len(endpoint_warns) - 40} more")

    print()
    if total_fail == 0:
        soft = len(warnings) + len(endpoint_warns) + len(deferred_softtracked)
        print(f"OK docs-reality: {len(all_refs)} CLI refs, "
              f"{len(cli_tracked)} tracked, {soft} warnings, 0 drift")
        return 0
    print(f"DOCS-REALITY INVALID: {total_fail} failures")
    return 1


if __name__ == "__main__":
    sys.exit(main())
