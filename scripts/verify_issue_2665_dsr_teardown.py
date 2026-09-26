#!/usr/bin/env python3
"""Guard the frozen DSR ownership and exact-delete seam for issue #2665."""

from pathlib import Path
import sys


SOURCE_PATH = Path("crates/corelink-container/src/routes/dsr/access.rs")
SOURCE = SOURCE_PATH.read_text(encoding="utf-8")


def require(condition: bool, message: str) -> None:
    if not condition:
        print(f"issue-2665 DSR teardown verification failed: {message}", file=sys.stderr)
        raise SystemExit(1)


rectification = SOURCE.split("pub(super) fn run_rectification(", 1)[1].split(
    "\n}\n\n#[cfg(test)]\n#[allow(", 1
)[0]
persist = SOURCE.split("fn persist_owned_r2_with(", 1)[1].split(
    "fn staging_receipt_ref(", 1
)[0]
teardown = SOURCE.split("pub(crate) async fn delete_staging_dsr_export_and_readback(", 1)[1]

require("StagingLoadTestResourceClass::AuditEvidence" in SOURCE, "retained DSR audit registration missing")
require("StagingLoadTestResourceClass::DsrArtifact" not in rectification, "rectification still registers disposable DSR state")
require("StagingLoadTestDisposition::Disposable" not in rectification, "rectification still claims disposable ownership")
require(persist.index("intent.prepare_statement") < persist.index("put_object(object_key") < persist.index("intent.commit_statements"), "R2 prepare/PUT/commit order changed")
require("INSERT INTO staging_load_test_teardown_locators" in persist, "typed 0151 locator insert missing")
require("'dsr_r2_export_v1'" in persist, "DSR locator kind changed")
require("pub(crate) struct StagingDsrR2ExportLocator" in SOURCE, "frozen locator adapter type missing")
require("pub(crate) enum StagingDsrTeardownError" in SOURCE, "typed fail-closed teardown errors missing")
require(
    "delete(locator.object_key.clone())" in teardown
    and "head_size(locator.object_key.clone())" in teardown
    and "r2.delete(&key).await" in teardown
    and "r2.head_size(&key).await" in teardown,
    "delete/HEAD exact-key readback missing",
)
require("StagingDsrTeardownError::ObjectStillPresent" in teardown, "HEAD residue is not rejected")

print("issue-2665 DSR teardown contract verification: PASS")
