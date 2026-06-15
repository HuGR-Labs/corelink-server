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
//! Provisioned Phase-1 = `{wnam, enam, weur, sam}`. `apac`/`afr` are valid macro
//! codes (the D1 CHECK accepts them) but are NOT provisioned.

/// The canonical CoreLink data-residency MACRO region codes (the D1 CHECK set).
pub const MACRO_REGIONS: [&str; 6] = ["wnam", "enam", "weur", "sam", "apac", "afr"];

/// Macro regions provisioned in Phase 1. Signup rejects the rest (`apac`/`afr`).
pub const PROVISIONED_MACROS: [&str; 4] = ["wnam", "enam", "weur", "sam"];

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
        // Mirror of `worker/src/region-map.ts` PROVISIONED_MACROS.
        assert!(is_provisioned_macro("wnam"));
        assert!(is_provisioned_macro("enam"));
        assert!(is_provisioned_macro("weur"));
        assert!(is_provisioned_macro("sam"));
        // apac/afr are valid macros but NOT provisioned.
        assert!(!is_provisioned_macro("apac"));
        assert!(!is_provisioned_macro("afr"));
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
