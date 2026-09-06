"""C4-C10 and nightly checks for the OKF validator."""
from __future__ import annotations

from pathlib import Path
from validate_okf_core1 import *
from validate_okf_core2 import *


def _under(p: Path, root: Path) -> bool:
    """Return whether ``p`` is inside ``root`` (including ``root`` itself)."""
    try:
        p.relative_to(root)
        return True
    except ValueError:
        return False


def _check_c5b(args, git: Git, bundle_root: Path, concepts: list[Concept], fails: Failures):
    base_bundle = Path(args.base_bundle).resolve() if args.base_bundle else None
    base_rev = None
    if base_bundle is None:
        mb = git.merge_base(args.base_ref)
        base_rev = mb

    if base_rev:
        _preload_show_files(git, base_rev, [c.path.relative_to(git.repo_root).as_posix() for c in concepts if not c.is_deferred and c.checkpoint_sha and _under(c.path, git.repo_root)])

    for c in concepts:
        if c.is_deferred or not c.checkpoint_sha:
            continue
        prev_text = None
        if base_bundle is not None:
            prev_path = base_bundle / c.rel
            if prev_path.exists():
                prev_text = prev_path.read_text(encoding="utf-8")
        elif base_rev and _under(c.path, git.repo_root):
            repo_rel = c.path.relative_to(git.repo_root).as_posix()
            prev_text = git.show_file(base_rev, repo_rel)
        if prev_text is None:
            continue  # new concept — nothing was advanced
        prev_block, _ = _split(prev_text)
        if prev_block is None:
            continue
        try:
            prev_fm = parse_frontmatter(prev_block)
        except Exception:
            continue
        prev_sha = prev_fm.get("checkpoint_sha")
        if not prev_sha or prev_sha == c.checkpoint_sha:
            continue  # not advanced
        # SHA changed: require a real body edit. Compare both texts with the
        # checkpoint_sha line stripped — if identical, only the SHA moved. There
        # is deliberately no orphan-repair exemption: an unreachable checkpoint
        # is a C4 failure, and a repair must carry a real reconciliation (adding a
        # first source_blobs anchor is a visible frontmatter change).
        if _strip_ckpt_line(prev_text) == _strip_ckpt_line(c.text):
            loc = c.path.relative_to(git.repo_root).as_posix() if _under(c.path, git.repo_root) else f"{bundle_root.name}/{c.rel}"
            fails.add(
                "C5b",
                loc,
                f"checkpoint_sha advanced ({str(prev_sha)[:12]} -> {c.checkpoint_sha[:12]}) "
                "without any body edit (phantom reconcile)",
            )


def _check_c4c(args, git: Git, bundle_root: Path, concepts: list[Concept], fails: Failures):
    """C4c — blob addressing is a ONE-WAY RATCHET.

    Without this, closing the forged-anchor laundering hole would only MOVE it:
    an author facing a legitimate C5 STALE could delete the file's `source_blobs`
    entry, drop back onto a legacy commit anchor, and bypass the immutable
    content baseline. So a path that WAS blob-addressed in the
    previous version of the concept must stay blob-addressed while it is still a
    declared source. Un-migrating requires deleting the source itself (a
    review-visible change to `source_files` that C3/C6/C10b all react to), not a
    one-line frontmatter deletion.

    The 'previous version' is read exactly as C5b reads it — the concept file at
    the merge-base with the base ref (or `--base-bundle`) — so a NEW concept and a
    detached checkout with no common ancestor are both no-ops, never failures.
    """
    base_bundle = Path(args.base_bundle).resolve() if args.base_bundle else None
    base_rev = None if base_bundle is not None else git.merge_base(args.base_ref)

    for c in concepts:
        if c.is_deferred:
            continue
        prev_text = None
        if base_bundle is not None:
            prev_path = base_bundle / c.rel
            if prev_path.exists():
                prev_text = prev_path.read_text(encoding="utf-8")
        elif base_rev and _under(c.path, git.repo_root):
            repo_rel = c.path.relative_to(git.repo_root).as_posix()
            prev_text = git.show_file(base_rev, repo_rel)
        if prev_text is None:
            continue  # new concept — nothing to ratchet against
        prev_block, _ = _split(prev_text)
        if prev_block is None:
            continue
        try:
            prev_fm = parse_frontmatter(prev_block)
        except Exception:
            continue
        prev_sb = prev_fm.get("source_blobs")
        if not isinstance(prev_sb, list):
            continue
        prev_paths = set()
        for entry in prev_sb:
            if not isinstance(entry, str):
                continue
            m = SOURCE_BLOB_RE.match(entry.strip())
            if m:
                prev_paths.add(m.group("path"))
        if not prev_paths:
            continue
        loc = (
            c.path.relative_to(git.repo_root).as_posix()
            if _under(c.path, git.repo_root)
            else f"{bundle_root.name}/{c.rel}"
        )
        for lost in sorted(prev_paths - set(c.source_blobs)):
            if lost not in set(c.source_files):
                continue  # the source itself was dropped — the anchor is moot
            fails.add(
                "C4c",
                loc,
                f"`source_blobs` anchor for `{lost}` was REMOVED while the path is still "
                "a declared source — blob addressing is a ratchet (dropping it would "
                "restore the mutable legacy commit baseline this concept no longer uses)",
            )


def _strip_ckpt_line(text: str) -> str:
    return "\n".join(
        ln for ln in text.splitlines() if not ln.strip().startswith("checkpoint_sha:")
    )


def _check_links_and_orphans(
    bundle_root: Path, concepts: list[Concept], deferred_ids: set[str], args, fails: Failures
):
    repo_root = args._repo_root
    by_id = {c.concept_id: c for c in concepts}
    existing_targets = {c.rel for c in concepts}
    planned = args._planned_ids  # set populated by manifest load (may be empty)

    def loc_of(c: Concept):
        return c.path.relative_to(repo_root).as_posix() if _under(c.path, repo_root) else f"{bundle_root.name}/{c.rel}"

    # C7: each bundle-relative link to a declared concept resolves.
    index_path = bundle_root / "index.md"
    link_sources: list[tuple[str, list[str]]] = []
    if index_path.exists():
        link_sources.append(("index.md", LINK_RE.findall(index_path.read_text(encoding="utf-8"))))
    for c in concepts:
        link_sources.append((loc_of(c), c.links))

    for src, links in link_sources:
        for target in links:
            rel = target.lstrip("/")
            if not rel.endswith(".md"):
                continue
            cid = rel[:-3]
            if rel in existing_targets:
                continue  # resolves
            # tolerate links to not-yet-written / deferred / planned concepts
            if cid in deferred_ids or cid in planned:
                continue
            fails.add("C7", src, f"broken link to declared concept: `/{rel}`")

    # C8: reachability from index.md
    if concepts:
        reachable: set[str] = set()
        if index_path.exists():
            frontier = [
                t.lstrip("/")[:-3]
                for t in LINK_RE.findall(index_path.read_text(encoding="utf-8"))
                if t.lstrip("/").endswith(".md")
            ]
        else:
            frontier = []
        while frontier:
            cid = frontier.pop()
            if cid in reachable:
                continue
            reachable.add(cid)
            node = by_id.get(cid)
            if node:
                for t in node.links:
                    r = t.lstrip("/")
                    if r.endswith(".md"):
                        frontier.append(r[:-3])
        for c in concepts:
            if c.concept_id not in reachable:
                fails.add("C8", loc_of(c), "orphan concept — not reachable from index.md")


def _check_manifest(args, bundle_root: Path, concepts, deferred_ids, fails: Failures):
    manifest_path = Path(args.manifest) if args.manifest else None
    if manifest_path is None or not manifest_path.exists():
        fails.warn("C10", str(args.manifest or "concept-manifest.yaml"),
                   "manifest absent — C10/C10b skipped")
        return
    data = load_manifest(manifest_path)
    if data is None:
        fails.warn("C10", str(manifest_path), "manifest unparseable without PyYAML — C10/C10b skipped")
        return

    concept_ids = {c.concept_id for c in concepts}
    concept_by_id = {c.concept_id: c for c in concepts}

    def _concept_grounds(c: "Concept", seed: str) -> bool:
        """True iff concept `c` ACTUALLY grounds the surface `seed` — i.e. `seed`
        appears in the concept's resolved `source_files` OR in its `# Citations`
        (the inline-cited files), under one of these GENUINE-coverage relations:

          (a) verbatim — `seed` is declared/cited as-is;
          (b) `seed` is a directory containing a declared/cited path;
          (c) `seed` is a file/dir under a declared/cited directory;
          (d) crate-cluster ADOPTION — a `type: CrateCluster` concept that grounds
              a crate directory adopts the whole crate as its narrative home, so a
              file/dir under that crate is covered even without a per-file cite
              (the §1.1 taxonomy assigns `crates/` to "the 8 crate clusters"; a
              cluster's job is breadth-of-narrative, not file-granular citation).

        This is the anti-self-certification check (fix #3 / audit-round R): a
        `seed_from` entry counts as coverage ONLY when the concept it names really
        explains the surface — a manifest can no longer silently "cover" a file by
        listing it under a concept that never mentions it (the silent-gap bypass).

        REMOVED in gate v5 (audit #3 HIGH #2): the former Rust "module-parent"
        relation auto-grounded an ENTIRE module subtree from a single parent-file
        cite — a concept citing `routes.rs` (the `mod routes;` declaration site)
        auto-covered ANY `routes/*.rs` added to its `seed_from` WITHOUT the concept
        ever mentioning that handler. That is the same self-certification
        relations (a)-(c) exist to forbid, one level deeper: `X.rs` is the module
        DECLARATION, not an explanation of `X/sub.rs`'s behaviour. A file under a
        declared module is now covered ONLY when the concept declares/cites that
        file (a), or grounds a DIRECTORY that contains it (b/c — a `routes/` or
        `routes/dsr/` directory seed, an explicit narrative-home decision), or is
        the crate-cluster adopting the whole crate (d). The `X.rs`-as-module-root
        shortcut is gone — cite the file or seed its directory, do not lean on the
        `mod` declaration to self-certify the subtree.
        """
        grounded = set(c.source_files) | set(c.cited_files)
        s = seed.rstrip("/")
        is_cluster = (c.type == "CrateCluster")
        for g in grounded:
            gn = g.rstrip("/")
            if s == gn:
                return True
            # (b) seed is a directory that contains a grounded path
            if gn == s or gn.startswith(s + "/"):
                return True
            # (c) seed lives under a grounded directory
            if s.startswith(gn + "/"):
                return True
            # (d) crate-cluster adoption: a CrateCluster grounding a crate dir
            #     adopts everything under that crate (its src files + subdirs).
            if is_cluster:
                m = re.match(r"^(crates/[^/]+)/", gn + "/")
                if m:
                    crate = m.group(1)
                    if s == crate or s.startswith(crate + "/"):
                        return True
        return False

    # Real W-MANIFEST schema: top-level `candidates:` (id/type/title/status/
    # source_cluster/seed_from) + `excludes:` (surface/reason). A candidate maps
    # to repo surface via its `seed_from` paths; C10b coverage = seed_from ∪ excludes.
    # A `seed_from` path counts as COVERAGE only when the candidate's concept doc
    # actually GROUNDS that path (declares/cites it) — a seed naming a concept that
    # doesn't mention the file is self-certifying and is REJECTED (fix #3).
    surface_root = Path(args.surface_root).resolve()

    # gate v7 fix #2: a strict-tree file is GENUINELY grounded only when the
    # covering concept cites that exact file at a range that contains a real CODE
    # line (not a blank / `//`-`//!`-`///` doc-comment / `#`-`*` comment line).
    # A cite that resolves only to the file's leading doc/license header (the
    # `:1-2` PoC) does NOT substantiate the executed handler body, so it does not
    # code-ground a strict-tree seed. Cached per (path) since file content is
    # shared across concepts.
    _strict_file_lines_cache: dict[str, list[str] | None] = {}

    def _strict_file_lines(rel: str):
        if rel not in _strict_file_lines_cache:
            p = surface_root / rel
            try:
                _strict_file_lines_cache[rel] = p.read_text(encoding="utf-8").splitlines()
            except (OSError, UnicodeDecodeError):
                _strict_file_lines_cache[rel] = None
        return _strict_file_lines_cache[rel]

    def _concept_code_grounds(c: "Concept", sp: str) -> bool:
        """True iff concept `c`'s grounding of the EXACT strict-tree file `sp`
        is GENUINE for gate v7 fix #2.

        The hole fix #2 closes is the per-file CITE that points at a non-code
        line: a concept lists `sp` in `source_files` + cites it under
        `# Citations`, but the cite resolves only to the file's `//`/`//!`
        doc-comment header (the `:1-2` PoC), leaving the executed handler body
        uncited. So when `sp` IS cited by `c`, AT LEAST ONE of its cite ranges
        must contain a real CODE line.

        A strict file grounded by CrateCluster ADOPTION (relation (d): an
        explicit `seed_from` entry under a `type: CrateCluster` concept, NOT a
        per-file cite) is a DIFFERENT, already review-gated coverage path — the
        cluster adopts the crate as its narrative home and is not expected to
        cite every file. That path is unaffected: `sp` is code-grounded when it
        is NOT cited by `c` at all (pure adoption) OR it is cited with a code
        line. It is NOT code-grounded only when it IS cited yet EVERY cite is a
        comment/blank line — the exact PoC shape."""
        if sp not in c.cited_files:
            return True  # adoption-only (or source_files-only) — not a cite hole
        lines = _strict_file_lines(sp)
        if lines is None:
            return False
        for (f, l1, l2) in c.cites:
            if f == sp and _cite_is_code_line(lines, l1, l2):
                return True
        return False

    entries = data.get("candidates") or data.get("concepts") or []
    seed_paths: set[str] = set()           # every declared seed (for diagnostics)
    grounded_seed_paths: set[str] = set()  # seeds a real concept grounds — THESE cover
    # strict-tree seeds whose covering concept cites a real CODE line (fix #2).
    code_grounded_seed_paths: set[str] = set()
    for e in entries:
        if not isinstance(e, dict):
            continue
        cid = e.get("id")
        status = (e.get("status") or "").strip().lower()
        if status == "planned":
            args._planned_ids.add(cid)
        c = concept_by_id.get(cid)
        for s in (e.get("seed_from") or e.get("covers") or []):
            sp = str(s)
            seed_paths.add(sp)
            # The seed grounds coverage iff its declaring concept exists AND
            # actually declares/cites the path. A planned (doc-less) candidate's
            # seed cannot self-certify coverage — there is no concept to ground it.
            if c is not None and _concept_grounds(c, sp):
                grounded_seed_paths.add(sp)
                if _concept_code_grounds(c, sp):
                    code_grounded_seed_paths.add(sp)
        # C10: active candidate must have a doc or a deferred marker (deferred
        # docs are scanned as concepts, so concept_ids already covers both).
        if status == "active" and cid:
            if cid not in concept_ids:
                fails.add("C10", str(manifest_path),
                          f"active candidate `{cid}` has no concept doc and no deferred marker")

    # C10b: manifest must cover the mechanically-enumerable repo surface.
    # NOTE: _glob_match is defined at module scope (`_segment_glob_match`) so it
    # is unit-testable and segment-aware; bound here as a local alias.
    _glob_match = _segment_glob_match

    # gate v7 fix #3 — a one-line `excludes:` entry with a fabricated reason can
    # hide a backdoor. We (a) require every exclude reason to be non-trivial (a
    # real sentence, not a placeholder), and (b) SURFACE-via-WARN every exclude
    # entry that targets a SECURITY-CRITICAL strict tree, so a human reviewer
    # sees exactly which strict-tree surfaces were waived (the sanctioned
    # alternative to a per-file cite must remain review-visible). A trivially-
    # reasoned strict-tree exclude is a hard [C10b] failure; an honest one is a
    # surfaced WARN.
    _PLACEHOLDER_REASONS = {
        "todo", "tbd", "fixme", "n/a", "na", "none", "test", "wip",
        "placeholder", "exclude", "excluded", "skip", "ignore", "-", "x",
    }

    def _reason_is_trivial(reason: str) -> bool:
        r = (reason or "").strip()
        if len(r) < 12:
            return True
        if r.lower().rstrip(".") in _PLACEHOLDER_REASONS:
            return True
        return False

    def _exclude_can_hit_strict(surf: str) -> bool:
        """True iff this exclude surface (exact or glob) can match SOME path in a
        strict tree — i.e. it waives strict-tree surface and must be reviewed."""
        s = surf.rstrip("/")
        # exact / prefix entry pointing into a strict tree
        if _is_file_granular_strict(s) or _is_file_granular_strict(s + "/x.rs"):
            return True
        # a broad pattern (e.g. **/tests/**) — does it match any strict prefix?
        if "*" in surf or "?" in surf or "[" in surf:
            probes = [
                "crates/corelink-container/src/routes/tests/x.rs",
                "crates/corelink-container/src/x.config.ts",
                "crates/corelink-container/src/x.d.ts",
                "worker/src/__tests__/x.ts",
                "worker/src/x.test.ts",
                "worker/src/x.config.ts",
                "apps/signup-worker/src/e2e/x.ts",
                "apps/signup-worker/src/x.test.ts",
            ]
            return any(_glob_match(p, surf) for p in probes)
        return False

    excludes: list[str] = []
    for ex in (data.get("excludes") or []):
        if isinstance(ex, dict):
            surf = ex.get("surface") or ex.get("path")
            reason = ex.get("reason")
            if surf and reason:
                surf = str(surf)
                excludes.append(surf)
                if _exclude_can_hit_strict(surf):
                    if _reason_is_trivial(str(reason)):
                        fails.add(
                            "C10b", str(manifest_path),
                            f"strict-tree exclude `{surf}` has a trivial/placeholder "
                            f"reason ({str(reason).strip()[:40]!r}) — a strict-tree waiver "
                            "must carry a real justification",
                        )
                    else:
                        fails.warn(
                            "C10b", str(manifest_path),
                            f"strict-tree exclude (review): `{surf}` — {str(reason).strip()[:80]}",
                        )

    # gate v10 fix #2 (HIGH): the set of wrangler-`main` entrypoints that live
    # OUTSIDE the conventional `apps/*/src/**` strict tree. Such a main IS the real
    # request-reachable deploy surface, so it must be treated as STRICT — directory/
    # cluster ADOPTION must NOT auto-cover it; it needs an exact grounded seed or an
    # explicit `excludes:` entry, the same as any other request-reachable enforcer.
    # gate v14 (MATERIAL): enumerate over EVERY wrangler config in the repo via a
    # RECURSIVE walk of the surface root (`_wrangler_config_dirs`, build-output /
    # vendor dirs pruned), not a fixed root+apps/*+crates/* location set. A wrangler
    # `main` pointing OUTSIDE `*/src/**` declared in ANY dir — a crate-NESTED
    # `crates/<x>/cf/`, a sibling top-level `services/edge/`, a `worker/` alt config,
    # or anywhere else — is now classified STRICT, so config LOCATION can no longer
    # hide a deploy entrypoint behind coarse crate-dir/cluster ADOPTION.
    wrangler_main_strict: set[str] = set()
    for _cfg_dir in _wrangler_config_dirs(surface_root):
        # gate v11: EVERY declared main — top-level AND every [env.*] override.
        for _main_rel in _wrangler_mains(_cfg_dir):
            _mp = (_cfg_dir / _main_rel).resolve()
            try:
                _r = _mp.relative_to(surface_root).as_posix()
            except ValueError:
                continue
            if _mp.is_file() and _is_exec_source(_r, js_ts=True, rust=True):
                # only the ones NOT already strict by the file-granular rule
                # (a main INSIDE the owning crate's src/** is already covered).
                if not _is_file_granular_strict(_r):
                    wrangler_main_strict.add(_r)

    def _strict(rel: str) -> bool:
        """`_is_file_granular_strict`, EXTENDED with the gate-v10/v13 wrangler-`main`
        entrypoints that live outside `*/src/**` — a non-conventional but
        request-reachable deploy surface declared by ANY wrangler config under
        apps/, crates/, or the repo root."""
        return _is_file_granular_strict(rel) or rel in wrangler_main_strict

    def is_covered(rel: str, is_dir: bool) -> bool:
        # --- DIRECTORY (crate) surface: coverage = reviewed cluster MEMBERSHIP ---
        # A crate-dir surface is covered if ANY seed_from path is the dir itself or
        # lives under it. Crate-level membership is the curated, review-gated
        # cluster assignment (taxonomy §1.1: `crates/` = "the 8 crate clusters"),
        # so it self-certifies at the COARSE crate granularity by design — the
        # nightly C-REV reverse-coverage WARN is what pressures source_files
        # completeness WITHIN a member crate. Fix #3's anti-self-certification is a
        # FILE-granular guard (a handler FILE slipped under a concept that never
        # mentions it), so it does NOT apply to a whole-crate directory surface.
        if is_dir:
            if any(s == rel or s.startswith(rel.rstrip("/") + "/") for s in seed_paths):
                return True
        else:
            # --- FILE surface: coverage requires GENUINE grounding (fix #3) -------
            # A file counts as covered ONLY by a seed whose declaring concept
            # actually grounds it (`grounded_seed_paths`, not the raw seed set): a
            # manifest entry naming a FILE under a concept that never cites/declares
            # it is self-certifying and is NOT coverage — closing the silent-gap
            # bypass where a load-bearing handler hides under an unrelated concept.
            # gate v7 fix #2 — in a strict tree, a verbatim grounded seed is
            # coverage ONLY when the covering cite resolves to a real CODE line.
            # A backdoor handler cited solely at its `:1-2` doc-comment header is
            # NOT grounded (the cited line doesn't substantiate the executed
            # body), so it falls through to the exclude check and REDs unless it
            # carries an explicit per-file exclude. Outside strict trees the
            # verbatim grounded seed covers unchanged.
            if rel in grounded_seed_paths:
                if not _strict(rel) or rel in code_grounded_seed_paths:
                    return True
            # a file is also covered when a GROUNDED directory seed contains it
            # (the concept that adopts the dir grounds the files beneath it) —
            # EXCEPT in the SECURITY-CRITICAL file-granular trees (gate v6 fix #1):
            # there, directory/cluster ADOPTION must NOT auto-cover an individual
            # file, or a new request-reachable handler (routes/poison.rs,
            # src/poison_top.rs, a new worker/src/** or apps/*/src/** module) would
            # ride the whole-crate `crates/corelink-container` seed and stay GREEN —
            # the dead anti-shadow enumeration. Those files are covered only by an
            # exact grounded seed (above) or an explicit `excludes:` entry (below).
            if not _strict(rel) and any(
                (s.rstrip("/") != "" and rel.startswith(s.rstrip("/") + "/"))
                for s in grounded_seed_paths
            ):
                return True
        strict = (not is_dir) and _strict(rel)
        for ex in excludes:
            exn = ex.rstrip("/")
            # An EXACT (or exact-prefix) exclude entry is an EXPLICIT, review-
            # visible per-file/per-dir decision — always honored, in or out of a
            # strict tree.
            if rel == exn or rel.startswith(exn + "/"):
                return True
            # A BROAD PATTERN exclude (dir-name glob like `**/tests/**`, or an
            # extension glob like `*.config.ts`) is a bulk rule. gate v7 fix #1:
            # inside a SECURITY-CRITICAL strict tree, a broad pattern may exclude
            # a file ONLY when that file is GENUINELY a test/config artifact by
            # its OWN basename. A real handler name (`poison.rs`) dropped under a
            # `tests/` dir matches `**/tests/**` but is NOT a test by its name —
            # it must NOT be auto-swallowed; it requires an explicit per-file
            # `excludes:` entry (handled above) or grounded coverage, else it
            # REDs. Outside strict trees the bulk rule applies unchanged.
            if "*" in ex or "?" in ex or "[" in ex:
                if not _glob_match(rel, ex):
                    continue
                if strict and not _basename_is_test_or_config(rel):
                    continue
                return True
        return False

    surface: list[tuple[str, bool]] = []
    adr_dir = surface_root / "specs" / "03_architecture" / "adrs"
    if adr_dir.is_dir():
        surface += [(p.relative_to(surface_root).as_posix(), False) for p in sorted(adr_dir.glob("*.md"))]
    routes_dir = surface_root / "crates" / "corelink-container" / "src" / "routes"
    if routes_dir.is_dir():
        # RECURSIVE: a handler dropped in routes/dsr/, routes/audit_export/, or
        # routes/audit_analytics/ (or any future subdir) must be enumerated too —
        # a non-recursive glob("*.rs") was a silent completeness hole (a new
        # handler at routes/dsr/backdoor.rs would never be gated). Genuine
        # non-handlers (tests_*.rs) are exempted via the manifest `excludes:`.
        # gate v10 fix #1 (MED — CASE-SENSITIVE rglob vs case-insensitive
        # classifier): `rglob("*.rs")` is case-SENSITIVE while `_is_exec_source`
        # lowercases the suffix, so a `Poison.RS` (pulled via `#[path]`) was
        # classified strict yet NEVER enumerated → shipped GREEN. Enumerate via
        # the SAME `_is_exec_source` predicate (case-insensitive) so the surface
        # walk and the strict classifier AGREE on what is a Rust source file.
        surface += list(_iter_exec_sources(routes_dir, surface_root, js_ts=False, rust=True))
    # The container crate ROOT, file-granular: a handler dropped directly in
    # crates/corelink-container/src/ (sibling to webhook.rs / native_pat_gate.rs)
    # is invisible if the crate is gated dir-granular only. Enumerate each
    # src/**/*.rs RECURSIVELY — a non-recursive glob left the src/storage/ subtree
    # (d1_http/r2_kv/r2_s3/region_map) silently un-gated (audit #2). routes/ is
    # also under src/ but already enumerated above; the dedup (dict.fromkeys on the
    # surface list) collapses the overlap. non-handlers (lib.rs is the crate wiring
    # root) are covered via the manifest like the rest.
    # gate v10 fix #1 (MED): same case-insensitive unification — enumerate via the
    # shared `_is_exec_source` predicate (which lowercases the suffix) instead of a
    # case-sensitive `rglob("*.rs")`, so a `src/Poison.RS` (classified strict by the
    # lowercasing classifier) is also ENUMERATED and surfaces as a [C10b] gap.
    container_src = surface_root / "crates" / "corelink-container" / "src"
    surface += list(_iter_exec_sources(container_src, surface_root, js_ts=False, rust=True))
    crates_dir = surface_root / "crates"
    if crates_dir.is_dir():
        surface += [(p.relative_to(surface_root).as_posix(), True) for p in sorted(crates_dir.iterdir()) if p.is_dir()]

    # --- Worker EDGE PLANE + apps/ (BR5 root fix) ------------------------------
    # The completeness oracle must also gate the TypeScript edge plane and the
    # apps/ workers (file-granular), not just the Rust container + ADRs — else a
    # future load-bearing worker/src or apps/*/src file with no concept stays
    # invisible. Enumerated as completeness-required surfaces; legit-exempt files
    # (types/config/test/UI/data) live in the manifest `excludes:`. The recursive
    # executable-source enumeration uses the shared `_iter_exec_sources` helper so
    # the surface walk and `_is_file_granular_strict` agree on the extension set
    # (gate v9).
    # gate v6 fix #3 (audit #3 LOW — worker/src was enumerated NON-recursively):
    # the old gate globbed `worker/src/*.ts` + a HARDCODED `worker/src/lib/*.ts`,
    # so a new `worker/src/<subdir>/*.ts` (any future subdir other than lib/)
    # escaped the surface entirely. Enumerate `worker/src/**` RECURSIVELY.
    # gate v9 fix (HIGH — extension misalignment): enumerate the FULL executable-
    # extension set via the shared `_is_exec_source` predicate (so the surface walk
    # and `_is_file_granular_strict` AGREE) — a `.mts`/`.mjs`/`.cts`/`.cjs`/`.tsx`/
    # `.jsx`/`.js` edge handler (all wrangler-`main`-eligible) is now enumerated, not
    # just `.ts`. Genuine non-handler subdirs surface as [C10b] and get an honest
    # manifest `excludes:` entry, the same as the container tree.
    worker_src = surface_root / "worker" / "src"
    surface += list(_iter_exec_sources(worker_src, surface_root, js_ts=True, rust=False))
    # gate v8 fix #1 (MEDIUM — app enumeration was a HARDCODED 3-app allowlist):
    # the surface walk only enumerated ("signup-worker","cas-worker","analytics-
    # worker"), but `_is_file_granular_strict` classifies EVERY `apps/*/src/**` as
    # strict — MISALIGNED. A brand-new app (e.g. `apps/runner-worker/src/poison.ts`)
    # was classified strict yet never ENUMERATED, so it could never surface as a
    # [C10b] gap → a new app's request-reachable handler shipped GREEN with zero
    # coverage. We now enumerate ALL `apps/*/src/**/*.{ts,rs}` DYNAMICALLY by
    # globbing the apps/ dir for any app that has a `src/`, so strict-classification
    # and the surface walk AGREE. Genuinely-non-app dirs under apps/ stay out of the
    # surface via the existing manifest `excludes:` (apps/admin-ui, apps/get-corelink-
    # worker are exact-prefix excludes honored by is_covered) — and any future
    # non-handler app that is wholesale-waived gets the same honest exclude entry.
    # gate v9 fix (HIGH): same extension unification for apps/*/src/** — enumerate
    # the FULL executable matrix ({.ts,.mts,.cts,.tsx,.js,.mjs,.cjs,.jsx} + .rs) via
    # the shared `_is_exec_source` predicate, not just `.ts`/`.rs`. A new app's
    # `.mts`/`.mjs` request-reachable handler is classified strict, so it must be
    # ENUMERATED too (PoC apps/runner-worker/src/poison.mts now REDs [C10b]).
    apps_dir = surface_root / "apps"
    if apps_dir.is_dir():
        for app in sorted(p for p in apps_dir.iterdir() if p.is_dir()):
            app_src = app / "src"
            if app_src.is_dir():
                surface += list(_iter_exec_sources(app_src, surface_root, js_ts=True, rust=True))

    # gate v10 fix #2 (HIGH): ALSO enumerate every wrangler `main` entrypoint —
    # the real deploy surface — wherever it lives (app-root, a non-src dir, build
    # output). An executable main OUTSIDE src/ surfaces as a required [C10b]
    # surface; an excluded surface stays covered by its existing exact-prefix
    # exclude.
    # gate v11 fix (HIGH): enumerate EVERY declared main — the top-level main AND
    # every per-environment `[env.<name>]` override — so an env-override
    # entrypoint (e.g. `[env.prod] main = "build/worker.mjs"`) that the top-level
    # decoy used to mask is now a required surface.
    # gate v14 fix (MATERIAL): enumerate over EVERY wrangler config found by a
    # RECURSIVE walk of the surface root (`_wrangler_config_dirs`, build-output /
    # vendor dirs pruned) — not a fixed root+apps/*+crates/* location set. A wrangler
    # main pointing OUTSIDE `*/src/**` declared in ANY dir — crate-NESTED
    # (`crates/<x>/cf/`), a sibling top-level dir (`services/edge/`), a `worker/` alt
    # config, etc. — now surfaces as a required [C10b] surface instead of riding
    # coarse crate-dir/cluster adoption from a hidden config LOCATION.
    for cfg_dir in _wrangler_config_dirs(surface_root):
        for main_rel in _wrangler_mains(cfg_dir):
            main_path = (cfg_dir / main_rel).resolve()
            try:
                rel = main_path.relative_to(surface_root).as_posix()
            except ValueError:
                rel = None
            if (
                rel
                and main_path.is_file()
                and _is_exec_source(rel, js_ts=True, rust=True)
            ):
                surface.append((rel, False))

    for rel, is_dir in dict.fromkeys(surface):
        if not is_covered(rel, is_dir):
            fails.add("C10b", str(manifest_path),
                      f"repo-surface item `{rel}` has no manifest entry and no exclude (silent gap)")


def _check_nightly(git: Git, concepts: list[Concept], fails: Failures):
    now = datetime.now(timezone.utc)
    for c in concepts:
        if c.is_deferred or not c.checkpoint_sha or not isinstance(c.checkpoint_sha, str):
            continue
        if not HEX40_RE.match(c.checkpoint_sha):
            continue
        dt = git.commit_date(c.checkpoint_sha)
        if dt is None:
            continue
        if (now - dt).days > 90:
            fails.warn("C-AGE", c.rel, f"checkpoint older than 90 days ({(now - dt).days}d) — re-attest")
    # C-REV reverse-coverage (best-effort WARN): for crate dirs that changed
    # since the oldest referenced checkpoint and are covered by NO concept's
    # source_files, surface a warning (pressure toward source_files completeness;
    # cannot be proven, only surfaced — per the contract).
    declared = set()
    for c in concepts:
        for sf in c.source_files:
            declared.add(sf)
    checkpoints = [c.checkpoint_sha for c in concepts
                   if isinstance(c.checkpoint_sha, str) and HEX40_RE.match(c.checkpoint_sha)]
    if not checkpoints:
        return
    crates_root = git.repo_root / "crates"
    if not crates_root.is_dir():
        return
    for crate in sorted(p for p in crates_root.iterdir() if p.is_dir()):
        rel = crate.relative_to(git.repo_root).as_posix()
        if any(d == rel or d.startswith(rel + "/") for d in declared):
            continue
        # changed since the oldest checkpoint?
        cp = git.run(["diff", "--quiet", checkpoints[0], "HEAD", "--", rel])
        if cp.returncode == 1:  # 1 == differences
            fails.warn("C-REV", rel, "crate changed since checkpoints but no concept declares it")
__all__ = [name for name in globals() if not name.startswith("__")]
