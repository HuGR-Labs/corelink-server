### Added

- **#1644 / B-032 Critical-vendor review census.** Added a fail-closed hosted
  contract manifest and verifier for the seven Critical vendors, their quarterly
  dates, VP-Sec ownership, local template paths, and external Drata/account
  evidence boundaries. The packet remains `pending_external`; no review,
  signature, approval, report, or credential is claimed.
- Each vendor packet now names repository evidence anchors and requires explicit
  scope, subprocessor, residency, security/DPA change, renewal, decision, and
  signer fields; all remain pending until a human supplies dated evidence.
- The weekly compliance gate treats any vendor review past its cadence as a
  regression, so a stale Critical review fails the hosted cadence check.

Refs #1644.

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
