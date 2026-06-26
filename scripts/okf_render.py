#!/usr/bin/env python3
"""
Render the OKF-CoreLink knowledge bundle (`docs/knowledge/`) into a SINGLE
self-contained, dependency-free static HTML visualizer.

This is the OKF *consumption* surface (Campaign 5 / WP-B). It is the read-side
mirror of `scripts/okf_index.py` (the machine-generated listing): instead of a
Markdown index, it walks the same bundle and emits one offline HTML page that a
human can open with no backend, no CDN, and no build step — inline CSS + vanilla
JS only, mirroring OKF's reference static visualizer.

What the page gives you:
  1. A left nav of every concept, grouped by taxonomy directory (profile §1.1
     order), each showing its `type` + `title`.
  2. A client-side search/filter box (title / type / tag / concept-id), vanilla JS.
  3. A per-concept rendered view: the Markdown body rendered to HTML, plus the
     concept's `source_files`, its `# Citations`, and its bundle-relative
     cross-links — all cross-links clickable to other concepts in-page.
  4. A cross-link GRAPH view: nodes = concepts, edges = the bundle-relative
     `/dir/x.md` links between them, drawn as an inline SVG (a deterministic
     fixed-iteration force layout) plus a grouped adjacency list. No deps.

The output lives at `docs/okf-wiki-site/index.html` — deliberately OUTSIDE
`docs/knowledge/` so the validator (`scripts/validate_okf.py`) never mistakes it
for a bundle concept.

Pure stdlib, no LLM, deterministic: running it twice produces byte-identical HTML.

Usage:
    python3 scripts/okf_render.py            # regenerate the site in place
    python3 scripts/okf_render.py --check     # exit 1 if the site is out of date

Exit:
    0  site is current (or was regenerated).
    1  --check and the site differs from the freshly generated content.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
BUNDLE = REPO_ROOT / "docs" / "knowledge"
SITE_DIR = REPO_ROOT / "docs" / "okf-wiki-site"
SITE = SITE_DIR / "index.html"

# Reserved root files (profile §1) — never concepts. Mirrors okf_index.py.
RESERVED = {"index.md", "log.md"}
# Subdir holding immutable cited artifacts, not concepts (profile §1).
NON_CONCEPT_DIRS = {"references"}

# Frozen taxonomy order (profile §1.1). Dirs not in this list sort after, alpha.
TAXONOMY_ORDER = [
    "planes",
    "surfaces",
    "auth",
    "storage",
    "tenancy",
    "flows",
    "crates",
    "adr",
    "ops",
    "security",
    "compliance",
    "testing",
    "launch",
]

FRONT_MATTER_RE = re.compile(r"^---\n(.*?)\n---\n?(.*)$", re.DOTALL)
# Bundle-relative markdown link with a leading slash: [text](/dir/x.md)
LINK_RE = re.compile(r"\[[^\]]*\]\((/[^)\s]+\.md)\)")


# ---------------------------------------------------------------------------
# Minimal, dependency-free frontmatter reader (stdlib only — no PyYAML).
# Handles the flat scalars + block lists + inline [..] lists the schema uses,
# exactly the house approach in scripts/okf_index.py / validate_okf.py.
# ---------------------------------------------------------------------------
def _scalar(v: str):
    v = v.strip()
    if v and v[0] in "\"'":
        q = v[0]
        end = v.find(q, 1)
        return v[1:end] if end != -1 else v[1:]
    return v


def parse_front_matter(text: str) -> tuple[dict, str]:
    """Return (frontmatter_dict, body_markdown)."""
    m = FRONT_MATTER_RE.match(text)
    if not m:
        return {}, text
    block, body = m.group(1), m.group(2)
    data: dict = {}
    cur_key = None
    for raw in block.split("\n"):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        m_item = re.match(r"^(\s+)-\s+(.*)$", raw)
        if m_item and cur_key is not None and isinstance(data.get(cur_key), list):
            data[cur_key].append(_scalar(m_item.group(2)))
            continue
        m_kv = re.match(r"^([A-Za-z0-9_\-]+):\s*(.*)$", raw)
        if m_kv:
            key, val = m_kv.group(1), m_kv.group(2).strip()
            cur_key = key
            if val == "":
                data[key] = []
            elif val.startswith("[") and val.endswith("]"):
                inner = val[1:-1].strip()
                data[key] = [_scalar(x) for x in inner.split(",") if x.strip()]
            else:
                data[key] = _scalar(val)
    return data, body


def _section(body: str, name: str) -> str:
    """Extract the text under a `# <name>` H1 section, up to the next H1."""
    lines = body.split("\n")
    out: list[str] = []
    capturing = False
    for line in lines:
        h = re.match(r"^#\s+(.*?)\s*$", line)
        if h:
            if capturing:
                break
            capturing = h.group(1).strip().lower() == name.lower()
            continue
        if capturing:
            out.append(line)
    return "\n".join(out).strip()


def find_concepts() -> list[dict]:
    """Walk the bundle; return one record per concept (sorted by id)."""
    concepts: list[dict] = []
    valid_ids: set[str] = set()
    raw: list[tuple[Path, str]] = []
    for path in sorted(BUNDLE.rglob("*.md")):
        rel = path.relative_to(BUNDLE)
        parts = rel.parts
        if len(parts) == 1 and parts[0] in RESERVED:
            continue
        if parts[0] in NON_CONCEPT_DIRS:
            continue
        cid = str(rel.with_suffix("")).replace("\\", "/")
        valid_ids.add(cid)
        raw.append((path, cid))

    for path, cid in raw:
        rel = path.relative_to(BUNDLE)
        taxonomy = rel.parts[0] if len(rel.parts) > 1 else "(root)"
        fm, body = parse_front_matter(path.read_text(encoding="utf-8"))

        # Outgoing bundle-relative cross-links that resolve to a real concept.
        links: list[str] = []
        for mref in LINK_RE.finditer(body):
            target = mref.group(1).lstrip("/")
            tid = target[:-3] if target.endswith(".md") else target
            if tid in valid_ids and tid != cid and tid not in links:
                links.append(tid)

        citations_block = _section(body, "Citations")
        citations = [
            ln.strip()
            for ln in citations_block.split("\n")
            if ln.strip() and re.match(r"^\d+\.", ln.strip())
        ]

        sf = fm.get("source_files")
        if isinstance(sf, str):
            sf = [sf]
        tags = fm.get("tags")
        if isinstance(tags, str):
            tags = [tags]

        concepts.append(
            {
                "id": cid,
                "dir": taxonomy,
                "type": fm.get("type") or "Concept",
                "title": fm.get("title") or cid,
                "description": fm.get("description") or "",
                "provenance": fm.get("provenance") or "",
                "tags": tags or [],
                "source_files": sf or [],
                "links": links,
                "citations": citations,
                "body": body.strip(),
            }
        )
    return concepts


def taxonomy_sort_key(name: str) -> tuple[int, str]:
    if name in TAXONOMY_ORDER:
        return (TAXONOMY_ORDER.index(name), name)
    return (len(TAXONOMY_ORDER), name)


# ---------------------------------------------------------------------------
# Page assembly. All data is embedded as a JSON island; vanilla JS renders the
# nav, the search filter, the per-concept view (with a minimal MD->HTML), and
# the SVG cross-link graph. No external fetch, no CDN.
# ---------------------------------------------------------------------------
def render_html(concepts: list[dict]) -> str:
    ordered = sorted(concepts, key=lambda c: (taxonomy_sort_key(c["dir"]), c["id"]))
    dirs = sorted({c["dir"] for c in ordered}, key=taxonomy_sort_key)
    total = len(ordered)

    payload = {
        "concepts": ordered,
        "dirOrder": dirs,
        "total": total,
    }
    data_json = json.dumps(payload, ensure_ascii=False, sort_keys=True, indent=0)
    # Guard against any literal </script> inside the JSON closing the island.
    data_json = data_json.replace("</", "<\\/")

    css = _CSS
    js = _JS

    return _TEMPLATE.format(
        total=total,
        css=css,
        data_json=data_json,
        js=js,
    )


_CSS = """
:root{
  --bg:#0d1117; --panel:#161b22; --panel2:#1c2230; --border:#2a3038;
  --fg:#e6edf3; --muted:#8b949e; --accent:#58a6ff; --accent2:#79c0ff;
  --chip:#21262d; --ok:#3fb950; --warn:#d29922;
}
*{box-sizing:border-box}
html,body{margin:0;height:100%}
body{background:var(--bg);color:var(--fg);font:14px/1.55 -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif}
a{color:var(--accent);text-decoration:none}
a:hover{text-decoration:underline}
code{background:var(--chip);padding:.1em .35em;border-radius:4px;font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:.86em}
.app{display:grid;grid-template-columns:340px 1fr;height:100vh}
.sidebar{background:var(--panel);border-right:1px solid var(--border);display:flex;flex-direction:column;min-width:0}
.brand{padding:14px 16px;border-bottom:1px solid var(--border)}
.brand h1{margin:0;font-size:15px;letter-spacing:.3px}
.brand .sub{color:var(--muted);font-size:12px;margin-top:3px}
.controls{padding:10px 12px;border-bottom:1px solid var(--border);display:flex;flex-direction:column;gap:8px}
.controls input{width:100%;padding:8px 10px;background:var(--panel2);border:1px solid var(--border);border-radius:6px;color:var(--fg);font-size:13px}
.controls input::placeholder{color:var(--muted)}
.viewtabs{display:flex;gap:6px}
.viewtabs button{flex:1;padding:6px 8px;background:var(--panel2);border:1px solid var(--border);border-radius:6px;color:var(--fg);cursor:pointer;font-size:12px}
.viewtabs button.active{background:var(--accent);border-color:var(--accent);color:#06203a;font-weight:600}
.nav{overflow:auto;flex:1;padding:6px 0}
.nav .group{padding:8px 14px 2px;color:var(--muted);font-size:11px;text-transform:uppercase;letter-spacing:.6px;font-weight:700}
.nav .group.hide{display:none}
.nav .group .gcount{opacity:.7;font-weight:500}
.nav a.item{display:block;padding:5px 14px 5px 18px;color:var(--fg);border-left:3px solid transparent}
.nav a.item:hover{background:var(--panel2);text-decoration:none}
.nav a.item.active{background:var(--panel2);border-left-color:var(--accent)}
.nav a.item .t{display:block;font-size:12.5px}
.nav a.item .ty{display:block;color:var(--muted);font-size:11px}
.nav a.item.hide{display:none}
.main{overflow:auto;padding:26px 34px;min-width:0}
.concept h1.ctitle{margin:0 0 4px;font-size:24px}
.meta{display:flex;flex-wrap:wrap;gap:6px;margin:8px 0 14px}
.chip{background:var(--chip);border:1px solid var(--border);border-radius:999px;padding:2px 10px;font-size:11.5px;color:var(--muted)}
.chip.type{background:#1f2a3d;color:var(--accent2);border-color:#28456b}
.chip.prov{background:#16241a;color:var(--ok);border-color:#1f4427}
.desc{color:var(--muted);font-size:14.5px;margin:0 0 20px;max-width:80ch}
.body{max-width:90ch}
.body h1{font-size:19px;margin:26px 0 8px;padding-bottom:5px;border-bottom:1px solid var(--border)}
.body h2{font-size:16px;margin:20px 0 6px}
.body ul,.body ol{padding-left:22px;margin:8px 0}
.body li{margin:3px 0}
.body p{margin:10px 0}
.panel{background:var(--panel);border:1px solid var(--border);border-radius:8px;padding:14px 16px;margin:18px 0;max-width:90ch}
.panel h3{margin:0 0 8px;font-size:13px;text-transform:uppercase;letter-spacing:.5px;color:var(--muted)}
.panel ul{margin:0;padding-left:18px}
.panel.empty{display:none}
.xlink{display:inline-block;margin:2px 8px 2px 0}
.hint{color:var(--muted);font-size:13px}
.graphwrap{display:none}
.graphwrap.active{display:block}
.conceptwrap.hide{display:none}
.graphhead{display:flex;align-items:baseline;gap:14px;margin-bottom:10px;flex-wrap:wrap}
.graphhead h1{margin:0;font-size:22px}
svg.graph{width:100%;height:620px;background:var(--panel);border:1px solid var(--border);border-radius:8px;display:block}
svg.graph .edge{stroke:#30363d;stroke-width:1}
svg.graph .node{cursor:pointer}
svg.graph .node text{fill:var(--muted);font-size:9px;pointer-events:none}
svg.graph .node:hover circle{stroke:#fff;stroke-width:1.5}
.legend{display:flex;flex-wrap:wrap;gap:10px;margin:12px 0}
.legend .l{display:flex;align-items:center;gap:5px;font-size:12px;color:var(--muted)}
.legend .dot{width:11px;height:11px;border-radius:50%}
.adj{margin-top:22px;max-width:95ch}
.adj h2{font-size:15px;border-bottom:1px solid var(--border);padding-bottom:5px;margin:18px 0 8px}
.adj .row{padding:4px 0;border-bottom:1px solid #1b2027}
.adj .src{font-weight:600}
.adj .arrow{color:var(--muted);margin:0 6px}
.footer{color:var(--muted);font-size:12px;margin-top:30px;padding-top:14px;border-top:1px solid var(--border)}
"""

_TEMPLATE = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>OKF-CoreLink knowledge wiki ({total} concepts)</title>
<style>{css}</style>
</head>
<body>
<div class="app">
  <aside class="sidebar">
    <div class="brand">
      <h1>OKF-CoreLink wiki</h1>
      <div class="sub">{total} concepts &middot; self-contained &middot; offline</div>
    </div>
    <div class="controls">
      <input id="search" type="search" placeholder="Filter by title / type / tag / id…" autocomplete="off">
      <div class="viewtabs">
        <button id="tab-concept" class="active" type="button">Concepts</button>
        <button id="tab-graph" type="button">Cross-link graph</button>
      </div>
    </div>
    <nav id="nav" class="nav"></nav>
  </aside>
  <main class="main">
    <div id="conceptwrap" class="conceptwrap"></div>
    <div id="graphwrap" class="graphwrap"></div>
  </main>
</div>
<script type="application/json" id="okf-data">{data_json}</script>
<script>{js}</script>
</body>
</html>
"""

_JS = r"""
"use strict";
const DATA = JSON.parse(document.getElementById("okf-data").textContent);
const CONCEPTS = DATA.concepts;
const BY_ID = Object.create(null);
CONCEPTS.forEach(c => BY_ID[c.id] = c);
const DIR_ORDER = DATA.dirOrder;
const PALETTE = ["#58a6ff","#3fb950","#d29922","#ff7b72","#bc8cff","#39c5cf",
  "#f778ba","#a5d6ff","#7ee787","#ffa657","#e3b341","#79c0ff","#d2a8ff","#56d364"];
const DIR_COLOR = Object.create(null);
DIR_ORDER.forEach((d,i)=>DIR_COLOR[d]=PALETTE[i%PALETTE.length]);

function esc(s){return (s||"").replace(/[&<>"]/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"}[c]));}

/* ---- minimal Markdown -> HTML (headings, lists, code, bold, links) ---- */
function inline(t){
  t = esc(t);
  t = t.replace(/`([^`]+)`/g, (m,c)=>"<code>"+c+"</code>");
  t = t.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  t = t.replace(/\[([^\]]*)\]\(([^)\s]+)\)/g, (m,txt,url)=>{
    if(/^\/[^)]+\.md$/.test(url)){
      const id = url.replace(/^\//,"").replace(/\.md$/,"");
      if(BY_ID[id]) return '<a href="#'+encodeURIComponent(id)+'" data-cid="'+esc(id)+'">'+txt+'</a>';
      return txt;
    }
    return '<a href="'+esc(url)+'" rel="noopener">'+txt+'</a>';
  });
  return t;
}
function mdToHtml(md){
  const lines = (md||"").split("\n");
  let out = [], i = 0, listType = null, para = [];
  function flushPara(){ if(para.length){ out.push("<p>"+inline(para.join(" "))+"</p>"); para=[]; } }
  function flushList(){ if(listType){ out.push("</"+listType+">"); listType=null; } }
  while(i < lines.length){
    let ln = lines[i];
    const h = /^(#{1,6})\s+(.*)$/.exec(ln);
    const ol = /^\s*\d+\.\s+(.*)$/.exec(ln);
    const ul = /^\s*[-*]\s+(.*)$/.exec(ln);
    if(h){ flushPara(); flushList(); const lvl=Math.min(h[1].length,6); out.push("<h"+lvl+">"+inline(h[2])+"</h"+lvl+">"); }
    else if(ol){ flushPara(); if(listType!=="ol"){flushList();out.push("<ol>");listType="ol";} out.push("<li>"+inline(ol[1])+"</li>"); }
    else if(ul){ flushPara(); if(listType!=="ul"){flushList();out.push("<ul>");listType="ul";} out.push("<li>"+inline(ul[1])+"</li>"); }
    else if(ln.trim()===""){ flushPara(); flushList(); }
    else { if(listType)flushList(); para.push(ln.trim()); }
    i++;
  }
  flushPara(); flushList();
  return out.join("\n");
}

/* ---------------- nav ---------------- */
const navEl = document.getElementById("nav");
function buildNav(){
  const groups = Object.create(null);
  CONCEPTS.forEach(c=>{ (groups[c.dir]=groups[c.dir]||[]).push(c); });
  let h = "";
  DIR_ORDER.forEach(d=>{
    const list = groups[d]||[];
    h += '<div class="group" data-group="'+esc(d)+'">'+esc(d)+' <span class="gcount">('+list.length+')</span></div>';
    list.forEach(c=>{
      h += '<a class="item" id="nav-'+esc(c.id)+'" href="#'+encodeURIComponent(c.id)+'"'
        + ' data-id="'+esc(c.id)+'" data-group="'+esc(c.dir)+'"'
        + ' data-search="'+esc((c.title+" "+c.type+" "+c.id+" "+c.tags.join(" ")).toLowerCase())+'">'
        + '<span class="t">'+esc(c.title)+'</span>'
        + '<span class="ty">'+esc(c.type)+'</span></a>';
    });
  });
  navEl.innerHTML = h;
}

/* ---------------- search ---------------- */
const searchEl = document.getElementById("search");
searchEl.addEventListener("input", ()=>{
  const q = searchEl.value.trim().toLowerCase();
  const visibleByGroup = Object.create(null);
  navEl.querySelectorAll("a.item").forEach(a=>{
    const hit = !q || a.getAttribute("data-search").indexOf(q) !== -1;
    a.classList.toggle("hide", !hit);
    const g = a.getAttribute("data-group");
    if(hit) visibleByGroup[g] = (visibleByGroup[g]||0)+1;
  });
  navEl.querySelectorAll(".group").forEach(g=>{
    const name = g.getAttribute("data-group");
    g.classList.toggle("hide", !visibleByGroup[name]);
  });
});

/* ---------------- concept view ---------------- */
const conceptWrap = document.getElementById("conceptwrap");
function renderConcept(id){
  const c = BY_ID[id];
  if(!c){ conceptWrap.innerHTML = '<p class="hint">Unknown concept: '+esc(id)+'</p>'; return; }
  let h = '<article class="concept">';
  h += '<h1 class="ctitle">'+esc(c.title)+'</h1>';
  h += '<div class="meta"><span class="chip type">'+esc(c.type)+'</span>';
  h += '<span class="chip">'+esc(c.id)+'</span>';
  if(c.provenance) h += '<span class="chip prov">'+esc(c.provenance)+'</span>';
  c.tags.forEach(t=> h += '<span class="chip">#'+esc(t)+'</span>');
  h += '</div>';
  if(c.description) h += '<p class="desc">'+esc(c.description)+'</p>';

  h += '<div class="body">'+mdToHtml(c.body)+'</div>';

  h += '<div class="panel'+(c.source_files.length?'':' empty')+'"><h3>Source files</h3><ul>';
  c.source_files.forEach(s=> h += '<li><code>'+esc(s)+'</code></li>');
  h += '</ul></div>';

  h += '<div class="panel'+(c.links.length?'':' empty')+'"><h3>Cross-links ('+c.links.length+')</h3><div>';
  c.links.forEach(l=>{
    const t = BY_ID[l];
    h += '<a class="xlink" href="#'+encodeURIComponent(l)+'" data-cid="'+esc(l)+'">&rarr; '+esc(t?t.title:l)+'</a>';
  });
  h += '</div></div>';

  const incoming = CONCEPTS.filter(x=> x.links.indexOf(id)!==-1);
  h += '<div class="panel'+(incoming.length?'':' empty')+'"><h3>Referenced by ('+incoming.length+')</h3><div>';
  incoming.forEach(x=> h += '<a class="xlink" href="#'+encodeURIComponent(x.id)+'" data-cid="'+esc(x.id)+'">&larr; '+esc(x.title)+'</a>');
  h += '</div></div>';

  h += '<div class="footer">Generated by <code>scripts/okf_render.py</code> from <code>docs/knowledge/</code>. Self-contained &middot; offline &middot; no external deps.</div>';
  h += '</article>';
  conceptWrap.innerHTML = h;
  conceptWrap.parentElement.scrollTop = 0;

  navEl.querySelectorAll("a.item.active").forEach(a=>a.classList.remove("active"));
  const nav = document.getElementById("nav-"+id);
  if(nav){ nav.classList.add("active"); nav.scrollIntoView({block:"nearest"}); }
}

/* ---------------- graph view ---------------- */
const graphWrap = document.getElementById("graphwrap");
let graphBuilt = false;
function buildGraph(){
  if(graphBuilt) return; graphBuilt = true;
  const edges = [];
  CONCEPTS.forEach(c=> c.links.forEach(l=>{ if(BY_ID[l]) edges.push([c.id,l]); }));

  const W = 1200, H = 600;
  const nodes = CONCEPTS.map(c=>({id:c.id,dir:c.dir,deg:0}));
  const idx = Object.create(null); nodes.forEach((n,i)=>idx[n.id]=i);
  edges.forEach(e=>{ nodes[idx[e[0]]].deg++; nodes[idx[e[1]]].deg++; });

  const anchors = Object.create(null);
  DIR_ORDER.forEach((d,i)=>{
    const a = (i/DIR_ORDER.length)*Math.PI*2;
    anchors[d] = {x: W/2 + Math.cos(a)*W*0.34, y: H/2 + Math.sin(a)*H*0.36};
  });
  let seed = 1337;
  function rnd(){ seed = (seed*1103515245 + 12345) & 0x7fffffff; return seed/0x7fffffff; }
  nodes.forEach(n=>{
    const an = anchors[n.dir] || {x:W/2,y:H/2};
    n.x = an.x + (rnd()-0.5)*120; n.y = an.y + (rnd()-0.5)*120; n.vx=0; n.vy=0;
  });

  const eidx = edges.map(e=>[idx[e[0]],idx[e[1]]]);
  for(let it=0; it<120; it++){
    for(let a=0;a<nodes.length;a++){
      let fx=0, fy=0; const na=nodes[a];
      for(let b=0;b<nodes.length;b++){
        if(a===b) continue;
        const nb=nodes[b];
        let dx=na.x-nb.x, dy=na.y-nb.y; let d2=dx*dx+dy*dy; if(d2<0.01)d2=0.01;
        const f = 900/d2; fx+=dx*f; fy+=dy*f;
      }
      const an = anchors[na.dir]||{x:W/2,y:H/2};
      fx += (an.x-na.x)*0.012; fy += (an.y-na.y)*0.012;
      na.vx=(na.vx+fx)*0.85; na.vy=(na.vy+fy)*0.85;
    }
    eidx.forEach(e=>{
      const a=nodes[e[0]], b=nodes[e[1]];
      let dx=b.x-a.x, dy=b.y-a.y; const dist=Math.sqrt(dx*dx+dy*dy)||1;
      const f=(dist-60)*0.02; const ux=dx/dist, uy=dy/dist;
      a.vx+=ux*f; a.vy+=uy*f; b.vx-=ux*f; b.vy-=uy*f;
    });
    nodes.forEach(n=>{ n.x+=Math.max(-12,Math.min(12,n.vx)); n.y+=Math.max(-12,Math.min(12,n.vy));
      n.x=Math.max(20,Math.min(W-20,n.x)); n.y=Math.max(20,Math.min(H-20,n.y)); });
  }

  let svg = '<svg class="graph" viewBox="0 0 '+W+' '+H+'" preserveAspectRatio="xMidYMid meet">';
  eidx.forEach(e=>{
    const a=nodes[e[0]], b=nodes[e[1]];
    svg += '<line class="edge" x1="'+a.x.toFixed(1)+'" y1="'+a.y.toFixed(1)+'" x2="'+b.x.toFixed(1)+'" y2="'+b.y.toFixed(1)+'"/>';
  });
  nodes.forEach(n=>{
    const r = 3 + Math.min(7, n.deg*0.7);
    const col = DIR_COLOR[n.dir]||"#888";
    const c = BY_ID[n.id];
    svg += '<g class="node" data-cid="'+esc(n.id)+'"><title>'+esc(c.title)+'  ['+esc(n.dir)+'] · '+n.deg+' links</title>'
      + '<circle cx="'+n.x.toFixed(1)+'" cy="'+n.y.toFixed(1)+'" r="'+r.toFixed(1)+'" fill="'+col+'" fill-opacity="0.85"/>'
      + (n.deg>=4 ? '<text x="'+(n.x+r+1).toFixed(1)+'" y="'+(n.y+3).toFixed(1)+'">'+esc(c.title.slice(0,22))+'</text>' : '')
      + '</g>';
  });
  svg += '</svg>';

  let legend = '<div class="legend">';
  DIR_ORDER.forEach(d=>{ legend += '<span class="l"><span class="dot" style="background:'+DIR_COLOR[d]+'"></span>'+esc(d)+'</span>'; });
  legend += '</div>';

  const groups = Object.create(null);
  CONCEPTS.forEach(c=>{ if(c.links.length){ (groups[c.dir]=groups[c.dir]||[]).push(c); } });
  let adj = '<div class="adj"><h2>Adjacency list (outgoing cross-links by taxonomy)</h2>';
  DIR_ORDER.forEach(d=>{
    const list = groups[d]; if(!list||!list.length) return;
    adj += '<h2 style="color:'+DIR_COLOR[d]+'">'+esc(d)+'</h2>';
    list.forEach(c=>{
      adj += '<div class="row"><a class="src" href="#'+encodeURIComponent(c.id)+'" data-cid="'+esc(c.id)+'">'+esc(c.title)+'</a><span class="arrow">&rarr;</span>';
      adj += c.links.map(l=>{ const t=BY_ID[l]; return '<a href="#'+encodeURIComponent(l)+'" data-cid="'+esc(l)+'">'+esc(t?t.title:l)+'</a>'; }).join(', ');
      adj += '</div>';
    });
  });
  adj += '</div>';

  graphWrap.innerHTML = '<div class="graphhead"><h1>Cross-link graph</h1><span class="hint">'
    + CONCEPTS.length+' nodes · '+edges.length+' edges · node size = link degree · click a node to open it</span></div>'
    + legend + svg + adj;
}

/* ---------------- view switching + routing ---------------- */
const tabConcept = document.getElementById("tab-concept");
const tabGraph = document.getElementById("tab-graph");
let mode = "concept";
function setMode(m){
  mode = m;
  tabConcept.classList.toggle("active", m==="concept");
  tabGraph.classList.toggle("active", m==="graph");
  conceptWrap.classList.toggle("hide", m!=="concept");
  graphWrap.classList.toggle("active", m==="graph");
  if(m==="graph") buildGraph();
}
tabConcept.addEventListener("click", ()=>{ setMode("concept"); });
tabGraph.addEventListener("click", ()=>{ setMode("graph"); });

function route(){
  const id = decodeURIComponent((location.hash||"").replace(/^#/,""));
  if(id && BY_ID[id]){ setMode("concept"); renderConcept(id); }
}
window.addEventListener("hashchange", route);

document.addEventListener("click", e=>{
  const a = e.target.closest("[data-cid]");
  if(a){ const id=a.getAttribute("data-cid"); if(BY_ID[id]){ location.hash = "#"+encodeURIComponent(id); } }
});

/* ---------------- boot ---------------- */
buildNav();
if(location.hash && BY_ID[decodeURIComponent(location.hash.replace(/^#/,""))]){
  route();
} else {
  renderConcept(CONCEPTS[0].id);
}
"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="Exit 1 if the generated site differs from on-disk (no write).",
    )
    args = parser.parse_args()

    if not BUNDLE.is_dir():
        sys.exit(f"ERROR: bundle root not found: {BUNDLE}")

    concepts = find_concepts()
    content = render_html(concepts)

    if args.check:
        current = SITE.read_text(encoding="utf-8") if SITE.exists() else ""
        if current != content:
            print("⛔ docs/okf-wiki-site/index.html is out of date — run `python3 scripts/okf_render.py`.")
            return 1
        print(f"✅ docs/okf-wiki-site/index.html is current — {len(concepts)} concepts.")
        return 0

    SITE_DIR.mkdir(parents=True, exist_ok=True)
    SITE.write_text(content, encoding="utf-8")
    edges = sum(len(c["links"]) for c in concepts)
    print(
        f"✅ Rendered {SITE.relative_to(REPO_ROOT)} — "
        f"{len(concepts)} concepts, {edges} cross-link edges, "
        f"{len(content):,} bytes (self-contained, no external deps)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
