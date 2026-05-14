#!/usr/bin/env python3
"""publish_privacy_diff.py — Daily privacy notice diff publisher.

Per WI-S11-004 §6.1 AC-007: generates HTML diff between consecutive notice
versions and deploys to Cloudflare Pages /privacy/changelog/<date>.html.

In production: called by Cloudflare Cron Worker (S-09 inheritance) daily.
Output: /privacy/changelog/<YYYY-MM-DD>.html + index.html update.

Usage:
    python3 scripts/publish_privacy_diff.py \\
        --prev legal/privacy-notice/v1.5.0/ \\
        --curr legal/privacy-notice/v2.0.0/ \\
        --date 2026-05-13 \\
        --out /tmp/privacy-changelog/

Cloudflare Pages deployment is handled by the CD pipeline via `wrangler pages deploy`.
"""

import argparse
import difflib
import html
import pathlib
import sys
from datetime import datetime, timezone


CANONICAL_LOCALES = ["pt-BR.md", "en-US.md", "es-MX.md"]


def generate_diff_html(
    prev_dir: pathlib.Path,
    curr_dir: pathlib.Path,
    date_str: str,
    prev_version: str,
    curr_version: str,
) -> str:
    """Generate an HTML diff page for a version transition."""
    sections: list[str] = []

    for locale_file in CANONICAL_LOCALES:
        prev_path = prev_dir / locale_file
        curr_path = curr_dir / locale_file

        prev_lines = prev_path.read_text(encoding="utf-8").splitlines(keepends=True) if prev_path.exists() else []
        curr_lines = curr_path.read_text(encoding="utf-8").splitlines(keepends=True) if curr_path.exists() else []

        diff = difflib.unified_diff(
            prev_lines,
            curr_lines,
            fromfile=f"{prev_version}/{locale_file}",
            tofile=f"{curr_version}/{locale_file}",
            lineterm="",
        )

        diff_text = "".join(diff)
        escaped = html.escape(diff_text) if diff_text else "(no changes in this locale)"

        sections.append(f"""
<section>
  <h2>{html.escape(locale_file)}</h2>
  <pre class="diff">{escaped}</pre>
</section>""")

    sections_html = "\n".join(sections)

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Privacy Notice Changelog — {html.escape(date_str)}</title>
  <style>
    body {{ font-family: system-ui, sans-serif; max-width: 900px; margin: 2rem auto; padding: 0 1rem; }}
    h1 {{ color: #1a1a1a; }}
    h2 {{ color: #333; margin-top: 2rem; }}
    pre.diff {{ background: #f6f8fa; padding: 1rem; overflow-x: auto; font-size: 0.85em; border-left: 4px solid #ccc; }}
    .meta {{ color: #555; font-size: 0.9em; margin-bottom: 1rem; }}
    a {{ color: #0066cc; }}
  </style>
</head>
<body>
  <h1>Privacy Notice Changelog</h1>
  <p class="meta">
    Published: <strong>{html.escape(date_str)}</strong> |
    Transition: <a href="/privacy/{html.escape(prev_version)}/">{html.escape(prev_version)}</a>
    → <a href="/privacy/{html.escape(curr_version)}/">{html.escape(curr_version)}</a>
  </p>
  <p>
    <a href="/privacy/changelog/">← All changelogs</a>
  </p>
  {sections_html}
</body>
</html>
"""


def generate_index_html(entries: list[tuple[str, str, str]]) -> str:
    """Generate the changelog index HTML.

    Args:
        entries: list of (date_str, prev_version, curr_version) tuples, newest first.
    """
    rows = "\n".join(
        f'    <tr><td><a href="/privacy/changelog/{html.escape(d)}.html">{html.escape(d)}</a></td>'
        f"<td>{html.escape(p)} → {html.escape(c)}</td></tr>"
        for d, p, c in entries
    )

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Privacy Notice Changelog Index</title>
  <style>
    body {{ font-family: system-ui, sans-serif; max-width: 900px; margin: 2rem auto; padding: 0 1rem; }}
    table {{ border-collapse: collapse; width: 100%; }}
    th, td {{ border: 1px solid #ddd; padding: 0.5rem 1rem; text-align: left; }}
    th {{ background: #f6f8fa; }}
    a {{ color: #0066cc; }}
  </style>
</head>
<body>
  <h1>Privacy Notice Changelog</h1>
  <p>Full history of privacy notice changes. Published daily per CTRL-PRIV-CONSENT-005.</p>
  <table>
    <thead><tr><th>Date</th><th>Version Transition</th></tr></thead>
    <tbody>
{rows}
    </tbody>
  </table>
</body>
</html>
"""


def main() -> None:
    parser = argparse.ArgumentParser(description="Publish daily privacy notice diff.")
    parser.add_argument("--prev", required=True, help="Previous notice version directory")
    parser.add_argument("--curr", required=True, help="Current notice version directory")
    parser.add_argument("--date", default=datetime.now(timezone.utc).strftime("%Y-%m-%d"))
    parser.add_argument("--out", required=True, help="Output directory for HTML files")
    args = parser.parse_args()

    prev_dir = pathlib.Path(args.prev)
    curr_dir = pathlib.Path(args.curr)
    out_dir = pathlib.Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    if not prev_dir.is_dir():
        print(f"ERROR: prev directory not found: {prev_dir}", file=sys.stderr)
        sys.exit(1)
    if not curr_dir.is_dir():
        print(f"ERROR: curr directory not found: {curr_dir}", file=sys.stderr)
        sys.exit(1)

    prev_version = prev_dir.name
    curr_version = curr_dir.name

    diff_html = generate_diff_html(prev_dir, curr_dir, args.date, prev_version, curr_version)
    diff_out = out_dir / f"{args.date}.html"
    diff_out.write_text(diff_html, encoding="utf-8")
    print(f"Generated: {diff_out}")

    # Update index.
    index_out = out_dir / "index.html"
    entries = [(args.date, prev_version, curr_version)]
    index_html = generate_index_html(entries)
    index_out.write_text(index_html, encoding="utf-8")
    print(f"Generated: {index_out}")

    print(
        f"Diff publication complete: {prev_version} → {curr_version} on {args.date}. "
        "Deploy via: wrangler pages deploy <out-dir> --project-name corelink-privacy --branch main"
    )


if __name__ == "__main__":
    main()
