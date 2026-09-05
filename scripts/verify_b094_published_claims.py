#!/usr/bin/env python3
"""Deterministic inventory gate for the published B-094 surface.

The old B-094 check was a closed list of English regular expressions.  It could
miss a new wording, could not account for every occurrence, and used population
floors that were easy to turn into folklore.  This gate has two independent
parts:

* the checked-in manifest gives every Buck2/Pants/gRPC occurrence a stable
  identity (path + normalized line + duplicate ordinal) and an explicit reason;
  an added, removed, or changed occurrence is a HALT until the inventory is
  deliberately re-derived;
* a small semantic guard rejects positive Buck2/Pants support claims even when
  the wording is new.  Negative status, protocol descriptions, competitor
  copy, and vocabulary lists remain valid inventory entries.

Only the customer-facing Markdown/MDX/HTML roots and the explicit Docusaurus
metadata sources listed in the manifest are in scope.  No network access is
performed.
"""

from __future__ import annotations

import argparse
import hashlib
import html
import json
import os
import re
import sys
import unicodedata
from pathlib import Path
from typing import Iterable

REPO_ROOT = Path(__file__).resolve().parent.parent
MANIFEST = REPO_ROOT / "scripts" / "b094_published_inventory.json"
SCHEMA = 1
EXTENSIONS = {".md", ".mdx", ".html", ".htm"}
ROOTS = ("apps/docs", "marketing", "legal")
ROOT_FILES = ("README.md",)
# ``docusaurus.config.ts`` is executable source, but its ``metadata`` array is
# emitted directly into the public site head.  It therefore needs the same
# inventory/hash protection as authored pages rather than a broad ``*.ts``
# sweep that would mix implementation-only source into the published corpus.
PUBLISHED_METADATA_FILES = ("apps/docs/docusaurus.config.ts",)
WALK_EXCLUDED = {"node_modules", "build", "dist", ".docusaurus", ".wrangler"}
TERM_RE = re.compile(r"\b(?:buck2|pants|gRPC)\b", re.IGNORECASE)

CLASSIFICATIONS = {
    "negative-status": "an explicit unsupported/unavailable/removed status",
    "protocol-context": "a protocol or implementation description, not a product promise",
    "competitor-context": "a comparison describing another product or protocol",
    "vocabulary": "a linter/style vocabulary or other non-customer text",
    "historical": "a dated or historical record retained for provenance",
    "instructional-counterexample": "an exact, reviewed example of a claim support staff must not send",
}

# Line numbers are deliberately excluded: occurrence identity is stable across
# harmless line moves, as exercised by the focused suite. Every other persisted
# occurrence field is part of the closed payload and must match the re-derived
# corpus exactly.
OCCURRENCE_PAYLOAD_FIELDS = ("id", "path", "text", "class", "reason", "terms")
OCCURRENCE_MANIFEST_FIELDS = set((*OCCURRENCE_PAYLOAD_FIELDS, "line"))

# This is intentionally a path-and-line allowlist, not an ``example`` keyword
# escape hatch.  The template introduces the quoted sentence as a reply agents
# must never send; changing either the location or wording makes it ordinary
# customer-facing copy again and the semantic guard evaluates it.
INSTRUCTIONAL_COUNTEREXAMPLES = {
    (
        "marketing/launch/SUPPORT-RESPONSE-TEMPLATES.md",
        'e.g., "Yes, we support Pants 2.20 — see docs link below."}}',
    ): CLASSIFICATIONS["instructional-counterexample"],
}

# These patterns intentionally describe the *relationship* rather than a fixed
# sentence.  A new sentence such as "CoreLink is fully compatible with Buck2"
# therefore fails even if it was not present when the manifest was generated.
POSITIVE_PATTERNS = (
    re.compile(r"\b(?:buck2|pants)\b(?:\W+\w+){0,3}\W+\b(?:is|are)\s+(?:(?:fully|directly|natively|only)\s+)?(?:supported|compatible|available|enabled|integrated)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b.{0,45}\b(?:fully\s+)?supported\s+by\s+\bcorelink\b", re.I),
    re.compile(r"\b(?:fully\s+supported|compatible|supported)\s+by\s+\bcorelink\b.{0,40}\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:supports?|accepts?|serves?|integrates?\s+with|works?\s+with)\s+(?:the\s+)?\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b.{0,45}\b(?:works?|connects?|runs?)\s+(?:with|on|against)\s+\bcorelink\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b.{0,60}\bcan\s+use\b.{0,40}\bcorelink\b", re.I),
    re.compile(r"\bcorelink\b.{0,60}\b(?:accepts?|supports?|serves?|integrates?\s+with|works?\s+with)\b.{0,60}\b(?:buck2|pants)\b", re.I),
    re.compile(r"\bcorelink\b.{0,60}\b(?:offers?|provides?)\b.{0,40}\b(?:buck2|pants)\b.{0,40}\b(?:integration|compatib(?:ility|le)|support)\b", re.I),
    re.compile(r"\bcorelink\b.{0,60}\b(?:offers?|provides?)\b.{0,40}\b(?:integration|compatib(?:ility|le)|support)\b.{0,40}\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b.{0,50}\b(?:is|are)\b.{0,25}\b(?:native|first[ -]class|built[ -]in)\b.{0,40}\bcorelink\b.{0,40}\b(?:integration|compatib(?:ility|le)|support)\b", re.I),
    re.compile(r"\buse\b.{0,20}\b(?:buck2|pants)\b.{0,30}\bwith\b.{0,30}\bcorelink\b.{0,60}\bremote\s+cache\b", re.I),
    re.compile(r"\buse\b.{0,20}\bcorelink\b.{0,60}\bremote\s+cache\b.{0,40}\b(?:for|with)\b.{0,20}\b(?:buck2|pants)\b", re.I),
    re.compile(r"\bcorelink\b.{0,60}\b(?:has|includes?|ships?\s+with|comes\s+with)\b.{0,40}\b(?:native|first[ -]class|built[ -]in)\b.{0,40}\b(?:buck2|pants)\b.{0,40}\b(?:integration|compatib(?:ility|le)|support)\b", re.I),
    re.compile(r"\bcorelink\b.{0,60}\b(?:has|includes?|ships?\s+with|comes\s+with)\b.{0,40}\b(?:buck2|pants)\b.{0,40}\b(?:native|first[ -]class|built[ -]in)\b.{0,40}\b(?:integration|compatib(?:ility|le)|support)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b.{0,60}\b(?:integration|compatib(?:ility|le)|support)\b.{0,40}\b(?:available|supported|works?|enabled)\b.{0,40}\b(?:on|by|with|via)\s+corelink\b", re.I),
    re.compile(r"\b(?:connect|configure|point|pair)\b.{0,20}\b(?:buck2|pants)\b.{0,50}\b(?:to|with|at|against)\s+corelink\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b.{0,50}\buses?\b.{0,50}\bcorelink\b", re.I),
    # Keep the subject/object direction explicit for ordinary product claims.
    # The older verb-first patterns do not see ``CoreLink is compatible with
    # Buck2`` or the reversed ``Buck2 supports CoreLink`` form.
    re.compile(r"\bcorelink\b\W+(?:is|are)\W+(?:(?:fully|directly|natively|only)\W+|not\W+only\W+)?(?:compatible|integrated|supported|available|enabled)\W+(?:with|for|by)\W+(?:\w+\W+){0,1}\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b\W+(?:supports?|accepts?|integrates?\W+with|connects?\W+(?:to|with)|works?\W+with)\W+(?:the\W+)?\bcorelink\b", re.I),
    re.compile(r"\bcorelink\b\W+(?:connects?\W+(?:to|with)|integrates?\W+with)\W+(?:the\W+)?\b(?:buck2|pants)\b", re.I),
    # Plain noun phrases and setup instructions are also product promises;
    # they occur naturally in headings, catalog cards, and how-to copy.
    re.compile(r"\bcorelink\b\W+(?:has|have|includes?|offers?|provides?|ships?\W+with|comes\W+with)\W+(?:the\W+)?\b(?:buck2|pants)\b\W+(?:integration|compatib(?:ility|le)|support)\b", re.I),
    re.compile(r"\bcorelink\b\W+(?:integration|compatib(?:ility|le)|support)\W+(?:for|with|of)\W+(?:the\W+)?\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b\W+(?:integration|compatib(?:ility|le)|support)\W+(?:for|with|of)\W+\bcorelink\b", re.I),
    re.compile(r"\b(?:use|connect|configure|point|pair)\b.{0,20}\bcorelink\b.{0,30}\b(?:to|with|for|at|against)\b.{0,20}(?:the\W+)?\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:use|connect|configure|point|pair)\b.{0,20}(?:the\W+)?\b(?:buck2|pants)\b.{0,30}\b(?:to|with|for|at|against)\b.{0,20}\bcorelink\b", re.I),
    re.compile(r"\bcorelink\b.{0,45}\bfirst[ -]class\b.{0,30}\b(?:with|for)\W+(?:the\W+)?\b(?:buck2|pants)\b", re.I),
    # Common relationship wording which is deliberately expressed as a small
    # grammar, rather than another one-off sentence allowlist.  These verbs
    # describe the same CoreLink↔client support relation in either direction.
    re.compile(r"\bcorelink\b\W+(?:can\s+be\s+used\s+with|interoperates?\s+with|works?\s+(?:with|together\s+with)|handles?|exposes?|includes?|supports?|accepts?|integrates?\s+with)\W+(?:the\W+|a\W+)?\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b\W+(?:can\s+be\s+used\s+with|interoperates?\s+with|works?\s+(?:with|together\s+with)|handles?|exposes?|includes?|supports?|accepts?|integrates?\s+with)\W+\bcorelink\b", re.I),
    re.compile(r"\bcorelink\b\W+and\W+(?:the\W+)?\b(?:buck2|pants)\b\W+(?:work|works|interoperate|interoperates|connect|connects|integrate|integrates|are)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b\W+and\W+\bcorelink\b\W+(?:work|works|interoperate|interoperates|connect|connects|integrate|integrates|are)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b\W+(?:support|integration|compatibility)\W+is\W+(?:provided|offered|included|exposed|available)\W+by\W+\bcorelink\b", re.I),
    re.compile(r"\bcorelink\b\W+(?:provides?|offers?)\W+(?:support|integration|compatibility)\W+(?:for|with)\W+\b(?:buck2|pants)\b", re.I),
    re.compile(r"\bcorelink\b\W+(?:funktioniert|kann\W+mit)\W+(?:mit\W+)?\b(?:buck2|pants)\b", re.I),
    re.compile(r"\bcorelink\b\W+bietet\W+(?:unterstützung|support)\W+(?:für|von)\W+\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b\W*[-–—]?\W*(?:unterstützung|support)\W+(?:ist\W+)?(?:über|von)\W+\bcorelink\b", re.I),
    re.compile(r"\bcorelink\b\W+(?:funciona|puede\W+usarse)\W+(?:con\W+)?\b(?:buck2|pants)\b", re.I),
    re.compile(r"\bcorelink\b\W+ofrece\W+(?:soporte|compatibilidad)\W+(?:para|con)\W+\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:soporte|compatibilidad)\W+de\W+\b(?:buck2|pants)\b\W+(?:está\W+)?disponible\W+(?:en|para)\W+\bcorelink\b", re.I),
    re.compile(r"\bcorelink\b\W+(?:funciona|pode\W+ser\W+usado)\W+(?:com\W+)?\b(?:buck2|pants)\b", re.I),
    re.compile(r"\bcorelink\b\W+oferece\W+(?:suporte|compatibilidade)\W+(?:a|com)\W+\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:suporte|compatibilidade)\W+(?:a|do|de)\W+\b(?:buck2|pants)\b\W+(?:está\W+)?disponível\W+(?:no|em|para)\W+\bcorelink\b", re.I),
    # The published locale trees contain these short, unambiguous forms.  Keep
    # the vocabulary deliberately narrow so translated negative prose remains
    # a valid status statement instead of becoming a generic machine-translation
    # parser with broad false positives.
    re.compile(r"\bcorelink\b\W+ist\W+(?:(?:vollständig|direkt|nativ|nur)\W+)?(?:mit|für)\W+\b(?:buck2|pants)\b\W+(?:kompatibel|integriert|unterstützt|verfügbar|aktiviert)\b", re.I),
    re.compile(r"\bcorelink\b\W+(?:unterstützt|akzeptiert|integriert)\W+\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b\W+(?:unterstützt|akzeptiert|integriert)\W+\bcorelink\b", re.I),
    re.compile(r"\bcorelink\b\W+(?:es|está)\W+(?:(?:totalmente|directamente|nativamente|solo)\W+)?(?:compatible|integrado|disponible)\W+con\W+(?:el\W+)?\b(?:buck2|pants)\b", re.I),
    re.compile(r"\bcorelink\b\W+(?:admite|soporta|integra)\W+(?:el\W+)?\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b\W+(?:es|está)\W+(?:compatible|integrado)\W+con\W+\bcorelink\b", re.I),
    re.compile(r"\bcorelink\b\W+(?:é|está)\W+(?:(?:totalmente|diretamente|nativamente|apenas)\W+)?(?:compatível|integrado|disponível)\W+com\W+(?:o\W+)?\b(?:buck2|pants)\b", re.I),
    re.compile(r"\bcorelink\b\W+(?:suporta|aceita|integra)\W+(?:o\W+)?\b(?:buck2|pants)\b", re.I),
    re.compile(r"\b(?:buck2|pants)\b\W+(?:é|está)\W+(?:compatível|integrado)\W+com\W+\bcorelink\b", re.I),
)
HTML_METADATA_RE = re.compile(
    r"\b(?:data-[\w:-]+|aria-[\w:-]+|title|alt|value|content)\s*=\s*(?:\"([^\"]*)\"|'([^']*)')",
    re.I | re.S,
)
NEGATION_RE = re.compile(
    r"\b(?:not(?!\s+(?:only|merely|just)\b)|no|cannot|can't|never|without|unsupported|unavailable|removed|doesn['’]?t|does not|won['’]?t|nicht|kein(?:e|en|er|es)?|sin|sem|não)\b",
    re.I,
)
ROADMAP_RE = re.compile(
    r"\b(?:roadmap|planned|plan(?:s|ned)?|future|will|would|could|may|might|target|timeline|geplant|zukünftig|künftig|wird|würde|könnte|soll|hoja\s+de\s+ruta|planificado|futuro|futura|será|podría|roteiro|planejado|planejada|futuro|futura|será|poderia)\b",
    re.I,
)
ROADMAP_PREFIX_RE = re.compile(
    r"\b(?:roadmap|planned|plan(?:s|ned)?|future|target|timeline|will|would|could|may|might|geplant|zukünftig|künftig|wird|würde|könnte|soll|hoja\s+de\s+ruta|planificado|futuro|futura|será|podría|roteiro|planejado|planejada|poderia)\b"
    r"(?:\W+\w+){0,2}\W*$",
    re.I,
)
CLAUSE_CONJUNCTION_RE = re.compile(
    r"\b(?:and|or|nor|but|however|although|while|yet|aber|und|oder|jedoch|obwohl|pero|y|o|sin\s+embargo|aunque|mas|e|ou|porém|embora)\b",
    re.I,
)
CORELINK_RE = re.compile(r"\bcorelink\b", re.I)
ACTOR_RE = re.compile(r"\b(?:corelink|buildbuddy|engflow|buildbarn|nativelink|bazel-remote|competitors?)\b", re.I)
COMPETITOR_RE = re.compile(r"\b(?:buildbuddy|engflow|buildbarn|nativelink|bazel-remote)\b", re.I)
RELATION_VERB_RE = re.compile(
    r"\b(?:support(?:s|ed)?|accept(?:s|ed)?|serv(?:e|es|ed)?|integrat(?:e|es|ed)?|"
    r"work(?:s|ed)?|offer(?:s|ed)?|provid(?:e|es|ed)?|has|have|includes?|ships?|comes?)\b",
    re.I,
)


def _is_clause_separator(text: str, index: int) -> bool:
    character = text[index]
    category = unicodedata.category(character)
    if category[:1] not in {"P", "S"}:
        return False
    if character in {"'", "’"}:
        previous = text[index - 1] if index else ""
        following = text[index + 1] if index + 1 < len(text) else ""
        if previous.isalnum() and following.isalnum():
            return False
    if character == "-":
        previous = text[index - 1] if index else ""
        following = text[index + 1] if index + 1 < len(text) else ""
        if previous.isalnum() and following.isalnum():
            return False
    return True


def _has_clause_boundary(text: str) -> bool:
    """Return whether text separates two independent claims.

    Inventory entries are line-oriented, so a full Markdown parser would not
    make the semantic check more reliable.  Instead, inspect the text directly
    between the nearest negation and positive match: any Unicode punctuation or
    symbol (including slash, en/em dash, and parentheses), or a coordinating
    conjunction, makes the negation belong to a neighbouring clause.  Apostrophe
    characters inside words remain part of contractions such as ``doesn't``.
    """
    if CLAUSE_CONJUNCTION_RE.search(text):
        return True
    return any(_is_clause_separator(text, index) for index in range(len(text)))


def _clause_start(text: str, position: int) -> int:
    """Find the start of the clause containing ``position``."""
    start = 0
    for index in range(position):
        if _is_clause_separator(text, index):
            start = index + 1
    for conjunction in CLAUSE_CONJUNCTION_RE.finditer(text, 0, position):
        start = max(start, conjunction.end())
    return start


def _clause_end(text: str, position: int) -> int:
    """Find the end of the clause containing ``position``."""
    for index in range(position, len(text)):
        if _is_clause_separator(text, index):
            return index
    conjunction = CLAUSE_CONJUNCTION_RE.search(text, position)
    return conjunction.start() if conjunction else len(text)


def _match_crosses_clause(text: str, match: re.Match[str]) -> bool:
    """Reject a broad pattern match that crossed a structural separator."""
    return any(_is_clause_separator(text, index) for index in range(match.start(), match.end()))


def _match_is_corelink_claim(text: str, match: re.Match[str]) -> bool:
    """Require the matched claim's local subject or object to name CoreLink."""
    clause_start = _clause_start(text, match.start())
    relation = RELATION_VERB_RE.search(match.group(0))
    if relation:
        verb_position = match.start() + relation.start()
        prefix = text[clause_start:verb_position]
        actors = list(ACTOR_RE.finditer(prefix))
        if actors:
            actor = actors[-1]
            if any(
                _is_clause_separator(text, index) and text[index] != ","
                for index in range(clause_start + actor.end(), verb_position)
            ):
                return False
            return actor.group(0).lower() == "corelink"
        if any(_is_clause_separator(text, index) for index in range(clause_start, verb_position)):
            return False
    suffix = text[match.end() : _clause_end(text, match.end())]
    if re.search(r"\b(?:by|with|against|for|on|via|through)\s+corelink\b", suffix, re.I):
        return True
    return bool(CORELINK_RE.search(match.group(0)))


def _match_is_recognized_competitor_claim(text: str, match: re.Match[str]) -> bool:
    """Allow only a known third-party subject in contextual inventory lines."""
    clause_start = _clause_start(text, match.start())
    relation = RELATION_VERB_RE.search(match.group(0))
    if relation:
        verb_position = match.start() + relation.start()
        prefix = text[clause_start:verb_position]
        actors = list(ACTOR_RE.finditer(prefix))
        if actors:
            actor = actors[-1].group(0)
            if any(
                _is_clause_separator(text, index) and text[index] != ","
                for index in range(clause_start + actors[-1].end(), verb_position)
            ):
                return False
            return bool(COMPETITOR_RE.fullmatch(actor))
    suffix = text[match.end() : _clause_end(text, match.end())]
    if re.search(r"\b(?:by|with|against|for|on|via|through)\s+" + COMPETITOR_RE.pattern, suffix, re.I):
        return True
    return bool(COMPETITOR_RE.search(match.group(0)) and not CORELINK_RE.search(match.group(0)))


def _negation_is_local(text: str, match: re.Match[str]) -> bool:
    """Check only negation attached to this positive match."""
    if NEGATION_RE.search(text[match.start() : match.end()]):
        return True
    previous_negations = list(NEGATION_RE.finditer(text, 0, match.start()))
    if not previous_negations:
        return False
    nearest = previous_negations[-1]
    # A sentence-leading answer such as ``No: CoreLink supports Buck2`` is a
    # negated statement, while ``CoreLink does not support Buck2; CoreLink
    # supports Pants`` is two clauses and must remain fail-closed.  Only treat
    # a bare leading label followed by a colon as local punctuation here.
    if nearest.start() == 0 and re.fullmatch(r"\s*:\s*", text[nearest.end() : match.start()]):
        return True
    return not _has_clause_boundary(text[nearest.end() : match.start()])


def _roadmap_is_local(text: str, match: re.Match[str]) -> bool:
    """Keep explicit future/roadmap statements out of the shipped-claim gate.

    A roadmap heading immediately before a claim (``Roadmap: CoreLink support
    for Buck2``) and a modal inside the claim (``CoreLink will support Buck2``)
    describe planned state rather than current compatibility.  Do not inspect
    arbitrary text after the match: ``CoreLink supports Buck2; the roadmap ...``
    still contains an ordinary present-tense claim that must fail.
    """
    prefix = text[max(0, match.start() - 48) : match.start()]
    matched = match.group(0)
    if ROADMAP_RE.search(matched) or ROADMAP_PREFIX_RE.search(prefix):
        return True
    # A noun-phrase match can end at ``Buck2`` while its future qualifier is
    # immediately after it (``CoreLink support for Buck2 is a future roadmap
    # item``).  Inspect only the remainder of this same clause; punctuation
    # first means a later independent sentence cannot launder a live claim.
    suffix = text[match.end() :]
    for index, character in enumerate(suffix):
        if _is_clause_separator(suffix, index):
            suffix = suffix[:index]
            break
    return bool(ROADMAP_RE.search(suffix))


def published_files(root: Path) -> list[Path]:
    paths: list[Path] = []
    for rel in ROOT_FILES:
        path = root / rel
        if path.is_file() and path.suffix.lower() in EXTENSIONS:
            paths.append(path)
    for rel in PUBLISHED_METADATA_FILES:
        path = root / rel
        if path.is_file():
            paths.append(path)
    for rel in ROOTS:
        directory = root / rel
        if directory.is_dir():
            for current, directories, filenames in os.walk(directory):
                directories[:] = sorted(name for name in directories if name not in WALK_EXCLUDED)
                paths.extend(
                    Path(current) / name
                    for name in sorted(filenames)
                    if Path(name).suffix.lower() in EXTENSIONS
                )
    return sorted(set(paths))


def occurrence_id(path: str, text: str, ordinal: int) -> str:
    """Stable across line moves, but not across an unreviewed text change."""
    payload = f"{path}\0{text.strip()}\0{ordinal}".encode()
    return hashlib.sha256(payload).hexdigest()[:24]


# Published Markdown and HTML are rendered after inline markup, entities, and
# attribute values have been interpreted.  Semantic checks must inspect that
# rendered-ish form rather than letting ``**``/links/``&nbsp;`` become fake
# clause boundaries.  This is deliberately a small normalizer, not a second
# Markdown renderer: it only removes syntax which can split a claim while
# preserving ordinary punctuation used to separate independent claims.
MARKDOWN_LINK_RE = re.compile(r"!?\[([^\]]+)\]\([^\n]*?\)")
MARKDOWN_REFERENCE_RE = re.compile(r"!?\[([^\]]+)\]\[[^\]]*\]")
HTML_TAG_RE = re.compile(r"<[^>]*>", re.S)
MARKDOWN_DECORATION_RE = re.compile(r"[`*~]+")
MARKDOWN_EMPHASIS_UNDERSCORE_RE = re.compile(r"(?<!\w)_(?=\w)|(?<=\w)_(?!\w)")


def normalize_published_text(text: str) -> str:
    """Return a claim-searchable approximation of rendered copy."""
    text = html.unescape(text)
    text = HTML_TAG_RE.sub(" ", text)
    text = MARKDOWN_LINK_RE.sub(r"\1", text)
    text = MARKDOWN_REFERENCE_RE.sub(r"\1", text)
    text = MARKDOWN_DECORATION_RE.sub("", text)
    text = MARKDOWN_EMPHASIS_UNDERSCORE_RE.sub("", text)
    # Quotes are inline formatting in rendered copy, not a clause boundary:
    # ``CoreLink &quot;supports&quot; Buck2`` must remain one claim.  Sentence
    # boundaries still come from punctuation such as ``;`` or ``.``.
    text = re.sub(r'["“”]', "", text)
    return re.sub(r"\s+", " ", text).strip()


def _contains_published_term(text: str) -> bool:
    """Fast-path raw copy before invoking the rendered-copy normalizer.

    The published corpus is large, while only a few hundred lines mention the
    controlled terms. Avoiding HTML/Markdown normalization for ordinary lines
    keeps the verifier comfortably bounded on a developer laptop.
    """
    lowered = text.casefold()
    if "buck2" in lowered or "pants" in lowered or "grpc" in lowered:
        return True
    # Formatting markers are common in Markdown; only normalize lines whose
    # raw bytes contain a plausible split/entity fragment. This preserves the
    # encoded-attribute fallback below without paying an HTML parse per line.
    if not any(fragment in lowered for fragment in ("uck", "pan", "grpc", "&#")):
        return False
    if "&#" in lowered:
        return bool(TERM_RE.search(normalize_published_text(text)))
    if not re.search(r"(?:b.{0,8}uck|pan.{0,8}ts|gr.{0,8}pc)", text, re.I):
        return False
    return bool(TERM_RE.search(normalize_published_text(text)))


def _line_has_published_term(line: str) -> bool:
    """Include visible HTML attributes whose term is entity encoded."""
    if _contains_published_term(line):
        return True
    if "=" not in line or "&" not in line or not ("&#" in line or "buck" in line.casefold() or "pants" in line.casefold()):
        return False
    return any(
        TERM_RE.search(normalize_published_text(_metadata_value(match)))
        for match in HTML_METADATA_RE.finditer(line)
    )


def _metadata_value(match: re.Match[str]) -> str:
    return match.group(1) if match.group(1) is not None else match.group(2)


CONTINUATION_END_RE = re.compile(
    r"(?:\b(?:a|an|and|are|can|compatible|connect(?:s|ed)?|for|integrat(?:e|ed|es)?|is|of|on|or|pair|point|support(?:s|ed)?|the|to|use|with)\b|[,/:;])$",
    re.I,
)
CONTINUATION_START_RE = re.compile(
    r"^(?:\b(?:buck2|pants|corelink|a|an|and|are|can|compatible|connect(?:s|ed)?|for|integrat(?:e|ed|es)?|is|of|on|or|pair|point|support(?:s|ed)?|the|to|use|with)\b)",
    re.I,
)
CONTINUATION_BRIDGE_RE = re.compile(
    r"^(?:a|an|the|our|your|its|their|this|that|with|to|for|as|of|on|at|via|only|directly|natively|remote|cache|client|backend|service|build|tool)(?:\s+(?:a|an|the|our|your|its|their|this|that|with|to|for|as|of|on|at|via|only|directly|natively|remote|cache|client|backend|service|build|tool)){0,3}$",
    re.I,
)


def _is_continuation_bridge(text: str) -> bool:
    """Return whether a short term-free line can sit inside a soft-wrapped claim."""
    if len(text) > 80:
        return False
    normalized = normalize_published_text(text)
    if not normalized:
        return False
    if CONTINUATION_BRIDGE_RE.fullmatch(normalized):
        return True
    # A renderer may wrap at any short grammatical connector (``both``,
    # ``all``, ``each``), and maintaining a growing English-word allowlist is
    # both brittle and easy to bypass.  Permit one lowercase word as a bridge;
    # multi-word prose remains a hard boundary, so unrelated/distant clauses
    # are not folded into the claim view.
    return bool(re.fullmatch(r"[a-z]+", normalized))


def _can_join_lines(left: str, right: str) -> bool:
    left_has_term = bool(re.search(r"\b(?:corelink|buck2|pants|support(?:s|ed)?|compatib\w*|connect\w*|integrat\w*)\b", left, re.I))
    right_has_term = bool(re.search(r"\b(?:corelink|buck2|pants|support(?:s|ed)?|compatib\w*|connect\w*|integrat\w*)\b", right, re.I))
    left_bridge = not left_has_term and _is_continuation_bridge(left)
    right_bridge = not right_has_term and _is_continuation_bridge(right)
    if not (left_has_term or left_bridge) or not (right_has_term or right_bridge):
        return False
    if not left_has_term and not right_has_term:
        return False
    left = normalize_published_text(left)
    right = normalize_published_text(right)
    # A term-bearing line which starts its own copular sentence is a distant
    # clause, not the object of a preceding wrapped relation (for example,
    # ``CoreLink supports`` / ``unrelated`` / ``Buck2 is a separate tool``).
    # The same line is still scanned normally, and explicit support wording is
    # therefore never weakened by this boundary check.
    if left_bridge and right_has_term and re.match(r"^(?:buck2|pants)\b\s+(?:is|are|was|were)\b", right, re.I):
        return False
    return bool(
        left
        and right
        and (CONTINUATION_END_RE.search(left) or left_bridge)
        and (CONTINUATION_START_RE.search(right) or right_bridge)
    )


def _semantic_texts_by_line(lines: list[str]) -> dict[int, list[str]]:
    """Build normalized same-line, continuation, and attribute claim views."""
    by_line: dict[int, list[str]] = {}

    def add(line_numbers: Iterable[int], text: str) -> None:
        if not _contains_published_term(text):
            return
        text = normalize_published_text(text)
        if not text:
            return
        for line_number in line_numbers:
            if text not in by_line.setdefault(line_number, []):
                by_line[line_number].append(text)

    term_lines = {
        line_number
        for line_number, line in enumerate(lines, 1)
        if _line_has_published_term(line)
    }
    for line_number in term_lines:
        add((line_number,), lines[line_number - 1])

    # Join only syntactic continuations.  Newlines between complete sentences
    # remain boundaries, while a wrapped ``CoreLink is compatible with`` /
    # ``Buck2`` claim is evaluated as one rendered sentence.
    candidate_indexes = {
        index
        for line_number in term_lines
        for index in (line_number - 3, line_number - 2, line_number - 1, line_number)
        if 0 <= index < len(lines) - 1
    }
    for index in sorted(candidate_indexes):
        if _can_join_lines(lines[index], lines[index + 1]):
            add((index + 1, index + 2), " ".join(lines[index : index + 2]))
        if index + 2 < len(lines) and _can_join_lines(lines[index], lines[index + 1]) and _can_join_lines(lines[index + 1], lines[index + 2]):
            add((index + 1, index + 2, index + 3), " ".join(lines[index : index + 3]))

    raw = "\n".join(lines)
    raw_lower = raw.casefold()
    if any(term in raw_lower for term in ("buck2", "pants", "grpc")) or "&#" in raw:
        for match in HTML_METADATA_RE.finditer(raw):
            start_line = raw.count("\n", 0, match.start()) + 1
            end_line = raw.count("\n", 0, match.end()) + 1
            add(range(start_line, end_line + 1), _metadata_value(match))
    return by_line


def classify(path: str, text: str) -> tuple[str, str]:
    lower = text.lower()
    if (path, text.strip()) in INSTRUCTIONAL_COUNTEREXAMPLES:
        return "instructional-counterexample", INSTRUCTIONAL_COUNTEREXAMPLES[(path, text.strip())]
    if any(token in path.lower() for token in ("styles", "accept.txt", "translation-quality")):
        return "vocabulary", CLASSIFICATIONS["vocabulary"]
    if any(token in lower for token in ("competitor", "competitors", "protocol", "rfc-", "bazel-remote", "buildbuddy", "engflow", "buildbarn", "nativelink")) or "/vs-" in path.lower():
        return "competitor-context", CLASSIFICATIONS["competitor-context"]
    if any(token in lower for token in ("historical", "archive", "superseded", "was removed", "dated")):
        return "historical", CLASSIFICATIONS["historical"]
    if NEGATION_RE.search(lower):
        return "negative-status", CLASSIFICATIONS["negative-status"]
    return "protocol-context", CLASSIFICATIONS["protocol-context"]


def collect_occurrences(root: Path, *, include_semantics: bool = False) -> list[dict[str, object]]:
    occurrences: list[dict[str, object]] = []
    duplicate_ordinals: dict[tuple[str, str], int] = {}
    for path in published_files(root):
        rel = path.relative_to(root).as_posix()
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except UnicodeDecodeError:
            continue
        semantic_by_line = _semantic_texts_by_line(lines) if include_semantics else {}
        for line_number, line in enumerate(lines, 1):
            # Search the rendered-ish form as well as source bytes.  Numeric or
            # named entities in an HTML attribute (``&#66;uck2``) otherwise
            # disappear before they can enter the manifest or semantic guard.
            has_term = _line_has_published_term(line)
            if not has_term:
                continue
            key = (rel, line.strip())
            ordinal = duplicate_ordinals.get(key, 0)
            duplicate_ordinals[key] = ordinal + 1
            category, reason = classify(rel, line)
            entry: dict[str, object] = {
                "id": occurrence_id(rel, line, ordinal),
                "path": rel,
                "line": line_number,
                "text": line.strip(),
                "class": category,
                "reason": reason,
                "terms": sorted({match.group(0).lower() for match in TERM_RE.finditer(line)}),
            }
            if include_semantics:
                entry["_semantic_texts"] = semantic_by_line.get(line_number, [])
            occurrences.append(entry)
    return occurrences


def build_manifest(root: Path) -> dict[str, object]:
    files = published_files(root)
    occurrences = collect_occurrences(root)
    # A sorted-line digest is intentionally invariant to harmless line moves,
    # while still making deletion/truncation of context (including lines that do
    # not mention Buck2/Pants/gRPC) a hard stop until the population is reviewed.
    file_hashes = {
        path.relative_to(root).as_posix(): hashlib.sha256(
            "\n".join(sorted(path.read_text(encoding="utf-8").splitlines())).encode("utf-8")
        ).hexdigest()
        for path in files
    }
    return {
        "schema": SCHEMA,
        "scope": {
            "roots": list(ROOTS),
            "root_files": list(ROOT_FILES),
            "metadata_files": list(PUBLISHED_METADATA_FILES),
            "extensions": sorted(EXTENSIONS),
            "excluded": {
                "internal_wiki": "docs/knowledge and generated OKF material are sources of truth, not published claims",
                "history": "CHANGELOG.md and changelog.d are release history, not the deployed docs surface",
                "source": "Rust/source files are product evidence; B-094 checks the published copy only",
                "metadata": "only explicit Docusaurus metadata emitters are included; implementation-only TypeScript remains source",
                "build": "node_modules, build, dist, and generated output are not authored surfaces",
            },
        },
        "population": {
            "files": [path.relative_to(root).as_posix() for path in files],
            "file_hashes": file_hashes,
            "occurrence_ids": [entry["id"] for entry in occurrences],
        },
        "classifications": CLASSIFICATIONS,
        "occurrences": occurrences,
    }


def positive_claims(occurrences: Iterable[dict[str, object]]) -> list[str]:
    failures: list[str] = []
    for entry in occurrences:
        text = str(entry["text"])
        # The exact reviewed template is an instructional counterexample, not
        # customer-facing copy.  Other contextual classes are checked per
        # positive match so ambient words such as "protocol" or "historical"
        # cannot waive a separate CoreLink claim.
        if entry.get("class") == "instructional-counterexample":
            continue
        # Searchable HTML metadata is customer-visible copy too. Evaluate its
        # value independently so surrounding markup (for example a separate
        # ``data-status=shipped`` or an unrelated clause) cannot hide a claim.
        # Mask the attribute values in the source copy first: this makes the
        # metadata pass load-bearing instead of accidentally matching the same
        # bytes through the surrounding HTML source.
        metadata_matches = list(HTML_METADATA_RE.finditer(text))
        visible_text = HTML_METADATA_RE.sub(lambda match: match.group(0)[: match.group(0).find("=")], text)
        claim_texts = [visible_text]
        claim_texts.extend(normalize_published_text(_metadata_value(match)) for match in metadata_matches)
        claim_texts.extend(str(value) for value in entry.get("_semantic_texts", []))
        claim_texts = list(dict.fromkeys(claim_texts))
        for claim_text in claim_texts:
            for pattern in POSITIVE_PATTERNS:
                for match in pattern.finditer(claim_text):
                    if _match_crosses_clause(claim_text, match):
                        continue
                    if entry.get("class") in {"competitor-context", "historical", "vocabulary"}:
                        if _match_is_corelink_claim(claim_text, match):
                            pass
                        elif _match_is_recognized_competitor_claim(claim_text, match):
                            continue
                    if _roadmap_is_local(claim_text, match):
                        continue
                    if _negation_is_local(claim_text, match):
                        continue
                    failures.append(f"{entry['path']}:{entry['line']}: positive support claim: {text}")
                    break
                else:
                    continue
                break
            else:
                continue
            break
    return failures


def _canonical_occurrence_payload(entry: object, *, manifest_entry: bool = False) -> tuple[object, ...] | None:
    """Return the persisted occurrence payload, ignoring derived semantics and line moves."""
    if not isinstance(entry, dict):
        return None
    if manifest_entry and set(entry) != OCCURRENCE_MANIFEST_FIELDS:
        return None
    if any(field not in entry for field in OCCURRENCE_PAYLOAD_FIELDS):
        return None
    return tuple(entry[field] for field in OCCURRENCE_PAYLOAD_FIELDS)


def validate(root: Path, manifest_path: Path = MANIFEST) -> list[str]:
    failures: list[str] = []
    if not manifest_path.is_file():
        return [f"HALT: inventory manifest is missing: {manifest_path}"]
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        return [f"HALT: inventory manifest cannot be parsed: {exc}"]
    if manifest.get("schema") != SCHEMA:
        failures.append(f"HALT: unsupported inventory schema: {manifest.get('schema')!r}")
    occurrences = collect_occurrences(root, include_semantics=True)
    expected = manifest.get("occurrences")
    if not isinstance(expected, list) or not expected:
        failures.append("HALT: inventory has no occurrence population")
        expected = []
    if not occurrences:
        failures.append("HALT: published corpus is empty or truncated; no Buck2/Pants/gRPC occurrences were parsed")
    population = manifest.get("population")
    expected_files = set(population.get("files", [])) if isinstance(population, dict) else set()
    actual_files = {path.relative_to(root).as_posix() for path in published_files(root)}
    if not expected_files:
        failures.append("HALT: inventory has no published-file population")
    if expected_files - actual_files:
        failures.append("HALT: published file corpus is truncated; inventoried file(s) disappeared")
    if actual_files - expected_files:
        failures.append("HALT: new published file(s) are outside the inventory; re-derive and review the manifest")
    expected_hashes = population.get("file_hashes") if isinstance(population, dict) else None
    if not isinstance(expected_hashes, dict) or not expected_hashes:
        failures.append("HALT: inventory has no file-content population")
    else:
        actual_hashes = {
            path.relative_to(root).as_posix(): hashlib.sha256(
                "\n".join(sorted(path.read_text(encoding="utf-8").splitlines())).encode("utf-8")
            ).hexdigest()
            for path in published_files(root)
        }
        if expected_hashes != actual_hashes:
            failures.append("HALT: published file content/population changed; review for truncation and re-derive the manifest")
    expected_ids = {str(entry.get("id")) for entry in expected if isinstance(entry, dict)}
    actual_ids = {str(entry["id"]) for entry in occurrences}
    missing = sorted(expected_ids - actual_ids)
    unknown = sorted(actual_ids - expected_ids)
    if missing:
        failures.append(f"HALT: {len(missing)} inventoried occurrence(s) disappeared; re-derive and review the manifest")
    if unknown:
        failures.append(f"HALT: {len(unknown)} new or changed occurrence(s) are not classified in the manifest")
    expected_payload = [
        _canonical_occurrence_payload(entry, manifest_entry=True)
        for entry in expected
    ]
    actual_payload = [_canonical_occurrence_payload(entry) for entry in occurrences]
    if expected_payload != actual_payload:
        failures.append(
            "HALT: occurrence manifest payload differs from the published corpus; review and re-derive the inventory"
        )
    expected_classes = set(CLASSIFICATIONS)
    for entry in expected:
        if not isinstance(entry, dict) or entry.get("class") not in expected_classes:
            failures.append(f"HALT: occurrence has no valid classification: {entry!r}")
    failures.extend(positive_claims(occurrences))

    anchor = root / "crates/corelink-container/src/routes/bazel_v2.rs"
    if not anchor.is_file() or "Buck2 cannot use these routes" not in anchor.read_text(encoding="utf-8"):
        failures.append("HALT: product anchor no longer records that Buck2 cannot use the REST routes; re-evaluate B-094")
    return failures


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=REPO_ROOT)
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    parser.add_argument("--write-manifest", action="store_true")
    args = parser.parse_args(argv)
    if args.write_manifest:
        args.manifest.write_text(json.dumps(build_manifest(args.root), indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(f"wrote B-094 inventory: {args.manifest}")
        return 0
    failures = validate(args.root, args.manifest)
    if failures:
        for failure in failures:
            print(f"B-094 INVALID: {failure}", file=sys.stderr)
        return 1
    print("B-094 OK: published occurrence inventory is complete and contains no positive Buck2/Pants support claim")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
