#!/usr/bin/env python3
"""okf_context.py — route OKF wiki concepts INTO agent context.

The OKF architecture wiki (docs/knowledge/, 146 code-grounded concepts) is
"context for AI agents", but nothing routes a concept INTO an agent's context
when it is about to touch the matching code. This closes that loop: given a
query, find the RELEVANT concepts and print them so an agent can load them
before working blind.

Query forms (pick one; combine freely):
  --file PATH   concepts whose frontmatter `source_files` declares PATH
                (the "load auth/* before touching auth code" lookup)
  --tag  TAG    concepts whose frontmatter `tags` contains TAG
  TEXT...       free text matched against concept id / title (and a bare
                taxonomy dir name like `auth` lists everything under it)

By default prints `id  —  title` lines; with --full also prints each concept
body so the agent works WITH the grounded architecture, not against it.

stdlib-only (no PyYAML): a small purpose-built frontmatter reader handles the
two YAML shapes this bundle uses — inline `[a, b]` lists and block `- item`
lists, quoted or unquoted.
"""
from __future__ import annotations

import argparse
import os
import sys

# repo_root/scripts/okf_context.py -> repo_root
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_KNOWLEDGE = os.path.join(REPO_ROOT, "docs", "knowledge")
# OKF reserved machine files — never concepts (profile C9).
RESERVED = {"index.md", "log.md"}


def _strip_scalar(v: str) -> str:
    v = v.strip()
    if len(v) >= 2 and v[0] == v[-1] and v[0] in "\"'":
        v = v[1:-1]
    return v


def _parse_inline_list(v: str) -> list[str]:
    v = v.strip()
    if v.startswith("[") and v.endswith("]"):
        v = v[1:-1]
    out = []
    for item in v.split(","):
        item = _strip_scalar(item)
        if item:
            out.append(item)
    return out


def parse_concept(path: str) -> dict | None:
    """Read a concept .md, returning {id,title,tags,source_files,body,path}.

    Frontmatter is the first `---`-fenced block. Only the keys we route on are
    extracted; both inline-list and block-list YAML shapes are supported.
    """
    try:
        with open(path, "r", encoding="utf-8") as fh:
            text = fh.read()
    except OSError:
        return None
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        return None
    # locate closing fence
    end = None
    for i in range(1, len(lines)):
        if lines[i].strip() == "---":
            end = i
            break
    if end is None:
        return None
    fm_lines = lines[1:end]
    body = "\n".join(lines[end + 1:]).strip()

    title = ""
    tags: list[str] = []
    source_files: list[str] = []

    i = 0
    while i < len(fm_lines):
        raw = fm_lines[i]
        line = raw.rstrip("\n")
        if not line.strip() or line.lstrip().startswith("#"):
            i += 1
            continue
        # top-level key (no leading indent)
        if not line.startswith((" ", "\t")) and ":" in line:
            key, _, rest = line.partition(":")
            key = key.strip()
            rest = rest.strip()
            if key == "title":
                title = _strip_scalar(rest)
                i += 1
                continue
            if key in ("tags", "source_files"):
                target = tags if key == "tags" else source_files
                if rest:  # inline list or inline scalar
                    if rest.startswith("["):
                        target.extend(_parse_inline_list(rest))
                    else:
                        target.append(_strip_scalar(rest))
                    i += 1
                    continue
                # block list: following indented `- item` lines
                j = i + 1
                while j < len(fm_lines):
                    item_line = fm_lines[j]
                    if item_line.strip().startswith("- "):
                        target.append(_strip_scalar(item_line.strip()[2:]))
                        j += 1
                    elif not item_line.strip():
                        j += 1
                    else:
                        break
                i = j
                continue
        i += 1

    rel = os.path.relpath(path, DEFAULT_KNOWLEDGE)
    cid = rel[:-3] if rel.endswith(".md") else rel  # drop .md
    cid = cid.replace(os.sep, "/")
    return {
        "id": cid,
        "title": title,
        "tags": tags,
        "source_files": source_files,
        "body": body,
        "path": path,
    }


def load_concepts(knowledge_dir: str) -> list[dict]:
    concepts = []
    for root, _dirs, files in os.walk(knowledge_dir):
        for name in files:
            if not name.endswith(".md"):
                continue
            full = os.path.join(root, name)
            if os.path.relpath(full, knowledge_dir) in RESERVED:
                continue
            c = parse_concept(full)
            if c is not None:
                concepts.append(c)
    concepts.sort(key=lambda c: c["id"])
    return concepts


def _norm_path(p: str) -> str:
    return p.strip().lstrip("./").replace("\\", "/").rstrip("/")


def file_matches(concept: dict, query_path: str) -> bool:
    q = _norm_path(query_path)
    if not q:
        return False
    for sf in concept["source_files"]:
        s = _norm_path(sf)
        if s == q or s.endswith("/" + q) or q.endswith("/" + s):
            return True
    return False


def tag_matches(concept: dict, tag: str) -> bool:
    t = tag.strip().lower()
    return any(ct.strip().lower() == t for ct in concept["tags"])


def text_matches(concept: dict, text: str) -> bool:
    t = text.strip().lower()
    if not t:
        return False
    return t in concept["id"].lower() or t in concept["title"].lower()


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(
        prog="okf_context.py",
        description="Find the OKF wiki concepts relevant to a file/tag/text "
        "and print them for an agent to load into context.",
    )
    p.add_argument("--file", help="repo path; match concepts that declare it in source_files")
    p.add_argument("--tag", help="match concepts whose frontmatter tags contain this tag")
    p.add_argument("text", nargs="*", help="free text matched against concept id/title (or a taxonomy dir name)")
    p.add_argument("--full", action="store_true", help="also print each concept's body")
    p.add_argument("--knowledge-dir", default=DEFAULT_KNOWLEDGE, help="override the docs/knowledge bundle dir")
    args = p.parse_args(argv)

    if not (args.file or args.tag or args.text):
        p.error("give a query: --file PATH, --tag TAG, or free TEXT")

    knowledge_dir = os.path.abspath(args.knowledge_dir)
    if not os.path.isdir(knowledge_dir):
        print(f"okf_context: knowledge dir not found: {knowledge_dir}", file=sys.stderr)
        return 2

    concepts = load_concepts(knowledge_dir)

    free_text = " ".join(args.text).strip()
    # bare taxonomy dir name (e.g. `auth`) -> list everything under it
    taxonomy_dir = None
    if free_text and "/" not in free_text and " " not in free_text:
        if os.path.isdir(os.path.join(knowledge_dir, free_text)):
            taxonomy_dir = free_text

    results = []
    for c in concepts:
        if args.file and file_matches(c, args.file):
            results.append(c)
            continue
        if args.tag and tag_matches(c, args.tag):
            results.append(c)
            continue
        if taxonomy_dir and c["id"].split("/", 1)[0] == taxonomy_dir:
            results.append(c)
            continue
        if free_text and not taxonomy_dir and text_matches(c, free_text):
            results.append(c)

    # describe the query for the header
    qparts = []
    if args.file:
        qparts.append(f"--file {args.file}")
    if args.tag:
        qparts.append(f"--tag {args.tag}")
    if free_text:
        qparts.append(f"text '{free_text}'")
    query_desc = " ".join(qparts)

    if not results:
        print(f"# OKF: no concepts match {query_desc}", file=sys.stderr)
        return 1

    print(f"# OKF concepts relevant to {query_desc} ({len(results)} found)")
    print("# load these before modifying this subsystem (docs/knowledge/)")
    for c in results:
        print(f"- {c['id']}  —  {c['title']}")
    if args.full:
        for c in results:
            print()
            print("=" * 78)
            print(f"# {c['id']} — {c['title']}")
            if c["source_files"]:
                print(f"# source_files: {', '.join(c['source_files'])}")
            if c["tags"]:
                print(f"# tags: {', '.join(c['tags'])}")
            print("=" * 78)
            print(c["body"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
