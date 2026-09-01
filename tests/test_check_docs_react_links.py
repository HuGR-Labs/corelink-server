"""Load-bearing tests for the JSX-route gate added by B-168.

The scanner deliberately has no Node/TypeScript-parser dependency, so these
tests exercise its syntax policy and a complete minimal Docusaurus population
with the same Python entry point CI runs.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT = REPO_ROOT / "scripts" / "check_docs_react_links.py"
SPEC = importlib.util.spec_from_file_location("check_docs_react_links", SCRIPT)
assert SPEC and SPEC.loader
gate = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = gate
SPEC.loader.exec_module(gate)


class ReactLinkGateTest(unittest.TestCase):
    @contextlib.contextmanager
    def gate_tree(self, page_source: str):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            app = root / "apps" / "docs"
            config = app / "docusaurus.config.ts"
            docs = app / "docs"
            blog = app / "blog"
            pages = app / "src" / "pages"
            static = app / "static"
            for directory in (docs, blog, pages, static):
                directory.mkdir(parents=True, exist_ok=True)
            config.write_text(
                'const BASE_URL = "/";\n'
                'const docs = { routeBasePath: "/" };\n'
                'const blog = { routeBasePath: "/blog" };\n',
                encoding="utf-8",
            )
            (docs / "guide.md").write_text("# guide\n", encoding="utf-8")
            (blog / "post.md").write_text("# post\n", encoding="utf-8")
            (pages / "index.tsx").write_text(page_source, encoding="utf-8")
            (static / "favicon.ico").write_bytes(b"fixture")
            with patch.multiple(
                gate,
                REPO_ROOT=root,
                DOCS_APP=app,
                CONFIG_FILE=config,
                DOCS_DIR=docs,
                BLOG_DIR=blog,
                PAGES_DIR=pages,
                STATIC_DIR=static,
                SCAN_DIR=app / "src",
                MIN_SCANNED_FILE_COUNT=1,
                MIN_LITERAL_ATTRIBUTE_COUNT=1,
                MIN_DYNAMIC_ATTRIBUTE_COUNT=0,
                MIN_INTERNAL_HREF_COUNT=1,
            ):
                yield

    def run_gate(self, *, verbose: bool = False) -> tuple[int, str, str]:
        stdout, stderr = io.StringIO(), io.StringIO()
        argv = [str(SCRIPT), "--verbose"] if verbose else [str(SCRIPT)]
        with patch.object(sys, "argv", argv), contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            result = gate.main()
        return result, stdout.getvalue(), stderr.getvalue()

    def test_simple_double_and_braced_literal_routes_pass(self) -> None:
        with self.gate_tree(
            '<a href="/guide">double</a><Link to=\'/guide\'>single</Link>'
            '<a href={"/guide"}>braced literal</a>'
        ):
            result, stdout, stderr = self.run_gate(verbose=True)
        self.assertEqual(result, 0, stderr)
        self.assertIn("POPULATION: 3 internal hrefs", stdout)

    def test_dead_literal_route_fails_with_file_and_line(self) -> None:
        with self.gate_tree('<a href=\'/gone\'>gone</a>'):
            result, _stdout, stderr = self.run_gate()
        self.assertEqual(result, 1)
        self.assertIn("src/pages/index.tsx:1: dead link -> /gone", stderr)

    def test_mixed_population_empty_target_fails(self) -> None:
        with self.gate_tree('<a href="/guide">valid</a><a href="">empty</a>'):
            result, _stdout, stderr = self.run_gate()
        self.assertEqual(result, 1)
        self.assertIn("empty href= is not a route", stderr)

    def test_empty_expression_fails_instead_of_becoming_dynamic(self) -> None:
        with self.gate_tree('<a href="/guide">valid</a><a href={}>empty expression</a>'):
            result, _stdout, stderr = self.run_gate()
        self.assertEqual(result, 1)
        self.assertIn("empty JSX expression", stderr)

    def test_unclosed_opening_tag_fails(self) -> None:
        with self.gate_tree('<a href="/guide"'):
            result, _stdout, stderr = self.run_gate()
        self.assertEqual(result, 1)
        self.assertIn("opening tag has no closing '>'", stderr)

    def test_bare_value_fails(self) -> None:
        with self.gate_tree('<a href=/guide>bare</a>'):
            result, _stdout, stderr = self.run_gate()
        self.assertEqual(result, 1)
        self.assertIn("value must be quoted", stderr)

    def test_js_strings_and_comments_do_not_enter_population(self) -> None:
        with self.gate_tree(
            "const fake = '<a href=\"/gone\" />';\n"
            "// <a href=\"/also-gone\" />\n"
            "export default function Page() { return <div>{/* <a href=\"/jsx-comment\" /> */}"
            '<a href="/guide">valid</a></div>; }'
        ):
            result, stdout, stderr = self.run_gate()
        self.assertEqual(result, 0, stderr)
        self.assertIn("POPULATION: 1 internal hrefs", stdout)

    def test_nonempty_dynamic_expression_is_reported_but_not_silently_skipped(self) -> None:
        with self.gate_tree(
            'const route = "/guide"; export default function Page() { return '
            '<div><a href="/guide">valid</a><a href={route}>dynamic</a></div>; }'
        ):
            result, stdout, stderr = self.run_gate(verbose=True)
        self.assertEqual(result, 0, stderr)
        self.assertIn("1 dynamic-classified", stdout)
        self.assertIn("dynamic policy:", stdout)

    def test_population_floor_fails_when_the_gate_loses_coverage(self) -> None:
        with self.gate_tree('<a href="/guide">only link</a>'), patch.object(
            gate, "MIN_LITERAL_ATTRIBUTE_COUNT", 2
        ):
            result, _stdout, stderr = self.run_gate()
        self.assertEqual(result, 1)
        self.assertIn("below the measured B-168 floor 2", stderr)


if __name__ == "__main__":
    unittest.main()
