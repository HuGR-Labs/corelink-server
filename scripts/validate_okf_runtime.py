"""Check orchestration for the OKF validator."""
from __future__ import annotations

from pathlib import Path
from validate_okf_core1 import *
from validate_okf_core2 import *
from validate_okf_checks import *


def _under(p: Path, root: Path) -> bool:
    """Return whether ``p`` is inside ``root`` (including ``root`` itself).

    This helper used to live only in ``validate_okf.py``.  After the validator
    split, the runtime module owns the orchestration functions and therefore
    must carry the helper in its own global namespace; relying on a caller's
    module globals makes both imports and standalone fixture copies fail.
    """
    try:
        p.relative_to(root)
        return True
    except ValueError:
        return False


def run_checks(args, git: Git, fails: Failures):
    repo_root = git.repo_root
    line_count_cache: dict[Path, tuple[int, int, int, int, int]] = {}
    bundle_root = Path(args.bundle)
    if not bundle_root.is_absolute():
        bundle_root = (Path.cwd() / bundle_root).resolve()

    concepts: list[Concept] = []
    reserved_files: list[Path] = []
    deferred_ids: set[str] = set()

    if bundle_root.is_dir():
        for md in sorted(bundle_root.rglob("*.md")):
            rel = md.relative_to(bundle_root).as_posix()
            # reserved files only at bundle root
            if "/" not in rel and md.name in RESERVED_NAMES:
                reserved_files.append(md)
                continue
            concepts.append(Concept(md, bundle_root))

    # --- C9: reserved files must not be used as concepts (carry source_files) ---
    for rf in reserved_files:
        block, _ = _split(rf.read_text(encoding="utf-8"))
        if block is not None:
            try:
                fm = parse_frontmatter(block)
            except Exception:
                fm = {}
            if "source_files" in fm or "checkpoint_sha" in fm:
                fails.add(
                    "C9",
                    rf.relative_to(repo_root).as_posix() if _under(rf, repo_root) else rf.name,
                    "reserved file used as a concept (declares source_files/checkpoint_sha)",
                )

    _preload_sha_exists(git, [c.checkpoint_sha for c in concepts if isinstance(c.checkpoint_sha, str) and HEX40_RE.match(c.checkpoint_sha)])

    # Per-concept structural checks.
    for c in concepts:
        loc = f"{bundle_root.name}/{c.rel}" if not _under(c.path, repo_root) else c.path.relative_to(repo_root).as_posix()

        # C9: a concept must not occupy a reserved root path (auto-excluded above,
        # but a subdir file literally named with reserved name is allowed; nothing).

        # C1: parseable frontmatter + non-empty type
        if not c.has_frontmatter:
            fails.add("C1", loc, "no parseable YAML frontmatter")
            continue
        if c.parse_error:
            fails.add("C1", loc, f"unparseable frontmatter: {c.parse_error}")
            continue
        if not c.type:
            fails.add("C1", loc, "missing/empty `type`")
            # keep going for other field checks

        if c.is_deferred:
            deferred_ids.add(c.concept_id)
            # deferred: exempt from grounding/body, but MUST have type + title
            if not c.title:
                fails.add("C2", loc, "deferred concept missing `title`")
            continue

        # C2: required fields on a non-deferred concept
        if not c.title:
            fails.add("C2", loc, "missing required field `title`")
        if not c.source_files:
            fails.add("C2", loc, "missing required field `source_files` (>=1)")
        if not c.checkpoint_sha:
            fails.add("C2", loc, "missing required field `checkpoint_sha`")

        # C3: every source_files path exists verbatim at HEAD (no rename-follow)
        missing_sources: set[str] = set()
        for sf in c.source_files:
            if not (repo_root / sf).exists():
                missing_sources.add(sf)
                fails.add("C3", loc, f"source_files path does not exist at HEAD: `{sf}`")

        # C4: checkpoint_sha is a well-formed commit that is REACHABLE from HEAD.
        # A commit object that merely happens to remain in a local object database
        # is not sufficient: that is the exact timing-dependent false-green caused
        # by squash/rebase orphaning. Both local validation and a fresh CI clone
        # must resolve the same history anchor, so missing and non-ancestor commits
        # fail closed with no base-ref fallback.
        ckpt_ok = False
        if c.checkpoint_sha:
            if not isinstance(c.checkpoint_sha, str) or not HEX40_RE.match(c.checkpoint_sha):
                fails.add("C4", loc, f"checkpoint_sha is not 40-hex: `{c.checkpoint_sha}`")
            elif git.sha_exists(c.checkpoint_sha):
                if git.is_ancestor(c.checkpoint_sha, "HEAD"):
                    ckpt_ok = True
                else:
                    fails.add(
                        "C4", loc,
                        f"checkpoint_sha `{c.checkpoint_sha[:12]}` resolves to a commit "
                        "that is not reachable from HEAD — squash/rebase orphan or "
                        "foreign history (fail-closed; re-anchor and reconcile)",
                    )
            else:
                fails.add(
                    "C4", loc,
                    f"checkpoint_sha `{c.checkpoint_sha[:12]}` is not a resolvable "
                    "commit in this clone — unreachable/missing anchor (fail-closed; "
                    "re-anchor and reconcile)",
                )

        # C4b: `source_blobs` — the per-file BLOB anchor (§2.2). Every rule here
        # is a HARD failure with no tolerance and no fallback, which is the whole
        # point of the key: blob ids are content hashes and
        # survive rebase, squash and cherry-pick untouched, so a blob that was
        # ever pushed is still in the gate's fetch-depth:0 clone. An unresolvable
        # blob is therefore a typo or a forgery, never an orphan — and it REDs
        # here rather than silently re-anchoring C5 to the base ref (which is the
        # exact move that let a forged unreachable checkpoint launder an
        # already-landed drift past C5).
        for bad in c.source_blobs_bad:
            fails.add(
                "C4b", loc,
                f"malformed `source_blobs` entry (want `path@<40-hex blob>`): `{bad}`",
            )
        for dupe in c.source_blobs_dupe:
            fails.add(
                "C4b", loc,
                f"duplicate `source_blobs` entry for path `{dupe}` — one blob anchor per file",
            )
        for bpath, bsha in c.source_blobs.items():
            if bpath not in set(c.source_files):
                fails.add(
                    "C4b", loc,
                    f"`source_blobs` anchors a path not declared in `source_files`: "
                    f"`{bpath}` (it would anchor nothing C5 gates)",
                )
                continue
            if bpath in missing_sources:
                continue  # C3 already reported the vanished path
            if not git.blob_is_present(bsha):
                fails.add(
                    "C4b", loc,
                    f"`source_blobs` anchor `{bpath}@{bsha[:12]}` is not a blob object "
                    "present in this clone — a blob id is immutable under rebase/squash/"
                    "cherry-pick, so this is a typo or a forged anchor, NOT a squash-orphan "
                    "(no base-ref fallback applies)",
                )
                continue
            # C4b REACHABILITY (the false-green closure). Presence in the object
            # database is a TIMING artifact — see `Git.blob_reachable_for_path`.
            # An anchor must name content this history actually carries, so the
            # verdict is identical in the `push:main` run that clones seconds
            # after the squash-merge and in every clone that comes after it.
            if not git.blob_reachable_for_path(bpath, bsha):
                fails.add(
                    "C4b", loc,
                    f"`source_blobs` anchor `{bpath}@{bsha[:12]}` resolves as a blob in "
                    "this clone but is NOT the content of that path at ANY commit "
                    "reachable from HEAD — it never landed on this history (typically an "
                    "intermediate PR commit superseded before the squash-merge, whose only "
                    "ref is the auto-deleted PR head). Re-anchor to the blob that landed",
                )

        # Build cited-line ranges per file (HEAD coordinates).
        ranges_by_file: dict[str, list[tuple[int, int]]] = {}
        for (f, l1, l2) in c.cites:
            ranges_by_file.setdefault(f, []).append((l1, l2))

        # C6: inline cites resolve (file exists at HEAD + lines in bounds);
        #     every source_files path is cited.
        for (f, l1, l2) in c.cites:
            if f in missing_sources:
                continue  # already reported under C3; don't double-count
            fpath = repo_root / f
            if not fpath.exists():
                fails.add("C6", loc, f"dangling citation — file does not exist: `{f}:{l1}`")
                continue
            n = _line_count(fpath, line_count_cache)
            if l1 < 1 or l2 > n:
                fails.add("C6", loc, f"citation out of range: `{f}:{l1}-{l2}` (file has {n} lines)")
        cited_set = {f for (f, _, _) in c.cites}
        for sf in c.source_files:
            if sf in missing_sources:
                continue
            if sf not in cited_set:
                fails.add("C6", loc, f"declared source not cited under `# Citations`: `{sf}`")

        # C6b: every cited file must be declared in source_files
        for f in cited_set:
            if f not in set(c.source_files):
                fails.add("C6b", loc, f"cited file not declared in source_files: `{f}`")

        # C6c: per-claim grounding under `# How it works` and `# Invariants`
        #      (relaxed for ADRs per §4.1)
        if not c.is_adr:
            # The test-only-grounding exemption is for a concept whose SUBJECT *is*
            # the test harness — there the test IS the enforcer, so grounding an
            # invariant on a test path is correct (parallel to C6c being relaxed for
            # ADRs per §4.1).
            #
            # gate v6 fix #2 (audit #2 MED — the exemption was defeated by file
            # PLACEMENT): the old gate keyed the exemption on the doc living under
            # `docs/knowledge/testing/` ALONE. An author could file a SECURITY /
            # auth / compliance invariant at `docs/knowledge/testing/sneaky.md`
            # grounded SOLELY on a test and inherit the exemption — a neuterable
            # grounding masquerading as enforced. (The earlier `type ==
            # "TestStrategy"` ALONE was the symmetric dodge — self-declare the type
            # under auth/ and shed the requirement.) The exemption now requires
            # BOTH axes to agree: the doc lives under `testing/` AND its declared
            # `type` is a testing type (`TestStrategy`). A concept ABOUT the harness
            # satisfies both; a security/auth/compliance concept satisfies neither —
            # so it cannot inherit the test-enforcer exemption by placement OR by a
            # frontmatter relabel alone (it would have to mislabel BOTH the folder
            # and the type, which is review-visible and miscategorizes it in the
            # taxonomy/index). This binds the exemption to the concept being a
            # genuine test-harness concept, closing the placement bypass without
            # breaking the 3 legit testing/ concepts (all `type: TestStrategy`).
            _TESTING_TYPES = {"TestStrategy"}
            _in_testing_dir = (c.rel == "testing" or c.rel.startswith("testing/"))
            _is_testing_type = (
                isinstance(c.type, str) and c.type.strip() in _TESTING_TYPES
            )
            is_testing = _in_testing_dir and _is_testing_type
            secs = _sections(c.body)
            for title in ("how it works", "invariants"):
                if title in secs:
                    for block in _bullet_blocks(secs[title]):
                        paths = _block_cite_paths(block)
                        first = block.splitlines()[0].strip()
                        if not paths:
                            fails.add(
                                "C6c",
                                loc,
                                f"ungrounded claim under `# {title.title()}` (no path:line): {first[:70]!r}",
                            )
                        elif not is_testing and all(_is_test_cite(p) for p in paths):
                            # An invariant must be grounded on a NON-test enforcer:
                            # a test path (isolation_tests.rs:42 …) as the SOLE cite
                            # lets the grounding be neutered later by editing the
                            # test. Tests are allowed only as ADDITIONAL cites.
                            fails.add(
                                "C6c",
                                loc,
                                f"test-only grounding under `# {title.title()}` "
                                f"(an invariant must cite a non-test enforcer; sole cite(s) {paths!r} "
                                f"are all test paths): {first[:70]!r}",
                            )

        # C5: freshness — CONTENT-ANCHOR. For each cited range compare the
        # CONTENT of those exact lines between the baseline and the working tree;
        # drift fires whether the cause is an in-range edit OR a pure position-shift.
        # Baseline = the reachable `checkpoint_sha`, unless this file has a
        # source_blobs anchor (which takes precedence and survives history rewrites).
        #
        # BLOB ANCHOR (§2.2 `source_blobs`) takes precedence PER FILE. A file with
        # a blob anchor is compared against that blob and nothing else. Files
        # without one use the reachable checkpoint commit; there is no orphan
        # tolerance or base-ref fallback. Migration remains incremental because
        # anchors are per-file, but every concept must retain a valid checkpoint.
        c5_commit_baseline = c.checkpoint_sha if ckpt_ok else None
        commit_anchor_kind = "checkpoint"

        # The set of (file, ranges) C5 will actually compare.
        c5_files: list[str] = []
        for sf in c.source_files:
            if sf in missing_sources:
                continue
            # ADR sub-profile: an accepted ADR does not go stale on the code
            # it governs — C5 applies only to the ADR file's own content.
            if c.is_adr and not sf.endswith(".md"):
                continue
            if not ranges_by_file.get(sf):
                continue
            c5_files.append(sf)
        for sf in c5_files:
            blob_anchor = c.source_blobs.get(sf)
            if blob_anchor:
                c5_baseline, anchor_kind = blob_anchor, "blob anchor"
            elif c5_commit_baseline:
                c5_baseline, anchor_kind = c5_commit_baseline, commit_anchor_kind
            else:
                continue  # no anchor and no blob — already hard-failed above
            cranges = ranges_by_file.get(sf, [])
            # EVERY drifted range is reported, not just the first. This used
            # to `break` after the first hit per (concept, file), which made
            # the C5 report a LOWER BOUND: an author who fixed exactly what
            # the gate printed could still be left with stale citations in
            # the same file, and only a manual `grep` over the concept found
            # the real set — a fix/re-run/fix loop per drifted range. The
            # ranges are already computed; reporting all of them costs one
            # more comparison per cite and turns C5's output into the
            # COMPLETE worklist it is consumed as (okf_reconcile has always
            # reported every range — this makes the gate agree with it).
            # Ranges are de-duplicated first (a concept legitimately cites the
            # same `path:Lx-Ly` under both `# How it works` and `# Citations`,
            # which would otherwise print the identical failure twice and
            # inflate the failure COUNT) — the same `dict.fromkeys` dedup
            # okf_reconcile already applies to its hit list.
            for (l1, l2) in dict.fromkeys(cranges):
                if cited_range_drifted(git, c5_baseline, sf, l1, l2, blob_sha=blob_anchor):
                    fails.add(
                        "C5",
                        loc,
                        f"STALE: cited content `{sf}:{l1}-{l2}` no longer matches "
                        f"{anchor_kind} {c5_baseline[:12]} "
                        "(in-range edit or position-shift)",
                    )

    # --- C5b: SHA advanced without a body edit (needs a 'previous' version) ---
    _check_c5b(args, git, bundle_root, concepts, fails)

    # --- C4c: blob addressing is a RATCHET (needs a 'previous' version) ---
    _check_c4c(args, git, bundle_root, concepts, fails)

    # --- C5c: a moved blob anchor must be paid for with citation renumbering ---
    from okf_anchor_reverify import _check_anchor_content_reverify  # noqa: E402
    _check_anchor_content_reverify(args, git, bundle_root, concepts, fails)

    # --- C10 / C10b: manifest (skip-with-warning when absent/unparseable) ---
    # Runs before C7 because it populates the planned-id set used by C7 tolerance.
    _check_manifest(args, bundle_root, concepts, deferred_ids, fails)

    # --- C7: bundle-relative links to declared concepts resolve ---
    # --- C8: every concept reachable from index.md ---
    _check_links_and_orphans(bundle_root, concepts, deferred_ids, args, fails)

    # --- Secondary nightly WARN checks ---
    if args.nightly:
        _check_nightly(git, concepts, fails)

    # Counts for the success string.
    n_concepts = sum(1 for c in concepts if not c.is_deferred and c.has_frontmatter)
    n_deferred = sum(1 for c in concepts if c.is_deferred)
    return n_concepts, n_deferred
__all__ = [name for name in globals() if not name.startswith("__")]
