//! Canonical breach notification event types.
//!
//! ## Typed enums (not string discriminants)
//!
//! Per Lote 10.9-quinquies NEW-P0-2 (absorbed via S-11): severity +
//! jurisdiction + locale are typed enums so compile-time taxonomy
//! enforcement catches classification errors that string-based
//! discriminants would miss at runtime.
//!
//! All public enums are `#[non_exhaustive]` to permit additive growth
//! (e.g., UK ICO or India DPDPA added post-GA via ADR-S11-009 quarterly
//! review without breaking downstream `match` sites).

use serde::{Deserialize, Serialize};

/// Canonical 3-arm breach severity taxonomy per RB-BREACH-NOTIF §2.
///
/// `#[non_exhaustive]` reserves additive growth for follow-on WIs
/// (e.g., a future S-19+ enterprise expansion may add `Sev0` for
/// nation-state attacks or mass-credential leaks > 100k).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "SCREAMING-KEBAB-CASE")]
pub enum BreachSeverity {
    /// SEV-1: > 1000 titulares, OR sensitive data, OR cross-tenant, OR
    /// crypto key material exposed. All 3 jurisdictions notified (ANPD +
    /// Irish DPC + California AG). Status page banner activated.
    Sev1,
    /// SEV-2: 100–1000 titulares, OR availability loss > 24h, OR
    /// integrity compromise (single-tenant). ANPD + Irish DPC notified;
    /// CCPA case-by-case.
    Sev2,
    /// SEV-3: < 100 titulares, OR pseudonymized data only, OR
    /// confirmed-no-PII near-miss. Internal post-mortem only.
    /// Regulatory notification at Privacy Officer judgment.
    Sev3,
}

impl BreachSeverity {
    /// Returns the canonical string representation (e.g., "SEV-1").
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sev1 => "SEV-1",
            Self::Sev2 => "SEV-2",
            Self::Sev3 => "SEV-3",
        }
    }

    /// Returns true if this severity requires ALL 3 jurisdictional
    /// notifications (ANPD + Irish DPC + California AG).
    #[must_use]
    pub fn requires_all_jurisdictions(self) -> bool {
        matches!(self, Self::Sev1)
    }

    /// Returns true if this severity triggers status page banner
    /// activation (SEV-1 only per RB-BREACH-NOTIF §7).
    #[must_use]
    pub fn activates_status_page_banner(self) -> bool {
        matches!(self, Self::Sev1)
    }

    /// Returns the canonical PagerDuty service name for this severity.
    #[must_use]
    pub fn pagerduty_service(self) -> &'static str {
        match self {
            Self::Sev1 => "corelink-breach-sev1",
            Self::Sev2 => "corelink-breach-sev2",
            Self::Sev3 => "corelink-breach-sev3",
        }
    }
}

impl core::fmt::Display for BreachSeverity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 3-arm notification jurisdiction taxonomy per
/// `rb-breach-notif-decision-tree.yaml`.
///
/// `#[non_exhaustive]` reserves additive growth for post-GA expansion
/// (UK ICO, India DPDPA, China PIPL, Canada PIPEDA — deferred per
/// ADR-S11-009).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum NotificationJurisdiction {
    /// ANPD — Autoridade Nacional de Proteção de Dados (Brazil).
    /// LGPD Art. 48 + ANPD Res. CD/ANPD nº 15/2024.
    /// Deadline: ≤ 72h calendar (conservative interpretation of "2 dias úteis").
    Anpd,
    /// Irish DPC — Data Protection Commission of Ireland (EU Lead SA).
    /// GDPR Art. 33 + EDPB Guidelines 9/2022.
    /// Deadline: ≤ 72h.
    IrishDpc,
    /// California Attorney General (US-CA).
    /// CCPA §1798.82. Deadline: "most expedient time possible" ≤ 72h.
    CaliforniaAg,
}

impl NotificationJurisdiction {
    /// Returns the canonical string name (for Prometheus labels).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Anpd => "ANPD",
            Self::IrishDpc => "Irish DPC",
            Self::CaliforniaAg => "California AG",
        }
    }

    /// Returns the canonical deadline in hours for this jurisdiction.
    /// All 3 canonical jurisdictions have a 72h deadline.
    #[must_use]
    pub fn deadline_hours(self) -> u32 {
        72
    }
}

impl core::fmt::Display for NotificationJurisdiction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 3-arm customer notification locale taxonomy.
///
/// All 3 locales are **mandatory** for SEV-1/2 customer notifications
/// per sprint contract §10.s11.7 + LGPD Art. 9 (direito à informação
/// acessível). NEVER send single locale.
///
/// `#[non_exhaustive]` allows adding future locales (e.g., `Fr` or
/// `De`) as enterprise customer geography expands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum CustomerLocale {
    /// Portuguese (Brazil). Template: `customer-breach-notification.pt-BR.mjml`.
    PtBr,
    /// English (United States). Template: `customer-breach-notification.en-US.mjml`.
    EnUs,
    /// Spanish (Mexico). Template: `customer-breach-notification.es-MX.mjml`.
    EsMx,
}

impl CustomerLocale {
    /// Returns the canonical BCP 47 locale tag.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PtBr => "pt-BR",
            Self::EnUs => "en-US",
            Self::EsMx => "es-MX",
        }
    }
}

impl core::fmt::Display for CustomerLocale {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical count of severity arms.
pub const SEVERITY_COUNT: usize = 3;

/// Canonical count of jurisdiction arms.
pub const JURISDICTION_COUNT: usize = 3;

/// Canonical mandatory customer locales (3 locales per sprint contract
/// §10.s11.7). NEVER reduce to 1-locale dispatch.
pub const CUSTOMER_LOCALES_MANDATORY: [CustomerLocale; 3] = [
    CustomerLocale::PtBr,
    CustomerLocale::EnUs,
    CustomerLocale::EsMx,
];

/// Canonical audit payload for `dev.hugr.corelink.breach.notification_dispatched.v1`.
///
/// Emitted after EACH regulatory or customer notification dispatch
/// (RB-BREACH-NOTIF §4.1). R2 audit-`<region>` Object Lock 7y.
///
/// `breach_id` is the idempotency anchor — same `breach_id` + `retry_attempt++`
/// on re-emit post audit-failure recovery.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreachNotificationDispatch {
    /// ULID assigned at incident declaration — idempotency anchor.
    pub breach_id: String,
    /// Breach severity classification.
    pub severity: BreachSeverity,
    /// Regulatory authorities notified. Empty for SEV-3 (internal only).
    pub jurisdictions_notified: Vec<NotificationJurisdiction>,
    /// Whether customer notifications were dispatched.
    pub customer_notifications_sent: bool,
    /// Customer notification locales dispatched (mandatory 3 for SEV-1/2).
    pub customer_locales: Vec<CustomerLocale>,
    /// ISO 8601 UTC dispatch timestamp (Unix epoch ms).
    pub ts_ms: u64,
    /// Idempotent retry counter. 0 = first emission.
    pub retry_attempt: u32,
    /// Unix epoch ms of `breach_detected_at` — anchors 72h regulatory
    /// SLA clock. Zero if unknown at emit time.
    pub breach_detected_at_ms: u64,
}

impl BreachNotificationDispatch {
    /// Returns the elapsed seconds from `breach_detected_at_ms` to
    /// `ts_ms` (dispatch time). Returns `None` if `breach_detected_at_ms`
    /// is zero (unknown) or if detected_at is after ts (clock skew).
    #[must_use]
    pub fn elapsed_seconds_to_dispatch(&self) -> Option<u64> {
        if self.breach_detected_at_ms == 0 {
            return None;
        }
        self.ts_ms
            .checked_sub(self.breach_detected_at_ms)
            .map(|ms| ms / 1_000)
    }

    /// Returns true if the notification was dispatched within the 72h
    /// regulatory SLA window.
    ///
    /// Returns `None` if `breach_detected_at_ms` is unknown (zero).
    #[must_use]
    pub fn within_72h_sla(&self) -> Option<bool> {
        self.elapsed_seconds_to_dispatch().map(|s| s <= 72 * 3600)
    }
}

/// One step in a PagerDuty escalation policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscalationStep {
    /// Role being paged (e.g., "Privacy Officer").
    pub role: &'static str,
    /// Timeout offset in minutes from incident declaration.
    pub timeout_minutes: u32,
}

/// Canonical escalation policy for a breach severity tier.
///
/// Maps SEV → ordered list of escalation steps (who + timeout).
/// Returned by [`escalation_policy_for`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EscalationPolicy {
    /// Severity this policy applies to.
    pub severity: BreachSeverity,
    /// PagerDuty service name.
    pub pagerduty_service: &'static str,
    /// Ordered escalation steps.
    pub steps: &'static [EscalationStep],
}

/// Canonical escalation steps for SEV-1.
///
/// Privacy Officer (15m) → Legal (30m) → Security Lead (45m) → CEO interim (60m).
/// Per RB-BREACH-NOTIF §8 + AC-006 property test.
pub static SEV1_ESCALATION_STEPS: &[EscalationStep] = &[
    EscalationStep {
        role: "Privacy Officer",
        timeout_minutes: 15,
    },
    EscalationStep {
        role: "Legal",
        timeout_minutes: 30,
    },
    EscalationStep {
        role: "Security Lead",
        timeout_minutes: 45,
    },
    EscalationStep {
        role: "CEO interim",
        timeout_minutes: 60,
    },
];

/// Canonical escalation steps for SEV-2.
///
/// Privacy Officer (15m) → Legal (30m) → Security Lead (45m).
pub static SEV2_ESCALATION_STEPS: &[EscalationStep] = &[
    EscalationStep {
        role: "Privacy Officer",
        timeout_minutes: 15,
    },
    EscalationStep {
        role: "Legal",
        timeout_minutes: 30,
    },
    EscalationStep {
        role: "Security Lead",
        timeout_minutes: 45,
    },
];

/// Canonical escalation steps for SEV-3.
///
/// Privacy Officer only (60m).
pub static SEV3_ESCALATION_STEPS: &[EscalationStep] = &[EscalationStep {
    role: "Privacy Officer",
    timeout_minutes: 60,
}];

/// Returns the canonical [`EscalationPolicy`] for the given severity.
///
/// Deterministic fn — pinned by `prop_breach_emit_escalation_matrix`
/// property test at 100k iterations (AC-006).
#[must_use]
pub fn escalation_policy_for(severity: BreachSeverity) -> EscalationPolicy {
    match severity {
        BreachSeverity::Sev1 => EscalationPolicy {
            severity,
            pagerduty_service: "corelink-breach-sev1",
            steps: SEV1_ESCALATION_STEPS,
        },
        BreachSeverity::Sev2 => EscalationPolicy {
            severity,
            pagerduty_service: "corelink-breach-sev2",
            steps: SEV2_ESCALATION_STEPS,
        },
        BreachSeverity::Sev3 => EscalationPolicy {
            severity,
            pagerduty_service: "corelink-breach-sev3",
            steps: SEV3_ESCALATION_STEPS,
        },
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn severity_str_representations_unique() {
        let v = [
            BreachSeverity::Sev1,
            BreachSeverity::Sev2,
            BreachSeverity::Sev3,
        ];
        let mut seen = std::collections::HashSet::new();
        for s in v {
            assert!(seen.insert(s.as_str()), "duplicate severity str: {s}");
        }
        assert_eq!(seen.len(), SEVERITY_COUNT);
    }

    #[test]
    fn jurisdiction_str_representations_unique() {
        let v = [
            NotificationJurisdiction::Anpd,
            NotificationJurisdiction::IrishDpc,
            NotificationJurisdiction::CaliforniaAg,
        ];
        let mut seen = std::collections::HashSet::new();
        for j in v {
            assert!(seen.insert(j.as_str()), "duplicate jurisdiction str: {j}");
        }
        assert_eq!(seen.len(), JURISDICTION_COUNT);
    }

    #[test]
    fn all_jurisdictions_have_72h_deadline() {
        let v = [
            NotificationJurisdiction::Anpd,
            NotificationJurisdiction::IrishDpc,
            NotificationJurisdiction::CaliforniaAg,
        ];
        for j in v {
            assert_eq!(j.deadline_hours(), 72, "jurisdiction {j} deadline != 72h");
        }
    }

    #[test]
    fn mandatory_locales_count_is_3() {
        assert_eq!(CUSTOMER_LOCALES_MANDATORY.len(), 3);
        let locales: std::collections::HashSet<_> = CUSTOMER_LOCALES_MANDATORY
            .iter()
            .map(|l| l.as_str())
            .collect();
        assert!(locales.contains("pt-BR"));
        assert!(locales.contains("en-US"));
        assert!(locales.contains("es-MX"));
    }

    #[test]
    fn sev1_requires_all_jurisdictions() {
        assert!(BreachSeverity::Sev1.requires_all_jurisdictions());
        assert!(!BreachSeverity::Sev2.requires_all_jurisdictions());
        assert!(!BreachSeverity::Sev3.requires_all_jurisdictions());
    }

    #[test]
    fn sev1_activates_status_page_banner() {
        assert!(BreachSeverity::Sev1.activates_status_page_banner());
        assert!(!BreachSeverity::Sev2.activates_status_page_banner());
        assert!(!BreachSeverity::Sev3.activates_status_page_banner());
    }

    #[test]
    fn pagerduty_services_are_distinct_per_severity() {
        let services = [
            BreachSeverity::Sev1.pagerduty_service(),
            BreachSeverity::Sev2.pagerduty_service(),
            BreachSeverity::Sev3.pagerduty_service(),
        ];
        let unique: std::collections::HashSet<_> = services.iter().collect();
        assert_eq!(
            unique.len(),
            3,
            "PagerDuty services must be distinct per severity"
        );
    }

    #[test]
    fn sev1_escalation_policy_has_4_steps_including_ceo() {
        let policy = escalation_policy_for(BreachSeverity::Sev1);
        assert_eq!(policy.steps.len(), 4, "SEV-1 must have 4 escalation steps");
        let has_ceo = policy.steps.iter().any(|s| s.role.contains("CEO"));
        assert!(has_ceo, "SEV-1 escalation must include CEO interim");
    }

    #[test]
    fn sev2_escalation_policy_has_3_steps_no_ceo() {
        let policy = escalation_policy_for(BreachSeverity::Sev2);
        assert_eq!(policy.steps.len(), 3, "SEV-2 must have 3 escalation steps");
        let has_ceo = policy.steps.iter().any(|s| s.role.contains("CEO"));
        assert!(!has_ceo, "SEV-2 escalation must NOT include CEO");
    }

    #[test]
    fn sev3_escalation_policy_has_1_step_privacy_officer_only() {
        let policy = escalation_policy_for(BreachSeverity::Sev3);
        assert_eq!(policy.steps.len(), 1, "SEV-3 must have 1 escalation step");
        assert!(policy.steps[0].role.contains("Privacy Officer"));
    }

    #[test]
    fn escalation_steps_monotonically_increasing_timeouts() {
        for sev in [
            BreachSeverity::Sev1,
            BreachSeverity::Sev2,
            BreachSeverity::Sev3,
        ] {
            let policy = escalation_policy_for(sev);
            let timeouts: Vec<_> = policy.steps.iter().map(|s| s.timeout_minutes).collect();
            for w in timeouts.windows(2) {
                assert!(
                    w[0] < w[1],
                    "escalation timeouts must be strictly increasing for {sev}"
                );
            }
        }
    }

    #[test]
    fn dispatch_within_72h_sla() {
        let dispatch = BreachNotificationDispatch {
            breach_id: "01JTVHXX-TEST".to_string(),
            severity: BreachSeverity::Sev1,
            jurisdictions_notified: vec![
                NotificationJurisdiction::Anpd,
                NotificationJurisdiction::IrishDpc,
                NotificationJurisdiction::CaliforniaAg,
            ],
            customer_notifications_sent: true,
            customer_locales: vec![
                CustomerLocale::PtBr,
                CustomerLocale::EnUs,
                CustomerLocale::EsMx,
            ],
            ts_ms: 1_000 + 60 * 3600 * 1_000, // 60h after detected_at (within 72h)
            retry_attempt: 0,
            breach_detected_at_ms: 1_000,
        };
        let elapsed = dispatch.elapsed_seconds_to_dispatch().unwrap();
        assert_eq!(elapsed, 60 * 3600);
        let within = dispatch.within_72h_sla().unwrap();
        assert!(within, "60h should be within 72h SLA");
    }

    #[test]
    fn dispatch_outside_72h_sla_detected() {
        let dispatch = BreachNotificationDispatch {
            breach_id: "01JTVHXX-TEST2".to_string(),
            severity: BreachSeverity::Sev2,
            jurisdictions_notified: vec![NotificationJurisdiction::Anpd],
            customer_notifications_sent: true,
            customer_locales: vec![
                CustomerLocale::PtBr,
                CustomerLocale::EnUs,
                CustomerLocale::EsMx,
            ],
            ts_ms: 1_000 + 80 * 3600 * 1_000, // 80h — OVER 72h
            retry_attempt: 0,
            breach_detected_at_ms: 1_000,
        };
        let within = dispatch.within_72h_sla().unwrap();
        assert!(!within, "80h should be outside 72h SLA");
    }

    #[test]
    fn dispatch_unknown_detected_at_returns_none_for_sla() {
        let dispatch = BreachNotificationDispatch {
            breach_id: "01JTVHXX-TEST3".to_string(),
            severity: BreachSeverity::Sev1,
            jurisdictions_notified: vec![],
            customer_notifications_sent: false,
            customer_locales: vec![],
            ts_ms: 100_000,
            retry_attempt: 0,
            breach_detected_at_ms: 0, // unknown
        };
        assert!(dispatch.elapsed_seconds_to_dispatch().is_none());
        assert!(dispatch.within_72h_sla().is_none());
    }
}
