#!/usr/bin/env python3
"""Adversarial regression tests for B217's capacity/candidate safety boundary."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import capacity_guard as guard


class CapacityGuardTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name) / "repo"
        self.root.mkdir()
        self.git_run("git", "init", "-q", "-b", "main")
        self.git_run("git", "config", "user.email", "test@example.invalid")
        self.git_run("git", "config", "user.name", "Capacity Test")
        (self.root / ".gitignore").write_text(
            "target\nnode_modules\n.turbo\n.pnpm-store\n", encoding="utf-8"
        )
        (self.root / "README").write_text("base\n", encoding="utf-8")
        self.git_run("git", "add", "README", ".gitignore")
        self.git_run("git", "commit", "-qm", "base")
        # unsafe_branch's production source is origin/main.  Make the fixture
        # mirror it without network access.
        self.git_run("git", "update-ref", "refs/remotes/origin/main", "HEAD")

    def tearDown(self) -> None:
        self.temp.cleanup()

    def git_run(self, *args: str) -> None:
        subprocess.run(args, cwd=self.root, check=True, capture_output=True)

    def test_report_includes_free_bytes_inodes_and_cache(self) -> None:
        cache = self.root / "target"
        cache.mkdir()
        (cache / "payload").write_bytes(b"x" * 4097)
        report = guard.inspect_worktree(self.root, "main")
        self.assertEqual(report.states, ("clean",))
        self.assertEqual(dict(report.caches)["target"], 4097)
        capacity = guard.filesystem_capacity(self.root)
        self.assertGreater(capacity.free_bytes, 0)
        self.assertGreaterEqual(capacity.free_inodes, 0)

    def test_active_dirty_and_untracked_are_distinct(self) -> None:
        self.git_run("git", "checkout", "-qb", "feature")
        (self.root / "feature-marker").write_text("feature\n", encoding="utf-8")
        self.git_run("git", "add", "feature-marker")
        self.git_run("git", "commit", "-qm", "feature work")
        (self.root / "README").write_text("changed\n", encoding="utf-8")
        (self.root / "untracked").write_text("x\n", encoding="utf-8")
        states = guard.worktree_states(self.root, "feature")
        self.assertEqual(states, ("active", "dirty", "untracked"))

    def test_cleanup_refuses_active_candidate_even_for_regenerable_cache(self) -> None:
        self.git_run("git", "checkout", "-qb", "feature")
        (self.root / "feature-marker").write_text("feature\n", encoding="utf-8")
        self.git_run("git", "add", "feature-marker")
        self.git_run("git", "commit", "-qm", "feature work")
        (self.root / "target").mkdir()
        with self.assertRaisesRegex(guard.GuardError, "active"):
            guard.cleanup_path(self.root, "target")

    def test_cleanup_refuses_dirty_and_untracked_candidate(self) -> None:
        (self.root / "target").mkdir()
        (self.root / "README").write_text("dirty\n", encoding="utf-8")
        (self.root / "new-file").write_text("untracked\n", encoding="utf-8")
        with self.assertRaisesRegex(guard.GuardError, "dirty"):
            guard.cleanup_path(self.root, "target")

    def test_cleanup_refuses_tracked_file_under_cache(self) -> None:
        (self.root / "target").mkdir()
        (self.root / "target" / "tracked").write_text("source\n", encoding="utf-8")
        self.git_run("git", "add", "-f", "target/tracked")
        with self.assertRaisesRegex(guard.GuardError, "tracked files"):
            guard.cleanup_path(self.root, "target")
        self.assertTrue((self.root / "target" / "tracked").exists())

    def test_cleanup_refuses_ignored_data_outside_cache(self) -> None:
        (self.root / ".git" / "info" / "exclude").write_text(".env\n", encoding="utf-8")
        (self.root / ".env").write_text("SECRET=keep\n", encoding="utf-8")
        (self.root / "target").mkdir()
        with self.assertRaisesRegex(guard.GuardError, "ignored data"):
            guard.cleanup_path(self.root, "target")
        self.assertTrue((self.root / ".env").exists())

    def test_cleanup_refuses_divergent_main_even_when_clean(self) -> None:
        (self.root / "local-only").write_text("local\n", encoding="utf-8")
        self.git_run("git", "add", "local-only")
        self.git_run("git", "commit", "-qm", "local divergence")
        (self.root / "target").mkdir()
        with self.assertRaisesRegex(guard.GuardError, "active"):
            guard.cleanup_path(self.root, "target")

    def test_cleanup_accepts_only_fixed_direct_cache_name(self) -> None:
        for target in (".", "../target", "/tmp", "target/*", "${HOME}", ".git"):
            with self.subTest(target=target), self.assertRaises(guard.GuardError):
                guard.cleanup_path(self.root, target)

    def test_cleanup_refuses_symlink_target_without_following_it(self) -> None:
        outside = Path(self.temp.name) / "outside"
        outside.mkdir()
        (outside / "keep").write_text("keep", encoding="utf-8")
        (self.root / "target").symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(guard.GuardError, "non-symlink"):
            guard.cleanup_path(self.root, "target")
        self.assertTrue((outside / "keep").exists())

    def test_execute_refuses_symlink_swap_and_preserves_outside_data(self) -> None:
        outside = Path(self.temp.name) / "outside"
        outside.mkdir()
        (outside / "keep").write_text("keep", encoding="utf-8")
        (self.root / "target").mkdir()
        original_cleanup = guard.cleanup_path
        calls = 0

        def swap_after_initial_validation(root: Path, target: str) -> Path:
            nonlocal calls
            path = original_cleanup(root, target)
            calls += 1
            if calls == 2:
                os.rename(self.root / "target", self.root / "target-original")
                (self.root / "target").symlink_to(outside, target_is_directory=True)
            return path

        with mock.patch.dict(os.environ,
                             {"CORELINK_CAPACITY_ALLOW_DELETE": "delete-regenerable-cache"}), \
             mock.patch.object(guard, "cleanup_path", side_effect=swap_after_initial_validation):
            with self.assertRaisesRegex(guard.GuardError, "descriptor"):
                guard.dry_run_cleanup(self.root, ["target"], execute=True)
        self.assertTrue((outside / "keep").exists())

    def test_execute_refuses_quarantine_external_swap_without_deleting_it(self) -> None:
        outside = Path(self.temp.name) / "outside"
        outside.mkdir()
        (outside / "SECRET").write_text("keep", encoding="utf-8")
        (self.root / "target").mkdir()
        (self.root / "target" / "original").write_text("original", encoding="utf-8")
        original_remove_tree = guard.remove_tree_fd

        def swap_quarantine_then_remove(directory_fd: int) -> None:
            quarantine = next(self.root.glob(".corelink-capacity-quarantine-*") )
            os.rename(quarantine, self.root / "quarantine-original")
            replacement = self.root / "replacement"
            replacement.mkdir()
            (replacement / "SECRET").write_text("keep", encoding="utf-8")
            os.rename(replacement, quarantine)
            original_remove_tree(directory_fd)

        with mock.patch.dict(os.environ,
                             {"CORELINK_CAPACITY_ALLOW_DELETE": "delete-regenerable-cache"}), \
             mock.patch.object(guard, "remove_tree_fd", side_effect=swap_quarantine_then_remove):
            with self.assertRaisesRegex(guard.GuardError, "quarantine replacement"):
                guard.dry_run_cleanup(self.root, ["target"], execute=True)
        replacement = next(self.root.glob(".corelink-capacity-quarantine-*/SECRET"))
        self.assertEqual(replacement.read_text(encoding="utf-8"), "keep")

    def test_execute_refuses_directory_swap_without_deleting_replacement(self) -> None:
        (self.root / "target").mkdir()
        (self.root / "target" / "SECRET").write_text("original", encoding="utf-8")
        original_rename = guard.os.rename

        def swap_then_rename(src: str, dst: str, *, src_dir_fd: int, dst_dir_fd: int) -> None:
            if src == "target":
                original_rename(self.root / "target", self.root / "target-original")
                (self.root / "target").mkdir()
                (self.root / "target" / "SECRET").write_text("replacement", encoding="utf-8")
            original_rename(src, dst, src_dir_fd=src_dir_fd, dst_dir_fd=dst_dir_fd)

        with mock.patch.dict(os.environ,
                             {"CORELINK_CAPACITY_ALLOW_DELETE": "delete-regenerable-cache"}), \
             mock.patch.object(guard.os, "rename", side_effect=swap_then_rename):
            with self.assertRaisesRegex(guard.GuardError, "replacement"):
                guard.dry_run_cleanup(self.root, ["target"], execute=True)
        self.assertEqual((self.root / "target-original" / "SECRET").read_text(encoding="utf-8"),
                         "original")
        self.assertEqual((self.root / "target" / "SECRET").read_text(encoding="utf-8"),
                         "replacement")

    def test_execute_refuses_external_swap_before_descriptor_acquisition(self) -> None:
        outside = Path(self.temp.name) / "outside"
        outside.mkdir()
        (outside / "SECRET").write_text("keep", encoding="utf-8")
        (self.root / "target").mkdir()
        (self.root / "target" / "original").write_text("original", encoding="utf-8")
        original_cleanup = guard.cleanup_path
        calls = 0

        def swap_after_initial_validation(root: Path, target: str) -> Path:
            nonlocal calls
            path = original_cleanup(root, target)
            calls += 1
            if calls == 2:
                os.rename(self.root / "target", self.root / "target-original")
                os.rename(outside, self.root / "target")
            return path

        with mock.patch.dict(os.environ,
                             {"CORELINK_CAPACITY_ALLOW_DELETE": "delete-regenerable-cache"}), \
             mock.patch.object(guard, "cleanup_path", side_effect=swap_after_initial_validation):
            with self.assertRaisesRegex(guard.GuardError, "replacement"):
                guard.dry_run_cleanup(self.root, ["target"], execute=True)
        self.assertTrue((self.root / "target" / "SECRET").exists())

    def test_root_rejects_filesystem_root_symlink_glob_and_subdirectory(self) -> None:
        link = Path(self.temp.name) / "repo-link"
        link.symlink_to(self.root, target_is_directory=True)
        for raw in ("/", str(link), str(self.root / "*"), str(self.root / ".git")):
            with self.subTest(raw=raw), self.assertRaises(guard.GuardError):
                guard.validated_root(raw)

    def test_detached_worktree_is_protected_as_active(self) -> None:
        self.git_run("git", "checkout", "--detach", "-q", "HEAD")
        (self.root / "target").mkdir()
        with self.assertRaisesRegex(guard.GuardError, "active"):
            guard.cleanup_path(self.root, "target")

    def test_malformed_floor_environment_fails_closed(self) -> None:
        with mock.patch.dict(os.environ, {"CORELINK_CAPACITY_FLOOR_MIB": "not-a-number"}):
            with self.assertRaises(argparse.ArgumentTypeError):
                guard.parse_args([])

    def test_malformed_floor_environment_main_returns_indeterminate(self) -> None:
        with mock.patch.dict(os.environ, {"CORELINK_CAPACITY_FLOOR_MIB": "not-a-number"}):
            self.assertEqual(guard.main(["--root", str(self.root)]), 2)

    def test_nonfinite_lock_timeout_fails_closed(self) -> None:
        for value in ("nan", "inf", "-inf"):
            with self.subTest(value=value), self.assertRaises(SystemExit) as raised:
                guard.parse_args(["--lock-timeout-seconds", value])
            self.assertEqual(raised.exception.code, 2)

    def test_unavailable_worktree_evidence_fails_closed(self) -> None:
        unavailable = guard.WorktreeReport(self.root, None, ("unavailable",), ())
        with mock.patch.object(guard, "collect_report", return_value=[unavailable]):
            self.assertEqual(guard.main(["--root", str(self.root)]), 2)

    def test_cache_read_error_is_indeterminate(self) -> None:
        cache = self.root / "target"
        cache.mkdir()
        with mock.patch.object(guard.os, "scandir", side_effect=PermissionError("denied")):
            with self.assertRaisesRegex(guard.GuardError, "cannot read cache directory"):
                guard.directory_size(cache)

    def test_cache_scan_bound_is_indeterminate(self) -> None:
        cache = self.root / "target" / "nested"
        cache.mkdir(parents=True)
        (cache / "payload").write_bytes(b"x")
        with mock.patch.object(guard, "MAX_CACHE_SCAN_ENTRIES", 1):
            with self.assertRaisesRegex(guard.GuardError, "scan exceeded"):
                guard.directory_size(self.root / "target")

    def test_huge_ignored_path_evidence_is_bounded(self) -> None:
        (self.root / ".git" / "info" / "exclude").write_text("ignored-*\n", encoding="utf-8")
        for number in range(3):
            (self.root / f"ignored-{number}").write_text("x\n", encoding="utf-8")
        with mock.patch.object(guard, "MAX_IGNORED_PATHS", 2):
            with self.assertRaisesRegex(guard.GuardError, "path evidence"):
                guard.ignored_non_cache_paths(self.root)

    def test_ignored_disposable_cache_is_excluded_before_capture(self) -> None:
        (self.root / "target").mkdir()
        (self.root / ".git" / "info" / "exclude").write_text("target\n", encoding="utf-8")
        for number in range(3):
            (self.root / "target" / f"artifact-{number}").write_text("x\n", encoding="utf-8")
        with mock.patch.object(guard, "MAX_IGNORED_PATHS", 0):
            self.assertEqual(guard.ignored_non_cache_paths(self.root), [])

    def test_hung_git_evidence_is_indeterminate(self) -> None:
        timeout = subprocess.TimeoutExpired(["git"], guard.GIT_COMMAND_TIMEOUT_SECONDS)
        with mock.patch.object(guard, "_run_bounded", side_effect=timeout):
            with self.assertRaisesRegex(guard.GuardError, "timed out"):
                guard.git(self.root, "status")

    def test_report_inspection_deadline_is_hard(self) -> None:
        original = guard.MAX_REPORT_SECONDS
        guard.MAX_REPORT_SECONDS = 0.05
        try:
            with mock.patch.object(guard, "parse_worktree_list",
                                   return_value=[(self.root, "main")]), \
                 mock.patch.object(guard, "inspect_worktree",
                                   side_effect=lambda *args, **kwargs: __import__("time").sleep(0.1)):
                with self.assertRaisesRegex(guard.GuardError, "report exceeded"):
                    guard.collect_report(self.root)
        finally:
            guard.MAX_REPORT_SECONDS = original

    def test_dry_run_never_calls_rmtree(self) -> None:
        (self.root / "target").mkdir()
        with mock.patch.object(guard, "remove_tree_fd") as remove_tree:
            guard.dry_run_cleanup(self.root, ["target"], execute=False)
        remove_tree.assert_not_called()
        self.assertTrue((self.root / "target").is_dir())

    def test_execute_requires_acknowledgement_and_keeps_cache_without_it(self) -> None:
        (self.root / "target").mkdir()
        with mock.patch.dict(os.environ, {}, clear=True):
            with self.assertRaisesRegex(guard.GuardError, "execution requires"):
                guard.dry_run_cleanup(self.root, ["target"], execute=True)
        self.assertTrue((self.root / "target").is_dir())

    def test_execute_is_narrow_and_idempotent_for_explicit_cache(self) -> None:
        (self.root / "target").mkdir()
        (self.root / "target" / "artifact").write_bytes(b"artifact")
        with mock.patch.dict(os.environ,
                             {"CORELINK_CAPACITY_ALLOW_DELETE": "delete-regenerable-cache"}):
            guard.dry_run_cleanup(self.root, ["target", "target"], execute=True)
            guard.dry_run_cleanup(self.root, ["target"], execute=True)
        self.assertFalse((self.root / "target").exists())

    def test_parallel_materialization_lock_refuses_second_holder(self) -> None:
        with guard.materialization_lock(self.root, 0):
            code = (
                "import pathlib,sys; "
                "sys.path.insert(0, sys.argv[1]); "
                "import capacity_guard as g; "
                "raise SystemExit(g.main(['--root', sys.argv[2], '--lock', '--', 'true']))"
            )
            child = subprocess.run(
                [sys.executable, "-c", code, str(Path(__file__).resolve().parent), str(self.root)],
                capture_output=True,
                text=True,
                check=False,
            )
        self.assertEqual(child.returncode, 2, child.stderr)
        self.assertIn("materialization lock is held", child.stderr)

    def test_gate_returns_one_below_floor_and_not_two(self) -> None:
        capacity = guard.FilesystemCapacity(8 * 1024 * 1024 + 123, 100)
        with mock.patch.object(guard, "filesystem_capacity", return_value=capacity):
            self.assertEqual(guard.main(["--root", str(self.root), "--gate", "--floor-mib",
                                         "9"]), 1)

    def test_gate_returns_one_when_inode_evidence_is_below_floor(self) -> None:
        with mock.patch.object(guard, "filesystem_capacity",
                               return_value=guard.FilesystemCapacity(10 * 1024**3, 0)):
            self.assertEqual(guard.main(["--root", str(self.root), "--gate",
                                         "--floor-mib", "1"]), 1)

    def test_gate_rejects_unknown_inode_evidence(self) -> None:
        with mock.patch.object(guard, "filesystem_capacity",
                               return_value=guard.FilesystemCapacity(10 * 1024**3, -1)):
            self.assertEqual(guard.main(["--root", str(self.root), "--gate",
                                         "--floor-mib", "1"]), 1)

    def test_lock_propagates_child_failure_without_claiming_success(self) -> None:
        self.assertEqual(
            guard.main(["--root", str(self.root), "--lock", "--",
                        sys.executable, "-c", "raise SystemExit(28)"]),
            28,
        )

    def test_lock_symlink_is_refused_without_following_target(self) -> None:
        outside = Path(self.temp.name) / "lock-outside"
        outside.write_text("keep", encoding="utf-8")
        lock_path = self.root / ".git" / "corelink-capacity-materialization.lock"
        lock_path.symlink_to(outside)
        with self.assertRaisesRegex(guard.GuardError, "lock"):
            with guard.materialization_lock(self.root, 0):
                pass
        self.assertEqual(outside.read_text(encoding="utf-8"), "keep")

    def test_lock_fifo_is_refused_without_blocking(self) -> None:
        lock_path = self.root / ".git" / "corelink-capacity-materialization.lock"
        os.mkfifo(lock_path)
        with self.assertRaisesRegex(guard.GuardError, "lock"):
            with guard.materialization_lock(self.root, 0):
                pass

    def test_locked_command_timeout_is_indeterminate(self) -> None:
        self.assertEqual(
            guard.main(["--root", str(self.root), "--lock", "--command-timeout-seconds", "0.01",
                        "--", sys.executable, "-c", "import time; time.sleep(1)"]),
            2,
        )

    def test_locked_command_timeout_kills_descendants(self) -> None:
        marker = Path(self.temp.name) / "late-marker"
        child = (
            "import pathlib,time; time.sleep(.3); "
            f"pathlib.Path({str(marker)!r}).write_text('leaked')"
        )
        parent = (
            "import subprocess,sys,time; "
            f"subprocess.Popen([sys.executable,'-c',{child!r}]); time.sleep(2)"
        )
        self.assertEqual(
            guard.main(["--root", str(self.root), "--lock", "--command-timeout-seconds", "0.05",
                        "--", sys.executable, "-c", parent]),
            2,
        )
        time.sleep(0.4)
        self.assertFalse(marker.exists())

    def test_lock_path_replacement_cannot_split_ownership(self) -> None:
        common = Path(guard.git(self.root, "rev-parse", "--git-common-dir").strip())
        if not common.is_absolute():
            common = (self.root / common).resolve()
        lock = common / "corelink-capacity-materialization.lock"
        moved = common / "old-lock"
        code = (
            "import pathlib,sys\n"
            "sys.path.insert(0,sys.argv[1])\n"
            "import capacity_guard as g\n"
            "raise SystemExit(g.main(['--root',sys.argv[2],'--lock','--','true']))\n"
        )
        with guard.materialization_lock(self.root, 0):
            os.rename(lock, moved)
            lock.write_text("replacement", encoding="utf-8")
            child = subprocess.run(
                [sys.executable, "-c", code, str(Path(__file__).resolve().parent), str(self.root)],
                capture_output=True, text=True, check=False, timeout=3,
            )
        self.assertEqual(child.returncode, 2, child.stderr)
        self.assertNotIn("ENTERED", child.stdout)

    def test_gate_rechecks_capacity_after_waiting_for_lock(self) -> None:
        initial = guard.FilesystemCapacity(10 * 1024**3, 100)
        exhausted = guard.FilesystemCapacity(0, 100)
        with mock.patch.object(guard, "filesystem_capacity", side_effect=[initial, exhausted]):
            self.assertEqual(
                guard.main(["--root", str(self.root), "--gate", "--floor-mib", "1",
                            "--lock", "--", sys.executable, "-c",
                            f"raise SystemExit(99)"]),
                1,
            )


if __name__ == "__main__":
    unittest.main()
