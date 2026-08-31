#!/usr/bin/env python3
"""Resolve every internal link written by hand in the docs React pages.

WHY THIS GATE EXISTS (measured, 2026-08-31)
-------------------------------------------
`apps/docs/docusaurus.config.ts` sets `onBrokenLinks: "throw"`, so the docs
build is green. That setting only covers links Docusaurus itself resolves:
Markdown/MDX links and `<Link>` elements whose target it can see. It does NOT
cover a raw `<a href="/...">` typed into a React page under
`apps/docs/src/pages/**`. Four such links on the LIVE legal pages were 404
in production while every gate in the repo was green:

    /explanation/compliance/dpa   (x6)  -- file exists but is `draft: true`
                                         AND declares `slug: "/compliance/dpa"`
    /explanation/privacy/gdpr           -- ditto, slug `/privacy/gdpr`
    /explanation/privacy/lgpd-full      -- ditto, slug `/privacy/lgpd-full`
    /explanation/sre/slo                -- no such directory at all

This script closes that hole: it collects every internal `href=` / `to=`
string literal in `apps/docs/src/**/*.{tsx,jsx}` and resolves it against the
set of destinations the PRODUCTION build actually serves.

ANTI-VACUITY CONTRACT
---------------------
A gate that cannot find anything must SCREAM, never pass. Every way of
failing to obtain a list is a named, loud `fail()` here:

  * a required directory or file is missing            -> named failure
  * a glob matches zero files                          -> named failure
  * front-matter is unparseable / not a mapping        -> named failure
  * zero doc destinations, zero page destinations,
    zero static destinations, zero redirects, or zero
    scanned hrefs                                      -> named failure
  * `routeBasePath` / `baseUrl` cannot be read from
    docusaurus.config.ts                               -> named failure

Front-matter is parsed with PyYAML, never with a regex.

Run: python3 scripts/check_docs_react_links.py [--verbose]
Exit 0 = every internal href resolves. Exit 1 = at least one does not, or the
gate could not establish its own inputs.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

try:
    import yaml
except ImportError:  # pragma: no cover - environment failure must be loud
    print("FAIL[dependency]: PyYAML is required (pip install pyyaml)", file=sys.stderr)
    raise SystemExit(1)

REPO_ROOT = Path(__file__).resolve().parent.parent
DOCS_APP = REPO_ROOT / "apps" / "docs"
CONFIG_FILE = DOCS_APP / "docusaurus.config.ts"
DOCS_DIR = DOCS_APP / "docs"
BLOG_DIR = DOCS_APP / "blog"
PAGES_DIR = DOCS_APP / "src" / "pages"
STATIC_DIR = DOCS_APP / "static"
SCAN_DIR = DOCS_APP / "src"

DOC_EXTS = (".md", ".mdx")
PAGE_EXTS = (".tsx", ".jsx", ".ts", ".js", ".md", ".mdx")
SCAN_EXTS = (".tsx", ".jsx")

# Route base paths, mirrored from docusaurus.config.ts. Asserted against the
# config file below rather than trusted -- if the config moves the docs off
# `/` this gate must fail loudly, not silently resolve against a stale map.
DOCS_ROUTE_BASE = "/"
BLOG_ROUTE_BASE = "/blog"

# Routes Docusaurus generates for the blog plugin itself (listing, archive,
# tag index). Not derived from files, so they are declared here explicitly.
BLOG_GENERATED_ROUTES = ("/blog", "/blog/archive", "/blog/tags")

# Docusaurus `numberPrefixParser` default: `01-foo` / `1.foo` -> `foo`.
NUMBER_PREFIX_RE = re.compile(r"^\d+[-._]\s*")

# `href="/..."` / `to="/..."` string literals in JSX.
HREF_RE = re.compile(r'\b(?:href|to)\s*=\s*"([^"]*)"')

FAILURES: list[str] = []


def fail(kind: str, message: str) -> None:
    FAILURES.append(f"FAIL[{kind}]: {message}")


def require_dir(path: Path, kind: str) -> bool:
    if not path.is_dir():
        fail(kind, f"required directory does not exist: {path}")
        return False
    return True


def read_front_matter(path: Path) -> dict:
    """Parse YAML front-matter. Any unreadable/odd shape is a LOUD failure."""
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as exc:
        fail("front-matter", f"{path}: unreadable ({exc})")
        return {}
    if not text.startswith("---"):
        return {}
    end = text.find("\n---", 3)
    if end == -1:
        fail("front-matter", f"{path}: opening '---' with no closing '---'")
        return {}
    raw = text[3:end]
    try:
        data = yaml.safe_load(raw)
    except yaml.YAMLError as exc:
        fail("front-matter", f"{path}: YAML parse error ({exc})")
        return {}
    if data is None:
        return {}
    if not isinstance(data, dict):
        fail("front-matter", f"{path}: front-matter is {type(data).__name__}, expected a mapping")
        return {}
    return data


def join_route(base: str, rel: str) -> str:
    joined = "/" + "/".join(p for p in f"{base}/{rel}".split("/") if p)
    return joined


def strip_number_prefixes(rel: str) -> str:
    return "/".join(NUMBER_PREFIX_RE.sub("", seg) for seg in rel.split("/") if seg)


def normalize(route: str) -> str:
    """Site-absolute, no trailing slash (`trailingSlash: false`), no case fold."""
    if not route.startswith("/"):
        route = "/" + route
    route = re.sub(r"/{2,}", "/", route)
    if len(route) > 1:
        route = route.rstrip("/")
    return route


def assert_config(verbose: bool) -> str:
    """Read baseUrl + assert the docs/blog routeBasePath this gate assumes."""
    if not CONFIG_FILE.is_file():
        fail("config", f"docusaurus config not found: {CONFIG_FILE}")
        return ""
    text = CONFIG_FILE.read_text(encoding="utf-8")

    base_url_match = re.search(r'const\s+BASE_URL\s*=\s*"([^"]+)"', text)
    if not base_url_match:
        fail("config", f"{CONFIG_FILE}: could not read `const BASE_URL = \"...\"`")
        base_url = ""
    else:
        base_url = base_url_match.group(1)

    route_bases = re.findall(r'routeBasePath:\s*"([^"]*)"', text)
    if not route_bases:
        fail("config", f"{CONFIG_FILE}: no `routeBasePath:` found -- route map cannot be trusted")
    else:
        expected = {DOCS_ROUTE_BASE, BLOG_ROUTE_BASE}
        actual = {normalize(r) if r else "/" for r in route_bases}
        # `docsRouteBasePath` of the search plugin repeats "/" -- set compare.
        if not expected.issubset(actual):
            fail(
                "config",
                f"{CONFIG_FILE}: routeBasePath set {sorted(actual)} does not contain the "
                f"{sorted(expected)} this gate assumes -- update DOCS_ROUTE_BASE/BLOG_ROUTE_BASE",
            )
    if verbose:
        print(f"  config: BASE_URL={base_url!r} routeBasePath={sorted(set(route_bases))}")
    return base_url


def collect_doc_routes(root: Path, route_base: str, kind: str, verbose: bool) -> set[str]:
    """Published routes for a content dir, EXCLUDING `draft: true` files."""
    routes: set[str] = set()
    if not require_dir(root, kind):
        return routes
    files = sorted(p for p in root.rglob("*") if p.suffix in DOC_EXTS and p.is_file())
    if not files:
        fail(kind, f"glob matched ZERO {DOC_EXTS} files under {root}")
        return routes
    drafts = 0
    for path in files:
        # Docusaurus ignores `_`-prefixed files and partials directories.
        rel_parts = path.relative_to(root).parts
        if any(part.startswith("_") for part in rel_parts):
            continue
        fm = read_front_matter(path)
        if fm.get("draft") is True:
            drafts += 1
            continue
        rel = path.relative_to(root).with_suffix("").as_posix()
        rel = strip_number_prefixes(rel)
        segments = rel.split("/")
        if segments[-1].lower() in ("index", "readme"):
            segments = segments[:-1]
        elif isinstance(fm.get("id"), str) and fm["id"]:
            segments[-1] = fm["id"]
        parent = "/".join(segments[:-1]) if segments else ""

        slug = fm.get("slug")
        if isinstance(slug, str) and slug:
            if slug.startswith("/"):
                route = join_route(route_base, slug)
            else:
                route = join_route(route_base, f"{parent}/{slug}")
        else:
            route = join_route(route_base, "/".join(segments))
        routes.add(normalize(route))
    if not routes:
        fail(kind, f"ZERO published routes derived from {len(files)} files under {root}")
    if verbose:
        print(f"  {kind}: {len(routes)} published routes from {len(files)} files ({drafts} draft, excluded)")
    return routes


def collect_page_routes(verbose: bool) -> set[str]:
    routes: set[str] = set()
    if not require_dir(PAGES_DIR, "pages"):
        return routes
    files = sorted(p for p in PAGES_DIR.rglob("*") if p.suffix in PAGE_EXTS and p.is_file())
    if not files:
        fail("pages", f"glob matched ZERO {PAGE_EXTS} files under {PAGES_DIR}")
        return routes
    for path in files:
        name = path.name
        if name.startswith("_") or ".test." in name or ".spec." in name or ".module." in name:
            continue
        if any(part.startswith("_") for part in path.relative_to(PAGES_DIR).parts[:-1]):
            continue
        rel = path.relative_to(PAGES_DIR).with_suffix("").as_posix()
        segments = [s for s in rel.split("/") if s]
        if segments and segments[-1].lower() == "index":
            segments = segments[:-1]
        routes.add(normalize("/" + "/".join(segments)))
    if not routes:
        fail("pages", f"ZERO routes derived from {len(files)} files under {PAGES_DIR}")
    if verbose:
        print(f"  pages: {len(routes)} routes from {len(files)} files")
    return routes


def collect_static_routes(verbose: bool) -> set[str]:
    routes: set[str] = set()
    if not require_dir(STATIC_DIR, "static"):
        return routes
    files = [p for p in STATIC_DIR.rglob("*") if p.is_file()]
    if not files:
        fail("static", f"ZERO files under {STATIC_DIR}")
        return routes
    for path in files:
        routes.add(normalize("/" + path.relative_to(STATIC_DIR).as_posix()))
    if verbose:
        print(f"  static: {len(routes)} asset routes")
    return routes


def collect_redirect_sources(verbose: bool) -> set[str]:
    """`from:` sources of @docusaurus/plugin-client-redirects."""
    routes: set[str] = set()
    if not CONFIG_FILE.is_file():
        return routes
    text = CONFIG_FILE.read_text(encoding="utf-8")
    if "plugin-client-redirects" not in text:
        if verbose:
            print("  redirects: plugin not configured (0 sources)")
        return routes
    for match in re.finditer(r'\bfrom:\s*"([^"]+)"', text):
        routes.add(normalize(match.group(1)))
    if not routes:
        fail(
            "redirects",
            f"{CONFIG_FILE}: plugin-client-redirects IS configured but ZERO `from:` entries "
            "were parsed -- the redirect map cannot be trusted",
        )
    if verbose:
        print(f"  redirects: {len(routes)} `from:` sources")
    return routes


SCANNED_FILE_COUNT = 0


def collect_hrefs(verbose: bool) -> list[tuple[Path, int, str]]:
    global SCANNED_FILE_COUNT
    found: list[tuple[Path, int, str]] = []
    if not require_dir(SCAN_DIR, "scan"):
        return found
    files = sorted(p for p in SCAN_DIR.rglob("*") if p.suffix in SCAN_EXTS and p.is_file())
    if not files:
        fail("scan", f"glob matched ZERO {SCAN_EXTS} files under {SCAN_DIR}")
        return found
    SCANNED_FILE_COUNT = len(files)
    for path in files:
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for match in HREF_RE.finditer(line):
                found.append((path, lineno, match.group(1)))
    if not found:
        fail("scan", f"ZERO href=/to= literals found across {len(files)} files under {SCAN_DIR}")
    if verbose:
        print(f"  scan: {len(found)} href/to literals across {len(files)} tsx/jsx files")
    return found


def is_internal(href: str) -> bool:
    if not href:
        return False
    lowered = href.lower()
    if lowered.startswith(("http://", "https://", "mailto:", "tel:", "//", "#", "data:")):
        return False
    return href.startswith("/")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    print("docs React-page internal-link gate")
    base_url = assert_config(args.verbose)

    doc_routes = collect_doc_routes(DOCS_DIR, DOCS_ROUTE_BASE, "docs", args.verbose)
    blog_routes = collect_doc_routes(BLOG_DIR, BLOG_ROUTE_BASE, "blog", args.verbose)
    blog_routes |= {normalize(r) for r in BLOG_GENERATED_ROUTES}
    page_routes = collect_page_routes(args.verbose)
    static_routes = collect_static_routes(args.verbose)
    redirect_routes = collect_redirect_sources(args.verbose)

    known = doc_routes | blog_routes | page_routes | static_routes | redirect_routes
    if not known:
        fail("destinations", "ZERO published destinations were derived -- gate would be vacuous")

    hrefs = collect_hrefs(args.verbose)
    internal = [(p, n, h) for (p, n, h) in hrefs if is_internal(h)]
    if hrefs and not internal:
        fail("scan", f"{len(hrefs)} href/to literals found but ZERO are internal '/'-rooted -- gate would be vacuous")

    if FAILURES:
        print()
        for line in FAILURES:
            print(line, file=sys.stderr)
        print(
            "\nThe gate could not establish its inputs. This is a FAILURE, not a pass.",
            file=sys.stderr,
        )
        return 1

    base_prefix = normalize(base_url) if base_url and base_url != "/" else ""
    broken: list[tuple[Path, int, str]] = []
    for path, lineno, href in internal:
        target = href.split("#", 1)[0].split("?", 1)[0]
        if not target:
            continue
        target = normalize(target)
        candidates = {target}
        if base_prefix and target.startswith(base_prefix + "/"):
            candidates.add(normalize(target[len(base_prefix):]))
        if not (candidates & known):
            broken.append((path, lineno, href))

    print()
    print(
        f"POPULATION: {len(internal)} internal hrefs "
        f"({len(hrefs)} href/to literals total, in "
        f"{len({p for p, _, _ in hrefs})} of {SCANNED_FILE_COUNT} scanned tsx/jsx files "
        f"under apps/docs/src), "
        f"resolved against {len(known)} published destinations "
        f"(docs={len(doc_routes)}, blog={len(blog_routes)}, pages={len(page_routes)}, "
        f"static={len(static_routes)}, redirects={len(redirect_routes)})."
    )

    if broken:
        print()
        print(f"BROKEN: {len(broken)} internal href(s) resolve to no published destination:", file=sys.stderr)
        for path, lineno, href in broken:
            print(f"  {path.relative_to(REPO_ROOT)}:{lineno}: dead link -> {href}", file=sys.stderr)
        print(
            "\nEach href above must either point at a published (non-draft) route or be "
            "replaced with prose. Note: a page that exists on disk but is `draft: true`, "
            "or that declares a different `slug:` in its front-matter, is NOT published.",
            file=sys.stderr,
        )
        return 1

    print(f"OK: all {len(internal)} internal hrefs resolve.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
