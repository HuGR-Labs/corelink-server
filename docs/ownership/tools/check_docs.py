#!/usr/bin/env python3
"""CO-1 structural checks, not semantic approval or independent review.

Python >=3.11; dependencies pinned in ../requirements.txt. Uses CommonMark tokens,
not regex over code samples. No network, shell execution, or document commands.
Exit 0: implemented checks passed; 1: violations; 2: input/dependency error.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit

from markdown_it import MarkdownIt
import yaml

BUDGETS = {
    'skill': {'S': (180, 1200, 12), 'H': (180, 1200, 12)},
    'reference': {'S': (400, 3000, 36), 'H': (700, 5500, 64)},
    'blast_radius': {'S': (900, 7000, 80), 'H': (1800, 14000, 160)},
    'maintenance': {'S': (400, 3000, 36), 'H': (700, 5500, 64)},
}
SECTIONS = {'skill': ('s', 7), 'reference': ('r', 8),
            'blast_radius': ('b', 6), 'maintenance': ('m', 6)}
ANCHOR = re.compile(r'<a\s+id="([a-z0-9-]+)"\s*></a>')
PLACEHOLDER = re.compile(r'\{\{[^{}]*\}\}|\bTODO\b|\bTBD\b')
RECORD = re.compile(r'^(REL|API|PROC|INV|FLOW)-([0-9]{3,})\b')
PARSER = MarkdownIt('commonmark', {'html': True}).enable('table')


class UniqueLoader(yaml.SafeLoader):
    """Reject duplicate YAML keys rather than silently keeping the last one."""


def _unique_mapping(loader, node, deep=False):
    result = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if not isinstance(key, str) or key in result:
            raise ValueError('frontmatter keys must be unique strings')
        result[key] = loader.construct_object(value_node, deep=deep)
    return result


UniqueLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _unique_mapping)


def metrics(text: str) -> dict[str, int]:
    return {'lines': len(text.splitlines()), 'words': len(text.split()),
            'bytes': len(text.encode('utf-8'))}


def frontmatter(text: str, template: bool = False):
    """Return (mapping, end line index, errors); YAML is always parsed as data."""
    lines = text.splitlines()
    errors = []
    if not lines or lines[0] != '---':
        return {}, -1, ['missing frontmatter']
    try:
        end = lines.index('---', 1)
    except ValueError:
        return {}, -1, ['unclosed frontmatter']
    raw = '\n'.join(lines[1:end])
    if template:
        raw = PLACEHOLDER.sub('TEMPLATE', raw)
    try:
        result = yaml.load(raw, Loader=UniqueLoader)
        if not isinstance(result, dict):
            raise ValueError('frontmatter must be a mapping')
    except (yaml.YAMLError, ValueError, TypeError) as exc:
        errors.append(f'invalid frontmatter: {exc}')
        result = {}
    return result, end, errors


def _tokens(text: str, template: bool = False):
    fm, end, errors = frontmatter(text, template)
    lines = text.splitlines()
    # Blank the frontmatter while preserving source line numbers in token.map.
    body = '\n'.join([''] * (end + 1) + lines[end+1:]) if end >= 0 else text
    return fm, end, errors, PARSER.parse(body)


def _rendered_structure(tokens):
    anchors, links, headings = [], [], []
    for i, token in enumerate(tokens):
        line = token.map[0] if token.map else -1
        if token.type == 'heading_open' and token.level == 0:
            headings.append((line, int(token.tag[1:]), tokens[i+1].content))
        if token.type == 'html_block':
            # HTML comments and raw HTML must not manufacture a heading/anchor.
            without_comments = re.sub(r'<!--[\s\S]*?-->', '', token.content)
            for m in ANCHOR.finditer(without_comments):
                anchors.append((m.group(1), line))
        if token.type == 'inline':
            children = token.children or []
            for j, child in enumerate(children):
                if child.type == 'html_inline':
                    piece = child.content
                    if j+1 < len(children) and children[j+1].type == 'html_inline':
                        piece += children[j+1].content
                    for m in ANCHOR.finditer(piece):
                        anchors.append((m.group(1), line))
                elif child.type == 'link_open':
                    links.append((child.attrGet('href') or '', line))
    return anchors, links, headings


def explicit_ids(text: str) -> set[str]:
    # Reference targets outside CO-1 may have no frontmatter; ignore its diagnostics.
    return {name for name, _ in _rendered_structure(_tokens(text)[3])[0]}


def _check_metadata(fm: dict, kind: str, profile: str, template: bool, path: Path | None,
                    allow_legacy_schema: bool):
    errors = []
    required = ['name', 'description', 'metadata'] if kind == 'skill' else [
        'schema', 'document', 'package', 'manifest', 'source_commit',
        'profile', 'state', 'evidence_set']
    for key in required:
        if key not in fm:
            errors.append(f'missing frontmatter key: {key}')
        elif key != 'metadata' and (not isinstance(fm[key], str) or not fm[key].strip()):
            errors.append(f'empty/non-string frontmatter key: {key}')
    if template:
        return errors
    if kind == 'skill':
        name, description = fm.get('name'), fm.get('description')
        if (not isinstance(name, str) or not re.fullmatch(r'[a-z0-9]+(?:-[a-z0-9]+)*', name)
                or len(name) > 64):
            errors.append('invalid skill name')
        if path is not None and name != path.parent.name:
            errors.append('skill name differs from directory')
        if not isinstance(description, str) or not 1 <= len(description) <= 600:
            errors.append('skill description must have 1..600 characters')
        metadata = fm.get('metadata')
        if not isinstance(metadata, dict) or not metadata or any(
            not isinstance(v, str) or not v.strip() for v in metadata.values()
        ):
            errors.append('skill metadata must be a nonempty string mapping')
        if 'allowed-tools' in fm:
            errors.append('ownership skill must not grant allowed-tools')
    else:
        if fm.get('schema') not in ('corelink-ownership/1', 'corelink-ownership/1.1'):
            errors.append('unsupported document schema')
        elif (fm.get('schema') == 'corelink-ownership/1' and not template
              and not allow_legacy_schema):
            errors.append('legacy schema 1.0 requires explicit historical-fixture mode')
        if fm.get('document') != kind:
            errors.append('document kind differs from --kind')
        if fm.get('profile') != profile:
            errors.append('document profile differs from --profile')
        if fm.get('state') not in ('draft', 'author_validated', 'candidate'):
            errors.append('document state must not self-certify approval')
        if not re.fullmatch(r'[0-9a-f]{40}', str(fm.get('source_commit', ''))):
            errors.append('invalid source_commit')
        manifest = fm.get('manifest', '')
        if (not isinstance(manifest, str) or not manifest.endswith('Cargo.toml')
                or manifest.startswith('/') or '..' in Path(manifest).parts):
            errors.append('invalid repository-relative manifest')
    return errors


def validate(text: str, kind: str, profile: str, *, template: bool = False,
             path: Path | None = None, root: Path | None = None,
             allow_legacy_schema: bool = False) -> dict:
    """Validate the restricted rendered Markdown profile and metadata consistency.

    Metadata syntax, file limits, section existence and record navigation are checked.
    Semantic correctness, reviewers' actual independence and operating evidence are not.
    """
    if kind not in BUDGETS or profile not in ('S', 'H'):
        raise ValueError('unknown kind/profile')
    errors = []
    counts = metrics(text)
    max_lines, max_words, max_kib = BUDGETS[kind][profile]
    for key, bound in [('lines', max_lines), ('words', max_words), ('bytes', max_kib*1024)]:
        if counts[key] > bound:
            errors.append(f'{key}: {counts[key]} > {bound}')
    if not template and PLACEHOLDER.search(text):
        errors.append('unresolved placeholder/TODO/TBD')
    fm, fm_end, fm_errors, tokens = _tokens(text, template)
    errors.extend(fm_errors)
    errors.extend(_check_metadata(fm, kind, profile, template, path, allow_legacy_schema))
    anchors, links, headings = _rendered_structure(tokens)
    lines = text.splitlines()
    ids = {name for name, _ in anchors}
    if len(ids) != len(anchors):
        errors.append('duplicate explicit anchor')
    prefix, amount = SECTIONS[kind]
    # Every rendered H2 is a navigable contract section, not an unindexed
    # appendix.  This closes the gap where only the eight required headings
    # were checked and an extra H2 could silently bypass navigation policy.
    for _line, level, title in headings:
        if level == 2 and not re.match(rf'^{prefix.upper()}\d{{2}}\b', title):
            errors.append(f'unindexed H2 heading: {title}')
        if level == 2:
            extra = re.match(rf'^{prefix.upper()}(\d{{2}})\b', title)
            if extra and int(extra.group(1)) > amount:
                ident = f'{prefix}{extra.group(1)}'
                if ident not in ids:
                    errors.append(f'missing section anchor: {ident}')
                if not any(link == f'#{ident}' and n < _line for link, n in links):
                    errors.append(f'section missing from navigation: {ident.upper()}')
                end = min([n for n, lev, _ in headings if n > _line and lev <= 3] or [len(lines)])
                if not any(link.startswith(f'#{prefix}') and _line <= n < end for link, n in links):
                    errors.append(f'section missing return-to-index link: {ident}')
    for i in range(1, amount+1):
        ident = f'{prefix}{i:02d}'
        h = [(n, title) for n, level, title in headings
             if level == 2 and re.match(rf'^{ident.upper()}\b', title)]
        if ident not in ids:
            errors.append(f'missing section anchor: {ident}')
        if len(h) != 1:
            errors.append(f'missing section heading: {ident.upper()}' if not h
                          else f'duplicate section heading: {ident.upper()}')
        else:
            # Adjacency is checked below allowing blank lines only.
            candidates = [n for a, n in anchors if a == ident and n < h[0][0]]
            if candidates and any(line.strip() for line in text.splitlines()[max(candidates)+1:h[0][0]]):
                errors.append(f'section anchor detached from heading: {ident}')
            if ident in ids and not candidates:
                errors.append(f'section anchor does not precede heading: {ident}')
        if not any(link == f'#{ident}' for link, _ in links):
            errors.append(f'section missing from navigation: {ident}')
    if sum(level == 1 for _, level, _ in headings) != 1:
        errors.append('expected exactly one H1')
    if any(level > 3 for _, level, _ in headings):
        errors.append('heading deeper than H3')

    # Rendered paragraphs and tables, never code examples pretending to be prose.
    for i, token in enumerate(tokens):
        if token.type == 'paragraph_open' and token.level == 0:
            content = tokens[i+1].content
            if len(content.split()) > 80:
                errors.append('prose paragraph exceeds 80 words')
        if token.type in ('fence', 'code_block'):
            if len(token.content.splitlines()) > 25:
                errors.append(f'code block exceeds 25 lines: {len(token.content.splitlines())}')
            if token.type == 'fence':
                start, end = token.map
                opening = token.markup
                last = lines[end-1].strip() if end > start+1 else ''
                if not re.fullmatch(re.escape(opening[0])+'{'+str(len(opening))+',}\\s*', last):
                    errors.append('unclosed code fence')
        if token.type == 'html_block':
            visible = re.sub(r'<!--[\s\S]*?-->', '', token.content).strip()
            if visible and not all(ANCHOR.fullmatch(x.strip()) for x in visible.splitlines() if x.strip()):
                errors.append('raw HTML other than explicit anchors is outside the document profile')
    # Also reject an over-wide pipe row even if it lacks the delimiter row for a table.
    code_lines = {n for t in tokens if t.type in ('fence', 'code_block')
                  for n in range(*(t.map or (0, 0)))}
    for n, line in enumerate(lines):
        if n <= fm_end or n in code_lines:
            continue
        if line.strip().startswith('|') and len(re.split(r'(?<!\\)\|', line.strip().strip('|'))) > 6:
            errors.append(f'table exceeds six columns on line {n+1}')

    # The lead prose of the first required section is the entry summary. Count
    # all its rendered paragraphs, so splitting into 70+70 words is not a bypass.
    first = next((n for n, level, title in headings
                  if level == 2 and title.startswith(prefix.upper()+'01')), None)
    if first is not None:
        end = min([n for n, level, _ in headings if n > first and level <= 3] or [len(lines)])
        tables = [t.map[0] for t in tokens if t.type == 'table_open' and first < t.map[0] < end]
        if tables:
            end = min(tables)
        lead = [tokens[i+1].content for i,t in enumerate(tokens)
                if t.type == 'paragraph_open' and t.level == 0
                and first < t.map[0] < end and not ANCHOR.fullmatch(tokens[i+1].content.strip())]
        if sum(len(p.split()) for p in lead) > 120:
            errors.append('entry summary exceeds 120 words')

    for start, level, title in headings:
        match = RECORD.match(title) if level == 3 else None
        if not match:
            continue
        typ, serial = match.groups()
        record_id = f'{typ}-{serial}'
        anchor = record_id.lower()
        # A record ends only at the next rendered H1/H2/H3. An arbitrary anchor
        # inside a record is still part of that record; it cannot cut its budget.
        end = min([n for n, lev, _ in headings if n > start and lev <= 3] or [len(lines)])
        # Exclude only the final next-heading anchor and trailing whitespace.
        tail = end
        while tail > start+1 and not lines[tail-1].strip():
            tail -= 1
        if tail > start+1 and ANCHOR.fullmatch(lines[tail-1].strip()):
            tail -= 1
        record = '\n'.join(lines[start:tail]).strip()
        if typ in ('REL', 'API', 'PROC'):
            words, limit_lines = {'REL': (120,14), 'API':(100,None), 'PROC':(220,50)}[typ]
            if len(record.split()) > words:
                errors.append(f'{record_id} exceeds {words} words')
            if limit_lines is not None and len(record.splitlines()) > limit_lines:
                errors.append(f'{record_id} exceeds {limit_lines} lines')
        if typ in ('PROC', 'FLOW'):
            steps = sum(t.type == 'list_item_open' and start < t.map[0] < end
                        for t in tokens if t.map)
            if steps > 8:
                errors.append(f'{record_id} exceeds eight numbered steps')
        if anchor not in ids:
            errors.append(f'missing record anchor: {anchor}')
        if not any(link == '#'+anchor and n < start for link,n in links):
            errors.append(f'record missing from preceding index: {record_id}')
        if not any(link.startswith('#'+prefix) and start <= n < end for link,n in links):
            errors.append(f'record missing return-to-index link: {record_id}')

    for link, _ in links:
        parsed = urlsplit(link)
        if parsed.scheme or parsed.netloc:
            continue
        if not parsed.path:
            if unquote(parsed.fragment) not in ids:
                errors.append(f'broken internal anchor: {link}')
            continue
        if template:
            continue
        if path is None or root is None:
            errors.append('relative-link checking requires --root and a real file path')
            continue
        target = (path.parent / unquote(parsed.path)).resolve()
        if not target.is_relative_to(root.resolve()):
            errors.append(f'local link escapes root: {link}')
        elif not target.is_file():
            errors.append(f'missing local link target: {link}')
        elif parsed.fragment and target.suffix.lower() == '.md':
            if unquote(parsed.fragment) not in explicit_ids(target.read_text(encoding='utf-8')):
                errors.append(f'broken explicit anchor in local target: {link}')
    return {'kind': kind, 'profile': profile, 'template_mode': template,
            'metrics': counts, 'errors': sorted(set(errors)),
            'verdict': 'FAIL' if errors else 'IMPLEMENTED_CHECKS_PASS',
            'not_certified': ['external links', 'semantic completeness', 'runtime claims',
                              'cold review', 'profile eligibility', 'cross-file three-click navigation']}


def main() -> int:
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('file', type=Path)
    ap.add_argument('--kind', choices=BUDGETS, required=True)
    ap.add_argument('--profile', choices=['S','H'], default='S')
    ap.add_argument('--root', type=Path)
    ap.add_argument('--template', action='store_true')
    ap.add_argument('--historical-fixture', action='store_true',
                    help='allow legacy schema 1.0 only for isolated historical fixtures')
    ns=ap.parse_args()
    try:
        result=validate(ns.file.read_text(encoding='utf-8'),ns.kind,ns.profile,
                        template=ns.template,path=ns.file.resolve(),root=ns.root,
                        allow_legacy_schema=ns.historical_fixture)
        print(json.dumps(result,ensure_ascii=False,indent=2))
        return int(bool(result['errors']))
    except (OSError,UnicodeError,ValueError) as exc:
        print(f'ERROR: {exc}',file=sys.stderr); return 2

if __name__=='__main__':
    raise SystemExit(main())
