#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""
generate-release-notes.py — customer-facing release-notes generator.

Inputs (all derivable from repo state — no invention):
  --from <ref>      Lower bound (exclusive). Required.
  --to   <ref>      Upper bound (inclusive). Defaults to HEAD.
  --version <vX.Y.Z>  Optional explicit version label (else derived from
                    `--to` ref name, falling back to short-sha).
  --out  <path>     Output file path. Defaults to
                    releases/RELEASE-<version>.md.
  --dry-run         Print to stdout, do not write file.
  --strict          Fail (exit 5) if a customer-visible commit lacks a
                    Conventional-Commits prefix.

Sections (in order):
  1. TL;DR              — 3 sentences derived from CHANGELOG header for
                          the matching version section (if present).
  2. What's new         — feat(*) commits + PRs labeled `customer-visible`.
  3. Performance        — perf(*) commits; surfaced measured deltas from
                          commit-body lines matching `(-|\+)\d+(\.\d+)?%`.
  4. Security           — sec(*) | security(*) commits + CVE refs from
                          commit body (regex `CVE-\d{4}-\d{4,}`).
  5. Compliance         — debt-register closures (rows containing
                          `CLOSED YYYY-MM-DD`) intersecting the date
                          range of the tag window; audit-doc refreshes.
  6. Breaking changes   — commits with `!:` or `BREAKING CHANGE:` footer;
                          OpenAPI deprecations (heuristic via diff of
                          `openapi/**` for `deprecated: true`).
  7. Migration guide    — link to `docs/customer/MIGRATION-<version>.md`
                          if present, else "(none required)".
  8. Acknowledgments    — distinct external author emails in commit log
                          that do not end in @humangr.com /
                          @anthropic.com / are not committers in
                          the CODEOWNERS list.

Exclusions (silently dropped):
  - refactor / test / ci / build / chore / style — internal noise.
  - Commits without a Conventional-Commits prefix UNLESS they are
    `seal(...)` (treated as `feat`) or are referenced by a PR labeled
    `customer-visible`.

Cross-references:
  - CHANGELOG.md            — source of TL;DR when version matches.
  - specs/_audits/*debt-register*.md  — source of compliance closures.
  - .github/PULL_REQUEST_TEMPLATE.md   — `customer-visible` label convention.
  - releases/TEMPLATE.md    — handcrafted-polish template.
  - marketing/launch/RELEASE-NOTES-EDITORIAL-GUIDE.md — operator workflow.

Exit codes:
  0  Success.
  2  Missing required argument.
  3  Invalid git ref.
  4  No customer-visible commits in range.
  5  Strict mode: customer-visible commit lacks Conventional-Commits prefix.

Charter (HARD):
  - No invention of release content not derivable from inputs.
  - Every bullet cross-references an underlying sha / PR / debt-register row.
  - Customer-friendly tone; internal jargon goes in TECHNICAL-CHANGELOG.md.
"""
from __future__ import annotations

import argparse
import datetime as _dt
import os
import pathlib
import re
import subprocess
import sys
from typing import Iterable

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent

# ---- Frontmatter -----------------------------------------------------------

FRONTMATTER_TEMPLATE = """---
id: "RELEASE-{version}"
type: "release_notes"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "{version}"
created: "{date}"
updated: "{date}"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
parent: "CHANGELOG.md"
tags:
  - "release-notes"
  - "customer-facing"
  - "auto-generated"
source_refs:
  - "from: {from_ref}"
  - "to:   {to_ref}"
generator: "scripts/generate-release-notes.py"
---
"""

# ---- Conventional Commits parsing -----------------------------------------

CC_RE = re.compile(r"^(?P<type>[a-zA-Z]+)(?P<scope>\([^)]+\))?(?P<bang>!)?:\s*(?P<subject>.+)$")
PERCENT_DELTA_RE = re.compile(r"[+-]?\d+(?:\.\d+)?\s?%")
CVE_RE = re.compile(r"CVE-\d{4}-\d{4,}")
DEBT_CLOSED_RE = re.compile(r"DEBT-\d{3,}.*?CLOSED\s+(\d{4}-\d{2}-\d{2})", re.IGNORECASE)
SKIP_TYPES = {"refactor", "test", "tests", "ci", "build", "chore", "style", "merge"}


def _run(cmd: list[str], check: bool = True) -> str:
    res = subprocess.run(cmd, cwd=REPO_ROOT, capture_output=True, text=True)
    if check and res.returncode != 0:
        raise RuntimeError(f"command failed ({res.returncode}): {' '.join(cmd)}\n{res.stderr}")
    return res.stdout


def _verify_ref(ref: str) -> str:
    res = subprocess.run(
        ["git", "rev-parse", "--verify", ref],
        cwd=REPO_ROOT, capture_output=True, text=True,
    )
    if res.returncode != 0:
        print(f"error: invalid git ref: {ref}", file=sys.stderr)
        sys.exit(3)
    return res.stdout.strip()


def _resolve_version(to_ref: str, explicit: str | None) -> str:
    if explicit:
        return explicit.lstrip("v")
    # If `to_ref` is itself a tag, use it.
    res = subprocess.run(
        ["git", "describe", "--exact-match", "--tags", to_ref],
        cwd=REPO_ROOT, capture_output=True, text=True,
    )
    if res.returncode == 0:
        return res.stdout.strip().lstrip("v")
    # Fallback to short-sha.
    short = subprocess.run(
        ["git", "rev-parse", "--short", to_ref],
        cwd=REPO_ROOT, capture_output=True, text=True, check=True,
    ).stdout.strip()
    return f"dev-{short}"


def _commits(from_ref: str, to_ref: str) -> list[dict]:
    """Yield commits with sha, subject, body, author_email, author_name."""
    rng = f"{from_ref}..{to_ref}"
    # Use a NUL-record split to safely handle bodies.
    fmt = "%H%x1f%s%x1f%ae%x1f%an%x1f%b%x1e"
    raw = _run(["git", "log", f"--format={fmt}", rng])
    commits = []
    for rec in raw.split("\x1e"):
        rec = rec.strip("\n")
        if not rec:
            continue
        parts = rec.split("\x1f")
        if len(parts) < 5:
            continue
        sha, subject, email, name, body = parts[0], parts[1], parts[2], parts[3], parts[4]
        commits.append({
            "sha": sha,
            "short": sha[:7],
            "subject": subject,
            "email": email,
            "name": name,
            "body": body,
        })
    return commits


def _parse_cc(subject: str) -> tuple[str | None, str | None, bool, str]:
    """Returns (type, scope, breaking, subject_rest)."""
    m = CC_RE.match(subject)
    if not m:
        return None, None, False, subject
    return (
        m.group("type").lower(),
        (m.group("scope") or "").strip("()") or None,
        bool(m.group("bang")),
        m.group("subject"),
    )


def _is_breaking(commit: dict, parsed_bang: bool) -> bool:
    if parsed_bang:
        return True
    return "BREAKING CHANGE" in commit["body"]


def _changelog_tldr(version: str) -> list[str]:
    """Pull up to 3 lines of prose from CHANGELOG.md under the matching version section."""
    path = REPO_ROOT / "CHANGELOG.md"
    if not path.exists():
        return []
    text = path.read_text(encoding="utf-8")
    # Match `## [X.Y.Z]` or `## [X.Y.Z-rc.N]` headings.
    pattern = re.compile(
        rf"^## \[{re.escape(version)}\][^\n]*\n(?P<body>.*?)(?=\n## \[|\Z)",
        re.MULTILINE | re.DOTALL,
    )
    m = pattern.search(text)
    if not m:
        return []
    body = m.group("body")
    # First 3 non-empty prose lines (skip subheadings and bullet lists).
    out: list[str] = []
    for line in body.splitlines():
        s = line.strip()
        if not s or s.startswith(("#", "-", "*", "Tag:", "|")):
            continue
        out.append(s)
        if len(out) == 3:
            break
    return out


def _debt_closures(date_lo: _dt.date, date_hi: _dt.date) -> list[tuple[str, str]]:
    """Scan latest debt-register doc for `CLOSED YYYY-MM-DD` rows within range.

    Returns list of (debt_id, closure_line).
    """
    audits = REPO_ROOT / "specs" / "_audits"
    if not audits.exists():
        return []
    registers = sorted(audits.glob("*debt-register*.md"))
    if not registers:
        return []
    latest = registers[-1]
    out: list[tuple[str, str]] = []
    for line in latest.read_text(encoding="utf-8").splitlines():
        m = DEBT_CLOSED_RE.search(line)
        if not m:
            continue
        try:
            closed_on = _dt.date.fromisoformat(m.group(1))
        except ValueError:
            continue
        if not (date_lo <= closed_on <= date_hi):
            continue
        # Extract DEBT-NNN id.
        idm = re.search(r"DEBT-\d{3,}", line)
        if not idm:
            continue
        # Trim to first sentence-ish.
        cleaned = re.sub(r"\s+", " ", line).strip("| ").strip()
        out.append((idm.group(0), cleaned[:240]))
    # De-dup by debt-id keeping first match.
    seen = set()
    deduped = []
    for did, txt in out:
        if did in seen:
            continue
        seen.add(did)
        deduped.append((did, txt))
    return deduped


def _openapi_deprecations(from_ref: str, to_ref: str) -> list[str]:
    """Heuristic: lines added to `openapi/**` containing `deprecated: true`."""
    res = subprocess.run(
        ["git", "diff", f"{from_ref}..{to_ref}", "--", "openapi/", "specs/openapi/"],
        cwd=REPO_ROOT, capture_output=True, text=True,
    )
    if res.returncode != 0:
        return []
    out = []
    cur_file = None
    for line in res.stdout.splitlines():
        if line.startswith("+++ b/"):
            cur_file = line[6:]
        elif line.startswith("+") and "deprecated: true" in line.lower():
            out.append(f"{cur_file}: {line[1:].strip()}")
    return out


def _is_external(email: str) -> bool:
    if not email:
        return False
    e = email.lower()
    return not any(e.endswith(d) for d in ("@humangr.com", "@anthropic.com", "@users.noreply.github.com"))


def _date_of(ref: str) -> _dt.date:
    raw = _run(["git", "log", "-1", "--format=%cd", "--date=short", ref]).strip()
    return _dt.date.fromisoformat(raw)


def _customer_visible_prs(from_ref: str, to_ref: str) -> list[str]:
    """Best-effort: look for `Closes #NNN` / `(#NNN)` in commit messages,
    cannot label-query without network. We surface the PR refs verbatim so
    a human can map them in the editorial pass — never inventing labels.
    """
    raw = _run(["git", "log", "--format=%H%x1f%s%x1f%b", f"{from_ref}..{to_ref}"])
    pr_refs: list[str] = []
    for rec in raw.split("\n"):
        m = re.search(r"\(#(\d+)\)", rec)
        if m:
            pr_refs.append(f"#{m.group(1)}")
    seen = set()
    out = []
    for p in pr_refs:
        if p in seen:
            continue
        seen.add(p)
        out.append(p)
    return out


def _bullet(commit: dict, prefix: str = "") -> str:
    return f"- {prefix}{commit['subject']} (`{commit['short']}`)"


def _render(
    *,
    version: str,
    from_ref: str,
    to_ref: str,
    tldr: list[str],
    new: list[dict],
    perf: list[dict],
    security: list[dict],
    compliance: list[tuple[str, str]],
    breaking: list[dict],
    api_deprecations: list[str],
    migration_link: str | None,
    ack: list[str],
    pr_refs: list[str],
) -> str:
    today = _dt.date.today().isoformat()
    out: list[str] = []
    out.append(FRONTMATTER_TEMPLATE.format(
        version=version, date=today, from_ref=from_ref, to_ref=to_ref,
    ))
    out.append(f"# CoreLink {version} — Release Notes\n")
    out.append(f"> **Status:** DRAFT — auto-generated on {today} from `{from_ref}..{to_ref}`.")
    out.append("> Editorial pass per `marketing/launch/RELEASE-NOTES-EDITORIAL-GUIDE.md` required before publish.")
    out.append("> Cross-ref: `CHANGELOG.md` (authoritative changelog) · `ROADMAP-TO-GA.md §8` (R-8 launch) · `marketing/launch/LAUNCH-CHECKLIST-V2.md`.\n")

    out.append("## TL;DR\n")
    if tldr:
        out.extend(tldr)
    else:
        out.append("_No CHANGELOG section matched `{}`; editorial pass should hand-write 3 sentences._".format(version))
    out.append("")

    out.append("## What's new\n")
    if new:
        for c in new:
            out.append(_bullet(c))
    else:
        out.append("_No customer-visible features in this range._")
    if pr_refs:
        out.append("")
        out.append(f"_PR references in range (for editorial label-cross-check): {', '.join(pr_refs[:20])}_")
    out.append("")

    out.append("## Performance\n")
    if perf:
        for c in perf:
            deltas = PERCENT_DELTA_RE.findall(c["subject"] + " " + c["body"])
            tail = f" — measured: {', '.join(sorted(set(deltas)))}" if deltas else ""
            out.append(f"- {c['subject']} (`{c['short']}`){tail}")
    else:
        out.append("_No performance changes in this range._")
    out.append("")

    out.append("## Security\n")
    if security:
        for c in security:
            cves = CVE_RE.findall(c["subject"] + " " + c["body"])
            tail = f" — refs: {', '.join(sorted(set(cves)))}" if cves else ""
            out.append(f"- {c['subject']} (`{c['short']}`){tail}")
    else:
        out.append("_No security fixes in this range._")
    out.append("")

    out.append("## Compliance\n")
    if compliance:
        for did, line in compliance:
            out.append(f"- **{did}** — {line[:200]}")
        out.append("")
        out.append("_Source: `specs/_audits/*debt-register*.md` (latest)._")
    else:
        out.append("_No debt-register closures landed in this range._")
    out.append("")

    out.append("## Breaking changes\n")
    if breaking or api_deprecations:
        for c in breaking:
            out.append(f"- **BREAKING** — {c['subject']} (`{c['short']}`)")
        for line in api_deprecations:
            out.append(f"- **API deprecation** — {line}")
    else:
        out.append("_No breaking changes in this range._")
    out.append("")

    out.append("## Migration guide\n")
    if migration_link:
        out.append(f"See [`{migration_link}`]({migration_link}).")
    else:
        out.append("_No migration steps required for this release._")
    out.append("")

    out.append("## Acknowledgments\n")
    if ack:
        for who in ack:
            out.append(f"- {who}")
    else:
        out.append("_No external contributors in this range._")
    out.append("")

    out.append("---\n")
    out.append("_Generated by `scripts/generate-release-notes.py`. Internal-only commits (refactor/test/ci/build/chore/style) excluded by design. See `TECHNICAL-CHANGELOG.md` for the engineering-internal trajectory if one exists._")
    return "\n".join(out) + "\n"


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(
        prog="generate-release-notes.py",
        description="Generate customer-facing release notes from git history.",
    )
    p.add_argument("--from", dest="from_ref", required=True, help="Lower bound git ref (exclusive).")
    p.add_argument("--to", dest="to_ref", default="HEAD", help="Upper bound git ref (inclusive). Default: HEAD.")
    p.add_argument("--version", dest="version", default=None, help="Explicit version label (else derived).")
    p.add_argument("--out", dest="out", default=None, help="Output path. Default: releases/RELEASE-<version>.md.")
    p.add_argument("--dry-run", dest="dry_run", action="store_true", help="Print to stdout; do not write file.")
    p.add_argument("--strict", dest="strict", action="store_true", help="Fail if a customer-visible commit lacks Conventional-Commits prefix.")
    args = p.parse_args(argv)

    from_sha = _verify_ref(args.from_ref)
    to_sha = _verify_ref(args.to_ref)
    version = _resolve_version(args.to_ref, args.version)
    commits = _commits(from_sha, to_sha)
    if not commits:
        print(f"error: no commits between {args.from_ref}..{args.to_ref}", file=sys.stderr)
        return 4

    new: list[dict] = []
    perf: list[dict] = []
    security: list[dict] = []
    breaking: list[dict] = []
    untagged_customer_visible: list[dict] = []

    for c in commits:
        ctype, scope, bang, _rest = _parse_cc(c["subject"])
        if c["subject"].lower().startswith(("merge ", "merge_")) or c["subject"].startswith("merge "):
            continue
        if ctype is None:
            # Allow `seal(...)` shorthand as a feat-equivalent.
            if c["subject"].startswith("seal("):
                new.append(c)
            else:
                untagged_customer_visible.append(c)
            continue
        if ctype in SKIP_TYPES:
            # Skipped silently.
            if _is_breaking(c, bang):
                breaking.append(c)
            continue
        if _is_breaking(c, bang):
            breaking.append(c)
        if ctype == "feat":
            new.append(c)
        elif ctype == "perf":
            perf.append(c)
        elif ctype in ("sec", "security"):
            security.append(c)
        elif ctype == "fix":
            # Customer-visible fixes are surfaced under What's new with a
            # `Fixed:` prefix; security-flagged fixes go under Security.
            if scope and "sec" in scope.lower():
                security.append(c)
            else:
                new.append(c)
        # docs / deprecate / remove are not auto-surfaced; editorial pass adds.

    if args.strict and untagged_customer_visible:
        print("error: --strict — commits without Conventional-Commits prefix:", file=sys.stderr)
        for c in untagged_customer_visible[:10]:
            print(f"  {c['short']} {c['subject']}", file=sys.stderr)
        return 5

    # Compliance: scan latest debt register for closures in the date window.
    lo = _date_of(from_sha)
    hi = _date_of(to_sha)
    compliance = _debt_closures(lo, hi)

    # API deprecations heuristic.
    api_deprecations = _openapi_deprecations(from_sha, to_sha)

    # Migration guide link if present.
    mig = REPO_ROOT / "docs" / "customer" / f"MIGRATION-{version}.md"
    migration_link = f"docs/customer/MIGRATION-{version}.md" if mig.exists() else None

    # External contributors.
    ack_emails: dict[str, str] = {}
    for c in commits:
        if _is_external(c["email"]):
            ack_emails.setdefault(c["email"], c["name"])
    ack = sorted(f"{name} <{email}>" for email, name in ack_emails.items())

    # CHANGELOG TL;DR pull (best-effort).
    tldr = _changelog_tldr(version)

    pr_refs = _customer_visible_prs(from_sha, to_sha)

    rendered = _render(
        version=version,
        from_ref=args.from_ref,
        to_ref=args.to_ref,
        tldr=tldr,
        new=new,
        perf=perf,
        security=security,
        compliance=compliance,
        breaking=breaking,
        api_deprecations=api_deprecations,
        migration_link=migration_link,
        ack=ack,
        pr_refs=pr_refs,
    )

    if args.dry_run:
        sys.stdout.write(rendered)
        return 0

    out_path = pathlib.Path(args.out) if args.out else (REPO_ROOT / "releases" / f"RELEASE-{version}.md")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(rendered, encoding="utf-8")
    print(f"wrote {out_path.relative_to(REPO_ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
