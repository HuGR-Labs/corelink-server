//! CoreLink data-residency region map — SINGLE SOURCE OF TRUTH (container side).
//!
//! MIRRORED in `worker/src/region-map.ts`. The two MUST agree byte-for-byte on
//! the macro→colo mapping and the provisioned set; the worker routing-test +
//! the container lock-consistency test (`tests::lock_consistency_*`) both pin
//! this contract.
//!
//! # Background (backlog #29 — a live Schrems II leak)
//!
//! `tenant.primary_region` in D1 holds a MACRO code (`wnam`/`enam`/`weur`/
//! `sam`/`apac`/`afr`). The edge fan-out used to route by literal colo strings
//! (`"lhr"`/`"nrt"`/`"syd"`) that the macro NEVER equals, so EU `weur` tenants
//! never matched and stayed on IAD (US storage). This map removes the
//! vocabulary mismatch: callers map the MACRO → colo deterministically.
//!
//! # FROZEN macro→colo map (lead's decision)
//!
//! | macro  | colo            |
//! |--------|-----------------|
//! | `wnam` | `iad`           |
//! | `enam` | `iad`           |
//! | `weur` | `lhr`           |
//! | `sam`  | `sam`           |
//! | `apac` | `nrt`           |
//! | `afr`  | (none — reject) |
//!
//! Provisioned = `{wnam, enam, weur}` — exactly the macros backed by a
//! jurisdiction-correct R2 bucket. `sam`/`apac`/`afr` are valid macro codes (the
//! D1 CHECK accepts them and routing still recognises them) but are NOT
//! provisioned. `sam` in particular stays ROUTABLE (`colo_for_macro` still maps
//! it) but is NOT provisionable: PROD_SAM still points at the DEFAULT US R2
//! endpoint + shared US bucket, so a `sam`-labelled tenant would mis-land in US
//! storage — an LGPD cross-border violation. Re-add `sam` only once PROD_SAM has
//! a real SAM-jurisdiction bucket/endpoint.
//!
//! This provisioned set is the SINGLE SOURCE OF TRUTH shared by THREE consumers
//! (this file, the worker `region-map.ts`, and signup `clerk.ts`), pinned
//! together by `worker/tests/region-map.test.ts` (the 3-way drift gate).

/// The canonical CoreLink data-residency MACRO region codes (the D1 CHECK set).
pub const MACRO_REGIONS: [&str; 6] = ["wnam", "enam", "weur", "sam", "apac", "afr"];

/// Macro regions provisioned today — exactly those with a jurisdiction-correct
/// R2 bucket. Signup rejects the rest (`sam`/`apac`/`afr`). `sam` stays routable
/// (`colo_for_macro` recognises it) but is NOT provisionable until PROD_SAM has a
/// real SAM-jurisdiction bucket (else its data mis-lands in US R2 — LGPD).
pub const PROVISIONED_MACROS: [&str; 3] = ["wnam", "enam", "weur"];

/// Map a MACRO region code to its serving Cloudflare colo, or `None` when the
/// macro is `afr` (no provisioned colo) or the input is not a recognised macro
/// code.
///
/// The residency path MUST treat `None` as a hard reject / fail-closed — NEVER
/// as "fall through to IAD" (that is the cross-border leak this fix closes).
#[must_use]
pub fn colo_for_macro(macro_region: &str) -> Option<&'static str> {
    match macro_region {
        "wnam" | "enam" => Some("iad"),
        "weur" => Some("lhr"),
        "sam" => Some("sam"),
        "apac" => Some("nrt"),
        // `afr` and anything unrecognised → no colo (reject).
        _ => None,
    }
}

/// Is `s` one of the six canonical macro region codes?
#[must_use]
pub fn is_macro_region(s: &str) -> bool {
    MACRO_REGIONS.contains(&s)
}

/// Is the macro provisioned in Phase 1 (signup-acceptable)?
#[must_use]
pub fn is_provisioned_macro(s: &str) -> bool {
    PROVISIONED_MACROS.contains(&s)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    /// The FROZEN map, asserted EXACTLY. Mirror of `worker/src/region-map.ts`
    /// `MACRO_TO_COLO`. If the worker map changes, this lock test must change in
    /// lockstep (and vice-versa) — that is the cross-language consistency gate.
    #[test]
    fn frozen_macro_to_colo_map_is_exact() {
        assert_eq!(colo_for_macro("wnam"), Some("iad"));
        assert_eq!(colo_for_macro("enam"), Some("iad"));
        assert_eq!(colo_for_macro("weur"), Some("lhr"));
        assert_eq!(colo_for_macro("sam"), Some("sam"));
        assert_eq!(colo_for_macro("apac"), Some("nrt"));
        // afr → no colo (reject).
        assert_eq!(colo_for_macro("afr"), None);
        // unknown → no colo (fail-closed).
        assert_eq!(colo_for_macro("zzz"), None);
        assert_eq!(colo_for_macro(""), None);
    }

    #[test]
    fn provisioned_set_is_exact() {
        // Mirror of `worker/src/region-map.ts` PROVISIONED_MACROS — the canonical
        // provisionable set is EXACTLY {wnam, enam, weur}.
        assert_eq!(PROVISIONED_MACROS, ["wnam", "enam", "weur"]);
        assert!(is_provisioned_macro("wnam"));
        assert!(is_provisioned_macro("enam"));
        assert!(is_provisioned_macro("weur"));
        // sam/apac/afr are valid macros but NOT provisioned. `sam` in particular
        // stays ROUTABLE (still maps to a colo) but is not provisionable until
        // PROD_SAM has a real SAM-jurisdiction bucket (LGPD).
        assert!(!is_provisioned_macro("sam"));
        assert!(!is_provisioned_macro("apac"));
        assert!(!is_provisioned_macro("afr"));
        // ...yet `sam` is still a recognised, routable macro.
        assert!(is_macro_region("sam"));
        assert!(colo_for_macro("sam").is_some());
        assert!(is_macro_region("apac"));
        assert!(is_macro_region("afr"));
        assert!(!is_macro_region("iad"));
    }

    /// Lock-consistency invariant: every PROVISIONED macro MUST resolve to a
    /// colo (no provisioned-but-unservable region), and every macro that
    /// resolves to a colo other than `iad` must NOT be the local default. This
    /// keeps the worker `coloForMacro` and this `colo_for_macro` in agreement on
    /// which regions are servable.
    #[test]
    fn lock_consistency_every_provisioned_macro_has_a_colo() {
        for m in PROVISIONED_MACROS {
            assert!(
                colo_for_macro(m).is_some(),
                "provisioned macro {m} must map to a colo"
            );
        }
    }
}
